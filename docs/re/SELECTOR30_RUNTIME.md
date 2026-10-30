# Infestation selector 30, the warpgate, and world start position

Selector 30 is the warpgate (model 62, `gate`). A gate either ends the world
for the vehicle that crosses it (00487560 to its world `+15C`) or, linked to
another gate (`+180`), moves the vehicle there within the world: the
surface-bound warpgate.

This document owns the `0048B170 -> 0040D800 -> 0047A7A0` construction
chain and its world-start side effects, the update `0048B3F0` and the pass
`0048BE70`. Shared registration, payload and handle lookup belong to
[ACTOR_RUNTIME.md](ACTOR_RUNTIME.md). This selector does not inherit
`00425E60` and consumes no constructor RNG draw.

The implementations are [selector30.rs](../../crates/inf-game/src/object_runtime/actor/selector30.rs),
its [update](../../crates/inf-game/src/object_runtime/actor/selector30/update.rs)
and [pass](../../crates/inf-game/src/object_runtime/actor/selector30/pass.rs),
the world's [services](../../crates/inf-game/src/session_platform/world_actor_update/selector30.rs)
and [world_start.rs](../../crates/inf-game/src/world_start.rs). They admit
fresh mode-0 construction, commands, postinitialization, network initialization,
after-load, the update with its persistent sound record and the pass.

## Constructor and payload

