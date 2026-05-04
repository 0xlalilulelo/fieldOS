# STATUS

> What I am doing right now. Updated whenever the milestone changes,
> a major design decision lands, or a session leaves something
> mid-flight that future-me needs to know about.

## Current milestone

**M1 — Boot to Long Mode** *(complete)*

2026-04-30 → 2026-05-04, four commits.

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
- [x] Step D — kmain order finalized; serial prints
  `Field OS: stage 1 reached` after all init; `-d int,cpu_reset`
  confirms zero exception/interrupt deliveries from the kernel
  (the only cpu_reset events are firmware-side: initial reset
  and a SeaBIOS SMM excursion, both in real mode)

### Exit criterion

Cold-boot to `Hello, Field` on the framebuffer (rendered with the
embedded TempleOS 8×8 bitmap font) plus `Field OS: stage 1 reached`
and `FIELD_OS_BOOT_OK` on COM1 within ~100 ms simulated time.
QEMU `-d int,cpu_reset` shows zero exceptions taken; smoke test
remains green.

## Active work

M1-D just landed. `kmain` final order: `serial_init` → `gdt_init`
→ `idt_init` → `fb_init` → `fb_puts("Hello, Field\n")` →
`serial_puts("Field OS: stage 1 reached\n")` →
`serial_puts("FIELD_OS_BOOT_OK\n")` → halt. The stage indicator
now lands AFTER all M1 initialisation rather than before, so its
presence on serial certifies that every step succeeded.

**M1 is complete.** Final exit criterion verified:
- `make iso && ci/qemu-smoke.sh` green; serial prints
  `Field OS: stage 1 reached` followed by `FIELD_OS_BOOT_OK`.
- Headless QEMU GUI run shows `Hello, Field` rendered in the
  TempleOS/ZealOS 8×8 font at the top-left of the 1280×800
  framebuffer; visual eyeball confirmed.
- `-d int,cpu_reset` log shows zero exception/interrupt
  deliveries (`v=` lines: 0) from the moment the kernel takes
  control. The two `CPU Reset` events in the log are both
  firmware-side (initial reset and a SeaBIOS SMM excursion,
  both in real mode at sub-1 MiB EIP), not kernel-side faults.

LOC: 1024 / 100,000 (1%). Eight base-system C files plus two
.S sources. Four commits across the milestone.

Next: M2 — physical memory manager, virtual memory manager,
kernel slab heap. Per phase-0.md §M2 this is "where most hobby
OS projects either get it right and accelerate, or get it wrong
and bog down for months." Estimated 2–3 FT-weeks per the plan,
~5 weeks part-time.

## Last completed milestone

**M1 — Boot to Long Mode** (2026-04-30 → 2026-05-04, four
commits, +700 LOC base-system). Boot path: GDT + TSS, IDT with
256 stubs and IST stacks for #DF/#NMI/#MC, software framebuffer
with TempleOS 8×8 font, "Hello, Field" rendered, zero
exceptions taken.

**M0 — Tooling and Bootstrap** (2026-04-29 → 2026-04-30, six
commits, ~190 LOC of base-system C, 21,000 LOC vendored at
arm's length under `vendor/limine/` and `holyc/`). Tag:
`M0-complete` on commit `60e1a48`.
