# Infestation briefing (phase-3) screen runtime

This document owns the phase-3 world preparation/briefing screen contract,
authored record program, menus, and bounded resident presentation. Phase
dispatch and resource lifetime remain in [Runtime foundation](RUNTIME_FOUNDATION.md),
the 9→3 completion path in [profile runtime](PROFILE_RUNTIME.md), and the
headless world loader in `world_loader` (tested against the recording with
recorded allocation inputs).

## Screen class and factory

| Address | Role |
|---|---|
| `0040A2E0` | Factory: allocates `0x32C` bytes, constructs through `0040A420` |
| `0040A420` | Constructor over base `00437F90` |
| `00520338` | Screen vtable, published by the constructor and restored by teardown |
| `0040AA00` | Class query `+18`: returns 3 |
| `0040AA10` | Reuse `+1C`: accepts only requested phase 3 (`cmp [esp+4],3; sete al`) |
| `0040AA20` / `0040AA40` | Deleting destructor / teardown |
| `0040C6A0` | Tick `+04`: finish gates, base input, scaled briefing-item tick |
| `0040B0F0` | Draw `+08`: renderer preamble, panel gates, scene and two menu controllers |
| `0040D1D0` | Presentation `+0C`, shared with the other screens |
| `00439470` | Held-bit dispatcher `+20` over controller-command slots and empty tails |
| `0040D4D0` / `0040D4B0` / `0040D4F0` | Held slots `+24/+28/+2C`: commands 4/0/1 to the player type-3 controller through screen `+EC`, owner `+88` |

Teardown `0040AA40` resets the vtable, clears the player word at `+3B +0x89`,
releases binding sets `00439D50`, destroys the menu controller `00453400`
and destroys the base `00438080`. There is no video handle to close.

## Constructor order

`0040A420` runs, in order:

1. Base `00437F90` (scheduler member, input context, analog references).
2. Byte/word field initialization across the `0x32C`-byte screen, including
   five `0040D6A0` sub-objects (vtables `00520384` then `00520378`) and a menu
   controller `00453380(0xffff, 100, 0x280, 0x2e4, 0x44c)`.
3. Vtable publish `00520338`.
4. Device enumeration `00439B30` over the inline twelve-slot arrays below,
   slot attach `00439D00`, context select `004947E0`.
5. H6 style publish through `00453E10`: selected-start slot 4 (inlined lookup),
   ordinary slot 5 and selected-end slot 6, the latter two through `00405540`.
6. Briefing inputs: the world resource at `DAT_0055EDDC` contributes its
   `+0x2C` relocated pointer to screen `+0x40` (the H14 briefing records),
   and the live player from `00488AA0` to screen `+0x3B`.
7. Menu construction: base menu `00452310` over `0052F1B0`, `004137D0`,
   derived menu `0040D560` over `0052F0D0`, push `00453440`, `004502C0`,
   shared animation reset `00439530`.
8. A conditional early return reads gameplay bytes (`+0x1EC` indexed by
   `+0x250`): unless bits 2 and 8 are both clear it returns without the
   final `0040D510`. The port preserves those status masks as explicit gates.

The five sub-objects own map (`+108`), terrain (`+174`), model (`+1E4`),
text (`+244`) and header (`+2A4`) animation. `briefing_runtime.rs` owns their
three-stage policy and record timing. `briefing_controls.rs` owns the two
menu controllers, while `briefing_draw.rs` owns the custom text callback.
`briefing_scene.rs` owns the original H8 panel frames, authored model preview
and decoration submissions. `briefing_sprites.rs` owns the direct background/progress
sprite arithmetic, and `briefing_renderer.rs` interleaves those layers with
the menus. `briefing_camera.rs` owns the current screen-camera vector, and
`briefing_map.rs` uses it for the production map backdrop, interleaving
`briefing_markers.rs` ordinary actor passes and the independent
`briefing_map_annotations.rs` tail after each pass. `briefing_terrain.rs` supplies the
terrain pane's context/backdrop; shared ground geometry and its remaining host
services are owned by [terrain runtime](TERRAIN_RUNTIME.md). Camera static and
its blinking disabled-camera label retain their separate pane policies.

## Authored record program

H14 world header `+28` is a DWORD count and `+2C` is a relocated pointer to
contiguous 16-byte records. Each record has duration in 100 ms units (`+0`),
kind (`+4`), an unconsumed DWORD (`+8`), and a payload pointer (`+C`).
`0040CE40` establishes these payload consumers:

| Kind | Payload and native meaning |
|---|---|
| 0 | WORD H0 text ID and two increment-policy bytes; either byte nonzero increments the previous ID instead of replacing it |
| 1 / 3 / 4 / 5 | DWORD animation flags for terrain / map / model / text; mask `0x2` selects stage 0, else `0x4` stage 1, else `0x8` stage 2 |
| 2 | Twelve bytes: signed model-slot WORD, signed XY WORDs, signed depth-bias WORD, signed panel-origin XY WORDs in thousandths of the viewport |
| 6 | DWORD copied as screen+1D4 camera-disabled flag; nonzero selects the disabled-camera branch |
| 8 | Twelve bytes: DWORD actor ID, signed heading in degrees, signed rotation WORD, unsigned speed WORD, then track/terrain bytes |
| 9 | No payload read; starts the 500,000 microsecond pulse |
| 10 | WORD map-scale target, shifted left by 9 by the native consumer |

[`briefing.rs`](../../crates/inf-formats/src/briefing.rs) validates every
consumed pointer against the original H14 relocation table and allocation,
retains source spans and raw words, and leaves the source bytes untouched.
Empty programs and pointer-free kinds do not follow ignored pointer fields.
Unknown kinds remain explicit records. The corpus test
[`briefing_corpus.rs`](../../crates/inf-formats/tests/briefing_corpus.rs)
covers all 33 world resources: mission selection 22's OVL 32 has no records,
and selection 18's OVL 28 has 50 records with explicit text IDs 875 through
887. These are authored corpus assertions, not production behavior values.

## Briefing menus

All three briefing definitions parse through the shared generic menu
decoder (`parse_frontend_menu`), with a retail corpus test pinning every
row and layout:

- `0052F1B0`, the pushed briefing menu (4 rows, initial selection 1): a
  header row (flags 9, generic text draw, no action) plus three selectable
  rows (flags 1) drawing H0 strings `0x157/0x158/0x159` through generic
  `00453A80` with thin action forwarders `0040A3F0` (`mov ecx,edx; call
  0040D1E0`), `0040A400` (`call 0040D3D0`) and `0040A410`. Two layouts:
  a 1-row header at `(80,60,900,60)` and a 3-row body at
  `(100,120,900,400)`, spacing 50, flags `0x82`.
- `0052F0D0`, a single custom row (flags `0x11`, draw `0040A350`, action
  `0040A340`) over a full-surface `1024x1024` layout (flags `0x9A`,
  spacing 100). `0040D560` builds it the same way it builds the pushed
  menu: base `00452310` plus `[0x17]=0` and vtable `00520394`.
- `0052F0E8`, four rows over the same single row record (identical draw,
  action and arguments on every row) with a `1200x1024` layout.

The derived `0040D560` constructor is shared: a second caller at retail
reuses it for the single-row menu, so these rows are not exclusive to one
screen. The root menu belongs to the external player type-3 controller;
the screen's `+2FC` controller owns detail/list text. Constructor `0040A420`
patches the root header argument to the mission selection plus `0xE0`.
`004137D0(1, menu)` supplies flag 1 to `00413230(3, flag, menu)`, so
`004140A0`/`00414140` select fade rate 32 and viewport `[0,0,1024,1024]`.
The text controller uses positive DWORD `0xFFFF` (65535) as its fade rate
and viewport `[100,640,740,1100]`.

