# Infestation selectors 25 and 28, the missiles

This document owns the ACAM's and the launchers' missiles: selector 28 (final
table `005259E8`, resources 112 to 114, weapon row 4) and selector 25
(`00525B88`, resources 110, 127 and 128, weapon row 5 and selector 11's
launchers), both on `00455560`'s family, and the table they share with
selector 68 (`00526398`, resource 129, row 7). The construction prefix and
the payloads belong to
[ACTOR_RUNTIME.md](ACTOR_RUNTIME.md#behavior-registry-resource-classes); the
emitter's arm to [WEAPON_FIRING_RUNTIME.md](WEAPON_FIRING_RUNTIME.md); how
the player gets rows 4 and 5 to [WEAPON_PICKUPS.md](WEAPON_PICKUPS.md).

The code lives in
[missile.rs](../../crates/inf-game/src/object_runtime/actor/behavior_resource/missile.rs),
with the world's services in
[world_actor_update/missile.rs](../../crates/inf-game/src/session_platform/world_actor_update/missile.rs).

## Table

| Slot | 28 | 25 | 68 | Role |
| --- | --- | --- | --- | --- |
| `+00` | `00458FE0` | `004582B0` | `00461BA0` | Destructor |
| `+0C` | `00455720` | `00458380` | `00455720` | Payload |
| `+24` | `00457370` | `00458510` | `00458C30` | Update |
| `+28` | `00457EE0` | same | same | Light |
| `+2C` | `00457620` | same | same | Draw |
| `+3C` | `00457660` | same | same | Remote update (network) |
| `+44` | `00457800` | same | same | Player-steered flight |
| `+80`, `+88` | `004575E0`, `00457750` | same | same | Missile camera |
| `+84` | `00456CF0` | same | `00458F30` | Camera control |
| `+98` | `00456AD0` | same | same | Messages |
| `+A0` | `00455610` | same | `00458DC0` | Pair acceptance |
| `+A4`, `+A8` | `00455640`, `00455660` | same | same | Contacts |
| `+AC` | `00456B10` | same | `00458E40` | Lifecycle state 1 |
| `+B0` | `00455680` | same | same | Lifecycle state 0 |
| `+B4` | `0047CBC0` | same | same | The shared damage reaction |

Selector 68, the Cranial Implanter, has its own destructor, update, view,
pair acceptance and state 1; the player steers it through the shared `+44`
and takes over the creature it reaches ([selector 68](SELECTOR68_RUNTIME.md)).

## Selector 28's update

`00457370` is selector 28's `+24` and the tail of 25's.

1. A live `+154` sound stops (`00487FE0`). Only the unported `+44` writes
   it, so the port stops there.
2. The gap to the ground beneath (`00450A30`) and to the ground a quarter of
   the velocity ahead, below the height a quarter of the vertical speed
   gives, the smaller of the two: under 200, the vertical speed `+90` loses
   the gap less 50, at most `(delta >> 5) * 400 >> 15` down or `(delta >>
   5) * -1000 >> 15` up. The missile holds about 50 above the ground.
3. `0047B1E0` with the position alone (mask 8).
4. Below the ground beneath, the fuse `+13A` is spent; then the fuse step
   `004555B0` (its milliseconds, the age `+13C`, and at zero `0047D670(0,
   +84, 0)`).
5. The exhaust ribbon `+14C`: the first update creates it (`00486600` with
   the missile's handle) and lays three points from a quarter of the velocity
   behind the missile to the missile in eighths (`004869B0`); each later one
   lays the position.

## Selector 25's update

`00458510` homes on a type-2 target `+160`, then runs `00457370`.

1. Without a target, `+158 = 0x7FFF` and `004583A0` acquires one: the old
   target goes, then the launcher's `+58` (`00404610`, itself, for the
   vehicle and selector 11) holds a target at `+D0` (`00402790`: `+148`). A
   type-2 target on an actor that is neither the missile nor one of the
   missile's owner's is cloned (`004828D0`). Otherwise a scan (`00475EC0`,
   flags `0x24`, 2000 in XZ) with `00458070`'s evaluator (table
   `00525B80`) builds a type-2 target (`004825D0`) on the best candidate.
2. With a target: its refresh (`+18`, `00482690`), the distance to its focus
   (the integer root of the signed WORD squares, quartered and doubled when
   bit 31 is set) and the closing speed (the relative velocity along the
   normalized line of sight, `00417DA0`). Closer than 150 and no closer than
   last time (`+158`, signed), the fuse is spent; `+158` takes the distance.
3. A slow missile (`+15E`) narrows its speed limits: the low one to the
   speed less the closing speed plus 200, at least 200; the high one to 500
   or `700 - age / 10`, at least the low one plus 200. Otherwise they are
   1500 and 3500.
4. The time to the target, `distance * 1000 / closing` milliseconds while
   the distance is under twice the closing speed, else 2000 (at most 2000).
   Above 50 ms the target is predicted that far ahead by its velocity (`+1A`,
   truncated by the reciprocal of 1000). Above 500 ms the aim rises by
   `(time - 500) >> 3` (250 from 2500 ms), at most half the distance.
5. The desired speed: under 2000 ms, the target's velocity along the
   missile's forward row plus the distance plus 400; else the high limit.
6. The turn: the offset in the missile's frame (`0047A660`), yaw and pitch
   from `0048F520` of `0048EF30`'s ratio over the distance (a target behind
   turns a quarter more), each at most `delta >> 4`; the larger turns whole
   (`0047A730` for yaw, `0047A720` for pitch, negated) and the smaller half.
