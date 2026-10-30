# Infestation gameplay screen runtime

This document owns the phase-4 (and shared phase-6) gameplay screen contract:
factory, constructor, input tables, tick/draw dispatch and teardown. Phase
dispatch and resource lifetime remain in [Runtime foundation](RUNTIME_FOUNDATION.md),
world construction in [the briefing document](BRIEFING_RUNTIME.md) (fresh-heap
WORDs adopted, allocator history a follow-up), and the world tick/update
simulation is a skeleton: `gameplay_screen.rs` owns factory/tick/draw order,
the [player block](#player-block-of-00478f20) drives the player's vehicle,
and [shared world camera](CAMERA_RUNTIME.md) owns selector-16 pose and selector-2
Chase through actual process/arena state for all loaded worlds;
`world_scene::draw_world_frame` owns the native world-draw orchestration
(`00442080` view setup, `00440F00` preamble, `00488020` depth state and
shared Flag0 object manager through the existing scene backend).
[World-22 masks and location caption](GAMEPLAY_OVERLAY.md) own the subsequent
`00441003..0044120F` overlay and the base screen's `+C0` caption clock.
[Shared object drawing](OBJECT_DRAW_RUNTIME.md) owns terrain/cone admission,
class callbacks, packet lighting, flash and distinct late execution; the
[live getter owner](OBJECT_MODEL_CALLBACKS.md) binds actual source/clock values.
Sky (`00440DA0`) and the shared `0044AA10` terrain traversal now reach the
production deferred/pixel backend. The
[terrain owner](TERRAIN_RUNTIME.md#shared-production-pixels) owns that bridge,
native D6/colored-quad consumers and their evidence bounds. Process particle
updates and sprite drawing now share the Session pool; required particle lights
feed the recovered world grid, and positional one-shots reach the SDL PCM mixer.
The [shared object-light pass](OBJECT_LIGHTING.md) now runs after active particles
over actual Flag0 nodes; active vehicle callbacks retain named per-object
omissions. The shared draw manager now follows this pass and owns offline
vehicle drawing with actual tint and profiling state. Network vehicle drawing,
other custom vertices and complete retail composition remain open.
The [shared furniture draw](FURNITURE_RUNTIME.md) now supplies its native
basis, record writes, effects and model callbacks. [Water](WATER_RENDERING.md)
now retains each view's actual world flags and clock. Terrain-emitter
drawing remains a per-feature boundary;
elements, text and the worker tail also remain open. Complete `004875A0` phases,
`00464B30` player update, HUD, collision, weapons, damage and persistent audio stay open.
This large authority retains the gameplay contract and cited mission-18
evidence chain. New terrain/pixel authority lives in `TERRAIN_RUNTIME.md`,
furniture submission in `FURNITURE_RUNTIME.md`, and shared object drawing in
`OBJECT_DRAW_RUNTIME.md`. No new draw-manager authority is appended to the
older mission evidence below. A later history split
must preserve the capture-ledger anchors and unique RE evidence.
Growth rule: H8 execution-context / sim-state evidence
(listeners, scratch records, refine tables) gets its own document from
birth under the proposed follow-up objective; face-owner evidence stays
here until a second consumer needs it.

## Screen class and factory

| Address | Role |
|---|---|
| `00440C60` | Factory: allocates the screen, constructs through `00440990` |
| `00440990` | Constructor over inner `004408B0` plus base `00437F90` |
| `00523370` | Screen vtable, published by the constructor |
| `00440B50` | Class query `+18`: returns 5 (shared by phases 4 and 6) |
| `00440B60` | Reuse `+1C`: accepts requested phases 4 and 6 only |
| `0040B80` / `0040BE0` | Deleting destructor / teardown |
| `00442B50` | Tick `+04`: device gates, settings latch, inner worker, phase-5 request (tail open) |
| `00442080` | Draw `+08` (world draw through `00488020`; details open) |
| `00440930` | Presentation `+0C` |
| `004947F0` | Select `+10` (standard) |
| `00440BA0` | Deselect `+14`: early return on network/complete/attract, else `00487230` plus a phase-5 request (fully owned below) |
| `00439470` | Held-bit dispatcher `+20` |
| `00443760/70/80/90` | Held slots `+28/+2C/+30/+34` (semantics open) |

The session tick *callback* for gameplay phases is the separate `004388D0`
(world update plus elapsed-time accounting); the screen `+04` virtual is
`00442B50`. Teardown `0040BE0` resets the vtable, releases binding sets at
both screen `+0x60` and the `+0xFC` sub-object, then destroys the base
`00438080`.

## Constructor order

`00440990` runs, in order:

1. Inner `004408B0` over the base parameters, then base `00437F90`;
   vtable publish `00523370`.
2. First device enumeration `00439B30` over the screen's inline arrays
   (digital roots below, analog table addresses snapshotted from the shared
   cells), slot attach `00439D00` first from the global table `00547450`
   then from the screen struct, and context select `004947E0`.
3. Live player lookup `00488AA0`.
4. `+0xFC` sub-object construction (`0x60` bytes through `004959E0`) with
   its own second device enumeration over a second inline array set.
5. Settings `+0x70` read through `00463AB0` into terrain-spacing setup
   `004502C0`.

The `+0xFC` sub-object owns input bindings (teardown releases both sets),
is enumerated after the player lookup, and carries the large `00469xxx`
vehicle/state table family. Whether it is the player-input owner is
unproven; it is named only by its screen offset.

## Authored gameplay binding tables

Digital roots decode statically with the generic digital table parser
(retail corpus test pins every count, shared pair and descriptor):

- Screen struct: slot 0 `00534290` (12-row keyboard: gameplay action
  callbacks `00443470`–`00443720`, two-byte chords, Alt+Enter; there is no
  Alt+Esc exit binding), slots 6/7 `00522A78`, slots 8/9 `00522AE0`
  (4 rows each, identical content from distinct tables).
- Sub-object struct: slot 0 `00534570` (45 rows over the `00469450`–
  `004698E0` vehicle/state family, including null-callback published-value
  descriptors with flags 6 and Alt-chorded repeats), slot 2 `00534CC0`
  (16 rows), slots 6/7 `00522D98`, slots 8/9 `00522FB8` (26 rows each,
  identical content from distinct tables).

Descriptor identities, callbacks and flags are pinned as an exact set; the
`00443xxx` gameplay actions and the `00469xxx` vehicle/state callbacks keep
open semantics. Only Alt+Enter survives from the shared system trio.

Analog input: the screen struct snapshots the current contents of
`00533398` and `005333E8` into slots 2 and 8/9 (the menu axes); the
sub-object struct stores `005231D8` in slot 2 (the joystick's X, Y and Z
to the player's `+5C`, `+60` and `+64`, Q31 `0x40000000`, scales 4, −4
and −4) and `00523238` in slots 8/9 (the pad's axes 0, 1 and 3 to the same
three at 1024, −1024 and −1024). Their descriptors lie in `.data`'s zero
fill. The joystick's path to the controls is in
[the input notes](INPUT_RUNTIME.md#the-joystick-the-mouse-and-the-input-mode).

## Tick and draw first pass

Tick `00442B50` queries `004399F0`/`00439A20`; when the first yields zero
it requests phase 5 (`00438380(5)`, the in-world menu), otherwise it sets
settings `+8` from the second; either way it runs the inner worker
`00440D80` ([detail](#screen-tick-detail)).

Deselect `00440BA0` (thiscall, `ret`, fully owned): returns early with
the network flag `0055F448` set, the world-object field `+0x250` equal to
`0x16` (22), or the attract flag `0055F14C` set; otherwise it runs
`00487230` and requests phase 5 (`00438380(5)` via screen `+E4`), whose
result is discarded. Teardown `0040BE0` is confirmed exact: reset vtable
`00523370`, release binding sets `00439D50` at screen `+0x60` then
`+0xFC`, destroy the base `00438080`. Immediately after teardown lies one
open thunk, `00440C40` (`mov ecx,0x534F78; jmp 0048EE20`).
The [`gameplay_menu`](../../crates/inf-game/src/gameplay_menu.rs)
owner implements the deselect decision alongside the `+0x40..+0x64`
gates above.

Draw `00442080` reaches the world draw `00488020`, which sets renderer
depth enable/write through virtual `+10`; viewport, entity submission and
HUD ownership are open. Presentation `00440930` is not the standard
`0040D1D0`: it hands `00438120` H7 slot `0x72` instead of `0x20`, so each
gameplay frame is cleared with the world's sky colour. The sky dome (H8
`0x116`) is open at the top and closed below by authoring, so that colour
fills the screen above it: blue on Level 1, red-brown in the intro, where
slot `0x20` is black
(ledger).
The port's `PresentationClear::Gameplay` uses it for every gameplay screen.
The HUD pass that ends `00440F00` (`00441456..004414E5`) calls each
player's unit's virtual `+84`. Of it the port draws the
[vehicle HUD](VEHICLE_DRIVE_RUNTIME.md#the-vehicle-hud-0041c320) and the
player message's line and hint boxes ([PLAYER_MESSAGES.md](PLAYER_MESSAGES.md#box-draw)),
whose `0046BE00` draws nothing more offline, and the
[reticle](VEHICLE_DRIVE_RUNTIME.md#the-reticle-0041c92e).

Before it come the world captions (`0044121E..00441456`), each one
`004A3AB0` line in H4 font 0 and H6 style 0. "Bonus Time: N" (H0 `0x113`)
goes at (10, 40) while `0048E290`'s count is positive: the H14 header's
`+0` less 20 per animation tick (`005471C0`), and nothing when `+0` is zero,
as it is in every authored world. Unless the first player's message box is
up (`00488AA0`, byte `+88`), two lines follow, centred by `00440E20` a tenth
and 14 hundredths of the way down: "Countdown: S.T" (H0 `0x114`) while the
mission's doom countdown W+24 runs, outside its second stage W+18 and the
attract demo (`0055F14C`), and W+2A's text with the count W+28, "Warp power
hoops: N" from a [gate](SELECTOR30_RUNTIME.md#update). A text past the H0
pool's count prints as sprintf's "(null)". The port's
[`world_captions`](../../crates/inf-game/src/world_captions.rs) plans them
and the HUD's text pass draws them; the original segment, with `0048E290`,
`00440E20` and sprintf native, runs over 55 cases in
`check_world_captions.py`, and the port
matches each.

## Input dispatch architecture

Gameplay input runs through two enumerations under one screen context. The
`+0xFC` sub-object is a `memset`-zeroed 0x60-byte input-state buffer
(`004959E0` is a byte-fill routine, verified by disassembly — no vtable is
published there), not a screen owner; teardown releases its binding sets
alongside the screen's. Which runtime consumers read the buffer besides the
input sets themselves stays open.

The constructor `00440990` runs `00439B30(screen)` and `00439D00` for the
screen's tables, then takes the first player (`00488AA0`), zeroes the
sub-object and runs `00439B30(player)`/`00439D00` for the `00534570` family.
The sub-object's rows therefore acquire their consumers over the player's
callback context and share the eleven references the player constructor took
(Left/Right `+68/+6C`, Dive/Climb `+70/+74`, Thrust `+78`, `0053A380` `+7C`,
Fire `+80`, Grenade `+84`). The tables are the live records phase 12 edits:
the sub-object's keyboard rows are the 38 records below `[00534CA0]`, the
screen's first six rows the records below `[00534410]`, and the joystick rows
take each button's bound descriptor. The
[gameplay screen](../../crates/inf-game/src/gameplay_screen.rs) installs
both, with the settings' current bindings, before selection; teardown releases
the screen's sets, then the sub-object's. Its test rebinds Thrust to K and
holds the player's own Thrust consumer with it. Rows with callbacks call the
player's [key callbacks](#player-key-callbacks-00469450004698e0) with the
player as context; the gameplay router finds the player whose construction
holds that context, while the screen's own context still reaches the screen.

The player update `0046A260` reads those consumers each tick into the
control block at player `+4..+47` and passes it to the vehicle; see the
[player block](#player-block-of-00478f20).

Keyboard callbacks (screen slot 0) forward key events into gameplay
virtuals resolved through `004391C0`:

- `00443470`: calls virtual `+0x54` on the `[session+0x18]`-routed object
  with the key value and stack arguments.
- `004434A0`: on press, dispatches held-bit slot 2 via `00439430`.
- `00443700`: on press, calls virtual `+0x64` on the same routed object.
- `00445200`/`00445220`: on press, dispatch held slots 1/2; on release,
  `00445200` clears the screen `+C4` latch bit.

The `+0xFC` enumeration carries the 45-row vehicle/state family: the
[key callbacks](#player-key-callbacks-00469450004698e0) plus null-callback
published-value descriptors with flags 6. Only Alt+Enter survives from the
shared system trio on either struct: gameplay binds no Alt+Esc exit and no
noop.

## Player block of `00478F20`

The world object update `00478F20` runs a player block in every loaded world.
Before its update loop, `0046B010` takes the first player (the same list read
as `00488AA0`). `0046A0E0` resolves the player's actor, `+D4` while `+D8` is
the process NULL, else the `+D8` actor (the camera after the vehicle's
death), ORs flag `10` into it (dirty bit 2 when a low flag changes) and
returns it; `00479040` then moves it to the end of the alternate list when
its flag 2 is set, so the loop never reaches it. While `+D8` is set, a mode
request in `+34` lapses once the vehicle is off its update list. With byte
`+58` set, a `+D8` actor over a dying `+D4` vehicle (the wreck) re-attaches
the player to the camera `00478E00` each tick; otherwise a mode request, or
a `+D8` handle that no longer resolves, attaches the process NULL over a
vehicle at lifecycle 3. Either way the camera then follows the `+D4` vehicle
again (`004825D0`, `00410E70`) and `+34` is cleared. After the loop and
before the header swap, `0046A260(delta, actor)`:

1. counts down the `+54` message-key repeat in whole milliseconds; at expiry
   it rearms (`+100` past the overshoot while `+50` holds a key, else zero)
   and runs `0046D240` on the held [message keys](#player-key-callbacks-00469450004698e0);
2. while no message is shown (`+88` BYTE 0 clear), reads the eight digital
   references with `004F0040`, each capped at the delta (a quarter of it while
   `G+5C5` is set): `+18` Dive−Climb, `+14` Left−Right, `+1C` Thrust−`+7C`,
   then Fire/Grenade into `+20/+24` unless the `+F4` lockout (counted with
   the raw delta) holds them at zero; `+3A` is set while `+1C` is positive and
   `+3E`, `+42` and `+4` are cleared. While a message is shown it brakes
   instead: `+3E = 1`, turn, pitch, Fire and Grenade zero, and `+1C` is -100
   times the unit's velocity along its basis row `+60..+68` when its view's
   mode `+1E8` is 1 or 2 (buggy or truck), else zero; `+3A`, `+42`, the
   analog block and `+F4` keep their values;
3. calls the actor's `+44(delta, player+4)` when its flag 2 is set and its
   lifecycle is 3 or above (`+24(delta)` below 3);
4. while command 3 is owned (`0046BDC0(3)`), adds the delta to `+F8`; past
   8 s it clears `+F8` and raises `+12C` (a count above 3 drops to 3);
5. updates the message with `00413870(delta, 0)`, which does nothing while
   its `+04/+08/+0C` children are NULL.

Infest01's 707 Level-1 gameplay ticks all reach the vehicle through `+44`
(return `0046A7C1`), never through `+24`. In the intro the player's vehicle has
left both lists (flag 2 clear), so the block runs (538 and 537 executions)
without a vehicle call; the first `00416170` execution is in Level 1.

The port runs this block ([player controls](../../crates/inf-game/src/object_runtime/player/controls.rs),
[world tick](../../crates/inf-game/src/session_platform/intro/runtime.rs)).
An unported branch stops it for that world with a logged reason, keeping the
writes already made: a shown message (Fire then selects through `0046D2E0`),
a nonzero settings `+8` input mode (joystick or analog) and the network
branch. Once stopped, the vehicle returns to the loop, whose `+24`
`00416090` runs the controlled update on a braked block, as for
[a parked vehicle](VEHICLE_DRIVE_RUNTIME.md#a-parked-vehicle-00416090-alive),
or [the wreck's update](VEHICLE_DRIVE_RUNTIME.md#the-vehicles-death-00420b90-and-the-wreck-00416090)
once the vehicle is dead. After the vehicle's death the actor is the camera,
whose `+44` is [the death camera](VEHICLE_DRIVE_RUNTIME.md#the-death-camera-0040f890-and-the-respawn).
The vehicle's `+44` runs natively: the [vehicle update](VEHICLE_DRIVE_RUNTIME.md)
`00416170` with its mode-1/2 ground drive `004195F0`, verified against
Infest01's Level-1 updates. A linked test holds Space in Level 1: Thrust's
full 125 000 µs reaches `+1C`, the drive consumes the `+3A` jump request
(turbo is not owned yet) and the buggy moves.

## Player key callbacks `00469450..004698E0`

The sub-object's rows call these with the player in ECX, the key state in EDX
(nonzero while down) and two stack words, the input clock in microseconds for
the rows that are also consumers (zero for the others). Most store a request
in the control block for the [vehicle update](VEHICLE_DRIVE_RUNTIME.md#the-vehicle-update-00416170)
to consume, on a press only; a release does nothing. Offsets are the
player's. The keys are the live records' defaults:

| Key | Callback | Effect |
| --- | --- | --- |
| `1`..`5` | `004695A0`..`00469620` | `+34` = mode 1..5: buggy, truck, hovercraft, VTOL, skimmer |
| Caps Lock | `00469580` | `+34 = -1`: the next owned mode |
| Home / End | `004696C0` / `004696E0` | `+28 = ±1`: cycle the primary weapon |
| F1..F7 | `00469800`..`004698C0` | `+28` = 2..8: pick a primary weapon |
| PgUp / PgDn | `00469700` / `00469720` | `+2C = ±1`: step the secondary weapon |
| C / D | `00469660` / `00469670` | `+3B` / `+3C = 1`: pick up / drop |
| A | `00469680` | `+30 = 1`: turn the hold ([`00411E70`](VEHICLE_DRIVE_RUNTIME.md#the-hold-00412380)) |
| L / V | `00469740` / `00469750` | `+3D` / `+3F = 1` |
| Left Ctrl | `004694B0` | `+38` = 2 while held, 0 on release |
| Right Ctrl / Numpad 0 | `00469760` / `00469770` | `+40` / `+41` = 1 while held, 0 on release |
| X (Grenade) | `004694D0` | a double tap sets `+39` |
| Up, Down, Left, Right, Enter | `00469780`, `004697C0`..`004697F0` | message keys: `+50` bits 2, 4, 8, 10h, 1 |
| Tab | `00469450` | `00413410` on the message at `+88`, then `+F4` = 0.5 s |
| Escape | `00469480` | event 18h to actors within 300h of the controlled unit |

`00469640` (`+34 = 6`), `004696A0` (`+30 = -1`) and `004698E0` (`+42 = 1`)
have no default keyboard row. Escape (record `005348E0`, Alt excluded) is a
fixed row of the sub-object's table, not one of the 44 configurable rows.
Tab and Escape do nothing while the network flag `0055F448` is set.
`0046AE60` gives Escape's unit: `+D8`'s actor, else `+D4`'s; with neither,
nothing is sent.

`004694D0` takes the clock through `__allshr` by 10 (1024 µs ticks) and keeps
its last time in `+48` and a state in `+4C`, which the constructor sets to
-1000 and 0. A press more than `0053A278` (350) ticks after the remembered
time arms the tap (state 1, time stored); a press within it waits (state 2)
for a release less than 350 ticks after the remembered press, which sets
`+39`. Every other step forgets the time (`-1000000` from now) and returns to
state 0.

A message key press runs `0046D2E0`: the bit joins `+50`, and if it was not
already held, `0046D240` runs at once and `+54` restarts at 250 ms; a release
clears the bit. `0046D240` calls `00413950` on the message at `+88` for each
held bit: bit 1 the select (code 4, then the bit clears), bits 2, 4, 8, 10h
the cursor codes 0..3. The cursor codes do nothing unless a message is shown.
The select leaves UI state 6 and, unless the UI state is 5, dismisses a live
hint box by setting its display time to 1; only without a hint does a shown
menu take it. Any reached call under a shown message sets the 0.5 s Fire hold
`+F4`.
While a message is shown, `0046A260` turns Fire into the select: Fire held
this tick presses bit 1 (`0046D2E0`) once, latched by `+FC`; released, `+FC`
and bit 1 clear. This comes before the `+54` timer, which therefore counts
the same tick's delta off a fresh press's 250 ms.

The [port](../../crates/inf-game/src/object_runtime/player/keys.rs) runs
every callback; the gameplay router delivers them and logs omitted calls
once per world. Tab's `00413410` shows or hides the computer
([the computer and the in-world menu](PLAYER_MESSAGES.md#the-computer-and-the-in-world-menu)),
Escape on a shown message hides it (`00443070`'s `004133C0`) and Escape
otherwise opens the in-world menu (phase 5). Retail calls the screen's
`00443070` before the player's `00469480` on every Escape. Escape's and the
computer's broadcasts from the unit run right after their callback, before
the next input event
([the factory's Tab and Escape](FAMILY40D800_RUNTIME.md#tab-escape-and-the-pad)).
A shown menu's commands and cues run as the
[message notes](PLAYER_MESSAGES.md#controllers-update-and-display) describe.
The select itself runs on the [message boxes](PLAYER_MESSAGES.md#controllers-update-and-display)
once the session has set the message's process values. `0046D240` itself runs, on a key press and on
the player update's repeat, so the cursor keys can be held while driving.
[The key receipt test](../../crates/inf-game/src/object_runtime/player/keys_tests.rs)
replays every key callback of both recordings (752: the mode keys 1 and 2,
pickup and drop, Right Ctrl, X, the cursor keys and Enter, Tab and Escape)
and the 740 repeats with a key held, each from its captured entry state
([player_keys_native.json](../../crates/inf-game/tests/fixtures/player_keys_native.json)).
All match, including the cursor keys and Enter on shown tutorial messages,
except Infest01's one Tab press, whose omitted `00413410` showed the message
display and set the Fire hold. In Infest01, `2` switched to the truck at tick
265 because Level 1 had granted upgrade 2 by then (`+100` went from `12h` at
the first tick to `96h`); before that grant the request falls back to mode 1.

## High frame rates

Retail's gameplay tick `004388D0` hands the world each frame's delta in
microseconds (whole milliseconds from `timeGetTime`), and most of the world
steps by shifting it: gravity `0047AD10` and lifetimes by `delta >> 10`,
positions and turns by `>> 5` and `>> 4`, and the selector-8 particles of
`00466980` by `>> 12` (outside the world, the loading and name-entry decays
shift by `>> 13`). Each shift drops the remainder every tick, so the loss
grows with the frame rate: at 144 Hz (6 and 7 ms frames) gravity loses about
13%, a `>> 12` step 37% and a `>> 13` step everything; at 60 Hz each loses
about 2%. Steps
with a per-tick constant, such as the drive's one unit of drag and the
VTOL's climb, also change with the number of ticks
([the other forms](VEHICLE_DRIVE_RUNTIME.md#the-other-forms)).

The port's `fixes.exact_world_ticks` (on by default; `--exact-world-ticks
false` restores retail) runs the world only in whole multiples of 16384 µs
(2^14), carrying the rest of each frame's delta to the next
([`WorldTickCarry`](../../crates/inf-game/src/session_platform.rs)); a
frame that reaches no whole quantum skips the world's update, and neither
draws nor presents: the window keeps the last tick's frame
(`PortSession::holds_gameplay_frame`). Retail drew once a tick, so the
draw's own per-frame steps run once a tick too: the world flash's eight
frames (`0x53C684`), the view focal's quarter-step easing in `00442080`,
and the random draws of the draw's sound service and models. Before the
hold, a 240 Hz display drew a frame every other vertical blank (the
scheduler's 8 ms gate, `MIN_TICK_MICROS`), about two a tick, and a
warpgate's white flash lasted half of retail's 133 ms at 60 fps. Every shift up to 14 bits
is then exact, and the world ticks at most about 61 times a second, as
retail did at 60 fps. The mission time
`G+1E8`, which adds each tick's whole milliseconds, carries its own
remainder, and the session's elapsed seconds keep the frames' own deltas.
The few world steps that take whole milliseconds (`delta / 1000`: the weapon
panels' slide and the player's 250 ms key repeat) truncate each quantum's
16.384 to 16, so they run 2.3% slow; a 30 fps frame's 33 ms loses nothing
there in retail.

Some classes move differently at 60 fps than at 30 even in retail, and
keeping retail's 60 fps does not keep their designed behaviour: the
scientists stand ([the gait](ACTOR_MOVEMENT_TAILS.md#the-scientists-gait-and-the-frame-rate)),
the alien ships fly slower ([selector 35](SELECTOR35_RUNTIME.md#frame-rate)),
and the creatures walk slower and, in contact, call twice as often
([calls](FAMILY4236D0_RUNTIME.md#calls-and-the-frame-rate)).
The port's `fixes.paced_actors` (on by default; `--paced-actors false`
restores retail) updates those classes (selectors 12 and 35, and the
creatures' 10, 21 and 36) every other quantum with both quanta's time, as
retail did at 30 fps.

The quantum also has to carry the keys. The vehicle drives by how long each
key was held during its tick: `0046A260` reads the input's durations
(`004F0040`), capped at the delta, into the control block, and the poll
`004F0200` resets them every frame. In retail both span the same frame. The
port polls every frame but runs the world only on frames that complete a
quantum, so at 120 frames a second a tick saw only the last frame's 8.3 ms
of Thrust against its 16.384 ms delta. Every held control ran at half
strength, and the VTOL, whose lift is Thrust over 600 a tick against
gravity's `delta >> 10`, could not climb past its ground cushion: holding
Thrust at 125 frames a second it peaked near 940 units and sank back.
`InputRuntime::carry_durations` keeps a skipped frame's durations for the
next poll, and `scale_durations` maps the real time they span onto the
tick's delta (`WorldTickCarry::take_input_micros`), so a key held
throughout reads the whole delta at any frame rate, as in retail. With
both, the same probe climbs to 2,740, where the lift fades.
[The input test](../../crates/inf-game/src/input_tests.rs)
(`skipped_ticks_carry_held_time_and_ticks_scale_it_to_their_delta`) and
the carry test above check the accounting. Retail's own ticks read held
keys whole: in Infest03's 107 VTOL drive calls
([vehicle_vtol_infest03.json](../../crates/inf-game/tests/fixtures/vehicle_vtol_infest03.json),
125 ms ticks) Thrust reads 125000 in 39 and pitch ±125000 in 61; the
others read 0 or a press within the tick (14000 to 54000). Held, Thrust
lifts the VTOL about 100 units a tick. The port's VTOL drive matches all
107 calls (`vtol_drive_matches_every_infest03_tick`).

Outside the world, two screens step per tick:

- The name entry (`0045A8F0`) eases its row animation by 7/8 a tick and
  decays each glyph effect by `effect * (delta >> 13) / 32`, after a 1-in-32
  flash draw per glyph per tick. Below 8.192 ms the limit is 0: a flashed
  glyph never fades, and the flashes come more often with the frame rate.
- The language carousel's tick (`00433640`) eases its angle by 7/8 a tick,
  so it settles faster.

`fixes.exact_frontend_ticks` (on by default, `--exact-frontend-ticks
false` for retail) runs those steps once a whole 16384 µs quantum, carrying
the rest, while input and the delta-based timers still run every tick. Tests
replay both at high rates: at 4 ms ticks retail's glyphs never fade and the
quantized ones do (`quantized_name_entry_effects_fade_at_high_frame_rates`),
and retail's carousel settles in under a third of its 16 ms time while the
quantized one settles within a quantum of it at both rates
(`quantized_carousel_easing_keeps_its_pace_at_high_frame_rates`). Infest01's
93 name entry ticks bear out the decay itself
(`name-entry-effects.js`):
of the 1,488 glyph bytes across them, 1,453 followed `effect - effect *
(delta >> 13) / 32` (a flash's 0x7F to 68, then 37, at the 125 ms cap's
limit of 15) and 35 were fresh flashes, none anything else; with a limit
of 0, as below 8.192 ms, the same step leaves every byte where it is. The
loading screen's `>> 13` effect decay (`0040E6B0`) ticks once per resource
read callback, not per displayed frame, so the display rate does not reach
it. The other frontend owners keep their rate at any frame rate:

| Owner | Its steps |
| --- | --- |
| Menus (tick `00452470`) | ages in `delta / 1000` ms, the basis turns by `delta / 100` |
| Frontend control and camera | `delta / 1000` ms timers; the camera eases by `delta << 11` (Q31 of the step) |
| Profile menu, screen input, results | `delta / 1000` ms timers and ages |
| Panels (`004554E0`) | countdown in microseconds; the scale eases by `difference * (delta >> 2) >> 15` |
| Briefing (`0040C6A0` and its tick) | `delta * 7 / 8` timers, `delta / 1000` ages; the map zoom retreats by `delta >> 5` |
| Title, key configuration, text entry | the animation clock in microseconds |

Retail's realtime clock gives whole milliseconds, so the `/ 1000` steps are
exact; a `>> 2` or `>> 5` loses under 1% at 6 ms frames.

Both recordings bear that out
(`frame-deltas.js`,
ledger): every delta the
gameplay tick (1,245 calls in Infest01, 618 in Infest03), gravity
`0047AD10` (6,384 and 3,465) and the name entry's tick (93 in Infest01)
received was a whole number of milliseconds, and gravity received the
gameplay tick's own delta, with no sub-stepping. Under TTD the game runs
near 2 fps, so nearly every delta sits at the scheduler's 125,000 µs cap
(a few are 36 and 38 ms); the recordings cannot show a high frame rate, and
the truncation losses above follow from the code at a given delta. That code
is visible in the recording too
(`gravity-steps.js`): in the
first 200 gravity calls of Infest01 the velocity word `+90` fell by exactly
`delta >> 10` every time, 122 of an exact 122.07 per 125 ms call (14 of
24,414 lost over the 200). The same instruction keeps 6 of 6.84 at 7 ms;
the port's 16384 µs quantum shifts with no remainder.

[`exact_world_ticks_keep_gravity_at_every_frame_rate`](../../crates/inf-game/src/object_runtime/actor/derived/physics_tests.rs)
runs the port's gravity (`tick_actor_physics` mask 1) through the tick's
carry over a second of whole-millisecond frames: retail's per-frame step
pulls 992, 945, 858, 750 and 0 at 33, 16, 7, 4 and 1 ms frames, the exact
ticks the elapsed time's `elapsed / 1024` less at most one carried quantum
at each. A whole Level 1 drive replayed at 25 ms and 8 ms frames on the same
game-time schedule diverges with or without the fix (contacts amplify the
first different tick), so the per-quantity test, not a trajectory, is the
check.
## Live gameplay sample

A bounded read-only replay from the first `0048DC30` return (`41743C:13CB`)
samples actual control dispatch. Bundle
`.tmp/infestation-ttd/20260927-214642-801-GameplayInput/` retains
`command-line.txt`, `gameplay.windbg.cmd` and `transcript.txt`; driver
`runtime_re/scripts/gameplay-input-probe.ps1`. Hooks on the ten screen-slot-0
keyboard callbacks plus every phase request, 24-hit budget, thread
unfiltered — the run reached end of trace with 8 hits:

| Ordinal | Observation |
|---|---|
| 0–1 | Request 8 from gameplay, then the same request dropped while pending (caller `0048759C` both): gameplay to results |
| 2 | Request 3 from results, caller `0042CFF5`: accepted |
| 3 | Request 4 from the second briefing, caller `0040D542` (inside `0040D510`): accepted |
| 4 | The sole hooked keypress: `00443700` with value 1 on object `03854980`, thread `C23C` |
| 5 | Request 5 from gameplay, caller `004430DC`: accepted (in-world menu entry) |
| 6 | Request 2 from the menu, caller `00442FE5`: accepted (back to frontend) |
| 7 | Request 4 from `0044344A` while the 5→2 transition is pending: dropped |

The `00443700` press immediately precedes the phase-5 request, and the
static vtable mapping now closes the chain (disassembly, no execution
claimed beyond the live hits): gameplay vtable `00523370` slot `+0x64`
is `00443070`, and `00443700` calls virtual `+0x64` on the routed
object. `00443070` falls through to `00438380(5)` at `004430D5`, whose
return address `004430DC` matches the observed caller. So ordinal 4→5
is press → `+0x64` → `00443070` → menu request. The `0044344A`
dropped request shows first-request-wins protecting a menu teardown:
`0044344A` is the return inside `00443400`, which requests screen
`+0x5C` (folding 6→4) through `004391B0` (returns session
`DAT_00546F2C`), then runs menu `00452420`. Its
requested value stays open. Only one hooked callback fired in the whole
span: input was sparse here, and joystick/mouse paths were not hooked,
so no input-absence claim follows. The `004430DC`/`00442FE5` owners below
close the menu-entry/exit side; per-key semantics beyond the Escape-class
`00443070` shape stay open.

A follow-up bounded replay from just before the keypress (`F17204:0`,
bundle `.tmp/infestation-ttd/20260927-215854-078-GameplayMenuDecision/`,
driver `runtime_re/scripts/gameplay-menu-decision-probe.ps1`, 5 of 10 budgeted hits)
samples the decision state live: `00443070` enters at `F17204:5F0` on
screen `03854980` — the same object the `00443700` press targeted —
with attract `00`, and the accepted phase-5 request follows 36 steps
later at `F17204:60B`. Attract-clear entry plus the menu request verifies
the fall-through: no attract diversion, no `+0x250 == 0x16` diversion,
no player-modal diversion. `00442FD0` then enters on the distinct
phase-5 menu object `03857EF0` with argument 1, followed by the accepted
phase-2 request — the shared nonzero-arg return path. The `+0x250` and
player-modal values are established by outcome (either diversion would
have suppressed the phase-5 request), not by direct reads.

A second bounded replay from the same keypress (`F17204:0` to end of
trace, bundle
`.tmp/infestation-ttd/20260927-220453-869-GameplayDeselect/`, driver
`runtime_re/scripts/gameplay-deselect-probe.ps1`, 4 of 12 budgeted hits) hooks
deselect `00440BA0` entry, `00487230` entry and every phase request:
deselect never fires across the 4→5→2 transitions, so it is not on that
transition path — the gameplay screen is deleted (reuse refuses phase 5),
not deselected, there. `00487230` fires once at `F204FA:2406`, after the
5→2 transition region, on thread `C23C`. A targeted caller-ID replay
from `F204C2:0` (bundle
`.tmp/infestation-ttd/20260927-220712-764-Prepare5Caller/`, driver
`runtime_re/scripts/prepare5-caller-probe.ps1`, 1 of 2 budgeted hits) identifies
the caller as `004839A3`: byte-proof `E8 8D380000` (`call 00487230`)
returning into the frontend factory `004838A0`, immediately followed by
`mov ecx,47` + `call 004EE1E0` (leaf-47 load) and `call 00487370`. So
this hit is the 5→2 transition's frontend construction running its
documented preparation order — not deselect, not gameplay. Static
candidates remain the phase-0 constructor `00432500` and the world
loader `0048DC30` for other lifecycle points.

## Every world from the start harness

On 2026-10-14 the start harness (`--start` with `{"world": N, "form": 1}`,
`--hidden --renderer opengl --fixed-step-micros 16000 --frames 18750`)
loaded worlds 0 to 24 one after another, each in a fresh process, with the
buggy parked at the first start, and let each run five minutes of game
time. World N's name is H0 `0xE0 + N`; world 0 loads world 19's map, and
world 1's name slot holds "Completed". The multiplayer arenas (25 to 33)
load only in a network game.
[`world_sweep.py`](../../scripts/world_sweep.py) runs it again and, with
`--baseline`, lists each world's stops that are new or gone since an
earlier sweep.

The first sweep ended one world: Stendahl Alpha, at frame 964, when
explosion debris reached packet depth 0
([depth zero](PIXEL_RUNTIME.md#depth-zero)). With that fixed, all 25
reach the end. Two end their mission instead of sitting: Stendahl Alpha's
and Altair Prime's starts are under fire, the parked buggy is destroyed
three times (two respawns through Phase 6), and at the world's
[loss limit](VEHICLE_DRIVE_RUNTIME.md#the-death-camera-0040f890-and-the-respawn)
the mission ends in the results page (Phase 8) after 35 and 46 s of
mission time: the first live port runs to reach a loss limit. World 22,
the intro's world, plays its intro (frame 4294), passes the results page
and stops at Level 1's briefing (Phase 3), as a new game does. What each
world still stops at is contained: the actor or feature stops and the
world goes on.

The sweep ran again on 2026-10-15, on the build with the parked vehicle's
own update and the family fire gate's split: all 25 worlds reach the end
again, with the same deaths and the same contained stops line for line.
Only the audio statistics at the end differ, as they follow wall-clock
time. A save-based start agrees: with the retail save as the profiles
file, the menu's seventh slot (Terra Nova, start 0) loads Terra Nova's
briefing and the fourth (Helniazine) Helniazine's world.

Later that day the affected worlds ran again (1, 4, 11 to 13, 15 to 17,
20, 21 and 23), after four ports. The walker tick `004476D0` replaced the
mode 4 stop. The spawner's damage gate `00480C80` ended the `+B4` stops,
and the gate's `0048B411` and the water bridge (`00409A20`'s `+166` arms)
ran in place of their boundaries. The table below is that state. The
`+B4` stops in worlds 15, 17, 20 and 21 were all spawners' (selector 60):
the earlier table read the stopped slots as placement indices, while a
slot is its placement index less the placements skipped before it. With
them gone, world 17's start comes under fire. The buggy is destroyed and
respawns twice through Phase 6, and the mission's doom countdown then ends
the world into the results page (frame 5080). There, as in worlds 3 and 5,
a sweep without a profile fails the results page's Restore. World 17 also
omits one draw, model 614's primitive opcode 0x07, which needs the
near-camera pointer-scale edge. Since the
[altitude ceiling](CONSTRUCTOR_ASSETS.md#the-altitude-ceiling-selector-48)
(selector 48) is built, the Defence Satellite's start is under fire too. A
start without grant `0x30` keeps the ceiling, and the parked buggy above it
is shot from the first update; after two respawns the loss limit ends the
world into the results page (frame 15994). Worlds 8, 17 and 24 then ran a
minute each with the
[timed spawner](CONSTRUCTOR_ASSETS.md#the-timed-spawner-selector-43-and-its-rocks-selector-44)
(selector 43) built. Their spawners threw 120, 203 and 97 rocks, and every
rock that landed (75, 203 and 97) died with its two visuals; world 8's
others were still falling, as its rocks start 0x3000 up. The first
eruption sets the alarm of world 8's and world 17's Sirens before their
first update, which then
[depends on heap residue](FAMILY40D800_RUNTIME.md#selector-47-the-siren),
so each stops there. With the [Bomb](HOLD_ITEMS.md#bomb-selector-29)
(selector 29) built, Altair Prime's three and Helniazine's two load too, and
both worlds ran their minute as before, the Bombs lying disarmed. With
[Helniazine's zone](CONSTRUCTOR_ASSETS.md#helniazines-zone-selector-74)
(selector 74) built, world 6 ran its 2400 frames as before with the zone
updating; its draw holds at the timer nothing writes, logged once. With
its [furniture marker](CONSTRUCTOR_ASSETS.md#helniazines-furniture-marker-selector-75)
(selector 75) built too, world 6 places only classes the port builds.

On 2026-10-17 every world ran its five game minutes again (18,750 frames at
16 ms) after the TL717's walker carries, the first-person hit flash, the draw
arena's `0x301` catch and the held frames, and every world exited normally.
The walkers of worlds 1 and 4 walk now. Terra Nova's dying actors leave
debris whose selector 66 helpers reached the furniture pass, which had no
`+A8` response for them and stopped there; their table holds the shared
`0047BFD0` ([effect attachment](ACTOR_EFFECT_ATTACHMENT.md)), and with it
world 2 ran its five minutes without a stop. World 8 reached one more stop:
placement slot 23's pair contacts with the timed spawner's rocks (selector
44) end at the auxiliary callback `00544C68+08`, as world 6's mining rigs'
do. Worlds 3, 5 and 16 end in the results page as before.

On 2026-10-18 every world ran its five minutes again, after the furniture
roots' [shared register block](MODEL_RUNTIME.md#terrain-furnitures-shared-root-block),
the grenade sparks' blasts and the paced scientists and alien ships, and
every world exited normally with no new stop. Grach'to's start is under
fire as before: its parked buggy is destroyed five times (three in the
sweep before), and the loss limit now ends the world into the results
page, whose Restore fails without a profile, as world 17's does.

A sweep later that day (`goal1018`, against `thumper1018`) ran every world
its five minutes with no new stop. Gone: worlds 0 and 19's spawner stop and
their two skipped sinking patches, and the Siren's stops in worlds 8 and 17.
Its build predates the ribbon start (world 1) and the selector 30 ground
vertex (worlds 23 and 24).

| World | Placements skipped at load | Contained stops |
| --- | --- | --- |
| 0 (19's map), 19 Alien Brain | the spawner's eyes (80); the brain (85), then the director (78) and the eyes' death are built (2026-10-18, five minutes each with no new stop), and so are the two [sinking patches](CONSTRUCTOR_ASSETS.md#the-sinking-patch-selector-83) (selector 83, 2026-10-18) | none: the spawner's first attempt starts its unwritten Y (`004095B7`) at 0 (2026-10-18) |
| 1 | none | two perships (selector 34) whose H8 model 665 is null stop their terrain, furniture and pair contacts, and the vehicle's aim (the sight test `0043AD90`, whose actor pass reaches `0047D080`) passes each, logged once (until 2026-10-23 that stop ended the player's vehicle update: a crash on the prototype map). Only levels 25 and 28 register H8 slot 665; the H8 pool's loader (`005276D8 +0`, `004EFBF0`) returns NULL, and `004F2C40` reads the model's byte `+7` without a test, so retail would fault there unless one of those levels' banks is resident. The lab cannot start world 1 from a save (it stops in name entry), so retail's residency there is unchecked (the Nemian walks: its walker's unwritten stride phase `+34` is a [fresh-heap carry](FAMILY4236D0_RUNTIME.md#mode-4-the-walker-tick-004476d0)) |
| 2 Terra Nova | none | none |
| 3 Stendahl Alpha | none | none (the depth-zero crash and the assault craft's Follow Player stop are fixed) |
| 4 Exxodus | none | none (the TL717, selector 21's walker, walks, fights and dies into its wreck: `exxodus_tl717_walks_and_dies`) |
| 5 Altair Prime | none | none |
| 6 Helniazine | none | none (its mining rigs' pairs, which stopped at the auxiliary callback `00544C68+08`, run the box wrapper `004F2690` since 2026-10-18) |
| 7 Sanrel-Dek | none | none |
| 8 Luna Azine | none | none: the Siren's silence byte `+16D` starts at the fresh heap's 0, as retail's reads in the lab, and it sounds the alarm the spawner's first eruption set (2026-10-18; its rocks' pairs run the box wrapper `004F2690` since 2026-10-18) |
| 9 Tabuchi | none | none |
| 10 Pillier Stren | none | none (Troy's transport runs its [rescue](SELECTOR35_RUNTIME.md#rescue-and-go-to-space-descriptors-10-and-11)) |
| 11 Artoria | none | none (its ten Pursuit Ships, selector 61, are [built](CONSTRUCTOR_ASSETS.md#the-pursuit-ship-selector-61)) |
| 12 Shalapreth | none | none |
| 13 Grach'to, 14 Foldrak Noir | none | none (Grach'to's start is under fire and its loss limit ends the world into the results page; Foldrak Noir's three attacks, selector 62, are [built](CONSTRUCTOR_ASSETS.md#the-alien-brains-attack-selector-62), and stay until the Defence Satellite is captured, grant 0x35) |
| 15 Kankutan | none | none |
| 16 Defence Satellite | none | none (its seven Pursuit Ships are built) |
| 17 Shak'tahh | none | none: the Siren's `+16D` starts at 0, as world 8's (2026-10-18) |
| 18 Command Centre (Level 1) | none | none |
| 20 Oolbryte, 21 Vernimab | none | none |
| 22 Homeworld (the intro's world), 23 Caspadril | none | none (Caspadril's selector 30 actor drew without its model: its +10 `0048B290` takes every selector but 8 to the ground under it, ported 2026-10-18, not yet swept) |
| 24 Caradion | none | none (the same selector 30 ground vertex, 2026-10-18) |

Kankutan's laser mines had been missing from the
[constructor census](CONSTRUCTOR_ASSETS.md#placed-selectors-the-port-does-not-construct).
The port built selector 38 only as the ACAM's mine, through the child
factory; its placements now load the same way, and the world runs its five
minutes with them, without a stop.

## Draw orchestration map

Screen draw `00442080` (thiscall, draw param) phases, recovered from the
decompile and the callee index:

1. Arena preamble `00439270` (a fresh `0xC8000`-byte command arena, which
   retail's game keeps on any frame; the view shown and the remaster's game
   size it by the view's reach, see
   [widescreen](PRESENTATION_POLICY.md#widescreen)), then the session `+648`
   context.
2. View setup: focal globals `00547660/64` initialize from `00546F20/24`
   when zero; with gameplay `+5C5` clear they track the focals with a
   `-0x50` decay, otherwise they ease toward a `+0x5C0`-scaled target by
   quarters. Renderer dimensions publish into the context (`+38/+3C` focals,
   `+40/+44` size, `+48/+4C` halves).
3. World worker `00440F00` with a stack struct (→ `00488020` below).
4. Online only, the network game's overlay: the game's end, the bars and
   the scoreboards `00441510`
   ([NETWORK_RUNTIME.md](NETWORK_RUNTIME.md#the-network-game-on-screen)),
   text through `004A3AB0`/`004A4240`.

World draw `00488020` (called only by `00440F00`) fans out to: renderer
lookups, player lookup `0046AE60` and player owners `00464C80`/`00464DA0`,
manager passes `00479500`/`00479570` and `004800E0`/`00480110`, entity-list
`00488AA0`, the anchor/message trio `0045B160`/`0045B180`/`0045B1A0`
(`0045B180` writes the `0054E2F0`/`00548400` anchor pair as args minus
`0x1680`; the [grid owner](TERRAIN_RUNTIME.md#shared-production-pixels)
owns settings-`+0x80`-gated `0045B1A0`; the [object-light owner](OBJECT_LIGHTING.md)
owns the admitted `00479500` callbacks;
`0045B160` runs `004959E0(0x5EEC)`), `0044AA10`, `004501A0`, `004A42B0`,
plus the standard clock/error brackets. Entity submission order, HUD
composition and viewport policy stay open; no per-entity draw semantics is
established here.

### Observed first-frame submission (retail, mission-22 intro cutscene)

Bounded read-only census from the accepted first `3->4` pending transition
`41743C:13CB` — the cutscene entry on mission 22's map, not the mission-18
level-1 frame (bundle
`.tmp/infestation-ttd/20260928-060800-917-FirstFrameDraw/`, driver
`runtime_re/scripts/first-frame-probe.ps1`, thread `C23C`; full command line and
per-event positions in the capture ledger):

- Screen draw `00442080` enters at `417AF9:1951` (screen `03838C00`,
  draw param `03832220`, focals still zero); the worker `00440F00`
  enters at `417AFE:2B3` with focals `0x200/0x200` already published
  from `00546F20/24`, and they stay `0x200` into frame 2. The render
  context threaded through the frame is `00546E88`.
- Pre-draw `00488380` (owner `047250CC`) runs `004501A0`, `0048E7D0`,
  camera query `00478E00` and retarget `00410D90` on camera `03829250`
  (the selector-16 spawn camera candidate; vtable/follow-target
  confirmation open), then `00440DA0` draws the sky H8 slot `0x116`
  once through H1. Its packet takes the context eye's X/Z
  (`00440E03/0A`) and Y from DWORD `00547668` (`00440DF5`). Screen
  constructor `004408B0` stores that DWORD at `00440910` from H14/0
  header DWORD +10 (`0048EDA0`): `FFFFF1A0` (-3680) for the intro and
  `FFFFEC5A` (-5030) for Level 1 in Infest01. The basis is identity.
- World draw `00488020` runs, in order: depth setter `004BC9B0(1,1)`
  (depth enable/write on), `0045B160`, `0045B180`, `00464C80` pass 0,
  `00479500`, `00479570`, `00464C80` pass 1, `00464DA0`, `004501A0`,
  `00480110` (which calls `004800E0` internally), direct `004800E0`,
  terrain `0044AA10` (renderer `0472D9F8`, context, arg 6).
- `004501A0` publishes the world light. `00490030` first sets the
  directional colour to tint `005474E0` along the H12 light direction
  `00548380`. The previous ambient is still current, so the effect colour
  `0055F268` becomes that ambient plus half the tint. `00490130` then sets
  the ambient to each tint byte shifted right by three. The terrain surface
  callbacks follow. Pre-draw runs it first; `004881B3` runs it again after
  the particle transfer. Furniture sets only an ambient, so it draws under
  this directional light and effect colour, not the last object's. The port
  once installed only the callbacks, and a seeded Infest01 comparison
  showed furniture models 163 and 49 keeping a pickup's light.
- Model (H1 `0049FEA0` -> H2 `004A00E0`) draws in frame 1: sky `0x116`
  x1; entity slots `0x228` x8, `0xDF` x2, `0x1DA` x2 through entity
  draw `0047AFC0` (per-entity H1 caller; it culls on model WORD
  `+0x42 == -1`, samples the `00548406` terrain-light grid, then H1);
  terrain-detail slots `0x233`, `0x235`, `0x4F` x3, `0xA3` through
  `0044CD00` -> `0044CEF0` -> `004359A0`. `004ED610` fires once per
  frame (inside `00479570`), not per entity. The deferred-enqueue
  `004A42B0` fires 400+ times per frame from a dozen `0049xxxx`
  H8-pipeline sites (own hook cap; exact per-frame total open).
- Slots are draw-time H8 table indices, not resolved model names;
  the table-state capture that would resolve them, the retail
  back-buffer pixels, and the worker-tail `00441510`/text draws stay
  open. Port owners that already cover the reused layers: the
  Handler-8 executor, primitive/deferred queues, the CPU raster/scene
  backend, and `TerrainMap` heights. The later shared production terrain
  bridge is tracked in [terrain runtime](TERRAIN_RUNTIME.md#shared-production-pixels);
  it does not extend this recorded census's coverage.

### Observed first-frame submission (retail, mission-18 level 1)

Same census shape re-anchored at the second `3->4` transition
`AF0B4B:330` (request `AF0A7B:8DF`), bundle
`.tmp/infestation-ttd/20260928-135746-345-Level1FrameDraw/`
(37.882s; tracked driver `runtime_re/scripts/
level1-frame-census.ps1`). Same pipeline order
(screen, worker, pre-draw, world draw, depth, anchors, managers,
entity passes, terrain), same depth `(1,1)`, same focals `0x200`,
same thread `C23C`, same pre-draw owner and render-context
family; the camera object differs as expected. Pre-draw is
noisier (81 player lookups plus 7 owner resolutions from
briefing teardown). Counts complete under caps (h1 41/60, h2
55/60).

- H1 slot set differs completely (19 distinct slots, no `0xDF`):
  `0x4F` x10 dominant, `0x4C` x4, `0x3B` x4, `0x33` x3, `0x18E`
  x3, `0x228` x2, `0x182` x2, `0x1DA` x1, singles `0x2F 0x31
  0x32 0x3E 0x4D 0x63 0x75 0xAF 0xB4 0x175`, sky `0x116` x1.
  H1 slots are global H8 slots (proven by convergence: every
  resolution below is a sensible model for its frame). Corpus
  name map, scene-OVL-wins for the six multi-provider slots:
  common bank (`1.ovl`) — `pipevert/ pipecorn/ pipetjun/
  pipestop` (`0x2F/31/32/33`), `token04` (`0x3B`), `gate`
  (`0x3E`), `bunkers` (`0x4C`), `arrow` (`0x4D`), `runway`
  (`0x4F`), `frame` (`0x63`), `acam` (`0x75`), `gunpower`
  (`0xAF`), `hudgnade` (`0xB4`); scene bank (`28.ovl`) —
  `backdrop` (`0x116`), `palm02` (`0x175`), `banners` (`0x182`),
  `alliedpost` (`0x18E`), `commship` (`0x1DA`), `imortalgun`
  (`0x228`).
- Per-caller path split from the H1 `ret` addresses (frame ordinals
  58..130): terrain-furniture `FUN_004359A0` (ret `00435B47`) —
  pipes, bunkers, arrow, runway x10, palm02, banners x2,
  alliedpost x3; entity draw `FUN_0047AFC0` (ret `0047B1D2`) —
  gate, acam, commship, imortalgun x2; `FUN_0046DAF0` (ret
  `0046E03B`) — token04 x4; `FUN_00455080` (ret `00455313`) —
  frame; `FUN_0041C320` (ret `0041E0DC/0041DD2A`) — gunpower,
  hudgnade; worker `FUN_00440DA0` (ret `00440E16`) — backdrop.
  So `0x4F` is terrain-furniture on both maps after all (not a
  per-map role change); only 4 slots go through the entity path
  with the H8 executor.
- Factory: model `factory1` (H8 global 157, common bank) is placed
  3x in world 28 (H13 global 12, constructor selector 15, authored
  XYZ `[47223,0,26207] [37085,0,34363] [20311,0,35992]`) and 0x in
  world 32. It is not drawn in frame 1 (`0x9D` absent from the
  census) — first hard drawn-vs-culled point: the spawn camera
  faces the runway/gun/post area, away from all three factories.
- Root opcode inventory (pinned by
  `level1_frame_slots_pin_their_remaining_opcode_owners` through
  port residency 47+48+28): fully owned roots — token04, gate
  (`0x22` in a child), arrow, acam (eight `0x0E` children open),
  backdrop, commship. Blocked roots — pipes/gunpower/banners/
  alliedpost on `0xE6`; bunkers on `0xD2/D6/DA`; runway (x10
  dominant) on `0x1A`; frame on `0xDA`; hudgnade on
  `0xD6/E6/EB`; palm02 on `0xD2/D6`; imortalgun on `0xDE/E6/EA`.
  Remaining root universe: `0x1A/D2/D6/DA/DE/EA/EB`
  (faces have no executor arm except `0xE2`; `0x1A` has none).
  These root counts describe the historical probe, not current coverage.
  The [E6 primitive owner](PRIMITIVE_RUNTIME.md#opcode-e6-shared-shade-quads-and-clipped-uv)
  supersedes the early null-material rejection and dropped-brightness theories:
  native null H3 still queries sorting and emits a packet, signed shade reads
  preserve unknown bytes, and Near/Far clipped outputs reach their shared-shade
  UV consumers. [DE/D6 clipping](PRIMITIVE_RUNTIME.md#de-and-d6-source-polygons)
  shares proven geometry while retaining each producer's distinct colors.
- Construction gap (owned fail-closed 2026-09-28, selector 6
  owned 2026-09-28, full load green throughout): `load_world`
  skips `UnsupportedSelector` placements per-feature (logged +
  recorded in `WorldLoadReport::skipped`); every other
  construction error still aborts. World 28's skipped selector
  set is now `{69}` (pinned; selectors 14, 15, 37, 39, 42, 50, 58
  and 64 owned 2026-10-08, and the
  [selector 53 pickup spawners](ACTOR_RUNTIME.md#selector-53-pickup-spawners)
  2026-10-09; selector 15 builds the factories at placements 0/1/3);
  mission-22 loads with zero skips (pinned). Selector 6
  (placements 166/213, the two commships) builds through the
  shared constructor + `0047AD70` wrapper with no `00426C10`
  extension: bare `initialize_actor`, no derived admission, and
  shared command handling for its `[2,15]` suffix (code-2 model
  override to 474 + word_76).
- Player/camera binding (owned 2026-09-28 from the `Level1BindProbe`
  bundle `.tmp/infestation-ttd/20260928-150219-412-Level1BindProbe/`,
  47.425 s, tracked driver `runtime_re/scripts/level1-bind-probe.ps1`): retail's player branch calls `0046AE90` with the null
  handle (detach; the `EDX=00528AB0` at entry is leftover, the arg
  travels on the stack — verified against the disassembly at
  `0046A90A..0046A915`), then `00478E00`, which constructs the
  camera when `camera_74` is empty and returns it with no attached
  requirement. The port's `world_camera` dropped its
  `CameraSearchBoundary` (removed variant) to match; the spawn tail
  wires the follow target from the vehicle as before, so
  mission-22 behavior is unchanged and level-1 loads end to end
  (camera follows the respawned vehicle, pinned). 0040FB10's
  search, which that camera starts with the player detached, is ported
  ([actor runtime](ACTOR_RUNTIME.md)).
- Load/draw observations from the same probe: the loader looks up
  H1 slot `0x75` (acam) 41x from `0040E970` (vehicle build); the
  briefing repeatedly draws the same quartet (two `0x228`
  imortalguns `0382CC70/0382CAB0`, two `0x1DA` commships
  `03834BC0/0383EEC0`, ret `0047D9F9`) — so frame 1's single
  `0x1DA` means one commship is culled there. Frame-1 camera is
  object `0383E910` (retargeted from `004884C6`).
- Entity-to-model linkage (pinned 2026-09-28 by
  `level1_frame_entities_bind_expected_models` through the real
  construction + code-2 path): imortalgun x7 (placements
  4/5/6/7/92/104/105), gate x2 (2/81), saber19 x2 (102/103),
  scientst x4 (32-35), hoverbot (188), tomspawn (191),
  commship x2 (166/213, selector 6, owned 2026-09-28), vehicle =
  acam (117, respawn craft). Code-2 operands read as the
  allocation-relative u16 at the command payload offset
  (validated against the port on the 552/665 bindings).
  Still open the same way: smbuild-392 via sel-37,
  checkpoint-689s via sel-64, grtargets via sel-39.
- Full H8 opcode→handler dispatch (read from the exe: near table
  file `0x140DC0`, far `0x1411C0`, 4-byte entries): `0x1A` →
  near `004A1D50` / far `004A2F00` (~0x4A0 + ~0x4C0 bytes,
  ownable); the `0x18-0x1B` family, `0x24/0x25/0x27/0x28/0x29`
  have no port executor arm; faces `0xD0-0xE6` (even) map to
  twelve near/far producer pairs (`0049xxx`), `0xE8-0xEF` and
  `0xF0-0xFF` to same-near-far handlers. Runway's program is nine
  `0x16`-gated `0x1A[slot,282,10,0]` plus two register ops (no
  faces) — owning `0x1A` unlocks the dominant slot, pending the
  furniture-caller (`0044CD00` chain) ownership. Furniture walk
  slice 1 OWNED 2026-09-29 (`terrain_furniture::walk_furniture_records`,
  5 focused tests): the `0044CD00` record-selection filter over
  decoded H12 buckets with native-shaped domain inputs (origins,
  F0/F4 scale/mask, coord words, row count) — the label-soup
  advance/consume loops collapse to one forward scan (each record
  fires iff in the wrapping x-range AND passes the odd-x flag
  test; equivalence proven clause-by-clause in the module docs).
  Buckets ride in `TerrainRuntime` untouched (loader fixup
  `byte_size >> 2` == decoded vecs). Pair provenance (bind
  oracle below): worlds 28/32 leave H14 `0x2000` clear (pinned
  `0xA0`/`0x20`), retaining the boot pair `(0x80, 0x7F)` — the
  fine walk reaches all 293 OVL-28 records exactly once (pinned
  `level1_furniture_walk_matches_direct_enumeration`, 34 lookup
  bytes banked raw). The earlier Units256-pair census (258
  reachable, odd buckets unreachable) was falsified by the
  oracle's odd-bucket binds and replaced; the walk itself takes
  the pair as input and never baked it. The retained
  [terrain policy](TERRAIN_RUNTIME.md#state-and-policy-custody) connects live
  settings to gameplay construction, preserves loader input policy, and owns
  briefing's later forced-coarse preparation. Gameplay Booleanizes nonzero;
  raw `00450370` restoration retains its distinct invalid-value arm. Spacing-pair oracle
  (2026-09-29, bundle `.tmp/infestation-ttd/20260929-155002-
  297-Level1SpacingPair/`, 5.126 s, tracked driver `runtime_re/
  scripts/level1-spacing-pair-oracle.ps1`): Detail word reads 1
  (validating the port's `0x54E918` constant) and F0/F4 read
  `(0x80, 0x7F)` at world-draw entry AND at every `0044CD00`
  entry — the pair is stable across the frame, no mid-frame
  reselect. EXE Detail default is 0, while the recorded setting is 1.
  Production reads the actual retained setting at the native gameplay
  constructor boundary. Matched comparisons must still establish the setting
  in both runs; this connection does not change saved settings. Driver hygiene: two breakpoints on one
  address shadow each other (only one body observably runs)
  and `bd` in a body never disables — merge shared-address
  hooks, end only the frame-closing branch without `gc`.
  Furniture setup `0044CEF0` now preserves all three signed
  light bytes, using grid `00548404` or the low three bytes of
  data DWORD `00526068`. Each unsigned tint byte from
  `005474E0` contributes `byte>>4` to its own ambient component;
  all three `00490130` arguments are computed, not leftovers.
  Signed components remain unclamped while packed ambient RGB
  clamps and retains its high byte. The current
  [furniture setup authority](TERRAIN_RUNTIME.md#furniture-item-setup-0044cef0)
  owns the original-instruction receipts and remaining model/effect
  boundaries; light/anchor/tint inputs retain their separate owners.
  The earlier setup oracle supplied `x=0x7D84 z=0x94C7` to
  `0045B180` and observed tint `0x007F9FCF`. Its census pins
  293/293 setup positions, including first-row `[32000,288,41216]`;
  these X/Z coordinates are anchor-independent. Those recorded
  inputs do not define current camera or light behavior. Stored
  height reads remain even-aligned under mask255, with the runtime
  diagonal bit irrelevant and height carry discarded by `&0x1FF`
  in anchor-relative indices.
  Furniture bind oracle (2026-09-29, bundle
  `.tmp/infestation-ttd/20260929-151828-019-Level1FurnitureBind/`,
  15.953 s, tracked driver `runtime_re/scripts/
  level1-furniture-bind-oracle.ps1`): 27 `004359A0` binds pair
  1:1 with 27 furniture-ret H1s in transcript order (zero
  orphans) — the empirical lookup→slot map: 50→`0x4F` x10,
  13→`0x4C` x4, 100→`0x33` x3, 51→`0x18E` x3, 95→`0x182` x2,
  101→`0x32`, 102→`0x2F`, 98→`0x31`, 26→`0x4D`, 15→`0x175`.
  Every bind lookup fits the census counts (95 exactly
  exhausted at 2/2). `0044CEF0` visits == binds (27/27: all
  visited records nonzero-lookup in frame 1). Setup contexts
  are per-item stack addresses (not bankable content).
  No pixels yet (consumer slices open): domain rows (terrain
  parent frustum), `004359A0` + H1 (needs live H8 slot tables
  + fade `0048FB00`); the static table-fill RE stalled (no
  `0055EEFC` writer in the decompiled sources), so the oracle
  pairs above are the acceptance map for slice 3.
- `0x1A` transcription recon (2026-09-28, static disassembly only,
  no code yet): near `004A1D50` (~0x490 B) is a fog-gated
  ground-quad producer in the 07-producer shape. Phases: entry
  vertex-slot math (`movsx +0x0`, `lea [edx+ecx*8]`, `0x40`-flag
  early-out) → two `004ED4B0` packed-value evaluations (matches
  the port's owned `evaluate_packed` by inspection: 4-bit
  rotate + optional NOT on bit `0x40`) → FPU scale off the shared
  `0055F22C/30` DWORDs with the `0x7FFF` saturate loop →
  `0x53DF00` angle-table sin/cos, `idiv`, `0x5A827999` fixed-mul
  → eight fog bytes (projected WORDs vs a bound and `0055F238`,
  quantized `2/4/0x10/0x20`) → gate byte at `[key+0x528BD0]`
  (zero skips the primitive before any enqueue) → brightness
  adjust (`0xFF` / `0x39` paths) → virtual `[edx+4]` submit +
  `004A42B0` deferred enqueue (packet kind `0x466150`) with an
  allocation-record fill. Far `004A2F00` (~0x4C0 B) shares the
  prologue shape through the `0x40` test; the diff is unread.
  `0x1B`-near (`004A21F0`) shares the template; `0x18/0x19`
  presumably too.
- `0x1A` oracle (2026-09-28, bundle
  `.tmp/infestation-ttd/20260928-152425-345-Level1Oracle1A/`,
  148.851 s, tracked driver `runtime_re/scripts/level1-1a-oracle.ps1`): frame 1 executes `0x1A`-near exactly 4x — all from
  `wheelmode1a/2a` (identical args, acam's `0x0E` children:
  `[92,585,2,0] [93,585,2,0] [92,282,16394,0]
  [93,282,16394,0]`), each followed by a `004A42B0` enqueue
  (ret `004A219F`, kind `0x466150`, zero prefix). Runway's nine
  static `0x1A` never execute in frame 1 (all `0x16`-gated off or
  furniture-culled): the dominant slot is silent, so `0x1A`
  ownership reaches the vehicle wheels, not the runway. Children
  resolve with no H1 event (direct table, like the port's
  `0x0E`). First probe iteration faulted on a null-EDX enqueue
  from the `0xDA` face path — enqueue hooks must filter by
  return address into the producer under study.
- E6-near shape + oracle (2026-09-28): `0049E1A0` (~440
  insns) mirrors owned `quad.rs` (mirror loop, `0048FA60` cull,
  `00528BD0` clip gate, material resolve, sort, `004A42B0`
  enqueue kind `0x40EF30` = EffectQuadNear). The descriptor
  `0xFF`/`0x39` branches select sort keys; they do not compute
  brightness. Packet `+0x28` copies the shared shade DWORD.
  Near's clipped fallback is `0049A270`; Far's is `0049A930`.
  E6-far adds `[0x2C]` far-color (context+`0x54`) and
  `[0x30..0x33]` fog bytes (slots+`0x15`), four bytes later
  than E2-far. The [E6 primitive owner](PRIMITIVE_RUNTIME.md#opcode-e6-shared-shade-quads-and-clipped-uv)
  preserves these distinct packets. Oracle (tracked driver `runtime_re/scripts/
  level1-e6-oracle.ps1`; bundles `...-162246-098` (24 near) and
  `...-162842-509` (40 far, capped)): 8x8–40px quads, depths
  `~0xB00`, fog bytes varying per corner, r20 zero, r24 =
  current-model slot pointer, r28 = material/context DWORDs.
- E6 execution attribution (2026-09-28, cross-bundle TTD
  positions are deterministic: census H1 order × oracle records):
  frame 1 runs E6-far 19x per imortalgun H1 (two H1s; five
  static faces each, so children + mirror multiply), E6-near
  24x after acam's single H1, and E6-far 2x after pipestop's.
  Acam has no E6 in its root program — its 24 come from `0x0E`
  children, which execute with NO H1 event (proven: H1 logging
  was complete at 41/60). So H1 counts bound lookups, not
  executions; child-heavy models multiply. Far total hit the
  40-cap in two identical spans (true count ≈38-40 far + 24
  near per frame).
- DE's [primitive owner](PRIMITIVE_RUNTIME.md#de-and-d6-source-polygons)
  retains direct Near/Far packets, signed nominal shade reads and three-source
  clipped fallbacks. `004A38C0/004A3900` dispatch through `+5C/+60` to the
  owned [shared-shade Near/Far consumers](PIXEL_RUNTIME.md#shared-shade-textured-triangles).
  Their allocation-corner UV mapping is source-proven; the early UV approximation
  is retired. Known shade high bytes are copied and unknown bytes stay unknown;
  the primary consumers require only RGB. Nullable H3 sorting and completed
  packet prefixes follow the primitive authority's explicit evidence bounds.
  Previous scoping note (kept for provenance): `0049CC10`
  mirrors E6 for three-vertex faces (`0xD8-0xDE`); the whole
  `0x4A38xx` block is renderer-virtual thunks, not pixel
  backends (near thunk re-disassembled to `+0x5C`, correcting
  an early `+0xBC` misread). `0xE4` (Handler-7 quad) uses kinds
  `0x4A39A0/0x4A39D0` (virtuals `+0x8C/+0x90`, still
  unmapped); its far/near shapes were skimmed, not owned.
- DA PRODUCER OWNED, consumer scoped open (2026-09-28):
  `prepare_da` (`model_primitives/da.rs`) mirrors DE for
  three-vertex H3 faces — mirror loop, 3-corner `0048FA60`
  cull, clip gate, null-tolerant material, brightness kernel
  feeding only the dropped prefix (`textured_sort_depth`) —
  but the record carries NO shade: eight DWORDs (corners,
  mode, material pointer); far adds far color + three fog
  bytes. Blocks: 2 near (`0x4A3A00`), 3 far (`0x4A3A40`).
  The decoder's shade word (+2) is unread by the main path
  (only the clipped fallback peeks past the instruction), so
  the producer requires the Shared shape without consuming
  its value; fallbacks `00498B10`/`00499130` stay open.
  Pinned: 5 producer unit tests. The consumer (renderer
  virtuals `+0x54/+0x58` = `004C2200`/`004C2DF0`, read from
  the `005290E8` table and cross-checked against the known
  `+0x4C/+0x7C` entries) is NOT transcribed. Frame-1 oracle
  (bundle `.tmp/infestation-ttd/20260928-205728-083-
  Level1Oracle2DG/`, tracked driver `runtime_re/scripts/
  level1-2d-da-entry-oracle.ps1`): 554's DA#1 (`0x02DA`) and
  DA#2 (`0x00DA`) both run FAR and ENQUEUE (2 faces x 2 imortalgun
  actors `0382CC70`/`0382CAB0`, not two draws; mat 633,
  mode 0, material pointer `0x0F5AD684`, far color
  `0x5284`, per-actor fog; corners match the 0x2D slot dumps
  (SUSPECT per the pre/post contradiction above — the dumps
  may be misattributed; re-validate against paired reads).
  Other models enqueue DA-near too (same material, one entry
  rejects). This CORRECTS the earlier "zero DA enqueues"
  claim (a ret-address filter blunder: call-site vs ret-site
  `0x49C897`/`0x49CAFF`; the range-filter rerun caught them):
  retail DA DRAWS, so imortalgun's frame-1 pixels are DA +
  E6, not E6-only. Consumer fully scoped statically
  (2026-09-29, `objdump -d --start-address=0x4C2200
  --stop-address=0x4C2E60 -M intel Infest.exe`; `ret 4` at
  `0x4C2DE5`, far twin entries at `0x4C2DF0`): the open
  `jne 0x4C2502` check is CLOSED — it selects the fill
  variant by the prologue flag (`[esp+0x28]` in
  `{0, 0x80000000, 0xFF000000}`): flag `== 0` fills the
  corner block `0x5627C0..0x56280C` pure (color
  `0xFF000000`), else a `+0x5C(0x1c, 0, 0x14D1)` setup plus
  `0x4F8450(0x544710)`/`0x4949A0` precedes the same-shaped
  fill (color `flag | 0xFFFFFF`). Corner XY come from the
  record words (`movsx [ebp+0/+2/+8/+A/+10/+12]`), w from
  `fild depth; fdivr [0x520A70]`, UV scales from the
  allocation-bounds floats (`[ebx+0x14..0x20]`) times the
  `[esi+0x38]` qword. `TEST AH, 2` at `004C266F` selects one
  draw or two; REA's listing (2026-10-12)
  shows its word is the selected variant's flags (loaded at
  `004C223D`), not the packet mode, and that `004C2696` also
  requires renderer `+44`. So variant flag `0x200` with a `+44`
  texture takes the two-pass, anything else one draw
  (clear: clip-or-`0x20`-bypass via `004BCCE0` on the
  corner block, single `+0x74` draw at `0x4C2D86`); the
  two-pass (set: `rep movs` 0x18 DWORDs to `0x560310`,
  `+0x74` draw of the corner block at `0x4C26F6`, refill,
  second upload, per-vertex UV-scale loop over `0x560310`
  with `0x5291C0`/`0x520A70` clamps, second clip, `+0x74`
  draw of `0x560310` at `0x4C2D04`). HRESULT tail:
  `0x8876021C` re-draws, otherwise the `0x80000000`-bit /
  `0x887601C2` tests gate the `0x4F8450`/`0x4949A0` chain;
  entry clip byte `0xFF` and <3 clip survivors skip to the
  common exit. Far twin differs from the top (extra
  `[esi+0x2c]+0x24/+0x28` and `[ebp+0x20]` fog path,
  setup `0x14D2`). Per-corner fill math FITTED 2026-09-29
  against all 9 draw-oracle corner0 blocks (exact to ~1e-8,
  so these are the transcription formulas, not hypotheses):
  `sx=x`, `sy=y` (record words, exact), `rhw=1/depth`
  (`[0x520A70]=1.0`, read from the EXE), `sz=1-64*rhw`
  (T=64.0 for every draw, both materials, near+far;
  source CLOSED 2026-09-29 by the fill oracle below:
  T=`[esi+0x38]` with fill-time `esi=0x0472D9F8` = the
  renderer object, `[esi+0x2c]=0x03832250` = the draw-time
  esi, so the port reads it as a renderer-context field
  pinned 64 — value proven, semantics open),
  `color=-1` (pass A) or `flag|0xFFFFFF` (pass B),
  `specular=((0xFF-fog)<<24)` with black color -- D3D fog
  alpha, 255 unfogged (`0x4C3215..0x4C3260`: fog byte,
  `inc`/`neg`/`shl 0x18`; far per-draw `0x8A/0x91` from
  record bytes `~0x75/0x6E`, near `0xFF` from fog 0;
  an earlier `fog<<24` shorthand stands corrected), `(tu,tv)` = allocation-bounds float pairs per
  corner (`(b14,b18)`, `(b1C,b18)`, `(b1C,b20)`; observed
  corner0 `(0.957|0.672, 1/256)`, V-scale `1/256 =
  [0x5291C8]`, clamp `[0x5291C0]=-1.0`). Still open for
  implementation: the `0x508FA0`/`0x4949A0`/`0x4F8450`/
  `0x4F8A50` callee identities, the `0x65EE4C` `+0x5C`
  state codes, the T=64/bounds/fog-byte sources as port
  texture-allocation fields, and the prologue flag rule.
  Callee
  identities resolved 2026-09-29 from the decompiled
  sources (no new capture): `004949A0` (`ovl_handlers.c`)
  drains the `0055F4E8` deferred-callback queue (type 2 =
  call) — the port's queue-flush equivalent, vacuous
  mid-consumer; `00508FA0` (`directx_wrappers.c`) is the
  texture upload/cache returning the texture pointer (the
  port already holds it from the producer's
  `host.texture()`); `004F8450` (`game_logic.c`) boxes an
  `0x80000000`-bit HRESULT into an error packet, else
  returns 0; `004F8A50` (`fgdk_engine.c`) consumes it
  (`0x8876021C` recovery attempt, else pass-through; the
  minimize-window arm is windowing, not pixels). So the
  HRESULT tail collapses in the software backend (no D3D
  device errors: the re-draw/chain branches are
  unreachable, backend errors propagate fail-closed —
  a labeled approximation at implementation time).
  `0x65EE4C` is the device-wrapper object behind the
  `+0x5C` (state) / `+0x74` (draw) virtuals. `+0x5C`
  state-code map RESOLVED 2026-09-29 against the port's
  native render-state owner (`d3d_textured_tests`: 7 =
  depth-enable, 14 = depth-write, 4 = perspective, 19/20 =
  blend src/dst, 28 = fog-enable): the prologue drives
  ZENABLE from `[esi+0x3c]`, ZWRITEENABLE from 0 or
  `[esi+0x40]` (material bit `0x31`), TEXTUREPERSPECTIVE
  from `~mode >> 8 & 1`, fog 28 from 0, and (SRCBLEND,
  DESTBLEND) from the material flag byte — `(2,2)`
  additive / `(1,4)` inverse-source / `(5,6)` source-alpha
  / `(2,1)` copy, exactly the port's `D3dTextureBlend` map
  (the pushed `0x14D1`/`0x14D2` belong to the following
  `0x4F8450` error-box line numbers, not the state call).
  Live `+0x74` draw oracle (2026-09-29, tracked driver
  `runtime_re/scripts/level1-da-draw-oracle.ps1`, bundle
  `.tmp/infestation-ttd/20260929-073512-356-Level1Oracle2DL/`,
  5.888 s, 10 draw/blk/ctx triples): 8 far draws at
  `0x4C3AF6` (block `0x560EA8`; 4 with mat `0x0F5AD684` =
  554's 2 faces x 2 actors, matching the entry oracle; 4
  with mat `0x0F5ACF84` = another model's 2 x 2) plus 2
  near draws at `0x4C2D86` (block `0x5627C0`, mat D684,
  bit-identical corner blocks from distinct records — same
  face twice, retry/double-visit mechanism open). All mode
  0, args `(obj=0x0CC75548=[0x65EE4C], 6, 3, block,
  surv=3, 0x1c)`, esi uniform `0x03832250` (`+0x2c=4`,
  `+0x38=0x0CC88058`, `+0x3c=0x0CC880F8` nonzero=TRUE for
  ZENABLE, `+0x40=0x000D59F8`, `+0x44=0x00002104`).
  Corner-block field map (consecutive read = scattered
  corner0 fields): X, Y, `[C8]=texq*W-K` (~0.96-0.98),
  `[CC]=W=1/depth` (~0.0003-0.0023), `[D0]=color` (-1 pass
  A; `flag|0xFFFFFF` pass B: `0xFFBFBFBF/0xFFCDCDCD`),
  `[D4]=fog` (`0x8A/0x91` far per-draw = per-actor fog;
  `0xFF` near), `[D8]/[DC]=bounds scales` (V=1/256 both;
  U material-dependent 0.957/0.672). Probe hygiene:
  breakpoints fire BEFORE the call, so esp points at the
  pushed args (obj=+0), not past a return address — the
  first 2DL run faulted on `poi(survivors)` proving it.
  Second-pass UV-scale loop spec'd 2026-09-29
  (`0x4C2B89..0x4C2CBA`, FPU-stack traced, no new capture):
  per-vertex `u=((X-P)*su*D)` / `v=((Q-Y)*sv*D)` with
  `P=[0x55F23C]` / `Q=[0x55F240]` (viewport center),
  `su=(1/[0x55F22C])/256` / `sv=(1/[0x55F230])/256`
  (focal over V-scale), `D=1/rhw` (depth), saturate to
  `[-1,1]`, then `tu=(u+1)*DU+C14` / `tv=(1-v)*DV+C18`
  with `DU=0.5*(c24-c1C)` / `DV=0.5*(c28-c20)` /
  `C14/C18` from the upload record (`[esi+0x44]+0x18`
  base). Walks 4 iterations (`edx=0x560374..0x560314`):
  the first writes out-of-block scratch (`0x560388`),
  the rest set the 3 drawn verts' tu/tv (offsets
  `+0x14/+0x18`, confirming the TL-vertex layout); the
  scratch write has no pixel effect and the port drops
  it. Port bindings: P/Q from its viewport center,
  su/sv from its focals, D/DU/DV/C14/C18 from its
  texture allocation + upload record. Fill/draw
  accounting (2026-09-29, tracked driver
  `runtime_re/scripts/level1-da-fill-oracle.ps1`, bundle
  `.tmp/infestation-ttd/20260929-074720-849-Level1Oracle2DM/`,
  5.401 s): 5 pass-A far fills, all with fill-time
  `esi=0x0472D9F8` (renderer) — 4 CF84 records plus one
  undrawn record (`0106F480`, clipped or mode-skipped);
  the D684 records and both near draws took the entry
  pass-B branch, so fill hooks must cover `0x4C31DA`+
  (far) and `0x4C2502`+ (near) for full accounting —
  coverage lesson recorded. Entry/tail flag split: CF84
  entered flag 0 (pass A) but refilled nonzero
  (`0xFFBFBF/0xFFCDCD` finals); D684 entered nonzero
  except the undrawn one — the prologue flag rule stays
  open with this split as its acceptance split.
  Prologue flag rule TRACED 2026-09-29 from the banked
  near prologue (`0x4C2200..0x4C2395`, no new capture):
  `bl` = low byte of the material word at
  `[matptr+(mode&0xFF)*28+8]`; `bl&0x30 != 0` takes pass A
  (flag 0; blend `bl&0x10 ? (2,2) : (1,4)`), else pass B
  (flag `bl&8 ? 0x80000000 : 0xFF000000`; blend `(5,6)`,
  or `(2,1)` when both `bl&8` and `[0x65EF2C]&8` clear).
  Frame-1 mapping (all mode 0): D684 `bl&0x30==0,&8==0`
  so entry pass-B fills with color `0xFF000000|0xFFFFFF =
  0xFFFFFFFF` (all 6 D684 draws are first-pass); CF84
  `bl&0x30!=0` so entry pass-A fills whose color is built
  from the record fog bytes (`0xFF-`fog derivations giving
  `0xFFBFBFBF/0xFFCDCDCD`), single-pass draws; the
  `flag|0xFFFFFF` store belongs only to tail refills,
  unobserved in frame 1. The tail machine rereads its
  flag byte from `[[esi+0x44]+4]` (upload record, not the
  material table) — same split logic, deferred with it.
  `bl`-as-port-field CLOSED 2026-09-29 by corpus
  join (scratch probe, since removed): sprite 633's
  variant flags = `SpriteFlags(5)` vs material `bl=0x05`,
  and sprite 577's = `SpriteFlags(20)` vs `bl=0x14` —
  exact matches both materials, so the prologue reads
  the mode-selected variant flags low byte (mode 0 in
  frame 1). Sprite 577 is 64x64 indexed/96-color (the
  CF84 draws' texture); 633 is 2x2 single-color.
  Upload-bounds banked 2026-09-29 (tracked driver
  `runtime_re/scripts/level1-da-bounds-oracle.ps1`, bundle
  `.tmp/infestation-ttd/20260929-075550-714-Level1Oracle2DO/`,
  5.6 s, 9/9 fills): pass-B D684 bounds base `0x03841038`
  with `(b14,b18,b1c,b20) =
  (0.95703,1/256,0.96484,0.01172)`; pass-A CF84 base
  `0x0384AA28` with `(0.67188,1/256,0.92188,0.251)`.
  Corner0 `(tu,tv)=(b14,b18)` matches both materials'
  draw blocks exactly, closing the fill->draw loop;
  per-corner UVs are `(b14,b18)`, `(b1C,b18)`,
  `(b1C,b20)` (CF84 samples a real subrect). Bounds
  base = material `+0x1C` heap ext + 8 (upload record);
  values stay acceptance data — the port derives them
  from its texture allocation at implementation time,
  never baked in.
  Material table banked 2026-09-29 (tracked driver
  `runtime_re/scripts/level1-da-material-oracle.ps1`,
  bundle `.tmp/infestation-ttd/20260929-075311-721-
  Level1Oracle2DN/`, 5.4 s, 8/8 far draws): `+0x00`
  clip bytes (0), `+0x04` sprite id (633/577),
  `+0x08` flags word (`0x00010005`/`0x00600014`, bl
  confirmed above), `+0x0C..0x14` per-material DWORDs,
  `+0x18`=0, `+0x1C` heap ext, `+0x20`=0, `+0x24`=id+1,
  `+0x28` small — constant per material across records.
  Still open: the two-pass path (variant flag `0x200` with
  renderer `+44`, which no static code sets; see
  [the secondary pass](PIXEL_RUNTIME.md#what-the-secondary-pass-draws))
  reports its boundary after the primary draw, and the
  CF84 fog-derived fill path (`bl&0x30 != 0`) skips fail-closed;
  far fog bytes and far color ride unconsumed (fog owner);
  then the real-0x2D. DA consumer OWNED 2026-09-29
  (`d3d_da.rs`: `read_da` + `prepare_da`, 7 focused tests,
  `software_frame` arm with `da_triangles` stat): single-pass
  entry only (`bl&0x30 == 0`; the earlier packet-mode `0x2` skip
  was a misread, corrected 2026-10-12), opaque white
  corners over the allocation-bounds UVs (`(b14,b18)`,
  `(b1C,b18)`, `(b1C,b20)`), flags-selected blend via the
  traced rule over the variant-flags low byte (global bit
  assumed set — every frame-1 draw runs SourceAlpha).
  Effect: 294 (hoverbot placement 188) submits through D2/D6/DA
  (19 submits, unresolvable 2, failed gate-only; frame counts
  tq 123 / eq 323 / da 1, fragments flat — its faces sit
  off-frustum in the fixed test view; unsupported back to 0).
  Update: second-pass UV math + submission transcribed
  (`da_second_pass_uv`, `prepare_da_second_pass`, 4 focused
  tests over the traced formulas with explicit center/focal/
  du/dv/c14/c18 inputs): the backend carries no focal/center
  state, so mode-`0x2` records keep skipping until that
  plumbing lands; no frame-1 path fires it (all mode 0).
  Update (far specular transcription, objdump, no capture):
  `0x4C3215..0x4C3260` fills specular as `((0xFF-fog)<<24)`
  (fog byte, `inc`/`neg`/`shl 0x18`); landed as
  `da_specular` + far fog bytes in the record (2 focused
  tests incl. banked 0x8A/0x91 anchors). Pixel-invisible
  for now (the flat backend ignores specular -- fog owner),
  load-bearing once fog lands. CF84 diffuse-fill site still
  open (the `0x4C2DF0` far twin decodes material-color
  expansion, not fog -- entry pass-A fill unlocated).
  Superseded 2026-10-13: both consumers' fills, blends and fog
  now follow native receipts
  ([DA textured triangles](PIXEL_RUNTIME.md#da-textured-triangles)).
- Unprojected-clip SKIP (2026-09-29, `clip_or_skip`, all eight
  face producers): a face whose corner slot was never projected
  (dynamic-only slots before their control op runs) skips the
  face (counted rejected) instead of aborting the model walk.
  Retail never exhibits these reads — its engine invariant
  projects every consumed slot — so no retail behavior is
  modeled either way; skipping fails closed per face where
  erroring failed the whole phase, preserving downstream
  faces the walk can draw. Callers must never invent a gate
  value; out-of-range slots still error. Effect: 552's DA
  faces skip (slots 28/30 await real 0x2D) and its E6-near
  faces submit — first imortalgun pixels (see pin). 390 also
  submits (3 textured tris + 1 effect quad, 0 fragments
  in-harness): walk-membership divergence (not distance —
  falsified; see instance selection), pixel-less for now.
  Pinned: walk pins (552 in roster; 390 submitting) plus the
  pre-existing suite, which guards the semantic (no test
  asserted clip-Unknown; passing walks never hit `None`).
- Gate chain scoped, not owned (2026-09-29): 223's program
  (53 instructions) runs `0x2D` (skipped) → DA (owned
  producer; corners include dynamic slots → skips) → E6
  (owned) → second `0x2D`/DA → E2 (owned) → children
  224/225/232/238, then fails at `0xD2` (in a child subtree —
  223's root has no `0xD2`; its queued E6 faces discard with
  the failed batch, so gate contributes zero pixels either
  way). 63 (200
  instructions, via 62's single `0x0E`) opens with five
  plain `0x0022` `0x22`s plus one `0x0222`, then
  `0x2D`/DA/`0xDE`/E6 faces with owned producers throughout.
  `0xD2` (near `0049EC80`, far `0049EF10` — an older note
  misattributed DA's `0049C710/0049C960` here; corrected
  2026-09-29 via the dispatch tables) mirrors the DE
  template (mirror loop, 3-corner cull, clip gate,
  null-tolerant material, prefix-only brightness) but its
  record carries per-corner shade DWORDs and NO UVs: corners
  + mode + material pointer + 3 shade DWORDs (12 DWORDs near;
  far adds far color + fog), kind `0x4A3930` → virtual
  `+0x64` (`004C6AB0`, `ret 4` at `0x4C8156`) — consumer
  fully scoped statically 2026-09-29 (objdump, no capture):
  +0x5C state machine (same code set as DA: 7/0xE/4/0x13/
  0x14/0x1C, no texture-handle select), `0x50E770` +
  `0x508FA0` upload + `0x4949A0` flush, float fills into a
  static `0x55FE08` block (X/Y/W/computed per corner plus
  integer shade/fog DWORDs, NO float UV fields anywhere),
  then `rep movs` + `004BCCE0` clip + SIX `+0x74` draws on
  the block (0x4C7513/3B/8075/9B/F7/811D) with the HRESULT
  tail. UV-source question sharpened, not closed: with no
  UVs and no texture select, the draws are either
  untextured gouraud (port `FlatTriangle` already carries
  per-corner rgb) or textured-with-clamped-garbage-UVs
  (shade DWORDs as floats); the state set cannot decide
  it because D3D texture state is sticky. Flat-vs-textured
  DECIDED 2026-09-29 (tracked driver
  `runtime_re/scripts/level1-d2-state-oracle.ps1`, bundle
  `.tmp/infestation-ttd/20260929-113927-141-Level1Oracle2DS/`,
  6.9 s; plus a fixed rerun after an off-by-one column
  bug briefly faked a TEXTUREHANDLE reading — owned):
  D2 draws TEXTURED. True states per draw are
  ZENABLE=1/ZWRITEENABLE=1/TEXTUREPERSPECTIVE=1 plus
  SRCBLEND=5/DESTBLEND=6 (SourceAlpha, same set as DA's
  textured path, no handle select — binding rides the
  `0x508FA0` upload, which runs per draw); the block is
  D3DTLVERTEX-shaped (sz/rhw/color/spec/tu/tv at
  +8/+0xC/+0x10/+0x14/+0x18/+0x1C, confirmed by field
  values) with a REAL varying UV triangle per corner
  ((0.004,0.324),(0.1445,0.324),(0.1445,0.441),
  constant across draws) and gouraud colors; sz fits
  `1-64*rhw` exactly (T=64 confirmed for D2 too, f32
  rounding only). Producer records banked 2026-09-29
  (tracked driver `runtime_re/scripts/
  level1-d2-entry-oracle.ps1`, bundle
  `.tmp/infestation-ttd/20260929-120136-341-Level1Oracle2DV/`,
  7.2 s, 10 near + 3 far; fixed revision after a first
  run faulted reading through EDX — the enqueue returns
  the packet in EAX and the record fills after, so the
  driver saves EAX at the post-call site and dumps at the
  mirror-loop jump): near = XY0,D0,XY1,D1,XY2,D2?,
  mode-ish 0, material pointer, 3 shade DWORDs, pad
  (kind `0x4A3930`); far adds far color + 3 fog bytes
  (kind `0x4A3960`). Thunk row decoded statically:
  `0x4A3930` → virtual `+0x64`, `0x4A3960` →
  `+0x68` — the scoped `0x4C6AB0` consumer IS the near
  path (its 8 draws consume 10 near records, 2 clipped),
  while the far consumer (`+0x68`) is unmapped and only
  matters if the port ever walks Far. Brightness wrinkle
  flagged: block colors look ~2x the record shades
  (`0x42`→`0x83`) but queue sorting breaks record↔draw
  pairing, so the shade→color scale stays an explicitly
  named approximation (like DE's dropped prefix) until a
  paired face proves it. Transcription plan: D2 producer
  (DE-template mirror/cull/clip/material/fallback,
  11-DWORD record) + backend emitting textured tris
  (corners from slots, colors from record shades, UVs
  from the block formula, texture via host resolution,
  SourceAlpha blend) — UV source CLOSED 2026-09-29
  (tracked driver `runtime_re/scripts/
  level1-d2-bounds-oracle.ps1`, bundle
  `.tmp/infestation-ttd/20260929-114740-237-Level1Oracle2DU/`,
  5.6 s, 10/10 invocations, one material `0x0F5AA104`,
  one upload): bounds `(0.00391,0.32422,0.14453,0.44141)`
  and the draw UVs are EXACTLY
  `((b14,b18),(b1C,b18),(b1C,b20))` — the identical
  per-corner assignment as DA, confirmed end to end
  (bounds floats → fill → draw block). The decoder
  already yields per-vertex shade refs for D2 faces and
  the shade-word-to-DWORD kernel is shared with DE, so no
  unknown remains: implement D2 producer + textured
  backend + tests, then 223 submits. Until
  the backend exists a D2 producer alone still submits
  into the open backend, buying nothing. Producer OWNED
  2026-09-29 (`prepare_d2`, DE-template transcription with
  per-vertex shades, 7 focused tests, executor arm wired;
  pins unchanged since the backend stays open).
  Historical D6 bring-up, 2026-09-29: producer `prepare_d6` landed
  (E4-quad-shape transcription with per-vertex shades, direct kinds
  `0x44EA60`/`0x44EA70`, null-material still enqueues, 7 focused tests,
  executor arm wired): 223/294/665 walk past into D6 faces
  and fail at the open D6 backend instead; fourteen more
  textured faces execute ahead of the failure (re-pinned
  437→451 tris, 70077→70078 fragments; roster/failed
  sets unchanged). D6 backend OWNED 2026-09-29 (`d3d_d6`:
  textured gouraud quad backend emitting per-corner-diffuse
  UV quads from the bound allocation rect, initially with a
  provisional blend/UV interpretation): 223/665 walks complete
  (re-pinned 12→18 submitted, 9→3 unresolvable, 451→537
  textured tris, 110→118 textured quads, 222→309 effect
  quads, d2 0→86, d6 199 quads, 70078→70079 fragments;
  roster 4→6 adds 223 x4 + 665 x2, failed 4→2 keeps 62/294;
  mission-22 pin moves 14→22 / 18→10 in the same direction).
  223's new faces rasterize but contribute ~zero fragments
  (off-frustum in the fixed test view — walk-membership
  divergence, named); 294 still fails on its next unowned
  face. Those counts describe that bring-up checkpoint, not the
  current shared terrain draw.
  Current D6 consumer ownership (2026-10-01) supersedes the old
  no-thunk interpretation and provisional `d6-blend-pin`: original
  `0044EA60/70` forward through renderer `+AC/+B0` to
  `004DBA90/004DDA30`. Both use the allocation's actual
  `(u0,v0),(u1,v0),(u1,v1),(u0,v1)` bounds, native flag-selected
  blend/depth policy and 255/128 shade normalization. Ordinary/alpha
  Far uses vertex fog; flags `0x10/0x20` fade diffuse RGB before
  normalization. The source-owned colored terrain counterparts
  `0044EA80/90` forward through `+94/+98` to `004D3EB0/004D47C0`.
  The native quad checker
  executes all four original consumers for 336 supplied-input cases;
  routine arithmetic/state agreement does not establish a matched
  retail scene. [Shared production pixels](TERRAIN_RUNTIME.md#shared-production-pixels)
  owns the detailed packet/UV/fog policy and remaining service boundaries.
  The [D6 producer owner](PRIMITIVE_RUNTIME.md#de-and-d6-source-polygons)
  supersedes the early bare-record/superset interpretation. `0049F800/0049FB10`
  write actual four-block packets with XY/depth, mode, the second H3 result
  and four shade DWORDs; Far adds color/fog. Near `0049B070` and Far `0049B6F0`
  use the shared clipper and serialize retained/interpolated corner RGB with
  explicit UVs. Source offsets, callback dispatch and evidence limits belong
  to that authority. D2 backend
  OWNED 2026-09-29 (`d3d_d2`: textured gouraud backend
  emitting per-corner-diffuse UV tris with SourceAlpha
  blend, UVs from the bound allocation, 4 focused tests;
  pins STILL unchanged — verified via scratch probe that
  model 223's walk now sails past D2 and fails at `0xD6`
  (opcode 214) in a child subtree, like 294/665). Naming
  caution recorded: this chain is model 223's (bound at
  placements 32-35, scientst actors in-port), distinct
  from gate model 62 (placements 2/81, whose child-63 walk
  now runs past its seven `0x22`s and stops at the `0xEE`
  depth read on slot 118 — downstream of its skipped
  `0x222D`, see below); retail gate identity stays open as
  before.
  Gate children use `0xD2` pervasively (224/225/232/238 each
  carry `0x05D2` plus deep `0x0E` trees) alongside fans and
  registers. Gate pixels need, in order: 0x22-plain OWNED
  2026-09-29 (`execute_jitter_22`, six focused tests; nibble-0
  exact advance, set bits jitter `args[1] ^ pass` into
  `args[0] + pass` through the shared `0055F18C` LCG with
  `(draw << 16) >> (args[2] & 31)` wrapping adds, bit 0
  stores world, bit 1 projects through the selected world
  entry — the same `+0x20` virtual 23/26 use; frame-1 child-63
  records banked statically: six `0x0022` + one `0x0222
  [120,44,25]`; RNG rides the scene per draw from the session
  word via `process_rng`, frontend scenes keep the
  unsupported boundary) + D2 producer + D2 consumer (both
  owned) + DA consumer + children. Jitter values are
  structurally faithful, not value-equal: construction skips
  (fail-closed unowned selectors) diverge the session word
  from retail's, so draw counts match but draws differ —
  evidence bound, not an approximation. Furniture
  stays dark behind its own caller (`004359A0` binds via H1
  then executes; the 64 constructed nonactors are the original 53
  TerrainLights (model 9) + 6 Route nodes + 5 map annotations. Furniture models
  have no arena objects at all, so the caller must come with
  unowned construction selectors) plus E6-far/`0x1A`/D2
  backends. Token04/frame direct paths (`0046DAF0` 260+
  lines/~20 callees, `00455080` similar) are not
  sky-pattern-trivial either. Nothing here is a quick win;
  the chain after the DA consumer is gate-`0xD2` faces as
  currently pinned (223 fails there).
- `0x2D` nibble-2 SKIPPED by labeled approximation (2026-09-28,
  skip-advance-2d-nibble2, replaceable; nibble 0 is an exact
  advance, other nibbles fail closed): frame 1 runs exactly
  two nibble-2 ops, both in grandchild 554 (`0x222D [28,2]` /
  `[30,2]`), which compute world through `00478950` and
  project through `004EBAA0` (far world) into slots 28/30.
  Evidence bounds (bundle `.tmp/infestation-ttd/20260928-
  194751-948-Level1Oracle2D/`, tracked driver `runtime_re/
  scripts/level1-2d-da-oracle.ps1`): callback object
  `001AFB18`, vtable `00520A44`, projection struct `00546E60`;
  world words per actor (28: `521/012bff12/0991`,
  30: `55c/012aff12/0a51` — SUSPECT per the pre/post
  contradiction flagged in the nibble-loop paragraph
  below; do not trust without a paired re-read),
  XY/depth/fog view-dependent (clip `0x12`); DA-near also enqueues for other models in
  frame 1, so retail DA DRAWS (two-pass) — this corrects the
  earlier zero-enqueue claim, which was a ret-address filter
  blunder (call-site vs ret-site), superseded by bundle
  `.tmp/infestation-ttd/20260928-205728-083-Level1Oracle2DG/`.
  The skipped slots' only frame-1 readers are the DA faces,
  and 554's slots 28/30 are dynamic-only (ranges end at 24),
  so the port skips those faces under clip-skip (no submit,
  no error) while retail submits them — the walk continues
  into 554's E6 faces either way (pinned: 552 submits; the
  clip-28 error pin retired). Follow-up owner: the real 0x2D
  with the DA two-pass consumer, validated against the oracle
  world words + targets + DA records when DA pixels are
  wanted. The same nibble-2 skip also covers 223's and 390's
  `0x222D`s; 223 stays failed (now at `0xD2`), while 390
  submits — walk-membership divergence, pixel-less
  (3 tris + 1 quad, 0 fragments); see instance selection.
  Nibble loop transcribed 2026-09-29 (`004EC670` static,
  ~60 insns to `ret 4`): per 2-bit group `bit0` stores
  compute outputs to the slot world, `bit1` projects via
  `[[esi+0x3C]+0x20]` (depth to slot+`0x10`), then
  `ebx>>=2`, slot `+=0x18`, loop while nonzero; exit
  advances the cursor by `raw>>12`. Nibble 0 is the exact
  advance (loop skipped); frame-1 `0x222D` nibble-2 passes
  run compute (outputs discarded) and project the slots'
  PRE-EXISTING contents — there is no out→slot-world
  mapping on this path. Pre-compute words (2026-09-29,
  tracked driver `runtime_re/scripts/
  level1-2d-precompute-oracle.ps1`, bundle
  `.tmp/infestation-ttd/20260929-105410-763-Level1Oracle2DP/`,
  11/11 nibble-bearing ops): slot 28 =
  (`0x40D`,`0x0107FF22`,`0x810`) twice, slot 30 =
  (`0x366`,`0xFF14019B`,`0x43C8`) twice (vertex base
  `0x00564E70`, cross-checked against the 2DH `a3`
  addresses `0x00565110/0x00565140`); also seen live:
  arg1-2 slots 18/20 and arg1-3 slots 116–122 (the
  kernel path fires in frame 1, just not for 554).
  CONTRADICTION flagged: these pre values differ from
  the older 2D-ret post values (`521/012bff12/0991`)
  although bit0-clear skips the store and the projector
  is projection-only per static read — RESOLVED
  2026-09-29 against the gate oracle (same driver + a
  `0x4C6DB` project-taken hook, bundle
  `.tmp/infestation-ttd/20260929-110239-809-
  Level1Oracle2DQ/`, 15/15 gates): 554's four passes DO
  reach the project call (gates n=1–4 are slots 28/30),
  so the missing-post mystery was hook-side, not retail.
  The post hook read world at `edi-0xC` (previous slot's
  tail) instead of `edi+0` — its world columns are
  retracted (my bug, owned), while its XY/depth columns
  stand (slot+`0xC`/+`0x10` were correct). Net: pre
  values authoritative, no-store + projection-only both
  statically certain, so slots 28/30 hold the pre words
  all frame; the older 2D-ret post values (`521/...`)
  stay SUSPECT (prime suspect unchanged: its unfiltered
  ret hook over the ~11-op mix). Claims resting ONLY on
  them — "world words per actor" above, the
  DA-corner/slot-dump match, item (a) as framed — RE-VALIDATED
  2026-09-29 with correctly-paired data, no slot dumps involved:
  the four 554 post-XY/depths (`00C403D1/A94`, `00C403DA/A92`,
  `00C70046/B05`, `00C7004F/B07`) exactly equal the four DA-far
  records' corner2 (`c10/c14`) in order — DA#1's corner2 comes from
  slot 28 and DA#2's from slot 30, same walk. Backend
  safety verified 2026-09-29 (static read of
  `software_frame.rs`): unowned callbacks fall through to
  `read_flat_triangle` → `Err(Unsupported)`, failing
  the model fail-closed per feature — so projecting
  0x2D slots before the DA backend exists would drop 552
  from the roster (partial 0x2D arm confirmed unsafe).
  A UV-carrying textured-tri pixel arm already exists
  (`TexturedUvTriangleNear`), the DA backend's most
  plausible emission target once its inputs exist. Third leg, static
  (2026-09-29, corpus read via scratch probe, since removed):
  model 554's faces are `0x000002DA [20,2,28]` and
  `0x000000DA [21,2,30]` (mat 633 both) — corner2 IS slots
  28/30 in the authored bytes, while corners 20/21/2 are
  static (port-exact, matching the clip-skip analysis that
  only 28/30 gate). Program → pass → record, triple-validated.
  The old match conclusion stands on this pairing; the old
  slot-dump path stays retired. Item (a)
  CLOSED: the port 0x2D arm runs compute (world store only when bit0 set),
  projects pre-world when bit1 set, and DA corner2 consumes the projection.
  Project-args oracle (2026-09-29, tracked driver
  `runtime_re/scripts/level1-2d-projectargs-oracle.ps1`, bundle
  `.tmp/infestation-ttd/20260929-110945-080-Level1Oracle2DR/`,
  5.9 s, 15/15 passes): proj=slot+`0xC`, coords
  `(edx,s0,s4)` = the compute outs (slot-28 pass:
  `73E8/36A/FFFF982F`), while the slot world AT PROJECT
  is (`521/012BFF12/991`) — NOT the pre words
  (`40D/0107FF22/810`). Writer discriminator (2026-09-29,
  tracked driver `runtime_re/scripts/
  level1-2d-writer-oracle.ps1`, bundle
  `.tmp/infestation-ttd/20260929-111501-126-Level1Oracle2DT/`,
  6.4 s): bl=2 EXACT at `0x4EC6BD` (store skipped exactly
  as transcribed), yet the slot already reads post values
  there — and the refine-alias is bank-DISPROVEN
  (`0x48CF70` gets `[actor+0x140]`, whose `+0x1C` bank
  `0x0382D010` never touches slots). The only remaining
  writer path is the listener chain (`0x47A660` →
  `0x490A80` virtual family, unread). Leading theory:
  dynamic slots are pool-leftover-initialized per
  invocation (pre values deterministic across walks) and
  the compute-listener writes true values on touch.
  CONSEQUENCE (2026-09-29): real-0x2D needs the H8
  sim-state family — listener callbacks, execution-context
  scratch, refine/sim tables — the same unowned family
  blocking `0x1A` slot-records, walk-membership inputs and
  fade. That exceeds this goal: no honest labeled
  approximation exists (banked slot words are captured
  values, never behavior; zero-init projects degenerate).
  Proposal: close this goal's entity-pixel arc here and
  open a dedicated H8-context objective (one owner
  unblocking `0x1A`, 0x2D sources, fade and
  walk-membership inputs together).
  Probe hygiene: parenthesize shift/mask chains in MASM
  `.if` (the first 2DP run's `(x>>8 & 15) != 2` filter
  matched zero hits with no error; unfiltered rerun
  worked).
- `0x2A` main-path COPIES owned (2026-09-28,
  `execute_copy_2a`): bits 8-9 copy one parent slot into the
  destination, bits 10-11 the mirror-indexed slot (`^1`) into
  destination + 1 — full 24-byte slots (world + projection
  tail), no mirror on the indices. The parent index resolves
  through the invoker's word table, read as the child frame's
  own `0x0E` state words: proven for 554 (`[14,12,10]`
  equals the directly-read table words, oracle bundle
  `.tmp/infestation-ttd/20260928-200722-238-Level1Oracle2DC/`,
  tracked driver `runtime_re/scripts/level1-2d-copy-oracle.ps1`;
  single-case evidence, fail closed outside it). The port
  snapshots the invoker's vertices (owned clone — the invoker
  is suspended, so values are exact). A clear mode word is an
  exact no-op; without a parent table the set-bits project
  path stays explicitly open (fail closed). Pinned: 4 unit
  tests (both groups, table/slot bounds, null parent) plus
  the arm test. Frame-1 effect: none observable yet — the
  copies fill 554's slots 24-27, but the walk still stops
  inside DA's clip gate on slot 28 (0x2D still skipped), so
  the pin is unchanged by construction; the copies light up
  with the real 0x2D when 554's E6 faces submit. Null-table
  map retained for its owner: projects via `00490900` +
  translates by (`0055F1C4/C8/CC`), joined at `004ED43E`
  (`0xA00` gate, virtual `[ebx+0x3C]+0x1C` projection, bit-11
  3-DWORD copy + depth-word copy) — stays open.
- E4 OWNED (2026-09-28, producer + near/far consumer): `prepare_e4`
  (`model_primitives/e4.rs`) mirrors the DE template for quads —
  mirror loop, 3-corner `0048FA60` cull, clip gate, Handler-7
  material, `flat_sort_depth` key (`mode & 0x16`, no brightness
  kernel), `004A42B0` enqueue. Record is eleven DWORDs (four
  corner XY/depth pairs, mode, the material record's first
  DWORD, shared shade DWORD); far adds far color + four fog
  bytes (slots+`0x15`). Scope corrections from transcription:
  E4-far is only `0049D8E0..0049DBCE` (later `0x48FA60`/`0x4A42B0`
  sites past the `ret` belong to other handlers); the epilogue
  `lea eax,[edi+0xC]` proves the six-word instruction the
  decoder already consumes (no decoder change); clipped
  fallbacks are `00498060` (near) / `00498570` (far), now the
  [clipped material wrapper](PRIMITIVE_RUNTIME.md#clipped-dc-and-e4-faces)
  shared with DC. Consumer:
  H7 materials are flat colors (`MaterialValue`), so the
  `+0x8C/+0x90` backend is a shaded flat quad — `FlatQuad` +
  `read_shared_shade_quad` + `prepare_flat_quad` (`d3d_flat.rs`)
  reuse the flat-triangle display-shift base color and the
  `modulated_rgb` kernel, uniform across corners. Far quads
  now blend their fog like `004C1940`
  ([E4 quads](PIXEL_RUNTIME.md#e4-shared-shade-quads)). LABELED
  approximation, replaceable: the E4 shade high byte stands in
  as zero (E4's remaining approximation; unconsumed). A null/unresolvable H7 material has
  no retail skip path (unconditional deref), so the port
  propagates the host error fail-closed. Pinned: 6 producer
  unit tests + 3 reader/prepare tests; the level-1 walk drives
  552 root + child-553 E4 faces (6 submits, near) and reached
  the grandchild-554 `0x2A` ops (since skipped by labeled
  approximation; fails at `0x2D` now). Side effect, named: the continued
  walk resolves later faces' textures, adding portable ordinals
  that reorder blended draws (fragments 70020→70017, same
  submits — retail sorts by native addresses, not claimed).
- Remaining entity-pixel scope (2026-09-28, child inventories +
  disasm sizes; EXCEEDS one turn — proposal, not a blocker;
  imortalgun's face/control chain (`DE` + `0x4A38C0` consumer +
  `EA` + `0x15` + `0xE4`, child 553 adding `0x15/0xE4` plus
  its own DE/E6 faces) is now fully owned — all landed in the
  EA/DE/E4 commits above. What remains for imortalgun is the
  grandchild-554 `0x2D` face (plus `0xDA`); the `0x2A` copies
  prerequisite is owned since the 0x2A-copy commit.
  Gate needs `0x2D` + `DA` + `DE` + `EA` (`0x22` owned
  2026-09-29; child 63 also uses `0x2B/0xE6/0xEE/0xEF`/F-range,
  all owned — its walk now stops at the `0xEE` depth read on
  slot 118, unwritten behind its skipped `0x222D [118,8,44,0]`,
  exactly the imortalgun chain: real-`0x2D` + DA consumer).
  FIDELITY TRAP (partly tripped — see 390 above): saber/hoverbot/
  tomspawn (and scientst) are retail-unvisited in frame 1 (no H1)
  but port-walked; the walk-membership gap (active set, not a
  distance threshold — falsified) decides what the port may
  additionally draw. Cull (walk-membership) before (or with)
  further opcode owners. Original
  proposal (all face owners landed since; superseded by the
  Update below): `EA`+`0x15` (tiny, dead until DE) → DE
  producer+consumer → `0xE4` → distance cull → gate leftovers
  (`0x2D`, DA — `0x22` landed 2026-09-29).
  Update 2026-09-28: `EA` (`004A0810`) and `0x15`
  (`004A08F0`) are owned (single-word depth-group openers
  mirroring `0xEE` without bias words; sorting false/true
  respectively; differential test against `0xEE`). The DE
  producer + near consumer are owned (DE commit), so `EA`/
  `0x15` lit up with DE. Update 2026-09-28 (E4 commit): the
  E4 producer + near/far shared-shade-quad consumer are owned
  (see E4 bullet), so root + child-553 faces execute end to
  end and the walk reached the grandchild-554 `0x2A` ops.
  Update 2026-09-28 (0x2A-skip commit): the `0x2A`-bits copies
  are skipped by labeled approximation (see bullet), so the
  walk fails at the grandchild-554 `0x2D` face instead
  (pinned: `opcode: 0x2d`). The DA producer is owned (dead
  until 0x2D; consumer open, see bullet). Update 2026-09-28
  (0x2D-skip commit): nibble-2 `0x2D` is skipped by labeled
  approximation (see bullet), so the walk runs DA's gates on
  the unwritten dynamic slots and fails inside DA's clip gate
  (pinned: clip byte, index 28) — same failed model, zero
  pixels. Update 2026-09-28 (0x2A-copy commit): the `0x2A`
  main-path copies are owned (see bullet; null path stays
  open), filling slots 24-27 for 554's E6 faces. Update
  2026-09-28 (DA-entry oracle): DA draws in frame 1 after all
  (see DA bullet — the zero-enqueue claim was a filter
  blunder), so the chain to imortalgun pixels is real-`0x2D`
  + DA consumer, estimated over a day. Proposed order from
  here: real `0x2D` (compute `00478950` + far-world project,
  validated against the oracle world words; compute in/out
  pairs captured in bundle `.tmp/infestation-ttd/20260928-
  201928-249-Level1Oracle2DD/`, tracked driver `runtime_re/
  scripts/level1-2d-compute-oracle.ps1`; per-type inner
  targets captured in bundle `.tmp/infestation-ttd/20260928-
  210547-983-Level1Oracle2DH/` (tracked driver `runtime_re/
  scripts/level1-2d-inner-oracle.ps1`): 554 routes through
  vtable `0051F080` to `004260C0`, gate-223 through `005289C0`
  to `0048B290`, others through `00520860` to `00415D30` — a
  per-callback inventory, one transcription per type;
  `004260C0` dispatches on arg1 and fans out to `00439E10` +
  `0048CF70`, four functions deep per slot write. Update
  2026-09-29 (glue transcribed, objdump static): `00478950`
  is 30 insns — inner virtual call then a delta-normalize
  tail `out[i] = base[i] + sext16(in[i]-base16[i])` with
  base from `[[ctx+0x58]+4+4i]`. `004260C0` dispatch:
  arg1 2 → `0x426213` (frame-1 554 path), 3 →
  `0x42617E`, 10 → `0x426135` (terrain-sample path),
  else the default terrain-coupled direct path.
  CORRECTION: `00439E10` is NOT the rotation kernel — it
  is a 15-insn 3-word row copy `dst.w[i] =
  u16[[ctx]+extra0*24+2i]` (`ret 4` at `0x439E32`). The
  200-line x87 kernel is `00439E40`, called ONLY on the
  arg1==3 path — off frame-1 554's path, so the multi-day
  estimate for OUR path collapses. Arg1==2 path, complete:
  frame words = table row extra0 (`0x0A`/`0x0C` for 554's
  ops); refine `0x48CF70` iff `[actor+0x140]` nonzero;
  out = sext of the frame words. `0x48CF70` (~50 insns)
  is side-effect-only for our outputs (scratch is
  read-only; it writes sim tables and calls `0x47A660`
  twice) — frame-1 implementations skip its side effects
  as a labeled approximation (simulation tables are out
  of scope) with no slot effect. Caution: the 2DH
  driver's inner `a0..a3` are off by one stack slot (2DI
  reads correctly: this=actor, p1=scratch, p2=ctx,
  arg1=2, p4=extra-words). Compute entry words +
  outputs captured in bundle
  `.tmp/infestation-ttd/20260928-210850-786-Level1Oracle2DI/`
  (tracked driver `runtime_re/scripts/level1-2d-compute26-
  oracle.ps1`). Rotation hypothesis reframed:
  extras SELECT precomputed rows (the +2 step between
  slot-28/30 extras with B-delta [-44,0,+2] is a row
  difference, not a live rotation); the separable fit
  stands as row arithmetic. Remaining for real-0x2D: (a)
  CLOSED (gate + paired pass→record match: compute runs,
  pre-world projects, DA#1 corner2 ← slot 28, DA#2 ← slot
  30), (b) the row-table source (`[ctx]` rows) as a port
  field, (c) the `004EBAA0` far-world project — CLOSED
  2026-09-29 with zero code change (tail read, no capture):
  the `>=0x40` path is focal-divide, center-add,
  OR-magnitude `0x7FFF` halving, fade-interval fog,
  XY/depth/clip stores — every step matches the port's
  owned `ModelProjectionBackend::project(World)` (same `<64`
  uncentered path, same shift round-trip, same fog shape).
  The 0x2D arm routes projection through the existing
  backend, so no projector work remains. Update 2026-09-29
  (rule-shape fit, no new capture): the banked pairs are
  additively separable and deterministic (690-model repeats
  identical outputs per op): out = A(actor) + B(slot,extras)
  with B(30)−B(28) = [-44,0,+2] for both 554 actors, o4 a
  per-ctx constant (554:874, 223:538, 690:320, 460:360).
  Transcriber entry point: items (a)–(c) above, then the
  port 0x2D arm computes rows → out → slots) +
  DA two-pass consumer
  (records captured in bundle
  `.tmp/infestation-ttd/20260928-205728-083-Level1Oracle2DG/`,
  tracked driver `runtime_re/scripts/level1-2d-da-entry-
  oracle.ps1`) → then E6 + DA faces submit and imortalgun
  draws → `00479570` distance cull → gate leftovers (`0x22`,
  `0x2D`, DA).
  Update 2026-09-29 (arg1==2 path independently verified +
  transcribed as pure code, objdump, no new capture):
  `004260C0` head reads arg1 from `[esp+0xC]` (a2), dispatches
  `-2 → 0x426213 / -1 → 0x42617E / -7 → 0x426135` exactly as
  noted; `0x426213` takes extra0 = `u16[extras+0]`, calls
  `00439E10(dst, ctx=a1, extra0)`, refines iff
  `[actor+0x140]` nonzero, and writes the outs across two
  pops — `out = (buf[0], buf[1], buf[2])` in order, no
  reversal. `00478950` verified: inner virtual
  `[this+0x3C]+0x10` gets `(a0, ctx, arg1, extras)`, tail is
  `out[i] = base[i] + sext16(in_low[i]-base16[i])` with
  wrapping 16-bit subtract. CORRECTIONS to the draft above:
  `00439E10` derefs its ctx param (`edx = [edx]`, so rows
  live at `[[ctx]]`, not `[ctx]`) and copies source words at
  byte offsets 0/4/8 (u16 0/2/4 of the row), not contiguous
  `+2i`. Landed as `model_compute_2d.rs` (dispatch, row
  fetch, sext, tail; 6 synthetic tests, no captured words):
  live wiring stays closed on item (b) — the `[[ctx]]` row
  table and `[[ctx+0x58]]` base as port fields.
  Update 2026-09-29 (ctx-identity oracle, tracked driver
  `runtime_re/scripts/level1-2d-ctx-oracle.ps1`, bundle
  `.tmp/infestation-ttd/20260929-194039-117-Level1Oracle2DJ/`,
  5.365 s, 6/6 calls with order-matched rets at the
  arg1==2 `ret 0x10`): `[ctx]` = `0x00564E70` (554) /
  `0x005655F8` (other model) — both above the image end,
  so runtime-allocated per-model tables, not static exe
  data; `[ctx+0x58]` = the global render context
  `0x00546E88` for both; extras = row/pass words
  (`0x0A/0x0C` for 554's ops); `[actor+0x140]` nonzero in
  all 6 (refine taken) yet rets equal `sext(rows)` exactly
  — refine output-neutral confirmed live, and the
  transcribed inner is retail-exact on every observed
  call (including high-bit `0x8706` → `0xFFFF8706`,
  closing the movsx open check). Tail hand-verified
  (n=1 reproduces the 2DD outs bit-exact from base
  `[0x7D84,0x216,0]`). Item (b) is now a precise
  shopping list, all three H8-owned: the per-model row
  table source, the per-invocation extras records, and
  the render-context base words as port fields.
  Update 2026-09-29 (extras-in-args + rows-as-slots,
  in-port walk probe, since removed): 0x2D args carry the
  extras directly — `args=[slot,?,extra0,extra1]` (554:
  `[28,2,10,0]`/`[30,2,12,1]`, matching the banked ex
  words 6/6), so no H8 extras records exist to own; the
  extras shopping item collapses. Rows are approximately the
  invocation own slots: port 554 slot-10/12 worlds vs the
  banked rows agree in ALL relative geometry (slot-10/12
  x-difference 44 exact both actors, y/z constant, local
  offset `(22,297,352)` identical across invocations) with a
  per-actor pure translation delta (actor1 `(+19,+17,-1)`,
  actor2 `(-19,+18,+1)`): rotation exact, absolute origin
  off — retail actor origins differ from authored
  placements by delta (spawn-settle prime suspect). So
  real-0x2D needs only true actor origins (3 DWORDs per
  actor, sim-state) plus the render-context base words;
  rows/slots and extras are port-owned already.
  extra0-as-source-slot fits every nibble-2 op seen
  (gate 44/46, 390 40, 666 38, dest=`args[0]`);
  `args[1]` (2/10/9/8) and extra1 refine meaning stay
  open. 04796C80 (banked ex0 0/1) matches no port-walked
  model — walk-membership note, not a 0x2D blocker.
  Update 2026-09-29 (actor-position oracle, tracked driver
  `runtime_re/scripts/level1-2d-actor-oracle.ps1`, bundle
  `.tmp/infestation-ttd/20260929-195226-622-Level1Oracle2DW/`,
  5.09 s, 6 entries + record heads): actor xyz lives at
  `this+0x32/34/36` (u16s in `w4hi/w5lo/w5hi`); 0382CC70 =
  placement 4 bit-exact, 0382CAB0 = placement 5 within 1 LSB
  (y 560 vs 559) — spawn-settle is DEAD (actor1 origin exact
  yet its rows still carry delta), and actor1=placement4 /
  actor2=placement5 mapping is now fixed. Refined attribution:
  the delta is translation-like (+-19 x, +17/18 y) with a +-1
  z split between the two slots — consistent with a ~3-degree
  orientation error in the 552->553->554 child chain acting on
  ~460-unit locals (common shift plus dR-times-44 x-split of
  ~2 in z), NOT an origin offset. Prime suspect: child-chain
  orientation handling; next owner reads orientation words
  (actor record above +0x3C, or the 0x0E child path).
  Update 2026-09-29 (ctx-basis oracle, tracked driver
  `runtime_re/scripts/level1-2d-ctxbasis-oracle.ps1`, bundle
  `.tmp/infestation-ttd/20260929-201450-899-Level1Oracle2DX/`,
  5.089 s, 24 events): `ctx+0x18..+0x38` holds three
  basis-vector triples (X@+0x18, Y@+0x24, Z@+0x30 — same
  column convention as the port `words[1..9]`); `+0x3C` is
  the projector struct (nibble loop confirmed live), `+0x40`
  renderer, `+0x44` callback object, `+0x48` sprite bank,
  `+0x4C/+0x50` H8 globals, `+0x54` far color. 554 carries
  a live yaw+pitch turret matrix (mirrored yaw per actor;
  other model exact diag(–1,+1,–1)). Transposed-basis
  plus port origin reproduces the rows sub-unit (T-agreement
  spread <=0.36, mean within ~2.5 of port origin): the delta
  was NEVER translation — pitch times z-local gives the
  +17y, yaw the x/z. Origin vindicated; basis is the entire
  gap modulo FISTP rounding. Lessons: (1) row-major misread
  fakes a +35 translation; (2) two slots leave a twist
  ambiguity — the +-2.6-degree yaw-only fit was its shadow,
  broken only by the direct basis read. Missing input now
  singular: the per-actor basis from packed 0x04/0x03 angles
  (552 Xz yaw + 553 Yz pitch, both zeroed by the World
  policy at that dated port baseline); the next investigation banks those values against the sine
  table (expect table index ~118-119 for actor1).
  Update 2026-09-29 (rotate-angle oracle, tracked driver
  `runtime_re/scripts/level1-2d-rotate-oracle.ps1`, bundle
  `.tmp/infestation-ttd/20260929-202157-164-Level1Oracle2DB/`,
  6.95 s, 19 yaw + 16 else rotate calls): 0x03 handler
  transcribed (`004A0AE0`: save via `00490520`, packed eval
  `004ED4B0`, axis 1 to `0x491530` / 2 to `0x4917D0` / else
  to `0x491290`, angle pushed + basis in ecx). !tt
  order-matching attributes actor1 yaw 562 + pitch 0xFDCE,
  actor2 yaw+pitch both 0xFDCE, other model yaw 0x4000.
  Angles reproduce the banked ctx basis BIT-EXACT through
  the executable table (permanent acceptance test
  `banked_actor1_angles_reproduce_ctx_basis`; a first-pass
  typo (65038 for 0xFDCE) briefly faked a mismatch, owned).
  Chain complete, all verified: 0x04/0x03 values -> table ->
  basis -> rows (slots) -> sext -> tail -> project -> DA.
  Then remaining for live wiring, two items: (1) the 0x04/0x03
  VALUES (per-actor sim state; World policy zeroed them at that baseline) and
  (2) the render-context base words as port fields (origins
  already port-exact within rounding). Current captured-source getters now
  have a [separate owner](OBJECT_MODEL_CALLBACKS.md); the dated oracle is not
  complete animation or custom-vertex acceptance.
  Update 2026-09-29 (base lifecycle + writers + camera
  source; tracked drivers `level1-2d-basewatch-oracle.ps1`
  (2DA, 18 events), `level1-2d-basereread-oracle.ps1` (2DY,
  6 reads), `level1-2d-basewriter-oracle.ps1` (2DZ, 18 events
  with trap EIPs), `level1-base-source-oracle.ps1` (partial,
  fault noted)): the base triple IS `[ctx+4/8/12]` (self-loop
  `[ctx+0x58]` points back at ctx); compute-time values
  `[7D84,0216,0000]` 6/6 across two drivers. Lifecycle:
  per-draw fills (camera-word copy at `00410D9E` for
  pre-draw; stack-computed at `0041E1C8`/`0046C60A`) plus
  zeroing paths (`0041DBE0`/`0041DFD7`); context allocated
  by fresh-heap `00495800`. The pre-draw fill source is the
  spawn camera `0383E910` (vtable `005204D0`), words
  `[7D84,0216,A35F]` at `+0x32/34/36`, sign-extended.
  ANOMALY (banked, unresolved): fill writes `+12=FFFA35F`
  yet computes read 0 with no trapped write between
  (watches armed throughout, caps unhit) -- silent mechanism
  or trap blind spot. HARMLESS on all observed rows:
  out2 = B2+sext16(in2-Lo(B2)) coincides for B2 in
  {0,0xFFFFA35F} for every in2 outside `[0x235F,0x7FFF]`
  (all frame-1 in2 exceed 0x8000). Remaining: the +12
  zeroing attribution, camera+0x32 semantics, the 0x04/0x03
  values; then wiring (rows/slots, extras, origins, base).
  Update 2026-09-29 (packed-selector correction + value
  pairing; tracked driver
  `runtime_re/scripts/level1-callback03-oracle.ps1`, bundle
  `.tmp/infestation-ttd/20260929-211711-534-Level1Callback03/`,
  26.2 s, 107 evals + 107 paired returns + 107 stub calls):
  CORRECTION -- selectors are 0x04 (552) / 0x03 (553), NOT
  0x44/0x43 (`and edx,0xff3f` clears bits 6-7: 0x84->0x04).
  Paired values: actor1 (562, 0xFDCE), actor2 (0xFDCE,
  0xFDCE); stub targets are 004789B0 for compute-ctx evals
  (cb=001AFB18; the nonzero-selector delegate at
  `[cb+0x3C]+0x14` is the next owner) with the args pointer
  at [esp+8]. Lessons: (1) an early revision hunted ghost
  selectors 0x43/0x44 (three zero-hit runs, owned); (2) the
  stub at 004ED4D4 is shared -- [esp+4] is the pushed edx
  (selector), [esp+8] the prologue-saved esi (args pointer),
  verified by 1:1 pairing with the register reads; (3) a
  hand-written breakpoint body with pre-escaped quotes
  double-escapes through Hook() -- always write plain quotes.
  Update 2026-09-29 (delegate + turret words; tracked
  drivers `runtime_re/scripts/level1-delegate-oracle.ps1`
  (54 delegate calls: per-actor-type targets 004015A0 /
  0048B330 / 00415BC0) and
  `runtime_re/scripts/level1-actor-words-oracle.ps1` (6/6
  calls)): imortalgun delegate `004015A0` reads the angle
  straight from the actor record -- sel 3 returns
  `[actor+0x188]`, sel 4 (and default) `[actor+0x18C]`
  (sel 2 computes from `+0xA0/A2`). Actor words match the
  banked rotate angles EXACTLY (0382CC70: FDCE/0232;
  0382CAB0: FDCE/FDCE; other model 0438/0440): the turret
  joint words ARE the missing input, end to end.
  FULL CHAIN VERIFIED: actor+0x188/+0x18C -> sel 0x03/0x04
  evals -> 0x03 handler rotates -> ori-6 inheritance ->
  554 basis -> rows (slots) -> sext -> tail -> project ->
  slot 28/30 -> DA corner2 -> DA pixels. Remaining for
  live wiring: (1) turret-word sourcing (LIVE per the
  liveness update below -- needs the AI tick, not
  construction), (2) render-context base
  mapping, (3) the +12 attribution. Origins/rows/extras
  already port-owned.
  Update 2026-09-29 (turret liveness; tracked driver
  `runtime_re/scripts/level1-turret-live-oracle.ps1`, bundle
  `.tmp/infestation-ttd/20260929-212308-422-Level1TurretLive/`,
  5.373 s, 12 events): +0x188/+0x18C are LIVE -- three
  write rounds (frame-1 values, then two updates) by one
  writer per word (`0040208A`/`004020DD`). Statically the
  writer is a tracking servo (yaw integrates a halved
  delta, pitch subtracts with `[0xFFFFC001,0x1000]` clamp),
  so the values need the AI tick, not construction: owning
  them is gameplay-AI sim work, out of drawing scope.
  Frame-1 values stay acceptance-only (never baked).
  Remaining for wiring: render-context base mapping, the
  +12 attribution; then DA two-pass + furniture tables.
  Control (same driver, ba5-8): the missing-hook anomaly
  was a bp/ba ID collision (`ba1` redefined `bp1` -- the
  transcript says so); fixed run shows frame-2 computes
  consuming round-2 words live. Hygiene rule recorded:
  ba IDs share the bp namespace. Remaining for wiring:
  render-context base mapping, the +12 attribution; then
  DA two-pass + furniture tables.
  Update (view-write oracle; tracked driver
  `runtime_re/scripts/level1-view-write-oracle.ps1`):
  RETRACTION -- bd-poisoning is dead: both prior "failed"
  runs show FRAME_END post-hoc (poll race: COMPLETE flushed
  first); the grace-retry fix is validated. Real lesson:
  poll for the end marker with retries, and verify the
  generated breakpoints file (quote layers bite).
  Draw entries bracket +12=FFFA35F on both draws while it
  flips per entity -- per-draw scratch, last-writer-wins.
  The silent transition is now a +12-only write inside
  draw 1 (rep-string blind spot suspect); harmless per the
  out2 proof on all observed rows.
  Update 2026-09-29 (furniture table read + fill watch;
  tracked drivers
  `runtime_re/scripts/level1-table-read-oracle.ps1` (27/27
  binds: base `0x0472F3A8`, entries `{slot, X}` with slot in
  the low word -- all 10 bind-oracle pairs confirmed live)
  and `runtime_re/scripts/level1-table-fill-oracle.ps1`
  (0 writes in-frame): the table is load-time filled, and
  lookup is NOT the bank local index (in-port probe, since
  removed: registration locals {77,74,49,35,6,48,45,47,75,
  125} vs lookups, no constant shift). Entry high words
  {15,516,16,16,16,16,15,12,15,0} stay open (not counts,
  not residency locals). Next: load-window fill hunt
  (earlier start anchor needed) with the H14 bind order
  (opaque handlers) as prime fill-rule suspect.
  Update (fetch-vtable read; tracked driver
  `runtime_re/scripts/level1-fetch-vtable-oracle.ps1`): the
  H11 manager is the static object `0x00527750` whose vtable
  holds the shared destructor (`004EFBF0`, returns 0) and
  Handler 11 Load (`00437B50`) -- the null-path "fetch"
  cannot populate anything; the table comes from the Load
  path or an earlier fill.
  Update (load-order killed both directions; in-port probes,
  since removed): the port binds 436 H8 models across leaves
  in deterministic order, but neither 0-based nor 1-based
  k-th bound globals match the table at lookup positions
  (wanted globals sit at non-constant offsets too). Common
  bank shows structural global = local + 2 throughout (not
  the mapping). Executed-opcode sets banked as a bonus:
  552 = {03, DE, E6, 0E, EA, EF, F1, F5} (no 0x05/0x1C),
  553 adds {15, 16, 1B, E4, 07, EE, F3}, 554 (16 ops) has
  no 0x03/0x05. Next: trace-begin fill hunt,
  first-appearance order, or H14 parsing.
  Update (bind chain + bank kills; static + in-port probes,
  since removed): `004508E0` (terrain furniture submission)
  resolves lookup the same way -- slot from the H11 table,
  then model record from H-table 8 (`[[0x55EEE4]+slot*4]`,
  null goes through the `0x55EEE0` virtual fetch). The port
  already does slot->model via residency, so ONLY the
  lookup->slot fill is missing. Lookup is NOT any bank
  local: scene-bank locals map wrong, common-bank locals
  mismatch (though common shows a structural global =
  local + 2). Next: load-window fill hunt (earlier anchor)
  or load-order reconstruction (k-th bound model test).
  Update (bind-head transcription + bank-local kill, static
  + in-port probe, since removed): `004359A0` head loads the
  table base from `[[0x55EEFC]]` (null goes through the
  `0x55EEF8` virtual fetch), indexes `base+lookup*4`, takes
  the H1 slot from the entry low word, then runs
  constructor `00437A70` + H1. Lookup is NOT any bank local
  index: scene-bank locals map to wrong globals
  ({50:589, 13:390, ...} vs wanted), common-bank locals
  mismatch too, no constant shift -- the table is genuinely
  runtime-filled (H-table 11 object `0x55EEF8`, slot array
  at `+0xFC`; Handler 11/12 Load call `004EFC00` per entry
  but no `0x55EEFC` writer exists in the decompiled
  sources). Next: load-window fill hunt from an earlier
  anchor watching the known base.
  Update (mission-table + loader gap; in-port probes, since
  removed): the only resident handler-11 table is slot 0
  from level 6 (identity slots) -- mission-18 tables never
  load (lazy fetch via the `0x55EEF8` virtual on first bind).
  The port transcribes `004EFC00` but its available requests
  (slot-0 and world-source overlays) map identity; the fetch
  source (mission OVL region? H14?) stays open with the
  vtable read as the next micro-step.
  Update (bind owned; static scan + parser read, no capture
  needed): the runtime table is a copy of `6.ovl` region 0
  (header DWORD + 256 `{slot, X}` DWORDs) -- all 10 oracle
  pairs match at `base+lookup*4` in table shape, so no
  translation runs. The parsed `MissionPlacementSlot` already
  carries the slot as `resource_word` (the parser labels it
  deliberately uninterpreted; the slot field stays the
  index). Landed as `bind_furniture_lookup` (first word at
  the lookup index). The [current draw authority](FURNITURE_RUNTIME.md)
  supersedes the old `FD..FF` omission: only zero skips this consumer.
  It also owns `00437A70`, the distinct RNG gates and H1 submission
  with the actual setup position.
  Update (walk-domain oracle; tracked driver
  `runtime_re/scripts/level1-walk-domain-oracle.ps1`, bundle
  `.tmp/infestation-ttd/20260929-224059-489-Level1WalkDomain/`,
  4.835 s, exactly ONE 0044CD00 call in frame 1): the domain
  struct IS the render context (`0x00546E88`); origins are
  its `+4/+0xC` words (`0x7D84/0xA35F`); rows=50 (frustum
  window, not the 512 grid); coords are per-draw scratch
  (`0x001AF310`, first word `0xFFF60005`); F0/F4 stable at
  (`0x80,0x7F`). UNIFICATION: base triple == domain origins
  == camera triple (all `[7D84,0216,A35F]`), so rows/origins/
  base/coords collapse onto ONE owner family (camera pose +
  terrain frustum); the esp+4 rows read validates the
  prologue-shift map. Port focus vs banked (0,+173,+384)
  still reads as chase-eye offset (lean EYE); the +0x40
  camera dump stays the discriminator. No fixture possible
  (origins/coords/rows all per-draw) -- furniture pixels
  wait on the camera/view-setup owner with terrain frustum.
  Update 2026-09-29 (camera record + bd-poisoning lesson;
  tracked driver
  `runtime_re/scripts/level1-camera-record-oracle.ps1`, bundle
  `.tmp/infestation-ttd/20260929-214000-375-Level1CameraRec/`
  (FAILED run -- cap-2 `bd 1` poisoned the session, but the 2
  pre-bd camera dumps are valid)): camera `0383E910`
  (vtable `005204D0`) carries position words
  `[7D84,0216,A35F]` at `+0x32/34/36` (+0x30=1) -- the base
  triple IS camera words, sign-extended. Port spawn view:
  focus (32132,361,41439) vs banked (32132,534,41823): x
  degenerate, delta (0,+173,+384) reads as chase-eye offset
  (lean EYE, `[0,-1.0,0]` basis row at `+0x44/48/4C`; +0x40
  dump named to confirm vs target). So base needs the TRUE
  camera pose (`00488380`, open) -- not the spawn
  approximation, not focus alone. LESSON (load-bearing):
  a mid-run `bd N` SILENTLY STOPS all later breakpoint
  delivery in this WinDbgX/TTD setup (this run plus the
  cap-8 Callback44 run); caps must be unhittable overflow
  guards, `bd *` terminal-only.
  Update 2026-09-29 (clip-skip commit): faces with
  never-projected corner clips skip instead of aborting the
  walk (`clip_or_skip`, all eight producers). 552 submits
  with E6-near/E4/DE pixels (DA faces skip; DA consumer still
  open for DA pixels); 390 submits pixel-less (walk-membership
  divergence, see comparison). Proposed order from here:
  real `0x2D` + DA two-pass consumer (DA pixels for 552 and the
  gate chain; needs the H8 sim-state family — dedicated-objective
  proposal on record) → `0x1A` slot-record lifecycle (needs H8
  execution-context records) → furniture domain rows + `004359A0`
  + H1 (bind-oracle pairs are the acceptance map; H8 slot tables
  open) → remaining terrain `0044AA10` services → remaining far backends/fog.
  Walk-membership itself (the 5-instance active set) is
  simulation-owned and stays unapproximated.
- E6 `+0x28` paradox RESOLVED (2026-09-28): the fateful read is
  `[EBX+0x4]`, not `[EBP+0x4]` — the bytes are `8b 4b 04`
  (ModRM rm=011 is EBX; EBX was reloaded to CTX at `0x49E38D`),
  so `[0x28] = [CTX+4 table][shade^mirror]`, no fault, no
  mystery. Caution, recorded so nobody re-trips it: objdump's
  Intel printer rendered those exact bytes as `[ebp+0x4]` in
  one invocation while its AT&T output and a second Intel pass
  agree on EBX — cross-check rm-field reads in both syntaxes
  (a scan found no other affected lines in the E6/0x1A dumps).
  The `0x00564Exx` pool pointers from the ebpread probe are
  vertex slots (`EBP==slot+0xC` verified 12/12; stride-`0x18`
  pairs), and `[slot+0x10]` depths stand. Remaining E6 inputs to
  source in port terms: the CTX+4 per-shade DWORD (candidate:
  shade-pool raw, vs port's decoded rgb — compare oracle `r28`
  low bytes against port shade computation) and the brightness
  material bytes (`[record+8+idx*28]` vs sprite entry fields).
- Current portable level-1 draw expectations are owned by
  [`world_scene_tests::level1_draw_pins_submit_fail_split_and_first_pixels`](../../crates/inf-game/src/world_scene_tests.rs).
  This fallback fixture checks model submissions, per-feature skips and
  pixels; its five admitted map annotations contribute to the nonactor
  count without adding world model submissions. The separate native-camera
  fixture requires reached terrain commands. These portable fixtures do not
  replace the retail draw observations or simulation-state conclusions below.
- `0x1A` slot-record semantics (2026-09-28, three sweep probes;
  TRANSCRIPTION BLOCKER): the `00405540(bank, slot)` counts at
  `[S+0x14/+0x16]` vary per model — plant01 (346) reads
  `129/127` while lockhoop/smallgun read `64/64` (flag bytes
  zero in all three). Neither file-header ranges (`[0,6]` /
  `[4,8]` / runway `[22,22]`) nor vertex/shade counts explain
  them, and a load-span write-watch on lockhoop's entry shows
  zero writes: the counts predate the mission-18 load (boot /
  handler-install time). Bank `0x0055EDA8` is global and stable;
  entries are pointer records (`S+0x04` = self-index), not a flat
  array. The port has no H8 slot-record lifecycle, so a faithful
  `0x1A` transcription needs that subsystem first (or a proven
  derivation of the counts). No clean approximation exists —
  `esi` is a load-bearing `idiv` divisor. Next: own the H8
  execution-context/scratch records, or pivot to faces /
  selector 6 / the furniture caller, which do not need them.
  Update 2026-09-29 (bank oracle — lifecycle SOLVED, mode
  source open): the 0x1A bank is the H3 sprite bank interface
  (`ctx+0x48` == `0x0055EDA8` at all 4 frame-1 executions),
  and the counts ARE sprite entry variant-0 dims (`S+0x14` =
  variant+0x10 width, `S+0x16` = variant+0x12 height — 585/282
  → 64x64, 346 → 129x127, proven against the port's parsed
  sprites). The earlier model-field hunt looked at the wrong
  records. No lifecycle is needed to READ them: the port
  resolves sprites by slot already, so the `0x1A` arm is a
  sprite resolve (fail closed when unresolvable) and the
  producer takes dims from variant 0. Brightness tail decoded
  against the same layout (`S+0/1` = mode bytes, `S+8+b*0x1C`
  = variant flags, `S+4+b*0x1C` = sprite_id sort key — the
  owned `0xFF`/`0x39` sort rule verbatim), and the packet IS a
  valid `00466150` E2-near record (mode at +8, entry at +9),
  so no new backend is needed. Remaining opens before the
  producer lands: the mode word at `ctx+0x5C` (0 in all 4
  frame-1 executions; writer unowned — structural mapping, not
  a baked constant) and the `ctx+0x10` brightness virtual
  (sort-depth only: collapses into the portable sort with no
  pixel impact on non-overlapping quads). Packed evals take
  record words 2/3 (objdump). Full input list and bundle
  references live in the ledger entry above. Mode-sweep
  2026-09-29 (bundle `.tmp/infestation-ttd/20260929-172542-
  139-Level1Oracle1AM/`, tracked driver `runtime_re/scripts/
  level1-1a-mode-sweep-oracle.ps1`): `ctx+0x5C` reads 0 in
  16/16 executions across 10 world draws (20/20 with the bank
  oracle; only ~4 of 10 draws fire, presumably
  dispatch-gated). Update 2026-09-29 (producer LANDED, no new
  oracle needed): E2-near reads the same `ctx+0x17` word into
  its packet mode (`0049DBE0`), and the port already maps it to
  `PrimitiveState.mode` (documented as "Model context +5C";
  0x1B does the same) — so `prepare_ground` takes `state.mode`
  through the established channel, not a new approximation
  (20/20 zeros on both sides; scene mode is 0 everywhere).
  Writer follow-up stays shared with E2/0x1B. `ground.rs`
  transcribes entry math, packed evals (record words 2/3 per
  objdump), FPU scale, angle/idiv/fixed-mul (shared kernels;
  u16 extract proven equivalent over the full sine/divisor
  domain), i16-truncated corners, fog gate (bounds travel as
  host inputs, banked `[1024, 768]`; derivation is the
  view-setup follow-up), brightness via the shared sort owner
  (`cVar8` collapses: sort-depth only), and emits the E2-near
  packet shape. 15 focused tests (C-semantics fixtures over a
  synthetic sine table). Frame-1 result: 294 walks past `0x1A`
  (gated out in fixed view) into D2/D6 and stops at the DA
  backend — the goal's slice 2 on deck; roster/failed sets
  unchanged; acam's wheel children stay unreached in-port
  (0x16-gated or frustum-culled, register/frustum state
  unmodeled — reach follow-up).
- `004ED610` never fires in this frame (once per mission-22
  frame) — open question, not a blocker.
- OVL 28 (mission 18 via the `selection+10` rule) is in the local
  corpus, and the port loader generalizes (`ovl =
  mission_selection + 10`), so no data or loader work gates the
  re-target. What re-runs per map: slot inventory, draw pins,
  oracles, camera/fade values. What transfers unchanged:
  executor, queues, raster, kernels, producer shapes.
- Decision record: re-target the world-drawing goal to mission 18
  (Jev 96% on re-consult with this census, up from 46%; cutscene
  reframe 2%, briefing 2%). Mission-22 pins stay green as
  regression coverage for the shared owners.

### Mission-18 frame-1 comparison (retail vs port, 2026-09-28)

The deltas below describe the saved 2026-09-28/29 port baselines. The
[current shared terrain bridge](TERRAIN_RUNTIME.md#shared-production-pixels)
and [furniture owner](FURNITURE_RUNTIME.md) supersede their terrain/furniture
skips; the retail observations and capture provenance
remain valid.

Scope and honesty limits first: the retail back-buffer was never
read (TTD text probes cannot capture GPU memory), so pixels are
compared structurally (which models submit what primitives),
not byte-wise. The port's pin-test pixels render in a FIXED test
view (eye origin, `0x200` focals), retail in its native spawn
camera — even matching submissions would rasterize at different
screen positions. Submitted-primitive comparison against the E6
and `0x1A` oracle record sets is exact only where both sides
execute the same producer. Settings mismatch, carried by every
row below: the port draws with EXE-default Detail 0 (coarse
pair) while the recorded session ran Detail 1 (fine pair,
proven by the spacing-pair oracle) — furniture density and
height sampling differ by settings before any owner does.

- Camera/view: retail eye `(8341, 109, -14374)` with fade
  `(2224, 10176)`, fog `0x83D`, focals steady `0x200/0x200`.
  Port: frozen spawn framing (`SpawnViewApproximation`, labeled),
  template focals, frontend-fed fade, depth
  `(1,1)` both sides. DIFFERS everywhere except depth state
  (named causes: unowned `0040FD10` pose, unowned `00488380`
  fade chain; focals match semantically, differ by tier).
- Terrain at that baseline: retail rasterizes `0044AA10` coverage; port skips
  (`skipped_terrain`). Heights share the `TerrainMap` decoder
  (slope precompute runs per load) but mission-18 slope values
  are unpinned. DIFFERS (unowned terrain subtree).
- Runway x10: retail looks it up but executes zero `0x1A`
  (all `0x16`-gated; no primitives, no pixels). Port never
  invokes it (no furniture caller). PIXEL-MATCH (both dark)
  for different reasons; retail register state is unmodeled.
- Furniture (pipes/bunkers/palms/banners/posts, 13 slots):
  retail executes via `004359A0` (27 binds == 27 furniture H1s,
  lookup→slot map + anchors + fade banked by the bind oracle);
  port absent. DIFFERS (unowned caller + faces; E6 owned since,
  still unreachable without the caller; walk + setup slices
  owned with the OVL-28 census banked — remaining: domain rows,
  `004359A0` + H1).
- imortalgun x2: retail draws (E6-far x19 per H1: five own
  faces plus fourteen from child 553, exact with no mirror
  multiplication needed; plus DA-far records, mat 633).
  Port SUBMITS all seven placements: root DE/E6/EA, child-553
  E4, grandchild-554 E6-near effect quads (~214), E4 flat
  quads (6) and DE triangles (~166) rasterize — FIRST
  IMORTALGUN PIXELS. DA faces skip (slots 28/30 await real
  0x2D), so DA pixels are still missing. PARTIAL then
  DIFFERS (missing DA two-pass consumer; port E6-near vs
  retail E6-far fog/dispatch deltas; all face producers
  owned). Dispatch asymmetry confirmed and narrowed: the
  port walks Near where retail runs far — the fade/dispatch
  comparison stays open.
- Gate x1: retail draws; port walks past its seven owned `0x22`s
  and the owned D2 backend, stopping at the `0xEE` depth read on
  slot 118 — unwritten behind the skipped `0x222D`, exactly the
  imortalgun chain (real-`0x2D` + DA consumer, H8-blocked).
  DIFFERS.
- 390-entity x1: retail draws NOTHING (culled, no H1); port
  SUBMITS (3 textured tris + 1 effect quad) but rasterizes 0
  fragments in-harness. DIVERGES in submit, not yet in
  pixels — walk-membership divergence (the port walks all
  placements; retail walks the 5 active instances), owned by
  the missing active-set selection, NOT a distance threshold
  (falsified above).
- Instance selection (2026-09-29, oracle bundle
  `.tmp/infestation-ttd/20260929-061120-885-Level1EntityVisit/`,
  tracked driver `runtime_re/scripts/level1-entity-visit-
  oracle.ps1`): retail's entity path visits EXACTLY 5
  instances — imortalgun `0382CC70`/`0382CAB0` at port
  placements 4/5, commship `03834BC0` at placement 166, gate
  `0382B0B0`, acam `04796C80` (= vehicle). Extra commship
  (213), extra imortalguns (6,7,92,104,105) and 390 (191)
  never visit: upstream selection, not the `+0x2C&0x80`
  flag (clear on all visited) and not frustum/dispatch
  (zero port-walk culls). Exhausted static discriminators —
  distance-to-spawn-focus, distance-to-retail-eye, height,
  grid cell, placement index, model — NONE separates drawn
  from culled (nearest-by-focus culled 7/104 falsify
  distance outright). The discriminator is simulation-owned
  active state (out of scope: per-tick simulation), so no
  rule is approximated and the over-draw stays pinned with
  this cause. (Open: retail gate position matches no port
  placement — moving actor or word-signedness question,
  moot while gate stops at the `0xEE`/118 skip-consequence.)
  Update 2026-09-29 (entity-path map, no new capture):
  `0047AFC0` (~195 lines) is visual check (owned) →
  transform publish → `+0x2C&0x80` skip (fail closed, writer
  unowned) → grid visibility COMPUTATION, not a cull
  (cells `(x>>7)&0x1FF` vs `0x5A`, table `0x548404` fog
  bytes; no skip branch) → clamps → H1 bind always. So
  selection lives UPSTREAM of this predicate (which 5 get
  visited), and placement records carry no active flags
  (H13 is relocation framing; class suffixes opaque). The
  manager was untranscribed at that historical baseline; the 64
  constructed nonactors are the original 53 TerrainLights + 6 Route
  nodes + 5 map annotations, so furniture models have no arena objects and
  their caller needs unowned construction selectors first.
- Commship: retail draws ONE (`03834BC0` = placement 166);
  port draws BOTH placements (268 textured tris + 10 quads +
  6 effect quads across the pair) — walk-membership
  divergence, not distance. PARTIAL then DIVERGES (over-draw
  of placement 213).
- Acam x1: both submit the vehicle; retail adds wheel quads
  (`0x1A` x4 oracle) plus child E6 faces, port emits zero
  pixels. PARTIAL (0x1A producer owned but its wheel walks stay
  unreached in-port — 0x16-gated or frustum-culled children).
- token04 x4, frame x1: retail draws via `0046DAF0`/`00455080`;
  port has no such paths (models fully owned). DIFFERS
  (unowned direct-path callers).
- gunpower/hudgnade: retail draws via `0041C320`, the vehicle HUD's
  weapon icons (models `0xAF` and `0xB4`); the port now draws them
  ([vehicle HUD](VEHICLE_DRIVE_RUNTIME.md#the-vehicle-hud-0041c320)).
- Backdrop/sky: retail draws; port draws it too since the
  `00440DA0` caller landed (resolve H8 `0x116`, submit at the
  eye's X/Z and the world's H14/0 +10 backdrop height;
  session-flag/capacity gates vacuous in the world draw). First sky pixels: backdrop textured quads fill the
  background (fragments in the tens of thousands).
- scientst x4, saber19 x2 (except the submitting models below),
  factories x3, hoverbot/tomspawn (except the 390-entity): retail
  draws none of them; port FAILS most of them the same way, but
  model-223 followers x4 and model-665 followers x2 now SUBMIT
  through the owned D2/D6 backends (roster 4→6) while retail
  culls them — the cull-trap warned about above has tripped
  for these two models: DIVERGES in submit, ~zero fragments
  in-harness (off-frustum in the fixed test view). Owning
  further opcode owners without the `00479570` active set will
  trip it for more models.
- Submission order: retail interleaves manager/entity passes
  with terrain; port walks placements + vehicle. DIFFERS
  (manager-list order open).
- Net: port submits 18 models (vehicle, 2 commships, 7
  imortalgun, 1 390-entity, 4 model-223 followers, 2 model-665
  followers, sky) where retail submits ~8
  entities + furniture + sky + direct models over terrain;
  port pixels number in the tens of thousands (sky backdrop
  fills the background) with entity faces on top, where retail
  covers the frame. Every delta above names its owner; unknowns are the
  drawn-retail commship's placement identity and the exact
  retail back-buffer.

### Port native-draw status and first-frame comparison

`world_scene::draw_world_frame` (with `view_focals`, `depth_policy`,
`entity_submission` and `WorldDrawOutcome`)
replaces `draw_skeleton` (the arcade `tick_drive` stand-in it left in place
has since been retired for the native vehicle update). Per frame it
runs the worker preamble (`begin_frame`), view setup from BSS-zero
focals, the `+10` depth decision from live settings `+50`, and one
`BeginBatch/Model/FlushBatch` per drawable actor (visual model WORD !=
`-1`, the exact `0047AFC0` cull) in placement order plus the respawned
vehicle. Each Draw freshly executes `00478E00` through the same factory-input
preparation as Tick, borrowing live session residency independently from the
renderer resource snapshot. It captures visibility before acquisition, then
reaches fade/setup and normal material before reading the returned camera's
signed eye and complete ten-DWORD basis, followed by `00410E5A`'s shift 16.
Draw does not run pose or consult the last Tick's identity as an admission gate;
reached pose-error mutations remain visible. Factory, material and unknown-position
errors preserve their reached prefixes. Actor `0047AFC0` and camera `00410D90`
sign-extend all three position WORDs; source placement/terrain conventions
remain separate. The sky uses this same eye. A NULL acquisition preserves the
fade/material prefix and existing view/shift, then reports the separately
unowned retained-context draw feature. The spawn-fit fallback is removed.
The [camera authority](CAMERA_RUNTIME.md#acceptance-and-external-boundaries)
owns those boundaries and `WorldProjectionSetupApproximation`, the existing
atomic projection/fog publication's bounded difference from native scalar stores.

Normal-world predraw `00488446..00488477` publishes the converted H7 material
`0x74` as context `+6C` far color. Constructor `00487C5F/65` clears world
`+5C3/+5C4`; load `0048DD5B` clears `+5C4`, and tick `004875CE` copies it to
`+5C3`. Its only other writer is the vehicle update's L key with Infrared
vision (upgrade `0x3D`) owned (`00417922..00417932`, `+5C4 = !+5C3`). A
nonzero `+5C3` selects H7 `0x20` instead
([infrared vision](#infrared-vision)).
Briefing's own template/decoration resets to far color zero remain separate.

The world now owns `00488380`'s `+5C8/+5CC` fade through `world_fade`, seeded
from manager `+1C/+20` after load. The shared timing ring writes `0055F154`
before animation, and `0048E2E0` updates manager visibility before actor/camera
ticks. Predraw applies the native 64-unit deadband slew and publishes bounded
world fade; offline reload retains manager F and timing history while reseeding
the slew fields from the new header. `WorldFrameInputs` supplies the view
zoom `G+5C0` that the free look `00421DC0` writes (the session's
`CameraGlobalState::view_zoom_5c0`, constructor zero). The source
chain, units and remaining input boundary belong to
[world visibility](WORLD_VISIBILITY.md). Complete terrain services and
actor-task owners remain separate retail-acceptance boundaries.

Current production terrain uses `SoftwareScene::draw_terrain` after the entity
passes, supplying this caller's retained view/projection, terrain sampling and
loaded tint. The briefing pane uses the same bridge with its own camera and
retained fade. Native D6 and colored Near/Far quad consumers now execute through
the shared deferred queue; the command counter includes submitted prefixes
before a later strip/service failure. Live settings +80 selects the recovered
90x90 process RGB grid: each world draw clears its cells, publishes camera-derived
anchors, deposits active-particle light before sprite rejection, then runs the
[shared Flag0 object-light pass](OBJECT_LIGHTING.md#shared-consumers-and-remaining-draw-boundary)
before the [shared object manager](OBJECT_DRAW_RUNTIME.md) and pending particles.
Later terrain reads the same cells. The manager now owns fresh embedded-list
traversal, terrain/cone admission, actual-class callbacks, packet lighting, FFFF
flag/list effects, flash and separate late execution. Its named per-object
omissions retain required backing and custom/ordinary-vehicle boundaries. Under
[infrared vision](#infrared-vision) the draw first seeds the grid with the
player's green light. The briefing prefix clears the same
cells while preserving anchors, and uses the same settings-gated ground-read
policy with its retained load-time tint. Native E150 ground clipping reaches the shared
triangle queue. [Furniture](FURNITURE_RUNTIME.md) and [water](WATER_RENDERING.md)
now share the retained world and briefing paths. The shared
[terrain-emission producer](TERRAIN_RUNTIME.md#near-strip-terrain-particle-emission)
uses actual trail/pool/RNG owners; its independent process event writer
remains open. `004395A0/C0` are
profiling markers, so entity, active/pending particle and terrain submissions
retain one deferred queue until the caller flushes. Sprite/light authority lives
in [particle runtime](PARTICLE_RUNTIME.md); positional queue/playback authority
lives in [audio runtime](AUDIO_RUNTIME.md). Detailed terrain policy and the 336-case original-consumer receipt
belong to [terrain runtime](TERRAIN_RUNTIME.md#shared-production-pixels), not a
new first-frame retail acceptance claim.

The following placement-roster comparison predates the shared manager and
is retained for its unique captured native census, not as current port status.
Historical retail comparison for the frame after the `3->4` request at `0040D542`
(port headless/windowed baselines `.tmp/inf-baseline/gameplay-native*.png`
vs TTD bundle `.tmp/infestation-ttd/20260928-060800-917-FirstFrameDraw/`):

- Depth state matches: `(1,1)` both sides (Z Buffering default).
- View setup matches semantically (zero-init then template focals, no
  doubling); values differ by display tier (port `[320,320]` at
  640x480 vs retail `[0x200,0x200]`).
- Submission set: the port walk covers all retail-drawn entity slots
  (`0x228` x8, `0xDF` x2, `0x1DA` x2 through `0047AFC0`); without the
  `00479570` distance cull it over-submits the remaining actors, and
  submission order is placement order, not the manager-list order.
- Model pixels differ, all with named causes, confirmed by the
  FrameOpsProbe census between frame-1 world-draw entries (bundles
  `.tmp/infestation-ttd/20260928-070651-058-FrameOpsProbe/` and
  `...-070823-483-FrameOpsProbe/`): retail executes 0x2B/0x2C
  projection controls, 0x2D custom vertices, E2-near quads, E6-FAR
  quads and 07-FAR there, while 0x22, E6-near and 07-near never fire.
  The port owns 0x2B/0x2C/07-near/0x1C and reaches the unowned
  paths retail does not take with zero registers, so its per-slot
  stops (0xE6 for `0x228`, 0x2D for `0xDF` and 448, 0x2A for 698,
  0x22/0x2D/0x07/0xD6 for culled actors) mark the owner list, not
  retail behavior. Measured split at first green: 4 submissions complete (slot
  474 through owned 0x2C + 07-near), 27 fail closed across 9
  unique slots, and the frame carries its first real pixels
  (effect-near quads, fragments > 0 — pinned in
  `world_scene_tests::native_draw_submits_first_world_entities_to_real_pixels`,
  re-pinned for later owners (22 submissions / 10 unresolvable
  after the D6 backend + `0x22` arm; retail walks past `0x2D`
  too, so the direction matches).
  Terrain differed at that saved baseline (then-unowned `0044AA10`);
  the current shared bridge replaces that skip without closing complete
  terrain comparison. The
  sky draws since the `00440DA0` caller landed (resolve H8
  `0x116`, submit at the eye's X/Z and the backdrop height), camera framing differs
  (fit-at-spawn stand-in).
- The presented frame is the native `+5C4` forced-black clear with no
  surviving fragments yet; driving still works under it.

  Window acceptance 2026-09-29 (mission 22, the default new-game
  mission; mission-18 window coverage stays harness-only):
  headless `--hidden --fixed-step-micros 16000` with replay
  `.tmp/inf-baseline-replay.json` walks Title (phase at frame 1,
  accepts input from ~300) → Frontend menu (300) → TextEntry
  (2263) → Briefing (2513) → Gameplay (2514); snapshots
  `.tmp/inf-baseline-gameplay-start.png` (frame 2520) and
  `.tmp/inf-baseline-gameplay-driven.png` (frame 2950, Up held
  2520→2920) differ substantially (spawn view to driven
  view), proving the (since retired) arcade drive moves the world under the
  native draw. The previous baseline replay is superseded
  (its early Returns predate the ~300-frame title gate and
  drift to AttractDemo). Mission-18 in-window flow stays
  open (no in-app mission select; new game defaults to 22).

  Update 2026-09-29 (debug `--mission`, mission 18 in-window):
  new game with `--mission 18` walks the same menu flow into
  briefing/gameplay for OVL 28 (replay
  `.tmp/inf-baseline-replay-m18.json`): `.tmp/inf-baseline-
  m18-start.png` (frame 3400, blue-sky ring + tan terrain +
  model specks — unmistakably level 1, not the mission-22
  yellow disc) vs `.tmp/inf-baseline-m18-driven.png` (frame
  3800, Up+Left held) differ substantially, proving drive
  under the native draw on 18 too. The flag only selects the
  OVL; rendering, briefing and simulation fidelity are
  unchanged by it. Re-verified on the post-`0x22`/furniture/
  spacing tree (`.tmp/inf-check-m18-start.png`,
  `.tmp/inf-check-m18-driven.png`): gameplay reached on that dated tree;
  start-vs-baseline and driven-vs-baseline each differ by
  exactly 1 pixel in 307200 (newly submitting D6 walks;
  exact face unattributed), start-vs-driven differs in 41%
  (drive moves the world under the native draw).
  Current portable draw expectations are pinned separately by
  [`world_scene_tests::level1_draw_pins_submit_fail_split_and_first_pixels`](../../crates/inf-game/src/world_scene_tests.rs).

### Frame-1 camera pose evidence (TTD, this turn)

- Retarget: exactly two `00410D90` hits in the interval, `417B06:619`
  (frame-1 pre-draw) and `41F235:615` (frame-2 pre-draw), byte-identical
  camera structs. Frame-1 eye shorts `(0x2095, 0x006D, 0xC7DA)` =
  `(8341, 109, -14374)`; basis floats `b0..b6 = 0, -0.367, 0, -0.68,
  -0.042, 1.0, 0.014`; heights `+0x64/+0x68/+0x6C` present. Bundles
  `.tmp/infestation-ttd/20260928-123504-632-Frame1ViewProbe/` and
  `...-123610-927-Frame1ViewProbe/` (exact `db`/`!tt` command lines
  in each bundle's `invocation.json`).
- Published view block mapping (corrected — an early read was off by
  one DWORD): `0055F190/94/98` = eye XYZ, `0055F19C..1B4` = basis
  `b0..b6`, `0055F1B8/BC` = heights. X/Y match the retarget exactly;
  Z reads 12 off (`-14362` vs `-14374`), unresolved — needs a write
  watch on `0055F198` to close. A `ba w4` watch proved `0055F194`
  is written exactly twice (both retargets, return `004884C6`), so
  the pre-draw owns the block each frame.
- Historical fade receipt: `0048FB00` takes `(context+0x64, context+0x68)` (disassembly
  `mov edx,[ebx+0x68]; mov ecx,[ebx+0x64]; call`). Frame-1 entity
  draws see `(near, far) = (2224, 10176)`; fog factor `0x83D`
  verifies as `0x1000000/7952`. Later frame-1 calls use other pairs
  (other subsystems). The former port feed was frontend `fade [1048576,
  1048576]`, which distorted dispatch and wrapped terrain footprint WORDs.
  The source-owned world visibility chain now replaces that input.
- Slew proof: frame-1 entry `5C8=10240/5CC=1280`, `F=10080/H=1280`,
  pitch `0`; post-slew `5C8=10176` reproduces far fade exactly, and
  frame-2 far `10112 = 10176-64` confirms the carry. Bundle
  `.tmp/infestation-ttd/20260928-125818-236-Frame1ViewProbe/`.
- Fade-lever experiment (temporary, reverted — tree clean as
  proof): `WorldFrameInputs.fade = [2224, 10176]` with the port eye
  moves the pin from 4/27 to 31 submitted / 0 unresolvable. Never
  baked in; it quantifies the lever, nothing more.
- `00488380` scope (disassembly): manager visibility/slews/fade/fog/setup-copy
  only — eye/basis are never written there. `0040FD10` is ~16KB
  with 24 callees (virtuals, FPU); its reached native camera subset is now
  owned in [camera runtime](CAMERA_RUNTIME.md). Retail near projection never writes fog
  (`004EB5A0` disassembly), which bounds the 07-near fog stand-in.
- The former F/H and slew-initialization questions are closed statically:
  `0048E7D0` reads manager `+1C/+20`, `0048D9F0` supplies constructor F,
  `0048DC30` derives H from the world header, `004874D0` seeds the retained
  slew, and `0048E2E0` adjusts F from `004875A0` timing feedback. These fields
  are visibility distances, independent from vehicle/focus/terrain Y.
  See [world visibility](WORLD_VISIBILITY.md) for the implemented writers.

### Control-owner decomposition (from the OpDetailProbe census)

- 0x2C (`004EC5C0`, owned): full signed-command mode chunks, selected
  renderer-table `+14` callback and successful wrapping depth-minus-20.
  [Model runtime](MODEL_RUNTIME.md#terrain-projected-vertices-renderer-2c-and-auxiliary-96)
  owns command transport, destination advancement and callback ordering.
- Slope-factor precompute (`0044FDD0`, owned): normalize-or-(1,0,0)
  on the H12 light vector (zero-length guard verified against the
  read-only `0x51F198` zero), chop-scale by 16384, saturated Q31
  ratios with explicit sign fixups. The first-world computation
  reproduces the recorded pair exactly.
- 0048EF30 callers pass clean registers (disassembly overrules the
  empty-paren decompile): the factor inputs are deterministic, and
  the earlier "garbage args" worry is withdrawn.

- 0x2B (`004EC510`, owned): masked `0F` mode through selected table
  `+10`, preserving its unbiased world-projector return. The template uses
  `004EBCD0`; the terrain-installed selected renderer uses `0044FF50`.
  Per-epoch resident terrain remains a service input, independently of
  [table selection](MODEL_RUNTIME.md#renderer-surface-callbacks-and-table-custody).
- 0x2C's terrain callback `0044FFA0` uses the retained slope pair and shared
  `q31_divide` (`0048EF30`, saturated Q31 divide, not RNG). Its `00410F60`
  calls have five genuine stack arguments: source or adjusted X/Z, actual
  process clock, H14 bit40 and signed sampled height. Push sites
  `004500C3/0045016D` and X arguments supersede the old phantom/height-as-X
  interpretation. The recorded first-frame gate was clear at each observed
  hit; that bounded observation is not a runtime false/zero-clock policy.
  [Terrain runtime](TERRAIN_RUNTIME.md#terrain-project-callback-0044ffa0)
  owns the corrected kernel, conditional sine admission and native receipts.
- 0x2D (`004EC670`): custom vertex generation through the draw
  object's callback (`001AFB18`/`00520A44`/`00478950`, slot 2 in
  frame 1). Retail code is uncallable; the next step is a
  per-callback input/output inventory, then pure-function owners.
- 0x22 (`004EC300`): OWNED 2026-09-29 (see gate paragraph
  above for the transcription, tests, RNG threading and
  frame-1 records). Previously noted as fully specified but
  deferred until register-faithful execution reaches it —
  child 63 now reaches all seven ops in frame 1.
- E6 ordinary: [shared direct and clipped ownership](PRIMITIVE_RUNTIME.md#opcode-e6-shared-shade-quads-and-clipped-uv)
  preserves `0049E1A0`/`0049E470`, nullable textures and the shared
  shade DWORD. The clipped wrappers `0049A270`/`0049A930` retain
  their own triangle/quad UV callbacks and per-output failure prefixes.
- 07 (`00495CA0` near / `004961A0` far, both producers owned):
  edge/line expander over two projected vertices (screen XY at
  vertex `+0xC/+0xE`, depth `+0x10`, clip `+0x14`, fog `+0x15`;
  skips when either clip byte has `0x40`. Far `004961F3..004961FD`
  also skips when both fog bytes are `-1`; near reads no fog byte,
  so stale near-slot bytes never drop an edge). The
  72-case receipt runs both handlers unchanged.
  A mean depth below 64 takes `00495E17`: native then uses the two halves
  of its invocation context's address as the scales. Infest01's contexts
  sit at `001AF0B8..001AF940`: `001AF938` at a common root, about `0x228`
  lower per child level, and a few bytes apart per caller. The high WORD
  (26) bounds the band's height. The low WORD (-1736 to -3912) widens X
  past the ±0x3FF window unless the edge is near-horizontal near the
  screen centre. The port keeps this as an unsupported boundary. That stops
  the model's draw, so a production battle that brings a quixle within 64
  units of the camera hides the rest of its model 306 for those frames.
  Retail never stops a model there: each such edge is either rejected by
  the window, as an ordinary out-of-window edge is, or drawn as that band,
  and the model's later primitives still draw. Infest01's intro never takes
  the branch. None of its 9,285 near-handler entries reaches `00495E17`, so
  only the port's production battle, reshaped by the labeled allocation
  approximations, brings a quixle that close.
  Skipping only the edge, or modelling the stack address per invocation
  depth, would each be a new labeled approximation, and the same choice
  applies to `1A`/`1B` below depth 64. The choice was deferred on
  2026-10-08 until decompilation work explains the branch.
  REA's instruction listing of `00495CA0` explains it
  (2026-10-12). The entry stores the context `ECX` at `[ESP+14h]`
  (`00495CA7`), before four pushes move that slot to `[ESP+24h]`. The
  handler reads it back as the context at `00495D09` (material table
  `+48`) and `004960B4` (callback `+4`), so the slot holds the context
  pointer for the whole call. At or above depth 64 the two scales come from
  `0055F22C/30` through `FIDIV`/`FISTP`. Below it, `00495DA5` jumps to
  `00495E17`, which loads `AX = WORD [ESP+24h]` and `DX = WORD [ESP+26h]`:
  the low and high halves of the context pointer itself. No path computes
  a near scale. This is an original defect, an unset local the compiler
  read from the pointer's slot, not a missing branch the port could
  transcribe. The near `1A`/`1B` producers (`004A1D50`, `004A21F0`) have a
  different one of the same kind: below depth 64 (`004A1DC5`, `004A2265`)
  they take X from `WORD [EBP-1Ch]`, which neither producer ever writes,
  and keep Y in `WORD [EBP-1Ah]`, which only their far path writes
  (`004A1E0C`, `004A22AC`). There X is stack residue from earlier calls and
  Y the last far sprite's extent at that frame address. All three remain
  an open choice: skip only the primitive, or reproduce retail's stack
  contents. Frame-1 yield proven by the Edge07SubmitProbe census
  (119 entries, 119 submits, 0 skips between the frame-1
  world-draw entries — it fires per edge, and every firing passes
  admission). Takes the integer square root of the squared edge length
  (`0048EFC0`, owned as `isqrt_48efc0` in
  `inf-formats::native_basis`: exact floor over the positive i32
  range, 0 below it — not an RNG draw), looks the material up
  through the `param_1[0x12]` table (zero falls back to the callback
  at table `+0`), scales the edge by `0055F22C/30` over the mean
  depth with float `ROUND` (halving while above `0x7FFE`), admits
  through `00528BD0` with a `±0x3FF` screen window (a widescreen frame
  moves its right bound, see
  [widescreen](PRESENTATION_POLICY.md#widescreen)), sorts like the
  owned textured path (`0xFF` selector or `0x39` flag bits force the
  `0xC0000000` band), and enqueues a 4-block packet through
  `004A42B0` under the table's own thunk (`0040EF30` near,
  `00411D50` far): 4 XY/depth pairs,
  mode at `[+0x20]` (`| 0x100`), material at `[+0x24]`, effect color
  at `[+0x28]`, `param_1[0x15]` (ctx+0x54) at `[+0x2C]`, fog byte
  pairs at `+0x30/+0x33` and `+0x31/+0x32` on far packets only.
  Owned as `model_primitives::edge::prepare_edge` for both
  dispatches, submitting `EffectQuadNear` (3 blocks) or
  `EffectQuadFar` (4 blocks); no new callback was needed, because
  the thunk passed to the allocator IS `0x411D50`
  (`EffectQuadFar`, virtual `+0xA8` — verified by disassembly), so
  the packet reuses the effect contract with `far_color` at
  `+0x2C` and per-vertex fog bytes on far packets. Corner pairing
  is Ya-with-Xd and Yd-with-Xa at `+0x00/+0x18` (`+0x08/+0x10`
  pair straight), proven by the disassembly WORD stores and then
  differentially by the Edge07PacketProbe oracle, which corrected
  an early straight-pairing transcription. Packet depths keep only
  the low word, sign-extended like the native `movsx` stores; sort key
  and mean stay full DWORDs. Corner halves pack zero-extended
  (`CONCAT22` semantics — sign-extending X would clobber Y).
  Thirteen unit tests pin exact packets (two retail oracle
  firings byte-for-byte), skips, and fail-closed paths, plus a
  labeled fog stand-in for near dispatch (missing bytes count as
  non-FF; far packets still require real bytes). The far pixel
  backend still stops every far callback.
  Disassembly corrections (`objdump` `004961A0..004966E4`, overrule
  the decompile where they differ): `FUN_00490350` is a thiscall
  filling packet `+0x28` with the live effect-color DWORD
  `0055F268` (bulk_clean body is 3 lines; the port already carries
  this as `PrimitiveState.effect_color`) — that open item is
  closed, and the fog bytes pair per corner as listed above.
  Exact store map: `[+0x00]=XYa, [+0x04]=depthA, [+0x08]=XYb,
  [+0x0C]=[+0x14]=depthB, [+0x10]=XYc, [+0x18]=XYd,
  [+0x1C]=depthA`; returns arg `+8` bytes.
  Provenance for the six shared DWORDs: world setup copies
  `param_3[0xE..0x13]` to `0055F22C` (`cheat_settings.c`, at the
  `00490030`/`00490130` world-setup site); `[0xE]/[0xF]` are the
  focal templates `00546F20/24` (`0x200`), so the float scales are
  focal-based perspective (`512*width/depth`). Live frame-1 values
  at the first 07-FAR hit (`417EA3:192F`, EdgeScaleProbe bundle
  `.tmp/infestation-ttd/20260928-100420-433-EdgeScaleProbe/`):
  scales `0x200/0x200`, X/Y clip bounds `0x400/0x300`, centers
  `0x200/0x180` — tier viewport extents (1024x768 in retail) with
  half-extent centers. The port derives all six from its own
  projection/viewport per tier and never bakes these words.
  `[0x10]/[0x11]` have no writer in the decompiled corpus. The H8
  `param_1[0x12]` material table and the
  `param_1[0x15]` packet word are separate mappings still open.
  Material-table resolution (from `sprites.rs` loader offsets):
  `param_1[0x12]` is the Handler-3 sprite pool view — `[0]`
  miss-callback, `[1]` entry-pointer array, `[2]` count — and
  `edge_type` indexes it exactly like `host.texture()` (negative
  and strictly-greater are null; the `== count` one-past read stays
  an explicit port error, never a fabricated null). The entry
  header (4 bytes: mode0/mode1 selectors) plus `0x1c` variants
  matches `SpriteEntry` byte-for-byte: flags u16 at variant `+0x04`
  (native tests the low byte against `0x39`), sort descriptor
  address at variant `+0x00` (native shifts the ADDRESS right one,
  same as `TextureSortIdentity::NativeEntryAddress`), and the
  width pair at entry `+0x14/+0x16` is variant 0's `width`/`height`
  (loader reads them at variant `+0x10/+0x12`). Mode comes from
  `param_1[0x17]` (port `PrimitiveState.mode`), the
  `(param_1[0x10]+4)` call is `material_sorting()`, and projected
  XY/depth/clip/fog come from the owned vertex slots. A total
  lookup miss faults retail (null deref); the port fails closed.
  Still open: only the mean-depth `< 0x40` path, which scales by
  the H8 context pointer halves (no port equivalent;
  address-derived, nondeterministic across runs). `FUN_00490350`
  is closed (thiscall filling packet `+0x28` with live `0055F268`,
  i.e. `PrimitiveState.effect_color`). Packet-reader note:
  `ctx+0x54` passes through at `[+0x2C]`; its reader-side use is
  mapped at reader transcription.
  Dispatch finding: with the current spawn-view camera and
  frontend fade thresholds, the owned dispatch rule runs slots
  474/448/698 as NEAR (retail runs them FAR). 07-NEAR is now
  owned, so 474 submits fully and rasterizes; 448/698 advance
  past 07-near to 0x1C, the next gap. The owned far producer
  takes over whenever native camera/fade inputs flip dispatch
  back to far.
- 0x1C (`004A0DA0`, owned): rotates the saved basis with no
  oriented copy and no raw-zero skip. The raw first word selects
  the axis (signed 1/2, else the `00491290` fallback) and the
  packed second word always resolves first (evaluating callbacks
  when selected; raw zero resolves to immediate zero, which still
  rotates — the exact split from 03's raw skip). Reuses the owned
  `rotate_saved_basis`. Owning it advanced slots 448/698 past
  their 0x07 stops: 448 now stops at 0x2D, 698 at 0x2A.
- 0x2A (`004ED350`, owned): records with no bits in 8..11 advance.
  A child copies 24-byte parent slots through its word table; a
  root stores the published origin and projects its local zero point
  through `+0x1C`, copying either to the next slot for the mirror
  bits ([model runtime](MODEL_RUNTIME.md#runtime-vertices-projection-and-shades)).
- 0xD6 (`0049F1E0`, per-vertex-normal quad): slot 291 stops here;
  outside the FrameOpsProbe frame-1 draw set, so culled-actor
  territory with 0x22 — no owner until the `00479570` cull decides
  what retail actually draws. Its shared D6 consumer (`0044EA60`)
  is now source-owned through renderer `+AC`/`004DBA90`, with native
  allocation UV and flag-selected blend policy. The distinct normal-quad
  producer and instance-selection boundary remain open; see
  [shared production pixels](TERRAIN_RUNTIME.md#shared-production-pixels).
- Static root-program inventories (pinned in
  `world_scene_tests::frame1_entity_slots_pin_their_remaining_opcode_owners`):
  223 now fails at `0xD2` (past the skipped `0x2D`); 474 submits
  fully (0x2B/0x2C/0x07 all owned); 448 stops at 0x2D and 698 at
  0x2A past owned 07-near/0x1C; 552 SUBMITS fully (root
  0xE6/0xDE/0xEA, child-553 E4, grandchild-554 `0x2A` copies
  owned, `0x2D` nibble-2 skipped, `0xDA` faces skipped on
  unprojected clips, E6/E4/DE rasterize — pinned in roster);
  390 submits (cull-trap, see comparison);
  62's root is fully owned (its 0x22 lives in a
  child). All remaining handlers size at 300+ instructions each, so
  no cheap pixel path is left.

## Phase-7 map screen

The in-world computer's "Map and Objectives" (`00415170`) requests phase 7,
as does the gameplay screen's `+40` handler `00442BC0` (unless its argument
is zero, the network flag is set or the world-object field `+0x250` reads
`0x16`). The gameplay screen's reuse rule `00440B60` refuses phase 7, so the
transition destroys it and builds the map; the map's return requests phase 4,
whose screen is built afresh. In between, the session tick runs the player
callback `00438950`, not the world update: the world stands still under the
map.

| Address | Role |
|---|---|
| `00450FC0` | Factory, constructs through `00451020` |
| `00451020` | Constructor over base `00437F90` (own frame layout) |
| `005255C8` | Screen vtable (mapped from the image: destructor `004511D0`, tick `00451F30`, draw `00451250`, shared presentation `0040D1D0`, standard select/deselect, class query `004511B0` returning 7, reuse `004511C0` accepting only phase 7, held dispatcher `00439470`, no held callbacks — every slot from `+24` is the empty `00401310`) |
| `0046B030` | The first player's `+D4` resolved through `0047AD90`: the vehicle |

The base constructor zeroes the age `+C0`. The constructor then sets the
zoom `+F8` to 1, clears the held bits `+EC`, publishes the vtable, resolves
the vehicle (`00488AA0`, then `0046B030`), installs one inline array set
(slot 0 `00525120`, slot 2 `00525238`, slots 6/7 `00525390`, slots 8/9
`00525498`, analog slot 2 snapshotting the shared cell `00533398` and slots
8/9 `005333E8`) and selects its context (`004947E0`). The map's centre
`+F0/+F2` starts at the vehicle's X and Z WORDs (`+32/+36`), or zero without
one, and the velocities `+F4/+F6` at zero. Every analog DWORD is written —
unlike the briefing constructor, there is no unwritten slot. The destructor
(`004511F0`) releases the binding sets (`00439D50`) and the base
(`00438080`).

Input tables decode statically with the generic digital parser (retail
corpus test pins counts 13/16/12/12, shared pairs, row-identical slots 6/7
vs 8/9, and the exact 9-entry descriptor set). The callbacks take the press
value in EDX:

| Callback | Keys | Press / release |
|---|---|---|
| `004520A0` | Esc, Space, Tab, Return, M, the joystick buttons | Press only: `00438380(4)` on screen `+E4`, back to gameplay |
| `004520C0`, `004520F0` | Up, Down | Hold bits 2 and 4 of `+EC` |
| `00452120`, `00452150` | Left, Right | Hold bits 8 and 0x10 |
| `00452180`, `004521B0` | Insert, Delete | Hold bits 0x40 and 0x80 |

The shared Alt+Esc/Alt+Enter pair completes the keyboard; there is no noop
record. The census merges these small bodies into their neighbours; the
table reads them as instructions.

Tick `00451F30(delta, elapsed)` adds the raw elapsed microseconds to `+C0`
(no base input `004392B0`, so no thousandths), takes `step = delta >> 7`
(arithmetic) and steers the WORD velocities: bit 2 `+F6 +=`, 4 `+F6 -=`,
8 `+F4 -=`, 0x10 `+F4 +=`. It moves `+F0 += +F4`, then damps `+F4` to 7/8
(C division, toward zero), then likewise `+F2` and `+F6`. Bit 0x40 raises
the zoom by 2 while below 0x80, bit 0x80 lowers it by 2 while above 0, so it
runs over the odd values from -1 to 129. Last, the screen's two analog
consumers (`004F0140`, each read twice) push `+F4` (`+CC`) and `+F6`
(`+D0`) by the step when beyond ±3000.

Draw `00451250` runs the preparation `00439270`, reads the surface's `W`
and `H` and resets the batch (`004A4400`). With `C = 3W/8` and
`R = 11H/24`:

1. It clips to the right three quarters (`x ≥ W − 2C`) and draws the map
   centred at (`6W/10`, `5H/10`) through the briefing's `00471F70` on
   `G+218`: tiles of `(zoom + 0x80)·C·2/128` by `(zoom + 0x80)·R·2/128`
   pixels, the centre `+F0/+F2`, the age `+C0/1000` and `0046BDC0(0x13)` —
   whether the player owns the Resource scanner. `00471F70` takes separate
   X and Y tile sizes and ignores its sixth argument (1 here).
2. It restores the whole surface's clip and frames the map with four
   renderer `+9C` quads of H3 `0x29E` at depth `0x6000`, each a quadrant of
   `4C/3` by `4R/3` about the map's centre listing the centre last and its
   outer corner second. The sprite darkens by inverse source colour and is
   a quarter vignette, so the map shows in a disc that fades to black.
3. It draws the legend's three blips through `004736E0` at (`52W/100`,
   `84H/100`, `87H/100`, `90H/100`) in H7 `0x59`, `0x56` and `0x57` — the
   marker palette's other, allied and enemy colours — scaled by H6 `0x0E`.
   They pulse together: with `p = −((+C0/10) & 0xFFFF) >> 8` (−256..0, one
   cycle every 0.655 s), the half size is `H·p/16384` and each colour byte
   is the style byte times `p` over 256 times the material's display channel
   over 256, signed, so the bytes wrap.
4. In H4 font 0 and style 5 with descriptor flags 2 (revealed from
   `+C0/1000` ms): "Map of" (H0 `0x11A`) and the world's name (`0x36A`) as
   "%s %s", centred over the whole width 5% down; then the four objectives
   `0x380..0x383` at `W/32`, `C` wide, from `H/8`, each starting `3H/32`
   below where the last one ended (`004A3B20` publishes its end position).
   The loop stops at a NULL text, which no world has: a world without four
   objectives holds empty strings. The mission clock `G+1E4` follows as
   "%02d:%02d:%02d" at (`50W/64`, `5H/100`) through `004A3AB0`.
5. The legend's text: "Key building" (`0x11C`, style `0x0E`), "Allied unit"
   (`0x11D`, style `0x2A`) and "Enemy unit" (`0x11E`, style `0x28`) at
   `55W/100`, `85H/100`, `88H/100` and `91H/100`.

The map draws nothing else of the world. In phase 7 the marker pass
`00472D30` reaches two arms the briefing never does:

- The vehicle (the only actor whose `+64` answers: every other actor-like
  vtable in the image has `00401300` there) trails its marker while alive
  (`+C4 ≥ 3`) outside phase 3 (`004730D7..0047323B`). With
  `a = ftol(basis +48 · 16)` and `b = ftol(basis +50 · −16)` as signed
  WORDs, eight pairs of the marker's blip step out from the marker along
  (`−(a/2 + b)`, `a − b/2`) and (`a/2 − b`, `b/2 + a`), each position an
  eighth of its sum.
- With the Resource scanner, a pickup spawner draws `004736E0` at its
  marker position with half size twice its count `+139`, in H6 `0x22`,
  `0x23` or `0x24` by its kind `+138` 1..3. Another kind keeps the previous
  spawner's style (stack `[esp+18]`); a pass's first such spawner reads
  unwritten stack, which the port reports as a boundary.

The [port](../../crates/inf-game/src/map_screen.rs) builds the screen
(`MapScreen`): its constructor, callbacks, tick and draw, the session's
admission of 4→7, 6→7 and 7→4, and the software draw over the live world.
On a widescreen frame it draws on the centred 4:3 canvas, as the briefing
does: the tiles scale with the width and the height apart. A failed draw
keeps what it reached and is reported once. A Level 1 render test pins the
whole frame; a session test opens the map from gameplay, zooms it and
returns.

## Phase-8 results screen

Results constructor, input, Tick/Draw and evidence bounds now belong to
[results runtime](RESULTS_RUNTIME.md#phase-8-results-screen).

## Phase-5 in-world menu screen

The gameplay tick reaches phase 5 through `00438380(5)`; the phase-5 screen
is a menu over the live world, reusing the gameplay tick worker:

| Address | Role |
|---|---|
| `00440D20` | Factory, constructs through `00443190` |
| `00443190` | Constructor over inner `004408B0` plus base `00437F90` |
| `005233D8` | Screen vtable (mapped from the image: destructor `00443340`, tick `00440D80`, draw `004437A0`, presentation `00440930`, standard select/deselect, class query `00443320` returning 6, reuse `00443330` accepting only phase 5, held dispatcher `00439470`, held slots `00443140/00443100/00443110/00443120/00443130/004430F0`: menu codes and Escape, below) |
| `00443150` | Menu row action: on command 4 only, invokes virtual `+0x64` on the callback object; all other commands return |

The constructor mirrors the gameplay shape with one enumeration: base,
vtable publish, inline twelve-slot arrays (slot 0 `00522B48`, slot 2
`00534418`, slots 6/7 `00522C28`, slots 8/9 `00522CE0`, analog snapshots
from the shared cells), global-then-struct attach, context select, then the
player lookup `00488AA0` (plus `0x88`) into a `0x60`-byte `+0xFC` allocation
that runs menu constructor `004433C0` over the menu definition below, and
menu attach `004137D0`. Analog decoding waits on the same calibration owner
as the gameplay screen.

The in-world menu `00535348` (4 rows, initial selection 1) parses through
the shared generic menu decoder: a header row (flags 9, H0 `0x102`, no
action) plus three selectable rows drawing H0 `0x104/0x9E/0x106`. Row 1
runs `00443150`; rows 2 and 3 reuse the generic child-push `00453BE0` with
pushed definitions `00539390` and `00535280`. Two layouts: a 1-row header at
`(50,100,900,100)` and a 3-row body at `(80,200,900,400)`, spacing 80.
Pushed-submenu contents stay open. Row-1 target semantics close
statically: `00443150` calls virtual `+0x64` on the callback object for
command 4 only, and phase-5 vtable `005233D8` slot `+0x64` is `004430E0`,
which jumps to `004133C0` through the screen `+FC` object — so row 1 is
command-4 → `+0x64` → `004133C0`. The phase-5 vtable shares `+0x54`
(`00442FD0`, nonzero-arg request phase 2) and `+0x58` (`00440940`, `+F8`
setter) with gameplay. The vtable ends at `+0x64`: `00523440` begins the
in-world menu's own vtable, whose destructor `004433E0` runs `00443400`,
the request for the phase the menu stored at `+5C` (6 as 4). The held
slots send codes 4, 0, 1, 2 and 3 (`00443140`, `00443100`..`00443130`) to
`00413950` on the screen's message (`+FC`), and `004430F0` is `004133E0`,
the Escape that pops a menu; see
[the computer and the in-world menu](PLAYER_MESSAGES.md#the-computer-and-the-in-world-menu).

## Screen tick detail

Tick `00442B50` (thiscall, `ret 8`), recovered from retail disassembly:

- Device queries `004399F0`/`00439A20` count the live input adapters:
  those present in the adapter cache `005474B0` whose status consumer
  (`0054755C`, built by `00439880` over descriptors `005475B8`) is not
  held (`004F0030`). `004399F0` counts all twelve, `00439A20` the types
  1, 2, 8 and 9. The process context's tables at `005222B0` bind a status
  input (code `0x1F`) only for types 1 and 6 to 9; the keyboard (type 0)
  has none, so an attached keyboard always counts.
- When the first count is zero it requests phase 5 (`00438380(5)`, the
  in-world menu): the controller pause, never reached with a keyboard.
  Otherwise, with no live adapter of types 1, 2, 8 or 9 it writes settings
  `+8` (the input mode) to 0, else latches it to 1 when zero. A keyboard
  alone thus keeps `+8` at 0 from the second gameplay tick on; the world
  update runs before the screen tick, so the first tick still reads a
  saved `+8`. The port runs both gates over its adapters, the keyboard and
  the joystick
  ([`gameplay_device_gates`](../../crates/inf-game/src/gameplay_screen.rs)):
  phase 5 only if neither is attached; without a joystick `+8` back to 0
  each tick, with one Disabled (0) turned to Analogue (1).
- Either way it runs the inner worker `00440D80` (screen plus the
  scheduler timing pair), which forwards both arguments to base input
  `004392B0` with no further decision (fully owned: two pushes, one call,
  `ret 8`). Its screen's analog consumers `+CC/+D0` read the menu axes of
  the shared cells (`00533398`, `005333E8`), which only a joystick or pad
  moves: past ±2500 they press held bits 2..0x10, repeated each 100 ms
  after the first 250, and the vtable `00523370`'s slots `+28..+34`
  (`00443760`, `00443770`, `00443780`, `00443790`, each through
  `00443740`) send codes 0..3 to the first player's message (`00413950`),
  so the stick steps a shown computer or shop; `+24` and `+38` are RETs.
  The port runs it in [`GameplayScreen::tick`](../../crates/inf-game/src/gameplay_screen.rs),
  except while the screen serves phase 5, when retail has no phase-4
  screen; back in phase 4 its base input restarts as a new screen's does.
- The phase-5 screen's draw `004437A0` is the gameplay draw `00442080`,
  then, when `004399F0` counts none, H0 `0x10C` ("Paused - please insert
  controller") centred on the surface in font 0 and style 0 (`004A3AB0`);
  with a keyboard it never shows, so the port's reuse of the gameplay
  screen for phase 5 draws the same.
- The gameplay vtable `00523370` maps slots `+0x40..+0x64` (image read,
  each confirmed by disassembly entry shape; SEH-frame tails stay open
  where noted):

| Slot | Address | Owned behavior |
|---|---|---|
| `+0x40` | `00442BC0` | M: nonzero arg requests phase 7 (map) unless the world-object field `+0x250` is `0x16` or the network flag `0055F448` is set |
| `+0x44` | `00442BF0` | F12: nonzero arg sets settings `+0x68` (First Person) to 0 when it is above 0, else 1, via `00463AB0`, then runs `0040F740` (below) |
| `+0x48` | `00442C30` | U: nonzero arg toggles settings `+0x60` (HUD) boolean (`sete`) via `00463AB0` |
| `+0x4C` | `00442FF0` | Alt+N: nonzero arg calls `00487560(edx,1,0)` with `edx = (world +0x250 mod 33)+1`; no phase request |
| `+0x50` | `00443020` | Alt+F3: nonzero arg toggles settings `+0x70` 0/1 via `00463AB0` plus terrain-spacing `004502C0` |
| `+0x54` | `00442FD0` | Shift+Esc: nonzero arg requests phase 2 via screen `+E4` session (shared with the phase-5 vtable; the live 5→2 exit caller `00442FE5` is its return) |
| `+0x58` | `00440940` | Backspace: sets screen `+F8` to `(arg != 0)` (shared with the phase-5 vtable); the gameplay draw `00442080` lays out a player list while it is set |
| `+0x5C` | `00442CF0` | T: SEH frame; head gates on nonzero arg and the network flag (tail open) |
| `+0x60` | `00442EE0` | Y: SEH frame; head gates on the network flag (tail open) |
| `+0x64` | `00443070` | P, and Esc without Shift: menu/attract handler (below); the live 4→5 entry caller `004430DC` is its `00438380(5)` return |

The keyboard table `00534290` binds those keys to wrappers
(`004435C0..00443720`) that pass the key state and the input clock to the
current screen's slot (`004391C0`'s `+18`); Alt+F3 (`004436B0`) and Alt+N
(`00443720`) call the screen's own `+50` and `+4C`. The port runs P/Esc,
F12, U, M, Shift+Esc, Alt+N (the next world, which marks the current one
complete), Alt+F3 (terrain detail through `004502C0`, prepared at once),
and T and Y, which do nothing without a network session. Backspace's
player list stays open. Infest04 presses F12 four times, on Artoria and Helniazine, and
none of the four recordings presses M, U, Backspace, T, Y, Shift+Esc,
Alt+F3 or Alt+N (key census).

`0040F740`, the world refresh, finds the camera through `00478E00`
(G+8; with no world or no camera it returns), gives it the First Person
setting's low byte as its view mode (`0040F800`, which offers the mode to
the camera's target's `+80`) and then the Camera Style's parameter tables:
`+154/+158/+178/+17C` = `00546990/80/D8/C8` when settings `+6C` is 1,
else `00546960/70/A8/B8`. Each of Infest04's presses toggled the setting
and moved the camera's `+188` between 3 and 2 this way. The
[port](../../crates/inf-game/src/object_runtime/player.rs) runs it from
F12 and from the in-world Options' rows that call it through `00463B50`
(Detail Level, First Person, Camera Style and Dynamic Lighting); with no
camera handle, where `00478E00` would build the camera, it logs that the
camera takes the settings when built.

`00443070` (thiscall, plain `ret`), in order: when the attract flag
`0055F14C` is set, clear it and request phase 2 (fall-through, no
return); when the world-object field `+0x250` is `0x16`, call
`00487560(0x12,1,0)` and return; on a live player (`00488AA0` nonzero
with byte `+0x88` set) call `004133C0` and return; otherwise request
phase 5. This is the Escape-class key path the live sample exercised.
The [`gameplay_menu`](../../crates/inf-game/src/gameplay_menu.rs)
owner implements these four gates (`00443070`, `00442FD0`, `00442BC0`,
`00440940`) as pure decisions over explicit inputs; heap reads, the
`00487560`/`004133C0` calls and `00438380` submission stay with the
future world/session host.

## World-tick phase map

Each scheduler tick in gameplay phases runs the session callback `004388D0`
around the world update `004875A0`, in per-phase `004395A0`/`004395C0`
brackets (clock sample `004A4890` plus error-scope destroy `004923C0`; the
delta-table detail is open):

- `004388D0(delta, elapsed)` forwards both timing arguments to `004875A0`,
  then folds the capped delta into the signed wrapping remainder `+0x658`,
  counting whole seconds past 1,000,000 into `+0x248` (exactly one second
  stays in the remainder). This accounting already has a port owner; the
  update itself does not.
- `004875A0` phases, in order:
  1. Owner bytes: sync `+5C3` from `+0x171`; when `+5C6` is set, OR the
     settings `+84` cheat word with `0x40`, the campaign's end bonus
     ([cheats](PROFILE_RUNTIME.md#cheats)).
  2. Timing ring: an 8-entry uncapped/capped accumulator (DWORD indexes
     `0x161..0x16C`) feeding the unsigned average at index `0x16B`, load
     factor `DAT_0055F154` and reciprocal `DAT_0055F158`. Normal average below
     34,464 microseconds yields factor 65,536; otherwise the factor is
     `100000 - min(average,100000)`. The exceptional second-argument-above-delta
     arm seeds all eight entries and sets factor zero. Particle birth use
     belongs to the [intro retirement boundary](INTRO_SEQUENCE.md#retirement-particle-admission-boundary).
  3. Clock `00439560`, then `0048E2E0(delta)`, `00436700`, `00478F20(delta)`
     and `004790C0(delta)` (individual semantics open). `0048E2E0` first
     adds the delta's whole milliseconds to the mission time `G+1E8`
     (`0048E312..0048E335`; 125 per 125 ms tick in Infest02). Its mission
     countdown `+24` remains open. It then advances the world tint's
     schedule ([below](#the-world-tints-schedule)).
  4. Camera query `00478E00`: when nonzero, `0040FD10(delta)`.
  5. Particle pending-list transfer `00464DA0`, then active particle update
     `00464B30(delta)`; births during the actor walk update in this same tick.
  6. When the screen link at `+0x18` is null, request phase 2 (return to
     the frontend).
  7. Network branch (`0055F448` set): settings-derived selector assembly
     with change detection (`0045C320`, `00492E50`), entity-list walk
     `0046B030`, conditional dispose drain `00479840` (gated by `0045C940`),
     `0045C5A0`, `00492830`, return-to-frontend request on
     `0055F3A4/3A8`, and conditional pickup grants (`0046B2B0` for
     `0x1A/0x1D/0x1C` gated by `0046BDC0`).
  8. Attract branch (`0055F14C` set with a camera): demo placement
     (`0047AD90`, `004400D0`, `004404D0`, `004825D0`, camera retarget
     `00410E70`, `00410EE0`) and a 30-second timer driving `00488C90`,
     `00487560` and a phase-8 request.
  9. Tail (`00487B2E`, reached from every path): `00487B90`, the music
     service that loops the world's track ([audio](AUDIO_RUNTIME.md)),
     settings fetch, `004F4780`, `004949A0`.
- Notably, the `0046B030` handle-list walk, the dispose drain and the
  pickup grants appear only inside the network branch in this function;
  the offline actor/collision path updates through `00478F20`/`004790C0`;
  `00464B30` is the separate particle pass. Complete internal actor routing
  remains open. No cadence, ordering beyond this map,
  or per-entity semantics is established here.

### The world tint's schedule

The world's light colour (the tint `005474E0`, which lights terrain and
models) is not fixed. The load copies H6 `0x37` into `0053DED0` and
publishes it (`0048DDE7`). After the countdown, each tick of `0048E2E0`
(`0048E521..0048E6BA`) runs a schedule from the H14 header: `+1C` is a
period in seconds, `+30` a count and `+34` a relocated list of that many H6
styles.

1. While mission `+13` is set, its timer `+2C` gains the tick's whole
   milliseconds (the same value `G+1E8` gains). The world's constructor
   sets `+13` (`0048DA54`) and the load clears it for H14 flag `0x1000`
   (`apply_world_header`); a target with `+162` sets it as it dies
   ([generators](FAMILY40D800_RUNTIME.md#selector-39-course-target)).
2. The period is `+1C * 1000`. Style `t / period` blends toward the next,
   both clamped to the last, by `((t mod period) << 7) / (period >> 8)`:
   each channel moves `(to - from) * fraction >> 15`, added as a byte. A
   short period's fraction passes 2^15 near its end (1 s: up to 42,624),
   so the blend overshoots and wraps for that tick. The high byte keeps
   `0053DED0`'s.
3. Once `t` reaches `period * count` the last style is copied whole.
4. Infrared vision (world `+5C3`) replaces the colour with `0055F15C`,
   `0053C678` and `0055F15D`: two BSS bytes no instruction writes and 127,
   a green ([infrared vision](#infrared-vision)).
5. `0043A370` publishes `0053DED0` to `005474E0`.

The schedules, by overlay (world plus 10):

| Overlay | Flag `0x1000` | Period | Styles |
|---|---|---|---|
| 21 (Artoria), 40 | yes | 1 s | `0x37`, `0x38` |
| 11 | no | 400 s | `0x37`..`0x3A` |
| 15 | no | 15 s | `0x37`, `0x38` five times |
| 25 | no | 90 s | `0x37`, `0x38` five times |
| 28 (Level 1) | no | 90 s | `0x37`, `0x38`, `0x37`, `0x38`, `0x37` |
| the rest | | | `0x37` alone |

A completed course target with `+162` sets mission `+13` again from its
after-load on every later visit (`0043B720`, after the load's own clear
at `0048DD6A`), so Artoria stays dark when the player comes back: the
user's save, back from Kankutan, arrives dark in retail's lab run (frame
mean (28, 15, 4)). The port recorded that write without making it, so the
return was lit; it now sets the byte there.

So on Artoria the four topside generators' deaths start a one-second fade
from (234, 128, 46) to (50, 25, 10), which the world keeps: retail's own
words at `005474E0` (`artoria-generators-dump.json`) are `002E80EA` before,
`00102C54` 625 ms in and `000A1932` after. The lab's
`artoria-generators-complete.json`, which kills them through the Cheats
Menu's Complete Level, matched every logged event before the port had the
schedule, yet its frames went near black only in retail; with
[world_tint.rs](../../crates/inf-game/src/world_tint.rs) the port's
frames match retail's to within a level per channel. Level 1 and the other
scheduled worlds swing their light from the start.

### Infrared vision

L with Infrared vision (upgrade `0x3D`) owned requests it: the vehicle
update sets the world manager's `+5C4` to `!+5C3` (`00417922..00417932`),
and the next tick's head copies `+5C4` into `+5C3`
(`004875BE..004875CE`). With it owned, L never reaches the Headlights
(`0x14`, `+208`), which keep whatever they were.

**Port fix** (2026-10-23, in every renderer): with both owned, L
steps through the lamps, off, the headlights, infrared (the headlights
off), off again (`CameraGlobalState::step_lamps`). In the remaster's game
(Vulkan single player, not a network game) a press between infrared and
off turns the thermal camera's palette from white hot to ironbow (a
non-native flag the draw reads); the extra step moves the lamp bytes, so it
follows the session's game, not the renderer drawing it. With one of them
owned, L toggles it as retail's does. Both bytes belong to the gameplay object, which the
session constructs once (`00487C5F/65` clear them), so they outlive a
world: each load clears `+5C4` (`0048DD5B`), and `+5C3` keeps the last
world's value until the new world's first tick, through its briefing.

While `+5C4` is set, every frame clears to black (`00438120`). While
`+5C3` is set:

- the world tint is (0, 127, 0) ([above](#the-world-tints-schedule));
- the predraw's far colour is H7 `0x20` instead of `0x74` (`00488480`);
- the sky is not drawn (`00440DA8`);
- before the light grid's anchor moves, the first player's unit
  (`00488AA0`, `0046AE60`) deposits a light at its X and Z, radius
  `0xC18`, colour (0, 127, 0) (`00488072..004880C2`). Retail reads NULL
  without a unit; the port skips the light and logs it once;
- every deposit's colour is scaled by (0, 127, 0) / 255 (`0045B29E`), so
  the player's light lands as (0, 63, 0);
- the terrain's far colour is packed black (`0044AA97`), its far strips
  take H7 `0x20` instead of `0x73` (`0044B14B`), and its pane colour is the
  three bytes halved, masked to five bits and shifted into the display
  layout (`0044BD7D..0044BDDE`), in the world and the briefing's pane
  alike.

So the whole screen turns green. The lab's `artoria-infrared.json` presses
L on Artoria with dynamic lighting on, and again later: the port's frames
match retail's through both toggles (frame means (11, 169, 10) against
retail's (11, 171, 10)), the only pixels that differ being the frame-edge
lines that also differ before L, and every logged event matches. Under the
Vulkan renderer the same state shows through a thermal camera instead
([thermal vision](PIXEL_RUNTIME.md#modern-mode-the-traced-world)); the game
runs exactly as here.
