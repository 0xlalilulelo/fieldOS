# Arsenal

A from-scratch desktop operating system written primarily in Rust,
targeting commodity 2026 hardware (Framework 13 AMD/Intel, Snapdragon X
laptops, Apple Silicon M1/M2 Macs, generic AMD/Intel desktops).

Arsenal commits explicitly that **performance, usability, and security
are peer concerns**. None is subordinated to the others. When two
pillars conflict, the resolution is an Architecture Decision Record,
not a silent ranking.

The kernel is a Rust monolith with capability-secured userspace.
Drivers are inherited from Linux 6.12 LTS via a LinuxKPI-style shim.
The compositor is a custom wgpu/Skia "Stage" rendering an iDroid + Big
Sur fusion identity. Applications ship in three tiers: native Rust,
sandboxed Wasm components (WASI 0.2 → 0.3), and a curated POSIX subset
(relibc-style) for ports of Firefox / mpv / git / foot.

Built by one person, evenings and weekends, on a multi-year arc — solo
today, designed to support a small-team transition around year 3–4.

## History

Arsenal was previously called **Field OS** and explored a TempleOS-
modernization framing in HolyC. The project pivoted on technical merit
on 2026-05-08 at the `field-os-v0.1` tag. Decision rationale is in
[`docs/adrs/0004-arsenal-pivot.md`](docs/adrs/0004-arsenal-pivot.md).

The C kernel from the Field OS arc is preserved at the tag;
`git checkout field-os-v0.1` resurrects it. The naming catalog and
visual identity carry forward; the language and architecture do not.

## Status

See [`STATUS.md`](STATUS.md). The project is in **Pre-M0 — Field OS →
Arsenal transition**. After the transition completes, ARSENAL.md M0
(boot and breathe) begins; ~9 calendar months part-time per the
timeline.

## Plan

The canonical plan is [`docs/plan/ARSENAL.md`](docs/plan/ARSENAL.md).
Milestones:

- **M0 — Boot and breathe** (months 0–9). Rust kernel skeleton, UEFI
  via Limine, serial console, virtio drivers in QEMU, basic scheduler,
  smoltcp + rustls, simple shell. Boots to a `>` prompt in QEMU.
- **M1 — Real iron** (months 9–24). LinuxKPI shim, amdgpu KMS, NVMe /
  xHCI / iwlwifi, first boot on real Framework 13 AMD. Slint app in
  software-rendered framebuffer.
- **M2 — It looks like Arsenal** (months 24–36). Stage compositor with
  iDroid/Big Sur identity, Wayland shim, first five native apps.
  **First public alpha.**
- **v0.5** (months 42–60). Wasm component runtime, POSIX/relibc subset,
  ports of Firefox / mpv / foot, Brief notebook app, Cardboard Box.
- **v1.0** (months 60–84). Daily-driver maturity on Framework 13 AMD.
  Snapdragon X port. Cassette / Stencil / Sequence apps. Mail / music /
  video / IDE. Accessibility shipped.
- **v2.0** (months 84–120). Apple Silicon via Asahi collaboration.
  Tablet experience. CHERI experimental support if commodity silicon
  arrives.

Total to v1.0: 5–7 calendar years. Total to v2.0: 7–10 calendar years.
Calibrated against Redox / SerenityOS / Genode / Asahi.

## Build

The Rust scaffolding lands in Phase C of the transition (currently in
progress). Once M0 step 1 lands, the canonical loop will be:

```
cargo build --release
cargo xtask iso
ci/qemu-smoke.sh arsenal-poc.iso
```

asserting `ARSENAL_BOOT_OK` on COM1 within seconds. Until then, the
historical Field OS build (`make iso && ci/qemu-smoke.sh`) works at
the `field-os-v0.1` tag.

## Naming

System component names follow the catalog in
[`docs/naming.md`](docs/naming.md). The aesthetic is MGS3-warm and
tactical: Patrol, Stage, Cache, Operator, Cardboard Box, Comm Tower,
Inspector. No religious framing, ever.

## License

BSD-2-Clause. See [`LICENSE`](LICENSE). Inherited Linux drivers
(landing in M1) retain their original GPLv2 at the LinuxKPI shim
boundary; Arsenal ships as a *combined work* with explicit license
boundaries (the FreeBSD / drm-kmod pattern).
