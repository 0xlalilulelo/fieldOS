Kickoff for the next session — M0 step 4, SMP.

M0 step 3 closed cleanly on 2026-05-13 across seven sub-blocks
(3A frame allocator → 3B scheduler skeleton → 3C virtio → 3D
smoltcp + rustls → 3E framebuffer console → 3F LAPIC + soft
preemption → 3G `>` prompt + perf gate). HEAD is 1b316c9
(docs(status,devlogs): M0 step 3 complete, step 4 (SMP) next);
main is five commits ahead of origin/main (3G-0 PS/2 polling,
3G-1 shell + line editor, 3G-2 commands, 3G-3 perf gate,
3G-paper STATUS + devlog). Working tree clean. Smoke green at
**ten sentinels in ~430-600 ms locally**, ~45 s on
`ubuntu-24.04`; the perf gate observes 0 ms boot→prompt under
TCG (kernel boot fits within one 50 ms polling cycle of the
harness). ARSENAL.md's M0 boot-to-prompt < 2 s target is met
with massive headroom; `BOOT_BUDGET_MS=2000 ci/qemu-smoke.sh`
is the conformance check, default 3000 ms is the hosted-runner
slack.

Step 4 is the last major surface of M0. After it lands, the
M0 milestone tag (`M0-complete`) ships and the next plan
section is M1 (LinuxKPI shim, amdgpu KMS, NVMe / xHCI /
iwlwifi). STATUS.md § "Active work — M0 step 4 — SMP" already
names the surface: inter-processor interrupts via LAPIC ICR,
AP startup through the canonical INIT-SIPI-SIPI sequence
(ACPI MADT enumerates the processor entries), per-CPU LAPIC
state (TICKS per core, SPURIOUS_SEEN per core — the
single-CPU `AtomicUsize` shape from 3F splits per-core), hard
preemption discipline (IRQ-driven context switch from inside
the timer handler, rflags save/restore in `switch_to`,
per-CPU preempt-disable counter), and IOAPIC bring-up so
device IRQs route through the LAPIC (which unlocks IRQ-driven
keyboard, virtio MSI-X, and the rest of the device-IRQ
ecosystem that 3G-0's "polled only" decision deferred).

Step 4's design space — multi-core correctness, IRQ-context
safety, ACPI parsing — is **genuinely the biggest jump in M0**.
3A through 3G each ran in 1-2 calendar days; step 4 sub-blocks
will take longer per block and may need re-decomposition once
the AP trampoline / hard-preemption pieces actually land. The
sub-block proposal below is a first-pass split; expect to
revise after 4-0 lands and the design constraints settle.

read CLAUDE.md (peer concerns, Rust-only, BSD-2-Clause base,
build loop sacred, no unsafe without // SAFETY: invariant
comment) → STATUS.md (M0 step 3 complete, step 4 is the
active block; § "Active work — M0 step 4 — SMP" enumerates
the surface) → docs/plan/ARSENAL.md § "Three Concrete Starting
Milestones" → M0 ("basic SMP" is in the M0 deliverable list at
line 163; performance / security / usability gates from step 3
remain in force, and the M0-complete tag is what closes the
milestone) → docs/devlogs/2026-05-arsenal-prompt.md (M0 step 3
exit log, ends with "step 4 next" and names IOAPIC /
hard-preemption as the two specifically-deferred items from
3G) → arsenal-kernel/src/apic.rs (467 LOC post-3F; the single
LAPIC instance is currently a process-global; the TICKS and
SPURIOUS_SEEN AtomicUsize/AtomicBool become per-core; init() is
BSP-only and AP-init is the new entry — `apic::ap_init`) →
arsenal-kernel/src/cpu.rs (70 LOC; the per-CPU data area is
currently single-CPU; step 4 either widens this struct or
replaces it with a GS-base-indexed table — see trade-off pair
below) → arsenal-kernel/src/sched.rs (292 LOC post-3G; the
global runqueue `Mutex<VecDeque<Box<Task>>>` becomes either a
single global with IRQ-disabled critical sections or per-CPU
local + work-stealing — recommend single global at M0, see
trade-off) → arsenal-kernel/src/task.rs (112 LOC; the 32 KiB
stack budget from 3F-2 applies to AP boot stacks too — heap
allocation pattern is already established) → arsenal-kernel/
src/idt.rs (140 LOC; vectors 0xEF timer / 0xFF spurious wired
today; step 4 adds IRQ1 → some vector once IOAPIC routes it,
and rewrites the timer handler to dispatch through the
scheduler instead of just incrementing TICKS) → arsenal-kernel/
src/paging.rs (182 LOC; map_mmio already handles the LAPIC and
virtio MMIO cases; IOAPIC is one more MMIO region with the
same shape) → arsenal-kernel/src/kbd.rs (252 LOC; the polled
path stays as fallback during 4-5's transition, then the
poll-based shell loop is replaced by an event-source wait) →
arsenal-kernel/src/main.rs (400 LOC; the boot order is
serial → frames → paging → heap → ACPI (new at 4-0) →
SMP-bring-up (new at 4-2) → virtio → net → shell → sched::init.
sched::init becomes the BSP's final transition into the
scheduler; APs reach their own sched::ap_init via the
trampoline path) → ci/qemu-smoke.sh (REQUIRED_SENTINELS grows
by one or two depending on per-block sentinel choice — see
"Sentinel discipline" below; the QEMU command line gains
`-smp 4` (or `-smp 2` for tighter loop) so the BSP actually
has APs to bring up) → Cargo.toml (4-0 may add the `acpi` crate
from rust-osdev (Apache-2.0 / MIT, no_std) if we pick that
over a hand-rolled MADT walker — see trade-off pair below) →
git log --oneline -10 → run the sanity check below → propose
4-N commit shape (or argue for a different decomposition) →
wait for me to pick → "go 4-N" for code, "draft 4-N" for
paper deliverables.

