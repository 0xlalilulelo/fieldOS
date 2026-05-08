#ifndef FIELDOS_LIB_SETJMP_H
#define FIELDOS_LIB_SETJMP_H

/* Freestanding setjmp / longjmp for x86_64 SysV.
 *
 * Saves: rbx, rbp, r12, r13, r14, r15, rsp, rip — the SysV
 * callee-preserved register set plus the stack and instruction
 * pointers. Volatile registers (rax, rcx, rdx, rsi, rdi, r8-r11)
 * are not saved because longjmp is itself a function call: the
 * setjmp call site's compiler-generated reload of any live
 * volatiles applies on the longjmp side too.
 *
 * Does NOT save RFLAGS. ADR-0002 §1: the kernel runs with IF=0
 * from Limine handoff through M3 and never executes sti, so
 * RFLAGS.IF is invariant across the longjmp window. Restoring it
 * would be defensive against a future M4 sti that has not yet
 * landed.
 *
 * Does NOT save MXCSR or x87 FCW. The M3 longjmp window only
 * crosses parse-error paths (runtime.c::exit -> holyc_eval) which
 * do not touch FP state in our emitted code. ADR-0002 §2 commits
 * to fxsave / fxrstor in the IDT entry path, not in setjmp; if a
 * future eval brings an FP-side exit path, this header gains
 * MXCSR + FCW save / restore.
 *
 * jmp_buf layout: 8 unsigned long (64 bytes), 8-byte aligned. The
 * order is fixed by setjmp.S; do not reorder fields casually. */
typedef unsigned long jmp_buf[8];

int setjmp(jmp_buf env);
_Noreturn void longjmp(jmp_buf env, int val);

#endif
