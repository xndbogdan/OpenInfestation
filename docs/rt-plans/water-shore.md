# Shorelines: plan

Status: steps 1 to 3 built (2026-10-23); step 4 is not needed (no line remains in the probes). The
wobble follows retail's shore shapes the cheap way: the fade takes the
smaller of the traced depth and the depth over the ground drawn bilinear
between its grid points, whose contours curve within each tile, plus a
slow noise.
Code references are to `main` at `7686c4b2a`.

## What was reported

The edges where water meets the land are very obvious and ugly; the water
should be able to go past its edge.

The screenshot (Vulkan, path tracing on) shows a narrow stream between
sandy dunes. The water is a flat patch whose edge is a chain of straight
cuts with sharp corners, traced all round by a thin dark blue-grey line.

## Where it shows

Level 1 (world 18) has the screenshot's sand and grass, and many narrow,
shallow streams. The stream at cells X 179-182, Z 430-446 reproduces the
report. It is 2 to 3 cells wide, 20 to 70 units deep, with banks 400 units
high. The buggy parks above its south end:

```json
{"world": 18, "start": 0, "form": 1, "hints_seen": "all", "position": [23232, 57920], "heading": 180}
```

A second view looks down the stream from the north: position
`[23488, 54336]`, heading 0. (Heading 0 faces +Z, 90 faces +X.)

Frames, crops and scripts are in `.tmp/plan-shore/` (not tracked):

- `user-shore.png`: the reported screenshot; `figures/user-shore-zoom.png`
  zooms in on its water.
- `figures/s180vk-zoom.png` (Vulkan) beside `figures/s180gl-zoom.png`
  (OpenGL, retail's look): the report, reproduced. Full frames:
  `s180vk.png`, `s180gl.png`; north view `n0vk.png`, `n0gl.png`.
- `figures/s180vk-edge2x.png` beside `figures/s180gl-edge2x.png`: one
  edge close up. The tracer's edge is straight and outlined; retail's
  curves, with a pale rim.
- `figures/s180-overlay-zoom.png`: where the water meets the ground,
  computed from the map (cyan), and the water mesh's outer edge (red).
- `figures/s180clear-edge4x.png`: the dark line survives water made
  infinitely clear.
- `figures/w23vk-zfight.png`, `figures/w23vk-zoom.png`,
  `figures/w23gl-zoom.png`: the speckled patches on world 23.
- `figures/shore-sprites.png`: retail's shoreline sprites.
- Scripts: `figures/shore_scan.py` (reads the extracted terrain),
  `overlay.py`, `outline_metric.py`, `step.py`, `halo.py`.

## What we found

### 1. The dark line: the refraction ray starts inside the bank (verified)

The water shows what lies under it by sending a ray through its surface
(refraction). That ray starts half a unit under the surface
(`pt_trace.comp:978`, `position - surface.face * 0.5`). At the waterline
the water is thinner than half a unit, so the start point is inside the
bank. The ray goes down through solid ground and meets nothing. A miss
from above takes the water's own deep colour, `body` (the miss argument at
`pt_trace.comp:979`, returned at `:699-700`). That is the dotted
one-pixel line.

Evidence. A `modern-tuning.json` beside the settings made world 18's water
infinitely clear and unreflective (`clarity` 1,000,000, `reflect` 0). The
line stayed (`figures/s180clear-edge4x.png`). Its pixels, (91, 91, 70)
and (90, 90, 69), are exactly the colour the same pixels take when the
water is made opaque (`clarity` 1). In a 7-pixel band along the waterline,
1,284 of 13,791 pixels (9.3%) show that colour (`outline_metric.py`).

The half-unit drop is not needed. The tracer keeps the water in a group of
its own that each ray can include or skip (`vk_device/trace.rs:1839`), and
the refraction ray skips it (`MASK_SOLID`, `pt_scene.glsl:45`), so it
cannot hit its own surface.

### 2. The straight cut: flat water meeting a faceted ground (verified)

The ground is a grid of flat triangles, 128 units apart at full terrain
detail, as in the probe's settings (256 at the low setting). A flat surface
cuts each triangle along a straight line. So the shoreline is a chain of
straight pieces that bend only where triangles meet. World 18's water is
flat: its flags are `0xA0` (`world_scene_tests.rs:2315`), without the
wave bit `0x40`.

