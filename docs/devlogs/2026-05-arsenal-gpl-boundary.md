# M1 step 2 — the GPL boundary, and the header strategy that didn't survive contact

*May 29, 2026. The third of the three-devlog cluster closing M1
step 2. How Arsenal keeps a BSD-2 base and inherited GPLv2 drivers
in one kernel without the licenses bleeding into each other — and
the assumption about Linux headers that the first real driver
falsified by 14×.*

Arsenal's license commitment (CLAUDE.md §3) is specific: the base
— kernel, supervisor, compositor, system apps — is BSD-2-Clause.
Inherited Linux drivers keep their GPLv2. The kernel ships as a
*combined work* with an explicit boundary between the two, the
FreeBSD drm-kmod pattern. M1 step 2 is where that boundary stops
being a sentence in a planning doc and becomes a directory layout,
a build-system check, and — after one falsified assumption — a
clear answer to "what, exactly, is GPL in this kernel."

This is the license story. The shim infrastructure is its own
devlog; the driver that proved it works is a third. This one is
about where the line is drawn and why the first plan for drawing
it was wrong.

## The boundary as a directory, not a comment

[ADR-0005](../adrs/0005-linuxkpi-shim-layout.md) §4 made the
license boundary a directory invariant rather than a per-file SPDX
tag to grep for:

- Everything under `linuxkpi/` is BSD-2-Clause, Arsenal-authored.
- Everything under `vendor/linux-6.12/` keeps its upstream SPDX
  header unchanged — GPL-2.0 for driver source, occasionally Dual
  BSD/GPL or LGPL for protocol headers.
- `arsenal-kernel/` and `xtask/` are BSD-2-Clause.
- `linuxkpi/build.rs` refuses to compile any `.c` outside
  `vendor/linux-6.12*/` (or `linuxkpi/csrc/`), and refuses any
  `.rs` inside `vendor/`. One pattern match per source path,
  before `cc::Build::file(...)`.

The reason for a directory fence over a per-file convention is
audit cost. A reviewer can establish the GPL surface of the whole
kernel by reading `ls vendor/linux-6.12/`; an in-crate or
per-file boundary would require reading the build graph. The
combined-work outcome is the same as drm-kmod's: the linked ELF
contains BSD-2 Rust objects (arsenal-kernel, linuxkpi, and the
permissively-licensed vendored Rust crates) plus GPL-2.0 C objects
(the inherited drivers), with a boundary anyone can verify by
directory name.

That much held. What did not hold was the assumption about which
Linux *headers* would end up on the GPL side of the fence.

## The assumption: vendor ~20 headers verbatim

ADR-0005 §3 committed to "a minimal hand-curated header subset
under `vendor/linux-6.12/`" — roughly 20 headers, driven by
virtio-balloon's top-level includes "plus whatever each of those
transitively pulls (typically a handful of `<asm/...>` and
`<uapi/linux/...>` shims)." Vendored verbatim, SPDX preserved,
expanded per-driver at each step kickoff via a `find-include-graph`
audit. It was a reasonable plan. It rested on the belief that a
simple driver's header closure is small and stable.

To test it, M1-2-5 Part B sub-task 2 ran a recursive vendor-fetch
script against `torvalds/linux` v6.12 — curl balloon's includes,
BFS-recurse with cycle detection, fetch the closure. Three
findings fell out in one session, and together they ended the
verbatim-vendoring plan.

## Finding 1: the closure is qualitatively wrong, not just bigger

virtio-balloon is the simplest inherited driver there is — ~1,200
lines, pure virtio-bus interaction, no scatter-gather DMA, no
hardware quirks. Its header closure reached **281 transitive
headers** before the BFS halted on an unresolvable include, and
281 was not even complete. The ADR-0005 estimate of ~20 was off by
roughly 14×.

The growth is super-linear in driver complexity. If the simplest
driver pulls 281, amdgpu (millions of lines of inherited source)
would pull headers into the thousands, and the cumulative GPL
surface compiled into Arsenal's kernel would be most of
`linux-6.12/include/`. The whole point of the minimal-subset
discipline — keep the GPL surface small and auditable — inverts:
verbatim vendoring of transitive closures makes the GPL surface
*maximal*, one driver at a time.

## Finding 2: some required headers don't exist as files

