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

**3B — scheduler skeleton (complete, 2026-05-09).** Panic handler
prints to serial; per-CPU data area; Task struct + 16-KiB Box-owned
kernel stacks; cooperative `switch_to` in `global_asm!` (callee-save
GP regs only); scheduler `init` / `spawn` / `yield_now` over an
`AtomicPtr<Task>` current + `Mutex<VecDeque<Box<Task>>>` runqueue;
two-task ping-pong demo printing `ping`/`pong` for three rounds each
before the last finisher prints `ARSENAL_SCHED_OK`. Smoke now asserts
four sentinels; first switch crosses cleanly off Limine's boot stack
into the idle task's heap-allocated stack. ELF ~47 KB, smoke still
~1 s locally.

3B sub-commits:
- `da3627e` panic handler prints to serial before halt
- `7795073` per-CPU data area
- `b2c748c` task struct + 16 KiB kernel stacks
- `7eadc79` cooperative context switch via global_asm
- `46b005f` scheduler init, spawn idle task
- `1264c20` ARSENAL_SCHED_OK after ping-pong demo

**3C — virtio bring-up (complete, 2026-05-09).** PCI configuration
scan via legacy CF8/CFC; virtio modern PCI transport probe with cap
list walk through HHDM-mapped MMIO; split-virtqueue infrastructure
(desc + avail + used in one 4-KiB frame for sizes ≤ 128); virtio-blk
sector-0 read asserting the hybrid-ISO MBR signature 0xAA55;
virtio-net probe-TX with 8-buffer RX pre-population and a 64-byte
zero TX frame round-trip via QEMU slirp. Smoke grew two sentinels
(`ARSENAL_BLK_OK`, `ARSENAL_NET_OK`); the QEMU command line gained
`-device virtio-rng-pci`, `-drive ... -device virtio-blk-pci`, and
`-netdev user ... -device virtio-net-pci`. ELF ~81 KB, smoke still
~1 s locally. The detour was `paging::map_mmio` — Limine's HHDM
covers RAM only, so device MMIO regions need explicit mapping
through a `FRAMES`-backed `OffsetPageTable`.

3C sub-commits:
- `d4ea3d2` PCI config-space scanner
- `1d90405` virtio modern PCI transport
- `8764f62` virtqueue rings + descriptor mgmt
- `bc6ccfc` virtio-blk + sector-0 read smoke
- `174127b` virtio-net + probe TX smoke

