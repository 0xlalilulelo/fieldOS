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

### Active work

**No active sub-block yet.** Next session writes the M1
HANDOFF — break the milestone into sub-blocks, name the first
target, surface trade-offs (LinuxKPI shim depth, which driver
ports first, real-hardware CI strategy), and pick a starting
direction. Don't start coding M1 surface before the HANDOFF
lands.

Expected pace for M1: substantially slower than M0. The
ARSENAL.md month-9-to-month-24 budget assumes ~15 hr/week
part-time × 2.3 calendar multiplier, and M1 is genuine real-
hardware work — porting kernel C code, debugging on actual
silicon, NVMe / amdgpu / xHCI quirks that virtual hardware
cannot surface. The M0 post-pivot cadence was *initial-
condition* concentration; do not project it forward.

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
