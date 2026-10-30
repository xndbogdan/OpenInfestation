# The ACAM's gun pods: plan

Status: parts 1 to 3 built (2026-10-23): the tuning, the twins for the
tracer and for Vulkan's raster corners,
with the measurements below. 2D's callbacks keep whole units (none of
the pods' slots take them).
Code lines are as of `8c3903afe`; other sessions are editing the tree, so
expect small drifts.

## What was asked

The ACAM's guns wobble against its body like a PS1 game, the paired side
modules (the plasmas) most of all, so the vehicle does not read as one
piece. Under the Remastered infrared they also look wrong.

Ground rules (2026-10-23): a port bug is fixed for every renderer. If the
wobble is retail's own, OpenGL and the software renderer keep it, and only
Vulkan holds the guns still. The game does not change.

Words used below:

- **Pod**: one weapon module on the hull. The forms carry them as separate
  small models, `silo1` to `silo8` (H8 142 to 154). The plasmas (primary 1,
  weapon model 7) and the other side weapons come as a left and a right
  pod.
- **Slot**: one of a model's working vertices, which retail keeps in whole
  world units.
- **Hop**: a pod's jump against the hull from one frame to the next.

## How we looked

The port ran as a probe from `.tmp\plan-guns-thermal\`
(hidden, silent: volume words 0), at 3840x2160. Retail ran in the scenario
lab. Each run folder is under `runs/`, with its `start.json`, `keys.json`
and log.

| Run | Scene |
| --- | --- |
| `A_gl_16`, `A_gl_33`, `A_gl_8` | Terra Nova (world 2), start 0, the buggy with the plasmas (`"upgrades": [1, 2, 165, 166, 167, 155, 152, 161, 158], "primary": 1`). Up held, then Left: a driving turn. OpenGL at 16, 33 and 8 ms steps, the keys scaled so each covers the same game time. |
| `A_vk_16`, `A_vk_33`, `A_vk_8` | The same under Vulkan with path tracing, no upscaler. |
| `A_dlss_16` | The same with DLSS Quality, as usually played. |
| `B_gl_16_straight` | Up only: driving straight over rough ground. |
| `C_vk_16_hover` | Tabuchi (world 9), the hovercraft with the plasmas, Up then Left. |
| `R_retail_1024` | Retail `Infest.exe` in the lab (`lab/retail-mounts.json`): the same start and keys at 1024x768, 16 ms passes. |
| `T2_*`, `T9_*` | Infrared vision on (thermal-vision.md): the ACAM under retail's green and the thermal camera. |
| `T2_vk_fix`, `T9_vk_fix`, `N2_vk_fix` | The same with a tuning file that fixes the pods' heat and material (`tuning-fix.json`). |

**How the hop was measured.** `track.py` follows four patches from frame
to frame by correlation, to a tenth of a pixel: the hull's top (the
reference), a second hull patch (the control), and the two pods. A pod's
hop is its motion less the hull's, less a five-frame median (the turn's
slow swing). The control shows what a rigid part of the same model does.

## What we found

### 1. The pods hop against the hull (measured)

At 4K, root mean square of the hop in pixels, the largest in brackets
(`sheets/06-mount-jitter.txt`):

| Run | Control | Left pod | Right pod |
| --- | --- | --- | --- |
| OpenGL, 16 ms | 1.1 | 3.5 (9.5) | 3.3 (7.1) |
| OpenGL, 33 ms | 1.6 | 3.5 (7.8) | 4.5 (9.9) |
| OpenGL, 8 ms | 1.1 | 3.5 (9.5) | 3.3 (7.2) |
| Vulkan, 16 ms | 1.1 | 3.5 (11.1) | 3.1 (6.8) |
| Vulkan, 33 ms | 1.6 | 4.1 (7.7) | 4.2 (11.3) |
| Vulkan, 8 ms | 1.1 | 3.5 (11.1) | 3.0 (6.8) |
| Vulkan and DLSS, 16 ms | 1.2 | 3.6 (11.0) | 3.1 (6.6) |
| OpenGL, straight, 16 ms | 0.9 | 2.1 (5.3) | 2.3 (5.8) |
| Vulkan, hovercraft, 16 ms | 1.2 | 3.2 (8.6) | 3.1 (8.9) |
| Retail, 1024x768 | 0.7 | 1.1 (3.2) | 1.3 (2.8) |

In half of the frames a pod moves more than 2 pixels against the hull.
The two pods hop at different moments. The black skirts that join each pod
to the hull (sprite 233) stretch and shrink with each hop, so the seam
opens and closes (`sheets/01-vulkan-4k-mount-hops.gif`, the hull held
still).

### 2. Not frame pacing, not the renderer (verified)

- The hops are the same at 8, 16 and 33 ms steps.
- The port interpolates nothing. A frame whose world tick was skipped is
  neither drawn nor presented (`holds_gameplay_frame`,
  `session_platform.rs:1123`): at 8 ms every other snapshot repeats the
  one before. One model program draws the hull and its pods from the same
  tick.
- OpenGL and Vulkan hop alike. Both take retail's whole-unit points:
  OpenGL through the projected slots, which then also snap to the
  1365x768 native pixel (2.8 screen pixels), and the tracer through the
  slots' view points (`scene_capture.rs:240-277`).
- DLSS adds nothing measurable, though its motion vectors follow only the
  camera (`pt_resolve.comp`). No ghosting showed on the vehicle.

### 3. Retail places the pods in steps of two units (verified)

- **Where a pod goes.** Each form draws its pods as children (`0E`) at a
  slot of its own (`model_execution.rs:1168-1179`). The child's origin is
  that slot's world point, in whole units. Pods 142 to 154 are children of
  the forms 118 to 127 and of nothing else.
- **How the slot is made.** Every form computes the pods' slot with opcode
  26, a blend between two hull points: the buggy's left pod at
  `0F26 [114, 17, 18, reg 10]`, its right pod at 115 from slots 16 and 19;
  the hovercraft at 102/103 from 27/28 (`tools/models.txt`). Register 10 is
  register 7 capped at `0x5FFF` (`F7 [10, reg 7, 0x6000]`), and register 7
  follows the second half of the weapon's deployment (`+210`, model value
  7). Deployed, the blend's weight is `0x5FFF`, 37.5%.
- **Retail's blend.** `004EC950` multiplies the difference by the weight,
  keeps the high word, and doubles it (`IMUL; SHL EAX,1; RCL EDX,1;
  LEA [left + EDX*2]`, read through REA). The result moves in steps of two
  whole units and errs up to two units low on each world axis. The port
  does the same (`interpolate`, `model_vertices.rs:1616`).
- **More rounding on top.** Both hull points are whole units (`00490900`),
  the child's view position is rounded (`0048FB40`,
  `model_context.rs:116-139`), and so is each vertex
  (`model_projection.rs:202`).
- **Size.** `tools/mountsim.py` runs the port's arithmetic over a full
  turn: a deployed pod's origin is off by -2.4 to +0.4 units on each axis,
  3.4 at most, and jumps two units every 0.66 degrees of heading (543
  jumps a turn). Rounding alone would leave at most 0.87.
  (`sheets/07-mount-origin-sim.txt`)
- **On screen.** At 4K the chase camera shows the buggy at about 3.3 pixels
  a unit (the pods' outer edges, 185 units apart, span 610 pixels), so a
  hop is about 7 pixels.
- **Retail's own frames.** In the lab at 1024x768 the pods hop 1.1 to
  1.3 pixels against a control of 0.7. Scaled to 4K (2.8 times) that is
  3.1 to 3.6 pixels, the port's numbers
  (`sheets/02-retail-1024-mount-hops.gif`). At 640x480 a hop was about a
  pixel, which is why it never showed in 2000.
- The axles are placed the same way (`0F26` at slots 124 to 127) and hop
  too, less visibly.

The port's `vehicle_gun_mounts` fix is unrelated: it only changes which
hull points the right pod's skirt copies during the hover morph.

### 4. The game reads the pods' slots (verified)

Each pod hands its muzzle slot to the vehicle as it draws: `222D
[18, 2, 16, 0]` in pod 148 (`282D [18, 2, 1, 1]` in 149) stores slot 16's
world point in the vehicle's behaviour group 0 (or 1)
(`capture_model_behavior_group`, `model_custom_vertex.rs:467`). That is
where the plasma bolts start. So retail's whole-unit slots must stay as
they are. A fix can only change what is drawn.

### 5. Why the guns look wrong under the Remastered infrared (verified)

- **Wrong heat.** The thermal table gives heat by model name. No pattern
  matches `silo*`, so the pods take the default 0.35; the buggy's hull
  (`wheelmode*`) has 0.45 and the wheels 0.52. On the hovercraft the hull
  (`hoverjet1`, `hoverjet2`) matches `hoverjet*`, which the table lists
  with the jet engines at 0.95. Measured grey: on Terra Nova the hull 197,
  the pods 150 and 158; on Tabuchi the hull 255 (clipped white), the pods
  100 (`sheets/04-guns-under-infrared.png`, `05-hover-guns-...`).
- **Wrong material, in the normal view too.** The materials table files
  `silo*` with the buildings (metalness 0.35, roughness 0.5); the hull is
  0.6 and 0.35. The pods shine less than the body they sit on.
- **Glow panels by brightness.** The pods' charge panels (sprites 469, 470,
  477; 410, 412, 416) are additive, so they stay on the raster, and the
  thermal camera shows them by brightness: white blocks on the dark pods
  while the plasma charges and fires.
- **Hops show more.** Dark pods on a white hull, with the camera's
  sharpening (0.6), make each hop stand out.

A tuning file alone (`tuning-fix.json`: `silo*` at the hull's heat and
material, `hoverjet*` off the engine list) brings the pods to the hull's
grey: 196 and 204 against 197 (`sheets/08-tuning-fix.png`).

## Options

1. **Exact geometry under Vulkan (recommended).** Beside retail's
   whole-unit slots, keep exact ones for drawing. Every model drawn by
   Vulkan becomes rigid and smooth; the game keeps retail's numbers.
2. **Exact pods only.** Make opcode 26 and the children's origins exact,
   nothing else. It removes the two-unit steps, but each vertex and each
   model's position still round, so the pods keep moving about a unit
   against the hull (the control's 1.1 pixels). It saves little: the same
   functions change.
3. **Smooth the pods over time.** Filtering the pods' screen positions lags
   behind the hull in turns and is still not rigid. Rejected.
4. **A port fix for every renderer.** Not wanted: the hops are retail's,
   and OpenGL and software keep retail's look.

## Recommended design

### Part 1: exact twins of the slots

`ModelVertices` already keeps a non-native side table for the scene feed:
each slot's view point (`views`, `model_vertices.rs:266`), copied with the
slot (`:1305`, `:1484`). Beside it, the store keeps exact twins, computed
by the same formulas without retail's rounding:

- **Raw vertices:** the object's position plus the basis times the local
  point, in f64 (the twin of `00490900`).
- **Opcode 26:** `left + (right - left) * weight / 65536` on the twins;
  23 the plain midpoint; 25 the plain offset.
- **Copies** (2A from the parent, slot to slot): copy the twins, as `views`
  already are. The skirts' hull corners therefore stay on the hull.
- **Children (0E):** the child's origin is the parent's exact twin of the
  slot; its transform carries an exact view position beside the rounded
  one (`ModelContext::exact_position`, `ModelTransform::exact_view_position`).
- **Projection:** the projector gives the exact view point and the exact
  screen point (the same perspective, without FISTP).
- **Callbacks** that write slots (opcode 2D, such as the wheels placed on
  the ground): their whole-unit output moves by the rounding of the slot
  they read (exact minus whole), so they follow the hull. The ground
  contact itself stays in whole units.
- A slot written without a twin falls back to its whole-unit point.

The twins are filled only when the frame is Vulkan's. Nothing in the game
reads them.

### Part 2: who draws from the twins

- **The tracer.** The tap takes the exact view points
  (`scene_capture.rs:240-277`, glow faces, edges, billboards).
- **Vulkan's raster.** Each raster corner of a model (the TL x and y the
  `d3d_*` builders set from the slot, as `d3d_textured.rs:159` does)
  takes the exact screen point. Clipping keeps deciding from retail's
  numbers and carries the exact point as one more attribute.

The raster part is needed even with path tracing. A traced model's
blended faces (the pods' charge panels, the lamps) stay on the raster. If
the traced pod moved to its exact place and its panel did not, the panel
would slide up to ten pixels off the pod.

OpenGL and the software renderer never fill or read the twins: their
frames stay byte for byte as now.

### Part 3: the tuning (now, independent)

In `modern_tuning.json`:

- thermal: `silo*` at the hull's 0.45; `hoverjet*` off the engine list
  (the hovercraft's hull then takes `hov*`'s 0.45), and its fans
  (`scatfan`) added to it beside `thruster` and `mainjet`;
- materials: `silo*` with the hull (0.6, 0.35).

A better rule, for the thermal plan: a child model takes its object's heat
and material unless its own name is listed.

## What changes where

- `src/model_vertices.rs`: the twins beside `views`, written by every slot
  writer (initial ranges, 23, 25, 26, 2A, 2B, 2C, 2D), cleared with them.
- `src/model_projection.rs`, `model_projection/process.rs`: the exact view
  and screen points.
- `src/model_context.rs`: the exact position and view position.
- `src/model_execution.rs` (children), `model_execution/late.rs`,
  `src/model_auxiliary.rs` (the auxiliary 18, 2C and 2F run the same
  owners).
- `src/scene_capture.rs`: exact points for the tracer.
- `src/d3d_*.rs` TL builders and `model_primitives/*clipping*.rs`: the
  exact corner under Vulkan.
- `src/world_scene.rs`, `src/frame_device.rs`: switch the twins on for
  Vulkan frames.
- `src/modern_tuning.json`: part 3.
- Notes: `MODEL_RUNTIME.md` (exact geometry under Vulkan, beside the
  native rounding it leaves alone), `VEHICLE_DRIVE_RUNTIME.md` (the pods'
  placement and its two-unit steps), `PIXEL_RUNTIME.md`, the RT plan.
- Tests: opcode 26's twin against its whole-unit result at weight
  `0x5FFF`; a child's origin from the parent's twin; with twins on, the
  slots and the behaviour groups unchanged; the tap and the raster reading
  twins under Vulkan only.

## Order of work

1. The tuning (part 3), with a thermal snapshot.
2. The twins and the tracer.
3. Vulkan's raster corners.

Each step ends with the measurement below and a commit.

## Cost

CPU only: a second, floating-point pass over the vertices of drawn models,
a few thousand a frame, tens of microseconds. No GPU cost. The tuning
costs nothing.

## Risks

- **Seams.** Two separate models whose points met only after rounding may
  part by under a unit. Copied points carry twins, so the skirts stay
  joined. Check the form morphs, which blend between hull shapes.
- **Clipped raster polygons** at the screen's edge may differ by a fraction
  of a pixel from where retail clipped them.
- **The vehicle on the ground.** The wheels touch the ground in whole
  units; the exact hull may sit up to half a unit off them (about two
  pixels at 4K). Check in snapshots.
- **A missed writer** leaves its slots rounded. The tests list the
  writers.
- **Shared code.** The round-models plan also reshapes the tap's faces;
  both read the same corners.

## Verification

- Rerun `A_vk_16`, `A_dlss_16` and `C_vk_16_hover` and track them: the pods'
  hop falls to the control's (about a pixel or less), and the control
  falls too.
- The stabilised strips again: the pods stay put on the hull.
- OpenGL and software: `A_gl_16`'s frames byte for byte as before; the
  event logs of the OpenGL and Vulkan runs identical to before, the bolts'
  origins included.
- Thermal: the pods at the hull's grey on Terra Nova and Tabuchi.
- `cargo test -p inf-game` with the new tests.

## Open questions

1. **Vulkan without ray tracing.** Rigid pods there too (our pick: the
   same twins do it), or retail's corners there as well?
2. **OpenGL keeps retail's hops** (our pick, as agreed). They are a pixel
   at 640x480 and seven at 4K. Confirm.

## Evidence

Run folders in `.tmp\plan-guns-thermal\runs\` (the
strips keep the tracked frames; the 8 ms runs keep one copy of each
repeated frame). Sheets in
`.tmp\plan-guns-thermal\sheets\`:

| Sheet | Shows |
| --- | --- |
| `01-vulkan-4k-mount-hops.png` / `.gif` | Vulkan, 12 frames with the hull held still: the left pod hops. |
| `02-retail-1024-mount-hops.png` / `.gif` | Retail at 1024x768: the same hops, a third the size. |
| `03-opengl-4k-mount-hops.png` / `.gif` | OpenGL: the same as Vulkan. |
| `04-guns-under-infrared.png` | Terra Nova: retail's green, then the thermal camera, still and firing. |
| `05-hover-guns-under-infrared.png` | Tabuchi: the white hovercraft with grey pods. |
| `06-mount-jitter.txt` | The tracking numbers for every run. |
| `07-mount-origin-sim.txt` | The simulated error of the pods' origin over a turn. |
| `08-tuning-fix.png` | Thermal and normal views before and after the tuning file. |

Helpers (scratch, not tracked) in the same folder: `probe.sh`,
`matrix_a.sh`, `track.py`, `stabilize.py`, `view.py`, and in `tools/`:
`mountsim.py`, `mlist.py` (model listings from the extracted corpus),
`peread.py` (bytes from `Infest.exe`) and `models.txt` (the forms and
pods).
