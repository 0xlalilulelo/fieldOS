# holyc-lang Audit (Field OS / M0 step 5)

> Reading-pass notes on the upstream `holyc-lang` source tree we
> vendored in `holyc/`. Written so future-me arriving at M3 can
> graft a freestanding backend without re-reading 21,000 lines.
> Snapshot date: 2026-04-30. Re-audit if we bump the pin.

## Vendored pin

| Field | Value |
|---|---|
| Upstream | https://github.com/Jamesbarford/holyc-lang |
| Tag | `beta-v0.0.10` (released 2025-08-02) |
| Tarball | `archive/refs/tags/beta-v0.0.10.tar.gz` |
| SHA-256 | `be664891b02e68424299d1ad874bddce84e46476a23436973865dd190731d3e4` |
| HEAD at audit time | `ecbac55efbf276ebb268de93521034cec7829681` (2026-04-27, *not* used — we pin the tag) |
| License | BSD-2-Clause, James W M Barford-Evans 2024 |
| Repo stats | 911 stars / 67 forks at audit time |

License is compatible with Field OS BSD-2 base. Vendored under
`holyc/` without a submodule — phase-0.md §M0 expects heavy
modification, and we want a single tree the kernel build can walk.

## Architecture

`holyc-lang` is a **non-optimising AST-walking compiler** that emits
AT&T-syntax x86_64 assembly text and shells out to host `gcc` to
assemble and link. There is no JIT in upstream. There is no IR; the
parser produces an AST and `x86.c` walks it directly.

Pipeline today:
```
.HC source
    │
    ▼  lexer.c  (208h + ~?? c)
[token stream]
    │
    ▼  parser.c (1,985 lines) + prslib.c (1,256) + prsutil.c (505)
[AST in ast.c (424h)]
    │
    ▼  x86.c   (2,460 lines)   ← the codegen
[AT&T asm text in an AoStr buffer]
    │
    ▼  main.c calls system("gcc ... -o file ...")
[ELF binary]
```

Alternative output paths supported by upstream CLI:
- `--transpile` (transpiler.c, 1,728 lines): HolyC → C source. **Not
  on Field OS's path** — we want native, not C-via-gcc.
- `--cfg` (cfg.c, cfg-print.c): emit Graphviz of the control-flow
  graph for debugging. Useful for our M3 debugging but not on the
  critical path.

## Code map (src/)

42 files: 20 .c + 22 .h. Grand total **21,497 LOC**. Largest concerns:

| File | LOC | Role | M3 disposition |
|---|---:|---|---|
| `x86.c` | 2,460 | x86_64 codegen, walks AST and emits asm text | **Heavy edit**: replace emit-text path with binary encoding *or* keep text and add an in-tree assembler |
| `parser.c` | 1,985 | HolyC grammar parser | **Keep mostly unchanged** |
| `transpiler.c` | 1,728 | HolyC → C transpiler | **Strip** for the JIT path (Path C); **keep** if Path B (Python holyc2c) fallback ever folds in |
| `prslib.c` | 1,256 | Parser helper library | **Keep** |
| `prsutil.c` | 505 | Parser utilities | **Keep** |
| `parser.c` | (above) | | |
| `cctrl.c` | ?? | "Compiler control" struct — the global state holder | **Keep**, narrow public API |
| `aostr.c` | ?? | Auto-string buffer used everywhere for output | **Keep**, point its allocator at `palloc`/`pfree` |
| `ast.c` | ?? | AST node types and ctors | **Keep** |
| `lexer.c` | ?? | Tokenizer | **Keep** |
| `containers.c` | ?? | Generic hashmap / list / vec | **Keep** |
| `cli.c` | ?? | Argument parsing — 20+ flags | **Strip** for in-kernel REPL; keep when used as host transpiler |
| `cli.h` | 87 | (CliArgs struct) | partial keep |
| `main.c` | ?? | Drives the pipeline; **3× `system()` calls** | **Replace entirely** with kernel `holyc_eval(source)` entry |
| `cfg.c` + `cfg-print.c` | ?? | CFG visualiser | Defer |
| `prsasm.c` | 444 | Inline-asm parser | Probably keep |
| `arena.c`, `mempool.c`, `memory.c` | small | Allocators | **Replace** with kernel slab |

(Sizes marked `??` are knowable but not yet logged; fill in when we
crack each file open in M3.)

## libc and host-assumption surface

The compiler is fully hosted today. Headers seen at the top of two
representative files:

`cli.c` includes: `<assert.h> <ctype.h> <errno.h> <fcntl.h>
<limits.h> <stdarg.h> <stddef.h> <stdio.h> <stdlib.h> <string.h>
<unistd.h>`

`x86.c` includes: `<assert.h> <math.h> <stdint.h> <stdio.h>
<stdlib.h> <string.h>`

The bigger problems:

1. **`system()` in `main.c`** (3 sites). Forks the host shell to run
   `gcc`. Cannot exist in a kernel JIT. The ELF assembling step has
   to come from us. Options at M3 entry:
   - Write a minimal x86_64 text assembler (~1,500 lines, real work)
   - Modify `x86.c` to emit machine bytes directly (more invasive
     but eliminates the assembler-shaped detour)
   - Build the source through host gcc, link the result into the
     kernel image at compile time, and call it "Phase 0 JIT-ish"
     (cheating; defers the real solution to v0.2)
