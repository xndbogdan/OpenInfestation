# Infestation asset readability

Infestation has complete installed OVL framing, but does **not** yet have
V2000-equivalent asset interpretation and viewing. This document owns that
distinction and the next asset investigations. The
objective keeps targeted asset work active
alongside frontend rendering and executable integration. Unresolved data must
still be established at its consumer before gameplay uses it.

## Reproduce the audit

From ``:

```sh
cargo run --release -p inf-extract -- coverage --data-root retail --summary-only
cargo run --release -p inf-extract -- coverage --data-root retail
cargo run --release -p inf-extract -- fonts --data-root retail
cargo run --release -p inf-extract -- tables --data-root retail
cargo run --release -p inf-extract -- worlds --data-root retail
cargo run --release -p inf-game -- --data-root retail --mode resident --activate 11 --inspect-world 0 --json
cargo run --release -p inf-viewer
```

`coverage` is read-only. It runs every canonical payload parser against the
installed corpus and stops on malformed data with OVL/handler provenance.
The full JSON also enumerates every model variant, classification, known
omission, dynamic dependency and physical occurrence. Its model identity is
the global slot plus exact entry content, excluding file padding. A parsed
entry count is not a semantic-completion percentage.

The corpus has 1,932 OVLs, including 257 nonempty files and 494 active handler
regions. All 158,707,834 input bytes are framed; `PreLoad.dat` is 18,280 bytes
and defines 46 scene leaves. The inventory alone cannot establish whether a
model renders, a field has the correct meaning, or a callback executes.

## Family coverage

| Family | Readable now | Remaining asset boundary |
| --- | --- | --- |
| H0 strings | Original byte strings and registrations | Encoding and label callback context |
| H1/2/6 words | Exact four-byte values and registration | Consumer roles remain incompletely named |
| H3 sprites | Indexed/RGB555 pixels, palettes, selectors, flags; PNG/raw export and direct viewer | Runtime composition and some alpha/backend policies |
| H4 fonts | All five fonts, 1,152 metrics and sprite arguments; typed JSON and exact bytes | Unknown header words retained; glyph drawing is a separate runtime concern |
| H5 display records | Seven five-word records | Packed display-policy fields; runtime HWND must stay distinct from file data |
| H7 materials | All 378 four-byte values; model/terrain associations | Unresolved terrain roles and presentation policies |
| H8 model programs | All 5,142 physical entries, 760 slots and 849 exact slot/content variants; all 354 active auxiliary bytecode variants | Partial static diagnostics, runtime-generated geometry, child assembly, effects, animation inputs and auxiliary callback execution; see the [model census](MODEL_ASSET_COVERAGE.md) |
| H9 sounds | PCM data, WAV export, alias records; [resident frontend cue playback](AUDIO_RUNTIME.md) | Cross-allocation alias lifetime, world sound owners and unrecognized resource kinds remain explicit |
| H10 sample records | All fifteen signed sample grids; 287,700 sample bytes plus 320 bytes of dimensions/metadata, with exact source spans | Live terrain-map sampling and display composition; see [sample assets](SAMPLE_ASSETS.md) |
| H11 terrain lookup | Eight 256-slot raw tables; first-word H8 lookup proven | Remaining second-word consumers; not an established entity-placement format |
| H12 terrain | All 33 grids, 8,650,752 cells, 5,375 sparse records; reversible diagnostic channels and viewer | Complete textured world composition, sparse furniture semantics and lighting |
| H13 constructor resources | All 162 prefixes and dispatch profiles; 155 initial model bindings, 123 shared actor prefixes with 1,230 damage responses, 52 derived prefixes with 208 sound arguments, 32 flags-only prefixes, two terrain-light prefixes, one selector-86 prefix, three selector-83 prefixes and one relocated sixteen-pair table | Further derived class fields and asset references; see [constructor assets](CONSTRUCTOR_ASSETS.md) |
| H14 world resources | All 33 placement graphs, 4,486 instance command lists and 11,476 ordered commands; bounded class-dependent payload views include 120 conditional bit-index inputs | Remaining header fields, specialized command payloads and final coordinate interpretation; see [instance assets](INSTANCE_ASSETS.md) |
| H15 effect selection | Eight five-word raw records; terrain-material selection consumer proven | Downstream effect meanings |

