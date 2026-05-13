# M0 step 3F — LAPIC and soft preemption

*May 13, 2026. Three sessions across two calendar days. Five
commits.*

3F is the sixth of seven sub-blocks in M0 step 3 (memory,
scheduler, virtio, network, framebuffer, **preemption**, `>`
prompt). The exit criterion is narrow on paper: bring up the
local APIC, arm a periodic timer, deliver IRQs into the existing
IDT. The exit criterion in practice is the boundary moment —
this is the commit where `IF=1` propagates through cooperative
code for the first time and `hlt` becomes real power-save. From
3F-3 forward, the kernel can be interrupted at any instruction
boundary in cooperative paths. Any post-3F-3 regression that
wasn't visible before should be evaluated through the "is this
code path IRQ-safe?" lens before any other hypothesis.

The "soft preemption" qualifier is the design call that shapes
3F: the timer IRQ handler does `TICKS.fetch_add(1)` + EOI, no
context switch. Hard preemption — an IRQ-driven `switch_to` into
the next runnable task — is deferred to M0 step 4 where SMP
forces the design surface (per-CPU current pointers, real
cli/sti preempt-disable counters, rflags save-restore in
`switch_to`). 3F lights up the timer; step 4 wires it into the
scheduler.

## What landed

Five commits across three sessions:

- `7dd1dfd` *feat(kernel): mask 8259 + map LAPIC MMIO.* 3F-0.
  Adds `arsenal-kernel/src/apic.rs`. Masks both 8259A PICs by
  writing `0xFF` to OCW1 on ports 0x21 / 0xA1 (canonical "mask
  all" per the Intel 8259A datasheet, Table 2). Reads
  `IA32_APIC_BASE` through `x86_64::registers::model_specific::
  ApicBase` and asserts `LAPIC_ENABLE | BSP & !X2APIC_ENABLE`
  — three preconditions whose violation would mean either
  firmware mis-configured the LAPIC or QEMU enabled x2APIC under
  us. Maps the LAPIC MMIO page through `paging::map_mmio` (same
  helper 3C added for virtio's BAR4 region — Limine's HHDM
  covers RAM only, so device MMIO needs explicit mapping with
  `PRESENT | WRITABLE | NO_CACHE | WRITE_THROUGH`). On QEMU q35
  the LAPIC sits at the conventional `0xFEE00000` and reads back
  ID 0, VERSION `0x00050014` (xAPIC, max LVT entry 5).
- `896183e` *feat(kernel): LAPIC software enable + spurious
  vector.* 3F-1. Writes the SVR (Spurious-interrupt Vector
  Register) with `LAPIC_SVR_ENABLE | 0xFF` — bit 8 software-
  enables the LAPIC, bits 0..7 set the spurious vector. Wires
  `apic::spurious_handler` into the IDT at vector 0xFF (a
  `Lazy<InterruptDescriptorTable>` initializer that
  `set_handler_fn`s the entry alongside the existing exception
  handlers). The handler logs the first occurrence behind a
  `SPURIOUS_SEEN: AtomicBool` swap and silently absorbs the
  rest. Spurious delivery does not set the LAPIC's ISR bit and
  therefore requires no EOI write (Intel SDM Vol. 3A §10.9).
  No timer yet; this is the "if anything mis-arms, we will see
  it once" landing pad.
- `41e7f8d` *fix(task): bump kernel task stack from 16 KiB to
  32 KiB.* Latent fix. 3F-2's binary-layout shifts (new module,
  additional `extern "x86-interrupt"` handler, expanded
  `init()` format string) pushed the post-TLS smoltcp / rustls
  callchain past 16 KiB at its deepest moments. The overflow
  wrote *below* the task stack's lowest address, into whatever
  adjacent allocation `linked_list_allocator` had handed out —
  often a `Vec`'s capacity field — and the corrupted capacity
  round-tripped through smoltcp / rustls and surfaced minutes
  later as a `LayoutError` unwrap inside `hole.rs:422`'s
  dealloc path. Pure 3F-1 happened to compile in a way that
  kept the chain under 16 KiB; pure 3F-1 + this bump is
  identical behavior. 3F-2 + the old 16 KiB triggers the
  `LayoutError` deterministically post-`ARSENAL_TLS_OK`;
  3F-2 + this bump is clean across repeated 30 s QEMU runs.
  32 KiB is the same order of magnitude as Linux's per-thread
  interrupt-stack region; the cost is +16 KiB per task,
  +80 KiB total at smoke time, comfortably inside HEAP_CAP's
  16 MiB. See the detour below.
