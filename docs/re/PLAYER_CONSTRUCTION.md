# Infestation player construction

This document owns the represented successful-allocation flow of `00469D60`,
its initial `00487410` caller, retained input sources and player teardown.
The supported original executable is `Infest.exe`, SHA-256
`bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956`.
The original body `[00469D60,00469FBD)` is 605 bytes with SHA-256
`b5fb844654bc0454cda342c72ca13edd1d7be568632fc2aba9a2e781ffc8f046`.
Addresses refer to that executable and the corresponding
player consumers; they do not establish live construction
or production attract playback.

## Allocation identity and actual owners

[`WorldPlayerConstruction`](../../crates/inf-game/src/object_runtime/player/constructor.rs)
retains the actual constructor input references and unpublished message/mask.
`ObjectArena::reserve_world_player_construction(callback_context)` reserves
an arena-qualified semantic player identity for a supplied successful allocation;
it neither executes the native allocator nor publishes a player-list node.
The callback context is a separately supplied stable logical payload identity,
unique for distinct simultaneously live players in the same `InputRuntime`.
It is not an `InputContextId`, arena-local `PlayerId` or invented native pointer.

`construct_world_player` consumes the carrier, actual `InputRuntime`, lazy
`PlayerInputDescriptorSource` and current `LiveGrantContext`. The carrier has
explicit `Allocated`, `Constructing`, `Stopped` and `Published` phases. Foreign
arena and repeated-attempt validation happen before constructor effects; lower
process, executable, residency and descriptor sources are admitted only when
reached. A stopped constructor cannot be replayed through the same carrier.

The carrier has no cloning/default policy. Its input references are not
duplicated in the published `WorldPlayer` projection, so it must remain alive
with that player. Acquired consumer IDs belong to the acquiring `InputRuntime`;
explicit release must use that same actual runtime. Publication moves the same
message and mask into the real FIFO list. Pending players remain absent from
`world_player`, `first_player`
and public message-token lookup; no temporary publication or lookup fallback
makes initial grants work.

## Native phase order

| Phase | Original source | Represented effects |
|---|---|---|
| Pre-input stores | `00469D84..00469D9D` | Raw signed DWORD `+48=-1000`, DWORDs `+4C/+50=0`, BYTE `+58=1` |
| Analog references | `00469DA1`, `00469DB3`, `00469DC6` | Stores at `+5C/+60/+64` |
| Digital references | `00469DD9..00469E64` | Eight stores at `+68..+84` |
| Bound message/grants | `00469E75 -> 00413140` | Actual `+88` message, `+9C` grant mask and twelve ordered setters |
| Current NULL | `00469E80`, `00469E8B` | Separate native reads initialize `+D8/+DC` after grants |
| Represented runtime/save stores | `00469EA4..00469F61` | Raw save fields, six `+100` words, eight `+13C` bytes, sixteen `+150` words and `+190` items |
| FIFO append | `00469F67..00469F7C` | Same player identity/components appended after existing players |
| Published suffix | `00469F7E..00469FA4` | Fresh native NULL to `+D4`, `+04..+47` clear, then command words `+130/+134/+138=0` |

The suffix includes the complete `004959E0` fill helper, which has no calls.
The admitted current process loan is immutable through the lower phases.
Once its NULL is available at `00469E80`, neither the `+DC` read nor the
post-FIFO `+D4` read can become unavailable in this domain. There is no earlier
NULL preflight: completed input and grant effects survive a missing NULL at
the first reached read.

Mandatory published fields such as `+D4` and the command mask are materialized
as part of a complete-known `WorldPlayer` projection. The actual append still
precedes the named suffix writes. This does not claim observation of partially
written native list nodes, asynchronous source mutation, physical aliases or
per-scalar memory faults. Represented `+28/+2C/+34/+50` become known zero in
this constructor; `saved_e0` and BYTE `+EC` remain unavailable. The projection
does not manufacture the rest of the `0x1A0` heap allocation.

## Lazy input-reference sequence

The [private input owner](../../crates/inf-game/src/object_runtime/player/input.rs)
requests each descriptor at its reached slot, validates its family/identity,
acquires through the actual input runtime and immediately retains the result.
Every original wrapper receives the same player pointer as callback context.

| Kind | Descriptor identity | Player store |
|---|---|---|
| Analog | `0054ECF8` | `+5C` |
| Analog | `0054ECE8` | `+60` |
| Analog | `0054ED08` | `+64` |
| Digital | `0053A368`, `0053A370`, `0053A358`, `0053A360` | `+68/+6C/+70/+74` |
| Digital | `0053A378`, `0053A380`, `0053A388`, `0053A390` | `+78/+7C/+80/+84` |

The digital identities name file-backed records. The analog identities lie
in the supported PE's virtual zero-fill tail, so the file-only descriptor
parser rejects them. The retained
[`PlayerAnalogDescriptors`](../../crates/inf-game/src/player_input_source.rs)
owner seeds these three distinct records once after structural executable and
exact `.data` raw/virtual layout validation. It stores known callback NULL,
flags zero and period zero, without inventing a file span. Explicit supplied
current records preserve unavailable slots, identities and raw metadata.
`PlayerInputSource` borrows that actual owner and parses each reached digital
record through the existing file-only parser. A source loan resets neither
metadata nor input consumers and does not scan later records.

