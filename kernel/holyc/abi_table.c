#include <stddef.h>
#include <stdint.h>

#include "abi_table.h"
#include "arch/x86_64/serial.h"
#include "lib/format.h"

/* argc / argv storage. The upstream main wrapper auto-emits
 *   .comm argc, 8, 8
 *   movq  %rdi, argc(%rip)
 *   movq  %rsi, argv(%rip)
 * and pass-2 surfaces the two %rip-relative references as extern
 * relocations (confirmed by the boot smoke for the witness
 * `I64 F() { return 42; } F();`: argc @ 40, argv @ 47). The kernel
 * has no real argc/argv; the writes are absorbed silently and the
 * locations are never read. */
static uint64_t abi_argc_storage;
static uint64_t abi_argv_storage;

typedef struct {
	const char *name;
	uint64_t    addr;
} AbiEntry;

/* Each logical symbol gets both unprefixed and prefixed entries
 * (see abi_table.h's cross-platform aliasing rationale). The list
 * grows in 5-3b (printf), at M4 (Patrol surface, K_ABI_VERSION
 * bump), and as later witnesses pull in additional ABI symbols.
 * Linear scan is fine while the count stays small; a hash is
 * config-without-consumer until corpus inputs push past ~50. */
static const AbiEntry abi_entries[] = {
	{"argc",  (uint64_t)(uintptr_t)&abi_argc_storage},
	{"_argc", (uint64_t)(uintptr_t)&abi_argc_storage},
	{"argv",  (uint64_t)(uintptr_t)&abi_argv_storage},
	{"_argv", (uint64_t)(uintptr_t)&abi_argv_storage},
};

#define ABI_ENTRIES_N (sizeof(abi_entries) / sizeof(abi_entries[0]))

uint64_t abi_table_lookup(const char *name, size_t name_len)
{
	for (size_t i = 0; i < ABI_ENTRIES_N; i++) {
		const char *e = abi_entries[i].name;
		size_t      j = 0;
		while (j < name_len && e[j] != '\0' && e[j] == name[j]) {
			j++;
		}
		if (j == name_len && e[j] == '\0') {
			return abi_entries[i].addr;
		}
	}
	return 0;
}

void holyc_abi_self_test(void)
{
	serial_puts("ABI: ");
	format_dec((uint64_t)ABI_ENTRIES_N);
	serial_puts(" entries, argc -> ");
	uint64_t addr = abi_table_lookup("argc", 4);
	if (addr == 0) {
		serial_puts("NULL\n");
		for (;;) {
			__asm__ volatile ("cli; hlt");
		}
	}
	serial_puts("NONZERO... OK\n");
}