`0040D800` sets byte `+138=1`, DWORDs `+13C/+140=0`, WORD `+144=FFFF`,
bytes `+14C/+14D/+14E=0`, DWORD `+150=0` and sound pointer `+154=null`.
The three WORDs at `+146` and DWORD `+158` remain unknown. Selector 30 adds
its own initialized scalars, null handle `+184`, and final vtable `005289C0`.
Position `+16C` and saved selector `+188` remain unknown until a later writer.
The prefix state is the shared `Actor40d800State` of the
[0040D800 family](FAMILY40D800_RUNTIME.md), which selectors 37 and 39 also embed.
The [exact full8 model callback](OBJECT_MODEL_CALLBACKS.md#selector30-exact8-capture)
now owns fresh16C/16E/170 capture through unsigned439E10; its default terrain
arm and later simulation remain separate boundaries.
The constant copied into `+174` from `00526068` is zero.

Context/payload `0048B390 -> 0048B3C0 -> 0040D950` executes the rotating
context and shared actor payload before reading these additional H13 fields:

| Payload offset | Read | Destination |
| --- | --- | --- |
| `+44` | signed DWORD | `+13C`, then signed clamp of `+140` |
| `+48` | exactly three WORDs | `+146..+14B` |
| `+50/+54` | full DWORDs | `+15C/+160` |

The clamp tests `current < 1` first, assigning zero; only its other branch
tests `current > maximum`. A nonzero maximum with zero current sets actor
selector `+B4=0` and dirty WORD bit 2. Payload gaps remain uninterpreted.

## Instance commands

`0048BBF0` delegates codes at most 20 to `0040D9C0`, which delegates codes
at most 15 to `0047ADA0`. Negative codes and codes above 32 reject without
reading an operand. The additional reads are:

| Code | Operand and effect |
| --- | --- |
| 16 | signed BYTE to DWORD `+13C`, then clamp current `+140` |
| 17 | BYTE boolean to `+14E` |
| 18 | BYTE boolean; when true add maximum into current, clamp, and dirty `0200` if the addend is nonzero |
| 19 | unsigned WORD to `+144` |
| 20 | exactly three WORDs to `+146` |
| 21 | unsigned WORD placement reference to DWORD `+180` |
| 22/23 | unsigned BYTE to DWORD `+15C/+160` |
| 24 | unsigned BYTE to WORD `+164` |
| 25/26 | raw BYTE to `+18F/+167` |
| 27/28/29/30/31 | BYTE booleans to `+18E/+166/+190/+191/+192` |
| 32 | unsigned WORD to `+18C` |

These commands retain no operand pointer. The DWORD constructor copies and
BYTE command overrides are intentionally different widths.

## World start query and postinitialization

`0048DAF0` reads the world owner's raw WORD `+A4`, doubles it with WORD
wrapping, and interprets the result as a signed H1 slot. The resource DWORD
contains low-WORD X and high-WORD Z. A nonzero owner byte `+A6` instead
selects stored override X/Z at `+A8/+AC` and skips H1 entirely.

Both branches recompute Y from the current **raw** H12 cell: signed height
shifted by the explicit vertical scale, plus 60, narrowed to a WORD. This
does not call interpolating sampler `00450A30` and does not preserve override
Y. Missing/unloaded H1, negative slots and the native one-past boundary are
explicit errors; no lazy resource load is fabricated.

Postinitializer `0048C4F0` owns this sequence:

1. If byte `+166` is set, grant WORD `+7A` is neither zero nor 72, a first
   player exists, and its real grant mask contains that grant, clear `+166`
   and WORD `+164`.
2. Copy current actor selector `+B4` to `+188`.
3. Query the current world start. Subtract its XYZ from actor XYZ with signed
   WORD wrapping, square each difference and sum with DWORD wrapping.
4. Only if the unsigned sum is below 1,000,000, publish an override. A
   nonzero actor byte `+18F` writes exactly one **BYTE** at world `+1C4`.
   Copy XYZ, adding the low WORD of native `__ftol(basis[7] * -450.0f)` to
   X and the corresponding `basis[9]` result to Z. Preserve actor Y in the
   stored copy. Set world `+256=1`, store XYZ at `+258`, dirty actor bit 2
   and set actor selector `+B4=2`.

It does not call `0047AAD0` or `0040E000`: it omits the former's extent-to-`+74`
copy and the latter's `+B4`/dirty update. One mutable `WorldStartState` is shared across
factory calls so earlier overrides affect later actors. The next query still
recomputes Y from terrain.

### The starts and their gates

Each start stands by a gate, which that postinitializer hands to the player
when the world loads there. Terra Nova (world 2) and Oolbryte (world 20)
have three starts each:

| World | Pair `+A4` | Start | Gate (placement) | Its world and start (`+15C`, `+160`) |
| --- | --- | --- | --- | --- |
| Terra Nova | 0 | (8200, 12000) | 8 | 18, 2 |
| Terra Nova | 1 | (31200, 31200) | 19 | 20, 1 |
| Terra Nova | 2 | (33536, 55040) | 97 | 20, 1 |
| Oolbryte | 0 | (30122, 23525) | 75 | 2, 2 |
| Oolbryte | 1 | (16033, 8866) | 72 | 4, 1 |
| Oolbryte | 2 | (48460, 37632) | 4 | 2, 3 |

Level 1's gate brings the ACAM to Terra Nova's first start. On that first
visit the only way on is gate 19. It starts on the aliens' side (command 6
with 2), will not let the vehicle leave while an alien lives (`+190`),
completes Terra Nova (`+167 = 1`) and grants `0x84`, a record with no text.
Terra Nova's main gate 97 and its northern factory 98 both carry shared
command 13 with `0x84` (`0047AF05`: unless the first player owns it,
`00479FA0` disables the actor), so before gate 19 neither stands. Infest02's
first Terra Nova load shows exactly that: at `4F40CF`, `00479FA0` runs from
`0047AF2C` on placements 97 and 98.

Gate 19 leads to Oolbryte's first start, by gate 75, and gate 75 leads back
to Terra Nova's second start, by gate 19. Oolbryte's gate 4, far to the
east, leads to Terra Nova's **third** start, by gate 97, and no other gate
in any world's placements does: that is the hidden base. Once the player
owns `0x84`, gate 97 and the factory stand there. The factory's shop holds
`0x9B` (its `+7A`), `0x96` "Dual fire cannons" and `0xA8` (its commands 23
and 24): that is the base's cannon upgrade. [A
test](../../crates/inf-game/src/world_start_tests.rs) loads Terra Nova
both ways. The first visit has gate 19 alone. The third start, with
`0x84` known, puts the ACAM 450 in front of gate 97, and the factory's shop
holds all three.

