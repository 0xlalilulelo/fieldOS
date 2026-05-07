#include <stdint.h>
#include <stddef.h>

#include "pmm.h"
#include "slab.h"
#include "arch/x86_64/serial.h"
#include "lib/format.h"

/* Slab page: 4 KiB PMM page with a 32-byte header at offset 0
 * (magic 0x5A1B). Slots are packed after the header for sizes
 * <= 512; for the 1024 / 2048 caches the first slot is placed at
 * offset cache_size so the slot grid stays aligned. The 2048
 * cache fits exactly one slot per page; the rest are well-packed.
 * Each free slot's first 2 bytes thread the freelist as a uint16_t
 * offset within the page (0 == end).
 *
 * Large alloc: contiguous N-page run with a 16-byte header at
 * offset 0 (magic 0x1A1B, pages count); payload starts at offset 16.
 *
 * When a slab becomes fully empty it is unlinked from its cache
 * list and its page returned to the PMM — the no-leaks self-test
 * depends on this. */

#define SLAB_HEADER_SIZE 32
#define LARGE_HEADER_SIZE 16
#define SLAB_MAGIC       0x5A1Bu
#define LARGE_MAGIC      0x1A1Bu
#define PAGE_SIZE        4096ULL
#define PAGE_MASK        (PAGE_SIZE - 1)

#define NUM_CACHES 8

static const uint16_t cache_sizes[NUM_CACHES] = {
	16, 32, 64, 128, 256, 512, 1024, 2048,
};

struct slab_header {
	uint16_t            magic;        /* 0x5A1B */
	uint8_t             cache_id;     /* 0..7 */
	uint8_t             reserved0;
	uint16_t            free_count;
	uint16_t            total_slots;
	uint16_t            first_free;   /* page offset; 0 == empty */
	uint16_t            reserved1;
	uint32_t            reserved2;
	struct slab_header *next;
	struct slab_header *prev;
};
_Static_assert(sizeof(struct slab_header) == SLAB_HEADER_SIZE,
	       "slab_header must be 32 bytes");

struct large_header {
	uint16_t magic;            /* 0x1A1B */
	uint16_t pages;
	uint8_t  reserved[12];
};
_Static_assert(sizeof(struct large_header) == LARGE_HEADER_SIZE,
	       "large_header must be 16 bytes");

static struct slab_header *cache_heads[NUM_CACHES];

static void *page_va(uint64_t pa)
{
	return (void *)(pmm_hhdm_offset() + pa);
}

static uint64_t va_to_pa(void *va)
{
	return (uint64_t)va - pmm_hhdm_offset();
}

static int cache_id_for_size(uint64_t size)
{
	if (size <= 16)   return 0;
	if (size <= 32)   return 1;
	if (size <= 64)   return 2;
	if (size <= 128)  return 3;
	if (size <= 256)  return 4;
	if (size <= 512)  return 5;
	if (size <= 1024) return 6;
	if (size <= 2048) return 7;
	return -1;
}

static _Noreturn void slab_panic(const char *msg)
{
	serial_puts("PANIC (slab): ");
	serial_puts(msg);
	serial_putc('\n');
	for (;;) {
		__asm__ volatile ("cli; hlt");
	}
}

static void slab_init_page(struct slab_header *h, int cache_id)
{
	uint16_t cache_size = cache_sizes[cache_id];
	uint16_t first_slot = (cache_size >= 1024)
		? cache_size
		: SLAB_HEADER_SIZE;
	uint16_t total = (uint16_t)((PAGE_SIZE - first_slot) / cache_size);

	h->magic       = SLAB_MAGIC;
	h->cache_id    = (uint8_t)cache_id;
	h->reserved0   = 0;
	h->free_count  = total;
	h->total_slots = total;
	h->first_free  = first_slot;
	h->reserved1   = 0;
	h->reserved2   = 0;
	h->next        = NULL;
	h->prev        = NULL;

	uint8_t *page = (uint8_t *)h;
	for (uint16_t i = 0; i + 1 < total; i++) {
		uint16_t this_off = (uint16_t)(first_slot + i * cache_size);
		uint16_t next_off = (uint16_t)(this_off + cache_size);
		*(uint16_t *)(page + this_off) = next_off;
	}
	uint16_t last_off = (uint16_t)(first_slot + (total - 1) * cache_size);
	*(uint16_t *)(page + last_off) = 0;
}

static void *slab_alloc(int cache_id)
{
	struct slab_header *slab;
	for (slab = cache_heads[cache_id]; slab != NULL; slab = slab->next) {
		if (slab->free_count > 0) {
			break;
		}
	}
	if (slab == NULL) {
		uint64_t pa = pmm_alloc_page();
		if (pa == 0) {
			return NULL;
		}
		slab = (struct slab_header *)page_va(pa);
		slab_init_page(slab, cache_id);
		slab->next = cache_heads[cache_id];
		if (cache_heads[cache_id] != NULL) {
			cache_heads[cache_id]->prev = slab;
		}
		cache_heads[cache_id] = slab;
	}

	uint8_t *page = (uint8_t *)slab;
	uint16_t slot_off = slab->first_free;
	uint16_t next_off = *(uint16_t *)(page + slot_off);
	slab->first_free = next_off;
	slab->free_count--;
	return page + slot_off;
}

