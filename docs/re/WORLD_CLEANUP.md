# Infestation world-manager cleanup

This document owns native `00478CD0` and the supplied-state
[`ObjectArena::cleanup_world`](../../crates/inf-game/src/object_runtime/world_cleanup.rs)
kernel. The pass invokes each represented object's actual virtual `+30` policy,
then its deleting destructor, while retaining the actual player FIFO and its
input/message components. The [retained-player suffix](../../crates/inf-game/src/object_runtime/world_cleanup/retained_player.rs)
now composes the post-effects part of `0048E890` in `PrepareIntroResults`.
Persistent reload, whole manager destruction and complete `LeaveWorld` remain
separate owners.

The authority is the supported, unmodified `Infest.exe` (SHA-256
`bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956`),
static disassembly and bulk/game_logic.c. The complete
`00478CD0..00478DF3` body is 292 bytes, SHA-256
`40c198dd01eb4f1b39270498a7ea598baa049910702308686b3c4ed983fd7b61`.
These source bounds describe instructions and canonical tables; they are not
a retail execution or acceptance receipt.

## Pass order and retained owners

The manager receiver is `M = G+8`. At `00478CD4` it reads the actual game
pointer from `M+70`, then `00478CD7` writes game `+1B8` / mission `W+08`
inhibit to 1. `M+70` is a game pointer, not a mission pointer. The pass visits:

1. Embedded `M+40` / `G+48` actor FIFO.
2. Heap headers `M+00`, `M+04`, `M+08`, `M+0C`: flag1 current/alternate,
   then flag0 current/alternate.
3. Embedded `M+10` / `G+18` disposal list.
4. Embedded `M+28` / `G+30` parked list.

Every loop selects the fresh current head, calls the object's mandatory virtual
`+30`, rereads that same selected node's payload, invokes deleting destructor
`+00` with argument 1 if the payload is nonnull, and restarts at the current
head. The pass does not remember a successor across destruction. Common
destruction unlinks all three memberships, so a later header must not delete
the same allocation again. Disposed objects still have flag0/flag1 nodes.

The normal tail rereads `M+70` and writes inhibit 0. There is no saved-inhibit
restoration: successful cleanup clears an initially nonzero value too. A
required typed source, callback, or destructor error leaves inhibit 1 and every
completed callback, deletion, registry release, or child-retirement prefix.

`M+58` / `G+60` is the independent player FIFO and is never traversed by this
pass. Its player identities, save fields, grant masks, live messages and acquired
input references remain owned. Common actor destruction releases its actual
handle before unlinking its actor FIFO node, then removes the common flag0,
flag1 and lifecycle nodes. The arena's generations and free-chain history remain
actual history; cleanup never resets a registry or replaces the arena.

## Canonical virtual +30 policies

The complete original constructor-table census contains 94 entries: 27 bind
`00401310`, 62 bind `0047EC50`, and one each binds `004053C0`, `00415BA0`,
`00435360`, `0043D160`, and `0048C850`. This is a native census, not a claim
that every one of those classes is admitted by the Rust constructor.

