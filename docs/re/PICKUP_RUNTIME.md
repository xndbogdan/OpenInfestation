# Pickup construction, collection and presentation

This authority owns the nonactor visual lifecycle of the placed selector14 and
timed selector89 pickups and their shared `0046D870` birth / `0046DAF0` draw
consumers. The
[family dying descriptors](ACTOR_DESCRIPTOR_CHANGES.md#family-staged-dying-descriptors)
produce kind83/value500 through these owners; they do not substitute an inert
model for a collectible object. Network respawn updates keep explicit
boundaries.

## Construction and lifetime

[pickup.rs](../../crates/inf-game/src/object_runtime/pickup.rs) retains the
actual `0046E060` timer (10,000,000), value46=1, save index48=FFFFFFFF,
clock50=0 and byte54=0. Shared `0046D840` leaves kind44 unwritten; its visual
starts modelFFFF and radius0. Constructor selector89, command3/4 kind/value
writes, registration `0046E0C0`, model refresh and virtual14 are represented
through the normal object owners. It is not an ActorHandle allocation.

Birth `0046D870` captures current process NULL before construction, selecting
H13 resource10 for low-WORD kind83 and resource9 otherwise. The production
factory admits the actual selector89 resource10 route. Kind publication and
model/dirty writes precede refresh; value follows normal refresh. Ground
comparison uses the original mutable caller XYZ, including fresh Y after the
first height callback. If below ground, the second query uses the newly
constructed object's current X/Z. The final Y increment is64 except for kinds
7/E/F/10. Failure retains the reached child and native write prefix.

Kind83 selects model168 through `0046D500`; other authored kind mappings are
also intrinsic and never selected by scene. Unknown kinds publish kind44 then
return the current network flag. Kinds4/1A raise Y by100 before model refresh.
Model callback14 uses process animation time for selector0, kind44 for selector1,
and the low requested selector otherwise (`0046E050`).

Update `0046E0D0` subtracts the full delta with DWORD wrapping and queues
`00479FA0` only when the result is signed negative. Exactly zero survives.
[The ordinary frame visitor](../../crates/inf-game/src/session_platform/intro/runtime.rs)
passes the actual delta; the next manager disposal pass owns release. Normal
known-kind destruction follows the common visual owner. Shared destructor
`0046E0A0` adds one to gameplay G+5B4 for each kind7 pickup it destroys,
placed or timed, before `00479D00`. `00487540` zeroes G+5B4 at the phase-8
transition before `0048E890` retires the world. The results draw `0042B510`
therefore counts the kind7 pickups left in the world. A missing kind stays an
explicit teardown boundary.

### Placed selector14

World placements construct selector14 through `0046D840`, final `00527208`.
That table shares 17 of its 19 slots with selector89's `00527254`, including
the `0046D780` commands, draw and destructor. Unlike selector89, the
constructor leaves the +58 timer unwritten. The two tables differ at +1C and
+24:

- +1C `0046D7D0` runs after the placement's commands. Placement word -1 does
  nothing. Any other placement copies world pickup serial `0054ED48` into +48
  and increments the serial. The world loader `0048DC30` zeroes that serial
  before the placement loop. If `0048E930` finds the serial's bit in the
  current world's W+B0 words, the pickup was already collected and disposes
  through `00479FA0`. Offline, `00479F20(0)` keeps it off the update list, so
  it collects only through the draw path below. The draw's accepted grant
  sets the same bit through `0048E980`.
- +24 `0046D950` is a network respawn update and is unreachable offline. The
  network `00479F20(1)` branch and any update call on a placed pickup stop as
  `PlacedPickupNetworkUpdate`.

The world loader passes the mission's W+B0 words to
[world_load.rs](../../crates/inf-game/src/object_runtime/world_load.rs)
before the placement pass. The native word index `world*4 + serial/32` has no
bound, so the port reads W+B0 through W+3CF as one span. An index past that
span, or a placement with no loader mission, stops as
`PickupMissionWordsUnknown`. The Level1 briefing camera targets are placed
pickups; see [the briefing camera](BRIEFING_RUNTIME.md#tick-and-drawing-owners).

## Draw and collection

[draw.rs](../../crates/inf-game/src/object_runtime/pickup/draw.rs) owns the
offline `0046DAF0` path. It resolves the actual first player's primary actor,
uses signed WORD XYZ deltas with wrapping DWORD squared sums, compares against
the sum of radii and tests signed lifecycle >1 before resolving the player
from the actor handle. Collection calls the real grant owner. A refused grant
continues normal drawing; an accepted grant rereads network state before the
save bit, collection flash, two sound2 requests and disposal. The save index
FFFFFFFF bypass is distinct from an unavailable mission owner. Sound pitches
use the fresh kind modulo6 and preserve their order around the particle birth.

The production adapter
[software_scene/pickup.rs](../../crates/inf-game/src/software_scene/pickup.rs)
borrows the retained mission save, terrain, particle/sound pools, process,
classification/text sources and renderer's current RNG loan. Grant83 healing
uses the shared actor status owner. Native pickup text formatting precedes the
actual HUD suppression gate; missing text is not silently replaced with an
empty message. Network collection remains a typed required protocol.

ModelFFFF clears draw flag0 before XYZ reads. Otherwise model and XYZ are
captured before color/clock/trail services. Kinds E/F/10 use an identity basis
and their separate pulsed color channel. Other kinds use the retained global
pickup basis; kind7 bypasses trail birth. The common pulse consumes actual
animation time and the original sine table. The ordinary model packet preserves
raw-local geometry, while the directional-light command receives the live
world light direction and native packed color before the existing renderer.
No pickup-specific model decoding or global orientation correction is added.

Process constructor `0046D3E0` supplies the initial basis. Tail `0046D940`
rotates it by signed `delta >>6`, preserving the basis normalization counter.
The world tick calls this immediately after `0048E738` stores manager far state
and before map jitter decay, as in `0048E2E0`.

`TimedPickupState::draw_stopped` is named portable containment, not a native
field. A reached draw/collection failure latches the allocation so later draws
do not repeat grants, births, sound or renderer prefixes. The timer still
reaches ordinary disposal; destruction and generation reuse clear custody.
The shared `SoftwareObjectEffectServices` carries firing and pickup loans;
missing firing tables/sound retain the old firing gate before RNG. World draws
provide pickup context independently of weapon tables. The briefing's object
pass (`0040C1EE`) is the same shared manager, so it lends the same pickup
services: the mission's save words, current world and pickup text. Before
it did, every Level 1 pickup's first draw came in the briefing and stopped
there on the missing terrain, and the latch kept it undrawn and uncollected
for the rest of the level. A probe drive now collects the pickups it
crosses.

## Pickup effect chain and evidence

Ordinary rotating pickups call `00488E90` with live XYZ, velocity `(0,-32,0)`
and strength200. The [particle authority](PARTICLE_RUNTIME.md#oriented-pickup-effects)
owns oriented bubbles/drops, terrain/water contact, flash and dust birth through
normal update, draw and destruction, including phase-gated scratch reuse.

Original-EXE fixtures use FPCW `027F` and leave instructions/data tables intact:
the tracked pickup generators and their compare/regeneration commands are
listed in NATIVE_RECEIPTS.md.

- [Object birth19](../../crates/inf-game/tests/fixtures/pickup_object_birth_native.json)
  executes `0046D870/0046D500` with supplied factory, refresh and height boundaries,
  including caller-pointer mutation and failures.
- [Object draw13](../../crates/inf-game/tests/fixtures/pickup_object_draw_native.json)
  executes the offline model branch, with supplied first-player NULL and lower
  trail/light/model services. It authenticates captured packet versus fresh
  operands, not a native successful collection.
- [Lifecycle and orientation](../../crates/inf-game/tests/fixtures/pickup_lifecycle_native.json)
  executes the actual constructor/rotation/timer leaves and oriented producer;
  producer terrain/birth returns are explicitly supplied.
- [Rust comparisons](../../crates/inf-game/src/object_runtime/pickup/tests.rs)
  and [failure tests](../../crates/inf-game/src/object_runtime/pickup/tests/behavior.rs)
  check whole retained prefixes, caller loans, normal disposal, no replay and
  generation reuse. Complete visible intro acceptance remains independent.
