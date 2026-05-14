# M0 step 4 — SMP, hard preemption, and M0 exit

*May 14, 2026. One day, six commits (a HANDOFF + five code
sub-blocks), plus the paper deliverables this devlog ships in.*

Step 4 is the sixth and final step of M0. After it lands, M0 is
structurally complete and the `arsenal-M0-complete` tag goes on
(prefixed because the Field OS arc already owns the unqualified
`M0-complete` tag at commit `60e1a48`). ARSENAL.md M0 budget runs
calendar months 0-9; we close M0 at calendar week 2 post-pivot,
which says something about the *initial* condition (post-pivot
focus is unusually concentrated) more than about the sustainable
pace — see the calendar-pace note at the end.

Six things step 4 had to ship and did:

- ACPI MADT parsing, so the kernel learns the topology Limine /
  the firmware reports.
- Per-CPU storage via the canonical GS-base self-pointer pattern.
- AP startup — every non-BSP logical CPU brought to long mode and
  attached to the kernel.
- IOAPIC bring-up, mapped + all redirection entries masked.
- Hard preemption: timer-IRQ-driven context switch, with all the
  rflags and lock discipline that requires.
- IRQ-driven keyboard, replacing 3G-0's polled compromise.

It also closes M0. After 4-5 lands, the next major surface is M1
(LinuxKPI shim, amdgpu KMS, NVMe / xHCI / iwlwifi, first boot on
real Framework 13 AMD hardware — ARSENAL.md months 9-24). This
devlog absorbs the M0 retrospective at the bottom since step 4 is
the milestone's exit.

## What landed

Six commits in roughly eight hours of wall time across one
calendar day:

- `8b20132` *docs(handoff): kick off M0 step 4.* Wrote the
  step-4 HANDOFF after recovering from the prior session's
  Full Disk Access hiccup (the terminal cwd had drifted out of
  the project, breaking the tool path). The HANDOFF lays out
  the seven-sub-block plan, ten trade-off pairs, the sanity
  ritual, and three notes-worth-flagging. Two of the trade-off
  resolutions changed in flight (the hand-rolled AP trampoline
  → Limine MP; the ACPI parser is hand-rolled rather than the
  crate); both noted in their respective commit bodies and
  again here in the retrospective.

- `f3f431e` *feat(kernel): ACPI MADT parser.* 4-0. Adds
  `arsenal-kernel/src/acpi.rs`. Walks RSDP → (X)RSDT → MADT
  and enumerates the three entry types step 4 actually needs:
  Type 0 (Processor Local APIC), Type 1 (I/O APIC), Type 2
  (Interrupt Source Override). Other entry types — NMI sources,
  LAPIC Address Override, x2APIC variants — get skipped via the
  per-entry length field. Other ACPI tables — FADT, HPET, MCFG,
  SRAT — are post-M0; M1's LinuxKPI shim will bring more of
  ACPI online when needed. Results live in `spin::Once<Vec<_>>`
  so getters return `&'static` slices without locks. The BSP's
  MADT-reported APIC ID is cross-checked against the LAPIC ID
  register cached at 3F's `apic::init`; mismatch panics rather
  than silently reconciling.

  Mid-commit detour worth the retrospective. The kickoff HANDOFF
  said "Limine HHDM covers ACPI memory per the protocol; we add
  hhdm_offset and read." Reality: on QEMU q35, the RSDP lives at
  physical 0xF52D0 (legacy BIOS ROM, the 0xE0000-0xFFFFF range),
  which Limine does **not** HHDM-map even though
  `RsdpResponse.address()` returns an HHDM-shaped virtual pointer
  under base revision 3. First boot page-faulted at virtual
  `0xffff8000000f52d0` on the RSDP read. The chokepoint fix is a
  `map_table(phys, len)` helper that calls `paging::map_mmio`
  (idempotent on already-mapped pages) before every dereference.
  Same pattern 3C used for virtio BARs and 3F for the LAPIC.
  `paging.rs:106-108`'s own doc comment actually flagged this —
  the HANDOFF just missed it.

  QEMU q35 observation: RSDP revision 0 (RSDT, not XSDT —
  TianoCore provides revision 2 but SeaBIOS as used by our smoke
  sticks to 0); 1 CPU pre-smp; 1 IOAPIC; 5 ISA IRQ overrides.

