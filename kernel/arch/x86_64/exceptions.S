/* x86_64 exception stubs and common dispatcher.
 *
 * The CPU on long-mode exception entry pushes (high to low):
 *   SS, RSP, RFLAGS, CS, RIP, [error_code]
 * Some vectors push an error code (8, 10, 11, 12, 13, 14, 17, 21);
 * the rest do not.
 *
 * The per-vector stub (isr_<vec>) pushes a synthetic zero error
 * code if the CPU did not, then pushes the vector number. After
 * that the layout is uniform and we jump to the common dispatcher,
 * which saves all GPRs into a struct regs on the stack and calls
 * the C exception_handler.
 */

	.text

	.altmacro

	.macro define_isr vec
	.global isr_\vec
	.type   isr_\vec, @function
isr_\vec:
	.if \vec == 8 || \vec == 10 || \vec == 11 || \vec == 12 || \vec == 13 || \vec == 14 || \vec == 17 || \vec == 21
	/* CPU pushed error code; nothing to do here. */
	.else
	pushq   $0
	.endif
	pushq   $\vec
	jmp     isr_common
	.size   isr_\vec, . - isr_\vec
	.endm

	.set    vec, 0
	.rept   256
	define_isr %vec
	.set    vec, vec + 1
	.endr

isr_common:
	/* Save GPRs so the on-stack layout matches struct regs in
	 * idt.h: r15 at lowest addr, then r14, r13, ..., rax, then
	 * the (vector, error_code) pushed by the stub, then the
	 * CPU-pushed (rip, cs, rflags, rsp, ss) at the top. */
	push    %rax
	push    %rbx
	push    %rcx
	push    %rdx
	push    %rsi
	push    %rdi
	push    %rbp
	push    %r8
	push    %r9
	push    %r10
	push    %r11
	push    %r12
	push    %r13
	push    %r14
	push    %r15

	mov     %rsp, %rdi          /* arg 1: struct regs * (= current RSP) */
	and     $-16, %rsp          /* align RSP to 16 for the SysV ABI call */
	call    exception_handler
	/* exception_handler is _Noreturn; the loop below is paranoia
	 * in case a future revision lets it return. */
1:
	cli
	hlt
	jmp     1b

	.section .rodata
	.global isr_table

	.altmacro
	.macro emit_isr_addr i
	.quad   isr_\i
	.endm

isr_table:
	.set    i, 0
	.rept   256
	emit_isr_addr %i
	.set    i, i + 1
	.endr

	.section .note.GNU-stack,"",@progbits
