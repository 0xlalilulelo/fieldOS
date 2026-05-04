# STATUS

> What I am doing right now. Updated whenever the milestone changes,
> a major design decision lands, or a session leaves something
> mid-flight that future-me needs to know about.

## Current milestone

**M1 — Boot to Long Mode** *(in progress)*

Started: 2026-04-30

### M1 deliverables

- [x] Step A — GDT (5 entries + 16-byte TSS); CS reload via
  far return; data selectors and TR loaded
- [x] Step B — IDT with 256 stub handlers; IST stacks for
  #DF/#NMI/#MC; `exception_handler` prints PANIC line on serial
- [x] Step C — Limine framebuffer request; TempleOS/ZealOS 8×8
  bitmap font (Unlicense, vendored from
  `Zeal-Operating-System/ZealOS:src/Kernel/FontStd.ZC`);
  `fb_init`/`fb_putc_at`/`fb_puts`/`fb_clear` against 32-bpp
  BGRA framebuffer
- [ ] Step D — `Field OS: stage 1 reached` + `Hello, Field` on
  framebuffer; QEMU `-d int,cpu_reset` confirms zero exceptions
  taken; full M1 smoke green

### Exit criterion

Cold-boot to `Hello, Field` on the framebuffer (rendered with the
embedded TempleOS 8×8 bitmap font) plus `Field OS: stage 1 reached`
and `FIELD_OS_BOOT_OK` on COM1 within ~100 ms simulated time.
QEMU `-d int,cpu_reset` shows zero exceptions taken; smoke test
remains green.

## Active work

M1-C just landed: Limine framebuffer wired in, the TempleOS/ZealOS
8×8 bitmap font vendored, and a software glyph blitter that draws
white-on-black 8×8 cells against a 32-bpp BGRA framebuffer. New
files:
- `kernel/arch/x86_64/font_8x8.h` / `font_8x8.c` — 256 glyphs
  copied verbatim from ZealOS `src/Kernel/FontStd.ZC` (Unlicense /
  public domain). Lineage record kept per CLAUDE.md hard
  constraint #6.
- `kernel/arch/x86_64/framebuffer.h` / `framebuffer.c` — `fb_init`
  reads `limine_fb_request.response`, captures pointer / pitch /
  dimensions, clears to black, parks the cursor at (0, 0).
  `fb_putc_at`, `fb_puts`, `fb_clear` for the rendering surface.

`kernel/main.c` adds the Limine framebuffer request to the
`.limine_requests` section and calls `fb_init()` followed by
`fb_puts("Hello, Field\n")` between `idt_init` and the sentinel.

Verification: serial smoke green; QEMU screendump via the monitor
TCP backdoor shows the expected glyph shapes for `H e l l o , F
i e l d` in the top-left 8×96 pixel region of the 1280×800
framebuffer. No bit-order, color, or coordinate bugs.

Next (after the visual eyeball check passes): M1-D — switch the
serial stage indicator to `Field OS: stage 1 reached`, run with
`-d int,cpu_reset` to confirm zero exceptions taken across the
entire boot path, full M1 wrap.

## Last completed milestone

**M0 — Tooling and Bootstrap** (2026-04-29 → 2026-04-30, six
commits, ~190 LOC of base-system C, 21,000 LOC vendored at
arm's length under `vendor/limine/` and `holyc/`). Tag:
`M0-complete` on commit `60e1a48`.
