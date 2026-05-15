Kickoff for M1 step 2 — LinuxKPI shim foundation + first
inherited driver.

M1 step 1 (NVMe) closed on 2026-05-14 across six sub-blocks
(1-0 PCIe MSI-X capability + dynamic IDT vector allocation →
1-1 NVMe device discovery + BAR mapping → 1-2 controller reset
+ admin queue + Identify → 1-3 I/O queue + sector 0 read
polled → 1-4 MSI-X interrupts → 1-5 STATUS + devlog) in one
calendar day. ARSENAL_NVME_OK is the 13th-then-14th sentinel
(the smoke list now reads 14: BOOT, HEAP, FRAMES, BLK, NET,
SCHED, TCP, TLS, TIMER, ACPI, IOAPIC, SMP, NVME, PROMPT).
HEAD is 298d9ba (docs(devlogs): Arsenal NVMe); working tree
clean. main is several commits ahead of origin/main; push
when 2-0 is about to kick off so the step-1 paper + step-2
HANDOFF land together on origin.

Step 2 is M1's structural backbone — the LinuxKPI shim that
hosts every later inherited driver (amdgpu KMS at step 5,
iwlwifi at step 6, the long tail thereafter). ARSENAL.md
flags it explicitly as **"the single largest engineering
task" of M1**, with a 12-20 part-time-week budget; the M1
milestone HANDOFF (git 9df4682) underlines that the shim is
**morale-load-bearing** because nothing user-visible ships
until step 5 lights up amdgpu on top. The step-level
discipline that protects against that morale gap: **one
shim API surface lands + compiles + has a smoke test, then
the next**, with a tiny inherited driver (virtio-balloon)
running through the shim end-to-end at step exit so the
step closes on a working artifact, not on an opinion that
the shim is "ready."

The first inherited driver is the forcing function for the
shim's API surface — the milestone HANDOFF's resolution was
**virtio-balloon** (~600 LOC of inherited C, pure
virtio-bus interaction, no DMA descriptor rings, no
firmware loading, no scatter-gather). e1000 (~3000 LOC) was
the alternative; it stresses more shim surface but at
unjustified cost for a step whose explicit goal is to
*establish* the shim, not to flex it. The trade-off pair is
preserved below for argument; recommend re-confirm
virtio-balloon at 2-0 kickoff.

read CLAUDE.md (peer concerns, Rust-only with the *one*
exception of inherited Linux drivers under the LinuxKPI
boundary, BSD-2-Clause base, GPLv2 preserved on inherited
drivers as a non-negotiable combined-work commitment, build
loop sacred, "no unsafe without // SAFETY: comment naming
the invariant"; the shim is the first M1 surface that
exercises CLAUDE.md §3's combined-work license discipline
in real source — every Rust shim file is BSD-2, every
inherited C file retains its original GPLv2 header) →
STATUS.md (M1 step 1 complete, step 2 active; the four
posture changes from step 1 — IDT-as-Mutex, pci config
read/write helpers, pub bar_address, frames-as-DMA-source —
are all load-bearing for step 2's PCI + IRQ shim adapters)
→ docs/plan/ARSENAL.md § "M1 — Real iron" (third bullet:
LinuxKPI shim "single largest engineering task; budget
accordingly"; the M1 security gate "Linux drivers run with
minimum required kernel capabilities; no shared kernel
state beyond explicit shim interfaces" is the architectural
constraint that keeps the shim a *shim* and not a
backdoor) → docs/plan/ARSENAL.md § "Architectural
Decisions" row "Driver strategy" (LinuxKPI-style shim
hosting Linux 6.12 LTS drivers, FreeBSD drm-kmod as the
battle-tested precedent — that precedent is the model the
shim shape borrows from) → docs/adrs/0004-arsenal-pivot.md
(Rust-only commitment that constrains the shim shape: the
shim itself is Rust, only the inherited driver C is C) →
M1 milestone HANDOFF at git show 9df4682 § "LinuxKPI shim
strategy" trade-off (resolved hybrid: structural
foundation for the load-bearing 30 APIs + incremental for
the long tail) and § "First inherited driver at step 2"
trade-off (resolved virtio-balloon) — the milestone-level
resolutions step 2 inherits → arsenal-kernel/src/pci.rs
(M0/M1 PCIe enumerator + step-1's MSI-X capability walker;
2-2's pci_register_driver / pci_iomap shims wrap these,
plus the pub(crate) config_read32 / config_write32 helpers
step 1 introduced) → arsenal-kernel/src/idt.rs
(register_vector(handler) -> u8 from step 1; 2-2's
request_irq shim routes through it directly) →
arsenal-kernel/src/frames.rs + paging.rs (4-KiB
page-aligned frames at known physical addresses + map_mmio
+ hhdm_offset; 2-2's dma_alloc_coherent / pci_iomap shims
are thin wrappers) → arsenal-kernel/src/virtio.rs (M0's
virtio modern PCI transport + the VirtqDesc / VirtqAvail /
VirtqUsed layouts; 2-3's virtio bus shim exposes these to
inherited C as struct virtqueue) → arsenal-kernel/src/
serial.rs (println!() → COM1; 2-1's printk / pr_info /
pr_warn / pr_err shims route here) → arsenal-kernel/src/
heap.rs (linked_list_allocator wired to alloc::Global; 2-1's
kmalloc / kfree shims wrap GlobalAlloc) → arsenal-kernel/
src/main.rs (boot order; the shim's self-test fires after
nvme::smoke and before net::init, with virtio-balloon's
probe slotted into a new linuxkpi::probe_drivers() call) →
ci/qemu-smoke.sh (add `-device virtio-balloon-pci` to the
QEMU command line at 2-5, plus 1-3 new sentinels per the
chosen sentinel granularity — see trade-off below) →
Cargo.toml (workspace adds new member(s) per the chosen
crate layout — see trade-off; `cc` crate as a build
dependency at 2-4 if we go that route, MIT/Apache-2.0,
clear under §3) → vendor/ (currently limine + spleen; 2-4
adds vendor/linux-6.12/ subset under explicit GPLv2 README
+ MAINTAINERS pointers — never modify the .c files; if a
local fix is unavoidable, fork into vendor/linux-6.12-arsenal/
with the patch documented) → git log --oneline -15 → run
the sanity check below → propose 2-N commit shape (or
argue for a different decomposition) → wait for me to pick
→ "go m1-2-N" for code, "draft m1-2-N" for paper
deliverables.

