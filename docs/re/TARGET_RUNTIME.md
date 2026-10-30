# Infestation shared target resources

This authority owns the target allocations shared by the camera, intro helpers
and object followers. [targets.rs](../../crates/inf-game/src/object_runtime/targets.rs)
implements native type 2 (`00528038`, 0x20 bytes), type 8 (`0051F154`,
0x14 bytes) and type 16 (`0051FB6C`, 0x18 bytes). Actor handles and
common-object identities stay distinct. These allocations own copied points
and may retain referent identities; destroying a target does not destroy its actor
or object. [CAMERA_RUNTIME.md](CAMERA_RUNTIME.md) owns camera selection, view
modes and smoothing. [INTRO_SEQUENCE.md](INTRO_SEQUENCE.md#effects-and-remaining-live-owners)
owns selector-90 movement, and [ROUTE_RUNTIME.md](ROUTE_RUNTIME.md) owns route
nodes and groups.

Authority is the supported retail executable's unchanged instructions. The
bulk decompile is a guide, particularly `004825D0..00482940`, `00462CC0` and
`004073A0`. The fixtures described below are isolated routine evidence, not
acceptance of the complete intro or a mission-specific target graph.

## Actor-follow construction and refresh

`004825D0` stores an actor handle at +04 and **DWORD** kind 2 at +10. Its
point-pointer argument selects two different constructor paths:

| Constructor input | Reached native writes |
| --- | --- |
| Null point pointer, live handle | Resolve through `0047AD90`; copy actor XYZ at +32 to target +08, adding unsigned WORD +74 to Y with WORD wrap; copy those six bytes to +14. |
| Null point pointer, unresolved handle | Neither point span is written. |
| Nonnull point pointer | Copy six supplied bytes to +14 only; do not resolve the handle or write +08. |

Neither path writes velocity +1A. The public `FollowTargetRequest` names the
choice as `ResolveActor` or `ExplicitPoint`. An explicit point can carry
unknown copied words; that unknownness is separate from the constructor mode.
The port never substitutes a zero position for unwritten native storage.

Virtual +18 (`00482690`) refreshes a live handle's +08 from XYZ and unsigned
WORD +74. It copies signed velocity WORDs +8E/+90/+92 to +1A, negating each
with WORD wrap when actor flags include `0x40000`. Only then does nonzero
settings +7C copy +08 to +14. The return is the actor's full **DWORD** +C4
lifecycle state. An unresolved handle returns zero without changing any of
the retained vectors. No actor class number controls this refresh.

Virtual +0C (`00482740`) returns the live actor's persistent basis at +44,
including its rotation counter, or the process identity basis for an
unresolved handle. It independently resolves the handle; a previous refresh
does not permanently bind a particular actor pointer.

Clone `004828D0` reruns the explicit-point constructor with the old +04 and
+14. Therefore cloning does not look up the actor, copy the refreshed +08
or copy velocity. An unknown +14 remains unknown in the clone. Rust's
`clone_native` names this policy explicitly.

Virtual +20 replacement `00482760` reads a nonnull input actor's +84 and
publishes that handle before resolving it through `0047AD90`. A resolving
replacement writes target +14 alone from actor XYZ plus **unsigned size WORD
+28 divided by four** on Y with WORD wrap. It preserves +08 and velocity,
unlike constructor/refresh +74. An unresolved replacement succeeds without
point writes. Null actor input publishes the process null handle and succeeds.
The shared `replace_follow_target` owner retains the new handle if a reached
position read is unavailable.

## Common-object position targets

Selector-90 owner `00462CC0` constructs the type-16 target inline at
`00462CEA..00462D11`. Clone `004073A0` independently reconstructs the same
allocation. Both store process null actor handle `0054E894` at +04, current
common-object XYZ at +08, DWORD kind 16 at +10, and the actual common-object
pointer at +14. The actor handle is not the referent identity. The shared
Rust owner accepts `ObjectId`; the route caller additionally requires a route
node before invoking it.

Virtual +24 (`00482860`) always succeeds for an identical +14 identity and
does not reread its XYZ. For a changed identity, it stores +14 **before**
reading and copying the new XYZ. The Rust error path retains that changed
identity with the old cached point if the reached XYZ read is unavailable.
A native clone reads the referent's current position rather than copying
the old cache. Selector-90 movement likewise reads live route coordinates
through +14; moving a node without replacing its identity does not refresh
the target's cached +08.

Its refresh (`00402700`) returns 4 without mutation. Its velocity query
returns process vector `0055F0F8`; its basis query returns the process
identity basis. These are native class policies, not substitutes for
unimplemented actor-follow state or other target subclasses.

## Caller decision matrix and lifetime

The shared owner preserves these caller-specific rules:

| Caller | Target and replacement policy |
| --- | --- |
| Player attachment/respawn, camera reselection, intro helper tracking | Construct type 2 through `ResolveActor`, so a live actor writes both points. |
| Camera primary-to-secondary copy | Reconstruct a present primary through its explicit +14 point without a registry lookup; absent primary clears secondary. Refreshed vectors remain unwritten. |
| Camera and selector-90 facing | Refresh type 2; stale handles preserve vectors, and settings +7C gates only the +14 copy. |
| Camera basis and view-mode queries | Resolve the live actor independently; retain the existing view-mode retry asymmetry. |
| Selector-90 route installation | Clear the old resource before validating the new route/position. Preserve arrival byte +140 and fractional remainders. |
| Selector-90 route advancement | Use live +14 route XYZ, retain same-identity cache, and publish a changed identity before its XYZ copy. |
| Weighted actor publisher `00427110` | Type 2's virtual +20 retains the allocation and updates handle/+14. Type 16's same slot is `00402720` false, requiring deletion and a new type-2 constructor. |
| Weighted object publisher `00427220` | Type 16's virtual +24 retains the allocation and uses the identity policy above. Type 2's same slot is `00402720` false, requiring deletion and a new type-16 constructor. |

Camera primary replacement `00410E70` first asks the old target's actor to
return to mode zero, destroys the old target and sets camera +188 to 3. It
then installs the new target and reapplies First Person. Secondary replacement
`0040F710` only destroys/stores the secondary target. These are camera
policies at the existing live call sites, not additional generic target modes.

Shared target destruction `00482840` calls base destruction `00407400`,
restores the base target vtable and frees the target when requested. It has
no actor/object retirement effect. The arena's retained actor-service heap
owns allocation lifetime separately from copied target data and referents.
It does not claim native allocator addresses.

## Copied world-point targets

[fixed.rs](../../crates/inf-game/src/object_runtime/targets/fixed.rs)
owns the genuine type-8 resource, independent from scene or actor selector.
The native constructor is inline at clone `00402730`, point publisher
`00427390` and several descriptor producers. A successful 0x14-byte
allocation receives vtable `0051F154` at +00, the current process null
ActorHandle `0054E894` at +04, six copied world-position WORDs at +08,
and **DWORD** kind 8 at +10. +0E/+0F are unwritten allocation padding;
the portable resource does not invent their values. It retains no actor or
common-object identity.

The vtable ends at `0051F17B`, before the next unrelated table. Its complete
callback policy is:

| Slot | Native callback | Policy |
| --- | --- | --- |
| +00 | `00482840` | Call base destructor `00407400`, restoring `0051F19C`; flags low-byte bit 0 gates `005116F4` free. No referent deletion. The base table's own `+00`, `00402770`, only restores it and frees on the same bit. |
| +04 | `00402730` | Allocate 0x14, read the current process null handle, and copy the old +08 point. Do not copy old +04 or padding. Actual allocation null returns null before handle/point reads. |
| +08 | `004026F0` | Return the actual process vector at `0055F0F8`, without resource reads. |
| +0C | `00401390` | Return the actual process basis at `00547590`, without resource reads. |
| +10 / +14 | `00401300` | Return null without actor registry or object queries. |
| +18 | `00402700` | Return DWORD 4 without mutation. |
| +1C | `00402710` | Return the stored six-byte +08 point. |
| +20 / +24 | `00402720` | Return AL false without reading the supplied actor/object. |

`clone_native_target` reconstructs this resource in a new retained allocation
and preserves unknown copied +08 words. Type-2 clone keeps its
handle and copies explicit +14, whereas type-16 clone rereads live object
XYZ; Rust `Copy` is not any of these native clone operations. The shared
`point_words`, `velocity_words`, `native_target_basis` and
`refresh_native_target` dispatch these proven class policies. Process vector
and basis are explicit supplied inputs: a required missing source reports
`NativeTargetQueryError`, never a zero vector or an invented identity.
A live type-2 basis query needs its actor basis instead; its velocity query
exposes retained +1A words. Unknown retained words remain unknown.

Canonical owned target deletion callers use flags 1. Their logical resource
release models base destruction plus free and preserves actor/object
referents. The native nondeleting destructor branches (flags 0/2) restore a
base vtable while retaining the allocation; that base-class live resource is
outside the portable enum. Direct native receipts document those branches
without treating them as portable deletion or claiming resumable callbacks
on that unrepresented base target.

Point publisher `00427390` has a distinct replacement policy. Valid point
and nonzero score write score before inspecting the old target's actual
**DWORD +10**. Kind 8 copies six bytes directly to raw +08, preserving the
allocation and handle, even if the admitted resource's vtable class differs.
This is not virtual +20/+24 acceptance; those callbacks on genuine type 8
remain false. A different kind requires old-resource deletion and a new
type-8 constructor, with the same Released custody gap described below.
Invalid input preserves score when the pointer is empty, or deletes a present
target before publishing null and zero. Returned AL reports the prior empty
pointer, including real allocation null, rather than successful allocation.

`object_position_target` also retains the proven constructor read order:
process null handle before live XYZ. Clone `004073A0` reads `0054E894` at
`004073B4` before its XYZ read at `004073C9`; the weighted type-16 constructor
shares this order. When both process handle and XYZ are unavailable, the
reached process-handle error wins. Intrinsic value construction retains the
explicit `ActorRegistry` fixture mode. Retained resource construction, clone
and weighted publishers accept a live process source, read only after a
successful allocation; that source remains independent from the registry.

check_fixed_targets.py executes all ten real
type-8 methods and all three weighted publishers, including canonical and
mismatched raw kind/vtable inputs. Its
[fixture](../../crates/inf-game/tests/fixtures/fixed_targets_native.json)
contains 290 unchanged-instruction cases, with 30 reached stopped prefixes.
[Rust comparisons](../../crates/inf-game/src/object_runtime/targets/fixed/native_receipts.rs)
cover every one of the 283 portable cases: class fields, preserved caches,
real allocation sizes and nulls, unsigned scores, published/released custody,
process query values, unknown copied points and surviving referents.
An additional pass compares all 266 portable publisher cases with the explicit
`Process` constructor source and an intentionally different registry null
handle. It checks the original process-null stops, accepted replacements and
allocation-null branches without changing any fixture or native instruction.
Seven cases remain explicit native-only evidence: the two nondeleting
destructor branches and five forced stops at supplied/copied raw point reads.
An owned array or unknown copied span is an infallible portable copy, so those
five stops do not manufacture a failed pointer-read effect. Allocation,
process-handle and actual unavailable arena-position frontiers remain directly
comparable. Raw poison, native addresses and unpublished constructor words
remain diagnostics, not allocator defaults or live resource values.

The concrete descriptor producers and StayStill update are owned by
[ACTOR_BEHAVIOR_RUNTIME.md](ACTOR_BEHAVIOR_RUNTIME.md#copied-point-descriptor-changes-and-staystill-update).
They retain distinct Roam/Home/StayStill prefixes and point sources; Home
update004026D0 is a no-op. Remaining point producers, Roam update and complete
actor+24 attachment stay outside this resource owner.

## Weighted pairs and family target ownership

[weighted.rs](../../crates/inf-game/src/object_runtime/targets/weighted.rs)
owns decoded `NativeTarget` data for those three proven classes, a
`NativeWeightedTarget` pair containing a nullable `NativeHeapRef` and score,
and a data-bearing `WeightedTargetRequest`. `Actor` takes an actual
`ObjectId` actor referent before reading its handle; `Object` retains a
common-object identity. `Point` supplies a nullable six-byte world-point
pointer to `00427390`; its pointer null is distinct from unknown copied
target words. An actor pointer input does not turn type 16 into
type 2 without the native false callback and reconstruction. Virtual +10
independently resolves a type-2 handle; types8/16 are original null `00401300`.
Virtual +14 retains the opposite distinction: null for types2/8, actual object
identity for type 16. Clone allocates through the explicit service and keeps
each class's existing native clone policy.

`WeightedTargetRequest.constructor_null` names the reached constructor operand.
`ActorRegistry` is the explicit decoded-fixture mode; production
`Process(&dyn TargetNullHandleSource)` borrows the actual retained process
owner. The constructor invokes its getter after successful allocation and
after preceding mutable callbacks, independently of registry state. No
missing process value becomes an invented default.

Type-8 publication reads `0054E894` at `004273D3` after crossed-target deletion
and allocation, or `00427431` for an initially empty slot. Type-16 publication
does the corresponding read at `0042726D` / `004272BA`, before object XYZ.
A missing value from the live getter reports
`NativeTargetError::MissingProcessNull` only at this reached phase. The score
remains published; a deleted old target leaves a nonnull raw reference to a
`Released` heap entry, while an initially empty slot stays null. The new
allocation remains `Pending` if its constructor stops.
Null input, score zero, actual allocation null, accepted replacement and
type-2 actor-follow construction never consume this operand. Accepted kind-8
point writes and type-16 replacement preserve the old +04 handle even when
the current process null differs or is unavailable.

All three publishers preserve this decision matrix:

| Input and previous pointer | Score and lifetime writes | Returned AL |
| --- | --- | --- |
| Null referent or zero score; old pointer null | Preserve the old score; no allocation/deletion | False |
| Null referent or zero score; old pointer present | Delete target, publish pointer null and score zero | False |
| Valid referent/nonzero score; old pointer null | Publish score before allocation; install constructed target or actual allocation null | True, including allocation null |
| Valid input; old pointer accepts replacement | Publish score before replacement; keep target with callback writes | False |
| Valid input; old pointer rejects replacement | Publish score, delete old target, allocate and reconstruct or publish actual allocation null | False |

At `00427398` / `004273A4`, a null point pointer or zero score branches to the
same invalid-input path at `0042746B`, before any point-byte or handle read.
Consequently a nonnull but unwritten stack point with score zero has the same
no-read, target deletion/null publication, score and AL observations as the
portable `Point(None)` invalid input. This bounded equivalence does not claim
that the unwritten bytes form a known point or that an owned array reproduces
native pointer faults when a positive score reaches its copy.

Scores are raw unsigned DWORDs; high-bit scores are neither negative nor
null. Allocation does not decide behavior selection or supply candidates.
Reached score, handle and object-identity writes survive errors.

### Retained actor-service heap

[heap.rs](../../crates/inf-game/src/object_runtime/targets/heap.rs) is owned
by the actual `ObjectArena`, independently of a temporary callback host.
`NativeHeapRef` carries arena and allocation identity. Copying a field aliases
that allocation; target virtual `+04` allocates a distinct resource. A short
`NativeTarget` value is decoded data, never an owned pointer or a clone.
The same heap owns the genuine eight-byte `00427640` radius-score input.
Target kinds request the original 32, 20 or 24 bytes explicitly.

`PortableFallibleActorHeapApproximation` reserves real fallible backing,
securing metadata before payload reservation. Unknown bytes remain unwritten.
It models successful reservation or allocation null, without claiming Windows
heap reuse, addresses, allocator history or `00511708` new-handler cadence.
Allocator-history reconstruction is the follow-up owner; this policy is not
retail acceptance. Fixture scripts select actual reservation, null or an
explicit supplied failure through the same owner.

Entries progress from `Pending` to constructed target/temporary data and then
`Released`. A constructor failure retains pending backing until an explicit
release; assignment does not release resources. Release drops backing and
retains an identity tombstone, preventing alias dereferences from exposing
freed data. Wrong-arena, wrong-kind, pending and released queries are errors.

Crossed-class deletion precedes allocation and leaves the old **raw** owner
reference untouched until a completed constructor or genuine allocation-null
store. A constructor/service error preserves that released reference and the
already-published score. An initially empty slot stays null. A later valid
publisher writes score before its released-target dereference error; invalid
input reaches deletion before clearing score. No separate pointer-custody
Boolean can disagree with the heap's actual state.

[family_targets.rs](../../crates/inf-game/src/object_runtime/actor/derived/family_targets.rs)
adapts the shared pair owner to common family slots
+1D4/+1DC/+1E4/+1EC/+1F4/+1FC and independent selector-12 slots
+26C/+274/+27C/+284, retaining their existing separate score words. Explicit
`COMMON` and `SELECTOR12` policies select real storage, without a public
numeric array index or duplicate publication APIs. The `00428650` and
`00467070` constructors write null pointers and zero scores. The 00425E60
prefix now owns concrete optional resources at +148/+154, initialized null.
Derived +14C/+150 are represented as unknown DWORDs for the
[common Attack update](ACTOR_BEHAVIOR_RUNTIME.md#common-attack-descriptor-update);
they and unrelated +158/+15C allocator holes retain no invented defaults.
Current `+148` and `+154` contain the same nullable retained references as
weighted source slots. The common Attack update deletes current `+148` before
calling the selected target's clone and leaves its raw reference until clone
or allocation-null publication. Native pointer-presence gates still admit a
released reference; actual handle, point, kind or virtual dereferences report
the heap's released boundary. Destruction preserves reached writes on failure
and clears the field only after completed cleanup. This prevents a portable
retry from silently reading freed target data.

The common clear helper `00429460` deletes +148 then +154, nulls them, and
clears byte +22C without changing weighted source slots. Actual vtables select
one of four clear policies; the dispatcher directly invokes this owned
operation instead of a whole-clear host callback:

| Native class | Vtable +10C | Suffix after `00429460` |
| --- | --- | --- |
| Base family428650, probe-only family429e20 | `00520F34` / `00521164` → `00429460` | None; preserve +208 |
| Family4236d0, selector 10, selector 36 | `00520B84` / `00520CC8` / `00527EE0` → `00425B70` | +208 = 0, then +260 = 1 |
| Selector 12 | `00527058` → `00468DE0` | +208 = 0 |
| Selector 34, selector 35 | `00521EAC` / `0052273C` → `0043F910` | Clear flags `0x04000000` with original dirty test, then +208 = 0, +234 = 200, +230 = 150, +238 = 1 |

Common destruction `00428750` releases the helper +218 before deleting source
targets in reverse field order. `00425F70` then deletes current +148/+154
before behavior +140. Retired Rust snapshots relinquish resource custody
without retaining native dangling allocation pointers or changing scores.
Destruction stops at a `Released` slot in the native reverse-field order,
after the helper and already reached higher targets. Those freed higher
slots also retain `Released` on a partial destructor, preventing a retry
from treating them as proven null pointers. Current-target clears preserve
weighted source custody. `family_weighted_target` returns one safe data-bearing
snapshot, allowing pointer presence/score comparisons without invoking a
released target's virtual. A selected released target still fails explicitly
at the actual dereference boundary.
That retry is a portable custody safeguard, not an exact native destructor
retry claim: native +218 and earlier probe/selector-10 helper frees leave raw
owner pointers dangling, while their existing Rust owners relinquish storage.
The original first-attempt release order remains the authority. Selector
12's `004671E0` deletes +284/+27C/+274/+26C before the lower probe and common
owners, with the same reached `Released` boundary and higher-field custody.
Its +10C method preserves all four class source slots and scores.
Only completed lower destruction resets the class's logical custody to
`Published`/null in a retired snapshot, matching the common six; a lower
failure preserves the already released class prefix and actor registration.
Subclass destructors `004237D0`, `004671E0`, `00433B90` and `0043D0F0` require
an active descriptor's change callback before this cleanup. The port reports
`FamilyBehaviorDestructionBoundary` on that reached callback and preserves
lower resources and actor registration. Selector 10 first releases its
+26C helper, matching the native prefix before `004237D0` reaches the boundary.

Type 4 remains a separate unrepresented native subclass. Type 8 has the
bounded copied-point, clone/query and descriptor owners above. Type 4 requires
its actual producers and consumers before a production API is added.
Complete family control +24,
integration of the separately owned scan/avoidance routines, weapon/projectile
callbacks and full intro actor execution remain open.

## Native evidence and verification

check_target_resources.py produces 43
receipts by executing original
constructor, refresh, clone, identity-replacement, query, destructor and
handle-resolver instructions. It supplies the registry/cache, object data,
settings and process vectors as explicit inputs; the original settings getter
and identity-basis initializer also execute. Allocation/free and camera
view-mode effects are bounded hosts. Write tracking distinguishes unwritten
constructor spans from supplied bytes. The
[receipt](../../crates/inf-game/tests/fixtures/target_resources_native.json)
is checked by [native comparisons](../../crates/inf-game/src/object_runtime/targets/native_receipts.rs),
with separate [custody tests](../../crates/inf-game/src/object_runtime/targets/tests.rs).
Rust comparisons cover the implemented resource methods and live helper/camera
callers; native-only allocation failures and currently unreached type-16
getters remain fixture evidence. Camera replacement receipts stop at the
required view-mode host, so their mode snapshots describe callback entry;
the existing concrete view-mode tests verify final camera behavior. Focused
camera/helper tests retain their own replacement order and movement contracts.
No recorded coordinates or mission selection determine behavior.

check_weighted_targets.py adds 110
unchanged-instruction publisher receipts and eight current-target clear
receipts. It executes actual target constructors/replacements/destructors and
the canonical handle resolver with independent pair/object/registry inputs.
Only allocation/free are hosts. Cases cover both classes and crossed virtuals,
null/zero inputs, high-bit scores, fresh-slot allocation failure, stale handles,
same/changed object identities, and class clear suffixes. The
[fixture](../../crates/inf-game/tests/fixtures/weighted_targets_native.json)
is compared by [publisher tests](../../crates/inf-game/src/object_runtime/targets/weighted/tests/native_receipts.rs)
and [family lifetime tests](../../crates/inf-game/src/object_runtime/actor/derived/family_targets/tests.rs).
These receipts establish pair ownership and mutation, not descriptor
selection or enclosing actor +24 execution. The separately owned
[periodic player and route target producers](ACTOR_BEHAVIOR_RUNTIME.md#periodic-player-and-route-target-producers)
now implement `00427490`/`00427510`, radius score `00427620` and the distinct
eight-byte temporary factory `00427640`. Their 43 original-instruction
receipts include the literal periodic wrapper and host allocation/free,
actual first-player +D4 registry resolution and selector-51 route traversal.
The separate [family table scanner and common selection callback](FAMILY_TARGET_SELECTION.md)
now compose `00428BB0`/`00428DB0` with live task sources, the candidate gate,
retail entry pruning and both weighted publishers. This shares `00427620`
without replacing the periodic producers' distinct distance calculations.
Eight publisher cases stop original instructions at the required allocator
or constructor position-read boundary. They independently prove that the
released old pointer remains in a crossed-class owner slot, while a fresh
empty slot remains null. These stopped prefixes establish pointer-write
ordering; they do not manufacture an executable exception outcome.

From the repository root, reproduce the receipts with Python 3 and Unicorn 2
available to that interpreter:

```powershell
python check_target_resources.py --output .tmp/target_resources_native.json
python check_weighted_targets.py --output .tmp/weighted_targets_native.json
python check_family_target_producers.py --output .tmp/family_target_producers_native.json
python check_fixed_targets.py --output .tmp/fixed_targets_native.json
python check_family_descriptor_points.py --output .tmp/family_descriptor_points_native.json
```

These fixtures do not establish allocator reuse, complete target selection,
the remaining target subclasses, or a retail cinematic comparison. Menu,
intro camera, briefing and gameplay visual smoke checks verify the connected
presentation paths; they remain separate from retail acceptance.