The detail menu's `00520394` virtual `+08` tick is `0040D5F0`, whereas the
list menu retains the base `00452470` tick. The detail override first adds
signed `delta_us/1000` to menu DWORD `+44`, clears **both** bytes `+0E/+0F`
of every authored layout (the signed scroll-animation WORD), and then calls
`00452470`. The base advances that same DWORD once more, but advances row
age WORDs and rotates the persistent basis only once. The extra age step
uses per-call truncation and wrapping DWORD arithmetic, independently of row
age saturation. Production selects this explicit `MenuTickPolicy` when the
detail menu is constructed or replaced, preserving the controller's existing
fade and drawn gates. Root/list menus retain the base policy; the shared menu
resources do not choose it through a selection ID or numeric layout sentinel.
Focused tests cover positive/negative timing, both scroll bytes, a later
rotation failure's retained prefix, controller draw gating and repeated
Detail/List replacement for three authored H14 leaf programs with independent
synthetic mission-title DWORDs. These establish the
native menu tick policy, not a matched full-briefing visual comparison.

## Briefing action semantics

Row actions run with the menu descriptor in `EDX` (forwarded to `ECX`) and
resolve against the screen:

- Row 1 → `0040A3F0` → `0040D1E0` ("detail view"): resets the four
  `0040D6A0` sub-object animation fields (refilling `+0x14C/+0x288/+0x1B8/
  +0x228` from `+0x148/+0x284/+0x1B4/+0x224` when zero, clearing the age
  bytes), zeroes `+0xFC/+0x1CC/+0x1D0/+0x1D4/+0x23C/+0x1D8/+0x240` and the
  `+0x16C/+0x16E/+0x1E0/+0x1DC` words, sets `+F2=1`, clears the shared
  scratch `0052F018`, then constructs the detail menu (`0040D560` over
  `0052F0D0`, vtable `00520394`) and pushes it via `00453480`.
- Row 2 → `0040A400` → `0040D3D0` ("list view"): sets `+F2=0/+F3=1`, zeroes
  `+FC`, fills the shared scratch `0052F018..0052F098` with four
  (`0x380+i`, 0) pairs, then constructs the list menu (`00452310` over
  `0052F0E8`) and pushes it via `00453480`, with the same sub-object
  refill tail as row 1.
- Row 3 → `0040A410` → `0040D320`: a one-shot initializer. Only when screen
  `+F8` is zero it sets `+F8=2000` (a 2000 ms timer) and refills the same
  sub-object fields; otherwise it returns without effect. The armed
  timer decrements by the item tick's truncated milliseconds. When its
  remainder is at most that delta, it requests phase 4 and retains value 1.

`0052F018..0052F098` is one shared scratch: row 1 clears its first DWORD,
row 2 fills the four pairs (the first pair's `[-1]` write covers
`0052F018`). `00453480` (used for derived-menu pushes) differs from the
constructor's `00453440` push. The port replaces the text view while copying
the aliased first scratch row, then resets ages through the shared menu
constructor. `0040A340` forwards to screen slot `+2C` (Next on the player
controller). The custom draw `0040A350` skips full argument zero, negative
signed low WORDs and absent H0 text; otherwise it clears descriptor flag
`0x20` and calls shared left-text helper `00453810`. The adapter preserves
the generic menu projection, native font, shadow, wrapping, reveal timing
and descriptor-height feedback. Missing text is skipped and logged.

Menu attach `004137D0(flag, menu)`: with a nonzero flag it runs player
lookup `0046AE60`, sets a byte, calls `00413230(3, flag, menu)` and, when
that is nonzero, `0047DD20(0x300, 6, 0)` plus a `+0x34` byte. The briefing
constructor calls it as `004137D0(1, menu)`; its controller is distinct from
the screen-owned text controller.

## Tick and drawing owners

Tick `0040C6A0` (thiscall, `ret 8`), recovered from retail disassembly:

- Reads the world resource (`0055EDDC`, lazy `0055EDD8` load) and its `+0x28`
  record count; zero records finish through `0040D510`.
