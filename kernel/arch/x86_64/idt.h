#ifndef FIELDOS_ARCH_X86_64_IDT_H
#define FIELDOS_ARCH_X86_64_IDT_H

#include <stdint.h>

/* CPU register snapshot delivered to exception_handler. The layout
 * is built up by the per-vector stubs in exceptions.S and the
 * common dispatcher: GPRs in reverse-push order at the bottom,
 * then (vector, error_code) pushed by the stub, then the
 * CPU-supplied (rip, cs, rflags, rsp, ss). Total 176 bytes. */
struct regs {
	uint64_t r15, r14, r13, r12, r11, r10, r9, r8;
	uint64_t rbp, rdi, rsi, rdx, rcx, rbx, rax;
	uint64_t vector, error_code;
	uint64_t rip, cs, rflags, rsp, ss;
};

void idt_init(void);

/* Called from isr_common after GPRs have been pushed. Does not
 * return; current implementation prints a one-line panic on serial
 * and halts. */
__attribute__((noreturn)) void exception_handler(struct regs *r);

#endif
