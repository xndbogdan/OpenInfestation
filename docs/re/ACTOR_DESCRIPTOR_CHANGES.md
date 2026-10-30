# Infestation descriptor callbacks

This authority owns the composed target and dying callbacks within virtual
`+D8`. The [dispatcher](ACTOR_BEHAVIOR_RUNTIME.md) selects authored descriptors;
the [target heap](TARGET_RUNTIME.md#retained-actor-service-heap) owns cloning and
release; [class movement](ACTOR_MOVEMENT_TAILS.md) follows a normal D8 return.
These phases retain separate state and failure boundaries.

## Shared submission and class policy

Production passes the current arena and live process constructor source into
intrinsic changes. No scene, actor slot, placement or model selects behavior.
Native descriptor addresses select a request; actual class state authenticates
its consumer. The dispatcher owns the preceding target-release phase. These
changes never retire an old `+148` themselves, and a normal NULL clone is a
normal result with its native suffix, not a missing-target error.

| Wrapper / actual virtual | Represented owner | Distinct boundary |
| --- | --- | --- |
| `00469170` / Selector12 `+12C = 004672F0` | Captured `+26C` change; fresh `+29D` spacing gate | Separate update `00435170` authenticates actual `+124` below |
| `00435170` / Selector12 `+124 = 00404840` | Complete RET4 empty update | Common `+124 = 00423BD0` and Selector35 `+124 = 0043DB90` retain distinct timer/target implementations |
| `004691D0` / Selector12 `+138 = 00467340` | Captured `+274` target change | Separate generic update `00425D90` |
| `004691F0` / Selector12 `+14C = 004675E0` | New kind8 point at current actor XYZ | `00469210` / `+148 = 00467390` owns the point/timer/RNG update |
| `004692F0` / Selector12 `+144 = 00467760` | Preferred `+27C/+284` target change | Update `00469310` / `+13C = 00467660` consumes actual terrain/grant sources and retained timers |
| `0043FA80` / `+E8 = 004288E0` | Parent4236D0, classes10/36 and Selector12 share the object-target consumer | [Selector35](SELECTOR35_RUNTIME.md) owns its distinct `+E8 = 0043F700`; shared `0043FAA0` / `+EC = 00428830` updates the retained route target |

The canonical class12 profiles select their wrappers through actual descriptor
tables and class storage. Production routes the common object-target wrapper for represented
non-Selector35 profiles. [Selector35's separate descriptor kernel](SELECTOR35_RUNTIME.md#construction-and-descriptor-selection)
owns its E8/EC/12C/F8/F4/124 implementations through the actual wrappers.
Shared ancestry does not make those changes equivalent.

## Selector12 changes

[selector12_descriptor_changes.rs](../../crates/inf-game/src/object_runtime/actor/derived/selector12_descriptor_changes.rs)
uses one request with explicit `Target26c`, `PointAtCurrentPosition`,
`Target274` and `PreferredTarget27c284` policies.

False `+12C` bypasses all receiver and source reads. True captures `+26C`
before writing status `0053A12C`, clones the actual retained source or
propagates a genuine NULL, then publishes `+148`. Only after normal publication
does fresh BYTE `+29D == 0` write spacing `+208 = 256`. Clone failure retains
the status and old current target; a nonzero byte preserves prior spacing.
This phase introduces no RNG, timer, distance or old-target release.

False `+138` returns before any actor, target or process read. True captures
`+274` before writing status pointer `0053A138`. A captured NULL stores spacing
`+208 = -100` before clearing `+148`. A nonnull source clones its actual retained
allocation, publishes the result to `+148`, then stores spacing. Clone failure
retains status, the previous current pointer and any actual heap prefix.

False `+144` clears only class12 BYTE `+29C`. True captures unsigned scores
`+280/+288`, writes status `0053A154` and `+29C = 1`, then selects `+27C` only when
its score is greater; ties select `+284`. A captured NULL ORs WORD `+22E` with1
and writes DWORD `+194 = 1`, preserving the other timer fields and current target.

A nonnull preferred source resolves its actual virtual `+10` before cloning.
The captured actor identity survives allocation. After a normal clone return,
writes occur in order: `+148`, spacing128, `+18C = 0`, `+190 = 4,000,000`,
`+194 = 0`, then `+198 = captured actor unsigned size + 288`, or288 for a genuine
NULL virtual result. The original source referent supplies size; the new clone
and virtual `+14` do not substitute for it. ActorFollow clones bypass the process
NULL getter; fixed/object clones read the actual live source at their native
constructor phase. Errors preserve earlier writes and never seed unknown carries.

The dispatcher calls an entered descriptor's update in the same D8 visit,
before the class movement tail. Change `+144` does not initialize `+14C`.
The complete `+13C` update preserves its real target-kind/grant gates before
requiring distance/timers; it never manufactures a first distance from `+198`.
The distinct class12 virtual `+148` point/RNG update has its own owner below;
it never initializes actor DWORD `+14C` or substitutes `+198` for distance.

## Selector12 captured-target empty update

Descriptor update `00435170` forwards the raw microsecond DWORD to the actual
actor's virtual `+124`. Complete constructor `00467070` publishes final table
`00527058`, whose `+124` cell selects `00404840`: only `RET4` (`C2 04 00`).
Both class12 descriptor pointer tables select singleton `0054ECC0` at index2;
its CRT-published vtable `00526FE8` has this update at descriptor `+0C`.
These method offsets are distinct from actor's retained DWORD `+14C`.

[WorldFamilyBehaviorHost](../../crates/inf-game/src/session_platform/world_actor_behavior.rs)
admits the canonical Selector12 descriptor only after authenticating the actual
retained class. The empty leaf reads no scalar, delta, target, allocation,
terrain or process service and leaves actor DWORD `+14C` unchanged, whether
known or unknown. Parent4236D0/classes10/36 instead select active `00423BD0`;
Selector35 selects distinct `0043DB90`. Both active consumers now run through
their own timer/target owners; the Dormant class dispatcher does not substitute
for them. Unknown constructor timer backing still fails at its actual read.

A normal return permits the existing dispatcher elapsed `+188` wrapping add.
Earlier change or update errors keep their reached prefixes and stop before
that suffix. The [focused callback fixtures](../../crates/inf-game/src/session_platform/world_actor_behavior/tests/selector12_empty_update.rs)
cover these class/state/error distinctions; [native guards](../../crates/inf-game/src/session_platform/world_actor_behavior/native_guards.rs)
bind the complete call chain and final table cells. This leaf owns neither
initial distance/timer backing nor the later class movement tail.

## Selector12 point target change and update

[selector12_point_update.rs](../../crates/inf-game/src/object_runtime/actor/derived/selector12_point_update.rs)
owns complete `00467390`. Its wrapper `00469210` reaches final Selector12
virtual `+148`; point change `004691F0` reaches virtual `+14C = 004675E0`.
These virtual offsets are distinct from actor's retained DWORD `+14C`.

False point entry returns before actor, process or allocation reads. True writes
status `0053A148` before the actual0x14-byte fixed-point reservation. Normal
allocator NULL publishes NULL `+148` and then `+18C = 0`, bypassing process
NULL and XYZ. Successful allocation precedes the live process constructor-NULL
read and actual current XYZ; construction and publication finish before clearing
`+18C`. Reached missing process/XYZ preserves status, old148/18C and the
Pending reservation. Whole-XYZ target backing does not model native partial
handle/vtable/XY-versus-Z stores. Padding stays unknown; dispatcher release
remains the preceding owner.

The update compares `+18C` and delta as signed DWORDs. A greater timer gets a
wrapping subtraction and returns before targets or RNG. Expiry tests raw
`+1EC` before `+1DC`; both NULL OR `+22E` with1 without resetting the timer.
Otherwise `+18C = 1,000,000` precedes one shared process draw; its unsigned
LOWWORD shifted4 is added to fresh18C. Target selection uses current unsigned
scores, choosing1EC only when greater than1DC and retaining1DC on ties.

The captured allocation is refreshed through its actual virtual18 owner.
Writeback occurs before a reached refresh error propagates; a normal return
value is ignored. Read actual actor XYZ and cached focus08, reducing each
actor-minus-focus displacement to a signed WORD. Squared DWORD sums wrap.
For bit31 set, distance is `isqrt(sum >> 2) << 1`; otherwise use `isqrt(sum)`.

For signed distance greater than256, X/Z each shift by `14 / distance` with
the x86 five-bit count mask, then receive separate unsigned LOWWORD process
draws multiplied by signed components and SAR16 noise, retaining each WORD.
The close branch instead uses two unsigned LOWWORD draws shifted3: odd distance
chooses `draw + 128`, even chooses `0xFF80 - draw`. It recomputes distance from
original signed Y and the new signed Z/X, then applies the same shift policy.
No terrain sample, camera projection or actor-position store participates.

Fresh actor XYZ is read before publishing computed distance to `+190`, then
fresh current `+148` is dereferenced. Only actual full DWORD kind8 receives
the raw point `[freshX+X, freshY, freshZ+Z]` with WORD wrapping. Other kinds
retain their point; NULL/released current targets stop after190. The current
target allocation, handle and padding survive this write. This callback never
writes14C or198, clones, releases, or substitutes a current-distance default.

Production routes by canonical descriptor identity and actual Selector12
storage. It loans the same retained target heap, live constructor process
source and mutable shared process RNG. Request settings7C is the existing
current canonical refresh input; represented targets do not mutate it.
Arbitrary native refresh overrides remain outside that domain and would need
an actual lazy settings source. Every reached error preserves prior writes.

[Native guards](../../crates/inf-game/src/object_runtime/actor/derived/selector12_point_update/native_guards.rs)
bind complete original wrappers, consumers, constructor/refresh helpers and
final vtable cells. [Intrinsic cases](../../crates/inf-game/src/object_runtime/actor/derived/selector12_point_update/tests.rs)
declare lazy timer/NULL branches, real heap/process custody, refresh writeback,
WORD/root/RNG arithmetic and late current-target failure prefixes. Static bytes
and supplied cases do not establish actual descriptor selection, a normal
session route, or complete retail intro acceptance.

## Selector12 dying descriptor

Both Selector12 profiles select Dying singleton `0054ECD8`, vtable `00526FD8`,
at descriptor index 1. Change wrapper `0043FE40` calls actual actor virtual
`+120`; update `00435240` calls `+11C`. Final class table `00527058` selects
`004678A0` and `004679C0`, respectively. The
[intrinsic owner](../../crates/inf-game/src/object_runtime/actor/derived/selector12/dying.rs)
authenticates actual Selector12 storage; the
[production services](../../crates/inf-game/src/session_platform/world_actor_behavior/dying.rs)
loan current H8 residency, particle/sound pools, process state and shared RNG.

Entry tests only the low BYTE of its argument. Zero returns before actor or
service reads. True entry adds unsigned size WORD `+28` to position Y and
velocity Y with WORD wrapping, clears flag `0x40` through the normal dirty
policy, then consumes three process RNG draws. X and Z each add
`size/2 - (signed_low32(draw_low16 * size) >> 16)` to their velocity WORD.
The signed shift after wrapping multiplication matters for large sizes.
The third draw adds `0x8000 - draw_low16` to angular DWORD `+98`, wrapping.
Dirty BYTE `+39` gains mask `0x01` and model WORD `+42` becomes 248 before `0047A110`.

After normal model refresh, fresh actor XYZ supplies the burst position.
DWORDs `+16C/+170` become `0x8000` before `0048A8B0`: count 16, radius 20,
central type 4, debris types 5/5, speed 8, flags 0. Existing particle producers
own those types and continue the same process RNG. Refresh failure retains
the impulses/model/dirty prefix and old scales; burst failure retains the
new scales and any particle prefix. There is no rollback or post-burst write.

Update ignores its stack delta. Only signed `+188 > 10,000,000` with flag
`0x02000000` clear reaches the current process-NULL read at `0054E894`, then
`0047D670` with argument 0, that source and target state 0. It draws no RNG.
The ordinary descriptor dispatcher owns elapsed-time accumulation after a
normal update. Production uses the shared state-change kernel; the reached
`+C4=1` transition authenticates Selector12 virtual `+B0 = 004026E0`, which
calls `00479FA0`. It queues disposal and retains the actor until the
[next per-frame drain](OBJECT_RUNTIME.md#per-frame-disposal). Other downward
states still require their attribution/StateOne services; this host does not
silently accept those effects.

The [28 entry](../../crates/inf-game/tests/fixtures/selector12_dying_entry_native.json)
and [19 update](../../crates/inf-game/tests/fixtures/selector12_dying_update_native.json)
receipts execute unmodified native bodies, including selected descriptor-thunk
cases, at FPCW `0x027F`. They cover BYTE/WORD/DWORD wrapping, RNG, signed timeout,
flag/source gates, fresh post-refresh reads and stopped service prefixes.
Refresh, burst and state-change returns/mutations are explicitly supplied;
these fixtures do not execute their lower services or establish scene acceptance.
[Consumers and source guards](../../crates/inf-game/src/object_runtime/actor/derived/selector12/dying/native_receipts.rs)
check all 47 cases, final tables, descriptors and complete original bodies.
Separate production tests exercise real model refresh, particle birth and
state-zero disposal. Family staged death has the distinct owner below; furniture
programs use their separate
[radius callback](TERRAIN_RUNTIME.md#furniture-world-radius-callback).

## Family staged dying descriptors

The same Dying wrapper `0043FE40` reaches selector10 `+120 = 00425840`,
which runs shared entry `00424280` before disabling its retained animation
helper. Selector36 uses the shared entry with its distinct stage callback.
[intrinsic family_dying](../../crates/inf-game/src/object_runtime/actor/derived/family_dying.rs)
owns entry, stage transitions and update through one data-bearing request;
[production services](../../crates/inf-game/src/session_platform/world_actor_behavior/family_dying.rs)
loan the actual arena, mission, tasks, terrain, factory, particle/sound owners
and process RNG. No captured scene values select these policies.

Entry tests only the low argument BYTE. True entry sets stage `+240 = 0`
and invokes actual stage `+138` before publishing status `00530D3C` and flag
`0100`. Mode `+174 = 2` reaches the camera flash and world-radius block
`005307D8`. Sound `+182` retains its resource gate; fresh XYZ precedes scale
stores `+16C/+170 = 8000`. Flags `+264` select the authored one/two bursts,
debris types and later radius block `00531250`. Radius uses the source actor's
handle and replication argument 1 through the shared live radius owner.

The optional `(flags264 & 0380) >> 7` spawn loop retains its own player,
distance and construction phases. It creates H13 resource 84 for owner18,
resource2 otherwise; only a resolved `+18` result receives the owner's B4
and dirty bit before `00405420`. That helper's secondary resource46 construction
is a separate required service until its complete lifecycle is admitted.
A NULL factory/component result follows the actual bypass. Persistent audio is
stopped only after this loop returns normally; selector10 helper disable is the
last wrapper phase. Errors preserve all earlier effects and allocations.

Shared update `00424590` compares signed elapsed `+188` with the current
`+244[stage]` timer. Expiry clears elapsed before stage exit, rereads stage,
increments it and enters the new stage. Stage1 is entered and then immediately
exited when flag20 is clear; its pickup prefix is therefore still required.
Stage1 updates each scale with its own RNG draw, creates/updates sound47,
and retains the rare sound66 branch's otherwise unused draw. Its later
`0047DFD0` call is status-visual work, not the physics routine. Stage2 stops the
persistent record. Stage3 retires offscreen; an expired stage3 timer retires
regardless of visibility. Final `0047D670` requests state0 with the current
actor handle. Family base/10/36 `+B0 = 004026D0` is RET8: this transition
changes lifecycle without immediately queueing object destruction.

Stage entry `00424080` (selector10) and `00423EE0` (shared) emits three actual
kind83/value500 [timed pickups](PICKUP_RUNTIME.md), even when stage1 is skipped.
Each captures the actor basis Z vector, uses float multiplier `00520CC0`, then
rotates the stack vector by `5555` through `00491D10`. Selector10 uses startup
axis `0055F280` (+Y); the shared stage uses `0055F290` (+X). Their separate
helper-release flags and model transition remain ordered around these births.
Selector36 wrapper `00481EA0` adds its real `004824A0` child gate at stage0
and four resource58 child/burst pairs at stage1; the children are
[selector 22 fading visuals](SHARED_ACTOR_FACTORY.md#selector-22-fading-visual). Update wrapper `00481FC0`
consumes its first RNG after every normal shared return, including state0;
its signed division precedes the stage-range guard. Authored trail emission
calls `00489D90` with flags `0800`, continuing the same RNG stream.

The [entry42](../../crates/inf-game/tests/fixtures/family_dying_entry_native.json),
[update39](../../crates/inf-game/tests/fixtures/family_dying_update_native.json)
and [stage35](../../crates/inf-game/tests/fixtures/family_dying_stage_native.json)
receipts execute original bodies at FPCW `027F`. The
tracked generator reproduces all three;
NATIVE_RECEIPTS.md records its compare/regeneration command.
The
[consumer](../../crates/inf-game/src/object_runtime/actor/derived/family_dying/native_receipts.rs)
compares 111 admitted cases; five explicitly supplied native helper faults or
mutations have no infallible retained-helper equivalent. Lower factory, burst,
audio and state effects are supplied boundaries in these receipts. Production
composition and complete visible sequence remain separate acceptance evidence.

## Shared object-target change

[descriptor_object_target.rs](../../crates/inf-game/src/object_runtime/actor/derived/descriptor_object_target.rs)
owns complete `004288E0`, using the actual virtual `+14` object identity.
False entry bypasses receiver/heap/process reads. True captures raw `+1FC`,
then publishes status `005311F0`. NULL source or a genuine NULL virtual `+14`
result returns with only that reached status. This is a different interface
from the class12 preferred change's virtual `+10`.

After a nonnull object result, the consumer freshly reads `+1FC` for its clone,
publishes the clone to `+148`, then reads original object and actor XYZ. Each
displacement wraps as a signed WORD; squared DWORD sums and the native bit31
square-root repair produce distance `+14C`. Only afterwards does unsigned
actor size produce wrapped spacing `+208 = -(size << 2)`. A normal NULL clone
still reaches these stores. Missing XYZ leaves the published current pointer
and actual clone allocation intact. Whole-XYZ backing does not represent
fault ordering between individual legacy WORD reads.

## Shared Herding change and update

[descriptor_herding.rs](../../crates/inf-game/src/object_runtime/actor/derived/descriptor_herding.rs)
owns the Herding descriptor. Wrappers `0043FC00/0043FC20` call `00427A70`
and `00427AA0` directly, so family, Selector34 and Selector35 profiles share
one owner. True entry captures `+1E4`, publishes status `005311DC` and clones
a nonnull herd target into `+148`. The kind-8 clone takes the live process
NULL as its handle and copies only the focus. False entry reads nothing.

Update compares signed `+18C` with the signed delta. Herding entry never
writes `+18C`. When Herding is an actor's first descriptor, production
supplies it from the labeled
[allocation carry](../../crates/inf-game/src/fresh_heap.rs); strict arenas
stop at that read. Above it, status `+150`
is read; `>= 3` only decrements the timer. Otherwise `+18C` gains 250,000
plus `(draw & FFFF) << 2` from one process draw. Group scan `00427E70` then
walks the live actor list, self included. Members need the same `+B4`, the
same resource WORD `+30` and `(flags & A0) == 20`, and their wrapped signed
offset's squared DWORD must be below unsigned `+164²`. Each adds its offset
plus linear velocity. More than one member gives signed truncating means.
`20000h < |mean|² < +164²` publishes self plus mean into `+1E4` through
`00427390` with score `isqrt(|mean|²)`. Anything else publishes the NULL
point, which leaves an empty slot's score untouched. An empty `+1E4` then
sets `+22E` bit 0. Otherwise a kind-8 `+148` takes the herd focus. A NULL
`+148` faults natively at `00427B0A` after the publish and is a typed stop.

## Preferred-target update and grant notification

[selector12_preferred_update.rs](../../crates/inf-game/src/object_runtime/actor/derived/selector12_preferred_update.rs)
owns complete `00467660` and its separate `00467720` notification phase.
A fresh NULL `+148` remains a required dereference. Kind DWORD 2 resolves actual
virtual `+10` and reads the referent's unsigned WORD `+7A`; genuine NULL or zero
grant ORs low BYTE `+22E` with `0x0C` and returns before distance/timer sources.
Every other kind reads raw focus Z then X and uses the canonical `00436BC0`
terrain/furniture grant lookup, with actual current H12/H11/EXE custody.

The counted retail distance history
has no CPU access before the frozen first-periodic-callback endpoint for one
authenticated allocation. A bounded continuation then observes its first
recorded DWORD publication: the class12 movement tail at `00467C00` stores
computed EDI to actor `+14C`, after virtual +D8, before the selected preferred
read hook is reached in that interval. The indexed four-byte Write and the
complete update body attribute this writer; the watch's next EIP alone does not.

This proof covers native placement `002F` / Primary reference48 with frozen
actor and handle. It supplies no initial value or allocation default, no
portable-slot mapping and no policy for other actors. A nonzero-grant preferred
update that demands `+14C` before a source-owned writer still stops at the
required backing boundary. The observed `00001FFE` is evidence of that one
computed publication, never a seeded timer/distance or replacement for the
actual movement/target path. Later preferred consumption remains unobserved.

A nonzero grant requires signed `+14C`, then signed `+198`, then signed `+190`.
When distance exceeds threshold, wrapped subtraction, signed cap of 920, wrapped
multiplication by the full DWORD delta and SAR10 produce the effective delta.
A greater remaining timer only decrements `+190`. Expiry ORs `+22E` with `0x0C`,
writes `+190 = 0` and `+194 = 1`, then calls the actual embedded `+24C` live grant
setter. Errors preserve those stores and any real grant mask/resource prefix.
Only normal setter return reaches the separate notification.

The notification freshly selects the actual first player. NULL player bypasses
receiver/mask reads; otherwise the canonical single-word Player `+9C` query
sets actor BYTE `+29D` to 1 when the player lacks the grant, else 0. Actor grants
never substitute for the player mask. Production lends the live process,
executable and residency to the existing grant setter. Its constructor-NULL
notification binding bypasses text formatting; any reached missing text or
changed binding remains the canonical error. Native upper EBX is represented
only within the proved downstream LOWWORD grant domain.

## Shared route-target update

The same object-target module owns complete `00428830`. NULL current `+148`
stops before virtual `+14`; a valid target returning NULL sets only low BYTE
mask `0x04`. Signed retained `+14C < 256` asks the canonical route for its next
node, setting that mask only at a genuine endpoint. This near branch never
requires segment length or actor XYZ.

The far branch first checks the source's squared segment DWORD, then obtains
the next node and its/actor XYZ. Signed WORD differences and wrapped DWORD
squares are compared unsigned against a fresh segment DWORD. Equality does
not advance; no square root or bit31 repair belongs here. A chosen successor
rereads current `+148` and uses the existing target virtual `+24` owner. Changed
referent identity is committed even if its later whole-XYZ read fails. The same
allocation survives; no cloning, release, RNG, timer, collision or process
source is introduced. Selector35's different `+EC` belongs to its
[class authority](SELECTOR35_RUNTIME.md#construction-and-descriptor-selection).

## Common timed target reselection

[descriptor_reselection.rs](../../crates/inf-game/src/object_runtime/actor/derived/descriptor_reselection.rs)
owns common Evade entry `+128 = 00423E50` through `00435150`, and
`+124 = 00423BD0` and `+130 = 00423A20`, reached through
`00435170` and `00425D90`. Signed +18C and +150 gates retain lazy reads.
Point update sets +190 to 20000 on its timer-retention branch; retry update
only subtracts the delta. Expiry uses the live RNG for the respective authored
two-million/half-million timer, and retry ORs bit10 into low +22E.

Point reselection chooses +1EC only when its unsigned score is strictly
greater than +1DC; ties retain +1DC. The selected target's real refresh runs
before cached focus is read, with its numeric return ignored. WORD distance,
near/far perturbation and RNG order follow original arithmetic. Publication
of +190 precedes current +148 access; only a kind8 destination receives the
new point. A refresh or null-target failure retains its reached prefix.
Production lends current targets, RNG and settings without moving the later
class movement writer ahead of descriptor dispatch. Selector12's actual
`+124/+130` both remain the empty `00404840` leaf.

Evade entry admits only the raw argument's low byte. A false byte returns
without reading actor storage. A true byte writes status `00530D30`, allocates
the fixed-point target, and reads current process NULL and actor XYZ only after
nonnull allocation. It publishes `+148` even for genuine allocation NULL,
zeros `+18C/+190`, draws the live RNG once, clears byte `+22C` and DWORD `+208`,
and writes `+194 = 2000 + ((random & 65535) >> 5)`. The old target is not
released by entry; the dispatcher's earlier target-clear phase owns that work.
Parent/class10/class36 share this body. The explicit `FamilyEvadeEntryKind`
selects selector35's distinct `0043DE00` policy: the same prefix completes
before `+234` and then `+230` become 500. Errors and false entry preserve both
fields. Production selects the actual class policy and lends the live
factory-process NULL source rather than the registry's construction value.
Both actual update continuations remain separate; their timer-zero gate can
continue without inventing unknown `+150` backing.

## Verification and remaining work

check_descriptor_reselection.py
reproduces 61 `00423BD0`/`00423A20` records in
`descriptor_reselection_native.json`. With Python 3 and Unicorn 2 installed,
run `python check_descriptor_reselection.py`
from the repository root; its default compares the tracked bytes, while
`--output <path>` explicitly writes regenerated LF JSON. The original RNG and
integer square root execute. Only selected-target virtual `+18` is supplied;
its live/stale/failure prefixes establish caller ordering, not the lower
callback implementation or cinematic parity.

check_family_herding.py reproduces 26 Herding
change/update/scan cases in `family_herding_native.json` (default compares,
`--write` regenerates). The actual change, update, group scan, publisher,
kind-8 clone/deletion and RNG run unmodified; allocator/free returns and the
`G+48` actor list are supplied, and the NULL `+148` fault is recorded as a
receipt event.

check_family_evade_change.py reproduces
56 `00435150`/`00423E50`/`0043DE00` cases in `family_evade_change_native.json`. Run
`python check_family_evade_change.py` to compare
tracked bytes; `--write` explicitly regenerates LF JSON. The actual thunk,
constructor instructions and RNG run unmodified over supplied actor storage.
Allocator returns and stopped source reads are explicit boundaries. Tests
compare actor/RNG prefixes, target ownership and complete constructed targets;
they do not claim raw partially constructed target-byte parity or heap history.

The tracked module-local native guards bind complete original bodies, final
vtable slots and descriptor pointers. Intrinsic tests exercise real retained
heap resources, current process sources, lazy gates, unsigned choices, NULL
clone results and reached error prefixes. Normal session replays validate host
composition independently; supplied fixtures and static bytes do not establish
the complete retail cinematic.

Remaining owners include required initial allocation backing and reached
class-specific contact or factory services. [Selector35's distinct
movement and lifecycle](SELECTOR35_RUNTIME.md) use their actual class owners.
Family dying lower services must
complete normally; a typed factory/helper boundary is not a completed death. The accepted eye
appearance is preserved; complete soldier, ship and enemy simulation remains
part of the intro priority.