Where the project is

  - HEAD: 298d9ba (docs(devlogs): Arsenal NVMe). Working
    tree is clean except this file. main is several commits
    ahead of origin/main (the M1-1 sub-block commits + the
    paper pair); this HANDOFF makes one more. Push when 2-0
    is about to kick off so the step 1 retrospective +
    step 2 HANDOFF land together on origin.

  - Kernel: 26 .rs files at M1-1 exit. ELF release ~1.55-
    1.60 MB (was 1.52 MB at M0; nvme.rs added ~30-50 KB +
    pci.rs grew ~150 LOC + idt.rs grew ~80 LOC + frames/
    paging untouched). Step 2 adds substantial structure —
    expect ELF growth into the 2.0-2.5 MB range by step
    exit (the shim's Rust LOC dominates; the inherited
    C is ~600 LOC and link-time-LTO-compressed).

  - Smoke at M1-1 exit: 14 sentinels, ~1.3-1.6 s on QEMU
    TCG with -smp 4. Boot→prompt 96-110 ms. Step 2 adds
    1-3 sentinels depending on chosen granularity (see
    trade-off below) and one new QEMU device flag at 2-5
    (`-device virtio-balloon-pci`).

  - Toolchain: nightly-2026-04-01. Step 2 may pin the C
    cross-compiler version explicitly (the inherited C
    expects a specific clang or gcc release — Linux 6.12
    LTS supports clang ≥ 13 and gcc ≥ 5.1; Arsenal will
    bias to clang for the cross-compile to match the
    Rust-LLVM toolchain bitness assumptions). Pinning
    happens at 2-4 in the build-integration block, not
    before.

  - Vendored crates at M1-1 exit: limine 0.5,
    linked_list_allocator 0.10, spin 0.10, x86_64 0.15,
    smoltcp 0.12, rustls 0.23, rustls-rustcrypto 0.0.2-
    alpha, getrandom 0.4 + 0.2, bitflags 2. Step 2's
    candidate additions: `cc` (build-dep, MIT/Apache-2.0)
    if we use the cc crate at 2-4; possibly `cty` or
    `core::ffi` only (no extra crate) for C-FFI integer
    typedefs at 2-1; possibly `cbindgen` (build-dep,
    MPL-2.0 — needs §3 attention; alternative is a
    hand-written shim_c.h header) at 2-4 for the
    C-callable side of the shim. **MPL-2.0 status decision
    deferred to 2-4 kickoff** — if cbindgen blocks, hand-
    writing the header is straightforward at the shim's
    scale.

  - QEMU command line at step 2 exit (proposed):
    `-cdrom $ISO -m 256M -smp 4 -machine q35 -accel tcg
    -cpu max -device virtio-rng-pci
    -drive file=$ISO,if=none,id=blk0,format=raw,readonly=on
    -device virtio-blk-pci,drive=blk0
    -drive file=$NVME_BACKING,if=none,id=nvme0,format=raw
    -device nvme,serial=arsenal0,drive=nvme0
    -netdev user,id=net0
    -device virtio-net-pci,netdev=net0
    -device virtio-balloon-pci
    -display none -no-reboot -no-shutdown
    -serial file:$SERIAL_LOG -d guest_errors -D $QEMU_LOG`.
    The balloon device requires no backing file; QEMU
    presents it as a virtio-pci device the shim's
    virtio-bus adapter discovers + the inherited
    virtio_balloon.c probes + binds to.

M1 step 2 — proposed sub-block decomposition

