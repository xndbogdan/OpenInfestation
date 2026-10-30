# Infestation diagnostic child composition

This document owns the asset viewer's bounded Handler-8 child composition.
It is a diagnostic presentation policy, not execution of a live model program.
Intrinsic model decoding remains in [model runtime](MODEL_RUNTIME.md) and
[model asset coverage](MODEL_ASSET_COVERAGE.md) owns corpus availability.

## Decision matrix before the authored-rotation change

The baseline compositor uses signed integer permutation matrices. Its exact
decisions are recorded here before replacing that representation:

| Case | Baseline decision |
| --- | --- |
| Ready root/child diagnostic | Copy its proven local geometry and omissions |
| Child-only or runtime-dependent root with instance omissions | Start empty, retaining every omission/dependency |
| Other unavailable root/child | Reject it as a composition seed |
| Program containing top-level `0B`, `13`, or `16` | Do not assemble its children |
| Recursion cycle or depth 16 | Keep the unresolved instance |
| Missing same-source H8 winner | Keep the unresolved instance |
| Attachment outside the first three root world ranges or possibly overwritten | Keep the unresolved instance |
| Fixed child orientation, low bits other than six | Apply signed permutation; ignore higher orientation bits |
| Saved child orientation, low bits six | Require immediately preceding `03` |
| `03` raw angle WORD zero | Copy its fixed base, independent of axis; reject base low bits six |
| Nonzero packed angle resolving to callback-derived zero, axis 0..2 | Treat as the explicitly labeled neutral pose |
| Other nonzero packed angle or unknown register | Keep the unresolved instance |
| Child register state | Begin unknown; do not infer inherited registers |
| Resolved parent instance | Reduce only that parent's instance omissions |
| Entire proven chain | Remove external-catalog/saved-basis dependencies only when no instance omission remains |
| Every composite | Retain `PartialMesh`; never promote it to complete runtime geometry |

The baseline treats a parent attachment as a raw initialized world slot and
maps it through the entry transform. Presentation vertices may instead be
projection-source geometry. These banks must remain separate.

## Native basis contract

`004A0AE0` calls `00490520` to copy oriented current basis vectors into the
saved basis at context `+0xA0`, preserving that destination's DWORD counter.
Orientation bits `0..2` assign source axes to destination columns; bits
`0x20/0x10/0x08` negate source X/Y/Z. Values six and seven in this helper are
identity permutations. This differs from child opcode `0E`, where low bits
six copy all ten saved-basis words, including its counter, and ignore signs.
Other child orientations begin with a zero counter. Each child's own saved
basis is newly initialized to identity and zero counter.

The authored third WORD of `03` is tested before packed decoding. Raw zero
skips rotation; any other encoding invokes the selected rotation even when
its decoded angle is zero. Signed axis one chooses `00491530`, two chooses
`004917D0`, and every other axis chooses `00491290`.

The three rotation helpers share a 4096-entry unsigned quarter-wave table at
`0053DF00`, quadrant folding using `0x3FFC`, and the exact f32 scale at
`00528CE0` (bits `0x38000100`). Angles use 65536 units per turn; the low two
bits are discarded by the table lookup. This is neither a generated sine
curve nor a `1/32768` normalization. The name-entry draw (`0045948E`,
`00459909..00459986`) reads the same fold as a plain signed WORD without that
scale or the Q31 replication used by menu rows (`sine_q15`). The
`0x3FFC` mirror maps angle `0x7FFC` to table entry 0.

`00417DA0` normalizes three signed WORD arguments: each converts exactly to
f32, the squared length accumulates as Z, Y, then X at 53 bits, the square
root stays in a register for all three divisions, and a zero length yields
(1, 0, 0). `normalize_words` exposes it for callers that pass words directly,
such as the name-entry light; basis renormalization quantizes floats first.
Twenty unmodified executions at FPCW `027F` are retained in the kernel tests.

