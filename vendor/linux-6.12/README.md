# Linux 6.12 LTS — vendored subset (GPLv2)

This directory is the GPLv2-fenced source surface inherited from the
mainline Linux kernel for use under the LinuxKPI shim. Everything
under this directory retains its original SPDX license header
unchanged (typically `GPL-2.0`; occasionally `Dual BSD/GPL` or
`LGPL-2.1`).

The directory-based GPL/BSD-2 boundary is the load-bearing license
invariant ADR-0005 § 4 commits to. `linuxkpi/build.rs` enforces it
at compile time: any `.c` source file outside `vendor/linux-6.12*/`
or `linuxkpi/csrc/` is refused.

## Upstream pin

(M1-2-5 will record the upstream Linux 6.12 LTS tag SHA here when
balloon's compile demands the actual vendored subset. M1-2-4
establishes the directory + the discipline; the `.h` / `.c` files
arrive when they're needed.)

Expected upstream source:
[`linux-6.12.y`](https://git.kernel.org/pub/scm/linux/kernel/git/stable/linux.git/log/?h=linux-6.12.y).

## Vendoring discipline (per ADR-0005 § 3)

- **Verbatim from upstream Linux 6.12 LTS.** Files copied unchanged
  from the upstream `linux-6.12.y` branch at the SHA recorded above.
- **Original SPDX header preserved.** Every `.h` and `.c` file
  retains its upstream `// SPDX-License-Identifier: ...` (or
  `/* SPDX-License-Identifier: ... */`) header without
  modification.
- **Per-new-driver expansion.** When a new inherited driver arrives
  (M1 step 5 amdgpu, step 6 iwlwifi), the kickoff HANDOFF for that
  step includes a `find-include-graph` audit enumerating the
  additional headers to vendor. New headers added in the same
  commit as the inherited driver they support.
- **No local patches.** If a driver needs modification (a hardware
  quirk that QEMU surfaces but Linux upstream hasn't shipped a fix
  for, etc.), the patched copy lives in `vendor/linux-6.12-arsenal/`
  (separate directory), the diff against upstream is documented
  inline in a `MAINTAINERS.md` in that directory, and the patched
  driver replaces the unmodified one in the build graph for that
  specific build. The unmodified upstream copy stays here for
  audit comparison.
- **Quarterly re-pin checklist** (deferred to M1 step 6's HANDOFF
  or sooner if a CVE forces): refresh the SHA, audit the diff
  against the prior pin, re-run the smoke. Upstream LTS releases
  ship security patches; we don't notice unless we re-pin.

## Files

(none yet — see "Upstream pin" above)
