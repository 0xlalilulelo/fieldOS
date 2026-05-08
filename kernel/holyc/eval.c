#include <stddef.h>

#include "eval.h"
#include "arch/x86_64/serial.h"
#include "lib/format.h"
#include "lib/setjmp.h"
#include "holyc/abi_table.h"
#include "holyc/jit.h"
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

/* Pass-1 walker output. Stored at file scope so 5-2c consumes the
 * table without re-walking. The label name pointers alias into the
 * AoStr the most-recent compileToAsm returned, so the table is valid
 * only between successive holyc_eval calls.
 *
 * holyc_extern_table holds extern relocations 5-3 will resolve against
 * the static ABI table in abi_table.c; symbol names are copied so the
 * entries outlive the per-line input that asm_encode aliased into. */
static HolycLabelTable  holyc_label_table;
static HolycExternTable holyc_extern_table;
static size_t           holyc_total_bytes;

/* Set up by holyc_eval before any vendored-tree call that can hit a
 * panic path; cleared after the path closes naturally. runtime.c::exit
 * reads holyc_eval_active through holyc_eval_try_longjmp; if set, it
 * longjmps back to holyc_eval's setjmp call site rather than halting.
 * volatile because the longjmp source (exit) and the setjmp site
 * (holyc_eval) share state through a non-local jump. */
static jmp_buf       holyc_eval_jmp_buf;
static volatile int  holyc_eval_active;

