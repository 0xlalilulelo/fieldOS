#ifndef FIELDOS_HOLYC_INCLUDE_ASSERT_H
#define FIELDOS_HOLYC_INCLUDE_ASSERT_H

/* Freestanding <assert.h> shim. The vendored holyc-lang sources use
 * assert(x) for invariant checks; we route failures into runtime_halt
 * via __holyc_assert_fail (defined in kernel/holyc/runtime.c when an
 * assertion is reached at runtime). M3-B candidate B: declared but
 * not yet defined; nm will report it as undefined. */

extern _Noreturn void __holyc_assert_fail(const char *expr,
                                          const char *file,
                                          int         line);

#define assert(x) ((x) ? (void)0 : __holyc_assert_fail(#x, __FILE__, __LINE__))

#endif
