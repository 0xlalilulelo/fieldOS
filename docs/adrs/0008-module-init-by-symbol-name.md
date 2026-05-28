# ADR-0008: Inherited-driver module init by explicit symbol-name call

## Status

Accepted. 2026-05-28. Resolves the "DEFERRED DESIGN DECISION"
marker [ADR-0005 § 6](0005-linuxkpi-shim-layout.md) and the
banner comment in
[`linuxkpi/include/linux/module.h`](../../linuxkpi/include/linux/module.h)
both flagged for the M1-2-5-closing commit: *how* arsenal-kernel
synchronously reaches an inherited driver's init at boot.

Takes the ADR-0008 slot that
[ADR-0006](0006-linuxkpi-headers-are-shim.md) and
[ADR-0007](0007-struct-page-thin-handle.md) had each provisionally
reserved (the three-crate-split reservation that shifted from 0007
to 0008 across those two ADRs). The provisional reservations shift
up by one again — three-crate split → ADR-0009, cbindgen adoption
→ ADR-0010, deferred init via kthread+workqueue → ADR-0011 —
recorded as one-line edits to ADR-0005 § "Reserved successor ADRs"
and ADR-0006's tracking note in this ADR's accepting commit. The
reservations were always provisional and unwritten; module-init is
a decision being made now, so it takes the next live number.

## Context

[ADR-0005 § 6](0005-linuxkpi-shim-layout.md) committed Arsenal to
synchronous module init at M1: `linuxkpi::probe_drivers()` (or
equivalent) is called explicitly from `arsenal-kernel/src/main.rs`
after device discovery, registered drivers' `module_init` fires
synchronously, and the deferred-init shim primitives
(`schedule_work` / `queue_work` / `kthread_run`) stay
panic-on-call until a driver actually needs them.

That ADR did not pin down the *mechanism* by which the kernel
reaches a specific inherited driver's init function. The
M1-2-5 Part B sub-task 3 work surfaced the question concretely:
virtio-balloon's `virtio_balloon.c` ends with
`module_virtio_driver(virtio_balloon_driver)`, which (via
`module_driver`) generates a `virtio_balloon_driver_init(void)`
wrapper that calls `register_virtio_driver(&virtio_balloon_driver)`.
The wrapper exists; nothing yet calls it.

Two shapes were on the table at the M1-2-5 closing commit:

1. **Explicit call by symbol name.** arsenal-kernel declares the
   wrapper as `extern "C" fn virtio_balloon_driver_init() -> c_int;`
   and calls it directly from a fixed point in boot (after
   `linuxkpi::self_test()`, before `sched::run()`). One line per
   inherited driver, name well-known per Linux's `module_driver`
   macro convention.

