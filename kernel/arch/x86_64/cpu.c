#include <stdint.h>

#include "cpu.h"

void cpu_enable_sse(void)
{
	uint64_t cr0;
	__asm__ volatile ("mov %%cr0, %0" : "=r" (cr0));
	cr0 &= ~(1UL << 2);   /* clear CR0.EM (no x87 emulation) */
	cr0 |=  (1UL << 1);   /* set   CR0.MP (monitor coprocessor) */
	__asm__ volatile ("mov %0, %%cr0" :: "r" (cr0));

	uint64_t cr4;
	__asm__ volatile ("mov %%cr4, %0" : "=r" (cr4));
	cr4 |= (1UL << 9);    /* set CR4.OSFXSR     (fxsave/fxrstor available) */
	cr4 |= (1UL << 10);   /* set CR4.OSXMMEXCPT (#XF handler installed) */
	__asm__ volatile ("mov %0, %%cr4" :: "r" (cr4));
}
