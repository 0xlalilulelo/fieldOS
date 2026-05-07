# ADR-0003: x86.c is already AoStr-sinked; encoder consumes captured corpus

## Status

Accepted. Supersedes ADR-0001 §3 step 4's "Files modified: holyc/src/x86.c
— output sink redirected from `fprintf` to an `AoStr` consumed by `asm.c`"
line. Leaves ADR-0001 §2 (in-tree minimal encoder) and the rest of §3
step 4 in force.

## Context

ADR-0001 §3 step 4's "Files modified" list — written before the audit
read `holyc/src/x86.c` end to end — names `holyc/src/x86.c` and frames
the edit as "output sink redirected from `fprintf` to an `AoStr`. Smallest
possible change; the rest of the codegen logic is untouched." That
paragraph presupposes a `FILE *` sink in `x86.c`. On reading the file at
M3-B step 4 entry the presupposition does not hold:

- `holyc/src/x86.c` contains zero calls to `fprintf` / `FILE *` / `fopen`
  / `fputs` / `fputc` (`grep -c` returns 0). It contains 246 calls into
  the `aoStrCat*` family (`aoStrCatPrintf`, `aoStrCatFmt`, `aoStrCat`,
  `aoStrCatLen`).
- The top-level codegen entry is `holyc/src/x86.c:2417 AoStr
  *asmGenerate(Cctrl *cc)`. It returns the full assembly text as an AoStr.
- Its sole caller is `holyc/src/compile.c:42 AoStr *compileToAsm(Cctrl
  *cc)`, which forwards the same AoStr.
- The host side's file write happens in `holyc/src/main.c:152 emitFile()`
  / `main.c:120 writeAsmToTmp()`, after `compileToAsm()` returns. ADR-0001
  §3 step 3 (commit `41a2060`) already excludes `main.c` from the
  kernel-resident subset.

The output sink is therefore already in the right shape for the in-tree
encoder. The asm text is in memory the moment the kernel side calls
`compileToAsm(cc)`. No `x86.c` edit is required to reach it.

The audit's separate question — *what AT&T forms does `x86.c` actually
emit?* — is unanswered, and the encoder cannot be specified without an
answer. That work survives the discovery; only the framing changes from
"redirect a sink" to "capture the corpus."

## Decision

### 1. `holyc/src/x86.c` is untouched in M3-B step 4

ADR-0001 §3 step 4's "Files modified" line for `holyc/src/x86.c` is
removed. The kernel side reaches the asm text through the existing
upstream API: `compileToAsm(cc)` → `AoStr *`. The encoder's parser
consumes `asmbuf->data` directly.

This strengthens the pin discipline in ADR-0001 §1: `x86.c` is not
locally edited at all, so a pin bump (the next one being post-M3) does
not require a manual rebase of an in-tree edit.

### 2. Host corpus capture replaces the redirect work

A host-only build target produces the encoder's specification input:

- `holyc/tools/dump-asm.c` — host program that links the existing host
  `hcc` libraries, calls `compileToAsm()` on a fixed input list, and
  writes the resulting `AoStr`s to `holyc/tests/corpus/*.s`.
- `holyc/holyc.mk` extension — `make corpus` builds and runs the tool;
  `make corpus-clean` removes the output. Both are host-side; the cross
  build does not depend on either.
- Initial input list: `holyc/bug-tests/Bug_171.HC` (the audit's witness
  for the host transpile path; commit `ea4c1da`). The list grows as
  later step 4 sub-rounds need new instruction forms.

The corpus is checked in. The `make corpus` target is reproducible from
the pin and the input list, so the checked-in `.s` files are diff-able
witnesses that x86.c's output for a given input has not silently changed
under us.

### 3. ADR-0001 §3 step 4's exit gate stands, restated

> For the instruction forms appearing in `Bug_171.HC`'s emitted assembly,
> `asm.c` output matches `$(CROSS_AS)` byte-for-byte. Harness runs in CI.

The exit gate is unchanged. What changes is *where the harness gets the
AT&T input from*: from `holyc/tests/corpus/*.s` (this ADR), not from a
redirected sink in `x86.c` (ADR-0001 §3 step 4 as written).

### 4. Path-B trigger unchanged

ADR-0001 §4's two-calendar-week / ~30-hour Path-B trigger applies to step
4 in aggregate (corpus capture + harness + encoder + coverage). This ADR
does not reset or extend the clock; it clarifies the work shape inside
the existing budget.

### 5. Alternatives considered and rejected

