# World contact pass `004791F0`

`00478F20` runs `004791F0` after its update-list walk, the `0046A260`
controller call and the Flag1 list swap (`0047901F..22`). The actor-pair pass
`004790C0`, the camera and the particles follow it. The port runs the pass
at the end of the world actor tick in
[runtime.rs](../../crates/inf-game/src/session_platform/intro/runtime.rs).
The [arena owner](../../crates/inf-game/src/object_runtime/world_contact.rs)
holds the native order and arithmetic. The
[production services](../../crates/inf-game/src/session_platform/world_contact.rs)
lend the model, terrain, furniture and damage owners.

## Per-actor order

The pass visits the `G+48` actor list (`M+40` with `M = G+8`) in order and
reads flags `+2C` once per actor. Flag 8 skips the actor. Both phase gates,
and the flags merged back at the end, use that single read.

1. **Terrain phase**, when `(flags & 0x04000100) == 0x100`:
   - The request takes H8 word `+42` through `00405540` (a NULL model faults
     in `004F2C50`), the virtual `+48` basis (`004026B0` in every admitted
     final table) and request `+10/+14/+18` from the sign-extended position
     WORDs. Wrapper `00527B18` captures the actor for packed values.
     `0044EC50` installs terrain table `00537090`, runs `004F2C40`, and
     returns `SETNE` of the result flag DWORD. Request `+20` stays
     uninitialized, and no slot of that table reads it.
   - A nonzero flag calls virtual `+94` with the contact, then ORs `0x80000`.
     A zero flag clears `0x80000`. The context copy refreshed afterwards at
     `00479346..6D` is dead.
   - The water classification below reads the current position and radius.
   - `0x60000000` is cleared, then `0047EBB0` ORs in the pre-pass flags
     masked with `0x9FFFFFFF`, plus the new water bit. A `0x80000` cleared
     this frame therefore returns if it was set at the start, as does any bit
     a response dropped. Both writes set dirty bit 2 only when the low nine
     bits change.
2. **Furniture phase**, when `(flags & 0x0400000C) == 4`:
   [`0047C1D0`](FURNITURE_COLLISION.md), whose nonzero contacts reach virtual
   `+A8`.

## Terrain responses (virtual `+94`)

