# Infestation dynamic model observation

This document owns the bounded observation of genuine dynamic Handler-8 draws
and the evidence required for captured-model presentation. Two source-matched
`healthsmall` invocations are now retained from the existing recording.
The capture ledger owns accepted
acquisitions, local artifacts and exact replay commands. The canonical
[observation importer](../../crates/inf-formats/src/observed_model.rs) now
loads these bundles for the viewer's separate **Captured Draw** presentation.

## Current evidence boundary

The retained `Infest01` recording covers menu activity and some gameplay. Its
accepted queries establish startup requests, a bounded phase/world-owner
chronology, and the two model draws below. The world-loader observation is selected/current
world **22**, followed later by selected **18**. The informal acquisition label
“level-1 gameplay” must not replace these raw observations. The accepted
`41743C:13CB` transition toward phase 4 is a candidate starting position for a
bounded gameplay query, not proof of a particular visible object.

The 41 unavailable runtime-geometry variants divide into nine parent-slot
cases, two custom-vertex cases, three camera-plane cases, and 27 packed-value
or register cases. These are the mutually exclusive dependency groups in
[model asset coverage](MODEL_ASSET_COVERAGE.md), not 41 unknown binary formats.

The first observed subject is **global 168, `healthsmall`, the unique `1.ovl`
content variant**. Its first instruction is `F5 [1, 0080, 400A]`: callback
selector zero supplies the left value, the literal shift decodes to nine, and
the low WORD is stored in register 1. The following `05` uses angles
`[00C1, 0000, C000]`, meaning register 1, zero, and the authored final rotation.
Its body is empty. Its 120 root-program fan triangles consume the projections
created by that `05` after the nested transform has been restored.

Root world ranges `[0,2,2,2]` initialize slot 0. The nested projection range
is start 2, mirrored boundary 170, end 170: even slots 2 through 168. Its
world range is empty, start/boundary/end 170. Captured projections must not be
presented as initialized world XYZ for those slots.

## Subject identity and selection

Canonical parsing identifies global 168 as local entry 166 of `1.ovl`, at
absolute bytes `20EB8..22110`. Its content and aligned extent are both `1258`
bytes (4,696); there is no alignment padding. SHA-256 of that full entry is
`FC7131397DE35138999DE9C317FDF31CABBF1B77738714C17B764EBF66454A92`;
the passive inspector's aligned-byte FNV-1a64 is `1031A8F7315E1BB3`.
All following ranges are entry-relative, with exclusive ends:

| Bytes | Canonical content |
| --- | --- |
| `0000..0020` | Header: 170 vertex slots, 85 raw vertices, 170 shade slots |
| `0020..021E` | Raw vertex triples |
| `021E..0416` | 84 normal triples |
| `0416..124C` | Command program, 1,819 WORDs |
| `124C..1258` | `healthsmall` plus its NUL terminator |

The first two header DWORDs are `0000001E, 0C00071B`, unique among the 849
installed model content variants. F5 starts at `416`, with arguments at `418`;
05 starts at `41E`, with arguments at `420`. Its empty-body terminator is at
`438`; the following root faces start at `43A`. These provide staged candidate
guards; the retained full-entry hash supplies durable identity. The allocated
vertex bank is 170 × 24 = `FF0` bytes, followed by 170 DWORD shades (`2A8`
bytes). Dumping all allocated slots does not establish that every field was
written; the initialized ranges remain those described above.

No installed H13 constructor initially binds model 168, and no decoded model
contains an authored external-child reference to it. Native world selection
22 requests leaf **32** (`current + 10` in `0048DC30`), after leaf 48; it is
not OVL leaf 22. The canonical difficulty-0/language-0 residency is
`[0,1,32,5]`. Leaf 32 has **118** authored placements, no H13-slot-9 pickups
or selector-14 placements, and no initial model-168 binding. Its eight typed
command-2 model inputs all select model 552, on placements 29 through 36;
none selects healthsmall. This authored absence does not exclude later
dynamic spawning.

The native selection is `0046D500`: an unsigned type WORD of 4 or `83` stores
model WORD `00A8` at object `+42`; type 4 first adds 100 to its position WORD
at `+34`. H14 virtual `+4` handler `0046D780` forwards command-3's low WORD,
returning at `0046D7C1`. Dynamic creator `0046D870` instead constructs H13 slot
10 for type `83`, otherwise slot 9, and calls the same selector with return
site `0046D8BF`. At the model store `0046D661`, ECX is the object, its `+44`
WORD is already the type, `[ESP+4]` is the caller, and `[ESP+8]` contains the
original type argument. Hook `0046D667` observes the completed model write.
This low-frequency writer can locate a dynamic subject before a targeted draw
query; observing selection alone does not prove admission, callback execution,
or rendering. Other data-driven model setters are outside this writer's scope.

