#ifndef FIELDOS_ARCH_X86_64_GDT_H
#define FIELDOS_ARCH_X86_64_GDT_H

#include <stdint.h>

/* GDT selector constants. A selector is the byte offset of the
 * descriptor in the GDT, OR'd with the requested privilege level
 * (RPL=3 for user-mode selectors). The CPU enforces RPL during
 * far transfers; it is irrelevant for kernel-only selectors. */
#define GDT_KERNEL_CODE  0x08
#define GDT_KERNEL_DATA  0x10
#define GDT_USER_CODE    (0x18 | 3)
#define GDT_USER_DATA    (0x20 | 3)
#define GDT_TSS          0x28

void gdt_init(void);

/* Set tss.ist[index-1] (where index is 1..7). idt_init uses this
 * to wire 4 KiB exception stacks into the IST entries that
 * #NMI/#DF/#MC will switch to. Indexes outside 1..7 are silently
 * ignored. */
void gdt_set_ist(int index, uint64_t stack_top);

#endif
