#ifndef FIELDOS_HOLYC_JIT_H
#define FIELDOS_HOLYC_JIT_H

#include <stdint.h>

/* JIT memory region for the HolyC runtime (phase-0.md §M3).
 *
 * 16 MiB of higher-half virtual address space reserved at boot,
 * lazy-backed by 4 KiB physical pages on demand. Pages are first
 * mapped W+NX (writable, non-executable). Once the codegen has
 * finished writing a stub, the caller commits the byte range with
 * holyc_jit_commit() which flips NX off per page via vmm_remap —
 * never a global W^X violation.
 *
 * Bump-cursor allocator. No per-allocation free in v0; the JIT
 * region is a single arena that the REPL drains and the next
 * compile refills. Per-stub free is M3-late. */

#define HOLYC_JIT_BASE   0xFFFFFFFFC0000000ULL
#define HOLYC_JIT_SIZE   (16ULL * 1024 * 1024)

void  holyc_jit_init(void);

/* Reserve `bytes` of writable, non-executable JIT memory. Backs
 * pages from the PMM as the cursor crosses page boundaries.
 * Returns NULL on OOM or on cursor exhaustion. */
void *holyc_jit_alloc(uint64_t bytes);

/* Mark `[addr, addr+len)` executable by clearing NX on every page
 * the range touches. Returns 0 on success, -1 if any page is not
 * mapped. */
int   holyc_jit_commit(void *addr, uint64_t len);

/* Boot self-test: alloc a page, write a 6-byte stub that returns
 * 42, commit, call it, assert == 42. Halts on failure so CI smoke
 * catches regressions. */
void  holyc_jit_self_test(void);

#endif
