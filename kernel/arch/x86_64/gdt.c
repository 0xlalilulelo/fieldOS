#include <stdint.h>

#include "gdt.h"

/* GDT entry encoding for x86_64 (Intel SDM Vol 3 §3.4.5;
 * AMD APM Vol 2 §4.8). Each entry is 64 bits, except the TSS
 * descriptor which is 128 bits and occupies two slots.
 *
 * Bit layout for a code/data segment descriptor:
 *   [15:0]   limit[15:0]
 *   [31:16]  base[15:0]
 *   [39:32]  base[23:16]
 *   [47:40]  access byte
 *              bit 47 P    present
 *              bit 46:45 DPL  privilege (0 or 3)
 *              bit 44 S    1 = code/data, 0 = system
 *              bit 43 E    1 = code, 0 = data
 *              bit 42 DC   direction/conforming (we keep 0)
 *              bit 41 RW   read for code, write for data
 *              bit 40 A    accessed (CPU sets)
 *   [51:48]  limit[19:16]
 *   [55:52]  flags
 *              bit 53 L    long-mode code segment
 *              bit 54 DB   32-bit op size (must be 0 if L=1)
 *              bit 55 G    granularity (4 KiB)
 *   [63:56]  base[31:24]
 *
 * In long mode, base/limit are ignored for code/data selectors,
 * but we set well-formed values so descriptors remain sensible
 * for compatibility-mode transitions in later phases. */

#define GDT_LIMIT_MAX  0x000F00000000FFFFULL  /* limit[15:0]=FFFF, [19:16]=F */

#define GDT_PRESENT    (1ULL << 47)
#define GDT_DPL_KERN   (0ULL << 45)
#define GDT_DPL_USER   (3ULL << 45)
#define GDT_NON_SYS    (1ULL << 44)
#define GDT_EXEC       (1ULL << 43)
#define GDT_RW         (1ULL << 41)
#define GDT_LONG       (1ULL << 53)
#define GDT_GRAN       (1ULL << 55)

/* Five normal entries (null + kernel code/data + user code/data)
 * plus a TSS descriptor that occupies two slots. Total: 7 qwords. */
static uint64_t gdt[7];

struct __attribute__((packed)) gdtr {
	uint16_t limit;
	uint64_t base;
};

/* Long-mode TSS layout per Intel SDM Vol 3 §7.7. RSP1/RSP2 are
 * unused (no rings 1/2). IST entries are zero for now; M1-B fills
 * them with #DF/#NMI/#MC stacks. iomap_base past the TSS limit
 * disables the I/O bitmap entirely. */
struct __attribute__((packed)) tss {
	uint32_t reserved0;
	uint64_t rsp[3];
	uint64_t reserved1;
	uint64_t ist[7];
	uint64_t reserved2;
	uint16_t reserved3;
	uint16_t iomap_base;
};

static struct tss tss = {
	.iomap_base = sizeof(struct tss),
};

extern void gdt_load_and_reload_segments(struct gdtr *r);

static uint64_t gdt_make_segment(uint64_t access)
{
	return GDT_LIMIT_MAX | access;
}

void gdt_init(void)
{
	gdt[0] = 0;  /* null descriptor */
	gdt[1] = gdt_make_segment(GDT_PRESENT | GDT_DPL_KERN | GDT_NON_SYS | GDT_EXEC | GDT_RW | GDT_LONG | GDT_GRAN);
	gdt[2] = gdt_make_segment(GDT_PRESENT | GDT_DPL_KERN | GDT_NON_SYS |            GDT_RW |            GDT_GRAN);
	gdt[3] = gdt_make_segment(GDT_PRESENT | GDT_DPL_USER | GDT_NON_SYS | GDT_EXEC | GDT_RW | GDT_LONG | GDT_GRAN);
	gdt[4] = gdt_make_segment(GDT_PRESENT | GDT_DPL_USER | GDT_NON_SYS |            GDT_RW |            GDT_GRAN);

	uint64_t tss_addr  = (uint64_t)&tss;
	uint64_t tss_limit = sizeof(struct tss) - 1;

	/* TSS descriptor is 16 bytes and lives in gdt[5] (low qword)
	 * and gdt[6] (upper 32 bits of base, plus reserved zero). */
	gdt[5] = (tss_limit & 0xFFFFULL)
	       | ((tss_addr & 0xFFFFFFULL) << 16)
	       | (0x89ULL << 40)                    /* P=1, DPL=0, S=0, type=9 (avail 64-bit TSS) */
	       | (((tss_limit >> 16) & 0xFULL) << 48)
	       | (((tss_addr  >> 24) & 0xFFULL) << 56);
	gdt[6] = (tss_addr >> 32) & 0xFFFFFFFFULL;

	struct gdtr r = {
		.limit = sizeof(gdt) - 1,
		.base  = (uint64_t)gdt,
	};

	gdt_load_and_reload_segments(&r);
	__asm__ volatile ("ltr %w0" :: "r"((uint16_t)GDT_TSS));
}

void gdt_set_ist(int index, uint64_t stack_top)
{
	if (index >= 1 && index <= 7) {
		tss.ist[index - 1] = stack_top;
	}
}
