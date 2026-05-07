#include <stdarg.h>
#include <stddef.h>
#include <stdint.h>

#include "runtime.h"
#include "arch/x86_64/serial.h"
#include "mm/slab.h"

/* See runtime.h for the contract. */

static _Noreturn void runtime_halt(const char *reason)
{
	serial_puts("Runtime panic: ");
	serial_puts(reason);
	serial_puts("\n");
	for (;;) {
		__asm__ volatile ("cli; hlt");
	}
}

/* --- memory operations ------------------------------------------------ */

void *memcpy(void *dst, const void *src, size_t n)
{
	unsigned char       *d = dst;
	const unsigned char *s = src;
	for (size_t i = 0; i < n; i++) {
		d[i] = s[i];
	}
	return dst;
}

void *memmove(void *dst, const void *src, size_t n)
{
	unsigned char       *d = dst;
	const unsigned char *s = src;
	if (d == s || n == 0) {
		return dst;
	}
	if (d < s) {
		for (size_t i = 0; i < n; i++) {
			d[i] = s[i];
		}
	} else {
		for (size_t i = n; i > 0; i--) {
			d[i - 1] = s[i - 1];
		}
	}
	return dst;
}

void *memset(void *dst, int c, size_t n)
{
	unsigned char *d = dst;
	for (size_t i = 0; i < n; i++) {
		d[i] = (unsigned char)c;
	}
	return dst;
}

/* --- string operations ------------------------------------------------ */

size_t strlen(const char *s)
{
	const char *p = s;
	while (*p) {
		p++;
	}
	return (size_t)(p - s);
}

int strcmp(const char *a, const char *b)
{
	while (*a && *a == *b) {
		a++;
		b++;
	}
	return (int)(unsigned char)*a - (int)(unsigned char)*b;
}

char *strchr(const char *s, int c)
{
	for (;; s++) {
		if (*s == (char)c) {
			return (char *)s;
		}
		if (*s == '\0') {
			return NULL;
		}
	}
}

char *strdup(const char *s)
{
	size_t n = strlen(s) + 1;
	char *p = malloc(n);
	if (p == NULL) {
		return NULL;
	}
	memcpy(p, s, n);
	return p;
}

/* --- weak-symbol allocator shim -------------------------------------- */

/* Default malloc/free wrappers over the slab. holyc/src/aostr.c (and
 * any other vendored source we end up linking under M3-B candidate B)
 * picks these up automatically. The kernel itself uses kmalloc/kfree
 * directly and never sees these names. Marked weak so an experimental
 * override (e.g. an arena allocator scoped to one compile pass) can
 * supplant them at link time without touching this file. */

__attribute__((weak)) void *malloc(size_t bytes)
{
	return kmalloc((uint64_t)bytes);
}

__attribute__((weak)) void free(void *p)
{
	kfree(p);
}

/* --- vsnprintf -------------------------------------------------------- */

struct fmtbuf {
	char  *buf;
	size_t cap;
	size_t pos;     /* chars that would be written, including any past cap */
};

static void fmt_put(struct fmtbuf *fb, char c)
{
	if (fb->pos < fb->cap) {
		fb->buf[fb->pos] = c;
	}
	fb->pos++;
}

static void fmt_puts(struct fmtbuf *fb, const char *s)
{
	while (*s) {
		fmt_put(fb, *s++);
	}
}

static void fmt_uint(struct fmtbuf *fb, uint64_t v, int base, int width, int zero_pad)
{
	char tmp[24];   /* 64-bit base-2 = 64 digits, base-8 = 22; base-10/16 fit easily */
	int  dlen = 0;
	if (v == 0) {
		tmp[dlen++] = '0';
	} else {
		while (v > 0) {
			uint64_t d = v % (uint64_t)base;
			tmp[dlen++] = (char)(d < 10 ? '0' + d : 'a' + (d - 10));
			v /= (uint64_t)base;
		}
	}
	int  pad  = (width > dlen) ? (width - dlen) : 0;
	char padc = zero_pad ? '0' : ' ';
	while (pad-- > 0) {
		fmt_put(fb, padc);
	}
	while (dlen-- > 0) {
		fmt_put(fb, tmp[dlen]);
	}
}

static void fmt_sint(struct fmtbuf *fb, int64_t v, int width, int zero_pad)
{
	if (v < 0) {
		fmt_put(fb, '-');
		if (width > 0) {
			width--;
		}
		fmt_uint(fb, (uint64_t)(-v), 10, width, zero_pad);
	} else {
		fmt_uint(fb, (uint64_t)v, 10, width, zero_pad);
	}
}

