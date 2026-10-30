# Infestation live object construction

This document owns the recovered live construction and custody contract.
[`ObjectArena`](../../crates/inf-game/src/object_runtime.rs) implements the
common base's live fields, ordered embedded-node memberships, flag setters,
disposal and destruction. Its
[`construct_world_object`](../../crates/inf-game/src/object_runtime/construction.rs)
consumer admits selectors 1, 2, 6, 7, 9, 10, 11, 12, 14, 15, 16, 19, 22, 30,
33, 34, 35, 36, 37, 39, 51, 54, 58, 60, 77, 86, 87, 88, 89, 90 and 93 in
world-construction mode 0. Selectors 14 and 89 are the
[placed and timed pickups](PICKUP_RUNTIME.md). Selectors 15, 37 and 39 are the
[0040D800 family's](FAMILY40D800_RUNTIME.md) factory, bridge and course
target. Selector 58 (`004753E0`, final `00527838`) is a
rotating-visual pad: `0047A610` context, payload flags, and a `+14`
(`004755B0`) that returns its `+6E` timer for selector 1. Its `+24` update
`00475420` is [the save pad](VEHICLE_DRIVE_RUNTIME.md#the-save-pad-00475420)
that sets the respawn point. The
visual prefix has concrete model binding; selector 19 adds terrain-light
payload and command state, which
[selector 54](#selector-54-rotating-terrain-light) carries over the rotating
visual. Selector 51 adds a fourth embedded node and
[route-group construction and finalization](ROUTE_RUNTIME.md). Selector 88
adds a concrete actor factory with retained H14 command custody and explicit
[later callback boundaries](ACTOR_RUNTIME.md#selector-88-construction-and-retained-commands).
Selectors 7/33 and 77 add the [central derived actor](ACTOR_RUNTIME.md#central-derived-actor-and-selector-77)
and its admission allocations. Selector 11 adds parameter commands and network
completion; [selector 60](SELECTOR60_RUNTIME.md) owns shared spawn rows and
their after-load allocation/destruction. Selector 86 owns retained mutable emitter data.
[Selector 87](#selector-87-map-annotations) owns numeric labels, radius/RGB
state and a separate ordered annotation list consumed by the briefing map.
[Selector 30](SELECTOR30_RUNTIME.md) owns its world-start override and completion
state; [selector 32](SELECTOR32_RUNTIME.md) is the cannon's shot,
[selector 26](SELECTOR26_RUNTIME.md) the grenade,
[selector 27](SELECTOR27_RUNTIME.md) the row-12 charge,
[selector 38](SELECTOR38_RUNTIME.md) the mine,
[selectors 25 and 28](SELECTOR25_RUNTIME.md) the missiles and
[selector 76](SELECTOR76_RUNTIME.md) the flamethrower;
[selectors 34/35](FAMILY428650_RUNTIME.md) own distinct retained ratio
inputs above the shared home-position/terrain-command family. Selector 34
also applies its native signed terrain-floor postinitializer.
[Selector 36 and the 004236D0 family](FAMILY4236D0_RUNTIME.md) own distinct
probe/postinit allocations, retained payload/table inputs and mode selection.
[CONSTRUCTOR_ASSETS.md](CONSTRUCTOR_ASSETS.md)
owns authored H13 fields; [INSTANCE_ASSETS.md](INSTANCE_ASSETS.md) owns the
ordered H14 command graph. [RUNTIME_FOUNDATION.md](RUNTIME_FOUNDATION.md#first-world-boundary)
owns the enclosing loader and phase boundary.

Allocation publishes objects before initialization, flag setters reorder lists,
and command failure destroys an already-published object. These are concrete
common-base effects; constructor asset views and terrain-adjusted inputs alone
do not supply them. The rotating visual prefix and
[shared actor constructor and handle registry](ACTOR_RUNTIME.md) also have
concrete owners, including actor-list custody, terrain classification,
shared payload initialization and ordered instance commands.
[Shared targets](TARGET_RUNTIME.md) retain actor handles or common-object
identities through distinct native constructor, clone and replacement policies;
they do not own referent destruction.
The [live model getter owner](OBJECT_MODEL_CALLBACKS.md) now reads captured
packet sources through actual class/extension state and the process clock;
native manager drawing, packet lighting and custom vertices stay separate.
Other derived actor classes still require their own construction, payload,
creation-listener and callback effects. Those dependencies remain explicit;
accepting unsupported classes as inert objects would bypass native effects.

[Constructor 93](ATTRACT_DEMO_RUNTIME.md#constructor-93-and-shared-xyz-copy)
adds its exact shared-actor extension, initialization and owned-target teardown.
Its generic resource argument remains distinct from the constructor ID. The
[direct demo factory](ATTRACT_DEMO_RUNTIME.md#direct-demo-factory) preserves
publication-before-resource lookup and retained required-source failures. The explicit
[movement request](ATTRACT_DEMO_RUNTIME.md#constructor-93-movement) owns complete
`00440170`; context-free `update` still requires its real world/process inputs.

## Authority and construction order

The authority is static disassembly of the unmodified `Infest.exe`, supported
by bulk/game_logic.c. Relevant ranges are
`004600D0..004603AE`, the dispatch at `004603B0`/`00461A04`,
`00479C60..0047A104`, and the actor constructor beginning `0047A7A0`.
The native receipts below exercise the named routines in isolation.
They do not constitute a complete native construction or manager-pass receipt.

`00460310` receives the complete H13 resource DWORD in ECX, the instance
suffix in EDX, and position, rotation, placement index, construction mode and
owner on the stack. Its ordered work is:

1. Resolve H13 using the signed low WORD. Negative values fail; a nonnegative
   index is accepted when it is at most the value at `0055ED98`. The slot
   table is `0055ED94`. A missing slot calls the handler at `0055ED90` with
   the original full DWORD, and a missing result fails.
2. Increment the resource's first WORD with WORD wrapping to obtain the
   constructor selector. Mode 2 checks `00462480(owner)` before allocation;
   a false result fails.
3. `004603B0` allocates the class, runs its constructor, and in some cases
   installs a final derived vtable. There are 94 accepted selectors. A failed
   allocation or unsupported selector returns null.
4. Call `00460120` on the resulting object with the resource argument,
   suffix and remaining construction inputs. Return the object only if this
   routine succeeds.

`00460120` resolves H13 again and zero-extends H13 WORD `+2` into a local
model argument. It rejects a null object or an object already carrying flag
`0x00100000`. It queries virtual `+18` for an actor interface before class
initialization. When that interface exists and mode is nonzero,
`004600D0` performs owner handling: mode 1 calls `004625D0`; other nonzero
modes call `0047E2B0`. The latter unregisters the previous object `+84` handle
through `00462210`, stores the supplied handle, and binds it to that object
through `00461FE0`. These operations do not use reference counts; the
[handle registry contract](ACTOR_RUNTIME.md#packed-handle-registration)
owns their free-chain and lookup-cache effects.

The initialization context contains four DWORDs: position pointer, rotation
pointer, pointer to the local model DWORD, and the **sign-extended low WORD**
of the placement index. Virtual `+0C` receives this context and H13 entry
`+4` as separate inputs. Its usual wrapper calls virtual `+08` for context
initialization before processing class payload data; the actual final vtable
selects both methods.

The common completion helper `004602D0` then calls `00479E10`, which:

- stores the resource argument's low WORD at object `+30`;
- invokes virtual `+38` as an admission guard;
- if admitted, walks the suffix's signed count in order when it is positive,
  calling virtual `+04(code, payload)` for every command;
- aborts at the first false command result.

A false guard or command result makes `004602D0` call the deleting destructor
at virtual `+00` with argument 1 and return null. Success invokes virtual
`+1C`, clears the entire dirty WORD at object `+38`, and returns the object.
This destructor path is distinct from `00460120`'s initial rejection of an
already-initialized object.

For a nonnull actor interface, `00460120` next visits creation listeners
rooted at `0054E868`. It inserts a temporary cursor with `00461BE0`, moves
that cursor past each selected node before invoking the callback, and unlinks
it on exit. Node payload `+18` supplies the callback interface;
`00461C00` calls that interface's first virtual method with the new actor.
Null node payloads are skipped. A simple immutable iteration cannot be
assumed equivalent to this mutation-aware ordering.

When network global `0055F448` is nonzero, virtual `+40` runs after listener
notification. Only after these callbacks does the routine set object flag
`0x00100000` and return true in AL. Earlier initialization callbacks therefore
observe that flag clear unless a class itself changed it.

## Common object and list custody

Base constructor `00479C60` initializes a `0x40`-byte base with vtable
`00527B24`. Each embedded list node is three DWORDs: next, previous and object
payload. Object `+0C`, `+18` and `+24` all point back to the object. Initial
fields are `+28 = 0`, `+2A = 0xFFFF`, `+2C = 0x08000003`, `+30 = 0`,
`+38 = 0`, and `+3C = 0`. The base constructor does not initialize the
position bytes at `+32..+37`, or padding `+3A..+3B`. The port's
`BaseObject::position_words: Option<[u16; 3]>` represents an unwritten position
as `None`, rather than exposing invented zero coordinates.
A context with no position preserves that state or the last supplied position.

With `G = [0055F160]`, the manager is `M = G + 8`. Constructor `00478A90`
allocates four 24-byte list headers at `M+0/+4/+8/+0C` and initializes
embedded lists at `M+10/+28/+40/+58`. Each list contains two three-DWORD
sentinels: the head's previous link and the tail's next link are null; their
payloads are null. In an empty list, the head's next points to the tail and
the tail's previous points to the head. Header `+10` is the tail's previous
link, not a movable iteration cursor.

Construction appends these nodes in this exact order:

| Object node | Tail-predecessor link | Destination |
| --- | --- | --- |
| `+10` | `[[G + 08] + 10]` | Current flag-1 list, header at `M+0` |
| `+04` | `[[G + 10] + 10]` | Current flag-0 list, header at `M+8` |
| `+1C` | `[G + 40]` | Embedded parked list at `M+28` |

Each insertion places the node immediately before the tail. The port retains
six physical lists for common-base custody: the two flag-1 headers, the two
flag-0 headers, the parked list and the disposal list at `M+10`. Current and
alternate roles refer to physical headers; they can swap without moving nodes.

Selector 87 adds node `+40` to the independent process annotation list at
`0055F170`; it does not join the ordinary actor list. Constructor prepend,
command-2 movement to the tail and destruction preserve that node separately
from all three common memberships.
The additional native lists at `M+40/+58` belong to later owners. A parked
list can contain the same object through its `+1C` node and either disabled
flag node. Membership is an ordered node identity, not a set of object IDs.

`00479E90(bool)` unlinks node `+04`, updates flag bit 0 and reinserts after
`[[G + 10] + 10]` when true or `[G + 40]` when false.
`00479F20(bool)` does the same for node `+10` and flag bit 1, using
`[[G + 08] + 10]` when true and `[G + 40]` when false. Both perform the
unlink/reinsert even if the boolean is unchanged; the native boolean tests
only the argument's low byte. A changed low flag sets bit 1 of dirty byte
`+38`, preserving its other bits and the high dirty byte.

Payload helper `00479D70` calls these setters from payload bits 0 and 1,
in that order, then writes `(old_flags & 0xFF000000) | payload_dword`.
The raw payload is not masked to 24 bits. It can add high bits as well as
replace the lower flags. This final assignment does not mark dirty bits for
changes to payload bits 2 through 8. List movement, the setters' dirty effect
and the final raw assignment are distinct effects.

Base context initializer `00479D40` writes context `+0C`'s low WORD to
object `+2A`. A nonnull position pointer copies its three WORDs to
`+32/+34/+36`; a null pointer leaves them alone. It does not set dirty bits
or consume context rotation/model inputs. Base command handler
`00479E70` accepts only codes 0 and 1. The base vtable's actor query
`00401300` returns null, guard `00401370` returns true, and post-initializer
`00401310` returns without work. Its network slot `+40` is `0047E320`, which
clears the dirty WORD; executing it does not supply actor/network behavior.

Destructor `00479D00` resets the base vtable and unlinks nodes `+04`, `+10`
and `+1C`, in that order, without clearing flags, dirty state or detached
node links. Deleting wrapper `00479CE0` additionally frees the allocation
when argument bit 0 is set. Disposal `00479FA0` instead calls
`00479F20(false)` before `00479E90(false)`, clears flags with `0xFFFFFE9B`,
marks dirty byte bit 1 when that mask changes any low nine flag bits, then
appends node `+1C` to the disposal list through `[G + 28]`. This setter order
is opposite to payload loading. Repeated disposal still reorders the nodes.

Later passes have distinct transfer policies. `00478F20` repeatedly moves
the current flag-1 head to the **front** of the alternate list before its
virtual `+24/+3C` callback, then swaps the two headers. Ordinary traversal
therefore reverses order every frame: Infest01 visits its turrets in exactly
opposite orders on consecutive frames
(ledger). The port's
`visit_flag1_update_callbacks` inserts at the front the same way, so shared
RNG draws reach actors in the native order. `00479040` instead appends to the alternate list
(the [player block](GAMEPLAY_RUNTIME.md#player-block-of-00478f20) uses it for the vehicle);
`00479080` appends to the current one. Lighting `00479500` and drawing
`00479570` append flag-0 nodes to their alternate list before callbacks and
swap headers only after normal exhaustion, preserving ordinary order. The
[shared light pass](OBJECT_LIGHTING.md) now consumes the actual arena's fresh
primary head and admitted `+28` callbacks. Role custody alone does not implement
the separate `00479570` draw pass.

World cleanup `00478CD0` repeatedly restarts at each head: embedded `M+40`,
the four heap lists, disposal `M+10`, then parked `M+28`. It calls virtual
`+30`, rereads the selected node's object pointer, then invokes the deleting
destructor. Disposal drain `00479840` visits only `M+10` and retains the
object pointer across virtual `+30`. Full manager destruction `00478BB0`
also drains `M+58`, using that list's virtual `+08` deletion rather than
`+00`, then frees the four heap headers. These later callback-driven drains
are not implemented by simply clearing the arena.

## Per-frame disposal

The [shared drain](../../crates/inf-game/src/object_runtime/disposal.rs) owns
`00479840`. It reads the current disposal head at `M+10`, captures node `+8`'s
object across virtual `+30`, then reads that object's current vtable for deleting
virtual `+00(1)`. It reloads the live head only after the destructor returns.
A callback can move the captured object, reorder the queue or enqueue another
object; none substitutes the new head for the captured destructor receiver.
The tail sentinel's NULL next pointer ends the pass. The destructor owns
successful unlinking; the drain does not remove a node before either call.
This differs from [world cleanup](WORLD_CLEANUP.md), which rereads the selected
node's object after `+30`.

Native `00478F4B` calls this drain before player/actor callbacks. The
[live-world adapter](../../crates/inf-game/src/session_platform/intro/runtime/disposal.rs)
runs at that boundary, borrowing the retained mission, grants, live process
state, residency and sound teardown services. Offline deaths queued
later in the actor pass remain live until the next frame's drain. Unlike terminal
cleanup, this pass does not set mission inhibit byte `+08`: actor `+30` observes
its actual value when recording placement death. Native's
additional late drain is network-gated by `0055F448` and `0045C940`; it does
not justify a second unconditional offline drain. Placement finishing reuses
the same traversal with its existing lazy actor-service boundary.

A Dormant Selector35's destructor exit (`0043E120`) needs the world's burst
and radius, so the drain pauses after that ship's `+30` (`exit_pending`); the
live-world adapter runs the exit over the tick's owners and drains again,
and the ship's destructor follows without a second `+30`. If the exit fails,
the resumed destructor keeps its typed boundary and the ship stops there;
the drain pauses once per ship. The load-time drains, where no ship has a
descriptor yet, refuse a pause as that boundary.

`ReturnFirstError` preserves the stopped prefix and returns the error. The
live world uses the explicitly non-native `RetainStoppedObjectApproximation`:
retain the failed object's allocation, fields and remaining memberships,
record the failing callback/destructor phase, and continue healthy queue work.
The diagnostic ledger is per world; the no-retry marker belongs to the live
allocation as well. A fresh frame/load ledger, terminal cleanup or direct
destruction cannot replay a partial release. Generation-bearing ObjectIds
prevent a reused slot inheriting an old stop; a destructor that already released
its allocation before failure cannot mark its replacement. The missing lower
callback/destructor owner must be completed to remove this approximation; it
does not count as retail acceptance.

Disposal's native flag changes already exclude retained objects from the
ordinary flag-gated simulation and selection paths. Three ungated consumers
also enforce this portable containment without deleting their list entries:

- Selector93 preserves the full actor FIFO, count, RNG draw and selected index,
  then returns typed `StoppedDisposal` before allocating a follow target.
- Zero-radius grant broadcast skips stopped receivers while preserving FIFO order.
- Creation listeners advance their cursor and preserve bindings, but skip
  callbacks to stopped owners in the current arena.

The [21 native receipts](../../crates/inf-game/tests/fixtures/object_disposal_drain_native.json)
execute original `00479840` with original vtables at FPCW `0x027F`. Native base
construction/disposal establish initial membership; callback mutations and
destructor unlink effects are explicitly supplied. Empty/multiple queues,
head changes, new queued nodes, fresh vtable reads and stopped prefixes are
covered by the [receipt consumers](../../crates/inf-game/src/object_runtime/disposal/native_receipts.rs).
They authenticate traversal, not lower cleanup bodies, per-frame timing,
reentrant drains or the portable no-retry policy. Separate
[containment tests](../../crates/inf-game/src/object_runtime/disposal/tests.rs)
cover that policy and allocation reuse. Complete native-body guards bind the
captured-object/reload ordering; full manager destruction remains separate.

## Derived state and retained inputs

Visual base constructor `0047A030` calls the common constructor, initializes
`+40 = 0` and `+42 = 0xFFFF`, and installs `00527B68`. Initial model resolution
through `0047A050`, including signed resource indices, disabled models and
model metadata, is described in
[the model-binding contract](CONSTRUCTOR_ASSETS.md#initial-h8-model-binding).
It performs state changes; the parsed binding is only its input.

`0047A050` first applies the base context and resets object WORD `+28` to
`0x100`. A null model pointer preserves both model WORD `+42` and radius
WORD `+40`. With a pointer, it stores the model WORD: `-1` clears flag `0x4`
but preserves the radius; other negative indices, indices above the native
limit, and null H8 results preserve it too. A resolved model copies header
WORD `+0E` to the radius, clears flag `0x4` if that WORD is zero, and copies
header WORD `+0` to `+28`, substituting `0x100` when zero. Nothing enables
flag `0x4`. H8's actual miss method is `004EFBF0` in vtable `005276D8`; it
returns null without loading a model.

Refresh `0047A110` is deliberately different: model `-1` resets both extent
and radius to `0x100` and zero; any unresolved other model preserves both.
Every exit checks the resulting radius and clears flag `0x4` when zero.
Model command 2 in `0047A1A0` first ORs dirty WORD `+38` with `0x100`, stores
the model WORD and refreshes it. Codes 0 and 1 are inherited payload-free
acceptances. The port retains these distinct initial-binding and replacement
policies rather than treating them as one generic model setter.

### Rotating visual prefix

`0047A490` calls visual construction `0047A030`, installs vtable `00527BB4`,
then copies the shared identity basis at `00547590` to `+44..+6B`.
The copy includes the rotation counter. Startup `00439630` initializes that
shared basis; the accepted recording's identity writer query also establishes
its startup-only custody. The port's `RotatingVisualBase` is an intermediate
class and does not admit any additional H13 selector.

Context initializer `0047A610` runs visual initialization first. A nonnull
rotation pointer supplies three signed WORD angles, consumed in order index
1 about Xz, index 0 about Yz, then index 2 about Xy. Each angle uses 65536
units per turn. A null pointer preserves orientation and counter; a nonnull
zero vector still makes three rotation calls. The existing basis accumulates
these rotations rather than resetting to identity. The implementation reuses
the exact native sine table, counter and normalization policy in
[`NativeBasisMath`](../../crates/inf-formats/src/native_basis.rs).

Wrappers `0047A720/730/740` forward full DWORD angles to Yz/Xz/Xy rotation.
They leave common flags and dirty bits unchanged. `0047A6C0` is a separate
signed-WORD vector transform through `004909E0`, not an angle setter. It
accumulates `(X product + Y product) + Z product`, rounds to a signed QWORD
with nearest-even x87 conversion and returns each low WORD. It supports an
in-place source/destination and does not add the object position. The existing
DWORD point transform has a different summation order and is not substituted.

### Selector 19 terrain light

Dispatcher `004607E3..00460810` allocates `0x4C` bytes, calls `0047A030`,
then installs the final vtable `00526978`. Constructor `0047A030` leaves
the light-specific extension untouched. Initializer `00448200` invokes
visual context `0047A050` before payload setter `004481C0`: common flags
are applied first, followed by payload bytes `+4/+5/+6` to object
`+44/+45/+46`, byte `+7` to `+4A`, and WORD `+8` to `+48`. The flags can
overwrite an earlier visual flag clear. Padding `+47/+4B` is not initialized
or interpreted as an alpha channel. No source pointer is retained.

Command handler `00448120` forwards codes 0–2 to the visual handler. Codes
3–5 replace the three colour bytes, code 6 replaces draw gate `+4A`, and
code 7 replaces unsigned radius `+48`; these scalar commands do not mark
dirty bits. At lighting consumer `0045B1A0`, the colour bytes are signed
additions. They are therefore represented as `[i8; 3]`, separately from the
unsigned gate and radius. Canonical payload and command spans are owned by
[constructor assets](CONSTRUCTOR_ASSETS.md) and [instance assets](INSTANCE_ASSETS.md).

This class is a terrain light with an optional model marker. Its initial
H8 name `powerup` is not evidence of collectible behavior. The sole authored
H13 resource is slot 53, with flags `0xB`, zero colours and gate, radius
1024, and initial H8 slot 9. All 42 first-world instances retain gate zero,
but their colours/radii are changed by authored commands. Across 33 world
assets there are 1,125 placements, including model and gate commands not
used in the first world; this census is asset coverage, not execution.

The final vtable's `+28` is `004481A0`, which accumulates terrain lighting
through `0045B1A0`; it depends on dynamic-lighting settings, current terrain
spacing/mask and moving origins, optional global colour scaling, and the
mutable 90×90 signed RGB grid. Its `+2C` is `00461C30`, which invokes world
model draw `0047AFC0` only for a nonzero gate. Neither callback runs during
construction. The [shared light pass](OBJECT_LIGHTING.md#current-callback-ownership)
now executes `+28` over actual retained storage and the process grid from both
briefing and gameplay; missing light/position storage remains explicit. The
separate `+2C` callback and enclosing draw pass remain required owners.

For the exact base, visual-base, rotating-visual, terrain-light and route-node
classes, virtual `+20` is `00401320`: a proven no-op `+34` call followed by flag `0x00800000`,
without changing dirty bits. Virtual `+24/+3C` is `00405140`, which calls
the flag-1 setter with false; it still moves the node when already disabled.
The production flag1 manager dispatches those exact authenticated classes,
including map annotations, through the same setter. All 42 loaded mission22
terrain lights consequently leave the update list while retaining their
independent lighting/draw memberships. Already-disabled callbacks still append
their `+10` node to parking in native order. Only the authenticated selector86
and timed-pickup alternate `+3C` entries share this forwarding admission; their
ordinary updates remain distinct. The tracked
generator and
[28 native cases](../../crates/inf-game/tests/fixtures/simple_object_updates_native.json)
cover selected vtable identities, flags/dirtiness and 84 ordered list snapshots.

Actor constructor `0047A7A0` calls rotating visual base `0047A490`, installs
additional actor fields, and publishes another intrusive node at object `+A8`
through `[G + 58]`. It calls `00461EA0` with owner byte `0055F2F4`, the actor
pointer and zero, stores the resulting handle at `+84`, and copies global
`0054E894` to `+88`. It also consults H12 and terrain globals within the
constructor, before `00460120` supplies its later initialization context.
The [shared actor owner](ACTOR_RUNTIME.md) integrates registration, ordered
actor-list publication, terrain classification and destruction. Its allocation
position is explicit: the recorded constructor reads nonzero allocation bytes
before the placement context. Neither zero nor the later authored position
is substituted. The shared actor's `+20/+34` permits the same after-load flag
operation; its distinct `+24/+3C` callbacks remain an explicit update boundary.

Retained payload pointers also need lifetime and mutation ownership. For
example, selector 86 uses constructor `0042DE50`, context `00479D40`, and
payload initializer `0042DE80`. The latter reaches `0042DEC0`, which stores
its data pointer at object `+40` and **writes WORD data `+6`**. A positive
signed WORD at data `+2` produces `60000 / value`; otherwise the written
value is 30000. Instance command 2 reuses this setter with its own payload.
An implementation must preserve the applicable shared/aliased resource
semantics without modifying original OVL evidence on disk.

### Selector 54 rotating terrain light

Case `0x36` of `004603B0` allocates `0x74` bytes, calls the rotating visual
base `0047A490` and installs final vtable `005265FC`: selector 19's light
over the rotating visual, its fields moved from `+44` to `+6C`. Initializer
`00464A20` runs virtual `+08` (`0047A610`, the rotating context with the
placement's rotation) and then `004649E0`, which is `004481C0` at the new
offsets: common flags, payload bytes `+4..+6` to the signed colours
`+6C..+6E`, byte `+7` to the gate `+72` and WORD `+8` to the radius `+70`.
Commands `00464940` are `00448120`, the light `+28` (`004649C0`) is
`004481A0` reading DWORD `+6C` and WORD `+70`, and the draw `+2C`
(`00461C60`) is `00461C30`'s gate before `0047AFC0`. Every other slot is the
rotating base's (`00527BB4`). Its `+48` (`004026B0`) answers the light's own
basis at `+44` where selector 19's `00401390` answers the shared one, so its
marker draws in the placement's orientation. Its `+20` is `00401320`, its
`+24` and `+3C` are `00405140` and its `+1C` is a `RET`, as on selector 19.

The class's one H13 resource, slot 35, is placed 133 times in leaves 13, 20,
30, 38 and 39, and every placement sets its model and gate by command.
Infest04 builds Oolbryte's 34 (resource 35, model 572, colours (2, 10, 15),
radius 1500, gate 1, flags `0x08100001`, zero rotations, so the basis is the
identity after three counted turns) and draws them 1,449 times. It never
calls their light, nor selector 19's: the light pass `00479500` finds its
settings gate (`+80`) closed at every call sampled.

The [port](../../crates/inf-game/src/object_runtime/construction.rs) builds
the class (`RotatingTerrainLight`) through the rotating context and selector
19's payload setter and commands, lights it through selector 19's arm, draws
it behind its gate with its own basis, and admits it to the flag-1
forwarding. Built from the same placements, all 34 match the captured
objects field for field
(`terrain-light-54.js`).

## Offline selection 22 construction inputs

Offline startup selects campaign value **22**, so `0048DC30` requests OVL
leaf **32** after leaf 48. This is distinct from phase 3 and from leaf 11.
The following inventory was obtained through the canonical resident CLI:

```powershell
cargo run --release -p inf-game -- --data-root retail --mode resident --activate 47,48,32 --inspect-world 0 --json
```

At difficulty/language 0, H14/0 in `32.ovl` supplies 118 placements and 13
constructor selectors. Names below are initial H8 asset names, not proof of
complete class behavior. Command columns are the authored codes for that
selector in this leaf; they do not establish that the commands executed.

| Selector | Count | Constructor | Initial model names | Authored command codes |
| --- | ---: | --- | --- | --- |
| 10 | 2 | `00425190` | quixle | 18 |
| 11 | 8 | `004014F0` | gunbase | 1, 2, 6 |
| 12 | 6 | `00467070` | scientst | 6, 18 |
| 19 | 42 | `0047A030` | powerup | 1, 3, 4, 5, 7 |
| 30 | 3 | `0048B170` | gate | 6, 21, 22, 23, 26, 30, 32 |
| 34 | 2 | `00433B20` | pership | 6, 21 |
| 35 | 3 | `0043D070` | commship, demont3 | 18, 21 |
| 36 | 3 | `00481110` | tankbot | 18 |
| 51 | 40 | `00404B20` | flag01 | 3, 5, 6, 7, 8 |
| 60 | 1 | `00480800` | tomspawn | 18, 19 |
| 77 | 1 | `00407410` | braineye | 6 |
| 86 | 6 | `0042DE50` | no shared initial binding | 1, 2 |
| 88 | 1 | `00443890` | nothing | 16 |

Placement 0 uses selector 19; selector 30 already appears at placement 3.
Selectors 19 and 51 account for 82 placements but have different command
owners. A successful small-class implementation therefore does not complete
this mission's constructor loop.

The loader calls `00460310` with mode 0 and owner `0054E894`. On a successful
return it queries virtual `+18` again: an actor receives `0047F6C0` when
`0055F448` is clear. The attract branch controlled by `0055F14C` performs
additional actor-interface and flag work. These caller-side effects follow
per-object completion and are not implied by implementing `00460120`.
`ObjectArena::register_loaded_actor` owns the offline `0047F6C0` step and
`finish_world_placements` the post-placement tail; see the
[actor loader order](ACTOR_RUNTIME.md). The attract branch is not ported.

## Bounded implementation and evidence boundary

The arena owns concrete objects under stable IDs and preserves each
embedded node separately. The mode-0 construction consumer resolves the
current resident H13 registration, publishes the exact selector-1/2/6/7/10/11/12/16/19/30/33/34/35/36/51/60/77/86/87/88/90
class, applies its context and payload, and stores the resource's low WORD. Commands
0 and 1 are accepted without reading their payloads. The first unsupported code
destroys the already-published object and returns its rejection index/code.
Decoding occurs only when a command is reached: missing or truncated later
commands cannot preempt an earlier rejection. Any host error after allocation
also attempts actual destruction, preserving earlier effects on other objects and
owners. Selector-60 dangling shared row pointers can stop that cleanup at an
explicit lifetime boundary. Successful completion invokes the actual postinitializer and clears
the dirty WORD. Nonactors then execute `+40` dirty clear when requested by
network state and publish flag `0x00100000`.

The [constructor6 shared factory](SHARED_ACTOR_FACTORY.md) allocates the
actual `0047A7A0` class without the derived mask/RNG phase and retains the
current H14 command owner through shared completion.
Selectors 1/19/51/86/87 have null actor queries, true guards and no-op
postinitializers. Selector 88 instead owns actor registration, shared actor
context/payload/postinit, the mutable creation-listener walk and its exact
network `+40=0047E2E0` before publishing the initialized flag. Its constructor
requires explicit allocation position, terrain and owner inputs. Listener
bindings require a supplied dispatcher; selector 18's body is the
[Force Field's](SELECTOR18_RUNTIME.md#marks-and-switches) mark, and
selector 78's remains unimplemented. The recorded first load has 30 empty actor-listener
walks, as scoped in [ACTOR_RUNTIME.md](ACTOR_RUNTIME.md#creation-listener-owner-and-callback-boundary).
No generic success callbacks stand in for other classes.
Selectors 7/11/33/60/77 run `00426C80` before suffix reads, including a null suffix,
and own its behavior arrays. Selectors 11/77 perform their actual postinitializers;
selector 11 also adds dirty bit `0200` after shared network completion. All
selector-35 commands follow its `004270E0` guard, which additionally copies
home XYZ even for an empty suffix. Selector 30 requires mutable world-start
state before publication and runs its own `0048C4F0` postinitializer. All
admitted actors share the proven dirty-clear/listener/network completion order.
Negative/out-of-range H13 indices, nonresident resources, inconsistent
registrations and unsupported selectors are explicit errors before allocation.
The port rejects an index equal to the allocated slot count. Native startup
`004EDFB2..004EE021` reads the raw PreLoad WORD, allocates that count times
four bytes, and stores the unchanged count at handler `+08`. H13's later
initialization hook is the no-op `00466D40`; registration `004480B0` through
`004EFC00` changes slot contents, not the count. Native lookup at
`00460324..00460337` nevertheless admits equality and reads one pointer past
that allocation. The port reports this boundary as out of range rather than
reading unrelated memory. H8 model lookup has the same count-sized allocation
and inclusive native comparison; its exact-equality boundary is also an
explicit error. An in-range null H8 slot instead preserves the native null
miss result. H13 lazy loading remains unsupported.
Lookup resolves current residency before allocation. Selectors 1/19/51/87
retain no H13/H14 pointers. Selector 86 retains its shared mutable data image.
The actor profiles retain their canonical H13 payload identity. Selector 88
also retains its H14 command-allocation owner and operand offset when code
16 is reached. A byte-equal replacement is not the same allocation.

The checked-in [common-base receipts](../../crates/inf-game/tests/fixtures/object_base_native.json)
contain 68 cases, 361 snapshots and 323 entries into unmodified native
constructor, context, payload, setter, disposal and destructor routines under
Unicorn. Native-shaped sentinel lists and preallocated poisoned storage expose
untouched position/padding bytes. Cases cover interleaved objects, all payload
bits, low-byte boolean handling, unchanged-value reordering, high-flag and dirty
preservation, repeated disposal, null context positions and destruction.
Synthetic initial-state writes are identified separately from native calls.
Four owner-swap cases explicitly exchange list-header pointers between native
routine calls; those synthetic exchanges do not execute the manager passes.
There are no patched routines or substituted allocation/callback methods.

The separate [command-admission receipts](../../crates/inf-game/tests/fixtures/object_base_command_native.json)
execute 45 native `00479E10` calls after 45 native base constructions, using
the actual `00527B24` vtable, true guard `00401370` and command handler
`00479E70`: 19 accept and 26 reject. In 22 rejection cases, an unmapped command
pointer immediately follows the unsupported code, proving the walk stops
there. Three nonpositive counts occupy the last mapped DWORD, proving no
pointer-array read. Each call changes only the resource WORD among the 64
object bytes and preserves all list identities and ordering.

These receipts do not execute gameplay/list allocation, deleting free,
constructor dispatch, or `00460120/004602D0` completion. The later completion
ordering has static authority and port-side checks; the `00479E10` receipt
does not cover post-initialization, dirty reset or initialized-flag publication.
The [visual receipts](../../crates/inf-game/tests/fixtures/object_visual_native.json)
contain 78 cases and 153 direct entries into `0047A030`, `0047A050`,
`0047A110` and `0047A1A0`, including the real null H8 miss callback. Field
policy tests cover null pointers, signed indices, zero/nonzero metadata,
retained fields and dirty-bit preservation. The synthetic native table
deliberately maps an extra readable slot at its inclusive limit; those cases
test field policy, while separate resident tests require the port's explicit
one-past error. No allocator, model-program execution or callback replacement
is claimed by these receipts.

The [terrain-light receipts](../../crates/inf-game/tests/fixtures/object_light_native.json)
contain 72 synthetic cases and the 42 canonical first-world placements, with
342 direct entries into unmodified `0047A030`, `00448200` and `00479E10`.
Tests compare initialization, command admission, all represented fields and
embedded-node membership against 71 synthetic cases; the remaining case pins
the deliberate error at H8's native inclusive boundary. Synthetic model
records go through the real installation, parser and resident registration.
The 42 authored cases also compare the full portable factory using current
H13/H14/H8 registrations. Native receipts stop before `004602D0`; for that
factory comparison, only the separately proven dirty clear and initialized
flag are added to the expected state. These receipts do not execute allocation,
the full dispatcher/completion chain, terrain-light accumulation or world draw.

The rotating visual receipt
[object_rotating_native.json](../../crates/inf-game/tests/fixtures/object_rotating_native.json)
contains 75 cases, 219 snapshots and 230 direct entries into the unmodified
executable. It executes actual identity startup, rotating construction and
context, all three angle wrappers, 14 signed-WORD vector transforms, and
reached normalization helpers with the original sine table and x87 control
word `027F`. The [comparison test](../../crates/inf-game/src/object_runtime/rotating/native_tests.rs)
checks common fields, all ten basis words, ordered list membership and
transform outputs. Synthetic prior bases and dirty words are explicit; no
code, callback or allocator is substituted. Actor construction, allocation,
world drawing and full factory execution remain outside this receipt.
Its SHA-256 is `9A5EB44AFFE6E2A3CCB430B26965789A64C49EB0DFB50E554968C5594B14AD28`.

Other derived actor constructors and concrete listener callbacks, nonzero
factory modes, remaining retained-data consumers, complete manager drawing and
[active vehicle light callbacks](OBJECT_LIGHTING.md#current-callback-ownership)
remain separate owners. Selector 1 is absent from the
first-world inventory; selectors 10, 11, 12, 19, 30, 34, 35, 36, 51, 60, 77, 86 and 88 account for all 118 of its
placements. [Selector 60](SELECTOR60_RUNTIME.md) owns its after-load row allocations
and their shared pointer lifetime. Route finalization
has its own [lifecycle and evidence boundary](ROUTE_RUNTIME.md). This partial class
coverage does not admit the mission's complete placement loop.
The loader's `+20` passes and selector 88's player attachment now run over
this world; the loader's own later player spawn/binding and owner setup
must still exist before phase 3 can be admitted.

## Selector 86 retained emitter data

Selector 86 is a nonvisual common object, with constructor `0042DE50` and
final vtable `005218CC`. After common publication it initializes pointer
`+40` to null, elapsed DWORD `+44` to zero and distance-squared DWORD `+48`
to 1,000,000. Its initializer `0042DE80` invokes base context `00479D40`,
then `0042DEA0` applies common flags and passes payload `+4` to `0042DEC0`.
The H13 header's model argument is unused.

`0042DEC0` retains the supplied data address at object `+40`, reads signed
WORD data `+2`, and writes WORD data `+6`. A positive rate produces
`60000 / rate`; zero and negative rates produce 30000. Rate 1 therefore
writes raw WORD 60000, which the later update reads as negative. The port
preserves that result without adding an invented rate clamp. Command handler
`0042DEF0` accepts codes 0 and 1 without reading operands; code 2 invokes
the same retained-data setter. Every other signed command rejects.

The setter changes a shared runtime allocation image, including overlapping
retained ranges. H13 images are keyed by source-allocation identity and
handler region, and remain owned while that source exists even if every
object has been destroyed. Reloading the source creates a different image.
H14 commands share `InstanceCommandAllocation`; original source bytes remain
available for custody checks and never change. Objects retain the runtime
image and operand offset, rather than a copied per-object interval.

The owned command walker rereads the signed suffix count after each accepted
command, then reads the next command pointer and current command bytes.
Earlier writes can therefore change a later code or terminate the walk by
shrinking its count. Rejection destroys the object but does not undo an
earlier allocation write. Missing later pointers cannot preempt a reached
rejection or a shortened count. The unowned command path remains an explicitly
immutable caller-supplied suffix and cannot execute a retaining code 2.

Address-dependent writes into relocated pointer fields are an explicit host
boundary: native writes would depend on the allocation's original absolute
address. Writes into other H13 class records are also rejected while their
owners retain immutable payload views. The guard includes actual actor and
derived-payload read spans, even when a selector-86 prefix is nested within
one. These guarded intervals do not define binary record sizes.
H13 dispatch headers are protected for every class, including selector 86;
changing a class header could otherwise bypass the retained-payload boundary.
These checks precede the write. Ordinary
selector-86 aliases, overlapping data ranges and all six first-world emitter
placements execute through the factory with unchanged original OVL bytes.

The class has a null actor query, true admission guard and no-op postinit.
Its common after-load path calls no-op `+34` then sets `0x00800000`.
Network `+40` clears dirtiness. Draw callback `+2C=0042DF20` only takes the
signed scene DWORD `+68`, divides by two toward zero and stores the wrapping
DWORD square at object `+48`; it draws no geometry. Update `+24=0042DF40`
now runs through the shared flag1 manager with current process RNG and cached
camera position, independently from its exact alternate `+3C=00405140`.
Destructor `00475400` forwards through `00404C20` to common `00479D00`, then
optionally frees the object.

The update compares unsigned wrapping distance squared against `+48`, unless
flag `02000000` bypasses that test. A far object resets `+44` to the signed
period. Near objects add unsigned `delta >> 10`; the initial signed comparison
is strictly greater, while subsequent emissions continue at equality. Radius
and table-selected particle type are captured once; current object XYZ and
shared period are reread at each accepted rejection-sampling point. The two
RNG calls precede the signed circle test. The timer subtraction precedes lazy
H12 height acquisition, then H14 wave flags and the inline zero-clock water
surface. The final Y is a signed WORD maximum. Actual `00489D90` receives
velocity `[0,100,0]`, flag `0800`, current process source and the same RNG.
All six mission22 emitters select trail11; its birth, update, draw and retirement
are the existing shared [particle owners](PARTICLE_RUNTIME.md).

A per-allocation portable stopped marker retains reached fields, births and
RNG on a failed service and prevents subsequent replay. It is not a native
field or success substitute. Malformed zero-radius IDIV stops after its first
RNG call. Named nonprogress guards contain zero periods after the completed
iteration and negative radii -10922 through -1 after their first rejected pair;
in the latter range neither components nor summed squares can wrap, so every
sample lies outside the circle. Other signed values preserve native arithmetic.

check_selector86_update.py regenerates
[30 native update cases](../../crates/inf-game/tests/fixtures/selector86_update_native.json):
actual distance/RNG/inline water instructions, with explicitly supplied H12/H14
pointers, terrain height returns and trail-service returns. Cases cover wrapped
coordinates/deltas, signed periods, negative-radius overflow, dry/wavy surfaces,
mutated callback returns and stopped prefixes. This authenticates the caller,
not those supplied services or full-scene cadence. Production tests separately
use loaded data, actual terrain, process source/RNG, real trail allocation and
update/retirement, plus allocation-prefix containment.

The [native receipt](../../crates/inf-game/tests/fixtures/selector86_native.json)
contains nine cases and 98 snapshots: 11 constructors, 11 initializations,
40 direct commands, seven draw-context calls, eight after-load calls and
11 destructor wrappers with free disabled. It uses unmodified executable
instructions and real vtables, with explicit synthetic allocations and no
hooks or callback substitutions. The comparison covers negative/zero/positive
rates, pointer replacement, shared and overlapping allocation bytes, all
represented object fields and ordered memberships. Its SHA-256 is
`39080A5219840B69C34EBBDBAAFF7DC57BF2A91CA41E7AF4FA1D9CBB5FB29F70`.
The fixture does not execute the spawning update, heap allocation/free or
the complete factory. Separate host tests cover canonical first-world factory
inputs, failure cleanup, mutable walk ordering, alias lifetime and guarded
cross-class or relocated-pointer writes.

## Selector 87 map annotations

Factory `004603B0` case `0x57` allocates `0x58` bytes and calls `0048C710`,
with final vtable `00528AB0`. The constructor first publishes the common
nodes, then prepends annotation node `+40` before the current first node.
Node payload `+48` points to the object; label DWORD `+4C` starts at zero,
unsigned radius WORD `+50` at 10000, and unsigned RGB bytes `+52..+54` at
zero. Context `00479D40` supplies the common position WORDs; payload
`00479DB0` applies common flags. No model or actor inputs are consumed.

`0048C6C0` initializes one list with head `0055F170` and tail `0055F17C`.
`0055F180` is the tail's previous link, not a second list. `0048C6F0` walks
forward from head.next and excludes the tail. The arena exposes this order
through `map_annotation_list()` and field state through `map_annotation()`;
the draw owner does not reconstruct it from common lists or placement order.

Command `0048C7A0` preserves these reads and effects:

| Code | Reached operand and effect |
|---|---|
| 0, 1 | Accept without reading an operand through `00479E70` |
| 2 | Read unsigned WORD, zero-extend to label DWORD, unlink the node and append before the tail; zero and unchanged values still move it |
| 3, 4, 5 | Read one unsigned byte into the respective RGB channel |
| 6 | Read unsigned radius WORD |
| Negative or other | Reject without reading an operand |

The canonical command decoder bounds only a reached field. A missing operand
cannot preempt an earlier rejection; factory failure destroys the already
published annotation. `0048C780` restores the class vtable, unlinks its fourth
node, then calls common destruction `00479D00`. Thus partial command writes
do not leave stale annotation-list membership after cleanup.

Tracked native checker executes the original
constructor, commands, destruction and sentinel initialization without
instruction patches or callback substitutions. Its compact
[25 state/list snapshots](../../crates/inf-game/tests/fixtures/map_annotations_native.json)
pin prepend order, repeated tail movement, unsigned field widths, rejection
and removal using supplied allocations and native-shaped common lists.
Host tests additionally construct actual mission selections 18/16 (OVL leaves
28/26) and cover truncated operands and cleanup. These bounds do not establish
heap allocation/free, complete factory execution or a retail scene comparison.
The independent [briefing annotation tail](BRIEFING_RUNTIME.md#independent-map-annotations)
owns projection, resource lookup and pixels.