Where the project is

  - main is at HEAD 1b316c9 (docs(status,devlogs): M0 step 3
    complete, step 4 (SMP) next). Working tree is clean
    except this file. main is five commits ahead of
    origin/main; this HANDOFF makes it six. Push to origin
    should bundle 1b316c9 through the 4-kickoff commit so the
    M0 step 3 closeout and the step 4 kickoff land together.
  - LOC: arsenal-kernel/src/ is 21 files post-3G with 5,281
    total lines (kbd.rs 252, shell.rs 233, apic.rs 467,
    sched.rs 292, task.rs 112, idt.rs 140, cpu.rs 70,
    paging.rs 182, main.rs 400, the rest tracking the M0 step
    3 surface). ELF release post-3G is 1,487,408 bytes;
    ISO 19.3 MB. Step 4 budget: ~600-900 lines net across the
    seven sub-commits below — 4-0 (ACPI) is the biggest single
    block at ~180-250 LOC, 4-2 (AP bring-up) is ~200-280 LOC
    counting the asm trampoline, the rest are 60-150 each.
  - Toolchain: nightly-2026-04-01 pinned in
    rust-toolchain.toml. Step 4 uses no new nightly features;
    `global_asm!` for the AP trampoline is the established
    3B pattern.
  - Crates currently linked: limine 0.5, linked_list_allocator
    0.10, spin 0.10, x86_64 0.15, smoltcp 0.12, rustls 0.23,
    rustls-rustcrypto 0.0.2-alpha, getrandom 0.4 + 0.2.
    Potential addition for 4-0: `acpi` 5.x or 6.x from
    rust-osdev (Apache-2.0 / MIT, no_std) — see trade-off; if
    we hand-roll MADT, no new deps for step 4.
  - Sentinels: smoke requires ten post-3G —
    ARSENAL_BOOT_OK / HEAP_OK / FRAMES_OK / SCHED_OK / BLK_OK /
    NET_OK / TCP_OK / TLS_OK / TIMER_OK / PROMPT_OK. Step 4
    adds at minimum one (ARSENAL_SMP_OK at 4-2, after all
    enumerated APs reach kernel-side init barrier); the
    aggressive shape would also add ARSENAL_ACPI_OK at 4-0
    and ARSENAL_IOAPIC_OK at 4-3 for bisect granularity. See
    "Sentinel discipline" below for the trade-off.
  - QEMU command line today: q35, 256 MiB RAM, virtio-blk +
    virtio-net + virtio-rng, `-cpu max`. **No `-smp` flag** —
    single CPU. Step 4's smoke needs `-smp 2` minimum, `-smp 4`
    to exercise the per-CPU storage and runqueue paths on
    something more than a trivial topology.
  - HANDOFF.md (this file) is being rewritten now for step 4.
    Prior contents (3G kickoff) are in git history at 61aace9.

Step 4 — SMP

The plan below is the kickoff proposal, not gospel. The user
picks the shape; deviations get justified before code lands.
Step 4's sub-block boundaries are the most likely to shift in
all of M0 — the AP bring-up path (4-2) and hard preemption
(4-4) each have multiple stable points where the work could
land in one commit or three.

