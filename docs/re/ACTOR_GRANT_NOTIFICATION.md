# Infestation canonical actor grant and vehicle-cleanup notification

This authority owns the represented actor `+98` event-22 grant notification
and event-10 vehicle-cleanup notification through the zero-radius arm of
`0047DAF0`. The two native suffixes share class admission and list traversal;
their prerequisites remain distinct. The grant predicate remains in
[the selector-12 grant authority](GRANT_DIFFERENCE_RUNTIME.md); its player
search remains in [the live lookup authority](PLAYER_ACTOR_LOOKUP.md).
Complete intro and briefing acceptance belongs to
the Infestation objective.

## Canonical receiver policy

Dispatch follows the actual final class. Mutable classification `+B4`, actor
model, campaign, scene and output resolution do not select its virtual binding.
These four-argument numeric receivers are distinct from the two-argument void
[interaction notification at `+9C`](ACTOR_INTERACTIONS.md).

| Final classes | Original `+98` body | Reached event-22 policy | Reached event-10 policy |
| --- | --- | --- | --- |
| Central derived, family/probe/4236 bases, selectors 10/34/35/36/77 | `004270B0` | Exclude event 13; full EAX zero, no actor payload read | Same event-13 exclusion |
| Selector 11 | `00402420` | Unsigned `22 - 7 > 14`; full zero, no actor payload read | Index 3, BYTE `0040251F = 6`, DWORD `00402518` selects `004024F8`; full zero, no actor payload read |
| Selector 60 | `00480F40` | Exclude events 7/8; full zero, no actor payload read | Same event-7/8 exclusion |
| Shared, selectors 16/88/90/93 | `004045F0` | Immediate full zero, no actor payload read | Same immediate return |
| Selector 30 | `0048C470` | Forward all four operands to actual `0040DE70`; event 22 excludes 7/8 and returns full zero | Same delegation; event 10 excludes 7/8 |
| Selector 2 | `00420BE0` | Read actual actor `+84`, call `0046B090`, then return full zero | Same mandatory lookup before the event suffix |
| Selector 12 | `00468BE0` | Actual first-player grant difference, conditional literal byte `+29D`, then full zero | Exclude events 13/22 at `00468BEF`; no player, grant or `+29D` access |

All nineteen represented canonical classes contribute full DWORD zero on a
completed call of either mode. They do not all bypass prerequisites.
Raw scalar-base table `0052104C` and selector-13 table `005269C4` have known
bindings but no portable final class or constructor. A stale receiver calling
a constant native body is separate from a live typed actor identity.

Selector-2's actual receiver handle is read at `00420BE4`, and `00420BEB`
calls the complete live player lookup **before** reading the event argument
at `00420BF2`. Its raw event map subtracts 4, checks unsigned index 0..19,
reads BYTE `00420D88 + index`, then jumps through DWORD table `00420D64`.
Event 22 selects index 18, byte 8 at `00420D9A`; event 10 selects index 6,
byte 8 at `00420D8E`. Both reach arm `00420D5C` through DWORD `00420D84`.
That suffix ignores the returned player, clears full EAX and executes `RET 16`
at `00420D60`.

The prerequisite captures current process null `0054E894` at `0046B096`
before request equality or player-list access. It still requires that operand
with no players or with a handle equal to the eventual null. Non-null `+D8`
is authoritative; only equality with the captured null permits `+D4` fallback.
No actor registry, resolver, network, clock or player mutation is consumed.
An unavailable process owner or null DWORD stops this reached receiver; other
canonical receivers acquire no such dependency.

Selector-12 retains its distinct first-player policy: no player preserves
`+29D`; a player causes live source actor `+24C` minus target player `+9C`
queries for IDs 0..174 and literal byte 0/1 publication. Neither mask changes,
and the callback's full DWORD result remains zero. This does not use selector-2's
handle search or captured process-null prerequisite.

## Vehicle cleanup caller and constructor-table admission

The complete caller `00415BA0..00415BB7` pushes literal fourth DWORD zero,
event 10 and radius zero, then calls `0047DD20` at `00415BA9` with the actual
vehicle as source ECX. Only after this notification completes does
`00415BB0` call common cleanup `0047EC50`. Owning the fanout does not replace
that later cleanup.

