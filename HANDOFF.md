Kickoff for the next session — M0 step 3G, the `>` prompt + M0 step 3 perf gate.

M0 step 3F (LAPIC + soft preemption) closed cleanly across five
commits ending at 0323497 on 2026-05-13, plus the 3F-4 STATUS +
devlog at d940b59 the same evening: 8259 masked, LAPIC MMIO
mapped (7dd1dfd), software enable + spurious vector 0xFF wired
(896183e), latent task-stack fix from 16 KiB to 32 KiB
(41e7f8d), PIT-calibrated 100 Hz periodic timer at vector 0xEF
(6c4b169), and idle's `sti` + `hlt` + ARSENAL_TIMER_OK probe
(0323497). HEAD is d940b59; main is six commits ahead of
origin/main. Working tree clean. Smoke is green at nine
sentinels in ~1 s locally; the calibration line reports
`apic: calibrated 624375 LAPIC ticks per 10 ms; armed periodic
100 Hz vector=0xef initial_count=624375` against QEMU TCG -cpu
max.

3G is the last sub-block of M0 step 3. After 3G lands, step 3
closes and the next major surface is step 4 (SMP — IPI bring-up,
AP startup, per-CPU LAPIC state, hard preemption discipline).
Three load-bearing things 3G has to ship: an interactive `>`
prompt over the serial + framebuffer stack, the M0 step 3
*usability gate* (prompt is keyboard-navigable; shows hardware
summary), and the M0 step 3 *performance gate* (boot to prompt
in < 2 s under QEMU, asserted in CI). The bug-prone surface is
the perf gate, not the prompt — keyboard input on QEMU q35 has
a single well-documented controller (i8042) and the cooperative
scheduler is a fine substrate for a polled input task, but
deterministic wall-clock measurement across hosted runners is
genuinely novel CI surface.

