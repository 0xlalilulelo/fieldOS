#include <stddef.h>

#include "eval.h"
#include "arch/x86_64/serial.h"

/* Tracks whether holyc_init has been called. Guards against ordering
 * bugs where a caller reaches holyc_eval before kmain has wired the
 * runtime. C-real will replace this flag with the cctrl handle the
 * vendored compiler returns from cctrlNew. */
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
