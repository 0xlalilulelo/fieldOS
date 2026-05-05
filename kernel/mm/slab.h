#ifndef FIELDOS_MM_SLAB_H
#define FIELDOS_MM_SLAB_H

#include <stdint.h>

/* Slab heap. Eight per-size caches at 16/32/64/128/256/512/1024/
 * 2048 bytes; allocations larger than 2 KiB are routed to a
 * contiguous-page path in the PMM. Both share kmalloc/kfree,
 * dispatched on a 16-bit magic word at the 4 KiB page base.
 * Single-CPU; no locks (lands at M11 with the SMP work). */

void slab_init(void);

/* Allocate `size` bytes. Returns NULL on size==0 or OOM. */
void *kmalloc(uint64_t size);

/* Release a pointer previously returned by kmalloc. NULL is a
 * silent no-op. A pointer that does not match either magic panics
 * via cli;hlt — Phase 0 has no graceful recovery path. */
void kfree(void *ptr);

/* 10,000 random-sized alloc / random-order free round-trip.
 * Halts on failure or leak so CI smoke catches regressions.
 * Prints a single line. */
void slab_self_test(void);

#endif
