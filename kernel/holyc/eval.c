#include <stddef.h>

#include "eval.h"
#include "arch/x86_64/serial.h"

/* Forward-declare the vendored AoStr type and the two functions
 * holyc_subset_self_test exercises. Avoids pulling holyc/src/aostr.h
 * (which transitively #includes <sys/types.h> and lives in the
 * kernel-resident subset's shim namespace, not the kernel's) into a
 * kernel-build compilation unit. The struct shape is frozen by the
 * holyc/VERSION pin per ADR-0001 §1; bumping the pin requires
 * re-confirming this declaration matches.
 *
 * HCC_VERSION mirrors holyc/src/version.h's literal; defining it
 * here keeps the version-string observability concern local to
 * eval.c rather than depending on the vendored header chain. */
typedef struct AoStr {
	char  *data;
	size_t len;
	size_t capacity;
} AoStr;

extern AoStr *aoStrNew(void);
extern void   aoStrCat(AoStr *buf, const void *d);

#define HCC_VERSION "beta-v0.0.10"

/* Tracks whether holyc_init has been called. Guards against ordering
 * bugs where a caller reaches holyc_eval before kmain has wired the
 * runtime. C-real (gated on the cctrl.c integration that C-minimal
 * defers) will replace this flag with the cctrl handle that
 * cctrlNew returns. */
static int holyc_initialised;

void holyc_init(void)
{
	holyc_initialised = 1;
}

int holyc_eval(const char *src)
{
	if (!holyc_initialised) {
		return -1;
	}
	if (src == NULL) {
		return -1;
	}
	if (src[0] == '\0') {
		return 0;
	}
	/* Skeleton: no parser yet. Any non-empty source returns -1. */
	return -1;
}

static _Noreturn void eval_halt(const char *reason)
{
	serial_puts("Eval halt: ");
	serial_puts(reason);
	serial_puts("\n");
	for (;;) {
		__asm__ volatile ("cli; hlt");
	}
}

void holyc_eval_self_test(void)
{
	serial_puts("Eval: skeleton... ");

	if (holyc_eval("") != 0) {
		eval_halt("empty");
	}
	if (holyc_eval(NULL) != -1) {
		eval_halt("null");
	}
	if (holyc_eval("y = 6 * 7;") != -1) {
		eval_halt("non-empty");
	}

	serial_puts("OK\n");
}

void holyc_subset_self_test(void)
{
	serial_puts("Subset: hcc ");

	/* Exercise the vendored aostr API. aoStrNew goes through
	 * _aoStrAlloc -> globalArenaAllocate -> malloc (kernel slab);
	 * aoStrCat goes through strlen + aoStrCatLen + memcpy. The
	 * return survives gc-sections because kmain reaches it
	 * transitively from this function.
	 *
	 * The subset is built with SSE enabled (aostr.c reads variadic
	 * doubles via xmm); the kernel IDT entry path does not yet
	 * save/restore xmm state. M3 boots with IF=0 throughout, so no
	 * interrupt can fire here and there is no corruption window.
	 * ADR-0002 documents the constraint and the M4 obligation:
	 * extend exceptions.S with fxsave/fxrstor before the first
	 * `sti`. */
	AoStr *banner = aoStrNew();
	if (banner == NULL) {
		eval_halt("subset alloc");
	}
	aoStrCat(banner, HCC_VERSION);
	aoStrCat(banner, " linked");
	if (banner->data == NULL || banner->len == 0) {
		eval_halt("subset cat");
	}
	serial_puts(banner->data);

	/* aoStrRelease in vendored aostr.c is a no-op (it expects the
	 * arena allocator's bulk-release semantics from holyc/src/
	 * memory.c, which is excluded per ADR-0001 §3 step 3). Our
	 * globalArenaAllocate shim routes to malloc, so the AoStr
	 * struct + buffer leak ~80 bytes per probe. The probe runs once
	 * per boot; the leak is invisible to slab_self_test, which
	 * captured pmm_stats before this function ran. */

	serial_puts("... OK\n");
}
