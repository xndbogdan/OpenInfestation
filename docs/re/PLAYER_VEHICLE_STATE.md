# Infestation vehicle state copied into players

This document owns actor virtual `+64` class identity and the complete
`0046CBB0` vehicle-to-player copy. The private shared phases serve existing
camera selection, pickup and the consumed results-retirement suffix. The
[offline production lifetime](WORLD_LOAD_LIFETIME.md) preserves actual players
across loads; native attract playback remains separate.

The authority is the supported original `Infest.exe`, SHA-256
`bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956`,
its static disassembly and bulk/game_logic.c.
The complete `0046CBB0..0046CC49` body is 153 bytes, SHA-256
`bbed96668abe9c329ab0ae3202fdc2e6e7a8a7c7ff0ac6e93e3d73c2063c6c15`.
Source coverage establishes these policies; it is not a retail execution
or acceptance receipt.

## Pure actor vehicle query

[Canonical constructor/table evidence](../../crates/inf-formats/src/constructor_assets.rs)
contains 94 rows. The 65 actor-prefix
tables use only two `+64` callbacks: selector 2's `00520860` binds self
`00404610`; the other 64 bind constant NULL `00401300`. Both complete leaves
are three bytes. The 29 nonactor, short-table and other-ABI rows are excluded
from actor dispatch. Their raw `+64` words can be data or another interface,
even when they happen to equal an actor callback address.

[`actor_vehicle_component`](../../crates/inf-game/src/object_runtime/actor.rs)
authenticates the actual live actor and exhaustively selects its represented
final class. It returns the actual selector-2 payload borrow or NULL. Mutable
`+B4`, lifecycle, flags, position, behavior, item handles, player and process
state do not determine the class query. No constructor is admitted by the
native census alone.

