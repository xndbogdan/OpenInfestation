# Infestation selector 32, the cannon's shot

This document owns selector 32, the ACAM's default cannon. It covers the shot
class `00485F50` (final vtable `005285F0`, resource 116): how the emitter
builds or repeats a shot, its update, draw and repeat, and the delivery
`00488830` of its damage. The class's construction prefix and the damage
cache's use of it belong to
[ACTOR_RUNTIME.md](ACTOR_RUNTIME.md#behavior-registry-resource-classes); the
fire chain that reaches the emitter belongs to
[WEAPON_FIRING_RUNTIME.md](WEAPON_FIRING_RUNTIME.md).

The code lives in four places:

- [shot32.rs](../../crates/inf-game/src/object_runtime/actor/behavior_resource/shot32.rs):
  the update, draw, repeat and group.
- [world_actor_update/shot32.rs](../../crates/inf-game/src/session_platform/world_actor_update/shot32.rs):
  the world's spark and delivery.
- The actor-resource arm of
  [emission.rs](../../crates/inf-game/src/object_runtime/actor/behavior/emission.rs).
- The world tick's visitor in
  [intro/runtime.rs](../../crates/inf-game/src/session_platform/intro/runtime.rs).

The shot is not a projectile. It sits at its owner's muzzle and aims at the
owner's target. Its update hands that target the damage at once.

## Emission

The cannon is firing registry kind 1 (`0053DBB8`): no factory, resource 116,
flags `0x16`. `0048C890` reaches its actor-resource arm with the behavior in
ECX and the node, the position and the owner on the stack. The node's group
is node `+10`.

1. With flag 2, `0048C9DF` reads the group's handle in behavior `+24`, and
   `0047AD90` resolves it. A live shot gets `+98(owner, 0, 2, 0)`. If it
   answers that it took the shot, no new shot is built. Without flag 2,
   `00481040` counts an actor firing for the owner's category.
2. Resource `0x82` goes to `00431EF0` instead, the
   [flamethrower](SELECTOR76_RUNTIME.md). Otherwise
   the owner's `+4C` (`0047AAF0`) builds the shot. It gets the resource WORD
   in DX (the high half is residue) and the position.
3. `0048CA63..0048CA7C` store the shot's handle in behavior `+24` for the
   group. `0047B510` adds the emission velocity to the shot's `+8E`.
4. The shot's `+6C` answers the shot itself. `0048CA90..0048CAC3` store the
   group at `+144`. With registry flag `0x40`, a positive strength `+146`
   becomes `-strength / 10`: `IMUL 99999999h`, `SAR EDX, 2`, then the sign
   bit is added.
5. If the owner has flag `0x10` and the shot has flag `0x200`, `0046AE90`
   tracks the player's shot (a port limit).
6. The tail runs whether a shot was built or repeated. With flag `0x10`,
   an even draw births `00489D90(2)` at the position, moving with the
   owner's `+8E` (`0048CB71`). Then come `+38 = 500000` and the sounds.

**WeaponHandleResidueApproximation.** Behavior `+24` is allocated but its
words are never written, so a group's first read is heap residue. All three
first reads recorded were residue that `0047AD90` resolved to no actor:
Infest01 `03842EB0`, Infest02 `0379094C` and `0394186C`. The port treats an
unwritten word as no earlier shot.

The construction is the shared one described in
[ACTOR_RUNTIME.md](ACTOR_RUNTIME.md#behavior-registry-resource-classes).
It sets life `+140` to 600,000 µs, one pending packet at `+14C` and channel
`+148` 6. The payload read `00485440` then stores strength `+146`: 200
(`0xC8`) for resource 116. The aim `+138..+13C` keeps the allocation's
residue until the first draw. The port leaves it unknown, and a spark
before a draw would stop.

## Update

`00486250` is the shot's `+24`.

1. If the life is at most the delta (JBE, unsigned), the life becomes 0 and
   the shot is disposed. Otherwise the life drops by the delta.
2. `00486100` (`+3C`): if the draw modulo 3 is 0, it throws a spark along
   the aim, `0048A0F0(5, position, velocity, 0)`.
3. `0047AD90` resolves the owner through `+88`. A lost owner disposes the
   shot.
4. With pending packets, the owner's `+58` names who holds the target. The
   vehicle's `00404610` answers the vehicle itself. Other owners have no
   ported `+58` and stop. The target is the holder's `+D0` (`00402790`,
   `+148`). Without a target the update ends here.
5. `004862BA..004862C7` build the packet on the stack: the strength WORD and
   the channel BYTE. Its fourth byte is residue. `00488830(packet, target,
   owner)` delivers it once per pending packet, and each delivery counts
   `+14C` down.

The spark's velocity runs on the x87 at double precision, with each store
an f32. The aim is divided by the f32 spill of its length, cut to 1/4096 by
`ftol`, and normalized again by `00417DA0`. That call reads each pushed
DWORD's low WORD and divides by the unspilled length. The result is scaled
by 3000 and truncated. A zero vector becomes `(1, 0, 0)`.

## Repeat

`004860D0` is the shot's `+98`. Mode 2 adds a pending packet. Modes 1 and 2
restore the life to 600,000 µs and answer 1. Any other mode leaves the shot
alone and answers 0.

## Draw

`00485F90` is the shot's `+2C`, through `004860C0`. It draws nothing.

- In a network game, a shot with flag `0x80000000` is left alone.
- With its owner alive, the shot moves to the owner's muzzle when the
  holder has a behavior: `0048D730(1, +144, owner)`, the weapon group's
  position through `0048CC00`.
- With a target, the target's `+18` refreshes it (it reads settings `+7C`),
  and the aim is the target's `+1C` point minus the shot's position, axis by
  axis. Without a target, the aim is the owner's forward row (basis `+1C`)
  times 2000, truncated.

## Delivery

Every call of `00488830` starts at `0048885A`: `00481030(0)` counts a
category-0 particle firing. The rest depends on the target's type, `+10`:

| Type | Path |
| --- | --- |
| 2, an actor | `0048896E`: `+10` resolves the actor, and a lost one ends the delivery. Then come the flash `00489D90(2, point, zero velocity)` and the damage `0047C6E0`. `004889A6` counts a particle hit with the target's category after the damage. The source is category 0, from the literal 0 that `0047F710` leaves on the stack. |
| 4, furniture | `00488939`: the flash, then `00436020` at the furniture's `+08` X and Z with the packet. |
| 8 and 16, a point | `00488878`: the material is the point's cell high byte `& 3`, and H15/0's DWORD for it is the selector. `00488E00(selector, point, 0x100, load factor)` births the ground burst. Then a draw with `& 3` equal to 0 adds the flash. |

## Port

The emitter builds a shot only when it is of a ported selector (32, the
[Lasers](SELECTOR31_RUNTIME.md)' 31, or the
[grenade](SELECTOR26_RUNTIME.md)'s 26). For another class the arm stays
at its `ActorResource` boundary. A grenade thrown in the group leaves its
handle in behavior `+24`; its `+98` (`004557F0`) answers event 2 with 0, so
the cannon builds a shot of its own. A live earlier shot of any other class
stops at `ShotRetrigger`. The world's
draw lends the emitter its child factory, and settings `+7C` reaches the
shot's draw. The world tick's object walk runs `00486250` for each live
shot. A stopped shot is logged once and is not updated again. A failed
draw stops at the `WeaponFire` boundary.

When the vehicle's firing timer `+38` runs out while a weapon's looping
sound is held, `0048D000` deletes the sound (`00487FE0`) and clears `+28`
and `+2C`. The vehicle's update and its dying tick both own that release.

## Verification

The [update receipts](../../crates/inf-game/tests/fixtures/shot32_updates.json)
hold every `00486250` call of the cannon's shots in Infest01 (one shot, 5
updates) and Infest02 (96 updates, all in world 18 at coarse spacing). The
ledger gives the
capture. Each record keeps the shot's entry and exit state, the RNG, the
spark roll, every spark with its draws, the owner lookup and each delivery
with its packet, its target's eight DWORDs, its draws and, for a ground
target, the selector retail passed to `00489390`.

- `shot32_update_matches_every_recorded_tick` runs all 101 updates from
  their entry state. The sparks, packets, disposal, life, pending count, aim
  and RNG all match.
- `shot32_ground_delivery_picks_the_cells_burst_and_draws_as_recorded` runs
  the world host on the four ground deliveries in the loaded world 18. The
  cell picks the recorded selector, and each delivery draws exactly as
  retail did.
- `shot32_pose_takes_the_muzzle_and_aims_at_the_target` checks the draw's
  aim against those four records, plus the forward row and the network skip.
- `shot32_retrigger_counts_and_restores_life` covers the repeat's modes.
- `shot32_emission_builds_then_repeats_the_groups_shot` runs Infest01's
  emission through the world's child factory. Building the shot draws
  nothing, the group keeps its handle, and the shot takes the velocity and
  the group. After the tail flash the RNG word is retail's. The group's next
  emission repeats the shot instead of building another.

The 11 recorded emissions built 8 shots and repeated 3. 7 of them drew the
tail flash.

Not yet ported:

- The actor (13 recorded) and furniture (2) deliveries follow the same
  composition but have no world-state receipts.
- The owners' `+58` other than the vehicle's.
- Player shot tracking and the other resource classes.
