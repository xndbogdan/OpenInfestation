# The modern raster: Vulkan without path tracing

Status: planned 2026-10-28; the modern sky and clouds (stage 2), the
reflections (stage 3: the water's, over retail's water strips, the rain's
puddles and wet ground, over retail's ground, and since 2026-10-29 the
metal's sky, in the models' own draws; what they hold is listed there),
the water (stage 4: since 2026-10-29 its splashes and lamps too) and
the grade (stage 5) are in, ahead of the plumbing (below). The stages
below are a
checklist: mark one `[x]` with its date, snapshots and GPU times. An
independent review against the code (`.tmp/fullpt/review.txt`) corrected
the first draft; its points are folded in.

## What was asked

- 2026-10-22, the RT plan's decisions: cards without ray tracing get plain
  Vulkan with the upgrades that need none; a modern raster mode (modern
  lighting, shadow maps, screen-space water reflections) was an optional
  last objective; 2026-10-23: left for later.
- 2026-10-28: backport visual improvements to the Vulkan non-PT branch,
  like screen-space reflections, maybe the fancy water, and clouds and
  sky.
- The same day, for every mode: a sharp image (no smeary blurry mess) and
  priority on good performance.

"Vulkan without path tracing" is two cases: a card that cannot trace rays,
and the Ray Tracing row off on one that can. Both get this mode. It comes
after [full path tracing](full-path-tracing.md).

## Where it stands (2026-10-28, `d76fa1132`)

From a read of the code (`.tmp/fullpt/maps/raster.md`, `sky.md`,
`water.md`):

- **Without ray tracing the device sees retail's screen-space fans** with
  only their blend, depth flags and texture (`DeviceDraw`): no surface
  class, no normal, no world position. Water and ground arrive through
  the same terrain callbacks; the painted sky is the first model drawn,
  before the depth clear.
- **What applies already**: SSAA, SMAA, FXAA and TAA, FSR, DLSS and XeSS,
  motion vectors for moving models, the thermal camera, the crystal
  sparks' tint, the underwater wobble.
- **What does not**: the modern sky and clouds, weather and lightning,
  the modern fog, the lighting presets' grade, any reflection, the modern
  water.
- **Why the sky cannot just run**: `Sky` (`vk_device/sky.rs`) belongs to
  the tracer; the frame block it reads is written only inside
  `trace_with` from the scene feed, which exists only while tracing; its
  scalar layout is enabled only on a device that traces. Its compute
  passes use no ray queries. The kept retail sky art (the Moon, star
  domes) is found by a ray query (`sky_objects`).
- **Why the water cannot just run**: about a third of `water_color` needs
  ray queries (the mirror and refraction rays, the sunk glows, the
  shadow tests); the rest (ripples, splashes, foam, shore, caustics,
  glints, lava) is a pure function of position, time and a few buffers.
  The water mesh is built only for the feed.
- **No hook after the world** runs every frame: `finish_raster` runs only
  with anti-aliasing or an upscaler on.
