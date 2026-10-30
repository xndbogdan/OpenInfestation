# Pelagia: the prototype world and the warpgate that wakes

Status (2026-10-24): the switch and design steps 1 and 2 built: gate 81
wakes for a scientist and leads to world 1 by the Teleport's load, named
"Pelagia" by the gate, the free look and the Teleport list
([`pelagia.rs`](../../crates/inf-game/src/pelagia.rs)). Steps 3, 4 and 5
built since: gate 73 leads back to the Command Centre's first start with
no scientist asked for, the port's H0 table names Pelagia in all six
languages, and the port's own 24-record briefing program plays with its
texts and the Mission Summary's one objective. Factory 2 stocks the
hovercraft and factory 17 the headlights, with 2 red and 2 blue crystals
by them. The egg's reward is born: 15 red and 15 blue crystals across the
islands, some on the outer islets, with persistent pickup serials. Still
to come: the five ships' model 665, and the map survey.

Since (2026-10-25, after a review): the briefing's lines stay up long
enough to read, its story's holes are closed, it has two objectives and a
second version for the rework off, and its list hands over to Start as
soon as retail's would; the ships stand solid; and a token trail runs from
the lagoon's south shore down onto its bed, glowing up through the water.
Later the same day the layout was redone after retail's own maps: a trail
from the gate, a sandbar to a crater isle and the egg's cache inside its
rim ([below](#the-layout-redone-2026-10-25)).

The request (2026-10-23): at the start of the first world (Command Centre) stands a
warpgate that does not work; bring a scientist to it and it powers up and
leads to the prototype world (world 1). The prototype's own warpgate, which
leads to Terra Nova, leads to the Command Centre instead. World 1 gets a
briefing: an old water world that humans found and settled, no concern of
the player's, "we're not even sure how you got here", the only objective to
get back to the Command Centre. As a little egg of its own, the hovercraft
early, with the crystals it costs. Every text in all six languages. The
user is free to fix the map's scenery, items and landscape ("it's
essentially an unfinished map") and to name the planet.

## What is there now

Read from the data with `inf-game --mode resident --inspect-world` (leaves
11 and 28), `inf-extract words` (the H1 starts), `inf-extract strings` and
`Infest.exe`'s tables at the addresses given.

### Command Centre's dead gate

- Command Centre (world 18, leaf 28) starts at (32256, 41216), H1 slot 0 of
  `28.ovl` (`0xA1007E00`). Retail's gate 81 stands at (32132, 41889),
  within 1000 of it, so the postinitializer `0048C4F0` hands it the player:
  the ACAM starts 450 in front of it (the camera's focus (32132, 361, 41439)
  in [GAMEPLAY_RUNTIME](../re/GAMEPLAY_RUNTIME.md)). The
  Command Centre keeps its retail positions and facings: the port never
  moves its gates, but gate 81 is still marked so a scientist wakes it.
  World 1 has only the one gate (73), so there is no second Pelagia gate
  to trade places with; the 2026-10-25 swap of gate 81 with gate 2 is
  reverted.
- Gate 81's only command is 24: `+164 = 1`, the hoop lock
  ([the gate](../re/SELECTOR30_RUNTIME.md#update)). While
  `+164` is positive the update `0048B3F0` skips its scan and goes to the
  sound, so the gate never names a destination, never lights and never
  passes anything. Message 0x15 (`0048C470`) counts the lock down, as
  Artoria's hoops count theirs, but Level 1's 13 hoops are linked to
  nothing ([ACTOR_RUNTIME](../re/ACTOR_RUNTIME.md)). It has
  no destination either: `+15C = 0`, no link `+180`. Most likely a hoop
  once powered it.
- The second start, (11763, 35862), is by gate 2, the way to Terra Nova
  (commands 22, 23 and 26: world 2, start 1, `+167 = 1`).
- Level 1's four scientists stand at the main base, about (18600 to 20200,
  34500 to 35100), some 13,000 from the start, each with `+7A = 0x25`
  (Standard Grenades). Its factories give Turbo (3), the Truck (2) and
  Standard Grenades (0x25). Bringing a scientist to gate 81 means driving
  one back from the base.

### World 1 as it is

- One start, (37536, 28000) (H1 slot 0 of `11.ovl`), with no gate within
  1000: the ACAM appears in the middle of the colony, not at a gate.
- Its H14 header: loss limit `+24 = 0` (the world never ends in Mission
  Failed), briefing records `+28 = 0` (no briefing program).
- Its strings, `11l0.ovl` to `11l5.ovl`: slot 874, the briefing's title,
  reads "Command Centre" (copied from Level 1); 875 to 899 are empty, so no
  briefing lines and no Mission Summary (896 on). Its name slot, H0 `0xE1`,
  holds "Completed", the results page's label for the campaign's end; it
  must keep that.
- Its 74 placements:
  - gate 73 at (28000, 19000): command 16 (one staff place, which sets its
    team `+B4` to 0), 22 and 23 (world 2, start 1), 26 (`+167 = 1`,
    Mission Complete). Hint trigger 44 stands on it (radius 1000, 5 s) with
    hint 9: "Drop a scientist next to the warpgate so that it can be
    activated for your craft";
  - factory 2 at (36736, 28400), beside the start, holding nothing of its
    own; factory 17 at (26880, 25856), holding Standard Grenades (0x25);
  - six scientists, five of them with `+7A` 3 (Turbo) or 2 (the Truck);
  - five perships (selector 34, placements 22 to 26) around the landing
    site, model 665, which only levels 25 and 28 register: unseen, their
    contacts skipped (item D);
  - a Nemian walker (10), a dormant quixle (19, `+C4 = 1`), two save pads
    (56, 57), one terrain light (21);
  - 17 hint triggers with Level 1's tutorial hints 0, 1, 3, 4, 5, 8, 9 and
    21 (scientists, factories, the interface pad, "You need more power to
    make that jump", crystals, the gate, the token trails);
  - 23 tokens (pickup kind 7) and 15 green crystals (kind 15); no blue or
    red ones.

  World 1 reads as an early draft of Level 1: scientists tied to the
  truck and the turbo, green crystals to pay for the turbo (5 green), and
  a scientist to wake the gate. Its land is a colony: huts, fences, palms,
  containers, sand tracks and a hill (`.tmp/plan-sky/sheets/w01_retail.jpg`).

### What the factories offer ("power plants")

The upgrades come from the factories, the buildings with the interface pad.
World 1's two are above: the one beside the start holds nothing of its own,
the other Standard Grenades. Both list too every product whose blueprint
the player has brought ([the shop](../re/FAMILY40D800_RUNTIME.md#the-shop-menu-00431000)).
The scenery that looks like power plants sells nothing: studied by a
scientist, a Power Coupling (furniture kind 23, `00532F88`) teaches Stun
grenades, a Power Transformer (kind 22) only adds its database entry
([the research tree](../re/HIDDEN_GOODIES.md#scientists-scans-and-the-research-tree)).

### The hovercraft's price

Hovercraft mode (0xA7) is the first of the 18 products every factory
carries (`005320D8`). Its record's costs are 3 green and 1 red crystal
(bytes `+F = 3`, `+10 = 1`; `0046C6D0` pays tokens, blue, green and red
from `+D..+10`). A factory lists it once its grant mask owns 0xA7 (its own
`+7A`, or the player's blueprints merged on connecting) and the player does
not own it yet. A red crystal (pickup kind 16) adds 2 to the red tank
(grants 0xE to 0x10 add twice their amount, up to 30); Terra Nova places
two. Headlights (0x14, the same list's sixth) cost 1 blue and 1 green
(`+E = 1`, `+F = 1`); a blue crystal is kind 14. Besides its own `+7A`, a
factory takes products by its commands 23 to 25, each granting a WORD ID
into its mask `+198` (`0042F100`).

### Two traps

- A gate's world end (`0048BE70`'s last step, `00487560`) into world 1 is
  the campaign's end: the results page takes selection 1 as the final
  world and returns to the frontend, and from then the profile cannot be
  loaded (`0044A9F0` tests its record's selection for 1); the port's
  completion bonus would count it as a finished campaign
  ([the completion bonus](../re/PROFILE_RUNTIME.md#the-completion-bonus)).
  The way in must take the Teleport's load (phase 3, `teleport` in
  `session_platform/cheats.rs`), never `00487560`. The way back is an
  ordinary world end.
- "Warp to %s" (H0 `0x15F`) names a world by H0 `0xE0 + world`, so a gate
  to world 1 reads "Warp to Completed"; the briefing's header (`0040A420`,
  selection + `0xE0`) the same.

## The design

1. **Gate 81 wakes for a scientist.** A scientist the player drops (D)
   near gate 81 walks to it and powers it. The port sends the gate
   retail's 0x15, which counts its lock from 1 to 0 as a passed hoop does
   on Artoria, and gives it world 1, start 1 as its destination. From then
   retail's own update scans, names the destination ("Warp to Pelagia"),
   blends its colours and hums.
   - Retail's `+CC` (`0048BE30`) answers only a scientist of another team,
     and retail's scientist is not thought (2026-10-23) to walk to
     gate 81: the port makes gate 81 answer, gate 81 alone.
   - The scientist reports on the player's message line (`004135D0`, as
     the gate's own refusal does), "Commander, I have successfully
     repaired this warpgate" in the six languages below, then walks back
     and boards the craft, as a scientist that walks into it does. With
     the hold full it waits by the gate. Its arrival lights its bulb: the
     port sets its +29D, as for a scientist carrying an unowned grant, so
     the model's selector-9 branch shows it has something to tell (world
     18 places no selector 88, so the loader's flag that branch reads
     stays set).
   - At 0, `0048C470` also sets the first player's silent command 9: find
     what that does before relying on it.
   - Nothing differs until a scientist is dropped near gate 81: Level 1's
     recorded replays (Infest01) stay bit for bit the same with it on.
2. **Gate 81 leads to world 1** by the Teleport's load: no results page,
   nothing saved; the vehicle keeps its forms, its crystals and its hold.
   World 1's briefing shows first. Command Centre reloads as authored on
   the way back, gate 81 dark again until another scientist.
   - The trip never marks the Command Centre done (2026-10-23),
     unless it already was. No world end runs, so neither
     bit 1 (left by its gate, `0048E820`) nor bit 2 (completed) is set,
     and the loader (`0048DC30`, `begin_world_load`) marks the world it
     leaves only when bit 2 is already set (then 4, "completed and left").
3. **Gate 73 leads to the Command Centre's first start**: `+15C = 18`,
   `+160 = 1`. The ACAM comes out 450 in front of gate 81, the gate it
   left by. It needs no scientist (2026-10-23): if command 16's
   staff place or neutral team holds it, the port lifts that, and hint
   trigger 44, whose hint 9 asks for a scientist, goes.
   - With the rework the gates trade places (2026-10-25): gate
     73 stands on world 1's dead gate's site, (50944, 26880) on the east
     island, which the start's land reaches, keeping its facing, and the
     dead gate (furniture `gatedead`, lookup 97, H8 286, the gate's own
     death model) stands where gate 73 stood, on the even cell at
     (27904, 18944), turned 0x60. The ACAM comes out of the working gate:
     a start of the port's own (`WorldStartState::port_spawn`, which the
     load's clear leaves, so a lost ACAM comes back there too) at
     (50600, 27169), 450 behind it as a gate start puts it, facing away
     (310 degrees). Line 882 now places the headlights' factory on the
     west island rather than "nearer the warpgate".
   - `+167` stays 1: "Mission Complete", world 1's status bit 2, and the
     results page saves the profile at the Command Centre as any world end
     does. Only the results page reads a world's status (the medal, and
     "World already completed" without a score once bit 4 is set,
     [the save's `+3C`](../re/SAVE_FORMAT.md)), so a second
     visit to Pelagia scores nothing, as any completed world.
4. **The name: Pelagia**, from the Greek for the open sea; the same word in
   all six languages. It shows wherever world 1 is named: "Warp to %s",
   the briefing's title (874) and header, the results page and the
   Teleport list (today "Unused prototype"). H0 `0xE1` keeps "Completed".
5. **The briefing**: world 1 never shipped one. Every language's
   `11lN.ovl` holds only its title (874, Command Centre's) and 25 empty
   slots, and its H14 has no records. The port writes a record program of
   its own, in Command Centre's style (`28.ovl`'s 50 records), whose
   camera follows the lines: the colony at the start (875 to 879), gate 73
   (880, model 62), factory 2 (881, model 157), factory 17 (882), the
   colony again (883). Its lines fill slots 875 to 883, and 896 to 899
   the Mission Summary's objectives. The story borrows retail's own
   threads: the outer colonies and the early warp routes (Artoria,
   Caspadril), "no strategic advantage" (Artoria), "we are not sure why"
   (Exxodus). No retail briefing names a year, so Pelagia's age is told
   as retail tells Caspadril's. Retail's data files stay as
   they are; the texts live in the code, as `port_text.rs` keeps the
   port's own.
   - The top-down map is retail's own: `11.ovl` registers its H3 `0x5B7`
     (254 by 254, a sea with one cluster of islands and a few islets),
     which the briefing's background (`004737A0`) and map pane
     (`0040C390`) draw for every world. The program opens the map pane
     (kind 3) and sets its scale (kind 10) as Command Centre's does.
   - So a bare executable carries the whole item: everything added is in
     the code, and everything else is in retail's data, which every
     installation has.
   - **Timing** (2026-10-25: some messages went by far too
     quickly). A line stays up until the next one, and it types itself out
     first. Across retail's 195 timed lines (every world, English) a line
     gets a median of 5.9 s per 100 characters, never under 3.6 s; the
     first program gave four of Pelagia's 1.9 to 3.3 s, so they were
     replaced before they had finished typing. Each now gets about 5.8 s
     per 100 characters of its longest language (French and German run a
     fifth longer), 4 s at least, about 80 s in all (the Command Centre's
     runs 112). The gate stays in view while its line is up.
   - **The story's holes**, closed the same day: "we are not even sure how
     you got here" while the player's own scientist had just repaired the
     gate (878 now credits the repair with reopening a forgotten route);
     "the aliens have ignored it" with a Nemian and a quixle on the map
     (877 admits a few of their creatures); a colony with no colonists
     (876: most left when the routes moved on); and a hovercraft with no
     purpose (881: hovercraft mode crosses the open sea to the outer
     islets). The hovercraft is a form the factory gives, as the buggy is,
     so no line or objective builds one.
   - **Two briefings**, by the rework flag (the same day): with
     it the gate leads home and the factories stock the hovercraft and
     the headlights; without it world 1's gate is retail's (a scientist
     wakes it and it leads to Terra Nova) and its factories hold nothing
     new, so lines 879 to 882 and the objectives tell that instead: the
     training ends here. Both run 875 to 883; the rework's tour adds the
     two factories.
   - **The objectives**: resupply with crystals, ammunition
     and any upgrades you find; then warp back and finish the training
     (with the rework), or warp to the front line on Terra Nova
     (without; that gate needs no scientist, 2026-10-25).
     Retail's list shows a row a second and hands over to Start at 5 s, two seconds after its fourth row; with one
     objective Pelagia's sat on empty rows (switching from objectives to
     start was too slow). Its list now hands over two seconds after its
     own last row, 3 s either way (`list_close_millis`);
     retail's worlds keep their 5 s.
6. **The hovercraft early**: factory 2, beside the start, holds Hovercraft
   mode (`+7A = 0xA7`, as Terra Nova's hidden base factory holds Plasma
   cannons by its `+7A`). Connecting teaches it ("New blueprint:
   Hovercraft mode") and Facilities lists it. Two red crystals (kind 16)
   by the factory (2 red each), with the green ones already on the map,
   pay for it.
   - Headlights too (2026-10-23), at the factory nearest gate
     73: factory 17, about 6,900 from it (factory 2 is about 12,800). It
     keeps its Standard Grenades (`+7A`) and takes Headlights by a command
     23 with 0x14. Two blue crystals (kind 14) by it pay for them, with a
     green one; the map's 30 green cover both purchases.
   - Crystals as the egg's reward (2026-10-23: someone who finds
     an easter egg expects to get something out of it): more blue and red
     across the islands, about 15 of each in all with those by the
     factories, so a thorough player leaves with all three tanks full
     (30 each, 2 a crystal; the green ones already fill theirs). A few
     stand on the outer islets, which the hovercraft reaches. Their places
     come from the look over the map (7). The briefing names neither the
     crystals nor the prices: the map and the shop show them.
7. **The map's fixes**, decided 2026-10-23:
   - The five ships seen: world 1's perships take model 665 from Command
     Centre's bank (level 28), as the database's model fix borrows a
     second residency's models (built 2026-10-26: the world draw falls
     back to a second residency activated at level 28). This reverses
     item D's decision. Everything else that reads an actor's model
     borrows it too (2026-10-25): the terrain, pair and furniture
     contacts, the shots' actor geometry and the aim's sight test had
     stopped at the NULL model, so the vehicle and its shots went through
     the ships (7 pair stops in 21 minutes, and furniture and sight stops
     by the colony; none since).
   - A sunk token trail (2026-10-25): four tokens from the lagoon's south
     shelf (20 under) west along its bed (64, 48 and 77 under), which the
     traced water shows glowing up through it. Retail sinks tokens only
     where the buggy and the truck still reach them (world 18's
     lie 72 and 83 deep), so these keep to 16 to 90 under the plane. Its
     end holds a sunken cache: two red crystals and a blue, the scarce
     ones, on the shelf past the last token (44 to 70 under); their
     additive faces glow through the water as the tokens do.
   - No lamps for its night: its light alone runs day, sunset, night
     ([the sky plan](sky-per-world.md#world-1-unnamed)), and the
     headlights carry the dark, as Artoria's do.
   - Its land and water: a top-down survey (shaded relief, water, every
     placed item marked, colony zoom, legend) is in
     `.tmp/pelagia/survey/` for picking (2026-10-25); any change
     to the terrain or the sea waits for that pick.
   - Its sky stays Stage 2's E7.
8. **Two switches**: `fixes.prototype_world`, on by default, with a launcher
   row and `--prototype-world false`, item 22's pattern; off, world 1 and
   gate 81 are retail's. `fixes.pelagia_rework`, on by default, with its
   own launcher row and `--pelagia-rework false`: off, world 1 stays
   retail's (no lagoon, no egg crystals or tokens, retail return gate and
   factories) and its briefing says so. The lab turns both off. Offline only,
   as the Teleport is.

## Ideas for more (2026-10-25, to pick from)

What makes Pelagia worth the detour is that it is a quiet, abandoned
place that changes as its day runs out. These build on that, cheapest
first; all five are built.

1. **The colonists' story (built 2026-10-25).** Each of world 1's six
   scientists, as it boards the craft, says one line of the colony's
   story on the message line, once, as gate 81's scientist reports
   (`pelagia::colonist_line`, marked by `prepare_world` with the rework;
   told from the boarding `00468160`'s vehicle arm). Together they tell
   it in play rather than in the briefing, and point at the rework's
   secrets: the outer islets' crystals and the lagoon's glow. Seen live:
   the first scientist boards, "New blueprint: Turbo boost", then its
   line under it.
   - 0: The others left on the last transport, Commander. I stayed to keep the factories running.
   - 1: We heard a warpgate hum for the first time in years. We thought the routes had come back.
   - 2: Mind the creatures on the far islands, Commander. They have never liked visitors.
   - 3: There are crystals out on the islets. None of us has had a hovercraft to reach them.
   - 4: Something sank in the lagoon long ago. On calm days you can see it glowing under the water.
   - 5: Tell the Command Centre that Pelagia is still here, Commander. Someone should remember us.

   The other languages' are in [`pelagia.rs`](../../crates/inf-game/src/pelagia.rs)
   (`COLONIST_LINES`), retail's words and forms of address as above.
2. **The colony lit at night (built 2026-10-25).** As the sunset fades
   (`pelagia::nightfall` of the schedule's tint: off by day and through
   the sunset, full at night), warm lights come on by the colony's huts
   and the two factories' pads, and a cool one by the return gate, so the
   headlights the briefing offers lead between pools of light and the
   way home shows (`NIGHT_LIGHTS`, lights of the tracer's own fed beside
   the shots', `night_lights`). Path tracing only; retail stays dark.
   Seen live at night (the schedule's timer set to 21 minutes for the
   probe): the huts' sand warm under the night's blue.
3. **A beacon over the outer islets (built 2026-10-25).** The last of
   `NIGHT_LIGHTS`, over the north-east islets whose crystals only the
   hovercraft reaches, pulsing once every two seconds. Not a terrain
   light like placement 21: retail places those for their marker, and
   its light pass found their settings gate closed at every call sampled
   ([selector 54](../re/OBJECT_RUNTIME.md#selector-54-rotating-terrain-light)).
4. **Rain with the night (built 2026-10-25).** World 1's tuning gives
   it rain that waits for the night (`precip.with_night`): as thick as
   night has fallen (`world_scene::nightfall`), none by day, its rings on
   the water with it, and over the painted sky too, where every other
   world's weather keeps to the modern sky. It is the rework's, so world
   1 has it only with the rework on (`SceneFeed::night_weather`). Seen
   live at night: light streaks through the colony's lights.
5. **Ruins on the lagoon bed (built 2026-10-25).** The colony's sunken
   corner beside the token trail's cache: an old pier's four posts
   walking out from the shore (8 to 76 under), a store hut (100 under),
   and the colony's statue, the dark monument that stands by its palms,
   rising half out of the lagoon (104 under), with a stone. All world 1's
   own furniture, placed as records of the port's own
   (`TerrainRuntime::add_furniture`: the row's bucket in X order, the
   cell marked, as the loader keeps every authored record; `RUINS`).
   Each takes an even cell, as every authored record does: the coarse
   ground's furniture walk visits even rows and keeps even columns. They
   show in every renderer and stand solid to the craft. The map's ships
   would not have done as wrecks: selector 34 is the `pership` class,
   which patrols, attacks and crashes.

## The layout, redone (2026-10-25)

After the first pass the layout was disliked, and other maps were to be
investigated for inspiration. The first pass had sprinkled
42 egg crystals over the colony's island, laid a ruler-straight token line
and a crystal grid on the flat south-east island, and left the arrival
island bare. Retail's levels, surveyed top-down (Level 1, worlds 3, 5, 10,
12, 13, 15 and 20), lead the player instead: token trails follow the
routes between sights, islands join by land bridges, and the caches sit in
craters and rings the player climbs into. The rework now reads as a walk
from the gate, with the rework flag only (rework off keeps retail's map):

1. **Gate Head.** The ACAM comes out of the gate on the east island, which
   keeps retail's outpost (its sandbag trench and four barracks). The
   dormant quixle (placement 19) no longer waits there. A red and a blue
   crystal stand at the head of the trail.
2. **The gate's trail.** Eleven tokens from the gate west between the
   barracks, round the trench's south end and over the neck to the
   colony's east side (`TOKEN_SPOTS`).
3. **The colony** keeps retail's layout, its two red crystals by factory
   2, and factory 17's two blue on the west island. The lagoon, its sunk
   trail, its ruins and its cache stay as they were.
4. **The sandbar.** A shallow S of sand, 40 over the water, across the
   strait from Gate Head's south knob to the south-east island
   (`ground::SANDBAR`), so the crater isle needs no hovercraft.
5. **The crater isle.** The flat south-east island rises into a cone
   (`pelagia/ground.rs`): grass slopes and a rock rim 750 over the island,
   a grass floor 380 over it inside. A road climbs clockwise round its
   east and south flanks, never steeper than 0.3, with 14 tokens along it,
   and through a notch in the west rim, flanked by stones, onto the floor.
   There a statue stands among six palms, ringed by four red and four blue
   crystals, the egg's biggest cache, with a lamp lit at night. The
   quixle sleeps beside it.
6. **The outer islets** keep a red and a blue each, for the hovercraft,
   under the beacon.

The ground is written whole: height, material (world 1's sand 0, rock 1,
grass 2, read off its own map) and the baked shade the draws read, from
the new slope against the world's light and scaled from the shade the
island had (`TerrainRuntime::write_sculpted_cell`). The diagonals are then
prepared again and Ultra ground built again (`refresh_sculpted`). The
egg's crystals fall from 42 to 20 (and the lagoon's three); its tokens
grow from 8 to 25 (and the lagoon's four).

## The briefing's views (2026-10-25)

The briefing's camera went to the centre of what it showed,
ending inside the gate's frame. Retail's briefing camera (`0040C770`)
moves its eye onto its target actor, 400 over the ground, looking along
its heading 22.5 degrees down; retail's briefings aim it at markers beside
what they show. Pelagia's program now asks for a stand-off instead
(`briefing_camera::stand_off_reference`, a port bit retail's placement
references never set): the eye stands 1,600 back from the gate, or 1,200
from the smaller factories, against the record's heading, so its subject sits ahead in the
pane. The gate is seen from the side the ACAM comes out of (130 degrees),
factory 2 from over the lagoon looking east into the colony (90), and
factory 17 from over the channel to its east (270; from the south a
rock stood in front of the eye). The camera crosses at speed 800, about
6,000 units a second, so even the long hop from the gate to the colony
lands early in its line.

## The texts

Latin-1, as `port_text.rs` keeps them. Each language uses retail's own
words for the warpgate, the factory, the scientists, the front line and
the Command Centre, retail's form of address (vous, Sie, tu, tú, je) and
its objectives' form, and the shop's own name for hovercraft mode. The
translations are mine, so a native speaker's read is welcome. The
briefing's title, 874, and the gate's name are "Pelagia" in every
language. Slots 875 to 883 are the briefing's lines, 896 to 899 its
objectives (rows left empty stay empty); 879 to 882 and the objectives
differ by the rework flag. The scientist's report at gate 81 is a port
text of its own, last in each list.

### English

- 875: This is Pelagia, an old water world on the edge of the outer colonies. It was settled nearly two hundred years before the alien invasion, in the early days of the warp routes.
- 876: Its settlers built their homes on its few islands and lived from the sea. When the warp network grew and its routes moved on, most of them left.
- 877: Pelagia offers no strategic advantage, and the alien fleet has passed it by. Only a few of their creatures have been seen on the islands.
- 878: We were not expecting you here. The Command Centre's warpgate had been dead for years, and nobody knew where it led. Your scientist has reopened a forgotten route.
- With the rework:
  - 879: Your only objective is to get back to the Command Centre and finish your training.
  - 880: The colony's old warpgate, situated here, still works and will take you back.
  - 881: The colony's factories still work too. This one can give your ACAM hovercraft mode, to carry you across the open sea to the outer islets.
  - 882: The factory here, on the west island, can fit your ACAM with headlights. You may want them, as night falls on Pelagia.
  - 896: 1 Resupply with crystals, ammunition and any upgrades you find
  - 897: 2 Warp back to the Command Centre and finish your training
- Retail's world 1 (the rework off):
  - 879: Your only objective is to get off Pelagia. Its warpgate does not lead back to the Command Centre, so your training ends here.
  - 880: The colony's old warpgate, situated here, still works, and it leads to the front line on Terra Nova.
  - 881: The colony's factories still work, and crystals lie across its islands. Resupply here before you leave for the front.
  - 882: Night falls quickly on Pelagia, so make the most of the daylight.
  - 896: 1 Resupply with crystals, ammunition and any upgrades you find
  - 897: 2 Warp to the front line on Terra Nova
- 883: Enjoy the sunset, commander. Transmission ends.
- Gate 81: Commander, I have successfully repaired this warpgate.

### Français

- 875: Voici Pelagia, un ancien monde aquatique aux confins des colonies extérieures. Il fut colonisé près de deux cents ans avant l'invasion extraterrestre, aux premiers temps des voies de transfert.
- 876: Ses colons ont bâti leurs maisons sur ses quelques îles et vivaient de la mer. Quand le réseau de transfert s'est étendu et que ses routes se sont déplacées, la plupart sont partis.
- 877: Pelagia ne présente aucun intérêt stratégique, et la flotte extraterrestre l'a ignoré. Seules quelques-unes de leurs créatures ont été aperçues sur les îles.
- 878: Nous ne vous attendions pas ici. Le portail intersidéral du Centre de contrôle était hors service depuis des années, et personne ne savait où il menait. Votre savant a rouvert une route oubliée.
- With the rework:
  - 879: Votre seul objectif : regagner le Centre de contrôle et terminer votre entraînement.
  - 880: Le vieux portail intersidéral de la colonie, situé ici, fonctionne encore : il vous ramènera.
  - 881: Les usines de la colonie fonctionnent elles aussi. Celle-ci peut doter votre VCM du mode aéroglisseur, pour traverser la haute mer jusqu'aux îlots du large.
  - 882: L'usine située ici, sur l'île de l'ouest, peut équiper votre VCM de feux avant. Ils pourraient vous être utiles, car la nuit tombe sur Pelagia.
  - 896: 1 Faire le plein de cristaux, de munitions et d'équipements
  - 897: 2 Regagner le Centre de contrôle et terminer votre entraînement
- Retail's world 1 (the rework off):
  - 879: Votre seul objectif : quitter Pelagia. Son portail ne ramène pas au Centre de contrôle : votre entraînement s'arrête donc ici.
  - 880: Le vieux portail intersidéral de la colonie, situé ici, fonctionne encore, et il mène au front, sur Terra Nova.
  - 881: Les usines de la colonie fonctionnent encore, et des cristaux sont dispersés sur ses îles. Ravitaillez-vous ici avant de partir pour le front.
  - 882: La nuit tombe vite sur Pelagia : profitez de la lumière du jour.
  - 896: 1 Faire le plein de cristaux, de munitions et d'équipements
  - 897: 2 Regagner le front sur Terra Nova par le portail intersidéral
- 883: Profitez du coucher de soleil, Commandant. Transmission terminée.
- Gate 81: Commandant, j'ai réparé avec succès ce portail intersidéral.

### Deutsch

- 875: Dies ist Pelagia, eine alte Wasserwelt am Rand der äußeren Kolonien. Sie wurde fast zweihundert Jahre vor der Invasion der Außerirdischen besiedelt, in den Anfängen der Warprouten.
- 876: Ihre Siedler bauten ihre Häuser auf den wenigen Inseln und lebten vom Meer. Als das Warpnetz wuchs und sich seine Routen verlagerten, zogen die meisten fort.
- 877: Pelagia bietet keinen strategischen Vorteil, und die Flotte der Außerirdischen hat sie übergangen. Nur einige ihrer Kreaturen wurden auf den Inseln gesichtet.
- 878: Wir haben Sie hier nicht erwartet. Das Warpgate im Kommandozentrum war seit Jahren außer Betrieb, und niemand wusste, wohin es führt. Ihr Wissenschaftler hat eine vergessene Route wieder geöffnet.
- With the rework:
  - 879: Ihr einziges Ziel ist es, zum Kommandozentrum zurückzukehren und Ihre Ausbildung abzuschließen.
  - 880: Das alte Warpgate der Kolonie, das sich hier befindet, funktioniert noch und bringt Sie zurück.
  - 881: Auch die Fabriken der Kolonie arbeiten noch. Diese hier kann Ihren ACAM mit dem Hovercraft-Modus ausrüsten, mit dem Sie über das offene Meer zu den äußeren Inselchen gelangen.
  - 882: Die Fabrik hier, auf der Westinsel, kann Ihren ACAM mit Scheinwerfern ausrüsten. Sie werden sie brauchen, denn über Pelagia bricht die Nacht herein.
  - 896: 1 Versorgen Sie sich mit Kristallen, Munition und Upgrades
  - 897: 2 Warpen Sie zurück zum Kommandozentrum und beenden Sie Ihre Ausbildung
- Retail's world 1 (the rework off):
  - 879: Ihr einziges Ziel ist es, Pelagia zu verlassen. Das Warpgate hier führt nicht zum Kommandozentrum zurück, Ihre Ausbildung endet also hier.
  - 880: Das alte Warpgate der Kolonie, das sich hier befindet, funktioniert noch und führt an die Front nach Terra Nova.
  - 881: Die Fabriken der Kolonie arbeiten noch, und auf ihren Inseln liegen Kristalle. Versorgen Sie sich hier, bevor Sie an die Front aufbrechen.
  - 882: Die Nacht bricht schnell über Pelagia herein: Nutzen Sie das Tageslicht.
  - 896: 1 Versorgen Sie sich mit Kristallen, Munition und Upgrades
  - 897: 2 Warpen Sie nach Terra Nova an die Front
- 883: Genießen Sie den Sonnenuntergang, Commander. Übertragung beendet.
- Gate 81: Commander, ich habe dieses Warpgate erfolgreich repariert.

### Italiano

- 875: Questo è Pelagia, un antico pianeta oceanico ai margini delle colonie esterne. Fu colonizzato quasi duecento anni prima dell'invasione aliena, agli albori delle rotte spazio-temporali.
- 876: I suoi coloni costruirono le loro case sulle poche isole e vivevano del mare. Quando la rete spazio-temporale si estese e le sue rotte cambiarono, quasi tutti se ne andarono.
- 877: Pelagia non offre alcun vantaggio strategico e la flotta aliena lo ha trascurato. Sulle isole sono state avvistate solo alcune delle loro creature.
- 878: Non ti aspettavamo qui. Il portale del Centro di comando era fuori uso da anni e nessuno sapeva dove conducesse. Il tuo scienziato ha riaperto una rotta dimenticata.
- With the rework:
  - 879: Il tuo unico obiettivo è tornare al Centro di comando e completare l'addestramento.
  - 880: Il vecchio portale spazio-temporale della colonia, situato qui, funziona ancora e ti riporterà indietro.
  - 881: Anche gli stabilimenti della colonia funzionano ancora. Questo può dotare il tuo ACAM della modalità hovercraft, per attraversare il mare aperto fino agli isolotti più lontani.
  - 882: Lo stabilimento situato qui, sull'isola occidentale, può dotare il tuo ACAM di fanali anteriori. Potrebbero esserti utili, perché su Pelagia cala la notte.
  - 896: 1 Rifornirsi di cristalli, munizioni e potenziamenti
  - 897: 2 Tornare al Centro di comando e completare l'addestramento
- Retail's world 1 (the rework off):
  - 879: Il tuo unico obiettivo è lasciare Pelagia. Il suo portale non riporta al Centro di comando: il tuo addestramento finisce qui.
  - 880: Il vecchio portale spazio-temporale della colonia, situato qui, funziona ancora e conduce alla linea di frontiera su Terra Nova.
  - 881: Gli stabilimenti della colonia funzionano ancora e sulle sue isole si trovano dei cristalli. Rifornisciti qui prima di partire per il fronte.
  - 882: Su Pelagia la notte cala in fretta: approfitta della luce del giorno.
  - 896: 1 Rifornirsi di cristalli, munizioni e potenziamenti
  - 897: 2 Raggiungere la linea di frontiera su Terra Nova
- 883: Goditi il tramonto, Comandante. Fine della trasmissione.
- Gate 81: Comandante, ho riparato con successo questo portale.

### Español

- 875: Este es Pelagia, un antiguo mundo acuático en el límite de las colonias exteriores. Fue colonizado casi doscientos años antes de la invasión alienígena, en los primeros tiempos de las rutas de teletransporte.
- 876: Sus colonos construyeron sus hogares en sus pocas islas y vivían del mar. Cuando la red de teletransporte creció y sus rutas cambiaron, la mayoría se marchó.
- 877: Pelagia no ofrece ninguna ventaja estratégica y la flota alienígena lo ha pasado por alto. Solo se han visto algunas de sus criaturas en las islas.
- 878: No te esperábamos aquí. El portal de teletransporte del Centro de Mando llevaba años sin funcionar y nadie sabía adónde llevaba. Tu científico ha reabierto una ruta olvidada.
- With the rework:
  - 879: Tu único objetivo es volver al Centro de Mando y terminar tu entrenamiento.
  - 880: El viejo portal de teletransporte de la colonia, situado aquí, aún funciona y te llevará de vuelta.
  - 881: Las fábricas de la colonia también funcionan. Esta puede dotar a tu MACA del modo hovercraft, para cruzar el mar abierto hasta los islotes exteriores.
  - 882: La fábrica situada aquí, en la isla del oeste, puede equipar tu MACA con luces largas. Puede que las necesites, porque en Pelagia cae la noche.
  - 896: 1 Reabastecerte de cristales, munición y mejoras
  - 897: 2 Volver al Centro de Mando y terminar tu entrenamiento
- Retail's world 1 (the rework off):
  - 879: Tu único objetivo es salir de Pelagia. Su portal no lleva de vuelta al Centro de Mando, así que tu entrenamiento termina aquí.
  - 880: El viejo portal de teletransporte de la colonia, situado aquí, aún funciona y lleva al frente en Terra Nova.
  - 881: Las fábricas de la colonia aún funcionan y hay cristales repartidos por sus islas. Reabastécete aquí antes de partir hacia el frente.
  - 882: En Pelagia la noche cae deprisa: aprovecha la luz del día.
  - 896: 1 Reabastecerte de cristales, munición y mejoras
  - 897: 2 Teletransportarte al frente en Terra Nova
- 883: Disfruta de la puesta de sol, comandante. Fin de la transmisión.
- Gate 81: Comandante, he reparado con éxito este portal de teletransporte.

### Nederlands

- 875: Dit is Pelagia, een oude waterwereld aan de rand van de kolonies. Ze werd bijna tweehonderd jaar voor de alien-invasie gekoloniseerd, in de begintijd van de warproutes.
- 876: De kolonisten bouwden hun huizen op de paar eilanden en leefden van de zee. Toen het warpnetwerk groeide en de routes verschoven, vertrokken de meesten.
- 877: Pelagia levert geen strategisch voordeel op en de alienvloot is haar voorbijgegaan. Alleen een paar van hun wezens zijn op de eilanden gezien.
- 878: We hadden je hier niet verwacht. De warppoort in het Commandocentrum werkte al jaren niet meer en niemand wist waar hij naartoe leidde. Je wetenschapper heeft een vergeten route heropend.
- With the rework:
  - 879: Je enige doel is terugkeren naar het Commandocentrum en je training afronden.
  - 880: De oude warppoort van de kolonie, die zich hier bevindt, werkt nog en brengt je terug.
  - 881: Ook de fabrieken van de kolonie werken nog. Deze kan je GGAE de hovercraft-stand geven, waarmee je over open zee naar de buitenste eilandjes kunt varen.
  - 882: De fabriek hier, op het westelijke eiland, kan je GGAE van koplampen voorzien. Die kun je goed gebruiken, want op Pelagia valt de nacht.
  - 896: 1 Vul je kristallen, munitie en opwaarderingen aan
  - 897: 2 Warp terug naar het Commandocentrum en rond je training af
- Retail's world 1 (the rework off):
  - 879: Je enige doel is Pelagia verlaten. De warppoort hier leidt niet terug naar het Commandocentrum, dus je training eindigt hier.
  - 880: De oude warppoort van de kolonie, die zich hier bevindt, werkt nog en leidt naar de frontlinie op Terra Nova.
  - 881: De fabrieken van de kolonie werken nog en op de eilanden liggen kristallen. Vul hier je voorraad aan voordat je naar het front vertrekt.
  - 882: Op Pelagia valt de nacht snel: maak gebruik van het daglicht.
  - 896: 1 Vul je kristallen, munitie en opwaarderingen aan
  - 897: 2 Warp naar de frontlinie op Terra Nova
- 883: Geniet van de zonsondergang, commandant. Einde bericht.
- Gate 81: Commandant, ik heb deze warppoort succesvol gerepareerd.

## Checks

- Level 1's seeded replay (Infest01) is bit for bit the same with the
  switch on, and so are the lab's Level 1 scenarios with it off.
- A probe on world 18 with a scientist in the hold beside gate 81 (built
  as the lab's `altair-drop-sci.json` builds its own): D, the scientist
  walks to the gate, its report, its walk back aboard, "Warp to Pelagia",
  the warp, world 1's briefing in each of the six languages, and the
  colony's start.
- The Command Centre's status is the same after the trip as before it,
  both from a profile that has not finished it and from one that has
  (then bit 4 is added, as leaving any completed world adds it).
- Gate 73 takes the vehicle with no scientist dropped, and no hint asks
  for one.
- On world 1: connecting to factory 2 gives the blueprint's notice;
  Facilities sells the hovercraft for 3 green and 1 red; form 3 works.
  Factory 17 sells Headlights for 1 blue and 1 green, beside its
  grenades, and they light.
- Gate 73: "Mission Complete", the save, the Command Centre's first start
  in front of gate 81. The profile loads again from the save list, and no
  completion bonus comes of it.
- Before and after snapshots of each step.
