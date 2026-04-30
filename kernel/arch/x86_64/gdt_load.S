/* gdt_load_and_reload_segments(struct gdtr *r)
 *
 * Loads the GDT pointed to by r (rdi per the SysV AMD64 ABI),
 * then reloads every segment register so the new GDT takes
 * effect. CS cannot be moved into directly; we use a far return
 * (lretq) to install the new code selector.
 *
 * This is the only assembly file in M1-A. M1-B introduces the
 * exception stub table in a separate .S file. */

	.text
	.global gdt_load_and_reload_segments
	.type   gdt_load_and_reload_segments,@function
gdt_load_and_reload_segments:
	lgdt    (%rdi)

	/* Reload data selectors with the kernel data selector (0x10). */
	mov     $0x10, %ax
	mov     %ax, %ds
	mov     %ax, %es
	mov     %ax, %fs
	mov     %ax, %gs
	mov     %ax, %ss

	/* Reload CS via far return.
	 *
	 * Stack on entry: [retaddr]
	 * After pop:      (retaddr now in %rdi; stack is empty)
	 * After two pushes (low->high): [retaddr, 0x08]
	 * lretq pops RIP=retaddr from RSP+0 and CS=0x08 from RSP+8. */
	pop     %rdi
	pushq   $0x08
	push    %rdi
	lretq

	.size   gdt_load_and_reload_segments, . - gdt_load_and_reload_segments
	.section .note.GNU-stack,"",@progbits
