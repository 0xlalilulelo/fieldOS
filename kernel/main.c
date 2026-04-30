#include <stdint.h>

#include "limine.h"
#include "arch/x86_64/gdt.h"
#include "arch/x86_64/serial.h"

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
	serial_puts("Field OS: stage 0 reached\n");
	gdt_init();
	serial_puts("FIELD_OS_BOOT_OK\n");
	halt();
}
