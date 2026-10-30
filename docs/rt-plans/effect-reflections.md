# Effects in reflections: plan

Status: built (2026-10-23). Step 1 (each bolt its own light) first; step
2 under Eye candy only, (the remastered preset stays as it
is, and only Eye candy exaggerates further). Two things differ from the proposal below: the emitters come
from every additive quad the raster draws, read from its command as it is
submitted (not from the particle pool), and what lies at the water's
level is left out, since the hovercraft's spray alone filled the list.

Revised 2026-10-26 (everything
water mirrors): the emitters are read once from the whole deferred queue
(lasers, barriers, ribbons, gates and force fields included), beams are
segments rather than discs, a laser is one line light, additive model faces
are traced in the glow structure, puddles share the water's mirror and
glints, and Remaster mirrors effects too.

## What was reported

In the water only one central beam of plasma reflects, though the shots
come from 2 opposing sides (either alternating or dual with the upgrade).

The screenshot shows the hovercraft firing plasma over open water at
night. The two blue streams of bolts fly left and right, side by side
in pairs, and in the water one blue streak runs down the middle between
them.

## What we found

Verified on Tabuchi's lake (world 9, the hovercraft firing plasma), with
a diagnostic build that logged the bolts and the lights the tracer got.
The build was then thrown away.

- **The twin bolts share one light.** With Dual fire plasmas (grant
  `0x9E`), both guns fire at once: ten bolts in flight make five
  side-by-side pairs, 136 units apart (guns at x -70 and +66). The scene
  feed gathers shot lights closer than a quarter of their reach into one
  (`SceneFeedBuilder::glow`, under `SHOT_GLOWS`). A plasma bolt's reach is
  700 (tuning `shots` 400, times the bolt's scale 1.75), so the pair's
  lights merge at the middle (x -2, two faces). The water's glint of that
  one light draws the central streak. The same port run gives the same
  picture as the reported screenshot (`d230.png` in `.tmp/vkreflect`). The
  merged light also lights the ground from the middle, not from the two
  bolts.
- **Alternating fire is right.** Without the upgrade the guns take turns,
  and consecutive bolts are 460 units apart along the flight. Each bolt
  keeps its own light, and each glints in a column under itself
  (`q230.png`). The merge never gathers bolts from one gun: they are
  always further apart than 175.
- **The bolts themselves never show in a reflection.** Retail's bolts,
  flames, explosions and sparks are blended sprites. The raster draws them
  over the traced image (`scene_capture.rs`: "Additive, darkening and
  half-alpha sprites stay blended on the raster"), so they are not in the
  tracer's scene. A ray mirrored off the water or a metal surface cannot
  see them. All the water shows of a bolt is its light's glint
  (`pt_trace.comp`, `water_color`), a column of highlights in the ripples
  under it.

## Proposed approach

1. **Each bolt its own light** (small, ready to build). The bolts' lights
   skip the merge: `world_scene.rs` pushes each bolt as its own glow. At
   the normal rate the merge gathered only the twin pair. Rapid fire and
   enemies' bolts still need checking.
   - Twin fire then lights and glints from both streams, with twice the
     light of one gun, as two guns should give.
   - The count stays bounded: ten bolts in flight in this run, against
     the tracer's 64 lights (nearest first).
   - The lasers keep their merge: they have 4 points per beam, and a
     quarter of their reach (125) is under the guns' 136. Check the other
     forms' gun spacing before relying on that 136.
2. **The bolts mirrored** (larger). Each frame, the feed
   takes a short list of the additive sprites drawn: bolts, flames,
   fireballs, sparks. Each record holds where the sprite is, its size,
   its sprite and its colour. Primary rays ignore the list, and the
   raster still draws those sprites in view. Rays that mirror or refract
   test the list analytically, as the water's spray drops already are
   (`pt_spray.glsl`). Those rays are the water's reflection, what shows
   through it, and metal's glossy bounce. A hit adds the sprite, faced
   toward the ray and soft at its edge. A bolt then shows as a blue blob
   mirrored under itself, in both streams, not only as a column of
   glints. The glints stay; their strength may come down where the
   mirrored sprite shows, so a bolt is not counted twice.

Alternatives considered for step 2:

- **The sprites as real geometry** in the traced scene, as camera-facing
  quads in an acceleration structure. This is heavier: the quads are
  rebuilt every frame, and primary rays must be kept from seeing them.
  The analytic list does the same for reflections at a fraction of the
  cost.
- **Screen-space reflections of the raster's sprites.** Cheap, but they
  vanish at the screen's edges and for anything off screen, which is most
  of what water mirrors at a low angle.

## What changes where

- Step 1: `world_scene.rs`, the bolts' lights pushed one per bolt (a
  `SceneFeedBuilder` method that adds a glow without gathering). Add a
  test that two bolts side by side give two lights.
- Step 2:
  - `scene_feed.rs`: a list of emitter records (position, radius,
    colour, texture slot), filled from the blended sprites the capture
    already sees.
  - `vk_device/trace.rs`: upload the list as a buffer beside the spray's.
  - `pt_trace.comp`: `shade_ray` (used by reflection and refraction)
    tests the list before the scene, and `water_color` adds what the
    mirrored ray met.
  - Tuning: an on/off and a strength for mirrored effects.

## Risks, cost, verification

- Step 1: twin fire lights the water and ground twice as much as now.
  Check that it is not glaring at night (Tabuchi, and a dark world).
- Step 2 cost: a reflection ray tests every record. With the list capped
  near the eye (say 64), that is small next to the reflection ray's own
  trace. Measure it with the tracer's GPU timer during heavy firing.
- Step 2 look: additive blobs could look flat. Using the sprite's own
  texture, faced toward the ray, keeps the bolt's shape.
- Verify with before and after snapshots:
  - plasma fire, alternating and twin, over water: both streams
    mirrored;
  - laser fire;
  - an explosion by the water;
  - glossy metal near a fireball.

## Open questions

- Step 1 is a fix. Build it now, ahead of the other plans?
- Step 2: should mirrored effects match the sprites' full brightness, or
  be dimmer (water mirrors only part of the light at most angles)?
