/* kernel/holyc/asm.c
 *
 * In-tree x86_64 encoder. Per ADR-0001 §2 / ADR-0003 §1 — keeps
 * holyc/src/x86.c untouched and consumes the AT&T-text AoStr that
 * compileToAsm() returns. The host harness at kernel/holyc/asm_test.c
 * drives this entry point against the checked-in corpus under
 * holyc/tests/corpus/ and compares output to $(CROSS_AS) byte-for-byte.
 *
 * Coverage so far:
 *   C-1: ret, leave, push reg64, pop reg64               (33/63 lines)
 *   C-2: movq (4 forms), leaq (mem-reg, RIP-rel)         (+22 lines)
 *   C-3: subq imm-reg, test reg-reg, je, call rel/indir  (later)
 *   D:   data directives (.quad, .asciz, .double)        (later)
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

/* --- Operand parsing ----------------------------------------------------- */

typedef enum {
    OPR_REG,        /* %rax                                */
    OPR_IMM,        /* $42 / $0x1f / $-8                    */
    OPR_MEM,        /* disp(%base) — disp may be 0          */
    OPR_MEM_RIP,    /* label(%rip) — symbol; rel32=0 stays  */
} OprKind;

typedef struct {
    OprKind kind;
    int     reg;     /* OPR_REG: register; OPR_MEM: base reg */
    int64_t imm;     /* OPR_IMM                              */
    int64_t disp;    /* OPR_MEM (zero if absent)             */
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

/* Parse one register operand: "%name". */
static int parseRegOperand(const char *s, size_t n, Oprnd *out) {
    n = trimWS(&s, n);
    if (n < 2 || s[0] != '%') return AS_E_MALFORMED;
    int rn = regLookup64(s + 1, n - 1);
    if (rn < 0) return AS_E_MALFORMED;
    out->kind = OPR_REG;
    out->reg  = rn;
    return AS_OK;
}

/* Parse a signed integer in decimal or 0x-hex. Used by both
 * immediate operands (after `$`) and memory displacements. */
static int parseSignedInt(const char *s, size_t n, int64_t *out) {
    if (n == 0) return AS_E_MALFORMED;
    int64_t sign = 1;
    if (s[0] == '-')      { sign = -1; s++; n--; }
    else if (s[0] == '+') {             s++; n--; }
    int base = 10;
    if (n >= 2 && s[0] == '0' && (s[1] == 'x' || s[1] == 'X')) {
        base = 16; s += 2; n -= 2;
    }
    if (n == 0) return AS_E_MALFORMED;
    int64_t v = 0;
    for (size_t i = 0; i < n; i++) {
        int d = -1;
        char c = s[i];
        if      (c >= '0' && c <= '9')              d = c - '0';
        else if (base == 16 && c >= 'a' && c <= 'f') d = c - 'a' + 10;
        else if (base == 16 && c >= 'A' && c <= 'F') d = c - 'A' + 10;
        else return AS_E_MALFORMED;
        if (d >= base) return AS_E_MALFORMED;
        v = v * base + d;
    }
    *out = sign * v;
    return AS_OK;
}

/* Parse `$<num>`. */
static int parseImmOperand(const char *s, size_t n, Oprnd *out) {
    n = trimWS(&s, n);
    if (n < 2 || s[0] != '$') return AS_E_MALFORMED;
    s++; n--;
    n = trimWS(&s, n);
    int64_t v = 0;
    int rc = parseSignedInt(s, n, &v);
    if (rc != AS_OK) return rc;
    out->kind = OPR_IMM;
    out->imm  = v;
    return AS_OK;
}

/* Parse `[disp](%base)` or `label(%rip)`. The disp portion is
 * optional (defaults to 0). For the RIP form, the label part is
 * not stored — the encoder emits a zero-filled rel32 (relocation
 * site) which matches GAS's pre-link bytes. */
static int parseMemOperand(const char *s, size_t n, Oprnd *out) {
    n = trimWS(&s, n);
    /* Locate '(' that opens the base-register expression. */
    size_t paren = 0;
    while (paren < n && s[paren] != '(') paren++;
    if (paren == n) return AS_E_MALFORMED;
    /* Locate matching ')'. C-2 has no nested parens in operands. */
    size_t close = paren + 1;
    while (close < n && s[close] != ')') close++;
    if (close == n) return AS_E_MALFORMED;
    /* Anything after ')' must be whitespace only. */
    for (size_t i = close + 1; i < n; i++) {
        if (!myWS(s[i])) return AS_E_MALFORMED;
    }
    /* Inside parens: %name. */
    const char *inner = s + paren + 1;
    size_t inner_len = close - paren - 1;
    inner_len = trimWS(&inner, inner_len);
    if (inner_len < 2 || inner[0] != '%') return AS_E_MALFORMED;
    /* %rip is special — RIP-relative addressing. */
    if (inner_len == 4 &&
        inner[1] == 'r' && inner[2] == 'i' && inner[3] == 'p') {
        out->kind = OPR_MEM_RIP;
        return AS_OK;
    }
    int rn = regLookup64(inner + 1, inner_len - 1);
    if (rn < 0) return AS_E_MALFORMED;
    /* Disp portion: empty means 0; otherwise a signed integer. */
    int64_t disp = 0;
    const char *dp = s;
    size_t dlen = paren;
    while (dlen > 0 && myWS(dp[dlen - 1])) dlen--;
    dlen = trimWS(&dp, dlen);
    if (dlen > 0) {
        int rc = parseSignedInt(dp, dlen, &disp);
        if (rc != AS_OK) return rc;
    }
    out->kind = OPR_MEM;
    out->reg  = rn;
    out->disp = disp;
    return AS_OK;
}

/* Top-level operand parser — dispatches by first non-WS character. */
static int parseOperand(const char *s, size_t n, Oprnd *out) {
    n = trimWS(&s, n);
    if (n == 0)         return AS_E_MALFORMED;
    if (s[0] == '%')    return parseRegOperand(s, n, out);
    if (s[0] == '$')    return parseImmOperand(s, n, out);
    return parseMemOperand(s, n, out);
}

/* Split operand region at the first top-level comma (not inside
 * parens) and parse both sides. */
static int parseTwoOperands(const char *s, size_t n, Oprnd *a, Oprnd *b) {
    int    depth = 0;
    size_t comma = (size_t)-1;
    for (size_t i = 0; i < n; i++) {
        if      (s[i] == '(') depth++;
        else if (s[i] == ')') depth--;
        else if (s[i] == ',' && depth == 0) { comma = i; break; }
    }
    if (comma == (size_t)-1) return AS_E_MALFORMED;
    int rc = parseOperand(s, comma, a);
    if (rc != AS_OK) return rc;
    return parseOperand(s + comma + 1, n - comma - 1, b);
}

/* --- Memory-operand encoding (ModR/M + SIB + disp) ----------------------- */

/* Emit the ModR/M (and SIB and disp bytes) for a memory operand
 * with a given reg-field value (the "r" half of ModR/M, lower 3 bits
 * of the encoder's reg/opcode-extension argument).
 *
 * Special cases (Intel SDM Vol. 2 §2.1.5):
 *   - rbp(5) / r13(13) (rm field == 5) cannot use mod=00 — that
 *     encoding means RIP-relative. Force mod=01 disp8=0 if disp==0.
 *   - rsp(4) / r12(12) (rm field == 4) require a SIB byte, because
 *     rm=100 with mod≠11 means "SIB follows". We emit SIB = (scale=0,
 *     index=4 [none], base=base&7).
 *   - OPR_MEM_RIP: mod=00, rm=101, rel32=0 (relocation will fill it). */
static int emitMem(OutBuf *o, int reg_field, const Oprnd *mem) {
    if (mem->kind == OPR_MEM_RIP) {
        outByte(o, ((reg_field & 7) << 3) | 5);
        outByte(o, 0); outByte(o, 0); outByte(o, 0); outByte(o, 0);
        return o->ok ? AS_OK : AS_E_NOSPACE;
    }
    int     base = mem->reg;
    int64_t disp = mem->disp;
    int     mod;
    int     disp_bytes;
    if (disp == 0 && (base & 7) != 5) {
        mod = 0; disp_bytes = 0;
    } else if (disp >= -128 && disp <= 127) {
        mod = 1; disp_bytes = 1;
    } else if (disp >= INT32_MIN && disp <= INT32_MAX) {
        mod = 2; disp_bytes = 4;
    } else {
        return AS_E_MALFORMED;
    }
    if ((base & 7) == 4) {
        /* SIB required for rm-field == 4. */
        outByte(o, (mod << 6) | ((reg_field & 7) << 3) | 4);
        outByte(o, (0 << 6) | (4 << 3) | (base & 7));
    } else {
        outByte(o, (mod << 6) | ((reg_field & 7) << 3) | (base & 7));
    }
    if (disp_bytes == 1) {
        outByte(o, (uint8_t)(disp & 0xFF));
    } else if (disp_bytes == 4) {
        outByte(o, (uint8_t)(disp        & 0xFF));
        outByte(o, (uint8_t)((disp >> 8) & 0xFF));
        outByte(o, (uint8_t)((disp >> 16) & 0xFF));
        outByte(o, (uint8_t)((disp >> 24) & 0xFF));
    }
    return o->ok ? AS_OK : AS_E_NOSPACE;
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

/* movq — four forms in the corpus:
 *   reg, reg     opcode 89  ModR/M(11, src, dst)
 *   mem, reg     opcode 8B  ModR/M(mod, dst, base)  [+SIB+disp]
 *   reg, mem     opcode 89  ModR/M(mod, src, base)  [+SIB+disp]
 *   imm32, reg   opcode C7 /0  ModR/M(11, 0, dst)  imm32 sign-extended
 *
 * All forms emit REX.W (0x48-base). The reg-reg case takes REX.R for
 * a high src and REX.B for a high dst; the mem cases take REX.R for
 * a high reg-side and REX.B for a high base. The imm-reg case takes
 * REX.B for a high dst.
 *
 * Full 64-bit immediates that don't fit signed-32 would route to
 * 0xB8+rn imm64; the corpus has none and the encoder returns
 * AS_E_UNKNOWN to surface the gap if a future input has one. */
static int encMovq(const char *opers, size_t n, OutBuf *out) {
    Oprnd a, b;
    int rc = parseTwoOperands(opers, n, &a, &b);
    if (rc != AS_OK) return rc;

    if (a.kind == OPR_REG && b.kind == OPR_REG) {
        uint8_t rex = REX_BASE | REX_W;
        if (a.reg >= 8) rex |= REX_R;
        if (b.reg >= 8) rex |= REX_B;
        outByte(out, rex);
        outByte(out, 0x89);
        outByte(out, (3 << 6) | ((a.reg & 7) << 3) | (b.reg & 7));
        return out->ok ? AS_OK : AS_E_NOSPACE;
    }
    if ((a.kind == OPR_MEM || a.kind == OPR_MEM_RIP) && b.kind == OPR_REG) {
        uint8_t rex = REX_BASE | REX_W;
        if (b.reg >= 8) rex |= REX_R;
        if (a.kind == OPR_MEM && a.reg >= 8) rex |= REX_B;
        outByte(out, rex);
        outByte(out, 0x8B);
        return emitMem(out, b.reg, &a);
    }
    if (a.kind == OPR_REG && (b.kind == OPR_MEM || b.kind == OPR_MEM_RIP)) {
        uint8_t rex = REX_BASE | REX_W;
        if (a.reg >= 8) rex |= REX_R;
        if (b.kind == OPR_MEM && b.reg >= 8) rex |= REX_B;
        outByte(out, rex);
        outByte(out, 0x89);
        return emitMem(out, a.reg, &b);
    }
    if (a.kind == OPR_IMM && b.kind == OPR_REG) {
        if (a.imm < INT32_MIN || a.imm > INT32_MAX) return AS_E_UNKNOWN;
        uint8_t rex = REX_BASE | REX_W;
        if (b.reg >= 8) rex |= REX_B;
        outByte(out, rex);
        outByte(out, 0xC7);
        outByte(out, (3 << 6) | (0 << 3) | (b.reg & 7));
        int32_t v = (int32_t)a.imm;
        outByte(out, (uint8_t)(v        & 0xFF));
        outByte(out, (uint8_t)((v >> 8) & 0xFF));
        outByte(out, (uint8_t)((v >> 16) & 0xFF));
        outByte(out, (uint8_t)((v >> 24) & 0xFF));
        return out->ok ? AS_OK : AS_E_NOSPACE;
    }
    return AS_E_UNKNOWN;
}

/* leaq — two forms in the corpus:
 *   disp(%base), reg     opcode 8D  ModR/M(mod, dst, base) [+SIB+disp]
 *   label(%rip), reg     opcode 8D  ModR/M(00, dst, 5)     rel32=0
 *
 * REX.W always; REX.R for a high dst; REX.B for a high base. */
static int encLeaq(const char *opers, size_t n, OutBuf *out) {
    Oprnd a, b;
    int rc = parseTwoOperands(opers, n, &a, &b);
    if (rc != AS_OK) return rc;
    if (b.kind != OPR_REG) return AS_E_MALFORMED;
    if (a.kind != OPR_MEM && a.kind != OPR_MEM_RIP) return AS_E_MALFORMED;

    uint8_t rex = REX_BASE | REX_W;
    if (b.reg >= 8) rex |= REX_R;
    if (a.kind == OPR_MEM && a.reg >= 8) rex |= REX_B;
    outByte(out, rex);
    outByte(out, 0x8D);
    return emitMem(out, b.reg, &a);
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
    { "movq",  encMovq  },
    { "leaq",  encLeaq  },
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