The existing [camera adapter](CAMERA_RUNTIME.md#native-host-rotation-and-draw-publication)
uses the shared query, then reads vehicle byte `+1C0` for its special-view
gate. Camera reselection needs only `+64` identity and does not consume that
later camera field. Camera `+60` self and derived actor `+58` self are distinct
virtual policies; neither is a vehicle query.

Pickup retains its existing bounded admission: shared actors and selectors
16/30/88/90/93 reject without a vehicle copy; non-selector-2 derived classes
reach `VehicleViewBoundary`. Sharing the class policy does not widen that
caller or own its remaining grant, network, ID-case and HUD phases.

## Ordered copy and units

[`vehicle_snapshot.rs`](../../crates/inf-game/src/object_runtime/player/vehicle_snapshot.rs)
owns the complete copy into the actual supplied player. The source vehicle is
the actual selector-2 allocation; no serialized player, copied arena, temporary
mask or atomic save buffer substitutes for the live destination.

| Reached source | Native destination | Policy |
| --- | --- | --- |
| Vehicle DWORD `+1E8` at `0046CBBA` | Player DWORD `+144` at `0046CBC2` | Raw DWORD, committed before behavior query. |
| Fresh `+7C`, behavior byte `+10` at `0046CBCA/CD` | DWORD `+148` at `0046CBD1` | Sign-extend i8 to full DWORD bits. |
| Fresh `+7C`, behavior byte `+11` at `0046CBDB/DE` | DWORD `+14C` at `0046CBE2` | Independent reached query and signed store. |
| Sixteen fresh `+7C` queries at `0046CBF4`; record heads at `0046CC06` | DWORDs `+150..+18C` at `0046CC0A` | Each source record strides 32 bytes; copy only its raw first DWORD. |
| Three indexed item resolutions at `0046CC20` | WORDs `+190/+192/+194` at `0046CC2D/33` | Actual live actor contributes raw resource WORD `+30`; actual NULL contributes zero. |

Selector-2's canonical `+7C` is `004027A0`, a seven-byte leaf reading its
current behavior pointer at `+140`. The typed stable-owner domain rereads the
actual behavior at each phase. It introduces no host callbacks to imitate
arbitrary native vtable or pointer mutation.

`00412780..00412797` is the complete 23-byte indexed item helper, SHA-256
`ed6bb0b14128eea266d46297c915ecbd2e01d98d006726b67c6c21e14a7ecc0b`.
It reads the array buffer at `+08`, selects the 16-byte element at the supplied
index, reads that element's handle at `+04`, then calls `0047AD90`. It reads
no count. The copy consumes exactly indices 0, 1 and 2; extra handles are
unread. A missing typed element is unavailable backing, not NULL resolution
or an implicit zero item.

Player record heads use an i32 save projection, but copying preserves raw
DWORD bits. Item resource WORDs are not constructor selectors or class IDs.
The signed behavior bytes are separate from those unsigned raw fields.

## Error prefixes and actual owners

Each source read precedes its corresponding destination store. Missing
behavior retains the already written `+144`. A short behavior array retains
`+144/+148/+14C` and every earlier copied head. A missing later handle or
resolver error retains all those fields and the earlier item WORDs. Later
fields, other players, actor allocations, messages and input references are
not reset after a stopped copy.

Item resolution uses the actual registry, including its cache effects. This
slice retains the existing private player's resolver error mapping. A typed
error must never be treated as the native helper's successful NULL result.
The copy performs no all-source preflight, rollback, automatic cleanup or
retry. Existing pickup grant effects preceding the copy remain that caller's
own prefix.

This is a supplied successful-allocation, stable class/behavior/registry
domain. Typed backing errors contain unavailable storage; they do not claim
physical partial-scalar faults, raw aliases, concurrent pointer replacement,
arbitrary callbacks, allocator/free behavior or native SEH equivalence.

## Original callers and enclosing retirement

The original has three direct calls to `0046CBB0`:

| Call site | Independent enclosing policy |
| --- | --- |
| `00420BBB` in `00420B90` | Find the vehicle's player through `0046B090`, command `37h` with flag 0, then `0046A860` before copying the supplied vehicle. The later timer and event-10 broadcast are distinct. No `+64` query precedes this copy. |
| `0046B307` in pickup `0046B2B0` | Controlled-actor guard, fresh acquisition and `+64`, then live grant before copy; later network/ID cases, upgrades and HUD remain separate. |
| `0048E8EF` in world retirement `0048E890` | Map, mission byte 12 and effects first; capture the first player once, resolve its controlled actor twice, then `+64` and optional copy. The same player's message cleanup precedes manager cleanup. No grant or command `37h` is added. |

Retirement's first `0046AE60` result guards the copy. Its second result
supplies virtual `+64`, without a second native NULL check. Actual first-result
NULL or `+64` NULL skips only the copy and still reaches that captured player's
message. An empty player FIFO bypasses the whole player/message phase and
still reaches the manager.

The [retained player constructor](PLAYER_CONSTRUCTION.md) owns actual input
references and message identity; [manager cleanup](WORLD_CLEANUP.md) retains
that player FIFO. The [map allocation/lists](WORLD_MAP_RUNTIME.md) now have
retained process custody and explicit local retirement. The actual
[`PrepareIntroResults` caller](../../crates/inf-game/src/session_platform/intro.rs)
now consumes the [post-effects suffix](WORLD_CLEANUP.md#retained-player-results-retirement)
against its same arena: two genuine controlled resolves, canonical query,
optional copy, captured message cleanup, then manager drain. Required copy
storage keeps the earlier player/cache prefix and prevents later message and
manager work. Its port-only retirement status retains a typed stop without
retry while results continue.

Players, input references and message identities remain owned; stale
camera/respawn diagnostics are not repaired after actor destruction. Production
offline reload transfers the same arena and player-constructor carriers through
the [borrowed load owner](WORLD_LOAD_LIFETIME.md). `SavedImport` fixtures retain
their explicit saved-data policy. `LeaveWorld` `004871C0` later deletes actual
players and releases input references, a separate lifetime from retaining them
for another world.

The [query tests](../../crates/inf-game/src/object_runtime/actor/vehicle_component_tests.rs)
and [copy tests](../../crates/inf-game/src/object_runtime/player/vehicle_snapshot/tests.rs)
cover genuine class admission, signed/raw fields, actual NULL and live item
resolution, short backing and retained error prefixes. Existing camera,
pickup and offline spawn tests remain regression consumers. These checks do
not establish full enclosing teardown or retail
acceptance. The composed results suffix has its own
[tests](../../crates/inf-game/src/object_runtime/world_cleanup/retained_player/tests.rs).
