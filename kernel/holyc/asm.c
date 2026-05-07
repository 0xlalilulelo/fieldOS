/* kernel/holyc/asm.c
 *
 * In-tree x86_64 encoder. v0 stub: every line returns AS_E_TODO.
 * (C) replaces this with the table-driven encoder skeleton plus the
 * prologue/epilogue cluster (push, pop, ret, mov reg-reg, lea, basic
 * ALU); (D) extends through the remaining clusters until the
 * Bug_171.HC corpus encodes byte-for-byte against $(CROSS_AS).
 *
 * Compiles under both the host build (via the asm-test harness rule
 * in holyc/holyc.mk) and the kernel build (via KERNEL_C_SRCS in
 * kernel/kernel.mk). The two compiles share the same source so
 * libc-isms in this file would surface immediately under -mno-sse
 * -ffreestanding rather than at step 5 integration time. */

#include "asm.h"

int asm_encode(const char *att_line, size_t line_len,
               uint8_t *out, size_t out_cap, size_t *out_len) {
    (void)att_line;
    (void)line_len;
    (void)out;
    (void)out_cap;
    if (out_len) *out_len = 0;
    return AS_E_TODO;
}
