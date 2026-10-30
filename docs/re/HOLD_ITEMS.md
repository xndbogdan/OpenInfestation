# Infestation hold items

Factories sell two kinds of product. Upgrades (kind 9) are grants, which the
port owns ([factory production](FAMILY40D800_RUNTIME.md#selector-15-factory)).
The others are objects a factory builds and loads into the vehicle's hold.
The port builds them; of their classes, the Bomb, the mining rig and the
ECM Unit are ported. This document records what REA
recovered about them on 2026-10-12; it is static evidence, not observed
play.

## The factory's item arm

Production `0042E570` grants an upgrade and returns. Any other product goes
to `0042E89E`, which the port's [factory](../../crates/inf-game/src/object_runtime/actor/selector15/factory.rs)
runs (`factory_items`, 2026-10-19). For each of `+18E` items, with the
docked player's unit (`0046AE60`):

1. The position is the dock point `+190..+194`. When the unit's vehicle view
   (`+64`) has fewer free hold slots (`004120E0` over its `+1A4`) than
   `+18E`, one RNG draw (`0048F060` at `0042E8FA`) gives a distance
   `d = ((r & 0xFFFF) >> 9) + 200` (200 to 327), and the point moves back
   by `d` along the factory's basis floats `+60/+64/+68`, each product
   truncated (`00511BA8`).
2. The factory's own `+4C` (`0047AAF0`, the shared child factory) builds
   resource WORD `+184` there (`0042E955`).
3. A built item is marked dirty and joins team 0 (`+B4 = 0`). When the unit
   has a vehicle view, `+64(item)` and `00421810` on its result load the
   item into the hold (`0042E981..0042E996`). `00421810` asks the item's
   `+5C` view only when its team is nonzero after the slot's `+8C` attach,
   which only the Prison Unit's attach (`0046E530`) sets; every hold item
   binds `+5C` to `00401300` (NULL), so the team change `00468C40` is
   skipped. The port answered a scientist's view alone and stopped the
   factory at a bought Prison Unit with room in the hold (a
   session of 2026-10-21 on Kankutan); it now answers NULL for the Prison
   Unit, the Bomb, the mining rig, the Remote Beamer, its receiver and the
   ECM Unit, read from their tables.
4. After a built item of resource 7, `+184` becomes `0x25` for the rest.

The factory's final table `00521940` binds `+4C` to `0047AAF0` and `+48`
(the child's source basis) to `004026B0`, as the shooters' tables do. The
[tests](../../crates/inf-game/src/object_runtime/actor/selector15/factory_tests.rs)
buy a Bomb from Level 1's truck factory, which builds it at the dock point
and loads it into the hold, and run four Remote Beamer items against a
hold of three free slots: from the RNG's zero seed every point moves back
along the factory's +Z by 200, 248, 204 and 217, the last three are
receivers, and the fourth stays outside. The Remote Beamer and its
receiver are built ([below](#remote-beamer-selectors-55-and-56)), and so
is the Prison Unit ([below](#prison-unit-selector-17)).

## Products

The product records (`0042F250`, parsed by
[`factory_shop.rs`](../../crates/inf-formats/src/factory_shop.rs)) name the
item kinds. Each resource resolves through `1.ovl`'s H13 table:

| Record | Grant | Resource | Selector, constructor | H0 name |
|---|---:|---:|---|---|
| `00532164` (common) | `0x1F` | 111 | 29, `00457FE0` | `0x1F6` Bomb |
| `00532178` (common) | `0x23` | 32 | 52, `0047A7A0` (vtable `00526714`) | `0x200` Portable mining rig |
| `005322F4` (list 0) | `0x80` | 49 | 73, `00450400` (vtable `005262CC`) | `0x2B0` ECM Unit |
| `00532308` (list 0) | `0x6D` | 7, then 37 | 55, `00473D10`; then 56, `0047A7A0` (vtable `00526530`) | `0x2A6` Remote Beamer |
| `0053231C` (list 0) | `0x71` | 13 | 17, `0046E120` | `0x2AE` Prison Unit |

Their descriptions: the Bomb is "a cannister of highly explosive material";
the mining rig, beamed "down near a resource mine, ... will extract
crystals"; the Remote Beamer "will beam people to a remote receiver unit when
they walk within range", which is why its second item is resource 37, the
receiver; the Prison Unit "will capture an individual person if they stroll
too close"; the ECM Unit, "when carried in your hold, ... will disrupt enemy
radar so they cannot get a solid lock on you".

## ECM Unit, selector 73

Case `0x49` of `004603B0` allocates 0x14C bytes, runs `00450400` (the shared
`0047A7A0` actor base, `+140 = 0`, vtable `00525030`), installs vtable
`005262CC` and stores the process NULL at `+144`. The class overrides five
slots of that base:

| Slot | Address | Behavior |
|---|---|---|
| `+24` | `00450450` | Settles. While its position equals the copy at `+138..+13C`, spins `+8E` and `+92` are zero and `|+90|` is at most 32, it adds the delta to `+140`; past 499,999 µs it sets flag `0x4000000` and stops updating (no `0047B1E0`) until something moves it. A set flag `0x400000` instead zeroes `+140` and clears `0x4000000`, `0x400000` and `0x200000`, as does moving, which also recopies the position. Each nonzero spin word then decays by `(spin * (delta >> 5)) >> 14`, snapping to zero within ±32, before `0047B1E0(delta, 9)`. |
| `+28` | `0042DAE0` | A pulsing light: its phase `+148` advances by the clock `0055F0E0`, and three sine-table reads 120° apart give the colour of a `0045B1A0(0x400, rgb)` glow. |
| `+8C` | `0042D9E0` | Attach/detach through the base `0047D7C0`. Attaching stores the holder's handle in `+144`. Detaching clears the holder's flag `0x40000` (dirty on change) and `+144`. |
| `+90` | `0042DA60` | Sets the holder's flag `0x40000` (dirty on change). |
| `+AC` | `0042DA90` | Clears the holder's flag as in a detach, then `00479FA0`. |

Flag `0x40000` on the vehicle is the ECM. Two readers, the only ones a
sweep of the executable for the mask finds (`sweep_instructions.py`):

- The target refresh `00482690` copies a target's velocity `+8E..+92` for
  aim lead, but negates it when the target has `0x40000`, so shooters lead
  the wrong way. The port's refresh already does this
  ([`missile_tests.rs`](../../crates/inf-game/src/object_runtime/actor/behavior_resource/missile_tests.rs)).
- `00420080` (the vehicle's light callback) adds the clock to `+588` and
  lights the ground through `0045B1A0` while the flag is set.

Placement 161 on Helniazine (leaf 16) is the only authored one: a shared-actor
payload with flags `0x167`, health 100, `+08` 10, grant `+38` = `0x80` and
`+40` = `0x2B0`. In Infest04 the buggy left it by the mutants' gate, and the
dropped scientist's grant target `+27C` holds it, which is why that
scientist's walk differs in the port
([selector 30](SELECTOR30_RUNTIME.md)). The selection rule that makes it a
grant candidate is in [target selection](FAMILY_TARGET_SELECTION.md#grants-and-canonical-interactions).

### In the port

The ECM Unit is [built](../../crates/inf-game/src/object_runtime/actor/selector73.rs)
(2026-10-14), because Infest04's player picks it up on Helniazine twice
(`ecm-pickups.js`): the take at `7857E9` and `7B226C` (from the vehicle
update's pickup, `0041789B` into `00421810` and `0041232D`), the drops at
`7A07ED` and `7BB2C9` (`00412880`), and 54 hold ticks between, each giving
the vehicle the ECM flag (its flags went from `0x4A106937` to
`0x4A146937`). The class keeps the shared actor's table but for its six
slots: it is built from placement 161 with the shared initialization and
commands, answers terrain through `0047D2F0`, furniture through `0047BFD0`
and pairs as the shared prefix. The vehicle's take, hold and drop run its
`+8C`, `+90` and release; its `+AC` releases the holder before the shared
disposal.

Its update runs every tick from where the update last saw it. The
constructor leaves `+138..+13C` unwritten (Infest03 and Infest04 both held
`[-5383, 0, 16000]` there), so the first update finds the unit moved and
copies its position, as both recordings did. At the recordings' 125 ms
updates it never settles: each update damps its rebound of 44 to 34,
gravity takes 122 and the terrain contact sends it back up at 44, which is
never within the 32 that counts as still. The port matches all 93 of
Infest03's updates and 31 of Infest04's 574 (its first 16 and those where
the rest or the flags change), and its glow matches Infest03's 100
(`ecm-receipts.js`,
[`ecm_unit_infest.json`](../../crates/inf-game/tests/fixtures/ecm_unit_infest.json)).

Two fields stay residue. Nothing writes the glow's phase `+148` before
`0042DAE0` adds the world's delta (`0055F0E0`, which the world tick
`00478F20` stores at `00478F3E`); retail held `0x37C0000` in Infest03 and
`0x21` in Infest04. The port starts it at zero
(`EcmGlowPhaseApproximation`), as it does the vehicle's pulse `+588`
(`VehiclePulsePhaseApproximation`): only where the colour's cycle starts
differs. The port's frames can draw without a world tick, or after two,
so each tick adds its delta and the light pass takes the sum
(`WorldSession::light_delta`); at one tick a frame, as retail ran, it is
that tick's delta.

## Bomb, selector 29

Case `0x1D` of `004603B0` (`00460AA5`) allocates `0x150` bytes and runs
`00457FE0`: the family base `00455560(2000)` of the grenades, mines and
missiles, then the Bomb disarmed (`+138 = 0`) with its fuse length `+14C`
copied from `+13A` (2000), and vtable `00525AB4`. Its table is the
missiles' `00525B88` in its contacts (`+A4 00455640`, `+A8 00455660`) and
its state-0 base (`00455680`), and the shared actor's elsewhere, but for
seven slots:

| Slot | Address | Behavior |
|---|---|---|
| `+04` | `00458C40` | Codes up to 15 (signed) are the shared `0047ADA0`'s; 16 stores a DWORD at `+14C` and its low WORD at `+13A`, the fuse; higher codes refuse. |
| `+0C` | `00458CA0` | The family's `00455720` (base initialization, the block copy to `+13E`), then `00479DD0` drops it onto the ground under it (`00450A30`). |
| `+24` | `00458C80` | Only while armed (`+138`): the family fuse `004555B0` counts `+13A` down by the delta's milliseconds and `+13C` up, and at zero goes to state 0 (`0047D670(0, +84, 0)`). |
| `+8C` | `00458CC0` | A take (nonzero low byte) disarms it, refills `+13A` from `+14C` and names the holder its owner `+88`; the first player then sees hint `0x1A` (a drop: `0x19`, `0046C760`, not silent), then the shared `0047D7C0`. |
| `+98` | `00458010` | Message 9 arms it, as does 10 from a live sender (`+C4` at least 3) whose handle is its owner; arming (unless flag `0x80`) spends the fuse, so the next update sets it off. Online, a 10 from no sender or a dead one disposes of it. It answers 0. |
| `+AC` | `00458F60` | State 0 with its own handle. |
| `+B0` | `00458DA0` | Its visual `+148` (0x36) through `+4C` (`00458D20` pushes the WORD in a register whose high half is residue), sixteen type-1 debris round a type-1 centre (radius 20, speed 8), then `00455680`: unless flag bit 31 its block `+13E` from its owner, the explosion (`0048A0F0(2)`) and the disposal. |

A carried Bomb keeps the shared actor's `+90`, `004026C0` (`MOV AL, 1`),
so the hold (`00412421`) keeps it and does nothing else with it; the port's
hold had stopped there (`0041243F`) when a player beamed one up. So do the
mining rig (`00526714`), the Remote Beamer's receiver (`00526530`) and
every registry class (the grenades, mines, missiles and shots: selectors
25 to 29, 31, 32, 38 and 68). Of the actor tables the notes name, only
selector 12's (`004688A0`, the scientists' repair), the ECM Unit's
(`0042DA60`), the Alien Brain's attack and selector 85's (`00464930`) and
the Prison Unit's (`0046E640`) bind their own `+90`. The port's hold keeps
the rig and the registry classes too. A game had stopped there
again after beaming up "the mine" in Helniazine (2026-10-18): the class
was one of these two, as nothing else changed, and their session's replay
now plays past it.

So a Bomb lies where it is put and does nothing until armed. The
detonator (row 6) calls the actors within 4000 with event 10 (`0047DD20`),
which arms the Bombs the vehicle owns; a vehicle's cleanup (`00415BA0`)
sends 10 from the dead vehicle, which arms none. An armed Bomb that touches
anything goes off too. Taking a Bomb into the hold claims it (its owner
`+88` becomes the vehicle), so the detonator sets off a Bomb the vehicle
took and dropped: in the lab (tampered-run:
`runtime_re/lab/scenarios/bomb-detonate-dropped.json`
(world 6's first start moved onto a Bomb, every upgrade but Sonic Shock))
the Bomb goes off the update after the detonator, and the port matches
retail in every compared event. Fired with the Bomb still in the hold
(`bomb-detonate-held.json`), the detonator sets nothing off, then or after
the drop, in both. With Sonic Shock (`0x26`) the detonator sends its pulse
`0048D980` instead: event 9 to the actors within 4000 of the vehicle, which
arms every Bomb in reach whoever took it (and sets off grenades, mines and
missiles), the map's jitter (`G+1B4` `+24`) at `0x1FFF` and the world flash
at the vehicle (radius `0xF00`). [The detonator](SELECTOR26_RUNTIME.md#messages)
covers it; the lab (tampered-run: `runtime_re/lab/scenarios/bomb-sonic-shock.json` (world 6's first start moved onto a Bomb, every upgrade owned)) sets off a Bomb never taken, and the port
matches it.

The port [builds it](../../crates/inf-game/src/object_runtime/actor/behavior_resource/bomb29.rs)
for Altair Prime's three placements and Helniazine's two, and for the
child factory. check_selector29.py runs its
bodies over 49 cases (`selector29_native.json`), with the real fuse and
ground height; the port matches every command, drop, update, capture,
message, death and state 0. Helniazine's two lie 921 apart, inside each
other's blast (outer radius 1200, strength 20000 on channel 9): the
vehicle's own, set off by the detonator, kills the other, which goes off
in turn.

Infest04 shows that chain
(`bomb-life.js`,
`.tmp/infestation-ttd/20261015-bombs/bombs04.txt`). Both Bombs hold health
1 of 1, disarmed, with the fuse at 2000 and no owner, and update 41 times
each. Then the player sets off a laser mine beside the pad (its state 0
`004566A0` at `6B4F45:1C2B`, at (38521, -1198, 19200)): the Bomb 421 away
goes from state 3 to 0 at `6B4F4C` (its death `00458F60` with the
vehicle's handle `CC50002`, the mine's owner, as source) and the other at
`6B4F52` (source 0, the first Bomb's owner). Seeded with Infest04's RNG at
the mine's last update (`0x01CA146A` at `6B4F45:E97`), the port's tick
queues the same 13 furniture records in the same order: the mine's two
pylons, then the first Bomb's post, nine fence segments and corner
([the test](../../crates/inf-game/src/session_platform/world_actor_update/grenade26_tests.rs)).

## Portable mining rig, selector 52

The [mining rig](ACTOR_RUNTIME.md#selector-52-mining-rigs) is built for its
world placements: a course target's links hold it, and while held it makes
the resource mines within 2000 drop their crystals. The vehicle beams a
placed rig into its hold and out again through the rig's `+8C`
(`004547E0`, ported 2026-10-18 after a vehicle stopped taking
one): a rig that holds a level refuses with the first player's hint
`0x18`; an empty one gives its taker hint `0x1F` and grant `0x23` before
the shared body `0047D7C0`. A rig dropped where a mine (selector 53)
answers its call within `0x400` (`0047DD20(0x400, 0xF, 0)`) starts
filling, and that mine arms and drops its crystals until its count runs
out.

## Prison Unit, selector 17

Case `0x10` allocates `0x160` bytes and runs `0046E120(0x300)` over the
shared `0047A7A0`: the radius `+138`, table `005272A0`, and a one-slot hold
at `+144`, the vehicle's hold class (`00412880`, `004121D0`, `004122C0`,
`004122A0`) with byte `+0C` set. Its slot (`0052736C`) places a held actor
at the unit through `00412740`, whose offset (`0054ED4C/50`) is zero BSS,
and gives it the unit's basis.

| Slot | Function | Does |
| --- | --- | --- |
| `+14` | `0046E200` | query 1: the charge `+156`; query 2: armed `+154` |
| `+24` | `0046E230` | the update: the arming delay `+13C` (1 s; 5 s after a reset) runs down and arms it. With its hold full (`004120E0` finds no free slot) it charges `+156` toward `0xFFFF` by `delta >> 3`; empty, it lets go of a team's task source it held and drains the charge, and when armed, every `(rng & 0xFFFF) * 8 + 1 000 000` microseconds (`+140`), it scans (`00475F50`, radius `+138`, the filter below) and takes the find into its hold with its own handle as source: it keeps the person's team `+158` and its `+7A` as the kind `+15C`, and when the person's view owns grant `0x15` (`004752E0`) holds it (`+15E`) as that team's task source (`00482C60(own, 1)`). The hold then runs (`00412380`) |
| `+8C` | `0046E530` | attach and detach (`0047D7C0` first): attached, it takes the carrier's team and flag 2 and posts message `0x10`; holding a person, the carrier gets it (`00482C60`, message 4 `0x13E` through `0047DD40`), or on detach it keeps it; else a player carrier with a kept kind is granted that person (`004750C0`) |
| `+90` | `0046E640` | carried, its own hold runs (`00412380`); a unit that held a prisoner its hold has lost lets go of the task source as itself and as the holder (`00482C60` twice) |
| `+98` | `0046E4A0` | message 1 drops the hold's slot (`00412880`), disarms it, forgets the kind and the holding, and arms it again after 5 s; it answers 0 |
| `+00` | `0046E6F0` | scalar deleting destructor (`0046E710`: `00411E30`, `0047AA60`) |
| filter `+4` | `0046E450` | scores only persons (a `+5C` view) of team `0x12`, nearest first (`-1 - distance`) |

[`selector17.rs`](../../crates/inf-game/src/object_runtime/actor/selector17.rs)
builds it (2026-10-19), with the vehicle's hold code taking either owner's
slots. check_selector17.py runs the update
over 50 states with an empty hold and actor list, the query, the filter
and the message, and the port matches every one. A Level 1 test buys the
unit with the hold full, so it stays beside the factory; a second's
updates arm it, and the same update's scan takes a scientist of team
`0x12` standing `0x200` away: a flash where it stood, then the hold puts it
at the unit. Two paths stop with typed errors, as nothing the port reaches
takes them yet: message 1 with a prisoner in the slot
(`ObjectError::PrisonerDrop`), and a carried unit whose prisoner has left
its hold while it held the task source (`0046E66C`).

## Remote Beamer, selectors 55 and 56

Case `0x36` allocates `0x148` bytes and runs `00473D10(0x200)`: the shared
`0047A7A0`, the charge `+138` zero, the range squared `+144` (`0x40000`)
and table `005274C4`. That table is the shared actor's but for five slots:

| Slot | Function | Does |
| --- | --- | --- |
| `+00` | `00473FE0` | scalar deleting destructor (`00474000`: the shared `0047AA60`) |
| `+04` | `00473F70` | codes through 15 are the shared `0047ADA0`; 16 sets the range squared from a WORD |
| `+14` | `00473D50` | query 1 is the charge; any other DWORD comes back as asked |
| `+24` | `00473D70` | the update (below) |
| `+8C` | `00473FB0` | the shared `0047D7C0`; an admitted attach empties the charge |

The update adds `((delta >> 10) * 0x6000) / 20000` (the multiply-high
`0xD1B71759`, `>> 12`) to the charge while it stays below `0x6000` less
that step (signed). Full, it sets `+138 = 0x6000`, `+140 = 0` and `+13C`
to the process NULL, and runs scanner B (`00475F50`: flags `0x60` set and
`0x80` clear, X/Z within `0x3FFF0001` squared, not itself, the strictly
best score) with the filter `00473EE0`. A receiver (resource `0x25`)
nearer than `+140`, or the first, becomes `+13C` at distance `+140` and
sets flag `0x10000000`, scoring nothing; anything else, a farther receiver
included, scores `-1 - distance` when its `+5C` view lies within the range,
shares the beamer's identity `+6C` and is not of team `0x12`. With a find,
an unresolved receiver clears the flag. Otherwise the person is beamed:
the charge empties, the person's view's `+22E` takes bits
`0xC` and team 2 (`00468C40`), a flash `00489A90(2)` marks where it stood,
it takes the receiver's position, a second flash marks its arrival, and
its grants `+24C` learn `0x6D` and `0x6E` (`004750C0`).

Case `0x37` allocates `0x138` bytes for the receiver: the shared
`0047A7A0` and table `00526530`, the shared table but for its destructor
`004110E0` (the shared body through `004854B0`).

[`selector55.rs`](../../crates/inf-game/src/object_runtime/actor/selector55.rs)
builds both (2026-10-19). check_selector55.py
runs the update over 121 charges and deltas (102 reaching a full charge
with an empty actor list) and the filter over 60 candidates, and the port
matches every one; the receipt found the farther receiver's fall-through
into the person arm. A Level 1 test buys the Beamer with the hold full, so
both items stay where they are built, and a full charge beams the scientist
beside the beamer to the receiver. Only selector 12's table among those the
port builds answers `+5C` (`00404610`); the rest bind `00401300`, as a test
checks. The `+04` code 16 is not ported; nothing the factory builds sends
one.

## Remaining work

The Bomb, the mining rig, the ECM Unit, the Remote Beamer and the Prison
Unit are built for their world placements and for the factories. The
Prison Unit's prisoner drop (message 1 with a full slot) and its carried
release (`0046E66C`) stop with typed errors.
