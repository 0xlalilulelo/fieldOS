#include <stdint.h>

#include "gdt.h"
#include "idt.h"
#include "serial.h"
#include "lib/format.h"

/* IDT entry layout in long mode (Intel SDM Vol 3 §6.14.1).
 *   [15:0]    offset[15:0]
 *   [31:16]   segment selector
 *   [34:32]   IST index (0 = no switch, 1..7 = tss.ist[index-1])
 *   [39:35]   reserved
 *   [47:40]   type/attr (0x8E = P=1, DPL=0, 64-bit interrupt gate)
 *   [63:48]   offset[31:16]
 *   [95:64]   offset[63:32]
 *   [127:96]  reserved
 *
 * We use interrupt gates (type 0xE) for every vector — they clear
 * IF on entry, which is what we want for any exception that could
 * race with another. Trap gates (0xF) would let nested events fire
 * mid-handler; not useful at this stage. */
struct __attribute__((packed)) idt_entry {
	uint16_t offset_low;
	uint16_t selector;
	uint8_t  ist;
	uint8_t  type_attr;
	uint16_t offset_mid;
	uint32_t offset_high;
	uint32_t reserved;
};

static struct idt_entry idt[256];

/* The per-vector stub addresses, populated in exceptions.S. */
extern uint64_t isr_table[256];

/* Per-CPU IST stacks for the three exceptions where the current
 * stack is untrustworthy:
 *   #NMI (2)  — asynchronous; can fire on a half-built stack frame
 *   #DF (8)   — we already faulted on entry; current RSP suspect
 *   #MC (18)  — machine check; same family as #DF
 * 4 KiB is enough for the panic path: one C call, no recursion,
 * halt. M2 may grow these once the kernel does real work. */
static __attribute__((aligned(16))) uint8_t df_stack [4096];
static __attribute__((aligned(16))) uint8_t nmi_stack[4096];
static __attribute__((aligned(16))) uint8_t mc_stack [4096];

static void idt_set_gate(int vec, uint64_t handler, uint8_t ist)
{
	struct idt_entry *e = &idt[vec];
	e->offset_low  = handler & 0xFFFF;
	e->selector    = GDT_KERNEL_CODE;
	e->ist         = ist & 0x07;
	e->type_attr   = 0x8E;
	e->offset_mid  = (handler >> 16) & 0xFFFF;
	e->offset_high = (handler >> 32) & 0xFFFFFFFFULL;
	e->reserved    = 0;
}

void idt_init(void)
{
	gdt_set_ist(1, (uint64_t)(df_stack  + sizeof(df_stack)));
	gdt_set_ist(2, (uint64_t)(nmi_stack + sizeof(nmi_stack)));
	gdt_set_ist(3, (uint64_t)(mc_stack  + sizeof(mc_stack)));

	for (int i = 0; i < 256; i++) {
		uint8_t ist = 0;
		switch (i) {
		case 2:  ist = 2; break; /* #NMI */
		case 8:  ist = 1; break; /* #DF */
		case 18: ist = 3; break; /* #MC */
		}
		idt_set_gate(i, isr_table[i], ist);
	}

	struct __attribute__((packed)) {
		uint16_t limit;
		uint64_t base;
	} idtr = {
		.limit = sizeof(idt) - 1,
		.base  = (uint64_t)idt,
	};
	__asm__ volatile ("lidt %0" :: "m"(idtr));
}

/* Tiny formatted-output helpers used only by the panic path. No
 * allocation, no stdio, no callees that touch global state. */

static void put_hex64(uint64_t v)
{
	char buf[19];
	buf[0] = '0';
	buf[1] = 'x';
	for (int i = 0; i < 16; i++) {
		unsigned d = (v >> ((15 - i) * 4)) & 0xF;
		buf[2 + i] = d < 10 ? '0' + d : 'a' + (d - 10);
	}
	buf[18] = '\0';
	serial_puts(buf);
}

void exception_handler(struct regs *r)
{
	serial_puts("\nPANIC: vec=");
	format_dec(r->vector);
	serial_puts(" err=");
	put_hex64(r->error_code);
	serial_puts(" rip=");
	put_hex64(r->rip);
	serial_puts(" rsp=");
	put_hex64(r->rsp);
	serial_puts("\n");

	for (;;) {
		__asm__ volatile ("cli; hlt");
	}
}
