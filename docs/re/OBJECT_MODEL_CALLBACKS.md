# Infestation captured object model callbacks

This document owns packet packed-value callback `004789B0` and the admitted
live-object virtual `+14` getters, plus packet custom vertex `00478950` and the
bounded family virtual `+10` selectors2/10, selector30 exact8, selector36 exact11,
selector60 cached9/exact7, selector10's [legs](FAMILY4236D0_RUNTIME.md#legs-custom-vertex-00425870)
and selector77 LOWBYTE2/10 owners below. The
[`ObjectModelCallbacks`](../../crates/inf-game/src/object_runtime/model_callback.rs)
owner borrows the actual arena, captured packet source, process animation clock
and executable. Its consumers are the shared gameplay and briefing
[object draw manager](OBJECT_DRAW_RUNTIME.md), with explicit common and late
model roots. Packed getters remain independent from traversal and presentation.
Intrinsic packed decoding and execution remain in
[model runtime](MODEL_RUNTIME.md); object lifetime remains in
[object runtime](OBJECT_RUNTIME.md).

## Packet identity and clock custody

Complete `004789B0..004789D2` tests the full DWORD selector first. Zero reads
only LOWWORD `[005471C0]`, before packet `+3C`, source object or vtable access.
Nonzero captures packet `+3C`, forwards the same model invocation context and
selector to that source's virtual `+14`, then consumes only AX. All admitted
`+14` bodies ignore the forwarded context argument and do not write invocation
registers, object fields, registry, RNG or lists.

The port therefore keeps selector as `u32` and result as raw `u16`. Native
`004ED4B0` already supplies `raw & FF3F`; the callback does not mask again or
truncate a direct caller's DWORD before dispatch. The ordinary packed domain
cannot be negative. A returned LOWWORD is not implicitly an angle, fraction,
decimal value or signed position; its model consumer owns the interpretation.

`ObjectModelCallbacks { arena, source, animation, executable }` borrows actual
current owners. `animation` is a shared `frontend_camera::AnimationClockSource` loan whose
`animation_value()` getter returns the full current DWORD. Production borrows
`FrontendProcessState`, reading its retained `Rc<Cell<AnimationClock>>` at each
reached selector-zero callback; a draw-entry copy is not that owner.
`AnimationClock` implements the same getter for explicitly supplied test poses.
Selector zero calls this getter before reading the source. Other selectors
resolve current typed fields lazily from that captured `ObjectId` without a
clock read. Immutable arena/executable loans admit the nonmutating getters;
the process-cell clock read remains fresh. This does not prove arbitrary native
alias writers or vtable replacement. A successful clock-only call with an
unavailable source does not admit unavailable model geometry.

Native `0047AFC0` stores its receiver into packet `+3C` at `0047AFF1`.
`0047D910` and wrappers `00427E60`/`004690E0` retain that receiver. The late
`00479570` path instead first calls original source `+18`, then stores the
returned component at packet `+3C` (`0047975D`). That component supplies model
WORD `+42`, XYZ, basis `+48` and behavior `+7C`; only the original source
clears `01000000`. These packet identities remain distinct even where currently
admitted actors return Self. Canonical nonactor `+18` callbacks return NULL.

Root `0049FEF0` stores the supplied packet at invocation `+44`. External child
`004A0ED0` copies nine DWORDs from parent `+3C`, retaining that packet binding.
A child selects another H8 resource/model without changing the captured source.
Packet vtable `00520A44/+0` is `004789B0`; its separate `+4=00478950` forwards
captured source virtual `+10` for custom vertices and remains a distinct owner.
The scoped Rust callback loan follows the same source through child execution.

Actor collision `0047CEB0` uses stack wrapper `00527B18/+0=00478A40`,
capturing the original actor at wrapper `+4`. Its complete `00478A40..00478A61`
body has the same clock-zero and source virtual `+14` policy as `004789B0`;
only the captured-source offset differs (`+4` versus packet `+3C`). The
[projectile geometry adapter](../../crates/inf-game/src/world_projectile_collision.rs)
therefore loans the same getter owner, with the actor, current clock and EXE.
Auxiliary child traversal retains this source while selecting each actual H8
child. Collision traversal and renderer traversal remain separate owners.

## Actual-class getter matrix

Dispatch follows `ObjectClass`, `ActorExtension` and nested family state.
Constructor/resource IDs, mutable resource words, actor `+B4` and placement
records do not select the callback. The native final-table census covers 94
classes; construction-unadmitted classes remain outside this typed domain.
The outer clock-zero rule precedes every row below.

