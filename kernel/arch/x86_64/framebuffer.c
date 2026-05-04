#include <stdint.h>
#include <stddef.h>

#include "framebuffer.h"
#include "font_8x8.h"
#include "limine.h"

/* Owned by kernel/main.c so the .limine_requests section sees it
 * in the request scan. We extern the same volatile-qualified type
 * here. */
extern volatile struct limine_framebuffer_request limine_fb_request;

static volatile uint8_t *fb_addr;
static uint64_t fb_pitch;
static uint64_t fb_width;
static uint64_t fb_height;
static uint16_t fb_bpp;

static int fb_cols;        /* text columns: fb_width  / 8 */
static int fb_rows;        /* text rows:    fb_height / 8 */
static int cursor_col;
static int cursor_row;

void fb_init(void)
{
	struct limine_framebuffer_response *resp = limine_fb_request.response;
	if (resp == NULL || resp->framebuffer_count == 0) {
		return;
	}

	struct limine_framebuffer *f = resp->framebuffers[0];
	fb_addr   = (volatile uint8_t *)f->address;
	fb_pitch  = f->pitch;
	fb_width  = f->width;
	fb_height = f->height;
	fb_bpp    = f->bpp;

	fb_cols    = (int)(fb_width  / 8);
	fb_rows    = (int)(fb_height / 8);
	cursor_col = 0;
	cursor_row = 0;

	fb_clear(0x000000);
}

void fb_clear(uint32_t color)
{
	if (fb_addr == NULL) {
		return;
	}
	for (uint64_t y = 0; y < fb_height; y++) {
		volatile uint32_t *row = (volatile uint32_t *)(fb_addr + y * fb_pitch);
		for (uint64_t x = 0; x < fb_width; x++) {
			row[x] = color;
		}
	}
}

void fb_putc_at(int col, int row, char c, uint32_t fg, uint32_t bg)
{
	if (fb_addr == NULL) {
		return;
	}
	if (col < 0 || col >= fb_cols || row < 0 || row >= fb_rows) {
		return;
	}

	uint64_t glyph = font_8x8[(unsigned char)c];
	int px0 = col * 8;
	int py0 = row * 8;

	for (int y = 0; y < 8; y++) {
		uint8_t row_bits = (glyph >> (y * 8)) & 0xFF;
		volatile uint32_t *fb_row =
			(volatile uint32_t *)(fb_addr + (py0 + y) * fb_pitch);
		for (int x = 0; x < 8; x++) {
			fb_row[px0 + x] = ((row_bits >> x) & 1) ? fg : bg;
		}
	}
}

void fb_puts(const char *s)
{
	if (fb_addr == NULL) {
		return;
	}

	while (*s) {
		char c = *s++;
		if (c == '\n') {
			cursor_col = 0;
			cursor_row++;
			continue;
		}
		if (cursor_col >= fb_cols) {
			cursor_col = 0;
			cursor_row++;
		}
		if (cursor_row >= fb_rows) {
			/* No scrolling in M1; cursor is parked. */
			return;
		}
		fb_putc_at(cursor_col, cursor_row, c, 0x00FFFFFF, 0x00000000);
		cursor_col++;
	}
}
