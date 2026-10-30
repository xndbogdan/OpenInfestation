# Infestation actor construction prerequisites

This document owns shared actor handle registration, construction and lifetime,
and the remaining derived-actor boundary. [OBJECT_RUNTIME.md](OBJECT_RUNTIME.md) owns the common
lists and visual prefix; [CONSTRUCTOR_ASSETS.md](CONSTRUCTOR_ASSETS.md) owns
authored actor payload fields. The priority and phase-3 acceptance gate remain
in the Infestation objective.

## Packed handle registration

The authority is retail `00461C80..00462650`, with call sites `004600D0`,
`0047A877`, `0047AA92` and `0047E2B0`. A handle is a raw DWORD:

| Bits | Meaning |
| --- | --- |
| 0..15 | Generation, incremented when allocating a free slot; zero is skipped |
| 16..25 | Slot index, ten bits |
| 26 | Mode-1 mark; allocation preserves this bit from the slot |
| 27..31 | Owner-table number |

These operations do not use reference counts. `00461FE0` binds an explicit
handle to an object; `00462210` unregisters it. The object owns its allocation
separately. A handle's generation is independent of the portable arena's
allocation generation.

`0054E888` points to owner records, each `0x18` bytes. Record `+08/+0C` are
the free-chain head/tail, `+10` is capacity, and `+14` points to twelve-byte
slots. Each slot stores the complete handle, object pointer and next-free
index. Owner zero is reserved. Slot zero is not a normal allocation, but its
physical storage and free link still affect the native chain.

`00461DE0` initializes every slot's generation and pointer to zero, embeds
owner/index in its handle, and sets next-free to index minus one. The free
head is capacity minus one and tail is one. Consequently fresh allocations
use descending slot indexes. Record `+00/+04` initialize to zero/one;
they are not counts of occupied slots.

The operation contracts preserve several intentional asymmetries:

- `00461EA0` pops the free head, stores the object, increments the low WORD
  with wrapping and skips zero. It reconstructs owner/index while preserving
  bit 26. It does not update the tail, clear the popped slot's link or publish
  a lookup-cache entry.
- `00462190` checks the single cache (`0054E884` handle, `0054E880` pointer)
  first. Otherwise it requires valid nonzero owner/index and a full DWORD
  match. A matching empty slot caches a null pointer; a miss leaves the old
  cache unchanged.
- `00461FE0` first performs that full-handle lookup. A resolving handle is a
  fatal duplicate. Otherwise it removes the selected slot from the free
  chain, stores the supplied handle and object, and publishes both in the
  cache. A different generation alone does not make an occupied slot free.
- `00462210` invalidates a matching cache before validating the handle. A
  matching occupied slot becomes empty and is appended to the free tail;
  its full handle is retained. Stale handles and repeated releases do not
  append the slot again.
- `00462480`, the mode-2 guard, tests that the selected slot is empty and its
  generation differs. It compares the low WORD rather than the whole handle
  and does not reject slot zero. It does not reserve or register anything.
- `004625D0` requires a current full-handle match, then sets bit 26 in both
  the caller's handle and table slot and clears the lookup cache. It does
  not replace the handle's owner.
- `0047E2B0` reassigns an actor by unregistering its old `+84`, storing the
  supplied handle at `+84`, and binding that handle to the same object, in
  that order. This is not a retain/release pair.

Allocation exhaustion and explicit out-of-capacity handles reach
`00461CE0`. Capacity doubles until it covers the request, capped at 1024.
Growth rebuilds a fresh descending chain, then visits old slots in ascending
order: occupied slots are rebound through `00461FE0`, while empty slots copy
only their full handle. The old free order is not copied. Rebinding also
determines the resulting lookup cache.

`004622F0` configures the number of owners and initial capacity. An unchanged
configuration returns without rebuilding, even after individual tables grew.
Startup `00461C80` requests two records of 256 slots; the observed first-world
constructor instead has 17 records of 256 slots. These are observed lifecycle
states, not interchangeable defaults. `004623F0` frees all table allocations
and clears the cache to the explicit global default `0054E894`.

## Portable owner and evidence boundary

[`ActorHandles`](../../crates/inf-game/src/object_runtime/actor_handles.rs)
implements these operations with explicit owner count, initial capacity and
null-handle inputs. Its object payload is a non-owning `ObjectId`: the caller
must unregister before destroying that allocation and still validate a
resolved ID against its arena. Clearing or reconfiguring the registry does
not destroy objects. Read-only owner, slot and cache views expose the state
needed to inspect this contract without permitting arbitrary mutation.

The portable domain is 2..32 owner records and 8..1024 initial slots, including
reserved owner/index zero. Native code diagnoses the lower limits but does
not make larger configurations safely representable in the packed handle.
Invalid binding arguments and exhaustion are explicit errors. A native free
walk that cannot find the requested slot is bounded and returns an error;
prior growth, cache or reassignment effects remain observable.

The native stale-tail behavior is preserved. Allocating all usable slots
leaves head zero and tail one. Releasing slot one writes its own index into
its next-free field through that stale tail; the next allocation can leave
the head pointing at the occupied slot. A subsequent allocation or binding of
a different generation can then overwrite that slot's registration. This is
not repaired into an ordinary queue. Focused tests cover that sequence, normal
FIFO release, generation wrap, marked handles, stale identity, cache misses, reserved-slot mode-2
behavior, growth and failed reassignment.