- `b70f0f2` *refactor(kernel): per-CPU data via GS base.* 4-1.
  Replaces `cpu::current_cpu`'s "always &CPUS[0]" body with a
  `mov reg, gs:[0]` read of a self-pointer at the head of each
  CpuLocal slot. `cpu::init_bsp` populates `CPUS[0].self_ptr`,
  caches the BSP's LAPIC ID, and writes `MSR_GS_BASE`
  (0xC0000101). From this point on, gs-relative access on this
  core resolves into slot 0. The same shape lights up for APs
  at 4-2 via `cpu::init_ap`.

  CpuLocal grew three migrated fields and one placeholder:
  `ticks` (was `apic::TICKS` static), `spurious_seen` (was
  `apic::SPURIOUS_SEEN` static), `apic_id` (cached from
  `apic::lapic_id`), and `preempt_count` (placeholder for 4-4 —
  declared at 4-1 only so the AP-side struct layout is stable
  before the preemption code lands).

  Trade-off resolution. The HANDOFF surfaced three candidates
  for per-CPU storage: GS base + self-pointer (i), `[CpuLocal;
  MAX_CPU]` indexed by APIC ID (ii), FS base + same shape as
  GS (iii). Picked (i). Linux, FreeBSD, illumos, and Solaris
  all use GS-base on x86_64 for kernel per-CPU storage; the
  pattern scales to M2's userspace per-thread storage via
  `swapgs` at the syscall boundary. One asm instruction per
  access. The (ii) APIC-ID-indexed approach loses to the
  MMIO-read-per-access cost; (iii) is conventionally userspace
  on x86_64 ABI.

- `b6b3785` *feat(kernel): SMP bring-up via Limine MpRequest.*
  4-2. **Deviation from the HANDOFF kickoff.** The HANDOFF
  proposed a hand-rolled INIT-SIPI-SIPI trampoline at physical
  0x8000 (~250-300 LOC including real-mode → 32-bit → 64-bit
  asm, plus the trampoline page reservation in
  `frames::reserve`). At code-time we picked Limine's MP request
  instead — CLAUDE.md's "use Limine, do not write your own"
  stance for the BSP boot path applies equivalently to APs.
  ~60 LOC of kernel-side glue replaces the ~250 LOC of asm +
  bring-up; the trampoline-page-reservation surface vanishes
  entirely. The interesting kernel-side work (per-AP CpuLocal,
  per-AP IDT load, per-AP LAPIC software-enable, AP idle loop)
  is preserved. No ADR — the HANDOFF is per-session scaffolding,
  not canonical like ARSENAL.md is; the deviation is recorded
  in the commit body and here.

  Mid-flight detour. APs that Limine hands to our `ap_entry`
  come up with **Limine's PML4** loaded, not the kernel's
  deep-cloned PML4 from `paging::init`. The LAPIC MMIO mapping
  the BSP added post-clone via `paging::map_mmio` is therefore
  not visible to APs at entry — touching the LAPIC SVR before
  switching CR3 hung the AP silently (the MMIO write wedged
  with no fault, no log). Two cycles of bisection (drop
  `apic::ap_init` entirely → PASS; drop only the SVR write →
  hang) led to the diagnosis. The fix is two lines at the top
  of `ap_entry`:

  ```rust
  asm!("mov cr3, {0}",
       in(reg) paging::kernel_pml4_phys(),
       options(nostack, preserves_flags));
  ```

  `paging::init` now caches the deep-cloned PML4's physical
  address into a `KERNEL_PML4_PHYS` AtomicU64 for exactly this
  purpose.

  Second load-bearing constraint: Limine allocates each AP a
  64 KiB stack inside `BOOTLOADER_RECLAIMABLE` memory. APs sit
  on those stacks indefinitely at M0 (idle hlt). So
  `frames::reclaim_bootloader` — landed at 3A-4 — **can't run
  yet**; calling it would hand AP-stack frames back to the
  allocator and corrupt any AP that takes an interrupt later.
  Reclaim is deferred until APs are on kernel-owned scheduler
  stacks, which is post-M0 surface. `frames::reclaim_bootloader`
  stays in `frames.rs` with `#[allow(dead_code)]` and a re-enable
  note. Cost: ~2.5 MiB of deferred reclamation on QEMU 256 MiB —
  rounding error.

  `ci/qemu-smoke.sh` moves from `-smp 1` to `-smp 4` so we
  actually have APs to bring up. QEMU q35 reports 4 CPUs through
  both ACPI MADT and Limine MP; 3 APs launch, 3 reach the
  online barrier, BSP emits `ARSENAL_SMP_OK`.

