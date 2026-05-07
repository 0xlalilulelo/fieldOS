#ifndef FIELDOS_HOLYC_ASM_H
#define FIELDOS_HOLYC_ASM_H

/* The in-tree x86_64 encoder. Per ADR-0001 §2 (chosen path: keep
 * holyc/src/x86.c's AT&T-text output, write a small table-driven
 * encoder here) and ADR-0003 §1 (the AoStr that compileToAsm()
 * returns is the encoder's input — x86.c is untouched).
 *
 * The harness at kernel/holyc/asm_test.c (host-only, built via
 * holyc/holyc.mk's `make asm-test` target) drives this entry point
 * against the checked-in corpus under holyc/tests/corpus/ and, once
 * (C)'s first cluster lands, compares each instruction line's bytes
 * to $(CROSS_AS) output.
 *
 * The v0 stub in asm.c returns AS_E_TODO for every line; (C) and
 * (D) replace it with the real table-driven encoder cluster by
 * cluster. The interface is stable from B onward; the harness does
 * not change between B and the end of D. */

#include <stddef.h>
#include <stdint.h>

/* Return convention for asm_encode(). 0 means "encoded; *out_len
 * bytes written". Negatives are failure modes; *out_len is undefined
 * on any negative return. AS_E_NOSPACE covers both the byte buffer
 * and the relocation buffer overflowing. */
#define AS_OK            0
#define AS_E_TODO       -1   /* not yet implemented (v0 stub) */
#define AS_E_UNKNOWN    -2   /* line did not match any encoder form */
#define AS_E_MALFORMED  -3   /* operand parse failed */
#define AS_E_NOSPACE    -4   /* out_cap or reloc_cap too small */

/* Relocation record. Three sites in the encoder leave zero-filled
 * rel32 placeholders that the linker (5-2's pipeline driver in
 * eval.c) must patch:
 *
 *   call sym           E8 + rel32                  (encCall)
 *   jcc  sym           0F 8x + rel32               (encJe)
 *   leaq sym(%rip), %r REX.W 8D ModR/M(00,r,5) + rel32   (emitMem)
 *   movq sym(%rip), %r REX.W 8B ModR/M(00,r,5) + rel32   (emitMem)
 *
 * All three patch the same shape: a four-byte signed displacement
 * stored at `offset` such that, post-patch,
 *
 *   *(int32_t *)(buf + offset) = (int32_t)(target_va - (next_va))
 *   where next_va = base_va + offset + 4
 *
 * No kind discriminator today because the patching formula is
 * identical for the three sites; if a future encoder addition
 * (rel8, movabs imm64) breaks that, a `kind` field lands then.
 *
 * `sym` points into the input AT&T line passed to asm_encode and is
 * NOT NUL-terminated; `sym_len` bounds the read. The pointer's
 * lifetime is the caller's — Reloc records do not own the symbol
 * name; the caller arranges for the input line to outlive the
 * relocation walk, or copies the names. */
typedef struct {
    size_t       offset;    /* position of rel32 inside `out` */
    const char  *sym;       /* into the input line; not NUL-terminated */
    size_t       sym_len;
} Reloc;

/* Encode one AT&T-syntax x86_64 line (instruction OR directive OR
 * label OR comment OR blank) into `out` (capacity `out_cap`). On
 * success the function returns AS_OK and *out_len receives the number
 * of bytes written; non-emitting lines (labels, comments, .text/
 * .globl/.align directives, blank lines) succeed with *out_len == 0.
 *
 * `att_line` is NOT required to be NUL-terminated; `line_len` bounds
 * the read. The line must not contain a trailing newline.
 *
 * Relocations: callers that want the encoder to surface placeholder
 * sites pass a `relocs` buffer of `reloc_cap` entries and a non-NULL
 * `reloc_count`; on AS_OK, *reloc_count is the number of entries
 * written. Callers that don't care pass `relocs = NULL`, `reloc_cap
 * = 0`, `reloc_count = NULL` and the encoder still emits the
 * zero-filled placeholders into `out` (matching GAS pre-link bytes).
 * Overflowing `reloc_cap` returns AS_E_NOSPACE. */
int asm_encode(const char *att_line, size_t line_len,
               uint8_t *out, size_t out_cap, size_t *out_len,
               Reloc *relocs, size_t reloc_cap, size_t *reloc_count);

#endif /* FIELDOS_HOLYC_ASM_H */
