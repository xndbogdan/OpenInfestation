# Fog and pop-in under Vulkan: plan

Status: built (2026-10-23), with these answers: closer to retail's denser fog, Altair
Prime densest (a smog), the lowest degrees of sky fogging over, a faint
dither accepted without anti-aliasing, the dissolve alone above the fog.
Differences from the proposal below: the fog grows as the square of the
way to F (a straight growth buried the middle distance), it is clear
above its top band, and a model's own fog (the actor flash) stays.
Reopened the same day (stronger, and the crystals
showed through it): the top follows the land's high ground, 99% at F with
a curve of 1.6, what lies past F fades out by 1.6 F, and the raster's late
draws sample the grid itself rather than a CPU copy of its curve. The
world-by-world review remains.

## What was reported

The fog should be nearer: pop-in is clearly visible and should be masked
by fog. Find where items pop in and use volumetric fog.

## In short

- **One distance decides what is drawn.** Retail draws models (buildings,
  trees, rocks, vehicles, creatures), the terrain's furniture and particles
  only out to the fog's end, called F here. At normal frame rates F is
  12,480 units, the same in every world. A model appears when its centre
  comes within F plus its own size, so it appears with its nearest parts at
  F.
- **Retail hides that moment with fog.** A model fades in a straight line
  into the fog colour and is fully fog-coloured at F. The tracer keeps that
  fade on models.
- **The tracer's fog around them is thinner.** Its haze over the ground is
  only 78% at F; it completes at 1.5 F. The mist adds a lot over low ground
  but little higher up, and only half of it lies over the sky. So a model
  appears as a flat fog-coloured shape in front of ground that still shows
  its own colour, or in front of clear sky.
- **In 11 worlds the colours differ too.** Past F the haze turns to the far
  colour (H7 `0x73`), while models fade to the fog colour (`0x74`). Where
  the two differ, new shapes are darker or brighter than everything around
  them.
- **Reproduced.** On Oolbryte (world 20), at the normal draw distance,
  moving the camera 50 units closer makes two trilithons appear on a far
  ridge. They are 37% darker than the sky behind them and 9% darker than
  the ridge. Retail's look (OpenGL) has the same pop in the same place.
- **Tuning alone does not fix it.** Completing the haze at F leaves the pop
  as visible. A mist thick enough to hide it buries the base in the middle
  distance and the whole sky.
- **Two cuts are the tracer's own.** Its rays stop at 4 F, which cuts far
  ridge tops at the skyline whenever F changes. And the sky models some
  worlds hang on the backdrop are traced as world geometry: on Command
  Centre (world 18, Level 1) the cumulus ring hides the painted mountains
  as soon as 4 F reaches it, which in play is always.
- **Proposed.** Let the volumetric fog do the masking: clear in front,
  complete at F near the ground, thinner above a per-world top so far
  hilltops and the upper sky stay readable. Give models, ground, water,
  the low sky and the raster's sprites that one fog. Fade models out over
  the last tenth before F as a safety net. Fix the tracer's two cuts.

## How it was checked