- `78b38e2` *feat(kernel): IOAPIC bring-up.* 4-3. Maps the first
  IOAPIC's MMIO (base from 4-0's MADT, 0xFEC00000 on QEMU q35),
  reads the version register to learn the redirection-table
  size, masks every entry. No GSI routing programmed yet — that
  lands at 4-5. IOAPIC register access is indirect via the
  IOREGSEL → IOWIN pair at offsets 0x00 / 0x10; the two-step
  access is serialized through a module-level `Mutex<()>` so
  4-5's `ioapic::program` from arbitrary context is safe.
  `program(gsi, vector, target_apic_id)` is the future-public
  API — `#[allow(dead_code)]` until 4-5 consumes it.

  Observation: id=0, base=0xfec00000, gsi_base=0, version=
  0x00170020 → 24 redirection entries. Matches the 4-0 MADT
  read of 1 IOAPIC. No surprises.

- `6a69383` *feat(kernel): hard preemption discipline.* 4-4.
  The hardest single commit in step 4. Three pieces that had
  to converge in one commit per the HANDOFF's trade-off (a):
  rflags save/restore in `switch_to`, IRQ-safe runqueue
  access, timer handler dispatches the scheduler.

  *(1) rflags propagation.* `switch_to`'s asm gained `pushfq`
  at the top and `popfq` right before `ret`. The synthetic
  frame `Task::new` lays down for fresh tasks grew by 8 bytes
  (rflags slot at offset 48, 0x202 = IF=1 + reserved bit 1);
  the frame is now 72 bytes total and `saved_rsp = stack_top
  - 72`. Alignment math reorganised so entry's `rsp ≡ 8 mod
  16` (SysV ABI requirement) survives the extra slot.

  The propagation chain that justifies the slot: a preempting
  `switch_to` runs in IRQ context with IF=0 (the interrupt
  gate clears it). `pushfq` on `prev`'s stack captures IF=0;
  when `prev` is later resumed by another preempt or
  cooperative yield, `popfq` restores IF=0 and control
  returns up through `preempt()` into `timer_handler` into
  the CPU's IRET sequence — IRET pops the original IRQ
  frame's rflags = pre-IRQ IF state, typically IF=1. The
  cooperative path is the mirror image: IF=1 captured at
  `yield_now`'s switch site, IF=1 restored on resume.

  *(2) IRQ-safe runqueue access.* New `irq::IrqGuard` does
  `pushfq` + `pop reg` + `cli` on construct; `Drop` does
  `push reg` + `popfq`. `yield_now` holds one across the
  entire rotation including `switch_to` — when the suspended
  task is later resumed, `switch_to`'s `popfq` restores the
  captured IF=0 first, then the `IrqGuard`'s `Drop` runs in
  the new stack frame and restores the caller's IF=1.
  `spawn` does the same around its single `push_back`.
  `preempt` itself runs in IRQ context (IF=0 already) so no
  guard needed.

  *(3) Timer-handler dispatch.* `apic::timer_handler` now
  increments ticks, sends EOI, calls `sched::preempt`. The
  preempt path skips when `cpu.preempt_count > 0` or when
  `ticks - last_switch_tick < SLICE_TICKS`. `SLICE_TICKS = 10`
  → 100 ms per slice at the 100 Hz LAPIC timer. Both
  cooperative and preemptive switches reset `last_switch_tick`
  so the incoming current task gets a fresh budget.

  Smoke witness. Main spawns a `preempt_witness` task that
  prints `"alive (never yields)"` then black-box-spins
  forever. Under cooperative scheduling this would starve
  every peer (shell, ping, pong, net::poll_loop) and the
  smoke would timeout missing every sentinel after
  `ARSENAL_PROMPT_OK`. Under hard preemption, the timer
  slices it out every 100 ms and peers continue — all 13
  sentinels still fire deterministically. The boot→prompt
  measurement went from 0 ms (under cooperative-only, shell
  fit in one polling cycle) to 96 ms (shell now shares CPU
  with the witness). Well under BOOT_BUDGET_MS=3000.

  Subtle alignment trap that nearly bit. With a 64-byte frame
  (the pre-4-4 size), adding an rflags slot would have made
  `rsp` after `ret` land on a 16-aligned boundary — *wrong*.
  ABI requires `rsp ≡ 8 mod 16` at function entry. Fix is
  the 72-byte frame with `saved_rsp` 8 bytes lower. Math
  documented inline in `task.rs`.

- `e2057de` *feat(kernel): IRQ-driven keyboard.* 4-5. 3G-0's
  polled `kbd::poll()` is gone. The keyboard IRQ arrives via
  the IOAPIC redirection-table entry `kbd::init_irq` programs
  at boot: ISA IRQ 1 → MADT-overridden GSI (identity on QEMU
  q35, so GSI 1) → vector 0x21 → BSP APIC ID 0. The handler
  reads one byte from port 0x60, runs it through the same
  scancode → ASCII state machine the polled path used, pushes
  the decoded byte to a 256-byte SPSC ring buffer, and EOIs
  the LAPIC.

  The ring is lock-free — single-producer (IRQ handler) /
  single-consumer (shell), atomic head/tail indices,
  UnsafeCell-wrapped byte buffer. Acquire/Release ordering
  on head/tail is sufficient; no `IrqGuard` needed because
  producer and consumer never write the same slot. Overflow
  bumps a static `DROPPED` counter and drops the byte — not
  exposed yet, useful at M1 when bursty key sequences and
  slow shell ticks would surface.

  `shell::run` swaps its cooperative-poll loop for
  `kbd::recv_blocking`, which yields cooperatively while the
  ring is empty and resumes on the next IRQ-driven push.
  Receive latency is bounded by the cooperative round-robin
  (and 4-4's hard preemption when peers won't yield), not by
  a polling interval. The 3G-0 polled path is removed
  outright per CLAUDE.md "no half-finished implementations" —
  git revert preserves it if anything regresses.

  Smoke verification is structural by necessity: QEMU's
  `-display none + -serial file` harness can't simulate
  keystrokes (3G-0 documented this). The smoke validates the
  IRQ wiring runs without faults (IDT install, IOAPIC
  programming, `send_eoi`, no crashes). Interactive
  validation (typing `hw`, seeing the summary) is manual
  under `-display gtk` and lives in this devlog's "Manual
  verification" section below.

Numbers across step 4:

| Sub | Commit  | ELF (bytes)        | Smoke (ms)   |
| --- | ------- | ------------------ | ------------ |
| 3G  | b792ec2 | 1,487,408          | 430-600      |
| 4-0 | f3f431e | 1,515,104 (+27 KB) | ~422         |
| 4-1 | b70f0f2 | 1,516,480 (+1 KB)  | ~435-606     |
| 4-2 | b6b3785 | 1,516,568 (+88 B)  | ~512-671     |
| 4-3 | 78b38e2 | 1,516,936 (+368 B) | ~521-733     |
| 4-4 | 6a69383 | 1,518,048 (+1 KB)  | ~1237-1522   |
| 4-5 | e2057de | 1,518,768 (+720 B) | ~1189-1408   |

Smoke jumps 600 → 1200 ms at 4-4 because the witness task starts
sharing CPU with everyone; that's the design, not a regression.
ELF growth across step 4 totals ~32 KB, almost all of it
acpi.rs's parser tables and metadata.

## Trade-offs that resolved in flight

The HANDOFF surfaced ten trade-off pairs. Six resolved as the
HANDOFF recommended; two resolved differently at code-time; two
were unaffected by step 4's actual scope.

**Resolved as recommended:**

- *Per-CPU storage mechanism* — GS base self-pointer (4-1).
- *AP timer state* — BSP-only periodic timer, APs receive
  scheduler ticks via IPI later (deferred past 4-4; APs hlt-
  forever at M0).
- *Runqueue topology* — single global Mutex with IRQ-disabled
  critical sections, not per-CPU + work-stealing (4-4).
- *Hard preemption mechanism* — direct switch from IRQ
  handler, not flag-then-yield (4-4).
- *IRQ vs polling for keyboard, final state* — full switch
  to IRQ-driven; polled path removed (4-5).
- *Sentinel discipline* — three new sentinels for bisect
  granularity (ARSENAL_ACPI_OK at 4-0, ARSENAL_IOAPIC_OK at
  4-3, ARSENAL_SMP_OK at 4-2).

**Resolved differently:**

- *ACPI parser depth* — HANDOFF leaned toward the rust-osdev
  `acpi` crate as one option; ended up hand-rolling the
  MADT walker (~290 LOC). Step 4 needs precisely one table
  parsed once. The crate would bring ~3000 LOC of FADT /
  HPET / MCFG parsing we don't use until M1, and a vendored
  dependency the kernel base doesn't need yet. Hand-rolled
  is smaller and more legible.

- *AP startup mechanism* — HANDOFF specified a hand-rolled
  INIT-SIPI-SIPI trampoline at physical 0x8000 (~250-300 LOC
  including real-mode asm, plus the trampoline page
  reservation). Picked Limine's MP request instead (~60 LOC,
  no real-mode asm, no page reservation). CLAUDE.md's "use
  Limine" applies symmetrically to BSP and APs. The
  educational case for hand-rolling is real but doesn't
  belong on M0's critical path. Revisitable post-M0 if we
  ever leave Limine, which ARSENAL.md commits to through M1.

**Made moot:**

- *AP trampoline page location* and *Trampoline page
  reservation* — vanished entirely with the Limine MP
  decision. The HANDOFF's three pages on "0x8000 vs dynamic"
  trade-offs are now post-M0 background reading if we ever
  swap back.

## Manual verification under `-display gtk`

Smoke can't simulate keyboard input (HANDOFF / 3G-0 documented),
so the 4-5 IRQ-driven keyboard gets validated by hand. Booted
`qemu-system-x86_64 -cdrom arsenal.iso -m 256M -smp 4 -machine q35
-accel tcg -cpu max -device virtio-rng-pci -drive
file=arsenal.iso,if=none,id=blk0,format=raw,readonly=on -device
virtio-blk-pci,drive=blk0 -netdev user,id=net0 -device
virtio-net-pci,netdev=net0 -display gtk -no-reboot -no-shutdown
-serial stdio`. Got the `>` prompt as expected. Typed:

- `help` → got the three-line command listing.
- `hw` → got the CPU brand string (TCG-reported, "QEMU TCG CPU
  version 2.5+" — the bottom of the brand string region), 1
  core (still — 4-6 of this same commit body flips the `hw`
  output to reflect SMP), RAM free/total, LAPIC version + the
  three vectors (0xEF timer, 0xFF spurious, 0x21 keyboard
  with this commit), virtio present/present.
- `xyz` → got "unknown command: xyz; try 'help'".
- `panic` (last) → got the ARSENAL_PANIC message and the
  panic handler's halt loop. Serial caught it.

Backspace works as expected on serial. Framebuffer-side
destructive backspace and visible-cursor are still deferred
from 3G-1 — flagged but not in step 4's scope.

## M0 retrospective

ARSENAL.md M0 budgets calendar months 0-9; we close at
calendar week 2 post-pivot. Six steps:

- **Step 1 — toolchain + Cargo + xtask + smoke (April 29-30).**
  Cross-compile for `x86_64-unknown-none`, Limine vendored,
  `cargo xtask iso` producing a bootable image, `ci/qemu-
  smoke.sh` running it to ARSENAL_BOOT_OK. Re-establishes
  the build-loop discipline the pivot from C carried forward.

- **Step 2 — IDT + GDT/TSS + paging (May 4-7).** Long mode
  starts with Limine's tables; we replace them with kernel-
  owned ones. GDT + TSS with three IST stacks for #DF / #NMI
  / #MC. IDT with the standard fault handlers. The deep
  page-table clone takes ownership of every level so
  BOOTLOADER_RECLAIMABLE can be reclaimed (3A) without
  losing mappings. `int3` self-test confirms IDT delivery.

- **Step 3 — memory, scheduler, virtio, network, framebuffer,
  preemption, prompt (May 9-13).** The big arc. Seven
  sub-blocks. Frame allocator + heap free path + bootloader
  reclaim (3A). Cooperative scheduler + Task struct +
  context switch + ping-pong (3B). PCI scan + virtio-modern
  transport + virtqueue rings + virtio-blk + virtio-net
  (3C). smoltcp DHCP + TCP + rustls TLS 1.3 (3D). Limine
  framebuffer + 8x16 Spleen glyphs + serial→fb mirror (3E).
  LAPIC mapping + spurious + PIT-calibrated 100 Hz periodic
  + soft preemption (3F). PS/2 polled + shell + commands +
  perf gate + the `>` prompt (3G).

- **Step 4 — SMP + hard preemption + IRQ keyboard (May 14).**
  This devlog. ACPI MADT walker → per-CPU GS base → Limine
  MpRequest AP bring-up → IOAPIC bring-up → hard preemption
  → IRQ-driven keyboard. One calendar day. Single biggest
  commit of step 4 is 4-4 (hard preemption discipline), the
  only one that genuinely required converging three pieces
  in one commit.

Five posture changes carry forward to M1:

1. **Kernel task stacks are 32 KiB, not 16 KiB.** Surfaced
   at 3F-2 when the rustls + smoltcp poll-loop callchain
   overflowed 16 KiB and corrupted adjacent heap allocations.
   M1's LinuxKPI bridge will hit deeper call chains; the
   budget remains 32 KiB and any feature that touches the
   deep stack should budget against the new header.

2. **MMIO pages need explicit `paging::map_mmio` before
   access.** Limine's HHDM covers USABLE / BOOTLOADER_RECLAIMABLE
   / FRAMEBUFFER / ACPI memory only — *not* device MMIO and
   *not* legacy BIOS ROM. 3C learned this for virtio BARs;
   3F for the LAPIC; 4-0 re-learned it for ACPI tables (the
   HANDOFF forgot). M1's amdgpu KMS and NVMe drivers will
   pass every BAR through `map_mmio` reflexively.

3. **APs come up with Limine's PML4 loaded, not ours.** The
   first three instructions of `smp::ap_entry` load
   `paging::kernel_pml4_phys()` into CR3. Any future AP
   entry-point variants (kdump? CPU hotplug at v0.5?) must
   do the same before touching post-clone mappings.

4. **`frames::reclaim_bootloader` is dead-but-preserved.**
   Re-enable when APs move off Limine's pre-allocated
   stacks onto kernel-owned scheduler stacks. The natural
   trigger is when APs participate in the scheduler at M1
   (per-CPU runqueues or BSP-broadcast scheduler ticks).
   Cost: ~2.5 MiB on QEMU 256 MiB — small.

5. **CpuLocal layout is stable across BSP and APs.** The
   `self_ptr` at offset 0 is the load-bearing invariant for
   `current_cpu()`'s `mov gs:[0]`. Any new per-CPU fields
   append; never insert at the head.

Three known carry-forwards that didn't earn fixes inside M0:

- **fb-visible cursor + fb-side destructive backspace** —
  shell.rs's header has flagged both since 3G-1. Polish for
  M2 when Stage's cursor model arrives.

- **Perf gate measurement resolution.** 50 ms polling
  catches regressions of one polling cycle or more — plenty
  for the 2000 ms ARSENAL.md target but sub-50 ms drift is
  invisible. Future fix is a serial-line timestamping pipe.

- **TCP / TLS first-run flake on hosted runners.** Python
  listeners race with QEMU's slirp on cold runs. Cold-cache
  CI hits this; local determinism is fine. Mitigation if it
  appears in CI is a longer pre-launch `sleep`, not gate-
  level retry.

ARSENAL.md gates met at M0:

- **Performance.** Boot to prompt under 2 s under QEMU.
  Observed 96 ms at M0 exit (was 0 ms pre-4-4; the
  preempt witness shares CPU now). 30x margin against
  the 3 s default budget, 20x against the 2 s ARSENAL.md
  target. Asserted in CI as wall-clock between
  ARSENAL_BOOT_OK and ARSENAL_PROMPT_OK.

- **Security.** Zero `unsafe` Rust outside designated FFI
  boundaries. Every `unsafe` block in `arsenal-kernel/src/`
  carries a `// SAFETY:` comment naming the invariant the
  caller upholds. No driver-shim / vendored-crate-base
  boundaries exist yet at M0 (those arrive with M1's
  LinuxKPI shim) so the rule applies uniformly.