The water mesh already goes past the visible edge. It covers every tile
with a corner below the water (`scene_water.rs:55`, `:101`), as retail
does. Its outer edge joins points whose ground is at or above the water.
There the game puts the water exactly at its plane
(`inf-formats/src/native_basis.rs:760-761`, `:796`), so that edge lies
under the bank.
`figures/s180-overlay-zoom.png` projects both onto the frame through the
run's camera. The cyan line (where the plane crosses the ground's
triangles) traces the visible edge, jogs included. The red line (the
mesh's outer edge) lies well up the banks, hidden. Extending the mesh
would only add more water under the banks.

### 3. The water's look stops all at once on that line (verified)

At the waterline the water still has its full least reflection (the share
of the sky it mirrors even looking straight down: `least`,
`pt_trace.comp:962-963`; 6% on world 18), its ripples and its sun glint
(`:998`). Only its own colour fades with depth (`:981`).

The ground under the water is also lit another way than the ground beside
it. Through the water, `shade_ray` lights it with the sun and a flat
ambient (`pt_trace.comp:720-722`). Dry ground gets the traced bounce light
and a soft sun shadow, both smoothed by the denoiser (`:1194-1215`, then
the resolve, the pass that combines the lighting into the final colour).

Measured with the water fully clear (`step.py`, 31 pieces of shoreline
away from the buggy): the water side is 4% darker at the median, from 20%
darker to 9% lighter, and more than 10% darker on a third of the pieces.
In places it is bluer by up to 20 levels (the reflection). Which share of
that is lighting, ripples or caustics will show once step 2 removes the
rest.

### 4. Ground exactly at the water's level speckles (verified on world 23)

Where a grid point's ground is exactly at the plane, the water there sits
exactly on the ground. Rays then hit one or the other at random per pixel
(z-fighting): speckled patches with straight tile edges
(`figures/w23vk-zfight.png`, world 23's marsh, position `[1600, 6080]`,
heading 0). World 19's map (world 0 uses it too) has 1,281 such cells,
world 23 has 281, the other water worlds none, so 0 and 19 should show it
too (not run). Retail draws white rounded puddles there
(`figures/w23gl-zoom.png`).

### 5. How retail does it (observed)

Retail draws each shore tile with one of 16 entries chosen by which
corners are wet (`terrain_draw/water.rs:200`, the table at `00537010`,
[WATER_RENDERING.md](../re/WATER_RENDERING.md)). They use
five sprites, 1331 to 1335, rotated. Each is water with a curved edge, and
in most worlds' sets a lighter rim (`figures/shore-sprites.png`). In the
OpenGL frame the stream is a translucent band whose edge curves within
each tile, with a pale rim. It covers about the same area as the traced
water (`figures/s180gl-overlay-zoom.png`). Under the tracer the raster
skips its own water (`software_scene/terrain.rs:63-71`,
`software_frame.rs:1094`), so the two never mix.

### 6. Suspected, small: a faint dark rim on the dry side

The denoiser (`pt_atrous.comp:49-67`) averages a pixel's bounce light with
neighbours whose normal agrees and which lie on its plane. Water pixels
store no bounce light (`pt_trace.comp:1179`). At a gentle shore they can
pass both tests, which would darken dry ground for a few pixels. Measured
(`halo.py`): the dry side is about 4% darker 2 to 3 pixels from the line
than 10 pixels away. Retail's frame also darkens towards the water (its
baked shade), so this cannot be told apart from the dark line yet.
Recheck after step 1.

Not the cause: the composite (`pt_composite.frag:132-134` copies colour
and depth), anti-aliasing (off in the reported settings), the raster's water
(skipped), waves (none on world 18).

## Proposed approach

Make the water fade in from its edge instead of starting at full strength
on a polygon, and give the dry side a damp band. The eye then reads a
soft, irregular shore, and the triangles stop showing. Four steps, each
checked before the next.

### Step 1: two small fixes

- **The dark line.** Start the refraction ray half a unit back towards the
  eye (`position - direction * 0.5`) instead of half a unit under the
  surface. That point is always in open air (or water, looking up from
  under it), so the ray cannot start inside the bank. The caustics'
  `bottom` (`pt_trace.comp:985`) uses the same start.
- **The speckle.** Where the water would sit on the ground at a grid
  point, lower that mesh point to 2 units under the ground. That is ground
  exactly at the plane, and on worlds with waves, a trough the game clamps
  to the ground. Points already under the ground stay where they are, so
  the water never climbs a bank. Only the drawn mesh moves, only where
  nothing can float, and the normals keep the game's heights.

