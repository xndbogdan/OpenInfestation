# Infestation family target selection

This authority owns the common family scanner `00428BB0`, inline-table
selection `+100 = 00428DB0`, selector-12 `+100 = 004686E0`, common actor-list
selection `+104 = 00428F40`, selector-12 `+104 = 00468350`, and embedded
handle-entry services `00482F50`, `00482FD0` and `00482D50`.
The policies use actual actor/table identities, process tasks and
the shared behavior predicate; no mission, model or placement selects them.
[Target resources](TARGET_RUNTIME.md) own the weighted
publisher and its allocation custody, while the
[behavior cache](BEHAVIOR_DAMAGE_CACHE.md) owns the
candidate gate's factory composition.

[family_selection.rs](../../crates/inf-game/src/object_runtime/actor/derived/family_selection.rs)
implements these shared owners. `FamilyTargetTable` retains inline ownership,
`FamilyTargetScanOutputs` names primary-only or both channels, and
`FamilySelectionCandidate` distinguishes unwritten, null and actor custody.
`FamilySelectionHost` borrows actual process tasks and the existing animation
clock only at their reached phases. `FamilySelectionServices` binds the concrete
particle adapter without copying task, clock, pool, RNG or predicate backing.
[selector12.rs](../../crates/inf-game/src/object_runtime/actor/derived/family_selection/selector12.rs)
owns the selector-12 override as a separate private policy. The existing
`select_family_targets(FamilySelectionRequest { actor }, ...)` entry selects
common or selector-12 policy from the actual represented `Family428650Class`
hierarchy. Actor `+B4`, a scene, model, radius or unused callback argument does
not select it. The public scanner remains the distinct `00428BB0` owner;
there is no parallel public selector-12 API or compatibility rejection.
[family_actor_list_selection.rs](../../crates/inf-game/src/object_runtime/actor/derived/family_actor_list_selection.rs)
dispatches the same actor-bearing `FamilySelectionRequest` to separate common
and selector-12 `+104` policies by actual represented class. Its private
[selector-12 policy](../../crates/inf-game/src/object_runtime/actor/derived/family_actor_list_selection/selector12.rs)
retains its different captures, scoring and four publishers. The task and clock
getters lend existing owners through pure borrows; neither mutates or replaces
the owner. Mutable task borrows end before gates, and the fresh clock borrow
ends before any later mutable service.

The supported retail executable and unchanged instructions are authoritative.
The bulk C does not contain the complete common outer callback, so its literal
instruction order is necessary evidence. Isolated kernel comparisons do not
establish complete actor `+24` execution or cinematic acceptance.

## Caller matrix

These are static retail callback identities. Portable execution requires the
represented `Family428650` hierarchy; the matrix does not add unrepresented
constructors such as standalone `004293D0` to the arena.

| Final vtable | `+100` policy | `+104` policy |
| --- | --- | --- |
| `00520F34` (`00428650`) | `00428DB0` | `00428F40` |
| `00521164` (`00429E20`) | `00428DB0` | `00428F40` |
| `00520B84` (`004236D0`) | `00428DB0` | `00428F40` |
| `00520CC8` (selector 10) | `00428DB0` | `00428F40` |
| `00527EE0` (selector 36) | `00428DB0` | `00428F40` |
| `0052273C` (selector 35) | `00428DB0` | `00428F40` |
| `00521EAC` (selector 34) | `00428DB0` | `00428F40` |
| `0052104C` (`004293D0`) | `00428DB0` | `00428F40` |
| `00527058` (selector 12) | owned distinct `004686E0` | owned distinct `00468350` |
| `005269C4` (selector 13) | static `004686E0`; constructor unrepresented | static `00468350`; constructor unrepresented |

Selector 13 runs shared constructor `00467070` before installing `005269C4`
at `004606A7`. Its final table is not selector 12's `00527058`.
The reusable table services apply to the same four-entry storage in these
families; sharing storage does not authorize common `+100` dispatch.
Authentic wrapper `00425CF0` tail-dispatches current virtual `+100`; both
owned bodies ignore its two stack DWORDs and remove eight bytes on return.
Authentic wrapper `00425D00` tail-dispatches current virtual `+104`; the common
body and selector-12 body ignore both stack arguments. Selector-12 reaction
`00468220` calls its distinct `+104` at `004682EC`; the enclosing reaction is
separate from this selection kernel.

The required canonical component queries and interaction notification have
their own [shared actor-interaction authority](ACTOR_INTERACTIONS.md).
Their composition in selector-12's `00468350` traversal is owned below;
selector-13 construction and production update attachment remain separate.

Family `004236D0` and selectors 10/34/35/36 use virtual `+114 = 0043F3D0`,
which invokes current virtual `+100` with two zeros. The common callback
ignores both arguments. They do not mean clear targets. The examined base
`00428650`/`00429E20` and selector-12 `+114` bodies are actual RET `00401310`.
Periodic producers `00427490/00427510` use separate selection and distance
policies; their shared radius-score consumer does not make their scans equal.

## Embedded table lifetime