void holyc_eval_try_longjmp(int status)
{
	if (holyc_eval_active) {
		holyc_eval_active = 0;
		longjmp(holyc_eval_jmp_buf, status != 0 ? status : 1);
	}
}

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

	/* setjmp arms the longjmp landing site that runtime.c::exit
	 * reaches via holyc_eval_try_longjmp. Direct call returns 0 and
	 * we proceed into the parser; a longjmp from exit() returns the
	 * caught status (or 1 if exit was called with 0). The window
	 * spans parseToAst + compileToAsm; everything after asmGenerate
	 * (walker, JIT, invoke) is our own code and does not reach
	 * runtime.c::exit, so we close the window once asmbuf is in
	 * hand. ADR-0001 §3 step 6. */
	int jmp_rc = setjmp(holyc_eval_jmp_buf);
	if (jmp_rc != 0) {
		serial_puts("Eval: longjmp caught from exit(");
		if (jmp_rc < 0) {
			serial_puts("-");
			format_dec((uint64_t)(unsigned int)(-jmp_rc));
		} else {
			format_dec((uint64_t)(unsigned int)jmp_rc);
		}
		serial_puts(")\n");
		return -1;
	}
	holyc_eval_active = 1;

	parseToAst(holyc_cctrl);

	AoStr *asmbuf = compileToAsm(holyc_cctrl);

	/* Past the vendored panic-reachable surface; the walker, JIT
	 * region, and invoke path are our own code. Clear active so a
	 * future stray exit() falls through to runtime.c's halt rather
	 * than longjmping into a stale jmp_buf. */
	holyc_eval_active = 0;

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

	/* 5-2d: emit into the JIT region. holyc_jit_alloc bumps a cursor
	 * over the 16 MiB higher-half VA reserved at boot (HOLYC_JIT_BASE)
	 * and lazily backs pages W+NX. After pass-2 fills the buffer we
	 * call holyc_jit_commit to flip NX off via vmm_remap on every page
	 * the range touches; that's the W^X-safe handoff to executable —
	 * 5-3 then resolves the extern relocations and 5-4 jumps in. */
	unsigned char *out_buf =
		(unsigned char *)holyc_jit_alloc((uint64_t)holyc_total_bytes);
	if (out_buf == NULL) {
		serial_puts("Eval: holyc_jit_alloc failed\n");
		return -1;
	}
	size_t out_len       = 0;
	size_t local_patched = 0;
	int prc = holyc_walker_pass2(asmbuf->data, asmbuf->len,
	                             &holyc_label_table,
	                             out_buf, holyc_total_bytes, &out_len,
	                             &holyc_extern_table, &local_patched);
	if (prc != 0) {
		serial_puts("Eval: pass2 rc=");
		format_dec((uint64_t)(unsigned int)(-prc));
		serial_puts(" (negated)\n");
		return -1;
	}
	serial_puts("Eval: pass2 - ");
	format_dec((uint64_t)out_len);
	serial_puts(" bytes, ");
	format_dec((uint64_t)local_patched);
	serial_puts(" local patched, ");
	format_dec((uint64_t)holyc_extern_table.count);
	serial_puts(" extern deferred");
	if (holyc_extern_table.overflow) {
		serial_puts(" + overflow");
	}
	serial_puts(":\n");
	for (size_t ei = 0; ei < holyc_extern_table.count; ei++) {
		const HolycExternReloc *e = &holyc_extern_table.entries[ei];
		serial_puts("  ");
		serial_puts(e->sym);
		serial_puts(" @ ");
		format_dec((uint64_t)e->buf_offset);
		serial_puts("\n");
	}

	/* 5-3c pass-3: resolve extern relocations against the ABI table.
	 * Runs BEFORE holyc_jit_commit so the buffer stays writable
	 * irrespective of any future strict-W^X ADR (today commit only
	 * flips NX off, but the ordering is one line either way). The
	 * base_va parameter is the absolute VA the buffer executes at —
	 * which is the kernel-side pointer itself, since the JIT region
	 * is identity-mapped from the kernel's view through HOLYC_JIT_BASE.
	 *
	 * 5-3c logs both counts and returns 0 to eval.c on success even
	 * with unresolved > 0; 5-3d codifies the unresolved>0 hard-error
	 * policy. For the M3 witness `I64 F() { return 42; } F();` argc
	 * and argv resolve via 5-3a's storage entries, leaving 0
	 * unresolved. */
	size_t resolved   = 0;
	size_t unresolved = 0;
	int p3rc = holyc_walker_pass3(&holyc_extern_table,
	                              out_buf, out_len,
	                              (uint64_t)(uintptr_t)out_buf,
	                              abi_table_lookup,
	                              &resolved, &unresolved);
	if (p3rc != 0) {
		serial_puts("Eval: pass3 rc=");
		format_dec((uint64_t)(unsigned int)(-p3rc));
		serial_puts(" (negated)\n");
		return -1;
	}
	serial_puts("Eval: pass3 - ");
	format_dec((uint64_t)resolved);
	serial_puts(" resolved, ");
	format_dec((uint64_t)unresolved);
	serial_puts(" unresolved\n");

	/* 5-4a entry-point lookup. The upstream emit always wraps the
	 * source unit in a `main(int argc, char **argv)` whose prologue
	 * stores rdi/rsi into argc/argv before calling user code. 5-4b
	 * casts (out_buf + main_offset) and invokes; 5-4a logs the offset
	 * so a regression in the upstream's wrapper shape (rename, change
	 * of entry symbol) shows up in the smoke output before the call
	 * site dereferences a stale pointer. Logged before the unresolved-
	 * policy gate so the diagnostic still lands when we early-return. */
	size_t entry_offset = 0;
	int entry_found = holyc_label_lookup(&holyc_label_table,
	                                     "main", 4, &entry_offset);
	if (entry_found) {
		serial_puts("Eval: entry main @ ");
		format_dec((uint64_t)entry_offset);
		serial_puts("\n");
	} else {
		serial_puts("Eval: entry main not found\n");
	}

	/* 5-3d unresolved policy: ADR-0001 §3 step 5 says holyc_eval
	 * refuses to run a module whose externs do not all resolve. Print
	 * each unresolved symbol so the gap is visible (not a silent
	 * "0 relocs deferred" lie), then return -1. Skip commit — leaving
	 * zero rel32s at unresolved sites and flipping NX off would make
	 * a #PF panic the most likely outcome of any 5-4 invocation. */
	if (unresolved > 0) {
		for (size_t ei = 0; ei < holyc_extern_table.count; ei++) {
			const HolycExternReloc *e = &holyc_extern_table.entries[ei];
			if (abi_table_lookup(e->sym, e->sym_len) == 0) {
				serial_puts("Eval: unresolved extern '");
				serial_puts(e->sym);
				serial_puts("'\n");
			}
		}
		return -1;
	}

	/* JIT commit: flip NX off on the pages we just filled. After
	 * pass-3 the rel32 sites hold the right displacements, so the
	 * bytes are both executable and reachable — 5-4b invokes below. */
	if (holyc_jit_commit(out_buf, (uint64_t)out_len) != 0) {
		serial_puts("Eval: holyc_jit_commit failed\n");
		return -1;
	}

	/* 5-4b: invoke the JIT region. The upstream emit unconditionally
	 * wraps the source unit in
	 *
	 *   int main(int argc, char **argv) { ...; call F; ...; return; }
	 *
	 * with a SysV prologue that stores rdi/rsi into argc/argv before
	 * calling user code. Cast (out_buf + entry_offset) to a function
	 * pointer of the matching shape and invoke with (0, NULL); the
	 * return value lands on serial as a diagnostic.
	 *
	 * IF=0 invariant. ADR-0002 §1: the kernel runs with IF=0 from
	 * boot through M3, and JIT-resident code runs at the same ring
	 * with IF inherited. No new exception path opens here; the M4
	 * fxsave/fxrstor obligation in ADR-0002 §2 does not move forward
	 * because the witness has no float / xmm touch (5-4c's 'X\\n' is
	 * a string, not a double).
	 *
	 * Where the call lives (HANDOFF.md trade-off pair 3): inside
	 * holyc_eval, naturally co-located with the rest of the pipeline.
	 * A holyc_jit_invoke(addr, argc, argv) helper in jit.c is the
	 * right shape for M4 once Patrol exists and a second caller
	 * appears; M3 has only one. Promote at that point.
	 *
	 * Entry-point absence (5-4a's `entry_found == 0` branch) is a
	 * hard error here: the bytes have been committed but we have no
	 * symbol to enter them at. */
	if (!entry_found) {
		serial_puts("Eval: invoke skipped (no main entry)\n");
		return -1;
	}

	int (*entry_fn)(int, char **) =
		(int (*)(int, char **))(out_buf + entry_offset);
	int rc = entry_fn(0, NULL);

	serial_puts("Eval: invoke main(0, NULL) -> rc=");
	/* format_dec is unsigned; the I64 witness returns 42 (positive),
	 * the U0 witness returns whatever %rax held after the inner call
	 * (printf's return on 'X\\n', typically 2). Negative returns
	 * would print as a large unsigned — acceptable for a diagnostic
	 * until a witness exercises that path. */
	format_dec((uint64_t)(unsigned int)rc);
	serial_puts("\n");

	/* Witness line per the kickoff §"Step 5-2 — pipeline driver":
	 *   Eval: pipeline... OK (N bytes, M relocs deferred)
	 * After pass-3, M is the residual unresolved count — entries
	 * whose symbol abi_table_lookup returned 0 for. 5-3d turns this
	 * non-zero case into a hard error; 5-3c just reports it. */
	serial_puts("Eval: pipeline... OK (");
	format_dec((uint64_t)out_len);
	serial_puts(" bytes, ");
	format_dec((uint64_t)unresolved);
	serial_puts(" relocs deferred)\n");

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
	/* 5-4c witness — ADR-0001 §3 step 5 verbatim. `U0 F() { 'X\n';
	 * } F();` lowers (per the upstream emit) to a printf call against
	 * a string-pool .asciz with the literal X+newline as bytes. The
	 * upstream emits the raw bytes of the source string into the AT&T
	 * text via x86.c:2167's `.asciz "%S"`; 5-4c-prep's quote-aware
	 * walker is what makes that survive line iteration.
	 *
	 * The actual X (followed by a newline from the embedded 0x0A)
	 * lands on serial between `Eval: invoke main(0, NULL) -> rc=N`
	 * and `Eval: pipeline... OK (...)`. 5-4d's smoke bracket grep-
	 * asserts the position so a silent regression (jump to wrong
	 * offset, miss the printf call, etc.) fails CI. */
	if (holyc_eval("U0 F() { 'X\n'; } F();") != 0) {
		eval_halt("witness");
	}

	/* 5-3d unresolved-policy witness lives in the host harness, not
	 * here. The natural runtime construction — call a deliberately
	 * undeclared function and assert eval returns -1 — is unreachable:
	 * the upstream parser rejects undeclared calls during parseToAst
	 * with "Variable or function has not been defined" and exits via
	 * the runtime exit() shim before pass-3 sees the extern. Reaching
	 * the unresolved path requires a parser-acceptable forward
	 * declaration whose symbol isn't in abi_table; the witness shape
	 * is parser-internal and out of M3-B step-5 scope. asm_test.c's
	 * 5-3e mock injects an unresolved symbol directly into the extern
	 * table and asserts the per-entry pass-3 behaviour. */

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
