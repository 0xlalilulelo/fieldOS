# ADR-0001: HolyC graft strategy and gates

## Status

Accepted

## Context

`docs/plan/phase-0.md` §M0 chose option (c) of the HolyC bootstrap
table: fork `holyc-lang` (Jamesbarford), graft a freestanding backend,
and use the result both on the host and inside the kernel. Option (b)
— a Python `holyc2c` transpiler emitting C through `$(CROSS_CC)` — is
the documented Path-B fallback (`docs/plan/phase-0.md` §6 risk #1).

`docs/skills/holyc-lang-audit.md` decomposes the graft into six steps.
Two have landed:

- Step 1 — fork in place (commit `60e1a48`, M0 step 5).
- Step 2 — host build wired into our Makefile (commit `ea4c1da`, M3-A
  step 5). Confirmed `beta-v0.0.10` builds cleanly under Apple clang
  21 / arm64-darwin and that `-transpile holyc/bug-tests/Bug_171.HC`
  produces 43 lines of C with exit 0.

Steps 3–6 are the in-kernel graft proper. The audit deliberately
leaves three questions open: the reach of `<math.h>`, the
assembler-vs-binary-emit decision in step 4, and whether the pin
builds at all. M3-A step 5 settled the third (yes) and the host-layer
half of the first (`-lm` link). This ADR's job is to turn the
remaining audit roadmap into a go/no-go-gated plan: per-step exit
criteria, the audit's open assembler call, an explicit Path-B trigger,
the kernel files each step touches, and a pin re-confirmation.

The "M3-A / M3-B" split is a session-level convention, not a
plan-level subdivision. Phase 0's M3 is one milestone with a 3–6
FT-week budget; M3-A is the groundwork already in motion (JIT region,
serial input, host build, this ADR, the ABI header), M3-B is the
graft proper described below.

## Decision

### 1. Pin re-confirmed

`holyc-lang beta-v0.0.10`, sha256
`be664891b02e68424299d1ad874bddce84e46476a23436973865dd190731d3e4`,
BSD-2-Clause. The audit reading and the host build both succeeded on
this pin. No bump for M3-B; bumping mid-graft would invalidate both.
Re-evaluate at M3 exit per the procedure in `holyc/VERSION`.

### 2. Assembler vs binary emit (audit step 4): in-tree minimal encoder

The audit leaves three options open. We choose (a): keep
`holyc/src/x86.c`'s AT&T-text output, feed it through a small
table-driven x86_64 encoder we write at `kernel/holyc/asm.c`.

Rejected:

- **(b) Modify x86.c to emit binary directly.** `x86.c` is 2,460 lines
  of AST-to-AT&T-text deeply intertwined with codegen state. Binary
  emit means rewriting most of those lines and forking deeply from
  upstream — every future pin bump becomes a manual rebase. The
  30 % Path-B buffer in `phase-0.md` §6 risk #1 should not also pay a
  structural-fork cost.
- **(c) Vendor keystone-engine or asmjit.** Keystone is GPLv2,
  incompatible with our BSD-2 base per CLAUDE.md hard constraint #3.
  Asmjit is BSD/Zlib but C++; introducing C++ to the kernel conflicts
  with the single-language ethos and adds toolchain surface for one
  use case.

Why (a) wins:

- Independently testable. The encoder's input is an AT&T-text line,
  output is a byte string; the test harness sits beside it.
- Bounded. The audit estimates ~1,500 lines for the subset `x86.c`
  emits.
- Path-B-compatible. `x86.c` stays unmodified; if we trigger the
  fallback, the encoder work is the only thing thrown away.

### 3. Per-step gates and kernel touch points

#### Step 3 — Strip host-assumption surface (audit §3)

Entry: M3-A complete (this ADR plus the ABI header).

Files removed from the kernel-resident subset:
`holyc/src/{transpiler,cfg,cfg-print,cli,main,memory,mempool}.c`. The
host-side `hcc` continues to use them via `holyc/holyc.mk`; the kernel
build excludes them.

Files added:
- `kernel/holyc/runtime.c` — `palloc`/`pfree`-backed `malloc`/`free`;
  freestanding `memcpy`/`memmove`/`memset`/`strlen`/`strcmp`/
  `strchr`/`strdup`; serial-backed `printf` via `serial_putc`.
- `kernel/holyc/eval.c` — replaces upstream `main.c`. Public entry
  `holyc_eval(const char *src)`.

Files modified:
- `holyc/src/aostr.c` — allocator redirect, preferred via weak-symbol
  shim rather than in-place edit, to keep the pin diff small.
- `kernel/kernel.mk` — `$(HOLYC_KERNEL_OBJS)` section under
  `$(CROSS_CC) -ffreestanding`.
- `kernel/main.c` — calls `holyc_init()` after `slab_init`.

Exit: kernel boots with `holyc_eval("")` returning 0 (no parse, no
codegen) and the existing five self-test lines plus sentinel still
firing. LOC delta reported.

#### Step 4 — In-tree x86_64 encoder (audit §4, §2 above)

Entry: step 3 done.

Files added:
- `kernel/holyc/asm.c`, `kernel/holyc/asm.h` — table-driven encoder
  for the subset of x86_64 instructions `x86.c` emits.
- `kernel/holyc/asm_test.c` — host-side harness, built via an
  extension to `holyc/holyc.mk`, feeding canned `.s` lines and
  asserting byte equivalence with `$(CROSS_CC) -c`.

Files modified:
- `holyc/src/x86.c` — output sink redirected from `fprintf` to an
  `AoStr` consumed by `asm.c`. Smallest possible change; the rest of
  the codegen logic is untouched.

Exit: for the instruction forms appearing in `Bug_171.HC`'s emitted
assembly, `asm.c` output matches `$(CROSS_AS)` byte-for-byte. Harness
runs in CI.

#### Step 5 — Wire into the JIT region (audit §5)

Entry: step 4 done.

`kernel/holyc/jit.c` already exists (commit `97c36ad`, M3-A step 1)
and is exercised every boot. Step 5 is integration: `eval.c` calls
`holyc_jit_alloc(byte_count)` for the encoder's output buffer,
populates it, then `holyc_jit_commit(addr, len)` to clear NX.

No new files. The existing JIT self-test stays in place; a second
self-test for end-to-end compile-and-run is added at M3 exit.

Exit: `holyc_eval("U0 F() { 'X\\n'; } F();")` prints `X` on serial.
Boot-to-sentinel ≤3 s on TCG.

#### Step 6 — REPL (audit §6)

Entry: step 5 done.

Files added:
- `kernel/holyc/repl.c` — line buffer over `serial_getc` (commit
  `7634f67`, M3-A step 3), dispatch to `holyc_eval`. Five-line
  scrollback minimum.

Files modified:
- `kernel/main.c` — calls `holyc_repl()` at the end of `kmain` behind
  a feature flag. The flag is off by default until M3 exits, so smoke
  stays green during M3-B development.

Exit (verbatim from `phase-0.md` §M3): five-line REPL session over
serial covering arithmetic, function definition, function call,
variable mutation, and a deliberate parse error that does not crash
the kernel.

### 4. Path-B trigger

Per `phase-0.md` §6 risk #1 and the audit's closing paragraph: if
step 4 or step 5 accumulates more than two calendar weeks of
part-time work (~30 hours) without reaching its exit gate, we switch
to Path B.

Path B is the Python `holyc2c` transpiler from `phase-0.md` §M0
option (b), estimated 4–6 FT-weeks. It emits freestanding C compiled
by `$(CROSS_CC)` and linked statically into the kernel image at build
time. There is no in-kernel JIT under Path B; the public framing
becomes "the in-kernel JIT is Phase 1."

We do not pre-build Path B. The audit is explicit that it is a risk
hedge, not a parallel track; pre-investment is time taken from the
primary path.

The switch is reversible — `x86.c` is untouched under §2 — so
resuming the in-tree encoder later costs only the time spent.
Triggering Path B requires ADR-0002 superseding the affected steps
of this ADR.

### 5. What this ADR does not decide

- The concrete C → HolyC ABI surface (the ~12 functions sketched in
  `phase-0.md` §M3). That is M3-A step 2 — the ABI header — landing
  next.
- Whether the in-kernel `hcc` is reentrant. M3 ships single-threaded;
  reentrancy is M4 work alongside Patrol scheduling.
- DWARF emission for HolyC. Out of scope for M3 entirely.

## Consequences

Easier:

- The audit's open assembler call is closed; future sessions do not
  reopen it without ADR-0002.
- Each step has a hard exit gate, so "is this done" is answerable
  from the smoke test rather than from feeling.
- The Path-B trigger is mechanical (calendar-weeks counter); the
  switch is not a heroic refusal-to-quit moment.

Harder:

- The in-tree encoder is novel work; the ~1,500-line estimate is the
  audit's, not measured. If it grows, the 100,000-line base-system
  budget tightens.
- We now maintain `x86.c` as an upstream-trackable file. A future pin
  bump that changes the AST-to-AT&T text shape costs us encoder
  rework; the pin re-confirmation in §1 is load-bearing on this.

New risks:

- The encoder's instruction-form coverage is closed-set against what
  `x86.c` emits today. A future HolyC language addition (wider FP,
  for example) could reach beyond it. Mitigation: the encoder
  rejects unknown forms with a loud panic, not silent miscompile.
- Two compile paths for `hcc` exist after step 3: the host build
  (clang via `holyc/holyc.mk`) and the kernel build (cross-GCC via
  `kernel/kernel.mk`). Divergence between them on the same input is
  a class of bug we will discover the first time it bites.

Follow-up:

- M3-A step 2 (ABI header) is a precondition for §3 step 3's
  `kernel/holyc/runtime.c`. It lands next.
- `STATUS.md` updates at M3-B entry, not at this ADR's landing. The
  ADR is paper, not progress.
- Re-evaluate at M3 exit; if Path B was triggered, supersede with
  ADR-0002.

## References

- `docs/plan/phase-0.md` §M0 (HolyC bootstrap strategy table)
- `docs/plan/phase-0.md` §M3 (trampolining strategy, REPL exit)
- `docs/plan/phase-0.md` §6 risk #1 (Path-B framing, 30 % buffer)
- `docs/skills/holyc-lang-audit.md` §"What M3 actually has to do"
- `holyc/VERSION` (pin re-confirmed at §1)
- Commit `97c36ad` — M3-A step 1: JIT region + vmm_remap
- Commit `7634f67` — M3-A step 3: polled serial_getc
- Commit `ea4c1da` — M3-A step 5: host build + transpile path green
- OSDev wiki, x86-64 instruction encoding:
  https://wiki.osdev.org/X86-64_Instruction_Encoding
