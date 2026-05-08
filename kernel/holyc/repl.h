#ifndef FIELDOS_HOLYC_REPL_H
#define FIELDOS_HOLYC_REPL_H

/* The smallest possible HolyC REPL.
 *
 * Per phase-0.md §M3 and ADR-0001 §3 step 6, holyc_repl() reads lines
 * from the polled serial console and dispatches each line to
 * holyc_eval. It runs at the end of kmain when FIELDOS_REPL=1 is in
 * the build flags (set via `make repl-iso`); the default `make iso`
 * build leaves the call site dead so the smoke path stays unchanged.
 *
 * Step 6 lands incrementally:
 *   6-1  banner + halt skeleton (this commit)
 *   6-2  line buffer over serial_getc with echo + backspace
 *   6-3  dispatch buffers to holyc_eval, redraw the prompt
 *   6-4  setjmp/longjmp parse-error recovery so the deliberate
 *        syntax error in the M3 exit-criterion 5-line session does
 *        not halt the kernel
 *
 * holyc_repl() does not return. */
void holyc_repl(void);

#endif