### Step 2: the water fades in from the shore

- **Depth.** Take the depth under the surface from where the refracted ray
  met the bottom (already traced). Over the first `shore` units of depth
  (a new tuning value, about 24 on world 18, whose heights step by 16),
  scale the reflection (its least share included), the ripples and the
  glints up from zero. At depth 0 the water adds nothing, so the line
  where it meets the ground has nothing left to show.
- **Wobble.** Offset that depth by a slow noise over the world (the
  shader's `wrap_noise`), a few units either way. The soft edge then
  wanders instead of following the triangles. On a gentle bank a few units
  of depth move the edge tens of units sideways; on a steep bank, hardly
  at all, as on a real shore.
- **Foam line.** A thin, broken white line where the depth is near zero,
  lit as the splashes' foam is (`pt_trace.comp:1016`). It plays the part
  of the pale rim on retail's shore sprites.

### Step 3: a damp band past the edge

This is the "water past its edge" part. Ground just above the water (its
height over the plane under a new tuning value `wet`, with the same
wobble) gets darker, as wet sand does, and takes the other half of the
foam line. The ground seen under the water gets the same darkening, so
nothing changes on the line itself. The band is keyed to the water's
plane, which is the waterline on every water world but 6: only worlds 6
and 24 have waves (H14 flags), and 24 is lava.

### Step 4, only if a line still shows: shallow water as a layer

After steps 1 to 3, one difference is left on the line: under the water
the ground is lit by `shade_ray`, beside it by the full path. If that
still reads as a line, shade the bottom of shallow water as ordinary
ground and lay the water over it:

- On a water hit whose bottom shows, keep tracing along the refracted ray,
  and run the normal surface path for what it meets: albedo, normal, the
  shadow and bounce rays, the denoiser.
- Write the water's own light (reflection, glint, foam, its colour
  scattered back) and how much of the bottom shows through into a new
  image.
- The resolve takes bottom times see-through plus water light. At the
  edge see-through is 1 and water light 0, so both sides match exactly.
  The damp band's sheen can use the same image.
- Deep or murky water, where the bottom does not show, stays as it is.

Recommendation: steps 1 to 3 together, then measure; step 4 only if the
line still shows. Step 2's fade and wobble carry over into step 4
unchanged.

## Alternatives considered

- **Extend the water mesh past retail's tiles** (an idea, taken
  literally). Checked: the mesh already reaches past the visible edge,
  under the banks (the red line). More of it changes nothing on screen.
  Raising it to show would float a sheet of water on the banks.
- **Retail's shoreline sprites as the water's outline.** Recognizable, and
  their curves hide the triangles. But each is a fixed shape per tile,
  blind to the depth. On a bank above the water the tracer cannot show
  water at all (the ground hides it), so they could only cut the water
  back. They could shape step 2's wobble instead.
- **Smoother ground near water** (each shore tile split into smaller
  triangles on a curved surface). It curves the shoreline itself, but the
  drawn ground would leave the shape vehicles drive on by a few units, and
  the split tiles need crack-free joins with their neighbours. More work
  than steps 2 and 3 for less.
- **A smooth depth field** (512 by 512, built with the ground mesh,
  sampled with smooth interpolation). A refinement of steps 2 and 3 if the
  noise is not enough, and a way to keep the damp band off dry flats far
  from water.
- **Dithered see-through** (some pixels show the bottom, some the water).
  Noisy without anti-aliasing, which some players play without.

## What changes where

Step 1:

- `pt_trace.comp`, `water_color` (`:951-1020`): the refraction start
  (`:978`) and the caustics' `bottom` (`:985`).
- `scene_water.rs` (`:51-66`, `:83-94`): where a point's height is not
  above its ground, its drawn height becomes the lower of that height and
  the ground less 2. Vertex positions only; the normals keep the game's
  heights. A test for ground exactly at the plane beside the existing one.

Step 2:

- `pt_trace.comp`, `water_color`: the depth, the fade on `fresnel`,
  `least`, the ripple normal and the glints, the foam line.
- Tuning `water.shore`, `water.wobble`, `water.lap` (fade depth, wobble
  in units, foam line strength): `modern_tuning.json` and `WaterTuning`
  (`modern_tuning.rs:185`), `pt_frame.glsl` (beside `water_clarity`,
  `:151`), the frame fill in `vk_device/trace.rs` (`:2354-2386`).

Step 3:

