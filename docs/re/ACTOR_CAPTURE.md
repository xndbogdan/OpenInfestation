# Infestation shared actor capture and release

This document owns the reversible flag/list policy in `0047D7C0..0047D906`.
It is separate from [actor damage and lifecycle](ACTOR_RUNTIME.md)
and from selector-12's enclosing reaction `00468220`. Priority and complete
intro/briefing acceptance remain in the Infestation objective.

## Callback identity and storage

The canonical actor callback is virtual `+8C`. Of the twenty final actor
tables authenticated for [canonical interactions](ACTOR_INTERACTIONS.md),
all bind `+8C` to `0047D7C0` except selector-12 `00527058 -> 00468220` and
static selector-13 `005269C4 -> 00468B30`. `+AC` is a different callback.
Direct shared-body calls occur in wrappers `00458CC0`, `00468220` and
`004771D0`; owning this body does not own those wrappers or arbitrary virtual
dispatch. Static selector-13 still has no portable constructor.

The first argument DWORD is consumed through its low BYTE at `0047D7C0`:
zero releases, nonzero captures. Thus `100h` releases and `101h` captures.
The second argument is the actual source actor handle. The third points to
mutable caller-owned saved flags. It is independent from actor DWORD `+D4`.

| Storage | Owner and initialization |
| --- | --- |
| Actor `+6C` | Shared constructor owns the scalar; capture compares source before self |
| Actor `+70` | Capture source handle; unwritten constructor hole |
| Actor `+D4` | Capture's prior flags; unwritten constructor hole |
| Caller saved flags | Independent four-byte storage supplied by the caller |
| Process `0054E894` | Live process null handle, independent of registry null |

The inspected selector-12 constructor chain is `00467070 -> 00429E20 ->
00428650 -> 00425E60 -> 0047A7A0 -> 0047A490 -> 0047A030 -> 00479C60`.
It does not initialize actor `+70` or `+D4`; neither does payload initializer
`0047AC10`. Actor `+88` is a separate constructor-initialized null handle.
These actor holes remain unknown until their actual writers execute. No
allocator-carried zero, null or saved flags are inferred.

## Capture decision and mutation order

Capture first requires `(flags & C0h) == 40h`. Rejection returns false before
reading source registry, caller storage or player state. Otherwise actual
`0047AD90` resolves the supplied source, retaining registry/cache effects.
A resolved source's `+6C` is compared with self `+6C` in that order.

On equality, `0047D7FF` writes full current self flags to the caller's saved
DWORD, then `0047D801` reads fresh self flags. If `80000000h` is clear:

1. OR dirty BYTE `+38` with 8.
2. Write current flags OR `80h`.
3. Store prior flags in actor `+D4`, then source handle in actor `+70`.
4. OR dirty WORD with `2h` if the changed flags intersect `1FFh`.

The remote flag bypasses those four writes. Both paths subsequently clear
flags `104h` with low-`1FFh` dirty tracking, then call actual `00479E90(false)`
before `00479F20(false)`, and return true. Those membership setters always
remove and append their node, even when the bit value is unchanged.
`dispose` has a different order and additional lifetime effects.

An unresolved source or unequal `+6C` instead queries first player through
`00488AA0`. If present, it calls that accessor again and sends command
`0046C760(player, 42h, 0)` to the second returned player. The first lookup
is not a captured player owner. No flags/storage/list writes precede this
command arm; resolver effects remain. Its result is false after a completed
command. This composition still requires its lower command service; the
separate [player-command kernel](PLAYER_COMMAND_RUNTIME.md) owns the actual
body without attaching it to this caller.

## Release decision and lazy reads

Release first requires `(flags & C0h) == C0h`; rejection consumes neither
saved storage nor process null. It then reads the saved low BYTE for mask `1h`
and calls `00479E90(true)` only when set. A fresh low-BYTE read for mask `2h`
precedes the conditional `00479F20(true)` call. Unset bits do not reorder
their list.

Next, fresh self flags lose `80h`, with low-`1FFh` dirty tracking, and dirty
WORD mask `8h` is cleared. Only then does `0047D8D6` read the complete saved
DWORD. Unknown upper bytes must fail here after the earlier membership and
flag changes; they do not become zero. Equality with post-clear flags
returns false without reading process null or writing actor `+70`.

On inequality, OR the saved DWORD into current flags and track changed
low-`1FFh` bits. Only afterward read fresh `0054E894` and write actor `+70`,
then return true. An unavailable process null preserves all earlier writes.
Release never writes actor `+D4`.

## Portable and remaining boundaries

[`ObjectArena::apply_shared_actor_capture`](../../crates/inf-game/src/object_runtime/actor/capture.rs)
is an explicit shared-body API with one actor-bearing request, the original
operation DWORD and source handle. Its context borrows the existing process
owner and mutable caller saved storage; its host requires the reached player
command. There is no class-id or scene selection through a numeric sentinel.
The shared body uses actual arena actors, packed-handle resolution, first
player and common membership setters. Caller storage distinguishes an
unavailable allocation from separately unknown bytes; capture writes all
four bytes, while release retains its low-BYTE then full-DWORD requirements.
The process input borrows its existing owner rather than copying registry
null or adding another process-null state.

Caller-storage aliases with actor flags, dirty bits or `+D4`, unmapped raw
pointers, physical link aliasing, allocation reuse and arbitrary owner
rebinding remain separate source diagnostics. Canonical first-player reads
and membership operations do not admit invented mutable query callbacks.
Native physical pointers and faults are not typed output values.

