# Handler-8 asset coverage

This document owns model asset-tool coverage and the next decoder probes.
Binary layouts remain in [format documentation](FORMAT_DOCUMENTATION.md),
execution ownership in [model runtime](MODEL_RUNTIME.md), and project priority
in the Infestation objective.

## What is readable

The canonical Rust parser reads every installed retail Handler-8 region:
36 OVL regions, 457 sub-blocks, and 5,142 physical registrations. All entries
retain their exact spans, names, auxiliary words, raw vertices, normals,
command records, and local/global identities. No retail command prevents
structural decoding. Some records deliberately retain opaque argument words
or explicit dynamic dependencies rather than unsupported semantic claims.
The lossless extractor writes every raw entry and decoded program, even when
no diagnostic OBJ can be produced.

The separate [auxiliary bytecode](MODEL_AUXILIARY.md) now decodes in all
354 active slot/content variants (2,526 physical registrations), preserving
signed callbacks, child references, branch targets, skipped bytes and padding.
Its 2,859 variant-level instructions are structurally readable; their runtime
policy and callback-table initialization remain separate execution boundaries.
Model export schema 4 includes these programs; coverage schema 2 counts them
independently of polygon diagnostics.

The corpus contains 760 global model slots and 849 distinct
`(global slot, exact content bytes)` variants. Sixteen slots have differing
payloads across OVLs; 744 have one payload. Physical registrations are not
distinct logical models. Deduplication retains every source occurrence and
does not choose a winner across unrelated scene OVLs.

Parsing is substantially ahead of presentation. The current static polygon
diagnostic census, after the branch and projection-source corrections below, is:

| Diagnostic status | Physical registrations | Distinct slot/content variants |
| --- | ---: | ---: |
| Complete polygon mesh | 2,369 | 372 |
| Partial polygon mesh available | 1,765 | 290 |
| Runtime-dependent polygon geometry | 290 | 41 |
| Child-instance-only geometry | 260 | 56 |
| No polygon primitives | 447 | 86 |
| Intentional empty program | 11 | 4 |
| Unresolved nested transform | 0 | 0 |
| Unresolved command topology | 0 | 0 |
| Total | 5,142 | 849 |

`No polygon primitives` includes legitimate effect/line/helper programs and
does not mean corrupt assets. A partial polygon result also does not establish
correct lighting, animation, runtime attachments, or effect rendering.

