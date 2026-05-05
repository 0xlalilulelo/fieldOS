#ifndef FIELDOS_MM_PMM_H
#define FIELDOS_MM_PMM_H

#include <stdint.h>

/* Physical memory manager. One bit per 4 KiB frame, two-finger
 * cursor over a packed bitmap. SMP-safety lands at M11 (a
 * spinlock around the search and free paths); Phase 0 is single-CPU
 * and contention-free. */

#define PMM_PAGE_SIZE 4096ULL

void pmm_init(void);

/* Allocate one 4 KiB frame. Returns its physical address, or 0 if
 * out of memory. Callers obtain virtual access via the HHDM
 * (pmm_hhdm_offset() + pa). */
uint64_t pmm_alloc_page(void);

/* Release a frame previously returned by pmm_alloc_page. Silently
 * ignores double-free in M2-A; M2-D may turn this into a panic. */
void pmm_free_page(uint64_t pa);

/* Inspect the bitmap. *free_bytes is the current free count;
 * *total_bytes is the total USABLE memory observed at boot. */
void pmm_stats(uint64_t *free_bytes, uint64_t *total_bytes);

/* Limine's HHDM offset, captured at pmm_init. M2-B's VMM uses
 * this to read page-table memory in the absence of a kernel
 * mapping helper. */
uint64_t pmm_hhdm_offset(void);

/* Print "Memory: NNN MiB free of NNN MiB total" on serial.
 * Called from kmain after pmm_init. */
void pmm_print_stats(void);

#endif
