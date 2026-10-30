# Full path tracing: plan

Status: planned 2026-10-28; stage 1 built the same day. Each stage below
is a checklist: mark it `[x]` with its date, its snapshots and its GPU
times when it is done. An independent review of this plan against the
code (two reviewers, a graphics lens and a feasibility lens,
`.tmp/fullpt/review.txt`) corrected its first draft; its points are
folded in below.

## What was asked

- 2026-10-22: path tracing should replace retail's lights and path trace
  everything, as a full path tracer (the RT plan's
  [Full path tracing](../INFESTATION-RT-IMPLEMENTATION.md#full-path-tracing),
  whose five stages were about the lights only).
- 2026-10-28, after asking whether what existed is a full implementation
  (it is not, see below): document, then properly implement the path
  tracing; then backport visual improvements to the Vulkan non-PT branch,
  such as screen-space reflections, perhaps the fancy water, and
  clouds/sky (the backport is [its own plan](modern-raster.md)).
- The same day: the image should be sharp and performance a priority;
  sharp means no smeary blurry mess (as in many modern games).

## Where the tracer stands (2026-10-28, `d76fa1132`)

A read of the code by seven agents (70 findings, an adversarial pass
confirmed 59 and corrected the wording of the rest; kept in
`.tmp/fullpt/audit.json`) found a real ray tracer with retail's lighting
under it. In `crates/inf-game/shaders/pt_trace.comp`:

**Traced, and sound:**
- The scene: hardware ray queries over bottom levels rebuilt every frame
  from every face retail drew (back faces and off-screen parts included)
  and the whole terrain.
- The sun's soft shadow: one ray a pixel to a random point of a 2.25°
  disc, leaving from Hanika's terminator offset (`terminator_lift`).
- One bounce a pixel: cosine-weighted (Lambert, its pdf cancelling) or,
  as often as a surface is metallic, GGX (`glossy_direction`).
- Retail's light records shadow-traced at primary hits; one light picked
  by its share at the bounce's hit (an exact pick over every light in
  reach, by its unshadowed light).
- The water's mirror ray and its Snell refraction ray (with total
  internal reflection).
- Alpha-tested cutouts on every ray (`PROCEED_THROUGH_CUTOUTS`), textures
  filtered by ray differentials at primary hits.
- A froxel mist with a traced sun shadow per cell (`pt_fog_inject.comp`),
  clouds raymarched with Beer-Lambert and Hillaire's octaves
  (`pt_sky.glsl`).
- An albedo-demodulated temporal plus a-trous denoiser, and DLSS Ray
  Reconstruction as an option.