| Canonical receiver | Policy and reached dependencies |
| --- | --- |
| Base `00527B24`, visual `00527B68`, rotating `00527BB4`, terrain19 `00526978`, route51 `0051F658`, selector86 `005218CC` | `00401310` genuine no-op. No current process or mission read. |
| Annotation87 `00528AB0` | `0048C850` genuine RET. Common memberships are still destroyed afterward. |
| Shared actor `00527C00` and represented derived families, 11/12/16/30/34/36/60/77/88/90/93 | Common `0047EC50`; actual class custody binds the callback. Mutable selector-like fields never select it. |
| Vehicle2 `00520860` | `00415BA0` calls `0047DD20` with source actor, radius 0, event 10 and final argument 0, then common `0047EC50`. |
| Selector35 `0052273C` | `0043D160` lazily reads unsigned byte `+242`; zero bypasses `+243`, `+7A` and players. Otherwise unsigned `+242 > +243` skips grants. WORD `+7A == 0` also skips. The remaining arm iterates the actual player FIFO, rereads WORD `+7A` for each player, invokes complete `004750C0` on that player's `+9C`, then runs common `0047EC50`. |
| Selector66 `0051F6C0` | `004053C0` resolves actual owner handle `+88` through `0047AD90` first. NULL bypasses later fields. Otherwise it clears source DWORD `+138` low two bits, merges that full value and `00400000` into referent flags, marks dirty bit 2 when low nine flag bits change, then enables flag0 and flag1 in order. It never runs common `0047EC50`. The complete shared lifecycle is owned by [ACTOR_EFFECT_ATTACHMENT.md](ACTOR_EFFECT_ATTACHMENT.md). |
| Selector75 `00522008`, the [furniture marker](CONSTRUCTOR_ASSETS.md#helniazines-furniture-marker-selector-75) | `00435360` consumes byte `+40` and packed XYZ through required `00437110` geometry (`00450350`, `00437150`, `00450370`), on the world terrain and furniture state the cleanup lends (none stops there). Nonzero return skips mission; zero sign-extends byte `+44` and invokes `0048EAD0` with force 1, defeating manager inhibit. It never runs common `0047EC50`. |

Vehicle event 10 is the genuine zero-radius actor notification owner in
[`grant_notification.rs`](../../crates/inf-game/src/object_runtime/actor/grant_notification.rs),
distinct from [event 22 grant differences](ACTOR_GRANT_NOTIFICATION.md). Each
represented receiver returns full DWORD 0, but selector2 still performs its
actual actor-handle/player lookup first. Selector12's event10 arm bypasses the
event22 grant phase and `+29D` stores. A missing current process-NULL or another
lookup error stops before the enclosing common callback and destructor.

Selector35 consumes the complete [live grant setter](GRANT_RUNTIME.md), including
mask-write-before-error, required full current network, text/flags, H0/CRT,
actual same-player message gate and recursive grants. It does not replace that
owner with a mask merge or format-free notification. Unknown `+242` and `+243`
are separate reached boundaries; zero or out-of-order scalar arms remain lazy.

Common `0047EC50..0047ECD0` reads fresh full network DWORD `0055F448`.
Network 0 skips native reports. Nonzero network reports only when object flags
`+2C` have bit `80000000` clear and unsigned DWORD `+C4 <= 4`:

| Raw +C4 | Native 00492500 event |
| --- | --- |
| 0 | 6 |
| 1 | 5 |
| 2 | No report |
| 3 | 4 |
| 4 | 3 |

The current kernel exposes a reached `NetworkReportRequired` error for an
unowned report, retaining the actual actor and handle. Remote actors and
unsigned `+C4 > 4` bypass that service; LOWBYTE(network) is not the gate.
Afterward the callback sign-extends actor byte `+78` and invokes the genuine
[`WorldMission::record_placement_death`](../../crates/inf-game/src/world_mission.rs)
leaf `0048EAD0` with force 0 and current inhibit. Index -1 returns early;
other indices encounter inhibit 1 before quotient or current-world indexing.
This suppression is source behavior, not a constant-success replacement.

## Typed domain and failure custody

`WorldCleanupRequest` borrows the actual mutable mission and one current
`LiveGrantContext`. The same mission admits stable agreement of manager `+70`
and the native global game owner; immutable process/resource loans admit no
rebinding by callbacks. Process and grant sources are required only at their
reached reads. No player, input runtime, descriptor table or residency owner is
created as a fallback.

The kernel captures physical heap-header identities at their reached phase;
represented callbacks do not replace or swap those headers. After `+30` it
validates and rereads the actual selected actor or embedded membership entry,
then destroys that payload. Raw NULL payloads, pointer replacement, allocator
ABA, arbitrary virtual overrides and game/header rebinding lie outside this
immutable-payload domain. A missing selected node is an explicit error, never
permission to destroy a remembered allocation or a fresh head instead.

The lower [`ObjectArena::destroy`](../../crates/inf-game/src/object_runtime.rs)
keeps its existing class policies and errors. Camera16 retires `+18C` then
`+190` before shared destruction; constructor93 retires owned `+148` first.
Family descriptor and already-released target boundaries can leave earlier
helper or reverse-child releases intact. Selector60 can retire earlier rows
before a later dangling or aliased row stops. Common `+30` does not establish
that every native deleting destructor is owned.

Cleanup is not a resumable transaction. Retrying a failed pass replays `+30`;
retrying a stopped lower destructor may reach an already-released pointer.
There is no rollback, forced unlink, list clear, synthetic progress repair,
iteration-limit success, or automatic retry. One pause is resumable: a
destructor exit that needs world services the arena does not hold (a
Dormant Selector35's `0043E120`) returns `DestructionExitPending` after the
ship's `+30`, with the ship marked; the caller runs the exit, and
`cleanup_world` resumes at that ship's destructor, without its `+30` or the
retained-player prefix. A resume at another head is `ResumedElsewhere`.

## Retained-player results retirement

Complete `0048E890..0048E90A` is 122 bytes, SHA-256
`af9bf27cb71c3d52f92fd8a3cd71e0c108d8b81d844d1794872d785dd8c92328`.
Its manager is captured before the [local map phase](WORLD_MAP_RUNTIME.md).
The consumed [`prepare_intro_results`](../../crates/inf-game/src/session_platform/intro.rs)
caller retires that map, stores mission BYTE `+12=0`, then clears its represented
effects before `ObjectArena::retire_world_owners`:

1. `0048E8C4` captures the actual first player once. An empty FIFO bypasses
   player, registry, vehicle and message reads and reaches the manager.
2. `0048E8D1` resolves that player's controlled actor through `0046AE60`:
   actual `+D8` first, then `+D4` after NULL. NULL skips only the copy.
3. A nonnull first result reaches a second genuine resolve at `0048E8DC`.
   That result supplies the pure canonical `+64` query at `0048E8E5`:
   selector2 self or admitted nonvehicle NULL. Native has no second NULL guard;
   missing second typed ownership is a required boundary, not the first bypass.
4. A vehicle reaches the [ordered player copy](PLAYER_VEHICLE_STATE.md) at
   `0048E8EF`. Both NULL branches still clean the same captured player's actual
   message at `0048E8FA`. Constructor-empty `00413A20` checks backing `+38`,
   then children `+0C/+04/+08`; known NULL branches perform no field stores.
   A live `+0C` is deleted there and then (`00413A40..00413A44`, its scalar
   deleting destructor), so its menus' destructors run while the world's
   actors still live: a docked player's shop menu undocks from its factory
   (`004315E0`, `0042F2A0`) and clears the player's `+DC`. The port runs
   the prefix alone (`retire_world_player`), then the session's shown-menu
   drain, and drops, logged, any destructor a stopped shop left, so none
   reaches the next world. Required message storage stops before the
   manager.
5. Only afterward does `0048E901` reach the actual manager pass above.

Before 2026-10-21 the port queued those destructors past the drain. A shop
left open when the player quit to the frontend then reached the reloaded
world's first drain against the old factory, expired, and stopped every
shown menu there: the next dock locked the camera on the factory with no
shop to leave by (crash log of 2026-10-21 12:21, on Artoria).

The caller borrows its current process, executable, residency and an ephemeral
colon/space projection from the retained classification owner. Results and
LeaveWorld share one walk (`retire_loaded_world_owners`): both lend the
world's terrain and furniture loans a furniture marker's `+30` needs, and
both run a paused ship's exit over the world's damage owners. There is no
eager source admission or default NULL-substitute binding. Copy, message and
manager errors retain all reached scalar, registry-cache and destructor effects.
The player FIFO, input references and message identities survive; diagnostic
camera/respawn IDs stay unchanged even when their actors have been destroyed.

`WorldSession::owner_retirement` is port-only containment, initialized
`NotStarted` with a new world. The actual results caller stores `Completed` or
`Stopped(WorldCleanupError)`, logs a reached feature stop and permits the
existing phase continuation. It never retries a completed/stopped pass or
labels an error as native completion. This status does not own another native
field, broaden represented effects cleanup or establish arbitrary callback,
heap-fault or physical read-cadence parity.

## Enclosing boundaries and checks

Native callers include `0048DE5D` in loader `0048DC30` and `0048E901` in world
retirement `0048E890`. The retained loader now consumes the manager pass after
map publication and before placement reset; it does not call the controlled-
player/message retirement suffix. Results and the admitted deleting
`LeaveWorld` call that distinct suffix after map/BYTE12/effects. The
[world load lifetime](WORLD_LOAD_LIFETIME.md) retains the actual arena, players,
input references and constructor carriers through those separate callers.
`0048E820` world completion is another distinct owner. Complete manager
destruction `00478BB0` drains players and frees headers; mission destruction
`0048DAD0`, whole game destruction and process-global shutdown remain separate.

The [shared per-frame disposal owner](OBJECT_RUNTIME.md#per-frame-disposal)
now supplies `00479840` traversal to live updates and placement finishing,
with their distinct service/error policies. It captures the object across
virtual `+30`, whereas this manager rereads the selected node afterward.
Allocation-owned disposal stops also guard this cleanup and direct destruction
against repeating a partial callback or release. Neither a fresh arena nor
registry clearing substitutes for these passes. Arbitrary callback/storage
domains and retail acceptance remain open. See
[player construction and explicit teardown](PLAYER_CONSTRUCTION.md) and
[the enclosing runtime boundary](RUNTIME_FOUNDATION.md#first-world-boundary).

The [owning tests](../../crates/inf-game/src/object_runtime/world_cleanup/tests.rs)
cover empty and genuine no-op passes, physical headers and multi-membership
retirement, actor-first stops, signed placement/inhibit policy, full network
report gates, selector35 lazy reads and real grant prefixes, vehicle event10
lookup precedence, class-destructor stops, retained players and current input
references, and successful offline selector35 fanout through original Boot/H0
and CRT data. Test source coverage is distinct from execution and production
admission. The [retained-player tests](../../crates/inf-game/src/object_runtime/world_cleanup/retained_player/tests.rs)
cover the composed bypasses, copy/manager error prefixes and constructor-empty
message custody; actual results attachment is covered by the
[session tests](../../crates/inf-game/src/session_platform/linked_tests.rs).
