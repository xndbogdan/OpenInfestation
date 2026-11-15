# The water's memory: wakes, foam, spray and the wet hull

Status: built 2026-11-15, Vulkan only (raster and path traced). Retail's
renderers (software, OpenGL) and the game are untouched: everything here
is a picture.

## What was reported

The splash effect looked sloppy and cost too much, and a vehicle half in
the water did not look like it: rings glued round the wheels that slid
along with them instead of being left behind, a swirl of warped mirror
round a wading craft, no foam trail, no spray off a lake, a dry hull and,
for a blast, white beads and nothing at all over Vulkan's raster.

## What changed

**The water's memory** (`vk_waves.comp`, `vk_device/wave_field.rs`,
`scene_waves.rs`). The water no longer works every ripple out from a list
of up to 128 splashes in every pixel (three or four loops of it per water
pixel in the tracer, which culled none). A window of 1024 by 1024
world cells, 8 units each, round the eye keeps the water's height, its
rise, its foam and its churn, a texel a world cell, wrapped with the world
so the window moves with the eye without copying (a cell the window takes
in starts still). Each frame steps it on the game's clock (4 ms steps,
at most 12 a frame), as a damped wave equation whose speed is the water's
depth's (`sqrt(g depth)`, 60 to 450 units a second): waves slow in the
shallows, die at the shore and at the window's rim. What stirs it is
stamped in: retail's splashes and where an eruption's water falls back
(pressing the middle down and the ring round it up: a stamp adds no water,
or a busy spot sinks into a pit), the blasts' domes, the craft's wheels and
hull (lifting the water ahead and pressing it behind by their velocity, so
a craft at rest leaves it flat and a moving one throws a bow wave and
leaves a V-shaped wake behind it, as in Crest's and Sea of Thieves' boat
wakes), and where the craft's spray falls back. Foam is laid by the
splashes, the craft (by its speed squared) and crests too steep to hold;
it spreads a little and fades over 3 s, so a wake leaves a trail that
breaks into lace. The water's shading (`pt_splash.glsl`, shared) reads the
heights' slopes, the foam (its square root into `foam_cover`, whose froth
was tuned for a young splash's whole foam) and the churn, one texture read
each instead of the loops. The splash buffer keeps only the eruptions,
whose glow under the water still reads it.

- The depth under the window comes with the feed: 66 by 66 points of the
  terrain's 128-unit cells, `plane - ground` (`WaveGround`, world_scene's
  `feed_splashes`).
- Vulkan's raster steps it once its draws are in, with the craft's wakes
  (`State::raster_lamps`, now every frame): its draws lay no wheels, so the
  craft's hull points stand in for them (`raster_wakes`); the raster's
  puddles take those wakes too. The tracer steps it with its splash upload.
- The raster's per-workgroup splash culling (`cull_splashes`) is gone; so
  are the tuning's `water.spread` and `water.width`. `water.rings` is now
  how steeply the waves tilt the surface (1: as their heights make them).

**Spray in the lakes** (`craft_spray.rs`): the puddles' spray now flies
off a lake too, from each ground point the water stands over by no more
than a wheel's height (wading), and from the hovercraft's points riding up
to 120 units over the surface, whose cushion throws its least spray even
at rest. Where it falls back on a lake it splashes the water's memory.

**The wet hull** (`craft_wet.rs`, `pt_wet.glsl`'s `craft_wetness`): the
craft the camera follows is wet up to a little over the highest water it
stood in, drying over 20 s out of it; the rain's model wet takes that
line (metal mirrors more and smoother, paint takes its clear coat, bare
rubber darkens), within 220 units of the craft's middle, in the tracer and
the raster's metal.

**A blast in the water** (`scene_spray.rs`): its jets throw 56 and 28
pieces, not 96 and 48 (the traced spray's cost), and it leaves mist, a
column up its plume and a ring rolling out over the water (its base
surge), as puffs: the tracer's mist grid takes them as it takes the
thrusters' trails (lit, shadowed, its shafts), and Vulkan's raster, which
traced no spray and drew none, draws the plume's drops and the mist as
soft discs (`eruption_drops`), the world's tint halfway to white.

## Measured

`.tmp/wmk/` (a kit of copies of the player's settings, path traced with
DLSS Ray Reconstruction and the same without rays): 3840x2160 shown,
`--fixed-step-micros 16000`, 330 frames, RTX 5080.

| Scene | Before | After |
| --- | --- | --- |
| Pelagia, buggy driving into the water firing (`pel90`), traced: trace stage mean / most | 7.56 / 16.59 ms | 6.26 / 16.19 ms |
| The same under the raster: GPU a frame, mean / most | 4.47 / 9.39 ms | 4.18 / 8.23 ms |
| Tabuchi, hovercraft across the lake (`hover`), traced: trace stage mean / most | 4.48 / 6.62 ms | 2.54 / 4.10 ms |

The memory's own steps were not timed apart from the frame. The traced
spray's worst frames (the 16 ms) are its drops, fewer now but
still tested one by one per ray.

## Held, skipped

- One window, 8 units a cell, 8,192 units across: the waves of a splash
  past about 3,300 units from the eye are not kept (the window's rim
  fades). Shortcut: cascades (a coarser window round it) if far splashes
  are missed.
- The waves are a wave equation, not deep water's dispersion: a splash
  leaves a train of rings, but its far rings are not the longer ones.
- The wet hull is not dripped on, and the raster's tyres darken only in the
  rain (their faces take the metal pipeline only then).
- The traced spray's drops are still tested in a loop (`pt_spray.glsl`),
  not in the hardware's ray tracing: worth it only if the blasts' worst
  frames matter.
- Tuning by eye in the game: the stamps' strengths (`scene_waves.rs`), the
  wave speed and damping (`vk_waves.comp`), the mist (`scene_spray.rs`).
