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
 * on any negative return. */
#define AS_OK            0
#define AS_E_TODO       -1   /* not yet implemented (v0 stub) */
#define AS_E_UNKNOWN    -2   /* line did not match any encoder form */
#define AS_E_MALFORMED  -3   /* operand parse failed */
#define AS_E_NOSPACE    -4   /* out_cap too small for the encoding */

/* Encode one AT&T-syntax x86_64 line (instruction OR directive OR
 * label OR comment OR blank) into `out` (capacity `out_cap`). On
 * success the function returns AS_OK and *out_len receives the number
 * of bytes written; non-emitting lines (labels, comments, .text/
 * .globl/.align directives, blank lines) succeed with *out_len == 0.
 *
 * `att_line` is NOT required to be NUL-terminated; `line_len` bounds
 * the read. The line must not contain a trailing newline. */
int asm_encode(const char *att_line, size_t line_len,
               uint8_t *out, size_t out_cap, size_t *out_len);

#endif /* FIELDOS_HOLYC_ASM_H */
