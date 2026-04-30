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
- [ ] Step B — IDT with 256 stub handlers; IST stacks for
  #DF/#NMI/#MC; common panic handler over serial
- [ ] Step C — Limine framebuffer request; embedded TempleOS
  8×8 bitmap font; `fb_init`/`fb_putc`/`fb_puts`
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

M1-A just landed: x86_64 GDT and TSS scaffolding. New files:
- `kernel/arch/x86_64/gdt.h` — selector constants, `gdt_init` decl
- `kernel/arch/x86_64/gdt.c` — 5-entry GDT + 16-byte TSS descriptor
- `kernel/arch/x86_64/gdt_load.S` — far-return CS reload + TR load

`kernel/main.c` calls `gdt_init()` between the stage-0 line and
the sentinel. `kernel/kernel.mk` learns to compile `.S` sources.
TSS allocated with empty IST entries; M1-B fills them.

Smoke green: serial prints `Field OS: stage 0 reached` then
`FIELD_OS_BOOT_OK` in 2 s. The GDT load is silent — survival of
the segment reload (which would triple-fault on a malformed
descriptor) proves correctness. M1-D will switch the stage line
to `stage 1` after all M1 pieces are wired up.

Next: M1-B — IDT with 256 stub entries pushing vector + synthetic
error code into a common asm dispatch, common C handler
`exception_handler(struct regs *)` that prints panic info on
serial, IST stacks (4 KiB each) for #DF/#NMI/#MC wired into TSS.

## Last completed milestone

**M0 — Tooling and Bootstrap** (2026-04-29 → 2026-04-30, six
commits, ~190 LOC of base-system C, 21,000 LOC vendored at
arm's length under `vendor/limine/` and `holyc/`). Tag:
`M0-complete` on commit `60e1a48`.