- `6c4b169` *feat(kernel): LAPIC periodic timer + PIT-calibrated
  100 Hz tick.* 3F-2. Calibrates the LAPIC timer against PIT
  channel 2: program PIT ch2 in mode 0 (interrupt on terminal
  count) with software-controlled gate via port 0x61 bit 0,
  load an ~11932-tick reload (10 ms at the architectural
  1.193182 MHz PIT crystal), start the LAPIC timer counting
  down from `0xFFFFFFFF` with divide /16 and a masked LVT entry,
  open the PIT gate and the LAPIC initial-count write back-to-
  back, poll port 0x61 bit 5 (PIT ch2 OUT) until terminal count,
  snapshot the LAPIC current-count register. Elapsed LAPIC
  ticks ÷ 10 ms gives the bus-clock-derived tick rate;
  `0xFFFF_FFFF - current` on QEMU TCG reports 624375 LAPIC ticks
  per 10 ms, matching the expected ~1 GHz bus clock through
  divide /16. Sanity-bounds the result against
  `[1_000, 1_000_000_000]`; anything outside is broken
  PIT/LAPIC sequencing rather than something to silently
  compensate for. Periodic arming writes the divide register,
  the LVT timer entry (`LVT_TIMER_PERIODIC | 0xEF`), and the
  initial-count register *in that order* — initial-count is the
  trigger per Intel SDM Vol. 3A §10.5.4, so the LVT must
  already be configured. `apic::timer_handler` is the second
  IDT entry installed at the Lazy initializer (vector 0xEF
  alongside spurious at 0xFF). The handler increments
  `TICKS: AtomicUsize` and writes 0 to the EOI register
  (offset 0xB0). IRQ delivery remains gated on `IF=0` after
  3F-2 — the timer counts but nothing reaches the handler.
- `0323497` *feat(kernel): idle hlt + sti + ARSENAL_TIMER_OK.*
  3F-3. Three coupled changes lighting up the timer:
  `apic::observe_timer_ok` — a cooperative-context probe that
  prints `ARSENAL_TIMER_OK` once `ticks() >= 10`, latched on a
  private `TIMER_OK_LATCHED: AtomicBool` swap so the print
  fires exactly once. `sched::idle_loop` regains the `hlt`
  it dropped at 3B-4 and gains a `sti` once at entry, with the
  body restructured to `yield_now` → `observe_timer_ok` → `hlt`.
  Cooperative `switch_to` doesn't save or restore rflags, so
  `IF=1` propagates from idle's first switch-in to every
  subsequent task scheduled in — the soft-preemption posture
  the 3F kickoff specified. `ci/qemu-smoke.sh` gets a ninth
  required sentinel (`ARSENAL_TIMER_OK`). Smoke passes ~1 s
  locally on all nine.

## How long it took

