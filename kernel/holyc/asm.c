/* kernel/holyc/asm.c
 *
 * In-tree x86_64 encoder. Per ADR-0001 §2 / ADR-0003 §1 — keeps
 * holyc/src/x86.c untouched and consumes the AT&T-text AoStr that
 * compileToAsm() returns. The host harness at kernel/holyc/asm_test.c
 * drives this entry point against the checked-in corpus under
 * holyc/tests/corpus/ and compares output to $(CROSS_AS) byte-for-byte.
 *
 * Coverage so far:
 *   C-1: ret, leave, push reg64, pop reg64               (33/63 inst)
 *   C-2: movq (4 forms), leaq (mem-reg, RIP-rel)         (+22 inst)
 *   C-3: subq imm-reg, test reg-reg, je rel32, call      (+ 8 inst)
 *   D-1: .byte / .short / .word / .long / .quad          (1/6 dd)
 *   D-2: .asciz / .ascii / .string                       (+4 dd → 5/6)
 *   D-3: .double / .float                                (+1 dd → 6/6)
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

/* OutBuf threads both the byte buffer and the optional relocation
 * cursor through every encoder. `relocs` may be NULL — callers that
 * don't want relocation reporting pay nothing; the encoder still
 * emits the zero-filled placeholder bytes (which is what GAS does
 * pre-link, so byte-equivalence holds either way). `ok` clears on
 * overflow of either buffer; asm_encode maps that to AS_E_NOSPACE. */
typedef struct {
    uint8_t *p;
    size_t   cap;
    size_t   len;
    int      ok;
    Reloc   *relocs;        /* NULL = caller opted out of reloc reporting */
    size_t   reloc_cap;
    size_t   reloc_len;
} OutBuf;

static void outByte(OutBuf *o, uint8_t b) {
    if (!o->ok) return;
    if (o->len >= o->cap) { o->ok = 0; return; }
    o->p[o->len++] = b;
}

/* Record a relocation at the current byte-buffer position. Called
 * just before the four zero placeholder bytes are emitted, so
 * `o->len` is exactly the offset where the rel32 lives. No-op when
 * the caller opted out (relocs == NULL). */
static void outReloc(OutBuf *o, const char *sym, size_t sym_len) {
    if (!o->ok) return;
    if (o->relocs == NULL) return;
    if (o->reloc_len >= o->reloc_cap) { o->ok = 0; return; }
    o->relocs[o->reloc_len].offset  = o->len;
    o->relocs[o->reloc_len].sym     = sym;
    o->relocs[o->reloc_len].sym_len = sym_len;
    o->reloc_len++;
}

/* --- Operand parsing ----------------------------------------------------- */

typedef enum {
    OPR_REG,        /* %rax                                  */
    OPR_IMM,        /* $42 / $0x1f / $-8                     */
    OPR_MEM,        /* disp(%base) — disp may be 0           */
    OPR_MEM_RIP,    /* label(%rip) — symbol; rel32=0 stays   */
    OPR_SYMBOL,     /* bare label (call _printf, je .L4)     */
    OPR_INDIRECT,   /* *%reg (call *%r11)                    */
} OprKind;