The captured table identity is its actual owner's inline `+1A0` count and
four twelve-byte entries at `+1A4`: raw ActorHandle, DWORD `+04`, DWORD `+08`.
No copied row or family borrow survives a candidate-gate callback.

`00482F50` computes the indexed row without a capacity guard, resolves its
handle through `0047AD90 -> 00462190`, then accepts signed lifecycle `+C4 >= 1`.
Unresolved or lower lifecycle writes fresh process null `0054E894` into that
row before returning null. Resolver cache custody stays with the existing
actor registry; its configured null and mutable `0054E894` are distinct inputs.

`00482FD0` writes fresh process null only for an unsigned index below four.
It does not clear either weight. Scanner lifecycle rejection is stricter than
`00482F50`: lifecycle one is resolved but subsequently invalidated.

`00482D50` preserves an unusual retail copy order. On a surviving row whose
input index differs from the earlier saved index, it copies the earlier row
into the current later row, in handle/`+04`/`+08` order. It does not move the
current survivor forward. Tail cleanup writes handles alone, then subtracts
the number of originally encountered nulls from the current count. A leading
hole can therefore propagate nulls and old weights into later retained rows.
Process null is read separately for every reached comparison and tail write.
Signed-nonpositive counts skip these reads and retain the original count bits.

The portable four-entry storage stops only on a reached unrepresented entry.
That is a semantic admission boundary, not an invented native bounds check:
an oversized count in the real inline actor can overlap later fields.
Earlier mutations survive; cleanup is not run after a stopped service or read.

## Scanner order and scoring

The scanner first caches wrapping DWORD `+164 * +164`, then invokes current
virtual `+7C`. The admitted canonical body `004027A0` returns actual `+140`.
Only its captured presence gates the primary channel. Current class `+B4`
selects `0055F100[class]` for unsigned classes below 19. Its task `+08` source
handle is read before table count, including empty or signed-negative tables.
A missing class/task owner is a reached boundary, not an empty-table success.

Each row is resolved through `00482F50`; null or signed lifecycle `<= 1`
then reaches `00482FD0` and marks cleanup pending. Otherwise candidate and
center X/Z WORDs are individually sign-extended before DWORD subtraction.
Two squares and their sum wrap as DWORDs. Y is ignored; subtraction does not
wrap as WORDs. The distance must be strictly below the cached radius square,
and the candidate's current handle must differ from the captured task source.

Shared `00427620` reads fresh radius and subtracts `0048EFC0`'s signed-input
integer square root with DWORD wrap. A squared distance with its high bit set
is therefore a nonpositive root input. This consumer is shared with the
periodic producers while their position metrics remain independent.

Primary reads entry `+08` only when captured `+7C` was nonnull; zero skips it.
Secondary reads entry `+04` after the primary gate, so gate mutations are live.
The weight policies are:

| Weight | Primary score | Secondary score |
| --- | --- | --- |
| below `02D00000` | arithmetic signed base score `>> 2` | same |
| `02D00000 .. 0B400000` | unsigned `wrapping((weight >> 13) * base) / 5A00` | same |
| at least `0B400000` | full base score | suppressed |

Each local best starts at zero and uses a strict signed comparison. Primary
invokes real `00429CB0` only on an improvement; only nonzero AL accepts it.
Secondary never invokes that gate. Rejection of a primary candidate does not
prevent the same candidate from winning secondary. The scanner reloads count
after every row and prunes only after a completed loop with invalidations.

Caller accumulators are updated last, score before candidate, only on strict
signed improvement. Native local candidate DWORDs remain unwritten until a
positive candidate wins. Replacing a negative caller score with local zero
can therefore propagate an unwritten candidate. Explicit candidate custody
must retain this fact; it is not evidence of native NULL initialization.

## Common selection callback

The common `+100` callback captures its position and compares raw `+20C` with fresh
process null. A nonnull source is resolved without adding a lifecycle gate.
An unresolved source rereads process null, writes `+210 = 0`, then `+20C`.

A resolved source supplies the initial center. Each axis is interpolated as
`referenceWORD + ((signed selfWORD - signed referenceWORD) >> 2)`, narrowing
after addition. Negative odd differences use arithmetic-floor shifting.
Actual source virtual `+58`, then returned `+CC`, may return null; admitted
pure family queries return self independently of active byte `+22C`.
A nonzero reference-table count, including signed-negative bits, invokes the
scanner primary-only. Success reads fresh `+214` and applies wrapping
`score + (signed wrapping(score * +214) >> 10)`.

The callback then reads the actor's own count and scans both channels when
nonzero, retaining the center. A self-reference can scan the same inline table
twice; all earlier pruning, table, task-source and gate changes remain visible.
The host owns actual process tasks, releasing a getter borrow before callbacks;
each scanner captures its task source anew.

Real `00427110` publishes primary `+1D4` followed by secondary `+1DC`, even
when either score is zero. A zero score bypasses referent dereference, so an
unwritten unused candidate may be represented as absent for this call alone.
First true does not suppress the second publisher; first failure does. The
result ORs both low-byte Booleans, which report previously empty pointer slots,
including actual allocation null, rather than score changes or replacement.
The shared arena-owned heap preserves target identity and released references
across publication failures; its reservation policy does not invoke callbacks
that rebind target slots or the arena.