**3D — smoltcp + rustls (complete, 2026-05-11).** smoltcp 0.12 on
top of the virtio-net Phy adapter, with a DHCPv4 socket pulling a
slirp lease (10.0.2.15/24, gateway 10.0.2.2); plain TCP probe to
10.0.2.2:12345 reaching Established; rustls 0.23
UnbufferedClientConnection (the no_std API) completing a TLS 1.3
handshake against a self-signed Python `ssl` listener on
10.0.2.2:12346. Crypto provider: rustls-rustcrypto 0.0.2-alpha
(pure-Rust no_std, RustCrypto primitives). Custom getrandom
backends for both 0.2 and 0.4 (transitively pulled) feeding into
an RDRAND-first / TSC-fallback `fill_bytes`. Smoke now asserts
eight sentinels (added `ARSENAL_TCP_OK`, `ARSENAL_TLS_OK`); the
script generates a self-signed P-256 cert with `openssl req` per
run and stands up two Python listeners (plain TCP + TLS 1.3)
before launching QEMU. The smoke wait-loop refactored from
"FINAL_SENTINEL fires" to "all required present" to absorb future
3E–3G sentinels without per-step rewriting. ELF ~144 KB → ~1.46 MB
(10x — RustCrypto's AES, ChaCha20-Poly1305, P-256, P-384, X25519,
Ed25519, RSA, SHA-2 plus rustls's protocol state machines); ISO
~19.3 MB; smoke still completes in ~1 s locally. The slip-prone
moments were exactly where the HANDOFF predicted: getrandom
backend wiring (two versions, two mechanisms) and discovering
that rustls's no_std API isn't `Connection::read_tls`/`write_tls`
but the `UnbufferedClientConnection` state machine.

3D sub-commits:
- `622a436` add smoltcp 0.12
- `4499dc0` smoltcp phy::Device on virtio-net
- `f8bfa02` smoltcp interface + DHCPv4
- `8c8a599` TCP smoke against slirp + ARSENAL_TCP_OK
- `db4625e` rustls handshake + ARSENAL_TLS_OK

**3E — framebuffer console (complete, 2026-05-13).** Limine
`FramebufferRequest` probe (1280×800×32, pitch 5120, HHDM-mapped
LFB on QEMU q35 std-vga); `fb::clear` + `fb::put_pixel` with
volatile 32-bit RGB writes (NAVY `#0A1A2A` background, AMBER
`#FFB200` foreground per CLAUDE.md §4); 8×16 glyph renderer over
Spleen 8x16 v2.2.0 (vendored under `vendor/spleen/`, BSD-2-Clause
— exact license match against the Arsenal base; rejected an
abandonware VGA BIOS font on provenance and the v0.1 TempleOS-
lineage 8×8 on ADR-0004 grounds); byte-level fan-out from
`serial::write_str` to a cursor-tracking `fb::print_str` with
newline + line-wrap + scroll-by-blit through `core::ptr::copy`.
The mirror gates on a Release/Acquire `FB_READY` AtomicBool and
uses `FB.try_lock()` so a panic mid-render still reaches serial
via the bypass path. No new sentinel — 3E's smoke target is
implicit (kernel continues past fb init / render / mirror without
faulting). ELF ~1.46 MB → ~1.475 MB (+~13 KB across four
sub-steps); smoke still completes in ~1 s with eight sentinels.
Current kernel boot is ~30 lines × 16 px ≈ 480 px tall, inside
one 800 px screen — the scroll-by-blit path is code-review-
correct but dead-untested by the headless CI smoke; 3F's
preemption banner or 3G's prompt will exercise it.

3E sub-commits:
- `b604f87` probe Limine framebuffer
- `6d9a2a3` framebuffer clear + put_pixel
- `fc5803f` 8x16 framebuffer console (Spleen 8x16 vendored)
- `8aad04d` mirror serial to framebuffer console

**3F — LAPIC + soft preemption (complete, 2026-05-13).** Mask
both 8259A PICs (write 0xFF to ports 0x21 / 0xA1) so legacy IRQ
lines stop competing for vectors. Read `IA32_APIC_BASE`, assert
LAPIC_ENABLE + BSP + ¬X2APIC_ENABLE, map the LAPIC MMIO page
through `paging::map_mmio` (Limine's HHDM covers RAM only —
same dance as 3C). Software-enable via SVR bit 8 with spurious
vector 0xFF; periodic timer vector 0xEF wired through the
existing `idt::IDT` Lazy initializer. Calibrate the LAPIC timer
against PIT channel 2 (mode 0 / interrupt on terminal count,
polled via port 0x61 bit 5) for a 10 ms window; QEMU TCG -cpu
max reports ~624 k LAPIC ticks per 10 ms through /16, scaled to
the 100 Hz arming target. Soft preemption only — the timer
handler increments `TICKS` and writes EOI; no IRQ-driven
context switch (hard preemption is deferred to M0 step 4 when
SMP forces the design). `sched::idle_loop` regains `sti` + `hlt`;
cooperative `switch_to` doesn't save or restore rflags, so IF=1
propagates from idle's first switch-in to every subsequent task.
Smoke now asserts nine sentinels (added `ARSENAL_TIMER_OK`,
emitted from a cooperative-context probe in `idle_loop` once
`apic::ticks()` crosses 10). ELF ~1.475 MB → ~1.479 MB, smoke
still ~1 s locally with calibration line "apic: calibrated
624375 LAPIC ticks per 10 ms; armed periodic 100 Hz vector=0xef
initial_count=624375". From 3F-3 forward, the kernel can be
interrupted at any instruction boundary in cooperative code.

3F sub-commits:
- `7dd1dfd` mask 8259 + map LAPIC MMIO
- `896183e` LAPIC software enable + spurious vector
- `41e7f8d` bump kernel task stack 16 KiB → 32 KiB (latent fix
  surfaced by 3F-2's binary-layout shifts; the rustls / smoltcp
  poll-loop callchain overflowed the 16 KiB task stack and
  corrupted adjacent heap allocations, surfacing minutes later
  as a `LayoutError` unwrap in `linked_list_allocator`'s
  dealloc path — flag for M0 step 3 retrospective)
- `6c4b169` LAPIC periodic timer + PIT-calibrated 100 Hz tick
- `0323497` idle hlt + sti + ARSENAL_TIMER_OK

**3G — `>` prompt + M0 step 3 perf gate (next).** Land an
interactive `>` prompt over the serial + framebuffer stack
(PS/2 or virtio-keyboard driver, line-editing buffer, command
dispatch shell, hardware-summary command for the usability gate),
then assert the < 2 s boot-to-prompt budget in CI. 3G closes M0
step 3; step 4 (SMP — IPI bring-up, AP startup, per-CPU LAPIC
state, hard preemption discipline) is the next major surface
after that. The bug-prone moments are PS/2 vs virtio-keyboard
choice (PS/2 is universally available under q35 but the i8042
state machine has corners; virtio-keyboard is cleaner but needs
the virtio modern-PCI transport from 3C reused for a different
device class) and the perf-gate CI surface (smoke needs to time
boot deterministically across hosted runners, not just locally).

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
