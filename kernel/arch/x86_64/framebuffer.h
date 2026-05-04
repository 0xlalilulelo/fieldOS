#ifndef FIELDOS_ARCH_X86_64_FRAMEBUFFER_H
#define FIELDOS_ARCH_X86_64_FRAMEBUFFER_H

#include <stdint.h>

/* fb_init reads the Limine framebuffer response, captures the
 * pixel buffer, and clears it to black. If Limine did not return
 * a framebuffer (or returned zero of them), fb_init becomes a
 * no-op and the rest of the API silently does nothing — the
 * kernel will still print on serial. */
void fb_init(void);

/* Fill the entire framebuffer with the given 24-bit RGB color
 * encoded as 0x00RRGGBB. Assumes 32 bpp BGRA / BGRX (QEMU std VGA
 * default; other layouts will be addressed in M5+). */
void fb_clear(uint32_t color);

/* Render a single glyph at character cell (col, row), 8x8 cells
 * starting at framebuffer pixel (col*8, row*8). Out-of-range
 * coordinates are silently dropped. */
void fb_putc_at(int col, int row, char c, uint32_t fg, uint32_t bg);

/* Write a string starting at the current framebuffer cursor
 * position. Advances the cursor; '\n' moves to column 0 of the
 * next row. Drops characters that would land past the bottom row
 * (no scrolling in M1; M2+). */
void fb_puts(const char *s);

#endif