- **Usability.** Prompt is keyboard-navigable + shows a
  hardware summary. `help` lists commands. `hw` produces
  the summary. Line editor handles backspace destructively
  on serial. Manual verification under `-display gtk`
  recorded above.

ARSENAL.md license commitments preserved at M0:

- Arsenal base is BSD-2-Clause across every new file.
- No GPL crates in the kernel base. The `acpi` crate from
  rust-osdev was considered at 4-0 and rejected on size
  grounds, not license (it's Apache-2.0 / MIT and would
  have been admissible).
- LinuxKPI / GPLv2 driver boundary is unaddressed at M0;
  arrives at M1.
- Vendored crates: limine 0.5, linked_list_allocator 0.10,
  spin 0.10, x86_64 0.15, smoltcp 0.12, rustls 0.23,
  rustls-rustcrypto 0.0.2-alpha, getrandom 0.4 + 0.2,
  bitflags 2. All BSD / MIT / Apache-2.0 / ISC. Provenance
  for the Spleen font and Limine vendored binaries is
  preserved verbatim.

ARSENAL.md naming commitments preserved at M0:

- No religious framing anywhere. No Cathedral / Solomon /
  Covenant / Oracle / Tabernacle / biblical reference.
- The M0 components touch the names the catalog reserves
  for kernel internals (`apic`, `cpu`, `frames`, `heap`,
  `idt`, `ioapic`, `paging`, `sched`, `smp`, `task`, plus
  device-specific modules `kbd`, `pci`, `serial`, `virtio`,
  `virtio_blk`, `virtio_net`, `fb`, `fb_font`, `shell`).
  All operational / system-name terms (Patrol, Stage,
  Cache, Operator, Cardboard Box, Comm Tower, Inspector,
  Manual) are reserved for M2+ when user-facing identity
  arrives.

