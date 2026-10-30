# Infestation selector 27, the row-12 charge

This document owns selector 27, the charge the ACAM throws from weapon row
12. The class is the [grenade's](SELECTOR26_RUNTIME.md) (`00455560`'s
family) with final vtable `00525848`. It flies as a grenade does until it
meets an actor of another team, sticks to it, and goes off when something
marks it, usually the detonator. Construction and the payload belong to
[ACTOR_RUNTIME.md](ACTOR_RUNTIME.md#behavior-registry-resource-classes).

The code lives in
[charge27.rs](../../crates/inf-game/src/object_runtime/actor/behavior_resource/charge27.rs),
with the grenade's flight, smoke, fuse, messages and detonation in
[grenade26.rs](../../crates/inf-game/src/object_runtime/actor/behavior_resource/grenade26.rs).

## Resource and table

Registry kind 20 builds resource 121 only. Its payload is a 5000-strength
channel-9 block (bytes `88 13 09 00 C8 00 2C 01 20 03`: WORDs 200, 300 and
800 follow), no charges (`+14D` 0) and payload flag 8 (`+14C`). Flag 8 only
spares teammates in the grenade's `+A4`. It has none of the grenade's smoke,
chain, burn or impact flags.

The table is the grenade's but for six slots:

| Slot | Grenade | Charge | Role |
| --- | --- | --- | --- |
| `+00` | `004110E0` | `00458FA0` | Deleting destructor over `004562F0` |
| `+24` | `00455E10` | `00455F30` | Update |
| `+2C` | `0047D910` | `00456180` | Draw |
| `+A0` | `00455610` | `00456010` | Pair acceptance |
| `+C0` | `00405580` | `004560D0` | Writes `+154` and `+158..+15C` (10 bytes) |
| `+C4` | `00405580` | `00456130` | Reads them back, or `+154` = the process null |

`+C0` and `+C4` copy the stuck state into and out of the object state
records that `0047E330` writes (`0047E5C9`) and `0047E600` reads
(`0047EB8D`). The port builds neither record. The constructor `00460A02`
stores the process null `0054E894` at `+154`;
`+158..+15C` stay allocation residue until the charge sticks.

## Update

`00455F30`:

1. Unmarked (`+14E` 0), the fuse `+13A` rises to 1000 if it is lower;
   marked, it becomes 0.
2. The grenade's smoke `004558B0` (nothing without payload flag 1).
3. If `+154` is the live process null, the charge is loose: the grenade's
   flight `004559E0`, then its fuse `004555B0`.
4. Otherwise `0047AD90` resolves `+154`. A lost target marks the charge
   (`+14E` = 1). A live one lends its position: `+32..+36` copy the
   target's. Then, when the target's flag `0x80` and the charge's flag 0
   agree, `00479E90` flips the charge's flag 0: it leaves the draw list
   while its target has `0x80`, and returns when the flag clears.
5. The fuse `004555B0`.

The held fuse loses at most `delta >> 10` before the next update restores
it, so an unmarked charge never goes off by itself. Once marked, its fuse is
0 and `004555B0` takes it through `0047D670` to the grenade's detonation
`004564B0` at once. A stuck charge then bursts at its target's own position.
Marks come from the grenade's `+98` (`004557F0`: event 9, or event 10 from
the thrower or no sender), the lifecycle's state 1 (`00458F80`, lethal
damage), or a lost target in the update or the draw.

## Pair acceptance

`00456010` is the charge's `+A0(other)`. A nonzero AL skips the pair.

1. Younger than 500 ms (`+13C`), it ignores its thrower (`other+84 ==
   +88`): AL 1.
2. An actor of its own team (DWORD `+B4`) is an ordinary contact: AL 0,
   and the grenade's `+A4` then spares it (payload flag 8).
3. Anyone else takes it. `+154` gets the actor's handle `+84`, and
   `+158..+15C` the charge's position less the actor's (WORD wrapping),
   taken into the actor's frame by `00490A80` over the actor's `+48` basis.
   Every actor the pair pass reaches binds `+48` to `004026B0`, its own
   `+44`. The charge's flags lose `0x104`: contact flag 4 takes it out of
   the pair pass, and a change in the low nine bits marks `+38` bit 1. A
   charge without flag `0x80000000` also marks `+39` bit 1. AL 1.

