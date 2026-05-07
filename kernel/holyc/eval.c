#include <stddef.h>

#include "eval.h"
#include "arch/x86_64/serial.h"

/* Forward-declare the vendored Cctrl handle. Cctrl is the global
 * compiler state (token tables, type maps, register sets, AST list,
 * macro defs, ...); cctrlNew() in holyc/src/cctrl.c allocates and
 * populates it. We treat it as opaque on the kernel side — eval.c
 * never reaches into Cctrl's fields, only through the function
 * surface (cctrlNew at M3-B/C-real; cctrlInitParse, cctrlTokenGet,
 * etc. at step 4+). The struct shape is frozen by the holyc/VERSION
 * pin per ADR-0001 §1.
 *
 * HCC_VERSION mirrors holyc/src/version.h's literal; defining it
 * here keeps the version-string observability concern local to
 * eval.c rather than depending on the vendored header chain. */
typedef struct Cctrl Cctrl;
extern Cctrl *cctrlNew(void);

#define HCC_VERSION "beta-v0.0.10"

/* The Cctrl handle replaces M3-B/C-minimal's static-int flag. NULL
 * means cctrlNew has not been called or returned NULL on OOM;
 * non-NULL is the live compiler-control struct that future steps
 * (parse, codegen) will thread through. */
static Cctrl *holyc_cctrl;

void holyc_init(void)
{
	/* cctrlNew allocates ~10 hash maps and sets, populates the type
	 * symbol table from holyc/src/cctrl.c's static built_in_types
	 * array, and seeds the x86_registers and libc_functions sets by
	 * splitting comma-separated string literals (aoStrSplit ->
	 * aoStrDup -> ...). The subset is built with SSE enabled and
	 * cpu_enable_sse() ran in kmain before us, so xmm-emitting code
	 * paths (movdqa, %f variadic args) are safe; the M4 obligation
	 * in ADR-0002 §2 still applies before the first sti. */
	holyc_cctrl = cctrlNew();
}

int holyc_eval(const char *src)
{
	if (holyc_cctrl == NULL) {
		return -1;
	}
	if (src == NULL) {
		return -1;
	}
	if (src[0] == '\0') {
		return 0;
	}
	/* Skeleton: parser/codegen not yet wired. Any non-empty source
	 * returns -1. ADR-0001 §3 step 4 (in-tree x86_64 encoder) and
	 * step 5 (JIT integration) close this gap. */
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

	/* C-real witness: holyc_init must have run cctrlNew successfully.
	 * cctrlNew exercises far more of the subset than the C-minimal
	 * aoStrNew / aoStrCat probe did -- it walks containers.c (mapNew
	 * + setNew + mapAdd + setAdd), aostr.c (aoStrSplit's full
	 * variadic path including aoStrPrintfVa with %s formatting),
	 * arena.c (arenaInit, arenaAlloc), and ast.c (astGlobalCmdArgs +
	 * the whole AstType allocation chain). The handle being non-NULL
	 * is therefore a strong signal that the 9-file subset is
	 * observably linked into the kernel ELF and works at runtime.
	 *
	 * cctrlNew's allocation footprint is ~80 KiB (maps/sets + the
	 * built_in_types AstType array + 60 entries in libc_functions
	 * set). All allocations route through globalArenaAllocate, which
	 * the runtime shims to malloc; aoStrRelease is the standard
	 * vendored no-op (the arena was meant to bulk-release). The
	 * allocations leak for the kernel's lifetime; that is correct
	 * for a one-shot in-kernel REPL where the cctrl handle lives
	 * forever, and matches the C-minimal probe's own per-boot leak
	 * profile at a larger scale. */
	if (holyc_cctrl == NULL) {
		eval_halt("cctrlNew returned NULL");
	}

	serial_puts(HCC_VERSION);
	serial_puts(" ready... OK\n");
}
