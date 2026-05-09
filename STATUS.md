# STATUS

> What I am doing right now. Updated whenever the milestone changes,
> a major design decision lands, or a session leaves something
> mid-flight that future-me needs to know about.

## Current milestone

**Arsenal M0 — Boot and breathe** *(0–9 months per ARSENAL.md timeline)*

### Active work

**M0 step 3 — toward `>` prompt.** Multi-block effort comprising 3A
(memory subsystem), 3B (scheduler), 3C (virtio), 3D (network),
3E (framebuffer), 3F (SMP), 3G (`>` prompt + perf gate). ARSENAL.md
budgets the bulk of M0's remaining 6–8 calendar months for this work.

**3A — memory subsystem completion (complete, 2026-05-09).** Frame
allocator + deep-clone page tables + linked-list heap with a real
free path + reclaim of `BOOTLOADER_RECLAIMABLE` into the frame pool.
Smoke now asserts `ARSENAL_BOOT_OK`, `ARSENAL_HEAP_OK`, and
`ARSENAL_FRAMES_OK`; final sentinel reports 61277 free / 61287 total
4-KiB frames on QEMU 256 MB. End-to-end smoke ~1 s locally, ~45 s
on `ubuntu-24.04`. Devlog at
[`docs/devlogs/2026-05-arsenal-mm-complete.md`](docs/devlogs/2026-05-arsenal-mm-complete.md).

3A sub-commits:
- `2719e3f` frame allocator over Limine memory map
- `3135ad6` deep-clone page tables, take ownership of all levels
- `f947d04` linked-list allocator with free path
- `df16d9f` reclaim BOOTLOADER_RECLAIMABLE into frame pool

**3B — scheduler skeleton (next).** Per-CPU data, task struct,
cooperative scheduler with yield points, two-task ping-pong demo.
Sub-commit decomposition deferred to next session start. Per the
3A → 3G shape, 4–6 commits across 2–3 sessions.

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
