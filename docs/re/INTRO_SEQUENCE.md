# Native engine intro and campaign continuation

New Game initializes mission selection **22**, whose first H14 world is in
**OVL leaf 32**. Its phase 3 briefing has no authored briefing records and
finishes on its first native gate tick. Phase 4 then owns the in-engine intro.
Intro completion selects **18**, enters phase 8, and phase 8 requests phase 3;
selection 18 loads **OVL leaf 28** and displays the Level 1 briefing. Starting
that briefing enters phase 4 over the already loaded Level 1 world. These
selection numbers, OVL leaves, and session phases are distinct identifiers.
The session/load chronology and accepted recording remain owned by
[BRIEFING_RUNTIME.md](BRIEFING_RUNTIME.md) and the
capture ledger.

This document owns selector 88's authored intro sequencing and its completion
contract. It does not establish a complete live cinematic. Production now
connects the retained controller, canonical helpers and live camera pose to
the session/world draw. Selector-11 retirement now composes its ordered world
effects, particle births and terrain tail. Referenced actor tasks and broader
damage/furniture/network callbacks remain explicit feature boundaries. The
sequencer owns the native completion request, and production now completes
the automatic handoff to the Level 1 briefing. Results retirement reaches
[shared world cleanup](WORLD_CLEANUP.md), whose selector12 destructor now
executes the selected descriptor exit before target/probe/helper release.
The [active-descriptor destruction owner](ACTOR_BEHAVIOR_RUNTIME.md#concrete-weighted-target-ownership)
uses both native descriptor tables and the actual false-argument callbacks;
preferred-target exit clears +29C and normal return clears active +184.
The shared `004237D0` family destructor now executes all three native mode
tables under its installed destructor vtable for selector10/36 and the base
class; selector10's helper release remains before descriptor exit. Other
family exits remain explicit feature boundaries.
The production continuation and bounded native checks establish this handoff,
not complete actor simulation or matched cinematic behavior. The existing
initial-deadline, results and reload approximations remain explicit.
Escape follows the native completion request. No observed scene duration is
used as a timer or skip.

The player-visible new-save order is **in-engine intro cutscene → mission
briefing → Level 1**. Startup `Title.avi` is separate. Native phase 3 first
processes selection 22's empty briefing program as load setup; that pass is
not the visible mission briefing. The cinematic completes to selection 18,
whose visible briefing precedes its level. Internal phase 8 is the native
completion/reload handoff between those two worlds.

## Controller and retained script

| Native address | Responsibility |
|---|---|
| `00443890`, vtable `00524AC0` | Selector 88 construction |
| `00443900` | Command 16 retains H14 script pointer at actor +13C |
| `00443930` | Post-load rewrites authored placement references to actor handles |
| `00444930`, virtual +24 | Timer/callback ordering and helper ticks |
| `00443B10` | Stage entry/exit effects |
| `00444630` | Elapsed work and actor-dependent stage gates |
| `00487560` | Offline completion wrapper and phase 8 request |

The source authority is the corresponding functions in
bulk_clean/game_logic.c, checked against the retail
executable's instruction ordering. Executable policy is in
[intro_sequence.rs](../../crates/inf-game/src/intro_sequence.rs), with a
required `IntroHost` for every query and effect. The actor's actual retained
allocation and timer state are exposed by
[selector88/intro.rs](../../crates/inf-game/src/object_runtime/actor/selector88/intro.rs).
The adapter shares the existing `Arc<RuntimeAllocation>` and relocation
records; it does not reconstruct the script from copied values.

Constructor `00443890` clears +13C/+144/+148/+14C/+154/+158. DWORDs +138,
+140 and +150 have no constructor write and stay optional until supplied by
an allocation owner or an explicitly named approximation. The ordinary
sequence writes +150 on stage 8 entry before its first read, so missing
allocator history for that field does not itself block the ordinary path.
An arbitrary resume inside stage 8 with unknown +150 remains an error.

The tracked corpus test
`mission22_intro_retained_script_and_native_state_share_the_loaded_allocation`
in the actor adapter pins mission 22's actual retained script at allocation
offset **6524**, selector 88 placement **118**, the shared allocation identity,
and the following authored references:

| Script offset | Meaning | Mission 22 values |
|---|---|---|
| +00/+04/+08/+0C | IDs passed unchanged to `00404F00` | 59, 65, 70, 108 |
| +10 | Main actor placement reference | 53 |
| +14/+18 | Primary count/array pointer | 6; 48, 49, 50, 51, 83, 84 |
| +1C/+20 | Sequential count/array pointer | 2; 30, 31 |
| +24/+28 | Secondary count/array pointer | 5; 68, 69, 52, 79, 80 |
| +2C/+30 | Exit count/array pointer | 2; 97, 98 |

The actor references are one-based authored placements before after-load
rewriting. The test resolves their rewritten handle DWORDs back to those
placements. Array relocations are at 0x19B0/0x19C8/0x19D0/0x19E4. Counts are
signed DWORDs and the native loops reread them. Stage 9 reads exit row 0
unconditionally; an empty count cannot be used to hide that dereference.
Route-node custody and lookup are described in
[ROUTE_RUNTIME.md](ROUTE_RUNTIME.md).

## Timer and stage policy

`00444930` treats its scheduler DWORD as unsigned microseconds and converts it
with a logical **right shift 10**, retaining no fractional remainder. The
fixed numbers below are native authored timer units after that shift, not
captured seconds or a guessed total scene duration.

Each tick adds the shifted delta to +138 and subtracts it from deadline +140.
A nonpositive deadline sets stage timer +144 to that delta. While +144 is at
most the remaining delta, native runs `00444630` with the old timer, runs the
stage exit, increments +148 only if callbacks did not change it, then runs
entry. Entry increments +144 by 1 before its stage-specific action. A zero
timer after entry becomes 5000; a deadline below the timer becomes timer + 250.
The remaining nonzero delta runs `00444630` and then subtracts from +144.
Arithmetic preserves native DWORD wrapping and signed comparisons.

The ordinary first tick cannot discard deadline allocation history. Let
`q = microseconds >> 10` and `D' = wrapping(deadline_140 - q)`. The scheduler's
ordinary clamped input gives `q <= 122`. Constructor stage 0/timer 0 reaches
helper initialization in either branch, which overwrites elapsed +138 with
zero; its preconstructor value does not affect this ordinary path. Stage 1
entry writes timer 10000, but the first tick's remaining delta differs:

| Signed initial deadline branch | Timer before stage 0 exit | Remaining delta after exit | Stage 1 timer at tick return |
| --- | --- | --- | --- |
| `D' > 0` | 0 | `q` | `10000 - q` |
| `D' <= 0` | `q` | 0 | 10000 |

Both branches retain `D'` when it is at least 10000, otherwise repair the
deadline to 10250. Stages 1–4 use fixed timers and the ordinary deadline
repair; stage 5 explicitly overwrites the deadline with 4000. The initial
branch therefore shifts the timer carry and subsequent transition phase,
even though the first helper geometry is identical. This analysis supplies no
clock default and does not authorize an arbitrary resume inside a later stage.

### FreshIntroDeadlineApproximation

The production attachment applies the named `FreshIntroDeadlineApproximation`
once, only to an untouched stage-0/timer-0 controller with null helpers and
unknown +138/+140/+150. It supplies **+140 = 0**, selecting the nonpositive
deadline branch above. This is a replaceable allocation policy, not a captured
value or a retail constructor write, and it never counts as retail acceptance.
Stage 1 initially returns timer 10000/deadline 10250 on that branch.
**The existing retail recording disproves zero on its initial controller.**
The accepted first-load and allocation evidence
establishes the actual `00444944` load at `417AEA:C86`: controller
`038295F0`, +140 = `3F4D0000`, raw delta 125000 microseconds, quantum 122.
The same tick returns stage 1, +138 = 0, timer **9878**, and deadline
`3F4CFF86`. Thus the approximation's first timer differs by 122 native timer
units; matching helper geometry does not establish matching sequence timing.

Both retail allocator paths leave payload history unowned: factory case
`0046187E` requests 0x15C bytes through `00511708`; `0051247C`/`005124A8` use
the small-block allocator `00515BFC` or `HeapAlloc` with flags zero. The
small-block allocator writes block/list headers and can reuse existing free
payloads. A committed virgin page being zero does not establish the contents
of every reused allocation. The allocator's separate zeroed metadata allocation
does not zero selector 88's payload. Static authority is
bulk/crt_runtime.c and the corresponding retail instructions.

For this allocation, bounded history identifies the last earlier payload
writer as texture-atlas `0050A28D` at `2A4689:A7E`: normalized Y
`(204+1)/256` writes float32 `3F4D0000` to `0382971C+14`, the bytes later
read as controller +140. All 539 indexed CPU accesses in the bounded interval
were counted independently (524 reads, 15 writes); no intervening CPU write
precedes the first controller load. The actual 0x15C request at `00461883`
uses a rounded 0x170 small-block allocation: free base `038290AC`, size
0x6B0, bin 0x3F, tail split `base + size - 0x170 + 4 = 038295F0`.
The residue is already present at both small-block and operator-new returns.
This proves shared atlas payload reuse, not a native positive initialization.

The follow-up owner is a general allocation model that preserves prior
resource allocation/free/coalescing and native payload bytes. Earlier heap
history and page/external initialization remain incomplete. Neither the
observed float word nor a positive sentinel may replace the approximation.
The tracked `IntroDeadline`/`IntroAllocation` producers, exact invocation
bundles and coverage limits live in the linked ledger; no query needs repeating
to recover these conclusions.

The adapter leaves +138 unknown in storage until the reached helper-entry
write. For the ordinary stage-0/timer-0 path, its local preceding addition is
dead arithmetic: `00443B10` overwrites +138 with zero before the first host
query in either deadline branch. This bounded extraction rule supplies no
allocator value to a later stage. **+150 stays unknown** until stage 8's actual
entry writes 2000; arbitrary resumed state is rejected.

After timer work, secondary helper +158 ticks before primary helper +154, both
with the original microsecond DWORD. In stage 13 only, the first player's
actor takes primary-helper position and the ten basis DWORDs returned by
virtual +48, when the returned pointer exists. That pose copy happens after
both helper ticks.

| Stage | Entry timer/deadline and effects | Elapsed work |
|---|---|---|
| 0 exit | Detach/move player; construct two selector 90 helpers; set routes and camera targets; clear primary actor flag 0x20 | None |
| 1 | timer 10000 | Once +138 > 500, primary +13C grows by elapsed / 2 up to 2000; a null primary disposes controller |
| 2 | timer 2500; secondary +13C = 1000, +138 = 50, +140 = 0; activate Primary list | None |
| 3 | timer 2000; activate Secondary list | None |
| 4 | timer 1500; first resolved Secondary becomes camera primary, bytes +184 = 0, +187 = 1; if secondary-helper +140 == 0 set its +13C = 2000 | None |
| 5 | timer 2000, deadline 4000; primary helper becomes camera primary, +184/+187 = 1; set Primary flag 0x20 | Set timer 500; a resolved state 3 Secondary with active derived component sets timer 0 |
| 6 | timer 5000, deadline 6000; nearest active derived Secondary becomes camera primary, +184 = 0, +187 = 1 | None |
| 7 | timer 7500, deadline 15000; primary helper targets successor of route +08; helper +13C = 200, +138 = 500, +140/+142 = 0; copy old camera primary to secondary; install helper primary, +184 = 1; activate main reference | Grow helper +13C by elapsed up to 1500; any resolved state 3 Secondary resets timer 2000 |
| 8 | index +14C = 0, sequence timer +150 = 2000, timer 2000, deadline 10000 | If sequence remains, timer 5000 and subtract elapsed from +150; strictly negative +150 retires the next resolved state 3 reference through `0047CE90`, sets +150 = 2000 and advances index |
| 9 | timer 3000, deadline 10000; helper targets Exit row 0, +13C = 0, +140 = 0, +138 = 50; present route +0C replaces helper route and camera secondary; activate Exit list with membership enabled | Grow helper +13C by elapsed / 4 up to 500; helper +140 != 0 sets timer 0 |
| 10 | timer 7000, deadline 10000 | Set timer 1000; same helper elapsed / 4 and arrival gate as stage 9 |
| 11 | timer 7000, deadline 12000 | None |
| 12 | No specific entry/exit action; inherits the entry increment | Grow helper +13C by elapsed up to 2000; helper +140 == 0 resets timer 750 |
| 13 | No specific entry/exit action; inherits the entry increment | Grow primary +13C by elapsed * 2 up to 2000 and copy to secondary +13C; player pose tail described above |
| 14 | Both entry **and exit** call `00487560(18, 1, 0)` when world exists | None |

Activation queries actor virtual +58 and skips a null derived actor. Its
negative signed DWORD +144 delay is multiplied by -1000, then flags +2C are
masked with 0xFB9FFFFF. Exit-list activation additionally enables the derived
actor's flag 1 membership before delay/flag writes. Stage 5's active component
test is separate from stage 7's state 3 presence test. Actor +C4 is a signed
lifecycle state, independent of the constructor selector or actor +B4.

The [actor behavior owner](ACTOR_BEHAVIOR_RUNTIME.md) now owns common
`004294A0` selection, timer/RNG policy and synchronous arena writes with
required descriptor callbacks. Stage 1 has no actor-task gate; stage 5's
active +22C byte is written by a resolved Attack target, while stage 7 tests
Secondary lifecycle state 3. The class-specific water/status/delay prefixes
have [source owners](ACTOR_BEHAVIOR_RUNTIME.md#enclosing-actor-update-prefixes).
Their motion tails and allocation history +230/+220 remain open, so this
dispatcher alone does not establish
automatic actor activity or complete cinematic progression.

Stage 6 uses wrapping signed-short position differences, then a wrapping
DWORD sum of squares. If its high bit is set, `0048EFC0` receives the logical
sum >> 2 and its result is doubled; otherwise it receives the sum directly.
The integer-square-root helper returns 0 for a signed nonpositive argument.
Only a strictly smaller distance replaces the target, preserving ties.

The focused [tests](../../crates/inf-game/src/intro_sequence_tests.rs) pin
callback order, timer asymmetries, strict sequence retirement, mutable count
rereads, signed delay conversion, shortest-distance policy, helper tick order,
stage 13 pose tail, duplicate stage 14 completion, and partial effects on host
failure. These are policy tests, not cinematic acceptance.

`0047CE90` admits only +C4 states 2/3, then calls `0047D670` with target state
1, source equal to the process null handle, and argument zero. It does not
activate tasks. The common owner in
[state_change.rs](../../crates/inf-game/src/object_runtime/actor/state_change.rs)
publishes each decremented state before callbacks: state 2 runs attribution
unless virtual +68 excludes it, conditionally records placement death through
virtual +58, then sets flag 0x8000; state 1 clears health and invokes virtual
+AC; state 0 invokes virtual +B0. Upward passage through state 2 revives only
zero health and clears 0x8000. Downward comparisons reread callback-mutated
+C4; errors retain reached state and earlier effects. The arena classifier,
borrowed mission `record_0c` statistics owner and placement bookkeeping now
back production retirement. The statistics record at mission W+0C is the
native world G+1BC allocation, not a second record. A null source resolves to
category 4; it never borrows the target category. Class-specific callbacks
remain required host effects, so this head alone does not establish complete death
or retirement behavior. The definition of `0047CE90` is in
bulk_clean/fgdk_engine.c.

## Effects and remaining live owners

`00462750` constructs selector 90 via `00462660`, vtable `00526B50`. Its constructor
sets +138 = 256, +140 = 1, +142 = 1 and clears +13C/+141/+144/+148/+14C/+150/+154.
`00462CC0` replaces helper +150 with a retained target accepting a common-object
identity; `00462D40` replaces helper +154 with a tracked actor target made by
`004825D0(handle, null)`. Both use the
[shared target resource owner](TARGET_RUNTIME.md), retaining common-object
identity and actor handles as separate domains. Installation releases the old
allocation before constructing its replacement; the old raw reference remains
in the slot until the new store, without resetting arrival or fractional-motion
fields. The movement kernel currently admits route-node motion separately;
constructor acceptance alone does not establish movement for every common object.
The [selector 90 owner](../../crates/inf-game/src/object_runtime/actor/selector90.rs)
and its [motion kernel](../../crates/inf-game/src/object_runtime/actor/selector90/motion.rs)
now implement these data-bearing lifetimes and the retail virtual +24 at
`00462BA0`, which is absent from the bulk decompile. Factory `00462750` passes
H13 resource argument **142** to `004600F0`/`00460120`; 142 is not a constructor
selector. The canonical world constructor dispatcher admits selector 90 using
shared payload `0047AD70`, shared ordered commands `0047ADA0`, postinitializer
`0047AAD0`, actual creation-listener dispatch, network +40 and the initialized
flag. Its virtual +18 `00404610` returns self, so ordinary actor listener
notification applies. The corpus test resolves resource 142 only after world
activation through leaves 48 and 32.

`004627F0` advances only with a present route target and unsigned scheduler
microseconds >= 1000. It uses live route-node coordinates, not the target's
cached +08 point. It selects the successor when distance is strictly less
than `max(signed +138, 256)`, or when the unsigned squared distance to that
successor is strictly less than the current route's +44 squared segment.
Replacement `00482860` publishes the changed route identity before copying
its cached point; identical identity succeeds without refreshing that cache.
For an open tail, signed speed +13C is capped to
`distance * 1000 / floor(microseconds / 1000)`, with DWORD wrapping before
signed division. Inside +138 it is also capped to
`max(+13C * distance / +138, 32)`. Route direction uses `00417DA0`'s signed
WORD normalization with retained length precision.

The process DWORD **00538BC0** selects two explicit integration policies;
the executable's initial data value is 1. In either branch, the speed/time
product wraps to a DWORD before unsigned division. Nonzero accumulates
`floor(product / 1000)` times the stored direction in signed +144/+148/+14C
thousandths, truncates each accumulator's signed division by 1000 into the
position WORD and preserves the signed remainder. Zero integrates
`floor(product / 1000000)` after low-WORD truncation and sign extension,
leaving remainders untouched. Distance <= 2 snaps to the route position,
clears velocity/remainders and returns whether a successor exists.
`00462BA0` writes +140 = 1 and clears velocity when already stopped or when
advance returns false; even a submillisecond rejection takes that branch.

The tick's facing tail always refreshes a present +154 target through
`00482690`, then normalizes its wrapping signed-WORD displacement with a
spilled f32 length and calls `00490370` with process up-vector `0055F280`.
The shared signed-WORD normalizer now selects this explicit
[`NativeLengthPrecision::Spilled`](../../crates/inf-formats/src/native_basis.rs)
policy; route normalization retains its distinct denominator precision.
Both the settings +7C value and up-vector remain explicit host inputs. A
never-resolved target's unwritten focus fails at that feature rather than
inventing a point. The [focused tests](../../crates/inf-game/src/object_runtime/actor/selector90/tests.rs)
cover replacement custody, fractional/whole integration, unsigned product
wrapping, live successor selection, approach cap, arrival and stopped facing.

Camera `00410E70` replaces primary +18C, `0040F710` replaces secondary +190 and
`00410EE0` applies `00410EC0` to both parameter blocks. These are required effects
in the kernel, not successful defaults.

The [arena intro host](../../crates/inf-game/src/object_runtime/actor/intro.rs)
connects intrinsic script queries/effects to those live owners: membership,
flags, visual WORD/dirty writes and direct `0047A110` refresh, helper targets
and movement, camera targets/parameter mode, derived delay +144 and the ten
basis DWORDs copied after helper +48. Helper/camera factories, retirement and
mission completion remain required services. Reached controller fields are
published after success or failure; callbacks that inspect or mutate those
controller timers during the kernel call require synchronous publication
before that alias domain can be accepted.

[NativeIntroServices](../../crates/inf-game/src/object_runtime/actor/intro/admission.rs)
now owns the canonical helper admission request and delegates camera lookup
and on-demand admission to the existing `00478E00` owner. It requires separate
helper/camera allocation requests, real math, settings, local/network state,
and the live creation-listener list and callback implementation. A nonempty
unsupported listener remains an error at its invocation. Retirement, world
presence and mission completion use a required `IntroCallbackHost`; no actor
task or retirement callback has a successful fallback.
The public admission factory phases let production borrow current terrain only
for a reached helper/camera call. A later admission therefore observes any
preceding retirement mutation, without cloning terrain or retaining a borrow
across the mutable world-effects phase.

The production [runtime adapter](../../crates/inf-game/src/session_platform/intro/runtime.rs)
is called by `PortSession`'s Gameplay tick. Native `004875A0` first updates
the shared world timing ring, then calls `00439560` before mission/object/camera
work, so the adapter advances the existing shared
`AnimationClock` and uses its current `005471C0` value for helper admission.
It does not reuse the loader's captured clock receipt. SDL supplies live
settings +68/+6C/+7C through required `world_tick_inputs`; native CRT up vector
`0055F280 = (0,1,0)` and executable motion switch `00538BC0 = 1` retain their
source-derived initial values. Process camera globals and inverse-sine table
custody survive world reloads. Shake switch `005483B0` starts at BSS zero;
its later native event writers remain unowned, but none runs in Infest01, so
the intro never shakes ([camera shake](CAMERA_RUNTIME.md#process-initialization-and-shake)).

The callback-loop portion of `00478F20` moves each flag-1 node to the front of
the secondary owner **before** dispatch, observes same-pass membership
changes/admissions, then swaps owners, so each frame runs the previous
frame's order backwards. The controller's native +24 runs through the arena host;
its own callback ticks secondary helper before primary helper. Admitted family
`+24` callbacks now execute the [shared activation-delay prefix](ACTOR_BEHAVIOR_RUNTIME.md#production-activation-delay-service)
with actual process inputs and delayed physics in every loaded world,
independently of this controller or constructor-93 presence. Negative/positive delay returns
remain eligible on later visits, including same-pass Exit-list admissions.
A reached missing service or later class continuation preserves preceding
writes and stops that actor feature with a deduplicated reason. After actual
+D8 completion, Selector10's [shared post-D8 continuation](ACTOR_BEHAVIOR_RUNTIME.md#selector10-post-d8-continuation)
refreshes retained targets, computes controls and reaches shared avoidance in
native order. Only a normal return enters actual `00425400` contact, alignment,
forces, physics and the installed helper, followed by its persistent sound
record phase. Required firing-service, vector, helper or contact backing preserves
prior writes and stops that feature. These owners serve every loaded world;
intro placements do not select their policy.
Selector36's [shared post-D8 continuation](ACTOR_BEHAVIOR_RUNTIME.md#selector36-post-d8-prefix)
consumes actual148 refresh/status, ordered target geometry/aim, shared weapon
position, packed counters and WeaponTick before fresh148 avoidance. Required
backing/effect/fire/query failures preserve their reached prefixes; only normal
return enters the [class36 Motion481C10 owner](ACTOR_MOVEMENT_TAILS.md#selector36-contact-alignment-and-motion)
with [shared terrain contact](ACTOR_CONTACT_RUNTIME.md), actual alignment,
motor and physics, then the [shared481B10 record owner](AUDIO_RUNTIME.md#class36-persistent-actor-sound).
A normal audio return completes this class36 virtual+24; persistent playback
and other reached actor boundaries remain required. This source ownership does not by
itself establish loaded actor progress or retail intro acceptance.
The [selector60 row owner](SELECTOR60_RUNTIME.md#update-and-spawn-ordering)
now dispatches its normal update and dynamic placement factory through the
same live H14 commands, terrain, RNG and post-load callbacks as world loading.
The concrete terrain-light `00405140` callbacks also leave the active list in
native order, and selector16 `0047AD30` advances its unsigned countdown
independently of the later camera-pose phase. The same countdown is the
`+24` of the shared table `00527C00`, which selector 6's plain actors (the
`flag01` flags) keep, so they run it too. Other `+24/+3C`
callbacks are skipped and logged on first reach, including later admissions.
Disposal, player selection/reordering/post-update, `004791F0`, mission work,
collision and the task-manager pass remain distinct logged skips. The separate
[shared camera phase](CAMERA_RUNTIME.md#acceptance-and-external-boundaries)
then runs `00478E00/0040FD10` for every loaded world, including ordinary missions
without this controller. Its acquired object identity survives a pose error;
world draw reads current arena XYZ and all ten basis DWORDs with coordinate
shift 16, rather than a cached successful pose. Each world Draw freshly
reacquires through the shared native factory inputs before fade/material and
late camera-field publication, including before the first Tick. The retained
NULL-context feature and atomic projection-setup prefix remain explicitly
bounded in the camera authority. The world consumes
source-owned [visibility and predraw slew](WORLD_VISIBILITY.md) with that view. Manager
`0048E2E0` reads the timing ring's process feedback before actor/camera work;
`00488380` then supplies bounded world fade. The former frontend fade wrapped
the terrain footprint's WORDs. Corrected visibility permits ground commands
through the shared terrain bridge without relaxing native quad rejection.
The retained final-draw receipt reports actual eye/basis, projection, height
and per-feature outcome independently from the once-per-world first-frame log.

Required controller-effect failure preserves all reached actor/controller
writes and stops that controller feature instead of retrying a partially
executed stage. Camera errors preserve their reached arena/global/RNG writes
and report the missing feature. The focused production corpus test
`production_intro_admission_moves_authored_helper_and_publishes_camera_pose`
exercises actual retained mission-22 routes, canonical resource-142 admission,
helper movement and pose publication; tests and connection alone do not
establish a moving retail cinematic comparison.

The [corpus test](../../crates/inf-game/src/object_runtime/actor/intro_tests.rs)
pins the authored mission-22 references independently of +C4 lifecycle state:

| Script list | One-based placement references | Full H13 argument | Constructor selector | Derived virtual +CC |
| --- | --- | --- | --- | --- |
| Primary | 48, 49, 50, 51, 83, 84 | 0xFFFF0003 | 12 | `00404610`, self |
| Sequential | 30, 31 | 0xFFFF000B | 11 | `00401300`, null |
| Secondary | 68, 69 | 0xFFFF0057 | 10 | `00404610`, self |
| Secondary | 52, 79, 80 | 0xFFFF006C | 36 | `00404610`, self |
| Exit | 97, 98 | 0xFFFF0091 | 35 | `00404610`, self |
| Main +10 | 53 | 0xFFFF002D | 60 | `00401300`, null |

For self-returning +CC, the gate reads that actor's owned family-428650 byte
+22C. It does not resolve a second component allocation. Supported derived
virtual +58 returns self; shared/16/30/88/90 return null. These static vtable
queries establish the gate, not task work that later changes +22C.

The following owners remain necessary for cinematic acceptance:

- The complete animated sequence with selector77's connected target simulation.
  The eye/scaffold appearance was accepted against retail; preserve it.
  The [target update](ACTOR_BEHAVIOR_RUNTIME.md#selector77-null-target-update)
  now owns both NULL and refreshed nonnull target routes. The actual
  [eye capture](OBJECT_MODEL_CALLBACKS.md#selector77-eye-capture) and
  [Common inline transform](MODEL_RUNTIME.md#common-inline-transform) supply
  shared draw phases, including the source-controlled middle-span pose;
  source-owned callbacks and that appearance check do not establish the complete
  animated sequence.
- Exact allocator history for initial +140 and the timer carry it changes;
  arbitrary callback aliases that inspect/mutate local controller timer state.
- The shared [production behavior host](ACTOR_BEHAVIOR_RUNTIME.md#production-behavior-services)
  now reaches genuine +D8 with retained targets/process services. Required
  lower callbacks and class movement tails still gate referenced actor tasks
  that change lifecycle +C4 and active byte +22C, plus manager
  mission/collision/player/task ordering around the subset. The
  [load-time furniture-radius publication](TERRAIN_RUNTIME.md#shared-furniture-radius-publication-004508e0)
  supplies retained `005483C0` from actual H12/H11/H8. The shared
  [collision entry/scan](FURNITURE_COLLISION.md) consumes the real 429EE0 miss
  through actual installed-cache custody and retained process state. Its
  [connected ACTIVE owner](MODEL_PROCESS_GRAPHICS.md) consumes genuine shared
  projection/table history and executes its explicit auxiliary domain; excluded
  operations and dependent contact responses remain typed boundaries.
  Class36 consumes shared terrain contact,
  alignment, motors and physics through Motion, then its shared persistent
  record callback and shared persistent SDL playback. The native
  [audio resource/device lifecycle](AUDIO_RUNTIME.md) is connected; complete
  cinematic callbacks and audible comparison remain open.
  The normal 650-frame New Game release route uses the selected
  [extended fresh-allocation approximation](../../crates/inf-game/src/fresh_heap.rs)
  for derived DWORDs `+14C`/`+150`, fresh helper DWORD `+00`, family DWORDs
  `+18C`/`+19C`, turret `+19C` and gate `+158`;
  it does not prove native initial bytes or supply equal-helper DWORD `+04`. The prior preferred
  `+14C`, Attack helper `+00` and class12 empty-update `00435170` stops are no
  longer recorded. Placements67/68 (class10) and51/78 (class36) now admit
  the [shared firing chain](WEAPON_FIRING_RUNTIME.md), including both draw
  consumers and reached factory/audio services. With `+19C` supplied, the
  class35 ships dispatch: refs 60h/61h follow native Roam/follow-trail
  selection. The [world contact pass](WORLD_CONTACT.md) now holds them on
  the terrain, and impact damage reaches live `0047C6E0`; their earlier
  `+260` stop is no longer reached ([evidence](SELECTOR35_RUNTIME.md#remaining-boundaries)).
  Their furniture contacts reach the shared `0047BFD0` response, and the
  pair pass `004790C0` exchanges momentum between colliding actors. The
  later ships score escorts through the owned `00428990` source and run the
  shared [Herding](ACTOR_DESCRIPTOR_CHANGES.md#shared-herding-change-and-update)
  descriptor. The two selector34 `pership` placements run their
  [own update](FAMILY428650_RUNTIME.md#selector-34) prefix, `+D8`, `+110`
  bias, Herding, their own Attack entry/update and class tail, and a
  [selector67 ribbon](SELECTOR67_RUNTIME.md) trails each. The
  [current behavior route](ACTOR_BEHAVIOR_RUNTIME.md#selector10-post-d8-continuation)
  owns these bounds; changed descriptor selection is not proof of every callback.
  The [test-only miss-stage observer](ACTOR_BEHAVIOR_RUNTIME.md#test-only-first-miss-provenance)
  separates stopped requests, actual inner normal returns and post-step
  cache/epoch/radius joins. Its current loaded producer passed all six
  armed/unarmed parity checks across three resource epochs, recording ten
  inner normal returns, including Selector12 placements48/49/50/83, and no
  stopped-callback packet. The viewed normal-release frame650 presents terrain
  and furniture; that portable result is not matched retail acceptance.
  Historical stopped-prefix
  receipts remain distinct. The same
  [retained fallback](TERRAIN_RUNTIME.md#retained-furniture-query-process-state)
  is consumed without per-query zero or default clear results. These bounded
  executions do not establish native contact fidelity or complete intro acceptance.
- Class-specific retirement beyond selector-11 `00402250` and shared family
  `00428140`, and
  the retained-manager cleanup walk that temporarily inhibits
  placement bookkeeping through mission W+08.
- Selector-2 special-view activation and stored-vector writers: the
  [camera reader](CAMERA_RUNTIME.md#selector-2-special-view-storage-and-reader)
  owns `00421D50` with known inputs and preserves required-read prefixes;
  fresh storage initialization remains unowned. Shake-event writers, broader
  camera settings paths and matched world presentation remain acceptance work.

For sequential selector 11, retail vtable `0051F080 +68` returns null
(`00401300`), and +AC is **`00402250`**, not an inherited empty callback.
`0047CE90`/`0047D670` now records target/source attribution in the retained
mission W+0C/world G+1BC matrix, including null-source category 4. It retains
the captured statistics owner for the separate +154 selector-18 count.
Derived +58 enables mission `0048EAD0` with the actual W+08 inhibit byte and
zero force argument. The head sets flag 0x8000, then reaches **state 1 with
health zero** before required +AC. Production failure at +AC preserves these
statistics, placement and actor writes and stops that controller feature.

`00402250` first calls world `004887D0` with actor +1A0, a pointer to its
position WORDs and the original source handle, unless flag 0x80000000 is set.
The ten-byte +1A0 block has constructor defaults and may be replaced by
authored command 20; constructor byte +1A3 remains allocator data until that
command supplies it. The actual mission-22 sequential actors at one-based
placement references 30 and 31 in `32.ovl` have only commands `[2, 6]`, so
neither initializes +1A3. The corpus test
`mission22_sequential_actors_keep_unwritten_byte_1a3_without_command20`
preserves `None` for both and pins the lookup identity and known constructor
block fields; it disproves an authored-byte shortcut on this path. This byte
does not block the newly owned furniture damage consumer: `0042A8F0` reads
only strength and channel, preserving byte +3 opaquely through effect copies.
`004887D0` first enters terrain/furniture effect
`00436390`, then manager radius-effect dispatch `00479870`, and optionally
emits network event 0x13. These are required mutable world services, not an
empty callback. The source-backed terrain traversal and shared falloff now
have owners in [ACTOR_RUNTIME.md](ACTOR_RUNTIME.md#selector-11-retirement-terrain-effects),
with live source rereads and mutable H12 custody. The generalized
[world-radius dispatcher](../../crates/inf-game/src/world_radius_effect/dispatch.rs)
preserves that order and original block/position identity. The
[production retirement host](../../crates/inf-game/src/session_platform/intro/retirement.rs)
composes the current H11/H8 resource, terrain, actor damage, process task,
particle, sound, statistics and placement owners. Network event 0x13 delivery
remains a required error when actually admitted; ordinary self-damage does not
increment its alternate-hit gate. No successful placeholder advances a missing
service.
`00436020` damage admission and the actual `00436A60` pending queue now have
a shared data-bearing owner. The EXE descriptor and channel-specific
resistance rule select the threshold/shared-RNG gate; accepted damage keeps
furniture present and queues original coordinates, ground Y, program identity
and native timer fields in append order. The resistance table at `00546E38`
is proven original file-backed zero data, establishing initial contents rather
than any later writer. Unmapped addresses remain errors. The canonical
`retirement_radius_blocks` corpus fixtures isolate both blocks: mission-22
references 30/31 reach no furniture model or damage-rule call. In the currently
supported mission-18 arena, all fourteen independent scans of its seven
selector-11 placements reach only themselves; reference 93 reaches lookup 51,
whose genuine H8 null result admits no furniture damage or queue record.
These fixtures exclude moved/scripted positions, omitted actor classes and the
intervening particle burst. The later `00436700`
executor now owns the pending queue's signed timer gates, program traversal,
saved-next ordering and five settled queue/terrain callbacks. `0048770C`
passes raw microseconds; the queue's +14/+20 fields and authored program
delays use those units, including `00437780`'s seven-second timer. H11 callback
userdata retains a host-issued allocation identity across the burst. Missing
program memory and the remaining particle/world-effect
callbacks remain required services. Resident H8 lookup preserves signed bounds,
its genuine null loader and the unsafe inclusive-count boundary. Furniture
audio `00436DE0` enqueues current stack XYZ into the shared positional sound
owner; playback occurs later. The queue runs before actor callbacks and clears
through `00450210` after mission W+12 is cleared, before the separately required
player/task cleanup. Reached timer/cursor and callback writes survive failure.
Details and callback boundaries are owned by
[TERRAIN_RUNTIME.md](TERRAIN_RUNTIME.md#pending-furniture-effect-executor).

The following `00479870` actor-radius manager now has a source-backed kernel
and meaningful callback/impulse tests in
[world_actor_radius_effect.rs](../../crates/inf-game/src/world_actor_radius_effect.rs).
It preserves the cached unsigned radius square, direct signed `0048EFC0` and
signed low-AX length, live source reads, current alternate flag, post-damage
mass reread, saved difference/current-basis impulses and the native linear
then angular publication order. Its return value counts alternate hits only.
The complete outer `0047C6E0` now has a
[shared damage owner](ACTOR_RUNTIME.md#shared-instantaneous-actor-damage):
virtual +B4 runs even for zero damage, before live flag/task/source/statistics
effects. Shared `0047CBC0`, status `0047CAE0`, and derived `004283E0` also have
[reaction owners](ACTOR_RUNTIME.md#shared-and-derived-damage-reactions).
They preserve reached actor writes and the recent-source order: +64 identity,
clock, +5A8 timestamp, then +5A6 owner WORD only for unsigned elapsed time
below 200. The family `00428040` wrapper, four-entry source table `00482E30`,
nearby scan `004281B0` and family +D4 `004282C0` also have an
[owned response kernel](ACTOR_RUNTIME.md#family-damage-response-and-target-notification).
It preserves the friendly-source early return, actual +114 dispatch, reached
packet-channel reads, shared RNG branch and signed channel-8 timer floor.
Selector 12's +114 is genuine RET; selectors 10/36/35 reach `0043F3D0` and
their common virtual +100 target-scoring/resource body now has a
[shared selection owner](FAMILY_TARGET_SELECTION.md). Production binding and
selector 12/13's distinct +100 remain separate work.
[Nearby retaliation and family target reaction](ACTOR_RUNTIME.md#nearby-retaliation-and-family-target-reaction)
also have concrete owners for the proven non-destructive +58/+CC/+D4 domain,
including independent byte-radius gates and exact one/two-draw target timing.
Their broader production composition, lifecycle callbacks, source notification
and network delivery remain required services when those branches are reached.
The selector-11 StateOne attachment uses the actual derived response rather
than substituting these live-health callbacks.

The process [task-storage owner](ACTOR_RUNTIME.md#process-task-storage)
now retains the actual nineteen task allocations in `PortSession` across
world reload. Constructor `004829A0` initializes each task's +8 from the
process-null handle and publishes first-owner-wins slots; `00482C60` owns
the live guarded setter. Constructor-unwritten counter words and separate
task-manager simulation remain explicit. The initial authored radius scan
for each sequential selector-11 actor reaches itself; flag 4 remains set
after retirement, and fixed flag 8 suppresses impulses. Constructor strength
2000 gives channel-9 damage 1000. The second EXE block's strength 1000 equals
the authored channel-9 threshold 1000, so it adds zero damage/statistics, while
the unconditional +B4 still runs. At reached lifecycle
state 1, selector-11 +B4 executes the derived activation prefix but does not
enter shared live-health reaction. The subsequent `0047C6E0` task lookup
requires the real class-2 allocation for nonzero damage even with null source;
its null-source setter branch reads no field and nonzero damage reaches
the separate W+0C damage matrix. The reusable
[ProcessActorDamageHost](../../crates/inf-game/src/process_tasks/damage.rs)
now supplies those concrete task/statistics reads around required real +B4
and source +108 services. Its canonical
`mission22_retiring_actors_each_self_hit_with_real_task_source_and_damage_statistics`
fixture executes both initial scans through the actual derived response.
The production adapter uses that same response; live-health and source +108
services remain errors if unexpectedly reached.

After that first call succeeds, +AC submits `0048A8B0` particle/effect data,
rereads the alternate flag, and conditionally calls `004887D0` again with the
ten-byte EXE table at `00531250`. It then writes WORD +1B2 = 2000, calls
`0047E190(2)`, and records placement death. `0047E190` changes H12 terrain
cell flags: after its height gate, argument 2 executes three RNG-selected
cell operations clearing the high nibble. The
[selector-11 retirement owner](../../crates/inf-game/src/object_runtime/actor/derived/selector11/retirement.rs)
now executes this complete ordered +AC, preserving all reached writes on a
required-service failure. Thirteen
[original-bytecode cases](../../crates/inf-game/tests/fixtures/selector11_retirement_native.json)
from the tracked checker pin supplied-
service order, flag/position rereads and failure prefixes. The
[production tests](../../crates/inf-game/src/session_platform/intro/runtime/tests.rs)
complete both initial authored retirements with actual tasks, particles,
statistics and queued sound, and verify that a missing task preserves the
earlier prefix before the burst. These checks do not establish a live
cinematic. `0047E190` is not a task setter. The local
effect record's WORD +0A is unwritten stack data and must not be silently
initialized. Neither reached particle birth consumer reads that WORD;
its absence alone does not block those calls. Referenced actor task gates,
broader callback ownership and matched scene evidence still gate cinematic
acceptance; native Escape completion remains available.

### Retirement particle admission boundary

Native `00402292` passes the constructed 32-byte record in ECX; its start is
the current `ESP+0C`, not the enclosing function's stack base. The record is:

| Record offset | Width | Value or source |
|---|---|---|
| +00 | WORD | Debris count 16 |
| +02/+04/+06 | WORDs | Current actor XYZ |
| +08 | WORD | Placement radius 20 |
| +0A | WORD | Unwritten; no reached birth consumer reads it |
| +0C | DWORD | Central particle type 1 |
| +10/+14 | DWORDs | Even/odd debris types, both 1 |
| +18 | DWORD | Speed 8; arithmetic uses its low WORD |
| +1C | DWORD | Trailing flags 0 |

`0048A8B0` snapshots XYZ before calling debris producer `0048A720`, then
calls central producer `0048A0F0`. At `0048A8D3`, ECX receives record +0C;
EDX points at the saved XYZ, the first stack argument is a null velocity
pointer and the second is +1C flags. The shortened decompiled call omits
these register arguments; +1C must not be mistaken for the central type.
Debris alternates +10/+14 types at `0048A792/0048A79B`.

Debris count is signed `wrapping(16 * 0055F154) >> 16`, falling back to 16
only when the result is zero, and entering the loop only when positive.
That load factor belongs to the eight-sample timing ring in `004875A0`, not
the current delta or a graphics-quality setting. The normal unsigned average
below 34,464 microseconds yields 65,536; otherwise the factor is
`100000 - min(average,100000)`. A second timing argument above delta seeds
all eight samples from that argument and publishes factor zero.

Admission requires the actual process particle manager (`G+80`, `0054ECB4`),
its phase-4/network gate and the 400-slot, 72-byte pool. `00466100 → 00464ED0`
returns a free/reclaimed slot or native nonnull scratch `0054EC60`;
Base constructor `00464FB0` calls `00464E50`, whose pending publication is
guarded by allocation marker +128. The retained vector cursor `0054EC5C`
increments as a wrapping DWORD; lookup uses modulo 100 into original
file-backed table `00539960`. Each `0048A3A0` debris birth consumes three
RNG draws; type-1 `0048A0F0` consumes five for each of eight instances.
The generalized [process particle owner](PARTICLE_RUNTIME.md) now owns the
pool, scratch/reclamation policy, pending/active lists, all six birth
selectors, same-tick lifetime/update and central/debris terrain collision
against actual executable tables and the shared RNG. Trails and splashes
are real admitted particles with their own retained callback policies. Record
`+14` is the process-null ActorHandle `0054E894`, not a terrain pointer;
intrinsic collision uses resident H12/H14 and actual H15 slot-0 impact
selectors. Required audio remains an error when absent.

Timing differs from furniture effects: `00436700` executes at `0048770C`
before the actor walk, so furniture admitted by retirement starts at age zero
and first advances next tick. Its +0A is explicitly written terrain Y and
is unrelated to the retirement record's unwritten +0A. Particle births enter
pending lists, transfer through `00464DA0` at `0048778D` after actor/collision/
camera work, then update through `00464B30(delta)` at `00487795` in the same
tick. Particle-instance +0A is explicitly constructed X velocity; +10 is an
unsigned WORD lifetime decremented by signed `delta >> 10`.
The timing-ring head is production-attached through the shared
[world visibility owner](WORLD_VISIBILITY.md). `PortSession` now retains the
process pool across screen/world changes, clears it directly on world clear
and attaches the post-camera transfer/update boundary. World draw attaches
active and pending animation draws, real terrain-light writes, transfer and
the later positional sound-list service before terrain. Impact updates enqueue
copied world coordinates for actual portable PCM output through the
[one-shot audio owner](AUDIO_RUNTIME.md#process-positional-one-shots). The
same request contract is used by selector-11 retirement births. Sound enqueue
retains current H9 custody and copied XYZ; resource-alias playback and its RNG
draw happen at the later sound-list service, after particle transfer/draw.
Deferred particle quads share the model/terrain queue; timing markers do not
reset it. The flag-8 shadow branch fails per particle after its main quad and
is absent from the currently authored producer animation tables.

Original-bytecode receipts verify bounded birth admission, all owned particle
updates/terrain contact, direct clear, active-tail reclamation, retained
list/RNG state and 101 supplied-input draw cases. Production StateOne now
admits the actual retirement burst after its first world-radius call, and
pending births feed the post-camera transfer/update and shared world draw.
The thirteen +AC cases verify the enclosing source kernel with supplied
services; the production tests cover initial-position composition. Matched
moving cinematic comparison remains a separate acceptance gate. Referenced
task +22C and lifecycle gates are not
fabricated to advance stage 8.

`ArcadeDrivePolicy::FrozenIntroControlApproximation` suppresses the existing
arcade vehicle adapter when the loaded arena contains a retained selector-88
controller. The loader's later respawn clears the controller attachment at
player +D8, so checking only the final player attachment would incorrectly
permit steering in the unfinished intro. This policy is derived from the
actual controller, not a mission number. It is a replaceable control
approximation, not recovered native vehicle-input behavior; the follow-up
owner is native player-input/task behavior around the connected selector-88/90
simulation and its camera/player transitions. Its historical name does not
claim that the newly connected helper/camera callbacks remain frozen.

## Native Escape and mission completion

The gameplay screen installs the executable's actual digital table roots.
Nonzero callback `00443700` invokes current-screen virtual +64 = `00443070`;
zero returns. `00443070` clears attract and requests phase 2 when attract was
set, then falls through. On current world 22 it calls `00487560(18, 1, 0)`; otherwise
the player modal/menu branches have their separate owners. The executable
callback boundary is exposed by
[gameplay_screen.rs](../../crates/inf-game/src/gameplay_screen.rs).

[world_completion.rs](../../crates/inf-game/src/world_completion.rs)
implements `00487560`/`0048E820`'s native completion writes. Network sessions return
before cleanup or mutation. Offline runs `004800E0` first: iterate its linked
records and invoke `0047FFA0` for flag +14 bit 2. That linked owner remains a
required host boundary. A supplied actor must match the
[shared player lookup](PLAYER_ACTOR_LOOKUP.md); its live process-null read
follows cleanup and precedes disposal or mission writes. Unmatched actors
are disposed before mission writes. Player `+D8` is authoritative when it
differs from that captured process value; only equality permits `+D4`.
The null-actor intro completion bypasses this lookup and its required input.
Production borrows the retained [process state](PROCESS_ACTOR_STATE.md) for
camera and completion, preserving its startup, screen/world/display lifetime
and explicit unavailable-input behavior. Retirement births borrow the same
owner under the separate
[post-allocation source policy](PARTICLE_RUNTIME.md#implicit-process-source-and-late-failures).
Missing source retains the reached retirement/burst prefix and stops its suffix.

Completion ORs 1 into status +3C[current +A0], writes next selection +30 = 18 and
WORD +A4 = (ordinal 1) - 1 = 0, selects the supplied or first player, then `0046AF70`
clears player byte +ED and DWORD +F0. The second argument is a **start ordinal**,
not a success boolean. Current +A0 remains 22 until the next world-load prelude.
Successful completion submits phase 8 through the existing first-request-wins
session owner. The
[completion tests](../../crates/inf-game/src/world_completion_tests.rs)
pin rejection order, cleanup failure, missing-player writes, and WORD wrap.

PrepareResults `00487540` clears gameplay +5B4 and calls `0048E890`: local
map cleanup/free and mission +04 NULL first, then clear mission +12,
`00450210`, first-player vehicle virtual +64 through `0046CBB0`, `00413A20`,
then `00478CD0` world cleanup. The actual results caller executes the
[map phase](WORLD_MAP_RUNTIME.md), BYTE `+12` store and represented effect
clearing before the [retained-player/manager suffix](WORLD_CLEANUP.md#retained-player-results-retirement).
It captures the first player once, performs both genuine controlled resolves
on the nonnull arm, then the pure `+64` query, optional ordered copy and same
message cleanup before the actual manager drain. Missing data remains lazy;
there is no fallback registry, source or message cleanup host.

The retained world's port-only `owner_retirement` stores `Completed` or a
typed `Stopped` error. A reached failure keeps every earlier write/cache and
destructor effect, is logged, and permits the existing results continuation
without retry. Player FIFO, input references and message identities remain
owned; camera/respawn diagnostics may refer to destroyed actors and are not
normalized into fresh objects. This attachment owns the results caller; the
separate [world load lifetime](WORLD_LOAD_LIFETIME.md) composes borrowed loading
and the admitted deleting LeaveWorld path over the same arena/input/carriers.
Mission/game destruction and process-global shutdown remain distinct. The
earlier gameplay `+5B4` field and full native effects storage retain their bounds.

## Results screen construction

[intro_results_screen.rs](../../crates/inf-game/src/intro_results_screen.rs)
owns phase 8 after the intro and after every world (its
[Tick and input](RESULTS_RUNTIME.md#the-general-tick-and-input)). It constructs a real base
screen, attaches scheduler/registry membership and constructs the actual empty
menu controller before device enumeration (`0042AB9D` -> `00453380`). It reuses
the shared `MenuStack` with fade rate 65535, initial fade zero, active/undrawn
flags and viewport `[0, 0, 1024, 1024]` in authored units. Output resolution
does not alter those constructor inputs. A later constructor failure retains
the controller; teardown releases devices, drops the menu, then releases the
base. Menu entries, ticking and pixels remain separate unowned phases.
The screen binds the authored phase 8 input set, selects the context and
retains a stable screen ID. Its first actual Tick executes the proven
`0042CFC0` head request for phase 3 while current world remains 22, then
completes the [fresh first-pass continuation](RESULTS_RUNTIME.md#implemented-fresh-first-tick)
through actual base input, shared age and process animation. Later passes
and results input run the general Tick.

The constructor's player prefix `0042AD0A..0042AD36` uses the actual retained
arena and [shared player-command kernel](PLAYER_COMMAND_RUNTIME.md). The first
`00488AA0` lookup alone is NULL-tested. An empty list preserves constructor
WORD screen `+144 = 0` (`0042ABD4`) without requiring process state. A present
list performs a second fresh lookup for `0046C760(37h, 1)`, then a third for
the player `+11C` read. MOVSX followed by the AX store preserves all raw
16 bits; the screen owns a `u16`, without score or fixed-point conversion.
The admitted pure lookups and silent command have no mutable lower callback;
raw manager rebinding and dangling storage remain outside this typed prefix.

Input selection `004947E0` follows the completed prefix. A missing reached
process retains the linked Stopped screen and installed inputs, leaves the
previous context selected, and publishes no new registry input binding.
Nonzero full network state bypasses command mutation while the later WORD copy
still occurs. Offline command 37h commits word `+134` bit 23 and silent 1
avoids executable text, H0 and messages. The production factory borrows the
existing live process; it retains `Some(screen)` before propagating failure.

The identity/score arm after device installation uses the exact DWORD
comparison `G+1E0 == 1` (`0042AC94`). This is mission `W+30`, with
`W = G+1B0`, owned by `WorldMission.save.mission_selection`. The constructor
borrows current world and mission selection from the same actual mutable
mission owner. Completion `0048E820` writes the next selection before
requesting phase 8; the intro's selection 18 therefore bypasses this arm.
Neither phase 8 nor current world 22 substitutes for that comparison, and
`101h` does not select it. The typed device host cannot mutate the mission
before the reached gate; arbitrary native-global mutation
is outside this bound.

The screen still skips and logs the identity/score formatting reached when
selection is 1. After input selection it reads the live mode
byte: zero runs [bounded working-profile Serialize](PROFILE_RUNTIME.md#live-working-profile-serialize)
through the actual retained store, mission, current classification and fresh
first-player lookup. Every nonzero mode runs [bounded working-profile Restore](PROFILE_RUNTIME.md#live-working-profile-restore)
over the original source at its emitted cursor, then invokes the existing
PlayerName/Keep identity writer after releasing the store loan. Unavailable
mode/store has a distinct logged boundary. Formatting/source/identity failure
retains reached constructor and restoration effects and permits the screen
to continue to the briefing. No profile file is written by this constructor;
successful Restore updates only the separate identity settings sidecar.
The [intro Draw branch `0042C2B0`](RESULTS_RUNTIME.md#intro-results-draw)
now executes its attract gate, shared renderer preparation, two width reads
and current-world22 exit. It draws no content on that native branch and still
runs in the first Tick's pending-phase3 frame. Any other world's draw goes
on to the [results page](RESULTS_RUNTIME.md#the-results-page). These phase
owners do not establish original
device-error cleanup, active identity/score formatting, or
whole-constructor/native-wrapper execution.

Campaign reload now preserves the actual arena, players, input references,
message identities and constructor carriers through the
[borrowed load owner](WORLD_LOAD_LIFETIME.md). The actual manager still retires
world actors before placement reset; surviving player IDs are not reimported
from a save projection. `IntroCampaignReloadApproximation` retains the distinct
clock/settings/fresh-allocation WORD bounds. The
[complete live player save-copy owner](SAVE_FORMAT.md#live-player-save-copy-owner)
exports every `PlayerSaveState` field from the retained player, including raw
+118/+11E/+120/+124/+128 and all eight +13C bytes. Explicit SavedImport fixtures
restore that shape deliberately; production reload retains the actual player.
Loader +11C reset and the later respawn writer still occur in order, with +12C
set to 3 when +100 bit 3 is set. Completion reset, mission selection/status and
start ordinal remain separate policies.

## Seeded comparison with Infest01

The intro's battle depends on the shared RNG word `0055F18C` when the world
loads, and that word depends on menu time and loading I/O before it. Even the
original intro therefore differs from session to session. To check the port's
simulation, an uncommitted diagnostic run supplied Infest01's own inputs and
compared each world tick with the recording:

- the RNG word `3E23E340` before map construction `00471AE0` (at
  `416FEB:A09`); the port then matched `9AD4E340` after the map and
  `ED4489BC` at the first tick;
- fine terrain detail (settings `+70 = 1`). Infest01 prepared diagonals with
  spacing `0x80`, while the fresh smoke settings use coarse `0x100`, which
  changes the terrain contact of pership `2E` on the first tick;
- the allocation residues the port deliberately does not own:
  - selector 88 `+140 = 3F4D0000`
    ([FreshIntroDeadlineApproximation](#freshintrodeadlineapproximation));
  - family WORD `+220` for placements `53/43/4E/61` and spawned handle
    `08D90001` (`A820/54/B0F0/746C/1690`);
  - family DWORD `+19C` for placement `61` (`00746665`) and spawned
    selector 35 ship `08DD0001` (`00020DED`);
  - family DWORD `+188` for the same two selector 35 ships (`55205550` and
    `51`). When a first dispatch keeps `+184 = -1`, the dispatcher's final
    accumulation reads it. For these ships the next descriptor change zeroes
    it before any decision reads it, so native never uses the value. The
    strict port arena still needs one there.

A spawned actor reads its residues in the tick that creates it, so the run
supplied those at construction. Placement residues were supplied before the
first tick. Family `+18C` residues (`A0` for `08DD0001`, `50430070` for `61`)
change nothing: the comparison matches without them.

The comparison logged the RNG word at every `00478F20` entry (538 intro
ticks), and for a divergent tick every `0048F060` draw interleaved with each
update-walk dispatch. Positions and family descriptor state were compared
at chosen ticks. With every input above, the port matches Infest01 bit for bit
at all 538 ticks, through clock 3362, the last intro tick. Both then load
Level 1. Each row below is the first difference left once the earlier rows'
inputs were supplied:

| First differing tick | Cause |
| --- | --- |
| 6 to 12 (pership `2E`) | Coarse terrain detail in the smoke settings |
| 518 | Selector 88 `+140` zero (the deadline approximation shifts every stage by 122 units) |
| 937 | Family `+220` zero: selector 36 `4E` scans its candidate list one index ahead |
| 1025 | Port bug, fixed: the `004281B0` ally alert compared placement `+2A` instead of resource `+30` |
| 1181 | Family `+19C` zero: ship `08DD0001` spawns at 1181. Native waits out its `00020DED` countdown and picks its first descriptor at 1187; the port's zero picks it at 1181 |
| None | All 538 ticks match |

Matching RNG words cannot show state that never consumes a draw. At clocks
1800, 2400 and 3356, the comparison therefore recorded every object in the
update walk before its callback: handle, position WORDs `+32/+34/+36`, health
`+A0` and lifecycle `+C4`. Walk order, positions and lifecycles match for all
41, 35 and 36 objects, and so does health, except selector 36 `4E`: 1536 in
Infest01, 3000 in the port.

`4E` drowns. The family update prefix `00429D60` keeps a breath WORD `+230`.
While an actor is more than half its size `+28` under the water plane, the
WORD grows by `us >> 10` up to 4000. Above 4000, each tick applies channel-5
damage `+A2 * us / 4000000` through `0047C7C0` instead. Otherwise a nonzero
WORD shrinks by `us >> 9`, wrapping below zero. Infest01's `4E` starts with
heap residue `B70C`, the low WORD of a free-list pointer the allocator left
before the load. It is still far above 4000 when `4E` dives at clock 706, so
24 ticks of 61 damage follow (3000 to 1536). The port's zero needs 33
submerged ticks first, and `4E` surfaces before then. Drowning names the
process NULL `0054E894` as its source, which skips the reaction's
retaliation draw, so the RNG comparison cannot see it. With residue `B70C`
supplied, `4E` ends at 1536 in the port too, and the 538 RNG words still
match.

Six more checks cover state the RNG cannot see:

- **Camera.** At every tick start, the selector 16 camera `08E10001`
  (Infest01 object `03829250`) has the same eye WORDs `+32/+34/+36` and all
  ten basis DWORDs `+44..+68`, bit for bit, at all 538 ticks.
- **Particles.** At clocks 1800, 2400 and 3356, the same pool cells are
  occupied (176, 26 and 5). Their 64-byte payloads are identical, apart from
  the native list and self pointers at `+18..+23`, which the port keeps as
  portable custody. All 10,764 compared bytes are known in the port.
- **Sound.** Infest01 constructs 1,081 sound records (`0047FFF0`) during the
  intro. The port constructs the same 1,081 in the same order and tick, with
  the same kind (one-shot or persistent), H9 slot, position, gain, pitch and
  flag.
- **Model draws.** In the 537 frames drawn after clocks 6 to 3356,
  Infest01 submits 22,130 models through common `0049FEA0` and manager-late
  `004ED610`: sky, actors, pickups and furniture. The port submits the same
  22,130 in the same order and frame, with the same root mode, H8 slot,
  position and all ten basis DWORDs. (The recording stops as clock 3362
  starts, before its last frame.) This check found a port bug, now fixed. The port drew
  the sky at the full camera eye, while `00440DA0` keeps the eye's X/Z and
  takes Y from H14/0 header DWORD +10 (-3680 in the intro). The intro sky
  had been drawn 4,140 to 5,126 units too high.
- **Renderer queue.** Every primitive packet reaches the renderer through
  `004A42B0`. The frames after clocks 1800, 2400 and 3356 allocate 2,224,
  1,373 and 921 packets: terrain, furniture, models, ribbons, particles and
  stars. At Infest01's own 1024x768 viewport, the port allocates the same
  packets in the same order, with the same callback, block count, header
  words and every geometric sort word. Pointer-derived texture batching keys
  map one to one, but in the portable order the port documents. Native keys
  follow each H3 bank's entry index. Banks sit where the heap placed them:
  in Infest01, level 32 Base (loaded last, into a freed block) lies below
  level 5 Difficulty, which lies below level 1 Difficulty. Keying the port
  in that exact order changes no pixel of the frame after clock 1887. This check
  found two more port bugs, now fixed. Opcode `1A` had rejected every ground
  sprite, because its corner clip waited for "fog bounds" that are really
  the published viewport `0055F234/38`. Opcode `07` near had applied the far
  handler's double-`FF` fog skip to its stale fog bytes and dropped 12 of
  the ship's line quads.
- **Packet payloads.** Each packet's payload was read right after its
  producer filled it, before any mid-frame queue reset could reuse the
  storage. Eleven frames were compared: those after clocks 943, 1256,
  1643, 1800, 1881, 2068, 2400, 2568, 2912, 3131 and 3356, spread from the
  battle's first clashes to the intro's end. All 17,011 payloads
  match: 952,641 bytes the port knows, apart from the texture pointers.
  The match needs one normalisation. Infest01
  ran D3D on an RGB565 surface, while the port's colour layout is RGB555,
  so 16-bit colours compare as equal red and blue with native green halved.
  This check found one more bug, now fixed. The world draw had never
  published `004501A0`'s world light, so furniture models 163 and 49 drew
  under a pickup's directional light and effect colour.

The allocation residues remain unowned in production, as labeled
approximations. With all of them supplied and the four draw bugs fixed,
no other difference remains. The RNG word, camera pose and model draws match
at every intro tick. The sampled object state, particle state and renderer
packets and payloads match too, as do all the sound requests. The
comparison does not cover overlays drawn outside the queue, final pixels or
mixed audio output.

## Verification and acceptance boundary

Run focused tests from ``:

```text
cargo test -p inf-game intro --lib -- --nocapture
cargo test -p inf-game completion --lib -- --nocapture
cargo test -p inf-game supported_save_prefix --lib
```

The [tracked New Game input](../../crates/inf-game/tests/fixtures/new_game_input.json)
replays the fresh frontend/profile/name-entry path, then supplies no further
input. From ``, this production smoke uses separate settings
and profile paths and requires no Escape or mission override:

```powershell
$smoke = Join-Path '../../.tmp' ('inf-intro-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $smoke | Out-Null
cargo run --release -- --data-root retail --renderer software --hidden --width 640 --height 480 `
  --frames 1100 --fixed-step-micros 125000 `
  --input-replay crates/inf-game/tests/fixtures/new_game_input.json `
  --settings "$smoke/settings.json" --profiles "$smoke/profiles.sav" `
  --snapshot "$smoke/briefing.png"
```

The verified run exits normally on the loaded Level 1 Command Centre briefing,
after shared actor retirement and world reload. The fixed timestep and input
frames are test inputs, not recovered retail cadence or behavior constants.
Successful firing is reported as `World weapon firing: N shot(s) emitted by
M nonempty drain(s)`. The shared [combat owners](PROJECTILE_COLLISION_RUNTIME.md)
now execute class `00526EFC` movement/drawing, collision, `00489600` impact,
material effects and supported live damage/retirement. Contact counters and
`World projectile damage` health diagnostics distinguish consequences from
construction. This checks automatic continuation under the existing
named approximations; it does not certify omitted actor features or complete
cinematic fidelity.

### Against retail in the lab

The scenario lab plays a new game in both games from the title
(`newgame-intro.json`, and the port's own new game in `run.py both`). With the
fixed step standing still within a frame, as the lab's clock does
([loading](LOADING_RUNTIME.md)), the port draws the same 66,367 random numbers
before the intro's first tick and starts it from retail's word. All 36 stage
exits and entries of `00443B10` then match retail's tick for tick (the lab's
`intro` boundary against the port's `intro` lines), and the intro ends at
tick 538 in both. Before, stage 5 ended eight ticks late: it waits for an
alien to start its attack (family `+22C`), which draws from the RNG.

### Frame-rate and resolution sweep

The tracked replay counts frames at 125 ms steps. For another step, scale
each press frame by `125000 / step`, round it, and release one frame later.
`--fixed-step-micros` takes whole milliseconds only. About 1,000 frames'
worth of 125 ms time reaches the briefing. On 2026-10-08 the production
route, with no harness, ran at 1024×768 with 8 ms steps for 16,000 frames,
16 ms for 7,700, 33 ms for 3,800 and 50 ms for 2,600. It also ran at 125 ms
steps for 1,100 frames at 640×480, 800×600 and 1024×768. All seven runs
reach the Level 1 briefing.

The faster steps reached two reads that 125 ms runs never reach. One was
Selector35's route distance
[`+260`](SELECTOR35_RUNTIME.md#remaining-boundaries). The other was the
`+08` point of Selector34's
[Attack entry clone](FAMILY428650_RUNTIME.md#selector-34). Both now start at
zero under the labeled extended allocation approximation. At 800×600 a
turret's retirement radius reached source
[`+108`](PROJECTILE_COLLISION_RUNTIME.md), which now uses the same binding
as projectile damage. After those changes, these diagnostics remain:

| Run | Remaining diagnostics |
| --- | --- |
| 1024×768, 8, 16, 33 and 50 ms | The two ribbons' first-frame `Position` omissions |
| 640×480, 125 ms | The ribbons only |
| 800×600, 125 ms | The ribbons, plus the draw-only [world flash `+20`](WORLD_FLASH.md#deferred-draw) boundary |
| 1024×768, 125 ms | The ribbons, plus model 306's [near-camera opcode `07`](GAMEPLAY_RUNTIME.md#control-owner-decomposition-from-the-opdetailprobe-census) omission at clock 1112 |

The 640×480 run's totals, 24,541 model draws and 186,858 stars, equal
those of the previous day's smoke. With these changes the seeded run still
matches Infest01: the RNG word at all 538 ticks, all 22,130 model draws and
all 1,081 sound requests. The port does not support 1280×960 or
1600×1200 as display sizes. With authored output, the 320×240 and 512×384
runs stopped at bootstrap on the test machine. Fullscreen requires the
exact display mode, and SDL offered only 640×480 as the closest one; the
[startup display search](PRESENTATION_POLICY.md#startup) now falls back
instead. With the former `--output-resolution 720p` the native framebuffer
kept those sizes, and both runs then reached the briefing with only the
ribbons' first frames omitted.

The complete realtime recording observes the actual full 1024×768 SDL target
and stereo mixer output using the port recording observer.
The retail source is a continuous
video intake.
Keep both images uncropped, including native dark bands. Align visual beats
with frame pairs and elapsed timestamps; 30 fps does not establish per-tick
cadence. The comparison tool
retains source hashes and decoder timestamps and leaves acceptance undecided.

The shared family dying, pickup, attached-effect, world-flash, particle-emitter
and terrain-light owners now continue the earlier callback stops. Selector35's
[distinct descriptors and post-dispatch continuation](SELECTOR35_RUNTIME.md)
retain their own target, motion and audio policies. Constructor-unwritten
`+19C` and spawned-family `+150` have no native writer before their first
read. The timer evidence
includes both zero and nonzero first values and does not justify a universal
zero. The production route nevertheless zero-fills both under the labeled
[extended approximation](../../crates/inf-game/src/fresh_heap.rs) (user
decision 2026-10-06 for `+19C`), so native ref61's first selection comes
about 7.5 s later than the port's. For `+150` the
census
found zero in both native spawned Selector12s. Existing named approximations remain separately
documented.

The eight selector-11 turrets and three selector-30 gates now run their
[turret update and retarget](ACTOR_RUNTIME.md#selector-11-turret-update-and-retarget)
and [gate update](SELECTOR30_RUNTIME.md#update), so the update list skips no
callback. Their first reads of turret `+19C` and gate `+158` use the same
labeled zero (evidence).
The turrets shoot down both selector34 `pership` ships (slots 46 and 77):
each runs its [dying descriptor](FAMILY428650_RUNTIME.md#selector-34), dives
and crashes through `+94=00434ED0`, and is disposed; dying selector35 ships
crash through `+94=0043F3E0` the same way. The class10/36 pair accept
`00425B90` now runs when a ship meets a Selector12; only its team-0
boarding, which Infest01 never reaches, stays a typed boundary.

Since the update pass alternates direction like `00478F20`, actors draw the
shared RNG in native order and the 1100-frame smoke also destroys furniture
(bursts, terrain flashes and camera flashes). The `00526F20` particle's
unwritten shadow `+30` reads zero under the labeled
[fresh-pool approximation](PARTICLE_RUNTIME.md#pool-and-list-custody), as
Infest01's own draw did; its draws then cull and retire those particles
earlier, so from about clock 1818 the smoke's shots differ from the
previous run. One draw-only stop remains: the world flash packet's `+20`
allocation mode at `00488288`, a byte no writer in its path fills. The
original intro never flashes ([WORLD_FLASH.md](WORLD_FLASH.md#deferred-draw)),
so this stop belongs to the smoke's divergent furniture destruction. The selector67 ribbons now
trail both ships, draw through `00486B30` and are disposed after them, and
the three zone signs draw through the view-plane branch `0B`.

The world draw used to log only its first frame's object omissions, which
hid about 1,700 later model omissions in the smoke. It now logs each new
object and reason once. Selector36's models 489/492 now draw through opcode
`25` and the dying selector12 model 248 through a root's opcode `2A`
([model runtime](MODEL_RUNTIME.md#runtime-vertices-projection-and-shades)).
The spawned selector35 ships (model 448) draw under the labeled
[WingModel457StackHistoryApproximation](MODEL_RUNTIME.md#wingmodel457stackhistoryapproximation),
and the selector60 at slot 52 births its exact-7 smoke trail
([callback](OBJECT_MODEL_CALLBACKS.md#selector60-exact7-trail)). The two
selector10 `quixle` actors now solve their
[legs](FAMILY4236D0_RUNTIME.md#legs-custom-vertex-00425870) each frame and feed the
helper tick, and their model 306 draws through the
[DC triangles](PRIMITIVE_RUNTIME.md#opcode-dc-material-shared-shade-triangles).
The one remaining stop is each selector67 ribbon's first frame, which only
reads its unwritten position ([ribbon runtime](SELECTOR67_RUNTIME.md#draw-00486b30)).
Infest01 draws far E4 quads 403 times in the intro; they now blend their fog
like the original instead of drawing unfogged
([E4 quads](PIXEL_RUNTIME.md#e4-shared-shade-quads)). The clipped DC/E4
wrapper is owned too, though Infest01 never calls it in the intro.

Infest01 never runs the player callback `00438950` in the intro window
(TTD execution counts of its entry and branches are zero), so the open
player work in the world tick does not affect the intro. Manager `00478F20`
still runs its player block every tick: `0046B010` finds the first player,
`0046A0E0` (538 runs) only ORs flag `10` into the player's selector-2 vehicle
(its `+58` gate never opens, so the camera retarget at `0046A1F7` never runs),
`00479040` moves that vehicle to the end of the alternate list, and
`0046A260` (537 runs) reads the input devices. The vehicle is constructed
once (`004156E0`) and is never updated (`00416090`) or drawn (`00420510`),
which matches the port's intro controller removing the first player from both
lists. The port runs the same [player block](GAMEPLAY_RUNTIME.md#player-block-of-00478f20)
in the intro, which therefore makes no vehicle call.

After the intro, the loading screen's model 117 now draws whole under
[LoadingModel117StackHistoryApproximation](MODEL_RUNTIME.md#loadingmodel117stackhistoryapproximation).

The actor model's real `698 → 699 → 700 → 701` auxiliary chain uses the
captured actor's current packed values, including rotations, interpolation and
external children; no broadphase-only hit stands in for the precise query.
The [selector12 dying owner](ACTOR_DESCRIPTOR_CHANGES.md#selector12-dying-descriptor)
refreshes model 248 and creates the actual burst before its elapsed-time update
queues disposal. The [next manager pass](OBJECT_RUNTIME.md#per-frame-disposal)
runs cleanup and releases its resources, handle and object. Focused production
tests preserve this two-frame boundary and the mission's actual inhibit byte.
Firing, damage counters and a successful handoff establish that those paths
execute; they do not establish complete actor motion or cinematic fidelity.

The retained-script corpus test requires the local retail executable,
PRELOAD and OVL corpus. It must not silently replace missing corpus with a
synthetic script. Live acceptance still requires original-game comparison
of the moving engine cinematic and the full new-save route: hidden intro
briefing, engine intro, results transition, visible Level 1 briefing, Level 1.
The [shared Selector12 tail](ACTOR_MOVEMENT_TAILS.md) composes target refresh,
steering and terrain/physics after actual `+D8` completion. The eye's appearance
has been checked against retail; that observation does not establish
the other actors' timing or the complete sequence. Current required services
and explicit intro/results/reload approximations still bound cinematic acceptance.
