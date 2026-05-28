# ADR-0006: LinuxKPI headers are the shim, not vendored from upstream

## Status

Accepted. 2026-05-18. **Amends [ADR-0005](0005-linuxkpi-shim-layout.md)
§ 3 (vendoring scope) and § 4 (license boundary directory).** The
other four sections of ADR-0005 (single workspace member, `cc`-driven
C build, bidirectional FFI via hand-written `shim_c.h`, synchronous
module init) are unchanged.

## Context

ADR-0005 § 3 committed to a "minimal hand-curated header subset under
`vendor/linux-6.12/`" — ~20 headers driven by virtio-balloon's
top-level includes plus "whatever each of those transitively pulls
(typically a handful of `<asm/...>` and `<uapi/linux/...>` shims that
the shim's own header set intercepts)." The vendoring discipline
required every `.h` file copied verbatim from upstream Linux 6.12
LTS with SPDX headers preserved; per-driver expansion at each step
kickoff via a `find-include-graph` audit.

M1-2-5 Part B sub-task 2 ran [`scripts/vendor-balloon.sh`](../../scripts/vendor-balloon.sh)
(committed at `1e6fba9` + `b2dd46f`) against `torvalds/linux` v6.12
to materialize that subset. Three falsifications of § 3's underlying
assumptions surfaced in one session:

### Finding 1: closure size is qualitatively wrong

virtio-balloon — the simplest possible inherited driver, 1223 LOC of
.c source, pure virtio-bus interaction, no DMA scatter-gather, no
hardware-quirk workarounds — pulls **281 transitive headers** before
the BFS halts on an unresolvable include. The closure is not even
complete at 281; the true number is higher. ADR-0005 § 3's "~20"
estimate is off by **~14×**. Growth is super-linear in driver
complexity: amdgpu (~2M LOC inherited) would pull headers into the
thousands; iwlwifi similar; the cumulative GPL surface compiled into
Arsenal's kernel would be most of `linux-6.12/include/`.

### Finding 2: some required headers don't exist in upstream as files

Linux's Kbuild generates per-arch header wrappers at compile time
via `generic-y += X.h` and `mandatory-y += X.h` rules in
`arch/<arch>/include/asm/Kbuild` and `include/asm-generic/Kbuild`.
The first concrete instance the vendor-fetch hit:

```
$ /usr/bin/curl -sS -o /dev/null -w "%{http_code}\n" \
    https://raw.githubusercontent.com/torvalds/linux/v6.12/arch/x86/include/asm/hash.h
404
$ /usr/bin/curl -sS -o /dev/null -w "%{http_code}\n" \
    https://raw.githubusercontent.com/torvalds/linux/v6.12/include/asm-generic/hash.h
404
```

`<asm/hash.h>` — included by `include/linux/hash.h:46` — does not
exist at any upstream URL. Kbuild synthesizes it during compilation
from a `generic-y` rule, writing `arch/x86/include/generated/asm/
hash.h` as a thin `#include <asm-generic/hash.h>`-style wrapper (or
an empty file when the header is purely a stub-arch-can-override
hook). Verbatim vendoring cannot capture build-time-synthesized
files; the discipline ADR-0005 § 3 commits to **literally does not
apply** to this class of header.

The script log at the point of halt: 281 files fetched, 188 in
`include/linux/`, 41 in `arch/x86/include/`, 14 in
`include/uapi/linux/`, 9 in `include/asm-generic/`, plus sched/ and
atomic/ subdirs.

### Finding 3: vendored Linux headers force inheriting implementation details we substitute for

`<linux/mm.h>` references `struct mm_struct`'s memory layout.
`<linux/spinlock.h>` references arch-specific spinlock memory
ordering primitives. `<linux/percpu.h>` references the percpu
allocator's slot layout. `<linux/sched.h>` references Linux's
scheduler classes. `<linux/atomic.h>` references the kernel's
atomic primitives. `<linux/kasan.h>` references KASAN shadow
memory. Each of these has an Arsenal-side Rust substitute already
implemented in `arsenal-kernel/src/` (frames + heap + paging +
sched + irq + atomic via x86_64 crate). Vendoring Linux's header
view means maintaining both Linux's interpretation of these
internals AND our Rust substitutes; the bridge between them is
the LinuxKPI shim, which is the layer ADR-0005 was supposed to
isolate this exact duplication into.

### The reference model already does what this ADR proposes

The FreeBSD drm-kmod project — explicitly cited as Arsenal's
reference model in [CLAUDE.md § 3](../../CLAUDE.md), [ADR-0004
§ "Driver strategy"](0004-arsenal-pivot.md), and ADR-0005 § 1 — does
**not** vendor Linux headers. Their `sys/compat/linuxkpi/common/
include/linux/*.h` files are BSD-2 / FreeBSD-licensed
reimplementations of just-enough-Linux-API for the inherited
drivers they host (AMDGPU, i915, Nouveau, plus many in-tree
out-of-Linux drivers). They have maintained this pattern across
multiple Linux LTS versions for approximately **10 years**; the
combined-work model is legally clean (BSD-headers + GPL-.c-source)
and operationally proven at industrial scale.

Arsenal's current `linuxkpi/include/shim_c.h` (committed at M1-2-1
and extended through M1-2-5 Part A) **already operates this way**:
it declares `printk`, `kmalloc`, `pci_register_driver`,
`request_irq`, `container_of`, `BUG_ON`, etc., without including a
single upstream Linux header. The shim's directional gravity has
always been "shim_c.h is the surface"; ADR-0005 § 3's vendor-headers
commitment was an unforced parallel discipline whose cost the
M1-2-5 Part B data now falsifies.