The native CRT selects 53-bit retained arithmetic with nearest-even rounding.
Basis products use f64 operations and only round to f32 at the original memory
stores. Each rotation increments the unsigned DWORD counter, then normalizes
when the incremented value exceeds 100; wrapping to zero bypasses that path.
Normalization retains the recovered signed-WORD quantization and asymmetric
retained/spilled length paths, rebuilding Z, Y, X, and finally Y in native
order. A generic matrix or quaternion normalization is not equivalent.

## Shared implementation

The pure kernels now live in
[`inf-formats::native_basis`](../../crates/inf-formats/src/native_basis.rs).
`NativeBasis` retains all ten DWORDs; `NativeBasisMath` requires the original
quarter-wave WORD table. The active game APIs retain their existing
`MenuBasis`/`MenuBasisMath` names through small re-exports, so existing menu,
model and frontend callers execute the same math. The viewer depends only on
`inf-formats`, never on the game runtime. The old arithmetic suite moved with
the implementation, including the x87 comparisons.

`00490900` transforms a point as `(Z product + Y product) + X product` in
retained precision, then performs nearest-even `FISTP QWORD` and keeps the low
DWORD. The model origin is added with DWORD wrapping. Using a generic f32
matrix multiply, a checked i32 sum, or a different addition order changes this
contract. The viewer uses this shared operation for child vertices and
attachments. Attachment inputs come from initialized **world** slots, while
the displayed diagnostic may use a separate projection-source bank.

The catalog reads the table once from the supported `Infest.exe` profile. A
missing/unrecognized executable preserves OVL catalog loading and fixed or
raw-zero child composition; any rotation requiring the table remains
unresolved. The model sidebar reports `ORIGINAL ROTATION TABLE UNAVAILABLE`.
No generated sine curve or skipped encoded-zero rotation is used as fallback.

## Current bounded state policy

The invocation tracks its entry origin/basis, current basis, saved vectors,
and saved counter separately. This removes the old immediately-preceding-03
restriction without treating intervening commands as harmless by default.

| Native operation | Diagnostic state transition |
| --- | --- |
| Invocation entry | Current basis is supplied; saved basis is identity with counter zero |
| `03` / `004A0AE0` | Oriented copy of current vectors, retaining saved counter; rotate only when the raw angle WORD is nonzero |
| `1C` / `004A0DA0` | Rotate saved basis, including raw angle zero; signed axis fallback matches `03` |
| `04` / `004A0AC0` → `00490370` | World-delta basis construction remains unsupported: saved vectors become unknown, saved counter becomes zero (`004904C6` / `00490509`) |
| `0F` / `004A0B60` | Camera-derived saved vectors become unknown; saved counter survives |
| `10` / `004A0BE0` | Copy all saved words to current basis; already initialized world-slot attachment origins remain unchanged |
| Child `0E`, low bits six | Copy known saved vectors and counter; ignore the orientation sign bits |
| Other child `0E` orientations | Oriented copy of known current vectors into a fresh zero-counter basis |
| Nested `05` / `09` | Current basis/origin restoration is known, but escaping saved/register/world writes are not executed; subsequent child attachments remain unresolved |
| Register write to 22 or above | Stop proving subsequent attachments because these WORDs alias the parent pointer or saved basis |

Native child invocation isolates its current/saved state from the parent and
copies only parent register WORDs 0..3. The diagnostic starts all 22 independent
register WORDs unknown at the root, then copies those four parent values at
each child call. Words 4..21 remain unknown in each child. It resolves packed
literals plus the already supported `F1`
subtraction and `F5` shift, retaining WORD wrap and callback provenance. Packed
callback inputs explicitly evaluate to zero for this viewer's neutral pose.
Known nonzero authored or derived angles are now rotated, rather than rejected
or neutralized. Other register operations invalidate their destination.

Unknown saved vectors do not imply an unknown counter: `0F`, unsupported
angles, and missing table inputs retain the provable counter transition. A
later `03` may recover vectors from known current state only when that counter
is known. The original call-101 normalization remains active; it is not
approximated or omitted for repeated rotations.