The plan below is the kickoff proposal, not gospel. The
user picks; deviations get justified before code lands.
Step 2 decomposes into seven sub-blocks (one structural
ADR + four shim-API surfaces + the vendor/integration
+ the inherited driver coming online + the paper). The
milestone HANDOFF's "one devlog per cluster" guidance
groups these into three devlogs: **shim foundation** (2-0
+ 2-1 + 2-2 + 2-3), **GPL boundary** (2-4), **first
inherited driver** (2-5). The retrospective at 2-6 gets
its own paper.

  **(M1-2-0) Workspace layout decision + ADR-0005 +
  empty crate skeleton.** No shim code yet — pick the
  crate organization (see trade-off below) and write
  ADR-0005 documenting the structural decision, the
  GPLv2/BSD-2 boundary, and the directory layout. The
  ADR also names the inherited-driver vendoring discipline:
  `vendor/linux-6.12/` mirrors the upstream Linux 6.12 LTS
  source tree at file-path resolution (e.g.,
  `vendor/linux-6.12/drivers/virtio/virtio_balloon.c` matches
  upstream's path), header subset only (we vendor the .h
  files transitively included by the .c files we host —
  not the entire kernel header tree), every inherited
  file's GPLv2 SPDX header preserved unchanged, no local
  patches without a forked `vendor/linux-6.12-arsenal/`
  shadow directory + diff documented inline. The empty
  crate(s) build clean (cargo check passes) but contain
  no shim code yet. ~200 LOC of Cargo / xtask glue + the
  ADR itself + the empty crate src/lib.rs files. **No new
  sentinel.** Two commits: `docs(adrs): ADR-0005, LinuxKPI
  shim layout + GPL/BSD-2 boundary` and `feat(linuxkpi):
  workspace skeleton`. Use **go m1-2-0** for the code,
  **draft m1-2-0-adr** for the ADR. **Devlog cluster: shim
  foundation.**

  **(M1-2-1) Foundational types + headers — the
  load-bearing 30 APIs.** Implement the shim API surface
  every inherited driver needs: C-FFI integer typedefs
  (`__u8`/`__u16`/`__u32`/`__u64`/`__s*` matching Linux's
  `<linux/types.h>` shape, `gfp_t` as a u32 newtype,
  `dma_addr_t` as u64, `size_t`/`loff_t`/`ssize_t` via
  `core::ffi`); printk + the pr_* family routed to our
  serial::println! (with KERN_* level prefix preserved);
  kmalloc / kzalloc / kfree / krealloc routed to
  `alloc::alloc::Global` honoring the GFP_KERNEL /
  GFP_ATOMIC distinction (atomic = "must not sleep,"
  enforced by IrqGuard scope at the call site); mutex_init
  / mutex_lock / mutex_unlock + spin_lock / spin_unlock +
  raw_spinlock + atomic_t / atomic_inc / atomic_dec /
  atomic_read over our spin::Mutex + core::sync::atomic;
  container_of (Rust macro_rules), BUG_ON / WARN_ON /
  WARN_ONCE (panic! / serial-warn respectively); jiffies +
  msleep + udelay over our LAPIC TICKS counter (jiffies =
  HZ-rate counter, msleep is sleep-not-busy-wait when a
  scheduler is available — at M1 cooperative-only, msleep
  busy-waits with a yield); copy_from_user / copy_to_user
  stubs that BUG_ON for now (no userspace at M1; the shim
  exposes the symbol so inherited drivers link, but any
  call panics — a cleaner failure mode than silent data
  corruption). Self-test fires from kernel main:
  ARSENAL_LINUXKPI_OK on a small "shim talks to itself"
  routine that touches printk + kmalloc + mutex + atomic
  in sequence. ~600-800 LOC of shim Rust + ~80 LOC of
  shim_c.h declarations (the C side sees `extern void
  *kmalloc(size_t, gfp_t);` etc.). **Sentinel:
  ARSENAL_LINUXKPI_OK** (or absent per chosen granularity).
  One commit: `feat(linuxkpi): foundational types + printk
  + slab + locks + atomics`. Use **go m1-2-1**. **Devlog
  cluster: shim foundation.**

  **(M1-2-2) PCI bus adapter + IRQ bridge.** Implement the
  Linux PCI driver registration model over our pci.rs
  enumerator: pci_register_driver(struct pci_driver *) /
  pci_unregister_driver iterates the registered driver
  table on every pci::scan() result, matches by
  vendor/device ID against the driver's id_table, calls
  the driver's .probe(struct pci_dev *) callback with a
  struct pci_dev whose fields wrap our (Bdf, BAR map);
  pci_resource_start(dev, bar) / pci_resource_len(dev, bar)
  expose our pci::bar_address; pci_iomap(dev, bar, len)
  calls paging::map_mmio + returns the HHDM-virtual
  address; pci_set_master toggles the bus-master bit via
  pci::config_write32; pci_enable_device toggles the
  memory-space bit similarly; pci_alloc_irq_vectors +
  pci_irq_vector + pci_free_irq_vectors wrap our
  pci::msix_info + idt::register_vector + the MSI-X table
  programming step 1 already exercises in nvme.rs;
  request_irq(vector, handler, flags, name, dev) /
  free_irq route directly through idt::register_vector +
  the LAPIC EOI path; dma_alloc_coherent(dev, size, &handle,
  gfp) wraps frames::FRAMES.alloc_frame (size rounded up
  to 4-KiB pages); dma_map_single / dma_unmap_single /
  dma_sync_single_for_device / dma_sync_single_for_cpu are
  no-ops on x86_64 (cache-coherent DMA — comment cites the
  spec section). Self-test: a no-op Linux-shaped pci_driver
  that registers, sees pci::scan() devices fire .probe(),
  records the vendor/device IDs, and unregisters. **Sentinel:
  ARSENAL_LINUXKPI_PCI_OK** (or absent per chosen
  granularity). ~600-800 LOC of shim Rust + ~120 LOC of
  shim_c.h growth. One commit: `feat(linuxkpi): PCI bus
  adapter + IRQ bridge + DMA coherent`. Use **go m1-2-2**.
  **Devlog cluster: shim foundation.** **This sub-block
  is the largest single piece of step 2 — budget 4-5
  focused sessions.**

  **(M1-2-3) Virtio bus adapter.** Implement the Linux
  virtio device + driver model over our virtio.rs:
  virtio_register_driver(struct virtio_driver *) /
  virtio_unregister_driver, struct virtio_device wrapping
  our VirtioDevice, struct virtqueue + virtqueue_add_buf /
  virtqueue_kick / virtqueue_get_buf wrapping our
  VirtqDesc/Avail/Used layouts, virtio_get_features /
  virtio_finalize_features over our common_cfg
  feature-bit handshake, virtio_cread / virtio_cwrite over
  the device_cfg pointer; virtio_pci_modern_probe enumerates
  PCI devices with virtio's vendor ID 0x1AF4, dispatches
  matching virtio_drivers' .probe via the driver table
  (the same pattern the PCI bus adapter uses, narrowed to
  virtio's PCI subsystem-ID space). Self-test: a no-op
  virtio_driver registers, sees the existing virtio-blk +
  virtio-net devices fire .probe(), unregisters cleanly.
  ARSENAL_LINUXKPI_VIRTIO_OK (or absent per granularity).
  ~400-500 LOC. One commit: `feat(linuxkpi): virtio bus
  adapter`. Use **go m1-2-3**. **Devlog cluster: shim
  foundation.**

  **(M1-2-4) Vendor Linux 6.12 LTS subset + build
  integration.** Vendor the inherited C subset:
  `vendor/linux-6.12/include/linux/{types,printk,kernel,
  gfp,slab,mutex,spinlock,atomic,err,list,kref,wait,
  workqueue,interrupt,pci,virtio,virtio_config,
  virtio_balloon}.h` (every header virtio_balloon.c
  transitively includes — sourced verbatim from the Linux
  6.12 LTS git tag, GPLv2 SPDX preserved unchanged) +
  `vendor/linux-6.12/drivers/virtio/virtio_balloon.c`
  itself. Build integration: a `linuxkpi-cc` build script
  (or xtask subcommand — see trade-off below) compiles the
  inherited .c files with `-target x86_64-unknown-none
  -nostdinc -fno-stack-protector -mno-red-zone -mcmodel=
  kernel -ffreestanding -I vendor/linux-6.12/include
  -I linuxkpi/include` and links the resulting object
  files into the kernel ELF. The shim's
  `linuxkpi/include/shim_c.h` declares the Rust-side
  symbols for C consumption (printk, kmalloc, mutex_lock,
  pci_register_driver, etc.) so the inherited C compiles
  + links. **No new sentinel.** ~150 LOC of build infra +
  the vendored C tree (file count high, hand-written LOC
  count zero — every line is upstream Linux). One commit:
  `feat(linuxkpi): vendor Linux 6.12 LTS subset + build
  integration` with a body that enumerates exactly which
  files were vendored + at which upstream commit (the
  Linux 6.12 LTS tag SHA) so future readers can audit the
  GPL boundary. Use **go m1-2-4**. **Devlog cluster: GPL
  boundary.**

  **(M1-2-5) virtio-balloon comes online.** Fix the
  inevitable shim API gaps (every shim has them; compile
  errors from virtio_balloon.c's specific include set drive
  the next batch of shim functions — typical gaps:
  list_for_each_entry, sysfs_create_group, debugfs_create
  stubs, kthread_run / kthread_stop scheduler-dependent
  pieces, schedule_work / queue_work workqueue stubs over
  our cooperative scheduler). Wire balloon's module_init
  through linuxkpi::probe_drivers() called from kernel main;
  on virtio-balloon-pci device discovery, the balloon's
  .probe runs, the virtio config queue gets allocated, the
  balloon device is registered with the inherited driver's
  internal state; the smoke validates that the balloon
  device is bound (the shim logs "virtio_balloon: probe
  succeeded" via printk) and the virtio config space shows
  the expected feature bits negotiated. Sentinel:
  ARSENAL_VIRTIO_BALLOON_OK on successful probe completion.
  ~300-500 LOC of shim API gap-filling Rust + 0 LOC of
  inherited C (balloon stays unmodified — if it tries to
  call something we haven't shimmed, the right answer is
  to add the shim stub, not to patch the driver).
  Smoke command line gains `-device virtio-balloon-pci`.
  One commit: `feat(linuxkpi): virtio-balloon online`. Use
  **go m1-2-5**. **Devlog cluster: first inherited driver.**
  **This is the sub-block that closes the ARSENAL.md step-2
  outcome ("first inherited driver runs through the shim").**

  **(M1-2-6) STATUS refresh + step 2 devlog cluster +
  step 3 (xHCI) HANDOFF kickoff.** STATUS flips step 2
  from "active" to "complete," promotes step 3 (xHCI USB)
  to "active," and writes the step 2 retrospective
  (what the actual shim API surface count came out to,
  which "load-bearing 30" guesses from the M1 milestone
  HANDOFF were right + which were wrong, what gap-filling
  at 2-5 surprised us, how close the LOC came to the
  ~12-20-week estimate, the GPL/BSD-2 boundary in
  practice). Three devlogs in
  `docs/devlogs/2026-NN-arsenal-linuxkpi-foundation.md`,
  `-linuxkpi-gpl-boundary.md`, `-linuxkpi-virtio-balloon.md`
  per the milestone HANDOFF's cluster guidance. The
  step-3 HANDOFF kickoff revisits the milestone-level
  trade-off "xHCI shape — native Rust vs LinuxKPI port"
  with the shim's actual shape now in evidence (if 2-2's
  PCI + IRQ adapters cover xHCI's needs comfortably, the
  LinuxKPI port becomes the preferred path; if the shim's
  USB-bus surface is bare, native Rust wins). Three or
  four commits depending on whether the step-3 HANDOFF
  lands in this sub-block or its own follow-up:
  `docs(status): M1 step 2 complete, step 3 (xHCI) next`,
  `docs(devlogs): LinuxKPI shim foundation`,
  `docs(devlogs): LinuxKPI GPL boundary`,
  `docs(devlogs): LinuxKPI virtio-balloon`,
  `docs(handoff): kick off M1 step 3 (xHCI)`. Use **go
  m1-2-6** for STATUS, **draft m1-2-6-devlog-N** (N = 1, 2,
  3) for the devlogs, **draft m1-3 HANDOFF** for the next
  step-level kickoff. **Devlog cluster: step 2
  retrospective.**

Realistic session-count estimate. M1's cadence is week-
scale, not day-scale (M1 milestone HANDOFF note #3); step 2
explicitly slower than step 1 because the shim has no spec
to lean on the way NVMe 1.4 § 7.6.1 leaned us through
controller reset — the shim is "build the API the next
inherited driver needs and no more" plus the discipline to
not over-engineer. 2-0 is half a calendar week (the ADR
demands more thought than code, and the empty crate is
mostly Cargo.toml plumbing). 2-1 is 2-3 focused sessions
of ~3 weeks calendar — type definitions are mechanical but
need cross-checking against Linux's `<linux/types.h>` for
ABI compatibility, and the printk format-string parser
is the one piece that can absorb a session on its own
("printk supports %pK %pS %pV with kernel-specific
formatters; we shim the non-trivial ones to a serial-safe
subset"). 2-2 is the longest single sub-block at 4-5
sessions / 3-4 calendar weeks — the PCI driver model is
where the shim earns its keep, and request_irq's
interaction with our IDT discipline needs unhurried
thought. 2-3 is 2-3 sessions / 2 weeks. 2-4 is 2-3
sessions / 2 weeks (build-system work always takes longer
than expected; the cross-compile flag set is a rabbit
hole). 2-5 is unpredictable — 3-5 sessions / 3-4 calendar
weeks — because every shim has surprise gaps and gap-
filling is not budgetable in advance; the M1 milestone
HANDOFF's three-sessions-then-step-away cue applies here
specifically. 2-6 is the milestone-style paper session,
1 calendar week. **Sum: ~14-17 calendar weeks at the
ARSENAL.md ~15hr/week cadence.** Inside the 12-20 week
budget; do not let post-pivot concentration-window
optimism shrink it.

Step-level trade-off pairs

  **Crate layout.**
  (i) **Single workspace member `linuxkpi/`** with
  `linuxkpi/src/{types,log,slab,locks,pci,irq,dma,
  virtio,...}.rs` + `linuxkpi/include/shim_c.h` + the
  vendored Linux subset under `vendor/linux-6.12/`.
  arsenal-kernel depends on linuxkpi as a regular crate.
  Simple workspace topology; one Cargo.toml per shim
  member.
  (ii) **Subdirectory `arsenal-kernel/src/linuxkpi/`** with
  the shim modules co-located with the kernel. No new
  crate; shim Rust is just kernel code with a directory
  fence. Smaller workspace; tighter coupling between
  kernel + shim; uglier license-boundary story (the kernel
  crate becomes a mixed BSD-2 / inherited-GPLv2 link
  combined work, where (i) keeps the BSD-2 vs GPLv2
  boundary at the crate edge).
  (iii) **Three crates: `linuxkpi-headers` (shim_c.h + the
  vendored .h subset, build-only), `linuxkpi-shim` (the
  Rust adapters), `linuxkpi-drivers` (the vendored .c
  files + per-driver Cargo features).** Maximally
  separated; drm-kmod-shaped; over-organized at step 2
  scale but the right shape by step 6 when amdgpu +
  iwlwifi both pull from the shim.
  Recommend (i) at step 2 with a documented intent to
  evolve toward (iii) when the second inherited driver
  arrives. The single-crate layout matches our M0 pattern
  (one file per subsystem, one crate per major boundary);
  splitting into three crates upfront optimizes for a
  future we don't have yet. The license boundary is
  preserved either way — what matters is the SPDX header
  on each file and the build-system enforcement at 2-4.

  **C compilation toolchain.**
  (i) **`cc` crate as build-dep + linuxkpi/build.rs.**
  The cc crate (MIT/Apache-2.0, mature, used by half the
  Rust ecosystem) handles cross-compile flags, target
  triple, output object file naming. Build dependency
  only — does not affect the kernel's runtime profile.
  (ii) **xtask subcommand `cargo xtask build-linuxkpi`.**
  Custom Rust harness inside our existing xtask; full
  control over the compile command line; no external
  build-dep. More code to maintain; fully ours.
  (iii) **Bind to a host Linux Kbuild invocation.** Use
  the host's Linux kernel build system to produce the
  inherited .o files; link them into our kernel ELF.
  Maximum fidelity to upstream Linux's expected build
  environment; biggest dependency footprint (the dev
  machine needs a full Linux source tree); unworkable for
  a CI runner that's not Linux.
  Recommend (i). The cc crate is the lowest-friction path
  with the smallest maintenance surface; xtask gets the
  same flexibility for the small price of one build-dep.
  (iii) is a non-starter because our CI runs on macOS-
  hosted GitHub runners (the smoke harness's Python TLS
  listener requirement) — binding to host Kbuild would
  break that.

  **Header vendoring strategy.**
  (i) **Minimal subset, hand-curated.** Vendor only the
  .h files virtio_balloon.c transitively includes; expand
  per-inherited-driver as new drivers arrive at steps 3,
  5, 6. Smallest tree; clearest GPL boundary; one
  read-of-the-#include-graph at every new-driver kickoff.
  (ii) **Full Linux 6.12 LTS include/ tree mirrored.**
  Vendor the entire `include/linux/` and `include/uapi/`
  subdirectories of upstream Linux. Largest tree (~10K
  header files); zero per-driver work to expand later;
  noisy git history when we update the LTS pin.
  (iii) **Lazy mirror via build-time fetch.** Cache the
  Linux include tree in CI but don't check it into the
  repo. Most surprising failure modes; don't.
  Recommend (i). Minimal subset is the discipline that
  keeps the GPL boundary visible at every PR; expanding
  per-driver is the same shape as our existing per-
  subsystem .rs file pattern. The cost is one
  `find-include-graph.sh` script (or xtask subcommand)
  invoked at each new-inherited-driver kickoff to
  enumerate what to vendor.

  **First inherited driver target (re-confirm).**
  (i) **virtio-balloon** (M1 milestone HANDOFF
  recommendation). ~600 LOC of inherited C; pure virtio-
  bus interaction; no DMA descriptor rings; no firmware
  loading; no hardware-quirk wrangling; QEMU emulates
  cleanly.
  (ii) **e1000** (Intel gigabit Ethernet). ~3000 LOC;
  needs DMA descriptor rings, ethtool stubs, netdev
  registration; better stress test of the shim. Pulls
  Linux's full netdev infrastructure into the shim
  earlier than necessary.
  (iii) **Linux serial 8250.** Tiny but conflicts with
  our existing serial.rs; the shim would have to suppress
  the inherited driver's MMIO use to avoid stomping COM1.
  Awkward.
  (iv) **A trivial Linux test module** (e.g., `lib/
  test_printf.c` or similar). Smallest possible shim
  exercise; no hardware interaction at all; doesn't
  validate the PCI / virtio / IRQ paths; pure printk +
  slab + locks. Useful as a 2-1 self-test, not as the
  step-exit driver.
  Recommend (i) — re-confirm the milestone HANDOFF's
  resolution. virtio-balloon is the smallest *useful*
  inherited driver in the Linux tree that exercises the
  shim's full vertical: PCI bus → virtio-pci modern
  transport → virtio bus → device probe → IRQ wiring →
  config queue. e1000 is the right step 3 candidate if
  xHCI doesn't materialize as the chosen step-3 driver
  (per milestone HANDOFF's "xHCI shape" deferred trade-
  off).

  **Sentinel granularity.**
  (a) **Single ARSENAL_VIRTIO_BALLOON_OK** at 2-5 (driver
  online + probe succeeded). 2-1, 2-2, 2-3 each get a
  step-internal log line but no sentinel. Smoke gains
  exactly one new sentinel (15 total).
  (b) **Per-shim-surface sentinels:**
  ARSENAL_LINUXKPI_OK at 2-1, ARSENAL_LINUXKPI_PCI_OK
  at 2-2, ARSENAL_LINUXKPI_VIRTIO_OK at 2-3,
  ARSENAL_VIRTIO_BALLOON_OK at 2-5. Smoke gains four
  sentinels (18 total). Per-sub-block bisect granularity.
  (c) **Two sentinels:** ARSENAL_LINUXKPI_OK at 2-1
  (shim self-test passes — covers types/log/slab/locks/
  atomics in one), ARSENAL_VIRTIO_BALLOON_OK at 2-5
  (driver online). Smoke gains two sentinels (16 total).
  Compromise — one "shim infrastructure works" sentinel
  + one "first user of the shim works" sentinel.
  Recommend (c). The shim's user-visible property at
  step 2 is "an inherited driver runs through it";
  ARSENAL_VIRTIO_BALLOON_OK asserts that. ARSENAL_
  LINUXKPI_OK at 2-1 asserts the foundational shim's
  internal correctness without claiming the bus
  adapters work — the latter only become observable
  when 2-5 actually exercises them. (b)'s per-surface
  sentinels are over-granular: PCI + virtio adapters
  with no driver consuming them are testing the shim's
  own self-test, not a real property.

  **Module init/exit semantics.**
  (i) **Synchronous at boot.** linuxkpi::probe_drivers()
  is called explicitly from kernel main after pci::scan
  (and after nvme::smoke); registered drivers' module_init
  fires, then their probe runs against discovered devices.
  Linear control flow; matches our M0/M1 cooperative-only
  scheduler shape.
  (ii) **Deferred / event-driven.** module_init runs
  in a kthread spawned at boot; probe runs from the
  PCI hotplug event channel (which doesn't exist yet at
  M1; would need kthread + workqueue infrastructure in
  the shim). More Linux-faithful; bigger 2-2 surface.
  (iii) **Hybrid.** Synchronous at boot for the M1
  inherited-driver set; deferred path stubbed but not
  exercised until M2 when Stage's UI thread needs
  background drivers.
  Recommend (i). The shim should serve M1's needs first;
  deferred init is real work that doesn't ship value
  until at least M2. Keep the shim's scheduler dependency
  explicit (msleep + workqueue stubs panic-on-call) so
  any inherited driver that needs the deferred path
  fails loudly rather than silently misbehaving.

  **Symbol exposure direction.**
  (i) **Rust-calls-C-only.** The shim is a one-way
  bridge: Rust kernel code calls C inherited drivers
  through C-FFI shim functions; the inherited C never
  calls back into Rust. Cleaner mental model; impossible
  for any non-trivial Linux driver (every driver calls
  printk, kmalloc, request_irq — all of which are
  shim-implemented Rust functions exposed to C).
  (ii) **Bidirectional.** The shim exposes Rust functions
  to C via `extern "C"` + a hand-written shim_c.h header
  (or cbindgen-generated). The inherited C calls printk
  (Rust), pci_register_driver (Rust), etc.; Rust calls
  the inherited driver's exported probe / module_init
  symbols. This is the necessary path; the only question
  is whether shim_c.h is hand-written or generated.
  Recommend (ii) with a hand-written shim_c.h. cbindgen
  is excellent but adds an MPL-2.0 build-dep that needs
  CLAUDE.md §3 attention; the shim_c.h header is small
  enough (~200-400 lines at step 2 exit) to maintain by
  hand without much cost. cbindgen joins later if the
  shim's C surface grows past the maintainability threshold
  (recommend revisit at step 5 when amdgpu's surface
  arrives).

  **Sub-block granularity.**
  (a) **Seven-block shape** above (ADR / types / PCI /
  virtio / vendor / balloon / paper). Bisect-rich; one
  PR-equivalent per shim surface; six green smoke runs
  along the way.
  (b) **Five-block shape** combining 2-1 with 2-2 (types
  + PCI + IRQ in one commit) and 2-3 with 2-4 (virtio +
  vendor in one commit). Saves two commits; loses the
  bisect granularity that protects against "the shim PCI
  adapter regressed three weeks ago and we just noticed."
  (c) **Four-block shape** also combining 2-4 with 2-5
  (vendor + balloon online in one commit). Fastest visible
  progress; biggest single sub-block at 2-4-merged-2-5
  (~600+ LOC of gap-filling on top of 150 LOC build infra).
  Hardest to debug if balloon's probe fails — is it a
  shim bug, a vendoring bug, or a balloon-specific quirk?
  Recommend (a). The shim's per-surface bisect granularity
  is the discipline that keeps the 12-20-week budget
  visible week-over-week (M1 milestone HANDOFF note #1's
  morale-load-bearing requirement). Each green sub-block
  is a checkpoint: "the shim's PCI adapter works"
  shipped + smoke-validated independently of "the shim's
  virtio adapter works," and 2-5's balloon failure modes
  isolate cleanly to "balloon-specific quirk" since the
  shim surfaces all individually green.

Sanity check before kicking off

    git tag --list | grep arsenal             # arsenal-M0-complete
    git log --oneline -10                     # 298d9ba (HEAD),
                                              # e08b7d2, dcd9ed1,
                                              # a75541c, 061e3cb,
                                              # bc6ddac, dd9f4a6,
                                              # 00e39fe, 077961d,
                                              # 9df4682
    git status --short                        # ?? HANDOFF.md (only,
                                              # while drafting this)
                                              # or clean once committed
    cargo build -p arsenal-kernel --target x86_64-unknown-none --release
                                              # clean, ~1.55-1.60 MB ELF
    cargo clippy -p arsenal-kernel --target x86_64-unknown-none --release -- -D warnings
                                              # clean
    cargo xtask iso                           # arsenal.iso ~19.3 MB
    ci/qemu-smoke.sh                          # ==> PASS (14 sentinels)

Expected: HEAD as above; smoke PASSes with 14 sentinels;
boot→prompt around 96-110 ms; ARSENAL_NVME_OK fires.

If 2-2 or 2-5 fails to make progress, the likely culprits
are:

  (a) **PCI driver match never fires.** The shim's
  pci_register_driver iterates the registered driver
  table on every pci::scan() result; if the iteration
  happens *before* pci::scan() runs (boot-order bug) or
  if the id_table comparison treats Linux's PCI_VENDOR_ID
  + PCI_DEVICE_ID layout differently than our (Bdf,
  vendor, device) tuple, no .probe ever fires. Log every
  pci::scan() result + every registered driver's id_table
  entry at boot to surface the mismatch.

  (b) **request_irq vector leak.** The shim's request_irq
  must call idt::register_vector; the vector returned is
  the IDT vector to program in the MSI-X table (or the
  IOAPIC RTE for legacy IRQs). If the shim allocates a
  vector but never wires it (forgets the MSI-X table
  write), the inherited driver's IRQ handler is never
  called and the driver wedges waiting for a completion
  that never arrives. Log the vector + the MSI-X table
  programming at every request_irq.

  (c) **GFP_ATOMIC honored too liberally.** The
  GFP_KERNEL / GFP_ATOMIC distinction means GFP_ATOMIC
  must not sleep (i.e., must not yield to the scheduler).
  Our shim's kmalloc routes both to alloc::Global which
  doesn't sleep — but if a future shim function (mutex_
  lock, msleep) is called from a path the inherited driver
  passed GFP_ATOMIC into, the implicit "must not sleep"
  invariant is broken silently. The shim's mutex_lock
  needs an IrqGuard scope check ("am I in IRQ context?")
  and a BUG_ON if so; the shim's msleep needs the same.

  (d) **DMA addresses confused with HHDM-virtual
  addresses.** dma_alloc_coherent returns *both* a
  CPU-side virtual address (the HHDM-mapped virt addr)
  *and* a DMA-side handle (the physical address); the
  inherited driver is required to use the DMA handle for
  any device-facing register write. If the shim returns
  the same address for both (because HHDM-virt and phys
  differ only by a constant offset), QEMU works (the
  IOMMU is permissive) but real hardware will fail
  silently with the controller reading from the wrong
  physical address. Compute + return the correct phys
  addr at the dma_alloc_coherent boundary; the regression
  surfaces only on real iron at step 7.

  (e) **container_of misalignment.** The Linux idiom
  `container_of(ptr, struct, member)` recovers the outer
  struct pointer from a member pointer using offset
  subtraction. Rust's `core::mem::offset_of!` macro
  gives the right offset; getting the offset_of! macro
  signature wrong (e.g., comparing repr(Rust) against
  repr(C) layouts) silently corrupts the pointer. Every
  shim type the inherited C inspects via container_of
  must be `#[repr(C)]`; verify with a static_assert at
  the type definition.

  (f) **module_init never called.** The Linux module_init
  macro registers a function in a special ELF section;
  Linux's loader iterates the section to call each
  module_init at boot. Our shim has no module loader;
  the inherited driver's module_init is a regular
  function we call directly from linuxkpi::probe_drivers().
  The shim's module_init macro must expose the function
  name (e.g., as a public extern symbol) so our probe path
  can call it; the inherited driver's module_init function
  becomes a regular `pub extern "C" fn virtio_balloon_init`
  in C, and the shim calls it explicitly. Document the
  pattern in 2-1's shim_c.h header comment so future
  inherited drivers follow it.

  (g) **printk format-string parsing.** Linux's printk
  supports format specifiers our serial::println! doesn't
  (%pK, %pS, %pV, %pa, %pdt, dozens more). The shim's
  printk should route to a vprintk implementation that
  parses the Linux format-string subset and dispatches to
  a serial-safe printer; format strings the shim doesn't
  support should print as `<unsupported %p%c>` rather
  than crash. The inherited drivers print a *lot*; a
  printk that crashes on an unrecognized specifier wedges
  the boot at the first non-trivial driver message.

Out of scope for step 2 specifically

  - **Real-hardware boot.** Step 2 validates the shim
    + first inherited driver in QEMU only. Real-iron
    boot is step 7; shim correctness on real hardware
    will surface failure modes QEMU's permissive virtio
    + IOMMU don't expose.
  - **More than one inherited driver.** Step 2 lands
    virtio-balloon. xHCI at step 3 (whether native Rust
    or LinuxKPI port — re-evaluated at step 3 kickoff
    with the shim's actual shape in evidence); amdgpu
    at step 5; iwlwifi at step 6.
  - **Workqueue infrastructure.** Linux drivers heavily
    use workqueues for deferred work; balloon doesn't
    (its work runs in the probe + IRQ paths only).
    Step 2's workqueue stubs panic-on-call; subsequent
    inherited drivers that need workqueues bring the
    real implementation along.
  - **kthread scheduler integration.** Balloon doesn't
    spawn kthreads. amdgpu / iwlwifi will; that work
    arrives at step 5 / 6.
  - **sysfs / debugfs / procfs.** Balloon publishes some
    sysfs nodes; the shim's sysfs_create_group is a
    no-op stub. User-visible introspection of inherited
    drivers waits for M2 when Inspector overlay arrives.
  - **CONFIG_* feature gates.** Linux's kernel-config
    system gates entire subsystems on Kconfig options.
    Our shim hardcodes the config: every inherited
    driver compiles as if the kernel were configured with
    the minimal CONFIG_ set virtio_balloon needs. The
    inherited .c source is unmodified; we change the
    config view, not the source.
  - **Module reloading / unloading.** Linux supports
    loading + unloading kernel modules at runtime; our
    shim doesn't. module_exit is exposed as a symbol
    but never called.
  - **Power management hooks (suspend / resume / pm_ops).**
    Out of scope per ARSENAL.md M1's "no S3/S4" note;
    the shim's pm_ops stubs are no-ops.

Permanently out of scope (do not propose)

  - Any unsafe block without a // SAFETY: comment naming
    the invariant the caller must uphold. CLAUDE.md hard
    rule. The shim is heavy on unsafe (FFI boundary, raw
    pointer wrangling, container_of, MMIO reads); every
    unsafe block needs the SAFETY comment.
  - Modifying inherited Linux .c source without forking
    into vendor/linux-6.12-arsenal/ + documenting the
    diff. CLAUDE.md §3 GPLv2 preservation rule.
  - Reverting any M0 or M1-step-1 commit. Both closed +
    tagged (M0) or merged (M1-1).
  - Force-pushing to origin. Branch is in sync; preserve
    history.
  - Dropping BSD-2-Clause SPDX header from any new
    Arsenal-base file. Inherited .c retains its original
    GPLv2 SPDX.
  - Pulling a GPL Rust crate into the kernel base or the
    shim crate. The GPL boundary is exactly + only the
    inherited Linux .c source under vendor/linux-6.12/;
    the shim's Rust source is BSD-2 with no GPL crates.
  - Religious framing. CLAUDE.md hard rule.
  - Reintroducing HolyC. ADR-0004's discard is final.
  - Going back to stable Rust.
  - Skipping the build + smoke loop on a feat(linuxkpi)
    or feat(kernel) commit.

Three notes worth flagging before you go

  1. **The shim is morale-load-bearing.** The M1 milestone
     HANDOFF underlines this; it bears repeating at step
     kickoff. Nothing user-visible ships from step 2 alone
     — the user-visible payoff arrives at step 5 (amdgpu
     KMS) when the framebuffer Stage will eventually run
     on. Step 2's discipline is one shim surface lands +
     compiles + has a smoke checkpoint + ships, then the
     next. Resist the temptation to "just finish the PCI
     adapter and the virtio adapter together" — six
     small green sub-blocks across 14-17 calendar weeks
     keeps the work visible to future-you in a way one
     monolithic four-month commit does not. CLAUDE.md
     cue: "This has been the active issue for three
     sessions. Want to write up what we've tried and step
     away for a day?" applies *especially* to 2-2 (PCI
     adapter, longest single sub-block) and 2-5 (gap-
     filling, least predictable). Use the cue
     proactively, not reactively.

  2. **The GPL/BSD-2 boundary is not a comment — it is a
     directory.** ADR-0005 at 2-0 names the discipline:
     `linuxkpi/src/` + every Arsenal Rust file is BSD-2
     (with the SPDX-License-Identifier: BSD-2-Clause
     header), `vendor/linux-6.12/` + every inherited C
     file is GPLv2 (with the file's original SPDX header
     preserved unchanged), the `cc`-built object files
     produced from `vendor/linux-6.12/` link into the
     kernel ELF as a *combined work* per the LinuxKPI
     precedent (FreeBSD drm-kmod's decade-deep
     justification). Future-me reading the source tree
     six months from now should be able to tell which
     license applies to any file by which directory it
     lives in. Inherited .c files NEVER live under
     `linuxkpi/`; shim .rs files NEVER live under
     `vendor/`. The build system (2-4) enforces this by
     refusing to compile anything that crosses the line.

  3. **2-5 is when the M1 milestone HANDOFF's "step away
     for a day" cue earns its keep.** Every shim's first
     inherited-driver bring-up surfaces gaps the API
     audit at 2-1/2-2/2-3 missed; gap-filling is the
     hardest-to-budget engineering work in the M1
     surface. Plan for 3-5 sessions; budget for the
     possibility of doubling that. If the third session
     of 2-5 ends with virtio-balloon's probe still not
     reaching its config-queue allocation, do not push
     into a fourth session the same week. Write up what
     was tried (gap inventory, what shim functions
     needed adding, what the failing call chain looked
     like in serial output), commit the partial work as
     a `wip(linuxkpi):` commit on a branch, step away for
     2-3 days. Balloon probe failures have a stubborn
     habit of yielding to a fresh look after rest, much
     like NVMe controller resets did at step 1.

Wait for the pick. Do not pick silently. The natural first
split is 2-0 as a standalone session (the ADR + the empty
crate skeleton — pure structural decision-making, no shim
code yet), 2-1 in 2-3 sessions (foundational shim API), 2-2
as the longest single sub-block (PCI + IRQ + DMA shim, 4-5
sessions), 2-3 in 2-3 sessions (virtio bus shim), 2-4 in
2-3 sessions (vendor + build integration), 2-5 in 3-5
sessions ending with ARSENAL_VIRTIO_BALLOON_OK firing, 2-6
as the milestone-style three-devlog paper session. Use
**draft m1-2-0-adr** to start with the ADR-0005 draft (the
crate-layout decision wants thought before code), or **go
m1-2-0** if the layout decision is made (recommend single
workspace member `linuxkpi/` per the trade-off above) and
you want the empty skeleton landed first. Happy to combine
2-0 with 2-1 if you want the ADR + the foundational shim
in one push, or to defer 2-3 (virtio bus) and let 2-5
discover the virtio-bus-API surface organically as balloon
needs it — the latter would mean 2-2 + 2-4 + 2-5 in one
chain with virtio bus extraction post-hoc, which couples
tightly but compresses the schedule. Your call.
