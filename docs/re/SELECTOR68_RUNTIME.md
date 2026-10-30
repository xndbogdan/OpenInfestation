# Infestation selector 68, the Cranial Implanter

Weapon row 7 (registry kind 28, upgrade `0x33`; see
[weapon pickups](WEAPON_PICKUPS.md)) fires selector 68, resource 129, on the
missiles' class `00455560` (final table `00526398`; the class and its shared
slots are in [the missiles](SELECTOR25_RUNTIME.md)). The player steers the
implant like a guided missile. When it touches a creature, the player takes
the creature over and drives it until a vehicle-mode key brings them back.
The campaign grants the implanter at every respawn once the `0x40` cheat bit
is set ([cheats](PROFILE_RUNTIME.md#cheats)).

## The table

Selector 68 shares selector 28's slots except five:

| Slot | Function | Role |
| --- | --- | --- |
| `+00` | `00461BA0` | Destructor: `00461BD0` restores 28's table `005259E8`, stops a live `+154` sound (`00487FE0`) and runs the actor's `0047AA60` |
| `+24` | `00458C30` | The world's update, which reaches an implant no player holds: `0047CE90` retires it at once |
| `+84` | `00458F30` | The view: the guided missile's `00456CF0` |
| `+A0` | `00458DC0` | Pair acceptance (below) |
| `+AC` | `00458E40` | Lifecycle state 1 (below) |

The shared `+44` (`00457800`, the player-steered flight), `+80`
(`004575E0`) and `+88` (`00457750`) serve every missile, but in the
recordings only the implant reaches them.

## Firing

The firing arm `0048C890` builds the shot through the owner's `+4C`
(`0047AAF0`: `00460310` with the resource, then the shot's `+88` = the
owner's `+84`, `+B4` = the owner's `+B4` and the owner's `+48` basis at
`+44..+68`). Then, at `0048CACA..0048CAFE`, when the owner is a player's unit
(`+2C` bit `0x10`) and the shot has `+2C` bit `0x200`, the owner's player
(`0046B090` of its `+84`) takes the shot (`0046AE90`). The implant is the
only shot with that bit in the recordings.

## Taking a unit (`0046AE90`)

A player takes the actor whose handle it is given when that actor's `+88`
accepts the player's `+4`. The player's `+D8` becomes the handle, and the
camera (`00478E00`) follows the actor (`004825D0`, `00410E70`) unless the
camera is the actor. The given handle equal to `+D8`, or the process null,
is stored without either. The acceptors:

- the missiles' `00457750`: with a camera, it follows the missile, view
  mode 2 (`0040F800`) and camera `+187` = 1; in any case the fuse `+13A`
  becomes 30,000 ms;
- `00405750`, which accepts: selectors 10 (`00520CC8`), 12 (`00527058`), 21
  (`0051F7A8`) and 36 (`00527EE0`), the family `004236D0` base (`00520B84`,
  selector 9), and selector 13 (`005269C4`), which the port does not
  construct;
- the camera's `0040F880` and selector 88's `00444AB0`.

Every other actor's `+88` (`00402720`, or the visual base's `00405140`)
refuses. While the player holds a unit, the player's pre-update
(`0046A0E0`) marks it (flag `0x10`) and `00479040` moves it off the world's
update list; the player's update `0046A260` calls its `+44` with the delta
and the control block (player `+4`) instead.

## The flight (`+44`, `00457800`)

Each call, with the control block's turn `+10`, pitch `+14`, thrust `+18`
and Fire `+1C` (an analog block, byte `+0` set, gives `+4`/`+8` instead):

1. The speed along the nose (the velocity's dot product with the basis row
   `+60..+68`, x87, `00511BA8`), less `(delta >> 10) * speed / 2000`.
   Without a `+154` sound, sound `0x15` starts at the implant (`00487F60`,
   `0x1000`, `0x800`, flag 1); with one, it follows the speed (`00487FC0`:
   `(speed + 0x400) * 16`, `speed * 8 + 0x800`).
2. The turn and pitch rates: `±delta / 16` from the digital block, or an
   analog axis beyond `0x3000`, `(|axis| - 0x3000) * (delta >> 4) / 0xD000`.
   A positive pitch rate shrinks once the nose's height times 4096 is below
   -3000, a negative one once it is above 3000; more than 4000 above the
   ground cell beneath (the H12 byte `<<` the terrain shift), the pitch rate
   grows by `(gap - 4000) * (delta >> 4) / 4000`, up to `delta >> 4`.
3. A launcher (`+88`) that is gone or below lifecycle 3 flashes the world
   (`00487D70`, `0xF00`) and retires the implant (`0047CE90`); the flight
   goes on.
4. The speed approaches its target: 1000, or the speed itself above that,
   or 3000 while thrusting, by `(delta >> 5) * 1000 >> 15` a call. Fire
   spends the fuse (`+13A` = 0) once it is below 28,000 (2 s flown).
5. With a rate: `0047A680` reads the angular velocity `+94` in the
   implant's frame, a rate against its axis doubles, the roll is minus half
   the turn, and `0047B4B0` applies them. Then `0047D440` (`+54`, the world
   up `0055F280`, `delta >> 4`), the angular velocity clamped to ±`0x7FFF`,
   and damped by `((delta >> 4) * |w| >> 15) + 32`, zero below 250.
6. The velocity becomes the speed along the nose (`0047A6C0`), and it moves
   (`0047B1E0`, mask `0x18`).
7. Below the floor (the higher of `00450A30` and the water plane, with the
   world's water flag a fixed wave of it), it rises onto it and the fuse is
   spent.
8. The fuse step `004555B0`, then the exhaust ribbon (`00486600`,
   `004869B0`).

## Touching (`+A0`, `00458DC0`)

The launcher is no contact. Another actor, not retired: when a player holds
the implant (`0046B090` of its `+84`) and takes that actor (`0046AE90`), the
implant clears its flags `0x974`, disposes itself and makes no contact.
Otherwise the contact stands and the missiles' `+A4` detonates it.

## The end (`+AC`, `00458E40`)

Retirement (`0047CE90`: a lost launcher, or the update of an implant no
player holds) takes the implant to state 1; a spent fuse (by time, Fire, the
floor or the shared `+98` events) goes through `0047D670` to state 0, and so
through state 1 first. In state 1 an implant a player holds offers that
player every actor of the actor list (`0055F160` `+48`) with flags `0x24` of
`0xB4` within 350 (squared distance under 122,500), not itself and not its
launcher, in list order: each one the player takes narrows the reach to its
distance. Then the implant disposes itself. From a spent fuse, state 0 then
detonates it as a missile (`00455680`: the radius effect of its block and
the explosion), wherever its player went. A player whose `+D8` no longer
resolves goes back to the vehicle.

## Driving a creature

Selector 36's `+44` (`00481A50`), in order:

1. The water, `00429D60`.
2. The control block's intents (`00428490`), `[turn, forward]`. Thrust gives
   full ahead (`0x7FFFF`) while positive and full back (`-0x7FFF`) while
   negative; without it, the pitch gives full ahead while negative and full
   back while positive; without either, an analog forward axis (`+8`)
   beyond `0x2000` gives the excess times the `+254` table's maximum (or
   minimum, backwards) times the WORD `+28`, `>> 12`, over `0xE000`;
   otherwise the speed along the nose. The turn is `-0x10000` while
   positive, `0x10000` while negative, or the analog turn axis (`+4`)
   beyond 2500.
3. With a weapon behavior (`+140`), its tick (`0048D030`); then, with Fire
   held and the selected entry ready (`+14` zero, `+0` nonzero, `+1C` below
   6,000,000), `0048D0D0` for the held time along the nose row (`+60`).
4. The motion (`+118`, `00481C10`) on the intents, the counters
   (`00481180`), the engine sound near the camera (`00481B10`) and the
   status (`0047C990`).

Its view (`+84`) is the shared actor's `0047F690`, as the death camera's
and every other taker's: the player's `0046BE00`.

## Pilot guided rockets

The missiles' `+44`, `+80` and `+88` serve any shot with flag `0x200`.
Upgrade `0xA4`, "Pilot guided rockets" (H0 `0x1CE`, granted once `0x5E` is
owned), makes `0046B110` set the vehicle's row 4 to registry kind 12,
resource 112: a selector 28 rocket whose payload flags include `0x200`
(kind 11, resource 113, is the plain rocket; kind 13, resource 114, is
`0x1E`'s). The firing arm hands it to the player as it does the implant,
and the player steers it through the same flight; but its contact and state
1 are the missiles' own, so it detonates on what it touches, or when its
fuse is spent, flashing the world while viewed (`00456B2E`). A rocket its
player lets go returns to its own update (`00457370`), which stops its
engine sound (`00487FE0`).

## Release

The player's pre-update `0046A0E0` returns the player to a vehicle at
lifecycle 3 on a vehicle-mode request (`+34`) or a `+D8` that no longer
resolves: it attaches the process null and the camera follows the vehicle.

## Infest04

One full cycle on Artoria (world 11), all one player, at 8 updates a second
(ledger):

| Position | Event |
| --- | --- |
| `5B2587:56C` | `00460310(0x81)` from the vehicle's `+4C` builds the implant |
| `5B2587:D84` | `0048CAF9` gives it the player (`0046AE90`) |
| `5B2E6C`..`5D7EB9` | 114 flights `00457800`, the guided view on screen; meanwhile 114 updates of the VTOL's own `+24` (`00416090`), sinking from 499 to about -1469 |
| `5B2EA9:12F9` | It touches its launcher: no contact |
| `5D7EEE:E2B` | It touches a selector 36 creature; the player takes it (`0046AE90` from `00458E06`) and the implant is disposed (`00458E2B`) |
| `5D83D7`..`5E6387` | 43 updates of the VTOL's own `+24` (`00416090`), standing braked ([a parked vehicle](VEHICLE_DRIVE_RUNTIME.md#a-parked-vehicle-00416090-alive)) |
| `5D83D9`..`5E684E` | 44 drives `00481A50`, five shots (`0048D0D0`) |
| `5E67C7:C79` | A vehicle-mode request (`+34` = 1) with the creature alive: back to the vehicle |

## Port

Built, in [implant.rs](../../crates/inf-game/src/object_runtime/actor/behavior_resource/implant.rs)
and [player_drive.rs](../../crates/inf-game/src/object_runtime/actor/derived/player_drive.rs):

- The firing arm builds selector 68 and hands a `0x200` shot to its
  shooter's player (`WorldActorChildHost::take_shot`). `attach_player`
  holds a missile (`00457750`), takes the `00405750` classes and refuses
  every other actor (`00402720`); the camera's view-mode requests reach a
  missile's `004575E0`.
- The player's update steers a held missile (`00457800`) and drives a held
  selector 36 (`00481A50`) through the class's own water, motion, counters,
  sound and status, and a held selector 9, 10 or 21 through `00424C40`
  ([under the player's hand](FAMILY4236D0_RUNTIME.md#under-the-players-hand-44-00424c40)).
- The vehicle the player left runs its own `+24` meanwhile, braked, and
  matches Infest04's 157 updates, 114 under the implant and 43 under the
  creature ([a parked vehicle](VEHICLE_DRIVE_RUNTIME.md#a-parked-vehicle-00416090-alive)).
  A held scientist (selector 12) walks through its `+44` `00467DA0`: the
  water (`00429D60`), the block's intents (`00428490`) over its own speed
  table `+290` (`0053A080`) where the family hands `+254`, its motion
  `+118` (`00467DF0`) on them and the status; it has no weapon.
- Selector 68's update retires it, its contact and state 1 hand over through
  the world's camera (`WorldImplantTake`), and the missiles' lifecycle
  flashes the world for a viewed missile. The `+154` engine sound is a
  process sound record, which the destructors release.
- The implant's HUD (`+84`, the guided view `00456CF0`) is drawn
  ([the HUD](VEHICLE_DRIVE_RUNTIME.md)), and its own draw `00457620` draws
  nothing while the player holds it.
- The respawn's `0x33` grant under the `0x40` bit stays a boundary
  (`SettingsPickupBoundary`): its grant notice is not delivered on that
  path. A save that owns `0x33` (Infest04's) gives the implanter.
- A held implant shot down near a creature reaches its state 1 without the
  camera's settings and stops at a named error.

Verification:

- `the_implants_flights_match_every_recorded_flight` replays all 114 of
  Infest04's flights from their entries on the loaded Artoria with their
  recorded blocks (thrust, turns and pitches among them): the fields, the
  engine sound's start and gains and the ribbon's points match
  ([`implant_flights_infest04.json`](../../crates/inf-game/tests/fixtures/implant_flights_infest04.json)).
  The flights do not pin the nose's limit, the ceiling, the thrust's 3000
  or the rates' stillness to the unit (each moved by one, every flight
  still matched); `the_flights_limits_hold_at_their_edges` holds each at
  its edge.
- check_selector36_drive.py runs the
  unchanged `00481A50` over 42 supplied creatures and blocks (digital,
  analog over every `+254` table, every fire gate); the port's intents, fire
  decisions and order match.
- `the_vehicles_row_seven_implant_takes_the_player_and_then_a_creature`
  fires row 7 from Artoria's vehicle, steers the implant and hands the
  player a creature; `a_pilot_guided_rocket_flies_under_the_players_hand_and_explodes`
  fires row 4 under `0xA4` and steers the rocket into the ground.
