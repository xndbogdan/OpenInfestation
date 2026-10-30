# Round models, and treads with real lugs: plan

Status: step 1 built (2026-10-24, `src/round_parts.rs`): the buggy's and
the truck's wheels, both sets, split onto their circles in the scene
feed, as designed below. The hubs round with the tyres (question 1). In the chase view at 4K, the buggy's rim edges take 5 pieces
and the truck's 6. The cost is about 0.05 ms a 4K frame (the truck
turning, 5.84 ms before and 5.88 after). Evidence:
`.tmp/plan-round/round_wheels.jpg` and `round_morph.jpg`. Step 3 built
the same day (`lugs_wheels.jpg`, `lugs_drive_morph.jpg`): real lugs 2
units deep on wheels over 40 pixels in radius, at no measurable cost.
Step 2, relief, is left out: below that size a lug spans 2 or 3 pixels.
Step 4 built the same day too (`ducts_jet.jpg`): the hovercraft's ducts
and the skimmer's jet, about 0.04 ms a 4K frame. Step 5's sphere split and
the menu's planets too (`menu_planets.jpg`); its world parts wait for
picks from `world_parts_review.jpg`. Code lines are as of `86c22b2f5`. Other sessions are editing the
tracer's shaders, so expect small drifts.

## What was asked

Some items want to be round but cannot, because retail had limited
geometry; for example, the ACAM's wheels. The threads on the tyre would be
expected to be tessellated, but retail had none. The aim: make round models
actually round, and tessellate some parts where it makes sense.

Ground rules (from the RT plan): only Vulkan with path tracing changes.
OpenGL and the software renderer keep retail's look. The game does not
change: no position, contact or timing moves.

Words used below:

- **Facet**: one flat face of a model. A wheel of 7 facets is a 7-sided
  prism.
- **Outline**: the edge of a model against what is behind it (its
  silhouette).
- **Split** (tessellate): cut a face into smaller ones. On its own that
  changes nothing; the new corners must also move onto the true shape.
- **Displacement**: moving the new corners in or out by a pattern (here
  the tread's lugs), so the pattern becomes real geometry.
- **Relief** (a bump or normal map): changing only the direction a
  surface is lit from, pixel by pixel, so a flat surface looks raised.
  The outline stays flat.
- **BLAS**: the tracer's search tree over triangles (bottom-level
  acceleration structure). The models' triangles share one (masked
  sprites aside), rebuilt every frame.

## How we looked

