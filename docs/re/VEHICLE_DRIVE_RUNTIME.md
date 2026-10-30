# Infestation player vehicle update

This document owns the player vehicle's controlled update: selector 2's
virtual `+44`, `00416170`, which the [player block](GAMEPLAY_RUNTIME.md#player-block-of-00478f20)
calls every tick with the delta and player `+4`, the mode-1/2 ground
drive `004195F0` it dispatches to, and the wheels its model places through
`00415D30`. Construction, carried units and the camera reader stay with
[the player vehicle](ACTOR_RUNTIME.md#player-vehicle-selector-2).

## The control block, seen from the vehicle

The vehicle reads the player's control block at its own `+0`, so a vehicle
offset `n` is player offset `n + 4`
([control block](../../crates/inf-game/src/object_runtime/player/controls.rs)).
The drive reads:

| Vehicle | Player | Meaning in modes 1 and 2 |
| --- | --- | --- |
| `+0` | `+4` | analog axes live (`00418F60`, `00423200`) |
| `+10` | `+14` | turn, Left minus Right |
| `+14` | `+18` | Dive minus Climb: with no thrust, Dive reverses and Climb drives forward |
| `+18` | `+1C` | Thrust minus the `+7C` action: forward/reverse power |
| `+34` | `+38` | bit 0, braking: set while reversing against forward speed, cleared on return |
| `+36` | `+3A` | jump request (Thrust held), consumed by the drive |
| `+3A` | `+3E` | a message is shown: forces gear `+238 = 1` |
| `+3B` | `+3F` | free-look (`00421DC0`) |

In Infest01 the player drove Level 1 almost entirely with Climb (the Up
arrow) and the turn keys; Thrust (Space) is held on 7 ticks, each a jump
request. A message is shown on 166 ticks, when `0046A260` fills the block
itself (no turn or pitch, a speed-dependent brake in `+1C`).

## The mode machine

`+1E8` is the current mode, `+1F0` the requested one and `+1EC` the
transition progress (`FFFF` when settled). In the game's terms the modes are
the vehicle's transformations: 1 buggy and 2 truck (wheeled, both served by
`004195F0` with their own tables), 3 hovercraft, 4 VTOL and 5 skimmer. Modes
unlock as the game goes on; a request is checked against a per-mode command
bit (`0046BDC0` on the `00530584` table) before it is accepted. The keys `1`
to `5` and Caps Lock store the request in control `+30` (player `+34`)
through the player's [key callbacks](GAMEPLAY_RUNTIME.md#player-key-callbacks-00469450004698e0). Space's boost
is the turbo: it needs command 3 (granted by a factory) and a `+12C` charge,
which the player update refills every 8 s; without it Space only drives
forward. Infest01's Level-1 run starts in mode 1: the
first update moves from mode 0 to 1 through `004184B0`, two later requests
change between modes 1 and 2, and Infest01's 707 Level-1 updates reach
`004195F0` 699 times and the transition path (`00426730` toward `0055F280`,
then virtual `+D4`) 8 times. Modes 3 to 5 are not reached there;
Infest03 drives all five (see [the hovercraft](#hovercraft-004191c0),
[the VTOL](#vtol-00418650) and [the skimmer](#skimmer-00422400)).

The full switch (`004168F9..00416A2C`) releases the engine loop `+168` and
`+548` (and `+54C`, which nothing but the constructor's zero ever stores),
runs the old mode's exit, clears byte `+160`, makes the request current,
clears control `+36`, sets flag `100h` and puts the camera in mode 0
(`00410EE0`), then runs the new mode's entry through the table at
`00417D78`: `004184B0` for modes 1 and 2, `004185D0` for 3, `00418430(1)`
for 4 and `00422380(1)` for 5. The exits are `00418430(0)` leaving 4 and
`00422380(0)` leaving 5. The dispatch table at `00417D8C` then runs
`004195F0`, `004191C0`, `00418650` or `00422400` once the progress settles.

`004184B0` resets the drive for modes 1 and 2: unless the previous mode
`+1F4` was 1 or 2 it zeroes the turn rate `+21C`, its smoothed copy `+244`,
the gear timer `+23C` and the traction `+57C` and sets gear `+238 = 1`; it
always clears the boost timer `+224`, `+20C`, `+228` and `+240`, stores 2000
in `+55C/+560`, and selects the motion tables: mode 1 uses lateral
`00530438`, forward `00530440`, angular `00530430` and suspension
`00530498`; mode 2 uses `005304C0/C8/B8/F8` and doubles `+184` into `+8C`.

## Ground drive `004195F0`

The [drive](../../crates/inf-game/src/object_runtime/actor/derived/selector2/drive.rs)
runs, in native order:

1. The forward speed: the velocity on the basis forward row
   (`(vy·m8 + vz·m9) + vx·m7`, x87, `__ftol`).
2. Free-look `00421DC0` when `+1C0` or control `+3B` is set
   ([`free_look.rs`](../../crates/inf-game/src/object_runtime/actor/derived/selector2/free_look.rs)); a shown message forces gear 1; the `+5AC` boost countdown.
3. A jump request: with no message, command 3 owned, no boost running and a
   `+12C` charge, one charge is spent. Dive held (or a forward analog push)
   flips the vehicle: a local roll (75000 toward the turn, else a 10000
   pitch) and a lift (800 up, 500 against the turn), with `+224 = 2000000`.
   Otherwise the forward boost table (`005304A8` in mode 1) is selected for
   `+224 = 3000000`. Both play sound 19h.
4. The power request: a running boost gives full power; else Thrust gives
   `7FFFF`/`-7FFF`, else Climb `7FFFF` and Dive `-7FFF`, else the analog
   throttle `00418F60`.
5. The turn rate `00419050` (`+21C` toward ±14000h at a rate that falls with
   speed and with the time the turn has been held, decaying four times as
   fast when released) and its smoothing `00419590` into `+244`. The desired
   turn is `speed·+220 / 768`. With the analog axes live (the Controls
   menu's Joystick on Analogue), the [analog turn](#the-analog-turn-00423200)
   adds the stick's turn to `+220` last.
6. A near-stationary hard turn pivots: below speed 150 with no reverse and
   more than 1800h of turn rate, the excess becomes the turn and power at
   least 4Bh.
7. The gear `+238` (0 reverse, 1 neutral, 2 forward) steps toward the power
   sign once per 250 ms of `+23C`.
8. The suspension `0041F3F0`: `0041EDC0` samples the ground at four hull
   points (table `00530418`; the front pair also a quarter of the velocity
   ahead, keeping the higher), stores their diagonal cross product as the
   `+198` normal and returns their average height. The height above it
   along the normal, scaled by the hull radius `+28`, selects support, grip
   and traction from the six-WORD `+194` table; the hull is lifted out of
   the ground and velocity into it is cancelled.
9. Gravity `0047AD10` (halved under water flag 20000000h).
10. On the ground: `0047D440` turns the up axis toward the normal, the
    traction is the table's factor times the normal's local Y. Power drives
    the shared forward motion `00426860`; the lateral grip `0041F7E0` removes
    sideways speed up to the `+188` limit and returns the skid, which reduces
    traction and rolls the hull; braking doubles traction; the shared
    angular motion `00426AE0` turns about the normal. On a slope steeper
    than vertical only the grip runs.
11. The intensity `+228` (speed, plus 300 while driving, 2000 while
    boosting), the skid loop sound 15h in `+548` (started at half level,
    then updated, released when it fades), the state bits `+58C` and, above
    100, the dust vector `+22C..+236`.
12. Water drag above the mode's speed limit (60h in mode 1) when the hull is
    within its radius of the wave surface (the inline `00410F60` form,
    always with waves).
13. The shared physics `0047B1E0` (angular and linear drag, rotation), the
    position step (truncating toward zero), the track phase `+1FC`, and the
    engine load `+55C/+560` with the smoothed pitch and volume `+564/+568`.

The drive draws no RNG. Its constructor-unwritten fields
(`VehicleDriveState`) stay unknown until an owned writer stores them;
`004184B0` covers all of them except the track phase `+1FC`, which the drive
only increments, and the dust vector, written once `+228` passes 100.

### VehicleTrackPhaseApproximation

`004156E0` never writes the track phase `+1FC`, so retail starts it from
allocator residue (`" to "`, `0x206F7420`, in Infest01) and the drive only
adds its steps. Its one reader is the model getter `00415BC0`, whose selector
3 returns the low WORD (`00415BF3`) to animate the wheels and tracks. Left
unknown, that read stopped the vehicle's model after its body: no wheels,
the truck barely drawn, and the omitted draw leaving its flash fog for the
rest of the frame. The port starts the phase at zero; it shifts only the
animation's phase relative to a retail run, whose own start is arbitrary
residue. The policy is logged once per process.

### VehicleEngineLoadApproximation

`004156E0` leaves the engine load `+55C/+560` unwritten too. A vehicle's
first update enters its saved form from mode 0, and only the ground reset
`004184B0` (modes 1 and 2) and the skimmer's drive store the load. The
hovercraft's entry `004185D0` and the VTOL's `00418430(1)` don't, so a world
started as either reaches the drive's load step (`00419533`/`00419539`,
`00418F0B`) on allocator residue. The lab (Sanrel-Dek start 0, every
upgrade, forms 3 to 5) read `68637562`/`00040069` there, the tail of a
"Tabuchi" string, and the first hover and VTOL ticks quartered it
(`1A18DD58`); the skimmer's first tick wrote its own (`CE00`/`E600`). The
load moves only the engine loop's pitch and volume (`00417BE7..00417CE6`),
and quartering without throttle settles any residue within a few ticks.
Left unknown, the read stopped the player's vehicle update: a
hovercraft stopped answering the controls as it arrived through Tabuchi's
gate (crash log of 2026-10-19 13:01). The port starts the load at zero, as
`--fresh-heap` retail does, and logs the policy once per process.

### The analog turn `00423200`

`00419050` ends in `00423200(step, controls)` when the control block's
analog byte is set. The stick's X (`+4`, the player's `+8`) inside a 6000
dead zone turns nothing and zeroes the hold `+248`. Past it, half the
excess, capped at `6C48h`, is the deflection `e`. While `e` is past `56A0h`
the hold grows by `step >> 5` up to 500; otherwise it is zero. The turn is
`27720e / 3`, plus `6(e − 4830h)²` past `4830h`, scaled to 40960 by its cap
`2DCCD440h` (768,398,400, the sum at the cap): small sums take a direct
division, larger ones shift both until the sum is below `CCCCh`. The hold
adds `40960·hold / 500`. The rate `+220` takes the turn with the stick's
sign and is clamped to ±14000h.

`check_analog_turn.py` runs the original over 319 cases (the dead zone and
both thresholds on each side, the hold's cap, unsigned steps, the clamp,
and a stick that stays `INT_MIN` when negated), and
`analog_turn_matches_every_native_stick_hold_and_clamp` checks the port's
`analog_turn` against them. The lab cannot drive it: its stub serves the
keyboard only.

### VehicleAnalogHoldApproximation

`004156E0` never writes the hold `+248`, and `00423200` reads it before
storing when the stick is past the hold threshold on a vehicle's first
analog tick, so retail starts that tick from allocator residue; a stick
inside the threshold stores zero first. Each respawn constructs a new
vehicle. The hold only adds to a clamped rate and returns to zero as the
stick eases, so the port starts it at zero, as `--fresh-heap` retail does,
rather than stop the vehicle, and logs the policy once per process.

### VehicleTurnHoldApproximation

`004156E0` never writes the turn hold `+240` either. Only the ground reset
`004184B0` and the turn rate `00419050` itself store it: a turn adds the
step to it (`004190CF..004190D7`), letting go stores zero (`0041914E`), and
past `0x2000` it eases the turn rate (`00419077`). The hovercraft's entry
`004185D0` clears the turn rate `+21C` but not the hold, nor do the VTOL's
and the skimmer's, so a vehicle that has only hovered, flown or skimmed
since it was built turns first on allocator residue. The lab
(`level1-hover-turn-hold.json`: Level 1 started as the hovercraft) read
zero there with either heap, `0x0F42` after two ticks of Left, and zero
again once it was let go. Left unknown, the read stopped the player's
vehicle update: a hovercraft stopped answering on Artoria as it
turned just after the switch from the VTOL, in a loaded game that had not
been the buggy or truck since (crash log of 2026-10-21 03:07). The port
starts the hold at zero and logs the policy once per process.

### Evidence

Two TTD recordings supply per-call receipts, captured with a script that
seeks each call's start and return and dumps the vehicle, the control block,
the player, the RNG word `0055F18C` and the clock `005471C0`:

- Infest01 ran on fine terrain (005372F4 = 7Fh, 128-unit cells). All 699
  Level-1 drive calls are in
  [vehicle_drive_infest01.json](../../crates/inf-game/tests/fixtures/vehicle_drive_infest01.json).
- Infest02 started on fine terrain and was switched to low detail during
  play: `00450315` re-prepared the terrain at `1786C0:170C`, between drive
  calls 33 and 34. Its first 517 Level-1 calls (34 fine, 483 coarse) are in
  [vehicle_drive_infest02.json](../../crates/inf-game/tests/fixtures/vehicle_drive_infest02.json).
  On coarse terrain `0045035A/004503C8` briefly restore 7Fh around a
  per-frame pass, so each call's spacing is read at its own entry.

[The receipt tests](../../crates/inf-game/src/object_runtime/actor/derived/selector2/drive_tests.rs)
load Level 1 at the matching terrain detail, write each call's captured
entry state into every field the drive reads, run the port's drive and
compare every field it writes, the control block and the player's charges
with the captured return state. All 1216 calls match exactly. On the wrong
detail only 341 of Infest01's calls match, so the terrain spacing is part of
the receipt. A mutation pass over eight constants (turn clamp, suspension
offset, dust scale, water limit, grip dead zone, normal cross product,
engine smoothing and look-ahead) was caught by every mutant, by 1 to 699
calls.

Infest02's grids, dumped from the trace, bound the terrain side. Before the
switch retail's fine grid equals the port's fresh fine grid cell for cell.
Right after it, the heights equal a fresh coarse load, but the in-game
re-preparation keeps the fine-prepared diagonal bit on 161169 odd cells
that a fresh coarse load clears; coarse sampling (`00450A30`) only reads
even cells, so no sampled height differs. From call 517 on, selector 37's
terrain-strip update, [the bridge](FAMILY40D800_RUNTIME.md#the-bridges-update-00409a20),
raises a strip through `00450890`, one height step per frame, under a
temporary fine spacing (`00450350`, restored by `00450370`). The vehicle
crosses that strip from call 588. Infest02's remaining Level-1 calls are
not yet captured; the port now raises the same strip, so they can be
replayed over it. Its last 15 calls are in a later world with height
shift 5.

## The high frame-rate hang

Above about 60 frames a second the ACAM can stop with Climb held, its hull
hanging a little above the ground, most often in buggy form. Retail does the
same, and the port reproduces it with the retail step. The port fixes it by
default.

The scheduler `004945A0` admits a tick once 8000 µs have passed, so a fast
machine runs the world at up to 125 ticks a second and the drive's step
`delta >> 5` falls to 250. The drive's last step (`0041A438..0041A47E`)
moves each axis by `velocity * step / 0x8000` toward zero. At 125 ticks a
second a velocity below 132 moves nothing; at 60 below 64, at the
recordings' 8 below 9. The fraction is dropped every tick.

The suspension `0041F3F0` pushes along the ground normal by
`grip * step >> 15`, its grip rising as the hull sinks into the table's band
and while it falls; gravity `0047AD10` takes `delta >> 10`. When the hull's
vertical velocity is too slow for the step, its height cannot change, so
neither can the suspension's spring: the push settles to balance gravity at a
constant vertical velocity, and the hull hangs where it happens to be. In the
upper part of the band (mode 1's `00530498`: support ends at ratio 3156 and
full traction at 2555) the traction falls toward zero. The forward motion
`00426860` then adds `((difference * traction >> 16) * radius) >> 12`, which
at 125 ticks is zero below about 29% traction (14% at 60). The buggy stays
put until something disturbs the hull: a turn that tilts the normal, a jump,
or a slower frame. The buggy's narrow band hangs it most often; the truck's
band is wider, but it hangs too.

[The frame-rate tests](../../crates/inf-game/src/object_runtime/actor/derived/selector2/drive_frame_rate_tests.rs)
drive Level 1's buggy from the world start with Climb held, on the drive
receipts' fine terrain, at fixed deltas:

- Straight for 10 s at 8000 µs, the hull hangs for 2.95 s past the first
  slope (369 ticks at height 191, falling at 24 with 31% traction) and covers
  6396 units, against 8823 at 16666 µs and 9117 at 125 ms. A hang is a tick
  whose height stays put with a nonzero vertical velocity and traction below
  half.
- Circling for two minutes at 10000 µs, it freezes for good on flat ground
  after a minute: velocity (0, -36, 0), normal straight up, 12% traction. At
  60 ticks the same course still hangs for 103 ticks, so capping the frame
  rate at 60 would not remove the hang.

### The fix

`VehiclePositionStep::CarryRemainder`, the port's default, keeps each axis's
dropped fraction in the vehicle (port field `position_remainder`, in
1/0x8000 units with its axis's sign) and adds it to the next step; the mode
reset `004184B0` clears it. Over time the hull moves by its velocity at any
tick rate, so it settles into the suspension's band with full traction: the
straight drive above never hangs for more than one tick and covers 8198
units, and the two-minute course never hangs for more than 17. The setting is
the sidecar's `fixes.vehicle_position_carry` (schema 7, true by default) and
`--vehicle-position-carry false` restores retail's step; the pre-game
launcher's Options offer it. The receipt tests replay retail's step.

The fix changes nothing else. The forward motion's truncation and the linear
drag's one unit a tick (`0047B1E0`) still make the drive slightly frame-rate
dependent: on that straight the buggy's top speed is 981 at 125 ticks with
the fix, against 1079 at 60 and 1054 at 8.

### The other forms

The hovercraft, the VTOL and the skimmer move through `0047B1E0`'s position
term instead, `velocity * step >> 15` with an arithmetic shift. It floors, so
a falling hull always drops at least one unit a tick and only a slow rise is
lost, and none of them hangs. Each was driven from its first Infest03 receipt
(world 11, coarse terrain) at deltas from 6944 to 125000 µs: 10 s with
Thrust held, a minute circling (Left held one second in three) and 20 s with
Thrust released after 5. Each kept moving under thrust and came to rest when
released, and the VTOL descended at every rate. Circling, the hovercraft
covered 55,249 units at 8000 µs against 61,874 at 16666 µs and 61,264 at
125 ms, and the skimmer 73,616, 86,507 and 65,390.

The VTOL's flight stays frame-rate dependent. Circling with Dive and Thrust
held, its nose pitched far up, it climbed 14,536 units at 8000 µs against
2,962 at 16666 µs and 4,519 at 125 ms. Retail does the same at the same
tick rate; the port's exact world ticks keep the world at most about 61
ticks a second, whatever the frame rate
([high frame rates](GAMEPLAY_RUNTIME.md#high-frame-rates)). These were
one-off runs; no test keeps them.

## Hovercraft `004191C0`

[The hovercraft](../../crates/inf-game/src/object_runtime/actor/derived/selector2/hover.rs)
floats on the suspension `0041F3F0` like the wheeled modes. Its entry
`004185D0` selects the tables `00530510` (lateral), `00530518` (forward),
`00530508` (angular) and `00530560` (suspension), sets byte `+160` and
clears the turn rate `+21C`, the traction `+57C` and the boost `+224` (the
network branch swaps two tables for role 4). Each tick the drive:

- sprays under the hull through `00488E90` at strength 1000, on the
  process RNG, the particle load factor `0055F154` and the water plane;
- takes its power from thrust or the boost (`7FFFF`), then the pitch keys
  (`-7FFF` back, `7FFFF` ahead), then the analog throttle on the `+18C`
  table, as the ground drive does;
- integrates the turn rate (`00419050` with no speed), holds the hull up
  (`0041F3F0` on `+194`) and applies gravity (`0047AD10`, unhalved);
- with support, rights itself toward the ground normal `+198` and grips
  (`0041F7E0` on `+188`); floating, it rights toward world up;
- damps each angular DWORD by `(w * -(0xFFFF * step >> 4)) >> 31` and
  turns toward the normal (`00426730`, full support, `delta >> 4`);
- under power pushes forward (`00426860` on `+18C`), with less push once
  the nose is raised past 30 degrees (forward row Y above `0x800/0x1000`);
- turns about its own Y (`00426AE0` on `+190`), runs the shared physics
  with mask `1E` (drags, position, spin) and raises or quarters the
  engine load `+55C/+560`.

## VTOL `00418650`

[The VTOL](../../crates/inf-game/src/object_runtime/actor/derived/selector2/vtol.rs)
steers itself rather than riding the suspension. Its entry `00418430(1)`
clears `+24C` and `+250..+25C`, sets the rest timer `+24E` to `7FFF`,
clears the traction and plays sound `0x11`; the exit `00418430(0)` plays
`0x12`. Each tick:

- the nose angle is the inverse sine `0048F520` of the forward row's Y,
  mirrored while the up row's Y is negative;
- the stick, past a 6000 dead zone, turns and eases the nose (`+24E`
  counts the time since the stick was last used, up to 2500);
- the pitch keys feed the pitch rate `+258` through the accumulator `+250`
  (target `-8000` or `8000`, scaled until `+250` reaches `0x2AB9`); after
  250 ms at rest (`+24C`), a nose more than 9 degrees off level is pitched
  back as if held, and past 30 degrees the rate yields to leveling;
- the turn keys feed the yaw rate `+25C` through `+254` (`682 * turn /
  step`, scaled until `+254` reaches 6250); the hull turns about world Y
  (`0047A750` -> `00491A70`, which mixes each row's X and Z), pitches about
  its own X (`0047A720`) and banks into the yaw while leveling its roll
  (`0047A740`);
- the flight physics `00418AB0` lifts along the up row at `thrust / 600`
  (a third more with a tow at `+1B4`), cushions the hull within 384 units
  of the ground or the water (`00450A30`, `00410F60` at clock 0: the call
  passes a zero clock), brakes falls faster than 384, fades the lift
  between 1500 and 2500 units up, sprays (strength `max(40 * lift, 1000)`),
  drags in the hull's frame (sideways and backward at 1/8192 of the step,
  up at 1/65536, forward at 1/32768, small speeds dropped whole), damps the
  spin and keeps the engine load; then the shared physics runs with mask
  `1A`, and `+1FC` (the rotor phase) advances by `delta >> 3`.

## Skimmer `00422400`

[The skimmer](../../crates/inf-game/src/object_runtime/actor/derived/selector2/skimmer.rs)
rides the suspension on its own table `005305D8`. Its entry `00422380(1)`
sets that table and byte `+160`, clears `+24C`, `+250..+260` and the
traction and puts the camera in mode 1; its exit lets the wake ribbon
`+264` go (`00486B20`), which only the remote vehicle update `+3C`
(`00420F40`) creates, so offline it is always absent; a drive with one
extends it to where the skimmer is (`00422E95`). Each tick:

- the boost `+5AC` pushes the forward speed toward `0x1194`;
- the power is thrust, then the reversed pitch keys, then the analog
  throttle; its gear is 0 ahead, 1 at rest and 2 astern;
- the drive budget is the power plus twice the delta, and the suspension's
  return value, its clearance ratio less `0x555`, is the depth;
- above `0x3000` deep, a wake of `rand * (0x3000 - depth) / 4096` throws a
  particle per `0x2000` over a spread that grows with the depth: a splash
  on water (`00489600`), a sprite with its shadow on ground (`004894A0`);
- the normal is leaned toward up (and toward forward when deep) and
  renormalized with its length spilled to f32;
- the yaw rate `+25C` works as the VTOL's; the hull yaws about the leaned
  normal (`0047A790`), scaled by the traction;
- the normal's slope along the forward and side rows drives the pitch rate
  `+258` and the roll rate `+260` (each decaying by its own step, reinforced
  by the traction and doubled while opposing the rate), and the hull turns
  by the local vector (pitch, 0, roll) (`0047A760`);
- `00422ED0` pushes along the forward row laid onto the surface
  ((N x F) x N), losing push past 30 degrees of nose and gaining it as the
  nose dips, then drags in the hull's frame: the grip turns sideways and
  vertical speed into forward speed, small speeds stop and forward drag
  eases with the gear; the shared physics runs with every term (`1F`) and
  the engine load follows the drive and the gear.

While `0054ED70` holds the race the boost and the budget take other
branches, by this machine's place (`0055F2F4`'s record) and the catch-up
scale (settings `+4C`): [the race](NETWORK_RUNTIME.md#the-race). Every
Infest03 call takes the offline branches.

### Evidence for modes 3 to 5

Infest03 drives the hovercraft 212 times (worlds 9 and 11), the VTOL 107
times (worlds 11 and 23) and the skimmer 16 times (world 11), all on coarse
terrain at the TTD's 125 ms ticks. A capture seeks each call's entry and
return and dumps the vehicle, the control block, the RNG word, the clock,
the current world, the terrain spacing and the load factor
(ledger). The
[receipt tests](../../crates/inf-game/src/object_runtime/actor/derived/selector2/mode_receipts.rs)
load each world as production does, write each call's entry state, run the
port's drive with the spray and the wake births in a fresh particle pool on
the recorded RNG word, and compare every field, the control block and the
RNG with the return state. All 335 calls match exactly. Mutations are
caught by 2 to 210 calls each: the hover's spin damping; the VTOL's bank
and vertical drag; the skimmer's pitch torque, vertical and forward drag,
wake size and surface cross product. Two survive because the recording
never reaches them: the VTOL's cushion band (it never flew within 512 units
of the surface) and the skimmer's yaw axis offset (its few turns are too
small to change the axis words). Probes on Level 1 switch the buggy into
each form and drive or lift it, with each form's model and HUD icon drawn;
the skimmer's model draws its wings under
[SkimmerModel127StackHistoryApproximation](MODEL_RUNTIME.md#skimmermodel127stackhistoryapproximation),
and the morph into it under
[SkimmerMorphModel126StackHistoryApproximation](MODEL_RUNTIME.md#skimmermorphmodel126stackhistoryapproximation).

The [frame-rate fix](#the-high-frame-rate-hang) carries the ground drive's
own position step. These three modes move through the shared physics
`0047B1E0` (mask bit 8, an arithmetic `>> 15`), which the fix does not
change; they have not been examined at high frame rates.

## The black band over the roof

While the ACAM turns into or out of the hovercraft with a side-mounted
weapon (weapon models 5 to 7: the plasmas on F2, and the F3 and F4
weapons), retail draws a black band across its roof. Each weapon's two
mounts are separate models, a left one (148, 150, 152) and a right one
(149, 151, 153), and black skirt quads (sprite 233) join each mount to four
hull points. The left mount copies them with opcode 2A's direct copy
(`022A`) from the slots its parent passes. The right mount copies the
mirror side with `082A`, which reads each passed slot's partner
(`004ED39D`: `xor eax, 1`). The forms pass both mounts the same even
slots, so the right mount gets their odd, right-side partners. The hover
morph model 120 passes the right mount the odd slots instead (`[103, 29,
17, 15, 27]` against the left's `[102, 28, 16, 14, 26]`). An odd slot's
partner is the even, left-side point, so the right mount's skirt stretches
across the roof over the hatch panel (sprite 498). Model 120 does this for
weapon models 6 and 7 both while the weapon switches and once deployed, and
for 5 once deployed. The buggy model 118 does it for weapon model 2's
right rack (144), whose skirt stays inside the hull except for a pixel at
a seam.

Infest03 enters the hovercraft three times with the plasmas deployed (`+20E`
7, `+210` `FFFF`). In the first morph frame the mounts copy through the
tables `66 1C 10 E 1A` and `67 1D 11 F 1B`. The right mount's slot 21 then
holds the left mount's slot 20 (screen 504,481, depth `16B`). `0049E1A0`
queues the right skirt `[21, 23, 17, 13]` (x 384 to 515) with sort key
`48F5C2C4`, beside the panel's `48F5D364` (x 423 to 479). That is the
material order, which runs only with the depth buffer, and the skirt is the
nearer at all 722 pixels the two share, by at least 9.8 depth units
(ledger).
The port draws the same band from the same data.

The port's fix, `MirrorCopySource::MirrorSide`, makes a copy that writes
only the mirror slot read the pair's odd member (`slot | 1`): an even
slot's partner as before, an odd slot itself. A copy of both slots keeps
retail's pair; the wheels and others pass odd slots to those, and each
pair stays whole. In every world the port loads (0 to 24, but 19), the
mirror-only copies fed an odd slot are those seven mount instances, and
[a test](../../crates/inf-game/src/model_vertices/copy_2a_tests.rs) holds
Level 1's set to them. The setting is the sidecar's
`fixes.vehicle_gun_mounts` (true by default); `--vehicle-gun-mounts false`
restores retail's mounts. Probes on Level 1 draw the band with the fix off
and the hatch panel with it on: the plasmas into and out of the hovercraft,
the F3 and F4 weapons into it.

## The hovercraft's rear fans

Once the ACAM has turned into the hovercraft, retail draws its two rear
fans as black blobs; they look right while it transforms, and while it
steers hard. The fans are model 123 (`scatfan`): each panel is two quads
over the same corners, grey metal outside (sprites 261, 264; culled as
usual, flags `0x500`) and black inside (sprite 233; culled the other way,
`0x700`), so only the side facing the camera draws. Both hovercraft
models place the fans with orientation 6, the basis their opcode 03 saved
just before. The transforming model 120 (`hoverjet1`) saves it with
orientation 0, but the settled model 121 (`hoverjet2`) with `0x20`, which
negates the basis's X axis (`MenuBasis::copy_oriented_from`): a mirror.
Every face's winding then flips on screen, the grey outsides cull and the
black insides draw, in every renderer. Steering hard draws model 120.

Each opcode 03 then turns the saved basis by the steer in its X-Z plane
(axis 1, packed angle `0x84`), so the fans swivel as the hovercraft
turns. In model 121's mirrored basis that turn runs the other way across
the world: a left steer swivels the settled hovercraft's fans left, while
model 120's swivel right.

The port's fix lets model 121 save and turn the mirrored basis as retail
does, then negates its X axis again. The fans are symmetric across X, so
their corners stand exactly where retail's do, swivel included, and only
their culling changes. The two `0x20` words are model 121's only such
words (`model_execution::hovercraft_fans_unmirrored`, and
[tests](../../crates/inf-game/src/model_execution_tests.rs) over the
loaded models and the turned basis).

The fix first gave model 121 orientation 0, model 120's basis, which also
reversed the swivel (a full left made them turn to
the right). The lab's
`level1-hover-turn-compare` run with snapshots showed retail's settled fans
swivelling as the fix-off port's do.

The setting is the sidecar's `fixes.hovercraft_fans` (true by default);
`--hovercraft-fans false` restores retail's fans. Probes on Pillier Stren
draw the grey housings and dark openings with the fix on, in Vulkan and
OpenGL alike, and retail's black blobs with it off. On Level 1, through a
left turn, the fixed fans cover retail's blobs.

## The guided missiles' mounts

The forms carry the guided missiles (primary weapon 5, weapon model 3 in
`00530590`) in different places: the buggy H8 118 and the truck in a roof
launcher (child 154), the hovercraft, VTOL and skimmer models in side
launchers (children 145 and 146). Each draws its launchers as far out as
the vehicle's model value 7, the weapon's deployment `+210`, which the
weapon switch runs down to 0 and back up to `FFFF`. A mode change morphs the
old form out (`+1EC` from `FFFF` down to 0 at `delta >> 2`) and the new one
in (0 up to `FFFF` at `delta / 12`), but it leaves `+210` at `FFFF`: across
the ground forms' boundary retail draws the roof launcher fully out until
the switch and the side launchers fully out from the first morph-in frame.

The port's fix,
[`guided_mount_deploy`](../../crates/inf-game/src/object_runtime/model_callback.rs),
answers value 7 with the lower of `+210` and the morph's progress while the
guided missiles are shown and the change crosses that boundary: the roof
launcher folds away as the buggy or truck morphs out, and the side
launchers come out as the hovercraft, VTOL or skimmer morphs in (and the
reverse). Only the vehicle's own draw reads the answer; `+210`, which gates
the firing, is untouched, so the guided missiles fire as in retail. The
setting is the sidecar's `fixes.guided_mounts` (true by default);
`--guided-mounts false` restores retail's swap. A
[test](../../crates/inf-game/src/object_runtime/model_callback/tests.rs)
covers both directions and the changes that do not cross.

## The vehicle update `00416170`

The [update](../../crates/inf-game/src/object_runtime/actor/derived/selector2/update.rs)
runs, in native order: the player lookup `0046B090` (attract playback
returns here), `+594 = 0`, the `+200/+204` countdowns, the shared status
timers `0047C990`, dirty WORD `|= 62F0h`, `+D8 = 0`, `00422300` (the `+59C`
countdown and the `+598` handle check), the `+5A0` speed timer,
[the weapon panels' timers](#weapon-cycling-and-the-panels), the mode
request in control `+30` (unlock bits from `00530584`; without upgrade 29h,
Invulnerability, the Alien Brain (world 13h) refuses modes 3 to 5 with H0
`16Ah`, "Only buggy modes are useable after that last warp jump", and
otherwise a world with flag 400h refuses modes 4 and 5 with H0 `154h`,
"Your craft cannot fly on this world.", each back to mode 1 through the
player's message `004135D0(text, 0)`), the weapon cycling keys,
the mode transition (a 1-to-2
switch below progress C000h re-runs `004184B0` at once; a full switch
releases the loops, clears `+160`, sets flag 100h, resets the camera's
parameter mode and enters the new mode), the speed magnitudes `+574/+578`
and `+56C..+570`, the mode dispatch (the ground drive, or during a
transition `0047D440` toward the world up `0055F280 = (0,1,0)` and virtual
`+D4 = 00415FF0`), the deep-water check (command 6 through `0046C760` over
deep water; below the plane `0047CAE0(-1000)` and the timed damage), the
weapon timer `0048D030`, the weapon switch animation `+20E/+210`, the
selection requests in controls `+24/+28` (player `+28/+2C`, which the respawn
fills: Infest01's first update selects primary 0 and secondary 8), the
item controls' `+204` cooldown, the splash check and the engine loop `+168`,
whose gain and pitch `+554/+558` follow the drive's load through the mode's
`005300A8` row.

Below the water plane the update hurts the vehicle (`00416D08`): world
flag 200h burns it (strength 32000, channel 3) and arms `+580`; otherwise
`0047CAE0(-1000)` eases the channel-2 status before strength 10000 on
channel 5. `0047C7C0` times the packet over 3 s from the process NULL and
reaches [the vehicle's own damage](#the-vehicles-damage-004212f0). While
`+580` exceeds the delta, each update (`0041647C`) burns on: 1000 on
channel 2 (2000 online, `0055F448`) from the process NULL, timed over 6 s,
and `+580` runs down by the delta; at or below it `+580` clears. Neither
packet writes its byte `+3`.

The splash check (`00417957..00417BED`) watches the hull against the
waves under it (`0053014C`, 64 units either side of the surface). Entering
or leaving that band draws the RNG and, below `delta / 1,000,000`, plays
sound 9 or 10. Leaving it while the vertical speed exceeds `00530154` (100)
creates the [water splash](SHARED_ACTOR_FACTORY.md#selector-65-water-splash)
through the vehicle's `+4C` (`0047AAF0(NULL, 0x3B, NULL)`), a ring of spray
on the water, and plays sound `0x13`: diving in from above crosses the band
twice, the second time leaving it downward, and bursting out leaves it
upward. More than 64 below the surface, a draw above `00530150` (252 of 255)
bubbles with sound `0x44`. A probe dropping the buggy into Level 1's water
draws the ring and its droplets for the splash's seven ticks.

`vehicle-splash.js` finds
five splashes in play, all from `00417B67`: Infest02's buggy twice on
Level 1 and Infest03's hovercraft three times on Tabuchi. Infest02's first
bursts out of the water at a vertical speed of 267 with Up held; its
second dives in and leaves the band downward at -113 with no key held.
Replayed in the port from snapshots four updates before (the vehicle's
drive state, the recorded keys and, for the waves, the recorded clock of
each update), both match every update for 13 updates, the splash on the
recorded update. The first also leaves the band again at 19, and neither
splashes there.

Sub-features not yet ported are reported per update as omissions, logged
once per world, and the update continues past them: the view toggles.

The `+598` handle is the Capture the Banner banner the vehicle carries.
Only the banner claims it: `004212B0` stores the banner's handle when
`+59C` is zero and `+598` holds the null handle, called from the banner's
update `0045B5A0` (a vehicle of another side, with a lifecycle `+C4` of at
least 3, comes within reach of the banner) and its pickup `0045B9E0`; `00422300` clears it
once the banner's `+B8` component no longer names the vehicle in its
`+144`, and online the item drop clears it (below). A census of every
`+0x598` operand finds no other writer (`sweep_instructions.py`), so the
constructor leaves allocator residue there. The first `00422300` keeps a
residue only if it names a live actor whose `+B8` component's `+144` is
this vehicle, which nothing a fresh vehicle carries can be; any other
value ends as the null handle, as Infest01's does. The port starts the
field as that residue and stores the null handle at the first update,
with no omission to report. Branches Level 1 never reaches stop the vehicle
update; the player block then keeps running without it. The update's own wreck branch belongs to
[the vehicle's death](#the-vehicles-death-00420b90-and-the-wreck-00416090);
the player block calls `+44` only from lifecycle 3, so only a status
retirement in this tick's prefix reaches it: [a burn that outlasts the
health](#the-vehicles-death-00420b90-and-the-wreck-00416090).

### The aim `0041F9C0`

The update calls the aim after the behavior's tick, whenever the vehicle has
a behavior. It keeps the vehicle's target object at `+148`, and clears the
vehicle's flag 4 for its duration so that neither the scan nor the sight
test meets the vehicle itself; a change of the low nine flag bits marks the
dirty word, as always.

It first builds an evaluator (`00415310`, vtable `00520858`): the vehicle's
forward row (virtual `+48` is the basis `+44`, read at `+1C..+24`), the
current target's handle (`+04`, or the process NULL), and whether it is
armed (virtual `+64` is the vehicle, `+7C` its behavior `+140`). An armed
evaluator drops the handle to the process NULL unless its actor resolves
with `+C4` above 1.

Unless free look is on (`+1C0`), the scan (`00475EC0`, functor `00520A5C`)
walks the list from `[0055F160]+48` for actors with both flags `0x24`, nearer
than `+164` in XZ (unsigned squares) and other than the vehicle, keeping the
strictly best score of `004153A0`:

- Nothing for an unarmed evaluator, a candidate whose `+C4` is not positive,
  or one on the vehicle's side `+B4` or side 1.
- The offset is projected on the forward row, (Z + Y) + X products
  truncated, and must be ahead. In modes 1 to 3 the offset is then taken
  into the vehicle's frame (`0047A660`, `00490A80`) with its height divided
  by 16 toward zero.
- The projection's square must exceed a share of the offset's square. With
  settings `+7C` (every trace has it) the share is `(0x7FFF - accuracy) <<
  16` as Q31, from the selected weapon's `+18` accuracy in the vehicle's
  `+26C` copy; it is zero while `+200` is set and 15/16 for the current
  target. Without `+7C` it is 31/32 in modes 1 to 3 (`0x7C000000`), else
  the whole square.
- The score is `0x7FFFFFFF` less the lateral square (the offset's square
  less the projection's), a quarter off for the current target. A
  candidate the selected weapon cannot damage (`0048D820`, whose first test
  of a row builds the row's
  [resource](ACTOR_RUNTIME.md#behavior-registry-resource-classes)) scores nothing
  if its virtual `+54` is itself (the factory, gate, bridge and course
  target), else a sixteenth. A candidate below lifecycle 3 or on side 2
  then keeps a sixteenth.

A found actor becomes the type-2 target: a current type-2 target is
retargeted in place (virtual `+20`, `00482760`), any other is freed for a
new `004825D0` resolving the actor, and the target is refreshed (virtual
`+18`). The [sight test](#the-sight-test-0043ad90) runs from the hull to the
target's `+08`; a clear line keeps the target.

Without a scored actor the aim looks ahead: the hull plus its forward row
times 3000.0 (`00520A54`), each axis truncated. A weapon whose spread
(`+14`) is nonzero adds `((draw >> 5) - 1024) * (spread / 2) >> 15` to
each axis from three draws of `0048F060`. The sight test runs from the hull
to that point, and a clear line frees the target.

A hit frees the target and allocates its replacement by type: the terrain
point as type 8 (`0051F154`, the process NULL at `+04`), the furniture as
type 4 (`004827D0`, vtable `00528060`: the furniture's position at `+08`,
the point at `+14`) and the actor as type 2 with the point (`004825D0`,
`+14` alone; `+08` keeps heap residue). Type 4 answers the virtuals of
type 8 but for its clone (`00482890`) and `+1C` (`00482940`, its `+14`).

Free look aims from the eye of `00421D50` along its basis, 7500.0 out
(`00520A58`), through the look-ahead's sight test and tail (`0041FCE8`, EBP
still zero there). No trace reaches it; the port's aim does the same
(`the_free_look_aims_from_its_eye`).

`0041FF20` turns the target into the fire direction and the behavior's
`+20` elevation.

In Infest01 every one of the 707 sight tests comes from the look-ahead
(`0041FE5A`), so no scan found a scored actor: the scorer runs 625 times
and reaches the predicate 31 times, never scoring. 229 sights hit nothing,
210 hit terrain, 134 furniture and 134 actors. The target slot is written
1187 times, and `0041FF20` runs 440 times, beyond the one primary shot.
Infest02 and Infest03 score 53 and 56 actors; no trace uses free look.

The port replays all 707 Infest01 aims from their captured vehicle and
target over the Level-1 world, with the recorded sight results: every
look-ahead request, every replacement target, flag 4, the dirty word and
the RNG match. The scorer matches 521 calls from Infest02 and Infest03
(modes 1 to 5, five weapons and four accuracies, current targets, both
predicate answers), each candidate a stand-in of the recorded virtual `+54`
with the recorded position, side and lifecycle.

[The update's receipt test](../../crates/inf-game/src/object_runtime/actor/derived/selector2/update_tests.rs)
replays all 707 of Infest01's Level-1 updates from their captured entry
state ([vehicle_update_infest01.json](../../crates/inf-game/tests/fixtures/vehicle_update_infest01.json)).
705 match every written field, the control block, the player's commands
and the RNG word, among them the deep-water hits at 263 and 264. The two
others, the drop at 565 and the take at 581, differ only in the RNG word:
each birthed a slot flash (`00489A90`) through draws the replay does not
make. The fixture does not carry the hold, so the replay's drop finds it
empty, and its host records the take without performing it.

#### The ray table `00544C20`

A model hit's request holds the model's origin in the ray's frame
(`+10..+18`), the ray's basis (`+34`) and its length (`+38`); the ray
leaves the origin along +Z. The actor hit `0047D080` moves the actor's
position into that frame as the WORD difference from the ray's origin
through the ray's `00490A80`, and its basis through `00490DC0`: each of the
actor's rows dotted with each of the ray's rows (`00490710`), the counter
plus one and, past 100, the rotations' renormalization. The view publish
`0048FB40` inlines the same composition. Only the flag is cleared before
`004F2C40`; the executor keeps the strictly best score, and a positive best
is a hit.

- `+00` (opcode 8E), `004F37F0`: the sphere at a vertex, in integers. It
  misses when the sphere lies wholly behind the origin or past the length,
  or when the lateral square exceeds the radius square (unsigned). The entry
  is the depth less the root (`0048EFC0`, its low WORD sign-extended),
  clamped at zero, and must not lie past the length.
- `+04` (90), `004F3870`: stores the cleared flag.
- `+08` (8F), `004F3880`: the world-aligned box of half extents at a vertex.
  Its bounding sphere must reach the ray; the centre returns to world axes
  through `004909E0`, and the ray's direction is its forward row times
  1024.0 (`00520A50`), rounded. The z, x and y faces follow in that order,
  each through the face the ray meets first.
- `+0C` (89) `004F3B20` and `+10` (8A) `004F3C60`, the quad and triangle
  tests, are never reached in Infest01; the port stops at them.

Each hit scores the length left past its entry (`+10`), so the nearest hit
wins. The port checks the sphere and box tests against 12 sphere hits and
25 box calls from Infest01, `00490DC0` against six compositions, and
`0047D080` end to end against 20 actor hits and misses on the models of
the factory, a selector-66 attachment and a selector-11 actor.

#### The sight test `0043AD90`

The sight's record holds the origin and target WORDs, then the result: the
type `+0C` (0 nothing, 1 terrain, 2 furniture, 3 actor), the point `+10`,
the furniture's position `+16`, the actor `+1C`, the length `+20` and the
ray's basis `+24`. It returns whether the type is set.

The ray's direction is the difference normalized by `0043B230` (the root of
the zero-extended square, which also returns it, unused). Its basis comes
from `00490370` about the world's up (`0055F280`), or about X (`0055F290`)
when the direction's height is beyond ±0.75 (`0052236C`, `00522370`).

The actor pass `0043B2E0` walks the list from `[0055F160]+48`. An actor
with flag 4 is tested when its centre's square is below the best so far
(at first the ray's), its distance along the ray (truncated) is positive
and its lateral square is below its radius `+40` squared. Its model is then
run over the ray up to its far side (`0047D080`, length along plus radius);
a hit makes its square the best and its entry the distance. The pass
returns that distance (or, if none or zero, the root of the best square) as
the length and places the point along the ray at it.

The march then samples the ray every 64 units from the origin, each axis
truncated. A sample whose cell differs from the last one (the first always
does) scans the furniture around it (`0043A990`) and takes the highest of
the cell's four corners. A sample below that corner reads the terrain's
height there (`00450A30`); at or above the sample it is a terrain hit at the
sample. The march ends at the length.

`0043A990` covers four cells each way, stepping one cell. The first scan
covers the whole square, column by column. Later scans cover two edges: the
column at `+4` when x grew, else `-4`, and the row at `+4` when z grew, else
`-4`; a scan after an x-only move therefore also covers the row at `-4`. Each
cell's sparse record, or the fallback record when the cell is unflagged,
names a placement; a resident model of nonzero radius runs over the ray
(`0043A7E0`). The furniture stands at its cell's corner at the cell's
height, with its state's basis (`00437A70`), and every packed value reads
zero (`005220B8`, `00437AE0`). A hit shortens the length to its entry and
records the furniture's position; after the scan the point lies along the
ray at the length, and the march ends.

The port replays all 707 of Infest01's sight tests from the actor pass's
captured result: every type, point, furniture position and length matches
over the Level-1 world's terrain and furniture. The actor pass itself
matches 12 of them, five hits, rebuilt from the actor list with each
flag-4 actor's position, radius, model and basis; `SightRay` rebuilds their
ray basis exactly.

Some models' programs group their geometry (opcodes 89..8D). On the ray
table a group of four or three vertices is hit when it straddles the ray's
axis, and scores the length less its mean depth
([MODEL_AUXILIARY.md](MODEL_AUXILIARY.md#group-tests-898d)). The port
replays Infest03's 47 sight tests on Helniazine whose march reaches one.
Before the port ran these opcodes, the first update of a saved game on
Helniazine reached 8C at `0x57AE7` and the vehicle's update stopped there,
so the ACAM never moved. A plain key replay of that save, holding Up for
35 updates, now drives it off the pad.

### The weapons (`004171A8..004174DF`)

Once the weapon switch has finished (`+210` at `0xFFFF`), the update runs
its fire region. A network game in mode 4 first takes the launcher branch
at `00416E34` (`00403940`), which the offline update never reaches.

The primary fires while control `+1C` holds its time (default key Z). The
selected weapon's behavior entry (`+18`, 32 bytes per weapon) must be
ready: no `+14` cooldown, a nonzero `+00` count and `+1C` below 6 s. The
fire direction `0041FF20` then runs:

- Under the special view (`G+5C5`) it frees the target, clears the
  behavior's `+20` lead and writes no direction.
- Without a target the direction is the forward row times 4096, each axis
  truncated, and the lead is cleared.
- Otherwise the target is refreshed (virtual `+18`) and the direction is
  its point (virtual `+1C`) less the hull. A type-2 target whose refresh
  answers above 1 leads by `(distance << 10) / speed` (the selected
  weapon's `+02`), at most `0xFFF`, times 5/4; the distance is the root of
  the offset's square, quartered and doubled when the square's sign bit is
  set.

A type-2 target whose second refresh answers above 1 takes a predictive
shot (`0048D340`, publishing its lead) with its offset, distance and the
fire time. Every other shot is queued along the normalized direction
(`00417DA0`, `0048D0D0`) with no speed fraction.

The secondary (default key X) adds control `+20` to the charge `+218` and
does nothing else while held. Then, with a quick throw (`+35`) or a
charge, the secondary weapon's entry must be ready; it fires as `+10` for
the moment (`+12` keeps the primary). A quick throw goes along the forward
row with fraction `0xFFFFD000`. A charge past 200 ms keeps the rest at
`+218` and lobs along the forward row times 4096, raised by 2000 except in
mode 4, normalized, with fraction `(rest >> 4) + 0x4000` below a further
800 ms and none above. The charge and the quick throw are then cleared,
fired or not.

In the three recordings the primary fires 33 times (9 predictive, 24
queued: 9 without a target, 7 at type 8, 4 at type 4 and 4 at a type 2 whose
refresh answers 1) and the secondary lobs 6 charges; the port's fire
region matches every shot's arguments, the lead and the secondary's state.
`0041FF20` also runs from the HUD every frame (`0041CE7F`), which the port
does not yet draw.

The shots enter the behavior's fire chain. Retail drains it when the
vehicle is drawn: its own draw does so in a cockpit view (`+D0` above 0),
and the manager's late lane does so otherwise. The port's vehicle draw
used to stop at the unwritten track phase `+1FC`, skipping that late lane;
with the [track phase approximation](#vehicletrackphaseapproximation) the
lane runs in the chase view too. The cannon's emission (resource 116,
flags `0x16`) builds its shot, [selector 32](SELECTOR32_RUNTIME.md), or
repeats the weapon group's live one. The shot sits at the muzzle, aims at
the target and delivers the damage in its own update. The
[Lasers](SELECTOR31_RUNTIME.md) build their own class over it. A shot of an
unported class would stop at its actor resource (`0048C9CE`), logged once,
and the vehicle would keep drawing.

### Weapon cycling and the panels

Each weapon has a panel, the primary's open while byte `+534` is set and
the secondary's while `+535` is. Each update first runs their timers
(`004163A2..0041646E`): an open panel's timer (`+52C`, `+530`) drops by
the delta in milliseconds. Once it would reach zero the panel closes with
sound 16h (volume 5555h, pitch 10000h). Then its slide (`+540`, `+544`)
halves toward zero.

After the mode request come the cycling keys (`004166CD..0041685A`): player
`+40` for the primary weapon and `+41` for the secondary, which the key
callbacks hold at the key's level. While no message is shown:

- once the key's timer (`+538`, `+53C`) is behind the animation clock, a
  turn left asks for the next weapon (`+28` or `+2C` = 1) and a turn right
  for the previous one (-1). The timer then moves 20 clock ticks ahead;
- without a turn, the key slides its panel open (`004216B0` or `00421750`
  with 0);
- before the timer, the key slides the panel open and, without a turn,
  restarts the timer at the clock.

The turn is consumed either way, so the vehicle does not steer while it
cycles. A key that is not held keeps its timer at the clock. Opening a panel
sets its timer to 1000 ms with sound 16h, and sets the other panel's timer
to 250 ms. Reopening an open panel lifts a timer below 250 to 1000 minus
itself, and one below 750 to 750. The weapon selection that takes the
request slides the panel further. It skips a row whose upgrade ID the player
lacks; the pickups that grant the IDs and stock the rows are in
[WEAPON_PICKUPS.md](WEAPON_PICKUPS.md).

The `cycle` queries find 20 updates with a cycle key held: 12 in Infest02
and 8 in Infest03, all on the primary weapon. [The update
tests](../../crates/inf-game/src/object_runtime/actor/derived/selector2/update_tests.rs)
replay each from its entry state to retail's state after the cycling
region, matching the timers, panels, slides, requests, consumed turns and
sounds.

### The hold (`00412380`)

Every update runs the hold from `004176FF`, before the tow and the item
controls. [The hold](../../crates/inf-game/src/object_runtime/actor/derived/selector2/items.rs) walks the `+1A4` slots:

- A NULL handle is skipped, and a handle that no longer resolves is
  cleared.
- A carried slot (state 2) keeps its actor while the actor's capture `+70`
  still names the vehicle. A local actor's `+90` then runs with the vehicle
  and the delta and keeps it while it returns true. Selector 12's
  (`004688A0`) repairs the vehicle: `+A0` rises by the delta over 2^14 µs,
  at least 1, as a WORD, to no more than `+A2`. The actor's status timers
  (`0047C990`) follow.
- A remote take (state 1) whose capture names the vehicle becomes state 2.
  One not yet captured is captured and announced over the network
  (`00412468`), and a remote drop (state 3) still captured is announced
  (`004123F4`) ([holds between machines](NETWORK_RUNTIME.md#holds-between-machines)).
- Any other slot is released: its handle becomes NULL and its state 0.
- A kept actor takes the vehicle's `+32`. With the hold's byte `+0C` set,
  each slot's `+0` places its actor and the holder's basis is copied; only
  the [Prison Unit](HOLD_ITEMS.md#prison-unit-selector-17)'s one-slot hold
  sets it.

A change compacts the hold (`00411F60`), as the drop does.

The A key's request (control `+2C`, `+1` from A, `-1` from `004696A0`, which
no key binds) turns the hold before it runs (`004176D9`, `00411E70`): the
hold compacts, then its first N slots, N those whose handle is not the
process NULL (the count less `004120E0`'s free slots), move by one, the
first to the last for a positive request and the last to the first
otherwise, each with its saved flags and state; the request is consumed. A
handle the compaction leaves behind (one that no longer resolves) keeps its
place, so a hole among the first N turns with them.
check_hold_rotate.py runs `00411E70` with the
compaction native over 36 holds, and the port matches each.

In Infest02 and Infest03 (the `holdtick` queries) each carried scientist
repaired 7 per 125 ms tick. One took the vehicle from 4820 to 4827, three
from 4995 to 5000 and three from 0 to 21. In Infest01 the vehicle stayed at
5000 through its 194 carrying ticks, and its scientists stood at the vehicle
after each. [The hold tests](../../crates/inf-game/src/object_runtime/actor/derived/selector2/items_tests.rs) replay those ticks and
the release rules.

### The Tractor Beam's tow (`00417E40`)

While byte `+1BC` is set (the constructor sets it), the update runs the tow
after the hold. With no tow (`+1B4` the process NULL; the constructor's
zero resolves to nothing and becomes it on the first update), a player
owning the Tractor Beam (grant `0x36`, `0046BDC0`) and the pickup key held
(block `+37`, C) runs scanner B from the vehicle: flags `0x20` set and
`0x80` clear, within 600 across X/Z, its filter `00418410` scoring only
resource `0x27`, the Powerball (selector 57), `-1 - n` for its squared
distance `n`. The nearest becomes the tow: `+1B4` its handle, `+1B8` its
distance (`00462DA0`), and the key is consumed. The drop key (block `+38`,
D) lets go.

While it holds a ball whose handle resolves and whose flag 2 is set, the
length `+1B8` eases to 400 by `(delta >> 5) * 400 >> 15` a tick, and the
ball moves to the alternate update list (`00479040`). When the line from
the ball to the vehicle is longer than the length, or shorter than half of
it, both ends move onto it: the centre lies at `share / total` of the line
from the vehicle, where the ball's share is its mass `+8C` plus a third of
the speed between them and the total adds the vehicle's mass; the vehicle
goes `length * share / total` and the ball `length * mass / total` from the
centre, along the line's direction (`00417DA0`, x87, `__ftol`), at the
length (or half of it), and each takes the velocity of that move (`(move <<
15) / (delta >> 5)` added, `0047DDF0`). Then short smoke (`00489BC0`, type 1
while it pulls, 0 while slack) marks the line from the ball, one particle
every thirtieth plus both ends.

[`tow.rs`](../../crates/inf-game/src/object_runtime/actor/derived/selector2/tow.rs)
ports it (2026-10-19). check_vehicle_tow.py
runs `00417E40` over 53 states with scanner B native; the port matches
each. A probe on Artoria (world 11, grant `0x30` keeps its Powerball)
starting 500 from the ball takes it with C and drags it 800 behind the
vehicle; the length settles at 400.

### The drop (`00417747..004177F1`)

Control `+3C` (D, "Beam Down") drops while the player exists and the `+204`
cooldown is spent, unless a pickup is pressed too; either way it arms
250 ms and clears the byte. [The drop](../../crates/inf-game/src/object_runtime/actor/derived/selector2/items.rs)
counts the hold's free slots (`004120E0`, those holding the process NULL).
With an actor carried, the slot array's `+0` (`00412880`, through
`00412110`) drops the last occupied slot, and three one-shots of sound 2 at
the vehicle follow, at pitches 0x6AAA, 0x5555 and 0x4000. An empty hold
plays nothing.

[`00412110`](../../crates/inf-game/src/object_runtime/actor/derived/selector2/carry.rs) takes the first slot from that index whose
state is not 3:

- The array's `+4` (`004121D0`) places the actor beside the vehicle:
  three times the vehicle's radius `+28` plus the actor's, as a signed
  WORD, along the vehicle's forward row (basis words 7 to 9 of `+44`, which
  its `+48` `004026B0` returns), from the vehicle's `+32`. Each product is
  exact in x87 before truncation. The actor takes the vehicle's `+6C`, that
  position with a zero velocity (`0047DDF0`), the terrain height there
  (`00479DD0`, without the water plane) and the slot flash `00489A90(2)`.
- A remote actor (flag bit 31) stays there, and its slot becomes state 3.
  Later drops pass over it.
- Otherwise the actor's `+8C` releases it with operation 0, the process
  NULL as source and the slot's saved flags. Selector 12's `00468220` sets
  `+22E` bits 0xC, clears its helper history (`00483400`), drops its targets
  `+1EC`, `+274` and `+27C` (`00427110` with nothing), searches again
  (`+104`, `00468350`) and runs the shared release `0047D7C0`.
- `00405420` then attaches resource 46 (selector 66, the dying path's
  effect) to the actor. The attachment's first update saves its owner's
  flags and clears bits 0x1, 0x2, 0x4, 0x20 and 0x100.
- The slot takes the process NULL and state 0 and keeps its saved flags.
  `00411F60` compacts the array: a later duplicate handle is cleared, live
  slots move to the front, and a handle that no longer resolves leaves the
  count where it stands.

In Infest01 (the `itemcontrols`, `dropspawn`, `dropbefore` and `dropafter`
queries), the vehicle at (47681, 63, 27848), radius 109, dropped slot 1's
scientist, radius 132. 459 along (-0.99606, 0.00668, 0.08437) put it at
(47224, 0, 27886). Its flags went from 0x481008E0 to 0x48100840, the
attachment took it as owner, and the search left an interaction target at
`+274`. [The drop tests](../../crates/inf-game/src/object_runtime/actor/derived/selector2/carry_tests.rs)
replay the placement, the release and the attachment's first update, and
the remote-slot rule.

The session lends the drop its owners. The search runs through the
behaviour tables, tasks and particles with a scratch copy of the process
state, which only native-receipt harnesses write. The attachment goes
through the world factory and the selector-66 update.

Both item branches first test the network flag `0055F448`. With it set and
a `+598` handle held, the drop instead stores `+59C` = 1 s and clears
`+598`, and the pickup zeroes `+59C`. The port's update is the offline one.

Level 1 starts with an empty hold, in Infest01 as in the port (the `takes`
query). Infest01's two scientists came from the factory's
[Evacuate](FAMILY40D800_RUNTIME.md#evacuate-0042f780-0040dcb0) (`0040DCB0`,
the takes from `0040DDAE`).

### The pickup (`00417843..004178ED`)

Control `+3B` (C) picks up while the player exists and the `+204` cooldown
is spent; either way it arms 250 ms and clears the byte.
[The pickup](../../crates/inf-game/src/object_runtime/actor/derived/selector2/items.rs)
runs scanner B `00475F50` over the actor list with the vehicle's filter
(functor `00520A0C`, whose `+0` is `004155C0`):

- The scanner admits actors with flags 0x60 set and 0x80 clear, nearer than
  the vehicle's `+164` in X/Z (squared, unsigned), other than the vehicle.
  It keeps the strictly best score.
- The filter refuses team 0x12, an X/Z distance of 1500 or more (squared)
  and a height difference of 1500 or more. Otherwise the score is
  `0x7FFFFFFF - distance squared`, so the nearest wins.

The take is `00421810`. When it refuses, the player gets "Cargo hold full
or beam unsuccessful" (`0x16E`). Level 1's four loose scientists are team
1, so taking one runs selector 12's
[team change](ACTOR_RUNTIME.md#carried-units) to the vehicle's team 2, and
the capture's grant merge announces its blueprint ("New blueprint: Standard
Grenades").

In Infest01 (the `itemcontrols` query) the vehicle carried two scientists,
dropped the second, and picked it up again at (47231, 0, 27809), 126130
away squared, scoring 0x7FFE134D. The take refilled slot 1 with saved flags
0x42100867.
[The pickup tests](../../crates/inf-game/src/object_runtime/actor/derived/selector2/items_tests.rs)
replay that find and each refusal.

### The vehicle's damage `004212F0`

The vehicle's `+B4`, [its damage](../../crates/inf-game/src/object_runtime/actor/derived/selector2/damage.rs),
takes the resisted damage, the packet and the source:

- A collision (packet channel 1) first plays sound 0x1D at the vehicle
  (`+32`) at gain `(min(+578, 1000) << 16) / 1000`, its speed change, and
  pitch 0x10000.
- The network arm (`0055F448`) is not ported.
- Offline, a collision hurts the truck (mode 2) by a third of the damage
  less 250, and not at all below 1. It hurts the VTOL and the skimmer
  (modes 4 and 5) by the packet's strength once that exceeds 200.
- While `0054ED70` holds [the race](NETWORK_RUNTIME.md#the-race) (vtable
  `00527390`, which `0046E830` installs), damage leaves the vehicle at
  least one health. Level 1 has none.
- The vehicle's player (`0046B090` of `+84`) divides the damage by
  `(100 + Shielding) / 100`, where Shielding is the level byte `+142`.
  Owning upgrade 0x29 makes the vehicle immune.
- A nonzero hit on a vehicle whose byte `+D0` is set flashes 512 along the
  basis's forward row (`00487D70`, radius 0xF00, style 0x27 when immune,
  else 0x28). The port's damage services do not hold the world flash, so
  reaching it stops the damage.
- The derived damage `004283E0` follows, unless the hit was nonzero and the
  vehicle is immune.

The vehicle's `+64` is itself and its `+68` NULL, which the shared
reaction reads only in network play.

Infest01 calls `004212F0` 26 times (the `vehicleb4` query). Two are
deep-water hits of 416 at Shielding 3 (the `shieldlevel` query), each
reaching `004283E0` with 403 and taking health 5000 to 4597 and then 4194.
The other 24 are collisions from `0047C6E0` with damage 0, each sounding
0x1D. For example, speed changes 8, 49 and 114 give gains 0x20C, 0xC8B and
0x1D2F, and each then reaches `004283E0` with 0. `+D0`, `0054ED70` and the
network flag read 0 at every call.
[The damage tests](../../crates/inf-game/src/object_runtime/actor/derived/selector2/damage_tests.rs)
replay those inputs and cover the modes, immunity and the flash. Before this,
the port's first vehicle collision with an actor stopped the world's pair
pass at the unported `+B4`. The vehicle's terrain response `+94` is the
shared `0047D2F0`, whose impact damage reaches this `+B4`; at the factory's
dock the pair's other side, the factory, takes its damage through the
family's `0040E030`.

#### Falls and collisions

A collision's strength is its velocity change times the mass, over 192
(`0047B590`). The vehicle's table (`+A4`) gives its channel 1 damage as
`(strength - 1500) * 4`, so the buggy and the hovercraft are hurt only by
hits above 1500, the truck above about 1690 (a third, less 250), while the
VTOL and the skimmer take the whole strength of any hit above 200. Across
the four recordings 130 collisions reach this `+B4` and 11 hurt
(census):
Infest02's one buggy landing (strength 1545, damage 180) and Infest04's
buggy landing (1675), hovercraft landing (1531), three VTOL pair hits
(265), three VTOL landings (359 to 752) and two skimmer landings (1432 and
1602), Infest04's ten halved by its Shielding. A carried scientist repairs the
vehicle by `delta >> 14` a tick (at least 1, about 60 a second at 60
ticks), so health climbs back between hits.

The recordings' own landings replay in the port exactly. All 45 of the four
recordings' terrain hits that reach the damage (`vehicle-falls.js`, eight of
them hurting) start from the vehicle's recorded `+000..+5FF` and its
player's Shielding; the port's terrain query `0044EC50` finds each recorded
contact (flag, normal and depth), the response `0047D2F0` leaves each
recorded position and velocity, and the damage each recorded health
([`every_recorded_landing_lands_in_the_port_as_it_did`](../../crates/inf-game/src/session_platform/intro/runtime/tests/fall_tests.rs)).
Three of the hurting landings are also replayed from their approach
([approach tests](../../crates/inf-game/src/session_platform/intro/runtime/tests/approach_tests.rs),
`vehicle-approach.js`): Infest02's buggy driving off a plateau on Level 1
(24 updates before the landing), Infest04's buggy jumping down a slope on
Helniazine with the turbo (24) and its VTOL turning into the hovercraft
over Tabuchi (9, after two blasts and two bumps before them). From the vehicle's,
its behavior's and its player's recorded state, each of the 72 recorded
updates returns where retail's did, field for field; chained from the
first, the port's updates and contact passes reach every later update and
the landing, and the landing costs what it cost retail (180, 350 and 62).
The only fields left out are the scientists' repair, which the injected
vehicle cannot carry (`carried_scientists_repair_the_vehicle` checks it).

How hard a vehicle lands depends on the length of the world's update.
Under TTD retail ran near 2 frames a second, so every recorded update
covered the scheduler's 125 ms cap. Run from the same recorded state with
the same keys at the port's 16384 µs, the approaches go elsewhere and land
at other strengths
([`retail_approaches_land_differently_at_the_ports_tick_rate`](../../crates/inf-game/src/session_platform/intro/runtime/tests/approach_tests.rs)):

| Approach | Retail, 125 ms | Port, 16384 µs, carried step | Retail's step at 16384 µs |
| --- | --- | --- | --- |
| Infest02, buggy off Level 1's plateau | 1545, hurt 180 | 1375 | 584 (takes the slope) |
| Infest04, VTOL to hovercraft over Tabuchi | 1531, hurt 62 | 1162 | 1162 |
| Infest04, buggy's turbo jump on Helniazine | 1675, hurt 350 | 1882, hurt 764 | 1340 |

The third column is retail's own code at about 61 frames a second, and none
of the three hurts there; under the port's carried step two soften below
the 1500 that hurts the buggy and the hovercraft, and the turbo jump lands
harder. Short updates let the drive and the suspension shape a fall that a
125 ms update ends in one step, and retail's step, which drops each
update's fraction of a turn, steers the same keys elsewhere. The
recordings' frequent hard landings come from TTD's slow updates, not from
a difference in the port, which lands as retail does at the same update
length. A dropped buggy (the [fall test](../../crates/inf-game/src/session_platform/intro/runtime/tests/fall_tests.rs))
still lands alike at both rates: from 400 units at about 1000 to 1270, from
800 at about 1700 and hurt. A live run of the port on Artoria (the skimmer,
the VTOL and the buggy, 16 ms frames) hurt the skimmer on each landing
above 200, as retail's rule does.

### VehicleSpeedChangeApproximation

Only the update publishes the speed change `+578` (`00416A62..00416B1D`);
the constructor `004156E0` and the mode reset `004184B0` leave it to
allocator residue. A respawned vehicle meets a pair pass before its first
update: the player block that respawns it (the held camera's `0040F890`
through `0046A890`) runs inside `00478F20` ahead of the contact passes
`004791F0` and `004790C0`, which walk the actor list the factory has just
joined. On Oolbryte a vehicle reborn at the world's start sits in the
arrival gate (placement 75), so that tick's pair pass collides the two, and
the collision sound reads `+578` unwritten. The port reads 0 there, a still
vehicle's change and a fresh heap's value, so the sound plays silent
(`VEHICLE_SPEED_CHANGE_APPROXIMATION`, logged once). Before, the read
stopped the gate's pair with the vehicle for the rest of the visit. A live
run (Oolbryte, killed by the turrets by (27950, 25500)) now respawns into
the gate and drives on.

The vehicle's furniture response `+A8` is the shared `0047BFD0` itself,
as selectors 34 and 35 bind it. The world contact pass tests the vehicle's
furniture every update (`0047C1D0` from `004794D2`: 707 times in Infest01,
914 in Infest02, 618 in Infest03), and only Infest02 touches any: five
responses, each pushing the vehicle out along the contact's normal and
rebounding its velocity. Two of them, faster than 150, damage the vehicle
(channel 1, strength from its velocity change times its mass `+8C`, 200)
and the furniture's cell. The port routes the vehicle there and matches all
five; before, its first touch of furniture stopped the world's furniture
pass.

The [player block](GAMEPLAY_RUNTIME.md#player-block-of-00478f20) now calls
this update with the player's control block; the arcade stand-in is gone.

### The vehicle's messages `00420BE0`

The vehicle's `+98` takes the messages other actors send it. It first looks
up the player whose unit the vehicle is (`0046B090` of `+84`), then acts on
six codes, through a byte table at `00420D88`:

- 4: with a player, H0 string `argument` on the player's message
  (`004135D0(text, 0)`).
- 5: a hit from the process NULL (`0047C6E0`). The packet is built in the
  argument's stack slot: strength 400, channel 7, and the argument's top
  byte as its fourth byte.
- 0xB and 0xC: with a player, the radar jam. Online, a hit from an actor
  whose owner `+88` is the vehicle sets the map's jitter (`G+1B4` `+24`) to
  0x3FFF. Otherwise a nonzero range `argument` sets it to
  `(range - distance) * 0xFFFF / range`, the distance being `0048EFC0` of
  the squared distance.
- 0xD: `+200 = 1,000,000`.
- 0x11, 0x12 and 0x13: online with a player, the grants 0xAA, 0xAB and
  0xAC (`0046B2B0(id, 1)`).
- 0x17: `+20B = 1`, the HUD's "Radiation warning" notice (H0 `0x16D`). The
  turrets' radiation (`00401920`) sends it within 500 units, as do the
  course targets (`0043BB80`) within 1000.

Every code returns 0. [The port's
handler](../../crates/inf-game/src/object_runtime/actor/derived/selector2/message.rs)
applies `+200` and `+20B` and returns the text, the hit and the jam for the
world's services to apply. The online grants stop at a boundary. The jam
writes the world's map when the sender's update lends it, as the
[grenade's](SELECTOR26_RUNTIME.md) messages do. Before this,
a turret's radiation notice stopped that turret's update for good the first
time the vehicle came within 500 units.

The `vehiclemsg` queries list every call in the three recordings. Most are
no-ops: codes 6 and 0x18 from the vehicle's own broadcasts, and 0xA and
0x16 from the zero-radius events. Infest03 has one jam, code 0xB from 2000
units with the squared distance 0x4F9F1, which leaves the jitter at 0xB6E8
as the formula gives: its chain grenade's broadcast. Its two online-only
0x13s are its [mines'](SELECTOR38_RUNTIME.md#detonation) word to their
thrower. No recording brings the
vehicle near enough a turret or target for 0x17.
[The message tests](../../crates/inf-game/src/object_runtime/actor/derived/selector2/message_tests.rs)
cover each arm on Level 1's vehicle, and the recorded jam.

## The vehicle's death `00420B90` and the wreck `00416090`

When a hit leaves the vehicle no health, the shared reaction `0047CBC0`
lowers it to lifecycle 1 with `0047D670(packet, source, 1)`. State 2
records the death in the mission's statistics (W+0C) and, because the
vehicle's `+58` is itself, the placement death `0048EAD0` of byte `+78`, then
sets flag 8000h. State 1 clears the health and calls the vehicle's `+AC`,
[its death](../../crates/inf-game/src/object_runtime/actor/derived/selector2/death.rs)
`00420B90`:

- the vehicle's player (`0046B090` of `+84`), if it has one, gets command
  37h with its notice (`0046C760(37h, 0)`); offline, `0046A860` adds one to
  the player's WORD `+11C`; `0046CBB0` copies the mode and the weapons into
  the player (`+144..`);
- the wreck gets 10 s in `+268`, which the constructor leaves unwritten;
- `0047DD20(0, 10, 0)` broadcasts event 10 at zero radius, as the vehicle's
  cleanup `00415BA0` does.

The update that dealt the hit continues. In Infest02 the controlled
update's deep-water check killed the vehicle, and its hold tick then still
healed the dead vehicle by 7 for each of its three carried actors
(`004688C0` from `00412445`), so the wreck keeps 21 health. Later in that
frame the camera loses its follow target, whose lifecycle is now below 3,
and attaches the player to itself (`0046AE90` from `0040FF66`). From the
next tick `0046A0E0` returns the camera, so the update loop reaches the
wreck while [the death camera](#the-death-camera-0040f890-and-the-respawn)
holds the player.

A burn can kill the vehicle as well: the status `+CC/+CE` that channel-2
shots leave (Altair Prime's turrets, the lava's `+580`) takes its share each
update in `0047C990`, and a share as large as the health calls `0047CE90`,
which lowers lifecycle 3 to 1 from the process NULL through the same
`0047D670` and `+AC`. The controlled update `00416170` then finds lifecycle
1 after its prefix and takes the wreck's branch below in the same call. The
port's controlled update hands that branch back to its caller, which runs it
with the dying owners (the burst's radius effect needs the terrain the drive
borrows) before the event log's line for the update.

The wreck's update is the vehicle's `+24`, `00416090`. It clears `+218` and
calls `+44` (`00416170`) on a zeroed control block whose byte `+3A` is 1.
Below lifecycle 3 that update runs its prefix (the player lookup, the
attract check, `+594`, the `+200/+204` timers, the status `0047C990`, dirty
`62F0h`, `+D8`, `00422300`, `+5A0`) and then takes the wreck's branch
`0041625C..0041639F`:

- `+1C0 = 0`, then full physics `0047B1E0(delta, 1Fh)` (gravity, both
  dampings, position and basis integration) and the status visual
  `0047DFD0`;
- one draw in sixteen (`draw & 0Fh == 0`) bursts the wreck: `0048A8B0`
  with sixteen type-1 debris about a type-1 centre within 20 units at
  speed 8, the radius effect `004887D0` of block `00531250` from the
  wreck's handle (unless flag bit 31 is set), and a local spin `0047B4B0`
  of three draws, each `(draw & 3FFh) - 200h`;
- `+268` runs down by the delta; once it is below the delta the wreck is
  disposed (`00479FA0`) and `+268` is left as it was;
- the weapons still tick (`0048D030`).

Back in `00416090`, a wreck below the water plane takes the controlled
update's deep-water hit. Its packet sits in the delta's reused stack slot, so
the packet's byte `+3` is the delta's high byte. `+594 = 1` ends the update.
The hit cannot hurt the wreck, since the derived damage ignores lifecycle 1
and below.

The world's disposal drain then runs the vehicle's cleanup `00415BA0` (the
event-10 broadcast again) and its destructor `00415B10`. The destructor
releases the sound records `+548` and `+54C` (`00487FE0`) and, through the
hold's `00411DD0`, disposes every actor the `+1A4` slots still hold, before
the derived teardown. The port stops at a nonzero `+54C`, which only the
unported modes create.

Infest02 drowns its Level 1 vehicle (world 18, terrain spacing 100h). The
`death` and `deathchain` queries give the chain: the timed hit `0047C7C0`
from `00416D78`, then `004212F0`, `004283E0`, `0047CBC0` and
`0047D670(packet, 0, 1)` at lifecycle 3 with health 608, followed by the
`+AC` calls in order and the respawn about 5 s later. The `healthwrites`
query finds the hold's three heals after the death. In the `handoff` query
all 81 dying updates come from the update loop (`00478FE5`): `0046AE90` sets
`+D8` to the camera in the death's frame, and every later tick re-attaches it
from `0046A17C`. The `dying3` query records each dying update's entry and
exit state and the RNG at every boundary.
[The death tests](../../crates/inf-game/src/object_runtime/actor/derived/selector2/death_tests.rs)
replay all 81 updates from their entry state and match retail's physics,
status words, flags, dirty word, timers and RNG, including 11 bursts and the
disposal at the 81st. They also check the `+AC` effects and the destructor's
hold disposal. A long live drive in the port now drowns the vehicle, keeps
the wreck burning for 10 s and destroys it.

### A parked vehicle (`00416090` alive)

The update loop also reaches a live vehicle's `+24`: the vehicle the player
left for a unit it holds (the Cranial Implanter's implant and then its
creature, a taken truck). The block `00416090` builds is zero but for its
`+3A`, which is 1, and the three bytes after `+00`, which it leaves as the
stack held them. The controlled update reads neither those bytes nor a
player: the lookup `0046B090` of `+84` matches each player's held unit
`+D8` before its vehicle `+D4`, so it finds none. The vehicle runs its whole
update braked (block `+3A` is the player's `+3E`, the brake gate) and
stands on its own physics, then takes the deep-water tail. The update's
grenade charge `+218` is cleared first, so a parked vehicle throws nothing,
and on the ground the brake gate puts the gear `+238` in neutral.

Infest04's VTOL ran it 157 times on Artoria. While its player steered the
implant (`5B2587:D84` to `5D7EEE:E55`, 114 updates) it sank 44 units an
update from its launch height, 499, and settled near -1469 after 90
updates; while the player held the creature the implant took (to
`5E67C7:C79`, 43) it hovered within four units. The port's
[`tick_vehicle_unheld`](../../crates/inf-game/src/object_runtime/actor/derived/selector2/death.rs)
runs the same update and tail. From each recorded start, all 157 return the
VTOL where retail's did, after the update and after the tail
([test](../../crates/inf-game/src/session_platform/intro/runtime/tests/unheld_tests.rs),
[`vehicle_unheld_infest04.json`](../../crates/inf-game/tests/fixtures/vehicle_unheld_infest04.json)).
The flying VTOL never reads the brake gate; a
[unit test](../../crates/inf-game/src/object_runtime/actor/derived/selector2/update_tests.rs)
shows it on Infest01's buggy, which the gate puts in neutral where a plain
block leaves it in drive.

### The death camera `0040F890` and the respawn

While the player holds the camera, the player block calls the camera's
`+44`, [`0040F890`](../../crates/inf-game/src/object_runtime/actor/selector16/held.rs),
with the delta and the player's control block. `0046A0E0` re-attaches the
player and points the camera at the wreck before each call, so the camera
keeps watching it (its pose still flies off on the escape velocity of a lost
target). The camera's `+180`, which its `+88` `0040F880` cleared when it took
the player, counts the time held:

- the wait is 2 s, 0.5 s while the first player's vehicle (`0046B030`) is at
  lifecycle 3, or 0.75 s in network play. Before it, the call clears the
  44h-byte control block (`004959E0`), and a camera whose target no longer
  resolves searches for another (`00475F50`).
  REA decodes the search. `0040F91F..0040F954` builds
  the query on the stack: `+4` the camera, `+14` required flags 0x20,
  `+18` range² 0x40000 (512 units), `+1C` excluded flags 0x80, and a scorer
  whose vtable is `0052059C`. `00475F50` walks the actor list `G+48`,
  admits an actor whose `+2C` has every required bit and no excluded one,
  is not `+4`, and lies within the range (squared X/Z distance), and keeps
  the strictly highest `scorer(actor, d²)` in `+8` (actor), `+C` (score) and
  `+10` (d²). A found actor replaces the old follow target, which the
  camera deletes, through `004825D0`. The scorer `00411080` returns -1
  less d², so the nearest admitted actor wins (the first of equals); none
  leaves the lost target. A detonated Cranial Implanter's camera reaches
  it: the port stopped there, and the player's update with it, so the
  vehicle stopped answering (2026-10-17); it now runs the search;
- after it, nothing happens until there is input (turn, pitch, thrust, fire,
  `+28`, `+2C`, `+34`, or bytes `+3A..+3C`) or 5 s have passed. Then the call
  clears the controls, finds the camera's player (`0046B090`), stops the
  camera (`+8E..+92 = 0`) and, when the player's byte `+EE` is set, detaches
  it (`0046AE90(null)`). If the first player's vehicle is gone or below
  lifecycle 3 it calls the respawn `0046A890(player, network)`, which writes
  the weapon requests `+28`/`+2C` (the saved weapons plus 2) and the form
  request `+34` into that same block, in place, after the clear. The new
  vehicle's first update takes them: it reselects both weapons (two clicks
  of sound 0x16 at a third of the gain) and its form. The port's camera
  works on a copy the caller stores back, which erased the respawn's
  requests until 2026-10-19: the respawned ACAM kept neither, and a lab run
  of Stendahl Alpha's start, where the ACAM dies three times, missed six of
  retail's 323 sound records. It now matches all of them;
- with a vehicle at lifecycle 3 the camera follows it (`004825D0`,
  `00410E70`), sets `+187` and reapplies the First Person setting
  (`0040F800`). Otherwise it holds `+180` at 5 s and clears the player's
  `+D8` and `+D4` (`0046AE90`, `0046CB50`).

[The respawn](ACTOR_RUNTIME.md) is the world load's `0046A890`, with its
loss limit checked: the losses `+11C` against the world's H14 header `+24`
(0 in Level 1, which never ends the game). Most campaign worlds set it
(OVL leaves 12..34 hold 3 to 8 except Level 1's 28 and the intro's 32;
leaves 11 and 35..43 hold 0). At the limit `0046A890` sets G+1D4 = 1 and
G+1C7 = 1 and returns before touching the player, which starts the
mission's [doom countdown](FAMILY40D800_RUNTIME.md#timed-generators). The
camera then takes its hold branch: `+180` = 5 s (`0040FAD4`; Ghidra prints
the constant as `FUN_004c4b40`), `+D8` set to the process NULL and `+D4`
cleared. From the next update `0046A0E0` resolves neither and returns NULL
(`0046A11A`, `0046A20E`), so the player block reaches no actor and nothing calls the
respawn again. The mission tick finds no unit, takes the first stage (`+18`
set, 3 s) and, when those run out, ends the world: "Mission Failed" about
3 s after the refusal. That sequence follows from the code; no accepted
recording has exhausted a loss limit yet.
`loss-limit.js` lists every write
to the mission's `+15`, `+17`, `+18` and `+24` and every `00487560` in
Infest02..04: `+17` is set once by the constructor (`0048DA60`) and cleared
only by Infest02's course targets (`0043BA2B`, with `+24 = 0` at
`0043BA25`); no doom countdown runs, `+15` is only ever zeroed, and the world
ends are the intro's `00487560(0x12, 1, 0)` and three gates
(ledger).
The port follows the code: the held
camera's [respawn host](../../crates/inf-game/src/session_platform/world_actor_update/respawn.rs)
starts the countdown, and [the mission](../../crates/inf-game/src/world_mission.rs)
runs both stages and ends the world into itself; a
[death test](../../crates/inf-game/src/object_runtime/actor/derived/selector2/death_tests.rs)
runs that last life on Level 1's loaded world. The vehicle-HUD's left number
shows `+11C` and its right number `+24`, the count the briefing draws. Over
the wreck the respawn binds a fresh vehicle: `0046CB50` moves flag 10h from
the wreck to it, and the wreck keeps dying until its 10 s run out. The
respawn reads the world's start, or the spawn [the save pad](#the-save-pad-00475420)
saved (`+ED`). The camera's own HUD `+84` (`0047F690`) is only its player's
`0046BE00`, so the player sees the message box and no vehicle HUD.

In Infest02 (the `deathcamera` query) the camera holds the player for 40
updates with idle controls, `+180` rising by 125 000 from 0. The 41st, past
5 s, respawns the vehicle, with `+180` at 4E3388h, the camera stopped and
`+187` set; `+D8` returns to the process NULL. The `handoff` query shows
`0046A0E0` re-attaching from `0046A17C` every tick in between. [The death
tests](../../crates/inf-game/src/object_runtime/actor/derived/selector2/death_tests.rs)
replay that timeline. With input held, they show the respawn on the first
update past the 2 s wait. They also check that a mode request returns the
player to a live vehicle. The actor factory's respawn test runs a second
respawn over the wreck, and a refused one at a reached loss limit. A live
long drive in the port now drowns the vehicle, shows the death notice over
the burning wreck, and respawns it at the world's start about 5 s later.

### The save pad `00475420`

Selector 58's update, [the pad](../../crates/inf-game/src/object_runtime/selector58.rs),
first runs its chime timer `+6E`: while set, it rises by a quarter of the
delta, and once it would reach FFFFh, three one-shots of sound 2 at volume
10000h play at the pad (pitches 8000h, C000h, 10000h) and it clears. A first
player (`00488AA0`) whose vehicle (`0046B030`) is at lifecycle 3 or more and
stands on the pad, 0 to 100h above it and less than 128 units across,
saves the spawn once per visit (`+6C`): `0046AF80` on the player, then the
chime at pitches 5555h, 8000h and 18000h, and `+6E = 1`. Leaving across
clears `+6C`; rising above the pad or dying does not, and without a player
it clears.

[`0046AF80`](../../crates/inf-game/src/object_runtime/player.rs) saves at
most once per 50 clock ticks (`005471C0` against `+F0`, signed). It sets
`+ED`, copies the player's unit (`0046AE60`) position into `+E0..+E5`, its
vehicle mode `+1E8` into `+EC` through its `+64` view (0 without one), the
clock into `+F0`, and its basis's angles into `+E8`. Those angles
(`00491E80`) are three 10-bit fields:

- the CRT `asin` (`005128A0`) of Z.x over Z's horizontal length, placed in
  its half-turn by the sign of Z.z;
- the `asin` of -Z.y over that length. Past 45 degrees that is outside
  asin's domain, and its NaN stores as 0;
- the inverse-sine table (`0048F4D0`) of the Y row lifted by the first
  angle, placed by the X row.

The respawn's saved spawn takes `+E0` and the mode `+EC`. After the
factory, `004911F0` rebuilds the vehicle's basis (`+48`, which is `+44`) from
the world axes (`0055F290/80/70`, the identity), keeps its counter, and
turns it by the three angles (`00491530`, `00491290`, `004917D0`). The
factory's own rotation words are unwritten stack on this path; the port
turns by zero angles there, a labeled approximation
(`SavedSpawnRotationApproximation`). Only the basis counter depends on
them, since `004911F0` replaces every row. `0046AF70` clears `+ED` and `+F0`
when the next mission's start is published.

Infest02's drowned vehicle had visited the pad at (4E7C, 90, 816E). The
`padupdate` query shows the visit's chime and save, the timer rising
7A13h and F425h and its expiry chime, and the respawned vehicle's visit
right after the respawn. The `savespawn` queries give the saves: Infest01's
Level-1 pad and Infest02's two, with the angles 3F9FEEECh and 3FB049DDh.
They also give the respawn's `004911F0`. [The basis
tests](../../crates/inf-formats/src/native_basis_tests.rs) match all three
angle readings and the rebuilt basis. [The pad
tests](../../crates/inf-game/src/object_runtime/selector58_tests.rs) follow
the visit and timer, and the death tests check Infest02's save. The actor
factory's respawn test respawns at the saved pad, with retail's position,
mode and basis (counter 6). Level 1's three pads now update every tick
instead of being skipped.

## The wheels `00415D30`

The vehicle's model places its wheels through selector 2's virtual `+10`,
`00415D30`, its custom vertex callback. It takes the full-DWORD selector 3;
every other selector jumps to the family callback `004260C0` with its
arguments unchanged. For a wheel the
[port](../../crates/inf-game/src/object_runtime/model_custom_vertex/selector2.rs)
runs, in native order:

1. `00439E10` copies the hub row named by extra WORD 0. Extra WORD 1 is the
   wheel's reach (unsigned). In mode 2, the truck, the reach scales by
   300/182 (signed truncation) and the lift is 50; otherwise the lift is 20.
2. The hub moves down by the lift along the hull's up axis `+54..+5C`, each
   component truncated by `__ftol`.
3. `0043A380` searches along up for the ground, over the lowered hub plus
   or minus the reach (it doubles the pushed half reach), with two regula
   falsi steps and no water sample. AL is false when the whole segment is
   above the ground (the wheel hangs at full reach) or up points down.
4. The output is the found point plus the reach along up.
5. While the process byte `0053B890` is clear and the dust intensity `+228`
   is above 100, a wheel may raise dust or splash. Dust needs the ground hit.
   A splash needs the hull not classified wholly above the water (flag bit
   `40000000h`, from [the contact pass](WORLD_CONTACT.md#water-classification))
   and the point below the wave surface; `00410F60` is called with its wave
   gate forced on by a literal 1. One `0048F060` roll below 40 times the
   intensity births a particle at the point, and a roll below half that
   births a second one offset by `+232..+236`. A wet wheel births the splash
   `00489600` (type 0, zero velocity); a dry one births the dust sprite
   `004894A0` (type 0, velocity `+22C..+230`, flags 0, no shadow).

The drive writes `+228`; the vehicle draw zeroes it after the model, at
`00420A13`, so each update's dust is spent by the next frame. In Infest01
the first update precedes the first wheel, so the draw never reads the
constructor's unwritten `+228`. The port fails the wheel if it would.

[The wheel receipt test](../../crates/inf-game/src/object_runtime/actor/derived/selector2/wheel_tests.rs)
replays all 5736 of Infest01's wheel calls from their captured entry state
([vehicle_wheels_infest01.json](../../crates/inf-game/tests/fixtures/vehicle_wheels_infest01.json)),
on Level 1's fine terrain. They cover both wheeled modes (504 truck calls)
and 1865 dust sprites and 130 splashes. The last 80 calls, 20 frames drawn
with the clock stopped at 4415 and no world update between them, see the
byte set and birth nothing. Every output, birth and RNG step matches.
The vehicle now draws in the release build: from the base into the palm
grove on Level 1, with its shadow.

## The vehicle's lights `00420080`

The vehicle's `+28` keeps flag `08000000` and lights the ground up to three
times through `0045B1A0`, in this order:

- With its headlights on (`+208`), the headlamp: a ray from the hull
  stepping 512 along the nose (the basis Z row, `00520A64`) and -128 along
  the up row (`00520A2C`), as WORD sums. A step with no rise or fall lights
  nothing. Steps 2 to 16 are sampled until the ground (`00450A30`) stands
  above one. The hit then moves back toward the first step by
  `(ground - z) / (z - first z)` of its distance in 1/32768ths (the native
  divides by the ray's Z where its height was meant, so the move is almost
  always zero; an equal Z first moves on by one). The light sits there with
  radius `3/4` of its distance from the first step plus 512, and each
  channel of the grey `00546A38` (0x60, which `00415610` writes at startup)
  becomes `channel * 500 / distance`, saturated to a signed byte. A traced
  frame (non-native, 2026-10-24) keeps the vehicle for the tracer's lamps
  in place of that deposit ([`PIXEL_RUNTIME.md`](PIXEL_RUNTIME.md));
  untraced frames deposit it as retail does.
- In the buggy or the truck (mode 1 or 2), unless the camera `00478E00`
  finds is in first person (`+188` = 2), the rear glow 150 behind the hull
  along its nose (`00520A60`): red (20, -2, -2) of radius 350 in gear 1
  (`+238`), amber (20, 20, 8) of radius 400 in gear 0, none in the boost's
  gear 2. A traced frame keeps which pair of rear lamps that lights for
  the tracer in place of the deposit ([`PIXEL_RUNTIME.md`](PIXEL_RUNTIME.md)).
- With flag `0x40000`, which the [ECM Unit](HOLD_ITEMS.md#ecm-unit-selector-73)
  sets on its holder, a pulse (`0042041C`): `+588` gains the world's delta
  `0055F0E0` and the hull's X/Z light up within `0x400` in three sine waves
  a third of a turn apart at `+588 >> 5`, each `>> 12` (-8 to 7). The
  constructor leaves `+588` to heap residue; the port starts it at zero
  (`VehiclePulsePhaseApproximation`, logged once), which only shifts where
  the colour's cycle starts. No recording runs the pulse with the light
  pass on, so it is checked against the unchanged segment under Unicorn
  (`check_vehicle_pulse.py`, 26 cases).

L (`00469740`, the player's `+3D`) reaches the update as the block's
`+39`. With a player, `004178FF..00417953` reads both upgrades first and
toggles Infrared vision (`0x3D`: the world's `+5C4 = !+5C3`) when owned,
else the Headlights (`0x14`: `+208`), and clears the byte. Infest03 pressed
L twice on Tabuchi: the headlights went on at `41A7F1:21BF` and off at
`473B12:1C1B`, with 66 headlamp runs and 54 deposits between, and the rear
glow ran 270 times (148 deposits) there and 717 times (220) in Infest01.
The [port](../../crates/inf-game/src/object_runtime/light_pass/vehicle.rs)
toggles the headlights and lights both arms; 40 headlamp and 40 rear glow
receipts from Infest03 and 20 amber rear glows from Infest01's Level 1
([`vehicle_lights_infest03.json`](../../crates/inf-game/tests/fixtures/vehicle_lights_infest03.json))
match retail's X, Z, radius and colour, including every ray that found no
ground. Before, the rear glow stopped the vehicle's light in every buggy and
truck frame. Infrared vision is the world draw's
([infrared vision](GAMEPLAY_RUNTIME.md#infrared-vision)).

## The vehicle HUD `0041C320`

After its update the vehicle draws its own HUD from virtual `+84`
(`0041ED40`) and then its player's (`0046BE00`). The
[plan](../../crates/inf-game/src/vehicle_hud.rs) follows `0041C320` in
native order and the [HUD pass](../../crates/inf-game/src/player_hud.rs)
draws it.

Infest01 runs the HUD 717 times over Level 1 at 1024x768, always in the
third-person layout 0 with the vehicle in the draw list (`+2C` bit 1), so
the cockpit prefix `0041C4D7..0041C92E` never runs. In the order the
instruction coverage shows:

1. [The reticle](#the-reticle-0041c92e), unless a menu is shown (player
   `+88`). With the "HUD" setting (`+60`) an actor or furniture target
   `+148` gets the red box, an actor its health bar and caption too;
   otherwise `+5C0` rises by 50000 up to 500000 and, with the setting, the
   three-ring sight follows the aim's direction `0041FF20`.
2. The health gauge `0047ED20` at (203/256 W, 200/256 H), sized 5/100 H,
   filled with the missing health `0x8000 - (+A0 << 15) / +A2` in
   `00 20 40`, then the mode icon, sprite `0x2AC` plus the mode WORD
   `+1E8`, as an effect quad tinted from red to green by the health. At or
   under three eighths of the health the icon shows only on odd 300 ms
   steps of the clock `005471C0 / 15`.
3. The models' light: direction `(128, 64, -128) / sqrt(36864)`, white,
   with a quarter-white ambient. Then [the hold](#the-hold-display-0041d59a):
   the carried actors spin in a row at the bottom left.
4. Each weapon icon, the secondary at (197/256 W, 90/100 H) and the primary
   at (59/256 W, 90/100 H): its model (`0049FEA0`, table `0041D902` by the
   behavior's `+11`/`+10`) at 1000 in front of an identity view centred on
   the icon, turned -0x1000 in YZ and 400 times the clock in XZ; a frame
   gauge sized 8/100 H; and the ammo count (entry `+0`) at 96/100 H,
   centred, for the primary only between 0 and 9. The `+218` gauge and the
   primary's charge gauge (`0048D7D0` over 3) come before their icons; the
   [switch panels](#the-switch-panels-0041a740) `0041A740` replace an icon
   while `+535`/`+534` is set. The models draw at the flush `004A4460`
   that ends the weapons.
5. The radar `00473490` at (230/256 W, 85/100 H), then its bare twin at
   (26/256 W, 85/100 H). Each frame is four mirrored quarters of sprite
   `0x2A6` at nine eighths of the radar's half size, the width and height
   at the `+10` offset of H10 global slot 0 (`00471A80`/`00471AB0`, 76 by
   76 on Level 1). The contents `004723F0` follow the first frame: one
   process RNG draw (`00472409`); the actor blips, only with upgrade
   `0x11`; then each map annotation (selector 87, the `0055F170` list of
   `0048C6F0`) with a nonzero label. Its offset from the eye's X/Z WORDs
   turns by the view's heading (the X axis in XZ scaled to 32768), and a
   distance r (from the integer root `0048EFC0` of the quartered squares,
   doubled) lands at the half size times the quarter sine of r, pinned at
   the rim from `0x4000` on. The label is "%d" in one `004A3AB0` pass, in
   style `0x24` with each channel scaled by `(-1 - 1200 * clock) & 0xFFFF`
   over `0x8000`. The [actor blips](#the-radars-blips-004723f0) come
   between the RNG draw and the labels. With upgrade `0x12` (or online,
   `0055F448`) the [terrain radar](#the-terrain-radar-00470170) `00470170`
   replaces the first radar and its frame (`0041E343..0041E3C3`). The list
   at `0055EF10` (save chunk 38) is empty throughout Infest01.
6. The counters `+120/+124/+128` (out of 30, eased through the quarter
   sine) in styles `0x22..0x24` at (35, 187), (44, 196) and (50, 209)/256,
   the turbo charges `+12C` (out of 3) in style `0x21` at (216, 188)/256,
   and the milestones `+11C` out of the H14 header's `+24` at (20/256 W,
   94/100 H).
7. The compass ring `0041BF60` of radius 8/100 W around the radar: 32
   marks a 0x800 turn apart about the world's up axis, from the view's
   horizontal forward (or its X turned a quarter when the view pitches
   beyond 0x800). Marks 0, 8, 16 and 24 are "n", "w", "s" and "e", a
   quarter of the font height lower; the rest are dots. The ring's other
   form, which only free look draws, is the pitch ring: its marks turn
   about +X from the forward's pitch and horizontal length (`0048EFC0`),
   move only in Y, and read " 00", "+90" and "-90" at 0, 8 and 24. Without
   its last argument a ring draws only the marks in front (direction Z
   above 0), the compass on its centre's row
   ([`hud_ring`](../../crates/inf-game/src/vehicle_hud.rs),
   receipt, 76 cases).
8. The mission clock, session `+248` (`G+1E4`, the whole seconds that
   `004388D0` counts at `00438921`), as `hh:mm:ss` at (27/64 W, 31/32 H).
9. The notices: `+20A`, which the update sets over deep water
   (`00416CCF`), shows "Water Hazard" (H0 `0x135`) at 50/100 H while clock
   bit 4 is set; `+20B` (set at `00420C3D`) shows "Radiation warning" (H0
   `0x16D`) at 54/100 H while it is clear. A drawn notice clears its flag.

`0047ED20(x, y, size, fill, colour)` draws a gauge. Below a size of 40 its
frame is one quad of sprite `0x2AC` at seven eighths of the size; at 40 or
more it is four mirrored quarters of sprite `0x2A6`. Widths follow the
focal ratio `00546F20/24`. A positive fill (`0x8000` is full) adds the
`0x2AB` disc in the colour, at the fill's fraction of three quarters of the
size, at least one pixel. All HUD quads carry depth `0x4000` and
descriptor 0. Text goes through `00415650`: H4 font 0 in style 7 one pixel
right and down, then in style 0.

`0041C3E8` builds the weapon descriptor once with `0043A150`, which zeroes
its `+4/+8/+C` and ends by installing vtable `00522360` (its base vtable
`00520384`, whose `+0/+4` are the CRT `_purecall`, stands only while the
basis is built). So the models sit at (0, 0, 1000), as all 1434 of
Infest01's weapon draws read, and their values come from `0043A270`, as the
name entry's objects' do (word `00533760` for selector 1).

[The receipt tests](../../crates/inf-game/src/vehicle_hud_tests.rs)
plan Infest01's calls 5 and 300 from their captured inputs. The sight,
the health frame and fill, the mode icon and tint, both weapon frames,
both radar frames and the five radar labels with their pulse, the counter
and turbo frames, the milestone text, the 32 compass marks, the clock and
"Water Hazard" match retail's quads and texts. In Infest01 the radar's
blip walk never runs: the player never owns upgrade `0x11`.

The layout is the First Person setting's byte while the camera is in first
person (`+188` = 2), else 0. Layout 1, which F12 gives with the hull still
drawn, draws the same HUD with the reticle's brackets half again as large
(`0041CA1C`), its sight rings doubled (`0041CF15`), and the radar centred on
the vehicle's X/Z WORDs and turned by its own basis (`+48`, `0041E2CB..0041E33E`)
instead of the eye and the view: Infest04's first-person radar calls read
the vehicle's (`5C0C`, `3485`) where the eye stood at (`5C19`, `3476`).
Layout 2, the cockpit prefix `0041C4D7..0041C92E` and its placements, which
an undrawn vehicle forces, is logged once and not drawn; no recording
reaches it.

### The reticle `0041C92E`

The reticle shows what [the aim](#the-aim-0041f9c0) keeps as the target
`+148`. It is skipped while a menu is shown (player `+88`), and online
(`0055F448`) when settings `+10` is 4. With the "HUD" setting (`+60`) and
a target, the target's refresh (virtual `+18`) runs first; its type `+10`
then picks what draws:

- **An actor (type 2) or furniture (type 4): the box.** The focus
  (virtual `+1C`, `+14` for both) is projected to the screen point P: a
  `0043A150` descriptor at the focus, published by `0048FB40`, through the
  view record's projector table (`+50`, `00546E60`) slot `+8`, the near
  local entry `004EB5A0`, at the origin. `+5C0` halves (toward zero) and
  the sight is off. Each of the three corners `+5B4/+5B8/+5BC` (an X and a
  Y WORD) draws a bracket in style `0x21`, sized `(5 - i) * 3` by
  `(5 - i) * 2` (three halves of that in layouts above 0), then moves a
  quarter of the way to the next corner (each WORD's difference truncated
  toward zero); the last moves onto P. A last bracket on P is `+5C0 / 500`
  tall in the narrow form, so as `+5C0` halves each frame it closes onto
  the target.
- **An actor's bar and caption.** For a type-2 target whose actor (virtual
  `+10`, `004827C0`; the other types' is `00401300`, NULL) resolves and is
  not the vehicle itself (`+84` compared), the health bar `0047F080` draws
  under P at `(P.x, P.y + 2H/100)`, `14H/100` long and `H/100` thick (H
  the surface's height), filled by `0x10000 - ((max - health) << 16) /
  (max + 1)`, the quotient's low WORD (`+A0`/`+A2`). Below `0x800` (1/32)
  it blinks between empty and `0x800` every 15 clock ticks; with no health
  it is empty. Then the caption at P through `00415650`: an actor of
  another side (`+B4`) than the vehicle's, and not side 0 or 1, is hostile:
  player command `0x21` (`0046C760(0x21, 0)`, whose text, shown once, is
  "When your weapons have locked on to a target, the target triangle goes
  red", H0 `0x2D5`), then "Lock" (H0 `0x155`). Any other actor, own side
  or side 0 or 1, is "Caution" (H0 `0x156`). Every actor gets the bar,
  whatever its side: Infest01's factory, side 0, shows it full under
  "Caution".
- **Another type (8, a terrain point; 16, an object).** `+5C0` rises by
  50000 up to 500000 and a bracket `+5C0 / 500` tall draws on the first
  corner in the narrow form; at 500000 it is a thousand pixels tall, off
  the screen. So when a lock is lost the closing bracket opens up and away.
  The sight follows.
- **No target.** `+5C0` rises; the sight follows.

The sight draws with the setting when no box did. `0041FF20` gives the
fire direction (it also writes the behavior's lead `+20`; under the
special view `G+5C5` it frees the target and writes none, which only the
free look `+1C0` reaches, so the port stops the plan there). For
each ring `i` from 0 to 2, the vehicle's position plus the direction
shifted right by `2 - i` (a quarter, a half, all of it) is projected as
the focus is; the corner moves a quarter of the way to it and draws a
bracket in style 1, sized 15x10, 12x8 and 9x6 (30x20, 24x16 and 18x12 in
layouts above 0).

The H6 styles are `0x21` RGB `90 2C 2C`, red, for the box and the bar's
empty part; 1, `16 48 30`, a dark green, for the sight; and 0, `21 6C
48`, green, for the bar's frame and fill. So red means a lock on an actor
or furniture; a point or no target leaves the green sight.

`0047F350(renderer, point, colour, w, h)` draws a bracket: three
one-pixel effect quads of sprite `0x22B` with vertex DWORD `0x40` and
descriptor 0. Every height goes through the focal ratio (`hy = h * F_y /
F_x`, `00546F24`/`00546F20`). The top line runs `w` either side of the
point at `hy` above it; the two sides run from `(x - 8h/9 - 4w/9, y +
4hy/9 - 8wy/9)` to `(x - 8h/9 + 5w/9, y + 4hy/9 + 8wy/9)` and mirrored,
each ninth truncated (`wy = w * F_y / F_x`). A zero `h` draws the narrow
form: half width 7 and height `max(w, 7)`. `0047F080(renderer, x, y,
fill_w, w, fill_h, h, rest, bar)` draws a bar `w` wide and `h` tall with
its bottom left at `(x, y)`: a frame of sprite `0xE9` in `bar`, one pixel
right of and below it, then two quads of sprite `0x22A`, the filled part
in `bar` and the rest in `rest`. With `fill_h` 0 the fill runs `fill_w`
across from the left, as in the box; otherwise it rises `fill_h` from the
bottom, as in the guided missile's view.

The vehicle's constructor never writes `+5B4..+5BF`, so retail's corners
first ease from heap residue: Infest01's held the text "mmence.\0Good",
still thousands of pixels off the screen five frames later; Infest03's
"age, but we "; Infest02's and Infest04's zeros. The port starts them at
zero, a fresh heap's value (`RETICLE_CORNER_RESIDUE_APPROXIMATION`); only
the first glide differs.

The [plan](../../crates/inf-game/src/vehicle_hud.rs) draws the reticle
from the target the [arena reads](../../crates/inf-game/src/object_runtime/player/hud.rs)
(refresh, focus, actor, then the direction), and the HUD pass writes back
`+5C0` and the corners and runs the player command. `targeting.js` (bundle
`.tmp/infestation-ttd/20261012-targeting`) counts, over HUD calls,
brackets, bars and `0041FF20` calls: Infest01 717/1939/59/440, Infest02
881/3079/91/657, Infest03 645/2276/104/498, Infest04 2953/10171/1031/2357.
Infest01's HUD calls split into 229 with the sight alone, 210 with a
point's bracket and the sight, 44 with furniture's box, 59 with an actor's
box and bar, and 175 under a menu. Hostile locks run 46 times in Infest02,
never in Infest03 and 335 times in Infest04. The alternate HUD `0041B230`
and free look never run in the four recordings. The
[plan tests](../../crates/inf-game/src/vehicle_hud_tests.rs) match
retail quad for quad in Infest01's call 5 (the sight), call 300 and frame
39 (points), frame 50 (furniture), frames 427 and 428 (the factory) and
Infest02's frame 417 (a side-18 actor at full health, "Lock" after
command `0x21`); [the arena's](../../crates/inf-game/src/object_runtime/player/hud_tests.rs)
cover the refresh, the actor and the direction's side effects.

### Free look's panel and the guided missile's view

Two more views draw targeting brackets and bars; with the HUD they are
the only callers of `0047F350` and `0047F080`. The port runs free look's
control and aim ([`free_look.rs`](../../crates/inf-game/src/object_runtime/actor/derived/selector2/free_look.rs), on 2026-10-17, when V stopped the
player's vehicle at `0041967C`) and, since 2026-10-19, its view
([`free_look_view.rs`](../../crates/inf-game/src/free_look_view.rs)),
after retail was seen to show more of the HUD under V. The view matches
its receipt call for call over 62 cases
(no target, other types, actors on every side, names past the pool, gates
with and without a destination, the bar's fill and blink, furniture of both
sides, surfaces and focal ratios), and a lab run of Sanrel-Dek's first
start under V (pass 270) shows retail's scope, rings and "Targetting / Military
barracks / Allied" panel where the port draws them. The missile's view
matches four Infest04 receipts.

**Free look.** The V key's control byte (player `+3F`; the drive's block
starts at player `+4`, so `004195F0` reads it as `+3B`) reaches `00421DC0`,
which toggles the vehicle's `+1C0`. While it is set, `00421DC0` runs every
drive and the same toggle fires on thrust (`+1C`, `+3A`), fire (`+20`), the
grenade (`+24`), a mode key (`+34`), the A, C, D, L, Left Ctrl or `+3E`
controls, or V again. With Right Ctrl (`+40`) held the pitch (`+18`) steps
the world manager's WORD `+5C0` by `0x1000` within 0..`0x7FFF`, and a
primary or secondary weapon key (`+28`, `+2C`) sets it to `0x7FFF` or 0
(`0x3FFF` on entry). The camera takes its special view (mode 4, `G+5C5`).
The aim skips the actor scan and, at `0041FCE8`, casts its sight test from
`00421D50`'s eye: the stored position `+1CA..+1CE` (which follows the hull
once it moves more than about 5 units) raised by 50, looking along the
stored forward `+1D0` and up `+1DC` (renewed when the hull turns more than
about 10 degrees) turned by `+1C6` in XZ and `+1C8` in pitch, 7500 units
out (`00520A58`); what the test meets becomes the target, as on the
look-ahead. The vehicle draws `0041B230` instead of `0041C320`: the light
`00490030` in grey `0x808080` along the terrain's direction `00548380`,
`004A4400` (a batch it never flushes), four quads of sprite `0x29E` (a
scope; +9C, vertex DWORD `0x4000`, descriptor 0) around the screen's
centre, each 3H/4 tall and that times the focal ratio wide, then the
"Targetting" panel (H0 `0x163`) for the target after its refresh, the
compass (`0041BF60` at (15/100 W, 4/100 H), radius W/10) and the pitch ring
(at (4/100 W, 2/10 H), radius 15H/100), both front halves only, and a
narrow bracket in style 0 on the centre. The panel's lines are `004A3AB0`
in style 0, one every 4H/100 below the centre, each centred on its width.
For an actor (type 2) that resolves, is not the vehicle and is not named
"Bonus Time" (`+76` `0x113`): "%s:" of `0x163`, its name (H0 `+76`, used as
the format; none for 0 or a slot past the pool), "Allied" (`0x15D`) for the
vehicle's side and sides 0 and 1, else "Alien Control" (`0x15C`), and a
health bar `18H/100` wide and `H/100` tall on the centre's row (style
`0x21` for the rest, 0 for the fill), which blinks every 15 clock units
once less than 1/32 is left; a gate (resource `+30` `0xF`, selector 30)
adds "Warp to %s" (`0x15F`) with the name H0 `+15C + 0xE0`, or
"Surface-bound warpgate" (`0x160`) while `+15C` is 0. For furniture (type
4), when `00436BC0` answers: `0x163` alone, its name (the first WORD of
`00475320`'s record) and "Alien Control" or "Allied" from flag 4 of the
furniture record of its cell. (A cell without the record flag would read
the default `005483C4`, but `00436BC0` answers 0 for one.) Any other
target, or none, raises `+5C0` by 50000 up to 500000 and shows no panel.
A furniture name, the side texts and `0x160` reach `sprintf` as formats
unchecked, so one past the H0 pool crashes retail; the port stops the view
with a logged error there. None of the four recordings enters free look
(`0041B230` and `00421D50` never run).

The scope spans `3H/2` times the focal ratio, more than any 4:3 surface's
width, so in retail nothing of the world shows outside it. On a wider
surface (the port's widescreen) the scope keeps its shape and leaves a band
either side; the port fills each with black (renderer `+84`, colour 0) so
the world shows only through the scope there too.

**The guided missile's view.** When a player's unit is a missile (its
`+D8`), the missiles' virtual `+84` (`00456CF0` for selectors 25 and 28;
selector 68's `00458F30` only calls it) draws instead: `00456B50` first
while `+150` is set; then, with the "HUD" setting, a narrow bracket on the
screen's centre in style 0 and five upright bars `W/100` wide and `18H/100`
tall, style 1 for the rest and 0 for the fill: at 88, 91 and 94/100 W with
their bottoms at 9/10 H, filled from half by `+94`, `+98` and `+9C`
(`(value * h >> 17) + h/2`), and at 60/100 W and 40/100 W with their
bottoms at 6/10 H, the height above the ground or water out of 2000 and the
speed `|+8E..+92|` out of 3000. Then the shared `0047F690`, the player's
`0046BE00`, and `+13A` as "%d" (above 1000, in thousands rounded up)
centred at (W/2, 6/10 H). Infest04 runs it 114 times, all for one selector
68 missile (resource 129, weapon row 7) at 640x480: `targeting.js`'s
`missile()` reads the narrow bracket at (320, 240), the three bars at x
563, 582 and 601 rising from 432 (half full while `+94..+9C` are 0), the
height and speed bars at x 384 and 256 rising from 288, and "30" for `+13A`
30000, all as above.

The port draws the view ([`missile_view.rs`](../../crates/inf-game/src/missile_view.rs),
2026-10-19). The bars are `0047F080`'s vertical mode: its fifth argument,
the fill, nonzero, the frame `+1` right and below as the horizontal mode's,
then sprite `0x22A` in the bar's colour from `y` up the fill and on to the
top in the rest's colour, its last corners written by `0040EF40`; a zero
fill falls back to the horizontal mode. The static `00456B50` draws every
row of the surface in one-pixel quads (renderer `+9C`), each a random H3
sprite `0xBF..0xC2` mirrored at random, side by side across: two process
RNG draws a quad, so a disrupted view moves the world's RNG. `0047F690` is
the player's `0046BE00`, and the view then calls `0046BE00` itself, so a
held missile's player gets its menu and messages twice a frame; without
the "HUD" setting it gets neither. check_missile_view.py
runs `00456CF0` over 40 states, its static over 7 and the bar over 8 in
Unicorn, and the port matches each call and quad.

### The hold display `0041D59A`

The left circle shows what the vehicle carries. For each of the three hold
slots `+1A4` whose handle resolves to an actor (`00412780`), the HUD sets up
an identity view at the origin (eye 0, `00547590`'s basis, `0048FB00` with
the world's fade) centred at `18/256 W + i * W / 30` across and `89/100 H`
down (`84/100 H` for slot 1, so the middle one stands higher), both signed
divisions of the surface's size. It lends the actor the position (0, 0,
its size `+28` plus 900) and the identity basis turned `400 * clock` in XZ
(`00491530`), calls the actor's own draw `+2C` with that view, then gives
back the actor's position and basis. The scientists' `+2C` is the common
actor draw `0047D910`: the grid light at its (lent) position, then its
model with its own animation values.

The [port](../../crates/inf-game/src/vehicle_hud.rs) plans one
`CarriedActor` per filled slot and the HUD pass draws it through the
world draw's object path (`ObjectArena::draw_with_lent_pose`), with the
world's light grid and fog colour. `hud-carried.js` reads every carried
draw in Infest02 (6) and Infest03 (6): selector 12 scientists of size 132
at (0, 0, 1032), the basis turned by the clock, the view's focal (512,
512) and viewport (1024, 768), and the centres (72, 683), (106, 645) and
(140, 683) the
[plan test](../../crates/inf-game/src/vehicle_hud_tests.rs) expects.
Replaying a session that loads a profile with three scientists in its hold
draws them standing in the circle, the middle one higher.

The carried actors draw over the renderer's default surface pair, not the
world's: in the lab (`helniazine-hold.json`, with a hook on each Ground
callback) every Ground call of the three figures went to `004EBCD0` (Y =
-300) and none to the terrain's `0044FF50`. The scientist model's ground
shadow (its two `0A2B` calls, then the quads over them) therefore falls
300 below the figure, out of sight. The port had drawn them over the
world's terrain pair, so each shadow took the terrain's height near the
world's origin and showed as a dark ellipse above the figure's head (the
user's report at 4K, 2026-10-18). The HUD pass now sets the default pair
aside for each carried actor and restores the world's after
(`SoftwareScene::select_default_surface_callbacks`); against the empty
hold, retail and the port now differ only in the figures.

### The switch panels `0041A740`

While a weapon key is held (`+534` primary, `+535` secondary, see
[weapon cycling](#weapon-cycling-and-the-panels)) the HUD draws that row's
panel instead of its icon: `0041A740(renderer, view, icon, W, H,
descriptor, row, timer, slide, models)`, the timer and slide being `+52C`
and `+540` for the primary or `+530` and `+544` for the secondary.

- **Scale.** The panel grows over its first 125 ms and shrinks over its
  last: a timer above 875 gives `(875 - timer) * 100 / 125 + 100`, one below
  125 gives `timer * 100 / 125`, otherwise 100. Every offset from the icon
  toward the surface's centre `(W/2, H/2)` is scaled by that percentage, so
  at 0 the whole panel sits on the icon.
- **The ring.** The row's eight weapons (primary 0..7, secondary 8..15)
  sit at the angles `((i - selected) << 16) / 8 + slide`, 0x10000 a turn,
  the selected one at the top: `W/4` times the quarter sine across and
  `(focal_y * W / focal_x) / 4` times the cosine up, each product shifted
  15 into a signed WORD. Each model (`0049FEA0`, the icons' descriptor and
  spinning basis) has its projection centred there and sits at 500 when
  selected, 1000 when the player owns its upgrade (`0046BDC0` of the IDs at
  `005305B0`: 0x32, 0x9B, 0x98, 0xA1, 0xA0, 0xA3, 0, 0x33, then 0, 0xA9,
  0xA8, 0xAA..0xAE) and 5000 otherwise, where the view's fade dims it.
- **Texts.** For an owned weapon the panel queues text packets
  (`004A42B0`, callback `00423370`, which reaches renderer `+0C` and
  `004A3AB0`): the selected one's name seven eighths of the way out
  (H0 `0x14C..0x153` Cannons..Cranial Implanter, `0x144..0x14B`
  Grenade..Napalm), then each count from 0 to 9 centred below the name or
  the model by the width of "AI" (`005305E8`). Each is one pass in H6
  style 0 with H4 font 0. Their sort word comes from font sprite 50, whose
  variants are blended, so it is `0xC0000000` (0 without material
  sorting): they replay after every model face, and equal words replay in
  the order queued.
- **The frame.** Last, four mirrored quarters of sprite `0x2A6` about the
  scaled centre, `scale * W / 250` across and `(focal_y * W / focal_x) *
  scale / 250` up.

The [port](../../crates/inf-game/src/vehicle_hud.rs) plans the panel and
the [HUD pass](../../crates/inf-game/src/player_hud.rs) draws the models
at their distances and the texts after the flush. On a widescreen surface
the sizes take the HUD's 4:3 width and the ring moves toward the surface's
own centre. The [receipt test](../../crates/inf-game/src/vehicle_hud_tests.rs)
replays all 40 panels of Infest02 (27) and Infest03 (13) from
[weapon_panels_native.json](../../crates/inf-game/tests/fixtures/weapon_panels_native.json)
(`weapon-panel.js`): every model's centre, distance and model, the four
frame quads, and the texts, their anchors and their order.

### The radar's blips `004723F0`

Between its first RNG draw (`00472409`) and the annotation labels, the
radar's contents walk the actor list `G+48`
([radar_blips.rs](../../crates/inf-game/src/radar_blips.rs)). The
ordinary radar runs it while the player owns the Scanner (grant 0x11, "Shows
the location of any other vehicles giving off an EMP trace"); the terrain
radar always runs it unless the map's jitter has reached 0x4000.

- An actor enters when its flags hold 0x21 without 0x10000, or its `+70`
  answers: the pickup spawners (selector 53), whatever their flags. The
  `+B8` arm, which would draw four dots, is null on every class. The walk
  reads an actor's X/Z only once it enters. A carried unit's capture
  (`0047D7C0`) clears flags 1 and 2, so the scientists a respawn puts in the
  hold, whose child factory gets a NULL position (`0046AD1C`), pass
  unread until the hold's update gives them the vehicle's. The port read
  every position first, and such a scientist stopped the whole HUD on the
  first frame.
- The vehicle (`+64`) shows on even sevenths of the clock.
- Its offset from the eye's X/Z WORDs is jammed while the jitter exceeds 7:
  each axis gains a process RNG draw modulo a quarter of the jitter (two
  draws per entering actor). It turns by the view's heading as the labels'
  does, and a distance r lands at the half size times the quarter sine of
  2r. From 2r = 0x4000 on, the blip is pinned to the rim as one pixel. It
  shows when the parity of a 16-bit word, the first RNG draw rotated right
  once per pinned blip, differs from the clock's sevenths.
- A spawner draws the Resource scanner's marker (grant 0x13, "This shows
  underground resource deposits on your radar") through `004736E0`: an
  effect quad of sprite 7, depth 0x40, half its count across, in style
  0x22, 0x23 or 0x24 by its kind (blue, green and red crystals). Without
  the upgrade it shows nothing.
- Anything else is a dot `004719E0`, written straight into the locked back
  buffer: five pixels in a plus about (x + 1, y), or that one pixel when
  pinned, in its team's H7 colour (`004721A0`: teams 1 and 2 slot 0x56,
  enemies 0x12 slot 0x57), halved through renderer `+30` while it dies
  (flag 0x8000).

The port stores each dot's WORDs straight into the frame, as the terrain
radar's pixels, with no depth test or write; a dying actor's colour is
halved through the mask the renderer derives from its material shifts
(`[6, 1, 4]` for RGB555). Infest03's first 60 radar walks (`radar-blips.js`,
[radar_blips_native.json](../../crates/inf-game/tests/fixtures/radar_blips_native.json))
replay exactly: 3091 dots, 128 markers and the RNG each left. Friends are
`FFC0` (yellow) and enemies `F800` (red) in that recording's RGB565.

### The terrain radar `00470170`

The Radar (grant 0x12) draws the ground itself: a disc of the map at
`0054ED78` around the vehicle, turned with the view, under a shading
overlay ([terrain_radar.rs](../../crates/inf-game/src/terrain_radar.rs)).
It writes 16-bit pixels straight into the locked back buffer (`0050DCF0`,
display `+1C` and pitch `+20`), so nothing sorts or depth-tests them, then
calls the contents `004723F0` with a skip byte of 0: the walk always runs,
so the actors' dots show as with the Scanner, then the labels.

- **Extent.** The square is twice H10 global slot 0's dimensions, its top
  left the HUD's centre WORDs less those dimensions. A corner left of or
  above the renderer's clip rectangle (`004A4C40`, renderer `+48..+4E`)
  draws nothing and skips the contents; the right and bottom edges cut the
  columns and rows short.
- **Quadrants.** Slots 0 and 1 store one quadrant, `W` cells a row: four
  pairs of signed WORDs and a mask byte per cell ([sample
  assets](SAMPLE_ASSETS.md)). Columns from `W` on read cell `2W - 1 - x`
  with each pair's first WORD negated; rows from the middle on read row
  `2W - 1 - y` (twice the width: every retail grid is square) with the
  second negated. Slot 2, an overlay index per output pixel in rows of
  `2W`, is never reflected.
- **Samples.** A pair (a, b) turns by the view's heading, `c = ftol(cos *
  32768)` and `s = ftol(sin * -32768)` from the X axis in XZ (both
  quotients stored as floats, unlike the contents' rotation): `X = eye +
  (a*c >> 15) + (b*s >> 15)`, `Z = eye + (b*c >> 15) - (a*s >> 15)`. The
  map byte at `(Z & 0xFF00) + ((X & 0xFFFF) >> 8)` picks one of 43
  colours.
- **Colours** (`00471D40`). For shades 3 to 10, each of the five terrain
  materials H7 `0x75..0x79` with every channel read through the renderer's
  material shifts, times the shade over 8, held to `0x7F` and repacked to
  its five bits, so no channel passes half intensity. Entries 40
  to 42 are H7 `0x1F`, `0x22` and `0x21` whole; the map's constructor
  `00471C40`, its only writer, stores 0 to 39 (shade times 5 plus the
  material, 0 below the world's plane).
- **Mask 0** draws the whole pixel. A process RNG draw below the map's
  jitter jams it: a second draw's parity picks H7 `0x20` (even) or `0x3C`
  (odd), halved through the mask colour H7 `0x0F` and added to the overlay
  colour (H7 `0x62..0x71`, by slot 2's index). Otherwise the four samples'
  colours are averaged in pairs (3 with 2, 1 with 0, then the two),
  halving through H7 `0x0F` at each step, and added to the overlay colour.
- **Mask 15** leaves the pixel. **Any other mask** draws a rim pixel without
  the overlay: bits 1, 2, 4 and 8 replace pairs 0 to 3 by the destination
  pixel itself, and the four halved terms average as the samples do. The
  retail masks (`0, 4, 5, 12, 13, 15`) shade the disc's edge into the
  frame.

The RNG is drawn once per mask-0 pixel in scan order (rows from the top,
each row's left half then its right half) and once more when jammed; the
contents' draw follows. The port plans the radar in the HUD
([vehicle_hud.rs](../../crates/inf-game/src/vehicle_hud.rs)) and the HUD
pass stores the pixels into the frame. The software frame reads each rim
pixel's destination WORD back (each channel's top five bits) as retail
does. The OpenGL device keeps no CPU pixels, so there a rim pixel draws its
colour over black, scaled by four over its sample count, blended with that
share as alpha: a labeled approximation of the halving arithmetic.

check_terrain_radar.py runs the original
routine, its colour table, `_ftol` and RNG unpatched under Unicorn over 25
synthetic cases (every quadrant, mask and noise branch, a draw equal to the
jitter, the four clip edges, a zero heading, the width-reflected rows and
RGB565), and both
headings' prefixes on eleven headings, four of them where the float-stored
cosine lands one further from zero than the contents' unstored one;
[terrain_radar_native.json](../../crates/inf-game/tests/fixtures/terrain_radar_native.json)
holds the pixels each case changed, the RNG and the contents call, and the
[port](../../crates/inf-game/src/terrain_radar_tests.rs) matches all of
them. Infest03's first terrain radar (`terrain-radar.js`,
[terrain_radar_infest03.json](../../crates/inf-game/tests/fixtures/terrain_radar_infest03.json))
replays from retail's own inputs, colours (an RGB565 display: material
shifts `[7, 2, 4]`, mask `7BCF`, the overlay a grey ramp of 16 steps from
black) and process map, with the grids of `1d4.ovl` (76 by 76) read from
the repository through the port's global slots: all 17972 stored WORDs
match in order and place, the 300 rim pixels read the destinations
retail read, and the RNG agrees.