## Native observation boundaries

These addresses were checked in the supported retail executable. The accepted
eight-event observations use H2 through root-program completion; H1 lookup
ancestry and the earlier dispatch-selection site were not separately observed.

| Site | Proven state available there |
| --- | --- |
| `0049FEA0` | Lookup entry: ECX is renderer owner, EDX the draw request; signed root argument is request `+38`, H8 owner is renderer `+5C` |
| `0049FED3` | Successful lookup: EAX is resolved model entry; its byte `+7` is the header flags, including force-near bit `20` |
| `004A00E0` | Invocation entry: ECX is transient context, EDX model entry; vertex allocation/preprocessing and dispatch selection have not completed |
| `004A0126` | EDI is context and ESI entry; `ctx+60` has been written and EBP is the chosen near/far handler table |
| `004A03A0` | `F5` entry: ECX context, EDX arguments; for fingerprinted healthsmall this follows root allocation/projection/world/shade initialization |
| `004ED4D4` | Actual packed-callback call: EAX context, ECX callback object, ESI vtable, target DWORD at `[ESI]`; stack contains context and masked selector zero |
| `004A03B0` | Return from the first F5 packed operand: EDI context, ESI arguments, low EAX WORD is the observed callback result for healthsmall |
| `004A03C9` | F5 destination write completed; register 1 is the WORD at `ctx+72` |
| `004A1150` | Render opcode05 entry: ECX context, EDX its twelve argument WORDs |
| `004A122F` | EDI context, ESI still points to the twelve arguments; all three rotations, projection/world preprocessing, and shading have completed; nested origin/basis are still installed |
| `004A022B` | Root command program consumed, EDI context and `ctx+1A8` entry still live, before allocation cleanup; retain the full entry fingerprint here |

The callback-call event and operand-return event must be correlated by the
same invocation, thread, argument address and call sequence. Merely reading
the callback object's address does not prove it executed. No tool invokes a
callback or supplies a replacement value. For this subject, the observed
register write must equal `(callback_result << 9) & FFFF`.

`004A1150` rotates current basis in XZ, YZ, then XY order, including a native
rotation call for the decoded zero second angle. Each call changes its counter.
At `004A122F`, the native source basis can be compared directly rather than
reconstructing it with diagnostic sine math. `004ED9C0` is the nonrender
opcode05 consumer; it is not a substitute hook for this rendered draw.

At `004A00E0` entry, `ctx+60` may still contain prior stack bytes. Capture its
selected value after `004A0126`, or at the proven F5/05 sites above. Likewise,
the vertex pointer at `ctx+00` must not be read as initialized before allocation.
The model entry at `ctx+1A8` is published at `004A0175` and is available at
the later instruction hooks.

## Accepted healthsmall observations

The model-selection writer at `54EE78:2DA` installed global 168 into object
`0384B150`, with type/input WORD `0083` and caller `0046D8BF`. This is the
dynamic creator path, not an authored world-22 placement. It locates the
subject; the transient draw callback object below is a different address,
and these queries do not establish object-to-draw request ancestry.

The two subsequent eight-event chains each preserve thread `C23C`, context
`001AF950`, entry `04756434`, parent zero, F5 arguments `0475684C`, opcode05
arguments `04756854`, and vertex bank `00564728`. The selected near/far byte
is 1 (far). The complete 4,696-byte entry matches the canonical SHA-256 above
at each program-completion event.

| Event | First invocation | Second invocation |
| --- | --- | --- |
| H2 entry | `5B2A7F:2586` | `5B4AB7:278E` |
| F5 entry | `5B2A80:22C3` | `5B4AB8:2458` |
| Actual callback call | `5B2A80:22D5` | `5B4AB8:246A` |
| Observed operand return | `5B2A80:22DD` | `5B4AB8:2472` |
| F5 register write | `5B2A80:22FF` | `5B4AB8:2494` |
| Opcode05 entry | `5B2A80:230D` | `5B4AB8:24A2` |
| Post05 nested state | `5B2A8B:1D85` | `5B4AC6:1F05` |
| Root program consumed | `5B2A91:1407` | `5B4ACA:1451` |

