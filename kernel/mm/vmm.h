#ifndef FIELDOS_MM_VMM_H
#define FIELDOS_MM_VMM_H

#include <stdint.h>

/* x86_64 4-level paging: PML4 -> PDPT -> PD -> PT -> 4 KiB page.
 * Page-table memory is accessed through Limine's HHDM (no
 * recursive mapping; phase-0.md §M2 explicitly rejects it).
 * Single-CPU; no locks (lands at M11 with the SMP work). */

enum {
	VMM_FLAG_PRESENT = 1u << 0,
	VMM_FLAG_RW      = 1u << 1,
	VMM_FLAG_USER    = 1u << 2,
	VMM_FLAG_GLOBAL  = 1u << 3,
	VMM_FLAG_NOEXEC  = 1u << 4,
};

/* Capture the current CR3 as the kernel master PML4. Limine's
 * mappings continue to work; we do not replace them. */
void vmm_init(void);

/* Map (virt -> phys) in the address space rooted at pml4_pa.
 * Allocates intermediate tables on demand from the PMM.
 * Returns 0 on success, -1 on OOM or invalid arg. */
int vmm_map(uint64_t pml4_pa, uint64_t virt, uint64_t phys, uint32_t flags);

/* Clear the leaf entry for virt. Intermediate tables are kept
 * (memory bloat is bounded; reclaim is a later optimisation).
 * Returns 0 on success, -1 if no mapping exists. */
int vmm_unmap(uint64_t pml4_pa, uint64_t virt);

/* Walk to the leaf and emit (phys + va_offset) into *phys_out.
 * Returns 0 on success, -1 if any level along the path is
 * not-present. */
int vmm_translate(uint64_t pml4_pa, uint64_t virt, uint64_t *phys_out);

/* Rewrite the leaf flags for an existing mapping while preserving
 * its physical frame. Used for per-page NX flips on the JIT region
 * (phase-0.md §M3) — never a global W^X violation. Returns 0 on
 * success, -1 if no mapping exists at virt. */
int vmm_remap(uint64_t pml4_pa, uint64_t virt, uint32_t new_flags);

/* Allocate a fresh PML4 with the upper half (entries 256..511)
 * cloned from the kernel master, so a user process inherits the
 * kernel mapping. Lower half is zero. Returns the PML4 physical
 * address, or 0 on OOM. */
uint64_t vmm_new_address_space(void);

/* The kernel master PML4 captured during vmm_init. */
uint64_t vmm_kernel_pml4(void);

/* 1 GiB map/unmap round-trip self-test. Halts on failure so CI
 * smoke catches regressions. Prints a one-line PMM delta on
 * success. */
void vmm_self_test(void);

#endif
