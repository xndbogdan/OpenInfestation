# Atmosphere: ambient sound, weather and drawn wind: plan

Status (2026-10-24): steps 0, 1, 3 and 5 built, 5 with its Headphones
(HRTF) option. The groundwork: the ambience
bus and its limiter, the Ambience volume, `Remastered.pak`, its builder and
manifest, `--audio-out`. The beds: every world's from the table below, with
their sweeteners, the ducking, the Brain's pulse, Artoria's power-down and
the light-following beds of 1 and 15. As built, departing from this plan
for now: the sweeteners are panned around the listener (the 3D placement
comes with the rest of it); the Brain's pulse keeps its own clock (the
sky's tissue clouds are held with its Stage 2); the canyon and metal echoes
of 7, 13 and 16 are the reverb's presets; the network arenas have no
beds yet; the loops decode whole on a thread as a world starts (about
50 MB, 100 MB on Artoria), the fallback "Streaming" names, rather than
streaming. The placed sources: the transformer is `util04`, `gentrans` Foldrak Noir's lightning spheres; Luna Azine's dome is
a force field (selector 18), the Defence Satellite's laser grid its
barriers (selector 69); no drill was found, and Sanrel-Dek's lava rivers
stay unmarked. The sound sheets, a minute of each world, wait for listening
(`.tmp/ambience-sheets/`). The rest follows the order of work below.
Code references are to `main` at `989fe40f6`.

## What was asked

Sounds like wind, rain, thunder, and maybe droplets hitting the ACAM and
other metallic structures: a sound pack for games to buy and include.

How games make sounds feel real, directional and proper, immersing the
player in the atmosphere. A full atmospheric pack is wanted, since there is
always background sound on a planet; even the Alien Brain's can be
something more ominous. On planets with worse weather some wind is drawn
too. Not extremely realistic, since the game is not, and not full cartoony
either: somewhat of a mix. The water belongs to the same plan.

## Our recommendation, in short

