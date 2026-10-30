# Water that repeats: plan

Status: chosen 2026-10-23: `r18b_murky.png` (the best by far:
the bed hidden, no rings) and `sb_199.png` (world 1's sandbar as played).
Built that day: step 4 (rings 0.15, capped), step 2's constants (focus
0.6, cap 2.2), and world 18's clarity 40, so its river mirrors the sky
while the banks keep their sand. Steps 1, 3, 5 and the full-bend caustics
wait, in case the tiles still show.
Code references are to `main` at `55915c1d0`.

## What was reported

The water looks repetitive; in the distance beyond the ACAM it looks
tiled, which makes it look bad; up close it distorts in an ugly way, or
too much.

The screenshot (`.tmp/plan-water-tiling/user-water.jpg`, Vulkan with path
tracing) shows a hovercraft on a river between sand dunes. From just past
the craft to the far bank, the water is a regular grid of bright golden
cells. Close to the camera it is a large, smooth swirl, like warped metal.

## Where it shows

The river of world 18 (Command Centre, "Level 1") matches the screenshot:
the same dunes, ridge line and grid (figure 01). The shore plan found the
user on this world too. The same grid shows on world 1's islands, on any
world with clear water over a sunlit bed, and more faintly wherever the
ripples mirror a bright sky.

The usual setting is 2560x1440 with SSAA 4x (the install's `settings.json`,
read only), so the tracer runs at 5120x2880.

Probes ran from the exe copy in `.tmp/plan-water-tiling/` under Vulkan with
path tracing, fixed 16 ms steps, 200 frames, a snapshot at frame 199 (wave
clock 157), output 3840x2160. The two views:

```json
{"world": 18, "start": 0, "form": 3, "hints_seen": "all", "position": [36352, 63488], "heading": 8192, "upgrades": [1, 2, 165, 166, 167]}
{"world": 1, "start": 0, "form": 3, "hints_seen": "all", "position": [59904, 38400], "heading": -4096, "upgrades": [1, 2, 165, 166, 167]}
```

World 18: the hovercraft floats on 115 to 180 units of water and looks west
(-X) down the river. World 1: 356 units deep, looking north-west at a sand
island. `heading` is in degrees, as the start harness reads it
(`harness_start.rs:19`; 0 faces +Z, 90 faces +X), so 8192 is 272 and
-4096 is 224.

A/B runs put a `modern-tuning.json` beside the settings:

| Snapshot | Tuning | What it isolates |
| --- | --- | --- |
| `r18b.png`, `hx_-4096.png` | none | as played |
| `r18b_norings.png`, `hx_norings.png` | `water.rings` 0, `water.foam` 0 | no splash rings |
| `hx_flat.png` | rings off, `sky.modern` true, `sky.clouds` 0 | a plain sky: what the water's shape alone does |
| `r18b_murky.png`, `hx_murky.png`, `hx_flat_murky.png` | the above, and the world's `clarity` 1 | the bed hidden: only the mirror is left |
| `hx_rings015.png`, `hx_rings025w20.png` | `rings` 0.15; `rings` 0.25 with `width` 20 | softer rings |

Figures are in `.tmp/plan-water-tiling/figures/` (listed at the end).

## What we found

On these worlds the water's shape is the game's flat plane, plus eight
fixed ripple waves (`ripple_field`, `pt_trace.comp:764`) and the splashes'
rings. Everything below follows from that shape.

### 1. The tiles are the caustics' grid on the bed (verified)

Caustics are the bright, moving net of light a rippled surface throws on
the bottom. The tracer fakes them (`caustic`, `:815`): where the refracted
ray meets a sunlit bed, the bed's light is multiplied by
`1 / (1 + depth x curvature)` of the ripples above (curvature: how sharply
the surface bends). The result is capped at 3, floored at 1/0.3, and fades
out between 250 and 800 units deep (`:821-822`). The focusing is four times
what water's index gives (`CAUSTIC_FOCUS = 1.0`, `:808`; physical is
0.25).

