#include <stdint.h>
#include <stddef.h>

#include "limine.h"
#include "pmm.h"
#include "arch/x86_64/serial.h"

/* Owned by kernel/main.c so the .limine_requests section sees them
 * during Limine's request scan. We only read the .response field
 * here. */
extern volatile struct limine_memmap_request limine_memmap_request_struct;
extern volatile struct limine_hhdm_request   limine_hhdm_request_struct;

#define PAGE_SHIFT 12
#define PAGE_SIZE  PMM_PAGE_SIZE
#define PAGE_MASK  (PAGE_SIZE - 1)

static uint64_t hhdm_offset;
static uint8_t *bitmap;
static uint64_t bitmap_size_bytes;
static uint64_t bitmap_pages;          /* total frames the bitmap covers */
static uint64_t free_pages;
static uint64_t total_usable_pages;
static uint64_t cursor_word;           /* word-aligned start of next search */

static inline int bit_get(uint64_t i)
{
	return (bitmap[i >> 3] >> (i & 7)) & 1;
}

static inline void bit_set(uint64_t i)
{
	bitmap[i >> 3] |= (uint8_t)(1u << (i & 7));
}

static inline void bit_clear(uint64_t i)
{
	bitmap[i >> 3] &= (uint8_t)~(1u << (i & 7));
}

static void *hhdm_phys_to_virt(uint64_t pa)
{
	return (void *)(hhdm_offset + pa);
}

void pmm_init(void)
{
	struct limine_memmap_response *mm = limine_memmap_request_struct.response;
	struct limine_hhdm_response   *h  = limine_hhdm_request_struct.response;

	if (mm == NULL || h == NULL) {
		/* Without a memmap or HHDM, the PMM cannot bootstrap.
		 * No idt panic infrastructure can rescue us here either —
		 * we silently leave bitmap=NULL and pmm_alloc_page returns 0. */
		return;
	}
	hhdm_offset = h->offset;

	/* Pass 1: find the highest USABLE address; size the bitmap
	 * to cover everything up to it. */
	uint64_t max_addr = 0;
	for (uint64_t i = 0; i < mm->entry_count; i++) {
		struct limine_memmap_entry *e = mm->entries[i];
		if (e->type != LIMINE_MEMMAP_USABLE) {
			continue;
		}
		uint64_t end = e->base + e->length;
		if (end > max_addr) {
			max_addr = end;
		}
	}
	bitmap_pages       = (max_addr + PAGE_SIZE - 1) / PAGE_SIZE;
	bitmap_size_bytes  = (bitmap_pages + 7) / 8;

	/* Pass 2: find the largest USABLE region big enough to host
	 * the bitmap; place it at the start of that region. */
	uint64_t bitmap_pa = 0;
	for (uint64_t i = 0; i < mm->entry_count; i++) {
		struct limine_memmap_entry *e = mm->entries[i];
		if (e->type != LIMINE_MEMMAP_USABLE) {
			continue;
		}
		if (e->length >= bitmap_size_bytes) {
			bitmap_pa = e->base;
			break;
		}
	}
	if (bitmap_pa == 0) {
		return;  /* couldn't fit the bitmap anywhere */
	}
	bitmap = (uint8_t *)hhdm_phys_to_virt(bitmap_pa);

	/* Initialise the bitmap as fully-used. We then clear bits for
	 * USABLE regions only, so any gap in the memmap (MMIO holes,
	 * reserved firmware) stays "used" by default — safer than
	 * starting fully-free and forgetting to mark something. */
	for (uint64_t i = 0; i < bitmap_size_bytes; i++) {
		bitmap[i] = 0xFF;
	}

	/* Mark USABLE pages free; count them for the stats line. */
	for (uint64_t i = 0; i < mm->entry_count; i++) {
		struct limine_memmap_entry *e = mm->entries[i];
		if (e->type != LIMINE_MEMMAP_USABLE) {
			continue;
		}
		uint64_t start_page = e->base / PAGE_SIZE;
		uint64_t end_page   = (e->base + e->length) / PAGE_SIZE;
		for (uint64_t p = start_page; p < end_page; p++) {
			bit_clear(p);
			free_pages++;
			total_usable_pages++;
		}
	}

	/* The bitmap's own pages are now (correctly) marked free; mark
	 * them used so we don't allocate over our own bookkeeping. */
	uint64_t bm_first = bitmap_pa / PAGE_SIZE;
	uint64_t bm_last  = (bitmap_pa + bitmap_size_bytes + PAGE_SIZE - 1) / PAGE_SIZE;
	for (uint64_t p = bm_first; p < bm_last; p++) {
		if (!bit_get(p)) {
			bit_set(p);
			free_pages--;
		}
	}

	/* Begin the two-finger cursor past the bitmap. */
	cursor_word = bm_last / 64;
}

uint64_t pmm_alloc_page(void)
{
	if (bitmap == NULL) {
		return 0;
	}

	uint64_t *words = (uint64_t *)bitmap;
	uint64_t total_words = bitmap_size_bytes / 8;

	for (int pass = 0; pass < 2; pass++) {
		uint64_t start = (pass == 0) ? cursor_word : 0;
		uint64_t end   = (pass == 0) ? total_words : cursor_word;
		for (uint64_t w = start; w < end; w++) {
			if (words[w] == 0xFFFFFFFFFFFFFFFFULL) {
				continue;  /* fully used; skip */
			}
			uint64_t free_mask = ~words[w];
			int bit_in_word = __builtin_ctzll(free_mask);
			uint64_t p = w * 64 + (uint64_t)bit_in_word;
			if (p >= bitmap_pages) {
				continue;  /* tail of bitmap is past tracked range */
			}
			words[w] |= (1ULL << bit_in_word);
			free_pages--;
			cursor_word = w;
			return p * PAGE_SIZE;
		}
	}
	return 0;  /* OOM */
}

void pmm_free_page(uint64_t pa)
{
	if (bitmap == NULL) {
		return;
	}
	uint64_t p = pa / PAGE_SIZE;
	if (p >= bitmap_pages) {
		return;  /* outside tracked range */
	}
	if (!bit_get(p)) {
		return;  /* double free; M2-D may turn this into a panic */
	}
	bit_clear(p);
	free_pages++;
	uint64_t w = p / 64;
	if (w < cursor_word) {
		cursor_word = w;
	}
}

void pmm_stats(uint64_t *free_bytes, uint64_t *total_bytes)
{
	if (free_bytes  != NULL) *free_bytes  = free_pages         * PAGE_SIZE;
	if (total_bytes != NULL) *total_bytes = total_usable_pages * PAGE_SIZE;
}

uint64_t pmm_hhdm_offset(void)
{
	return hhdm_offset;
}

/* TODO(M2-D or later): consolidate with idt.c's put_dec into a
 * shared kernel/lib/format.{h,c} once we have a third caller. */
static void serial_print_dec(uint64_t v)
{
	if (v == 0) {
		serial_putc('0');
		return;
	}
	char buf[21];
	int i = 20;
	buf[i] = '\0';
	while (v > 0) {
		buf[--i] = '0' + (char)(v % 10);
		v /= 10;
	}
	serial_puts(&buf[i]);
}

void pmm_print_stats(void)
{
	uint64_t fb = 0, tb = 0;
	pmm_stats(&fb, &tb);
	serial_puts("Memory: ");
	serial_print_dec(fb / (1024 * 1024));
	serial_puts(" MiB free of ");
	serial_print_dec(tb / (1024 * 1024));
	serial_puts(" MiB total\n");
}