| Final table | `+94` | Port policy |
| --- | --- | --- |
| Selector10 `00520CC8`, Selector12 `00527058`, Selector36 `00527EE0`, the vehicle (Selector2) `00520860`, the [Powerball](ACTOR_RUNTIME.md#selector-57-the-powerball) (Selector57) `00526464` | `0047D2F0` | Shared response; Infest01 reaches it 4 times on the vehicle, whose impact damage goes to its own [`004212F0`](VEHICLE_DRIVE_RUNTIME.md#the-vehicles-damage-004212f0); the Powerball's first three contacts in Infest03 and Infest04 replay exactly |
| The [Boulders](ACTOR_RUNTIME.md#selector-59-the-boulders) (Selector59) `0051F900` | `00405A00` | The shared response, then, drawn near (flag `0x2000000`) and faster than 100, a draw below `(speed + 20) * 64` throws up dust (`004894A0`, no shadow), or a splash (`00489600`) where the water stands above the contact point, with half the velocity; a draw below half of that throws a second at a jittered point. Infest04's first sixteen that threw any up replay exactly |
| Selector35 `0052273C` | `0043F3E0` | `+C4 == 1` first runs radius `004887D0` (unless flag `0x80000000`), burst `0048A8B0` over the position copied before the radius effect and state `0047D670(0)`, then the shared response; other states take the shared response alone ([13 native cases](../../crates/inf-game/tests/fixtures/selector35_crash_native.json)) |
| Selector34 `00521EAC` | `00434ED0` | Every `+C4` except 3 first runs burst `0048A8B0`, radius `004887D0` (unless flag `0x80000000`), pickup `0046D870(+32, 0x83, 500)` and state `0047D670(0)`, then the shared response; `+C4 == 3` takes the shared response alone ([13 native cases](../../crates/inf-game/tests/fixtures/selector34_crash_native.json)) |

Other classes stop at a typed boundary before any write.

### Shared response `0047D2F0`

The signed low WORD of `+10` scales each binary32 normal component. Each
truncated product is added to position WORDs `+32/+34/+36` with no dirty
bits. The closing speed is the negated truncation of
`(v.z*n.z + v.y*n.y) + v.x*n.x`, with velocity WORDs `+8E/+90/+92`. A
nonpositive closing speed returns.

Otherwise the rebound is the low WORD of the signed `(3 * closing) / 2`. The
impulse, the truncated `rebound * n` per component, goes through `0047B510`'s
WORD-wrapped add. Flag `0x80000000`, or a closing speed of at most `0x96`,
returns. With `0055F448` set, a zero DWORD `+34` of `00463AB0`'s static
session `0054E8A8` also returns.

The remaining case calls `0047C6E0` with the actor's own handle `+84` as
source. Its packet holds:

- the strength WORD from `0047B590`;
- channel byte 1;
- byte 3, the sign byte that the rebound WORD left in the reused argument
  DWORD.

`0047B590` takes the WORD-wrapped change from the saved velocity. A
sign-set i32 square sum takes `0048EFC0` of its logical quarter, doubled.
The length is multiplied by mass WORD `+8C` and divided by 192 through
`2AAAAAAB` and `SAR 5`.

## Furniture response `0047BFD0`

The flag-4 phase runs `0047C1D0` through a pass host whose terrain reads go
through the host, so a reached response can lend the same terrain to the
damage owners. A nonzero contact reaches virtual `+A8`:

- Selector34/35 (`00521F54`/`005227E4`) hold `0047BFD0` itself, as do the
  vehicle (`00520860`), the Powerball (`00526464`) and the Boulders
  (`0051F900`); no recording has a Powerball touch furniture.
- Selectors 10/12/36 hold guard `0042A760`. Its `0x8000` arm returns AL 1;
  otherwise it continues into the same response.

`0047C1D0` passes four arguments:

1. the contact;
2. the H11 entry behind the cell's placement lookup, whose byte `+2` (the
   low byte of `parameter_word`) is the kind that `00436020` also reads;
3. its stack cell position `[x, height, z]`;
4. an unread pointer.

The response always returns AL 0, so the scan continues.

1. The closing term is `ftol((v.y*n.y + v.z*n.z) + v.x*n.x) - 1`. The push
   matches `0047D2F0`'s.
2. A negative term becomes `-1500` for kind `0x0E`, otherwise
   `term + (term >> 3)`. The velocity WORDs become the saved velocity minus
   the truncated `rebound * n`, written directly rather than through
   `0047B510`.
3. Flag `0x80000000`, or a term at or above `-0x32`, returns.
4. Kind `0x0E` then calls `0047C6E0` with strength 3000 and channel 7. Its
   source is process NULL `0054E894`, and the call returns.
5. The network gate follows, as in `0047D2F0`. A term below `-0x96` calls
   `0047C6E0` with `0047B590`'s strength, channel 1 and the actor's own
   handle. It then calls `00436020` with the same packet at the cell's X/Z
   WORDs.

Packet byte 3 is the top byte of the H11 entry pointer, whose argument slot
holds the packet: an allocation address that the port leaves unknown.

## Actor-pair kernel `0047B610`

`004790C0` runs after `004791F0`. It pairs each flag-4 actor with every later
`G+48` actor whose flags also hold 4, unless both hold flag 8. The
broadphase is the unsigned i32 square of the WORD position deltas, below
the square of the summed radius WORDs `+40`. `0047B610(A, B, &delta)`
returns AL. A nonzero AL rereads A's flags, and A advances once its flag 4
is gone. Over the intro window
`31:0..735E8F:26E6`, the native kernel runs its query 1880 times and reaches
the contact branch 400 times. Every contact continues past `+A0` into `+A4`,
and one reaches `0048AB10`. To the trace end there are 422 contacts, mostly
the two Selector36 placements 51/79 (241), Selector12 with Selector77 66
(65), Selector10 with Selector66 children (40) and Selector12 pairs (34)
(census).

The kernel admits both H8 headers `+0E`, builds A's context (origin `delta`,
basis `+48`) and B's (origin 0, basis `+48`), and runs `004F2BE0` with table
`00544C68`. A zero contact returns AL 0. After a contact the
[pair owner](../../crates/inf-game/src/object_runtime/world_contact/pair.rs)
runs the rest in order, and the kernel returns AL 1:

