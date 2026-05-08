# Legacy plan documents

These are the four-phase Field OS plan documents (`phase-0.md` through
`phase-3.md`), preserved as historical record after the project pivoted
to Arsenal on 2026-05-08.

The canonical Arsenal plan is [`docs/plan/ARSENAL.md`](../ARSENAL.md).
Decision rationale for the pivot is in
[`docs/adrs/0004-arsenal-pivot.md`](../../adrs/0004-arsenal-pivot.md).

The Field OS code these documents described is preserved at the
`field-os-v0.1` tag (commit `dffe259`); `git checkout field-os-v0.1`
resurrects it.

These documents are **not** maintained going forward. They are kept
for the historical record of how Field OS was planned, and so that a
future archaeologist can see the multi-year arc that shaped the
language and discipline that carried into Arsenal even though the
technical primitives did not.

## What they covered

- **phase-0.md** — QEMU PoC. M0 (toolchain) through M10 (PoC packaging).
  HolyC compiler on bare metal; Brief renderer; software compositor;
  PS/2 input; BGA framebuffer. 12–18 months part-time.
- **phase-1.md** — Real hardware on Framework 13 AMD. M11 through M50.
  LinuxKPI shim; AMDGPU/i915; Foundry GPU compositor; Comm Tower;
  Wavelength; Cardboard Box; Stockpile; Patch; accessibility v1.0;
  launch app suite. v0.1 release. 18–30 months part-time.
- **phase-2.md** — Snapdragon X bring-up. M51 through M90. WASM
  Tabernacles; Manual / Armory / Cassette / Negatives v2; stable ABI;
  SDK; remote Stockpile; 11 localizations. v1.0 release. 24–36 months
  part-time.
- **phase-3.md** — Apple Silicon M1/M2 via Asahi collaboration. M91
  through M130. Full tablet experience; Stencil; Sequence; cellular;
  server profile. v2.0 release. 24–36 months part-time.

## What changed in the pivot

Arsenal's timeline collapses these into M0 / M1 / M2 / v0.5 / v1.0 /
v2.0 (see ARSENAL.md). The substantive differences:

- HolyC → Rust. The "single-language base" hard constraint is dropped;
  Rust is the primary language with C only under the LinuxKPI driver
  boundary.
- The 100,000-LOC budget is dropped. Arsenal's discipline is
  performance / usability / security peer-concerns gates per milestone,
  not a global LOC ceiling.
- LinuxKPI driver inheritance moves from Phase 1 to M1 (months 9–24)
  — earlier and more central.
- Wasm components (WASI 0.2 → 0.3) replace the "WASM Tabernacles"
  framing as Arsenal's third app-distribution tier.
- The TempleOS-derived primitives (F5 hot-patch, source-as-documentation,
  the source-is-the-program model) are dropped. Inspector overlay
  (Genode Leitzentrale pattern) replaces F5; rendered docs (Manual app
  + Field Manual help system) replace source-as-documentation.

The naming catalog and visual identity carry forward unchanged. See
ARSENAL.md and `docs/naming.md` for the post-pivot canonical references.