Everything is in `.tmp\plan-fog\`; paths below are
relative to it. The illustrations are in `evidence/`, the helper scripts in
`tools/`.

- **Runs.** The folder's copy of `inf-game.exe`, Vulkan with path tracing,
  hidden, a 1920x1080 window (snapshots come out at 3840x2160), fixed 16 ms
  steps. Settings `rt.json.keep`; starts `start_*.json`.
- **Fog distances.** The run's last world-draw receipt prints the fade
  pair `[start, F]`. Every world at frames 60 to 80 read `[2745 to 2870,
  12480]`.
- **Pops at the normal draw distance.** The vehicle placed by the start
  file's position and heading, standing still; one run per position, 50
  units apart, one snapshot each at frame 60 (`start_q900.json`,
  `start_q950.json`, ...).
- **Pops as F grows.** `"world_far": 5120` in the start file makes F start
  at 5,120 and climb 64 units a frame to 12,480 while the camera stands
  still. Every frame is snapshotted; `tools/popmap.py` lists the blocks of
  the image that change once and then stay (`tools/climb.sh`). This shows
  what appears at each F without camera motion.
- **Driving** (worlds 17, 18, 20, 23, snapshots every 1 to 8 frames) was
  tried first. The bouncing vehicle and the passing scenery drown one-off
  changes in a frame difference, so the still-camera methods carry the
  evidence.
- **Retail's look:** the same starts with `--renderer opengl`.
- **Tuning:** a `modern-tuning.json` beside the settings (`tun_t1.json` to
  `tun_t3.json`).

## Where things appear and disappear

"View depth" is the distance along the camera's forward axis. Retail's fog
and its culling both use it.

| What | Where in the code | Distance at normal frame rates | Retail's mask | The tracer today | Status |
|---|---|---|---|---|---|
| Models | `model_context.rs:220-237`: drawn while view depth minus the model's extent is below F | view depth F plus the extent: 12,480 plus up to 2,048 (half of all models within 168, 90% within 726) | straight fade to `0x74`, complete at F (`model_projection.rs:59-70`) | keeps that fade (`scene_capture.rs:213-264`; `fog = surface.fog` in `pt_trace.comp` `main()`; `pt_resolve.comp:53-54`) | verified: code, world 20 probes |
| Objects, before their models | `object_draw.rs:596-625`: horizontal distance below F plus radius (plus 6 times its height above the ground), inside the view cone | as models | as models | as models | code |
| Furniture | `terrain_draw/plan.rs:118-131`: a footprint 512 past F ahead, then the model test | within F plus 512 ahead | as models | as models | verified: world 20's trilithons are furniture |
| Particles | `process_particles/draw.rs`, `draw_animation`'s `depth >= projection.fade[1]` tests: culled at view depth F | F | far fog to `0x74`; additive ones fade to black | drawn by the raster after the tracer, with retail's fog only; the mist never covers them | code |
| Far terrain silhouettes | `frame.far * 4.0` in `pt_trace.comp` `main()` and `pt_depth.comp:32,36`: primary rays stop at 4 F | 49,920, a sphere round the eye | retail's far terrain ends at 1.5 F plus 1,000, fading into `0x73` | the cut moves when F changes | verified: world 20, F climbing |
| Sky models other than the ring | `scene_capture.rs:25,80`: only H8 `0x116` counts as sky | hit once 4 F reaches them (16,000 to 30,000 out) | drawn unlit by the raster as sky | traced as emissive walls, then misted | verified: world 18 |
| Shadows and lights of models | appear with their models | F | blob shadows only | a traced shadow on ground still 5 to 20% visible | suspected |
| Water | `world_scene.rs`, `water_reach` = F (clamped to 1,024..12,800): the wave mesh ends, the flat plane goes on | F | none | a seam that moves with the camera, not a pop | code |
| Terrain detail | the raster's near and far ground meet at 0.55 F plus 512 | none | none | the tracer uses the whole map's ground: no switch | code |

How F moves (`world_fade.rs:47-76`): the game raises F toward 12,544 while
frames average under 46.75 ms (about 21 frames a second) and lowers it to
5,120 at the least when they are slower. The predraw follows it 64 units a
frame and stops one step short, so it rests at 12,480. A saved game's
first world starts at 10,176, dips to 9,856 while the frame-time history
still holds its starting samples, and reaches 12,480 by about frame 55; a
world reached in play keeps the last world's F. The fog's start is (F
minus the world header's H) / 4, H being 1,000 to 1,500.

## The fog today, in numbers

At F = 12,480 and a start of 2,800 (`vk_device/trace.rs`, the fog block
near line 2240 that sets `haze_reach` and `mist_density`; `haze()` in
`pt_trace.comp`, `pt_composite.frag:111-133`, tuning `fog` in
`modern_tuning.json`: density 1.5, height 600, floor 0.1, sky 0.5, reach
1.5, haze 2.5). Percentages are how much is hidden.

| View depth | Retail: models | Retail: ground | Tracer: haze on ground and water | Tracer: mist at the low ground / 1,500 above it | Ground, haze and mist together (low / high) |
|---|---|---|---|---|---|
| 3,120 (0.25 F) | 3% | 0% | 5% | 31% / 6% | 34% / 11% |
| 6,240 (0.5 F) | 36% | 80% | 42% | 53% / 12% | 73% / 49% |
| 9,360 (0.75 F) | 68% | 100%; past 0.55 F the far terrain is fog-coloured, its relief shaded | 64% | 68% / 18% | 88% / 71% |
| 12,480 (F): models appear | 100% | 100% | 78% | 78% / 23% | 95% / 83% |
| 18,720 (1.5 F) | not drawn | fades into `0x73`, ends at 19,720 | 100%, in `0x73` | | |
| 49,920 (4 F) | | | the tracer's rays stop | | |

- The sky gets no haze and half the mist. Looking level from the low
  ground, about 55% of the painting still shows; looking level from 1,500
  up, about 85%.
- Models get retail's fade and the mist on top. At F they are fog colour
  under mist.
- So what is behind a new model still shows 5% (low ground), about 17%
  (ground 1,500 up) or 55% and more (sky). Low on the screen the pop is
  nearly hidden; on ridges and against the sky it is not.

## World by world

F is 12,480 in every world. The start is measured; the colours are retail's
(from the [sky survey](sky-per-world.md)); "sky models" are those the
backdrop hangs, traced today as world geometry.

| World | Name | Fog start | Fog `0x74` | Far `0x73` | Colours | Sky models traced |
|---|---|---|---|---|---|---|
| 0 | (Brain's map) | 2,800 | (66, 49, 49) | (0, 0, 0) | differ | `cloud` x2 |
| 1 | (unnamed) | 2,800 | (99, 99, 99) | same | equal | |
| 2 | Terra Nova | 2,870 | (33, 115, 82) | same | equal | |
| 3 | Stendahl Alpha | 2,870 | (0, 49, 49) | (8, 25, 25) | differ | |
| 4 | Exxodus | 2,800 | (58, 0, 0) | (66, 0, 0) | nearly | |
| 5 | Altair Prime | 2,800 | (82, 74, 16) | (173, 148, 25) | differ | |
| 6 | Helniazine | 2,800 | (123, 74, 74) | same | equal | |
| 7 | Sanrel-Dek | 2,800 | (49, 0, 0) | same | equal | |
| 8 | Luna Azine | 2,800 | (115, 66, 16) | same | equal | `stars01`, `spacegradient` |
| 9 | Tabuchi | 2,795 | (90, 156, 82) | same | equal | `mist` |
| 10 | Pillier Stren | 2,795 | (115, 132, 165) | same | equal | |
| 11 | Artoria | 2,800 | (66, 49, 16) | (8, 0, 0) | differ | |
| 12 | Shalapreth | 2,795 | (173, 156, 99) | same | equal | |
| 13 | Grach'to | 2,800 | (115, 74, 66) | same | equal | |
| 14 | Foldrak Noir | 2,800 | (66, 99, 99) | same | equal | `moonstar`, `moongradient` |
| 15 | Kankutan | 2,800 | (132, 99, 82) | (197, 140, 148) | differ | |
| 16 | Defence Satellite | 2,800 | (49, 49, 49) | (49, 107, 49) | differ | `moon2star` |
| 17 | Shak'tahh | 2,745 | (189, 66, 16) | same | equal | |
| 18 | Command Centre | 2,800 | (82, 82, 33) | (148, 123, 66) | differ | `commandmountains` |
| 19 | Alien Brain | 2,800 | (66, 49, 49) | (0, 0, 0) | differ | `cloud` x2 |
| 20 | Oolbryte | 2,800 | (140, 140, 107) | (255, 255, 189) | differ | |
| 21 | Vernimab | 2,800 | as 4 | as 4 | nearly | |
| 22 | Homeworld | 2,800 | as 5 | as 5 | differ | |
| 23 | Caspadril | 2,870 | (173, 197, 197) | same | equal | |
| 24 | Caradion | 2,800 | (82, 33, 0) | (107, 33, 8) | differ | |

The whole H8 set with the force-near flag `0x20` is the sky's: `backdrop`
(the ring, 278), `stars01`, `spacegradient`, `mist`, `moonstar`,
`moongradient`, `purplegradient`, `moon2star`, `commandmountains`,
`cloud`, and the frontend's `titles`.

## The pop, measured

Oolbryte (world 20), the camera still at heading 93 degrees, F = 12,480,
eye `[30294, -233, 23470]` and then 50 units closer `[30344, -233, 23468]`
(`q900_f060.png`, `q950_f060.png`; `evidence/01_pop_world20.png`).

- Two trilithons appear on the far ridge, fully fog-coloured. Over a
  1,152 by 216 pixel crop of the 4K frame round them, 3,003 pixels change.
- Against the sky they are 37 to 38% darker than what was there, against
  the hazed ridge 9 to 10% darker. The fog colour here (140, 140, 107) is
  darker than the far colour the haze is turning to (255, 255, 189).
- OpenGL at the same two places shows the same pop: 36% darker than the
  sky (`evidence/03_pop_opengl.png`). Retail never hid it against the sky.
- With F climbing on the same world (`evidence/04_climb_world20.png`), a
  trilithon appears at F = 9,472 and two more at 10,944: the same shapes,
  each one frame from nothing to solid. In the same run, ridge tops
  appear at the skyline as the rays' reach (4 F) passes them.
- Command Centre with F climbing (`evidence/05_world18_sky.png`): at F =
  5,120 the painting's mountains and cumulus show as in OpenGL; from 5,504
  (4 F = 22,016) the traced cumulus ring cuts in as arches; by 5,888 the
  mountains are gone. At the normal F they are gone from the start, where
  OpenGL shows them.

Tuning alone, the same two positions (`evidence/02_pop_tuning_ab.png`,
full frames in `evidence/06_tuning_full_frames.png`):

| Tuning | Pixels that change | The look |
|---|---|---|
| today | 3,003 | as now |
| `reach` 1.05: the haze complete at F | 3,271, stronger | the ridge behind turns flat and pale (`0x73`), the trilithons darker against it |
| `reach` 1.05, `density` 2, `floor` 0.3, `height` 1,200, `sky` 0.6 | 3,196 | a pleasant thicker haze; the pop still 34% against the sky |
| `reach` 1.05, `density` 4, `floor` 0.5, `sky` 1 | 0 | the base in the middle distance and the whole sky buried |

`reach` cannot go below 1 (`fog.reach.max(1.0)` in `trace.rs`), and at
exactly 1 the haze's colour turn is a zero-width `smoothstep`, which GLSL
leaves undefined; hence 1.05.

## Proposed approach

The aim: nothing appears or vanishes where the player can see it; the fog
is nearer and volumetric, as asked; far silhouettes stay readable. Nothing
retail does not draw gets drawn: the changes only hide.

### 1. The volumetric fog does the masking

Today's mist (`pt_fog_inject.comp`: a grid of 160 cells across the view
and 64 slices deep, lit by the sky and the sun through the tracer) becomes
the world's distance fog.

- **Clear in front.** It starts where retail's fog starts (the fade's
  start, about 2,800).
- **Complete where things appear.** Near the ground it hides 98% (tunable)
  at F, so whatever appears there is already inside opaque fog. The tuning
  gives the curve, as retail's straight line from start to F or something
  softer; the grid turns it into a density per slice. "Optical depth" is
  how much fog dims light: 1 leaves 37%, 4 leaves 2%. The light keeps
  working: the sun scatters in it, terrain and buildings cast shafts,
  retail's lights glow.
- **A top.** Below a height set per world it is the full fog; above it,
  it thins over a band to today's light floor. Default top: just above
  the height four fifths of the world's ground lies below (the feed
  already finds the low ground the same way). Hilltops, mesas and
  mountains that rise above it stay readable as silhouettes. The first
  Vulkan sweep's flat fog lost such mountains (world 17's mesa among
  them), and today's haze was built to bring them back. The low mist over
  valleys and water stays as an extra layer.
- **One colour at F.** The fog turns from the fog colour (`0x74`) near the
  eye to the far colour (`0x73`) at F. It then meets the horizon of the
  sky plan's sky (its horizon is `0x73`) without a line, and models and
  ground at F share one colour in the 11 worlds where the two differ.
- **Slices end just past F** (today 1.5 F), so the 64 slices cover what
  matters more finely.

### 2. One fog for everything in view

- **Ground, water and models** take the volume. For the normal view the
  screen-space haze and retail's per-vertex fog on traced models are
  dropped. This changes a recorded decision ("Models keep retail's fog"):
  two fogs with different curves and colours are what lets new shapes
  stand out. Thermal vision has no mist, so it keeps the haze, made
  complete at F.
- **The sky** takes the volume as its rays do. The lowest few degrees above
  the horizon fog over (the top decides how many), which hides what
  appears against the low sky; above them the sky stays as painted, or as
  the sky plan makes it.
- **The raster's late draws** (particles, sprites, beams, glows; drawn
  after the tracer so they sort against it) take the same fog by their
  depth. Today they keep retail's fade only, so far smoke and explosions
  would stand out against a fogged world.

### 3. A dissolve for what rises above the fog

A tower on a high ridge or a ship in the air can still appear against
clear sky. Over the last tenth before F (tunable), a model's pixels are
hidden at random, in proportion to how close they are to F, and the ray
goes on to what is behind. Blended over frames (TAA, DLSS, FSR, XeSS) this
is a smooth fade in from nothing. Because a model appears with its nearest
parts at F, it is fully hidden in its first frame, whatever its size or
background. Shadow rays take the same test, so a shadow fades with its
model. Two ways to build it: put the triangles of models inside the band
in the non-opaque set, as masked sprites are, so the existing hit loop
(`PROCEED_THROUGH_CUTOUTS` in `pt_scene.glsl`) tests them for primary and
shadow rays alike; or, for primary rays, trace on from the hit when the
test hides it. Particles fade their alpha, or their brightness for
additive ones, over the same band.

### 4. The tracer's own cuts

- **A fixed reach.** Rays go to a fixed distance (4 x 12,544 = 50,176, four
  times the largest F) instead of 4 F, so the skyline stays put while F
  moves (each world's first second, slow frames). Above the fog's top,
  distant terrain fades into the far colour well before that reach.
- **Sky models are sky.** Every model whose H8 header has flag `0x20`
  becomes sky like the ring: the raster draws it, primary rays never hit
  it, reflections still see it, and the sky's top edge takes its edges
  too. Command Centre's mountains come back. The other worlds that hang
  sky models (0, 8, 9, 14, 16, 19) draw them as the raster does; Tabuchi's
  (9) and the Brain's (19) skies differ from OpenGL today. The sky plan
  found the same models "traced today as far emissive walls"; this fix
  serves both plans.

### Starting values for the review

Start at retail's fog start; 98% at F near the ground; the top 500 above
the four-fifths height, thinning over 800; the dissolve from 0.9 F; the
low mist as today. All per world in the tuning file's `worlds`, reviewed
next to the sky.

## Alternatives considered

- **Tuning only.** Measured above: no setting of today's knobs keeps the
  middle distance and hides the pop. Worth trying as a look in the
  meantime (`tun_t3.json`), not as the fix.
- **A flat fog wall at F**, retail-like, one colour, no top. It hides pops
  against low ground, not on ridges or against the sky. It loses the far
  mountains again (world 17's mesa) and is not volumetric.
- **The dissolve alone.** Cheap, and it hides every pop on any background.
  It leaves the view as long and clear as now, though nearer fog was
  asked for, and it leaves the skyline cut.
- **Uniform height fog without a distance ramp** (physically plain): to
  hide 98% at F it would already hide almost two thirds at a quarter of F.
- **Drawing past F:** not allowed (retail decides what is drawn), and
  costly.

## What changes where

- `modern_tuning.rs`, `modern_tuning.json`: fog fields `start`, `opacity`,
  `curve`, `top`, `band`, `dissolve`; `reach` and `haze` kept only for
  thermal vision. Per-world overrides as today.
- `world_scene.rs` (`feed_world`, the fog near line 1313) and the terrain
  feed: the height for the top (the feed already finds the low ground).
- `vk_device/trace.rs` (the fog block near line 2240): the frame's fog values, the volume's
  far end at F, the fixed reach.
- `shaders/pt_frame.glsl`: the new fields.
- `shaders/pt_fog_inject.comp`: density from the curve times the height
  profile; colour by depth.
- `shaders/pt_trace.comp`: no haze or vertex fog on traced surfaces in the
  normal view; the dissolve on model hits; the reach (`frame.far * 4.0`
  in `main()`). The dissolve in `pt_scene.glsl`'s hit loop for shadow
  rays; the reach in `pt_depth.comp` (32, 36).
- `shaders/pt_composite.frag` (102-131): the sky takes the volume by its
  height profile in place of the fixed half share.
- The raster's late draws in modern mode: the gathered fog sampled in
  `vk_textured.frag` and `vk_flat.frag` at the fragment's depth (binding
  the grid and the frame block), or a per-vertex fog from the same curve
  on the CPU.
- `scene_capture.rs` (25, 80): flag-`0x20` models are `SceneSurface::Sky`.
- Tests: the tuning parse (new fields, the per-world merge), the curve's
  CPU twin if the sprites use one, a capture test for a flag-`0x20` model.
- Docs: `PIXEL_RUNTIME.md` (Fog, Mist, "Models keep retail's light and
  fog") and the RT plan's Fog section.

## Risks

- **A smaller-feeling world.** The fog completes at about 12,500 instead
  of 18,700. Retail's own ground was fully fogged by about 6,900, so this
  is still clearer than retail on the ground. It is decided per world.
- **The top.** Too low and ridges show pops (the dissolve still catches
  them); too high and a tall band of the sky fogs over. Per world.
- **The dissolve with anti-aliasing and upscalers off** (the probe
  settings are both off): without frames to blend, the random hiding shows
  as fine dither for about a second at the far end. Below the fog's top
  it sits behind 95% fog and hardly shows; against clear sky it does.
  Options: an ordered dither, or the dissolve only with TAA or an
  upscaler on.
- **Flying.** The VTOL climbs to about 2,500 above the ground; above the
  fog's top, things far below can still appear in thinner fog. The
  dissolve covers them; raising the top with the camera would fog more
  sky.
- **Slow frames** lower F toward 5,120; the fog comes nearer with it, as
  retail's did.
- **Mist shadows over the sky.** Behind a near building the mist's sun
  shadow already darkens a hard-edged column of sky (world 1's start). A
  fog over the low sky makes these stronger; the top limits them.
- **Other plans on the same grid.** The headlight plan wants its beam in
  the air; the volume's lighting path must stay as it is.

## Cost

No new pass. The mist grid already runs every frame (about 0.1 ms at 4K);
a different density costs nothing measurable, and dropping the haze saves
a little. The dissolve adds one more ray query only for model pixels in the
last tenth before F. The raster's sprites read one more texture. Measure
with the tracer's GPU-time line before and after.

## Verification

1. The pop pair: world 20, `start_q900.json` and `start_q950.json`, 60
   frames each (`evidence/01` is the before). Inside the two boxes where
   the trilithons appear (4K pixels x 1928-1980 and 2165-2209, y 680-723)
   the change must fall under 2%; today it is 37% where sky was behind and
   9% where ridge was. The rest of the crop changes by 400 to 1,500 pixels
   between any two 50-unit steps (the HUD's marker, the vehicle's pitch),
   so count only the boxes.
2. F climbing (`tools/climb.sh` NAME WORLD, then `tools/popmap.py`) on
   worlds 1, 4, 12, 18 and 20, the ones named: no one-off
   changes in the far field except animations.
3. Drives on the same worlds: 600 frames at 16 ms, snapshots every 4
   frames, before and after side by side.
4. Command Centre against OpenGL: the mountains back. Worlds 0, 8, 9, 14,
   16 and 19: their skies against OpenGL.
5. World 17's mesa stays visible above the fog.
6. The VTOL at its ceiling (form 4 with `"upgrades": "all"`, key 4, Space
   held): look for pops below.
7. Thermal vision and the briefing's camera pane: unchanged.
8. Slow frames: `--fixed-step-micros 125000` (as the lab steps) drops F
   to 5,120 within about 80 frames; the fog follows and nothing pops.
9. GPU time per pass, before and after.

## Order of work

1. Sky models as sky: small, on its own, and it brings Level 1's
   mountains back. Coordinate with the sky plan.
2. The fog volume: curve, top, one colour at F; ground, water, models and
   the low sky take it.
3. The raster's late draws take it.
4. The dissolve.
5. The fixed reach.
6. The world-by-world review, beside the sky's.

## By distance from the eye (2026-10-25)

The distance fog first counted by view depth, as retail's raster fog does.
That held while everything it veiled lay within retail's planar cull, but
the backdrop's land band, which the modern sky keeps as far geometry on a
ring 18,500 to 27,500 out, sat past the fog's fade at the view's middle
and inside it at the sides (depth R cos 50 degrees at 16:9), so the haze
seemed to sit in the middle of the sky and turn with the view, and tall
band tiles stood clear at the screen's edges (seen on Artoria,
Exxodus and Altair Prime, in 16:9 and 4:3). The mist's grid now holds the
fog's density by distance from the eye, per unit along the ray, and the
fade past the fog's end, in the composite and in the raster's late draws
over the traced world, and the models' dissolve before it, read the same
distance. Retail's raster keeps its
own depth fog. A world's tuning may also stretch its draw distance under
the tracer (`fog.far`) and give its fog a colour of its own
(`fog.color`); the Command Centre takes both (half again its reach, and
an open blue).

The fog's top, above which it thins so the sky stays readable from the
ground, now rises with an eye that climbs past it (2026-10-26: from
the VTOL, high enough, the view still reached past the fog). What
showed was the sky below the horizon: from high up, rays a little below
it miss the ground within the rays' reach and meet no fog within the
grid's, so they showed the sky under the far ground's fade. The top first
rose to the eye itself, which hid that sky but stood the fog up as a wall
to the eye (2026-10-29: "the top part of the fog rises a bit too much. I
just wanted the bottom part to not uncover more of the skybox"). Now that
sky takes the fog's own colour in the composite (`fog_colour`, the colour
the ground past 1.6 F beside it fades into, so no seam), behind any nearer
clouds, and the top follows the eye half its band below it (`fog.lag`
0.5): the haze at the eye's height half as thick, the fog half as high
over the horizon, its edge soft at any height. A top following only part
of the eye's climb would have left the eye above the band from some
height, the horizon then a hard line.

## Open questions

1. How near: complete at the draw distance (about 12,500) with hilltops
   above it readable, or denser, closer to retail's ground (fully fogged
   by about 6,900)?
2. Is it acceptable that the lowest few degrees of the sky fog over? It is
   what hides things appearing against the low sky.
3. With anti-aliasing and upscalers off, is a faint dither for about a
   second at the far end acceptable, or should the dissolve run only with
   TAA or an upscaler?
4. When flying above the fog, should its top rise with the camera (more
   sky fogged), or should the dissolve alone handle what appears below?
   (Answered 2026-10-26 and 2026-10-29: it rises, half its band below the
   eye, and the sky below the horizon takes the fog's colour.)
