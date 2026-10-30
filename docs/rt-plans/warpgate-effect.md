# The warpgate's energy effect: plan

Status: steps 1 and 2 of option (b) built. Step 1 (2026-10-23): the field
and the arc lights off. Step 2 (2026-10-24, `scene_gate.rs`): the bolts,
glints and hub light. The bolts are drawn by the raster right after the
traced world, as untextured additive strips depth-tested against it, not in
the composite: the raster already sits after the upscalers and the
denoiser, at the output's resolution, and needs no new shader. The light's
reach is 600, not the 900 first tried, for about 0.35 ms of trace instead
of 0.9. Step 3, the shimmer, waits on approval.
Code is as of `71eaa3703`. Other sessions are editing `pt_frame.glsl`,
`pt_trace.comp`, `modern_tuning.*` and `vk_device/trace.rs`, so this plan
names functions, not line numbers.

## What was asked

Another line: either make the stargate's effect like retail under Vulkan,
or come up with a better one, perhaps overriding the original energy beams
entirely with a higher-quality effect. Under Vulkan it looked worse than
retail, so it needs reimagining.

Earlier black areas were also reported where its LEDs are on the
warpgate. That was fixed on 2026-10-22, and is checked again below.

The "stargate" is the warpgate: selector 30, drawn by models `gate`
(H8/62) and `gatebod` (H8/63). The ground rules:
Only Vulkan with path tracing changes. OpenGL and the software renderer
keep retail's look. The game does not change. For the gate this means two
things:
- It turns on and off on the same tick as retail's.
- Its model program still runs every instruction. The program's jitter
  draws the game's RNG.

## How we looked