Three sessions across two calendar days (2026-05-13 morning,
afternoon, evening). The 3F sub-block was always the one where
the per-sub-block estimate diverged most from reality and the
divergence ran in both directions — 3F-0 and 3F-1 were tighter
than expected (`x86_64::registers::model_specific::ApicBase`
exists and Just Works; the IDT's `Lazy` initializer
accommodates new entries without touching `idt::init`'s body),
while 3F-2 ate the budget of multiple sub-blocks because of the
task-stack incident below.

The HANDOFF's calibration expected 3F to be the *bug-prone*
sub-block of M0 step 3, with the explicit warning that "the
bug-prone moment is APIC vector collision with the int3 /
page-fault handlers already installed in 3A." That specific
prediction was wrong — vector collision turned out to be a
non-event because the exception-vector range (0..32) is
architecturally distinct from the LAPIC vector range we picked
(0xEF, 0xFF) — but the spirit was right. The actual bug was
load-bearing in a way 3F's design space didn't naturally
surface: a stack-budget regression masquerading as a heap
corruption. Logged below.

## Detours worth recording

**The 16 KiB task stack overflowed.** This is the 3F story
worth carrying forward across milestones. The setup: each
scheduler-managed task (idle, ping, pong, `net::poll_loop`)
runs on a `KernelStack` the scheduler hands its saved RSP into
on first switch. At 3B-3 the stack was sized to 16 KiB on the
assumption that no M0 task would recurse particularly deep.
That assumption held through 3B, 3C, and *most* of 3D —
including 3D-3's plain TCP probe — but it was wrong by 3D-4
in a way that didn't manifest until 3F-2.

The poll_loop task is by far the deepest call chain in M0:
smoltcp's `Interface::poll` recurses into TCP retransmit logic
which, post-handshake, drives rustls's
`UnbufferedClientConnection` state machine — a stack profile
that includes RustCrypto's ChaCha20-Poly1305 and the TLS 1.3
record-layer buffers. The chain's *deepest* moment lands
post-handshake, several seconds into smoke wall time, and only
on iterations where smoltcp's retransmit timer happens to align
with rustls's record processing.

3F-2's binary-layout shifts pushed that callchain just past
16 KiB. The overflow wrote below the task stack's lowest mapped
address into whatever adjacent allocation `linked_list_allocator`
had handed out — most often a `Vec`'s capacity field, occasionally
a `Box<Task>`'s discriminant. The corrupted byte round-tripped
through smoltcp / rustls without anyone caring (the field whose
low byte was clobbered was usually a length that subsequent code
overwrote anyway) and only surfaced minutes later when *some
unrelated allocation* called `dealloc`, which called
`align_layout` on the corrupted `Layout` and unwrap'd a
`LayoutError` inside `hole.rs:422`. The panic site was three
allocations and ~30 seconds removed from the actual fault.

The diagnostic shape: pure 3F-1 (no 3F-2 changes, no task-stack
bump) passes 30 s smoke reliably. Pure 3F-1 + the task-stack
bump is identical behavior. 3F-2 + the old 16 KiB triggers the
`LayoutError` deterministically post-`ARSENAL_TLS_OK`. 3F-2 +
the bump is clean across repeated 30 s runs. Bisecting from
the panic site to the actual fault required the same trick the
3B-3 switch-test commit used in miniature: write the fault into
the assumption being violated, not into the code observing the
violation.

The **carry-forward posture change**: kernel task stacks are
now 32 KiB. New features that touch the deep callchain — Stage
IPC at M2, the LinuxKPI bridge at M1, the Wasmtime runtime at
v0.5 — should budget against that header, not against the 16 KiB
that worked through 3B/3C/3D-3. The 32 KiB number isn't sacred
either; it's the same order of magnitude as Linux's per-thread
interrupt-stack region, with enough headroom to absorb the next
feature that doesn't think it's growing the stack but is. Worth
adding stack-watermark instrumentation when M0 step 4's per-CPU
state lands — a per-task `stack_bytes_used_high` counter
sampled at `yield_now` would have surfaced this in seconds
instead of a session.

**xAPIC, not x2APIC.** The design call was upstream of any
code. The kickoff trade-off pairs surfaced both; the answer was
xAPIC for M0. Reasoning: x2APIC's register interface (MSRs at
`0x800..0x83F` instead of MMIO at `0xFEE00000`) is genuinely
nicer — 64-bit accesses, no MMIO indirection, no calibration-
window race against the PIT — but x2APIC's value materializes
at scale (ID space > 8 bits, IPIs across many cores, x2APIC
self-IPI). Single-core M0 doesn't need any of that, and xAPIC
makes the calibration code identical to what every other x86
kernel does. The IA32_APIC_BASE check in `apic::init` asserts
`!X2APIC_ENABLE` explicitly rather than tolerating both — if
the BIOS ever flips it on us, we want to surface that
immediately, not silently fall back. x2APIC revisits at M0
step 4 when SMP arrives and the IPI shape forces the
conversation.

**PIT channel 2 for calibration.** Two alternatives surfaced:
channel 0 (the legacy "system timer") or channel 2 (the
"speaker" channel with software-controlled gate). Channel 0 is
free-running and its OUT line is wired to the 8259 IRQ0, which
we just masked — usable but it would have required leaving
8259 IRQ0 unmasked for the calibration window and re-masking
afterward. Channel 2's gate is software-controlled via port
0x61 bit 0 and its OUT line is readable via port 0x61 bit 5,
which is exactly the shape calibration wants: open the gate
and the LAPIC initial-count write back-to-back, poll the OUT
line, close the gate. No IRQ wiring; no temporary state on the
8259. The PIT crystal frequency (1.193182 MHz) is one of the
architectural constants preserved on every x86 chipset since
1981 — same number on QEMU q35 as on a Framework 13 AMD.

