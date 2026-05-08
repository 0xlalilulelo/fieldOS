#include "repl.h"
#include "arch/x86_64/serial.h"

/* 6-1: banner + halt. The line buffer (6-2), holyc_eval dispatch
 * (6-3), and parse-error recovery (6-4) land in subsequent commits.
 * The witness for this commit is purely structural: with FIELDOS_REPL
 * unset, kmain skips the call entirely and smoke stays green; with
 * FIELDOS_REPL=1 the banner prints and the kernel halts here instead
 * of reaching the stage-2 sentinel. */
void holyc_repl(void)
{
	serial_puts("Field OS REPL -- type ; to dispatch, "
	            "Ctrl-D to quit\n");
	serial_puts("(skeleton: banner only, dispatch lands in 6-3)\n");
	for (;;) {
		__asm__ volatile ("cli; hlt");
	}
}
