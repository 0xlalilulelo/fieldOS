# Field OS — Phase 0 Engineering Plan & Hardware Compatibility Matrix

*Single-builder edition · QEMU-bootable PoC · x86_64*

---

## Preamble

This document is the working blueprint for the first six to eighteen months of Field OS — a serious, modern, single-language desktop operating system inspired by the technical primitives of TempleOS (HolyC as the universal language, the Brief executable-document format derived from DolDoc, the shell-is-the-compiler REPL, the source-as-documentation `#help_index` model, the line-count discipline) and the visual language of macOS Big Sur (translucent vibrancy, inline toolbars, 8/12/20 px corner radii, 4 px spacing grid, IBM Plex SIL OFL typography). It carries forward none of TempleOS's religious framing or ring‑0/identity-mapped architecture; Field OS is a paged, user/kernel-separated, preemptively scheduled system that happens to feel as immediate and direct as Terry Davis's original.

The two deliverables that follow are written for one person who is comfortable with kernel/OS work and is starting from an empty git repository tomorrow morning. They are deliberately conservative about timelines, deliberately small about scope, and deliberately specific about commands. The naming is MGS3-warm and tactical throughout: Patrol, Stage, Channel, Cardboard Box, Cache, Codec, Radar, CQC, Frequencies, Cure, Survival Kit, Stamina, Stockpile, Comm Tower, Wavelength, Foundry, Engine, Listening Post, Calling Card, Operator, Armory, Manual, Recon, Briefing, Field Manual, Field Symbols.

---

# DELIVERABLE 1 — PHASE 0 ENGINEERING PLAN

## 1. Phase 0 Scope and Explicit Non‑Goals

### 1.1 What the PoC must show
Phase 0 ends with a single artifact: `field-os-poc.iso`, runnable as

```
qemu-system-x86_64 \
    -cdrom field-os-poc.iso \
    -m 1G -smp 2 -enable-kvm \
    -machine q35 -cpu host \
    -device VGA,vgamem_mb=32 \
    -serial stdio \
    -usb -device usb-tablet
```

…that, within ninety seconds and without narration beyond on-screen captions, demonstrates three things to a stranger:

1. **HolyC is alive on bare metal.** The Operator window opens; the user types `1+2*3;` and `Print("Hello, Field\n");` and a HolyC compiler running inside the kernel JIT-compiles and executes both, printing `7` and `Hello, Field` to a Brief block.
2. **Brief is alive.** A second window opens showing a `Hello.BR` document. The document contains formatted text, a colored hyperlink, an embedded sprite, and an embedded `[Run]` macro. Clicking the macro re-evaluates a HolyC expression and the document re-renders inline — without restarting anything.
3. **The skeleton of a real OS is there.** Cursor moves smoothly under a mouse, windows can be dragged, a Stage compositor draws translucent material over a desktop wallpaper, a Cache file-manager window lists a ramfs and double-click opens a Brief in Manual.

### 1.2 What is explicitly not in Phase 0
- No GPU acceleration. Software framebuffer only. Foundry (Vulkan-class) and Engine (compute) are Phase 1+.
- No networking. Comm Tower is Phase 2.
- No audio. Wavelength is Phase 2.
- No real filesystem. A ramfs and a tiny read-only RedSea-compatible image are sufficient. ext2/exFAT/NTFS are Phase 1+.
- No user accounts, no login, no Calling Card. Boot drops directly to a single-user Operator session on TTY 1.
- No package manager. Stockpile is Phase 2.
- No SMP scheduling. The PoC pins everything to BSP; APs are parked in `hlt` after init. SMP is Phase 1.
- No real hardware. The PoC is QEMU-only. First real-hardware boot is the headline event of Phase 1.
- No third-party app porting. The first ports happen in Phase 2.

### 1.3 Definition of done
A 90-second screen-capture video, captioned, showing items 1–3 above, posted to the project README, Hacker News, lobste.rs, and r/osdev, with a `git tag v0.0.1-poc` and a reproducible `make iso` from a clean checkout on a stock Debian 12 / Ubuntu 24.04 / Fedora 41 host.

### 1.4 Calibration: how long this actually takes
The single most important honesty-check in this plan is the timeline. A solo builder writing a non-trivial kernel from scratch should expect, at the comparable-project benchmarks below, the following bands:

| Project | Solo or core team | First commit → first windowed app | Notes |
|---|---|---|---|
| Linux 0.01 (Linus Torvalds) | solo, full-time student | ~5 months (Apr 1991 → 17 Sep 1991) | No GUI; serial console + bash + gcc port |
| SerenityOS year 1 (Andreas Kling) | solo, then small community, evenings + post-rehab full days | ~6 months from empty repo (Oct 2018) to recognizable windowed system; 3 years to "daily driver"; quit day job at 2.5 years (May 2021) | Single biggest comparable; he had the C++/WebKit background |
| Sortix 1.0 (Jonas Termansen) | solo, evenings + GSoC | 5 years (Feb 2011 → Mar 2016) to self-hosting; no GUI in 1.0 | Conservative target; no GUI |
| Redox OS year 1 (Jeremy Soller) | solo evenings → small team | ~12 months to windowed compositor (Orbital) | Rust, larger ecosystem leverage |
| Asahi Linux (Hector Martin & team) | small team, sponsored full-time | ~12 months from kickoff to alpha installer (Dec 2020 → Mar 2022) | Reverse-engineering a closed platform; not a from-scratch kernel |
| TinkerOS / ZealOS | small community forks of TempleOS | ~ months for Limine boot; years for VBE/AHCI/network | The most relevant prior art for HolyC modernization |

The honest read across these projects is: **a solo builder, kernel-comfortable, working ~20 evening/weekend hours per week, should plan for 12–18 months to PoC; full-time, 4–6 months is plausible but tight.** The SerenityOS clock is the most accurate analogue, and Andreas Kling has been candid that the first 6 months produced the vertical slice and the next 30 looked like compounding on it.

Suggested calendar bands, rounded:

| Mode | M0–M2 (boot+memory) | M3 (HolyC bare metal) | M4–M6 (sched+I/O+Stage) | M7–M10 (Brief+Operator+Cache+pkg) | Total to PoC |
|---|---|---|---|---|---|
| Full-time (35 h/wk) | 4–6 wk | 4–8 wk | 6–10 wk | 4–6 wk | **~5–7 months** |
| Part-time (15 h/wk) | 10–14 wk | 10–18 wk | 14–22 wk | 10–14 wk | **~12–18 months** |

These are *p50* estimates. Add 30 % for HolyC-bootstrap risk (see §6) and another 20 % for the first-time-doing-this tax on any milestone you have not previously shipped. **Do not promise 18 months for what realistically takes 3 years part-time.**

---

## 2. Milestone Breakdown

Each milestone is named, scoped, given exit criteria, dependencies, and a full-time-week effort estimate (multiply by ~2.3× for part-time at 15 h/wk).

### M0 — Tooling and Bootstrap *(2–3 FT-weeks)*

**Scope.** Get a host environment, a cross-compiler, a HolyC bootstrap path, and a CI loop in place before writing any kernel code. The temptation to "just start hacking on the boot stub" is the single biggest cause of solo-OS abandonment; M0 is the inoculation against it.

**Toolchain.** Build an `x86_64-elf` cross-compiler (GCC 14 + binutils 2.42 or LLVM 18). The OSDev wiki "GCC Cross-Compiler" page is the canonical recipe; expect ~30 min on a modern laptop. Pin the version in `tools/toolchain.mk`.

```
# tools/build-toolchain.sh
PREFIX="$HOME/.local/x86_64-elf"
TARGET=x86_64-elf
mkdir -p $PREFIX/src && cd $PREFIX/src
curl -O https://ftp.gnu.org/gnu/binutils/binutils-2.42.tar.xz
curl -O https://ftp.gnu.org/gnu/gcc/gcc-14.2.0/gcc-14.2.0.tar.xz
# … unpack, build binutils with --target=x86_64-elf --with-sysroot --disable-nls --disable-werror
# … build gcc with --target=x86_64-elf --enable-languages=c --without-headers
```

