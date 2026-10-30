# World load and player lifetime

The offline session retains one actual arena, mission, creation-listener owner
and player-construction collection across initial Prepare, LoadWorld, results
and deleting LeaveWorld. Player input references stay in the same `InputRuntime`
until their actual destructor releases them. Saved export is a projection of
those players; it does not replace them with imported records on reload.

This authority describes the represented source domain, not complete native
heap, exception, network, audio or gameplay acceptance. The supported original
executable SHA-256 is
`bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956`.
The complete initial Prepare is `[00487410,004874C8)`, loader is
`[0048DC30,0048E11E)`, retained-world retirement is
`[0048E890,0048E90A)`, and deleting LeaveWorld is `[004871C0,0048722E)`.
The [mission constructor and load fields](WORLD_MISSION.md) and
[player constructor](PLAYER_CONSTRUCTION.md) own their respective backing.

## Actual owners and explicit import policy

[`WorldLoadOwner`](../../crates/inf-game/src/world_backend/owners.rs) holds
the actual arena, mission, start state, listeners, player FIFO/carriers and
timing ring. `PortSession::pending_world` retains this owner before construction
or loading can fail. `WorldSession::from_loaded` moves those same owners into a
loaded world only after successful metadata construction; `into_load_owner`
moves them back without exporting/reimporting a player or releasing inputs.

| Policy | Actual backing and use |
|---|---|
| `PreparedPlayers` | Genuine constructor carriers, messages, masks and input references retained by the production session |
| `SavedImport` | Explicit saved-data fixture policy using `import_saved`; it has no fabricated live-constructor carrier |
| Lower `CleanImport` | Preserves the early clean-arena admission and locally applies the mission prelude |
| Lower `RetainedAfterMissionPrelude` | Accepts the actual existing manager after the enclosing backend has applied the mission prelude before activation |

