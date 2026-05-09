# M0 step 1 — Arsenal first boot

*May 8 to May 9, 2026. Three sessions. Seven commits.*

Phase C of the Field OS → Arsenal transition was the milestone where
Arsenal stopped being prose. The exit criterion was specific and
small: the Cargo workspace builds, `cargo xtask iso` assembles a
bootable hybrid BIOS+UEFI ISO, and QEMU prints `ARSENAL_BOOT_OK` on
COM1 within seconds. That criterion is now met. The Rust kernel runs.

This is M0 step 1 of ARSENAL.md's M0 milestone — boot, breathe, no
more. No paging, no GDT, no scheduler, no allocator, no drivers. The
kernel writes one line to serial and `hlt`s in a loop. Steps 2 through
6 of M0 carry the boot path forward to a `>` prompt over the next
~9 calendar months.

## What landed

Seven commits across three sessions, in order:

- `c082d33` *chore: rust toolchain pin and Cargo workspace skeleton.*
  `rust-toolchain.toml` pins Rust 1.85 — the stable release that
  landed `x86_64-unknown-none` (February 2025). Workspace `Cargo.toml`
  declares `panic = "abort"` on both profiles (no_std + no_main rules
  out unwinding). `.gitignore` adds `target/` and drops two stale
  rules left by Phase B.
- `4bb8f10` *feat(kernel): no_std no_main arsenal-kernel crate
  skeleton.* `#![no_std]` + `#![no_main]`, single `_start` halting in
  `hlt`, panic handler. The linker script at `arsenal-kernel/linker.ld`
  puts `_start` at `0xffffffff80000000` — the canonical -2 GB
  higher-half base — paired with `code-model=kernel` so all symbol
  references fit in 32-bit signed displacements. `.cargo/config.toml`
  wires the linker flags target-scoped; the host-side xtask crate
  is unaffected.
- `dc53815` *feat(kernel): boot via Limine, emit ARSENAL_BOOT_OK on
  COM1.* `limine = "0.5"` (the version dance is below), a
  `BASE_REVISION` static in `.requests` declaring protocol revision
  3, and an ~80-line polled COM1 driver. Each `unsafe` block carries
  a `// SAFETY:` comment naming the precondition the call site
  upholds, per CLAUDE.md §3.
- `c126674` *style(kernel): name COM1 register offsets.*
  `clippy::identity_op` flagged `COM1_BASE + 0` as a no-op. Replaced
  the offset arithmetic with named `PORT_*` constants — silences the
  lint, and the init sequence now reads by register name rather than
  by 16550 spec table line number.
- `faa0cb7` *feat(xtask): cargo xtask iso assembles bootable Arsenal
  ISO.* Host-only `xtask` crate. The `iso` subcommand drives
  `cargo build` for the kernel + xorriso for ISO synthesis +
  `limine bios-install` for the BIOS boot path. The xorriso
  incantation is cribbed verbatim from `field-os-v0.1:Makefile`,
  proven against Limine v12.0.2. This commit also fixed the
  start/end marker bug (next section).
- `b74af3e` *test(smoke): ARSENAL_BOOT_OK serial assertion under
  QEMU.* `ci/qemu-smoke.sh` adapted from
  `field-os-v0.1:ci/qemu-smoke.sh`. Same headless TCG discipline,
  same distinct-exit-codes-per-failure-mode pattern. Field OS's JIT
  witness assertion stripped — that pipeline does not exist in
  Arsenal.
- `304bfa2` *ci: arm cargo xtask iso + ARSENAL_BOOT_OK smoke.*
  GitHub Actions workflow on `ubuntu-24.04`. apt installs
  qemu-system-x86 + xorriso + build-essential, rustup target add,
  clippy on both crates, then `cargo xtask iso` + `ci/qemu-smoke.sh`.
  ~1 minute end-to-end on a hosted runner.

## How long it took

Three evening sessions on Apple Silicon, 2026-05-08 to 2026-05-09.
On the order of four hours of active time. The HANDOFF.md kickoff
calibrated this as 2–3 calendar weeks at the project's 15 h/wk
part-time rate; it landed in two calendar days because the runtime
sub-tasks were each individually small and the build loop (cargo +
xtask + smoke) was tight enough for fast iteration. That is a
property of M0 step 1, not a forecast about M0 step 2 onwards. Paging
+ GDT + IDT — the M0 step 2 work — is where bugs first hide deeply
in this kind of kernel.

## Detours worth recording

Three things broke that the Limine docs and OSDev wiki did not warn
about clearly. Recording them so future-me does not lose another
half hour next time.

**The `limine` crate version dance.** HANDOFF.md called for
`limine = "1.x"` from crates.io. Reality: the latest published
version is `0.6.3`, and 0.6 requires nightly Rust
(`feature(ptr_metadata)` for trait-object metadata on a stable type).
0.4.0, 0.3.x, and 0.2.0 are all yanked. 0.1.x predates the v12 base
revision 3 marker layout. The only published version that compiles
on stable Rust *and* matches Limine v12.0.2's expected request
format is `0.5.0`. About five minutes of `cargo info limine` and
`crates.io/api/v1/crates/limine/versions` spelunking. The CLAUDE.md
ethos prefers stable Rust when it suffices; for M0 step 1, 0.5
suffices.

