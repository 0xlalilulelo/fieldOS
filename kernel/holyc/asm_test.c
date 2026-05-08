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
#include "jit.h"
#include "walker.h"

#ifndef AS_BIN
#define AS_BIN "as"
#endif
#ifndef OBJCOPY_BIN
#define OBJCOPY_BIN "objcopy"
#endif

/* Limit verbose mismatch dumps so a regression doesn't drown CI. */
#define MAX_DIAG_PRINTS 8

/* Per-line reloc cap. The corpus's most-relocating instruction
 * touches one symbol (call/je carry one operand; leaq sym(%rip)
 * also carries one). 4 is comfortable headroom for a future form
 * that emits two relocations per line. */
#define MAX_RELOCS_PER_LINE 4

/* Aggregated symbol slots. Bug_171.s currently hits 7 distinct
 * symbol names (_printf, _PrintMessage, .L0..L4 = 7 total). 32 is
 * comfortable for the next few corpus inputs without making the
 * harness allocate. Beyond 32, the breakdown elides — the total
 * count is still accurate. */
#define MAX_DISTINCT_SYMS 32

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

/* Two-axis encoded/unknown counters: the C-arc (C-1..C-3) closed the
 * 63 instruction lines and the D-arc (D-1..D-3) is closing the 6
 * directive-data lines. Tracking each kind separately preserves the
 * progress shape of both arcs; the report can collapse to a single
 * "emitting lines encoded" axis at M3-B step-4 exit. Regression
 * buckets (mismatch / malformed / nospace / other / gas_failed)
 * are shared because a regression is a regression regardless of
 * whether an instruction or a data directive triggered it. */
typedef struct {
    int counts[6];        /* by LineKind */
    int enc_inst_ok;      /* instruction encoded; bytes match GAS */
    int enc_inst_unknown; /* instruction AS_E_UNKNOWN — coverage gap */
    int enc_dd_ok;        /* directive-data encoded; bytes match GAS */
    int enc_dd_unknown;   /* directive-data AS_E_UNKNOWN — coverage gap */
    int enc_mismatch;     /* encoded; bytes disagree (REGRESSION) */
    int enc_malformed;    /* AS_E_MALFORMED (REGRESSION) */
    int enc_nospace;      /* AS_E_NOSPACE (REGRESSION) */
    int enc_other;        /* AS_E_TODO + any unexpected rc (REGRESSION) */
    int gas_failed;       /* GAS rejected the line — investigate */
    int total;
    int diag_prints;      /* limits verbose dumps */
    /* Relocation surface (5-1). reloc_total is the sum across all
     * encoded lines; sym_names/sym_counts hold the per-symbol
     * breakdown, with sym_overflow set if more than MAX_DISTINCT_SYMS
     * names were seen. Symbol names are copied into sym_names because
     * Reloc.sym points into per-line input that's reused on the next
     * fgets — without the copy, all entries would alias the line
     * buffer's last contents. */
    int  reloc_total;
    int  sym_count;
    char sym_names[MAX_DISTINCT_SYMS][64];
    int  sym_counts[MAX_DISTINCT_SYMS];
    int  sym_overflow;
} Stats;

