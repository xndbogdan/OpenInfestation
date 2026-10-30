# Ground detail round the camera (Vulkan)

Plan of record, 2026-10-26. Puddles should be tessellated
inwards so they have volume (a shader alone looked fake), and the ground
could serve more things; the approach chosen is RDR2's, a
finely tessellated ground round the player whose heights the remaster
shapes. Vulkan only: OpenGL and the software renderer stay retail.

## Why the ground needs it

The traced ground is one static mesh: retail's 512 by 512 grid of
128-unit cells (`scene_terrain.rs`), built into one bottom-level structure
once and placed 3 by 3 round the eye for the world's wrap. Puddles change
as the rain soaks the ground, so carving them into that mesh would mean
rebuilding all of it; until now they only shaded a dip (normals tilted
into the hollow, a darker bank). The shaders also read the ground's grid by
position (`ground_height`, puddles, damp shores, fog, the headlamps'
smooth shadows), so a finer mesh sits beside that grid, never in place of
it.

## The design

1. **Tiles.** The coarse ground is cut into tiles of 4,096 units (32 by
   32 cells on High), each its own bottom level over its slice of the
   ground's index buffer, built once per terrain mesh. Each tile is placed
   at its copy across the wrap nearest the eye, so 256 instances cover the
   world once (the 3 by 3 copies placed every tile nine times). A tile's
   first triangle travels in its instance's shader-binding-table offset,
   which ray queries do not otherwise use.
2. **The detail patch.** The 3 by 3 tiles round the eye's tile (12,288
   units across; the eye is at least 4,096 from its edge) are left out of
   the frame, and a patch takes their place: each cell split 4 by 4
   (32-unit points), 295,000 triangles. It is built again when the eye
   moves to another tile, when the ground changes, or when the puddles'
   fill moves a step; its bottom level is built then, never per frame.
3. **Its heights** are retail's triangles exactly (each fine triangle lies
   on its cell's triangle, the cell's own diagonal kept), less what the
   remaster carves:
   - puddles: the hollows sink under their water, deepest at the middle,
     none at the shore, faded where the lake's shore takes the puddle
     (`puddle_shore`) and toward the patch's edge;
   - later: small relief from the ground's textures, tyre tracks and
     craters in a heightmap that follows the player.
   The carve is one Rust module (`scene_ground_detail`: `point_carve`,
   smoothed as `bed_carve`), the same the wheels' contact reads (below),
   its noise and hollows the shader's own.
4. **The water stays where it was.** A puddle's surface is the uncarved
   ground, so silhouettes and shadows above it do not change; a ray that
   meets the carved bed has crossed that surface, and the tracer shades the
   water there: the mirror and glints from the surface, the bed below it
   lit as it lies (real banks, real shadows into the hollow).
5. **Beyond the patch** the ground is retail's mesh and puddles keep their
   shading-only look, past the fog's middle on every world.

## The wheels (a gameplay setting)

"Wheels sink into puddles": with the remaster, single player only, the
player's wheels meet the carved bed instead of the water's surface. The
game decides it at a session's start and keeps it in the session's
settings, so a replay keeps it whatever renderer plays it back (Ultra
ground's rule). Off with OpenGL chosen, in network games and in the lab's
comparisons. Only the wheels' ground contact reads the carve: shots, the
AI and placed things keep retail's ground.

## Network games

Network games keep retail's weather: none (no rain or snow, dry ground),
whatever the world's tuning. The remaster's look otherwise stays.

## Steps

- [x] Tiles (1), 2026-10-26: 256 bottom levels, built 32 at a time when
  the ground changes (one at a time, the first frame took 51 ms of GPU;
  batched, 8.8 ms, as a frame's worst was before). Pelagia, dry, at the
  same render size: trace 2.10 ms against 2.19 ms with the 3 by 3 copies.
  The dry view looks as it did (`.tmp/puddle-pt/td-grid.jpg`).
- [x] The patch with the puddles carved (2 to 4), its water shaded,
  2026-10-26. Built on a worker thread in about 19 ms (50 ms on the
  render thread, threaded within too); the last patch stays drawn till the
  next takes over, 100 ms of the game's clock after it started, waited for
  then if not done (2026-10-29: taken whenever the worker finished, it
  landed a frame early or late from run to run, so Pelagia in the rain at a
  fixed step gave one of two frames, main's build too). In the rain the trace costs 2.36 ms against 2.28 ms
  without. Depth: 4 units (10 made the puddles read as pits, their banks a
  point or two wide, faceted and bright on the sunlit side), the carve
  smoothed 1-4-6-4-1 each way over the 32-unit points. 7 was tried
  2026-10-29, once the puddles took water's Fresnel and showed their bed:
  the same frames read alike at 4 and 7, but the wheels sank more (below),
  so 4 stays (`.tmp/visualfix/sheets/depth_4_vs_7.png`). With the
  puddles full no bed in worlds 1 and 9 sinks under the lake's plane but
  by the smoothing's hundredths of a unit at the waterline, which the lake
  covers (`no_puddle_bed_sinks_under_the_plane_in_the_rain_worlds`). Probes:
  `near-zoom.jpg` (10 units) and `near-zoom2.jpg` (4, smoothed),
  `p3-grid.jpg` (the lagoon, the puddles, infrared).
- [x] The wheels' setting, 2026-10-26: `ModernSettings::puddle_wheels`
  (on by default; in effect in a session whose game is the traced
  remaster's, `GameView::Remaster { traced: true }`: Vulkan single player
  started path traced, whatever renders a replay of it). The
  session sets the carve on the world's terrain each tick
  (`TerrainRuntime::set_puddle_carve`); the drive's four hull samples of
  the buggy and the truck read `wheel_height_at`, and the drawn wheels'
  segment probe reaches the same bed (`WheelGround`). The carve there is
  `bed_carve`: the patch's points on the world's lattice, smoothed alike,
  so it never depends on where the camera is; a test holds it to the
  drawn bed within a thousandth of a unit. In a puddle's middle the
  buggy rests 2 to 3 units lower and leans into it (`sink-cmp2.jpg`;
  2026-10-29, Pelagia's puddle at 31888,27800: the buggy 2, the truck 3;
  at depth 7 both 5). Driven through it from 32300,27790 (heading 270, Up
  from tick 20, the hull logged each tick) its hull drops a unit at most
  and, while it crosses (ticks 0 to 140), its pitch parts from a dry run's
  by 0.4 degrees, the truck's 0.5 (0.6 and 0.6 at depth 7); the buggy ends
  6 units off the dry run's line (20 at depth 7). Since 2026-10-29 the
  depth is 6 and the bed reaches it wherever the water is whole: resting
  the samples sink 4, 4, 4 and 3, the lean with one side in a puddle 2.0
  to 2.4 degrees (1.6 before); the spray at speed is `craft_spray.rs`. No
  launcher row yet: the settings file's `modern.puddle_wheels`.
- [x] Network games without weather, 2026-10-26: `WorldWeather::retail`
  when the session's network flag is set at the world's load.
- [ ] Later: texture relief, tracks, craters.
