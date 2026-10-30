# The headlight, and crystals catching it: plan

Status: step 1 built (2026-10-24): the lamp record, the pool left out of
traced frames, and the beam on the ground and models with shadows and the
facet guards. The beam's look moved from the starting values: intensity
2.5, spread 35° with 0.35 at full, and below the axis a Gaussian thinning
over 5° (to nothing: a floor of 0.08 there first lit a bright pool right
under each lamp, the falloff being so strong so near, plain under a
jumping buggy and a flying VTOL; 2026-10-24: a buggy jumped over a hill left the light source on the ground),
since with the plan's gentler thinning the
inverse-square falloff made the ground just ahead of the bumper the
brightest; the hot spot is now 1,000 to 2,000 units out. The
`ground_shadows` switch is not built: the guards kept Foldrak Noir's
hills clean (`.tmp/plan-headlight/lamp7_drive.jpg`). Steps 2 and 3 built
the same day (`lamp23_sheet.jpg`, `lamp23_E.jpg`): the water as planned,
and the air at 0.3 with the beam capped at 1.5 times its light 1,000 units
out, since near the lamp the falloff made it a white blot in the mist. The
light's own path down to the lit bottom is not dimmed (only the view's
is). Step 4 built the same day: bounces and metal's reflections take one
lamp by its share, the lenses, and the subtle glare (0.5). The bounced
light is faint (a mean difference of 0.02 on Foldrak Noir) and costs
nothing measurable. Nothing in the game looks at a nose from in front
but another player, so the lenses and the glare are checked by tests
(`a_lamp_glares_only_toward_the_eye`), not by snapshots. Step 5 built the
same day (`lamp5_sheet.jpg`): the small way, retail's crystals glowing. The
beam is taken at a crystal's foot as well as its middle: the middles of
Stendahl Alpha's tall shards stood above the beam's top edge, so the first
build lit none. The glow eases toward its share (`glow` 0.6, `flash` 0.5)
instead of growing with the light, which first washed the shards flat.
On review, the beam's top edge
went to 8° above the axis, fading over 12° more, so a hill in the beam is
lit some way up its face as retail's disc lights it. Its spread went to
50° (full over 0.45 of it) and its thinning below the axis to 8°, nearer
retail's disc in size (`.tmp/plan-slope/slope_sheet.jpg`).
The rear lamps (question 5) were built the same day
(`rear_lamps_sheet.jpg`). They sit under the sloping back, so they aim
down 45°. Their shadows are soft across 12 units: the rear pods beside
them cut a point's light into a hard-edged tongue. At 0.025 they stay
clear of the brightest-channel cap, which brighter lamps hit, browning
the moonlit ground. Code lines are as of `55915c1d0` plus the uncommitted
shader work in the tree that day; other agents are editing
`pt_trace.comp`, so expect small drifts.

## What was asked

- Headlights do not illuminate the water and need fixing. The headlight
  should act as a proper light emitter, from a spot on the ACAM that is the
  light source and projects white light; even the air should be slightly
  illuminated by it. Its light should be realistic.
- Crystals should glow a little in their own colour when lit by the
  headlights: in Terra Nova they should reflect white light, and in
  Stendahl Alpha the blue-ish one.

Ground rules (from the RT plan): only Vulkan with path tracing changes.
OpenGL and the software renderer keep retail's look. The game itself does
not change: L still toggles the vehicle's `+208` exactly as retail does.

## How we looked