**Build system.** Plain GNU Make for Phase 0. Meson is tempting but introduces cross-compilation friction; CMake is overkill. A single top-level `Makefile` plus per-component `*.mk` files is enough. Switch to Meson only if/when cross-compiling to aarch64 in Phase 5 forces the issue.

**HolyC bootstrap strategy.** This is the highest-stakes decision in the whole plan. The realistic options:

| Option | Effort | Risk | Long-term cost |
|---|---|---|---|
| (a) Port/modernize ZealOS or TinkerOS HolyC compiler (`Compiler.HC`) to host-on-Linux as a transpiler | Medium (~3–5 wk) | Low — the code exists, has a Limine prekernel, and is public-domain | High initially (HolyC code), low later (already self-hosting target) |
| (b) Write a HolyC→C transpiler in Python or Rust as the bootstrap; later self-host | Medium (~4–6 wk) | Lowest — Python/Rust development is fast and debuggable | Medium — you'll throw it away when self-hosting, but it's a great fallback |
| (c) Fork holyc-lang (Jamesbarford) — already a working AOT HolyC compiler emitting x86_64, written in C, MIT-style usable | Low (~1–3 wk to fork & graft a freestanding backend) | Low–medium — backend currently emits assembly fed through `gcc` for ELF; you'll need to teach it to emit a static `.o` against your bare-metal ABI | Medium — C codebase, you'll want to rewrite in HolyC eventually |

**Recommendation: (c) primary, (b) fallback.** Fork `holyc-lang` (the Jamesbarford implementation; ~857★ on GitHub, active in 2026), strip the libc dependency, swap in a freestanding runtime, and use it both as the cross-compiler on the host *and* — once the kernel is up — as the in-kernel compiler. Keep a parallel `holyc2c` Python transpiler as a Plan B; if the holyc-lang graft stalls, you can ship the PoC by emitting C, compiling with the cross-GCC, and linking. This dual-path approach is the single most important risk hedge in the project (see Risk Register §6).

You will *not* port Terry Davis's original `Compiler.HC` directly. It is JIT-only, ring-0-only, and assumes the TempleOS runtime; the TinkerOS/ZealOS forks have already paid most of the modernization cost and inform the work but they do not solve hosting it as a Linux cross-compiler.

**Repository layout.**
```
field-os/
├── README.md
├── LICENSE              (BSD-2-Clause)
├── CHANGELOG.md
├── Makefile
├── tools/               # toolchain build scripts, ISO build, qemu launchers
├── boot/                # Limine config, EFI boot images
├── kernel/              # bootstrap C kernel
│   ├── arch/x86_64/
│   ├── mm/              # PMM, VMM, heap
│   ├── sched/           # Patrol-stub
│   ├── drivers/         # PS/2, framebuffer, serial, PIT/APIC
│   └── holyc/           # the HolyC runtime trampoline
├── holyc/               # forked holyc-lang, modified for freestanding
├── base/                # HolyC source for runtime, Stage, Brief, Operator, Cache
│   ├── stage/
│   ├── brief/
│   ├── operator/
│   ├── cache/
│   ├── manual/
│   └── lib/             # HolyC stdlib equivalents
├── assets/              # IBM Plex, Cozette PSF, Field Symbols icons
├── docs/                # Field Manual sources
└── ci/                  # GitHub Actions workflows, smoke tests
```

**License.** **BSD-2-Clause.** Reasoning: (i) Limine itself is BSD-2; matching upstream license simplifies vendoring; (ii) MIT/BSD removes friction for hardware-vendor driver contributions later — vendors will not ship GPL kernel modules of their own accord, but they will tolerate MIT/BSD shims; (iii) GPL on a niche solo-built OS deters the very contributors (porters, driver authors, chip vendors) you most need; (iv) TempleOS is public-domain, ZealOS is Unlicense, and the PoC must be willing to vendor and adapt their code — neither can flow into a GPL repository cleanly. MIT and BSD-2 are functionally identical for this purpose; BSD-2 is chosen for symmetry with Limine, FreeBSD, and the MGS3-warm aesthetic of brevity.

**CI.** GitHub Actions, two jobs:

```yaml
# .github/workflows/ci.yml (sketch)
jobs:
  build-iso:
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@v4
      - run: sudo apt-get install -y nasm xorriso mtools qemu-system-x86 build-essential
      - run: ./tools/fetch-toolchain.sh   # caches the prebuilt cross-gcc
      - run: make iso
      - uses: actions/upload-artifact@v4
        with: { name: field-os-poc, path: field-os-poc.iso }
  smoke:
    needs: build-iso
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/download-artifact@v4
        with: { name: field-os-poc }
      - run: ./ci/qemu-smoke.sh field-os-poc.iso  # boots, asserts on serial output
```

The smoke script uses `-serial file:serial.log -display none -no-reboot -no-shutdown -d guest_errors`, runs for 30 s, then `grep -q "FIELD_OS_BOOT_OK"` against `serial.log`. If the kernel can't print that string within 30 s, the build fails red.

**Exit criteria.** `make iso && ./tools/qemu-run.sh` produces a Limine boot menu and prints "Field OS: stage 0 reached" over serial. Empty kernel, but the entire chain compiles.

---

### M1 — Boot to Long Mode *(1–2 FT-weeks)*

**Scope.** Limine hands off to the kernel in 64-bit long mode, higher half mapped at `0xFFFFFFFF80000000`, with framebuffer info, a memory map, and a serial console.

**Bootloader choice.** **Limine v9.x or later.** Rationale, point by point:
- BSD-2 licensed (matches Field OS).
- Supports both BIOS and UEFI from a single ISO using an `xorriso -as mkisofs` hybrid recipe; this is critical when the project moves from QEMU to real hardware in Phase 1.
- The Limine boot protocol delivers the kernel already in long mode, identity-mapped + higher-half-direct-mapped (HHDM), with a clean memory map distinguishing usable / bootloader-reclaimable / kernel-and-modules / framebuffer regions — eliminating ~500 lines of GDT/long-mode-trampoline assembly that GRUB+Multiboot2 would force you to write.
- Active prior art: ZealOS already uses Limine via the BSD-2 ZealBooter prekernel; TinkerOS, SkiftOS, Hyperion, and the OSDev wiki "Limine Bare Bones" template all use it.
- Excellent documentation: the OSDev wiki "Limine Bare Bones" page and the upstream `PROTOCOL.md` are everything you need.

GRUB is rejected because (i) GPL, (ii) `grub-mkrescue` is a notorious dependency-management tarpit on macOS hosts, and (iii) Multiboot2 leaves the CPU in protected mode with stale segmentation, making 32→64 trampoline assembly the first thing you'd write — wasted time. Writing your own multiboot stub is rejected because the value is zero and the time is two months.

**Subgoals.**
1. `boot/limine.conf` minimal config:
   ```
   timeout: 1
   /Field OS
       protocol: limine
       path: boot():/boot/field-kernel
       kaslr: no
   ```
2. Kernel embeds `LIMINE_BASE_REVISION(3)` and a request triple for framebuffer, memory map, and HHDM offset, all in `.requests`.
3. Linker script places kernel at `0xFFFFFFFF80000000` (the Limine protocol requires kernels at or above this address).
4. GDT (5 entries: null, kernel code, kernel data, user code, user data + TSS) installed in the first ~50 lines of kernel C.
5. IDT with 256 stub handlers, exception entries 0–31 routed to `panic_exception(vector, error_code, regs)`. Use IST stacks for `#DF` (vector 8), `#NMI` (vector 2), and `#MC` (vector 18) so you can crash safely.
6. Serial console at COM1 (`0x3F8`) — first thing initialized, *before* anything else, because everything subsequent depends on being able to `Println` from any state.
7. Framebuffer-backed `Println` — fall back to bitmap font on the framebuffer once the PSF font is loaded.
8. Single-line "Hello, Field" printed to both serial and framebuffer.

**Exit criteria.** Cold-boot to `Hello, Field` over serial in <100 ms simulated time; QEMU `-d int,cpu_reset` shows zero exceptions taken; smoke test green.