static void recordSym(Stats *s, const char *sym, size_t sym_len) {
    if (sym_len >= sizeof(s->sym_names[0])) sym_len = sizeof(s->sym_names[0]) - 1;
    for (int i = 0; i < s->sym_count; i++) {
        if (strlen(s->sym_names[i]) == sym_len &&
            !memcmp(s->sym_names[i], sym, sym_len)) {
            s->sym_counts[i]++;
            return;
        }
    }
    if (s->sym_count >= MAX_DISTINCT_SYMS) {
        s->sym_overflow++;
        return;
    }
    memcpy(s->sym_names[s->sym_count], sym, sym_len);
    s->sym_names[s->sym_count][sym_len] = '\0';
    s->sym_counts[s->sym_count] = 1;
    s->sym_count++;
}

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
    if (k != LINE_INST && k != LINE_DIRECTIVE_DATA) return;

    /* enc[] sized for the longest emitting form in the corpus. C-arc
     * instructions are <=10 bytes; D-1's .quad is 8; D-2's .asciz
     * strings can be up to ~16 chars + NUL. 64 bytes is comfortable
     * across all three sub-rounds. */
    uint8_t enc[64];
    size_t  enc_len = 0;
    Reloc   relocs[MAX_RELOCS_PER_LINE];
    size_t  reloc_count = 0;
    int rc = asm_encode(line, len, enc, sizeof(enc), &enc_len,
                        relocs, MAX_RELOCS_PER_LINE, &reloc_count);

    if (rc == AS_E_UNKNOWN) {
        if (k == LINE_INST) s->enc_inst_unknown++;
        else                s->enc_dd_unknown++;
        return;
    }
    if (rc == AS_E_MALFORMED) { s->enc_malformed++; return; }
    if (rc == AS_E_NOSPACE)   { s->enc_nospace++;   return; }
    if (rc != AS_OK)          { s->enc_other++;     return; }

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

    if (k == LINE_INST) s->enc_inst_ok++;
    else                s->enc_dd_ok++;

    /* Reloc accounting: only fold relocations into the aggregate when
     * the line passed both encoder and GAS — a regression line's
     * relocations would otherwise inflate the total without a
     * meaningful coverage signal. */
    s->reloc_total += (int)reloc_count;
    for (size_t r = 0; r < reloc_count; r++) {
        recordSym(s, relocs[r].sym, relocs[r].sym_len);
    }
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

/* --- Pass-3 mock lookup harness (5-3e) -----------------------------------
 *
 * Verifies pass-3's resolution and rel32 patch math against synthetic
 * abi_table mocks that map _printf to a fixed virtual address (or to
 * nothing). The expected displacement formula is
 *
 *   disp = (int32_t)(target_va - (base_va + buf_offset + 4))
 *
 * with target_va = MOCK_PRINTF_VA and base_va = HOLYC_JIT_BASE. For
 * Bug_171.s specifically, all three deferred _printf relocs (at 73,
 * 154, 180) must end up with disp values matching this formula and
 * the unresolved count must be 0.
 *
 * The "lookup returns zero" mock exercises the 5-3d unresolved-policy
 * path: every extern reports unresolved, no rel32 is patched, the
 * buffer's zero-fill at extern sites is preserved. */

static const char *baseName(const char *path);

#define MOCK_PRINTF_VA 0x0000000010000000ULL

static uint64_t mock_lookup_printf(const char *name, size_t name_len) {
    if (name_len == 7 && !memcmp(name, "_printf", 7)) {
        return MOCK_PRINTF_VA;
    }
    return 0;
}

static uint64_t mock_lookup_none(const char *name, size_t name_len) {
    (void)name; (void)name_len;
    return 0;
}

static int32_t readRel32(const unsigned char *p) {
    return (int32_t)(
        (uint32_t)p[0]
        | ((uint32_t)p[1] << 8)
        | ((uint32_t)p[2] << 16)
        | ((uint32_t)p[3] << 24));
}

