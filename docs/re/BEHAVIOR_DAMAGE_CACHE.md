# Infestation behavior damage cache

This authority owns the shared behavior predicate `0048D820`, its selected
44-byte descriptor cache and the ordered damage packet transfers it reaches.
The [actor behavior dispatcher](ACTOR_BEHAVIOR_RUNTIME.md) owns enclosing
callback selection; [helper history](FAMILY_HELPER_HISTORY.md) owns the
independent allocation queried by `00429CB0`. These policies select no
mission, model, camera or authored placement.

The source below is the supported retail executable and its unchanged native
instructions. The original predicate receipts supply lower services; the
composed particle adapter receipts also execute the owned factory, packet-copy
and deleting bodies. Actor resource services, network encoding, impact effects
and complete actor updates remain separate. Live cinematic acceptance remains
separate.

[`damage_cache.rs`](../../crates/inf-game/src/object_runtime/actor/behavior/damage_cache.rs)
provides `ObjectArena::test_behavior_damage_candidate` with a data-bearing
`BehaviorPredicateRequest`. `BehaviorRowSelection` captures a shared byte
address or a vehicle allocation and signed offset. The live arena and process
backing remain available to required mutable callbacks. No row borrow is held
across callbacks; the captured row token remains fixed while live behavior
state may change.

`ProcessActorBehaviorTables` maps original descriptor, registry and default
packet bytes through the supported executable's structural data mapper. It
does not authenticate an EXE hash. Keeping this owner outside the arena allows
ordinary rows to share across actors and world reloads. `BehaviorPredicateContext`
borrows these tables, one live `ParticleFactoryProcessState` and the current
process RNG. The tables retain no second process-null handle: cache creation
and particle notification read the same `0054E894` owner at their reached
instructions. This remains a kernel/service interface; production actor
dispatch attachment is separate.

## Enclosing predicate cache: 0048D820

The behavior pointer owns signed BYTE `+10` and table pointer `+14`; they
select a **44-byte** descriptor-row address. Null candidate returns false after
that address arithmetic, before any row read or row-range admission. Row BYTE
`+22` is cache readiness, and DWORDs
`+24/+28` are two complete damage packets: signed strength WORD, channel BYTE
and retained fourth byte. Ordinary actors selecting the same executable address
share this mutable cache. `ActorBehaviorState` retains the table identity and
selected byte; mutable descriptor bytes belong to a process owner. Vehicle
behavior has its separate copied 16-row table at `+26C`.

Any existing nonzero readiness BYTE skips initialization and retains its raw
value. An unready row reads signed kind WORD
`+00`, writes actual process DWORD `00546A94` to packet `+24`, rereads that
source and writes packet `+28`, then selects the 16-byte resource registry
entry at `0053DBA8 + 16*kind`. Supported executable rows' readiness `+22` and
packet DWORDs `+24/+28`, and the file-backed default DWORD, start at zero;
other row fields retain their authored data and the process source remains
independently owned.

| Registry branch | Required lower operation | Packet and readiness writes |
|---|---|---|
| Nonnull `+00` factory | Indirect fastcall with `ECX=DWORD[registry+04]`, `EDX=&local8`; stack receives the same scratch pointer, actual process null and zero | Actual null preserves readiness zero. Nonnull writes readiness one at `0048D8A1`, then `00466120` conditionally copies pointed DWORDs from temporary `+34/+38`; required deleting virtual `+00(1)` follows |
| Null factory, zero `+08` | No creation | Writes readiness one at `0048D92B`, then tests default packets |
| Null factory, nonzero `+08` | `00460310` with signed low WORD of `+08`, zero EDX and native scratch/null arguments | Actual null preserves readiness zero. Nonnull writes readiness one at `0048D8DE`, queries temporary virtual `+18` and reaches required `00479FA0` cleanup |

The latter actor branch queries returned component virtual `+6C` first.
Nonnull uses `004863D0`: copy source BYTE `+148` into first packet channel
before source WORD `+146` into its strength; packet fourth byte remains.
If `+6C` returns null, virtual `+68` may supply `00458F50`, copying the
complete source DWORD `+13E` into the first packet. Second packet stays at its
default on both branches. Null `+18`, or both null component queries, still
require cleanup. Scratch bytes are uninitialized input custody, not zeros.
The temporary reaches the creation listeners as any new actor does; a
Force Field's test of its residue position is unknown, and the port holds
that bit unknown on the temporary
([the Force Field](SELECTOR18_RUNTIME.md)).
Creation, copy and cleanup failures retain all reached cache writes. A stopped
copy does not run later cleanup automatically: the native body has no such
error-path unwind.