`0047DD20..0047DD3A` forms source `+32` by **LEA** at `0047DD24`; it does not
read XYZ. It forwards source ECX, radius EDX and stack event/fourth/XYZ to
`0047DAF0` at `0047DD32`, preserving the returned full EAX and executing
`RET 12` at `0047DD37`. Thus this caller's four receiver arguments are
`(captured vehicle source, 0, 10, 0)`. Represented event-10 receivers never
dereference the source. A stale or foreign captured source is distinct from
a reached receiver, which still requires actual typed class custody.

The [canonical 94-constructor dispatch](../../crates/inf-formats/src/constructor_assets.rs)
and original constructor base-call chains distinguish the following complete
set. Selector numbers are decimal; a raw DWORD at table `+98` alone does not
establish an actor event receiver or its calling convention.

| Constructor-table classification | Selectors |
| --- | --- |
| Actor-FIFO prefix, final table represented by the current receiver owner (22 rows) | 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 16, 30, 33, 34, 35, 36, 60, 77, 88, 90, 93 |
| Actor-FIFO prefix, final class unrepresented (43 rows) | 13, 15, 17, 18, 20, 21, 23, 24, 25, 26, 27, 28, 29, 31, 32, 37, 38, 39, 40, 41, 44, 47, 49, 52, 53, 55, 56, 57, 59, 61, 62, 66, 68, 69, 73, 74, 78, 79, 80, 82, 84, 85, 91 |
| Nonactor constructor prefix; raw `+98` is not admitted as this event receiver (29 rows) | 1, 14, 19, 22, 42, 43, 45, 46, 48, 50, 51, 54, 58, 63, 64, 65, 67, 70, 71, 72, 75, 76, 81, 83, 86, 87, 89, 92, 94 |

The 65 actor prefixes reach `0047A7A0`, whose `+A8` node is appended to `G+48`.
The other 29 reach visual/base constructors `00479C60`, `0047A030` or
`0047A490`, without that actor-FIFO prefix. This classification establishes
the inspected initial publication domain, not arbitrary later list insertion.
The nineteen typed classes also include represented intermediate family and
probe tables that are not separate final rows in the 94-constructor table.

For example, selector 75 constructs through `004352F0` to `00479C60`.
Its table `00522008` raw `+98` word names `00435870`, a two-argument `RET 8`
reader, not the four-argument actor notification ABI. Selectors 63/64 reach
`0047A490`; raw `+98` values `004026B0`/zero do not turn those short tables
into actor receivers. The unrepresented actor body `004557F0`, used by
selectors 26/27/38, can compare source `+84` with receiver `+88`, publish
byte `+14E = 1` and return full EAX one on event 10. Therefore the current
completed-zero policy is bounded to represented classes, not all native actors.

## Zero-radius list and return ordering

`0047DAF0` is fastcall: ECX is an opaque source pointer, EDX is radius, and
the three stack operands are event, fourth raw operand and XYZ pointer. The
owned policies are the literal zero-radius event-22 branch and the vehicle
caller's zero/event-10/fourth-zero branch, with authentic `RET 12`.
Nonzero radius and other events remain separate.

The original captures source into EBX at `0047DAF7`, clears accumulator EDI,
captures game owner `[0055F160]` at `0047DAFB`, and takes the zero-radius
branch at `0047DB0B`. It reads the captured owner's `G+48` head-next at
`0047DBA4`, then reads that current node's next pointer at `0047DBA7` before
testing the current pointer itself for null. A valid empty list reaches its
tail; malformed raw null nodes are not an empty typed list.

For each node, `0047DBB4` reads payload `+8`, `0047DBBD` reads the receiver's
table, and `0047DBC2` calls actual slot `+98`. The four callback operands are
`(captured source, literal distance zero, event, captured fourth operand)`.
Completed callbacks execute `RET 16`. The represented event-22 and event-10
arms ignore source, distance and fourth; this list arm never reads XYZ. It
requires no source actor, position, distance calculation or self-exclusion.

After a completed callback, `0047DBC8` reads the **fresh successor of the
captured current node** before `0047DBCA` adds full EAX to EDI with DWORD
wrapping. `0047DBCC` reads the successor's next pointer before its null test;
the loop preserves FIFO order. The final result is full EDI at `0047DBD5`,
followed by `RET 12` at `0047DBDE`. Completed admitted contributions are zero;
no invented nonzero callback proves native overflow behavior.