## Selector-12 inline-table selection callback

`004686E0` is a complete policy distinct from common `00428DB0` and scanner
`00428BB0`. It consumes neither classification `+B4`, a process task, source
`+20C`, source timer `+210`, boost `+214`, nor a captured caller center. Its
selector-12 class is selected before entry; source setters and mutable classification do
not redirect it. Selector 13's identical static callback does not supply its
missing portable constructor. Its actor-list `+104` policy remains distinct
from this inline-table policy.

### Count, resolution and distance order

The callback initializes both local scores and the sticky cleanup byte to
zero, leaving candidate locals unwritten. The returned OR accumulator is
assigned after the first ordinary publisher, not initialized by this byte.
It reads radius DWORD `+164` at
`004686F1` and caches its wrapped DWORD square before count `+1A0` at
`00468706`. Count zero takes `00468882`: publish literal null and score zero
only to `+1DC`, preserving `+1D4`. This path reads no row, position, behavior,
task or process null. A nonzero signed-negative count instead bypasses the
loop/prune and reaches both score-zero publishers in `+1DC`, then `+1D4`
order. Normalizing these count values to one empty-table path is incorrect.

Each positive-loop row uses actual `00482F50`, accepting signed lifecycle
`+C4 >= 1`. There is no scanner-style second lifecycle test or `00482FD0`
clear. Failed resolution has already stored fresh process null in that row
and sets sticky cleanup; a valid outside-radius row is retained unchanged.
Read order is candidate X at `00468736`, self X at `0046873A`, self Z at
`0046873E`, then candidate Z at `00468744`. Sign-extend each WORD before
DWORD subtraction; wrap both squares and their sum. Require strict unsigned
`distanceSquared < capturedRadiusSquared`. Y is unused. The arena's numeric
input is a complete XYZ triple; independently known X/Z with unknown Y does
not become a manufactured portable triple.

Only an inside-radius candidate reaches shared `00427620`, which reads
fresh self radius and subtracts the signed-input `0048EFC0` root. Preserve
that base for both channels across the later gate. Later rows read current
self position and radius anew, while the original squared radius remains
captured for every range test.

### Secondary before primary and live callbacks

Read row DWORD `+04` first at `0046876A`. Secondary `+1DC` admits zero weight:
below `02D00000` use signed arithmetic `base >> 2`; the middle range uses
unsigned `wrapping((weight >> 13) * base) / 5A00`; at least `0B400000`
suppresses secondary. Strict signed improvement stores score before candidate
at `0046879F/004687A3`. No task/source exclusion or candidate gate applies
to secondary, and a later primary rejection does not undo it.

Then invoke fresh self virtual `+7C` at `004687AB`. A zero full-DWORD result
skips primary. Otherwise `004687B2` independently reads self `+140` and skips
if null. The shared pure canonical query owner returns this field's presence,
but these remain two reached reads; no mutable query callback is invented.
Only then read row `+08` at `004687C0`. Primary zero weight bypasses the
channel; low and middle weights use the same arithmetic as secondary, while
at least `0B400000` keeps the full base. Strict signed improvement reaches
real `00429CB0` at `004687FD`; only nonzero low AL accepts, then stores score
before candidate at `00468806/0046880A`.

No row or actor borrow survives this real mutable gate. Its admitted effects
can change the same arena's actor fields, rows/count, behavior, process/RNG,
descriptor backing and particle pool. Current base and the already selected
secondary survive; the next row performs a fresh query and direct `+140`
read. Gate-cleared behavior therefore suppresses later primary reads without
suppressing secondary. The callback reloads count at `0046881D` before
advancing the captured twelve-byte cursor and wrapping index; it uses a
signed loop comparison. A removed typed owner or reached fifth row stops at
that custody boundary rather than retargeting stale inline storage.

Only a completed positive loop with sticky failed resolution invokes
`00482D50` at `00468847`. It retains the shared reverse-copy policy and fresh
process-null comparisons/tail stores; a null written before a later notice
changes the process null need not still compare equal. Stopped reads/gates,
zero count and initially signed-negative count do not run prune.

### Reversed publisher suffix

The ordinary suffix calls real `00427110` for secondary `+1DC` at `0046885C`,
then primary `+1D4` at `00468873`. First true does not skip the second call;
first error prevents it. Return ORs low-byte Booleans reporting formerly empty
pointer slots, including allocation null. Zero-score unwritten candidate bits
are an unused semantic referent, not inferred native null initialization.
The count-zero branch alone supplies literal null to its single publisher.

The same weighted request/storage owner preserves score-before-release writes
and live/null/Released target custody on crossed-class allocation failures.
Scalar allocation cannot rebind arena, process or target slots. The typed
arena represents the captured actor/inline allocation; arbitrary replacement,
ObjectId ABA, raw pointer/list/field aliases, wrapped index-address overlap,
independent partial numeric spans and arbitrary virtual overrides remain
outside this policy's admission. This method attaches no production actor
update or allocator-carried `+220/+230` values.

