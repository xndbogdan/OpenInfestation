# Idle attract demo ownership

This document owns the main-menu idle request, native demo selection, its
supplied-state cycle and remaining production dependencies. Phase IDs,
campaign selections, OVL leaves and actor selectors remain separate. The
supported original executable is `Infest.exe`, SHA-256
`bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956`.
Original addresses below refer to that executable and the corresponding
`bulk/game_logic.c` consumers; source evidence does not establish demo execution.

## Idle request and failure path

Frontend `00484F41..00484F7D` accumulates signed wrapping **raw scheduler
elapsed microseconds / 1000** into its idle count when the menu stack has one
entry. Strictly more than 60,000 ms sets process attract byte `0055F14C` and
requests session phase 9. A different stack size clears idle time without
clearing the attract byte. The digital `004852B0` callback can also arm the
same request by setting idle time to 60,001. These are intentional native
entry paths, not an arbitrary frame limit.

`00438380` preserves the first pending request. The portable scheduler polls
input before admission and performs admission before native screen queries,
reuse, destruction, transition effects and construction. The former production
admission returned `SessionBoundary::AttractDemo` for frontend phase 2 to
phase 9 with attract set. The SDL app reported this boundary as a bootstrap
failure, ending a healthy menu session after its first idle deadline.
`AttractDemo` is a portable boundary name, not an additional native phase.

## Native preparation and selection

`00487410`, with the actual game receiver in ECX, performs these phases:

1. Set game byte `+5C2 = 1`.
2. Allocate the `0x1A0` player and, when non-NULL, construct it through
   `00469D60` using the game's actual arena at `+8`.
3. Clear game byte `+5D8`.
4. Read attract `0055F14C` before network `0055F448`. Attract takes
   `00488C90`; ordinary offline preparation uses selection 22. Network has
   separate `00488A20`/`0054E814` consumers.