static void slab_free(struct slab_header *slab, void *ptr)
{
	uint8_t *page = (uint8_t *)slab;
	uint16_t slot_off = (uint16_t)((uint8_t *)ptr - page);

	*(uint16_t *)(page + slot_off) = slab->first_free;
	slab->first_free = slot_off;
	slab->free_count++;

	if (slab->free_count == slab->total_slots) {
		int cache_id = slab->cache_id;
		if (slab->prev != NULL) {
			slab->prev->next = slab->next;
		} else {
			cache_heads[cache_id] = slab->next;
		}
		if (slab->next != NULL) {
			slab->next->prev = slab->prev;
		}
		pmm_free_page(va_to_pa(page));
	}
}

static void *large_alloc(uint64_t size)
{
	uint64_t total = size + LARGE_HEADER_SIZE;
	uint64_t pages = (total + PAGE_SIZE - 1) / PAGE_SIZE;
	if (pages > 0xFFFFu) {
		return NULL;
	}
	uint64_t pa = pmm_alloc_pages(pages);
	if (pa == 0) {
		return NULL;
	}
	struct large_header *h = (struct large_header *)page_va(pa);
	h->magic = LARGE_MAGIC;
	h->pages = (uint16_t)pages;
	return (uint8_t *)h + LARGE_HEADER_SIZE;
}

static void large_free(struct large_header *h)
{
	uint16_t pages = h->pages;
	pmm_free_pages(va_to_pa(h), pages);
}

void slab_init(void)
{
	for (int i = 0; i < NUM_CACHES; i++) {
		cache_heads[i] = NULL;
	}
}

void *kmalloc(uint64_t size)
{
	if (size == 0) {
		return NULL;
	}
	int cache_id = cache_id_for_size(size);
	if (cache_id < 0) {
		return large_alloc(size);
	}
	return slab_alloc(cache_id);
}

void kfree(void *ptr)
{
	if (ptr == NULL) {
		return;
	}
	uint8_t *page_base = (uint8_t *)((uintptr_t)ptr & ~PAGE_MASK);
	uint16_t magic = *(uint16_t *)page_base;
	if (magic == SLAB_MAGIC) {
		slab_free((struct slab_header *)page_base, ptr);
	} else if (magic == LARGE_MAGIC) {
		if ((uint8_t *)ptr != page_base + LARGE_HEADER_SIZE) {
			slab_panic("kfree: bad large pointer");
		}
		large_free((struct large_header *)page_base);
	} else {
		slab_panic("kfree: bad magic");
	}
}

uint64_t slab_size_of(void *ptr)
{
	if (ptr == NULL) {
		return 0;
	}
	uint8_t *page_base = (uint8_t *)((uintptr_t)ptr & ~PAGE_MASK);
	uint16_t magic = *(uint16_t *)page_base;
	if (magic == SLAB_MAGIC) {
		struct slab_header *h = (struct slab_header *)page_base;
		return cache_sizes[h->cache_id];
	}
	if (magic == LARGE_MAGIC) {
		if ((uint8_t *)ptr != page_base + LARGE_HEADER_SIZE) {
			slab_panic("slab_size_of: bad large pointer");
		}
		struct large_header *h = (struct large_header *)page_base;
		return (uint64_t)h->pages * PAGE_SIZE - LARGE_HEADER_SIZE;
	}
	slab_panic("slab_size_of: bad magic");
}

#define SELF_TEST_N 10000

struct slab_test_record {
	void    *ptr;
	uint64_t size;
};
static struct slab_test_record slab_test_records[SELF_TEST_N];

static _Noreturn void test_halt(void)
{
	for (;;) {
		__asm__ volatile ("cli; hlt");
	}
}

void slab_self_test(void)
{
	serial_puts("Slab: 10K random alloc/free... ");

	uint64_t free_before = 0;
	uint64_t total       = 0;
	pmm_stats(&free_before, &total);

	/* LCG x_{n+1} = a*x_n + c (Numerical Recipes constants). */
	uint32_t lcg = 0xDEADBEEFu;
	for (uint32_t i = 0; i < SELF_TEST_N; i++) {
		lcg = lcg * 1103515245u + 12345u;
		uint64_t size = 1 + (lcg & 4095u);   /* 1..4096 */
		void *p = kmalloc(size);
		if (p == NULL) {
			serial_puts("FAIL (alloc)\n");
			test_halt();
		}
		slab_test_records[i].ptr  = p;
		slab_test_records[i].size = size;
		*(uint8_t *)p = (uint8_t)(i & 0xFFu);
	}

	/* Cheap overlap detector: byte 0 of each allocation should
	 * still read back as i mod 256. Catches start-of-alloc
	 * collisions; combined with the PMM leak check below this is
	 * sufficient sanity for Phase 0. */
	for (uint32_t i = 0; i < SELF_TEST_N; i++) {
		if (*(uint8_t *)slab_test_records[i].ptr
		    != (uint8_t)(i & 0xFFu)) {
			serial_puts("FAIL (overlap)\n");
			test_halt();
		}
	}

	/* Fisher-Yates shuffle so frees happen in an order independent
	 * of allocation order. */
	for (uint32_t i = SELF_TEST_N - 1; i > 0; i--) {
		lcg = lcg * 1103515245u + 12345u;
		uint32_t j = lcg % (i + 1);
		struct slab_test_record tmp = slab_test_records[i];
		slab_test_records[i] = slab_test_records[j];
		slab_test_records[j] = tmp;
	}

	for (uint32_t i = 0; i < SELF_TEST_N; i++) {
		kfree(slab_test_records[i].ptr);
	}

	uint64_t free_after = 0;
	pmm_stats(&free_after, &total);
	if (free_after != free_before) {
		serial_puts("FAIL (leak: ");
		format_dec((free_before - free_after) / PAGE_SIZE);
		serial_puts(" pages)\n");
		test_halt();
	}

	serial_puts("OK (no leaks)\n");
}
