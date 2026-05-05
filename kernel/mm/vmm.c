#include <stdint.h>
#include <stddef.h>

#include "pmm.h"
#include "vmm.h"
#include "arch/x86_64/serial.h"

/* x86_64 page-table entry bits (Intel SDM Vol 3 Ch 4.5).
 *   bit 0  P    present
 *   bit 1  RW   read/write (data) or instruction-fetch (code)
 *   bit 2  US   user/supervisor
 *   bit 3  PWT  page-level write-through
 *   bit 4  PCD  page-level cache disable
 *   bit 5  A    accessed (CPU sets)
 *   bit 6  D    dirty (CPU sets, leaf only)
 *   bit 7  PS   page size (large at PD/PDPT; we use 4 KiB only)
 *   bit 8  G    global (leaf only; honored when CR4.PGE=1)
 *   bits 12..51  physical frame number
 *   bit 63 NX   no-execute (honored when EFER.NXE=1; Limine sets it)
 */
#define PTE_P     (1ULL << 0)
#define PTE_RW    (1ULL << 1)
#define PTE_US    (1ULL << 2)
#define PTE_G     (1ULL << 8)
#define PTE_NX    (1ULL << 63)
#define PTE_ADDR_MASK 0x000FFFFFFFFFF000ULL

/* The shifts that pull each level's 9-bit index out of a virtual
 * address: PML4 = 47:39, PDPT = 38:30, PD = 29:21, PT = 20:12. */
static const int LEVEL_SHIFT[4] = { 39, 30, 21, 12 };

static uint64_t kernel_master_pml4_pa;

static uint64_t *table_virt(uint64_t entry_or_pa)
{
	return (uint64_t *)(pmm_hhdm_offset() + (entry_or_pa & PTE_ADDR_MASK));
}

static void zero_table(uint64_t *t)
{
	for (int i = 0; i < 512; i++) {
		t[i] = 0;
	}
}

/* Walk from the PML4 to the PT entry for `va`. If a level is not
 * present and create=1, allocate a new table from the PMM, zero
 * it, install it as PRESENT|RW|USER (leaf flags govern actual
 * access). Returns NULL on OOM or on a missing path with
 * create=0. */
static uint64_t *walk(uint64_t pml4_pa, uint64_t va, int create)
{
	uint64_t *table = table_virt(pml4_pa);

	for (int level = 0; level < 3; level++) {
		int idx = (int)((va >> LEVEL_SHIFT[level]) & 0x1FFu);
		uint64_t entry = table[idx];
		if (!(entry & PTE_P)) {
			if (!create) {
				return NULL;
			}
			uint64_t new_pa = pmm_alloc_page();
			if (new_pa == 0) {
				return NULL;
			}
			zero_table(table_virt(new_pa));
			table[idx] = new_pa | PTE_P | PTE_RW | PTE_US;
		}
		table = table_virt(table[idx]);
	}
	int leaf_idx = (int)((va >> LEVEL_SHIFT[3]) & 0x1FFu);
	return &table[leaf_idx];
}

void vmm_init(void)
{
	uint64_t cr3;
	__asm__ volatile ("mov %%cr3, %0" : "=r"(cr3));
	kernel_master_pml4_pa = cr3 & PTE_ADDR_MASK;
}

int vmm_map(uint64_t pml4_pa, uint64_t va, uint64_t pa, uint32_t flags)
{
	uint64_t *pte = walk(pml4_pa, va, 1);
	if (pte == NULL) {
		return -1;
	}
	uint64_t entry = pa & PTE_ADDR_MASK;
	if (flags & VMM_FLAG_PRESENT) entry |= PTE_P;
	if (flags & VMM_FLAG_RW)      entry |= PTE_RW;
	if (flags & VMM_FLAG_USER)    entry |= PTE_US;
	if (flags & VMM_FLAG_GLOBAL)  entry |= PTE_G;
	if (flags & VMM_FLAG_NOEXEC)  entry |= PTE_NX;
	*pte = entry;
	__asm__ volatile ("invlpg (%0)" :: "r"(va) : "memory");
	return 0;
}