`include/linux/hash.h` includes `<asm/hash.h>`. That file does not
exist at any upstream URL — not under `arch/x86/include/asm/`, not
under `include/asm-generic/`. Both return 404. Linux's Kbuild
*synthesizes* it at compile time from `generic-y += hash.h` rules
in `arch/x86/include/asm/Kbuild` and `include/asm-generic/Kbuild`,
writing a thin wrapper (or an empty stub for arch-override hooks)
into a `generated/` directory during the build.

Verbatim vendoring cannot capture a file that upstream does not
ship as a file. The discipline ADR-0005 §3 committed to literally
does not apply to this class of header — and the class is not
small or exotic; `hash.h` was just the first the BFS happened to
hit.

## Finding 3: vendoring forces inheriting internals we already replaced

`<linux/mm.h>` references `struct mm_struct`'s layout.
`<linux/spinlock.h>` references arch spinlock ordering primitives.
`<linux/percpu.h>`, `<linux/sched.h>`, `<linux/atomic.h>`,
`<linux/kasan.h>` — each reaches into a kernel internal that
Arsenal *already has its own Rust implementation of* (frames,
heap, paging, scheduler, irq, atomics via the `x86_64` crate).

Vendoring Linux's header view of these means maintaining two
interpretations of the same internals — Linux's and Arsenal's —
and bridging them. But bridging them is exactly what the shim is
*for*. ADR-0005 set out to isolate this duplication into the shim
layer, then §3 quietly re-introduced it by pulling Linux's
internal headers back in. The plan contradicted its own purpose.

## The correction: the headers are the shim

[ADR-0006](../adrs/0006-linuxkpi-headers-are-shim.md) amends
ADR-0005 §3 and §4. The Linux API surface — `<linux/*.h>`,
`<asm/*.h>`, `<asm-generic/*.h>` — is provided by
`linuxkpi/include/`, as **BSD-2-Clause Arsenal-authored
reimplementations of just-enough-Linux-API** for the drivers we
host. The C compile sees `-I linuxkpi/include/` and never
`-I vendor/linux-6.12/include/`. When balloon writes `#include
<linux/slab.h>`, it resolves to `linuxkpi/include/linux/slab.h`,
which declares `kmalloc` with Linux's signature; the body lives in
`linuxkpi/src/slab.rs`.

The decisive observation is that the shim had *already been
working this way since M1-2-1*. `shim_c.h` declared `printk`,
`kmalloc`, `pci_register_driver`, `request_irq`, `container_of`,
`BUG_ON` — none of them pulled from an upstream header. The shim's
directional gravity had always been "the header is the surface."
ADR-0005 §3's vendor-headers clause was an unforced parallel
discipline running alongside the real one, and the 281-file halt
is what made the redundancy impossible to ignore.

This is not a novel position. The FreeBSD drm-kmod project — cited
as Arsenal's reference model in CLAUDE.md §3 and ADR-0004 — does
not vendor Linux headers either. Its
`linuxkpi/.../include/linux/*.h` are BSD-licensed reimplementations
of just-enough-Linux-API, and it has run AMDGPU, i915, and Nouveau
that way across multiple Linux LTS versions for roughly ten years.
The combined-work model (BSD headers, GPL `.c` source) is legally
clean and industrially proven. Arsenal was already doing it; 0006
just stopped pretending it wasn't.

## What stays verbatim, and the narrow UAPI carve-out

Two things keep the verbatim-from-upstream discipline:

- **Inherited `.c` source.** `virtio_balloon.c`, future
  `drivers/gpu/drm/amd/*.c`, future iwlwifi — these ship
  unchanged with their GPL-2.0 SPDX headers, under
  `vendor/linux-6.12/`. The directory license fence (ADR-0005 §4)
  is unchanged for them.

- **A narrow UAPI carve-out for protocol/ABI headers.** Some
  headers are not internal Linux API — they are the device or
  wire-protocol ABI shared with userspace and non-Linux stacks.
  For balloon: `<linux/virtio_balloon.h>` (feature bits, the stats
  struct), `<linux/virtio_ids.h>`, `<linux/virtio_types.h>`. These
  may be vendored verbatim, but only when all three hold: the file
  is BSD/MIT/dual-licensed (not GPL — the virtio UAPI headers
  carry explicit BSD boilerplate precisely so non-Linux stacks can
  use them); the content is pure protocol definition with no
  kernel-internal references; and reimplementing it would mean
  transcribing magic numbers where a typo is a silent correctness
  bug. Each carve-out is vendored in the same commit as the driver
  needing it, with the (license, content, transcription-risk)
  triple named in the commit body.

