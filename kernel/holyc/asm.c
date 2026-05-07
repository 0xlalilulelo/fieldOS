/* kernel/holyc/asm.c
 *
 * In-tree x86_64 encoder. Per ADR-0001 §2 / ADR-0003 §1 — keeps
 * holyc/src/x86.c untouched and consumes the AT&T-text AoStr that
 * compileToAsm() returns. The host harness at kernel/holyc/asm_test.c
 * drives this entry point against the checked-in corpus under
 * holyc/tests/corpus/ and compares output to $(CROSS_AS) byte-for-byte.
 *
 * C-1 covers the smallest validating cluster: zero-operand
 * instructions (ret, leave) and single-register-operand instructions
 * (push reg64, pop reg64). Subsequent commits extend this dispatch
 * table with mov / lea / sub / test / je / call clusters until the
 * encoder matches GAS for every instruction line in Bug_171.s.
 *
 * No libc dependency. Compiles under both the host build (via the
 * asm-test rule) and the kernel build (-ffreestanding -mno-sse).
 * Local helpers (myWS, myStrlen, myStrncmp) cover the small string
 * surface this file needs; pulling in <string.h> would split the
 * two builds.
 */

#include <stddef.h>
#include <stdint.h>

#include "asm.h"

/* --- REX prefix bits ----------------------------------------------------- */

#define REX_BASE  0x40
#define REX_W     0x08    /* 64-bit operand size */
#define REX_R     0x04    /* extends ModR/M.reg */
#define REX_X     0x02    /* extends SIB.index  */
#define REX_B     0x01    /* extends ModR/M.rm / SIB.base / opcode reg */

/* --- Local string helpers (no libc) -------------------------------------- */

static int myWS(char c) { return c == ' ' || c == '\t'; }

static size_t myStrlen(const char *s) {
    size_t n = 0;
    while (s[n]) n++;
    return n;
}

static int myStrncmp(const char *a, const char *b, size_t n) {
    for (size_t i = 0; i < n; i++) {
        unsigned char ca = (unsigned char)a[i];
        unsigned char cb = (unsigned char)b[i];
        if (ca != cb) return (int)ca - (int)cb;
        if (ca == 0)  return 0;
    }
    return 0;
}

/* --- Register table — 64-bit GPRs only for C-1 -------------------------- */

typedef struct { const char *name; uint8_t rn; } Reg;

static const Reg kRegs64[] = {
    { "rax",  0 }, { "rcx",  1 }, { "rdx",  2 }, { "rbx",  3 },
    { "rsp",  4 }, { "rbp",  5 }, { "rsi",  6 }, { "rdi",  7 },
    { "r8",   8 }, { "r9",   9 }, { "r10", 10 }, { "r11", 11 },
    { "r12", 12 }, { "r13", 13 }, { "r14", 14 }, { "r15", 15 },
};

static int regLookup64(const char *s, size_t n) {
    for (size_t i = 0; i < sizeof(kRegs64) / sizeof(kRegs64[0]); i++) {
        size_t nl = myStrlen(kRegs64[i].name);
        if (nl == n && !myStrncmp(s, kRegs64[i].name, n)) {
            return kRegs64[i].rn;
        }
    }
    return -1;
}

/* --- Output buffer ------------------------------------------------------- */

typedef struct {
    uint8_t *p;
    size_t   cap;
    size_t   len;
    int      ok;     /* set to 0 on overflow */
} OutBuf;

static void outByte(OutBuf *o, uint8_t b) {
    if (!o->ok) return;
    if (o->len >= o->cap) { o->ok = 0; return; }
    o->p[o->len++] = b;
}

/* --- Operand parsing (C-1: registers only) ------------------------------ */

typedef enum {
    OPR_REG,
} OprKind;

typedef struct {
    OprKind kind;
    int     reg;     /* 0..15 for OPR_REG */
} Oprnd;

/* Trim leading and trailing whitespace; advance *psrc past leading
 * WS and return the new length. */
static size_t trimWS(const char **psrc, size_t len) {
    const char *s = *psrc;
    size_t i = 0;
    while (i < len && myWS(s[i])) i++;
    s += i;
    len -= i;
    while (len > 0 && myWS(s[len - 1])) len--;
    *psrc = s;
    return len;
}

/* Strip a trailing GAS comment. Returns new length. */
static size_t stripTrailingComment(const char *s, size_t len) {
    for (size_t i = 0; i < len; i++) {
        if (s[i] == '#') return i;
    }
    return len;
}

/* Parse one register operand: "%name". Returns AS_OK on success. */
static int parseRegOperand(const char *s, size_t n, Oprnd *out) {
    n = trimWS(&s, n);
    if (n < 2 || s[0] != '%') return AS_E_MALFORMED;
    int rn = regLookup64(s + 1, n - 1);
    if (rn < 0) return AS_E_MALFORMED;
    out->kind = OPR_REG;
    out->reg  = rn;
    return AS_OK;
}

/* --- Encoders ------------------------------------------------------------ */

