# Legacy skills

Field OS-era skill documents preserved as historical record after the
pivot to Arsenal on 2026-05-08. Future Arsenal skill documents (LinuxKPI
shim patterns, Slint UI authoring, wgpu compositor work, Wasmtime
embedding, smoltcp integration) land directly in `docs/skills/` and are
written from scratch when those layers come online.

## What's here

- **`holyc-lang-audit.md`** — the audit-before-grafting skill that
  shaped the M3 HolyC graft. Six-step roadmap (fork, wire to host
  transpiler, strip host-assumption surface, solve assembly handoff,
  wire to .text-allocator, REPL). The pattern itself — "audit a
  vendored upstream before grafting it into a kernel" — generalizes
  to any large vendored crate Arsenal needs to integrate, but the
  specific content is HolyC-only.

The audit *pattern* (not this document) is referenced in the pivot
devlog [`docs/devlogs/2026-05-arsenal-pivot.md`](../../devlogs/2026-05-arsenal-pivot.md)
under "What was learned" as a discipline that carries forward.