| Actual typed owner | Native virtual `+14` | Nonzero selector policy |
| --- | --- | --- |
| All admitted nonactors; Shared/16/88/90/93 actor extensions | `0047A020` | Return LOWWORD zero without payload reads. |
| Derived Base; Family428650 Base/34/35; Family429e20 Base; Family4236d0 Base/10 | `004260A0` | Selector 9 reads WORD `+178`; every other value returns the selector's LOWWORD itself. |
| Derived selector 11 | `004015A0` | Selector 2 reads unsigned health `+A0`, then required maximum `+A2`, evaluates wrapped `(A2-A0)<<16` with signed IDIV by `A2+1`. Selector 3 reads LOWWORD `+188`; all other nonzero selectors read LOWWORD `+18C`. Maximum health is not read for those other branches. |
| Derived selector 2 | `00415BC0` | Twelve-way current-field dispatch below, without eager model-mode or behavior lookup. |
| Family429e20 selector 12 (canonical selectors 12/13) | `00468920` | Read signed velocity WORDs `+90/+92/+8E` and compute wrapped sum/sqrt before selector dispatch. Selector 1 returns speed LOWWORD. Selector 2 uses signed speed `>100` → `C000`, `>5` → `8000`, otherwise byte `+29C` nonzero → `4000`. Selector 9 reads byte `+29D`, then current arena loader flag `0054ECE4` only if that byte is nonzero. Other selectors return zero after the velocity prefix. |
| Family4236d0 selector 36 | `00481E50` | Signed selector `<=6` reads WORD `+266 + 2*selector`. Positive 1/2 alias the low/high WORDs of DWORD `+268`, 3/4 of `+26C`, 5/6 of `+270`; `>6` delegates to `004260A0`. Zero is intercepted by the outer clock getter. |
| Derived selector 60 | `00480E70` | Selector 6 reads WORD `+1A0`; 7 reads WORD `+19E`; others return zero. |
| Derived selector 77 | `004074A0` | Unsigned selector `<7` sign-extends the selected WORD from `+184..+190`, negates the full DWORD for selectors 1/5, then returns LOWWORD. Values `>=7` return zero. |
| Actor selector 30 | `0048B330` | WORD `+164` nonzero returns zero before selector dispatch. Otherwise selector 1 requires DWORD `+168==1` and computes wrapped `(1200-zeroextendedWORD17C)*65535`, with signed truncating division by 1200. Selector 2 returns 1; others return zero. |

Speed uses the shared exact `isqrt_48efc0`, not floating length or widened
normalization. A negative wrapped velocity-square sum takes unsigned sum `>>2`
before signed sqrt, then shifts the result left one. The ordinary sum goes
directly to signed sqrt. For vehicle DWORD `+244`, negative magnitude uses
wrapped NEG before sqrt: signed MIN remains nonpositive and produces zero.
No floating `abs`, saturating arithmetic or i64 replacement is implied.

Selector 36's direct negative DWORD enters a native signed `<=6` branch whose
preceding storage is outside the represented object fields. It returns
`Selector36Backing { object, selector }`, rather than casting the index or
reading a guessed zero. This does not reject any ordinary `FF3F`-masked packed
selector. Camera selector-16 smoothing `0040F1C0` is unrelated to virtual `+14`.

## Selector-2 branches and required backing

Native jump table `00415CF8` has twelve authored branch DWORDs. The complete
getter body is `00415BC0..00415CF7`; out-of-range selectors return zero before
field reads. The outer selector-zero rule still precedes this body.

| Selector | Reached policy |
| --- | --- |
| 1 | Current DWORD `+1E8` indexes a WORD at `00530578 + 2*mode` with native DWORD wrapping. Read actual `FrontendExecutable::mapped(address, 2)` backing; there is no inferred six-entry capacity. The first twelve observed bytes are finite source evidence, not table-length authority. |
| 2 | LOWWORD of DWORD `+1EC`. |
| 3 | LOWWORD of DWORD `+1FC`. Constructor `004156E0` leaves this field unwritten and the current typed state omits it. Return `RequiredField { object, offset }` when reached; zero is not established. |
| 4 | Modes 1/2 use signed DWORD `+244` sqrt magnitude times `47/28`, negating the result when `+244` is nonpositive. Other modes use signed DWORD `+220 / 10` toward zero. |
| 5, 9 | Return zero without other field reads. |
| 6, 7 | WORD `+20E` / LOWWORD of DWORD `+210`. |
| 8 | DWORD `+238`: exactly 1 → `1000`, exactly 0 → `8001`, every other bit pattern → zero. |
| 10 | DWORD `+224` zero → zero. Otherwise byte `+20C` zero → `FFFF`, nonzero → zero. |
| 11, 12 | Both read current behavior `+140`, signed byte `+10`, entries `+18` pointer, then selected 32-byte row LOWWORD `+4/+8` respectively. Byte `+11` is not the row selector. |

