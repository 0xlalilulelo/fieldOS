# Field OS

A from-scratch desktop operating system in HolyC for x86_64 — and later
ARM64 Snapdragon X and Apple Silicon M1/M2. Inspired by the technical
primitives of TempleOS (HolyC as the universal language, the Brief
executable-document format, the shell-is-the-compiler REPL, the F5
hot-patch live-coding loop, the source-as-documentation `#help_index`
model, the line-count discipline) and the visual identity of macOS Big
Sur (translucent vibrancy, 8/12/20 px corner radii, 4 px spacing grid,
IBM Plex SIL OFL typography).

Field OS is paged, user/kernel-separated, preemptively scheduled. It
carries forward none of TempleOS's religious framing or
ring-0 / identity-mapped architecture. It happens to feel as immediate
and direct as Terry Davis's original.

Built by one person, evenings and weekends, on a multi-year arc.

## Status

See [`STATUS.md`](STATUS.md). The project is in **Phase 0 — QEMU Proof
of Concept**.

## Plan

The full multi-year roadmap is in [`PLAN.md`](PLAN.md), with detailed
phase plans in [`docs/plan/`](docs/plan/):

- **Phase 0** (M0–M10): QEMU PoC. HolyC on bare metal, Brief renderer,
  software compositor, PS/2 input, BGA framebuffer. 12–18 months
  part-time.
- **Phase 1** (M11–M50): Real hardware on Framework 13 AMD. v0.1
  release. 18–30 months part-time.
- **Phase 2** (M51–M90): Snapdragon X bring-up, WASM Tabernacles,
  pro-tier apps, stable ABI. v1.0 release. 24–36 months part-time.
- **Phase 3** (M91–M130): Apple Silicon M1/M2, full tablet experience,
  Stencil and Sequence apps. v2.0 release. 24–36 months part-time.

Total to v2.0: 6–9 calendar years from M0.

## Build

The Phase 0 build chain is not yet wired. Once M0 lands, the canonical
loop will be:

```
make iso && tools/qemu-run.sh
```

## Naming

System component names follow the catalog in
[`docs/naming.md`](docs/naming.md). The aesthetic is MGS3-warm and
tactical: Patrol, Stage, Channel, Cardboard Box, Cache, Operator,
Brief. No religious framing, ever.

## License

BSD-2-Clause. See [`LICENSE`](LICENSE). Driver shims and vendored
libraries retain their upstream licenses at the LinuxKPI / Cardboard
Box boundary.
