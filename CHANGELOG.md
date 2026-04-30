# Changelog

All notable changes to Field OS are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Semantic
versioning applies once a public release is cut (v0.1 at the end of
Phase 1).

## [Unreleased]

### Added

- Repository scaffolding: directory tree, BSD-2-Clause license,
  README, naming catalog at `docs/naming.md`, ADR template at
  `docs/adrs/0000-template.md`. (M0 step 1)
- Cross-compiler toolchain build script
  (`tools/build-toolchain.sh`) with pinned binutils 2.42 and
  gcc 14.2.0 + SHA-256 hashes; companion CI fetch-script stub
  (`tools/fetch-toolchain.sh`); shared pin file
  (`tools/toolchain.mk`). (M0 step 2)
