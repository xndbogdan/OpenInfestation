# Infestation player lookup by actor handle

This authority owns `0046B090..0046B0E2`, the ordered player search used by
camera target loss, actor-bearing mission completion and class reactions.
It is separate from actor-handle resolution, player construction and player
attachment. Full intro and briefing acceptance remains in
the Infestation objective.

## Captured process operand and read order

`0046B096` reads live process DWORD `0054E894` into EDI exactly once. This
precedes request equality, manager availability and every player-list read.
The value is independent from the actor registry's configured null handle.
An unavailable process operand cannot become registry null, a particle birth
source, a task-acquisition handle or literal zero, even when no player is listed.

| Reached instruction | Policy |
| --- | --- |
| `0046B096` | Capture live process null |
| `0046B09C` | Request equal to captured null returns no player |
| `0046B0A0..A7` | Read gameplay manager `0055F160`; null returns no player |
| `0046B0A9..B5` | Read current player-list node and its payload |
| `0046B0B8..C6` | Non-null player `+D8` is authoritative; compare it alone |
| `0046B0C8..CE` | Only `+D8 == captured null` permits comparison of `+D4` |
| `0046B0D0..D9` | Advance to the live next node; retain the first match |

"Non-null" here means raw handle inequality with the captured process value.
A stale handle, literal zero or registry null can therefore be authoritative.
Neither candidate handle is resolved. The routine reads no actor state,
lifecycle, registry slots or resolver cache; it calls no lower service and
writes no process/player/list storage. Duplicate matches keep the earlier
player. Process changes between separate calls affect the next call, while
one call retains its single captured value.

The native head/next sentinel test dereferences the node before the subsequent
null-pointer test. A corrupt raw node is not equivalent to an empty portable
list. Physical list aliases, uninitialized pointers/handles and a missing
native manager remain separate evidence domains.

## Caller ordering

[Mission completion](../../crates/inf-game/src/world_completion.rs)
returns immediately for network sessions. Offline cleanup precedes an optional
actor's actual `+84` handle read and this lookup. A failed required input
retains cleanup, then stops before actor disposal or mission writes. A
completed search with no match queues disposal and returns rejection. The
null-actor intro path never invokes this lookup; it publishes mission state
before its separate first-player fallback and checkpoint reset.