static int encRet(const char *opers, size_t n, OutBuf *out) {
    n = trimWS(&opers, n);
    if (n != 0) return AS_E_MALFORMED;
    outByte(out, 0xC3);
    return out->ok ? AS_OK : AS_E_NOSPACE;
}

static int encLeave(const char *opers, size_t n, OutBuf *out) {
    n = trimWS(&opers, n);
    if (n != 0) return AS_E_MALFORMED;
    outByte(out, 0xC9);
    return out->ok ? AS_OK : AS_E_NOSPACE;
}

/* push reg64. Default operand size is 64 in long mode; no REX.W
 * needed. REX.B if rn>=8. */
static int encPush(const char *opers, size_t n, OutBuf *out) {
    Oprnd r;
    int rc = parseRegOperand(opers, n, &r);
    if (rc != AS_OK) return rc;
    if (r.reg >= 8) outByte(out, REX_BASE | REX_B);
    outByte(out, 0x50 | (r.reg & 7));
    return out->ok ? AS_OK : AS_E_NOSPACE;
}

static int encPop(const char *opers, size_t n, OutBuf *out) {
    Oprnd r;
    int rc = parseRegOperand(opers, n, &r);
    if (rc != AS_OK) return rc;
    if (r.reg >= 8) outByte(out, REX_BASE | REX_B);
    outByte(out, 0x58 | (r.reg & 7));
    return out->ok ? AS_OK : AS_E_NOSPACE;
}

/* --- Dispatch ----------------------------------------------------------- */

typedef int (*EncoderFn)(const char *opers, size_t n, OutBuf *out);

typedef struct {
    const char *mnemonic;
    EncoderFn   fn;
} Dispatch;

static const Dispatch kDispatch[] = {
    { "ret",   encRet   },
    { "leave", encLeave },
    { "push",  encPush  },
    { "pop",   encPop   },
};

/* Re-classify here so asm_encode is self-contained — the harness's
 * own classifier exists for reporting; the encoder must not trust
 * it for correctness. Lines that emit zero bytes (label, comment,
 * non-data directive, blank) succeed with *out_len == 0. Data
 * directives (.quad et al.) currently fall through to instruction
 * dispatch and AS_E_UNKNOWN; D adds a directive-side branch. */
static int isNonEmittingLine(const char *s, size_t n) {
    size_t i = 0;
    while (i < n && myWS(s[i])) i++;
    if (i == n) return 1;
    if (s[i] == '#') return 1;
    if (s[i] == '.') {
        /* Distinguish data directives from non-emitting ones. For
         * C-1, every directive is treated as non-emitting; D will
         * route .quad/.asciz/.double to a separate dispatch.
         * Conservative for now: even data directives return zero
         * bytes here, so the harness reports them as "encoded"
         * with empty output — which mismatches GAS for .quad et al.
         * That mismatch is the bar D lifts. */
        return 1;
    }
    size_t j = n;
    while (j > i && myWS(s[j - 1])) j--;
    if (j > i && s[j - 1] == ':') return 1;
    return 0;
}

int asm_encode(const char *att_line, size_t line_len,
               uint8_t *out, size_t out_cap, size_t *out_len) {
    if (out_len) *out_len = 0;

    if (isNonEmittingLine(att_line, line_len)) {
        return AS_OK;
    }

    /* Strip a trailing GAS comment before mnemonic split. */
    line_len = stripTrailingComment(att_line, line_len);

    /* Read mnemonic. */
    size_t i = 0;
    while (i < line_len && myWS(att_line[i])) i++;
    size_t mn_start = i;
    while (i < line_len && !myWS(att_line[i])) i++;
    size_t mn_len = i - mn_start;
    if (mn_len == 0) return AS_E_MALFORMED;

    /* Operand region (may be empty). */
    while (i < line_len && myWS(att_line[i])) i++;
    const char *opers = att_line + i;
    size_t opers_len = line_len - i;

    /* Verbatim mnemonic match. Suffix-bearing forms (movq, leaq,
     * subq) join the table in C-2/C-3 with their suffix in place;
     * GAS treats `push` and `pushq` as synonyms but the corpus only
     * uses the bare forms for C-1's surface. */
    for (size_t k = 0; k < sizeof(kDispatch) / sizeof(kDispatch[0]); k++) {
        size_t dnl = myStrlen(kDispatch[k].mnemonic);
        if (dnl == mn_len &&
            !myStrncmp(att_line + mn_start, kDispatch[k].mnemonic, dnl)) {
            OutBuf ob = { out, out_cap, 0, 1 };
            int rc = kDispatch[k].fn(opers, opers_len, &ob);
            if (out_len) *out_len = ob.len;
            return rc;
        }
    }

    /* Mnemonic not in dispatch table. Distinct from AS_E_TODO; that
     * v0-stub return is no longer reachable after C-1. AS_E_UNKNOWN
     * is the "encoder coverage gap, not regression" signal that
     * later cluster commits close. */
    return AS_E_UNKNOWN;
}
