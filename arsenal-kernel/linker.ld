/* SPDX-License-Identifier: BSD-2-Clause
 *
 * Higher-half kernel layout for Limine. Entry at 0xffffffff80000000,
 * the canonical -2 GB kernel base — paired with rustc code-model=kernel
 * so all symbol references fit in 32-bit signed displacements.
 *
 * Limine maps the kernel here per its protocol's "Kernel Address" feature
 * (base revision 3). The .requests sections below are scanned by Limine
 * for the request markers emitted by the limine crate; keeping them in
 * .rodata between explicit start/end markers is the convention from
 * Limine's PROTOCOL.md §"Features".
 *
 * Reference: https://github.com/limine-bootloader/limine/blob/v12.x/PROTOCOL.md
 */

OUTPUT_FORMAT(elf64-x86-64)
ENTRY(_start)

PHDRS
{
    text    PT_LOAD FLAGS(5);  /* R-X */
    rodata  PT_LOAD FLAGS(4);  /* R-- */
    data    PT_LOAD FLAGS(6);  /* RW- */
}

SECTIONS
{
    . = 0xffffffff80000000;

    .text : {
        *(.text .text.*)
    } :text

    . = ALIGN(CONSTANT(MAXPAGESIZE));

    .rodata : {
        *(.rodata .rodata.*)
    } :rodata

    .requests_start_marker : {
        KEEP(*(.requests_start_marker))
    } :rodata
    .requests : {
        KEEP(*(.requests))
    } :rodata
    .requests_end_marker : {
        KEEP(*(.requests_end_marker))
    } :rodata

    . = ALIGN(CONSTANT(MAXPAGESIZE));

    .data : {
        *(.data .data.*)
    } :data

    .bss : {
        *(COMMON)
        *(.bss .bss.*)
    } :data

    /DISCARD/ : {
        *(.eh_frame)
        *(.note .note.*)
    }
}
