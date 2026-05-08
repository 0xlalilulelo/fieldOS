#include <stdarg.h>
#include <stddef.h>
#include <stdint.h>

#include "runtime.h"
#include "arch/x86_64/serial.h"
#include "lib/format.h"
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

/* strndup — copy at most n bytes plus a NUL terminator. Used by
 * lexer.c (lexDefine) and x86.c (asmBinaryOp). The vendored callers
 * always pass a length they computed from a known-good source range,
 * so we trust n. */
char *strndup(const char *s, size_t n)
{
	size_t len = 0;
	while (len < n && s[len]) {
		len++;
	}
	char *p = malloc(len + 1);
	if (p == NULL) {
		return NULL;
	}
	memcpy(p, s, len);
	p[len] = '\0';
	return p;
}

int memcmp(const void *a, const void *b, size_t n)
{
	const unsigned char *p = a;
	const unsigned char *q = b;
	for (size_t i = 0; i < n; i++) {
		if (p[i] != q[i]) {
			return (int)p[i] - (int)q[i];
		}
	}
	return 0;
}

int strncmp(const char *a, const char *b, size_t n)
{
	for (size_t i = 0; i < n; i++) {
		unsigned char ca = (unsigned char)a[i];
		unsigned char cb = (unsigned char)b[i];
		if (ca != cb) {
			return (int)ca - (int)cb;
		}
		if (ca == 0) {
			return 0;
		}
	}
	return 0;
}

static unsigned char to_lower(unsigned char c)
{
	return (c >= 'A' && c <= 'Z') ? (unsigned char)(c + 32) : c;
}

int strncasecmp(const char *a, const char *b, size_t n)
{
	for (size_t i = 0; i < n; i++) {
		unsigned char ca = to_lower((unsigned char)a[i]);
		unsigned char cb = to_lower((unsigned char)b[i]);
		if (ca != cb) {
			return (int)ca - (int)cb;
		}
		if (ca == 0) {
			return 0;
		}
	}
	return 0;
}

/* strerror returns a stub string. Vendored holyc-lang only calls
 * strerror in error-reporting paths that do not run in the M3-B
 * kernel-resident subset (no file IO, no fork/exec, no networking).
 * If a real caller hits this, the returned string makes the stub
 * visible in the output rather than pretending to full POSIX. */
char *strerror(int errnum)
{
	static char buf[] = "(holyc runtime: strerror stub)";
	(void)errnum;
	return buf;
}

/* errno storage. Single global; no per-thread state — the kernel is
 * single-threaded for M3 (Patrol scheduling lands at M4). The shim
 * header in kernel/holyc/include/errno.h defines `errno` as a macro
 * for this symbol. */
int holyc_runtime_errno = 0;

/* Body of the assert() macro from kernel/holyc/include/assert.h.
 * Routes a failed expression through the same halt path as the
 * runtime's other panics, with the source location in the message. */
