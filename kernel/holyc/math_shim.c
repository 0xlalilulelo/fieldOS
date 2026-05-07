/* kernel/holyc/math_shim.c
 *
 * The four <math.h> functions the kernel-resident HolyC subset reaches
 * for, all from holyc/src/x86.c:56 ieee754(double). Compiled with
 * HOLYC_KERNEL_CFLAGS (SSE enabled) because `double fabs(double)` rides
 * xmm0 under SysV; the kernel proper's -mno-sse build cannot host it.
 *
 * Coverage is exactly what ieee754() needs for finite, positive,
 * normal doubles. The upstream algorithm itself is fragile outside
 * that range; we do not reach beyond it. Subnormals, infinities,
 * and NaN inputs reach AS_E_UNKNOWN through the encoder's own
 * float path before they ever land in ieee754(). */

#include <stdint.h>

#include "include/math.h"

/* fabs — strip the IEEE-754 binary64 sign bit. Bit-aliasing avoids
 * the FPU entirely; the only reason this TU needs SSE is the ABI. */
double fabs(double x)
{
	union { double d; uint64_t u; } v;
	v.d = x;
	v.u &= ~(1ULL << 63);
	return v.d;
}

/* floorl — round toward -inf. The upstream caller hands us
 * log2l(...) results, which are bounded by ~|1024| for any normal
 * binary64 input; that fits in long long with room to spare. */
long double floorl(long double x)
{
	long long i = (long long)x;
	long double r = (long double)i;
	if (r > x) {
		r -= 1.0L;
	}
	return r;
}

/* log2l — only the integer part is consumed (the upstream caller
 * wraps in floorl). For the integer-result regime, log2 of a normal
 * binary64 value is exactly its unbiased exponent. We extract from a
 * (double) round-trip rather than 80-bit long-double bits because the
 * caller's input traces back to a double in every reachable path.
 *
 * Subnormals, zero, infinities, NaN are out of scope — the encoder's
 * float path filters them before ieee754() ever runs. The 0x0
 * exponent case below returns -1023 (subnormal) only as defensive
 * behavior; in our reach, it is unreached. */
long double log2l(long double x)
{
	double d = (double)x;
	union { double d; uint64_t u; } v;
	v.d = d;
	int exp_field = (int)((v.u >> 52) & 0x7FF);
	int unbiased  = exp_field - 1023;
	return (long double)unbiased;
}

/* ldexpl — multiply by 2^n. For our reach, |n| <= 1024; iteration
 * is fine. Bit-fiddling the long-double exponent would be faster
 * but tangles with x86_64's 80-bit format; the loop avoids that. */
long double ldexpl(long double x, int n)
{
	while (n > 0) {
		x *= 2.0L;
		n--;
	}
	while (n < 0) {
		x *= 0.5L;
		n++;
	}
	return x;
}