After initialization the predicate rereads readiness. **Still zero returns
true** at `0048D936 -> 0048D967`, including actual creation failure. A ready
row instead tests packet zero followed by packet one through `0042A8F0`,
modifier zero and a freshly read candidate damage table `+A4`. The first
nonzero adjusted signed strength returns true, including a negative value;
both zero return false. `0042A8F0` reads packet channel before strength, then
reads the selected threshold even for zero or negative strength. It reads
the multiplier only when signed strength exceeds that unsigned threshold.
The portable tail admits the actor's complete canonical ten-channel response
table and shares existing resisted-damage arithmetic with modifier zero.

## Shared and copied descriptor storage

The thirteen ordinary table identities lie in one contiguous executable
region `00530E60..005311BC`, containing 19 authored 44-byte rows and 24
padding bytes. Preserve the original image and address aliasing; independent
profile-sized copies would lose native sharing.

| Mode | Table start | Authored rows |
|---|---|---:|
| 1 | `00530E60` | 2 |
| 13 | `00530EB8` | 2 |
| 2 | `00530F10` | 2 |
| 3 | `00530F68` | 1 |
| 4 | `00530F98` | 2 |
| 5 | `00530FF0` | 2 |
| 6 | `00531048` | 2 |
| 7 | `005310A0` | 1 |
| 8 | `005310D0` | 1 |
| 9 | `00531100` | 1 |
| 10 | `00531130` | 1 |
| 11 | `00531160` | 1 |
| 12 | `00531190` | 1 |

The six four-byte gaps start at `00530F94`, `005310CC`, `005310FC`,
`0053112C`, `0053115C` and `0053118C`. Constructor counts own allocation
and ordinary initial selection, not predicate range checks. `0048D820` has
no profile-count guard: Mode 2 with raw index `FF` selects Mode 13 row 1.
Capture the signed-index address before callbacks, then require only each
reached backed span. Do not require an entire nominal row before the first
read, or reject a null candidate because its row would be out of range.
Traversal outside represented storage remains an explicit boundary rather
than portable native-fault parity.

Selector 2 copies 16 rows (`704` bytes) from `00530158` into its own `+26C`
only when admission sees null behavior `+140`. An existing behavior retains
that copy. The vehicle's selected identity is its actual allocation plus
signed byte offset; it does not use the process executable rows. The existing
DWORD values remain authoritative, with independent byte custody for unknown
copy sources. Vehicle upgrades write their actual `+26C` fields and do not
overlap row readiness or packet bytes.

Both defaults and copied packets retain byte-level known custody. Unknown
data can be copied without pretending it is zero or failing solely because
the fourth byte is unknown. Only a reached consumer demands its used fields.
Unavailable source storage or an unknown source pointer is a different
boundary from available storage containing unknown bytes.

## Packet transfer phases

The predicate is the sole direct caller of each of these copy bodies.

| Body / call site | Ordered transfer |
|---|---|
| `00466120` / `0048D8A5` | Read temporary `+34`; nonnull copies the pointed full DWORD into packet zero. Then reread temporary `+38` after that store; nonnull copies its pointed full DWORD into packet one. Null pointers preserve the defaults. |
| `004863D0` / `0048D8FD` | Read source BYTE `+148` and store first-packet channel before reading source WORD `+146` and storing strength. Preserve packet byte 3. |
| `00458F50` / `0048D91D` | Copy source DWORD `+13E` into packet zero. The second destination argument is unused. |

Keep source reads and destination writes in that order and retain partial
copies on a reached failure. Pointer-alias experiments beyond represented
source/descriptor storage remain diagnostics; they are not a reason to
snapshot all sources before the first write.

## Required lower services and lifetime bounds

Static registry bytes and raw `1.ovl` resource selectors establish the lower
factory boundaries; the mapping alone does not establish their execution.

| Registry kind | Factory / argument | Returned particle vtable |
|---|---|---|
| 2..7 | `00466710` / `5,6,7,2,3,4` respectively | `00526EFC` |
| 9 | `00466850` / `8` | `00526F60` |
| 30, 31 | `004665D0` / `11,12` | `00526F20` |

