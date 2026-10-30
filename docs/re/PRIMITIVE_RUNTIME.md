# Infestation primitive commands and deferred drawing

This document owns the native consumers of model opcodes `01`, `D0`, `D4`, `D8`, `DC`, `DE`, `D6`, `E0`, `E2`, `E6`, `1A`, and `1B`,
their callback packets, and the deferred draw arena. Model traversal, vertex
projection, lighting, and packed operands belong to [model runtime](MODEL_RUNTIME.md).
The [frontend runtime](FRONTEND_RUNTIME.md) owns scene order and renderer selection.

The concrete path is [`ModelCommandBackend`](../../crates/inf-game/src/model_commands.rs)
through [`model_primitives`](../../crates/inf-game/src/model_primitives.rs) into
[`DrawQueue`](../../crates/inf-game/src/draw_queue.rs). It resolves resident H3
sprites and H7 material words, validates the published context/projection identity,
and stores typed callback packets. Queue replay executes supported callbacks
through the concrete CPU pixel backend described below; merely preparing a
packet does not establish visible pixels.

## Shared state, resources, and admission

`PrimitiveState` carries context `+5C` mode, `+54` far-color DWORD, the live
effect-color DWORD at `0055F268`, and the matching projection configuration.
The backend's projection configuration must agree with the model invocation.
Intrinsic resource decoding does not acquire a menu-specific orientation policy.

`PrimitiveTables` reads bounded original executable data: the 256-byte clip
admission table at `00528BD0`, quad U/V tables at `00528EB8/00528EC8`, clipping
projection constants at `00528F18/00528F1C`, and the sine table at `0053DF00`.
Its separate water records at `00537010` belong to
[water rendering](WATER_RENDERING.md), rather than the ground blend lookup.
The clip table is indexed by the OR of the relevant projected clip bytes.
An accepted table entry does not mean that all vertices are inside the viewport.

Projected XY contains two signed 16-bit screen coordinates. The facing helper
`0048FA60` computes `(x1-x0)*(y2-y0) - (y1-y0)*(x2-x0)` with wrapping 32-bit
products/subtraction and tests strictly greater than zero. Degenerate triangles
are not positive faces; mathematical non-overflowing area is not a substitute.
Unknown projection bytes remain unknown until a recovered writer supplies them.

H3 lookup consumes a signed low-word index. Negative indices and indices greater
than pool `+08` return null; an absent accepted slot invokes its lazy loader.
`004EDEA0` allocates `capacity*4` bytes but `00405540` accepts `index==capacity`,
so that exact one-past boundary remains an explicit Rust lookup failure rather
than fabricated memory or a null return. A resident-only owner must preserve
the missing-loader boundary. H7 flat material paths
dereference the returned word, so they cannot treat a missing material as zero.
Resource identities in packets borrow the resident entry and provenance; global
slot numbers and source file offsets are not native texture addresses.

## Opcode 01: triangle fans

[`fan.rs`](../../crates/inf-game/src/model_primitives/fan.rs) implements near
`004966F0` and far `00496C90`. Material lookup occurs once before either mirror
pass, shade modulation, facing tests, or clip admission. Flag `08` selects the
textured vertex record; otherwise the material is a full H7 word.

Vertex and shade references are **unsigned** words, then XORed with the mirror
index. This differs from E2's signed references. Flag `10` enables mirror 1
before mirror 0. The remaining low flags control the fan's facing tests:

| Flag | Native effect |
| --- | --- |
| `01` | Reverse the facing predicate. |
| `02` | Test the first triangle, then clear this bit. A rejection ends this pass. |
| `04` | Test every triangle. With original flag `02`, any later rejection also ends the pass. |

The current corner's shade modulation runs before facing rejection. Helper
`004EBDE0` multiplies each shade RGB byte by its corresponding authored byte,
shifts right seven, and clamps to 255. The three temporary color DWORDs have
only their RGB bytes initialized; their fourth bytes must not become invented
alpha or zero. Textured U/V words are zero-extended into packet DWORDs.

After facing acceptance, the clip-table decision precedes the first vertex's
depth read and the renderer sorting query. These handlers do **not** invoke
geometric clipping: an admitted triangle preserves its original projected
vertices. The queued renderer callback owns any subsequent viewport clipping.
Material sorting is queried for every admitted triangle, including null texture.
The first vertex supplies its sort depth; fan triangulation advances the previous
corner even when a triangle fails the clip lookup.

## Opcode E2: textured quads and the near-plane fallback

[`quad.rs`](../../crates/inf-game/src/model_primitives/quad.rs) implements near
`0049DBE0` and far `0049DE90`. Command bit `100` enables facing, `200` reverses
it, and `400` selects mirror 1 then 0. Vertex references are signed words XORed
with parity. Material lookup follows facing and clip admission; the far path
also rejects when all four fog bytes are 255, reading them in vertex order.
The ordinary packet has no shade field or explicit U/V coordinates.