int vmm_unmap(uint64_t pml4_pa, uint64_t va)
{
	uint64_t *pte = walk(pml4_pa, va, 0);
	if (pte == NULL || !(*pte & PTE_P)) {
		return -1;
	}
	*pte = 0;
	__asm__ volatile ("invlpg (%0)" :: "r"(va) : "memory");
	return 0;
}

int vmm_translate(uint64_t pml4_pa, uint64_t va, uint64_t *phys_out)
{
	uint64_t *pte = walk(pml4_pa, va, 0);
	if (pte == NULL || !(*pte & PTE_P)) {
		return -1;
	}
	if (phys_out != NULL) {
		*phys_out = (*pte & PTE_ADDR_MASK) | (va & 0xFFFULL);
	}
	return 0;
}

uint64_t vmm_new_address_space(void)
{
	uint64_t new_pa = pmm_alloc_page();
	if (new_pa == 0) {
		return 0;
	}
	uint64_t *new_pml4 = table_virt(new_pa);
	uint64_t *master   = table_virt(kernel_master_pml4_pa);

	for (int i = 0; i < 256; i++) {
		new_pml4[i] = 0;        /* user half: empty */
	}
	for (int i = 256; i < 512; i++) {
		new_pml4[i] = master[i]; /* kernel half: clone */
	}
	return new_pa;
}

uint64_t vmm_kernel_pml4(void)
{
	return kernel_master_pml4_pa;
}

/* TODO(M2-D): consolidate this with idt.c::put_dec and
 * pmm.c::serial_print_dec into kernel/lib/format.{h,c}. */
static void serial_print_dec(uint64_t v)
{
	if (v == 0) {
		serial_putc('0');
		return;
	}
	char buf[21];
	int i = 20;
	buf[i] = '\0';
	while (v > 0) {
		buf[--i] = '0' + (char)(v % 10);
		v /= 10;
	}
	serial_puts(&buf[i]);
}

static _Noreturn void test_halt(void)
{
	for (;;) {
		__asm__ volatile ("cli; hlt");
	}
}

void vmm_self_test(void)
{
	serial_puts("VMM: 1 GiB map/unmap... ");

	const uint64_t test_va_base = 0x100000000000ULL; /* 16 TiB; clearly unused */
	const uint64_t pages        = 256ULL * 1024;     /* 1 GiB / 4 KiB */
	uint64_t pml4 = vmm_kernel_pml4();

	uint64_t backing_pa = pmm_alloc_page();
	if (backing_pa == 0) {
		serial_puts("FAIL (backing OOM)\n");
		test_halt();
	}

	uint64_t free_before = 0;
	uint64_t total       = 0;
	pmm_stats(&free_before, &total);

	for (uint64_t i = 0; i < pages; i++) {
		if (vmm_map(pml4, test_va_base + i * 4096, backing_pa,
			    VMM_FLAG_PRESENT | VMM_FLAG_RW | VMM_FLAG_NOEXEC) != 0) {
			serial_puts("FAIL (map)\n");
			test_halt();
		}
	}

	for (uint64_t i = 0; i < pages; i++) {
		uint64_t out = 0;
		if (vmm_translate(pml4, test_va_base + i * 4096, &out) != 0) {
			serial_puts("FAIL (translate)\n");
			test_halt();
		}
		if (out != backing_pa) {
			serial_puts("FAIL (translate mismatch)\n");
			test_halt();
		}
	}

	for (uint64_t i = 0; i < pages; i++) {
		if (vmm_unmap(pml4, test_va_base + i * 4096) != 0) {
			serial_puts("FAIL (unmap)\n");
			test_halt();
		}
	}

	uint64_t out_after = 0;
	if (vmm_translate(pml4, test_va_base, &out_after) == 0) {
		serial_puts("FAIL (still mapped post-unmap)\n");
		test_halt();
	}

	pmm_free_page(backing_pa);

	uint64_t free_after = 0;
	pmm_stats(&free_after, &total);
	uint64_t retained_pages = (free_before > free_after)
		? (free_before - free_after) / 4096
		: 0;

	serial_puts("OK (PMM retained ");
	serial_print_dec(retained_pages);
	serial_puts(" pages = ");
	serial_print_dec(retained_pages * 4);
	serial_puts(" KiB for page tables)\n");
}