1. **`+A0` on both.** A's `+A0(B)`, then B's `+A0(A)`; a nonzero AL from
   either ends the kernel.
2. **One momentum arm.**
   - *Static B (flag 8):* A takes rebound `along + (along >> 3)` from
     `ftol((v.x*n.x + v.z*n.z) + v.y*n.y)` when it is negative, then the
     push by the signed low WORD of `+10`.
   - *Exchange:* used when neither actor is static, unless A weighs over 16
     times B (mass WORD `+8C`, compared with signed `JL`/`JGE`) and A's
     `+C4` is at least 3 (signed). While the relative velocity closes along
     the center line (Y/Z offsets spilled through binary32), each axis sets
     both velocities to the integer center-of-mass velocity. A nonzero
     `(vA - vB) >> 1` is shared out by the partner's mass. Each actor then
     moves, and gains velocity, by `displacement * partner_mass / sum + 1`
     along `±n`.
   - *B rebound:* used when A is static, or heavy as above. B takes the
     same rebound from its own velocity, or from `vB - vA` in the heavy
     case. Its position moves by `-push`.
3. **Impact effect.** When `|(mA + mB) * along|` from the B-rebound arm
   exceeds 80000, `0048AB10(0, midpoint, energy / 13333)` runs at the
   radius-weighted midpoint.
4. **`+A4` on both.** A's `+A4(B, &savedA, &savedB)`, then B's, with the
   pre-exchange velocities.
5. **Damage.** For each actor without flag `0x80000000` and not static,
   `0047C6E0` uses `0047B590`'s strength and channel 1, sourced from the
   partner's handle. A static partner takes the same packet back.

check_actor_pair_response.py runs the
unchanged kernel from its entry over 262 cases. It supplies the H8 lookup,
the pair query, `+48`/`+A0`/`+A4`, `0048AB10` and `0047C6E0`, all recording
their arguments. The [Rust comparison](../../crates/inf-game/src/object_runtime/world_contact/tests.rs)
matches every call, argument, position and velocity of the 238 cases with a
contact. Two targeted cases pin the 1/16 mass boundary and the exact-80000
energy skip.

### Pair query and class virtuals

The port runs `004790C0` right after `004791F0`, through the
[pair walk](../../crates/inf-game/src/object_runtime/world_contact/pair.rs).
`0047B610`'s `004F2BE0` call uses the interpreter's
[stack counterpart](../../crates/inf-game/src/model_auxiliary.rs): the
composite table recurses into B's own stack context, carrying B's
`00527B18` value wrapper, instead of the furniture query's process B. A
retail test checks that, with identity bases, both counterparts give the same
contact for models 223/51.

Every actor table the port constructs holds `+48 = 004026B0` (selectors 2,
6, 7, 10, 11, 12, 15, 16, 26, 30, 33, 34, 35, 36, 37, 39, 60, 66, 77, 88,
90, 93), so all three pass phases read `rotating.basis` without class
admission. The class virtuals' partner calls resolve statically:
`+5C/+54/+64/+68` are `00401300` (NULL) for every admitted class, except
Selector12 `+5C`, Selector2 `+64`, the grenade's `+68` and the `+54` of
Selector30, Selector15, Selector37 and Selector39, which are `00404610`
(self). The factory's table `00521940`, the bridge's `0051FF54` and the
course target's or generator's `00522398` share the gate's
`+48/+54/+5C/+64/+68/+A0/+A4`; their `+D0`s are `0042F170`, `0040DAB0` and
`0043BEA0`. Selector 16's `005204D0` has the shared table's.

