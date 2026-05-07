#include <stddef.h>
#include <stdint.h>

#include "limine.h"
#include "arch/x86_64/framebuffer.h"
#include "arch/x86_64/gdt.h"
#include "arch/x86_64/idt.h"
#include "arch/x86_64/serial.h"
#include "holyc/eval.h"
#include "holyc/jit.h"
#include "holyc/runtime.h"
#include "mm/pmm.h"
#include "mm/slab.h"
#include "mm/vmm.h"

/* Limine protocol v12 anchored markers. The bootloader scans the
 * kernel image between the start and end markers for request magic;
 * grouping them in their own sections keeps the scan cheap and the
 * layout audit-able in the final ELF. */

__attribute__((used, section(".limine_requests_start")))
static volatile uint64_t limine_requests_start_marker[4] =
	LIMINE_REQUESTS_START_MARKER;

__attribute__((used, section(".limine_requests")))
static volatile uint64_t limine_base_revision[3] =
	LIMINE_BASE_REVISION(3);

/* Framebuffer request — externable so framebuffer.c can read the
 * response. The .limine_requests section keeps it in Limine's
 * scan range; volatile so the bootloader's response write to
 * .response is observed by our kernel after the handoff. */
__attribute__((used, section(".limine_requests")))
volatile struct limine_framebuffer_request limine_fb_request = {
	.id = LIMINE_FRAMEBUFFER_REQUEST_ID,
	.revision = 0,
	.response = NULL,
};

/* Memory map request — pmm.c reads it during pmm_init to discover
 * USABLE physical memory regions. */
__attribute__((used, section(".limine_requests")))
volatile struct limine_memmap_request limine_memmap_request_struct = {
	.id = LIMINE_MEMMAP_REQUEST_ID,
	.revision = 0,
	.response = NULL,
};

/* HHDM request — pmm.c uses the offset to read physical memory
 * via the bootloader's higher-half direct map (since the kernel's
 * own VMM doesn't exist yet — that lands in M2-B). */
__attribute__((used, section(".limine_requests")))
volatile struct limine_hhdm_request limine_hhdm_request_struct = {
	.id = LIMINE_HHDM_REQUEST_ID,
	.revision = 0,
	.response = NULL,
};

__attribute__((used, section(".limine_requests_end")))
static volatile uint64_t limine_requests_end_marker[2] =
	LIMINE_REQUESTS_END_MARKER;

static _Noreturn void halt(void)
{
	for (;;) {
		__asm__ volatile ("cli; hlt");
	}
}

void kmain(void)
{
	if (!LIMINE_BASE_REVISION_SUPPORTED(limine_base_revision)) {
		/* Bootloader doesn't speak revision 3. No serial yet —
		 * halt is the only graceful option at this stage. */
		halt();
	}

	serial_init();
	gdt_init();
	idt_init();
	pmm_init();
	vmm_init();
	slab_init();
	holyc_jit_init();
	holyc_init();
	fb_init();
	fb_puts("Hello, Field\n");
	pmm_print_stats();
	vmm_self_test();
	slab_self_test();
	holyc_jit_self_test();
	holyc_runtime_self_test();
	holyc_eval_self_test();
	serial_puts("Field OS: stage 2 reached\n");
	serial_puts("FIELD_OS_BOOT_OK\n");
	halt();
}