**Limine v12 base revision 3 silently rejects requests without
start/end markers.** First boot attempt hung at `limine: Loading
executable boot():/boot/arsenal-kernel...`. The actual failure mode
was downstream: the kernel ran, but `BASE_REVISION.is_supported()`
returned `false` because Limine never processed the marker — without
an explicit `RequestsStartMarker` / `RequestsEndMarker` pair around
the `.requests` section, Limine v12 does not bound-scan the kernel
image for requests. `_start` halted before COM1 init, and the
silence on serial looked like a hang rather than a code path
executed. About thirty minutes of looking at ELF sections, extracting
`limine.conf` from the ISO, peeking at QEMU register state, and
finally reading the `limine` crate source for the marker types. The
[Limine PROTOCOL.md](https://github.com/limine-bootloader/limine/blob/v12.x/PROTOCOL.md)
describes the markers as "an optimization for fast scanning;" in v12
they are functionally mandatory.

**`limine bios-install` is required for the BIOS boot path even on
ISOs.** Without the bios-install pass, the El Torito record alone
does not place Limine's stage 2 where the BIOS firmware can find it.
QEMU sat in firmware execution at `EIP=0x7d47`, never reaching the
bootloader. The UEFI path worked without bios-install — only BIOS
needed the extra step. xtask now invokes `vendor/limine/limine
bios-install arsenal.iso` after xorriso; the host limine tool builds
from the vendored `limine.c` on demand. About ten minutes of "wait,
why is QEMU still in 16-bit mode."

None of these are M0-step-1-shaped problems. They are "first time
integrating with this version of this protocol on this exact stack"
problems — the kind of first-time-doing-this tax phase-0.md's risk
register §6 warned about by another name.

## The numbers

- **7 commits.** Each ships a self-contained piece of M0 step 1.
  Bisect surface clean.
- **548 lines of new content.** ~140 lines of Rust kernel code
  (`main.rs` + `serial.rs`), 65 lines of linker script, 146 lines of
  host Rust (`xtask`), 96 lines of bash (`qemu-smoke.sh`), and ~100
  lines of TOML / YAML configuration. The kernel itself fits on two
  screens.
- **9.2 KB ELF.** The static `arsenal-kernel` binary at
  `target/x86_64-unknown-none/release/arsenal-kernel`. Two LOAD
  segments: `.text` is 0x153 bytes (339 bytes of code), `.rodata`
  carries the 32-byte `RequestsStartMarker`, the 24-byte
  `BaseRevision`, and the 16-byte `RequestsEndMarker`.
- **17 MB ISO.** Same shape as the M0 Field OS ISO; most of the
  size is the upstream UEFI El Torito boot image, a fixed-format
  FAT container.
- **~0.5 seconds** wall time from QEMU launch to `ARSENAL_BOOT_OK`
  on serial under headless TCG locally. On the GitHub-hosted
  `ubuntu-24.04` runner, end-to-end (apt → rustup → clippy →
  xtask iso → smoke) is ~1 minute.

## What the boot looks like

The serial output is the entire M0 step 1 user-facing surface, and
it is one line:

```
ARSENAL_BOOT_OK
```

`limine.conf` carries `quiet: yes` and `timeout: 0` so Limine does
not write its TUI to anywhere visible. The kernel writes its
sentinel and `hlt`s in a loop. There is no compositor, no
framebuffer write, no input loop, no scheduler, no IDT — `hlt` will
return on any interrupt, and the loop catches the wake.

That is, deliberately, all M0 step 1 promises.

## What M0 step 2 looks like

Per ARSENAL.md § "Three Concrete Starting Milestones" → M0, step 2
is the first milestone where the kernel does something beyond
announcing itself:

- 4-level paging mapped against the Limine `MemoryMapRequest`
  response. Build PML4 + PDPT + PD + PT for the regions Limine
  reports usable; switch `CR3` to the kernel's own tables.
- GDT with kernel CS/DS, eventually user CS/DS, plus a TSS for IST
  stacks.
- IDT with 256 stub handlers, exceptions 0–31 routed to a shared
  panic path.
- IST stacks for `#DF` / `#NMI` / `#MC` so the kernel can crash
  cleanly even from a broken state.
- A bump or fixed-region allocator under `core::alloc::GlobalAlloc`
  so subsequent steps have a heap.

Honest estimate per ARSENAL.md: 2–4 FT-weeks of work, ~5–10 calendar
weeks at the project's part-time rate. Long enough that the cadence
note below applies before M0 step 2 wraps.

## Cadence

This devlog is the once-a-step artifact for M0. M0 step 2's wrap
will get its own. Bi-weekly progress notes between steps if anything
notable surfaces. The pattern is calibrated against Asahi Linux's
blog cadence, which is the prior art that fits Arsenal's solo,
part-time shape best.

—
