# ADR-0005: LinuxKPI shim layout and GPL/BSD-2 boundary

## Status

Accepted. 2026-05-14. Subordinate to [ADR-0004](0004-arsenal-pivot.md)
(Arsenal's Rust-only base + LinuxKPI driver-inheritance commitment).
Operationalizes CLAUDE.md §3 ("BSD-2-Clause base; GPLv2 preserved on
inherited Linux drivers; combined-work model, the FreeBSD / drm-kmod
pattern") in source-tree terms.

## Context

M1 step 2 ("LinuxKPI shim foundation + first inherited driver") is
the M1 milestone HANDOFF's "single largest engineering task" — a
12–20 part-time-week block that builds the shim layer through which
amdgpu (M1 step 5), iwlwifi (M1 step 6), and the long tail of
inherited drivers thereafter will run. Per the M1 milestone HANDOFF
(git `9df4682`) and the M1-2 step HANDOFF (this commit's parent), the
shim is morale-load-bearing because nothing user-visible ships until
step 5 lights up amdgpu KMS on top of it.

Three structural questions must be resolved before any shim code
lands. None of them are individually load-bearing for correctness —
each has multiple workable answers — but the combination shapes
every per-driver kickoff for the rest of M1 and into M2:

1. **Where does the shim live in the source tree?** A single Cargo
   workspace member, a subdirectory of the kernel crate, or a split
   into header / shim / drivers crates?
2. **How are the inherited Linux .c source files compiled and
   linked?** Via the `cc` build-dep crate, via a custom `cargo xtask`
   harness, or via a host Linux Kbuild invocation?
3. **What gets vendored from Linux 6.12 LTS, and at what
   granularity?** A minimal hand-curated header subset, the full
   `include/linux/` mirror, or a build-time fetch?

Adjacent decisions that fall out of the same conversation and merit
recording in this ADR rather than three follow-ups:

4. The directory-level GPL/BSD-2 license boundary discipline — what
   files live where, how the build system enforces it.
5. The FFI direction (Rust-calls-C only vs bidirectional with shim
   functions exposed to C) and the header-generation strategy for
   the C-callable side.
6. Module init/exit semantics under our cooperative scheduler (M1's
   surface) — synchronous-at-boot vs deferred via kthread/workqueue.

Primary references consulted:

- ARSENAL.md § "Driver strategy" — commits to the LinuxKPI-style
  shim modeled on FreeBSD drm-kmod.
- FreeBSD drm-kmod source layout
  ([https://github.com/freebsd/drm-kmod](https://github.com/freebsd/drm-kmod))
  — the decade-deep precedent that the combined-work model is
  legally and practically workable.
- Linux 6.12 LTS source tree
  ([https://git.kernel.org/pub/scm/linux/kernel/git/stable/linux.git/log/?h=linux-6.12.y](https://git.kernel.org/pub/scm/linux/kernel/git/stable/linux.git/log/?h=linux-6.12.y))
  — the upstream baseline whose drivers we inherit and whose header
  subset we vendor.
- The `cc` build-dep crate
  ([https://crates.io/crates/cc](https://crates.io/crates/cc),
  MIT/Apache-2.0) — proposed C-toolchain integration.
- M1 milestone HANDOFF (git `9df4682`) §§ "LinuxKPI shim strategy"
  and "First inherited driver at step 2" — the milestone-level
  trade-off resolutions this ADR makes structural.

## Decision

### 1. Single Cargo workspace member `linuxkpi/`

The shim is one new workspace member, peer to `arsenal-kernel/` and
`xtask/`. `arsenal-kernel` depends on `linuxkpi` as a regular Cargo
crate. The directory layout at step 2 exit:

```
fieldOS/
  arsenal-kernel/                  (BSD-2-Clause)
    src/
    Cargo.toml
  linuxkpi/                        (BSD-2-Clause; new)
    src/
      lib.rs
      types.rs                     (M1-2-1)
      log.rs                       (M1-2-1: printk + pr_*)
      slab.rs                      (M1-2-1: kmalloc + kfree)
      locks.rs                     (M1-2-1: mutex + spinlock + atomic)
      pci.rs                       (M1-2-2: PCI bus adapter)
      irq.rs                       (M1-2-2: request_irq + DMA)
      virtio.rs                    (M1-2-3: virtio bus adapter)
    include/
      shim_c.h                     (M1-2-1+; hand-written)
    build.rs                       (M1-2-4)
    Cargo.toml
  vendor/
    limine/                        (BSD-3-Clause-ish, unchanged)
    spleen/                        (CC0/SIL-OFL, unchanged)
    linux-6.12/                    (GPLv2; new at M1-2-4)
      include/linux/*.h            (~20 hand-curated headers)
      drivers/virtio/virtio_balloon.c
  Cargo.toml                       (workspace)
```

**Alternatives rejected:**

- **Subdirectory `arsenal-kernel/src/linuxkpi/`** — moves the shim
  inside the kernel crate. Smaller workspace; tighter coupling.
  Rejected because the BSD-2 → GPLv2 link boundary then sits at the
  *kernel-crate ELF link step* rather than at a clean directory
  fence, blurring the combined-work model. The directory-based
  fence is an audit-friendly invariant a reviewer can verify by
  reading `ls`; an in-crate boundary requires reading the build
  graph.
- **Three-crate split: `linuxkpi-headers`, `linuxkpi-shim`,
  `linuxkpi-drivers`** — drm-kmod-shaped, with per-driver Cargo
  features. The right shape *eventually* — when the second
  inherited driver lands at step 5 (amdgpu) and pulls a different
  subset of the shim than virtio-balloon, the per-driver feature
  surface will justify the split. At step 2 with one inherited
  driver, it over-organizes for a future we don't have yet.
  **Reserved for a successor ADR** when amdgpu's surface confirms
  the split's value.

### 2. The `cc` build-dep crate compiles the inherited C

`linuxkpi/build.rs` invokes `cc::Build::new()` against the vendored
.c files with the cross-compile flag set:

```rust
// linuxkpi/build.rs (sketch; landed at M1-2-4)
fn main() {
    cc::Build::new()
        .compiler("clang")
        .target("x86_64-unknown-none")
        .flag("-nostdinc")
        .flag("-ffreestanding")
        .flag("-fno-stack-protector")
        .flag("-mno-red-zone")
        .flag("-mcmodel=kernel")
        .include("../vendor/linux-6.12/include")
        .include("include")
        .file("../vendor/linux-6.12/drivers/virtio/virtio_balloon.c")
        .compile("linuxkpi-drivers");
}
```

The resulting object archive links into the kernel ELF as a
combined work, the FreeBSD drm-kmod pattern. `cc` is a build-dep
only — it does not appear in the kernel's runtime profile and adds
no link-time dependency to the binary.

**Alternatives rejected:**

- **`cargo xtask build-linuxkpi`** — full control via our existing
  xtask harness; no external build-dep. Rejected on maintenance
  cost: cc handles cross-compile flag normalization, output object
  layout, and incremental rebuild bookkeeping that we'd otherwise
  re-implement. The build-dep surface is one MIT/Apache-2.0 crate
  (`cc`) used by half the Rust ecosystem; the maintenance trade is
  not close.
- **Host Linux Kbuild** — bind to the host's `/usr/src/linux-6.12`
  kernel build system. Maximum upstream-fidelity. Rejected because
  CI runs on macOS-hosted GitHub runners (the smoke harness's
  Python TLS listener requirement); binding to host Kbuild breaks
  the runner's cross-platform reach. Rejected even *with* a Linux
  CI runner: the dev machine would also need a full Linux source
  tree, breaking solo-dev on macOS.

### 3. Minimal hand-curated header subset under `vendor/linux-6.12/`

Only the .h files transitively `#include`d by the .c files we
host get vendored. At step 2 exit, that's ~20 headers driven by
`vendor/linux-6.12/drivers/virtio/virtio_balloon.c`:
`<linux/types.h>`, `<linux/printk.h>`, `<linux/kernel.h>`,
`<linux/gfp.h>`, `<linux/slab.h>`, `<linux/mutex.h>`,
`<linux/spinlock.h>`, `<linux/atomic.h>`, `<linux/err.h>`,
`<linux/list.h>`, `<linux/kref.h>`, `<linux/wait.h>`,
`<linux/workqueue.h>`, `<linux/interrupt.h>`, `<linux/pci.h>`,
`<linux/virtio.h>`, `<linux/virtio_config.h>`,
`<linux/virtio_balloon.h>`, plus whatever each of those
transitively pulls (typically a handful of `<asm/...>` and
`<uapi/linux/...>` shims that the shim's own header set
intercepts).

The vendoring discipline:

- **Verbatim from upstream Linux 6.12 LTS** — the SHA of the
  source tag goes in the commit body of the `feat(linuxkpi):
  vendor Linux 6.12 LTS subset` commit at M1-2-4. The
  `vendor/linux-6.12/` subdirectory's `README.md` records the
  pin and the upstream URL.
- **Original SPDX header preserved unchanged** — every `.h` and
  `.c` file ships its upstream `// SPDX-License-Identifier: GPL-2.0`
  (or `Dual BSD/GPL` etc.) header without modification.
- **Per-new-driver expansion at each step kickoff** — when M1
  step 5 (amdgpu) and step 6 (iwlwifi) land, the kickoff
  HANDOFFs include a `find-include-graph` audit that enumerates
  the additional headers to vendor. New headers are added in the
  same commit as the inherited driver they support.
- **No local patches** — if a driver needs modification (a
  hardware quirk that QEMU surfaces but Linux upstream hasn't
  shipped a fix for, etc.), the patched copy lives in
  `vendor/linux-6.12-arsenal/` (separate directory), the diff
  against upstream is documented inline in a `MAINTAINERS.md`
  in that directory, and the patched driver replaces the
  unmodified one in the build graph for that specific build.
  The unmodified upstream copy stays in `vendor/linux-6.12/`
  for audit comparison.

**Alternatives rejected:**

- **Full Linux 6.12 LTS `include/` tree mirrored** — vendor
  ~10K header files, never worry about per-driver expansion.
  Rejected on audit-surface grounds: a reviewer cannot tell at
  a glance which headers the project actually depends on; a
  CVE in (say) `<linux/bpf.h>` surfaces as a question we can't
  answer from `ls`. The minimal-subset discipline keeps the
  GPL-licensed surface visible at every PR.
- **Lazy mirror via build-time fetch** — `build.rs` clones the
  Linux source on first build. Rejected because offline builds
  break, bisect breaks (the upstream HEAD moves under us), and
  CI cache invalidation becomes a debugging nightmare.

### 4. Directory-based GPL/BSD-2 boundary, build-system enforced

The license boundary is a directory invariant, not a per-file
SPDX comment to grep for:

- Every file under `linuxkpi/` is **BSD-2-Clause**, with the
  `// SPDX-License-Identifier: BSD-2-Clause` header at top.
- Every file under `vendor/linux-6.12/` (and any future
  `vendor/linux-6.12-arsenal/`) retains its **upstream Linux
  SPDX header unchanged** — typically GPL-2.0, occasionally
  Dual BSD/GPL or LGPL-2.1.
- `arsenal-kernel/` and `xtask/` remain BSD-2-Clause.
- `linuxkpi/build.rs` refuses to compile any `.c` source file
  outside `vendor/linux-6.12*/` and refuses to compile any
  `.rs` source file inside `vendor/`. The check is one pattern
  match per source path, run before `cc::Build::file(...)`.

The combined-work outcome: the linked kernel ELF contains
BSD-2 Rust object code (from `arsenal-kernel`, `linuxkpi`, and
the vendored Rust crates whose licenses are MIT/Apache-2.0/ISC
per CLAUDE.md §3) plus GPL-2.0 C object code (from
`vendor/linux-6.12/`). This is the FreeBSD drm-kmod model
exactly; the legal precedent and operational discipline are
both well-established.

### 5. Bidirectional FFI; hand-written `linuxkpi/include/shim_c.h`

The shim is a two-way bridge. Inherited C code calls Rust shim
functions (`printk`, `kmalloc`, `pci_register_driver`,
`request_irq`, etc.) declared `extern "C"` and exposed via
`linuxkpi/include/shim_c.h`. Rust kernel code calls inherited
driver entry points (`virtio_balloon_init`, etc.) by their
exported `extern "C"` names directly.

`shim_c.h` is **hand-written**, not generated. At step 2 exit
it is ~200–400 lines covering the API surface 2-1, 2-2, and
2-3 expose. The maintenance cost is small relative to the
build-dep cost of `cbindgen` (MPL-2.0, which would need
CLAUDE.md §3 attention before adoption).

**Reserved for a successor ADR:** revisit `cbindgen` adoption
when the shim's C-callable surface grows past the
maintainability threshold — provisionally at M1 step 5
(amdgpu's surface arrives) or whenever `shim_c.h` exceeds
~1500 lines, whichever comes first.

### 6. Synchronous module init/exit at M1; deferred path stubbed

`linuxkpi::probe_drivers()` is called explicitly from
`arsenal-kernel/src/main.rs` after `pci::scan` and after
`nvme::smoke`. Registered drivers' `module_init` fires
synchronously, then their `.probe(struct pci_dev *)` runs
against discovered devices.

The deferred-init shim primitives (`schedule_work`,
`queue_work`, `kthread_run`) are exposed as `extern "C"` stubs
at M1-2-1 that **panic-on-call**. The fail-loud behavior
matches the M1 milestone HANDOFF's "fail loudly rather than
silently misbehave" guidance; any inherited driver that needs
the deferred path will surface its dependency at probe time
rather than at runtime.

The deferred path lands when an inherited driver actually
needs it — likely M1 step 5 or step 6. This ADR does not
forecast that work; the next ADR will.

## Consequences

**Easier:**

- **License boundary is visible by `ls`.** A reviewer can
  audit GPL vs BSD-2 surface by reading directory names. The
  build-system enforcement (`build.rs` refuses cross-boundary
  compiles) catches accidental cross-pollination at compile
  time, not at code review.
- **Per-driver bisect granularity preserved.** Adding amdgpu
  at M1 step 5 or iwlwifi at M1 step 6 is a `find-include-graph`
  audit + a vendor-header expansion + per-driver shim API
  gap-filling — each step's contribution to the shim is
  visible in git diff against the prior step's exit.
- **The shim's Rust surface is one Cargo crate.** Standard
  `cargo doc` / `cargo clippy` / `cargo test` paths apply.
  No bespoke build-system reasoning to onboard a new
  contributor (when the project transitions to small-team
  per ARSENAL.md year 3–4).
- **`cc`-driven C compile is one build-dep with predictable
  semantics.** Cross-compile flag plumbing, incremental rebuild
  bookkeeping, output object layout — all delegated to a
  well-maintained build crate. Maintenance cost is one line in
  `Cargo.toml`.
- **Shim API surface stays minimal by construction.** The
  per-new-driver header-graph audit + the per-driver gap-filling
  at each step's "driver online" sub-block force the shim to
  grow only in response to a concrete inherited-driver
  requirement. ARSENAL.md's "no shared kernel state beyond
  explicit shim interfaces" security gate becomes a structural
  invariant, not an aspiration.

**Harder:**

- **Per-new-driver header curation is recurring work.** Every
  step that adds an inherited driver (3 if xHCI goes the
  LinuxKPI route, 5 amdgpu, 6 iwlwifi, plus any inherited driver
  added post-M1) starts with a `find-include-graph` audit and a
  vendor-header expansion commit. The audit is mechanical but
  not zero-cost — budget half a session per new driver kickoff.
- **`shim_c.h` must be hand-maintained.** No `cbindgen`-driven
  regeneration; every new shim function exposed to C requires a
  matching declaration in `shim_c.h`. Mitigation: the shim's
  function count grows slowly (the M1 milestone HANDOFF's
  "load-bearing 30 APIs" is the order of magnitude); 200–400
  lines is hand-maintainable indefinitely.
- **`build.rs` cross-compile flag set requires upfront thought
  at M1-2-4.** Linux's expected freestanding-kernel flag set
  (`-nostdinc -ffreestanding -fno-stack-protector -mno-red-zone
  -mcmodel=kernel`) plus our specific x86_64-unknown-none
  target plus clang version pinning needs careful first-pass
  testing. The first inherited driver's compile-failure
  iterations will surface flag gaps; budget extra time at
  M1-2-4.

**New risks:**

- **ABI drift between vendored header subset and upstream
  Linux 6.12 LTS.** If a transitive `#include` is missed at
  M1-2-4 and silently picked up from the cross-compiler's
  default search path (which `-nostdinc` should prevent, but
  only if the flag set is correct), the shim could compile
  against header A but the inherited driver against header B.
  Mitigation: `-nostdinc` is non-negotiable; the build-system
  check enforces it; the first inherited driver's compile is
  the canary.
- **Linux 6.12 LTS upstream pin drift.** If upstream Linux ships
  a security fix to one of our vendored headers, we don't
  notice unless we re-pin. Mitigation: a `vendor/linux-6.12/
  README.md` records the upstream tag; a documented quarterly
  re-pin checklist (deferred to M1 step 6's HANDOFF or sooner
  if a CVE forces the issue) keeps the subset current.
- **Per-driver gap-filling at each step's "driver online"
  sub-block is unpredictable in scope.** The M1-2 step HANDOFF
  budgets 3–5 sessions for M1-2-5 with explicit acknowledgement
  that gap-filling can double; the same pattern recurs at every
  inherited driver's first-online sub-block. Mitigation: the
  CLAUDE.md "step away for a day" cue applies; every gap-filling
  sub-block carries a `wip(linuxkpi):` branch as the partial-
  work checkpoint.

**Follow-up work:**

- Update `STATUS.md` § "Active work" to point at the M1-2-0
  artifacts (this ADR + the empty `linuxkpi/` crate skeleton).
  Step 2 sub-block tracking gets the M1-2-0 entry.
- ARSENAL.md does not need modification — this ADR
  operationalizes the existing § "Driver strategy" commitment;
  it does not change it. (Per CLAUDE.md, the plan is not to be
  edited unless the deviation is structural; this is an
  operationalization, not a deviation.)
- Reserved successor ADRs:
  - **ADR-0006 (provisional):** "Evolve LinuxKPI to three-crate
    split when amdgpu lands." Triggered at M1 step 5 kickoff.
  - **ADR-0007 (provisional):** "Adopt cbindgen for `shim_c.h`
    generation." Triggered when `shim_c.h` exceeds ~1500 lines
    or when a third inherited driver's C-callable surface
    expansion crosses a maintainability threshold.
  - **ADR-0008 (provisional):** "Deferred / event-driven module
    init via kthread + workqueue." Triggered when an inherited
    driver actually needs the path (likely amdgpu or iwlwifi).

## References

- [ADR-0004: Pivot from Field OS to Arsenal](0004-arsenal-pivot.md)
- [`docs/plan/ARSENAL.md` § "Driver strategy"](../plan/ARSENAL.md)
- [`CLAUDE.md` §3 (BSD-2-Clause base, GPLv2 preserved on inherited drivers)](../../CLAUDE.md)
- [`HANDOFF.md` (M1 step 2 kickoff, this commit's parent)](../../HANDOFF.md)
- M1 milestone HANDOFF (git `9df4682`) §§ "LinuxKPI shim
  strategy" and "First inherited driver at step 2"
- [FreeBSD drm-kmod source tree](https://github.com/freebsd/drm-kmod)
  — combined-work GPL/BSD precedent
- [Linux 6.12 LTS (longterm) branch](https://git.kernel.org/pub/scm/linux/kernel/git/stable/linux.git/log/?h=linux-6.12.y)
  — upstream baseline for inherited drivers and vendored headers
- [`cc` crate](https://crates.io/crates/cc) — MIT/Apache-2.0,
  cross-compile build dependency
- Linux kernel docs § "Process / Coding style"
  ([https://www.kernel.org/doc/html/latest/process/coding-style.html](https://www.kernel.org/doc/html/latest/process/coding-style.html))
  — applies to any local fork patches under
  `vendor/linux-6.12-arsenal/`
- Michael Nygard, "Documenting Architecture Decisions" (2011) —
  ADR template authority

---

*This ADR makes structural the M1 milestone HANDOFF's resolution
of the "LinuxKPI shim strategy" trade-off (hybrid: structural
foundation + incremental). It does not preclude the three-crate
evolution; it sequences it after a second inherited driver
demonstrates the need.*
