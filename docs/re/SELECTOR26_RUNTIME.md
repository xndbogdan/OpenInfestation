# Infestation selector 26, the grenade

This document owns selector 26, the ACAM's grenades. It covers the class on
`00455560`'s family with final vtable `005268AC`: how the emitter throws
one, its update, light, draw, messages, contacts and detonation, and the
detonator that sets grenades off. The class's construction prefix and
payload belong to
[ACTOR_RUNTIME.md](ACTOR_RUNTIME.md#behavior-registry-resource-classes); the
fire chain that reaches the emitter belongs to
[WEAPON_FIRING_RUNTIME.md](WEAPON_FIRING_RUNTIME.md).

The code lives in these places:

- [grenade26.rs](../../crates/inf-game/src/object_runtime/actor/behavior_resource/grenade26.rs):
  the update, flight, fuse, detonation, light, messages and contacts.
- [world_actor_update/grenade26.rs](../../crates/inf-game/src/session_platform/world_actor_update/grenade26.rs):
  the world's services and the lifecycle host.
- The actor-resource arm and the detonator in
  [emission.rs](../../crates/inf-game/src/object_runtime/actor/behavior/emission.rs).
- The pair class and furniture arm in
  [world_contact.rs](../../crates/inf-game/src/session_platform/world_contact.rs).
- The world tick's visitor in
  [intro/runtime.rs](../../crates/inf-game/src/session_platform/intro/runtime.rs).

Six resources build the class: 119, 120, 123, 124, 125 and 126. The ACAM's
weapon rows reach five of them:

| Row | Registry kind | Resource | Payload flags `+14C` | Grenade |
| --- | --- | --- | --- | --- |
| 8 | 14 | 119 | 0 | Standard |
| 9 | 17 | 120 | 0, 8 charges `+14D` | Power |
| 10 | 18 | 123 | 1 (fuse 10 s) | Smoke |
| 14 | 22 | 124 | 2 | Chain |
| 15 | 24 | 126 | 4 | Napalm |

Kind 29 builds resource 125 and no ACAM row uses it. Kind 16 is a second
registry entry for resource 119. Row 12's [charge](SELECTOR27_RUNTIME.md)
(selector 27) shares this class with its own update, draw, pair acceptance
and destructor.

## Table

| Slot | Function | Role |
| --- | --- | --- |
| `+24` | `00455E10` | Update |
| `+28` | `00456560` | Light |
| `+2C` | `0047D910` | The common actor draw |
| `+68` | `00404610` | Answers the grenade itself |
| `+98` | `004557F0` | Messages |
| `+A0` | `00455610` | Pair acceptance |
| `+A4` | `00455830` | Pair reaction |
| `+A8` | `00455880` | Furniture impact |
| `+AC` | `00458F80` | Lifecycle state 1 |
| `+B0` | `004564B0` | Lifecycle state 0, the detonation |
| `+B4` | `0047CBC0` | The shared damage reaction |

## Emission

The grenades' registry entries have no factory and no flag 2, so
`0048C890` never looks for a group's earlier shot before throwing one. The
owner's `+4C` (`0047AAF0`) builds the grenade at the emission point, and
`0047B510` adds the emission velocity: the row's speed (WORD `+2`, 2000),
scaled by node `+18` when it is nonzero (`speed * scale >> 16`), along the
node's direction, truncated, plus the owner's `+8E`. The group's handle in
behavior `+24` becomes the grenade's.

All four recorded throws drew nothing. When the emitter returned, each
grenade had:

- radius `+28` 31 (resource 119) or 44 (124) and flags `0x48100807`;
- the owner's basis `+44..+6B` and the owner's handle at `+88`;
- the velocity above and still angular words;
- health 100 in state 3, `+138` 1, the 2 s fuse `+13A` at age `+13C` 0;
- the resource's block `+13E..+147` and visual `+148` (54, or 57 with
  payload flag 2);
