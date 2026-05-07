# ADR-0002: HolyC subset xmm activation and the M4 save/restore obligation

## Status

Accepted

## Context

ADR-0001 §3 step 3 lands the kernel-resident subset of `holyc-lang`
under a separate compile flag set (`HOLYC_KERNEL_CFLAGS` in
`kernel/holyc/holyc-kernel.mk`) that, unlike the kernel ELF's own
`KERNEL_CFLAGS`, deliberately omits `-mno-sse` / `-mno-sse2` /
`-mno-mmx`. The reason was identified at commit `95e8012`'s first
architectural discovery: vendored `holyc/src/aostr.c:485` reads
`va_arg(ap, double)` for `%f` formatting; the SysV AMD64 ABI puts
variadic floats in `xmm0..xmm7`, and a `-mno-sse` build refuses to
emit the load. The audit's plan ("keep AoStr as the universal sink")
foreclosed on stripping the float path from `aostr.c` — so the
subset must compile with SSE.

The 95e8012 commit message flagged two paths forward without
choosing:

> Linking this subset into the kernel ELF will require either (i)
> extending `kernel/arch/x86_64/exceptions.S` to save/restore xmm
> state in the IDT entry path (M4-aligned), or (ii) running hcc with
> interrupts disabled (poor M3 PoC compromise).

M3-B candidate B (commit `41a2060`) deferred the choice because the
subset .o files were not yet linked into the kernel ELF. M3-B
candidate C-minimal (this commit landing alongside ADR-0002) does
link them, and the choice now closes against measured behaviour.

Two things turned out to be true that the 95e8012 framing did not
fully anticipate:

1. **The xmm corruption window does not exist in M3.** Limine hands
   off with `RFLAGS.IF=0`, and Field OS does not execute `sti`
   anywhere in the boot path. No interrupt source is wired (PIC is
   at default Limine state; LAPIC timer is M4 work). An interrupt
   cannot fire during the subset's xmm-emitting code, so the IDT
   entry path's xmm-unawareness cannot corrupt FP state. Path (ii)
   from 95e8012 — "running hcc with interrupts disabled" — is not a
   compromise, it is the inevitable M3 state.

2. **Limine does not enable SSE in CR4 / CR0.** The first call into
   the subset (an `aoStrAlloc` triggering GCC's `movdqa`-based
   16-byte struct copy) raised `#UD` (vector 6) because
   `CR4.OSFXSR` was clear. The Limine protocol leaves SSE activation
   to the kernel; `Intel SDM Vol. 1 §13.1` documents the four bits
   the OS must set / clear before issuing SSE instructions.

The decision below covers both: how SSE turns on for M3, and what
M4 must do before the first `sti`.

## Decision

### 1. M3: enable SSE at boot, run with IF=0 forever

`cpu_enable_sse()` runs once from `kmain` after `idt_init` and
before any subsystem that could call into the subset. It performs
the four-bit transition documented in Intel SDM Vol. 1 §13.1:

- `CR0.MP` (bit 1): set — monitor coprocessor (FPU present).
- `CR0.EM` (bit 2): cleared — FPU is not emulated.
- `CR4.OSFXSR` (bit 9): set — OS uses fxsave/fxrstor.
- `CR4.OSXMMEXCPT` (bit 10): set — OS has an `#XF` (vector 19)
  handler installed.

