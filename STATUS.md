# STATUS

> What I am doing right now. Updated whenever the milestone changes,
> a major design decision lands, or a session leaves something
> mid-flight that future-me needs to know about.

## Current milestone

**Arsenal M1 — Real iron** *(months 9-24 per ARSENAL.md timeline)*

M0 closed on 2026-05-14 at the `arsenal-M0-complete` tag (the
unprefixed `M0-complete` tag is the Field OS arc's milestone,
preserved on commit `60e1a48`). M1's surface per ARSENAL.md:
LinuxKPI shim, amdgpu KMS, NVMe, xHCI, iwlwifi. First boot on
real Framework 13 AMD hardware is the milestone's exit criterion.

### M1 step plan (9 steps)

The milestone HANDOFF at git 9df4682 proposed an 8-step plan;
during M1 step 1 (NVMe) kickoff, the plan was restructured to
insert virtio-gpu (native Rust) as step 4 before amdgpu KMS.
Rationale: QEMU does not emulate amdgpu, so the M1 step 4
HANDOFF would otherwise have no CI substrate — amdgpu
development would proceed against real Framework hardware
only, with no per-commit smoke validation. virtio-gpu (~1000-
1500 LOC, no shim dependency) gives the kernel a KMS-capable
GPU driver that QEMU smokes on every commit; the GPU/display
abstraction stabilizes against virtio-gpu before amdgpu has
to consume it. Pushes M1 from ~62 to ~67 weeks at part-time
pace; still inside the 15-month ARSENAL.md budget.

1. **NVMe native Rust** (~880 LOC actual; well under ARSENAL.md's
   ~5K LOC ceiling and slightly above the HANDOFF's 600-800 target
   range). **Complete (2026-05-14, one calendar day, six
   sub-blocks).** Devlog at
   [`docs/devlogs/2026-05-arsenal-nvme.md`](docs/devlogs/2026-05-arsenal-nvme.md).
2. **LinuxKPI shim foundation + first tiny inherited driver.**
   ARSENAL.md's "single largest engineering task" — budget
   12-20 weeks. **Active.**
3. **xHCI USB.** Native Rust vs LinuxKPI port — evaluate at
   step kickoff.
4. **virtio-gpu native Rust.** KMS-capable GPU driver for
   QEMU CI; stabilizes the kernel-side GPU/display
   abstraction. ~1000-1500 LOC.
5. **amdgpu KMS via LinuxKPI shim.** The headlining driver;
   ports against the abstraction step 4 stabilized.
6. **iwlwifi + mac80211 via LinuxKPI.** Wireless.
7. **First boot on real Framework 13 AMD hardware.** Real-
   iron exit criterion. ARSENAL.md performance gate
   (cold boot to login < 8 s) asserted here.
8. **Slint app on software-rendered framebuffer.** First
   "modern UI" — runs on virtio-gpu in CI, amdgpu on real
   hardware.
9. **M1 retrospective + arsenal-M1-complete tag.**

### M1 step 1 retrospective (NVMe — 2026-05-14)

Six sub-blocks (1-0 through 1-5) across one calendar day,
four feat commits plus the paper. Foundation work (PCIe
MSI-X capability parsing + dynamic IDT vector allocation +
`pub unsafe fn pci::bar_address` + `pci::config_write32`)
will be consumed by every later M1 driver — xHCI at step 3,
virtio-gpu at step 4, amdgpu via the shim at step 5,
iwlwifi via the shim at step 6.

Step 1 sub-commits:
- `dd9f4a6` PCIe MSI-X capability + dynamic IDT vector
  allocation (1-0). Foundation step.
- `bc6ddac` NVMe device discovery + BAR mapping (1-1).
- `061e3cb` NVMe controller reset + admin queue + Identify
  (1-2). The spec-rich block; NVMe 1.4 §7.6.1 sequence,
  admin SQ/CQ, Identify Controller + Identify Namespace
  via polled completion.
- `a75541c` NVMe I/O queue + sector 0 read, polled (1-3).
  The cathartic block — `ARSENAL_NVME_OK` first fires
  through the polled path.
- `dcd9ed1` NVMe MSI-X interrupts (1-4). Converts the I/O
  queue to interrupt-driven completion. End-to-end pipeline:
  idt::register_vector → pci MSI-X table programming →
  Create-I/O-CQ with IEN=1+IV=0 → MSI delivered to IDT
  vector 0x40 → IRQ_COUNT bump → cooperative drain.
- (this commit pair) M1 step 1 retrospective + step 2
  kickoff (1-5).