5. Reset the actual mission at game `+1B0` through `0048EBA0(selection, true)`:
   the [shared save-field reset](SAVE_FORMAT.md#shared-mission-reset) always
   clears progress and `+A4`; the nonzero argument BYTE also clears elapsed.

`00488C90` takes **no stack arguments** and uses the retained game receiver.
The preceding `PUSH 1` belongs to the later `0048EBA0` call; the decompiler's
`FUN_00488C90(1)` signature is incorrect. The complete selection body increments
game DWORD `+4` with wrapping arithmetic, publishes it, and resets it to zero
when the unsigned result is at least 11. It freshly reads the index to access
the eleven DWORDs at `0053C6AC`:

`[18, 2, 8, 6, 23, 17, 3, 9, 15, 4, 14]`.

Game constructor `00487BE0` initializes the retained index to zero at
`00487C0B`, so the first ordinary attract request selects **2**. Other exact
cases are `9 -> 10 -> 14`, `10 -> 0 -> 18`, and wrapping `FFFFFFFF -> 0 -> 18`.
This helper uses no RNG.
The body `[00488C90,00488CB9)` is 41 bytes with SHA-256
`40c70921df05b37fc17bad3bc5620ab18f069df3e3b4d678a9081ba396c8f6c2`;
the table's 44 bytes have SHA-256
`d306a64a1053d629bc215a697860ccb1988902c148efdb13a0c93d454dd063c2`.
The canonical [`FrontendAttractSelectionTable`](../../crates/inf-formats/src/frontend.rs)
decoder retains all eleven raw DWORDs, the source virtual address and its
section-relative byte span. It requires the complete 44-byte backing, without
restricting selection values or substituting defaults. `FrontendAssets`
[caches the parsed table](../../crates/inf-game/src/frontend_assets.rs) as
`attract_selections()`. The [cycle owner](#supplied-state-attract-cycle) and
[initial preparation](PLAYER_CONSTRUCTION.md#initial-preparation) consume
that same authored table through the complete `00488C90` arithmetic policy.
Initial preparation publishes owner1, retains the actual successful allocation,
executes the shared player constructor, clears armed, then selects and resets
the caller's actual mission. Offline and attract share that sequence; the
ordinary nonzero-network selection remains a late required boundary.
The retained analog source and explicit player teardown have concrete owners
in [player construction](PLAYER_CONSTRUCTION.md). The
[offline production lifetime](WORLD_LOAD_LIFETIME.md) retains actual descriptors,
input references, arena and player carriers through loading and enclosing
manager cleanup. Native allocation-NULL handling and production attract
admission remain separate. These kernels do not attach playback.
Neither index/table recovery nor loading one selected world proves the complete
demo. Reusing the engine intro's selection 22 would change the native policy.

## Required continuation

Phase 9 still constructs the real text-entry screen (`00459000`/`00459060`).
Its Tick `0045A8F0` immediately calls completion `0045AD30` when attract is
set; it does not wait as an inert placeholder. The completion's identity,
player and profile effects must retain their actual owners before phase 3.

`004874D0` then calls `0048DC30`. The loader publishes mission `+30` to
current `+A0`, requests leaf 48 and `current + 10`, and subsequently reads
attract. Its attract branch pumps loading callback `0040EDB0` and the separate
`00546920`, `0054691C` and `00546918` handshakes. The
[shared loading authority](LOADING_RUNTIME.md) owns their actual startup,
clock, effect aliases, ordinary presentation and file-read cadence. These
counters cannot be replaced with successful zero defaults. The lower
[per-placement attract phase](#per-placement-attract-initialization) has a
separate consumed owner; it does not admit those loading handshakes.

Gameplay `004875A0` has its own attract camera and lifetime branch
(`00487982..00487B2E`). Factory `004400D0` allocates `0x14C`, calls constructor
`00440010` (final vtable `005228F0`), then initializes that allocation through
`004600F0` with **H13 resource argument `0x34` (decimal 52)**. The argument in
EDX reaches `00460120`, which resolves its signed low WORD through H13; it
is not a constructor selector. The global `004603B0` dispatcher separately
uses selector `0x5D` (decimal 93) to allocate the same `0x14C` class and call
`00440010`, matching the [canonical constructor dispatch](CONSTRUCTOR_ASSETS.md#authority-and-dispatch).
Leaf 1's Base H13 bank registers slot `0x34` with header bytes `5C 00 4D 00`:
the tag 92 maps to constructor 93, and the separate initial H8 argument is 77.
This authentic descriptor agrees with the class. The bounded
[setup kernel](#attract-actor-placement) owns construction, placement and the
captured camera relation. The [supplied-state cycle](#supplied-state-attract-cycle)
composes its guards, first-use, timer and completion prefix; production demo
entry and lifetime remain separate.
The demo actor's `+24` callback
`00440170` consumes RNG `0048F060`, live actor traversal, target factory
`004825D0`, terrain sampling and motion. Its actual camera relation reaches
`00410E70`, camera byte `+184 = 0`, and `00410EE0(0)`.

The [supplied-state cycle](#supplied-state-attract-cycle) owns the retained timer,
authored selection and `00487560(next, 1, 0)` completion/request order. General
demo phase-8 loading remains outside the current intro-only 22-to-18 admission.
The captured camera must be non-NULL (`0048798F..00487995`) to enter either
demo setup or this timer; a NULL camera bypasses both without advancing it.
Gameplay menu `00443070` clears attract and requests frontend before continuing
its own world/player gates. The existing gameplay-menu recording has attract
zero; it does not establish this return path's execution.

## Per-placement attract initialization

The general [offline lower loader](../../crates/inf-game/src/world_loader.rs)
borrows the current native attract BYTE and freshly reads it after each
constructed object's ordinary registration. Rejected or skipped factories
bypass this phase. This is a caller-owned process value, separate from the
mission selection, OVL leaf and constructor selector. The enclosing production
backend currently admits only non-attract loads and supplies that known domain;
the lower driver also supports supplied nonzero attract state across levels.

[`apply_world_load_attract`](../../crates/inf-game/src/object_runtime/world_load_attract.rs)
owns `0048DF5B..0048DFB8`. Zero attract skips all object queries. Otherwise,
canonical object virtual `+18` returns the actual actor or NULL; a nonnull
actor's existing canonical `+58` query returns its derived component or NULL.
Neither callback invents a component for a selector. A nonnull component
reads the full signed DWORD `+144`; only a negative value is multiplied by
`-1000` with wrapping 32-bit arithmetic. Flags are then masked with
`FB9FFFFF` and the literal low-nine-bit change test preserves dirty-byte
bit 1. The mask clears only bits 21, 22 and 26, so these pure canonical queries
leave the low nine bits unchanged and do not introduce a dirty write.

The ordinary registration prefix precedes the fresh attract read, and reached
failures retain prior serial/publication changes. Attract alone does not select
a different player tail: native `0048E09F` still clears player `+11C`, sets
`+58`, then uses the fresh full network DWORD and settings `+10`. The admitted
offline driver retains its genuine respawn. Network camera attachment and
network spawn have separate policies; feeding a nonzero network path into an
offline respawn would be incorrect. Arbitrary vtable overrides, physical memory
faults, full loading callbacks and initial demo playback remain unadmitted.

The focused [phase tests](../../crates/inf-game/src/object_runtime/world_load_attract/tests.rs)
and [lower-loader tests](../../crates/inf-game/src/world_loader_tests.rs)
cover the supplied-state domain. These checks are source/port validation,
not evidence that the accepted recording executed an attract demo.

## Constructor 93 and shared XYZ copy

The [constructor-93 owner](../../crates/inf-game/src/object_runtime/actor/selector93.rs)
represents `00440010` above the existing shared `0047A7A0` actor. It preserves
the five zero DWORD writes at `+138/+13C/+140/+144/+148`. The first three
remain unnamed raw words; the later `00440170` consumer treats `+144` as a
full signed DWORD accumulator and consumes its signed low WORD for velocity.
`+148` owns a type-2 follow target, whose actor referent has a separate lifetime.
Destruction releases that target before shared handle, actor-list and common
node teardown. Clearing the retired Rust target is logical resource release;
`00440070` itself does not write a native NULL into that field.

Generic H13 construction uses the authentic selector-93 descriptor, shared
`0047AD70` initialization and shared actor commands. It requires the actual
actor inputs and listeners, without derived-class RNG or world-mask inputs.
Its canonical null component queries, mode-zero-only `+80`, false event-22
`+98`, void `+9C` and no-op `+34` after-load callback retain the recovered
policies. The new class does not turn `+24 = 00440170` into a successful no-op.

`ObjectArena::copy_actor_position` owns the shared `004404D0` copy of raw
XYZ WORDs. It changes no basis, velocity, flags, dirty state or target. The
two existing intro-helper calls at `00443C0E` and `00443CB3` use this same
owner; the original game also calls it for other helpers and the demo actor.
The typed complete-word request does not reproduce partial memory faults.

The [direct factory](#direct-demo-factory) preserves its separate publication
and resource order. Constructor admission does not establish demo entry or
retail playback. [Focused class tests](../../crates/inf-game/src/object_runtime/actor/selector93/tests.rs)
cover the owned state, initialization, position and lifetime domains.

## Direct demo factory

[`construct_selector93_direct`](../../crates/inf-game/src/object_runtime/selector93_factory.rs)
owns `004400D0` in the completed-constructor and decoded shared-payload domain.
The complete body `[004400D0,00440163)` is 147 bytes, SHA-256
`674c68d3435a1a661b4cd82c85a3345816ad84082a9cb6b1b18156e155612aa4`.
It has no stack arguments or selectable mode. The authenticated full-text direct
call scan finds `004879E9`; no literal function-pointer DWORD occurs in the
original sections. Computed indirect callers are outside that scan's coverage.

| Phase | Original owner | Portable custody |
|---|---|---|
| Allocate and publish | `004400D0` / `00440010` / `0047A7A0` | Genuine constructor93, actor handle and update/actor-list nodes |
| Capture process NULL | `0044011D`, then fresh `00460103` | Required current process owner; these arguments do not affect class93 context |
| Resolve resource | `00460120` | H13 argument `34h`, resolved after actor publication |
| Initialize | `0047AD70` / `0047A610` / `0047AC10` | Header+2 model and shared payload; NULL position/rotation, placement `FFFFFFFF` |
| Commands and completion | `00479E10` / `0047AAD0` | Store resource WORD 52, true shared guard, no suffix; extent-derived `+74` |
| Listener and final state | `00460120` | Clear dirty, moving-cursor notification, fresh network read, shared `+40`, initialized flag |

`004600F0` ignores its incoming third stack argument and rereads current
`0054E894`. It forwards six arguments to `00460120`: NULL suffix, NULL
position, NULL rotation, placement `FFFFFFFF`, mode0 and freshly read NULL.
The fixed class93 context consumes neither the wrapper's handle argument nor
the earlier factory capture. The shared constructor's separate `0047A885`
read **does** initialize actor `+88`; those consumers must not be conflated.
The actor allocation request now borrows the current process owner separately
from registry configuration. Direct admission accepts differing process and
registry NULL values; `+88` receives the former after handle allocation.
Unavailable process NULL remains an explicit host-input preflight before
publication. The immutable process borrow excludes callback aliases that
mutate it.

Generic construction continues to resolve H13 before allocation and destroy
failed initialization. The direct factory instead publishes93 first. Reached
required H13, model or listener errors return its actual identity and retain
the completed prefix; they do not become a successful NULL factory result or
invoke generic cleanup. H13 handler construction `00474D10` publishes tables
`005277B4/005277A0`, both with genuine `+0 = 004EFBF0` NULL return. Native
`00460120` reads entry+2 without a NULL guard after acquisition; a missing
resource cannot manufacture successful default data. Typed nonresident/source
errors bound that native dereference domain without claiming native faults.

The shared resolver distinguishes authored-class dispatch from the already
published93 target. A decoded shared-actor descriptor may supply model/payload
data without changing class93. Other decoded payload layouts and parser/raw
memory faults remain outside this initializer's typed source domain. No H13
bytes are patched, shifted or given presentation-dependent decoding rules.

The [shared actor completion phase](../../crates/inf-game/src/object_runtime/actor_factory.rs)
keeps class-specific commands and `+1C` outside its boundary. Ordinary actor
and camera callers retain their captured network policies, selector11's
additional dirty bit and enclosing cleanup. The direct current-source policy
reads full `0055F448` only after listener notification, matching `00460288`;
network-active class93 uses shared `0047E2E0` with the actual world-active input.
Native mallocNULL, arbitrary overrides/aliases and the outer AL-false
destruction branch are not exercised by the admitted new93/NULL-suffix domain.
The genuine shared guard is true here, so no fabricated rejection is exposed.

[Direct factory tests](../../crates/inf-game/src/object_runtime/selector93_factory_tests.rs)
cover constructor/source ordering, retained error prefixes, real listener and
registry custody, fixed-class payload interpretation, and network completion.
The [complete setup phase](#attract-actor-placement) consumes this factory in its
separate caller order, then attaches its target to the captured camera. Enclosing
camera acquisition/lifetime, initial state availability and phase continuation
still gate production playback.

## Attract actor placement

[`setup_attract_actor`](../../crates/inf-game/src/object_runtime/attract_placement.rs)
covers `004879CA..00487AE6` within `004875A0`: the complete placement phase
followed by target allocation and the captured camera relation. Attract false
and the earlier captured NULL camera bypass this phase and the timer. An existing
demo handle that resolves at `004879BD` also skips setup. First-use timer/armed writes
at `004879A5/004879AF` and that enclosing resolver are composed by the
[cycle owner](#supplied-state-attract-cycle), outside this setup kernel.
`AttractActorSetupState` represents only the actual object `+5E0` and handle
`+5DC`, without constructor defaults. An unavailable handle is distinct from a
known actual registry-NULL handle; completed setup publishes `Some(handle)`.
`AttractActorSetupRequest` supplies that
publication, genuine factory inputs, the previously captured camera ObjectId
and known current `CameraSettings`. The kernel does not reacquire or construct
a camera. Its complete span `[004879CA,00487AE6)` is 284 bytes, SHA-256
`8a4c2e8c33092106e3cc53afefee69076f6aa9c25011168572f105c4da74c8ec`.
The placement subphase `[004879CA,00487A8E)` is 196 bytes,
SHA-256 `ff9ce23fe90c4b34b5a294ffac11d260f8fd2e63648e5bc52b70e70f7af3ed61`.

The pool is **H1**, identified by handler-table entry 1 at `0053AA18`, whose
receiver is `0055EE80` and entry array is `0055EE84`. The original dispatch
count is 16. Startup `00474110 -> 00474190` installs temporary/final tables
`005275D4/005275C0`; both `+0` entries are `004EFBF0`, the complete
`XOR EAX,EAX; RET4` NULL leaf. Final `+08 = 0050ED90` loads four-byte entries,
advancing their pointer by four at `0050EDE9`. The canonical
[word parser](../../crates/inf-formats/src/words.rs) already retains each raw
DWORD, its local index and byte span for handlers 1/2/6. H1 slot 0's first
DWORD is a packed raw X/Z pair at this consumer, with X in LOWWORD and Z in
HIGHWORD. It is neither a count nor a header. Other H1 consumers assign
different meanings, including WorldStart's odd-slot signed heading WORD;
those policies stay outside intrinsic parsing.

| Reached phase | Source order and retained effects |
|---|---|
| Capture position | Read current H1 slot0 at `004879CA/004879CF`, invoke actual miss `+0(0)` only on a hole, then capture the full DWORD at `004879E3` before construction/listeners |
| Construct | Call direct93 factory `004400D0` at `004879E9`; a factory error preserves its reached actor/listener prefix without publishing caller fields |
| Publish | Store returned object to `+5E0` at `004879EE`, read fresh actor `+84`, store handle to `+5DC` at `004879FB` |
| Resolve again | Call real `0047AD90` at `00487A01`; its value is ignored, while required registry errors and cache effects remain visible |
| Acquire terrain | Read fresh H12 slot0 at `00487A06..00487A22`, after completed construction, publication and resolution |
| Place | Sample the raw cell/plane and call `004404D0` at `00487A89`, changing XYZ WORDs alone |

The [shared raw H1 acquisition](../../crates/inf-game/src/word_resources.rs)
uses the current `ResidentRuntime` registration,
region/local index and allocation, then checks the registered Words bank's
handler before returning its raw value. It does not use a renderer snapshot or
assume global slot equals local index. Its capacity, nonresident and registration
errors are explicit portable source boundaries. Attract's native slot0 path
does **not** read capacity `0055EE88`; it dereferences the miss result
unconditionally at `004879E3`. The typed capacity check bounds that raw table
domain without claiming native fault parity or a successful default source.
WorldStart retains its separate signed WORD-wrapped slots, override bypass,
public errors, heading conversion and +60 height offset. It keeps the native
inclusive one-past lookup as an explicit typed boundary when consuming the same
acquisition owner. H2/H6 caller policies
are not migrated merely because their records share a shape.

Terrain acquisition uses the existing `ActorTerrainHost` contract. The current
world host checks H12/0's complete registration and Arc identity against the
retained preparation, then returns the same prepared terrain with its current
cells/diagonals. Missing or replaced H12 retains factory, publication and resolver
effects before stopping; it does not undo or retry the actor. A missing H1 stops
before direct construction. The H12 startup tables also bind the genuine NULL
miss leaf, so neither required source can become successful zero data.

X/Z grid selection treats both WORDs as unsigned: shift right7, mask with
signed `(~005372F4) >> 7`, then use X*512+Z. `00487A56` reads signed height
BYTE, shifted by `005372F8 & 31`; only its signed LOWWORD participates in the
comparison. The full signed plane DWORD is arithmetically shifted by
`005483BC & 31`, then its signed LOWWORD is compared at `00487A68`. Choose
that WORD maximum and add literal 512 with WORD wrap. The owner composes this
from `TerrainRuntime::raw_height_at`, `plane_base` and `plane_shift`, under the
existing canonical prepared sampling domain. Interpolated `00450A30`, wave
adjustment, WorldStart's +60 and a full-i32 maximum are different policies.
The prepared sampling state does not establish every later native global writer.

Native `00487A2A` reads a DWORD spanning captured stack+1A and two adjacent
bytes, but its relevant consumers use only the low WORD as Z. The typed
complete-word request does not fabricate those adjacent bytes or claim stack,
alias or partial-fault parity. Its immutable residency/process loans also
exclude arbitrary callbacks mutating those same owners during the factory;
late terrain acquisition preserves the reached phase within that admitted
borrow domain.

### Captured camera relation

After the shared XYZ copy completes, `00487A90` calls `00511708` with size
`20h`. The existing required `NativeTargetAllocator` receives
`NativeTargetClass::ActorFollow`; no default-success supplier is installed.
`Ok(false)` represents the actual NULL allocation branch, while a service error
is `AttractActorSetupError::Allocation`. That error retains the completed
placement/publication prefix and stops before camera validation or mutation.

Only successful allocation reaches fresh `game+5E0` at `00487AA8` and that
actor's current handle `+84` at `00487AB0`. It then calls the genuine
`004825D0(handle, NULL point)` constructor through
[`follow_target`](TARGET_RUNTIME.md#actor-follow-construction-and-refresh).
The separately retained `handle_5dc` and earlier returned local object are not
substitutes for these reads. Live construction copies raw XYZ WORDs, adds
unsigned actor `+74` to Y with WORD wrap, and supplies both focus blocks;
velocity remains unwritten. Stale resolution produces kind2 with unknown
focus, without zero values or an eager refresh. Native partial heap stores,
allocation addresses and faults remain outside the completed-value domain.

The actual NULL branch bypasses these fresh publication and handle reads and
the new constructor, but still reaches primary replacement. An existing old
primary has its independent view-zero resolver requirement even on that branch. The camera is the earlier
captured identity, used at `00487AC2`; a fresh world-camera lookup, early class
preflight or replacement camera would change the reached prefix.

The consumed private selector16 `complete_attract_camera_relation` executes
these stores only after their preceding operations finish:

| Reached phase | Source order and custody |
|---|---|
| Replace primary | `00487AD1 -> 00410E70`: reset the old actor's view mode, release only its target resource, publish the new target or NULL, then apply settings `+68` LOWBYTE |
| Select pursuit policy | Store literal camera BYTE `+184 = 0` at `00487ADA` |
| Select smoothing modes | `00487AE1 -> 00410EE0(0)`: set eye block `+138` mode first, then look block `+15C` mode |

Replacement errors retain reached old actor, list, cache and new target
publication effects without applying the later byte or parameter stores.
Target destruction does not dispose or destroy its actor referent. Old family
view-zero policies can reappend flag0 membership and write actor BYTE `+D0`;
the shared camera APIs retain that class-specific behavior.

A live class93 target accepts only view mode zero. Nonzero First Person
LOWBYTE therefore falls back to zero and leaves camera mode `+188 = 3`.
A NULL or stale target has no live actor refusal, so a nonzero LOWBYTE instead
selects camera mode2. Raw settings `100h` and `101h` select distinct LOWBYTE
zero/one arms. BYTE `+184` is independent from target kind2 and the smoothing
mode DWORDs: zero selects the generic mode3/4 behind-actor policy. Setting both
parameter modes to zero does not initialize vectors, tables, snap, secondary
target or basis. The [camera owner](CAMERA_RUNTIME.md) keeps those later phases.

The request carries a known current settings copy unchanged across the phase.
The native replacement calls `00463AB0` after target publication and reads the
live First Person BYTE; the typed owner applies the supplied value under the
domain that the actual settings owner stays unchanged through the represented
synchronous callbacks. It does not execute the process settings getter or
admit arbitrary aliases that mutate that owner. The immutable residency/process
and exclusive publication loans likewise bound the source domain. Successful
placement guarantees object publication, so only the successful allocation arm
consumes that invariant. These bounds are not claims about original global
immutability, physical pointer identity or a measured Rust read timeline.

[Setup tests](../../crates/inf-game/src/object_runtime/attract_placement_tests.rs)
retain the placement source/error cases and exercise required allocation
true/false/error, live93 versus NULL LOWBYTE policy, old-family reset and
camera publication/error custody. Supplied private-phase stale, missing-registry
and invalid-class cases bound those lower operations; they do not assert that
canonical completed placement normally produces those inputs. No source writer
or native heap/SEH execution is inferred from fixture preparation.

The complete setup kernel supplies no actor, target, camera or RNG fallback.
Its required allocation service, enclosing camera acquisition/lifetime,
initial state availability, preparation/loading and later phase8/return transitions
remain separate production owners. The following cycle composes setup without
enabling idle admission or proving original demo execution or retail acceptance;
[AttractDemoDeferredApproximation](#attractdemodeferredapproximation) remains intact.

## Supplied-state attract cycle

[`tick_attract_cycle`](../../crates/inf-game/src/attract_cycle.rs) owns the
reached `00487982..00487B2E` policy with the actual arena, mission and
`SessionState`. `AttractCycleRequest` carries retained state, current attract,
the earlier captured camera, `SchedulerTick`, authored selections and genuine
setup/settings inputs. `AttractCycleServices` requires terrain acquisition,
target allocation and world completion services; no successful default callback
or production allocator is supplied. Its outcomes distinguish `Skipped`,
`Waiting` and `Advanced { next_selection }` without loading another world.

`AttractCycleState::from_native_constructor_fields()` represents only the
proven stores of the complete game constructor `[00487BE0,00487CC7)`, 231 bytes,
SHA-256 `d0571fd566f6b7b6cf98fee9356b87fe727fec43948a697092d1da7bc2800613`.
Startup requests ordinary heap storage through `00511708`; its HeapAlloc arm
uses flags zero. Neither the caller nor the game constructor clears the whole
allocation, so unwritten fields cannot acquire known zero defaults.

| Retained field | Constructor custody |
|---|---|
| `selection_index_04` | DWORD zero at `00487C0B` |
| `timer_micros_5d4` | Unwritten in the complete constructor; `None` is unavailable backing |
| `armed_5d8` | BYTE zero at `00487C77`; retain the full raw byte |
| `publication.handle_5dc` | Unwritten; `None` is unavailable, distinct from `Some(actual NULL handle)` |
| `publication.object_5e0` | Known NULL at `00487C7D` |

This field custody does not establish the earlier captured camera. Attract
false or captured camera NULL returns `Skipped` before reading the state or
validating any lower source. A non-NULL camera is not eagerly class-checked:
the live-handle path never reaches its setup operations.

If armed is zero, `004879A5` writes timer zero and `004879AF` arms it before
the handle read. `HandleUnavailable` retains those first-use stores; an unknown
handle cannot be treated as stale so that setup repairs it. A known current
handle reaches the genuine resolver at `004879BD`, retaining cache effects.
A live referent bypasses all setup dependencies, while stale resolution reaches
the complete setup kernel. Resolver/setup errors retain their prefixes and stop
before timer arithmetic. Nonzero armed skips the initial stores; an unavailable
timer is `TimerUnavailable` only after any reached resolver/setup effects.

The timer reads DWORD `+5D4` at `00487AE6`, adds the **capped** first scheduler
argument at `00487AEC` with DWORD wrap and publishes the sum at `00487AF0`.
The unsigned comparison at `00487AF6/00487AFB` advances only when the result is
strictly greater than `30,000,000` microseconds. Equality and wrapped low sums
return `Waiting`. This policy preserves the argument's full DWORD bit pattern;
it is separate from frontend raw elapsed/1000 idle time and does not advance
a second elapsed clock.

Advancement resets timer zero at `00487B03`, then calls the shared selection
body at `00487B0D`. Its wrapped index is published before the authored table
lookup. `00487B15` invokes the real
[`complete_world`](../../crates/inf-game/src/world_completion.rs) with
`next_selection`, start ordinal **1** and actor **None**. The wrapper reads the
current full process network DWORD at this reached phase. Any nonzero value,
including `100h`, returns `NetworkIgnored` before cleanup or mission writes.
ActorNone adds no process-NULL or actor lookup/disposal requirement.

Offline completion calls required `004800E0` cleanup, then `0048E820` on the
actual mission. `ProcessSoundRuntime` owns its represented flag-bit2 one-shot
queue; this is not an arbitrary native linked-list/address contract or a clear
of independent PCM voices, actors or particles. Mission completion retains
status/selection/start-ordinal stores before any missing-first-player error,
then resets the actual first player's checkpoint BYTE `+ED` and DWORD `+F0`
through `0046AF70`. Literal 1 is an ordinal stored WORD-minus-one, not a reset
Boolean. ActorNone cannot produce `ActorRejected`; backing/range and missing
player errors are typed required-source boundaries, not native AL-false or
hardware-fault claims.

Accepted completion submits its inner phase-8 request before armed is cleared.
Accepted and NetworkIgnored then both store armed zero at `00487B22` and submit
the outer phase-8 request at `00487B29`. Actual `SessionState::request_phase`
retains the first pending request; duplicate false is discarded, preserving
even an earlier Frontend request. A reached completion error retains timer
reset, index, cleanup and mission/player effects and stops before armed clear
or the outer request. No rollback, retry, forced phase or substitute mission
reset is applied.

Exclusive state and immutable process/settings/table/residency loans bound this
synchronous domain. Borrowing an owner does not eagerly consume its contents;
unrepresented native aliases or callbacks changing those same owners remain
outside it. [Cycle tests](../../crates/inf-game/src/attract_cycle_tests.rs)
exercise guards, required inputs, genuine setup/resolver, raw-versus-capped
timing, selection, real completion prefixes and pending-request custody.
This is source-backed typed policy, without original CPU/cadence, partial
memory-fault, complete game-constructor or retail-playback claims.

Production attachment of initial `00487410` player/armed/reset preparation,
establishment of
the current handle/timer and captured camera lifetime, phase9 completion,
attract loading/handshakes, general phase8 successor and input/cleanup return
remain separate. Neither this cycle nor its constructor-field initializer
attaches production demo admission;
[AttractDemoDeferredApproximation](#attractdemodeferredapproximation) remains required.

## Constructor 93 movement

[`ObjectArena::tick_selector93`](../../crates/inf-game/src/object_runtime/actor/selector93/update.rs)
owns complete virtual `+24 = 00440170` through its `RET4` at `004404C9`.
The body `[00440170,004404CC)` is 860 bytes, SHA-256
`0cac1a7237f19651dbef82b214dfa38e4b6d6a84989b1e43f5b80cc9a694daf7`.
Its request carries native microseconds, settings `+7C`, original sine/inverse
tables, basis math and optional current process up. Shared process RNG and
current H12 acquisition remain separate mutable owners. Context-free `update`
cannot manufacture these inputs and retains its boundary.

With NULL `+148`, the callback counts the whole current actor FIFO, draws once
through `0048F060`, chooses low16 modulo actual count, freshly traverses the
list, constructs a genuine type-2 target through `004825D0` and publishes it
only on return. The tail sentinel count is decremented; self, disposed actors,
flags, class and lifecycle are not filters. A one-actor list can choose itself.
Existing `+148` bypasses selection/RNG. Refresh `00482690` retains stale cached
vectors and the reached registry-cache effects; cached focus `+08` is distinct
from virtual focus `+14`. XYZ subtraction wraps WORDs before `00462DA0` distance.
Signed distance strictly below 2000 releases the child, explicitly writes
`+148 = NULL` at `0044029E`, and returns before terrain, up or motion.

Far updates use **H12/0**, whose handler-table entry `0053AA44` names `0055EE38`.
Both acquisitions are separate. The first captures XYZ before acquisition and
samples `00450A30`; the second freshly captures XYZ before its acquisition but
reads velocity X/Z only after it returns. The second sample adds those WORD
velocities with wrapping. Both clearances use captured signed Y minus signed
returned AX. The smaller signed clearance controls all later corrections.
The first low-clearance Y write does not replace that saved operand: below 300
raise live Y, then independently below 1200 add the shifted/wrapping correction;
above 1500 subtract the distinct correction. These writes do not mark dirty.

The shared `ActorTerrainHost` also serves Roam, whose raw-height sampling and
capture/target order remain distinct. Startup handler constructor `00474BA0`
reaches `00474C20`, publishing temporary/final tables `0052778C/00527778`.
Both virtual+0 entries are the genuine `004EFBF0` NULL return. A missing slot
cannot become successful default terrain. Production retains the original H12
registration and strong allocation from `WorldPlacements::terrain_source()`;
[each acquisition](../../crates/inf-game/src/world_actor_terrain.rs) freshly
checks current session residency, complete registration and Arc identity before
returning the same prepared terrain with its mutable cells/diagonals retained.
Missing and replaced sources are separate required feature boundaries.

Desired speed is signed distance/4 + 500, capped above 3000. Unsigned delta/160
limits yaw. Inverse WORD basis projection feeds `0048EF30` Q31 division then
the separate `0048F520` inverse-sine policy: wrapping negative magnitude,
signed shift21, signed original table WORD, full-DWORD inversion. Q15 saturation
and shift5 are different policies. Raw `i32::MIN` falls outside the supplied
1024-WORD table owner; this is a typed backing exclusion, not a native fault
claim. The actual division never produces MIN, so this consumer stays in range.
Rear-half angle adjustment and signed clamp precede exact X/Z basis rotation.

The callback then captures fresh basis-Y and current process up for the complete
[shared angular response](ACTOR_ANGULAR_RESPONSE.md), with delta shifted by
three. Only afterward does it relax full signed `+144`, sign-extending the low
WORD correction before a wrapping DWORD addition. Signed low WORD `+144` scales
fresh forward X/Y/Z through CRT truncation, storing velocity WORDs separately
in source order. Shared `0047B1E0` physics with mask `18h` runs last using the
original delta. Native `00440474` overwrites the consumed caller stack argument
with speed, while EBX retains delta; typed requests do not claim ABI-slot parity.

The [production callback pass](../../crates/inf-game/src/session_platform/intro/runtime.rs)
dispatches actual constructor93 ordinary `+24` independently of selector88
presence, preserving transfer-before-callback and list-owner swap. Alternate
`+3C` remains unowned. No intro and no93 retains the existing no-pass boundary;
other family prefixes retain their existing intro attachment. General phase8
player, collision and later object ordering remain separate owners. Reached93
errors retain their prefixes and stop that actor for this world attachment;
other callbacks, camera and particles continue.

[Movement tests](../../crates/inf-game/src/object_runtime/actor/selector93/update/tests.rs)
and [production tests](../../crates/inf-game/src/session_platform/intro/runtime/tests/selector93_tests.rs)
exercise genuine targets/registry, selection, near/far boundaries, mutating
two-acquisition read order, Y/steering/error prefixes, full-speed/WORD velocity
and original-delta integration. Direct allocation retains constructor dirty
bit2; shared target refresh can update the registry cache on a stale referent.
Native mallocNULL, arbitrary virtual overrides, raw aliases/partial faults and
whole original attract playback are outside this complete typed callback claim.

## AttractDemoDeferredApproximation

The production session currently defers exactly a **pending Frontend ->
Phase9 request with attract set**, before destructive transition callbacks.
It requires the actual ready frontend, resets only that controller's idle
counter, clears the attract flag and logs the named approximation once per
eligible idle interval. Missing/invalid frontend ownership remains an error.
This keeps the current screen, input context, resources, profile, music and
native tick callback alive; tick, draw and presentation continue in the same
scheduler iteration.

Shared `SessionTransitionAdmission::{Proceed, Deferred}` makes this decision
explicit. Only the driver cancels the deferred pending request and restores
requested phase to current phase. Admission errors still stop and retain their
failure prefix. Pure `PortSession::admit_transition` continues to report the
missing native capability; the consumed frame-admission policy owns this
separate approximation. Normal offline New Game and all other boundaries keep
their existing decisions. Native idle timing and first-request arbitration
are unchanged.

The approximation skips demo playback; it never claims native preparation,
completed transitions or retail acceptance. Its replacement owner is the
session/world runtime: establish actual initial player/state preparation,
phase-9 completion, attract loader/interface effects, demo-actor camera/task
lifetime, and general next-demo/return transitions before removing
the deferral. Share the existing terrain, actor and presentation owners where
their contracts match; do not special-case one selected world.

Focused driver and linked-session tests cover explicit cancellation, error
prefixes, repeated idle intervals, retained screen/input/resource/profile/audio
custody and subsequent navigation/New Game. The release SDL smoke crosses
multiple idle deadlines, then exercises both Quit and ordinary New Game through
physical key translation. Those checks establish menu recovery, not original
demo fidelity. Complete demo acceptance requires matched original entry,
selected-world playback, automatic next-demo handoff and input return.