The ripples' curvature reaches plus or minus 0.006 per unit (in 98% of
places). At 160 units deep (world 18's river) or 350 (world 1),
`depth x curvature` reaches plus or minus 1 to 2, so the formula
saturates. On world 18's river bed:

- Nine points of the bed in ten get between 0.49 and 3 times their light
  (5th and 95th percentile); 15% sit at the cap of 3. Capped cells are
  flat-topped bright blobs with sharp edges: the golden cells of the
  screenshot.
- The bed is 35% brighter on average than without caustics (mean 1.35).
  Real caustics move light around; they do not add it.

The evidence that this is the grid:

- **Hiding the bed removes most of it.** With the bed hidden (clarity 1)
  and a plain sky, the grid's energy (the image's contrast at the ripples'
  sizes, 40 to 160 units, in a world patch 1,200 units ahead, measured by
  `scripts/lattice_metric.py`) falls to 1.3% of the plain-sky run on world
  1 (figure 03). With retail's painted sky it falls to 15%.
- **The pixels match.** A replica of the shader in Python
  (`scripts/water_model.py`, `caustic_check.py`) predicts the caustic
  pattern per pixel. On `hx_flat.png` it correlates 0.75 with the image,
  pixel by pixel; with the bed hidden, -0.04 (figure 04).
- Caustics came 19 minutes after the fix for the earlier complaint
  (`fef7e4d85`, then `1c6b11ef7`). They copy the ripples' grid onto the bed
  at high contrast.

### 2. The ripples are two crossed pairs of waves (verified)

`ripple_field` sums eight fixed waves in two sets of four
(`pt_trace.comp:769-777`). `lead`, a slow field, picks which set shows.
Within each set, two families of waves cross at nearly a right angle with
nearly the same length:

| Set | Strongest waves (length, direction) | Angle between |
| --- | --- | --- |
| 1 | 68 units at 19 degrees, 71 at 125; with 55 at 109 and 41 at 25 | 106 degrees |
| 2 | 134 units at -28 degrees, 109 at -122; with 90 at 32 and 78 at 147 | 94 degrees |

Two crossed waves of the same size make an egg-crate: a regular grid of
bumps. The mirror shows it too (the grid left in `r18b_murky.png`): on world
18, whose sky is bright cloud on blue, 71% of the grid's energy stays at
1,200 units ahead with the bed hidden, and 24% at 500 to 1,000 units.

In a patch of the game's image (`scripts/topdown.py` on `ab_norings.png`,
world 1 facing +Z), the strongest frequencies are those of the replica's
ripple slopes over the same patch: 63 units at (-437, 947) waves per wrap
first in both, then 46 units at (-291, 1384). The drift shifts them a
little from the table's values.

The slow fields of `fef7e4d85` (chop, lead, drift) only vary the grid
between patches. Inside a patch, about 700 to 1,400 units across, it stays
regular.

### 3. The slow fields sit on square cells (shown; a minor cause)

`wrap_noise` (`:739`) is value noise: random values at the corners of an
axis-aligned square grid, blended between. Its cells are 682 (chop), 1,024
(drift) and 1,365 units (lead) wide. `lead` is sharpened
(`smoothstep(0.3, 0.7)`, `:767`), so 55% of the water is one set or the
other outright. Figure 08 shows the square cells; in play they show as
patches of different texture with straight-ish edges. This may be part of
"tiled", but we did not isolate it.

### 4. Up close: the splash rings, the blobs and the warp

- **Splash rings (verified).** The hovercraft keeps throwing retail's
  splash particles, and each rings the water (`splashed`, `:942`). A ring
  tilts the surface by up to 0.41 (22 degrees) at `rings` 0.5; several
  overlap, and nothing caps their sum. The ripples tilt it by 0.064 at most
  in 99% of places. That is the swirl of warped mirror around and behind
  the craft: it goes with `rings` 0 (figure 05). The craft's lamps glint on
  the steep ring slopes as white streaks (`:1011`), which go with the rings.
- **Caustic blobs.** Near the camera the capped caustic cells are large,
  hard-edged golden blobs (`r18b_norings.png`).
- **The drift's warp (measured, suspected).** `drift` moves the ripples by
  up to 400 units over 1,024-unit cells (`:768`). Its slope is 0.18 on
  average and 0.57 at most, so the ripples stretch and bend by up to 57%
  across a few hundred units: a melted look.

### 5. Far off: aliasing (measured; minor at the usual settings)

The tracer takes one sample at each pixel's centre, and the water is not
denoised (`:965-966`). Waves smaller than two pixels then alias (show as
false, larger patterns). Comparing one sample against 16 per pixel in the
replica (`scripts/alias_check.py`, figure 09), the ripples' slope keeps
false detail beyond about 4,000 units at 3,840 pixels wide (its spread is
0.0173 against 0.0108 at 5,500 units, 0.0180 against 0.0067 at 8,900). The
grid itself sits at 500 to 3,000 units, where nothing aliases yet. The
user's SSAA traces 5,120 wide and averages four samples a pixel, which
pushes this further out. It still shows as shimmer near the horizon.

