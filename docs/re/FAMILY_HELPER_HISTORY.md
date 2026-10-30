# Infestation family helper history

This authority owns the shared `0x5C`-byte allocation at family actor `+218`,
its four history records, and the recovered record/fade/clear/query policies.
The [behavior dispatcher](ACTOR_BEHAVIOR_RUNTIME.md) owns caller ordering;
[target resources](TARGET_RUNTIME.md) own the separate current/source targets.
These policies do not depend on a mission, level, model or camera.

[`family_helper_history.rs`](../../crates/inf-game/src/object_runtime/actor/derived/family_helper_history.rs)
provides the arena owners and concrete Attack adapter. One shared
`FamilyHelperRecordRequest` retains the raw handle, WORD displacement and
DWORD strength; `FamilyHelperHistoryInputs` supplies the independent null
handle. The live helper remains in the arena while resolving records.

## Allocation and byte custody

Constructor `00482FF0` returns the supplied allocation and writes only DWORD
`+08=0`. Header DWORDs `+00/+04` and all 80 record bytes remain unknown.
Postinitialization reaches the `0x5C` allocation only with null actor `+218`
and nonnull actor virtual `+7C` result (behavior `+140`): selector 10 uses
`0042514A -> 00425164 -> 0042516D`, selector 12 uses
`0046811D -> 00468137 -> 00468140`. Actual allocation null publishes null.
The constructor does not establish the header handle/timer later consumed by
Attack entry/update. Strict and old four-carry arenas preserve that unknown
backing. The separately selected
[FreshHeapZeroExtendedCarryApproximation](../../crates/inf-game/src/fresh_heap.rs)
supplies only fresh helper `+00=0` inside these two gates; `+04` and every row
remain unknown. Existing helpers and supplied/written headers are never reseeded.
Attack's actual handle mismatch writes `+00` and `+04=0`; equality still requires
the independent timer before signed halving. This replaceable backing policy is
not a native constructor write or acceptance of allocator history.

Each row begins at `+0C + 20*index`; production count starts at zero and
recording retains the four-row capacity.

| Row offset | Native field | Portable custody |
|---|---|---|
| `+00` | Raw actor-handle DWORD | Optional handle |
| `+04/+08/+0C` | Three binary32 direction DWORDs | Independent optional bit patterns |
| `+10` | Unsigned weight WORD | Optional WORD |
| `+12` | Untouched padding WORD | Independent optional WORD |

A DWORD-only array cannot represent a newly written weight with unknown
padding. Recording writes the low WORD only. Compaction copies four DWORDs
and that WORD, preserving the **destination** padding; it does not copy all
20 bytes. Copying unknown normal bits retains unknown custody and does not
invent zero values.

The process null handle `0054E894` is an explicit, conditionally required
input, independently of actor registry `0054E888`. Missing null data fails
only when a reached tail clear reads it, after earlier weight/compaction
writes. A count-zero record does not require either process source.

## Record producer: 004830B0

The only direct retail caller is common Attack `00429AE0`. It supplies the
helper captured from actor `+218` at `00429A99`, current target raw `+04`
handle, actor-minus-current-raw-`+08` WORD displacement and literal weight 90.
Target class, kind, allocation identity and virtual point are not inputs to
this service. No allocation, free or RNG occurs inside it.

Incoming weight is `min((argument & 0xFFFF) << 11, 0xFFFF)`: 31 produces
`F800`, while 32 and 90 produce `FFFF`; high argument bits are ignored.
Unsigned count below four appends without resolving any existing handle.
Append writes handle (`00483281`), low weight (`0048328B`), then reads the
three signed displacement WORDs. Direction components publish in XYZ order;
count increments only after all three stores (`00483311/0048332E`).

Length uses exact `(Z*Z + Y*Y) + X*X` arithmetic at supported 53-bit x87
precision, followed by square root **spilled to binary32 before division**.
A zero vector writes `[1,0,0]`. The shared normalizer's
[`NativeLengthPrecision`](../../crates/inf-formats/src/native_basis.rs)
policy preserves this spilled denominator independently of other
consumers' retained denominator; ordinary normalization is not a replacement.

At count four, prune handles in row order through original
`0047AD90 -> 00462190`. A missing referent or signed actor `+C4 < 3`
immediately clears that weight and increments dead count, including an
already-zero weight. A reached source failure preserves earlier clears.
If any dead handle was found, compact nonzero-weight rows in stable order,
clear vacated handles from the actual process null source, then subtract
**dead count** from count. Do not replace count with the surviving-row count:
a preexisting live zero-weight row can leave a count/active-row hole.
Without a dead handle, existing zero weights are not compacted.

After pruning, append if count is below four; otherwise replace the first
strictly minimal unsigned weight. The minimum starts at `FFFF` with index
zero, so all-`FFFF` rows and ties retain the first index. Incoming weight
need not exceed the old minimum; even zero replaces a full row. Replacement
writes the same handle/weight/direction sequence without incrementing count.
Header `+00/+04` remains untouched.

## Fade and clear

Wrapper `00425AF0` reaches fade `00483340` at `00425B4E`; the nonnull
behavior branch of selector-12 wrapper `00468D10` reaches it at `00468D67`.
Both precede common dispatch `004294A0`; selector-12's null branch skips it.