static int runPass3(const HolycExternTable *externs,
                    const unsigned char *baseline,
                    size_t out_len,
                    const char *path) {
    int regressions = 0;

    /* Mock 1: _printf -> MOCK_PRINTF_VA. */
    unsigned char *buf = malloc(out_len);
    if (!buf) return -1;
    memcpy(buf, baseline, out_len);

    size_t resolved = 0, unresolved = 0;
    int rc = holyc_walker_pass3(externs, buf, out_len,
                                HOLYC_JIT_BASE, mock_lookup_printf,
                                &resolved, &unresolved);
    if (rc != AS_OK) {
        fprintf(stderr, "    pass3: rc=%d *** REGRESSION ***\n", rc);
        free(buf);
        return 1;
    }

    printf("    pass3           %zu resolved, %zu unresolved "
           "(mock _printf -> 0x%llx)\n",
           resolved, unresolved, (unsigned long long)MOCK_PRINTF_VA);

    /* Verify each rel32 the mock resolved matches the formula. */
    for (size_t i = 0; i < externs->count; i++) {
        const HolycExternReloc *e = &externs->entries[i];
        if (mock_lookup_printf(e->sym, e->sym_len) == 0) continue;

        uint64_t patch_va = HOLYC_JIT_BASE + e->buf_offset;
        int64_t  disp64   = (int64_t)MOCK_PRINTF_VA - (int64_t)(patch_va + 4);
        int32_t  expected = (int32_t)disp64;
        int32_t  actual   = readRel32(buf + e->buf_offset);

        if (actual != expected) {
            fprintf(stderr,
                    "    pass3: rel32 at %zu mismatch "
                    "(expected 0x%08x, got 0x%08x) *** REGRESSION ***\n",
                    e->buf_offset, (unsigned)expected, (unsigned)actual);
            regressions++;
        }
    }

    /* Bug_171.s canonical: 3 resolved (_printf x3), 0 unresolved. */
    if (!strcmp(baseName(path), "Bug_171.s")) {
        if (resolved != 3) {
            fprintf(stderr,
                    "    pass3: expected 3 resolved, got %zu "
                    "*** REGRESSION ***\n", resolved);
            regressions++;
        }
        if (unresolved != 0) {
            fprintf(stderr,
                    "    pass3: expected 0 unresolved, got %zu "
                    "*** REGRESSION ***\n", unresolved);
            regressions++;
        }
    }

    free(buf);

    /* Mock 2: lookup-returns-zero. Exercises the 5-3d unresolved-policy
     * path: pass-3 must report every extern as unresolved, leave
     * every rel32 site at zero, and never patch. */
    buf = malloc(out_len);
    if (!buf) return -1;
    memcpy(buf, baseline, out_len);

    resolved = 0; unresolved = 0;
    rc = holyc_walker_pass3(externs, buf, out_len,
                            HOLYC_JIT_BASE, mock_lookup_none,
                            &resolved, &unresolved);
    if (rc != AS_OK) {
        fprintf(stderr, "    pass3 (none): rc=%d *** REGRESSION ***\n", rc);
        free(buf);
        return 1;
    }

    printf("    pass3 (none)    %zu resolved, %zu unresolved "
           "(mock returns 0)\n", resolved, unresolved);

    if (resolved != 0 || unresolved != externs->count) {
        fprintf(stderr,
                "    pass3 (none): expected 0/%zu, got %zu/%zu "
                "*** REGRESSION ***\n",
                externs->count, resolved, unresolved);
        regressions++;
    }
    for (size_t i = 0; i < externs->count; i++) {
        size_t off = externs->entries[i].buf_offset;
        if (buf[off] | buf[off + 1] | buf[off + 2] | buf[off + 3]) {
            fprintf(stderr,
                    "    pass3 (none): rel32 at %zu was patched "
                    "(expected zero) *** REGRESSION ***\n", off);
            regressions++;
        }
    }

    free(buf);
    return regressions;
}

/* --- Pass-1 walker harness (5-2b) ----------------------------------------
 *
 * Slurps the whole corpus file into one buffer and runs
 * holyc_walker_pass1 against it, mirroring what eval.c does on the
 * AoStr compileToAsm returns at boot. Reports the cumulative byte
 * count + label table; for Bug_171.s specifically (the only corpus
 * input today) asserts the canonical label set the kickoff calls
 * out: _FN, _PrintMessage, _main, .L0..L4, sign_bit, one_dbl. A
 * missing canonical label is a regression and fails the harness. */

/* Forward-declared above runPass3 (5-3e) so the path-canonical
 * Bug_171.s pass-3 assertions can resolve before this definition. */
static const char *baseName(const char *path) {
    const char *slash = strrchr(path, '/');
    return slash ? slash + 1 : path;
}

static int labelTablesContains(const HolycLabelTable *t,
                               const char *name) {
    size_t nlen = strlen(name);
    for (size_t i = 0; i < t->count; i++) {
        if (t->entries[i].name_len == nlen &&
            !memcmp(t->entries[i].name, name, nlen)) {
            return 1;
        }
    }
    return 0;
}