**100 Hz, not 1000 Hz.** Linux's HZ is configurable (100 / 250 /
300 / 1000) and modern defaults trend toward 250 or 1000. 100 Hz
is the desktop legacy choice and the one M0 wants: at 100 Hz the
timer-IRQ overhead is one fetch_add + one EOI write per 10 ms,
which is negligible on any real CPU and dominated by other M0
costs. Higher rates would buy us nothing pre-SMP and force
sooner conversations about per-CPU tick state. 1000 Hz is a
real-time-systems and tight-scheduling choice that Arsenal can
adopt later if the scheduler discipline ever needs it — and the
`TIMER_HZ` constant in apic.rs is the one knob to turn.

**Spurious vector wired once at IDT-init time.** The
`x86_64::structures::idt::InterruptDescriptorTable` is a fixed
256-entry table; the `Lazy<InterruptDescriptorTable>` shape in
`idt.rs` means *every* IDT entry must be set inside the Lazy
initializer body before `IDT.load()` runs. After load, the
table is alive on the CPU and adding entries is a different
operation (rewriting the in-memory IDT and reloading would
work, but the x86_64 crate's API doesn't expose that path).
For M0 that's fine — the timer and spurious vectors are the
only LAPIC vectors we need pre-SMP — but it's a constraint
worth flagging now: device IRQs added past M0 step 3 will need
either pre-registration in the same Lazy initializer (the
3F-1 / 3F-2 pattern) or a different table-management strategy.
The constraint surfaces explicitly when virtio's MSI-X support
arrives.

**Cooperative `switch_to` doesn't save rflags — the IF=1
propagation.** This is the subtle correctness point that makes
the single `sti` in `idle_loop` work for the whole system.
`switch_to` (sched.rs:62-82) pushes only callee-save GPRs
(rbx, rbp, r12-r15) and the return address; it does not push
rflags. After `sti` runs once in idle, IF=1 is part of the CPU's
flags register. The first time idle yields, `switch_to`'s `ret`
lands in another task's stack frame *with IF still set* —
because the new task's stack was constructed by `Task::new`
without touching rflags either, and the only ways IF gets
cleared during normal cooperative operation are (a) an explicit
`cli` somewhere in code (we have none) or (b) an interrupt
gate's IF-clear on entry, which is matched by `iretq`'s restore
on exit. So IF stays set across every cooperative switch from
idle's first sti onward, and every task gets soft preemption
"for free" without per-task setup. Hard preemption at M0 step 4
will need a real rflags save/restore in `switch_to` so that
preempt-disable counters can survive a context switch, but
that's a step-4 concern.

The corollary: `extern "x86-interrupt"` handlers from the
x86_64 crate use **Interrupt Gates** (not Trap Gates), which
clear IF on entry and restore the saved IF on `iretq`. Inside
the timer or spurious handler, IF=0 — no nested IRQ window.
The handler bodies are trivial enough (fetch_add + write) that
nothing inside them could legitimately want to be interrupted,
and trying to make them re-entrancy-safe would be an
anti-pattern at this scale.

**Probe site, latch home, and yield-then-hlt ordering.** Three
sub-decisions in 3F-3 that the kickoff surfaced as trade-off
pairs:

(a) *Probe location.* `idle_loop` is the natural site —
cooperative, runs after sti, runs frequently enough to observe
TICKS crossing 10 within the smoke window. A separate
"observer" task would have allocated a 32 KiB stack to make
one boolean observation. Inside `ping`/`pong`'s `finish-loop`
would have coupled the observation to the demo's lifetime.

(b) *Latch home.* `apic.rs` owns `TICKS` and `SPURIOUS_SEEN`;
`TIMER_OK_LATCHED` belongs in the same module because the
three together are timer-observability state. Putting the
latch in `sched.rs` near the probe site, or in `main.rs` near
the existing sentinel infrastructure, would have separated
the state from its owner.

