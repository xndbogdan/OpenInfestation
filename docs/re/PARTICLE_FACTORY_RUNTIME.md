# Shared particle factories and normal deletion

This document owns `004665D0`, `00466710`, `00466850`, creation notification
`00466010`, packet-source residency, and the corresponding normal deleting
wrappers. [PARTICLE_RUNTIME.md](PARTICLE_RUNTIME.md) owns the process pool,
common construction, existing producer/update/draw attachment and timing.
[PROJECTILE_COLLISION_RUNTIME.md](PROJECTILE_COLLISION_RUNTIME.md) owns the
ordered actor/furniture/terrain probes and cosmetic `00489600` impact birth.
[BEHAVIOR_DAMAGE_CACHE.md](BEHAVIOR_DAMAGE_CACHE.md) owns the `0048D820`
caller and `00466120` packet transfers. These factories are process services;
their selectors do not depend on a mission, terrain leaf or model whitelist.

## Captured requests and live storage

[factories.rs](../../crates/inf-game/src/process_particles/factories.rs)
uses one `ParticleFactoryRequest`: named factory, full selector DWORD, source
ActorHandle, full flags DWORD and captured velocity-pointer mode. The mode is
`Null`, `SamePosition` or `Separate`; equal vector values do not establish
pointer identity. `ParticleFactorySources` holds live six-byte position and
optional separate velocity storage. Each byte has its own known mask.
Available unknown bytes are copied by the native DWORD/WORD operations;
they are not numerical read failures.

`ParticleFactoryContext` holds the mutable raw process network flag,
optional process-null ActorHandle and shared RNG, with stable read-only
`FrontendExecutable` backing. Current phase has separate manager custody:
`ParticleFactoryHost::current_phase` supplies it lazily at the native read.
Creation receives live sources explicitly;
network notification receives those same sources, while deletion and impact
receive only the common context. A deleting invocation has no dummy creation
vector. The pool remains the method receiver and is supplied once to reached
hosts. No factory caches a second pool, RNG, phase or null handle. Missing
source storage differs from available unknown bytes. Numeric consumers require
only the exact spans they read.

`ParticleVectorBytes` also carries common `00464FB0` construction. XYZ copies
four bytes then two bytes; the late nonnull velocity copy uses the same shape.
Null velocity retains destination storage. A missing separate source fails
after the common constructor's reached position, birth fields, publication
and flag-`0100` writes. Same-position unknown bytes clear prior known masks
even when a phase-gated allocation returns the initialized scratch record.
Existing debris, central, trail and splash callers preserve their distinct
authored null/zero policies.

## Selection, notification and construction order

All three factories reject unsupported full DWORD selectors before required
process globals, allocation or parameter storage. High bits are not narrowed.

| Factory | Selectors | Parameter records or local operands | Final class |
| --- | --- | --- | --- |
| `004665D0` | 1, 11, 12 | `00539EC8`, `00539ED8`, `00539EE8` | `00526F20` |
| `00466710` | 2, 3, 4, 5, 6, 7 | `00539F28`, `00539F38`, `00539F48`, `00539EF8`, `00539F08`, `00539F18` | `00526EFC` |
| `00466850` | 8, 9 | Animation `00539E70`; packet `00539EA0`/`00539E9C`; scale `0400`/`0200` | `00526F60` |

After selection, flags bit 0 suppresses notification. Otherwise `00466010`
rereads process-null `0054E894`, even when networking is offline. It combines
factory family 0/1/2 with the raw selector as `(family << 16) | selector`.
The captured source ActorHandle is compared with that freshly read process-null
handle: equality selects event `17`, inequality selects event `18`. This is
independent of captured position/velocity pointer identity. Event `17` leaves
the fourth native DWORD unwritten; event `18` supplies the source handle there.

`00466010 → 00492500` is a **creation-side** notification. Actual offline
`00492500` returns before reading the notice vectors. A nonzero live network
flag requires `ParticleFactoryHost::network_notice`, receiving the captured
notice and mutable pool/context. Encoder `0049255A` and later network
serialization remain required boundaries. Supplied host effects are tested
as effects of that service, without claiming its body executed.

Allocation follows successful notification and rereads the full live network
DWORD. Only zero reaches the mandatory `current_phase` host call corresponding
to `004391B0` and manager `+64C`; nonzero networking bypasses that call. The
production [family particle host](../../crates/inf-game/src/session_platform/world_actor_behavior/particles.rs)
reads actual `SessionState.current_phase`, not a requested transition or a copy
in process storage. A failed notice bypasses the getter; a failed getter retains
the earlier notification/source mutations and stops before allocation.