---

### M2 — Memory Management *(2–3 FT-weeks)*

**Scope.** A working physical memory manager, virtual memory manager, and kernel heap. This is where most hobby OS projects either get it right and accelerate, or get it wrong and bog down for months.

**Subgoals.**
1. **PMM (bitmap allocator).** One bit per 4 KiB frame. Walk the Limine memory map at boot, mark `USABLE` regions free, everything else used. Two-finger search for free frames is fine for Phase 0; add a buddy allocator only if profiling demands it.
2. **VMM (page-table walker).** Implement `vmm_map(pml4, virt, phys, flags)`, `vmm_unmap`, `vmm_translate`. PML4 + 4-level walk; avoid recursive mapping (it's clever but it bites later under SMP). Use the Limine HHDM to access page-table frames from the kernel.
3. **Higher-half kernel.** Already mapped by Limine at `0xFFFFFFFF80000000`. Keep kernel global mappings (PG_GLOBAL) so they survive CR3 reloads.
4. **Per-process address spaces.** Each user process gets a fresh PML4 with the upper-half kernel mappings cloned from a master PML4. Lower half is process-private.
5. **Kernel heap.** Slab allocator with per-size caches at 16, 32, 64, 128, 256, 512, 1024, 2048 bytes; fall through to a buddy/large-page allocator for >2 KiB. Wire `palloc()` and `free()` HolyC entry points to it.

**Why not TempleOS's identity-mapped 1:1 model?** TempleOS deliberately put everything in ring 0, identity-mapped, with no protection. That made the JIT immediate and the system trivially debuggable, but it also made it impossible to (a) sandbox a Brief macro from clobbering the kernel, (b) share the system between users, (c) survive a hostile USB stick. Field OS is willing to give up the *purity* of identity mapping to keep the *feeling* of immediacy, and it pays for that with paged virtual memory — but the felt directness is preserved by:

- A higher-half kernel that's always mapped, so any kernel pointer is dereferenceable from any context.
- An HHDM region (mirror of all physical memory at a fixed offset), so `phys2hhdm(p)` gives a usable virtual address with a single add — no map/unmap dance for kernel-internal code.
- The HolyC JIT lives in kernel address space (ring 0), so top-level expressions are still a function call away, not an IPC away.

**Exit criteria.** Allocate 10 000 random-sized objects, free them in random order, no leaks. Map and unmap 1 GiB of test pages; `vmm_translate` round-trips. Smoke test boots through and prints free-RAM count.

---

### M3 — HolyC Runtime on Bare Metal *(3–6 FT-weeks; the Big Risk)*

**Scope.** A HolyC compiler running inside the kernel that can accept source code over the serial console, compile it to native x86_64 in memory, execute it, and print its result. The tiniest possible REPL.

**Architectural decision: C bootstrap → HolyC init.** Boot a minimal C kernel (M0–M2) that, once memory and serial are up, calls `holyc_init()` and then `holyc_repl()`. **Do not boot directly into HolyC.** Reasons:
1. GDB-friendliness. The C kernel has DWARF symbols; the HolyC compiler does not (yet). Crashing in the bootloader hand-off and not knowing whether the bug is in your HolyC parser or your IDT install is a debugging hell that will kill morale.
2. Incremental verification. The C kernel can be exercised at every stage (M0, M1, M2) before HolyC enters the picture.
3. Recovery. If HolyC bringup stalls for two weeks, the C kernel is still bootable and shippable.
4. ABI clarity. Forces an explicit, documented small ABI surface from C → HolyC, which becomes the kernel-runtime contract everything else builds on.

**The export ABI** (kept under ~20 functions for Phase 0):
```
// kernel/holyc/abi.h
void  k_serial_putc(char c);
void  k_fb_blit(int x, int y, int w, int h, const u32 *pixels);
u64   k_palloc(u64 bytes);
void  k_pfree(void *p);
void  k_irq_register(int vec, void (*handler)(struct regs*));
u64   k_time_ns(void);
int   k_kbd_poll(struct kbd_event *out);
int   k_mouse_poll(struct mouse_event *out);
void  k_sched_yield(void);
void  k_panic(const char *msg);
// Brief renderer needs:
const u8 *k_font_lookup(u32 codepoint, int *w, int *h);
// Patrol scheduler stubs, exposed once M4 lands
```

**Trampolining strategy.** The C kernel reserves a 16 MiB executable region for JIT output (`PG_RW | PG_NX` cleared on demand using a small `holyc_jit_alloc()` helper that does a `vmm_remap` to flip NX off only for emitted pages — never a global W^X violation). The forked holyc-lang backend is modified to:
- Emit position-independent x86_64 code into a buffer rather than to a file.
- Resolve external symbols against a small static table that lists the ABI functions above.
- Honor the SysV AMD64 calling convention (already does).

**Smallest possible REPL.**
```
field> 1+2*3;
7
field> Print("Hello, %s\n", "Field");
Hello, Field
field> U0 Greet(U8 *name) { Print("Salute, %s.\n", name); }
field> Greet("Naked Snake");
Salute, Naked Snake.
field> :load /res/Hello.BR
[Hello.BR loaded into Manual window 1]
```

The colon-prefixed commands are kernel meta-commands handled in C; everything else flows to the HolyC compiler.

**Exit criteria.** Five-line REPL session over serial: arithmetic, function definition, function call, variable mutation, and a deliberate parse error that doesn't crash the kernel.

---

### M4 — Preemptive Scheduler Stub (Patrol v0) *(2–3 FT-weeks)*

**Scope.** Cooperative multitasking first, then preemptive on a 1 kHz timer tick. Kernel threads, then a single user-space stub process.

**Subgoals.**
1. **LAPIC timer setup.** Use the LAPIC timer in periodic mode at 1 kHz. Calibrate via the PIT or HPET (PIT is universally present on QEMU and on every commodity x86_64 machine; HPET is nicer but optional for Phase 0).
2. **Thread struct.** Per-thread: `regs`, `kernel_stack`, `state` (`READY|RUNNING|BLOCKED|ZOMBIE`), `time_slice`, `prio`, intrusive list links.
3. **Cooperative phase.** `sched_yield()` saves callee-saved regs, picks next ready thread, switches stack and IRET-frame. Verify with three kernel threads printing different letters.
4. **Preemptive phase.** Timer ISR calls `sched_tick()`; if `time_slice <= 0`, schedule. Use IRQ stack frame to swap.
5. **User-space stub.** Load a tiny statically-linked HolyC payload (compiled by the in-kernel compiler) into a fresh user-space PML4 with code at `0x400000`, stack at `0x7FFF_FFFF_F000`. Set up a TSS RSP0 for ring-3 → ring-0 transitions. Implement five syscalls: `write`, `exit`, `yield`, `getpid`, `nanosleep`. SYSCALL/SYSRET (not int 0x80) — modern x86_64 only.
6. **The divergence from TempleOS.** TempleOS's cooperative-only model is rejected here, deliberately and explicitly. Field OS commits to preemption from M4 forward. The cost is ~500 lines of additional code and the discipline of locking; the benefit is that a runaway Brief macro cannot wedge the system, which is the precondition for treating Brief as an executable document format that strangers can ship to each other.

**Naming note.** This is **Patrol v0**. The scheduler will eventually be the supervisor / launchd-equivalent (start, stop, supervise, restart services) and the name carries through. In Phase 0, Patrol is just the scheduler.

**Exit criteria.** Three kernel threads + one user-space "hello-from-ring-3" process running concurrently, with timer-driven preemption verifiable by `xchg`-based race tests.

---

### M5 — Input and Output Stack *(2–3 FT-weeks)*

**Scope.** Keyboard, mouse, framebuffer, font rendering — enough for windowed applications.

**Subgoals.**
1. **PS/2 keyboard.** Scancode set 1, US layout for now; a `keymap.h` table translation. Works on every QEMU configuration and on most real laptops via i8042 emulation.
2. **PS/2 mouse.** Three-byte packet protocol; `usb-tablet` emulated by QEMU is a different, friendlier path (absolute coordinates) — support both. On real hardware we'll switch to USB HID (xHCI) in Phase 1.
3. **Linear framebuffer.** Use the Limine-supplied framebuffer info (BGRA 32 bpp, no banking, no VBE shenanigans). Fix at 1280×800 for Phase 0; defer GPU-driven mode-set entirely. On QEMU, request resolution via the `-vga std` device or the `bochs-display` device with `xres`/`yres` properties.
4. **Font rendering.** PSF1/PSF2 loader. Bundle:
   - **Cozette 13** (MIT) as the default UI font.
   - **Terminus 16** (SIL OFL) as the Operator font.
   - **Spleen 12/16** (BSD-2) as a fallback.
   - **IBM 3270 Nerd Font** (BSD-3) for the "tactical terminal" theme.
   - **Cream12** (MIT) for the dense Brief body text.
   - The original **TempleOS 8×8 font** (public domain) as the bundled "Field Standard" theme so that the lineage is honored. PSF conversions exist in the ZealOS `fontconverter` repository.
5. **IBM Plex.** Ship `IBMPlexSans`, `IBMPlexMono`, `IBMPlexSerif` (SIL OFL 1.1) as TTF in `/assets/fonts/` for the post-Phase-0 vector renderer; for Phase 0, only the PSF bitmap fonts are rasterized. Vector text is Phase 1.

**Exit criteria.** Mouse cursor visible on framebuffer; arrow keys move it; a 32×16 rectangle drawn at the cursor follows it at 60 fps in QEMU.

---

### M6 — Stage v0: Proto-Compositor *(3–4 FT-weeks)*

**Scope.** One on-screen window primitive, drawn in software, with a title bar, a body, and basic vibrancy.

**Subgoals.**
1. **Surface model.** A `Surface` is a 32 bpp BGRA buffer with width, height, stride, opacity, z-order, and a damage rectangle. The compositor maintains a back-to-front list of surfaces and a damage region.
2. **Window primitive.** Title bar (28 px), body, 8 px corner radius (matched to Big Sur small-radius scale per WWDC 2020 session 10104), 1 px hairline at `#FFFFFF20` over a translucent body fill at `#1A1A1A80`. Drop shadow: software-rendered Gaussian, 24 px blur, 32 px offset, 60 % black — pre-baked into a sprite stretched 9-slice for performance.
3. **Vibrancy.** Software box blur (3-pass separable, kernel size 16) of the framebuffer underneath the window, tinted with the window's material color. SSE2 intrinsics for the blur loop are mandatory; AVX2 is a nice-to-have. This is the single most expensive software-rendering operation; budget ~3 ms per frame on a Ryzen 5 7640U at 1280×800 — reasonable. Defer GPU shaders to Phase 1 with Foundry.
4. **Window manager rules.** **Floating with snap-to-grid (4 px grid, matching the spacing system).** Tiling is rejected for the PoC: it requires a far richer keyboard/UI layer to be useful, and floating + snap is more visually consistent with the Big Sur aesthetic. Phase 1 may add a tiling mode behind a Frequencies toggle.
5. **Compositor input dispatch.** Mouse move → top-of-stack hit-test → dispatch `pointer_motion(window, x, y)`. Click → focus + dispatch. Drag of title bar → window-move.

**Exit criteria.** Two windows on screen, one in focus, drag-moveable, redrawn at 60 fps with vibrancy enabled and no tearing in QEMU `-display sdl,gl=on`.

---

### M7 — Brief Renderer v0 *(3–4 FT-weeks)*

**Scope.** A document format and viewer that demonstrates the central Field OS thesis: documents are programs, programs are documents.

**Brief format (subset for Phase 0).** Brief is the modernized DolDoc. Inline tags use `$` as the escape character (preserving the DolDoc convention) but extend the tag vocabulary:
```
$FG,1$Hello, $UL,1$Field$UL,0$.$FG,0$
$LK,"See more","FI:Tutorial.BR"$
$IM,"FI:logo.png"$
$MA,"Compute","Print(\"%d\\n\", 21*2);"$
```
- `$FG,n$` foreground color (palette index)
- `$UL,1$ … $UL,0$` underline
- `$LK,"label","FI:path"$` hyperlink to another Brief
- `$IM,"FI:path"$` embedded image
- `$MA,"label","HolyC source"$` macro: a button labeled `label` that, when activated, runs the HolyC source through the in-kernel compiler.

The full Brief vocabulary (sprites, tables, trees, calculator widgets, footnotes, `#help_index` cross-references) is Phase 1; Phase 0 ships these five tags.

**Subgoals.**
1. Parser: single-pass tokenizer that walks the source and emits a flat list of `BriefRun` records (text run | tag | macro). ~500 lines of HolyC.
2. Layout: monospace bitmap text reflow at the surface width; line breaks at word boundaries; hyperlinks underlined and colored.
3. Macro execution: clicking a `[Run]` macro tail-calls `holyc_eval(source)`; output is captured into a sub-`Surface` and reflowed into the document at the macro's anchor position. The document re-renders inline.
4. The "Hello, Field" demo Brief is checked into `/assets/briefs/Hello.BR` and is the seed document the PoC opens.

**Exit criteria.** `Hello.BR` opens in a Manual window. Clicking the `[Run]` macro re-evaluates and updates the document. Clicking a hyperlink to `Tutorial.BR` opens that document in a new Manual window.

---

### M8 — Operator v0 *(2 FT-weeks)*

**Scope.** The shell, rendered in a Stage window, where each command and its output is itself a Brief block — making the shell session a live, scrollable, hyperlinked document.

**Subgoals.**
1. Command line is editable single-line text rendering inside a Stage window.
2. Pressing Enter sends the line to `holyc_eval()`. Captured stdout becomes a new Brief block appended to the session document.
3. Any path-shaped output (`/foo/bar.BR`) is auto-detected and rendered as a `$LK$` hyperlink that opens Cache or Manual on click.
4. F5 is the **hot-patch live-coding** key: edit a function source in Operator, press F5, the new compiled function is patched into the global symbol table and any future call lands in the new code. This preserves the shell-is-the-compiler REPL feel of TempleOS while running in a paged, preemptive system.

**Exit criteria.** Operator session showing arithmetic, function definition, file listing, and a clicked hyperlink that opens Cache.

---

### M9 — Cache v0 *(1 FT-week)*

**Scope.** A trivial file manager. List files in a virtual ramfs. Double-click opens Briefs in Manual, opens HolyC source in Operator, opens images in a tiny image viewer.

**Subgoals.**
1. ramfs populated at boot from a `cpio` archive bundled into the ISO (just like Linux initramfs).
2. Listing pane: icon (Field Symbols Lucide-fork glyph) + name + size + modified time.
3. Single-click selects, double-click opens via mime sniffing on file extension.
4. Drag a file out of Cache into Manual: opens it.

**Exit criteria.** Cache opens. Listing has at least 5 files (a Brief, two HolyC sources, a PNG, a directory). Double-click flows work.

---

### M10 — PoC Packaging and Demo *(1–2 FT-weeks)*

**Scope.** A reproducible build, a demo script, a public release.

**Subgoals.**
1. `make iso` produces `field-os-poc.iso`, byte-identical between two clean builds (deterministic timestamps in cpio, fixed `SOURCE_DATE_EPOCH`).
2. `tools/qemu-run.sh` is a one-liner.
3. The 90-second demo script (timed):
   - 0–5 s: BIOS/UEFI splash, Limine entry auto-selected.
   - 5–10 s: Field OS logo, "loading" with Patrol stage indicators (boot → mm → sched → input → stage → operator).
   - 10–25 s: Operator window opens. Type `1+2*3;` → `7`. Type `Print("Hello, Field\n");` → "Hello, Field".
   - 25–45 s: `:load /res/Hello.BR` → Manual window opens with a formatted Brief, a colored hyperlink, a sprite, and a `[Compute]` macro button.
   - 45–60 s: Click `[Compute]`; document re-renders inline showing `42`. Click hyperlink; second Manual opens.
   - 60–80 s: Open Cache from the dock; double-click `Tutorial.BR`; opens in Manual.
   - 80–90 s: Cut to a screen showing the README, repo URL, `git tag v0.0.1-poc`.
4. README with the embedded video, a short architecture diagram, and the line-count budget tracker (HolyC + assembly LOC, target ≤ 100 000 for the entire base system over its lifetime, with current consumed shown as e.g. `7 412 / 100 000`).
5. CHANGELOG with one entry: `v0.0.1-poc — first bootable proof of concept`.
6. Submission to Hacker News, lobste.rs, and r/osdev with a one-paragraph framing: "Field OS is a from-scratch desktop OS in HolyC with a Big Sur visual identity and an MGS3 vocabulary. This is the first PoC. Honest about timelines: ~12–18 months part-time to here. Phase 1 targets real hardware on Framework 13 AMD."

**Exit criteria.** Public repo, public video, public release tag, three forum posts. PoC is *done*.

---

## 3. Skill-Building / Ramp-Up Reading List

Grouped by the milestone each item supports. This is a kernel-comfortable list — it presumes you already understand C, x86_64 instruction set basics, and what a system call is.

**Boot, long mode, bootloader (M1).**
- OSDev wiki: "Long Mode", "Higher Half Kernel", "Limine Bare Bones". The wiki is the canonical reference; the Limine Bare Bones page is the literal starting point for the kernel skeleton.
- Limine `PROTOCOL.md` (v9.x or later) on the upstream Codeberg/GitHub mirror — the single most important document for M1.
- Phil Opp, *Writing an OS in Rust* (os.phil-opp.com). The first edition's GRUB+long-mode chapters and the second edition's bootloader chapters are gold even if you're writing in C; the conceptual scaffolding maps 1:1.
- The Multiboot2 spec (read once for vocabulary; don't implement against it).
- *AMD64 Architecture Programmer's Manual, Volume 2: System Programming* — chapters on long-mode initialization, paging, and exception architecture. The AMD manual is more readable than Intel's; cross-reference Intel SDM Vol 3 only when AMD is silent.

**Memory management (M2).**
- OSDev wiki: "Memory Management", "Page Tables", "Slab Allocator".
- Phil Opp, *Paging Implementation* and *Heap Allocation* posts.
- Tanenbaum, *Modern Operating Systems*, 4th ed., Ch. 3 — virtual memory, page replacement, segmentation contrast.
- Linux kernel `Documentation/admin-guide/mm/` and the Bootlin "Linux mm subsystem" walkthrough articles.
- Mel Gorman's *Understanding the Linux Virtual Memory Manager* (free PDF) — even though it covers a 2.6-era kernel, the slab-allocator and PMM chapters are timeless.

**HolyC modernization (M0, M3).**
- Terry Davis's `Compiler.HC` in the TempleOS source tree (read; do not vendor).
- TinkerOS source tree (`Compiler.HC`, `KernelA.HH`).
- ZealOS source tree (Limine boot path, ZealC documentation under `/Doc/ZealC.DD.html`).
- `holyc-lang` (Jamesbarford, GitHub) — the primary fork base; read `src/parser.c`, `src/codegen.c` end-to-end before grafting.
- HolyC for Linux ("secularize") (jamesalbert) — useful only as a worked example of HolyC→C transpilation; not production.
- `nrootconauto/HolyCC2` — alternative compiler with caching ideas worth stealing.

**Compositor (M6).**
- Drew DeVault, *The Wayland Book* (online).
- Smithay (Rust Wayland compositor) source — particularly `smithay/src/wayland/compositor`.
- Hyprland source — for window-rule and snap-grid algorithms, even though Field OS isn't Wayland.
- Kristian Høgsberg's original "Wayland and weston" papers / talks for the conceptual model.
- Alec Murphy's TempleOS GUI code in TinkerOS for an honest read of how minimal a windowing system can be.

**Document format (M7).**
- TempleOS `DolDocOverview.HC` and `DolDocDemo.HC`. The DolDoc format is the conceptual ancestor of Brief.
- Pollen (Matthew Butterick) — for "documents that compute" thinking, executable footnotes, and the cross-reference model.
- org-mode internals (`org-element.el`, `ox.el`) — for hyperlink and macro-expansion semantics.
- Notion's block-model post-mortems (engineering blog) and the Roam Research block-graph papers — for inline-references-as-edges thinking.

**Sandboxing / IPC (Phase 1 prep, but read now).**
- Fuchsia documentation site (fuchsia.dev) — the Zircon kernel object/handle model is the cleanest published capability system in a real OS.
- Capsicum papers (Watson et al., USENIX 2010).
- OpenBSD `pledge(2)` and `unveil(2)` papers and man pages — Cardboard Box will lean heavily on this style.
- seL4 papers (Klein et al.) — for the proof-of-isolation reading; aspirational, not blocking.

**Visual design (cross-cutting).**
- Apple WWDC 2020 session 10104, *Adopt the new look of macOS* (developer.apple.com/videos/play/wwdc2020/10104/) — the canonical statement of Big Sur design intent: full-height sidebars, inline toolbars, bigger bolder controls, semitransparent vibrancy, refined corner radii.
- Apple Human Interface Guidelines for macOS — corner radii, materials, focus rings.
- Tidwell, *Designing Interfaces*, 3rd ed.
- Wathan & Schoger, *Refactoring UI*.
- Big Sur teardowns by Sebastiaan de With and Michael Flarup (public posts).

**Hardware / x86 (cross-cutting).**
- *Intel SDM Vol 3* — System Programming Guide (LAPIC, IOAPIC, paging, MTRRs).
- *AMD64 APM Vol 2*.
- The QEMU manual, especially the `-d` debug flags and `-machine q35` notes.
- OSDev wiki hardware lists: "PS/2 Keyboard", "PS/2 Mouse", "PCI", "AHCI", "NVMe", "xHCI", "HPET", "APIC".

**Inspirational case studies (cadence and morale).**
- Andreas Kling's awesomekling.github.io blog and YouTube channel; the "I quit my job to focus on SerenityOS full time" post is the canonical solo-builder transition document. The CoRecursive podcast interview (2022) captures the mental model.
- The SerenityOS 4th-birthday post (`serenityos.org/happy/4th/`) for honest year-by-year progress.
- Linus Torvalds's first Linux 0.01 announcement and the 0.01 source tarball — calibration of how minimal "first release" can be.
- Hector Martin's blog on Apple Silicon reverse-engineering and Asahi Lina's GPU posts on `asahilinux.org/blog/` — read as cautionary tales about how long real-hardware support takes, *and* as inspiration for the engineering culture.
- Redox OS year-1 documentation (redox-os.org/news/).
- Jonas Termansen's `maxsi.org` and the Sortix release notes — calibration for "small, correct, slow" as a viable strategy.

---

## 4. Tooling Recommendations

**Editor.** Helix or Neovim for power users; VS Code with the C/C++, GitLens, and a HolyC syntax extension (write your own — it's 200 lines of TextMate grammar) for everyone else. The Field OS Armory IDE is a Phase 2 deliverable; do not yaks-have it at the cost of M0.

**Debugging.**
```
# Terminal A
qemu-system-x86_64 -s -S -cdrom field-os-poc.iso -serial stdio -m 1G \
    -d int,guest_errors -no-reboot -no-shutdown
# Terminal B
gdb-multiarch -ex "target remote :1234" \
              -ex "symbol-file kernel/build/field-kernel.elf" \
              -ex "break kernel_main" -ex "continue"
```
Bochs's internal debugger is a useful second pair of eyes when QEMU disagrees with itself; use `bochs -q -f bochsrc` for tricky paging bugs.

**Disassembly.** `ndisasm -b 64`, `objdump -d -M intel`, Ghidra for static analysis of the JIT output once it gets interesting.

**Tracing.** `qemu -d in_asm,int,cpu_reset,page,unimp` to a log file. Combine with `-D trace.log -no-reboot` to capture pre-crash state.

**Build.** Make for Phase 0. Meson optional later. CMake only if upstream-vendored library demands it.

**Version control.** Git. Conventional Commits (`feat(stage): floating window snap-to-grid`). Even as a solo dev, **squash-merge PRs** against `main` to keep history bisectable; bisect saves more time than a clean linear log costs.

**Issue tracking.** GitHub issues with milestones M0..M10. Or a single `TODO.md` if that's lighter weight; promote items to issues only when they get a number. Don't over-engineer this.

**CI.** GitHub Actions with the build-iso + qemu-smoke jobs in §M0. Add a nightly job that publishes `field-os-poc-nightly.iso` to GitHub releases for casual testers.

---

## 5. Solo-Builder Cadence and Discipline

**Devlog rhythm.** Monthly long-form devlog post (Asahi-Linux style: ~2000 words, screenshots, code snippets, lessons learned). Bi-weekly short progress note (~300 words, what shipped, what's next). One demo video every four weeks, even if the only delta is "windows now have shadows." This rhythm has been the consistent factor distinguishing finished hobby OSes from abandoned ones over the last decade.

**The "refactor before adding" rule.** Before every new milestone, spend one day rereading the previous milestone's code and refactoring for clarity. This compounds. Andreas Kling has said publicly that ~25 % of his SerenityOS time is refactoring; it is not optional, it is the work.

**Burnout avoidance.**
- Public commitments. Announce M-numbers, miss them publicly when you do, resume publicly. Andreas's pattern: never apologize at length, just resume.
- Asynchronous community. Discord/IRC channel, but answer in batches once a day, not in real-time. The Field OS community is welcome from day 1 but is *not* a full-time job.
- One day a week off the project. Non-negotiable.
- Hard stop if you find yourself debugging the same issue for >4 hours: write down the state, sleep, return.

**When to accept outside contributions in Phase 0.** *Not until M10 is shipped*, except for: documentation fixes, hardware test reports, font/icon contributions, and reviews. Reasoning: solo cohesion of vision and code style is the project's highest asset for the first year; merge-conflict bandwidth is the most expensive form of bandwidth a solo builder has. The SerenityOS pattern was: full solo for the first ~6 months, then progressively opening up. Field OS should follow that pattern.

---

## 6. Risk Register

| # | Risk | Probability | Impact | Mitigation |
|---|---|---|---|---|
| 1 | **HolyC bootstrap is harder than expected.** Forking holyc-lang, stripping libc, and emitting bare-metal-ABI x86_64 is the highest-novelty work in the plan. | High | Critical — without HolyC, there is no Field OS | Maintain Path B: a Python HolyC→C transpiler that produces freestanding C compiled by the cross-GCC. The PoC can ship with this fallback if necessary; the public framing is that "the in-kernel JIT is Phase 1." Add a 30 % buffer to M3. |
| 2 | **GPU driver realism creep.** Tempting to "just add a basic AMDGPU shim." | Medium | High | Hard rule: no real GPU code in Phase 0. Software framebuffer only. Foundry is Phase 1's headline deliverable. |
| 3 | **Audio creep.** Same temptation. | Low | Medium | Defer entirely to Phase 2. The PoC is silent. |
| 4 | **Network creep.** | Low | Medium | Defer entirely to Phase 2. PoC has no networking. Comm Tower is named, not built. |
| 5 | **Filesystem creep.** | Medium | Medium | ramfs + a single read-only RedSea-compatible image. ext2/exFAT/APFS deferred to Phase 1. |
| 6 | **Solo-builder burnout / pace collapse.** Statistically the leading cause of hobby-OS death. | High | Critical | Public cadence (§5), one day off/week, an explicit "M10 then re-evaluate" off-ramp. The plan must be survivable if Phase 0 takes 24 months instead of 18. |
| 7 | **Real-hardware shock at Phase 1.** Suspend/resume, modern standby, ACPI `_PSx` quirks — all the things QEMU doesn't model. | Certain | High (in Phase 1) | Choose a single Tier-1 reference machine (Framework 13 AMD Ryzen 7040) and go deep, Asahi-style. Don't fight breadth. |
| 8 | **Vendor pushback on naming.** Apple/Konami/IBM trademark concerns. | Low | Medium | "Field OS" is generic. Internal names (Patrol, Stage, etc.) are too generic to be trademarkable. IBM Plex is OFL-licensed and Field OS will not use IBM logos. Lucide is MIT and forkable. |
| 9 | **Line-count budget breach.** 100 000 LOC HolyC+asm is a hard constraint. | Medium | High to project identity | Track LOC in CI as a budget; fail builds at 95 % budget. The line-count discipline is part of the spirit Field OS inherits from TempleOS and gives up only with explicit deliberation. |
| 10 | **The "demo looks like a toy" problem.** Phase 0 PoC is 1280×800, bitmap fonts, static wallpaper. | Medium | Medium (PR risk) | Lean into it. Ship the PoC honestly framed as a foundation, not a beauty contest. The Big Sur visual identity comes online with vector text and Foundry in Phase 1. |

---

# DELIVERABLE 2 — HARDWARE COMPATIBILITY MATRIX (TIERED)

The strategic argument first: Field OS is built by one person. Linux has thousands of paid driver engineers, decades of accumulated quirks tables, and a quality-assurance lever — the universe of distros — that Field OS does not have. **Field OS will not promise broad hardware support.** It will choose a small reference set, make those machines feel pristine, and document the rest as best-effort. This is the Asahi Linux strategy applied to commodity x86_64. The alternative — Haiku-style breadth — produces an OS that mostly works on many machines and feels excellent on none.

## 1. Tier Definitions

### Tier 1 — Reference (Apple-quality on this hardware)

Field OS commits to making these machines feel like they were designed for Field OS. Driver work is in-tree. CI runs against virtualized profiles of these machines. Most components have first-party drivers. Bug reports are P0. The scope is small and defensible.

The Tier-1 selection criteria:
- **Open or open-friendly firmware** where possible (coreboot, Dasharo, or a vendor that publishes UEFI sources). Framework's open EC firmware (Zephyr-based, Chromebook-derived) is a major plus; coreboot/Dasharo support on Framework boards is in active community development.
- **Broad Linux driver coverage** to reverse-engineer / port from. Field OS's Phase 1 driver model is "thin HolyC shim around ported C drivers" (see §4); Linux is the source of those C drivers.
- **Accessible chip documentation.** Intel and AMD publish enough datasheets for their integrated graphics, HD-Audio codecs, and chipsets. NVIDIA, Broadcom, and Realtek are notably worse on this axis.
- **Repairability and longevity** — Framework's modular design means a Tier-1 machine you target today will still be a Tier-1 machine in five years.

**Recommended Tier-1 reference list (for Phase 1 launch, 12–24 months out):**

1. **Framework 13 AMD Ryzen 7040 series (primary reference).** AMD Ryzen 5/7 7640U/7840U, Radeon 760M/780M iGPU (RDNA3, well-documented via AMDGPU), AMD/MediaTek RZ616 Wi-Fi 6E (open-firmware), Realtek ALC295 HD-Audio codec, NVMe (M.2 2230). 2880×1920 or 2256×1504 eDP. Limine boots cleanly; Linux 6.9+ supports everything modulo the documented modern-standby quirk. The DIY edition's published Linux compatibility (Fedora 38+, Ubuntu 22.04+) and the upstream BIOS/EC openness make this the canonical Field OS development laptop.
2. **Framework 13 Intel Core Ultra (Series 1 / Meteor Lake).** Intel Core Ultra 5 125H / 7 155H, Iris Xe / Arc iGPU (i915 / Xe driver lineage), Intel AX210 Wi-Fi 6E (iwlwifi), Intel HDA. Provides Intel-side coverage symmetric to the AMD reference.
3. **ThinkPad X1 Carbon Gen 12 (or T14 Gen 5 AMD).** Mature hardware, very large Linux community testing corpus, mostly open hardware. Lenovo's firmware is well-behaved on Linux. Secondary reference.
4. **Desktop reference — generic AMD build.** Ryzen 7 7700, ASRock or ASUS B650 motherboard (any with Realtek ALC1220 or higher audio), Radeon RX 7600 (RDNA3) or Radeon Pro W7500, 32 GiB DDR5-5600, Samsung 990 Pro NVMe 1 TB. Documented as a recipe in `docs/reference-hardware.md` rather than as a specific SKU.

**Why Framework specifically.** Framework publishes its EC firmware as open source (Zephyr-based, Chromebook EC fork), supports Linux as a first-class OS in marketing and engineering, distributes BIOS updates via fwupd/LVFS (so Field OS users will get firmware via a standard flashrom path in Phase 2+), and uses commodity AMD/Intel chipsets with no exotic ASICs. Coreboot and Dasharo have community ports of varying maturity. The repairability and modular mainboard architecture mean a Field OS commitment to Framework 13 today is a commitment that ages well.

### Tier 2 — Best-Effort

Hardware that uses broadly compatible chipsets — Intel/AMD CPU with integrated GPU, Intel iwlwifi or RTW88-class Wi-Fi, common NVMe/SATA/USB, HDA audio, eDP/HDMI/DP — but is not a reference target. Should boot. Should mostly work. Bug reports accepted, fixes welcome but not prioritized. Most non-OEM-locked laptops 2018+ from Lenovo, HP, Dell, and ASUS land here. Most generic AMD/Intel desktops with Radeon or Arc/Iris Xe land here.

### Tier 3 — Community

Pre-2018 hardware. Exotic peripherals (fingerprint readers, OEM-specific function keys, vendor-specific power-management ICs). NVIDIA gaming GPUs above the open-kernel-module threshold (Turing+ via Nouveau/NVK is a maybe; pre-Turing is dead, llvmpipe fallback only). Realtek wireless cards (notoriously poor docs and shifting silicon revisions). Broadcom Wi-Fi (worse docs, often non-free firmware). Best-effort, community drivers welcome, no project commitment.

---

## 2. Component-Class Compatibility Rubric

This is the matrix that determines what tier any *individual* component lands at, regardless of the machine it's in.

| Component class | Tier 1 | Tier 2 | Tier 3 |
|---|---|---|---|
| **CPU** | x86_64 with AVX2 + SSE4.2 mandatory; AVX-512 optional. ARM64 → Phase 5. | — | Pre-AVX2 chips (e.g., Sandy Bridge / Bulldozer); refused. |
| **Boot firmware** | UEFI 2.7+ (Limine native). Secure Boot supported but not enforced; signed boot in Phase 2. | Legacy BIOS (Limine BIOS path works on QEMU and most Framework/ThinkPad firmware in CSM mode). | Coreboot/Dasharo (works in principle, occasional FW-table quirks). |
| **Storage** | NVMe (well-specified, 1.4 spec is small and cleanly implementable). SATA AHCI (oldest and best-documented host controller spec). USB Mass Storage via xHCI (Tier 1 for HID, Tier 2 for storage in Phase 1). | SD/MMC, eMMC. | RAID HBAs, exotic NVMe over Fabrics. |
| **GPU** | Intel UHD/Iris Xe/Arc (i915/Xe lineage, well-documented). AMD RDNA2/RDNA3 (AMDGPU is the reusable code corpus). | NVIDIA Turing+ via NVK/Nouveau (depends on `GSP-RM` open firmware — workable but fragile). | Pre-Turing NVIDIA → llvmpipe fallback only. PowerVR → declined. |
| **Network — Wi-Fi** | Intel iwlwifi (open driver, redistributable firmware). AMD/MediaTek MT7921/MT7922 (RZ616, RZ717 in Framework). | Realtek RTW88/RTW89 (workable; documentation is variable). | Broadcom (firmware quirks, often non-free); pre-iwlwifi Intel. |
| **Network — Bluetooth** | BT 5.0+ via standard HCI. | Audio codecs (LC3, aptX) → Tier 2 due to codec licensing. | Vendor-specific dongles. |
| **Input — KB/mouse** | PS/2 (trivial; QEMU and almost every laptop). USB HID via xHCI. I²C-HID precision touchpads (well-specified by Microsoft Precision Touchpad). | Synaptics/ELAN proprietary protocols. | Apple Force Touch trackpads (require RE; Apple Silicon notwithstanding, the x86 MacBook trackpads are documented worst-case). |
| **Audio** | Intel/AMD HD-Audio codecs (Realtek ALC2xx/ALC8xx/ALC1220 family). USB Audio Class 1/2. | Bluetooth A2DP. SOF (Sound Open Firmware) on Intel laptops. | Proprietary low-latency/pro-audio cards (RME, MOTU). |
| **Camera** | UVC USB webcams (universally documented, in every spec). | — | MIPI-CSI integrated webcams (vendor-specific — this is the single hardest desktop-Linux hardware area today, see Asahi's webcam timeline). |
| **Sensors** | Battery, thermal, fan via ACPI. | Ambient light, accelerometer via IIO. | Lid switches with vendor quirks; Yoga/2-in-1 rotation sensors. |
| **Power management** | ACPI S3 sleep on machines that still expose it. | **Modern Standby / S0ix.** This is the single biggest non-Apple-OS pain point and must be called out: most 2021+ laptops have removed S3 in firmware in favor of S0ix, which requires per-platform tuning of LAPIC, IOAPIC, PCIe ASPM, and PCH power-gating. Asahi Linux's blog explicitly chronicles years of work on the equivalent Apple Silicon problem; Intel's S0ixSelftestTool and the Linux kernel's `pmc_core` debugfs are the tooling to lean on. Field OS's honest position: **Tier 2 for S0ix in Phase 1, Tier 1 only on the specific reference machines we test.** | Hibernate / S4. |
| **Display** | eDP internal panels. HDMI / DisplayPort external (well-specified by VESA). | USB-C alt-mode DP, Thunderbolt 4 docks. | MST hubs, HDR pipelines, VRR. |
| **Thunderbolt / USB4** | — | Tier 2: useful but driver-heavy; the Linux `thunderbolt` driver is workable and portable. | TB device-pairing / authentication beyond default-permissive. |

---

## 3. Reference Hardware Shortlist (Tier 1)

| Machine | CPU | GPU | Wi-Fi / BT | Audio | Storage | Why Tier 1 |
|---|---|---|---|---|---|---|
| **Framework 13 AMD Ryzen 7040 (primary)** | Ryzen 5/7 7640U/7840U (Zen 4 + AVX-512) | Radeon 760M/780M (RDNA3) | MediaTek RZ616 (Wi-Fi 6E) | Realtek ALC295 | NVMe M.2 2230 | Open EC firmware; LVFS-distributed BIOS; Linux-first vendor; modular and repairable; AMDGPU is the most reusable open GPU stack; documented in Phoronix, Arch Wiki, and the GitHub `tlvince/framework-laptop-13-amd-7640u` notes corpus. |
| **Framework 13 Intel Core Ultra** | Core Ultra 5 125H / 7 155H (Meteor Lake) | Intel Arc / Iris Xe | Intel AX210 (Wi-Fi 6E, iwlwifi) | Realtek + Intel HDA / SOF | NVMe M.2 2230 | Same vendor benefits as above; provides Intel-side coverage; SOF is the leading edge of open audio firmware on x86. |
| **ThinkPad X1 Carbon Gen 12 / T14 Gen 5 AMD** | Core Ultra 7 / Ryzen 7 PRO 8x40 | Iris Xe / Radeon 780M | Intel AX211 / RZ616 | HDA | NVMe | Mature, very large Linux test community, Lenovo firmware is well-behaved, used by half of the kernel-developer population. Excellent secondary reference. |
| **Desktop generic AMD reference** | Ryzen 7 7700 | Radeon RX 7600 (RDNA3) discrete + iGPU | (PCIe Wi-Fi card per build; AX210 recommended) | ALC1220 on B650 board | Samsung 990 Pro NVMe 1 TB | Documented as a build recipe rather than a SKU. Provides desktop-form-factor coverage with all-AMD silicon (kernel-and-GPU-driver-friendly). |

---

## 4. Driver Development Priority

### Phase 0 (PoC) — QEMU Only
The PoC explicitly targets QEMU and only QEMU. The drivers are:
- **PS/2 keyboard / mouse** (`-machine q35` and i8042 emulation).
- **BGA / Bochs VBE / `bochs-display`** linear framebuffer at fixed resolution.
- **8250 UART (COM1)** for serial.
- **LAPIC + LAPIC timer + PIT** for time and preemption.
- **PCIe enumeration** (read-only; no real device drivers attached).

Explicitly *not* in PoC drivers: virtio-blk, virtio-net, e1000, AHCI, NVMe, xHCI, HDA, AC97, USB (any class).

### Phase 1 — Real Hardware
Bring-up in roughly this order, on Framework 13 AMD as the primary target:
1. **CPU init** — APIC, SMP startup (parked APs first, then scheduling), TSC calibration via HPET or invariant-TSC, microcode update via BIOS path.
2. **ACPI** — table parsing, AML interpreter (port `acpica` — it's MIT-licensed, the standard reference, and used by every BSD), `_PSx`, `_PIC`, `_OSC`. ACPI is unavoidable on real hardware and writing one from scratch is months — port acpica.
3. **AHCI** — for SATA on the desktop reference and on older laptops. Spec is short and complete.
4. **NVMe** — version 1.4 baseline, single-queue first, then multi-queue per-CPU. Spec is well-written.
5. **xHCI** — for USB. The single most painful Phase 1 driver. Allocate 8–10 weeks; lean heavily on the Haiku and Redox xHCI implementations as references.
6. **USB HID** — for keyboard, mouse, and external touchpads.
7. **Intel HDA** — for audio. Spec is available; codec quirks are handled per-laptop.
8. **AMDGPU shim** — port the kernel-mode parts of Linux's amdgpu via a thin HolyC compatibility shim; for Phase 1, only mode-set + KMS, no Vulkan. Vulkan-class Foundry is Phase 2's headline.
9. **i915/Xe shim** — symmetric.
10. **iwlwifi shim** — for Intel Wi-Fi.
11. **MT7921/MT7922 shim** — for AMD/MediaTek.
12. **Bluetooth HCI** — leveraging the upstream BlueZ stack via a shim.

### The shim model
Field OS commits to a "thin HolyC shim around ported C drivers" approach for Phase 1, with a long-term ambition of in-tree HolyC drivers for the most-used hardware classes (PS/2, simple framebuffer, perhaps NVMe). This mirrors what Asahi Linux does in spirit (writing some new drivers in Rust, leveraging existing ones in C) and what Genode does mechanically (Linux drivers as porting targets via DDE Kit). The shim layer itself is C, lives under `kernel/drivers/`, and exposes a small driver-API surface to HolyC code. The line-count budget of 100 000 HolyC+asm explicitly excludes drivers, in keeping with the TempleOS-inspired discipline that the *base system* is what's bounded.

---

## 5. Realistic Tier-1 Device Shortlist for Phase 1 Launch (12–24 months from PoC)

A small, defensible list. This is what "supported" will mean in the v0.1 release notes:

1. **Framework 13 AMD Ryzen 7040 series** (DIY edition, Ryzen 5 7640U or Ryzen 7 7840U, RZ616 Wi-Fi, 2256×1504 or 2880×1920 panel, 32 or 64 GiB DDR5-5600). The "boots, sleeps, all hardware works" reference.
2. **Framework 13 Intel Core Ultra Series 1**.
3. **ThinkPad X1 Carbon Gen 12** or **T14 Gen 5 AMD** (one only, to keep test surface bounded).
4. The **generic AMD desktop recipe** documented in `docs/reference-hardware.md`.

Four. Not forty. The argument for keeping the list small: every Tier-1 machine is a CI commitment, a regression-test commitment, a release-blocker if it breaks. Asahi Linux ships excellently on M1/M2 because it does not also try to ship excellently on Snapdragon. Field OS will ship excellently on Framework 13 because it does not also try to ship excellently on Dell XPS or Surface Laptop in Phase 1.

---

## 6. What "Works" Means at Each Tier

**Tier 1 acceptance tests** (run as part of Phase 1 CI on hardware-in-the-loop against virtualized profiles, and manually on physical machines for each release):
- Cold boot from power-button to login prompt ≤ 2.5 s.
- Suspend (S0ix or S3) and resume reliable across 24 cycles, no battery anomaly.
- Battery charge-state accurate to ±5 %; runtime within 10 % of the same machine running stock Fedora.
- All internal hardware functional: camera, Wi-Fi, Bluetooth (headset + speakers), audio (speakers + 3.5 mm jack + USB-C audio), trackpad with multi-touch gestures, internal display brightness control, keyboard backlight.
- External display via HDMI, DP, and USB-C alt-mode DP.
- Battery life ≥ 8 hours on light productivity load (the Framework 13 AMD baseline is ~10 h on Linux).
- Suspend power draw ≤ 2 % of battery per night (8 h).

**Tier 2 acceptance tests:**
- Boots to login.
- Wi-Fi works on at least one band.
- Audio works on internal speakers.
- Suspend/resume may be unreliable; battery may be off; some peripherals may not work.
- Users can file a bug; bug is triaged but not necessarily fixed.

**Tier 3:**
- Best-effort. No commitment from project. Community PRs welcome. Issues are tagged `tier-3` and live in their own backlog.

---

## 7. Comparison to Other Small-Team OS Projects' Hardware Strategy

| Project | Strategy | Outcome | Lesson for Field OS |
|---|---|---|---|
| **Asahi Linux** | Chose one platform (Apple Silicon Macs) and went deep. Reverse-engineered the GPU, the DCP, the SMC, the USB-C controllers from scratch over four years. | Genuinely Apple-quality on M1/M2; the only fully-compliant AGX driver in any open ecosystem. | Pick one platform, go deep. Don't try to be Linux-on-everything. |
| **SerenityOS** | QEMU-only for the first ~3 years. Real-laptop bring-up began in earnest only around 2022–2023. | Allowed the userspace, browser, and IDE to mature without driver overhead. | QEMU-only Phase 0 is exactly right. Don't ship to laptops before the userspace is worth running on a laptop. |
| **Haiku** | Long-tail i386 and AMD64 support; many machines partially supported; no flagship. | Boots almost anywhere; feels great almost nowhere. | The cautionary tale. Avoid breadth-without-flagship. |
| **Redox OS** | QEMU + a small list of community-tested machines. | Working, but the Rust-driver compounding has been slower than hoped. | Document your reference set publicly, hold the line. |
| **Pop!_OS / elementary OS** | Ride Linux's driver corpus. | Excellent hardware compatibility, with very modest engineering investment. | **This is the lever Field OS does not have.** The honesty obligation: Field OS is alone with its drivers. Plan accordingly. |
| **System76** | Vertical integration — they ship laptops they engineered alongside the OS. | Excellent hardware support on their hardware. | Long-term, a Framework partnership of some kind is the analogue. Not Phase 0 or 1 work. |

**Synthesis for Field OS.** Choose the small reference set (4 machines), document explicitly, refuse to promise breadth, lean on Linux's driver corpus via the shim model in Phase 1, and earmark in-tree HolyC drivers as a multi-year Phase 3+ ambition for the most-used hardware classes only. This is the only resource-realistic path for one person to build an OS that feels good on the hardware it runs on, instead of one that mostly runs on hardware that mostly feels OK.

---

## Closing Note

This plan is deliberately conservative on timeline and deliberately specific on commands. The Field OS thesis — that a single language, a single document format, a coherent visual identity, and a small reference hardware set can produce a desktop OS that feels designed instead of accreted — is testable in 12 to 18 months part-time, not 18 weeks. The PoC is the gate; ship it, then re-evaluate everything else, including this plan.

The single most important habit between now and the PoC video is the cadence: monthly devlog, bi-weekly progress note, four-week demo video, one day off per week, refactor before adding. Andreas Kling's SerenityOS, Hector Martin's Asahi Linux, and Jonas Termansen's Sortix all converge on the same lesson: the engineering is the easy part. The cadence is the project.

`git init field-os && cd field-os` — and good luck.