## Common actor-list selection callback

`00428F40` captures `[0055F160]` and its game/list owner, then invokes self
virtual `+7C` at `00428F54` before reading radius `+164` or class `+B4`.
It caches the wrapping DWORD radius square and caller class, captures the
class task's `+08` source at `00428F9F`, then separately captures the actual
task allocation from the same class slot. These reads precede center and list
access, even with no candidates. A missing or out-of-range task is a reached
semantic boundary; it does not become a zero relation or empty-list success.

Center selection reads self `+20C`, then fresh process null `0054E894`.
Equality captures current self XYZ. A nonnull source resolves through the
actual actor registry without a lifecycle gate. With a live reference it reads
reference XYZ before self XYZ and interpolates each axis as
`referenceWORD + ((signed selfWORD - signed referenceWORD) >> 2)`.
An unresolved reference instead writes fresh null to `+20C` at `0042902A`,
then zero to `+210` at `00429030`, then reads self XYZ. This prefix order is
deliberately different from the common `+100` callback.

### Actor-list admission and scoring

The walk uses embedded actor `+A8` nodes at `M+40`, with `M=G+08`. Each
candidate first requires `(flags+2C & A0) == 20`; rejection skips its remaining
fields. Candidate-minus-center XYZ differences wrap independently as WORDs
before sign extension. The three squared products and sum wrap as DWORDs;
the squared distance must be strictly **unsigned** below the cached radius
square. This is distinct from the inline scanner's X/Z DWORD subtraction.

Candidate class equal to the captured caller class enters the group branch
before lifecycle or source exclusion. Actual `+58` alone must be nonnull;
`+CC` and active `+22C` are not consumed. The branch adds each captured signed
displacement and fresh signed velocity WORD `+8E/+90/+92` to wrapping DWORD
sums and increments a wrapping count. The caller itself is an ordinary list
member and can contribute; signed lifecycle zero or negative does not exclude
a same-class actor. A different-class zero candidate skips before `+58`.

For other different-class candidates, actual `+58` is followed by a direct
read of returned component `+140`, not virtual `+7C`. Its presence is captured.
Shared radius score `00427620` reads fresh self `+164` and subtracts the signed
input integer root of the captured squared distance; retain this base across
the mutable candidate gate. The captured caller task is queried against the
candidate first. A nonzero relation and captured self `+7C` enable primary:
wrap `relation * base` as a DWORD, interpret it as signed, then divide by four
with truncation toward zero. A null captured candidate `+140` applies another
arithmetic `>> 1`. Native `(-1 * 2) / 4` therefore becomes zero before the
later `| C000`; replacing division with `SAR 2` changes gate reachability.

Primary then requires fresh signed candidate lifecycle `+C4 > 1`, ORs `C000`,
uses a strict signed best-score comparison and excludes the candidate's fresh
`+84` when it equals the captured task source. Only then does real `00429CB0`
run; nonzero low AL accepts. Rejecting primary does not suppress reverse.
Reverse reloads candidate class at `004291E7`, selects its current task, and
queries that task against self. The relation reads self class fresh, while
same-class grouping still uses the original caller class. Reverse retains the
earlier base and candidate-presence capture, uses signed wrapped-product `/4`,
then applies an extra arithmetic `>> 2` for null candidate presence. It uses
strict signed improvement without a lifecycle, source or candidate-gate test.

### Canonical query and live owner custody

The composed task host executes only the proven pure canonical query bodies:

| Represented actor | `+58` | `+5C` | `+7C` |
| --- | --- | --- | --- |
| Derived base, 2/11/60/77 and ordinary common families | self `00404610` | null `00401300` | live `+140`, `004027A0` |
| Selector 12 | self | self | live `+140` |
| Shared and selectors 16/30/88/90 | null | null | null |

