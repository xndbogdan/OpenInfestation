# Handler-8 auxiliary bytecode

This document owns the separate byte stream consumed by `FUN_004F2C50`.
The renderer WORD program remains in [model runtime](MODEL_RUNTIME.md);
container layout is in [format documentation](FORMAT_DOCUMENTATION.md).
Canonical decoding is in
[model_auxiliary.rs](../../crates/inf-formats/src/model_auxiliary.rs).
Structural decoding alone does not establish execution, collision-policy
coverage or source identity for a replay snapshot. The
[retained graphics and ACTIVE auxiliary owner](MODEL_PROCESS_GRAPHICS.md)
executes the generic05/07/18/2F/30/8E+00/[89..8D](#group-tests-898d)/forward95/88 domain with actual process/caller
loans. It reuses the [shared contact geometry owner](CONTACT_GEOMETRY.md)
for composite, direct point and sphere callbacks. Other direct callback policies
remain separate required work.

## Admission, storage and alignment

At `004F2C69`, header WORD `+0E != 0` admits the consumer. The numeric value
is not an instruction count. Visual initializer `0047A050` publishes it as radius WORD
`+40`, and the [shared furniture-radius producer](TERRAIN_RUNTIME.md#shared-furniture-radius-publication-004508e0)
uses its unsigned magnitude; this auxiliary admission only tests nonzero.
The stream
begins at entry `+20`. Header BYTE `+06` gives its length in **DWORD units**,
and raw XYZ begins at `entry +20 +4*header[6]` (`004F2CCF`). The historical
`auxiliary_count`, `raw_0e`, and raw four-byte `auxiliary` records remain in
the lossless schema; `auxiliary_program` exposes actual instruction boundaries.
An inactive gate leaves all raw bytes intact without interpreting them.

Let `p` be the opcode offset relative to the aligned runtime entry. Define:

* `W = (p+2) & ~1`: ordinary WORD arguments.
* `V = (p+3) & ~1`: shared vertex-handler arguments.
* `D = (p+4) & ~3`: callback DWORD arguments.

The auxiliary start is itself aligned at entry `+20`, so these formulas also
work relative to the auxiliary span. **Do not align absolute OVL offsets**:
the loader copies sub-blocks into aligned allocations, while their file
offsets can be odd. All exported spans are absolute OVL offsets, computed
only after relative decoding. Skipped/alignment bytes are retained verbatim.

The dispatcher consumes a byte, not a renderer command WORD. `88` ends the
stream and releases temporary storage at `004F3502`; remaining declared
auxiliary bytes are preserved as padding. Unknown opcodes call `004F3650`
and enter a native failure loop, so the parser rejects them. It never scans
past the declared span into vertices looking for a terminator.

## Grammar

Sizes and next pointers below are in **bytes**. All branch displacements are
signed 16-bit byte distances from `W`, not from the opcode or its successor.

| Opcode | Arguments / next pointer | Recovered role |
| --- | --- | --- |
| `05` | `W + i16[W+4]` | External model ID, orientation, byte extent, attachment/state WORDs from `W+6`; recursively calls this auxiliary consumer |
| `30` | `W+5` if byte `W+0 == 0`, otherwise `W+9` | One or two packed rotations with asymmetric orientation flags |
| `31` | Three signed WORDs, `W+6` | Vertex slots forming a saved basis through `004A0940` |
| `36` | Four WORDs, `W+8` | Signed operation index, signed destination register, packed left/right operands |
| `87/93/94` | `p+1` | No operation |
| `88` | End | Release temporary allocation and publish best positive result |
| `89` | Four signed vertex slots, `W+8` | Request callback `+0C`; failure jumps to saved group fallback |
| `8A` | Three signed vertex slots, `W+6` | Request callback `+10`; same failure rule |
| `8B` | One retained WORD, `W+2` | Require a nonzero result or jump to saved group fallback |
| `8C` | One signed displacement, `W+2` | Arm group and store fallback `W+displacement` |
| `8D` | `p+1` | Commit an armed group |
| `8E` | Signed DWORD scalar + signed vertex slot, `D+6` | Request callback `+00` |
| `8F` | Three signed DWORD scalars + signed vertex slot, `D+14` | Request callback `+08` |
| `90` | Two signed DWORD scalars + signed vertex slot, `D+10` | Request callback `+04` |
| `95` | One signed displacement | Fall through to `W+2` only when result flag is exactly one; otherwise `W+displacement` |

Request `+1C` owns the direct five-callback table. Scalar inputs feed native
integer world-coordinate checks; they are not screen pixels or fixed 16.16
values. Callback-specific shape policies are not synthesized by the decoder.
`89/8A` slot words are signed at `004F3B37..4D` / `004F3C75..81`, although
other selected policies may ignore those arguments.

Shared vertex handlers take `V` as arguments and **the opcode shifted left
eight bits** as the handler command (`004F2D6B` and sibling dispatch blocks).
The intervening byte is skipped, not a count or mode. It happens to be `01`
or `04` in retail; interpreting it as the high command byte would change
native flags and callback lengths.

| Auxiliary opcode | Renderer handler opcode | Native handler | Argument bytes |
| --- | --- | --- | ---: |
| `07` | `20` | `004EBF00` | 2 |
| `18` | `23` | `004EC410` | 6 |
| `2C` | `25` | `004EC810` | 6 |
| `2D` | `24` | `004EC710` | 8 |
| `2E` | `21` | `004EC1E0` | 4 |
| `2F` | `26` | `004EC950` | 8 |
| `3D` | `22` | `004EC300` | 6 |
| `49` | `2B` | `004EC510` | 4 |
| `61` | `27` | `004ECCE0` | 12 |
| `7E` | `28` | `004ECAE0` | 10 |
| `85` | `2A` | `004ED350` | 4 |
| `96` | `2C` | `004EC5C0` | 4 |
| `97` | `2D` | `004EC670` | 22 |

For `97`, `0x9700 >> 12 == 9` selects nine extra WORDs after its two fixed
arguments. Raw WORD operands and effective command values are exported;
sharing a handler does not prove identical caller state.

Command transport is caller-specific. Renderer dispatch `004A0212/17`
sign-extends its command WORD to a DWORD before calling the selected handler.
Auxiliary dispatch instead clears EAX at `004F2D3C`, loads the opcode BYTE,
then shifts that positive DWORD left eight bits. Thus auxiliary `96` supplies
`00009600`, while renderer WORD `962C` supplies `FFFF962C`; they are not
interchangeable handler inputs. Shared execution needs separate handler
identity and native signed-DWORD command custody, not an invented renderer
WORD with the auxiliary high byte. Stream wrappers are further caller
policies: `004EDD80` masks AH with `F5` before reaching `004EC5C0`.

## State and execution boundaries

### Consumer setup and reached writes

`004F2C50` takes the five-DWORD contact result in ECX and the supplied request
in EDX. Request `+00` names the working context, `+08` the model entry and
`+1C` the direct five-callback table. The entry sequence is:

| Boundary | Reached reads and writes |
| --- | --- |
| `004F2C5E` | Clear contact flag `+00` before either admission test; retain its other four DWORDs |
| `004F2C69` | Test header WORD `+0E` for zero; zero returns with no allocation or context reset |
| `004F2C88..9B` | Compute wrapped entry `+20`; its null result also returns without allocation/reset |
| `004F2C9C/A2` | Clear context world-origin DWORDs `+08/+0C/+10` |
| `004F2CA7` | Install the shared default-table address `00544740` in context `+3C`; do not write the table's selected slots |
| `004F2CC1`, `004EBE60` | Reserve unsigned header WORD `+08` vertices at 24 bytes each, then two DWORD shades; publish context `+00/+04` pointers and advance process scratch cursor `0065E728` |
| `004F2CCF/D3` | Publish raw XYZ pointer `entry+20+4*header[6]` in context `+64` |
| `004F2CDA..004F2D37` | Construct a stack publication basis, overwrite all ten words from context `+14..+38`, set the stack publication origin to zero and call `0048FB40` |

`004EBE60` initializes neither vertex bytes nor the two shades. Top context
registers `+70..+9B`, saved basis `+A0..+C7`, parent/state pointers and other
fields are not initialized by this entry. Preserve actual preceding writes;
otherwise these fields remain unknown. The `0040D6A0` call at `004F2CDA`
constructs the stack publication basis, not context `+A0`. Renderer ambient
seeding, header-range vertex initialization, frustum/depth admission and
Near/Far selection are absent. The wrapper `004F2C40` tests header BYTE `+07`
bit `08`, then unconditionally jumps here; that test is no extra admission
gate.

The interpreter starts with best and previous score `-1`; their vectors have
no entry writer. At `88`, `004F3506` restores the saved scratch cursor before
publishing the result. A positive best score writes flag one, score, then
normal X/Y/Z DWORD bits at `004F351B..2E`; otherwise only the flag is cleared.
Release does not zero context pointers or allocation bytes. Unknown opcode
failure loops at `004F3500` without release. A portable stopped invocation
must retain reached result/context/table/vertex mutations and allocation
custody; automatic clearing or restoration would invent a native suffix.

### Shared vertex state matrix

Let `d` be the signed first argument WORD and `p` the zero-based two-bit
pass. A destination advances to `d+p` on **every** pass, including inactive
ones. Ordinary source indices XOR zero on the first pass and one on every
later pass. `W` below means write the three world DWORDs at slot `+00/+04/+08`;
`L`/`P` mean call selected local/world projection at context `+3C` table
`+1C/+20`. World writes precede a following projection call. These names do
not imply that the destination's other bytes were initialized.

| Auxiliary / native handler | Inputs and callback ordering | Effective passes for its auxiliary command |
| --- | --- | --- |
| `07` / `004EBF00` | Read signed raw XYZ at `context+64+(d>>1)*6` before testing mode; capture once and negate captured X between passes | `W+L`, then mirrored `W`; no prior slot read |
| `18` / `004EC410` | Read two source world triples; wrapped component sum divided by two with truncation toward zero | Inactive pass 0, then `P` of mirrored sources; destination XYZ remains unwritten |
| `2C` / `004EC810` | Read two source world triples and current transformed zero-local origin; compute source A+B-origin | Inactive pass 0, then `W+P` |
| `2D` / `004EC710` | Read three source world triples; compute A+B-C with DWORD wrap | `W`, then `W+P` |
| `2E` / `004EC1E0` | Read source world triple and current transformed zero-local origin; compute twice-origin-source | `P`, then `W+P` |
| `2F` / `004EC950` | Resolve packed weight before the mode loop/source reads; then read two source triples and apply the recovered integer interpolation | `W+P` on both passes |
| `3D` / `004EC300` | Capture the source pointer; draw process RNG then read X, draw then read Y, draw then read Z, before destination writes | `W`, then `W+P` |
| `49` / `004EC510` | Invoke `context+58 -> +50` table `+10` with the source-slot pointer before consuming its temporary XYZ | `W`, then `P` |
| `61` / `004ECCE0` | Resolve packed weight before the mode loop; read four source triples for the native integer cubic | `W` on pass 0 only |
| `7E` / `004ECAE0` | Resolve two packed weights in argument order before the mode loop; sum their unsigned low WORDs for interpolation of two source triples | `P`, then `W+P` |
| `85` / `004ED350` | Nonnull parent: signed state WORD indexes the parent slot; copy its entire 24 bytes. Null parent: compute zero-local world origin and copy its XYZ to the next destination | Parent copies into `d` and `d+1` from slot and slot XOR one; null-parent command `8500` writes only both XYZ triples, with no projection |
| `96` / `004EC5C0` | Invoke `context+58 -> +50` table `+14` with each source-slot pointer; consume temporary XYZ; successful `P` return stores wrapping EAX-20 at destination `+10` | Four passes: `P`, `W`, `W`, `P`; source selectors `0,1,1,1`, destinations `d,d+1,d+2,d+3` |
| `97` / `004EC670` | Invoke object at context `+44`, vtable `+04`, with signed selector and the same nine unsigned extras on each active pass; reuse one temporary | `W+P`, then `W`; callback output is unknown until written, and later unwritten components retain earlier temporary writes |

Handlers `004EBF00` and `004EC5C0` use initial signed DWORD `SAR 8` followed
by logical `SHR 2` until zero; the other pair loops mask the initial mode to
four bits. A negative renderer command can therefore produce sixteen chunks
in these unmasked loops. `004ED350` has its separate bit tests rather than a
pair loop. Raw `20` reads XYZ even when its shifted mode is zero; `2C` zero
mode reads argument words and computes addresses but invokes neither terrain
nor projection and reads no source XYZ. A common zero-mode input bypass
would change one of these native policies.

The default Near/Far projection callbacks establish packed XY `+0C`, depth
`+10` and clip BYTE `+14`. Far additionally establishes fog BYTE `+15` when
depth is at least 64; its below-64 path and both Near callbacks leave that
byte untouched. Neither writes `+16/+17`. The handler's successful
return store changes only `+10`; the `96` depth bias does not change the
projector's XY/clip/fog calculation. A stopped callback can leave a partial
tail or temporary, so unknown fields cannot be filled from the world triple
or a captured projection. Packed weights may likewise call the live object
or read context words beyond the independent register owner.

The native command checker
executes unchanged `004EC5C0` and the real flat-resident `0044FFA0/00450A30`
copy path with an explicitly supplied mutable world projector. Its
[fixture](../../crates/inf-game/tests/fixtures/model_terrain_project_command_native.json)
separates command widths, all destination passes, source selection, return
bias and stopped-write prefixes; it does not execute this auxiliary stream
or establish arbitrary terrain/projector callback custody.

`ModelSurfaceVertexRequest` preserves these command-origin differences.
The separate [renderer surface binding](MODEL_RUNTIME.md#renderer-surface-callbacks-and-table-custody)
owns the default/terrain Ground and Project pair; auxiliary `+3C` publication
alone does not establish the object reached through `context+58`.

### Projection-table publication and alias custody

Default table `00544740` contains Far local/world entries
`004EB8E0/004EBAA0` at `+00/+04`, Near entries `004EB5A0/004EB730` at
`+08/+0C`, and static nulls at the selected `+1C/+20` slots. Searching only
literal references to `0054475C/00544760` misses the actual writers: regular
renderer entry `004A00E0` obtains the table through context `+3C` at
`004A0141` and mutates that alias.

With model header BYTE `+07` bit `20` clear, signed DWORD comparisons first
cull when `(0055F200 - model[+00]) >= 0055F24C`. Otherwise
`(0055F200 + model[+00]) >= 0055F248` selects Far/context BYTE `+60 = 1`;
the opposite branch selects Near/zero. Arithmetic wraps at DWORD width.
Header bit `20` selects Near directly. The temporary vertex/shade allocation
`004EBE60` precedes publication, so a culled invocation never owns these
writes.

| Reached policy | Source table slots | Selected `+1C/+20` writer IPs |
| --- | --- | --- |
| Regular Far draw | `+00/+04` | `004A014D/004A015B` |
| Regular Near draw | `+08/+0C` | `004A0158/004A015B` |
| Child return, parent Far | Parent `+00/+04` | `004A1118/004A111B` |
| Child return, parent Near | Parent `+08/+0C` | `004A112A/004A112D` |

Renderer child `004A0ED0` calls `004A00E0` at `004A10FE`, then restores the
parent's projection pair according to the parent's BYTE `+60` and current
table alias. These are ordinary rendering policies, not an auxiliary-specific
initializer. Copies made through `0041D3F8` and into `00546E60` through
`004386FC` have distinct table custody. Terrain customization `00450201..08`
copies only default `+00` into its caller's table; it does not publish the
default table's selected pair.

The portable [model execution owner](../../crates/inf-game/src/model_execution.rs)
selects projection before renderer vertex initialization and restores it
after normal child completion. The [shared recording adapter](MODEL_PROCESS_GRAPHICS.md#startup-selected-aliases-and-real-publications)
now preserves the actual process default/copy identity and reached selection.
File-backed NULL selected slots and an unreached renderer-copy phase remain
distinct from Near/Far. Auxiliary setup performs neither selection nor
renderer `ModelVertices::initialize()`.

The accepted default-table history
identifies regular Near publication and parent Near restoration at these exact
addresses in one recording interval. It establishes neither auxiliary execution
nor a universal initial projection policy.

### Child and control state

Child `05` builds a separate context, points its `+6C` state array at `W+6`,
sets parent `+9C`, and takes world attachment XYZ from the signed first state
WORD. It transfers registers 0..2, incrementing register 0 with WORD wrap;
register 3 is not copied, unlike renderer child opcode `0E`. The child world
basis `+14` and saved basis `+A0` are constructed at `004F2F6E/7A`, and its
pose helper at `+C8` at `004F2F86`; this does not initialize the top context's
saved basis. Context `+3C..+5C` is copied as nine DWORDs, retaining its callback
and resource aliases rather than cloning their targets. Orientation low bits
equal to six select the parent's saved basis; other values orient its world
basis. The child request copies fifteen DWORDs and subtracts the attachment
XYZ from request `+10/+14/+18`; unknown request extension fields remain
unknown. Child call is `004F30DF`, global restoration `004F30ED`, and
next-pointer advancement `004F30F2`. Auxiliary recursion does not perform
the renderer's projection-pair selection/restoration.

Rotation `30` retains `[second_present, mode1, angle1:u16, orientation1:i8]`
and optional `[mode2, angle2:u16, orientation2:i8]`. The first record applies
orientation when its mode is nonzero; the second when its mode is zero.
Those paths rotate only when angle is nonzero; the other paths always call
the rotation helper, `004A0E10` (called at `004F2EFF` and `004F2F41`): it
resolves the packed angle, then turns the basis at context `+A0` by the
angle's low WORD, axis 1 through `00491530`, axis 2 through `004917D0` and
any other through `00491290`. Signed orientation shifts right two bits for
basis selection; its low two bits select the rotation axis. Packed angle resolution
uses `004ED4B0` and low-WORD turn units. Register operation `36` resolves the
right operand before the left and writes a low WORD through table `00544C80`.
Each operator takes the left operand in DX and the right on the stack, both
as WORDs (zero-extended unless noted), and the low WORD of its answer is
kept:

| Slot | Operator | Answer |
| --- | --- | --- |
| `+00` | `004F11C0` | left + right |
| `+04` | `004F11D0` | left - right |
| `+08` | `004F11E0` | left * right |
| `+0C` | `004F11F0` | left / right, or left when right is 0 |
| `+10` | `004F1220` | left >> right (logical) |
| `+14` | `004F1230` | left << right |
| `+18` | `004F1240` | the larger, unsigned |
| `+1C` | `004F1250` | left when below right, else right - 1 (unsigned) |
| `+20` | `004F1270` | (left << 16) / right, unsigned, or left when right is 0 |
| `+24` | `004F12A0` | left >> right, the left sign-extended |
| `+28` | `004F12B0` | the context's `+44` callback's first virtual at left + right |
| `+2C` | `004F12E0` | left when right is at most left, else 0 (unsigned) |
| `+30` | `004F1300` | left when right is at least left, else 0 (unsigned) |
| `+34` | `004F1320` | left times the Q31 sine of right (`0053DF00`), `>> 31` |
| `+38` | `004F1380` | the same with the cosine (right + `0x4000`) |
| `+3C` | `004F13F0` | left & right |

There is one group fallback, not a nesting stack. `8C` arms it and resets
score to -1. `89/8A/8B` fail on a zero flag and disarm the group. `8D` does
not disarm after committing. `95` instead tests equality to one. Successful
`05/8E/8F/90` results and armed `8D` commits inherit the previous vector/score
when their score remains -1; best-result promotion is suppressed when the
next opcode is `95`. Group callbacks `89/8A` only test and route their result;
they do not perform this promotion themselves. End publishes a result only
for best score greater than zero.
The parser records both branch outcomes without running any of these effects.

Both branch targets must name decoded instruction starts or the terminator.
Backward branches are represented without following them. Child extents
below eight overlap fields the native consumer reads unconditionally; this
layout is explicitly **unsupported by the bounded structural decoder**, not
a native validation rule. Odd forward extents are accepted, with any partial
state WORD retained in raw argument bytes. All installed child extents are
positive even values 8, 12 or 26.

### Group tests `89..8D`

The [interpreter](../../crates/inf-game/src/model_auxiliary.rs) runs 89 and
8A through the request's table:

| Table | `+0C` (89) | `+10` (8A) | Test |
| --- | --- | --- | --- |
| Ray `00544C20` | `004F3B20`, four vertices | `004F3C60`, three | Clear the flag. The vertices must straddle the ray's axis in x and in y (an OR of the values above zero, or all of them negative, misses) and lie neither all behind the origin nor all past the length `+38`. A hit sets the flag and `+10` to the length less their mean depth: the wrapped sum `SAR 2`, or over three through `IMUL 55555556` |
| Sphere `00544C08` | `004F2720` → `004F2930` | `004F2930` | The plane through the first three vertices against the sphere at `+10..+18` of radius `+20`. A miss clears the flag; a hit sets it and, when `+10` is negative or above the penetration, writes the unit normal and the penetration |
| Point `00544BF0` | `004F2710` → `004F2730` | `004F2730` | The same plane test at radius 0 |
| Composite `00544C68` | `004F2B30` | `004F2B30` | The [group direction](CONTACT_GEOMETRY.md), as in the cylinder and box tables (the port reaches the box table's) |
| Terrain `00537090` | `0044FBE0` | `0044FBE0` | The flag and `+10` one |

The [plane test](../../crates/inf-game/src/model_auxiliary/group.rs) takes
its edges as WORD differences from the first vertex. The cross product, its
length and the normal's second and third components pass through `f32`; the
first stays in the x87 register for the distance, which `FISTP` rounds. A
zero cross product gives the normal (1, 0, 0). The ray tests are in
[`ray.rs`](../../crates/inf-game/src/model_auxiliary/ray.rs).

A miss at 89, 8A or 8B inside a group jumps to 8C's end and closes the
group; 8D publishes an open group as a hit callback does. A miss before any
8C would read `+3C` unwritten, and the port refuses it.

`check_auxiliary_groups.py` runs the
unchanged routines under Unicorn. Its
[fixture](../../crates/inf-game/tests/fixtures/auxiliary_groups_native.json)
holds 2,207 cases, 872 of them hits. The first 159 are the calls Infest03
recorded (`auxiliary-groups.js`):
139 of `004F3B20` and 20 of `004F2720`, each reproduced under emulation before
it joins. The port matches every word. The 47 sight tests in Infest03
(Helniazine) whose march reaches a furniture group
(`sight-march.js`) replay with
their recorded type, point, furniture and length.

### Saved-basis rotation `30`

The [execution owner](../../crates/inf-game/src/model_auxiliary/rotation.rs)
preserves both mode tests from `004F2EDD..004F2F48`. In the first record,
nonzero mode copies oriented current basis into saved `+A0`; in the second,
zero mode performs that copy. A copy uses signed orientation BYTE shifted
arithmetically by two through `00490520`, preserving the saved counter.
It occurs before optional packed-angle evaluation, so a failed value read
retains that write. After a copy, packed angle zero skips rotation; without
a copy the rotation runs even for zero. Axis low bits 1 and 2 select
`00491530` and `004917D0`; 0 and 3 select `00491290`. These operations alter
saved basis only, never current basis or model graphics publication.

The shared packed-immediate decoder and original `0053DF00` sine table own
the arithmetic. Explicit captured-object sources borrow `ObjectModelCallbacks`
for reached packed virtual values, retaining the original object across H8
children. Independent register or unavailable callback custody remains a typed
required value. Direct actor `0047CEB0` constructs saved identity at its caller;
interpreter entry does not reset it. Process B writes its retained `B+A0`
owner, separately from current `B+14`. Native
[rotation receipts](../../crates/inf-game/tests/fixtures/combat_auxiliary_rotation_native.json)
cover the mode asymmetry, all axes, signed orientations, nonzero saved counter
and register-source boundary under FPCW `027F`.

### External child `05` and interpolation `2F`

[The child owner](../../crates/inf-game/src/model_auxiliary/child.rs) captures
the signed attachment slot's world XYZ, chooses saved basis for orientation
low bits 6 or constructs an oriented current basis otherwise, and resolves the
actual child through the retained current H8 manager. The child keeps the
parent callback source, policy table, radius and counterpart identity. It
rereads attachment XYZ after model lookup when subtracting from the copied
relative query center. Its saved basis starts at the native constructor's
identity; auxiliary entry then clears child origin exactly as for other working
contexts. Only normal recursive return republishes the original parent context
and promotes the contact through the usual prior/best-result rules. A failed
child retains reached graphics, pool and contact state.

The native child also carries parent/state pointers and three register WORDs,
incrementing the first. Those fields are not fabricated: current admitted
operations do not consume them, and packed register requests remain typed
boundaries. The [child receipts](../../crates/inf-game/tests/fixtures/combat_auxiliary_child_native.json)
retain those additional native fields as evidence, alongside argument identity,
saved/current basis, query copying and normal-only restoration.
The [child comparison tests](../../crates/inf-game/src/model_auxiliary/tests/child.rs)
run the captured basis/query operands against real retained H8 geometry and
compare them to recursive child execution. This substitutes a real child for
the fixture's supplied child return and does not claim its synthetic pointers
or unowned register metadata. A reached actual child packed-value failure
separately verifies that parent publication has not been restored.

`18/2F` invoke the same `004EC410/004EC950` owners as renderer `23/26` with an
explicit positive auxiliary command DWORD. The shared data-bearing vertex
request retains command origin, signed destination/source slots and midpoint
or interpolation policy. Interpolation resolves its packed weight before the
mode loop or source reads. The existing wrapped arithmetic, source parity,
destination advance, projection and partial-write behavior are shared; an
auxiliary opcode is never encoded as an invented renderer WORD. Auxiliary
`18` deliberately projects mirrored sources without writing its destination
XYZ; auxiliary `2F` writes and projects both passes.

The original leaf32 chain `698 braineye → 699 eyemid → 700 eyetop → 701 eyeball`
uses these operations and live actor packed selectors. Its source bytes are
ordinary authored input, not a model-ID exception in the interpreter.

### Required production custody

The connected [ACTIVE owner](MODEL_PROCESS_GRAPHICS.md#owned-generic-grammar-and-geometry)
consumes actual H8/context/request identities, current process projection
history, the shared byte arena and mutable contact custody for its explicit
05/07/18/2F/30/8E+00/sphere8F+08/89..8D/forward95/88 domain. `004F2BE0` captures B-A before working-context
zero stores; Composite544C68+00 recurses through captured counterpart H8.
Focused execution cases consume unchanged original 223/51 and 492/392
programs through that domain. Decoding other operations does not admit their
execution or complete their context. `ModelVertices::initialize()`, ambient
seeding, saved-basis initialization and renderer child register copying remain
absent from auxiliary entry.

Sphere-table `8F+08` passes three authored signed DWORD half extents and the
signed vertex slot to [sphere-box `004F1F90`](CONTACT_GEOMETRY.md#sphere-box-004f1f90).
It retains the callback's strict gates, fixed post-CRT cardinal vectors and
reached contact writes. Composite `8F+08`, the box wrapper `004F2690`,
runs the counterpart's program with the box table `00544C50` and the half
extents, as `004F18B0` does with the sphere table; there the counterpart's
`8E` reaches [box-sphere `004F15F0`](CONTACT_GEOMETRY.md#box-sphere-004f15f0)
and its groups the group direction. Helniazine's crystal extractors (H8
430) are boxes, and the vehicle (H8 117) meets one through it (a
vehicle drove through them while the port stopped there, 2026-10-18).
Point-table box, cylinder policies, unowned packed registers and other
shared vertex commands require their actual producers at reach. Full
`0047BFD0` response remains a separate boundary. A named outer registered-mark
unwind can preserve independent actor progress while returning the original
required error; it is not an auxiliary return or native contact acceptance.

## Corpus and parent source boundary

Of 849 exact slot/content variants, **354** have active auxiliary programs,
appearing in **2,526** of 5,142 physical registrations. The other 495 variants
have both zero gate and empty auxiliary span. Active variants contain 18,180
declared bytes, 2,859 nonterminal instructions and 354 terminators; physical
totals are 128,628 bytes and 20,253 nonterminal instructions. Every active
stream parses, all explicit branch targets resolve, and trailing bytes are
zero. The 228 distinct nonempty byte patterns are not 228 logical models.

| Opcode | Variant instruction occurrences | Physical instruction occurrences |
| --- | ---: | ---: |
| `05` | 3 | 6 |
| `07` | 1,365 | 9,250 |
| `18` | 18 | 242 |
| `2F` | 5 | 10 |
| `30` | 5 | 10 |
| `89` | 75 | 277 |
| `8C/8D` | 11 each | 50 each |
| `8E` | 932 | 7,404 |
| `8F` | 85 | 436 |
| `95` | 349 | 2,518 |

The three auxiliary child references form `698 -> 699 -> 700 -> 701`.
They are distinct from renderer child assembly.

For leaf 32 parent slot 553, the 32-byte source stream is:
`07(slot 0), 8E(scalar 418, slot 0), 95(failure -> offset 30),
07(slot 34), 8E(scalar 22, slot 34), 88, padding 00`.
This explains its source bytes but does not repair the literal all-zero
auxiliary span in the retained TTD draw. Renderer completion does not execute
this stream. The [parent observation](PARENT_MODEL_OBSERVATION.md) remains
partial until full source identity is independently established; no source
bytes are substituted into replay evidence and the fingerprint gate is intact.