(c) *yield-then-hlt vs hlt-then-yield.* yield-then-hlt
preserves the 3B-4 round-robin shape: idle's first action is
always a switch into a Ready peer, and idle only blocks once
the runqueue is empty. hlt-then-yield would have idle blocking
immediately on entry; the first switch into a peer would have
been pushed past the first timer wake. The functional outcome
is identical, but the yield-first shape matches the "idle is
just another task" model the cooperative scheduler was built
around.

## The numbers

- **5 commits in 3F** (3F-0, 3F-1, the 41e7f8d task-stack fix
  carried with 3F-2, 3F-2, 3F-3) plus the 3F-4 STATUS + devlog
  commit this devlog ships in.
- **Net new Rust** in `arsenal-kernel/src/`: 0 lines in main.rs
  beyond the `mod apic;` declaration and the `apic::init()`
  call site, ~428 lines in `apic.rs` (full module — LAPIC
  bring-up, calibration, periodic arming, two IRQ handlers,
  observe_timer_ok), ~25 lines net in `sched.rs` (idle_loop's
  sti + hlt + probe), ~5 lines net in `idt.rs` (two
  `set_handler_fn` calls), 2 lines net in `task.rs` (the
  16 KiB → 32 KiB stack-size constant). Total ~460 lines added
  across the five commits, ~12 removed.
- **ELF**: 1,474,744 (end of 3E) → 1,479,400 (end of 3F-3).
  Net +4,656 bytes across five sub-steps; the apic.rs module
  is small in machine code because so much of it is constants
  and one-shot init paths the linker can inline aggressively.
  ISO unchanged at 19.3 MB.
- **Sentinels: 8 → 9.** Added `ARSENAL_TIMER_OK`. The smoke
  wait-loop already absorbed the addition without per-step
  changes (the 3D-5 refactor from "FINAL_SENTINEL fires" to
  "all required present" was foresight that paid off here too).
- **Smoke time: still ~1 s** locally. The IRQ delivery at 100 Hz
  is one fetch_add + one EOI write per 10 ms; calibration adds
  one ~10 ms window at boot to the smoke's wall time.
