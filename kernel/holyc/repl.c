#include <stddef.h>

#include "repl.h"
#include "arch/x86_64/serial.h"
#include "holyc/eval.h"
#include "lib/format.h"

/* 6-3 lands holyc_eval dispatch on the captured buffer. The REPL is
 * the second caller of holyc_eval (the first being holyc_eval_self_test
 * in kmain, which stays smoke-side); each non-empty line goes through
 * the same parse + codegen + encode + relocate + commit + invoke
 * pipeline that printed `X` for the M3 step-5 witness. 6-4 wires
 * setjmp / longjmp parse-error recovery so the deliberate syntax
 * error in the M3 exit-criterion 5-line session does not halt the
 * kernel via runtime.c's exit() shim. */

/* HolyC source lines are usually short; 256 chars is comfortably
 * above the longest one-liner the M3 exit-criterion session (6-6)
 * dispatches. Overflow silently drops the offending char today —
 * surfaces in 6-3 if a manual session exposes a longer line. */
#define REPL_LINE_CAP 256

static void prompt(void)
{
	serial_puts("field> ");
}

void holyc_repl(void)
{
	serial_puts("Field OS REPL -- type ; to dispatch, "
	            "Ctrl-D to quit\n");

	char buf[REPL_LINE_CAP];
	size_t len = 0;

	prompt();
	for (;;) {
		char c = serial_getc();

		/* Ctrl-D (EOT, 0x04) — graceful exit. The banner promises
		 * it; honouring it here keeps the contract before 6-3
		 * defines what "after the REPL" means in dispatch terms. */
		if (c == 0x04) {
			serial_puts("\r\nCtrl-D, halting REPL\r\n");
			break;
		}

		/* Most serial terminals send CR on Enter; the kernel echoes
		 * CRLF for the visible newline either way. */
		if (c == '\r' || c == '\n') {
			serial_puts("\r\n");
			buf[len] = '\0';
			/* Empty Enter just redraws the prompt; calling
			 * holyc_eval("") returns 0 silently but the rc log
			 * would be needless noise for a bare keystroke. */
			if (len > 0) {
				int rc = holyc_eval(buf);
				serial_puts("eval rc=");
				if (rc < 0) {
					serial_puts("-");
					format_dec(
					    (uint64_t)(unsigned int)(-rc));
				} else {
					format_dec(
					    (uint64_t)(unsigned int)rc);
				}
				serial_puts("\r\n");
			}
			len = 0;
			prompt();
			continue;
		}

		/* macOS Terminal sends DEL on backspace; Linux console and
		 * PuTTY send BS. Treat both as delete-one-char and emit
		 * `\b \b` so the cell clears on either side. */
		if (c == 0x7F || c == 0x08) {
			if (len > 0) {
				len--;
				serial_putc('\b');
				serial_putc(' ');
				serial_putc('\b');
			}
			continue;
		}

		if (len < REPL_LINE_CAP - 1) {
			buf[len++] = c;
			serial_putc(c);
		}
	}
}