Both calls use callback object `001AFB20`, vtable `00520A44`, actual target
`004789B0`, and selector zero. Retail disassembly at `004789C9` reads
`WORD[005471C0]` directly and returns with `ret 8`; the nonzero-selector
delegation through object `+3C` is a different branch. Observed returns are
`06B0` and `06B6`, producing register-1 WORDs `6000` and `6C00` respectively.
Within each invocation the complete retained F5-entry and callback-return
context windows (428 bytes) are identical. This is an executed clock operand,
not a guessed callback value or a claim about its cadence.

### Raw memory and field validity

Early bulk reads in this replay exposed zero-filled regions inside the model
entry even though its header, name and consumed command prefix matched.
The first H2 snapshot differs from the source in 1,894 bytes, all nonzero
source bytes observed as zero; after native program consumption, all 4,696
bytes match. The index reported loaded and `DefaultMemoryPolicy` reported
`InFragmentAggressive`. No index rebuild, memory-policy override or retail
write was performed. This is an observed availability boundary, not a general
interpretation of every zero returned by TTD.

The harness therefore retains both `model-entry-before.bin` and the late
`model-entry.bin`, and accepts the draw only if the latter hashes exactly.
Locators may retain an explicitly deferred identity; they cannot certify a
draw. Context and slot dumps preserve literal bytes. The native writers,
rather than an allocated buffer's length, determine which fields are usable.

At post05, only the 84 even slots 2..168 have newly initialized projection
fields: packed XY at `+0C`, depth at `+10`, clip byte at `+14`, and fog byte at
`+15` for this far path. Their world XYZ and trailing bytes `+16/+17` are not
established by this producer. Slot 0 carries the initialized world position.
The 144-byte global snapshot at `0055F1C4` retains world origin/basis, view
origin/composed basis, and the native projection/fog/threshold/shift inputs.

### Arithmetic comparison

Using the original executable's sine table, `NativeBasisMath` applies the
observed XZ register angle, YZ zero and XY `C000` in native order. All ten
post05 basis DWORDs match each observation, including counters `48→4B` and
`49→4C` respectively.
The original-table zero-angle call still increments that counter. The first
52 published global bytes equal the observed world origin and ten-DWORD basis.
Origin and slot-0 world XYZ remain `[18794, 256, -19975]` in both draws.

The existing `ModelProjectionBackend` receives the canonical raw local
vertices and captured view transform/projection parameters. In far/local
mode it matches all **840 defined projection bytes** (84 slots × 10) in each
observation. All 84 projected slots change between the draws, differing in
192 of the 840 defined bytes. The incoming basis changes as well as the clock
operand, so this difference cannot be attributed to the callback alone. View
origin and projection parameters are unchanged. This compares packed XY,
depth, clip and fog only; it
neither promotes uninitialized world slots nor validates material 33,
lighting, face clipping, rasterization, an entire frame or complete animation.

## Minimum retained observation

Each complete healthsmall invocation retains the following data together,
leaving explicitly unobserved ancestry and presentation fields unknown:

- **Identity:** supported executable fingerprint, trace identity, TTD position
  for every event, thread, invocation ordinal, transient context address,
  parent context at `+9C`, resolved entry pointer and exact entry-byte
  fingerprint. Match those bytes to the canonical global-168 content variant
  and physical occurrence. Pointer equality alone is not durable identity.
- **Lookup provenance:** the observed root argument and H8 table/count/entry
  when the H1 chain is correlated. A child invocation can bypass H1; its model
  slot and root ancestor must not be conflated. Keep an unobserved ancestor
  unknown rather than borrowing the preceding lookup event.
- **Dynamic input:** F5 argument address/WORDs, callback object/vtable/actual
  target, observed selector and returned WORD, and post-write register 1.
  Retain before/after observations if the callback mutates other context.
- **Transform:** invocation world position `ctx+08..10`, current basis
  `ctx+14..38` as ten DWORD bit patterns, and the nested position/basis at
  `004A122F`. The counter is part of each basis. Saved basis `ctx+A0..C4`
  can be retained as raw evidence; this subject's 05 does not consume it.
- **Vertex custody:** bounded slot allocation from `ctx+00` and the matched
  entry's slot count; initialized slot-0 world XYZ; post-05 projection bytes
  at even slots 2..168. Keep each 24-byte record's world and projection fields
  separate. Unwritten raw bytes are evidence of memory contents, not valid
  coordinates. The source range and producing instruction identify which
  fields can be consumed.
- **Presentation context:** selected `ctx+60`, published transform/projection
  globals when retaining screen-space output, and the actual material-33
  registration if comparing native face appearance. A geometry observation
  alone does not establish lighting, clipping, rasterization or frame parity.