Behavior reads remain lazy. `RequiredVehicleBehavior` preserves the missing
owner boundary; `VehicleBehaviorRow { object, row, rows }` distinguishes a
negative signed row or unavailable row backing. A constructor-empty chain does
not supply model callback rows. An unmapped authored mode address remains an
`Executable` error; captured words and an invented array capacity do not replace
the strict executable map. Generic object lookup failures remain `Object` errors.
These are required-source boundaries, not successful zero values.

## Consumed object-model request

[`ObjectModelPacket`](../../crates/inf-game/src/object_runtime/object_draw.rs)
contains captured `source`, `model_slot`, signed XYZ and basis. The shared
manager produces it at the actual common or late reach, after required flags,
terrain and class callbacks. `FFFF` common models run the real Flag0 setter;
there is no production placement-vector redraw or geometry-only NoModel skip.

`SoftwareObjectModelDrawRequest` adds explicit `root_mode`, current caller
context, mutable arena, animation and executable. `SoftwareScene::draw_object_model`
supplies actual scoped callbacks, inherited renderer surface table and process
RNG to the selected genuine entry. Common `0049FEA0` and late `004ED610` retain
different resource, initialization, child, dispatch and custom policies; late
is not common Far projection. Both callers lend the actual process clock,
reading its current DWORD only at reached selector-zero callbacks. One
`ObjectModelServiceLoan` lends the actual arena through the root and its
children. Packed getters create a short shared `ObjectModelCallbacks` view on
each reach; custom vertices mutate the same retained actor storage. No actor,
behavior, clock value or model callback result is cloned into a draw cache.

## Family custom vertex selector 2

