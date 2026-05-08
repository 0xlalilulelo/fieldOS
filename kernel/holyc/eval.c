#include <stddef.h>

#include "eval.h"
#include "arch/x86_64/serial.h"
#include "lib/format.h"
#include "holyc/walker.h"

/* Vendored upstream headers — reachable because the per-file rule for
 * eval.o in kernel/holyc/holyc-kernel.mk extends KERNEL_CFLAGS with
 * -I holyc/src and -I kernel/holyc/include (the freestanding shim).
 * No SSE is enabled in this TU; eval.c does not touch double / xmm. */
#include "aostr.h"
#include "cctrl.h"
#include "compile.h"
#include "lexer.h"
#include "parser.h"

extern void *malloc(size_t);    /* runtime.c shim */

/* HCC_VERSION mirrors holyc/src/version.h's literal; defining it
 * here keeps the version-string observability concern local to
 * eval.c rather than depending on the vendored header chain. */
#define HCC_VERSION "beta-v0.0.10"

/* The Cctrl handle replaces M3-B/C-minimal's static-int flag. NULL
 * means cctrlNew has not been called or returned NULL on OOM;
 * non-NULL is the live compiler-control struct that the pipeline
 * threads through cctrlInitParse + parseToAst + compileToAsm. */
static Cctrl *holyc_cctrl;

/* Pass-1 walker output. Stored at file scope so 5-2c can consume the
 * table without re-walking; for now only 5-2b's smoke uses it. The
 * label name pointers alias into the AoStr the most-recent
 * compileToAsm returned, so the table is valid only between successive
 * holyc_eval calls. */
static HolycLabelTable holyc_label_table;
static size_t          holyc_total_bytes;

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

	/* Bypass holyc/src/compile.c's compileToAst, which assumes a
	 * file path (lexPushFile -> open / read). lexInit accepts a
	 * source pointer directly; we feed it the caller's string and
	 * synthesize a minimal cur_file so error-reporting paths
	 * (lexer.c:1322 #define syntax, lexer.c:1726 lexerReportLine)
	 * have something to point at. The string is aliased — aoStr's
	 * aoStrRelease is a vendored no-op, so nothing frees it.
	 *
	 * No CCF_PRE_PROC: 5-2a's witness contains no #include / #define
	 * and we have no built-in tos.HH header to feed. The audit's
	 * step 4 mention of preprocessor support is deferred. */
	Lexer *l = (Lexer *)malloc(sizeof(Lexer));
	if (l == NULL) {
		return -1;
	}
	lexInit(l, (char *)src, 0);

	LexFile *lf = (LexFile *)malloc(sizeof(LexFile));
	if (lf == NULL) {
		return -1;
	}
	AoStr *fname = aoStrNew();
	aoStrCat(fname, "<eval>");
	AoStr *srcbuf = aoStrNew();
	srcbuf->data     = (char *)src;
	srcbuf->capacity = 0;          /* sentinel: not owned, not freed */
	size_t slen = 0;
	while (src[slen]) {
		slen++;
	}
	srcbuf->len      = slen;
	lf->filename = fname;
	lf->ptr      = (char *)src;
	lf->lineno   = 1;
	lf->src      = srcbuf;
	l->cur_file  = lf;

	cctrlInitParse(holyc_cctrl, l);
	parseToAst(holyc_cctrl);

	AoStr *asmbuf = compileToAsm(holyc_cctrl);
	if (asmbuf == NULL) {
		return -1;
	}

	/* Log the AT&T text on serial, fenced so the smoke output stays
	 * readable when multiple eval calls run back-to-back. The bytes
	 * are 5-2a's witness; 5-2b walks the same buffer to build the
	 * label table, 5-2c re-walks to fill the JIT region. */
	serial_puts("Eval: compileToAsm ---\n");
	serial_puts(asmbuf->data);
	serial_puts("Eval: compileToAsm ---\n");

	/* 5-2b pass-1 walker. Builds the label table the JIT-fill pass
	 * (5-2c) consumes when patching local rel32 relocations. The
	 * static table inside holyc_label_table sidesteps the question
	 * of where the table's storage lives across the two passes —
	 * 5-2c will read it back; not freed until the next eval call. */
	int wrc = holyc_walker_pass1(asmbuf->data, asmbuf->len,
	                             &holyc_label_table,
	                             &holyc_total_bytes);
	if (wrc != 0) {
		serial_puts("Eval: walker rc=");
		format_dec((uint64_t)(unsigned int)(-wrc));
		serial_puts(" (negated)\n");
		return -1;
	}
	serial_puts("Eval: walker - ");
	format_dec((uint64_t)holyc_total_bytes);
	serial_puts(" bytes, ");
	format_dec((uint64_t)holyc_label_table.count);
	serial_puts(" label(s)");
	if (holyc_label_table.overflow) {
		serial_puts(" + overflow");
	}
	serial_puts(":\n");
	for (size_t li = 0; li < holyc_label_table.count; li++) {
		const HolycLabel *e = &holyc_label_table.entries[li];
		serial_puts("  ");
		for (size_t j = 0; j < e->name_len; j++) {
			serial_putc(e->name[j]);
		}
		serial_puts(" @ ");
		format_dec((uint64_t)e->offset);
		serial_puts("\n");
	}

	return 0;
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
	serial_puts("Eval: pipeline\n");

	if (holyc_eval("") != 0) {
		eval_halt("empty");
	}
	if (holyc_eval(NULL) != -1) {
		eval_halt("null");
	}
	/* 5-2a witness (kickoff §"Step 5-2 — pipeline driver"): a
	 * function definition followed by a call. compileToAsm produces
	 * the AT&T text 5-2b's line walker will consume. The asm dump
	 * lands between the two "---" fences in the smoke output.
	 *
	 * I64 (not U0) for F's return type: the kickoff's literal U0 is
	 * a HolyC type-mismatch (returning 42 from a void) and trips the
	 * upstream type checker's loggerWarning, which renders cosmetically
	 * through our vsnprintf shim's incomplete %.*s handling. The
	 * compileToAsm output is structurally identical for I64; only the
	 * informational warning goes away. */
	if (holyc_eval("I64 F() { return 42; } F();") != 0) {
		eval_halt("witness");
	}

	serial_puts("Eval: pipeline OK\n");
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
	 * is therefore a strong signal that the kernel-resident subset
	 * is observably linked into the kernel ELF and works at runtime.
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