The model bytes already provide topology, raw local projection inputs, and
the exact command ranges. An observed basis can support a narrowly labelled
projection-source geometry view later. Such a view must not populate world
slots that the program never writes, or claim complete runtime execution.
The current native executor still rejects opcode05 explicitly.

For parent import `2A`, the minimum is larger: the child context and its parent
pointer, the child state-word array referenced by the pointer at `ctx+6C`,
its signed source selector, command mode bits, the selected complete parent
24-byte slot and mirrored `slot ^ 1`, plus their writer/initialization
provenance and the child transform.
Non-null-parent imports copy complete slot records; projection-only and
world-only mode names do not constrain that copy. A null parent is a different
native origin path and must be observed explicitly. A returned mesh cannot
stand in for any of this state. Start with healthsmall before this branch.

## Canonical import and captured presentation

`load_healthsmall_bundle` accepts the completed harness bundle plus the exact
canonical source-entry slice. It bounds its input files, independently checks
the eight ordered transcript events against metadata, binds raw dump blocks to
their event addresses, compares every retained binary file with the transcript,
and verifies the late full-entry SHA-256. Unsupported source bytes, incomplete
chains, inconsistent pointers, nonfinite bases, or unprojected far slots are
errors. The original trace/executable paths and byte counts remain acquisition
provenance; importing does not reopen or hash the multi-gigabyte recording.

The representation retains all raw snapshots, positioned events, callback
result/write, incoming and nested ten-DWORD bases, and the parsed source model.
Only slot 0 exposes initialized world XYZ. Even slots 2..168 expose packed XY,
signed depth and clip/fog bytes, with their other bytes preserved as raw data.
This narrow importer requires the far path and signed depth at least 64:
below that threshold the native producer stores view coordinates and leaves
fog unwritten, requiring a different validity contract.

The viewer loads one or more bundles with `--capture`, retaining their order.
Previous/Next or Left/Right selects observations. `C` switches between:

- **Captured XY:** a uniformly magnified wireframe of the observed native
  signed screen coordinates. It preserves screen Y direction, pixel
  quantization and overlapping points; no camera reprojection occurs.
- **Oriented Source:** canonical raw projection inputs transformed with the
  observed nested basis, using native integer point rounding, then shown with
  the diagnostic orbit camera. World translation is omitted for framing.
  These computed offsets are not captured or initialized world-slot values.

Both views retain all 120 authored root fan triangles with command-span
provenance and 84 projection inputs. All edges are shown without reproducing
native visibility, materials, lighting, fog blending or rasterization.
Captured values and TTD positions appear beside the geometry. Missing captures
remain unavailable; no static fallback fills the captured tier. `--snapshot`
writes a PNG through the same workbench drawing path for repeatable visual
checks. Invocation examples are in the [port README](../../README.md).

`inf-inspect` samples stable H8 registration tables and the world handle list.
It fingerprints registered model bytes and reads object position/basis/model
fields, but does not observe transient draw contexts, instruction execution,
callback returns, invocation ancestry or vertex slots. Increasing its sampling
rate does not repair that boundary.

The staged `RuntimeFrame` **schema version 1** has no producer or viewer reader.
Its fields are insufficient for the contract above: there is no TTD position, context or
entry identity, callback result/callsite/order, current basis, vertex snapshot,
or per-field custody. Its 64 “animation registers” are a raw context window:
only WORDs 0..21 are independent; 22/23 alias the parent pointer and 24 onward
overlap saved basis and other storage. Its arbitrary `child_bases` WORD vectors
do not describe the proven ten-DWORD basis. The ledger's old `mode_raw` mapping
also cannot substitute language or phase fields for a proven mode value.

Do not fabricate a v1 frame from passive table samples, fill unknown registers
with zero, record callback operands as outputs, or invent child/world state to
satisfy its shape. A concrete accepted observation should determine any future
record revision. The new source-specific observation representation is
independent of these legacy v1 records; no generic runtime-frame schema or
gameplay execution is claimed.

## Next bounded task

The first [parent-import observation](PARENT_MODEL_OBSERVATION.md) establishes
three-model ancestry and 44 initialized bytes in a 48-byte copied pair. It
remains partial: the parent's separate auxiliary stream does not yield a
matching full-entry source fingerprint. Decode and audit that consumer,
then establish parent source identity before broadening captured presentation.
The four resident candidates from the nine-variant group had no watched first
import access before the next phase request; the observed gun assembly already
has a partial mesh. Custom vertices and camera-plane dependencies remain
separate, and static presentation counts are unchanged.