typedef struct {
    OprKind     kind;
    int         reg;     /* OPR_REG / OPR_INDIRECT / OPR_MEM base */
    int64_t     imm;     /* OPR_IMM                               */
    int64_t     disp;    /* OPR_MEM (zero if absent)              */
    const char *sym;     /* OPR_SYMBOL / OPR_MEM_RIP — into input  */
    size_t      sym_len; /* 0 when kind has no symbol              */
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
    /* %rip is special — RIP-relative addressing. Capture the label
     * text before '(' as the relocation symbol; trim WS on both
     * sides. The corpus has no `0(%rip)` numeric form, so we don't
     * try to parse a non-symbol disp here — if a future input adds
     * one, parseSignedInt will fail and surface AS_E_MALFORMED. */
    if (inner_len == 4 &&
        inner[1] == 'r' && inner[2] == 'i' && inner[3] == 'p') {
        const char *sp = s;
        size_t sl = paren;
        while (sl > 0 && myWS(sp[sl - 1])) sl--;
        while (sl > 0 && myWS(sp[0]))      { sp++; sl--; }
        out->kind    = OPR_MEM_RIP;
        out->sym     = sp;
        out->sym_len = sl;
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

/* Parse `*%reg` — the indirect form used by `call *%r11`. */
static int parseIndirectOperand(const char *s, size_t n, Oprnd *out) {
    n = trimWS(&s, n);
    if (n < 3 || s[0] != '*' || s[1] != '%') return AS_E_MALFORMED;
    int rn = regLookup64(s + 2, n - 2);
    if (rn < 0) return AS_E_MALFORMED;
    out->kind = OPR_INDIRECT;
    out->reg  = rn;
    return AS_OK;
}

/* Top-level operand parser — dispatches by first non-WS character.
 * A bare token (no leading sigil and no '(') is taken as a symbol;
 * the encoder emits a zero-filled rel32 (relocation site) for it,
 * matching GAS pre-link bytes. */
static int parseOperand(const char *s, size_t n, Oprnd *out) {
    n = trimWS(&s, n);
    if (n == 0)         return AS_E_MALFORMED;
    if (s[0] == '%')    return parseRegOperand(s, n, out);
    if (s[0] == '$')    return parseImmOperand(s, n, out);
    if (s[0] == '*')    return parseIndirectOperand(s, n, out);

    /* A '(' anywhere means memory; otherwise treat as a bare symbol.
     * The whole trimmed run is the symbol — call/je take a single
     * bare label (the comma is a top-level separator handled by
     * parseTwoOperands, which never reaches here for SYMBOL forms). */
    for (size_t i = 0; i < n; i++) {
        if (s[i] == '(') return parseMemOperand(s, n, out);
    }
    out->kind    = OPR_SYMBOL;
    out->sym     = s;
    out->sym_len = n;
    return AS_OK;
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
        outReloc(o, mem->sym, mem->sym_len);
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

/* subq $imm, %dst — opcode 83 /5 (imm8 sign-extended) or 81 /5
 * (imm32 sign-extended). REX.W; REX.B for a high dst. GAS picks the
 * shorter imm8 form whenever the immediate fits in signed-8. */
static int encSubq(const char *opers, size_t n, OutBuf *out) {
    Oprnd a, b;
    int rc = parseTwoOperands(opers, n, &a, &b);
    if (rc != AS_OK) return rc;
    if (a.kind != OPR_IMM || b.kind != OPR_REG) return AS_E_UNKNOWN;
    if (a.imm < INT32_MIN || a.imm > INT32_MAX) return AS_E_UNKNOWN;

    uint8_t rex = REX_BASE | REX_W;
    if (b.reg >= 8) rex |= REX_B;
    outByte(out, rex);

    if (a.imm >= -128 && a.imm <= 127) {
        outByte(out, 0x83);
        outByte(out, (3 << 6) | (5 << 3) | (b.reg & 7));
        outByte(out, (uint8_t)(a.imm & 0xFF));
    } else {
        outByte(out, 0x81);
        outByte(out, (3 << 6) | (5 << 3) | (b.reg & 7));
        int32_t v = (int32_t)a.imm;
        outByte(out, (uint8_t)(v        & 0xFF));
        outByte(out, (uint8_t)((v >> 8)  & 0xFF));
        outByte(out, (uint8_t)((v >> 16) & 0xFF));
        outByte(out, (uint8_t)((v >> 24) & 0xFF));
    }
    return out->ok ? AS_OK : AS_E_NOSPACE;
}

/* test %src, %dst (reg-reg) — opcode 85 /r. ModR/M.reg=src,
 * ModR/M.rm=dst. Operand order is symmetric for test (no "real"
 * destination, just flags); GAS picks this canonical encoding. */
static int encTest(const char *opers, size_t n, OutBuf *out) {
    Oprnd a, b;
    int rc = parseTwoOperands(opers, n, &a, &b);
    if (rc != AS_OK) return rc;
    if (a.kind != OPR_REG || b.kind != OPR_REG) return AS_E_UNKNOWN;

    uint8_t rex = REX_BASE | REX_W;
    if (a.reg >= 8) rex |= REX_R;
    if (b.reg >= 8) rex |= REX_B;
    outByte(out, rex);
    outByte(out, 0x85);
    outByte(out, (3 << 6) | ((a.reg & 7) << 3) | (b.reg & 7));
    return out->ok ? AS_OK : AS_E_NOSPACE;
}

/* je rel32 — long form (0F 84 + rel32). For an undefined symbol
 * GAS cannot guarantee the target fits in rel8 and emits the long
 * form; we match unconditionally because the encoder, like GAS at
 * assemble time, has no way to know the final displacement. The
 * rel32 placeholder is zero-filled — the relocation belongs to
 * the linker, not the assembler. */
static int encJe(const char *opers, size_t n, OutBuf *out) {
    Oprnd a;
    int rc = parseOperand(opers, n, &a);
    if (rc != AS_OK) return rc;
    if (a.kind != OPR_SYMBOL) return AS_E_UNKNOWN;
    outByte(out, 0x0F);
    outByte(out, 0x84);
    outReloc(out, a.sym, a.sym_len);
    outByte(out, 0); outByte(out, 0); outByte(out, 0); outByte(out, 0);
    return out->ok ? AS_OK : AS_E_NOSPACE;
}

/* call symbol      — opcode E8 + rel32, pre-link rel32=0
 * call *%reg       — opcode FF /2 + ModR/M(11, 2, rn), REX.B if rn>=8
 *
 * Default operand size for call in 64-bit mode is 64; no REX.W. */
static int encCall(const char *opers, size_t n, OutBuf *out) {
    Oprnd a;
    int rc = parseOperand(opers, n, &a);
    if (rc != AS_OK) return rc;

    if (a.kind == OPR_SYMBOL) {
        outByte(out, 0xE8);
        outReloc(out, a.sym, a.sym_len);
        outByte(out, 0); outByte(out, 0); outByte(out, 0); outByte(out, 0);
        return out->ok ? AS_OK : AS_E_NOSPACE;
    }
    if (a.kind == OPR_INDIRECT) {
        if (a.reg >= 8) outByte(out, REX_BASE | REX_B);
        outByte(out, 0xFF);
        outByte(out, (3 << 6) | (2 << 3) | (a.reg & 7));
        return out->ok ? AS_OK : AS_E_NOSPACE;
    }
    return AS_E_UNKNOWN;
}

/* --- Data-directive encoders (D-1) --------------------------------------- */

/* .byte / .short / .word / .long / .quad — emit 1/2/2/4/8 little-endian
 * bytes of a signed integer. Re-uses parseSignedInt for decimal and
 * 0x-hex; the operand grammar is a single value (the corpus has no
 * comma-separated lists, so we don't accept them yet). Out-of-range
 * values are silently truncated to the directive's width — GAS warns
 * but emits the same low N bytes, so byte-equivalence holds.
 *
 * The shift uses uint64_t to avoid implementation-defined behavior on
 * arithmetic-right-shift of negative int64_t (e.g. INT64_MIN for
 * `.quad 0x8000000000000000`). */
static int encWidth(const char *opers, size_t n, OutBuf *out, int width) {
    n = trimWS(&opers, n);
    int64_t v = 0;
    int rc = parseSignedInt(opers, n, &v);
    if (rc != AS_OK) return rc;
    uint64_t u = (uint64_t)v;
    for (int i = 0; i < width; i++) {
        outByte(out, (uint8_t)((u >> (i * 8)) & 0xFF));
    }
    return out->ok ? AS_OK : AS_E_NOSPACE;
}

static int encByte (const char *o, size_t n, OutBuf *b) { return encWidth(o, n, b, 1); }
static int encShort(const char *o, size_t n, OutBuf *b) { return encWidth(o, n, b, 2); }
static int encLong (const char *o, size_t n, OutBuf *b) { return encWidth(o, n, b, 4); }
static int encQuad (const char *o, size_t n, OutBuf *b) { return encWidth(o, n, b, 8); }

/* String-operand parser shared by .ascii / .asciz / .string. The
 * grammar is a single double-quoted run; the corpus has no
 * comma-separated multi-string forms. Escape sequences honoured:
 *
 *   \n  0x0A     \\  0x5C     \0  0x00
 *   \r  0x0D     \"  0x22     \xHH (1-2 hex digits)
 *   \t  0x09
 *
 * GAS additionally accepts \b \f \v and full octal escapes (\NNN).
 * Out-of-corpus inputs that use those forms will hit AS_E_MALFORMED
 * — a coverage gap surfaced loudly, not a silent miscompile.
 *
 * Note: \0 here is exactly one NUL byte, not the start of a 3-digit
 * octal escape. GAS treats \012 as octal-12 (a newline). The corpus
 * has neither form, so we keep the simpler reading. */
static int parseStringOperand(const char *s, size_t n, OutBuf *out) {
    n = trimWS(&s, n);
    if (n < 2 || s[0] != '"') return AS_E_MALFORMED;
    s++; n--;
    /* Find closing quote, skipping over backslash-escapes. */
    size_t end = 0;
    while (end < n) {
        if (s[end] == '\\' && end + 1 < n) { end += 2; continue; }
        if (s[end] == '"') break;
        end++;
    }
    if (end >= n || s[end] != '"') return AS_E_MALFORMED;
    /* Anything after the closing quote must be whitespace. */
    for (size_t i = end + 1; i < n; i++) {
        if (!myWS(s[i])) return AS_E_MALFORMED;
    }
    /* Emit decoded bytes. */
    for (size_t i = 0; i < end; i++) {
        char c = s[i];
        if (c != '\\') { outByte(out, (uint8_t)c); continue; }
        if (i + 1 >= end) return AS_E_MALFORMED;
        char e = s[++i];
        switch (e) {
            case 'n':  outByte(out, 0x0A); break;
            case 'r':  outByte(out, 0x0D); break;
            case 't':  outByte(out, 0x09); break;
            case '\\': outByte(out, '\\'); break;
            case '"':  outByte(out, '"');  break;
            case '0':  outByte(out, 0x00); break;
            case 'x': case 'X': {
                /* GAS reads up to 2 hex digits after \x. At least one
                 * is required; AS_E_MALFORMED otherwise. */
                int got = 0;
                uint32_t v = 0;
                while (i + 1 < end && got < 2) {
                    char h = s[i + 1];
                    int d = -1;
                    if      (h >= '0' && h <= '9') d = h - '0';
                    else if (h >= 'a' && h <= 'f') d = h - 'a' + 10;
                    else if (h >= 'A' && h <= 'F') d = h - 'A' + 10;
                    else break;
                    v = v * 16 + (uint32_t)d;
                    i++;
                    got++;
                }
                if (got == 0) return AS_E_MALFORMED;
                outByte(out, (uint8_t)v);
                break;
            }
            default: return AS_E_MALFORMED;
        }
    }
    return out->ok ? AS_OK : AS_E_NOSPACE;
}

/* .ascii — string only, no terminator.
 * .asciz / .string — string followed by a NUL byte. On x86 GAS, the
 * two are synonymous; some assemblers (z80, m68k) define .string
 * differently, but we follow the host GAS the harness compares against. */
static int encAscii(const char *o, size_t n, OutBuf *b) {
    return parseStringOperand(o, n, b);
}
static int encAsciz(const char *o, size_t n, OutBuf *b) {
    int rc = parseStringOperand(o, n, b);
    if (rc != AS_OK) return rc;
    outByte(b, 0);
    return b->ok ? AS_OK : AS_E_NOSPACE;
}

/* Decimal-float operand parser. Grammar:
 *
 *   [+-]? int-digits ('.' frac-digits)? ([eE] [+-]? exp-digits)?
 *
 * The encoder performs the parse with integer arithmetic only — no
 * double / float in this translation unit, since asm.c also compiles
 * under the kernel's -mno-mmx -mno-sse -mno-sse2 flags (kernel.mk).
 * Correctly-rounded fractional-decimal-to-binary conversion needs
 * extended-precision integer math (~hundreds of lines for a real
 * strtod); the corpus has only ".double 1.0", and AS_E_UNKNOWN is
 * the right answer for anything that would require rounding.
 *
 * The parser succeeds iff the literal denotes an exact non-negative
 * integer after exp-folding (e.g. "1.0", "1.", "10e-1", "1e2" → 100).
 * Genuine fractions ("0.5", "0.1") and out-of-range integers return
 * AS_E_UNKNOWN. The integer is then handed to the IEEE-754 bit-pattern
 * builder, which is exact for |v| < 2^53 (double) or < 2^24 (float).
 *
 * Hex floats (0x1.8p3), NaN, Inf, and full GAS float syntax are
 * out of scope for D-3; the corpus does not exercise them. They
 * fall through to AS_E_MALFORMED today — a future input needing
 * them gains a typed branch on first sight. */
static int parseDecimalOperand(const char *s, size_t n,
                               int *sign_out, uint64_t *value_out) {
    n = trimWS(&s, n);
    if (n == 0) return AS_E_MALFORMED;

    int sign = 1;
    if (s[0] == '-')      { sign = -1; s++; n--; }
    else if (s[0] == '+') {             s++; n--; }
    if (n == 0) return AS_E_MALFORMED;

    size_t i = 0;
    size_t int_start = i;
    while (i < n && s[i] >= '0' && s[i] <= '9') i++;
    size_t int_end = i;

    size_t frac_start = 0, frac_end = 0;
    if (i < n && s[i] == '.') {
        i++;
        frac_start = i;
        while (i < n && s[i] >= '0' && s[i] <= '9') i++;
        frac_end = i;
    }

    if (int_start == int_end && frac_start == frac_end)
        return AS_E_MALFORMED;

    int exp10 = 0;
    if (i < n && (s[i] == 'e' || s[i] == 'E')) {
        i++;
        int exp_sign = 1;
        if      (i < n && s[i] == '-') { exp_sign = -1; i++; }
        else if (i < n && s[i] == '+') {                 i++; }
        size_t exp_start = i;
        while (i < n && s[i] >= '0' && s[i] <= '9') {
            exp10 = exp10 * 10 + (s[i] - '0');
            if (exp10 > 1000) return AS_E_UNKNOWN;
            i++;
        }
        if (i == exp_start) return AS_E_MALFORMED;
        exp10 *= exp_sign;
    }

    if (i != n) return AS_E_MALFORMED;

    /* Strip trailing zeros from the fractional part. Anything left
     * is a genuine fractional digit; folded into exp10 below. */
    while (frac_end > frac_start && s[frac_end - 1] == '0') frac_end--;
    int frac_digits = (int)(frac_end - frac_start);
    int net_exp = exp10 - frac_digits;

    /* Combine surviving digits into mantissa. */
    uint64_t m = 0;
    const uint64_t MAX_DIV10 = (uint64_t)0xFFFFFFFFFFFFFFFFULL / 10;
    for (size_t j = int_start; j < int_end; j++) {
        if (m > MAX_DIV10) return AS_E_UNKNOWN;
        m = m * 10 + (uint64_t)(s[j] - '0');
    }
    for (size_t j = frac_start; j < frac_end; j++) {
        if (m > MAX_DIV10) return AS_E_UNKNOWN;
        m = m * 10 + (uint64_t)(s[j] - '0');
    }

    /* Strip trailing zeros from mantissa, raising net_exp. */
    while (m != 0 && m % 10 == 0) { m /= 10; net_exp++; }

    /* Accept only the integer reduction: net_exp >= 0 and the
     * multiply-out fits in uint64_t. */
    if (net_exp < 0) return AS_E_UNKNOWN;
    while (net_exp > 0) {
        if (m > MAX_DIV10) return AS_E_UNKNOWN;
        m *= 10;
        net_exp--;
    }

    *sign_out  = sign;
    *value_out = m;
    return AS_OK;
}

/* IEEE-754 binary64 bit-pattern of an exact integer value (sign * v).
 * Exactly representable iff v < 2^53. Caller emits the 8 LE bytes. */
static int encDouble(const char *o, size_t n, OutBuf *b) {
    int sign;
    uint64_t v;
    int rc = parseDecimalOperand(o, n, &sign, &v);
    if (rc != AS_OK) return rc;
    if (v >= (1ULL << 53)) return AS_E_UNKNOWN;

    uint64_t bits;
    if (v == 0) {
        bits = (sign < 0) ? (1ULL << 63) : 0;
    } else {
        int k = 0;
        uint64_t tmp = v;
        while (tmp > 1) { tmp >>= 1; k++; }
        uint64_t exp_field = (uint64_t)(k + 1023);
        uint64_t fractional = v - (1ULL << k);
        uint64_t mantissa = fractional << (52 - k);
        bits = (((sign < 0) ? 1ULL : 0ULL) << 63)
             | (exp_field << 52)
             | mantissa;
    }
    for (int i = 0; i < 8; i++) {
        outByte(b, (uint8_t)((bits >> (i * 8)) & 0xFF));
    }
    return b->ok ? AS_OK : AS_E_NOSPACE;
}

/* IEEE-754 binary32 bit-pattern. Exactly representable iff v < 2^24. */
static int encFloat(const char *o, size_t n, OutBuf *b) {
    int sign;
    uint64_t v;
    int rc = parseDecimalOperand(o, n, &sign, &v);
    if (rc != AS_OK) return rc;
    if (v >= (1ULL << 24)) return AS_E_UNKNOWN;

    uint32_t bits;
    if (v == 0) {
        bits = (sign < 0) ? 0x80000000U : 0U;
    } else {
        int k = 0;
        uint64_t tmp = v;
        while (tmp > 1) { tmp >>= 1; k++; }
        uint32_t exp_field = (uint32_t)(k + 127);
        uint64_t fractional = v - (1ULL << k);
        uint32_t mantissa = (uint32_t)(fractional << (23 - k));
        bits = (((sign < 0) ? 1U : 0U) << 31)
             | (exp_field << 23)
             | mantissa;
    }
    for (int i = 0; i < 4; i++) {
        outByte(b, (uint8_t)((bits >> (i * 8)) & 0xFF));
    }
    return b->ok ? AS_OK : AS_E_NOSPACE;
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
    { "subq",  encSubq  },
    { "test",  encTest  },
    { "je",    encJe    },
    { "call",  encCall  },
    /* Data directives (D-1). .word is GAS's 2-byte form on x86, a
     * synonym for .short — both route to encShort. */
    { ".byte",   encByte  },
    { ".short",  encShort },
    { ".word",   encShort },
    { ".long",   encLong  },
    { ".quad",   encQuad  },
    /* String forms (D-2). */
    { ".ascii",  encAscii },
    { ".asciz",  encAsciz },
    { ".string", encAsciz },
    /* Float forms (D-3). */
    { ".double", encDouble },
    { ".float",  encFloat  },
};

/* Names that name themselves as data-emitting. Mirrors the harness's
 * isDataDirective (asm_test.c) but the two lists are independent on
 * purpose — the encoder must not trust the harness's classifier for
 * correctness, and the harness must not infer encoder coverage from
 * what dispatches today. D-1 covers the integer-width forms; .ascii
 * et al. fall through to dispatch and return AS_E_UNKNOWN until D-2
 * and D-3 add encoders. */
static int isDataDirective(const char *s, size_t n) {
    static const struct { const char *name; size_t len; } dd[] = {
        { ".byte",   5 }, { ".short",  6 }, { ".word",   5 },
        { ".long",   5 }, { ".quad",   5 }, { ".asciz",  6 },
        { ".ascii",  6 }, { ".string", 7 }, { ".double", 7 },
        { ".float",  6 },
    };
    for (size_t i = 0; i < sizeof(dd) / sizeof(dd[0]); i++) {
        if (n >= dd[i].len && !myStrncmp(s, dd[i].name, dd[i].len)) {
            if (n == dd[i].len || myWS(s[dd[i].len])) return 1;
        }
    }
    return 0;
}

/* Re-classify here so asm_encode is self-contained — the harness's
 * own classifier exists for reporting; the encoder must not trust
 * it for correctness. Lines that emit zero bytes (label, comment,
 * non-data directive, blank) succeed with *out_len == 0. Data
 * directives fall through to dispatch; D-1 covers integer widths,
 * D-2/D-3 close strings and floats. */
static int isNonEmittingLine(const char *s, size_t n) {
    size_t i = 0;
    while (i < n && myWS(s[i])) i++;
    if (i == n) return 1;
    if (s[i] == '#') return 1;
    if (s[i] == '.') {
        if (isDataDirective(s + i, n - i)) return 0;
        return 1;
    }
    size_t j = n;
    while (j > i && myWS(s[j - 1])) j--;
    if (j > i && s[j - 1] == ':') return 1;
    return 0;
}

int asm_encode(const char *att_line, size_t line_len,
               uint8_t *out, size_t out_cap, size_t *out_len,
               Reloc *relocs, size_t reloc_cap, size_t *reloc_count) {
    if (out_len)     *out_len     = 0;
    if (reloc_count) *reloc_count = 0;

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
            OutBuf ob = { out, out_cap, 0, 1, relocs, reloc_cap, 0 };
            int rc = kDispatch[k].fn(opers, opers_len, &ob);
            if (out_len)     *out_len     = ob.len;
            if (reloc_count) *reloc_count = ob.reloc_len;
            return rc;
        }
    }

    /* Mnemonic not in dispatch table. Distinct from AS_E_TODO; that
     * v0-stub return is no longer reachable after C-1. AS_E_UNKNOWN
     * is the "encoder coverage gap, not regression" signal that
     * later cluster commits close. */
    return AS_E_UNKNOWN;
}