Gate 4 hangs over the slope of a deep pit: its ring stands in the plane
x = 48638, about 1000 above the pit floor, and only its lower edge comes
near the ground, at the gate's foot. A run from the live save's Oolbryte
profile places the buggy (a temporary harness) 78 in front of the ring's
centre, rolling through at 700. The gate ends Oolbryte for world 2, start 3,
and after the results Terra Nova loads with the ACAM 450 in front of
gate 97. Gate 97 and the factory stand, since that player knows `0x84`.

The loader reads the start from the mission's `+A4`, which the save reader
(`0048ECC0`) and a gate's end (`0048E820`, `+160` less one) write. The port
took it only when it imported a save, so a menu load or a gate to a later
start arrived at the first; [the loader](../../crates/inf-game/src/world_loader.rs)
now takes it on every load (`a_saved_second_start_places_the_vehicle_there`).
Retail wrote such a save: the fourth profile of the TTD copy's `Infest.sav`
left Oolbryte by a gate (world 20 status 1), with selection 2 and
`+A4 = 1`, which only gate 75 gives.

An unmodified run from the live save's Oolbryte profile (slot 6) takes gate
75, back to Terra Nova's second start, on the game's keys. After the menus (Return at 10 and 100, Down at 180,
200, 220, 240 and 260, Return at 300, Down at 420 and 430, Return at 440),
Down 470-540 reverses the ACAM from the start into the gate, which ends the
world for world 2, start 2. The results read "Mission Incomplete" (frame
520), as gate 75 has no `+167`, and Space at 545 loads Terra Nova. The ACAM
arrives at (31544, 31407), 450 in front of gate 19 (frame 600; a build that
only adds logging renders it pixel for pixel the same). The loader as it
was took the same keys to (8203, 11912), by the gate back to world 18.

## After-load and lifetime

Virtual `+34=0048B230` reads the selected world's four completion DWORDs via
`0048EA80`, using signed actor byte `+78`. Integer division truncates toward
zero: indexes -31 through -1 consequently address word zero, with masked
x86 shift counts. A set bit changes `+B4` to 18 when saved `+188` equals 2,
or to 2 otherwise, and sets dirty bit 2. A nonzero raw `+180` is resolved
through `0047F7B0` into handle `+184`. `ObjectAfterLoadInputs` requires the
actual selected four words; there is no all-zero default.

The enclosing `+20=00401320` sets flag `00800000` only after that callback
succeeds. Network `+40` is exactly the shared `0047E2E0`. Construction leaves
sound `+154` null; the update creates it, and destruction `0040D870` releases
a live record through `00487FE0` before `0047AA60` unregisters and unlinks.

## Update