- `pt_trace.comp`: the primary ground path, before its albedo is stored
  (`:1238`); `shade_ray` (`:716-723`) for ground under the water.
- Tuning `water.wet` (band height in units, 0 for none) and `water.damp`
  (how much darker), per world in `worlds`.

Step 4:

- `pt_trace.comp`, the water branch of `main` (`:1174-1187`): trace on to
  the bottom and run the surface path. The geometry image takes the
  bottom's normal and its distance along the pixel's ray (surface plus
  refracted path), so the temporal and denoise passes need no change. The
  depth the raster reads stays the water's surface, so sprites under water
  stay hidden.
- `pt_resolve.comp` (`:46-53`): bottom times see-through plus water light,
  before the haze.
- `vk_device/trace.rs`: the new image and its bindings.

Docs, for each step: `docs/re/PIXEL_RUNTIME.md` (modern mode, Water)
and `docs/INFESTATION-RT-IMPLEMENTATION.md` (Water).

## Floating things

They ride the game's own wave heights (`00410F60`), which the mesh keeps
wherever there is water over ground. Step 1 moves the mesh only where the
water is at or under the ground, where nothing floats. Steps 2 and 3 only
change shading. Step 4 shows a floating thing's underwater part through
the layer, lit as the rest of it is.

## Cost

- Steps 1 to 3: a few arithmetic operations and two noise lookups per
  water or near-shore ground pixel. Estimated well under 0.1 ms.
- Step 4: one 16-bit RGBA image at the render size (66 MB at 3840 by
  2160), and about two more rays per shallow-water pixel (a shadow and a
  bounce), against about six there now. Estimated a few tenths of a
  millisecond at 4K; to measure. Today these views trace in 6.7 to 9.8 ms
  at 3840 by 2160 (the probe logs' "Vulkan tracer" line).

## Risks

- Step 2 makes the shallows of murky water (worlds 5, 6, 22, whose least
  reflection is 20%) more see-through near the edge than now. That is the
  point, but murky water was meant to stay reflective; only the
  first `shore` units of depth change.
- Step 3 on world 6: its waves move the waterline by up to about 40 units
  at the shore, so a band keyed to the plane will not follow it. Either a
  wider band there, or the shader reads the mesh's own heights near the
  eye.
- Step 3 on flats just above the water far from any (world 23's marsh is
  at the plane; dry stream beds): they would look damp. Per-world tuning
  (snow may not want it at all), or the depth field above.
- Step 4: under rippled water the bottom moves, so the temporal pass keeps
  less history there: a little more noise, dimmed by the water. Motion
  vectors follow the bottom, so with a temporal upscaler the reflection
  may smear slightly. Under Ray Reconstruction the guide images for these
  pixels need a decision.
- Lava (worlds 17 and 24) keeps its own look; none of this applies to it.

## Verification

Same probe as above (`--renderer vulkan`, and `--renderer opengl` for
retail's look), world 18 south and north views and world 23's marsh;
`modern-tuning.json` beside `run.json` for the clear and opaque runs.

- Dark line: `outline_metric.py` on the clear run. Today 1,284 of 13,791
  band pixels (9.3%) show the water's deep colour; expected about 0 after
  step 1.
- Step across the line: `step.py` (leave out pieces on the buggy). Today
  the water side is 0.96 of the dry side at the median, 0.80 to 1.09;
  expected nearer 1 after step 2, and 1 after step 4.
- Speckle: world 23's crop should show no water-coloured speckle after
  step 1.
- Dry-side rim: `halo.py`, again after step 1.
- Side by side with the reported look: the same crops before and after, and
  OpenGL for retail's.
- Elsewhere unchanged: Tabuchi's lake (world 9, `[51978, 37383]`),
  Altair's deep water (world 5, `[28858, 47177]`), the F12 view under
  water there, lava (world 17). GPU time from the log.

## Open questions

- How soft should the edge be: a narrow fade (about a quarter of a tile)
  or a broad one?
- Should the edge creep up and back slowly, like small waves lapping? Most
  worlds' water is still in retail (only 6 and 24 have waves).
- A white foam line at the edge, like the pale rim on retail's shore
  sprites, or none?
- A damp band on the dry side: how high, and on which worlds? Snow (23)
  and ice (3) may not want it.
- Ground under water would look a little darker (wet). Is that fine?
- If a faint line remains after steps 1 to 3, is step 4 worth its cost?
- Should the edge's wobble follow retail's shore-sprite curves rather than
  a noise?
