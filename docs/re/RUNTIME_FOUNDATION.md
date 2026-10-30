# Infestation runtime foundation

This document owns the native port's startup, phase, resource-lifetime, and
first-world boundaries. The gameplay port was authorized on 2026-09-20.
Implementation priorities belong in the Infestation objective.
Binary formats remain owned by [FORMAT_DOCUMENTATION.md](FORMAT_DOCUMENTATION.md)
and the canonical [inf-formats crate](../../crates/inf-formats/src/lib.rs).
Capture acquisition and replay status belong in the
Infestation tracer ledger.

The findings below are static evidence from the supported retail `Infest.exe`,
its decompiled code, and `PreLoad.dat`. They do not assert that every branch ran
in the accepted TTD recording. The large recording is available for matching
these boundaries to observed execution; another recording is not a prerequisite.

## Evidence and naming

- bulk/game_logic.c: session construction, phase dispatch,
  scheduler, frontend, world loading, and gameplay update.
- bulk/ovl_handlers.c: preload construction, resource
  registration, resource handlers, and the clock adapter.
- bulk/fgdk_engine.c: phase request setter and common
  screen destruction.
- bulk/directx_wrappers.c: title-video startup.
- ovl_callers.c, scene_tree_functions.c,
  and scene_tree_helpers.c: focused loader extracts.

Ghidra's recovered signatures omit arguments at some register-based call sites.
The narrow static disassembly below supplies the missing selectors. The
`bulk_clean` name `g_model_entries` for `DAT_0065E734` is misleading: this is
the **scene-node lookup table**, indexed by overlay ID. It is not the
Handler-8 model registration array. Retain addresses when discussing it.

There are four distinct identifiers: session phase ID, overlay leaf ID,
campaign/world selection, and per-handler global resource slot. None may
silently substitute for another.

[Process actor state](PROCESS_ACTOR_STATE.md) owns the authenticated initial
null/network DWORDs and their retained portable custody. It does not duplicate
the current phase owned by the session below.

[Retained model scratch](MODEL_SCRATCH.md) owns the separate process byte arena,
its startup loader-zero proof, and explicit model-feature recovery policy.
Resource and display recreation preserve that owner.

## Startup and phase ownership

| Boundary | Retail owner | Proven responsibility |
|---|---|---|
| Application entry wrapper | `0x00437BD0` | Startup checks, game start/end logging, entry to `0x004F4DF0` |
| Platform loop | `0x004F4DF0` | Window initialization, Win32 messages, repeated `0x00438E30`, shutdown |
| Guarded runtime iteration | `0x00438E30` | Calls scheduler `0x004943A0`; owns error classification and display-recovery branches |
| Session startup | `0x004389C0` | Requests overlay 0, allocates the `0x668`-byte session, selects display setup, creates the initial phase-0 screen |
| Session constructor | `0x00438640` | Registers scheduler, initializes current/requested phase to 1, creates save owner, publishes `DAT_00546F2C` |
| Phase request | `0x00438380` | Accepts a request only when no transition is pending |
| Phase transition | `0x004383B0` | Queries class/reuse on the current screen, runs transition side effects, selects phase tick callback, constructs the next screen if needed |
| Phase tick adapter | `0x004394E0` | Stores first tick argument at `DAT_00533348`, forwards both arguments to the session's `+0x664` callback |

### Static construction and teardown

Before the entry runs, the CRT's `_cinit` walks the C++ dynamic
initializers with `_initterm` (`0051237E`, called at `0051229C`) over
`[0052E000, 0052E190)`: 99 game initializers after the null first word.
Fourteen build their object inline: the list heads (`004942F0`,
`00494320`), tables and other objects whose initial state the port keeps
in its own owners. The other 85 are thunks: `call` a constructor thunk
(`mov ecx, object; jmp constructor`) then `jmp` a registrar, or a single
`jmp`, or a bare `ret`. Each registrar passes a destructor thunk
(`mov ecx, object; jmp destructor`) to `atexit` (`00511D89`), which runs
them in reverse at exit. Those 85 initializers, their 58 registrars and
the 121 constructor and destructor thunks only they reach are compiler
glue, with no behaviour of their own: the constructors and destructors
they name stay counted. The progress report marks the 264 `not-needed`
(`compiler_glue.py` finds them from the table
and the code, not by pattern).

The session constructor calls `0x00487BE0` for its gameplay subobject.
`0x00487180` publishes that subobject at `DAT_0055F160`; do not assume this
pointer equals `DAT_00546F2C`. The world/player accessor `0x00488AA0` reads
the first object from the list at `DAT_0055F160 + 0x60`, returning zero when
empty. This is separate from the session's screen list at `+0x48`.

| Session-relative offset | Meaning established by consumers |
|---|---|
| `+0x48` | Current screen/list ownership consulted during transitions |
| `+0x64C` | Current phase; initialized to 1 |
| `+0x650` | Requested phase; initialized to 1 |
| `+0x654` | Pending-transition flag; initially zero |
| `+0x658` | Gameplay elapsed-time remainder used by `0x004388D0` |
| `+0x65C` | Set by `00438749` when the `004631E0` settings load returns; the phase-0 constructor and accept test it |
| `+0x660` | Save-system owner created through `0x00449A70` |
| `+0x664` | Phase update callback |

