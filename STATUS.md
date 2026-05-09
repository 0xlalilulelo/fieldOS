# STATUS

> What I am doing right now. Updated whenever the milestone changes,
> a major design decision lands, or a session leaves something
> mid-flight that future-me needs to know about.

## Current milestone

**Arsenal M0 — Boot and breathe** *(0–9 months per ARSENAL.md timeline)*

### Active work

**M0 step 2 — paging + GDT + IDT (complete, 2026-05-09).** Five
substantive commits plus the nightly toolchain pin landed across two
sessions. The kernel now owns its allocator, GDT, IDT, and CR3 —
Limine's hand-off versions are all replaced. Smoke asserts both
`ARSENAL_BOOT_OK` and `ARSENAL_HEAP_OK` (the latter fires only if a
heap round trip survives the CR3 swap). End-to-end smoke in ~1 s
locally, ~45 s on `ubuntu-24.04`. Devlog at
[`docs/devlogs/2026-05-arsenal-paging.md`](docs/devlogs/2026-05-arsenal-paging.md).

Sub-commits:
- `f2663b5` ingest Limine memory map + bump allocator
- `ca6a390` GDT + TSS with IST stack reservations
- `8bfa5f2` pin nightly Rust toolchain for x86-interrupt ABI
- `556bcd2` IDT with stub handlers, IST routing for #DF/#NMI/#MC
- `9c38083` 4-level paging, take ownership of CR3
- `8cd0186` smoke requires ARSENAL_HEAP_OK after paging

**M0 step 3 — toward `>` prompt (next).** Per ARSENAL.md M0 the
remaining bullets: deep-clone page tables (so we can reclaim
`BOOTLOADER_RECLAIMABLE` physical RAM), real frame allocator,
linked-list (or buddy) allocator with a free path, basic scheduler,
virtio block + virtio-net, smoltcp + rustls, framebuffer console,
basic SMP, boot to a `>` prompt. Sub-step decomposition deferred to
the next session start; per ARSENAL.md this is the bulk of M0,
~6–8 calendar months of part-time work.

### Step 3 performance + security + usability gates (from ARSENAL.md)

- Performance: boot to prompt in < 2 s under QEMU.
- Security: zero `unsafe` Rust outside designated FFI boundaries.
- Usability: prompt is keyboard-navigable; shows hardware summary.

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