**Why it is not full path tracing** (each with its code anchor):
1. **One bounce.** `bounce_light` calls `shade_ray` once; the light past
   it is the constant `frame.ambient` (retail's tint over 4), added at the
   bounce's hit and returned by every diffuse ray that misses. No loop,
   no radiance cache.
2. **Retail's light is the base.** A model's light is retail's per-corner
   Gouraud shade (`surface.shade`, from `004901B0` `shade_world`), with
   the traced sun's shadow cut out of its sun share and traced lights
   swapped in for its light grid (`traced_light`). The ground's albedo is
   its baked shade (`shade4`, 0..15) over N·L plus a quarter
   (`scene_terrain.rs`), up to 4.
3. **The sky lights nothing but mirrors.** Diffuse light from the sky is
   the constant ambient; only glossy and water rays read the sky cube.
4. **Light was capped at 1** (retail's `normalized_shade`): `capped()` at
   every secondary hit, the resolve's cap, 8-bit albedo and sky storage,
   the composite's `clamp(color, 0, 1)`. Colours are display values used
   as linear ones. (Stage 1 removed the caps.)
5. **Emitters are not lights.** The lights are a list (the sun, retail's
   records, the tuned glows, the headlamps). A retail-unlit face that a
   bounce ray hits gives at most its colour; additive faces (flames,
   bolts, crystal shells) light nothing (`MASK_GLOW` is read only by
   mirrors); lava lights through a gain of 5 on bounce hits
   (`LAVA_LIGHT`). Glows light primary hits only. There is no MIS. Some
   lights have no shadow ray at all: an actor's, any share under 0.08,
   any within 160 units.
6. **Translucency is raster.** Smoke, particles, fireballs and half-alpha
   sprites are drawn after the trace in retail's colours: no traced light
   or shadow. Reflections show them as discs and ribbons
   (`mirrored_effects`).
7. **Models retail culls are absent.** The feed copies what the game
   draws, and the game culls to the view cone: nothing behind or beside the camera casts a shadow or shows
   in a reflection. The first-person hull is absent too. (Stage 8 takes
   both, 2026-10-29.)
8. **Materials.** Lambert plus an isotropic GGX metal lobe whose weight
   has no Fresnel, no geometry term and no pdf; non-metals have no
   specular. One metalness and roughness per model name, nothing per
   texel. A path holds at most one specular event (water never shows
   metal, puddles never show in reflections). Puddles, wet ground,
   lightning and sparks exist only where the eye ray lands.
9. **Media.** Fog and mist lie only between the eye and the first hit:
   reflection, bounce and shadow rays cross clear air (the lake mirrors
   unfogged hills). No water medium under the surface.
10. **The denoiser** filters only the bounce and the sun's visibility,
    reprojects by the camera only (a moving model restarts its history),
    has no variance guide, and white noise drives every sample.

## What "full" means here

The bar is the games that ship full path tracing (Cyberpunk 2077's RT
Overdrive, Alan Wake 2, Portal RTX), with our two rules on top:

- **Every light comes from traced paths**: the sun, the sky, retail's
  light records, the glows, the headlamps, emissive surfaces and lava. No
  constant ambient term and no retail vertex light anywhere in a lit
  pixel. A world with retail's painted sky gets a uniform sky radiance (a
  traced sky, just without detail).
- **Light bounces on**: the first bounce traced at every pixel, every
  bounce after it through a world-space radiance cache, as those games
  do, with light sampled at every vertex.
- **HDR radiance in linear light** from the first hit to one tonemap.
- **Physically based BSDFs**: Lambert diffuse and GGX specular for every
  surface (dielectric and metal), with Fresnel, Smith's masking and
  correct pdfs; water a dielectric interface.
- **Fog along reflections**, not just the eye's view.
- **Translucent effects lit** by the same light as everything else.
- **Sharp** and **fast** (the ground rules below).

Not part of "full", as in those games: unbiased convergence of a still
frame, depth of field, motion blur, subsurface scattering, spectral light.

## Ground rules

- **Sharp, no smear** (2026-10-28).
  - Denoise light, never texture: albedo stays demodulated, and the sun's
    shadow keeps its own visibility channel (sharp edges, soft where the
    disc makes them so).
  - Histories stay short and clamped (diffuse at most 16 frames with a
    fast 2-to-4-frame history beside it that the long one may not stray
    from, ReBLUR's anti-lag; specular only above roughness 0.4 and at
    most 8 frames; shadows 12), reproject by each surface's own motion
    (specular by its reflected image's), and drop on disocclusion, where
    the pixel traces 2 to 4 extra samples instead of widening the
    filter.
  - Mirrors (water, puddles, smooth metal) are lit deterministically at
    their hits and never denoised.
  - The spatial filter stops at edges in normal, depth and luminance
    (variance-guided), so contact light keeps its shape.
  - Under an upscaler the denoiser's history shortens (two temporal
    filters stacked smear), and DLSS Ray Reconstruction at native size is
    measured as the sharp reference.
  - Sampling quality (blue noise, the exact light pick, MIS) comes before
    more accumulation.
- **Performance first** (2026-10-28). Every stage is measured
  on the RTX 5080 with `scripts/perf-scenes.ps1` at the reference
  settings (4K, path tracing, SMAA, no upscaler) and at DLSS Quality,
  against the [baseline](#baseline-2026-10-28), with the rays a pixel each
  stage adds written beside its times. The budget is in the baseline
  section; a stage that breaks it is reworked before the next starts.
- **Recognisably Infestation.** Retail's colours stay where they were for
  what retail lit flatly; where real light changes a look (the
  terminator, the ground's shadowed slopes, colour bleed), the stage says
  so and shows it in its snapshots. The values are tuned in
  `modern_tuning.json` (no player-facing sliders).
- **Only Vulkan with path tracing changes.** OpenGL and the software
  raster stay retail, their game retail's under any camera or scaling;
  the tracer reads the game's state, timing and random numbers and never
  writes them. Vulkan single player plays the remaster's game (the
  Remastered view and, in a session that started traced, the longer draw
  distance drive it), and network games retail's on every renderer
  ([the game's draw and the shown draw](../re/PRESENTATION_POLICY.md#the-games-draw-and-the-shown-draw)).
- **Each stage ends with before-and-after snapshots** of worlds 2, 3, 9,
  14 (night), 17 and 18 and Pelagia, plus a low-sun scene for the
  terminator (a bunker, round models, hills), then a commit and a push.

## Design

### Units and colour

- **The unit is retail's shade**: light is irradiance over π, so a
  surface's outgoing light is its albedo times its light, and a white
  surface under retail's full shade is 1.0. Every BSDF evaluated toward a
  light (diffuse and GGX, the sun and each picked light) is multiplied by
  π to match, so the sun, the sky, the lights and emitters (in radiance)
  agree. The GGX sun highlight today lacks that π. A furnace test pins
  it: a uniform sky of 1 on a white Lambert surface gives 1, and the sun
  at N·L 1 on white gives the sun's colour.
- **Linear light** (stage 2). The tracer's textures are made
  `R8G8B8A8_SRGB`, so the hardware decodes and filters them, and their
  mips are built in linear light; retail's colours (tints, vertex
  colours, the lights' colours) are decoded where the trace reads them;
  the albedo image stores albedo sRGB-encoded (a linear 8-bit albedo
  bands in the darks) and the resolve decodes it.
- **The cost of linear light, stated.** Retail lit in display tones, so
  N·L 0.5 under full sun showed 0.5; in linear light it shows 0.73 (0.25
  shows 0.54). Terminators soften and curved models and slopes flatten a
  little. This is the look of every path-traced game and of physical
  light; it is the stage-2 look change, shown in the low-sun scene. If
  retail's harder terminator is wanted back, a per-world contrast
  term in the tuning can steepen it; there is no physical way to keep it.
- **Retail's fog is mixed in retail's tones** (it is retail's flat fog,
  not light), and so are the composite's clouds, mist and weather, which
  were tuned in them.
- **The tonemap** is the identity up to 0.8, then Khronos PBR Neutral's
  shoulder to white, hues kept until late (`tonemap` in `pt_frame.glsl`).
  PBR Neutral's toe, which takes 0.04 off every colour, is left out: it
  would darken retail's shadows. The Lighting presets' grade (exposure,
  saturation, curve) stays where it is, after the raster's late draws. No
  automatic exposure: retail's night worlds stay dark. Retail's sunlit
  surfaces lie above 1 before its cap (the sun plus a quarter, up to
  1.99 in shade units), so a bright world's sunlit side pales; each
  snapshot world's share of pixels past the knee is measured, and a
  per-world exposure set where its hue washes out.

### The HDR frame (stage 1)

- **Light runs uncapped from the trace to the composite**, which
  tonemaps it into the 8-bit target. The raster's late draws (smoke,
  fireballs, the HUD) blend over the tonemapped world as they blended
  over retail's: a darkening draw, half-alpha smoke and additive glows
  keep their look, no pipeline needs an HDR format, and no 66 MB image
  is added at 4K. The colour clear is recorded at the end of the previous
  frame, before anyone knows the next is traced, which a separate HDR
  world image would also have had to work around.
- **Finished colours** (the water's, lava's, a glow's, the sky's, the
  spray's) go to the light image (RGBA16F), not the 8-bit albedo, which
  keeps a clamped copy for Ray Reconstruction's guide.
- **Between the resolve and the composite** the light travels in a
  reversible form: squeezed below 1 by its brightest channel (x / (1 +
  max), Karis) and sRGB-encoded (`to_transport` in `pt_frame.glsl`). SMAA,
  FXAA, TAA, FSR, DLSS and XeSS take it unchanged, as the 0-to-1 tones
  they were made for (no flags change, so the NGX bridge needs no
  rebuild); the composite expands it (`from_transport`). Ray
  Reconstruction, which is told the colour is HDR, takes the linear light
  as it is.
- The puddles' `extra_cap` and the resolve's cap are gone; so is
  `capped()` at every secondary hit.

### Materials

- **Albedo** is the texture times the vertex colour, both decoded. A
  model's retail shade is no longer its light (a flag of its own, not the
  menu's `lit_by_trace`, which also changes normals, the sky and round
  parts). The shade's other parts move: the fan factors above 128 (the
  red cross's white) into the albedo, as `min(colour × factor, 1)`; the
  pick-ups' pulsing share (`own`) into emission in the pulse's colour;
  the hit flash stays in the vertex fog (`OWN_FOG`).
- **The ground** loses its baked shade: its reflectance comes from the
  slope of that shade with the sun's facing and the sun's tint (stage 2's
  entry below has the formula and why), so sunlit ground stays as much
  dimmer than the models as retail drew it; its local lights stay laid on
  the bare texture, as retail added them. Retail's ground took about half
  the models' ambient share; under one sky its shadowed and turned-away
  slopes come out brighter than retail's, a stated look change.
- **BSDF**: Lambert diffuse weighted by (1 − F) plus GGX specular for every
  surface, sampled by visible normals (Heitz 2018) with height-correlated
  Smith masking and Schlick's Fresnel. Dielectrics have F0 = 0.04 and the
  tuning's roughness (a default for models, rougher for the ground);
  metals take their F0 from the albedo and have no diffuse.
- **Lobes at the primary hit**: one diffuse ray; a specular ray as well
  only where the surface is metal or smoother than 0.5. A rough
  dielectric's specular comes from the sun and the picked light only.
  Lobes are picked by weight only at later vertices.
- **The resolve splits diffuse and specular**: `emission +
  albedo_d × (direct_d + sun_d × seen + indirect_d) + (direct_s + sun_s ×
  seen) + albedo_s × indirect_s`, where `albedo_s` is the pre-integrated
  specular term (Karis's analytic fit of the split sum), which also
  demodulates the specular light and is Ray Reconstruction's specular
  albedo guide.
- **Wet** means darker and smoother, not more reflective (rule of
  2026-10-28). Built (2026-10-29): a puddle mirrors by water's
  Fresnel (Schlick from F0 0.02) over the dark wet bed, and soaked
  ground's sun sheen is a water film's (F0 0.02, roughness 0.5), the
  raster's constants (`pt_ground.glsl`'s `WATER_F0`,
  `WET_GROUND_ROUGHNESS`). The ground is soil, sand, grass and rock
  (retail's terrain names none hard); the hard surfaces are models, whose
  paint takes a clear coat and whose metal mirrors more and smoother; the
  tyres' bare rubber (`BARE`) soaks darker, smoother, with a faint mirror
  of its own colour and no coat. Still planned: the coat at every path
  vertex, not just the first.
- **Emission** is a material with a black BSDF (an emitter reflects
  nothing): retail-unlit faces emit their colour (the scene unit, so they
  look as retail drew them), tuned by name in `materials.emitters` for
  the faces meant to glow (lights, signs, the logo). Lava emits its
  colour in HDR (`lava_glow`, one function for every ray that sees it),
  which retires `LAVA_LIGHT`. The headlamps' lens discs and the land band
  emit to the eye and mirrors only, never as lights. Emission is
  two-sided, as the instances are. (Stage 5 built the tuning, empty by
  default, and kept the lenses and the band out of the light list as
  their own surface, `Shown`; their light through the rays that meet them
  stays as it was. Lava's gain in what the eye sees is held: see stage 5.)
- **Bounce colour**: today a diffuse bounce is pulled half way to grey
  (`bounce.colour` 0.5, tuned so sunlit grass does not tint shaded walls
  green). It stays, as a desaturation of the path's throughput after its
  first diffuse vertex: not physical, but the tuned look, and colour
  bleed compounds with every bounce.

### Lights

- **The sun**: a disc of `sky.sun.shadow` degrees, sampled by its cone at
  every vertex with one shadow ray; it is never in the sky radiance that
  rays read, so nothing counts it twice. Its specular on every lobe is one
  rule: GGX with the roughness widened by the disc (Karis's representative
  point), times the sun, times Fresnel, under the same visibility as its
  diffuse. That replaces the water's glint (`pow(N·H, 600) × 4`) and the
  metal highlight, whose tuned strengths stay as multipliers.
- **The sky**: one cube, rendered without the sun's disc (a flag in
  `sky_gradient` for the cube pass), at 512² with mips, two faces updated
  a frame. Rays that miss read it, a coarser mip the rougher the last
  bounce; mirror misses add `sky_gradient` analytically, so the horizon,
  stars and planet stay sharp, and take only the clouds from the cube.
  The kept retail sky art (`sky_objects`, a ray-query loop) is read only
  by mirror rays.
- **The sky's strength as light**: retail's ambient (tint over 4) stood
  for all indirect light. The sky lights with retail's ambient, in its
  colour, given out by where the sky is bright (stage 2's entry), times a
  per-world tuned share (`sky.light`, 1 until stage 4's bounces are
  measured against it). Mirrors show the sky as it is drawn. Painted-sky
  worlds (0 and 19) get a uniform sky of retail's ambient.
- **Retail's records** (explosions, flames, missiles, the vehicle's
  lights) keep retail's window as their profile, now in 3D distance:
  `colour × (1 − d²/r²)² × N·L` (models keep retail's 1 + N·L). An
  inverse-square falloff cannot keep retail's pools: a record sits 64
  units over the ground with a radius near 512, so at half its reach the
  inverse square gives 1.4% where retail's window gives 75%. A small
  sphere (`LIGHT_SIZE`) is only the target of the shadow rays.
- **Retail's negative channels** (`OBJECT_LIGHTING.md`): the missiles'
  green and blue wrap below zero within 1500 of the ground (a red glow
  that also darkens), and terrain lights 19 and 54 and selector 30 carry
  signed colours. Light cannot be negative: paths and the light pick use
  a light's positive part only, so the missiles' glow is plain red. An
  authored light that is negative in every channel (a darkness) is listed
  by world before stage 2 lands; if any world has one, it is kept as
  retail's darkening at primary hits only, the result held at 0 or above,
  and it is an open call.
- **The tuned glows, shots, lasers, gates and headlamps** stay analytic
  lights (they were tuned by hand), now at every path vertex, each with its
  shadow ray. Their drawn geometry (crystal shells, bolts) is what
  mirrors and the eye see; the light it gives is the glow's, so nothing
  counts twice.
- **Picking one light**: at every vertex after the first, one light is
  picked from every light in reach (records, glows, shots, lamps) in
  proportion to its unshadowed light there, the BSDF included, and
  traced with one shadow ray: the exact pick the bounce already makes,
  widened. Resampled importance sampling (RIS) or a light tree only if
  the lights grow into the hundreds: a few uniform candidates would
  mostly land on lights out of reach. Primary hits keep tracing every
  light in reach while that stays cheap (0.2 to 0.5 ms through a
  grenade's blast at 4K).
- **Emissive triangles**: the faces named in `materials.emitters` go into
  a per-frame light list (area times radiance, a CDF), sampled at every
  vertex; a BSDF ray that hits one is weighted by MIS against that
  sampling with the lobe mixture's pdf. Unnamed retail-unlit faces (dim
  and many) light only through the rays that hit them. (As built in
  stage 5: every retail-unlit face with light is listed, named or not, so
  the noise drops with the tuning empty; grouped, and sampled at the
  first bounce's diffuse vertex only.)
- **Lava** has a sampler of its own: points on the lava plane within a
  disc round the shading point's foot, by solid angle, with MIS against
  BSDF rays, both through `lava_glow`. It is too big and too procedural
  for the triangle list. (As built in stage 5: directions, cosine-weighted
  and turned down toward the plane, over water tiles only, each traced to
  what it meets.)
- **Lightning**: the flash becomes a light from its bolt's side at every
  vertex, with a shadow ray (or else it lights tunnels and bunkers), and
  stays out of the sky's light.

### The integrator

Per pixel, one sample:

1. The primary ray as today (the dissolve, the far water plane).
2. At its hit: emission; the sun (one ray, its visibility to the
   denoiser's shadow channel); every light in reach traced; the lamps.
3. **The first bounce, traced**: the diffuse ray (and the specular one,
   above), and at its hit emission (with MIS where it is a sampled
   light), the sun and one picked light.
4. **Every bounce after, from a world-space radiance cache** (as
   Cyberpunk's RT Overdrive does with SHaRC): a hash grid of cells, finer
   near the eye, keyed by position and normal, each holding the light
   leaving it. The baseline shows a second traced bounce at every pixel
   would cost about what the first does, far past the budget; the cache
   gives every bounce after the first for a lookup, with no extra ray,
   and holds only light from the second bounce on, so it cannot blur the
   image. It is filled by a sparse share of pixels (one in 16, tuned by
   cost) that trace on, vertex by vertex, writing what each vertex sees
   into its cell; their roulette starts after the third vertex, with
   survival `min(1, max throughput channel)` and no floor (a floor of 0.05
   made ×20 fireflies), to a cap of `bounce.depth` (4). A resolve pass
   blends each cell's new samples into its value and drops cells unseen
   for a while.
5. **Mirrors**: water's two rays at the primary hit, puddles and smooth
   metal stay noise-free. Their hits are lit deterministically: every
   light in reach traced, the sun by one ray to its disc's middle, and the
   light from further bounces from the radiance cache; no stochastic path
   continues from them. A mirror's hit may be another mirror once (water
   showing metal, a puddle in the lake).
6. **Water** at a path vertex is a dielectric: reflection or refraction
   picked by Fresnel. Its body is a medium: the light it scatters is its
   colour times the sun at the surface (visibility, clouds) and the sky,
   not retail's flat shade; one transmittance law for every path (the
   tuned Gaussian `water_clear`, on the total distance travelled under
   the surface). The sun reaching the bed loses its Fresnel share and its
   absorption on the way down, times `caustic()`.
7. **Fog on reflections**: the distance fog counts from the eye, by
   design, so it applies only along mirror chains, by the whole path's
   length (the reflection's light times T(eye + reflected) / T(eye), the
   eye's part already applied). Diffuse segments take only the height
   mist's attenuation, which is a property of space; shadow rays take
   none but the mist's.
8. **Fireflies**: each indirect contribution after the first bounce is
   clamped (its brightest channel at most `bounce.clamp` times full light,
   4 to 8).
9. **Outputs**: emission and finished radiance; direct light diffuse and
   specular; the sun and its visibility; indirect diffuse and indirect
   specular, each demodulated by its own albedo.

The paths run in a dispatch of their own (`pt_paths.comp`), reading the
primary hit from the trace's images, so the primary kernel (3,229 lines,
every feature inlined) keeps its occupancy; both ways are timed.

### Translucent effects

Alpha-blended and darkening late draws (smoke, debris, half-alpha
sprites) are lit once per sprite: a small compute pass over the feed's
sprites takes, at each sprite's middle, the sun through one shadow ray
and the cloud shadow, the sky's light and retail's records, and writes
each sprite's light over retail's full light into a buffer the late draws
multiply by, in display tones. Additive draws are emitters and keep their
colour. (The mist's froxels hold the fog's own colour, not a light field,
and exist only where there is mist, so they cannot light smoke.) Smoke
casting shadows is not planned.

### The denoiser

- Indirect diffuse and specular apart; a-trous guided by variance
  (SVGF's luminance stop) as well as normal and plane.
- Diffuse reprojected through each surface's own motion
  (`object_motion`); specular through its reflected image (the hit
  distance, kept in 32 bits: `geometry.w` is 16-bit), blending toward the
  surface's motion as roughness rises. Built 2026-10-29 (stage 6): the
  glossy rays' lengths in a 16-bit image of their own (`glossy_hit`, ample
  for the parallax they set), the image placed by the specular dominant
  direction's share.
- The anti-lag history and the disocclusion samples of the ground rules.
  Built (stage 6); since 2026-10-29 the diffuse anti-lag judges a change
  by its samples' hit probability, so sparse light is not read as light
  going out (the glossy lobe's keeps its spread test, held).
- Blue noise (a spatiotemporal blue-noise texture) in place of white
  hashes. Built 2026-10-29 (stage 6) for the pairs of numbers the
  interleaved gradient noise drew (the bounce's direction, the sun's disc,
  the emitters' points); the picks (the lobe, a light, the roulette) stay
  hashes.
- Ray Reconstruction's guides made real: specular albedo (the split-sum
  term), roughness, a metal's zero diffuse albedo.

### Culled models

Shadows and reflections of models outside the view need them drawn, and
drawing a model writes game state (the wheels' and legs' callbacks write
the arena; the draws use the world's random numbers). The plan's rule
for this ("off-screen geometry comes from a side-effect-free pass") is
met by the shown draw's sandbox: the models outside the view draw on
copies of all they could write, for the tracer alone (stage 8;
[the tracer's models outside the view](../re/PRESENTATION_POLICY.md#the-tracers-models-outside-the-view)).

### Plumbing

- `TEXTURE_BINDING` (the bindless array, which must stay the highest
  binding) moves from 31 to 63 in `trace.rs` and `pt_scene.glsl`, so
  later stages add bindings below it; the pool sizes, counted by hand,
  grow with them.
- The tracer's timing summary is derived from `STAGES` by name, not by
  index; the tonemap and the late draws get a stamp pair.
- `the_frame_block_has_as_many_floats_as_the_shaders_read` pins the frame
  block's size against `pt_frame.glsl` (a `debug_assert` did before,
  which release probes skip).

## Order of work

1. [x] **HDR frame** (2026-10-28). The caps removed; finished colours in
   the light image; the transport form through the anti-aliasing and the
   upscalers; the tonemap in the composite. The trace keeps retail's
   tones, so the look moves only where retail clipped: mean difference
   from the baseline 0.06 to 1.8 levels (lava most, its sunlit rock and
   banners), `.tmp/fullpt/shots/s1a/`. GPU time unchanged within the
   runs' noise (Terra Nova 14.0 ms, Stendahl Alpha 18.6, lava 13.5).
2. [x] **Traced light in place of retail's** (2026-10-28). Linear light
   in the trace; every model and the ground lit by the world's sun, the
   sky and the lights alone (retail's shade only for the pick-ups' share);
   retail's records in 3D with retail's window; one sampled glow at a
   bounce's hit; the sun-free sky cube with mips; the lamps' roll-off knee
   gone (the tonemap rolls off). Mirror hits take every light, as primary
   hits do. Sheets `.tmp/fullpt/shots/s2h/` (stage 1 left, stage 2
   right, eight worlds). What it settled, by measurement:
   - **The sun's strength.** Retail's tints pass full light (Terra
     Nova's 1.0, 1.55, 1.39; Foldrak Noir's 1.4, 1.9, 1.9; world 18's 1.6,
     1.2, 1.0), which its cap turned into "everything facing it is fully
     lit". Decoding the excess with retail's gamma made the sun three to
     four times full light and blew the ground out to white; capping it at
     full light left bright worlds' ground at half. The sun is its hue
     decoded at full light times its peak as it is (`sun_light`): retail
     showed min(1, tint N·L + ambient), which this follows best under the
     tonemap.
   - **Decoding a light past full light** keeps its hue (decode at full
     light, then scale): decoding each channel past 1 turned Terra Nova's
     cyan sun green (`light_to_linear`).
   - **The ground** keeps the slope of retail's baked shade
     (`ground_shade_slope`, fitted per terrain mesh) and takes its
     reflectance from it and the sun's peak (`ground_reflectance`:
     `to_linear(min(1, a × peak)) / peak`), so its sunlit face shows as
     retail drew it. Its local lights (records, glows, lamps) are laid on
     the bare texture, as retail added them to the ground's vertex colours.
   - **The sky lights as retail's ambient did, in its colour**, given out
     by where the cube is bright: its light is the ambient times the cube
     over the cube's cosine-weighted mean, measured each workgroup from 16
     directions at the cube's level 5 (`measure_sky`), at `sky.light` (1).
     The sky's own colour (Terra Nova's green clouds) flooded every shadow
     otherwise, and a mean from the recipe's zenith and horizon missed the
     clouds.
   - **Look changes, for review**: lights now add in linear light, so
     overlapping coloured pools add up less over lit ground than retail's
     display-tone sums did (Pelagia's 34 lights: a rear lamp's pool about
     35% darker on screen at night; Terra Nova's lavender snow, retail's
     cap mixing the markers' pink with the cyan sun, is the sun's cyan; the
     markers' pools subtler by day). No constant gain gives retail's sums
     back: it would also lift the dim ground that linear light already
     lifts. The ground's baked darkening gives way to traced shadows
     (Foldrak Noir's far hill, dark in retail's bake, faces the traced
     sun).
   - GPU, 4K native: Pelagia at night 13.6 ms, Terra Nova 14.9, Stendahl
     Alpha 18.0, lava 14.2, against 13.6, 15.2, 18.9 and 14.0; the sky
     stage 0.25 ms more (the cube's mips). A later run of all ten scenes
     was slowed by other GPU work (the baseline exe too: Terra Nova 18.4
     against its 14.4) and is not counted.
   - Not done here, moved on: the π convention and its furnace test, and
     the BSDF's π, go with stage 4's BSDFs; the fan factors above 128 into
     albedo with the shade flag (no world model but the pick-ups showed
     them).
3. [x] **Trim and pacing** (2026-10-28). Pacing: the traced frame no
   longer waits for the frames in flight. The tracer's host-written
   buffers, its descriptor pool and timestamps, the sky's pool and SMAA's
   pool are one per frame slot; a full barrier opens each trace (the GPU
   rewrites the bottom and top levels and images the last frame read); an
   outgrown acceleration structure is destroyed only once its slot's
   submission is done. Sync validation (`VK_LAYER_VALIDATE_SYNC=1`) clean
   under SMAA, TAA and DLSS; snapshots identical to stage 2's. Frame time
   at the reference settings, stage 2 against this, two interleaved rounds:
   Pelagia 16.4-17.3 to 13.8-14.4 ms, Terra Nova 18.5 to 16.3-16.9,
   Stendahl Alpha 21.7-22.0 to 18.6-20.2, lava 17.1-17.3 to 14.7-14.9;
   the frame is now the GPU's time, CPU and GPU overlapping. The grade,
   thermal and raster passes still settled (their paths only); the grade
   and the raster's anti-aliasing took one pool a frame in flight on
   2026-10-28 (Eye candy's traced frame 13.5 to 13.0 ms on Pelagia, 14.4
   to 14.1 on Stendahl, at 4K), the raster opening its passes with the
   frame barrier as the trace does (DLSS's own images wrote across two
   frames in flight otherwise), and thermal vision's camera too (validation
   clean; its frame is the CPU's, 8.1 to 8.5 ms against 8.3 to 8.6). Only a
   new upscaler's set-up and an explicit read back still wait. The model
   bottom level is built for a fast trace (`PREFER_FAST_TRACE`): its build
   0.04 to 0.07 ms longer, the trace a little more shorter, the frame's GPU
   time 12.53 to 12.43 ms on Pelagia, 12.35 to 12.18 on Terra Nova, 13.99 to
   13.78 on Stendahl (medians of three alternating rounds at 4K).
   The trim, measured the same day (a local build whose trace switched
   one feature off at a time, `.tmp/fullpt/trim/`; the GPU's clock
   drifts with the desktop's own GPU work, Wallpaper Engine and Parsec
   among it, so later comparisons alternate builds and take medians):
   - **The one bounce is half the trace**: without it Pelagia's trace
     fell from 8.4 to 5.7 ms, Terra Nova's 11.4 to 5.4, Stendahl Alpha's
     12.0 to 6.0, lava's 8.8 to 4.6. Its light picks were not the cost:
     one pick over every light (records, glows, lamps) with one shadow ray,
     in place of a pick of each kind (six loops, three rays), saved 0.2 to
     0.25 ms (medians of five alternating runs). The ray and its hit's
     shading are; inside the primary kernel a bounce on fewer pixels saves
     nothing (a warp waits for its slowest pixel). So stage 4 runs the
     bounce and the cache's paths in their own dispatch, at a quarter of
     the pixels (one of each 2x2, the one turning each frame), reconstructed
     guided by depth and normals before the denoiser: indirect light is
     soft and demodulated, so texture and direct light keep every pixel.
   - The rest is within the runs' noise (the glows at primary hits, 0.7 to
     2 ms on Terra Nova's markers, the lights, the sun's shadow ray, the
     dissolve, puddles, the water, the sky's measure).
   Plan, as written before:
   Remove `settle()` from the traced frame: the
   frame block, the host buffers and the descriptor pools become one per
   frame in flight, so the CPU records the next frame while the GPU
   draws this one (today the device waits 4 to 13 ms a frame for the GPU
   and the GPU then idles while the CPU records). Then find what the
   trace spends its 8 to 13 ms on at 4K (shader variants with one feature
   off at a time, timed in the baseline's scenes) and take the cuts that
   change nothing seen; try `PREFER_FAST_TRACE` on the model bottom
   level.
4. [x] **Paths** (2026-10-28). First the path pass: the bounce left
   the primary kernel for `pt_trace.comp` built again with `PATHS_PASS`
   (`pt_paths.comp.spv`, from `scripts/compile-shaders.ps1`), reading the
   primary hit back (depth, normal, and a face and material image, binding
   27) at one pixel of each 2x2, the one turning each frame
   (`path_offset`); `pt_temporal` fills the other three from the traced
   pixels round them on the same surface (distance, normal, and bounce
   light taken at all). Ray Reconstruction gets every pixel traced. The
   trace at 4K, medians of four alternating runs: lava 9.3 to 5.6 ms,
   Stendahl Alpha 11.2 to 6.3, Terra Nova 11.8 to 6.3; snapshots within
   0.1 level of before (`.tmp/fullpt/shots/s4a/`); sync validation clean
   under SMAA and Ray Reconstruction.
   Then the radiance cache, the same day (`pt_cache.glsl`,
   `pt_cache.comp`, binding 31; the bindless textures moved to 63): a
   hash grid of 2^20 cells (36 MB), 32 units within 1,024 of the eye and
   doubling with each doubling of the distance, split by the axis a
   surface faces; a bounce's hit (and a mirror's) takes the light of
   every bounce after it from its cell, retail's ambient until a path has
   reached it. A quarter of the traced pixels (a sixteenth of all) carry
   their path one bounce further and add what arrives to the cell they
   left (its far end reading the cache in turn, so the cells gather every
   bounce); the resolve blends a cell's samples over 24 frames and empties
   one no path reached for 300. Emptied when the world changes. In the
   open the cells' light is near the ambient it replaces (the sky is
   matched to it); under and between things it is the light that reaches
   them. 0.4 to 0.65 ms of the trace at 4K (medians of four alternating
   runs: lava 5.1 to 5.5, Stendahl Alpha 6.0 to 6.4, Terra Nova 6.1 to
   6.8).
   Then the BSDFs' first part, the same day: every model takes the sun's
   specular (GGX, height-correlated Smith, Schlick's Fresnel from 0.04
   rising to the albedo with metalness, the roughness widened by the sun's
   disc and renormalised; in light_of's units, so the π the old highlight
   lacked is in), the matte ones at roughness 0.75 at most, a soft sheen;
   it goes to a specular image (binding 32) the resolve adds under the
   sun's filtered visibility, not times the albedo; the diffuse is weighted
   by one less Fresnel and by one less metalness; wet ground's sheen is the
   same specular at roughness 0.2 (a water film's since 2026-10-29: F0
   0.02, roughness 0.5, the wet rule above). The metal's reflection ray samples GGX's
   visible normals (Heitz 2018) with Smith's masking as its weight; a
   bounce sample is held to 8 times full light (its hue kept); the bounce's
   direction and the sun's cone take interleaved gradient noise turned by
   the golden ratios in place of white noise. The metal highlight's own
   hard shadow ray went with it: Stendahl Alpha's trace 6.5 to 6.3 ms,
   Terra Nova's 6.8 to 6.7 (medians of three). Fixed with it: the path
   surface image was never moved to the general layout (validation caught
   it with the specular image). Then fog on reflections: a mirror's light
   (the lake's, a puddle's) loses what the fog takes over its own leg,
   between the mirror's distance from the eye and the hit's
   (`mirror_fog`: the distance fog's optical depth, or the haze, the
   composite fogging the eye's leg), so the lake mirrors the hills as
   fogged as they stand (`.tmp/fullpt/shots/s4d/`, world 9). And the
   water's body takes the sun only where the sun reaches its surface (one
   shadow ray, the cloud shadow), retail's quarter in shade: world 22's
   water in its hill's shadow darkens (`s4e`). Left for stage 6, which
   owns the denoiser: the indirect specular apart from the diffuse with
   its own history, the variance guide, the anti-lag history. The water
   as a full medium (its body's light from the sky, one transmittance law
   along paths, the bed's sun through the surface) waits on a
   look at the water. As planned before:
   its sparse paths in `pt_paths.comp`; one picked light per vertex; the
   deterministic mirrors; water as a medium; fog on reflections; the
   firefly clamp; and with them the denoiser they need: blue noise, the
   variance guide, the specular chain, the anti-lag history.
5. [x] **Emitters** (2026-10-28). Lightning as a light first: a flash
   lights what faces it and sees it (a shadow ray toward it, only while it
   flashes), not the inside of a tunnel or a bunker. Then the rest, on
   branch `infestation-pt-finish`:
   - **The emissive light list** (`vk_device/emissive.rs`), built each
     frame from the feed: every face retail drew unlit with any light,
     named or not, so that with the tuning empty the noise drops and
     nothing else moves. Faces are grouped by the object that drew them
     and a 256-unit cell; a face's power is its area times its light's
     luminance (its corners' colours, its texture's mean over its drawn
     texels, its gain); the groups go the likeliest from the eye first and
     the first 64 are sampled (`MAX_GROUPS`). The headlamps' lenses and
     the land band are a surface of their own (`Shown`): seen and lighting
     through the rays that meet them, as before, never sampled.
   - **Next-event estimation with MIS**, in a pass of its own after the
     path pass at the same pixels (`pt_emitters.comp.spv`, the trace's
     source built with `EMITTERS_PASS`), dispatched only in frames with
     listed emitters or lava. For a diffuse bounce (the path pass's own
     pick, from the same random numbers): one group picked by its power
     over its distance squared and how far its sphere rises over the
     surface's horizon, a face by power, a point even over it (a masked
     sprite's keyed texel gives nothing), one shadow ray. It is weighed
     against the bounce ray by the power heuristic, and the path pass
     weighs what its own ray met of a listed face the same way, so each
     emitter counts once. Where the emitters' unshadowed light sums to
     under 0.001 (`EMITTER_LEAST`) the bounce rays alone light a point, as
     before: a lamp thousands of units off is not worth a ray. A glossy
     (metal) bounce takes emitters through its ray alone, and so do the
     bounce's hit and the radiance cache's paths (the cache averages them
     over its 24 frames). The first build cost 0.7 to 1.4 ms of trace at
     4K in every scene, emitters or none
     (`.tmp/ptemit/evidence/perf-4k-v1-inline.txt`). Moving the samples
     into a pass of their own did not remove it (`perf-4k-v3.txt`);
     keeping the primary trace's records at their old size did: a
     triangle's place in the list lives in a buffer of its own (binding
     35) rather than in its record, which every hit reads, and a tuned
     face's gain rides on its albedo rather than in a field of the
     surface (`perf-4k-v4.txt`). The pass of its own stays: the path pass
     keeps only the weights.
   - **Lava's sampler**: a direction drawn as the diffuse bounce draws
     one, cosine-weighted about the normal, turned down toward the lava's
     plane where it rose (its density counts both draws that give it),
     kept only where it crosses the plane over a water tile within the
     water mesh's reach (`over_lava`, the heightfield's corners, the
     mesh's own rule: a point over land spends no ray), traced to what it
     meets, the melt's light taken as a bounce ray takes it. The plan's
     disc round the foot became that domain: a wall or vehicle over the
     lake aims at the melt below it, the ground beside it at nothing.
   - **HDR emission**: `materials.emitters` (model name and gain, `*` as in
     `names`) makes a model's unlit faces glow that many times brighter
     than retail drew them, shown past full light (the tonemap pales them)
     and lighting in proportion. Empty by default: every emitter as retail
     drew it. Which faces (the RT plan's candidates: lights, signs, the
     logo) is an open pick.
   - **Held: lava's drawn brightness.** Its light is `lava_glow` at
     `LAVA_LIGHT` (5) for every ray that takes its light (the bounce, the
     sampler, the cache's paths), as before; what the eye and mirrors see
     is its colour without the gain. Folding the gain into what is seen
     (the plan's "retires `LAVA_LIGHT`") would show the lake five times
     brighter under the tonemap, a look change; an open call.
   - **What the list holds** (the start views of worlds 0 to 24): in most
     worlds nothing. The unlit faces of the scientists, the ships'
     wings, the gates and the silos are a two-by-two sprite keyed through
     (no light). Worlds 0, 19 and 21 have their retail-unlit trees
     (`fatlinetree`, `fattreeplant*`; 147 groups in world 0, the nearest
     6,800 units off), world 6 a scaffold's `Caution` sign (`scafold`, a
     candidate for the list), world 4 `brome1`, world 22 the ship. Pelagia at night and Foldrak Noir at night have none in view
     (their light is the headlamps, records and glows, analytic lights
     already): their frames are bit-identical to before.
   - **What it showed about the bounce alone**: met only by bounce rays, a
     small emitter's light settled at 71% of what it gives: the denoiser's
     anti-lag reads the frames between rare hits as the light going out.
     Sampled, the light settles within 4% of the emitter's form factor
     (3.3% under; `vulkan_emitter_sampling_settles_a_lit_patch_sooner`).
     So where a small emitter lights something near, the image is brighter
     than before, by what the old estimate lost; the measured scenes moved
     by 0.3% of their mean or less. The anti-lag's bias for sparse light is
     stage 6's to look at (it still holds for the bounce's other sparse
     light; taken there on 2026-10-29: met only by bounce rays, the panel
     now settles at 99.6%). These figures are from 2026-10-29, the test
     reading every frame back: first taken from a run that traced 116
     frames between read-backs, which replays the last frame's view and random numbers
     (stage 6's "found on the way"), they read "under half" and, for the
     first frames, 95 to 140% off where the light settles (12 to 36%).
   - **Fixed on the way**: the path pass read `last_hit` (and `met_lava`)
     unset where a glossy bounce casts no ray (its direction turned under
     the surface), so the radiance cache's feed took whatever the
     registers held. Moving the path pass's registers made this branch's
     frames differ run to run by a hundred or two pixels; they start
     cleared now and repeat. Main's own already differ run to run in a few
     scenes (Artoria, Tabuchi's pond and Tabuchi deep, by up to a few
     hundred pixels of one level, against this build's frames that
     repeat); that this was the cause there is likely, not shown. Pelagia
     in the rain still gave one of two frames, here and on main
     (`cd4c27f4d`) alike: the puddles' carved ground patch, built on a
     worker, took over in whichever frame found it done, a frame early or
     late from run to run. Since
     2026-10-29 it takes over 100 ms of the game's clock after it started
     ([ground detail](ground-detail.md)), and Pelagia in the rain, Tabuchi's
     pond, Artoria and Tabuchi deep each repeat in 5 runs of 5
     (`.tmp/ptfix/evidence/determinism-final.txt`).
   - **Evidence** (`.tmp/ptemit/`). Snapshots of 21 scenes at 1080p, main's
     build, this one and this one with three models tuned
     (`shots/ab4/`, `evidence/ab4-stats.txt`, `ab4-gain-stats.txt`): where
     nothing emits, the same as main's (Pelagia dry, in the rain and at
     night, Terra Nova, Stendahl Alpha, Foldrak Noir at night, worlds 5,
     9 and 22 bit-identical; Artoria and Tabuchi's three a few pixels of
     one level, main's own run-to-run difference above); the lava scenes
     0.11 to 0.24% brighter on average, pixels on the lit rock and the
     craft moving at most 15 to 18 levels; world 17's start (the lava far)
     111 pixels by at most 2; worlds 0, 6 and 21 a few hundred pixels.
     With `materials.emitters` naming world 6's scaffold (its `Caution`
     sign) and the trees four times bright, the sign and the trees' leaves
     brighten (`shots/ab4/w6.gaincrop.png`, `w21.gaincrop.png`) and the
     rest of the frame keeps its pixels. Noise, measured on the bounce
     before the denoiser (a measurement-only build tracing every pixel and
     resolving the raw bounce, the samplers switched off and on,
     `noise/raw1/`): the spread of the noisiest 5% of pixels over eight
     frames fell from 12.2 to 8.1 levels on the lava shore, 11.5 to 7.9 on
     the second shore view and 12.7 to 6.9 round the hovercraft on the lava
     (the clumps of rare hits on the hills facing the melt gone, a fine
     even grain left); the whole frame's in Caradion's trench from 6.4 to
     5.3; worlds 6 and 21 unchanged (their emitters lie past
     `EMITTER_LEAST`). Sync validation clean under SMAA (the lava shore,
     world 6) and Ray Reconstruction, FSR showing only its known
     `rw_luma_history` warning (`validation/v4-*.log`). GPU tests: the new
     one and 18 more pass, `vulkan_thermal_vision_shows_heat_in_grey`
     fails as on main (`evidence/gpu-tests-v4.txt`); the library's 4,400
     tests pass.
   - **Cost** at 4K, the reference settings (path tracing, SMAA, no
     upscaler), main's build and this one back to back, the order
     alternating by round, 2 to 8 pairs a scene
     (`evidence/perf-4k-v4*.txt`, `perf-4k-pairs.txt`). Other sessions'
     replays shared the GPU through most rounds, so the figures are
     medians of each pair's difference, and a pair's spread runs to a
     millisecond or two. Where nothing emits, the trace's time moved by
     -0.09 to +0.29 ms (Terra Nova -0.09, Pelagia at night +0.08, Foldrak
     Noir at night +0.14, Stendahl Alpha +0.29, world 6 -0.09: the noise's
     size). Where the samplers run, +0.14 ms on the lava shore, +0.22 at
     world 17's start, +0.26 among world 0's trees (the pass runs, nearly
     every point under `EMITTER_LEAST`), -0.21 round the hovercraft (two
     pairs); the GPU frame at most 0.3 ms longer, 2 to 4%, inside the
     budget's 15%. Rays: one shadow ray (an emitter) and one ray (the
     lava) at a path pixel, a quarter of the pixels, each only where its
     sampler applies.
6. [x] **Denoiser** (2026-10-28 to 2026-10-29; its anti-lag's sparse-light
   bias and the blue noise last, below; held: the glossy lobe's anti-lag).
   Done 2026-10-28: the temporal pass reprojects each
   pixel through its own surface's motion too (`object_motion`), so a
   moving model keeps its history instead of starting again; a metal's
   history follows its roughness (2 frames for a mirror to 8 for rough
   metal, as metalness has it: its light moves with what it mirrors); the
   history shortens to 12 frames under an upscaler (`frame.upscaling`),
   whose own history follows; the a-trous passes stop at the light's own
   edges as well (SVGF's luminance stop, against each pass's 3x3 spread),
   the sun's visibility filtered as before; Ray Reconstruction's guides
   are real (roughness, the split sum's specular albedo by Karis's fit, a
   metal's zero diffuse). Denoise 0.2 to 0.45 ms more at 4K; sync
   validation clean. The anti-lag, 2026-10-28: a short history (the bounce
   light's luminance over at most 4 frames) runs beside the long one, and
   where the two part by more than three standard errors of their
   difference (by this frame's spread round the pixel on its surface: the
   short history's own spread holds the change and hid it) the long one
   gives way to the short. A wall lit only by its sunlit floor keeps a
   fifth of its light (0.19) eight frames after the sun goes out, where a
   32-frame mean keeps four fifths (`vulkan_bounce_light_follows_a_change_within_frames`,
   every frame read back, 2026-10-29: first measured as under a third from
   a run that replayed one frame);
   still scenes flicker as before (frame to frame 7.074 against 7.080 on
   Stendahl, 0.376 against 0.379 on Pelagia); denoise 0.15 ms more at 4K;
   validation clean. Disocclusions, 2026-10-28, by NRD's history fix in
   place of extra rays: tracing every pixel with no history (the path pass
   over the whole frame, picking its two by two's and those) cost 0.6 ms
   of trace on Pelagia's drive, 1.1 on Pelagia and 2.8 on Stendahl (whose
   fog dissolve leaves pixels without history every frame), so a surface
   just come into view instead starts from the traced samples of five by
   five of the lattice round it on its surface (`pt_history.glsl` holds
   the reprojection the temporal pass shares): a revealed wall's first
   frame spreads 0.011 against 0.014 from its neighbours alone and 0.0087
   settled, 1.26 times the settled spread against 1.57
   (`vulkan_newly_seen_surface_starts_quiet`, every frame read back,
   2026-10-29; the 0.019 first given as settled came from a settling run
   that replayed one frame, and had the first frame quieter than settled),
   at no measurable cost; the a-trous passes already smoothed most of it. The indirect
   specular apart from the diffuse, 2026-10-29: the path pass leaves which
   lobe each traced bounce took and its glossy ray's length
   (`glossy_hit`), and a metal's or a wet coat's glossy light keeps a
   history of its own beside the diffuse one (each lobe's history takes
   only its own samples; the filter's last pass weighs the two by
   metalness, as the path pass picked between them). The glossy history is
   found where its reflected image lay (NRD's virtual motion): behind the
   surface along the view by the rays' length, all of it for a mirror and
   less as roughness spreads the lobe (the specular dominant direction's
   share, Lagarde and de Rousiers; `pt_glossy.glsl`), a moving mirror's
   image moved by twice its motion along its normal. It holds as many
   frames as the view to that image takes to turn across the lobe's width:
   all 32 while the camera keeps still, two or three on a sharp lobe in a
   fast move. Beside it runs the history's own reach (the rays' length,
   weighed by the light it brought, `glossy_track`): where the history's
   image lay at another depth than this frame's (a reflection's trailing
   edge, whose rays now meet what lies past it) the eye's move parts the
   two images, and the history shortens as for the view's turn (NRD's
   hit-distance check). The a-trous filter's first two steps take the
   glossy light with stops of its own (within the lobe's footprint on
   screen, normals within the lobe's width, its own luminance stop); the
   third weighs the lobes together, so a reflection stays as sharp as its
   lobe left it. The glossy history and the glossy filter steps are
   dispatches of their own (`pt_temporal.comp` and `pt_atrous.comp` built
   with `GLOSSY_PASS`) over the pixels with a lobe, which the sign of the
   diffuse layers' alpha marks: inlined, the glossy code cost every pixel
   registers, and a build with it present but never run denoised about
   0.7 ms slower than main's on Pelagia. This replaces the single history
   shortened by metalness (2 frames for a mirror to 8 for rough metal),
   which dragged a reflection along with the surface's motion and cut the
   diffuse light under a metal as short. Ray Reconstruction takes the
   samples as they are and runs none of it.
   - **Evidence** (`.tmp/ptspec/`). The new
     `vulkan_glossy_reflection_follows_a_camera_move`: a lit panel over a
     half-metal floor (roughness 0.3) in the dark, the eye gliding two
     units aside over eight frames. Four frames after the glide the
     reflection's column profile lies 0.17 of the glide's distance from
     where it settles (0.28, 0.23, 0.20, 0.17 frame by frame); the same
     test on the stage-5 build gives 0.63, 0.57, 0.50 and 0.44, its history
     trailing the reflection along the floor. Settled, a frame lies 3.4%
     of that distance from the frames' mean (2.6% on the stage-5 build:
     each lobe now averages only its own samples, and the glossy light
     takes two filter steps, not three). Against
     the stage-5 build, still starts change only on metal and, in the
     rain, on wet models: by a level or two on average (means under 0.07
     levels), but up to 61 levels at Stendahl Alpha, 49 in Pelagia's rain
     and 36 at Tabuchi dry, where a reflection's edge moved (11 scenes,
     `shots/ab3/`, `evidence/ab3-stats.txt`); in Pelagia's drive and turn the
     frame-to-frame change on the craft is the same (6.19 against 6.20
     levels, 5.48 both; `evidence/d12-stats.txt`), and the wet cockpit,
     where the old history held one broad smoothed highlight, shows the
     reflected sky and sand, with some grain while the view turns
     (`shots/d12/dome_both.png`; no converged reference exists for a moving
     frame, so the test carries the measure). Sync validation clean under
     SMAA and Ray Reconstruction in the drive and at Stendahl Alpha, FSR
     showing only its known `rw_luma_history` warning
     (`validation/v12-*.log`). GPU tests: the new one and 19 more pass,
     `vulkan_thermal_vision_shows_heat_in_grey` fails as on main
     (`evidence/gpu-tests-v12.txt`).
   - **Cost** at 4K, the reference settings (path tracing, SMAA, no
     upscaler), main's build (`95b117356`) and this one back to back, the
     order alternating, three pairs a scene on a quiet GPU
     (`evidence/perf-v12.txt`; the medians). The denoise stage: Pelagia's
     drive 2.73 to 3.04 ms (+0.33), dry +0.26, in the rain +0.52 (its
     models are wet: most of them have a lobe), Terra Nova +0.32, Stendahl
     Alpha 3.81 to 5.01 (+1.20: its buildings are metal). The tracer's
     GPU frame grows by 0.07 to 0.56 ms (0.5 to 4%), and at Stendahl Alpha
     by 1.25 ms (8.7%): within the budget's 15%.
   - **Found on the way**: the GPU tests' `trace_world` records into the
     frame that the next read-back (or swap) submits, and each trace
     rewrites that frame slot's buffers (the frame block, the vertices),
     so a run of traces between two reads executes as that many passes of
     the last trace's view and random numbers. Before the new test read
     every frame back, a sideways step showed the floor's light 30 to 70%
     too dark for frames after it. The older tests that trace a run of
     frames between reads (`vulkan_bounce_light_follows_a_change_within_frames`,
     the settling runs of `vulkan_emitter_sampling_settles_a_lit_patch_sooner`
     and `vulkan_newly_seen_surface_starts_quiet`) measured fewer distinct
     frames than they said; since 2026-10-29 they read every frame back
     (`trace_frame`), and their figures above are the measured ones.
   - **Review fixes** (2026-10-29, `.tmp/ptfix/`):
     - **Ghosts with the eye still**: the history's reach was checked
       only through the eye's move, so a model passing a still metal left
       its reflection's history whole but for the luminance anti-lag. Now,
       with the eye still too, where this frame's glossy rays meet their
       light at another depth than the history's (a share past 0.15 of
       the further, all of it by 0.5, counted by the lobe's dominant
       share), the history shortens to the short history's four frames;
       where they bring less than half the history's light, only once the
       short history agrees, so a lone miss of a sparse hit leaves it.
       `vulkan_glossy_reflection_follows_a_moving_model` (the lit panel
       gliding two aside past the still metal floor): four frames on, 0.21
       of the glide's distance from where it settles (0.38, 0.29, 0.23,
       0.21 by frame; 0.43, 0.34, 0.28, 0.24 before: the luminance
       anti-lag held this ghost near already, and the diffuse light under
       the panel follows within frames too). The camera-move test now
       gives 0.25, 0.19, 0.15, 0.12, its settled flicker 3.7% of the step
       (3.4%). Cutting to one frame instead, or on a lone dark frame,
       brought the moving model to 0.17 to 0.19 but raised the still
       flicker to 4.8 to 14% (`glossy_fix*.txt`). Still scenes flicker as
       before (frame to frame over frames 110 to 117: Stendahl 1.078
       levels both, Pelagia's rain 0.312 against 0.313, Tabuchi's rain
       0.956 against 0.957, world 5 0.351 against 0.356).
     - **A lost history stays sharp**: where the reflected image's place
       held no glossy history (a mirror's edge, a curved hull), the
       history now follows the surface's own motion before starting
       again, and a fresh start's wide gather (five by five of the
       lattice, about ten pixels) is no wider than the lobe's footprint.
     - **Not covered**: a wet coat's lobe is sparse (12% of its traced
       pixels take it), and where fewer than two of the lattice samples
       round a pixel took it the luminance anti-lag has no spread to judge
       by and stays off; its ghosts rest on the reach check above.
     - Against the build before the fixes, still starts change by means
       under 0.03 levels (the craft's metal in every scene, wet models in
       the rain; at most 85 levels, a wet palm's edge at Tabuchi,
       `shots/fa1/`).
     - **Cost** at 4K after the fixes, the whole branch (stages 5, 6 and
       9) against main's build (`cd4c27f4d`), the reference settings, three
       pairs a scene, the order alternating, the medians
       (`.tmp/ptfix/evidence/perf-final.txt`): the tracer's GPU frame
       +0.32 ms on Pelagia dry, +0.53 in its rain, +0.38 on its drive,
       +0.51 at Terra Nova and on the lava, +0.72 on world 0's trees
       (trace +0.35, its emitters), +1.12 at Stendahl Alpha (8.5%; denoise
       +0.94) and +1.05 on Tabuchi's deep start with the eye under the
       water (8.9%; trace +0.81): within the budget's 15%. The emitter
       pick's binary search changes nothing measurable where there are
       emitters (worlds 0 and 21, within 0.02 ms, `perf-pick.txt`): their
       groups are small, and it bounds a large one's per-pixel cost.
     - **Checked** (`.tmp/ptfix/evidence/`): sync validation clean over
       Pelagia's drive, Stendahl Alpha, the lava and Tabuchi's deep start
       under SMAA and Ray Reconstruction, FSR with its known
       `rw_luma_history` warning alone, the layer's own log showing its
       synchronization checks on (`validation-final.txt`); 22 of 23 GPU
       tests pass, `vulkan_thermal_vision_shows_heat_in_grey` failing as
       on main (`gpu-tests-fin1.txt`); the 4,411 library tests pass
       (`lib-tests-final.txt`); the raster's frames match main's in ten
       scenes, the rain's puddles and wet ground included
       (`raster-main-vs-final.txt`).
   - **Left for stage 6**, both done 2026-10-29 (below): the anti-lag's
     bias against sparse light (stage 5's note: the frames between rare
     hits read as the light going out; measured with every frame read
     back, a small emitter's light met only by bounce rays settles at 71%
     of its form factor), and a spatiotemporal blue-noise texture in place
     of the golden-ratio interleaved gradient noise.
   - **The anti-lag's bias against sparse light** (2026-10-29,
     `.tmp/ptstage6/`). The test took this frame's spread round a pixel
     (five samples of the lattice) for the noise its short history may lie
     within. Where a light reaches a surface through few of its rays the
     five often all miss it and agree, and the history gave way to its last
     few frames: the frames between rare hits counted as the light going
     out. A model of the temporal pass (`sim.py`, the lattice, the gathers
     and both histories on a flat surface, `sim-antilag.txt`) put the
     settled light at 72% of its mean for a light met by one ray in thirty,
     87% at one in ten and 90% at one in five, the anti-lag test's own wall
     among them (about one ray in five from it meets the floor). Judging by
     the history's own spread instead (SVGF's moments, as ReLAX's temporal
     sigma) mends the bias, but no test of a few frames' mean can then tell
     a light met by one ray in five from its going out: eight frames after
     the light went out the model kept 0.78 of it, against 0.11. What tells
     them apart is the hit probability: how often a sample lies above the
     history's mean (about half for even light, the hit rate for sparse
     light, whose rare hits lift the mean), which the history now keeps
     beside its light (`lag_record`, as its light's mean takes it). A frame
     whose samples round the pixel on its surface all lie below the mean
     (or all above) has, under the history's light, the chance of the
     product of theirs, and a run of such frames the product of all of
     them: its log, the run's evidence, grows each frame the run goes on,
     and past one in a hundred the history begins to give way, by one in
     three thousand to FAST_FRAMES (`lag_test`, `LAG_START`, `LAG_FULL`).
     Five samples were too few for that (one ray in five leaves all five
     dark one frame in three, and the model's wall then kept 0.46 of its
     light eight frames on); five by five of the lattice are enough (all
     twenty-five dark by chance one frame in five hundred for it, two in
     five for one ray in thirty). The workgroup reads that tile of the
     lattice once into shared memory, 81 samples for 64 pixels (where the
     five-sample spread read 15 images a pixel), packed to 8 bytes a
     sample; a pixel reads its three by three, which alone gives the
     record's share, and the ring round it only while every sample so far
     lies on one side (a mixed frame ends the run whatever the rest show),
     the shared reads being the test's cost.
     - **Measured.** The model: sparse light settles at 99.6%, 98.9%,
       99.6% and 99.8% of its mean (one ray in 30, 10, 7, 5); eight
       frames after a light goes out its history keeps 0.20, 0.13 and 0.13
       (one ray in five, in two, nine in ten; 0.11, 0.10 and 0.10 before);
       a light falling to half or to 0.6 of itself follows faster than
       before (0.13 to 0.38 of the step left eight frames on, against 0.19
       to 0.68). On the GPU, with every frame read back: the small panel
       met only by bounce rays settles at 99.6% of its form factor over 64
       frames (main's 65%), 95% over 512 (sampled as a light, 96.9% and
       97.0%), and `vulkan_emitter_sampling_settles_a_lit_patch_sooner`
       now asserts both within 8%; the wall of
       `vulkan_bounce_light_follows_a_change_within_frames` keeps 0.18 of
       its light eight frames after the sun goes out (main 0.19). The newly
       seen wall, lit by one ray in five, settles about 15% brighter than
       on main (`gpu-main-probes.txt`, `gpu-final3.txt`).
     - **Held: the glossy lobe's anti-lag**, which keeps its spread test.
       With the same sign test a panel gliding past a still half-metal
       floor left its reflection 0.29 of the glide behind four frames on
       (`vulkan_glossy_reflection_follows_a_moving_model`, bound 0.22, main
       0.21; no anti-lag at all, 0.31; three by three of the lattice in
       place of five by five, 0.30): the reflection's trailing edge crosses
       the samples, those still on the lit side keep each frame's run mixed,
       and the old test follows it by the very bias it has. So a sparse
       glossy light (a rough metal's reflection of a small one) keeps that
       bias, not measured; a test local to the lobe's footprint would take
       it.
   - **Blue noise** (2026-10-29). The tracer's pairs of numbers (the bounce
     ray's direction, the point of the sun's disc, the emitter's point)
     come from a spatiotemporal blue-noise texture (`blue_noise.rs`): 64
     frames of 64 by 64 pairs, each frame a jittered stratum of every one
     of 64 by 64 cells of the square, laid out by Georgiev and Fajardo's
     swaps under STBN's energy (Wolfe et al., EGSR 2022: by distance on
     screen within a frame, by distance in time at a texel). Made for the
     port, as NVIDIA's own STBN textures come under its RTX SDK licence;
     `writes_the_blue_noise` makes the same bytes again (three minutes).
     Its spectrum (`noise_spectrum.py`): below a quarter of the highest
     frequency, a hundredth to a thirtieth of white noise's power over the
     screen and a tenth to a quarter over time. The path pass and the
     emitters' pass read it by their cell of the lattice, so the samples
     traced each frame lie even over the lattice the denoiser gathers (read
     by pixel, each pixel's own frames in turn, the wall's error after the
     denoiser came out 0.018 against 0.014, `variants1.txt`); each run through
     its 64 frames lays it at another place, as a texture read the same 64
     numbers over and over left each pixel the error of its 64-point
     quadrature (the sparse panel settled 4% lower). The lobe pick stays a
     hash: drawn from the noise too it raised the glossy noise 20%.
     512 KB in a storage buffer, uploaded once.
     - **Noise after the denoiser**, in the GPU tests' static scenes at
       256 by 192 against each scene's 1,024-frame mean with white noise
       (`noise_runs.txt`; a frame's RMS error, then its frame-to-frame
       change, as shares of the light): the wall lit by its floor 0.0258
       to 0.0241 and 0.0126 to 0.0100; the panel met only by bounce rays
       0.109 to 0.078 and 0.039 to 0.026; the panel sampled as a light
       0.0073 to 0.0070 and 0.0038 to 0.0035; the panel's reflection in the
       half-metal floor 0.187 to 0.182, but its frame-to-frame change 0.108
       to 0.114, 6% more. White noise gives 0.038, 0.109, 0.008 and 0.21.
     - **Before the denoiser** (a measurement build tracing every pixel
       and resolving the raw bounce, the same scenes and reference): a
       sample's own spread is the light's, about alike (the wall 2.17 to
       1.91 of the light, the panel 4.25 to 3.91, the sampled panel and
       the reflection unchanged); eight frames' mean and a frame's 4x4
       blocks gather better for the sparse panel (1.39 to 1.33, 1.00 to
       0.91) and alike for the sampled panel and the reflection, and worse
       for the wall (0.52 to 0.59, 0.45 to 0.50): the interleaved
       gradient's pairs form a lattice over a few pixels, which suits a
       light decided by one of the two numbers (which half of the sky a
       ray leaves by). The first frame of a newly seen surface, which
       gathers one frame's samples, comes out as quiet (eight reveals at
       different turns of the noise: 0.0131 against 0.0138 with the
       gradient noise; one reveal alone ran from 1.03 to 1.94 times the
       settled spread with either, so `vulkan_newly_seen_surface_starts_quiet`
       now takes eight).
     - **In the game** (still starts at 1080p, 64 frames from frame 100,
       main's build against this one, `still2.txt`): what the bounce's
       noise moves is a small part of the frame-to-frame change, the rest
       the water, the rain and the sky: Pelagia 0.359 to 0.356 levels a
       pixel, in the rain 0.325 to 0.323, Stendahl Alpha 3.296 to 3.282,
       the lava shore 0.236 to 0.234, Tabuchi in the rain and Caradion's
       trench unchanged; the median pixel's spread over the 64 frames
       0.149 to 0.145 (Pelagia), 0.143 to 0.140 (rain), 0.173 to 0.166
       (lava shore), unchanged elsewhere. The raw-bounce build's in-game
       figures move a few percent either way, under the sun's and the
       scene's motion.
     - **Sharp**: the 64-frame mean's gradient (levels a pixel) is main's
       to the third decimal in all six scenes (Pelagia 2.157 against 2.158,
       Stendahl 0.685 against 0.687), and its fine detail, the mean less
       its 5x5 mean, too (4.035 both, 1.081 against 1.083): nothing
       blurred. The means differ by 0.02 to 0.16 levels on average, the
       sparse light's settled brightness and the noise's own pattern.
     - **In motion** (driving forward from the start, 1080p, main's build
       above this one, frames 160 and 240, `.tmp/ptstage6/motion/`):
       Stendahl Alpha's frames differ in the turrets' metal and the shade
       under them, up to 38 levels in 265 pixels of two million, both
       sharp, the gradient 2.783 against 2.786 and 2.886 against 2.894:
       nothing smeared or blurred. The lava shore's differ by one level at
       most.
   - **Checked** (`.tmp/ptstage6/`): sync validation clean under SMAA and
     Ray Reconstruction over Pelagia's drive, Stendahl Alpha and the lava,
     FSR with its known `rw_luma_history` warning alone, the layer's own
     log showing its synchronization checks on (`validation.txt`,
     `validation/final2/`); 24 of 25 GPU tests pass,
     `vulkan_thermal_vision_shows_heat_in_grey` failing as on main
     (`gpu-tests-commit.txt`); the 4,421 library tests pass, the blue
     noise's own among them (`lib-tests.txt`); Pelagia in the rain gives
     the same frame in 5 runs of 5, the lava shore (its emitters' pass) in
     3 of 3 (`det-pelagia-rain.txt`); the raster's frames are main's in
     Pelagia's rain, Stendahl Alpha and the lava, every pixel
     (`raster-main-vs-branch.txt`). Memory: the anti-lag's record two
     16-bit channels where the short history was one (33 MB more at 4K,
     both frames' images), the noise 512 KB.
   - **Cost** (4K, each stage's least over 400 frames, main's build and
     this one back to back, `perfleast-*.txt`): the denoise 0.08 to 0.16
     ms more in six scenes (with the packed tile and the early stop 0.02
     to 0.23, alike within the runs' spread), 0.05 to 0.09 under DLSS
     Quality; the trace unchanged within 0.14 either way. The pixel's own
     reads moved ahead of the wait for the tile afterwards, not timed on
     their own.
7. [x] **Lit translucents** (2026-10-28), by a cheaper road than the
   compute pass planned: the raster's draws over the traced world take a
   vertex shader of their own (`vk_primitive_lit.vert`) that traces one
   sun shadow ray from each corner (ray queries work in any stage; the
   composite set carries the scene for it), and a half-transparent draw
   (fog mode 1) takes the sun where it reaches and the sky's share where
   it does not, against both, in retail's tones; additive and darkening
   draws are light of their own and keep their colour. Few draws take it:
   the game's effects are mostly additive (fireballs, sparks, dust) or
   darkening (the black chimney smoke, which absorbs light rather than
   scattering it); forcing the light to a quarter changed 174 pixels of
   Terra Nova's start. Validation clean.
8. [x] **Culled models** (2026-10-29), on the shown draw's sandbox
   ([the tracer's models outside the view](../re/PRESENTATION_POLICY.md#the-tracers-models-outside-the-view)).
   Held on 2026-10-28 because the object pass's draw (virtual `+2C`)
   writes game state; since then the view shown draws apart from the game
   on copies of all it could write (`GameAsDrawn`), and the culled models
   draw the same way, last of the world draw, into the feed alone:
   - **Which.** The view shown's pass lists the objects in its radius whose
     `+2C` draws and whose centre its cone turns away; the tracer keeps
     those that can reach the view (`world_scene::reaching_view`): within
     1,024 units of the eye past their extent, mirrored into the view by
     the water's plane, or whose bounds swept along the sun's light down to
     the ground under that line meet the view's frustum. On Pelagia's drive
     that is 1 or 2 of about 55 a frame (`.tmp/fullpt/stage8/`); taking all
     55 changed 115 pixels of a 4K frame by more than 2 levels (scattered
     bounce samples, at most 22) where the kept ones change 3, and on lava
     152 against 59 (`shots/*-diff.png`). An earlier branch took all of
     them (`infestation-pt-culled-models`, review `.tmp/fullpt/result_ae9b4c.md`)
     and cost Pelagia's drive 39% of GPU and 7 ms of CPU.
   - **The first-person hull**: the first
     player's craft in first person draws the same way, its view mode
     cleared on the copy so its draw lays the hull. Its exhausts and wheels
     are dropped with the other overlays, so the mist takes no trails from
     it and the wakes stay the craft's one, as before it was traced.
   - **How they draw.** Their models pass no frustum gate and choose their
     table and far cut-off by distance from the eye, not by view depth
     (negative behind the eye, which took the detailed table past every
     cut-off: the review's finding 2); their raster commands go to an
     arena that is dropped.
   - **In the tracer.** Their own bottom levels (opaque and masked sprites)
     under `MASK_OUTSIDE` (`0x40`): every ray but the eye's, the depth
     pass's and the eye's view on through the water meets them. Their
     records follow their kind's view triangles and each instance's record
     offset holds the view's count, which `hit_surface`, `hit_motion`,
     `hit_emitter` (stage 5's light list, built over the same order) and
     `cutout_solid` add. The earlier branch kept the shaders' lookup and
     padded each bottom level with a degenerate triangle per view
     triangle: 0.18 ms more of uploads and builds on Pelagia, dry (the
     review's measure; its claim that they cost nothing measurable was
     wrong). The offset, it found, moved a level in a few hundred pixels of
     every frame; here frames without such models (Stendahl Alpha, Terra
     Nova, lava, Artoria) differ from main's by at most 1
     level at 4K, within one build's run-to-run spread
     (`shots/*-branch-diff.png`). Its other
     account, that something reads past the feed's vertices, was a
     hypothesis it never confirmed: its NaN placeholder vertex also raised
     every bottom level's `max_vertex`, which can change the driver's BVH
     and so which of two coincident faces a ray meets, explaining the test
     it moved as well; with no placeholders the question is gone. A culled
     face never wins over the view's copy of the same corners
     (`settle_faces`); their sunk glows are left out; the refraction and
     sunk-glow rays from the eye, and its view up through the surface from
     under the water (`water_from_below`), take the view's solids only
     (`MASK_VIEW_SOLID`), as the eye's own rays do.
   - **The radiance cache leaves them out.** Its paths' rays pass them
     (`shade_ray` and `occluded_by` drop `skipped`, `MASK_OUTSIDE`, while
     the path pass feeds the cache) and no path feeds a cell from a hit on
     one (`last_hit`; their instances' custom index carries
     `INSTANCE_OUTSIDE` beside their kind, which the kind's readers mask),
     so the cells hold what the view's own scene gives them, and the
     models outside the view reach it through its hits' rays alone. The
     review found the hull, which moves with the eye, caught in the cells
     round it: a daylight first-person turn showed a darker square a cell
     wide on the ground ahead, 60,000 pixels of the 4K frame by more than
     2 levels against main's; with the cache leaving them out, 109, a strip
     the craft's glow lights (1 with its glows dropped too, an experiment)
     (`.tmp/review-stage8/cache-cell/before-after-c.png`). The other
     experiments: the cache's rays alone passing them left other cells
     brighter than main's, as the hull's hits still fed theirs; their glows
     dropped alone left the square; a cache path's other random draws on
     main moved one faint cell, so the square was the hull's, not noise.
   - **The game.** Unchanged, rebased onto main (`7c345b333`, after
     stages 5, 6 and 9) and checked against its build: a Tabuchi
     replay traced (the remaster's game, one pass) gives main's `[status]`
     RNG and eye lines in all 12, and a 600-frame traced drive in first
     person, firing (the pass on every frame), writes main's event log,
     310,334 events, every random draw, spawn, damage and sound, the audio
     clock's ambience beds aside (`.tmp/fullpt/stage8r/`); before the
     rebase three more runs (the drive in third person, Foldrak Noir at
     night, lava in first person) did too (`.tmp/fullpt/stage8/events.txt`).
     [`the_tracer_draws_outside_the_view_and_leaves_the_game_as_it_found_it`](../../crates/inf-game/src/world_scene_tests/shown_draw.rs)
     pins it, and that the feed holds the object's and then the hull's
     faces and none of their wheels or exhausts. Sync validation clean on
     four runs (the drive in first person at native and DLSS Quality, lava
     in first person at DLSS Quality, Foldrak Noir at night), the layer's
     own log proving synchronization on (`stage8r/validation.txt`). GPU tests: 24 of 25 pass, thermal
     vision's failing as on main;
     [`vulkan_models_outside_the_view_light_it_unseen`](../../crates/inf-game/src/software_device_tests.rs)
     (both record offsets broken in turn failed it, before the rebase).
   - **What it costs** (4K, rebased, against main's build `7c345b333`,
     the two alternating scene by scene, six rounds; a run whose geometry
     stage, the same work every frame, took over 1.15 times its build's
     least on that scene ran on a GPU slowed by other work on the machine
     (every stage 1.4 to 1.9 times), and its pair is left out: medians of
     the 3 to 6 native pairs left and all six at DLSS Quality,
     `.tmp/fullpt/stage8r/perf-*-pairs.txt`): the device's GPU frame, and
     the CPU's (`INF_CPU_PROFILE=1`, at DLSS Quality), in ms.

     | Scene | native GPU | DLSS Q GPU | CPU frame | against the baseline |
     |---|---|---|---|---|
     | Pelagia, rain | 13.48 → 13.70 | 9.23 → 9.30 | 12.23 → 12.43 | −1%, −8% |
     | Pelagia, dry | 12.55 → 12.69 | 8.66 → 8.75 | 11.79 → 12.06 | −4%, −6% |
     | Pelagia, night | 12.18 → 12.33 | 8.48 → 8.54 | 8.21 → 8.44 | −9%, −12% |
     | Pelagia, infrared | 2.44 → 2.44 | 2.47 → 2.47 | 7.78 → 7.76 | −21%, −3% |
     | Pelagia, driving | 10.75 → 10.89 | 7.83 → 7.91 | 9.04 → 9.16 | +8%, −0% |
     | Terra Nova | 12.04 → 12.39 | 8.66 → 8.75 | 8.98 → 9.09 | −18%, −12% |
     | Stendahl Alpha | 14.45 → 14.64 | 9.58 → 9.65 | 7.76 → 7.88 | −22%, −16% |
     | Artoria | 12.54 → 12.58 | 8.75 → 8.80 | 4.85 → 4.86 | −9%, −9% |
     | Foldrak Noir, night | 10.74 → 10.84 | 7.55 → 7.59 | 6.76 → 6.89 | −20%, −14% |
     | Lava | 12.00 → 12.19 | 8.80 → 8.88 | 8.48 → 8.55 | −13%, −7% |
     | Driving, first person | 8.87 → 9.46 | 6.87 → 7.26 | 7.33 → 8.45 | |
     | Pelagia dry, first person | 9.99 → 10.39 | 7.67 → 8.00 | 10.04 → 11.07 | |

     Every scene stays inside the budget (Pelagia's drive at native, +8%
     over the baseline, is main's +6%). Where no model lies outside the
     view (Terra Nova, Stendahl Alpha, lava, Artoria; Stendahl's frame is
     main's to the level) the GPU frame is still 0.5 to 1% dearer at DLSS
     Quality and 1 to 3% at native: the trace stage takes about 0.2 ms
     more from the shaders alone. This build with main's shaders matched
     main; with main's mask values, or without the record offsets' reads,
     it did not (`perf-v2`, `perf-v3`, `perf-v4`), nor did the rebase
     before the radiance cache left them out (`perf-bisect`), so it lies in
     how the driver compiles the larger tracer, not in one of its changes.
     First person, where the hull is drawn every frame: the GPU 6 to 7% at
     native and 4 to 6% at DLSS Quality (its bottom levels' builds about
     0.2 ms, the rays round the eye that meet them about 0.4), and the CPU
     about 1 ms (the pass 0.75 to 0.86 ms, about what the craft's draw
     costs in third person). At DLSS Quality the CPU leads there, so the
     drive's first-person frame goes 7.57 → 8.57 ms (+13%), still cheaper
     than its third person (9.21), and Pelagia's dry first person 10.04 →
     11.07 (+10%). Before the rebase (`.tmp/fullpt/stage8/perf.txt`) the
     same frames were within 1% of main's and first person 4 to 7% dearer.
   - **What shows** (4K, against main's build; both give the same frames
     run to run; the review's table re-measured, `.tmp/review-stage8/rebased/`).
     The hull in first person: at Pelagia's start at night with the lamps
     on, the bonnet and the wheel arches shadow the headlamp pool's edges,
     as third person shows them, 308,000 pixels by more than 2 levels and
     96,000 by more than 8 (at most 45; 341,000 before the cache left it
     out); in a daylight turn (the hover thrusting, frame 300) 109, the
     strip its glow lights. By day its shadow falls within about 70 units of
     it while the view's ground starts about 160 ahead (the Remastered
     focal), and a level mirror shows only what is above the view, so its
     shadow on level ground and its reflection in level water are out of
     view. The models outside the view show little, as the earlier branch
     found: in third person the same turn differs by 1 pixel and the night
     by 162; the suns stand 38 to 74 degrees high (worlds 1 to 24) and the
     models behind the eye are mostly far.
     The check build that showed the outside models to the eye put the
     bonnet where third person has it (`shots/first-person-*.png`), and
     with every model moved under `MASK_OUTSIDE` the palms' shadows and
     reflections and the craft's shadow stay where the view's own cast
     them (`pelagia-every-model-outside.png` against
     `pelagia-third-person.png`).

9. [x] **The water round an eye under it** (2026-10-29; the final
   stretch's item 38, Camera Style's Remastered eye under the live
   surface). The water is a medium there, single scattering in closed
   form (`pt_frame.glsl`'s `water_through`): a ray's light through `L` of
   it keeps `exp(-sigma L)` per channel, `sigma` from the water's colour
   (its strongest channel's 1/e distance `water_murk`, 3,139 units, so it
   hides half where Vulkan's raster's murk does; the others sooner by how
   much less of them the colour holds), and gains the sun's and the sky's
   light it scatters, each dimmed by the water above the point
   (`exp(-sigma depth / cos)` down the refracted beam; the sky's down
   Snell's window, 1.19 times the depth), the integral of each exponential
   taken in closed form. Not an exact model: the sky's light the water
   scatters is unshadowed everywhere (inside a sunk wreck the water still
   glows with it), and so is the sun's along the bounce rays and the
   radiance cache's paths; only the eye's own view takes the sun's with
   its shadows (the grid). Where it lies: the eye's view in the resolve (the sky's
   part) and the mist's grid (the sun's, with its shadow up the beam and
   on to the sun and the ripples' caustics, so its shafts); the bounce
   rays, the radiance cache's paths and the surface's mirrored ray from
   below; the sun and the flash on what lies under the water, down the
   refracted beam through Fresnel's share, absorbed, times the caustics.
   The view from above keeps its tuned look (the clarity's Gaussian over
   the bed): one law for every path, as stage 4 planned, would change the
   water as tuned from above, and waits for a decision. Above the
   water, every frame is the same to the pixel (Pelagia's rain gives one
   of two frames run to run either way): the passes' code for it is kept
   apart, since even reordered statements moved a few pixels a level (the
   fog's gathering, the composite and the water's colour did, on several
   of 14 scenes).
   Review fixes (2026-10-29): the emitters' pass samples an emitter
   through the water as a bounce ray meeting it takes it (`water_way`),
   and the power heuristic weighs only the light of what a bounce ray met,
   not the water's scattered light along it; the raster's effects drawn
   over the traced world take the water between them and the eye
   (`vk_textured_fog.frag`); the wobble shows whole texels, unfiltered,
   with the depth and the fields unwobbled; the surface seen from below
   ripples as the same surface whose slopes bend the caustics.

## Baseline (2026-10-28)

Measured at `d76fa1132` with the reference settings (`settings.json` beside
the release exe: 3840x2160 borderless, path tracing, SMAA, Remaster) and a
DLSS Quality copy, 300 frames each, `.tmp/fullpt/perf/`.

Mean GPU ms a frame (the tracer's own stages; "frame" is the device's
whole submission):

| Scene | 4K native: frame | trace | denoise | DLSS Q (1440p): frame | trace | denoise | upscale |
|---|---|---|---|---|---|---|---|
| Pelagia, rain | 13.8 | 8.42 | 2.40 | 10.1 | 4.47 | 1.20 | 1.62 |
| Pelagia, dry | 13.2 | 7.84 | 2.43 | 9.3 | 3.92 | 1.13 | 1.56 |
| Pelagia, night, lamps | 13.6 | 8.54 | 2.17 | 9.7 | 4.49 | 1.07 | 1.61 |
| Pelagia, driving | 10.1 | 5.33 | 2.18 | 7.9 | 2.83 | 1.06 | 1.57 |
| Terra Nova | 15.2 | 9.68 | 2.67 | 9.9 | 4.44 | 1.22 | 1.46 |
| Stendahl Alpha | 18.9 | 13.30 | 3.00 | 11.4 | 6.00 | 1.41 | 1.46 |
| Artoria | 13.8 | 7.72 | 3.20 | 9.7 | 3.88 | 1.55 | 1.59 |
| Foldrak Noir, night | 13.5 | 8.58 | 2.55 | 8.8 | 3.96 | 1.16 | 1.51 |
| Lava (world 17) | 14.0 | 8.33 | 2.56 | 9.5 | 3.91 | 1.15 | 1.44 |

(Pelagia's infrared scene traces at half size, 2.2 ms, and is left out.)
The rest of each frame: geometry builds 0.6 to 0.9 ms, the sky 0.3 to
0.7, the resolve 0.2 (0.9 under DLSS, which writes its guides), SMAA 0.3,
the composite 0.2 (0.8 on lava). At the reference settings the GPU is
already the limit: the device waits 4 to 13 ms a frame for it.

**The budget**:
- No scene's mean GPU frame may grow by more than 15% over this table,
  at either setting.
- The aim, by the end of stage 4: every scene at most 16.7 ms mean and
  22 ms worst at 4K native with SMAA, and at most 11 ms at DLSS Quality.
  Stendahl Alpha is over that today; stage 3 has to bring it down.
- Rays a pixel today: the primary, the sun's shadow, one per light in
  reach at the primary hit, the bounce and its two shadow rays: about 5
  to 8. Stage 4's plan adds one sun and one light ray at the first
  bounce's specular hit where there is one, and a sixteenth of a pixel's
  longer path for the cache: about 1.5 more on average.

**After stages 1 to 7 (2026-10-28, `276d05a6d`)**, the same ten scenes
at the reference settings, the baseline's exe and this one alternating
(`.tmp/fullpt/perf/`), mean frame ms (the first round; in the second the
desktop's other GPU work slowed both builds by up to half): Stendahl Alpha
19.9 to 13.2, Terra Nova 17.9 to 12.5, lava 16.0 to 12.0, Foldrak Noir at
night 15.0 to 10.7, Pelagia at night 15.9 to 12.2, in the rain 16.5 to
13.5, dry 15.2 to 12.6, driving 13.0 to 11.1, Artoria 15.4 to 12.6. Every
bounce, the traced light, HDR and the BSDFs, and the frame is still 15 to
34% faster: the pacing (stage 3) and the quarter-rate path pass (stage 4)
paid for them.