7. The speed `+15A` eases toward the desired one within the limits, by
   `(delta >> 4) * difference >> 15`, and the velocity becomes the forward
   row times the speed (`ftol`).

`004580B0`, the scan's score: nothing below lifecycle 2, for the launcher
or on the missile's side; nothing more than 128 off the ground cell without
`+15C`, nor under 128 without `+15D` unless the candidate's `+64` (only the
vehicle's answers) is in mode 4 or 5; nothing behind the missile or outside
60 degrees of its forward row (the square of the forward distance must
exceed a quarter of the squared distance, unsigned). Otherwise `0x7FFFFFFF`
less the lateral square, a quarter of it off for the evaluator's `+8`, the
process null. The scan keeps the strictly best score.

## Light, draw, messages and contacts

`00457EE0` splats at the missile's X/Z within 1500 of the ground cell
beneath: red `(1500 - h) * 20 / 1500`, green and blue `(5h - 7500) /
1500` (below zero, so their bytes wrap), radius `h * 500 / 750`, at least
256. `00457620` is the common draw, except for a live missile flagged
`0x10` (a player's unit, `0046A0E0`) that a player holds (`0046B090`),
which draws nothing: its player looks out of it.

`00456AD0` sets `+150` for events 0xB and 0xC and answers 1; otherwise
`00455740` spends the fuse for events 5 and 9 and answers 1, and answers 0
to the rest (the emitter's retrigger among them). `00455610` ignores the
launcher for 500 ms, as the grenade does. `00455640` and `00455660` detonate
an armed missile (`0047D670(0, +84, 0)`); the furniture contact answers 0
without the shared response.

## Lifecycle

State 1 (`00456B10`) flashes the world for a viewed missile (`+D0`,
`00487D70` with 0xF00) and goes on to state 0 (`0047D670(0, +84, 0)`).
State 0 (`00455680`), unless retired: the radius effect of its block `+13E`
from its owner (`004887D0`, replicated), the explosion (`0048A0F0(2,
position, 0, 0)`) and the disposal. The destructors free selector 25's
target and stop a `+154` sound. Only the missile camera sets `+D0`, so a
flash is a named boundary.

## Emission

Kind 15 (row 5) carries registry flag 8: `0048C890` launches it along the
owner's `+48` basis (`004026B0`, `+44`), its row at `+10` times -128.0
(`00520A2C`), each axis truncated, plus the owner's velocity; neither the
node's direction nor the row's speed. The missile's first update replaces
that velocity. The child factory builds selectors 25 and 28.

In Capture the Banner, selector 94's launcher builds selector 25 directly
([its zones](CONSTRUCTOR_ASSETS.md#placed-selectors-the-port-does-not-construct)):
`00458A50` (at `00445A5D`) allocates the `0x164` bytes, constructs them
(`00458250`) and registers them (`004600F0` with the null handle; a failure
deletes the missile), places it at the given position, and when it has no
follower at `+160` yet gives it one (`004825D0` on the target handle) and
turns its basis toward the target (`00490370` at `0055F280`, the offset
normalized, or +X when it is zero); then its `+24(0)`. `004600F0` runs the
generic initialization `00460120` on H13 `0x6E`, which is selector 25, with
no position, so the port builds that resource through the child factory
and places and turns the missile after (`place_missile25`). Its first
update reads the last distance `+158`, which nothing has written, but only
a target nearer than 150 lets that value decide, and the launcher's missile
starts 1500 out; the port reads `+158` only there.

## Port

The world tick runs selector 25's and 28's updates through the grenade's
world host, and the lifecycle (from the update, a contact or lethal damage)
through `MissileStates` over the world's damage owners. The light pass, the
draw, the messages, the contacts, the damage reaction and the destructors
are ported, and so are the player-held slots (the steered flight `+44`,
the view mode `+80` and the hold `+88`, with `+D0`), which a pilot guided
rocket and the [Cranial Implanter](SELECTOR68_RUNTIME.md) reach, and the
guided view `+84` ([the HUD](VEHICLE_DRIVE_RUNTIME.md)). The remote update
`+3C` is a named boundary.

## Verification

The [missile receipts](../../crates/inf-game/tests/fixtures/missile25_updates.json)
hold every call of selector 25's table in Infest03's world 23, where
selector 11's launchers fired three missiles
(ledger).

- `missile25_update_matches_every_recorded_tick` runs the 28 updates from
  their entry on the loaded terrain. Two missiles inherit the launcher's
  target, home on it and are spent by a close pass; the third finds nobody
  and flies straight. The world's vehicle stands in for the target and a
  selector 11 for the launcher, so handles compare by role. The fields, the
  target's refresh, the ribbon's points, the detonations and the RNG match.
- `missile_light_matches_every_recorded_call` matches the 26 lights and
  `missile_detonation_matches_both_recorded_calls` both detonations.
- `missile25_score_matches_the_native_evaluator` matches 257 runs of the
  unmodified `004580B0` in Unicorn
  (`check_missile_score.py`), every early
  out, the cone, the lateral square and the process null's quarter. The
  recording's seven scores are all early outs.
- `the_vehicles_row_four_rocket_hugs_the_ground_until_a_slope_takes_it` and
  `the_vehicles_row_five_missile_flies_out_its_fuse` fire rows 4 and 5 from
  Level 1's vehicle through the world's services to their detonation.

A Level 1 probe granted `0xA3` (selecting primary 5 with four missiles) and
fired: the missile left the vehicle, laid its ribbon and glow down the road,
and the HUD counted three left.
