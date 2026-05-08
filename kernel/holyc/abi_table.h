#ifndef FIELDOS_HOLYC_ABI_TABLE_H
#define FIELDOS_HOLYC_ABI_TABLE_H

#include <stddef.h>
#include <stdint.h>

/* ABI symbol resolution table for the in-kernel HolyC runtime.
 * Maps symbol names that holyc/src/x86.c emits (and that walker
 * pass-2 deferred as extern relocations) to runtime addresses.
 * Pass-3 (5-3c) walks the deferred extern table and patches each
 * rel32 against the address abi_table_lookup returns.
 *
 * Two kinds of entries:
 *   1. Storage entries — argc, argv. The upstream main wrapper
 *      auto-emits `.comm argc, 8, 8` plus `movq %rdi, argc(%rip)`
 *      writes; the kernel has no real argc/argv, but the storage
 *      must exist somewhere or the rel32 lands in unmapped memory.
 *      8-byte static U64s in abi_table.c absorb the writes silently
 *      and are never read.
 *   2. Function entries — printf/_printf (5-3b), and any future
 *      callable surface. M3 picks direct-address aliasing: the
 *      kernel runtime's existing primitive (printf in runtime.c)
 *      is named directly. No k_* shim TU exists in M3-B; the abi.h
 *      contract is honoured the first time a HolyC source unit
 *      actually calls a k_* symbol (5-4 or later).
 *
 * Cross-platform name aliasing: each logical symbol gets two
 * entries, `name` and `_name`. The kernel hcc's cross-GCC ELF
 * target emits unprefixed names (argc, argv); the host build's
 * macOS Mach-O target emits prefixed names (_printf, _PrintMessage).
 * asm_test.c verifies against the host corpus, so both shapes must
 * resolve. The 5-3e harness exercises the prefixed path explicitly.
 *
 * abi_table_lookup returns the absolute virtual address for the
 * symbol, or 0 if not found. 5-3d codifies the unresolved>0 hard-
 * error policy in eval.c. */
uint64_t abi_table_lookup(const char *name, size_t name_len);

/* Boot self-test. Asserts the table is non-empty and that the
 * canonical entry (argc) resolves to a non-zero address. Halts the
 * kernel on regression so CI smoke catches table-shape changes. */
void holyc_abi_self_test(void);

#endif
