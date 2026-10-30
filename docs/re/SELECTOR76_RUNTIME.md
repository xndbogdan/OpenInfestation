# Infestation selector 76, the flamethrower

This document owns selector 76, the ACAM's flamethrower (weapon row 3,
registry kind 10, resource 0x82). It is not an actor: `00431ED0` builds it
over the `0047A490` rotating visual with final table `00521A40` and 0x84
bytes. The emitter builds one through `00431EF0`, and the object follows its
thrower, grows a flame forward and goes once the flame is full. The
construction prefix and the behavior damage cache's use of the class belong
to [ACTOR_RUNTIME.md](ACTOR_RUNTIME.md#behavior-registry-resource-classes).

The code lives in
[selector76.rs](../../crates/inf-game/src/object_runtime/selector76.rs),
with the world's services in
[world_actor_update/selector76.rs](../../crates/inf-game/src/session_platform/world_actor_update/selector76.rs)
and the emitter's arm in
[emission.rs](../../crates/inf-game/src/object_runtime/actor/behavior/emission.rs).

## Table

| Slot | Function | Role |
| --- | --- | --- |
| `+24` | `00431FE0` | Update |
| `+28` | `00432490` | Light |
| `+2C` | `004323B0` | Draw |

## Emission

The emitter `0048C890` meets resource 0x82 at `0048CA2C` and calls
`00431EF0` with the owner instead of the owner's `+4C`; it then goes
straight to its tail, with no group handle and no emission velocity.

`00431EF0` allocates the 0x84 bytes, runs `00431ED0` (`+70 = 1`, `+74 = 0`)
and reads resource 0x82 through `004600F0` with the process null and no
position. For a local owner it sends the network notice `00492500(1,
0x19)`, which returns at once offline. A refused construction deletes the
object. Otherwise `+6C` takes the owner's handle and the update runs once
with no time.

The recorded object had radius 800, flags `0x08100003`, the owner's basis
and handle, and its root 108 (the vehicle's model radius) along the forward
row.

## Update

`00431FE0` is the flamethrower's `+24`.

1. **Follow.** `0047AD90` resolves `+6C`; a lost thrower disposes the
   flamethrower. Otherwise `+78` and the position `+32` take the thrower's
   position, and the basis `+44..+6B` the thrower's (its `+48`). The root
   `+7E` is the thrower's position plus its model radius (WORD `+40`) along
   the forward row, each axis truncated (`ftol`). With no time the update
   ends here.
2. **Grow.** `+70` gains a quarter of the microseconds, capped at 0x10000,
   and the length `+74` is `+70 * 600 >> 16`. When it grew by less than 60
   since the last length reached, nothing more happens this tick.
3. **Flames on the ground.** `+70 * 8 >> 16` times: two draws place a
   point at a random depth in the new growth (`draw * growth >> 16`) and a
   random side offset of up to `depth * 320 / 600` (`draw * spread >>
   17`), on the left or right by the depth's parity, 80 below the flame.
   `0047A6C0` turns it by the basis. Where that point is below H12's height,
   it moves up to the ground and a flame is born: `00489D90(9, point, (0,
   30, 0), 0x800)`.
4. **Burns.** From the last length reached, in steps of 60 up to the new
   length: the block at `00532768` (`00532778` online) takes inner radius
   `5r / 4` (`+4`) and outer `6r / 4` (`+6`), where `r` is half the step's
   depth times 320 / 600, at least 60. `004887D0` applies it at the point
   that far along the forward row from the root, with the thrower as the
   source and no replication, and a flame is born there too.
5. The length reached becomes `+74`. At full growth `+70` becomes `0xFFFF`
   and the flamethrower is disposed.

Only the flamethrower reads the two blocks (strength 5000, or 2500 online,
channel 2, impulse 300 or 200) and it writes both radii before each burn,
so the port builds each burn's block from the executable's bytes and the
radii.

## Light

`00432490` splats red `+70 * 127 >> 16`, green and blue `+70 * 80 >> 16`,
radius `(+70 >> 6) + 256`, at the position's X and Z.

## Draw

`004323B0` saves the basis, turns it a quarter in its Y-Z plane
(`0047A720(0x4000)`), and runs the common visual draw `0047AFC0` at the
thrower's position, then at the root. The position goes back to `+78` and
the basis is restored.

The model is H8 110, `exwave2` (resource 0x82's, shared with the
selector-22 blast wave of resource 0x3A): a funnel of blue streaks (edges,
sprite 602) and translucent faces (material 603) whose rings lerp by model
value 1, which `004324E0` (`+14`) answers as `0x10000 - +70`. It is the
flamethrower's shockwave: a white bubble at the muzzle that opens into a
ring ahead of the vehicle as the flame grows. The lab's Terra Nova runs
drew model 110 twice a frame for the flame's 17 frames (16 ms steps,
`0049FEA0` called from `00432417` and `00432433`).

## Port

The emitter builds the flamethrower through the world factory's registry
construction (the child host's `create_registry_object`) and runs its
first update. The world tick's object walk runs `00431FE0` for each live
flamethrower; a stopped one is logged once and is not updated again. The
light pass and the draw are ported; the draw reads the sine table from the
draw host. Online, the network notice is a named boundary.

## Verification

The [update receipts](../../crates/inf-game/tests/fixtures/selector76_updates.json)
hold every `00431FE0` call of the flamethrower Infest03's vehicle fired in
world 6. The ledger
gives the capture.

- `selector76_update_matches_every_recorded_tick` runs the four updates
  from their entry: the no-time one inside `00431EF0`, then three ticks to
  lengths 240, 540 and 600 with 3, 7 and 8 flames on the ground, burns
  every 60 whose radii widen from 75/90 to 200/240, and the disposal. The
  fields, every flame and burn, and the RNG words match; the burns drew
  nothing.
- `selector76_light_glows_with_its_growth` covers the light.
- `the_vehicles_flamethrower_burns_out_in_three_ticks` fires row 3 in world
  6 through the emitter and runs the flame to its end through the world's
  flames and burns.
- `the_flamethrower_draws_its_wave_at_the_thrower_and_at_the_root` runs the
  draw through the object pass: both model packets, the turned own basis,
  and the position and basis restored.

Until 2026-10-23 the object draw's model word and packet had no arm for the
class, so the draw stopped the whole object pass with `NotVisual` (logged
only on a world's first frame) every frame the flame lived: no shockwave,
every object after the flame in the list left undrawn, and the lists left
unswapped, so the next frame's light pass ran no callbacks. With both arms,
Terra Nova's lab scenarios (the idle buggy firing, and a drive with turns,
at 16 ms steps, `--fresh-heap --lockstep-rng`) match retail in every
compared event and every particle birth, and the frames match retail's
but for the wave's blended edges.
