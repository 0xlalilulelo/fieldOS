#ifndef FIELDOS_HOLYC_RUNTIME_H
#define FIELDOS_HOLYC_RUNTIME_H

/* Freestanding runtime subset for the in-kernel HolyC compiler.
 *
 * Per ADR-0001 §3 step 3, this is the libc-shaped surface that lets
 * vendored sources under holyc/src/ compile and link against the
 * kernel once kernel.mk picks up the in-kernel hcc subset (M3-B
 * step-3 candidate B). Names match libc so the vendored sources need no
 * source edits; the headers libc would normally provide live in B
 * alongside the freestanding compile rules.
 *
 * Memory: malloc and free are weak by default and wrap kmalloc /
 * kfree from the slab. holyc/src/aostr.c's allocator redirect
 * (audit §3 bullet 3) consumes these symbols; the weak-default
 * discipline means we never edit the vendored file.
 *
 * String / memory operations: hand-written freestanding
 * implementations.
 *
 * Output: vsnprintf with the format specifiers vendored hcc uses
 * today — %s, %c, %d/%ld, %u/%lu, %x/%lx, %p, %%. Optional positive
 * width and 0-padding flag (e.g., %04d, %016lx). No precision, no
 * left-justify, no %.*s, no floats. Float specifiers (%f/%e/%g/%a)
 * raise a runtime halt so the gap is observable, not silent — M3-B
 * candidate B will surface which other gaps real callers hit. */

#include <stddef.h>
#include <stdarg.h>

void  *memcpy(void *dst, const void *src, size_t n);
void  *memmove(void *dst, const void *src, size_t n);
void  *memset(void *dst, int c, size_t n);
int    memcmp(const void *a, const void *b, size_t n);

size_t strlen(const char *s);
int    strcmp(const char *a, const char *b);
int    strncmp(const char *a, const char *b, size_t n);
char  *strchr(const char *s, int c);
char  *strdup(const char *s);
char  *strerror(int errnum);

int    printf(const char *fmt, ...) __attribute__((format(printf, 1, 2)));
int    snprintf(char *buf, size_t cap, const char *fmt, ...)
       __attribute__((format(printf, 3, 4)));
int    vsnprintf(char *buf, size_t cap, const char *fmt, va_list ap);

/* fprintf in the kernel ignores the FILE* and routes to printf. The
 * FILE * argument exists only so vendored callers compile. Same for
 * stderr — it is declared in <stdio.h> shim, defined in runtime.c
 * as NULL, and only ever passed to fprintf which discards it. */
struct __holyc_file;
int    fprintf(struct __holyc_file *fp, const char *fmt, ...)
       __attribute__((format(printf, 2, 3)));

void  *malloc(size_t bytes)              __attribute__((weak));
void   free(void *p)                     __attribute__((weak));
void  *realloc(void *p, size_t bytes)    __attribute__((weak));
void  *calloc(size_t nmemb, size_t size) __attribute__((weak));

/* exit in the kernel routes to a runtime halt with the status logged.
 * No graceful unwind in Phase 0; the entire compile is a single
 * top-level invocation and an exit from inside is a panic. */
_Noreturn void exit(int status);

/* Kernel has no tty. isatty always returns 0 so vendored code's ANSI
 * color guards short-circuit correctly. */
int    isatty(int fd);

void   holyc_runtime_self_test(void);

#endif