Honest cadence note: the M1 step 1 HANDOFF estimated 4-6
weeks at part-time pace. Step 1 took ~8 focused hours on
2026-05-14 (same calendar day M0 closed). This is the
*post-pivot concentration window*, not the sustainable
ARSENAL.md cadence. The M1 milestone-level budget (~67
part-time weeks across 9 steps; ARSENAL.md months 9-24)
does NOT shrink because step 1 finished fast — variance is
now concentrated in the harder later steps (shim, amdgpu,
real-hardware bring-up) where it always lived. The right
posture is gratitude for the speed and continued discipline
against the budget. The devlog has the full framing.

Posture changes carrying to M1 step 2:
- IDT is now `spin::Mutex<InterruptDescriptorTable>` (not
  `Lazy`). `register_vector(handler) -> u8` is the public
  API for dynamic vector allocation; LinuxKPI's IRQ-
  registration shim will route through it.
- `pci::config_read32` and `pci::config_write32` are
  available as `pub(crate) unsafe fn`. The shim's `pci_*`
  API mapping will need them.
- `pci::bar_address` is `pub unsafe fn`; LinuxKPI's
  `pci_resource_start` / `pci_iomap` shim sees through it.
- DMA buffers come from `frames::FRAMES.alloc_frame()`
  (4-KiB page-aligned by construction). LinuxKPI's
  `dma_alloc_coherent` shim will be a thin wrapper.

### Active work

**M1 step 2 — LinuxKPI shim foundation + first inherited
driver.** Step-2 HANDOFF landed at `5fb0382` with a 7-sub-
block decomposition; **M1-2-0 complete** (2026-05-14, four
commits: HANDOFF + ADR-0005 + empty `linuxkpi/` workspace
member skeleton + lockfile). The shim is ARSENAL.md's
"single largest engineering task" of M1 — 12-20 part-time
weeks budgeted, ~14-17 weeks projected per the step
HANDOFF's calendar arithmetic, morale-load-bearing because
the shim doesn't ship anything user-visible on its own.
Step-2 HANDOFF discipline: one shim API surface lands +
compiles + has a smoke test, repeat — bisect-rich
checkpoints week-over-week.

M1-2-0 structural picks (ADR-0005):
- Single Cargo workspace member `linuxkpi/` (peer to
  arsenal-kernel + xtask). Three-crate split deferred to a
  successor ADR when amdgpu confirms it.
- `cc` build-dep crate compiles inherited C from
  `linuxkpi/build.rs` at M1-2-4. Host Linux Kbuild rejected
  on macOS-CI grounds.
- Minimal hand-curated header subset under
  `vendor/linux-6.12/`; per-driver expansion at each step
  kickoff via `find-include-graph` audit.
- Directory-based GPL/BSD-2 boundary, build-system
  enforced. `linuxkpi/*` = BSD-2; `vendor/linux-6.12/*` =
  upstream Linux SPDX preserved.
- Bidirectional FFI; hand-written `linuxkpi/include/
  shim_c.h`. cbindgen deferred (MPL-2.0 attention; revisit
  at ~1500 lines or M1 step 5).
- Synchronous module init at M1; deferred-path stubs
  (schedule_work, queue_work, kthread_run) panic-on-call.

**Next sub-block: M1-2-1** — foundational shim API surface.
C-FFI integer typedefs + printk + pr_* family + kmalloc /
kfree honoring GFP_KERNEL/GFP_ATOMIC + mutex / spinlock /
atomic_t + container_of / BUG_ON / WARN_ON + jiffies /
msleep / udelay + copy_*_user stubs that BUG_ON. Self-test
fires from kernel main, emits `ARSENAL_LINUXKPI_OK`.
~600-800 LOC shim Rust + ~80 LOC `shim_c.h`. One commit:
`feat(linuxkpi): foundational types + printk + slab + locks
+ atomics`. HANDOFF estimates 2-3 focused sessions / ~3
calendar weeks.

First inherited driver target (re-confirmed at step-2
HANDOFF): virtio-balloon (~600 LOC inherited C, pure
virtio-bus interaction). Lands at M1-2-5.

Expected pace for M1 overall: substantially slower than
M0 or M1 step 1. The ARSENAL.md month-9-to-month-24 budget
assumes ~15 hr/week part-time × 2.3 calendar multiplier,
and the harder steps (shim, amdgpu, iwlwifi, real-hardware
boot) are genuine real-hardware work — porting kernel C
code, debugging on actual silicon, driver quirks that
virtual hardware cannot surface. The post-pivot
concentration window has not closed yet, but the right
projection remains the ARSENAL.md cadence.

## Last completed milestone

**Arsenal M0 — Boot and breathe.** Tag `arsenal-M0-complete`
on commit (assigned at tag time); 2026-04-29 → 2026-05-14
(~16 calendar days post-pivot, well under the ARSENAL.md
0-9 month budget). Six steps:

1. **Step 1 — toolchain + Cargo + xtask + smoke**
   (2026-04-29 → 2026-04-30, pre-pivot reset). Re-establishes
   the build loop after the pivot from C: cross-compile to
   `x86_64-unknown-none`, Limine vendored, `cargo xtask iso`
   producing a bootable image, `ci/qemu-smoke.sh` running it
   to `ARSENAL_BOOT_OK`. Devlog at
   `docs/devlogs/2026-04-m0.md`.

2. **Step 2 — IDT + GDT/TSS + paging** (2026-05-04 → 2026-05-07).
   GDT + TSS with three IST stacks for #DF / #NMI / #MC. IDT
   with the standard fault handlers + `int3` self-test. Deep
   page-table clone takes ownership of every level. Devlogs:
   `2026-05-arsenal-first-boot.md`, `-paging.md`.

3. **Step 3 — memory, scheduler, virtio, network,
   framebuffer, preemption, prompt** (2026-05-09 → 2026-05-13).
   Seven sub-blocks 3A-3G. Frame allocator + heap free path +
   reclaim of `BOOTLOADER_RECLAIMABLE`; cooperative scheduler
   + Task + cooperative context switch; PCI scan + virtio-modern
   transport + virtio-blk + virtio-net; smoltcp DHCP + TCP +
   rustls TLS 1.3; Limine framebuffer + 8×16 Spleen glyphs +
   serial→fb mirror; LAPIC software-enable + spurious + PIT-
   calibrated 100 Hz periodic + soft preemption; PS/2 polled +
   shell + commands + perf gate + the `>` prompt. Devlogs:
   `2026-05-arsenal-mm-complete.md`, `-scheduler.md`,
   `-virtio.md`, `-network.md`, `-framebuffer.md`,
   `-preemption.md`, `-prompt.md`.

4. **Step 4 — SMP, hard preemption, IRQ keyboard**
   (2026-05-14, single day). Six sub-blocks. ACPI MADT walker;
   per-CPU GS-base storage; Limine MpRequest AP bring-up;
   IOAPIC mapped + masked; hard preemption (rflags
   save/restore in switch_to + IrqGuard + timer-IRQ dispatch
   to scheduler); IRQ-driven keyboard. Closes M0. Devlog at
   `2026-05-arsenal-smp.md`.

   Step 4 sub-commits:
   - `f3f431e` ACPI MADT parser (4-0)
   - `b70f0f2` per-CPU data via GS base (4-1)
   - `b6b3785` SMP bring-up via Limine MpRequest (4-2)
   - `78b38e2` IOAPIC bring-up (4-3)
   - `6a69383` hard preemption discipline (4-4)
   - `e2057de` IRQ-driven keyboard (4-5)
   - (this commit) M0-complete: STATUS + devlog + tag (4-6)

### ARSENAL.md M0 gates — all met

- **Performance.** Boot to prompt under 2 s under QEMU.
  Observed at M0 exit: 96 ms (was 0 ms pre-4-4 cooperative-
  only; the preempt witness shares CPU now). 30× margin
  against the BOOT_BUDGET_MS=3000 default; 20× against the
  ARSENAL.md verbatim 2 s. Asserted in CI as wall-clock
  between `ARSENAL_BOOT_OK` and `ARSENAL_PROMPT_OK`.