2. **`<math.h>`** in `x86.c`. Smells like float-constant folding.
   Need to audit which functions; most can be replaced with inline
   bit-twiddles, but if it's transcendentals (`sin`, `cos`, `pow`)
   we either ship a freestanding libm subset or limit codegen.
3. **`<stdio.h>`** — `printf`/`fprintf`/`fopen`/`fread` for source
   input and asm output. Replace with our kernel `serial_*` and an
   in-memory buffer interface (likely just keep `AoStr` as the
   universal sink).
4. **`<stdlib.h>`** — `malloc`/`free`/`exit`/`atoi`. Map `malloc`/
   `free` onto the kernel slab; `exit` becomes a kernel panic;
   `atoi` is trivially re-implementable.
5. **`<string.h>`** — `memcpy`/`memmove`/`memset`/`strlen`/`strcmp`/
   `strchr`/`strdup`. All have freestanding equivalents we can write
   in ~50 lines; `gcc` may also emit calls to `memcpy` and friends
   from generated code, so they need real implementations.
6. **`<unistd.h>`/`<fcntl.h>`** — POSIX file I/O. Strip; the kernel
   has no files at M3 (ramfs lands in M9).
7. **`<assert.h>`** — replace with our `Bt(cond, msg)` panic.
8. **CMake build** (`src/CMakeLists.txt`). Replace with rules in
   `kernel/kernel.mk` so the in-kernel compiler builds with our
   cross-GCC, freestanding flags, and links against the kernel.

## ABI assumptions

`x86.c:21–35` documents the calling convention with exact register
names: `rdi rsi rdx rcx r8 r9 r10 r11 r12 r13 r14 r15` for integer
arguments, `xmm0–xmm15` for floats. **System V AMD64 ABI**, not
Windows x64. Matches the Limine handoff and our higher-half kernel
ABI; no surprise here.

`stack_pointer` is a file-static counter in `x86.c`. That's fine for
single-threaded compile but will need rethinking if we ever
parallelise compilation.

`REGISTERS[]` includes `r10` in the integer-argument list with a
comment about Linux kernel syscalls. Field OS uses SYSCALL/SYSRET,
not `int 0x80` — safe to ignore, but worth keeping a note for when
we have user-mode HolyC programs in M4.

## Build today

```
make            # invokes cmake -S ./src -B ./build && make -C ./build
make install    # installs to /usr/local
make unit-test  # runs CMake's unit-test target
make clean      # rm -rf ./build ./hcc
```

Plus an experimental sqlite3 link-in (commented out in the
`Makefile`). Dependencies on the host: `gcc`, `cmake`, GNU make.
The compiler itself is small — the entire src/ tree is 21k lines.

The `bug-tests/` directory has at least one HolyC reproducer
(`Bug_171.HC`) — useful as smoke seeds when we start cutting code.

## What M3 actually has to do

In rough order, smallest first:

1. **Fork in place.** We're already doing it; this is what M0 step 5
   leaves behind.
2. **Wire it into our cross-GCC build** as a *host transpiler*
   first. `kernel/holyc/holyc-host` builds with cross-GCC, runs on
   the developer host, and produces an x86_64 ELF object we link
   into the kernel image. This is the Path-C "AOT" warm-up and
   gives us the entire HolyC language without solving the JIT
   problem.
3. **Strip the host-assumption surface** in a side branch:
   - Drop `transpiler.c`, `cfg*.c` from the kernel-resident subset.
   - Replace `main.c` with a `holyc_eval(const char *src)` entry.
   - Implement `palloc`/`pfree`-backed `malloc`/`free` shim,
     freestanding `string.h` minimal, `aoStr` allocator-redirect.
   - Provide `printf` via `serial_puts` for diagnostics.
4. **Solve the assembly handoff.** Most likely: bring in a freestanding
   minimal x86_64 assembler (existing OSS options worth scouting:
   `keystone-engine`, `Asmjit`, hand-rolled per OSDev wiki), or
   modify `x86.c` to emit binary directly.
5. **Wire into the kernel's `.text`-allocator** so emitted code is
   placed in W^X-controlled pages (`vmm_remap` flag flip per
   phase-0.md §M3).
6. **The REPL** (phase-0.md §M3 bottom): a five-line interactive
   shell that `holyc_eval`s lines from serial.

If any of (4) or (5) stalls beyond two work weeks, switch to **Path
B** — the Python `holyc2c` transpiler — and ship the PoC. The
plan's risk register §6 (#1) explicitly authorises this fallback.

## What this audit deliberately does not do

- Does not modify any vendored file. The first edit is M3 work.
- Does not benchmark or profile. We've never built it on this host.
- Does not attempt to run the unit tests. M3 will, against a
  freshly stripped subset.
- Does not re-derive the parser grammar. Upstream
  `holyc-lang.com/docs` is canonical for that.

## References

- Upstream README: `holyc/README.md` (vendored).
- Upstream docs: https://holyc-lang.com/
- Phase-0 plan, M3 section: `docs/plan/phase-0.md` "M3 — HolyC
  Runtime on Bare Metal".
- Risk register entry: `docs/plan/phase-0.md` §6 risk #1.
- TempleOS reference (do not vendor): the original `Compiler.HC`
  in TinkerOS / ZealOS source trees.