The port ran as a probe from `.tmp\plan-round\`
(hidden, silent: volume words 0), Vulkan with path tracing at
3840x2160, fixed 16 ms steps. Each run folder is under `runs/`, with its
`start.json`, `keys.json` and log. Model data came from the extracted H8
corpus (`extracted/models/manifest.json`) and the sprites from
`extracted/sprites/`, read with `python -I`.

| Run | Scene |
| --- | --- |
| `A_w18_buggy` | Level 1 (world 18), start 0, the buggy from behind (frame 120). |
| `C_w18_buggy_turn`, `C_w18_buggy_turn_gl` | Up and Left held from frame 20, Up let go at 50: the chase camera swings round and shows the wheels' sides (frames 70, 85). Vulkan, then OpenGL. |
| `D_w18_truck_turn` | The same in truck form. |
| `E_w18_form3`, `form4`, `form5` | The same as hovercraft, VTOL and skimmer. |
| `M_menu` | The main menu, Escape over the intro videos (frame 700). |

Two traps: forms 2 to 5 need their upgrades in `start.json`
(`"upgrades": [1, 2, 165, 166, 167]`), or the start quietly stays a
buggy. And V is the scanner (a binocular view), not a free camera, so the
turns are what show the wheels' sides.

## What we found

### 1. The wheels are small models of their own (verified)

The vehicle model `acam` (H8 117) draws each form's body as a child. The
buggy's body `wheelmode1a` (118) and the truck's `wheelmode2a` (119) each
draw four axles, and each axle draws one wheel. Each body has two sets,
chosen by its register 17, the morph's progress: settled (`FFFF`) it draws
the "simple" set, during a form change the other.

| Form | Settled: axle, wheel | During a form change: axle, wheel |
| --- | --- | --- |
| Buggy | `stubaxlesimple` (130), `huskywheelsimple` (134) | `stubaxle` (129), `huskywheel` (133) |
| Truck | `bigstubaxlesimple` (132), `bigballwheelsimple` (136) | `bigstubaxle` (131), `bigballwheel` (135) |

The axle's rod is an edge (opcode 07, a strip the tracer already lights
as a rod, `scene_capture.rs:512-606`). Before drawing the wheel, the axle
turns its saved frame about the wheel's own X axis by the track phase
(opcode 03 `[0, 0, angle]`, `model_execution.rs:844-857`,
`model_context.rs:86-93`; the angle is getter 3, which animates wheels and
tracks, or in the settled set the body's register 3, plus or minus it for
each side), and the wheel takes that frame (orientation 6,
`model_context.rs:101-110`). In the settled set the body first turns the
two front axles about the upright axis by getter 4, the turn in the
wheeled modes (`OBJECT_MODEL_CALLBACKS.md`, selector-2 branches): the
steering. So the spin and the steering live in the wheel's frame, not in
its corners.

### 2. How each wheel is built (verified)

**Buggy, `huskywheelsimple` (settled) and `huskywheel`: 7 sides.** In
model units (world units):

- Tread: a ring of 7 corners at radius 32, at x = +12 and its mirror copy
  at x = -12, so the tread is 24 wide. Seven quads, sprite 195.
- Side wall: the rim ring to a hub ring of radius 16.5 at x = 19. Seven
  quads, drawn on both sides (mirror bit), sprite 196.
- Hub: seven triangles from the hub ring to a centre at (10, 0, 0),
  sprite 197: a cone set into the wheel. On the inner side the settled
  wheel has a flat disc instead.
- Every corner has its own normal. The rim's normals lean 45° out to the
  sides, so the tread is lit like a rounded balloon tyre. Around the
  wheel the normals point straight out, so the shading is already smooth
  round; only the outline is a heptagon.
- In the form-change wheel the rings are not stored at full size.
  Register 12 = max(getter 2, `0x9000`) (`F6`, `model_values.rs:226`)
  scales each ring point from the centre. Getter 2 is the morph's progress
  (`MODEL_RUNTIME.md`, VehicleModel117), so the wheel grows from 56% to
  full size during the change.

**Truck, `bigballwheelsimple` (settled) and `bigballwheel`: 8 sides.**

- Tread: 8 corners at radius 50, x = ±8 (some ±9), sprite 195.
- Shoulders: the rim to an inner ring of radius 41 at x = ±26, also in
  the tread sprite 195. The whole outside of the "ball" carries lugs.
- Hub: eight triangles from the inner ring to (7, 0, 0), sprite 198.
- Corner normals point straight out; the form-change wheel starts at 25%
  (`max(getter 2, 0x4000)`).

Script: `scripts/dump2.py` in the probe folder. The snapshots show the
settled wheels.

### 3. The tread sprite is a straight-across lug pattern (verified)

Sprite 195 is 36x40 in detail banks `1d2` to `1d4` (18x20 in `1d0`,
`1d1`). It is four identical periods of nine columns; each column's mean
is 27, 13, 16, 38, 46, 74, 76, 76, 57 (a dark groove rising to a lit lug
top), and every row's mean is 47. So the lugs are bars straight across
the tread. Each quad maps the whole sprite (u along the rim, v across), so
there are four lugs per facet: 28 round the buggy's wheel (one every 7.2
units), 32 round the truck's (one every 9.8). The side wall's sprite 196
has four notches along its top edge, the same lugs seen from the side
(figure 07).

### 4. How much the facets show (verified, snapshots)

In the chase view at 3840x2160 the buggy's wheel is about 95 pixels in
radius (3.0 pixels a unit) and the truck's about 170 (3.4 a unit). The
gap between a facet and the circle through its corners is
r(1 - cos(180°/sides)):

| Wheel | Gap in units | Gap on screen |
| --- | --- | --- |
| Buggy tread (7 sides, r 32) | 3.2 | about 9.5 px |
| Buggy hub (7 sides, r 16.5) | 1.6 | about 5 px |
| Truck (8 sides, r 50) | 3.8 | about 13 px |

- Figure 01: the turning buggy under Vulkan and OpenGL. The front-left
  wheel is a heptagon, hub included, and its traced shadow on the sand is
  one too. OpenGL shows the same heptagon under a retail blob shadow.
- Figure 02: that wheel at 3x. Seven flat tread faces, the four painted
  lugs per face making a chevron with the side wall's notches.
- Figure 03: the truck. The octagon reads on every outline, and in the
  shadow.
- Figure 04: from straight behind, the most common view, the tread faces
  the camera. The outline kinks where facets meet; the lugs are painted
  stripes.

**The ground.** The game puts each wheel's centre a fixed reach above the
ground it finds under the hub (`VEHICLE_DRIVE_RUNTIME.md`, "The wheels
`00415D30`"). The reach is 32 in all 5,736 recorded wheel calls
(`tests/fixtures/vehicle_wheels_infest01.json`), the buggy wheel's radius:
a round buggy wheel touches the ground exactly, where today's heptagon
floats 0 to 3.2 units above it as it turns. The truck's reach is scaled
to 52 against a radius of 50, so its round wheel would sit 2 units up
(the octagon 2 to 5.8).

### 5. The capture already knows each wheel's exact axis (verified)

Every primitive the scene capture copies comes with its model's own
transform: where the model's origin is and its three axes, in the view's
frame (`ModelPrimitiveRequest::transform`, `model_execution.rs:74-85`;
`ModelTransform`, `model_context.rs:182-187`). Basis words 1-3, 4-6 and
7-9 are the model's X, Y and Z axes (`model_projection.rs:157-169`). The
wheels' rings are centred on their model's X axis (the buggy's corners
at radius 31.8 to 32.2, the truck's 49.5 to 50). So for a named part
nothing has to be fitted from its corners: the axis is exact every frame,
spin and steering included, and the radius comes from the corners
themselves, which follows the morph. There is nothing to keep between
frames, so nothing can drift or pop.

### 6. Models take their sunlight from retail's vertex shade (verified)

A model face in the tracer is lit by retail's own light at its corners
(the shade), blended across the face. The tracer only works out how much
of that is the sun, to shadow it (`direct_light`,
`pt_trace.comp:573-604`); the resolve adds it back where the sun is seen
(`pt_resolve.comp:48-51`). So in sunlight the sun's share is retail's,
whatever the pixel's normal is; the normal changes only the shadow, the
bounce light and retail's small dynamic lights. Two consequences:

- **Split faces need shades at their new corners**, worked out the way
  retail works them: ambient plus the light's colour times the facing
  (`shade_normal`, `model_lighting.rs:396-411`), from the model's own
  light (`request.shades.lighting()`). For the wheels, whose corner
  normals are already smooth, the old corners keep retail's values. For
  a part retail lit with one normal per face (the ducts' insides, the
  skimmer's jet, the towers), blending the old shades would keep it
  flat-shaded however round it is.
- **Relief does nothing in sunlight on its own.** A bent normal changes
  only how much a shadow takes away and where the bounce ray goes. For
  the treads to show relief, those pixels must take the sun's facing from
  the bent normal instead of retail's shade: a small change in
  `direct_light`.

### 7. The other forms (verified)

- **Hovercraft**: two ducted fans, `scatfan` (123): 7 corners at radius
  18 to 21 round the model's Z axis, a barrel shape (figure 05). The
  outside has corner normals (smooth shading, heptagon outline); the
  inside, seen through the open end, has one normal a face.
- **VTOL**: nothing round found. `thruster` (128) is a box.
- **Skimmer**: the main jet is a pentagon. The morph's child `mainjet`
  (141) has 5 corners at radius 10 to 18 round a Z axis at y = 9; the
  settled body `jetmode2` (127) carries the same jet inside it, round
  y = 13 (figure 06).

### 8. Round parts elsewhere (a scan; a few seen)

A scan of every model for rings of five or more corners at one radius,
in the planes of the model's own axes (`scripts/rings.py`, output
`scripts/rings.txt`), finds 206 models. Most
are faceted on purpose or not round at all:

- **On purpose**: the bunkers (hexagonal huts, seen on Level 1), boulders
  and rocks, the crystals, `fang*` and `spike*`.
- **Not round**: the gun barrels the names suggest are square
  (`gunbar`, `smallgunbarrel`, `sandbarrel`); the "silo" parts every
  ACAM form carries (`silo1` to `silo8`, 142 to 154) are boxes; Level 1's
  long pipes are edges, already lit as rods, and their joints (`pipe*`)
  are small boxes.
- **Plausibly round, for a review sheet**: octagonal towers
  (`imortalgun`, seen on Level 1 as panelled columns, radius 118;
  `towergun`, `guntower`, `gdtower`, `gunring`, `outpost`, `minetower*`);
  the silos `f03silo1/2` (6 sides, radius 59, 446 tall); tanks
  and stacks (`util03`, `petrotfact2`, `powergen`, `sgenspin`); the
  warpgate's rings (`gatering`, `hoopbig`, `hoopsmall`, 8 sides, radius
  180 to 290; `gatebod`); spheres (`eyeball`, `oneeye`, `powerbal`,
  `brainball`, 10 to 11 sides; `beachbal`); `healthsmall`; enemy tracks'
  end rollers (`track`, arcs of 3 to 4 corners round two axes per
  model); missiles and grenades (6 to 8 sides, small and fast).
- **The menu's planets** (24 sides, `titleplanet*`) show corners on their
  outline (figure 10), but the menu is traced only with `menu.traced`,
  off in the shipped tuning (`modern_tuning.json:2-4`). Untraced, the raster draws them as retail.

A rule by name (`*barrel*`, `silo*`) would round the wrong things. Which
parts were meant round is a judgement per model.

### 9. What a frame costs today (measured)

The tracer's line over the world-18 runs: 6.6 to 8.3 ms a frame, of which
uploads and builds 0.64 to 0.69 ms. The models' BLAS is built from
scratch every frame with the fast-build flag, all models in one, masked
sprites in a second (`trace.rs:1715-1760`, `:1951-1957`). Today the four
wheels are about 280 of its triangles. A rough count of the whole frame's
model triangles, from 119 model draws a frame and the corpus's 35
(median) to 57 (mean) triangles a model, is 4,000 to 7,000 (estimate).

### 10. Also seen

- Motion for the upscalers is the camera's alone (`pt_resolve.comp:78-88`,
  `vk_raster_prepare.comp:4-7`), so the ACAM, which moves with the
  camera, already gets the wrong motion under DLSS, FSR and XeSS.
- The bounce light's history is dropped where a pixel's normal turns by
  more than about 26° (`pt_temporal.comp:67-70`). A spinning heptagon
  turns the normal under a pixel by 51° at every facet joint, so the
  wheels lose their history while driving. Round wheels keep the same
  normal under a pixel as they spin.
- The tyres are tuned 40% metal (`modern_tuning.json:48`, `*wheel*`),
  which suits the hubs more than the rubber.

## Recommendation

Our view, changed only where the findings argue:

1. **Wheels: split retail's faces onto the true circle** (kept). The
   outline is what shows: 9.5 to 13 pixels of facet at 4K (section 4).
   Smooth normals alone change nothing there; the wheels' normals are
   already smooth. Two refinements: nothing needs fitting, since each
   wheel's exact axis comes with its draw (section 5); and the new
   corners take retail's own shading formula (section 6), or flat-shaded
   parts stay flat.
2. **No exact analytic shapes** (kept, firmer). The split's piece count
   follows the wheel's size on screen, so what is left of each facet stays
   under half a pixel at any distance; an exact cylinder could not look
   rounder. It would cost a procedural path in all six ray queries
   (`pt_trace.comp` four, `pt_depth.comp`, `pt_fog_inject.comp`), a
   second kind of BLAS, the hub and side wall made exact too (or gaps open
   at the rim), the tread's per-facet mapping rebuilt by angle, and
   retail's shade rebuilt at every hit. We would not plan for it.
3. **Treads: relief first, then lugs as geometry on the wheels near the
   camera** (refined). The order is kept, but we expect the second step
   to be needed, and we plan it rather than leave it open:
   - Relief is not free here: it needs the sun's facing taken per pixel
     on the tread (section 6). It is still cheap, and it is what wheels
     far from the camera should get.
   - It cannot touch the outline, and the outline is where the wheels
     show today, by the same argument as point 1. A 2-unit lug is about
     6 pixels at the chase distance. The truck's lugs run over its
     shoulders, so its outline would carry them from every side.
   - Displacement is unusually cheap and safe here: the pattern varies
     along the rim only, four lugs a facet on every wheel, so the extra
     cuts go one way, about 3,000 to 6,000 triangles for four wheels.
   - Tessellated treads were asked for by name.

   The go-ahead stays a snapshot test after relief (verification below):
   if, with relief alone, the lugs read as raised from the chase camera
   (the turned front wheel and the truck included), stop there.
4. **Scope: a short named list in the tuning** (kept). The scan backs it:
   most of the 206 ringed models are faceted on purpose or are not round
   (section 8). First the ACAM's wheels; then its other round parts (the
   hovercraft's ducts, the skimmer's jet); world parts only after
   picks from a before-and-after sheet.

## Proposed design

### Step 1: round parts split onto their shape

**Selecting a part.** A tuning list names the models (the same name
patterns as the materials list, `modern_tuning.rs:108-128`) with the axis
in the model's own frame and, where it is off the origin, the centre. The
first entries:

- `huskywheel*`, `bigballwheel*`: axis X.
- Later `scatfan` (axis Z) and `mainjet` (axis Z, centre (0, 9, 0)).

Every face of a listed model is considered. Parts inside a larger model
(the settled skimmer's jet in `jetmode2`) also take a radius window, so
that only edges on the ring curve.

**The part's frame.** In `capture_primitive` (`scene_capture.rs:72-303`),
for a listed model, take the centre from `request.transform.view_position`
plus the tuning's centre along the axes, and the axes from
`request.transform.view_basis`, both through `builder.world`
(`scene_feed.rs:660-667`). A mirrored pass uses the mirrored centre
(x negated), as the mirror copies are made.

**Which edges curve.** In the part's frame each corner has a height along
the axis, a distance from it and an angle round it. An edge is a **rim
edge** when its two ends are at the same height (within 1.5 units, as
`bigballwheel`'s 8/9 alternation needs), at about the same distance
(within 10%), off the axis, and less than 75° apart (a ring of five sides
or more). Every other edge stays straight: along the axis (the tread's
width), from ring to ring (the side wall's slope), or to the centre (the
hub's spokes).

**How a face splits.** Each rim edge is cut into n pieces. A new point
takes the edge's height and distance blended between its ends, and its
angle blended along the short way round, so it lies on the circle (on a
cone where the ends' distances differ). Then:

- a face with two opposite rim edges (the tread, the side wall, the
  shoulders, the ducts) becomes a strip of n quads; lines along a
  cylinder or a cone are straight, so the strip lies on the shape;
- a face with one rim edge and a point on the axis (the hub) becomes a
  fan of n triangles from that point;
- a flat face whose rim edges border no other ring (an end cap) becomes a
  fan from its middle;
- if a strip's two edges get different counts, they are joined
  zipper-fashion.

Duplicate corners (the hub's quads with a repeated corner) are dropped
first.

**No cracks.** An edge's piece count and points depend on that edge alone
(its ends, and the model-wide size on screen below), so two faces that
share it cut it the same way, bit for bit. The mirror pass and both
wheel sets follow the same rule.

**How many pieces.** n is the least count that leaves the gap between
each piece and the circle under half a pixel:
n = ceil(edge angle / (2 acos(1 - 0.5 / r))), with r the part's largest
radius in pixels at its depth, from the camera's focal length and the
render height. The buggy's wheel at the chase distance takes 5 (35
sides), the truck's 6 (48), at 1080p 4 each. A part far away takes 1 and
passes unchanged, at no cost. A cap (`most`, 16) bounds the rare close
view.

**Normals and shades.** A new corner's normal keeps the ends' lean along
the axis and their outward part, drops any sideways part, and turns with
the corner's angle. Retail's smooth wheel normals come out unchanged at
the old corners; a face-wide normal becomes a smooth one. The shade at a
new corner is retail's formula at that normal (section 6). The old
corners keep retail's shade where their normal is unchanged.

**Texture coordinates, colour, fog.** Blended by the same position along
and across the face. Each facet keeps its own copy of the sprite, so the
tread keeps four lugs a facet and the hub's painted rings turn into
circles.

**A guard.** A face of a listed model whose rim corners are not at one
distance from the axis (within 10%) passes unchanged, and one line is
logged per model and run, naming it. Nothing is bent into a shape it does
not have.

**Into the feed.** The face goes in as one face whose triangles are the
split ones, keyed by its original corners, so faces drawn twice over the
same corners still settle as now (`SceneFeedBuilder::face`,
`settle_faces`, `scene_feed.rs:443-477`, `:599-630`).

### Step 2: the treads' relief

- The capture marks triangles in a listed tread sprite (195): a flag on
  `SceneTriangle`, packed as a bit of the material word's third byte
  (`trace.rs:1592-1616` packs the first two).
- In `model_surface` (`pt_trace.comp:317-347`) those pixels read the
  sprite's brightness a half texel either way along u, at the mip they
  already use, and bend the normal along the rim by its slope: dark is
  low, light is high. The sprite's own pattern drives it; there is no
  normal map to author. With distance the mips flatten the slope, so the
  relief fades as it should.
- In `direct_light`'s shade path (`:573-604`) those pixels take the sun
  as retail took it, less its facing along the smooth normal, plus its
  facing along the bent one. That share then goes through the usual sun
  shadow; the bounce ray leaves along the bent normal.
- The geometry image keeps the smooth normal (`pt_trace.comp:1363`), so
  the denoiser and the bounce history do not see the lugs.

### Step 3: lugs as geometry, on wheels near the camera

Only when a wheel is large on screen (its radius over about 40 pixels;
the buggy's is 95 in the chase view):

- the tread's rim edges, and every rim edge on a ring a tread face
  touches (the side wall's rim, the truck's shoulders and inner ring),
  are cut into 6 pieces a lug, 24 a facet;
- each point there moves in by depth × (1 - height), where height is the
  sprite's column brightness at that point's u, scaled 0 to 1. Lug tops
  stay at retail's radius, so the wheel still touches the ground where
  the game puts it; the grooves are cut in;
- which rings carry lugs is worked out once per model from its faces
  (the slots that tread faces use), so the side wall's rim moves with the
  tread and no crack opens. Every face on a shared rim edge maps u the
  same way along it (checked for all four wheel models, 133 to 136); a
  test holds it;
- normals come from the displaced shape, shades from retail's formula;
- the relief of step 2 stays for the wheels that are not displaced.

### Step 4: the ACAM's other round parts

`scatfan` and `mainjet` as their own entries, then the jet inside
`jetmode2` with a radius window. The ducts' insides and most of the jet's
faces have one normal a face, so they need the smooth shades of step 1.

Built (2026-10-24):

- A part takes a `centre` on its axis and, inside a larger model,
  `radius` and `along` windows its rims stand within.
- A face with a normal leaning round the axis (retail's one normal for the
  face) takes the round normal at each corner, as its new points do.
- A flat piece of an end cap, rim edges and a chord (the duct's front cap),
  fans from its own middle.
- The duct's grey underside is retail's one folded quad (slots 20, 21, 29,
  21): it covered half the black face under it, speckled in the trace
  and, once round, a grey triangle inside the duct. The tracer gives it
  slot 28 (`scene_capture.rs`, `unfold_duct`).
- The jet's mouth (radius 10 to 11) stays a pentagon: the raster draws its
  blue glow (sprite 599, additive) over the trace in retail's shape, so a
  round mouth left black slivers round the glow. Both jets' windows start
  at radius 12.

### Step 5: world parts, by review

A sheet of before-and-after crops for each plausible part of section 8,
to pick from. The picked ones join the list. Spheres (the
eyeballs, and the menu's planets if the traced menu is turned on) need
the same split in two directions, every new point pushed out to the
sphere; a small addition to the same code.

Built (2026-10-24):

- **Spheres** (`"axis": "sphere"`, `round_parts::split_sphere`). The face
  is cut as a fan of triangles from its first corner. Each triangle halves
  an edge that turns wider round the centre than a piece may at the
  sphere's size on screen, as `pieces` does for a rim, and recurses, at
  most `most` pieces an edge. Each new point is pushed out to the sphere
  at its edge's mean distance, and every corner faces out from the
  centre. An edge is halved from its ends alone (the same sum, either
  way round), so the faces on it agree. A face whose corners stand at
  different distances from the centre (a planet's ring) stays as it is.
- **The menu's planets** (`titleplanet*`) round under the traced menu, the
  only round part there. About 0.24 ms more a 4K menu frame (1.05 ms
  before, 1.29 after).
- **Where the candidates stand**: a census of every world's placed
  objects (0 to 18; 19 stops loading in tests), following each model's
  instances. Warpgates (`gate`, with `gatebod`) in worlds 1 to 18; the
  power generator in 4, 8, 9, 10, 15 and 17; `f04base`'s silos in 7, 13
  and 17; `gatering` in 0, 7, 13, 16 and 18; the lock hoops (`hoopbig`)
  in 11 and 14 to 18; `braineye`'s eyeball in 0; `powerbal` in 14; the
  radar dish (`comdish`) in 16. No placement holds `util03`,
  `petrotfact2`, `sgenspin`, `hoopsmall`, `oneeye`, `brainball`,
  `beachbal`, `healthsmall` or `track`.
- **The sheet** (`.tmp/plan-round/world_parts_review.jpg`, probes in
  `.tmp/plan-round/sheet/`): each part before beside after, cropped where
  the two frames differ. The warpgate's arch, the power generator, the
  silos and the eyeball show. The lock hoops, `gatering` and `powerbal`
  never drew in the probes' views, and the dish stands out of sight on
  its platform.

### What else it touches

- **Shadows.** The wheels' traced shadows turn round (heptagons today,
  figure 01). The sun's shadow ray leaves from the face's own normal
  (`pt_trace.comp:1321`); on small split faces that is close to the
  smooth one, so the tyre's shadow edge stays clean without the lift the
  traced menu's planets need (`scene_feed.rs:299-302`).
- **The denoiser and the bounce history** compare normals and distances
  (`pt_atrous.comp`, `pt_temporal.comp:67-70`). Round wheels pass more
  often than today's facets; lugs as geometry fail on their flanks while
  they spin (risks).
- **Reflections.** The tyres are tuned partly metal, so they mirror a
  little; round faces stop those reflections jumping from facet to facet.
- **Thermal vision** takes heat by model name; split triangles keep their
  model, so their heat.
- **The raster's blended parts** (dust, the rear lamps' glows) stay on the
  raster and test against the depth the tracer writes
  (`pt_trace.comp:1292`), which now follows the round outline.
- **Motion**: the split keeps nothing between frames; the new corners move
  with the old ones.
- **OpenGL and software** never see the split: it lives in the scene feed.

## Alternatives considered

- **Smooth normals only.** The wheels have them already, and the outline
  stays a heptagon. Not enough on its own.
- **Exact analytic shapes** (recommendation, point 2). Rejected for the
  cost against no visible gain over the screen-size split.
- **Finding round parts automatically** (any ring of five or more). The
  scan's list includes the bunkers, boulders, rocks and a crystal
  (`xshard03`), all faceted on purpose (section 8).
- **Lugs everywhere at every distance.** Wasted on far wheels, where
  relief is enough.
- **Splitting on the GPU** (a compute pass before the build), or
  NVIDIA's displacement micro-maps. More machinery for a few thousand
  triangles; the micro-maps are one vendor's extension.
- **Edges as real tubes** (the axles, pipes and antennae, drawn as flat
  strips facing the eye). Not asked; the strips are already lit as rods.
  A later question if wanted.

## Tuning (`modern_tuning.json`)

A new block, starting values to tune by eye. The tuning reloads while the
game runs (`modern_tuning.rs:488`).

```json
"round": {
  "tolerance": 0.5,
  "most": 16,
  "parts": [
    {"names": ["huskywheel*", "bigballwheel*"], "axis": "x"},
    {"names": ["scatfan"], "axis": "z"},
    {"names": ["mainjet"], "axis": "z", "centre": [0, 9, 0]}
  ],
  "treads": {
    "sprites": [195],
    "relief": 2.0,
    "depth": 2.0,
    "lugs_from": 40.0
  }
}
```

- `tolerance`: the facet gap allowed, in pixels; `most`: the most pieces
  an edge takes.
- `parts`: name patterns, the axis in the model's frame, an optional
  centre and an optional `radius` window `[least, most]`.
- `treads`: the lug sprites, the relief's height and the lugs' depth in
  units (2 for a start: a quarter of the buggy's lug spacing), and the
  wheel radius in pixels above which lugs become geometry (0 turns
  geometry off).

## What changes where

- `src/round_parts.rs` (new): the part frame, edge test, split, counts,
  normals, retail's shade, lugs and the guard, with its tests.
- `src/scene_capture.rs`: `capture_primitive` looks up the model's entry
  (cached by model and tuning, as the heat is in `trace.rs:1562-1591`),
  splits each pass's corners, marks tread triangles.
- `src/scene_feed.rs`: a face added as a triangle list, keyed and settled
  by its original corners.
- `src/model_lighting.rs`: `shade_normal`'s formula for a world normal
  (floats), shared with the capture.
- `src/modern_tuning.rs`, `src/modern_tuning.json`: the `round` block.
- Step 2: `src/vk_device/trace.rs` (the tread bit), `shaders/pt_scene.glsl`
  (the triangle word), `shaders/pt_trace.comp` (`model_surface`,
  `direct_light`, the bounce); SPIR-V rebuilt with
  `scripts/compile-shaders.ps1`.
- Notes: `docs/re/PIXEL_RUNTIME.md` (Modern mode: round parts and
  treads) and `docs/INFESTATION-RT-IMPLEMENTATION.md` (the scene feed).
- Tests: two faces sharing a rim edge give identical points; u and the
  shade are continuous across a split; the counts follow the tolerance;
  the real `huskywheel` and `bigballwheel` corners land on their circles
  (and on the right radius mid-morph); the guard leaves an off-shape face
  alone; the tuning block parses.

## Cost

Estimates, to be measured with the tracer's line:

| | Four wheels' triangles |
| --- | --- |
| Today | about 280 |
| Step 1 (buggy 5 pieces, truck 6) | about 1,100 to 1,550 |
| Step 3 (lugs on near wheels) | about 3,000 to 6,000 |

- Against a frame of perhaps 4,000 to 7,000 model triangles (section 9),
  step 3 can double the models' BLAS. A fast build of that size is well
  under 0.1 ms; uploads and builds are 0.64 to 0.69 ms today.
- Tracing: the same rays; a little more tree to walk near the wheels.
- Relief: two texture reads per tread pixel.
- The CPU: tens of microseconds a frame for the split.
- World parts (step 5) add per part picked; far ones cost nothing (one
  piece).

## Risks

- **Cracks** if two faces cut a shared edge differently. The counts and
  points depend on the edge alone; the tests hold it.
- **Shading seams** if new corners were lit unlike the old: retail's own
  formula with the model's own light avoids that. One exception: where
  retail reads a stale shade from an earlier model (`scene_capture.rs:260-261`),
  a round part's new corners take the formula's value.
- **Parts inside larger models** (the skimmer's jet): every face touching
  the ring must split alike, or a sliver opens. That is why they come
  after the wheels, with a radius window.
- **Lugs while driving**: the normal under a pixel changes as lugs pass,
  so the bounce history drops there (it already does on today's
  heptagon); the dark rubber hides the noise. Fast spins can strobe, as
  the painted stripes already do.
- **Upscalers**: the ACAM's motion is already the camera's, so spinning
  lugs may smear under DLSS, FSR or XeSS, as the stripes do.
- **The ground**: round buggy wheels touch it exactly (section 4); the
  lug tops stay at retail's radius, so nothing sinks in.
- **Shared code**: the headlight and water plans also edit
  `pt_trace.comp`'s `direct_light`. Merge in order.

## Verification

All under Vulkan with path tracing, hidden, 16 ms steps, before (the
runs above) and after:

- `C_w18_buggy_turn` and `D_w18_truck_turn`, frames 70 and 85: the wheels'
  outlines and their shadows round. To measure: fit a circle to the front
  wheel's outline pixels in the crop. Today the facets should stray about
  9.5 pixels from it (computed, section 4); after, under one.
- `A_w18_buggy`: from behind, no kinks on the tyres' outlines.
- The hub's painted rings concentric; the four lugs a facet still lined
  up with the side wall's notches.
- A drive (Up held, a snapshot every 5 frames): no popping or flicker on
  the outline as the wheels turn.
- A form change mid-run (keys 1 and 2, then 3): the wheels shrink and grow
  without cracks.
- Step 2's go/no-go for step 3: the same frames with relief, compared
  with the outline mock-ups (figures 08, 09). If the outline and the
  turned front wheel still read flat, do step 3.
- The same runs under OpenGL and software: identical to before, byte for
  byte.
- The tracer's GPU line before and after; `cargo test -p inf-game` with
  the new tests.

## Order of work

1. Step 1 for the buggy's and the truck's wheels (both sets), with its
   tests and snapshots.
2. Step 2, the relief, and its snapshots: the decision on step 3.
3. Step 3, lugs on near wheels.
4. Step 4, the hovercraft's ducts and the skimmer's jet.
5. Step 5, the review sheet of world parts, then the picks.

Each step ends with before-and-after snapshots and a commit.

## Open questions

1. **The hubs**: round them with the tyres (the painted bolt rings become
   circles), or keep the 7-sided hub as a styled cap?
2. **Lugs**: relief only, or real lugs on the wheels near the camera (our
   view: real lugs)? How deep: about 2 units, a quarter of the lug
   spacing?
3. **The hovercraft's ducts and the skimmer's jet**: round as well?
4. **World parts**: which, from the review sheet? The octagonal towers
   read as panelled columns today; keep that look, or round them?
5. **Tyres' material**: the tuning makes them 40% metal. Split the rubber
   (matte) from the hubs (metal)?
6. **The menu's planets**: round them too, if and when the traced menu is
   turned on?

Decided 2026-10-23: the hubs round, real
lugs about 2 units deep, the ducts and the jet round, matte rubber and
metal hubs, the planets round, and the towers keep their panelled look.
Still open: which world parts of the sheet round.

## Evidence

Figures in `.tmp\plan-round\figures\`:

| Figure | Shows |
| --- | --- |
| `01-buggy-vulkan-vs-opengl.png` | The turning buggy: heptagon wheels and their traced shadows (Vulkan), and retail's look (OpenGL). |
| `02-buggy-front-wheel-3x.png` | The front wheel at 3x: seven tread faces, the 7-sided hub, four lugs a face. |
| `03-truck-vulkan.png` | The truck's octagonal wheels and shadows. |
| `04-buggy-from-behind.png` | The usual chase view: kinked tyre outlines, painted lugs. |
| `05-hovercraft-ducts.png` | The hovercraft's two 7-sided ducted fans. |
| `06-skimmer-jet.png` | The skimmer's pentagonal jet. |
| `07-wheel-sprites.png` | Sprites 195 (tread), 196 (side wall), 197 and 198 (hubs), 8x. |
| `08-outline-buggy.png` | A drawing from the model data at the on-screen size (radius 96 px): today, split 5 times, split with 28 lugs. Not a render. |
| `09-outline-truck.png` | The same for the truck (radius 170 px, 6 times, 32 lugs). |
| `10-menu-planets-raster.png` | The menu's planets as the raster draws them: 24-sided outlines. |
| `11-level1-tower.png` | Level 1's octagonal gun tower (`imortalgun`): panelled column, flat-shaded faces. |

Run folders in `.tmp\plan-round\runs\`. Helpers
(scratch, not tracked) in the probe folder: `probe.sh`, `view.py`,
`pair.py`, `mockup.py`, and in `scripts/`: `dump2.py` (one model's
corners, normals and commands), `rings.py` (the ring scan), `lit.py`
(lighting kinds and children), `names.py`, `spr.py`, `sprsheet.py`.