- the payload flags `+14C`, charges `+14D` 0, `+14E/+14F` 0 and the burn's
  `+150` 5,000,000.

A cannon shot later in the same group finds the grenade's handle in
`+24`. The cannon's flag 2 sends that grenade `+98(owner, 0, 2, 0)`, which
answers 0, so the cannon builds a shot of its own.

## Update

`00455E10` is the grenade's `+24`.

The [mine](SELECTOR38_RUNTIME.md) runs this update at the end of its own.

1. **Spent.** In state 0 the timer `+13A` grows by the delta's
   milliseconds (`delta >> 10`). For each 100 it loses, `00456350` throws a
   spark and the charges `+14D` drop by one. At zero the grenade is
   disposed. The spark is `004665D0`'s selector 1; where it retires,
   `004661B0` builds the blast 0x36 (`00460310` at `004661CD`, the
   selector-22 visual the grenade's own detonation builds). The lab's
   `frag-throw.json` throws the Power grenade (the shop's frag): it
   detonates at tick 43, its eight blasts come and go, and the grenade
   is disposed at tick 50; the port matches every compared event and its
   frames.
2. **Flight**, `004559E0` (below).
3. A grenade marked to detonate (`+14E`) zeroes its fuse.
4. **Smoke**, `004558B0`, with payload flag 1 once the grenade is older
   than 1000 ms. The quota is `(0xC000 - load factor) / 0x2000`. Below 2,
   or when a draw modulo the quota is 0, three draws make a velocity
   (`>> 9`, `>> 11`, `>> 9`) for `00489D90(0x0B, position, velocity,
   0x800)`. Past 1500 ms the fizz alerts side 0x12: the first time it sets
   flag `0x80` and calls `0047DBF0(800, 0xD, 0, 0x12)`; after that, one draw
   in 16 calls it within 1200. The receivers'
   [arms for 0xD](ACTOR_INTERACTIONS.md#messages-98) raise the alert timer
   `+17A` of family actors and turrets, retire a selector 12 and set the
   vehicle's `+200`.
5. **Burn**, with payload flag 4. `+150` counts the delta down. When it
   runs out it restarts at 200,000 µs and `004887D0` applies the burn's
   block (strength 500, channel 2, outer radius 512) from the grenade with
   its owner as source. Every tick then calls `0047DFD0`, the burning
   status visual.
6. **Fuse**, `004555B0`. The fuse drops by the delta's milliseconds, never
   below 0, and the age rises by them. At 0 the grenade goes to state 0
   through `0047D670(0, its own handle, 0)`.

### Flight

`004559E0` runs only while the grenade is not resting. A resting grenade
(`+14F`) wakes when any velocity axis exceeds 8, and moves the tick after.

1. The shared physics run with mask `0x18` under payload flag `0x20`, else
   `0x1F`.
2. The ground is `00450A30`'s height plus half the radius `+28`. A grenade
   below it bounces:
   - payload flag `0x40` marks it to detonate;
   - `0047A2F0` gives the ground normal;
   - the grenade moves out by half its depth along the normal (`ftol`
     each axis), and never ends below the ground;
   - the velocity's speed into the ground is the x87 dot product with the
     normal. The velocity becomes 7/8 of itself (truncating), and a
     positive speed into the ground pushes it back out with 3/2 of it along
     the normal (`0047B510`);
   - X and Z take another 7/8;
   - `0047D440` turns the up row (basis row 1) toward the normal with gain
     `delta >> 3`;
   - when |X| and |Z| are below 30, |Y| below 100, and `ftol(up.y * 256)`
     exceeds 240, the grenade stops and rests.
3. In the air the basis turns in its Y-Z plane by |vz| + |vy| + |vx|.
   Below the water plane the velocity takes 15/16 on X and Z and 7/8 on Y.
   Under its target depth, the water level less `((handle >> 16 & 0x3FF) %
   10 + 1) * 100`, it also rises by `(target - y) * (delta >> 3) >> 15`.

## Detonation

The lifecycle `0047D670` takes the grenade down a state at a time.

- **State 2.** The grenade's `+68` answers, so the lifecycle skips the
  attribution. There is no `+58` component, and flags gain `0x8000`.
- **State 1.** Health becomes 0, then `+AC` (`00458F80`) marks it to
  detonate with a 5 s fuse. Lethal damage arrives here.
- **State 0.** `+B0` (`004564B0`) detonates it:
  1. Without payload flags, `00458D20` builds the visual `+148` through the
     grenade's `+4C` and bursts sixteen pieces of debris (`0048A8B0`:
     placement radius 0x14, central type 1, debris types 1 and 1, speed 8).
  2. With flag 2, `0047DD20(2000, 0xB, 2000)` broadcasts the chain. The
     vehicle's `00420BE0` jams its player's radar (the map's jitter
     `(2000 - distance) * 0xFFFF / 2000`), a turret raises its jam timer
     `+1B0` to 8000, and a course target clears `+164`; the other
     [receivers](ACTOR_INTERACTIONS.md#messages-98) ignore it.
  3. Unless the grenade is remote (flag `0x80000000`), `004887D0` applies
     its block `+13E` from its position, with its owner `+88` as source.
  4. `0048A0F0(1, position, 0, 0)` births the explosion.
  5. With charges left, the fuse becomes 0 and flags lose `0x104`; the
     grenade stays, spent. Otherwise it is disposed.

The radius effect reaches the grenade itself at distance 0. Its `+B4` is
the shared `0047CBC0`, which in state 0 stops after the damage arithmetic.

## Messages

`004557F0` is the grenade's `+98(sender, distance², event, data)`. Event 9
marks it to detonate (`+14E`). Event 10 does the same when the sender is
null or its handle (`+84`) is the grenade's owner (`+88`). It answers 1
when it marked itself and 0 for anything else.

The detonator is registry kind 23 (no factory, resource 0), the ACAM's row
6. `0048CB00` finds the owner's player through `0046B090`:

- With upgrade `0x26`, Sonic Shock, it sends the pulse `0048D980`
  instead, and no hint. From the owner, `0047DD20(4000, 9, 0)` sends event
  9 to every actor within 4000: grenades and mines mark themselves
  whoever owns them, missiles end their fuse, Bombs arm (`00458010`), and
  the rest answer 0. Then the map's jitter (`G+1B4` `+24`) is set to
  `0x1FFF`, so the radar shakes for about 1.6 s, and the world flashes at
  the owner (`00487D70`, radius `0xF00`, style 14 under style argument 0):
  the screen whitens while the camera is within it. Online a local owner
  then sends the notice `00492500(1, 0x14)`. The port runs it from the
  object draw, as the detonator fires, through the per-class `+98`s and a
  pulse host for the map and the flash; online it stops at the notice. In
  the lab (tampered-run: `runtime_re/lab/scenarios/bomb-sonic-shock.json` (world 6's first start moved onto a Bomb, every upgrade owned)) a Bomb the vehicle never took goes off from the pulse,
  and the port matches retail in every compared event.
- Otherwise `0047DD20(4000, 10, 0)` sends event 10 from the owner.
  `0047DAF0`'s radius arm walks the actor list. It takes signed WORD deltas
  from the source's live position, sums their wrapping squares and compares
  unsigned with 4000². The vehicle's own `+98` receives it too and looks up
  its player (`00420D5C`). Then `0046C760(0x30, 0)` sets the player's hint
  0x30.

## Contacts

- `+A0` (`00455610`) ignores the grenade's owner while the grenade is
  younger than 500 ms.
- `+A4` (`00455830`) marks the grenade to detonate on touch, unless it has
  payload flag 1, or the partner's `+68` answers, or it has payload flag 8
  and the partner is on its team (`+B4`).
- `+A8` (`00455880`) marks it on furniture under payload flag `0x40`, then
  runs the shared `0047BFD0` and answers 0.

The grenade's `+68` answers (`00404610`), as the charge's, the mine's, the
missiles', the implant's and the Bomb's do. Selector 77's `+A4`, `00407BB0`,
asks it: from state 2 on, such a partner striking the eye's tip marks the
eye with its thrower at `+1B0`
([the eyes](ALIEN_BRAIN_RUNTIME.md#struck-at-the-tip)). Every other admitted
`+A4` ignores the partner's `+68`.

## Light

`00456560` lights the ground under a grenade within 1500 of it. It takes
the raw cell height at the grenade's X and Z (`[0055EE3C]`, shifted by
`005372F8`). The grey level is `(1500 - height above) * 20 / 1500`, the
radius `height above * 500 / 750`, at least 256. `0045B1A0` splats it at
the grenade's X and Z.

## Draw

The grenade's `+2C` is the common `0047D910`. A model word of `0xFFFF` takes
it off the draw.

## Port

The emitter builds selectors 26, 32 and 38. The world tick's object walk
runs `00455E10` for each live grenade, as it runs the cannon's shots. A
stopped grenade is logged once and is not updated again.
The world lends the grenade its smoke, the radius effect through the world
damage, the spark, the default burst's visual through the child factory, the
debris, the explosion, the messages (the alert, the chain, with the map's
jitter for the radar jam), the burning actors' status visual (`0047DFD0`,
shared with the family update) and the lifecycle.

The world damage admits the grenade's `+B4` as `0047CBC0`. The reaction's
`+68` query answers the grenade itself. Lethal damage reaches the lifecycle's
state 1, whose `+AC` is the grenade's.

Not yet ported. Each is a named boundary that stops the grenade or pair
where it is reached:

- The detonator pulse's network notice `00492500` (online only).
- A receiver's arm the port does not own, such as a factory's for the
  alert ([messages](ACTOR_INTERACTIONS.md#messages-98)).
- The spark's network notice and impact, which an offline game does not
  reach.

So every grenade runs whole, and so does the detonator, with Sonic Shock too.

## Verification

The [update receipts](../../crates/inf-game/tests/fixtures/grenade26_updates.json)
hold every `00455E10` call of the four grenades thrown in the recordings:
Infest02's three standard grenades on Level 1 (world 18) and Infest03's
chain grenade in world 11, 52 updates in all. The
ledger gives the captures.
Each record keeps the grenade's entry and exit fields, the RNG and the
nested calls.

- `grenade26_update_matches_every_recorded_tick` runs all 52 from their
  entry state on their world's loaded terrain. The flight with its bounces
  and settle, the fuse and age, and each detonation through the lifecycle
  match every field the update writes. The services reached match the
  native calls.
- `grenade_throws_match_the_recorded_emissions` runs the four throws
  through the emitter and the world's child factory, from the recorded
  node, emission point, owner velocity and basis. Every field above
  matches, and the RNG word is unchanged.
- `a_thrown_grenade_detonates_next_to_an_alien` runs a thrown grenade's
  last tick through the world host. The lifecycle detonates it, builds the
  visual, kills the alien next to it, reaches the grenade's own `+B4` and
  disposes it.
- `the_detonator_marks_only_the_vehicles_grenades` and
  `a_grenade_in_the_group_leaves_the_cannon_its_own_shot` cover the
  detonator and the cannon's group lookup.
- `a_chain_grenade_jams_the_vehicles_radar`,
  `a_smoke_grenade_alerts_the_alien_near_it` and
  `a_napalm_grenade_burns_as_it_flies` cover the three payloads: the
  chain's jam by distance, the fizz's first alert at the age 1586, and a
  flame from each of the napalm's 16 ticks of flight.

A Level 1 probe with ammunition threw grenades and set one off with the
detonator, with no stops: the grenade flies, bounces, bursts and damages
the vehicle beside it.