The actual list is actor `+A8` at `G+48`, represented by
`ObjectArena::actor_list`. Construction initializes payload at actor `+B0`
and appends through tail predecessor `G+58` at `0047A84D..0047A86D`, before
handle registration. Destruction unregisters `+84`, then unlinks `+A8` at
`0047AA97..0047AAB2`, before common teardown. The list is independent of
the common six flag-selected lists. Parking or disposal changes do not
exclude an actor that remains in this list; actual destruction removes it.

If a later selector-2 requires unavailable process null, earlier event-22
receiver `+29D` writes remain and later nodes are not visited. Event 10 bypasses
those writes while retaining the same reached-error and later-node boundary.
There is no rollback or phase-wide preflight. No admitted receiver changes
actor-list custody during a call; raw pointer aliases, replaced native globals,
arbitrary virtual overrides and allocation reuse remain separate boundaries.

## Portable owner and production boundaries

The [portable notification owner](../../crates/inf-game/src/object_runtime/actor/grant_notification.rs)
provides `ActorGrantDifferenceNotificationRequest` with explicit direct-actor
and actor-list delivery modes, and a separate
`ActorVehicleCleanupNotificationRequest { source }` consumed by
`notify_actor_vehicle_cleanup`. Cleanup owns the actual actor-list delivery;
the grant operation retains its distinct public error and suffix policy.
Private typed modes share one complete receiver prefix and moving-node walker.
Reached receivers are authenticated; cleanup's captured source remains unread.
The narrow process borrow is read only on reached selector-2 lookup. The
existing selector-12 callback
and player lookup retain their meaningful independent consumers.

The list path uses the actual arena's FIFO and reloads the captured node's
successor after each successful receiver. It preserves source wrapping return
arithmetic and existing reached owner errors. It introduces no detached masks,
new player mutator, registry fallback, arbitrary host callback or callback
cadence observer. Actor construction, lifecycle and full typed custody retain
their existing boundaries.

The literal zero/event-22 callers are `0042F5C6`, selector-12 pickup
`004681F4`, and selector-12 capture `0046825C`. Shared Set/Merge policies are
owned by the [live grant kernel](GRANT_RUNTIME.md); caller-specific owners,
enclosing composition and enabled message phases remain separate. In particular,
the `0042F5C6` predecessor's `+198` grant receiver is unrepresented. The
[player-message gate](PLAYER_MESSAGES.md) owns live suppression and its early
return, without skipping earlier grant formatting. Capture also retains its
returned player for [commands](PLAYER_COMMAND_RUNTIME.md) `0`, `1` and `48h`,
status identity and
[shared capture](ACTOR_CAPTURE.md). Owning this list delivery does not attach
those surrounding callers or own their command effects.

Only `00415BA9`'s fixed cleanup call through `0047DD32` and the
detonator's radius call below are owned here; the wrapper's other
radius/events remain unadmitted. `00454359` broadcasts
event 14, which is also separate. Complete vehicle/manager cleanup, surrounding
capture reactions, arbitrary native actor classes and matched retail acceptance
remain separate owners. Stable arena and borrowed process backing exclude
mid-call owner replacement, native physical aliases and allocation reuse.

## The detonator's radius arm

