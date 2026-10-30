# Infestation live grant difference and selector-12 notification

This authority owns the read-only `004753A0` predicate and its selector-12
virtual `+98`, event-22 consumer at `00468BE0`. It is separate from grant
construction, recursive grant setters and player messages. Canonical
event-22 and zero-radius delivery belong to the
[actor notification authority](ACTOR_GRANT_NOTIFICATION.md).
Complete intro and mission acceptance remain in
the Infestation objective.

## Original callback and owner identities

The final selector-12 table `00527058 +98` points to `00468BE0`. Static
selector-13 table `005269C4` has the same binding, but its constructor/class
is not represented. This is a four-argument numeric callback, distinct from
the two-argument void [interaction notification at `+9C`](ACTOR_INTERACTIONS.md).

Event code 22 is the third stack DWORD, read at `00468BE0`. The original
callback captures its actual receiver, calls `00488AA0` at `00468BF1`, and
uses the actual first player. The accessor reads `0055F160`, manager `+60`
and the first list payload; it does not resolve an actor handle or read the
process-null handle. An absent player returns full EAX zero at `00468C37`
without reading the actor's grant mask or changing byte `+29D`.

With a player, the callback captures these embedded owner identities:

| Operand | Actual storage | Original preparation |
| --- | --- | --- |
| Source | Receiver actor `+24C`, mask `+250` | `00468BFA..00468C00` |
| Target | First player `+9C`, mask `+A0` | `00468C01` |

`00468C07` calls `004753A0` with this source-minus-target direction. A true
AL writes literal 1 to actor byte `+29D` at `00468C10`; false writes literal
0 at `00468C1D`. Both paths then zero the **full** EAX and execute `RET 16`.
The predicate's Boolean is not the callback's numeric contribution.
Neither mask is changed. Embedded grant vtable/notification pointers,
actor handles, clocks, RNG and other flags are not consumed by this branch.

The enclosing broadcast `0047DAF0` supplies receiver `this` and four operands
`(source actor, distance squared, event code, fourth raw operand)`. Event 22
ignores the first, second and fourth operands; they do not require owner
validation. The zero-radius capture call supplies `(0, 0, 22, 0)` to each
receiver. This leaf remains distinct from the now-owned canonical event-22
list traversal and its other class bindings. Event 13 can call `0047CE90`
and remains separate.

## Source-first reads and lazy ownership

`004753A0..004753DC` captures source and target once and scans IDs `0..AE`
in ascending order. It queries source first at `004753AE`. Only a source-owned
ID queries target at `004753BA`. The first source-owned, target-unowned ID
returns AL 1; exhaustion returns AL 0. Bits 175 through 191 are outside this
scan even though the six-word backing retains them.

The query `004752E0..0047531A` first tests the raw argument's low WORD. Zero
returns AL 1 before any owner storage is read. For nonzero it narrows to an
unsigned WORD, selects bit `id & 31`, and reads the DWORD at
`owner + 4 + 4 * (id >> 5)` at `0047530F`. Its result contract is AL, not
the upper EAX bytes. The difference loop supplies only IDs 0 through 174,
all within the represented six words.

Owner identities remain captured while each reached word is read from live
backing. An empty source bypasses a missing target beyond the ID-0 bypass;
an invalid source fails first at ID 1. The same source and target identity
is valid. Detached mask snapshots, word-wide equality and an eager fetch of
both owners would hide this read/error order. The normalized
`inf-formats::grant_is_owned` API retains its separate range contract.

## Portable scope and remaining boundaries

The [portable owner](../../crates/inf-game/src/object_runtime/actor/derived/selector12/grant_difference.rs)
provides `notify_selector12_grant_difference` with one actor-bearing
`Selector12GrantDifferenceRequest`. It authenticates a live actual selector-12
receiver, uses `ObjectArena::first_player`, and reads actual
`Selector12State.grants_24c` and `WorldPlayer.grants_9c`.
Mutable classification `+B4`, model, placement,
campaign and scene do not choose the binding. The actual actor/player owners
retain their masks; only the receiver's `byte_29d` changes on a present-player
path. No registry, process owner, grant table, residency or mutable lower
service is required by this leaf.
Its successful result is the original full DWORD zero. Required owner errors
retain their existing actor/player error types; the private embedded-owner
identity and read-only query do not become public compatibility APIs.

