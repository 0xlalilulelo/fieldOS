#ifndef FIELDOS_HOLYC_ABI_H
#define FIELDOS_HOLYC_ABI_H

/* kernel/holyc/abi.h
 *
 * The C -> HolyC ABI surface for the in-kernel runtime. Translates
 * phase-0.md §M3's ~12-function sketch into a real header against
 * the SysV AMD64 calling convention documented at holyc/src/x86.c:21-35.
 *
 * Consumers:
 *   1. Kernel C source — includes this header and implements each
 *      `k_*` symbol against the relevant subsystem (mm/, arch/x86_64/,
 *      …).
 *   2. The in-kernel HolyC runtime (kernel/holyc/eval.c, M3-B step 3)
 *      — links unresolved externs against the static table in
 *      kernel/holyc/abi_table.c. The table mirrors this header by
 *      hand; if the two drift, K_ABI_VERSION bumps and both must
 *      land in the same commit.
 *   3. Future Brief documents and HolyC userspace — call these
 *      symbols directly. No syscall layer in Phase 0; the kernel
 *      and HolyC code share a single address space (M4 stub
 *      processes get their own PML4 but link the same ABI).
 *
 * SysV register convention (mirrored verbatim from holyc/src/x86.c:21-35
 * so the in-kernel hcc and this header agree):
 *
 *   Integer args 1..6:   rdi, rsi, rdx, rcx, r8, r9
 *   Integer args 7..N:   stack, right-to-left
 *   Float args 1..8:     xmm0..xmm7
 *   Integer return:      rax (rax:rdx for __int128)
 *   Float return:        xmm0
 *   Caller-saved:        rax, rdi, rsi, rdx, rcx, r8..r11, xmm0..xmm15
 *   Callee-saved:        rbx, rbp, r12..r15
 *
 * One Field-OS-specific deviation: the kernel build disables the
 * 128-byte red zone (-mno-red-zone in kernel/kernel.mk). The M3-B
 * graft of holyc-lang's x86.c must emit code that does not assume
 * a red zone either; otherwise an interrupt taken inside emitted
 * code will corrupt local variables. Tracked by ADR-0001 §3 step 4.
 */

#include <stdint.h>

/* --- Stable typedefs --------------------------------------------------
 *
 * HolyC-shaped names map to fixed-width SysV types. The kernel C
 * code uses these throughout the ABI surface so the signatures read
 * the same on both sides of the contract — what a HolyC programmer
 * types matches what the C implementer reads.
 *
 * Width mapping is intentional and SysV-correct:
 *   I8/U8   pass in the low byte of a register; sign-extension on read.
 *   I16/U16 pass in the low word; sign-extension on read.
 *   I32/U32 pass in the 32-bit half (eax/edi/...); zero-extended on
 *           write per the AMD64 architecture.
 *   I64/U64 pass in the full 64-bit register.
 *   F64     passes in xmm0..xmm7 (float class).
 *   U0      is void; usable as a return type only.
 *
 * Pointers are I64/U64-equivalent (8 bytes, integer class). The
 * header uses native `T *` rather than U64 for pointer arguments so
 * the C type checker catches mistakes; HolyC sees them as 8-byte
 * integer-class values regardless. */

typedef uint8_t   U8;
typedef int8_t    I8;
typedef uint16_t  U16;
typedef int16_t   I16;
typedef uint32_t  U32;
typedef int32_t   I32;
typedef uint64_t  U64;
typedef int64_t   I64;
typedef double    F64;

#define U0 void

/* --- ABI version ------------------------------------------------------
 *
 * K_ABI_VERSION is monotonic. Bumps on:
 *   - any signature change to a function declared below,
 *   - any removal of a function below,
 *   - any change to the typedef widths above,
 *   - the M4 exposure of the Patrol scheduler surface (see policy
 *     at the bottom of this header).
 * Adding a function without changing existing ones does not bump
 * the version; the static link table in kernel/holyc/abi_table.c
 * grows in place.
 *
 * The in-kernel hcc reads K_ABI_VERSION at compile time and embeds
 * it as a U32 prologue word in every emitted module. holyc_eval()
 * (M3-B step 3) refuses to run a module whose prologue version does
 * not match the running kernel — the boundary between modules built
 * against different ABI generations must be visible, not silent. */

#define K_ABI_VERSION 0u

/* --- Forward-decl policy for opaque struct types ----------------------
 *
 * The function signatures below reference three kernel structs that
 * are defined elsewhere. We forward-declare them here rather than
 * pulling in their full headers to keep this file consumable from
 * the smallest possible compilation unit (the in-kernel hcc's symbol
 * resolver, in particular, parses this header without needing the
 * exception frame layout or the input subsystem types).
 *
 * Files holding the full definitions:
 *   struct regs         kernel/arch/x86_64/idt.h        (M1, exists)
 *   struct kbd_event    kernel/input/keyboard.h         (M5, future)
 *   struct mouse_event  kernel/input/mouse.h            (M5, future)
 *
 * A C source that needs to access fields includes the relevant
 * header in addition to this one; HolyC code never reaches into
 * these structs and so does not include them.
 *
 * If these struct shapes change, K_ABI_VERSION does not bump (the
 * shape is private to the C side); the bump is only when the
 * function signatures themselves change. */

struct regs;
struct kbd_event;
struct mouse_event;