Sub-candidate decomposition

  (4-0) **ACPI MADT parser.** Adds `arsenal-kernel/src/acpi.rs`.
        RSDP via Limine's `RsdpRequest` (Limine 0.5 provides
        the RSDP pointer directly — no EBDA / BIOS scan
        needed). Walk the RSDT (or XSDT if revision ≥ 2);
        find the MADT (signature "APIC"). Parse the MADT
        header (Local APIC address, flags) and enumerate the
        entries: Type 0 (Processor Local APIC) records the
        APIC IDs of all logical processors and a per-entry
        "enabled" / "online-capable" flag, Type 1 (I/O APIC)
        records IOAPIC ID + base address + GSI base, Type 2
        (Interrupt Source Override) records ISA-IRQ → GSI
        remappings (the IRQ1 keyboard case lives here on most
        firmware). Expose `acpi::cpus() -> &[CpuInfo]`,
        `acpi::ioapics() -> &[IoapicInfo]`, and `acpi::
        irq_override(isa_irq: u8) -> Option<GsiOverride>`.
        Sentinel ARSENAL_ACPI_OK emitted from acpi::init once
        the MADT walk completes and the BSP's APIC ID matches
        the LAPIC ID register from 3F. ~180-250 LOC if
        hand-rolled; ~50 LOC of glue if the `acpi` crate is
        adopted (but the crate brings ~3000 LOC of parser
        surface for M0 features we don't use — see trade-off).
        One commit: `feat(kernel): ACPI MADT parser`. Use
        **go 4-0**.

  (4-1) **Per-CPU storage via GS base.** Replaces single-CPU
        cpu.rs with per-CPU data accessed via `MSR_GS_BASE`
        (0xC0000101) pointing at a `CpuLocal` struct per core.
        BSP allocates one `CpuLocal` for itself in 4-1
        (single-CPU behavior unchanged); AP entry path in 4-2
        is what actually populates the others. The struct
        holds: APIC ID, scheduler runqueue head (today still
        a pointer into the single global runqueue from
        sched.rs — see 4-4 trade-off for per-CPU rq), TICKS
        (moved from apic.rs), SPURIOUS_SEEN (moved from
        apic.rs), preempt-disable counter (placeholder until
        4-4 wires it), current_task pointer (moved from
        AtomicPtr in sched.rs). Access via `gs:[offset]` in
        asm or via a `current_cpu()` helper that issues
        `rdgsbase` (or `mov rax, gs:[0]` for the self-pointer
        idiom). Refactor `apic::ticks()` and `apic::
        observe_timer_ok()` to read per-CPU. ~120 LOC plus
        ~40 LOC of refactor across apic.rs and sched.rs. One
        commit: `refactor(kernel): per-CPU data via GS base`.
        Use **go 4-1**.

  (4-2) **AP startup via INIT-SIPI-SIPI.** The real-mode
        trampoline. Carve out a fixed 4 KiB page under 1 MiB
        — recommend 0x8000 (vector 0x08 in the SIPI second
        byte field). Mark the page un-reclaimable in the
        frame allocator at boot (4-2's first detour will be
        confirming Limine doesn't hand 0x8000 back as usable —
        most QEMU configurations leave 0x6000-0x9FFFF
        reserved, but the Limine memory map is the
        authoritative source). The trampoline is hand-written
        16-bit → 32-bit → 64-bit code in a separate `.S`
        file or `global_asm!` block: enable PAE + LME + paging,
        load the BSP's CR3 (APs reuse the kernel's page
        tables), jump to 64-bit `ap_entry` in Rust.
        BSP-side: for each enumerated AP from 4-0's
        `acpi::cpus()`, allocate a 32 KiB kernel stack (heap
        Box like task::Task does), populate the AP's
        `CpuLocal` from 4-1, send INIT IPI via LAPIC ICR
        (vector field zero, delivery mode 0b101 INIT, target
        APIC ID), 10 ms PIT-polled delay, send first SIPI
        (vector 0x08, delivery mode 0b110 Startup), 200 µs
        delay, send second SIPI. AP reaches `ap_entry`, sets
        its CpuLocal from a parking-slot the BSP filled in,
        increments AP_ONLINE_COUNT, calls `apic::ap_init`
        (LAPIC software-enable, no timer arming yet — timer
        is BSP-only at M0; APs run scheduler ticks via IPI
        broadcast from the BSP at 4-4), then sched::ap_idle
        loop. BSP waits on AP_ONLINE_COUNT reaching `cpus().
        len() - 1`, then emits ARSENAL_SMP_OK. ~200-280 LOC
        (50 LOC trampoline asm, 80 LOC BSP-side INIT-SIPI-SIPI
        + IPI helpers in apic.rs, 60 LOC AP-side ap_entry +
        ap_idle in sched.rs, 30-90 LOC of debugging helpers
        that may or may not stay). One commit if it lands
        clean: `feat(kernel): AP startup via INIT-SIPI-SIPI`.
        Two commits if the trampoline lands separate from
        the BSP-side ICR programming. Use **go 4-2**.

  (4-3) **IOAPIC bring-up.** Map the IOAPIC MMIO region (base
        from 4-0's `acpi::ioapics()[0].base`, typically
        0xFEC00000) through `paging::map_mmio` — same dance
        as 3F-0 for the LAPIC. IOAPIC has two MMIO registers:
        IOREGSEL (offset 0x00, index register) and IOWIN
        (offset 0x10, data window); read register N by
        writing N to IOREGSEL then reading IOWIN. Read
        IOAPIC_VER (register 0x01) to learn redirection-table
        size (typically 24 entries on QEMU). Initial state:
        all entries masked (bit 16 set in the low half of each
        redirection-table entry pair). Provide `ioapic::
        program(gsi: u8, vector: u8, target_cpu: u8)` to
        unmask a specific GSI → vector → target APIC ID
        routing. Don't actually program any GSI at 4-3 — the
        keyboard's IRQ1 → GSI (via 4-0's irq_override map)
        gets programmed by 4-5 when it consumes the IOAPIC
        API. 4-3's smoke is structural ("IOAPIC mapped, ver
        register reasonable, all entries masked"); optional
        ARSENAL_IOAPIC_OK sentinel — see "Sentinel discipline"
        below for whether to add it. ~120 LOC. One commit:
        `feat(kernel): IOAPIC bring-up`. Use **go 4-3**.

  (4-4) **Hard preemption.** The hardest single block in
        step 4 and likely the one that decomposes further once
        the design lands. Three pieces in one commit if they
        all converge; three commits if they don't:
        (a) **rflags save/restore in `switch_to`.** Today's
        cooperative `switch_to` (sched.rs::switch_to via
        global_asm!) saves/restores callee-saved GP regs only;
        IF=1 propagates from idle's `sti` because we never
        touch rflags. Under IRQ-driven preemption, the timer
        handler runs with IF=0 (interrupt gate clears IF on
        entry), then context-switches mid-IRQ — rflags must
        be saved-with-IF=0 and restored-with-IF=0 on the prev
        task so when the prev task is re-scheduled it resumes
        with IF=0 too, with the IRET on handler exit (vs
        sysret or plain ret) being what restores IF=1 in the
        normal cooperative case. `pushfq` + `popfq` in
        switch_to, with care about ordering.
        (b) **IRQ-safe runqueue access.** Today's runqueue is
        `Mutex<VecDeque<Box<Task>>>` (spin::Mutex). An IRQ
        handler that tries to acquire the runqueue while
        cooperative code holds it deadlocks. Two paths:
        (i) wrap every runqueue lock in IRQ-disable (cli +
        save_flags / restore_flags + sti pattern, akin to
        Linux's `spin_lock_irqsave`); (ii) per-CPU local
        runqueues with no cross-core locking (work-stealing
        for load balance). Recommend (i) at M0 — single
        global runqueue with IRQ-saved locks is correct,
        well-understood, and the contention story doesn't
        matter at single-digit cores. (ii) is post-M0.
        (c) **Timer handler dispatches the scheduler.**
        Today's timer handler increments TICKS and writes
        EOI. Step 4's handler additionally calls a "consider
        preemption" path: if `current_task`'s time-slice has
        elapsed (TICKS - task.start_tick > SLICE_TICKS),
        push current onto runqueue, pop next, switch_to.
        Time-slice budget: 10 ms × 10 = 100 ms slice for
        single-core M0 (whole-second responsiveness with
        room for 10 cooperative tasks). The actual switch
        happens from IRQ context — switch_to must be
        IRQ-context-safe (it already is, since cooperative
        switches don't manipulate IRQ state, but verify).
        Per-CPU preempt-disable counter prevents reentry
        during the switch itself (counter > 0 ⇒ skip
        preemption this tick). ~200 LOC across sched.rs and
        apic.rs (timer handler). One commit: `feat(kernel):
        hard preemption discipline` (or three sub-commits if
        the design forces it: rflags / IRQ-safe lock / timer
        dispatcher). Use **go 4-4**.

  (4-5) **IRQ-driven keyboard.** Now that IOAPIC routes IRQs
        and the timer handler runs the scheduler, IRQ1 →
        IOAPIC GSI (1, or whatever the override says — usually
        the identity mapping; QEMU q35 ISA bus has the
        identity override) → LAPIC vector 0x21 wires through.
        Add an IDT entry for 0x21 (keyboard handler).
        Handler: read scancode from i8042 port 0x60, push
        into a per-CPU 256-byte ring buffer, write EOI to
        LAPIC. Shell task changes from `kbd::poll` in a tight
        loop to `kbd::recv_blocking` which yields if the ring
        is empty and gets re-scheduled when the handler
        flips a "data pending" flag (or, simpler, when the
        shell task's next preempt-tick comes around and the
        ring is non-empty). Replace shell.rs::run's
        cooperative-poll loop. Remove kbd's polled poll()
        path or keep as a fallback (recommend remove —
        polled was the 3G compromise; keeping both is dead
        code now that IOAPIC routes for us). ~80 LOC net.
        One commit: `feat(kernel): IRQ-driven keyboard`. Use
        **go 4-5**.

  (4-6) **STATUS + devlog + M0-complete tag.** STATUS flips
        step 4 from "next" to "complete," writes the step 4
        retrospective sub-section (the trampoline page
        reservation detour, the GS-base vs APIC-ID-array
        trade-off, the time-slice budget calibration, any
        IRQ-safe-lock surprises), and **closes M0**. The M0
        retrospective lives at the top of STATUS (the
        seven-step arc 3A-3G plus step 4, what posture
        changes carry forward to M1, what surprises showed
        up, how the calendar vs FTE-weeks projection held —
        ARSENAL.md's "0-9 calendar months" budget against
        actual months at M0-complete). Devlog at
        `docs/devlogs/2026-05-arsenal-smp.md` (or whatever
        month it actually lands) records the ACPI parser
        depth call (hand-rolled vs `acpi` crate), the AP
        trampoline page choice, the per-CPU storage
        mechanism choice, the hard-preemption design, and
        the "this is what the M0 milestone exit looked like"
        summary. Two-or-three commits: `docs(status): M0
        complete, M1 (LinuxKPI shim) next` and
        `docs(devlogs): Arsenal SMP + M0 milestone exit`,
        then the tag `git tag M0-complete` on the final
        SHA. Use **go 4-6** for STATUS, **draft 4-6-devlog**
        for the devlog, **go tag M0-complete** for the tag.

Realistic session-count estimate: 4-0 is one focused session
— ACPI table walking is mechanical but the first attempt at
parsing the MADT's variable-length entry list typically
misses a length-field subtlety. 4-1 is one session — the
GS-base mechanism is well-understood but the refactor surface
touches apic.rs and sched.rs, so the first build will surface
ordering issues (cpu_local() called before MSR_GS_BASE is
populated). 4-2 is **two-to-three** sessions; the AP
trampoline is the single hardest piece of code in all of M0,
and the bring-up loop typically takes two-three iterations to
get the timing right (INIT delay, first SIPI, 200 µs delay,
second SIPI — QEMU is forgiving but real hardware at M1 is
not). 4-3 is one session. 4-4 is **two sessions** at minimum;
the rflags / IRQ-safe-lock / timer-dispatcher trio interacts,
and any one of the three has at least one subtle failure
mode (re-entry, lock ordering, scheduler reentrance in the
preempted task's stack frame). 4-5 is one session. 4-6 is
one session including the devlog and the M0 tag. Per
CLAUDE.md's "~15 hours per week, multiply by ~2.3," step 4
is **3-5 calendar weeks** if the cadence holds, **5-8** if
4-2 or 4-4 surface a meaningful design rethink. ARSENAL.md
M0 budget runs calendar months 0-9; at calendar day 14
post-pivot we're at month 1 with months 2-3 plausibly
sufficient for step 4 if it doesn't break.

Trade-off pairs to surface explicitly

  **ACPI parser depth.**
  (i) **Hand-rolled MADT walker.** Limine gives the RSDP,
  we walk the RSDT/XSDT looking for the "APIC" signature,
  parse the MADT entries directly. ~180 LOC. Zero new
  dependencies. Scope is exactly what step 4 needs:
  enumerate CPUs, find IOAPICs, learn ISA-IRQ overrides.
  Doesn't grow with future features (FADT for ACPI shutdown,
  HPET for higher-res timing, MCFG for PCIe ECAM) — those
  would each add their own ~40-80 LOC parser at their own
  step.
  (ii) **Vendor the `acpi` crate from rust-osdev**
  (https://github.com/rust-osdev/acpi, Apache-2.0 / MIT,
  no_std). Battle-tested across multiple hobby OSes;
  parses every table M0 cares about and more. ~50 LOC of
  glue. But it brings ~3000 LOC of parser surface (AML
  interpreter dependencies for some tables, FADT details
  we don't use, HPET / MCFG / SRAT scaffolding) and a
  dependency relationship for the kernel base that
  CLAUDE.md §3's "BSD-2 base, vendored crates BSD/MIT/
  Apache/ISC/zlib/SIL-OFL" allows but doesn't require.
  Recommend (i). Step 4 needs precisely one table (MADT)
  parsed precisely once. 180 LOC is a clear, auditable,
  vendor-free piece of code in the kernel core. Switch to
  the `acpi` crate at M1 if FADT / HPET / MCFG arrive
  together; until then, hand-rolled is the smaller surface.

  **AP trampoline page location.**
  (i) **Fixed 0x8000** (SIPI vector field byte 0x08).
  Universal convention; the SeaBIOS / OVMF / Coreboot
  trio all leaves this region usable post-Limine handoff
  on QEMU. Easy to mark un-reclaimable in the frame
  allocator (one constant; one early `frames::reserve`
  call before the first allocation).
  (ii) **Dynamic — pick first usable page < 1 MiB from
  Limine memory map.** More portable across firmware
  weirdness, but the trampoline asm has to be position-
  independent or relocatable, and the SIPI vector byte
  has to be computed at AP-bring-up time.
  (iii) **0x1000 or 0x2000** (low memory, traditionally
  IVT / BIOS scratch on real hardware but usable on QEMU
  post-Limine).
  Recommend (i). 0x8000 is what Linux and FreeBSD both
  use; QEMU q35 + Limine leaves it untouched; the
  trampoline can be a fixed-position blob loaded via
  global_asm with `.section .ap_trampoline, "ax"` and
  a linker-script-placed page. Save the dynamic-pick path
  for if M1 surfaces a firmware that uses 0x8000.

  **Per-CPU storage mechanism.**
  (i) **MSR_GS_BASE pointing at a per-CPU CpuLocal
  struct.** Canonical x86_64 kernel pattern. `mov rax,
  gs:[offset]` is single-instruction per-CPU access.
  Linux, FreeBSD, illumos, Solaris all do this. Pairs
  cleanly with `swapgs` if/when userspace lands at M2.
  (ii) **`[CpuLocal; MAX_CPU]` array indexed by APIC ID.**
  Simpler; no MSR involved. But access requires
  computing or caching the current core's APIC ID, which
  is a LAPIC MMIO read (~25 cycles) — vs the GS-base
  approach's 1-cycle deref. And the APIC-ID-array pattern
  doesn't generalize to userspace per-thread storage.
  (iii) **Thread-local-style** via FS base (`MSR_FS_BASE`)
  with the same shape as GS. Less canonical for kernels;
  user space owns FS in the System V ABI.
  Recommend (i). The GS-base infrastructure scales from
  M0's per-core kernel state to M2's per-thread userspace
  state (with swapgs at the syscall boundary). Building
  the right shape now avoids a refactor later. The MSR
  write is one instruction per AP-bring-up.

  **AP secondary stack provisioning.**
  (i) **Heap-allocated by BSP before SIPI.** BSP calls
  `Box::leak(Box::new([0u8; 32 * 1024]))` per AP, puts the
  stack-top pointer into the AP's parking slot
  (CpuLocal->kernel_rsp or a transitional global before
  GS is set), AP picks it up immediately on entry. Matches
  task.rs's existing 32 KiB stack pattern.
  (ii) **Static array `[[u8; 32 * 1024]; MAX_CPU]`** in
  .bss. No allocator dependency at AP boot time. But fixed
  cap on CPU count and wastes BSS on configurations with
  fewer cores than MAX_CPU.
  Recommend (i). Heap is up by the time 4-2 runs (3A's
  heap landed at f947d04); the per-AP `Box::leak` is the
  same shape as task.rs's per-Task stack alloc. MAX_CPU
  isn't a real constraint at M0 (single-digit cores) but
  the allocator pattern composes with M1+'s "discover
  what's there" stance.

  **Trampoline page reservation.**
  (i) **Frame allocator carve-out** at boot. Add
  `frames::reserve(addr, count)` (or similar) called from
  main.rs before any other allocation, marking 0x8000
  reserved so 3A's free-list never hands it out. The frame
  is freed and reused for general allocation after 4-2
  finishes (all APs are past the trampoline by then).
  (ii) **Limine memory-map check.** Trust that Limine
  marks 0x8000 reserved or usable-after-bootloader-
  reclaim. Less code but more brittle — depends on Limine's
  memory-map shape staying stable.
  (iii) **Copy trampoline at AP-startup time** into
  whatever free page is available, fix up the SIPI vector
  byte. Most portable; most code.
  Recommend (i). Carve-out is a 10-LOC helper, deterministic
  across firmware versions, and frees cleanly post-bring-up.

  **Hard preemption mechanism in the timer handler.**
  (i) **Direct switch from IRQ handler.** Timer handler:
  EOI → check time-slice expiry → if expired, call
  sched::preempt() which does the runqueue dance + switch_to
  with IRQ-saved-flags. The switch happens *in IRQ context*
  on the prev task's stack; control returns to the next
  task via switch_to's saved state. Standard kernel design.
  (ii) **Flag-then-yield.** Timer handler sets
  current->need_resched = true and returns. The scheduler
  checks the flag at every cooperative entry point (yield_now,
  blocking calls). Simpler IRQ context, but fairness
  collapses if a task never enters the scheduler voluntarily
  — a tight CPU-bound loop never preempts.
  Recommend (i). True preemption is the M0 deliverable;
  flag-based is a degenerate fallback that doesn't satisfy
  the design. The IRQ-context switch is well-understood; the
  fragility surface is rflags handling and the runqueue lock
  (both addressed explicitly in 4-4).

  **Runqueue topology.**
  (i) **Single global `Mutex<VecDeque<Box<Task>>>`** with
  IRQ-saved locks (cli + lock + work + unlock + popfq).
  Correct at any CPU count; contended at high CPU count.
  (ii) **Per-CPU local runqueue + work-stealing.** No
  cross-core lock contention on the common path; balancing
  done by idle cores stealing from busy ones. ~150 LOC of
  extra surface.
  (iii) **Hybrid: per-CPU rq with periodic rebalance.**
  Linux CFS / EEVDF-style. Way out of M0 scope.
  Recommend (i). Single-core today, two-to-four-core
  through M2; contention isn't measurable. Per-CPU rq
  arrives at M1 or M2 once Stage's UI tasks need responsive
  scheduling under load. Today, simple-correct beats
  complex-fast.

  **IRQ vs polling for keyboard, final state.**
  (i) **Full switch to IRQ-driven** at 4-5. Remove
  `kbd::poll`; replace with `kbd::recv_blocking`. Cleanest
  end state; matches the M0 step 3 → step 4 trajectory.
  (ii) **Keep both, gated by a kernel flag.** Easier to
  fall back if 4-5's IRQ wiring surfaces a bug. But the
  fallback path is dead code from the moment it's not
  needed, and CLAUDE.md §"What you should not do" includes
  half-finished implementations.
  Recommend (i). If IRQ keyboard regresses, git-revert is
  the rollback. The polled path is fully preserved at
  6e2f823 (3G-0) for reference.

  **Number of APs to bring up.**
  (i) **All enumerated.** `acpi::cpus()` returns N; BSP
  brings up N-1 APs. The smoke harness sets `-smp 4` so
  N=4 in CI; locally users can vary.
  (ii) **Cap at MAX_CPU=8** (or 16, or 64). Bounds per-CPU
  static allocations on hypothetical massive machines. M0
  doesn't have static-sized per-CPU arrays (4-1 uses heap
  via Box::leak), so the cap is unmotivated.
  (iii) **Cap at 2 for M0** (BSP + 1 AP). Validates the
  bring-up path with the simplest possible topology;
  shipped scope-pull rather than a "we support N cores"
  story.
  Recommend (i) in code, (i) capped to `-smp 4` in CI smoke.
  No artificial cap; the BSP-AP boundary is the same shape
  whether N=2 or N=16, and exercising more makes the
  per-CPU and IPI paths see real concurrency.

  **Sentinel discipline.**
  (a) **Three new sentinels** — ARSENAL_ACPI_OK at 4-0,
  ARSENAL_SMP_OK at 4-2, ARSENAL_IOAPIC_OK at 4-3. Bisect-
  rich; each block has its own smoke witness; 4-3's
  structural-only check gets an explicit signal.
  (b) **One new sentinel** — ARSENAL_SMP_OK at 4-2.
  Smaller smoke footprint; 4-0 and 4-3 are validated by
  the fact that 4-2 succeeds (you can't bring up APs
  without ACPI enumeration; you can't route IRQs without
  IOAPIC). But bisect points are coarser.
  (c) **Two new sentinels** — ARSENAL_ACPI_OK at 4-0 and
  ARSENAL_SMP_OK at 4-2; skip IOAPIC's because 4-3 is
  structural-only and 4-5's IRQ-keyboard exercise validates
  it implicitly.
  Recommend (a). Per the 3F-2 task-stack retrospective:
  the more sentinels, the faster you isolate which sub-
  block regressed. Smoke harness already handles ten
  sentinels with the "all required present" wait-loop from
  3D; adding three more is mechanical.

  **AP timer state.**
  (i) **BSP-only periodic timer.** Only the BSP arms its
  LAPIC timer (as today). APs run scheduler ticks via
  IPI from the BSP at the same 100 Hz cadence (broadcast
  IPI on each BSP tick, or one-to-one IPI per scheduled
  AP). Simpler; one calibration; predictable global tick.
  (ii) **Per-CPU periodic timer.** Each AP calibrates its
  own LAPIC timer (same PIT calibration as 3F-2, but
  serialized across APs since PIT is a global resource).
  Independent clocks per core; better isolation for
  per-CPU scheduling decisions; cleaner separation. But
  cross-CPU time comparisons get fuzzy without TSC sync.
  Recommend (i) at 4-2 / 4-4. M0 doesn't need independent
  per-CPU clocks; the global tick is what time-slice
  expiry compares against. (ii) arrives if Stage's
  per-display vsync demands it at M2.

  **Sub-candidate granularity.**
  (a) **Seven-commit shape** above (ACPI / per-CPU / AP-
  bring-up / IOAPIC / preemption / IRQ-kbd / STATUS+devlog).
  Bisect-rich; matches the 3A-3G granularity.
  (b) **Five-commit shape** combining 4-0+4-1 (ACPI + per-CPU
  as "what's needed before we bring APs up") and 4-3+4-5
  (IOAPIC + IRQ-keyboard as "device IRQ story"). Tighter
  history; harder to bisect.
  (c) **Three-commit shape** — "SMP infrastructure" (4-0
  through 4-3), "hard preemption" (4-4), "IRQ keyboard"
  (4-5). Coarsest; highest blast radius per commit.
  Recommend (a). 4-2 and 4-4 are independently load-bearing
  enough to deserve their own commits even if 4-4 stays
  monolithic. Folding 4-1 into 4-2 is tempting but loses
  the "GS-base refactor under single-CPU" smoke checkpoint
  — a refactor commit that doesn't change behavior is
  exactly the kind of bisect point that matters when 4-2
  surfaces a subtle ordering bug.

Sanity check before kicking off

    git tag --list | grep field-os-v0.1   # field-os-v0.1
    git log --oneline -10                 # 1b316c9, b792ec2,
                                          # 7992d32, 287897f,
                                          # 6e2f823, 61aace9,
                                          # d940b59, 0323497,
                                          # 6c4b169, 41e7f8d
    git status --short                    # ?? HANDOFF.md (only,
                                          # while drafting this)
                                          # or clean once committed
    cargo build -p arsenal-kernel --target x86_64-unknown-none --release
                                          # clean, ~1.487 MB ELF
    cargo clippy -p arsenal-kernel --target x86_64-unknown-none --release -- -D warnings
                                          # clean
    cargo xtask iso                       # arsenal.iso ~19.3 MB
    ci/qemu-smoke.sh                      # ==> PASS (10 sentinels
                                          # in ~430-600 ms locally,
                                          # boot→prompt = 0 ms
                                          # under TCG)

Expected: HEAD as above; smoke PASSes with ten sentinels; perf
gate observes 0 ms boot→prompt (kernel boot inside one 50 ms
polling cycle of the harness).

If smoke fails after 4-0 / 4-2 / 4-4 land, the likely culprits
are: (a) MADT entry-length confusion — entries are variable-
length records with a one-byte length field; off-by-one in
the cursor advance loops past the end of the table; (b) AP
trampoline timing — INIT-SIPI-SIPI's 10 ms / 200 µs delays
must use PIT-polled wait, not the LAPIC timer (LAPIC timer is
*the thing being brought up*; circular dependency); a too-
short delay leaves APs at the SIPI-fetch instruction and
neither online nor reaching the parking slot; (c) AP page-
table sharing — if APs use the same CR3 as the BSP, any TLB-
unsafe page-table change during 4-2 corrupts AP execution
(safe at 4-2 because no page-table mutation runs during
INIT-SIPI-SIPI, but worth flagging when 4-4's per-CPU
preempt-disable counter lands and the runqueue may grow);
(d) IRQ-context lock acquisition in the timer handler before
4-4's IRQ-safe lock pattern is in place — old code path
(3F's timer handler that just increments TICKS) is safe but
4-4's preempt() inside the handler must not take the
runqueue lock without IRQ-disable; (e) GS-base set ordering
— `current_cpu()` called before MSR_GS_BASE is populated
reads garbage; AP entry must set GS-base in the first three
instructions of ap_entry before any current_cpu() call;
(f) rflags save/restore in switch_to — `pushfq` must come
*before* the callee-save GP pushes so the rsp arithmetic in
the existing global_asm! stays consistent; new prev/next
state slots in Task may need a Layout review.

Out of scope for step 4 specifically

  - Multi-socket / NUMA-aware allocation. ARSENAL.md M2 or
    later. M0 single-socket is the only target.
  - x2APIC mode. xAPIC (memory-mapped) is what 3F brought up
    and what step 4 keeps using. x2APIC adds an MSR-based
    interface for >255 logical CPUs; not M0 territory.
  - TSC synchronization across cores. APs use the BSP-driven
    100 Hz tick; TSC offset measurement is post-M0 if Stage
    or perf tooling needs it.
  - Hyperthreading topology awareness. Step 4 enumerates
    APIC IDs flat; HT sibling detection is a post-M0
    optimization for scheduling decisions.
  - CPU hotplug. Linux supports it; FreeBSD partly; M0 is
    boot-time discovery only.
  - Power management on APs. Idle hlt is the M0 stance from
    3F-3; APs do the same. No C-states, no P-states.
  - IPI broadcast for TLB shootdown. Required when M0+ kernel
    code starts mutating page tables that other CPUs have
    cached. M0 step 4 doesn't mutate page tables post-boot;
    when M1's LinuxKPI shim adds dynamic mappings, TLB
    shootdown lands then.
  - Inter-task signaling primitives (futex, condvar). M0
    cooperative + preemptive scheduler runs to-completion or
    voluntary-yield tasks only; signaling primitives arrive
    with the WASI Component scheduling at v0.5.
  - The kbd ring buffer at 4-5 grows past 256 bytes. At
    100 Hz preempt and human typing speed (~10 Hz), 256
    bytes is two-three orders of magnitude over what's
    needed; growing it is unmotivated.
  - SMP-aware `hw` output. The shell's `hw` command at 3G-2
    hardcodes "core count: 1". Step 4-6's STATUS update
    flips this to `acpi::cpus().len()` so the M0 step 3
    usability gate reflects reality post-SMP.

Permanently out of scope (do not propose)

  - Any unsafe block without a // SAFETY: comment naming the
    invariant the caller must uphold. CLAUDE.md hard rule.
  - Reverting any 3A / 3B / 3C / 3D / 3E / 3F / 3G commit.
    All landed and validated by smoke + CI.
  - Force-pushing to origin. Branch is in sync; preserve
    history.
  - Dropping the BSD-2-Clause license header from any new
    file. acpi.rs, ioapic.rs (if separated from apic.rs),
    and any ap_trampoline.S all need the header.
  - Pulling a GPL crate into the kernel base. The `acpi`
    crate from rust-osdev is dual-licensed Apache-2.0 / MIT,
    so it'd be admissible if we pick (ii) above — but
    confirm at adoption time.
  - Religious framing. CLAUDE.md hard rule.
  - Reintroducing HolyC. ADR-0004's discard is final.
  - Going back to stable Rust.
  - Skipping the build + smoke loop on a feat(kernel) commit.

Three notes worth flagging before you go

  1. **The trampoline page reservation has to land before any
     heap allocator activity** that would hand out 0x8000.
     3A's frame allocator runs at boot; its free-list is
     populated from Limine's memory map. If Limine reports
     0x8000 as USABLE (which on QEMU q35 it does in the
     low-memory region), the very first frame request might
     get 0x8000 back — at which point the AP trampoline
     can't be safely written there. 4-2's first commit (or a
     prep commit before 4-2) must call `frames::reserve(
     0x8000, 1)` before any other allocator user runs. The
     boot-order edit in main.rs is one line; the helper in
     frames.rs is ~15 LOC. Trivial code but easy to forget,
     and the failure mode is silent (AP startup just doesn't
     work, kernel hangs at SMP_ONLINE_COUNT-spin).

  2. **The hard-preemption block is where the cooperative
     scheduler from 3B starts paying its debt.** Today's
     `switch_to` is callee-save-only and runs on the prev
     task's stack with IF=1 propagated transparently from
     idle's `sti`. Under hard preemption, switch_to runs on
     the prev task's stack mid-IRQ with IF=0 and a partial
     saved-state from the interrupt gate; the rflags
     handling has to be right or the next task resumes with
     IF=0 silently (no immediate panic, just frozen
     preemption — the next timer tick never delivers).
     Recommend writing a deliberately-tight 4-4 test path:
     a `preempt_test` task that loops on a cycle counter
     and observes that the cycle gap between consecutive
     observations is bounded — if preemption is broken, the
     gap will be unbounded (the task runs to completion
     before yielding). This is a 30 LOC validation tool
     that catches the IF=0-silently-stuck case before it
     hides for weeks. It can live behind a `#[cfg(test)]`
     or just stay in main.rs as a smoke-witness alongside
     the existing ping/pong.

  3. **M0 closes when 4-6 lands.** ARSENAL.md month 0-9
     budget; calendar day 14 post-pivot at HEAD. Even at
     the slow end of step 4's 5-8-week estimate, M0
     completes inside calendar month 3-4 — well under the
     9-month budget. The right posture at that close-out
     is not "we beat the budget by 5 months" (calendar pace
     was abnormally fast at step 3; the budget was set
     against ~15 hr/week part-time and the post-pivot
     concentration is unrepresentative). The right posture
     is "the milestone landed; the next milestone, M1
     LinuxKPI, is genuinely calendar-scale because real
     iron requires real driver work." M0 retrospective in
     STATUS should note that the pace from 3A-3G was the
     fast-end outlier and project M1 against the ARSENAL.md
     baseline, not against M0's observed cadence.

Wait for the pick. Do not pick silently. The natural first
split is 4-0 in one focused session ("MADT walks; ACPI_OK
fires"), 4-1 in one session ("GS base populated on the BSP;
no behavior change but the refactor lands cleanly"), 4-2
across two-or-three sessions ("AP startup works; SMP_OK
fires"), 4-3 in one session ("IOAPIC mapped; structurally
correct"), 4-4 across one-or-two sessions ("hard preemption
discipline; preempt_test bounded"), 4-5 in one session
("IRQ keyboard replaces polled; the shell is still
responsive"), 4-6 in one session including the devlog and
the M0-complete tag. Happy to combine 4-0 + 4-1 if you want
the SMP prerequisites in one push, or to do 4-4 (the hard-
preemption block) first as a single-CPU correctness check
before bringing APs up at 4-2 — that would prove the
preemption design on the simpler topology before
multi-core makes the failure modes harder to bisect. Your
call.
