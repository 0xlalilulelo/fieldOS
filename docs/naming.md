# Naming Catalog

> The canonical names for Arsenal system components. Use these names;
> do not invent variants. CLAUDE.md points here as the authoritative
> source. ARSENAL.md (`docs/plan/ARSENAL.md` § "Naming Catalog (Preserved)")
> contains the same names with role descriptions; this file is the
> longer list and the canonical addition / shortlist protocol.

The naming aesthetic is **MGS3-warm and tactical**: short, evocative,
operational, never religious. No Cathedral, Solomon, Covenant,
Tabernacle, Oracle, or biblical references. Ever. The project name
**Arsenal** is itself an MGS reference (Arsenal Gear, Metal Gear Solid 2)
and is consistent with the vocabulary.

## Catalog

### System layer

| Layer | Name |
|---|---|
| Service supervisor (init / systemd-equivalent) | Patrol |
| Compositor / window server (Wayland-protocol-compatible) | Stage |
| IPC primitive (capability-secured) | Channel |
| Per-app sandbox container | Cardboard Box |
| Notification surface | Codec |
| Search / launcher (system-wide) | Radar |
| Command palette (power-user keyboard-driven actions) | CQC |
| System settings | Frequencies |
| Diagnostics / repair (failed-boot first aid) | Cure |
| Recovery environment (bootable rescue partition) | Survival Kit |
| Resource monitor (Activity-Monitor-equivalent) | Stamina |
| Package manager (install / update / remove apps) | Stockpile |
| Logs / observability (journald-equivalent) | Listening Post |
| Identity / auth (user accounts + authentication) | Calling Card |
| Network stack (userland TCP/IP + TLS daemon) | Comm Tower |
| Audio stack (PipeWire-equivalent userland audio server) | Wavelength |
| Graphics API (Vulkan-class) | Foundry |
| Compute API (OpenCL/Metal-equivalent for GPGPU) | Engine |
| Developer overlay (live component graph / IPC / capability tree) | Inspector |

### Apps (M2 onward)

| App | Name |
|---|---|
| File manager | Cache |
| Terminal | Operator |
| IDE | Armory |
| Document viewer / editor (Brief renderer) | Manual |
| Web browser | Recon |
| Mail | Dispatch |
| Contacts | Roster |
| Calendar | Schedule |
| Photos | Negatives |
| Music | Frequency |
| Video player | Projector |
| DAW (v1.0) | Cassette |
| Vector graphics editor (v2.0) | Stencil |
| Video editor / NLE (v2.0) | Sequence |

### Identity / brand

| Surface | Name |
|---|---|
| First-run onboarding | Briefing |
| In-system help | Field Manual |
| Icon set (Lucide ISC fork + custom) | Field Symbols |
| Visual themes | Camo Index |

### Format

| Concept | Name |
|---|---|
| Executable document format (notebook-style; embedded code blocks, hyperlinks, inline macros) | Brief |

The Brief format is generic to Jupyter / Pluto.jl / Quarto / Mathematica;
the word is milspec vocabulary, not religious framing. Manual is the
default Brief renderer / editor app.

## Adding a new name

When a new system component needs a name:

1. Identify the conceptual space — is it a service, a surface, a
   tool, a format, a protocol?
2. Propose **3 to 5 candidates** in MGS3-warm tactical voice. Short.
   Evocative. Operational. Avoid metaphors that drift toward
   ceremony, hierarchy, or religion.
3. Surface the shortlist for selection. **Do not invent a name without
   a shortlist.**
4. Once selected, add it here in the table above. If it is one of the
   names that earns a mention in CLAUDE.md's "names you'll touch most"
   list, update CLAUDE.md too. If it appears in ARSENAL.md's catalog,
   add it there as well in the same commit.

## Rejected names

> If a name was considered and rejected, document it here so the
> reasoning is not re-litigated. Format: `Name — rejected because X.`

- *Field OS* — superseded by *Arsenal* on 2026-05-08; "Field" was
  retained in `Field Manual` and `Field Symbols` because in those
  contexts the word reads as the operational-environment metaphor (a
  field manual is a small printed reference an operator carries; field
  symbols are tactical map markers), not as a project name.
- *Cathedral, Solomon, Covenant, Tabernacle, Oracle* — religious
  framing rejected at project inception. Non-negotiable.
