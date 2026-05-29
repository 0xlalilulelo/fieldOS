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

**M1-2-1 complete (2026-05-15, one focused session,
`4b1f88e`).** ~620 LOC of shim Rust + 112 LOC of
`shim_c.h` + wiring: types.rs (FFI typedefs), log.rs
(printk + KERN_* prefix detection routed to serial via
`linuxkpi_serial_sink` extern), slab.rs (kmalloc / kzalloc
/ kfree / krealloc with 16-byte header for layout
recovery), locks.rs (atomic_t + mutex + spinlock with
repr(C) layouts + Rust-friendly Mutex<T> + AtomicInt).
Self-test exercises printk (Rust + C-callable), kmalloc /
kfree round-trip, kzalloc zero-fill, Mutex<T>::lock,
AtomicInt inc/read/dec, C-callable mutex round-trip;
emits `ARSENAL_LINUXKPI_OK`. Smoke is now 15 sentinels.
Bug caught + fixed in-session: KERN_INFO encoded as
`\x01\x06` (SOH + integer 6) instead of `\x016` (SOH +
ASCII '6'); strip_kern_level fell through silently and the
`[INFO]` tag never appeared. HANDOFF failure mode (g)
material; fixed before commit.

**M1-2-2 partial — PCI bus adapter + DMA coherent
landed (2026-05-15, `f61c1a0`).** ~700 LOC shim Rust +
~70 LOC `shim_c.h` + 118 LOC `arsenal-kernel/src/
linuxkpi_bridge.rs` (the new pattern for kernel-side
primitives linuxkpi consumes via `extern "C"` since the
crate dep is one-way). Surfaces: struct pci_device_id /
pci_dev / pci_driver matching Linux <linux/pci.h>;
pci_register_driver walking every (bus, dev, func) +
matching against NULL-sentinel id_table (PCI_ANY_ID +
class_mask honored) + .probe dispatch with cached BAR
addresses + lengths (BAR-sizing dance per PCI Local Bus
Spec 3.0 § 6.2.5.1); pci_resource_start / pci_resource_len
/ pci_iomap / pci_iounmap / pci_set_master /
pci_enable_device; dma_alloc_coherent / dma_free_coherent
/ dma_map_single / dma_unmap_single / dma_sync_* (no-ops
on x86_64 per Intel SDM Vol. 3A § 11.3 cache-coherent DMA).
Self-test extension: pci walk found 9 present functions +
no-op pci_driver matches every one + dma_alloc_coherent
round-trip with page-aligned dma_handle assertion.

Bug caught + fixed in-session: static `AtomicInt`
declarations landed in `.rodata` because `atomic_t {
counter: i32 }` had no interior-mutability marker; first
.inc() page-faulted on a kernel-text address. Fix:
`counter: UnsafeCell<i32>` — layout invariant preserved
(repr(transparent)), C ABI intact (`int counter`), statics
now writable. Worth-recording trap for future Rust types
intended for `static` use.

**M1-2-2 IRQ bridge landed (2026-05-15, `911518f`).**
~666 LOC across 8 files: 251 LOC `linuxkpi/src/irq.rs`
(new) + 248 LOC pci.rs growth + 64 LOC bridge growth +
35 LOC shim_c.h growth + 57 LOC lib.rs (self_test) +
22 LOC main.rs (dispatcher init wiring).

