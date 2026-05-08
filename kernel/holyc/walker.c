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

/* --- Pass 2: JIT-buffer fill + local-label patch ------------------------- */

#define WALKER_PASS2_RELOCS_PER_LINE 4

static int sym_eq(const char *a, size_t alen, const char *b, size_t blen)
{
	if (alen != blen) {
		return 0;
	}
	for (size_t i = 0; i < alen; i++) {
		if (a[i] != b[i]) {
			return 0;
		}
	}
	return 1;
}

/* Look up `name[0..nlen)` in the label table. Returns 1 on hit with
 * *out_offset filled; 0 on miss. Linear scan — Bug_171.s has 11
 * labels; a hash is config-without-consumer until corpus inputs grow
 * past ~50 per module. */
static int label_lookup(const HolycLabelTable *labels,
                        const char *name, size_t nlen,
                        size_t *out_offset)
{
	for (size_t i = 0; i < labels->count; i++) {
		if (sym_eq(labels->entries[i].name, labels->entries[i].name_len,
		           name, nlen)) {
			if (out_offset) {
				*out_offset = labels->entries[i].offset;
			}
			return 1;
		}
	}
	return 0;
}

int holyc_walker_pass2(const char *data, size_t len,
                       const HolycLabelTable *labels,
                       unsigned char *out, size_t out_cap,
                       size_t *out_len,
                       HolycExternTable *externs,
                       size_t *local_patched_count)
{
	if (externs) {
		externs->count    = 0;
		externs->overflow = 0;
	}
	if (local_patched_count) {
		*local_patched_count = 0;
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

		if (offset > out_cap) {
			return AS_E_NOSPACE;
		}
		size_t cap_remaining = out_cap - offset;

		Reloc  per_line[WALKER_PASS2_RELOCS_PER_LINE];
		size_t bytes       = 0;
		size_t reloc_count = 0;
		int    rc = asm_encode(data + line_start, line_len,
		                       out + offset, cap_remaining, &bytes,
		                       per_line, WALKER_PASS2_RELOCS_PER_LINE,
		                       &reloc_count);
		if (rc != AS_OK && rc != AS_E_UNKNOWN) {
			return rc;
		}

		/* Per-line relocations: resolve against the label table or
		 * defer to the extern list. asm_encode's r.offset is line-
		 * local; absolute is offset + r.offset. */
		for (size_t r = 0; r < reloc_count; r++) {
			size_t abs_off = offset + per_line[r].offset;
			size_t target  = 0;

			if (label_lookup(labels, per_line[r].sym,
			                 per_line[r].sym_len, &target)) {
				int32_t disp = (int32_t)(target - (abs_off + 4));
				out[abs_off + 0] = (unsigned char)(disp        & 0xFF);
				out[abs_off + 1] = (unsigned char)((disp >> 8)  & 0xFF);
				out[abs_off + 2] = (unsigned char)((disp >> 16) & 0xFF);
				out[abs_off + 3] = (unsigned char)((disp >> 24) & 0xFF);
				if (local_patched_count) {
					(*local_patched_count)++;
				}
				continue;
			}

			if (externs == NULL) {
				continue;
			}
			if (externs->count >= HOLYC_EXTERN_MAX) {
				externs->overflow++;
				continue;
			}
			HolycExternReloc *e = &externs->entries[externs->count];
			size_t copy = per_line[r].sym_len;
			if (copy >= HOLYC_EXTERN_SYM_MAX) {
				copy = HOLYC_EXTERN_SYM_MAX - 1;
			}
			for (size_t j = 0; j < copy; j++) {
				e->sym[j] = per_line[r].sym[j];
			}
			e->sym[copy]  = '\0';
			e->sym_len    = copy;
			e->buf_offset = abs_off;
			externs->count++;
		}

		offset += bytes;
	}

	if (out_len) {
		*out_len = offset;
	}
	return 0;
}

/* --- Pass 3: extern symbol resolution ----------------------------------- */

#define INT32_MIN_C ((int64_t)-0x80000000LL)
#define INT32_MAX_C ((int64_t) 0x7FFFFFFFLL)

int holyc_walker_pass3(const HolycExternTable *externs,
                       unsigned char *out, size_t out_len,
                       uint64_t base_va,
                       HolycAbiLookup lookup,
                       size_t *resolved_count,
                       size_t *unresolved_count)
{
	if (resolved_count) {
		*resolved_count = 0;
	}
	if (unresolved_count) {
		*unresolved_count = 0;
	}
	if (externs == NULL || out == NULL || lookup == NULL) {
		return AS_E_MALFORMED;
	}

	for (size_t i = 0; i < externs->count; i++) {
		const HolycExternReloc *e = &externs->entries[i];

		if (e->buf_offset + 4 > out_len) {
			return AS_E_NOSPACE;
		}

		uint64_t target = lookup(e->sym, e->sym_len);
		if (target == 0) {
			if (unresolved_count) {
				(*unresolved_count)++;
			}
			continue;
		}

		uint64_t patch_va = base_va + e->buf_offset;
		int64_t  disp64   = (int64_t)target - (int64_t)(patch_va + 4);
		if (disp64 < INT32_MIN_C || disp64 > INT32_MAX_C) {
			/* Out-of-rel32-reach. M3 has no thunking; surface as a
			 * hard error so a future allocator change that breaks
			 * the +/- 2 GiB invariant lands as a kernel panic via
			 * eval.c, not as a silent miscompile. */
			return AS_E_MALFORMED;
		}
		int32_t disp = (int32_t)disp64;

		out[e->buf_offset + 0] = (unsigned char)(disp        & 0xFF);
		out[e->buf_offset + 1] = (unsigned char)((disp >> 8)  & 0xFF);
		out[e->buf_offset + 2] = (unsigned char)((disp >> 16) & 0xFF);
		out[e->buf_offset + 3] = (unsigned char)((disp >> 24) & 0xFF);

		if (resolved_count) {
			(*resolved_count)++;
		}
	}

	return AS_OK;
}
