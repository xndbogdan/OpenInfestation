# Infrared under Vulkan, a thermal camera worth using: plan

Status: steps 1 to 4 built (2026-10-23): the gain from a histogram
without the sky, no upscaler under infrared, the 1,280-pixel sensor with
detail enhancement, white hot, and ironbow by L's step (the menu row was
dropped as sloppy);
heat from the particles' and shots' lights (not lamps, which made the ACAM
heat itself), the hottest light counting, and the heat memory, capped at
0.4 and gathered over half each light's reach (a barrage first turned the
ground one bright field); heat by object (`vk_device/object_heat.rs`): an
attachment takes its object's heat, moving things warm by `moving`, a
bolt or cannon spark two ticks old warms the guns of the object nearest
it by `fired`, and a root turned wreck keeps its heat over the wreck's,
halving every `cooling` seconds. Of step 5, smoke at half strength.
The isotherm went the same day (the point is to have BW
mode), and with it the host's read of the gain's range. The billboards'
dimming is not built: 1B's quads carry a colour to cut, but 1A's sprite
quads (Terra Nova's start beacons among them) carry none, so it needs a
flag through the raster, and the glare reads as the lamp's heat anyway.
The tuning's `machines` became `moving` (the warmth and the speed it
peaks at), and `guns` and `wrecks` list the names.
Code lines are as of `8c3903afe`; other sessions are editing the tracer's
shaders, so expect small drifts.