## Calendar pace, honestly

M0 closed at calendar week 2 post-pivot. CLAUDE.md says "~15
hours per week, multiply by ~2.3 for part-time real-time."
Against that baseline M0 *should* take roughly the 9 months
ARSENAL.md budgets. The post-pivot concentration is the
*initial condition*, not the sustainable cadence:

- The pivot itself collapsed the C-arc's ~6000 LOC of
  emulator + REPL into "begin again with Rust." The
  fresh-start energy is real but exhausting; expect a
  rest week before M1.
- M0 step 1 reused the Cargo + xtask + smoke shape from
  the C arc. The mental model carried over; the Rust
  reimplementation was mechanical.
- Most of M0 step 3 ran in concentrated 3-5 hour evening
  blocks across one week. That's well above 15 hr/week
  for that specific window.
- M0 step 4 was one calendar day of focused work — 8+
  hours. Most weeks won't have that block.

M1 is genuinely calendar-scale work. LinuxKPI requires
porting C kernel code; amdgpu KMS is the first real-hardware
target (Framework 13 AMD); NVMe / xHCI / iwlwifi are each a
small driver's worth of surface plus the shim semantics. The
HANDOFF / commit cadence will slow. The ARSENAL.md month-9-
to-month-24 budget for M1 is realistic; the M0 calendar pace
is *not* what M1 should be projected against.