- **A feed in raster mode would delete retail's draws**: while the feed
  holds terrain or water the raster skips retail's ground and water
  strips (`traced_ground`), and `capture_primitive` drops what it takes
  (the sky's painting among it).

## Ground rules

- **Sharp**: the new passes add no temporal reuse of their own, with two
  stated exceptions shared with path tracing: the clouds (marched at a
  quarter of the display with a 6-frame history; moved to half size with
  2 to 3 frames, measured) and the mist. Screen-space reflections trace
  at full resolution for mirrors and fade to the frame's own far pixels
  and then the cube, never blurred by history.
- **Fast**: every new pass gets a GPU timestamp in a raster query pool
  with named stages, printed as a `Vulkan raster stages` line that
  `perf-scenes.ps1` captures. The budget, in ms at 4K and at 1080p (the
  stand-in for a card without ray tracing until one is measured), is set
  from the baseline before stage 1 lands.
- **Retail stays retail on OpenGL and software.** Here, as under path
  tracing, Vulkan is the remaster: the look may change, the game may not.
- **One frame block and one colour convention for both modes**: the
  raster's new passes read the tracer's `pt_frame.glsl` block, written by
  the same code, and work in linear light ending with the same `tonemap`
  and `to_srgb` before writing the 8-bit target, so `pt_water.glsl` means
  the same in both. Retail's own draws stay in retail's tones.

## Design

### Shared plumbing (stage 1)

- **Scalar layout** is enabled wherever `scalarBlockLayout` is supported
  (an optional Vulkan 1.2 feature); where it is not, the passes that read
  the frame block stay off and `ModernSupport` says so.
- **The frame block's writer** moves out of `trace_with` into one function
  that takes the world's inputs (camera, sun, fog, sky, weather, water,
  lights) and the geometry's offsets (zero in the raster), called by the
  tracer and the raster. The block, the host buffers and the pools are one
  per frame in flight (the full path tracing plan's stage 3), so writing
  it before the world does not race the last frame.
- **The world's inputs without the feed's side effects**: the raster's
  frame collects sun, fog, sky, weather, the ground heights, the water
  mesh, the splashes and the light records, but never sets `feed.terrain`
  or `feed.water` and never calls `capture_primitive`. Retail's ground and
  water strips are skipped only where a flag says the device draws them
  itself ("the raster draws the water"), set by the stage that ships the
  replacement.
- **Moved to the device's state**, next to `Sky`: the ground-height
  upload, the splash, record and cloud-shadow buffers, and the previous
  camera, tracked on every world frame.
- **A finish hook that always runs** after the world's opaque draws, at
  render size, before anti-aliasing and the upscaler, for surfaces only
  (the water, screen-space reflections). The sky's clouds and the weather
  run after the anti-aliasing or the upscaler, at display size, as path
  tracing draws them.
- **Two-phase replay**: the raster keeps blended batches back and draws
  them after the surfaces pass (as path tracing already does), so smoke
  and glows are not reflected or refracted. Deferred blended draws then
  test depth (LESS_OR_EQUAL, no write) whatever the Z Buffering setting:
  with it off they would show through hills. Overlays retail meant as
  overlays (the lamps' glare, the sky's art) are exempt.
- **Depth is always written** in this mode: with Z Buffering off, opaque
  draws write depth with an ALWAYS test (a new pipeline bit:
  `graphics_pipeline` ties writing to testing today), so the painter's
  order still decides what shows and every pass below has depth.
- **Mip support** in `State::image`, for the depth pyramid and the frame
  copy's mips.

### The sky and clouds (stage 2)

- The modern sky's gradient (sun, planet, stars) is drawn first, inside
  the raster's viewport with its jitter, where retail's painting would
  be, on the worlds whose recipe is modern.
- The painting's land band is drawn by the raster with its sky cut away
  (`resolve_band`, alpha-tested), and its kept art (the Moon, the star
  domes) by the raster as retail drew it, additive: no ray query needed.
  The backdrop's filter moves out of `scene_capture` into a raster-mode
  check this stage turns on.
- After the anti-aliasing or the upscaler, at display size, a pass lays
  the view's clouds over the sky and over ground beyond their distance,
  then the lightning, rain, snow, ash and embers (`pt_weather.glsl`), with
  a display-size depth made from the prepare pass's depth as
  `pt_depth.comp` does.
- The cube (with the cloud field, no sun disc) is made every frame for
  the reflections below.
- The rain and lightning sounds follow a new `draws_modern_sky()`;
  `puddle_wheels()`, a game setting, follows the session's game
  (`GameView::Remaster { traced: true }`), not the renderer.
- Cloud shadows on the ground need the sun's share of retail's shade,
  which retail does not keep apart. Held until decided, not faked.

**Built, 2026-10-28** (`vk_device/raster_sky.rs`, `shaders/vk_sky.comp`),
changed from the design above where the code showed better:

- The sky's inputs: `world_scene::world_light` (split out of `feed_world`)
  fills a world-only `SceneFeed` (`raster_sky_feed`: camera, sun, fog,
  weather, clocks, sky colour; no geometry, never installed as the scene
  feed), and the device writes the tracer's frame block from it
  (`frame_block`, `FrameShape::world_view`). `Sky` moved from the tracer
  to the device's state; the raster marches only the view's clouds (the
  cube and the cloud shadow map stay the tracer's). The scalar block
  layout is enabled wherever the device has it (`ModernSupport::modern_sky`).
- The painting: the frame starts black, and the sky's models draw as the
  tracer takes them (`scene_capture::raster_sky_draw`): the recipe's land
  with its painted sky cut away (`resolve_band`), laid on by its alpha;
  the art it keeps (the Moon, the star domes) as retail drew it; nothing
  else.
- **The gradient goes in after the painting and before the world**, not
  after the anti-aliasing: smoke, palm fronds and every other blended draw
  that writes no depth then blend over the sky as they did over the
  painting (laid on afterwards by depth, the first build washed the fronds
  out). At the world's size and jitter, `vk_sky.comp`'s first pass lays
  the gradient behind the land (a mask of how much sky shows through it)
  and the art, the view's clouds over all, a near strike's bolt, through
  the tonemap once.
- After the anti-aliasing or the upscaler, at the display's size, its
  second pass lays the clouds over the world beyond them, the lightning's
  flash over the world, the drops' splashes and the snow or rain over
  everything (no display-size depth copy: it reads the world's depth at
  the render size).
- The rain and lightning sounds follow `draws_modern_sky()` (Vulkan, on a
  device that draws the sky); `puddle_wheels()` follows the session's game.
- Since 2026-10-29 the second pass lays the clouds with the tracer's
  `clouds_before` (the near band to the followed craft, then the rest by
  their distance; the world faded past the fog's end takes them as the sky
  does), from the four-channel `away` image (`sky-per-world.md`, the
  clouds pass).
- Not the tracer's: no mist veils the sky (the raster keeps retail's fog),
  so a day sky shows clearer than the traced one; no cloud shadows (above).
- Cost at 4K (SMAA, the default settings, RTX 5080): GPU 1.6 to 2.3 ms a
  frame with the water's reflections, from 0.9 to 1.3 ms with them alone
  (Pelagia by day and night, Stendahl); the frame stays CPU-bound (9 to
  12 ms). Sync validation clean, under SMAA and FSR (FSR's own shaders
  warn about a format, as before).
- Since 2026-10-30 (`4645694f9`) the world's fade, clouds and lightning
  run at the render size before the blended draws, and a third pass (the
  over pass) lays the arcs and the rain or snow after the anti-aliasing.
  Cost at 4K (RTX 5080, GPU timestamps round each dispatch, mean over
  frames 120 to 220 of each frame's least in 3 runs, Pelagia):
  backdrop 0.21 ms, world pass 0.09 to 0.12 ms, over pass 0.07 ms with
  nothing to lay (a copy) and 0.72 to 0.73 ms in Pelagia's night rain.
  The over pass is now skipped without rain, snow or arcs
  (`FrameBlock::over`); the frame it leaves is the same.
- Option, not built: rain and arcs as geometry. A bigger engine draws the
  rain as instanced streaks near the eye and the arcs as thin emissive
  strips, in the blended draws, depth-tested and sorted, so they need no
  full-screen pass after the anti-aliasing (the fog and clouds already go
  in before the blended draws). It would cut the 0.7 ms of rain to the
  cost of its streaks' pixels. Not done now: it is a new instanced
  pipeline with its own sorting, the streaks' and arcs' look matched
  again to the tracer's (`pt_weather.glsl`, `pt_arcs.glsl`), and the
  frame is CPU-bound at 4K (9 to 14 ms) either way.

### Screen-space reflections (stage 3)

- **What reflects**: the water (stage 4), puddles and wet ground in the
  rain: flat surfaces. Metal by model name takes the sky cube at its
  roughness instead: retail's fans are flat facets, and a low-poly curved
  hull mirrors a different place in each. (Built without the G-buffer
  below: the metal is shaded in its own draws, below.)
- **A thin G-buffer**: a second colour attachment during the opaque world,
  written by opaque pipelines only: the surface class (ground, water,
  model) and the material per batch. Normals come from the depth in the
  fragment shader (`cross(dFdx(p), dFdy(p))` of the view position rebuilt
  from `gl_FragCoord` and the depth), exact for retail's flat fans, with
  no change to `DeviceVertex` (which the OpenGL device shares).
- **The trace**: a hierarchical march over a depth pyramid, at full
  resolution, with a thickness test, fading at the screen's edges. A
  sample at the far plane (the painted sky, the land band, which leave no
  depth) is a hit at infinity and takes the frame's own colour there; the
  cube only where the ray leaves the screen.

**Built, 2026-10-28: the water** (`vk_device/mirror.rs`,
`shaders/vk_mirror*.comp`), ahead of the plumbing, over retail's own water
strips:

- The terrain's water strips mark their commands (`PrimitiveCommand::water`);
  the device keeps their triangles and, after the world's last draw and
  before the anti-aliasing, draws them again over the world's depth (the
  motion pass's pipeline) into a mask holding each water pixel's own
  reciprocal depth. Nothing else changes in the frame.
- A min-max depth pyramid in a buffer: level 0 is the frame's depth less
  the water, its one-pixel holes closed and every surface brought 1%
  nearer, so rays never slip through the hairline gaps and steps between
  retail's terrain strips.
- Each water pixel's reflection off a flat surface crosses the pyramid
  (whole cells where every surface is in front of the ray, or well behind
  it as it goes on away), takes the frame's colour where it meets
  something within the thickness, and comes in by the tracer's Fresnel
  term (the world's `reflect` times the preset's reflections, at least
  0.02, and its gain). A ray that meets nothing takes the painted sky it
  points at, or the sky where it leaves the frame, faded at the sides.
  Lava mirrors nothing.
- A point on a cell's edge counts in the cell the ray goes into: a steep
  ray crossing an edge across its direction moves too little that way for
  its position to show it, and without this it stalled (the first build's
  dashes of unmirrored water along a strip seam).
- Cost at 4K (SMAA, the default settings, RTX 5080): the raster's GPU time
  0.66 to 0.75 ms before, 1.02 to 1.22 ms after, on Pelagia's lagoon and
  Stendahl; the frame stays CPU-bound (about 12 and 9 ms). Sync
  validation clean.
- With Z Buffering off the world's opaque draws write their depth
  untested (`PipelineKey::depth_always`, 2026-10-28: the plumbing's
  always-written depth), so the water mirrors there too; the painter's
  order still decides what shows, and with Z Buffering on the frame is
  pixel for pixel the same.
- Not yet: the sky above the frame for the water's rays leaving its top
  (they take the frame's colour where the march's last step passed the
  edge; the puddles below take the sky's cube, made only while they stand,
  and the water taking it too would make the cube every frame and change
  the dry frames: held until decided).

**Built, 2026-10-28: puddles and wet ground**, in the water's pass
(`vk_mirror.comp`'s `wet_ground`, `vk_ground.frag`), while the weather's
`wet_ground` is above zero (`world_weather.rs`; rain only):

- **Which pixels are ground**: the terrain's ground strips mark their
  commands (`PrimitiveCommand::ground`, the terrain host's quads and
  clipped tiles that are not water). While the rain wets a raster world the
  device keeps their corners and, beside the water's mask, draws them again
  over the world's depth (the primitives' own vertex shader, so the depth
  test passes on exactly the pixels they drew) into a ground image: the
  colour each corner's fog took and how much of the ground shows through
  it, retail's vertex fog for the textured near ring; for the far ring's
  colored quads, which mix their fog into their colours on the CPU in
  256ths (`d3d_terrain.rs`), the far colour they mixed and their specular
  alpha (`ground_corner`), marked below zero. Retail's draws are unchanged;
  dry, nothing is kept or drawn.
- **The ground's grid**: the tracer's own mesh (`scene_terrain::terrain_mesh`,
  cached, read from the game's terrain, no draw touched) comes with the
  raster's world light (`SceneFeed::raster_ground`, never the feed's
  `terrain`, which would take retail's ground's place) and is uploaded once a
  mesh, with a bit a cell for the diagonal its triangles share
  (`TerrainMesh::rising_diagonals`). It is made only where the device wets
  the ground (Vulkan's raster shading the water, the eye above it, not on
  lava: never for OpenGL, software or the tracer's frames), and from the
  load of a world the rain may wet (`WorldWeather::may_wet`: rain, not a
  network game's), built and uploaded then rather than on the frame the
  rain first wets the ground: there, in Tabuchi from a dry start, that
  frame took 24.6 ms of CPU against 9.7 around it (the mesh 10.3 ms, the
  upload 4.0), and takes 9.7 now, the work done at the load
  (`.tmp/puddles-fix/timing/`). While the rain wets the ground
  (`SceneFeed::raster_wet_ground`) the frame block takes its grid and the
  water's reach; dry, nothing in it changes. The ground's height, the
  puddles, the lake's shore and the craft's wake move out of
  `pt_trace.comp` into `pt_ground.glsl` and the sun's specular into
  `pt_sun.glsl`, unchanged (the trace's SPIR-V, ids canonicalized, holds
  the same instructions; the path pass's is the same).
- **The look**: each ground pixel's point comes from the depth and the
  camera; its normal is the tracer's (the triangle's corners' normals
  blended across it, read from the grid packed as normal and height a
  point); `puddle_at` places the puddles as the tracer does, never under
  the water's plane. The ground darkens by `RAIN_DARKEN` as it soaks, more
  under a puddle and at its bank, in retail's tones as tuned. **Wet means
  darker and smoother, not more reflective** (the rule of 2026-10-28). In full: a puddle takes water's Fresnel, F0 about 0.02 with
  Schlick's curve, so looking down it shows through to the dark wet bed
  and it mirrors toward the horizon; soil, sand and grass darken as they
  soak and stay fairly matte, soil with a rougher, weaker sun sheen; only
  hard surfaces turn glossy. The references: Sébastien
  Lagarde's write-ups of the rain in *Remember Me*, *Forza Horizon*,
  *GTA V* and *The Last of Us Part II*. Here the puddles mirror as water
  does, Schlick's Fresnel from water's F0 0.02 (`WATER_F0`; the lake's look
  by its shore), so a chase camera 20 degrees over the ground sees about
  14% mirrored and the dark wet bed through the rest; the soaked ground's
  sun sheen is a water film's, F0 0.02 and broad (`WET_GROUND_ROUGHNESS`
  0.5), less what pools; retail's ground says nothing of which surface is
  hard, so none turns glossy. The tracer follows the same rule since
  2026-10-29 (it had `0.5 + 0.45 F` and a sheen of roughness 0.2), the
  constants shared in `pt_ground.glsl`. The puddles are rippled by the rain's
  rings (`rained`), flecked (`rain_flecks`) and glint in the sun. Retail's
  fog is taken off the ground's colour and laid back as retail laid it;
  the reflection takes the frame's colour where the march meets something
  (from 2% nearer than the ground, twice the pyramid's bias, so a ray
  leaves its own ground), the frame's sky where it points at it, and above
  the frame the modern sky's cube (`Sky`'s, made only while puddles stand,
  laid through the tonemap as the raster's sky is; made only while puddles
  stand on ground the frame wets, not under the water or on lava), as the
  tracer's
  puddles take it; the painted sky's colour under a painted sky. Taking
  the frame's colour where the march's last coarse step passed the edge,
  as the water does, drew lines across a puddle and stretched the frame's
  top row over it (`.tmp/puddles-raster/shots/v1-raster/pel-puddle-cmp.png`).
  The sun's shadow on the sheen and the glints is a march toward it across
  the frame's depth, under the clouds' shadow, run only where lit and
  shadowed differ in the 8-bit frame. The bed under the water darkens as
  the bank does.
- **What lies over the wet ground stays over it, and the water still
  mirrors it**: once the ground's first strip is in, a blended or
  depth-less draw that the water's rule does not hold (that one: from the
  water's first strip, reaching above its plane) is held for the wet
  ground (`VkFrame::late_ground`). With water and wet ground both in view
  the pass runs twice (`MirrorStage`): the ground first, the water left as
  the frame holds it; then those draws, in order, over the wet ground; then
  the water, which mirrors them as it does dry, and mirrors the ground wet.
  Held until after the whole pass, as at first, the water lost them: in
  Pelagia's lagoon the fort, the boulders and the posts beyond it (drawn
  blended), shown by tinting what the ground holds
  (`.tmp/puddles-fix/shots/proof.lagoon.png`), and in Tabuchi's pond the
  water mirrored the far cliffs dry under their wet faces
  (`pond.horizon.png`). The second pass costs 0.3 to 0.5 ms at 4K (below).
  The puddles' reflected rays are kept above the horizon, as the water's
  (they had met the ground near by at grazing angles, dark specks).
- **Not the tracer's, held**: the carve round the eye (the detail patch's
  sunk bed; retail's ground is drawn as it lies, so the tracer's uncarved
  look beyond the patch is taken, the bank's slope turning the puddle's
  normal and the sheen; its dip's diffuse shading is held, since the
  raster's light is retail's baked shade, which cannot be lit again
  exactly); the damp band by the lake (not the rain's, and dry frames must
  stay as they are); the craft's wake (the raster's feed has no wheels);
  the lights' and lamps' glints and the
  lightning's on the puddles (no light records; the water has the lamps'
  since 2026-10-29, stage 4, but not the lights' or the lightning's);
  what is drawn blended or without its depth over the ground mirrored in
  them (smoke, sparks, and models drawn so, as Pelagia's fort and the
  craft: held draws are not in the frame when the ground is shaded); a
  ray leaving the frame through something other than the sky shows the
  wet bed, all or nothing, and the cube (128 a face) is softer than the
  frame's own sky in the same puddle; the wet models'
  gloss; the far ring's colored quads only darken (their cells, four times
  the grid's, lie off its surface on slopes: the grid's hollows showed as
  puddles on drawn cliffs); under the water no ground is wet.