The viewer's separate neutral compositor proves partial compositions for 105
global roots/content variants, across 697 source occurrences. Twenty-five of these
variants have no available local-shell polygon diagnostic. These are the
results of calling the current compositor for each recorded source occurrence;
they are not complete assemblies or captured runtime poses. Neutral callback
substitution remains explicitly labeled. Native four-WORD child register
inheritance now exposes the five planets and ring child under `titles` 738;
its 13-child assembly remains a neutral diagnostic, with unchanged aggregate
availability. See [child composition](MODEL_COMPOSITION.md#four-word-child-register-inheritance).
The Captured Draw presentation imports
two verified `healthsmall` observations independently of the legacy frame
schema. The viewer has Models, Sprites, and
Terrain tabs; it does not yet provide V2000's runtime-backed entity-type and
full linked-assembly browsing.

## Equal-target conditional records

Native opcode `16`, `004A0C90`, evaluates both packed comparison operands and
returns either `args + jump` or `args + 3`. When the jump word is three, both
paths reach the same following instruction. Unknown operand values therefore
do not make subsequent polygon topology conditional.

The diagnostic previously marked an unresolved comparison whose destination
was not strictly beyond the record as an unresolved topology effect. A corpus
probe found 99 such records in 43 distinct variants: every one had jump three,
and every destination was an existing instruction boundary. They accounted for
all 177 physical entries formerly classified as unresolved command effects.

The correction preserves unknown packed-callback/register provenance, retains
enclosing conditions, and only removes this false topology barrier. It exposes
31 additional physical registrations / nine additional variants as partial
meshes. The remaining affected entries receive their actual geometry/effect
classification. No callback value, animation frame, or raw vertex range is
invented. The focused regression checks all command comparison senses and an
equal-target branch inside a genuinely unresolved outer condition.

## Remaining work and discriminating probes

1. **Separate projected inputs from world slots in model presentation.**
   The two banks are now implemented as described below. Extend evidence for
   still-unresolved projection, custom-vertex, and attachment consumers rather
   than broadening the world-coordinate initialization range. The remaining
   41 unavailable runtime-geometry variants are the next bounded set to inspect.
2. **Follow actual presentation omissions.** Prioritize generated world-slot
   attachment inputs, genuinely guarded child branches, inherited context and
   nonpolygon effects. The `E6` quad family is already typed, emitted by the
   static diagnostic and textured by the viewer. An absent concrete game
   executor opcode does not by itself identify an unreadable asset primitive.
   Preserve material domains, shading, mirror/cull policy and provenance when
   closing a specific presentation gap.
3. **Supply explicit dynamic inputs and attachment owners.** Parent-vertex
   import `2A` and custom callback `2D` each occur in 99 variants; projected
   copy `2C` occurs in 71. Effect opcode `1A` occurs in 67 and spline opcode
   `27` in 32. Recover callback identities and required inputs from the callers
   before claiming a complete assembly or animation. All raw records are
   already preserved, so these are semantic/presentation gaps.
   [Dynamic model observation](DYNAMIC_MODEL_OBSERVATION.md) retains two
   source-matched `healthsmall` 168 draws with executed clock operands and
   native basis/projection outputs. The canonical importer and Captured Draw
   viewer expose both observed screen coordinates and oriented source geometry.
   These separate observations do not change the static census's 41
   runtime-dependent variants or establish complete animation. The first
   [parent-import observation](PARENT_MODEL_OBSERVATION.md) proves ancestry
   and 44 initialized bytes in a copied pair, but remains partial because the
   parent's auxiliary bytecode does not yield a matching source fingerprint.
   Resolve that separate consumer and identity boundary before presentation.
4. **Make coverage repeatable and retain provenance.** Every view/export should
   identify the global slot, exact content variant, source occurrence, and
   available/omitted components. Validate improvements over the entire corpus
   and inspect representative asset families, including nonpolygon programs,
   instead of accepting one attractive model as complete asset support.

The native executor's supported opcode set occurs alone in 113 variants, but
that syntactic count includes empty programs and wrappers whose children may
be unsupported. It is not an execution-success count and must not be used as
the asset-readability acceptance metric.

The concrete runtime executor currently closes the 13-model authored frontend
family. Its missing primitive families occur in 366 unique variants for `E6`,
236 for `D6`, 185 for `DE`, 144 for `D2`, and 109 for `DA`; edge opcode `07`
occurs in 289. These overlapping counts identify later runtime work, not
missing diagnostic polygon families or a count of completed models.

## Projection sources and world-coordinate custody

`004A00E0` calls `004EBFE0` with header boundaries `+10/+14/+18` to project
raw-local inputs, then `004EC0A0` with `+12/+14/+16` to initialize world XYZ.
They write different parts of each 24-byte runtime slot. A projected input is
not evidence that world XYZ is initialized. Native fixed-face consumers such
as `E4` (`0049D650/0049D8E0`) read the projection at slot `+0C`, depth at
`+10`, and clip/fog bytes at `+14/+15`.

The diagnostic now keeps both banks. Polygon extraction reads the proven 3D
source of a projection; vertex calculations and nested attachment origins read
only the world bank. The existing returned geometry remains in root-model
coordinates for the diagnostic orbit renderer, not retail screen pixels.
Mirrored raw sources negate X and retain command/mirror culling provenance.

The command mode pairs independently request world and projection writes.
A world-only mutation leaves the previous face input intact. A projection-only
mutation cannot become an attachment origin or a later world-coordinate source.
An unresolved write clears only the requested bank, so stale projected geometry
cannot survive an unknown callback that overwrites it. Original/mirrored passes
observe earlier writes in native order. Opcode `23` uses wrapping sums and
truncating signed division; `26` uses the recovered signed multiply/shift
sequence with even interpolation steps. Reflection/completion operations retain
the active origin and wrapping arithmetic.

Nested `05` retains separate projection `args[4/6/8]` and world `args[5/6/7]`
ranges. Unknown transforms invalidate their bounded writes, and invalid ranges
fail conservatively. The unrecovered `09` projection remains unavailable.
Restoring a nested origin/basis does not restore either vertex bank.

Two small retail fixtures establish the boundary directly:

| Model | World ranges | Projection geometry | Authored polygon commands |
| --- | --- | --- | --- |
| Global 1 `mark` | `[0,0,0,36]`: empty world range | 18 raw vertices, both mirror parities | 23 `E4` records plus seven mirror instances: 30 quads, 36 referenced vertices |
| Global 4 `ammobox` | `[16,16,16,16]`: empty world range | Eight even raw sources | Three `E4` records: three quads, seven referenced vertices |

Both now have complete authored polygon topology without any callback or world
coordinate guess. `CompleteMesh` means all polygon primitives were recovered;
it does not claim a closed solid, evaluated lighting, exact pixels, or complete
nonpolygon effects. For example, `missile` has 42 proven polygon faces while
its two sprite effects remain explicitly omitted. `beacon`, turret pieces, and
callback-driven geometry retain their remaining omissions.

## Remaining dependency groups and parent-slot imports

The 41 unavailable runtime-geometry variants divide into nine requiring parent
vertex data, two requiring custom vertex callbacks, three camera-plane-dependent
zone signs, and 27 requiring packed callback/register inputs. These categories
are mutually exclusive in that order; some parent-dependent programs also use
callbacks. All three signs (`zonec` 560, `zoneb` 561, `zonea` 562) use opcode
`0B` to select view-dependent faces. Their diagnostics now retain `ViewPlane`
provenance rather than reporting an empty dependency list.

The previous single `NestedProgram` classification was `healthsmall`, global
slot 168 in `1.ovl`. Its `F5` instruction loads register 1 from packed callback
0, then opcode `05` uses angles `[register 1, 0, 0xC000]` to project slots
2 through 170. The nested body is empty; 120 fan triangles appear afterward
in the root program and consume those projections. This is a callback/register
dependency, not an unknown transform format. It now retains both dependencies
under `RuntimeDependentGeometry`. No geometry or callback values were invented.
The installed corpus has no opcode `09`; a synthetic unsupported `09` body
still reports `NestedProgram` where all omitted faces belong to that body.

Among the 56 child-instance-only variants, three require parent imports, 32
carry packed callback/register dependencies, and 21 lack either of those input
classes. Those 21 are useful assembly candidates, not proof that all children,
saved bases, or attachment sites are resolved. Proven authored saved-basis
rotations are handled by the bounded policy below. Unproven attachment sites,
camera bases and inherited dynamic inputs remain separate gaps.

Opcode `2A` is an exception to the ordinary generated-vertex mode pairs.
`004ED350` with a nonnull parent copies the entire 24-byte parent slot whenever
either original mode bit is set, and copies the entire `parent_slot ^ 1` record
whenever either mirrored mode bit is set. Both world and projection custody
change, even for an apparently world-only or projection-only request. With a
null parent, the original origin field is computed first; mirror-only flags can
therefore write the original field before copying it to the mirrored slot.

The isolated diagnostic does not assume either parent condition. It invalidates
the union of those proven writes and retains `ParentVertexSlots`. The runtime
interpreters execute both branches, including a root's origin and projected
zero point ([model runtime](MODEL_RUNTIME.md#runtime-vertices-projection-and-shades)).
In particular,
a world-only parent copy cannot leave a stale face projection, and a
projection-only parent copy cannot leave a stale attachment/world source.
Actual imported geometry requires an explicit child-site snapshot containing
the parent world/projection banks, state-word mapping, current child transform,
and inherited register state. Returned polygon vertices alone are insufficient.

The audit also corrected opcode `F5` to use native `004A03A0` DWORD shift counts
masked by 31 before storing the low WORD. A Rust WORD shift would mask the count
by 15 a second time and select the wrong static branch for shifts 16 through 31.
These custody, dependency, and arithmetic corrections retain the census above;
they strengthen the boundary rather than claiming additional runtime inputs.

### View-plane signs: next inspection path

This is static executable/corpus evidence, not observed execution. The only
three `0B` variants are `zonec` 560, `zoneb` 561 and `zonea` 562, with two
guards each. They have no callback or basis mutation. World initialization
`[24,26,26]` supplies slot 24 from raw vertex 12, `[-128,256,0]`; projection
initialization `[0,26,26]` supplies all thirteen even geometry slots.

Consumer `004A0CE0` sign-extends the vertex-slot WORD at arguments `+2` and
vector-index WORD at `+4`. Its vector is six literal bytes at
`ctx+68 + index*6` (`004A0CFC..004A0D25`). This is **not** the lighting
consumer's `normal_pool + (shade_slot/2)*6 - 6` addressing. H2 publishes the
raw normal-pool base at `004A0188`; lighting does not change that pointer.
All six guards use `[10,24,2]`. Although each entry has one normal
`[0,0,4096]`, index 2 reads into its own command bytes:

| Global slot | `15.ovl` normal base | Consumed six-byte span | Signed vector |
| --- | --- | --- | --- |
| 560 | `6AB3A` | `6AB46..6AB4C` | `[2,226,1372]` |
| 561 | `6AC0A` | `6AC16..6AC1C` | `[2,226,1371]` |
| 562 | `6ACDA` | `6ACE6..6ACEC` | `[2,226,1370]` |

Those words are the first guard's index, following `00E2`, and its texture
ID. Preserve this literal retail access within the entry; neither rejecting
it as `normals[2]` nor substituting the geometrical normal is faithful.
Each signed component undergoes DWORD left shift then arithmetic right shift
by `BYTE[0055F250] & 31`. `00490900` transforms it through the current world
basis at `0055F1D0`, with nearest-even integer conversion. The consumer then
subtracts camera eye `DWORD[DWORD[ctx+58] + 4/8/0C]` from slot-24 world XYZ and
computes the dot product with wrapping signed-DWORD subtract/multiply/add.
It tests the resulting sign, without normalizing the vector.

| Dot sign | `000B` / first quad `[24,10,6,4]` | `010B` / second quad `[22,20,16,18]` |
| --- | --- | --- |
| Nonnegative, including zero | Fall through / selected | Jump / skipped |
| Negative | Jump / skipped | Fall through / selected |

The jump displacement is a signed WORD in **WORD units from the arguments**
(`004A0D78..7D`); fallthrough is arguments+6. Each displacement 10 skips its
one quad. Four subsequent edge commands remain a separate presentation gap.

The canonical diagnostic now takes one `ModelDiagnosticRequest`, with an
unconditioned or explicit-eye view policy. Explicit eyes use root-model
coordinates under a declared zero-origin identity basis, with source bytes and
coordinate shift carried in the same request. The unconditioned asset census
and captured-draw tier remain distinct.
The [viewer orbit](../../crates/inf-viewer/src/camera.rs) is orthographic
and has no finite eye; yaw/pitch alone cannot supply this native input.
The viewer's separate Explicit Eye tier accepts `--inspection-eye X,Y,Z` and
`--eye-shift`, exact-coordinate editing with `E`, and axis step controls.
Its cache includes eye and shift; the overlay retains instruction/vector spans,
dot products and selected branches. Orbit changes do not move this eye.
Unknown world slots, changed/unproved bases, backward jumps or out-of-entry
vector reads retain the unresolved dependency. Missing eye input does not
silently become an origin camera.

The runtime interpreters now execute `0B` too, the common and the late
walk through one [decision](../../crates/inf-game/src/model_execution.rs):
the vector at the signed index, shifted and turned by the current basis
(the model context's, which `0055F1D0` holds), dotted with the vertex
slot's world point minus the view eye. A vector outside the model's source
bytes stops that model. check_view_plane.py
runs `004A0CE0` over 52 cases under identity, quarter-turn, scaled and
random bases, and the Rust comparison matches every turned vector, dot
product and branch. The intro's three zone signs, previously skipped as an
unsupported opcode, now draw.

Isolated execution of original `004A0CE0` and `00490900` checked 4,105 cases,
including signed overflow, full BYTE shift counts and non-Boolean command
high bytes. Canonical tests retain selected native fixtures and all three
original sign sources. Hidden viewer snapshots verify both sides of `zonec`
and selected sides of `zonea/zoneb`; static census behavior is unchanged.
Their textures 1370..1372 are in shared bank `5d*`, outside the viewer's current
source/startup resolver. The geometry is therefore inspectable as unresolved
surface edges. Establish scene/shared-bank custody before claiming filled
sign presentation; this is the next asset boundary for these signs.

REA settles that custody (2026-10-12). Retail binds no
texture to a scene: the face producers resolve a texture through `00405540`,
which reads `slots[id]` of the handler table (count at `+8`, slots at `+4`,
miss callback at `+0`), and the Handler-3 loader `0050EE80` fills that table
through `004EFC00` as each bank loads, relocating variant pointers before
the atlas upload `0050E5B0`. Global slots 1370..1372 are local sprites
82..84 of leaf 5's tier files `5d0.ovl`..`5d4.ovl`, 96x81 in every tier.
Leaf 5 is resident whenever the intro is: leaf 32's Group activates its
inactive siblings (`004EEEB0`), so the intro holds leaves 0, 1, 32 and 5.
`004EE4E0` names each leaf's tier file `overlay\%dd%d.ovl` from
`DAT_0065E72C`, which `00438AD0` sets through `00437C60` to the display's
[resource tier](PRESENTATION_POLICY.md#native-dimensions-and-output) (the
port's `SceneContext::difficulty`); a later tier or language change reloads
the resident files (`004EE160`). So retail draws the three signs with
sprites 82..84 of `5d<tier>.ovl`.

The [viewer](../../crates/inf-viewer/src/catalog.rs) now resolves a
model's global H3/H7 resources over the leaves the retail tree holds with
the model's level, in load order ([`SceneTree`](../../crates/inf-formats/src/scene_tree.rs)
after boot and that level's request; each leaf's four phases in order), not
only the model's own level and the startup leaves. The signs therefore draw
filled from `5d<tier>.ovl` at the scene's tier; a viewer test pins slots
1370..1372 to locals 82..84 of each tier file.

## Authored saved bases in child composition

Opcode `03` (`004A0AE0`) first calls `00490520` at `004A0AF5` to copy the
oriented current basis into context `+0xA0`. The branch at `004A0AFA..004A0B01`
tests the authored angle WORD at `args + 4` before invoking the packed-value
decoder or consulting the axis. A raw zero returns without a rotation call.
External child opcode `0E` then uses `004A14A0`; orientation low bits six copy
all ten saved-basis WORDs into the child's current basis. This establishes an
authored static orientation, without inventing a callback result or matrix.

The compositor now tracks current and saved basis state through proven
`03`/`1C`/`10` operations and evaluates nonzero authored rotations with the
original executable's sine table. Encoded nonzero values that decode to zero
still execute the native rotation/counter path. Raw-zero copying remains
available without that table. The shared arithmetic, counters, state barriers
and before/after decision matrix are owned by
[model composition](MODEL_COMPOSITION.md).

Callback-derived zero remains an explicitly labeled neutral policy. Camera
bases, unsupported `04` basis vectors, escaping nested writes, register aliases,
guarded instances and unproven attachment slots retain their omissions.
Missing original table data never causes generated trigonometry or a skipped
rotation to masquerade as a resolved pose.

Corpus comparison against the prior 77-variant/583-occurrence compositor gives
105 variants/697 occurrences, with no previously available occurrence lost.
Variants without local-shell geometry increase from 19 to 25. The static
polygon census remains separate from child composition.

Matched software-render comparisons cover `turbogroup` 264 (one child becomes
three), `samlauncher` 67 (one newly resolved child), `spawner` 636 (previously
unavailable partial geometry), and `neuralmain` 705 (37 child instances).
Each pair uses the same center, scale and orbit; all available results draw
filled pixels from two views with resolved materials. Vertex, face, material
and omission provenance remains attached. These are partial diagnostic
assemblies, not acceptance of runtime pose, lighting, effects or retail frames.