The `fonts` and `worlds` commands publish typed exports in `fonts/` and
`world_assets/`. World manifests retain exact framed bytes, absolute OVL
spans, request mappings and local registration order. Subtract the framed
region start to address its raw export. Constructor-class suffix lengths are
not guessed from neighboring entry pointers, and authored position words are
not silently converted into final terrain-adjusted positions. See
[world assets](WORLD_ASSETS.md) for exact addresses, layouts and limits.

The `tables` command exports H10 signed eight-WORD samples, four-sample masks,
and material indices into `record_tables/`. Dimensions and source spans make
every typed value traceable to its original bytes. The `record_tables`
manifest uses schema version 2; `world_assets` uses version 9, including
instance graphs, damage-response inputs, derived sound arguments and the
specialized point-table view, selector-86 data, selector-11 commands and
selector-30/35 payload tails and command operands.
Initial H13 model bindings are constructor inputs, not final live visuals;
shared health/flags fields do not imply complete class decoding.

The [resident world inspector](WORLD_INSPECTION.md) joins placements to the
actual H13 and initial H8 registration winners and applies bounded command
views using the resolved class. All 4,486 constructor links resolve; two
initial model references remain explicitly absent from their selected worlds.
All 1,240 positive derived sound inputs used by placements resolve to current
H9 registrations; zero inputs remain skipped, and aliases retain their source
records without a claim about playback or native alias lifetime.

`all` regenerates thirteen supported export families; the archival
`opaque_resources` family remains a lossless source copy even for fields now
decoded elsewhere. Its name does not override the typed decoders.

The direct workbench has Models, Sprites and Terrain tabs. Model-local shells,
neutral child composites, explicit-eye geometry and captured draws are separate
evidence tiers. The eye tier resolves three view-plane signs while retaining
literal vector spans and branch decisions; shared-bank surfaces remain open.
The captured tier imports two verified `healthsmall` observations with explicit
projection-field validity; it does not use the legacy runtime-frame schema.
This is not yet a complete actor/type inspection workbench.
The [child compositor](MODEL_COMPOSITION.md) uses the original executable's
sine table and shared native basis arithmetic for proven authored rotations;
missing table data leaves those rotations unavailable.

Outside OVLs, the source `Title.avi` is a 640x480 IV50 video and
cdaudio contains eight canonical disc tracks.
`Infestation.swg` is the shipped SideWinder profile; save/settings state has
separate runtime ownership. The JSON coverage census explicitly excludes
these files and executable tables; it must not be read as a universal media
or save-format certification.

## Asset-first investigation order

1. Recover model presentation from the actual primitive inputs. Keep raw
   projection sources separate from world-coordinate slots, preserve dynamic
   dependencies, and add missing proven primitives/child traversal. Verify
   representative models visually and retain a classification for every variant.
2. Extend the proven H13 shared payloads into derived class fields and decode
   additional H14 command payloads at their exact virtual consumers. Extend
   the resident class/model/placement report with newly proven references,
   preserving initial input, ordered mutation and final runtime state separately.
3. Recover remaining H5/H11/H15 fields; replace historical
   “mission” guesses only where consumers prove the actual role.
4. Supply a bounded observation path for genuinely runtime-generated model
   inputs using the existing recording/inspector. Do not replace unknown
   callbacks with fabricated static values or ask for a new trace before
   checking the accepted recording.
   The [dynamic observation contract](DYNAMIC_MODEL_OBSERVATION.md) now has two
   source-matched `healthsmall` draws with observed callback results, basis,
   projection bytes and trace positions. A canonical importer and the viewer's
   separate Captured Draw view now expose those observations with field validity
   intact. [Parent-import evidence](PARENT_MODEL_OBSERVATION.md) now establishes
   ancestry and writer provenance for a reachable gun assembly, but its source
   fingerprint gate rejects an unobserved auxiliary span. Its separate
   [bytecode grammar](MODEL_AUXILIARY.md) is now decoded and corpus-checked;
   establish source identity before parent presentation;
   the static census and broader asset gate remain open.

Each closure needs a canonical parser/presentation path, retained provenance,
focused malformed/behavior tests where relevant, corpus validation and updated
coverage. Full gameplay execution and backend playback remain separate later
milestones; they are not required just to inspect proven asset data.