- **Calibration result on QEMU TCG -cpu max**: 624375 LAPIC
  ticks per 10 ms (≈62.4 MHz effective LAPIC clock after /16,
  consistent with QEMU TCG's nominal ~1 GHz bus). Real silicon
  ranges from a few hundred thousand to tens of millions —
  the sanity bound `[1_000, 1_000_000_000]` covers both ends.

## What the boot looks like

Three new serial lines appear in the boot trace, between
`ARSENAL_TLS_OK` and `sched: init complete`:

```
apic: 8259 masked; LAPIC phys=0x00000000fee00000 id=0 version=0x00050014 svr-enabled; spurious vector=0xff
apic: calibrated 624375 LAPIC ticks per 10 ms; armed periodic 100 Hz vector=0xef initial_count=624375
sched: init complete; switching to idle
sched: idle running
ping
pong
...
ARSENAL_TIMER_OK
```

The `ARSENAL_TIMER_OK` lands ~100 ms after `sched: idle running`
(the threshold is 10 ticks at 100 Hz). In wall time this is
well under one second from boot — the smoke observes all nine
sentinels at the 1 s mark consistently across local TCG runs.

On `-display gtk` / `-display sdl` the framebuffer mirror from
3E carries the new lines onto the screen in amber-on-navy. The
mirror gets exercised through the timer handler exactly zero
times — soft preemption keeps the handler trivial. If hard
preemption arrives at step 4 and the IRQ handler ever wants to
print, the `FB.try_lock()` in `fb::print_str` (designed for
this case at 3E-3) absorbs the contention; the print drops on
the floor in the rare case but the panic-safety property
holds.

## What 3G looks like

Per ARSENAL.md M0 step 3, the next and last sub-block before
SMP: the interactive `>` prompt and the perf gate.

- **Keyboard input.** Two real options: PS/2 via the legacy
  i8042 controller (universally available under q35; the
  i8042 state machine is well-documented but has corners —
  the OSDev wiki's i8042 page is the canonical reference, and
  the Linux `drivers/input/serio/i8042.c` is the
  battle-tested implementation we'd cite from), or
  virtio-keyboard (which would reuse the modern-PCI transport
  from 3C and matches the "Arsenal does virtio first" thesis,
  but virtio-keyboard is less universally supported by host
  hypervisors than virtio-blk or virtio-net). The PS/2 choice
  is the conservative one and likely the 3G kickoff
  recommendation.
- **Line editing.** A bounded buffer (256 chars feels right
  for M0), cursor position tracked in `fb`'s cursor state,
  backspace handling, basic ANSI editing keys (Home / End /
  cursor-arrows), Enter dispatches.
- **Command dispatch.** A simple `match` on the first
  whitespace-delimited token. Three commands seed it: `help`
  (list commands), `hw` (hardware summary — the usability
  gate from ARSENAL.md M0 says "prompt is keyboard-navigable;
  shows hardware summary"), and `panic` (a deliberate panic
  to exercise the panic handler in interactive mode). More
  commands accrue naturally as M0 step 4 / step 5 features
  want a way to be poked.
- **Perf gate in CI.** The hard one. ARSENAL.md asserts < 2 s
  boot-to-prompt under QEMU as the M0 step 3 performance gate.
  `ci/qemu-smoke.sh` currently times out at 15 s and passes
  in 1 s — we don't actually know our boot-time *budget*,
  just that we're inside a generous one. 3G needs the smoke
  to assert a wall-clock budget (a `time` capture around the
  QEMU launch, a `bash` check against an envvar threshold,
  failure mode when boot drifts). Hosted runners have higher
  variance than local TCG so the threshold needs headroom —
  3 s on ubuntu-24.04 against 2 s locally is a defensible
  shape. The risk is that the gate flakes; the mitigation is
  retry-on-tight-miss with a hard fail at 1.5× the budget.

The bug-prone moments in 3G are PS/2 timing (the i8042's
"send command, wait for OBF / IBF" handshakes are real even
under TCG — anything that doesn't poll the status register
properly will hang) and the perf-gate CI surface (deterministic
boot-time measurement across hosted runners is harder than it
looks; the smoke needs to distinguish "QEMU was slow to start"
from "Arsenal was slow to boot").

After 3G, M0 step 3 closes. M0 step 4 (SMP — IPI bring-up, AP
startup, per-CPU LAPIC state including ticks, hard preemption
discipline with cli/sti counters and rflags save-restore in
`switch_to`) is the last major surface before M0 wraps. The
M0 exit criterion ("boots to `>` prompt in QEMU") is structurally
there after 3G; what 4 adds is the "real preemption" polish that
turns soft preemption into a scheduler that can actually preempt.

## Cadence

This is the sixth sub-block devlog of M0 step 3 (3A, 3B, 3C,
3D, 3E, 3F). The 3F-2 task-stack incident is the kind of
load-bearing posture change the M0 step 3 retrospective should
absorb explicitly — kernel task stacks at 32 KiB is now the
baseline assumption, not 16 KiB, and the next time a feature
grows the call chain it should budget against the new header
rather than rediscovering the limit through a `LayoutError`
unwrap three allocations downstream of the actual fault.

The xAPIC-over-x2APIC, PIT-channel-2, 100-Hz, soft-preemption
calls are all decisions that the 3F kickoff documented as
trade-off pairs and the implementation locked in. None of them
need ADRs — they're inside the design surface the M0 plan
already authorized — but they're worth recording here so the
next time the question "why didn't Arsenal start with x2APIC?"
comes up (M0 step 4 will surface it), the answer is one search
away.

Three sessions on 3F is one more than the 3A/3B/3C/3E baseline
and on par with 3D — 3D's third session was crate archaeology,
3F's third session was the task-stack diagnosis. Both are the
sort of session where the per-sub-block estimate diverges most
from reality; the framebuffer devlog's note that "kickoff
documentation makes spec-driven work template-shaped" continues
to hold, with the corollary that *non-spec-driven* surprises
(a stack overflow corrupting an unrelated allocation) are where
multi-session sub-blocks come from.

The Asahi cadence stays the model — calibrated, honest, never
marketing. The 3G devlog is next, sized against the same
template; if 3G ends up reading as "PS/2 spec section we worked
from" then that's the signal to consolidate, but the perf-gate
work is genuinely novel CI surface and worth its own writeup.
