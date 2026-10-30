# Infestation 0040D800 family: factories, course targets, terrain bridge and links

This document owns the `0040D800` intermediate actor, shared by selectors 15,
23, 30, 37, 39, 47 and 49, and the three members that Level 1 needs:
selector 15 (the factory), selector 37 (a terrain-strip bridge) and selector
39 (a destructible target).
Selector 30's own construction and update stay in
[SELECTOR30_RUNTIME.md](SELECTOR30_RUNTIME.md). Shared registration, payload
and handle lookup belong to [ACTOR_RUNTIME.md](ACTOR_RUNTIME.md).

The implementations are
[family40d800.rs](../../crates/inf-game/src/object_runtime/actor/family40d800.rs),
[selector15.rs](../../crates/inf-game/src/object_runtime/actor/selector15.rs),
[selector37.rs](../../crates/inf-game/src/object_runtime/actor/selector37.rs),
[selector39.rs](../../crates/inf-game/src/object_runtime/actor/selector39.rs)
and [selector47.rs](../../crates/inf-game/src/object_runtime/actor/selector47.rs).

## Family prefix

`0040D800` runs `0047A7A0` and writes the prefix described for
[selector 30](SELECTOR30_RUNTIME.md#constructor-and-payload): byte `+138=1`,
maximum `+13C` and current `+140` zero, WORD `+144=FFFF`, link bytes
`+14C/+14D` and flag `+14E` zero, timer `+150` and sound `+154` null.
`Actor40d800State` now lives in the family module and selector 30 embeds it.

| Slot | Address | Behavior |
| --- | --- | --- |
| `+0C` | `0040D920` | `0047A610` context, then `0040D950` over a nonzero payload |
| `0040D950` | | `0047AC10`, maximum `+13C` from payload `+44` with the signed clamp, spawn offset `+146/+14A` from `+48..+4D`; a nonzero maximum with no stored count sets `+B4=0` and dirty 2 |
| `+04` | `0040D9C0` | codes 16..20 (maximum, flag `+14E`, refill, `+144`, offset); codes up to 15 reach `0047ADA0` |
| `+1C` | `0040E000` | the same empty-store test, then `0047AAD0` |
| `+34` | `0040DFC0` | a completed placement (`0048EA80` over signed `+78`) fills the store through `0040D8D0`, ORs dirty 2 and sets `+B4=2` |
| `+54` | `00404610` | keeps a completed placement for its own `+34` |
| `+98` | `0040DE70` | link messages below |
| `+AC` | `0040DE20` | the death of the factory and the terrain bridge (below) |

The refill `0040D8D0` adds a delta to `+140`, ORs dirty `0200` (`+39 |= 2`)
for a nonzero delta, and clamps with the signed tests.

The factory (`00521940`), the gate (`005289C0`) and the terrain bridge
(`0051FF54`) share the damage virtual `+B4` `0040E030`. It returns at once
for zero damage or a lifecycle `+C4` of 1 or less (signed). Otherwise it
writes the hit colour, `+CA = 55` and `+C8 = (min(damage * 512, 7FFFh) as
a WORD * 31250) >> 15`, as `004283E0` does but without that function's
`+144` rescale and flag clear, and runs the shared reaction `0047CBC0`
([the port](../../crates/inf-game/src/object_runtime/actor/damage_reaction.rs)).
The course target (`00522398`) keeps its own `0043BD90`, which is not
ported.

[`constructor_actor_specialized.rs`](../../crates/inf-formats/src/constructor_actor_specialized.rs)
decodes the `0040D950` fields as `Family40d950Payload` on the `0040D920` and
`0042EDD0` wrappers. Selector 30's `0048B3C0` keeps its own view.

## The family's death

`0040DE20`, the base table's `+AC`, is the death the factory (`00521940`)
and the terrain bridge (`0051FF54`) keep; `0047D670` runs it at state 1,
after a shot through the shared damage `0040E030` and reaction `0047CBC0`
takes the last of their health. It records the placement's death
(`0048EAD0(+78, 0)`, so a reload finds it complete), scorches the ground under
the actor (`0047E190(5)`) and sets the fuse `+150` to 1,000 microseconds with
`+14E` set, else 2 s. Neither class's update reads the fuse: only the base
update `0040DB40` burns one, and no constructor bank holds a selector that
keeps the base table (23). [The port](../../crates/inf-game/src/object_runtime/actor/family40d800.rs)
runs it from the world's state-one dispatch; before, a dying factory or bridge
stopped there. No lab scenario kills one: a buggy firing at Level 1's truck
factory for 3,000 passes (`level1-factory-light`'s start) never damages it.

## Link messages

`0047DD40(handle, range, code, argument)` resolves the handle through
`0047AD90` and calls `0047DD80`, which skips a target farther than a nonzero
range and otherwise calls the target's `+98(sender, distance², code,
argument)`. The family's `0040DE70` keeps a link count:

- Message 7 adds one to `+14C` and sets `+14D`. On an actor in state 2 with
  a positive count it locks the actor: `0047D670(0, +84, 3)`.
- Message 8 removes one. When the signed count drops below 1 on a linked
  actor in state 3, it unlocks the actor: `0047D670(0, +84, 2)`.
- Other codes return zero.

A receiver runs its `0047D670` inside `+98`, before the next message, so the
loader applies each delivery's change at once. A second completed target that
links the same actor therefore sees the state the first one left. Fresh actors
start in state 3, so links counted at load lock nothing. An unlock at load,
which only a completed target can cause, takes state 3 down to 2. That step
runs the `+68` test (null for the shared, derived and `0040D800` tables) and
attribution into the mission's statistics record W+0C. It also runs the
selector-18 count and placement death with the live W+08. The retained-manager
load lends that mission; a clean import keeps `LinkUnlockBoundary`. Locks run
through `0047D670` with a host that no upward change reaches.

Other receivers of link codes 7 and 8:

| Receiver | Behavior |
| --- | --- |
| Selector 11 `00402420` | Counts in DWORD `+198`; message 8 also clears `+190`; unlocks without a linked byte |
| Selector 60 `00480F40` | Counts in DWORD `+1A4`, linked byte `+1A8` |
| Selector 30 `0048C470` | Forwards to `0040DE70`; codes `0xD/0x14/0x15` have their own arms |
| `004270B0`, selector 12 `00468BE0`, `004045F0` | Return zero (central derived and family bases, selectors 10/12/16/34/35/36/77/88/90/93, shared) |
| Selector 2 `00420BE0`, selector 66 | Boundaries; selector 2 looks up the player first |

## Selector 37: terrain bridge

Case `0x25` allocates `0x180` bytes and calls `004099A0(0x400, 0x100, 0)`
over `0040D800`. It writes length `+158=0x400`, width `+15C=0x100`, height
offset `+160=0`, seconds `+164=4`, direction `+165` from the argument,
bytes `+166/+167/+168=0`, timer `+16C=0`, countdown `+170=4,000,000`,
WORD `+17C=FFFF` (the update passes 0..72 to `0046C760`) and grant gate
`+17E=0` (tested through `004752E0`). The sampled strip ends
`+174/+178` stay unknown until the update writes them.

Commands `0040A1D0` pass codes up to 20 to `0040D9C0`, then:

| Code | Operand and effect |
| --- | --- |
| 21/22 | WORD, zero-extended to `+158/+15C` |
| 23 | signed WORD to `+160` |
| 24 | BYTE to `+164`, and BYTE times 1,000,000 to `+170` |
| 25 | BYTE boolean to `+166` |
| 26/28 | WORD to `+17C/+17E` |
| 27 | BYTE to `+165` |

Its `+98` (`0040A180`) restarts `+170` from the raw `+164` BYTE, not
microseconds, for message 2 with `+167` clear or message 3 with `+167` set,
then always runs `0040DE70`. Its `+34` is the family `0040DFC0`.

### The bridge's update `00409A20`

[The port](../../crates/inf-game/src/object_runtime/actor/selector37/bridge.rs)
runs the whole update, for dry and water bridges:

1. **Sampling.** The first update samples the strip's ends under the
   temporary fine spacing (`00450350`). Then `00450370` restores the
   spacing from settings `+70`, rather than to coarse. Directions 0..3 run
   along +Z, +X, -Z and -X. The start is 0x100 from the bridge and the end
   is the length `+158` from it, each the raw cell height (`movsx << H14
   scale`) plus `+160`, into `+174` and `+178`. Then `+168 = 1`. A water
   bridge (`+166`) instead reads the water plane, the H12 entry's first
   DWORD (`00405540(0)`), shifted down by `005483BC`, as a signed WORD
   into `+174` (`00409A44..00409A5C`); `+178` stays unwritten.
2. **The hold.** The bridge holds its one-second timer `+16C` full while
   any of these is true: it has free staff (`+CC`, `0040E090`); its links
   hold it (`+14D` set, `+14C` nonzero); or the first player lacks grant
   `+17E` (`004752E0`). A bridge with staff (`+140` nonzero) skips the
   links and the grant. Otherwise the timer counts down by the delta, and
   when it runs out it sounds 0x17 (`00487F10`, volume 0x10000, pitch
   0x4000). Before that count, a water bridge whose links are set and
   released (`+14D` set, `+14C` zero) starts the mission's doom countdown
   while the timer is still running: `G+1C7 = 1` and `G+1D4 = +164`
   seconds (`00409DEF..00409E1F`). Each such update restarts it at the
   full time, so it runs down from the update that empties the timer.
3. **The rise.** While the countdown `+170` runs and `+16C` is empty, each
   update first sets the first player's commands `+17C` (when it is 0..72)
   and silent 0x31. Then `+170` counts down; at its end `0040DCB0` sends
   out the staff and `+13C/+140` clear. The strip follows under the fine
   spacing: cells 0x80 apart, from 0 to the length along and from
   -`+15C` to `+15C` across, starting 0x100 from the bridge. Each cell
   moves from its current height toward the ramp
   `+174 + (+178 - +174) * along / +158`, by the elapsed share
   `(+164 s - +170) / +164 s` of the difference (`00450890`). When `+170`
   reaches zero the cell takes the ramp itself. A water bridge skips the
   strip and both spacing calls and moves the plane instead
   (`0040A0DD..0040A146`): `+174` plus `+160` times the elapsed
   microseconds shifted down by 10, over `+164` thousands, shifted back up
   by `005483BC` into the H12 DWORD. Shifting by 10 but dividing by
   thousands leaves the full rise at 1000/1024 of `+160` (512 rises 499).
   The shared status `0047C990` ends every update.

`00450890` stores the height arithmetically shifted down by the H14 scale
as the cell's low byte, keeping the other byte, and prepares nothing. A
cell already on the ramp therefore stays, and a cell 16 units off moves
only once its share rounds past a step. The staff go out through
[`0040DCB0`](#evacuate-0042f780-0040dcb0) with no vehicle (`00409ED5`
pushes 0), so each scientist stays where it comes out, at the bridge's
`+146` offset on the bridge's team, under the slot flash, and the emptied
bridge turns team 0. Altair Prime's sends its scientist out 500 along its
X, where retail's came out (tampered-run: `runtime_re/lab/scenarios/altair-drop-sci.json` (a constructed save with a scientist in the hold, the vehicle teleported beside the gate), pass 320).
The port stops at a strip cell that needs ends a
direction above 3 never sampled (`00409F62`) or the stale cell such a
direction leaves (`0040A01E`). The last stops are the divisions by zero
seconds (`0040A071`, `0040A119`), which neither command 24 (it writes both
`+164` and `+170`) nor the restart (`+170` from `+164`) can set up. An
empty strip reaches none of the cell stops.

Level 1's bridge (placement 106) runs -X for 3500 with half-width 300 and
4 seconds; both its ends sample at -48. Infest02 opens it at the third
course death: `+16C` runs out over eight updates with the sound, then 32
updates of `+170` write its 140 cells each and set commands 0x33 and 0x31.
The cells rise from -320 toward -48 in steps of 16, and it stays raised
when the first target comes back. [The bridge
test](../../crates/inf-game/src/object_runtime/actor/selector37/bridge_tests.rs)
replays those 60 updates over the loaded Level 1
([bridge_rise_native.json](../../crates/inf-game/tests/fixtures/bridge_rise_native.json)).
It matches the sampled ends, the cells before, each update's timers, sound
and commands, and every cell after each of the 32 rising updates. The
vehicle [drive receipts](VEHICLE_DRIVE_RUNTIME.md#evidence) cross the
risen strip.

No recording reaches a water bridge, so
check_selector37_update.py runs the
original update under Unicorn
([selector37_update_native.json](../../crates/inf-game/tests/fixtures/selector37_update_native.json)).
It covers 27 supplied sequences of 284 updates: water bridges free, linked
(the doom restarts while the timer runs), gated, staffed, restarted and with
a product past 32 bits, and dry strips in all four directions, wrapped,
empty and held. It also covers the stops. The bridge test matches each
update's fields, `G+1C7`/`G+1D4`, plane DWORD, spacing, changed cells and
calls.

## Selector 39: course target

Case `0x27` allocates `0x168` bytes and calls `0043B6D0` over `0040D800`,
which zeroes the retained pointer `+158`, `+15C` and bytes `+160..+166`.
Commands `0043B7D0` pass codes below 21 to `0040D9C0`, then:

| Code | Operand and effect |
| --- | --- |
| 21 | retains the operand address at `+158`: a count, then a relocated array of one-based placement references |
| 22 | DWORD to `+15C` |
| 23/25/27/28 | BYTE booleans to `+162/+163/+161/+165` |
| 24 | BYTE seconds to `+160` |

`0043B7D0` has no case 26 and returns zero for it.

The `+34` (`0043B720`) tests the placement's completion and clears `+138`.
For each reference it resolves `0047F7B0` (the first actor whose `+2A` is
the reference minus one, or the null handle), writes the handle back into
the H14 array, and sends message 7 through `0047DD40` with no range. A
completed target also sends message 8, sets session byte G+1C3 (mission
`+13`, which starts the
[world tint's schedule](GAMEPLAY_RUNTIME.md#the-world-tints-schedule)) when
`+162` is set, and disposes itself. The count and array pointer are reread
per row. Receivers are 37, 39 or the shared actor (`004045F0`, which returns
zero); any other class stops at `LinkMessageBoundary`.

Its `+98` (`0043BDF0`) handles messages `0xB/0xC` (set `+164`, then retire
through `0047CE90` when `+160` is set) and `0x14/0x15` (adjust `+166`).
Those need world services and stay boundaries; every other code is
`0040DE70`. Its `+14` (`0043BF10`) returns 1 for selector 1 unless the
actor is in state 3. For selector 2 it animates the `+150` timer against
the `+160` window: inside its last 250 ms the signed /250 rise from
`0xC000`, under 200 ms the signed /200 fall from `0xFFFF`, otherwise
`0xC000`.

### The hangar takes a scientist (`0043BEA0`)

The target's `+D0` is `0043BEA0`. It notes whether the target has room
(`+CC(0)`, the free staff `0040E090`, above 0 as a signed count), then
offers the scientist to the family's take `0040DAB0`
([staffing](#staffing-0042f170-0040dab0-beam-down-0042f670)): the target
takes the scientist's team, flashes at it, disposes of it and gains one
staff, and the scientist that fills it records the placement. When the take
fills a target that had room and its `+7A` is nonzero, the first player
(`00488AA0`) is granted `+7A` (`004750C0` on `+9C`), with its message.

Only the Defence Satellite's hangar has room (26.ovl placement 40,
resource 0x15): command 16 gives it one staff, and its `+7A` is 0x35,
"Defence satellite captured", which takes Foldrak Noir's attacks away at
load ([the attack](CONSTRUCTOR_ASSETS.md#the-alien-brains-attack-selector-62))
and opens the satellite's gate to Foldrak Noir (placement 30, command 32).
Altair Prime's three hangars, the same resource, have no staff and give
0x17 by their deaths. In the lab
(scene) a
scientist beamed down beside the hangar is taken at its first update,
tick 37, with the grant and the message; the port matches retail in every
compared event, and both frames show "Defence satellite captured". The
port used to stop the pair there (`0043BEA0` a typed boundary), so the
scientist stood by the hangar and nothing was granted. Covered by
`a_course_target_that_a_scientist_fills_grants_its_7a`.

### The target's life

[The port](../../crates/inf-game/src/object_runtime/actor/selector39/course.rs)
follows a target from its damage to its return:

- **Damage `+B4` (`0043BD90`).** A target with `+161` or `+166` set takes
  nothing. So does a collision (packet channel 1) whose source resolves to
  an object outside resources 0x29..0x2C. Everything else reaches the
  family's `0040E030`.
- **Death `+AC` (`0043B910`),** which `0047D670` calls at state 1. Message
  8 goes to each link (`0047DD40`, no range). A timed target (`+160` set,
  `+163` clear, and `+14D` clear or `+14C` set) adds to the mission's
  countdown instead ([timed generators](#timed-generators)) and skips to
  the shared tail below. Otherwise the
  death is marked (`0048EAD0(+78, 0)`). A target with seconds clears the
  mission's `+24` and `+17` (G+1D4, G+1C7), lights its fuse `+150` at
  `+160` seconds and sounds 0x32 (volume 0x10000, pitch 0x18000). One
  without seconds, [a generator](#generators), scorches six ground cells
  (`0047E190(5)`) and takes a 2 s fuse. With `+162` it sets the mission's
  `+13` (G+1C3). The first
  player then gets command `+15C` when set, and silent commands 0x39, 0x3A
  and 0x3B. Last, unless another actor of the same resource and grant ID
  `+7A` lives in state 3, every player is granted `+7A` (`004750C0`), and
  `+15C = 0xC` adds silent command 0x23. A new grant formats its message,
  "New info: Heavy generator" for Tabuchi's 0x13 generator (`+7A` 0x4F), so
  the port's death needs the current text binding (`00545C74`). The world's
  damage owners lend it to every path that can reach a death: the world tick,
  the projectiles and the furniture effects.
- **Update `+24` (`0043BB80`).** A target whose family count `+140` is below
  its maximum `+13C` does nothing. With a first player and `+165` clear,
  the target hits that player's primary actor within 1000 with radiation:
  `(1200 - distance) * 4` on channel 3 for 2 s (`0047C7C0`), the packet's
  fourth byte being the delta's. Then `0047DD80` sends it the notice 0x17
  within 1000. A target without seconds takes the family's `0040DB40`
  ([below](#generators)), and one without a fuse follows the mission's
  countdown (`0043BCCD`, [below](#timed-generators)). A fused target counts
  `+150` down by the delta, sounds 0x32
  as it passes 0.2 s, and at its end returns: `0043BFE0` raises it to state
  3 (`0047D670`), restores its health `+A2` and sends message 7 to its
  links. The shared status `0047C990` ends every fused update.
- **Light `+28` (`0043B8A0`).** It keeps flag `08000000`. With `+165` set,
  or below state 3, it lights nothing. Otherwise it draws twice from the
  process RNG and lights the ground at its X/Z twice (`0045B1A0`): a glow
  of radius `((draw & 0x3F) + 0x60) * 4` in `0054761C`'s (8, 8, 24), then a
  shade of radius 256 in `00547618`'s (-8, -8, -24), both constants
  `0043B680` stores at startup. The second draw (`| 0xF`, never zero)
  decides nothing. The [light pass](../../crates/inf-game/src/object_runtime/light_pass.rs)
  takes the world's RNG for it; the briefing's pass lends none, so there it
  stays the `TargetFlicker` boundary. A world-11 run's last frame deposits
  21 lights where it used to deposit 5 and skip the 8 generators.

Level 1's seven targets all have `+163` and `+165` set, and seconds 30 (the
three course targets at placements 96..98), 5 or 3. Infest01 never shoots
one: each runs only its status update, 707 times. Infest02 shoots six,
the one with death index 33 twice. Each death sends message 8, makes the three commands and
marks the death; each fuse ends 240 updates later for 30 s, with its sound
on the 239th. The three course targets link to the bridge: the third death
takes its count `+14C` to 0 and opens it (state 2, `0040DE70`'s
`0047D670`), and the first return closes it (state 3). The deaths clear
G+1C7, which the mission constructor sets to 1. Target 189's link names
the selector-69 barrier (reference 191), which counts that one link; the
target's death releases it.

REA decompiles the barrier's class (2026-10-12):
constructor `00411090` over `0047A7A0` (vtable `005205D8`) sets `+144 = 6`,
`+14C = 600`, `+154 = +158 = -32767` and the link count `+15C = 0`. Its
commands `00411100` take `0x11` into `+144` and twice the value of `0x10`
into `+14C`; Level 1's placement 190 gives 10 and 1800, at
`(23005, 300, 37783)` turned 6007.

- `+1C` `00411150`: half its length `+150 = +14C / 2` becomes the radius
  `+28`. A zero `+14C` or `+144` disposes it at once. Otherwise it samples
  the ground from the half length down every 0x40, at that distance either
  way along its X axis, keeping the highest heights towards the positive end
  in `+154` and the negative end in `+158`. It stands at their mean and puts
  two posts (`+138..+143`) a sixth of the length either way along X, each
  250 below its end of the line. It replaces the shared `0047AAD0` without
  calling it, so `+74` keeps the constructor's value.
- `+24` `00411380`: for each listed actor with `(+2C & 0x8C) == 4` (modelled,
  `0x8` and `0x80` clear) at lifecycle 3 and below the higher end, it turns
  the offset into its own frame (`0047A660`). It hits one whose `|x|` is
  under `+150`, whose `|z|` is under its speed across the line times
  `(delta >> 5) >> 15` plus its radius `+40`, and which is below the line
  between the ends (`(+158 - +154) * x / +14C + Y`). A hit kills it through
  `0047D670(0, barrier, 1)` and builds H13 resource 0x36 at its position
  (`00411548`, `ECX = 0x36`).
- `+28` `00411C50`: a light-grid pulse (`0045B1A0`) at each post's X/Z. Its
  radius is `phase + half/2`, the phase being `(age >> 11) % half` folded
  back from the middle; its colour is blue at level `radius * 64 / half`,
  plus green from level 0x20 up.
- `+2C` `00411570`: `+144` bolts, the i-th at `(i * period / bolts +
  (age >> 11)) % period` with the period five quarters of the length. Under
  the length a bolt runs along the ground (`00450A30`) and births a spark
  (`004894A0(0, point, zero velocity, 0, 0)`); past it, it flies back
  along the straight line between the ends' heights. A bolt deeper than
  0x40 is joined to each post by a tapered quad of H3 sprite 0, at most 50
  wide at either end, near (`0040EF30`) or, with fog at either end, far
  (`00411D50`), mode `[0052F628]` = 4 and the effect colour `[0055F26C]`.
- `+98` `00411CE0`: message 7 adds a link and 8 removes one; when 8 leaves
  none, or on `0x15`, it grants its `+7A` (Level 1's is 0) and disposes.

The age `+148` is never written before the update adds to it: Infest01's
Level 1 barrier held `0x100` and Infest02's `0x52FE38`
(`barrier-age.js`). Only the
bolts' and pulses' phase depends on it; the port starts it at zero
(`BarrierAgeResidueApproximation`).

So the barrier destroys whatever crosses it until target 189 dies.
[The port](../../crates/inf-game/src/object_runtime/actor/selector69.rs)
builds it. check_selector69.py runs the
commands, ground fit, update, light, draw and `+98` over 71 cases and
the [tests](../../crates/inf-game/src/object_runtime/actor/selector69_tests.rs)
match every sample, kill, light, spark and packet byte. On Level 1's fine
terrain the port's ground fit stands placement 190 exactly as Infest01 and
Infest02 recorded at its first update
(`barrier-fit.js`): at 340 between
ends 575 and 105, posts `(23508, 168, 37457)` and `(22502, 12, 38109)`,
one link.
[The course tests](../../crates/inf-game/src/object_runtime/actor/selector39/course_tests.rs)
replay the three course deaths, the bridge's opening and closing, and target
0x14's 240-update fuse, and check the damage guard.

### Generators

A generator is a target without seconds (`+160` is 0) and with its
radiation on. Two resources place most of them, told apart by their H13
damage responses (`0042A8F0`: a hit's strength above the channel's
threshold, times its Q8 multiplier):

| Resource | Model | Health | Responses that hurt it |
| --- | --- | --- | --- |
| 0x12 | 0x2E | 2000 | 0, 2, 3, 5, 7, 8 (x1); 4, 9 (x2); 6 above 100 |
| 0x13 | 0x1A7 | 2000 | 0, 3, 5 (x1); 4 and 9 above 2000 (x2); 6 above 500; 7 above 1000 |

The cannon's shots are channel 6 at 200 (selector 32's `+148`), so they
take 100 from a 0x12 generator and nothing from a 0x13 one. The missiles'
blasts are channel 4 at 2700 (resources 110 and 113) or 12000 (112): a
0x13 generator takes 1400 from the smaller. Most grenades' blasts are
channel 9 at 1500 or 2000, under its threshold; selector 27's 5000 is
not. Infest02's hits on Level 1's 0x12 targets read the same arithmetic:
the cannon's 200 on channel 6 took 100, blasts on channel 9 took twice
their strength, the vehicle's 500 on channel 7 took 500. Resource 0x15
(model 0x12B, 10000 health) answers only channel 2, and 4 and 9 above
2000;
resource 0x14 (500 health) only collisions, which the damage guard admits
from resources 0x29..0x2C alone. The vehicle answers the radiation's
channel 3 above 1500, so a generator hurts it within 825 and a turret
(`00401920`, `(600 - distance) * 8` within 500) within about 412; both
send the notice 0x17, the HUD's "Radiation warning", within their range.
The turret receipt runs retail's own
update `004019E0`, `00401920` included, with the player beside a turret:
231 away (squared 53600) it takes 2952 on channel 3 over the 2 s period
and the notice 0x17, while far away or across the coordinate wrap it takes
neither, and the port's update gives the same
(`selector11_update_matches_native_segment_a`).
`00401920` tests no team, so the player's own turrets burn too. An
unmodified port run from the Terra Nova save shows it on the game's own
keys. After the menus it holds Up from frame 466 to 512, Right for frames
467 to 470, 476 to 479, 485 to 488 and 494 to 500, Left at 506 and Down
from 512 to 515. The buggy stops 281 from the save's team-2 turret at
(10502, -1504, 15083), and frame 563 shows "Radiation warning" over the
turret. A logging build's frame 560 matches the clean build's pixel for
pixel. Its log has the buggy losing 25 or 26 health a 125 ms tick, 5000 to
2926 in about ten seconds: the turret's hit less the hold's repair. The
turret's 2552 or 2560 on channel 3 is 65 or 66 after the vehicle's
response (1500, x1) and the tick's sixteenth of the 2 s period. The
profile's Shielding of 40% (`004212F0`, `100 / 140`) leaves 46 or 47, and
the three scientists in the hold repair 21. Backing off, the buggy regains
exactly 21 a tick.

Infest04's second load (world 9) holds the radiation in retail: 74 hits
from one 0x13 generator on the player's vehicle, each with the notice 0x17
at range 1000, and no turret's
(`radiation()`). The
vehicle's channel 3 is (1500, x1) there too, its flag 0x4000 skips the
mission's scale (81) in `0047CBC0`, and its player's Shielding is 100%. So
a hit of strength S costs ⌊⌊(S - 1500) / 16⌋ / 2⌋, the port's arithmetic.
Between hits the hold's three scientists repair 21 (three `004688A0` calls
from `00412445`). Hits of 848 to 1336
cost nothing and hits of 1604 to 2404 cost 3 to 28. This accounts for 28
of the 29 health changes between consecutive hits. The other change also
includes a notice 5 from `00476C00`, which costs the vehicle 400 on
channel 7 (`00420C4B`), or 200 after Shielding. Read directly, the vehicle
has 2886 entering that hit's `004212F0` and 2686 leaving it (`004283E0`
gets 200). The three repairs that follow bring it to the 2707 the next hit
finds (`radiation-notice5-04.txt`).

Live port runs from saved games show both resources with real keys. On
Terra Nova the cannon takes exactly 100 per shot from the 0x12 generator,
a shot every 250 ms (2000 to 1200 in 2 s). Its radiation costs the buggy
17 to 56 health a tick as it closes in, and the HUD shows "Radiation
warning". On Artoria the cannon's hits on a 0x13 generator log nothing
(channel 6, 200 under its 500). On Tabuchi the 0x13 generator's radiation
costs about 6 a tick at 600 and 48 pressed against it, with the warning. Driven
into, either generator stops the buggy and takes nothing (channel 1,
multiplier 0). A power grenade's blasts reach the 0x13 at 2000 and below on
channel 9 and do nothing. A temporary harness only placed the vehicle in
those runs; the rest was the game's own input.

Rockets need no harness. The charge run's drive from the Tabuchi save
([the run](SELECTOR27_RUNTIME.md#verification)) stops the hovercraft against
the 0x13 generator by frame 548. F5 at 556 then selects the rockets (row 4),
and Z at 582 and at 602 fires two; a press at 562 fires nothing. The first
blast is 2700 on channel 4, which the generator answers with
`(2700 - 2000) * 2` = 1400. The shared reaction `0047CBC0` then applies two
adjustments. Tabuchi's mission scale of 81 gives `1400 * 81 >> 8` = 442. The
shooter's Rockets level of 71 adds `442 * 71 / 25`, for 1697. That leaves 303,
and the second rocket kills it. Two seconds later it bursts into the wreck,
model 0x46. The blasts reach the hovercraft too: 991 and 786 after its
Shielding. The clean build's frame 606 shows two rockets gone. Frame 626
shows "New info: Heavy generator" over the burst, and a build that only adds
logging renders it pixel for pixel the same.

The row-12 [charge](SELECTOR27_RUNTIME.md) (selector 27) is the secondary
weapon that hurts a 0x13 generator. Thrown at one, it sticks to it, takes
the generator's position in each update, and waits for the detonator (row
6). Its 5000 on channel 9 then bursts at the generator's own position:
`(5000 - 2000) * 2` = 6000, three times the generator's health. An
unmodified port run from the Tabuchi save shows it on the game's own keys
([the run](SELECTOR27_RUNTIME.md#verification)). The hovercraft crosses the
lake and stops against the 0x13 generator, and the charge sticks to it. The
detonator, fired from about 900 away, takes the generator from 2000 to 0;
"New info: Heavy generator" appears, and two seconds later the wreck (model
0x46). Before the port lent the death its grant text, that death stopped at
the grant, whatever weapon dealt the blow. The same drive, firing the primary
from against the generator instead, leaves it at 2000. Eight plasma hits
(channel 7 at 500, under its 1000) and sixteen cannon hits after F1
(channel 6 at 200, under its 500) all do nothing, under "Radiation
warning".

Shot to death, a generator scorches the ground and takes a 2 s fuse. Its
update then runs the family's `0040DB40`:

- **Unlit fuse.** A generator its links hold up (`+14D`, the first message
  7) dies (`0047CE90`) once their count `+14C` falls to 0.
- **Burning.** The fuse counts down by the delta and `0047DFD0` smokes.
- **The end.** Two child visuals, resources 0x36 and 0x37 (`+4C`, the
  shared child factory), the burst of 16 debris (radius 0x14, types 1, speed
  8), a local generator's blast of `00531250` from its position with its
  own handle, then the wreck: model `+144` (0x46 when command 19 left it
  unset, `+39 |= 1`, `0047A110`), `+74` cleared, the basis turned in XZ by a
  random angle (`00491530`), the fuse and `+C8` cleared, and it leaves the
  update list (`00479F20(0)`).

`0047C990` ends each branch. The port ran none of this before: its first
update stopped at `0040DB40` and the world never updated the generator
again, so its radiation could fire once at most.
[A test](../../crates/inf-game/src/object_runtime/actor/selector39/course_tests.rs)
runs world 11's 0x13 generator through the radiation, the death, 15
burning updates and the explosion. No recording shoots a generator:
`generator-life.js` finds
Infest03's 17 world-11 generators idle and its `radiation()` no generator
or turret radiation in Infest01..03 (Infest04's is above). The original
instructions run instead in
`check_world_interactions.py`: 60
updates through `0043BB80`'s own dispatch (burning, the explosion with and
without the blast, timed targets regrowing, coming back through the real
`0043BFE0` or retiring, fuses), 48 deaths (generators, fused, timed and
held targets, with and without another of their kind alive), 40 lights
and 48 messages. The port's services in order, fuse, wreck and model
words, `+74`, `+C8`, health, dirty word, basis, RNG and mission bytes
match every case.
### Timed generators

World 12's three generators and seven of world 14's eight have seconds
(command 24: 30 and 20), no fuse and `+161` set, so no hit hurts them
(OVLs 22 and 24, model 626; world 14's eighth has a fuse). Only
the chain grenade's message 0xB or selector 57's 0xC knocks one out
(`0043BDF0`: `+164` 0 or 1, then `0047CE90`). Its death (`0043B987`) does
not mark the placement: the world's countdown G+1D4 (mission `+24`) takes
its seconds in microseconds, four times that after 0xC. Unless G+1C8 is
set, a countdown already running and no longer than the target's own
seconds keeps going and gains half the new time; a longer one is
replaced. G+1C7 (`+17`) clears. The shared tail (`+162`, the commands, the
grants) follows.

The mission tick `0048E2E0` counts G+1D4 down by its delta and ends it at 0
(`0048E32A..0048E51A`). The knocked-out target's update (`0043BCCD`) dies
outright if links held it (`+14D`) and none are left; otherwise, while the
countdown runs, its health climbs back as the elapsed share of its seconds
(signed milliseconds), and when the countdown ends (or G+1C8 is set) it
comes back through `0043BFE0`: state 3, full health, message 7 to its
links. None of these branches runs `0047C990`.

With G+1C7 set, a running countdown instead sets off explosions near the
player's unit under 5 s and at its end kills the vehicle, then 3 s later
ends the world (`00487560`), as below. Course targets clear G+1C7 when they write
G+1D4 (a fused one zeroes both). That does not keep worlds out of those
arms (REA, 2026-10-12): the mission constructor sets
G+1C7 to 1 (`0048DA60`), and three other writers set it together with a
countdown:

- the respawn `0046A890` at a world's loss limit: G+1D4 = 1, G+1C7 = 1
  ([the loss limit](VEHICLE_DRIVE_RUNTIME.md#the-death-camera-0040f890-and-the-respawn));
- selector 37's water bridge, `00409DFD..00409E1F`: G+1C7 = 1 and G+1D4 =
  BYTE `+164` seconds, when `+14D` is set, `+14C` clear, `+166` set and
  `+16C` nonzero (`00409DEF`), again on every such update;
- selector 49's Seismic Stabiliser event `00476520`: G+1D4 = `+15C`
  seconds, G+1C7 = 1 ([terrain event](TERRAIN_RUNTIME.md)).

The end, `0048E4C7..0048E51A`, takes two stages. With `+18` clear it sets
`+18`, kills the player's unit's vehicle (`+64`, then `0047CE90`) and
restarts the countdown at 3 s. With `+18` set it clears it, sets mission
`+15` to 1 and calls `00487560(W+A0, 1, 0)`, ending the world into the
current world again. With `+15` set the results page reads "Mission Failed"
(H0 `0x15B`), and its constructor restores the saved profile instead of
saving (`0042AD46`), so the retry starts from the last save. The first
stage's writes come in this order: `+24 = 0` (`0048E4CA`), `+18 = 1`
(`0048E501`), the retirement when the first player's unit (`0046AE60`)
answers its virtual `+64` (`0048E50C`; the call retires the unit itself,
`ECX` = the unit at `0048E513`), then `+24 = 3,000,000` (`0048E51A`).

Each tick of the last 5 s with a player unit (any unit: a camera counts
too, `0048E378`) is a chance, drawn with the process RNG `0048F060`: a draw
whose rest by `left / 2,000,000 + 2` (signed; 2 to 4) is zero sets one off
(`0048E380..0048E3AB`). Three more draws place it by the unit's `+32`
position: X and Z by `rest(800) - 400`, Y by `rest(400)` up, each a WORD
add (`0048E3C4..0048E419`). The world factory builds resource `0x37` there
(`00460310`, `ECX` 0x37, NULL command and rotation, placement -1, mode 0,
the current NULL; `0048E40E..0048E42E`), and a `0048A8B0` burst follows at
the same point: four type-1 debris within 20, a type-1 centre, speed 8, no
flags (`0048E433..0048E484`). A last draw, on the same chance, flashes the
whole world: `00487D70(NULL, 0, 0x28)` (`0048E489..0048E4C0`).

The port runs all of it. [The mission](../../crates/inf-game/src/world_mission.rs)
owns the countdown's fields and stops the first stage after `+18` with a
player unit (`UnitEnd`); the session's
[countdown](../../crates/inf-game/src/session_platform/countdown.rs) then
retires a vehicle through the world's lifecycle owners and arms the 3 s,
and builds each explosion through the world factory, the burst and the
flash. Before, the port held a doom with a living unit at its end, so it
never failed the mission; worlds 3 and 5 now end in "Mission Failed" in
the sweep, and a test runs a 4 s doom on Level 1 to explosions and the
vehicle's retirement. The segment with a unit runs natively over 64 cases
(`mission_countdown_unit` in
`check_world_interactions.py`): the
port's draws, points, burst records, flashes, retirements and fields match
every one, a non-vehicle unit (its `+64` the real `00401300`) included. The
original countdown segment
(`0048E32A..0048E521`, with no player unit) runs over 40 supplied
countdowns in `check_world_interactions.py`:
the port's `+24`, `+18`, `+15` and world-end call match every one, signed
expiries of negative countdowns and both stages of the doom's end included.
[The mission](../../crates/inf-game/src/world_mission.rs) owns the
arithmetic, and [a test](../../crates/inf-game/src/object_runtime/actor/selector39/course_tests.rs)
knocks out one of world 12's generators, regrows a quarter of its health
a quarter of the way through and brings it back at the end.

World 11's four 0x12 generators need grant 48 (command 13), which a new
player lacks; Infest03's player had it, and its first 0x13 generator was
held up by them (`+14C` 4). A run loading a world-11 profile, with the ACAM
held 600 from the generator at (62475, 0, 29942), shows the "Radiation
warning" and the ACAM's health falling; killed through `0047CE90`, the
generator burned 2 s, blew up with its debris and blast, and left the wreck
on darkened ground.

## Selector 15: factory

Case `0x0E` allocates and calls `0042E250` over `0040D800` (final
`00521940`). It zeroes `+15C/+160`, `+168..+174` and `+184`, stores the
process NULL `0054E894` at `+188`, and builds a grant owner at `+198`
(`00475020(0)`: vtable `00527818`, empty mask, null notification `+1B4`), the
type selector 12 embeds at `+24C`. Bytes `+1BD/+1BF` become 1;
`+1B8/+1BC/+1BE/+1C0/+1C4` are zero.

| Slot | Address | Behavior |
| --- | --- | --- |
| `+0C` | `0042EDD0` | `0047A610`, then `0042EE00` over a nonzero payload: `0040D950`, `+158 = 0`, `+15C` = payload DWORD `+50` |
| `+04` | `0042F100` | codes up to 20 reach `0040D9C0`; 21 accepts without a read, 22 stores a DWORD at `+15C`, 23..25 grant a WORD ID into `+198` (`004750C0`) |
| `+1C` | `0042E330` | `+164` = health WORD `+A0`; `+178/+17A` = low WORDs of `+140/+13C`; `+17C..+182` zero; grants its own `+7A` into `+198`; with `+15C == 1`, `+39 |= 1`, model `0xBD` through `0047A110` and, with a nonzero store, dirty 2 and `+B4 = 0x12`; then the family `0040E000` |
| `+14` | `0042E550` | model selectors 3..8 return the WORDs `+178..+182`; others return the selector |
| `+10` | `0042E3D0` | the model's custom vertex: exact 6 captures the dock row (extra WORD 0) at `+190..+194`; exact 7 births the chimney trail while the free capacity (`+CC(0)` = `0040E090`, `+13C - +140`) is zero; others return the X/Z at the ground height `00450A30(x, z, 0)` |
| `+28` | `00431DC0` | production states other than 4..6 (`+160`) return at once. The grant state (5) draws twice from the RNG and lights the dock point `+190`/`+194` (`0045B1A0`), radius `((draw & 0x3F) + 0x60) * 2` in (32, 32, 96); the second draw decides nothing. Then in states 4..6 the chimney model (`+42` = 0x9D) with WORD `+180` set lights it again, radius 300 in (64, 64, 0). The colours are the startup initializer `0042E200`'s. Ported in the light pass (`factory_lights`), with the receipt's 40 cases and the lab's `level1-factory-light` (every RNG step alike); without the RNG, a dock point or the chimney's `+180` it is the `FactoryProduction` boundary |
| `+98` | `0042F380` | 6 and `0x18` answer the unit's broadcast (below); `0xD` on a local factory is [the smoke](ACTOR_INTERACTIONS.md#smoke); others reach `0040DE70` |
| `+34` | `0040DFC0` | the family after-load |
| `+24` | `0042EE30` | production update ([`factory.rs`](../../crates/inf-game/src/object_runtime/actor/selector15/factory.rs)), with its item arm `0042E89E` (below) |

`+3C/+C0/+C4` are network paths (`00431A90`, `00431AD0`, `00431C00`). A null
notification makes `004750C0` the pure ownership closure, which the port
applies with the executable grant table.

### The model's dock point and chimney

The dock point `+190..+194` is a vertex of the factory's own model:
[the custom vertex](../../crates/inf-game/src/object_runtime/model_custom_vertex.rs)
captures it on every draw (exact selector 6), and nothing else writes it.
The constructor leaves it unwritten. Infest01's three factories request
selectors 6, 4 and 7 on each draw, 1818 calls from the briefing on. Factory
`0384F080`'s dock is `(90DD, 50, 842B)`, 528 units in front of its centre.
Selector 7 trails smoke (type 6 for extra WORD 1, type 5 for 0) at the
chimney row while the store is full, gated as selector 60's exact 7 but
with the load slack divided by 8192; the factory with free capacity
returns its ground position instead.

The factory model (H8 157 in `1.ovl`) then reads register 17 in a child
context before writing it: opcode 16 at `1.ovl+1C19E` compares it with the
immediate 1, with a jump of 3. A taken branch continues at the first
argument WORD plus the jump in WORDs and a fall-through at that WORD plus
three, so both continue at the next instruction and the comparison decides
nothing ([opcode 16](MODEL_RUNTIME.md#packed-values-and-register-ownership)).
The register is stack history: from the briefing on, Infest01's 738 reads
held `472` in 266, `1A` in 246, 0 in 113, 1 in 111 and `FFFF` in 2, and no
later command sees it. The port reads the unwritten register as nothing
there and draws the rest of the factory; it used to stop the model at that
read, which left the factory's later parts undrawn near Level 1's first
factory.

### The shop menu `00431000`

[The shop](../../crates/inf-game/src/factory_shop.rs) builds the menu the
docked player's message shows. Its rows are copies of ten templates at
`00532478`, and its layouts copy the group at `005325B8`, whose third and
fourth records are patched with the option count (at most eight visible)
and the Back row's. Row 0 is the header, "Equipment Factory" (`0x2F6`, or
`0x2F7` "Enemy Factory" when `+15C` is 1); row 1 is a title. The texts
quoted here are the retail English strings. By the state `+160`:

| State | Title | Rows after it |
| --- | --- | --- |
| 0 | "Options" (`0x308`) | Facilities (`0x2F8`, to state 1), Energy Distribution (`0x2F9`, to state 2), Evacuate factory (`0x305`, `0042F780`), Disconnect (`0x2FA`, `0042F620`) |
| 0, short of scientists | the staff row (`00430C50`) | Beam down scientist (`0x310`, `0042F670`), Evacuate factory once a scientist is inside, Disconnect |
| 1 | `0x30A`, drawn by `0042FD70` | a row per listed product (`0042F870`, action `0042F7E0` with the product index), then Back (`0x306`) |
| 2 | Energy Distribution, drawn by `0042FC00` | seven token levels (`00430AB0`, action `0042F800` with the level), then Back |
| 3 | the build progress (`00430E30`) | none |
| 4, 5 | the fitting progress (`00430D70`) | none |
| 6 | "Recovering for next process" (`0x302`) | none |

The factory holds scientists, `+140` of at most `+13C`; one that needs
none drops Evacuate from its Options. Products are 0x14-byte records
(`0042F250`): 18 shared at `005320D8`, then by `+15C` the lists at
`00532240` and `00532348`, each ended by the name `0xFFFF`. A product is
listed (`0042F1D0` counts them) when the factory's grant mask `+198` owns
its ID and the player either does not own it (`0046BDC0`) or it is
repeatable (`+C`). Its row is disabled (flag 0x40) when one of the three
token counters `+120/+124/+128` is below its cost (`+E..+10`; only the
payment tests the crystal cost `+D`), or when `00431950` finds its weapon
already stocked to the product's amount (`+A` against `0046D310`, DWORD 0
of an entry of the first player's unit's behavior `+18`). The Energy
Distribution rows of upgrades the player does not own (IDs at `00532608`)
get flags 0x48. The selection starts at 2, or 1 in states 3 to 6. Without
a player for `+188` the menu keeps no rows.

Infest01's twelve builds match the port row for row and layout for
layout: the Options, Energy Distribution with only upgrade 0x32 and the
always-owned ID 0 enabled, Facilities listing only the truck (index 0x13,
`00532254`) at the first factory and only turbo (index 0x12, `00532240`,
five of the second token) at the second, and the production titles.

### Docking, the shop and production

The factory is a shop. [Its update](../../crates/inf-game/src/object_runtime/actor/selector15/factory.rs)
`0042EE30(delta)` first handles the dock `+188`. While it holds the process
NULL, the cooldown `+1C4` counts down; once it is spent, the first player's
primary unit (`0046B030`) is tested at the pad (`0042EC60`):

- The squared distance to the dock point `+190..+194`, with the height
  difference halved, is compared with the vehicle's radius `+28` squared.
  Farther than four times it re-arms the factory (`+1BD = 1`) and sets the
  cooldown to 50 µs per unit of the root of `d² - 3r²`, at most 2 s; between
  two and four times nothing happens.
- Within twice it, the vehicle docks if the factory is armed and the
  vehicle is slower than 700 (`+8E/+92`, the same doubled root above bit
  31).

Docking (`0042F490`) needs a player for the vehicle. When the factory is
neutral (team 0 or 1) or on the vehicle's team, `+188` takes the vehicle's
handle, the player's `+DC` the factory's (`0046C860`), the player gains
every ID the factory's mask owns with their notices (`00475350`), the
factory's mask becomes the player's (`00475380`), event 22 goes to every
actor (`0047DAF0` with radius 0), and the shop is built and shown
(`004137D0(0, menu)`). Showing it sends event 6 to the actors within 0x300
of the player's unit (`0047DD20`): factories re-arm and answer 1, the
vehicle (after its player lookup), the camera and selector 12 answer 0,
and a nonzero sum sets the message's `+34`, which nothing reads in
Infest01. Another team's factory says "Access denied" (`0x2FB`) and
disarms; team `0x12` also sets command `0x1D`. Either way, the player then
gets commands 3, 4 and `0x26` silently.

Only a vehicle at most 700 fast docks (`0042ED9C`/`0042EDB2`: the XZ speed against
`0x2BC`), so one driven across the pad at full speed passes: the buggy
cruises near 1050 with Up held, in retail as in the port. Driven at the
factory, though, it docks: the dock point sits at the building's face
(528 in front of the factory's centre), so the vehicle hits the factory
just past it, the collision stops it below 700 and the next pad test docks
it. The shop shows, and under it the controls brake (thrust -100 times the
forward speed for the buggy and the truck, `0046A719`, which also zeroes
turn and pitch, so a held Up no longer drives) while the pull below draws
the ACAM onto the dock point. It stops short where the pull's
`(delta << 12) * offset >> 31` (`imul`, then `shl eax, 1` / `rcl edx, 1`:
a floored shift) reaches 0 for a positive offset: below 5 at 125 ms ticks,
below 32 at a 16384 µs tick, as retail's is at 60 fps.

`factory-dock-run.js`
reads the vehicle updates around a dock and snapshots the vehicle and the
pad before it. Replayed in the port from those snapshots (a temporary
harness writes the vehicle's drive state, the dock point, `+1BD` and
`+1C4`, then the recorded keys play), all three recorded docks match
update for update:

- Infest01's first (factory `0384F080`, Level 1): the buggy coasts in at
  279, docks, brakes (-27900, -22600, -5500) and is pulled to
  (37081, 33831), 4 short of the dock point on both axes. All 25 captured
  updates match.
- Infest01's second (factory `0384F250`): Up held at 1053 straight at the
  factory. The face stops it at 264, the pad docks it, the shop shows and
  the pull brings it onto the dock point in about 2 s. On fine terrain
  (settings `+70 = 1`), as recorded, all 34 updates match; the port run
  that keeps Up held 5 s longer is identical. Later in that visit the
  player held Down in the shop and the vehicle stayed on the pad.
- Infest03's (Tabuchi): the buggy coasts in at 687 after a hover request
  (`+34 = 3`) two updates earlier. The dock brakes it (-68700) on the tick
  the mode becomes hover, so the brake ends (modes 1 and 2 only), the face
  turns it aside and it glides off along the building; the message hides
  when it leaves half the factory's radius. Every update matches until the
  player presses Down, nine updates after the dock.

Where the vehicle bounces off the face, the port's sample falls after the
contact pass and retail's update ends before it; the next update agrees
again.
`factory-pad.js` finds no
crossing faster than 700 in the recordings: Infest01's and Infest03's 21
pad tests near a dock were the four docks below and 17 unarmed tests while
a vehicle left the pad.
`vehicle-speed.js`
reads every tenth vehicle update: the buggy's samples reach 1055 in
Infest02 and 1625 in Infest01, where two-thirds of the moving ones lie
between 700 and 1100. The recorded docks came at 279, 264 and 62
(Infest01) and 687 (Infest03). A port run on Terra Nova driven from 1500
across the pad crosses it at 1020, and the shop stays shut.

While docked, a vehicle farther than half the factory's radius (`+28`
squared over 4) or no longer alive (`+C4 < 3`) hides the player's message
(`004133C0`). Otherwise each axis is pulled toward the dock point by
`(delta << 12) * offset >> 31`, capped at 16 toward the positive side only
(`0047DDF0` writes the position, not the velocity), and every tick the world
camera (`00478E00`) gets a new follow target on the factory (`00410E70`)
with mode 1 (`+188`) at the offset (300, 500, -1500) (`+194..+198`).

`0042E570(delta)` then runs production on the state `+160`. The state store
`00431CE0` rebuilds a bound menu (`+1B8`) through `00431D10` unless the
state is unchanged or moves between 4 and 5.

- A fully staffed factory (`+140` equal to `+13C`) repairs its docked
  player's unit: `0046B2B0(4, delta / 2000)` while the unit's damage
  fraction `(max - health) * 65536 / (max + 1)` is nonzero.
- 3 builds `+16C` up to `+158` (progress WORD `+17E`), then 4. A zero build
  time passes at once. Short staff resets the state to 0 first.
- 4 charges `+170` toward 2 s while docked and drains it without a vehicle
  (progress `+180`). When full, `+174` restarts, a non-upgrade product whose
  hold has fewer free slots than `+18E` (`004120E0` over the vehicle's
  `+1A4`) sends text `0x143` ("There is insufficient space in your hold"),
  and the state becomes 5.
- 5 returns to 4 without a vehicle (its timer one short). It counts `+174`
  past 2 s and grants: an upgrade (`+184 == 9`) is `0046B2B0(+18C, +18E)`,
  with sound `0x30` when the player does not own it yet; other products spawn
  `+18E` items through virtual `+4C` and loads them into the hold
  ([hold items](HOLD_ITEMS.md#the-factorys-item-arm)). On the following tick the
  recovery starts: 6 with `+168` at 2 s, hiding the shop when nothing is
  left to list (`0042F1D0`).
- 6 counts `+168` down (progress on both WORDs) and returns to 1, or 0
  when nothing is listed.

Within 3000 units of the first player's unit a looped sound `+154` plays
`0x1C` in states 3 to 6 and `0x4B` otherwise, restarted when it changes
(`+1C0`), at volume `(3000 - d) * 0xC000 / 3000`; farther, it is released.
The family's destructor `0040D870` stops a live `+154` (`00487FE0`) before
the shared `0047AA60`, and every class of the family reaches it (the
factory's `0042E320`, the gate's, bridge's and course target's `0048B220`,
the Siren's `0047FEE0`, the stabiliser's `004763E0`), so a world's end
silences its factories' hums with them. The port did this for the gate
alone until 2026-10-21: a factory near the last world's gate hummed on
through the briefing.
The docked player's shop menu is the message's shown child; its purchase
callback `00431740` pays the product's crystal and token costs
(`0046C6D0` with its bytes `+D..+10`). On success it starts production
(`00431E50`, state 3); otherwise it sends text `0x303` ("You do not have
enough resources to make this item") and command 7.
Undocking (`0042F2A0`) runs from the menu's destruction (`004315E0`, which
clears `+1B8` first): the camera follows the vehicle again (`00410E70`,
then `0040F800` with First Person), `+1BD`, the dock and the player's `+DC`
clear, and states 1 and 2 return to 0. Leaving the world while docked runs
the same destructor from the player's message cleanup, before the factory
goes ([world cleanup](WORLD_CLEANUP.md#retained-player-results-retirement)).

### Tab, Escape and the pad

The shop is a menu on the docked player's message, so the message keys
reach it. Tab's `00413410` hides a shown message (`+0` clears and the
controller's `+4` starts its fade out); otherwise it builds the computer and
shows it through `004137D0(0, menu)`.

The type-3 controller fades at `+20 = 0x20` per 128 µs of the delta
(`00453560`), so an opaque menu (0xFFFF) fades out in about 262 ms. When the
fade reaches zero the controller's update destroys every menu it holds, and
`00413870` frees it. The shop's destructor `004315E0` then undocks
(`0042F2A0`), which clears `+1BD`. A vehicle still on the pad stays
undocked, because the pad test docks only an armed factory, and only a
vehicle more than twice its radius from the dock re-arms it.

- **A quick second Tab**, while the hidden shop still fades: `+0` is clear,
  so Tab builds the computer, and `00413230` finds the controller alive. It
  gives the controller the computer's viewport (`00414140`), fades it back
  in (`+8`) and pushes the computer over the shop (`00453440`). Nothing is
  destroyed; the vehicle stays docked under the computer.
- **A slow second Tab**, after the fade: the computer shows on a new
  controller. `004137D0` then broadcasts event 6 from the player's unit
  (`0047DD20(0x300, 6, 0)`, the unit from `0046AE60`). The factory's
  `0042F380` re-arms (`+1BD = 1`) and answers 1, which sets the message's
  `+34`. The factory's next update finds the vehicle on its pad, docks it
  and shows the shop through `004137D0`, which pushes it onto the
  computer's controller: the player sees the factory come back.

Escape runs two callbacks: the gameplay screen's `00443070` first, which
hides a shown message (`004133C0`) or, with none, asks for the in-world
menu, then the player's `00469480`, which broadcasts event 0x18 from the
unit within 0x300. `0042F380` answers it with 1: in Facilities or Energy
Distribution (states 1 and 2) the state store `00431CE0(0, 0)` returns the
bound shop to the Options; in any other state it hides the first player's
message (`004133C0`), whatever that message shows. Because `00443070` has
already hidden the shop, Escape closes it in every state; a shop in
Facilities is rebuilt as the Options on the fading controller first. The
factory's own hide matters only where `00443070` returns before hiding (the
attract flag, world 22). Every other receiver the port builds
answers both events with 0 and changes nothing: the vehicle's `00420BE0`
looks its player up first, and the grenades', mines' (`004557F0`, also
selector 27's) and missiles' (`00456AD0`, also selector 68's) `+98`s act on
other codes.

The [port](../../crates/inf-game/src/object_runtime/actor/selector15/broadcast.rs)
runs both broadcasts and the factory's arms. A key callback queues its
broadcast with the unit it found, and the gameplay router runs it with the
shop's tables before the next input event, so the screen's `00443070`
keeps running first. The shop's own `004137D0` runs the same broadcast. A
receiver without a ported `+98` is skipped and logged once.

Infest01 shows the slow Tab (`factory-tab.js`, bundle
`.tmp/infestation-ttd/20261011-factorytab/`): the turbo factory `0384F250`
undocks at `DE3368:627` (`+1BD` 1 to 0) when its faded shop is destroyed;
Tab at `DE5FA3:1543` builds the computer and broadcasts event 6; the
factory's `+98` at `DE5FA4:27` re-arms (`+1BD` 0 to 1, answer 1); its update
docks the vehicle at `DE5FA4:DCE` in state 6, and the shop's `004137D0` at
`DE5FA5:13A7` pushes onto the computer's controller `03856E50`. All five of
Infest02's Escapes call `00443070` before `00469480` and its 0x18
broadcast; none is near a factory.
[The factory tests](../../crates/inf-game/src/object_runtime/actor/selector15/factory_tests.rs)
follow both Tab timings over Level 1's truck factory and Escape in
Facilities and in the Options.

The world tick runs all of this. The dock point is written only when the
factory's model is drawn; Infest01's briefing draws had written every dock
before gameplay's first pad test, so until the port's first draw of a
factory its pad test waits (FactoryDockPointApproximation, logged once).
Infest01's first dock matches the port's pull tick for tick (13 distinct
rows of the `dockpull` query, from (9080, 90, 8400) on).

The shop's [commands](../../crates/inf-game/src/object_runtime/actor/selector15/shop.rs)
are the menu's: its virtual +4 `00431660` sounds the navigation at the
factory (2 for the selection, 3 and 5 for AdjustPrevious and AdjustNext),
then `00452E20` moves the selection or runs the selected row's action:
Back and Disconnect (`0042F620`, through `00431710`), Facilities and Energy
Distribution (`0042F630`, the latter setting the first player's command
`0x27`), a product (`0042F7E0`, the purchase) and a token level
(`0042F800`: AdjustPrevious sets command `0x28`; AdjustNext raises a level
below 100 for one crystal). The purchase refuses a factory short of
scientists (sound 4, "Insufficient Staff"), a fully stocked weapon
(`0x304`) and a product the player cannot pay for (sound 4, `0x303`,
command 7); otherwise it plays sound 5, starts production, stores state 3
and plays sound `0x17` at pitch `0x4000`. Evacuate factory and Beam down
scientist are [below](#evacuate-0042f780-0040dcb0).

### Evacuate (`0042F780`, `0040DCB0`)

Evacuate factory (`0042F780`) finds the docked player (`0046B090` of
`+188`) and its unit (`0046AE60`), whose `+64` view only the vehicle
answers. It sends out the whole staff `+140` through `0040DCB0`, then the
shop backs out (`00431710`), which fades the controller out. The terrain
bridge's rise sends its staff out through `0040DCB0` too, to no vehicle.
[`0040DCB0`](../../crates/inf-game/src/object_runtime/actor/selector15/staff.rs) does nothing for a remote factory. For each
scientist:

- It spawns at the factory's `+146` offset turned by the factory's basis
  (`+48`, then `004909E0`), added to its position, as H13 resource 2
  through the world factory (`00460310` with `EDX` 0, no rotation,
  placement -1, mode 0 and the process NULL).
- A creation leaves the staff (`0040D8D0(-1)`: a nonzero change sets `+39`
  bit 1, and the staff is kept between 0 and the capacity `+13C`).
- When the vehicle's hold has a free slot (`004120E0`), the scientist gets
  `+38` bit 1, the vehicle's team and position, and boards (`00421810`,
  whose result is ignored). Otherwise it gets `+38` bit 1 and the
  factory's team where it stands, under the slot flash `00489A90(2)` at
  the spawn point.
- An emptied factory gets `+38` bit 1 and team 0, and the loop ends.

In Infest01 (the `staff` and `evacuate` queries) the factory at
(47223, 16, 26207), turned half a turn about Y, held 2 of 2 scientists, and
its `+146` was (0, 0, -1000). Both spawned at (47223, 16, 27207); the world
factory set them on the ground at team 1. Each took team 2 and the vehicle's
position (47219, 53, 26734) and boarded, and the factory ended at team 0
with `+38` 0x202. [The factory tests](../../crates/inf-game/src/object_runtime/actor/selector15/factory_tests.rs) replay that evacuation from
the loaded factory, an evacuation without a vehicle, and Evacuate from the
docked shop.

### Staffing (`0042F170`, `0040DAB0`, Beam down `0042F670`)

A scientist joins a factory through its `+D0`
([`0042F170`](../../crates/inf-game/src/object_runtime/actor/selector15/staff.rs)). The factory needs free staff (`+CC`,
[`0040E090`](ACTOR_INTERACTIONS.md#staffed-relation-0040e090) with no
source). The scientist's grants then join the factory's `+198`
(`00475350`), and `0040DAB0` takes it:

- A remote factory or scientist, or a full factory, refuses.
- The factory takes the scientist's team and `+38` bit 1, flashes at the
  scientist (`00489A90(2)`), disposes of it (`00479FA0`) and gains one
  staff (`0040D8D0(1)`).
- The scientist that fills the factory records the factory's placement
  `+78` (`0048EAD0`, force 0), as a destroyed placement does.

`0042F170` then rebuilds a bound shop menu (`00431CE0` with force).

The terrain bridge's `+D0` is `0040DAB0` itself, through the same staff
query: a bridge short of staff takes a scientist the same way, and a full
or remote one refuses it, which sets the scientist's `+22E` bit 0. The port
runs both through one
[`take_scientist_staff`](../../crates/inf-game/src/object_runtime/actor/selector15/staff.rs);
Altair Prime's [scientist gate](SELECTOR30_RUNTIME.md#a-scientist-takes-a-gate-0048bd90)
sends its scientist to the bridge.

Two paths reach it:

- A dropped scientist whose search picked the factory walks into it. In
  the [pair pass](WORLD_CONTACT.md), selector 12's `00468160` finds its
  current target `+148` naming the factory and offers itself to the
  factory's `+D0`. Acceptance ends the pair; a refusal sets the scientist's
  `+22E` bit 0. Level 1 hints at this after an evacuation: "Drop scientists
  close to a factory with the beam down key and they will go to work".
- Beam down scientist (`0042F670`), on the Options of a factory short of
  staff, takes the docked actor's `+64` view (the vehicle). From its last
  occupied hold slot back, while the factory has room, each carried
  actor's `+5C` view (only selector 12 answers) goes to the factory's
  `+D0`. A refusal, which only a remote factory or scientist gives, tells
  the docked player "No scientist to beam in" (`0x311`) and ends it. The
  hold keeps the absorbed actors' handles until
  [its next tick](VEHICLE_DRIVE_RUNTIME.md#the-hold-00412380) clears them.

No recording reaches either path. [The factory tests](../../crates/inf-game/src/object_runtime/actor/selector15/factory_tests.rs)
check the staffing rules, the relation and Beam down from the docked shop.
A live probe evacuates Infest01's factory, drops a scientist, and watches it
walk back in.

### Drawing the shop

The shop's rows draw through [their own callbacks](../../crates/inf-game/src/factory_shop_draw.rs),
which read the menu's factory (`+5C`) and, through `0046B090` of its
`+188`, the docked player; the port gathers both before the draw. Text goes
through `00453810` on the row's descriptor (a left text, then a right one
aligned to the row's end), and bars through `0042FA70(x, y, across, up,
length, height, background, fill)`: two +A4 quads of sprite `0x22B`
(vertex DWORD 0x40), the fill first. A nonzero `across` fills that far from
`x`; otherwise the fill rises `up` from the base line `y`. Sizes below are
in pixels of the surface `W` by `H`, divisions truncating.

- `0042FC00`, the Energy Distribution title: "Spare Tokens" (`0x309`) with
  the player's `+118` right-aligned, and a bar from ten sixteenths of the
  row across, a fifth of its width long and `H / 48` high, filled
  `width * min(+118, 100) / 500` across, in style 0 over style 1.
- `00430AB0`, a token level: the weapon's name (Cannons, Plasmas, Lasers,
  Flamethrower, Rockets, Grenade, Shielding) with its level (the signed
  byte `+13C + index`) as "%d%%", and the same bar of the level.
- `0042F870`, a product: its name in four fifths of the row's width (cut to
  17 characters and "..." beyond 20), then its three token costs
  (`+E..+10`) right-aligned in styles `0x27`, `0x29` and `0x28`, padded by
  four, two and no spaces (the empty left text still draws). A descriptor
  with flag 0x40 takes style 5 first; the shop's layouts never set it (it
  comes from layout flag 0x80), and the menu walk colours a disabled row.
- `00430C50`, the staff row: "Insufficient Staff (present/most)" in style
  `0x28`, shown while `005471C0 / 25` is even. Every draw sets the player's
  command `0x43` (once owned, a no-op).
- `00430E30`, building: a bar from a sixth of the row across, two thirds of
  it long, `H / 24` above the row's bottom, filled with the progress
  `+16C * 100 / +158` (100 without a build time) in style 0 over style
  `0x21`; then "Manufacturing %s" with the name of the product's grant
  (`00475320` of `+18C`). `00430D70`, fitting: "Fitting %s... Please wait".
- `0042FD70`, the Facilities title, for the selected row's product (its
  `+0C`; nothing at all for Back). Shifted up `60H / 256` and right
  `W / 20`, it turns the product's grant model (`00475320` WORD +2):
  `0043A150`'s descriptor (vtable `00522360`) at (0, 0, 6r + 500) for the
  model's first DWORD r, turned in XZ (`00491530`) by the clock times 64,
  the context's eye raised two fifths of that distance, fade from 2000 and
  2500 beyond it, far colour 0, the clip opened to the surface around a
  batch. The vehicle's model (0x75) sits 0x14 higher and first stores the
  product's form in word `00533760`, which its values read: 2 for the
  truck (product 2), 3, 4 and 5 for products 0xA7, 0xA5 and 0xA6 (the
  hovercraft, the VTOL and the skimmer), else 1. A status follows, a 64th
  of the surface higher: "Insufficient Crystals" (`0x30E`) when a token
  counter is below its cost, "Fully Stocked" (`0x30D`) when `00431950`
  refuses the product (both only while `005471C0 / 25` is odd), else
  "Duration: %d secs " (`0x30B`) with the build time; a product without one
  formats "Crystals Required" (`0x30F`) and draws nothing. Then a column per
  token counter at five, three and one `W / 52` from the row's end,
  `W / 52` wide and `H / 16` high: the counter out of 20 in its style
  (`0x22`, `0x23`, `0x24`) over style 8, then the cost over style 7, the
  unpaid cost only while the warning blinks. Each column also formats
  "%d/%d" (cost over counter) in styles `0x27`, `0x29` and `0x28` and never
  draws it. Last, in style 5, the product's name `W / 10` right and `8H /
  256` down, `W / 10` narrower.

The port keeps word `00533760` (its `.data` 1) in the process state: the
shop's preview writes it, the HUD's weapon icons, the briefing and the name
entry read it. The message's model notice `00414DD0`, its other writer, is
not ported.

Infest01's draws (`shopdraw` queries over its five shop menus at 1024 by
768, `panelquads` and `previewvalues`) are the receipts of
[the row tests](../../crates/inf-game/src/factory_shop_draw_tests.rs):
Spare Tokens 10 at x 984 over its bar from 812 (filled to 822) to 914,
y 86 to 102; the levels' "0%" at x 974; Truck mode's costs at x 949, 970
and 991 with its name in 408 pixels; the preview at (545, -78) for the
truck at (0, 20, 1154), eye 461 and fade (3154, 3654), whose 44 value reads
went to `0043A270` (selector 1 read 2); the token columns at x 909, 947 and
985, 19 wide and 48 high; the name at (596, 126). Font 0 at 1024 by 768
right-aligns those texts at the receipts' x, and Level 1's styles are the
receipts' colours.

In Infest01 the player docked twice: factory `0384F080` from about tick 128
(state 3 to 4, 5 and 6 at about ticks 184, 200 and 218, the dock cleared at
about 234), granting the truck (2), and factory `0384F250` from about tick
436, granting three Turbo charges (3). Both visits show the shop as the
player's message (`+88` set), which is when `0046A260` brakes the vehicle
and makes Fire the select.

## Selector 47: the Siren

Case `0x2F` of `004603B0` (`00460EF0..00460F13`) allocates `0x170` bytes
and runs `0047FA40`: the family prefix `0040D800`, then radius
`+164 = 0xA00`, alarm `+160`, dying count `+168`, the DWORD `+158` (the
WORD `+158` and the spin `+15A`) and the turn `+15C` all zero, and vtable
`00527CD4`. It leaves the WORD `+15E`, the alarm seconds BYTE `+16C`, the
silence BYTE `+16D` and `+16E..+16F` unwritten. Its table is the bridge's
`0051FF54` but for `+00` (`0047FEC0`), the commands `+04` (`0047FDC0`),
the custom vertex `+10` (`0047FA80`, its own position), the model WORDs
`+14` (`0047FAA0`, the WORD at `+158` plus twice the selector), the update
`+24` (`0047FAC0`), the after-load `+34` (`0047FE90`, the stabiliser's),
`+98` (`0047FE10`) and the death `+AC` (`0047FAB0`). It initializes through
the family's `0040D920` and `0040E000`.

The commands pass codes up to 20 to `0040D9C0`; code 21 accepts without a
read, 22 copies the seconds BYTE and 23 sign-extends the radius WORD;
higher codes refuse. Its `+98` silences it for 10 s on messages 0xB and
0xC (`+160 = 10,000,000`, `+16D = 1`), sets the alarm on 0xE to the
argument or, when that is zero, to `+16C` seconds, and passes every other
message to `0040DE70`. Its death sets `+168` to 2,000,000.

The update runs the shared status (`0047C990`) first. A dying Siren counts
`+168` down with the status visual (`0047DFD0`) and sets its sound's pitch
to `((+168 / 1000) << 16) / 2000`; at the end it flashes the world
(`00487D70(+32, 0xF00, 0)`), releases the sound and disposes itself. A
silenced one releases its sound and counts `+160` down, clearing `+16D` at
the end. Otherwise, while the family counts `+13C` and `+140` are equal, it
alerts with the process NULL handle when they are nonzero (`0047C840`),
and:

- a running alarm counts `+160` down to zero, raises the spin by
  `delta >> 5` up to `0xFFFF` and starts sound 0xE at full volume and pitch
  (`00487F60`, not during attract playback) or updates it;
- a spent one lowers the spin by `delta >> 5` down to zero and releases the
  sound;
- the spin turns `+15C` by `((delta >> 4) * spin) >> 16`;
- below `+16C` seconds of alarm, the first player's vehicle (`0046B030`) of
  another side within the radius (signed squares), or a linked Siren
  (`+14D`) whose links are all gone (`+14C` zero), sets `+160` to the
  seconds plus `0x7A121` microseconds and alerts the Siren's side to the
  vehicle (`0047C840` with its `+84`).

All twelve installed Sirens (shared H13 slot 26, on the mutants' side
`+B4 = 0x12`, in leaves 14, 18..21, 25, 27 and 31) set their seconds by
command, and nine set a radius. World 11's three dispose themselves at
construction (command 13) unless the player owns grant `0x30`. The
silence BYTE is heap residue at the first update: Infest04's Tabuchi
Sirens hold 32 and 117, every other recorded one 0. Luna Azine's and
Shak'tahh's
[timed spawners](CONSTRUCTOR_ASSETS.md#the-timed-spawner-selector-43-and-its-rocks-selector-44)
come earlier in the update list and erupt on their first update: event
0xE sets each Siren's alarm before the Siren's own first update, which
then depends on the residue. No recording visits either world; in the lab
(`siren-world8.json`) world 8's Siren reads 0 there, with the lab's heap
and with a fresh one, and sounds its 30 s alarm from its first update.
The world load's `FreshHeapZeroExtendedCarryApproximation` starts the byte
at that 0, and the port then matches retail in every compared event and
every tick's RNG over 301 ticks. Without that policy the port leaves the
byte unknown: the first update clears it when both arms agree, as they do
for a quiet Siren the player is not near, and otherwise stops there.

Infest03 and Infest04 build all ten of their Sirens (worlds 9 and 11) and
update them 1,049 and 2,111 times without an alarm: none calls
`0047C840`, starts a sound or spins. The model reads only selector 1, the
spin. The port builds the class and its commands, links, messages, death,
model values and custom vertex, admits it with the bridge's other slots
and updates it on the world tick. Its tests match the ten Sirens'
construction and their state after the load, and check the alarm,
silence, death and residue arms against the disassembly
(`siren.js`).

## Class admission

Every exhaustive class match places 15, 37, 39, 47 and 49 by its slot. The
Siren's `00527CD4` is the bridge's table but for
`+00/+04/+10/+14/+24/+34/+98/+AC`. The
Seismic Stabiliser's 00527908 is the bridge's table but for
`+00/+04/+1C/+24/+34/+98/+AC/+B4` (its `+98` is the family's own
`0040DE70`); see [the terrain event](TERRAIN_RUNTIME.md#near-strip-terrain-particle-emission). The shared
answers are `+58/+5C/+64/+68/+6C/+70/+7C/+B8` null, `+80=0047AAE0` and
`+30=0047EC50`. Selector 37's `+14` and `+28` are the shared bodies.
Selector 15 differs from selector 37 only at `+04/+0C/+10/+14/+1C/+24/+28`,
`+3C/+50/+98/+C0/+C4/+D0` and its own slots from `+D4`.

## Level 1

Mission 18 builds three factories at placements 0, 1 and 3 (model `0x9D`,
flags `4890002F`, team 0, state 3). They grant their own IDs 3, 2 and 37 and
hold stores 2/2, 2/2 and 0/2. Infest01's factories at the first briefing
marker pass (`7417C0:1A2A`) match these fields, positions and masks. Their
flags pass the briefing's ordinary marker test, so they now draw as map
markers. It also builds seven targets (placements 96..98, 189 and 201..203, model
686) and the bridge at placement 106 (model 392, length 3500, width 300,
direction 3, `+17C` 51). Targets 96..98 each link to reference 107, the
bridge, so the loader leaves it with link count 3 and `+14D` set. Target
189 links to reference 191, the selector-69 barrier at placement 190,
which counts that link.
Targets 201..203 have no links. The
[world loader test](../../crates/inf-game/src/world_loader_tests.rs)
pins these links over the real world. The briefing's late camera target
(actor ID 190) is target 189; see
[the briefing camera](BRIEFING_RUNTIME.md#tick-and-drawing-owners).