Selector 13 has the same static query words as 12 but no admitted constructor
in this arena. No synthetic selector-13 or standalone scalar allocation is
introduced. The [shared `+58` owner](ACTOR_RUNTIME.md#pure-actor-derived-component-query),
[pure `+7C` owner](ACTOR_RUNTIME.md#pure-actor-behavior-presence-query) and
[task relation kernels](TASK_RELATIONS.md) retain their independent policy.
Only DWORD presence of `+140` is consumed here; a nonzero semantic presence is
not a claimed physical pointer value. Each relation borrows the actual mutable
tasks and live process network flag briefly, ending those borrows before the
real gate. Subsequent queries see callback mutations. Arbitrary mutable
`+5C/+7C` services remain a standalone task-host domain, outside this pure
composition; no counter or task-source defaults are inferred.

Native `00429231` reloads the captured current node's successor after callbacks.
The same typed arena represents the captured native game/list allocation.
Canonical appends and removal of other actors affect this same walk. A live
actor whose current node is absent stops at continuation; destroying the
candidate can instead fail earlier at the fresh `+B4` read. Neither error
claims a native dangling-pointer fault or stop-IP match. Replacing the arena,
ObjectId ABA, raw list/global-owner aliases and arbitrary physical link
rewiring are outside admission. There is no snapshot list or cached-next
fallback, and no simulation guard is invented for unadmitted rebinding.

### Group point and three publishers

A signed group count above one divides each wrapped signed sum by the count,
truncating toward zero. Squared signed means sum with DWORD wrap; strictly
**unsigned** `normSquared > 20000h` enables point construction. It reads fresh
self XYZ and WORD-adds the means into all six point bytes at `00429285..29D`
before signed-input `0048EFC0` supplies the group score. A high-bit norm can
therefore require position and construct a point even though the root is zero.
Count at most one or a norm at most the threshold leaves the local point
unwritten, with score zero; no zero vector is manufactured.

Fresh process null precedes fresh self `+20C` at `004292AB..B8`. When unequal,
fresh `+214` boosts primary with the same wrapping signed-shift formula as
`+100`. The comparison uses current callback state, not the initial source
resolution outcome. Publishers run independently in order:
`00427110 -> +1F4`, `00427110 -> +1EC`, then `00427390 -> +1E4`.
First true does not skip later publishers; first error stops the sequence.
Return ORs their low-byte Booleans, including genuine null allocation.

Primary/reverse candidate locals are unwritten until selected. At score zero
their unused bits are represented only as a semantic bypass. The group native
pointer always addresses local storage; an unwritten `Point(None)` at score
zero likewise does not claim null-pointer or local-byte parity. Positive group
publication uses `WeightedTargetConstructorNull::Process` with the live process
null, independent from the arena registry's configured null. Actual allocation
null bypasses the constructor operand; an existing kind8 replacement retains
its old handle and copies only the point. The retained heap reservation cannot
rebind process, arena or target slots. The constructor reads the borrowed
process getter only after successful allocation; reached score/deletion and
released-reference prefixes survive failures in the shared publisher.

## Selector-12 actor-list selection callback

`00468350` captures the actual game/list owner, self classification `+B4`,
radius `+164`, its wrapped DWORD square and unsigned quarter before calling
self `+7C` at `00468394`. Primary's signed threshold starts at zero when that
query is nonnull, otherwise `INT_MAX`; all other best scores start at zero.
All four candidate locals are literal NULL. Empty lists still consume the
earlier class/radius/query, but consume no position, class task or clock.
There is no source `+20C`, timer `+210`, boost `+214` or velocity-group phase.

### Fresh positions and carried secondary score

Rows first require `(flags & A0) == 20`. Each admitted row reads candidate
XYZ and fresh self XYZ, subtracts as wrapping WORDs, sign-extends, then wraps
three DWORD squares and their sum. Range is strict unsigned less-than the
captured radius square. Complete typed XYZ triples do not imply parity for
independent partial native spans or mutable coordinate-query overrides.

Fresh candidate `+B4` equal to captured self class bypasses enemy/grant work.
Different class zero or signed lifecycle `+C4 < 3` enters grant evaluation.
Other candidates call actual `+58`, capture the returned component's `+140`
presence, then calculate fresh radius score `00427620`. Fresh candidate class
selects its current process task, whose actual virtual `+00` queries self.
There is no captured outer task, task-source `+08` read or reciprocal query.
A missing or out-of-range task fails only at this reached ownership boundary.

The native EBX operand is seeded once with `capturedRadiusSquared >>> 2` at
`004683DA` and survives rows. It is explicit carried state, not an independent
secondary score recomputed for each candidate:

| Reached enemy branch | Carried score before secondary comparison |
| --- | --- |
| Nonzero relation, captured component `+140` present | Retained fresh base |
| Nonzero relation, captured presence absent | Signed base `SAR 2` |
| Zero relation, captured presence absent | Zero |
| Zero relation, presence true and unsigned distance squared below `90000h` | `(trunc_signed(base / 4)) SAR 2` |
| Zero relation, presence true and distance squared at least `90000h` | Prior carried score remains unchanged |

For nonzero relation, write the base to carried state before strict signed
primary improvement and actual `00429CB0`. Only nonzero AL accepts primary.
The pre-gate base and component presence survive callback mutations; secondary
still runs when the primary gate rejects. All best comparisons are signed
and strict. Negative base `-4` yields `-1` in the near zero-relation branch,
unlike truncating `base / 16`. Neither result improves a best starting
at zero; native register evidence and public outcome discrimination are separate.

### Grants and canonical interactions

Grant evaluation requires distance squared strictly below the captured quarter,
nonzero unsigned candidate WORD `+7A`, and signed wrapped relative Y below 512.
There is no absolute-value test: negative Y qualifies. Actual `004752E0`
checks self's six-DWORD mask at `+24C`; only an unowned id reaches fresh radius
scoring. That score overwrites carried state even when it does not improve
grant best. IDs at least 192 reach the typed mask-span error, which does not
claim a native bounds check.

Captured self class 1 skips interactions. Other in-range candidates query
actual `+54`, retaining component identity. A NULL result requires freshly read
flags `(flags & 10000080h) == 10000000h`; a nonnull result bypasses this filter.
Candidate void `+9C(self, distanceSquared)` runs before that captured component's
fresh numeric `+CC(self)`. These use the
[canonical interaction owner](ACTOR_INTERACTIONS.md), including selector-35's
conditional `+244` write. A short borrow supplies the actual live animation
clock after any earlier mutable gate; no pass-wide snapshot or second clock
state exists. Unknown `+242` precedes missing-clock failure, while zero `+242`
does not require the clock's value.

For nonnull `+54`, numeric relation zero writes carried zero and skips scoring.
Nonzero relation queries fresh self `+50`. Canonical NULL plus distance squared
below `10000h` computes `radius_score(self, 0) << 2` with DWORD wrap, not a score
at the current distance. Otherwise fresh base is divided by the signed numeric
relation. Canonical selector-30 returns only zero or one; arbitrary divisor
overrides are excluded. With NULL `+54`, fresh self `+B4 == 18` after notification
gives zero, otherwise fresh distance score. Each completed interaction replaces
carried state and can affect a later far zero-relation enemy.

### Live successor and four publications

Every flags-admitted row restores EDI from the current primary threshold at
`0046861A`; rejected flags leave that unchanged threshold intact. The callback
reloads the captured current node's live successor at `00468622`, preserving
canonical appends and removal of other nodes. A missing current node is a
reached typed custody error; destroyed actors can fail earlier at a fresh field
or query. Arena/global-list replacement, ObjectId ABA and dangling raw backing
are excluded rather than silently revived.

All four actual `00427110` publishers run in this order:

| Target pair | Score operand |
| --- | --- |
| Secondary `+1EC/+1F0` | Secondary best |
| Resource `+274/+278` | `wrapping(2 * freshRadius + SAR(interactionBest, 1))` |
| Grant `+27C/+280` | `wrapping(2 * freshRadius + SAR(grantBest, 1))` |
| Primary `+1F4/+1F8` | `wrapping(2 * freshRadius + SAR(primaryThreshold, 1))` |

Radius is reread before each of the last three calls. A biased positive score
with a NULL candidate still follows the actual NULL-input clearing policy;
it does not fabricate a candidate or erase an already-empty slot's old score.
First true does not suppress later calls; first error retains the reached
score/deletion/Released prefix and stops the suffix. Return ORs the four low-byte
Booleans. Retained heap reservation and explicit target release do not rebind
arena, slots, tasks or process owners. No point/group target or constructor process-NULL read
is added to this policy. This owns neither enclosing reaction `00468220` nor
production actor `+24` or allocator-carried `+220/+230` values.

## Evidence and remaining boundaries

### Inline-table receipts

check_family_selection.py authenticates the
supported EXE, whole unchanged text and FPCW `027F`. The
[structured fixture](../../crates/inf-game/tests/fixtures/family_selection_native.json)
contains 64 histories, 68 steps and 999 native routine entries. It executes
actual scanner, table services, resolver, radius score, canonical queries and
both publishers. Reached primary gates also execute actual `00429CB0`,
`0048D820`, all three recovered particle factories, packet sources and deleting
bodies. Allocation/free and network/impact remain explicitly supplied or
stopped bodies; there is no successful stand-in for another virtual method.

The [Rust comparison](../../crates/inf-game/src/object_runtime/actor/derived/family_selection/native_receipts.rs)
covers 57 kernel histories/61 steps and two task-owner semantic prefixes/two
steps. The missing-task and class-outside-19 prefixes compare retained state
and the reached semantic boundary, not raw native faults or stop IPs. Five
source-only histories remain outside the portable domain: overcapacity raw
storage, unknown weight custody in bypassed fields/copies, and independently
known X/Z with unknown Y.
The latter is not a numerical Y consumer; the current arena admits complete
position triples. Raw table/output aliases, wrapped standalone-index addresses,
arbitrary virtual overrides and overcapacity continuations are not admitted.

Comparisons retain outputs, inline tables, reached actor/behavior fields,
19 task sources, observed resolver slots/cache, descriptor/registry/default
bytes, process null/phase/network/RNG, and semantic live/null/Released target
custody. Particle payload masks, public lists, occupants, markers and free
count are compared. Private free-stack order and unused raw registry/task
bookkeeping remain outside this composed comparison; the independent
[factory](PARTICLE_FACTORY_RUNTIME.md) and
[cache-adapter](BEHAVIOR_DAMAGE_CACHE.md#concrete-particle-adapter-and-live-process-context)
receipts retain their broader allocator/free-order proof. Freed target bytes
are not presented as live resources. Cases cover retail pruning, signed and
wrapped scoring, strict ties, lazy table reads, callback changes, repeated scans,
third-impact failure, odd interpolation, boost overflow and publisher errors.
The eight focused tests also pin required task reads before empty-table bypass,
source capture across gates and fresh source reads between scans. Existing
periodic-producer native comparisons retain their separate distance policies.

### Actor-list receipts

check_family_actor_list_selection.py
authenticates the same EXE/text and FPCW `027F`, including the literal final
callback/query words. Its
[fixture](../../crates/inf-game/tests/fixtures/family_actor_list_selection_native.json)
contains 25 histories, 31 steps and 688 native routine entries. Authentic
`00425D00` dispatches unchanged `00428F40`; reached canonical queries, all
four task relations, radius/root, candidate gate/cache, all three particle
factories, packet copies, normal deletion and all three publishers execute
unchanged. It records 78 original query entries/returns and 48 task entries.
Scalar allocation/free, active network/impact and actor-resource services are
supplied or stopped explicitly. Closed notice effects append or retire an
actor, or replace a task allocation; these effects supply ownership changes
without claiming original actor/task construction, destruction or free-body
execution. Retired physical bytes remain diagnostic backing, not live owners.

The [comparison](../../crates/inf-game/src/object_runtime/actor/derived/family_actor_list_selection/native_receipts.rs)
passes 23 admitted histories/29 steps: 20 kernel histories/26 steps and three
typed prefixes. The native step partition is 22 completed bodies, four stopped
lower services, three semantic prefixes and two excluded diagnostics. Missing
initial task source, Scratch cleanup and a later query of a retired captured
task compare the retained state and reached typed error; they do not claim raw
native fault or stop-IP parity. After the task notice replaces the registered
slot, the reverse class-18 query executes before the next primary attempt stops
on the old captured task. The two named diagnostics retain partial candidate
Y input and an unbacked live-next pointer outside portable admission.

The comparison retains live actor order/lifetime, represented actor fields,
all nineteen task slots/source/class/callback/counter fields, resolver cache,
process/descriptor backing, packet byte masks, RNG, public particle
payload/list/free-count custody and semantic target resources/Released prefixes.
Target snapshots compare backed pairs among the five initialized or supplied
family slots; untouched `+1FC` and appended actors' unbacked weighted-slot spans are
excluded rather than converted into inferred values. Native local
candidate/group-point masks, raw game/header/links and freed bytes remain
evidence, not additional typed output or physical-address parity.

Cases distinguish signed `/4` from shifting, primary/reverse presence shifts,
WORD XYZ/radius boundaries, inactive same-class grouping, negative means,
high-bit group roots, repeated ready-cache calls, real factory RNG, live notice
changes, canonical append/removal of the next actor, task replacement and
allocation failures at each publisher. The nine focused tests additionally pin
the fresh-position read before zero group root, live-current-node absence,
selector-12 query/dispatch distinction and process null independent from
registry null. Fixture SHA-256:
`CF44A89B7489F4C2610D712AAD2A37944272A82D3C378BDE30AD3EB02E08B86E`.

### Selector-12 receipts

check_selector12_target_selection.py
authenticates the same supported EXE, unchanged text and FPCW `027F`, including
selector-12/13's literal final query/dispatch words. Its
[fixture](../../crates/inf-game/tests/fixtures/selector12_target_selection_native.json)
contains 20 histories, 26 invocations and 575 recorded routine entries.
Authentic `00425CF0` dispatches unchanged `004686E0`, with reached resolver,
reverse prune, radius/root, canonical `+7C`, gate/cache, recovered particle
factories/copies/normal deletion and both weighted publishers executed
unchanged. It records 27 pure self-`+7C` entries/returns and 42 publisher
entries. Three recorded `00489600` entries are explicitly supplied impact
services, not executed impact bodies; allocation/free and active network
also remain explicit supplied or stopped boundaries. The entry total is not
a claim that all 575 lower bodies execute natively.

The [comparison](../../crates/inf-game/src/object_runtime/actor/derived/family_selection/selector12_native_receipts.rs)
passes 18 admitted histories/24 steps: 16 kernel histories/22 steps and two
typed prefixes. Kernel steps contain 19 completed bodies and three stopped
lower services. Scratch cleanup compares its reached missing-node prefix;
the fifth inline-entry case stops explicitly before the native unrepresented
row read and compares prior typed state. Neither claims native raw-pointer,
bounds-check, fault or stop-IP parity. Two named source-only diagnostics
retain an unknown primary weight in a bypassed field and unknown radius on
an empty count. The nonoptional typed radius/weight fields do not gain an
invented zero merely because another branch bypasses them.

Snapshots retain supplied known actor/table fields, resolver cache, unchanged
task backing, process/descriptor bytes, RNG, public particle payload/list/
free-count custody and semantic target resources/Released prefixes. Scratch
starts with all eight bytes unknown; only explicit notice writes make its
supplied bytes known, and first-six-byte vector plus tail-two-byte custody
survives factories and cleanup. Native transient score/candidate masks and
unused publisher arguments remain diagnostic evidence, not exposed portable
stack locals. Private free-stack order, raw aliases and unused unbacked
target spans are not inferred output values. Closed notice effects supply
mutations of the live owners without claiming a network encoder body.

Cases distinguish count-zero/negative paths, lifecycle one, leading holes,
signed WORD crossing and strict radius equality, low/middle/high weight and
wrapped-product policies, nonzero full-DWORD query presence, repeated ready
calls, live notice position/count/behavior/radius changes, fresh-null prune,
allocation null and first/second allocation failure, third-impact failure and
the two typed stops. Fixture SHA-256:
`A294D003F767BBA58F09F6CEB9638238D15336103289AD71984618B8C41E86EE`.

The [eight focused selector-12 tests](../../crates/inf-game/src/object_runtime/actor/derived/family_selection/selector12_tests.rs)
pass through the existing class-dispatched API. They pin count-zero versus
signed-negative publisher paths with unavailable position/behavior/process
inputs, lifecycle one, unused Y, signed X/Z boundaries and strict ties,
asymmetric weight thresholds and wrapped middle products, retained secondary
across a mutable gate, live count growth/position/radius/behavior reads, fresh
null pruning and both ordered publisher failure/null prefixes. Required
creation in these ordinary tests is an explicit supplied mutable lower
service; authentic factory composition is compared separately above. Existing
common inline-table and actor-list comparison tests continue passing after the shared pure-query
extraction; the actor query test independently covers its canonical null and
non-null classes. These isolated checks do not establish production or retail
cinematic acceptance.

### Selector-12 actor-list receipts

check_selector12_actor_list_selection.py
authenticates the same EXE/text, FPCW `027F`, final twenty-table query matrix,
and unchanged `00425D00` wrapper and `00468350..004686D5` callback. Its
[fixture](../../crates/inf-game/tests/fixtures/selector12_actor_list_selection_native.json)
contains 33 histories and 43 invocations: 24 kernel histories/34 steps, six
semantic prefixes and three diagnostics. The 936 recorded routine entries
contain 933 authentic body entries and three supplied `00489600` impact
service entries. Reached canonical queries, task relations, grant ownership,
radius/root, gate/cache, particle factories/copies/deletion and all four
publishers execute unchanged. This is not a claim that every recorded entry
executes its original body.

The [comparison](../../crates/inf-game/src/object_runtime/actor/derived/family_actor_list_selection/selector12_native_receipts.rs)
admits 30 histories/40 steps. Kernel outcomes contain 29 completed bodies
and five stopped supplied services; the six named semantic prefixes retain
state through missing candidate task, unknown notification byte, missing
animation clock, unrepresented grant 192, Scratch cleanup and a notice that
retires the current actor before the cache's fresh `+A4` read. They compare
typed errors and reached state, without native raw fault or stop-IP parity.
The three named diagnostics retain partial XYZ, the unrepresented static
selector-13 constructor and selector-30's unknown required byte `+18E`;
they do not acquire invented portable defaults.

Snapshots retain known actor fields and live list/registry ownership, all
nineteen task allocations and sources, explicit registry null independently
of process null, six grant-mask DWORDs, process/descriptor/cache/RNG state,
public particle payload masks/list/free-count custody, all four target
slots and Released prefixes. Selector-35's clock value and accumulated
counter use the actual portable clock owner; its in-call advance is a
supplied owner effect, not executed native `00439560`. Task replacement and
retiring the next actor likewise supply explicit ownership effects. Scratch
starts with eight unknown bytes; all eight are compared at the factory's
source return before native stack reuse. Private free-stack order, registry
free-chain bytes, hidden EBX and transient native score/candidate locals
remain outside typed output parity.

Cases distinguish the seeded and retained EBX score, canonical query classes,
WORD XYZ/square wrapping and strict radius/quarter equality, signed Y at 512,
grant ownership, task replacement, notification clock changes, interaction
distance equality, repeated scans, allocation null and failure at each of
the four ordered publishers. Initially empty slots with allocation null
return true in this callback, as the native bodies establish. Ten public
policy tests additionally cover append/current unlink and lazy required
reads. A separate private reached-phase test checks negative carried-score
arithmetic; public best-zero outcomes alone cannot distinguish that hidden
negative intermediate from zero. Existing three selection fixtures remain
byte-identical. Independently regenerated fixture SHA-256:
`957C0F975BE87ED4F4294F289233FBA6D0F75EF438B803351A39ED0BB61D2C06`.

Production actor dispatch, the unrepresented selector-13 constructor,
enclosing selector-12 reaction, other reached selection callbacks,
required lower actor/component/network/impact bodies,
and full actor update attachment remain separate boundaries. These receipts
are isolated routine evidence, not complete intro or briefing acceptance.
The [shared capture/release flag/list body](ACTOR_CAPTURE.md) is an independent
prerequisite of the enclosing reaction; it does not replace its class prefix.

```text
python check_family_selection.py --output .tmp/family-selection.json
python check_family_actor_list_selection.py --output .tmp/family-actor-list-selection.json
python check_selector12_target_selection.py --output .tmp/selector12-target-selection.json
python check_selector12_actor_list_selection.py --output .tmp/selector12-actor-list-selection.json
cargo test --release -p inf-game --lib family_selection
cargo test --release -p inf-game --lib family_actor_list_selection
cargo test --release -p inf-game --lib target_producer_tests
```

Run Cargo commands from ``; the native checker requires the
same Python/Unicorn environment as the existing factory checkers. No retail
capture or debugger replay is part of this evidence.