2. **Initcall-style table.** Reproduce Linux's `__initcall(fn,
   level)` machinery — a custom linker section, a section-walker
   in arsenal-kernel that calls each entry in order, init-level
   ordering for cross-driver dependencies. Drivers register
   themselves into the section via a macro; the kernel discovers
   them at boot without a hard-coded name list.

The choice is what lights `ARSENAL_VIRTIO_BALLOON_OK` and shapes
how every future inherited driver wires its init at M1.

## Decision

Inherited-driver init at M1 is **an explicit call to the
`module_driver`-generated `<driver>_init` symbol from
arsenal-kernel's boot sequence**, by well-known name.

Concretely:

- `linuxkpi/include/linux/module.h`'s `module_driver` macro
  expansion drops `static` from the generated `<driver>_init` /
  `<driver>_exit` wrappers (and the `__init` / `__exit` markers
  expand to nothing at M1 per the same header). The wrappers
  become external C symbols with predictable names:
  `<driver_var>_init` and `<driver_var>_exit`.
- `arsenal-kernel/src/main.rs` declares each inherited driver's
  init wrapper as `extern "C" fn <name>() -> c_int;` and calls it
  at a fixed point in boot — after `pci::scan` and the LinuxKPI
  self-test, before `sched::run()`. The return value is checked;
  a nonzero return is a boot-time fatal (matches Linux semantics
  for a driver `init` that fails to register).
- The `module_init(initfn)` / `module_exit(exitfn)` macros in
  `linux/module.h` stay no-ops. They reference the wrapper names
  only to suppress `-Wunused`; nothing in the macro chain
  auto-invokes them. The wrappers are reached *only* by the
  explicit call from arsenal-kernel.

The first user is `virtio_balloon_driver_init` at M1-2-5
closing-commit round 22, the call that lights
`ARSENAL_VIRTIO_BALLOON_OK`. Every inherited driver added through
M1 follows the same shape: vendor the `.c`, add it to the
`build.rs` source manifest, add one `extern "C"` declaration plus
one call in `arsenal-kernel/src/main.rs`.

## Alternatives rejected

- **Initcall-style table with a linker section walker.** The
  shape Linux ships: a `.initcall<N>.init` section per priority
  level, an `__initcall` macro that places a function pointer
  into the section, a `do_initcalls()` boot routine that walks
  the sections in order. Powerful when driver count is large
  and cross-driver init ordering matters. **Rejected at M1**
  for three reasons: (1) the inherited-driver count at M1 is
  one (balloon at sub-task 3 close), and the budget is at most
  a handful through the rest of M1 — amdgpu, iwlwifi, possibly
  xHCI — well under the threshold where naming each by hand
  becomes a maintenance burden; (2) custom linker sections in
  a freestanding-no_std target need additional `target.json` and
  linker-script attention, and would push back the boot-up of
  ARSENAL_VIRTIO_BALLOON_OK by a non-trivial amount of yak
  shaving; (3) init ordering between inherited drivers is not
  yet a problem — at M1 each driver's init is independent given
  device discovery has already run. The table is the right
  successor when the count or ordering forces it; that is the
  ADR-0012 trigger ("initcall-style table for synchronous
  inherited-driver init" successor — see References below).
  *(Originally cited as the ADR-0011 trigger; ADR-0011's claim
  on slot 0011 split this provisional and shifted the
  initcall-table side to ADR-0012.)*

- **Explicit Rust calls to `register_virtio_driver(&driver_var)`
  directly from `arsenal-kernel/src/main.rs`, bypassing the
  `module_driver`-generated wrappers entirely.** Possible — the
  driver struct (e.g., `virtio_balloon_driver`) is a static
  symbol, and Rust could declare it `extern "C" static`. Rejected
  because it discards the Linux `module_driver` macro's
  call-site validation (the macro statically validates that
  `__register`'s signature matches the driver type, which surfaces
  ABI drift between balloon and the shim at compile time rather
  than at link time), and because a Linux driver's `<drv>_init`
  is a natural extension point — if balloon (or a future driver)
  adds per-init work above the bare `register_virtio_driver`
  call upstream, the wrapper carries it for free. Keeping the
  wrapper as the call site preserves the upstream shape and lets
  re-pins inherit any upstream init-side changes mechanically.

- **A C-side dispatcher in the shim (e.g.,
  `linuxkpi_run_inherited_inits(void)`) that hard-codes the call
  list inside the LinuxKPI crate's C portion.** Moves the name
  list from `arsenal-kernel/src/main.rs` to a `.c` file under
  `linuxkpi/`. Rejected because the BSD-2 Rust crate owns the
  boot orchestration per ADR-0005 § 6; the inherited-driver list
  is an arsenal-kernel concern, not a LinuxKPI-internal one.
  Keeping the per-driver `extern "C"` declarations in
  `arsenal-kernel/src/main.rs` puts the surface on the side of
  the boundary that drives it.

## Consequences

**Easier:**

- **One-line-per-driver wiring.** Adding an inherited driver at
  M1 step 5 (amdgpu) or step 6 (iwlwifi) means one `extern "C"`
  declaration plus one call in `arsenal-kernel/src/main.rs`.
  No linker sections, no macro authorship, no boot-time table
  walker. The diff for "add inherited driver" stays small and
  reviewable.
- **The `module_driver` macro stays semantically faithful to
  upstream.** Dropping `static` is the only material change to
  the expansion; the macro still emits the standard `<drv>_init`
  / `<drv>_exit` shape with the same register/unregister
  signature checks. Re-pinning a vendored driver to a newer
  Linux LTS does not require revisiting Arsenal's module-init
  mechanism.
- **No new freestanding-target machinery.** The boot path uses
  ordinary `extern "C"` calls — well-understood by `cargo build`,
  by `clippy`, and by the Limine handoff. The QEMU smoke test
  validates the path end-to-end at every commit.

**Harder / deferred:**

- **The init-call list lives by name in `arsenal-kernel/src/main.rs`.**
  Each inherited driver adds two lines (an `extern "C"`
  declaration and a call). The list is short by intent — M1
  expects to host single-digit inherited drivers — but it is
  hand-maintained and grows monotonically. **Mitigation:** the
  count is bounded by the M1 milestone scope; the ADR-0012
  successor (initcall-style table) is pre-identified with a
  concrete trigger.
- **Init ordering is encoded by source-line order in
  `arsenal-kernel/src/main.rs`.** If a future inherited driver
  needs to init after another inherited driver, the dependency
  is implicit in line order rather than declared. **Mitigation:**
  at M1 each inherited driver's init is independent given
  arsenal-kernel's pre-init device discovery; the implicit
  ordering matches the explicit "bus first, drivers second"
  shape already in `main.rs`. The successor table-driven model
  has explicit priority levels when the need arises.

**New risks:**

- **A driver whose init wrapper name is non-standard breaks the
  pattern.** Drivers that don't use `module_driver` /
  `module_virtio_driver` and instead define a hand-rolled
  `__init` function would need either a hand-written wrapper or
  case-by-case handling. **Mitigation:** the inherited drivers
  on the M1 roadmap (balloon, amdgpu, iwlwifi, xHCI) all use the
  standard macros; a driver that doesn't is an unusual case
  worth a one-line bespoke wrapper rather than a mechanism
  change.

## References

- [ADR-0005 § 6: Synchronous module init/exit at M1; deferred path stubbed](0005-linuxkpi-shim-layout.md)
  — the original commitment to synchronous init that this ADR
  operationalizes
- [`linuxkpi/include/linux/module.h`](../../linuxkpi/include/linux/module.h)
  — the `module_driver` macro this ADR finalizes (the
  `static`-drop edit lands in the same commit as this ADR)
- [`arsenal-kernel/src/main.rs`](../../arsenal-kernel/src/main.rs)
  — the boot site where `extern "C"` declarations and explicit
  calls land (first user: `virtio_balloon_driver_init` at
  M1-2-5 closing-commit round 22)
- [Linux 6.12 LTS `include/linux/module.h`](https://git.kernel.org/pub/scm/linux/kernel/git/stable/linux.git/tree/include/linux/module.h?h=linux-6.12.y)
  — upstream `module_driver` macro this expansion mirrors
- [Linux 6.12 LTS `include/linux/init.h`](https://git.kernel.org/pub/scm/linux/kernel/git/stable/linux.git/tree/include/linux/init.h?h=linux-6.12.y)
  — upstream `__initcall` machinery the ADR-0012 successor
  would adopt when triggered
- [ADR-0011: Deferred-work via a cooperative workqueue runner](0011-deferred-work-cooperative-runner.md)
  — resolved the deferred-work half of the previously-combined
  ADR-0011 reservation; balloon's M1-2-5 probe triggered it
  ahead of the inherited-driver-count side
- **ADR-0012 (provisional):** "Initcall-style table for
  synchronous inherited-driver init." The other half of the
  previously-combined ADR-0011 reservation, surviving as a
  separate provisional after ADR-0011's split. Triggered when
  the inherited-driver count passes the explicit-list
  maintainability threshold (heuristic: 5+ inherited drivers,
  or any cross-driver init-ordering requirement). The
  provisional reservation previously sat at ADR-0010 (ADR-0005
  § "Reserved successor ADRs", as shifted by ADR-0006 and
  ADR-0007), then at ADR-0011 (this ADR's claim on slot 0008);
  ADR-0011's claim on slot 0011 shifts it to 0012.
- Michael Nygard, "Documenting Architecture Decisions" (2011) —
  ADR template authority