_Noreturn void __holyc_assert_fail(const char *expr, const char *file, int line)
{
	serial_puts("Assertion failed: ");
	serial_puts(expr);
	serial_puts(" at ");
	serial_puts(file);
	serial_puts(":");
	format_dec((uint64_t)line);
	serial_puts("\n");
	for (;;) {
		__asm__ volatile ("cli; hlt");
	}
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

/* realloc: NULL pointer reduces to malloc; size==0 frees and returns
 * NULL; allocation failure leaves the old block intact (per C99
 * 7.20.3.4). The new-block-then-copy-then-free path is correct but
 * not optimal — within-cache grows could reuse the same slot. The
 * optimisation belongs in the slab itself; runtime.c stays a thin
 * libc-shaped surface. */
__attribute__((weak)) void *realloc(void *p, size_t bytes)
{
	if (p == NULL) {
		return malloc(bytes);
	}
	if (bytes == 0) {
		free(p);
		return NULL;
	}
	uint64_t old_size = slab_size_of(p);
	void *q = malloc(bytes);
	if (q == NULL) {
		return NULL;
	}
	size_t copy = (old_size < (uint64_t)bytes) ? (size_t)old_size : bytes;
	memcpy(q, p, copy);
	free(p);
	return q;
}

__attribute__((weak)) void *calloc(size_t nmemb, size_t size)
{
	size_t bytes = nmemb * size;
	void *p = malloc(bytes);
	if (p == NULL) {
		return NULL;
	}
	memset(p, 0, bytes);
	return p;
}

/* --- vendored-tree externs ------------------------------------------- */

/* exit halts. The status is logged so the boundary between vendored
 * code's normal-return path and its bail-out path is observable in
 * the serial output. */
_Noreturn void exit(int status)
{
	serial_puts("Runtime exit(");
	format_dec((uint64_t)(unsigned int)status);
	serial_puts(") -- halting\n");
	for (;;) {
		__asm__ volatile ("cli; hlt");
	}
}

/* Kernel has no tty. The vendored tree calls isatty only to gate
 * ANSI escapes in pretty-printers; returning 0 short-circuits them. */
int isatty(int fd)
{
	(void)fd;
	return 0;
}

/* --- numeric parsers ------------------------------------------------- */

/* strtoull / strtoll — used by lexer.c:lexNumeric for hex (0x...) and
 * decimal integer literals. We honour leading whitespace, optional
 * sign for strtoll, and 0x/0X prefix when base==16 or base==0. The
 * vendored caller passes base 10 or 16 explicitly, so the auto-detect
 * branch (base==0) is uncovered today; if a future caller needs it,
 * the test surfaces. *endptr is set to the first unconsumed byte. */
unsigned long long strtoull(const char *nptr, char **endptr, int base)
{
	const char *p = nptr;
	while (*p == ' ' || *p == '\t' || *p == '\n') {
		p++;
	}
	if (*p == '+') {
		p++;
	}
	if ((base == 0 || base == 16)
	    && p[0] == '0' && (p[1] == 'x' || p[1] == 'X')) {
		p += 2;
		base = 16;
	} else if (base == 0) {
		base = (*p == '0') ? 8 : 10;
	}
	unsigned long long v = 0;
	while (*p) {
		int d = -1;
		char c = *p;
		if      (c >= '0' && c <= '9')              d = c - '0';
		else if (base == 16 && c >= 'a' && c <= 'f') d = c - 'a' + 10;
		else if (base == 16 && c >= 'A' && c <= 'F') d = c - 'A' + 10;
		else break;
		if (d >= base) {
			break;
		}
		v = v * (unsigned long long)base + (unsigned long long)d;
		p++;
	}
	if (endptr != NULL) {
		*endptr = (char *)p;
	}
	return v;
}

long long strtoll(const char *nptr, char **endptr, int base)
{
	const char *p = nptr;
	while (*p == ' ' || *p == '\t' || *p == '\n') {
		p++;
	}
	int neg = 0;
	if (*p == '-') {
		neg = 1;
		p++;
	} else if (*p == '+') {
		p++;
	}
	unsigned long long u = strtoull(p, endptr, base);
	return neg ? -(long long)u : (long long)u;
}

/* --- deferred POSIX surface -----------------------------------------
 *
 * The vendored holyc-lang reaches for POSIX file IO and a long-double
 * parser through code paths the kernel-resident pipeline does not
 * exercise: lexReadfile (called from lexPushFile, which fires only on
 * #include resolution; CCF_PRE_PROC is off in eval.c) and strtold
 * (called from lexNumeric only when a float literal appears in the
 * token stream; M3-B's witnesses are integer-only). --gc-sections
 * keeps the call sites because lexPushFile and lexNumeric are
 * themselves reachable from lex(); the link demands real symbols.
 *
 * Each stub halts loudly so a future input that does reach them
 * surfaces as a kernel panic with the function name, not a silent
 * miscompile. Real implementations land when the witness needs them
 * (#include support is a step-6 REPL nicety; float literals come
 * with the M3 exit criterion's full feature set). */

_Noreturn static void deferred_halt(const char *fn)
{
	serial_puts("Runtime: deferred POSIX stub reached: ");
	serial_puts(fn);
	serial_puts("\n");
	for (;;) {
		__asm__ volatile ("cli; hlt");
	}
}

int    open (const char *p, int f, ...)        { (void)p; (void)f; deferred_halt("open"); }
long   lseek(int fd, long off, int w)          { (void)fd; (void)off; (void)w; deferred_halt("lseek"); }
long   read (int fd, void *b, unsigned long n) { (void)fd; (void)b; (void)n; deferred_halt("read"); }
int    close(int fd)                           { (void)fd; deferred_halt("close"); }

/* strtold — long double parser. lexNumeric calls this only when the
 * token text contains '.', 'e', or 'E'; M3-B witnesses are integer-
 * only. Long double uses x87 ST(0)/ST(1), independent of SSE, so
 * this signature compiles under -mno-sse. */
long double strtold(const char *nptr, char **endptr) { (void)nptr; (void)endptr; deferred_halt("strtold"); }

/* fprintf in the kernel ignores the FILE * and routes to printf.
 * Vendored sources use it only for diagnostic output; conflating
 * stdout and stderr is fine for the M3 single-output-stream model. */
int fprintf(struct __holyc_file *fp, const char *fmt, ...)
{
	(void)fp;
	char    buf[512];
	va_list ap;
	va_start(ap, fmt);
	int n = vsnprintf(buf, sizeof buf, fmt, ap);
	va_end(ap);
	serial_puts(buf);
	return n;
}

/* Definitions for the externs declared in <stdio.h> shim and util.h.
 * stderr is passed only to fprintf, which discards it. is_terminal
 * gates ANSI color paths in cctrl.c and elsewhere; mirrors what
 * holyc/src/main.c sets it to via isatty -- always 0 here. */
struct __holyc_file *stderr = NULL;
struct __holyc_file *stdout = NULL;
struct __holyc_file *stdin  = NULL;
int is_terminal = 0;

/* globalArenaAllocate is declared in holyc/src/memory.h, defined in
 * holyc/src/memory.c (which is excluded from the kernel-resident
 * subset per ADR-0001 §3 step 3). aostr.c calls it for AoStr struct
 * and capacity-buffer allocation. The original is an arena bump-
 * pointer with bulk-release semantics; the kernel shim routes to
 * malloc -- aostr.c's aoStrRelease path frees individual allocations
 * already, so the arena's bulk-release optimisation is unused here.
 * If a future caller assumes "no per-allocation free is needed," the
 * leak surfaces in slab self-test and we revisit. */
void *globalArenaAllocate(unsigned int size)
{
	return malloc((size_t)size);
}

/* --- time stubs ------------------------------------------------------ */

/* gettimeofday returns zeros; localtime returns a static zero-filled
 * struct tm. The vendored holyc-lang reaches these in
 * cctrlAddBuiltinMacros to populate __TIME__, __DATE__, and
 * __TIMESTAMP__ at parse time. The output values become "00:00:00"
 * and "1900/01/01" inside any compiled-in-kernel module's macro
 * expansion -- cosmetic in a single-process boot, fixable when the
 * kernel grows a real clock source (M4 LAPIC calibration unblocks
 * a real gettimeofday implementation alongside k_time_ns). */

struct __holyc_timeval {
	long tv_sec;
	long tv_usec;
};
struct __holyc_timezone {
	int tz_minuteswest;
	int tz_dsttime;
};

int gettimeofday(struct __holyc_timeval *tv, struct __holyc_timezone *tz)
{
	(void)tz;
	if (tv != NULL) {
		tv->tv_sec  = 0;
		tv->tv_usec = 0;
	}
	return 0;
}

struct __holyc_tm {
	int tm_sec, tm_min, tm_hour;
	int tm_mday, tm_mon, tm_year;
	int tm_wday, tm_yday, tm_isdst;
};

struct __holyc_tm *localtime(const long *t)
{
	(void)t;
	static struct __holyc_tm zero;
	return &zero;
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
	if (strlen("Field") != 5)              runtime_halt("strlen");
	if (strcmp("abc", "abc") != 0)         runtime_halt("strcmp eq");
	if (strcmp("abc", "abd") >= 0)         runtime_halt("strcmp lt");
	if (strchr("abc", 'b') == NULL)        runtime_halt("strchr hit");
	if (strchr("abc", 'z') != NULL)        runtime_halt("strchr miss");
	if (memcmp("abc", "abc", 3) != 0)      runtime_halt("memcmp eq");
	if (memcmp("abc", "abd", 3) >= 0)      runtime_halt("memcmp lt");
	if (memcmp("abd", "abc", 3) <= 0)      runtime_halt("memcmp gt");
	if (memcmp("abc", "abd", 2) != 0)      runtime_halt("memcmp prefix");
	if (strerror(22) == NULL)              runtime_halt("strerror");

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

	/* realloc round-trip — three grow paths, each verifies that the
	 * old payload survives the move:
	 *   (1) within slab cache: 100 → 120 (both land in the 128-byte
	 *       cache; same cache_id, in-cache regrow)
	 *   (2) across cache boundary: 100 → 300 (128-byte → 512-byte
	 *       cache; different cache_id, slab → slab)
	 *   (3) slab → large path: 1500 → 5000 (2048-byte cache →
	 *       contiguous-page large path; SLAB_MAGIC → LARGE_MAGIC,
	 *       exercises both arms of slab_size_of)
	 * Numbers chosen so cache_id_for_size lands the bounds I expect;
	 * if the cache size table changes, this test wants a re-check. */
	char *r = malloc(100);
	if (r == NULL)             runtime_halt("realloc(1) malloc");
	memset(r, 0x33, 100);
	r = realloc(r, 120);
	if (r == NULL)             runtime_halt("realloc(1)");
	for (int i = 0; i < 100; i++) {
		if ((unsigned char)r[i] != 0x33) runtime_halt("realloc(1) preserve");
	}
	free(r);

	r = malloc(100);
	if (r == NULL)             runtime_halt("realloc(2) malloc");
	memset(r, 0x55, 100);
	r = realloc(r, 300);
	if (r == NULL)             runtime_halt("realloc(2)");
	for (int i = 0; i < 100; i++) {
		if ((unsigned char)r[i] != 0x55) runtime_halt("realloc(2) preserve");
	}
	free(r);

	r = malloc(1500);
	if (r == NULL)             runtime_halt("realloc(3) malloc");
	memset(r, 0x77, 1500);
	r = realloc(r, 5000);
	if (r == NULL)             runtime_halt("realloc(3)");
	for (int i = 0; i < 1500; i++) {
		if ((unsigned char)r[i] != 0x77) runtime_halt("realloc(3) preserve");
	}
	free(r);

	serial_puts("OK\n");
}