- **Every world gets a bed**: a quiet, never-ending background (air, wind,
  a hum, the Brain's drone). Retail has none: under its music and away
  from machines a world makes no sound of its own, and with the music off
  it is silent. Silence reads as broken.
- **Weather you can hear is weather you can see.** Rain, gusts and thunder
  come from the same weather state as the sky's rain, clouds and
  lightning. Thunder follows each flash, later and duller the farther the
  flash.
- **Sounds placed in the world**: shores lapping, the Brain's liquid, lava,
  and the machines retail leaves silent (generators, the power
  transformer, drills). Retail's own hums (warpgates, factories, engines)
  stay as they are and are not doubled.
- **Rain on the ACAM's hull and on metal near you**, single drops pinging
  on it, and in first person the hull around you.
- **Placed well**: smooth panning by direction, far sounds quieter and
  duller, muffled behind hills, and the echo of the place (reverb).
- **Style**: real recordings, edited and layered, a notch exaggerated
  (punchier thunder, a cleaner drum of rain on the hull), fewer layers
  than reality, no cartoon effects, no animal calls.
- **The switch**: an Ambience volume of its own beside retail's Sound and
  Music (0 turns it off and gives back retail's sound exactly), on by
  default with Remastered (Vulkan). Vanilla (OpenGL) and the software
  renderer keep retail's soundscape exactly. Everything the port adds
  plays at the Ambience volume, thunder included.
- **Headphones**: plain stereo by default; HRTF (see below) only as an
  option, last.
- **Drawn wind** only where the weather is rough (Tabuchi's storm, the
  snow worlds, the dusty red worlds, the volcanic ash): low over the
  ground, subtle, never hiding an enemy. One wind per world moves the
  clouds, the rain, the snow, the dust, the ripples and the sound of the
  gusts together.
- **Order**: the groundwork, then the beds, then the weather's sounds with
  the sky's weather, then placed sources, then rain on the hull and on
  metal, then reverb and occlusion.
- **Sounds**: the local files first (mapped below: they cover the Brain,
  the airless worlds, the machines and part of the storm), the gaps from
  the free Sonniss GDC bundles and CC0 packs, a paid thunder pack only if
  the storm falls short.

### Details the findings settle

- **Thunder plays at the Ambience volume**, not the effects volume: one knob turns all of the port's sound down, and at 0 the game
  sounds exactly as retail.
- **Weather sounds need the weather drawn** (Vulkan with path tracing),
  not merely Remastered: no rain heard that cannot be seen.
- **No under-water bed**: retail's camera stays above the surface.
  Instead, retail's blasts under the water can be muffled.
- **No waterfalls**: the game has none.
- **Only the silent machines get sound**: retail already hums its
  warpgates and factories; the generators and the transformer have
  nothing.
- **A limiter from the first bed**: retail's effects alone already reach
  full scale.
- **Thunder's delay comes from each flash's distance class**, not the
  world's geometry: a world is too small for real delays.
- **The flash schedule moves to the CPU**, shared by the sky and the
  sound: a small change to the sky plan.
- **Drawn snow and dust** go into the mist and the composite's layers, not
  traced drops: every pixel would test thousands of flakes.
- **A groundwork step comes first**, and the weather's sounds follow the
  sky's own steps.

## How it was checked

- **Read**: the port's mixer and sound owners (`platform_audio.rs`,
  `platform_audio/lifecycle.rs`, `process_sound.rs` and its folder,
  `frontend_audio.rs`, the actors' sound owners), the retail notes
  ([AUDIO_RUNTIME.md](../re/AUDIO_RUNTIME.md),
  [VEHICLE_DRIVE_RUNTIME.md](../re/VEHICLE_DRIVE_RUNTIME.md)),
  the sound bank's manifest (`extracted/sounds/manifest.json`),
  the plans this one leans on ([the sky](sky-per-world.md),
  [shorelines](water-shore.md), [blasts and foam](underwater-and-foam.md),
  [water tiling](water-repetition.md), [fog](fog-and-pop-in.md)).
- **Decoded**: each world's music track from its world header (H14 WORD
  `+14`), with `h14_music.py` over the extracted regions
  (`extracted/opaque_resources`). The live probe below
  confirmed Tabuchi's.
- **Counted**: every world's placed actors with the resident inspection
  (`inf-game.exe --mode resident --activate 47,48,<world+10>
  --inspect-world 0 --json`), worlds 1 to 24, summed by `placed.py`.
  Everything is in `.tmp\plan-atmosphere\` (`w*.json`,
  `placed.txt`, the scripts, a copy of the exe).
- **Measured**: the mix's peak in three silent probes on Tabuchi's lake
  (hovercraft idling on the water, realtime, hidden, OpenGL, 1,200
  frames): Sound and Music at 15, Sound only, Music only
  (`hover-w9*.txt` in the same folder).
- **Read only**: the headers (length, rate, channels) of the local
  sound files, with `audiohdr.py` (`python -I`), nothing copied.
- **Not done**: no listening (nothing here judges how a sound sounds), no
  count of terrain furniture (the transformer is furniture), no timing of
  the mixer's CPU. See [Not checked](#not-checked).

## What we found

### 1. Retail's sound bank is 58 short clips, the same in every world (verified)

- One bank for the whole game: 77 slots in `0.ovl` (30) and `1.ovl` (47),
  of which 58 are recordings and 19 are aliases (a slot that plays another
  slot's recording with its own pitch and random spread). No world brings
  sounds of its own (`extracted/sounds/manifest.json`, `totals`).
- Every recording is 22,050 Hz, mono, 16-bit. The longest is 4.8 s (slot
  46); most are under 2 s. 2.5 MB in all.
- So retail has no ambience to keep: no wind, rain, water or lava
  recording could be a long loop. That the 58 clips hold none of these is
  suspected, not heard: we listened to nothing.

### 2. What each world plays today (verified)

**Music.** Each world picks one CD track from its header: track = H14
WORD `+14` + 2 (`world_backend.rs:510-514`, `frontend_audio.rs:108-126`).
The frontend plays track 8. The probe on Tabuchi reported `track_word: 8`,
as decoded.

| World | Track | World | Track | World | Track |
|---|---|---|---|---|---|
| 1 | 8 | 9 Tabuchi | 8 | 17 Shak'tahh | 3 |
| 2 Terra Nova | 7 | 10 Pillier Stren | 7 | 18 Command Centre | 8 |
| 3 Stendahl Alpha | 4 | 11 Artoria | 5 | 19 Alien Brain | 5 |
| 4 Exxodus | 5 | 12 Shalapreth | 8 | 20 Oolbryte | 4 |
| 5 Altair Prime | 4 | 13 Grach'to | 5 | 21 Vernimab | 5 |
| 6 Helniazine | 4 | 14 Foldrak Noir | 4 | 22 Homeworld | 6 |
| 7 Sanrel-Dek | 6 | 15 Kankutan | 3 | 23 Caspadril | 7 |
| 8 Luna Azine | 3 | 16 Defence Satellite | 3 | 24 Caradion | 3 |

World 0 has no world header of its own (it loads world 19's map); its
music is not checked. The network arenas (worlds 25 to 33) use tracks 3
and 5.

The music loops by restarting: a finished track is played again by the
next service after its deadline, the track's length plus 2 s
(AUDIO_RUNTIME.md:58-78). So there are about two seconds of silence
between loops, and none of it is filled.

**Loops placed in the world.** Retail's looping sounds all belong to an
actor or to the player's vehicle. Counted from each world's placements:

| World | Warpgates | Factories | Sirens | Engines | World | Warpgates | Factories | Sirens | Engines |
|---|---|---|---|---|---|---|---|---|---|
| 1 | 1 | 2 | | 5 | 13 | 2 | 2 | | 4 |
| 2 | 3 | 2 | | | 14 | 5 | 2 | | 4 |
| 3 | 2 | 1 | | 6 | 15 | 2 | 1 | 2 | 7 |
| 4 | 4 | 2 | 1 | 11 | 16 | 2 | 1 | | |
| 5 | 5 | 3 | | 8 | 17 | 3 | 2 | 1 | 7 |
| 6 | 9 | 2 | | 5 | 18 | 2 | 3 | | 3 |
| 7 | 3 | 3 | | 3 | 19 | 11 | | | 5 |
| 8 | 1 | 3 | 1 | 4 | 20 | 7 | 2 | | 6 |
| 9 | 6 | 3 | 2 | 4 | 21 | 3 | 2 | 1 | 11 |
| 10 | 3 | 2 | 1 | 7 | 22 | 3 | | | 8 |
| 11 | 7 | 6 | 3 | 28 | 23 | 7 | 2 | | 10 |
| 12 | 4 | 1 | | 3 | 24 | 2 | 1 | | |

- Warpgates (class 30) hum while active, full within 1,500 units of the
  camera (`selector30/update.rs:26-27`, `:400-414`).
- Factories (class 15) loop an idle or a busy sound within 3,000 units
  (`selector15/factory.rs:52-57`, `:1150-1170`).
- Sirens (class 47, model `powergen`) sound only when an alarm is set off
  (`selector47.rs:1-13`).
- Engines are the ships, hover bots and tanks (classes 34, 35, 36), each
  with a loop near the camera (class 36 within 7,680 units,
  AUDIO_RUNTIME.md:336-341). Missiles loop while they fly.
- The player's vehicle loops its engine and its skid
  (VEHICLE_DRIVE_RUNTIME.md:116, `:579-581`), and plays one-shots for
  splashes (slots 9 and 10), diving in (`0x13`) and bubbles under the
  water (`0x44`) (`:595-601`).
- The generators (`newgen`, class 39, a target class) have no hum while
  they stand: they sound only when they die, a fuse and its end
  (`selector39/course.rs:32-35`). Artoria has 4, Tabuchi 5, Helniazine 4,
  several others 1. The transformer (`gentrans`) is terrain furniture,
  loaded in worlds 11, 12 and 14 and the arenas; it has no sound owner at
  all.
- Waterfalls: none. Each world has one water plane (with waves on 6 and
  24), and no model is named for falling water. None are needed.

So today a world's background is its music, the hum of whatever gate or
factory is near, and engines. Out on open ground, in the music's gaps or
with the music off, it is silent.

### 3. How retail places a sound (verified)

`process_sound.rs:431-485`, with the policy words at `0053BB80`
(AUDIO_RUNTIME.md:185-192). Distances are in world units; a unit is about
a centimetre, as the water's spray takes it.

- **The listener is the camera** (the render context's eye and axes),
  read in each world draw (`world_scene.rs:983-1012`).
- **Range is a box**: a sound 8,192 units (about 82 m) away along any one
  axis is not played.
- **Loudness**: full within 256 units, then falling in a straight line to
  nothing at 8,192.
- **Panning by sideways offset, not by direction.** The sound's sideways
  distance in the camera's frame sets the pan, in DirectSound's way:
  attenuating the other speaker, up to fully one-sided at 4,096 units.
  At 820 units to the side the other speaker is already 20 dB down, at
  2,048 it is 50 dB down (almost silent). So a sound 80 m ahead and 8 m
  to the right plays mostly in the right ear. On headphones that is
  harsh.
- **No filtering, no reverb, no Doppler** (the pitch drop of a passing
  car), **no occlusion** (a sound behind a hill reaching you duller).
- **One-shots are placed once**, when they start; loops are re-placed
  every frame.
- **The in-world menu (phase 5) silences** placed sounds.

### 4. The port's mixer (verified)

- **One SDL device**, 44,100 Hz stereo, 1,024-sample buffers (23 ms)
  (`platform_audio/lifecycle.rs:313-317`). It mixes three things
  (`platform_audio.rs:174-262`):
  - the music, stereo 44.1 kHz Ogg Vorbis decoded with Lewton (already a
    dependency), each track decoded whole and kept (`:463-471`);
  - retail's effects, mono 16-bit recordings resampled by straight-line
    interpolation (pitch 1.0 is 22,050 Hz, `:1213-1215`), each with a
    gain and a pan in hundredths of a decibel (`:216-220`);
  - the title video's sound.
- **Voices**: retail allows 24 (`process_sound/voices.rs:151`); the
  port's fix allows 64, at most 12 of them water splashes
  (`frontend_settings.rs:322-326`).
- **No filters, no reverb, no limiter.** The sum is clipped hard at full
  scale (`platform_audio.rs:249-252`).
- **A small waste**: each playing voice computes three power functions
  for every output sample (`:217-220`). With about 11 voices, as when
  hovering, that is about 1.5 million a second (not timed). Computing them
  once per change gives the same numbers.
- **Retail's sounds draw the game's random numbers.** Resolving an alias
  steps the world's RNG (`platform_audio.rs:1170`; `world_scene.rs:1020`).
  So anything new must never go through retail's sound path: it would
  change the game.
- **Hidden runs are silent**: `--hidden` mutes the speakers while the mix
  and its counters run on (`platform_audio.rs:291-294`, `:259-261`).

### 5. The mix has no headroom left (measured)

Three silent probes on Tabuchi's lake, the hovercraft idling on the water
(`.tmp\plan-atmosphere\hover-w9*.txt`, the run's closing `Audio:` line):

| Sound | Music | Seconds | Peak | Voices playing, mean |
|---|---|---|---|---|
| 15 | 15 | 10.9 | 1.0 (clipped) | 10.9 |
| 15 | 0 | 10.4 | 1.0 (clipped) | 11.6 |
| 0 | 15 | 14.4 | 0.40 | |

Retail's effects alone reach full scale while hovering: the splashes add
up. Any bed added on top would clip more. So the master needs a limiter
(a gentle automatic turn-down of the loudest moments) from the first bed
on, and the beds must sit low. The probe counted 737 sound requests in
11 s, 132 of them started.

### 6. What the port can record (verified)

- `--record-session DIR` records the final mix (float, stereo) with the
  frames, but only in a visible, realtime 1024x768 window: hidden or
  fixed-step runs are refused
  (record_port_intro.md).
- In a hidden fixed-step run the SDL device still mixes in real time, out
  of step with the game's frames, so the mix is not reproducible.
- `--event-log FILE` logs every sound record retail creates (slot,
  position, gain, pitch; `event_log.rs:200-216`), which proves retail's
  sounds unchanged.
- The run's closing line prints the mix's counters and peak (`main.rs:569`).

So headless checks need one new tool: an offline mix written to a file,
driven by the game's frames (below).

### 7. What the world data offers for placing sounds (verified)

- **Water and liquids**: 15 worlds have a liquid plane (the tuning's
  `worlds` block, `modern_tuning.json:139-157`): water on 1, 3, 5, 6, 9,
  12, 13, 15, 18, 22, 23; the Brain's red liquid on 0 and 19; lava on 17
  and 24. Worlds 6 and 24 have waves. World 12's reservoir rises at the
  end. Shorelines come from the terrain against the plane, as the
  shoreline plan computed them.
- **Lava on the ground**: Sanrel-Dek (7) has lava rivers drawn on the
  ground, with no plane. Which ground material marks them is not checked.
- **Machines by name**: actors carry their model's name (`newgen`,
  `factory1`, `gate`); the tuning already names the metal models
  (`materials.names`, `modern_tuning.json:44-78`), which can also say
  where rain drums on metal.
- **Light schedules**: Artoria's light falls when its generators die,
  Kankutan's swings every 90 s (`world_tint.rs`; the sky plan). Sounds can
  follow them.

## The soundscape, world by world

### The bed types

A bed is two or three long recordings of one kind, crossfaded (one fades
out as the next fades in) every few minutes, with sparse details
("sweeteners": a distant rumble, a creak) placed around you at random.

| Type | Character | Worlds |
|---|---|---|
| A. Open air, mild | light breeze, wide and soft | 1, 6, 12, 15, 18 |
| B. Cold wind | wind with a high edge, gusts, hushed distance | 2, 10, 23 |
| C. Still cold night | near-still air, a faint high wind, ice creaks | 3 |
| D. Dry desert wind | gusty, sand hiss on the gusts, heat | 4, 13, 21 |
| E. Volcanic | deep rumble, hot wind, crackle | 7, 17, 24 |
| F. Storm air | pressure, gusts, far thunder | 5, 9, 20, 22 |
| G. Airless | no wind: a low, sparse hum; the ACAM carries it | 8, 14 |
| H. Station | metal hum, distant clanks | 16 |
| I. Clarifier city | a broad drone of the clarifiers, wind in towers | 11 |
| J. The Brain | organic drone, a slow pulse, wet textures | 0, 19 |

Why no wind on the airless worlds: there is no air to carry it. The
ACAM's own engine and the low hum make them feel empty, and the contrast
makes the windy worlds stand out.

Why no animal calls anywhere: a call in the distance reads as an enemy.
The game's creatures have their own sounds; the beds must not compete.

### World by world

"Weather" is the sky plan's, as approved (snow on 10 and 23,
on 2 if its white ground is snow; rain and lightning on Tabuchi;
lightning on 5, 22 and 17; ash and embers on 7 and 17). "Placed" lists
the new sources only; retail's hums stay.

| World | Bed | Weather sounds | Placed (new) | Drawn wind |
|---|---|---|---|---|
| 0 | J | | red liquid | |
| 1 | A, quieter at night | | grey water shores | |
| 2 Terra Nova | B | wind; snow hush if snow | | light drift, if snow |
| 3 Stendahl Alpha | C | | clear water shores, gentle | |
| 4 Exxodus | D | gusts, sand hiss | | dust, strong |
| 5 Altair Prime | F | gusts, far thunder after flashes | toxic water, thick lapping | |
| 6 Helniazine | A, denser | | toxic water with waves; generators | |
| 7 Sanrel-Dek | E, canyon echo | ash wind, ember crackle | lava rivers (if the ground marks them) | embers and ash |
| 8 Luna Azine | G | | the shield dome's hum (model to find); generator | |
| 9 Tabuchi | F | rain, rain on the lake, gusts, thunder | choppy shores; rain on metal; generators | rain slant, ripples, foam streaks |
| 10 Pillier Stren | B | wind, gusts, blowing snow | | blowing and falling snow, strong |
| 11 Artoria | I | | generators; transformer if placed | |
| 12 Shalapreth | A, warm | | reservoir shores that follow the rising water; transformer if placed | light dust (optional) |
| 13 Grach'to | D, canyon echo | gusts | clear water shores | light dust |
| 14 Foldrak Noir | G, ice creaks | | transformer if placed | |
| 15 Kankutan | A, breathing with the light | | flowing river | |
| 16 Defence Satellite | H, metal echo in the trenches | | the laser grid's buzz when you fly near it | |
| 17 Shak'tahh | E | ash wind, crackle, thunder in the ash cloud | lava: bubbling, hiss, pops | embers and ash |
| 18 Command Centre | A | | streams, small trickles | |
| 19 Alien Brain | J | | red liquid | |
| 20 Oolbryte | F, clearing | easing gusts, rare far rumbles | | |
| 21 Vernimab | D | as 4 | | as 4 |
| 22 Homeworld | F | as 5 | toxic water | |
| 23 Caspadril | B, crisp | light wind, snow hush | marsh shores, trickles | blowing snow, moderate |
| 24 Caradion | E, warm | | lava with waves: lava surf | embers (optional) |

Every generator (`newgen`) hums, in any world; the table names them only
where there are several.

Notes on the worlds that need more than a row:

- **The Alien Brain (19, and 0 on its map).** Ominous but restrained: a
  deep organic drone; a slow pulse like a heartbeat, about 50 a minute,
  felt more than heard; wet textures; distant groans every 20 to 60 s.
  Near the red liquid, a slow, thick bubbling. If the sky's tissue clouds
  get their slow pulses, the sound's pulse runs on the same clock. It
  plays instead of the music (built 2026-10-28): when
  the pak holds the Brain's beds, under Vulkan with the Ambience above 0,
  track 5 is hushed from the world's load (its briefing) through play and
  its in-world menu (phases 5 and 6). In that menu the beds keep playing,
  unfaded, as the music would, and track 5 stays hushed; the computer's
  Map and Objectives (phase 7) and the results screen (phase 8) have
  track 5 back, where the beds stop. The beds' recordings decode from the
  load, so they are there when play begins. Without them, at Ambience 0,
  and under OpenGL and the software renderer (also an OpenGL standing in
  for a Vulkan that failed to start), track 5 plays as in retail; on every
  other world nothing changes. The music's state (its selection, the CD's
  position and end) stays retail's; only its sound goes
  (`ambience::replaces_music`, `PlatformAudio::hush_music`).
- **Tabuchi (9).** The storm world and the showcase. Rain swells and
  eases with the weather; near the lake, rain hissing on the water joins;
  gusts rise with the drawn gusts; each flash brings its thunder. Rain
  drums on gun bases, gates and factories near you, and on the hull.
- **Artoria (11).** The clarifiers' drone is the bed. When the four
  generators die and retail's light falls (one second), the drone powers
  down with it: pitch and level follow the light. What remains is wind
  between dark towers, a few creaks, emptier. The briefing's jet fighters
  can pass far overhead now and then.
- **The airless worlds (8, 14).** Near silence under a low hum. On Foldrak
  Noir, rare ice creaks from the ground. On Luna Azine, the shield dome
  hums near the start.
- **The Defence Satellite (16).** A station: metal hum, distant clanks,
  the trenches' metal echo. The laser grid buzzes faintly overhead when
  you fly near it.
- **Kankutan (15).** The bed swells and dims gently with retail's
  90-second light swing.
- **Shalapreth (12).** When the reservoir rises at the end, a rushing
  water layer plays while it rises, and the shores move with the plane.
- **Oolbryte (20).** A storm that has passed: the wind easing, and rare
  far rumbles with no flash (the storm is beyond the horizon). That is
  the one place thunder plays without a visible flash; a question below.
- **World 1.** Its light goes from day to
  sunset to night; the bed can grow quieter with it.
- **The network arenas (25 to 33)**: optional. Each takes the bed type of
  its look.

## The sound system

### The switch and the volume

- A new **Ambience** volume, 0 to 15 like retail's two, saved in the
  port's settings. It shows in the game's Sound menu (front end and in
  game) under Vulkan, in retail's six languages, as the Vulkan rows of
  the video menu do (`frontend_video.rs:264-288`), and in the launcher's
  Vulkan group.
- On by default under Remastered (Vulkan). Under Vanilla (OpenGL) and the
  software renderer the row is absent and the mixer runs exactly today's
  path.
- **0 turns it all off and gives back retail's sound exactly**, even under
  Vulkan: no voices, no limiter, retail's own panning. That makes the
  user's comparison one slider, and the tests' too.
- **Everything the port adds** plays at the Ambience volume: beds,
  weather, thunder, placed sources, rain on the hull. Retail's sounds stay
  at the Sound volume, the music at the Music volume. One knob should
  turn all of the port's sound down without touching retail's.
- **Weather sounds play only when the weather is drawn**: under Vulkan
  with path tracing, which the modern sky needs. Without path tracing the
  beds and placed sources still play, but no rain you cannot see.
- Not in the intro, the briefing or the results: those are retail's
  scripted presentations. In the in-world menu (phase 5) the ambience
  fades out, as retail silences its placed sounds there.

### Where it runs

- **A controller on the game thread** (`ambience.rs`, new), once per
  frame after the world draw's sound service. It reads, never writes: the
  world, the camera, the vehicle (place, speed, form, first person), the
  weather state, the actors and the world's water and lava. It decides
  what should be heard and sends targets to the mixer.
- **An ambience bus in the mixer** (`platform_audio.rs`), beside retail's
  effects: its own voices, never retail's pool (24 or 64) and never the
  game's RNG. Its random choices use a hash of the world and the game's
  animation clock, as the sky's weather does, so fixed-step runs repeat.
- **Changes ramp** over one buffer (23 ms), so nothing clicks.
- **Budget**: at most about 34 ambience voices: 4 for beds, 6 for
  weather, 16 placed sources, 8 one-shots.

### Beds, loops and variation

- **Long loops**: 90 to 180 s each, two or three per bed, crossfaded over
  8 to 10 s every 2 to 4 minutes. Each starts at a random point.
- **The mixer loops by crossfading** the end into the start (1 to 2 s), so
  a recording needs no perfect loop point and MP3 sources (whose encoders
  leave gaps) work after conversion.
- **Round-robin**: several recordings of each one-shot played in turn,
  never the same twice running, each with a small change of pitch (±4%)
  and level (±2 dB), so nothing repeats exactly.
- **Sweeteners**: one-shot details every 8 to 40 s, placed in a random
  direction and distance, through the same 3D placement as everything
  else.
- **Level**: beds sit about 15 dB under the music's average, and are
  shaped darker, out of the 1 to 4 kHz band where the game's cues (shots,
  pickups, warnings) live, so they never mask them.
- **The beds fill retail's 2-second gaps** between music loops for free.

### Weather: one state for the sky and the sound

- **One weather state per world**, a pure function of the world and the
  game's animation clock (`weather.rs`, new): rain, snow, ash, the wind
  (direction, strength, the gust swell) and the lightning flashes (time,
  direction, distance class, brightness). The renderer gets it in its
  frame block; the ambience reads it directly.
- This asks one thing of the sky plan: compute the flash schedule on the
  CPU and pass it to the shaders, rather than inside them, so the thunder
  knows when each flash was. Its tuning (`rain`, `lightning`) moves from
  the sky's block into a shared `weather` block.
- **Rain**: a light and a heavy loop crossfaded by intensity; rain on
  water added by how much water surrounds you; rain on the hull and on
  metal (below).
- **Wind**: a calm and a strong loop crossfaded by strength, gust
  one-shots at the gust swells (the same swells that thicken the drawn
  drift), and air rushing past when the ACAM is fast or the VTOL is high.
  A synthesized layer over the loops is to be tried as well (see
  "Ready-made atmosphere generators" under the alternatives).
- **Thunder after each flash.** The world is too small for real delays:
  it is about 65,000 units (some 650 m) across, so sound would arrive
  within two seconds of any flash. So each flash's distance class sets
  its thunder:
  - a near bolt (at most one flash in four): a crack and a boom, full
    range, 0.3 to 1.5 s after;
  - a flash inside the clouds: a low roll, duller and quieter, 2.5 to 6 s
    after, rumbling 6 to 15 s.

  Bright flashes are near ones. At most two thunders overlap; a new one
  turns the older down. The rate follows the sky's flash limit (at most
  about one flash every few seconds), never a strobe.
- **Snow hushes**: in snow, distant sounds are duller and the echo
  shorter, as snow absorbs sound.

### Placed sources

- **Points**: actors and furniture matched by model name in the tuning
  (`newgen`: generator hum; `gentrans`: transformer hum with crackles;
  drills; the laser grid). The controller follows the actors each frame,
  so a generator's hum dies with it. The transformer's crackle and the
  flicker of its planned orange glow can share one
  schedule.
- **Areas**: shores, liquids and lava. At load, a coarse map of the world
  (64 by 64 cells) marks shore, liquid and lava cells from the terrain
  against the plane. Each frame, the nearest marked cell in each quarter
  around you becomes one virtual source. So the lapping comes from the
  side the water is on and swells as you near it. The shoreline plan's
  cautions apply (world 6's waves, world 23's marsh at the plane).
- **The 16 loudest** candidates play, with a margin so voices do not
  flicker in and out, fading in and out over half a second.
- **Not doubled**: warpgates, factories, sirens and engines keep retail's
  sounds.

### Placing in 3D

- **Listener**: the camera, as retail. Using the ACAM for distance in
  third person was weighed; keeping retail's choice keeps the new and the
  old sounds consistent.
- **Panning by direction** with constant power (a sound moves smoothly
  between the speakers without dipping in the middle).
- **Distance**: louder near, quieter far, each source with its own range.
  Far sounds also lose their highs (a one-pole low-pass, cheap), as air
  absorbs them.
- **Occlusion**: ten times a second, each source's line to the camera is
  sampled at 16 points against the terrain's heights. Behind a ridge it
  is 6 dB quieter and much duller, changing over 0.3 s.
- **Reverb** (the echo and tail of a place) through one shared reverb that
  every voice sends a share to (a send), so its cost does not grow with
  the number of sounds. A small feedback delay network: eight delay lines
  feeding back through a mixing matrix, with damping. Each world has a
  preset (open, canyon, metal trenches, city towers, organic); the local
  enclosure scales it: eight height samples around the camera say how
  walled-in you are.
- **Ducking** (turning the ambience down briefly while something more
  important plays): a follower on retail's effects lowers the ambience by
  up to 6 dB when a big explosion goes off, back over about 0.8 s. The
  music is not ducked.
- **A limiter** on the master instead of the hard clip, while Ambience is
  above 0 (the probe shows retail's effects alone reach full scale).

### The hull and first person

- **First person** (F12): you are inside the shell. Outside sounds lose
  some highs and 2 dB; rain becomes a close drumming on the roof with
  single drops pinging left and right; wind becomes a muffled buffeting.
- **Third person**: rain on the hull is a small source at the ACAM, under
  the rain around it.
- **Rain on the hull** grows with the rain and with speed (driving into
  it). **Single drops** fall at random times, from 24 to 40 recordings,
  more in heavy rain.
- **Rain on metal near you**: models the tuning marks as metal (gun bases,
  gates, factories, silos, pipes) within about 1,500 units get a
  rain-on-metal loop while it rains, louder for bigger ones.
- Under the water: retail's camera keeps its eye above the surface (the
  blasts plan, `selector16/pose/terrain.rs:68-103`; a passing wave crest
  could dip it for a moment, never seen), so no under-water bed is needed.

### Retail's own sounds under Remastered (optional, recommended)

Same recordings, same moments, same gains; only their placement changes,
so they sit in the same space as the ambience:

- panning by direction instead of DirectSound's hard sideways pan;
- far sounds duller;
- a share into the reverb of the place;
- blasts under the water muffled (a sound record below the water's plane
  over water gets a deep low-pass, as the blast's glow shows it);
- Doppler on missiles and ships: not recommended. Retail's engine pitch
  already carries speed and load; Doppler would bend it.

With Ambience at 0 they are retail's again.

### Headphones

Plain stereo by default. HRTF (filters that make headphones place sounds
above, behind or in front, the way the head and ears shape real sound)
comes last, as an option: a cheap model (the delay between the ears and
the head's shadow), not a full measured set. It can sound odd on
speakers, hence off by default.

As built (2026-10-24): a Headphones row after Ambience in both Sound
menus and a launcher combo, Vulkan only. The far ear hears a placed sound
up to 0.66 ms later, low-passed to about 2 kHz and 3 dB down; the near ear
is 2 dB up so the loudness holds as the head turns; from behind both ears
are duller. The beds and sweeteners keep their stereo. Details in
`docs/re/AUDIO_RUNTIME.md`.

### The sounds on disk: `Remastered.pak`

One file to drop into the game folder was wanted (2026-10-23), and
the question was whether a missing one should turn off Vulkan or the
weather.

- **One file, `Remastered.pak`**, beside the game's data: the player's
  game folder, and `infestation/` for development. A release's zip
  carries it, so unpacking the zip into the game folder is the whole
  install.
- **Our own small archive, not an OVL.** It has a header, an index (name,
  offset, length, SHA-256) and the files stored as they are. Ogg Vorbis
  is already compressed, so a loop streams straight from its offset.
  Reading it is about 80 lines with no new dependency.
  - An OVL would pass for retail data: the data check fingerprints the
    overlays as retail's, and retail's loader rules would apply to files
    retail never had.
  - A zip would need a reader the port lacks.
- **Inside**: the sounds and a `credits.txt` holding every licence's
  attribution.
  - Format: Ogg Vorbis (Lewton already decodes it) at 44.1 kHz, the
    mixer's rate, so no resampling. Stereo for beds and weather, mono for
    placed sources and drops.
  - The tuning file still maps worlds to sounds.
- **Built, never stored.** `scripts/build-remastered-pak.py` makes the
  pak from the original sound files, through a manifest in git: each
  file's source, licence and processing (trim, loop, EQ, loudness,
  conversion). The sounds never enter git.
- **Missing, or built from another manifest.** The pak names the
  manifest it was built from.
  - The remastered sounds are off, and retail's sounds play as always.
    Everything else under Vulkan stays: no picture depends on the pak,
    and the drawn weather still shows. Retail had no weather sounds at
    all, so silent rain is still more than retail had.
  - The launcher says so in its setup lines, as a warning; Play stays
    enabled. The in-game Ambience row shows Unavailable.
  - Our view: turning off Vulkan for a missing sound file would punish
    the picture for the sound, and turning off the drawn weather would
    hide the part that works.
  - A pak missing some entries leaves those layers silent, each logged
    once.
- **Streaming.** Loops of 20 s and longer stream from the pak through a
  small decoder thread; shorter sounds decode at load. Decoding whole
  loops, as the music does, would take about 100 MB of memory per world
  and several seconds of loading. If streaming costs more than about 2%
  of a core, decode the world's loops during its briefing instead.

### Logs and capture, for checking

- **`--audio-out FILE.wav`**: for hidden fixed-step runs, the mix is
  rendered from the game's frames (each frame mixes its share of samples)
  instead of the SDL device's thread, so the same run gives the same file.
  Optionally one file per bus (retail's effects, music, beds, weather,
  placed, reverb).
- **`ambience` lines in the event log**: bed changes, each second the
  buses' levels, each thunder (its flash, delay and class), the placed
  sources heard, the reverb and enclosure, the ducking. A new kind, so the
  lab's comparisons select their own kinds as now.

## Drawn wind

### Where

| Strength | Worlds | What shows |
|---|---|---|
| Strong | 9 Tabuchi | rain slanted, ripples and foam streaks along the wind, cloud drift |
| Strong | 10 Pillier Stren | snow blowing low over the ground, falling snow |
| Strong | 4 Exxodus, 21 Vernimab | dust blowing low, wind-torn clouds drifting |
| Moderate | 23 Caspadril, 2 Terra Nova (if snow) | blowing snow, lighter |
| Moderate | 7 Sanrel-Dek, 17 Shak'tahh | ash drifting, embers rising and blown |
| Light | 13 Grach'to, 12 Shalapreth | a little dust on the gusts |
| Clouds only | 1, 3, 5, 6, 11, 15, 18, 20, 22, 24 | clouds drift with the wind; ripples lean with it |
| None | 0, 8, 14, 16, 19 | no air, or the Brain |

### How each is drawn under Vulkan

- **Snow, sand or dust low over the ground** (spindrift): a thin layer of
  the volumetric mist (`pt_fog_inject.comp`) near the ground, whose density
  is a noise stretched along the wind and carried by it, thicker and
  higher in each gust. It is lit and shadowed as the mist is, in the
  world's colour (white, ochre, red). The mist's cells are fine near the
  camera, where it matters. Estimated 0.05 to 0.1 ms.
- **Flakes, grains, ash and embers in the air**: the streak layers the sky
  plan draws its rain with (in the composite, three layers by depth,
  hidden behind nearer geometry), with other speeds, sizes and colours,
  slanted by the wind. Embers glow. Estimated 0.1 to 0.2 ms, only on these
  worlds.
- **Clouds** drift along the wind (the sky's cloud speed becomes the wind).
- **Water**: the water tiling plan's new waves spread their directions
  around the wind and grow with its strength and gusts; on Tabuchi's lake,
  long foam streaks lie along it (a stretched mask in the foam's pattern).
  Next to nothing in cost.
- **Never hiding an enemy**: the drift stays below the ACAM's roof, the
  layers never hide more than about a fifth of what is behind them,
  flakes fade within a few metres of the camera, and thermal vision shows
  none of it (as the sky plan's rain).

Rejected: drawing each flake as a traced drop like the spray. The spray's
drops are tested one by one against every pixel's ray, grouped only by
their source's sphere (`pt_spray.glsl:19-66`); thousands of flakes over
the whole view would cost milliseconds. Sprites in the raster would not
take the traced light.

### One wind per world

In the shared `weather` block: a bearing, a strength (0 to 1) and how
gusty (0 to 1). The gust swell is one function of the clock that the
sound's gusts, the drift's thickening, the rain's slant and the ripples
all read.

## The tuning (a sketch)

In `modern_tuning.json`, reloaded within a second as now, so levels can be
tuned while playing:

```json
"weather": {
  "wind": {"bearing": 0.0, "strength": 0.2, "gusts": 0.3},
  "rain": 0.0, "snow": 0.0, "ash": 0.0,
  "lightning": {"rate": 0.0, "near": 0.25}
},
"atmosphere": {
  "bed": [], "gain": 1.0, "sweeteners": null,
  "reverb": "open", "dull": 1.0,
  "sources": [
    {"models": ["newgen"], "sound": "hum_generator", "range": 2500, "gain": 0.6},
    {"models": ["gentrans"], "sound": "hum_transformer", "range": 2000, "gain": 0.6}
  ],
  "shore": {"sound": "lap_calm", "range": 2000},
  "liquid": null, "lava": null
},
"worlds": {
  "9": {
    "weather": {"wind": {"bearing": 250.0, "strength": 0.7, "gusts": 0.6},
                "rain": 0.8, "lightning": {"rate": 0.25, "near": 0.25}},
    "atmosphere": {"bed": ["storm_a", "storm_b"], "shore": {"sound": "lap_choppy"}}
  },
  "19": {
    "atmosphere": {"bed": ["brain_a", "brain_b"], "pulse": 50.0,
                   "liquid": {"sound": "bubble_thick"}, "reverb": "organic"}
  }
}
```

Sound names map to files in `Remastered.pak` through its manifest.

## Alternatives considered

- **Middleware (FMOD, Wwise).** Mixing tools, HRTF and reverb out of the
  box, but a large proprietary dependency with its own licence and credit
  terms, and it would sit beside the SDL mixer that reproduces retail's
  sound. The pieces needed here (filters, one reverb, a limiter) are a few
  hundred lines.
- **A Rust audio library (kira, fundsp).** A second mixer beside SDL's, or
  replacing it, puts retail's exact path at risk. Their algorithms can be
  borrowed; the dependency is not needed.
- **Ready-made atmosphere generators.** Searched on 2026-10-23, after the
  user asked whether plugins already do this. There are three kinds:
  - Plug-ins that synthesize weather from no recordings. Wwise's
    SoundSeed Wind makes wind from changing parameters, driven by game
    values such as speed. Nemisindo's Nature Pack for Unreal has
    droplets, wind, waves, rain, fire and water, all adjustable at run
    time. Each runs only inside Wwise or Unreal.
  - Tools that render files: AudioGaming's AudioWeather (EUR 49), and
    AudioWind and AudioRain Pro, SirenFX's WindGen and RainGen, and
    Tsugi's GameSynth. What they render is a recording like the packs'.
    It can fill a gap (rain on water, a wind bed of an exact length),
    with nothing to integrate.
  - Web mixers (myNoise, Ambient-Mixer) layer recordings under sliders,
    the same scheme as the beds and sweeteners here. myNoise's sounds
    need a licence, which it rarely grants. Each of Ambient-Mixer's
    carries its uploader's Creative Commons terms.

  FMOD's scatterer instrument plays sounds from a list at random
  intervals, distances and directions, each at its own pitch and level.
  That is what the sweeteners above build. The one idea worth taking is
  synthesized wind. Wind must follow the weather's strength and gusts and
  the ACAM's speed all the time, and crossfaded loops follow them only in
  steps. Noise through a moving resonant filter follows them smoothly and
  never repeats. Alone it can sound thin, so it goes over the recorded
  bed. Build it, and choose by ear with and without it, as for the
  sunsets.
- **Steam Audio** (occlusion and reverb from geometry). Powerful, but a
  heavy native library; the terrain test gives most of the effect.
- **Ambience through retail's sound path.** Rejected: it draws the game's
  random numbers and takes retail's voices.
- **One long ambience track per world, like a second music track.**
  Simplest, but nothing placed, no weather, and it repeats. The beds are
  this, plus the rest.
- **Thunder delayed by the real distance in the world.** At most about 2 s
  anywhere in a world this size; a storm needs 2 to 6 s rolls to read as
  one.

## What changes where

- `src/ambience.rs` and `src/ambience/` (new): the controller, the bed and
  layer logic, the source picking, the occlusion and enclosure tests, the
  shore/liquid/lava map built at load.
- `src/weather.rs` (new, shared with the sky): the weather state from the
  tuning and the clock.
- `src/platform_audio.rs`: the ambience bus and its voices (ramps, a
  low-pass, constant-power pan, a reverb send), the reverb, the ducking,
  the limiter; retail's gains computed once per change (the same
  numbers); the optional placement of retail's sounds; the offline render
  for `--audio-out`.
- `src/platform_audio/ambience_assets.rs` (new): `Remastered.pak`'s index,
  Ogg streaming and decoding.
- `scripts/build-remastered-pak.py` (new) and the manifest it builds from.
- `src/world_scene.rs`: the controller's call after the sound service
  (`:983-1012`), with the camera.
- `src/modern_tuning.rs`, `modern_tuning.json`: `weather` and
  `atmosphere`, per world, with tests for the merge.
- `src/frontend_settings.rs` (the Ambience word), the Sound menu's row,
  `port_text.rs` (six languages), `launcher/native.rs` and
  `launcher_setup.rs` (the row and the folder's report).
- `src/event_log.rs` (`ambience` lines); `src/main.rs` (`--audio-out`).
- Shaders, for the drawn wind: `pt_fog_inject.comp` (the drift),
  `pt_composite.frag` (flakes, grains, ash, embers), `pt_trace.comp`
  (ripples and foam streaks along the wind), the sky's clouds; the frame
  block (`pt_frame.glsl`) gets the wind and the gust.
- The sky plan's flash schedule moves to the CPU (`weather.rs`).
- `ambience/` (new): `README.md` and the manifest; the sound
  files themselves untracked.
- Docs: [AUDIO_RUNTIME.md](../re/AUDIO_RUNTIME.md) (a section
  on the port's ambience, beside retail's), `PIXEL_RUNTIME.md` (the drawn
  wind), `INFESTATION-RT-IMPLEMENTATION.md`.

## Cost

| Part | Where | Estimate |
|---|---|---|
| Retail's gains computed once per change | audio thread | saves about 1.5 million power functions a second while hovering |
| Beds, weather, placed sources (34 voices at most) | audio thread | about 20 million simple operations a second, 1 to 2% of one core |
| Reverb, ducking, limiter | audio thread | well under 1% of one core |
| Controller: picking, occlusion (16 x 16 samples, 10 a second), weather | game thread | well under 0.05 ms a frame |
| Streaming Ogg (2 to 5 long loops at once) | decoder thread | about 1 to 5% of one core (to measure) |
| Optional HRTF (cheap model) | audio thread | about 1% of one core |
| Drift in the mist | GPU | 0.05 to 0.1 ms |
| Flakes, grains, ash, embers | GPU, rough worlds only | 0.1 to 0.2 ms |
| Wind on ripples, foam and clouds | GPU | next to nothing |

Memory: a few MB of stream buffers, 20 to 40 MB of short sounds per world.
On disk: about an hour of unique audio, some 70 MB as Ogg.

## Risks

- **Licences.** The Lucid pack's licence names "music productions" only
  and forbids sharing the files; Pixabay's allows use in a game but not
  passing the files on as they are. Kept out of git, processed into our
  own beds, and Lucid asked before shipping (below).
- **Fatigue.** A bed that is too loud or too busy tires quickly. Low
  defaults, darker beds, sparse sweeteners, and the slider.
- **Audible loops.** Long recordings, two or three per bed, random starts,
  crossfades, sweeteners.
- **Masking cues.** Beds kept out of the cues' band, ducking under big
  blasts, no animal calls.
- **Clipping.** Retail's effects already reach full scale; the limiter
  comes with the first bed.
- **Sound and picture apart.** One weather state, thunder scheduled from
  the flash itself, the gusts shared with the drawn drift.
- **The game changed by accident.** The ambience never touches retail's
  pool or RNG; the event log's `sound` and `rng` lines must match with
  Ambience at 0 and 15.
- **The audio thread starved.** Gains computed once, filters of one pole,
  one reverb; the callback's time measured in the first step.
- **Streaming stalls.** A late buffer plays silence for that layer; the
  callback never waits.
- **Wrong models matched by name** (`gate*` also matches the dead gates).
  Exact lists in the tuning, and a per-world list of matches in a test.
- **Shores.** World 6's waves move the waterline; world 23's marsh sits at
  the plane; world 12's water rises. As the shoreline plan found.
- **Flashes and thunder** at too high a rate: the sky's limit holds both.

## Verification

- **Retail unchanged**:
  - a unit test feeds today's mixing and the new one the same voices and
    requires the same samples with Ambience at 0;
  - with Ambience at 0 and at 15, the event log's `sound`, `rng` and
    `tick` lines are identical (`compare.py --kinds`), and so is the
    offline mix's retail stem (unless the placement of retail's sounds is
    chosen, which changes only that stem's panning and filtering);
  - OpenGL frames byte-identical.
- **Offline mixes per world** from hidden fixed-step runs (silent, as all
  probes), one minute each on a fixed route (drive, stop by water, wait
  for a gust or a flash), as Ogg to listen to: the sound
  sheets, as the sky has its picture sheets. Each
  world's is approved by ear.
- **Levels**: per bus loudness and peak from the files; no clipped sample
  with the limiter; beds 12 to 18 dB under the music's average.
- **Thunder**: each flash in the log against its thunder's start, and the
  snapshots of the flash.
- **Unit tests**: the pan law, the distance curve, occlusion behind a
  synthetic ridge, the weather state's repeatability (same clock, same
  flashes), the crossfade loop's seam (no jump between samples), the
  limiter, the tuning's merge, the model matches per world.
- **Cost**: the audio callback's time (logged), the decoder thread's load,
  the new GPU passes' timers.

## Order of work

0. **Groundwork, nothing audible.** Retail's gains computed once (same
   output). The ambience bus with its own voices, ramps and streaming.
   The Ambience volume (settings, the Sound menu in six languages, the
   launcher). `Remastered.pak`, its builder and manifest. `--audio-out` and the
   `ambience` log lines. Checked: the same samples from today's mixing
   and the new one for the same voices.
1. **The beds**, type by type, with the limiter, the ducking and the
   sweeteners: the Brain and the airless worlds first (the local
   sounds cover them), then open air, snow, desert, volcanic, storm, the
   station, and Artoria's drone with its power-down.
2. **The weather's sounds with the sky's weather**: Tabuchi's rain, gusts
   and thunder land with the sky's step E3; the lightning worlds (5, 22,
   17) with step E5; the snow worlds' wind with E2. The shared weather
   state is built with the sky's base (E1). If the sky's weather comes
   later, step 3 goes first.
3. **Placed sources**: shores and liquids from the terrain, lava, the
   machines by name.
4. **Rain on the hull and on metal**, single drops, the first-person
   shell.
5. **Reverb and occlusion**, then the placement of retail's own sounds
   (if agreed), then HRTF as an option.
6. **Drawn wind** with the sky: cloud drift and the rain's slant with E1
   and E3, snow drift and flakes with E2, dust with the red worlds (E5),
   ash and embers with 7 and 17, ripples and foam streaks with the water
   tiling's new waves (item 6).

A first trial of the whole system can use only the local sounds: the
Brain, an airless world, the station, and Tabuchi's light rain with three
thunders and two winds.

## The shopping list

How to read it: **have** names the local file; **maybe** is worth
auditioning (judged from names and lengths only, nothing was heard);
**missing** is a gap. "Var." is how many different recordings. Beds and
weather loops are stereo, placed sources and drops mono. Everything
44.1 kHz or better; 24-bit WAV originals kept outside the repository.

The local sources:
- Pixabay MP3s in `infestation-atmosphere\` (rain, thunders,
  winds), written below from that folder (`rain\...`);
- Lucid Samples, *1000 SFX Production Tools Vol. 3* (`Sample Packs\
  Lucid.Samples.1000.SFX.Production.Tools.Vol.3.WAV\`), 24-bit 44.1 kHz
  stereo. Its paths are written below as Lucid `folder\name`, without
  the "LS-SFX3 " that starts every folder and file name: Lucid
  `Ambience\Humming Device` is `LS-SFX3 Ambience\LS-SFX3 Humming Device.wav`.

### Beds (stereo, 90 to 180 s each, 2 to 3 var.)

| Need | Worlds | What is on hand | Status |
|---|---|---|---|
| Open air, mild breeze | 1, 6, 12, 15, 18 | | missing |
| Cold wind with a high edge | 2, 10, 23 | Pixabay `winds\jci-21-wind-blowing-sfx-12809.mp3` (36.4 s); Lucid `Ambience\Noise\Windy atmosphere` (12.8 s) | maybe, as layers; long loops missing |
| Still cold night air | 3 | Lucid `Ambience\Noise\Distant Muffled Noise 1-2` (19, 22 s) | maybe, as a layer; missing |
| Dry desert wind | 4, 13, 21 | | missing |
| Volcanic rumble and hot wind | 7, 17, 24 | Lucid `Ambience\Sub Bass\Sub Bass 1-6` (10 to 15 s), `Ambience\Noise\Distant Muffled Noise 1-2` | maybe for the rumble; hot wind missing |
| Storm air, gusty | 5, 9, 20, 22 | the two Pixabay winds | maybe, short; long loops missing |
| Airless hum | 8, 14 | Lucid `Ambience\Humming Device` (16.8 s), `Ambience\Sub Bass\Sub Bass 1-6`, `Ambience\Noise\Ambience Noise 1-5` (17 to 52 s) | maybe, good candidates |
| Station hum, metal | 16 | Lucid `Ambience\Noise\Machine Ambience Noise 1-3` (15 to 50 s), `Ambience\Metallic\Metallic 1-4` (35 to 64 s) | maybe, good candidates |
| Clarifier drone, and its power-down | 11 | Lucid `Ambience\Noise\Modulated Ambience Noise` (71 s), `Ambience\Humming Device`; `Power Down\Power Down Fx 01-21` | maybe |
| The Brain: drone, pulse, wet textures | 0, 19 | Lucid `Ambience\Mystical` (244.8 s), `Mysterious\Mysterious 01-28` (2 to 83 s), `Ambience\Sub Bass\Sub Bass 1-6` (the pulse), `Noise FX\Wet Noises\Wet Noise 01-05` (5 to 11 s), `Ambience\Underwater`, `Ambience\Underwater 2` (33, 47.5 s) | maybe, likely enough to build it |

### Weather

| Need | Var. | Length | What is on hand | Status |
|---|---|---|---|---|
| Rain, light, outdoors | 2 | 60 to 120 s | Pixabay `rain\liecio-light-rain-109591.mp3` (104.4 s, 44.1 kHz stereo) | have 1 of 2 |
| Rain, heavy, outdoors | 2 | 60 to 120 s | | missing |
| Rain on water (a lake's surface) | 2 | 60 s | | missing |
| Rain on metal, outside (roofs, a vehicle) | 2 | 30 to 60 s, mono | | missing |
| Rain on the hull, inside (a car or cabin) | 2 | 60 s | | missing |
| Single drops on metal (thin sheet, hollow tank, plate) | 24 to 40 | 0.2 to 1 s, mono | | missing |
| Wind, calm | 2 | 60 to 120 s | Pixabay `winds\jci-21-wind-blowing-sfx-12809.mp3` (36.4 s) | maybe; long loops missing |
| Wind, strong | 2 | 60 to 120 s | | missing |
| Gusts | 8 to 12 | 3 to 10 s | Pixabay `winds\dragon-studio-cemetery-wind-467488.mp3` (12.6 s, 48 kHz); Lucid `Ambience\Noise\Windy atmosphere` | maybe 2; most missing |
| Thunder, near (crack and boom) | 6 to 8 | 5 to 15 s | Pixabay `thunders\u_vrs223ln83-loud-thunder-439064.mp3` (5.5 s), `thunders\universfield-thunder-strike-124463.mp3` (11.1 s) | have 2; 4 to 6 missing |
| Thunder, far (low roll) | 6 to 8 | 8 to 20 s | Pixabay `thunders\freesound_community-big-thunder-clap-99753.mp3` (11.5 s, 24 kHz: dull, fine only far) | have 1; more can be made from the near ones (duller, lower); 3 to 4 missing |
| Snow and sand hiss on gusts | 2 + 4 gusts | 30 to 60 s | | missing |
| Ember and ash crackle | 2 + 8 | 30 s loops | | missing |

### Placed sources (mono loops, 15 to 40 s)

| Need | Var. | What is on hand | Status |
|---|---|---|---|
| Shore lapping, calm | 3 | | missing |
| Shore lapping, choppy (storm, world 6's waves) | 2 | | missing |
| Stream or river, flowing | 2 | | missing |
| Thick liquid bubbling (toxic water, the Brain's liquid) | 2 | Lucid `Noise FX\Wet Noises\Wet Noise 01-05`, `Ambience\Underwater`, `Ambience\Underwater 2` | maybe, processed |
| Lava: bubbling and hiss, plus pops | 2 + 8 | | missing |
| Generator hum | 2 | Lucid `Ambience\Machines\Machine Noises 1-5` (11 to 20 s), `Ambience\Humming Device` | maybe |
| Transformer hum, plus crackles | 1 + 6 | Lucid `Misc Sounds\Miscs\Electric noise` (11.3 s), `Misc Sounds\Miscs\Electric discharge` (0.6 s) | maybe; more crackles missing |
| Drill or rig | 1 | Lucid `Misc Sounds\Miscs\Drill` (7.4 s, short for a loop) | maybe |
| Laser grid buzz (16), shield dome hum (8) | 1 each | Lucid `Misc Sounds\Miscs\Electric noise`, `Ambience\Humming Device` | maybe |

### Sweeteners (one-shots, sparse)

| Need | Var. | What is on hand | Status |
|---|---|---|---|
| Distant rumbles and booms | 8 | Lucid `Explosions\Explosion FX 01-24` (made far and dull), `Ambience\Noise\Distant Muffled Noise 1-2` | maybe |
| Metal creaks and groans | 8 to 12 | | missing |
| Ice creaks and cracks | 6 to 8 | | missing |
| Rock falls, distant | 6 | | missing |
| The Brain: wet squelches, groans, single heartbeats | 8 to 12, 6 to 8, 4 | Lucid `Noise FX\Wet Noises\Wet Noise 01-05`; the short `Mysterious\Mysterious` 19, 20 and 22 to 27 (2 to 8 s); `Ambience\Sub Bass\Sub Bass 1-6` | maybe |
| Distant eruption booms | 4 to 6 | Lucid `Explosions\Explosion FX 01-24`, made far | maybe |
| Jets passing far overhead (Artoria) | 4 to 6, stereo, 6 to 12 s | | missing |

Not needed: waterfalls (the game has none), birds and insects (we
recommend none; Lucid `Misc Sounds\Miscs\Crickets` stays unused), an
under-water bed (the camera never goes under; Lucid `Ambience\Underwater`
covers it if ever). Lucid `Mysterious\Mystic Flute\Mystic Flute 01-18`
are melodic and would fight the music; the `Sci-Fi\Sci-Fi 01-18`
one-shots read as game effects rather than places.

In short, the gaps to look for: heavy rain, rain on water, rain on metal
outside and inside a vehicle, single drops on metal, long wind loops
(calm and strong) and gusts, 4 to 6 more thunders (near and far), outdoor
air beds (mild, cold, desert, volcanic, storm), shore lapping (calm and
choppy) and flowing water, lava, fire crackle, metal and ice creaks, rock
falls, and far-off jets.

### Where the gaps are likely found

- **The free Sonniss GDC bundles**: rain of every kind (on water, on
  metal, inside vehicles), long wind loops, thunder, shore and river water,
  fire and lava-like bubbling, creaks, machinery. Most of the missing rows.
- **CC0 packs** (free for anything, the safest if a file ever lands in a
  public repository): cluckfox "Rain and Thunder", OpenGameArt "Rain + Long
  Thunder", BigSoundBank (check each file's licence page).
- **Paid, only if the free ones fall short**: BOOM Library "Thunderstorm
  Chaser" (thunder), A Sound Effect "Rain Designer: Int, Ext and Single
  Drops" (the drops on metal and the inside-the-car rain, the hardest
  rows to fill), Sonniss "Textured Rain", SoundMorph "RainScapes".

Nothing was bought or downloaded.

### Found in the local packs (2026-10-23)

Sonniss's GDC bundles 2015, 2016 and 2026 (their
sample selections) and a "Designer's Choice" set in `Sample Packs 4`,
and bought a loop on Splice. Judged by names and lengths, as above
(96 kHz stereo unless said):

| Need | Candidates |
| --- | --- |
| Rain light, medium, heavy | Soundopolis Nature's Fury Rain: Light 240 s, Medium 126 s, Heavy 115 s, Splatty Heavy 118 s |
| Rain on water | Soundopolis Rain Splatty Puddle Drips 107 s (partly) |
| Rain on the hull, inside | Jake Fielding Heavy Rain on Window 31 s, Hail on Door Window 66 s; GAF Hard rain, plexiglas ceiling, inside 11 s; The Noisery Rain Light Interior Hail Storm 84 s |
| Wind, long | Soundopolis Wind Heavy 150 s; 344 Audio Eye Of The Storm 180 s (192 kHz); The Noisery Wind Strong Gusts Hurricane 162 s, Wind Strong Metal Rattle 54 s; Pole Position snow wind strong gusty 54 s (the snow worlds) |
| Gusts, howls | Hzandbits Howling Winds (a 9 s gust, howls 33 s, sleet gusts 91 s), Urban Winds (metal clinking 69 s) |
| Thunder | Soundopolis Thunder Boom 37 s, Crack 30 s, Rumble Deep 37 s, Rumble Mid 5 s, Thunderstorm Distant 191 s; Jake Fielding Interior Thunder Rumble 19 s; ESM Texas storm 132 s; InMotionAudio storm beds 205 s and 151 s; with the three Pixabay thunders |
| Shore lapping | ESM Lake Dock Water Ripples Wake Wave Gentle 103 s; Soundopolis River Lapping 31 s, Lake Huron Evening 156 s; Beach Ocean Waves 203 s (choppy) |
| Flowing water | Sound Ex Machina river flows 39 to 57 s, small waterfall 118 s, turbulent flow 114 s; Soundopolis waterfall 55 s |
| Lava | GAF Yellowstone mud pot 91 s (bubbling), geysers; ShapingWaves Planets 16, low eruptions 170 s |
| Fire | Ivo Vicic Campfire crackling 121 s |
| Airless worlds | Julian Ray Sounds of Space (8, 14 to 93 s); SoundMorph Solar Sky ship rumble low pulse 53 s; 3maze Ultra Drones 59 s and 45 s; Lucid's hums |
| The Alien Brain | Splice ESM "dark crypt ... deep horror demon tension" 145 s (48 kHz); Whatisvalis unEarthy Ambience "pulsing darkness" 95 s (a pulse), "gates of mydian" 94 s; SoundMorph Doom Drones; Lucid's Mystical and Mysterious |
| Alien planet beds | ShapingWaves Planets 08, buzzing cave entrance 104 s |
| Machines | Detunized HumBuzz (hum, buzz, tonal, short); Julian Ray Background Machinery (4 to 24 s); Lucid's Humming Device and machines |
| Ice, rock, jets | Pole Position snow; 3maze Ice Block; Kopeikin Rocks; Timothy McHugh Over Head Aircraft; Designer's Choice Jet By Overhead |

Then `Sample Packs 4\DesignersChoice-Rain`, 12 files of "The
Designer's Choice UCS Pack #59 - RAIN" (Freesound pack 43669, mostly
smartphone recordings, 44.1 or 48 kHz, 16-bit):

| Need | Candidates |
| --- | --- |
| Rain on metal, outside | rain on a car roof 32 s (filed as rain on glass; it sounds like rain on metal) |
| Rain on the hull, inside | a rainy night in a car 16 s |
| Rain beds | drizzly 211 s, light 305 s, heavy 88 s, distant heavy 49 s, raining 75 s, heavy with rumbling wind 280 s, rain with an overhead jet, thunder and wind 343 s, a freezing winter night 18 s |
| Rain on cloth, wood | an umbrella 49 s (mono), wood 43 s |

Its licence is Attribution 4.0 for every file (the readme adds "you can
credit me or not"): credit Nicholas A. Judy, The Designer's Choice, in
the port's credits. The pack's page lists more surfaces than these
twelve (metal, water, plastic, concrete, glass).

And `Sample Packs 4\323683__balz__rain_on_metal.wav` (111 s, mono,
48 kHz, 24-bit): chosen for drops on metal: not much rain, mostly drops
hitting metal, which is what is wanted. The
hull's drip layer, and the single drops cut from it.

### Found in Sonniss's GDC 2018, 2019 and 2020 bundles (2026-10-23)

The full bundles (984, 1,112 and 1,464 files), judged the same way. The
best of what they add:

| Need | Best candidates |
| --- | --- |
| Drops on the hull | Articulated Sounds Moody Rain Loops, metal plate with asynchronous drips (60 s loop); PMSFX Rain Designer, drippy on iron plate (98 s); Pole Position, medium rain with drops on metal (99 s) |
| Rain in the cockpit | Borg Sound, rain inside a car and inside a metal-roofed shed; Coll Anderson, light rain on a car roof (ambisonic) |
| Wind | Articulated Sounds Rare Winds (beach storm 125 s, mountain gusts 48 s, and a synthetic eerie howl 96 s for the alien worlds); Sound of Essen Tonal Wind (howling 128 s, flute-like 138 s, mono); Ivo Vicic Bora (wires whistling in a hurricane 217 s); Soundrangers (desert 61 s, harsh mountain 32 s); a whistling blizzard for the snow worlds |
| Thunder | Soundrangers Thunder (clap, strike, big crack, distant; 8 to 15 s); Pole Position, a distant storm approaching (57 s) |
| Lava | Discover Oregon, Kilauea's lava (close 101 s, windy 200 s, crackles 22 s; 44.1 kHz); Charlie Atanasyan, an Ethiopian volcano erupting and degassing (110 s) |
| Liquid, bubbling | Articulated Sounds, continuous bubbles with a low resonance (22 s loop) |
| Shore lapping | Soundrangers, gentle lake lapping (60 s loop); BlueZone lake waves (55 s and single waves); PMSFX, waves lapping between rocks (225 s) |
| Under water | Spectravelers Submersion, abyss (133 s); Ivo Vicic underwater textures (64 to 191 s) |
| Alien beds | 344 Audio Alien Atmospheres (a huge planet's rumble, a radiation planet, an alien rainforest; 180 s each); BlueZone Alien Planet (wind 32 s, night creatures 56 s); Submerged Tapes alien atmospheres (126 to 132 s) |
| Airless worlds | Fox Audio Space Ambiences (60 s each); Bluezone Lunar (44 s) |
| Generators, transformers | Ivo Vicic, an overhead power line in wind (87 to 267 s); BlueZone Dark Mesa, industrial and underground-lab beds, electric crackling (30 s) |
| Electric arcs | Sound Spark, a seamless deep arc loop (10 s); Hear and Now high-voltage arcs: the transformer's crackle and the warpgate |
| Metal creaks | Rock The Speakerbox, mine-shaft metal stress (long); toneglow, a metal fence creaking in strong wind; Sound Spark Metal Souls, groaning depths; a junkyard van door |

Still missing: rain falling on open water. The nearest are rain on waves
and rocks and a cave mouth's drips. It can also be built from a lapping
bed and scattered single drops. These bundles carry Sonniss's GDC
licence, as the other years. Their `License.pdf` was not read: its text
does not extract without a PDF reader.

### Sound credits

What the port must credit if these ship (its credits or readme), and
what each licence asks:

- "Rain_on_metal.wav by balz -- https://freesound.org/s/323683/ --
  License: Attribution NonCommercial 4.0". Non-commercial: the port must
  not be sold while it carries this sound (fine for a game that is not
  sold).
- The Designer's Choice UCS Pack #59 - RAIN, by Nicholas A. Judy
  (https://freesound.org/people/designerschoice/packs/43669/),
  Attribution 4.0.
- Sonniss GDC bundles (2015, 2016, 2018 to 2020, 2026) and Splice: no
  credit asked; not to be passed on as files.
- Lucid Samples: its licence names music productions only; asked about
  before anything ships.

The Sonniss bundles' licence allows use in any number of projects with
no credit (not as a sound library of their own, and not for training
AI); Splice's sounds are royalty-free in one's own works, not to be
passed on as files. Only Lucid's stays a question.

## Licences

As we understand them; each licence's own text decides.

- **Royalty-free game licences** (Sonniss, BOOM, A Sound Effect,
  SoundMorph): the sounds may ship inside the game, without credit, but
  the raw files may not be shared. So they stay out of any repository and
  ship only built into `Remastered.pak`.
- **Pixabay** (the MP3s): free to use in a game without credit;
  not to be passed on or sold as they are.
- **Lucid Samples**: the licence allows use "for commercial and
  non-commercial music productions" and forbids copying, sharing or
  reselling. A game is not named. Use them for the trial now, processed
  and layered into our own beds; ask Lucid Samples in writing whether
  that covers a game before shipping them.
- **CC0**: anything goes, including a public repository.
- The decomps repository is private; even so, the manifest goes into git
  and the sound files do not.

## Open questions

1. **The switch.** An Ambience slider (0 to 15, 0 = retail's sound
   exactly), on by default under Remastered, absent under Vanilla.
   Recommended. Default level: about 10, tuned by ear on the first sound
   sheets?
2. **Thunder's knob**: the Ambience volume (recommended), not the Sound
   volume?
3. **Retail's own sounds under Remastered**: placed by direction, duller
   far away, in the place's echo, muffled under water. Recommended. Doppler
   on missiles and ships: recommended against.
4. **Headphones or speakers?** It decides how soon the HRTF option is
   worth doing (last, either way).
5. **The worlds' moods**: the table above; above all the Brain's pulse
   (about 50 a minute), the airless worlds' near-silence, Artoria's drone
   powering down with its light, and no animal calls anywhere.
6. **Music and beds**: the music stays retail's, the beds sit under it,
   nothing turns the music down. Should the beds rise when the Music is 0?
   We recommend not: the Ambience slider does that. (Since 2026-10-28,
   on the Brain's worlds the beds play instead of the music; see the
   Brain's notes above.)
7. **Oolbryte (20)**: rare far rumbles with no flash (a storm beyond the
   horizon), or thunder only where lightning shows? We lean to the
   rumbles, rare and very low: they tell the storm's story.
8. **Drawn wind**: the worlds in the table above, strongest on 9, 10, 4
   and 21? Recommended.
9. **The Lucid pack**: may we write to Lucid Samples to ask about use in a
   game? Recommended; until they answer, its sounds serve the trial only.

## Not checked

- No sound was listened to: "have" and "maybe" are judged from names and
  lengths, and that retail's 58 clips hold no weather is inferred.
- Terrain furniture was not counted: whether worlds 11, 12 and 14 place
  the transformer (`gentrans`) is open.
- World 0's music; Sanrel-Dek's lava material; Luna Azine's dome model.
- The mixer's and Lewton's CPU time (estimates only); the volume settings
  in use.
- Retail's own mix (DirectSound) was not recorded; the peak figures are
  the port's mix.