read CLAUDE.md (peer concerns, Rust-only, BSD-2-Clause base,
build loop sacred, no_std + nightly + abi_x86_interrupt — the
usability gate from §3 of CLAUDE.md / M0 step 3 of ARSENAL.md
is what 3G has to satisfy) → STATUS.md (3F complete, 3G is the
active sub-block of step 3) → docs/plan/ARSENAL.md § "Three
Concrete Starting Milestones" → M0 (performance gate: boot to
prompt < 2 s under QEMU; security gate: zero unsafe Rust outside
designated FFI boundaries — already satisfied; usability gate:
prompt is keyboard-navigable and shows hardware summary) →
docs/devlogs/2026-05-arsenal-preemption.md (specifically the
"What 3G looks like" section — PS/2 vs virtio-keyboard, line
editing, command dispatch, and the perf-gate CI surface are the
four pieces enumerated there) → arsenal-kernel/src/sched.rs
(idle's `sti` + `hlt` is at sched.rs:255-289; new tasks spawn
via sched::spawn which already exists at sched.rs:157-160 and
is currently `#[allow(dead_code)]` until 3G's shell task uses
it — drop the allow when the shell spawns) →
arsenal-kernel/src/main.rs (the boot order at _start —
sched::init runs last; the shell task must be sched::spawn'd
*before* sched::init switches into idle, so spawn() goes in
main.rs between net's smoke and sched::init) →
arsenal-kernel/src/serial.rs (UART RX is at 0x3F8 base + line
status register bit 0; UART input is not the M0 prompt path
but it's a useful fallback if PS/2 misbehaves) →
arsenal-kernel/src/fb.rs (the cursor state machine is at
fb.rs:cursor_x / cursor_y; visible cursor rendering is new — a
static underscore at the insertion point is the recommendation
below) → arsenal-kernel/src/idt.rs (3G adds *no* new IDT
entries — polled keyboard, no IRQ; if the IRQ-driven path wins
the trade-off, 3G adds vector 0x21 alongside 0xEF / 0xFF) →
ci/qemu-smoke.sh (REQUIRED_SENTINELS array at line 39 grows by
one to "ARSENAL_PROMPT_OK"; the perf-gate addition is the
genuinely new CI surface — wall-clock measurement between two
sentinels, threshold from envvar) → Cargo.toml (3G adds no new
deps — i8042 is x86 port I/O, the shell is core Rust + alloc) →
git log --oneline -10 → run the sanity check below → propose
3G-N commit shape (or argue for a different decomposition) →
wait for me to pick → "go 3g-N" for code, "draft 3g-N" for
paper deliverables.

Where the project is

  - main is at HEAD d940b59 (docs(status,devlogs): M0 step 3F
    complete, 3G next). Working tree is clean except this file.
    main is six commits ahead of origin/main (3F-0 + 3F-1 +
    41e7f8d task-stack fix + 3F-2 + 3F-3 + 3F-4 paper). CI
    armed; smoke ~1 s locally, ~2-3 min on ubuntu-24.04 runners
    (mostly toolchain setup + cargo's release build of rustls +
    RustCrypto). The 3F-2 task-stack bump from 16 KiB to 32 KiB
    means new tasks 3G adds (the shell) cost +32 KiB heap each;
    HEAP_CAP at 16 MiB has ample headroom.
  - LOC: arsenal-kernel/src/ is 19 files post-3F (apic.rs added
    at 3F-0). Net Rust LOC ~4,768 post-3F-3. ELF 1,479,400 bytes
    / ISO 19.3 MB. 3G adds maybe 350-450 lines net across the
    five sub-commits: ~120 in a new kbd.rs (PS/2 driver +
    scancode→ASCII translation), ~80 in a new shell.rs (line
    editor + dispatch loop), ~120 in shell.rs for the seed
    commands (`help`, `hw`, `panic`), ~30 in main.rs (mod
    declarations + spawn site + prompt print site), ~30 shell
    changes in ci/qemu-smoke.sh for the perf gate.
  - Toolchain: nightly-2026-04-01 pinned in rust-toolchain.toml.
    3G uses no new nightly features.
  - Crates currently linked: limine 0.5, linked_list_allocator
    0.10, spin 0.10, x86_64 0.15, smoltcp 0.12, rustls 0.23,
    rustls-rustcrypto 0.0.2-alpha, getrandom 0.4 + 0.2. The
    x86_64 crate has the I/O port helpers (`x86_64::instructions::
    port::Port`) we need for the i8042; the CPUID intrinsics for
    `hw`'s output are in `core::arch::x86_64::__cpuid`. No
    Cargo.toml changes for 3G.
  - Sentinels: smoke requires nine. 3G adds one —
    ARSENAL_PROMPT_OK — emitted from the shell task after the
    prompt has printed and the task is online (recommend
    "emit immediately after the first `> ` writes, before the
    first poll loop iteration" — see trade-off below). Total
    on 3G exit: ten sentinels, plus the perf gate as a tenth
    *implicit* assertion ("time-to-ARSENAL_PROMPT_OK is within
    the budget").
  - HANDOFF.md (this file) was committed at dcf2377 with the 3F
    contents and is being rewritten now for 3G. Prior contents
    are in git history.

3G — `>` prompt + perf gate

The plan below is the kickoff proposal, not gospel. The user
picks the shape; deviations get justified before code lands.

Sub-candidate decomposition

  (3G-0) **PS/2 keyboard driver (polled).** Adds
         `arsenal-kernel/src/kbd.rs`. The i8042 controller lives
         at I/O ports 0x60 (data) and 0x64 (status / command).
         `kbd::init` is essentially "trust QEMU's default
         configuration" — q35 boots with the controller enabled,
         keyboard on port 1, scancode set 1 translation active
         from the controller. Verify with a status-register
         read (0x64 bit 2 should be clear — output buffer
         empty — after any boot-time scancodes drain) and log.
         `kbd::poll` reads the status register; if bit 0 (OBF)
         is set, reads 0x60, translates the scancode through a
         static table to ASCII (with internal modifier state
         for left/right shift; ctrl arrives in 3G-2 if needed
         by `panic` / `hw` modifiers). Returns Option<u8>.
         Scancode set 1 single-byte ranges are 0x01-0x58 for
         press, 0x81-0xD8 for release (high bit set); extended
         (E0/E1) sequences for arrow keys / numpad get
         absorbed-and-ignored at M0 (consume the prefix byte,
         consume the next byte, return None). ~120 LOC; one
         commit: `feat(kernel): PS/2 keyboard polling`. Use
         **go 3g-0**.

  (3G-1) **Shell task + line editor.** Adds
         `arsenal-kernel/src/shell.rs`. `shell::run` is the task
         entry point: it prints the prompt `> ` once at startup,
         emits ARSENAL_PROMPT_OK, then loops calling
         `kbd::poll`, accumulating bytes into a bounded
         `[u8; 256]` line buffer with backspace handling, echoing
         each byte to serial (which fans out to fb via 3E's
         mirror). Newline triggers `dispatch(&buf)` (a no-op
         stub in 3G-1; commands land in 3G-2), buffer clears,
         next prompt prints. The task is sched::spawn'd from
         _start *before* sched::init runs (sched::init takes
         the boot stack and switches into idle; spawn-then-init
         is the established 3B-5 pattern). Visible cursor: a
         static underscore at the insertion point, redrawn on
         each character (clear the underscore by drawing a
         space, write the new character, draw a new underscore
         one cell right). ~80 LOC; one commit: `feat(kernel):
         shell task + line editor`. Use **go 3g-1**.

  (3G-2) **Command dispatcher: `help`, `hw`, `panic`.**
         `dispatch(&buf)` matches the first whitespace-delimited
         token. `help` prints a one-line description per known
         command. `hw` prints the hardware summary required by
         M0's usability gate: CPU brand string (CPUID leaves
         0x80000002..0x80000004, each returns 16 bytes of brand
         string), CPU core count (always 1 at M0), RAM total +
         free (from `frames::stats()` already in 3A), LAPIC
         version + spurious + timer vectors (from apic.rs
         getters added here), virtio devices (block + net device
         IDs from virtio_blk / virtio_net, getters added). `panic`
         calls `panic!("user-initiated panic")` which exercises
         the panic handler from 3B-2 in interactive mode. Unknown
         tokens print `unknown command: <token>; try 'help'`.
         ~120 LOC plus 3-5 LOC of getters in apic / virtio_blk /
         virtio_net. One commit: `feat(kernel): shell commands —
         help, hw, panic`. Use **go 3g-2**.

  (3G-3) **Perf gate in CI.** ci/qemu-smoke.sh grows wall-clock
         measurement between two sentinels: ARSENAL_BOOT_OK
         (kernel's first serial line, after Limine hands off)
         and ARSENAL_PROMPT_OK (shell task online). The
         difference is "kernel boot time" — distinguished from
         QEMU launch + harness overhead by anchoring against a
         kernel-emitted sentinel rather than wall time from
         script start. Threshold from envvar `BOOT_BUDGET_MS`,
         default 3000 (3 s — gives headroom over ARSENAL.md's
         2 s target for ubuntu-24.04 runner variance; local
         runs should report ~500-700 ms and hit the 2 s target
         comfortably). Tight-miss handling: if time-to-prompt
         is in (BOOT_BUDGET_MS, 1.5 × BOOT_BUDGET_MS], retry
         once and pass if either run is under budget; if both
         exceed or any run exceeds 1.5×, fail. Output the
         measurement on PASS so the trend is visible across
         runs. ~30 LOC shell. One commit: `ci(smoke): perf gate
         — boot to prompt < 3 s`. Use **go 3g-3**.

  (3G-4) **STATUS.md refresh + 3G devlog + M0 step 3 close.**
         STATUS flips 3G from "next" to "complete," promotes
         M0 step 4 (SMP) to the active block, and writes the
         M0 step 3 retrospective sub-section (3F-2 task-stack
         posture change, 3E scroll-by-blit no-longer-untested
         since `hw` output finally crosses 800 px, the
         sub-block-per-devlog cadence as a model, calendar-
         days-vs-FTE-weeks calibration). Devlog at
         `docs/devlogs/2026-05-arsenal-prompt.md` records the
         PS/2-over-virtio-keyboard call, the polled-over-IRQ
         call (IOAPIC deferral to step 4), the perf-gate CI
         shape (anchoring to a kernel sentinel, the tight-miss
         retry), and the "this is what the M0 step 3 exit
         looked like" summary. Two commits: `docs(status):
         M0 step 3 complete, step 4 (SMP) next` and
         `docs(devlogs): Arsenal prompt + M0 step 3 exit`. Use
         **go 3g-4** for STATUS, **draft 3g-4-devlog** for the
         devlog.

Realistic session-count estimate: 3G-0 is one focused session
— the i8042 spec is well-trodden and the scancode-set-1 table
is mechanical, but the first attempt at a polling loop
typically misses some status-register subtlety. 3G-1 is half a
session (the line editor is well-bounded; the cursor rendering
adds maybe 15 LOC). 3G-2 is one session (the three commands
plus their getters; CPUID brand-string formatting is the only
fiddly bit). 3G-3 is one session — wall-clock measurement is
30 LOC of shell but the right shape requires deliberate
thought about what variance you're trying to absorb. 3G-4 is
one session including the devlog. Per CLAUDE.md "~15 hours per
week, multiply by ~2.3," call it 1-2 calendar weeks for 3G if
the cadence holds, or 2-3 weeks if 3G-3's gate flakes on hosted
runners and needs iteration.

Trade-off pairs to surface explicitly

  **PS/2 vs virtio-keyboard.** The keyboard input transport.
  (i) **PS/2 (i8042).** Universally available on every QEMU
  machine type since q35's introduction; same controller on
  every commodity x86 motherboard since 1984. Port I/O at
  0x60 / 0x64. No probe — the controller is just *there*.
  Polled or IRQ-driven (IRQ1 → vector 0x21 once IOAPIC routes
  it).
  (ii) **virtio-keyboard.** Reuses the modern-PCI transport
  from 3C, same shape as virtio-blk / virtio-net. Cleaner
  abstractly, but not all hypervisors expose virtio-input
  device classes by default (QEMU does; some cloud environments
  don't), and the device-class probe adds surface 3C doesn't
  cover.
  Recommend (i). M0's hardware story is "boot on what's
  available everywhere." PS/2 is exactly that. virtio-keyboard
  arrives if there's ever a reason to retire i8042 — there
  probably isn't, at least not pre-M1.

  **Polled vs IRQ-driven input.**
  (i) **Polled.** Shell task calls `kbd::poll` on each
  iteration; if no scancode pending, yields. With idle's hlt
  at 100 Hz and the cooperative round-robin from 3B,
  shell gets scheduled at least every ~10 ms, giving an
  effective 100 Hz polling rate — well above human typing
  speed (a fast typist is ~10 Hz at the keystroke level).
  Zero new IRQ infrastructure.
  (ii) **IRQ-driven.** Wire i8042 IRQ1 → an IDT vector. The
  8259 is masked from 3F-0; the LAPIC has no LVT entry for
  IRQ1 today, so this requires either unmasking just IRQ1 on
  the 8259 (which would re-introduce the 8259 as a delivery
  path the kernel has explicitly stopped using) or bringing
  up the IOAPIC for IRQ1 routing through the LAPIC. IOAPIC
  bring-up is M0 step 4 territory (ACPI MADT parsing, IOAPIC
  MMIO, redirection table programming).
  Recommend (i). Polling sidesteps the IRQ-routing question
  entirely. The latency cost is bounded at ~10 ms, which is
  imperceptible. If a future workload needs sub-10-ms input
  responsiveness, 3G's polled shape is a 30-line change to
  IRQ-driven once IOAPIC arrives at step 4.

  **Scancode set 1 vs set 2 vs USB HID.**
  (i) **Set 1 with i8042 translation.** The QEMU q35 i8042
  default. Single-byte press codes 0x01-0x58, single-byte
  release codes with high bit set, extended sequences with
  0xE0 / 0xE1 prefixes. Mechanical translation table.
  (ii) **Set 2 with controller translation disabled.** More
  modern, finer-grained, but requires explicitly disabling
  controller translation and the translation table is larger.
  (iii) **USB HID via xHCI.** M1 territory.
  Recommend (i). The translation table for printable ASCII
  (0x1E-0x39 covers a-z + most punctuation) is straightforward;
  extended sequences (arrows, numpad) get consumed-and-ignored
  at M0 with a 5-line state machine. Set 2 buys nothing M0
  needs.

  **Shell task vs idle does it vs ping/pong does it.**
  (i) **Dedicated shell task.** sched::spawn(shell::run).
  Owns its own line buffer, cursor state, dispatch logic. Idle
  stays minimal (yield + observe + hlt).
  (ii) **Idle does the shell work.** Folds shell into idle_loop.
  Saves one Task allocation but couples shell behavior to idle's
  power-save semantics — idle's `hlt` would block keyboard
  polling between ticks.
  (iii) **Repurpose ping/pong.** ping/pong's six-iterations-
  then-yield-forever loop is currently dead-after-completion;
  could be repurposed for the shell.
  Recommend (i). Cleanest separation, matches the 3B-5 pattern,
  and ping/pong is preserved as the scheduler's smoke witness
  (yes, the scheduler still works) without semantic overload.

  **Cursor: blink / static / none.**
  (i) **Static underscore at insertion point.** Drawn once on
  each character write. Simple, visible, no animation surface.
  (ii) **Blinking cursor.** Requires periodic redraw, which
  means hooking the timer probe or running a dedicated
  cursor-blink task. Cosmetic value only.
  (iii) **No cursor.** The mouse-driven world's default, but
  for a TUI-style prompt the insertion point should be visible.
  Recommend (i). The static underscore is what TempleOS,
  early Linux consoles, and `cat` interactively show. Zero
  animation infrastructure.

  **Echo: serial + fb / serial only / fb only.**
  (i) **Both.** The shell's `echo_char` writes to serial,
  which fans out to fb via 3E's mirror. Smoke can verify
  the prompt is responsive via the serial log; humans see it
  on the framebuffer.
  (ii) **Serial only.** Smoke-friendly, but the framebuffer
  doesn't show typed input — bad usability.
  (iii) **fb only.** Inverse of (ii) — no smoke verification.
  Recommend (i). The 3E mirror exists exactly for this case;
  using it costs nothing.

  **ARSENAL_PROMPT_OK fires when.**
  (i) **Immediately after the first `> ` writes**, before the
  first kbd::poll call. The sentinel asserts "the shell task
  is online and the prompt is on screen." Smoke-observable
  without simulating input.
  (ii) **After the first input character is consumed.** The
  sentinel asserts a stronger property ("the input loop is
  alive") but requires the smoke to simulate input, which
  QEMU's `-display none + -serial file` doesn't support
  cleanly — would need either `-chardev pipe` for stdin
  injection or a QMP-driven scripted-input harness, both
  significantly more surface than M0 wants.
  Recommend (i). The prompt printing is the user-observable
  "I see a prompt" moment, matches ARSENAL.md's "boot to
  prompt < 2 s" phrasing exactly, and is testable in the
  existing headless smoke. Interactive input testing
  (typing `hw` and seeing the summary) is manual under
  `-display gtk`; record outcomes in the devlog.

  **Perf gate measurement anchor.**
  (i) **Wall clock from QEMU launch.** Captures QEMU startup,
  Limine, kernel boot, shell spawn — everything end-to-end.
  Most pessimistic; includes harness variance.
  (ii) **Wall clock between two kernel sentinels** —
  ARSENAL_BOOT_OK (the first kernel-emitted sentinel) and
  ARSENAL_PROMPT_OK. Measures only the kernel's contribution.
  (iii) **Wall clock between Limine handoff and prompt.**
  Subset of (ii) without ARSENAL_BOOT_OK's print cost.
  Recommend (ii). Isolates kernel performance from harness
  variance (TLS cert generation, Python listener startup,
  QEMU process spawn — all of which add 200-500 ms on hosted
  runners). Both timestamps are in `$SERIAL_LOG` if we add
  a per-line wall-clock prefix to the file or use `date +%s%N`
  captures bracketing the grep window. The script shape:
  capture `start_ns` when ARSENAL_BOOT_OK first appears in
  the log, `end_ns` when ARSENAL_PROMPT_OK appears, assert
  `(end_ns - start_ns) / 1_000_000 < BOOT_BUDGET_MS`.

  **Perf gate threshold.**
  (i) **Hard 2000 ms** matching ARSENAL.md verbatim. Risks
  flaking on hosted runners.
  (ii) **3000 ms with `BOOT_BUDGET_MS` envvar** for local
  override (locals set 2000 to confirm the M0 spec is being
  met). Tight-miss retry between 1× and 1.5× the budget.
  (iii) **Per-environment thresholds** (local: 2000, hosted:
  4000) via uname or `$CI` detection.
  Recommend (ii). Single threshold with envvar override
  keeps the script simple and the M0 spec verifiable; the
  default of 3000 ms gives the hosted runner enough slack
  while keeping the gate meaningful. Local manual run with
  `BOOT_BUDGET_MS=2000 ci/qemu-smoke.sh` is the ARSENAL.md
  conformance check.

  **Sub-candidate granularity.**
  (a) **Five-commit shape** above (kbd / shell+editor /
  commands / perf gate / STATUS+devlog). Bisect-rich.
  (b) Combine 3G-0+3G-1 (kbd + shell pulled together since
  shell consumes kbd's output and they're useless apart).
  (c) Combine 3G-2+3G-3 (commands + perf gate as "everything
  needed for the M0 step 3 exit gate").
  Recommend (a). 3G-1's shell skeleton with a stub dispatch
  is independently smoke-verifiable (ARSENAL_PROMPT_OK fires;
  manual under -display gtk shows typed input echoing). 3G-2
  adds the dispatch; 3G-3 adds the perf assertion. Folding
  loses bisect points at each transition.

Sanity check before kicking off

    git tag --list | grep field-os-v0.1   # field-os-v0.1
    git log --oneline -10                 # d940b59, 0323497,
                                          # 6c4b169, 41e7f8d,
                                          # 896183e, 7dd1dfd,
                                          # dcf2377, ef36a68,
                                          # 9049f56, e115095
    git status --short                    # ?? HANDOFF.md (only,
                                          # while drafting this)
                                          # or clean once committed
    cargo build -p arsenal-kernel --target x86_64-unknown-none --release
                                          # clean, ~1.479 MB ELF
    cargo clippy -p arsenal-kernel --target x86_64-unknown-none --release -- -D warnings
                                          # clean
    cargo xtask iso                       # arsenal.iso ~19.3 MB
    ci/qemu-smoke.sh                      # ==> PASS (9 sentinels in ~1s)

Expected: HEAD as above; smoke PASSes in ~1 s with nine
sentinels including ARSENAL_TIMER_OK; calibration line in serial
reports "apic: calibrated 624375 LAPIC ticks per 10 ms".

If smoke fails after 3G-0 / 3G-1 land, the likely culprits are:
(a) i8042 status-register polling backwards — confusing IBF
(0x64 bit 1 — input buffer full from controller's perspective,
which we want clear before writing) with OBF (0x64 bit 0 — output
buffer full, which we want set before reading); (b) scancode
table off-by-one against the set-1 reference; (c) the shell
task's stack overflowing on the first `dispatch` call before
3G-2 lands the actual commands (stub dispatch should be a
literal no-op — print nothing, just return); (d) the shell
spawning *after* sched::init has switched into idle (idle's
runqueue would never see the shell — spawn-before-init is the
established pattern from main.rs); (e) ARSENAL_PROMPT_OK firing
from inside cpu::current_cpu().runqueue.lock() (deadlock on
re-entry — emit the sentinel after the lock is dropped, before
the poll loop).

If 3G-3's perf gate fails locally but passes on hosted runners
(or vice versa), the working hypothesis is that the wall-clock
anchor measurement is including harness variance the bound
shouldn't be sensitive to. Walk the script's `start_ns` /
`end_ns` capture points and confirm they bracket the kernel's
contribution only, not the surrounding script.

Out of scope for 3G specifically

  - IOAPIC bring-up. M0 step 4 — required when IRQ-driven
    keyboard or other device IRQs arrive past the BSP-only
    LAPIC story.
  - IRQ-driven keyboard input. Polled at M0; revisits at
    step 4 once IOAPIC routes IRQ1 → an LAPIC vector.
  - USB keyboard / xHCI. M1 — requires the xHCI driver from
    ARSENAL.md M1's surface.
  - Scancode set 2 / extended (E0/E1) scancode sequences.
    Set 1 with E0/E1 consume-and-ignore is plenty for M0's
    printable-ASCII input.
  - Cursor blink animation. Static underscore is enough.
  - Tab completion. The dispatcher matches whole tokens; tab
    completion is post-M0.
  - Command history. Same — post-M0.
  - TUI / curses-style screen control. No alternate buffer,
    no clear-screen escape, no positioned text. The prompt
    is a simple line-oriented REPL.
  - Color escapes in the prompt. The amber-on-navy from 3E is
    the global palette; per-byte color codes wait for a
    richer UI.
  - Multi-line input or line continuation. One line per Enter.
  - Read line history persistence. Same — post-M0.
  - The big "hardware summary" output. `hw` at M0 reports CPU
    brand string + RAM total/free + LAPIC version + virtio
    devices. Richer summaries (per-PCI-device descriptor,
    ACPI tables, NUMA topology) arrive when those subsystems
    land.
  - PCI device enumeration past what 3C already does. `hw`
    queries `virtio_blk` and `virtio_net` directly; broader
    PCI walk is post-M0.
  - SMP-aware `hw` output. Single-CPU at M0; the core count
    line is a hardcoded `1` until step 4 makes it real.
  - Configuration files / boot args / kernel cmdline. The
    shell takes interactive input only; persistent
    configuration is post-M0.

Permanently out of scope (do not propose)

  - Any unsafe block without a // SAFETY: comment naming the
    invariant the caller must uphold. CLAUDE.md hard rule.
  - Reverting any 3A / 3B / 3C / 3D / 3E / 3F commit. All
    landed and validated by smoke + CI.
  - Force-pushing to origin. Branch is in sync; preserve
    history.
  - Dropping the BSD-2-Clause license header from any new
    file.
  - Pulling a GPL crate into the kernel base.
  - Religious framing. CLAUDE.md hard rule.
  - Reintroducing HolyC. ADR-0004's discard is final.
  - Going back to stable Rust.
  - Skipping the build + smoke loop on a feat(kernel) commit.

Three notes worth flagging before you go

  1. **The smoke can't simulate keyboard input.** QEMU's
     `-display none + -serial file` configuration in
     ci/qemu-smoke.sh has no path for injected stdin bytes
     reaching the i8042 (`-chardev pipe` for stdin would,
     but that's a different chardev wiring and adds smoke
     surface). The implication: 3G's smoke verifies the
     prompt *prints* and the shell task is *online*
     (ARSENAL_PROMPT_OK), and the perf gate measures
     time-to-prompt. Interactive validation — typing `hw`,
     seeing the summary, typing `panic`, seeing the panic
     handler — is manual under `-display gtk` / `-display
     sdl` and gets recorded in the 3G devlog. This isn't a
     limitation worth eliminating; the smoke verifies the
     deterministic property (prompt online) and humans
     verify the interactive property (input responsive),
     which is the right split.

  2. **3E's scroll-by-blit path will finally run.** Boot
     output post-3F is ~35 serial lines × 16 px ≈ 560 px,
     still inside the 800 px frame. 3G's prompt adds at
     least one more line; the `hw` command (when run
     interactively) prints ~8-12 lines, which would push
     total output past 800 px and trigger the scroll-by-blit
     in fb.rs for the first time. Worth a manual test under
     `-display gtk` after 3G-2 lands. The path is code-review
     correct from 3E but has never executed; if it has a bug,
     this is when it surfaces.

  3. **The perf gate is the most genuinely novel CI surface
     in M0 step 3.** Sentinels are easy: a string appears in
     a log, the grep matches, the test passes. Wall-clock
     budgets are not: hosted runners have variance that
     interactive sessions don't, the clock source (nanoseconds
     via `date +%s%N` on Linux, microseconds via `gdate +%s%N`
     on macOS with coreutils, or millisecond-precision
     `python3 -c 'import time; print(int(time.time()*1000))'`
     as a portable fallback) needs care, and the tight-miss
     retry shape is the kind of thing that hides bugs for
     months because retries cover up real regressions. The
     3G-3 commit message should document the methodology
     explicitly so future-you can audit it. If 3G-3 ends up
     flaky and the retry isn't enough, the right move is to
     raise the budget (which is documented as an envvar) and
     file a TODO against either kernel boot-time profiling
     or harness deflaking, not to delete the gate.

Wait for the pick. Do not pick silently. The natural first
split is 3G-0 in one focused session ("PS/2 is alive,
scancode bytes appear in the log"), 3G-1 in a session ("the
prompt prints and ARSENAL_PROMPT_OK lands"), 3G-2 in a
session ("`hw` works manually"), 3G-3 in a session ("the
budget is asserted in CI"), 3G-4 in a session ("STATUS +
devlog, M0 step 3 closes"). Happy to combine 3G-0 + 3G-1 if
you want the smoke-observable "prompt prints" milestone in
one push, or to do 3G-3 (the perf gate) up front as a
standalone change against the existing 9-sentinel smoke
before any prompt work lands — that would let the gate
infrastructure mature before it has to cover the actual
boot-to-prompt path. Your call.