The modes are explicit enums, not empty-list or numeric sentinels. Standalone
renderer, actor and lower-loader fixtures retain their deliberate import
policy. An imported player cannot be passed off as a completed native
construction when a deleting destructor requires its actual input references.
The process map store, input runtime, residency, RNG and
[furniture-radius WORD](PROCESS_ACTOR_STATE.md#retained-furniture-radius-word)
are separate retained owners or scoped loans; no loader installs a hidden
default pool or new seed. `OfflineWorldLoadInputs` borrows the mandatory radius
owner independently from the manager and its immutable factory-process input.

## Initial offline preparation

[`world_owners.rs`](../../crates/inf-game/src/session_platform/world_owners.rs)
calls the shared [`prepare_player`](../../crates/inf-game/src/player_prepare.rs)
on the retained owner. The native sequence is gameplay owner `+5C2=1`, retain
the successful player allocation before any fallible constructor phase,
complete `00469D60`, clear armed `+5D8`, select the mission, then reset it through
`0048EBA0` with elapsed Clear. Ordinary offline selection is literal 22 after
the late full network-DWORD read. The supplied attract kernel shares the
authored selector, but production attract and network admission remain closed.
Neither Prepare nor the player constructor writes timer `+5D4` or handle
`+5DC`; those constructor-unwritten fields are not initialized by convenience.

The actual input source is the retained analog descriptor owner plus the
current supported executable's digital records. Opaque, monotonic port
callback contexts identify distinct player allocations; they are neither
player IDs nor native pointers. Acquired references and stopped constructors
remain in their actual carriers. A stopped Prepare cannot replay its prefix,
drop the carrier, or invoke an invented successful unwind/destructor.

Grant formatting receives an ephemeral colon/space projection of the same
retained CRT classification used by profile names. Binding and individual-byte
availability remain explicit. NULL-substitute `00545900` is separately unowned
and stays absent; a reached use still stops at its genuine lower boundary.
The actual process, executable and residency are borrowed without validating
all later grant inputs before earlier input/message/bit effects.

Only after the mission reset completes does the enclosing caller restore
the sole `SessionState` elapsed-seconds alias. Its scheduler remainder remains
unchanged. Export then selects the actual first FIFO player; this saved
projection is an extra fallible phase and does not select the newly allocated
player merely because Prepare returned its identity. Explicit debug mission
selection applies after the native reset.

## Borrowed LoadWorld

[`load_offline_world`](../../crates/inf-game/src/world_backend.rs) accepts a
mutable owner bundle and returns successful presentation metadata. Its
`RetainedManager` arm applies `WorldMission::begin_world_load` and clears the
start override before resource activation, matching the beginning of
`0048DC30`. It then activates leaf 48 followed by current selection plus 10,
prepares current terrain/header/presentation data, and calls the lower loader
with `RetainedAfterMissionPrelude`. `CleanImport` preserves its earlier
validation and lower prelude instead; no second prelude is silently applied.

During `AssetPreparation`, the backend lends the actual `WorldPlacements`
H12 runtime and retained `ProcessFurnitureRadius` WORD to the
[resident radius producer](TERRAIN_RUNTIME.md#shared-furniture-radius-publication-004508e0).
Current H11/H8 supplies each reached model radius. A typed radius error leaves
the reached reset/prior maxima in the process owner and stops this attempt at
`AssetPreparation`, before new prepared-state/presentation publication, map RNG,
placement reset or construction. Earlier activation and callback effects remain;
the loader does not rewind them. The already prepared pure terrain slope factors
and diagonals precede this scan, so this attachment does not establish complete
native `0044FDD0` fault order or its raw H15 publication.

The lower [`load_world`](../../crates/inf-game/src/world_loader.rs) constructs
and publishes the actual map before calling the real manager cleanup. Native
`0048DE41` stores mission `+04`; the following span captures the current manager
and calls `00478CD0` at `0048DE5D`. This is the manager pass itself, not
`0048E890` controlled-player snapshot/message retirement. Only normal manager
return permits completion reads, placement reset and subsequent construction.
An empty manager or absent player uses its genuine bypass; no player is inserted
to make those branches succeed. The shared lower driver freshly reads the
caller-owned attract BYTE for the
[per-placement component/speed/flag phase](ATTRACT_DEMO_RUNTIME.md#per-placement-attract-initialization),
after ordinary registration. Its offline player/respawn tail is shared by
ordinary and supplied-state attract loads. The production backend still admits
only non-attract loading. The [loading callback and file reader](LOADING_RUNTIME.md)
remain separate required owners before production demo admission.

Cleanup receives the same current process, executable, residency and optional
text projection. Base/visual or empty passes need none of those sources;
represented actor callbacks consume them lazily at their own reached phase.
Missing process, registry, text or residency therefore keeps already published
map/RNG/mission changes and earlier callback/destructor effects. The manager's
inhibit byte remains 1 on a stopped walk, and placement reset has not run.

`WorldLoadStatus` records the reached phase and a diagnostic stopped reason;
the call returns the actual typed error. A failed attempt retains the owner,
prepared source, map publication and all
actual players/carriers; it produces no successful loaded-world metadata or
stale fallback world. A second attempt is rejected rather than replaying
effects. Immutable terrain retained across activation must still authenticate
against the fresh current registration/allocation before its next consumer.

RNG custody transfers once from `LoadedWorld` to the actual platform process
owner before a new backend attempt. On error the platform remains authoritative;
an old loaded-world snapshot cannot overwrite those draws on retry or cleanup.
Success alone transfers the returned snapshot back into the loaded world for
its existing callbacks. Session elapsed/remainder and timing ownership retain
their original single lifetime; loading creates no second clock epoch.

## Results retirement and deleting LeaveWorld

Results first performs local map retirement, mission BYTE `+12` and represented
effects clearing, then the
[retained-player/manager suffix](WORLD_CLEANUP.md#retained-player-results-retirement).
It captures the first player once, makes both genuine controlled-actor calls
on the nonnull arm, queries canonical virtual `+64`, copies the reached vehicle
state and cleans that same message before manager retirement. Players, input
references and constructor carriers survive for reload. A logged typed stop
permits the existing results continuation while retaining its non-retry receipt.

`004871C0` is a distinct deleting lifetime. The session keeps the actual world
present through map/BYTE12/effects and the retained-player/manager pass. A
completed earlier results pass does not substitute for this new native call;
a stopped pass remains a boundary. Only then does it transfer the retired
loaded owner back to pending custody.

The admitted game-audio branch is constructor-proven NULL: game constructor
`00487BE0` stores `+5D0=0` at `00487C71`. Unavailable or present audio is not a
default successful cleanup. After this branch, Leave clears owner `+5C2`, runs
the represented one-shot sound cleanup and real particle cleanup, then repeatedly
selects the fresh first player and its unique actual constructor carrier.
[`destroy_constructed_world_player`](../../crates/inf-game/src/object_runtime/player/constructor/destruction.rs)
unlinks that player, destroys its represented message and releases its actual
digital then analog references. Shared external input references and queued
callback custody remain with the existing input owner.

Earlier deletions survive a later missing, imported, ambiguous, unpublished or
stopped carrier. Such a carrier is not converted into a fake completed
destructor. The outer Leave receipt prevents automatic replay. Only after the
FIFO loop does the fresh full network DWORD select the offline return or
required `00493650` network teardown. Normal offline completion clears the
saved projection; stopped operations retain it and their actual owner state.

## Remaining bounds and checks

Native allocator NULL, nonzeroing heap reuse, partial physical backing, SEH,
asynchronous source/metadata mutation, enabled message widgets, present game
audio and network teardown retain explicit required boundaries. The admitted
descriptor loans stay immutable and use the same `InputRuntime` until complete
release; they do not prove arbitrary native descriptor reuse without metadata
reads. Manager-header destruction, mission/game destruction and process-global
shutdown are distinct from this bounded Leave attachment.

`IntroCampaignReloadApproximation` still names actor clock 245, zero camera
settings and zero-filled fresh allocation WORDs where their native current
writers/allocator history remain unowned. Initial mission current world 1 and
BYTE `+13=1` come from the authenticated
[mission constructor](WORLD_MISSION.md#native-mission-constructor); they are not
captured defaults. Registry configuration and the represented statistics-record
allocation retain their explicit portable domain. Arcade driving, general
campaign/gameplay, initial attract playback and retail acceptance remain separate.

The [lower-loader tests](../../crates/inf-game/src/world_loader_tests.rs),
[backend owner tests](../../crates/inf-game/src/world_backend.rs),
[radius handoff and prefix tests](../../crates/inf-game/src/world_backend/radius_tests.rs),
[session owner tests](../../crates/inf-game/src/session_platform/world_owners/tests.rs)
and [linked session tests](../../crates/inf-game/src/session_platform/linked_tests.rs)
cover the authored import/retained modes, real current sources, stop prefixes,
owner transfer and reference/destructor custody. Their source coverage is not
a claim of test execution, original enclosing-function execution or retail
playback acceptance. Radius tests share one process owner across two actual
saved-import loads; their missing-H11 and later-zero failures are explicit
resident-producer domains, not a full-load failure or campaign transition.
