#ifndef FIELDOS_HOLYC_EVAL_H
#define FIELDOS_HOLYC_EVAL_H

/* The kernel-side entry point for the in-kernel HolyC compiler.
 *
 * Per phase-0.md §M3, the C kernel calls holyc_init() once memory
 * and serial are up, then holyc_eval(src) for each source unit
 * (eventually a serial-fed REPL line). M3-B candidate C-skeleton
 * lands a callable entry wired into kmain; the cctrl initialisation
 * that the literal kickoff scoped (C) for is deferred to (C-real)
 * and gated on (B-followup-2) realloc and the kernel-resident
 * subset expansion to ast.c / arena.c / containers.c. */

/* Initialise the in-kernel HolyC runtime. Idempotent today; will
 * own a cctrl handle once the vendored sources are linked. */
void holyc_init(void);

/* Compile-and-run one HolyC source unit. Skeleton return values:
 *   0   on empty input (src == "")
 *  -1   on NULL src, or on any non-empty src (no parser wired)
 * Once (C-real) lands, the contract sharpens to "0 on successful
 * compile-and-run, -1 on parser error or runtime panic." */
int holyc_eval(const char *src);

/* Boot self-test: holyc_eval("") returns 0; holyc_eval(NULL) and
 * holyc_eval("y = 6 * 7;") both return -1. Halts on any
 * unexpected return so CI smoke catches regressions. */
void holyc_eval_self_test(void);

/* Boot probe of the kernel-resident hcc subset (M3-B candidate
 * C-minimal). Allocates an AoStr via vendored holyc/src/aostr.c,
 * concatenates HCC_VERSION + " linked", prints to serial. Witnesses
 * that the holyc subset .o files are observably present in the
 * running kernel ELF and that the runtime + slab chain works
 * end-to-end at runtime. Prints "Subset: hcc <version> linked... OK"
 * on success; halts the kernel on any unexpected return. */
void holyc_subset_self_test(void);

#endif
