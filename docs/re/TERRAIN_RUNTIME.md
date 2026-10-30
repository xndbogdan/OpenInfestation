# Infestation terrain runtime preparation

This document owns the native terrain state needed before world placements
can be constructed: diagonal preparation `004506A0`, height sampling
`00450A30`, initial terrain-map indices `00471AE0/00471C40`, the shared
terrain draw traversal `0044AA10`, and the spacing and height policies
their callers supply. It also owns the sparse furniture-radius publication
`004508E0`. The
[H12 format](FORMAT_DOCUMENTATION.md#handler-12-terrain-grid-format-decoded) owns immutable source
bytes; [world assets](WORLD_ASSETS.md) owns H14 placement order and constructor
arguments. [Runtime foundation](RUNTIME_FOUNDATION.md#first-world-boundary)
owns the session transition and the remaining world-loading boundary.

## Renderer failure custody

The shared [software terrain bridge](../../crates/inf-game/src/software_scene/terrain.rs)
retains the first typed renderer or resource-custody failure at a strip's
omission boundary. All 16 required terrain/clipping operations stop before
further callbacks, RNG, publications or command allocation; the original
cause returns before a later `Stopped` error or successful traversal outcome.
A full arena (native `0x301`) is not such a failure: `0044AA10` runs each
14-row strip, near and far, in its own error frame, which disposes any error
(`004923C0`) and goes on to the next strip. The strip's omission therefore
resumes the queue (`SoftwareScene::recover_native_301`), and the next strip
allocates what still fits; the world draw flushes the batch after the
terrain as before ([presentation policy](PRESENTATION_POLICY.md)).

Furniture forwards typed renderer errors, required registration/metadata and
source-custody failures through the same owner while committing its reached RNG.
Model scratch capacity, range and lifetime faults remain hard through lighting
and custom row-access wrappers. The existing null-header item boundary, known
unavailable resources, geometry, particle births and unowned semantic services
retain their separate per-feature policies;
diagnostic strings never select renderer recovery. The [actual allocator
regression](../../crates/inf-game/src/software_scene/terrain/tests/hard_custody.rs)
uses original textures/materials with declared terrain and queue capacity.
The [custom-row custody fixture](../../crates/inf-game/src/software_scene/furniture/tests/scratch_custody.rs)
reads a deliberately retired real process view before output/RNG/birth work.
These fixtures establish portable failure containment, not native heap reuse
or complete error-unwind acceptance. The [briefing caller](BRIEFING_RUNTIME.md#briefing-feature-failures-and-batch-custody)
owns its distinct content and frame flush boundaries.

## State and policy custody

H12 stores a fixed 512 by 512 grid, indexed `cell[x * 512 + z]` for the
loader's X/Z inputs. Both world coordinates are WORDs and wrap. Grid spacing
is process state, not an H12 field or a property inferred from image size.

| Policy | 128-unit spacing | 256-unit spacing |
| --- | --- | --- |
| `005372F0` world step | `0x80` | `0x100` |
| `005372F4` fraction mask | `0x7F` | `0xFF` |
| Stored-cell neighbour distance | 1 | 2 |
| Interpolation shift | 7 | 8 |

`0048DD75..0048DDA5` supplies the separate height scale from H14 flags:
flag `0x80` clear selects `005372F8 = 5` and `005483BC = 8`; set selects
4 and 9. The first shifts signed cell heights. The second belongs to plane
height consumers; `00450A30` itself never reads or clamps to the plane.

The executable's initial `.data` contains spacing 256, mask `0xFF`, and
height shift 5. That is not proof of the live policy at an arbitrary load.
The loader only overrides spacing when H14 flag `0x2000` is set. A portable
loader caller must therefore supply the current spacing explicitly; a saved
Detail setting alone does not establish the preceding native writes.

| Native owner | Policy and preparation effects |
| --- | --- |
| `004502C0(detail)` | Uses its ECX argument, overriding it to zero when H14 flag `0x2000` is set. Zero selects 256/`0xFF`; one selects 128/`0x7F`; any other value leaves both globals unchanged. Then resolves H12 slot 0 and calls `004506A0(0)` unconditionally. |
| `00450350` | Temporarily selects 128/`0x7F`; does not prepare the grid. |
| `00450370` | Restores from raw settings DWORD `+70`, or zero for H14 flag `0x2000`. Only values zero and one update the globals. Does not prepare the grid. |
| Loader `0048DDA5..0048DDB4` | Calls `004502C0(0)` only for flag `0x2000`; otherwise retains the preceding spacing. |
| Briefing constructor `0040A987..0040A98E` | Calls `004502C0(0)`, after world loading and placement construction. |
| Gameplay constructor `00440B20..00440B34` | Converts settings `+70 != 0` to zero/one before calling `004502C0`. This differs from `00450370`'s raw-value handling. |

[`ProcessTerrainPolicy`](../../crates/inf-game/src/terrain_runtime/policy.rs)
retains the horizontal pair through loads, screens, errors and reloads.
`WorldPlacements` accepts that real owner or an explicitly supplied inspection
policy. Production loading publishes the H14 override before resolving H12 and
preparing placement inputs; it does not read Detail at load time. Height and
plane shifts remain the separately loaded H14 policy.

Gameplay reads the actual setting after its input-selection prefix,
Booleanizes it, then executes `004502C0` before marking the screen ready.
Briefing selects literal zero after its world/control/color prefix and before
animation reset. Both mutate existing cells and prepare diagonals even when
the pair is unchanged. Heights, materials, furniture and unvisited triangle
bits survive. A missing H12 source stops after the pair publication; a missing
setting stops before it. Reload consumes the retained pair before the next
screen applies its own policy.

The older claim that `00450350` runs unconditionally at boot was unsupported.
The original PE pair is coarse. A complete text census finds seven direct
calls, in temporary scopes `00409A20`, the `00409Fxx` map path, `00437110`,
`00437400`, `0043C4C0`, `0043C620`, and `004765A0`, each followed by
`00450370`; the PE has no absolute pointer to `00450350`. This establishes
the initial source and explicit writers, not an arbitrary runtime first-read
value or a player's saved setting.

Temporary fine selection and raw restoration never regenerate triangle bits.
Their intrinsic policy methods are owned; each temporary caller must loan the
actual process and terrain when admitted. The first admitted caller is
[the bridge's update](FAMILY40D800_RUNTIME.md#the-bridges-update-00409a20)
`00409A20`, whose height writes `00450890` store a cell's low byte only. Far terrain's separate 512-unit
scope retains its existing draw-local publication and restoration.

The [72 native receipts](../../crates/inf-game/tests/fixtures/terrain_policy_native.json)
execute `004502C0/00450350/00450370` with supplied resource/preparation
boundaries, pinning raw 0/1/retain selection, H14 override, startup data and
policy-before-preparation writes. Reproduce with
check_terrain_policy.py. Tests also cover
existing-cell preparation and the real intro load, briefing, gameplay and
level-1 reload chain. Object, pickup and furniture model submissions loan the
current terrain to the selected opcode `2B/2C` callbacks; they do not consult
the ordinary scene renderer's epoch snapshot. The
[live-model regression](../../crates/inf-game/src/software_scene/terrain/tests/live_models.rs)
keeps that snapshot deliberately coarse while actual policy preparation
changes the borrowed terrain to fine, then executes both native command
owners against the changed source. These proofs do not equate Detail with fog distance:
[adaptive visibility](WORLD_VISIBILITY.md) still consumes actual frame timing.

## Diagonal preparation: 004506A0

The routine receives H12 through ECX and a traversal step on the stack.
A zero step uses the current `005372F0`. It visits both world-coordinate
axes from zero to below `0x10000`, with the second storage coordinate in the
outer loop. Coordinates are masked to WORDs before the stored-cell lookup.
The current fraction mask still controls address alignment, independently
of the traversal step.

At a visited origin, the material digits are read in this order:

```text
a = material(x, z)           b = material(x, z + step)
c = material(x + step, z)    d = material(x + step, z + step)
combo = a + 4*b + 16*c + 64*d
```

The orientation word is the second DWORD of the eight-byte lookup entry
at `00547B08 + combo*8`, generated by `0044B5C0`. Its low nibble determines
the origin cell's bit 11: clear for zero or two, set otherwise. All other
cell bits remain unchanged. The existing canonical
[`terrain_blend`](../../crates/inf-formats/src/terrain.rs) reproduces this
table, including first-match tie order.

At the normal zero-step call, fine spacing visits all 262,144 stored cells;
coarse spacing visits the 65,536 even/even origins. Unvisited cells keep
their existing values. Runtime preparation must own a copy of the cells;
the canonical H12 source and its sparse records remain unchanged.

The loader's order matters. After the optional flag-`0x2000` preparation,
`0044FDD0` publishes H15 slot 0, resolves H12, scans furniture radii through
`004508E0`, normalizes the H12 lighting vector and updates plane-cache state
through `00450230`. It does not prepare diagonals. The following
`0044B590` unconditionally builds the blend table (`0044B5C0`), resolves
H12 and calls `004506A0(0)`. This second preparation precedes the placement
loop `0048DEB9..0048DF39`. The bounded terrain owner models the resulting
prepared grid; it does not claim the other `0044FDD0` effects.

The light-direction publication at `0044FE17..0044FE97` is now separately
owned by `normalize_terrain_light`. Native X retains its signed DWORD value
in x87; Y is spilled to `f32`, and Z multiplies its retained signed DWORD
by its spilled `f32`. The squared-length sum is `(Z product + Y squared)
then X squared. The square root is spilled to `f32` before all three
divisions. Equal raw X/Z values can therefore produce different stored
direction bits. The read-only checker
executes those unmodified instructions with the supported CRT control word,
without arithmetic hooks or patches. Its 553 checked cases include 512
synthetic random signed-DWORD triples, eight labeled small/extreme triples
and all 33 original H12 vectors, whose canonical export spans are verified
against the OVL bytes. This is static routine validation, not a retail
observation or a light-grid/terrain frame comparison.

## Shared furniture-radius publication: 004508E0

[`furniture_radius`](../../crates/inf-game/src/terrain_runtime/furniture_radius.rs)
owns the ordered scan over an explicit mutable output. The separate
[`ProcessFurnitureRadius`](../../crates/inf-game/src/terrain_runtime/furniture_radius/process.rs)
retains process WORD `005483C0` across loads. Caller
`0044FDD0` resolves H15/0, resolves H12/0, then calls this producer at
`0044FE12`, before light normalization. Every invocation first writes zero
at `004508F5`; there is no one-shot or nonzero-radius guard.

The producer scans all 512 sparse buckets in increasing ordinal order and
every stored record in each bucket's order. It reads no cell flag, spacing,
state nibble or X-alignment gate. The native row pointer is retained at row
entry; the count WORD is reread after each reached record. The canonical
runtime retains the loader-proven count, storage and sorted X words, while
the same mutable request permits services to change sparse payload bytes
and the process output. Each reached lookup BYTE is cached before H11/H8
resolution, so changes to a future record are visible and changing the
current payload does not change that already-cached lookup. Mutable bucket
shape/count or storage replacement is outside this bounded API.

Lookup zero sets the H11 entry pointer to NULL and then unconditionally
reads its first WORD at `0045094F`. The port reports that reached error
after the entry reset and any earlier writes, without invoking H11 or
applying the final cap. Nonzero bytes, including `FD`, `FE` and `FF`, are
ordinary unsigned H11/0 indices in this producer. Its first WORD is a
signed H8 slot: negative and strictly above the allocation count skip;
equality reaches the explicit pointer-table boundary error. A missing H11
allocation reaches a required-table error after the actual manager `+00`
returns NULL. A valid missing H8 model invokes the same authenticated
`004EFBF0` NULL return and then skips. The resident adapter composes the
existing current H11/H8 policy and reads exact model WORD `+0E`; it supplies
no table or radius fallback.

The comparison at `004509B2..004509C4` zero-extends the model radius WORD
and sign-extends the current process WORD. Consequently, `[65535, 0]`
publishes zero, `[32768, 1]` publishes one, and `[600, 32768]` retains
`32768`. Normal completion caps only a signed process WORD greater than
512; a negative WORD is left unchanged. Required-service failures preserve
the uncapped prior output and all service mutations. The unsigned
model-radius warning threshold is 384, but its `0048C850` target is a
literal RET in this authenticated EXE and adds no service or mutation.

The unchanged-instruction checker
and [fixture](../../crates/inf-game/tests/fixtures/terrain_furniture_radius_native.json)
cover 73 ordered cases: empty/repeated scans, far-apart buckets, signed-WORD
boundaries, the H11/H8 decision matrix, and mutable payload/output/resource
services with reached failure prefixes. Their supplied services do not prove
an enclosing native load.

The [borrowed world loader](WORLD_LOAD_LIFETIME.md#borrowed-loadworld) now calls
this same resident adapter during `AssetPreparation`, after activation and
`WorldPlacements::new`. It borrows that owner's actual mutable H12 runtime and
the mandatory process WORD retained by `PortSession`; H11/H8 resolve from the
same current residency. No terrain clone, per-world radius owner or captured
radius supplies this publication. Required errors retain the reset and prior
maxima before later presentation, map RNG and placement work; the backend
records the stopped phase rather than returning successful metadata.

`WorldPlacements::new` already computes pure slope factors/light normalization
and prepares diagonals before this call. Radius does not read those results.
The call precedes retained presentation-light and prepared-state publication,
but this inherited earlier preparation does not reproduce the complete native
`0044FDD0`/`0044B590` order. The raw H15 pointer publication `00548394`, plane-cache
side effects and complete `0044FDD0` remain separate owners.

[Load and prefix tests](../../crates/inf-game/src/world_backend/radius_tests.rs)
lend one process owner to real Intro/leaf32 and Level1/leaf28 saved-import
loads and compare against an intrinsic scan of the same H12/current H11/H8.
Missing H11 and later lookup-zero tests declare their supplied runtime domains;
they are not injected full-load failures or campaign-transition witnesses.
The [native guards](../../crates/inf-game/src/terrain_runtime/furniture_radius/native_guards.rs)
bind complete consumers and the supported PE loader-zero range. The shared
[collision entry and scan](FURNITURE_COLLISION.md) consumes required actor H8
radius before either zero exit. Its active query context and
[auxiliary interpreter](MODEL_AUXILIARY.md) remain required; the published radius
never supplies an invented no-contact result.

## Height consumer: 00450A30

The sampler uses only the low WORD of each coordinate. It aligns the cell
origin using the current mask, obtains the four wrapped neighbours, and
sign-extends each cell's low byte before shifting by the selected height
scale. The origin's prepared bit 11 chooses the diagonal. Equality follows
the upper branch of each comparison: `x_fraction >= z_fraction` for a
clear diagonal, or `x_fraction >= spacing - z_fraction` for a set diagonal.

Each of the two signed interpolation products is arithmetic-shifted
separately before addition. Combining the numerators and shifting once
changes negative rounding and is not equivalent. The observable result is
the low signed WORD. The third stack argument, zero at the loader call,
is not read by this routine. There is no plane clamp, sparse-object lookup,
terrain lighting, or floating-point interpolation in this consumer.

At `0048DEB9..0048DF39`, the loader copies the placement's raw X/Y/Z words,
samples X/Z, and adds the returned height to Y with WORD wrapping. It then
passes rotation words in source order `+0E,+0A,+0C`, the full resource
argument whose signed low WORD selects H13, the original placement index,
and the instance-command address to `00460310`. Duplicate pointers and
authored iteration order remain observable; this is not a deduplicated set
of static models.

Port setting, not retail: Ultra ground detail ([`terrain_runtime::ultra`](../../crates/inf-game/src/terrain_runtime/ultra.rs)).
On High, offline, the sampler reads a fine grid every 64 units instead of
the cells, split as the cell above it is: retail's heights at the grid
points, a bicubic Hermite patch between them that rounds hills and keeps
creases where the ground turns more than the tuning's crease angle (40
degrees). A cell write (a bridge, the flood) redoes the fine points within
two cells of it. The slope probe `00450BD0` keeps the cells: it snaps to
cell centres for its decision. Ultra moves the ground at about three of
four points, by about 2 units on average and at most 25 to 93 in the
worlds measured (8, 14 and Level 1). It also rises under buildings, to
their base (`ultra_footings.rs`): retail stands furniture at its cell's
height and a placed model at its origin's, so on a slope their bases float
on one side.

## Terrain Project callback: 0044FFA0

[`TerrainRuntime::project_44ffa0`](../../crates/inf-game/src/terrain_runtime.rs)
implements the selected renderer-table `+14` callback installed by `004501A0`.
Default renderer callbacks and table alias custody belong to
[model runtime](MODEL_RUNTIME.md#renderer-surface-callbacks-and-table-custody).
Inputs are complete source world XYZ, the retained H12 grid/plane/slope pair,
actual H14 entry-zero `+04` flags, process clock `005471C0`, original renderer
`+04/+08/+0C` XYZ and a conditionally required original sine table. Model position
is not the copy-path source. Residency is supplied; arbitrary lazy-resource
mutation and native aliased/partially readable storage remain outside this API.

The plane DWORD arithmetic-shifts by native `005483BC`; the initial branch
narrows that datum to signed WORD. If wrapping source Y-10 is at most that
datum, `00450A30` samples unsigned low-WORD X/Z. With sampled height at most the
datum and wrapping source Y+10 below it, the slope branch adjusts X/Z using
the retained `0044FDD0` factors, resamples height and returns. Within the
remaining near-datum band it copies the original renderer XYZ. Those branches
need neither world waves nor the sine table.

The tail samples source X/Z, calls water owner `00410F60`, chooses the signed
WORD maximum of water/terrain height, adjusts X/Z with the same factors,
resamples at adjusted X/Z, and takes another signed WORD maximum for output Y.
Each adjustment retains native signed product/high-DWORD extraction and
wrapping DWORD additions. Both water calls use
`TerrainWaterSurfaceRequest`: low unsigned X/Z WORDs, process wave clock,
H14 bit `40` and the **genuine fifth signed terrain-height argument**.
The first height is pushed at `004500C3`, before the H14 lookup; the second
at `0045016D`. The first X is source X; the second is adjusted output X.
Neither is a plane datum or an output-Y substitute. The former unspecified
caller-stack/height-as-X interpretation is superseded by those instructions.

`00410F60` admits sine reads only when bit `40` is set and signed terrain
height is below the full shifted plane DWORD. A set flag alone does not
require a table. A missing table fails only at a reached wave branch; clear
flags, above-plane heights and early slope/copy returns keep that source
unread. All arithmetic in
[`spacing_sample`](../../crates/inf-formats/src/native_basis.rs) retains the
native wraps, low-WORD folds and signed comparisons. Camera callers still
supply their own literal zero clock; model callbacks receive the live clock.

The unchanged native checker and
[compact receipts](../../crates/inf-game/tests/fixtures/terrain_project_44ffa0_native.json)
execute original `0044FFA0`, `00450A30`, `00410F60` and resident getter
`00405540`, with all executable text checked unchanged. Their 569 cases
contain 543 supplied-domain comparisons and 26 native-only diagnostics.
They cover both spacings and linked height/plane shift pairs 4/9 and 5/8,
copy/slope/tail branches,
clocks, signed/DWORD limits, two fifth arguments and conditional sine access.
Native alias, pointer, partial output and independently changed plane-shift
cases retain their explicit diagnostic limits. A value-returning Rust error
does not reconstruct the callback's partially written native temporary; the
model handler publishes XYZ only after callback success. These receipts do
not establish enclosing invocation, arbitrary callback mutation or matched
retail pixels.

## Shared actor terrain probes

The shared [slope/plane probe](../../crates/inf-game/src/terrain_runtime/probe.rs)
owns `00450BD0`, separately from the interpolated height query. Its only
recovered caller is cache miss `00429EE0`, with signed threshold 128 and
native byte policies `(clamp,reject)=(0,1)` or `(1,0)`. The resolved policy
enum also preserves the native unmodified arm; clamp dominates rejection
when both bytes are nonzero. The owner consumes the existing loader pairs
of spacing, height shift and plane shift, not arbitrary process overrides.

Unsigned X/Z WORDs select four wrapped cell heights in native X-major order.
Fine spacing snaps both axes to offset 64. Coarse spacing selects offset
64 or 192 and constructs a subcell from the full-cell diagonal mean and
adjacent means. All means truncate toward zero except quadrant one's
`00450D51` arithmetic shift. Plane classification uses the signed low WORD
of shifted H12 datum. Rejection occurs after snapped X/Z stores and before
the stack Y store: `TerrainProbePosition::y` remains `None`. Clamping changes
all four subcell heights, but an inactive coarse diagonal retains the
original full-cell mean for Y. Active diagonals recompute that mean and
compare the extra deviation scaled by `363 >> 8` with all four edge
differences. The final signed comparison retains native wrapping
`SHL8/SAR7`, rather than replacing it with unrestricted arithmetic.

The [miss wrapper](../../crates/inf-game/src/object_runtime/actor/derived/probe_miss.rs)
captures the **whole** actor byte `+160` before required mutable H12
acquisition: zero rejects below-plane terrain, any nonzero value clamps it.
A blocked probe returns before actor position/guard reads. Otherwise it ORs
`+22E` bit `8000`, captures current XYZ, installs the temporary probe XYZ,
and requires actual mutable collision `0047C1D0`. After successful collision
it rereads the guard, restores XYZ, then clears only a surviving guard;
other callback writes remain. Direct position stores do not mark dirty bits.
Acquisition and collision errors preserve all reached writes, including an
installed temporary position and guard on collision failure. There is no
automatic restoration on failure and no empty collision default. The shared
[production composition](FURNITURE_COLLISION.md#required-miss-and-normal-return)
lends the same actual mutable arena/actor and retained process state to both
Selector12 and Selector36. Executed zero-lookup or genuine counterpart skips
can return normally; the first ACTIVE candidate stops at its data-bearing
retained-context boundary. Active geometry/contact and complete enclosing
actor execution remain separate owners.

The unchanged-code checker and
[fixture](../../crates/inf-game/tests/fixtures/terrain_slope_probe_native.json)
cover 934 terrain cases and 168 miss cases. They compare both spacings and
height scales, four subcell quadrants, both stored diagonals, signed odd
clamped means, unsigned high/wrapped coordinates, signed plane WORD wrapping,
all low-byte plane policies and signed threshold boundaries. Miss receipts
include `+160` values 0/1/2/8/128/255, acquisition mutations before old-position
capture, collision mutations, surviving/cleared guards and stopped service
prefixes. H12 acquisition and collision are explicitly supplied services,
not evidence that those services or a complete actor frame run. Reproduce
from the repository root with Python and Unicorn 2:

```powershell
python check_terrain_slope_probe.py --output crates/inf-game/tests/fixtures/terrain_slope_probe_native.json
```

## Native receipts and validation

The checked-in [terrain receipts](../../crates/inf-game/tests/fixtures/terrain_runtime_native.json)
were captured by executing unmodified `0044B5C0`, `004506A0`, and `00450A30` from the retail
executable under Unicorn x86-32, without patched code or external stubs.
Four preparation receipts cover both spacings with zero and explicit matching
step arguments. The synthetic grid includes all 256 four-corner material
combinations at both spacings. Tests compare hashes of every prepared cell
and diagonal bit, plus changed-cell counts and selected cell values.

Another 1,024 native samples cover both height scales and spacings, both
diagonals, equality and adjacent triangle edges, negative heights and
separate-product rounding, wrapped grid edges, and discarded coordinate
high WORDs. The receipts also vary the unused third argument.

The [placement tests](../../crates/inf-game/tests/world_placement.rs) cover
all 33 retail world leaves, containing 4,486 placements, under both explicit
initial spacings. Their 66 native corpus receipts pin the raw H12 input,
the complete prepared grid, and the ordered, wrapped constructor-Y words
from 8,972 height queries. Together with the synthetic cases, this covers
70 preparation executions and 9,996 height samples. Eleven
worlds select 16-unit heights, 22 select 32-unit heights, and two force coarse
spacing. The tests also retain the actual H12/H14 registrations, full
resource DWORD, authored index and rotation order, instance-command
provenance, immutable source bytes, and re-entry through leaf 48.

The five terrain tests and three placement tests pass with:

```powershell
cargo test --release -p inf-game --lib terrain_runtime
cargo test --release -p inf-game --test world_placement
```

These are isolated routine and asset-consumer comparisons. They do not
execute the world constructor loop, later terrain mutations, the scheduler,
collision, or a rendered retail frame.

## Initial terrain-map indices: 00471AE0/00471C40

The loader constructs a map owner after terrain preparation and before the
placement loop. Its constructor `00471AE0` fills a 256 by 256 byte index
map at `0054ED78`. This is distinct from H10 projection samples, the palette
builder `00471D40`, and the [terrain radar](VEHICLE_DRIVE_RUNTIME.md#the-terrain-radar-00470170)
that draws it. The concrete
[`TerrainMap`](../../crates/inf-game/src/terrain_map.rs) owns the intrinsic
initial index snapshot. The [map lifecycle](WORLD_MAP_RUNTIME.md) now consumes
it through the real offline loader, retaining the process grid/RNG, ordered
paired markers and mission publication. `00471C40` is the map's only
writer and the constructor's loop its only caller, so the map never changes
after construction. The map object's other callbacks remain separate
owners.

The loop `00471B51..00471B73` visits X outside Z, both from 0 through 255.
`00471C40` reads H12 cell `(2*x, 2*z)` in `x*512+z` source order, but writes
the map at `x+z*256`. Both supported spacing masks preserve these even/even
coordinates. It reads raw signed height, material and shade bits, without
interpolation or the prepared triangle bit.

Every visited cell calls shared RNG `0048F060` once, before its plane branch,
even if the shade or plane makes the returned bit irrelevant. Given the
source shade nibble `s` and the RNG return `r`, the shade group is
`min(7, (s >> 1) + (r & 1 & s))`. The process state advances as
`state = state * 0x13D56D + 0x13D573` with DWORD wrapping, returning
`state >> 16`. The caller passes that state by mutable reference; map
construction must not invent a new seed or use a renderer-local RNG.

H14 flag `0x20` independently enables plane classification. The comparison
uses signed H12 `plane_height_fixed >> plane_shift`, with arithmetic shift
9 for 16-unit heights or 8 for 32-unit heights. If enabled and the signed
cell height is strictly below that plane, the palette entry is zero;
otherwise it is `material + 1`. Equality retains the material. The final
index is `shade_group * 5 + entry`, in `0..40`.

The [map receipts](../../crates/inf-game/tests/fixtures/terrain_map_native.json)
execute the complete unmodified native loop and both callees under Unicorn,
without hooks or code patches. Twenty-four synthetic maps cover both
spacings, both height scales, enabled/disabled plane classification, extreme
signed planes and three RNG seeds. Sixty-six corpus maps cover all 33 worlds
under both initial spacings. Each receipt pins all 65,536 bytes by hash,
the index histogram, selected coordinates and final RNG state. Another 96
isolated native cell receipts cover signed below/equal/above plane cases,
negative non-divisible fixed-point planes and shade clamping.

The existing recording supplies a separate full-map comparison for selection
22 / leaf 32. Its captured first map matches every byte generated from the
canonical resident H12 source and captured RNG seed `3E23E340`; the final
state is `9AD4E340`. Map SHA-256 is
`73DA3BB18743828B1F49C9A91F7B8018893D6FB3D891B34D9D2C6C9855700EC2`.
An indexed memory query counts exactly 65,536 four-byte RNG writes in that
construction interval, all from `0048F070` on the same thread. The
capture ledger
owns the exact queries, positions and observation limits. This proves the
initial map data and RNG consumption for that recorded load, not later
mutations or rendered briefing pixels.

Four unit tests and three [integration tests](../../crates/inf-game/tests/terrain_map.rs)
cover the policies, traversal, source immutability, complete native maps and
the recorded load. The corpus test constructs maps from `WorldPlacements`'
prepared terrain and actual H12/H14 registrations without changing residency.

## Shared terrain traversal and strip geometry

[`terrain_draw`](../../crates/inf-game/src/terrain_draw.rs) implements the
source-backed `0044AA10` footprint, `0044B2E0` scan domain,
`0044C130/0044B8C0` alternating row walk, `0044BC30` strip context,
`0044BE90` cell projection and direct `0044D6A0/0044D030` quad submission.
These routines share geometry between the gameplay world draw and the
briefing terrain pane `0040BB40`. Their caller must supply its actual view,
retained context projection/fade, tint, night color, dynamic-light grid and
clock. This owner does not replace that caller state with selector16 camera
data or regenerate a terrain mesh.

`0044AA10` quantizes the caller's stored forward axis, or its X axis when
the forward Y magnitude exceeds `0x800` after multiplication by4096. It
spills the two-axis length to `f32`, builds a planar basis through
`00490370`, and transforms four signed-WORD footprint corners through
`00490900`. CRT `__ftol` truncation and nearest-even `FISTP` transformation
are different operations here. The full footprint reaches furniture first;
the near and far ground footprints then split into fourteen-cell X strips.

Each footprint reaches 11/8 of a corner's depth to each side, and its back
corners `0x300` plus the tilt's share. That covers a 4:3 view, whose focal
is half its width, but not a port widescreen frame: at a 16:9 frame's sides
the near ground ended short of the view, the far strip drops any tile that
needs clipping, and on Altair Prime's token islet (44940, 34505) deep water
drew its additive surface straight over the background as bright yellow
wedges. A view that sees further than retail's
([`TerrainFootprintWidth`](../../crates/inf-game/src/terrain_draw/plan.rs))
widens every lateral extent by its reach, as the objects' cone takes it
(`ObjectDrawCone::View`): half the frame across over the focal, or half
its height over 3/4 of the focal, the larger. A 16:9 frame at retail's
focal widens by `width / (height * 4 / 3)`; the Remastered camera's 100
degrees across a 4:3 frame by its narrower focal (512 / 430 on the
1024x768 tier), and its speed kick further. Keyed to the aspect alone, the
4:3 Remastered view kept retail's footprints: over an Artoria
flight (2026-10-29, Vulkan, path traced, 1024x768 game frame) a quarter
of the frames left out furniture in view, up to 34 pieces (up to 76 on
Pelagia and 33 on Foldrak Noir in the same session), most of them toward
one side of the view. Widened by the view's reach, the same replay leaves
out at most 2 a frame, near the horizon. A widened corner
stops 0x7F00 from the eye, so the planar turn cannot wrap its WORDs: a far
footprint at Tabuchi's fade already reaches 32427, and widening it whole
wrapped its corners. A corner retail places beyond that keeps retail's
extent, and a view reaching no further than retail's keeps retail's
footprints. The footprint's walk
is game state (the furniture's random draws, trail births and state bits,
the near strips' dust), so it follows the game's view: retail's game draws
its 4:3 view with retail's footprints whatever the frame, and only the
view shown and the remaster's game take the widened ones
([the game's draw and the shown draw](PRESENTATION_POLICY.md#the-games-draw-and-the-shown-draw)). A
remaster's replay recorded at a view the reach-keyed footprints widen
parts from its recording where they walk more furniture: a
2026-10-29 replay kept every eye position through Artoria, but the
process random number parted on Pelagia and Artoria's creatures flew
elsewhere.

`0044B2E0` walks down from maximum to minimum signed Z by the current
spacing. Its left edge decrements the corner index and its right edge
increments it, skipping horizontal edges. The right-edge advance suppresses
both X additions on that row; the corresponding left-edge advance alone
does not. Signed division precedes every final WORD store. At176 rows the
routine repeatedly overwrites row175, while extrema still include all
reached rows. Empty/inverted rows survive the fourteen-cell clipping pass.

That buffer already loses rows in retail at the full draw distance (the far
manager's cap, `0x3100`) on fine terrain: seen along X, the furniture
footprint's lateral reach becomes 270 rows of Z. A widened footprint loses
far more (360 rows: every furniture row beyond about 600 units to one side
of the eye, and near ground rows too), so furniture flickered in and out as
the view turned. A widescreen footprint therefore keeps every row
([`row_capacity`](../../crates/inf-game/src/terrain_draw/plan.rs)); a view
reaching no further than retail's, retail's game's among them, keeps
retail's 176.

`0044C130` and `0044B8C0` each alternate two stack buffers of fifteen
sixteen-byte vertices. The union of neighboring row ranges determines each
new buffer's filled prefix; the metadata retains separate tile offset/count
words. A shorter row does not clear old tail vertices, and the final fill
does not reset the previous offset. The port retains this custody rather
than synthesizing unwritten tail geometry. Each vertex contains three RGB
bytes, the H12 cell's high byte, projected XY/depth, clip and fog bytes.
The low signed height byte is shifted by the live height scale. Both ground
walks select far-local projection `004EB8E0` through the first pointer at
`00544740`; being in the near strip pass does not select the near projector.

Near palette channels are `tint * shade / 15`. Far palette channels are
`shade * 256 / 15`, stored as bytes, including the shade15 wrap. Only the
near pass admits settings `+80` dynamic light. Its90-square signed RGB grid
uses the actual low-WORD anchors and caller mask; each channel adds twice
the signed light byte and clamps to0..255. Off-grid cells use the original
`00526068` bytes supplied by the lighting owner. The far pass temporarily
selects512-unit spacing/mask511 and adds the native quadratic Y curvature.

| Direct producer | Deferred callback | Renderer slot | Packet distinction |
| --- | --- | --- | --- |
| `0044D6A0` | `0044EA60/0044EA70` | `+AC/+B0` | H3 relocation at `+24`, four RGB DWORDs at `+28`; far variant adds H7/74 color and four fog bytes. |
| `0044D030` | `0044EA80/0044EA90` | `+94/+98` | Material DWORD at `+24`, depth `+0x400`, per-corner transition toward RGB128; H3 lookup still precedes sorting but is not the packet texture. |

All four callbacks are native renderer thunks. Material blend lookup,
orientation permutation, diagonal choice and the signed cross-product cull
remain independent from the pixel consumer. The producer resolves a blend
texture only after the direct tile's cull, but before `0044E150` for clipped
near tiles. Far strips reject a failed clip-admission table entry without
resolving a texture. Sorting preserves the first H3 selector and the
native transparent/depth versus opaque allocation grouping policy.

The fade order is independently source-settled: `0044AD71` publishes the
incoming context interval before furniture; then the near interval is30%
and55% of its original far distance; the far interval is125% and150%.
Returning from `0044AA10` restores the retained context interval and grid,
but does not republish `0048FB00`, so the far global interval survives.
The briefing backdrop's earlier `INT_MAX/INT_MAX` publication must not be
silently restored before this actual first terrain publication.

The surrounding `004395A0/004395C0` calls measure elapsed time; they neither
reset nor replay the deferred queue. Furniture, near strips and far strips
retain their submitted prefixes in the caller's shared batch. Gameplay keeps
that queue through active particles, entity draws, pending particles and
terrain. The briefing pane explicitly flushes ground at the native
`0040C201..0040C21C` boundary before resetting depth for camera static.

Particles `0044C4A0` remain a required host service. The
[shared water owner](WATER_RENDERING.md) implements `0044C6A0/0044DBB0`
with explicit live H14 flags and process animation clock. Near clipped tiles `0044E150` now have the
shared producer and deferred consumer described below. Furniture uses the
owned native row domain and existing sparse-record walk; its
[shared item draw](FURNITURE_RUNTIME.md) connects the native basis, mutable
record state, particle effects and H1 callbacks. A missing service is reported per feature, preserving
submitted prefixes and subsequent strips/passes. Direct command production
alone does not establish a matched retail terrain frame. Focused tests cover domains, resource/fade
order, native packet differences and failure-prefix custody; no new retail
capture supplied these values.

### Furniture item setup: 0044CEF0

[`terrain_furniture`](../../crates/inf-game/src/terrain_furniture.rs) owns
the selected sparse record's position and ambient-light setup. Lookup byte
zero skips before any height/light read. A nonzero lookup samples the signed
stored H12 height, then selects all three signed RGB bytes from process grid
`00548404` when both anchor-relative indices are below90. Otherwise it reads
the low three signed bytes of the data DWORD at `00526068`; those are not
bytes derived from that address. Mask127/255/511 retains the caller's native
alignment, and this sampling has no settings gate of its own.

Original `0044CFA7..0044D00B` supplies all three `00490130` arguments:
`ambient[channel] = (unsignedTintByte >> 4) + signedLightByte`, from retained
`005474E0`. They are not register leftovers or one scalar fade argument.
`00490130` stores the unclamped signed component DWORDs at `0055F258/25C/260`
for normal shading, clamps only packed RGB at `0055F264`, and preserves that
DWORD's high byte. The port returns `light_rgb` and `ambient_rgb` separately;
`clamp_ambient_rgb` supplies only the packed RGB policy.

Tracked native checker executes unchanged
`0044CEF0` and `00490130` with supplied H12/grid/anchor/tint/context storage.
The terminal `004359A0` model/effect draw is an explicit recording host.
Its compact [48 setup and six ambient receipts](../../crates/inf-game/tests/fixtures/furniture_setup_native.json)
pin reached position, all signed RGB arguments, grid-edge/fallback decisions,
unclamped component state, packed RGB and high-byte retention. Rust comparisons
also reject a truncated reached RGB triple and preserve lookup-zero's empty
table gate. The separate [furniture draw owner](FURNITURE_RUNTIME.md) owns
`004359A0`'s model basis, effect/RNG and pending-effect fade composition.
These supplied-input receipts do not establish furniture pixels or retail
scene acceptance.

### Near-plane tile producer: 0044E150

[`terrain_draw::clipping`](../../crates/inf-game/src/terrain_draw/clipping.rs)
owns the tile producer reached by both ground `0044D6A0` and
water `0044DBB0`. Their separate height producers supply a `TerrainClipRequest` carrying the
four original row vertices, independently recomputed signed eye-relative
WORD coordinates, the actual published transform/projection/fog factor,
texture, orientation, material and raw signed context+62 bias. Gameplay
and briefing reach the same producer through `SoftwareScene::draw_terrain`.
There are no mission or level branches.

The gate requires at least one source clip40 bit and at least one source
without it, before dereferencing the logical texture's first descriptor
dimensions at +14/+16. Orientation nibbles map A(old), B(new), C(new+1), D(old+1)
to the fixed pixel UV rectangle. The original bit3/low-orientation diagonal
selects `[0,1,2]/[0,2,3]` or `[1,2,3]/[1,3,0]`. Original visible vertices
retain their complete RGB DWORD, depth, clip, fog and XY. A behind endpoint's
XY is never used for its crossing; clip40 does not require the extra80 bit
that the generic model `00497250` clipper requires.

Crossing positions do not come from copied projected coordinates or a stale
row vertex's retained world position. The strip preserves current buffer+F0
Z and recomputes X from current left/offset/spacing. Native `0044DB70`
resamples signed raw H12 height at the reached coordinates. E150's C uses
old-row X+spacing, while D uses new-row X+spacing; that asymmetry is retained
even when the row starts differ. World-minus-eye subtraction narrows to signed
WORD before interpolation. This keeps the original final stale-buffer offset
and its independently current crossing geometry.

`0048EF30` produces a saturating signed Q31
`(front_depth-64)/(front_depth-behind_depth)`. RGB and pixel UVs use this
initial factor. For each position
attempt E150 interpolates signed WORD coordinates, then decrements the factor
by `0x01000000` before reprojection/admission. A depth below64 retries while
the decremented factor remains positive. A crossing is only appended when
its reprojected depth is at least64 **and** that factor is still positive;
factor exhaustion discards that crossing. RGB/UVs do not follow the retries.
The bespoke X/Y dot order is `(X+Y)+Z`, depth is `(Y+Z)+X`, and CRT
`00511BA8` truncates before each DWORD publication. Ordinary model projection's
nearest-even conversion and other near-plane policy are not substituted.
The original overflow XY compression, clip-axis codes and wrapping fog
product remain explicit.

Each polygon fan triangle passes `00528BD0` admission without an extra
geometric cull, then queries renderer sorting separately. Its geometric sort
word is always signed context+62+64; it does not receive the direct quad's
spacing adjustment or an average depth. Mode0 H3 flags/selector retain the
shared depth/opaque texture grouping policy. The original five-block packet
uses callback `0044EA50`/renderer+80, mode0 at+18, texture relocation at+1C,
per-corner RGB at+20, signed `(pixelUV<<16)/dimension` at+2C, material at+44
and fog at+48. Padding and newly interpolated color high bytes stay unwritten.
The queue command counter retains every submitted triangle prefix, including
when a later triangle or strip fails.

The read-only tile clipping checker
executes unmodified `0044E150`, `0044DB70`, `0048EF30` and CRT conversion code
under Unicorn. Its 1,111 supplied-input cases match 2,075 packets and 67,140
projection retries across all sixteen near-plane masks, eight orientation
permutations, both diagonals, raw signed bias and sorting policies, nonidentity
transforms, WORD wrapping, height scales, projection overflow, fog-product
wrapping and factor exhaustion. Only renderer sorting and packet allocation
are intercepted. Eight first-packet/count cases also pin the Rust producer's
XY/depth/RGB/UV/fog words. These are synthetic function inputs, not captured
camera state or proof of a complete terrain frame.

The Far triangle checker
separately checks the owned `004D0170` renderer+80 consumer in 1,024 supplied
TL/state cases: 768 bypass cases and 256 native viewport-clipping calls. The
consumer shares an explicit Near/Far shading policy with the existing textured
triangle path. Packet ownership, TL/state checks and CPU rasterization do not
establish original device pixel precision or retail scene acceptance.

The read-only row-domain checker
executes unmodified `0044B2E0/0044B4C0/0044B510` under Unicorn over fifteen
synthetic inputs covering all three grid steps, asymmetric edges and the
176-row overwrite. Every row and both X extrema match the transcription.
It patches no native code, reads no recording, writes no fixture and does
not validate the other traversal phases or pixels.

### Shared production pixels

[`SoftwareScene::draw_terrain`](../../crates/inf-game/src/software_scene/terrain.rs)
now connects the original strip packets to the same deferred queues and CPU
pixel backend used by actor models. Both gameplay world views and the briefing
terrain pane supply their own view, retained projection, terrain sampling and
tint. It never selects a level or builds a replacement mesh. The bridge retains
texture allocation identities, packet bytes, diagonal selection and batch/fade
order under the caller's actual reset/replay boundaries, rather than treating
profiling markers as batches. Its command counter includes prefixes submitted
before a missing service;
the traversal's completed-strip count alone cannot count those prefixes.

The four thunk mappings are source-checked in the original executable:
`0044EA60/70` forward to renderer `+AC/+B0` (`004DBA90/004DDA30`),
and `0044EA80/90` to `+94/+98` (`004D3EB0/004D47C0`). The earlier D6
no-thunk interpretation and `d6-blend-pin` approximation are superseded.
Textured quads normalize each shade by 255/128 with hue-preserving overflow
scaling. Near and Far preserve native flag-selected blend/depth policy and
the allocation's actual UV bounds. Ordinary/alpha Far uses specular visibility
and native fog; flags10/20 instead fade RGB before normalization. Colored Far
blends unrounded material-modulated shade with unreplicated far-material channels
using fog/256; it keeps device fog disabled. RGB high bytes do not supply alpha.

The read-only quad checker executes all
four original consumers under supplied renderer/packet inputs. Its 336 cases
match TL RGB, UV, depth, specular fog, blend/depth/fog render-state writes and
quad diagonal across managed/unmanaged depth, saved depth values, global blend
policy and allocation UV bounds. Only device/texture/error calls are stubbed;
arithmetic and state-selection code are unmodified. Viewport clipping is
explicitly bypassed for these cases. Run with Python3
and Unicorn2. These checks establish the supplied-input routines, not a captured
full scene or device pixel precision.

The direct textured D6 consumers also retain completed primary draws when
descriptor0200 with renderer+44 requires a secondary pass. Near primary fans
`004DC843/004DC8DE` precede secondary setup `004DC938`; Far primary fans
`004DE973/004DEA0E` precede `004DEA68`.
[`d3d_d6`](../../crates/inf-game/src/d3d_d6.rs) returns its unchanged
implicit-UV primary plus an explicit continuation. The scene backend commits
primary pixels and D6/fragments counters before reporting that continuation.
SelectorFF and primary validation/upload failures retain their earlier gates.
check_d6_secondary_prefix.py executes
both original consumers and their actual viewport clipper for
[48 receipts](../../crates/inf-game/tests/fixtures/d6_secondary_prefix_native.json),
including toggle-identical TL/state/fans, four selector-FF returns and four
viewport rejects. [Rust prefix tests](../../crates/inf-game/src/d3d_d6_prefix_tests.rs)
also retain earlier/current draws and stop later queue commands on that error.
External COM/upload/display/error services are supplied; full secondary setup,
HRESULT/retry outcomes and GPU/retail pixels remain outside this check.
The [clipped D6 path](PRIMITIVE_RUNTIME.md#de-and-d6-source-polygons) shares
geometry data with other model opcodes but uses its own explicit-UV consumers.

[`world_lighting`](../../crates/inf-game/src/world_lighting.rs) now owns the
90 by 90 process grid: `0045B160` clears exactly `0x5EEC` cell bytes without
changing anchors; `0045B180` publishes low-WORD coordinates minus `0x1680`;
`0045B1A0` deposits signed RGB under actual settings DWORD `+80`. It retains
spacing, wrapped signed radius-square/weight arithmetic, end-exclusive sampling,
optional world-`+5C3` tinting and signed-byte saturation. World anchor inputs
are context eye minus `ftol(forward * -3840.0)`. Cells and anchors have process
lifetime across world replacement. The
[infrared vision](GAMEPLAY_RUNTIME.md#infrared-vision) player-light deposit
uses the old anchor before publishing the current one.

The native light checker executes unchanged
clear/anchor/writer/memset instructions with only a supplied settings-pointer
host. Its 86 cases compare every cell, anchors, canaries and native sample/store
counts, including a division fault before grid-index rejection. The compact
[27-case receipt](../../crates/inf-game/src/world_lighting/native_receipt.json)
backs [Rust comparisons](../../crates/inf-game/src/world_lighting/tests.rs).
These supplied inputs establish the kernels, not a matched retail light frame.

Gameplay connects the retained grid to both process-particle list passes and
near terrain reads; light deposition precedes a particle's final far-distance
rejection. The separate terrain-row particle producer `0044C4A0` now has a
shared owner described below; its process event gate remains distinct from
particle rendering and the graphics settings.
Water shares the production bridge with its own projection, wet/shade and
[shoreline policies](WATER_RENDERING.md).
The [shared furniture draw](FURNITURE_RUNTIME.md) reads the retained grid in
world and briefing views and reports unsupported item primitives separately.
The [briefing terrain prefix](BRIEFING_RUNTIME.md) clears the same process
cells at `0040BE6F` while retaining anchors, then admits shared near-ground
reads with the actual settings `+80` value. Furniture reads those cleared cells
independently of that setting. The previous `StaticTerrainLightApproximation`
is removed; the pane retains its load-time tint and publishes no new anchor.
Complete terrain and retail-frame acceptance remain open; optional HD output
does not change these evidence bounds.
The former million-unit frontend fade wrapped AA10's WORD footprint corners
on the native world-camera path. The source-owned
[world visibility](WORLD_VISIBILITY.md) chain now supplies bounded F/H and
retained predraw slew. World-camera regression fixtures check submitted ground
and the actual final draw receipt; native quad culling is unchanged. Briefing
and world fixtures establish their reached bridge paths, not complete
terrain services or retail-frame acceptance. Every world Draw now freshly
acquires its camera before fade/material and later live view publication. The
[retained NULL-context boundary](CAMERA_RUNTIME.md#acceptance-and-external-boundaries)
preserves reached writes without a fitted spawn view; camera acquisition remains
shared across missions rather than selected by this terrain bridge.

## Near-strip terrain particle emission

[`terrain_draw::emission`](../../crates/inf-game/src/terrain_draw/emission.rs)
owns `0044C4A0`, reached before each near strip when process DWORD `005483B0`
is nonzero. This same independent event word gates camera shake; it is not
graphics settings `+84`. Terrain preparation `0044FDD0` and cleanup `00450210` clear it;
the `00476520` event callback sets it. The actual registration is constructor
selector 49 (`00476380`, final vtable `00527908`), virtual `+AC`. Shared lifecycle
`0047D670` reaches it after publishing state 1 and clearing health. The actor's
virtual `+B4 = 00476720` admits damage through `0047CBC0` only when its signed
current terrain-rise height `+164` reaches target `+162`; lethal damage then
enters that lifecycle path. This is an authored terrain event, not a display
setting or an intro callback.

`00476520` first grants actor WORD `+7A` through the current player's `004750C0`
inventory owner, then writes the process gate. It stores the wrapping
`+15C * 1000000` countdown at actor `+168` and gameplay subobject
`G = [0055F160]`, `G+1D4`, sets byte `G+1C7 = 1`, and retires a resolved linked
`+16C` actor only in lifecycle state 3. Its enclosing `+24 = 004765A0` owns
terrain-height changes before activation and the later countdown/effect tail.
Constructor, placement commands `004763F0`, postinitialize `00476470`, damage
admission and those update services remain unowned in production. Connecting
only the gate would bypass that event sequence.

REA decompiles the whole class (2026-10-12). Its payload
names it the Seismic Stabiliser (H0 at `+40`, grant `+7A` = 81).

- Constructor `00476380`: over `0040D800`, `+15C = 10`, WORDs `+158` and
  `+15A` zero, `+168 = 0`, vtable `00527908`.
- Commands `004763F0`: `0x15` WORD `+158` (half-width of the raised square),
  `0x16` WORD `+15A` (sink depth), `0x17` DWORD `+15C` (countdown seconds),
  `0x18` DWORD `+16C` (linked handle); codes up to `0x14` go to `0040D9C0`.
- Postinitialize `00476470`: `+158 &= 0xFF80`; `+162` = ground height
  `00450A30(x, z, 0)`; `+160 = +164 = +162 - +15A`; `+166 = +164 + 1`;
  `+16C` = the handle of `0047F770()`'s result, or the process NULL; then
  the family's `0040E000`.
- Update `+24 = 004765A0(delta)`. With no countdown, the height `+164`
  waits at `+160` until `+14D` is set and `+14C` clear (one unit), then
  rises by `delta >> 13` per call up to `+162`. Whenever `+164` differs from
  `+166` it rewrites the terrain: `00450350`, then `00450890(x + i, z + j,
  +164)` over `[-+158, +158)` in steps of 0x80 on both axes, `+34 = +164`,
  `00450370`, `+166 = +164`. With a countdown it counts `+168` down and runs
  `0047DFD0(delta)`; at its end it calls `00487D70(position, 0xF00, 0)` and
  disposes itself (`00479FA0`).
- Damage `+B4 = 00476720` admits `0047CBC0` only once `+164 >= +162`.
- Event `+AC = 00476520`: above.

So a stabiliser rises out of the ground before it can be hit, and its
destruction shakes the world and starts the mission's doom countdown
(`+17` set, `+24` = `+15C` seconds; see
[the mission countdown](FAMILY40D800_RUNTIME.md#timed-generators)).

The canonical [static custody audit](../../crates/inf-game/tests/terrain_event_gate.rs),
run as `cargo test --release -p inf-game --test terrain_event_gate`, inspects all
33 world leaves with difficulty/language 0 and pins the executable vtable.
Selector 49 is authored only at leaf/zero-based placement `(17,35)`, `(23,3)`
and `(27,50)`: shared H13 slot 28 in `1.ovl`, initial H8 slot 279 `f04base`.
It is absent from intro leaf 32. Leaves 17/23 supply linked-reference command
24; leaf 27 omits it, while `00476380` leaves `+16C` unwritten. That actor's
initial linked reference therefore also needs an allocation-history owner.
These static registrations do not prove live activation.

[The port](../../crates/inf-game/src/object_runtime/actor/selector49.rs)
builds the stabiliser in all three worlds, with every method above and the
family's own `0040DE70` links. Its death writes the gate through the mission
it holds; the session applies the write after the actor callbacks, before the
camera pose and the terrain draw read the word, which is retail's order since
nothing reads it in between. World load (`0044FDD0`) and mission cleanup
(`00450210`) clear it, and the world draw passes it to the near strips' dust.
Leaf 23's reference names placement 39, built after the stabiliser, so
`0047F770` finds nothing and the link is the process NULL, as in retail.
Leaf 27's stays unknown: its death stops at `StabiliserLinkResidueBoundary`
when it reaches the link, after the grant, the gate and the countdown.
check_selector49.py runs the commands, the fit
with its link search, the update with every terrain write, the damage gate,
the death and the after-load over 54 cases, and the
[tests](../../crates/inf-game/src/object_runtime/actor/selector49_tests.rs)
match every height, write, link, event and countdown.

Each six-byte row consumes one RNG draw and is admitted only below
`spacing << 4`. The producer uses context `+04/+0C` low-WORD origins and
the row's signed WORD span, with `abs(span)/4096 + 2` births. Each birth draws
its selector first: the PE's `0044C56F..57F` counts trailing one bits, masks
the count with 3 and indexes literal selectors `6/9/10/11`. The shortened
decompilation loses this index and does not specify a fixed particle type.
X/Z products keep wrapped DWORDs before arithmetic shifts; Y reads H12's
signed cell byte directly under the actual horizontal and vertical scales.
Three further draws supply velocity `[32-(rng>>11), (rng>>9)+64,
32-(rng>>11)]`. `00489D90` then receives position, velocity and flags `0x800`;
its own RNG draw must occur before the next birth. No packet collection or
captured timing may replace this interleaving.

The shared terrain host now borrows those explicit origins, rows and resident
terrain, and admits actual process-pool trail births through the existing
producer. Its retained RNG writeback preserves a failure prefix. A missing
RNG remains a per-strip error before births. Pending publication and later
transfer/draw remain owned by the process particle schedule.

The tracked checker executes
unchanged `0044C4A0`, `00489D90`, RNG, H12 reads and pool admission. Its
[ten receipts](../../crates/inf-game/tests/fixtures/terrain_particle_emission_native.json)
cover both spacings and height scales, patterned signed heights, wrapped
origins/row edges, negative/zero spans, empty input and phase/network scratch.
[Rust comparisons](../../crates/inf-game/src/terrain_draw/emission/tests.rs)
match all four selectors, requests, native records, pending order and final
RNG; a separate required-birth failure check pins the reached prefix. These
are supplied-input static checks, not proof of the live event callback or a
matched cinematic particle sequence. The event owner around `00476520` and
matched scene comparison remain follow-ups.

## Pending furniture effect executor

[`world_furniture_effect::FurnitureEffectQueue`](../../crates/inf-game/src/world_furniture_effect/queue.rs)
owns `00436A60` admission and the
[`00436700` executor](../../crates/inf-game/src/world_furniture_effect/executor.rs).
World tick `0048770C` supplies raw microseconds, before the actor-manager
`+24` walk. Production now retains this process queue and services it before
intro actor callbacks; effects admitted by retirement first advance next tick.
`0048E890` clears W+12, then `00450210` calls queue clear `00436B80` and zeroes
shake enable `005483B0`, before player/task cleanup. Both results preparation
and world leave execute that owned subphase; the later cleanup stays skipped. The executor wraps +14 age, decrements +20 using
signed DWORD
comparisons, and consumes signed delays from 12-byte `[delay, callback,
argument]` program records. The original EXE program at `00532C88` pins
500000-unit delays; program words outside file-backed memory require a live
memory owner. Timer/cursor and earlier mutation effects survive host errors.

Each due burst resolves sparse/H11 data and preloads only a missing valid
H8 entry before taking its aligned raw-H12-height stack position. Its H11
allocation identity and mutable stack position survive across callbacks.
The [resident host](../../crates/inf-game/src/world_furniture_effect/resident.rs)
resolves current H11/0 and H8 registrations. Native `005276D8 → 004EFBF0`
genuinely returns null for a missing valid model; the inclusive count boundary
remains an explicit error. This differs from an unowned-loader fallback.
`00437650` first skips program records to the next greater signed state
argument, without consuming skipped delays. It then replaces only sparse
state low bits 0..1. `00436D00` writes the removal policy and can terminate
when furniture is absent; `00436E00` clears state/lookup; `00437750` returns
false; `00437780` sets state bit 3 and +20 to 7000000 microseconds. Cells
without flag 4 alias the [retained process record](#retained-furniture-query-process-state)
`005483C4`, borrowed by the actual resident host. A flagged missing record at
a native unconditional dereference is an explicit portable error boundary.

The native saved-next pointer is retained: an append from the original tail
waits until the next tick, while an earlier append can be reached after an
existing successor. Audio `00436DE0` now enqueues the current mutable stack
XYZ, selector, volume/pitch 65536 and flags zero into the shared process sound
list. It retains H9 allocation custody without executing playback or alias RNG
early. World radius `004375A0` uses the live damage services described below.
The pickup callback `00437880` drops the furniture's pickup
([WEAPON_PICKUPS.md](WEAPON_PICKUPS.md#furniture-pickups)). The RNG/terrain
`00436F10` consumers remain required host operations.

Camera effect `004375D0` is owned by the
[camera callback](../../crates/inf-game/src/world_furniture_effect/camera_callback.rs)
and the world host. It acquires the actual `00478E00` camera first, even for
a zero argument. A nonzero argument then compares the unsigned squared WORD
distance from the stack XYZ to the camera with the wrapping square of the
argument and skips the flash when it is larger. Otherwise it calls the
[world flash](WORLD_FLASH.md) `00487D70` on the stack XYZ with radius `F00h`
and style zero, which applies its own forward-radius test. The callback
returns AL one either way. The authored record at `00532D84` (argument
`1400h`) is exercised through the real queue.
Missing program memory and those callbacks fail the
furniture feature after reached writes. Selector-11 production retirement now
composes queue admission through the shared radius owner; this does not prove
that a later program executed or a cinematic matched retail. Focused tests
cover signed overflow, exact due
thresholds, raw-height/aligned coordinates, state skips, failure prefixes,
callback cursor/stack aliases, fallback mutation, and nested enqueue order.

### Furniture world-radius callback

The [callback adapter](../../crates/inf-game/src/world_furniture_effect/radius_callback.rs)
owns `004375A0`. It captures current process NULL `0054E894` before invoking
`004887D0` on the current world, forwarding the original mutable stack XYZ,
raw effect-block address and replication byte 1. It ignores the queue receiver
and cached H11 userdata. Only a normal lower return produces AL=1; upper EAX
is not a portable result. Missing process custody stops before block/XYZ reads.

The [production host](../../crates/inf-game/src/session_platform/furniture.rs)
borrows the executor's actual queue, terrain, sounds and RNG at that call.
It joins the [shared radius adapter](PROJECTILE_COLLISION_RUNTIME.md) with
live actors, mission, tasks, particles and process state. Its explicit furniture
origin rereads the borrowed stack position; projectile origins retain their
separate live particle-cell identity. Both read their original EXE block and
run terrain before actors. Nested furniture admission changes the same queue;
the enclosing executor still owns its saved-next traversal and failure stop.

The [22 native receipts](../../crates/inf-game/tests/fixtures/furniture_radius_callback_native.json)
execute the unchanged 34-byte adapter at FPCW `027F`, supplying only the lower
radius call. They cover argument order, raw block/source forwarding, original
position identity, lower mutations, AL-only return and stopped prefixes.
Concrete production tests separately execute authored program `00532DB4`:
sound 6, radius block `00532C68`, then required particle callback `00436D20`.
They verify real actor damage/statistics, nested furniture admission and shared
RNG, plus retained prefixes without retry. That subsequent callback's selector 1
requires factory `00466A50` and particle class `00526F80`; its complete birth,
update and draw remain separate owners. Admitting radius damage does not claim
completion of that furniture program or matched cinematic acceptance.

### Retained furniture-query process state

[`ProcessFurnitureQueryState`](../../crates/inf-game/src/world_furniture_effect/process.rs)
owns the physical four-byte record `005483C4`, init mask `0055F0DC` and five
40-byte basis stores at `0055EF44/0055EFD0/0055F004/0055F038/0055F06C`.
The supported PE declares these exact ranges in its loader-zero `.data` tail.
`PortSession` constructs one owner at startup and retains it across world,
screen, renderer and resource replacement. Queue cleanup `00436B80`, including
its `00450210` caller, does not recreate or clear this owner. Physical zero
basis bytes are distinct from the later `0040D6A0` constructor values. The
[actual reached query entry](FURNITURE_COLLISION.md#entry-and-registration)
reads mask/radius after its H12 capture; with bit01 clear it publishes the bit,
constructs all five bases and reaches the named
`LiteralRetExitRegistrationApproximation`. Native EAX is ignored, so no
registration-outcome gameplay failure is introduced. Repeated entry preserves
existing bases and record history. Startup, world replacement and cache hits
never perform this initialization.

Furniture effects and the read-only miss census borrow the same current
record. `00437650/00437780` update only state BYTE+2, preserving lookup BYTE+3
and WORD+0. The owned clear writes BYTE+2 then BYTE+3 without an earlier read.
The queue's existing cursor, timer, sparse branch and failure-prefix rules
remain in force. A zero lookup skips H11 and counterpart reads; it does not
establish that a collision query completed.

This proves startup storage and the represented writers, not a retail value
at an arbitrary later callback. Native `00436F10`, reached through
`00437060/004370F0`, now publishes lookup before RNG/state through the
[replacement owner](#furniture-replacement-callbacks). Computed aliases and
unowned callbacks remain outside this finite evidence scope. No
per-query zero, captured fallback value, contact result or geometry policy is
introduced. The actual `00429EE0` terrain gate and `0047C1D0` entry/scan now
share this owner. The first active pair still requires retained B origin/history,
selected surface-table callbacks, auxiliary execution and contact output;
initializing the mask and five bases supplies none of those fields.

### Furniture replacement callbacks

[`replacement.rs`](../../crates/inf-game/src/world_furniture_effect/replacement.rs)
owns `00436F10` and program adapters `00437060/004370F0`. The former adapter
first clears a cell's high flags nibble and passes lookup `FF`; the latter
passes `FD` without that earlier write. H12 flag04 selects the actual sparse
record; a missing flagged row returns normally without RNG, whereas an
unflagged cell uses the retained process record `005483C4`.

The owner publishes lookup, ORs one random nibble into state, clears terrain
flags, then consumes three RNG draws per loop, including the discarded first
draw. The second selects X offset `±128` relative to the original position;
the third's low byte repeats only above `E6`. A final draw replaces the state
low two bits. The terrain write intentionally reads an aligned cell's low
flags nibble but writes it at the original unaligned grid coordinate, keeping
the destination height byte. Coarse spacing must preserve this asymmetry.

The tracked generator executes the
original callbacks and real RNG, supplying only sparse lookup. Its
[24 receipts](../../crates/inf-game/tests/fixtures/furniture_replacement_native.json)
cover sparse, fallback and NULL records, both spacings, wrapped positions,
repeated loops, exact RNG endpoints and all changed cells. The production
queue test runs authored record `00532DD8` through its NULL terminator.
These synthetic receipts establish callback behavior, not retail allocator
history or complete-scene equivalence.

### The fence chain: 004373B0

The fence segments (H11 kind `0x0E`) run program `00532E30`: removal
policy 1, sound `0xB`, after 250 ms the debris burst (`00436D20(0)`), then
[`004373B0`](../../crates/inf-game/src/world_furniture_effect/chain.rs),
and last the clear `00436E00`. `004373B0` publishes the fine spacing
(`00450350`) and walks the seven by seven cells `0x80` apart around the
callback's XYZ, X outer and Z inner from `-0x180` to `0x180`. An unflagged
cell reads the process record `005483C4`. Each lookup whose H11 entry has
kind `0x0E` (byte +2) gets `00436240` at that point: offline, the record at
that cell queues its kind's program (the descriptor at `00532F88 + kind *
16`, +8) through `00436A60`, keyed under the fine spacing. `00450370` then
rereads settings +70, and AL is 1. Its first lookup at the XYZ, under the
current spacing, discards its result. One segment hit hard enough (their
channel 9 starts at 2500) thus takes every connected segment down.

Before the port had `004373B0`, its executor stopped the whole furniture
feature at the first segment's program, so the segment was never cleared
and no furniture effect ran for the rest of the session. Helniazine's pad
is fenced by 30 segments. In Infest04's fourth load the player detonated a
mine beside the fence. Its blast killed the nearer of the pad's two
[Bombs](HOLD_ITEMS.md#bomb-selector-29), whose blast queued nine segments
(`00436020`), and their walks queued the other 21 through `00436240`, along
the south, north and east sides, with 43 more calls finding the record
already queued
(`furniture-queue.js`,
`fence-queue04.txt`; the earlier reading took the Bomb's blast for a
second mine, but only one mine goes off there,
`bomb-life.js`).
[The test](../../crates/inf-game/src/world_furniture_effect/chain_tests.rs)
starts from the blast's nine on world 6 and runs the real executor at
125 ms ticks: its walks queue the same 21 in Infest04's order, each
segment bursts once, all 30 are cleared and the coarse spacing is back
after every walk. The mine, at Infest04's spot and from Infest04's RNG at
its last update, queues the two pylons' `00532D00`, and the Bombs it sets
off queue the rest of that tick's records in Infest04's order
([its test](../../crates/inf-game/src/session_platform/world_actor_update/grenade26_tests.rs)).
A build that logs `00436020` shows the mine's seven requests with
Infest04's strengths: 1594, 319, 1059, 609, 49, 319 and 154.

A fence brought down stays down: Helniazine's
[furniture marker](CONSTRUCTOR_ASSETS.md#helniazines-furniture-marker-selector-75)
(selector 75) marks its completion bit when the world is left without the
fence, and the next load's marker removes the fence again through
`00437110`, under the temporary fine spacing above.

## Scope and remaining owners

[`TerrainRuntime`](../../crates/inf-game/src/terrain_runtime.rs) owns an
independent prepared grid and `height_at([x, z])` queries. Its
`TerrainSampling` input separates `TerrainSpacing` from
`TerrainHeightScale`. World callers apply `TerrainSpacing::for_world(flags)`
and `TerrainHeightScale::from_world_flags(flags)` explicitly; there is no
default Detail setting hidden in the constructor.

[`WorldPlacements`](../../crates/inf-game/src/world_placement.rs) resolves
the actual resident H12 slot 0 and H14 slot 0 registrations, retaining their
source provenance and the canonical ordered instance-command graph. Its
`input(index)` and `inputs()` queries return terrain-adjusted constructor
inputs against that prepared grid. Borrowing the residency prevents replacing
its resources while this owner lives. A future constructor that changes the
terrain must supply that live mutation explicitly; these immutable queries
do not model mutations between object constructors.

World loading, H13 constructors and instance commands now have separate
owners in [object runtime](OBJECT_RUNTIME.md) and
[runtime foundation](RUNTIME_FOUNDATION.md). The retained world supplies
the actual prepared terrain to phase3 briefing and subsequent gameplay.
The [briefing owner](BRIEFING_RUNTIME.md) owns H14 records, screen-camera
movement, menus, panels, map and the phase4 Start transition; its remaining
presentation services do not follow from terrain preparation alone.
The phase1 `Title.avi` lifecycle remains separate in
[Title handoff](FRONTEND_RUNTIME.md#title-handoff).