## Draw

`00456180` is the charge's `+2C`. A loose charge, or one whose target is
gone (which marks it), goes straight to the common `0047D910`. A stuck
charge first returns to its target:

1. The offset leaves the target's frame through `004909E0`, and the
   charge's position becomes the target's plus that vector.
2. `00417DA0` normalizes the vector (`n`, its length kept in the x87).
3. The row `r` is the target's Z row (`+60`) when the offset's X word is
   nonzero, its X row (`+48`) otherwise. `v = r × n`, each component
   rounded to `f32` from 53-bit products.
4. `00490370(v, n, 0)` rebuilds the charge's basis: Z = `v`, Y = `n`,
   X = Y × Z normalized, then Y = Z × X. The counter becomes 0.

So the charge sits on the target's surface with its up row pointing out.

## Destructor

`004562F0` calls `0047DD40(+88, 0, 0x12, 0)` before the base `0047AA60`.
A resolved thrower hears 0x12 through `0047DD80`. The vehicle's `00420BE0`
grants upgrade `0xAB` for it only online (`0046B2B0`); offline it answers 0.

## Port

The world tick runs `00455F30` for each charge, with the grenade's flight,
smoke and fuse. The emitter and the world's child factory build selector 27,
and the world contact pass
gives it its own pair class: the charge's `+A0`, and the grenade's `+A4`,
`+A8` and `+68`. The object draw poses a stuck charge before the common
draw; the live process null comes from the draw host. The world's disposal
lends the process to the destructor's message, which the port delivers to
the thrower's `+98`. A reply that would change the world is a boundary, and
so is a resolved thrower outside the world's disposal.

## Verification

`check_charge27.py` runs `00455F30`, `00456010`
and `00456180` unchanged under Unicorn. Its
[fixture](../../crates/inf-game/tests/fixtures/charge27_native.json) has
624 cases: 144 updates (each mode, mark and fuse), 240 acceptances and 240
draws over rotated, scaled and flat bases. In the updates, the grenade's
steps, `0047AD90` and `00479E90` are recorded. The acceptances run the
real `004026B0` and `00490A80`. The draws run the real `004026B0`,
`004909E0`, `00417DA0` and `00490370`. `the_charge_matches_its_native_receipts`
reproduces every case: the steps with the fuse they find, flag 0, position,
`+14E`, AL, `+154`, `+158`, flags, `+38`/`+39`, and the drawn position and
basis bit for bit. No recording throws a row-12 charge.

`a_charge_stuck_to_a_generator_hits_it_with_its_whole_block` throws a charge
through the world's child factory on Tabuchi (world 9). It sticks the charge
to the 0x13 generator, fires the detonator and ticks the charge. The
generator loses `(5000 - 2000) * 2` = 6000, scaled by that world's
difficulty (mission `+10`, 81) to 1898.

A run of the unmodified port shows the whole cycle on the game's own input.
It loads the Tabuchi save and replays these keys (frames at 125 ms):

1. 3 at frame 462: the hovercraft.
2. Right for frames 478 to 481, then single taps at 483 and 485: about
   −173° toward the island.
3. Up from 488 to 542, with one more Right tap at 519.
4. PgDn at 500: row 12.
5. Down from 542 to 548: the hovercraft stops against the 0x13 generator,
   256 from its centre, under "Radiation warning".
6. X held from 566 to 568 (past 200 ms) throws a charge; it sticks to the
   generator's front at once.
7. F7 at 572 selects the detonator. Down from 576 to 580 backs off about
   900, out of the block's 300 reach.
8. Z from 592 to 594 fires the detonator.

The generator flashes and takes 6000 on channel 9, its health falls to 0,
and the HUD shows "New info: Heavy generator". Two seconds later it bursts
into its wreck (model 0x46). The ACAM takes no blast damage. A build that
only adds logging renders the same frames pixel for pixel, and its log gives
these numbers.