The fourth bit is load-bearing on the IDT install: M1 wired all 32
exception vectors (including #XF) to the standard handler in
`kernel/arch/x86_64/exceptions.S` that dumps registers and halts.
We do not yet have a SIMD-specific handler, but the standard one is
sufficient — any #XF in M3 is a programming error worth halting on,
not a recoverable condition.

The kernel's own C code remains compiled `-mno-sse -mno-mmx
-mno-sse2`. `cpu_enable_sse()` makes SSE *available* to the holyc
subset; it does not start using SSE in kernel code paths. The
kernel's own functions still cannot emit SSE because the compile
forbids it.

M3 boot flow consequence: every boot, `cpu_enable_sse()` runs
exactly once, the holyc subset's SSE-emitting code paths run with
the FP state owned exclusively by that single execution thread, and
no interrupt fires to disturb it.

### 2. M4: extend exceptions.S with fxsave/fxrstor before the first `sti`

When the Patrol scheduler lands and the kernel issues its first
`sti`, the xmm-corruption window opens. Any interrupt taken inside
subset code where the compiler has emitted SSE will be handled by
an entry path that saves only integer registers. On `iretq`, the
xmm registers may have been clobbered by the handler's compiler-
emitted SSE (we forbid that with `-mno-sse` on kernel C, but the
risk re-emerges if any handler is itself compiled with SSE).

The required M4 work, before the first `sti`:

- Reserve a 512-byte fxsave slot per task (or per-CPU at first;
  Patrol ships single-CPU per `phase-0.md` §M4).
- In `kernel/arch/x86_64/exceptions.S`'s common entry stub, after
  the integer GPR push, emit `fxsave64 (rsp-512)` and adjust rsp;
  on exit, `fxrstor64` the same slot.
- The 16-byte alignment requirement of `fxsave` is satisfied by the
  IDT IST stack alignment we already enforce.

This is M4 architectural scope, not M3's. ADR-NNNN (to be written
alongside the M4 Patrol entry) supersedes this section when M4
ships; this ADR's §1 stays in force regardless because SSE
activation is independent of save/restore.

### 3. Alternatives considered and rejected

**(a) Disable SSE in HOLYC_KERNEL_CFLAGS.** The audit's plan
forecloses on this: vendored `aostr.c:485` reads variadic doubles
via `va_arg(ap, double)`. `-mno-sse` refuses to compile the load.
Stripping `%f` formatting from `aostr.c` in the kernel build
violates ADR-0001 §1's pin discipline (no in-place edits to
vendored sources).

**(b) Modify x86.c / aoStr to avoid SSE.** Same pin-discipline
violation as (a), and a deeper structural fork from upstream than
ADR-0001 §2 already accepted for the encoder boundary.

**(c) Keep cli around every subset call indefinitely.** Tractable
in M3 (we already are at IF=0). Not tractable post-M4 — the
scheduler needs interrupts to pre-empt; bracketing every subset
call in cli/sti destroys the scheduling guarantee. fxsave at the
IDT entry path is the standard solution and what this ADR commits
to for M4.

## Consequences

Easier:

- The subset is now observably linked into the kernel ELF and runs
  during boot self-test (`Subset: hcc beta-v0.0.10 linked... OK`
  per `kmain`'s eight-line ladder). The xmm question is closed for
  M3.
- The C-minimal scope holds. cctrl.c and the broader subset
  expansion remain a separate commit (C-real); this ADR does not
  pre-commit to that surface.
- `--gc-sections` is now enabled in `KERNEL_LDFLAGS` (with
  `-ffunction-sections -fdata-sections` on both kernel and subset
  CFLAGS). The kernel ELF strips unreached vendored functions,
  which means future subset expansions can be pulled in without
  paying the full vendored .text cost. ast.o currently contributes
  ~4.5 KiB of reached code instead of 20 KiB.

Harder:

- M4's first `sti` is now load-bearing on a structural change to
  `exceptions.S`. The Patrol entry ADR must reference this one and
  land the fxsave/fxrstor work in the same commit as the first
  `sti`. Forgetting this is silent FP corruption — the test rig
  must include a fxsave round-trip self-test post-M4.
- The kernel ELF now has SSE state to manage. Even before M4, any
  future code path that calls into the subset must run with IF=0
  or accept that an interrupt will corrupt xmm. This constraint is
  documented in `kernel/holyc/eval.c:holyc_subset_self_test`.

New risks:

- A future subset addition that uses 256-bit AVX or 512-bit AVX-512
  instructions reaches beyond fxsave (which only saves the first
  16 xmm); xsave/xrstor with appropriate XCR0 bits is needed. M3
  does not enable AVX (`-O2` without `-mavx*` flags emits SSE only),
  so this is hypothetical for now. Mitigation: keep
  `HOLYC_KERNEL_CFLAGS` free of `-mavx*`; surface explicitly in a
  future ADR if a real driver needs AVX.

Follow-up:

- ADR-NNNN (M4 Patrol) supersedes §2 of this ADR with the concrete
  fxsave/fxrstor implementation.
- `STATUS.md` does not need updating yet — this ADR is in service
  of M3-B step 3 / candidate C-minimal, not a milestone exit.
- `docs/plan/phase-0.md` §M4 should pick up a one-line reference to
  this ADR's §2 obligation in the same commit that this ADR lands;
  the exit criterion for M4 already covers the scheduler ABI and
  this is a fixed cost inside it.

## References

- Intel® 64 and IA-32 Architectures Software Developer's Manual,
  Vol. 1, §13.1 ("Initialization of the SSE/SSE2/SSE3/SSSE3
  Extensions"): https://cdrdv2.intel.com/v1/dl/getContent/671200
- ADR-0001 §3 step 3 (subset compile flags), §1 (pin discipline)
- 95e8012 commit message — first architectural discovery flagging
  the xmm question
- Limine Protocol v12 (does not enable SSE):
  https://github.com/limine-bootloader/limine/blob/v9.x/PROTOCOL.md
- `kernel/arch/x86_64/cpu.{h,c}` — implementation of §1
- `kernel/holyc/eval.c:holyc_subset_self_test` — the first observed
  caller; documents the IF=0 invariant inline