- Checked (`.tmp/puddles-raster/shots/`): Tabuchi and Pelagia in the rain
  (`v7-raster/`) put the puddles where the traced frames have them
  (`base-traced/`; the traced ones still mirror the tracer's way), the
  far ring darkening evenly, the craft in Tabuchi's pond; the raster dry
  is the same to the pixel as `95b117356` in twelve scenes (Pelagia by day
  and night, Tabuchi dry and deep under the water, worlds 5, 9 and 22,
  Stendahl, Terra Nova, lava, Artoria, Foldrak by night: `v8-dry/`), and so
  are the traced frames (Pelagia dry, Stendahl, lava, Tabuchi deep, world
  22: `v8-traced/`). Traced rain differs from run to run on `95b117356`
  itself (Tabuchi's every run, `base-traced2/`; Pelagia's between two
  images, one of them the branch's to the pixel, `base-traced3/`); the
  trace's shaders are the same (its SPIR-V byte for byte with the height
  read in place of `GROUND_Y`; the moves' ids canonicalized).
  Sync validation is clean under SMAA, FSR (its own `rw_luma_history`
  warning, as before) and DLSS (`val-smaa/`, `val-fsr/`, `val-dlss/`).
- Cost at 4K (SMAA, the default settings, RTX 5080, GPU timestamps round
  each step, `.tmp/puddles-raster/perf/run7/`): the ground image 0.13 to
  0.16 ms; the water's pass 1.38 to 1.40 ms in the rain against 0.22 to
  0.43 dry; the sky's cube about 0.5 ms more (0.79 to 0.92 against 0.28 to
  0.33 for the sky's passes). The frame's GPU time, two interleaved runs
  each (`perf-4k-v7.txt`): Pelagia in the rain 3.58 to 3.66 ms against
  2.51 to 2.54, Tabuchi 4.24 to 4.29 against 2.94 to 2.97, Tabuchi dry the
  same (2.11 to 2.16 against 2.12 to 2.46). The frames stay CPU-bound;
  their CPU times (11 to 38 ms) moved with other work on the machine, not
  the build. Dry, nothing is added. With the ground wet before the water
  (2026-10-29, `.tmp/puddles-fix/perf.txt`, two interleaved runs): Pelagia
  in the rain 3.92 to 3.93 ms against 3.59 to 3.70 for the single pass
  and 2.71 to 2.76 on `8a950659d`, Tabuchi 4.35 to 4.62 against 3.96 to
  4.12 and 2.80 to 2.86, Tabuchi dry 1.96 to 2.00 against 1.96 to 2.00 and
  1.99 to 2.01.
- Checked again, 2026-10-29 (`.tmp/puddles-fix/`): dry raster frames the
  same to the pixel as `8a950659d` in thirteen scenes (`shots/id-dry/`, the
  twelve above and Tabuchi's pond dry), traced in six (`shots/id-traced/`),
  OpenGL and software in five each, rain among them (`shots/id-opengl/`,
  `shots/id-software/`). A drive through the rain over Pelagia's and
  Tabuchi's puddles under TAA, FSR and DLSS at 4K, four consecutive frames
  going straight and four turning (`motion/`), shows the puddles'
  reflections and the rain's flecks holding still, no shimmer or trails,
  with synchronization validation on (the layer's log, `motion/val-*.txt`,
  lists "Synchronization" among its enables) and clean but for FSR's own
  `rw_luma_history` format warning.

**Built, 2026-10-29: metal by model name** (`raster_metal.rs`,
`vk_device/metal.rs`, `shaders/vk_metal.*`), shaded in the world's own
draws rather than in a pass after them:

- **Which pixels are metal, exactly**: a model primitive's command carries
  its face as the tracer takes it (`PrimitiveCommand::metal`, from
  `scene_capture::retail_face`): its material by the model's name
  (`materials.names`) or, for a sprite with its own, that one, bare, with
  its split (`materials.sprites`: the ACAM's hubs metal, its tyres rubber,
  the truck's hub the side wall above half its height); and each corner
  where the model drew it, its normal as retail lit it, its colour (white
  textured, the flat face's own, either by the fan's factors) and its V in
  its sprite. Only a face with metal, or a sprite's bare surface, carries
  one; a model's name is looked up once (`RasterMetals`). The device draws
  such a face's solid fans (opaque, or keyed: retail blends its sprites by
  their texels' alpha at full; never half-transparent ones) with
  pipelines of their own, where retail's colour is laid as the others lay
  it, so no G-buffer or pass after the world is needed: what is drawn over
  the metal later (smoke, glows) lies over the finished metal, the water
  and the puddles mirror it, and fans held for the water's or the wet
  ground's pass are held with their metal. Each fan corner takes the
  face's values where it lies on the face (`MetalFace::at`: where a corner
  of the face lies, its own; a corner the clipper made, blended across the
  tracer's triangles), its normal turned to the side the eye sees as the
  tracer's `orient` turns it.
- **The look** (`vk_metal.glsl`): the tracer's path pass mirrors a metal's
  surroundings as often as it is metallic, along GGX's visible normals
  (`glossy_direction`, moved into `pt_glossy.glsl`, shared), its colour
  times what each ray meets, the rest of its light diffuse. The raster
  does the same with the sky: 16 fixed directions of the lobe (the same at
  every pixel and in every frame: no temporal reuse, nothing shimmers),
  each weighted by Smith's masking as the tracer's, each reading the
  modern sky's cube through the tonemap (as the raster's sky and the
  puddles show it; under a painted sky its colour) at the level its share
  of the lobe covers (filtered importance sampling). The pixel's metallic
  share of the directions that reach the sky takes its colour times the
  sky's light in place of retail's light; by the preset's reflections, as
  the tracer's (`reflect_gain`). Directions below the horizon meet the
  ground and what stands on it in the tracer, which the raster cannot see:
  there retail's light stays (held, below).
- **The rain's wet coat**, the tracer's (`wet_model`, moved with
  `RAIN_DARKEN` into `pt_wet.glsl`, shared): a wet metal mirrors more and
  smoother, the more where it faces up; bare rubber darkens as the soaked
  ground does and turns a faint mirror of its own colour. Dry, the tyres
  are drawn as before.
- **The cube**: `Sky`'s, made the first time metal is drawn in a frame
  (`Sky::record_cube`, between the world's draws), unless the puddles made
  it already; a frame with no metal in view makes none.
- **Shared code unchanged**: the path pass's, emitters', denoiser's and
  the water's pass's SPIR-V are the same byte for byte; the trace's holds
  the same instructions (ids canonicalized: the frame block's wet read
  before the normal's height rather than after, two phis swapped;
  `.tmp/metal-raster/spv/`).
- **Held, not faked**:
  - what the lobe meets below the horizon (the ground, the hills, other
    models), which the tracer's metal mirrors: retail's light stays there.
    A march across the frame, as the puddles', would mirror a different
    place in each flat facet of a low-poly hull (the plan's reason for the
    cube);
  - the sun's highlight on metal (the tracer's `sun_specular`, on every
    model): it needs the sun's shadow at the pixel, which a draw in the
    world cannot test (the puddles march across the finished frame);
    unshadowed it would glint in the shade;
  - the sky's kept art (the Moon, the star domes) in the mirror: the tracer
    finds it with a ray query;
  - the painted models' clear coat in the rain: non-metal models stay as
    retail drew them;
  - edges (opcode 07, the axles and legs retail widens on the screen),
    billboards, and the tracer's rounder parts (`round_parts`: its wheels
    are round where retail's are octagons);
  - metal drawn half-transparent or without its depth (with Z Buffering off
    retail's keyed sprites draw untested, blended, and the plumbing's
    always-written depth takes only its opaque draws), metal under the
    water's plane, and the view from under the water: retail's colour.
- Checked (`.tmp/metal-raster/`): the ACAM, Stendahl's buildings, Terra Nova's
  base (its towers, dish, silos and turrets) and the craft turning (its
  hubs), dry and in the rain, beside the traced frames
  (`shots/v3/sheets/`: main's raster, the branch's, the traced frame, the
  change): the metal greys toward the tracer's, takes the sky's tint (Terra
  Nova's green, Pelagia's grey-orange), darker where its lobe sees a dark
  sky; in the rain it mirrors more and the tyres darken. With the
  anti-aliasing off, every pixel that changed lies on a metal fan or its
  mirror in the water (a build painting the metal fans magenta,
  `shots/noaa/inmask.txt`: none outside, four scenes); under SMAA a few
  dozen more within two pixels of the metal's edges (`shots/v3/inmask.txt`).
  The traced frames are the same to the pixel as main's in six scenes,
  Pelagia's rain among them (`shots/v3/traced-identity.txt`), OpenGL's and
  software's in five each (`shots/retail/identity.txt`). Five runs of
  Pelagia in the rain play the same game as each other and as main's (their
  logs, `shots/determinism*/`); their images differ by one level in a few
  hundred pixels of the sky and the rain, as main's own five do. Sync
  validation is clean under SMAA, TAA, FSR (its own `rw_luma_history`
  warning) and DLSS, Terra Nova dry and Pelagia in the rain (`val/`, the
  layer's logs listing "Synchronization"). A GPU test draws a metal face
  split by its sprite (`vulkan_raster_metal_mirrors_the_sky`).
- Cost at 4K (SMAA, the default settings, RTX 5080; three interleaved runs
  of main's build and the branch's, medians,
  `.tmp/metal-raster/perf/final-native4k/`): GPU
  a frame Pelagia dry 2.69 ms against 2.52, Pelagia in the rain 3.80
  against 3.75 (the puddles' cube already made), Stendahl 2.16 against
  1.76, Terra Nova 1.98 against 1.65, Artoria 2.05 against 1.71: most of
  it the sky's cube made for the metal (the metal's own shading about
  0.05 ms in Pelagia, where the rain's puddles made the cube already).
  Under DLSS Quality (`perf/final-dlssq4k/`): 3.11 against 2.99,
  3.66 against 3.65, 2.86 against 2.59, 2.72 against 2.51. The CPU's time
  rises 0.2 to 0.6 ms a frame (0.5 to 0.8 under DLSS; Terra Nova's base
  the most): the faces' corners in the model programs about 0.15 ms, the
  metal fans in the raster's replay about 0.3, and about 0.1 that shows
  with the metal switched off too, not traced to any one change (a CPU
  profile with each part off, `perf/exp*/`). The frames stay CPU-bound,
  8.9 to 13.2 ms.

### The water (stage 4)

- **The mesh**: the tracer's water mesh (`scene_water::add_water`, the
  game's own wave heights) drawn in world space by a new pipeline, in
  place of retail's strips, with the raster's depth convention.
- **The look**: the pure half of `water_color` moves from `pt_trace.comp`
  into `pt_water.glsl`, shared by both modes: ripples, splashes and
  eruptions, rain rings, the shore and foam, caustics, glints, the lamps
  in the water, lava. The water's texture comes as its mean colour, not
  the bindless array.
- **Refraction**: a copy of the frame's colour and depth before the water;
  the bed's colour from the copy, offset by the ripples and checked
  against depth, its distance through the water from depth, absorbed as
  the tracer does (`water_clear`, the world's clarity).
- **Reflection**: screen-space (stage 3).
- **Shadows**: the sun's glint and the caustics are gated by a march
  toward the sun over the ground's heights (`ground_through`, no ray
  queries), times the cloud shadow: the hills' shadows fall on the water.
- **What it cannot do**: models' shadows on the water, water seen in its
  own reflection, things off-screen in the mirror (the cube stands in).

**Built so far, 2026-10-28: the surface**, on retail's own water strips
and in the mirror's pass (`vk_mirror.comp`), the frame block now written
for every raster world (its light; the sky only where modern):

- The tracer's swell and ripples (`pt_water.glsl`, moved out of
  `pt_trace.comp` unchanged: traced frames are bit for bit the same) bend
  the mirrored view and its Fresnel term, as the tracer's water.
- The shore's fade from the water's depth: the water writes no depth, so
  the frame's depth behind a water pixel is its bed; the depth is the
  water's height over the bed along the view (the tracer's takes the
  ground's heights, which the raster has not). Ripples, reflection and
  glint fade out together at the edge, as the tracer's do.
- The sun's glint on the ripples, as the tracer's (`pow(n.h, 600)`),
  where a march toward the sun across the frame's depth (the mirror's own)
  meets nothing, under the clouds' shadow map (`cloud_light`, moved to
  `pt_sky.glsl`; `Sky` now makes the map for the raster's clouds too).
- Cost: within the noise of the mirror's (GPU 2.3 ms a frame at 4K on
  Pelagia with the sky, 1.8 on Stendahl). Sync validation clean under SMAA
  and DLSS. The mirror now needs the frame block, so the scalar block
  layout, as the sky does.

**Built, 2026-10-28: the body**, still on retail's strips rather than the
tracer's mesh (the strips are the game's own wave heights, and the mask
already has their depth):