On rejection, the mixed near-plane path remains eligible when combined clip
bits include both `40` and `80`, but not every vertex has bit `40`. It calls
`00498B10` near or `00499130` far, then clipper `00497250`.
This fallback deliberately reads four signed shade references at instruction
byte offsets `+0C/+0E/+10/+12`: the last vertex word and bytes following the
nominal E2 record. The canonical source slice must back these reads. Replacing
them with E2's nominal shared-shade operand changes the original consumer.
Those references are not bounded by the model's declared shade count in the
original. `004EBE60` places vertices and shades in a fixed `FA000`-byte arena;
the [scratch ownership contract](MODEL_RUNTIME.md#native-scratch-allocation)
tracks nested allocation offsets and balanced restoration. A full DWORD read
outside the declared shades but inside that arena is mapped stale data. The
port preserves its value as unknown, rather than manufacturing an initialized
color or rejecting a valid mapped read. Reads outside the arena remain errors.

The actual first-entry camera reaches this branch at frame 134 of a fixed
16,000-microsecond, 640×480 variant-2 run. Root model 745, source instruction
`1E0286`, references vertices `0/2/12/10` and sprite 1813. Its 22 vertices and
two declared shades place the shade base at arena offset 528; the four literal
references `10/226/1814/0` read offsets `40/904/7256/0` from that base, all
mapped. This supersedes the earlier claim that authored menu distances never
straddle the near plane. The native clipped result is outside the viewport and
emits no packet; that result comes from clipping/admission, not a skipped draw.

[`clipping.rs`](../../crates/inf-game/src/model_primitives/clipping.rs) traverses
edges in source order, starting with the last-to-first edge. A bit-40 crossing
emits its intersection before a retained current vertex. A current bit-40
vertex without bit 80 aborts the entire polygon at that point. Geometry reads
follow this traversal rather than requiring unused later projections first.

For a crossing, the front endpoint is reconstructed using wrapping 32-bit
products and signed `IDIV`, which truncates toward zero. Behind XY already holds
view coordinates. Helper `0048EF30` computes the saturated signed Q31 ratio
`(behind_z-65)/(behind_z-front_z)`; negative ratios clamp to zero. Interpolation
uses signed 64-bit products and their bits 31..62, preserving wrapping adds.
The result stores depth **64**, fog byte **0**, and uses the original fixed
projection constants with retained 53-bit x87 arithmetic and `FISTP QWORD`.
This deliberate 65-versus-64 distinction must not be normalized away.

Projection reduces X and Y together by arithmetic shifts while the signed
OR of their wrapping absolute values is at least 32767. Clip classification
uses unsigned viewport comparisons followed by signed lower-side tests.
The scratch record also copies/interpolates RGB, although these textured UV
callback packets do not serialize those colors. Both allocated unknown RGB and
mapped stale arena words propagate as unknown without blocking these callbacks.
Live RGB consumers such as opcode 01 retain strict shade reads. Unknown bytes
are not cleared. An isolated original-PE differential varied all four shade
DWORDs across 128 assignments each for the frame-134 geometry and a visible
five-vertex straddle: all output packets remained identical (zero and two
packets respectively). Two synthetic receipts are retained in
`tests/fixtures/e2_scratch_native.json` and compared against the Rust producer.

The fallback retests facing against the first three output vertices. Three
vertices emit one UV triangle; four emit one UV quad; five emit a quad using
0/1/2/3 followed by a triangle using 0/3/4. Each output performs its own clip
lookup, material lookup, and sorting query. Other output counts emit nothing.
Retained fog bytes stay unchanged; only new intersections receive zero fog.

## Opcode E6: shared shade quads and clipped UV

[`e6.rs`](../../crates/inf-game/src/model_primitives/e6.rs) owns direct
Near `0049E1A0` and Far `0049E470`. Facing precedes clip-table admission;
Far rejects all-four fog255 only on the direct path. Mirror mode walks1 then0.
H3 lookup precedes the renderer sorting query and allocation. A native null H3
entry bypasses texture-based key grouping, but still queries sorting and emits
the effect-quad packet. `0049E2CB` reaches Near allocation `0049E31B`;
Far follows `0049E5C3` to `0049E613`. The earlier null-as-rejection interpretation
was wrong. Required lazy-loader/registration failures remain explicit services.

Direct packets retain plain mode at+20, H3 relocation at+24 and the whole shared
shade DWORD at+28. Near uses EffectQuadNear/three blocks; Far uses
EffectQuadFar/four blocks, adding context far color at+2C and vertex fog at+30.
Native shade lookup is signed WORD XOR mirror through the
[scratch address contract](MODEL_RUNTIME.md#native-scratch-allocation).
Known RGB/high bytes are copied; unknown bytes remain unknown until a reached
consumer needs them. Byte+2B is not read by the effect consumer. It must not be
filled with a captured zero or used to reject known RGB pixels.

Mixed-plane fallback has separate entry points: Near `0049A270`, Far
`0049A930`. Both use `00497250` with four literal signed shade WORDs starting
at instruction+0C and continuing into following bytes. Their geometry, UV,
scratch RGB interpolation, source-edge order and 65-versus-64 projection are
the same authenticated clipper used by E2 and D6. DE supplies three sources
to the same shared-shade wrappers; E6 supplies four. E6 retains its separate emission
policy: output shade is the nominal shared shade, independent from the four
copied/interpolated colors. Unknown scratch RGB therefore does not lose an
otherwise known output input.

After clipping, facing is retested against the first three output vertices.
Counts3/4 emit a triangle/quad; count5 emits quad0/1/2/3 then triangle0/3/4.
Each output independently tests clip admission, resolves H3, queries sorting
and allocates. Unlike the direct Far path, the Far fallback has no all-fog255
gate. Earlier output packets remain queued if the later split output fails.

| Shaded UV output | Deferred callback | Renderer slot | Blocks | Shade / UV / Far color / fog |
| --- | --- | --- | --- | --- |
| Near triangle | `004A3A10` | `+74` |4| `20 / 24 / - / -` |
| Far triangle | `004A3A50` | `+78` |5| `20 / 24 / 3C / 40` |
| Near quad | `004A38D0` | `+BC` |5| `28 / 2C / - / -` |
| Far quad | `004A3910` | `+C0` |6| `28 / 2C / 4C / 50` |

Mode and H3 offsets remain18/1C for triangles and20/24 for quads. These four
callbacks are distinct from E2's white UV callbacks and direct effect quads.
The [pixel owner](PIXEL_RUNTIME.md#shared-shade-uv-primitives) retains their
own packet, RGB/fog and device policies while sharing proven final TL/raster
data. Neither intrinsic decoding nor submission consults the mission selection.

Original-instruction checkers
check_e6.py and
check_e6_clipping.py reproduce
[152 direct receipts](../../crates/inf-game/tests/fixtures/e6_native.json) and
[94 clipped receipts](../../crates/inf-game/tests/fixtures/e6_clipping_native.json).
The Rust [direct comparisons](../../crates/inf-game/src/model_primitives/e6_tests.rs)
and [clipped comparisons](../../crates/inf-game/src/model_primitives/shared_shade_clipping_tests.rs)
check packet masks, actual scratch geometry/RGB/UV, mirror/cull decisions,
signed references, nullable/lazy H3 and split-output failure prefixes. The
checkers execute unchanged producer, clipper and sorting instructions; source
slots, resource residency and terminal allocations are explicit supplied hosts.
They do not establish complete allocation-fault unwinding or retail pixels.

Reproduce from the repository root with Python 3 and Unicorn 2:

```powershell
python check_e6.py --output .tmp/e6-native.json
python check_e6_clipping.py --output .tmp/e6-clipping-native.json
```

Both checkers reject an unsupported executable hash. Temporary outputs and the
original executable remain local; compare the outputs with the tracked fixtures.

The [shared-shade wrapper](../../crates/inf-game/src/model_primitives/shared_shade_clipping.rs)
carries explicit triangle/quad source data. The DE/D6 source contracts below
retain their distinct shade and packet ownership.
Completing a reached E6 call can still produce no visible fragments; removal
of an omission is not evidence of a visible improvement or complete furniture
acceptance.

## Opcode DC: material shared-shade triangles

DC (`0049C280` near, `0049C4B0` far) is E4's triangle sibling. Its record is
`[material, shade, v0, v1, v2]`. The handlers run the same mirror loop, the
`0048FA60` area cull for command bit 8 (bit 9 flips it), the `00528BD0` clip
gate and, Far only, the all-fogged skip. They look up the Handler-7 material
inclusively through context `+4C`, loading an empty entry lazily, and store
its first DWORD. A null material is dereferenced, so the port propagates the
host error. Sorting follows the flat rule. Three blocks go to `004A42B0` under
`004A3950` (renderer `+44`) or `004A3980` (`+48`): the corner XY/depth pairs,
mode `+18`, material `+1C`, shade DWORD `+20`, then for Far the far color
`+24` and fog bytes `+28..+2A`. The rest stays unwritten.

The consumers `004C03E0` and `004C0A30` are instruction-for-instruction the
flat triangle's `004C12F0`/`004C1940`, apart from their static TL buffers. They
read the one shade DWORD for every corner, and Far reads its color and fog at
DC's offsets. The software frame therefore feeds DC through the flat
preparation. A DC triangle that crosses the near plane goes to the
[clipped material wrapper](#clipped-dc-and-e4-faces), which E4 shares.
check_dc.py runs both producers, with the native wrapper,
over 200 cases (193 packets; eight cases call the wrapper). The
[Rust comparison](../../crates/inf-game/src/model_primitives/dc_native_tests.rs)
matches every gate, lazy load, sort key and packet byte.

## Clipped DC and E4 faces

A DC triangle or E4 quad whose clip bytes OR to `C0`, without `40` on every
corner, goes to `00498060` (near) or `00498570` (far). The caller first loads
one shade DWORD per corner through the signed WORDs after its record (DC
record `+08`, E4 `+0A`), XORed with the mirror. These feed only the clipper's
scratch RGB, which neither wrapper reads back. The wrapper clips with
`00497250`. For command bit 8 it culls the first three results with
`0048FA60`, flipped by bit 9 and the mirror. Three results queue a DC
triangle, four an E4 quad, and five the quad `0/1/2/3` and then the triangle
`0/3/4`; other counts queue nothing. Each packet passes the `00528BD0` gate on
its own corners, looks up the Handler-7 material inclusively and takes the
flat sort key. It stores the direct handlers' layout: corners, mode,
material, the nominal shade DWORD and, Far only, the far color and the
clipped corners' fog bytes. Neither wrapper has Far's all-fogged gate.
Infest01 never calls either wrapper in the intro window (TTD execution counts
of both entries are zero).

check_material_clipping.py runs both
wrappers over 236 cases: every clip count, culls, five-vertex results where
only one packet passes the gate, lazy and null materials, division faults and
each failing host call. The
[Rust comparison](../../crates/inf-game/src/model_primitives/material_clipping_tests.rs)
matches every event, error and packet byte.

## Material faces D0, D4, D8 and E0

Each face family's first opcode takes a Handler-7 material where its siblings
take a texture or a shade (Near table `00540DC0`, Far `005411C0`):

| Opcode | Near / far handler | Its sibling, less | Direct callback, near / far | Blocks |
| --- | --- | --- | --- | --- |
| D0 triangle | `0049E790` / `0049E9F0` | D2's texture | flat triangle `004A39B0` / `004A39E0` | 3 / 4 |
| D4 quad | `0049F1E0` / `0049F4C0` | D6's texture | colored quad `0044EA80` / `0044EA90` | 4 / 4 |
| D8 triangle | `0049BE10` / `0049C020` | DC's shade | solid triangle `004A38E0` / `004A3920` | 2 / 3 |
| E0 quad | `0049D110` / `0049D380` | E4's shade | solid quad `00466170` / `00466180` | 3 / 3 |

All four run their sibling's mirror loop, `0048FA60` cull on the first three
corners, `00528BD0` clip gate and, Far only, the all-fogged gate. They look the
material up inclusively through context `+4C` and take the flat sort key. D0
and D4 copy each corner's whole shade DWORD (the record's shade WORDs,
sign-extended and XORed with the mirror) after the material, D0 at `20` with
Far color/fog at `2C/30`, D4 at `28` with `38/3C`. D8 and E0 stop at the
material: D8's Far color/fog sit at `20/24`, E0's at `28/2C`. D4 looks the
material up twice: once before sorting without reading it (`00405540` over
`+4C` near, the same lookup inline far), then after allocation for the packet.
D4's packet is terrain's colored quad (`0044D030`), so the two share
`PrimitiveCallback::ColoredQuad*`; D0's is the flat triangle the `01` fan
queues.

A face that crosses the near plane goes to a wrapper with the shared clipper
`00497250`. D8 and E0 call `004976A0`/`00497B30` (calls at `0049BFFC`,
`0049C259`, `0049D360`, `0049D632`), which are `00498060`/`00498570` without the
shade DWORD: they queue D8's triangle (count 3) and E0's quad (count 4), with
the callers' literal shade loads at record `+08`/`+0A` as DC and E4 have. D0 and
D4 call `004997E0`/`00499CE0` (`0049E9C8`, `0049EC66`, `0049F49D`,
`0049F7D9`) with the corners' shade DWORDs: three results queue the flat
triangle, four the colored quad, five the quad `0/1/2/3` and the triangle
`0/3/4`, each corner carrying the clipper's color DWORD. A retained corner
copies its whole source DWORD; an intersection writes only RGB, so its fourth
byte is the stack's and stays unknown. The material dereferences sit at
`0049778A`/`0049787F`/`00497997`/`00497AA1` and `00497C1A`/`00497D2E`/
`00497E5E`/`00497F98` for the flat wrappers, `004998CA`/`004999D8`/`00499B09`/
`00499C35` and `00499DCA`/`00499EFA`/`0049A043`/`0049A199` for the per-corner
ones.

Before these producers the model executor stopped at the face with
`Unsupported`: the object draw omitted the rest of the model, the database
and briefings the rest of the preview. Retail models with these faces are
the Nemian (308, D4), Ichneu and Power Ichneu (597, 277, D0), the four
cicadas (498, 501, 504, 507), `gateopsspin` (582) and the shelter (605) for D4,
the four splodges (717..720) and markers (731..734) for D8, and 26 for E0:
ferns, flowers, dead cicadas, brome, `grock` (603, 683, 684), `smstore`
(378), `rhinodead` (311), `deadmstone` (712) and the frontend's `planetsun`
(758).

check_flat_material.py and
check_material_per_corner.py run the
direct handlers over 400 cases each (flags, mirror, cull, lazy materials,
sort modes, the Far gates, clipped fallbacks); the
[D8/E0](../../crates/inf-game/src/model_primitives/flat_material_tests.rs) and
[D0/D4](../../crates/inf-game/src/model_primitives/material_per_corner_tests.rs)
comparisons match every event and packet byte. D4's second lookup follows the
native allocation; the port's complete packets submit after it, the boundary
D6 also has. check_flat_material_clipping.py
and check_material_per_corner_clipping.py
run the wrappers over check_material_clipping's 236 cases each, matched by
[material_clipping_tests](../../crates/inf-game/src/model_primitives/material_clipping_tests.rs)
and
[material_per_corner_clipping_tests](../../crates/inf-game/src/model_primitives/material_per_corner_clipping_tests.rs).

## DE and D6 source polygons

The original `00497250` geometry owner now accepts explicit triangle or quad
inputs through [`ClipSource`](../../crates/inf-game/src/model_primitives/clipping.rs).
It retains the source-edge walk, projection constants and first N entries of
the original quad UV tables `00528EB8/00528EC8`. In particular, a DE triangle
uses the first three **quad** UV rows; a new triangle UV assignment would change
the original texture. DA, E2, DE, E6, D2 and D6 share this proven geometry data shape
while retaining their own packet emitters.

| Caller | Near / Far fallback | Source vertices | Scratch shade WORD offsets from instruction | Emitted color |
|---|---|---:|---|---|
| DA `0049C710/0049C960` | `00498B10/00499130` | 3 | `+0A/+0C/+0E` | No shade field |
| E2 `0049DBE0/0049DE90` | `00498B10/00499130` | 4 | `+0C/+0E/+10/+12` | No shade field |
| DE `0049CC10/0049CE70` | `0049A270/0049A930` | 3 | `+0A/+0C/+0E` | Independent nominal shared shade |
| E6 `0049E1A0/0049E470` | `0049A270/0049A930` | 4 | `+0C/+0E/+10/+12` | Independent nominal shared shade |
| D2 `0049EC80/0049EF10` | `0049B070/0049B6F0` | 3 | Three signed nominal corner-shade references | Copied/interpolated corner RGB |
| D6 `0049F800/0049FB10` | `0049B070/0049B6F0` | 4 | `+0C/+0E/+10/+12` | Copied/interpolated corner RGB |

DE's literal scratch reads start with its last vertex WORD and cross into the
following instruction. The emitted nominal shade remains argument WORD1.
The caller publishes both the literal source offset and nominal shade explicitly.
No helper infers that policy from a numeric mode or mission identifier.

Direct DE also retains the native nullable H3 packet after its sorting query.
Its signed nominal shade reads use the proven arena address contract; unknown
RGB/high bytes remain unknown in the queued packet. Direct D6 performs two H3
lookups: the first chooses its sorting key, and the second supplies the packet
texture. Native reaches the second lookup after allocation and XY/mode stores
(`0049FA12/0049FD5D`). The portable complete-packet submit API does not reproduce
allocator-side partial packets on a later read failure; its earlier complete
packets and required lookup failures retain their existing containment scope.

[`per_corner_clipping`](../../crates/inf-game/src/model_primitives/per_corner_clipping.rs)
serializes the clipper's corner shades and explicit UVs. Retained vertices copy
their supplied shade high byte; intersections write only RGB, leaving high
unknown. Unknown RGB remains unknown until a reached pixel consumer requires it.
Counts3/4 emit one triangle/quad; count5 emits quad0123 then triangle034.
Each output retains its own clip-table, H3, sorting and allocation boundary.
Neither Far wrapper adds the direct path's all-fog255 gate.

| D6 clipped output | Deferred callback | Renderer / Direct3D consumer | Blocks | RGB / UV / Far color / fog |
|---|---|---|---:|---|
| Near triangle | `004A38F0` | `+7C` / `004CE930` | 5 | `20 / 2C / - / -` |
| Far triangle | `0044EA50` | `+80` / `004D0170` | 5 | `20 / 2C / 44 / 48` |
| Near quad | `004A3940` | `+C4` / `004E6B70` | 6 | `28 / 38 / - / -` |
| Far quad | `004A3970` | `+C8` / `004E8D30` | 6 | `28 / 38 / 58 / 5C` |

The new quad packets retain explicit signed 16.16 allocation coordinates and
four independent shade DWORDs. They do not reuse direct D6's implicit corner UVs.
The [pixel owner](PIXEL_RUNTIME.md#per-corner-shaded-uv-quads) retains the primary
draw prefix before the separate secondary-pass boundary.

The authenticated original-instruction checks
check_de.py, check_de_clipping.py,
check_d6.py and check_d6_clipping.py
reproduce [152 direct DE](../../crates/inf-game/tests/fixtures/de_native.json),
[96 clipped DE](../../crates/inf-game/tests/fixtures/de_clipping_native.json),
[160 direct D6](../../crates/inf-game/tests/fixtures/d6_native.json) and
[100 clipped D6](../../crates/inf-game/tests/fixtures/d6_clipping_native.json)
receipts. Rust compares packet masks, signed scratch reads, actual clipper
geometry/colors/UVs, mirror/cull/sorting decisions and failure prefixes in
[DE tests](../../crates/inf-game/src/model_primitives/de_native_tests.rs),
[D6 direct tests](../../crates/inf-game/src/model_primitives/d6_direct_tests.rs) and
[D6 clipping tests](../../crates/inf-game/src/model_primitives/per_corner_clipping_tests.rs).
The D6 clipped set contains 94 actual caller-wrapper cases and six separately
supplied varied-color clipper/emitter cases. Geometry, clipping and sorting run
unchanged original instructions; source slots, texture residency and terminal
allocations are supplied hosts. Complete native allocator-fault unwinding and
matched retail pixels remain outside these checks.

Reproduce from the repository root with Python 3 and Unicorn 2:

```powershell
python check_de.py --output .tmp/de-native.json
python check_de_clipping.py --output .tmp/de-clipping-native.json
python check_d6.py --output .tmp/d6-native.json
python check_d6_clipping.py --output .tmp/d6-clipping-native.json
```

DA and D2 now retain their own direct and count-three clipped contracts.
Direct DA's null-H3 paths query sorting at `0049C81B/0049CA81` and allocate at
`0049C892/0049CAFA`; the adapter preserves those nullable packets. Direct D2
likewise performs one nullable H3 lookup before sorting/allocation, then copies
three signed mirror-indexed shade DWORDs, preserving unknown bytes. Neither
borrows direct D6's second lookup. DA's nominal shared shade remains unread.

[`uv_only_clipping`](../../crates/inf-game/src/model_primitives/uv_only_clipping.rs)
owns DA/E2's common wrapper, including DA's literal reads across its nominal
record; copied/interpolated scratch RGB never enters these packets. D2 uses
the per-corner wrapper with three explicit source shades. Both preserve the
original quad UV rows, per-output admission/sorting and completed-packet prefix.
The complete-packet host still cannot reproduce a partial native allocation
when a later payload read fails.

check_da.py, check_d2.py,
check_d2_clipping.py and
check_uv_only_clipping.py reproduce
[152 direct DA](../../crates/inf-game/tests/fixtures/da_native.json),
[154 direct D2](../../crates/inf-game/tests/fixtures/d2_native.json),
[98 clipped D2](../../crates/inf-game/tests/fixtures/d2_clipping_native.json) and
[182 DA/E2 clipped](../../crates/inf-game/tests/fixtures/uv_only_clipping_native.json)
receipts. They authenticate the original executable and execute its producers
with supplied source storage, texture registrations and terminal hosts. Run each
with Python 3/Unicorn 2 and `--output .tmp/<checker>-native.json`; compare against
the corresponding tracked fixture. These bounded instruction comparisons do
not establish complete allocator-fault parity, secondary passes or retail pixels.

## Opcode 1A: sprite ground quads

[`ground.rs`](../../crates/inf-game/src/model_primitives/ground.rs) owns
`004A1D50` Near and `004A2F00` Far. Both resolve H3 before the signed vertex
reference and clip40 gate. Far then rejects source fog255 before evaluating
packed size and angle; Near does not test source fog. The two packed callbacks
retain their native order and completed register mutations on failure.

The complete corner expansion is shared with `1B` only after original
instructions establish equivalence. `2*size/depth` remains at53-bit x87
precision before the two focal products and nearest-even integer stores.
The saturation loop compares signed `x|y` against32767 and uses arithmetic
right shifts before narrowing to WORDs. Negative or wrapped extents must not
be replaced with unsigned shifts or absolute magnitudes. The four Q31
products retain separate X/Y focal ownership. These source corrections also
repair the earlier Near implementation for unequal or negative focal values.
The corner clip is `1B`'s too. Both compare each corner with the published
viewport `0055F234/38` (`004A1F7E/004A1FC4`, `004A241E/004A2464`). The
frame setup publishes it with the focal and centre, as six DWORDs from
`0055F22C` (`00483D87`). An unsigned value below the bound codes as inside (2 or 0x10);
otherwise it codes as negative (1 or 8) or beyond (4 or 0x20). Production once
fed `1A` a separate "fog bound" that no caller supplied, so every ground
sprite was rejected. A seeded Infest01 comparison found twelve model-79
sprites missing from one intro frame, and they now draw.

After the corner gate, the native renderer virtual `+04` (`004EB490`) admits
material sorting. The existing first-selector/flags policy and texture identity
owner then determine the key. Both ground packets reserve three16-byte blocks
and use the existing ordinary textured-quad callbacks `00466150/00466160`
(renderer `+9C/+A0`). Mode is at `+20`, H3 relocation at `+24`. Far writes
context `+54` at packet `+28` and repeats the source fog byte at `+2C..+2F`;
Near leaves that tail unwritten. There is no effect-color DWORD in this packet.

The labeled `mode-from-scene-state` channel remains explicit. `0049FEF0`
copies outer drawing-context `+70` to invocation `+5C` at `004A00AB..004A00B5`.
World location `00546EF8` starts image-zero; frontend template `00547558`
starts loader-zero and `0043B581..0043B587` copies it into each local context.
These facts prove initialization and copy custody, not every indirect writer
or a universal mode0 policy. Current furniture packed callbacks, resident H3
resolution and the sorting query do not mutate this captured context.

The original-instruction checker produces the
[96-case receipt](../../crates/inf-game/tests/fixtures/ground_quad_native.json)
compared by [Rust](../../crates/inf-game/src/model_primitives/ground_native_tests.rs).
It executes both `1A` dispatches, both `1B` dispatches, original rotation and
`004ED4B0/004EB490`, with supplied H3, packed-callback and allocation services.
Cases include signed/overflow geometry, unequal focals, threshold32767,
source fog, descriptor selectors/flags, both sorting gates and failure prefixes.
Allocation is a required host, not a second proof of the arena. Reproduce with
`python check_ground_quad.py --output <local.json>`
using Unicorn and compare against the versioned receipt.

Unknown vertex state, null dimensions, zero maximum dimension and depth below64
without clip40 retain explicit Rust containment. Portable texture allocation
ordering, unchecked mode writers and matched whole-scene pixels stay separate
bounds. Clipped `E6` uses its independent shaded UV producer described above.

## Opcode 1B: rotated sprite effects

[`effect.rs`](../../crates/inf-game/src/model_primitives/effect.rs) implements
`004A21F0` near and `004A33C0` far. It resolves H3 before testing the signed
vertex reference, bit-40 visibility, or far fog. Packed size is evaluated before
packed angle, with mutations visible through the live `ModelValues` callback.

For admitted depth at least 64, `2*size/depth` remains at 53-bit precision before
each focal multiplication and integer conversion. The native signed extent
reduction precedes the low-word stores. Dimensions come from the sprite's first
physical descriptor, independently of the mode-selected batching descriptor.
Rotation uses the original sine table, signed division by the larger dimension,
and Q31 constant `5A827999`; screen corners preserve native wrapping arithmetic.
The resulting corner clip OR gates material sorting and packet emission.

The effect color getter `00490350` supplies the **whole DWORD** at `0055F268`.
Far packets append the context far-color word and repeat the source fog byte
for all four corners. A null sprite at the dimensions dereference, zero maximum
dimension, or depth below 64 without bit 40 is an explicit unsupported/failure
boundary. The last case reads uninitialized native stack extents; the port does
not manufacture a quad from zeros. Earlier lookup and packed-value effects remain.
The extents are `WORD [EBP-1Ch]` (X, written by neither near producer) and
`WORD [EBP-1Ah]` (Y, written only by their own far path at `004A1E0C`/`004A22AC`),
so retail draws stack residue; see the
[opcode-07 note](GAMEPLAY_RUNTIME.md#control-owner-decomposition-from-the-opdetailprobe-census).

## Packet layouts and sorting keys

Every packet reserves the native number of 16-byte payload blocks, in addition
to its 16-byte callback header. Header DWORDs `+04/+08/+0C` are initialized to
zero by these producers. Payload padding and uninitialized color bytes remain
`None`; a non-null texture pointer becomes a borrowed resource relocation at
its exact payload offset. Native null is a known zero DWORD.

| Producer | Near / far callback | Payload blocks, near / far |
| --- | --- | --- |
| 1A ground quad | `00466150` / `00466160` | 3 / 3 |
| 01 textured triangle | `004A38F0` / `0044EA50` | 5 / 5 |
| 01 flat triangle | `004A39B0` / `004A39E0` | 3 / 4 |
| D0 per-corner material triangle | `004A39B0` / `004A39E0` | 3 / 4 |
| D4 per-corner material quad | `0044EA80` / `0044EA90` | 4 / 4 |
| D8 material triangle | `004A38E0` / `004A3920` | 2 / 3 |
| E0 material quad | `00466170` / `00466180` | 3 / 3 |
| DE shared-shade textured triangle | `004A38C0` / `004A3900` | 3 / 3 |
| DC material shared-shade triangle | `004A3950` / `004A3980` | 3 / 3 |
| DE clipped shared-shade UV triangle/quad | Same shaded UV callbacks as E6 below | 4/5 / 5/6 |
| D6 direct per-corner quad | `0044EA60` / `0044EA70` | 4 / 4 |
| D6 clipped per-corner UV triangle | `004A38F0` / `0044EA50` | 5 / 5 |
| D6 clipped per-corner UV quad | `004A3940` / `004A3970` | 6 / 6 |
| E2 ordinary quad | `00466150` / `00466160` | 3 / 3 |
| E2 clipped UV triangle | `004A3990` / `004A39C0` | 4 / 4 |
| E2 clipped UV quad | `004A3A20` / `004A3A60` | 5 / 5 |
| E6 direct shared-shade quad | `0040EF30` / `00411D50` | 3 / 4 |
| E6 clipped shared-shade UV triangle | `004A3A10` / `004A3A50` | 4 / 5 |
| E6 clipped shared-shade UV quad | `004A38D0` / `004A3910` | 5 / 6 |
| 1B effect quad | `0040EF30` / `00411D50` | 3 / 4 |
| Selector67 ribbon `00486B30` | `0040EF30` / `0044EA70` (D6 far) | 4 / 4 |
| Frontend star solid quad | `00466170` / none | 3 / none |

Packets start with interleaved XY/depth DWORDs, then mode and texture/material.
The ribbon's near effect quads carry a fourth block of per-corner levels
that the effect consumer never reads; its far quads use the D6 layout.
Fan RGB starts at `20`; textured fan UV starts at `2C`. Clipped E2 UV follows
mode/texture directly. Ordinary E2 has neither field. Effect color is at `28`.
DE shared RGB is at `20`, with Far-color/fog at `24/28`.
Far-color/fog offsets are respectively `44/48` for textured fans, `2C/30` for
flat fans and effects, `28/2C` for ordinary E2, `38/3C` for clipped triangles,
and `48/4C` for clipped E2 quads. E6 clipped UV inserts a shade DWORD:
Far-color/fog begins at `3C/40` for triangles and `4C/50` for quads.
D6 direct retains four shade DWORDs at `28..37` and Far color/fog at `38/3C`.
D6 clipped quads retain the same shades followed by UV at `38` and Far
color/fog at `58/5C`; its clipped triangles use the textured fan layout.
Three-vertex fog writes only three bytes.

Renderer virtual `+04` is a required call at each sorting boundary; it may
publish pending commands before returning its batching policy. If disabled,
the original first-vertex depth is the key. For textured batching, mode bit 0
selects one of the H3 selectors. Selector `FF`, or selected descriptor flags
intersecting `39`, uses `(depth >> 1) + C0000000`, with a **logical** shift.
Otherwise the key is `(signed(descriptor_address) >> 1) + 40000000`, with an
**arithmetic** shift and wrapping addition. The descriptor address is logical
entry base plus `4 + selector*1C`; a real allocation identity is required only
on this branch. Null textures retain their depth after the sorting query.
Flat batching uses `40000000` when `(mode & 0x16) == 0`, otherwise the depth band.

`TextureSortIdentity` names the two allocation policies explicitly:
`NativeEntryAddress` preserves the original pointer formula above, while
`PortableEntryOrdinal` provides the CPU backend's stable opaque texture rank.
An ordinal is never passed as a fabricated native address. Portable capacity,
physical-selector ordering, source-entry aliasing and retained source lifetime
are enforced by the implementation; FF and translucent paths still use their
geometric keys without requesting an identity. The portable key and its
unresolved coplanar allocation-order comparison belong to
[Pixel runtime](PIXEL_RUNTIME.md#resident-scene-backend).

## Deferred arena and lifetime

Allocator `004A42B0` grows commands forward and pending entries backward in
16-byte units. A submission consumes header plus payload and one pending block
for an unsorted group, two for a sorted group. Equal front/back cursors are
allowed; capacity overflow raises native `301` before committing ownership.
Native scratch pending words may be written before that exception; neither
cursor nor owner state is committed.
Reset `004A4400` creates an implicit sorted root outside the command arena.

Group entry `004A4740` calls renderer virtual `+04` first, ignores its return,
then allocates its parent entry/header and one payload block. The preparation
callback's resulting live queue state controls allocation. Closing `004A44A0`
copies one block per pending entry forward and restores that group's saved
backward cursor. An unmatched close corresponds to native `302`.

Sorted replay is descending **signed** depth; equal keys replay ascending
unsigned command pointer. Monotonically allocated command offsets preserve
that tie order without fabricating process addresses. Unsorted groups replay
in insertion order. A group's key orders the group inside its parent; its
children replay at that position rather than joining a global flat sort.

Replay `004A4460` closes all open groups and walks the root; `004A4430` handles
nested groups. Each callback receives its payload and the renderer receiver
plus header `+04`, using wrapping 32-bit addition. Successful replay does not
reset or consume the batch: a second replay runs its callbacks again, retaining
any packet edits made by the first replay. Explicit reset releases the queue's
commands and their resource borrows. A persistent scene also retains texture
uploads and sort identities, so its owner must release those borrows before a
mutable residency transition; clearing the queue alone is insufficient.

The `004395A0/004395C0` pairs in world and terrain traversal are profiling
markers, not reset/replay boundaries. World worker `00440F00` flushes the sky,
resets depth through renderer `+14(NULL)`, then resets the queue before
`00488020`. Its active particle pass, entity draws, pending particle pass and
transfer, and `0044AA10` terrain therefore share the following queue until the
worker's actual replay. Missing feature services retain queued prefixes; the
portable continuation guard refuses to revive a stopped arena or a failed
pixel flush/presentation. The [briefing terrain](TERRAIN_RUNTIME.md#shared-terrain-traversal-and-strip-geometry)
caller explicitly flushes its ground at `0040C201..0040C21C` before the next
batch/depth reset. Neither caller flushes each terrain profiling interval.

## Renderer-thunk dispatch

Retail deferred callbacks are virtual forwards, not rasterizers. Each
disassembles to `mov eax,[ecx]` then `jmp [eax+offset]` (checked with MinGW
`objdump -d -Mintel` against `infestation/Infest.exe`):

| Producer | Thunk | Renderer slot |
| --- | --- | --- |
| 01 textured triangle near | `004A38F0` | `+0x7C` |
| 01 textured triangle far | `0044EA50` | `+0x80` |
| 01 flat triangle near | `004A39B0` | `+0x4C` |
| 01 flat triangle far | `004A39E0` | `+0x50` |
| DC material shared-shade triangle near | `004A3950` | `+0x44` |
| DC material shared-shade triangle far | `004A3980` | `+0x48` |
| DE shared-shade textured triangle near | `004A38C0` | `+0x5C` |
| DE shared-shade textured triangle far | `004A3900` | `+0x60` |
| D2 direct per-corner triangle near | `004A3930` | `+0x64` |
| D2 direct per-corner triangle far | `004A3960` | `+0x68` |
| D6 direct per-corner quad near | `0044EA60` | `+0xAC` |
| D6 direct per-corner quad far | `0044EA70` | `+0xB0` |
| D6 clipped per-corner UV quad near | `004A3940` | `+0xC4` |
| D6 clipped per-corner UV quad far | `004A3970` | `+0xC8` |
| E2 ordinary quad near | `00466150` | `+0x9C` |
| E2 ordinary quad far | `00466160` | `+0xA0` |
| E2 clipped UV triangle near | `004A3990` | `+0x6C` |
| E2 clipped UV triangle far | `004A39C0` | `+0x70` |
| E2 clipped UV quad near | `004A3A20` | `+0xB4` |
| E2 clipped UV quad far | `004A3A60` | `+0xB8` |
| E6 clipped shared-shade UV triangle near | `004A3A10` | `+0x74` |
| E6 clipped shared-shade UV triangle far | `004A3A50` | `+0x78` |
| E6 clipped shared-shade UV quad near | `004A38D0` | `+0xBC` |
| E6 clipped shared-shade UV quad far | `004A3910` | `+0xC0` |
| 1B effect quad near | `0040EF30` | `+0xA4` |
| 1B effect quad far | `00411D50` | `+0xA8` |
| Frontend star and E0 solid quad | `00466170` | `+0x84` |
| E0 solid quad far | `00466180` | `+0x88` |
| D4 and terrain colored quad near | `0044EA80` | `+0x94` |
| D4 and terrain colored quad far | `0044EA90` | `+0x98` |
| D8 solid triangle near | `004A38E0` | `+0x3C` |
| D8 solid triangle far | `004A3920` | `+0x40` |

[`PrimitiveCallback::renderer_virtual_offset`](../../crates/inf-game/src/model_primitives.rs)
preserves this map. [`pixel_backend`](../../crates/inf-game/src/pixel_backend.rs)
routes flushed queue commands through one required `draw` call carrying the
callback identity, payload, and receiver bookkeeping; portable renderer
handles are never fabricated into native addresses.

## Pixel consumer boundary

[`software_frame`](../../crates/inf-game/src/software_frame.rs) validates
packets and dispatches these recovered Direct3D consumers:

| Callback | Concrete consumer |
| --- | --- |
| Near flat triangle | `004C12F0` |
| Near and far DC triangle | `004C03E0` / `004C0A30` |
| Near textured triangle | `004CE930` |
| Far textured triangle | `004D0170` |
| Near shared-shade textured triangle | `004C3B60` |
| Far shared-shade textured triangle | `004C5210` |
| Near direct per-corner triangle | `004C6AB0` |
| Far direct per-corner triangle | `004C8160` |
| Near clipped UV triangle | `004C9A00` |
| Far clipped UV triangle | `004CA7B0` |
| Near ordinary textured quad | `004D5410` |
| Far ordinary textured quad | `004D6570` |
| Near clipped UV quad | `004DFC80` |
| Far clipped UV quad | `004E1000` |
| Near shared-shade UV triangle | `004CB6B0` |
| Far shared-shade UV triangle | `004CCEF0` |
| Near shared-shade UV quad | `004E2550` |
| Far shared-shade UV quad | `004E4710` |
| Near direct per-corner quad | `004DBA90` |
| Far direct per-corner quad | `004DDA30` |
| Near per-corner UV quad | `004E6B70` |
| Far per-corner UV quad | `004E8D30` |
| Near effect quad | `004D78A0` |
| Far effect quad | `004D9840` |
| Near and far solid quad | `004D1BB0` / `004D2170` |
| Near and far solid triangle | `004BFAB0` / `004BFE80` |
| Near and far colored quad | `004D3EB0` / `004D47C0` |

[`SoftwareScene`](../../crates/inf-game/src/software_scene.rs) connects
resident model execution to this queue and pixel target. The frontend star
producer queues the solid callback; footer drawing invokes the effect consumer
directly without adding a deferred command. Its frame reset and presentation
clear order belong to [Frontend runtime](FRONTEND_RUNTIME.md#frame-reset-and-presentation-tail).

Material conversion, corner/effect lighting, reciprocal depth, upload ownership,
sampling, native viewport clipping and persistent device state belong to
[Pixel runtime](PIXEL_RUNTIME.md). These callback families retain their distinct
packet and color semantics while sharing final triangle coverage. DA/E2 clipped
UV callbacks use white RGB with their Near/Far fog policies and signed DWORD UV
coordinates through their separate
[consumer](../../crates/inf-game/src/d3d_textured_uv.rs); copied scratch colors
never become packet inputs. E6 clipped UV instead copies its independent
shared shade into [four shaded UV consumers](PIXEL_RUNTIME.md#shared-shade-uv-primitives).
DE shares those clipped consumers. D2/D6 retain interpolated corner shades in
ordinary [Near/Far textured triangles](PIXEL_RUNTIME.md#per-corner-far-textured-triangles)
and [two shaded UV quad consumers](PIXEL_RUNTIME.md#per-corner-shaded-uv-quads).
Shared-shade triangles, ordinary textured
quads and effect quads retain distinct Near/Far primary policies. See
[shared-shade triangles](PIXEL_RUNTIME.md#shared-shade-textured-triangles),
[ordinary Far quads](PIXEL_RUNTIME.md#ordinary-far-quad), and
[effect quads](PIXEL_RUNTIME.md#effect-quads-and-solid-quads). Other unowned Far
callbacks, unsupported upload/device policies and secondary texture passes
remain explicit failures. The CPU target
implements the recovered Direct3D policy; it does not substitute the viewer's
unlit material expansion or
claim to execute the original software scan converter.

The Rust command backend stops after a validation or required-host failure,
retaining prior commands and completed callback counts. The queue itself stops
only on arena/group faults or failures in its own prepare/flush operations, so
commands queued before a primitive validation failure remain flushable. This is
containment, not a claim to reproduce native exception unwinding. Queue reset
is explicit; it does not silently revive a stopped model executor. Unsupported
primitive opcodes, absent resource loaders and unimplemented renderer callbacks
remain explicit boundaries. Portable texture ownership and SDL background
presentation are concrete; native atlas eviction/allocation-order parity and
matched retail pixels remain separate acceptance work.
Submitting after the root group has closed is rejected until reset; this is
also a Rust containment rule, not a recovered native recovery path.