[`tick_selector30`](../../crates/inf-game/src/object_runtime/actor/selector30/update.rs)
owns virtual `+24=0048B3F0`. A set byte `+166` with a nonnegative `+164`
publishes it as WORD `G+1D8` (the mission's `W+28`) with `0x119` at `G+1DA`
(`W+2A`) at `0048B411`, and the update goes on as below. No recording
reaches it; the native receipt does (worlds 11 and 15 have such gates). The
gameplay draw shows them as "Warp power hoops: N" while no message box is up
([world captions](GAMEPLAY_RUNTIME.md#tick-and-draw-first-pass)); the
loader clears `W+28`. Then `+178=0` and
`+17C=0x4B0`; a positive `+164` stores the process NULL `0054E894` in
`+158`, clears `+168` and goes straight to the sound.

The scan point is the position plus half the size along basis Y, each
product truncated by `__ftol`. Two steps follow it.

1. **The crossing.** The actor the gate tracked last update (`+158`,
   resolved by `0047AD90`) is tested first (`0048B4EE`). The residues the
   intro trace shows there (`3F4B0000`, `3ECA0000`, `12`) never resolve.
   Within `2*(size/2)^2` of the point, `0047A660` turns its offset into the
   gate's frame. The actor crosses if it is inside the ring (local X^2 + Y^2
   below `(size/2)^2`) and no longer in front of it (local Z at most 0). A
   selector-12 actor of a `+18E` gate crosses from anywhere that near.
   A crossing still has to clear two checks:
   - With `+190`, a live alien (team 18, `+C4` at least 3, not built from
     resource 15, without flag `0x8000`) blocks it. `0047DD80` then sends the
     actor message 4 with H0 string `0x161`, "You must not leave until you
     have cleared this world of aliens". If no alien is left, `G+1C6 = 1`.
   - With `+18C` and `+191`, the first player must own grant `+18C`
     (`004752E0`). Otherwise the player's message (`004135D0`) gets `"%s: %s"`
     of H0 `0x164`, "Before warping, you need the following information", and
     the grant's name (its record WORD +0).

   Otherwise the actor passes (`0048BE70`). Whether it crossed or not, `+158`
   then returns to the process NULL (`0048B70A`).
2. **The scan.** The G+48 list is scanned for flags `&0x888=0x800`,
   `+C4>=3`, only team 18 for a team-18 gate, and with `+18E` only actors
   whose virtual `+5C` answers (selector 12). The nearest square below
   `0x15F900` (1200 units) wins. A candidate within `2*(size/2)^2` of the
   point is turned into the gate's frame (`0048B810`). Inside the ring and in
   front of it (local Z above 0), nearer the face than any before (from 800),
   it is tracked at `+158` (`0048B87F`). For a selector-12 actor of a `+18E`
   gate the ring is 9/8 as wide, and either side of the face counts.

Without a find, `+168=0`. With one, `+17C` is its root minus half the size
(floor zero). Offline, the first find (`+168=0`) after more than 2000 ms of
mission time `G+1E8` names the destination on the found actor's player's
message (`0048B91F`). The mission tick `0048E2E0` adds each capped delta's
whole milliseconds to `G+1E8` first. A world gate formats H0 `0x15F`, "Warp to %s", with
the world's name, H0 `0xE0 + world` ("Warp to Terra Nova"). A gate to no
world shows H0 `0x160`, "Surface-bound warpgate". Then `+168=1`,
`+178=(0x15F900-+17C^2)>>13`, and `+174..+176` blend the colours
`0048B120` stores once (green `20 50 20`, blue `20 20 60`) by
`(clock*4)%256` in two 128-step halves, keeping `+177`.

The sound tail then creates sound `0x1A` at the gate (`00487F60`, flag 0)
while `+168>0` and `+154` is null, updates it (`00487FC0`) when present, both
with volume `((1500-+17C)<<16)/1500` from `0053D980` and pitch `0x10000`, and
releases it (`00487FE0`) once `+168` is zero.

## Pass

`0048BE70` takes the crossing actor and does the following.

1. A remote actor (flag `0x80000000`) is left alone.
2. The actor's player (`0046B090`) takes the gate's grant `+7A`
   (`004750C0`). Every recorded gate has 72, which the player always owns.
   Terra Nova's completing gate has `0x84`, whose record has no text
   (flags 2), so its grant says nothing; it raises the hidden base (see
   [the starts](#the-starts-and-their-gates)).
3. The actor's `+64` (the vehicle answers itself) drops every item in its
   hold `+1A4` built from resource `0x31`, through the hold's `+0`. It reads
   the count again after each drop.
4. **A linked gate** (`+180`, its handle `+184`) moves the actor. A lost
   link is forgotten (`+180 = 0`). Online (`0055F448`) a gate whose
   `+184` is still the NULL (a client's gate, built from the host's
   record, never ran the load's link) walks the actor list (`G+48`) for
   the first actor whose placement WORD `+2A` is `+180`'s low WORD less
   one (`0048BF29`), and `+184` keeps its handle. With none, the walk ends
   holding the last actor it read, and the actor moves there; an empty
   list would leave a stack word nothing wrote (`0048BF93`), which the
   port stops on, but the crossing actor is always on the list.
   - The gate's basis is copied and turned half a circle by `00491530`
     (`0x8000`).
   - The actor's offset from the gate goes into that frame (`00490A80`),
     10 further along Z, and out through the linked gate (`0047A6C0`).
     Added to the linked gate's position, that is where the actor lands.
   - The velocity `+8E` turns the same way, and `0047B510` adds the
     difference. The spin `+94` turns through `00490870` and `0047A6E0`,
     but `0048C1ED..0048C1F9` store it through EBP, which holds the gate
     (`0048BE89`): the gate's `+94..+9C` take the turned spin, and the
     actor keeps its own.
   - The basis goes through `00490DC0` and `00490BC0`, the latter
     renormalizing past 100 steps.
   - A selector-12 actor gets `+22E |= 0xC`.
   - For a player's actor the world flashes at the gate (`00487D70`,
     radius `0xF00`, style 0), the camera snaps (`00478E00`, `+187 = 1`),
     and the linked gate is marked used (`+38 |= 2`, `+B4 = 2`).
   - Last, the creation listeners hear of the actor as if it were new
     (`00461BE0`, `00461C00`, `0048C6A0`).
5. **Any other gate** ends the world for a player's vehicle. Anything else
   that crosses is disposed.
   - A nonzero `+167` first marks the current world's status `G+1EC[world]`,
     unless bit 2 is already set. With `+167 = 1` and the player owning
     `+18C` (or no `+18C`), it sets bit 2. Completing world 4 that way also
     grants the BUGGY RACING cheat (settings `+84 |= 0x80`). Any other
     `+167` sets bit 8.
   - Then `00487560(+15C, +160, actor)` completes the world and requests
     the results, phase 8 (see
     [world completion](../../crates/inf-game/src/world_completion.rs)).

### A scientist takes a gate `0048BD90`

A gate's `+CC` (`0048BE30`) answers 1 to a scientist of another team (or
to any, with `+18E`), so a scientist the vehicle drops near an enemy gate
picks it as its target and walks to it. When it touches the gate, the pair
pass's scientist arm (`00468160`) reaches the gate's `+D0`, `0048BD90`:

1. Unless the gate is held (`+192`) or already on the scientist's team, it
   takes the scientist's team (`+B4`, dirty 2). Its scan then no longer
   keeps to team 18, so it tracks the player's vehicle, names its
   destination and lights up.
2. The world's record of the gate's placement (`+78`) follows its side:
   set (`0048EAD0`) while its team differs from its first (`+188`) on the
   alien side, cleared (`0048EB30`) when it returns. The after-load
   (`0048B230`) restores the flip from that bit.
3. A `+18E` gate then passes the scientist (`0048BE70`). It answers 0.

[The port](../../crates/inf-game/src/object_runtime/actor/selector30/pass.rs)
runs the entry from the [pair pass](WORLD_CONTACT.md), the `+18E` pass with
the update's own services, and the scientist's refusal bit (`00468191`,
`+22E |= 1`) after the gate answers 0.

The port's Pelagia option ([`pelagia.rs`](../../crates/inf-game/src/pelagia.rs)) adds one gate to
this: the Command Centre's dead start gate (placement 81, `+164 = 1`, no
destination) answers `+CC` with 1 to a scientist outside team 0x12 while
its lock holds, and the scientist's arrival sends it `0x15` (the lock to 0
and the first player's silent command 9) and gives it world 1, start 1,
before retail's take. Its pass then loads world 1 as the Teleport does,
never through `00487560`. Off, and in the lab, it is retail's.

Only Altair Prime's gate has `+18E` (world 5, placement 27 at (47407,
30141)). Its one-based link 17 names placement 16, the terrain bridge
(selector 37) at (51736, 31220), with one staff (command 16) and a strip of
3800 by 384 running -X (commands 21, 22 and 27). A scientist the vehicle
beams down near the gate walks into it, and the gate lays its offset out
about the bridge, on the tower: retail's crossed at (47215, 159, 30208) at
speed (60, -1, -2) and landed at (51664, 95, 31420) at speed (37, -1, -47)
(tampered-run: `runtime_re/lab/scenarios/altair-drop-sci.json` (a constructed save with a scientist in the hold, the vehicle teleported beside the gate), pass 279, the lab's `gate_pass`
site). There it walks into the
bridge, whose `+D0` takes it ([`0040DAB0`](FAMILY40D800_RUNTIME.md#staffing-0042f170-0040dab0-beam-down-0042f670));
with its one staff the bridge stops holding, sounds 0x17 after a second and
reshapes the strip over four seconds. At the rise's end it sends the
scientist out again at its `+146` offset, 500 along its X, on its team
([`0040DCB0`](FAMILY40D800_RUNTIME.md#the-bridges-update-00409a20)): retail's
came out at (52236, 96, 31220), 39 updates after the take. A second
scientist reaches the tower too, and the full bridge refuses it. [A
test](../../crates/inf-game/src/session_platform/intro/runtime/tests/contact_tests.rs)
runs it over the loaded world: from retail's crossing the scientist lands
where retail's did, the bridge takes it, and the bridge's own update sends
it out where retail's came out. In a live run from the harness
(`"world": 5`, `"hold": [2, 2, 2]`, D at frame 20 beside the gate) the
gate's update passes the dropped scientist, the bridge takes it and its
rise starts. Enemy gates
(command 6 with 2, team 18) stand in worlds 2 to 7 and 11. [A
test](../../crates/inf-game/src/session_platform/intro/runtime/tests/contact_tests.rs)
lets a scientist that targeted Terra Nova's gate to world 20 take it for
team 2 and mark its placement, then a mutant take it back and clear the
mark. `check_world_interactions.py`
runs the original `0048BD90` with the real `0048EAD0`/`0048EB30` over 64
supplied gates and missions (held or not, `+18E` or not, every team pair,
indexes from -128 to 127, the inhibit byte, three worlds): the port's team,
dirty byte, completion words and pass match every one. A live run that beams a scientist down 500 from that gate shows it
walk in about 4 s later; the gate, dark before, lights with its blue arcs.
That run placed the vehicle with a temporary harness. The gate stands on a
plateau walled by slopes steeper than 30 degrees: a ditch on the west and
north, steep banks east and south. An unmodified drive from the Terra Nova
save reaches the ditch below it. A scientist dropped there, 1870 from the
gate (inside its 2500 scan radius), only slides along the ditch floor.
Tabuchi and Artoria hold no enemy gate.

Helniazine's start pad is fenced in, and the fence comes down as Infest04's
did once the port has the fence chain `004373B0`
([TERRAIN_RUNTIME.md](TERRAIN_RUNTIME.md#the-fence-chain-004373b0)). An
unmodified run from the Helniazine save then takes the mutants' gate on the
game's own keys. After the menus (Return at 10 and 100, Down at 180, 200
and 220, Return at 300, Down at 420 and 430, Return at 440) it does this:

1. A laser mine (X 492-495) comes to rest beside the fence, and the
   detonator (F7 at 527, Z at 547) sets it off. The fence segments chain
   down, and the pad is open.
2. A drive along Infest04's route north-east to the gate follows. The keys
   come from an autopilot that only presses keys.
3. Down at 1160 stops the buggy about 370 from the gate, and D at 1172
   beams a scientist down. At tick 745 the scientist walks in, and gate 73
   turns from team 0x12 to team 2.
4. Up 1240-1256 carries the buggy south past the gate, and Down 1256-1300
   reverses it back north through the gate. At tick 824 it warps from
   (48069, 34148) to the linked gate at (27186, 23078).

The clean build's frame 1195 shows the gate dark with the scientist walking
off, 1240 the gate lit with its arcs, and 1290 the buggy at the lit linked
gate under "Surface-bound warpgate". A build that only adds logging renders
frame 1240 pixel for pixel the same. On the far side, the plateau with
Helniazine's outside mutants is reached by one ramp, and its turrets
destroy the buggy there; Infest04's buggy was destroyed on that plateau too.

Infest04's own drop and takeover also replay in the port.
`scientist-drop.js` and
`gate-warp-run.js` read the buggy's updates and the scientist's around them.
A temporary harness writes the buggy's drive state from a snapshot at
`7AA3E9:BAB`, two updates before the drop, and then writes retail's turn,
pitch and thrust each tick. Some of those are part-tick holds, such as a
10 ms turn, which 125 ms key frames cannot press. D on the recorded tick
does the drop, and the run keeps the retail position step.

- The buggy matches every update.
- The scientist lands at retail's (47980, 64320, 34221) on the same update
  and settles at (47979, 64329, 34222).
- The gate turns team 2 on the same update as retail's `7AFEF5`.
- The scientist's next two updates match, the gate's pushes included,
  down to the basis words. Retail runs the scientist before the buggy, so
  its sample at an update is the port's at the next tick.
- Its third update, the walk away from the gate, does not match. Retail's
  heads +X+Z at (42, -15, 29), the port's -X-Z. That stays so even with
  retail's RNG word before each scientist update and its leftovers `+19C`
  (`0C904924`) and `+220` (`8098`) supplied; the pair calls draw no RNG.
  At that update's start the descriptor (`+184` 5), its time `+188`,
  `+19C`, `+21C`, `+220` and the targets `+26C` (the buggy) and `+274`
  (the gate) agree; byte `+22D` differs (retail's 4), but nothing in the
  port reads it. The grant target `+27C` does not: retail's holds the ECM
  Unit (placement 161, selector 73, grant `0x80`), which the recording's
  buggy had carried and left by the gate, and the port's is empty, because
  the port builds no selector 73 yet (its load skips placement 161). In
  the update retail moves to descriptor 8 and makes `+274` its target
  `+148`; the port moves to descriptor 7. The ECM Unit's class is what that
  update still waits on; [hold items](HOLD_ITEMS.md#ecm-unit-selector-73)
  has its decompiled class.

Infest04 records the takeover itself, on Helniazine. `gate-entry.js` finds
255 entries. At `7AFEF5:1B5B` the player's scientist (team 2, at
`(47983, 64317, 34222)`) walks into the mutants' gate `363AE00` (team 0x12,
first team 0x12, index 6, at `(48054, 64320, 34129)`). The gate takes team 2
with `0048EAD0`'s mark and answers 0; one later entry finds it already
team 2. Earlier, at `6A61E0:861`, a mutant takes gate `363D260` (team 1,
index 4) as in Infest03, and its other 252 entries change nothing.
`gate-light.js` reads the taken
gate's updates. Before the entry, with the vehicle beside it, the gate finds
nothing: `+168` 0, colour `+174` 0, light `+178` 0. From its next update at
`7B069E:19B8` it finds the vehicle: `+168` 1, the light at 175, the colour
blending each update (`152410`, `112710`, ...), and its hum created. It has
lit up for the player. The port's entry gives the same for all 255
(`gate_entries_match_infest04`).
Infest03 records the same entry the other way round:
`gate-entry.js` finds 112
`0048BD90` calls, the first a mutant scientist taking gate `037DCC20`
(team 1, index 4) for team 0x12 with `0048EAD0`'s mark at `2BAF6F:1DB1`,
the other 111 finding it already the mutants' and changing nothing; every
one answers 0. The port's entry gives the same for all 112
(`gate_entries_match_infest03`).

### Port boundaries

- A world gate's end runs the [results](RESULTS_RUNTIME.md#the-general-tick-and-input)
  and then the next world's briefing.
- The hold drop for a resource-`0x31` item needs the vehicle update's drop
  services, so it is a typed boundary. No recorded pass held one: every
  vehicle carried scientists (resources 2 and 3). Resource `0x31` is the
  [ECM Unit](HOLD_ITEMS.md#ecm-unit-selector-73), selector 73.

## Death

The gate's `+AC` (`0048C630`), which `0047D670` calls at state 1, shuts
it: flag 1 off (`00479F20(0)`), dirty `+39 |= 1`, model WORD `+42 =
0x11E` refreshed through `0047A110`, then, when the H8 pool cannot give
that model (a negative or past-count index, or a slot that is neither
loaded nor loadable from the current world's resources), the gate is
disposed (`00479FA0`); `+168` is cleared either way. Sanrel-Dek's
stabiliser retires its linked gate this way when it dies (`00476594`),
and the Cheats Menu's level sweep reaches gates of the aliens' side. The
port runs it
([`retire_selector30`](../../crates/inf-game/src/object_runtime/actor/selector30.rs)),
and the lab's Sanrel-Dek sweep matches retail
([the Cheats Menu](PLAYER_MESSAGES.md#the-cheats-menu)).

## Evidence and coverage

The [native receipt](../../crates/inf-game/tests/fixtures/selector30_native.json)
contains eight cases and 232 snapshots from unchanged retail instructions,
including full constructor, payload, specialized commands, `0048C4F0` with
real `0048DAF0`, `0048B230` with real `0048EA80/0047F7B0`, network callback,
and nondeleting destructor. Inputs explicitly seed allocation bytes,
registrations, terrain and world state. The native H1 cases use pair zero
and zero-height terrain; other slots/heights have separate static and host
checks. There is no claim of an enclosing factory or full world replay.
SHA-256: `D99FC22B4E902AD7FE3312F6C48949B8C7C8C91BFD27E39E9EABF0D2BF0D2902`.

The [update receipt](../../crates/inf-game/tests/fixtures/selector30_update_native.json)
runs the original `0048B3F0` over 38 supplied worlds after the real colour
initializer; handle resolution and the sound calls record and return, and the
four typed boundaries stop the run. SHA-256:
`D915F7601EBB7C846A649A12ECA622817455CD678B5A3D96E78A63CEB6E3DD7E`. In the
intro trace's 1611 gate updates only the scan, light and sound arms run; one
found an actor. The port now runs past two of those boundaries, the
near-gate tracking and the destination notice, so those two cases only
check that the update completes.

The gate ledger entry holds
TTD receipts of all five recorded passes:
- **Infest02.** Level 1's gate to Terra Nova (world 2). One update tracks the
  vehicle 58 in front of the ring (local `(-37, 256, 58)`); the next sees
  it 55 behind (`(-31, 254, -55)`). The pass then calls `00487560(2, 1,
  vehicle)`, and phase 8 follows at once.
- **Infest03.** Two world gates (worlds 23 and 7) and the surface-bound
  warpgate.
- **Infest04.** Helniazine's mutant gate `363AE00`, which the player's
  scientist took, at `7BB2C9:9AC`: the buggy reverses north through it and
  comes out of the linked gate `363AC60` (link `0x4B`).

The tests that check them:
- `level1_exit_gate_tracks_the_vehicle_then_passes_it` replays Infest02's two
  updates on the loaded world 18.
- `level1_exit_gate_ends_the_world_for_terra_nova` runs the pass and the
  notices through the world's services. It checks "Warp to Terra Nova", the
  status bits `1 | 2`, world 2 from start 1, and phase 8.
- `selector30_surface_gate_lays_the_actor_out_about_the_linked_gate`
  reproduces Infest03's teleport bit for bit: landing `(19248, 65216,
  45917)`, velocity `(-67, 60, -1396)`, and every basis row with X and Z
  negated, its counter two further.
- `selector30_helniazine_gate_lays_the_vehicle_out_as_infest04` reproduces
  Infest04's: from `(48025, 64396, 34157)` the buggy lands at `(27238,
  64396, 23117)`, its velocity `(5, 10, 659)` becomes `(-5, 10, -659)`, its
  basis rows have X and Z negated with the counter `0x21` two further, and
  the gate's spin becomes the turned `(-62, 1919, -442)` while the buggy's
  stays `(62, 1919, 442)`.

`gate-warp-run.js` reads
Infest04's buggy updates around that warp and snapshots the buggy at
`7B9AAB:E42`, 125 south of the taken gate. The warp replays in the port
from that snapshot. A temporary harness writes the buggy's drive state at
tick 790 of the unmodified Helniazine run above, after its own takeover.
Then retail's controls play: Up and Right, Up, Up, then nothing, with the
retail position step (`--vehicle-position-carry false`) from then on.
Every update matches retail's position and velocity to the unit for ten
updates. The buggy reaches `(48025, 64396, 34157)`, the pass's input, and
crosses on the same update. It then comes out of the linked gate and runs
on to `(27224, 64385, 22585)`. Retail's sample of the crossing update
falls after that frame's gate update, and the port's before it; the next
update agrees.

The first world has three selector-30 placements (3, 11 and 81), all using
H13 slot 15 and model 62, `gate`. Their authored fields and commands run
through the production factory tests with explicit world/terrain inputs.
Those controlled runs do not infer the retail heap allocation policy or
establish later gameplay behavior.