[Camera target loss](CAMERA_RUNTIME.md#native-host-rotation-and-draw-publication)
invokes this lookup before falling back to the first player. Missing required
input must stop before that fallback, `0046B060`'s separate first-player `+D8`
resolution, camera mode zero and attachment. The camera tick has already
published its reached escape-velocity/state prefix; failure retains it.
A matched player identity can differ from the first player consulted by
`0046B060`. Neither that pure accessor nor ordinary camera-height queries
acquire a new process-null dependency.

Selector-12's enclosing [capture reaction](ACTOR_CAPTURE.md#portable-and-remaining-boundaries)
uses this lookup before actor-to-player grant merge and retains the returned
player for its later commands. This lookup alone does not own that reaction,
grant notifications, actor broadcasts or player commands.

## Portable custody and production boundary

[`ObjectArena::find_player_for_actor_handle`](../../crates/inf-game/src/object_runtime/player.rs)
uses one handle-bearing request and a `PlayerActorLookupContext` that borrows the existing
process owner, distinguishing an unavailable owner from an owner whose null
handle has not been supplied. Both failures belong at the reached `0046B096`
read. The portable list belongs to the actual arena; no mutable query callback,
compatibility fallback or detached player snapshot replaces it.

The production session now retains one
[process owner](PROCESS_ACTOR_STATE.md), initialized from authenticated PE
loader zero-fill and borrowed by camera/completion builders across screen,
world and display replacement. The process null remains mutable and optional;
unavailable input still stops only a reached lookup feature. Registry
construction does not establish that value. The admitted null-actor intro
completion and an unreached camera target-loss branch remain independent.
Other process-null consumers and callback writers retain their separate
boundaries; this attachment does not constitute complete cinematic or retail
acceptance.

## Focused verification

The release-mode [player tests](../../crates/inf-game/src/object_runtime/player_tests.rs)
pass all eight cases, including mandatory missing input for an empty list or
raw-zero source, process/registry null separation, authoritative unresolved
`+D8`, full-DWORD identity, first duplicate match, no registry requirement and
unchanged player/cache/slot owners. Repeated calls observe changes to the same
supplied process owner.

The eight [completion tests](../../crates/inf-game/src/world_completion_tests.rs)
and nine [native camera-host tests](../../crates/inf-game/src/object_runtime/actor/selector16/native/tests.rs)
also pass in release mode. They preserve network/cleanup/null-actor bypasses,
cleanup before missing lookup input, rejection before mission writes, lookup
failure before camera fallback/resolution/view/attachment, and the actual
camera tick's escape XYZ, velocity and published state before its error.
These tests exercise typed callers; they do not establish execution of the
whole native completion/camera routine. The ordinary game release build passes
separately from test compilation.

```text
cargo test --release -p inf-game object_runtime::player::tests
cargo test --release -p inf-game world_completion::tests
cargo test --release -p inf-game selector16::native::tests
```

## Original-byte evidence

check_player_actor_lookup.py executes
the unchanged 82-byte `0046B090` body. It authenticates the supported EXE
SHA-256 `BF09F66E3685D4990D95DA13D4BE1CA7E293FB31CDDDCCC90BD575882921E4956`,
unchanged `.text`, FPCW `027F` and body SHA-256
`18DFB4B22C6657B39616026564AF78C321EE059AD2C8A25DA2B6C0C2AC975EE8`.
Its [fixture](../../crates/inf-game/tests/fixtures/player_actor_lookup_native.json)
contains 20 histories/31 invocations: thirteen kernel histories with 21
completed calls, three required-input prefixes, and four named diagnostics
with seven calls. There are 31 authentic entries, 25 returns, 185 recorded
reads and nine supplied mutations between calls. No resolver, first-player
accessor, allocation or lower actor body executes. Recorded process/player/list
regions remain unchanged by each lookup; native stack stores are outside that
purity comparison.

The required-input prefixes distinguish absent process custody and an owned
but unavailable DWORD, stopping at the first `0046B096` read. Diagnostics
retain native manager null, unknown player `+D4/+D8`, bypassed unknown `+D4`
and raw player-node aliases. A raw null head stops before the harness's mapped
zero-page support could imitate an empty list. These are bounded diagnostics,
not native fault or stop-IP parity. Native setup supplies manager/list and
player-field bytes without executing constructors. Portable registry inputs
are metadata only; the native lookup consumes no registry backing.

Histories cover source-null bypass, empty lists, raw-zero and stale authoritative
`+D8`, conditional full-DWORD `+D4`, registry/process null separation, absent
registry, duplicate and later matches, no match, and changes to the process
value or player words between calls. Return EAX is the complete player pointer
or zero, normalized to the declared player identity; Boolean AL is insufficient.
Independent regeneration reproduced fixture SHA-256
`B249898D8F5650B7DA00A2821F9B02B2C6CDFF9A50AC12DCE5E37B044F857BC4`.

```text
python check_player_actor_lookup.py --output .tmp/player-lookup.json
```

Use the existing Python/Unicorn environment for original-byte checkers.
No retail capture or debugger replay is part of this evidence. The reused
emulator's unrelated particle-pool seed runs before recording and is excluded.

The [Rust comparison](../../crates/inf-game/src/object_runtime/player_actor_lookup/native_receipts.rs)
passes in release mode for all sixteen admitted histories/24 calls, including
the three required-input prefixes. Four named diagnostic histories/seven calls
are skipped before typed construction. It checks full-DWORD result identity,
source-owned native read order, all native masks/list links, and unchanged whole
typed arena/process custody. Real player constructors precede declared `+D4/+D8`
inputs; their other fields are checked for immutability without claiming native
constructor-byte parity. Test-only registration under the player module permits
private player preparation without widening a runtime API.

```text
cargo test --release -p inf-game player_actor_lookup_matches_independent_native_histories
```
