# Infestation process state used by actors

This document owns startup and retained custody for process null handle
`0054E894` and raw network flag `0055F448`. Their actor, camera, completion
and particle consumers retain separate read and callback policies. This is
shared process state, independent of campaign selection or overlay leaf.

## Supported executable initialization

The supported original `Infest.exe` has SHA-256
`BF09F66E3685D4990D95DA13D4BE1CA7E293FB31CDDDCCC90BD575882921E4956`.
Its preferred image base is `00400000`. Both complete DWORDs lie in the
unbacked virtual tail of `.data`:

| Operand | Address | Offset within `.data` | Raw size | Virtual size |
| --- | --- | --- | --- | --- |
| Process null handle | `0054E894` | `00020894` | `00019000` | `00132C38` |
| Network flag | `0055F448` | `00031448` | `00019000` | `00132C38` |

The section starts at `0052E000`. PE loader zero initialization establishes
initial DWORD zero for each operand. The complete base relocation directory
at RVA `00266000` contains 73,836 bytes and 35,447 non-padding entries; neither
DWORD overlaps a relocation target. Rebasing therefore does not change these
initial values. This evidence concerns loader initialization; it does not
execute a game initialization body or establish that either value remains zero.

`FrontendExecutable::mapped` continues to expose physically backed bytes.
The runtime does not manufacture mapped zero bytes for the virtual tail.

Bounded code guards retain direct network reset/publication, null-value
copies, registry cache/configuration writes and the allocator's actual phase
read. The nearby registry pointer destination `0054E888` has only a DWORD
write in guarded `00495940`; the represented cache/configuration writes end
at `0054E894`, without overlapping it. Settings copies begin at `0054E8A8`
and cover 34 DWORDs through exclusive end `0054E930` under the clear-DF ABI.
These finite spans do not prove absence of every computed alias or writer.

## One retained portable owner

[`ParticleFactoryProcessState::native_startup`](../../crates/inf-game/src/process_particles/factories.rs)
initializes `network_flag = 0` and `process_null_handle = Some(raw 0)`.
It does not own or initialize session phase. The initializer runs once when
constructing the logical
[`PortSession`](../../crates/inf-game/src/session_platform.rs). Its
`factory_process` field retains subsequent changes across screen, world and
display replacement. A separately constructed `PortSession` represents a new
logical run; it does not prove that native globals reset on every session-object
construction in the same OS process.

This field replaces the previous retained network Boolean. Existing Boolean
request inputs derive `network_flag != 0` at their existing construction
points, preserving values such as `00000100` as present. These request scalars
are captured inputs, not additional process owners. This rule does not claim
that every native network consumer uses a nonzero test: raw comparisons such
as equality to one require their own recovered policy. Native network setup
`00493300` and teardown `00493650` remain separate unimplemented backend owners.

`FrontendProcessState` still owns frontend animation/colors. The particle pool,
actor registry, camera globals and mission record also remain distinct owners.
A registry's configured null handle cannot substitute for current `0054E894`.
Initial process task retention captures the startup owner's known null value
once; later changes to the process value do not rewrite retained task sources.

