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

M1-B just landed: 256-entry IDT, per-vector asm stubs, common
dispatch into a C panic handler, IST stacks for #DF/#NMI/#MC.
New files:
- `kernel/arch/x86_64/idt.h` — `struct regs` layout, decls
- `kernel/arch/x86_64/idt.c` — gate encoding, IST stack
  allocation, `idt_init`, panic-path print helpers
- `kernel/arch/x86_64/exceptions.S` — 256 stubs (`.altmacro` +
  `.rept` over 0..255), uniform `isr_common` dispatcher,
  `isr_table` array consumed by `idt_init`

`gdt.h` / `gdt.c` gained `gdt_set_ist(index, top)` so idt.c
populates the TSS IST slots without the TSS itself being
visible. `kernel/main.c` calls `idt_init()` immediately after
`gdt_init()`.

Verification: smoke is green (sentinel still prints in 2 s) and
the fault path was exercised by inserting a temporary `ud2`
after `idt_init`. Serial captured:
`PANIC: vec=6 err=0x0 rip=0xffffffff8000102b rsp=0xffff80000ff99ff0`.
The `ud2` was removed before the commit landed.

Next: M1-C — Limine framebuffer request, embedded TempleOS 8×8
bitmap font, `fb_init`/`fb_putc`/`fb_puts`. After that, M1-C
needs visual confirmation in a QEMU GUI (per pre-agreed plan).

## Last completed milestone

**M0 — Tooling and Bootstrap** (2026-04-29 → 2026-04-30, six
commits, ~190 LOC of base-system C, 21,000 LOC vendored at
arm's length under `vendor/limine/` and `holyc/`). Tag:
`M0-complete` on commit `60e1a48`.
