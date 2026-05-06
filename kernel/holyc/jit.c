#include <stddef.h>
#include <stdint.h>

#include "jit.h"
#include "arch/x86_64/serial.h"
#include "lib/format.h"
#include "mm/pmm.h"
#include "mm/vmm.h"

/* Bump cursor through the 16 MiB window. `backed_end` tracks the
 * first page past the last lazy-mapped frame; the alloc path walks
 * it forward as the cursor crosses page boundaries. */
static uint64_t cursor;
static uint64_t backed_end;

void holyc_jit_init(void)
{
	cursor     = HOLYC_JIT_BASE;
	backed_end = HOLYC_JIT_BASE;
}

static int back_one_page(uint64_t va)
{
	uint64_t pa = pmm_alloc_page();
	if (pa == 0) {
		return -1;
	}
	uint32_t flags = VMM_FLAG_PRESENT | VMM_FLAG_RW | VMM_FLAG_NOEXEC;
	if (vmm_map(vmm_kernel_pml4(), va, pa, flags) != 0) {
		pmm_free_page(pa);
		return -1;
	}
	return 0;
}

void *holyc_jit_alloc(uint64_t bytes)
{
	if (bytes == 0) {
		return NULL;
	}
	if (cursor + bytes > HOLYC_JIT_BASE + HOLYC_JIT_SIZE) {
		return NULL;
	}

	uint64_t addr = cursor;
	uint64_t need_end = (addr + bytes + 0xFFFULL) & ~0xFFFULL;
	while (backed_end < need_end) {
		if (back_one_page(backed_end) != 0) {
			return NULL;
		}
		backed_end += 4096;
	}

	cursor += bytes;
	return (void *)addr;
}

int holyc_jit_commit(void *addr, uint64_t len)
{
	uint64_t va  = (uint64_t)addr & ~0xFFFULL;
	uint64_t end = ((uint64_t)addr + len + 0xFFFULL) & ~0xFFFULL;
	uint64_t pml4 = vmm_kernel_pml4();
	uint32_t flags = VMM_FLAG_PRESENT | VMM_FLAG_RW; /* NX cleared */

	for (uint64_t p = va; p < end; p += 4096) {
		if (vmm_remap(pml4, p, flags) != 0) {
			return -1;
		}
	}
	return 0;
}

static _Noreturn void test_halt(void)
{
	for (;;) {
		__asm__ volatile ("cli; hlt");
	}
}

void holyc_jit_self_test(void)
{
	serial_puts("JIT: alloc/commit/exec... ");

	/* mov eax, 42 ; ret  →  b8 2a 00 00 00 c3 */
	static const uint8_t stub[6] = {
		0xB8, 0x2A, 0x00, 0x00, 0x00, 0xC3,
	};

	uint8_t *code = holyc_jit_alloc(sizeof stub);
	if (code == NULL) {
		serial_puts("FAIL (alloc)\n");
		test_halt();
	}
	for (uint64_t i = 0; i < sizeof stub; i++) {
		code[i] = stub[i];
	}
	if (holyc_jit_commit(code, sizeof stub) != 0) {
		serial_puts("FAIL (commit)\n");
		test_halt();
	}

	int (*fn)(void) = (int (*)(void))code;
	int result = fn();
	if (result != 42) {
		serial_puts("FAIL (returned ");
		format_dec((uint64_t)result);
		serial_puts(")\n");
		test_halt();
	}

	serial_puts("OK (returned ");
	format_dec((uint64_t)result);
	serial_puts(")\n");
}