int vsnprintf(char *buf, size_t cap, const char *fmt, va_list ap)
{
	struct fmtbuf fb = { buf, cap, 0 };

	while (*fmt) {
		if (*fmt != '%') {
			fmt_put(&fb, *fmt++);
			continue;
		}
		fmt++;  /* skip '%' */

		int zero_pad = 0;
		int width    = 0;
		int long_flag = 0;

		if (*fmt == '0') {
			zero_pad = 1;
			fmt++;
		}
		while (*fmt >= '0' && *fmt <= '9') {
			width = width * 10 + (*fmt - '0');
			fmt++;
		}
		if (*fmt == 'l') {
			long_flag = 1;
			fmt++;
			if (*fmt == 'l') {
				fmt++;        /* tolerate %lld */
			}
		}

		char spec = *fmt;
		if (spec) {
			fmt++;
		}

		switch (spec) {
		case '%':
			fmt_put(&fb, '%');
			break;
		case 'c':
			fmt_put(&fb, (char)va_arg(ap, int));
			break;
		case 's': {
			const char *s = va_arg(ap, const char *);
			if (s == NULL) {
				s = "(null)";
			}
			fmt_puts(&fb, s);
			break;
		}
		case 'd':
		case 'i': {
			int64_t v = long_flag
				? (int64_t)va_arg(ap, long)
				: (int64_t)va_arg(ap, int);
			fmt_sint(&fb, v, width, zero_pad);
			break;
		}
		case 'u': {
			uint64_t v = long_flag
				? (uint64_t)va_arg(ap, unsigned long)
				: (uint64_t)va_arg(ap, unsigned int);
			fmt_uint(&fb, v, 10, width, zero_pad);
			break;
		}
		case 'x': {
			uint64_t v = long_flag
				? (uint64_t)va_arg(ap, unsigned long)
				: (uint64_t)va_arg(ap, unsigned int);
			fmt_uint(&fb, v, 16, width, zero_pad);
			break;
		}
		case 'p': {
			uintptr_t p = (uintptr_t)va_arg(ap, void *);
			fmt_puts(&fb, "0x");
			fmt_uint(&fb, (uint64_t)p, 16, 16, 1);
			break;
		}
		case 'f':
		case 'e':
		case 'g':
		case 'a':
			runtime_halt("printf: float specifier not yet supported");
			break;
		default:
			/* Unknown specifier — emit literal so the malformed
			 * site is visible in the output rather than swallowed. */
			fmt_put(&fb, '%');
			if (spec) {
				fmt_put(&fb, spec);
			}
			break;
		}
	}

	if (cap > 0) {
		size_t term = (fb.pos < cap) ? fb.pos : cap - 1;
		fb.buf[term] = '\0';
	}
	return (int)fb.pos;
}

int snprintf(char *buf, size_t cap, const char *fmt, ...)
{
	va_list ap;
	va_start(ap, fmt);
	int n = vsnprintf(buf, cap, fmt, ap);
	va_end(ap);
	return n;
}

int printf(const char *fmt, ...)
{
	char    buf[512];
	va_list ap;
	va_start(ap, fmt);
	int n = vsnprintf(buf, sizeof buf, fmt, ap);
	va_end(ap);
	serial_puts(buf);
	return n;
}

/* --- self-test -------------------------------------------------------- */

void holyc_runtime_self_test(void)
{
	serial_puts("Runtime: memcpy/memmove/printf/malloc... ");

	/* memset + memcpy round-trip a 1 KiB pattern */
	static char src[1024];
	static char dst[1024];
	memset(src, 0, sizeof src);
	for (int i = 0; i < 1024; i++) {
		src[i] = (char)((i * 31 + 7) & 0xff);
	}
	memset(dst, 0xAA, sizeof dst);
	memcpy(dst, src, sizeof src);
	for (int i = 0; i < 1024; i++) {
		if (dst[i] != src[i]) {
			runtime_halt("memcpy");
		}
	}

	/* memmove with overlap: slide right by 4 bytes. Source range
	 * [0..11] copied to [4..15]; the backward-copy path must run
	 * to preserve the unread tail bytes. */
	char m[16] = {
		'0','1','2','3','4','5','6','7',
		'8','9','a','b','c','d','e','f',
	};
	memmove(m + 4, m, 12);
	if (m[0] != '0' || m[3] != '3' || m[4] != '0' || m[15] != 'b') {
		runtime_halt("memmove");
	}

	/* string ops */
	if (strlen("Field") != 5)         runtime_halt("strlen");
	if (strcmp("abc", "abc") != 0)    runtime_halt("strcmp eq");
	if (strcmp("abc", "abd") >= 0)    runtime_halt("strcmp lt");
	if (strchr("abc", 'b') == NULL)   runtime_halt("strchr hit");
	if (strchr("abc", 'z') != NULL)   runtime_halt("strchr miss");

	/* snprintf format ladder — exercises %d, %x with width and
	 * 0-pad, %s, %c, and the cap=0 path (returns length without
	 * writing). */
	char buf[64];
	int  n = snprintf(buf, sizeof buf, "x=%d hex=0x%04x s=%s c=%c", 42, 0xC0DE, "ok", '!');
	if (n != 24) {
		runtime_halt("snprintf len");
	}
	if (strcmp(buf, "x=42 hex=0xc0de s=ok c=!") != 0) {
		runtime_halt("snprintf bytes");
	}
	if (snprintf(NULL, 0, "%d", 12345) != 5) {
		runtime_halt("snprintf cap=0");
	}

	/* malloc / free round-trip via the weak symbols */
	char *p = malloc(64);
	if (p == NULL) {
		runtime_halt("malloc");
	}
	memset(p, 0x5A, 64);
	if (((unsigned char *)p)[0] != 0x5A || ((unsigned char *)p)[63] != 0x5A) {
		runtime_halt("malloc store");
	}
	free(p);

	serial_puts("OK\n");
}