Same-source final H8 registration, attachment-range validation, possible vertex
writes, guarded-program rejection, recursion/cycle limits and omission
accounting retain the baseline policy. Every composite remains `PartialMesh`.
The composed seed is existing diagnostic geometry; this does not execute
nested programs, animation, dynamic lighting, procedural edges/effects, live
camera state or runtime callbacks. In particular, exact basis math does not
make existing diagnostic nested geometry a native-frame rendering claim.

### Attachment custody and redundant branches

The compositor's attachment barrier follows native **world XYZ** writes;
projection-only writes do not invalidate an initialized attachment. Ordinary
generated-vertex mode pairs use their low bit for world XYZ and their high bit
for projection. Parent import `2A` is exceptional: either bit copies the whole
parent slot, and a null-parent mirror-world request also initializes the
original origin. The conservative union of those two native paths remains
invalidated; no imported coordinate is invented.

`20` (`004EBF00`) and `2C` (`004EC5C0`) do not mask the mode nibble. Dispatch
`004A0212` sign-extends the command WORD; those handlers use `SAR 8` and then
consume pairs with unsigned `SHR 2`. The barrier therefore preserves upper
pairs, including sign-extension pairs. Other generated-vertex handlers mask
four bits; `2D`'s top nibble is its argument count, not additional writes.

Callback generators retain the diagnostic's **declared-output policy**. The
default renderer table `00544740` supplies `2B`/`2C` targets `004EBCD0` and
`004EBCF0`, which write only the output XYZ. World renderer setup `004501A0`
replaces them with `0044FF50` and `0044FFA0`; those also write only output XYZ.
`0044FFA0` reads the renderer origin through context `+58` on one path, without
writing that context. The inspected menu `2D` target `0043A2D0` reads context
slots through `00439E10` and writes the supplied output vector. Actor target
`0047A240` additionally writes actor `+74` for selector ten; this is not a write
to the model draw context. Merely passing context to a callback does not prove
that all later attachments were overwritten.

These targets support the existing output-slot boundary; they do not prove
every actor callback registration or side effect. The compositor does not
execute those callbacks, reconstruct their live pose, or reproduce the actor
mutation. Unknown generated coordinates and callback dependencies remain
explicit. A future concrete target that writes another draw-context field
requires a matching custody rule, not an assumed neutral result.

Opcode `16` with jump WORD three reaches the next instruction for either
comparison outcome (`004A0C90`). It is no longer an assembly guard, matching
the canonical diagnostic's equal-target rule. Unknown callback/register
dependencies survive. Other `16` targets and camera/plane guards `13`/`0B`
remain barriers; this does not select a runtime branch or assume an inherited
register value.

These corrections leave all installed retail composite geometry, presentation
metadata and child counts unchanged: **105 variants / 697 registrations / 25
unavailable-local variants**. Synthetic regressions distinguish the affected
world/projection modes, `2A`, signed upper pairs and equal versus genuine
branches. Matched software images of TURBOGROUP, SAMLAUNCHER, SPAWNER and
NEURALMAIN remain identical within each before/after viewport; this is a
correctness boundary, not an availability gain.

The next asset work should target proven generated attachment coordinates,
parent-slot imports, required callback inputs and real guarded branches.
Opcode `04` has zero records in the installed corpus, including nested streams,
so its currently unknown world-delta basis is not a retail readability blocker.
Likewise, fixed polygon families such as `E6` already retain their typed
geometry/material/lighting/culling in the canonical diagnostic and viewer.
Counts of missing **runtime executor** opcodes must not be presented as missing
asset polygon families or used to claim newly exposed meshes.

## Corpus and visual validation

Against the same retail catalog and diagnostic parser, availability changed
from **77 to 105 content variants**, **583 to 697 physical registrations**,
and **19 to 25 variants whose local geometry is unavailable**. Availability
never regressed in that comparison. These are bounded composite counts, not
counts of complete renderable game objects. A corpus regression checks the
new totals and retains `PartialMesh` for every emitted result.