- **Retail's strips are kept, not drawn** (`VkFrame::fan`, while the
  device shades the water): the frame keeps the bed under them. The
  mirror's pass draws them again, as retail laid them (texture, light and
  vertex fog), into a surface image over the world's depth: the water's
  own colour. The mask takes each corner's fog visibility beside its
  depth, and the pass the fog's colour, so what lies on the surface fogs
  as retail fogged the strips.
- **The bed through the water** (`vk_mirror.comp`): the view bent by the
  ripples (index 1.333) finds the bed one step along, the frame's colour
  there where the mask still has water and the surface found lies past
  the water's own (a model standing in the water is not under it), else
  the pixel's own bed; it closes over the deep into the water's colour by
  the tracer's `water_clear` along the bent path. No damp darkening: the
  raster's bank has no damp band; in the rain the bed darkens as the
  soaked bank does (stage 3's wet ground, 2026-10-28).
- **Caustics** on the sunlit bed (`caustic`, the tracer's), where a march
  toward the sun from the beam's way in at the surface meets nothing,
  under the clouds' shadow; on the bed's own light, not the fog over it.
- **The shore's foam line** (`lap_line` with `foam_cover`, the tracer's;
  the shore's depth from the bed, wander and all), lit as the tracer lights
  it (the sun where the frame shows nothing in its way, the sky's light),
  and **the rain's rings and flecks** (`rained`, `rain_flecks`).
- **What lies over the water stays over it**: once the world's first water
  strip is in, a blended or depth-less draw reaching above the water's
  plane (smoke, sparks, splashes) is held and drawn after the water's pass
  in its order, depth-tested against the world; what lies wholly under the
  plane stays in the frame as the bed. The plane comes with the frame
  block (`SceneFeed::water_level`, which the raster's block now carries).
  Such draws keep the world's depth test rather than retail's painter's
  order, as they do over a traced world; held draws are not mirrored.
- Cost: about 0.05 ms of GPU a frame at 4K on Pelagia (2.45 to 2.79 ms
  against 2.42 to 2.74, two runs each, dry and raining); the frames stay
  CPU-bound at about 12 ms.
- Not done: splashes, eruptions and the lamps in the water (the raster
  had no splash list or lamps): built 2026-10-29, below.

**Built, 2026-10-29: the splashes and the lamps**, from the tracer's own
sources into the raster's frame block and a splash buffer beside it:

- **The splashes** (`pt_splash.glsl`: the tracer's `splashed`, `erupted`,
  `churned` and `under_glow` and the splash buffer's layout, moved out of
  `pt_trace.comp` unchanged; the trace pass's SPIR-V is the same
  instruction for instruction, ids aside). The world draw gives the
  raster's feed retail's splash objects and the blasts as it gives the
  tracer's (`world_scene::feed_splashes`), and the frame block's writing
  (`raster_sky.rs`) makes the splash buffer as the tracer's does
  (`trace::water_splashes`: the eruptions, which the device keeps from
  frame to frame, and where their water falls back, then retail's
  splashes, the nearest first, 128 at most). The water's pass rings and
  tilts the surface by them, foams it where they fell and where an
  eruption's dome swells, lightens the water a blast churned (retail's
  colour under its fog, lightened as the tracer lightens its body) and
  screens in the blasts' glow from under the surface; seen from below the
  surface takes the rings too.
- **The lamps** (`lamps_in_water` moved to `pt_water.glsl`). The light
  pass keeps the vehicles' lamps for the raster too, its grid keeping
  retail's pools (`Flag0LightPassRequest::lamps_light`: only the tracer's
  lamps light in the pools' place; retail's water strips take no pool,
  `terrain_draw/water.rs`), and once it has them the frame block is
  written again with them and the water's hue
  (`SoftwareScene::water_colour`, the tracer's), for the water's pass
  alone (`State::raster_lamps`). As the tracer's water: each lamp's glint
  on the ripples (its lobe at the tuning's gloss), its light entering the
  water and scattering back in the water's colour, more over deeper
  water, the foam lit by them, and the lamps under the surface glowing in
  the water along the bent view. A lamp lights a point of the water where
  a march toward it across the frame's depth meets nothing short of its
  craft (256 units: the craft never stands in its own beam, but from
  behind it hides where the beam leaves it), the tracer's shadow ray's
  stand-in. Where the lamps light the water its colour takes the tracer's
  tonemap, so their light past full rolls off in the water's hue as the
  tracer's composite lays it; clipped, it was a white blot
  (`sheets/pelagia-night-drive.f0130.tonemap.png`).
- Checked against the traced frames (`.tmp/raster-water/shots/sheets/`,
  the branch head's raster, this one's, traced): the craft driving into
  Pelagia's water firing its gun and grenades (`pelagia-drive`, the shots'
  and wheels' rings, the eruption's ring and churned patch at frame 240),
  the headlamps at night over the water (`pelagia-night-drive`, Stendahl's
  beach `w3-beach-night`, Tabuchi's pond `tabuchi-lamps`, which has no
  night: its lamps by day), the hovercraft's lamps on the water
  (`w3-hover-night`) and the view from under Tabuchi's deep water
  (`tabuchi-deep`: the rings open Snell's window where they tilt the
  surface). With no splash and no lamp on the water the frames are the
  branch head's to the pixel: the raster in 11 scenes (Pelagia dry, wet
  and by night, Terra Nova, Stendahl, Artoria, lava, world 22, Tabuchi dry
  and wet), the traced frames, OpenGL and software in 4 each
  (`.tmp/raster-water/identity/identity.txt`); the twelfth, Tabuchi's
  deep start, has splashes over the craft and changes only there. The
  game is the branch head's: the light pass still lays retail's pools
  in the raster, and the rest is drawing (not rerun). Sync validation was
  not run for this stage. A GPU test (`vulkan_raster_water_takes_splashes_and_lamps`)
  holds the lamp's light, the splash's rings and that nothing beside the
  water changes.
- Cost (the water's pass alone, 4K, its GPU time, median of 5 runs,
  `.tmp/raster-water/perf/exp5/`): about 0.15 ms while the craft's first
  splashes and the lamps reach the water (0.85 to 0.99 ms, the lamps 0.71
  to 0.87 ms), but 1.6 to 5.3 ms with the craft driving and firing in the
  water (128 splashes, culled per workgroup) and 1.6 to 3.2 ms with the
  lamps over the water at night (the night drive's 128 splashes, not its
  lamps). Cut down the same day: each workgroup makes its kept rings once
  into shared memory (`cull_splashes`: radius, width, strength and foam's
  radius, packed in the buffer's order) and every pixel walks that list
  (`pt_splash.glsl`'s `FOR_EACH_RING`), rather than finding each in a
  bit mask and reading and working it out of the buffer again. The frames
  are the head's to within 1 of 255 (`.tmp/splash-cost/`, frame 180 of
  both; `sheet-drive.f0180.png`, the head above this). Mean over passes 120 to 220 of each pass's least in 3 runs:
  driving and firing 4.6 to 2.7 ms, the night drive 3.8 to 2.6 ms (about
  +1.1 and +1.0 ms over the water without splashes). Skipping a ring's
  inside past three widths (and its hole holding a workgroup clear) saved
  another 0.1 ms but flips the grain of sunlit shallows in whole patches
  (79,000 pixels by more than 8 in the night frame): not taken.
  Remeasured 2026-10-30 (GPU timestamps round the water's dispatch, same
  kit): driving and firing 2.65 to 2.67 ms, the night drive 2.42 to 2.44.
  Also tried and dropped: each ring's reach squared in its position's
  spare w, so a pixel past a ring reads one vec4 of shared memory before
  making the ring (the look unchanged); it was slower, 2.83 and 2.52 ms.
  Bounds culling is already per workgroup: nothing safe left, kept as is.
- Before/after (the branch head's raster, this one's, traced; the
  headlamps at night driving into Pelagia's water):
  `.tmp/raster-water/final-shot/sheet.f0130.png`, `sheet.f0190.png`.
- Held: the eruptions' plume and drops (the tracer's spray, a volume its
  rays march, `pt_spray.glsl`; the raster shows retail's explosion
  sprites there); the lamps' shadows from what the frame does not show
  (behind what it shows, off it) and from anything within the craft's 256
  units of a lamp, which the tracer's ray finds; the lamps on the bed seen
  through the water (the bed keeps retail's pool, as the raster's ground
  does), in the murk with the eye under the water (the tracer gathers them
  in its medium; the raster's murk is the old fog's) and on the puddles;
  retail's own lights glinting on the ripples (the raster has no light
  records).

**Built, 2026-10-28: the view from under the water** (in
raster mode the view above water was missing). With Camera Style's Remastered eye
under the water's live surface, Vulkan's raster, where it shades the water
(the scalar block layout; not lava), no longer takes the world draw's own
underwater look (its fog shortened to `0x1000`, the dimmed colours,
V2000's wobble, that fog for a sky, the surface's strips left out;
`world_scene::UnderwaterLook`). The world draws as the same eye above
the water has it, its fade, sky and strips and all, as the tracer's does.
That fade is game state (the objects' pass radius, the terrain's
footprints, the particles' flags), so the game's draw never takes an
underwater look: the remaster's game (Vulkan single player) keeps the
header's fade under the water on every device, as the eye above it gets,
nearer retail, whose camera never goes under, and retail's game draws
retail's view
([the game's draw and the shown draw](../re/PRESENTATION_POLICY.md#the-games-draw-and-the-shown-draw)).
Software, OpenGL, lava and devices without the scalar layout keep the old
look on screen.

- **The water's pass runs on every pixel** (`vk_mirror.comp`): what lies
  nearer than the surface shows through the murk over its distance, and a
  pixel with nothing drawn is the murk. The surface is seen from below,
  rippled and rained on as from above (its normal the one the eye above
  sees, turned to face the eye): inside Snell's window the view bends out
  through it (index 1.333) and takes what lies above across the frame's
  depth, or else the sky along it (the frame's own where it points, else
  the modern sky's gradient, else the painted sky's colour); outside the
  window it reflects whole, and within it by the tracer's Fresnel term
  from below, the reflected ray finding the bed and what lies under the
  water through the murk along its way, or the murk alone. Under the
  water the depth's pyramid (`vk_mirror_depth.comp`, its `under`) is the
  frame's whole depth: the water's pixels hold the depth of what lies
  above the surface (its strips are not drawn), where above the water
  they are holes so that a reflection never meets its own water. Which
  side of the surface a march met is the depth there, not the ray's own
  point.
- **The murk is the old fog's**: the water's colour at 0.35, whole from
  256 to 4,096 units, but along the light's way through the water rather
  than the view's depth. The design took the tracer's `water_clear` over
  the world's clarity; Tabuchi's 260 hides half of the bed 330 units off,
  so the craft 600 off was nine tenths murk and the surface over the
  user's dives (850 to 1,100 units under it) all of it: the complaint
  stood (`.tmp/uwraster/v1_pair.png` against `v2_pair.png`). Kept so on
  review: it is the port's own underwater look, the fog software and
  OpenGL draw there. The tracer's water round an eye under it
  (2026-10-29) hides half where this murk
  does (`world_scene::MURK_REACH`), so the two read as the same water.
- **The world draw and the device agree on the look**: the world draw
  asks for the murk only where the device shades the water
  (`shades_water`: Vulkan's raster with the scalar block layout, on a
  world whose water is not lava), a capability rather than the frame's
  outcome, since the look decides the game's fade. The frame block the
  pass takes goes unwritten only on a device error, which stops the
  renderer (logged once, "Vulkan renderer stopped"), so the view from
  above never stands in for the murk; a debug assertion in
  `begin_raster_world` holds it.
- **What lies under the water waits for the murk**: with the eye under
  the water, a blended or depth-less draw with a corner under the plane is
  held whenever it comes and drawn after the pass, each corner seen
  through the murk over its distance (what adds light or darkens fades
  out; a textured draw takes the murk as fog over retail's own, the two
  laid as one, `Murk::through`). Above the plane, a draw stays in the
  frame and is seen through the window (`holds_for_water`).
- **An eye between the plane and a wave's crest** (the camera keeps 64
  units under the live surface over its own spot, and deep water's waves
  can stand higher over the plane): the world draw tests the live surface
  at the eye, so the view is the one from below throughout. The pass needs
  no plane (each water pixel's own point, its normal turned to face the
  eye: a trough below the eye shows as from below, though the eye sees
  its top); only the held draws split at the flat plane, so a blended
  draw between the plane and the eye stays in the frame and takes the
  pass's murk over the depth behind it rather than its own distance. Left
  so: the live surface changes across a draw, and no one plane is exact.
- **No clouds, lightning, splashes or fall over the world** with the eye
  under the water (`finish_raster_sky`).
- Checked: the deep-water start (Tabuchi `[51978, 37383]`, frames 60 to
  380, `.tmp/uwraster/seq-new/` against the traced `seq-traced/`) shows
  the window's sky and its rippled edge where the traced frame has them;
  the replay `run-20261028-181209` plays back as recorded (its
  status lines' RNG and eye) through frame 6600, past its first dive at
  about 4,920; at 7200 its RNG differs (`41AA40E8` against the
  recording's `72539C36`, which the old look reproduces), because under
  the water the world now keeps its above-water fade, which the game
  reads, where the old look shortened it: by design (retail's camera
  never goes under, so its game keeps that fade), but a Vulkan raster
  recording that dives replays as recorded only up to the dive. Its last
  eye is still the recorded one (`.tmp/uwraster/sl-new/`, snapshots in
  `rp-new/`). Since the game's draw and the view shown parted
  (2026-10-29), no underwater look reaches any renderer's game, and that
  recording, made before the session's game was recorded, replays on
  OpenGL as retail's game; the
  raster above the water is the same to the pixel (Pelagia dry and wet,
  world 22, Tabuchi's pond and rain); sync validation is clean under
  water. Cost at 4K on the deep start: GPU 3.1 to 3.2 ms a frame against
  1.2 for the old look (whose short fog drew far less), as above the water
  there (3.4); CPU 5.8 ms against 1.7, the world's whole draw distance.
- Checked again on review (the pyramid's depth under the water, the side
  tests at the surface met, the held corners written straight into their
  batch): the window at frame 220 of the deep start shows the bank on its
  right where the traced frame has it (`.tmp/uwfix/f0220_top3.png`:
  before, after, traced), and more of it than the trace, whose window's
  left is sky: the march meets what the frame shows straight through the
  surface at other pixels, the screen's limit. The replay shows the
  shore at the window's rim after the dive (`.tmp/uwfix/f7720_top2.png`)
  and its status lines are the branch's before (`.tmp/uwfix/rp-new/`); the
  five scenes above the water are identical to the pixel; sync validation
  is clean under the water (`.tmp/uwfix/val-new/`); GPU at 4K on the deep
  start 3.12 to 3.13 ms against 3.11 to 3.12 before, two interleaved runs
  each.
- Not done: the caustics and the sun's shafts under the water, the sun's
  glint seen from below, and what the mirror cannot see off the frame (the
  bed beside the view).
- Held: splash crowns straddling the plane are held with what reaches
  under it and drawn after the pass, white and unrefracted, over the
  window (`rp-new/f4960.png`, `f7240.png`), as in the old look: retail
  laid them after its water, and the pass bends only what the frame
  shows, not the draws held after it.

**The weather stops at the water** (2026-10-28: it was
raining under the water): the falling rain and snow end where the view
passes the water's plane (2 units under it), none shows with the eye
under the water, the drops' splashes never land on the water or its bed,
and under the water the hull's patter and the wind fall silent (the rain
on the surface and the thunder still carry, muffled). Both renderers.

### The grade (stage 5)

The Lighting row and its presets apply here too (today it locks without
path tracing): exposure, saturation and curve after the world.

**Built, 2026-10-28**: the Lighting row (and the launcher's) moves under
Vulkan traced or not; the raster's world takes its preset's grade after
the sky's pass and before the HUD (`finish_raster_world`, the grade the
traced world takes). The preset's reflections share already set the
water's mirror.

## Order of work

1. [-] Shared plumbing, with the raster's stage timings and its budget:
   skipped 2026-10-29 (internal only, nothing on screen).
2. [x] The sky and clouds, 2026-10-28 (above).
3. [x] Screen-space reflections: the water's, and the rain's puddles and
   wet ground, 2026-10-28; metal by model name taking the sky cube at its
   roughness, 2026-10-29 (`.tmp/metal-raster/shots/v3/sheets/`, GPU 0.05
   to 0.4 ms at 4K); what they hold is listed above.
4. [x] The water: its surface (ripples, shore, glint) and body (the bed
   through it, caustics, foam, rain), and the view from below, 2026-10-28;
   the splashes, eruptions and lamps in it, 2026-10-29
   (`.tmp/raster-water/shots/sheets/`, the water's pass 0.15 ms, up to
   3.7 ms more in heavy splashing at 4K); what it holds is
   listed above.
5. [x] The grade, 2026-10-28 (above).

## Baseline

Measured with the Ray Tracing row off at the default settings and a DLSS
Quality copy, `.tmp/fullpt/perf/`, when stage 1 starts. Before the water's
reflections (`d76fa1132` plus the full path tracing work, SMAA, 4K):
Pelagia dry 12.0 ms a frame, GPU 0.66 to 0.69 ms; Stendahl 8.7 to 9.0 ms,
GPU 0.74 to 0.75 ms.
