/* kernel/holyc/asm_test.c
 *
 * Host harness for the in-tree x86_64 encoder. Reads a corpus file
 * (typically holyc/tests/corpus/Bug_171.s, produced by `make corpus`
 * per ADR-0003 §2), classifies each line, calls asm_encode() on each
 * instruction line, and reports the v0 baseline.
 *
 * v0 contract (B): the encoder stub returns AS_E_TODO for every
 * line. The harness reports counts per line kind and per encoder
 * return code so the baseline is "K instruction lines; 0 encoded;
 * K stubbed (AS_E_TODO)". Each round of (C)/(D) coverage flips a
 * subset of those K lines from stubbed to encoded, with a
 * byte-for-byte $(CROSS_AS) comparison landing alongside (C)'s first
 * cluster. The comparison is dead weight while the encoder returns
 * AS_E_TODO for everything; deferring it keeps B small and earns
 * its keep at the moment it can find real disagreement.
 *
 * Exit code: 0 if every instruction line met its expected outcome
 * (stubbed in v0; encoded matching $(CROSS_AS) once (C) lands), 1
 * otherwise. The expected-outcome bar lifts as encoder coverage
 * grows; the harness's binary-pass shape stays load-bearing for CI.
 *
 * Built host-side via the `asm-test` rule in holyc/holyc.mk. Not
 * linked into the kernel ELF — main() only, no kernel-side use. */

#include <ctype.h>
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

#include "asm.h"

/* Line classification. Drives both the harness's iteration and its
 * reporting. v0 only exercises encoder behaviour on LINE_INST; later
 * clusters extend the encoder to cover LINE_DIRECTIVE_DATA (.quad,
 * .asciz, .double) once instruction coverage closes. */
typedef enum {
    LINE_BLANK,
    LINE_COMMENT,
    LINE_LABEL,
    LINE_DIRECTIVE,       /* .text, .globl, .L0, etc. — non-emitting */
    LINE_DIRECTIVE_DATA,  /* .quad, .asciz, .double, .byte, .long, .short */
    LINE_INST,
} LineKind;

static const char *kindName(LineKind k) {
    switch (k) {
    case LINE_BLANK:           return "blank";
    case LINE_COMMENT:         return "comment";
    case LINE_LABEL:           return "label";
    case LINE_DIRECTIVE:       return "directive";
    case LINE_DIRECTIVE_DATA:  return "directive-data";
    case LINE_INST:            return "instruction";
    }
    return "?";
}

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
    /* Labels: any line whose last non-whitespace character is ':'. */
    size_t last = len;
    while (last > i && isspace((unsigned char)line[last - 1])) last--;
    if (last > i && line[last - 1] == ':') return LINE_LABEL;
    return LINE_INST;
}

typedef struct {
    int counts[6];      /* indexed by LineKind */
    int enc_ok;
    int enc_todo;
    int enc_other;      /* AS_E_UNKNOWN / AS_E_MALFORMED / AS_E_NOSPACE */
    int total;
} Stats;

static void runOnLine(const char *line, size_t len, Stats *s) {
    LineKind k = classifyLine(line, len);
    s->counts[k]++;
    s->total++;
    if (k != LINE_INST) return;

    uint8_t buf[16];
    size_t n = 0;
    int rc = asm_encode(line, len, buf, sizeof(buf), &n);
    if (rc == AS_OK)            s->enc_ok++;
    else if (rc == AS_E_TODO)   s->enc_todo++;
    else                        s->enc_other++;
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

    int instructions = s.counts[LINE_INST];
    printf("==> asm-test: %d line(s) across %d corpus file(s)\n",
           s.total, argc - 1);
    for (int k = 0; k <= LINE_INST; k++) {
        printf("    %-16s %4d\n", kindName((LineKind)k), s.counts[k]);
    }
    printf("    encoded         %4d / %d (instruction lines)\n",
           s.enc_ok, instructions);
    printf("    stubbed (TODO)  %4d / %d\n", s.enc_todo, instructions);
    if (s.enc_other) {
        printf("    other failure   %4d / %d  *** unexpected ***\n",
               s.enc_other, instructions);
        return 1;
    }

    /* v0 expected outcome: all instruction lines stubbed. As (C) and
     * (D) land, the bar shifts to "encoded == instructions". The
     * harness's exit code stays 0 as long as nothing returns
     * AS_E_UNKNOWN / AS_E_MALFORMED / AS_E_NOSPACE — those are real
     * regressions, not coverage gaps. */
    return 0;
}
