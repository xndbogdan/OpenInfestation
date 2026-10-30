# Native selector-16 camera runtime

This authority owns camera target selection, destruction, smoothing, and pose
callback `0040FD10`.
The [shared target owner](TARGET_RUNTIME.md) owns target construction, refresh,
queries, cloning and referent custody.
The selector-88 sequence and selector-90 moving targets are separate owners in
[INTRO_SEQUENCE.md](INTRO_SEQUENCE.md) and [ACTOR_RUNTIME.md](ACTOR_RUNTIME.md).
Evidence is the retail executable's static disassembly and
bulk/game_logic.c; no captured camera coordinates define
runtime behavior. Unit fixtures establish the implemented arithmetic and
ordering, and do not constitute retail scene acceptance.

## State and target custody

[selector16.rs](../../crates/inf-game/src/object_runtime/actor/selector16.rs)
keeps the constructor's written and unwritten fields distinct. The two camera
parameter blocks at `+138/+15C` contain signed WORD velocity and wrapping WORD
position vectors. Their current and desired vectors remain unwritten until a
native writer reaches them. Byte `+187` requests a snap; byte `+185` controls
the smoothing routine's deadzone/far snap and raw H12 cell lift. It does not bypass
the later interpolated terrain/water clamp.

The camera uses the shared native type-2 actor-follow resource. A null-point
constructor resolves the actor and writes both +08 and +14 when live; an
explicit-point constructor writes only +14 without resolving the actor.
Velocity remains unwritten until refresh. The
[target authority](TARGET_RUNTIME.md#actor-follow-construction-and-refresh)
owns the branch-specific writes, settings gate, full DWORD lifecycle return,
live basis query and stale-handle preservation.

`00410E70` first applies view mode zero to a present old primary, deletes that
target, then publishes the new primary and reapplies the First Person setting.
Its errors retain those reached phases, including a new target published before
a later mode error. `0040F710` only deletes the old secondary and publishes the
new one. Primary-to-secondary copying uses the shared explicit-point clone; a
true NULL primary clears the secondary.
`00410EE0` applies
`00410EC0` to both smoothing blocks: an unsigned input greater than one is
clamped to one. Mode-one local offset `+194` remains explicitly unwritten
until its separate writer is owned. Fixed-point target subclasses are not
represented as actor-follow targets.

The complete camera destructor `0040F6A0..0040F70A` reads and deletes primary
`+18C` at `0040F6C3/0040F6D9`, then secondary `+190` at
`0040F6DB/0040F6E9`, before calling shared actor destruction `0047AA60` at
`0040F6F5`. Neither child pointer is NULL-stored. The admitted type-2 table
`00528038+00` selects `00482840`: it calls the base vtable restore `00407400`
and conditionally frees its own allocation. It does not resolve or delete the
referent, read focus/velocity, or consume settings. Empty pointers bypass the
child calls and still reach shared handle, actor FIFO and common-node removal.

[destruction.rs](../../crates/inf-game/src/object_runtime/actor/selector16/destruction.rs)
is consumed by `ObjectArena::destroy` before that shared tail. The private
camera custody fields distinguish a constructor NULL (`Published` with no
target) from a released present allocation (`Released` with no owned target).
A returned retired snapshot owns neither child; this is logical allocation
release, not a native NULL store or physical pointer/free claim. A reached
released slot reports `ReleasedCameraTargetSlot`. An error at secondary `+190`
retains an earlier primary release and leaves the camera's shared ownership
intact; repeating the whole destructor cannot silently repair that prefix.

Primary replacement, view-mode changes, cloning and pose check this custody at
their reached primary reads. Secondary replacement and the alternate generic
pose branch check secondary custody only when that pointer is reached. Normal
new or NULL publication establishes `Published` custody. This preserves the
ordinary mode policies and late errors without treating a freed pointer as a
constructor NULL. [Owning tests](../../crates/inf-game/src/object_runtime/actor/selector16/destruction/tests.rs)
use genuine constructors and registry/list owners, plus explicitly supplied
interrupted custody; they cover child order, referent preservation, setter
prefixes and lazy pose reads. Arbitrary target subclasses, raw aliases, native
free failure/SEH and physical heap or pointer parity remain outside this domain.

`ObjectArena::world_camera` owns on-demand `00478E00` acquisition. A resolved
G+74 handle returns that actor's virtual +60 result, including null, before
reading process state. A resolution miss reaches `00478E3D`'s current
`0054E894` read before the H13 factory, even if the factory later rejects the
request. `CameraFactoryInputs.process` borrows the actual retained owner;
missing owner/value fails before construction, without a registry-null default.
The captured argument is forwarded through `00460310/00460120`, but their
camera mode-zero branch bypasses its consumer. No unused numeric sidecar is
added to construction. Loader and intro admission carry the same borrow;
helper-only admission and existing-camera reuse retain their lazy bypass.

## Smoothing arithmetic and units

[parameters.rs](../../crates/inf-game/src/object_runtime/actor/selector16/parameters.rs)
implements `0040F190/0040F1C0`. Tick time is microseconds; the routine first
uses unsigned `delta >> 5`, and motion products divide by 32768 with signed
truncation toward zero. Coordinate subtraction, softened differences,
velocity changes, and final positions all narrow or wrap at their native
WORD stores. Squared lengths accumulate with signed DWORD wrap before
`0048EFC0`; square-root results are narrowed to signed WORD where the retail
consumer does so. A native divide fault becomes a typed feature error.

The CRT initializers `0040F000/030/060/090/C0/F0/0040F120/150` call
`0040EFC0(acceleration, far threshold, speed coefficient, deadzone)`. The
third argument is squared before storage at table `+08`.

| Table | Acceleration | Far threshold | Speed coefficient | Deadzone |
| --- | ---: | ---: | ---: | ---: |
| `00546960` | 5000 | 3000 | 40 | 32 |
| `00546970` | 12000 | 3000 | 80 | 32 |
| `00546980` | 12000 | 3000 | 80 | 48 |
| `00546990` | 3500 | 4000 | 20 | 48 |
| `005469A8` | 6000 | 3000 | 80 | 32 |
| `005469B8` | 12000 | 3000 | 120 | 32 |
| `005469C8` | 8000 | 3500 | 600 | 48 |
| `005469D8` | 4000 | 3500 | 40 | 48 |

Nonzero snap with nonzero `+185` initializes current vectors directly from
the desired vectors. Otherwise, the routine requires written current state.
With `+185` nonzero, crossing the far threshold strictly snaps both vectors;
equality does not. With `+185` zero, deadzone/far snap are skipped, speed is
capped at 2000, and signed raw H12 cell floor plus 512 raises a low Y halfway.
The velocity and position writes at `0040F492..505` precede the H12 query at
`0040F511`, so an unavailable query preserves those already reached writes.

## Pose order and native policies

The process cached XYZ at `005469E8/EA/EC` starts from six file-backed zero
bytes in the authenticated executable. `CameraGlobalState::INITIAL` retains
`Some([0;3])`; a failed pose keeps the last published value. Selector86 distance
reads this cache, not a newly acquired camera actor. Byte guards distinguish
this authored initial state from an invented heap-carry value.

The camera's separately selected virtual `+24=0047AD30` is the shared unsigned
WORD `+C8` countdown: subtract unsigned `delta >> 5`, saturating to zero at or
above the remainder. Production dispatch runs that callback before the later
dedicated camera-pose phase. Its distinct `+3C=0047EBF0` remains a named boundary.
The complete original countdown body is covered by 54 native cases in
[simple_object_updates_native.json](../../crates/inf-game/tests/fixtures/simple_object_updates_native.json).

[pose.rs](../../crates/inf-game/src/object_runtime/actor/selector16/pose.rs)
uses one data-bearing `CameraPoseTickRequest`: time, settings, process up
vector, process shake switch/state, shared RNG, original sine table math,
and a required host. Target refresh precedes target selection, desired
vectors, smoothing, terrain clamp, water mode changes, orientation, shake,
and final cached position publication. The actor pointer resolved before
`0040FB40` remains the pointer used later in that tick, even if reselection
replaces the primary target. Byte `+186` adds tick time to `+19C` until the
valid target gate succeeds, or the signed timer exceeds 5,000,000.

The target policies are implemented in
[targets.rs](../../crates/inf-game/src/object_runtime/actor/selector16/pose/targets.rs):
mode zero uses the camera actor's position/velocity as eye and target as
look; mode one transforms the signed local WORD offset through the target
basis; mode three uses the derived camera consumer if present. Shared
actors use their live basis/position/velocity: `+184=0` tracks behind a
horizontal forward vector by `max(-400 - projected_speed/5, -1000)`, adds
WORD `+74 + 150` on Y, and looks 400 units ahead. `+184!=0` places the eye
at the actor and refreshes a secondary look target, or looks 8192 units
along its full forward vector. Mode four delegates to the derived special
view consumer and retains the camera basis. Mode two and out-of-range modes
rearm snap every tick; derived eyes add 50 along target up with negative Y
offset clamped to zero, and look 400 along horizontal forward.

### Selector-2 chase targets

[selector2/camera.rs](../../crates/inf-game/src/object_runtime/actor/derived/selector2/camera.rs)
owns `004218E0` through a mutable `CameraDerivedTargetRequest`. Genuine modes
are named `Chase` and `SpecialView`; native stored mode DWORDs keep their own
meaning. A returned pair of complete vectors cannot preserve the eye-before-look
failure prefix, so the request lends the actual eye/look parameter blocks and
camera basis. The pose owner republishes reached parameter/basis writes on both
success and error. `NativeCameraServices` retains only player attachment.

Chase first captures current process NULL at `004218E4`. Full vehicle DWORD
`+1B4` equality selects base stand-off -400, otherwise -1000. `00421D30` projects
signed velocity through basis Z in `(Vy*Zy + Vz*Zz) + Vx*Zx` order and truncates
through `__ftol`; its arguments are signed DWORDs, while this caller sign-extends
the three vehicle WORDs. The native negative-fifth multiply implements signed
division toward zero. Wrapped addition precedes the signed -1000 clamp, followed
by signed-WORD narrowing. Horizontal X/Z quantize through 4096 into DWORDs;
zero length selects (1,0,0). The initial normalized components spill to f32.
Eye Y adds 150 to the vehicle position. `00421B17` writes desired eye position,
then fresh vehicle velocity, before reading `+1E8`, `+98` or `+148`.

Only exact `+1E8 == 1/2` reads signed DWORD `+98` and divides it by four toward
zero. A nonzero quarter rotates a local basis copy through `00491530`; the
vehicle's basis and rotation counter remain unchanged. This second horizontal
normalizer retains X until the final 400-unit multiply, while Y/Z spill to f32.
The 24/7 quantized direction distinguishes default-look X383 from retained X384
without changing the eye. The earlier 200-unit scratch words are overwritten
and do not define the final look target.

Fresh nonnull `+148` with exact target kind DWORD two reaches the genuine shared
refresh. Its return value is ignored; reached refresh writes remain on error.
The subsequent raw WORDs `+08/+0A/+0C`, rather than virtual focus `+14`, form a
wrapped signed delta from fresh vehicle position, normalized by `00417DA0` and
scaled by 400. Known stale raw focus can complete; released or unwritten storage
fails after the already published eye. Final look velocity is the vehicle's
fresh velocity, independently of target velocity and settings-gated virtual
focus. Required failures never manufacture a completed look vector.

### Selector-2 special-view storage and reader

`004156E0` writes byte `+1C0` and the four angle WORDs `+1C2..1C8` to zero,
but leaves position `+1CA..1CE`, forward `+1D0..1D8` and up `+1DC..1E4`
unwritten. The vehicle owner represents each stored position WORD and raw
float DWORD as an independently unknown scalar. Raw float bits preserve the
stored data; constructor zeros and current vehicle XYZ do not initialize
these holes. The storage remains private pending an owned writer.

`00421D50` reads stored X/Y in one DWORD before either temporary output write,
then reads Z and adds 50 to temporary Y with WORD wrap. Unknown X or Y stops
before both copies; unknown Z retains temporary X/Y before the Y addition.
These six output bytes are temporary caller storage, distinct from the camera's
persistent eye parameter block.

`00490370` next copies each forward DWORD into basis Z, then each up DWORD
into basis Y. A missing later component preserves earlier copies and the old
basis X/count; it has not reached cross math or the counter reset. Complete
inputs use the shared orientation kernel with the original zero rebuild flag:
quantize and normalize X=up cross forward, then rebuild Y=Z cross X while
preserving supplied Z. The reader then calls yaw `00491530` and pitch
`00491290`, in that order, with signed WORD angles `+1C6/+1C8`. Both calls
execute for zero angles. Orientation resets the count to zero and the two
rotations advance it to two, making their >100 normalization/CRT tails
unreachable within a complete leaf call.

The actual mode-four suffix at `0041006C..00410104` publishes eye only after
leaf success, using the temporary XYZ and current vehicle velocity. It then
truncates camera basis Z times original constant `005205B8` (500.0), narrows to
WORD and adds fresh current vehicle XYZ. Finally, the current primary target's
virtual +08 supplies its `+1A` velocity for look publication. The mutable
request borrows that actual target owner; it does not carry a ready velocity
copy. Missing late target velocity retains completed basis and eye writes
while leaving look unchanged. Primary focus `+08/+14`, process NULL and
settings are not consumed by this path.

Only genuine derived mode-three/four dispatch moves ahead of generic focus
and velocity preflight. The enclosing pose still owns target presence and
refresh; modes zero/one/two and the null-derived mode-three fallback preserve
their existing requirements and order. The camera owner commits reached basis
writes on both success and feature error. This reader does not bypass earlier
acquisition, reselection or target refresh.

The free look `00421DC0` ([`free_look.rs`](../../crates/inf-game/src/object_runtime/actor/derived/selector2/free_look.rs)) activates it and writes
the storage. Its active path reads the old stored position before a
conditional position write; the unsigned wrapped squared-distance >24 gate
is not a fresh initializer, and the vehicle's position words start as a
fresh-heap carry (zero) for it.
Input-buffer controls, game zoom state, lazy state construction and lifetime
also remain separate boundaries. Known supplied storage establishes reader
behavior, not an ordinary successful special-view activation or retail scene
acceptance.

The separate original-instruction checker
produces [masked leaf receipts](../../crates/inf-game/tests/fixtures/selector2_special_view_native.json)
for twenty supplied histories and twenty-one calls: ten complete calls, nine
whole-scalar storage prefixes and two missing-angle diagnostics. Each full
original body and the sine/constants are authenticated. Complete calls execute
only the leaf, orientation, yaw and pitch; the unreachable normalization/CRT
tails do not count as executed coverage. Reached nonstack reads/writes and
masked temporary XYZ/basis are retained; stack knownness, ABI restoration and
control-word checks are online, without a serialized stack timeline. Nested
consumed arguments are genuine math scratch, while the root's return and two
pointer slots remain intact.

The [Rust comparer](../../crates/inf-game/src/object_runtime/actor/derived/selector2/camera/special_view_native_receipts.rs)
audits all twenty-one native calls before constructing genuine owners for
eighteen histories and nineteen admitted calls. It compares reached errors,
temporary XYZ, all ten raw basis DWORDs and unchanged source/arena custody.
The two unknown constructor-angle cases are audited native diagnostics,
excluded before typed construction; the constructor already owns complete
angle WORDs. This leaf proof observes temporary XYZ and all ten basis DWORDs. It does not execute the
mode-four persistent eye/look suffix, constructor, activation, movement writer
or arbitrary pointer aliases. Ordinary source/runtime tests own the caller's
late velocity and parameter-publication checks.

### Terrain clamp and later pose publication

[terrain.rs](../../crates/inf-game/src/object_runtime/actor/selector16/pose/terrain.rs)
owns `00410BF0`'s line samples from target focus, beginning 128 units away,
and the subsequent eye floor clamp. The clamp queries desired-eye X/Z,
chooses the larger interpolated terrain/water surface, and adds
`min(abs(signed desiredEyeY - signed desiredLookY) + 32, 80)`. This difference
uses full signed operands before subtraction, rather than WORD wrap.
Negative current Y velocity bounces as `-trunc(velocity/4)`.

Camera position is written before orientation at `004107AF..7B7`. Mode two
copies all ten target basis DWORDs, including the rotation counter. When a
derived component is present, first-person tilt samples Z.y times 32768,
truncates through `__ftol`, queries `0048F4D0`, compresses through `00410B70`,
and rotates Y/Z. It then samples the resulting X.y and repeats with a
negated X/Y angle. The second sample must observe any renormalization caused
by the first rotation's counter crossing 100. Other orientations use
`00490370`: copy forward/up, quantize and normalize X=up cross forward,
rebuild Y=forward cross X, and reset the counter.

## Process initialization and shake

Static executable data initializes DWORD `0052F510` to `0x004C4B40`, the
integer 5,000,000 microseconds. `005469F0/F4` and rotation WORDs
`00546958/5A/5C` are BSS zero. The CRT initializer pointer at `0052E154`
invokes `0048F630`, which constructs three floats through
`00402680(numerator,4096)` and `00402690`: `0055F280/284/288 = [0,1,0]`.
The [current up owner](ACTOR_ANGULAR_RESPONSE.md#current-process-up) is retained
once by `PortSession`; production camera and intro/demo requests forward its
current data across world and display replacement. Startup seeding is supported
by the CRT registration and bounded writer audit, without claiming arbitrary
computed writers or native execution of every consumer.
The selector-90 motion switch `00538BC0` is executable-initialized DWORD one;
its fractional-thousandths path is independent of camera microsecond units.

[shake.rs](../../crates/inf-game/src/object_runtime/actor/selector16/pose/shake.rs)
preserves process interval/pause/step timers and the shared LCG. Interval
expiration draws a pause `(random + 0x7A12)*16`. Pause expiration draws an
interval `(random + 0x1312D)*64` and still reaches the step in that tick.
The strict `step < delta` gate draws X, Z, then Y. Modes other than two add
`32 - (random >> 10)` to position; mode two adds
`1024 - (random >> 5)` to persistent rotation WORDs, decays by signed 15/16
truncation, then calls the local-vector rotation consumer. Disabling shake
resets interval/pause without consuming RNG or clearing step/jitter.

Shake switch `005483B0` has two nonzero writers, `00406382` and `00476520`
(`0047653F`, which also arms a countdown from its `+15C` seconds). Neither runs
anywhere in the Infest01 recording: its only five writes are zero stores at
world load `0044FEA3` and cleanup `00450215`. The intro reads the switch 2,675
times, always zero, so it never shakes; the port's unowned writers do not affect
it.

## Native host, rotation and draw publication

[native.rs](../../crates/inf-game/src/object_runtime/actor/selector16/native.rs)
implements `CameraPoseHost` as `NativeCameraHost`. Its data-bearing
`NativeCameraInputs` carries retained H12 terrain, H14 world flags, the
original quarter-sine table and original signed inverse-sine table. Raw
cell, interpolated terrain, water surface and plane queries keep their
distinct units and native callers. The camera water clock is literal zero.

Actor virtual `+64` is null `00401300` for every admitted class except the
selector-2 vehicle, whose `00520860+64` is self `00404610`. The
[shared class-only query](PLAYER_VEHICLE_STATE.md#pure-actor-vehicle-query)
returns its actual payload without consuming camera fields. The camera adapter
then reads live byte `+1C0` for the special-view gate; reselection needs only
class identity. Null dispatch is proven at the final
vtables `00527C00` (shared), `005204D0` (camera), `005289C0` (selector30),
`00524AC0` (intro controller), `00526B50` (helper90), `00520E58` (derived
base), `0051F080/00527DE8/0051FBB0` (11/60/77), and the admitted family
tables `00520F34/0052104C/00521164/00520B84/00520CC8/00527058/00527EE0/
00521EAC/0052273C`. Their `+B8` slots are all null; `+60` is self only for
the camera. These are concrete callback dispatches, independent of `+B4`
class ID or `+C4` lifecycle state.

`ObjectArena::reselect_camera_target` owns `0040FB40`'s G+48 ordered actor
walk. Flag bit0 gates candidates. Camera `+60` self scores one without
RNG; all other admitted candidates reach one shared-LCG low-WORD draw.
The score ORs bits for signed `+B4` ranges, `+C4 == 3`, non-null `+64`, and
speed greater than 100. Native `0040FC3F` shifts an overflowed squared-speed
sum logically by two before integer sqrt and doubles the result; it does
not take sqrt of a negative DWORD. Strict unsigned score improvement
preserves the earlier actor on equal scores.

The adapter also owns target-loss player selection through the
[shared actor-handle player lookup](PLAYER_ACTOR_LOOKUP.md) (`0046B090` then
`00488AA0`) and `0046B060`'s separate first-player query. The finder requires
the live process null handle before any empty-list or fallback decision.
Production borrows the session's retained
[process state](PROCESS_ACTOR_STATE.md); an unavailable current value stops
only the reached target-loss feature.
That separate first-player query resolves
only the first player's `+D8`, with no `+D4` fallback, even if the owner found
by the old target handle is a different player. Only a missing actor or
signed lifecycle state below three reaches camera view-mode zero followed
by required `0046AE90` attachment. `NativeCameraServices` requires the real
attachment/factory owner. Vehicle Chase and the special-view reader are
arena-owned phases; special-view activation retains its required writer
boundary. Neither supplies captured poses.

[rotation.rs](../../crates/inf-game/src/object_runtime/actor/selector16/pose/rotation.rs)
owns `0047A760 -> 004909E0 -> 00490FC0/00491D10`: transform the signed local
WORD rotation vector through the current basis with nearest-even low-WORD
rounding, normalize that world vector, and use its rounded length as one
axis/angle rotation. The trig samples replicate each table WORD into a
Q31 DWORD and scale by float `00528CE4` (2^-31), distinct from the Euler
consumer's `00528CE0`. Dot order is `(X product + Z product) + Y product`.
Parallel/orthogonal components and cross X/Y spill to floats; orthogonal Z
also remains in-register for cross X, and cross Z stays retained through
the trig multiply. The basis counter increases by three; unsigned values
above 100 quantize/normalize Z, Y, then X through `00417DA0`, rebuild Y and
reset the counter. This X normalization retains its divisor, whereas the
Euler consumer's inlined X normalization spills it. Three Euler calls or
the Euler renormalizer would implement different arithmetic.

`CameraPose::draw_request` and `ObjectArena::camera_draw_request` expose
`CameraDrawRequest { eye, basis, coordinate_shift }`. Camera draw
`00410D90` sign-extends **all** XYZ WORDs (`00410D95/9E/A5`) and copies all
ten basis DWORDs unchanged. Its final instruction at `00410E5A` writes
byte `0055F250 = 0x10`, so live camera publication carries coordinate shift
**16**. `0048FB40` reads that byte at `0048FD81`; each model-eye DWORD
delta is shifted left then arithmetically right (`0048FD9F/A1`,
`0048FDB5/B7`, `0048FDC4/C6`), giving signed low-WORD relative distances.
Entity submission `0047AFC0` likewise sign-extends actor X/Y/Z at
`0047AFF9/0047AFF5/0047B006`; asset WORDs remain unchanged. Render-context
`+6C`, assigned by predraw `00488380` from H7 material 0x74 or 0x20, is
a separate color resource and is not this coordinate-shift process byte.

## The remastered camera (port option)

The Video menu's Camera Style row is settings `+6C`: retail's 0 Normal and
1 Cinematic, whose exact 1 selects the alternate tables (`0040F170`). The
port adds 2, Remastered (2026-10-27), on every renderer: both
menus' rows step on to it (`settings_row::row_most`), it draws as port text
in the six languages, and the launcher's Options offer the three. Retail's
camera code takes 2 as Normal, which the remaster builds on.

[camera_clearance.rs](../../crates/inf-game/src/camera_clearance.rs)
changes only the view the world draw publishes (`SceneView` from
`camera_draw_request`), never the camera actor, its cached position
`005469E8` (which selector 86 reads) or anything the game reads of it; its
tests keep their own `ProcessFurnitureQueryState`, so the game's query
residue stays retail's, and its shake its own noise. Retail runs game
logic inside the world draw for what is in view, so which view that draw
takes is the game's
([the game's draw and the shown draw](PRESENTATION_POLICY.md#the-games-draw-and-the-shown-draw)):

- **Software, OpenGL and every network game play retail's.** The game's
  draw keeps the camera's own eye and the template's focal, and the
  remastered view is drawn apart from the game: there the camera changes
  only the picture, and sound pans and fades from retail's camera.
- **Vulkan single player plays the remaster's** (2026-10-28:
  the Remastered view drives the game there): its eye, focal and speed
  kick are the game's draw's. On Shak'tahh's start, reversing, Normal and
  Remastered event logs parted at tick 66 (measured before the split, when
  every renderer played the drawn view), where the swung view put an
  object in view: its draw took another branch (`object_models.rs:93` for
  `:220`) and birthed a particle, and the shared random stream moved on
  from there.

The session's game is fixed when its window opens and its replay records
it. Commit `02b367d31`'s message ("so play, replays and network games are
unchanged") was wrong for play: until the split (2026-10-29) the drawn
view reached the game on every renderer.

- **It acts in chase and first person** (camera modes 3 and 2). The view
  of a lost craft (mode 0, the death camera) and the dock's (1) let it go:
  its lift and dive ease back onto the game's camera, so a craft dying
  under the water has the eye rise through the waterline and settle onto
  retail's death camera, which carries on as always (2026-10-28: zoom out above water
  if dying under water, then continue on the regular death-camera
  functionality). The special views (4) and any jump of the
  game's eye over 256 units in a frame, more 8192 units a second over a
  long frame (a respawn, a new view, a warpgate; not a stall at speed),
  start it afresh. Before that it kept its dive while the death camera flew up,
  5,500 units apart, and then snapped.
- **It rises over what blocks it.** While it chases a vehicle, a view is
  clear when retail's sight test (`0043AD90`) finds nothing on the ray from
  48 units above the craft to the eye. A view blocked for 0.2 s raises the
  eye about the craft, at the same distance and bearing, in 6-degree steps
  to 60 and three halvings, to the lowest clear elevation, and turns to
  look where the game's view did, as V2000's chase camera clears what lies
  behind its craft. The lift moves as a critically damped spring (about
  0.25 s up and 0.7 s down, at most 70 degrees a second), holds its height
  0.75 s once no longer needed, then settles. Retail's own camera keeps
  above the ground and water alone (`00410BF0`). Backed into a parked
  vehicle on Shak'tahh (14672, 30353), it rises to 28 degrees over a second
  and looks over what hides the craft from retail's view.
  - The first lift (2026-10-27) also counted any actor's or furniture's
    model within 96 of the eye as blocking (`0047CEB0`, `00435CB0`), and
    the ray carried on 96 past the eye, and it rose in 0.06 s. Played (a
    run of 2026-10-28, reversing past a dock's pylon), it answered a
    wall beside and behind the camera, one it never sees, with a 40-degree
    swoop up and back over a second and a half: the "snaps back" that was
    seen. Neither test is kept, and what passes between the camera and the
    craft for less than 0.2 s never moves it.
- **At speed it chases** (2026-10-28, of the skimmer circling
  Oolbryte's pit: unusable at high speeds and turning; it needs to behave
  more like a chase camera rather than a lazy camera). Retail's slew
  (`0040F1C0`) moves the eye and the look toward where the game wants them
  (behind the craft at 400 plus a fifth of its speed, at most 1000, and 400
  ahead of it) with the craft's own velocity plus a pull, but caps the
  change at 5000 to 12000 units a second each second. The skimmer circling
  the pit (around the floor's warpgate, placement 155, at 3300 units a
  second, turning 190 degrees a second) needs more, so the look fell 930
  units behind and the eye drifted round to 76 degrees off the craft's
  tail: retail's own view lost the craft on 441 frames of a recorded run,
  and the remaster drew it as retail did.
  - In the chase, the drawn eye and look are pulled back toward where the
    game wants them (`eye_138` and `look_15c`'s desired positions) on a
    leash: 3000 units below 1500 units a second (no change), tightening to
    200 from 2500; past it the drawn lag eases out to 1.5 times as far,
    never further. Slowing down keeps the leash until the game's own lag
    closes within it: dropped at once, a crash at speed swung the eye 50
    degrees back out round the craft to retail's lagging eye and back over a
    second (the review, on a lap of the pit); held, it stays 18 to 22
    degrees off the tail through it. The pull moves as a spring (about
    0.15 s), as the game's targets jump (a crash shortens the stand-off; a
    target lock turns the look 400 units), lets go the same way for the
    death camera and the dock, and keeps the eye no nearer the ground or
    the water than the game's own, up to retail's 80. Retail clamps its eye
    at the ground under the desired eye, so where it lags it may stand
    below the ground under itself; the leash leaves that as it is.
  - Whatever the game's camera does, a craft straying past 28 degrees from
    the view's middle turns the view toward it, easing to at most 36 (the
    view reaches 42 degrees up and down); the turn moves as a spring (about
    0.15 s, at most 4 radians a second). In that run, below 3000
    units a second, retail kept the craft within 25 degrees and it never
    acted. On Shak'tahh, a buggy spinning after a crash close to the camera
    drifts 50 degrees off and the view turns up to 13.5 degrees toward it;
    where the game hands first person to the chase under water, retail's
    chase starts looking 50 degrees past the sinking craft and the view
    swings up to 30 onto it, in 0.2 s.
  - Replayed with both over a 3-minute run (identical in every
    frame's game state to the run without them, measured before the
    game's draw kept retail's view; on software and OpenGL every camera's
    game is now retail's) and four laps of the pit
    with the skimmer from a `--start` on Oolbryte: no frame loses the
    craft, which stays within 0.34 of the half-frame in that run (0.75
    on the laps). At speed the eye stays within 35 degrees of the craft's
    tail, about 21 on the laps (retail up to 76). Below 2000 units a second
    only 12 of the run's 5046 chase frames change, by at most half a
    degree, as the leash lets go after a lap. The view turns about as fast
    as retail's at its fastest (253 degrees a second on the worst lap, to
    retail's 244), and its turning changes less abruptly (p99);
    the eye moves faster as it goes round with the craft (p99 5500 units a
    second, retail 4200) and rides the pit's wall a little bumpier (steps
    over 10 units in a frame on 3 percent of fast frames, retail 2). Backing
    into blockers and the turbo on Shak'tahh, its skimmer off the cliff into
    the lake, and a buggy sinking in Altair's trench draw exactly as
    before.
- **It sees wider:** the focal lengths scale by `tan 45 / tan 50`, so a
  4:3 view spans 100 degrees for retail's 90, outside special views (the
  scope's zoom), and wider still with speed: 6 more degrees as the craft's
  speed along its hull goes from 300 to 1000 (velocity `+8E..+92` moves
  `v * delta / 2^20` a tick, so about units a second; the buggy tops out
  near 1010), and 4 more while a turbo boost runs (the vehicle's `+224`
  timer, not a flip). The kick moves as a spring of its own (about 0.35 s
  in and 0.6 s out, at most 12 degrees a second) and scales the focals
  after `00442080`'s slew (`WorldFrameInputs::focal_kick`). Fed to the slew
  as its source, as first built, it flickered: the slew answers any drop
  in its source, even of one unit, with its whole `0x50` step, so each
  widening tick drew one frame about 10 degrees wider and snapped back, on
  155 frames of an 87-second run (the "jitters"). Only the draw and
  the HUD's reticle projection read the focals; in retail's game the
  game's draw slews the template's focal apart (`world.view_focals`), and
  the widened, kicked focals reach only the view shown
  (`world.shown_focals`).
- **First person rolls half as far** as the game's view, itself half the
  hull's roll; its heading and pitch stay the game's. The first remaster of
  first person (2026-10-28) also eased the pitch after the game's (0.2 s):
  played, it trailed the game's by up to 11 degrees and carried a tilt
  across a cut out of the pause menu, so it went.
- **A turbo boost kicks the camera:** a 0.4 s shake that fades as the boost
  begins, from smooth noise of its own at 12 Hz, and a gentler 7 Hz rumble,
  0.3 as strong, while the skimmer is pushed on (Thrust or Climb held in
  form 5), coming and going over 0.25 s. It moves the chase eye up to 5
  units and turns first person up to 0.6 degrees, never with the game's
  random draws, and stays out of the way of retail's own shake (process
  `005483B0`, a stabiliser's death or a brain attack). A rumble at full
  speed in every form, at 18 Hz, read as jitter and went.
  - **Per form** (2026-10-29: the shake on the skimmer, the buggy and
    the truck as it is, the skimmer's 10-15% stronger, the buggy's
    stronger while boosting (Space) and under acceleration). The
    skimmer's whole shake is 1.12 times as strong. The buggy's is 1.4
    times, and it rumbles on the same 7 Hz noise while Up or Space is held
    and its speed along the hull rises (none below 200 units a second each
    second, 0.6 of the full shake from 1500), and at least 0.3 while a boost
    runs. Its old shake ended 0.4 s into a 3 s boost, at about 1073 of the
    1731 the boost takes it to in a second, and plain throttle (at most
    about 730 a second each second from a standstill) never shook it. The
    truck, the hover and the VTOL shake as before, and outside a boost a
    buggy standing, cruising or rolling with nothing held never shakes.
  - **Measured in motion** (`.tmp/camshake/`, scripted drives on
    Shak'tahh, OpenGL; chase eye offset in world units, rms with the peak
    in brackets): the buggy's boost from a standstill 1.10 (2.84) to 3.31
    (4.36) while it gains speed and 0 to 1.84 (2.25) for the two seconds it
    holds it; the throttle from a standstill 0 to 0.63 (1.18); the skimmer
    pushed on 1.28 (1.49) to 1.43 (1.67), 1.12 times on every frame; the
    truck's boost 1.13 (2.84) and the hover's and VTOL's drives the same
    on every frame. On OpenGL the game's event logs stay identical; in
    Vulkan single player, where this view is the game's draw, the shaken
    eye moves what the draw sees: the truck's run stays identical, the
    buggy's and the skimmer's part within a second of the first changed
    frame (a particle's birth, then the shared random stream), so a Vulkan
    Remastered replay recorded before, with the buggy or the skimmer
    shaking, can part from its recording.
- **It follows the craft under the water** (2026-10-28: it should be able
  to see under the water instead of remaining above it at all times). Retail's clamp (`0041063A..0041071D`) holds the eye above
  the higher of the ground and the water (the waves at clock 0), and once
  the water stands 256 above a sinking craft first person gives way to the
  chase (`pose.rs`). While the craft lies more than 64 below the live
  surface (the drawn waves, at the animation clock), the eye goes down to
  the height the game wants (`eye_138`'s desired eye, clipped to the ground
  but not the water), at least 128 above the ground (a steep look down
  keeps the near plane off the seabed) and 64 below the waves, so the near
  plane never cuts the waterline. Where a trench leaves no room it comes in
  toward the craft (its distance times 0.8, 0.65, 0.5, 0.4), where none has
  room the game's eye stands, and it keeps looking where the game's view
  did. It follows down and back as a spring (about 0.3 s, at most 800
  units a second) and passes the band round the surface in one step each
  way.
- **Under the water the raster draws the water's murk** (software, OpenGL,
  and Vulkan with ray tracing off where it cannot shade the water: no
  scalar block layout, or lava), decided once a frame from the final eye
  against the live surface (`WorldDrawReceipt::under_water`):
  - V2000's own underwater projection, as OpenV2K carries it from that
    game's wet projectors: each projected vertex moves `-2..1` pixels
    of a 480-line view, `(tick + 2 * (x + y)) * 0x200` through the sine
    table, `* 0x10001 >> 30`. Infestation has no underwater view of its
    own; its sine table gives V2000's cardinal offsets exactly. It is laid
    on each triangle's vertices before clipping (`WetFrame`), so the
    screen's edges stay sealed, and the HUD never wobbles.
  - Retail's own vertex fog, shortened to `[0x100, 0x1000]` and laid in the
    water's colour: the all-water tile's mean colour (`00537010`'s
    sixteenth record, as the tracer takes it) at 0.35, which the terrain's
    fog materials `0x73`/`0x74` take too.
  - The world's colours at 0.7, leaning half way to the water's hue, the
    textured fog dimmed alike so flat and textured draws meet in the same
    murk, and the frame cleared to it in place of the sky (a 4:3 canvas
    keeps its bars and its sky). The water's surface strips are not drawn
    from below.
  - Vulkan's raster, where it shades the water, draws the world as the
    eye above has it and lays the same murk itself, the surface seen from
    below through Snell's window
    ([the modern raster's water](../rt-plans/modern-raster.md#the-water-stage-4)).
  - Path tracing makes the water round its eye a medium (2026-10-29,
    the plan, item 38),
    the surface from below through Snell's window.
- **The game's sound is muffled under the water**, on every renderer: the
  mix less the music through a one-pole low-pass at 600 Hz, at 0.8,
  fading in and out over 0.2 s (`PlatformAudio::set_under_water`).
- Reversing used to swing the view round (0.6 s past -80); it fired on
  almost any backing off, as the wheeled forms reverse at 0.4 of their
  forward speed and pick it up some 16 times slower, and a three-point turn
  flipped the view twice. A rear-view mirror was planned in its place and
  dropped (2026-10-28: it kills performance and is not worth it
  for a remaster): a second view is a second world draw, and the draw
  writes game state (the wheels' and legs' vertex callbacks write the
  arena), so the world cannot simply be drawn again.
- **How it was played** (2026-10-28): a log of the game's view and the drawn
  one at every draw (a local diagnostic, not kept), over a
  run replayed (87 s: two long reverses, first person, a turbo) and over
  scripted drives: backed into what blocks the view, fast turns and a
  turbo on Shak'tahh, the skimmer off its start's cliff into the lake,
  first person, and a buggy sinking and drowning in Altair's trench. No
  frame's focal moves more than 2 units; outside the lift and the dive the
  drawn view moves from tick to tick as the game's does; and every change
  of mode is continuous but the game's own cuts.

On Shak'tahh's start, a 9 by 9 grid at four headings changed only five
places under the first lift, where something stood between the camera and
the craft or beside the eye. Backed into three of them again (2026-10-28),
two still rise; at the third a model only stood beside the eye. The tests
cost no measurable CPU time a frame.

## Acceptance and external boundaries

Required-host failures preserve target/timer/parameter writes already reached.
The target-loss callback can mutate the same camera through player attachment;
the runtime republishes before invoking it and reloads after success or error.
Position, basis copy, and individual first-person rotations likewise retain
reached writes. This is feature-local failure handling, not a claim of exact
native exception or rollback behavior. The shared production camera phase owns
session clock/settings custody and native camera publication for every loaded
world, including ordinary vehicle worlds without an intro controller;
[the intro authority](INTRO_SEQUENCE.md) records its remaining actor-task and
allocator bounds. Complete scene rendering and a retail visual comparison
remain acceptance work. Special-view activation, native vehicle physics,
ordinary actor simulation and player controls remain separate owners; the helper90 follow
path does not reach selector-2 Chase.

`004875A0` orders object `00478F20`, collision `004790C0`, acquisition
`00478E00`, pose `0040FD10`, then particle transfer/update `00464DA0/00464B30`.
[session_platform/camera.rs](../../crates/inf-game/src/session_platform/camera.rs)
connects that shared camera seam after the supported object subset and before
particles. A null acquisition skips only pose; a feature error retains reached
owners and permits particles and elapsed accounting. Actual process clock,
settings, terrain, listeners, camera globals and world RNG are borrowed fresh.
The common fresh-allocation policy remains the existing labelled approximation.

The retained `CameraGlobalState.special_view_active` owns gameplay byte `+5C5`.
`0040FD10` writes `mode_188 == 4` before its mode-specific target callbacks;
later pose failure preserves that reached publication. Production world drawing
passes the same session owner into `WorldFrameInputs`: `00442080` decays each
focal by `0x50` while `+5C5` is clear, and uses quarter easing toward the
`+5C0`-scaled target when special view is active. This shared draw policy applies
to intro and ordinary mission worlds without a campaign-selection check.
Player look/zoom `+5C0` still uses the explicit constructor-zero input boundary.

World Draw now executes a fresh `00478E00` acquisition at the `004883AA`
seam, independently from the last Tick's diagnostic `WorldSession.native_camera`.
Tick and Draw share the actual factory-input preparation, borrowing current
session residency, settings, terrain, listeners, sine and process state. Draw
captures one current resource-animation value; it does not tick pose, advance
either clock or clear an earlier pose failure. A resolved handle bypasses the
process read; a resolution miss reads current process-null custody before H13
factory admission, including rejection.

Predraw captures header H then manager F before acquisition. After acquisition,
the reached fade/setup and normal H7/0x74 material publication precede
`camera_draw_request`. That request reads current signed XYZ and all ten basis
DWORDs from the returned camera, including after a pose error; `00410E5A`
publishes coordinate shift 16. Unknown camera position fails after the reached
acquisition, fade and material writes. Material failure likewise preserves
earlier factory/listener/list/handle changes. The typed position admission is
whole-span containment, not native per-scalar context-fault parity.

A genuine NULL return reaches fade/material, then reports
`RequiredRetainedWorldView` for the context-dependent world-draw feature.
The fitted spawn-view approximation is removed. Existing backend view/shift
persist without a camera publication. `G+648` points to context `00546E88`,
while `00410D90` separately synchronizes model globals `0055F190..1C0`;
sky reads context X/Z. A prior frontend model view therefore does not prove
complete retained native context custody. That NULL continuation remains a
separate owner; the surrounding linked/presentation pass continues.

Canonical slot 1 constructs selector 16 with no command suffix. This current
factory domain returns a camera or a typed error; inactive H13 reports
`NotResident` before slew. The NULL policy follows the native conditional call
and the kernel's virtual `+60` contract; public Draw tests do not fabricate it.

`WorldProjectionSetupApproximation` retains the existing atomic backend Setup
command: projection/fog and both fade bounds publish together before material.
Native `00488443` writes the near bound before material, `0048847D` writes far
after it, and the fog tail follows camera publication. The renderer projection
owner must replace this bounded failure-prefix difference; this connection
does not claim complete `00488380` context-write parity.

Production corpus coverage uses the retained intro and ordinary mission 18.
An additional selection-2 load reaches the pre-existing `00479840` actor
virtual +30 disposal boundary before camera admission. That loader error is
kept explicit; this camera connection does not establish later campaign loads
or replace their unowned cleanup with success.

The reproducible original-instruction checker
produces [masked native receipts](../../crates/inf-game/tests/fixtures/vehicle_camera_targets_native.json)
for supplied selector-2 and target storage. The
[Rust comparer](../../crates/inf-game/src/object_runtime/actor/derived/selector2/camera/native_receipts.rs)
admits 38 Chase calls, including five reached error prefixes, and six separate
signed-DWORD projection calls. It audits all 48 native calls first; four raw
diagnostic histories have storage masks outside the typed domain and are
excluded before genuine actor construction. Original virtual callbacks, target
refresh, settings, math and CRT instructions execute without success stubs or
patched executable bytes. Known stack reads are checked online by the producer;
the receipt retains counts and final masked storage, not a serialized stack
timeline. Native registry capacity/free-chain/cold-cache numbers are supplied
inputs; the typed phase compares referent and cache semantics using its genuine
registry. This evidence does not establish constructor heap history, special
view, enclosing Tick cadence or retail visual acceptance.

The original inverse-sine WORD table at `0053FF00` has a bounded signed
decoder, `parse_frontend_inverse_sine_table`. `native_basis::inverse_sine_q15`
implements `0048F4D0`: saturate the absolute signed input to 32767, shift by
five for its table index, sign-extend the selected WORD, and negate for a
negative input. This handles `i32::MIN` through the native saturation branch.

`TerrainRuntime::water_surface_height(TerrainWaterSurfaceRequest, sine)`
reuses the separately owned `00410F60` kernel `native_basis::spacing_sample`.
The handler pointer table at `0053AA14`, entry twelve `0053AA44`, names
`0055EE38`; its registered entry array is `0055EE3C`. Both raw camera cell
sampling and the plane DWORD used by `00410F60` therefore belong to **H12**,
whose retained `TerrainRuntime::plane_base()` is the source prefix DWORD.
The gate is separately H14 world entry `+4 & 0x40`, loaded from `0055EDDC`
at `004106BE..6C8`. These are distinct from handler ten's map resources.
The old provisional H10/H12-flags labels have been corrected in the API.

The water request carries unsigned low-WORD X/Z, explicit wrapping DWORD
`wave_clock`, H14 `world_flags`, and signed terrain low AX. The H12 plane
is shifted arithmetically by process `005483BC`: 8 for Units32 terrain
height, 9 for Units16. The sine block is reached only when flag40 is set
and the full signed shifted plane exceeds the signed terrain argument.
It combines three plain signed sine samples with SAR shifts five/six,
narrows to signed WORD, scales by `plane - terrain + 512` with wrapping
DWORD multiply/SAR15, narrows again, adds the plane, and clamps to terrain
before returning low AX. The camera caller at `004106D1` passes literal
zero for the wave clock; a generic caller must preserve its own clock
input. Tests discriminate both height scales, the independent wave gate,
clock phase/sign changes, the plane comparison, and the terrain clamp.