Revised 2026-10-24: [the lamp at the eye](#the-lamp-at-the-eye-2026-10-24)
replaces the look. The heat of steps 1 to 4 stays, under the lamp's
image. Built 2026-10-25 (item 33): the lamp, the heat's fade and no fog
in the trace (`thermal_image`, `pt_trace.comp`), `sunlit` replaced by
`lamp`; the raster's inputs by the plan's pick, the tracer's own scene
under infrared wherever the device traces rays, Ray Tracing on or off.

## What was asked

Infrared needs more work: it should look better and be useful, not just a
filter. The best approach was not settled, so this plan lays
out the options with our pick. The ACAM's guns also look wrong in it
([gun-mounts.md](gun-mounts.md#5-why-the-guns-look-wrong-under-the-remastered-infrared-verified)).

Ground rules (2026-10-23): this is Vulkan only. OpenGL and the
software renderer keep retail's infrared exactly. The game does not
change; only what infrared shows may.

Later asks (2026-10-24):

- Bug (2026-10-23): in infrared BW mode the hovercraft's thrusters and
  some lights turned orange, unexpected since the point is to have BW
  mode. Built the same day:
  white hot is black and white only, the tint over body heat gone
  (`.tmp/plan-guns-thermal/obj_bw.jpg`).
- Thermal view felt sloppy (2026-10-23): remove the selector in the menu;
  with both infrared and the headlights upgrade, the first L press turns
  on the headlights and the second infrared; in Vulkan mode one L press is
  headlights, the next infrared with white as hot, and another toggles
  ironbow. Built the same day: no palette row; with both owned, L steps off,
  headlights, infrared, off in every renderer (a fix for retail, whose L
  toggled infrared alone), and under Vulkan off, headlights, white hot,
  ironbow, off (`.tmp/plan-guns-thermal/lamps.jpg`). With one owned, L
  toggles it as in retail.
- Infrared should work similarly in rastered and path-traced Vulkan,
  following similar rules, since it looked completely different under the
  two modes (2026-10-23). Vulkan without path tracing
  greys retail's green image through the same camera; the traced mode
  shows heat.
- The lamp at the eye, below (2026-10-24).

## The lamp at the eye (2026-10-24)

The request (2026-10-24): shadows should not apply to infrared light
except where the infrared cannot reach; on Artoria the fog comes closer,
yet the point of infrared is to see further. A proper infrared mode
ignores fog and renders a bit further, not infinitely but to its own
distance, separate from the fog: take the planet with the furthest fog
and make that the limit of the infrared.

Why it looks poor (verified, code): the traced infrared is already one
ray per pixel with no bounce light (`pt_trace.comp`, the thermal branch),
so the tracer's noise is not the cause. Heat is one number per model, so
every face of an object takes the same grey and shapes go flat; the
detail pass can only sharpen what little is left. The sun's warming
follows its traced shadow, so shadows show, and the raster can draw none,
so the two Vulkan modes cannot match.

The design:

- **A lamp at the eye.** The ACAM's infrared projector lights the scene
  from the camera itself, so every surface the camera sees is lit: no
  shadows, and no shadow rays. A surface darkens only as it turns away
  from the lamp (its normal against the view) and toward the lamp's
  reach. Its brightness is its infrared reflectance (its colour's
  brightness, unless the tuning names it) times the lamp's falloff. The
  lamp sits at the camera, not on the ACAM: from behind the vehicle a
  lamp on it would throw its shadow ahead.
- **Heat on top, slimmer** (our call, 2026-10-24: the heat mode
  could even go if it complicated things too much; what is wanted is an IR
  mode that works well for the game). The lamp alone is retail's night vision made
  sharp: it shows shapes, but a soldier reads as grey as the rock beside
  it. Heat is what makes infrared worth switching on in a fight, so it
  stays as a layer: heat + `lamp` × reflectance × facing × falloff. It
  keeps what tells the player something: what is alive or running, guns
  that fired, fresh wrecks cooling, glowing surfaces, blasts and shots
  warming what they light, scorch marks (steps 3 and 4). It drops what
  the lamp now does better: the sun's warming (a second light that fights
  the lamp's shading) and the darker-colours-warmer nudge. All of the
  heat's sums move from `surface_heat` in the trace into the shared pass
  below, so both modes compute them alike from position and one input:
  the base heat of the object a pixel shows.
- **If heat cannot match.** Should the raster fail to get each pixel's
  object (below), drop the heat in both modes and keep the lamp alone:
  both Vulkan modes following the same rules comes first.
- **The heat sees to the reach.** The lamp falls off over the whole reach
  (it is a lamp); the heat stays full until the last part of the reach
  and fades out by F, so a warm enemy reads at range where the lamp's
  light is thin, and nothing pops in at F.
- **Less camera.** With real shapes from the lamp the detail pass and
  the sharpening likely go to nothing; set by eye and removed if 0. The
  1,280 sensor's grain and stripes stay as the camera's character.
- **No fog.** Infrared sees through the world's fog and mist; on Artoria
  after its generators fall (item 34) it is the way to see. Smoke keeps
  its half.
- **Its own reach: F, 12,480 units.** The furthest fog of any world is
  Luna Azine's (the latest and thinnest, 98% at F), and F is 12,480 in
  every world. The game submits no objects past
  F (`fade[1]`, `world_scene.rs`), so infrared cannot see farther without
  changing the game. The lamp and the heat both fade to the gain's black
  by the reach, the ground past F with them, so nothing pops in; the
  curve is in the tuning (`reach`, and the falloff's shape). Retail's
  green lamp reached 3,096.
- **One image in both Vulkan modes.** Both write the same inputs per
  pixel (depth, normal, reflectance, heat), and one shared pass makes the
  image before the gain and the sensor. The traced mode has them
  (`geometry_out`, `albedo_out`, `heat_out`), less its shadow rays: the
  sun's two and `direct_light`'s. The raster has neither normals nor
  heat: retail's stream is pre-lit vertices (position, texture, colour,
  fog; `vk_primitive.vert`). Two ways, to check:
  - draw the tracer's scene feed under infrared: the same triangles,
    normals and heats, so the same image by construction (our pick, if
    the feed is built without path tracing);
  - else normals from the depth buffer (faceted: right for the
    flat-shaded models, the terrain shows its triangles) and each draw
    tagged with its model's heat.
- OpenGL and the software renderer keep retail's green, as before.
- Online: the same (decided 2026-10-23). It reveals to F, where retail's
  green reached 3,096.

This replaces Part 3 (it saw as far as the fog), the ground's
terrain-only sun shadow and the sun's warming (Part 1), and the haze
under infrared (`pt_resolve.comp`; thermal vision keeps the haze).

Concerns, for whoever builds it:

- **A lamp shows reflectance, not heat.** Without the heat layer the
  player gives up the warm figures; hence the layer above.
- **The raster's object per pixel.** The heat needs it and retail's
  stream does not carry it. The scene feed gives it if it is built
  without path tracing (check first); else each draw tagged with its
  model's heat. Neither: the lamp alone, both modes.
- **The raster's normals.** From the scene feed, or from depth
  (faceted; the terrain would show its triangles).
- **The lamp at the eye, not on the ACAM.** On the ACAM it throws the
  vehicle's shadow ahead from the third-person camera, needs a shadow ray
  or map, and the modes part again.
- **No fog anywhere under infrared.** The raster's late draws (sprites,
  particles, beams) read the tracer's fog at their depth (item 5); under
  infrared they must not, or they vanish where the world shows through.
- **Pop-in at F.** The game draws nothing past F; the lamp's falloff and
  the heat's fade must reach the gain's black by then, the ground past F
  cut with them.
- **It reveals more than retail.** To F in any light and fog, online too
  (decided 2026-10-23), where retail's green reached 3,096.
- **The gain.** Near ground lamp-bright and hot things above it: the
  percentiles (`gain`) and the lamp's strength set by eye so a soldier
  near the ACAM still stands out from the lit ground.

Cost (estimated from the shaders, not timed): the traced mode gets
cheaper, primary rays only; the shared pass under 0.2 ms at the trace's
1920x1080. The work is the raster's inputs.

Order of work:

1. The shared pass (lamp, heat, reach, no fog), the traced mode first,
   its shadow rays gone and `surface_heat`'s sums moved into the pass.
2. The raster's inputs into the same pass.
3. Sheets of `T2`, `T9`, `T14`, `T3`, `T17` and Artoria after `+13`,
   both Vulkan modes side by side, and the two modes' greys compared on
   the same frames (rms); OpenGL's frames byte for byte as before.

Words used below:

- **Heat**: a surface's temperature in the thermal camera, 0 the coldest to
  1 the hottest.
- **Gain**: how heat maps to the screen's dark-to-bright range. Thermal
  cameras set it automatically from what is in view.
- **Isotherm**: a camera mode that colours everything above a chosen heat,
  over the grey image.
- **Palette**: the colours heat is shown in (white-hot: hot is white).
- **Detail enhancement**: adding back the image's fine contrast (a
  high-pass) so a narrow gain does not flatten shapes, as real thermal
  cameras do.

## How we looked

The port ran as a probe from `.tmp\plan-guns-thermal\`
(hidden, silent: volume words 0) at 3840x2160 and 16 ms steps, each scene
three ways: OpenGL (retail's infrared), Vulkan with path tracing, and the
same with DLSS Quality, as a player would. Every start adds upgrade 61
(`0x3D`, Infrared vision) and the plasmas; L at frame 110 (150 on Tabuchi)
turns infrared on.

| Run | Scene |
| --- | --- |
| `T2_*` | Terra Nova (world 2), day: the start facing the base, its soldiers and turrets; then the plasmas fire (frame 165). |
| `T9_*` | Tabuchi (world 9): the hovercraft on the water. |
| `T14_*` | Foldrak Noir (world 14), night: open snow, two soldiers, the base beyond. |
| `T3_*` | Stendahl Alpha (world 3), dusk: walkers, and the buggy hit by fire. |
| `T17_*` | Shak'tahh (world 17): enemy tanks at range. |
| `T2_vk_fix`, `T9_vk_fix` | Path tracing with `tuning-fix.json` (the pods' and the hovercraft's heat fixed). |

## What we found

### 1. Retail's infrared (verified, notes and the port's OpenGL frames)

- **Getting it.** Every factory stocks Infrared vision. Once it is owned, L
  toggles infrared and no longer reaches the headlights (`00417922`,
  `object_runtime/actor/derived/selector2/update.rs:496-510`): a player
  who buys it gives up the lamp of [headlight.md](headlight.md).
- **What it is: night vision.** The frame clears to black with no sky, the
  world tint turns (0, 127, 0), and a green light of radius `0xC18` (3,096
  units) follows the player; every other light is scaled to green
  (`GAMEPLAY_RUNTIME.md`, infrared vision). The ground within about 3,000
  units glows green; beyond, it fades to black.
- **What it shows.** Shapes near the player, in green. It knows no heat: a
  soldier is a dark figure on green snow (sheet 12); a distant base is a
  black outline (sheet 14). Its use is dark worlds: Foldrak Noir's night,
  Stendahl Alpha's dusk, Artoria once its generators fall.
- **Not "Image enhancement mode".** The computer's second row (`004151A0`
  sets control `+3F`, as V does) starts the free look: a standing view the
  turn keys swing round the vehicle, with zoom (`00421DC0`). It works with
  infrared on, a scope for looking far, but it is not infrared.

### 2. The thermal camera today (verified, code)

- **Heat per surface** (`surface_heat`, `pt_trace.comp:1111`; tuning
  `thermal`): sky 0, water 0.12, ground 0.2 and up to 0.12 more in sun,
  glowing surfaces 0.95, models by name (the first matching pattern, else
  0.35), darker colours a little warmer. Retail's lights add nothing
  (`lights` 0): under infrared retail has turned them all green. Far things
  cool halfway to the sky through the fog (`pt_resolve.comp:55`).
- **Gain**: the lowest and highest heat of 64x36 samples, followed 8% a
  frame (`pt_thermal_gain.comp`).
- **Camera**: the world traced at half the display's size, or at the
  upscaler's size; brought to an 800-pixel grey sensor (the brightest
  channel of the frame, the raster's effects included); sharpened, with
  halos, stripes, grain and a vignette (`vk_thermal_*.frag`).

### 3. What the snapshots show

- **It already helps at night.** Foldrak Noir's soldiers are white on grey
  snow, where retail's green shows dark figures (sheet 12).
- **By day it is a grey filter.** Terra Nova's ground, hills and base are
  all mid grey; only the soldiers stand out, as white specks (sheets 10,
  14).
- **The sky sets the gain.** Its 0 is the lowest heat in every outdoor
  frame, so the ground sits at mid grey (99) and anything warm clips: the
  ACAM is a flat white shape (197 to 255), the hovercraft a white blob.
- **Heat never changes.** A firing turret, a parked one and a wreck look
  alike; scorched ground stays cold; explosions and shots are white only
  because their raster sprites are bright.
- **Shadows are cold at once.** The sun's warming uses the traced shadow,
  so the ACAM drags a crisp cold shadow across sunlit ground (sheet 04). A
  real thermal camera would still see that ground warm.
- **Flares read as heat.** Terra Nova's start beacons become two huge
  white stars over dark tripods; black smoke reads as cold.
- **DLSS works for nothing.** Under DLSS Quality the world is traced at
  2560x1440 and upscaled to 4K, then the camera shrinks it to 800 pixels.
- **The ACAM's pods** are greyer than its hull (gun-mounts.md).

## Options

**A. Fix the table and the gain (small).** The ACAM's parts at one heat,
the hovercraft's hull out of the engine list, the sky out of the gain.
Looks better at once; still a static heat map, so still feels like a
filter.

**B. A thermal camera that follows what happens (recommended).** A, plus
heat from what things do (shots, engines, explosions, fresh wrecks
cooling, scorched ground), and a modern camera: a gain that keeps detail,
an isotherm that makes body heat and hotter stand out, palettes, no
upscaler. Useful in daylight too.

**C. Tactical vision.** B, plus outlines or markers on enemies, seeing
through smoke and no fade with distance. The most useful and the most
revealing; it looks like a game feature, not a camera.

**D. Retail's green, remastered.** Retail's night vision through the
tracer: its green lamp as a real light with shadows, a phosphor look and
grain. Shows exactly what retail shows; no heat.

We pick **B**. It keeps infrared a camera (tasteful, recognisably the same
tool), makes it pay off in every light, and leaves the reveal questions
open. D can live on as B's green palette.

## Recommended design (B)

### Part 1: heat from what things are and do

- **One vehicle.** All the ACAM's parts take its hull's heat; its hot
  parts by name (`thruster`, `mainjet`, the hover fans `scatfan`). A
  child model takes its object's heat unless its own name is listed, so
  no attachment can go wrong again.
- **Per object.** The object draw tells the feed which object each model
  belongs to, with a few readings of its live state (read only; the game
  never sees the result):
  - **moving machines run warmer** with their speed, up to `machines[1]`;
  - **shooters warm**: a new shot warms its owner's gun parts (the pods,
    `*gun*`, turrets) by `fired`, cooling with a half-life;
  - **wrecks and corpses cool**: the `dead*` models start at the heat of
    what died and cool to the ground's with `cooling` as half-life. The
    feed remembers when it first saw each, by object.
  - each object's heat eases over a third of a second, so it never
    flickers.
- **Heat from retail's lights.** The feed keeps each light's own colour
  from before infrared's green, and leaves out the player's infrared lamp.
  Explosions, fires, muzzle flashes and lamps then warm what they light
  (`lights`).
- **Heat memory.** A grid of 128 by 128 cells of 64 units round the eye
  (8,192 units across) gathers what those lights put on the ground and
  furniture, and loses half of it every `memory` seconds: scorch marks
  after explosions, warm spots where shots hit, fading.
- **The world.**
  - The sky: coldest overhead, a little warmer at the horizon.
  - Water: cold; lava: glowing hot (as now).
  - The ground warms in the sun, shadowed by the terrain only, so moving
    things leave no instant cold shadow (thermal inertia). Superseded:
    no shadow at all ([the lamp at the eye](#the-lamp-at-the-eye-2026-10-24)).
- **The raster's effects.** Additive sprites (bolts, explosions) stay hot
  by their brightness. Billboards (lamps' glare stars, opcodes `1A`/`1B`)
  draw at a third of their brightness: the lamp's model already reads hot
  in the trace (`beacon`, 0.85). Darkening and half-alpha sprites (smoke,
  dust) draw at half their strength (open question 5).

### Part 2: a camera that reads well

- **Gain without the sky.** The gain pass builds a histogram of the heat,
  sky left out. Black and white sit at low and high percentiles
  (`gain`), followed over time as now. A few hot pixels no longer wash out
  the rest.
- **Detail enhancement.** The finish pass adds back the heat's fine
  contrast (`detail`): relief, building edges and figures keep their
  shape in a narrow gain.
- **Isotherm.** Heat at or above `isotherm[0]` (body heat, running
  engines, fire, shots) takes a tint (amber by default) over the grey,
  by its absolute heat, whatever the gain. Enemies stand out at a glance,
  day or night. (Built, then removed on 2026-10-23: white hot stays black
  and white.)
- **Palettes.** White-hot (the default, chosen 2026-10-20),
  black-hot, ironbow (purple through yellow to white), and green phosphor
  (retail's colour, with heat). A lookup texture in the finish pass.
- **Edges, faint.** Optionally the visible scene's outlines (depth and
  normal steps in the trace) drawn faintly over the heat, as cameras that
  fuse a visible image do. Off or low by default (`edges`).
- **Sensor.** 1280 pixels wide instead of 800, with subtler grain and
  stripes: the camera's character stays, without looking coarse at 4K.
- **No upscaler.** Under infrared the world is traced at half the
  display's size and never through DLSS, FSR or XeSS; the upscaler's
  history resets when infrared ends. The heat carries no bounce light, so
  it has almost no noise to reconstruct, and the sensor averages it down.

### Part 3: how far it sees

Superseded (2026-10-24): no fog, its own reach of F
([the lamp at the eye](#the-lamp-at-the-eye-2026-10-24)).

Three choices (open question 1):

- **The fog's range** (our pick, today's behaviour): heat contrast fades
  with the world's fog, by `haze` (now one half). Infrared sees as far as
  the normal view, but in any light.
- **Retail's reach**: beyond 3,096 units (`reach`), heat fades to the
  background, as retail's green fades to black. The same information as
  retail.
- **Everything drawn**: no fade. A real thermal camera at these
  distances.

### Tuning (`modern_tuning.json`)

The `thermal` block grows; `worlds` may override it per world (night
worlds colder):

```json
"thermal": {
  "sky": 0.0, "sky_horizon": 0.08,
  "water": 0.12, "ground": 0.2, "lamp": 0.35,
  "glowing": 0.95, "models": 0.35, "names": [],
  "lights": 0.5, "memory": 8.0,
  "machines": [0.4, 0.55], "fired": [0.3, 6.0], "cooling": 15.0,
  "gain": [0.02, 0.995], "rate": 0.08, "detail": 0.5,
  "isotherm": [0.6, [1.0, 0.65, 0.2]], "palette": "white-hot", "edges": 0.0,
  "haze": 0.5, "reach": null,
  "resolution": 1280.0, "grain": 0.02, "stripes": 0.008,
  "sharpen": 0.4, "halo": 0.3, "vignette": 0.2
}
```

`names` keeps today's list, fixed for the ACAM (gun-mounts.md, part 3).
Starting values, to be set by eye on the five scenes.

## What changes where

- `src/modern_tuning.rs`, `src/modern_tuning.json`: the block above.
- `src/object_runtime/object_draw.rs`: the object and its readings for
  the feed, before its model runs.
- `src/scene_feed.rs`, `src/scene_capture.rs`: each face's object; heat
  by object; the first-seen times.
- `src/world_lighting.rs`, `src/object_runtime/light_pass.rs`: the lights'
  own colours under infrared, without the player's lamp.
- `src/vk_device/trace.rs`: the heat memory grid, the per-object heats,
  the thermal render size without the upscaler.
- `shaders/pt_trace.comp`: `surface_heat` (objects, lights, memory,
  terrain-only sun shadow, sky gradient).
- `shaders/pt_thermal_gain.comp`: the histogram gain without the sky.
- `shaders/pt_resolve.comp`: the reach and haze.
- `shaders/vk_thermal_sensor.frag`, `vk_thermal_finish.frag`: detail,
  isotherm, palette, edges.
- The raster under infrared (Vulkan only): billboards dimmed, smoke
  thinned.
- SPIR-V rebuilt with `scripts/compile-shaders.ps1`.
- Notes: the RT plan's Infrared Vision, `PIXEL_RUNTIME.md` (thermal
  vision). Retail's notes do not change.
- Tests: the tuning parses; children inherit their object's heat; a
  wreck cools by its half-life; the light colours kept apart from the
  green ones; OpenGL's infrared untouched.

## Order of work

1. The tuning fixes (shared with gun-mounts.md), the gain without the sky,
   no upscaler under infrared. Small, and most of the "looks better".
2. Detail, isotherm, palettes, the 1280 sensor.
3. Heat from lights and the memory grid.
4. Per-object heat: shooters, machines, wrecks.
5. Edges, reach and the raster's effects, after the answers.

Each step ends with before and after sheets of the five scenes and a
commit.

## Cost

Measured now at 3840x2160 on the RTX 5080: under infrared the world is
traced at 1920x1080 without an upscaler, and at 2560x1440 under DLSS
Quality (6.3 ms a frame over that run) plus DLSS's own 2 ms or so.
Estimated:

- no upscaler under infrared: saves DLSS's time and almost half the traced
  pixels against DLSS Quality;
- per-object heat: CPU microseconds;
- the heat memory grid: a 128x128 image updated each frame, under 0.05 ms;
- histogram gain: under 0.05 ms;
- detail, isotherm, palette, edges at the sensor's size: under 0.2 ms.

Cheaper than today's infrared under DLSS.

## Risks

- **It reveals more than retail.** Body heat at the fog's range, at night,
  with an isotherm, finds enemies retail's green never showed. That is a
  gameplay question (below), and in network games a Vulkan
  player sees more than an OpenGL one.
- **Gain pumping.** A hot thing entering the view dims the rest; the
  isotherm keeps targets visible through it, and the rate smooths it.
- **Object bookkeeping.** A wreck seen late looks fresh; first-seen times
  are kept a minute after an object leaves the view.
- **Raster effects** stay approximate: their heat is their brightness.
- **Shared code.** The headlight, atmosphere and round-models plans edit
  `pt_trace.comp` and the feed; merge in order.

## Verification

- Before and after sheets of `T2`, `T9`, `T14`, `T3` and `T17`, same starts
  and keys.
- New runs on Foldrak Noir: a grenade (X), snapshots 1, 5, 10 and 20 s
  after: the scorch fades by `memory`; the plasmas fired (Z): the pods
  warm, then cool; a soldier shot: its corpse cools; a drive in sun: no
  cold shadow under the ACAM.
- Numbers: the pods at the hull's grey; the ground's fine contrast (the
  spread of its high-passed grey) at least twice today's; under 1% of the
  non-target pixels clipped; soldiers above the isotherm at 2,000 and
  5,000 units, and at the chosen reach.
- OpenGL and software: the `T*_gl` frames byte for byte as before; the
  event logs identical.
- The tracer's GPU line under infrared with DLSS selected: no upscaler
  time.

## Open questions

1. **How far may infrared see?** The fog's range (our pick), retail's
   3,100 units, or everything drawn? Answered 2026-10-24: no fog, a reach
   of F.
2. **The isotherm**, enemies and hot things tinted amber: on by default
   (our pick)?
3. **Outlines or markers** on enemies (option C): our pick is no.
4. **Palette**: white-hot (your earlier choice, and ours), ironbow,
   black-hot or green? A Video menu row to switch?
5. **Smoke**: thinned as a thermal camera sees it (our pick, half), or as
   in retail?
6. **Network games**: the same infrared online, or retail's reach there?
7. **The sensor**: 1280 pixels (our pick) or the coarser 800 of today?

## Evidence

Run folders in `.tmp\plan-guns-thermal\runs\`. Sheets in
`.tmp\plan-guns-thermal\sheets\`:

| Sheet | Shows |
| --- | --- |
| `04-guns-under-infrared.png` | The ACAM: retail's green, thermal, thermal firing. |
| `05-hover-guns-under-infrared.png` | The hovercraft: a white blob with grey pods. |
| `08-tuning-fix.png` | The pods and the hovercraft with the tuning fixed. |
| `10-terranova-day.png` | Terra Nova: normal, retail's green, thermal, thermal under DLSS, firing. |
| `11-tabuchi-hover.png` | Tabuchi: normal, retail's green, thermal. |
| `12-foldrak-night.png` | Foldrak Noir at night: soldiers dark in green, white in thermal. |
| `13-stendahl-shaktahh.png` | Stendahl Alpha at dusk (an explosion) and Shak'tahh's tanks. |
| `14-range.png` | The far base on Terra Nova and Foldrak Noir, three ways. |

The built steps' sheets, before above after, in
`.tmp\plan-guns-thermal\`: `ir_sheet.jpg` (the camera,
by day, by night and on the water, with and without DLSS), `g3_ir.jpg`
(Stendahl Alpha under fire), `obj_o2.jpg` and `obj_o3.jpg` (heat by
object), `obj_bw.jpg`, `lamps.jpg`. The warpgate's redraw under infrared,
in first person: `.tmp/plan-warpgate/g13_irfp.jpg`.

Helpers (scratch, not tracked): `probe.sh` and `matrix_t.sh` (the runs),
`view.py` (sheets), `tuning-fix.json`.
