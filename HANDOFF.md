Kickoff for the next session — M0 step 3F, LAPIC + preemption.

M0 step 3E (framebuffer console) closed cleanly across four
commits ending at 8aad04d on 2026-05-13, plus the STATUS flip at
e115095 and the 3E devlog at 9049f56: Limine FramebufferRequest
probe, fb::clear + fb::put_pixel over the linear framebuffer,
8x16 glyph rendering against vendored Spleen 8x16 v2.2.0 (BSD-
2-Clause under vendor/spleen/), and a byte-level fan-out from
serial::write_str to a cursor-tracking fb::print_str with newline
+ line-wrap + scroll-by-blit. The 3D devlog catch-up landed at
4c5385b before 3E-0 started, and a chore(ci) bump of
actions/checkout to v5 at ef36a68 followed the push. CI is green
on both subsequent runs (8 sentinels in 1 s each). Smoke still
asserts the same eight sentinels (ARSENAL_BOOT_OK, ARSENAL_HEAP_OK,
ARSENAL_FRAMES_OK, ARSENAL_BLK_OK, ARSENAL_NET_OK, ARSENAL_SCHED_OK,
ARSENAL_TCP_OK, ARSENAL_TLS_OK) — 3E intentionally rode on the
existing set rather than adding ARSENAL_FB_OK; the smoke target
is implicit ("the kernel continues past fb init / render / mirror
without faulting or deadlocking").

3F is the sub-block where the cooperative-correctness shortcuts
from 3B and 3C have to start surviving interrupts. The bug-prone
surface is real for the first time since 3D: IRQ context safety
against the existing spin::Mutex<VecDeque<Box<Task>>> runqueue is
the load-bearing decision, and the "soft preemption" vs "hard
preemption" trade-off pair below is the most important call.

read CLAUDE.md (peer concerns, Rust-only, BSD-2-Clause base,
build loop sacred, no_std + nightly + abi_x86_interrupt — the
last is what 3F's new extern "x86-interrupt" handlers ride on) →
STATUS.md (3E complete, 3F is the active sub-block of step 3) →
docs/plan/ARSENAL.md § "Three Concrete Starting Milestones" → M0
(perf gate: boot to prompt < 2 s under QEMU; 3F adds a one-shot
PIT calibration that costs maybe 10 ms and a periodic timer
interrupt at 100 Hz that costs near-zero) →
docs/devlogs/2026-05-arsenal-framebuffer.md (specifically the
"What 3F looks like" section — APIC vector collision against the
existing IDT and the runqueue Mutex from inside IRQ context are
the two flagged surfaces) → arsenal-kernel/src/idt.rs (the IDT
is a Lazy<InterruptDescriptorTable> with eight CPU exception
handlers installed today; 3F adds vectors at >= 32 for LAPIC
timer + LAPIC spurious) → arsenal-kernel/src/sched.rs (yield_now
locks a spin::Mutex<VecDeque<Box<Task>>>; idle's comment at
sched.rs:250-254 explicitly anticipates "3F's preemptive timer
brings hlt back as a proper power-save") → arsenal-kernel/src/
paging.rs (the map_mmio helper from 3C is what maps the LAPIC
MMIO page at 0xfee00000 into the kernel-owned page tables) →
arsenal-kernel/src/serial.rs (the fan-out call to fb::print_str
uses try_lock; the IRQ handler's print path through here is the
exact reentrancy case that try_lock was designed for) →
Cargo.toml (3F adds no new deps — LAPIC is x86_64 instructions
+ MMIO, and the x86_64 crate already exposes the InterruptStack-
Frame and the MSR helpers we need) → git log --oneline -10 →
run the sanity check below → propose 3F-N commit shape (or argue
for a different decomposition) → wait for me to pick → "go 3f-N"
for code, "draft 3f-N" for paper deliverables.

Where the project is

  - main is in sync with origin/main at HEAD ef36a68
    (chore(ci): bump actions/checkout to v5). Working tree is
    clean except this file. CI armed; smoke takes ~1 s locally
    and ~2-3 min on ubuntu-24.04 runners (most of it is toolchain
    setup + cargo's release build of rustls + RustCrypto).
  - LOC: arsenal-kernel/src/ is 17 files now. Net Rust LOC in
    arsenal-kernel/src/ is ~4,200 post-3E (up from ~3,665 at end
    of 3D); fb.rs is ~215 lines, fb_font.rs is ~280 (mostly the
    4 KiB bitmap table). The kernel image is 1,474,744 bytes ELF
    / 19.3 MB ISO post-3E. 3F adds a few hundred lines of LAPIC
    bring-up + timer handler; no new vendored data.
  - Toolchain: nightly-2026-04-01 pinned in rust-toolchain.toml.
    3F uses no new nightly features beyond abi_x86_interrupt
    (already enabled).
  - Crates currently linked: limine 0.5, linked_list_allocator
    0.10, spin 0.10, x86_64 0.15, smoltcp 0.12, rustls 0.23,
    rustls-rustcrypto 0.0.2-alpha, getrandom 0.4 + 0.2. The
    x86_64 crate has the MSR / port-I/O / IDT helpers we need
    for LAPIC — no Cargo.toml changes for 3F.
  - Sentinels: smoke requires the eight listed above. 3F adds
    one — ARSENAL_TIMER_OK — after the timer-tick counter
    crosses a threshold (recommend N=10, which is 100 ms at
    100 Hz, easy to assert during boot before sched::init runs
    out of work).
  - HANDOFF.md (this file) is committed at the session start
    and rewritten between sessions; the prior contents are in
    git history (commit 5e53f54 — 3D's session opening, and
    cc36003 / e115095 are the STATUS pivots).

3F — LAPIC + preemption

The plan below is the kickoff proposal, not gospel. The user
picks the shape; deviations get justified before code lands.

Sub-candidate decomposition

  (3F-0) **PIC mask + LAPIC base discovery.** Mask the legacy
         8259 PIC by writing 0xFF to ports 0x21 and 0xA1 so the
         15 legacy IRQ lines stop competing for vectors 0x20-
         0x2F. Read the IA32_APIC_BASE MSR (0x1B); on QEMU q35
         this reports the canonical 0xfee00000 base with the
         enable bit and the BSP bit set. Map the 4 KiB LAPIC
         MMIO region via paging::map_mmio (the helper from 3C
         that already handles device BARs outside HHDM). Read
         the LAPIC ID register (offset 0x20) and version
         register (offset 0x30); log them. No interrupts armed
         yet. ~50 LOC; one commit: `feat(kernel): mask 8259 +
         map LAPIC MMIO`. Use **go 3f-0**.

  (3F-1) **LAPIC software enable + spurious vector.** Set bit 8
         of the spurious-interrupt-vector register (SVR, offset
         0xF0) — that's the "APIC enable" bit. Choose 0xFF as
         the spurious vector and install an extern "x86-
         interrupt" handler that does nothing but log on first
         entry (deduplicate via an AtomicBool so the log doesn't
         spam if spurious storms happen). Install the spurious
         handler in the IDT alongside the existing CPU exception
         entries. ~30 LOC; one commit: `feat(kernel): LAPIC
         software enable + spurious vector`. Use **go 3f-1**.

  (3F-2) **LAPIC timer + PIT calibration.** Calibrate the LAPIC
         timer frequency against the PIT (channel 2, gate enable
         via port 0x61 bit 0, count down from 0xFFFF for ~10 ms,
         read elapsed LAPIC ticks). Set divide config (offset
         0x3E0) to /16; that's the common choice and gives a
         comfortable range against typical bus frequencies (~3-4
         million LAPIC ticks per 10 ms calibration window).
         Configure the LVT timer entry (offset 0x320) with
         vector 0xEF in periodic mode. Set the initial count
         register (offset 0x380) to one tick's worth — at
         100 Hz that's `calibrated_ticks_per_10_ms * 1`. Install
         the timer handler: increment an `AtomicUsize TICKS`,
         write EOI (offset 0xB0, value 0). Log calibration
         results. ~80 LOC; one commit: `feat(kernel): LAPIC
         periodic timer + PIT-calibrated 100 Hz tick`. Use
         **go 3f-2**.

  (3F-3) **Idle gets hlt back; ARSENAL_TIMER_OK.** Restore the
         `hlt` in idle (sched.rs removed it in 3B-4 because
         cooperative-no-IRQ + hlt = stuck CPU). Now that the
         timer wakes hlt, idle becomes real power-save. After
         sched::init runs and ping/pong fires, a probe somewhere
         in the cooperative round-robin checks `TICKS.load() >=
         10` (100 ms worth of ticks) and prints
         ARSENAL_TIMER_OK. The sentinel proves the IRQ entered
         the handler — N tick observations are stronger evidence
         than a single one because spurious / NMI / external
         IRQs can't cluster on the timer vector. Smoke gains
         the sentinel; required-sentinel list grows to nine.
         ~25 LOC; one commit: `feat(kernel): idle hlt + ARSENAL_
         TIMER_OK`. Use **go 3f-3**.

  (3F-4) **STATUS.md refresh + 3F devlog.** STATUS flips 3F from
         "next" to "complete," 3G (`>` prompt + perf gate)
         becomes the next-session sub-block — and the last of M0
         step 3. Devlog at `docs/devlogs/2026-05-arsenal-apic.md`
         (or `-preemption.md` — pick at write time) records the
         PIT calibration trade-off, the soft-preemption vs hard-
         preemption call from below, and anything that surprised
         in xAPIC bring-up. Two commits: `docs(status): M0 step
         3F complete` and `docs(devlogs): Arsenal LAPIC +
         preemption`. Use **go 3f-4** for STATUS, **draft 3f-4-
         devlog** for the devlog.

Realistic session-count estimate: 3F-0 is half a session — the
MSR read + map_mmio + log call is template-shaped against 3C's
infrastructure. 3F-1 is half a session for the same reason. 3F-2
is one focused session; PIT calibration is where genuine bugs
hide (gate-enable bit ordering, off-by-one on the count
direction, missed transition from one-shot to periodic mode).
3F-3 is half a session if the soft-preemption shape below holds;
substantially more if hard preemption is chosen. Per CLAUDE.md
"~15 hours per week, multiply by ~2.3," call it 2 calendar weeks
for soft-preemption 3F, 3-4 weeks for hard-preemption.

Trade-off pairs to surface explicitly

  **Soft preemption vs hard preemption.** The most important
  call in 3F.
  (i) **Soft preemption.** Timer IRQ handler does nothing but
  increment TICKS and write EOI. The cooperative yield_now path
  is unchanged; idle becomes wake-on-IRQ via hlt. "Preemption"
  is observable as "the IRQ fired and incremented TICKS while
  idle was hlted." No context switch happens *from inside the
  IRQ handler*; cooperative tasks still yield manually.
  (ii) **Hard preemption.** Timer IRQ handler saves the
  interrupted context, calls into sched, swaps to the next
  runnable task, restores. Requires that the IRQ stack frame
  shape be compatible with the cooperative switch_to frame (or
  a translation layer), critical-sections that disable IRQs
  around runqueue Mutex operations, and a per-CPU "preempt
  disabled" counter to keep nested-IRQ-safe code paths
  preemption-free. ~3-5× the LOC of soft, and a much larger
  bug surface.
  Recommend (i) for 3F. The exit criterion — "idle hlts, the
  IRQ wakes it, ARSENAL_TIMER_OK after 10 ticks" — is fully
  satisfied by soft preemption. Hard preemption is a real
  feature surface but it's separately load-bearing for SMP
  (M0 step 4+); folding both into 3F bundles two design
  surfaces. Soft preemption now, hard preemption when SMP
  forces it. Document the deferral in the 3F-3 commit message.

  **xAPIC vs x2APIC.**
  (i) xAPIC — MMIO at 0xfee00000, 32-bit register accesses, well-
  documented in intel-sdm Vol. 3A §10.4. The universal baseline.
  (ii) x2APIC — MSR-based, 64-bit registers, requires CPUID 0x01
  ECX bit 21 + enable via IA32_APIC_BASE MSR bit 10. Faster (no
  MMIO round-trip), required for >255 CPUs (irrelevant pre-SMP).
  Recommend (i). M0 single-core has no x2APIC need; the MMIO
  cost is negligible at 100 Hz; the simpler code reads better.
  x2APIC revisits at M0 step 4 when SMP arrives and ICR
  performance starts to matter.

  **Periodic vs TSC-deadline timer mode.**
  (i) Periodic — LVT timer mode bit set, initial count register
  reloads on every expiration. Classic, simple, has well-known
  errata. Linux defaults toward TSC-deadline where available
  but periodic still ships everywhere.
  (ii) TSC-deadline — LVT timer mode bits set differently;
  expiration when TSC crosses a deadline MSR. Requires invariant
  TSC + CPUID 0x01 ECX bit 24. More accurate, lower wake-jitter,
  but more design surface.
  Recommend (i) for 3F. Periodic at 100 Hz is what M0 wants;
  TSC-deadline arrives when 3G's perf gate or a future scheduler
  rework actually needs the precision.

  **Calibration source.**
  (i) PIT — channel 2 in one-shot mode, count down from 0xFFFF
  at 1.193182 MHz, gate via port 0x61 bit 0. ~70 LOC, portable,
  the universal fallback. Linux's `calibrate_APIC_clock()` shape.
  (ii) HPET — clean, monotonic, but adds HPET surface (ACPI
  parsing, MMIO mapping, register layout).
  (iii) CPUID leaf 0x16 — gives bus frequency directly on Intel
  Sandy Bridge+; trivial when it works; not universal (AMD
  Zen 1-2 don't populate it consistently, QEMU's `cpu max` does).
  Recommend (i). PIT calibration is universal, costs maybe ~10 ms
  once at boot, and 3F doesn't justify HPET infrastructure.
  CPUID 0x16 as a fast-path fallback ("if leaf 0x16 reports a
  non-zero value, use it; otherwise PIT-calibrate") is a 10-line
  addition that could land in 3F or wait for 3G; I'd leave it
  for 3G unless the user wants belt-and-suspenders.

  **Tick rate.**
  (i) 100 Hz — Linux desktop default. 10 ms granularity, low
  IRQ overhead.
  (ii) 1000 Hz — Linux server default. 1 ms granularity, higher
  overhead.
  (iii) Lower (10 Hz / 1 Hz) — adequate for hlt-wake, not
  enough for a scheduling tick.
  Recommend (i). 100 Hz exercises preemption clearly during
  boot (10 ticks in 100 ms is well under sched::init's wall
  time) and matches what real workloads will expect at M1+.

  **TIMER_OK print site.**
  (i) From IRQ handler — directly inside the timer ISR after EOI.
  Routes through serial::write_str → fb::print_str → try_lock,
  which is exactly what 3E-3's try_lock was designed for. Safe
  in principle, but printing from IRQ context is a habit worth
  avoiding generally — it interacts badly with future printk-
  rate-limiting and per-CPU log buffers.
  (ii) From cooperative context — IRQ handler increments TICKS;
  a probe in main / sched / a dedicated probe task checks
  `TICKS.load() >= 10` and prints from non-IRQ context.
  Recommend (ii). Cleaner separation, and the assertion "TICKS
  crossed a threshold" is exactly as strong as "the IRQ fired
  10 times" because nothing else writes to TICKS.

  **Sub-candidate granularity.**
  (a) Four-commit shape above (PIC+map / SVR / timer+calibrate /
  idle+sentinel) plus 3F-4 STATUS+devlog. Bisect-rich, exactly
  what 3A/3B/3C/3D/3E used.
  (b) Combine 3F-0+3F-1 (PIC mask through SVR enable is the
  "make LAPIC ready" block).
  (c) Combine 3F-2+3F-3 (timer + sentinel land together).
  Recommend (a). The "PIC mask + LAPIC base" and "SVR + spurious
  handler" pieces are independently testable; folding them
  loses a bisect point against "did we mask the PIC correctly?"
  vs "did we get the SVR write wrong?" 3F-2 is the longest
  sub-commit but it's coherent — calibration and timer setup
  are inseparable.

Sanity check before kicking off

    git tag --list | grep field-os-v0.1   # field-os-v0.1
    git log --oneline -10                 # ef36a68, 9049f56,
                                          # e115095, 8aad04d,
                                          # fc5803f, 6d9a2a3,
                                          # b604f87, 4c5385b,
                                          # cc36003, db4625e
    git status --short                    # ?? HANDOFF.md (only,
                                          # while drafting this)
                                          # or clean once committed
    cargo build -p arsenal-kernel --target x86_64-unknown-none --release
                                          # clean, ~1.475 MB ELF
    cargo clippy -p arsenal-kernel --target x86_64-unknown-none --release -- -D warnings
                                          # clean
    cargo xtask iso                       # arsenal.iso ~19.3 MB
    ci/qemu-smoke.sh                      # ==> PASS (8 sentinels in ~1s)

Expected: HEAD as above; smoke PASSes in ~1 s with eight
sentinels. If smoke fails after the LAPIC bring-up lands, the
likely culprits are: vector collision (the spurious vector at
0xFF or timer vector at 0xEF aliasing onto something the IDT
already routes — verify with a printout of which IDT entries
are populated), miscalibrated tick value (zero initial count
= one-shot expiration immediately = unbounded IRQ storm), or
EOI omission (next IRQ blocked because the LAPIC's ISR bit
stays asserted). All three classes are detectable from serial
output — IRQ storm shows as endless TIMER_OK or
"unrecoverable triple fault"; missed EOI shows as exactly one
tick then silence.

Out of scope for 3F specifically

  - SMP. M0 step 4 brings up additional cores via INIT-SIPI-SIPI.
    3F's LAPIC bring-up is BSP-only.
  - IPIs (inter-processor interrupts). Need ICR, AP startup,
    per-CPU LAPIC state — all M0 step 4+.
  - x2APIC. xAPIC is what 3F ships.
  - TSC-deadline timer mode. Periodic only.
  - Hard preemption (IRQ handler context-switches the
    interrupted task). Soft preemption only — see trade-off
    pair above.
  - HPET. Not needed; PIT calibration suffices.
  - ACPI MADT parsing. Useful for SMP and for confirming the
    LAPIC base, but the MSR read covers M0's single-core case.
    MADT arrives with M0 step 4 or M1's LinuxKPI dependencies.
  - Interrupt-disable critical sections around runqueue Mutex.
    Soft preemption sidesteps this by never locking the
    runqueue from IRQ context; hard preemption would need it.
  - Per-CPU "preempt disabled" counter. Same — only needed
    under hard preemption.
  - Latency benchmarks / jitter measurement. 3G's perf gate
    covers boot time; per-IRQ jitter measurement is post-M0.

Permanently out of scope (do not propose)

  - Any unsafe block without a // SAFETY: comment naming the
    invariant the caller must uphold. CLAUDE.md hard rule.
  - Reverting any 3A / 3B / 3C / 3D / 3E commit. All landed
    and validated by smoke + CI.
  - Force-pushing to origin. Branch is in sync; preserve
    history.
  - Dropping the BSD-2-Clause license header from any new
    file.
  - Pulling a GPL crate into the kernel base.
  - Religious framing. CLAUDE.md hard rule.
  - Reintroducing HolyC. ADR-0004's discard is final.
  - Going back to stable Rust.
  - Skipping the build + smoke loop on a feat(kernel) commit.

Three notes worth flagging before you go

  1. **The 3E scroll-by-blit path is still untested in CI.**
     Boot output is currently ~30 serial lines × 16 px ≈ 480 px,
     comfortably inside the 800 px QEMU std-vga frame. 3F adds
     maybe 5-10 more log lines (LAPIC ID, version, calibration
     result, sentinel) which still keeps total well under 50
     lines. If the scroll path matters before 3G or M1, force
     it with a one-shot 60-line print somewhere — but per
     CLAUDE.md "nothing speculative," I'd leave that for when
     a real workload exercises it. Flagged for the M0 step 3
     exit retrospective.

  2. **The IDT is a Lazy<InterruptDescriptorTable>.** 3F adds
     LAPIC vectors after the Lazy has been forced — IDT.load()
     in idt::init() runs on first boot, and adding entries
     after that is a no-op against the already-loaded table.
     Two shapes work: (a) add the LAPIC entries inside the
     Lazy initializer before .load() runs (timer + spurious
     handlers as `extern "x86-interrupt" fn`s declared in
     idt.rs); (b) drop the Lazy in favor of a regular static
     Mutex<InterruptDescriptorTable> that 3F adds entries to.
     Recommend (a). It matches the existing pattern, keeps
     the IDT immutable post-init, and the new vectors are
     known at compile time. Document in the 3F-1 commit
     message that the IDT is now a one-shot static — late
     vector additions (e.g., device IRQs in M1) will need a
     different mechanism.

  3. **3F is the first sub-block where reality might argue with
     the spec.** 3A through 3E ran at HANDOFF reading speed —
     spec-driven, well-trodden ground. 3F's bug-prone surface
     is real: PIT calibration has gate-enable ordering and
     count-direction subtleties that bite first attempts; LAPIC
     spurious vectors fire surprisingly often during bring-up
     transitions; the soft/hard preemption call has cascading
     consequences if revisited mid-implementation. The pace
     may genuinely slow vs 3E. If a session ends mid-3F-2 with
     calibration not yet stable, that's normal — the 3D arc's
     three-sessions-on-rustls is the model. Don't push to
     "finish 3F-2 today" if calibration's first attempt didn't
     produce a sensible LAPIC frequency.

Wait for the pick. Do not pick silently. The natural first
split is 3F-0 + 3F-1 in one focused session ("LAPIC ready,
nothing armed yet"), 3F-2 in a session ("calibration + first
ticks observed"), 3F-3 in a session ("idle hlts, sentinel
lands"). Happy to do 3F-0 alone if you want to confirm the
IA32_APIC_BASE MSR shape before committing to the rest, or to
do soft/hard preemption as a separate up-front design pass
before any code lands. Your call.