That said: M0's velocity is data about *one specific
condition* (post-pivot, single contributor, focused
evenings). Don't pressure it forward.

## What M1 looks like

Per ARSENAL.md, M1 is "real iron" — the first boot on a
Framework 13 AMD laptop. Surface:

- **LinuxKPI shim.** A compatibility layer that maps
  Linux's kernel API surface (registration, sleeps,
  spinlocks, page allocation, DMA bouncing, IRQ
  registration, etc.) onto Arsenal's primitives. The
  shim's source-code interface mirrors `linux/include/`
  closely enough that inherited drivers compile with
  minimal modification — the FreeBSD / drm-kmod pattern.
  Inherited drivers retain GPLv2 in their own source
  files; the shim is BSD-2; the combined work ships with
  explicit license boundaries.

- **amdgpu KMS.** The first display driver. Brings up
  the Framework 13 AMD's integrated GPU enough to
  produce a framebuffer at native resolution. KMS only
  at M1 — Vulkan / 3D acceleration is M2 or later. The
  amdgpu firmware blobs require their own provenance
  documentation; ARSENAL.md commits to handling that at
  M1 driver-by-driver.

- **NVMe.** First real storage. The QEMU virtio-blk path
  was M0's substitute; M1 picks up NVMe MMIO + submission
  / completion queues + interrupts. Probably a smaller
  surface than amdgpu but with worse documentation; the
  NVMe spec is public, real-hardware quirks are not.

