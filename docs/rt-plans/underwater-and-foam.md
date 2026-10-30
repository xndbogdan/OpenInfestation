# Underwater blasts and foam: plan

Status: plan for review (2026-10-23). Nothing here is built.
Code lines are as of `7686c4b2a`; another commit moved `pt_trace.comp` by
one line while this was written, so expect small drifts.

## What was reported

Explosions underwater do not look great, and agitated water goes white but
feels wrong.

## How we looked

The port ran as a probe (no window, no sound) from a copy of the exe in
`.tmp\plan-underwater\`, under Vulkan with path
tracing, fixed 16 ms steps, snapshots at 3840x2160. The usual setting is
2560x1440 with SSAA 4x (a `settings.json`, read only); one run at SSAA
4x (`r8/`) looks the same as the runs without it. Each finding below names
the run folder and frame. Side-by-side sheets of the key frames are in
`.tmp\plan-underwater\sheets\` (listed at the end).

| Run | Scene | Input |
| --- | --- | --- |
| `r5/`, `r8/` | Tabuchi's lake (world 9), hovercraft; a grenade lands about 2,300 units ahead and goes off under the water at frame 279 | `start-w9.json`, X held frames 100 to 150 |
| `r1/`, `r10/` | The same, a short throw: the blast is close (about frame 245), and it kills the hovercraft, whose own blast follows at about 274 | X held 100 to 116 |
| `r5gl/`, `r10gl/`, `d1gl/` | The same runs under OpenGL: retail's look | |
| `r6/`, `r9/`, `r10/`, `d1nf/` | A/B runs with the tuning file: the traced eruption off, the flash off, the foam off | `modern-tuning.json` |
| `d1/`, `r3/` | The hovercraft driving across the lake, and idle on it | Up held 120 to 300 |
| `w5/`, `w19/` | Altair's toxic water (5) and the Alien Brain's red liquid (19) | grenade |
| `u1/` to `u3/` | A buggy sinking in Altair's deep water (`start-w5.json`): the chase view, then first person | `u2/`, `u3/`: F12 at frame 5 |

## What we found

### Blasts in the water

Every grenade that lands in a lake goes off under the water: it sinks
toward a depth of 100 to 1,000 units that its handle picks
(`grenade26.rs:497-519`) and its 2 s fuse runs out there. Retail's water is
translucent, so retail shows the fireball and its ring through it, with
blue streaks flying out above (`r5gl/f280` to `f316`). The raster draws
the fireball and ring over the traced picture, and the traced water's
depth hides them (`r6/`), so the port draws an eruption in their place (`scene_spray.rs`,
`erupted()` in `pt_trace.comp`). What looks wrong:

1. **The flash is a flat glowing disc pasted on the surface** (verified;
   sheet 2, `r5/f280`, `r8/f282`; the vehicle's blast, `r1/f274`, sheet
   6). Four causes, all in `pt_trace.comp`:
   - It is added on top of the finished water (`return color + glow`,
     :1019), after the reflection, so it never looks like light coming up
     through the water.
   - It ignores how deep the blast is and how clear the water is. The
     eruption's record carries only its age and size
     (`scene_spray.rs:505`), and the glow (:909) uses neither depth nor
     clarity: a blast 1,000 units down flashes as brightly as one at 100.
     A blast floating on the surface (the hovercraft's death) also gets
     this "under the surface" flash.
   - Its centre is 2.5 times full brightness. The water's colour is
     stored in 8 bits, 0 to 1 (`albedo_out`, :43), so everything above 1
     flattens into one pale yellow plateau.
   - It stops at a fixed radius (the early return at :877), where it is
     still about 4% of its peak: the disc has a rim.
2. **A dark disc with a rim under it** (verified with the flash off,
   `r9/f282`, sheet 2). The dome tilts the water's normal outward
   (:884), which changes what the water mirrors, and the same early return
   cuts the tilt off while it is still strong.
3. **The spray reads as rice, confetti or pearls, not water** (verified;
   sheet 1 from frame 316 on, sheet 4 up close).
   - The drops are big: 2.5 to 5 units in radius, up to 8 in the dome
     (`scene_spray.rs:248`, :354, :385). With the spray's gravity of 980
     units a second squared (:31), a unit is a centimetre, so each drop is
     5 to 16 cm across. Seen at 2,300 units each is a distinct oval several
     pixels wide; up close they are ping-pong balls (`w5/f300`).
   - They are opaque and nearly one colour each. A white drop's colour is
     the light (:1072-1073) with only 30% of the sky mixed in at its rim
     (:1075). Even the "clear" drops start from that white, so they look
     like white beads rather than glass.
   - Each jet is a ball of 48 or 96 like drops around one moving point
     (`jet()`, `scene_spray.rs:215-270`), so the jets fly as separate
     clumps, like flocks, not as streams that stretch and break up
     (`r5/f340` to `f376`).
   - The spray is white whatever the liquid: white beads over the Brain's
     red liquid (`w19/f230`) and over Altair's toxic water.
4. **Retail's fireball is cut by the water line** (verified; sheet 3,
   `r10/f266` with the traced eruption off, and `r1/f266`). A close blast
   just under the surface shows a yellow block of fire with a straight
   lower edge. Retail draws each fireball as a flat, camera-facing picture
   at one depth (`process_particles/draw.rs:299-467`). The traced water's
   depth hides the part of that picture lying "behind" the surface, which
   is everything below one horizontal line. Retail's own water lets the
   whole fireball show through (`r10gl/f266`).
5. **The blue streaks are retail's** (verified): the same six streaks show
   under OpenGL (`r5gl/f284`), with the traced eruption off (`r6/f284`), and
   as blue sparks on land (`l1/`). Probably the grenade's blast visual
   (H13 resource 0x36, a selector-22 fading model, spawned at the blast
   tick; `SELECTOR26_RUNTIME.md`); that the streaks are its model is
   suspected, not checked. Over the traced white spray they read as neon.
6. **What a real underwater blast has and this lacks** (verified in the
   code): no glow seen through the water, no white mound of water as the
   blast breaks the surface (the "dome" is 80 small drops,
   `scene_spray.rs:372-399`), no churned, lighter water and bubbles
   afterwards, and only a faint foam patch.
7. **Suspected, minor**: the fireballs under the water still light the
   shore and the spray through the water as if it were not there (shadow
   rays skip the water), so the drops at the base turn orange.

### The camera under the water

Camera Style's Remastered (2026-10-28) now follows a sinking craft under
the surface; the software and OpenGL rasters draw V2000's underwater
wobble and a short fog in the water's colour there
([the remastered camera](../re/CAMERA_RUNTIME.md#the-remastered-camera-port-option)),
Vulkan's raster that murk over the world as above with the surface seen
from below ([the modern raster's water](modern-raster.md#the-water-stage-4)),
and the tracer's own medium (absorption from an eye under the water, the
plane past the mesh's reach seen from below) waits for a later
stage. Retail's,
and Normal's, still has none: retail's camera keeps its eye 32 to 80
units above the higher of the ground and the water
(`selector16/pose/terrain.rs:68-103`, called for every mode at
`pose.rs:340`), and it leaves first person when the vehicle sinks more than
256 units (`pose.rs:341-356`). In Altair's deep water the F12 eye stayed at
the surface until the buggy drowned (`u3/`). One caveat: that clamp reads
the waves at clock 0 (`selector16/native.rs:214-221`), while the drawn
water moves, so a passing crest could put the eye under the surface for a
moment. We did not see it happen.

### Agitated water turning white (foam)

The whitening is the traced foam: with `{"water": {"foam": 0}}` it is
gone and the water keeps its reflections (sheet 5, `d1/f330` against
`d1nf/f330`). It brightens the wake by half: the mean of the patch behind
the hovercraft goes from (61, 108, 79) to (94, 146, 109). The white
sparkles over it are retail's splash sprites, the same under OpenGL
(`d1gl/`). Why it looks wrong (all verified in `pt_trace.comp`):

1. **It is a veil, not foam.** The water is mixed toward one flat colour by
   a smooth amount (:1016), so the reflections fade evenly into a milky
   film (`r3/f250`, sheet 7). Real foam is lacy: solid white bubbles and
   streaks with clear water between them.
2. **It covers each splash's whole ring.** A splash's foam is a soft cone
   out to the ring's radius (:943), and the ring spreads from 24 to 360
   units times the splash's scale (:934). So each splash leaves a large
   soft disc that grows as it fades, instead of a patch where the water
   was hit that breaks up and drifts. Around the hovercraft, dozens
   overlap into a haze.
3. **It is lit as no other surface is.** Its colour is
   `capped(sun * 0.8 + ambient)`: no sun angle, no shadow test, no cloud
   shadow, and scaled to the brightest colour the game's light cap allows.
   So it glows next to everything else, in the sun's hue (pale mint on
   Tabuchi), even in shade.
4. **It has no texture.** Only the eruption's foam is broken up, by blobs
   about 60 units wide (:897-905).
5. **It ignores the liquid**: the same light-coloured film on red liquid
   (`w19/`) as on clear water.

## Proposed approach

Two parts. The first replaces what is wrong with the blasts; the second
replaces the foam, which the blasts then reuse. Game state, timing and
retail's particles stay as they are; only the Vulkan picture changes.

### Part 1: blasts in the water

**1a. Light under the water from retail's own fireballs (recommended).**
Retail's fireballs already put lights in its light grid while they burn,
and the tracer already gets each light with where it shines from, at its
own depth under the water (`world_scene.rs:1404-1428`). In `water_color`,
for each such light under the water's plane, add the glow the water
scatters toward the eye along the refracted ray (the ray bent into the
water, the part we see through the surface). A closed form is cheap: the
ray's closest approach to the light, the integral of 1 over distance
squared along the ray up to the bottom, and the water's attenuation over
the distance travelled, `exp(-path / clarity)`. The result goes into the
see-through part, before the Fresnel mix (Fresnel: how much the surface
mirrors, more at low angles), tinted toward the water's colour with
depth, and soft-clipped (`1 - exp(-x)`) so it never flattens at 1.

What this gives: a soft orange glow under the surface where the fireballs
are, bent by the ripples and rings, dimmer and greener the deeper the
blast and the murkier the water, timed by retail's own fireball animation.
It replaces the eruption's flash; `water.flash` becomes its strength.
Blasts on or above the surface give no underwater glow; their light keeps
glinting in the water as it does today.

Alternatives:
- Keep the eruption's flash and fix it: add the blast's depth to its
  record, move it into the see-through part, tint and soft-clip it, drop
  the cut-off. Similar cost, but a fixed shape and timing of our own
  rather than retail's fireballs.
- Show retail's fireball pictures through the water. The emitter list
  proposed in [effect-reflections.md](effect-reflections.md) (the blended
  sprites as records that reflected and refracted rays can meet) would do
  it: closest to retail's look, but it depends on that plan's work. 1a
  can come first and stay as the scattered glow around them.

**1b. Hide retail's fireballs under the water when the world is traced
(recommended).** In `draw_animation` (`process_particles/draw.rs`), a
particle of the explosions' class (`CENTRAL_VTABLE`, `splashes.rs:18`)
whose centre lies under the water's plane, over water (the test
`world_scene.rs:1346` uses), gets its command marked as `scene_fed`. The
raster already drops such commands only when the world is traced
(`software_frame.rs:1094`), so OpenGL and software are untouched. The
particle's flag write (:395) and its light (:354) stay, so the game sees
nothing different, and 1a shows the blast.

Alternatives: soft particles (sprites fade out where they near the
surface behind them, using the traced depth) fix every sprite that meets
the water or the ground, but the raster would need to read the traced
depth: a larger change, worth its own plan. Dimming and tinting the
fireballs instead leaves the cut edge.

**1c. No more hard rims.** Fade the dome's tilt and the rings to zero
before the early return in `erupted()` (or set the return's radius from
where each term has died out), so nothing ends at an edge.

**1d. White water instead of rice.** Keep traced drops (the first, volumetric version
was too gaseous, so everything stays opaque, with
crisp, lumpy edges), but change what they are:
- **A white mound.** For the first half second the surface over the blast
  turns to white water (Part 2's foam at full cover) and bulges (the dome's
  tilt). The 80 dome drops go.
- **Streams, not clumps.** Each jet becomes a stream along its path: a
  chain of overlapping, stretched blobs from the surface to its head,
  thick at the base and thinning toward the head, breaking into drops
  after the top of its flight. A few large blobs (10 to 30 units at full
  plume height) make the column read as one mass of white water near the
  base.
- **Fine drops at the edges.** Drops of 1 to 2.5 units, about as many as
  today in all, fringe the streams and fall back as spray. The tracer
  already shows a drop smaller than a pixel in as many frames as it
  covers (`pt_spray.glsl:35`, :64), which reads as glitter.
- **Shading.** White water: off-white, tinted toward the liquid (pink over
  red liquid, yellowish over Altair's), lit through its edges (wrap
  lighting: the lit side wraps a little around the shadow side), brighter
  when the sun is behind it, a noisy normal so the surface looks frothy,
  and each blob a little brighter or darker than its neighbours. Clear
  drops: the liquid's colour and the sky seen through and off them, a dark
  rim and a sharp glint, instead of a white bead.

Alternatives: a smooth implicit surface over the blobs (metaballs: one
surface wrapping nearby blobs) gives continuous columns but needs a ray
march per pixel, several times the cost; try it only if the blobs do not
convince. Textured spray pictures on the raster would not be lit or
shadowed by the tracer.

**1e. Churned water after the blast.** For three to five seconds the
eruption's patch is lighter, as water full of air is, with bubbles (cells
of Part 2's pattern) rising and popping, then breaking into lace and
fading.

**1f. Retail's blue streaks**: an open call (below). If hidden under
the water, the same mark as 1b on that model's draw.

### Part 2: foam

**2a. Where foam forms.** A splash leaves foam where the water was hit,
out to about a third of its ring (to tune), strongest when young and
fading with the splash's age; the ring itself stays a ripple. The
eruption keeps its patch, and its falls ring and foam where they land.

**2b. A foam pattern.** Cellular noise (a pattern of cells like bubbles)
over a slower patchy noise, drifting outward with the ring and changing
slowly with the clock. The foam amount from 2a picks how much of the
pattern shows: full amount, solid white; less, only the bubbles' walls
(lace); none, clear water. So foam dissolves as it ages, rather than
fading as a film.

**2c. Lit like the ground.** An off-white albedo (about 0.8; more of the
liquid's colour for coloured liquids), lit by the sun at its angle with
its traced shadow and the clouds' shadows, the sky's light and retail's
lights, under the game's light cap. A shadow ray is already traced for
the water's glint (:999); foam needs one wherever it shows.

**2d. Opaque where it is foam.** Where the pattern says foam, the foam
covers the water; between the bubbles the reflections stay, and the
sun's glint stops on foam.

**2e. Tuning.** `water.foam` stays its strength; add `water.foam_size`
(the bubbles' size, about 10 units). Nothing else unless asked.

Alternative, later: a foam map, a world-space texture around the eye that
splashes and wakes stamp foam into and that fades and drifts each frame.
Wakes would then linger behind the hovercraft after retail's splash
particles end. It is a new buffer and pass with state to reset on a world
change, so only if longer wakes are wanted after seeing 2a to 2d.

## What changes where

- `shaders/pt_trace.comp`
  - `erupted()`: drop the flash (`glow`), fade the tilt and rings before
    the cut-off (1c), full foam over the dome for its first half second
    (1d), the lighter churned patch (1e).
  - `splashed()`: the foam's shape (2a).
  - `water_color()`: the underwater glow from the lights (1a), added to
    `below`; the foam pattern, light and blend (2b to 2d) in place of
    :1016; no more `+ glow` at :1019.
  - `drop_color()`: the white water and clear drop shading, tinted by the
    liquid (1d).
  - New helpers: the glow integral, the foam pattern.
- `shaders/pt_spray.glsl` and `pt_depth.comp`: no change to the hit test;
  the record's last float can carry a blob's brightness beside its kind.
- `src/scene_spray.rs`: streams of blobs and fine drops (`jet()`,
  `throws()`), no dome drops (`dome()` goes), sizes; its tests follow.
- `src/process_particles/draw.rs`: the `scene_fed` mark for fireballs
  under the water (1b), with a test (one under the plane is marked, one
  above is not, other classes are not).
- `src/vk_device/trace.rs` and `pt_frame.glsl`: `water.foam_size` in the
  frame block; `eruption_flash` becomes the glow's strength.
- `src/modern_tuning.rs`, `src/modern_tuning.json`: `foam_size`; the
  meaning of `flash`.
- Notes: `PIXEL_RUNTIME.md` (modern mode: splashes, blasts in the water),
  the RT plan's Water section, the project objective.

## Order of work

1. 1c, 1a and 1b together: the first second of a blast stops being a
   disc. Show the before and after.
2. Part 2, the foam: tuned on the wake, the idle hovercraft, Altair and
   the Brain's red liquid.
3. 1d and 1e, the white water and the churned patch, using the foam.
4. 1f, per the answer to the open question.

## Cost

Estimates, to measure with the tracer's GPU line ("Vulkan tracer: ... GPU
X ms a frame") on the same probe runs before and after:
- 1a: a loop over the frame's lights (at most 64) for each water pixel,
  real work only for lights under the water, only while a blast burns.
  Well under 0.1 ms at 4K.
- 2b and 2c: a few noise lookups and one shadow ray per water pixel with
  foam on it, only near splashes. Around 0.1 to 0.3 ms at 4K with a busy
  wake.
- 1d: about the same number of drops and blobs as today, so about
  today's cost.
- SSAA 4x traces four times the pixels, so each of these costs
  four times as much there.

## Risks

- The spray was rejected once as too gaseous and now looks like rice.
  1d must land in between: show a frame sequence before tuning
  further.
- 1b hides retail's fireball under the water. The glow (1a) must still
  read as an explosion; it is an open call.
- Retail drops a particle's light when the particle is well off screen
  (`draw.rs`, the clip test before :354), so a blast just off screen
  gives no glow at the edge of the view.
- The light list keeps the 64 nearest lights; a busy scene could crowd
  out a far blast's fireballs.
- 1a leans on retail's dynamic lighting (settings `+80`, on in the usual
  settings). Whether particles still light the grid with it off is
  unchecked; if they do not, keep a small flash of our own as the
  fallback.
- Game state must stay bit-identical: 1b only drops a raster command. The
  checks below compare event logs and the OpenGL frames.

## Verification

- Before and after frame sequences, same inputs, same frames (the runs
  are deterministic at fixed steps). From `.tmp\plan-underwater\`:

  ```sh
  cp rt.json.keep run.json
  ./inf-game.exe --no-launcher --start start-w9.json --data-root retail \
    --settings run.json --profiles profiles.sav --renderer vulkan --hidden --width 1920 --height 1080 \
    --fixed-step-micros 16000 --frames 442 --input-replay keys-grenade-long.json \
    --snapshot-at 280:f280.png --snapshot-at 292:f292.png --snapshot-at 316:f316.png > out.txt 2>&1
  ```

  `start-w9.json` is `{"world": 9, "start": 0, "form": 3, "position":
  [51978, 37383], "heading": 0, "hints_seen": "all", "upgrades": [1, 2,
  165, 166, 167], "secondary": 8, "ammo": {"8": 20}}`;
  `keys-grenade-long.json` presses X at frame 100 and releases it at 150.
  - The eruption from above: frames 276 to 440, every 4 (`r5/`).
  - A close blast and the cut fireball: `keys-grenade.json` (X 100 to
    116), frames 248 to 266 (`r1/`, `r10/`).
  - A blast on the surface (the hovercraft's death): frames 274 to 300 of
    the same run.
  - The wake and idle foam: `keys-drive.json`, frames 150 to 330 (`d1/`);
    no input, frames 250 and 290 (`r3/`).
  - Coloured liquids: `start-w5h.json`, `start-w19h.json`, frames 150, 230,
    300 (`w5/`, `w19/`).
  - One run at SSAA 4x.
- OpenGL frames byte-identical before and after (`r5gl/`, `d1gl/`), and
  `--event-log` identical for the Vulkan run (the game is untouched).
- Unit tests: `scene_spray.rs`'s tests (rise and fall, rings, sources
  holding their drops) updated; the new fireball mark test.

## Open questions

1. **Retail's blue streaks** over a blast in the water: keep them as
   retail has them, tint them to the spray's colour, or hide them under
   the water as the white spray replaces them?
2. **Size and length**: the plume rises about 800 units (8 m) at full
   strength and the whole eruption lasts 4.5 s. Right for a grenade?
   Bigger for a vehicle's death?
3. **Coloured liquids**: should the Brain's red liquid throw pink spray
   and foam (as a real liquid would), or keep white?
4. **Foam**: how white and how long? Should wakes linger behind the
   hovercraft (the foam map), or end with retail's splashes?
5. **Retail's fireball under the water**: hide it and show the glow
   (recommended), or keep it, dimmed?

## Evidence

Sheets (in `.tmp\plan-underwater\sheets\`):
- `1-eruption-sequence.png`: the eruption from above, frames 280 to 428.
- `2-flash-ab.png`: the flash disc; flash off (dark tilt disc); eruption
  off (only retail's streaks); retail under OpenGL.
- `3-sliced-fireball.png`: retail's fireball cut at the water line,
  retail's look, today's look.
- `4-closeup-drops.png`: white beads over red liquid; pearls up close.
- `5-foam-ab.png`: the wake under OpenGL, Vulkan, Vulkan without foam.
- `6-vehicle-blast-disc.png`: the disc under the hovercraft's own blast.
- `7-idle-foam.png`: the milky film round the idle hovercraft.

Frames: `r5/`, `r8/` (SSAA 4x), `r9/`, `r6/`, `r5gl/`, `r1/`, `r10/`,
`r10gl/`, `d1/`, `d1nf/`, `d1gl/`, `r3/`, `w5/`, `w19/`, `u1/` to `u3/`,
`l1/`, in the same folder. Snapshots keep their alpha: convert to RGB
before viewing.