### 6. Ruled out

- **The game's swell.** `00410F60`'s three sine waves run only where the
  world's flags have `0x40` (`terrain_runtime.rs:364`, flags at H14 +4,
  `world_assets.rs:246`). Only worlds 6 and 24 (lava) have it; world 18's
  flags are `0xa0`, world 1's too. So on these worlds the water mesh is
  flat, its normals point straight up, and the 128-unit mesh grid and the
  swell fade beyond 1,500 units (`rippled`, `:795`) change nothing. World
  6 shows the swell as broad smooth bands (figure 10), not a grid. A
  replica with the swell on drew dark bands the game does not show.
- **The sun's glint.** World 18's sun stands 74 degrees up, world 1's 57.
  For the eye to see the sun's glint (`:998`), the surface must tilt
  towards it by 13 degrees or more close to the camera, and 23 or more a
  thousand units out. The ripples tilt 6 at most. No sun glint shows in
  any probe. (A splash ring, at 22, could.)
- **The painted sky.** The grid stays with a plain sky (`hx_flat.png`).
  The sky's clouds only add contrast to what the ripples mirror.

## The plan

Keep what the game owns: the water's heights, the mesh, what floats, the
splash particles. Change only how the traced water shades. Each step is
small and can be checked on its own.

### Step 1: ripples without a grid

Replace the eight waves with about 24 that never line up:

- Lengths spread evenly on a log scale from about 30 to 320 units.
- Directions spread within about 70 degrees either side of one wind
  direction, by a low-discrepancy sequence (each next angle a golden ratio
  further on), so no two similar waves cross at right angles.
- Steeper slopes for longer waves (slope proportional to length^0.75),
  totalling about today's typical tilt (a root-mean-square slope of
  0.035).
- Whole numbers of waves across the 65,536-unit wrap, as now, so the
  pattern meets itself where the world wraps.
- Each wave's rate (radians a tick) proportional to the square root of its
  wave number, as deep water's waves go, so long waves outrun short ones
  and the pattern does not repeat in time either.
- `lead` blends the long half of the set against the short half (calm
  against choppy patches) instead of two crossed sets.
- `drift` smaller (about 150 units) or over larger cells, so the warp's
  slope stays under about 0.15.

The table stays a constant array in the shader, generated once by a short
script (`scripts/water_model.py`, `rich_waves`, kept with the plan). This
removes the grid from the mirror and from the caustics at once.

### Step 2: caustics that look like caustics

- Take the bed's light from the full bend of the refracted beam:
  `1 / |det(I + s x depth x H)|`, where `H` is the ripples' 2x2 matrix of
  second slopes. Today's formula keeps only its trace (the curvature). The
  full form gives thin bright lines where the light folds, the familiar
  net, instead of round blobs. It costs three running sums per wave
  instead of one.
- Strength `s` about 0.6 (2.4 times physical) instead of 1.0 x 4, the cap
  about 2.2 instead of 3, the floor about 0.45 instead of 0.3. Then the bed
  stays near its own brightness on average.
- Fade the caustics with the pixel's footprint on the bed (step 3), so a
  far bed is evenly lit rather than speckled.

Figures 06 and 07 show today against the prototype, on the bed and from
the probe camera.

### Step 3: filter what a pixel cannot hold

- **Footprint.** How much water one pixel covers: across the view,
  distance times the pixel's angle (from `frame.focal` and the traced
  size); along the view, that divided by the sine of the angle between the
  ray and the water. Use the traced size, so SSAA and the upscalers keep
  the detail they can resolve.
- **Fade each wave** by `smoothstep(2, 4, length / footprint)` in
  `ripple_field`, for the normal and for the caustics. Far water goes
  smooth instead of shimmering.
- **Widen the glints by what was faded.** The faded waves' slope variance
  becomes roughness: the sun's and the lamps' glints (`:998`, `:1011`) use
  a lobe widened by it (an energy-normalised exponent from 600 down), so a
  low sun's path of glitter stays wide and steady far off. This is the
  standard trick (LEAN or Toksvig mapping) for filtered normals.

### Step 4: gentler rings near the craft

