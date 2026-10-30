# The Alien Brain and the campaign's end

World 19, the Alien Brain (leaf 29), ends the campaign. One of its classes
(83, the sinking ground) is unbuilt in the port, which skips its 2
placements. This note is a static (REA, Ghidra 12 over the seeded
function census) reading of them; no recording reaches the world, and since
2026-10-15 the TTD route is not used, as replay is
too slow. Native receipts through the Unicorn harness pin each piece as it
is ported.

| Selector | Placements | Constructor / vtable | What it is |
| ---: | ---: | --- | --- |
| 84 | 6 | `0043C070` over the shared actor, `00522488`, `0x140` bytes | Capture node (built) |
| 85 | 1 | `00405F10` over selector 62's `00463F60`, `0051F9CC`, `0x19C` | The brain (built) |
| 78 | 1 | `00406640` over the shared actor, `0051FA9C`, `0x174` | The ending's director (built) |
| 79 | 10 | `00408740` over selector 77's `00407410`, `0051FC8C`, `0x1D8` | The eyes, in five linked pairs (built) |
| 81 | 1 | `00409250` over the base object `00479C60`, `0051FE44`, `0x5C` | A spawner round a slow player (built; holds at its first placement) |
| 83 | 2 | `0043C300`, `00522570`, `0x7C` | Sinking ground ([census](CONSTRUCTOR_ASSETS.md#what-each-unbuilt-class-needs)) |

The world's other classes (6, 14, 30, 35, 42, 50, 51, 58, 77, 87) are built.

## How the campaign ends

Three classes share two process words: `00546910`, the ending's phase,
and `00546914`, the brain's restart flag.

1. The six capture nodes start on the aliens' team (`0x12`), and the
   aliens' count `00547620` holds them. Touching one hands it, and its
   linked actor, to the toucher's team.
2. The brain waits in state 0 until `00547620` is zero, then in state 1
   until the first player's vehicle locks on to it (the vehicle's `+58`,
   `00404610`, answers itself, and its `+D0(0)`, `00402790`, its target
   `+148`, whose `+4` is the brain's handle). Then it grants the player
   Invulnerability (`0046B2B0(0x29, 1)`, with the message's byte, player
   `+89`, set around the call so it says nothing, and player `+58` cleared
   after it) and sets the phase to 1.
3. The director, waiting in state 1 for phase 1, takes the player
   (`0046AE90(own handle)`), heals the player's unit by 1000 (`+A0`, to
   its `+A2`), builds two cinematic helpers (selector 90, `00462750`) and
   points the camera at them: it follows `+164` and looks at `+168`.
4. When helper `+164` arrives (its `+140`), the director sets phase 2.
   The brain runs its attack (selector 62's update `00464050`) while the
   phase is 2, and once the attack ends (`+138 == 3`) drops its own
   lifecycle to 2 (`0047D670(0, handle, 2)`), turns the camera shake on
   (`005483B0 = 1`) and sets phase 3.
5. In phase 3 the director's state 3 kills the aliens' remaining units one
   by one: the camera follows each (flags `0x24`, team `0x12`, lifecycle 3,
   a view whose `+CC` answers 0), five debris bursts (`004665D0`) go off
   round it, and after 500 ms it is retired (`0047CE90`); a unit whose
   `+CC` answers is disposed instead. When none remain the state ends.
6. The director's state 4 sets phase 4; the brain collapses: its model
   registers (`+190..+198`, read by its `+14`, `00406630`) open and fall
   over states 4 to 6, with debris sparks round it, and state 6 sets phase
   5. In state 7 the world's liquid rises (the H12 plane,
   `[0055EE3C]->[0]`, from the level state 0 saved, by one unit per 45 ms
   after 5 s, 1000 units by 50 s), and state 8 puts it back and sets
   `00546914`, which resets the brain to state 1 at its next update.
7. Phase 5 moves the director to state 5 (the helpers fly the route
   nodes the placement names) and then state 6, which 750 ms in ends the
   world with `00487560(1, 1, 0)` and sets the completion bonus (settings
   `+84 |= 0x40`, world `+5C6`; see
   [the completion bonus](PROFILE_RUNTIME.md#the-completion-bonus)).

## Capture node (selector 84)

The constructor sets `+138 = -1` (the linked actor's handle) and
`+13C = 0` (the capture clock).

- Command 16 (`+04`, `0043C2A0`) sets `+138` from its payload DWORD;
  commands below 16 go to the shared `0047ADA0`.
- Registration (`+1C`, `0043C110`): a node on team `0x12` adds one to
  `00547620` and sets flag `0x10000000` (`+2C`); any other team adds one
  to `00547624`.
- After load (`+34`, `0043C260`): `+138` takes the linked placement's
  handle (`0047F7B0`); a resolved actor gets the node's team (`+B4`,
  dirty), otherwise `+138` is the null handle.
- Contact (`+A4`, `0043C150`): when the partner's `+5C` answers an actor
  and the clock is 0, the node moves one count from its old team's word to
  the new team's, takes the partner's team, passes it to the linked actor
  and starts the clock at 1. Any answering partner clears flag
  `0x10000000`.
- Update (`+24`, `0043C0F0`): a running clock gains `delta >> 4`.
- Model registers (`+14`, `0043C200`): register 1 is half the clock,
  register 2 the clock less `0x20000`, register 3 the clock less
  `0x30000`, each 0 below its start and at most `0xFFFF`.
- Teardown (`0043C0C0`): the node leaves its team's count before the shared
  `0047AA60`.

Among the tables the port builds, only selector 12's `+5C` (`00404610`)
answers, with itself, so a node is captured by touching it with a
selector 12: a scientist the player beams down beside it, or a creature
the Cranial Implanter has taken. The lab's
brain-beacon-scientist
beams a scientist down 150 from node 0: in retail it walks to the node and
takes it within about six seconds (the aliens' count `00547620` falls from 6
to 5), and the port takes the same node with the same scientist. In the
Alien Brain, nodes 0 to 4 name placements 13 to 17, all gates (selector
30), so a node's capture hands its gate to the capturer's team; node 5
names none.

The port builds the nodes ([selector84.rs](../../crates/inf-game/src/object_runtime/actor/selector84.rs)):
check_selector84.py runs the six bodies over
160 cases and the port matches all of them, and a world test builds the
Alien Brain's six nodes, counts them, follows the five links and captures
one. With them built, the world's start shows a second hint while the
first pauses the game; the session now admits that repeated request for
phase 6, as `00438380` takes any request when none is pending and the
gameplay screen's `+1C` accepts 6 again. A world-19 sweep runs its 2400
frames.

## The brain (selectors 85 and 62)

Selector 62's constructor `00463F60` clears `+138`/`+13C`, sets `+14C` and
`+150` to 4000 and `+160` to 1; selector 85's `00405F10` adds `+178..+184`,
the model words `+190 = +192 = +198 = 0xFFFF`, `+194`/`+196 = -1 -
[0052ECA8]`, and zeroes the phase `00546910`.

Selector 62's update `00464050` cycles `+138` through 1 to 4 on the
durations at `00539808`:

- State 1 targets the player's unit when `+160` is set and it is within
  3000 (a `004825D0` follow target), else a fixed point (`0051F154`), and
  raises five ribbons (selector 67, `00486600`), which it swings round the
  target (`004869B0`) with the integer square roots `0048EFC0`.
- State 2 converges them, and each 500 ms builds a blast at the target
  (`00460310`, `004887D0` from the table `00539820`) and flashes the world
  (`00487D70`, `0xF00`).
- State 3 disposes the ribbons and the target; state 4 clears flag 8 and
  sets flag `0x40`. From state 4 a clear flag `0x80` starts again at state
  1; a set one holds state 4.
- The port [builds selector 62](CONSTRUCTOR_ASSETS.md#the-alien-brains-attack-selector-62),
  and Foldrak Noir's three match retail.
- Its light (`+28`, `004647E0`) splats a colour that fades with the state's
  clock (`0045B1A0`).

Selector 85's table `0051F9CC` is selector 62's but for `+00` (`00405F90`
over the attack's destructor `00463FF0`), `+14` (`00406630`) and `+24`
(`00405FC0`); the light and `+90` (`00464930`, which runs `+24`) are the
attack's. Its update steps `+184` (0 to 8) as above. `+17C` is the state's
clock: every update adds the delta, and a new state starts it at 0. In
detail:

- A set `00546914` comes first: the brain goes back to state 1 with
  registers 1 to 5 at `0xFFFF`, the speeds `+188`/`+18C` and `+160`
  cleared, and the flag taken.
- Below state 4, `+178` gathers the delta and registers 3 and 4 swing on
  its sine (`0053DF00`, the angle `+178 >> 5`): `swing - 0x445` and
  `-1 - swing - 0x444`, where swing is the sine times `[0052ECA8]` (0x444,
  which nothing writes) over `0x8000`.
- From state 4, a first draw (`0048F060`) above `0x6000` sets off a spark
  (`004665D0(1, point, velocity, +84, 0)`). Two draws place it within the
  radius WORD `+28` on X and Z, and its height is half the radius less
  the integer square root (`0048EFC0`) of their squared sum, not below 0.
  An odd first draw throws it at three draws of -600..599 each; an even one
  at -500..499, 600..799 and -500..499, and the point moves by the X and Z
  over 16. Both move it up by the Y over 16.
- State 0 saves the liquid's level (`[0055EE3C]->[0] >> [005483BC]`) at
  `+19A` every update; state 4 closes register 2,
  `(0x3FFFC00 - (clock >> 10) * 0xFFFF) >> 10`, until `0x100000` us; state
  5 pulls registers 3 and 4 toward `0xD000` with speeds that gain
  `((0xD000 - word) / 2 - 0x80) * (delta >> 5) >> 15` (clamped to
  `0xD000..0xFFFF`), register 1 following their mean, `0xFFFF` at `0x2000`
  above the floor, and once it is 0 and both are down, state 6; state 6
  sinks register 5 by `delta >> 7`.
- The model registers (`00406630`) are the WORDs at `+18E` plus twice the
  register: 1 to 5 are `+190..+198`, 6 the saved level `+19A`; register 0
  is `+18C`'s high WORD, and past 6 the object ends.

The port [builds the brain](../../crates/inf-game/src/object_runtime/actor/selector85.rs)
as selector 62's state with the brain's fields beside it, and runs its
update in place of the attack's.
check_selector85.py runs the constructor, the
registers and the update over 73 cases with the real RNG, square root,
sine and the vehicle's `+58`/`+D0`; the port matches all of them, and four
mutated constants each fail it. A world test over the Alien Brain builds
it, saves the level, wakes it by capturing the six nodes, sheds debris,
raises the liquid and restarts it through the production world tick.

### Aiming without a unit

The brain clears `+160` when it wakes, so its attack never targets the
player: entering state 1 it takes the fixed point at itself and aims the
ribbons' cross along `([esp+18], EBX)` (`004642B4`). Nothing on that path
writes `[esp+18]`, and EBX is the update's delta. The lab's
`brain-attack-aim.json`
and its 16 ms twin play the ending in retail from the lock on
(tampered-run: the aliens' node count emptied, then the lock's own writes
made). Helper `+164` arrives 13 s after the lock, which sets phase 2, and
the attack ends 5 s later in phase 3. The `attack_aim` site logs the read:
the slot held `0x2801114C`, the world object `[0055F160]`, which the camera
lookup `00478E00` pushes (its ECX) earlier in the same tick, beside its
call's return address `00478E2D`. The offsets came out `(2356, 4000,
-3231)` at the 125 ms step and `(1066, 4000, 3855)` at 16 ms: the slot's
low WORD `0x114C` against the delta's. Retail's own heap puts the world
object anywhere, so on a player's machine the cross turns by an arbitrary
angle. The port takes the lab's WORD (`BrainRibbonAimApproximation`, logged
once). Until 2026-10-22 the port held the attack there instead, which
froze the ending at phase 2: the camera rested on the helper and no ribbons
came. A placed attack (Foldrak Noir's) reads the same slot one call
shallower whenever no player's unit stands in lifecycle 3; no run has shown
its leftover there, so the port still holds it.

## The director (selector 78)

Its table `0051FA9C` is the shared actor's `00527C00` but for `+00`
(`00406730`), `+04` (`00406800`), `+24` (`004072A0`), `+44` (`00407340`,
which runs `+24`) and `+88` (`00444AB0`, which lets a player take it). Its
`+138` is a creation listener (`0051FA98`, `00407350`) on the list
`0054E878`, which `00460120` walks for every admitted actor; the
destructor `00406750` unbinds it. Command 16 keeps its operand's address
at `+154`: three one-based route-node numbers (`00404F00` finds node
`n - 1`). `+16C` starts as the process null, and `+15C` is unwritten until
state 3.

Its update `004072A0` runs the ending `00406830` as a timed state machine:
the delta in milliseconds is `delta >> 10`; while the state's clock `+158`
is at most that, the state ticks for what is left of its clock
(`00406830(2, clock)`, when positive), is left (`(1, 0)`), `+160` advances
and the next is entered (`(0, 0)`, which sets the clock to 1000 before the
state's own entry). Then the state ticks for the rest (`(2, ms)`) and the
clock falls by it. It then updates helper `+168`, then `+164` (their
`+24`). Until the take the update list runs it; from then on the player's
update runs its `+44` (or `+24` below lifecycle 3).

1. State 1 holds its clock at 1000 until phase 1 (a 500 ms frame still
   leaves it). Leaving it, it finds route nodes 0 and 2, the first player
   (`00488AA0`) and the camera (`00478E00`), and builds two helpers
   (`00462750`); with any of these missing it disposes itself. The first
   player takes it (`0046AE90`), their unit (`0046B030`, its `+D4`) gains
   1000 health up to its `+A2`, helper `+168` starts at the unit and flies
   route 2 (speed 0, approach 75), helper `+164` starts at route 0 and
   flies it (speed 20, approach 150) looking at `+168`, and the camera
   follows `+164` (`+184 = 1`, parameter mode 0).
2. State 2 holds its clock at 1000, `+164`'s speed gaining half the
   milliseconds and `+168`'s a quarter, each up to 1000. Once `+164` has
   arrived in phase 1 the phase becomes 2 (the brain's attack); the state
   ends in phase 3.
3. State 3 shows the aliens' units dying. Entering it sets the victim clock
   `+15C` to 500. While the victim `+16C` lives, its first tick sets off
   five sparks round it (`004665D0(1, ...)`, owned by the victim: within
   50 of it on X and Z; an odd first draw throws them at three draws of
   -600..599, an even one at -500..499, 600..799 and -500..499, moving the
   point by the X and Z over 16; both move it up by the Y over 16), and past
   500 ms a victim still in lifecycle 3 is retired (`0047CE90`). With none,
   from 1200 ms the actor list (`G+48`) is walked for a unit with flags
   `0x24`, team `0x12`, lifecycle 3, a placement `+2A` of 0 or more, a
   `+58` that answers and not the director itself: one whose `+58`'s `+CC`
   answers (the creature families, `00404610`) is disposed unseen; the
   first other one becomes the victim, the camera follows it (`+184 = 0`,
   mode 0) and the clock is set to 2000. World 19's victims are its eyes
   (selectors 77 and 79), its creatures (resource 95) vanish.
4. State 4's entry has the camera follow `+164` and look at `+168`
   (`0040F710`, `+184 = 1`, mode 1) and sets phase 4 (the brain opens);
   the state ends in phase 5.
5. State 5's entry sends `+168` (the bug) along route 1 (speed 50), draws
   it (`00479E90(1)` and flag `0x800`), sends `+164` from two nodes on
   (speed 0, approach 50) looking at the bug, and has the bug look at the
   route's last node (a type-16 target, `0051FB6C`). Each tick the bug
   gains half the milliseconds up to 1500, and `+164` a quarter up to 1600
   once the bug is past 600 (else it stops); while the bug has not arrived
   the clock holds at 600. With one or two nodes left after the bug's
   current one, `+164` moves to the last and arrives, and the camera looks
   at the one before it.
6. State 6 sets its clock to 1750; once it is below 1000 each tick ends the
   world (`00487560(1, 1, 0)`), sets the completion bonus (settings `+84 |=
   0x40`) and the world's `+5C6`, which only has `004875E3` repeat the
   same OR every frame. Later states do nothing.

The listener (`00407350`) acts from state 5 on: when the actor announced
has helper `+168`'s handle, it sets both helpers' `+140` (arrived) and
clears `+164`'s look-at target (`00462D40(0)`). A gate's pass announces
the actor that crossed it again (`0048C2E9`), so the bug flying into a
gate ends state 5 early.

The port [builds the director](../../crates/inf-game/src/object_runtime/actor/selector78.rs)
and runs it from the update list and the player's update.
check_selector78.py runs `004072A0` over 88
cases with the real RNG and selector 90's own `00462BA0`: sparks round a
victim, and runs of updates through states 2, 3, 5 and 6 on; the port
matches all, and six mutated constants each fail it. Two world tests
over the Alien Brain play the ending with the production updates,
standing in for the brain's phase steps: one through the take, `+164`'s
flight (phase 2) and every alien unit's death, its eyes (selectors 77 and
79) each dying into their remains until state 4; one with the aliens'
units gone, through the opening, the bug's flight and the world's end with
the bonus. A third runs the brain and the director together, from the six
captures and the vehicle's lock to the world's end: phases 1 to 5 in order
and the attack's states 1 to 3.

### The eyes' death

The eyes' `+AC` (`00408560`, both `0051FBB0` and `0051FC8C`) runs a line of
bursts down the stalk: from the eye to its root `+192`, one every 50 units
of the stalk (5 to 100; a squared length past 2^31 is rooted a quarter at
a time and doubled, `00408601`), each at the eye plus its share of the
stalk and of types 2, 3, 6 and 9 in turn (`00489D90`, rising at 50). Its
`+D8` (`00408310`) finds the stalk's tip: a copy of the eye's basis turned
by its WORDs `+18A`, `+188`, `+18E` and `+186` (`00491530` and `00491290`
alternately) carries (0, 0, the stalk model 0x2BE's header WORD `+0E`) up
from the root (`00490B20`), or with that model not resident the tip is the
root. Sound 0x28 plays there (`00487F10`, `004923C0`), its child factory
`+4C` (`00407E70`) leaves resource 0x89 there turned by the same basis, the
child takes flag `0x400000` (woken) and the eye is disposed.

Resource 0x89 is selector 82 (case 0x52, `00409890`, table `0051FE88`, 0x14C
bytes): a boulder (selector 59, `00405780`) with a 5 s timer `+148`. Its
update `004098C0` is its only own method: once the delta exceeds the timer
it bursts (`0048A8B0`: 16 debris of type 5 round it, central type 1, speed
8), plays sound 9 and is disposed; otherwise the timer falls. Either way
the boulder's update follows, so the woken remains fall and roll.

The port [builds both](../../crates/inf-game/src/object_runtime/actor/derived/selector77/death.rs).
check_eye_death.py runs `00408560` over 48
eyes with their own `+D8` and `+4C`, the real square root, rotations,
transform and sine table; the port matches every burst, the sound, the
child's position, basis and flag, and four mutated constants each fail it.

### Struck at the tip

What kills an eye is a thrown weapon at its tip. The eyes' `+A4`
(`00407BB0`) takes a partner whose `+68` answers (a grenade, charge, mine,
rocket, missile, the Cranial Implanter or the Bomb, all `00404610`) from
the eye's state 2 on: the partner within 500 of the tip (`+D8`, the rooted
`0048EFC0` as the death's), with the tip within 150 of the partner's own
axis (the tip in its frame, `0047A660`, `x*x + y*y < 0x57E4`), leaves its
`+88`, its thrower, at `+1B0`. The eye's next update (`00407CA0`, selector
79's `00408AB0`) then deals itself 10,000 damage on channel 9 from that
source through its `+B4`, and dies. The port held `00407BB0` as a typed
boundary until 2026-10-21, so a rocket or the implanter only bumped an eye
(as reported); it now
[ports the arm](../../crates/inf-game/src/object_runtime/actor/derived/selector77/contact.rs).
check_eye_strike.py runs `00407BB0` over 96
eyes and partners (the tip and the partner's frame natively, partners on
the grenade's answering table or the shared actor's silent one), and the
port marks the same 19 eyes with the same throwers.

## The others

- Selector 79, the eyes, is
  [built](CONSTRUCTOR_ASSETS.md#world-19s-eyes-selector-79): five pairs
  (placements 6/7, 8/9, 51/52, 89/90, 94/95), each eye naming the other
  (command 19), strike what crosses between them.
- Selector 81, the spawner, is
  [built](CONSTRUCTOR_ASSETS.md#world-19s-spawner-selector-81): it raises
  selector-80 eyes out of the liquid round a slow player. Its first
  attempt reads a stack slot the update never writes; the port starts it
  at 0, as retail's reads did in the lab until eyes updated just before it.

## In the port

The capture nodes, the brain (selector 85) and its attack (selector 62),
the eyes (selector 79) with their death and remains (selector 82), the
spawner (selector 81), the director (selector 78) and the
[sinking ground](CONSTRUCTOR_ASSETS.md#the-sinking-patch-selector-83)
(selector 83) are built, and so are the spawner's members
([selector 80](CONSTRUCTOR_ASSETS.md#world-19s-members-selector-80)).

`00546910` and `00546914` are process words, which the port keeps in the
object arena beside the capture nodes' counts: a retained load carries
them from world to world as retail's process does, and only the brain's
constructor resets the phase.
