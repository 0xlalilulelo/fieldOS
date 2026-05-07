#ifndef FIELDOS_HOLYC_INCLUDE_MATH_H
#define FIELDOS_HOLYC_INCLUDE_MATH_H

/* Freestanding <math.h> shim for the in-kernel HolyC subset.
 *
 * The vendored holyc/src/x86.c calls floorl/log2l/fabs/ldexpl from
 * exactly one helper — ieee754(double) at x86.c:56 — which builds a
 * binary64 bit pattern for `.double` directives in emitted asm. No
 * other math.h consumers exist across the kernel-resident subset
 * (verified by grep at 5-2a-prep entry).
 *
 * Implementations live in kernel/holyc/math_shim.c, compiled with
 * the holyc subset's SSE-enabled flags (HOLYC_KERNEL_CFLAGS) because
 * `double fabs(double)` consumes/produces xmm0 under SysV. The
 * kernel proper's -mno-sse build cannot host that signature. */

double      fabs(double x);
long double floorl(long double x);
long double log2l(long double x);
long double ldexpl(long double x, int exp);

#endif