**(a) Edit ADR-0001 in place.** The ADR-0001 file is a landed
architectural record; in-place edits to a landed ADR break the audit
trail that the `docs/adrs/` directory exists to provide. Michael Nygard's
template (`docs/adrs/0000-template.md`) prescribes superseding-by-new-ADR
for exactly this case.

**(b) Drop the corpus capture entirely; let the harness use synthetic
AT&T input.** The harness's value is byte-for-byte equivalence against
`$(CROSS_AS)`; that property is meaningful only against AT&T lines that
`x86.c` actually emits, not lines we made up. Synthetic input risks
hidden divergence — the encoder passes the harness while miscompiling
real `x86.c` output — exactly the failure mode ADR-0001 §2's "two
compile paths" risk warns about.

**(c) Land the encoder first, capture the corpus later.** Possible, and
fast to first encoded byte, but it inverts the discipline: the encoder
ends up specified by what we wrote rather than by what `x86.c` emits.
The 1,500-line audit estimate is the audit's, not measured; specifying
against a measured corpus is the cheapest way to keep the estimate
honest.

## Consequences

Easier:

- ADR-0001 §1's pin discipline tightens. `x86.c` is not locally edited;
  a pin bump's `x86.c` rebase cost goes from "small but nonzero" to
  zero. The next pin re-eval (post-M3) is structurally simpler.
- Step 4 sub-decomposition gets a clean opening commit (corpus capture)
  that is independently committable, independently reviewable, and adds
  value even if the encoder work that follows is later abandoned for
  Path B.
- The corpus is a checked-in witness, not a runtime artifact. CI can
  diff `make corpus` output against the checked-in tree on pin bumps;
  any unexpected `x86.c` behaviour change surfaces as a real diff
  rather than as a downstream encoder breakage.

Harder:

- A new artifact tree (`holyc/tests/corpus/`) lives in the repo and
  must be regenerated whenever the input list grows or the pin moves.
  Cost is mechanical; the `make corpus` target is the single point of
  regeneration.
- `holyc/tools/dump-asm.c` is a small host-only file. It does not count
  against the 100,000-line base-system budget (it is host tooling, not
  base system) but its license and dependency profile are tracked the
  same way as the rest of `holyc/` (BSD-2-Clause, links the same host
  libraries the existing host `hcc` already links).

New risks:

- Corpus drift on pin bump. If a future `holyc-lang` pin emits the same
  semantics with different AT&T-text shape (whitespace, register
  ordering, instruction selection), the checked-in corpus diff at pin
  re-eval time is the witness, but the encoder may need new instruction
  forms to keep the harness green. Mitigation: pin re-eval procedure
  (`holyc/VERSION`) gains a step "run `make corpus`; review diff before
  bumping `VERSION`."
- Coverage scope-creep. The corpus is initially Bug_171.HC; later
  rounds will add more inputs to drive new clusters. The risk is that
  the corpus grows faster than the encoder can keep up, and the harness
  stays red for longer than ADR-0001 §4's two-week trigger allows.
  Mitigation: each round's commit lands corpus + encoder change
  together, never corpus alone.

Follow-up:

- ADR-0001 §3 step 4 stays in force minus its `holyc/src/x86.c`
  modification line. No edit to ADR-0001 itself; this ADR is the trail.
- `STATUS.md` does not update yet — this ADR is in service of M3-B
  step 4 entry, not a milestone exit.
- `docs/plan/phase-0.md` is unaffected; the §M3 trampolining strategy
  paragraph does not reach into x86.c-redirect specifics.
- Next session lands sub-candidate (A′) — `holyc/tools/dump-asm.c` and
  the `make corpus` target.

## References

- ADR-0001 §2 (in-tree minimal encoder), §3 step 3 (kernel-resident
  subset boundary), §3 step 4 (the superseded paragraph), §4 (Path-B
  trigger)
- `docs/skills/holyc-lang-audit.md` §"What M3 actually has to do" item 4
  (the audit's open assembler call, now closed by ADR-0001 §2 and
  shaped by this ADR)
- `holyc/src/x86.c:2417` — `AoStr *asmGenerate(Cctrl *cc)`
- `holyc/src/compile.c:42` — `AoStr *compileToAsm(Cctrl *cc)`
- `holyc/src/main.c:152` — `emitFile()`, the host-only file write
  excluded from the kernel-resident subset by ADR-0001 §3 step 3
- `holyc/bug-tests/Bug_171.HC` — initial corpus input (the audit's
  host-transpile witness, commit `ea4c1da`)
- `docs/adrs/0000-template.md` — Michael Nygard's ADR template; the
  superseding-rather-than-editing rule applied here
