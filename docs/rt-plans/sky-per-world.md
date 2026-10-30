# The sky, world by world: survey and plan

Status: Stage 2, held on
2026-10-23, the painted sky being fine for now. The base and the first
recipes are built and switched off: every world ships retail's painted sky,
without clouds or cloud shadows.

## What was reported

The hybrid sky showed an obvious, bad separation between the painting and
the added clouds. The approach asked for: check each skybox, one after the
other, for every world, and write a document for each with what is
observed. Some worlds have realistic clouds, very similar to the clouds
earth has, so the same sky colour backdrop can carry clouds of the same
style. Tabuchi looks like it has lightning and should have rain with
blue-green-ish droplets. So: ditch the hybrid approach and do the whole
sky, case by case.

Artoria looks like it has a whole city in the distance and an orange sky;
the briefing states that the planet is artificially illuminated, which is
why the light also goes down when the generators are destroyed.

The reported screenshot (`.tmp/plan-sky/user-hybrid-sky.jpg`) is Command Centre
(world 18). The painting's white cumulus ends in a hard curved line. Above
it, the hybrid's clouds are smooth beige-grey blobs in another style and
colour. The line is the painting's top edge.

## How the survey was done

Everything is in `.tmp\plan-sky\`. Paths below are
relative to that folder.

- **Snapshots.** The port's copy of `inf-game.exe`, Vulkan with path
  tracing, hidden, fixed 16 ms steps, the hybrid's clouds off
  (`{"sky": {"clouds": 0}}`), so the sky is retail's painting. For every
  world 0 to 24: the first start at headings 0, 90, 180 and 270 degrees
  (the start harness takes degrees), a third-person shot at frame 40, then
  F12 and a first-person shot at frame 75; and a VTOL run that climbs and
  pitches up in first person (shots at frames 120, 160, 200).
  - `shots/wNN_retail_hK_3p.jpg`, `shots/wNN_retail_hK_fp.jpg` (K = 0 to
    3 for 0, 90, 180, 270 degrees), `shots/wNN_retail_up_fF.jpg`.
  - `sheets/wNN_retail.jpg`: one sheet per world (the 11 shots and the
    painting's tiles).
  - `sheets/overview_retail_0.jpg` to `_4.jpg`: five worlds a sheet.
  - Rerun: `python sweep.py --worlds 0-24 --tag NAME --tuning '{...}'`, then
    `python sheets.py --tag NAME`.
- **The paintings themselves**, read from the extracted sprites
  (`extracted/sprites`): `paint/strip_wNN.png` and
  `sheets/paintings_w01-w06.png` (and the next three sheets). Colours
  measured from them: `paint/palette.txt`.
- **Retail's data** for each world: the clear colour (H7 `0x72`), the far
  colour (`0x73`), the fog colour (`0x74`), the light colour (H6 `0x37`)
  and its schedule, the light direction (the terrain's prefix), and the
  backdrop model (H8 `0x116`) with what it instances. All from the
  extracted manifests.
- **Extra runs.** Artoria's other starts (`sheets/w11_artoria_starts.jpg`);
  Artoria going dark when its generators die, through the Cheats Menu's
  Complete Level as the lab does it (`sheets/w11_artoria_dark_before_after.jpg`);
  the Alien Brain's clouds nine seconds apart
  (`sheets/w19_clouds_turn_9s.jpg`); and retail against the hybrid and the
  existing procedural sky in five worlds
  (`sheets/compare_retail_hybrid_modern.jpg`).
- **Limits.** Worlds 0, 3, 6, 7, 13, 19 and 24 refuse flying ("Your craft
  cannot fly on this world"); Artoria has a ceiling and starts in a canyon;
  world 22 plays its intro, whose camera ignores the heading. Their look-up
  shots show little sky, and their first-person shots carry the survey.

## What retail's sky is made of

These hold across the worlds, with the exceptions named.

- **Two painted bands and a skirt.** The backdrop is a ring about 16,000
  to 27,500 units out, placed at the eye's X and Z and at the world's
  backdrop height. In most worlds it carries two painted images, each 500
  by 100 pixels (five 100-pixel tiles). Image B (tiles 5 to 9) is the
  lower band, image A (tiles 0 to 4) the band above it. Below them a flat
  colour skirt closes the ring. Above them is nothing: the frame's clear
  colour fills the sky.
- **Each image covers a quarter turn and repeats four times** around the
  horizon (four quads per tile). So a painted glow or sun shows four times.
  Luna Azine's nebula is the exception: drawn once.
- **The paintings were made to blend.** The top row of image A equals the
  clear colour in almost every world (Terra Nova (130, 197, 180) against
  (132, 197, 181); Artoria (140, 74, 0) exactly). The bottom of image B
  matches the far colour in most worlds (Exxodus (67, 0, 0) against (66, 0,
  0)). Command Centre is the exception: its painted sky is a greener, duller
  blue than the clear colour above, (43, 117, 176) against (41, 107, 197).
  So even retail (OpenGL) shows a curved seam when you look up there
  (`sheets/w18_opengl_vs_vulkan.jpg`).
- **Some worlds hang more models on the backdrop.** The backdrop's program
  instances them, so they follow the eye too:
  - Luna Azine (8): `stars01` (a star dome), `spacegradient` (a brown
    glow), and a large yellow sun-burst sprite drawn twice.
  - Tabuchi (9): `mist`, five additive tiles of green glowing cloud.
  - Foldrak Noir (14): `moonstar` (stars and a photographed Moon) and
    `moongradient` (a teal glow).
  - Defence Satellite (16): `moon2star` (stars and the same Moon).
  - Command Centre (18): `commandmountains` (despite its name, the two
    cumulus images) and a white sun sprite.
  - Alien Brain (0, 19): `cloud` twice, pink and teal, turning at
    different speeds (they visibly shift in nine seconds).
- **Today those extra models are traced as world geometry.** The capture
  (`scene_capture.rs`) marks only H8 `0x116` itself as sky. Its instanced
  models are unlit quads, so they go to the tracer as emissive surfaces 16,000
  or more units away, and the haze greys them as it greys far ground. That
  is why Command Centre's cumulus looks duller under Vulkan than under
  OpenGL, and why it survives even the `modern` sky
  (`compare_retail_hybrid_modern.jpg`, fourth row).
- **Every world's light is high.** The light comes from 38 to 74 degrees
  above the horizon. The painted suns and glows never match it:
  - World 1's painted sunset sun sits on the horizon. Its light comes from
    57 degrees.
  - Command Centre's sun sprite sits about 15 to 28 degrees up. Its light
    comes from 74 degrees.
  - Exxodus, Vernimab and Artoria paint a bright glow into their images.
    It repeats four times around the horizon, so it has no single direction.
- **The painting is drawn unlit.** Its quads (opcode E2) take no shade, so
  the world's light schedule never touches it. On Artoria, after the four
  generators die, the towers and ground go near black while the painted sky
  stays golden. In the Vulkan run, its band's mean fell only from (147, 75, 6)
  to (81, 40, 6), and that drop is the mist laid over it.
- **The light changes in four worlds** (H14's tint schedule,
  `world_tint.rs`):
  - World 1 is day (186, 133, 105) for about 7 minutes, sunset (220, 117,
    89) by about 13, then night (32, 28, 57) by about 20 (400-second steps).
  - Artoria (11) holds gold (234, 128, 46) until its generators die, then
    falls to (50, 25, 10) in one second.
  - Kankutan (15) swings between (170, 128, 190) and half of it every 90
    seconds for 15 minutes.
  - Command Centre (18) swings by about a tenth every 90 seconds.
  - Altair Prime's schedule alternates two identical colours, so nothing
    shows there.
- **Why the hybrid seams.** It keeps the painting and adds a flat noise
  layer above its top edge (`pt_composite.frag`, from `frame.sky_top`). Two
  things clash at that edge:
  - **Style.** Photographed, painted clouds below; smooth blobs above.
  - **Colour.** The added clouds take the world's light paled. That gives
    beige over Command Centre's white cumulus and pale green-white over
    Tabuchi's dark veins.

  The fade over 0.08 of a sine cannot hide a change of style.
- **The existing procedural sky** (`sky.modern`) is a gradient, a sun
  glow, stars and the same flat layer. It is plainer than every painting
  (the review of 2026-10-22), and near the horizon its layer thins to
  streaks.

## The worlds at a glance

| World | Name | Painting | Proposed sky |
|---|---|---|---|
| 0 | (Alien Brain's map) | as 19 | as 19 |
| 1 | (unnamed prototype) | golden sunset, moon, small clouds, a twister | golden cumulus, day to sunset to night |
| 2 | Terra Nova | teal sky, towering cumulus | realistic cumulus, teal |
| 3 | Stendahl Alpha | moonlit night, silver-edged clouds | moonlit broken overcast, a moon |
| 4 | Exxodus | crimson, coral cloud, red mountains | wind-torn coral cloud, mountain band |
| 5 | Altair Prime | purple storm cumulus over sulphur yellow | storm towers, yellow gaps |
| 6 | Helniazine | mauve dusk, soft streaks | mauve stratus, pale gold gaps |
| 7 | Sanrel-Dek | black, fiery swirls | ash deck lit from below by lava |
| 8 | Luna Azine | stars, orange nebula, sun-burst | space: stars, nebula, sun with glare |
| 9 | Tabuchi | slate sky, storm with green veins | storm, green glow, lightning, blue-green rain |
| 10 | Pillier Stren | pale blue, white cumulus | realistic cumulus, cold light |
| 11 | Artoria | golden, dark bands, dark hill | golden overcast lit from below, goes dark |
| 12 | Shalapreth | sand-coloured haze, cumulus | hazy warm cumulus |
| 13 | Grach'to | pale desert sky, red mesas | clear hazy sky, mesa band |
| 14 | Foldrak Noir | stars, Moon, ice ridges | space: stars, Moon, ridge band |
| 15 | Kankutan | magenta dusk, silver-pink streaks | magenta stratocumulus, light that breathes |
| 16 | Defence Satellite | stars, green glow, Moon | space: stars, green glow |
| 17 | Shak'tahh | orange fire sky, plum ash clouds | ash storm lit by lava |
| 18 | Command Centre | blue sky, white cumulus, desert mountains | Earth summer cumulus, mountain band |
| 19 | Alien Brain | black, turning pink and teal clouds | glowing turning "tissue" clouds |
| 20 | Oolbryte | grey storm, cream horizon gap | breaking storm, bright horizon |
| 21 | Vernimab | as 4 | as 4 |
| 22 | Homeworld | as 5 (the intro's world) | as 5 |
| 23 | Caspadril | teal sky, small cumulus, snow mountains | crisp fair-weather cumulus, mountain band |
| 24 | Caradion | salmon sky, orange cumulus, red mountains | warm cumulus, mountain band |

## World by world

Each section lists the shots, retail's numbers (clear colour, far colour,
light colour and the light's elevation), what the painting shows, its mood,
whether a painted sun agrees with the light, and a recipe. Colours are 0 to
255. "Clouds" in a recipe means volumetric clouds (see the plan). "Band" means
the painting's silhouette kept as a horizon layer.

### World 0 (the Alien Brain's map)

- Shots: `sheets/w00_retail.jpg`, `shots/w00_retail_h2_fp.jpg`.
- World 0 loads world 19's map. Its sky matches world 19's in every shot.
  Its name slot holds "Cheat Accepted", so it is not a campaign world.
- Recipe: world 19's.

### World 1 (unnamed)

- Shots: `sheets/w01_retail.jpg`, `shots/w01_retail_h1_3p.jpg` (the painted
  sun), `shots/w01_retail_up_f200.jpg`. Sprites: `paint/world1_sprites.png`.
- Retail: clear (66, 74, 66) grey-olive; far and fog (99, 99, 99); light
  (186, 133, 105) from 57 degrees, then sunset and night (above). Grey
  water.
- Painting: its backdrop differs from all the others. A low band of a
  photographed golden sunset, the sun in it, sits under a band of golden
  cumulus. Separate sprites hang in it: a full Moon among the cumulus, two
  small lit clouds higher up, and an additive twister (a dust devil) on the
  horizon. From the VTOL: scattered small golden clouds on grey-olive.
- Mood: late golden hour; calm.
- Sun: painted on the horizon; the light is at 57 degrees. They disagree.
- Recipe: scattered golden cumulus and altocumulus (lit gold (209, 151,
  87), olive-grey shade (67, 72, 59)), cover 0.35; grey-olive zenith to a
  warm horizon; the sun disc where the light is. Follow the schedule: the
  sky reddens towards the sunset style, then goes to night with stars and
  the Moon (retail's sprite). Keep the twister as a far billboard if wanted.
- Note: no name ("Completed" in its slot), its briefing is titled Command
  Centre, and its perships miss a model. It looks like a prototype level.
  Last in the queue.

### World 2: Terra Nova

- Shots: `sheets/w02_retail.jpg`, `shots/w02_retail_up_f160.jpg`.
- Retail: clear (132, 197, 181) teal; far and fog (33, 115, 82) green;
  light (128, 199, 178) from 53 degrees. No water.
- Painting: towering cumulus with sunlit tops on a teal sky (image A) over
  layered darker cloud (image B). Photographed Earth clouds, tinted green.
  The top blends into the clear colour: retail shows no seam here.
- Mood: a bright, mild day on a green-tinted Earth.
- Sun: none painted.
- Recipe: realistic cumulus congestus (tall, cauliflower tops, flat dark
  bases), lit (180, 242, 218), shade (47, 138, 115), cover 0.4, base about
  4,000, tops to 12,000. Teal zenith to a paler teal horizon that meets the
  green haze. Sun disc at 53 degrees in the light's colour. Strong drifting
  cloud shadows on the snow. No band (the mountains are real terrain).

### World 3: Stendahl Alpha

- Shots: `sheets/w03_retail.jpg`, `shots/w03_retail_h0_fp.jpg`.
- Retail: clear and far (8, 25, 25) near black; fog (0, 49, 49); light (80,
  144, 144) from 46 degrees. Clear teal water. Flying refused.
- Painting: a night sky. Dark teal-grey stratocumulus whose edges catch a
  bright white light (193, 196, 186), as if a moon stood behind them.
- Mood: cold, moonlit, quiet.
- Sun: none painted. The cool dim light reads as moonlight.
- Recipe: a broken stratocumulus deck (cover 0.6, base 3,500, thin) with
  silver edges and near-black cores. A moon disc with a soft halo where the
  light comes from; clouds near it glow silver. Stars in the gaps. The
  water mirrors the moonlit clouds.

### World 4: Exxodus

- Shots: `sheets/w04_retail.jpg`, `shots/w04_retail_up_f160.jpg`,
  `shots/w04_retail_h3_fp.jpg`.
- Retail: clear (165, 49, 66) crimson; far (66, 0, 0); fog (58, 0, 0);
  light (240, 80, 80) from 58 degrees. No water.
- Painting: wind-torn coral and peach cloud on crimson (image A); dark red
  mountains against a hot sky with a white-gold glow (image B). From the
  VTOL the painting's top is a visible curve against flat crimson.
- Mood: a hot, dusty red planet at the end of the day.
- Sun: the glow repeats four times; the light is at 58 degrees.
- Recipe: wind-torn altostratus and cirrus in coral and cream (lit (255,
  234, 187), shade (159, 44, 42)), cover 0.5, streaked along the wind.
  Crimson zenith to an apricot horizon (224, 143, 104). One hot white-gold
  glow around the sun at 58 degrees, in place of the four painted ones.
  Band: image B's mountains, cut out against the sky.

### World 5: Altair Prime

- Shots: `sheets/w05_retail.jpg`, `shots/w05_retail_up_f160.jpg`.
- Retail: clear (107, 74, 66) mauve-brown; far (173, 148, 25) sulphur;
  fog (82, 74, 16); light (144, 128, 96) from 50 degrees. Toxic
  yellow-green water.
- Painting: dark plum cumulus towers (dark (15, 8, 17), amber edges (226,
  176, 76)) against a sulphur-yellow horizon, mauve-brown above.
- Mood: oppressive; a storm over a chemical sky.
- Sun: none painted.
- Recipe: cumulonimbus towers with anvils, plum-black with amber rims,
  cover 0.5, gaps of sulphur yellow at the horizon; mauve-brown zenith.
  Optional distant heat lightning, yellow-white, with no rain (a question
  below). The toxic water mirrors the towers.

### World 6: Helniazine

- Shots: `sheets/w06_retail.jpg`, `shots/w06_retail_h1_fp.jpg`.
- Retail: clear (140, 90, 107) mauve; far and fog (123, 74, 74); light
  (133, 72, 112) from 60 degrees. Toxic yellow-green water. Flying refused
  (the briefing: the moon's oxygen-rich air).
- Painting: soft streaky stratus in rose and lilac, with pale yellow
  sunlit gaps near the horizon (255, 248, 193).
- Mood: a soft mauve dusk.
- Sun: none painted.
- Recipe: stratus and altocumulus sheets (cover 0.55), rose-lilac, soft
  edges, slow drift; the pale-gold gaps around the sun's side. A light
  haze, for the dense air.

### World 7: Sanrel-Dek

- Shots: `sheets/w07_retail.jpg`, `shots/w07_retail_h1_fp.jpg`.
- Retail: clear (0, 0, 0); far and fog (49, 0, 0); light (255, 128, 64)
  from 59 degrees. No water; lava rivers on the ground. Flying refused. The
  start is in a canyon.
- Painting: black and deep red with fiery orange-yellow swirls (226, 112,
  19), like smoke lit by flames.
- Mood: a volcanic world about to erupt (the briefing: its seismic
  stabiliser keeps it from erupting).
- Sun: none painted.
- Recipe: an ash deck (cover 0.8, low and thick), near black on top, its
  underside lit orange by the lava below (an "under-light" in the lava's
  colour), with glowing filaments where it is thin. Black sky above. Embers
  drifting up near the lava are optional (a question below).

### World 8: Luna Azine

- Shots: `sheets/w08_retail.jpg`, `shots/w08_retail_up_f160.jpg`,
  `shots/w08_retail_h3_fp.jpg`.
- Retail: clear (33, 16, 0); far and fog (115, 66, 16); light (240, 114,
  4) from 38 degrees. No water. The start is inside a shield dome, which
  tints the view olive.
- Painting: space. Stars on black (`stars01`), a large orange-white nebula
  in a 3 by 3 grid drawn once in one direction, a brown glow at the horizon
  (`spacegradient`), and a big yellow sun-burst sprite.
- Mood: an airless moon under a spectacular nebula.
- Sun: where the backdrop places the sun-burst is not yet read (opcode 27
  at one of its vertices). It should sit where the light is, 38 degrees up.
- Recipe: a black sky with a dense starfield (retail's star tiles as a
  base, sharp procedural stars on top); the nebula kept as an additive sky
  texture in its retail direction; the brown horizon glow; a sharp
  yellow-white sun with a sun-burst glare at the light's direction. No
  clouds, no haze above the horizon.

### World 9: Tabuchi

- Shots: `sheets/w09_retail.jpg`, `shots/w09_retail_up_f160.jpg` (best
  view), `shots/w09_retail_h3_fp.jpg`.
- Retail: clear (74, 74, 123) slate blue; far and fog (90, 156, 82) green;
  light (150, 200, 156) from 43 degrees. A lake of paler grey water.
- Painting: grey-brown and purple storm masses (dark (57, 47, 56))
  threaded with vivid green glowing veins (96, 212, 98) to (103, 227, 119),
  over a slate-blue top. The backdrop adds `mist`: green glowing cloud,
  additive.
- Mood: a toxic electrical storm. The veins read as lightning lit inside
  the clouds.
- Sun: none painted.
- Recipe:
  - A low, heavy storm deck: cover 0.8, base about 3,000, tops to 9,000,
    grey-brown-purple, dark underneath.
  - Green veins glowing inside it: an emissive channel from ridged noise,
    flickering slowly.
  - Lightning: in-cloud flashes, green-white, every few seconds; now and
    then a bolt to the far ground; each flash briefly lights the world.
  - Rain: blue-green streaks (about (115, 215, 190)), slanted by the wind,
    rings on the lake, brighter in each flash.
  - Slate-blue breaks in the deck now and then. The green haze stays.

### World 10: Pillier Stren

- Shots: `sheets/w10_retail.jpg`, `shots/w10_retail_up_f160.jpg`.
- Retail: clear, far and fog (115, 132, 165) pale blue-grey; light (215,
  214, 244) from 56 degrees. No water. Snow everywhere.
- Painting: soft billowing white-blue cumulus on pale blue; hazy below.
- Mood: a cold, bright winter day; Earth-like.
- Sun: none painted.
- Recipe: realistic cumulus (cover 0.45, base 3,000, tops 7,000), white
  tops, blue-grey bases, crisp edges; pale blue zenith to a whiter horizon.
  Sharp sun at 56 degrees. Strong moving cloud shadows on the snow fields
  (where the current shadows already show best). Light snow is optional.

### World 11: Artoria

- Shots: `sheets/w11_retail.jpg`, `shots/w11_retail_h0_fp.jpg` (canyon
  start), `sheets/w11_artoria_starts.jpg`, `shots/w11_artoria_s4h0_fp.jpg`
  (the city), `sheets/w11_artoria_dark_before_after.jpg`.
- Retail: clear (140, 74, 0) gold; far (8, 0, 0); fog (66, 49, 16); light
  (234, 128, 46) from 55 degrees, falling to (50, 25, 10) when the four
  generators die. No water. An altitude ceiling.
- Painting: golden-orange sky with long dark brown cloud bands (image A);
  a golden glow behind a dark hill silhouette, under the same bands (image
  B).
- The city seen in the distance is real geometry, not the painting. Starts 3 and 4
  stand among tall tower blocks, black against the golden sky, with a
  bright blue-white light above one tower. The painted dark hill sits
  behind them.
- Briefing: "the Golden City, so called because of the golden light
  bathing the planet caused by the atmospheric clarifiers needed to thin
  the dense clouds enshrouding the planet." Taking out the power plant
  disables the clarifiers and "should plunge the world into darkness". Also
  "the skies are filled with the dark trails of jet fighters".
- Retail today: the land goes dark, the painted sky stays golden (above).
- Mood: a rich, hazy city under an artificial golden glow.
- Sun: the light is the clarifiers' glow, not a sun. No disc.
- Recipe:
  - A dense overcast (cover 0.9) of brown-black banks (31, 5, 3), lit from
    below and within by the clarifiers' gold (250, 137, 3) to (255, 219,
    2). Brighter golden rifts where the clouds are thinned, brightest
    towards the light's direction. No sun disc.
  - Band: image B's dark hill, cut out, behind the city's towers. Small
    city lights along it are optional.
  - Dark trails of jet fighters across the sky (optional).
  - When the generators die: over retail's one-second fade, the clouds'
    glow flickers and dies with the light. What remains is a near-black
    umber overcast with a faint warm glow low on the horizon.
  - First recipe (2026-10-23): dark brown cumulus banks (cover 0.62, base
    5,000, top 7,500) lit from below in a dim gold, a gold zenith to a
    bright gold horizon, no sun disc, the dark hill as the band (sprites
    1592 to 1596). Through the Cheats Menu's Complete Level the sky falls
    with the light to near-black umber
    (`.tmp/plan-sky/runs/w11_dark_e4c`). Not yet: the flicker as it dies.
  - An overcast (cover past 0.7) now closes toward a full deck; below it
    the weather map still leaves the 3D noise a third to carve.

### World 12: Shalapreth

- Shots: `sheets/w12_retail.jpg`, `shots/w12_retail_h2_fp.jpg`.
- Retail: clear, far and fog (173, 156, 99) sand; light (192, 176, 160)
  from 43 degrees. Grey water (the reservoir that rises at the end).
- Painting: Terra Nova's and Pillier Stren's cumulus in sand colours,
  hazy, low contrast.
- Mood: a warm, dusty, hazy day on a moon of Artoria.
- Sun: none painted.
- Recipe: soft hazy cumulus (cover 0.4) in pale sand (lit (215, 207, 153)),
  low contrast; sand zenith to a dusty horizon; a broad, soft sun glow.

### World 13: Grach'to

- Shots: `sheets/w13_retail.jpg`, `shots/w13_retail_h3_fp.jpg`.
- Retail: clear (222, 214, 222) pale lilac-white; far and fog (115, 74,
  66); light (221, 175, 234) from 45 degrees. Clear teal water. Flying
  refused.
- Painting: a pale, cloudless sky (image A's top) over red sandstone mesas
  (image A's lower part, and the rock slopes of image B), like a desert
  canyon on Earth.
- Mood: hot, bright, dry.
- Sun: none painted.
- Recipe: a clear, bright, hazy sky (pale lilac zenith, white haze at the
  horizon), a wide soft glow around the sun, at most a few thin cirrus
  streaks (cover 0.1). Band: the mesas, cut out against the pale sky.

### World 14: Foldrak Noir

- Shots: `sheets/w14_retail.jpg`, `shots/w14_retail_h0_fp.jpg` (the Moon
  behind the ridge), `shots/w14_retail_up_f160.jpg`. Sprites:
  `paint/space_sprites.png`.
- Retail: clear (0, 0, 0); far and fog (66, 99, 99); light (180, 246, 240)
  from 47 degrees. No water.
- Painting: space. A black starfield (`moonstar`), a teal glow at the
  horizon (`moongradient`), icy grey-white ridges cut against the sky, and a
  photographed Moon rising behind a ridge.
- Briefing: a former roaming meteor "captured by the gravitational pull of
  the enormous artificial alien homeworld".
- Mood: an airless ice moon.
- Sun: none painted.
- Recipe: black sky, dense stars, the teal horizon glow, the Moon (retail's
  sprite) where retail puts it, the ridges as a band. A small sharp sun with
  glare where the light is. Optionally the alien homeworld as a huge
  artificial planet in the sky (a question below).

### World 15: Kankutan

- Shots: `sheets/w15_retail.jpg`, `shots/w15_retail_h1_fp.jpg`.
- Retail: clear (58, 16, 33) maroon; far (197, 140, 148) rose; fog (132,
  99, 82); light (170, 128, 190), swinging to half and back every 90
  seconds. Grey river water.
- Painting: Stendahl Alpha's cloud shapes recoloured: streaky
  stratocumulus with silvery-pink edges (226, 183, 193) on maroon, fading
  to rose at the horizon.
- Mood: a long magenta dusk.
- Sun: none painted.
- Recipe: magenta-pink stratocumulus (cover 0.6) with silver-pink lit
  edges; maroon zenith to a rose horizon. Let the 90-second swing read as
  thick banks passing over the sun: the sky dims with the light and the
  cover thickens in the dim half.

### World 16: Defence Satellite

- Shots: `sheets/w16_retail.jpg`, `shots/w16_retail_up_f200.jpg`.
- Retail: clear (0, 0, 0); far (49, 107, 49) green; fog (49, 49, 49);
  light (215, 240, 214) from 44 degrees. No water. Deep metal trenches; a
  laser grid kills you if you fly too high.
- Painting: stars (`moon2star`), a green glow low over the horizon, and the
  same photographed Moon.
- Mood: in orbit; mostly a strip of black sky between trench walls.
- Sun: none painted.
- Recipe: black sky, stars, the green horizon glow, the Moon, a sharp sun
  with glare. Faint lines of the laser grid overhead are optional.

### World 17: Shak'tahh

- Shots: `sheets/w17_retail.jpg`, `shots/w17_retail_up_f160.jpg`.
- Retail: clear (107, 74, 66); far and fog (189, 66, 16) orange; light
  (255, 128, 64) from 51 degrees. Lava (glowing since 2026-10-22).
- Painting: Altair Prime's cloud shapes in fire colours: plum-black ash
  clouds (14, 7, 14) with orange-lit streaks over an orange horizon (192,
  66, 16).
- Mood: a volcanic crater under an ash storm.
- Sun: none painted.
- Recipe: towering ash clouds, plum-black, their undersides lit orange by
  the lava (the under-light), orange streaks between them, mauve-brown
  zenith. Falling ash and lightning in the ash plume are optional.

### World 18: Command Centre (Level 1)

- Shots: `sheets/w18_retail.jpg`, `shots/w18_retail_up_f200.jpg`, the
  reported screenshot.
- Retail: clear (41, 107, 197) blue; far (148, 123, 66); fog (82, 82, 33);
  light (208, 160, 128) from 74 degrees, swinging by about a tenth every
  90 seconds. Grey water.
- Painting: photographed Earth cumulus on blue (`commandmountains`, both
  images), a band of desert mountains on blue (the backdrop itself), and a
  white sun sprite. The painted blue is greener and duller than the clear
  colour, so retail shows a seam when you look up
  (`sheets/w18_opengl_vs_vulkan.jpg`).
- Mood: a summer day on Earth; the training ground (the briefing: Alpha
  Centauri).
- Sun: the sprite is about 15 to 28 degrees up; the light is at 74.
- Recipe (the clouds earth has): fair-weather cumulus (cover 0.4,
  base 3,000, tops 7,000), white sunlit tops (240, 251, 254), flat
  blue-grey bases (66, 129, 193); a few high cirrus. Deep blue zenith to a
  paler horizon. White-yellow sun high at 74 degrees. Cloud shadows
  drifting over the grass and sand. Band: the desert mountains, cut out.
  The 90-second swing reads as a cloud crossing the sun.

### World 19: Alien Brain

- Shots: `sheets/w19_retail.jpg`, `sheets/w19_clouds_turn_9s.jpg`.
- Retail: clear (0, 0, 0); far (0, 0, 0); fog (66, 49, 49); light (224,
  176, 192) from 50 degrees. Red liquid. Only the wheeled modes work.
- Painting: two layers of pink-salmon (199, 106, 112) and dark teal cloud
  on black, like tissue or a nebula, turning at different speeds; a
  darkening band low down. Black above.
- Mood: alien, organic; the aliens' nerve centre.
- Sun: none painted.
- Recipe: a black void with two turning layers of glowing "tissue" cloud,
  rendered as an emissive volume: pink at the thin edges, teal in the
  dense cores, fibrous (ridged noise), with depth between the layers. Slow
  brighter pulses running along the fibres are optional. No sun disc. No
  cloud shadows (the light is the world's own).

### World 20: Oolbryte

- Shots: `sheets/w20_retail.jpg`, `shots/w20_retail_up_f160.jpg`.
- Retail: clear (74, 82, 99) slate; far (255, 255, 189) cream; fog (140,
  140, 107); light (255, 255, 182) from 47 degrees. No water.
- Painting: a dark grey storm deck, lenticular and stratocumulus (dark
  (66, 81, 95), edges (165, 167, 183)), breaking over a bright cream-yellow
  horizon, with iridescent pink-orange fringes where it thins.
- Mood: a storm clearing at dusk.
- Sun: the horizon glow runs all around; the light is at 47 degrees.
- Recipe: a broken deck (cover 0.7) of grey stratocumulus with a few lens
  clouds, a clear cream-yellow band at the horizon, pink-orange fringes on
  thin edges facing the light; slate zenith.

### World 21: Vernimab

- Shots: `sheets/w21_retail.jpg`, `shots/w21_retail_up_f160.jpg`.
- Retail: Exxodus's painting and colours; light (255, 128, 64) from 50
  degrees. No water.
- Recipe: world 4's, with its own light.

### World 22: Homeworld

- Shots: `sheets/w22_retail.jpg` (the intro's letterboxed camera).
- Retail: Altair Prime's painting and colours. It is the intro's world: a
  new game flies through it, so its sky is the first one the player sees.
  Toxic yellow-green water.
- Recipe: world 5's.

### World 23: Caspadril

- Shots: `sheets/w23_retail.jpg`, `shots/w23_retail_up_f160.jpg`.
- Retail: clear (66, 132, 132) teal; far and fog (173, 197, 197); light
  (218, 255, 252) from 62 degrees. The clearest teal water.
- Painting: small flat-bottomed fair-weather cumulus on teal (image A);
  snow-covered mountains (image B).
- Mood: a crisp, clear, cold day.
- Sun: none painted.
- Recipe: small cumulus (cover 0.3) with crisp white tops (210, 225, 224)
  and teal shade; teal zenith to a mint horizon. Sharp sun at 62 degrees.
  Band: the snowy mountains. Light snow optional.

### World 24: Caradion

- Shots: `sheets/w24_retail.jpg`, `shots/w24_retail_h2_fp.jpg`.
- Retail: clear (148, 66, 58) salmon; far (107, 33, 8); fog (82, 33, 0);
  light (240, 80, 18) from 63 degrees. Lava. Flying refused.
- Painting: big orange-pink sunlit cumulus (252, 202, 203) on salmon
  (image A); red mountains under more cloud (image B).
- Mood: a warm, dusty dusk over a meteor crater.
- Sun: none painted.
- Recipe: heavy cumulus (cover 0.5), orange-pink tops, salmon-brown bases
  (148, 65, 57); salmon zenith. Band: the red mountains.

## The plan

### The approach

Replace the hybrid with one sky made by the renderer, set world by world in
the tuning file.

1. **The base** is a colour gradient from the zenith (straight up) to the
   horizon. It is not a physical atmosphere: these skies are crimson,
   green, black. Its colours come from the world's painting and its H7
   colours. The horizon colour equals the far colour, so the haze over
   distant ground melts into the sky.
2. **The sun, moons and stars** sit in it. The sun stands exactly where the
   world's light comes from: one truth for the sky and the shadows.
3. **Volumetric clouds** sit over the base: clouds with depth, lit through
   their thickness. Each world sets their shape (cumulus, stratus, storm,
   ash), coverage, heights and colours, taken from its painting.
4. **The painting's silhouettes**, the mountains, mesas, ridges and
   Artoria's hill, stay as a band at the horizon. Only the land is cut out
   of each painting, so no painted cloud is left to meet a new one.
5. **Weather** (rain, lightning) and **effects** (glowing veins, lava
   under-light, turning tissue clouds) are added per world.

Every world starts on retail's painting (`"style": "painted"`) and switches
when its recipe is approved.

### The layers

- **The gradient.**
  - `zenith`, `horizon` and a curve between them, plus a glow around the
    sun. The glow uses Henyey-Greenstein, a formula for how much light
    scatters forward, already in `modern_sky`.
  - A second warm glow low on the horizon for the sunset paintings (1, 4,
    11, 20, 21).
  - Defaults: zenith from image A's top row (the clear colour), horizon
    from the far colour.
- **The sun.**
  - A disc and a glow at the light's direction, in the light's colour.
  - Its size and glare per world: a sharp disc with a starburst for the
    airless worlds (8, 14, 16); none where the light is not a sun (Artoria's
    clarifiers, the Alien Brain); a moon with a halo for Stendahl Alpha's
    moonlight.
  - Retail's painted suns and glows are dropped. None matched the light,
    and most repeated four times.
- **Moons, nebulae, stars.**
  - Sprites placed by direction and size: retail's Moon photo (Foldrak
    Noir, the Defence Satellite), world 1's Moon, Luna Azine's 3 by 3
    nebula as one additive texture.
  - Built 2026-10-23 as `sky.objects`, a range of H3 sprites from retail's
    own sky art to keep under the modern sky. All of it is additive (flag
    0x10): the star domes (`stars01`, `moonstar`, `moon2star`), the
    glows (`spacegradient`, `moongradient`, 1659), Luna Azine's nebula on
    the ring and the Brain's turning `cloud`. Kept faces go to the
    tracer's sky set; where a primary or reflected ray meets nothing, it
    sums their texels along the ray (`sky_objects`), as retail's blend
    added them, and the composite takes that traced sky as its base.
  - The Moon is not a face but a 1B billboard (sprite 1635 on 14, 1654 on
    16, a masked photo). The raster's sky draws lie under the modern
    sky, so a kept billboard's evaluated command is placed back in the
    world from its four screen corners and depths (`place_sky_billboard`)
    as a sky face; masked, it covers the stars behind it. The world hides
    it as retail did (the hill in front of Foldrak Noir's Moon).
  - First recipes: Luna Azine (stars, nebula and glow kept, its painted
    sun-burst 1557 left for the real sun), Foldrak Noir (stars, Moon and
    teal glow, the ridges as the band), the Defence Satellite (stars, Moon
    and green glow); each with a sharp sun where the light comes from.
    The Brain (19, 0) stays painted: its turning tissue clouds are
    retail's best sky, and the plan's emissive turning volume is the open
    piece.
  - Stars: retail's star tiles as a base layer, with sharp hashed stars on
    top, only where the sky is dark.
- **The band** (worlds 4, 11, 13, 14, 18, 21, 23, 24). Built 2026-10-23:
  - The recipe names the backdrop tiles that hold the land (`band.sprites`,
    first and last H3 ids). Under the modern sky those faces are traced as
    far ground, unlit and masked, where retail drew them: on the backdrop's
    ring, 16,000 to 27,500 units out, four times round. Every other face of
    the backdrop, and what hangs on it, goes to nobody.
  - Each tile's painted sky is cut away once, when its texture is first
    resolved (`d3d_texture::cut_band`): sky is any texel within `band.cut`
    of the top row's median colour. Each column's skyline is where two
    texels in a row turn, searching as `band.from` says: up from the bottom
    to the first sky (clouds painted above the land go too), or down from
    the top to the first land (Caspadril, whose snow strays near its sky's
    colour). A five-column median drops lone misread columns.
  - `band.bright` makes any texel at least that bright (luminance 0 to 1)
    sky as well, for glows painted in the sky: Exxodus's and Vernimab's
    (luminance 74 to 215 of 255) left white blocks above land that never
    passes 60 (`bright` 0.26).
  - As far ground the band takes the mist by its depth, as the sky does,
    and the clouds lie over it only where they are nearer than its ring.
    Reflections and bounces see it.
  - First recipes: Command Centre (1683 to 1687), Caspadril (1716 to 1720,
    from the top at 0.08) and Caradion (1728 to 1732).
  - Command Centre's land was never seen in retail: its cumulus images
    hung in front of it. Now its desert mountains stand at the horizon.
- **Clouds.** A ray march (stepping along each view ray through the cloud
  layer, adding up density and light):
  - The layer is a slab between `base` and `top` world heights. It wraps
    every 65,536 units like the ground, so it is seamless across the
    world's edge.
  - Density: a 2D weather map for coverage, times a height profile for the
    shape, eroded by two tiling 3D noise textures. The textures are a
    Perlin-Worley base at 128³ and a Worley detail at 32³, made once at
    startup by a compute shader. This is the standard method of Horizon Zero
    Dawn's sky ("Nubis").
    - The weather map rises gently and never past 0.7, so the 3D noise
      always carves the clouds. At full coverage they were the map's 2D
      blobs drawn upward: pancakes, or with smaller cells, blocky towers
      (2026-10-23, `.tmp/plan-sky/shots/w02_e2c_*`).
    - Opaque within a few hundred units (`CLOUD_SIGMA` 0.01), so edges are
      sharp. Thinner clouds read as grey smudge.
    - A broader field, at half the weather map's frequency, gathers them
      into groups with clear sky between. Without it they stood in an even
      lattice of like puffs (Caspadril, 2026-10-23).
    - Rounded, not rectangular (review, 2026-10-23: the skies
      looked a bit rectangular). Each shape's envelope fades steadily up the
      layer instead of holding, and the footprint's edge rises toward the
      top (`cloud_shape`), so sides lean in to domed tops; the weather map
      leans with height (wind shear), each cloud's base sits a little
      higher or lower than the next, and the edges erode harder. A held
      envelope had drawn every 2D footprint straight up: walls and a flat
      top.
    - Per world: `clouds.haze`, how far off they fade into the horizon's
      haze (36,000 by default), and `clouds.albedo`, how much light inside
      them scatters on (1 for water clouds; low for smog, whose bodies stay
      dark while their edges catch the sun).
    - The modern sky takes the same share of the world's mist as the
      painted one (`fog.sky`); the whole of it veiled dark clouds in the
      fog's colour.
    - Altair Prime's recipe after a 4/10 review (way darker clouds, like
      smog, and very tall): storm towers to 14,000, plum bodies
      (albedo 0.25) with amber edges, a sulphur horizon, haze 120,000.
    - `clouds.round`, per world (a second review, the same day):
      how far the clouds round off, 1 today's domes, 0 the fuller first
      shapes, whose envelope holds to mid-layer and whose footprint stands
      straight. On Command Centre the domes read as flat saucers seen from
      below, where the first full-bodied puffs were preferred; the
      worlds that were liked at 1 keep it.
    - The storm's veins went back to the first version, as chosen:
      thin sheets where the Worley noise crosses its middle, a web of light
      across the deck. Tabuchi takes the whole of the mist over its sky
      (`fog.sky` 1), as its first sheets did.
    - Cover per world went up where the paintings are cloudy ("many other
      worlds would benefit from more cloud density"): Caradion to 0.85
      with bigger clouds, Oolbryte 0.85, Stendahl Alpha, Helniazine,
      Kankutan and Artoria about 0.72 to 0.75, Command Centre, Terra Nova,
      Shalapreth and Caspadril about 0.55.
  - Shapes: cumulus (round tops, flat bases), stratus (thin sheets),
    storm (tall towers with anvils), ash (low, thick, lit from below).
  - Light:
    - Beer-Lambert: light fades exponentially through density. A few steps
      towards the sun give each sample its sunlight.
    - Light scattered many times inside: Hillaire's octaves (2016), each
      with half the share, half the depth's hold and half the phase's lean.
      Single scattering alone left thick clouds grey; with the octaves the
      tops are white and the bases grey.
    - A "powder" term darkens the edges facing the sun, as real cumulus
      shows.
    - Two scattering lobes give the silver lining near the sun.
    - Ambient: the zenith colour from above, a ground colour from below.
  - Per-world extras:
    - An emissive vein channel: Tabuchi's green glow. Built 2026-10-23
      (`clouds.veins`, its colour): thin sheets where two octaves of the
      billowing noise cross their middle, flickering slowly as a broader
      noise drifts through. Worley's cells read as a honeycomb.
    - An under-light in the lava's colour: worlds 7 and 17. Built
      2026-10-23 (`clouds.under`): the undersides take it, fading up
      through the layer; Artoria's clarifiers use it too.
    - A self-lit volume: the Alien Brain's tissue clouds, in two layers
      turning as retail's do.
  - Optional per world: a second thin layer high up for cirrus.
  - Wind-torn clouds (`clouds.streak`, built 2026-10-23): the weather map
    stretched that many times across X, for Exxodus's and Vernimab's coral
    streaks and Kankutan's and Helniazine's stratus.
  - The clouds pass of 2026-10-29 (Vulkan only, both renderers;
    `pt_sky.glsl`, `pt_sky_clouds.comp`, `cloud_wake.rs`):
    - **The Kankutan jump** (world 15): the eye wraps at x=0, and with
      `streak` 3 each noise field had 10/3 tiles across X, so the whole sky
      swapped in one frame. Each field now takes whole tiles across X
      (`cloud_tiles`), and the view's history folds its offset into the
      wrap. Wrap step 30.1 to 1.6 traced, 37.1 to 2.2 raster. The streaked
      worlds' (4, 6, 13, 15, 21) stretch is rounded to whole tiles.
    - **Heights** (`clouds.rise`, 0.12 by default, 0.04 to 0.08 on the tall
      storm and ash decks): each cloud's base and top stand up to that
      share of the layer's thickness above or below it, by a field at the
      weather map's scale; the march, shadow map and `cloud_light` take the
      layer widened by the rise.
    - **Flying through one**: the near-eye clear zone went from 800 to 50
      units (a whiteout inside a cloud, not a clear bubble); near the eye
      the march steps start at 12 units and grow by a twelfth of the
      distance; the history takes half of each frame where the clouds lie
      within 1,000 to 3,000 units, so wisps stream past. A near band, the
      clouds between the eye and the craft the camera follows, takes at
      most 0.5 of optical depth, so the craft stays at least 60% in sight;
      none in first person or on the Cranial Implanter. The followed craft
      leaves a wake (a point each 0.25 s for 3 s) that carves a tube
      through the cloud, closing as it ages. Clouds are laid after the far
      fade, so they no longer end in a straight seam at the far skyline.
  - Ash and embers (`precip.kind` ash and embers, built 2026-10-23): the
    snow's walk; ash grey and lit, embers glowing in their own colour,
    each flickering on its own beat, rising where `fall` is negative.
- **Weather.** All of it runs on the game's animation clock and a hash.
  Nothing uses the game's random numbers, so replays and fixed-step runs
  stay exact.
  - **Lightning** (Tabuchi, maybe 5, 17, 22). A flash schedule (rate,
    duration, flicker). Built 2026-10-23, first pass (`sky.lightning`:
    rate, colour, light): the game's clock runs in slots of the mean
    spacing, each slot's hash saying whether, when and where it flashes;
    a flash is a stroke and a weaker return, gone in under half a second.
    It glows in the deck round a point in its direction and brightens the
    world in the composite; the cloud history takes the flash quickly. Not
    yet: bolts, and a directional light with its shadow ray. Each flash:
    - lights the cloud volume from a point inside it;
    - draws a branching bolt in the sky for a few frames, sometimes;
    - adds a short directional light from the flash to the traced world
      (one extra shadow ray, during flashes only).
  - **Rain** (Tabuchi).
    - Three depth layers of falling streaks drawn in the composite at the
      display's resolution. Each layer is hidden behind nearer geometry
      (the depth image), slanted by the wind, coloured by the world's light
      times the rain's tint (Tabuchi's blue-green), and brighter in flashes.
    - The traced water gets rain rings: fine ripples in its normal while it
      rains. Built 2026-10-23 (`rained` in `pt_trace.comp`): in 48-unit
      cells fixed on the surface, as many as the rain is dense, one ring at
      a time spreads from a hashed spot, a new spot each time.
    - The streaks are the snow's walk with a drop stretched along its fall
      (`sky.precip.kind` rain).
    - Wet ground (darker, glossier) is a later option.
  - Snow, ash and embers are the same streak layers with other speeds and
    sizes (questions below).
  - Built 2026-10-23, snow first (`pt_weather.glsl`, the tuning's
    `sky.precip`): flakes and drops hang in cells fixed in the world, one at
    a hashed spot in the middle of each kept cell, the field falling and
    drifting on the game's clock. Each view ray walks the cells it crosses in
    four shells about the eye (200 to 3,200 units, cells a tenth of their
    distance); a cell belongs to a shell by its middle's distance, so no
    flake is cut by a cell wall, and it fades at the shell's edges as the
    eye moves. Drawn by the composite at the display's size, after the
    upscaler, over the mist; nothing behind nearer geometry. A drop is the
    same, stretched along its fall. 0.65 ms a frame at 4K.
- **Light that changes.**
  - The feed passes the current tint divided by the world's first style
    (`0x37`), per channel. The sky's colours, cloud lighting and glows
    scale by it, with a floor so dark is not pure black. Stars and
    emissive veins do not scale. Built 2026-10-23: the world's
    presentation keeps H6 `0x37` beside the tint (`base_tint_word`), so a
    save loaded after Artoria's generators died still falls from gold; the
    floor is 0.08 a channel.
  - Artoria's clarifier glow follows it to near black in one second, with
    a flicker on the way.
  - Kankutan's sky breathes every 90 seconds, Command Centre's dims a
    little.
  - World 1 also needs the schedule's position (which style, how far into
    it) to blend its day, sunset and night recipes. Only it changes hue.

### Where the sky is seen

- **The view.**
  - A new compute pass marches the clouds at a quarter of the display's
    width and height (960 by 540 at 4K). Each pixel marches from a jittered
    start and blends a sixth of the result into its history: temporal
    accumulation, reusing last frame's work. The history is found where the
    last frame saw the clouds' own point, so a moving camera does not
    smear them.
  - The composite then draws the whole sky into every sky pixel (depth 1)
    at the display's resolution: gradient, sun, moons, stars, band, the
    upscaled clouds, bolts and rain. It draws them under the mist and the
    warpgates' fields, as the modern branch already does.
  - The upscalers (DLSS, FSR, XeSS) never see the drifting clouds, so they
    cannot smear them.
  - **Flying through.** The VTOL can reach a cloud wherever the layer is
    low or the ground high, and it should pass through nicely
    rather than the layer being raised out of reach. The pass also keeps
    how far along each ray the clouds' light comes from. The composite
    lays the clouds over any geometry farther than that: a peak in the
    layer, or the world below from inside a cloud. Within about 800 units
    of the eye the view's clouds thin, so inside one the craft stays in
    sight in a mist, with the cloud's mass around it (`.tmp/plan-sky/shots/
    w02_low3_fly*`). Shadows and light keep the whole cloud. Points inside
    the layer take the shadow of the part above them.
  - The trace still writes a cheap sky colour for rays that miss, so the
    edges the upscaler blends against geometry match.
- **Reflections and glossy bounces.**
  - A small sky cube map (six 128-pixel faces covering every direction,
    cheap to look up) is rendered each frame with a short march. It
    includes the band, sun and objects.
  - The water's reflections, refraction from below, metal's glossy bounce
    and the spray drops sample it.
  - This replaces `painted_sky()`'s extra ray query against the painting.
- **Diffuse bounce light** keeps retail's ambient (the tint shifted right
  by two). That calibration keeps each world's brightness. The sky does not
  change it, except through the tint ratio above.
- **Cloud shadows.**
  - A 512 by 512 shadow map of the clouds' transmittance along the sun is
    rendered from the same volume each frame, covering the area around the
    eye.
  - `cloud_light()` samples it at its existing call sites: the ground,
    models, the water's glints and caustics, primary hits and bounces.
  - The shadows match the clouds you see, which today's separate 2D noise
    cannot guarantee.
- **Mist and haze.** The horizon colour equals the far colour (H7 `0x73`),
  which the haze turns to, so distant ground meets the sky without a line.
  The mist lies over the sky as the modern branch does now.
- **Thermal vision.** The sky stays one cold value (`thermal.sky`). Clouds
  read slightly warmer, at low contrast. No rain, no bolts, no flashes.
- **Other cameras.** The briefing's traced pane, world 22's intro and the
  ending draw the world, so they get the sky. The OpenGL and software
  renderers keep retail's painting.

### The tuning file

`sky` keeps its per-world merge (`worlds.N.sky` over `sky`, objects merging
key by key, arrays replacing). The flat cloud keys become blocks. A sketch:

```json
"sky": {
  "style": "painted",
  "zenith": null, "horizon": null, "curve": 3.0, "horizon_glow": 0.0,
  "sun": {"show": "disc", "radius": 1.0, "glow": 0.6, "glare": 0.0},
  "stars": 0.0,
  "clouds": {"shape": "cumulus", "cover": 0.0, "base": 4000.0, "top": 9000.0,
             "size": 4096.0, "speed": 2.0, "lit": null, "shade": null,
             "under": null, "veins": null, "shadow": 0.5},
  "high": {"cover": 0.0, "height": 14000.0, "color": null},
  "band": null,
  "objects": [],
  "lightning": {"rate": 0.0, "color": [1.0, 1.0, 1.0], "light": 1.0, "bolts": 0.3},
  "rain": {"density": 0.0, "color": [0.7, 0.75, 0.8], "speed": 1.0, "wind": [0.0, 0.0]},
  "follow_light": true
},
"worlds": {
  "9": {"sky": {
    "style": "modern",
    "clouds": {"shape": "storm", "cover": 0.8, "base": 3000.0, "top": 9000.0,
               "lit": [0.55, 0.52, 0.6], "shade": [0.22, 0.18, 0.22],
               "veins": [0.38, 0.85, 0.42]},
    "lightning": {"rate": 0.25, "color": [0.75, 1.0, 0.85], "light": 1.5},
    "rain": {"density": 0.6, "color": [0.45, 0.85, 0.75], "wind": [0.3, 0.0]}
  }},
  "18": {"sky": {
    "style": "modern",
    "clouds": {"shape": "cumulus", "cover": 0.4, "base": 3000.0, "top": 7000.0,
               "lit": [0.94, 0.98, 1.0], "shade": [0.26, 0.5, 0.76]},
    "band": {"from": "backdrop", "cut": "skyline"}
  }}
}
```

- `null` colours take the world's own (the H7 colours; the painting's
  measured bright and dark tones for `lit` and `shade`).
- `band.from` names which painted band holds the land: `lower`, `upper`,
  `both`, or the backdrop's own band where retail moved the clouds
  elsewhere (world 18).
- `objects` is a list of `{"sprite", "azimuth", "elevation", "size",
  "blend"}`.
- Old override files with the flat keys stop reading. That matches the
  port's rule of no legacy paths. The file says why it failed and keeps the
  last good values, as it does now.
- The file reloads within a second, so recipes can be tuned while playing.

### What changes where

- `shaders/pt_sky.glsl` (new): the gradient, sun, moons, stars, band,
  cloud density and lighting, weather timing; shared by the passes.
- `shaders/pt_sky_clouds.comp` (new): the cloud march with its history,
  the reflection cube map and the cloud shadow map.
- `shaders/pt_composite.frag`: the full sky in sky pixels, rain over all
  pixels, bolts. The hybrid's branch goes.
- `shaders/pt_trace.comp`:
  - `sky_along` becomes a cube map lookup; `painted_sky()` goes;
  - `cloud_light()` samples the shadow map;
  - the lightning light joins the direct light;
  - the water takes rain rings.
- `shaders/pt_frame.glsl`: the frame block's sky fields. `sky_top`, the
  flat cloud fields, `clouds_along`, `cloud_cover_at` and `cloud_noise`
  go.
- `src/vk_device/sky.rs` (new, as `thermal.rs` is): the passes' pipelines,
  the noise textures, the weather map, the cloud images, the cube map and
  the shadow map, and their GPU timers.
- `src/vk_device/trace.rs`:
  - the frame block;
  - the sky bottom level, its instance and `MASK_SKY` go;
  - the feed no longer splits off sky triangles.
- `src/scene_capture.rs`: the backdrop and every model it instances
  (found through its instance tree, not a list of ids) leave the tracer. The
  capture keeps only their textures and geometry for the band and objects.
  This stops `commandmountains`, `mist`, `cloud` and the star domes from
  being traced as distant emissive walls.
- `src/scene_feed.rs`: `SceneSurface::Sky` goes; a sky record carries the
  band's texture slots and elevations, the H7 colours, the world, and the
  tint ratio and schedule position.
- `src/world_tint.rs`: expose the schedule's position, read only.
- `src/modern_tuning.rs`, `modern_tuning.json`: the new `SkyTuning`, with
  tests for the per-world merge and for unknown keys.
- The raster keeps drawing retail's backdrop, since the game's draw
  pipeline must run every frame. The composite covers every sky pixel.
- Docs: `PIXEL_RUNTIME.md` (Modern mode, Sky) and
  `INFESTATION-RT-IMPLEMENTATION.md` (Sky, and the Decisions line about the
  hybrid).
- Tests: the skyline cut on a synthetic band; the capture leaving the
  backdrop's instanced models out; the tuning's recipes reading.

### Alternatives considered

- **Fix the hybrid's seam** by matching the added clouds to each painting
  and blending across the edge. Rejected. And the painting
  stays static, repeats every quarter turn and has no top, so two styles
  still meet somewhere.
- **Upscale the paintings into a full painted sky box** (painting the
  missing top). It keeps the mood exactly. But it is still static: no
  drift, no weather, no matching cloud shadows, suns in the wrong place. It
  would make a decent fallback for slow cards.
- **A physical atmosphere** (Rayleigh and Mie scattering), the plan of
  2026-10-20. It suits the Earth-like worlds (2, 10, 18, 23). But it
  fights skies that are crimson, green or black. The artistic gradient
  reaches each painting's colours directly. A physical model can come back
  later for the Earth-like worlds.
- **2.5D cloud layers** (flat noise with fake thickness and normals).
  Cheap, but they turn to streaks near the horizon, the flat layer's known
  failure. Kept as the low-quality setting.
- **Clouds as geometry in the traced scene.** Volumes are better marched in
  their own pass. The tracer's rays would not gain.
- **Pre-rendered sky cube maps per world.** Cheapest at run time, but
  frozen: no drift, weather or changing light.

### Cost

Estimates for the RTX 5080 at 4K. The hybrid's 2D clouds cost about 0.5 ms
in the composite today: 0.94 against 0.20 ms in Command Centre, 1.16
against 0.41 in Tabuchi. Those were measured with other GPU work running, so
treat them as rough.

| Part | Where | Estimate |
|---|---|---|
| Gradient, sun, moons, stars, band | composite, sky pixels | 0.1 to 0.2 ms |
| Cloud march, a quarter of the pixels each frame | own pass | 0.3 to 0.6 ms |
| Reflection cube map, 6 by 128² | own pass | about 0.1 ms |
| Cloud shadow map, 512² | own pass | under 0.05 ms |
| Rain streaks | composite (rainy worlds only) | 0.1 to 0.2 ms |
| Lightning light | trace, during flashes only | about 0.3 ms then |

- Total: about 1 ms in most worlds, 1.5 ms in Tabuchi.
- Memory: about 9 MB of noise textures, under 4 MB for the rest.
- A card with a quarter of the 5080's throughput (RTX 3060 class) would
  pay about 4 ms. So the sky gets a quality setting: low (the 2.5D layers,
  about 0.3 ms), high (the march).

### Risks

- **Plainer than the paintings, again** (the 2026-10-22 review). Mitigation:
  - colours measured from each painting;
  - the painted silhouettes kept;
  - real cloud depth, strong rim light and dark bases;
  - a recipe per world;
  - `"style": "painted"` stays available per world.
- **The band's cut.** The automatic skyline may fail on pale skies over
  snow (23) or on cloud behind mountains (4, 21, 24). Per-world thresholds,
  and checking each band at load in a test.
- **Ghosting.** The game's camera swings fast. The cloud history is
  reprojected with the camera's turn only (the sky is at infinity) and
  rejected where it changes too much.
- **Banding** in dark skies. Blue-noise dithering in the composite.
- **Readability.** Rain and flashes must not hide enemies or the HUD.
  Flashes only add light. Storm worlds keep retail's light calibration.
- **Flashes.** Lightning brightness and rate need a limit, with a setting,
  for people sensitive to flashing light.
- **The VTOL inside or above a cloud layer** must look right: the march
  starts inside, and fog thickens in the cloud.
- **The world's wrap.** The weather map and noise tile at 65,536 units, so
  crossing the edge changes nothing.

### Verification

- **Snapshot sheets per world** with this survey's tools, beside the
  retail sheets:
  - `python sweep.py --tag sky2 --tuning '{"worlds": {...}}'`, then
    `python sheets.py --tag sky2`;
  - the same four headings, first person, and the VTOL looking up;
  - no line may show at any elevation in the look-up shots.
- **Motion strips:** a snapshot every 30 frames for ten, to check drift,
  the lightning's rhythm and the rain.
- **Special runs:**
  - Artoria before and after its generators (the Cheats Menu run of this
    survey, `runs/w11_dark/`);
  - Kankutan across one 90-second swing;
  - world 1 at day, sunset and night. That needs a start field for the
    tint timer (mission `+2C`), which the harness lacks today.
- **Other checks:** thermal vision; the water reflections in worlds 3, 9,
  12, 15, 18 and 23; the briefing pane and world 22's intro; the VTOL
  through a cloud layer; a drive across the world's edge.
- **GPU time:** the tracer's closing line and the new passes' timers, per
  world, at the default settings (DLSS on).
- **Sign-off:** each world's sheet is reviewed. Its `style` turns to
  `modern` in the shipped tuning file only after that.

### Order of work

1. **The base.**
   - The sky pass and its tuning.
   - The capture leaving the backdrop's models out.
   - The cube map and the shadow map.
   - Remove the hybrid.

   Every world stays `painted`.
2. **The Earth-like cumulus worlds:** 18, then 2, 10, 23, 12 and 24. The
   first example.
3. **Tabuchi:** the storm, the green veins, lightning, rain.
4. **Artoria:** the clarifier overcast, the band, the darkness.
5. **Dusk and storm worlds:** 3, 6, 15, 20, 5 and 22, 17, 7, then 4 and
   21, 13.
6. **Space and the Brain:** 8, 14, 16, then 19 and 0.
7. **World 1**, if wanted.

## Stage 2 built

Built 2026-10-25: thunder after each flash; wet ground with the rain, a
near strike's bolt and the flash's light from its side; Artoria after dark
leaning dark, its haze falling with its light; the homeworld in Foldrak
Noir's sky and beside the Defence Satellite, a render of the sky's own;
world 1's recipe. Every approved recipe is on; a world returns to its painting with
`"style": "painted"`.

1. **Sunsets.** Worlds 1, 4, 11, 20 and 21 paint a sunset, but their light
   comes from high above. Keep the sun high and paint the warm horizon
   anyway (the default)? Or lower the light for long sunset shadows? That
   changes how the whole world is shaded.
2. **More weather.** Lightning beyond Tabuchi: Altair Prime and Homeworld's
   purple storms, Shak'tahh's ash cloud? Ash and embers in 7 and 17? Light
   snow in 10 and 23?
3. **Thunder.** Should lightning have a sound? Retail has none for
   weather, so it would need a sample.
4. **Rain's reach.** Streaks and water rings only, or also wet, glossy
   ground?
5. **The homeworld.** Show the "enormous artificial alien homeworld" in
   Foldrak Noir's sky, and a planet beside the Defence Satellite?
6. **Artoria after dark.** Fully dark, or a faint glow (fires, emergency
   lights)? Should the clarifiers show, say as light columns into the
   clouds that switch off?
7. **The paintings.** Keep a Painted/Modern sky choice in the settings, or
   modern only once each world is approved?
8. **Flashes.** How bright and how frequent may lightning be?
9. **World 1** looks like an unused prototype level. Give it a recipe at
   all?