The supported `.data` starts at `0052E000`, with raw file offset `0012E000`,
raw bytes `00019000`, virtual bytes `00132C38` and characteristics `C0000040`.
All three complete twelve-byte analog records fall beyond raw end `00547000`
and inside virtual storage. Loader zero-fill is startup provenance, not proof
of arbitrary CRT/device initialization or later native writer absence. A
native metadata writer would need shared current custody at acquisition,
publication, timer emission and release; merely changing a source record
would leave `InputRuntime`'s copied consumer metadata stale. The current owner
therefore admits stable untimed metadata and provides no mutable writer.

The native pools share registrations by descriptor pointer and callback
context. Reuse does not reread descriptor contents. The typed owner uses
`InputRuntime`'s explicit known, stable, period-zero metadata domain, including
its conflict and checked-reference errors. Nonzero analog periods, changing
native metadata, allocation NULL and wrapping native counts are outside this
domain. The existing portable digital timing initialization remains the
[input owner's policy](INPUT_RUNTIME.md#held-duration-and-destruction).

Acquisition creates no binding context, sets or device, selects no input,
polls no callback and samples no external clock. Required descriptor/family/identity
or input errors retain every earlier acquired reference and stop before
message construction.

## Bound initial message and grant phases

`00469E75` calls `00413140` on player `+88`. That body sets suppression BYTE
`+1=1` at `00413153`, then passes the actual message subobject to the grant
constructor at `00413163`. `00475027` stores this recipient at grant `+1C`
(player `+B8`) before the six-word clear. The port retains the actual
[arena-qualified message identity](PLAYER_MESSAGES.md), without a duplicate
raw recipient pointer or arbitrary retargeting policy.

The canonical
[`PLAYER_INITIAL_GRANTS`](../../crates/inf-formats/src/grant_table.rs)
sequence is `[1,48h,4,83h,5,6,76h,Dh,Ah,7Ah,7Ch,4Eh]`, in the twelve
`00413175..004131DB` setter-call order. It applies to the native player
constructor regardless of network/attract state. The existing offline save
projection consumes the same IDs through its separate pure closure policy.

Published and constructing players share the complete
[`004750C0` setter](../../crates/inf-game/src/object_runtime/grants/set.rs).
The constructing target borrows the carrier's actual mask/message and reserved
identity; public published-player authentication remains in the published arm.
Each new grant commits its bit before reading the full current network DWORD.
Offline text, canonical H0 capture and CRT formatting remain reached before
the final suppression gate. Suppression therefore does not permit missing
text/classification data to bypass their reads. Dependency closure completes
each satisfied child immediately in ascending candidate order.

Missing process after the first committed bit, H0/CRT failure or recursive
dependency failure keeps the actual unpublished prefix. Missing process NULL
after complete grants keeps all inputs, message and grant state and prevents
FIFO publication. The [grant owner](GRANT_RUNTIME.md) retains the precise
record, signed H0, formatter and message error order; this constructor adds
no loader/message-success host or normalized OR-merge substitute.

## Initial preparation

[`prepare_player`](../../crates/inf-game/src/player_prepare.rs) composes the
same constructor and full mission reset for initial offline or attract
preparation. The caller lends its actual arena, retained constructor collection,
input runtime, mission, owner byte and `AttractCycleState`, along with current
descriptor/grant sources and the authored selection table. It publishes owner
`+5C2=1`, retains the new allocation before any fallible construction, completes
`00469D60`, then clears armed `+5D8`. Only then does attract select through the
same `00488C90` helper used by later cycles. The ordinary branch reads the full
network DWORD; zero selects 22, while nonzero preserves an explicit required
network-selection boundary. Complete selection invokes `0048EBA0` with elapsed
Clear on the caller's actual mission.

The original complete body `[00487410,004874C8)` is 184 bytes, SHA-256
`a9dfe80ef84bdf71198c2fa96d9a0ab6140ad38899f8d19b6c7033dc2b8bd6ce`.
Source order is owner store `0048742E`, constructor call `00487453`, armed store
`00487458`, attract read `0048745F`, ordinary network read `0048747C`, then reset
`004874B3`. Neither this caller nor the constructor establishes timer `+5D4`.
Preparation leaves timer, demo object and handle `+5DC` unchanged.

Errors retain the actual player/carrier, input references, committed grant
prefix and last reached phase. A completed network-domain constructor remains
published when its later selection is unavailable; mission reset has not run,
and armed has already cleared. No serialized-player substitute, temporary list
publication, replay guard or automatic cleanup changes those effects. Saved
export is a separate enclosing phase which must select the actual first player;
the preparation receipt identifies the newly constructed player independently.

This is the explicitly supplied successful-allocation, stable-current-source
domain. Native allocator NULL skips construction but continues selection/reset;
that outcome is not manufactured by this owner. Network selection, native SEH,
asynchronous source mutation remain distinct. The admitted offline session now
consumes this kernel through the [retained world lifetime](WORLD_LOAD_LIFETIME.md);
initial attract/network admission remains separate.

## Published destruction and component cleanup

[`destroy_constructed_world_player`](../../crates/inf-game/src/object_runtime/player/constructor/destruction.rs)
owns the represented `00469FE0` phases for the same completed construction.
It authenticates the arena/carrier and published FIFO membership before
effects, moves the actual player record out of the list into its carrier,
destroys that record's message, then releases the actual input references.
Old public message tokens become unknown as soon as FIFO unlink completes.
Other players and actor/list/handle owners stay independent.

`PlayerDestructionPhase` records Unlinked, MessageDestroyed and Complete.
The carrier retains the same retiring record even after completion; this is
readonly typed state, not a live native allocation pointer. A lower failure
keeps the reached prefix. Repeating the whole destructor is rejected, and a
prior attempt at public input-component cleanup also rejects full destruction
before unlink. This prevents replay of lifetime effects that native code only
performs once. Construction history remains Published; retirement has its
own explicit phase rather than silently publishing a replacement player.

The complete empty-message domain is genuine `004131F0`: constructor
`00413140` writes known NULL to child pointers `+04/+08/+0C` and backing `+38`.
Destruction reads child `+0C`, then `+04`, then `+08`, then backing `+38`.
All NULL branches skip deletion/free; the native body performs no NULL stores.
The owner records this custody independently from suppression. Enable changes
only suppression, and an enabled send currently stops before storage writes,
so neither can invalidate those known constructor fields. Required unknown
storage stops at its reached field; no empty widget or successful destructor
host replaces an unowned child. Enabled widget/backing destruction remains a
separate domain in [player messages](PLAYER_MESSAGES.md).

`release_input_references` implements only the proved reference component of
`00469FE0`: digital `+84..+68`, then analog `+64..+5C`. A stopped release clears
only completed slots and retains the failed/earlier references. Real final
release keeps the input owner's bulk queued-target invalidation and captured
callback snapshots.

Unpublished stopped construction uses only explicit component cleanup; typed
source errors do not invoke a full destructor or native SEH thunks. A stopped
published release can also finish remaining references through that component
API without replaying the enclosing destructor or rewriting its phase receipt.
Native physical free, allocation NULL, partial heap backing and SEH remain
outside these typed owners.

Native world reload `0048E890` retains the player FIFO while cleaning world
objects. It acquires the first player's controlled actor through `0046AE60`,
performs the reached [vehicle query and ordered copy](PLAYER_VEHICLE_STATE.md),
then calls message cleanup `00413A20` before the manager pass. It
must therefore retain these actual input references and message identity.
Returning to the frontend through `004871C0` is a different lifetime: after
world, sound and particle cleanup, `00487200..0048721E` repeatedly destroys
the current first player before its network tail. A completed return does
not retain players for the next game. The shared
[manager cleanup](WORLD_CLEANUP.md) performs
required virtual `+30` callbacks before class destructors; clearing a registry
or allocating a fresh arena cannot substitute for that pass. The
[retained world lifetime](WORLD_LOAD_LIFETIME.md) now composes actual arena/carrier
transfer, post-map manager cleanup and the admitted deleting return through
`world_backend` and `session_platform`, using the same input runtime until
complete release.

The [constructor tests](../../crates/inf-game/src/object_runtime/player/constructor/tests.rs)
cover reservation/foreign arena, stopped input before message construction,
retained grant bits, suppressed offline H0/CRT failures before NULL, late NULL
failure after network grants and actual offline FIFO publication. The
[input tests](../../crates/inf-game/src/object_runtime/player/input/tests.rs)
cover all eleven reached source/family/identity prefixes, timed/conflicting
metadata, shared versus distinct contexts, reverse release stops and real
queued-callback custody. The retained
[source tests](../../crates/inf-game/src/player_input_source/tests.rs) cover
the original loader profile, lazy current availability and actual consumer
sharing. The [preparation tests](../../crates/inf-game/src/player_prepare/tests.rs)
cover real Boot/H0/current CRT, source/NULL failure prefixes, first-player
export, full-DWORD network selection and wrapped attract selection. The
[destruction tests](../../crates/inf-game/src/object_runtime/player/constructor/destruction/tests.rs)
cover FIFO/message invalidation, actual shared references and queued callbacks,
stopped release and repeat/foreign/manual-cleanup admission.
These are supplied-owner tests, not native allocator,
SEH, pointer-layout or production constructor execution proofs.

Existing `add_world_player` import/save callers preserve their prepared-mask
policy and earlier field availability through the explicit SavedImport mode.
They are not silently replaced by this live constructor. The admitted offline
session prepares and loads its actual constructor carriers and deletes them
on the bounded LeaveWorld path; source coverage does not establish native
allocation/SEH execution. Initial attract playback/admission, enabled HUD/widgets,
present audio, network teardown and complete game shutdown remain separate in
[world lifetime](WORLD_LOAD_LIFETIME.md#remaining-bounds-and-checks) and
[attract ownership](ATTRACT_DEMO_RUNTIME.md#native-preparation-and-selection).
