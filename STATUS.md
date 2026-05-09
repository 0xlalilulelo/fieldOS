# STATUS

> What I am doing right now. Updated whenever the milestone changes,
> a major design decision lands, or a session leaves something
> mid-flight that future-me needs to know about.

## Current milestone

**Arsenal M0 — Boot and breathe** *(0–9 months per ARSENAL.md timeline)*

### Active work

**M0 step 1 — first boot (complete, 2026-05-09).** Cargo workspace +
`arsenal-kernel` crate + Limine handshake + COM1 sentinel
`ARSENAL_BOOT_OK` landed across seven commits ending at `304bfa2`.
Boot time ~0.5 s under headless QEMU TCG locally; CI green on
`ubuntu-24.04` in ~1 minute end-to-end (apt install → rustup target →
clippy → `cargo xtask iso` → `ci/qemu-smoke.sh`). Devlog at
[`docs/devlogs/2026-05-arsenal-first-boot.md`](docs/devlogs/2026-05-arsenal-first-boot.md).

**M0 step 2 — paging + GDT + IDT (next).** 4-level paging mapped
against the Limine `MemoryMapRequest` response; GDT with kernel CS/DS
plus a TSS; IDT with 256 stub handlers, exceptions 0–31 routed to a
shared panic path; IST stacks for `#DF` / `#NMI` / `#MC`; a first
allocator under `core::alloc::GlobalAlloc` so subsequent steps have a
heap. Per ARSENAL.md § "Three Concrete Starting Milestones" → M0,
calibrated as 2–4 FT-weeks of work — ~5–10 calendar weeks at the
project's 15 h/wk part-time rate.

### After step 2

Arsenal M0 in full per ARSENAL.md → M0 steps 3–6: virtio block +
virtio-net, smoltcp + rustls, basic scheduler, framebuffer console,
SMP, boot to a `>` prompt. Performance gate: boot to prompt in < 2 s
under QEMU. Security gate: zero `unsafe` Rust outside designated FFI
boundaries. Usability gate: prompt is keyboard-navigable; shows
hardware summary. Estimated 9 calendar months total per the new
timeline.

## Last completed milestone

**Field OS PoC v0.1** (tag `field-os-v0.1`, commit `dffe259`,
2026-05-08). M3 step 6-5: per-eval cctrl reset, the HolyC REPL working
in QEMU under `make repl-iso`. Encoder byte-equivalent with GAS across
a 63-instruction corpus; JIT path landed `X` on serial through a
six-step pipeline (parse → codegen → encode → relocate → commit →
invoke); the M3 5-line exit-criterion session worked in miniature.
~6,274 LOC of base-system C across 56 files at the high-water mark.

The C kernel is preserved at the tag; `git checkout field-os-v0.1`
resurrects it. Bringing it back into `main` would require reverting
Phase B's removal commit.

## Earlier milestones

**M2 — Memory Management** (2026-05-05 → 2026-05-06, four commits,
+1,814 LOC). Tag `M2-complete` on commit `6cd9855`. PMM + VMM + slab.

**M1 — Boot to Long Mode** (2026-04-30 → 2026-05-04, four commits,
+700 LOC). Tag `M1-complete` on commit `c211cf8`. GDT + TSS, IDT, BGA
framebuffer with 8×8 font, "Hello, Field" rendered.

**M0 — Tooling and Bootstrap** (2026-04-29 → 2026-04-30, six commits,
~190 LOC base-system C, ~21,000 LOC vendored). Tag `M0-complete` on
commit `60e1a48`. Cross-GCC toolchain, Limine vendored, `make iso`
producing a bootable ISO.

These tags remain in place; the work is preserved at `field-os-v0.1`
along with everything else from the Field OS arc.
