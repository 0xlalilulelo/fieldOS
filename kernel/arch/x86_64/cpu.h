#ifndef FIELDOS_ARCH_X86_64_CPU_H
#define FIELDOS_ARCH_X86_64_CPU_H

/* CPU-level setup that runs once at boot, before any code path that
 * needs the corresponding CR0/CR4 state. Today this is just SSE
 * activation; future arrivals (XSAVE, NX in CR4 paging-control bits,
 * SMEP/SMAP) land here too. */

/* Enable SSE / SSE2 instruction execution.
 *
 * Intel SDM Vol. 1 §13.1: an OS that uses fxsave/fxrstor for context
 * save/restore must set CR4.OSFXSR (bit 9). An OS that has an #XF
 * (vector 19) handler installed must set CR4.OSXMMEXCPT (bit 10).
 * CR0.EM (bit 2) cleared and CR0.MP (bit 1) set tells the CPU the
 * x87 FPU is present and not emulated.
 *
 * Without OSFXSR, any SSE instruction (movdqa, movups, etc.) raises
 * #UD. The vendored holyc-lang subset is built with SSE enabled
 * (aostr.c reads variadic doubles via xmm); GCC also emits SSE for
 * 16-byte struct copies in -O2. cpu_enable_sse() must run before
 * the first call into the subset.
 *
 * Field OS does not yet have an XSAVE-aware context switch (Patrol
 * scheduler is M4); M3 boots with IF=0 throughout, so no interrupt
 * can fire and corrupt xmm state mid-instruction. ADR-0002 documents
 * the M4 obligation: extend kernel/arch/x86_64/exceptions.S with
 * fxsave/fxrstor at the IDT entry path before the first `sti`. */
void cpu_enable_sse(void);

#endif