Raw stale/unmapped receiver calls that bypass payload reads, selector-13,
unknown numeric mask bytes, out-of-range standalone queries, physical aliases
and absent native manager backing have separate evidence bounds. They do not
become typed owners or known-zero masks. The represented callback does not
establish constructor execution or allocator history in a supplied native
comparison.

The [constructor and postinitializer](FAMILY4236D0_RUNTIME.md#selector-12)
have different grant consumers: first-player-to-actor merge, followed by a
single-ID `+7A` postinit query. Those policies remain independent. The
[enclosing capture reaction](ACTOR_CAPTURE.md#portable-and-remaining-boundaries)
still requires composition of the [live grant kernel](GRANT_RUNTIME.md),
enabled message owner, captured-player
[command kernel](PLAYER_COMMAND_RUNTIME.md) and status services. The
[player-message gate](PLAYER_MESSAGES.md)
owns live suppression and the early return, after required grant formatting.
The shared formatter owns complete bounded raw-byte
results and reached current CRT data. The live kernel separately owns the
setter, ordered Merge/Copy and canonical H0 string acquisition.
Its canonical zero-radius event-22 broadcast
is owned separately; other events and ranged delivery remain open. Production actor
updates, complete intro/briefing behavior and matched retail acceptance remain
open.

## Focused verification

The [eight ordinary tests](../../crates/inf-game/src/object_runtime/actor/derived/selector12/grant_difference/tests.rs)
pass in release mode. They cover absent-player preservation, actual first/second
player append order, live masks between requests, IDs 0/1/174 and ignored high
bits, source/target asymmetry, actual-class admission despite mutable `+B4`,
lazy invalid-owner errors and equal owner identities. Full typed object, player,
list and registry comparisons preserve everything apart from the expected byte.
No new production setter or mutable player accessor is introduced.

The [Rust/native comparison](../../crates/inf-game/src/object_runtime/actor/derived/selector12/grant_difference/native_receipts.rs)
also passes in release mode. Its nine admitted histories exercise 14 actual
portable callbacks, preparing genuine actors, append-only players and declared
input masks independently of receipt outputs. It compares the public result,
whole typed actor and player state, and list/registry/allocation custody.
Six named raw diagnostics are excluded before typed owner preparation. Its
separate receipt validator checks native read/return ordering across all
histories; it introduces no Rust cadence observer or measured Rust query-count
claim. The ordinary private-owner tests retain the lazy error coverage.

check_selector12_grant_difference.py
executes original `00468BE0`, `00488AA0`, `004753A0` and `004752E0` against
supplied actor/player/list storage. Four complete body hashes, thirteen exact
instruction guards, all twenty final-table `+98` bindings, unchanged `.text`
and x87 control word `027F` remain authenticated. No service or result is
injected. Its optional original `0047DBBB..0047DBC8` caller fragment stops
before the next fanout instruction; it supplies distance zero and establishes
the four-argument call without claiming the complete broadcast.

The [separate native fixture](../../crates/inf-game/tests/fixtures/selector12_grant_difference_native.json)
contains 15 histories/24 invocations: nine admitted histories/14 invocations and
six source-only diagnostic histories/10 invocations. Recorded requests enter
native routines 1,682 times and execute 1,675 checked returns, including
1,627 grant-query entries and 1,624 query returns. The 21 completed invocations
and three intentional unknown-DWORD stops retain cumulative byte masks and
supplied between-request changes. Inherited harness preparation is excluded
from these counts; no actor/player constructor executes in the receipts.

The native timeline pins ascending source-first reads, target reads only after
owned source IDs, the ID-0 bypass, the first difference and the strict scan
limit. Direct event-22 calls execute `RET 16` with stack delta 20; the original
caller fragment restores its pre-push stack. Raw query diagnostics execute
`RET 4` with stack delta 8 and preserve the AL-only contract, including
`10000h -> 10001h` before any owner read. Query 192 reaches the separately
unknown notification-pointer DWORD; it does not become a seventh mask word.
Unknown required bytes stop before `0047530F`; this is a supplied evidence
boundary, not a native bounds check or hardware-fault observation.

Independent regeneration reproduced SHA-256
`05D0D8BB44416B889DDD0AA2216B00B139D19182B17AA24A72B2CA3B8198C8CA`.
Raw selector-13, stale receiver, unknown mask and standalone-query diagnostics
remain outside typed callback parity and retail acceptance.

```text
cargo test --release -p inf-game --lib grant_difference::tests
cargo test --release -p inf-game --lib selector12_grant_difference_matches_native_callback_histories
python check_selector12_grant_difference.py --output .tmp/grant-difference.json
```
