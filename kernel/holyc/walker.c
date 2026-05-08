/* kernel/holyc/walker.c
 *
 * Pass-1 line walker. Builds the label table the JIT-fill pass (5-2c)
 * needs to resolve local rel32 relocations against. See walker.h for
 * the contract.
 *
 * No libc. Compiles under both KERNEL_CFLAGS (kernel ELF, -mno-sse)
 * and the host asm-test rule in holyc/holyc.mk (clang/gcc with libc),
 * so the same logic verifies kernel-side eval output and host-side
 * Bug_171.s parity. */

#include <stddef.h>
#include <stdint.h>

#include "asm.h"
#include "walker.h"

static int isWS(char c)
{
	return c == ' ' || c == '\t';
}

/* Trim leading and trailing WS in place on a (start, len) pair. */
static void trimRange(const char **pstart, size_t *plen)
{
	const char *s = *pstart;
	size_t      n = *plen;
	while (n > 0 && isWS(*s)) {
		s++;
		n--;
	}
	while (n > 0 && isWS(s[n - 1])) {
		n--;
	}
	*pstart = s;
	*plen   = n;
}

int holyc_walker_pass1(const char *data, size_t len,
                       HolycLabelTable *labels,
                       size_t *total_bytes)
{
	if (labels) {
		labels->count    = 0;
		labels->overflow = 0;
	}

	size_t offset = 0;
	size_t i      = 0;

	while (i < len) {
		size_t line_start = i;
		while (i < len && data[i] != '\n') {
			i++;
		}
		size_t line_len = i - line_start;
		if (i < len) {
			i++;     /* skip newline */
		}

		/* Pass-1 length probe. We hand a throwaway stack buffer
		 * because asm_encode's outByte path treats out==NULL /
		 * cap==0 as overflow (returns AS_E_NOSPACE). 256 is more
		 * than enough for any single corpus line — instructions
		 * cap at ~10 bytes, .quad / .double at 8, .asciz strings
		 * in Bug_171.s at ~14. NULL relocs out-param matches 5-1's
		 * opt-out path — no relocation accounting on this pass. */
		uint8_t scratch[256];
		size_t  bytes = 0;
		int rc = asm_encode(data + line_start, line_len,
		                    scratch, sizeof scratch, &bytes,
		                    NULL, 0, NULL);
		if (rc != AS_OK && rc != AS_E_UNKNOWN) {
			return rc;
		}

		/* Label detection: strip trailing comment first (a `:` could
		 * appear inside a comment), then trim WS, then check for the
		 * trailing colon. Labels emit zero bytes — confirmed by the
		 * encoder's isNonEmittingLine. */
		const char *lp = data + line_start;
		size_t      ll = line_len;
		for (size_t j = 0; j < ll; j++) {
			if (lp[j] == '#') {
				ll = j;
				break;
			}
		}
		trimRange(&lp, &ll);
		if (ll > 0 && lp[ll - 1] == ':') {
			size_t name_len = ll - 1;
			if (labels) {
				if (labels->count >= HOLYC_LABEL_MAX) {
					labels->overflow++;
				} else {
					HolycLabel *e = &labels->entries[labels->count];
					e->name     = lp;
					e->name_len = name_len;
					e->offset   = offset;
					labels->count++;
				}
			}
		}

		offset += bytes;
	}

	if (total_bytes) {
		*total_bytes = offset;
	}
	return 0;
}
