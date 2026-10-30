# Infestation selector 38, the mine

This document owns selector 38, the ACAM's mines: the class on `00455560`'s
family with final table `00525914`, built from resources 117 (row 11,
registry kind 19) and 118 (row 13, kind 21). It covers the mine's update,
its target scan and tracking, its detonation and its destructor. The mine
shares most of its table with the [grenade](SELECTOR26_RUNTIME.md): the
grenade's update `00455E10` runs at the end of the mine's, and the light,
draw, messages, contacts and lifecycle state 1 are the grenade's. The
construction prefix and payload belong to
[ACTOR_RUNTIME.md](ACTOR_RUNTIME.md#behavior-registry-resource-classes).

The code lives in
[mine38.rs](../../crates/inf-game/src/object_runtime/actor/behavior_resource/mine38.rs),
with the world's services in
[world_actor_update/grenade26.rs](../../crates/inf-game/src/session_platform/world_actor_update/grenade26.rs).

## Table

| Slot | Function | Role |
| --- | --- | --- |
| `+0C` | `00456840` | The payload: `00456740` |
| `+24` | `00456860` | Update |
| `+B0` | `004566A0` | Lifecycle state 0, the detonation |
| destructor | `00456640` | Deletes the `+15C` target; `00458FC0` is its scalar deleting form (`+00`) |

Every other slot is the grenade's: `+28` `00456560`, `+2C` `0047D910`, `+68`
`00404610`, `+98` `004557F0`, `+A0` `00455610`, `+A4` `00455830`, `+A8`
`00455880`, `+AC` `00458F80` and `+B4` `0047CBC0`.

## Construction

The constructor `00460CD8` adds `+15C` and `+160` (zeroed) to the grenade's
tail and sets `+138 = 0`: the mine starts unarmed. `00456740` reads the
payload: byte `+154` is set when payload DWORD `+58` is nonzero, DWORD
`+158` is payload `+5C`, and `+148` becomes `0x37` when `+154` is clear.

Both recorded throws (Infest03, resource 118) left the mine with the
vehicle's basis and handle, radius 36, flags `0x48100807`, health 1 in
state 3, the 2 s fuse, payload flag 8, `+154` 1 (a flat mine) and trigger
radius `+158` 600.

## Update

`00456860` is the mine's `+24`.

1. **Rearm.** When the fuse `+13A` is at or below 1000, it goes back to
   2000, `+138` becomes 1 (armed), the tracked distance `+160` is cleared
   and the target `+15C` is deleted. The grenade's fuse then counts it
   down again, so a mine never runs out its fuse. It arms a second after
   it is laid, and refreshes its target every second after that.
2. **Scan,** without a target. `00456780` builds the evaluator (table
   `005259E0`) and `00475EC0` walks `G+48`: an actor with flags `0x24`,
   nearer than 2000 in X and Z (unsigned squares), not the mine. The
   evaluator `004567A0` scores nothing for an actor below lifecycle 2, the
   mine's thrower, or an actor on the mine's side, side 1 or side 0.
   Otherwise, within the trigger radius (unsigned squares) it scores
   `0x7FFFFFFF` less the squared distance, so the nearest wins. A flat mine
   measures X and Z only.

   For a winner, `004825D0` builds a 0x20-byte type-2 target on its handle
   at `+15C`, and `+160` is the squared distance to the actor itself.
3. **Track,** with a target, while `+160` is below the trigger radius
   squared (signed). The target's `+18` (`00482690`) refreshes its focus,
   the actor's position raised by its `+74`. If the squared distance to
   that focus exceeds `+160` (signed) and the mine is armed, the mine marks
   itself to detonate (`+14E`). Otherwise `+160` takes the new distance.
4. The grenade's `00455E10` (flight, smoke, burn, fuse).

## Detonation

`004566A0` is the mine's `+B0`.

- **A flat mine** (`+154` set) sets `+148 = 0x3A` and runs `00458D20`: the
  visual through its `+4C` and the debris, at its own position. It then
  sets `+148 = 0x38` and applies its block `+13E` 1500 above itself
  (`004887D0` from a point on its stack). It tells its thrower `0x13`.
- **Any other mine** tells its thrower `0x11`.
- Then the grenade's `004564B0`.

The thrower hears through `0047DD40(+88, 0, event, 0)`: the handle
resolves, then `0047DD80` sends `+98(mine, distance², event, 0)` with no
range limit. The vehicle's `00420BE0` acts on `0x11..0x13` only online (its
grants `0xAA..0xAC`).

## Port

The emitter builds selector 38 through the child factory, and the world tick
runs `00456860` for each live mine. The update's target scan, tracking and
detonation are ported. The detonator's event 10 reaches a mine through the
grenade's `004557F0`, and the mine's destructor deletes its target.

A mine's blast reaches what the port builds. Infest03's mine also set off
two of world 6's selector-29 placements, which the port does not construct
yet.

The laser mine draws H8 85, whose model value 1 is `+138`. Until it arms,
its program reads register 3 before writing it; the object draw starts that
register at Infest03's stack value, so a thrown mine keeps its third fin and
its base ([LaserMineModel85StackHistoryApproximation](MODEL_RUNTIME.md#laserminemodel85stackhistoryapproximation)).
The proximity mine draws H8 84 with the same unarmed read, which retail
finds `2861` ([ProximityMineModel84StackHistoryApproximation](MODEL_RUNTIME.md#proximityminemodel84stackhistoryapproximation)).

## Verification

The [update receipts](../../crates/inf-game/tests/fixtures/mine38_updates.json)
hold every `00456860` call of the mine Infest03 laid in world 6: 25 updates,
from its landing to its detonation. The
ledger gives
the captures.

- `mine38_update_matches_every_recorded_tick` runs all 25 on world 6's
  terrain. The scan (no enemy inside 600), the rearm, the flight and the
  detonation match every field the update writes. On the detonation tick
  the bursts' impulses on the mine are the world's, which the recording
  host does not run. The services reached match the native calls.
- `grenade_throws_match_the_recorded_emissions` covers both mine throws.
- `the_detonator_sets_off_the_vehicles_mine` lays a mine in world 6 and
  sets it off with the detonator: the flat burst's visual, message `0x13`,
  the disposal.
- `a_mine_tracks_an_enemy_and_goes_off_as_it_leaves` lays a mine 300 from
  one of Level 1's side-0x12 actors. The scan picks it, the mine tracks it
  through the type-2 target, arms, and goes off the tick the actor moves
  away.

A Level 1 probe laid a mine and set it off with the detonator, with no
stops.