The software smoke compares the saved 77-variant compositor with the new one,
using the same authored H3/H7 resources and two orbit views. Each before/after
pair shares a center and scale derived from both bounds; source vertices and
faces are unchanged. Existing local geometry is shown when the old composite
is unavailable; wholly unavailable old geometry stays blank and labelled.

| Direct retail selection | Before | After | Visible finding |
| --- | --- | --- | --- |
| `#264 TURBOGROUP`, `1.ovl` local 264 | 1 child, 38 vertices / 70 faces | 3 children, 114 / 210 | Authored rotated siblings now surround the original child |
| `#67 SAMLAUNCHER`, `1.ovl` local 65 | Local 12 / 11 | 1 child, 24 / 27 | Quarter-turn child appears while the local shell stays aligned |
| `#636 SPAWNER`, `23.ovl` local 33 | Unavailable | 1 child, 18 / 12 | A partial set of textured child surfaces becomes inspectable |
| `#705 NEURALMAIN`, `29.ovl` local 32 | Unavailable | 37 recursive child instances, 288 / 240 | Authored angular repetitions form the visible surrounding structure |

Both views of every displayed mesh produce filled fragments; these selected
meshes have zero unresolved materials. SPAWNER still has incomplete local
effects, and SAMLAUNCHER retains existing implicit-UV presentation. No claim
of complete effects, gameplay or retail-frame parity follows from this smoke.
The temporary evidence is under `.tmp/inf-asset-visual-smoke/`:
`native-basis-comparison-0.png`, `native-basis-comparison-1.png`, their text
metrics, and `basis-census.txt`; reference/generated assets were not replaced.

Focused regressions cover source-axis signs/permutations, destination counters,
point-operation order, ties-even and low-DWORD narrowing, raw versus encoded
zero, signed-axis fallback, call-101 normalization, saved/current asymmetry,
unknown camera/world-delta state, register aliases, missing-table fallback,
attachment origin stability, omission provenance and the retail census.

## Four-word child register inheritance

`004A0ED0` reads parent context `+70/+72/+74/+76` at
`004A10B9/004A10C7/004A10D3/004A10DF` and finishes storing them into the child
at `004A10FD`, immediately before child execution at `004A10FE`. It does not
copy child registers back on return. The diagnostic now preserves those four
values, including their callback-backed neutral provenance; later siblings
receive the parent's values at their own call boundaries. Nested-program and
register-alias barriers still prevent unsupported context writes from becoming
known inputs. Root inspection of a wrapper has no parent values to inherit.

The old/new decision matrix differs only at that call: previously all 22 child
registers were unknown; now 0..3 copy known/unknown parent values and 4..21 stay
unknown. Child-local `F1`/`F5` operations retain WORD wrapping and can feed a
grandchild, without changing the parent or siblings. All other composition,
guard, attachment, transform and omission decisions remain unchanged.

This exposes six additional instances under `titles` 738 in `47.ovl`: planet
models 747/749/748/754/756 and Jupiter's ring child 751. Its partial composite
grows from **7 to 13 children, 35 to 1,374 vertices, and 20 to 2,664 faces**.
The five wrappers 739..743 remain unavailable when inspected as roots because
their register-1 input is then unknown. Packed clock callbacks still use the
explicit neutral-zero policy; this is not an observed animation pose.

A full same-catalog comparison changes only this title composite; availability
remains **105 variants / 697 occurrences / 25 unavailable-local variants**.
Matched two-orbit software images use identical source materials, common
centers/scales and the old compositor. The newly visible planets/rings have
zero unresolved materials; the TURBOGROUP control image is unchanged. Local
evidence is `.tmp/inf-asset-visual-smoke/inherited-registers-comparison-{0,1}.png`
and its text metrics. These are diagnostic model images, not retail frames.