static int runWalker(const char *path) {
    FILE *f = fopen(path, "rb");
    if (!f) return -1;
    fseek(f, 0, SEEK_END);
    long sz = ftell(f);
    fseek(f, 0, SEEK_SET);
    char *buf = malloc((size_t)sz + 1);
    if (!buf) { fclose(f); return -1; }
    size_t got = fread(buf, 1, (size_t)sz, f);
    fclose(f);
    buf[got] = '\0';

    HolycLabelTable labels;
    size_t total = 0;
    int rc = holyc_walker_pass1(buf, got, &labels, &total);
    if (rc != 0) {
        fprintf(stderr, "    walker: rc=%d *** REGRESSION ***\n", rc);
        free(buf);
        return 1;
    }

    printf("    walker          %4zu bytes, %zu label(s)%s\n",
           total, labels.count,
           labels.overflow ? " + overflow" : "");
    for (size_t i = 0; i < labels.count; i++) {
        printf("      %-22.*s @ %zu\n",
               (int)labels.entries[i].name_len,
               labels.entries[i].name,
               labels.entries[i].offset);
    }

    int regressions = 0;

    /* Bug_171.s canonical-label assertion. Other corpus paths skip
     * this check; their canonical sets land alongside their entries
     * if more inputs join. */
    if (!strcmp(baseName(path), "Bug_171.s")) {
        static const char *const expected[] = {
            "sign_bit", "one_dbl",
            ".L0", ".L1", ".L2", ".L3", ".L4",
            "_FN", "_PrintMessage", "_main",
            NULL,
        };
        for (int i = 0; expected[i] != NULL; i++) {
            if (!labelTablesContains(&labels, expected[i])) {
                fprintf(stderr,
                        "    walker: missing canonical label '%s' "
                        "*** REGRESSION ***\n",
                        expected[i]);
                regressions++;
            }
        }
    }

    /* 5-2c pass-2: emit + patch + defer externs. The byte buffer is
     * sized from pass-1's total; pass-2 fills it then we cross-check
     * each reloc against the label table independently to catch any
     * drift between pass-1's recording and pass-2's resolution. */
    unsigned char *out = malloc(total);
    if (!out) { free(buf); return -1; }
    HolycExternTable externs;
    size_t out_len = 0, local_patched = 0;
    int prc = holyc_walker_pass2(buf, got, &labels,
                                 out, total, &out_len,
                                 &externs, &local_patched);
    if (prc != 0) {
        fprintf(stderr, "    pass2:  rc=%d *** REGRESSION ***\n", prc);
        free(out); free(buf);
        return 1;
    }

    printf("    pass2           %4zu bytes, %zu local patched, "
           "%zu extern deferred%s\n",
           out_len, local_patched, externs.count,
           externs.overflow ? " + overflow" : "");
    for (size_t i = 0; i < externs.count; i++) {
        printf("      %-22s @ %zu\n",
               externs.entries[i].sym, externs.entries[i].buf_offset);
    }

    if (out_len != total) {
        fprintf(stderr, "    pass2: byte count drift pass1=%zu pass2=%zu "
                "*** REGRESSION ***\n", total, out_len);
        regressions++;
    }

    /* Verify every extern reloc has zero bytes at its site. */
    for (size_t i = 0; i < externs.count; i++) {
        size_t off = externs.entries[i].buf_offset;
        if (out[off] | out[off + 1] | out[off + 2] | out[off + 3]) {
            fprintf(stderr,
                    "    pass2: extern '%s' @ %zu not zero-filled "
                    "*** REGRESSION ***\n",
                    externs.entries[i].sym, off);
            regressions++;
        }
    }

    /* Bug_171.s canonical pass-2 split: 6 local relocs (.L0..L4 +
     * _PrintMessage) patched, 3 extern relocs (_printf x3) deferred.
     * Total 9 matches asm-test's per-line reloc count. */
    if (!strcmp(baseName(path), "Bug_171.s")) {
        if (local_patched != 6) {
            fprintf(stderr, "    pass2: expected 6 local patched, got %zu "
                    "*** REGRESSION ***\n", local_patched);
            regressions++;
        }
        if (externs.count != 3) {
            fprintf(stderr, "    pass2: expected 3 extern deferred, got %zu "
                    "*** REGRESSION ***\n", externs.count);
            regressions++;
        }
        for (size_t i = 0; i < externs.count; i++) {
            if (strcmp(externs.entries[i].sym, "_printf") != 0) {
                fprintf(stderr, "    pass2: extern[%zu] '%s' (expected _printf) "
                        "*** REGRESSION ***\n",
                        i, externs.entries[i].sym);
                regressions++;
            }
        }
    }

    /* 5-3e pass-3 mock-lookup harness. Run *after* pass-2 has filled
     * the buffer + recorded externs but *before* we free out, so the
     * baseline byte image (locals patched, externs zero-filled) is
     * what mock pass-3 invocations copy from. runPass3 owns its own
     * buffer copies so successive mock runs are independent. */
    int p3rc = runPass3(&externs, out, out_len, path);
    if (p3rc < 0) { free(out); free(buf); return -1; }
    regressions += p3rc;

    free(out);
    free(buf);
    return regressions ? 1 : 0;
}