Actual current phase belongs to `SessionState` (`session +64C`), initialized
and updated by its session owner; requested transitions remain distinct.
`ParticleFactoryProcessState` has no duplicate phase field. Lookup does not read
phase. The consumed family host lends actual `SessionState` to the mandatory
factory `current_phase` getter, reached only after successful synchronous
notification and a fresh zero network DWORD. Online allocation bypasses it.
This separate manager loan never resets retained process fields or invents
phase 4; known non-4 still returns the native scratch record and unavailable
input remains a reached error. The [factory owner](PARTICLE_FACTORY_RUNTIME.md#selection-notification-and-construction-order)
and [session adapter test](../../crates/inf-game/src/session_platform/world_actor_behavior/tests.rs)
keep those contracts explicit.

## Reached camera and completion reads

The linked session, intro tick and retirement host borrow the same retained
state. Production [`NativeCameraInputs`](../../crates/inf-game/src/session_platform/camera.rs)
and [`WorldCompletionInputs`](../../crates/inf-game/src/session_platform/linked/intro.rs)
pass that borrow into the existing
[player lookup](PLAYER_ACTOR_LOOKUP.md). The lookup still captures the live
null value once at its reached `0046B096` read; no constant or registry fallback
replaces it.

Network completion returns before cleanup. Offline null-actor completion runs
cleanup and bypasses lookup. Supplied-actor completion needs the live null
after cleanup and before disposal or mission writes. Camera target loss needs
it before player fallback and the separate first-player `0046B060` query.
Setting the retained null input to unavailable preserves those callers'
existing feature errors and prior writes. Empty player lists and raw-zero
requests do not bypass a reached lookup input.

On-demand camera resolution bypasses the process read when the manager handle
already resolves, including a noncamera virtual +60 null result. A miss reaches
`00478E3D` before the camera H13 factory; loader, intro admission and shared world
Tick carry the same actual borrow through `CameraFactoryInputs.process`.
`OfflineWorldLoadInputs` keeps that borrowed owner separate from the owned
mission/player reload request. Headless fixtures explicitly supply their process
input; no compatibility wrapper invents startup state. Camera mode-zero factory
consumption and the required read are distinguished in [camera runtime](CAMERA_RUNTIME.md).

Selector-2 Chase captures current NULL again at `004218E4`, independently of
registry configuration and earlier acquisition. Ordinary missions use this
consumer through the shared world camera phase. Unavailable current state with
an existing camera therefore fails at the reached pose feature, preserving
earlier target refresh and allowing later particles and elapsed accounting.
The selector-2 special-view reader `00421D50` and its eye/look suffix do not
read current NULL. Their [camera policy](CAMERA_RUNTIME.md#selector-2-special-view-storage-and-reader)
bypasses that unconsumed operand; earlier factory acquisition, reselection and
target refresh retain their own reached requirements. This is a consumer-local
bypass, not a claim that an entire mode-four Tick needs no process owner.

Current production camera/completion services have no legal mutable access to
the borrowed process owner during those calls. Future services that can change
process operands must expose their actual mutable custody and preserve fresh
read order. Arbitrary raw aliases and unowned callback writes are outside this
composition; retaining a field does not establish those native effects.

## Particle source consumers

Intro retirement births, process particle ticking, briefing drawing and
software world drawing borrow this same retained owner. The shared
[implicit birth-source policy](PARTICLE_RUNTIME.md#implicit-process-source-and-late-failures)
reads current `0054E894` after actual allocation at each lower producer.
Their former eager registry-source checks are removed. Missing source retains
earlier allocation, RNG, sound and draw effects; nonemitting paths do not require
that input. Known zero and a value differing from the registry remain valid.
These reads do not copy session phase into retained process storage.

Shared actor construction now borrows this owner independently of registry
configuration and stores its current NULL at `+88` after handle allocation
(`0047A885..0047A88A`). Callers without a process owner must explicitly select
`RegistrySnapshotApproximation`; live child creation and direct selector93
construction use the actual process input. Target publication and resolver
registry policies retain their separate evidence bounds. The shared
[capture/release](ACTOR_CAPTURE.md) and
[particle factory](PARTICLE_FACTORY_RUNTIME.md) kernels can consume supplied
live process state, but retained custody does not attach their enclosing
production reactions, network encoder or missing actor services. Complete
intro/briefing and retail acceptance remain governed by
the Infestation objective.

## Reproducible static evidence

`check_particle_process_startup.py`
authenticates the complete executable, unchanged `.text`, PE headers and
relocation bytes. It parses all 323 relocation blocks: 35,447 HIGHLOW entries
with four-byte write width and 179 ABSOLUTE padding entries with no write.
Unknown relocation widths are rejected. Full-span intersection checks exclude
relocation writes crossing either DWORD from a neighboring address.

Its [fixture](../../crates/inf-game/tests/fixtures/particle_process_startup_native.json)
records two field proofs, fifteen aligned code guards totaling 446 bytes and
one 136-byte settings source guard. Independent regeneration reproduced
SHA-256 `F1065D2D3DFA29D3C2CEF5937124A534B0AA6BF9E0B0DB7749BB9FA7FE0C09F7`.
This is static original-byte evidence with zero executed native bodies. No
debugger replay, retail capture or exhaustive writer audit is implied.

```text
python check_particle_process_startup.py --output .tmp/process-startup.json
```

The [Rust static comparison](../../crates/inf-game/src/process_particles/factories/startup_native_receipts.rs)
passes in release mode, comparing the actual const initializer with the
two loader field proofs and checking the finite source/relocation metadata.
It preserves the fixture's zero-executed-body bounds. Its historical supplied
phase operand does not establish a process-owned phase field or execute the
manager getter.

The [session tests](../../crates/inf-game/src/session_platform_tests.rs),
[production intro tests](../../crates/inf-game/src/session_platform/intro/runtime/tests.rs)
and [factory tests](../../crates/inf-game/src/process_particles/factories/tests.rs)
cover retained state through real screen/world replacement and `install_display`,
unchanged task sources, raw network `100h`, supplied offline missing-phase
rejection, completion bypasses and production camera
loss with unavailable null distinct from registry zero. The camera test
destroys the followed helper before invoking the public view-mode setter;
queuing disposal alone keeps its handle live. Missing lookup input retains
the reached velocity/mode/`+187` writes. This supplied test schedule does not
establish native intro timing or complete helper lifecycle execution.
The ordinary optimized game build passes separately.

```text
cargo test --release -p inf-game --lib session_platform::tests
cargo test --release -p inf-game --lib session_platform::intro::runtime::tests
cargo test --release -p inf-game --lib process_particles::factories::tests
cargo test --release -p inf-game --lib particle_process_native_startup_matches_original_pe_evidence
```


## Authored surface-snap byte

Selector12 motion `00467DF0` reads process BYTE `0053A128` after its signed
lifecycle gate. The supported executable supplies authored `01` at raw offset
`0013A128` in physically backed `.data`. This differs from the loader-zero
DWORDs above: strict `FrontendExecutable::mapped` reads the original byte.
`PortSession.surface_snap_53a128` retains that sole value across screen, world
and display replacement; the actual world tick loans it to the
[Selector12 tail](ACTOR_MOVEMENT_TAILS.md). No actor/mission seeding supplies it.

The [complete-source guard](../../crates/inf-game/src/session_platform/selector12_surface_snap_native_tests.rs)
binds the executable/profile, file-backed byte and 585-byte motion body.
No recovered direct writer or static pointer alias exists in the inspected
raw sections/recovered C; computed or external writes are outside that claim.
A future authenticated writer belongs to this same retained owner and requires
reviewing the current supplied-value motion request's post-terrain read order.
Fixture mutations test retention; they do not establish a native writer.

## Retained furniture-radius word

[`ProcessFurnitureRadius`](../../crates/inf-game/src/terrain_runtime/furniture_radius/process.rs)
is a separate owner from `ParticleFactoryProcessState`. Its private WORD
`005483C0` starts through `FrontendExecutable::loader_zero_data(005483C0, 2)`:
`.data` starts at `0052E000`, ends its raw backing at `00547000`, and extends
virtually to `00660C38`. The complete WORD lies in that loader-zero tail;
`mapped` does not manufacture file bytes there. The
[native guard](../../crates/inf-game/src/terrain_runtime/furniture_radius/native_guards.rs)
authenticates the supported executable and these bounds.

`PortSession` constructs this owner once and lends the same mutable WORD through
mandatory `OfflineWorldLoadInputs.furniture_radius`. Screen, display and world
replacement do not initialize it again. The
[load-time producer](TERRAIN_RUNTIME.md#shared-furniture-radius-publication-004508e0)
performs its own reached reset before scanning current H12/H11/H8 and retains
earlier maxima on failure. Startup zero is not a radius fallback or a claim
that the WORD stays zero. The future `0047C1D0` query must consume this retained
owner together with its own required actor/resource/context inputs.