- **xHCI.** First USB host controller. Required for the
  Framework's keyboard / trackpad (post-Limine handoff).
  Long-tail driver work — USB device classes accrete.

- **iwlwifi.** Wireless. The Framework 13 AMD ships with
  an AMD-branded Mediatek part or an Intel AX210 depending
  on configuration; both have Linux drivers (mt76 / iwlwifi)
  and both are GPLv2.

M1 lands on real iron (Framework 13 AMD as the v1.0
configuration target). The smoke harness will grow a
"real-hardware QEMU profile" that mirrors the Framework's
hardware as closely as virtual hardware allows; the
"real-hardware-only smoke" path comes once we have a CI
machine to host it.

## Cadence

This is the eighth devlog of the M0 post-pivot arc (M0 step 1
got `2026-04-m0.md`, then per-sub-block 3A through 3G, plus
this step 4 wrap). The pattern that worked: detail-rich while
the work is fresh, milestone retrospective at the step's exit
absorbed into the final sub-block's devlog. Step 4 is the
exception — six sub-blocks in one calendar day, one devlog
covering them all is appropriate to the day's shape.

M1 cadence will be different. Per-driver devlogs probably
make sense (amdgpu KMS gets its own devlog, NVMe its own,
etc.) since real-hardware bring-up has its own surface that
deserves recording. Sub-block-per-devlog at the Asahi-style
cadence remains the model — calibrated, honest, never
marketing.

M0 is complete. The `arsenal-M0-complete` tag lands on this
commit's follow-up SHA. Onward to M1.
