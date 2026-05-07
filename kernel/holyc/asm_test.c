/* kernel/holyc/asm_test.c
 *
 * Host harness for the in-tree x86_64 encoder. Reads a corpus file
 * (typically holyc/tests/corpus/Bug_171.s, produced by `make corpus`
 * per ADR-0003 §2), classifies each line, calls asm_encode() on each
 * instruction line, and — when the encoder accepts a line — compares
 * its output to $(CROSS_AS) byte-for-byte by re-assembling the line
 * standalone and reading the .text section.
 *
 * Each round of (C)/(D) coverage flips lines from `unknown` (encoder
 * doesn't yet recognise the mnemonic) to `encoded` (encoder produced
 * bytes matching GAS). Other return-code buckets — `mismatch`,
 * `malformed`, `nospace`, `gas-failed` — are real regressions and
 * fail the harness; `unknown` is an expected coverage gap and does
 * not.
 *
 * Built host-side via the `asm-test` rule in holyc/holyc.mk. Not
 * linked into the kernel ELF — main() only, no kernel-side use.
 *
 * AS_BIN / OBJCOPY_BIN are set via -D from holyc.mk to the cross
 * tools' absolute paths so the harness works regardless of $PATH. */

#include <ctype.h>
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

#include "asm.h"

#ifndef AS_BIN
#define AS_BIN "as"
#endif
#ifndef OBJCOPY_BIN
#define OBJCOPY_BIN "objcopy"
#endif

/* Limit verbose mismatch dumps so a regression doesn't drown CI. */
#define MAX_DIAG_PRINTS 8

/* --- Line classification -------------------------------------------------- */

typedef enum {
    LINE_BLANK,
    LINE_COMMENT,
    LINE_LABEL,
    LINE_DIRECTIVE,       /* .text, .globl, .L0:, etc. — non-emitting */
    LINE_DIRECTIVE_DATA,  /* .quad, .asciz, .double, .byte, .long, .short */
    LINE_INST,
} LineKind;

static int isDataDirective(const char *s, size_t n) {
    static const char *const data_directives[] = {
        ".quad", ".asciz", ".ascii", ".double", ".byte",
        ".long", ".short", ".word", ".string", ".float",
        NULL,
    };
    for (int i = 0; data_directives[i]; i++) {
        size_t dl = strlen(data_directives[i]);
        if (n >= dl && !strncmp(s, data_directives[i], dl) &&
            (n == dl || isspace((unsigned char)s[dl]))) {
            return 1;
        }
    }
    return 0;
}

static LineKind classifyLine(const char *line, size_t len) {
    size_t i = 0;
    while (i < len && isspace((unsigned char)line[i])) i++;
    if (i == len) return LINE_BLANK;
    if (line[i] == '#') return LINE_COMMENT;
    if (line[i] == '.') {
        return isDataDirective(line + i, len - i)
             ? LINE_DIRECTIVE_DATA
             : LINE_DIRECTIVE;
    }
    size_t last = len;
    while (last > i && isspace((unsigned char)line[last - 1])) last--;
    if (last > i && line[last - 1] == ':') return LINE_LABEL;
    return LINE_INST;
}

/* --- $(CROSS_AS) ground-truth lookup ------------------------------------- */

/* Re-assemble a single AT&T line through the cross assembler and
 * extract the resulting .text bytes. Returns 0 on success, negative
 * on any failure (GAS rejected the line, objcopy failed, file I/O
 * error). The .text section is empty for non-emitting lines (labels,
 * .text/.globl directives) — that case returns 0 with *out_len == 0.
 *
 * Lines that reference undefined symbols (call _printf, je .L4)
 * still assemble cleanly: GAS leaves a placeholder in .text and
 * records a relocation; the placeholder bytes are exactly what the
 * encoder must emit pre-link, so byte-for-byte comparison works. */
static int gasAssemble(const char *line, size_t len,
                       uint8_t *out, size_t out_cap, size_t *out_len) {
    char src_path[64], obj_path[64], bin_path[64];
    int  pid = (int)getpid();
    snprintf(src_path, sizeof(src_path), "/tmp/asm-test-%d.s",   pid);
    snprintf(obj_path, sizeof(obj_path), "/tmp/asm-test-%d.o",   pid);
    snprintf(bin_path, sizeof(bin_path), "/tmp/asm-test-%d.bin", pid);

    FILE *sf = fopen(src_path, "w");
    if (!sf) return -1;
    fprintf(sf, ".text\n%.*s\n", (int)len, line);
    fclose(sf);

    char cmd[1024];
    snprintf(cmd, sizeof(cmd),
             "%s -o %s %s 2>/dev/null", AS_BIN, obj_path, src_path);
    if (system(cmd) != 0) {
        unlink(src_path);
        return -2;
    }
    snprintf(cmd, sizeof(cmd),
             "%s -O binary -j .text %s %s 2>/dev/null",
             OBJCOPY_BIN, obj_path, bin_path);
    if (system(cmd) != 0) {
        unlink(src_path);
        unlink(obj_path);
        return -3;
    }

    FILE *bf = fopen(bin_path, "rb");
    if (!bf) {
        unlink(src_path);
        unlink(obj_path);
        return -4;
    }
    size_t n = fread(out, 1, out_cap, bf);
    fclose(bf);
    *out_len = n;

    unlink(src_path);
    unlink(obj_path);
    unlink(bin_path);
    return 0;
}

/* --- Stats and diagnostics ----------------------------------------------- */

