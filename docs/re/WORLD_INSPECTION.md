# Infestation resident world asset inspection

The headless resource CLI can inspect authored world placements against its
actual resident registrations. This joins H14 placements, H13 constructor
inputs, initial H8 model references and proven H9 sound arguments without executing constructors or
changing residency. Intrinsic decoding stays in `inf-formats`; the
[inspection adapter](../../crates/inf-game/src/world_inspection.rs) reuses
`ResidentRuntime`, whose Group/Select ownership is documented in
[runtime foundation](RUNTIME_FOUNDATION.md).

## Use

From ``:

```sh
cargo run --release -p inf-game -- --data-root retail --mode resident --activate 47,48,11 --inspect-world 0 --json
```

`--inspect-world` names a **Handler-14 global resource slot**, not an OVL
leaf, campaign selection or constructor selector. It requires resident mode.
The example boots the authored tree, visits frontend/loading/world leaves,
then inspects the current world resource at H14 slot 0. Omit `--json` for
placement-to-constructor/model summaries. Difficulty and language remain
explicit CLI selectors, defaulting to the preload header.

The JSON keeps the existing residency summary and ordered transition events,
then adds a `world_assets` snapshot with schema version 5. This version adds
selector-30/35 payload tails and their specialized command operands, including
the shared `00428650` terrain-position inputs. It contains:

- Selected difficulty/language and active leaves.
- Exact H14 source, world header, ordered placements and instance command graph.
- One constructor link per placement, resolving the signed low WORD of its
  resource argument through the current H13 registration table.
- Proven shared constructor fields, damage-response pairs, and initial model
  links to the current H8 winners, including source names and content spans.
- Class-dependent command views using the resolved constructor selector;
  unsupported methods or payloads remain explicitly opaque.
- Derived constructor sound inputs and their current H9 registrations,
  preserving exact PCM/alias/unknown record metadata and source spans.

The graphs and bounded fields are owned by
[world assets](WORLD_ASSETS.md), [constructor assets](CONSTRUCTOR_ASSETS.md)
and [instance assets](INSTANCE_ASSETS.md). Each source includes its OVL path,
overlay key, handler, global slot, local index, region index and load phase.
Spans are absolute offsets in that source OVL. They can be joined to the
lossless `inf-extract worlds` exports by source OVL/handler and local order;
subtract the exported framed-region start to address its raw bytes.

## Registration custody

An independently prepared leaf does not contain every shared or family bank.
A corpus-wide last-writer search also cannot identify the resident winner.
The adapter uses `ResidentRuntime::resource` at every link, and caches parsed
H13 banks only within one immutable snapshot. New snapshots after an unload,
world change or selector reload perform fresh resolution. Missing references
are reported rather than triggering lazy loads or restoring older winners.

All 4,486 retail H14 resource arguments have upper WORD `0xFFFF`; interpreting
the entire DWORD as a signed H13 index would incorrectly reject them. Global
and local indices also differ: H8 slot 195 in `1.ovl` is registered by locals
193 and 254, with local 254 winning; H8 slot 423 in `21.ovl` is registered by
locals 34 and 38, with local 38 winning. The snapshot retains those winners.

The installed H8/H13/H14 banks are base-phase only. Their registration plans
do not change across the thirty installed difficulty/language combinations;
other selected resources still reload through native residency policy.
Tests check the complete world corpus, source-byte links, duplicate local
registrations, world/family transitions, frontend unloading and selector
reloads. The snapshot must leave the resident summary unchanged.

Sound inputs follow the proven derived-class consumers' strictly positive
lookup policy. Zero and negative arguments retain distinct skipped states;
positive inputs resolve only through the current H9 registration winner.
Absent slots stay absent. The report retains the exact sound record, including
PCM spans or an alias's raw target/pitch/volume fields. It does not follow an
alias recursively or substitute present-day registration for the original
loader's pointer-fixup lifetime.

## Coverage and explicit limits

All 4,486 placement references resolve to a constructor resource. Among their
initial model inputs, **4,355 resolve**, **129 use a proven initializer that
does not consume the shared model argument**, and **two are not resident**:

| World OVL | Placement index | H13 slot / source | Constructor selector | Missing initial H8 slot |
| --- | ---: | --- | ---: | ---: |
| `13.ovl` | 28 | 5 / `1.ovl` | 69 | 480 |
| `20.ovl` | 102 | 45 / `1.ovl` | 60 | 390 |

The two H8 slots exist in unrelated worlds, which cannot supply a valid
fallback here. These are observations about initial asset inputs, not proof
that native object creation fails. Later class initialization, instance
commands, lazy callbacks or runtime methods can change visual state.

There are 749 placements whose constructors expose the common derived prefix,
with 2,996 sound inputs: **1,756 zero arguments** and **1,240 resolved positive
arguments**. The resolved records comprise 798 PCM and 442 aliases; none of
these installed inputs is negative or missing. These counts concern authored
references in this family, not every sound an object could play. Tests retain
negative/missing cases separately and prove that inspection never loads them.

The ordered commands now distinguish these two cases. World 20 placement 102
has code 2 selecting H8 slot **577** (resident), code 18 carrying raw DWORD 1,
and code 19 carrying a relocated placement-spawn list. Its initial slot 390
remains reported as absent. World 13 placement 28 has code 17 carrying 8 and
code 16 carrying 700, with no authored model setter. Selector 69's vtable draw
method `00411570` instead has a specialized procedural path using H3 resource
0 and constructed quads. These static consumers explain why initial-model
availability alone cannot classify either object's final visual. They do not
establish that a method executed or that the rendered result is complete.

A negative H13 argument, unsupported constructor profile, proven unused
model argument, exact model `-1` disable argument, other negative argument,
and absent resident slot have distinct report states. Unsupported profiles
must not be presented as proven non-consumption. This initial-model policy
is also distinct from the per-instance model setter's runtime effects.

Commands remain in authored order with exact raw words and spans; the
inspection adapter does not fold them into final state. No terrain-adjusted
position, executed health, animation, callback cadence, lighting, complete
class schema or final rendered model is claimed. Derived class fields,
specialized command tails and runtime-dependent model inputs remain open
under the [asset-readability gate](ASSET_COVERAGE.md).

Shared actor commands 13/14 expose 120 conditional bit-index operands across
the installed worlds. The report retains both their distinct predicates and
whether the two source bytes are available. It does not assume the native
first-world-object guard passed or that its bitset satisfied the predicate;
see the [conditional input contract](INSTANCE_ASSETS.md#conditional-bit-index-inputs).