Fade returns for zero or signed-negative count before any other helper or
process read. Otherwise the step is logical `delta >> 11`. A weight strictly
above step subtracts it in the low WORD; every other weight becomes zero and
increments zeroed count. Step zero still removes a preexisting zero weight.
If any row was zeroed, stable compaction copies nonzero rows with the same
18-byte policy, clears vacated active handles, then subtracts zeroed count.
Normals, destination padding and header remain intact. No floating point,
registry, allocation or RNG is needed.

Clear `00483400` has one direct caller, `004682B2`. Count zero bypasses all
other inputs. Any nonzero count, including signed-negative count, processes
all four rows: clear weight, read actual process null, then store handle.
It preserves normals/padding/header and writes count zero only after all
rows complete. A missing null source therefore retains a cleared first
weight and the old count.

Backed nonnegative record/fade counts are 0..4. Larger positive counts would
traverse outside the original allocation and remain an explicit unsupported
input. Record admits the source-proven high-bit DWORD path: it skips
signed-positive loops, chooses row zero and preserves count. Those rows are
excluded from the 573 admitted fixture comparisons, alongside native raw
alias/pointer diagnostics. Fade's negative bypass and clear's nonzero policy
retain their actual distinct signedness.

## Read-only query and predicate boundary

Query `00483000`, directly called only at `00429D16`, has a separate read-only
[owner](../../crates/inf-game/src/object_runtime/actor/derived/family_helper_history/query.rs).
`FamilyHelperQueryRequest` supplies raw handle and WORD displacement, without
unused record weight or process sources. A zero or signed-negative captured
count returns false before request/row reads. A positive backed count scans
handles in order, evaluating each matching row until one accepts. A rejected
match continues to later rows; the query returns false if none accepts. Only reached
matching rows' three normal bit patterns are required. Header,
weight, padding, registry and process-null data are never inputs.

The dot is `(X*nx + Z*nz) + Y*ny`, using signed WORD displacement and the
stored binary32 normals at supported 53-bit x87 precision. It retains the
sum until `00511BA8`'s truncating QWORD conversion, then uses the low DWORD.
The existing [actor numeric owner](../../crates/inf-game/src/object_runtime/actor/derived/kinematics.rs)
provides that conversion. A signed-positive dot admits only when its unsigned
wrapping square exceeds logical `wrap(X*X + (Y+Z)*Z) >> 2`. Weight zero does
not bypass a matching row; the record producer's dead-count subtraction can
leave such rows active. The threshold is not ordinary squared XYZ length.

Under masked control word `027F`, known NaN/infinity or out-of-QWORD values
produce integer-indefinite `8000000000000000`, whose low DWORD is zero.
Finite values in `[-2^63,2^63)` truncate and retain the low DWORD, without
signed-DWORD saturation. FP status flags and unmasked exceptions are outside
the portable contract. Larger positive counts remain an explicit unsupported
input rather than portable out-of-allocation reads. No helper field changes.

Enclosing `00429CB0` first requires the [behavior predicate](BEHAVIOR_DAMAGE_CACHE.md);
callers are `00428CCD/004291D6/004684D2/004687FD`. Neither a correct history
builder nor the query owner alone establishes the enclosing candidate gate,
authentic weapon factories or complete actor `+24` execution. The separate
gate owner now composes the predicate and query.

### Enclosing predicate cache: 0048D820

The [behavior damage cache](BEHAVIOR_DAMAGE_CACHE.md#enclosing-predicate-cache-0048d820)
owns the shared executable descriptor bytes, vehicle copy, damage packet
transfers and predicate body. Required creation, component-query and particle
deletion services remain separate boundaries.

### Family candidate gate: 00429CB0

The shared [candidate gate](BEHAVIOR_DAMAGE_CACHE.md#family-candidate-gate-00429cb0)
captures helper presence before the predicate and reloads the current helper
and actor positions afterward. Its kernel and supplied-service comparisons
are owned by that document; complete caller dispatch remains separate.

## Native evidence

The tracked checker executes unchanged
constructor, record, fade, clear and resolver instructions against supplied
header/records, registry, process-null and displacement inputs. Its
[compact fixture](../../crates/inf-game/tests/fixtures/family_helper_history_native.json)
covers 590 histories and 604 calls: 573 admitted cases and 17 diagnostics.
The [Rust receipt comparison](../../crates/inf-game/src/object_runtime/actor/derived/family_helper_history/native_receipts.rs)
checks all 573 admitted histories and their 587 ordered calls; focused tests
also cover actual wrapper fade and the concrete Attack recording adapter.
It keeps repeated calls, ordered writes, known/unwritten spans, partial failures
and native-only diagnostics distinct. These comparisons establish supplied
kernel behavior; they do not establish allocator reuse or retail cinematic
acceptance.

The query checker executes unchanged
`00483000/00511BA8` against independently supplied helper spans and complete
WORD displacement. Its [fixture](../../crates/inf-game/tests/fixtures/family_helper_query_native.json)
has 525 cases: 514 portable comparisons and 11 pointer/alias/read-stop or
overcapacity diagnostics. The [Rust comparison](../../crates/inf-game/src/object_runtime/actor/derived/family_helper_history/query/native_receipts.rs)
checks every portable result or reached missing-span error and unchanged
storage. Cases pin continued duplicate matches, unused unknown fields,
X/Z/Y normal read order, masked nonfinite/QWORD conversion, strict equality
and the unsigned wrapping threshold. No predicate factory is substituted.
