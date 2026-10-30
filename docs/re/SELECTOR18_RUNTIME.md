# Infestation selector 18, the Force Field

This document owns selector 18, the Force Field: a dome of light over a
site that keeps actors on their side of it until something lets them
through. A generator (a selector-39 course target) usually holds it up;
destroying the generators drops it. The class is the shared actor's
(`00527C00`) with final vtable `005279E0`, built by the factory's case
`0x12` (`0x184` bytes, `00476750(0x400, 0x300)`).

The code lives in
[selector18.rs](../../crates/inf-game/src/object_runtime/actor/selector18.rs)
(the class),
[selector18_draw.rs](../../crates/inf-game/src/object_runtime/actor/selector18_draw.rs)
(the dome),
[software_scene/force_field.rs](../../crates/inf-game/src/software_scene/force_field.rs)
(the dome's packets) and
[world_actor_update/force_field.rs](../../crates/inf-game/src/session_platform/world_actor_update/force_field.rs)
(the update's world services).

## Placements

Twenty-six fields are placed in eleven campaign worlds: 3 (two), 7, 8
(five), 9 (Tabuchi), 10, 11 (Artoria, six), 12, 13, 14, 17 and 21 (two
each). Every one is alien (team `0x12`) but world 3's placement 12, which
is the player's (team 1). Radii run from `0x2BC` to `0x1388`, heights
from `0x12C` to `0x960`. Tabuchi's field and world 8's four big ones let
their own team through (`+178`); eight open for a key grant (`+17A`: world
8's four `0x2B`, world 11's `0x2C` and `0x2D`, world 17's two `0x2A`); world
12's placement 22 sends hint `0x41`
when it is released. Every field grants `0x50` when released but world
14's two (`0x3E`, none).

## Construction and table

`00476750` builds the shared actor (`0047A7A0`), zeroes `+15C..+180`, sets
radius `+154` = `0x400` and height `+158` = `0x300`, takes a bit of the pool
(`0047DE30`), stores the radius's WORD at `+28`, switches the field on
(`004772B0(1, 0)`) and last binds its listener object at `+138` (vtable
`005279DC`) at the tail of the creation list `0054E878`. It also writes the
light colour `0055EF2C` (bytes 0, `0xF`, 0).

| Offset | Field |
| --- | --- |
| `+154` | Radius; command `0x10` also stores its WORD at `+28` |
| `+158` | Height, which scales the vertical offset by `+154 / +158` |
| `+15C` | Microseconds until it switches back on |
| `+160` | Fade, 0 to 500000 |
| `+164` | Clock, the sum of its updates' deltas |
| `+16C` | Its bit |
| `+170` | Clock at the last "not secured" warning |
| BYTEs `+174..+178` | Told secured, linked, on, shown, own team passes |
| WORD `+17A` | Key grant |
| `+17C` | Links |
| `+180` | Hint command a released field sends |

The table is the shared actor's but for `+00` (`004768B0`, the deleting
destructor over `004768D0`), `+04` (commands `00478860`), `+24` (update
`00476AE0`), `+28` (light `00478910`), `+2C` (draw `00477370`), `+8C`
(capture `004771D0`), `+98` (messages `004787E0`) and a slot past the
shared table's end, `+CC` (the scan score `004769C0`).

### The bits

`0047DE30` shifts the cursor `0053B8B0` (1 at startup) left, past the top
bit to 1, until it finds a bit free in `0055F0E4`; after 32 shifts it gives
up with 0 (the cursor stays where it stopped). The destructor gives the bit
back (`0047DE80`). Both are process globals: in Infest04, Tabuchi's field
took bit 2 and Artoria's six, built next, 4 to `0x80`; Infest03, which
loaded Artoria first, gave them 2 to `0x40`. The port's arena carries the
pool, so a retained load continues the cursor and a fresh arena starts it
at 1. Nothing reads a bit's value, only its equality and masks (the
capture's `+6C` test, a child's and a carried actor's copy of its parent's
`+6C`).

### The first marks

The constructor's switch runs before the payload places the field, so it
marks around the allocation's leftover position. Infest03's and Infest04's
fifteen constructions read (0, 0, 0) eleven times, (0, `0xFA`, 0) once and
heap residue such as (`0x392`, `0x1790`, `0x392`) three times, and each
marked only the field itself (flags `0x124` admit it). The port's loader
supplies zero ([fresh heap](../../crates/inf-game/src/fresh_heap.rs)).

## Marks and switches

`00477210(actor, r²)` clears the field's bit in the actor's `+6C`, then
sets it again for an actor inside, unless its flags keep it out: with
`0x40`, `0x80` does; without, 8 does, or the lack of 4. Inside means the
squared sum of the WORD differences in X and Z and the vertical
`(field.y - actor.y) * radius / height` (an `idiv`, as a WORD, zero when
positive) is below r². Above the centre the field is an ellipsoid; below
it the height counts for nothing.

`004772B0(on, timer)`: on marks every actor of `G+48` at the radius; off
stores the timer at `+15C` and clears the bit everywhere. Both store `on`
at `+176` and `+177`. The listener `00477350` marks each new actor the
same way, so fields mark the fields built after them.

That includes the temporaries the damage-packet cache `0048D820` builds at
eight unwritten stack bytes
([the cache](BEHAVIOR_DAMAGE_CACHE.md#enclosing-predicate-cache-0048d820)):
an admitted one is tested at stack residue, and the outcome, the field's
bit in its `+6C`, is unknown. The port keeps such bits in the actor's
`unknown_6c`, and every reader of the bit (the field's walk, the capture's
`+6C` compare) stops on one; the attachment's and the hold's `+6C` copies
carry them along. Nothing reads one in retail: `0048D820` disposes the
temporary in the same call, and a disposed actor loses flag 4, which every
field's walk requires. Before, the listener stopped at the temporary's
position, so on a world with a Force Field, selecting the Cranial
Implanter's row (resource 129) stopped the vehicle's update: a
weapon-switch crash.

## Update

`00476AE0`:

1. While shown, the fade climbs by the delta, and an update that finds it
   at 500000 or more sets it to 500000; while not, it falls by the delta,
   floored at 0.
2. A linked field whose links are gone (`+175`, `+17C` = 0) gives the first
   player its grant `+7A` (`004750C0`) and hint `+180` (`0046C760(+180,
   0)`), then switches off for good with a hint, or disposes itself
   without one. Tabuchi's field went this way in Infest04 when its
   generator died.
3. The clock adds the delta; a running off timer counts down and at zero
   switches the field on.
4. While on, each actor of `G+48` without flags 8 or `0x80` and with 4 is
   kept on its side. With `p` the edge (radius less the actor's `+28`
   inside, plus it outside) and `d²` the squared distance with the
   vertical scaled (an `idiv`, zero when negative):
   - within `0x300` of the edge, the passage `004769F0`;
   - past the edge, the bounce: the normal `00417DA0` of the WORD offset
     (f32, 53-bit sums), the speed along it, `k` = `-(speed + P)` inside or
     `P - speed` outside with `P` 600 for a player's unit (`0046B090`) and
     200 otherwise, `k` times the normal added to the speed `+8E`
     (`0047B510`), and the actor's `+98(field, 0, 5, correction)`. An actor
     with `+8C` = 1 hears where the edge is: `p - isqrt(d²) - 1` inside,
     `+ 1` outside, times the normal. The vehicle takes message 5 as 400
     damage on channel 7.
5. While still on, the player's vehicle near a field of another team (not
   `0x12`) hears whether the area is secured: no alien (team `0x12`, state
   3) with flag `0x20` and without `0x80` within twice (radius + `0x500`)
   in X/Z. Within radius + `0xA00` it is told once "Area secured. Shields
   will now deactivate on approach" (H0 `0x141`); within `0x300` of the
   edge, a secured field (or the vehicle already inside) switches off for
   2 s, and an unsecured one says "Area not secured - shields cannot be
   lowered" (`0x142`) with hint `0xF`, at most every 10 s.

The passage `004769F0`: with `+178` set and the actor of the field's team
(or the actor team 1 and the field team 2), an alien field switches off for
5 s (the push of `0x4C4B40`, `004C4B40`'s address), another for 2 s once no
alien is near; then, with a key grant, an actor whose player owns it
(`0046BDC0`) switches it off for 2 s.

### The scans' residue

`00475F50` writes its best actor (the scanner's `+8`) only for a candidate. The
passage and the secured notice test that slot, which nothing initializes;
the edge's scan tests the best score `+C`, which `00475F50` zeroes first.
With no alien near, retail's notice therefore reads whatever an earlier
call left on the stack: at Infest03's and Infest04's 4,690 field updates
the slot held `0047FD0F` (a return address) 2,144 times, `0x1E848` 2,180
times and zero 83 times (`force-field.js` `residue()`). Every field there is
alien, so neither test ran. Only world 3's field reaches the notice, and no
placed field the passage's scan (it needs `+178` and a non-alien field).
Selector18ScanResidueApproximation reads zero, as for a scan that found
nothing: the port tells the area is secured on the first update in range,
where retail waits for an update whose leftover is zero.

## Messages, capture and scan score

`004787E0`: 7 links the field (`+175`, `+17C` up) and switches it on; 8
unlinks; `0xB` and `0xC` switch a field under radius 1000 off for 5 s; it
returns 0. The course targets' after-load (`0043B720`) and death send 7 and
8; Tabuchi's and Artoria's generators link their fields at the after-load.

`004771D0`: a capture switches the field off; a release sets it to switch
back on in 2 s; then the shared `0047D7C0`.

`004769C0(actor, d²)` scores `0x7FFFFFFF - d²` for an actor in state 3 on
team `0x12`, else 0. It reads only the candidate, though `00475F50` calls it
through the filter object `00527AAC`.

## Light

`00478910` keeps flag `0x08000000`: while on it lights the ground at the
field's X/Z (`0045B1A0`), radius `(+154 * 5) >> 2`, in `0055EF2C`'s green
`0xF`.

## Draw

`00477370`, the `+2C`, sets the light `00490030` from H12's light words
(X and Z quartered by signed division, normalized as `0044FE17` does) in
grey `0x20`, which leaves `0055F26C` at the ambient + `0x20` per channel;
sets the context at the field (`0048FE60`); and, while on, draws the dome
`004777A0`. Settings `+70` = 1 rings it with 32 meridians of 16 segments,
anything else with 16 of 8.

`004774E0` builds one meridian from the top (height above the centre) to
the equator (the radius), each point projected through the context's
projection. A point below the ground + 50 (the higher of `00450A30` and its
water `00410F60` at clock 0) and every point below it collapse onto that
level, and the equator always does, so the dome's rim sits on the ground.

`004777A0` walks the meridians in pairs from angle `0x10000` down, two
strips per pass. Between meridians P and Q each segment is one D6 quad
(corners P[j], Q[j], Q[j+1], P[j+1]) if the clip table `00528BD0` admits the
OR of the four codes: near (`0044EA60`) when no corner carries fog, far
(`0044EA70`) unless all four are fully fogged. Each corner's colour is
`0055F26C`'s channel times a pulse, `((400 sin(clock * 0x200 + angle) >>
15) + 1600) * (fade >> 10) / 488`, over half its depth + 2000. A pass's
first strip lights its Q side with the next meridian's pulse; its second
strip's near quads sort by Q[j]'s depth, every other quad by P[j]'s. The bottom
band takes sprite `0x2B4`, the rest `0x2B3`, and the last pass's first
strip's middle segment `0x2B2`. With material sorting, a sprite gives the
key its translucent half or its descriptor's opaque key, as the ribbon's
do. A depth of -4000 makes the colour's divisor zero; retail's frame
`00511C90` catches the fault and ends the dome, the port stops it there.

## Destruction

`004768D0` switches the field off, gives its bit back and unbinds its
listener; then the shared `0047AA60`. The port's disposal drain and world
cleanup unbind it with the creation list they are lent.

## Evidence

- `force-field.js` over Infest03 and Infest04: constructions, switches,
  pushes (all ten replay, `the_recorded_pushes_replay`), link messages,
  listener marks, teams, the scans' residue, and each field's first update
  (`tests/fixtures/force_fields_infest.json`, which loaded worlds 9 and 11
  match field for field).
- check_selector18_draw.py runs the
  unchanged `00477370` over eighteen supplied fields, views and grounds,
  one faded far past its cap so that the pulse's product passes 2^30 and
  only the exact magic division by 488 holds: 2,702 packets
  (`selector18_draw_native.json`), which the port's dome matches byte for
  byte, with every ground sample in order.
- `tabuchi_force_field_dome_reaches_the_pixels` draws Tabuchi's field in a
  loaded world.