The 16-slot dispatcher pool: pre-generated
`dispatch_0..dispatch_15` via a `gen_dispatcher!` macro,
each `extern "x86-interrupt" fn(InterruptStackFrame)` that
calls a common dispatch path indexing a static slot table.
`linuxkpi::irq::register_dispatchers(idt::register_vector)`
called early in arsenal-kernel/src/main.rs installs all 16
in the IDT and records the (slot → IDT vector) mapping in
`SLOT_TO_IDT_VEC`. `request_irq(irq, handler, ...)`
populates `SLOTS[irq]`; the dispatcher invokes the
registered Linux handler then sends LAPIC EOI via the new
`linuxkpi_lapic_eoi` bridge fn. `pci_alloc_irq_vectors`
allocates a contiguous slot range, reads MSI-X capability
via the new `linuxkpi_pci_msix_info` bridge fn, programs
each MSI-X table entry (LAPIC fixed-delivery 0xFEE00000 +
APIC ID 0 destination, Message Data = the slot's IDT
vector, Vector Control = unmasked), enables MSI-X in the
cap's Message Control register. `pci_free_irq_vectors`
clears slots + disables MSI-X. struct pci_dev grew
`msix_first_slot` + `msix_vector_count` fields.

**M1-2-2 closed in ~2 sessions (HANDOFF estimate: 4-5).**
Combined with f61c1a0 (PCI+DMA), the full HANDOFF surface
for M1-2-2 (PCI bus adapter + IRQ bridge + DMA coherent)
is complete. Post-pivot concentration window still open;
M1 milestone budget unchanged; variance now concentrated
in M1-2-3 (virtio bus, ~2-3 sessions), M1-2-4 (build
integration / cc-crate cross-compile flag plumbing,
~2-3 sessions), M1-2-5 (gap-filling, ~3-5 sessions of
unpredictability — the "step away for a day" cue moment
per HANDOFF note #1).

Five lints addressed during iteration worth recording
for future sub-blocks: `doc_lazy_continuation` (continuation
lines need indent), missing `#![feature(abi_x86_interrupt)]`
on linuxkpi crate root once the IRQ pool added it, missing
`c_uint` import in pci.rs after the new public API needed
it, `non_camel_case_types` allow on `irq_handler_t` (Linux-
ABI name preserved), one missing `# Safety` on the new
no-op extern fn.

**M1-2-3 complete (2026-05-15, `2fed90c`).** ~524 LOC
`linuxkpi/src/virtio.rs` (new) + 66 LOC bridge growth
(LinuxkpiVirtioDev struct + linuxkpi_virtio_resolve) +
77 LOC shim_c.h growth + 66 LOC lib.rs (self_test
extension) + 1 LOC try_resolve visibility bump. The
"shim foundation" devlog cluster (M1-2-1 + 2-2 + 2-3)
is now structurally complete. Self-test extension:
virtio walk found 3 devices (blk + net + rng) + a
no-op virtio_driver with VIRTIO_DEV_ANY_ID matches all
three. Virtqueue surface (find_vqs, virtqueue_add_*,
virtqueue_kick, virtqueue_get_buf) ships as panic-on-
call stubs; functional implementations land at M1-2-5
when virtio-balloon online demands them.

Cumulative M1-2 picture so far (post-pivot concentration
window remains open per the established pattern; M1
milestone budget unchanged):
| Sub-block | HANDOFF estimate | Actual |
|---|---|---|
| 2-0 ADR + skeleton | 0.5 weeks | 0.5 sessions |
| 2-1 foundational shim | 2-3 sessions | 1 session |
| 2-2 PCI + IRQ + DMA | 4-5 sessions | 2 sessions |
| 2-3 virtio bus | 2-3 sessions | 1 session |
| **subtotal** | **~10-11 sessions** | **~4.5 sessions** |

**M1-2-4 complete (2026-05-15, `6880b01`).** ~295 LOC
across 6 files: 164 LOC `linuxkpi/build.rs` (new) + 43
LOC `linuxkpi/csrc/smoke.c` (new) + 52 LOC
`vendor/linux-6.12/README.md` (new, GPLv2-fenced
directory + vendoring discipline) + Cargo.toml/lib.rs
wiring. The Rust↔C↔Rust FFI loop is wired end-to-end:
the smoke output's `[INFO] linuxkpi: cc-build smoke ok`
line confirms clang cross-compiles → `ar` crate
archives → rust-lld pulls via `+whole-archive` →
inherited C calls Rust shim → Rust shim calls back into
inherited C → returns.

Two real engineering decisions surfaced this session
that the HANDOFF M1-2-4 failure-mode list didn't
anticipate (added to the M1-2-6 retro list):

1. **macOS `ar`/`ranlib` are Mach-O-only.** Apple's
   tools (`ranlib` is a libtool symlink) silently
   produce ELF-archive-index-less `.a` files; rust-lld
   then can't resolve symbols. `llvm-ar` would fix it
   but isn't shipped under that name in stock Apple
   Xcode toolchains and isn't bundled with rustup. Fix:
   pure-Rust `ar` crate writes GNU-format archives
   without a symbol index; paired with rustc's
   `+whole-archive` link modifier (stable since Rust
   1.61) which pulls every `.o` unconditionally.

2. **`-nostdinc` blocks freestanding-safe builtin
   headers.** `<stddef.h>` / `<stdint.h>` aren't libc;
   clang ships them in its resource-dir for freestanding
   use. Fix: `-isystem $(clang -print-resource-dir)/
   include` — the canonical Linux Kbuild dance. M1-2-5
   needs the same flag pair when balloon's transitive
   includes pull stddef.h via vendored Linux headers.

**M1-2-5 Part A complete (2026-05-16, `40176cd`).** 695 LOC
across 8 files: 118 LOC `linuxkpi/src/macros.rs` (new,
container_of! Rust macro + BUG_ON / WARN_ON extern fn shims),
70 LOC `linuxkpi/src/err.rs` (new, IS_ERR / ERR_PTR / PTR_ERR
/ IS_ERR_OR_NULL with MAX_ERRNO=4095), 164 LOC
`linuxkpi/src/list.rs` (new, intrusive list_head + INIT /
add / add_tail / del / empty, C-callable extern "C" surface),
96 LOC `linuxkpi/src/time.rs` (new, jiffies + msleep + udelay
+ ndelay over a new `linuxkpi_jiffies` bridge fn reading
arsenal-kernel's `apic::ticks()`, HZ=100), 70 LOC
`linuxkpi/src/userspace.rs` (new, copy_*_user / strncpy_from_
user / clear_user as panic-on-call stubs — no userspace until
M2 per ADR-0005 § 6), 70 LOC `shim_c.h` growth (preprocessor
macros over the Rust externs + LIST_HEAD / list_for_each_entry
convenience + MAX_ERRNO + HZ + `__user` no-op), 7 LOC bridge
growth (`linuxkpi_jiffies`), 101 LOC `lib.rs` self_test
extension (err round-trip, 3-element list build/iterate/del,
jiffies callable-smoke, container_of recovery).

Bug caught + fixed in-session: initial `time` self-test
called `time::msleep(20)` and asserted `jiffies() > 0` +
post-wait advancement. `linuxkpi::self_test` runs at
main.rs:334, before `sched::run`'s `sti` at sched.rs:362, so
the LAPIC timer interrupt has never fired and TICKS is still
0 — the `t0 > 0` assert would have panicked first; if not,
msleep's busy-wait would spin forever. Fix: scope the time
self-test to jiffies-callable-smoke only; real advance + msleep
coverage arrive when M1-2-5 Part B's balloon driver calls them
post-sti. Worth-recording trap for any future shim primitive
the self_test exercises that depends on LAPIC IRQ delivery.

Cumulative M1-2 picture (post-pivot concentration window
remains open per the established pattern; M1 milestone budget
unchanged):
| Sub-block | HANDOFF estimate | Actual |
|---|---|---|
| 2-0 ADR + skeleton | 0.5 weeks | 0.5 sessions |
| 2-1 foundational shim | 2-3 sessions | 1 session |
| 2-2 PCI + IRQ + DMA | 4-5 sessions | 2 sessions |
| 2-3 virtio bus | 2-3 sessions | 1 session |
| 2-4 cc-build infra | 2-3 sessions | 1 session |
| 2-5 Part A gap-fill | (split) | 1 session |
| 2-5 Part B sub-tasks 1+2 | (split) | 1 session (ADR-0006 triggered) |
| **subtotal so far** | **~13-14 sessions** | **~7.5 sessions** |

Smoke output at Part A exit: PASS, 15 sentinels in 1465 ms,
boot→prompt 184 ms (well under BOOT_BUDGET_MS=3000).
`ARSENAL_LINUXKPI_OK` now covers the new primitives. Smoke
unchanged through Part B sub-tasks 1+2 (script + ADR, no
kernel code).

**M1-2-5 Part B in flight — ADR-0006 reshaped the vendoring
discipline.** Sub-task 1 (recursive vendor-fetch script,
`1e6fba9` + the include-resolution / license-gate fix at
`b2dd46f`) landed clean — 279 LOC bash: curls balloon.c,
greps its `^#include <{linux,asm,asm-generic,uapi}/...>`
references, BFS-recurses with cycle detection, deduplicates,
writes under `vendor/linux-6.12/`, records the upstream SHA.
Sub-task 2 ran the script against `torvalds/linux` v6.12 to
materialize balloon's transitive closure; three falsifications
of ADR-0005 § 3's assumptions surfaced in one session and
triggered ADR-0006 (`16f6d4d`):

  1. **Closure size is qualitatively wrong.** virtio-balloon
     — the simplest possible inherited driver — pulled 281
     transitive headers before BFS halted on an unresolvable
     include, and 281 is not even complete. ADR-0005 § 3's
     "~20" estimate is off ~14×; the Part B scoping
     `~80-150` guess in the prior STATUS revision was off
     ~2-3×. Growth is super-linear in driver complexity:
     amdgpu would pull thousands.

  2. **Some required headers don't exist in upstream as
     files.** `<asm/hash.h>` (transitively pulled by
     `<linux/hash.h>`) is synthesized at compile time by
     Kbuild's `generic-y` / `mandatory-y` rules in
     `arch/<arch>/include/asm/Kbuild` and
     `include/asm-generic/Kbuild`. Verbatim vendoring
     literally cannot capture this class of header.

  3. **Vendored Linux headers force inheriting
     implementation details we already substitute for.**
     `struct mm_struct`, `spinlock_t`, percpu allocator
     slot layout, Linux scheduler classes, KASAN shadow
     memory — each has an Arsenal-side Rust substitute
     under `arsenal-kernel/src/`. The shim is *exactly*
     the layer ADR-0005 was supposed to isolate this
     duplication into.

**ADR-0006's pivot:** `linuxkpi/include/` IS the Linux
header surface — BSD-2-licensed Arsenal-authored
reimplementations matching just-enough Linux 6.12 LTS API
for the inherited drivers we host. The FreeBSD drm-kmod
precedent is the model: 10 years of proven operation,
combined-work model legally clean (BSD-headers + GPL-.c-
source). The shim's `shim_c.h` has been operating this way
since M1-2-1; ADR-0006 codifies it and retires the
"vendor headers verbatim" discipline ADR-0005 § 3
committed to as an unforced parallel burden. Verbatim
vendoring still holds for inherited `.c` source and a
**narrow UAPI carve-out** (BSD-licensed protocol/ABI
headers — for balloon: `virtio_balloon.h`, `virtio_ids.h`,
`virtio_types.h`). `vendor/linux-6.12/` is currently
`README.md` only — sub-task 2's partial 281-file tree was
cleaned before commit per ADR-0006 § 4.

Remaining Part B sub-tasks (post-ADR-0006 shape):

  1. **Simplify `scripts/vendor-balloon.sh`** from 279
     LOC (recursive closure-walker) to ~60-80 LOC (named-
     file fetcher: balloon.c + the three BSD UAPI carve-
     out headers; no recursion, no candidate resolution,
     no Kbuild emulation). Recursive form preserved in
     git history at `b2dd46f` for any future "audit-what-
     the-full-closure-would-be" question. Bisect seam.

  2. **Re-vendor under ADR-0006's discipline:**
     `vendor/linux-6.12/drivers/virtio/virtio_balloon.c`
     + `vendor/linux-6.12/include/uapi/linux/{virtio_
     balloon,virtio_ids,virtio_types}.h`. Each UAPI
     header carries its upstream BSD/dual-license SPDX
     preserved; the vendoring commit body names the
     (license, content, transcription-risk) triple per
     ADR-0006 § 3 for each carve-out. Bisect seam.

  3. **Add balloon.c to `linuxkpi/build.rs`'s source
     manifest.** Attempt first compile. The error stream
     IS the work — each missing type / undefined macro /
     unknown function either extends a header under
     `linuxkpi/include/linux/*.h` (reimplemented in BSD-2,
     not vendored), grows `shim_c.h`, or extends a
     `linuxkpi/src/*.rs` module. Compile-error iteration
     is the unbudget-able part — HANDOFF estimate 3-5
     sessions worth; ADR-0006 doesn't change that estimate
     (the work moves from "vendor a header" to "write a
     BSD-2 header declaring the same surface," net cost
     similar). Bisect seam at each new header file or
     shim module landed.

  4. **ARSENAL_VIRTIO_BALLOON_OK sentinel:** balloon
     device probe fires through linuxkpi::virtio's
     driver registration, balloon's `init_vqs` succeeds
     against a real virtqueue (M1-2-3's panic-on-call
     virtqueue stubs get their first real implementation
     swap-in, driven by what balloon actually calls),
     one stats report round-trip lands on the host.
     Smoke command line needs `-device virtio-balloon-
     pci` added. Bisect seam.

  5. M1-2-6 paper + retro close out M1 step 2. Three
     devlogs per the HANDOFF cluster guidance: shim
     foundation, GPL boundary, virtio-balloon. The
     ADR-0006 finding goes in the GPL boundary devlog
     and the step-2 retrospective — "the shim's
     directional gravity has always been `shim_c.h` is
     the surface" is one of the load-bearing observations
     worth recording.

**Per HANDOFF note #1: Part B is where the morale-
load-bearing "step away for a day" cue applies most.**
The cumulative M1-2 cadence (~7.5 sessions vs ~13-14
HANDOFF estimate for 2-0 through 2-5 Part B sub-tasks
1+2) is still riding the post-pivot concentration window,
though the ADR-0006 detour absorbed one full session of
the buffer. Sub-task 3's compile-error iteration is where
session-count optimism stops applying; treating it as
3-5 sessions / ~3-4 calendar weeks remains the right
posture even though ADR-0006 cleaned up the scope.

**Sub-task 3 in flight — compile-error iteration underway.**
Each round resolves one `#include` in balloon.c by adding a
BSD-2 `linuxkpi/include/linux/*.h` proxy (+ shim_c.h / Rust
shim growth as needed); balloon.c stays out of `build.rs`'s
source manifest (probed via a direct `clang` invocation), and
`main` is green at every commit per the build-loop-is-sacred
discipline. Rounds landed so far:

  - Rounds 1-3 (prior sessions, `3782bc8`..`25b8b48`): virtio.h,
    types.h, virtio_config.h (+ `__le`/`__be` aliases), swap.h
    (+ mm.rs stubs).
  - Round 4 `9bb8fd3`: workqueue.h + workqueue.rs panic stubs.
  - Round 5 `488fd10`: delay.h (over existing time.rs).
  - Round 6 `ac43a2f`: slab.h + 4 GFP modifier flags.
  - Round 7 `3396e8d`: module.h (no-op metadata + module_driver
    shape; init-invocation mechanism deferred to the closing
    commit).
  - **ADR-0007 `8f35963`: struct page is a thin per-frame handle**
    (not a mem_map array) — first-use decision forced by
    balloon_compaction.h; foundational for every later mm-touching
    driver. Took the 0007 slot; the three ADR-0006 provisional
    reservations shifted up by one (three-crate → 0008, cbindgen
    → 0009, deferred init → 0010).
  - Round 8 `864aa62`: balloon_compaction.h + struct page in
    shim_c.h + page.rs stubs. CONFIG_BALLOON_COMPACTION left
    undefined collapses balloon's whole migration path, so the
    header is minimal (3-field balloon_dev_info, no
    migratepage/enum migrate_mode).
  - Round 9 `533a07a`: oom.h + struct notifier_block (+ NOTIFY_OK)
    in shim_c.h + register/unregister_oom_notifier stubs in mm.rs.
  - Round 10 `bbed62d`: wait.h — busy-poll wait_event (cooperative
    M1, mirrors time.rs's msleep posture), wait_queue_head_t,
    cpu_relax, no-op init_waitqueue_head / wake_up.
  - Round 11 `58b9002`: mm.h — page_to_pfn inline + page_address /
    alloc_pages / free_pages / put_page / adjust_managed_page_count
    over the ADR-0007 struct page (page.rs stubs); + PAGE_SHIFT /
    PAGE_SIZE / ULONG_MAX / BUILD_BUG_ON in shim_c.h.
  - Round 12 `6b2e4ef`: page_reporting.h (LAST top-level include) +
    page_reporting_register/_unregister stubs. **Closes the include
    set** — all 12 of balloon's top-level includes now resolve.
  - Round 13 `8e46d58`: first body-error wave foundations —
    u8..u64 / s8..s64 kernel scalar aliases, spinlock_t + the
    irqsave/irq spinlock variants (M1 no-IRQ-disable simplification,
    documented), PM wakeup no-ops (pm_stay_awake / pm_relax /
    device_set_wakeup_capable).

  - Round 14 `02718fa`: kernel core surface — errno constants,
    min/max/ARRAY_SIZE/ALIGN/round_up, likely/unlikely, memset/
    memcpy/memmove, list_for_each_entry_safe, MAX_PAGE_ORDER, +
    test_and_set/clear_bit stubs (bitops.rs). KBUILD_MODNAME handled
    as a per-TU build define (-DKBUILD_MODNAME), not a shim symbol.
  - Rounds 15-16 `e0568da`: mechanical body-error surface — dev_*
    print no-ops, vmstat (global_node_page_state→0), page-poison
    no-ops, PAGE_REPORTING_CAPACITY; scatterlist + sg_init_one
    stub; shrinker (struct + alloc/free/register stubs); workqueue
    globals (cancel_work_sync + system_freezable_wq NULL symbol).
  - Round 17 `99e2959`: virtio-core surface — **balloon.c COMPILES**.
    struct virtio_driver reshaped to Linux's embedded device_driver
    (.driver.name) + feature_table/validate/config_changed;
    virtio_device gains `config` (virtio_config_ops with get/del_vqs)
    and renames the PCI `dev`→`pci_dev` to free `dev` for Linux's
    embedded `struct device dev` (a real collision: balloon takes
    &vdev->dev in 7 places); virtqueue gains vdev/num_free; endian
    helpers; virtio_find_vqs reshaped to the 6.12 struct
    virtqueue_info form. The virtio self-test still passes after the
    reshape. shim_c.h now includes <stdbool.h> (order-independent).

**MILESTONE: virtio_balloon.c compiles against the BSD-2 shim**
(clang exit 0; only 3 residual upstream -Wpointer-sign warnings in
balloon's own int-vs-unsigned get_buf calls, to be suppressed via
-Wno-pointer-sign on balloon.c's eventual manifest entry). Every
#include resolves and every body symbol balloon references is
declared. The compile phase of sub-task 3 is DONE.

**Closing-commit work — incremental, self-tested pieces.
Rounds 18 through 22a complete (2026-05-28); ARSENAL_VIRTIO_-
BALLOON_OK lights at 22a's third commit. First inherited
Linux driver online in Arsenal:**

  - Round 18 `16070ec`: real atomic test_and_set_bit /
    test_and_clear_bit (LOCK-prefixed AtomicU64::fetch_or /
    fetch_and over the unsigned-long bitmap word) + self-test.
  - Round 19 `c52ea00`: real struct page lifecycle — alloc_pages
    (order 0) + put_page + page_address over the frame allocator +
    HHDM bridge; balloon_page_alloc / _enqueue / _dequeue with a
    Rust mirror of struct balloon_dev_info; sg_init_one (buf - hhdm
    → dma_address); adjust_managed_page_count intentionally a
    no-op (balloon hot-path, M1 has no managed-page accounting).
    Self-test covers all three round-trips. free_pages stays panic
    (free-page-hint feature path, gated off at M1).
  - Round 20 `b291b95`: virtqueue bridge — 10 bridge fns in
    linuxkpi_bridge.rs wrap arsenal-kernel's Virtqueue
    (push_descriptor / push_chain / pop_used) + activate_queue +
    notify + set_driver_ok; the shim's virtqueue_add_outbuf/_inbuf/
    kick/get_buf/get_vring_size/virtio_find_vqs panic-stubs swap to
    real implementations routing through them, with shim-side
    per-queue ShimVirtqueueState (Box-leaked).
  - Round 21a `1deb3f1`: struct virtio_device gains a
    `features: u64` field; virtio_has_feature /
    __virtio_clear_bit / virtio_clear_bit panic-stubs go real
    over it. Self-test covers the bit-op state machine on a
    stack-allocated vdev.
  - Round 21d `06bbf85`: drops `static __init` from
    `module_driver`'s expansion so the
    `<driver_var>_init` wrapper symbol is externally callable.
    Lands [ADR-0008](docs/adrs/0008-module-init-by-symbol-name.md)
    documenting the "explicit call by symbol name" decision
    (vs initcall-style table); bumps the provisional-
    reservation list in ADR-0005 / 0006 down by one.
  - Round 21b `1382919`: register_virtio_driver drives the
    init_transport lifecycle. New linuxkpi_virtio_init_transport
    bridge does the v1.2 § 3.1.1 dance with **bus-side feature
    intersection** (read device_features, AND with driver's
    feature_table → u64 mask, write back, FEATURES_OK).
    virtio_device_ready / virtio_reset_device go real; per
    matched device the path is init_transport → validate (if
    non-null) → probe; on probe-decline the device is reset so
    a later driver (or virtio_blk::smoke / virtio_net::smoke)
    can re-init it from RESET.
  - Round 21c `eba56c6`: static virtio_config_ops table —
    .get is a panic-stub fn pointer (balloon's validate
    null-checks but doesn't call it during init); .del_vqs is
    a no-op (single balloon init, bounded leak at M1).
    register_virtio_driver installs &CONFIG_OPS on vdev.config
    before validate / probe.
  - Round 22a-step1 `c2b9760`: [ADR-0011](docs/adrs/0011-deferred-work-cooperative-runner.md)
    accepted. Resolves the deferred-work half of ADR-0005 § 6's
    panic-on-call state. Splits ADR-0008's previously-combined
    ADR-0011 reservation; initcall-table side stays provisional
    as ADR-0012, per-workqueue / freezable successor becomes
    provisional ADR-0013.
  - Round 22a-step2 `0e18ffb`: real workqueue — cooperative
    single-runner. linuxkpi_work_init + queue_work + cancel_-
    work + cancel_work_sync + alloc_workqueue + destroy_-
    workqueue + system_freezable_wq (non-null sentinel) +
    drain_one (runner-side primitive). arsenal-kernel spawns
    `workqueue_runner` cooperatively before sched::init.
    Self-test covers INIT_WORK + queue_work + drain_one +
    duplicate-queue idempotency + empty-queue → false +
    re-queue after idle.
  - Round 22a-step3 `b1043fc`: light virtio-balloon at boot.
    virtio_balloon.c into linuxkpi/build.rs's manifest with
    -DKBUILD_MODNAME='"virtio_balloon"' + -Wno-pointer-sign;
    Linux Kbuild's standard `-mno-sse -mno-mmx -mno-avx
    -msoft-float` added (clang -O2 emitted xorps in stack-
    zero-init and #UD'd at first instruction since CR4 has
    SSE off at M1). `extern "C" fn virtio_balloon_driver_init`
    called from arsenal-kernel/src/main.rs after self_test;
    `-device virtio-balloon-pci` in ci/qemu-smoke.sh;
    ARSENAL_VIRTIO_BALLOON_OK in REQUIRED_SENTINELS. **First
    inherited Linux driver online in Arsenal.**

Final smoke: 16/16 sentinels in ~1.4 s. ARSENAL_LINUXKPI_OK
asserts the shim self-test (now including the workqueue
round-trip); ARSENAL_VIRTIO_BALLOON_OK asserts balloon's
probe returned 0 against the real QEMU virtio-balloon-pci
device through the full lifecycle (init_transport with
feature intersection → validate → probe → INIT_WORK × 3 +
virtqueue allocation + virtio_device_ready).

**Rounds 22b-22d complete (2026-05-29): QMP-driven inflate
cycle closed end to end.** `ARSENAL_VIRTIO_BALLOON_INFLATE_OK`
is now in REQUIRED_SENTINELS; smoke is 17/17.
  - 22b/22c (`4179259`): `-qmp tcp:127.0.0.1:N,server=on,
    wait=off` + a fire-and-yield Python harness that waits for
    BALLOON_OK then sends `{"execute":"balloon",...}`; the
    config-changed MSI-X scaffolding (per-device IDT slot +
    handler ctx + `linuxkpi_virtio_set_msix_config_vector`);
    `adjust_managed_page_count` flips the one-shot sentinel.
  - 22d (`7934d05`, this session): the config MSI-X never
    reached the LAPIC despite every guest-side precondition
    being provably correct. **Root cause: the linuxkpi
    virtio-pci bring-up never set PCI Bus Master Enable.** An
    MSI is a bus-master memory write to the LAPIC; QEMU routes
    `msi_send_message` through the device's bus-master address
    space (BME-gated) and *silently drops* the write when BME
    is clear — so `msix_notify` ran, found nothing to gate on,
    and the message evaporated. The differential that isolated
    it: nvme/virtio-blk had BME set (`0x0107`) and delivered
    MSI-X; balloon/net/rng had it clear (`0x0103`), and
    balloon's config IRQ was the system's first virtio MSI so
    the gap had never surfaced. Fix: `register_virtio_driver`
    enables bus mastering during transport bring-up (mirrors
    Linux's `vp_modern_probe → pci_set_master`). Folded into
    the same commit: the `pci_to_virtio_id` transitional-device
    subsystem-ID fix (the old `pci_id - 0x1000` matched
    balloon's id_table against QEMU's RNG) and real
    `si_meminfo` / `si_mem_available` over two new frame-count
    bridge fns (balloon's stats-vq prime panicked on the stub).

**Worth-recording trap for every future MSI-X driver:** PCI
Bus Master Enable is a hard precondition for MSI delivery, not
just for descriptor/ring DMA. QEMU drops MSI writes when BME is
clear with no diagnostic. The native NVMe driver set it; the
shim path didn't. Any new device that delivers interrupts via
MSI/MSI-X must have BME set first. (Debugging technique that
cracked it: read PCI config space live via q35 ECAM —
`xp 0xb0000000 + (dev<<15) + offset` under QMP — and compare
COMMAND registers across working vs broken devices.)

Final smoke: 17/17 sentinels, boot→prompt ~210 ms. QMP reports
a real `BALLOON_CHANGE` (~8 MiB inflated) — virtballoon_changed
→ workqueue runner → inflate → adjust_managed_page_count now
runs as a closed loop. M1-2-5 Part B sub-task 3's config-change
path is done; the runner is no longer idle on a live config update.

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
