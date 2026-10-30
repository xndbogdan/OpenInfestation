# Infestation route-node runtime

This document owns selector 51's live nodes, group lifetime and post-placement
finalization. [`object_runtime/route.rs`](../../crates/inf-game/src/object_runtime/route.rs)
implements that owner on top of [common object custody](OBJECT_RUNTIME.md).
[Instance assets](INSTANCE_ASSETS.md#route-node-command-inputs) owns the exact
authored command spans. Group construction does not implement the actors that
later move along routes, the world renderer, or the enclosing phase-3 loader.
The [shared target owner](TARGET_RUNTIME.md#common-object-position-targets)
owns type-16 resources retaining these nodes; selector-90 movement remains in
[INTRO_SEQUENCE.md](INTRO_SEQUENCE.md#effects-and-remaining-live-owners).

Authority is the supported retail executable, particularly
`00404AB0..00405140` and loader call `0048DFD0`, with
bulk/game_logic.c as the decompiled guide. Register and
WORD/DWORD behavior is checked against disassembly; the native evidence
below has a narrower scope than the entire world loader.

## Construction and the fourth node

Dispatcher `00460FCF..00460FEF` allocates `0x60` bytes and invokes `00404B20`.
That constructor calls visual constructor `0047A030`, initializes the fields
below, installs `0051F658`, then publishes the fourth node:

| Object field | Constructor value and ownership |
| --- | --- |
| `+44` DWORD | Zero; later squared segment length |
| `+48` byte | Zero; raw route policy bits |
| `+4C` pointer | Null; assigned route-group owner |
| `+50/+54/+58` | Fourth embedded node: next, previous, object |
| `+5C` WORD | `FFFF`; route identity copied from placement context |
| `+5E` WORD | `FFFF`; predecessor identity |

The fourth node is appended to the pending list at `005468F8`, separately
from the common object's three memberships. Startup `00404AF0` initializes
that list. `00404AB0` initializes the independent group list at `005468E0`.
The port owns both lists per arena and keeps stable group IDs distinct from
object IDs, authored placement identities and H13 resource slots.

Context `00404C30` first executes visual `0047A050`, then copies the low
placement WORD to `+5C`. Common placement WORD `+2A` remains a separate
field. Wrapper `00479DB0` calls this context method before applying the
ordinary flags DWORD through `00479D70`. No extra H13 schema or retained
payload pointer is introduced.

Command 3 stores its authored WORD minus one with WORD wrapping at `+5E`.
Command 4 accepts full DWORD zero by ORing policy byte `+48` with `1`, accepts
one unchanged, and rejects every other value. Commands 5 and 8 OR that byte
with `2` and `0x10`; commands 6 and 7 accept without reading payloads or
changing state. These commands do not dirty the common flags/model bytes.
Codes 0–2 retain the visual handler's policy; all other codes reject.

The actual final vtable has the same null actor query, true command guard,
no-op completion callback and network dirty clear as the previously ported
small classes. It shares `00401320` at `+20` and `00405140` at `+24/+3C`.
Lighting `+28` clears flag `0x08000000` through `00401350`; drawing `+2C`
is world model draw `0047AFC0`. These later manager callbacks are not
executed by construction or route finalization.

## Post-placement finalization

The loader calls `00404D40` at `0048DFD0` **after** the complete placement
loop, before disposal drain and virtual `+20` passes. The ordered stages are:

1. Walk pending nodes, saving the next node before mutation. Delete nodes with
   identity `FFFF`. Each self-reference (`+5C == +5E`) allocates a `0x28`
   group through `00404F90` and stores it at `+4C`. Groups enter their global
   list in root encounter order. The native allocation-failure path deletes
   that root; the Rust owner uses normal host allocation semantics.
2. For each still-unassigned pending node, `00404F40` scans the **current
   pending list** for the first raw-WORD identity matching its predecessor,
   then resolves that predecessor recursively. It stores the resulting group
   during unwind. Failure deletes this outer node immediately, changing what
   later searches can find. Duplicate identities are legal and order-sensitive;
   already-finalized groups are not searched.
3. Repeatedly walk pending order. `00405010` appends a self-root only to an
   empty group. Other nodes append only when the group's current last identity
   equals their predecessor. Each successful append physically moves the
   fourth node out of the pending list. Roots publish their policy byte to the
   group; appended nonroots overwrite their own byte with the group's value.
4. Stop when a pass moves nothing, then delete remaining pending nodes from
   the head. This removes conflicting branches instead of sorting them into
   an invented graph order.
5. Visit groups and their members in ordinary list order, calculating each
   node's segment to its successor. An open tail's existing `+44` is preserved.

Group policy byte `+0` is untouched by `00404F90`; the port represents it as
unknown until root insertion. Assigned group and physical list membership are
independent: a node can already point to a group while still pending.

`00404C50` and `00404C70` return next and previous members, with group-head
or group-tail wrapping when policy bit `0x01` is set. Ordinary iteration
`00404CA0` does not wrap. Periodic target producer `00427510` tests group
policy bit `0x02` to scan every linear member; clear bit scans only the root.
It separately skips the entire group when the root's common base flags +2C
contain `0x10`; child base flags do not repeat that guard. This use does not
assign the route-policy bit `0x10`, whose meaning remains open. Exact RNG,
metric and publication are owned by the
[periodic producer](ACTOR_BEHAVIOR_RUNTIME.md#periodic-player-and-route-target-producers).
`00404F00` searches finalized groups by comparing a full
DWORD one-based reference minus one with the **sign-extended common placement
WORD `+2A`**; this is different from the resolver's raw-WORD comparison.

For each axis, segment calculation subtracts coordinate WORDs with WORD
wrapping, interprets the result as signed `i16`, then squares it as `i32`.
The three squares are added with DWORD wrapping. A zero sum becomes one.
There is no square root, saturation, world-unit scaling or recomputation on
ordinary node deletion.

An unrooted predecessor cycle has no terminating case in `00404F40`. The
port detects repeated **object IDs**, returns `RouteCycle`, and retains
effects already reached in earlier passes. It does not classify such a cycle
as a native rejection or silently delete it. After any failed finalization,
the caller may inspect the reached state but must discard the whole arena
before loading again. Retrying finalization or per-node route cleanup is not
supported: for example, several pending nodes can already reference the same
empty group, which a first node's deletion would retire. This is a terminal
host error boundary, not a claimed native return path. Unknown coordinates
also produce an explicit error when a
segment needs them; unwritten position bytes never become synthetic zeroes.

## Destruction and lookup lifetime

`00404B90` unlinks node `+50` first. If its assigned group's member list is
then empty, `00404FD0` unlinks the group and `005116F4` frees it. Common
`00479D00` subsequently unlinks the other three nodes. A surviving group
keeps its policy, member order and stored distances. Command rejection must
also remove the still-pending fourth node.

The port's group handle includes its arena identity and a monotonic serial;
deleting and later allocating a group cannot revive an old handle. Read-only
group access keeps mutations under the lifetime owner. This is logical native
custody, not an emulation of allocator addresses or dangling pointer reads.

## Authored coverage and evidence boundary

The sole authored selector-51 resource is H13 slot 30. At the first-world
selection it has initial H8 WORD 255 and flags zero. All 40 `32.ovl` placements
use full argument `FFFF001E`. Their nine authored open chains, in root order,
are `53–57`, `58–63`, `64–65`, `69–72`, `73–76`, `98–101`, `102–106`,
`107–111`, and `112–116`. Across 33 world assets the same resource supplies
369 placements; corpus command counts are in the asset authority.

Focused port tests cover pending-order iteration, duplicated identities,
deletion changing later resolution, malformed cycles, stale/foreign handles,
last-member destruction, wrapping neighbors and WORD/DWORD distance arithmetic.
Canonical construction tests use current resident H13/H14/H8 resources and
the terrain-adjusted positions owned by `WorldPlacements`.

The [native routine receipts](../../crates/inf-game/tests/fixtures/object_route_native.json)
contain 17 scenarios, 308 snapshots and 276 direct native entries. Tests
compare every snapshot's represented scalar fields, common memberships,
pending nodes and group/member order. The receipts execute real constructor,
context, payload, command/admission, group append/resolution/query and bounded
destructor routines. Four calls execute all of `00404D40`, using either empty
state or honestly preallocated existing groups; they do not replace its
allocator with a success callback. Null append is retained as ABI-only
evidence because the safe object-ID API cannot represent a null object.
An empty-group destructor comparison triggers the port's actual empty-group
cleanup through a temporary node, rather than duplicating its unlink logic.

These isolated runs exclude CRT allocation/free, invalid-node deleting paths
and the first load's group allocation. The separate
[original-recording receipt](../../crates/inf-game/tests/fixtures/object_route_retail.json)
observes that real first-load allocation: function entry at `417258:1C86`
and caller return at `41725A:D19` on the same guarded thread/stack. Forty
pending nodes and no groups become nine ordered groups with member counts
`[5, 6, 2, 4, 4, 4, 5, 5, 5]`, and no pending nodes remain. The production
factory/finalizer test compares every represented base, visual and route
field, terrain-adjusted position, group policy, membership order and squared
distance both before and after. Placements 103 and 104 share a position;
the recorded segment from 103 is one, preserving the native zero case.

This snapshot ends before disposal and the loader's virtual `+20` passes.
It establishes neither invalid-graph deletion/free nor actor movement,
rendering or a playable phase-3 transition. Exact acquisition and local
transcripts belong in the
capture ledger.
