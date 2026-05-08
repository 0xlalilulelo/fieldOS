#ifndef FIELDOS_HOLYC_WALKER_H
#define FIELDOS_HOLYC_WALKER_H

/* Pass-1 line walker over an AT&T-text AoStr. The pipeline driver in
 * kernel/holyc/eval.c (5-2 step) calls this on the buffer that
 * compileToAsm() returns; the host harness in kernel/holyc/asm_test.c
 * runs the same routine over the checked-in corpus under
 * holyc/tests/corpus/ for label-set verification (5-2b's witness).
 *
 * Pass 1 yields each line's emitted byte count via asm_encode (with
 * NULL output buffers so nothing is written) and accumulates a running
 * offset. Lines ending ':' (after WS / trailing-comment trim) get
 * recorded as labels, paired with the offset at which they sit in the
 * eventual emitted buffer. 5-2c re-walks the same buffer in pass 2 to
 * fill the JIT region and patch local rel32s; 5-1's relocation API
 * surfaces extern symbols the label table does not match. */

#include <stddef.h>

typedef struct {
	const char *name;     /* into the input buffer; NOT NUL-terminated */
	size_t      name_len;
	size_t      offset;   /* byte offset in the eventual emitted buffer */
} HolycLabel;

#define HOLYC_LABEL_MAX 128

typedef struct {
	HolycLabel entries[HOLYC_LABEL_MAX];
	size_t     count;
	int        overflow;  /* nonzero if more than HOLYC_LABEL_MAX seen */
} HolycLabelTable;

/* Walk `data[0..len)` line by line. On success returns 0 with *labels
 * populated and *total_bytes set to the cumulative emitted byte count.
 * Returns the asm_encode failure rc (negative) on any real encoder
 * error; AS_E_UNKNOWN is tolerated (coverage gap, not regression) and
 * contributes 0 bytes to the offset.
 *
 * Either out param may be NULL — passing labels=NULL skips label
 * recording, total_bytes=NULL discards the count. */
int holyc_walker_pass1(const char *data, size_t len,
                       HolycLabelTable *labels,
                       size_t *total_bytes);

#endif