`0x00438380` is **first request wins while pending**. At transition end,
`0x004383B0` copies requested to current and clears pending. It calls the
current screen's virtual `+0x18` class query (discarding its result), captures that screen's `+0xE8` context,
then calls `+0x1C(requested)` on the same captured screen. A false return
causes deletion through `0x00438110`. Every replacement factory receives
the captured context in EDX and the session in ECX. Base screen `00437F90`
stores the factory's second argument at `+0xE8`; startup constructs that
scheduler-context object through `004382F0`/`00494630`. It is not an OVL ID
or an established portable window handle. The class query is not an exit hook;
its IDs differ from phase IDs. See the [screen-class table](FRONTEND_RUNTIME.md#screen-classes-and-virtual-calls).

New screen construction occurs only when the actual screen collection is
empty after side effects, including when another screen survives deletion.
The reuse callback sees the original request even when phase 9 later rewrites
it to 3. Side effects run on accepted reuse too. Requests made inside any
transition callback remain subject to the pending flag, including requests
made by the replacement factory. Final `004F0340` synchronizes every input
consumer's 64-bit baseline timestamp and accumulated held duration, and discards queued input by copying the write
cursor to the read cursor; it does this before publishing the new phase and
clearing pending. It is not an OS message pump.

### Phase dispatch matrix

All factory mappings are directly visible in `0x004383B0`. Roles marked
provisional are descriptive investigation labels, not enum semantics proven
by a complete UI traversal.

| Phase | Screen factory / constructor | Tick selection / evidence |
|---:|---|---|
| 0 | `00433810` / `00432500` | Startup (language) screen; requests overlay 2; its constructor requests phase 1 when `+65C` is set, otherwise its accept does ([frontend runtime](FRONTEND_RUNTIME.md#phase-0-startup-language-screen)) |
| 1 | `00404A10` / `00404620` | Title video; `00404890` opens `Title.avi`, `004049C0` requests phase 2; callback `004A4AF0` |
| 2 | `00483430` / `004838A0` | Main frontend; explicitly requests overlay 47 and constructs menu data at `0053C088`; callback `004A4AF0` |
| 3 | `0040A2E0` / `0040A420` | World preparation/briefing candidate; transition invokes `004874D0`, which invokes world loader `0048DC30`; callback `004A4AF0` |
| 4 | `00440C60` / `00440990` | Gameplay screen; callback `004388D0` invokes world update `004875A0` |
| 5 | `00440D20` / `00443190` | In-world menu candidate; installs `00438950` when `DAT_0055F448 == 0` |
| 6 | `00440C60` / `00440990` | Reuses gameplay screen factory but does not select phase-4's tick; same conditional callback rule as 5 |
| 7 | `00450FC0` / `00451020` | World view/map candidate; same conditional callback rule as 5 |
| 8 | `0042A970` / `0042AB00` | [Results/progression screen](RESULTS_RUNTIME.md) with save operations; callback `004A4AF0` |
| 9, 10, 11 | `00459000` / `00459060` | Related text-entry screens with phase-dependent initialization; 10/11 install `00438950`; 9 installs `004A4AF0` |
| 12 | `00444AC0` / `00444BD0` | Menu screen built from `00535E40`, retaining previous phase; callback `004A4AF0` |

There are intentional asymmetries. Entering 9 when `DAT_0055F448 != 0`
rewrites the requested phase to 3 and runs its preparation. Entering 5/6/7
with that global nonzero preserves the prior tick callback. Phase 0 has no
callback-selection case. That global means an active network session:
`00493300` sets it after creating three network workers, while `00493650`
clears it before peer/backend release and worker joins. Setup `00493D90`
uses the executable's DPLAYX import. Read it at the branch that consumes it;
phase-2 world cleanup can clear it. Screen-role labels still require care.

The transition's audio helpers have narrower responsibilities than their
surrounding phase changes: `00487360` selects frontend music,
`00487250` selects world music, and `004872A0` resumes/stops it according to
settings. Preserve their native calls and request-index conversion; e.g.
`00487310(7)` creates a kind-1 request with track word 8. The downstream MCI
consumer now proves physical CD track 8; source and playback limits are in
[audio runtime](AUDIO_RUNTIME.md). Entering gameplay always sets the first world-list object's
`+0xF4` timer to 500,000 after those conditional audio calls and before
selecting the gameplay callback.

### Time contract

`0x004A4890` computes `timeGetTime() * 1000` into a 64-bit destination.
Scheduler deltas therefore use **microseconds**, with the original clock's
millisecond granularity. Session construction passes `8000` and `125000`
to `0x004944D0`; `0x004945A0` gates updates below the former and clamps the
delivered update delta to the latter. These are 8 ms and 125 ms, not a fixed
simulation frame rate.

The scheduler calls virtual `+0x08` before `+0x04`. The session vtable at
`0x005221B4` resolves those to phase transition `004383B0` and tick adapter
`004394E0`, respectively. This places a pending transition before that
session's next tick. Eligibility gates input, transition, and session tick
together. Selected input is polled through `004F0200` before transition.
The transition occurs before setting the session busy flag, so it may replace
the screen that the later pass will use. Screen construction through
`004946F0` rejects replacement during a session tick or linked pass.

`004945A0` subtracts only the low 32 bits of the clock samples and compares
the result as signed. A subthreshold result clears raw elapsed to zero and
retains the last timestamp. An eligible update receives capped elapsed as its
first argument and uncapped elapsed as its second; the pre-callback clock
sample is committed after the session tick. Excess above 125 ms is discarded,
not carried into catch-up ticks. Signed wrap/backward-clock behavior must not
be replaced with saturating `Duration` arithmetic.

`004943A0` finishes every session first pass before starting its second list.
Both global lists run newest first: `004944D0`/`00494630` prepend entries
to the head/tail lists initialized by `004942F0`/`00494320`.
That pass uses the linked screen/context's session elapsed fields, skips zero
raw elapsed, and invokes current linked virtuals `+0x04`, `+0x08`, `+0x0C`
in order. It rereads the linked object between calls; the latter two receive
the context's `+0x04` backend value. Base `+0x0C` at `00494830` calls
`00495890` on that value. These are separate screen/backend callbacks, not
extra gameplay updates.

Suspend `00494430` and resume `00494480` broadcast to all sessions, then all
linked contexts. Session callbacks only set/clear `+0x60`; linked callbacks
deselect/select input through `00494810`/`004947F0`. The clock is not reset
and eligibility is not suppressed. `004F4980` can deliver these notifications
directly for activation/menu-loop changes or defer them to `004F4DF0`;
platform message coalescing remains a separate owner.

Gameplay `004388D0` forwards both timing arguments to `004875A0`, then adds
the capped delta to signed wrapping remainder `+0x658`. It increments the
wrapping seconds counter `+0x248` while the remainder is **strictly greater**
than 1,000,000. Exactly one second stays in the remainder. The player callback
`00438950` does not advance that counter; it performs the player's selected
object work and always finishes `00487B90`, even when no player exists.

The seconds DWORD is the serialized mission's `W+34`, not a separate timer:
`0043868E` passes session `+64` as gameplay `G`, and `00487C26` constructs
mission `W` at `G+1B0`, making `W+34 = session+248`. Serializer `0048EC46`
reads that same `W+34`. The port's `SessionState::gameplay_time` owns seconds
and the subsecond remainder; the gameplay callback mirrors its successful
post-update seconds into the mission save view. Initial mission loading
restores seconds while preserving the session remainder. The intro's phase8
and selection18 briefing retain both values without advancing them, and the
reload request exports the authoritative seconds before replacing the world.

### Implemented control and timing

[`SessionController`](../../crates/inf-game/src/session.rs) implements the
request/transition matrix, typed factory selection, context transfer, callback
selection, and gameplay time accounting. Its required `SessionHost` methods
retain actual screen and world custody. There is no successful placeholder
host for unimplemented factories, audio, or simulation. Application startup
creates screen 0 specially while current/requested remain 1.

[`Scheduler`](../../crates/inf-game/src/scheduler.rs) implements the two
passes, signed clock arithmetic, threshold/clamp, callback order, and explicit
activity delivery. [`ScheduledSession`](../../crates/inf-game/src/session_driver.rs)
connects those passes to the controller and requires the linked screen/input
callbacks. Multiple owners must run all first passes before all second passes;
each native list runs newest first. The concrete registry below supplies the
native attachment and busy-flag rules for production hosts.

Host errors stop the combined session owner rather than replaying partially
executed effects. This is explicit Rust failure containment, not rollback or
a reconstruction of retail nonlocal-jump recovery. Digital/untimed analog
input, base screen repeat, and screen registration have concrete owners.
The SDL frontend now runs these passes through the production linked session
below, including its physical keyboard, audio, CPU rendering and presentation.
Title, network and world owners remain incomplete; this is a working frontend
and name-entry path, not a playable game. The resource CLI remains headless.

### Implemented screen-registration owner

[`ScreenRegistry`](../../crates/inf-game/src/screen_registry.rs) owns distinct
session, context-owner, and screen handles, with generations and per-registry
identity checks. It preserves the native global/member ordering and requires
real binding-application callbacks for selection. Actual binding state lives
only in [`InputRuntime`](../../crates/inf-game/src/input.rs), independently
of selected-screen identity. The [input authority](INPUT_RUNTIME.md) documents
shared binding-set lifetime, consumer references, queue mutation, and timing.

Screen construction prepends to the scheduler member list (`004946F0`) first,
then creates the input context and analog references, then prepends to the distinct
session-screen collection (`00437F90`). `attach_scheduler_member` and
`link_screen_membership` expose that order. The linked base stores session at
`+0x10`, the second-pass context owner at `+0x14`, and input context at
`+0x18`; the screen mirrors the first two at `+0xE4/+0xE8`. Separately, that
second-pass owner stores backend/display context at `+0x04`, a busy flag at
`+0x08`, and its current linked screen at `+0x18`. Keep those two `+0x18`
fields distinct. Retail attachment checks session busy before list insertion,
then checks context-owner busy before publishing its current-screen link.
Prevalidating both in Rust is an explicit failure-containment choice.

Base destruction (`00438080`) first removes session-screen membership, then
releases analog references and the input context. Only the final scheduler-member
destruction (`00494760`/`004947B0`) unlinks and clears matching selected input,
without a symmetric busy check or restoring/clearing the second-pass owner's
current-screen pointer. A portable registry must preserve that asymmetry
without leaving dereferenceable stale handles; resolving such a link returns
an explicit error. `004947E0` installs the input
context and invokes virtual `+0x10`; base `004947F0` applies input bindings
through `004F03B0` before selecting the linked object. `00494810` clears
selected identity only on a match, but **always** calls `004F03B0(NULL)`, even
when deselecting a different object. Input selection and active bindings
therefore need separate state.

Screen deletion does not release the context owner or its backend. The
`0xA08` recovery path in `00438E30` destroys/recreates the backend through
`00438340`/`00438AD0`, then `00438370` updates `DAT_00546F28 + 4` on the same
context owner. Screens retain that owner's identity across backend replacement.
Resolve its backend afresh for each linked presentation callback; the generic
context destructor `00494670`/`004946B0` itself only unlinks.

## FGDK modules

A module is a record `{count, table}`. Its table holds `{acquire, release}`
pairs: the first is the module's own initialise and clean-up, the rest its
dependencies, up to a zero. `0048EE20` acquires: it counts up, and on the
first acquire takes each dependency in table order, then initialises the
module; a failure unwinds the dependencies already taken, in reverse.
`0048EEC0` releases in the mirror order. A dependency entry is a two-
instruction thunk, `mov ecx, MODULE` then `jmp 0048EE20` (or `0048EEC0`);
the boot and exit code (`004740A0`, `00474D10`, `004F4DF0`) call a few
directly. The port models the sound modules' lifecycle as retail runs it
([`process_sound::lifecycle`](../../crates/inf-game/src/process_sound/lifecycle.rs));
elsewhere its owners construct each module's state themselves, so the 150
thunks are omitted as glue, while every module's own initialise and
clean-up stays counted.

## Scene tree and resource lifetime

`0x004EDEA0` allocates handler slot arrays from `PreLoad.dat`, builds the
scene tree, publishes root `DAT_0065E730` and lookup `DAT_0065E734`, and
activates the root. Leaf construction `0x004EE260` owns four request/output
pairs. `0x004EE1E0(overlay_id)` first asks the parent to activate the path,
then loads the leaf only if its active byte is clear.

| Node-relative offset | Meaning |
|---|---|
| `+0x04` | Active byte |
| `+0x08` | Parent node pointer |
| `+0x0C` | Leaf overlay ID; branch child count |
| `+0x10` | Leaf base request table; branch child-pointer array |
| `+0x14` | Leaf base load output |
| `+0x18/+0x1C` | Leaf difficulty requests/output |
| `+0x20/+0x24` | Leaf language requests/output |
| `+0x28/+0x2C` | Leaf combined requests/output |

The executable's two branch policies are now reflected by
[`BranchKind`](../../crates/inf-formats/src/preload.rs):

| Serialized branch | Vtable | Initial activation | Requested descendant |
|---|---|---|---|
| Implicit root and tag 2: `Group` | `00529290` | `004EEE70` activates every child, in order | `004EEEB0` activates parent path and inactive siblings, excluding the requested child |
| Tag 1: `Select` | `00529274` | `004EED50` activates only the first child | `004EED80` unloads active siblings when already active; otherwise activates parent path |

Both use `004EEBF0` to unload active children and clear their active bytes.
The nodes' other methods: the node base's (`00529228`) scalar deleting
destructor is `004EE230`; a leaf's (`00529240`, `004EE360` over `004EE380`)
frees its four request tables and outputs, each table through `004EF0A0`'s
walk (every handler's list, then the array; leaf construction `004EE260`
also hands it to its unwinding); a branch's (`00529258`, `004EEB60` over
`004EEB80`) deletes its children, as the Select's and Group's do
(`004EECC0`/`004EECE0`, `004EEE00`). A branch's `+10` and `+14`
(`004EEC60`, `004EEC90`) forward to every active child. `004EEFC0` reads
one request table from `PreLoad.dat` (per handler a WORD, a WORD count and
that many DWORD requests; from `004EF73B` and `004EFA8F`), and `004EE100`
tears the tree down: it unloads an active root, then deletes it.
The original parser's `Group`/`Select` names were reversed; that error
did not affect its flat leaf inventory but would reverse runtime lifetime
ownership. The retail-tree regression in
[corpus.rs](../../crates/inf-formats/tests/corpus.rs) pins the corrected roles.

The top of the actual retail tree is:

```text
Group
  leaf 0
  Select
    leaf 2
    leaf 47
    Group
      leaf 1
      Select
        Group: Select[11,19,22,28,35,25,43] + leaf 6
        Group: Select[12,20,13,33,38]       + leaf 9
        Group: Select[14,23,42,31]          + leaf 8
        Group: Select[17,27,34]             + leaf 7
        Group: Select[15,16,21,36,37,39,40,32] + leaf 5
        Group: Select[18,24,30]             + leaf 3
        Group: Select[26,41]                + leaf 4
        Group: Select[29]                   + leaf 10
        Group: leaf 48
```

This proves cross-leaf retention from authored tree ownership, instead of
corpus-wide accumulation. Initial root activation selects **0 and 2**.
Requesting frontend leaf 47 unloads 2 and keeps 0. A world request retains
0 and shared bank 1 plus the selected family's bank and selected level.
The first family's example is `{0,1,6,11}`. These sets do not by themselves
specify registration order: record the actual activation/unload operations.

### Four overlay phases and registration

`0x004EE680` loads base, difficulty, language, combined in that order.
`0x004EE4E0` skips a whole file if all requested counts are zero; within a
file it calls active handlers in index order. The resource key is
`overlay_id * 4 + phase`. Keep [`SceneContext`](../../crates/inf-formats/src/scene.rs)
as the explicit difficulty/language/leaf selection, while a separate runtime
owner handles the tree and retains coexisting leaves.

All four request tables are serialized. Each entry is an `i16` count plus
a segment list. A segment's low half is a local-index threshold; its signed
high half is added to the local index to obtain the global slot. The older
format notes' descriptions of the fourth table as "derived" and segments as
allocation hints must not guide runtime code. The canonical parser and
`0x004EFC00` establish the actual mapping.

`0x004EFC00` overwrites the destination global slot. `0x004EE6D0` unloads
each active handler allocation and writes zero to its mapped slots; it does
not restore an older shadowed registration from a stack. Difficulty/language
changes go through `0x004EE160`: difficulty virtual `+0x0C`, language
`+0x10`, then combined `+0x14` when either changed. Branch callbacks visit
active children only. A scene snapshot must retain source leaf, overlay
phase, handler, local index, global slot, and registration sequence.

The language and combined leaf reloaders are `004EE9E0` and `004EEAA0`;
their full bodies were checked in retail disassembly because the focused C
extracts retain only tail fragments. Both match difficulty `004EE920`:
clean up the phase allocation, clear every mapped local slot, then load that
leaf's newly selected phase. Reload traversal is **phase-major**: complete
difficulty for every active leaf, then language for every active leaf, then
combined once if either selector changed. Each leaf's unload/load pair is
complete before the next leaf is visited. The base phase is untouched.

Handler-specific cleanup precedes those generic slot clears. Handlers 3 and 7
unlink backend conversion callbacks; 3 also releases variant backend objects
while the global atlas buffer has separate custody. Handler 8 frees its model
allocation chain. Handler 9 visits every local entry to release type-1 backend
objects; aliases in the global table do not deduplicate this cleanup. Other
handlers, including fonts in 4 and constructor/world resources in 13/14, use ordinary allocation release at
`0050ED60`. The generic mapping-clearing policy is shared by all of them.

### Implemented resource owner

[`ResidentRuntime`](../../crates/inf-game/src/residency.rs) executes this
storage-lifetime contract through `Boot`, `Activate`, and `Select` requests.
[`SceneTree`](../../crates/inf-formats/src/scene_tree.rs) preserves ordered
ancestor activation and forward sibling release, including zero-request
leaves. Immutable allocations are shared while a candidate transaction is
prepared, so missing/corrupt data cannot partially replace active state.
Retained banks preserve allocation identity; successful transitions preserve
retail registration and unconditional clearing order. Public resource lookups
borrow provenance and the owning bytes together across a single state.

Handler-4 resources are canonical [fonts](TEXT_RUNTIME.md). The borrowed
presentation adapter resolves strings, sprites, fonts, style/material words,
and model programs by their registration's local index; see
[resident presentation resources](FRONTEND_RUNTIME.md#resident-presentation-resources).
Typed lookup does not execute native pointer relocation or allocate backend objects.

The native operation sequence across the first-world entry is:

- Boot: load 0, then 2.
- Request 47: unload 2, load 47.
- Request 48: unload 47, load shared bank 1, activate empty leaf 48.
- Request 11: release 48, load family bank 6, then world bank 11.

The final authored-order inventory is `[0,1,11,6]`; it is deliberately not a
claim that 11 registered before 6. Receipts retain the actual event sequence.
Unloading never restores a shadowed earlier registration, even if its bank
remains resident. Selector changes preserve this same asymmetry.

This owner does not yet create backend allocations, invoke their cleanup
callbacks, relocate original pointers, or construct entities. In particular,
Handler-9 type-5 alias resolution happens after all local bank registrations
against the then-current sound table (`FUN_0050F760` in
resource_handlers.c); stored alias data alone does not
establish playable audio ownership.

## First world boundary

`0x004874D0` invokes `0x0048DC30`. The latter copies its selected world
field `+0x30` into current `+0xA0`, explicitly requests leaf **48**, then
requests **current + 10**. Leaf 48 has no active resource requests; its
use still changes tree ownership. Bypassing it because its OVL is empty
would omit the unloading boundary.

`0x0048DC30` then reads the resource at `DAT_0055EDDC[0]`, derives terrain
scale globals `DAT_005372F8` and `DAT_005483BC` from flags, and iterates
record pointers at resource `+0x3C` using count `+0x38`. It samples terrain
through `00450A30` and constructs objects through `00460310`, followed by
list traversal and virtual initialization callbacks. The concrete
[terrain runtime prerequisite](TERRAIN_RUNTIME.md) now owns `004506A0`
diagonal preparation and `00450A30` height queries, keeping H12 immutable
and requiring explicit current spacing. The loader only forces coarse
spacing for H14 flag `0x2000`; its unconditional `0044B590` builds the blend
table and prepares the grid before placements are sampled. Terrain-adjusted
constructor requests preserve WORD-wrapped Y, authored index, rotation order,
and instance-command custody. They do not construct live objects. The canonical
[world asset decoder](WORLD_ASSETS.md) now exposes the constructor prefixes,
ordered placement graph, raw geometry inputs and relocation provenance.
The [common live-object owner](OBJECT_RUNTIME.md) now supplies stable object
IDs, ordered embedded-node lists, context/payload initialization, flag setters,
disposal and destruction. Its construction consumer admits exact selectors
1, 2, 6, 7, 10, 11, 12, 16, 19, 30, 33, 34, 35, 36, 51, 60, 77, 86, 87, 88,
90 and 93 in mode 0, including ordered command rejection/deletion and completion
state. The visual prefix binds H8 metadata and handles model replacement;
selector 19 initializes terrain-light colours, gate and radius, and selector
51 owns [route nodes and post-placement grouping](ROUTE_RUNTIME.md). Together
with selectors 10, 11, 12, 30, 34, 35, 36, 60, 77, 86 and 88 they cover construction of all 118 of the first world's placements.
The rotating visual prefix
and [shared actor owner](ACTOR_RUNTIME.md) now supply actor construction,
terrain classification, packed handle registration and destruction with
explicit allocation-position, owner and terrain inputs. Selector 88 adds its exact
extension initialization and retained H14 operand address, without inventing a
portable allocator's pre-context bytes.
The central derived actor adds the signed world-parameter RNG operation and
owned admission state. Selector 77 adds a second process RNG draw and its
postinitializer; selector 11 adds its commands and network dirty-bit callback.
[Selector 60](SELECTOR60_RUNTIME.md) retains shared spawn rows and owns their
after-load allocation, lookup and destruction. Selector 86 preserves retained-data writes in shared runtime
allocation images while leaving original OVL bytes immutable.
[Selector 87](OBJECT_RUNTIME.md#selector-87-map-annotations) owns numeric
labels, radius/RGB commands and a separate annotation list, with native
prepend, tail movement and destruction order.
[Selector 30](SELECTOR30_RUNTIME.md) supplies the real world-start query and
override writes with explicit H1/H12 state, plus completion-bit after-load.
[Selectors 34/35 and their family](FAMILY428650_RUNTIME.md) supply home-position
admission, terrain commands, retained ratio inputs and placement-reference
resolution, with selector 34's distinct ratio/flag and signed terrain-floor
postinitializer. [Selectors 10/36 and their family](FAMILY4236D0_RUNTIME.md) add
owned probe and postinit helpers, retained payload inputs and mode selection;
selector 10 additionally owns the descriptor-selected helper and its children.
Their controlled construction tests do not admit the remaining
six first-world selector-12 actors or the later complete loader pass.
Shared actor context/payload initialization, retained damage responses and
ordered instance commands now preserve their native effects. Creation-listener
bindings and moving-cursor traversal have a concrete owner with mandatory callback
dispatch; all 30 recorded first-world actor factories encounter an empty list.
Selector 88's later shared-data rewrites, other derived actor payloads/commands,
the concrete listener callback bodies and callback-driven manager passes
remain open; these actor owners do not
execute the complete mission placement loop.

The complete transition still needs loading-progress callbacks during resource
activation, mission setup, enclosing furniture/light-manager initialization,
derived instance commands, list initialization and player attachment. The
[map allocation and process markers](WORLD_MAP_RUNTIME.md) now compose
constructor order, initial grid/RNG, mission publication and explicit local
retirement. The intrinsic grid is verified against the recorded complete map
and RNG write count; later map refresh and global drain remain separate.
`004874D0` finishes those world effects and its music work before
`004383B0` invokes phase-3 factory `0040A2E0/0040A420`. That screen is the
briefing interface, with tick `0040C6A0`, draw `0040B0F0` and presentation
`0040D1D0`; it consumes the live player and H14 briefing records (`+28` count,
`+2C` relocated pointer). It is not the earlier loading-progress callback or
phase-1 `Title.avi`. The [title lifecycle](FRONTEND_RUNTIME.md#title-handoff)
plays transcoded frames when `video/` is present (one-command transcode via
`scripts/ensure-title-video.ps1`), else takes the failed-open path. The
production session admits 9→3 with fresh-heap WORDs (`world_backend`), owns
the authored briefing records, menus and Start timing and the gameplay skeleton
(arcade movement, shared native camera and world draw), retaining the world
across briefing→gameplay. The
zero-record intro briefing exits on its native first-tick gate; an authored
briefing stays resident through record exhaustion until Start or a skip gate.
Remaining visual
and actor constructors stay per-feature fail-closed, not per-phase.

The [borrowed world load lifetime](WORLD_LOAD_LIFETIME.md) retains the actual
arena, mission, listeners, player/input carriers and timing across Prepare,
loading and the admitted deleting LeaveWorld path. `WorldLoadOwner` is distinct
from successful scene metadata; a stopped load retains its actual prefix and
cannot replay or fall back to a stale world. SavedImport fixtures deliberately
retain their serialized-player policy. Start ordinal comes from mission `+A4`;
production reload keeps the actual player's grant owner. Initial current world
1 and BYTE `+13=1` are [native mission constructor](WORLD_MISSION.md#native-mission-constructor)
stores, not captured or fresh-heap defaults.

`IntroCampaignReloadApproximation` retains actor clock 245, zero camera/settings
inputs and zero-filled actor allocation WORDs where current writers/allocator
history remain unowned. Actual player/arena/listener lifetime and shared RNG
are retained independently. This does not establish general campaign loads or
retail acceptance. The arcade
drive adapter resolves player `+D8` before `+D4` through `0046AE60` and moves only the effective
vehicle. Selector 88's `00443930` attachment therefore retains intro control.

The shared [manager cleanup](WORLD_CLEANUP.md) now owns `00478CD0` on
supplied actual arena/mission/current grant owners: actor FIFO, both physical
flag-list pairs, disposal, then parking, with native `+30` before deleting
destructors and normal-return-only inhibit clear. It retains the actual player
FIFO, registry allocation and earlier error prefixes. The vehicle's distinct
event-10 fanout, selector-35 grant callback and camera's ordered child deletion
are lower owners of that pass. Constructor-known empty message `00413A20`
cleanup also has its distinct owner. The actual results caller now consumes
the [post-effects retirement suffix](WORLD_CLEANUP.md#retained-player-results-retirement)
after local map retirement, mission BYTE12 and represented effect clearing.
Captured first player, both genuine controlled resolves, canonical `+64`,
ordered vehicle copy and same message precede that manager drain. Its port-only
`WorldSession::owner_retirement` retains `Completed` or a logged typed `Stopped`
prefix, permits phase continuation and prevents retry. Player FIFO/input/message
owners survive; stale camera/respawn diagnostics remain unchanged. The separate
[world load lifetime](WORLD_LOAD_LIFETIME.md) consumes the post-map manager pass
and the admitted deleting LeaveWorld sequence using those actual owners.
Mission/game destruction and global shutdown remain distinct required policies.

The [save serializers](SAVE_FORMAT.md) now establish this field's custody:
`0048EC10` writes mission owner `+30`, and `0048ECC0` restores it. It is
the DWORD immediately after the variable-length name's NUL, followed by
elapsed seconds. `0048DC30` copies that selection into current `+A0` before
requesting `current + 10`. A restored mission selection therefore determines
that request; it is still distinct from the phase ID and OVL leaf ID.
Runtime world positions, heading scales, player physics, RNG cadence,
contact ordering, and entity handles retain their own Infestation evidence.

The offline `00487410` path selects world `0x16` (22) when `DAT_0055F14C`
is clear; this selection is not necessarily world 1. The bounded phase query
in the capture ledger observes loader selection 22 and later selection 18
while current world is still 22. Operator descriptions such as first-mission
play must not be used as numeric world IDs. These observations establish
selection/entry state, not successful construction or a complete mission.

The [`session_effects`](../../crates/inf-game/src/session_effects.rs)
policy now owns the recovered transition-effect bodies for the 2→9→2 flow,
checked against the executable:

- `00487410` offline: sets gameplay owner byte `+5C2 = 1`, allocates the
  live `0x1A0`-byte player through `00469D60`, clears `+5D8`, then pushes
  mission selection 22 and `true` into `0048EBA0(+1B0, 22, true)`. The
  [shared mission reset](SAVE_FORMAT.md#shared-mission-reset) owns the complete
  save-field selection/progress policy, with elapsed Preserve/Clear. The
  [initial preparation kernel](PLAYER_CONSTRUCTION.md#initial-preparation)
  composes the live constructor, armed clear, shared attract/offline selection
  and complete reset. The admitted offline session consumes it on the
  [retained world owner](WORLD_LOAD_LIFETIME.md), including actual input/carrier
  transfer through loading and deleting LeaveWorld. Saved export remains a
  separate first-player projection, not a reimported replacement. The same
  attract selector serves the
  [supplied-state cycle](ATTRACT_DEMO_RUNTIME.md#supplied-state-attract-cycle);
  network `00488A20`/`0054E814` retains its separate selection boundary.
- `004871C0`: after actual map/BYTE12/effects and retained-player/manager
  retirement, the admitted constructor-absent `+5D0` audio branch precedes owner
  clear, sound/particle cleanup and fresh-head deleting player destructors.
  Actual input references release through the same runtime. The full network
  DWORD is read late; present audio, network teardown and stopped/unpublished
  carrier custody remain explicit boundaries rather than empty-world success.
- `00487360` frontend music selection and `004F0340` baseline reset forward
  to the existing audio/input owners.

A controller-level test pins the real sequence: entering 9 from 2 runs
PreparePhase9, returning to 2 runs LeaveWorld then SelectFrontendMusic, and
every transition closes with ResetInputBaselines. The production
[`PortSession`](../../crates/inf-game/src/session_platform.rs) host now
composes the real frontend and text-entry screens behind that policy: kind
and reuse queries and ordered construction/destruction. Direct frontend entry
runs the phase-2 transition effects and real factory. Title playback, fresh-heap
world loading, briefing and the gameplay screen are connected; native world
simulation and attract/network preparation remain bounded owners.
Its [`with_name_entry_draw`](../../crates/inf-game/src/session_platform/draw.rs)
scopes mutable access to the actual text-entry screen alongside resident resources,
authored assets, registry and process animation under the linked-context busy guard.
It verifies the linked screen identity and offers no text-entry draw after return
to phase 2. The [outer draw](PROFILE_RUNTIME.md#name-entry-outer-frame-00459310)
finishes by unlocking the surface; the scheduler's separate presentation callback
owns display and clearing through the concrete linked host below.

### Production linked session

[`session_platform::linked`](../../crates/inf-game/src/session_platform/linked.rs)
routes the selected-input poll and linked `+04` tick to the registry's current
linked screen. Services must match that screen before any effect runs. The
frontend receives its live profile/settings/audio services; phase 9 receives
the same process keyboard adapter, RNG, profile store and prepared offline
mission/player state. Physical key notifications are delivered in arrival order
immediately before the selected-input poll. Their exact arrival placement within
a retail scheduler iteration has not been established by the recording.

[`SoftwareLinkedFrame`](../../crates/inf-game/src/session_platform/software.rs)
resolves the linked screen again for `+08` and calls its real frontend or
name-entry outer draw under the context busy guard. Linked `+0C` resolves the
current H7/32 clear material (or the separately owned forced-black policy),
presents the completed surface, then clears it. The SDL app and headless tests
use this same draw/presentation glue; surface unlock is never presentation.

The reachable production flow is direct phase 2, Start Game's derived profile
menu, an empty slot's delayed phase-9 request, and Back to a new phase-2 screen.
Completion performs `0045AD30` identity persistence and offline serialization
in memory, then requests phase 3. The fresh-heap loader retains selection 22's
OVL 32 through its empty briefing pass and the engine intro. Selection 18's
OVL 28 has the later visible Level 1 briefing. The
[briefing owner](BRIEFING_RUNTIME.md) executes authored records, menus, text,
status gates and the native Start delay. Native background sprites, panel
frames, model preview and progress/Start layers now have production owners.
Decoration submission and the primary Far effect pixel consumer `004D9840`
are owned, with original globe effect commands and changed pixels verified
at 640x480 and 1024x768. Screen-camera map tiles and ordinary G+48 actor
markers and the [independent annotation tail](BRIEFING_RUNTIME.md#independent-map-annotations)
execute after each wrapped tile in native order with retained per-world map
state. Numeric labels and radius/RGB sprites use original font/style/sprite
lookups and selector-87 state across loaded missions. Native camera static, pulse and blinking disabled-camera text now
execute with the process RNG and animation clock. The terrain panel and world
draw share [terrain submission](TERRAIN_RUNTIME.md#shared-production-pixels)
and recovered colored/textured quad consumers, including native E150 ground
tile clipping and the [shared furniture draw](FURNITURE_RUNTIME.md), including
[shared model clipping](PRIMITIVE_RUNTIME.md#de-and-d6-source-polygons) for
DE/E6 shared shade and D6 per-corner shade, with their distinct Near/Far UV
consumers and retained primary draws before unavailable secondary passes.
The [shared water owner](WATER_RENDERING.md) retains the pane's actual world
flags and animation clock. Briefing clears the same process light cells while
retaining anchors, then shares the live settings-gated ground policy. The
shared [terrain emission](TERRAIN_RUNTIME.md#near-strip-terrain-particle-emission)
producer retains native trail births and RNG order; its event writer,
nonzero-jitter first-X stack custody and lightning remain feature boundaries.
Annotation state/list and draw receipts
cover bounded original-bytecode inputs; authored pixel tests do not establish
a matched retail briefing comparison.
The [world visibility owner](WORLD_VISIBILITY.md) replaces frontend fade with
constructor/header-derived manager distances, shared timing feedback and the
native predraw slew. Manager F and the timing ring survive offline reload;
the new header reseeds H and the two slew fields. Current world input reads the
view zoom `+5C0`, constructor zero, which the free look `00421DC0` writes.
Normal world draws resolve H7 slot 74 and convert its
source RGB through the actual backend layout; the alternate +5C3/H7 slot 32
is [infrared vision](GAMEPLAY_RUNTIME.md#infrared-vision).
The [intro controller](INTRO_SEQUENCE.md) retains its native script and state
sequence, with arena-backed effects, canonical selector-90 helpers and the
[selector-16 camera pose](CAMERA_RUNTIME.md) connected to the shared world
Tick and live-arena Draw. Draw also freshly acquires the camera through the
shared current factory inputs, then reads its fields after fade/material; NULL
retained-context custody remains an explicit feature boundary. Selector-2 Chase
follows the actual vehicle on ordinary
missions without an intro controller. The special-view reader owns known-input
math and reached partial writes; normal activation still requires its separate
stored-vector writer.
Lazy camera acquisition reads the current process owner only after a handle
resolution miss. Initial deadline +140 uses the named
`FreshIntroDeadlineApproximation`. Selector-11 StateOne now composes live
terrain/furniture and actor-radius effects, actual process particle births,
terrain scatter, statistics and placement death. Thirteen original-bytecode
cases verify supplied-service +AC order and failure prefixes; referenced task
+22C/lifecycle gates, moved-actor reactions, source/network delivery and remaining
furniture callbacks still gate faithful cinematic behavior and retail
comparison. Shared selector12 and `004237D0` family descriptor exits now
preserve retirement through automatic intro-to-results-to-Level-1-briefing
continuation. Existing deadline, results and reload approximations remain;
successful handoff does not establish complete cinematic simulation.
The [process particle owner](PARTICLE_RUNTIME.md) retains its pool,
pending/active lists and timing rules across world callbacks. Retirement births
feed the post-camera transfer/update, shared world draw/light and positional
sound service. The furniture queue runs before actors and clears at the owned
`00450210` mission-cleanup subphase; later results cleanup retains its named
approximation. [Output scaling](PRESENTATION_POLICY.md) enlarges
the authored framebuffer consistently across screens when the resolution is
not an original size, and retains native defaults. No
profile disk save is reported. Idle demo playback is skipped through the named
[AttractDemoDeferredApproximation](ATTRACT_DEMO_RUNTIME.md#attractdemodeferredapproximation)
before screen teardown, retaining the live menu across subsequent idle intervals.
The [complete constructor93 update](ATTRACT_DEMO_RUNTIME.md#constructor-93-movement)
uses shared actor, current-terrain, angular-response and physics owners through
the actual callback class. Its [direct factory](ATTRACT_DEMO_RUNTIME.md#direct-demo-factory)
preserves the distinct native construction order. The
[complete attract setup](ATTRACT_DEMO_RUNTIME.md#attract-actor-placement) captures
raw H1/0 before construction, publishes the actor/handle, then freshly acquires
H12 for signed-WORD height selection and the shared XYZ copy. Required target
allocation precedes fresh actor-handle reads and the captured camera relation;
actual NULL allocation still reaches replacement and the literal byte/mode stores.
The [supplied-state cycle](ATTRACT_DEMO_RUNTIME.md#supplied-state-attract-cycle)
composes first-use, current handle resolution, the capped-DWORD timer, authored
selection, real completion and first-pending phase requests. Constructor-unwritten
timer/handle backing stays explicit rather than default zero. The represented
[player construction and teardown](PLAYER_CONSTRUCTION.md) owns ordered input
references, bound message/grants, FIFO publication and genuine empty-message
destruction in its supplied successful-allocation domain. Initial preparation
shares the constructor, selector and full mission reset; the admitted offline
session now retains that actual player/input lifetime through loading and return.
Initial attract state/camera admission, general phase8 and playback remain
separate required owners. Network preparation,
phases 10/11 and other unimplemented transitions retain their explicit errors.
Alt+Escape ends the loop after its current iteration;
Alt+F1 follows the native return-only callback. Alt+Enter's display-reset
callback remains unconnected, independently of the working Video menu.

The [linked tests](../../crates/inf-game/src/session_platform/linked_tests.rs)
exercise the round trip, name completion/world handoff, exit callback and service
mismatch with retail resources and the production CPU renderer. They preserve
one keyboard adapter and renderer across 2→9→2 and verify presentation/clear
after each successful iteration. Renderer retention and resource rebinding in
the SDL host are described with
[frontend presentation](FRONTEND_RUNTIME.md#concrete-frontend-presentation).
Matched retail-frame acceptance of the complete intro and visible briefing
remains open. World-load music selects native argument 1, resuming immediately
unless selection is 22 (`004874D0`). Gameplay entry reads signed H14 header
WORD +14, adds one, selects that native argument and resumes (`0048E780`,
`00487250`). These use the existing CD owner, whose selection argument is
distinct from the resulting disc track number.

## Implementable milestones and acceptance

1. **Native data boot:** parse the installed preload and original OVLs;
   expose explicit selection and exact resource provenance; reject missing
   required files and malformed tables. Do not require generated exports.
2. **Scene ownership:** reproduce root activation `{0,2}`, frontend switch
   `{0,47}`, and leaf-48/world selection without a renderer. Assert operation
   order, sibling unloads, retained bank 0, zero-request leaf transitions,
   phase precedence, and slot clearing. Unsupported resource semantics stay
   visible rather than being converted to guessed actors.
3. **First visible frontend:** use the phase-2 factory/data and the `{0,47}`
   receipt to render the authored menu, implement input and title handoff,
   and compare bounded TTD positions/screenshots. The static asset viewer's
   neutral composites are not a captured retail frontend frame.
4. **First world:** join the recovered preparation/briefing path to native
   terrain and actor construction, player update, rendering, and return to
   frontend. Acceptance needs a matched first-world sequence from the
   existing trace, not just a successful resource load.

Only the first two are small foundational code milestones. This document's
address map is not a claim that the full menu, save system, world tick,
render callbacks, or campaign transition is implemented.

## Reproducible static checks and next evidence

No debugger or live process was used for these findings. Missing short
functions were checked with MinGW `objdump -d -Mintel` against
`infestation/Infest.exe`, using bounded `--start-address`/`--stop-address`:

| Address range / data | Fact checked |
|---|---|
| `004394E0..00439500`, vtable `005221B4` | Phase tick callback dispatch and transition-before-tick binding |
| `004EEBF0..004EEC30` | Active-child unload and active-byte clearing |
| `004EED50..004EEDE0` | Exclusive initial selection and sibling unloading |
| `004EEE70..004EEF10` | Additive all-child activation and requested-child exclusion |
| Vtables `00529240`, `00529274`, `00529290` | Leaf, exclusive, and additive virtual method mappings |
| `004389C0..00438A25` | Session startup requests leaf 0 (`ECX = 0`) |
| Call `00432684`, preceding instructions | Phase-0 screen requests leaf 2 |
| Call `004839A8`, preceding instructions | Frontend requests leaf 47 |
| Calls `0048DCA9`, `0048DCB7` | World loader requests 48, then current `+ 10` |

The accepted phase census now correlates requests/transitions and world-loader
entry; its coverage limits and exact bundles are in the capture ledger.
Continue the [authored frontend consumers](FRONTEND_RUNTIME.md) before asking
replay for per-draw state. Resource-retention observations and matched rendered
frames remain separate evidence. Keep addresses from the supported executable
profile, distinguish installed callbacks from executed callbacks, and keep all
large transcripts local.