The carve-out is deliberately narrow. amdgpu will likely add
`<uapi/drm/drm.h>`; iwlwifi `<uapi/linux/nl80211.h>`. Each is an
explicit per-driver decision, never a default. The
281-file vendor tree from sub-task 2 was deleted before commit;
`vendor/linux-6.12/` now holds the inherited `.c` plus three
BSD-licensed UAPI headers and nothing else.

## What this costs

The trade is real and worth stating honestly.

`shim_c.h` grows substantially — from ~250 lines at M1-2-5 Part A
to a projected 1,000-1,500 after balloon, more with amdgpu. The
cbindgen-revisit trigger (ADR-0005 §5) now arrives at step 5
rather than later. And reimplementing a Linux API surface is real
engineering, not transcription: `struct page`, `struct
workqueue_struct`, and their kin have semantic complexity the shim
must capture or stub-with-fail-loud. The HANDOFF's "shim semantics
drift from upstream" failure mode becomes *more* central — shim
correctness is now the shim's own responsibility, not something
inherited from upstream headers. The mitigation is the discipline
from the foundation devlog: every surface gets a self-test in the
same commit, and unused paths panic-on-call rather than returning
plausible-but-wrong values.

The honest version of the marketing sentence gets one clause
longer, too. Not "Arsenal runs upstream Linux drivers" but
"Arsenal compiles upstream Linux driver source against a
BSD-licensed reimplementation of the Linux API." That is exactly
drm-kmod's claim, word for word, and it is the true one.

## What it buys

The closure-explosion problem is solved structurally. A driver
needs the shim surfaces it actually calls — balloon ~30-50 beyond
the foundation, amdgpu maybe 200-400 — bounded by API consumption,
not by transitive closure. The Kbuild-generated-header
impossibility disappears: the shim provides the headers, so there
is no upstream non-file to fail on. The per-driver
`find-include-graph` audit goes away — the shim grows by reading
one driver's compile errors at a time, each "declare what driver X
needs" commit bisect-rich and tied to a concrete surface. And the
GPL surface in the kernel becomes structurally minimal: `ls
vendor/linux-6.12/` shows the inherited `.c` files and a few BSD
UAPI headers, which is the entire GPL contribution. The license
boundary is not just clean; it is small, and stays small as the
driver fleet grows.

## A note on letting a plan be falsified

ADR-0005 was a careful document and §3 was a reasonable bet. It
took the simplest possible driver, run through an honest closure
experiment, to show the bet was wrong by an order of magnitude.
The right response was not to defend the plan — emulating Kbuild's
`generic-y` rules in the fetch script would have kept "verbatim
from upstream" technically true while accepting the 281-file
closure that was the actual problem. The right response was to
notice the shim had been demonstrating the correct pattern all
along and to write down what it was already doing. Per CLAUDE.md's
"when the plan must change" discipline: ADR-0006 records what
changed and why, amends the affected ADR sections, and the cost is
one redirected sub-task, not a rewrite.

## Primary sources

- [ADR-0005 §3-§4: vendoring scope and the license boundary](../adrs/0005-linuxkpi-shim-layout.md)
- [ADR-0006: LinuxKPI headers are the shim, not vendored](../adrs/0006-linuxkpi-headers-are-shim.md)
- [FreeBSD drm-kmod LinuxKPI headers](https://github.com/freebsd/drm-kmod/tree/master/linuxkpi/gplv2/include/linux)
  — the BSD-licensed Linux-API reimplementation pattern
- [Linux 6.12 `arch/x86/include/asm/Kbuild`](https://git.kernel.org/pub/scm/linux/kernel/git/stable/linux.git/tree/arch/x86/include/asm/Kbuild?h=linux-6.12.y)
  — the `generic-y` generation verbatim vendoring cannot capture
- [CLAUDE.md §3](../../CLAUDE.md) — the BSD-2 base / GPLv2-preserved
  combined-work commitment this devlog operationalizes

## Cadence

This is the GPL-boundary entry in the three-devlog M1-2-6 cluster,
alongside the shim-foundation and virtio-balloon writeups. The
license model that started as a sentence in ARSENAL.md is now a
directory fence, a build-system check, and a header strategy that
survived its first real driver — after the first plan for it
didn't. With the step-2 retrospective, that closes M1 step 2.