The factory uses `00466100` with payload size `40` and final reclamation
category 3. Known phase 4 admits the pool/reclamation path; another known phase
returns scratch, while unavailable phase remains a reached typed requirement.
A notifier's source/global mutations therefore precede both real allocation
and common source copies. The allocator's scratch result is
nonnull; constructors still run against it. Exhausted earlier categories
destroy their eligible active tail without reading flags. Only the limiting
category tests **BYTE** `+12 & 80`; unknown high byte `+13` cannot block that
test. Pending records remain ineligible.

Only after the allocator returns its nonnull pool/scratch result are a
selected record's DWORD operands read, in order `+08`, `+04`, `+0C`, `+00`.
`00466850` instead supplies its
authored local operands. Common construction publishes before the derived
animation field `+28` and intermediate vtable `00526F40`. The animation
lifetime read follows that intermediate publication: a reached unavailable
header leaves the partially constructed class and earlier list custody.

`00466710` stores `+38`, `+2C`, `+34`; the other factories store `+34`, `+38`,
`+2C`. Final class and selector `+3C` then publish. Initial lifetime uses the
animation WORD `+0C`, decremented and narrowed to a WORD, except `00466850`'s
actual **DWORD** at `00539E7C` before the low-WORD store. `004665D0` makes its
one RNG draw only after the final class/selector writes and replaces lifetime
with `(low_word(draw) & 03FF) + 0100`. Caller-side RNG draws are separate.

## Packet lifetime and copying

The factory's `+34/+38` pointers reference resident executable packet data;
pool destruction does not destroy those packets. `factory_packet_source`
reads only the requested slot's fresh pointer, returns native null for zero,
and otherwise obtains four bytes from stable executable backing. It does
not prefetch the other slot or interpret the packet's opaque fourth byte.
Actual `00466120` destination ordering, known-mask transfer and subsequent
damage interpretation stay with the shared behavior-cache packet owner.
First transfer can precede a changed second pointer; readers must resolve
each slot after preceding work.

Complete supplied executable images can exercise alternate known parameters.
The current `FrontendExecutable` cannot represent unknown byte masks in its
backing. Native unknown-packet-source receipts are retained as copy evidence,
but excluded from factory-source Rust parity; the behavior-cache owner has
its own unknown-packet transfers. Runtime mutation of executable bytes and
source/payload aliases require a different custody owner.

## Per-class normal deletion and failure prefixes

[deletion.rs](../../crates/inf-game/src/process_particles/factories/deletion.rs)
uses the actual virtual deleting class, with a full captured flags DWORD;
only low BYTE bit 0 controls allocation release.

| Vtable | Normal deleting body | Additional reached work |
| --- | --- | --- |
| `00526EFC` | `00466D20` | Write its class, then `00466280` when low flags `+12 & 0C` is clear |
| `00526F20`, `00526F40`, `00526F60` | `00466D50` | Normal base teardown |
| Base/owned central/debris/trail/splash | Existing class-specific body | Existing normal base teardown |

Fresh common flags include `0C`, so freshly born `00526EFC` bypasses impact
work. The partially published `00526F40` is independently a proven normal
deleting class. It must remain deletable/reclaimable after a lifetime-header
failure, rather than being treated as an unknown vtable.

When `0C` is clear, `00466280` reads signed velocity WORDs in Y, X, Z order.
Its squared sum is `(Z*Z + Y*Y) + X*X`. The square root is compared before
being spilled to the f32 denominator. X division retains 53-bit precision;
Y and Z divisions are spilled to f32. Zero length reads the actual authored
X normal `00520A70` and uses zero Y/Z. Each axis separately reads scale
`00526EF8`, then uses original `00511BA8`: masked CW `027F`, truncation to
signed QWORD and low-DWORD return, followed by a low-WORD step. The shared
`native_trunc_low32` exposes the existing numeric owner; this module does not
introduce a second conversion algorithm.