typedef struct {
    int counts[6];        /* by LineKind */
    int enc_ok;           /* encoded; bytes match GAS */
    int enc_mismatch;     /* encoded; bytes disagree (REGRESSION) */
    int enc_unknown;      /* AS_E_UNKNOWN — coverage gap, expected */
    int enc_malformed;    /* AS_E_MALFORMED (REGRESSION) */
    int enc_nospace;      /* AS_E_NOSPACE (REGRESSION) */
    int enc_other;        /* AS_E_TODO + any unexpected rc (REGRESSION) */
    int gas_failed;       /* GAS rejected the line — investigate */
    int total;
    int diag_prints;      /* limits verbose dumps */
} Stats;

static void hexdump(FILE *f, const uint8_t *b, size_t n) {
    for (size_t i = 0; i < n; i++) fprintf(f, "%02x ", b[i]);
    if (n == 0) fprintf(f, "(empty)");
}

static void diagMismatch(Stats *s, const char *line, size_t len,
                         const uint8_t *enc, size_t enc_len,
                         const uint8_t *gas, size_t gas_len) {
    if (s->diag_prints >= MAX_DIAG_PRINTS) return;
    s->diag_prints++;
    fprintf(stderr, "    MISMATCH: %.*s\n", (int)len, line);
    fprintf(stderr, "      encoder: "); hexdump(stderr, enc, enc_len);
    fprintf(stderr, "\n      gas:     "); hexdump(stderr, gas, gas_len);
    fprintf(stderr, "\n");
}

/* --- Per-line driver ----------------------------------------------------- */

static void runOnLine(const char *line, size_t len, Stats *s) {
    LineKind k = classifyLine(line, len);
    s->counts[k]++;
    s->total++;
    if (k != LINE_INST) return;

    uint8_t enc[16];
    size_t  enc_len = 0;
    int rc = asm_encode(line, len, enc, sizeof(enc), &enc_len);

    if (rc == AS_E_UNKNOWN)        { s->enc_unknown++;   return; }
    if (rc == AS_E_MALFORMED)      { s->enc_malformed++; return; }
    if (rc == AS_E_NOSPACE)        { s->enc_nospace++;   return; }
    if (rc != AS_OK)               { s->enc_other++;     return; }

    /* Encoder accepted. Get GAS ground truth and compare. */
    uint8_t gas[64];
    size_t  gas_len = 0;
    if (gasAssemble(line, len, gas, sizeof(gas), &gas_len) != 0) {
        s->gas_failed++;
        if (s->diag_prints < MAX_DIAG_PRINTS) {
            s->diag_prints++;
            fprintf(stderr, "    GAS-FAILED: %.*s\n", (int)len, line);
        }
        return;
    }

    if (enc_len != gas_len || memcmp(enc, gas, enc_len) != 0) {
        s->enc_mismatch++;
        diagMismatch(s, line, len, enc, enc_len, gas, gas_len);
        return;
    }

    s->enc_ok++;
}

static int runCorpus(const char *path, Stats *s) {
    FILE *f = fopen(path, "r");
    if (!f) {
        fprintf(stderr, "asm-test: open %s: %s\n", path, strerror(errno));
        return -1;
    }

    char line[1024];
    while (fgets(line, sizeof(line), f)) {
        size_t len = strlen(line);
        while (len > 0 && (line[len - 1] == '\n' || line[len - 1] == '\r'))
            line[--len] = '\0';
        runOnLine(line, len, s);
    }
    fclose(f);
    return 0;
}

/* --- Reporting ----------------------------------------------------------- */

static int report(const Stats *s) {
    int instructions = s->counts[LINE_INST];

    printf("    %-16s %4d\n", "blank",          s->counts[LINE_BLANK]);
    printf("    %-16s %4d\n", "comment",        s->counts[LINE_COMMENT]);
    printf("    %-16s %4d\n", "label",          s->counts[LINE_LABEL]);
    printf("    %-16s %4d\n", "directive",      s->counts[LINE_DIRECTIVE]);
    printf("    %-16s %4d\n", "directive-data", s->counts[LINE_DIRECTIVE_DATA]);
    printf("    %-16s %4d\n", "instruction",    instructions);
    printf("    encoded         %4d / %d (matched GAS byte-for-byte)\n",
           s->enc_ok, instructions);
    printf("    unknown         %4d / %d (mnemonic not yet covered)\n",
           s->enc_unknown, instructions);

    int regressions = s->enc_mismatch + s->enc_malformed +
                      s->enc_nospace  + s->enc_other     + s->gas_failed;
    if (regressions == 0) return 0;

    if (s->enc_mismatch)
        printf("    mismatch        %4d / %d *** REGRESSION ***\n",
               s->enc_mismatch, instructions);
    if (s->enc_malformed)
        printf("    malformed       %4d / %d *** REGRESSION ***\n",
               s->enc_malformed, instructions);
    if (s->enc_nospace)
        printf("    nospace         %4d / %d *** REGRESSION ***\n",
               s->enc_nospace, instructions);
    if (s->enc_other)
        printf("    other-rc        %4d / %d *** REGRESSION ***\n",
               s->enc_other, instructions);
    if (s->gas_failed)
        printf("    gas-failed      %4d / %d *** investigate ***\n",
               s->gas_failed, instructions);
    return 1;
}

int main(int argc, char **argv) {
    if (argc < 2) {
        fprintf(stderr, "usage: %s <corpus.s> [<corpus.s> ...]\n", argv[0]);
        return 2;
    }

    Stats s;
    memset(&s, 0, sizeof(s));

    for (int i = 1; i < argc; i++) {
        if (runCorpus(argv[i], &s) < 0) return 1;
    }

    printf("==> asm-test: %d line(s) across %d corpus file(s)\n",
           s.total, argc - 1);
    return report(&s);
}