The port ran as a probe (hidden, silent: volume words 0) from the copy in
`.tmp\plan-headlight\`, Vulkan with path tracing,
fixed 16 ms steps, snapshots at 3840x2160. Each run folder is under
`runs/`; labelled sheets of the key frames are in `sheets/` (listed at the
end).

One trap first: **L does nothing without the Headlights upgrade** (grant
`0x14`, `object_runtime/actor/derived/selector2/update.rs:496-510`). Our first runs pressed L with no effect;
every run below adds `"upgrades": [20]` (forms 2 to 5 also 1, 2, 165,
166, 167). The probe log's `object_light_deposits` count goes up by one
when the lamp lands a pool, which is how each run was checked.

| Run | Scene |
| --- | --- |
| `E_w3hov_vk_on/off`, `E_w3hov_gl_on/off` | Stendahl Alpha (world 3), dusk. Hovercraft on the water at (25300, 46016), facing the beach (heading 90). Vulkan, then OpenGL. |
| `C_w3beach_vk_on/off`, `C_w3beach_gl_on/off` | Buggy on the same beach at (26304, 46016), facing open water (heading 270). |
| `Cinland_on/off` | Same spot facing inland (heading 90), a control. |
| `D_w14_vk_on/off` | Foldrak Noir (world 14), night, the start: open snow and two soldiers. |
| `F14_fp_on/off` | The same in first person (F12). |
| `M14_on/off` | The same with the mist four times as thick (`modern-tuning.json`: `{"fog": {"density": 6.0}}`). |
| `C3_vk_on/off`, `K3_*` | Stendahl Alpha's crystal shards ahead of the buggy: (29200, 44650) heading 90. |
| `C2_vk_on/off` | Terra Nova's crystal clump (world 2): (15548, 20544) heading 90. |

Heading in `start.json` is in degrees, not 16-bit angle units
(`world_start.rs:66-76`): 0 faces +Z, 90 faces +X.

## What we found

### 1. What the headlamp is today: retail's pool (verified)

Retail has no beam. With the headlights on, `00420080` steps a ray from
the hull: 512 along the nose and -128 along the up row per step, so 14°
down. Steps 2 to 16 are tested against the ground. Where the ground
stands above the ray, retail drops one grey light into the light grid
(`vehicle.rs:59-132`):

- centre: the hit, usually step 2, about 1,000 units ahead;
- radius: 3/4 of the distance from step 1, plus 512 (about 900);
- colour: grey `0x60 * 500 / distance`, about 0.70 of full light at the
  centre (`HEADLAMP_RGB`, `vehicle.rs:20`).

The tracer gets that deposit as a light source with the hull as its
"emitter" (`light_pass.rs:210-243`, `world_lighting.rs:163-177`,
`world_scene.rs:1381-1431`). It lights the ground as retail does: the
colour times `1 - d²/r²` in X/Z, the same on every slope, with no facing
term (`source_share`, `pt_trace.comp:415-430`). Models take it once all
round and once more facing the hull. Nothing shadows it: actor lights
were made unshadowed on 2026-10-22 after the ground's own facets cut the
pool into black triangles (`share_shadowed`, `pt_trace.comp:438-443`).

What the player sees (sheet 04): a flat grey disc on the snow about
1,000 units ahead, the soldier in it lit evenly. Nothing comes out of
the ACAM; there is no lamp, no cone, no falloff from the lamp, no shadow.

**The pool vanishes over deep water and downhill** (verified). The ray
tests the terrain height, which under water is the bottom
(`height_at`, `terrain_runtime.rs:441`; `vehicle.rs:87-95`). When the
bottom (or a slope) falls away at least as fast as the ray drops, no step
lands and retail lights nothing. On world 3's beach facing open water
(`C_*`, sheet 03) the lamp is on, the deposit count is the same as with
it off (43 and 43), and the on-minus-off image is black under both
renderers. Facing inland from the same spot (`Cinland_*`) it deposits (44
against 43).

### 2. Why the water takes no headlight (verified)

Sheet 01 (`E_w3hov_vk_*`) shows it best: the hovercraft faces the beach;
the lit patch covers the beach and stops in a hard, jagged line at the
waterline. The water between the hovercraft and the beach is as dark as
with the lamp off. Five causes, all verified in the code:

1. **The water's own colour takes no light but the sun.** The traced
   water is finished in `water_color` (`pt_trace.comp:955-1028`); it never
   calls `direct_light`, so no deposit or grid light reaches the surface
   (`main`, `pt_trace.comp:1181`). Its body colour is the texture's mean
   times the sun (`water_body`, `pt_trace.comp:655-660`).
2. **The pool sits on the bottom, or nowhere.** Over water the deposit
   lands on the bottom (cause above), and the feed lifts its light point
   to 64 above the bottom, still under the water
   (`world_scene.rs:1410-1415`).
3. **The bottom shows only in shallow, clear water.** What shows through
   (`seen`) takes the grid at the bottom, then fades with the path:
   `exp(-t / clarity)` (`pt_trace.comp:988`), mixed toward the unlit body.
   Deep water hides it.
4. **The glints point away from the camera.** Each deposited light glints
   in the ripples, aimed from the hull, with a narrow lobe (power 120,
   `pt_trace.comp:1009-1020`). A lamp behind the camera's line of sight
   reflects forward, away from it, so from the chase camera there is
   nothing to see.
5. **Retail shows it, the tracer does not.** Retail's water is additive
   over the ground, and the ground under it takes the grid. Under OpenGL
   (sheet 02) the water in front of the hovercraft brightens with the
   lamp. The traced water is physically based: depth hides the bottom, so
   the one way retail's lamp reached the water is gone.

### 3. Why the air takes no headlight (verified)

The mist (`pt_fog_inject.comp`) lights each cell with the sky's fog
colour, the sun where a ray finds it, and the light grid near the ground:
`grid_light * exp(-height above ground / 384)` (`:46`, `:108`). So
retail's pool makes a low smudge over the disc, never a beam from the
vehicle. Over water the "ground" is the bottom, so even that smudge
fades. Sheet 06 (`M14_*`, the mist at four times its density): the
on-minus-off image shows a haze over the pool and nothing between the
ACAM and it. The default mist is thin: its density is the tuning's 1.5
spread over the fog's end (`trace.rs:2251-2254`), about 0.12 per 1,000
units at the base on world 3.

### 4. Where a lamp could sit on the ACAM (verified from the models)

The vehicle model is `acam` (H8 117). Each form draws a body as its child
at the same origin, with no turn (`0E` instances, orientation 0): buggy
`wheelmode1a` (118), truck `wheelmode2a` (119), hovercraft `hoverjet1`/`2`
(120/121, with `hovcowl` and `scatfan`), VTOL `vtol1`/`2` (124/125),
skimmer `jetmode1`/`2` (126/127). Model units are world units (the buggy
is 281 long, z -164 to 117). +Z is the nose, +Y up; the light pass uses
the same rows (`vehicle.rs:74-78`: basis words 7-9 the nose, 4-6 up).

- **There is no front lamp in any form.** The only lamps are at the rear
  of the buggy and truck: additive billboards (opcode `1A`) at
  (±10, -6, -81), a white disc and star (sprites 586, 452), and at
  (±17, -6, -81), a red disc and star (585, 282), each pair behind its
  own register test (sheet 10). The skimmer has red side lights at
  (±94, -67, -6).
- **All five bodies share the same nose.** Their front-most vertices are
  identical: (6, -23, 116), (10, -1, 105), (14, -37, 104), (22, -6, 83)
  and their mirror twins. The nose is a narrow wedge; its two side faces
  (sprite 346) have their centres at (±13, -17, 102). That is where a
  pair of lamps fits, the same spot in every form.
- The skimmer's skis reach further forward, to (±33..53, -42..-53, 137),
  below and outside the lamps: they would clip the beam's lower outer
  edge, which looks natural.
- The hull is about 80 to 90 units above the ground on the buggy
  (estimate: the wheel hubs at y -59), so the lamps sit about 70 above
  it.

### 5. The crystals (verified)

- **Which models.** The scenery crystals are `xshard01..03` and
  `xclump01..06` (H8 409 to 417). They are terrain furniture, not actors:
  no class and no grant. H12's sparse records place them by a lookup
  byte, and the H11 table in `9.ovl` maps lookups 1 to 8 to H8 416 down to
  409 (`xclump01` is in no lookup). Terra Nova (world 2) has 17: 16
  clumps and one shard. Stendahl Alpha (world 3) has 42 shards. Two spots
  were visited to confirm (`C2_*` next to `xclump05` at (16448, 20544);
  `C3_*` next to the shards at (30016..30272, 44608..44864)). The models
  are also loaded in worlds 23, 28 and 33; whether those place any is
  unchecked.
- **Not the pickups.** The collectable crystals (`token01..03`) already
  glow under Vulkan (`materials.glows`). The scenery is meant.
- **How they are drawn.** Their faces use sprites 1443 and 1444, which
  are half-transparent (variant flags `0x0C`, half alpha `0x08`). Such
  faces stay on the raster, drawn over the traced world
  (`scene_capture.rs:181-186`). They are lit as furniture: retail's
  ambient is the tint plus the light grid at their cell
  (`terrain_furniture.rs:143-200`, `software_scene/furniture.rs:350-380`).
  So today the headlamp's pool washes them toward grey (sheet 07: teal
  `(64, 123, 120)` with the lamp off, `(96, 145, 141)` with it on).
- **Their colour.** Sprite 1443 is the same in all five texture banks
  (`9d0` to `9d4`): a 20x69 gradient, mean `(90, 99, 99)`, a pale grey
  with a hint of teal (sheet 09). So the texture does not make Terra
  Nova's white and Stendahl Alpha's blue. What does: each world's light,
  and what shows through the half-transparent faces. Measured with the
  lamp off: Terra Nova's clump `(99, 143, 128)` (pale mint, under green
  daylight and a green sky), Stendahl Alpha's shard `(64, 123, 120)`
  (teal, under its dark teal light). Neither world has a night; Stendahl
  Alpha's dusk is as dark as they get.

### Also seen

- On bright snow the lit pixels clip: the composite clamps colour to 0..1
  (`pt_composite.frag:132`), and the water is stored in 8 bits
  (`albedo_out`, `pt_trace.comp:43`). The on-minus-off images turn red on
  snow because green and blue hit the top first (suspected cause; it
  fits the numbers). A brighter, realistic lamp makes this matter more.
- The headlamp's deposit costs nothing measurable: Foldrak Noir traces at
  8.0 ms a frame with the lamp off and 8.2 with it on.

## Proposed design

The lamp becomes a real light in the tracer: two small lamps on the nose
throwing a shaped beam of white light. It lights the ground, models and
water, it casts shadows, it bounces, and it glows in the mist. Retail's
pool stays for OpenGL and software, and leaves traced frames so the light
is not counted twice.

Words used below:

- **Beam shape**: how bright the lamp is in each direction. Here a wide,
  flat beam with a soft top edge (the cut-off), like a car's.
- **Falloff**: light spreads out, so it weakens with the square of the
  distance (inverse square).
- **Facing**: a surface square to the light takes all of it; one it
  grazes takes little (Lambert's cosine).
- **Fresnel**: water reflects more at grazing angles and lets more
  through when seen from above.
- **Phase**: how much light a fog droplet sends each way.

### Part 1: the lamp record, and no double light

- **The light pass records the lamp.** When `+208` is set, `vehicle_lights`
  also returns the vehicle's hull, basis and form, whether or not
  retail's ray found ground (`vehicle.rs:165-247`). The traced lamp
  shines over deep water and downhill, where retail's pool vanished.
- **Traced frames skip the pool.** The world draw knows the frame is
  traced before the light pass (`tracing`, `world_scene.rs:692`, pass at
  `:815`). The request then carries a list for the lamps, and the
  dispatch (`light_pass.rs:210-243`) records the lamp instead of
  depositing the headlamp's packet. The rear glow and the ECM pulse stay
  deposits. Untraced frames (OpenGL, software, the lab) deposit exactly
  as now; `headlamp_light` and its 40 Infest03 receipts are untouched.
- **Why skipping is enough.** The light grid feeds only lighting: the
  ground's vertex colours, objects' and furniture's ambient, and the
  tracer (the readers of `cells()` are `object_draw.rs`,
  `software_scene/furniture.rs`, `terrain_draw/strip.rs` and the feed).
  With the pool gone from the grid in traced frames, every tracer path
  that reads the grid (reflections, what shows through the water, models'
  own shade, the mist) is free of it at once. No second grid, no
  subtraction.
- **The cost of skipping.** Raster-drawn parts in a traced frame (blended
  model parts, billboards, the crystals) no longer get the pool. The
  crystals get the lamp back in part 6. The probe's deposit count drops by
  one in traced frames.
- **The feed places the lamps.** For each recorded vehicle the feed puts
  two lamps at model (±13, -17, 102), moved 2 units forward along the
  nose, through the hull and basis rows: `hull + x*X + y*Y + z*Z`. The
  axis is the nose turned down by `pitch`. Eye-relative, like the other
  lights. The nearest four vehicles count (network games), eight lamps at
  most.

### Part 2: the beam on the ground and on models

At a point `P` with normal `N`, each lamp gives:

```text
light = color * intensity * shape(dir) * 1e6 / (d² + near²) * window(d / reach)
        * response(N·L) * shadow
