/* Field OS x86_64 kernel link script.
 *
 * Layout follows Limine protocol expectations: kernel mapped at
 * canonical higher half (0xffffffff80000000) with each section
 * page-aligned and grouped into a distinct PT_LOAD segment so the
 * bootloader can apply the right page-table flags.
 */

OUTPUT_FORMAT(elf64-x86-64)
ENTRY(kmain)

PHDRS
{
	limine_requests PT_LOAD;
	text            PT_LOAD;
	rodata          PT_LOAD;
	data            PT_LOAD;
}

SECTIONS
{
	. = 0xffffffff80000000;

	.limine_requests : {
		KEEP(*(.limine_requests_start))
		KEEP(*(.limine_requests))
		KEEP(*(.limine_requests_end))
	} :limine_requests

	. = ALIGN(4K);

	.text : {
		*(.text .text.*)
	} :text

	. = ALIGN(4K);

	.rodata : {
		*(.rodata .rodata.*)
	} :rodata

	. = ALIGN(4K);

	.data : {
		*(.data .data.*)
	} :data

	.bss : {
		*(.bss .bss.*)
		*(COMMON)
	} :data

	/DISCARD/ : {
		*(.eh_frame*)
		*(.note .note.*)
		*(.comment)
	}
}