- `water.rings` from 0.5 to 0.15 in `modern_tuning.json` (`:36`). Figure 05
  shows 0.5, 0.25 with width 20, 0.15 and 0.
- Cap the sum of the rings' tilt at a pixel (about 0.2) in `splashed()`,
  so overlapping rings cannot stack into a mirror swirl.

### Step 5: slow fields off the square grid

Build each slow field from two layers of `wrap_noise` at different sizes
whose cell counts share no factor (for example 37 and 59 cells across the
wrap), and soften `lead` to `smoothstep(0.2, 0.8)`. Patch edges then
wander instead of following a square grid.

### Optional: two tuning values

`water.caustics` (the caustics' strength) and `water.ripples` (the slope
scale), global and per world, beside `clarity` and `reflect`, if they are
wanted for taste. Only if asked.

### What changes where

- `shaders/pt_trace.comp`
  - `ripple_field` (`:764-788`): the new table, `lead`, `drift`, a
    footprint argument and the fade, the second-slope sums and the faded
    variance as outputs.
  - `wrap_noise` (`:739`) callers: two layers each (step 5).
  - `rippled` (`:793-803`): passes the footprint, returns the faded
    variance. Its swell fade stays (it acts only on world 6).
  - `caustic`, `CAUSTIC_FOCUS` (`:808-823`): the full bend, the new
    strength, cap and floor, the footprint on the bed.
  - `water_color` (`:951-1020`): computes the footprint, widens the glints
    (`:998`, `:1011`).
  - `splashed` (`:920-946`): the cap on the rings' tilt.
- `src/modern_tuning.json`: `water.rings` 0.15.
- Only if the optional values are wanted: `modern_tuning.rs`
  (`WaterTuning`, `:185`), `pt_frame.glsl` (beside `water_clarity`),
  `vk_device/trace.rs` (the frame fill), and the tuning test.
- `scripts/compile-shaders.ps1` rebuilds `pt_trace.comp.spv`.
- `docs/re/PIXEL_RUNTIME.md`, "Water that varies" and "Caustics":
  record the new waves and caustics, and that the swell runs only on worlds
  6 and 24.