| Final table | `+A0` | `+A4` |
| --- | --- | --- |
| Selector10, Selector36 | `00425B90`: both `+C4` 3 and a Selector12 partner clear the ship's flag `10000000` (outside the dirty mask), then 0 unless the ship's team is 0. A team-0 unit then takes the person aboard (`00425BE0`): the person's team (dirty 2), `+1A0` emptied, the behavior's choice `00428390`, the team's mode `00425AB0` through `00425A40`, effect `00489A90(2)` at the person and its disposal, and the pair accepts; so a scientist mans a neutral tank, and an alien one would make it the aliens' ([64 native cases](../../crates/inf-game/tests/fixtures/family_rescue_native.json), the four boarded ones checked through the team, dirty and `+1A0`) | `00427C20` |
| Selector12 | `00468160`: a partner whose handle is target `+04` answers `+54`. The gate's `+D0=0048BD90` [takes the scientist's team](SELECTOR30_RUNTIME.md#a-scientist-takes-a-gate-0048bd90), a `+18E` gate passes it (`0048BE70`), and it answers 0, so `00468191` sets the scientist's `+22E` bit 0; the bridge's `+D0` is the family's staff take `0040DAB0` [itself](FAMILY40D800_RUNTIME.md#staffing-0042f170-0040dab0-beam-down-0042f670), whose acceptance ends the pair and whose refusal sets `+22E` bit 0; the target's `+D0=0043BEA0` [takes it the same way and, when that fills it, grants `+7A` to the first player](FAMILY40D800_RUNTIME.md#the-hangar-takes-a-scientist-0043bea0), whose acceptance ends the pair and whose refusal sets `+22E` bit 0; the factory's `+D0=0042F170` [staffs it](FAMILY40D800_RUNTIME.md#staffing-0042f170-0040dab0-beam-down-0042f670), whose acceptance ends the pair and whose refusal sets `+22E` bit 0. Then 0 unless `+C4 > 2`, the partner is Selector2 and team `+B4 != 0x12`: the scientist [boards the vehicle](ACTOR_RUNTIME.md#carried-units) and the take's result ends the pair | `00469030`: [the vehicle's knock](#the-vehicles-knock-00469030), then `00427C20` |
| Selector35 | `0043F790`: with `+C4` at least 3 and a count `+242`, a scientist (Selector12, whose `+5C` answers) is disposed and counted aboard ([the pickup](SELECTOR35_RUNTIME.md#rescue-and-go-to-space-descriptors-10-and-11)); otherwise 0 | `00427C20` |
| Selector34 | `00402720`: 0 | `00427C20` |
| Selector2, Selector11, Selector60 | `00402720`: 0 | `00427BE0` |
| Selector15, Selector30, Selector37, Selector39, Selector66 | `00402720`: 0 | `00405570`: returns |
| Shared `00527C00` and its near copies: selectors 6, 16, 52, 53, 57, 69, 88, 90, 93 | `00402720`: 0 | `00405570`: returns |
| Selector59, the [Boulders](ACTOR_RUNTIME.md#selector-59-the-boulders) | `00402720`: 0 | `00405E30`: with `+144` or `+145` set, its change of speed since the saved velocity past 320 (any, with `+145`) damages the partner (`0047C6E0`) by `isqrt(excess) * excess >> 4`, times its mass `+8C >> 6`, at most `0xFFFF`, on channel 3 from its handle |
| Selector77 | `00402720`: 0 | `00407BB0`: inert while the partner's `+68` is NULL. From state 2 on, a partner whose `+68` answers (a grenade, charge, mine, missile, the implant or the Bomb, all `00404610`) within 500 of the stalk's tip (`+D8`, `00408310`), with the tip within 150 of its own axis (`0047A660`), leaves its `+88` at `+1B0`, which the eye's next update (`00407CA0`, `00408AB0`) turns into 10,000 damage from that source ([the eyes](ALIEN_BRAIN_RUNTIME.md#struck-at-the-tip)) |
| Selector26, the [grenade](SELECTOR26_RUNTIME.md#contacts), and Selector38, the [mine](SELECTOR38_RUNTIME.md) | `00455610`: 1 for its owner while it is younger than 500 ms | `00455830`: marks it to detonate unless payload flag 1, an answering partner `+68`, or flag 8 with the partner on its team |
| Selectors 25 and 28, the [missiles](SELECTOR25_RUNTIME.md#light-draw-messages-and-contacts) | `00455610`, as the grenade's | `00455640`: an armed missile detonates through the lifecycle; its furniture `+A8` (`00455660`) does the same and answers 0 without the shared response |

### The vehicle's knock `00469030`

A mutant scientist (team `+B4` 0x12) alive in state 3 that the vehicle hits
(its partner's `+64` answers, which only Selector2's does) measures how far
the exchange changed its velocity: `+8E..+92` against the copy saved before
the exchange, signed WORD deltas' wrapping squares, a sign-set sum taking
the root of a quarter, doubled (`0048EFC0`). Above 500 the scientist dies
through `0047CE90`, as a shot one does; then `00427C20` runs as for any
scientist. A scientist of another team only sidesteps. Before, the port
stopped the pair there, so the vehicle passed through a mutant scientist
after the first touch; course targets, generators, bridges and selector 16
stopped every pair as unadmitted, so the vehicle drove through them. [A
test](../../crates/inf-game/src/session_platform/intro/runtime/tests/contact_tests.rs)
knocks Level 1's scientists by 500 and 501 on team 0x12 and by 2022 on
team 1: only the 501 dies. No recording knocks one:
`scientist-knock.js` finds
no `00469030` call with the vehicle and a living mutant in Infest01..03
(Infest02 has 14 calls, none of them).
`check_world_interactions.py` runs
the original `00469030` with the partner's real `+64` (`00404610` or
`00401300`) and `0048EFC0` over 76 knocks, among them 500 (spared), 501
and two whose squares pass bit 31; the port's `knock_kills_scientist`
matches every one. Live on Terra Nova, the buggy held at Up into a mutant
scientist kills it on contact at about 800 (health 0, state 1, through
`0047CE90` with no damage packet). At 470 against one walking away at 130,
its speed jumps from 132 to 736 and it dies. Nudged at about 200, its speed
goes from 130 to 226 and it walks on. Those runs placed the vehicle with a
temporary harness. An unmodified run from the Terra Nova save shows the
knock on keys alone. The buggy drives down the valley at about 1050 and
meets mutant scientist 83 head on. Its velocity jumps from (-14, 0, 85) to
(-289, -32, 1484) and it dies in that tick (health 300 to 0, state 1). The
pair's channel-1 hit that follows does nothing. A build that only adds
logging renders the same frames pixel for pixel.
A generator takes no damage from the vehicle's
own collision (channel 1 from a resource outside 0x29..0x2C, `0043BD90`);
the vehicle takes it at its channel-1 threshold 1500, four times over.

No recording drives into a course target or generator
(`vehicle-target-contact.js`
finds the vehicle's `+A4` with a factory, a bridge and two other tables,
never `00522398`). The bridge shares their pair bodies, and Infest02's
buggy hits Level 1's bridge three times from `3BEA37` on. Replayed in the
port from a snapshot five updates before
(`vehicle-run.js`), with the
recorded Left and Down and the bridge at its retail height, every update
matches: the bounce off the bridge at 808 (to `(93, 326, 197)`), the next
two contacts and the run after them. The bridge stands at y -26 because
Infest01 and Infest02 load Level 1 on fine terrain: the intro's gameplay
screen selects the detail (`00440B20`, `004502C0`) and the world load
keeps it. The port does the same after the intro. Only its
`--mission 18` shortcut skips the intro, loads Level 1 on the startup's
coarse spacing and places the bridge at -24.

`00427BE0` turns a negative `+144` into `+144 * -1000` and clears flags
`0x04600000`. The [shared reaction `00427C20`](../../crates/inf-game/src/object_runtime/actor/derived/family_pair.rs)
does the same, then branches on the partner:

- **Another team (`+B4`):** it only sets `+22E` bit 1.
- **Same team:** it continues only when the partner's mass WORD times 8
  exceeds its own, the partner's `+C4` is 3 and `+154` is empty. The
  sidestep radius is `max(word+28 * 4, +14C / 2)`. The partner offset goes
  through `0047A660` (rows of the `+44` basis); one RNG draw gives a 12-bit
  angle.
  - A partner ahead (local Z > 0) gives a straight step to `∓radius` on X.
  - Otherwise the point is `(sin(angle+0x1000), 0, sin(angle+0x5000)) *
    radius >> 15` from table `0053DF00`, X mirrored away from the partner.
  - `0047A6C0` returns the point to world, added to the actor position.
  - The 0x14-byte allocation becomes a kind-8 copied point (`0051F154`)
    sourced from process NULL `0054E894`. It is stored at `+154`, and
    `+15C = (rng16 + 0xF424) << 4`.

check_family_pair_reaction.py runs
the unchanged `00427C20` over 202 cases, with its own transforms, RNG and
sine table and a supplied allocator. They cover other-team marks, gated
cases and 87 helpers on both branches. The
[Rust comparison](../../crates/inf-game/src/object_runtime/actor/derived/family_pair/tests.rs)
matches every field, the RNG state and the helper point. Targeted cases pin
the exact 1/8 mass gate and a partner straight ahead.

The impact effect `0048AB10` enqueues sound `0x47` and then births
`strength` sparks of class `00528954`
([impact sparks](PARTICLE_RUNTIME.md#impact-sparks)). It ran once in the
native intro. The port's pair host lends it the live particle pool, the
process sound list and the shared RNG. A failed pair is logged once and
skipped until world replacement; its reached writes stay.

## Water classification

This uses the constructor's [terrain classification](../../crates/inf-game/src/object_runtime/actor_terrain.rs)
with the actor's unsigned radius WORD `+40` in place of the fixed 32:

- `Y + r + margin < plane` sets `0x20000000`, with
  `margin = 0x200 >> (005483BC - 8)`.
- `Y - r - margin > plane`, or a `0047F640` cell at or above the plane,
  sets `0x40000000`.
- Otherwise `00410F60`'s water height, with H14/0 `+4` bit `0x40` and clock
  `005471C0`, gives `0x40000000` above `Y - r`, `0x20000000` below `Y + r`,
  or neither.

The pass lazily loads H12 at `004793A9` rather than taking the constructor's
miss arm, so the port requires the resident surface.

## Evidence

- The `Selector35FallHistory` ledger row
  shows native ref61 held at Y `151/152` by `0047D33D` after each physics
  drop. `004791F0` reads Y at `00479282` and rereads it at `00479351` after
  each contact.
- check_terrain_sphere_contact.py
  pins the `+00` kernel ([contact geometry](CONTACT_GEOMETRY.md#terrain-sphere-0044ec90)).
- check_terrain_contact_response.py
  runs the unchanged `0047D2F0` with its own `0047B510`, `0047B590`,
  `0048EFC0` and `00463AB0` over 162 cases: push only, rebound, both skips
  and 52 recorded `0047C6E0` packets. Two cases use non-unit normals to reach
  the sign-set square and the wrapped `3 * closing`. The
  [Rust comparison](../../crates/inf-game/src/object_runtime/world_contact/tests.rs)
  matches position, velocity, flags and every packet. The same tests cover the
  flag merge and the phase gates.
- A retail test runs intro ship model 474 (`07/8E/95` with eight hull
  spheres) through table `00537090` over flat terrain. Every contact is +Y,
  and each 10-unit drop adds exactly 10 to the displacement.
- check_furniture_contact_response.py
  runs the unchanged `0047BFD0` over 180 cases, covering:
  - no rebound and rebound only;
  - the `0x80000000` and network skips;
  - 26 kind-`0x0E` fixed damages;
  - 46 impact damages, each followed by its `00436020` call.

  The Rust comparison matches positions, velocities, AL, packets, sources
  and cells. A separate test pins the `-0x32` threshold through the network
  gate read, since kind `0x0E` always rebounds at -1500.

## Port containment and remaining owners

A failed phase is logged once and stops only that phase of that actor until
world replacement. Reached writes are kept, and the other phase keeps
running. The 1100-frame intro smoke runs with no contact-pass stop of
either phase. The Selector35 ships no longer stop at `+260`, and terrain
and furniture impacts run through the live `0047C6E0` and `00436020`
compositions. The pair pass also runs without a stop: the pair that used to
stop at `0048AB10` (strength 7) now births its sparks.

The remaining required owners are:

- `0047BFD0` from class-tail `0047C1D0` callers;
- the class arms above that remain typed boundaries (class10/36 team-0
  boarding, the target's staffing,
  pickup), and pair classes outside the table;
- a networked session's `0054E8A8 +34`.