- **Security.** Zero `unsafe` Rust outside designated FFI
  boundaries. Every `unsafe` block in
  `arsenal-kernel/src/` carries a `// SAFETY:` comment
  naming the invariant the caller upholds. No driver-shim /
  vendored-crate-base boundaries exist yet at M0 (those
  arrive with M1's LinuxKPI shim).

- **Usability.** Prompt is keyboard-navigable + shows a
  hardware summary. `help` lists commands. `hw` produces
  the summary (CPU brand string, core count, RAM
  free/total, LAPIC + virtio devices). Line editor handles
  backspace destructively on serial. Manual verification
  under `-display gtk` recorded in `2026-05-arsenal-smp.md`.

### M0 posture changes carrying to M1

1. **Kernel task stacks are 32 KiB, not 16 KiB.** 3F-2's
   rustls + smoltcp poll-loop callchain overflowed 16 KiB;
   M1's LinuxKPI bridge will hit deeper chains and should
   budget against the new header.

2. **MMIO pages need explicit `paging::map_mmio` before
   access.** Limine's HHDM covers USABLE /
   BOOTLOADER_RECLAIMABLE / FRAMEBUFFER / ACPI memory only —
   not device MMIO, not legacy BIOS ROM. 3C learned for
   virtio BARs; 3F for the LAPIC; 4-0 for ACPI tables (and
   the HANDOFF forgot). M1 drivers will `map_mmio`
   reflexively.

3. **APs come up with Limine's PML4 loaded, not ours.**
   `smp::ap_entry`'s first three instructions load
   `paging::kernel_pml4_phys()` into CR3. Any future AP
   entry-point variants (CPU hotplug, kdump) must do the
   same before touching post-clone mappings.

4. **`frames::reclaim_bootloader` is dead-but-preserved.**
   Re-enable when APs move off Limine's pre-allocated stacks
   onto kernel-owned scheduler stacks. Cost of leaving it
   off: ~2.5 MiB on QEMU 256 MiB. Re-enable trigger is
   wherever in M1 the AP scheduler integration lands.

5. **`CpuLocal` layout is stable across BSP and APs.** The
   `self_ptr` at offset 0 is the load-bearing invariant for
   `current_cpu()`'s `mov gs:[0]`. New per-CPU fields
   append; never insert at the head.

### M0 carry-forwards (deferred, not blocking)

- **fb-visible cursor + fb-side destructive backspace** —
  `shell.rs` header flags both since 3G-1. Polish for M2
  when Stage's cursor model arrives.

- **Perf gate measurement resolution.** 50 ms polling
  catches regressions of one polling cycle or more — plenty
  for the 2000 ms ARSENAL.md target but sub-50 ms drift is
  invisible. Future fix is a serial-line timestamping pipe.

- **TCP / TLS first-run flake on hosted runners.** Python
  listeners race with QEMU's slirp on cold runs. Cold-cache
  CI hits this; local determinism is fine.

### M0 cumulative shape (final)

- 22 `.rs` files under `arsenal-kernel/src/` (acpi, apic,
  cpu, fb, fb_font, frames, gdt, heap, idt, ioapic, irq,
  kbd, main, net, paging, pci, rand, sched, serial, shell,
  smp, task, virtio, virtio_blk, virtio_net).
- ~5,900 lines of Rust kernel code + ~10 KB of font tables
  + tiny smoke harness.
- ELF release ~1.52 MB, ISO ~19.3 MB.
- 13 required sentinels in `ci/qemu-smoke.sh`:
  `ARSENAL_BOOT_OK`, `_HEAP_OK`, `_FRAMES_OK`, `_BLK_OK`,
  `_NET_OK`, `_SCHED_OK`, `_TCP_OK`, `_TLS_OK`, `_TIMER_OK`,
  `_ACPI_OK`, `_IOAPIC_OK`, `_SMP_OK`, `_PROMPT_OK`.
- Smoke pass time at M0 exit: ~1.2-1.5 s locally on QEMU
  TCG with `-smp 4`. Boot→prompt 96 ms (well under
  BOOT_BUDGET_MS).

## Earlier milestones

**Field OS PoC v0.1** (tag `field-os-v0.1`, commit `dffe259`,
2026-05-08). M3 step 6-5: per-eval cctrl reset, the HolyC REPL
working in QEMU under `make repl-iso`. Encoder byte-equivalent
with GAS across a 63-instruction corpus; JIT path landed `X`
on serial through a six-step pipeline (parse → codegen →
encode → relocate → commit → invoke); the M3 5-line
exit-criterion session worked in miniature. ~6,274 LOC of
base-system C across 56 files at the high-water mark.

The C kernel is preserved at the tag; `git checkout
field-os-v0.1` resurrects it. Bringing it back into `main`
would require reverting Phase B's removal commit.

**M2 — Memory Management** (2026-05-05 → 2026-05-06, four
commits, +1,814 LOC). Tag `M2-complete` on commit `6cd9855`.
PMM + VMM + slab.

**M1 — Boot to Long Mode** (2026-04-30 → 2026-05-04, four
commits, +700 LOC). Tag `M1-complete` on commit `c211cf8`.
GDT + TSS, IDT, BGA framebuffer with 8×8 font, "Hello,
Field" rendered.

**M0 — Tooling and Bootstrap** (2026-04-29 → 2026-04-30, six
commits, ~190 LOC base-system C, ~21,000 LOC vendored). Tag
`M0-complete` on commit `60e1a48`. Cross-GCC toolchain,
Limine vendored, `make iso` producing a bootable ISO.

These tags remain in place; the work is preserved at
`field-os-v0.1` along with everything else from the Field OS
arc. Arsenal milestone tags are prefixed `arsenal-`
(`arsenal-M0-complete`, future `arsenal-M1-complete`, etc.)
to coexist cleanly with the Field OS arc's unprefixed
`M0-complete` / `M1-complete` / `M2-complete` tags. Both
reference distinct commits on distinct project arcs.