The [detonator](SELECTOR26_RUNTIME.md#messages) `0048CB00` calls
`0047DD20(4000, 10, 0)` from the vehicle. `0047DAF0`'s nonzero-radius branch
walks the same `G+48` list. For each actor it takes signed WORD deltas from
the source's live position, sums their squares with wrapping, and compares
that sum unsigned with the radius squared. A receiver within it gets
`+98(source, distance², 10, 0)` through the same class admission as the
cleanup, and the full EAX results are summed. The grenade's and the mine's `004557F0` is a
represented receiver here: one the vehicle owns marks itself to detonate
and answers 1, and one of another owner answers 0. Infest02's detonator call
reached the vehicle's own `+98`, whose lookup ran first; Infest03's marked a
mine. Other codes go through the general
[message walks](ACTOR_INTERACTIONS.md#messages-98).

## Focused verification

The [ordinary tests](../../crates/inf-game/src/object_runtime/actor/grant_notification/tests.rs)
retain eight event-22 regressions that pass in release mode. Their genuine
constructors cover the eighteen classes represented by that fixture,
independently of mutable `+B4`, unused XYZ, flags and lifecycle.
They distinguish mandatory selector-2 process-null reads, authoritative stale
player `+D8`, actual empty-list and non-vehicle bypasses, preserved earlier
receiver effects on later lookup failure, and real parking/disposal/destruction.
Actual first-player appends and live masks are reread between deliveries.
Full typed actors, players, list/registry/cache and allocation custody remain
unchanged except the reached selector-12 literal byte writes.

Six event-10 tests use all nineteen current genuine classes, including
selector 93. They check full arena/player/list/registry/process custody,
selector-12 grant bypass versus actual event-22 publication, selector-2's
empty-player and authoritative `+D8` prerequisites, unread foreign/stale source,
and fresh FIFO delivery through real parked/disposed/destroyed/appended actors.
The later lookup failure retains the earlier state and leaves later receivers
unvisited. These source-backed test cases do not measure native execution
cadence or establish a general cleanup/retail domain.

```text
cargo test --release -p inf-game --lib grant_notification::tests
```

## Original-byte evidence

check_actor_grant_notification.py
owns the **event-22** original-byte evidence below. It executes the complete
unchanged `0047DAF0` zero-radius body, all seven receiver
entry bodies, selector-30's actual `0040DE70` delegation and the nested lookup,
first-player and grant leaves. Thirteen complete body guards, twenty final
table `+98` bindings, four switch-data guards and twenty-seven instruction
guards authenticate the supported EXE, unchanged `.text` and FPCW `027F`.
The selector-11 switch data is authenticated but unreached by event 22.
There is no service/result injection, executable patch or fabricated return.
Event 10 is established here by the complete original caller, wrapper,
receiver/switch sources and typed ordinary cases; this checker does not execute
event 10 or measure its callback cadence.

The [native fixture](../../crates/inf-game/tests/fixtures/actor_grant_notification_native.json)
contains 34 histories and 66 invocations: 27 admitted histories/56 invocations,
including three required-input prefixes, and seven named diagnostic
histories/10 invocations. It records 1,263 routine entries and 1,239 authentic
return completions, including 1,093 grant-query entries and 1,091 query returns.
There are 1,546 recorded storage reads, 72 executable-table reads, twelve
literal `+29D` writes and fifteen supplied between-request effects.

There are 53 completed admitted invocations and three missing-process-null
prefixes. The diagnostics separately contain three completions and seven stops.
All retain full supplied byte knownness; no fault is observed. The required
prefixes stop at the `0046B096` operand and retain
any earlier receiver writes. Unknown required numeric bytes stop before their
consumer rather than becoming zero or a native bounds/fault assertion.
Direct callbacks execute `RET 16` with stack delta 20; full broadcasts execute
`RET 12` with delta 16. Unmapped ignored source/origin and raw high direct
distance/fourth operands do not become new typed dependencies.

Native timeline checks establish FIFO payload/table delivery, fresh successor
reads after successful callbacks, actual selector-2 handle/lookup order,
first-player selection, ascending source-first grant queries and termination
at the first target-unowned ID. Native measured cadence remains separate from
portable result/state parity. Supplied constructor, flag/disposal/destruction
and registry carriers are setup; their original bodies and numeric registry
parity are not measured by this checker. Inherited harness preparation is
excluded from the recorded counts.

Independent regeneration reproduced fixture SHA-256
`AEEC41FA0BF4828C8A380D0545AC314DC24DB30F22A3DDEB888CED5B1DE9EA7B`.
Raw scalar/selector-13, stale constant-body receiver, unknown actor
handle/grant/global/list backing and partial process-null diagnostics are
excluded before typed preparation. They establish no retail acceptance.

```text
python check_actor_grant_notification.py --output .tmp/actor-grant-notification.json
```

The [native evidence audit](../../crates/inf-game/src/object_runtime/actor/grant_notification/native_evidence.rs)
checks the original bytes, full masked backing and ordered native entries,
reads, writes and authentic returns independently. The
[typed comparison](../../crates/inf-game/src/object_runtime/actor/grant_notification/native_receipts.rs)
passes in release mode over all 27 admitted histories: 53 completed calls and
three reached required-process-null failures. Seven exact diagnostic histories
are excluded before typed construction. Genuine constructors, live players,
borrowed process storage and actual flag/disposal/destruction operations supply
the portable inputs; only post-call assertion clones receive the native byte
writes. Complete live typed payloads and arena/player/list/registry/cache,
allocation and process custody are compared. This proves bounded result,
error and state parity; no Rust cadence observer is installed.

```text
cargo test --release -p inf-game --lib actor_grant_notification_matches_native_event22_broadcast_histories
```