```

- `dir`, `d`: the direction and distance from the lamp to `P`; `L` the way
  back to the lamp.
- **shape**: measured in the lamp's own frame (across, up, along the
  axis). Full within `inner` of the axis sideways, fading to nothing at
  `spread`. Upward a soft top edge: full up to `cutoff` above the axis,
  gone `soft` degrees higher. Downward the light thins below `hot`
  degrees under the axis, so the ground just ahead of the nose does not
  blow out while the ground far ahead stays lit; real headlamps are built
  this way.
- **falloff**: inverse square, with `near` stopping it from exploding at
  the nose, and a smooth fade to zero at `reach`.
- **intensity**: the light on a surface square to the beam 1,000 units
  away, where 1 is full light (a shade of 128).
- **response**: Lambert's cosine, softened by `wrap` for the ground. The
  lamp grazes flat ground (about 4° at 1,000 units), so a smooth surface
  would look nearly black there. Rough snow and soil send more of a low
  lamp's light back toward a viewer riding near the lamp, as the chase
  camera does; the wrap stands in for that.
- **shadow**: one ray per lamp from `P` to the lamp, stopping 4 units
  short, against models and the ground (`MASK_SOLID`). Two lamps 26
  apart give the double shadow of a real car. The lamps sit in front of
  the nose, so the ACAM cannot shadow its own beam; the front wheels and
  the skimmer's skis may clip its edges.

Where it goes in `pt_trace.comp`:

- **Primary hits** (`direct_light`, `:554-651`): the ground and models in
  the beam take the lamps, shadowed. Models no longer take the pool there
  (it left the grid), so nothing is doubled.
- **Bounces** (`sampled_sources`, `:470-504`): the lamps join the lights a
  bounce ray picks from, by their share at the hit, shadowed by the same
  one ray. The lit ground then lights what is near it a little.
- **Reflections and what shows through water** (`shade_ray`,
  `:693-724`, the paths that take the grid): add the lamps, shadowed by
  one ray when their share is worth it, so the lit shore mirrors in the
  water as it is lit.

Superseded 2026-10-26 (the headlamps' pool no longer steps): the facing fade below made the pool
step in the triangles' shape. The lamps' rays now meet the ground's smooth
surface (`ground_through`) and the models alone through the BVH.

**Black facets** (the 2026-10-22 report on Foldrak Noir's hills and
Artoria's trenches) come back if nothing is done. The ground is coarse
and lit along smooth normals; where a facet barely faces a low lamp, the
facet next to it can block the shadow ray while the smooth normal says
"lit". Three guards: start the ground's shadow ray a few units up its
normal; fade the shadow out where the flat face is nearly edge-on to the
lamp, so facing alone decides there; and a tuning switch, `ground_shadows`,
that lets only models shadow the lamp (models then need a mask bit of
their own: the ground's instance is `MASK_SOLID | MASK_GROUND`,
`trace.rs:1835-1863`). The old pool lit facets turned away from the lamp
at full strength, which made their shadows black; with facing, those
facets are already dim.

### Part 3: the water

In `water_color` (`pt_trace.comp:955-1028`), for each lamp:

- **The water body glows in its own colour.** Light entering the water
  (`1 - Fresnel`, facing up) scatters off what floats in it and comes
  back. Add `tint * lamp light at the surface * water_scatter *
  (1 - exp(-depth / clarity))`, the tint being the water's texture mean.
  Murky water glows near the surface; clear water lets the light down to
  the bottom instead. World 3's teal, Tabuchi's green and Altair's
  yellow-green each glow in their colour. This is the main fix for "not
  illuminating the water".
- **The lit bottom.** What shows through takes the lamps at the bottom,
  dimmed by the light's own path down: `exp(-path in water / clarity)`.
  In shallows the lit beach carries on under the water instead of
  stopping at the waterline (sheet 01).
- **Glints.** A GGX lobe (a standard model of a rough mirror) on the
  rippled normal, roughness `water_gloss`, in the lamp's white, shadowed.
  From behind the vehicle only ripples tilted back toward the camera
  catch it: a sparse glitter in the beam, stronger in first person and
  turning. The current deposit glints (`:1009-1020`) stay for the other
  lights.
- **Foam and spray** take the lamps as a white surface does (`:1022`,
  `drop_color` `:1066`).
- Lava (`water_glow`) is left alone: it outshines a lamp.

### Part 4: the air

The beam scatters in the mist. In `pt_fog_integrate.comp`, at each slice
along each column, add `density * lamp light * phase * air`.

- **Not in the injection pass.** That pass keeps 90% of each cell from
  the last frame where the world point was. The beam moves with the
  vehicle, so its history would smear behind every turn. The gathering
  pass has no history: the beam is drawn fresh each frame at the mist
  grid's resolution (160 columns, 64 slices). Its edges are soft anyway.
- **Phase.** The sun's mist uses forward scattering (`anisotropy` 0.35).
  The camera sits behind the lamp and sees light scattered back toward
  it, which forward scattering makes three times dimmer. The beam gets
  its own `phase`, near 0 (even scattering), so it shows from the chase
  camera.
- **Thin air.** `air_floor` is a least density for the beam alone, so the
  air glows faintly even where the world's mist is thin (slightly
  illuminated).
- **Left out: shafts.** Models do not shadow the beam in the air (a
  soldier's dark shaft through it). That needs a ray per cell and a
  history the beam cannot use; a later step if wanted.

### Part 5: the lamps themselves

- **Lenses.** Two small white discs on the nose (radius `lens`, 4), fed as
  unlit (emissive) triangles: seen from in front, in the water's
  reflection and in metal. They sit just behind the light's starting
  point, so they never shadow it.
- **Optional glare.** A retail-style flare when a lamp faces the camera
  (the rear lamps' white star, sprite 452, already exists). Off unless the
  user wants it.

### Part 6: crystals catching the light

**Recommended (small, keeps retail's crystals):**

1. The tap knows the crystals by name (`xshard*`, `xclump*`, in the
   tuning's `crystals.names`) and the furniture draw knows their cell.
2. In traced frames the CPU works out the lamps' light at each crystal
   (the same beam function, plus a coarse test of the terrain along the
   line so a crystal behind a ridge stays dark). One small Rust function
   serves this and the feed; the shader has its twin.
3. **The crystal glows in its colour.** Its furniture ambient (where the
   grid light went, `software_scene/furniture.rs:363-380`) gains
   `color * received * glow`: the whole crystal brightens evenly, as a
   translucent gem does.
4. **It flashes white.** For crystals in the beam the lamp also becomes
   the model's directional light, in the lamp's white, so the facets
   turned toward the vehicle light up: the "reflect white light" of Terra
   Nova.
5. **It lights its surroundings.** A glow light (the existing kind 2) at
   the crystal's centre, in its colour, strength `received * glow`, reach
   `crystals.reach`: the snow round it takes a tinted glow, shadowed by
   the tracer. Only crystals receiving light emit.

**The colour.** The texture is the same pale grey-teal in both worlds, so
it alone would make every crystal glow the same pale cyan-white. Two
choices:

- per-world colours in the tuning, as asked: Terra Nova
  (2) white `[1, 1, 1]`, Stendahl Alpha (3) blue-ish `[0.55, 0.8, 1.0]`;
- automatic: the sprite's mean times the world's light colour, scaled to
  full brightness (Stendahl Alpha teal, Terra Nova pale mint).

We propose the per-world colours, with the automatic one for any other
world. The white flash stays white everywhere: a gem's surface reflects
the light's own colour.

**Which lights drive it.** The headlight, as asked. Explosions, plasma
bolts and glows could drive it through the same function (a tuning
switch, off). Never the sun or the sky: the crystals would glow all day
and the effect would be lost.

**Alternative: crystals as traced gems.** The tap takes their faces as a
new translucent surface, finished in the trace like the water: Fresnel
reflection, the background refracted through and tinted, per-pixel GGX
glints of the lamps, an inner glow. Truer sparkle, but more work, the
crystals' look moves away from retail's flat half-alpha, and what shows
through them goes undenoised.

### Tuning (`modern_tuning.json`)

New blocks; `worlds` overrides them per world as it does the others.
Starting values, to be tuned by eye on Foldrak Noir, Artoria and
Stendahl Alpha:

```json
"headlight": {
  "color": [1.0, 1.0, 1.0],
  "intensity": 1.5,
  "position": [13.0, -17.0, 102.0],
  "pitch": 4.0,
  "spread": 30.0,
  "inner": 0.5,
  "cutoff": 4.0,
  "soft": 3.0,
  "hot": 6.0,
  "near": 250.0,
  "reach": 6000.0,
  "wrap": 0.6,
  "lens": 4.0,
  "ground_shadows": true,
  "water_scatter": 0.6,
  "water_gloss": 0.15,
  "air": 1.0,
  "air_floor": 0.03,
  "phase": 0.0,
  "forms": {}
},
"crystals": {
  "names": ["xshard*", "xclump*"],
  "glow": 0.6,
  "reach": 500.0,
  "color": null,
  "all_lights": false
}
```

- `position` is model space, mirrored for the pair; `forms` overrides it
  or `pitch` per form ("1" buggy to "5" skimmer), empty by default since
  the nose is shared.
- Angles are in degrees; `inner` is the share of `spread` at full
  strength; `cutoff` is the top edge above the axis.
- `color: null` takes the automatic colour; worlds 2 and 3 set theirs.

## Alternatives considered

- **Fix only the water.** Let the water take the grid, and put the pool on
  the water's surface. A few lines, but still a flat disc from nowhere,
  still gone downhill, no beam, no lamp. It does not meet "a proper light
  emitter".
- **The lamp as a new kind of deposited light.** The source buffer has a
  spare float and an emitter slot; a "kind 3" spotlight could ride in it
  (`trace.rs:2102-2172`, `LIGHT_SOURCES` 64). That reuses the loops but
  the lamps must never be cut by the nearest-64 sort, and the record
  would need more room for the beam's frame. A small lamp list in the
  frame block is clearer. Either works; the implementer picks.
- **A sharper beam in the air:** march each pixel's view ray through the
  beam in the composite, with shadows. Crisp shafts, but several
  milliseconds at 4K. The mist grid costs almost nothing.
- **Keep retail's pool as well and add the beam.** Double light, and the
  pool would still show where no beam is.
- **A real low beam** (sharp horizontal cut-off at the horizon): the most
  realistic, but soldiers' heads stay dark at mid range. The default
  `cutoff` of 4° above the axis sits between low and high beam; see the
  questions.

## What changes where

- `src/object_runtime/light_pass/vehicle.rs`: return the lamp (hull,
  basis, form) beside the packets, the headlamp's packet apart from the
  rear glow and pulse.
- `src/object_runtime/light_pass.rs`: `Flag0LightPassRequest` gains the
  lamp list; when present, record the lamp and skip the headlamp's
  deposit.
- `src/world_scene.rs`: pass the list in traced frames (`:815`); feed the
  lamps beside `feed_light_grid` (`:1381`).
- `src/scene_feed.rs`: a `SceneLamp` list (position, axis, across, up,
  colour) and the lens triangles.
- `src/vk_device/trace.rs`: upload the lamps and the tuning's beam values
  in the frame block; the lens triangles ride with the models.
- `shaders/pt_frame.glsl`: the lamp fields and `lamp_light()` (shape and
  falloff), shared by the trace and the mist.
- `shaders/pt_trace.comp`: `direct_light`, `sampled_sources`,
  `shade_ray`, `water_color`, `drop_color`.
- `shaders/pt_fog_integrate.comp`: the beam's scattering per slice.
- `src/modern_tuning.rs`, `src/modern_tuning.json`: the two blocks.
- Crystals: `src/scene_capture.rs` (name them), the furniture draw
  (`src/software_scene/furniture.rs`) for their ambient and flash in
  traced frames, `SceneFeedBuilder::glow` for their light.
- SPIR-V rebuilt with `scripts/compile-shaders.ps1`.
- Notes: `PIXEL_RUNTIME.md` (Modern mode: dynamic lights, glints, mist),
  `INFESTATION-RT-IMPLEMENTATION.md` (the light sources table),
  `VEHICLE_DRIVE_RUNTIME.md` (traced frames record the lamp instead of
  the pool).
- Tests: a traced light pass records the lamp and skips the deposit, an
  untraced one deposits as before; lamp placement from a known basis; the
  tuning blocks parse.

## Order of work

1. The lamp record, the skipped pool, the beam on the ground and models
   with shadows and the facet guards. Snapshots on Foldrak Noir and
   Artoria (fully dark until the headlights come on).
2. The water: body glow, lit bottom, glints, foam.
3. The air.
4. Lenses, bounces, reflections.
5. The crystals.

Each step ends with before and after snapshots and a commit. The water
step lands after the shoreline and underwater plans' changes to
`water_color`, which touch the same lines.

## Cost

Measured now at 3840x2160 on the RTX 5080: Foldrak Noir 8.0 ms a frame
(trace 7.4), Stendahl Alpha's beach 5.1 to 7.3. Estimated additions:

- primary hits in the beam: two shadow rays each, perhaps a third of the
  screen: 0.3 to 0.6 ms;
- water pixels in the beam: the lamp math and one shadow ray: under
  0.2 ms;
- the mist: about 900,000 cheap evaluations, no rays: under 0.05 ms;
- bounces: the lamps join the existing pick, no new ray;
- crystals: CPU microseconds; a few glow lights: under 0.1 ms.

About 0.5 to 1 ms in all, only while the lights are on.

## Risks

- **Clipping.** The frame is clamped to 0..1 and the water stored in 8
  bits. A real lamp is far brighter on a soldier facing it than on the
  ground it grazes, so near, facing surfaces turn flat white. The beam's
  shape (`hot`, `near`) and `wrap` keep the ground and models in range;
  a gentle roll-off of highlights for the whole frame would help more but
  changes every bright pixel, so it is an open question.
- **Black facets on hills** (part 2's guards; reported once).
- **The ground grazed at 4°** may still look dim far ahead; `wrap`,
  `intensity` and `pitch` are the knobs.
- **Lamps misplaced** if the drawn model is offset from the hull (the
  hovercraft's bob, the VTOL's tilt): checked by the lens in snapshots.
- **Raster-drawn parts lose the pool** in traced frames (part 1).
- **Shared code.** The shoreline, underwater, effect-reflection and sky
  plans also edit `water_color`, `shade_ray` and the frame block; merge
  in order. The sky plan may darken worlds (Artoria), which makes the
  lamp matter more.
- **The beam in the air** has no shafts and follows the mist grid's
  resolution.

## Verification

All under Vulkan with path tracing, hidden, 16 ms steps, `"upgrades":
[20]`, before (the runs above) and after:

- `E_w3hov`: the water between the hovercraft and the beach glows teal
  in the beam; the lit beach continues under the shallows; no hard line at
  the waterline.
- `C_w3beach`: the beam lights the open water and the air where retail's
  lamp lit nothing.
- `D_w14`, `F14_fp`: a beam from the nose, brightest some 1,000 to 2,000
  ahead; the soldier lit on his near side, his shadow behind him.
- `M14` and the default mist: a visible beam from the nose; faint, not
  foggy, in the default.
- `C3`, `C2`: the shards glow blue-ish and the clump white, the snow round
  them tinted, the facing facets flashing.
- A drive over Foldrak Noir's hills and Artoria's trenches (Up held,
  snapshots every 30 frames): no black facets.
- Each form (1 to 5) at night: the lenses on the nose, the beam from them.
- A turn (Left held): the beam stays clean in the mist, no smear.
- The same runs under OpenGL and software: identical to before (byte for
  byte), and lab event logs unchanged.
- The tracer's GPU line ("Vulkan tracer: ... GPU X ms") before and after.
- `cargo test -p inf-game` with the new tests.

## Open questions

1. **Low beam or high beam?** A low beam lights the ground evenly and
   leaves things above the horizon darker (realistic); a high beam lights
   enemies fully but glares. We propose in between (`cutoff` 4°).
2. **White exactly**, or slightly cool (LED) or warm (halogen)?
3. **How much air glow?** Barely there, or a clear beam in every world?
4. **A glare on the lenses** when the ACAM faces the camera?
5. **The rear glow**: give the red and white rear lamps the same
   treatment (real lights from the model's rear lamps)?
6. **Crystals**: the small way (retail's crystals, glowing) or traced
   gems? White for Terra Nova and blue-ish for Stendahl Alpha as the
   colours? Should explosions and plasma make them glow too?
7. **A highlight roll-off** for the whole frame, so lamp-lit surfaces do
   not clip to flat white? (Tried 2026-10-24 and held: retail's own sun
   and moon pass full on 8 to 18% of the pixels, so the roll-off
   brightened them all, by up to 41 levels. The roll-off stays out.)

## Evidence

Run folders in `.tmp\plan-headlight\runs\` (each with
`start.json`, `out.txt`, `snap.png`). Sheets in
`.tmp\plan-headlight\sheets\`:

| Sheet | Shows |
| --- | --- |
| `01-hover-water-vulkan.png` | The lit beach stops at the waterline; the water stays dark (on above, off below). |
| `02-hover-water-opengl.png` | Retail's additive water shows the lamp. |
| `03-beach-facing-deep-water.png` | Lamp on, nothing lit, both renderers (difference images black). |
| `04-foldrak-pool.png` | Retail's pool on the snow, and its difference image. |
| `05-foldrak-first-person.png` | The same in first person. |
| `06-foldrak-dense-mist.png` | Thick mist: a smudge over the pool, no beam. |
| `07-crystals-stendahl.png` | Shards, lamp on (washed grey) and off (teal). |
| `08-crystals-terranova.png` | A clump in daylight, no visible change. |
| `09-crystal-sprite.png` | Sprite 1443 in the five banks: the same pale grey-teal. |
| `10-buggy-lamp-sprites.png` | The buggy's sprites: the rear lamps' discs and stars. |

Helpers used (scratch, not tracked): `beaches.py`, `shores.py`,
`sparse.py` (H12 sparse records), `view.py`, in the session scratchpad;
`diff.py`, `tile.py`, `probe.sh` in the probe folder; the world placement
export in `extract/`.