Selector-12's `00468220` capture prefix uses the distinct
[live player lookup](PLAYER_ACTOR_LOOKUP.md), then still needs composition of
the [live grant Merge](GRANT_RUNTIME.md), three captured-player
[commands](PLAYER_COMMAND_RUNTIME.md) and status identity at `+228` before
this shared body. The broadcast call
uses ECX/EDX zero and stack `(22, 0, self + 32h)`; its zero-radius branch
invokes each receiver's `+98(0, 0, 22, 0)` without consuming the XYZ pointer.
The [canonical event-22 broadcast](ACTOR_GRANT_NOTIFICATION.md) now owns this
zero-radius delivery, including selector-12's live grant difference and
selector-2's mandatory player lookup. Other events, ranged delivery and the
surrounding class reactions remain separate.
Its release prefix loads the helper before
ORing BYTE `+22E` with `0Ch`, clears a present helper, publishes literal
`(NULL, 0)` to `+1EC`, `+274`, `+27C`, then calls its actual
[actor-list selection](FAMILY_TARGET_SELECTION.md#selector-12-actor-list-selection-callback)
before this body. At `004682FB`, assembly forwards the original third pointer;
the decompiler's rendering of it as the second argument is incorrect.
Selector-13 has additional effects and remains a distinct wrapper.
Production actor `+24`, unowned movement/tasks and allocator-carried timers
remain open. Isolated shared-body evidence does not establish complete
intro, briefing or retail gameplay acceptance.

## Focused verification

The [eleven focused tests](../../crates/inf-game/src/object_runtime/actor/capture/tests.rs)
pass in release mode. They distinguish raw `100h`/`101h` modes, constructor
holes and separate `+88`, registry-null resolution/cache effects, source
resolution before missing saved backing, ordinary and remote capture,
setter order and repeat reappend, low-BYTE failure before list changes,
late partial-DWORD failure after the release prefix, equality with unavailable
process null, unequal subset flags that still require process null, and
required-command effects retained on success or failure. Those command
effects are declared supplied mutations, not execution of `0046C760`.
The ordinary game release build is checked separately from test compilation.

## Original-byte evidence

check_actor_capture.py authenticates the
supported EXE SHA-256
`BF09F66E3685D4990D95DA13D4BE1CA7E293FB31CDDDCCC90BD575882921E4956`,
unchanged `.text`, FPCW `027F`, the complete shared callback and resolver,
both membership setters and first-player bodies. It also records all twenty
canonical final-table `+8C` bindings. Those table words are static evidence;
the outer selector-12/13 bodies and virtual dispatch are not executed.

The [fixture](../../crates/inf-game/tests/fixtures/actor_capture_native.json)
contains 28 histories/37 invocations and 106 authentic body entries with
95 recorded returns. Fifteen body histories contain 24 completed invocations;
eight bounded prefixes contain six storage/owner stops and two required or
failed command-service stops. Five named diagnostics retain unknown actor
flags, source/self `+6C`, unknown group backing bypassed by a cached source,
and caller storage aliasing actor flags. Two diagnostic bodies complete;
their results do not become portable admission. `0046C760` has one required
and two supplied service entries, with no executed command body. There is no
allocation/free call in these shared-body histories. The reused emulator's
unrelated pool seed occurs before recording and is excluded from this corpus.

Native setup supplies shared `00527C00` backing, handles, list links, players,
registry/cache and saved-byte masks. It does not execute constructors or
establish allocation-byte defaults. Recorded output retains all six common
list orders/node kinds, actor/player list custody, represented actor fields,
registry occupied-slot identities/cache, caller bytes and current process
null. Unused registry free-chain words, physical pointers, incidental full
EAX and raw faults/stop-IP equality are outside typed output parity. In the
missing-registry prefix, semantic registry/cache custody is unavailable even
though old native physical bytes remain diagnostic backing.

Cases cover repeated capture/release with raw `101h`/`100h`, remote holes,
all flag-restoration combinations, complete-DWORD comparison and high-bit
merge, strict eligibility bypasses, full-`+6C` mismatch, stale lookup, canonical
empty-slot null-cache mutation, unavailable saved allocation, partial saved
reads, lazy process null and explicit command effects/errors. In particular,
release can restore the saved flags and return false after reordering nodes;
false is not rollback. The independently regenerated fixture SHA-256 is
`A32E692AEDF2D06973F0AC0789363417A5D5CAA44043C1A2B8758D385EC2AF7E`.

```text
python check_actor_capture.py --output .tmp/actor-capture.json
```

Use the same Python/Unicorn environment as the existing original-byte checkers.
No retail capture or debugger replay is part of this evidence.

The [Rust comparison](../../crates/inf-game/src/object_runtime/actor/capture/native_receipts.rs)
passes in release mode for 23 histories/32 invocations, including all eight
bounded prefixes. The five named diagnostics are excluded explicitly.
It compares represented actor fields and unknown masks, saved storage, live
process null, all six common list orders/node kinds, actor/player custody and
occupied registry slots/cache. Supplied command mutations and their errors
retain the same prefix; unconsumed typed actor/player/process state is checked
separately. The test is registered under the registry module so declared
private cache preparation needs no wider runtime API. Native constructors,
unused free-chain backing and pointer alias diagnostics remain outside this
comparison.

```text
cargo test --release -p inf-game shared_actor_capture_matches_independent_native_histories
```
