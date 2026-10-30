# Infestation selector 31, the Lasers

This document owns selector 31, the ACAM's Lasers: weapon row 2, registry
kind 8 (resource 115, flags `0x02`), unlocked by upgrade `0x98` ("Lasers",
"A constant particle stream of proto-ions"; `0x99` is the Laser power-up and
`0x9A` the Laser guidance system). Its class `00485480` derives from the
cannon's shot `00485F50` ([selector 32](SELECTOR32_RUNTIME.md)): it sets
the final table `00528524`, zeroes the angle WORD `+150` and the age DWORD
`+154`, and puts channel 8 in `+148`. The construction prefix belongs to
[ACTOR_RUNTIME.md](ACTOR_RUNTIME.md#behavior-registry-resource-classes).

The code lives in
[laser31.rs](../../crates/inf-game/src/object_runtime/actor/behavior_resource/laser31.rs)
(the reach and the beam),
[shot32.rs](../../crates/inf-game/src/object_runtime/actor/behavior_resource/shot32.rs)
(the shared update, repeat and pose) and
[software_scene/laser.rs](../../crates/inf-game/src/software_scene/laser.rs)
(the beam's quads on the scene).

## The class

The table is the cannon shot's but for four slots:

| Slot | Laser | Cannon shot |
| --- | --- | --- |
| `+24` update | `00485500` | `00486250` |
| `+28` light | `00401310` (RET) | `0048C850` (RET) |
| `+2C` draw | `00485E60` | `004860C0` |
| `+3C` | `004854C0` | `00486100` |

The emission, the repeat `+98` (`004860D0`), the group store through `+6C`
and the delivery `00488830` are the cannon's. The payload gives strength
`+146` 210 (`0xD2`). Each laser shot lasts 600 ms unless a repeat restores
it; holding the fire key repeats it every emission, adding one packet each
time. The ACAM's two weapon groups each hold one laser.

## Update

`00485500` adds the delta to the age `+154`, then runs the cannon's
`00486250`. That calls the laser's `+3C`, `004854C0`, in place of the
cannon's spark: the angle `+150` turns by a tenth of the delta (`MUL
0CCCCCCCDh`, `SHR EDX, 3`), backwards when the weapon group `+144` is
nonzero. The update draws no RNG.

## Draw

`00485E60` runs the shared pose `00485F90`: the laser moves to its group's
muzzle and aims at the owner's target, or 2000 along the owner's forward
row. In a network game a laser flagged `0x80000000` skips the next step.

The reach then scales the aim, each WORD times a factor, `SAR 15`. While
the age divided by 5 is below `0x7FFF` (the first 164 ms) the factor is
that fifth. After that, the factor is half the life, once the life falls
below `0xFFFE`, so the beam grows from the muzzle and shrinks back in its
last 65 ms. `0048FE60` moves the context to the laser's position, and
`00485520` draws the beam.

`00485520`:

1. The sprite is H3 slot `0x24E`, or `0x116` for a negative strength; a
   slot past the sprite count gives none.
2. Under a `setjmp` frame, the aim's length is `0048EFC0` of the sum of
   squares (a sum past 31 bits is quartered and its root doubled), and
   `0048F080` makes the aim a Q31 unit `n`. Two Q31 cross products give `u`
   and `v`, square to `n`, each product `(a * b) >> 31`.
3. Two strips of sixteen segments each start from the projected origin
   (the context's `+50` projector at `(0, 0, 0)`). Segment `k` ends at
   `aim * k / 16` (signed, toward zero) plus:
   - in the first strip, three RNG draws, Z then Y then X, each adding its
     top four bits less 8;
   - in the second, a winding of radius `(k - 1) * length / 128` up to the
     middle and `(16 - k) * length / 128` after, at the angle: `rc * u + rs
     * v` with `rc` and `rs` the radius times the sine table's cosine and
     sine, `SAR 15`. Each segment turns the angle by `0x1800`, or `-0x1800`
     in group 0.
4. A segment draws when `00528BD0` admits the OR of both ends' clip bytes,
   and not both ends' fog is `0xFF`. Without fog the quad is `0040EF30`'s
   (3 blocks), else `00411D50`'s (4 blocks, the context's far colour at
   `+2C` and the fog bytes at `+30..+33`). Its sort key is the previous
   point's projection, through the renderer's material sorting. The packet
   has mode 4 at `+20`, the sprite at `+24` and the effect colour
   (`00490360`) at `+28`.
5. The quad's corners are the previous point widened by the previous
   widths, the previous point, the current point, and the current point
   widened by its own widths. Across a screen step of `dx`, `dy` the
   widths are `min(6, 6 * dy / dx)` in X and `min(6, 6 * dx / dy)` in Y
   (6 where the divisor is 0). Each strip starts at widths 6; a segment
   that does not draw passes on the widths of the last quad drawn.

**LaserWidthResidueApproximation.** Before the first quad of a draw, the
last widths (`[EBP-2C]`, `[EBP-28]`) are stack words `00485520` never
writes. In the first person a beam's first segment begins behind the near
plane, so nearly every draw reaches them: in `laser-fire-16ms-fp.json`
544 of 564 draws did. The next quad then widened its first corner by their
low WORDs, a long pale band from the beam to the screen's edge. Retail's
words were registers `0048CC00` had saved during the muzzle lookup: the
vehicle (`0x286C3330`) and `0x20031010`, or another heap pointer
(`0x20031F50`, in both). The port has no such addresses; it uses the widths
each strip starts from, 6 and 6, so it draws no band.

## Frame rate

At the lab's 125 ms steps no laser is ever drawn: the emission places the
new laser about 29,000 units from the vehicle (`laser-fire.json`, retail and
port alike), beyond the draw distance, and only the draw's pose brings it
back to the muzzle. At 16 ms steps both lasers are drawn every frame. The
damage reaches the target either way. Why the emission position lies so far
at 125 ms was not traced.

## Verification

- `laser31_update_ages_and_turns_by_group`, `laser31_reach_grows_with_age_and_shrinks_with_life`
  and `laser31_draw_lays_two_strips_of_sixteen_quads` check the update
  against the first updates of `laser-fire-16ms.json`, the reach against
  its second pose, and the strips' shape and RNG draws.
- `run.py both laser-fire-16ms.json --fresh-heap`: every compared event
  matches over 792 ticks, every tick draws the RNG alike, and the frames
  show the same beams.
- `run.py both laser-fire.json --fresh-heap` (125 ms, chase then first
  person): every damage, spawn, dispose and vehicle event matches over
  301 ticks and every tick draws the RNG alike. At tick 249, as the vehicle
  dies in the first person, two particles are born in the opposite order: a
  first-person draw-order difference, not traced.
