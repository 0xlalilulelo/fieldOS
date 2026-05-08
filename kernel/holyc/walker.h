#ifndef FIELDOS_HOLYC_WALKER_H
#define FIELDOS_HOLYC_WALKER_H

/* Pass-1 line walker over an AT&T-text AoStr. The pipeline driver in
 * kernel/holyc/eval.c (5-2 step) calls this on the buffer that
 * compileToAsm() returns; the host harness in kernel/holyc/asm_test.c
 * runs the same routine over the checked-in corpus under
 * holyc/tests/corpus/ for label-set verification (5-2b's witness).
 *
 * Pass 1 yields each line's emitted byte count via asm_encode (with
 * NULL output buffers so nothing is written) and accumulates a running
 * offset. Lines ending ':' (after WS / trailing-comment trim) get
 * recorded as labels, paired with the offset at which they sit in the
 * eventual emitted buffer. 5-2c re-walks the same buffer in pass 2 to
 * fill the JIT region and patch local rel32s; 5-1's relocation API
 * surfaces extern symbols the label table does not match. */

#include <stddef.h>
#include <stdint.h>

typedef struct {
	const char *name;     /* into the input buffer; NOT NUL-terminated */
	size_t      name_len;
	size_t      offset;   /* byte offset in the eventual emitted buffer */
} HolycLabel;

#define HOLYC_LABEL_MAX 128

typedef struct {
	HolycLabel entries[HOLYC_LABEL_MAX];
	size_t     count;
	int        overflow;  /* nonzero if more than HOLYC_LABEL_MAX seen */
} HolycLabelTable;

/* Extern relocation, deferred until 5-3 resolves against the ABI table.
 * Symbol name is COPIED (NUL-terminated) — Reloc.sym from asm_encode
 * aliases into the per-line input that the line iterator overwrites
 * on the next iteration; extern entries must outlive that scope. */
#define HOLYC_EXTERN_SYM_MAX 64
#define HOLYC_EXTERN_MAX     128

typedef struct {
	char   sym[HOLYC_EXTERN_SYM_MAX];
	size_t sym_len;
	size_t buf_offset;     /* absolute offset of the rel32 in `out` */
} HolycExternReloc;

typedef struct {
	HolycExternReloc entries[HOLYC_EXTERN_MAX];
	size_t           count;
	int              overflow;
} HolycExternTable;

/* Walk `data[0..len)` line by line. On success returns 0 with *labels
 * populated and *total_bytes set to the cumulative emitted byte count.
 * Returns the asm_encode failure rc (negative) on any real encoder
 * error; AS_E_UNKNOWN is tolerated (coverage gap, not regression) and
 * contributes 0 bytes to the offset.
 *
 * Either out param may be NULL — passing labels=NULL skips label
 * recording, total_bytes=NULL discards the count. */
int holyc_walker_pass1(const char *data, size_t len,
                       HolycLabelTable *labels,
                       size_t *total_bytes);

/* ABI lookup function pointer. Pass-3 takes a function pointer rather
 * than calling abi_table_lookup directly so walker.c stays host-buildable
 * inside the asm-test harness without dragging abi_table.c (and its
 * serial / format kernel-only deps) into the host link. eval.c passes
 * &abi_table_lookup; asm_test.c (5-3e) passes a synthetic mock that
 * maps _printf to a fixed virtual address. Returns 0 to signal not-
 * found per the abi_table_lookup contract. */
typedef uint64_t (*HolycAbiLookup)(const char *name, size_t name_len);

/* Pass 3: extern symbol resolution.
 *
 * Walks `externs` (built by pass-2), calls `lookup(sym, sym_len)` for
 * each entry, and patches the rel32 at `out + entry.buf_offset` against
 * the resolved virtual address. The patch math is absolute-VA aware:
 *
 *   patch_va = base_va + entry.buf_offset
 *   *(int32_t *)(out + entry.buf_offset) =
 *       (int32_t)(target_va - (patch_va + 4))
 *
 * `base_va` is the absolute VA the `out` buffer will execute from
 * (typically (uint64_t)(uintptr_t)out itself, since the JIT region
 * is identity-mapped from the kernel's view). pass-3 runs before
 * holyc_jit_commit to keep the buffer writable irrespective of any
 * future strict-W^X ADR.
 *
 * Returns 0 on success with *resolved_count and *unresolved_count
 * populated; returns AS_E_NOSPACE / negative if a buf_offset would
 * spill past out_len, or if a resolved displacement does not fit in
 * a signed 32-bit (rel32 reach is +/- 2 GiB; the M3 kernel and JIT
 * region are within reach but a future allocator change might not be).
 *
 * Entries whose lookup returns 0 are not patched and contribute to
 * *unresolved_count. eval.c's 5-3d policy treats unresolved > 0 as
 * a hard error; pass-3 itself reports both counts and returns 0 so
 * the policy can be applied at the call site. */
int holyc_walker_pass3(const HolycExternTable *externs,
                       unsigned char *out, size_t out_len,
                       uint64_t base_va,
                       HolycAbiLookup lookup,
                       size_t *resolved_count,
                       size_t *unresolved_count);

/* Pass 2: re-walk `data[0..len)`, this time emitting bytes into
 * `out[0..out_cap)` and using `labels` (built by pass 1) to patch
 * local-label rel32 relocations in place. Each line's bytes land at
 * the offset pass 1 implicitly recorded (recomputed here as the
 * cumulative sum of line byte counts). Relocations whose symbol
 * matches a label entry get the patch
 *
 *   *(int32_t *)(out + abs_off) = (int32_t)(target - (abs_off + 4))
 *
 * applied immediately; relocations whose symbol does not match append
 * to *externs for 5-3 to resolve against the ABI table. The four
 * placeholder bytes at extern reloc sites stay zero (asm_encode's
 * pre-link representation), matching GAS's behaviour for undefined
 * references. *out_len receives the cumulative byte count on success;
 * *local_patched_count receives the number of relocations patched in
 * place. */
int holyc_walker_pass2(const char *data, size_t len,
                       const HolycLabelTable *labels,
                       unsigned char *out, size_t out_cap,
                       size_t *out_len,
                       HolycExternTable *externs,
                       size_t *local_patched_count);

#endif
