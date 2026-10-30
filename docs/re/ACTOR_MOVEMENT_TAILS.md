# Infestation family movement tails

This authority owns the class-specific continuation after a normal virtual
`+D8` return. The [behavior dispatcher](ACTOR_BEHAVIOR_RUNTIME.md),
[retained target heap](TARGET_RUNTIME.md#retained-actor-service-heap) and
[process state](PROCESS_ACTOR_STATE.md) retain their separate custody.

## Class dispatch

| Actual final class/table | Post-D8 continuation | Current boundary |
| --- | --- | --- |
| 12 / `00527058` | `00467A77..00467D98`, then motion `00467DF0` | Owned nullable-target/control protocol; reached unknown fields and probe misses stay required; firing shares the retained queue |
| 9 / `00520B84` | `004248DD` onward | Selector 10's tail and audio, then the family's own [motion `00424CF0`](FAMILY4236D0_RUNTIME.md#selector-9-the-familys-own-class) |
| 10 / `00520CC8` | `004248DD` onward | [Owned target/control, motion and common-child continuation](#selector10-target-motion-and-child-callback), then [resource-gated persistent sound](AUDIO_RUNTIME.md#class10-helper-and-persistent-actor-sound); reached backing, vector, water-address and emission limits stay typed |
| 36 / `00527EE0` | `00481787` onward | [Owned target/aim and counter continuation](ACTOR_BEHAVIOR_RUNTIME.md#selector36-post-d8-prefix), then WeaponTick/fresh148 avoidance, [owned motion](#selector36-contact-alignment-and-motion) and [persistent record update](AUDIO_RUNTIME.md#class36-persistent-actor-sound); shared looping playback uses the same SDL voice owner |
| 35 / `0052273C` | `0043E766` onward | Five controls, prediction, terrain/autopilot, motion `0043EF60` and camera-dependent audio |

Production uses the arena's actual typed class. Scene numbers, placement
references, model IDs and visual identities never select a tail. A prefix or
behavior failure retains the reached visit and stops that actor; it does not
invoke movement. A normally completed Selector12 tail remains eligible for the
next native list visit. Required failures retain their partial fields and are
not silently replayed. The shared selected weapon position and class36 target
refresh/aim/counters are owned in the [behavior authority](ACTOR_BEHAVIOR_RUNTIME.md#selector36-post-d8-prefix).
They return the exact Motion continuation after normal WeaponTick/avoidance;
this movement document does not equate class36 motion/contact with Selector12.

## Selector12 ordered target and control protocol

[selector12_tail.rs](../../crates/inf-game/src/object_runtime/actor/derived/selector12_tail.rs)
owns the complete post-D8 instruction interval and calls existing shared
avoidance, target refresh, basis and motion kernels.

A fresh null `+148` writes both stack controls zero and skips primary/secondary
geometry, `+150/+158/+15C/+208`, and firing. This is a source-owned path,
independent of the intro scene. It still executes avoidance's zero-forward
return, forward-velocity threshold, terrain alignment and physics.

A nonnull primary target refresh writes back into the same retained heap
allocation, including a refresh error's prefix, before publishing full status
`+150`. Geometry consumes the target's direct six-byte `+08` focus, not its
separate virtual point interface. Each displacement wraps as a signed WORD;
squared sums wrap DWORDs, using the original bit31 integer-square-root repair.
The actual embedded actor basis performs the nearest-even inverse WORD transform.

A fresh nonnull secondary `+154` refreshes first, stores distance `+158` before
the inverse transform, then reads required timer `+15C`. Signed timer greater
than raw delta and distance greater than unsigned size decrements the timer.
Otherwise deletion of the fresh target allocation precedes clearing the field;
a failed release preserves it. Steering retains the captured secondary geometry
after normal deletion. These two fields remain unknown at generic construction.

Without a secondary target, primary distance is stored at `+14C` before the
transform and required signed spacing `+208` read. Nonnegative spacing uses
truncating `(localZ-spacing)/2`; negative spacing uses the distinct forward-only
multiply/divide adjustment. Turn division, behind-Z correction, zero-distance
bypass and x86 fault metadata retain native wrapping and order.

Firing acquires actual `+140` only after geometry. Radius, `+22C`, refreshed
status, signed behavior-entry index and that entry's separate timer/target/age
allocation gate the call. Local forward/aim/Y tests run afterwards. Only the
reached `0048D340` call runs the [shared direction/queue owner](WEAPON_FIRING_RUNTIME.md)
with update-`+20` true. Normal return continues this tail; a reached service
error preserves the prefix. The later draw consumer owns emission.

## Avoidance and motion

The existing `0042A2C0` consumer always runs. Nonzero forward control scans the
actual `+234` packed cache through `00429FF0`. Genuine cache hits are consumed;
a miss preserves scan flags and cache-window/storage prefixes before invoking
the real `00429EE0` service. The [shared captured allocation](FAMILY4236D0_RUNTIME.md#captured-actor-cache-allocation)
keeps current `+234` installed during that arena callback and validates its
identity only on normal return. The [collision owner](FURNITURE_COLLISION.md)
consumes current terrain and [retained ACTIVE graphics](MODEL_PROCESS_GRAPHICS.md);
a typed error retains the reached guard/XYZ/cache prefix without a clear result.
A false result projects live signed velocities using native Y+Z then X
x87 order. Absolute wrapped result below 10 clears WORD `+92`, then `+8E`.

At the actual `+118` entry, production authenticates current H12 before actor
XYZ reads. It passes that retained mutable world's terrain to existing
`00467DF0`, avoiding a second registry acquisition or copied terrain. The
retained authored BYTE `0053A128` supplies surface-snap policy, separately from
native turn/forward controls. The current no-writer domain supports its supplied
value request; a future recovered callback writer requires rechecking freshness.

The complete motion consumer retains Y-snap-before-IDIV failure, alignment,
forward/angular/lateral tables, dead-body differences and final native physics.
A zero steering value never represents a terrain mode or missing service.

### The scientist's gait and the frame rate

A scientist walks or runs by its speed alone: its model (223, or 197 for
the mutants of team 0x12) asks the [model callback](OBJECT_MODEL_CALLBACKS.md) `00468920`
for value 2, which is `0xC000` (the run) above a speed of 100, `0x8000` (the
walk) above 5, and otherwise `0x4000` with byte `+29C` set, or 0. How fast it
gets depends on the frame rate. The forward control is half the distance to
the target, but the forward motion moves the velocity toward it by at most
`0x2000 * (delta >> 5) >> 15`, twice that from rest (table `0053A080`, maximum
`0x3000`), and the threshold above clears the velocity whenever its forward
part is under 10. At 60 frames a second one update's push is about 6, so the
threshold clears it every update and the speed never builds: the scientist
stands, or creeps when a push gets past. At 30 it is about 12, and the speed
climbs past 100 to the run.

Retail's own runs show it, from the same drop beside Altair Prime's gate
(tampered-run: `runtime_re/lab/scenarios/altair-drop-moved*.json`
(a constructed save with a scientist in the hold, world 5's first start moved
beside the gate)). At 125 ms its scientist runs every update (velocity median
131, at most 287); at 33 ms it runs 331 updates of 341 (peaks of 346 units a
second); at 16 ms the velocity stays at a median of 6, at most 39, and it
stands 84% of the time. Its model stays 223 throughout. The port ticks the
world at 16.384 ms (`exact_world_ticks`), so with every fix off its
scientists keep retail's 60 fps gait: `run.py both` matches retail in every
compared event from that state at 125 ms and at 16 ms.

Retail's gait at 30 fps is the one its scientists were made to walk with,
so the port's `paced_actors` fix (on by default since
2026-10-18) gives them that step: it updates the scientists (and the alien
ships, [selector 35](SELECTOR35_RUNTIME.md#frame-rate)) only every other
16.384 ms world tick, with both ticks' time
([`ActorPacing`](../../crates/inf-game/src/session_platform.rs)), while the
rest of the world keeps its own step. From the Altair drop at 60 frames a
second, the paced scientist stands in 3% of its updates and runs in 21%,
covering 45 units a second, where unpaced it stands in 99% and covers 15;
retail's at 33 ms, in its own drop, stands in 1% and runs in 38%. The lab
runs the port with `--paced-actors false`, so its comparisons stay
retail's.

## Selector10 target, motion and child callback

[selector10_tail.rs](../../crates/inf-game/src/object_runtime/actor/derived/selector10_tail.rs)
owns `004248DD..00424BBF` after the existing WeaponTick and normal `+D8`
return. It does not repeat that prefix. A fresh NULL `+148` supplies zero
controls and skips target geometry and firing; contact, physics and the
installed child callback still run. NonNULL targets use the same retained
heap allocation for refresh/writeback, including error prefixes. Secondary
geometry stores `+158` before the inverse transform and required `+15C` read;
normal deletion precedes clearing `+154`. Without a secondary target, primary
geometry stores `+14C` before required signed spacing `+208`. The negative
spacing adjustment differs from Selector12 and retains its signed division
and behind-Z correction.

The real behavior, distance, `+22C`, refreshed status, entry timer/target/age
and local aim gates select shared `0048D340` or `0048D5A0`. Class10 supplies
update-`+20` true; class36 supplies false independently of its behind flag. Fresh `+260` then fresh `+148` admit shared installed-cache
avoidance. That call lends the actual retained model-process state. A reached
NativeNULL projection callback is genuine selected-table history: it remains
a typed feature error, without a replacement projection table or clear result.

[selector10_motion.rs](../../crates/inf-game/src/object_runtime/actor/derived/selector10_motion.rs)
owns virtual `+118 = 00425400`. Signed current C4 greater than 1 selects
`delta >> 5` for shared `00426270` contact; otherwise native step zero still
computes contact and runs later physics. Current H12/H14, clock, up vector,
plane policy and table `+25C` retain their ordered reads. Nonzero-step water
contact keeps `RequiredNativeAddress`; the separate class10 stack-origin
source proof is held and has not changed this policy. It does not inherit the
class36 stack-address admission.

Nonzero alignment reads actual mode `+268` and turns the actual Y basis row
toward an axis through the angular response; other modes use the contact
normal. Modes 3 and 4 use function statics (`0042544C..00425562`). Alive
(signed C4 at least 3), the world up `0055F280` is copied once into
`00546A50`. Dying, the normal crossed with basis row 2 (`+60..+68`) is
rewritten into `00546A88` on every call: exact products, one rounding of
each difference under control word `027F`, then one store to f32. The
first-use guard `00546A68` and its atexit registrations (`00425820`,
`00425830`) change nothing else. Fresh C4 after contact/alignment gates the
motor arm; positive inverse-normal Y and nonzero power admit the existing
forward, angular and lateral table consumers in that order. BYTE `+22E`
mask `0x20` boosts power and step with native wrapping/clamping. Signed C4
at most 1 and flags mask `0x80000` admit the separate damping arm. Final
physics uses raw microseconds and mask `0x1F`; zero control or step never
substitutes for an unowned service.

After physics, a fresh NULL helper `+26C` bypasses its callback and the later
mode read. A present helper always calls its actual virtual `+04` before
fresh mode and impulse gates. Mode 1 skips the vector arms; it never skips
the child callback. The
[common child owner](../../crates/inf-game/src/object_runtime/actor/derived/selector10_helper/tick.rs)
implements `00445CD0` and `00445BC0` against installed helper/child backing.
It reads descriptor count before enabled byte `+0C`, preserves rotating index
`+08`, and returns the real completed-child count. The return local is the
same stack storage across the loop's PUSH/POP EDI; mode is not a substitute
for that result. Actual count-4 children constructed by `00445A70` have zero
flags `+14` and timer WORD `+36`, so their real checks can return zero without
unknown-vector, RNG or sound reads. Changed child state admits later reads;
count-2 raw holes remain typed reached requirements. Mode 4's helper runs
the [walker tick](FAMILY4236D0_RUNTIME.md#mode-4-the-walker-tick-004476d0)
instead, whose first call reads the stride phase `+34` that nothing writes
(a fresh-heap carry, zero, as a fresh block holds it).
Positive completion, alignment, mode and velocity gates retain the native
post-callback impulse arm rather than an unconditional success path.

A normal Motion return enters the distinct
[class10 persistent-audio suffix](AUDIO_RUNTIME.md#class10-helper-and-persistent-actor-sound).
Any error retains target, actor, basis, velocity, child, RNG and record prefixes
already reached and stops only that actor feature. Initial derived `+14C` and
the separate family-history helper `+218` header `+00/+04` retain their
allocation-history boundaries; this child owner does not initialize them.

## Selector36 contact, alignment and motion

[selector36_motion.rs](../../crates/inf-game/src/object_runtime/actor/derived/selector36_motion.rs)
owns complete virtual+118 `00481C10`, including `00482160` alignment.
Production invokes it only after the real post-D8 target/WeaponTick/avoidance
prefix returns its current full DWORD controls `[yaw, forward]`. The old
unfinished-Motion stop is removed; normal Motion return enters the distinct
[persistent-audio `00481B10` owner](AUDIO_RUNTIME.md#class36-persistent-actor-sound).
A normal audio return completes this class36 virtual+24 callback. Reached
unknown fields and lower services still stop their own actor feature.

Entry captures signed lifecycle C4 and actual contact table+25C. Positive C4
passes `delta >> 5` to [shared terrain contact](ACTOR_CONTACT_RUNTIME.md);
nonpositive C4 passes the native zero step while still executing contact,
scalar/packet publication, alignment and physics. The initial C4 decision is
retained for the later motor gate; the damping gate rereads current C4.

`00482160` captures Y/Z/X before its possible fresh H12 acquisition. Height
separation and unsigned size*2 determine the lower alignment bound `038E`.
Current normal and basis feed the original inverse-sine/quadrant logic. Ordered
local XY/YZ corrections preserve native signs, and their angular recovery adds
half the opposing local angular component back in world space. The final
angular-response service uses the actual current Y basis, normal and
`wrapping(alignment * step) SHR14` gain.

The motor arm requires the retained positive-C4 decision, nonzero alignment,
positive inverse-normal Y and nonzero power. Its FLOAT inverse captures the
actual DWORD floats, evaluates `(Z product + Y product) + X product`, and spills
each result once. Fresh inherited BYTE22E bit20 boosts power and step in the
original order, with the original power clamp. Actual tables+254/+258/+250 are
read only in reached forward/angular/lateral order and feed existing shared
motion kernels. Later C4<3 with flags80000 uses the native logical velocity
product shift and thresholds64/24/64. Final physics always uses raw microseconds
and mask1F after earlier normal returns.

Errors retain contact scalar, packet/actor writes, basis changes and already
applied motor increments; they never invoke audio. Class10 motion `00425400`
and class35 motion `0043EF60` remain different consumers. Their virtual+A8
policies also differ: class10/36 use guarded `0042A760` before `0047BFD0`, while
class35 binds `0047BFD0` directly. Shared fields do not equate those callbacks.

## Evidence and validation bounds

Class10 [tail/native guards](../../crates/inf-game/src/object_runtime/actor/derived/selector10_tail/native_guards.rs)
bind complete `00424830`, `00425400`, common-helper bodies, original constants
and final table entries. Its
[target cases](../../crates/inf-game/src/object_runtime/actor/derived/selector10_tail/tests.rs),
[motion cases](../../crates/inf-game/src/object_runtime/actor/derived/selector10_motion/tests.rs)
and [child cases](../../crates/inf-game/src/object_runtime/actor/derived/selector10_helper/tick/tests.rs)
use explicit supplied inputs to distinguish nullable paths and retained failure
prefixes. These source contracts and fixture domains do not establish a normal
intro actor's reach, native allocation history or matched retail movement.

[Native guards](../../crates/inf-game/src/object_runtime/actor/derived/selector12_tail/native_guards.rs)
bind complete original class bodies and relevant final vtable entries.
[Focused tests](../../crates/inf-game/src/object_runtime/actor/derived/selector12_tail/tests.rs)
exercise nullable target, secondary lifetime, signed geometry, lazy dependencies
and reached failure prefixes. The
[null-target composition test](../../crates/inf-game/src/session_platform/intro/runtime/tests/family_prefix_tests.rs)
`production_selector12_composes_real_behavior_and_null_target_motion` supplies
zero score coefficients and reaches native null-target motion. It leaves `+19C`
unwritten: the next visit stops at `Uninitialized { offset: 0x19c }`, and a
further visit preserves that stopped actor and RNG. No timer initializer is
inferred from the first normal motion return.

The separate `production_family_prefix_preserves_class_specific_stops_and_the_live_selector12`
scheduling fixture retains selected timer carries, checks actual installed-cache
identity and guard clearing, and keeps normal Selector12/36 callbacks eligible
for a second visit while preserving the stopped class10 actor. Those fixture
inputs do not establish original intro coefficients, timing or allocator fidelity.
Neither a complete individual tail nor static original bytes establish a complete
retail cinematic; remaining damage/contact responses, class tails and descriptor
effects stay separate work.