[`compute_model_custom_vertex`](../../crates/inf-game/src/object_runtime/model_custom_vertex.rs)
owns LOWBYTE selector 2 of complete source callback `004260C0..0042626A`.
The selector is the incoming sign-extended WORD DWORD, then masked to BYTE;
extras stay unsigned WORDs. [Selector10](#family-custom-vertex-selector-10)
shares this actual class policy. Other selector arms and other source callbacks
return a typed `Unowned { object, selector }` at their reach. Models, resource
slots, placements, actor `+B4` and level selection never choose the callback.

The 94-class final-table census has this callback on constructors
7/8/9/11/12/13/33/34/35. Represented sources are Derived Base, selector11,
Family428650 Base/34/35, Family429e20 Base/12 and Family4236d0 Base. Intermediate
tables `00520F34` and `00521164` also contain `004260C0` at `+10`.
The actual selector11 table is `0051F080`; constructor13 has no exact final
typed variant or generic factory admission. Sharing the source callback does
not establish constructor13 parity. Selector2/60/77 and Family4236d0
selector10 use other callbacks, owned in their own sections. Selector36 retains
its actual `00527EE0/+10=004823F0` forwarding policy below.

`0042621D/25` fetches extra0 through complete `00439E10..00439E35`: unsigned
index times 24, LOWWORDs at slot byte offsets `0/4/8`. The shared
`row_words_439e10(u16)` also serves frontend and furniture consumers. Unknown
whole XYZ or capacity errors are portable backing containment; native439E10
does not perform those admission checks. No `u16 as i16` index conversion is
valid. Only after this fetch does `0042622A` read optional actor `+140`.
Actual NULL skips extra1 entirely. NonNULL reads unsigned extra1 and invokes
complete `0048CF70..0048D000`; the captured slot temporary remains unchanged.

`00425E60` establishes NULL `+140`. Payload `00426C10` leaves it unchanged;
guard `00426C80` replaces it only for low-seven-bit modes1..13, preserving the
previous owner on a guard miss. Mode6 uses real table `00531048`, two entries
and one or two groups according to the mode high bit. Constructor `0048CC70`
writes only group DWORD `+0C=4`; coordinate bytes `0..0B` remain unknown.
The single group owner therefore retains six independent optional coordinate
WORDs. `dwords_00_08()` reports a DWORD only when both halves are known;
historical native JSON snapshots keep their original schema and bytes.

Within the reached unsigned-WORD index domain, `0048CF70` compares the index
as signed DWORD against signed BYTE `+0D`. `index>=count` returns normally
before group backing, actor XYZ or basis reads. Otherwise it writes world X/Y
with DWORD `+6`, then world Z with WORD `+A`. If flags bit4 is set, it clears
only that bit before reading actor X32/store-relativeX0, Y34/storeY2,
Z36/storeZ4. Subtraction wraps in WORDs. `0047A660` supplies actual actor basis
`+44` to `00490A80`, which captures all signed input WORDs before ordered
inverse X/Y/Z writes, using the existing nearest-even QWORD/LOWWORD policy.
Final flags `+0C=3` follow normal completion. With bit4 clear, relative words
and actor XYZ/basis remain unread; world capture and final flags still occur.

Current actor position custody is `Option<[u16;3]>`: complete XYZ or an
unavailable first X. That missing-X boundary retains world capture and cleared
bit4, leaves relative words untouched, omits final flags3 and publishes no new
callback output. Individually unknown Y/Z, arbitrary aliases and direct negative
DWORD helper indices are outside this admitted domain; no partial-position
fixture or zero fill establishes them. Missing extra/group/field errors retain
the same reached storage and callback temporary. A group index admitted by the
owned count with absent allocation backing is a hard custody error; required
extra/field/unknown-world and unowned callback arms retain feature containment.

After successful source return, `00478950` reads the enclosing invocation
`+58` caller origin. Common `004A0024` and late `004ED6E6` retain that pointer
through child context copies. For X, then Y, then Z:
`sign_extend_WORD(output_low - eye_low) + full_DWORD_eye`, all additions
wrapping. `ObjectModelServiceLoan` reads actual `context.view.eye` after the
source phase; model/child position cannot replace it. Later store/projection
failure retains completed actor state and model command/RNG prefixes.

The [custom tests](../../crates/inf-game/src/object_runtime/model_custom_vertex/tests.rs)
guard all94 final and both intermediate table entries, complete callback/math
bodies and constructor/lifetime bodies. They discriminate lazy extras,
signed counts, unsigned indices, wrapped relative math, nearest-even inverse,
repeated capture and reached unknown storage. These source and port checks
do not establish original-game execution or retail visual acceptance.
The real service tests use unchanged original model552 children553/554 through
both common and late roots, reaching both selector2 instructions with the same
actual source and enclosing caller loan. Their declared pose stays within the
signed-WORD relative window while distinguishing full-DWORD eye from model
origin. A separate reached projection failure proves retained group mutations;
none of these checks claims complete model or original-frame acceptance.

## Family custom vertex selector 10

The same source `004260C0` admits LOWBYTE10 and uses unsigned extra0 through
`00439E10` first (`0042613F/47`). Only afterward does `00426159` read actual
actor WORD `+34`; `00426160` stores `slotY-actorY`, wrapping in a WORD, to the
existing `ActorState.word_74`. Output X/Y/Z follows at `0042616A/6C/74`.
No extra1, behavior `+140`, terrain, basis, dirty flag, registry or RNG belongs
to this arm. Height is neither clamped nor substituted with extent. Subsequent
camera follow consumers read this same actor field; no draw-state clone owns it.

LOWBYTE aliases `010A` and sign-extended `FF0A` select this arm. The whole-XYZ
`Option` domain requires current Y at offset `34` after the row fetch; it does
not invent individually known X/Z. Missing row/extra/Y backing preserves old
height and caller output. Normal source return then uses the same `00478950`
full-DWORD caller-eye rebasing above. A later projection failure retains the
completed height store. Complete-output `Option` custody does not establish
native output-pointer faults between the axis writes.

Original parent1 H13slot3 binds constructor12/table `00527058` to model223
(`scientst`), whose `222D[54,10,44,0]` is at `1.ovl` bytes `190148..190158`.
Initial intro leaf32 and Level1 leaf28 placements refer to this same authored
resource; their earlier register/vertex prefix and visibility gates still own
actual reach. These joins locate consumers without choosing runtime policy by
model, placement or level.

[Real service tests](../../crates/inf-game/src/software_scene/object_models/tests.rs)
construct original H13slot3 through the actual constructor12 with original H12
and EXE grants. Declared instruction budgets13/14 bracket the first selector10
in both CommonVisual and ManagerLate roots: four actual clock reads precede it,
and only the reached callback writes actual actor `+74`. Its captured row and
full caller-eye output are observed through the real service loan. A separate
original `222D` common-root test injects a projector failure after the real
callback and retains the height write without publishing destination54. That
isolated packet does not establish whole-prefix reach. Neither test proves
complete model223 execution, an original-game fault or matched retail pixels.

Shared actor `0047A240` compares the full DWORD to10 and publishes Y/X/Z;
its other selectors enter a separate terrain arm
([shared actor](#shared-actor-0047a240)). It is not this family's dispatcher.

Selector36's complete `004823F0..0048245C` (108 bytes) compares the full DWORD
with11 at `004823F5` before the family mask. Every other selector forwards the
same source, caller context and extras to `004260C0` at `00482453`, with no
actor-field read/store before that call. `ObjectCustomCallbackOrigin` retains
this actual wrapper identity separately from direct `004260C0` tables. Its
forwarded2/10 arms share the recovered kernel; exact11 uses the separate
[capture owner](#selector36-exact11-capture) below. `010B`/`FF0B` forward
unchanged but then reach the still-unowned family LOWBYTE11 default terrain arm.
Neither alias selects the exact11 capture. Authored intro constructor36/model492
retains this wrapper identity without becoming a direct-table class or borrowing
shared actor policy.

The same model223 in briefing preview
uses frontend `0043A2D0`, whose default arm returns stack residue for
selector10 ([CustomVertexStackResidueApproximation](MODEL_RUNTIME.md#customvertexstackresidueapproximation));
actor ownership does not complete that preview. Other family selector arms, arbitrary
aliases and complete matched retail model/scene acceptance remain follow-ups.

## Selector36 exact11 capture

Exact full-DWORD11 of `004823F0` sets actual BYTE `+265` bit8 at `00482400`
before extra0 or row backing. `Family4236d0State.word_264 |= 0800` preserves
BYTE264 and the other high bits. Existing `Selector36State.words_278_27c`
retains the three independent optional WORDs; constructor `00481110` leaves
all three unwritten. No new captured-position cache, dirty setter, actor XYZ,
height, behavior, terrain or extra1 belongs to this callback.

Unsigned extra0 at `0048240D` selects `00439E10`; `00482411..41A` supplies
actual actor `+278` as its destination. Normal completion stores LOWWORD X/Y/Z
at `+278/+27A/+27C`, then `0048241F..43A` sign-extends those captured WORDs
into output DWORD X/Y/Z. Existing packet `00478950` rebases only after success
through the enclosing caller eye, independently of the model/child origin.
The callback captures a fresh row even when the valid bit was already set.

Portable missing-extra, invalid-slot or unknown-world containment preserves
the reached `0800`, old captured WORDs and previous output. The shared row
owner requires a complete known world triple before its stores; native per-axis
source-pointer faults, buffer aliases and output-pointer faults are outside this
loan. No fixture establishes those native partial-write fault domains.

Original parent1 H13global108 tag35 selects constructor36/model492. Intro
leaf32 placements51/78/79 bind that same resource. Original492's forwarded
height record precedes twelve primitives, children493/494 and orientation6
child497; these earlier geometry, resource and visibility phases can stop reach.
Original497 starts with `2F2D[46,11,12,0]` at `32.ovl` bytes `445084..445094`.
Its real world ranges populate unsigned row12 from raw vertex6. These static
joins never choose callback policy by placement, model or level.

[Kernel tests](../../crates/inf-game/src/object_runtime/model_custom_vertex/tests.rs)
cover actual constructor storage, flag preservation, repeated capture,
unsigned rows, error prefixes and full11 versus forwarded aliases/direct classes.
[Actual service tests](../../crates/inf-game/src/software_scene/object_models/tests.rs)
use the genuine selector36 allocator and original497 with a declared one-record
budget. Common performs two mode3 captures and Late one forced mode1 capture
through the same source/caller loan. A separate common projector-negative case
retains captured actor fields and destination46 world after real callback/rebase;
destination47 remains unreached. These tests do not claim full H13 initialization,
parent492 traversal, complete model execution or matched retail pixels.

Downstream simulation `004824A0` consumes the capture under its own near/valid
gates and clears `0800`/sets `1000` only after successful projectile work.
That simulation, its payload/velocity effects and later model/height writes
remain separate owners; capture success does not admit them.

## Selector60 cached9 capture

Actual selector60 table `00527DE8/+10=00480CF0` compares the full DWORD with7
and9. Only exact9 is owned here. It first reads current BYTE `+184`; any
nonzero value skips extras and the invocation row. When zero, `00480D69`
stores1 before unsigned extra0 and `00439E10` writes the captured LOWWORDs
at `+198/+19A/+19C`. `00480D83..00480DA3` publishes signed retained X/Y/Z,
then packet `00478950` rebases through the actual enclosing caller eye.

The real constructor `00480800` initializes184=0 and leaves those coordinate
WORDs unwritten. Existing `Selector60State.position_198` owns the cached triple;
there is no second render cache. First-row backing errors retain the earlier
184=1 and prior cache/output. Nonzero184 with unknown cache stops at required
X198 before extras, without retrying or inventing a position. Nonzero admission
publishes the same retained first row on later calls, even with no extras.

This differs from selector36 exact11's fresh278 capture. It also differs from
family4260C0 LOWBYTE9's default terrain phase. Class60 aliases0109/FF09 reach
its native default terrain arm, which remains an explicit boundary; exact 7
is the [trail](#selector60-exact7-trail) below. Exact9 uses no actorXYZ, behavior, terrain, extra1,
height write, dirty setter or RNG. Its spawn-list allocation and later simulation
remain owned separately by [selector60 runtime](SELECTOR60_RUNTIME.md).

Whole optional cache/current-row custody bounds the portable implementation;
later-axis pointer faults, output aliases and native partial row writes are not
claimed. Authored H13global45 joins constructor60/model390, with original
222D[108,9,40,1] after forty earlier instructions. That static join chooses no
runtime callback policy and proves no complete model or intro execution.

[Kernel tests](../../crates/inf-game/src/object_runtime/model_custom_vertex/tests.rs)
cover real constructor0/None, signed LOWWORD capture, cache hits before extras
or current-row backing, interrupted first-row latch and retained prior cache,
nonzero184 admission, full selector aliases and actual source-class exclusion.
The versioned [loaded-manager test](../../crates/inf-game/src/world_scene_tests/custom_vertex.rs)
`world_scene::tests::custom_vertex::loaded_flag0_custom_capture_source_joins` uses the
actual loaded Flag0 manager and unchanged software backend. It identifies the
same typed class60 source/model390/ManagerLate and observes real0/None→1/Some
cache mutation independently of an arena slot or placement index.
The observed model packet and complete Flag0 call return successfully in this
supplied phase fixture; the earlier same-source Unowned9 result is absent.

That phase fixture supplies eye0, near1,000,000 and far2,000,000; it establishes
loaded callback reach and mutation, not the production camera or a complete
world frame. Later custom7, primitive and queue boundaries remain separate.
Neither these port checks nor the existing constructor/row native receipts
establish all model390 branches, complete production intro or matched retail
acceptance.

## Selector60 exact7 trail

`00480CF0` takes exact full DWORD 7 at `00480CFF`, before the cached9 test.
It reads no object field. Unsigned extra0 selects the `00439E10` row, and its
signed WORDs go to the output first. Then the load factor gates a trail:
`(0xC000 - [0055F154]) / 2048`, with a wrapping subtraction and division
toward zero. A divisor of at least 2 draws `0048F060`, and a nonzero
remainder of its low WORD skips the trail. Extra1 then picks the
`00489D90` type: 0 gives 5 and 1 gives 6. Any other value passes the
output pointer as the type, which `00489D90` rejects before using the RNG.
One more draw sets the velocity `(0, 128 + (low WORD >> 9), 0)`, flags
`0x800`, at the row's WORD position. Even a rejected type draws it.

The receipt runs the callback, its row
fetch and its RNG unchanged over 46 cases. These include every type WORD,
twelve load factors (zero, negative and wrapping divisors) and the exact-9
latch. The port matches every output, draw, trail, latch and cache. World
object draws lend the particle owners, the model draw's RNG and the world's
load factor. Briefing draws lend no load factor, so this arm stops there at
reach.

## Selector30 exact8 capture

Actual `ActorExtension::Selector30` selects table `005289C0/+10=0048B290`.
`0048B295` compares the complete DWORD with8; exact8 takes
`0048B2EA..0048B327`. It reads unsigned extra0, passes the actual
`+16C/+16E/+170` row to `00439E10` at `0048B300`, then publishes its signed
WORDs at `0048B305/0048B30F/0048B319`. Packet `00478950` rebases only after
normal source return, through the enclosing caller eye. This is its own
callback override, not the shared47A240 or family4260C0 selector policy.

Existing `Selector30State.position_16c` is the sole retained backing. The
constructor leaves it unknown, and every successful exact8 reach replaces it;
there is no first-row latch. Missing extra, invalid unsigned row and unknown
current XYZ retain the prior capture and output. This typed loan admits complete
known XYZ rows: native output/source aliases, invalid pointers and per-axis
partial writes are not established. It reads neither actorXYZ, terrain,
extra1 nor behavior, and writes no actor height74, dirty flag, registry or RNG.
Its enclosing original model can still consume RNG before or after the callback.

Every other selector, including `0108`/`FF08`/`-248`, takes `0048B29D..0048B2E7`:
the actor's X, the ground `00450A30(x, z, 0)` at its X/Z, then its Z, the
same body as `0047A1E0` (the port's `ground_model_vertex`), with no extras
read and the `+16C` row untouched. Worlds 23 and 24 reach it with selector 7
from their first frame. Constructor, payload, commands and later
simulation remain owned by [selector30 runtime](SELECTOR30_RUNTIME.md).

Original H8model62 calls child63 with orientation6/state24. Child63 advances
two zero-mode0022 jitter records without consuming RNG, then reaches original
222D[118,8,44,0] before its first triangle:
unsigned row44 is the source; the trailing extra0 is not another row input.
Common mode2 computes once and projects; the late root's forced mode1 stores
world XYZ without that projection. These root policies remain shared with other
captured-object callbacks, independently of the class-specific source+10 arm.

[Kernel tests](../../crates/inf-game/src/object_runtime/model_custom_vertex/tests.rs)
guard the full151-byte native body and cover genuine constructor unknowns,
fresh recapture, signed LOWWORD output, unsigned high/unknown-row faults,
full-selector aliases, source-class exclusion and complete arena/row custody.
[Original-model service tests](../../crates/inf-game/src/software_scene/object_models/tests/capture30.rs)
run the real62-to63 prefix with budgets immediately before/after its custom
record in both roots. They preserve the same source, caller-eye rebase, complete
arena custody, queue and zero-mode RNG bypass. A separate direct original222D
case retains capture and rebased output after an injected later projection
error; those portable budget/error boundaries do not assert native fault parity.
The [loaded-manager test](../../crates/inf-game/src/world_scene_tests/custom_vertex.rs)
`loaded_flag0_custom_capture_source_joins` runs the actual loaded Intro32 arena
through the unchanged Flag0 manager and software backend. It selects the genuine
selector30 None-to-Some capture before checking model62/CommonVisual; the same
packet and whole manager return successfully under its supplied view. No model,
placement index or arena slot chooses the production callback.

That witness supplies eye0, near1,000,000 and far2,000,000. It establishes this
loaded intro callback and original62-to63 model path, not a complete production
camera/frame, every model branch, full intro animation or matched retail pixels.
A separate Level1 broad-view diagnostic did not reach its SDL custom8 omission;
that source identity is not established by the Intro witness.

## Selector77 eye capture

Actual `DerivedActorClass::Selector77` selects table `0051FBB0/+10=004074E0`.
The complete 273-byte callback dispatches on LOWBYTE, independently of model,
placement, mission and arena slot. LOWBYTE2 and LOWBYTE10 now have explicit
owners; other values retain the required default terrain-service boundary.

LOWBYTE2 reads unsigned extra0 and the current `00439E10` world row, then
writes the actual actor WORDs `+192/+194/+196`. Only after this capture does it
read current optional `+140`; non-NULL behavior requires extra1 and uses the
existing complete `0048CF70` group owner. Signed group-count bypass, world
stores, bit4 clear, relative coordinates and actor-basis reads retain their
existing order. Missing extra1 or later group/backing failure preserves the
new actor capture without publishing callback output or writing height74.

On normal group return, the callback rereads the retained capture, publishes
signed WORD XYZ, then reads actor Y34 and stores wrapped `capturedY-actorY`
to WORD74. Output survives a required Y read failure. LOWBYTE10 instead reads
the unsigned row into a local and publishes signed XYZ without reading actor
position/behavior or writing either retained192 or height74. The enclosing
`00478950` caller-eye rebasing remains shared and runs only on normal return.

Constructor `00407410` does not write192; post-construction `00408090` copies
actual actor XYZ there. A directly allocated constructor fixture therefore
starts unknown, while the loaded world has a known post-initialization row.
Drawing supplies none of the constructor-unwritten19C/19E/1A0/1AC state.
The separate [allocation policy](../../crates/inf-game/src/fresh_heap.rs)
provides19C/1A0 only in the selected production approximation; strict
constructor fixtures retain unknown storage.

Original Intro32 H8 root698 descends through699/700 to701. Its first custom
record is `222D[82,2,6,0]` at original32 offset392988. The current source
class supplies the capture; this authored record does not choose class77.
The next inline05 at393014 uses the separate shared
[Common model transform](MODEL_RUNTIME.md#common-inline-transform).

The original hierarchy has one unconditional child at each698→699→700→701
step. Lower ribs, three middle ribs, the top scaffold and nine trailing eyeball
ribs belong to this chain. Model702 is separate, including assignments to the
intro's two selector90 camera helpers; it is not another leg segment to append.

Model699's three raw-local control pairs span Y0..Y768; opcode26 interpolates
their transformed world rows using packed0x86, whose actual selector77 getter6
reads WORD190. Constructor/postinit leave that word
zero, collapsing the middle span at first load. Native vtable+D4 selects
`004080C0`, which owns subsequent WORD190 motion; `004081B0` steers other pose
words. The [NULL-target update](ACTOR_BEHAVIOR_RUNTIME.md#selector77-null-target-update)
owns this extension phase within its required input/service bounds.
The eye was compared directly against retail on 2026-10-05 and its appearance
was approved; no executable/build, frame or pose was recorded for that check.
A complete production pose timeline remains a separate acceptance condition.

[Callback tests](../../crates/inf-game/src/object_runtime/model_custom_vertex/selector77_tests.rs)
guard the complete original callback/constructor/helper bodies and vtable,
LOWBYTE aliases, unsigned rows, fresh recapture, optional group error prefixes,
output-before-Y failure and genuine behavior completion. Checked complete-known
row/actor XYZ custody remains the port domain; arbitrary native pointer aliases
and per-axis partial faults are not established.

The [loaded-eye test](../../crates/inf-game/src/world_scene_tests/eye.rs) selects
actual class77 from the loader report before independently checking H8 root698.
It retains actual XYZ/basis, behavior140, selected allocation carries and
remaining unknown fields while
executing unchanged698→699→700→701 through Flag0 and the real software backend.
Two declared poses supply Near/Far views at tiers2/4: constructor190=0 and a
test-only190=0x4000 quarter span. All eight packets complete without omissions,
mutate actual192/74/group backing, flush primitives and produce pixels without
consuming RNG. In this retained actor basis, the quarter-span fixture changes
pixels and raises captured world Y, relative/world group Y and height by
exactly192. It isolates the original middle
segment's length control; it does not supply a runtime default or native update.
The synthetic views remain distinct from the production camera timeline,
animated eye, full intro and matched retail pixels. The NULL-target update has
a separate owner; nonnull target services and full simulation remain open.

## Shared actor 0047A240

Forty-four actor classes keep the shared table's `+10`, `0047A240` (166
bytes): selectors 3..6, 16..18, 20, 23..29, 31, 32, 37..41, 44, 49, 52, 53,
55..57, 59, 62, 66, 68, 69, 73, 74, 78, 82, 84, 85, 88, 90, 91 and 93, by the
94-row table census in
`all_original_tables_and_complete_callback_bodies_guard_the_shared_policy`.
`0047A247` compares the full DWORD with 10. Exact 10 is `004260C0`'s height
capture: unsigned extra 0 through `00439E10`, actor `+74` = the row's Y minus
actor Y (`+34`), then the row's signed Y, X and Z. Every other selector,
`010A` and `FF0A` among them, publishes actor X (`+32`), the ground
`00450A30(x, z, 0)` under actor X/Z (world resource `0055EE38`) and actor Z
(`+36`): the base object's `0047A1E0` body, which the factory's `0042E3D0`
repeats for its other selectors.

The port gives `ActorExtension::Shared` and every class extension without its
own `+10` this policy (`ObjectCustomCallbackOrigin::Shared47a240`); the
factory (15), selector 30, the Siren (47) and the derived classes keep
theirs. The ground arm takes the callback's leg services, as the factory's
does. Terra Nova's generator (selector 39, H8 46) asks for exact 10, and was
omitted as `Unowned { selector: 10 }` before
([model runtime](MODEL_RUNTIME.md#thumpermodel46stackhistoryapproximation)).
Covered by `shared_actor_full10_captures_and_other_selectors_reach_the_ground`.
Non-actor tables (`0047A1E0`, and `0047A000`'s zero output) stay unowned.

## Callback containment and source identity

Callback, model and resource boundaries preserve completed command, context
and RNG prefixes. The named enclosing per-object omission policy never claims
normal late return merely to clear source pending or reach component behavior.
Queue, publication and custody faults remain hard. Gameplay commits the
manager's reached RNG before any return; briefing commits its enclosing draw
prefix on success or error. Unknown fields and resources never become callback
zero. Frontend, loading, name-entry, briefing-panel and furniture callback
policies retain their own source identities and semantics.

Sky H8 `0x116` (278) has no authenticated actor packet/source. It retains
`WorldSkyStaticValuesApproximation`, returning zero for all selectors. It does
not invent an ObjectId or claim the actor clock rule. Viewer neutral values
remain a separate diagnostic policy.

## Remaining draw ownership and acceptance

[Shared object drawing](OBJECT_DRAW_RUNTIME.md) now owns traversal, terrain/cone
admission, component selection, packet lighting, FFFF membership writes, flash
and the distinct late root. Other packet/source custom branches remain
required at their actual reach: common mode0 bypasses it,
common mode2 and late forced2D invoke it before subsequent projection/stride
work. The live Object policy never borrows the static sky approximation.
[Offline selector-2 drawing](OBJECT_DRAW_RUNTIME.md#ordinary-vehicle-and-retained-process-profiling)
now uses this same source loan and admits retained nonempty firing chains.
Network drawing and the remaining [weapon factory and shot lifecycle limits](WEAPON_FIRING_RUNTIME.md)
stay explicit boundaries.
Simulation writers, allocator-carried fields, construction-unadmitted classes
and native aliases are not established merely by current-field reads.

The [getter tests](../../crates/inf-game/src/object_runtime/model_callback/tests.rs)
authenticate original bodies and cover actual-class branches, lazy reads,
signed/wrapped arithmetic and required backing.
[Object-model tests](../../crates/inf-game/src/software_scene/object_models/tests.rs)
check captured source through children and stopped command/RNG prefixes.
These bounded checks and consumed release smoke tests remain separate from
matched retail timelines, complete animation and full intro/briefing acceptance.