The standalone registry does not implement network cursor operations
`00462510/00462560` or destroy its referenced objects. The arena's actor
owner below joins registration to lifetime and list publication. The world
factory
[construction boundary](OBJECT_RUNTIME.md#bounded-implementation-and-evidence-boundary)
owns the current concrete selector coverage and remaining class-specific limits.

The native receipt
[actor_handles_native.json](../../crates/inf-game/tests/fixtures/actor_handles_native.json)
contains 17 scenarios and 127 snapshots. Its
[comparison test](../../crates/inf-game/src/object_runtime/actor_handles_native_tests.rs)
checks every logical owner/slot field, lookup cache and tracked caller handle
after each operation. The original routines complete 97 direct calls across
allocation, binding, lookup, release, mode-2 admission, mode-1 marking and
reassignment. Three failing reassignments instead stop at the first diagnostic
entry `00439230`; they establish mutation ordering before failure, not a
completed fatal path.

Table setup executes the genuine post-allocation success tail
`00461E04..00461E9B` 63 times on supplied buffers, with its exact register and
stack prerequisites recorded in the fixture. It does not execute the earlier
allocation call. There are no patched instructions, hooks or substitute
callbacks. Seeded prior table/cache/caller values are identified separately.
Full owner allocation, growth/reallocation, fatal reporting, actor lifetime
and network processing are outside these native receipts. Growth is covered
by static disassembly, independent review and focused source tests. Fixture
SHA-256: `1BDC2EA36F67E4D7594022073D01BCD94F4D7E4168D01A19E0E80D44CCDFB4D8`.

## Shared actor owner

The [shared factory](SHARED_ACTOR_FACTORY.md) owns constructor6's actual
`004604F5 -> 0047A7A0` class and retained H14 command custody. It does not
enter the derived constructor's additional mask/RNG phase.

[`ObjectArena::allocate_actor`](../../crates/inf-game/src/object_runtime/actor.rs)
owns the intermediate `0047A7A0` actor with read-only `ActorState`. The arena
must be created with `ActorRegistryConfig`: owner count, initial capacity and
null handle are explicit. Each `ActorConstructionRequest` supplies the
current owner byte, allocation position WORDs and terrain inputs. It does
not substitute the later placement context for unknown allocation bytes.
The constructor's `+88` source is a separate process input: `0047A885` reads
current `0054E894` after handle allocation, and `0047A88A` stores it. Live
factory callers borrow the process owner; the explicit
`RegistrySnapshotApproximation` preserves the older supplied-fixture policy
only where no process owner is supplied. Registry lookup NULL is not evidence
for the current constructor operand.

[`classify_actor_terrain`](../../crates/inf-game/src/object_runtime/actor_terrain.rs)
receives the current H12 surface or a genuine miss, signed plane DWORD,
coordinate shift, clock, optional H14 flags and sine table. The surface
carries the terrain spacing and height policy. H12 miss selects
`0x40000000` without reading position; a present H12 requires known position.
Missing H14 becomes an error only if native control flow reaches its flags
read. The near-plane branch uses a raw terrain cell and `00410F60` wave
math, not the interpolating `00450A30` sampler.

The host validates evidence/terrain inputs before publishing any object.
Successful construction owns the common nodes, ordered actor list (`G+48`
head and `G+58` tail predecessor within the same header),
handle registration and initialized scalar fields. Unwritten actor fields
remain unknown, including capture handle `+70` and prior capture flags `+D4`.
Their independent optional storage is written by the
[shared capture/release body](ACTOR_CAPTURE.md), not by the actor constructor
or payload initializer. If handle allocation fails, the new actor/common nodes are
removed while prior registry growth effects remain; this is host error
containment, not a reproduction of native fatal reporting. `destroy` releases
the handle, removes the actor node, then removes common nodes in native order.

`actor_list`, `actor_handles` and `actor` expose read-only state.
`resolve_actor_handle` runs the native handle lookup and then validates the
arena lifetime and actor class. The registry cannot be reset underneath live
actors through this API. The exact shared vtable's after-load `+34` is a
no-op, so `initialize_after_load` can set `0x00800000`; `update` instead
returns `ActorUpdateBoundary`, preserving the unimplemented update owner.
The exact shared payload and command paths are described below. Selector 88
adds the concrete extension and factory completion described in its own
section; other derived actors and manager passes remain separate owners.

The [shared angular response](ACTOR_ANGULAR_RESPONSE.md) owns complete
`0047D440` with explicit caller axes/gain and angular-only writes. Constructor93
composes it with the existing basis and physics owners; its
[movement policy](ATTRACT_DEMO_RUNTIME.md#constructor-93-movement) remains distinct
from construction, direct demo admission and context-free `update`.

The separate [shared-actor native receipt](../../crates/inf-game/tests/fixtures/actor_base_native.json)
contains 54 cases and 120 snapshots: 60 complete `0047A7A0` constructors,
60 complete nondeleting `0047AA60` destructors and 54 original identity
initializers `00439630`, plus 108 genuine owner-table initialization tails.
The [comparison](../../crates/inf-game/src/object_runtime/actor/native_tests.rs)
checks initialized scalar fields, visual basis, four ordered node memberships
and registry/cache state. Constructor classification counts are 21 unchanged,
8 setting `0x20000000` and 31 setting `0x40000000`.

Inputs explicitly supply allocation bytes/positions and resident H12/H14
state. Coverage includes the actual H12 miss callback, both grid spacings and
height scales, signed cells, plane/margin boundaries, original sine-wave
paths, clock/shift/WORD wrapping and three multi-object destruction orders.
The oracle also checks untouched holes, guards and terrain bytes. It uses no
patched code or substitute callbacks; heap allocation/free, allocation-byte
provenance, derived actors and world admission are excluded. Fixture SHA-256:
`819245E97BF35B8178E34DDA8A443D40C51D74FE182E82C6CB9B9F51FEDF4F9E`.

The [fresh-heap policy owner](../../crates/inf-game/src/fresh_heap.rs) also
records the explicit retained four-carry allocation approximation and its
[constructor service](../../crates/inf-game/src/object_runtime/actor/allocation.rs).
It supplies no native heap-history or full actor-update acceptance.

### Pure actor derived-component query

[`ObjectArena::actor_derived_component`](../../crates/inf-game/src/object_runtime/actor.rs)
owns actor virtual `+58` alone. The literal callbacks are pure:
`00404610` returns `ECX`, while `00401300` returns zero. All represented
derived classes return self, including the base, selector 2, selectors
11/60/77 and the `00428650` families. Shared actors and selectors 16/30/88/90/93
return null. The implementation exhaustively classifies the represented
actor extensions and derived classes after validating the live actor.

This query does not invoke `+CC` or read its component's `+22C` byte. It also
does not consume behavior `+140`, classification `+B4`, lifecycle `+C4` or
position. In particular, the base and selectors 2/11/60/77 still return self
from `+58` even though their `+CC` returns null. A family with `+22C = 0`
also returns self. Intro stages that require `+CC` and active `+22C` perform
that separate query after `+58`; retirement's `HasDerived58` predicate does
not. The [focused test](../../crates/inf-game/src/object_runtime/actor/tests.rs)
pins these distinctions, unchanged state, and live/non-actor admission.
The query itself owns no traversal or update attachment; the separate
[common actor-list selection kernel](FAMILY_TARGET_SELECTION.md#common-actor-list-selection-callback)
uses this pure `+58` policy without an extra `+CC` or active-byte test.

### Pure actor behavior-presence query

`ObjectArena::actor_behavior_presence_7c` in the same
[actor query owner](../../crates/inf-game/src/object_runtime/actor.rs)
owns the exhaustive canonical `+7C` policy. All represented derived tables
use `004027A0`, reading actual `+140`; shared actors and selectors 16/30/88/90/93
use null `00401300`. Consumers require full-DWORD nonzero presence, not low
AL or a fabricated physical pointer. The method consumes no `+CC`, `+22C`,
classification, lifecycle or position and changes no state. The focused pure
query tests cover derived null/non-null behavior and the null query classes,
non-actors and retired identities alongside the distinct `+58` self policy.

The [common inline scanner](FAMILY_TARGET_SELECTION.md#scanner-order-and-scoring)
captures this presence once; common `+104` borrows it for its initial query
and pure task-relation bridge. The
[selector-12 `+100` policy](FAMILY_TARGET_SELECTION.md#selector-12-inline-table-selection-callback)
queries it per reached row, then separately rereads `+140` directly. Sharing
the pure query does not collapse these intentional caller/read-order policies
or admit arbitrary mutable virtual overrides. No production `+24` is attached.

The [selector-12 actor-list policy](FAMILY_TARGET_SELECTION.md#selector-12-actor-list-selection-callback)
captures self presence after class/radius, seeds its primary threshold from it,
and uses direct candidate `+140` captures through the mutable gate. Its fresh
task relations and conditional clock notifications share their existing owners.

[Canonical actor interactions](ACTOR_INTERACTIONS.md) separately own `+50`/`+54`,
selector-30's numeric `+CC` and void `+9C`, including selector-35's conditional
write through the borrowed live animation clock.

## Shared payload, commands and completion

[`ActorInitializationRequest`](../../crates/inf-game/src/object_runtime/actor/initialization.rs)
combines rotating visual context, the current local-owner byte and a borrowed
`Arc<ActorConstructorPayload>`. `initialize_actor` follows exact wrapper
`0047AD70`: virtual `+08` (`0047A610`) installs context/model/basis first,
then `0047AC10` applies the separately supplied payload. A payload flag can
therefore restore mask `0x4` after a disabled model cleared it during context
initialization. A direct `initialize_actor_payload` entry also represents the
native payload setter without repeating context work.

The payload sets current/maximum health, `+8C`, mapped selector `+B4`, `+7A`
and `+76`; it preserves the high flag byte and ORs `0x80000000` when the
handle owner differs from the supplied local owner. A matching owner does not
clear an existing remote bit. `+7A` clamps inputs at least 175 to zero. The
optional `+28` override tests the full payload DWORD before truncating its
low WORD, so `00010000` applies a zero override. These are runtime policies;
the canonical parser preserves the original inputs.

Native `+A4` retains the response table at payload `+10`. `ActorDamageTable`
represents either immutable constructor table `00531260` or a retained clone
of the same canonical payload `Arc`. Multiple actors can share that identity;
replacing a resident H13 slot does not redirect an existing actor's responses.
It is not a copied mutable response array or a later lookup by resource slot.
Exact shared postinit `0047AAD0`, exposed by `finish_actor_initialization`,
stores unsigned `+28 >> 3` at `+74`. It does not itself clear dirtiness, mark
factory initialization, notify listeners or run a derived postinitializer.

[`ActorCommandContext`](../../crates/inf-game/src/object_runtime/actor/commands.rs)
supplies current model residency and the actual first player's borrowed
six-DWORD grant mask (`PlayerSaveState::owner_a0`). `None` means the first
player list is absent, not an all-zero mask. Commands 13/14 then accept
without consuming even a missing operand. With a player, they consume an
unsigned WORD index and use `grant_is_owned`: index zero is true, indexes
1..191 read the corresponding bit, and indexes at least 192 are explicit
host errors instead of reading beyond the owned six-word storage. The two
commands queue disposal on opposite results and still report acceptance.

The runtime consumes the canonical decoder's explicit `SharedActor` target,
which names `0047ADA0` without inventing an H13 selector. Codes 0/1/3 accept
without payload reads, code 2 applies the existing model policy, and codes
4..15 apply their exact shared scalar/conditional effects. Code 9 does not
repeat the payload's clamp. Code 12 stores the signed DWORD at `+C4` and
ORs sticky `0x8000` for values below three, including negatives. That mask
cannot satisfy the native low-nine-bit dirty test, so dirtiness is unchanged.
Negative codes and codes above 15 reject without reading their operands.

`run_actor_commands` implements the ordered `00479E10` walk: store the low
resource WORD, pass the exact shared `+38 = 00401370` always-true guard,
then visit each reached command. Null suffixes and nonpositive signed counts
read no entries. A rejection stops before any later pointer/payload read and
retains the object and earlier mutations. Disposal does not stop the walk.
The enclosing native completion `004602D0` owns deletion after rejection,
or postinit and dirty clearing after acceptance; the standalone walk does
not perform that cleanup or claim complete factory admission. Shared vtable
`+80 = 0047AAE0` is a separate one-byte-argument predicate, not this guard.

The [initialization receipt](../../crates/inf-game/tests/fixtures/actor_initialization_native.json)
contains 48 cases and 216 snapshots, with 233 complete direct native entries:
31 payload calls, nine context/payload wrappers, 18 postinitializers, 55
direct commands and six ordered command walks, plus constructor, identity
startup and destructor setup. It also executes 96 genuine owner-table
initialization tails. The [comparison](../../crates/inf-game/src/object_runtime/actor/initialization_native_tests.rs)
checks scalar fields, model/basis, shared payload identity, all ordered nodes
and registry state. Cases cover signed switch wrapping, full-DWORD extent
tests, unchanged dirty bits, null/empty suffixes, early rejection, lazy
conditional reads and disposal followed by further mutation.

Four native grant queries deliberately use oversized synthetic storage for
index 65535. The port checks their explicit `GrantClosureError::Id` and
unchanged state; it does not claim completed-call return equivalence outside
the six-DWORD player mask. Other direct-command returns and all six suffix
outcomes match. The original routines, vtable guard, player lookup, grant
query and disposal run without patches, hooks or replacement callbacks.
Allocation inputs, prior scalar seeds, model residency and player masks are
explicit synthetic prerequisites. Heap allocation/free, derived actors,
creation listeners, manager updates and world admission are excluded.
Regeneration is byte-identical; fixture SHA-256:
`B79C71DBF8A70E61D52E57576802518A945A7FC66366BAB1E59E90CB0EC64248`.

## Central derived actor and selector 77

[`DerivedActorState`](../../crates/inf-game/src/object_runtime/actor/derived.rs)
owns `00425E60`, the final constructor for selectors 7/33 and a shared
intermediate for most first-world actors. After `0047A7A0` publishes the
actor, it consumes exactly one process RNG step from `0048F060`. World
accessor `0048E120` returns **signed** H14/0 bytes `+0E/+0F`; the range is
neither sorted nor clamped. With the unsigned 16-bit RNG result `r`, the
iteration count is `i16((r * (high - low)) >> 15) + low`. The constructor
builds WORD `+178` by ORing the low WORD of `1 << (i & 31)` for each
iteration. Nonpositive counts produce zero, and counts at least 16 produce
`FFFF`. Equal limits still consume a draw. The factory requires both these
world parameters and the caller-owned process RNG explicitly.

The constructor sets `+B4=1`, dirty bit 2, `+164=2500`, and its known null
or zero fields. Payload-only fields remain unknown. Wrapper `00426BF0`
initializes the final rotating context, then `00426C10` applies the shared
actor payload followed by the extension. It stores byte `+44` at `+13D`,
ORing bit 7 when payload WORD `+48` is nonzero; sign-extends payload WORD
`+4A` into actor `+164`; copies DWORD `+4C` into `+174`; and preserves the
four raw sound WORDs `+50..+56` at `+17C..+182`.

Command `00426030` delegates signed codes at most 15 to `0047ADA0`.
Code 16 succeeds without reading its operand. Code 17 zero-extends a WORD
into `+164`, deliberately unlike the signed payload read. Code 18 stores
the wrapping negation of its DWORD at `+144`; a nonzero result ORs flags
`04200000`, while zero does not clear earlier flags. Other codes reject.
Such an actor sleeps: flag `00200000` marks it, and the updates return
while `+144` is negative.

### Selector 42 wake triggers

Factory case 42 (`00460DDE`) allocates `0x50` bytes and calls `00401020`:
the visual base `0047A030`, final vtable `0051F018`, then `+44 = 0`,
`+48 = 0x5F5E10` (2500²), `+4C = +4D = 0`. The vtable differs from the
visual base only in the destructor, commands `+04` (`00401080`), update
`+24` (`004011F0`) and draw `+2C` (`004012D0`); `+3C` is `00405140`. Its
scalar deleting destructor is `00401050` (`00401070`).
Commands pass codes up to 2 to `0047A1A0`; code 3 squares a WORD into the
trigger radius `+44`, code 4 a WORD into the wake radius `+48`, and codes
5 and 6 store a BYTE ≠ 0 at `+4C` (fire on draw) and `+4D` (update off).

Unless `+4D` is set, the update measures the ground square (WORD-wrapped X
and Z differences) to the first player's primary actor (`0046B030`) and
remote unit (`0046B060`, the `+D8` handle), keeps the nearer (the remote on
a tie) and fires when it is below `+44`. The draw draws nothing; with
`+4C` set it fires outside UI state 3. Firing (`00401110`) walks the G+48
actor list: each actor with flag `00200000` whose ground square is at most
`+48` gets flag `00400000`; its `+58` component then turns a negative
`+144` into `+144 × -1000` and clears flags `04600000`. The trigger then
disposes itself (`00479FA0`).

The [port](../../crates/inf-game/src/object_runtime/selector42.rs)
constructs, commands, updates and draws the triggers. Level 1 places two;
Infest01 updates each 707 times and never fires one (bundle
`.tmp/infestation-ttd/20261008-152412-858-wake/`). Infest02 fires two
later (bundle `.tmp/infestation-ttd/20261008-152553-576-wake02/`): at
`1E4848:132F` an update wakes one of eight sleepers within 1000 (flags
`44B0002F` → `4090002F`, `+144` -1 → 1000), and at `1EF4DF:13B9` a draw
(`+4C` and `+4D` set) wakes four within 2500 (`46B00867` → `42900867`) after
177 briefing draws under UI state 3 left it waiting. The
[port's test](../../crates/inf-game/src/object_runtime/selector42_tests.rs)
replays both and matches every sleeper's flags and `+144`.

### Selector 64 course gates

Factory case 64 (`00461319`) allocates `0x78` bytes and calls `004407C0`:
`004404F0` builds the rotating visual `0047A490` (vtable `005229D8`) with
the animation DWORD `+6C = 0` and bytes `+70..+72 = 0`, then `+74 = 0` and
the final vtable `00522A28`. Its commands `004407E0` pass codes up to 2 to
`0047A1A0`; code 3 keeps the operand's address at `+74`. The after-load
`+34` (`00440890`) replaces the operand's first DWORD, a placement
reference, with that placement's actor handle (`0047F7B0`).

The update `+24` (`00440520`) eases `+6C` (the model value, `004407B0`) by
`delta >> 4`: up to `0xFFFF` once passed, down to 0 before. If the first
player's primary actor (`0046B030`) is alive (`+C4 = 3`), the distance is
the integer root (`0048EFC0`) of its squared offset (a quarter of it, then
doubled, when the sum overflows); otherwise `0x7FFF`. Below the gate's `+28`
extent the offset goes through the gate's inverse basis (`0047A660`); local
Z above zero sets `+70`, otherwise `+71`, and with both set the gate runs
`+4C` (`00440810`) and clears them. Outside the extent both clear. Without a
live actor and an extent above `0x7FFF`, native transforms a stale offset;
the port reports that boundary.

`00440810` runs once (`+72`): when the actor's handle names a player
(`0046B090`), it grants pickup kind 4 with 5000 (the vehicle repair, through
`0046B2B0`) and plays sound 1 at the gate (`00487F10`); then the linked actor
gets message 0x15 with the operand's second DWORD. Selector 30's `0048C470`
takes 0x15 by counting its signed WORD `+164` down and, at 0 or below,
setting the first player's silent command 9; code 0x14 counts up. The draw
`+2C` (`00440640`) publishes a directional light from the H12 light words
with X and Z quartered, normalized like `0044FE17`, in style 0x0E (0x28 once
passed), then draws the common visual. Later objects inherit that light.

What a passed gate does to its link depends on the linked class's `+98`
for 0x15. The worlds' links (each world loaded, 2026-10-19; worlds 1, 18
and 19 do not load in the offline test, and Level 1's are unlinked):

| World | Gates | Linked to | 0x15 |
| --- | ---: | --- | --- |
| 11 Artoria | 16 | its warpgate (resource 0xF) | counts the lock `+164` down |
| 14 Foldrak Noir | 3 | its warpgate | the same |
| 15 Kankutan | 4 | its warpgate | the same |
| 16 Defence Satellite | 13 | a barrier each (selector 69, resource 5) | `00411CE0`: its `+7A` to the first player's grants, then it goes (`00479FA0`) |
| 17 Shak'tahh | 4 | a turret each (selector 11, resource 0xB) | `00402420`: it retires (`0047CE90`) |
| 17 Shak'tahh | 1 | a Force Field (selector 18) | none (`004787E0` acts on 7, 8, 0xB and 0xC) |
| 17 Shak'tahh | 2 | its two warpgates | counts the lock down |

So flying Shak'tahh's high gates destroys its four guns. The port's `+4C` once handled only the warpgate and treated
every other class as ignoring 0x15: Shak'tahh's turret gates stopped at
an "unported `+98`" and Defence Satellite's barriers stayed. It now sends
every link but a warpgate through the shared dispatcher (`message.rs`)
and the world's effects, as other senders do
([test](../../crates/inf-game/src/object_runtime/selector64_tests.rs),
over both worlds' real links).

Level 1 places 13 gates (extents 400 and 250), none linked. Infest01
passes two (bundle `.tmp/infestation-ttd/20261008-154513-091-gates/`); the
[port's test](../../crates/inf-game/src/object_runtime/selector64_tests.rs)
replays every update around both passes (bundle
`.tmp/infestation-ttd/20261008-154702-626-gates2/`) and matches each side,
animation and passed byte and both repairs.

### Selector 53 pickup spawners

Factory case 53 (`00461067`) allocates `0x140` bytes and calls the shared
`0047A7A0`, then writes the spawner's defaults: kind `+138 = 1`, count
`+139 = 10`, seconds `+13A = 3`, repeat `+13B = 0` and timer `+13C = 0`.
Its final vtable `00526648` is the shared actor's `00527C00` but for five
slots: the destructor `004110E0` (the same `0047AA60` teardown), the
commands `+04` (`00454A30`), the update `+24` (`004548E0`), `+70`
(`00404610`, which returns the spawner) and `+98` (`00454AB0`). It
initializes through the shared `0047AD70`. The commands pass codes up to 15
to `0047ADA0`, copy one BYTE each into `+138..+13B` for codes 16 to 19, and
refuse higher codes.

The update compares the timer with the delta, signed. A timer above the
delta counts down. A zero timer leaves the update list (`00479F20(0)`)
unless the repeat flag is set; then it arms at `+13A` seconds (the signed
BYTE times 1,000,000). Otherwise the spawner takes one from its count
(`00454B10(1)`: the smaller of 1 and the signed count, so a negative count
takes itself). A nonzero take drops pickup kind `+138 + 13` with value 1
(`0046D870`) at the spawner's position, moved by two process RNG draws in X
then Z, each `(draw & 0x3FF) - 0x200`. An emptied count leaves the update
list. The timer then restarts, or stays zero without the repeat flag.

The `+98` acts on message 0xF only. With a zero timer and a nonzero count,
it arms the timer and rejoins the update list (`00479F20(1)`); it returns
the signed kind while the spawner is on the list. Its senders
(`0045463D`, `0045469C` and `00454851`) are the
[mining rigs](#selector-52-mining-rigs); Level 1 has none. The messages it
does deliver (7, 8, 0xA, 0x15, 0x16 and 0x17) return 0 here. The port's
`+98` matches the first twelve 0xF calls Helniazine's three mines take in
Infest04: an idle mine arms 3 or 12 seconds and joins the list, and each
answers its kind.

In the briefing map, `+70` sends the spawner into `00472D30`'s marker block
whatever its flags. It takes both jitter draws, moves the cursor and
resolves its palette (`00472170`). It draws a marker
(`00472EAD..00472F7C`: style 0x22 to 0x24 by kind, size twice the count)
only when the final argument's low byte is set. The briefing's `0040C390`
passes 0; the port reports a nonzero byte as a boundary.

Level 1 places four spawners (placements 47, 50, 51 and 52), all of kind 2
(pickup 0x0F) with repeat set: 8 pickups 6 s apart, 5 at 35 s, 5 at 45 s
and 10 at 30 s. Retail numbers its actors' death indices `+78` in
construction order (`0047F6C0`), spawners included. Before the port built
them, every later actor's index was four short. The spawners take 12 to 15,
the gate 16 and the two Selector34 ships 21 and 22 (`spawner` query,
`.tmp/infestation-ttd/20261009-052913-096-spawner-i01/`). Infest01 updates
the first spawner 385 times, until its eighth pickup empties it, and the
others 707 times, with 2, 1 and 2 drops. [The port's
test](../../crates/inf-game/src/object_runtime/actor/selector53_tests.rs)
replays all four from their first update and matches each of the 13 drops:
the position `0046D870` receives, the RNG, the pickup's kind and value, and
the timer and count after.

### Selector 57: the Powerball

The Powerball is case `0x39` of `004603B0`: `0x138` bytes and the shared
`0047A7A0`, with no fields of its own. Its final vtable `00526464` is the
shared actor's `00527C00` but for four slots: the destructor `004110E0`,
the update `+24` (`004863F0`), `+98` (`004864B0`) and the death `+AC`
(`00486420`). It initializes through the shared `0047AD70` and takes its
commands through `0047ADA0`.

Its update runs the shared physics `0047B1E0(delta, 0xD)` (gravity, linear
drag and position), sets its health to its maximum (`+A0 = +A2`), then runs
the shared status `0047C990`; the terrain contact holds it on the ground.
Its `+98` acts on message 5 alone: `0047CE90` retires it, then its sender
too when the sender's resource word `+30`, read after the first retirement
and with no null check, is `0xE`. Every message answers 0. Its death spawns
resource `0x36` through its `+4C` (`0047AAF0(0, 0x36, 0)`), bursts
(`0048A8B0`: 16 type-1 debris within 20 of it and a type-1 centre, speed
8), calls the actors within 2000 with message `0xC` (`0047DD20(2000, 0xC,
2000)`) and disposes itself (`00479FA0`).

World 11 (leaf 21) places one Powerball (placement 102, resource 39) and
leaf 24 eight. Infest04 and Infest03 build world 11's and update it 517 and
199 times, and the only message it gets is `0x16`; it never dies. It
settles on the ground: each update takes its velocity from the contact's
40 to -81 and its height down 10, and the contact pushes it back.
[The port](../../crates/inf-game/src/object_runtime/actor/selector57.rs)
builds all nine, runs the update and `+98`, and admits it to the shared
callbacks and to the terrain and furniture contacts (its table's `0047D2F0`
and `0047BFD0`); the world delivers message 5's two retirements. Its death
runs inside the lifecycle, whose child factory and message delivery it
borrows from the world's owners; no flag gates any of it, and
check_selector57_death.py's four cases
match. The world's damage owners lend the world map to the lifecycle, so a
player's vehicle within 2000 takes its radar jam (`00420BE0`) and the death
runs on to its disposal; a caller with no map (none outside a world) still
stops there.
Its tests match world 11's construction, its load, four of its updates and
its first three ground contacts (`powerball.js`). Its load
serial `+78` (`0047F6C0`) counts the five
[Force Fields](SELECTOR18_RUNTIME.md) (selector 18) placed before it.

### Selector 59: the Boulders

The Boulders are case `0x3B` of `004603B0`: `0x148` bytes, `00405780` over
the settling base `00450400` (the shared `0047A7A0`, then `+140 = 0`).
`00405780` sets crushing `+144 = 1` and `+145 = +146 = 0`. The final
vtable `0051F900` is the shared actor's `00527C00` but for the destructor
`004057B0`, the commands `+04` (`00405D10`), `+0C` (`00405DE0`), `+1C`
(`00405DC0`), the update `+24` (`004057D0`), the network `+40`
(`00413120`, the shared `0047E2E0` and then dirty `0x10`), the terrain
contact `+94` (`00405A00`) and the pair reaction `+A4` (`00405E30`). Its
`+98` (`004045F0`) answers every message with 0 and its death is the
shared `00404600`.

`00405DE0` runs `0047AD70`'s two calls, then stores the rest timer
`+140 = 500001` and sets flags `0x04200000`: a new boulder is dormant (for
the [wake trigger](#selector-42-wake-triggers), selector 42, which sets
`0x400000` on it) and asleep, which also keeps it out of the terrain and furniture
contacts. The commands pass codes through 15 to `0047ADA0`; code 16 with a
nonzero BYTE wakes it (`+140 = 0`, flags `0x04200000` cleared), 17 stores
BYTE == 0 at `+144`, 18 and 19 BYTE != 0 at `+145` and `+146`; higher
codes refuse. `00405DC0` replaces the shared `0047AAD0`: the boulder rises
by its `+28` and keeps that position as `+138`.

The update compares the position with `+138`. Where it was and still (no
X or Z velocity, Y within 32), a woken boulder clears `+140` and the flags
`0x04600000` and moves; otherwise an asleep one (`+140` at least 500000)
returns, and a resting one adds the delta to `+140` and moves, or, reaching
half a second, sets flag `0x4000000` and returns without moving. Having
moved, while drawn near (flag `0x2000000`) it rolls: each axis's move
becomes a WORD angle (the signed difference times 10430 over its radius
`+28`), the world up vector `0055F280` crosses it in the FPU (X and Z
spilled to f32, `00511BA8` truncation), and the result is turned into its
frame (`0047A660`) and back out to turn its basis (`0047A760`); then
`+138` takes the new position. Moving clears a set `+140` and the three
flags, runs the shared physics `0047B1E0(delta, 0xD)` and, with `+146`, the
status visual `0047DFD0`. A boulder resting on the ground never sleeps:
the contact's rebound leaves Y at 40.

Its terrain contact runs the shared `0047D2F0`, then, drawn near and
faster than 100, a draw (`0048F060`) below `(speed + 20) * 64` throws up a
particle at the contact point (the radius back along the normal) with half
its velocity (Y's magnitude): a splash (`00489600`) where the water
(`00410F60` with its waves always on, over `00450A30`'s ground) stands
above the point, dust (`004894A0`, no shadow) elsewhere. The same draw
below half of that throws a second, the point moved by up to half the
radius on each axis. Its `+A4` crushes: with `+144` or `+145` set, its
change of speed since the velocity saved before the exchange past 320 (any,
with `+145`) damages the partner (`0047C6E0`) by `isqrt(excess) * excess
>> 4`, times its mass `+8C >> 6`, at most `0xFFFF` (the product wraps), on
channel 3 from its handle; the packet's BYTE `+3` is the high byte of a
reused argument slot, which the port leaves unwritten.

World 7 (leaf 17) places eight boulders, world 13 (leaf 23) nineteen and
world 20 (leaf 30) two; on world 20 a [selector 60](SELECTOR60_RUNTIME.md) spawner
builds more at placement 3's place. Infest03 builds world 7's, which sleep
through its 232 updates. Infest04 builds world 20's two and three
spawned; placement 3's own boulder is destroyed between its
construction and the first update. Their 6,633 updates, 4,338 contacts and 688 pair reactions
include five crushes, one of the player's vehicle by 1804.
[The port](../../crates/inf-game/src/object_runtime/actor/selector59.rs)
builds the class, its commands, update, network completion, terrain,
furniture and pair contacts. Its tests match the construction and load of
every placed boulder the recordings build, six updates (one rolling), sixteen dust-throwing
contacts and two others with their births and RNG, and all five crushes
and two misses (`boulders.js`).

### Selector 52 mining rigs

The portable mining rig ([hold item](HOLD_ITEMS.md) `0x23`, "beamed down
near a resource mine, ... will extract crystals") is case `0x34` of
`004603B0`: `0x144` bytes, the shared `0047A7A0`, then running BYTE
`+138 = 1`, links `+13C = 0`, level WORD `+140 = 0` and filling BYTE
`+142 = 0` (`0046102E..0046104F`). Its final vtable `00526714` is the
shared actor's `00527C00` but for six slots: the destructor `004110E0`,
the commands `+04` (`00454880`), the model values `+14` (`00454720`), the
update `+24` (`00454630`), the pickup and drop `+8C` (`004547E0`) and
`+98` (`004546F0`). It initializes through the shared `0047AD70`. The
commands pass codes up to 15 to `0047ADA0`; code 16 stores its BYTE == 0
at `+138` and clears flag `0x40` (dirty `+38 |= 2` when the low nine bits
change); higher codes refuse.

Its `+98` counts links: message 7 adds one and 8 takes one; every message
answers 0. A course target's after-load (`0043B720`) sends its linked rigs
7, and a completed target 8 as well. While any link holds it, the update
calls the mines within 2000 (`0047DD20(2000, 0xF, 0)`) and does nothing
else. A free rig whose running BYTE is set takes `delta / 100` (unsigned)
units: draining, a level above them (signed) loses them and any other
empties; filling, a level below `0xFFFF` less them (signed) gains them,
else it becomes `0xFFFF` and the rig calls the mines within `0x400`; no
answer clears the filling BYTE. The `+14` values ramp with the level for
model selectors 1 to 5 and 8, and selector 9 answers whether it is full.
The `+8C` refuses a load while the rig holds a level, giving the first
player hint 0x18 (`0046C760`); loading an empty rig gives the loader's
player (`0046B090`) hint 0x1F and grant `0x23` (`004750C0` on its `+9C`)
before the shared `0047D7C0`. A drop that `0047D7C0` accepts starts the
filling when a mine answers within `0x400` (`0045485E`). The vehicle's
carry runs this `+8C` both ways ([hold items](HOLD_ITEMS.md#portable-mining-rig-selector-52)). Its
death is the shared `+AC` `00404600`, which only disposes it (`00479FA0`),
as the shared actor's and the spawner's do; the port's lifecycle runs it
for all three tables.

Helniazine places four rigs (placements 76, 99, 102 and 109) and its
course target links the first three; the fourth runs empty. Infest04
builds them, links them at load and updates the linked ones 2,564 times,
each calling the mines within 2000 and changing nothing; no rig is picked
up. Infest03 shows the same on Helniazine and three unlinked, stopped rigs
on world 7. [The port](../../crates/inf-game/src/object_runtime/actor/selector52.rs)
builds the class, its commands, links, update and model values, and
admits it to the after-load and shared callbacks. Its tests match the four
rigs' construction and links and three of their updates
(`mining-rig.js`).

Admission `+38=00426C80` runs after the resource WORD is stored and before
any suffix count or command read, including a null suffix. Low-seven-bit
modes 1 through 13 replace owned behavior `+140`; other modes succeed
without changing the previous owner. The high bit independently selects
one or two groups. [`ActorBehaviorState`](../../crates/inf-game/src/object_runtime/actor/behavior.rs)
owns the initialized descriptor, entry and group arrays from `0048CC70`.
Unwritten array words remain unknown. Descriptor `+08` for mode 10 is
numeric **5,000,000** (`004C4B40`), not the code pointer printed by Ghidra.
Selection `00428390` preserves the current selection when flag `2000` is set,
otherwise chooses entry zero
for actor `+B4` equal to 3/18 and entry one modulo the signed entry count
for other values. Destructor `00425F70` releases `+148`, `+154`, behavior
`+140`, then audio `+168`, before shared destruction. The implemented
construction paths leave the first two resources and audio null; later
resource creation and behavior execution remain unsupported.

The [central native receipt](../../crates/inf-game/tests/fixtures/derived_actor_native.json)
contains 20 cases and 260 snapshots. Complete original constructor,
context/payload, command, postinitializer and destructor calls cover signed
range extremes, RNG values across bit 15, signed payload versus unsigned
command reads, lazy invalid operands and sticky flags. All 20 complete
guard calls exercise nonallocating modes. Allocating guard branches and
resource-owning destructor branches have static authority and host ownership
tests, not completed native execution receipts. No allocator or callback is
substituted. Fixture SHA-256:
`6C2FFD05D6A851888D5138D9E6252D86A30454BC16DCECA95D4A82B7426238D6`.

The separate [behavior allocation-tail receipt](../../crates/inf-game/tests/fixtures/actor_behavior_allocation_tails_native.json)
compares 208 states across all 13 profiles, both group counts, selector cases
and the selection-preserving flag. It executes 208 original guard prefixes,
832 postallocation tails and 208 full `00428390` calls. Each allocation
boundary records its real requested size and return site; the following
tail receives explicitly supplied poisoned storage. These are segmented
execution checks, not completed allocator/guard/free calls. The old `+140`
is null in every case, so replacement destruction remains statically proven
and tested through portable lifetime ownership. Fixture SHA-256:
`EF306E051F616D55336185BBC133F12BBBDB8E6D783EFB1225B1C11A252AFBCD`.

[`Selector77State`](../../crates/inf-game/src/object_runtime/actor/derived/selector77.rs)
adds constructor `00407410`: fourteen zero bytes at `+184`, null state at
`+1A4/+1A8`, the explicit global null handle at `+1B0`, and a **second**
process RNG draw shifted left two into `+198`. Position `+192` remains
unknown until postinitializer `00408090` reselects behavior after commands,
calls shared `0047AAD0`, then copies the six current position bytes.
Its commands, payload and admission are inherited from the central class.
Selectors 7/33/77 use actual no-op `+34=00401310`; their common after-load
flag operation is supported. Selector77's [model capture](OBJECT_MODEL_CALLBACKS.md#selector77-eye-capture)
owns LOWBYTE2/10 on the same retained192 row. Its
[NULL-target update](ACTOR_BEHAVIOR_RUNTIME.md#selector77-null-target-update)
owns the bounded+24=00407CA0 route; target services, central-class update and
other derived classes keep explicit boundaries.

The [selector-77 receipt](../../crates/inf-game/tests/fixtures/selector77_native.json)
compares 56 snapshots in eight cases, including complete original constructor,
context/payload, nonallocating guard, command, postinitializer and destructor
calls. Both process RNG draws and current position copying are compared.
It does not execute an allocating guard or the enclosing world factory.
Fixture SHA-256:
`CBC35833B61020FE2EEF841721B62B300935DEA93FBE86D289815D9AA9EB228A`.

## Behavior registry resource classes

The behavior registry `0053DBA8` names at `+08` the H13 resource of each
row's weapon. The vehicle's sixteen rows (`00530158`) and the mode tables
name resources 110 and 113 to 130, whose constructors are selectors 25, 26,
27, 28, 31, 32, 38, 68 and 76. [`0048D820`](BEHAVIOR_DAMAGE_CACHE.md) builds
one the first time a row tests a candidate, copies its damage packet and
disposes it in the same call, through `00460310`: ECX holds the resource
WORD (the high half is left from the process null read at `0048D8BB`; every
reader takes the WORD), EDX a NULL command list, and the stack a pointer to
eight unwritten stack bytes, rotation NULL, placement -1, factory mode 0 and
the process null. `00460120` passes `{&bytes, NULL, &H13 +2, -1}` to the
class's `+0C`, whose `00479D40` copies the first six bytes to `+32..+36`;
nothing reads the other two. The recordings copied `F9A7 FFFF 5469`
(Infest01), `09B8 0000 5469` (Infest02) and `FFC1 FFFF 5469` (Infest03).

| Selector | Construction | Bytes | Final table | `+0C` | Packet |
|---|---|---|---|---|---|
| 25 | `00458250` over `00455560` | `0x164` | `00525B88` | `00458380` | `+68` |
| 26 | `00455560`, `004609A3` | `0x154` | `005268AC` | `004557D0` | `+68` |
| 27 | `00455560`, `00460A02` | `0x160` | `00525848` | `004557D0` | `+68` |
| 28 | `00455560`, `00460A72` | `0x158` | `005259E8` | `00455720` | `+68` |
| 31 | `00485480` over `00485F50` | `0x158` | `00528524` | `00485460` | `+6C` |
| 32 | `00485F50` | `0x150` | `005285F0` | `00485460` | `+6C` |
| 38 | `00455560`, `00460CD8` | `0x164` | `00525914` | `00456840` | `+68` |
| 68 | `00455560`, `0046141F` | `0x158` | `00526398` | `00455720` | `+68` |
| 76 | `00431ED0` over `0047A490` | `0x84` | `00521A40` | `00479DB0` | none |

All but 76 start with the shared `0047A7A0`. `00455560(0x7D0)` stores
`+138 = 1`, `+D8 = 1`, `+13A = 0x7D0`, `+13C = 0` and `+148 = 0x36`; then
26 and 27 zero `+14E/+14F` and set `+150 = 5,000,000` (27 also keeps the
process null at `+154`), 28 and 68 zero `+14C`, BYTE `+150` and `+154`, 38
does what 26 does, zeroes `+15C/+160` and sets `+138 = 0`, `+13A = 0x7D0`,
and 25 zeroes `+14C`, `+150`, `+154`, `+15A`, `+15E` and `+160`, sets
`+15C = +15D = 1` and `+13A = 10,000`. `00485F50` stores `+140 = 600,000`,
`+144 = 0`, `+148 = 6` and `+14C = 1`; selector 31 then zeroes `+150` and
`+154` and sets `+148 = 8`. Selector 76 stores `+70 = 1` and `+74 = 0`.

Each `+0C` runs `+08` (`0047A610`) and the shared payload `0047AC10`, then
reads past the shared `0x42` bytes. `00485440` stores WORD `+44` at `+146`.
`004556F0` copies payload `+44..+4D` to `+13E..+147`. `00455770` (`004557D0`)
adds BYTE `+50` to `+14D` and BYTE `+54` to `+14C`; bit 0 of `+54` adds
`0x1F40` to `+13A`, then bit 2 sets `+148 = 0x37` and bit 1 `+148 = 0x39`.
`00456740` (`00456840`) follows it with `+154 = (DWORD +58 != 0)`,
`+158 = DWORD +5C`, and `+148 = 0x37` when `+154` is zero. `00458330`
(`00458380`) stores WORD `+50` at `+15A` (below 600, signed, it sets
`+15E = 1`) and the zero tests of BYTEs `+52` and `+53` at `+15C/+15D`.
Every actor class finishes with the shared `+1C 0047AAD0`; 76 has the RET
`00401310`. `+18` is `00404610` on the actors and `00401300` on 76.

The cache reads `+6C` first: on 31/32 it is the actor, and `004863D0` copies
BYTE `+148` into the packet's channel before WORD `+146` into its strength.
Otherwise `+68`, the actor on 00455560's family and 25, lets `00458F50` copy
the DWORD at `+13E`. Then `00479FA0` unlinks both flag lists and queues the
object; its actor node stays on `G+48` until the drain. Between that and
the destructor, every recording shows only flag-gated scans of that list
(the aim's `00475EC0`, the sight ray's actor pass `0043B2E0`, `004791F0`
and the pair pass `004790C0`, which test bits 4 or 8 before anything else) and the map-marker pass, whose `+B8`
and `+70` are `00401300` on every one of these tables. No read of the
position. Selector 76 is not on the actor list at all. The destructors are
`004562F0` for 27 (`0047DD40` message 0x12 to its `+88` owner, the null
handle here), `00456640` for 38 (deletes a nonzero `+15C`), `004582D0` for
25 (a nonzero `+160`, then `00456A70`), `00456A70` for 28 and 68 (stops a
nonzero `+154` sound), and the shared `0047AA60` under all of them.

The port builds these classes only through this path, except that the
child factory builds [selector 32](SELECTOR32_RUNTIME.md) for the cannon's
shots, [selector 26](SELECTOR26_RUNTIME.md) for the grenades,
[selector 27](SELECTOR27_RUNTIME.md) for the row-12 charge,
[selector 38](SELECTOR38_RUNTIME.md) for the mines and
[selectors 25 and 28](SELECTOR25_RUNTIME.md) for the missiles, and the
emitter's `00431EF0` builds [selector 76](SELECTOR76_RUNTIME.md), the
flamethrower, through this very path. The authored placement factory and
the child factory keep refusing the others, because their updates
(`+24`), lights (`+28`), draws (`+2C`), selector 25/28/68's `+80`
(`004575E0`), messages beyond the vehicle cleanup's event 10 and damage
reactions (`+B4` over class-specific `+B0/+C0/+C4`) are not ported. Each of
those slots is an explicit boundary; the cache's objects reach none of them.
Ported are the construction above, `+14` (`004556D0` returns BYTE `+138`
for selector 1 on 00455560's family; 31/32 keep `0047A020`), the component
and packet reads, and event 10 of `004557F0` (26, 27, 38), which sets `+14E`
when the sender's `+84` is its `+88`. The position stays unknown; any
reached read of it fails. A cannon shot from the child factory takes the
emission position, and its update, draw and `+98` are ported. So does a
grenade or a mine, whose update, light, draw, `+98`, contacts, lifecycle and
`+B4` (the shared `0047CBC0`) are ported too, with the mine's `+15C` target,
which its destructor deletes.

At `00460310`'s return the recordings' objects had flags `0x4810000B`
(resource 116, selector 32), `0x48100807` (113, selector 28) and
`0x08100003` (130, selector 76), and packets `+148 = 6`, `+146 = 0xC8`
(116) and `+13E = 0x00040A8C` (113). The session test builds all three in
the loaded Level 1 and matches them, then builds every one of the sixteen
resources and destroys it, and runs the vehicle's row 0 and the mode table
`005310A0`'s row through one full cache initialization.

## Selector 11 construction and commands

[`Selector11State`](../../crates/inf-game/src/object_runtime/actor/derived/selector11.rs)
owns `004014F0` above the central derived prefix, with final vtable
`0051F080`. It adds five zero DWORDs at `+188..+198`; defaults
`+1A0=2000`, `+1A2=9`, `+1A4=50`, `+1A6=512`, `+1A8=256`; and zero
WORDs at `+1AE/+1B0/+1B2`. Other fields, including `+184/+19C` and byte
`+1A3`, remain unknown. Its scalar deleting destructor is `00401570`
(`00401590`). Its `+4C`, `00402530`, runs the base `0047AAF0` and then
turns the attached child's basis by the yaw `+18C` (`00491530`) and the
pitch `+188` (`00491290`); its `+C0`, `004025C0`, packs them into one DWORD
for the network (`+18C << 16 | +188`). Payload and admission use the central owners.

Command `00402330` delegates signed codes at most 18 to `00426030`.
Code 19 reads a DWORD boolean controlling low-byte bit 2 at `+1AE`.
Code 20 copies exactly ten bytes to `+1A0..+1A9`, including the previously
unwritten byte `+1A3`. Code 21 reads its DWORD only when behavior `+140`
exists, writes behavior `+04`, preserves `+08`, and sets byte `+0C=1`.
Codes 22/23 read WORD booleans controlling low-byte bits 4/16 at `+1AE`.
Other codes reject without operand reads. Postinit `00402590` reselects
behavior before shared `0047AAD0`. Network `+40=004025B0` runs shared
`0047E2E0`, then ORs dirty WORD `0200`, even when the shared callback
returned early. Its after-load `+34=00401310` is a genuine no-op; the
update is described below.

The [native receipt](../../crates/inf-game/tests/fixtures/selector11_native.json)
contains eight complete runs and 144 compared snapshots, including original
constructor, payload, nonallocating guard, commands, postinit and destructor.
The null behavior branch of command 21 is included; allocating guards and
network completion have separate host tests and static authority. SHA-256:
`D597E2AE70DEE29C9DCABE82DD262D11488D22BFABD90A8D20B43E2F81717857`.

A separate [seeded command-21 receipt](../../crates/inf-game/tests/fixtures/selector11_command21_seeded_native.json)
checks five nonnull behavior cases with supplied storage. It executes the
original command, not construction or allocation. SHA-256:
`D3F54CBE766098254A9EE4A1B75A232BACDA2C22753552DF0665E2B9D72C4479`.

### Selector-11 retirement terrain effects

Virtual +AC is `00402250`, not an inherited empty callback. Its first
non-alternate call enters world `004887D0` with the actor's live ten-byte
+1A0 block, position WORD allocation, original source handle and replication
byte one. That world call executes terrain/furniture `00436390`, then actor
manager `00479870`, then event 0x13 only when the manager result and replication
byte are nonzero. The manager reads M+40/G+48, the existing actor-list header;
it does not introduce a separate base-object list.

The [selector-11 +AC owner](../../crates/inf-game/src/object_runtime/actor/derived/selector11/retirement.rs)
and generalized [world-radius dispatcher](../../crates/inf-game/src/world_radius_effect/dispatch.rs)
are composed by the [production host](../../crates/inf-game/src/session_platform/intro/retirement.rs).
It executes the first radius call, snapshots current XYZ for the process
particle burst, rereads the alternate flag for EXE block `00531250`, then
publishes +1B2=2000, scatters terrain and records placement death. Required
services fail after retaining preceding writes. Thirteen original-bytecode
[cases](../../crates/inf-game/tests/fixtures/selector11_retirement_native.json)
verify the source kernel with supplied callbacks, including live rereads and
failure prefixes. The [production tests](../../crates/inf-game/src/session_platform/intro/runtime/tests.rs)
complete both initial authored actors through real process owners and verify
the missing-task prefix before particle admission. Neither establishes matched
cinematic acceptance.

Both blasts reach every actor in range through that actor's own +B4: the
manager hands each target to its class's reaction. In the world, the
production host lends the retirement the world's damage owners, so a dying
turret's blast damages a family actor, the ACAM or any other class through
its own reaction ([`RetirementWorldOwners`](../../crates/inf-game/src/session_platform/intro/retirement.rs)).
It used to send every target through selector 11's +B4 alone and refuse the
rest. A turret dying beside any other actor (the ACAM, a neighbouring gun
turret) then stopped part-way and never finished dying, and neither did a
dying actor whose own blast had killed it. On Oolbryte this left gun turrets
standing at no health. The intro's sequence lends no world owners and keeps
the selector-11-only damage.
[A test](../../crates/inf-game/src/session_platform/particles/damage/tests.rs)
kills a Level 1 turret with a family actor beside it: the turret reaches
state 1 and the neighbour takes the blast.

The shared arithmetic owner
[world_radius_effect.rs](../../crates/inf-game/src/world_radius_effect.rs)
implements `0042A800` and signed-WORD distance `00462DA0`. Strength is signed
WORD +0; +2/+3 are copied raw bytes; inner radius +4, falloff outer radius +6
and impulse +8 are unsigned WORDs. The outer comparison is strict. Falloff
uses integer Q31 quantization; optional impulse WORDs use signed, truncating
division with a zero-distance divisor of one. Without an impulse destination,
low AL is true only for nonzero resulting strength. Byte +3 can remain unknown
in the data shape until authored command 20 supplies it.

The canonical mission-22 corpus test
`mission22_sequential_actors_keep_unwritten_byte_1a3_without_command20` in
[world_radius_effect_tests.rs](../../crates/inf-game/src/world_radius_effect_tests.rs)
checks the actual H14 placement references 30 and 31 in `32.ovl`. Both have
H13 argument `FFFF000B` and exactly commands `[2, 6]`; neither supplies command
20. Their known +1A0 block fields therefore retain constructor defaults, while
+1A3 remains `None`. The fixture also checks the one-based lookup against each
zero-based constructor placement index. This settles the current retirement
actors' missing byte. It stays opaque while copying the effect DWORD;
`0042A8F0` reads signed strength +0 and channel +2, so furniture damage does
not require +1A3's value. Later consumers must establish any use independently
rather than silently replace the allocation byte with zero.

[TerrainRuntime's effects owner](../../crates/inf-game/src/terrain_runtime/effects.rs)
implements the full `00436390` X-outer/Z-inner square walk using **signed**
WORD +6 and the retained process spacing. The initial radius controls the
walk; later falloff calls reread the live block. It aligns wrapped coordinates
for the H12 flag-4 gate and `00435260` sparse lookup, then requires H11 slot 0
lookup and H8 resolution. Model WORD +0E participates in signed, wrapping Y
comparisons above the average of four signed terrain heights. A proven native
null model clears only sparse record bytes +2/+3. A missing portable resource
owner must return an error rather than impersonating that null-model branch.
The [resident resolver](../../crates/inf-game/src/world_furniture_effect/resident.rs)
now supplies actual H11/0 and H8 custody. H8 vtable `005276D8` starts with
`004EFBF0`, whose `XOR EAX,EAX; RET 4` is a genuine null loader. Signed negative
slots and indices strictly above the native count also return null; equality
retains the explicit unsafe table-boundary error.
The [furniture damage owner](../../crates/inf-game/src/world_furniture_effect.rs)
implements `00436020`: network state rejects before H12/H11 resolution;
nonzero sparse lookup resolves H11 slot 0 before the `FD/FE/FF` rejection.
The H11 second WORD's low byte selects a 16-byte EXE descriptor at `00532F88`.
Descriptor +0 points to four-byte `{unsigned threshold, unsigned multiplier}`
channel records consumed by `0042A8F0`; +4 is a signed acceptance threshold;
+8 is the optional effect program. Negative signed strength survives, while
positive strength above its resistance threshold uses a logical DWORD product
shift by eight. A nonzero generic modifier selects product bits 31..62.
Furniture passes modifier zero.

Positive damage at or above the descriptor threshold takes no RNG draw.
Smaller positive damage executes one shared `0048F060` draw and compares its
low WORD, inclusively and unsigned, against signed-IDIV's quotient after the
DWORD-wrapping `damage * 65535` numerator. This preserves the native negative
quotient/unsigned-comparison branch rather than clamping it to probability.
The following H12 search writes no sparse bytes and does not gate acceptance.
Original file-backed resistance tables resolve from the executable. The
forty zero bytes at `00546E38` lie inside the actual `.data` raw span
`0052E000..00547000`; they are original file data, not synthesized virtual
zero-fill. The corpus test pins all ten zero channel entries and type 2's
actual descriptor. This establishes initial table contents only; a recovered
later process writer would need separate retained mutation custody. Genuinely
unmapped addresses remain errors rather than triggering a speculative fallback.

The same module owns `00436A60`'s pending queue: duplicates compare aligned
X/Z packed into DWORD +10 before allocation, while WORDs +8/+C retain the
original unaligned X/Z and +A receives interpolated terrain Y. Program +18,
zero age +14, zero removal flag +1C and zero remaining timer +20 are retained
in native append order. Damage still returns one when queueing finds a
duplicate. Its explicit clear corresponds to `00436B80`; enqueue never claims
that a program callback ran or that furniture was removed. Native allocation
failure/error-object dispatch is represented by a feature error, not a
successful empty callback. Tests cover the source arithmetic/gates and actual
original EXE rules in
[world_furniture_effect/tests.rs](../../crates/inf-game/src/world_furniture_effect/tests.rs).
The pending queue's `00436700` executor and its remaining mutable service
boundaries are owned by
[TERRAIN_RUNTIME.md](TERRAIN_RUNTIME.md#pending-furniture-effect-executor).

The [actor-radius owner](../../crates/inf-game/src/world_actor_radius_effect.rs)
implements the complete manager scan `00479870` with a required, complete
`0047C6E0` damage service. It caches unsigned outer-radius squared once, then
walks the live actor nodes from M+40/G+48. Flag 4 enables each candidate;
wrapped signed-WORD XYZ differences produce a DWORD-wrapping square compared
unsigned and strictly against that cached radius. This call invokes
`0048EFC0` **directly** on the signed square and sign-extends low AX: it does
not use `00462DA0`'s unsigned-square divide-by-four recovery. The live block
and source position are reread for later candidates. The mobile arm supplies
an impulse destination only when flag 8 is clear and unsigned mass +8C is
below `0800`; this preserves the falloff's true-AL zero-strength path.

Current flag `80000000` skips local damage and increments the native return
count; ordinary damage does not increment it. Mobile candidates still receive
impulse after that alternate branch. After local damage the manager rereads
mass, without retesting flag 8. It uses the saved pre-callback WORD difference
and the current basis for `0047A660`/`00490A80` inverse transformation, the
signed direction gates at X/Z zero and Y -100, and `0047A700`'s forward float
transformation. Cross products truncate to signed low WORDs before angular
scaling. Mass below 128 keeps impulse WORDs and angular scale 256; larger
mobile masses use signed linear division and angular factor `32768 / mass`.
Only that larger-mass arm queries current network state/settings: role 2 uses
800, role 3 uses 64, otherwise 256, with the native second state/settings read.
`0047B510` publishes linear WORD additions before `0047B470` publishes angular
DWORD additions. Neither helper changes dirty flags.

The scan follows the current node after required callbacks, so appended actors
and removal of earlier nodes remain visible in native order. Destruction of
the current node reports an explicit invalidated-node boundary rather than
fabricating the successor of a native freed allocation. Host failure retains
all reached arena/RNG effects and prior impulses. Focused tests in
[world_actor_radius_effect/tests.rs](../../crates/inf-game/src/world_actor_radius_effect/tests.rs)
cover the gates, signed length asymmetry, impulse quantization, callback
mutations, repeated network queries and live-list continuation.

The [two-block corpus fixture](../../crates/inf-game/tests/retirement_radius_blocks.rs)
isolates both radius calls at **initial constructor positions**. Mission-22
references 30/31 reach no H11/H8 model or damage-rule call. Fourteen independent
scans of seven selector-11 placements in the currently supported mission-18
arena reach only themselves; reference 93 reaches furniture lookup 51 but its
genuine H8 null result admits no damage or queue record. Shared RNG stays
unchanged across these isolated scans. Moved/scripted positions, unconstructed
actor classes and the intervening particle burst are outside that evidence.

Callback failures retain earlier sparse writes, and subsequent traversal
iterations reread source position/block mutations. Malformed signed-radius
walks that repeat forever fail the feature after retaining their first reached
cycle.

The same mutable owner implements `0047E190`: absolute signed actor-Y versus
interpolated terrain height must be at most 128 before any RNG draw. Native
argument 2 executes three do/while bodies; zero or negative arguments execute
one. Each uses one shared `0048F060` draw for signed X/Z offsets in 64-unit
steps, wraps/aligns them with the current spacing, then clears the selected
cell flags' high nibble while preserving height and low flags. This is terrain
work, not a task setter. Production +AC now composes these kernels with the
current process task/particle/sound owners and the retained furniture queue.
Broader health/source reactions, admitted network delivery, missing program
memory and remaining furniture callbacks retain explicit service errors.

### Selector-11 turret update and retarget

[`tick_selector11`](../../crates/inf-game/src/object_runtime/actor/derived/selector11/update.rs)
owns virtual `+24=004019E0`. After the shared status owner `0047C990`, a
signed WORD Y below `H12 +0 >> 005483BC` retires through `0047CE90` without
returning; `+C4=2` returns; a negative delay `+144` returns, and a positive
one is counted down (floor zero) before physics `0047B1E0(delta,0x1F)` and
return. [Proximity](../../crates/inf-game/src/object_runtime/actor/derived/selector11/update/proximity.rs)
`00401920` then damages the first player's primary actor within 500 units
(`0047C7C0`: strength `(600-d)<<3`, channel 3, this handle, period 2 s) and,
when `0047DD80`'s reread square is at most 250000, calls that actor's `+98`
with code `0x17`. WORD timers `+17A/+1B0` drop by signed `delta>>10` to zero.

A nonzero `+1B2` takes the death branch: above the step it counts down and
runs the status visual `0047DFD0`; a positive expiry writes `FFFE`, creates
child `0x36` at the actor and `min(size/50+2, 6)` children `0x3A` from Y-100
in 50-unit steps; the negative state rotates the basis by a random Xz angle,
ORs dirty `0x100`, selects model `0x46`, refreshes it through `0047A110` and
clears `+74`, `+C8` and flag 1. Without behavior `+140` the update ends.
Otherwise it captures inaccuracy `+04`, runs the weapon tick `0048D030`, then
retargets when `+190` is below the delta (else counts it down) and counts
`+194` down while `+1AE&1`. Without a target, a nonzero `+19C` zeros `+190`,
and `+1AE|=1`, `+19C=0` end the tick; a target refresh `+18` returning zero
counts `+190` down again.

Aim starts from the selected weapon mount `0048D6E0(1)` toward target focus
`+08`. No constructor or command writes the wobble angles `+1AC/+1AA`, so
a turret's first wobble walks them on from allocator residue. In Infest04
the one turret a stun reached (Oolbryte, message `0xD` through `00402420`,
which sets `+17A` to `0x1F40`) held 0 in both each time a stun arrived;
no recording shows a live turret's wobble
(ledger). The
port starts them at 0 (Selector11WobbleAngleApproximation). The unwritten
read used to stop the turret's update for good, so a stunned turret froze
and could no longer die. A running `+17A` adds the folded `0053DF00` sine
of wobble angles `+1AC/+1AA` (each advanced by `RNG*(delta>>4)>>16`) to
X/Z; otherwise behavior byte `+0C` leads by target velocity `+08` over
`(d<<10)/speed`, with the row kind's `0053DBB4` gravity term. Three RNG
draws apply the inaccuracy. The basis copy rotated by yaw `+18C` (Xz) and
pitch `+188` (Yz) maps the offset through `00490970`; each turn is the
`0053FF00` arcsine of the Q31 ratio, adjusted behind the turret, clamped
to `delta>>5` and halved by `+1AE&8`. Pitch is clamped to
`[-0x3FFF, 0x1000]` and `+19C` accumulates the delta. Only an unclamped
turn on both axes reaches the firing gate:
`+1AE&1` and `+1B0` clear, target `+04` different from the team task's
`+08`, and the selected entry with `[5]=0`, `[0]!=0` and `[7]<6000000`
queue `0048D0D0(delta, normalized offset, 0)`.

[Retarget](../../crates/inf-game/src/object_runtime/actor/derived/selector11/update/retarget.rs)
`004015F0` clears `+1AE&8` and releases an old target with `+1AE|=1`. While
`+1B0` is zero, flag 4 selects scanner B `00475F50`, whose filter `004014C0`
needs a live candidate (`+C4` 3) whose virtual `+74` answers: of the
represented tables only the
[falling rock's](CONSTRUCTOR_ASSETS.md#the-timed-spawner-selector-43-and-its-rocks-selector-44)
does (`00404610`). It scores the turret's DWORD `+164` less the root of the
X/Z squared distance (`00427620`), and the strictly best score wins.
Otherwise scanner A
`00476180` walks the G+48 actor list from the mount (virtual `+7C`):
flags `&0xA4=0x24`, a 3D square below signed WORD `+164` squared, and not
itself. Filter A `004013B0` requires `+C4=3`; for owner model `0x41`, an H12
clearance of at least `0x80`, with a vehicle candidate (`+64`) also in state
4 or 5; a false `+54` (only the gate returns itself); and a nonzero relation
from the owner team's task. `00482CE0` gives 2 for team 18, or with the
network flag for a team other than 0, 1 and the task's own. `00482C90`
(team-18 owners) gives 0 for teams 0 and 18, 2 for teams above 1, and 1 for
team 1 unless its `+5C` object has a NULL `+7C`. Team-0 and team-1 owners'
tasks (`004027B0`, `004EFBF0`) relate to nothing. The score is
`((DWORD +164 - root) * relation) >> 2`, so a candidate at the range edge
can score zero. Zero scores form the first list; scored candidates are
tried in list order, and a higher score wins unless a nearer candidate on
either list lies ahead within `(size>>1)*size` of its ray (`00475FF0`).

A find builds a `004825D0` follow target, clears `+1AE&1`, stores the handle
at `+184`, sets `+190=1000000` and, with `+1AE&2`, calls `0047C840`.
Otherwise a kind-8 point target with the process NULL `0054E894` replaces
it at the actor, offset by RNG (X/Z +-0x400, Y +0..0x3FF) unless `+1AE&0x10`
with `+1B0` zero; then `+190=(RNG+0x16E36)<<4` with flag 8, or
`RNG*8+250000` while `+1B0` runs. Kind-8 targets read process vector
`0055F0F8`, which the whole trace never writes (0 writes, 16548 reads), as
the constant zero `PROCESS_VELOCITY_0055F0F8`.

The [segment A receipt](../../crates/inf-game/tests/fixtures/selector11_update_native.json)
runs the original update over 68 supplied actors with eleven named lower
boundaries, including all three death branches, both pitch clamps and the
proximity pair. Its one-second steps leave both turns unclamped, so their
shots expose the exact aim vector after lead, gravity, wobble and
inaccuracy; retarget is outside it. SHA-256:
`81B8C2BE41F2C1508D7265240DC45FAE6A0B01FB63323C35828913B1D139F8F0`.
The [segment B receipt](../../crates/inf-game/tests/fixtures/selector11_retarget_native.json)
runs `004015F0` over 66 supplied worlds whose candidates carry real final
tables and whose team task carries its real relation table; 31 find a
target. SHA-256:
`4EA590F6BA62774A21C442CF54A5C50BACDE2391C184BDF2E76554E72B911F20`.
The [production host](../../crates/inf-game/src/session_platform/world_actor_update/selector11.rs)
supplies children, model refresh, damage and shots. Its radiation notice,
the player's `+98` code `0x17`, reaches [the vehicle's
messages](VEHICLE_DRIVE_RUNTIME.md#the-vehicles-messages-00420be0), which
raise the HUD's "Radiation warning".

A launcher's shot, a SAM's missile or a grenade launcher's grenade, is built
when the late draw drains the firing node, as the launcher's child
(`0047AAF0`). The child starts from the parent's virtual `+48`, which every
shooter's final table binds to `004026B0` (`lea eax,[ecx+0x44]`, the
actor's own basis): selectors 2, 10 and 21, 11, 12, 35 and 36. The
[world's child service](../../crates/inf-game/src/session_platform/world_actor_factory/child.rs)
admitted only selectors 2 and 36, so a launcher's first shot stopped its
node, and the stopped node blocked every later drain: a SAM or grenade
launcher fired once at most. Retail turrets keep firing: over `Infest04.run`
the 636 calls of `0048D0D0` include 264 by selector-11 turrets, up to 97 by
one (ledger). Live, a
VTOL climbing inside world 23's ring of SAMs draws 11 to 30 missiles from
each in a minute, and Oolbryte's grenade launcher at (17303, 19922) throws
30 grenades at a buggy 1000 units off, where before its first shot stopped
the node and nothing followed.

### Shared instantaneous actor damage

[damage.rs](../../crates/inf-game/src/object_runtime/actor/damage.rs) owns
the complete outer `0047C6E0` with required synchronous class, task, source and
statistics services. It reuses `0042A8F0` resistance arithmetic and the current
retained actor +A4 table. The packet keeps signed strength, channel byte and
opaque byte +3; missing +3 is never filled with zero. An out-of-table channel
reports its reached read rather than selecting a different resistance entry.

The owner always invokes target virtual `+B4(damage,packet,source)`, including
zero adjusted damage. After that callback it rereads flags and, when `200000`
is set, ORs `400000` with the native dirty-mask policy. It then caches the
current +B4 classification and reads process task slot `0055F100[class]`.
Native `0047C744` dereferences that task unconditionally: a null slot or
unsigned class at least 19 fails this feature after preserving earlier writes.
When task +8 equals the original source handle, the same class's table slot is
reread before exact `00482C60(source,0)`. That helper captures the current
`0054E894` from the live process owner, independently of the registry's
construction value. An equal source performs no field read/write; otherwise
task +8 is reread and cleared to the captured value only if it still matches.
The class itself is not reread between the two lookups.

Nonzero adjusted damage then resolves the original source handle and calls
nonnull source `+58 → +CC → +108(target)`. Supported derived vtables return
self at +58; the `00428650` family returns self at +CC independently of +22C.
Intro selectors 10/12/36 use +108 `00429C70`, which depends on actual +218 and
target +148 custody. Selector 35 overrides +108 with the empty `00466D40`
(`RET 4`); it must not inherit the family callback. Only after notification
does the owner capture statistics allocation `G+1BC`/mission `W+0C`, resolve
and classify the source again with `0047F750`, classify the current target
with `0047F710`, and record the original adjusted amount through `00480FE0`.
[WorldStatistics](../../crates/inf-game/src/world_statistics.rs) adds that
signed amount as a wrapping DWORD at `(target + 12)*5 + source`, offsets
`F0..150`, in the captured allocation. This is separate from retirement
increments at `8C..EC` and the selector-18 counter at `154`.

The [damage tests](../../crates/inf-game/src/object_runtime/actor/damage_tests.rs)
pin the retail callback addresses and exercise zero/negative damage, task-slot
and field rereads, source invalidation, captured statistics custody and reached
writes on callback failure. Common actors' `0047CBC0` and selector 11's
`004283E0 → 0047CBC0` have the reaction owners below. Mission-22 family actors
enter through the owned [family wrapper](#family-damage-response-and-target-notification)
`00428040`. The [process task storage](#process-task-storage) now owns the
table and source field. The production
[damage adapter](../../crates/inf-game/src/session_platform/particles/damage.rs)
composes actual admitted class reactions and source +108 with shared retaliation,
target selection and lifecycle services; its remaining class/network limits are
listed in [PROJECTILE_COLLISION_RUNTIME.md](PROJECTILE_COLLISION_RUNTIME.md).
StateOne skips live-health reactions according to +C4.

### Shared timed actor damage

[timed.rs](../../crates/inf-game/src/object_runtime/actor/damage/timed.rs) owns
`0047C7C0`. Period zero passes modifier zero to resistance, meaning full damage.
Otherwise signed wrapping absolute values compare elapsed and period. At or
above the period, equal signs produce `7FFFFFFF`, opposite signs `80000000`.
Below it, signed `(elapsed <<31)/period` computes the Q31 modifier. Native
`INT_MIN` absolute-value overflow remains significant; an overflowing IDIV is a
typed error before resistance. A zero ratio returns before reading the actor.
The current +A4 table feeds actual `0042A8F0`, and only nonzero adjusted damage
calls virtual +B4. This owner has no instantaneous damage task/source/statistics
suffix. Its narrow host therefore exposes only the reached class callback.

The live family update adapter uses these same class services for drowning and
preserves the packet's unwritten byte +3. The
[20 ratio receipts](../../crates/inf-game/tests/fixtures/combat_actor_timed_damage_native.json)
authenticate modifier/early-exit/IDIV behavior with supplied lower returns;
[10 integrated receipts](../../crates/inf-game/tests/fixtures/combat_actor_timed_damage_integrated_native.json)
execute actual resistance and stop/supply only +B4. Both execute unchanged native
instructions. Focused production tests distinguish nonlethal timed reactions
from the instantaneous outer suffix and exercise live trail/retirement services.

### Process task storage

[process_tasks.rs](../../crates/inf-game/src/process_tasks.rs) owns the
nineteen task pointers at `0055F100..0055F148` as stable allocation identities
and separate registration slots. These are process module allocations, not
world-owned actor tasks. Main dependency descriptor `005220F0` includes
actor-module retain/release `00478A70/00478A80` at `00522158/0052215C`.
That actor descriptor, `00527AB8`, includes handle module `00461CC0` before
task-module `00482C40/00482C50`. The latter wraps reference-counted module
`0053BDC0`, whose descriptor `00528088` calls constructor `004829A0` only on
the zero-to-one edge and destructor `00482C00` on the one-to-zero edge.
`PortSession` retains one aggregate process dependency acquisition across
world replacement and releases it at shutdown; it does not reconstruct task
source fields on intro-to-Level-1 reload.

`004829A0` requests nineteen `0x18`-byte allocations in order
`0, 1, 18, 2..17`. Each successful `00482950` constructor writes DWORD +4
classification and DWORD +8 from process-null handle `0054E894`, then
publishes the object only if its classification slot is null. Both the null
handle and pointer table begin in BSS zero. Final virtual +0 callbacks are
class 0 `004027B0`, class 1 `004EFBF0`, class 18 `00482C90`, and classes 2..17
`00482CE0`. The first two return zero; the latter two are relation queries
requiring their actor/component/network inputs, not an automatically executed
task simulation. WORDs +0C/+0E/+10/+12/+14 are constructor-unwritten and stay
`None` until the actual reset writer `0045CB70`. That writer clears these
five words on every registered object but preserves DWORD +8.

`00482C60(source,enable)` returns before any receiver read when source is the
process-null handle. Otherwise nonzero enable replaces +8; zero enable
clears +8 only if its current value equals that source. `00482980` clears the
classification slot unconditionally, even when destroying an unregistered
duplicate whose slot points at another object. `00482C00` visits slots in
0..18 order, destroys/frees each registered object and clears that slot; an
unregistered duplicate is outside that walk. The portable owner preserves
these alias semantics and does not synthesize null allocations. Real Rust
allocation failure reports a feature error with its reached construction
prefix rather than pretending that the module completed with unknown owners.

The [focused tests](../../crates/inf-game/src/process_tasks_tests.rs) pin
the original dependency/vtable words and extract all nineteen constructor
arguments from the actual EXE instructions. They also distinguish stable
allocation identity from registration, null-before-dereference, live equality,
counter-reset failure prefixes and source survival across retain/release.
The [relation-query kernels](TASK_RELATIONS.md) now own the four virtual `+0`
bodies, preserving lazy process reads and captured task/component identities.
Reached component queries remain required synchronous services. Counter
consumers, production selection dispatch and task-manager simulation remain
separate owners; task storage does not execute those simulation callbacks.

[ProcessActorDamageHost](../../crates/inf-game/src/process_tasks/damage.rs)
composes the live slots/source fields with the existing W+0C damage matrix.
It requires synchronous real virtual +B4 and source +108 services; those
callbacks receive the live process/mission owners, so replacement before a
later native read is observed. The task allocation identity also contains a
runtime identity, preventing handles from another process from aliasing the
same portable index. Statistics capture retains the existing Box address
after source notification. The intervening category queries have no external
callback; unsupported post-capture replacement is an error rather than a
write to a different allocation.

The canonical
`mission22_retiring_actors_each_self_hit_with_real_task_source_and_damage_statistics`
[test](../../crates/inf-game/src/process_tasks/damage_tests.rs) composes
`00479870 → 0047C6E0 → 004283E0` on both initial sequential actors after the
actual retirement head reaches state 1. Each scan reaches only itself;
channel-9 resistance adjusts 2000 to 1000, which is recorded with source
category 4. Fixed flag 8 skips impulses, and the live-state reaction services
are configured as required errors and are not reached. The test also keeps
the unwritten packet byte opaque and asserts unchanged RNG and task counters.
Additional fixtures exercise a task-slot replacement by +B4 before lookup,
the real guarded clear before a missing-statistics error, and required
shared-reaction/source-notification failures. The current production adapter
uses the same class-specific response and real retained class-2 task owner.
The two-block corpus fixture additionally checks EXE strength 1000 against
authored channel-9 threshold 1000: the unconditional +B4 runs, but the second
call contributes zero damage/statistics. These initial scans do not establish
moving full-retirement behavior or a complete cinematic.

### Shared and derived damage reactions

[damage_reaction.rs](../../crates/inf-game/src/object_runtime/actor/damage_reaction.rs)
owns shared `0047CBC0`, channel-2 status `0047CAE0`, and derived `004283E0`.
The derived prefix rescales a negative signed +144 with wrapping `*-1000`,
clears flags with mask `FB9FFFFF`, and enters shared reaction only for nonzero
damage in lifecycle state 2 or 3. It writes +CA = 55 and derives +C8 from the
signed low WORD of `min(wrapping(damage*512),32767)`, multiplied by `31250`
and arithmetically shifted by 15. Constructor-unwritten +CA remains `None`
until this actual writer is reached.

Shared reaction's flag `80000000` arm requires network event `0x12` for nonzero
damage, then returns. Ordinary offline adjustment runs only with flag `4000`
clear and current network state inactive: difficulty uses signed low AX,
wrapping multiplication and arithmetic shift by eight. A marked source may
add its channel-selected signed player bonus using wrapping multiplication
and signed division by 25. These local adjustments do not replace the outer
kernel's cached resistance-adjusted amount used for statistics.

Channel 2 applies status before rereading lifecycle +C4, then sets local
health damage to zero. Nonpositive status adds twice the damage to nonzero
+CE with WORD wrap; a zero resulting +CE caps unsigned +CC at 500. Positive
status first calls virtual +68 at `0047CAEE`; a nonzero result requires
retirement `0047CE90` before any status write. Otherwise the current +CC/+CE
are reread: existing +CE uses `max(unsigned CC,damage/5)` then adds damage,
while zero +CE initializes +CC to `max(damage/5,100)` and +CE to damage.
Final status writes retain the low WORD.

Only lifecycle 2 or 3 reaches health handling and the current process-null
read at `0047CD05`; earlier status effects remain ordered before that read.
A source unequal to that live `0054E894` value with +7C or
+7D set can pass the signed +80 cooldown comparison against `005471C0`'s
50 Hz animation ticks. One shared RNG draw is followed by a fresh clock read;
the owner publishes `clock + (draw & 127) + 150` before required retaliation
`0047C840`, then rereads health. Negative damage caps the signed, wrapping
DWORD healing result at unsigned maximum health before narrowing to a WORD.
Nonlethal damage writes dirty bit `2000` and subtracts from current health;
lethal damage requires `0047D670(packet,source,1)` and rereads the actor flags
before considering the network tail.

Network notices use source handle **HIGH WORD >> 11**, not its low WORD.
For an unmarked lethal source, virtual +64 at `0047CE45` returns a retained
identity only. The owner then reads the animation clock, source +5A8 at
`0047CE52`, and unsigned wrapping elapsed time. Only elapsed time below 200
reaches source WORD +5A6 at `0047CE62`; null or expired sources do not read
that owner WORD. The local-player byte follows attribution. The death notice
retains the incoming resistance-adjusted damage's low WORD rather than the
later local difficulty/bonus-adjusted amount.

The [reaction tests](../../crates/inf-game/src/object_runtime/actor/damage_reaction_tests.rs)
cover signed/wrapping arithmetic, required-effect failures, clock/RNG order,
callback-mutated status and health, post-lethal flags, owner bits, and reached
recent-source reads. The retaliation and process-task owners are described
here; their composition, lifecycle callbacks, class-specific target selection,
player/source resolution and network delivery remain required services.
The caller retains the original mutable packet's
identity for remote events and lifecycle callbacks; copied packet data does
not authorize a fabricated native pointer or a successful empty callback.

### Family damage response and target notification

[damage_family.rs](../../crates/inf-game/src/object_runtime/actor/damage_family.rs)
owns complete `00428040` around shared/derived reaction. It ORs byte +22E.1
before `004283E0`, then repeats the negative +144 rescale, `FB9FFFFF` flag
clear and +22E.1 write using the callback's reached live state. The original
resistance-adjusted damage argument remains cached. Packet +2 is a required
read only for zero damage at `004280AB`, and again at `0042811D` after the
target callbacks. Caller-retained packet identity makes intervening mutations
visible; copied initial channel bytes do not stand in for these reads.

Zero damage/channel 1 and self-source skip target response but still reach
the final channel read. A resolved source with the same current +B4 class
instead returns at `004280D1`, skipping that read and the channel-8 suffix.
Channel 8 raises **signed** WORD +17A to 5000 when it is below that value;
an existing high-bit WORD is therefore negative, not a large positive timer.

Embedded target admission `00482E30` uses the existing four entries at +1A4.
Matching handles update unsigned +08 then +04 maxima and return false, even
when those fields increase. A free slot receives handle, +08, +04 and the
incremented original count in that order. Signed-nonpositive counts write
slot zero and increment their original DWORD bits rather than normalize the
count. An oversized positive count fails only at the first out-of-allocation
read; an earlier matching entry remains supported. At capacity, replacement
selects the first strict minimum of
`(08 == 0 ? 0 : max(08 & FFFF0000,10000)) | (04 >> 16)` and requires the
incoming unsigned score to be strictly greater. Equal scores do not replace.

The damage wrapper supplies +08 = `07800000` and +04 = `07800000` for nonzero
damage, otherwise +04 = 1. New admission/replacement invokes actual virtual
+114: base `00428650`/`00429E20` and selector 12 use genuine RET `00401310`;
the `004236D0` family and selectors 34/35 use `0043F3D0`, which calls current
virtual +100 = `00428DB0` with two zero arguments. That callback scores target
tables through `00428BB0` and updates the +1D4/+1DC resources with `00427110`;
it does not clear a target simply because both call arguments are zero. The
shared [selection kernel](FAMILY_TARGET_SELECTION.md) now owns this common
callback, scanner and entry pruning, plus selector-12's separate `004686E0`
and actor-list `00468350` policies through actual class dispatch. Production
service binding and enclosing selector-12 reaction remain required. An existing or unreplaced
target draws the shared RNG once and continues only when its low two bits are
zero. The notification
uses +08 = `03C00000` and the original +04 logically shifted right once.

The same owner implements complete nearby scan `004281B0`: it caches
unsigned `wrapping(+164 * +164) >> 4`, +B4 and resource WORD +30 (`004281D1`,
the resource argument's low WORD, not the placement WORD +2A), then tests each
live actor's class, resource WORD (`0042820F`) and `(flags & A0) == 20`. The
port compared the placement WORD until a seeded replay of Infest01 showed a
hit soldier alerting a squad mate there; because every placement is distinct,
no ally was ever alerted. Signed wrapped
WORD position differences produce a wrapping square and a strict unsigned
radius comparison. Current handles exclude self; proven +58/+CC self returns
reach family +D4 = `004282C0` regardless of lifecycle or active byte +22C.
That callback rescales negative +144, clears the same flags, then mutates the
target table. The scan rereads positions/handles and follows the current live
node after notification; it does not copy an actor-ID vector.

[Focused tests](../../crates/inf-game/src/object_runtime/actor/damage_family_tests.rs)
pin the retail vtables and RET/+100 instructions, table scoring/count quirks,
one-draw cadence, friendly/self asymmetry, callback-mutated actor fields,
both reached packet reads and retained writes on service failure. No +220 or
+230 value is read or initialized. Production target-selection binding, shared
reaction effects and the outer task/source/statistics services remain required for
broader family damage and complete cinematic acceptance.

### Nearby retaliation and family target reaction

[retaliation.rs](../../crates/inf-game/src/object_runtime/actor/retaliation.rs)
owns complete `0047C840` in the arena's proven +58/+CC and family +D4
vtable domain, plus family target reaction `00429340`. The nearby scan caches
victim +B4 and unsigned byte +7C/+7D squared and shifted left 16. These are
radius bytes in units of 256 world WORDs. Their unsigned maximum gates the
scan; the two original radii independently gate target notification and
reaction.

Each live candidate must match the cached class, have `(flags & A0) == 20`
and signed lifecycle +C4 greater than one before position reads. Position
subtraction wraps as WORDs before sign extension; three wrapping DWORD
squares are summed and compared unsigned, with strict radius boundaries.
The current self-handle comparison follows distance calculation. Proven
+58/+CC return self or null independently of +22C; no source-handle
resolution substitutes for these virtual queries.

Inside +7C radius the concrete +D4 owner receives the unchanged original
damage-source handle, +08 = `07800000` and +04 = 0. After notification,
`0047C954` rereads component flags. Inside +7D radius, current flag `1000`
reaches `00429340` with the victim's current +84 handle. Both concrete
callbacks mutate scalar fields/target entries without admitting or unlinking
actor-list nodes; the portable traversal relies only on this proven domain.

`00429340` reads the current process `0054E894`, then caches retained target
+20C. The reached callback requires that explicit process input; skipped
reaction-radius branches do not require it. A nonnull target with zero +210 returns
without RNG or writes, including an identical incoming handle. A different
nonnull target with nonzero timer draws once and accepts only unsigned low
WORD below `0800`. Accepted, null and identical-target cases draw separately
for the timer, then read signed +144 before writing +20C and
`+210 = ((draw & FFFF) + 156250) << 7` microseconds. Negative +144 uses
wrapping `*-1000`; flags use the existing `FB9FFFFF` clear/dirty policy.

The [focused tests](../../crates/inf-game/src/object_runtime/actor/retaliation_tests.rs)
cover exact radius edges, WORD wrap, unsigned square sums, signed lifecycle,
position reads before self rejection, null components, independent radii,
target-table failure prefixes, unequal registry/process-null values and exact
one/two-draw cadence. These are
source-backed owner tests, not a full live-health or cinematic comparison.
Production shared reactions attach this owner through the live damage adapter;
remaining class/network service limits retain their explicit boundaries.

## Placement lookup and selector 60

Manager constructor `00478A90` initializes one embedded actor-list header
at `M+40`, with `M=G+8`. `G+48` is its head's next link and `G+58` its
tail's predecessor. They are **not separate lists**: `0047A7A0` publishes
actor node `+A8` through the latter; `0047F770` scans from the former.
Lookup rejects negative DWORD references, then compares reference minus one
to each sign-extended placement WORD, returning the first published match.
Reference zero can match `FFFF`; disposal does not remove the actor node,
while destruction does. `0047F7B0` returns that actor's `+84` or the explicit
global null handle and does not itself change the handle cache.

The [lookup receipt](../../crates/inf-game/tests/fixtures/actor_placement_lookup_native.json)
checks 74 states across two null-handle configurations, duplicates, disposal,
destruction, negative references and signed-WORD boundaries. It executes
42 complete calls each to `0047F770` and `0047F7B0`. SHA-256:
`592F98D06D0247F495D8064A5D922E08391DE2DE42D74FCD2DB3D327026411B6`.

Selector 60's constructor, mutable command-19 table, real after-load row
allocation and aliased destruction are owned by
[SELECTOR60_RUNTIME.md](SELECTOR60_RUNTIME.md). Row identities belong to
the arena heap and are stored in shared typed pointer slots; they are not
per-actor copies or fabricated native addresses. The spawn/update methods
remain separate boundaries.

The independent [selector-30 branch](SELECTOR30_RUNTIME.md) owns its
`0040D800` prefix, world-start override and completion-bit after-load query.
The [0040D800 family](FAMILY40D800_RUNTIME.md) shares that prefix with
selectors 37 and 39, whose `0043B720` after-load links targets to other
actors through `0047DD40` messages 7 and 8.
The [00428650 family and selectors 34/35](FAMILY428650_RUNTIME.md) own the
home-position guard, terrain commands, retained H13/H14 ratio input and
placement-reference after-load callback. These use explicit world/terrain
inputs and retain their different postinitializers, including selector 34's
signed terrain floor and distinct retained numerator offset.
The [004236D0 family and selectors 10/36](FAMILY4236D0_RUNTIME.md) add owned
probe storage, the distinct common `+218` helper, table selection and
mode postinitialization. Selector 10 also owns its descriptor-selected helper
and nested child allocations. Their resources are released before shared actor
destruction; later movement/collision and behavior transitions remain open.

## Selector 88 construction and retained commands

[`Selector88State`](../../crates/inf-game/src/object_runtime/actor/selector88.rs)
represents constructor `00443890`: call shared `0047A7A0`, install final
vtable `00524AC0`, then zero DWORDs `+13C/+144/+148/+14C/+154/+158`.
The constructor does not initialize `+138/+140/+150`; these remain unknown
in the portable state. The deleting path `004438D0 -> 004438F0` delegates
to shared destruction `0047AA60`. It does not free the retained H14 pointer
or invoke manager cleanup `+30=0047EC50`.

The final class reuses context `+08=0047A610`, payload wrapper
`+0C=0047AD70`, postinit `+1C=0047AAD0` and true guard `+38=00401370`.
Its H13 payload has no extra class-specific reads. Command `00443900`
forwards signed codes at most 15 to `0047ADA0`; code 16 stores the operand
address at `+13C` without reading any operand bytes. Larger codes reject.
[`InstanceCommandAllocation`](../../crates/inf-game/src/object_runtime/actor/selector88.rs)
owns one source allocation and relocation metadata. All placements using
that allocation share its `Arc`; the retained value holds that identity and
an allocation-relative offset. A one-past operand address is valid at this
stage. The factory validates the borrowed suffix's source identity, and
only a reached code 16 requires or retains its owner. Repeating code 16
replaces the reference without copying a record or affecting other actors.

The mode-0 factory supplies allocation position, owner, terrain, rotation,
basis math, current first-player mask, listener owner and network state
explicitly. H13 payload caching keys the current loaded-overlay identity,
region and native payload address (canonical prefix-span start). Aliased
registrations therefore share the response-table owner, while replacing a
resident allocation does not redirect existing actors. Completion
runs the command walk, shared postinit, dirty clear, listener notification,
conditional virtual `+40`, then publishes flag `0x00100000`.

Selector 88's `+40=0047E2E0` differs from the nonactor dirty clear. It first
clears the dirty WORD, returns for a remote actor (`0x80000000`), and gives
a local actor with placement WORD `FFFF` dirty value `0080`. Other local
actors remain clean when `0055F3A0` is nonzero; otherwise it sets dirty
`8000` and requests disposal through `00479FA0`. That world-state input is
separate from `0055F448`, which decides whether the factory calls `+40`.

Later virtual `+20` calls `+34=00443930`. Byte `00535510` is an initialized
`.data` 1 whose only code reference is this read, so in practice only a null
`+13C` requests disposal. Otherwise it clears
`0054ECE4`, binds the first player through `0046AE90`, and rewrites retained
H14 references in place: DWORD `+10` and four count/pointer pairs at
`+14/+18`, `+1C/+20`, `+24/+28`, `+2C/+30`. Counts are signed; nonpositive
counts skip pointer and row reads. Each positive-count row becomes an actor
handle or the explicit null handle. The last array also clears referenced
actors' flag `0x20` and calls `00479F20(false)`. Loader `0048DE64` sets
`0054ECE4` to 1 before placements; a selector-12 routine reads it with `+29D`.
`initialize_selector88_after_load` implements this order, with relocation-checked
array pointers, when the loader supplies camera-factory inputs; without them
`ActorAfterLoadBoundary` remains the explicit frontier.

Reference lookup `0047F770` uses the shared actor list described above.
The four header references at `+00/+04/+08/+0C` remain authored route
references for later `00404F00` lookups. After-load therefore requires a
shared mutable runtime allocation and real actor/player/list owners; an
independent copy per retained command would lose native aliasing.

Update `+24=00444930` now has a retained-script kernel and an arena-backed
effect adapter in [INTRO_SEQUENCE.md](INTRO_SEQUENCE.md). Production now
connects that callback, canonical selector-90 helpers and selector-16 camera
pose through the native update-list ordering. Initial allocator +140 uses
the named `FreshIntroDeadlineApproximation`; construction still leaves
allocator words unknown and arbitrary resume is rejected. Referenced actor
tasks and broader class-specific retirement/world-effect callbacks remain logged
feature boundaries; selector-11 StateOne is now composed as described above.
This connection does not establish cinematic acceptance. The
first-world selector-88 placement is index 117, H13 global slot 143, with
one code-16 pointer to H14 main-allocation offset 6524. The combined supported
classes construct all 118 placements; they do not admit the complete world or
its later loader passes.

The [selector-88 receipt](../../crates/inf-game/tests/fixtures/selector88_initialization_native.json)
contains 16 cases and 48 snapshots, with 16 complete entries each into
identity startup `00439630`, constructor `00443890`, listener startup
`00460010` and initialization `00460120`, plus 32 genuine owner-table
initialization tails. The [comparison](../../crates/inf-game/src/object_runtime/selector88_native_tests.rs)
checks 43 snapshots: all constructor/input states and all 11 supported
empty-list mode-0 initialization cases. Native mode-1/2, already-initialized
and two nonempty-list endpoints remain outside that full host comparison.
The latter execute actual `005279DC -> 00477350 -> 00477210` callbacks;
their reversed order changes the actor's `+6C` as recorded. Host tests require
`UnsupportedInterface` for those callback bodies, not equivalent success.
Four-byte code-16 commands end at an unmapped page, proving pointer retention
without an operand read. Unknown extension bytes retain supplied poison.
There are no patched routines or substitute callbacks. Heap allocation/free,
full `00460310/004603B0`, deleting failure paths, listener constructors and
registration, after-load and updates are excluded. Independent regeneration
is byte-identical; SHA-256:
`DDA4C02763E34CB985DB53E56B180E5C1F9694B94C0C61EE412FF0152BCB81BB`.

## Creation-listener owner and callback boundary

[`CreationListeners`](../../crates/inf-game/src/object_runtime/creation_listeners.rs)
owns the ordered bindings and temporary moving cursors for
`00460120/00461BE0/00461C00`. Before each callback, its null-payload cursor
moves past the selected binding. Removal of current or future bindings,
appending bindings and nested notifications therefore affect traversal
through current links. Bindings carry an owner ID and concrete interface;
the owner must unregister them before destroying that interface. Global
cleanup removes bindings without destroying their actors.

`CreationListenerEffects` requires real dispatch whenever a binding is
reached. Selector 18's `00477350` is the
[Force Field's](SELECTOR18_RUNTIME.md#marks-and-switches) mark; selector
78's `00407350` is not implemented, and no successful default replaces it. Tests of
the list protocol use explicit test effects; they do not establish those
class-specific behaviors. On a host callback error, cursor cleanup retains
earlier callback effects rather than pretending to roll them back.

The accepted original-recording query `20260927-073042-401-CreationListenerCensus`
covers all 118 first-load placement factories and 30 actor listener walks.
Every actor walk enters with the empty sentinel at `0054E874`, next zero,
previous `0054E868`, and null payload; no callback invocation occurs in
those bounded walks. The capture ledger
owns the exact query and guards. This proves the empty branch for that
recorded load, not universal absence of listeners or later callback behavior.

## Constructor and terrain boundary

`0047A7A0` calls the rotating visual constructor, initializes its known actor
fields and installs vtable `00527C00`. It publishes embedded node `+A8`
through the list owner at `[0055F160]+58`, allocates a handle using owner byte
`0055F2F4`, stores it at `+84`, and copies `0054E894` to `+88`. It ORs common
flags with `0x124`, including the common low-nine-bit dirty policy.

The constructor then resolves H12 slot zero and classifies position against
terrain, potentially ORing `0x20000000` or `0x40000000`. It reads the WORDs at
object `+32/+34/+36` before `00460120` supplies the placement context. Neither
the common nor rotating visual constructor initializes those WORDs. The
dispatcher uses `00511708` and the CRT allocation path `0051247C`; its ordinary
heap allocation at `005124ED` passes flags zero rather than requesting cleared
memory. A zero-filled position is not a supported default for this read.

The bounded recording observation
at `417231:2054..417231:226B` confirms this distinction for the first observed
shared actor constructor: its input allocation already contains position
WORDs `[0,1,0]`, which survive the constructor. It returns flags `0x48000127`,
handle `0x08FF0001` and an identity basis. The position is allocation state,
not an authored placement.

The accepted placement census extends that observation to all 118 direct
factories and all 30 shared actors in the same first-world load. It records
constructor entry, exit site `0047AA28`, and each factory return at
`0048DF3A`; these are individual endpoints, not one simultaneous world
snapshot. Seven allocations have nonzero position WORDs (placement indexes
3, 46, 47, 49, 51, 80 and 82). All 30 constructors preserve their input
position, produce flags `0x48000127`, dirty WORD 2 and consecutive owner-1
handles from `08FF0001` through `08E20001`. Every factory endpoint has the
supplied placement XYZ, the corresponding derived vtable, dirty WORD zero,
the same handle and terrain bits `0x40000000`. The census stops at route
finalizer entry `417258:1C86`, before later loader passes. Neither observation
defines a cross-platform policy for uninitialized allocation bytes. The
bounded option ranking for unblocking phase-3 admission preferred replaying
recorded bytes, but the census evidence points the cheaper way for this
shared path: all 30 endpoints retain `0x40000000` regardless of zero or
small-magnitude nonzero inputs, and the port classifier returns
`Set40000000` across its probed boundaries, so later passes consume
identically-derived flags. Closing it needs (1) a unit check feeding the 7
recorded nonzero positions through the port classifier under first-world
terrain against the census flags, and (2) an audit confirming no later
consumer reads the raw leftover bytes themselves — known consumers (the
`+20` pass, movement `00419C1D`/`0041A1F1`) read derived flags. Heap
replication is rejected: the port cannot reproduce Windows-heap leftover
values, only their evidenced effects.

Later initialization does not establish a universal replacement for these
bits. Shared payload setter `00479D70` preserves the entire high flag byte.
The observed intermediate constructor `0040D800`, payload chain
`0040D950 -> 0047AC10`, and post-initializer `0040E000 -> 0047AAD0` do not
clear `0x60000000`. Manager `004791F0` does recalculate those flags for an
eligible subset: it skips flag `0x8`, requires `(flags & 0x04000100) == 0x100`,
uses the current model radius instead of the constructor's fixed 32, and
adds terrain predicate `0047F640`. This is not the constructor operation
moved to a later point.

Manager `00478F20` invokes virtual `+24/+3C` and controller update `0046A260`
before calling `004791F0`.
There are meaningful consumers in movement routine `004195F0`: `00419C1D`
uses bit `0x20000000` to halve an update argument, and `0041A1F1` uses
`0x40000000` to select a terrain-response path. These static consumers do
not prove that the recorded intermediate class reaches either branch, but
they rule out assuming all actors discard the constructor flags before use.

Destructor `0047AA60` unregisters handle `+84`, unlinks node `+A8`, then runs
common destruction `00479D00`. Deleting wrapper `0047AA40` additionally frees
the allocation. Other derived constructor/payload and creation-listener
effects remain prerequisites for admitting their selectors through the
world factory. Later loader callbacks separately gate a complete world;
the shared constructor alone does not satisfy either boundary.

Shared lifecycle `0047D670` now has an arena owner in
[state_change.rs](../../crates/inf-game/src/object_runtime/actor/state_change.rs).
Actor +C4 is a signed lifecycle state, separate from +B4 or the final class.
The loop publishes each state before its required virtual callbacks and rereads
callback-mutated +C4; a failed callback retains reached writes. Retirement
`0047CE90` accepts only states 2/3 and targets state 1 (health zero and virtual
+AC), rather than activating a task. Its explicit process-null input comes from
current `0054E894`, independently of the registry's null handle; unavailable
storage fails only after the states-2/3 guard. Nonretiring states need no source.
Attribution has an arena-backed classifier
and a retained-record owner in
[world_statistics.rs](../../crates/inf-game/src/world_statistics.rs).
World mission W starts at G+1B0, so **W+0C and G+1BC are the same pointer**:
constructor `0048D9F0` allocates 0x158 bytes and `00480FC0`/`00480FD0` clear
them; loader `0048DC30` clears that same allocation again. The owner borrows
`WorldMission::record_0c`, preserving unrelated statistics bytes.

`0047D670` captures G+1BC before calling source classifier `0047F750`, then
target classifier `0047F710`, then counter writer `00481010`. Flags bit 0x10
has priority and returns category 0. Otherwise +B4 values 18, other, 1 and 0
return categories 1, 2, 3 and 4 respectively; an unresolved or null source
handle also returns category 4, independently of the target. The target-major
5-by-5 DWORD matrix starts at byte +8C: counter offset is
`((target_category + 7) * 5 + source_category) * 4`. Its increments wrap at
32 bits. The separate selector-18 count is captured owner +154; the lifecycle
head rereads the target's +B4 before incrementing it. Class-specific death
callbacks remain required host effects. The
[intro authority](INTRO_SEQUENCE.md#timer-and-stage-policy) owns the sequencer
use of this common head and its acceptance limits.

Placement death bookkeeping `0048EAD0` is implemented by
`WorldMission::record_placement_death`, with explicit live inhibit byte W+08
and force-byte inputs. It sets a bit in **current world +A0**'s four +240
DWORDs, independently of saved selection +30. Index -1 returns immediately;
signed division by 32 truncates toward zero, so -31..-2 reach word 0, while
unsigned quotients at least four skip the write. This is the set operation;
`0048EB30` is the distinct clear operation. The public state-change host gets
the signed actor +78 byte and must preserve the native zero force argument.
`WorldMission::inhibit_byte_08` retains this field: constructor `0048D9F0`
writes zero at `0048DA1A`. Manager cleanup `00478CD0` sets G+1B8 (W+08) to
one before its callback/destructor walk and resets it to zero only on normal
return. The [retained offline loader](WORLD_LOAD_LIFETIME.md#borrowed-loadworld)
calls the actual manager after map publication; a stopped walk retains inhibit
1 and preceding mutations. Explicit `CleanImport` fixtures keep their separate
clean-arena admission. Production retirement reads the retained byte rather
than supplying a constant.

## First-world completion and loader callbacks

Canonical leaf-32 H13/H14 resolution and the accepted factory endpoints agree
on the following actor inventory. The other 88 placements use nonactor
selectors 19, 51 and 86. Addresses in this table are hexadecimal.

| Selector | Count | Constructor | Postinit `+1C` | After-load `+34` |
| --- | --- | --- | --- | --- |
| 10 | 2 | `00425190` | `004252B0` | `004277E0` |
| 11 | 8 | `004014F0` | `00402590` | `00401310` |
| 12 | 6 | `00467070` | `00468040` | `004277E0` |
| 30 | 3 | `0048B170` | `0048C4F0` | `0048B230` |
| 34 | 2 | `00433B20` | `00434FA0` | `004277E0` |
| 35 | 3 | `0043D070` | `0043F760` | `004277E0` |
| 36 | 3 | `00481110` | `00481E80` | `004277E0` |
| 60 | 1 | `00480800` | `0047AAD0` | `00480BB0` |
| 77 | 1 | `00407410` | `00408090` | `00401310` |
| 88 | 1 | `00443890` | `0047AAD0` | `00443930` |

All reach shared constructor `0047A7A0`. Selector 30 uses intermediate
`0040D800`; 11/60/77 use `00425E60`; 34/35 use
`004293D0 -> 00428650 -> 00425E60`; 12 uses
`00429E20 -> 00428650 -> 00425E60`; 10/36 prepend `004236D0` to that
last chain; 88 calls the shared constructor directly. Placement 3 is the
first actor, selector 30 / H13 slot 15, correlating the earlier observed
return to `0040D808` with a complete derived factory.

Completion `004602D0` executes instance commands, calls virtual `+1C` and
clears the dirty WORD. The actual postinitializers can create helpers, fix
health/animation fields and change position, but do not reclassify terrain
bits. In particular, selector 12 calls `00479DD0` to set terrain Y, and
selector 34 can raise Y using `00450A30`; neither replaces `0x60000000`.

The following loader order is static authority, not execution covered by
the placement census. `0048DC30` zeroes world `+3D0` (`[G+1B0]+3D0`) just
before its placement loop. When network flag `0055F448` is clear, each
constructed actor then receives `0047F6C0`: a nonnegative placement word
`+2A` stores the counter's low byte in `+78` and post-increments it
(`0048EB90`), then `0048EA80` tests that signed index in the selected
world's completion words; a set bit calls virtual `+54` and disposes the
actor when it returns zero. After all placements the loader calls route
finalizer `00404D40`, then disposal drain `00479840` over `M+10` with
world-owner `+8` set to one only around that call. With `G = [0055F160]`,
two inline loops then traverse the current flag-0 list `[G+10]` and the
parked list `G+30`. Each object lacking flag `0x00800000` receives virtual
`+20`. Each loop compares its cached next node with the link after the
callback and restarts at the head if they differ.
Only afterward does the loader perform player creation/binding through
`0046A890`/`0046AE90` and finish owner setup. It does not call `004791F0`.
The [loader pass owner](../../crates/inf-game/src/object_runtime/world_load.rs)
implements `0047F6C0` and this tail. Virtual `+54` follows the final tables:
selector 30's `005289C0` binds `00404610`, which returns the gate, so a
completed gate stays. Every other represented actor table binds `00401300`,
which returns zero. Those are the shared `00527C00`, derived base `00520E58`
and selectors 2, 10, 11, 12, 16, 34, 35, 36, 60, 66, 77, 88, 90 and 93. A
completed placement is therefore queued through `00479FA0`. The retained-manager
load passes its mission and live grants to the drain, so each such actor's
`+30` runs its class cleanup before its destructor. Its placement death
(`0047EC50`'s `0048EAD0`) records nothing: the world owner's `+8`, set
around the drain (`0048DFDB..0048DFE4`), is the mission's inhibit byte. So
an actor that a placement condition removes at load (commands 13 and 14,
whose `00479FA0` runs inside construction, before `0047F6C0` gives it a
serial) is not counted as finished, and stands on a later visit once the
condition allows it. The port had left the byte clear and marked such
placements finished: Exxodus's disabled turrets (H14 placements 170 to 172,
serials 31 to 33, in state 2 with command 13 on 0x17) were marked by the
first visit, so the return from Altair Prime, whose hangars give 0x17, found
neither set (reported 2026-10-17). In the lab a first visit leaves
world 4's finished bits (`G+430`) clear (tampered-run:
`runtime_re/lab/scenarios/exxodus-fresh.json` (a
constructed save at Exxodus's first start)), and the return from a profile whose
bits 31 to 33 are clear has the three standing in state 2; the selector-11
update returns at state 2 (`004019E0`), so they never fire. A load without
those owners keeps the strict actor `+30` boundary. Loading the
imported Artoria save (selection 11) completes this pass with completed
placements disposed. Offline construction of the first world serializes its
30 actors as `+78 = 0..29`. With the offline player present, both passes
complete like the recorded passes:
76 flag-0 objects, then the parked route nodes, selector 88, the parked
selector-35 placement `0x50` and the camera that selector 88's binding created,
119 objects in all. Selector 88's rewritten `+10` and four arrays equal the
recorded handles, and the last array's two actors lose flag `0x20` and flag-1
membership.

## Player attachment and world camera

`00487410` constructs the offline player through `00469D60` (0x1A0 bytes,
vtable `005271D8`), appending node `+C8` to manager list `M+58`; world
cleanup does not drain it. The recorded player embeds grant owner `00527818`
at `+9C` with mask `+A0` and the offline closure, and has null handles `+D4/+D8`.
`00469E6A` constructs the message subobject at player `+88`; `0041314C`
places the grant owner at message `+14`, passing the message itself as its
notification recipient. Player `+B8` stores that message pointer.
`00488AA0` returns the first player; `0046AE60` resolves `+D8`, then `+D4`.
The player table's entry at `005271F0`, `0046D360`, copies the published
view basis (`0055F270..0055F298`) to `0054ED24..0054ED44`.

`0046AE90(player, handle)` stores an unchanged or global-null handle directly.
Otherwise it resolves the actor and calls its virtual `+88(player+4)`;
selector 88's `00444AB0` returns true and camera `0040F880` clears its
`+180` and returns true, while derived `00405750`/`00402720` remain boundaries. It then stores `+D8`, obtains the
camera through `00478E00` and, unless the camera is that actor, installs a
new follow target through `00410E70`.

`00478E00` resolves `M+74` and returns that actor's `+60` (the camera's
`00404610` returns itself; `00401300` elsewhere returns null). With no live
camera it constructs H13 argument 1 (selector 16) with null position,
rotation and placement `-1`, publishes its handle in `M+74`, sets camera
byte `+187`, and follows the first player's actor. With none it starts the
camera's search `0040FB10`: `+186` = 1, view mode 0 (`0040F800`), `+188` = 3,
then `0040FB40` scores every actor with flag 1 (one RNG draw each once `+60`
and `+B8` return null), releases the old target and follows the best, and
`+19C` restarts. A load reaches it in the respawn's spawn tail, with the
player detached: 31 draws in Level 1's. The port's `world_camera` takes the
world RNG for it (`world_camera_with_rng`); a caller that lends none fails at
the search.

Selector 16 (`0040F590`, vtable `005204D0`) initializes parameter blocks at
`+138/+15C` with static tables chosen by Camera Style (`+6C == 1`), bytes
`+184..+187 = 0,1,0,0`, `+188 = 3` and target `+18C` null, then applies
`0040F800(First Person low byte)`. `0040F800` asks the target actor's `+80`
(default `0047AAE0`: accept mode 0 only); a refusal retries with 0 for a
nonzero mode or 2 for zero, and `+188` becomes 3 for mode 0 or 2 otherwise.

The [class-aware view-mode owner](../../crates/inf-game/src/object_runtime/actor/selector16.rs)
dispatches the represented vtables explicitly:

| Represented actor class | Native +80 | Reached policy |
|---|---|---|
| Derived base, selector 11, selector 60 and currently modeled derived families | `00427670` | Signed low mode byte <=0 enables flag 0; 1..127 disables it. Then store the original byte at +D0 and return true. |
| Selector 2 | `004231B0` | Store +D0 and return true without list changes. |
| Selectors 77 and 93 | `004074D0` | Return mode-byte ==0 without writes. |
| Shared actor, camera 16 and selectors 30/88/90 | `0047AAE0` | Return mode-byte ==0 without writes. |

`00427670` therefore treats bytes 128..255 as negative, not as positive modes.
It calls the existing `00479E90` flag-0 owner before publishing +D0: remove
and append the node even when the requested flag equals its current value,
and preserve the low-flag dirty-2 rule. The tracked
bytecode checker records all 256 bytes for
each of the four methods against original instructions, including repeated
same-mode list operations. Its [receipt](../../crates/inf-game/tests/fixtures/actor_view_modes_native.json)
pins the class/vtable matrix and is compared by
[focused tests](../../crates/inf-game/src/object_runtime/actor/selector16/view_mode_tests.rs).
This establishes view-mode dispatch and target-replacement prefixes, not the
remaining camera arithmetic, actor tasks or a matched cinematic.

The [shared target owner](TARGET_RUNTIME.md) constructs follow targets
(`004825D0`, vtable `00528038`, 0x20 bytes). The null-point branch used here
copies the live actor's position with unsigned WORD `+74` raised on Y, sets
DWORD `+10 = 2` and duplicates the point. Explicit-point cloning has a
different write set and does not resolve the actor.
`00410E70` first resets an existing target's mode, frees it and writes
`+188 = 3`, then installs the new one and reapplies First Person. The
[player owner](../../crates/inf-game/src/object_runtime/player.rs) and
[camera owner](../../crates/inf-game/src/object_runtime/actor/selector16.rs)
reproduce the recorded camera handle `08E10001`, flags `48100002`, blocks,
`+187/+188` and follow target `08E20001` at `(343D, 00E0, B11E)`. Camera
commands `0040F7D0`, network `+40 = 00410F50` and drawing remain separate
owners. The [camera runtime](CAMERA_RUNTIME.md) owns target refresh, smoothing,
mode-specific pose construction, terrain units, shake/RNG and reached writer
order. The intro production adapter connects the retained terrain/math host,
native reselection, player-camera transfer and live pose submission. Remaining
selector-2 target arithmetic and actor/event writers require explicit hosts;
full cinematic comparison remains open.

After both after-load passes the loader queries the first player again. If
one exists it calls `0046A880` (WORD `+11C = 0`, byte `+58 = 1`). Offline
loads, and network sessions whose Settings `+10` is neither 0 nor 4, then
call `0046A890(player, 1)`, which spawns the player's vehicle through the
mode-1 factory. The other network sessions attach the player to the camera:
camera `+88 = 0040F880` clears `+180` and accepts, and `0046AE90` returns
without retargeting because the camera is the attached actor.
`run_loader_player_setup` owns this branch and returns the respawn request;
In the recorded offline load the player already had `+11C = 0` and
`+58 = 1` before the call.

`0046A890`'s byte argument skips its first check: with the argument clear
(the camera's respawn after a death passes the network flag), a nonzero H14
header `+24` no greater than the signed lost-vehicle count `+11C` ends the
game instead (`G+1D4 = 1`, `G+1C7 = 1`): the camera's respawn host starts
that [doom countdown](FAMILY40D800_RUNTIME.md#timed-generators) and returns
before any player write.
Level 1's `+24` is 0, which never ends it. `0046A890` then detaches through
`0046AE90(null)`. A first-player `+D4` vehicle with `+C4 == 3` receives
retirement `0047CE90`, which the port still reports as a boundary until its
class-specific retirement callbacks are attached to the common lifecycle
head; a dying vehicle (the wreck) is left as it is. It then clears `+50`,
sets
`+12C = 3` when bit 3 of the `+100` bitset is set, stores `+34 = 0048ED60()`
(world byte `+14` as signed, otherwise world-header DWORD `+18`) or `+144`
when that is zero, and `+28 = +148 + 2`, `+2C = +14C + 2`. The constructor
supplies `+144 = 1`, `+148 = 0`, `+14C = 8`, `+12C = 0`, `+ED = +EE = 0` and a
zero bitset. With `+ED` clear the spawn point is `0048DAF0` and heading
`0048DBC0` (rotation `[0, (i16 H1[2*pair+1] << 16) / 360, 0]`); a set `+ED`
uses saved `+E0..+E5` with signed `+34 = +EC` and leaves the rotation
uninitialized. Network sessions then run a scored spawn search (not ported),
`+EE` becomes 1, and the mode-1 factory receives resource argument 0 and
placement `-1`. `prepare_player_spawn` reproduces the recorded first offline
factory inputs: position `(724E, FFFC, B559)` from selector 30's override X/Z
with resampled Y, rotation `(0, C000, 0)`, `+28 = 2`, `+2C = 0x0A`,
`+34 = 1` and null `+D8`.

## Player vehicle (selector 2)

`00460310` takes the resource from ECX, so `0046A890`'s vehicle is H13 global
slot 0, which the first world registers as selector 2; factory mode 1 only
makes `004600D0` call `004625D0`, marking the new `+84` handle and its registry
entry. Selector 2 (`004156E0`, 0x5C4 bytes, vtable `00520860`) extends
`00425E60` with a three-slot handle array at `+1A4` (`00411D60(3)`: vtable
`005206C4`, elements `005206C0` + null handle), scalar defaults through
`+5C0`, `+1B4..+1BF` zeroed with `+1BC = 1`, and byte `+209 = |pre-context Y
- wave height| < 64` (static `0053014C`), where the height is the inlined
`00410F60` wave over `00450A30` at the allocation's leftover X/Z. The network
constructor branch chooses `+1F0` from session settings and is not ported.
Payload initialization is the shared `00426BF0`. Admission `+38 = 0041A620`
keeps an existing `+140`; otherwise it copies 16 records of 44 bytes from
`00530158` into `+26C` and builds `0048CC70(2, 8, 8, +26C)` (groups, entries,
extra records, descriptor), leaving `+04/+08` zero. It then copies maximum
health `+A2` into `+A0`; network sessions scale `+A2` first. Postinit
`004217F0` stores WORD `+8C` in `+184` before `0047AAD0`.
The constructor's zero byte `+1C0` and angle WORDs `+1C2..1C8` are distinct
from unwritten special-view position `+1CA..1CE`, forward `+1D0..1D8` and up
`+1DC..1E4`. The vehicle keeps these stored scalars privately unknown; the
[camera reader](CAMERA_RUNTIME.md#selector-2-special-view-storage-and-reader)
owns their reached reads and partial basis writes, and the free look `00421DC0`
([`free_look.rs`](../../crates/inf-game/src/object_runtime/actor/derived/selector2/free_look.rs)) writes them.

The [vehicle owner](../../crates/inf-game/src/object_runtime/actor/derived/selector2.rs)
reproduces the recorded
first offline spawn from leftover allocation WORDs `(0, 2, 0)`: flags
`48106927`, extent `0x6D`, handle `0CE00001`, health `1388/1388`, `+B4 = 2`,
`+184 = 0xC8`, `+209 = 0`, the handle array, the behavior shape and the
copied descriptors. Commands and destruction's `00487FE0` releases remain
open; the [controlled update and ground drive](VEHICLE_DRIVE_RUNTIME.md) has
its own document. Selector 2's `+80 = 004231B0` stores the camera view mode
in byte `+D0` and always accepts.

After the factory returns, offline `0046A890` obtains the camera
(`00478E00`), takes the vehicle's `+64` (itself), and binds it through
`0046CB50`: flag `0x10` moves from the old `+D4` actor to the new one, with
dirty marking, and `+D4` stores the handle. It then ORs vehicle dirty WORD
`0x02`, sets `+B4 = 2`, and calls `0041A600`, which runs `0046B110` on the
first player: owned upgrade IDs in the player's `+100` bitset patch fields of
the vehicle's `+26C` copy (`95`, `96`, `97`, `9C`, `9D`, `9E`, `9F`, network
`27`, `A0`/`1E`/`A4` at `+B0` in that order, `A2`, `9A`), WORD `+134 = 1C` is
written unconditionally, and each player `+150` value above its `0053A310`
limit is lowered and copied into the first DWORD of the matching `+140`
behavior record. The equipment loop that follows is described under
[carried units](#carried-units). A new follow target on the vehicle replaces the camera's through
`00410E70`, and camera `+187` is set. `finish_player_spawn` owns this span;
the `0046B2B0` grants that follow remain open. The
recorded wiring
matches: `+D4 = 0CE00001`, vehicle flags `48106937`, dirty `2`, `+B4 = 2`,
player `+150` lowered to `-1,-1,-1,-1,0,0,-1,-1,0...` from limits
`-1,-1,-1,-1,8,8,-1,-1,8...`, identical behavior record heads, and a camera
target on `0CE00001` at `(724E, 0009, B559)`.

### Carried units

The equipment loop rereads each player WORD `+190..+194`. A nonzero item
creates a child through the vehicle's `+4C` (`0047AAF0`) with a NULL
position. `0046AD11` loads the WORD with `mov ax`, so the argument's high half
is register residue and only its LOWWORD is admitted
(`ActorChildResource::RegisterLowWord`). When the out slot names an actor,
the loop notes item 2 and attaches the actor through `00421810`. If no item 2
was carried, world `0x13` then creates and attaches item 2 as well. Every
imported retail save carries items `(2,2,2)` or `(2,3,2)`. H13 slots 2 and 3
are both selector 12 in `1.ovl`. The
[carry owner](../../crates/inf-game/src/object_runtime/actor/derived/selector2/carry.rs)
implements `00421810`:

- The slot array's `+8` (`004122C0`) refuses an actor it already carries,
  then takes the first slot that holds the process NULL handle. The actor's
  `+8C` captures it, with the slot's `+8` DWORD as its saved flags. The slot
  then stores the handle. The array's `+0C` (`004122A0`) births effect
  `00489A90(2)` at the actor, and slot state `+C` becomes 1 for a remote
  actor, else 2.
- Selector 12's `+8C` is `00468220`. Its capture arm looks up the source's
  player (`0046B090`). If one is found, it merges the actor's grants into the
  player (`00475350`), broadcasts event 22 with zero radius (`0047DAF0`) and
  sets player commands 0 and 1 silently and `0x48` with its message
  (`0046C760`). It then stores "Captured" (`0053A160`) at `+228` and runs the
  shared `0047D7C0`. Its release arm runs in
  [the vehicle's drop](VEHICLE_DRIVE_RUNTIME.md#the-drop-00417747004177f1).
- A carried actor with a nonzero `+B4` joins the vehicle's team through its
  `+5C` view (selector 12 returns itself) and `00468C40`, then copies `+A2`
  to `+A0`. The child already inherits the vehicle's `+B4` from `0047AAF0`,
  so the team change returns at once. A real one sets `+38` bit 1, stores
  the team, empties the actor's `+1A0` table, sets `+22E` bit 0 and drops
  its targets `+1DC`, `+1D4`, `+1EC` and `+1F4` (`00427110` with nothing).
  It then walks the actor list (`G+48`). Each actor of the new team whose
  `+58` component answers `+CC` forgets the carried actor in its own `+1A0`
  table: `00482F90` clears the row holding its handle and prunes the table
  (`00482D50`). Every family-428650 table answers `+CC` with itself
  (`00404610`): the bases `00520F34`, `00521164` and `00520B84` and
  selectors 10, 12, 34, 35 and 36. The other derived tables (`00520E58` and
  selectors 2, 11, 60 and 77) answer NULL (`00401300`).

  Level 1's four loose scientists are team 1, so the vehicle's pickup
  changes their team, as does boarding. In Infest02 (the `teamchange` query) three such takes
  each set `+22E` bit 0 and dropped the `+1EC` target (0x17D784). They then
  walked the team-2 scientists and Level 1's two selector-34 actors, whose
  tables were empty.
- Three `00487EC0` one-shots of sound 2 at the vehicle (gain `0x10000`,
  pitch `0x4000/0x6000/0x8000`), each released through `004923C0`, close the
  attach.

The world loader lends the live child factory, the session's particle pool
and its sound records to this loop. A clean import without them keeps
`RespawnEquipmentBoundary`. Six more callers of `00421810` are gameplay
takes. [The vehicle's pickup](VEHICLE_DRIVE_RUNTIME.md#the-pickup-00417843004178ed)
(`0041789B`), boarding (`004681FC`) and the factory's
[Evacuate](FAMILY40D800_RUNTIME.md#evacuate-0042f780-0040dcb0) (`0040DDA9`)
are ported. `00414A29`, `0042E996` and `00488C31` are not. Each update then
runs [the hold](VEHICLE_DRIVE_RUNTIME.md#the-hold-00412380).

A scientist also boards by touching a vehicle. Selector 12's pair reaction
`00468160` (`+A0`, from the [pair pass](WORLD_CONTACT.md)) first offers
itself to a partner its current target names (the factory's and the gate's
`+D0`, still boundaries). Alive (`+C4` at least 3) and outside team 0x12,
against a vehicle (partner `+64`), it merges its grants into the vehicle's
player and broadcasts event 22 (`00475350`, `0047DAF0`, as the capture
does). It then takes itself into the hold, and the take's result ends the
pair before its physics. In Infest02 (the `boarding` query) a team-1
scientist touched a vehicle already carrying three: the merge and the
broadcast ran, the take refused, and the scientist stayed team 1. The
[carry tests](../../crates/inf-game/src/object_runtime/actor/derived/selector2/carry_tests.rs)
replay that refusal and a boarding with room. The
[loader test](../../crates/inf-game/src/world_loader_tests.rs) loads level 1
with items `(2,3,2)`. The
[session test](../../crates/inf-game/src/session_platform/linked_tests.rs)
loads the retail Terra Nova record through Start Game.

### Respawn pickups and the world loader

`0046A890` then calls pickup routine `0046B2B0(player, id, amount)` with
`(1, 0)`, `(4, 5000)`, the settings-`+84`-bit-`0x40` pair `(0x33, 0)` plus
`004750C0(0x33)`, and offline `(0x32, 0)`. The routine resolves the player's
actor (`0046AE60`) and its `+64` vehicle view (selector 2 returns itself; the
`00401300` classes return null and reject), grants the ID into the player's
grant owner at `+9C` (mask `+A0`, notification pointer `+B8` = player `+88`, so a
newly granted ID with record text posts its own message), refreshes the player
from the vehicle through `0046CBB0` (`+144 = vehicle +1E8`, `+148/+14C` =
signed behavior bytes `+10/+11`, `+150..` = behavior-record heads, `+190..` =
the three item actors' `+30` WORDs), and runs its ID switch. The shared
[vehicle-to-player owner](PLAYER_VEHICLE_STATE.md) now preserves each reached
store and late-error prefix; its private class-only query also serves the
camera without consuming the later special-view byte. Uncased IDs fall
through; IDs 4/0x83 heal up to maximum and, at full health, return 0 without a
message; excess healing reaches `0047CAE0`. The shared tail sets the ID in the
`+100` bitset, reapplies `0041A600` on the vehicle, and for an offline first
pickup whose record WORD `+0` is nonzero builds `004135D0`'s HUD text from
string `0x139`, the record text and, when nonempty, the record text + 1.
`grant_player_pickup` owns uncased IDs, 4/0x83 and the class cases Level 1
reaches: 3 adds the amount to the Turbo charges `+12C`; 7, a crystal, raises
the count `+118` and heals a damaged hull by 100 up to its maximum; 0x0E..0x10
raise the counters `+120/+124/+128` by twice the amount up to 30 (a full
counter takes nothing). Those cases use prefix `0x13A`. It returns the
message, and `004750C0`'s notice for the requested ID when that ID is newly
granted with record text (`%s` alone for record flag bit 0, else category
`0x13B` for bit 3 or `0x13C`, `: ` and the text), both as requests the pickup
draw delivers in native order. Case 2 sets command 0x44; the weapon IDs'
cases, their nested grants and the full-row notice `004693C0` belong to
[WEAPON_PICKUPS.md](WEAPON_PICKUPS.md#weapon-cases). A notice from the
dependent-record rescan remains a boundary; the respawn keeps any notice a
boundary.
[The grant receipt test](../../crates/inf-game/src/object_runtime/actor_factory_tests.rs)
replays the 68 grants Infest01 and Infest02 made on a resolved vehicle (the
respawns, 38 crystals, six `0x0F` pickups, the factory's Turbo charges and the
healing trigger `00440810`) from the captured mask, `+100..+12C` and hull
([pickup_grants_native.json](../../crates/inf-game/tests/fixtures/pickup_grants_native.json)).
Every grant matches in those fields, its hull and result, with the four
first-time notices, the factory's truck grant (2) and Infest02's two grenade
pickups (`0x25`) included.
Recorded offline results: `(1)` returns 1 with "Upgrade Advanced Combat
Assault Module. - You must learn to pilot this craft well to destroy the
enemy", `(4)` returns 0, `(0x32)` returns 1 without a message, and the player
ends with grant word 1 `00040000`, `+100` `00000012,00040000` and the
refreshed `+144/+148/+14C = 0/0/8`.

`respawn_player` composes the offline `0046A890` from these owners in native
order, the load's argument 1 and the death camera's 0 alike: spawn setup, the mode-1 factory call for H13 argument 0 with
placement `-1` and no suffix, the vehicle wiring only when the factory
constructs the vehicle, then pickups `(1, 0)`, `(4, 5000)` and `(0x32, 0)`,
returning the vehicle and the HUD message requests. Settings `+84` bit `0x40`
(the `0x33` pickup and grant) remains a boundary. From the recorded loader
state it yields vehicle `0CE00001` and the single "Upgrade" message.

The [world loader](../../crates/inf-game/src/world_loader.rs) composes the
offline, non-attract `0048DC30` over these owners: mission prelude, header
fields, the `0048DE64` resets, every placement's factory call with `0047F6C0`,
the post-placement passes, the player branch with `respawn_player`, and the
mission tail. Selector 12 reads the live first player's grant mask. W+A6 and
W+14 each belong to two port owners (mission prelude/world-start override and
header byte/vehicle selection), so the driver clears both views. World cleanup
`00478CD0` has a [shared supplied-owner kernel](WORLD_CLEANUP.md); the loader
still requires its caller to provide an arena without world objects.
The first offline load finishes with 118 placements, the camera and vehicle
`0CE00001`, serial 30, every placement after-load flagged, the "Upgrade"
message, mission `+10 = AA` and selector 30's recorded start override.

Every actual first-world vtable uses `00401320` at `+20`: call virtual
`+34`, then set flag `0x00800000`. This callback is distinct from the
admission guard. `00401310` is a no-op; the other after-load implementations
have these bounded effects:

- `004277E0` resolves the retained placement reference at `+20C` into an
  actor handle through `0047F7B0`.
- `0048B230` restores state using world placement metadata and resolves
  another retained placement reference.
- Implemented `00480BB0` constructs helper objects from retained `+190` records and
  sets `0x01000000`, or requests disposal when that input is absent.
- `00443930` resolves retained placement-reference arrays and can disable
  referenced actors through `00479F20`; missing prerequisites request
  disposal. It can mutate list membership during the initialization pass.

These paths do not replace the constructor terrain classification. The
first actor's factory flags `0x4810002F` include `0x8`, which makes the
later `004791F0` skip it unless another path changes that flag. At the 30
individual factory endpoints, only placements 46, 77 and 80 satisfy that
routine's initial flag predicate. This does not establish later eligibility,
which callbacks execute during gameplay, or whether a particular actor
reaches either movement consumer before reclassification.
