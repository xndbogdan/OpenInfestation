# Infestation: Ray Tracing and Upscaling

> **Status: built 2026-10-22.** The whole campaign was played end to
> end in the port (the project objective) and the open
> decisions were settled ([Decisions](#decisions-2026-10-22)). Code anchors below were
> read on 2026-10-20 and may have moved.
>
> Built, per the sections below:
> - the Vulkan device, path tracing with its own denoiser, FSR, DLSS, Ray
>   Reconstruction and XeSS;
> - water, the painted sky in reflections, fog and mist, thermal vision,
>   metals, and shadows from retail's lights;
> - the tuning file, and the settings rows in the launcher and in game;
> - anti-aliasing without ray tracing, the briefing's camera pane, and the
>   kept NGX answer.
>
> Current work (2026-10-22: what was optional becomes the current thing to
> deal with), after the queued reports (the project objective):
> - **full path tracing** (2026-10-22: the whole point of path tracing is to
>   replace retail's lights and path trace everything; 2026-10-28: document
>   it, then implement it properly): what is built is one bounce
>   over retail's lighting, capped at 1; the plan of record for the rest
>   is [rt-plans/full-path-tracing.md](rt-plans/full-path-tracing.md)
>   ([below](#full-path-tracing));
> - a ray-traced frontend: the menu's backdrop models are traced wherever
>   Vulkan path traces (2026-10-27; built as a limited test 2026-10-23),
>   the sun and planets done, the logo as a light still to come;
> - the modern raster mode, which is where a heat image without ray
>   tracing would come from (left for later; then
>   2026-10-28, its reflections, water and sky:
>   [rt-plans/modern-raster.md](rt-plans/modern-raster.md));
> - model detail (held: no model has shown the need);
> - HDR output, frame generation and Linux, whose scope is still open.
>
> Open: whether Ray Reconstruction stays in the Upscaler row. FSR 4
> waits for AMD's Vulkan library; no DirectX 12 bridge (decided
> 2026-10-23, [FSR 4](#fsr-4)).

## Goal

An opt-in Vulkan renderer that upgrades Infestation's visuals: a plain Vulkan
renderer first, then path tracing (lighter ray tracing modes only if path
tracing is too slow on weak cards), with DLSS, FSR, or XeSS upscaling.
OpenV2K is out of scope for now; Infestation goes first.

## Decisions (2026-10-22)

- **Performance:** no weak card to test on yet. Measure GPU time per pass
  on the RTX 5080 and keep a wide margin there; weak cards are estimated
  from it until one is available.
- **Cards without ray tracing** get plain Vulkan (retail's look) with the
  upgrades that need no ray tracing: the sky, thermal vision, upscaling. A
  modern raster mode (modern lighting, shadow maps, screen-space water
  reflections) is an optional last objective.
- **In-engine scenes outside gameplay** get the upgraded look too: the
  intro, the ending, the briefing's 3D view. Menus and the HUD stay
  retail's 2D. **The briefing's camera pane is traced since 2026-10-22**
  ([notes](re/PIXEL_RUNTIME.md#modern-mode-the-traced-world));
  the intro and the ending were already, since they draw the world itself.
  The briefing's map and wireframe globe are 2D panels and stay retail's.
- **Upscalers:** DLSS (with Ray Reconstruction), XeSS and FSR. Each loads
  at runtime as an optional library; a missing one hides its option.
  Publishing is decided later ([licenses](#third-party-upscalers-and-publishing)).
- **API:** raw Vulkan through `ash`. DLSS, XeSS and NRD take raw Vulkan
  handles, and wgpu's ray tracing is still experimental.
- **Later, optional:** HDR output, frame generation, Linux.
- **The sky was a hybrid** (chosen 2026-10-22, after the first cloud
  pass). Retail's painting stays; drifting clouds fill
  the open sky above it in its own colour, and cast moving shadows on the
  ground. **Rejected 2026-10-23** (the separation between painting and
  clouds was too obvious; the whole sky is done, case by case):
  the renderer will make the whole sky, world by world
  ([plan](rt-plans/sky-per-world.md)). There is no Painted/Modern
  setting: OpenGL keeps the painting, and Vulkan with path tracing gets
  the modern sky.
- **A ray-traced frontend starts as a limited test** (a more limited, lower fidelity
  test first), to be judged before the full one.
- **The modern raster mode waits** (left for later).

## Ground rules

- **Retail fidelity lives in OpenGL and software.** OpenGL stays the default
  renderer. The CPU raster stays the pixel reference, and harness probes keep
  using `--renderer software`.
- **Vulkan diverges on purpose** (2026-10-20). It exists to upgrade the
  visuals; matching retail is not a goal there beyond phase 1's bring-up
  check. Upgrades that don't need ray tracing apply with ray tracing off too.
- **Vulkan renders things its own way** (2026-10-22): it is an
  upgraded engine, so retail consistency is not a concern. Nothing should
  become unrecognizable, but something can be rendered differently if
  quality calls for it; the whole point of the Vulkan port is to bring the
  game into the current decade. Where
  retail's way of drawing something looks poor under modern lighting (a
  light without a direction, a texture trick, a flat effect), Vulkan draws
  it the way that looks best and keeps it recognizable. The game's state
  and rules stay retail's.
- **Game state is read, never written.** Every tap that feeds the ray tracer
  copies state the game already produced. Drawing a frame consumes the
  world's random numbers (the camera lookup and the scene's draws,
  2026-10-22), so every renderer runs the game's own draw pipeline each
  frame; modern mode discards its screen-space output rather than skipping
  it. Which view that draw takes is the session's game (2026-10-28): software, OpenGL and network games play retail's, the view
  shown drawn apart from it; Vulkan single player plays the remaster's,
  whose Remastered camera, widescreen frame and (in a session that started
  traced) longer draw distance drive it
  ([the game's draw and the shown draw](re/PRESENTATION_POLICY.md#the-games-draw-and-the-shown-draw)).
- **Off-screen geometry comes from a side-effect-free pass.** The game runs
  only the models it draws, but shadows and reflections need nearby models
  outside the view. Those run in a separate pass on copies of their state,
  with a scratch random-number source, so the game never sees it.
- **Vendor-neutral first.** Ray tracing uses Vulkan's cross-vendor extensions
  (`VK_KHR_acceleration_structure`, `VK_KHR_ray_query`). It runs on NVIDIA
  RTX 20+, AMD RX 6000+ (RDNA2), and Intel Arc. Vendor features (DLSS,
  Ray Reconstruction) are optional extras on top.
- **As light as possible.** The goal is to run well on the weakest card we can
  reach; the actual floor is set once there are measurements. Cards without
  ray tracing support still get plain Vulkan and FSR.
- **Windows first, Linux as far as possible.** Infestation targets Linux, so
  nothing here is Windows-only where a portable option exists. Vulkan, FSR,
  NRD, and DLSS should all run on Linux; verify at the start.
- **Retail fog and draw distance stay** as the plan's default. They bound
  how far rays travel and how much of the world is in the scene, which
  helps performance. The fog's look is modernised ([Fog](#fog)). Since
  2026-10-25 a world's tuning may draw further under the tracer (`fog.far`,
  `fog.past`); the objects that
  reach admits play in a session that started traced and only draw in any
  other.

## Settings and menus

- Renderer labels make the split explicit (2026-10-20): Automatic,
  **OpenGL (Vanilla)**, **Vulkan (Remastered)**, Software. The Vulkan row was
  "Upgraded" until 2026-10-23, when it was renamed: what it adds, in
  picture and sound, is a remaster. Automatic keeps its
  meaning, OpenGL with the CPU fallback, so the default stays vanilla. The
  launcher's renderer row (`launcher/native.rs` `RENDERERS`) has an
  explainer. **Built 2026-10-22** as the first lines of the Options window's
  Vulkan group, since the Display group's right column already holds its own
  note, since 2026-10-29: "Vanilla (OpenGL) draws Infestation as the
  original did. Remastered (Vulkan) adds traced light and shadows,
  reflections, water, fog and thermal vision; these rows apply to it only.
  Ray tracing switched in game reaches the game itself at the next session."
  (The session's game is fixed when its window opens,
  [the game's draw and the shown draw](re/PRESENTATION_POLICY.md#the-games-draw-and-the-shown-draw).)
  A PrintWindow of the launcher's own Options window at 125% scaling showed
  the earlier note in three lines, with every row below in place; the
  current one measures three lines at 100% and 125% in its font (GDI
  `DrawText`), not yet seen in the window.
- Ray Tracing and Upscaler rows are greyed out unless the renderer is Vulkan.
  A greyed row says why ("Needs Vulkan", "No hardware ray tracing on this GPU").
- **Ray Tracing:** Off / Path Tracing, plus lighter modes only if path tracing
  proves too slow on weak cards (decided after measuring). Offered only when
  the device reports `VK_KHR_ray_query` and `VK_KHR_acceleration_structure`.
- **Upscaler:** Off / DLSS / FSR / XeSS, each with Native AA, Quality,
  Balanced, Performance. DLSS appears only when NVIDIA's DLSS library loads
  on an RTX card; FSR is shader-only and always available.
- **Ray Reconstruction:** shown only with DLSS plus Path Tracing.
- Upscaling works without ray tracing (Native AA mode as anti-aliasing for the
  ordinary image). **Done 2026-10-22**, with the Anti-aliasing row: see
  [anti-aliasing without ray tracing](#anti-aliasing-without-ray-tracing).
- A saved choice the current GPU cannot run (DLSS on an AMD card) drops to the
  best available option and shows it. Startup never fails over it. **Done
  2026-10-22** in game, and in the launcher's Vulkan group as before:
  - The device says once what it runs.
  - The Upscaler row offers only those upscalers, and a saved one the GPU
    cannot run shows as the next best (DLSS as FSR on an AMD card) while it
    stays saved.
  - Ray Tracing reads "Unavailable" without hardware ray tracing.
  - The launcher still lists every choice, since it opens no device.
  - Without ray tracing (the GPU's or the player's) an upscaler brings the
    raster's world up from its quality's size, so Upscaling moves as it
    does traced (since 2026-10-23, whatever the ray tracing option; before, the
    raster ran at the native size only and the row was locked at Native
    AA). Ray Reconstruction, with no traced light to denoise, runs as DLSS
    and leaves the row.
  - With the Upscaler Off there is nothing to scale (2026-10-22):
    Upscaling reads Off in game and does not move, and the launcher greys
    it. The saved quality stays for when an upscaler runs again.
  - NGX's answer about DLSS's device extensions, 0.6 s of each Vulkan
    start, is now kept ([first-run preparation](#first-run-preparation)).
- Changing Ray Tracing or Upscaler settings in game rebuilds Vulkan resources
  only. Switching between OpenGL and Vulkan recreates the window: try doing it
  live in game; if that proves hard, the switch applies at the next launch.
- Rows go in the launcher and in game. Retail's Video menu has no renderer row
  (`presentation.rs`), so the in-game rows are a deliberate
  departure from retail.
- **Filtering** (2026-10-22; **done** the same day) replaces retail's "Bilinear Filtering"
  row (settings `+3C`, `frontend_video.rs`). Saved values keep their meaning:
  0 Point and 1 Bilinear, which are all OpenGL and Software offer, as
  retail. Vulkan adds Trilinear and Anisotropic 2x, 4x, 8x and 16x, capped at
  the device's limit. The row is in the pre-game launcher too. On Software
  and OpenGL a saved Vulkan choice stays saved but draws, and shows, as
  Bilinear (2026-10-22), so switching back to Vulkan finds it again.
  Changing the row there starts from Bilinear. The traced world already samples sprites
  from their own mip chains (`PIXEL_RUNTIME.md`, modern mode), so the setting
  picks its sampler. The Vulkan raster's 256x256 texture pages need
  per-sprite mips before their draws can follow.
- **Detail Level** starts High (2026-10-27). Retail's authored
  `+70` is Low, so fresh installs and Restore Defaults use the fine ground
  (`DETAIL_DEFAULT`, `frontend_settings.rs`); a saved Low stays Low, and the
  Video row and Alt+F3 still toggle it. It is a gameplay setting, not a
  remaster visual: the same default applies on every renderer. Worlds whose
  H14 flag `0x2000` forces the coarse ground keep it.
- **Port text in every language** (2026-10-22; **done** for Filtering
  and Borderless, `port_text.rs`). Each string the port
  adds to the game's own menus comes in all six retail languages (settings
  `+64`: English, Français, Deutsch, Italiano, Español, Nederlands, whose
  strings are `0l0.ovl` to `0l5.ovl`). It is encoded as the retail strings
  are, so the game's font draws the accents, and follows an in-game
  language change. That covers Filtering's label and values, the new rows
  above, and the existing Display row's "Borderless", which today shows in
  English whatever the language.

## What exists today

| Piece | Where | Use for ray tracing |
|---|---|---|
| Backend boundary | `SoftwareFrame` (`software_frame.rs`) holds `device: Option<GlFrame>`; draws either rasterize on the CPU or go to OpenGL | Becomes a backend trait; Vulkan is a second device |
| Window and backend choice | `frontend_app/display.rs` builds an OpenGL or CPU window at launch | Add a Vulkan window (SDL `.vulkan()`); in-game swapping is new work |
| What backends receive | Screen-space `D3dTlVertex` (`d3d_viewport.rs`): projected x, y, depth, 1/w, baked shade | Not enough to trace: no world space, nothing off screen |
| World-space model vertices | `world: Option<[i32; 3]>` on each slot (`model_vertices.rs`) | Plain world units, set only for the world range: the feed reads a view-space side table instead (phase 2) |
| Per-model transform and camera | `ModelProjectionBackend` (`model_projection.rs`) | Instance transforms and the camera |
| Model normals | `shade_normal` takes `[i16; 3]` (`model_lighting.rs`) | Model lighting |
| Sun | `ModelLighting`: direction (`0055F2A0`), colour (`0055F254`), ambient | Directional light; ambient becomes sky brightness |
| Point lights | `TerrainLightRequest` (`world_lighting.rs`): X/Z, radius, RGB from the player, particles, entities | Explosion and projectile lights; each needs a height (terrain height plus an offset) |
| Terrain | `terrain_map` heights; `terrain_draw` emits projected quads | Build the mesh from the heights; normals from the height field |
| Water | `terrain_draw/water.rs` (material `0x74`, infrared variant, 16 shoreline tiles); `TerrainWaterSurfaceRequest` (`terrain_runtime.rs`) with wave clock and world flag `0x40` for spatial waves | See [Water](#water) |
| Tick rate | `EXACT_WORLD_TICK_MICROS = 16384` (~61 Hz) | Good rate for temporal upscaling and denoising |

## Phases

1. **Vulkan draws what OpenGL draws.** Backend trait over `SoftwareFrame`'s
   device, a Vulkan device, the launcher option, the greyed rows, and the
   [first-run preparation](#first-run-preparation) pass with its cache and
   loading screen. From there Vulkan diverges on purpose, and it stays usable
   with ray tracing and upscaling off. *Done when* every screen renders
   properly (2026-10-22: no pixel match with OpenGL is needed).
   **Done 2026-10-22**: the shared [frame device](../crates/inf-game/src/frame_device.rs)
   and the [Vulkan device](../crates/inf-game/src/vk_device.rs)
   ([notes](re/PIXEL_RUNTIME.md#vulkan-device)), and the
   launcher's "Vulkan (Remastered)" row. The greyed rows wait for what they
   control (phase 3), and the preparation pass for what it prepares (smooth
   normals and bevels, materials and the pipeline cache).
2. **World-space scene feed.** Read-only taps at model projection and terrain
   emission collect each frame's triangles (texture, UV, normal), the sun, and
   the point lights. *Done when* every 3D draw in a world sweep appears in the
   feed, and software frames are unchanged. **Models done 2026-10-22**
   ([modern mode](re/PIXEL_RUNTIME.md#modern-mode-the-traced-world)):
   a view-space side table on the vertex slots (the `world` field below is
   absent for most drawn vertices, and is not fixed point), the shade
   slots' world normals, and one tap per primitive instruction. **Ground
   and dynamic lights done 2026-10-22**: the whole terrain grid as one mesh
   with retail's tiles, lit through a per-vertex albedo from the baked
   shade; retail's light grid as the frame left it. Point lights with
   positions of their own (for shadows) wait on the grid's sources.
3. **Modern mode with path tracing.** Vulkan draws the 3D world from the scene
   feed; HUD, menus, and 2D overlays still come from the screen-space stream
   and are composited on top. Path tracing (sun and glowing lights, bounce
   lighting, water), a denoiser (NRD on any GPU, Ray Reconstruction on
   NVIDIA), and the upscaler choice. *Done when* it has been measured on weak
   cards. **Measured 2026-10-22 on the RTX 5080** (measured
   there and inferred): the primary ray and the traced sun shadow over models
   and the whole ground take 1.3 to 1.4 ms a frame at 3840x2160, with the
   validation layers on; the first frame (ground build, texture uploads)
   about 5.5 ms. **Bounce lighting and our own denoiser done 2026-10-22**
   ([notes](re/PIXEL_RUNTIME.md#modern-mode-the-traced-world)):
   one bounce a pixel with its own sun shadow, a temporal pass and an
   edge-aware a-trous filter; 3.7 to 4.3 ms a frame at 3840x2160 on the
   5080.
4. **Lighter modes, only if needed.** If path tracing is too slow on weak
   cards, add a cheaper mode over the normal image: ray-query sun shadows,
   shadow-casting explosion light, and water reflection and refraction.

## Water

Water is the biggest single win, and most of it lands in phase 3.

**First pass done 2026-10-22**
([notes](re/PIXEL_RUNTIME.md#modern-mode-the-traced-world)):
the water within sight as a mesh at the game's own wave heights each frame,
reflections, refraction with depth colour, ripples and the sun's glint, in
retail's per-world colour. The sky circle seen from below renders: from
under the surface the sky and the sun show through Snell's window, and
outside it the water mirrors the scene below (seen on world 3,
2026-10-22). Still to come: rough light. **Caustics done 2026-10-22**,
faked from the ripples' curvature
([notes](re/PIXEL_RUNTIME.md#modern-mode-the-traced-world)). The water's "night" material is
infrared vision's, not a night world's (`terrain_draw::night_material`):
dark worlds reflect their own sun and, since 2026-10-22, their lamps.
Explosions and lamps now glint in the water from their
deposited lights (2026-10-22), and splashes ring it. **Splashes done
2026-10-22**: the feed reads retail's live splashes, and the water shows
them as rings spreading where each fell and foam that fades, while the
sprites stay retail's.

- **Reflections** that mirror sky, terrain, vehicles, and buildings at shallow
  angles and turn see-through looking straight down.
- **Refraction with depth colour.** Depth is water height minus terrain height
  (`terrain_height` is already in the request), so shorelines shade themselves
  where retail uses 16 shoreline tiles.
- **Explosions** light the surface and ripple in it.
- **Underwater**: the bright circle of sky seen through the surface from below.
  Test with the F12 underwater recipe.
- **Night worlds** reflect moonlight and scene lights.
- **Splashes** (2026-10-22). Retail's are process particles: splash
  `004893A0`, born where debris, a projectile or a falling drop meets the
  water and by selector 65 as the vehicle leaves it fast, and the impact
  bubble `00528814` for crossings and buoyancy
  (`PARTICLE_RUNTIME.md`, `ACTOR_RUNTIME.md`), drawn as blended sprites that
  stay on the raster for now. Their births, positions and lifetimes stay
  retail's, since they are game logic and draw on the random numbers. Only
  the look changes: lit, refracting spray, a ring ripple in the traced
  surface at each one, and foam that fades. This needs a tap on the
  particle draw (position, size, age and class).
- **Blasts in the water, done 2026-10-22** (grenades exploding in
  water should make it splash out). A grenade that sinks before going off
  showed only a faint flash: retail's water is translucent and shows the
  fireball through it, the traced water does not. Now the water erupts:
  a flash under the surface, a dome, rings and foam, and above it jets of
  white water and clear drops traced as drops of their own, which ring the
  water where they fall back
  ([notes](re/PIXEL_RUNTIME.md#modern-mode-the-traced-world)).
  The first, volumetric version was too gaseous; spray is
  drops that glint and fall fast, as in games' splashes (refraction-shaded
  sheets and droplet particles, not soft volumes).
- **Reflective water and clarity by world, done 2026-10-22** (water should
  have some reflectivity, especially on levels with dirty water that one
  cannot see well through; some levels have clearer water and some
  dirtier). The tuning file's `water` takes
  `clarity` (how far one sees into it, world units) and `reflect` (the
  least share of its surroundings it mirrors, looking straight down;
  water's own Fresnel term is 0.02), and `worlds` sets them per world.
  The worlds' water textures fall in six families, picked as:

  | Water | Worlds | `clarity` | `reflect` |
  |---|---|---|---|
  | Clear teal | 3, 13 | 420 | 0.04 |
  | Clear teal, snow world | 23 | 650 | 0.04 |
  | Paler grey | 1, 9, 12, 15, 18 | 260 | 0.06 |
  | Toxic yellow-green | 5, 6, 22 | 70 | 0.2 |
  | Red liquid | 0, 19 | 45 | 0.15 |
  | Lava | 17, 24 | 12 | 0.03 |

  The global values (160, 0.02) stay what all water took before. Since
  2026-10-25 the bottom fades by `exp(-(t / 1.5 clarity)^2)` of the
  refracted ray's length `t`, clearer over the shallows than the plain
  `exp(-t / clarity)` it replaced
  ([notes](re/PIXEL_RUNTIME.md#modern-mode-the-traced-world)). Worlds
  2, 4, 7, 8, 10, 11, 14, 16, 20 and 21 have no water: their plane lies
  below all their ground.
- **Lava, done 2026-10-22; rebuilt 2026-10-27** (found on the way; 2026-10-27:
  scrapped and made anew, a better one): Shak'tahh (17) and Caradion (24) use the water's tile for lava,
  which the tracer mirrored and refracted as water. The tuning's
  `water.glow` marks the worlds whose water is lava. The melt glows in a
  blackbody ramp around the retail tile's hue under a cracked dark red
  skin; basalt plates, lit as rock, crowd the banks and drift as rafts over
  the deep, their cracks glowing and seen with depth; upwellings push them
  aside; sub-pixel detail resolves to its average. The lake lights walls,
  ground and vehicles through the bounce rays that meet it, its orange kept
  ([notes](re/PIXEL_RUNTIME.md#modern-mode-the-traced-world)).
- **Repetition** (2026-10-22; **done** the same day,
  [notes](re/PIXEL_RUNTIME.md#modern-mode-the-traced-world)).
  A large body of water looks the same everywhere, a visibly repeating
  pattern. Near the vehicle
  the surface stays at the game's own wave heights, since floating things
  ride them. Further out, nothing reads those heights, so the detail can
  vary: ripple sets whose direction, scale and strength drift with position
  (a cheap noise field), with the waves' amplitude modulated the same way,
  at little cost. As built, the heights stay the game's everywhere, since
  other floating things ride them as well: beyond 1,500 units only how
  much of the swell shows in the shading varies.

Needs:

- A denser water mesh sampled from the game's own wave function, so
  reflections move in step with floating vehicles, plus a small-ripple detail
  texture (modern-mode only; retail has nothing at that scale).
- Water as a clear material with index of refraction 1.33 and a per-world
  absorption colour taken from retail's water texture, so each planet keeps its
  water colour.

Cost: mirror reflections are cheap and nearly noise-free; refraction and rough
reflections need the denoiser; caustics are faked (a pattern animated from the
wave function) or get their own pass.

## Sky

**Retail** (`GAMEPLAY_RUNTIME.md`): each gameplay frame is cleared with the
world's sky colour (H7 slot `0x72`). The `backdrop` model (H8 `0x116`), a ring
of textured quads open at the top (57 vertices in `11.ovl`), is drawn at the
eye's X/Z and the world's backdrop height; the clear colour fills the sky
above it. Other sky-like models exist (`spacegradient`, `planetsun`, `moon01`,
`stars01`, `saturnrings`, `cloud`); where they appear is unchecked.

**Vulkan** (2026-10-20): a modern sky per world, the high-fidelity
option. A physically based atmosphere computed from the sun's direction, so
sky colour, sun disc, and shadows always agree, with per-world settings
(tint, density, haze, clouds, or none for airless worlds). Retail's sky colour
and backdrop textures are inspiration only, not drawn as they are. One
direction is the truth: if retail's visible sun and its light direction
disagree, align them, or shadows come from where the sun isn't.

**For now** (2026-10-22): modern mode shows retail's backdrop and sky colour
behind the traced world (the composite leaves the pixels its rays miss to the
raster's sky), so each world keeps its painted sky until the modern one is
reviewed. Reflections and the bounce light still see the sky as one colour.

**A first modern sky to review, 2026-10-22**
([notes](re/PIXEL_RUNTIME.md#modern-mode-the-traced-world)):

- **Where it lives.** The tuning file's `sky`, everywhere or per world, is
  off by default.
- **What it draws.** The world's own sky colour overhead gives way to its
  far terrain colour at the horizon, where the haze meets it. The sun's
  glow and disc stand exactly where the world's light comes from, and
  stars can show where it is dark. Reflections see it too.
- **What the review shows.** A sweep of all 25 worlds beside retail's
  painting found it plainer everywhere. Without clouds it loses each
  world's painted mood: world 0's pink clouds, world 1's golden ones,
  world 4's red sunset. So the paintings stay.
- **Next.** Clouds before any world switches. **A first pass, 2026-10-22**,
  a drifting noise layer over the eye, was still plainer than the
  paintings. **The sky plan's base, 2026-10-23**: volumetric clouds marched
  at a quarter of the display's size with a history, a sky cube map for
  reflections, and a cloud shadow map; the tuning's `sky` block now holds
  `style`, the colours, `sun`, `stars` and `clouds`
  ([notes](re/PIXEL_RUNTIME.md#modern-mode-the-traced-world),
  [plan](rt-plans/sky-per-world.md)). Every world stays painted until its
  recipe is approved.
- **The hybrid, 2026-10-22, removed 2026-10-23.** The clouds filled the
  open sky above retail's painting and seamed against it. They are gone:
  the painted sky is retail's again, in the view and in reflections. Their
  drifting shadows on the ground, models and water (`cloud_shadow`) stayed
  for the day; since 2026-10-23 clouds and their shadows come only with a
  world's modern sky, now Stage 2
  ([notes](re/PIXEL_RUNTIME.md#modern-mode-the-traced-world)).
- **The painted backdrop in reflections, done the same day.** Water and
  metal mirror retail's painting where they mirrored one colour. The
  painting keeps its mood, so the plan's reflections need not wait for
  the modern sky.

**Workflow:** for each world, collect retail's sky colour, backdrop textures,
sun direction and colour, and fog colour; propose modern sky settings from
them; each world is reviewed against retail captures (harness
`--start` and `--snapshot`) and signs off each one. The settings live in the
tuning file.

## Fog

**Retail**: a flat fade to the world's fog colour between two distances
(`far_color`, `fade[0]..fade[1]` in `world_scene.rs`). It hides the draw
distance: the game submits no objects beyond `fade[1]`.

**Vulkan** (2026-10-22): volumetric fog in place of the flat fade,
built with the sky, since the atmosphere and the fog are one model.

- **Lit by the scene.** Sunlight scatters in the fog, and terrain and
  buildings shadow it, giving light shafts. Explosions, projectiles and
  headlights glow in it.
- **Height.** Thicker low down and thinning with altitude, so valleys and
  water hold mist and hilltops stand clear.
- **Aerial perspective.** Distance takes the sky's colour, brighter toward
  the sun.
- **Per world.** The colour comes from retail's fog colour; density and
  height live in the tuning file and are reviewed world by world with the
  sky.

It must still be opaque by `fade[1]`, or objects beyond it pop in. Cost:
march a low-resolution grid aligned with the view, shadowed through the
acceleration structure, and accumulate over frames.

**First pass done 2026-10-22**
([notes](re/PIXEL_RUNTIME.md#modern-mode-the-traced-world)):

- **Haze.** A haze over the ground and the water replaces retail's flat
  fade. It is complete where retail's far terrain ends (1.5 times the fog's
  end), so distant mountains stay in sight as they do in retail, lit and
  shadowed. Its colour turns to retail's far colour past the fog's end.
  Models keep retail's fog, so nothing pops in.
- **Mist.** A mist lit by the sun casts shafts and thins with height above
  the world's low ground, and retail's lights glow in it. It costs about
  0.1 ms a frame at 4K.
- **Tuning.** Its values are in the tuning file, per world if need be, and
  the painted sky takes half of the mist.
- **Still to come:** the review world by world with the sky, and headlights
  and projectiles, which glow only through retail's light grid for now.

**The distance fog, 2026-10-23**:
the mist's grid carries one fog for a world's whole view, complete at the
fog's end near the ground, thinning above a per-world top (its land's
high ground); the haze and retail's vertex fog leave the traced surfaces,
the sky takes the whole volume, the raster's late draws read the same grid
at their depth, what lies past the fog's end fades out by 1.6 times it,
far models dissolve before the fog's end, and the rays reach a fixed
distance. The painted sky takes all of the volume now, not half of the
mist.

## Infrared Vision

**Retail** (`GAMEPLAY_RUNTIME.md`, `VEHICLE_DRIVE_RUNTIME.md`): with the
upgrade (`0x3D`) owned, L toggles it (otherwise L toggles the headlights).
The frame clears to black with no sky, the world tint turns (0, 127, 0), a
green light follows the player, and terrain and water take a night colour
(the green look). The port draws it as retail does
([infrared vision](re/GAMEPLAY_RUNTIME.md#infrared-vision)).

**Vulkan** (2026-10-20): a real thermal camera instead of green.

- Temperature per material, a column in the tuning file: engines, turbo
  exhaust, weapons, explosions, and fire hot; mutants and creatures warm;
  metal hulls near ambient (warmer near engines); terrain, rock, and water
  cold; sky coldest.
- Black and white, white-hot, with automatic gain: each frame maps the
  scene's temperature range to black through white (smoothed), so hot things
  stand out.
- Subtle, tunable camera character, like an uncooled thermal camera:
  per-frame grain (strongest in flat, cold areas), faint fixed column/row
  stripes, a deliberately low internal resolution (authentic and cheaper on
  weak cards), edge sharpening that keeps outlines readable, halo around hot
  objects, a brief gain reaction when an explosion appears, light vignette,
  at most very light motion smear. The periodic shutter freeze is off by
  default. All of it runs after upscaling and denoising, at output
  resolution, so DLSS/FSR don't remove or smear the grain. Grain must never
  hide a small enemy at range.
- Works without ray tracing; path tracing adds hot objects reflected in
  shiny metal.
- Whatever retail's infrared lets the player spot must still stand out.
- Later: heat that changes over time (explosion sites and scorched ground
  cooling, gun barrels warming while firing, wrecks cooling).

**First pass done 2026-10-22**
([notes](re/PIXEL_RUNTIME.md#modern-mode-the-traced-world)):

- **Heat.** Each traced surface has a heat: the sky, water and ground by
  kind; each model by its name, through patterns in the tuning file; sun
  warming. It is shown white-hot over a range that follows the scene.
- **Camera.** It renders at half size, and its character (grain, fixed
  stripes, sharpening, halos, vignette) is laid on at the camera's own
  resolution after the world's last draw, before the HUD.
- **Gaps.**
  - The raster's blended effects and billboards (Terra Nova's start
    beacons among them) show by their brightness, not by heat.
  - Retail's dynamic lights warm nothing by default: under infrared they
    are all green, retail's lamp on the player included.
  - Without ray tracing the camera greys retail's infrared image, which is
    brightest where that lamp shines rather than where it is warm. Real
    heat there would need the raster to carry each draw's heat.
  - Reflections of hot things and heat that changes over time are still to
    come.

**The camera reworked, 2026-10-23**
([plan](rt-plans/thermal-vision.md), steps 1 and 2): the gain from a
histogram without the sky, no upscaler under infrared, a 1,280-pixel
sensor with detail enhancement, white hot, and ironbow by a press of L (no
menu row: a row looked sloppy). Its amber tint over body heat went
the same day: white hot is black and white. Then heat by
object (step 4): moving things run warmer, shots warm the guns that fired
them, wrecks and corpses cool from the heat they had, and an attachment
takes its object's heat.

## Materials and light sources

### Materials

The 760 model names (`extracted/models/manifest.json`)
already separate materials:

| Material | Example models |
|---|---|
| Robotic / metal | `headrobotic`, `headcyborg`, `metmantarm`, `metmid`, `mantron`, `hoverbot`, `tankbot`, `zifbot`, `gun*`, `towergun`, `pipe*`, `silo*`, `truck*`, `acam` |
| Organic | `headorganic`, `mutant*`, `mut*` limbs, `gryf*`, `eyeball`, `brain*`, `entrails`, `rhino*` |
| Crystal | `xclump01-06`, `xshard01-03` |
| Plants (normal / infected) | `cicad*`, `fern1`, `flower1`, `virused*` |

Granularity, in order: one material per model name, then one per texture
(shared textures carry the material across models; not yet checked whether
textures have names), then per-pixel masks (texture-pack work, later).
Metal is moderately rough, never mirror-like. Textures with painted-in
highlights may need them removed on metal.

**Metals by model name done 2026-10-22**
([notes](re/PIXEL_RUNTIME.md#modern-mode-the-traced-world)):

- **Which models.** The tuning file gives each model a metalness and a
  roughness by name, as thermal vision gives heat: the vehicle's forms,
  robots, guns, ships and craft, and more lightly the industrial structures.
- **How they look.** A metal's bounce ray follows its reflection, blurred by
  its roughness, as often as it is metallic. It keeps the rest of its
  painted diffuse light and takes the sun's highlight.
- **How much shows.** The effect is subtle, because retail's light cap
  (`normalized_shade`) holds sunlit surfaces at full light either way, and
  the mostly one-coloured worlds reflect much what they light. A mirror
  test shows the reflections working, the ACAM in a bunker's face. Crystals
  and plants keep no material yet.

### Light sources

In path tracing any glowing surface is a light, so most sources need only an
emissive material, not a light record.

| Source | Retail (RE notes) | Modern mode |
|---|---|---|
| Plasma, laser | No light: the bolts' animation frames hold none, and the laser's beam is drawn in screen space | Each bolt and beam lights what it passes, in its sprite's colour (done 2026-10-22) |
| Cannons | Machine guns; not in the light table | Brief warm muzzle flash |
| Detonator / charge (selector 27) | Small grey-white light | Keep |
| Missiles / rockets (selectors 25, 28) | Red ground glow within 1500 of the terrain (`OBJECT_LIGHTING.md`) | Glow like retail to start; tuned by eye |
| Flamethrower (selector 76) | Red-orange light | Keep |
| Grenade, mine (selectors 26, 38) | Grey light | Keep |
| Vehicle | Grey headlamp; rear glow red in gear 1, amber in gear 0, **off in the boost gear** (`VEHICLE_DRIVE_RUNTIME.md`) | Keep, but the boost gear glows too (see turbo; done 2026-10-22). The headlamp is two lamps on the nose with a shaped, shadowed beam under path tracing in place of retail's pool (done 2026-10-24, [the headlight plan](rt-plans/headlight.md)) |
| ACAM turbo / boost | None | New: a flame-like exhaust effect that glows, on `turbo`, `turbospin`, `thruster`; colour tuned by eye (first idea: faint blue). The VTOL's jets (`vtol2`, `jettail`) and the skimmer's (`jetmode2`) light the ground from their own sprites (done 2026-10-22); the VTOL's two jets each their own light, together about as bright as the turbo (done 2026-10-24). Thrusting leaves a faint trail behind each exhaust, smoke behind the turbo and vapour behind the jets and fans, carried by the mist (done 2026-10-24). Retail draws the buggy's and truck's turbo as blue flames at the exhausts (the additive parts of `wheelmode1a`/`2a`, only while boosting), which now light the ground behind them blue (done 2026-10-22) |
| ECM Unit | Three-phase colour-cycling pulse around its holder | Keep |
| Placed terrain lights (selectors 19, 54) | Authored colour and radius | Keep |
| Candidates by model name | — | napalm, stun, `lazegate`, `elecgate`, `vaporisebeam`, `emibeam`, `rembeam`, `orbitaray`, `rhinoeyelight`, `deyelight`, `mantlight`, `lites`, `talllite`, `spinlite`, `onlight`, maybe faint crystal glow |

In retail these lights only tint the terrain grid; in modern mode they light
models too, and metal reflects them.

**The active warpgate, done 2026-10-22** (when the warpgate is on, it needs a
better, more modern effect too). Its
lightning and flares (`gatebod`) light the gate and the ground round it
blue-white while they are drawn, so only while the gate is on, and a
field of energy fills its opening: swirling filaments round a bright
middle, its shape taken from the arcs each frame
([notes](re/PIXEL_RUNTIME.md#modern-mode-the-traced-world)).
The tuning file's `materials.glows` takes a colour of its own for a model
(the gate's), and `materials.fields` names the models whose arcs span a
field, with its strength and colour. Since 2026-10-23 both are off by
default. The gate looked worse than retail's, and the field was
why: it veiled the opening and pumped. The gate kept retail's look until
the redraw ([the warpgate plan](rt-plans/warpgate-effect.md)), and the
trace ran about 1 ms faster near a gate. Since 2026-10-24 its lightning is
redrawn (`materials.gates`): crisp jagged bolts between retail's ends, a
new shape each tick, glints in the flares' colours, and one steady light
at the opening while the spokes show, all drawn after the upscaler.

**Weapons as lights, done 2026-10-22** (some guns should emit light, for
example the plasma weapon and its bolts, as the flamethrower and the
lasers may already do). Retail's flamethrower
(selector 76) deposits a red-orange light, and the missiles, rockets,
grenades, charges and mines red or grey ground glows
([OBJECT_LIGHTING.md](re/OBJECT_LIGHTING.md)); the tracer
lights the ground, models and water from those. The bolts and the lasers
light nothing in retail: every bolt table's frames hold no light, and the
laser draws its beam in screen space. Under the tracer each bolt in flight
and each laser beam now lights what it passes in its sprite's colour (the
tuning file's `materials.shots`, strength and reach), each bolt its own
light (2026-10-23: gathered, Dual fire's side-by-side bolts glinted as one
streak between them); at night on Foldrak Noir the plasma bolts throw blue
light and shadows on the ground, the lasers green
([notes](re/PIXEL_RUNTIME.md#modern-mode-the-traced-world)).
The cannon's sparks light a little too (`materials.sparks`, 2026-10-23),
less than the bolts, and glint in the water.
Muzzle flashes are not built.

**Crystals glow, done 2026-10-22** (the green crystals should emit
their own low-level green light, and the rest likewise): the
tuning file's `materials.glows` names the models that light their
surroundings in their sprite's colour (the crystals and the yellow orbs),
with each one's strength and reach
([notes](re/PIXEL_RUNTIME.md#modern-mode-the-traced-world)).

**The power transformer glows, done 2026-10-23** (the power transformer entity also
needs to generate an orange hue around it): the pylon's orange core (`util04`) lights its frame and the ground
round it orange, a tinted entry in `materials.glows`
(plan item 8).

**Pick-ups read in any light, done 2026-10-23** (the red crosses should be far
less affected by light, being pick-ups): the
tracer keeps most of the light retail gives a pick-up, its own pulsing
colour, unshadowed; and every fan keeps its corners' colours, which the
red cross's red had been
(plan item 20).

**Shadows from the grid's lights done 2026-10-22**
([notes](re/PIXEL_RUNTIME.md#modern-mode-the-traced-world)):

- **The sources.** The grid now keeps each deposit's position, reach and
  colour. A primary ray lights the ground from them as retail's falloff
  does, with a shadow ray each.
- **What may shadow.** A particle's light (explosions, projectiles) is
  shadowed by anything, from the particle. Nothing shadows an actor's light
  (2026-10-22). The ground shadowing the headlamp, from its hull or from
  above its spot, cut the pool into black facets, since retail's lights
  light every slope alike.
- **Heights.** Each light's pool stays centred where it was deposited, a
  little above the ground. It shines from its emitter: the hull for the
  vehicle's lamps, the particle (at least a little above the ground) for a
  particle's; the other actors' lights from their pool's centre.
- **Where the grid stays.** Models keep the grid's light, which retail's
  shade already holds.
- **Its scale** on the ground was twice retail's until 2026-10-22: retail
  adds twice the grid's bytes to vertex colours whose 255 is one.

**Soft shadows done 2026-10-22** (use path tracing to make the
shadows a little softer, especially the trees): the sun is a disc,
`sky.sun_radius` degrees across each way (1 by default), each primary
hit's shadow ray aiming at a random point of it, and the denoiser gathers
the samples over frames and neighbours
([notes](re/PIXEL_RUNTIME.md#modern-mode-the-traced-world)).
The trees' shadows were sharpest because their leaves are masked sprites,
whose whole rectangles cast shadows until rays passed their keyed texels
(the same day).

### Full path tracing

**Superseded 2026-10-28 by [the full path tracing plan](rt-plans/full-path-tracing.md).**
A read of the code that day found these stages were about the lights
only: paths still end after one bounce, retail's vertex shade is still the
models' light, and light is capped at 1. The plan covers those as well.
Stage 2's "resampled importance sampling" below is a single pick weighted
by each light's share, not RIS.

**Queued 2026-10-22** (the whole point of path tracing is to replace
retail's lights and path trace everything; full path tracing joins the
list of plans). What the tracer does with lights today:

- **Retail's light records** (the grid deposits: explosions, the
  flamethrower, missiles, grenades, the vehicle's lamps) are the tracer's
  light sources: where the game says a light is, its colour and reach.
  The ground under them is traced, with a shadow ray each.
- **Models** took those lights from retail's own vertex shading
  (`surface.shade`), unshadowed, until stage 1 below.
- **Bounce rays** took retail's grid, not the lights, until stage 3.
- **The emitter** is the record's point, not the flame, fireball or exhaust
  drawn there; effects retail never recorded (bolts, lasers, crystals, the
  warpgate) were added as glows.

Full path tracing, then:

1. **One set of lights for everything**: the records and the glows light
   models and the ground alike, retail's grid share taken out of the
   models' vertex light, each light shadowed. **Done 2026-10-22** for
   primary hits: a model takes each record from its emitter's direction, a
   particle's shadowed, as the ground does
   ([notes](re/PIXEL_RUNTIME.md#modern-mode-the-traced-world));
   the glows lit both already.
2. **Light sampling**: each pixel picks one light by its likely
   contribution (resampled importance sampling) and traces its shadow, the
   denoiser gathering the samples, so the cost holds however many lights
   burn. **Bounce hits do, since 2026-10-22** (stage 3). A primary hit
   still traces every light within reach, free of noise, for 0.2 to 0.5
   ms through a grenade's blast at 4K; sampling there waits until a
   scene's cost calls for it.
3. **Bounce light from the lights**: a bounce ray's hit is lit by them too.
   **Done 2026-10-22**: one light picked by its share of their sum at the
   hit, shadowed with one ray; 0.25 ms more a 4K frame. The water's
   reflections, which nothing denoises, keep the grid.
4. **Emitters from what is drawn**: fireballs, flames, exhausts and sparks
   read from the particle pool and the draw stream, each in its sprite's
   colour and size, with the records kept for the lights nothing draws
   (the headlamp's pool). **Begun 2026-10-22**: a glow takes only a
   model's additive parts, the lights among its blended ones, and the
   VTOL's and the skimmer's jets light the ground under them. Fireballs
   and flames still light through retail's records. Foldrak Noir's base
   showed what else is drawn additively (a spawner's orbs, a shield
   generator's spinner, a factory's charge): as lights they barely
   showed, so they wait.
5. **Pulses** follow what is drawn: a glow's strength follows its sprite's
   brightness and size frame by frame (the tokens' pulse). **Done
   2026-10-22** for the brightness: a glow's strength follows how much
   light its sprites hold against a crystal's, so the tokens' light pulses
   with their core and the crystals' stays steady. A sprite drawn larger
   still holds the same light.

### Tuning

- No player-facing reflectivity or glow sliders (decided 2026-10-20);
  the values are tuned in the data file.
- One data file keyed by model or texture name: metalness, roughness, glow
  colour and strength per material; colour, strength, radius per light; global
  multipliers (overall reflection strength, glow strength).
- Seed lights from retail's colours and radii, converted, so the starting mood
  matches retail; push from there on purpose.
- Hot reload on a key in a running game.
- **Started 2026-10-22, with the fog**
  ([`modern_tuning.rs`](../crates/inf-game/src/modern_tuning.rs)).
  - The defaults are built in (`modern_tuning.json`).
  - Its `worlds` entries override them again for one world.
  - A `modern-tuning.json` beside the settings file overrode any part of
    them, read again within a second of it changing, while the looks were
    tuned by eye; the game reads it no longer (2026-10-27: the file is
    dropped), and the tests lay their overrides
    over the built-in values (`ModernTuning::read`).
- Fixed reference scenes from the existing harness (`--start`,
  `--input-replay`, `--fixed-step-micros`, `--snapshot`) for exact
  before/after comparisons.

## Model detail

Models are very low poly (median 15 vertices, largest 267; `headrobotic` is 8),
so detail comes from how edges and surfaces catch light. Planned, both
**subtle** so the game never looks sloppy:

1. **Smooth normals with a conservative crease angle**, computed at load.
   Only nearly-flat joins are smoothed; sharp edges stay hard. Path tracing
   needs the shadow terminator fix (offset shadow-ray origins using the
   smoothed surface) or low-poly self-shadowing turns blocky.
2. **Thin auto-bevels on hard edges**, at load, so edges catch highlights.
   Small fixed width; tiny parts skipped.

Both have per-model opt-outs and strength values in the tuning file.
Later options: tiling detail normal maps per material class; curved
silhouettes (PN triangles) on organic models only; replacement models by name.
Unchecked: whether any model's command program moves vertices per frame
(those can't be processed once at load), and which stored normals are per face
or per vertex.

**Held, 2026-10-22.** The traced models already shade smoothly wherever
retail shades by vertex: the feed takes the normal each shade slot was lit
along. The faces retail shades flat are its hard-surface design. Beside
the raster on Terra Nova, the traced bunker and vehicle show no faceting
that smoothing would fix. Both passes wait until a model shows the need,
or they are asked for.

## First-run preparation

- **Built in phase 1** (decided 2026-10-20). One preparation pass runs
  on the first Vulkan launch and every later feature plugs into it: processed
  geometry (normals, bevels), resolved materials, converted textures, and
  Vulkan's saved pipeline cache. After it has run once, loading is as fast as
  retail.
- **Lives in the game folder**, since the port replaces the old executable
  (retail already writes `Infest.sav` there): a folder beside `Overlay/`, such
  as `<game dir>\Cache\`. Not inside `Overlay/` and not `.ovl`: the extractor
  (`inf-extract/src/installation.rs` `overlay_paths`) treats every `*.ovl` in
  `Overlay/` as retail, and `inventory.rs` reads the whole folder.
- **Read-only installs** fall back to the port's per-user folder, where
  `launcher.json` already goes: `%LOCALAPPDATA%\Infestation Port\cache\` on
  Windows, the XDG equivalent on Linux.
- **Never touches retail files**, so hardlinked lab copies stay safe; harness
  probes run `--renderer software` and never build it.
- **Keyed by a fingerprint** of the retail OVL bytes, the tuning file, the
  processing code version, and the GPU/driver; rebuilds automatically on any
  change; safe to delete. Hot reload while tuning rebuilds only what changed.
- **Loading screen** only when preparation takes over about half a second
  (a split-second flash looks worse). Reuse retail's loading screen pieces in
  `loading_draw.rs`: the title, the equalizer animation as the progress
  indicator, and retail's font through `draw_line_text`, with text such as
  "Preparing graphics".

**Measured 2026-10-22, and what that left to build.** On the RTX 5080 a
Vulkan device starts in about 0.8 s. NGX's answer about DLSS's device
extensions takes 0.6 s of that; the rest takes 0.1 s.

- **Pipelines.** The tracer's are made in 2.5 ms, since the driver keeps
  its own compiled pipelines, so a saved pipeline cache would save nothing
  to see.
- **Nothing else to prepare.** Materials are matched by name as they are
  drawn, textures are converted as they are first sampled, and model
  detail is held ([model detail](#model-detail)), so no preparation pass or
  loading screen is needed yet.
- **NGX's answer is kept.** It changes only with the GPU, its driver or the
  DLSS libraries, so `ngx-device-extensions.json` in the per-user cache
  folder keeps it. That folder sits beside `launcher.json`
  (`%LOCALAPPDATA%\Infestation Port\cache\`), where an answer about the
  machine's GPU belongs. The entries are keyed by the GPU, the driver
  version and the three libraries' sizes and dates, newest first, four at
  most.
- **Hidden runs keep nothing**, so probes and tests never write to the
  user's folders; `INF_NGX_EXTENSIONS` names a file for checks.
- **The result.** A kept start is 0.75 s faster (2.0 s against 2.77 s for a
  30-frame probe), and DLSS starts on the kept list with no validation
  messages.

## Upscaling and denoising

**FSR 3.1, DLSS, Ray Reconstruction and XeSS done 2026-10-22**
([notes](re/PIXEL_RUNTIME.md#modern-mode-the-traced-world)):
the render size, jitter, motion vectors and display-size depth that every
upscaler takes; FSR through AMD's library; DLSS and Ray Reconstruction
through NGX, via a bridge DLL built with MSVC because the port builds with
the GNU toolchain; XeSS through Intel's library.
`scripts/build-upscalers.ps1` puts all the libraries beside the
executables. The settings rows choose them (done the same day): Ray
Tracing, Upscaler, Upscaling and Anti-aliasing, in the launcher's Vulkan
group and the Video menu. Anti-aliasing offers Off, FXAA, SMAA, TAA,
SSAA 2x and SSAA 4x (SMAA and supersampling since 2026-10-22, below). MSAA
is not built; it would suit only the raster's own draws, which
supersampling covers.

- **Ray Reconstruction lags pulsing lights** (held).
  RR keeps long histories, so retail's pulsing dynamic lights (the beacons
  around Terra Nova's start) lose their pulse and tint the ground behind
  it. NVIDIA's remedies (the responsivity and current-colour masks) changed
  nothing on Vulkan. Preset D halves the lag compared with the default
  model. DLSS Super Resolution with the port's own denoiser keeps the pulse
  exact, so it is the recommended setting. Whether RR stays in the
  Upscaler row, marked as smoothing lights, is still to be decided.

- **FSR 4** does not run under Vulkan yet ([below](#fsr-4)).

- No 8K/16K input. NVIDIA trained DLSS against 16K references offline; at
  runtime the input is smaller than the output (e.g. 1080p to 1440p internal
  for a 4K display).
- Inputs we must supply: a per-frame sub-pixel camera jitter, motion vectors,
  depth, exposure, and for FSR, masks for transparent and additive surfaces
  (water, particles, explosion glow).
- Neither backend produces motion vectors today. They need each object's
  previous transform, plus the moving geometry: infected vertices, waves,
  animated textures.
- The HUD and 2D overlays are drawn after upscaling, at output resolution.
- **Anti-aliasing** (2026-10-22), a Vulkan row wired with the upscaler
  rows: Off and the common methods, such as FXAA, SMAA and TAA (people
  dislike its blur, but some want it). The traced world takes supersampling
  (more rays a pixel) where a raster would take MSAA; MSAA itself suits the
  raster's own draws. The row greys out while DLSS or DLAA, FSR or XeSS
  runs, since each brings its own anti-aliasing (their native modes are the
  1x ones). The jitter and motion vectors the upscalers use serve TAA too.
  Like Filtering, it is a row in the in-game Video menu as well as the
  launcher, and both apply while playing (2026-10-22). While an
  upscaler runs, the row is greyed out and shows the upscaler (for example
  "AI Upscaler") in place of its own value.
- **More anti-aliasing** (2026-10-22: more
  antialiasing options were wanted), **done** the same day:
  - **SMAA**, the reference implementation of Jimenez et al.'s Enhanced
    Subpixel Morphological Antialiasing (MIT), byte for byte in
    `shaders/smaa/`, as three compute passes over the world's colour,
    traced or the raster's: luma edges, blending weights from its area and
    search tables, then the blend. It is sharper than FXAA and needs no
    history.
  - **SSAA 2x and 4x** draw the whole frame, world and HUD, two or four
    times the pixels (each side the square root of two, or two) and
    average it down to the screen with an exact box filter
    (`vk_downsample.frag`; snapshots take the same average). The traced
    world then has two or four rays a pixel. The raster's draws gain too,
    but their corners stand on the game's native pixels, so their steps
    stay as wide as the native resolution makes them.
  - **Cost** at 3840x2160 on the RTX 5080: SSAA 4x traces at 7680x4320,
    55 ms a frame against 17 ms; SSAA 2x about twice the native cost. They
    suit lower display resolutions. The size follows the row from the next
    frame.
- Path tracing runs 1 to 2 rays per pixel at internal resolution; light
  sampling and the denoiser carry the rest. High-resolution, high-sample
  renders are only offline ground truth for checking the denoiser.

### Anti-aliasing without ray tracing

**Built 2026-10-22** ([`vk_device/raster_aa.rs`](../crates/inf-game/src/vk_device/raster_aa.rs)).
With Ray Tracing off, or on a GPU without it, the Anti-aliasing and
Upscaler rows still apply to the raster's world, which is retail's look:

- **What runs.** FXAA, SMAA, TAA, supersampling, and DLSS, FSR or XeSS at
  the player's quality: Native AA (DLAA, and FSR's and XeSS's
  anti-aliasing modes) or a smaller size brought up (since 2026-10-23).
  The world is then drawn at the upscaler's render size, at the frame's
  top left: its draws' viewport and scissor shrink to it (the raster's
  vertex shader maps retail's coordinates by the native size alone), the
  jitter takes that size's pixels and more phases (the tracer's
  `ceil(8 ratio^2)`), the prepare pass reads that corner, and the upscaler
  fills the frame before the HUD, which stays at the full size. An
  upscaler that fails after a small draw has that frame stretched once,
  and is then off as before.
- **How.** For TAA and the upscalers the world's draws move a sub-pixel
  each frame, the tracer's Halton jitter, through the raster's vertex
  shader. After the world's last draw and before the HUD, a pass turns the
  frame's colour and depth into the inputs the tracer gives the
  upscalers. Each pixel's motion follows the camera, unless a moving model
  drew it (2026-10-23, [`raster_motion.rs`](../crates/inf-game/src/raster_motion.rs)):
  each object's models are remembered by their view transforms, keyed as
  the tracer's vertex history is, and a queued model command carries the
  map from this view to where its model stood in the last one. Its solid
  fans' corners are reprojected through it on the CPU, and a small pass
  draws those triangles again over the world's depth (tested, not
  written) into a motion image the prepare pass prefers. Under FSR at
  Performance the turning buggy's ghost is gone: in its box it sits 19%
  nearer a frame drawn without history. The result goes back into the
  frame, and the HUD draws over it sharp and unmoved.
  Under infrared vision the thermal camera's own look applies instead.
- **Cost**, from wall-clock time over 1,200 frames at 3840x2160 on the RTX
  5080, start-up included: FXAA and TAA cost nothing measurable; FSR adds
  about 1.5 ms a frame, DLSS 2 ms, XeSS 2.5 ms.
- **Native AA under validation**, checked here for the first time, traced
  or not. DLSS clears its output at the native size, so upscaler outputs
  now allow transfers. FSR's library warns that it binds its own luma
  history at another format there. Its output is right, and nothing on
  the port's side can change it.

### FSR 4

**Not runnable under the port's Vulkan renderer** (checked 2026-10-23;
still to be tested on an RX 9070 XT). AMD ships FSR 4
only in DirectX 12 libraries: SDK 2.x has no Vulkan library at all, and
its latest, 2.3.0 (2026-06-24, FSR 4.1.1), still says "Vulkan is
currently not supported" and that backend-specific functionality is
"currently only for DirectX 12". AMD said in October 2026 (press reports
of 2026-10-21) that FSR 4 is coming to Vulkan, without a date.

- **When AMD ships it.** The FidelityFX API lists a library's upscalers
  (`ffxQueryDescGetVersions`) and takes a version override
  (`ffxOverrideVersion`) at creation. The port loads that library in
  place of 1.1.4's and offers FSR 4 where the library lists it for the
  device, so the library is the GPU check and no table of device IDs is
  kept. It goes after FSR 3.1 in every Upscaler row, as "AMD FSR 4" in
  the launcher and "FSR 4" in game. OptiScaler, the tool the
  package was put together for, puts upscalers into games that do not
  offer them; the port calls AMD's library itself, so the label names
  FSR 4, not OptiScaler.
- **The package** ("SDK 2.0 + FSR4 INT8", looked at 2026-10-23)
  holds four DirectX 12 libraries and no readme or license. Each carries
  DXIL shaders and no SPIR-V, calls D3D12 and DXGI, and loads AMD's
  D3D12 driver extension (`amdxc64.dll`); the FSR 4 provider is
  `fsr4\dx12\ffx_provider_fsr4_dx12.cpp`. The port's
  `amd_fidelityfx_vk.dll`, by contrast, holds 478 SPIR-V shaders and
  calls Vulkan.

  | File | What it is | SHA-256 |
  |---|---|---|
  | `amd_fidelityfx_dx12.dll` | SDK 2.0.0's loader, `amd_fidelityfx_loader_dx12.dll`, under SDK 1.x's name | `6bf0d4f89611ff3cf0f15f767eb4c16c7044cba1e83d6272d996add42980b767` |
  | `amd_fidelityfx_upscaler_dx12.dll` | FSR 4.0.2, whose FP8 model needs RDNA 4 (RX 9000), with FSR 3.1.5 and FSR 2.3.4 | `8a976a2efce7738487a581bec5aa214eac9cabe9b4bc35ba54e6f1e76d1839b4` |
  | `amd_fidelityfx_framegeneration_dx12.dll` | FSR 3.1.5 frame generation | `a6f0021a1c8f3f510d3ebf0289be00a3703d9d006dc5699614ff87c34f737297` |
  | `FSR4 INT8/amd_fidelityfx_upscaler_dx12.dll` | FSR 4.0.2 with an INT8 model, a community build | `9123f83739e7bb39fcb135cafc339606dec78a74c650338ad275ee45c2d59d02` |

  The first three are byte for byte the unsigned `Kits/FidelityFX/bin/`
  copies in AMD's own release zip of SDK 2.0.0 (2025-08-20), whose
  license is MIT; AMD asks applications to load only its signed copies,
  which the same zip has in `signedbin/`. The fourth is not AMD's build:
  unsigned, version 4.0.2.0 without AMD's build number, compiled
  2025-09-12 on a private machine (its debug path is
  `FidelityFX-SDK_WithFSR4-main\...`), with an INT8 model
  (`fsr4_model_v07_i8_*`). It fits the press accounts of a DLL a Reddit
  user compiled from the FSR 4 source and INT8 model files that AMD
  published by mistake with SDK 2.0 and quickly withdrew; reports have
  it running on RDNA 3, RDNA 2 and even NVIDIA's RTX 30 series. AMD's
  own INT8 FSR 4 is 4.1.1 in SDK 2.3.0, for the RX 7000 (RDNA 3); RDNA 2
  was promised for early 2027.
- **A DirectX 12 bridge would run it now**, as OptiScaler's test build
  0.9.0-pre10 (2026-02-23, "FSR4 VK w Dx12") does for Vulkan games. It
  needs a D3D12 device on the same GPU (matched by the LUID Vulkan
  reports); FSR's colour, depth, motion and output images shared between
  the two APIs (`VK_KHR_external_memory_win32`) with a shared fence
  (`VK_KHR_external_semaphore_win32`); the FidelityFX API's DirectX 12
  descriptors; and each frame split into two Vulkan submissions around
  the D3D12 queue's upscale, since the upscale is recorded today in the
  middle of the frame's one command buffer (`trace.rs` and
  `raster_aa.rs`). It is Windows-only and costs some speed (OptiScaler
  puts its similar DirectX 11 bridge at "up to 10-ish %"). The same
  library's FSR 3.1 provider runs on any GPU, so the bridge could be
  checked on the RTX 5080, leaving only FSR 4's own provider for the RX
  9070 XT. It would ship AMD's signed SDK 2.3.0 upscaler, which runs FSR
  4 on RDNA 4 and RDNA 3, rather than the package's files.
- **Decided 2026-10-23: no bridge.** It is AMD's
  problem: FSR 4 comes when AMD ships its Vulkan library, through the
  path above, and not before.

## API choice

- **Vulkan**, through wgpu's ray queries or through `ash`. Ray queries in a
  compute shader are enough for a path tracer. wgpu's ray tracing support is
  still experimental; check its state before choosing.
- **Not OpenGL:** it has no ray tracing API, and DLSS supports only D3D11,
  D3D12, and Vulkan.
- **Not D3D12:** Windows-only.
- **Not RTX Remix:** badly optimized, and it needs Direct3D 9 fixed-function
  draws with real world/view/projection matrices; Infestation submits
  pre-transformed vertices.

## Open questions before starting


- Do all 3D draws pass through `model_vertices`, or do some effects
  (`d3d_effect`, the `d3d_d2` / `d3d_d6` / `d3d_da` packets) exist only in
  screen space? This decides how complete the scene feed can be.
- Does each world set its own sun direction and colour, or is a per-planet
  table needed?
- Is `TerrainWaterSurfaceRequest`'s wave function the same one vehicles float
  on? How many worlds set the spatial-wave flag?
- Do models carry per-vertex colours beyond lighting? Those would need
  separating from texture colour.
- Which particle types deposit retail light, and with what colours? Are
  plasma and laser shots particles only, or also models?
- Do textures carry names, or only indexes? This decides how per-texture
  materials are keyed.
- Do the backdrop textures show a sun, and does it line up with the light
  direction? Where do the other sky-like models appear? (See [Sky](#sky).)

## Third-party upscalers and publishing

A reading of each license (not legal advice; check the current terms
before a release):

- **FSR** (AMD FidelityFX SDK): SDK 1.1.4, its prebuilt signed
  `amd_fidelityfx_vk.dll` included, is MIT, so the library ships beside the
  executable with AMD's notice kept. The port loads it when present. FSR
  4's libraries are DirectX 12 only, so the port has none yet
  ([FSR 4](#fsr-4)). SDK 2.0.0's license is MIT. From 2.1.0 AMD's license
  file puts a binary-only license (no reverse engineering) on every file
  it does not list under MIT, and in 2.3.0 the MIT list names every file
  in the repository, the signed DLLs among them. AMD asks applications to
  load only its signed DLLs. The community's FSR 4 INT8 build has no
  license from AMD: it was compiled from source AMD withdrew, so shipping
  it risks a takedown, and as an unsigned native library from an unknown
  builder it would run inside the game unchecked.
- **XeSS** (Intel Simplified Software License): the library ships
  unmodified with Intel's copyright notice and license text. No reverse
  engineering, and no implied endorsement.
- **DLSS** (NVIDIA RTX SDK license): the runtime may ship inside an
  application with material functionality of its own. NVIDIA's marks go on
  the splash screen, the about box and the credits, per their branding
  rules. NVIDIA wants notice before a commercial release
  (developer.nvidia.com/sw-notification), which costs nothing to file for a
  free one. The SDK must never become subject to an open-source license.
  The port's bridge (`inf_ngx.dll`) links NGX's static library, so it ships
  as object code under NVIDIA's terms, while its source (`ngx-bridge/`)
  stays the port's own; NVIDIA's headers are not in the repository.
- **SMAA** (MIT): the reference shader and its two tables are in the
  repository (`shaders/smaa/`, with their origin), the notice heading the
  shader. The license does not ask binary releases to carry it.
- **The port's own license** must stay permissive (as OpenV2K's MIT or
  Apache-2.0); a copyleft license would clash with NVIDIA's terms. Loading
  each upscaler as an optional library keeps any release free to leave one
  out.

## Later, not now

- **Not planned: a day-night cycle** (2026-10-20). The game never says
  how long a mission lasts, and the map wraps around, so a moving sun would
  either vanish confusingly or make the planet feel like a toy. Each world
  keeps one fixed sun. Confirmed: every world is the same fixed 512 by 512
  terrain grid, 65,536 by 65,536 world units, wrapping on both axes
  (`TERRAIN_RUNTIME.md`; coarse terrain detail uses every other cell over the
  same area). That is about 230 buggy lengths across (`wheelmode1a` is 281
  units long), roughly 1 km if a mutant (132 units tall) is human-sized. The
  wrap is a torus, not a sphere, so there is no consistent "other side".

- Volumetric smoke (start with particles as glowing sprites); light shafts
  come with the [fog](#fog).
- **A ray-traced frontend** (2026-10-22, after anti-aliasing). The
  menus' backdrop already has a sun and planets, and the Infestation logo
  would be a light source of its own. **A limited test first**, **built 2026-10-23** behind the
  tuning's `menu.traced` (off): the main menu's models after its far
  backdrop, traced as one batch, the logo, sun and ring as lights
  ([notes](re/PIXEL_RUNTIME.md#modern-mode-the-traced-world)).
  It changed little, and the ringed planet loses its pink. **Then, the
  same day** (do what the tracer already has): the
  planets lit by the tracer alone, from the menu's sun off the screen's
  right, with smooth normals from each sphere's middle and shadow rays
  lifted above their low-poly hulls, and a stronger logo light. The bloom
  around the sun and the logo, the nebula backdrop and the other screens
  wait.
  **Since 2026-10-27 the traced menu is the default wherever Path Tracing
  is on** (the `menu.traced` switch is gone); without it the menu is
  retail's raster.
- Texture replacement packs (lighting will expose the low-resolution palette
  textures).
- Any of this for OpenV2K.