The latter factory also draws the process RNG. Before allocation, all three
factories reach creation notification `00466010 -> 00492500` when input flag
bit 0 is clear; this is not the deleting callback. The predicate supplies
flags zero. Offline `00492500` returns without reading the vector payload;
network registration/encoding/queueing remain separate required services.
Other nonnull H13 resource
entries select constructors 25, 26, 27, 28, 31, 32, 38, 68 and 76, the
[behavior registry's resource classes](ACTOR_RUNTIME.md#behavior-registry-resource-classes).
Components for 31/32 reach `004863D0`; 25/26/27/28/38/68 reach `00458F50`;
76 returns null from virtual `+18` and still requires cleanup. Kinds 0 and
23 have neither factory nor H13 slot.

The concrete particle adapter below composes the three recovered factories,
their fresh packet sources and normal deleting wrappers. The session's
services build the actor resources through `00460310` and answer their
component queries and packet reads (the world factory is lent wherever a
target selection can reach this cache; elsewhere the creation stays an
explicit boundary). Network encoding and the reached impact body remain
required mutable services. Actor cleanup uses the existing `00479FA0`
disposal owner. No successful stub closes these boundaries; actual creation
null remains its separate source-proven result.

## Concrete particle adapter and live process context

[`particle_adapter.rs`](../../crates/inf-game/src/object_runtime/actor/behavior/damage_cache/particle_adapter.rs)
implements `BehaviorPredicateHost` with real `ParticleId` custody. It dispatches
the captured callback identity to `004665D0`, `00466710` or `00466850`, passing
the full registry `+04` DWORD as the selector. An unowned callback is an error;
an invalid selector handled by an owned factory returns its actual null result.
No model, mission or actor-class whitelist substitutes for callback ownership.

The native call-local eight-byte scratch remains wholly unknown at entry.
Its sole storage is partitioned into the live six-byte `ParticleFactorySources`
position and two retained unread bytes. Both survive throughout creation and
are exposed together to the required network service. `SamePosition` denotes
the same position/velocity source, so a synchronous change of the first six
bytes precedes both constructor transfers. The adapter neither keeps a second
full scratch snapshot nor invents padding or zero vector words.

`BehaviorParticleServices` retains the required actor-resource and component
boundaries. Its particle effect request names network notification or
`00489600` impact. The mutable service context exposes the live arena, tables,
pool and common process/RNG/EXE context together. Notification additionally
receives the actual creation sources; impact receives the live particle
identity and has no dummy creation source. Mutations remain visible to later
native phases without a final synchronization or row reselection.

After construction, each packet source resolves its current `+34/+38` pointer
through `factory_packet_source` at the reached copy. Normal deletion consumes
flags `1` through the actual deleting body. Fresh factory flags include `9C`,
so ordinary `00526EFC` temporaries skip the impact vector consumer even when
scratch vector words remain unknown. A changed or reclaimed clear-`0C` particle
instead reaches the real required velocity reads and impact service. Neither
case permits invented vector values or automatic error-path cleanup.

Factory allocation can return the nonnull process scratch record. The cache
still writes readiness and copies its packets before deletion reaches an
unowned list/allocation-header boundary; this result is not coerced into
allocation null. Factory, packet-source, notice and impact errors retain their
phase and underlying error text, along with all prior mutations.

check_behavior_particle_adapter.py
executes unchanged `0048D820`, the selected factory, fresh packet-copy and
normal deleting owners in one invocation. The
[compact fixture](../../crates/inf-game/tests/fixtures/behavior_particle_adapter_native.json)
contains 26 histories, 27 predicate calls and 370 native entries. The
[Rust comparison](../../crates/inf-game/src/object_runtime/actor/behavior/damage_cache/particle_adapter/native_receipts.rs)
compares 23 kernel histories and three separately bounded scratch-cleanup
prefixes. It checks retained byte masks, tables, process state, candidate data,
particle/list/free custody and lower-service cadence, including repeated ready
calls. Scratch prefixes compare the prior writes and semantic `MissingNode`
boundary; the native receipt stops before an unowned raw write at `0046505C`,
with no native fault or stop-IP parity claimed. Nonzero-network encoding and
`00489600` impact remain supplied mutable bodies; the offline `00492500` gate
executes unchanged. The original
299-history/306-call cache, 224-row gate and 113-history/202-step factory
comparisons remain independent evidence for their admitted domains.

The context is shared by the candidate gate as well as the cache. Its supplied
process state does not by itself publish a new production global owner:
`PortSession` owns the persistent particle pool, `WorldSession.rng` is the live
world RNG, and the current network/phase owners remain the session. Production
binding must use those actual owners and provide independently mutable
`0054E894`; a configured actor-registry null handle is not a fresh global read.
The [common and selector-12 `+100`/`+104` selection kernels](FAMILY_TARGET_SELECTION.md)
now compose this gate with their distinct inline-table/actor-list reads and
ordered weighted publishers. Common policies retain actual live task custody;
selector-12 `004686E0` requires no task or source read. Selector-12 actor-list
`00468350` instead selects each candidate's fresh task and retains its score
and presence captures through the gate. Production dispatch, the unrepresented
selector-13 constructor, enclosing reaction, lower actor factories and complete
actor `+24` attachment remain separate ownership work.

Inspected `00426C80/00428390/0048CC70` bind tables or change selection without
resetting external rows. `0048CD60/00425F70` release behavior-owned arrays;
the inspected ready writers are the three `0048D820` stores of one. Vehicle
upgrade `0046B110` changes its fields without touching readiness/packets.
Direct world cleanup `00478CD0` and H13/H14 resource release/reload do not
directly reset executable rows. Their invoked callbacks and destructors
require further audit; these bounded observations do not prove that no other
path ever resets or mutates a cache. Ordinary backing must survive world
reloads rather than being reconstructed with each world arena.

## Family candidate gate: 00429CB0

This gate captures actor `+218` presence first. Null candidate or null actor
behavior `+140` returns false. An absent helper returns `0048D820` directly.
A present helper calls the same predicate and returns false on its zero
result; otherwise it rereads both actors' XYZ WORDs, forms wrapping
actor-minus-candidate displacement, reads candidate raw handle `+84`, then
reloads actor `+218` at `00429D09`. It returns the logical inverse of the
query on that **current** helper. Predicate callbacks may change actor state,
so retaining the old helper pointer or position packet would be an unsupported
alias assumption.

Direct gate callers are `00428CCD/004291D6` inside the common virtual
`+100/+104` paths and `004684D2/004687FD` inside selector-12/13 overrides.
The predicate also appears at `0041556D` in scalar score `004153A0`, whose
constructor caches behavior through actor virtual `+7C`; it is not specific
to the intro.

The shared [gate kernel](../../crates/inf-game/src/object_runtime/actor/behavior/family_gate.rs)
now composes the real cache predicate and helper query. Its request retains
the actor and nullable candidate identities. Only initial helper **presence**
is captured: an initially absent helper returns the predicate result even if
a callback installs a helper, while an initially present helper uses the
replacement allocation after a successful predicate. A callback that removes
that helper reaches a typed missing-helper error after fresh position and
handle reads; the native query would dereference null `+08`. A failed
predicate skips that tail, retaining all preceding callback/cache writes.

The gate checker executes unchanged
`00429CB0`, `00483000` and its CRT truncation path with `0048D820` explicitly
supplied as a mutable service. The [fixture](../../crates/inf-game/tests/fixtures/family_candidate_gate_native.json)
has 225 cases, including 224 typed comparisons and one unbacked-pointer
diagnostic. Its [Rust comparison](../../crates/inf-game/src/object_runtime/actor/behavior/family_gate/tests.rs)
invokes the real Rust cache through supplied particle services, compares
branch results, reached stops and callback effects, and covers wrapped
displacements, raw handles, helper replacement/removal and lazy unknown
normal reads. Complete XYZ packets are the typed actor domain; native
partial-axis storage and raw aliases are not claimed. This gate receipt
does not establish execution of the native predicate body; the separate
cache receipts below own that evidence. [Particle factory kernels](PARTICLE_FACTORY_RUNTIME.md)
now own construction, source resolution and deleting bodies separately. Their
concrete cache adapter is owned separately above; the common scanner/selection
caller is owned by [family target selection](FAMILY_TARGET_SELECTION.md).
Production dispatch, distinct callbacks, lower actor resources and complete
actor `+24` attachment remain open.

## Native evidence

The tracked checker authenticates
the supported EXE hash and executes unchanged `0048D820`, `00466120`,
`004863D0`, `00458F50` and `0042A8F0` instructions. Creation, component
queries, particle deletion and `00479FA0` are explicitly supplied mutable
service boundaries; their instruction bodies are intercepted, not claimed
as executed. The checker asserts the entire executable `.text` stays intact.

Its [compact fixture](../../crates/inf-game/tests/fixtures/behavior_predicate_cache_native.json)
contains 324 cases and 331 calls: 299 portable cases with 306 ordered calls,
plus 25 native-only diagnostics. It covers shared repeated calls, adjacent
profile aliases, independent vehicle copies, rebinding after captured row
selection, null-candidate and failed-creation bypasses, live readiness,
partial default/copy/service failures and lazy unknown-byte consumption.
Negative strengths still reach the native threshold read. Copied unknown
opaque bytes are retained without becoming numeric input requirements.

The [Rust comparisons](../../crates/inf-game/src/object_runtime/actor/behavior/damage_cache/native_receipts.rs)
separate the supplied kernel/service contract from concrete disposal;
[focused tests](../../crates/inf-game/src/object_runtime/actor/behavior/damage_cache/tests.rs)
pin storage, packet order and actor/vehicle custody. Raw pointer aliases,
out-of-image selection, registry kinds outside represented backing,
noncanonical channels and artificial read-stop experiments stay diagnostics.
Arbitrary cache mutations or an unavailable external body at intercepted
`00479FA0` also stay diagnostic: the Rust predicate calls actual queued
disposal rather than exposing a new cache-mutating cleanup callback.
These receipts establish bounded supplied-state behavior, not weapon
construction, cache invalidation across every callback or retail cinematic
acceptance.