Primary references consulted:

- [`scripts/vendor-balloon.sh`](../../scripts/vendor-balloon.sh) +
  the script-run logs from M1-2-5 Part B sub-task 2 — the closure
  data that triggered this ADR.
- [FreeBSD drm-kmod `sys/compat/linuxkpi/common/include/linux/`](https://github.com/freebsd/drm-kmod/tree/master/linuxkpi/gplv2/include/linux)
  — the BSD-licensed Linux-API reimplementation pattern this ADR
  adopts.
- [Linux 6.12 `arch/x86/include/asm/Kbuild`](https://git.kernel.org/pub/scm/linux/kernel/git/stable/linux.git/tree/arch/x86/include/asm/Kbuild?h=linux-6.12.y)
  — the `generic-y` / `mandatory-y` build-time header generation
  that verbatim vendoring cannot capture.
- [Linux 6.12 `include/asm-generic/Kbuild`](https://git.kernel.org/pub/scm/linux/kernel/git/stable/linux.git/tree/include/asm-generic/Kbuild?h=linux-6.12.y)
  — the mandatory generic-header list that Kbuild enforces.
- [ADR-0005 § 3 (vendoring scope)](0005-linuxkpi-shim-layout.md)
  and [§ 4 (license boundary)](0005-linuxkpi-shim-layout.md) —
  the two sections this ADR amends.

## Decision

### 1. Linux headers are `linuxkpi/include/`'s job, not vendored verbatim

The Linux API surface (`<linux/*.h>`, `<asm/*.h>`,
`<asm-generic/*.h>`) is provided by `linuxkpi/include/shim_c.h`
and any `linuxkpi/include/linux/*.h` / `linuxkpi/include/asm/*.h`
it organizationally splits into as the surface grows. These are
**BSD-2-Clause Arsenal-authored reimplementations of just-enough-
Linux-API for the inherited drivers we host**. The discipline
`linuxkpi/build.rs` enforces (added at M1-2-5 Part B sub-task 3):
only `-I linuxkpi/include/` is on the C compile's include path;
no `-I vendor/linux-6.12/include/` flag is ever added.

When an inherited driver `.c` writes `#include <linux/slab.h>`,
the compiler resolves it to `linuxkpi/include/linux/slab.h`,
which declares `kmalloc` / `kfree` / `kzalloc` / `krealloc` with
the same C signatures Linux 6.12 LTS uses. The bodies of those
functions live in `linuxkpi/src/slab.rs` (already true today;
this ADR codifies the pattern that's been operating since M1-2-1).

### 2. Verbatim vendoring continues to apply — but only to `.c` source

The verbatim-from-upstream commitment in ADR-0005 § 3 still
holds for inherited Linux `.c` source files:
`vendor/linux-6.12/drivers/virtio/virtio_balloon.c`, future
`vendor/linux-6.12/drivers/gpu/drm/amd/*.c`, future
`vendor/linux-6.12/drivers/net/wireless/intel/iwlwifi/*.c`, etc.
These ship unchanged with their upstream SPDX headers preserved.
The license boundary ADR-0005 § 4 establishes still holds:
`vendor/linux-6.12/` is GPL-2.0 (or whatever the upstream
`.c` ships); `linuxkpi/` (including `linuxkpi/include/`) is
BSD-2-Clause. The build-system enforcement (`build.rs` refuses
`.c` files outside `vendor/linux-6.12*/` or `linuxkpi/csrc/`)
is unchanged.

### 3. Narrow UAPI carve-out for device/protocol headers

Some headers are not internal Linux API — they're the **device
or protocol ABI** shared with userspace tools, guest agents, and
non-Linux implementations. Examples for virtio-balloon:
`<linux/virtio_balloon.h>` (the **BSD-licensed** protocol header
defining `VIRTIO_BALLOON_F_*` feature bits and `struct
virtio_balloon_stat`), `<linux/virtio_ids.h>` (BSD-licensed
device-ID list), `<linux/virtio_types.h>` (BSD-licensed
`__virtio16` / `__virtio32` / `__virtio64` endian-tagged types).

These **may** be vendored verbatim under
`vendor/linux-6.12/include/uapi/linux/*.h` — but only when **all**
of the following hold:

1. The file is BSD/MIT/dual-licensed (NOT GPL-2.0). The
   UAPI virtio headers carry an explicit BSD boilerplate
   precisely because they're meant to be shared with non-Linux
   stacks.
2. The content is purely a protocol/ABI definition — register
   layouts, feature bits, struct definitions matching hardware
   or wire-protocol shape. **No internal kernel implementation
   references** (no `struct mm_struct`, no `spinlock_t`, no
   kernel-only macros).
3. Reimplementing it in `linuxkpi/include/` would mean
   transcribing magic numbers (feature bits, command opcodes,
   etc.) where typos are silent correctness bugs.

Each carved-out header is vendored in the **same commit** as the
inherited driver that needs it, with a one-line justification in
the commit body naming the (license, content, transcription-risk)
triple.

This carve-out is structurally narrow: at M1 step 5, amdgpu will
likely add `<uapi/drm/amdgpu_drm.h>` and `<uapi/drm/drm.h>` (the
DRM userspace protocol). At M1 step 6, iwlwifi will likely add
`<uapi/linux/nl80211.h>` (the wireless netlink protocol). Each is
an explicit decision in its driver's vendoring commit, not a
default.

### 4. Delete the partial 281-file vendor tree from sub-task 2

The script run that surfaced this ADR's findings was halted
before commit (vendor tree was already cleaned to `README.md`
only as of `b2dd46f`'s parent state). This ADR's accepting
commit confirms that state: `vendor/linux-6.12/` contains only
`README.md`, with no `include/`, no `arch/`, no `drivers/`.
The next commit (Part B sub-task 3) re-vendors only
`drivers/virtio/virtio_balloon.c` + the three BSD virtio UAPI
headers per § 3 above.

### 5. `scripts/vendor-balloon.sh` is simplified, not retired

The recursive vendor-fetch script (279 LOC at `b2dd46f`) was
built to drive per-driver header closure. Under this ADR's
discipline, it has no recurring use in that form — the headers
come from `linuxkpi/include/`, not from upstream. The script is
**simplified** to fetch only the named `.c` driver + the named
UAPI carve-out headers (no recursive include closure; no
candidate-resolution; no Kbuild emulation). Approximate
post-simplification size: 60-80 LOC. The recursive form is
preserved in git history (`b2dd46f`) in case a future
"audit-what-the-full-closure-would-be" question arises.

## Alternatives rejected

- **Continue ADR-0005 § 3 + emulate Kbuild `generic-y` /
  `mandatory-y` in the vendor-fetch script.** Script learns to
  fetch `arch/x86/include/asm/Kbuild` + `include/asm-generic/
  Kbuild`, parse the rule lines, synthesize wrapper headers at
  `arch/x86/include/asm/X.h` containing `#include <asm-generic/
  X.h>` (or an empty stub for stub-arch-can-override hooks).
  Faithful to upstream behavior; ~30-50 LOC addition. **Rejected**
  because (a) it accepts the 281-file-per-simplest-driver closure
  that triggered this ADR — amdgpu would still pull thousands;
  (b) it pays complexity tax to keep "verbatim from upstream"
  technically true while the synthesized wrappers are NOT
  verbatim — they're Arsenal's reconstruction of Kbuild's rules;
  (c) it does nothing about Finding 3 (the duplicated
  implementation-detail burden).

- **Vendor verbatim + hand-stub Kbuild-generated headers.** Keep
  the 281 fetched files; hand-write empty stubs for `asm/hash.h`
  and the ~10-20 other Kbuild-generated headers balloon's closure
  needs. **Rejected** as worst-of-both-worlds: full vendored-Linux-
  headers compile-time and audit-surface cost without the benefit
  of a clean abstraction. The hand-stubs are no more "verbatim
  from upstream" than shim reimplementations would be.

- **Pause and write the ADR over multiple sessions.** Considered
  for the calendar slip — the strategic question is real. **Rejected**
  because the trade-off data is already in hand from sub-task 2's
  281-file halt; the ADR draft is the work, not a separate
  "think more" step. The HANDOFF note #1 "step away for a day"
  cue is still available for sub-task 3 (the compile-error loop)
  where the unbudget-able friction actually lives.

- **Three-crate split now (the provisional ADR-0006 reservation
  from ADR-0005 § "Reserved successor ADRs").** Splitting
  `linuxkpi` into `linuxkpi-headers` / `linuxkpi-shim` /
  `linuxkpi-drivers` was provisionally reserved as ADR-0006
  pending amdgpu's surface. **Not in conflict with this ADR**:
  the three-crate split addresses Cargo-workspace organization;
  this ADR addresses what headers exist and where. The provisional
  reservation shifts to **ADR-0007**; cbindgen adoption shifts to
  **ADR-0008**; deferred init via kthread+workqueue shifts to
  **ADR-0009**. (Recorded as a small inline edit to ADR-0005's
  "Reserved successor ADRs" list in this same commit.) *Superseded
  by ADR-0007 (`struct page` thin handle), which took the 0007 slot
  as a first-use decision forced earlier than these three; the
  reservations shift up by one again — three-crate split → ADR-0008,
  cbindgen → ADR-0009, deferred init → ADR-0010. Superseded again
  by ADR-0008 (module-init by symbol name), which resolved
  ADR-0005 § 6's deferred design decision and took the 0008 slot;
  the reservations shift up by one yet again — three-crate split
  → ADR-0009, cbindgen → ADR-0010, deferred init → ADR-0011.*

## Consequences

**Easier:**

- **The closure-explosion problem is structurally solved.**
  Balloon needs maybe 30-50 new shim surfaces beyond what
  M1-2-5 Part A already provides; amdgpu needs maybe 200-400;
  iwlwifi maybe 100-200. Each is bounded by the driver's actual
  API consumption, not by the transitive closure of every
  header reached. The ARSENAL.md month-9-to-month-24 budget for
  M1 step 2 stays viable.

- **The Kbuild-generated-header impossibility goes away.** The
  shim provides headers; there is no upstream file that doesn't
  exist to fail on. Future drivers cannot trip the same
  unresolvable-include wall.

- **License boundary becomes structurally cleaner.**
  `vendor/linux-6.12/` shrinks from "every GPL Linux header
  reachable from any inherited driver's transitive closure" to
  "the inherited `.c` drivers plus a narrow BSD-licensed UAPI
  carve-out." The GPL-licensed surface compiled into the Arsenal
  kernel is **structurally minimal** — a reviewer can `ls`
  `vendor/linux-6.12/` and see exactly the GPL surface in
  scope.

- **No more per-driver `find-include-graph` audit work.** The
  shim grows by reading compile errors of one driver's `.c` at
  a time. Each "extend shim_c.h to declare what driver X needs"
  commit is bisect-rich and tied to a specific driver's surface.

- **The quarterly re-pin checklist** (ADR-0005 § 3, deferred to
  M1 step 6's HANDOFF) becomes lower-stakes — re-pinning a
  handful of inherited `.c` drivers plus a handful of BSD UAPI
  headers is straightforward; the original "re-pin 300+
  transitive headers and re-audit each diff" would have been
  operationally heavy enough to discourage the re-pin.

**Harder:**

- **`shim_c.h` grows substantially.** From ~250 LOC at M1-2-5
  Part A exit to a projected **1000-1500 LOC after balloon
  online**, growing further with amdgpu/iwlwifi. The
  "hand-maintained, no cbindgen" decision from ADR-0005 § 5
  still holds, but the cbindgen-revisit trigger (~1500 lines)
  arrives at M1 step 5 (amdgpu) rather than later.

- **Reimplementing Linux API surfaces is real engineering work,
  not just transcription.** Some types (`struct page`, `struct
  mm_struct`, `struct workqueue_struct`) have semantic complexity
  the shim must capture or stub-with-fail-loud-on-use. The
  HANDOFF M1-2-5 (a) failure-mode ("shim semantics drift from
  upstream") becomes **more central** — shim correctness is
  now the shim's responsibility, not upstream's. Mitigation:
  the shim self-test discipline established at M1-2-1 (every
  primitive gets a smoke-test) continues; each new shim surface
  added for balloon gains a self-test in the same commit; the
  panic-on-call posture for unused-path stubs surfaces drift as
  runtime failure rather than silent misbehavior.

- **The "we run upstream Linux drivers" sentence becomes longer.**
  More precisely: "Arsenal compiles upstream Linux `.c` source
  against a BSD-licensed reimplementation of the Linux API." This
  matches drm-kmod's actual claim word-for-word. Truthful, but
  the marketing-shaped sentence is two clauses instead of one.

**New risks:**

- **Shim ABI drift across upstream Linux releases.** If Linux
  6.13 changes the signature of `struct virtio_device` (highly
  unlikely for stable bus interfaces, but real for internal
  headers), Arsenal's shim view becomes wrong silently.
  **Mitigation:** the quarterly re-pin checklist (ADR-0005 § 3)
  expands to "re-pin `.c` drivers + diff their `#include`d
  declarations against the shim's current declarations" — drift
  surfaces as a re-pin-time compile error rather than a runtime
  bug.

- **Over-stubbed shim functions.** A function the inherited
  driver calls rarely (one path in a thousand) might be stubbed
  panic-on-call and only surface as a runtime panic months
  later. **Mitigation:** the M1-2-X gap-filling discipline
  already accepts this; the shim self-test covers the common
  path; the panic-on-call posture is fail-loud rather than
  silent-misbehave per ADR-0005 § 6's stance. The QEMU smoke +
  real-Framework-13 boot validation gates at M1 step 7 catch
  the load-bearing paths.

- **The narrow UAPI carve-out widens over time without a clear
  gate.** A future amdgpu or iwlwifi PR might argue for
  vendoring an internal Linux header on convenience grounds,
  eroding the structural minimality this ADR establishes.
  **Mitigation:** the three-clause carve-out test (§ 3 above)
  is explicit; any deviation requires a follow-up ADR amending
  this one, not a tactical PR comment.

## Follow-up work

- **`STATUS.md`** § "Active work" updates to:
  - Mark M1-2-5 Part B sub-task 1 (vendor-fetch script) and
    sub-task 2 (closure audit + ADR-0006) complete with the
    findings summary.
  - Redraw Part B sub-task 3 from "iterate balloon compile
    against vendored headers" to "extend `linuxkpi/include/`
    with shim headers balloon's compile demands; iterate."
  - Note the scope reduction: no more recursive vendor-fetch
    cycles per driver.

- **`vendor/linux-6.12/README.md`** updates to reflect the
  narrowed scope: "this directory contains inherited Linux .c
  drivers + a narrow BSD-licensed UAPI carve-out for protocol
  headers; the Linux API headers (`<linux/*.h>`, `<asm/*.h>`,
  `<asm-generic/*.h>`) live in `linuxkpi/include/` per
  ADR-0006."

- **`linuxkpi/build.rs`** (sub-task 3 work): keep `-nostdinc`;
  do NOT add `-I vendor/linux-6.12/include`; do add `-I
  linuxkpi/include`. The cross-compile flag set and the BSD/GPL
  directory-boundary `check_path` enforcement are unchanged.

- **`linuxkpi/src/lib.rs`** docstring: drop the "minimal
  hand-curated header subset under `vendor/linux-6.12/`" line;
  point at this ADR for the headers-are-shim discipline.

- **`scripts/vendor-balloon.sh`** simplified per § 5: drop the
  recursive BFS + candidate-resolution + license-gate machinery
  (preserved in git history at `b2dd46f`); reduce to a
  ~60-80 LOC script that fetches `drivers/virtio/virtio_
  balloon.c` + the three BSD virtio UAPI headers + records the
  SHA in `vendor/linux-6.12/README.md`.

- **ADR-0005's "Reserved successor ADRs" list** (one-line
  inline edit in this commit): the provisional reservations
  shift up by one — ADR-0007 (three-crate split), ADR-0008
  (cbindgen adoption), ADR-0009 (deferred init via kthread +
  workqueue). *(Shifted again by ADR-0007's first-use claim on
  the 0007 slot: three-crate split → ADR-0008, cbindgen →
  ADR-0009, deferred init → ADR-0010. Shifted again by ADR-0008
  (module-init by symbol name), which resolved ADR-0005 § 6's
  deferred design decision and took the 0008 slot: three-crate
  split → ADR-0009, cbindgen → ADR-0010, deferred init → ADR-0011.
  Shifted again by ADR-0011 (deferred-work via a cooperative
  workqueue runner), which resolved the deferred-work half of
  ADR-0005 § 6 and claimed the 0011 slot — splitting the
  previously-combined "deferred init + initcall-style table"
  reservation. The initcall-style-table half stays provisional
  at ADR-0012; per-workqueue runner / freezable semantics is
  ADR-0013.)*

- **ARSENAL.md does not need modification.** Per CLAUDE.md,
  the plan is not edited unless the deviation is structural.
  This ADR amends an operationalization decision (ADR-0005),
  not a milestone or strategic commitment in ARSENAL.md. The
  "LinuxKPI-style shim modeled on FreeBSD drm-kmod" wording
  in ARSENAL.md § "Driver strategy" is now **more accurate**
  under this ADR, not less.

## References

- [ADR-0004: Pivot from Field OS to Arsenal](0004-arsenal-pivot.md)
- [ADR-0005: LinuxKPI shim layout and GPL/BSD-2 boundary](0005-linuxkpi-shim-layout.md)
  — the ADR this one amends (§ 3 and § 4)
- [`docs/plan/ARSENAL.md` § "Driver strategy"](../plan/ARSENAL.md)
  — the LinuxKPI-style-shim commitment this ADR brings closer to
  drm-kmod's actual practice
- [`CLAUDE.md` § 3 (BSD-2 base, GPLv2 preserved on inherited drivers)](../../CLAUDE.md)
  — preserved by this ADR, with a narrower interpretation of
  "inherited" (drivers, not headers)
- [`scripts/vendor-balloon.sh`](../../scripts/vendor-balloon.sh)
  — the recursive vendor-fetch tool whose 281-file halt
  surfaced this ADR
- [FreeBSD drm-kmod LinuxKPI headers](https://github.com/freebsd/drm-kmod/tree/master/linuxkpi/gplv2/include/linux)
  — the BSD-licensed Linux API reimplementation pattern this ADR
  adopts (10-year industrial-scale precedent)
- [Linux 6.12 LTS `arch/x86/include/asm/Kbuild`](https://git.kernel.org/pub/scm/linux/kernel/git/stable/linux.git/tree/arch/x86/include/asm/Kbuild?h=linux-6.12.y)
  — the `generic-y` / `mandatory-y` build-time header-generation
  rules that verbatim vendoring cannot capture (Finding 2)
- [Linux kernel docs § "Kbuild" — header generation](https://www.kernel.org/doc/html/latest/kbuild/makefiles.html)
  — the canonical reference for `generic-y` / `mandatory-y`
  semantics
- Michael Nygard, "Documenting Architecture Decisions" (2011) —
  ADR template authority

---

*This ADR is a structural amendment to ADR-0005, not a
replacement. The single-workspace-member layout, `cc`-driven C
build, bidirectional FFI with hand-written `shim_c.h`, and
synchronous module init are all preserved unchanged. What changes
is what gets vendored — `.c` source from upstream, BSD UAPI
protocol headers from upstream when the three-clause carve-out
test holds, and nothing else.*