- Reads the mission `+A0`-indexed `+3C` status: bit 8 set finishes, bit 2
  set finishes (mirroring the constructor's gate bytes).
- Attract flag `0055F14C` set finishes.
- First run calls world-music resume `004872A0` and latches screen `+F5`.
- Dispatches base input `004392B0` with the scheduler timing pair, then
  ticks screen `+F0` briefing items through `0040C770`, with the constructor's
  mode scaling delta by `7/8`. The base age uses the unscaled elapsed input.
- Every finish path runs `0040D510`: `004133E0`/`004133C0`, request phase 4
  (`00438380(4)`), then `0048E7F0`, whose `00439530` resets the shared
  animation clock. A finish reached inside `0040C770` (its start timer)
  resets it before that item tick's own `00439560` advance, so gameplay's
  first tick reads the 7/8 delta and one whole one: 11 in the lab's Level 1
  at 125 ms. The port applies the reset in the tick's effect order
  (`BriefingTickEffect::Complete`).

`0040C770` ticks all five animations and the text controller, moves the
screen-camera vector and map scale, then ticks pulse/RNG, model age and
record playback. Camera heading advances after records, before shared
animation and the player controller. The header opens to stage 2; after base age exceeds 1000 ms,
detail playback begins. It advances at most one record per item tick and
discards surplus record age, so zero-duration records still require separate
ticks. Exhaustion enters the four-row list instead of finishing the screen.
List rows reveal over time, followed by the native cue/Next operation after
five seconds. Start remains the explicit normal completion route.
The item tail advances shared animation `00439560` with the same scaled
delta, before its header-gated player-controller tick. Construction resets
that clock through `00439530` and publishes the briefing's H6 slots 4/5/6.

Draw `0040B0F0` opens with `00439270` even when screen byte `+F1` suppresses
presentation. `+F1` begins at one and only normal `0040C770` clears it;
empty/skip completion therefore draws no menu. The root player controller
draws only when header current/requested stages are both 2; the text
controller has the same gate on its text stages. The port submits the native
arena preamble and those authored menu rows through the shared software font
renderer. Shared presentation `0040D1D0` clears the next surface with H7 slot
32; the briefing introduces no guessed background clear.

The scene adapter executes the recovered layer order: background, map
backdrop/frame, model preview/frame, shared terrain content/frame,
header/root menu, text frame/menu, decoration, then progress or Start text.
Known missing or unimplemented producers may omit their remainder under the
[typed containment policy](#briefing-feature-failures-and-batch-custody).
Hard renderer, pixel, queue and scratch failures stop later work while
retaining the reached prefix; error handlers never reset their batch.

`0040B0F0` opens and resets one outer batch around `0040C390` (map and frame)
and `0040B840` (authored preview and frame), then flushes once before opening
the terrain batch. Production retains that shared sort and command allocation;
it no longer flushes each feature separately. Known producer omissions retain
the preceding queued siblings and the reached model prefix through the same
final flush. Hard producer or replay errors stop before later batches. The
enabled terrain pane separately keeps backdrop, ground, lighting and objects
queued together until its native content boundary. Its static strips and
disabled-camera label use their separate original branch. An open frame does
not establish complete world contents or matched retail presentation.

The [map/model batch regression](../../crates/inf-game/src/briefing_renderer_tests/map_models.rs)
compares production against one source-ordered deferred flush using original
H14 payloads for Level 1's H8 117 and leaf 12's thumper H8/46. No Level 1
preview stops any more. After the map frame the thumper draws, since that
frame's context bases set its register 1
([briefing preview after the map frame](MODEL_RUNTIME.md#briefing-preview-after-the-map-frame));
with the map panel closed it stops at an omittable unowned register read.
At 640x480 and 1024x768 it checks pixels, retained command custody,
RNG, the single begin/reset/flush and the next closed-terrain bracket. Camera
and timing inputs are explicit fixtures; this closes the extra-flush policy,
not the custom-output writer or matched retail presentation.

## Resident presentation layers

All coordinates below come from the native consumers and constructor, not
from a screenshot or replacement layout. Models and sprites resolve through
the loaded world's original global H8/H3 registrations. No parser supplies a
briefing-specific orientation or mesh fix.

| Native owner | Submitted original resources and policy |
|---|---|
| `004737A0` background | H3 `0x5B7` effect quad with quarter RGB from H6 `0x3B`, then four H3 `0x29E` ordinary textured wedges; all depth DWORDs are 100000 |
| `00455080` / `00455360` frames | H8 100 for map/terrain and 99 for model/text/header; constructor center WORDs, focal pair multiplied by 32, and stage/countdown callback selectors 1–4 |
| `0040B840` model preview | H8 slot from kind-2 payload; signed XY, depth bias and viewport shifts, original header extent, model-age fade, screen-age yaw, and object callback `0043A270` |
| `0040C390` → `00471F70` map | Four H3 `0x5B7` effect tiles with unquartered H6 `0x3B`, descriptor `0x100`, depth `0x8000`, current screen-camera WORDs, native wrapping and stage-dependent clip; ordinary H3 7 actor markers then independent numeric/radius annotations after each tile, then restores full viewport before the map frame |
| `0040BB40` terrain prefix | Screen-camera view and native pane context, H6/37 and H12 lighting, optional H8 278 generic backdrop, then immediate `+84` H7/72 fill before the shared ground/light/object content flush |
| `0040AF50` camera static | Original H3 `0xBF..0xC2` one-pixel strips, ordinary `+9C`, descriptor 0 and depth `0x8001`; original width, random resource/flip order, and all-row versus pulse-selected-row traversal |
| `0040AAC0` disabled camera | The same static owner visits every pane row, then `0040C257` refreshes screen `+1CC` to 250000; the executable's literal "Camera disabled" uses H4/0 and H6/4 through `004A3AB0`, blinking from shared clock `005471C0/25` |
| `0040AD00` / `0040AAF0` decoration | H8 98 after both menus; H6 5 light bytes, screen-age rotation, and signed opening/Start fade WORD |
| `0040B0F0` progress | H3 `0x2AD` effect quads, H14 header `+24` milestone count, 2000 ms initial reveal plus 500 ms per item, original sine-table color pulse and depth `0x4000` |
| `0040B0F0` Start overlay | H0 `0x166`, font H4 0 and H6 5; centered using the signed WORD measured width, raw reveal flag 2 and age `2000 - +F8`; an armed Start suppresses progress |

The background's center is `(-width/8,height/2)` and its radius WORD is
`width*7/10`. Its row-wise `00490A80` transform is explicitly distinct from
the model local-to-world column transform. The direct sprite request names
ordinary `+9C` versus effect `+A4` submission and retains each vertex's depth
DWORD. Existing frontend/footer effect callers retain their mode-0 admission,
depth 64, descriptor and color policy through that same data-bearing API.

Panel frames preserve the native asymmetric three-stage callback, including
shifting both countdown DWORDs by ten before division. Closed stage-0/0
frames emit no model. Header and text menus still require current/requested
stages 2/2; partial opening shows the original frame without revealing rows.
Context `+0` is the live renderer pointer; `+6C` is the far color. The
`0043B4A0` template copy and decoration `0040AAF0`'s `+6C=0` write do not
change global coordinate shift `0055F250`. These layers preserve its incoming
value. They publish far color zero from the frame/preview template `00547554`
or the decoration's literal write. Terrain `0040BB40` has a separate gated
shift-16 write in its geometry branch; it is not a universal briefing shift rule.

### Terrain-pane context and backdrop

`0040BB40` tests screen `+1D4 == 0` for its world-view branch. The previous
`terrain_enabled` Rust name was backwards: the stored native values were correct,
and the owner now names this byte `terrain_camera_disabled`. Kind6 and
`0040C770` preserve their original values. Missing world geometry never selects
the genuine `0040AAC0`/"Camera disabled" branch as a fallback.

The [`briefing_terrain::noise`](../../crates/inf-game/src/briefing_terrain/noise.rs)
owner now implements `0040AF50` for both native callers. `0040AAC0` passes
literal -1: its signed WORD bottom-minus-top determines the row count, and
each row is visited in order without a row-selection random draw. The world
view passes unsigned `screen+1CC/2000`; each row first consumes `0048F060`
and computes `top + ((random*height)>>16)` with a wrapped signed DWORD
product. Empty random-row widths still consume that row-selection draw.
Each strip then consumes a random resource draw, selects H3
`0xBF + ((random>>4)&3)`, and reads the logical sprite's first physical
descriptor width at `+14`. Only after that lookup succeeds does it consume
the random bit `0x10` which selects the two X corner orders. X advances by
the unsigned authored width. Both width and height loop edges are exclusive;
the last strip retains its full width and the active renderer clips it to
the pane. This is a scene submission policy, with no asset/parser mirroring.

The streaming adapter submits each direct strip before requesting the next
one. Every return commits the already reached process RNG prefix and retains
preceding pixels. A known missing-resource boundary may omit the remaining
strips and continue only with healthy renderer custody. Pixel, queue and
scratch failures propagate through the enclosing panel; they do not begin a
replacement batch or permit later panel work. A zero authored width remains
an explicit error because the native loop would not advance. The camera-disabled branch does
not publish a world view, light, fade or coordinate shift. Its font label
retains the inherited projection origin but its corners are already absolute
pixels, centered using the signed WORD width returned by `004A4240`. The
blink uses the shared animation counter, one unit per 20000 microseconds,
not screen age in milliseconds. The native draw-time 250000-microsecond
pulse writer is retained for the next item tick. These owners generalize
over every loaded briefing and its authored presentation tier; they do not
encode mission selections or level-specific art.

Pure tests cover row/strip gates, signed wrapping, original corner order,
and RNG prefixes on lookup failures. Original-resource pixel fixtures cover
both 640x480 and 1024x768, compare the blinking label under the original H8/100
frame, and assert the fixed executable literal at `0052F1D4`. These are bounded
synthetic poses; no matched retail timeline or full briefing acceptance is
claimed. Complete world composition and environmental lightning
remain their own boundaries; `0040AF50` is camera static, not a replacement
for a world lightning service.

The caller decision matrix remains explicit:

| Caller | Context/global publication |
|---|---|
| Frontend scene | Doubled focal, shift0, scene context fade |
| Name entry | Original context focal/fade; inherited shift |
| Briefing frame/preview/decoration | Their own focal/center/fade; inherited shift |
| Briefing terrain geometry | Native pane context, signed screen eye, yaw then pitch0x1000, shift16 |
| Optional terrain backdrop H8/278 | Same view/focal; only global fade becomes INT_MAX/INT_MAX |
| Shared terrain0044AA10 | Republishes retained pane fade at0044AD71, then near/far intervals; normal return restores caller distances without publishing them |
| Following object models | Packet transforms retain published view/shift; common dispatch and Far fog retain published fade bounds/factor |

The pane center comes from signed screen WORDs `+1B0/+1B2` multiplied by live
dimensions and shifted10. Maximum percentages are34/38; opening and closing
retain the native integer divisions. The renderer clip narrows/clamps WORDs,
while context `+40/+44` retains raw DWORD right/bottom limited by surface size.
Context focal is original `00546F20/24` times the width/height percentages,
and context fade is `[0x1800,0x1A00]`. Eye `+160/+162/+164` sign-extends each
WORD; it does not substitute the selector16 gameplay camera.

Load0048DC30 copies H6/37 into0053DED0 and005474E0 at0048DDE7/EC. Phase3
retains that tint and H12's normalized00548380/84/88 direction. Lighting004501A0
publishes directional RGB first and then ambient RGB>>3, preserving the old
ambient's contribution to effect color. World `+5C3`
([infrared vision](GAMEPLAY_RUNTIME.md#infrared-vision)) keeps the last
world's value through the briefing, so after a warp with infrared on the
pane's terrain takes its night colours while the tint stays the load's.

Reached resource order is H7/74, optional H8/278, H14/0+10 from0048EDA0,
backdrop children, then H7/72. The generic0043A270 callback submits the backdrop
at `[screenX, header+10, screenZ]` with identity object basis. Only0048FB00's
globals0055F244/48/4C change to INT_MAX at0040C10A; retained context+64/+68 is
unchanged. Direct renderer+84 then draws the clip rectangle at depth100000,
descriptor0, using converted H7/72 before the shared deferred content flush.

The production `terrain_panel` owns both the outer `0040B0F0` bracket and
its `0040BB40` leaf. The outer caller begins the batch at `0040B2C1` and clears
depth before calling the leaf at `0040B2DA`, including when the leaf's two
visibility bytes close the pane. The leaf gate returns before its own pane
setup; it does not cancel the caller's reset.

The enabled pane keeps optional H8/278, direct H7/72 fill, shared terrain
`0044AA10`, object lighting `00479500` and object drawing `00479570` in one
content batch. Its native boundary is flush `0040C1FC`, begin `0040C20A`, then
separate depth clear `0040C21C` before camera static. The adapter performs no
extra backdrop flush. It restores the full viewport before the enclosing
frame and final flush `0040C371`. The preceding map/preview/frame submissions
share their own native outer batch and close it before this terrain bracket.
The shared `SoftwareScene::draw_terrain` receives the pane's retained
focal/viewport/center and fade, independently of the backdrop's `INT_MAX`
global override. Terrain domains, strip traversal, packet construction and
pixel consumers are shared with gameplay without mission or level checks.
The original camera-byte gate at `0040BE5A..0040BE62` admits lighting
publication `0040BE6A`, then grid clear `0040BE6F` before any pane-resource
lookup. The production prefix now clears the shared process grid there,
including when settings DWORD `+80` is zero. Closed panes and the disabled-camera
branch retain cells and anchors. Phase 3 calls no `0045B180` anchor setter:
anchors survive from the previous world draw, and no camera-derived replacement
is published. The shared ground-read policy admits those cells only with the
actual settings `+80` value and uses executable `00526068` RGB outside the grid.
This replaces `StaticTerrainLightApproximation`; loaded-world tint remains the
distinct retained color input. [Furniture](FURNITURE_RUNTIME.md) consumes the
same cleared cells regardless of settings `+80`, preventing stale prior-world
light from changing its ambient color during ground. The
[shared object-light pass](OBJECT_LIGHTING.md#shared-consumers-and-remaining-draw-boundary)
now runs afterward at `0040C1E0`, before the existing flush/reset and camera
static. It borrows the actual mutable arena, current settings and the same grid;
new deposits do not change ground already submitted. Active or unwritten
callback inputs retain the named per-object omission policy. The
[shared object draw manager](OBJECT_DRAW_RUNTIME.md) now runs at `0040C1EE`,
using the actual pane context, prepared terrain and retained process owners.
Source+10 custom vertices take the same world inputs as gameplay's (load
factor, terrain, H14 flags and byte `0053B890`), so a walker's leg solve
`00425870` runs on each briefing frame that draws it, as retail's does, and
its legs start gameplay solved.
Normal terrain return restores caller `[6144,6656]` only; its final far
publication remains `[8320,9984]`, from `[far*5/4,far*3/2]`, with the
corresponding factor. Common model dispatch and fog consume that published
state until an actual publisher changes it. Both object roots retain the
published model view and shift; the borrowed pane camera is separately
used for manager admission, flash and custom-vertex rebasing. Actor terrain
expansion and flash have their own explicit fade publications and normal
restoration order. The adapter performs no implicit caller-view/fade copy.
Its unsupported object features preserve reached prefixes and omit only the
current callback remainder before independent pane work continues. [Water](WATER_RENDERING.md) uses the retained
H14 flags and process animation clock. Both tile families reach shared E150 clipping. Unsupported item
primitives and complete entity composition retain per-feature omissions. Particle drawing has a
separate service and stays disabled in this adapter until that owner is joined.
Camera static and disabled-camera text execute after ground. The adapter
retains `0040C21C`'s separate depth clear before static; color and renderer
depth/blend policy survive while the strips can overlay preceding ground.
The static effect then restores full viewport before the enclosing frame.
This composition does not close full presentation acceptance; the ground owner's host
requirements remain explicit in [terrain runtime](TERRAIN_RUNTIME.md).

### Briefing feature failures and batch custody

The [typed feature policy](../../crates/inf-game/src/briefing_renderer/feature_errors.rs)
extracts scene failures through resource, terrain, menu/glyph and text wrappers.
Queue faults, failed replay, pixels, scratch identity/range/borrow/capacity,
resource registration and unknown errors remain hard. Native allocator301
therefore reaches the existing outer catch with its typed cause. The omission
whitelist admits only known missing-resource and explicitly unowned callback,
value or instruction boundaries; a general model failure is not permission to
reset a batch.

Every admitted producer captures its actual group state, running scene and
Ready scratch before work. Producer-only omissions require unchanged groups
and Ready scratch; they admit either the sole implicit root or the native
closed-root immediate tail. Model omissions require an implicit-root entry
and a healthy surviving root. `BriefingModelGroupClosureApproximation` closes
only groups created above that root through actual queue `end_group` before
`RegisteredMarkUnwindApproximation` retires the stopped draw's registered
marks. Both preserve command payloads, pixels, RNG, published scene state and
scratch bytes. `BriefingModelFeatureOmissionApproximation` then skips only the
current feature remainder. These portable recovery policies do not establish
native error unwinding or model completion.

The [shared terrain adapter](../../crates/inf-game/src/software_scene/terrain.rs)
retains its first typed renderer-custody failure at the existing omission
hook. All 16 fallible terrain/clipping host entries stop before further work,
and traversal returns that first cause before an outcome or later pane phase.
Furniture preserves typed renderer, required registration/metadata and source-custody
failures through this path while committing its actual RNG on every return. The
existing null-header item boundary, native geometry, particle birth and semantic
service omissions keep their separate owners.

[Batch regressions](../../crates/inf-game/src/briefing_renderer_tests/batching.rs)
require the caller reset for a closed leaf, one enabled content batch at both
authored tiers, preserved pixel-replay and allocator301 failures, and pending
original H8/278 commands surviving the thumper preview's unowned register stop
(drawn without the map frame) before the real ground flush. That preview has no EE/EF before its failing read;
it exercises implicit-root and registered-mark containment, not a genuine
new-group closure witness. The [terrain hard-custody regression](../../crates/inf-game/src/software_scene/terrain/tests/hard_custody.rs)
requires the first real allocator301 to survive strip containment. These are
bounded portable fixtures; full visible briefing and matched retail
presentation acceptance remain open.

[Lighting regressions](../../crates/inf-game/src/briefing_renderer_tests/lighting.rs)
exercise original Level1 terrain and pane resources at 640x480 and 1024x768.
They preserve seeded cells in closed/disabled panes, require the visible
world branch to clear them under both settings values while retaining anchors,
and verify that the furniture consumer then reads zero light. A separately
supplied post-clear light checks settings-gated ground RGB contribution; that
fixture isolates the read policy and does not establish the subsequent
object-light callback execution, matched retail lighting, or full briefing
acceptance.

[Renderer regressions](../../crates/inf-game/src/briefing_renderer_tests.rs)
use original Level1 assets at640x480 and1024x768 with explicitly synthetic
screen poses and clocks. They require H8/278 to affect the fully composited
pane against a fill-plus-frame baseline, and independently require the original
H8/100 enclosing frame to add pixels. The map-to-successful-H8/117 sequence also
requires both original frames and preview pixels after the shared native flush.
The thumper preview stops at its unowned register read when drawn alone and
draws after the map frame, keeping that frame's pixels. These checks cover
layer contribution and batch continuation, not matched retail acceptance.

The decorative globe reaches the recovered `EffectQuadFar` primary consumer
`004D9840`. With original H8 98 extent 392 and Z 1000, the settled native fade
interval is `[216,1392]`; `Z+extent` selects the Far table. Native Far effect
packets retain their additive RGB attenuation or specular-alpha vertex fog;
the draw does not substitute Near shading. The actual decoration fixture uses
synthetic screen ages 2000/7500 ms and explicit incoming shifts 0/8/16, preserving
each incoming global shift while clearing the context far color. It requires
original effect commands and changed pixels at both 640x480 and 1024x768.
The separate synthetic H8 98 preview continues to cover Near rasterization.
Renderer recovery and its bounded submission evidence belong to
[PIXEL_RUNTIME.md](PIXEL_RUNTIME.md#textured-triangles-nearfar-effect-quads-and-solid-quads).

The rotating preview and panel-frame renderer copies use the template's
terrain-free Ground/Project callbacks. Pane/backdrop and decoration submissions
retain the selected renderer alias, including a reached pane/world installer;
a closed or disabled pane preserves earlier callback selection.
The shared opcode `0x2C` signed command transport, consecutive destinations,
required selected callback and successful projection depth adjustment follow
[the recovered vertex handler](MODEL_RUNTIME.md#terrain-projected-vertices-renderer-2c-and-auxiliary-96).
The explicit table origins and renderer copy/publication proof belong to
[surface callback custody](MODEL_RUNTIME.md#renderer-surface-callbacks-and-table-custody).
Terrain-bound Project receives actual H14 world flags, process clock and
renderer XYZ, with its two genuine five-argument water calls recovered in
[terrain runtime](TERRAIN_RUNTIME.md#terrain-project-callback-0044ffa0).
Preview pixels alone do not establish full native terrain rendering.
All seven actual Level1 kind-2 payloads are exercised at model age 2000 ms
with synthetic process RNG, actual loaded terrain and an explicit isolated
unknown scratch fixture. Its bounded matrix is:

| H8 slot | Fixture result and remaining owner |
|---|---|
| 117 | Original authored preview produces pixels after owned frontend custom vertices and EB/EC depth groups |
| 157 | Produces pixels; its selector 4/6/7 beam triangles read stack residue and are skipped |
| 250, 294 | Original authored preview produces pixels |
| 223 | Produces pixels; its selector-10 beam triangle reads stack residue and is skipped |
| 686 | In the isolated unknown-storage fixture, child 687's far `0x07` needs the unknown fog byte of slot 18 copied from a near parent projection tail. The native loader-zero scratch knows that byte, so production draws it |
| 62 | Produces pixels; child 63's EE group keys on selector-8 residue (slot 118) and sorts farthest |

The residue policy and its Infest01 evidence are
[CustomVertexStackResidueApproximation](MODEL_RUNTIME.md#customvertexstackresidueapproximation).
Before it, factory, scientist and gate stopped at the residue and the briefing
omitted them; retail shows all seven. Across every world's briefing (leaves
12..34, tier 2, loader-zero scratch), each after the map frame, all 100
previews draw. Nemian H8/308 (leaf 25) is among them since its
[D4 quads](PRIMITIVE_RUNTIME.md#material-faces-d0-d4-d8-and-e0) draw, and so
is the thumper H8/46 (leaves 12, 20, 25). It interpolates by register 1
before writing it, and the map frame's context bases set that register
([briefing preview after the map frame](MODEL_RUNTIME.md#briefing-preview-after-the-map-frame)).

These are per-preview execution bounds, not missing parser framing.
Later synthetic release replay frames additionally reach
`SharedShadeTriangleFar` (`004A3900`, renderer `+60` → Direct3D `004C5210`). Its
primary packet, allocation-corner UV, fog, blend and depth policy are now owned
by [the pixel adapter](PIXEL_RUNTIME.md#shared-shade-textured-triangles), with
no Near fallback. At synthetic screen age 7,500 ms and model age 500 ms, the
original H8 117 preview reaches Far triangles and ordinary `TexturedQuadFar`:
thunk `00466160`, renderer `+A0`, Direct3D primary consumer `004D6570`. The
[ordinary Far-quad adapter](PIXEL_RUNTIME.md#ordinary-far-quad) owns its
three-block tail, allocation UV, literal `255-fog` blend branch and vertex
fog branch; it does not route through Near or Far effect shading. Differential
pixel controls omit one named consumer at a time while the actual fixture
requires a strict complete flush at both 640x480 and 1024x768. Descriptor `0200`
with renderer `+44` retains its explicit secondary-texture boundary. The same
bounded screen pose also exercises
H8 117/250/294 at model age 2,000 ms. These are synthetic poses through original
resources, not a matched retail timeline or complete animation acceptance.
Generic custom outputs retain their own boundaries. The 62 fixture asserts
the original `0x222D [118,8,44,0]` writer and `EE [118,65336,65535]` reader.
The isolated 686 fixture asserts child 687's `0x0A2A [18,1]` import and the
later edge using slots 18/16. Near projection leaves the fog byte untouched.
Its production byte custody is now owned by the process arena described in
[retained model scratch](MODEL_SCRATCH.md): startup loader zeros and reached
prior writes persist through allocation retirement and renderer recreation.
No per-allocation fog value is synthesized.

The separate [original 686/687 backend test](../../crates/inf-game/src/software_scene/scratch_tests.rs)
executes the original model branches and depth groups. It verifies the first
authored far edge returns normally with both fresh process storage and a
supplied Far/Far warm-up followed by Near/Far projection. The latter preserves
the reached nonzero Far fog through the full 24-byte parent copy. Separate
renderer fixtures share the same arena. An explicit test-only host error stops
execution after that real edge; these bounds do not establish the rest of the
preview, queue flush, complete producer history or a matched retail timeline.
A separate strict adapter pixel fixture uses original H8 98 with a clearly
synthetic preview payload; it proves the submission path without calling that
payload authored.

Draw copies the loaded world's shared RNG into the renderer, commits its
consumed prefix on success or failure, then removes the renderer copy. A real
Level1 jitter prefix followed by an injected later owner error tests that
custody; it makes no claim about a captured seed or complete draw cadence.

The map backdrop uses screen current WORDs `+160/+164`, not the selector-16
camera actor's position. `0040A420` zeros the current and target vectors;
kind 8 updates target X/Z and `0040C770` moves the current vector. The
[`briefing_camera`](../../crates/inf-game/src/briefing_camera.rs) owner
receives the loaded world's object arena, prepared terrain and original
sine table through linked tick dispatch. `0040D0CC..0040D125` searches current
G+10 list order, comparing sign-extended object `+2A` against the payload's
DWORD ID minus one. Zero ID or no matching object preserves the preceding
target; a match copies only object X/Z. Detail resets the camera-record
pointer and headings, but preserves current and target vectors.

Level1's kind-8 targets are world-placed
[selector14 pickups](PICKUP_RUNTIME.md#placed-selector14). Before the port
constructed selector14, no ID matched, so the camera stayed at the origin. The
map then centered on the wrong place and the camera never toured the course.
With selector14 constructed, the port's camera start, map center and tour
match the retail video at seven matched points from the opening through the
course. Any world whose briefing targets a class the port still skips loses
those targets the same way. The skipped selectors are listed in
[the gameplay census](GAMEPLAY_RUNTIME.md).

The last part of Level1's tour targets actor IDs 190 and 194: course target
189 ([selector 39](FAMILY40D800_RUNTIME.md)) and the selector-58 pad at 193.
With both constructed, the port's map follows retail through the end of the
briefing, including the red target markers over the second complex, at the
same matched points. The three Level-1 factories
([selector 15](FAMILY40D800_RUNTIME.md#selector-15-factory)) now also draw
as ordinary markers: their flags `4890002F` pass the marker test, as
Infest01's factories do at the first marker pass.

`0040C7E3..0040C85C` rotates a copied identity basis by the preceding heading,
then truncates stored forward X/Z times original float 1200 (`00520390`).
The two ground samples read aligned stored signed H12 height bytes at current
X/Z and the wrapped forward offset; each is maxed against the signed low
WORD of the shifted plane datum. They do not use interpolated `00450A30`.
The signed WORD error is target X/Z minus current X/Z, with Y equal to
current Y minus ground minus 400. Wrapped DWORD squared length feeds
`0048EFC0`; signed nonpositive overflow returns zero.

The unsigned payload speed WORD times four caps the X/Z error before signed
division. `0040CA57/0040CB00` extract each movement step from the signed
product with `delta<<12` shifted by 31. A nonzero original X/Z error forces
one unit if its resulting step is zero; Y uses the uncapped error and moves
only outside the inclusive -100..100 band, without a forced minimum. An
unforced track farther than 2000 shrinks map scale toward `0x10`, retaining
X/Z while scale is at least `0x2000`. Payload byte 11 suppresses terrain;
otherwise distance above 1000 enables it, and crossing back disables it and
starts a 500000-microsecond pulse before that tick's pulse decrement.

Production `0040C390` presentation now consumes these owned current X/Z
WORDs and runtime map scale. Camera-free ticks retain the source-initialized
vector; they do not introduce a live-world-camera substitute.
`00471F70` uses four original effect tiles and interleaves `00472D30` markers;
it does not read `TerrainMap` or call `00471D40`. The indexed-terrain palette
owner at `00470170` is a distinct display-surface path.

Production [`briefing_markers`](../../crates/inf-game/src/briefing_markers.rs)
streams an ordinary `00472D30` actor pass after **each** of the four tiles,
preserving G+48 order separately from the camera target's G+10 search.
Admission is `(object+2C & 0x10021) == 0x21`, or a nonnull virtual `+70`
or `+B8`. Every admitted vtable returns null at `+B8` but Capture the
Banner's [banner](NETWORK_RUNTIME.md#the-banner-and-its-base), whose
`00404610` returns itself; it blinks, skipped before any draw while the
scan's age over 15 is odd. At `+70` all return null but the
[pickup spawner's](ACTOR_RUNTIME.md#selector-53-pickup-spawners), which
returns itself. A spawner takes the jitter draws, moves the cursor and
resolves its H7 palette, then draws only when the final argument's low byte
is set; `0040C390` passes 0. Exhaustive class matches and an original-PE
vtable test bound that policy. Phase 3 excludes the
selector-2 velocity trail. The phase-7 map passes the Resource scanner and
draws both arms ([gameplay runtime](GAMEPLAY_RUNTIME.md#phase-7-map-screen)).
Coordinates use unsigned actor WORDs divided by
two, wrapping DWORD products with tile size, arithmetic shift 15 and signed
low-WORD offsets. Each pass preserves raw tile origin/size DWORDs separately
from the backdrop's truncated sprite corners.

Colors follow signed actor `+B4` through `004721A0`'s H7 palette. Case 2
first acquires H7 `0x56`, then additionally `0x5B` when network flag
`0055F448` is set. Flag `0x8000` applies the persistent renderer `+30` dim
mask from `004A49B0`. The ordinary path then acquires H6 `0x0E`, extracts
material channels using the actual display shifts, and applies the scan
intensity derived from actor Z and `screen_age_ms<<2`. `004736E0` emits H3 7
Near effect quads at depth 64 with descriptor zero. Reached jitter RNG
writes are committed even if a later palette/style/pixel read fails.

The retained process [map allocation](WORLD_MAP_RUNTIME.md) owns DWORD
`+24`; the loaded world's identity resolves that single jitter leaf. `0048DC30` creates a fresh `0x28`-byte
`00471AE0` owner, which explicitly initializes that DWORD to zero, and
publishes it at mission `+04` / gameplay G+1B4. `0048E890` destroys and
clears it. Thus briefing→gameplay retains the value; the intro→Level1 load
creates a fresh owner. `0048D980` writes `0x1FFF` after its nonnull-object
gate. `00420BE0` codes `0x0B/0x0C` write `0x3FFF` for an admitted network
owner impact, or `(range-distance)*0xFFFF/range` with wrapping signed DWORD
arithmetic for a nonzero range. These write policies are owned; their
invoking mission/damage callbacks remain separate gameplay owners.
`0048E742..0048E773`, at the end of world tick `0048E2E0` before actor
dispatch, subtracts signed `delta_us/200` and clamps at zero. The linked
gameplay adapter applies that decay; phase-3 item tick does not.

Nonzero jitter consumes two RNG draws per admitted actor per tile, using
random remainders plus the preceding **projected screen** X/Y as wrapped
position WORDs. On the first actor, `00472E06` reads an unwritten previous-X
stack local; initial Y is the caller's final zero argument. That first-X
custody remains required. Production fails closed at that reader, retains
the reached first RNG draw and logs the skipped pass; it does not force live
jitter to zero. Constructor-owned zero jitter bypasses that reader and
renders the normal ordinary markers without RNG. The independent annotation
tail is reached only after that ordinary pass completes, so a failed first-X
read also preserves the native prefix before annotations.

### Independent map annotations

Production [`briefing_map_annotations`](../../crates/inf-game/src/briefing_map_annotations.rs)
owns `00472D30`'s tail at `00473258..00473479`, after ordinary G+48 actors
for each wrapped map tile. It consumes the [selector-87 list and state](OBJECT_RUNTIME.md#selector-87-map-annotations)
in native `0048C6F0` order, independently of actor admission or camera-target
search. The pass first resolves H4 font 0 and H6 style 0 even when the list
is empty. Tile clipping remains active through both actor and annotation
passes; the caller restores the full viewport before drawing the map frame.

For unsigned position WORDs X/Z, tile size `[w,h]` and origin `[ox,oy]`, each
offset is the signed low WORD of `(word/2 * size) >> 15`, with a wrapping
DWORD product and arithmetic shift. Center is `[ox+offsetX, oy-offsetZ+h]`
using wrapping DWORD arithmetic. The label consumer retains those DWORD
coordinates; sprite corners consume signed low WORDs.

A nonzero label DWORD selects signed decimal formatting with executable
literal `%d` at `0052F1D0`, then `004A3AB0` single-line text with glyph
callback `00439DE0`. It does not look up H0 text. Only this branch resolves
H6 style `0x24`: scan is `(unsignedZ - (scan_age<<6) - 0x8000) & 0xFFFF`,
where the caller supplies `scan_age = screen_age_ms<<2`. Each RGB byte is
the low byte of `(style_byte*scan)>>15`; overflow wraps rather than clamps,
and the high style byte is preserved. Command 2 writes a zero-extended WORD,
while this draw kernel preserves the entire DWORD's signed consumer.

A zero label instead draws H3 sprite 7 through `004736E0`: radius is the
wrapping signed DWORD product `unsignedRadiusWORD*h`, divided by 40000
toward zero. Four corners use center±radius, depth 64 and descriptor zero.
RGB comes from the annotation's three bytes; the high byte comes from the
caller's final argument, which phase 3 supplies as zero. This branch reads
no H6 `0x24`, and neither annotation branch consumes RNG.

Tracked draw checker runs unchanged
`00472D30`, `0048C6F0` and `004736E0` with a supplied empty ordinary actor
list. Its [eight compact receipts](../../crates/inf-game/tests/fixtures/map_annotation_draw_native.json)
record list traversal, resource reads, geometry, colors and label arguments;
formatter, line-text and renderer `+A4` remain explicit external hosts.
Together with the [25 class/list snapshots](../../crates/inf-game/tests/fixtures/map_annotations_native.json)
from the construction checker, these validate
separate state and draw owners, not complete native heap or scene custody.
Production pixel tests separately use actual leaf-28 numeric annotations and
leaf-26 radius/RGB annotations with original H4/H3 resources at 640x480 and
1024x768. These are bounded authored-input checks, not a retail visual
comparison or full briefing acceptance.

Parser bounds, timeline/controller tests, native font pixels at all five
presentation tiers, representative closed/transition/open frame pixels,
background/progress/Start pixels at 640x480 and 1024x768, and linked mission-18
input/draw tests cover these bounded owners. The exact authored-preview
failure matrix keeps unowned consumers visible; original decoration
additionally exercises the Far effect pixel owner. Camera tests cover native
lookup order, retained targets, signed steps and overflow, zoom/terrain gates,
and stored-cell/plane sampling. Map backdrop fixtures check original pixels
at open/transition clip gates; production feeds the same planner from the
screen-camera state. Marker fixtures cover palette order, admission,
WORD/DWORD geometry, scan/dim color, jitter prefix errors, actor-list custody
and original H3 7 pixels at both presentation tiers. Retail visual comparison and full
New-Game acceptance remain open.

## Observed completion path

A bounded read-only replay from the accepted world-loader entry
(`412C94:1CBC`) pins how the recording's first briefing actually completes.
Bundle `.tmp/infestation-ttd/20260927-205015-495-BriefingComplete/` completed
in 5.4 seconds with `command-line.txt`, `briefing.windbg.cmd` and
`transcript.txt` retained; the one-off driver is
`runtime_re/scripts/briefing-complete-probe.ps1`. Hooks on constructor `0040A420`,
tick `0040C6A0`, completion `0040D510` and every phase request, eight-hit
budget, thread unfiltered:

| Ordinal | Observation |
|---|---|
| 0 | Constructor on screen `03838A30`, thread `C23C` (during the 9→3 transition) |
| 1 | Tick on the same screen: no `0040D510` ran in between, so the constructor's conditional gate saw both bytes clear |
| 2 | Completion `0040D510` from the tick path |
| 3 | Request 4, current=3, pending=0, caller `0040D542` (inside `0040D510`): accepted |
| 4 | Request 8, current=4, caller `0048759C`: accepted (gameplay to results) |
| 5 | Request 8 again while pending=1, same caller: dropped by first-request-wins |
| 6 | Request 3, current=8, caller `0042CFF5`: accepted (results to briefing) |
| 7 | Second briefing construction on screen `0383E570` |

A single briefing tick precedes completion: with the world just loaded, the
tick's record/flag gates fail at once and it finishes straight through
`0040D510`. The constructor gate never fires in this load. Ordinals 4–7 are
entry observations beyond the briefing question: they log requests whether
accepted or ignored (including a dropped duplicate) and prove a second
briefing construction, but establish no gameplay/results lifecycle, no
cadence and no second-mission semantics. The port now admits 9→3 under the
labeled fresh-heap policy below; this observation proves the empty intro
briefing's immediate completion, not a general one-tick briefing policy.

## Authored briefing binding tables

The constructor builds its arrays inline as a `0x60`-byte stack struct:
digital roots at `+0x00` (slot 0 `00520028`, slot 2 `005200E0`, slots 6/7
`00520238`, slots 8/9 `005202A0`) and analog pointer cells at `+0x30`
(slot 2 through the shared cell `00533398`, slots 8/9 through the shared
cell `005333E8`). Decoded record counts are 8 / 16 / 4 / 4. Non-system
records drive the briefing callbacks `0040D660/70/80/90` (descriptors
`0052EFF0..0052F008`, flags 2); the keyboard table ends with the three
shared Alt system records (`00439260` exit, `00437D30` fullscreen toggle,
`004451D0` no-op, descriptors `00533408/10/18`).

One stack slot is never written: analog slot 7 (`+0x4C`, old `esp+0x68`)
has no store in `0040A420`, so it holds indeterminate stack contents. It is
encoded as empty, consistently with the frontend's six null digital slots:
no authored device type consumes it, and on provable hardware no native
type-7 device enumerates. A type-7 device would read that garbage as a
pointer cell exactly as retail does; that path is explicitly unsupported.

## Session LoadWorld execution boundary

The 9→3 transition effect `LoadWorld` (`004874D0` → `0048DC30`) borrows the
actual prepared arena, mission, listeners and player/input carriers. The
[retained load policy](WORLD_LOAD_LIFETIME.md#borrowed-loadworld) applies the
mission prelude before activation and the actual manager cleanup after map
publication. Explicit saved-import fixtures remain a separate mode. Initial
current world 1 is the [mission constructor's](WORLD_MISSION.md#native-mission-constructor)
store at `0048DA88`, not a captured first-load default. The actor allocations
still use the documented fresh-heap policy (`fresh_heap.rs`: zero-fill
`[0,0,0]` for every construction's pre-context WORDs, explicit labeled
approximation). Allocator-history ownership (which freed block each
construction receives) is a follow-up. A full consumer audit of the unowned
bytes (three `u16` WORDs per construction at
`ActorConstructionRequest::allocation_position`) narrows the gap to two
residual fields:

| Consumer (production code) | WORD-dependent? | Evidence |
|---|---|---|
| Terrain classification (`classify_actor_terrain`): flags only | Outcome-identical on the observed load | All 30 placement endpoints retain `flags & 60000000 == 0x40000000`, which is also the miss-branch outcome; the vehicle records `0x48106927` |
| Initial `position_words` store (`allocate_actor`), then factory overwrite from authored context (`initialize_base_context`, always `Some` in `load_world`) | No: all 118 placements end on authored XYZ | `object_runtime.rs` overwrite; census endpoints confirm |
| Vehicle final position (respawn wiring from `prepare_player_spawn` start/override through context) | No | `player.rs` respawn; recorded `(0x724E,0xFFFC,0xB559)` reproduced from start state, not WORDs |
| Vehicle wave bit (`allocate_selector2_actor`: `byte_209 = \|y − wave\\_height(x,z,clock)\\| < 64`) | **Yes, one bit** | Recorded `byte_209 = 0` with WORDs `[0,2,0]`; the constructor reads WORDs unconditionally (`UnknownAllocationPosition` otherwise) |
| Home positions (admission copies post-factory positions; commands 20/21 terrain-sample payloads) | No | `family428650.rs` admission/commands |
| Instance-command conditional reads | No: they read overlay bytes via `RetainedActorAllocation`, never heap WORDs | `allocation.rs`, `commands.rs` |
| Camera position under miss inputs | Open: camera-draw reads of a `None` position are unowned | `finish_world_placements` camera path |

Consequences:

- Baking the recorded WORDs into production would compensate the unresolved
  allocator with captured data (banned). Zero-fill is an explicit labeled
  approximation: it sits inside the proven reproducing band for both outcome
  dimensions (30/30 endpoints `Set40000000`, 16384/16384 cells, joint y-band
  32850/65536 with mismatches only at signed-negative heights below about
  −82), matching the fossil distribution (virgin-zero fresh subsegments,
  usually-zero LIFO reuse). It does not claim exact heap leftovers.
- Every reload, second mission and death respawn reads fresh heap leftovers
  per construction. Allocator-history ownership (tracing which prior occupants
  leave the WORDs each constructor reads) remains a follow-up; it is not a
  phase gate. Unknown subsystems fail closed per-feature (skip + log), not
  per-phase.
- Measured robustness (world-load probe
  `allocation_word_outcomes_across_the_triple_space`, retail corpus only):
  at the recorded cell the joint reproducing band covers 32850/65536 y
  values, with mismatches exactly at signed-negative heights below about
  −82 (far above the plane, flipping to `Set20000000`); at the recorded
  height every sampled map cell (16384/16384) reproduces. Small-magnitude
  WORDs are thus unobservable in these two outcome dimensions, which tells
  future allocator work how precise its model must be. The headless
  `world_loader` tests (recorded inputs) remain the evidence vehicle;
  `world_backend` (`fresh_heap_first_world_loads_like_the_recording`) proves
  zero-fill loads 118 placements + camera + vehicle with a respawned vehicle.

## Heap-fossil structure (from the accepted placement census)

No new capture was needed: the accepted census transcript already retains
all 30 `(object, entry WORDs)` pairs with TTD positions. Read in placement
order, the object addresses are not bump-allocated — they step mostly
downward in strides of 512–1968 bytes, with larger jumps in both directions
(±6K–60K) marking heap-region changes. That is the Windows heap free-list
signature: fresh subsegments open upward, freed blocks return LIFO:

| Observation | Count |
|---|---|
| First construction (prior boot/menu heap history) | 1, nonzero `[0,1,0]` |
| Upward jumps (fresh subsegments) | 5, every one with zero entry WORDs |
| Reuse steps with zero WORDs | 18 |
| Reuse steps with nonzero WORDs (placements 46, 47, 49, 51, 80, 82) | 6, matching the census nonzero list minus the first |

The nonzero leftovers are all small-magnitude (`00FB`, `00CB`, `00FE`,
`0065` in y; pointer-shaped `3ECE/3ECA` and `3B80/3F4B` in x/z) — prior
payload fragments, never wild values — and sit inside the robust band
measured above. So the fossil distribution is known: virgin-zero pages on
fresh subsegments, usually-zero LIFO reuse, occasionally small-magnitude
prior payloads. What remains unowned is the *sequence* itself: which freed
block each of the 32 constructions receives depends on the whole process
allocation history (boot, menu, frontend, load), which the port does not
replicate. A heap-history model would have to replay that order, not just
the value distribution.
- The session admits 9→3 with fresh-heap WORDs (`PortSession::admit_transition`,
  `world_backend::load_offline_world`), retaining arena/mission/listeners across
  briefing → gameplay. The briefing (`briefing_screen.rs`) binds the selected
  H14 program: mission 22 finishes in its first tick, while mission 18 plays
  its authored records and waits for Start. The gameplay
  screen (`gameplay_screen.rs`, the player block's vehicle update and `world_scene::draw_world_frame`)
  drives with arcade vehicle input as an explicit approximation; the
  [shared native camera](CAMERA_RUNTIME.md) ticks on every loaded world.
  Native allocator history, vehicle physics/bindings, special-view activation,
  HUD, collision, weapons and audio stay follow-ups.

## Recorded New-Game chronology (intro identity pinned)

Bounded read-only replay
`.tmp/infestation-ttd/20260928-001950-580-IntroInterval/` (16-hit shared
budget from the `9->3` request at `41284B:5B3`, hit limit, thread `C23C`)
hooks the briefing factory/tick, the video graph/play entries, world
effects `004874D0` (logging gameplay `+0x1E0`), the loader
`0048DC30` (logging selected/current), and phase requests. Observed
ordinal order:

| Ordinals | Event |
|---|---|
| 0–1 | `004874D0` with mission field 22, then loader with selected 22 |
| 2–3 | Briefing factory, then a single briefing tick |
| 4 | Request `3->4` from caller `0040D542` |
| 5–6 | Request `4->8` from caller `0048759C`, then its duplicate dropped by the pending guard |
| 7 | Request `8->3` from caller `0042CFF5` |
| 8–9 | `004874D0` with mission field now 18, then loader with selected 18 |
| 10–15 | Briefing factory, then five briefing ticks |

Conclusions, all within this bounded window: the post-New-Game intro
is the in-engine cutscene on mission 22's map, played in gameplay
phase with no player control — not the phase-3 briefing. The
mission-22 briefing exits after one tick plus present (skippable,
no content shown); the cutscene then plays under phase 4 until the
`4->8` completion request, and the five-tick mission-18 briefing
that follows is the read one for the next map. No video graph or
play entry fires in either arc, and no `BannerLo/Hi.avi` files
exist beside the recording or in the repo (only `Title.avi`;
`2.ovl` holds filenames alone). Mission end lives in the world tick
(`4->8` from beside `004875A0`, matching `00487560`'s completion
request). Native `0048E820` writes next selection `W+30` (gameplay `+0x1E0`)
to 18 before `00487560` requests phase 8; current world `W+A0` remains 22
until the next load. The bounded trace observes selection 18 at the later
`004874D0` and loader calls, but does not time that field's write. Numeric
world IDs are raw native fields, never UI level
labels. This pins the M2 phase/world chronology. The session's fresh-heap
approximation admits phase 3 without claiming native allocator history.

The world loader fires exactly twice per New-Game arc, so its
selected value is the map cue: `selected=22` (cutscene) then
`selected=18` (level 1), and gameplay entry does not reload —
level 1 runs on the briefing-preloaded map. Tracked replay
`.tmp/infestation-ttd/20260928-134832-259-PhaseTransitions/`
(`query-ttd.ps1 -Preset PhaseTransitions -StartPosition
'41284B:5B3' -MaxHits 16`, 33.154 s) from the `9->3` request:
transition `9->3`, loader 22 (`current_before=1`), request plus
transition `3->4` (cutscene entry), request `4->8` (plus its
pending-guard duplicate) plus transition (cutscene end), request
plus transition `8->3`, loader 18 (`current_before=22`), request
plus transition `3->4` (level-1 entry), then `4->5` and `5->2`
with no further loader hit through the 16-hit cap. Full hit table
in the capture ledger.