Exactly five reached `00489600` calls receive selector 0, flags 0 and the
live particle's position/velocity identities. `impact_489600` is a required
mutable service, not an invented successful impact. Its success precedes a
fresh XYZ read and wrapped WORD increments. Step components stay captured
across callbacks. An unavailable third call retains two successful increments
and its callback effects, with no later increment, unlink or release.
The [live projectile host](../../crates/inf-game/src/session_platform/particles/combat.rs)
now supplies the real [impact birth](PROJECTILE_COLLISION_RUNTIME.md#cosmetic-impact-birth)
through that same contract, rereading position and velocity for every birth.
An adapter without this loan still reports the reached service boundary.

Only completed class teardown writes base `00526ED8` and unlinks through
`00465050` (the `0048B0D0` thunk for normal base deletion). Flags bit 0 then
returns the allocation via `00466110 → 00464EC0 → 00464E30`. Flags 0/2 unlink
but retain allocation/occupancy. The ordinary pool `destroy` API reports a
required-impact error on the clear-`0C` branch; factory reclamation supplies
the same live impact host. There is no unconditional base unlink before that
host has completed. Ordinary phase-gated scratch has no owned node: teardown
writes the base class and then reports `MissingNode` before unlink completes.
Only scratch with actual recorded node custody can reach the unsupported
allocation-header release after successful unlink, reported as `ScratchRelease`.

## Evidence and remaining boundaries

check_particle_factories.py authenticates
the executable and unchanged text bytes, then executes original factories,
offline notification, allocator, common constructor, packet copy, normal
deleting wrappers, list/free bodies and `00466280/00511BA8` at CW `027F`.
Pool initialization and process/source storage are supplied. Only named
network/impact boundaries are intercepted or stopped; this fixture does not
authenticate their bodies. The separate combat fixtures own impact birth.

The [compact fixture](../../crates/inf-game/tests/fixtures/particle_factories_native.json)
contains 154 histories and 262 reached steps: 100 ordinary native-kernel
histories, 15 histories with explicit supplied mutable impact services and
39 diagnostic histories. [Rust comparisons](../../crates/inf-game/src/process_particles/factories/native_receipts.rs)
admit exactly **113 histories / 202 steps**, excluding those 39 diagnostics
and two unknown executable-packet-mask histories. They compare retained
known/unknown payload bytes, returned pool/scratch/null identity, occupancy,
allocation marker, all pending/active list orders, free stack, RNG, process
null custody, packet-source output, required impact arguments/order and
interrupted reclamation. Native list/self pointer bits are represented by
semantic list custody, not invented numeric payload pointers.

Focused tests additionally cover a mutable network notification changing
phase/source before allocation, missing separate source after publication,
same-position unknown scratch transfer, flags 0/2 retention, unknown flag
bytes at allocator category boundaries and intermediate-class failure
reclamation. Supplied callbacks may mutate represented payload/process/source
storage. Physical header/list aliases, source aliases into pool storage,
double deletion/reallocation inside callbacks, malformed pointer admission,
unknown parameter masks and network encoder behavior remain diagnostics.
Native replay or retail capture is not part of these checks.

The [behavior-cache adapter](BEHAVIOR_DAMAGE_CACHE.md#concrete-particle-adapter-and-live-process-context)
now composes these factories with real per-slot packet source resolution and
normal deletion. Its eight-byte scratch has one live six-byte vector carrier
and two retained unread bytes, with explicit `SamePosition` identity and no
zero filling. Cache and factory reads share one process state and RNG through
`BehaviorPredicateContext`; mutable notice/impact services also receive the
actual arena and descriptor backing. That adapter's original fixture remains
independent of the now-connected [weapon consumers](WEAPON_FIRING_RUNTIME.md)
and [projectile lifecycle](PROJECTILE_COLLISION_RUNTIME.md). Lower actor
resources/components, network encoding, and adapters lacking a reached impact
loan remain feature-local boundaries; none imply complete intro actor `+24`.

The production session retains source-proven
[startup and process lifetime](PROCESS_ACTOR_STATE.md) for raw network and NULL.
Camera/completion borrow its live null operand, and existing network Boolean
inputs derive its raw network flag. No phase field remains in this carrier.
Intrinsic particle builders keep their genuine request phase under the
[post-allocation source policy](PARTICLE_RUNTIME.md#implicit-process-source-and-late-failures);
shared factories instead receive the actual manager loan through the consumed
[behavior adapter](../../crates/inf-game/src/object_runtime/actor/behavior/damage_cache/particle_adapter.rs).
The [phase service cases](../../crates/inf-game/src/process_particles/factories/phase_tests.rs)
cover offline/online laziness, post-notice changes and failure prefixes.
The [actual session adapter case](../../crates/inf-game/src/session_platform/world_actor_behavior/tests.rs)
distinguishes current from requested phase. These supplied cases and complete
allocator guards do not implement network serialization or establish all impact adapters or
complete intro behavior.

```text
cargo test --release -p inf-game --lib process_particles
python check_particle_factories.py --output <receipt>
```
