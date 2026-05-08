/*
 * kernel/lib/setjmp.S
 *
 * Freestanding setjmp / longjmp for x86_64 SysV. See lib/setjmp.h
 * for the saved-register policy and the rationale for skipping
 * RFLAGS / MXCSR / FCW under M3's IF=0 invariant.
 *
 * jmp_buf layout (8 unsigned long, 64 bytes):
 *   [0]  rbx
 *   [8]  rbp
 *   [16] r12
 *   [24] r13
 *   [32] r14
 *   [40] r15
 *   [48] rsp     (caller's, after our call instruction's push)
 *   [56] rip     (caller's return address)
 */

	.text

/* int setjmp(jmp_buf env)
 *   rdi = env. Returns 0 on direct call. On a longjmp(env, val) the
 *   call site sees the longjmp's val (or 1 if val == 0). */
	.globl setjmp
	.type setjmp, @function
setjmp:
	movq    %rbx, 0(%rdi)
	movq    %rbp, 8(%rdi)
	movq    %r12, 16(%rdi)
	movq    %r13, 24(%rdi)
	movq    %r14, 32(%rdi)
	movq    %r15, 40(%rdi)
	leaq    8(%rsp), %rax        /* caller rsp = our rsp + 8 (return addr) */
	movq    %rax, 48(%rdi)
	movq    (%rsp), %rax         /* return address sits on top of stack */
	movq    %rax, 56(%rdi)
	xorl    %eax, %eax
	ret
	.size setjmp, . - setjmp

/* _Noreturn void longjmp(jmp_buf env, int val)
 *   rdi = env, esi = val. Restores callee-preserved registers, rsp,
 *   and rip; control resumes at the matching setjmp's call site with
 *   eax == val (or 1 if val == 0, per C11). */
	.globl longjmp
	.type longjmp, @function
longjmp:
	movq    0(%rdi), %rbx
	movq    8(%rdi), %rbp
	movq    16(%rdi), %r12
	movq    24(%rdi), %r13
	movq    32(%rdi), %r14
	movq    40(%rdi), %r15
	movq    48(%rdi), %rsp
	movl    %esi, %eax
	testl   %eax, %eax
	jnz     1f
	movl    $1, %eax
1:
	jmp     *56(%rdi)
	.size longjmp, . - longjmp

/* Mark the section as non-executable-stack-aware. Linux convention;
 * harmless on freestanding x86_64 ELF, keeps lld / GNU ld from
 * inferring an executable stack for the kernel image. */
	.section .note.GNU-stack, "", @progbits