The probes ran hidden and silent (volume words 0) from a copy of the exe
in `.tmp\plan-warpgate\`. They used fixed 16 ms steps
and took snapshots at 3840x2160, converted to RGB.

A gate can be lit without any driving. Altair Prime's (world 5) start
gate stands at (29564, 46409). We placed the hovercraft 500 units in front
of it, facing it (`start-gate.json`: `{"world": 5, "start": 0, "form": 3,
"position": [29064, 46409], "heading": 90, "hints_seen": "all",
"upgrades": [1, 2, 165, 166, 167]}`). The gate is lit from the start (at
frame 45, the first we looked at). The older runs in `.tmp\vkgate\` needed
the vehicle to die and come back through the gate; this start does not.

The runs are deterministic, so the arcs have the same shape under both
renderers. A/B comparisons used a `modern-tuning.json` beside the
settings:
- `cfg\nofield\`: `materials.fields` empty.
- `cfg\noglow\`: the same, and no `gatebod` entry in `materials.glows`.

| Runs (`shots\`) | Scene | Renderers and settings |
| --- | --- | --- |
| `gl-`, `vk-`, `vkA-`, `vkB-` | The gate from 500 in front (`start-gate.json`). Frames 60 to 74 every 2, and 120. | OpenGL; Vulkan today; field off; field and arc lights off |
| `fp-` | The same, in first person (F12 at frame 20, `keys-fp.json`) | the same four |
| `side-`, `back-` | At an angle (heading 40), and from the far side | OpenGL and Vulkan today; field off at the angle too |
| `w14-`, `w17-` | Foldrak Noir (14) at night; Shak'tahh (17) under its orange sky. A buggy faces each world's start gate. | OpenGL, Vulkan today, field off |
| `d150` to `d1500` | The hovercraft 150 to 1500 in front of the gate | OpenGL |
| `ssaa-`, `rr-` | As `vk-`, at SSAA 4x (a typical setting) and with DLSS RR | `cfg\ssaa\`, `cfg\rr\` |
| `ir-` | Infrared on (upgrade 61, L at frame 20) | OpenGL, Vulkan |
| `drive-` | Driving through the gate (Up from frame 61). This world gate ends the world. | OpenGL, Vulkan |
| `rtaa-` | Vulkan without path tracing, with TAA | `cfg\rastertaa\` |
| `c-` (logs only) | 300 frames each, run twice, for cost | Vulkan today, field off, field and arc lights off |

The gate's program was listed from the model export
(`inf-extract models`, kept as `extract\gatebod-program.txt`), and its
sprites from `inf-extract sprites` (kept in `extract\gate-sprites\`).

## What retail draws (verified)

These facts come from the model, its sprites and the runs.

**The models.** `gate` (H8/62) only draws `gatebod` (H8/63) as a child
(opcode 0E). `gatebod` holds the ring, its feet and the effect:
- **The outer arcs.** Eight blended edges (opcode 07), 50 units to each
  side. They chain from the spike on top, down each side over three
  joints, to the feet. Each draw picks one of three ways to deal the
  lightning sprites 595, 596 and 597 among them. Small flares (opcode 1B,
  sprite 321) sit on the spike and the six joints, and one joint gets a
  larger flare each draw.
- **The spokes.** Ten blended edges, 40 units to each side. They run from
  the ring's nine inner corners to one hub. Every draw, the hub is the
  opening's centre jittered with the process RNG (opcode 0222, slot 120
  from slot 44). The sprites are again dealt one of three ways. The hub
  takes one flare per draw: sprite 321 (blue), 322 (yellow) or 323
  (green). The colour changes every tick or two (sheet 2).
- **The jitter.** The program opens with jitter instructions (opcodes 22
  and 0222) that draw the process RNG on every draw of the gate. The
  game's later random choices see those draws, so the program must keep
  running.
- **The panels.** Each foot's three-slot panel is a masked sprite (247).
  Its keyed slots let an additive glow (449) show through. The panels
  flash between violet and white (sheet 2).

**The sprites** (sheet 8). The lightning sprites 595 to 597 are 40 by 20
texels, additive and dim blue. Only 13 to 21% of their texels are lit,
the brightest at (0.35, 0.52, 0.87). The flares 321 to 323 are 32-texel
stars. From 500 units away at 4K, a spoke is about 400 pixels long, so
each texel of its sprite covers about 10 pixels. Retail's arcs are soft,
low-resolution strips.

**The stages** (sheet 6). These come from the runs and the gate's update
(`0048B3F0`, the port's `selector30/update.rs`):
- **Dark.** Nothing qualifies within 1200 units of the gate's middle. The
  gate draws no arcs and deposits no light: with the hovercraft 1500 from
  the face, the frame's light deposits numbered 39.
- **The crawl.** Something qualifies within 1200. The gate sets `+168`,
  hums, and deposits its own ground light. That light's radius is
  `+178 << 3`, and its colour (`+174`) blends green and blue with the
  clock. The outer arcs crawl round the ring. At 850 from the face the
  deposits numbered 40: the gate's light is the one more.
- **The spokes.** Closer in, the spokes and the hub flare join (at 700
  and nearer). The program reads the gate's nearness through its model
  callback (`0048B330`, selector 1: `(1200 - +17C) * 65535 / 1200`). We
  did not read out the exact threshold.
- **The crossing.** This world gate ends the world (to world 4, then the
  results). A linked gate moves the vehicle and flashes the world white.

**The colour** (sheet 5). Retail's arcs are additive: they add blue light
to the picture. Over Foldrak Noir's night sky they read blue. Over
Shak'tahh's orange sky they read pink-white.

## What Vulkan draws today (verified)

- **Retail's arcs and flares**, unchanged. The raster draws them after the
  traced world, depth-tested against it.
- **The field** (2026-10-22: `scene_field.rs`, and `fields_along` in
  `pt_composite.frag`). It is drawn in the composite, over the traced world
  and the sky and under retail's arcs, while `gatebod` draws blended edges
  (`materials.fields`: `["gatebod", 0.6, [0.45, 0.7, 1.0]]`). By the code
  it shows only in the spokes stage: it needs three arcs meeting at one
  point, and only the spokes give that.
  - Its shape: the hub is the end most arcs share. The plane is fitted to
    every other arc end, seen from the hub. The ellipse is inscribed in
    those ends' extent, and its centre is the middle of that extent.
  - Its look: value-noise filaments swirl in polar coordinates round that
    centre, with a white core (`exp(-7 s^2)`) and a rim. It is blended over
    what lies behind with an opacity of up to 0.9.
- **The arc lights** (`materials.glows`: `["gatebod", 0.3, 1000.0,
  [0.55, 0.75, 1.0]]`). Each additive arc's middle and each flare becomes
  a shadowed light. Lights from the same model within 250 units merge
  (`SceneFeedBuilder::glow`). They burn in both stages, the crawl included.

## Why it reads worse (verified)

1. **A veil, not light.** Retail's arcs add light. The field paints over
   what lies behind. Inside its footprint the mean colour goes from (88,
   77, 56) to (107, 115, 119) on Altair, and from (124, 55, 33) to (133,
   101, 104) on Shak'tahh. Blue doubles or triples. The sky and hills
   behind the gate turn a milky blue-grey (sheets 1 and 3). Just before
   the crossing, the way through is fogged (sheet 13c).
2. **Bigger than the opening, and over the ring.** The ellipse is fitted to
   every arc end. That includes the outer arcs' ends: the spike, the joints
   and the feet. So in the view from 500 units at 4K, it reaches 50 to 110
   pixels past the ring's outer edge, and down onto the ground between the
   feet. Wherever the ring's
   metal lies behind the field's plane, the field covers it: the inner
   faces with their green lights, and half the arch in a side view. A soft
   halo shows outside the ring (sheets 3, 4 and 11).
3. **Centred off the hub, with a pinwheel.** The field's middle is the
   middle of all the ends. That is above the hub where retail's spokes
   meet. Its polar noise pinches into a star of radial streaks there
   (sheets 3 and 4).
4. **It pumps.** From one tick to the next, its edge moves by about 50
   pixels and the area it covers changes between 0.51 and 0.85 million
   pixels (frames 60 to 74). Its plane is fitted round the hub, which
   retail jitters on every draw, and its filaments swirl.
5. **One cold colour in every world.** The field is a fixed pale cyan,
   painted over the picture. On Shak'tahh, retail's arcs read pink-white
   and the field is a cold blue disc among them (sheet 5b).
6. **A blown-out middle.** The field's white core adds to retail's hub
   flare. The result is a white blob larger than retail's (sheet 2,
   frames 62 and 64).
7. **Two kinds of motion.** Retail's arcs change shape every tick. The
   field's filaments drift smoothly (sheet 2).
8. **The arc lights.**
   - They cost about 2 ms of a 7 ms trace at 4K near the gate (RTX 5080):
     with them off, the trace took 5.0 ms instead of 7.0.
   - In first person they light the ground between the feet in
     hard-edged triangles. The lights sit just above flat terrain facets,
     so each facet takes its own facing (sheet 7).
   - They tint the metal blue, even under Shak'tahh's orange sky.
   - They also burn in the crawl stage, and they add to the gate's own
     ground light.

**Not the problem** (verified):
- **Retail's arcs over the traced world.** With the field off they look as
  they do under OpenGL, depth-sorted against the ring (sheets 1, and
  `2-motion-field-off.png`).
- **The upscalers and SSAA.** The field and the arcs are drawn after them.
  No AA, SSAA 4x and DLSS RR look the same (sheet 9).
- **Thermal vision.** The field is off under infrared, and the arcs read
  white-hot (sheet 12).
- **The LEDs.** We found no black areas from the front, the side, the far
  side or in first person (sheets 4, 10 and 11). The panels' keyed slots
  let the glow through, as fixed on 2026-10-22.
- **Vulkan without path tracing** (TAA) draws retail's arcs as OpenGL does
  (sheet 14).

## The options

### (a) Retail's look under Vulkan

Retail's arcs and flares, drawn as retail draws them. No field. The only
light is the gate's own ground light.

- **Build.** In the built-in tuning (`modern_tuning.json`), empty
  `materials.fields` and remove `gatebod` from `materials.glows`. That is
  exactly what `cfg\noglow\` does (sheets 1 and 7). The tuning test that
  expects `gatebod`'s field and tinted glow changes with it. If this
  is kept for good, delete the field's code (listed under
  [What changes where](#what-changes-where)).
- **Cost.** About 2 ms cheaper at 4K near a gate.
- **Risks.** None for gameplay. But it brings back retail's soft strips
  over a sharper traced world, so nothing at the gate looks remastered.
  The metal and the ground no longer catch the arcs' light. The gate's own
  green-blue pulse stays.
- **Gameplay.** Untouched: only the composite and the tracer's lights
  change.

### (b) Retail's lightning, redrawn

Under Vulkan, retail's arcs and flares are replaced by crisp ones in the
same places and at the same times. While the spokes show, a faint
heat-shimmer crosses the opening. It stays the same gate, with the same
rhythm: dark, then the crawl round the ring, then spokes to the hub as you
come close.

**What it draws**
1. **Bolts**, one for each arc retail draws, between retail's own end
   points. Each bolt gets a new shape every tick, as retail's arcs do.
   - The shape is a jagged line: two or three octaves of offsets across
     the arc, about 5% of its length.
   - It has a thin white-hot core, a pixel or two wide, and a soft blue
     halo about as wide as retail's visible squiggle.
   - Now and then a spoke throws a short fork.
   - The blend is additive, in retail's blue (the sprites' colour). Over
     warm skies the bolts turn pink-white, as retail's arcs do.
2. **Glints** in place of retail's flares, at the same points. Each takes
   the colour of the sprite retail picked that tick: blue, yellow or green
   at the hub. A glint is a small hot core with four to six fine rays.
3. **The opening's shimmer**, in the spokes stage only. What lies behind
   the opening wavers slightly: most at the hub, not at all at the ring.
   A few faint bright ripples run in to the hub. Nothing is painted over.
4. **One light** at the opening's centre while the spokes show. It is
   steady, in the bolts' blue, and shadowed. It lights the ring's inner
   faces, the ground, and a vehicle passing through. The gate's own
   green-blue ground light stays.
5. **Thermal**: bolts and glints white-hot, no shimmer.
6. **Optional: sparks** where the spokes strike the ring. These would be
   the renderer's own, never the game's particle pool or RNG.

**How it is built**
- **The tap** (`scene_capture.rs`) already sees each arc. For the gate's
  model it records each blended edge (its ends, sprite and width) and each
  1A/1B flare at an arc's end (its slot, sprite and size). It returns
  "fed" for them, so the raster skips them, and only while the world is
  traced. `blast_wave_under_water` already does this for another model.
  The model program itself runs every instruction. Its RNG jitter, its
  registers and the gate's state are untouched.
- **A gate builder** (`scene_field.rs` reworked):
  - It groups the arcs by gate, as today, and tells the spokes (sharing
    the hub) from the outer chain.
  - It takes the opening from the spokes' far ends only, the nine
    corners.
  - It builds each bolt's points for this tick from a hash of the gate,
    the arc and the tick.
  - It projects the points with the frame's camera.
- **The composite** (`pt_composite.frag`) draws the bolts, glints and
  shimmer in place of `fields_along`. This happens after the upscaler and
  the denoiser, at the output resolution. Each bolt is depth-tested
  against the traced depth and added to the picture. A small storage
  buffer carries the bolt points.
- **The light** is one glow at the opening's centre
  (`SceneFeedBuilder::glow`), in place of the per-arc glows.

**Cost** (estimates at 4K, to measure):
- **Trace.** One shadowed light in place of the arc lights: about 0.2 to
  0.3 ms instead of about 2 ms.
- **Composite.** About 20 bolts of 16 segments per gate, each tested only
  inside its own screen box. That is about 0.2 to 0.5 ms with the gate
  filling a third of the view. The shimmer adds about 0.05 to 0.1 ms.
- **CPU.** A few hundred points per gate per tick.
- **Net.** About 1 to 1.5 ms cheaper than today near a gate.
- **At 1440p with SSAA 4x** (2560x1440 with SSAA 4x), the trace and the
  composite cover 5120x2880 pixels, 1.8 times as many as 4K.

**Risks**
- **Taste.** Crisp bolts could look too sharp or too modern next to the
  game's art. Retail's colours, soft halos, modest widths and the tuning
  file keep this in check. Show sheets before each further step.
- **Readability.** The shimmer must never hide what lies behind. Its
  offset is a few pixels at 4K at most. It fades to nothing at the ring,
  and when the opening is seen edge-on.
- **Draw order.** The bolts are depth-tested against the traced world, so
  the ring's front and a passing vehicle hide them. But they draw under the
  raster's later effects: an explosion behind the gate draws over them.
  Retail sorted its arcs among those effects, back to front. Most of those
  effects are additive, so the difference is minor.
- **Upscalers and the denoiser.** The bolts are drawn after both, so
  nothing smears them. The light stays steady, so the denoiser's history
  holds. Check this with DLSS RR, FSR and XeSS.
- **Reflections.** The bolts will not show in water or metal. Retail's
  arcs don't today either. The light does show. If item 1's mirrored
  sprites land, the bolts can join them.
- **Frame rate.** The port draws once a tick, so at 240 Hz the bolts
  change at the tick rate, as retail's arcs do.
- **Infrared.** Item 15 is reworking it. The bolts must stay white-hot
  there.
- **Scope.** Only path tracing changes. OpenGL, the software renderer and
  Vulkan without path tracing keep retail's arcs.
- **Two gates in view.** Shak'tahh has two warpgates. The frame keeps the
  nearest two, as it does today.

**Gameplay.** The program still runs. Under the traced world, the raster
skips only the gate's arc and flare commands. The gate's update, sound,
light deposit, crossing and flash are untouched. The effect is keyed each
frame to the arcs retail draws, so it is on exactly when they are. Event
logs and OpenGL frames stay identical.

### Alternatives considered

- **Fix today's field in place.** Fit it to the spokes' corners only,
  centre it on the hub, and add light instead of painting over. Use noise
  without the polar pinch, hold its plane steady and take retail's colour.
  This is smaller and would fix the veil. But it stays a generic swirl
  over retail's soft strips, and the strips are what look dated at 4K.
- **A Stargate-style event horizon**: a rippling liquid sheet filling the
  ring, perhaps with a surge when it opens. That is not this gate.
  Retail's gate is a lightning ring that charges up. A sheet would hide
  the way through, and it is over the top for a tasteful remaster.
- **Bolts as traced geometry or line lights.** They would light and
  reflect physically. But they change shape every tick, so the denoiser
  and the upscalers would have no history for them, and they would smear
  and be noisy. Thin lines also alias at one ray a pixel. And a light per
  bolt costs what today's arc lights cost. So the light stays one steady
  traced light, and the bolts are drawn afterwards.
- **A bloom pass.** Nothing else needs one. A halo computed per bolt is
  cheaper and exact.

## Our recommendation

Option (b), built in three steps. Each step is shown as
before-and-after sheets.
1. **Retail's look first.** This is option (a), a tuning change. It
   removes the veil, the colour clash, the pumping, the facets and about
   2 ms. If retail's gate is preferred, stop here and delete the
   field's code.
2. **The bolts, glints and the hub light** replace retail's arcs and
   flares.
3. **The opening's shimmer**, if step 2 wants more.

Why: what looks worse today is the field, not retail's design. Retail's
gate has a good, recognisable rhythm: dark, crawl, spokes, then the
crossing. Only its art is dated: 40-by-20 strips stretched about ten
times at 4K. Redrawing the same rhythm crisply is the polish the final
stretch asks for, still recognisably Infestation. It also costs less than
today.

## What changes where

**Step 1, option (a)**
- `src/modern_tuning.json`: `materials.fields` empty, and `gatebod`
  removed from `materials.glows`.
- `src/modern_tuning.rs`: the defaults test's assertions on `gatebod`'s
  glow colour and field. Its tinted-glow case from a tuning string stays.
- Notes: the four places listed under steps 2 and 3 say the field and the
  arc lights are off.
- If (a) is final, delete the field:
  - `src/scene_field.rs`, and its `mod` line in `lib.rs`;
  - `SceneArc` and `SceneFeed::arcs` (`scene_feed.rs`);
  - the field half of `glow_edge`, and `model_field` (`scene_capture.rs`);
  - `Field`, `field_count` and `fields` (`pt_frame.glsl`, and the frame
    block in `vk_device/trace.rs`, which calls `scene_field::fields`);
  - `fields_along` and its uses (`pt_composite.frag`);
  - `MaterialTuning::fields` and `field_of` (`modern_tuning.rs`);
  - then `scripts/compile-shaders.ps1`: every shader that includes
    `pt_frame.glsl` gets a new `.spv`.

**Steps 2 and 3, option (b)**
- `src/scene_capture.rs`: in `capture_edge`'s blended branch and the 1A/1B
  branch, for the gate's model (named by the tuning, as now), record the
  primitive and return true. Test: the gate's arcs are fed only with a
  traced world, and other models' edges are not.
- `src/scene_feed.rs`: `SceneArc` gains its sprite and width, and a list
  of flares is added.
- `src/scene_field.rs`, reworked as the gate builder: spokes and chain,
  the opening, the stage, and each tick's bolts and glints, projected.
  Tests, on a synthetic gate as today's test builds:
  - it gives the spokes, the chain and the opening;
  - the same tick gives the same bolts, and the next tick different ones;
  - the chain alone gives bolts but no opening;
  - no arcs give no gate.
- `src/vk_device/trace.rs`, `shaders/pt_frame.glsl`: a gate record
  replaces `Field`. A storage buffer carries the bolt points and glints.
- `shaders/pt_composite.frag`: the bolts, glints, shimmer and thermal
  branch, in place of `fields_along`. Then `scripts/compile-shaders.ps1`,
  since the `.spv` files are tracked and the frame block is shared.
- `src/modern_tuning.rs`, `src/modern_tuning.json`: `materials.fields`
  becomes `materials.gates`: bolt and halo width, colour, the hub light's
  strength and reach, and the shimmer. `gatebod`'s glow entry goes.
- Notes:
  - `PIXEL_RUNTIME.md`: the field bullet, and the `gatebod` sentence under
    Glows;
  - `INFESTATION-RT-IMPLEMENTATION.md`: "The active warpgate";
  - the project objective.

## Cost

At 4K on the RTX 5080, with the gate filling the third-person view:

| | Trace | Composite | Source |
| --- | --- | --- | --- |
| Today | 7.0 ms | 0.25 ms | measured: the field about 0.02 ms, the arc lights about 2 ms |
| (a) | 5.0 ms | 0.23 ms | measured (`cfg\noglow\`) |
| (b) | about 5.2 to 5.3 ms | about 0.5 to 0.8 ms | estimate |

Another agent's probe shared the GPU for part of this. In repeat runs of
the same frames, the trace took 12.4 to 12.6 ms with the arc lights on,
against 7.1 to 8.7 ms with them off. The gap holds, but remeasure on an
idle GPU when building. SSAA 4x at a 4K output traced 7680x4320 pixels:
28.3 ms of trace, 1.12 ms of composite.

## Verification

- Rerun the same probes before and after each step, and set the sheets
  beside OpenGL's:
  - the gate at 500 (frames 60 to 74 every 2), first person, side and far
    side;
  - the stages at 150 to 1500 from the face;
  - worlds 14 and 17;
  - infrared;
  - the drive through the gate (the crossing frames);
  - DLSS RR, FSR, XeSS and SSAA 4x.
- OpenGL snapshots must stay byte-identical. The `--event-log` of a Vulkan
  run must stay identical too, since the game is untouched.
- Read the tracer's GPU line ("Vulkan tracer: ... GPU X ms a frame") on an
  idle GPU.
- Run the unit tests listed above.
- The probe, from `.tmp\plan-warpgate\` (Git Bash; it
  sets `TMP` and `TEMP` to `.tmp`). Copy the new release exe over
  `inf-game.exe` there first:

  ```sh
  ./probe.sh NAME vulkan cfg/cur start-gate.json 121 "60 62 64 66 68 70 72 74 120" [keys.json]
  # = ./inf-game.exe --no-launcher --start start-gate.json --data-root retail \
  #     --settings cfg/cur/run.json --profiles cfg/cur/profiles.sav --renderer vulkan --hidden \
  #     --width 1920 --height 1080 --fixed-step-micros 16000 --frames 121 \
  #     --snapshot-at 60:shots/NAME-60.png ...
  ```

  `python label_sheet.py OUT 2 0.6 1150 0 2700 1450 PNG=LABEL ...` crops
  the gate into a labelled RGB sheet.

## Open questions

1. Retail's look (a), or retail's lightning redrawn (b)? We recommend (b),
   with (a) as its first step.
2. The shimmer across the opening: wanted? If so, only in the close stage,
   as planned?
3. Sparks where the spokes strike the ring?
4. The gate's light: one steady light at the hub, as planned, or only
   retail's own green-blue pulse?
5. The hub's colour: keep retail's cycle (blue, yellow and green, changing
   every tick or two), or blue only?
6. Other rings use the same lightning sprites (595 to 597). One is
   `gatering` (H8/480), probably the course gates, the hoops on Artoria,
   Foldrak Noir, Kankutan and others; we have not checked which actor
   draws it. Redraw theirs the same way later, so everything looks alike?

## Evidence

The sheets are RGB, in `.tmp\plan-warpgate\sheets\`:
- `1-side-by-side.png`: OpenGL, Vulkan today, field off, and field and arc
  lights off, at frame 60.
- `2-motion-retail.png`, `2-motion-vulkan.png`, `2-motion-field-off.png`:
  frames 60 to 74, every 2.
- `3-field-alone.png`: the field's own contribution (today minus field off,
  doubled), on Altair and Shak'tahh. It shows the footprint past the ring
  and the pinwheel above the hub.
- `4-closeup-first-person.png`: the ring's inner face up close, veiled.
- `5a-night-world14.png`, `5b-red-world17.png`: the same comparison in
  other worlds.
- `6-retail-stages.png`: retail at 550, 700, 850 and 1500 from the face.
- `7-ground-facets.png`: first person, the ground between the feet.
- `8-retail-sprites.png`: sprites 595 to 597, 321 to 323, 449 and 247,
  enlarged.
- `9-upscalers.png`: no AA, SSAA 4x and DLSS RR.
- `10-leds-and-panels.png`: the left foot's panels, OpenGL and Vulkan.
- `11-side-and-back.png`: at an angle, from the far side, and in first
  person.
- `12-infrared.png`: retail's green, and the Vulkan thermal camera.
- `13-drive-through-retail.png`, `13b-crossing-retail.png`,
  `13c-crossing-vulkan.png`: driving through, and the frames before the
  crossing.
- `14-vulkan-raster-taa.png`: Vulkan without path tracing, with TAA.

In the same folder:
- `shots\`: the raw snapshots, which keep their alpha (convert to RGB
  before viewing);
- `logs\`: every run's output, including the cost runs `c-*`;
- `cfg\`: the settings and tuning variants;
- `start-*.json`, `keys-*.json`: the starts and inputs;
- `extract\gatebod-program.txt`: the gate's program;
- `extract\gate-sprites\`: its sprites;
- `probe.sh`, `label_sheet.py`, `crop.py`, `sheet.py`: the scripts.