What stays: `scene_water.rs` and `spacing_sample` (the game's heights),
the mesh, the reflection and refraction rays, the Fresnel term (how much
the water mirrors, by angle) with each world's `clarity` and `reflect`,
the splash feed, the eruptions, lava.

The shore plan (`water-shore.md`, step 2) fades the ripples and glints
near the shore inside `water_color`, and the foam plan
(`underwater-and-foam.md`, 2a) reshapes the foam in `splashed()`. Same
functions: land them one after another, not in parallel.

## Alternatives considered

- **Only soften the caustics** (constants). Quickest, and it takes most of
  the grid off world 1. On world 18 the mirror keeps the grid (71% of it at
  1,200 units), so not enough alone.
- **A baked normal map.** A tileable texture of waves made once at start
  from a wave spectrum, sampled at two or three scales and angles with
  hardware filtering through the ray differentials the tracer already has
  for textures (`uv_steps`). Filtering would come free. But it is a new
  texture and binding, and the texture tiles too, which the scales must
  hide. Kept in reserve if step 3 falls short.
- **A full FFT ocean** (wave cascades computed on the GPU each frame). Most
  realistic, much more code, and the heights must stay the game's, so only
  the normals would gain. Too much for a lake in this game.
- **Temporal smoothing of the water.** Hides the far shimmer while moving,
  not the grid. The water is shaded once, in the trace, on purpose, to
  keep its mirror sharp.
- **More slow fields over the old waves** (what `fef7e4d85` did). The grid
  survives inside each patch, as seen.
- **A caustic pattern of its own** (cellular "net" noise) instead of one
  derived from the ripples. Cheaper and easy to shape, but no longer tied
  to the waves above; nobody would notice at these angles. The fallback if
  step 2's net looks too faint.

## Cost

No new rays. On the RTX 5080 at 3840x2160:

- Today's caustic block (its sun shadow ray and second `ripple_field`)
  takes about 0.75 ms in a water-filled view (trace 4.89 ms against 4.14
  ms with the bed hidden; the GPU is shared, so plus or minus 0.3 ms). It
  stays.
- 24 waves instead of 8, evaluated twice a water pixel: about 0.05 to 0.15
  ms more. The second-slope sums and the footprint fade: under 0.05 ms.
  Two noise layers: about 0.05 ms. The ring cap and the glint lobe:
  nothing measurable.
- Total about 0.1 to 0.3 ms at 4K; at SSAA (5120x2880, 1.8
  times the pixels) about 0.2 to 0.5 ms.

## Risks

- **Taste.** Every water world changes. Calmer or busier water, and
  fainter caustics, may disappoint. The values sit in one table and three
  constants; the optional tuning values make them tunable.
- **The wrap seam.** Wave numbers must stay whole over 65,536 units, or a
  seam appears where the world wraps.
- **The footprint.** It must follow the traced size under SSAA, the
  upscalers and Ray Reconstruction's jitter; check each.
- **Thin caustic lines alias.** The cap and the footprint fade guard
  against it; check the bed far off and in motion.
- **Shared functions.** The shore and foam plans edit `water_color` and
  `splashed()`; order the work.
- World 6's swell and its mesh normals are not touched here.

## How to verify

1. Rerun both probe views, as played and in the A/B tunings, before and
   after. With a plain sky and no rings (`hx_flat`'s tuning) the grid is
   easiest to see.
2. Numbers (`scripts/lattice_metric.py` on the world patch 1,200 units
   ahead): the grid's energy down by at least two thirds with the bed
   visible, and the share of its strongest 1% of frequencies down from
   about 0.3 to under 0.2. The Python prototype scores 0.30 today and 0.18
   after. Caustic statistics on the bed: mean 1.0 to 1.1, 95th percentile
   under about 2, under 2% at the cap.
3. Motion: hold Left for a second (`--input-replay`), snapshot every frame,
   and compare the frame-to-frame change beyond 3,000 units, before and
   after.
4. The usual settings: 2560x1440 with SSAA 4x.
5. Other worlds: 9 (Tabuchi's lake), 3, 13 and 23 (clear), 5 and 22
   (murky), 0 and 19 (red), 6 (the swell), 17 and 24 (lava, unchanged),
   and under water (the F12 recipe).
6. GPU time: alternate runs (A, B, A, B) of 600 frames each and compare
   the trace's milliseconds, since the GPU is shared.
7. Show the result, and tune.

## Open questions

1. Caustics: keep them? How strong? Figure 07 shows a softer and a
   stronger version beside today's. Same on every world, or stronger where
   the water is clearer?
2. Ripples: about today's size (30 to 320 units long), or smaller and
   calmer? One wind direction per world?
3. The rings around the craft: 0.15 (figure 05), softer still, or none?
4. Anything you like in today's look to keep, such as the bright golden
   dapples in the shallows?

## Files

In `.tmp/plan-water-tiling/` (not tracked):

- Figures (`figures/`):
  - `01-user-vs-world18.jpg`: the screenshot beside the world 18 probe.
  - `02-world18-ab.jpg`: as played, rings off, the bed hidden.
  - `03-world1-ab.jpg`: as played, rings off, a plain sky, a plain sky
    with the bed hidden.
  - `04-caustic-match.png`: the game's image beside the replica's
    caustics and its mirror.
  - `05-rings-ab.jpg`: rings 0.5, 0.25 with width 20, 0.15, 0.
  - `06-bed-caustics.png`: the bed's caustics today, with the full bend,
    and the prototype.
  - `07-preview.jpg`: the replica of today against the prototype, softer
    and stronger.
  - `08-slow-fields.png`: chop, lead, drift and the ripples' slope over
    16,384 units.
  - `09-aliasing.png`: one sample a pixel against 16.
  - `10-world6-swell.jpg`: world 6, the one water world with the swell.
- Snapshots and logs: `r18*.png`, `hx*.png`, `se_199.png`, `ab_*.png`
  (world 1 facing +Z), `w6_199.png`, and `out_*.txt` beside each.
- Scripts (`scripts/`; run with `py -3.11`, since the default Python's
  numpy crashes):
  - `water_model.py`: the replica (`ripple_field`, `wrap_noise`,
    `spacing_sample`, the mesh) and the prototype's waves.
  - `caustic_check.py`, `lattice_metric.py`, `topdown.py`,
    `alias_check.py`, `fields_map.py`: the measurements above.
  - `preview*.py`: the prototype's renders. `make_figures.py`: the
    figures.
  - `terrain_find.py`: finds a world's terrain in its overlay (world N's
    is `Overlay/(N+10).ovl`); `w1_h.npy`, `w18_h.npy`, `w6_h.npy` are the
    extracts, with `sine.json`, retail's sine table (`0053DF00`).