/* ====================================================================
 *                      ABI surface — Phase 0
 * ====================================================================
 *
 * 11 functions. Symbols are `k_*`-prefixed so HolyC source and C
 * implementations share a single namespace without colliding with
 * holyc-lang's emitted internal labels (which are unprefixed or
 * `_`-prefixed) or with the kernel's own subsystem-prefixed
 * functions (`pmm_*`, `vmm_*`, …). The static link table in
 * kernel/holyc/abi_table.c (M3-B) maps each symbol below to its
 * implementation address.
 *
 * Functions are grouped by milestone-of-implementation. Phase-0
 * milestones land them on the schedule in docs/plan/phase-0.md.
 * Until a function's milestone lands, the kernel's stub returns a
 * sentinel value (-1 / NULL / silent no-op) and a HolyC caller can
 * tell from the return that the surface is not yet wired. */

/* --- M3 (this milestone): serial, memory, panic ---------------------- */

/* Write one byte to COM1. Blocks on THR-empty per kernel/arch/x86_64/
 * serial.c. The basis for HolyC's Print() and "..." string syntax. */
U0 k_serial_putc(I8 c);

/* Allocate `bytes` from the kernel slab heap. Returns the address as
 * U64 rather than `void *` so the SysV register class is unambiguous
 * (integer; rax). 0 on size==0 or OOM. The HolyC `MAlloc` keyword
 * lowers to a call here; `Free` lowers to k_pfree. */
U64 k_palloc(U64 bytes);

/* Release a pointer previously returned by k_palloc. NULL is a
 * silent no-op. Mismatched pointer panics via cli;hlt — the kernel
 * has no graceful recovery in Phase 0. */
U0 k_pfree(U0 *p);

/* Halt the kernel with a serial banner. The HolyC `throw` keyword
 * and the language's runtime checks (array bounds, divide-by-zero,
 * etc.) lower to a call here. Never returns. */
U0 k_panic(const I8 *msg) __attribute__((noreturn));

/* --- M4 (Patrol scheduler): timing and yield ------------------------- */

/* Monotonic nanosecond clock. Source is the LAPIC timer once it's
 * calibrated (M4); pre-M4, returns the TSC scaled by a boot-time
 * estimate, accurate to within a few percent. Wraps after ~584
 * years; treat as unsigned monotonic. */
U64 k_time_ns(U0);

/* Cooperative yield. Pre-M4, no-op (single-thread kernel). Post-M4,
 * trips the scheduler. Documented here so HolyC code written before
 * M4 lands continues to work after. */
U0 k_sched_yield(U0);

/* Register a C handler against an x86_64 interrupt vector. Pre-M4
 * the IRQ subsystem is installed but exposes no public registration
 * API; this stub returns -1 until M4 wires it. The handler signature
 * matches kernel/arch/x86_64/idt.h's exception entry. */
I32 k_irq_register(I32 vec, U0 (*handler)(struct regs *));

/* --- M5 (input/output): framebuffer, keyboard, mouse, fonts ---------- */

/* Blit `pixels` (BGRA32, w*h elements, row-major) at framebuffer
 * coordinates (x, y). Out-of-bounds rectangles are clipped. Pre-M5
 * a no-op; the framebuffer driver only renders text via fb_putc_at
 * today (M1), and the HolyC contract for blit requires the M5
 * surface model. */
U0 k_fb_blit(I32 x, I32 y, I32 w, I32 h, const U32 *pixels);

/* Poll one keyboard event into *out. Returns 1 if an event was
 * consumed, 0 if the queue was empty, -1 if the keyboard subsystem
 * is not yet initialised (pre-M5). The HolyC InKey() lowers here. */
I32 k_kbd_poll(struct kbd_event *out);

/* Poll one mouse event into *out. Same return convention as
 * k_kbd_poll. The HolyC mouse routines lower here. */
I32 k_mouse_poll(struct mouse_event *out);

/* Look up a glyph for `codepoint` in the active font. Writes glyph
 * width/height to *w and *h and returns a pointer to the bitmap
 * (8-bit-per-pixel coverage). Returns NULL if the codepoint is
 * unmapped. The HolyC text-rendering routines lower here; Brief's
 * inline glyphs ($IM tag) consume the same path. */
const U8 *k_font_lookup(U32 codepoint, I32 *w, I32 *h);

/* ====================================================================
 *                  Forward-decl policy for M4 Patrol
 * ====================================================================
 *
 * Phase-0 §M3's sketch closes with the line "// Patrol scheduler
 * stubs, exposed once M4 lands." This header honours that literally
 * — the Patrol surface (start, stop, supervise, restart, opaque
 * thread handles, priority/state queries) is not declared here.
 *
 * Why not pre-declare:
 *   - The supervisor/launchd-equivalent shape settles during M4
 *     bring-up; locking the names against an unbuilt subsystem
 *     would commit us to abstractions we have not yet validated.
 *   - HolyC code written in M3 cannot meaningfully call any
 *     Patrol symbol before M4, so a present-but-stubbed surface
 *     gives no value and risks looking implemented when it is not.
 *
 * What lands when M4 lands:
 *   1. K_ABI_VERSION bumps to 1.
 *   2. A second header `kernel/holyc/abi_patrol.h` is included from
 *      the bottom of this file under `#if K_ABI_VERSION >= 1`.
 *      Patrol symbols are declared there, not here, so this file
 *      stays a stable Phase-0 contract.
 *   3. abi_table.c grows new entries for each Patrol symbol; the
 *      module-prologue version check in holyc_eval refuses to run
 *      M3-era modules that did not embed the new version (because
 *      they would not have linked against the new symbols anyway).
 *
 * The single Patrol-shaped function declared here — k_sched_yield —
 * is in the M3 group not the M4 group on purpose. It has no opaque
 * handle, no state, no priority; the pre-M4 stub is a literal nop
 * and the post-M4 implementation costs no signature change.
 *
 * If a piece of work needs more of the Patrol surface than
 * k_sched_yield before M4 ships, the right move is an ADR
 * proposing the affected subset, not a quiet expansion of this
 * header. */

#endif /* FIELDOS_HOLYC_ABI_H */
