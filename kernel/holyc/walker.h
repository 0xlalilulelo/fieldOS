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