/* --- Reporting ----------------------------------------------------------- */

static int report(const Stats *s) {
    int inst = s->counts[LINE_INST];
    int dd   = s->counts[LINE_DIRECTIVE_DATA];
    int emit = inst + dd;

    printf("    %-16s %4d\n", "blank",          s->counts[LINE_BLANK]);
    printf("    %-16s %4d\n", "comment",        s->counts[LINE_COMMENT]);
    printf("    %-16s %4d\n", "label",          s->counts[LINE_LABEL]);
    printf("    %-16s %4d\n", "directive",      s->counts[LINE_DIRECTIVE]);
    printf("    %-16s %4d\n", "directive-data", dd);
    printf("    %-16s %4d\n", "instruction",    inst);
    printf("    encoded inst    %4d / %d (matched GAS byte-for-byte)\n",
           s->enc_inst_ok, inst);
    printf("    encoded dd      %4d / %d (matched GAS byte-for-byte)\n",
           s->enc_dd_ok, dd);
    printf("    unknown inst    %4d / %d (mnemonic not yet covered)\n",
           s->enc_inst_unknown, inst);
    printf("    unknown dd      %4d / %d (directive not yet covered)\n",
           s->enc_dd_unknown, dd);
    printf("    relocations     %4d (%d distinct symbol%s%s)\n",
           s->reloc_total,
           s->sym_count,
           s->sym_count == 1 ? "" : "s",
           s->sym_overflow ? ", + overflow" : "");
    for (int i = 0; i < s->sym_count; i++) {
        printf("      %-22s %4d\n", s->sym_names[i], s->sym_counts[i]);
    }

    int regressions = s->enc_mismatch + s->enc_malformed +
                      s->enc_nospace  + s->enc_other     + s->gas_failed;
    if (regressions == 0) return 0;

    if (s->enc_mismatch)
        printf("    mismatch        %4d / %d *** REGRESSION ***\n",
               s->enc_mismatch, emit);
    if (s->enc_malformed)
        printf("    malformed       %4d / %d *** REGRESSION ***\n",
               s->enc_malformed, emit);
    if (s->enc_nospace)
        printf("    nospace         %4d / %d *** REGRESSION ***\n",
               s->enc_nospace, emit);
    if (s->enc_other)
        printf("    other-rc        %4d / %d *** REGRESSION ***\n",
               s->enc_other, emit);
    if (s->gas_failed)
        printf("    gas-failed      %4d / %d *** investigate ***\n",
               s->gas_failed, emit);
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
    int rc = report(&s);

    /* Walker pass — runs after the per-line encoder report so the
     * existing parity numbers stay first in the output stream. */
    for (int i = 1; i < argc; i++) {
        printf("==> walker: %s\n", argv[i]);
        int wrc = runWalker(argv[i]);
        if (wrc != 0) {
            rc = 1;
        }
    }
    return rc;
}
