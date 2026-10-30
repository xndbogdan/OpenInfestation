# Infestation actor behavior dispatch

This document owns the common `004294A0` AI descriptor dispatcher in the
`00428650` family, shared actor motion/physics, and bounded prefixes of the
intro actors' enclosing updates.
It is distinct from the world task manager and retirement. The dispatcher is
[behavior_dispatch.rs](../../crates/inf-game/src/object_runtime/actor/derived/behavior_dispatch.rs).
Its required `FamilyBehaviorHost` retains arena custody for target probes,
descriptor callbacks, process RNG and audio. No callback succeeds by default.

## Mission 22 callback admission

The retained selector-88 script and actual constructor dispatch identify these
actors. References are authored one-based placement references, not H13 slots.
The retail callback entries are pinned by the
[intro corpus test](../../crates/inf-game/src/object_runtime/actor/intro_tests.rs)
and [behavior tests](../../crates/inf-game/src/object_runtime/actor/derived/behavior_dispatch_tests.rs).

| Script list | References | Selector | Final vtable | Virtual +24 | Virtual +D8 |
| --- | --- | --- | --- | --- | --- |
| Primary | 48, 49, 50, 51, 83, 84 | 12 | `00527058` | `004679F0` | `00468D10` |
| Secondary | 68, 69 | 10 | `00520CC8` | `00424830` | `00425AF0` |
| Secondary | 52, 79, 80 | 36 | `00527EE0` | `00481710` | `00425AF0` |
| Exit | 97, 98 | 35 | `0052273C` | `0043E6D0` | `0043F880` |

Each +D8 wrapper passes a stack descriptor wrapper to `004294A0`. Scoring is
`00425C30` → descriptor virtual +04; enter/exit is `0043F990` → descriptor
virtual +08; update is `0043F9B0` → descriptor virtual +0C. Duration uses
wrapper virtual +0C: ordinary `0043F9D0` dispatches a nonnegative index to
descriptor virtual +10, while selector 12's `004690F0` produces a timer from
process RNG without reading a descriptor. Periodic target probes use `00425C60`
and twelve-byte callback/actor-offset/optional-input-factory records.

`FamilyBehaviorProfile` retains the executable descriptor identities:

| Native wrapper | Selection | Descriptor pointer table / count | Periodic table / count |
| --- | --- | --- | --- |
| `00425AF0` | WORD +266 = 0 | `00530740` / 3 | `005307B0` / 3 |
| `00425AF0` | WORD +266 = 1 | `0053074C` / 8 | `005307B0` / 3 |
| `00425AF0` | WORD +266 = 2 | `0053076C` / 9 | `005307B0` / 3 |
| `00468D10` | null actor virtual +7C | `0053A098` / 8 | `0053A0E0` / 5 |
| `00468D10` | nonnull actor virtual +7C | `0053A0B8` / 9 | `0053A0E0` / 5 |
| `0043F880` | ordinary | `00533D38` / 12 | `00533D80` / 3 |

`dispatch_actor_family_behavior` owns the actual +D8 wrapper entry before
calling the common dispatcher. `00425AF0`/nonnull-`00468D10` first tick a present
+218 helper through `00483340`. Its constructor initializes count +08 to zero;
zero and signed-negative counts return without reading other allocation holes.
Positive entries now use the [shared history fade](FAMILY_HELPER_HISTORY.md#fade-and-clear),
preserving partial weight/compaction writes and destination padding. Null-`00468D10` does
not tick that helper. `0043F880` first decrements signed timer +210 and resets
+20C to the process null handle at expiry; an initially zero timer preserves
+20C. Directly calling `dispatch_family_behavior` retains the supplied-wrapper
boundary and executes neither prefix.

## Dispatcher policy and custody

Time is the original scheduler microsecond DWORD. Lifecycle +C4 equal to 0/1
forces descriptor 0/1. Otherwise +22E bit 2 preserves an index greater than 1.
A null descriptor wrapper on the ordinary branch returns immediately. Bit 4
exits a nonnegative current descriptor before writing +184 = -1. Any low-byte
bits in 0x0D clear +21C.

Embedded target decay `00482DE0` handles +1A0's signed-positive count. Each
entry's +08 timer is written before +04: zero or unsigned timer <= delta
becomes 1, otherwise subtract delta. The portable fixed array supports the
four embedded entries and reports positive overflow at that feature.

Unsigned +21C above delta decrements. Otherwise a present periodic table
consumes one process RNG draw and sets +21C to
`(draw & 0xFFFF) * 4 + 250000`. Bit 8 instead sets +22E bit 1, uses base
1000000 and scans the full table. `004295AA` reads +220, increments it as a
WORD, zero-extends, uses signed division by the captured count, writes the
remainder to +220, then invokes the probe. A true result sets +22E bit 1.

Without bit 1, signed +19C > signed delta preserves the current descriptor
and subtracts delta. Otherwise one RNG draw tests occasional sound; a low
two-bit zero consumes a second draw and chooses +17C/+17E. Positive signed
slots require registration/acquisition and actual `00487F10`/`004923C0`
effects. The dispatcher then clears bit 1 and refreshes a nonnegative current
descriptor's duration.

Candidates begin at index 2. A noncurrent candidate queries actor virtual
+110 for bias. A current candidate uses signed +188 elapsed and +204 retained:

```text
elapsed < 7500000:
    bias = wrapping(2 * retained - (wrapping((elapsed / 234375) * retained) >> 4))
otherwise:
    bias = 0
```

The first strictly positive score wins; only a strictly larger score replaces
it. Low five +22E bits are cleared before transition callbacks. A changed
selection exits the old descriptor, writes native +228 status, calls actor
virtual +10C to delete real targets, then enters the new descriptor. Entry
sees old +184; the dispatcher publishes the index after entry returns, obtains
duration and clears +188. It then invokes current update and adds delta to
the callback's reached +188 with DWORD wrapping. Errors retain reached writes
and RNG draws. Fields are reread after mutable callbacks.

Forced lifecycle index 0/1 carries a native stack-wrapper pointer into +204,
rather than a scored integer bias. The portable `Option<u32>` remains unknown
for that opaque pointer write; no executable address is fabricated as a
runtime pointer. The lifecycle descriptors and any future required read
remain separate.

### Duration and class-specific candidate bias

Selector 12's descriptor wrapper vtable `005271AC` installs `004690F0` at
virtual +0C. This routine ignores its index argument and consumes exactly one
`0048F060` process RNG draw:

```text
duration_microseconds = ((draw & 0xFFFF) + 125000) << 5
```

Its result ranges from 4,000,000 through 6,097,120 microseconds. Both null and
nonnull behavior profiles use this wrapper. Ordinary `0043F9D0` instead
dispatches a nonnegative index to descriptor virtual +10; its separate negative
index branch consumes one draw and returns `((draw & 0xFFFF) + 46875) << 6`.
Those are distinct native duration policies.

The represented intro actors' virtual +110 consumers share arithmetic while
preserving their distinct retained WORD offsets. Each reached noncurrent
candidate consumes one process RNG draw before rereading actor +23C:

| Concrete actor class | Final vtable | Native +110 | Unsigned WORD in retained +23C |
| --- | --- | --- | --- |
| Family `004236D0` base | `00520B84` | `004350E0` | +0C |
| Selector 10 | `00520CC8` | `004350E0` | +0C |
| Selector 36 | `00527EE0` | `004350E0` | +0C |
| Selector 12 | `00527058` | `00468E00` | +10 |
| Selector 35 | `0052273C` | `0043F960` | +0E |
| Selector 34 | `00521EAC` | `004350E0` | +0C |

```text
product = DWORD wrapping((draw & 0xFFFF) * retained_unsigned_word)
bias = signed product >> 16
```

The multiply retains the low DWORD and the shift is arithmetic. The input is
live shared allocation data, not a copied constructor scalar. Its custody is
already represented by [selector12.rs](../../crates/inf-game/src/object_runtime/actor/derived/selector12.rs):
`00468330` retains payload +58 at +23C, command 23 can replace that retained
address, and postinitializer `00468040` clamps its current WORD +10 to at most
1000. A callback must reread the current allocation after consuming RNG;
missing retained custody must preserve that preceding draw.

`004350F6` and `0043F976` perform the family/selector-34 +0C and selector-35 +0E reads;
their pointers are reread after the calls to `0048F060`. The existing
[family retained owner](../../crates/inf-game/src/object_runtime/actor/derived/family4236d0.rs)
and [selector-35 retained owner](../../crates/inf-game/src/object_runtime/actor/derived/selector35.rs)
already preserve those shared mutable allocations. The family constructor
payload and command 24 can replace +23C; selectors 34/35 retain their payload +58
and command 23 operand.
Their +110 consumer does not clamp or copy the current bias WORD.

The existing dispatcher now executes these recovered policies at its
actual call sites. The supplied `FamilyBehaviorProfile::Selector12` wrapper
selects duration independently of the actor's class; the concrete actor
selects virtual +110 independently of the supplied wrapper. Both use
`FamilyBehaviorHost::random`, preserving the caller's process RNG. Missing
current +23C custody or a reached allocation-range error preserves the preceding
draw. Ordinary duration is also owned through the singleton mapping below;
unrepresented classes' +110 remains a required host effect. No constructor
timer default is introduced.

The tracked native checker executes
258 original duration/bias cases and 30 supplied-dispatch receipts without
instruction patches. The [Rust comparison](../../crates/inf-game/src/object_runtime/actor/derived/behavior_callback_native_tests.rs)
uses the [authenticated receipt](../../crates/inf-game/tests/fixtures/family_behavior_callbacks_native.json)
to pin signed overflow, index-independent duration, mutation after RNG,
retained-address replacement, current-duration/entry ordering and failure after
the reached RNG/timer prefix for each actual class. Descriptor score/change/update remain explicitly
supplied services in those comparisons. The unknown-cursor case stops at +220
before the concrete callbacks; the water families' +230 remains unknown throughout.

These attached dispatcher policies close bounded callbacks shared across
missions. The enclosing +24 still requires its prior water/cursor inputs,
periodic probes, descriptor/target/weapon effects and remaining tail. The accepted
first-timer receipt
ends before the selected periodic probe executes; it proves no descriptor
duration, candidate-bias call or completed +24 update. Allocation and remaining
callback limits stay in [the intro gates](#intro-gates-and-first-remaining-owners).

### Singleton descriptors and ordinary duration wrappers

[behavior_descriptors.rs](../../crates/inf-game/src/object_runtime/actor/derived/behavior_descriptors.rs)
owns the stable `FamilyBehaviorDescriptor` identity selected by each executable
pointer table. Entries are four-byte pointers to distinct static descriptor
objects in EXE BSS. Original CRT initializer leaves `00423390..00423690`,
`0043CC90..0043D010` and `00466D80..00467020` write their vtables. These object
addresses identify authored singleton data; they never become portable actor
or allocation pointers. All 49 entries across the six profile views select
31 singleton objects. In particular, family mode 0's index 2 is `00546A64`,
not mode 1/2's `00546A60`; selector 12's nonnull behavior view inserts
`0054ECC4` at index 4 and shifts subsequent entries.

Ordinary wrapper `0043F9D0` reads the signed index, then for nonnegative indices
loads `wrapper+08[index]`, reads that singleton's vtable, and invokes virtual
+10. Each admitted singleton duration method consumes exactly one process
`0048F060` draw; no class or scene supplies its duration:

| Descriptor +10 | Original arithmetic in microseconds | Reached table indices |
| --- | --- | --- |
| `0043FA00` | `((draw & 0xFFFF) + 46875) << 6` | All ordinary entries except those below |
| `0043FBE0` | `((draw & 0xFFFF) + 93750) << 4` | Family mode 1/2 and selector 35 index 5 |
| `0043FC40` | `((draw & 0xFFFF) + 31250) << 4` | Family mode 1/2 and selector 35 indices 4 and 6 |

The negative-index branch at `0043F9E6` skips the table and uses the first
formula directly. Selector 12's descriptor vtables instead contain only
delete/score/change/update slots; reading a fifth slot would consume the next
vtable's deleting destructor. Its wrapper `004690F0` deliberately ignores the
index and uses `((draw & 0xFFFF) + 125000) << 5`, including indices outside its
descriptor count. The portable dispatcher executes duration through the
supplied profile's wrapper policy independently of the actual actor class.
The obsolete ordinary-duration host callback is removed. Invalid nonnegative
ordinary indices fail before the unowned table read and process RNG.

`dispatch_actor_family_behavior` selects the actual family mode or selector-12
behavior presence before the helper prefix, matching the native stack-wrapper
construction. Selector 35's +210 comparison is signed on both timer and raw
microsecond DWORD; its subtraction wraps. An expiry writes +210 = 0 before
reading the actual process `0054E894` and publishing it at +20C. Registry NULL
is a separate source; missing process backing stops after the timer clear while
preserving +20C and bypassing common dispatch. The empty helper branch
preserves all other +218 allocation holes; positive counts reach the
[shared history fade](FAMILY_HELPER_HISTORY.md#fade-and-clear), including its
required process-null tail input and stopped mutation prefixes.

The tracked descriptor checker
executes original CRT leaves and duration wrappers without instruction patches,
then stops original +D8 entries at their next `004294A0` boundary. The
[authenticated receipt](../../crates/inf-game/tests/fixtures/family_behavior_descriptors_native.json)
and [Rust comparisons](../../crates/inf-game/src/object_runtime/actor/derived/behavior_descriptor_tests.rs)
pin all singleton/table/callback identities, 438 duration cases and 31 entry
prefixes across seven profiles. Selector 34's `00435050` entry shares Selector
35's `+210/+20C` prefix over tables `005329F8` (eight descriptors, CRT leaves
`00433880..00433AE0`) and `00532A20`; descriptors 0-6 reuse Selector 35's
callbacks, and index 7 scores through `004351E0`. Cases cover all ordinary table indices, negative branches, selector
12's ignored large index, low-WORD RNG boundaries, zero/negative helper counts,
signed timer expiry and wrapped subtraction. The scalar scores and copied-point
changes below now have concrete owners; other score/change/update callbacks remain required
descriptor services. These receipts supply actor and stack storage and establish neither positive helper
entry custody, +220/+230 defaults, enclosing +24 execution nor intro acceptance.

### Scalar descriptor scores feeding motion selection

[descriptor_motion.rs](../../crates/inf-game/src/object_runtime/actor/derived/descriptor_motion.rs)
maps all nineteen literal virtual `+04` callback identities. A data-bearing
`FamilyDescriptorScoreRequest` selects an explicit policy by the actual
`FamilyBehaviorDescriptor.score_address`, independently of profile index,
scene or target presence. Owned pure sources ignore the current flag. The
original four scalar callbacks retain these arithmetic policies:

| Callback | Unsigned coefficient WORD through actor `+23C` | Native source |
| --- | --- | --- |
| `0043FB20` | Retained `+02` | Unsigned maximum of actor DWORDs `+1E0/+1F0` |
| `004351B0` | Retained `+06` | Actor DWORD `+1E8`, without reading its target pointer |
| `0043FB60` | Retained `+08` | Signed actor DWORD `+164` divided by two toward zero |
| `00469230` | Retained `+0C` | The same signed division with its distinct coefficient |

Each of these four callbacks reads the retained pointer and coefficient first. A zero
coefficient returns zero before reading the source or bias. Otherwise it
returns the low DWORD of `(coefficient + bias) * source`, with wrapping
addition and multiplication. A high-bit coefficient remains unsigned; the
maximum compares unsigned scores, whereas the radius division is signed.
Negative odd radii use truncation toward zero. No target allocation,
reference, position, timer, RNG or actor mutation belongs to these callbacks.

The portable owner retains the real shared `RuntimeAllocation` and its base
offset for the represented family-4236D0, selector-12, selector-34 and
selector-35 `+23C` bindings. Missing or short storage remains a reached error,
including with an otherwise zero coefficient. Live byte mutations are visible
on the next call. Class admission identifies the represented portable owner;
the native callbacks themselves do not check class IDs. Other retained
bindings stay explicit unsupported errors.

The unchanged-instruction checker
executes original singleton initializers, the `00425C30` wrapper and these
four actual callbacks. The
[compact receipt](../../crates/inf-game/tests/fixtures/family_descriptor_motion_native.json)
and [Rust comparisons](../../crates/inf-game/src/object_runtime/actor/derived/descriptor_motion/tests.rs)
cover 312 cases, including unsigned score selection, signed negative/odd
radii, coefficient and bias wrapping, current-flag independence and the
zero-coefficient read prefix. Native read hooks distinguish the reached
retained reads from bypassed source reads, and assert no actor/retained writes
or RNG change. Supplied storage establishes scalar callback behavior; it does
not execute descriptor changes, Type-8 target production, movement or the
enclosing actor update.

The other literal wrappers distinguish constants, pure sources and required
callbacks. `00425DF0` returns one and `00469370` returns zero without reading
the receiver or `+23C`. Common/selector-12 `+F0` wrappers use the unsigned
maximum of `+1F8/+1D8`; selector-12 `00469130` owns its full `+B4==18` gate
and BYTE `+29D` controlled DWORD `+270` shift. Home `00428320` uses signed
WORD displacement, the actual integer square root and signed-radius clamp.
These source-call wrappers reread the live coefficient after the reached
source before wrapping bias addition and multiplication. Sources can mutate
actor/RNG state, as the common `+FC` owner below does.

The canonical score request carries the dispatcher's genuine current flag,
separately from bias. Selector12's `+134` is the complete `00461C20` DWORD
`+278` getter. Its `+140` is `00467840`: unsigned max of `+280/+288`, with
current-only target-pointer gates and a lazy required `+190` read. Released
references remain nonnull for those gates. The existing score call takes an
explicit live RNG loan alongside its request for [common `+FC`](../../crates/inf-game/src/object_runtime/actor/derived/descriptor_motion/fc.rs):
final classes10/36 use `00424F50`, while final12/35 use its shared source
`00427320` directly. False-current returns actual DWORD `+200` before target,
distance, BYTE `+260`, RNG or basis work. True-current `00427320` bypasses
distance on NULL148; otherwise it reads signed DWORD14C before virtual `+14`.
Signed distance greater than 256 uses the owned radius score. The near branch
requires the actual referent's raw DWORD44: only represented `RouteNode`
storage owns that layout; another object's same-offset field is not substituted.
Missing distance/referent storage remains a reached typed requirement.

Common true-current captures that source result first. BYTE260 zero consumes
one low-WORD shared RNG draw, returning early when it is unsigned `>=4000h`.
Otherwise fresh148's actual virtual `+14` query precedes BYTE260=1. A NULL
referent or NULL previous node then returns normally. The owned geometry uses
wrapped signed-WORD target-minus-previous direction, literal actor-minus-current
X but actor-minus-previous Y/Z, and the native float-spill/FTOL reductions.
Its squared sum wraps as an unsigned DWORD; `>=10000h` skips the basis query.
The remaining dot uses actual embedded44 from final virtual `+48`, accumulating
Z+Y then X; only strict `>f32(3F666666h)` clears BYTE260. Equality and unordered
comparison retain one. Reached RNG/260 writes survive later errors. The consumed
[production host](../../crates/inf-game/src/session_platform/world_actor_behavior.rs)
lends its actual shared RNG, without a class/mission/placement admission rule.

The 004236D0 family's `+12C` (`00423B40`, Follow Player) is
[owned](FAMILY4236D0_RUNTIME.md#follow-player); other classes' `+12C`
retains a typed boundary. Direct `0043E360/0043D730/0043D8F0` are
[Selector35's](SELECTOR35_RUNTIME.md#rescue-and-go-to-space-descriptors-10-and-11)
and stay typed boundaries for any other receiver.
Escort source `00428990` is owned. It reads process NULL `0054E894` before
escort handle `+20C` and scores zero when they are equal. Otherwise it resolves
the handle through `0047AD90`. A referent with signed `+C4 >= 1` yields the
wrapped signed-WORD distance plus `0x100`, or `0xFFFF` when that distance
exceeds signed `+164`; a bit31 square uses `isqrt(sum >> 2) << 1`. A NULL or
dead referent clears `+210`, stores the NULL reread at `00428A20` into
`+20C`, and scores zero. The production host lends the live process NULL. A
caller without one stops at `MissingProcessNull` after the coefficient read.
Selector 34's `004351E0` is the `0043FCE0` twin over retained `+23C` WORD
`+0E` (the same numerator offset its postinitializer uses). The
[30 native escort receipts](../../crates/inf-game/tests/fixtures/family_escort_source_native.json)
run both wrappers through `00425C30`, with only `0047AD90` supplied.
Zero coefficient bypasses every source/current gate. Missing retained bytes
remain errors. [Wrapper tests](../../crates/inf-game/src/object_runtime/actor/derived/descriptor_motion/wrappers_tests.rs)
cover source-read gates; [FC semantic cases](../../crates/inf-game/src/object_runtime/actor/derived/descriptor_motion/fc_tests.rs)
and [complete native/table guards](../../crates/inf-game/src/object_runtime/actor/derived/descriptor_motion/fc_native_guards.rs)
pin the current/RNG/error order, bit31 unsigned threshold and literal geometry.
Whole XYZ/Home storage retains its partial-knownness limit. A scalar score
does not establish its descriptor's effects or complete the enclosing update.

### Dormant descriptor virtuals

[descriptor_dormant.rs](../../crates/inf-game/src/object_runtime/actor/derived/descriptor_dormant.rs)
owns wrappers `00469380/004693A0` through the actual final actor table.
Selector IDs name recovered class policies, independently of scene or model.

| Class | Change `+E0` | Update `+DC` |
| --- | --- | --- |
| 10 / 36 | `00424790`: false bypass; true publishes status `00530D44` and clears flags `0x104`, preserving list bits | `004247C0`: captured XZ before mutable H12 acquisition, fresh Y afterward; dispose only when signed terrain-Y exceeds unsigned size `*5/4` strictly |
| 12 | `00428630`: false bypass; true publishes status only | `00404840`: complete RET4 no-op |
| 35 | `0043E120`: mandatory burst, conditional radius and normal status suffix, including effects on false entry | `0043E1C0`: load-gated trail6 with ordered RNG |

Disposal queues `00479FA0`; it does not destroy the actor or implicitly release
targets. Flag changes use the original dirty test without list setters.
[Tests](../../crates/inf-game/src/object_runtime/actor/derived/descriptor_dormant/tests.rs)
pin acquisition order, strict threshold, failure prefixes and complete native
bodies/final tables. [Selector35's separate lifecycle owner](SELECTOR35_RUNTIME.md#dying-and-dormant-lifecycle)
borrows the additional particle/radius/RNG services through production wrappers.

### Copied-point descriptor changes and StayStill update

[descriptor_points.rs](../../crates/inf-game/src/object_runtime/actor/derived/descriptor_points.rs)
owns three shared virtual `+08` change callbacks and StayStill's virtual `+0C`
update. `FamilyPointTargetChangeRequest::from_descriptor` chooses an explicit
kind from the recovered callback identity, independently of scene or level.
The [shared target authority](TARGET_RUNTIME.md) owns the type-8 resource,
its clone/query policies and `00427390` weighted publisher.

| Descriptor callback / shared owner | Entering writes before allocation | Point copied after successful allocation |
| --- | --- | --- |
| Roam `0043FBA0` / `00427990` | `+228=005311C8`, then DWORD `+18C=0` | Current six XYZ bytes at `+32` |
| Home `00435190` / `00427A00` | `+228=005311D0`; preserve `+18C` | Home WORDs at `+222/+224/+226` |
| StayStill `00425E00` / `00427B40` | `+228=005311E4`; preserve `+18C` | Current six XYZ bytes at `+32` |

Low-byte entering zero bypasses all actor and allocation reads. Entering
nonzero reaches a `0x14`-byte allocation before reading the current process
null handle or point span. An actual allocator null publishes null `+148`
without those reads. Successful construction copies the six supplied bytes,
leaves `+0E/+0F` padding unwritten, and only then publishes `+148`. An
unavailable allocation or reached source preserves the earlier status/timer
writes and previous `+148`. These entries never delete that previous resource;
the behavior dispatcher's earlier `+10C` owns its release.

StayStill update `00425E20 -> 00427BB0` ignores delta and first tests target
presence and its actual DWORD kind `+10 == 8`. Only then does it read current
XYZ and copy them to target `+08`, preserving handle, allocation and unrelated
fields. The gate is the stored native kind, not the target's vtable class.
There are no RNG, dirty-flag or timer effects on this update.
Home's update callback `004026D0` is an empty native stub. Roam's timed
`00427800` update has its separate timer/RNG/terrain owner below.

Selector12's captured26C and point callbacks use the class-authenticated
[descriptor target owner](ACTOR_DESCRIPTOR_CHANGES.md). Its `00435170` update
admits actual Selector12 `+124 = 00404840` RET4; common/Selector35's same-wrapper
active updates remain required. Its `00469210` point update loans the same
retained heap/process RNG and leaves actor DWORD14C unchanged. Shared
copied-point and StayStill policies above remain independent.

check_family_descriptor_points.py
executes original CRT singleton initialization, literal wrapper selection,
change/update wrappers and reached callbacks without instruction patches. Its
[138-case fixture](../../crates/inf-game/tests/fixtures/family_descriptor_points_native.json)
and [Rust comparisons](../../crates/inf-game/src/object_runtime/actor/derived/descriptor_points/tests.rs)
cover all reached profile indices, low-byte entry gates, prior target classes,
actual kind poisoning, allocation null and stopped allocation/source prefixes.
Allocation is the sole substituted host; actor/home/process storage is supplied.
The scalar-score receipts above remain separate evidence. Neither set executes
remaining descriptor callbacks, enclosing `+24` or a
complete intro. Full-update attachment remains bounded by those services and
the unwritten `+220/+230` timers.

### Roam timer, heading and raw terrain update

[descriptor_roam.rs](../../crates/inf-game/src/object_runtime/actor/derived/descriptor_roam.rs)
owns descriptor update `0043FBC0 -> 00427800` through one
`FamilyRoamUpdateRequest`. The literal singleton appears at family modes 1/2
index 4, selector 12 absent/present indices 5/6, and selector 35 index 4.
These are behavior-table indices, independently of actor selector or scene.
The caller supplies the native delta DWORD, actual sine table, shared process
RNG word and required mutable H12 acquisition service.

`00427810/12` compares both timer `+18C` and delta as signed DWORDs. A timer
greater than delta receives a wrapping subtraction and returns before RNG,
basis, position, terrain or target reads. Otherwise `0042781D` first adds
2,000,000 to the old timer. One original `0048F060` draw then supplies its low
WORD shifted left by six; `00427838` adds that amount to the reached timer.
The expiry branch does not subtract delta and does not loop. It consumes a
second process draw for the heading. Unknown `+18C` remains an explicit
reached error; no allocator `+220/+230` value is read or initialized here.

The heading is `q = (32768 - unsigned_draw_WORD) SAR 2`, then `q + 4096`
when positive or `q - 4096` otherwise. Zero deliberately chooses the latter
arm. Plain signed sine samples shifted arithmetically by two form local X/Z;
local Y is zero. The existing `0047A6C0 -> 004909E0` vector owner transforms
these signed WORDs in place with 53-bit nearest-even QWORD conversion and
keeps each low WORD. Current actor X/Z add with WORD wrapping afterwards.
Transformed Y and actor Y are discarded; the raw terrain sample supplies Y.

Only after capturing X/Z does the native path obtain H12 slot zero through
resident `*0055EE3C` or synchronous `0055EE38` virtual zero. The required
`FamilyRoamTerrainHost` receives the same live arena, allowing acquisition
mutations to remain visible. It returns the reached `TerrainRuntime`, including
post-acquisition sampling policy. The existing `raw_height_at` matches the
supported native spacing 128/256 and height shifts 4/5: align low-WORD X/Z,
index the `X*512+Z` cell, sign-extend its height byte, shift and retain the low
WORD. No interpolation, plane clamp or water offset is inserted.

`00427937` captures the then-current `+148` target before reading the height.
Only `0042795B` subsequently reads its actual kind DWORD `+10`; kind 8 copies
the point to raw `+08` and `+0C`, independently of the vtable class. Other
kinds preserve their target. A null target reaches an explicit error after
terrain sampling, preserving timer/RNG/acquisition effects. Direct point
stores do not dirty actor XYZ, allocate targets, reset handles or modify other
target fields. Acquisition failure and missing actor position retain all prior
timer and process RNG writes.

check_family_descriptor_roam.py
executes actual singleton initialization, update wrapper `0043F9B0`, Roam
callbacks, RNG, sine lookup, vector transform and raw H12 height instructions
without byte patches. The [compact fixture](../../crates/inf-game/tests/fixtures/family_descriptor_roam_native.json)
and [Rust comparisons](../../crates/inf-game/src/object_runtime/actor/derived/descriptor_roam/tests.rs)
cover 460 cases and 480 ordered updates across the five literal indices:
signed timer boundaries/equality/wrap, heading boundaries, nonidentity bases,
both supported terrain policies, raw kind/vtable mismatches, repeat cadence,
mutable acquisition, stopped position/acquisition prefixes and real late null
kind-read faults. Missing-H12 acquisition is the sole substituted service;
the terrain grid, target storage and process state are supplied native inputs.
These receipts establish this callback, not enclosing `+24`, complete behavior
dispatch attachment or retail cinematic acceptance.

### Concrete weighted target ownership

The common family now retains real native type-2/type-8/type-16 resources in its six
weighted target slots +1D4/+1DC/+1E4/+1EC/+1F4/+1FC and in derived current slots
+148/+154. One shared request distinguishes actual actor pointers,
common-object pointers and copied-point pointers; six explicit offset slots
replace six separate APIs. `00427110`/`00427220`/`00427390` publish score before replacement, reuse accepted targets,
and delete/reconstruct after original crossed-class `00402720` false callbacks.
Their AL result means a previously empty pointer slot, including allocation
failure, rather than score change or success. Null input with an already null
pointer preserves the old score. [TARGET_RUNTIME.md](TARGET_RUNTIME.md#weighted-pairs-and-family-target-ownership)
owns the complete decision matrix, allocation boundary and native receipts.

`00429460` and every represented family's proven +10C wrapper now have
concrete target-clear owners at the shared dispatcher's reached call. The
obsolete `FamilyBehaviorHost::clear_targets` service is removed. Base/probe-only
families preserve +208; the other suffixes follow the native class policy.
Common destruction releases source and current
targets without retiring their referents. Selector12 owns its active
descriptor exit as described below. The shared
[family4236d0 destruction prefix](../../crates/inf-game/src/object_runtime/actor/derived/family4236d0/destruction.rs)
also owns `004237D0`, reached directly by the base, through selector36's
`00481170` jump, and after selector10's `004251D0` helper +26C release/null.
The destructor installs `00520B84` before dispatch: negative +184 bypasses
even the unsigned WORD +266 mode read; otherwise the original mode table
selects descriptor +08(actor, false), then normal return clears +184.
All ten unique callbacks have false branches with no actor effects under this
destructor vtable. Live subclass entry/update virtuals are not substituted.
Unknown mode/index retains the reached prefix, including selector10's released
helper. Selector34/35 active exits still stop at
`FamilyBehaviorDestructionBoundary` before lower resources are discarded.
The native checker and its
[86 receipts](../../crates/inf-game/tests/fixtures/family4236d0_destruction_native.json)
cover all three mode tables and classes, negative-index/invalid-mode guards,
helper-null and stopped helper-deletion prefixes. Helper deletion is a supplied
effect and lower `00429E70` is a stop boundary; arena tests separately cover
actual lower releases, registration and reached failure custody. Actual score producers
and unsupported descriptor effects remain explicit services; stored resources
do not establish their execution or close actor +24.

### Common Attack descriptor entry

[attack.rs](../../crates/inf-game/src/object_runtime/actor/derived/attack.rs)
owns shared entry `00429B00`, identified by the original descriptor callback
table. Zero entering byte returns before any actor read. A real entry
writes +228 = `00531244`. When both +1D4/+1F4 sources are null it sets +22E
bit 1 and skips RNG/cloning. Otherwise it writes +18C = 2,000,000, consumes
one process RNG draw and adds `(draw & 0xFFFF) << 4`. Selection compares
unsigned scores +1F8/+1D8: +1F4 wins only when strictly greater, so a tie
selects +1D4. A lone nonnull source wins independently from score.

The entry compares native raw pointer presence separately from live resource
custody. A `Released` source still participates in presence and unsigned score
selection; an unselected released source does not block a live winner. A
selected released source reports its slot at the reached clone dereference,
after +228/+18C and process RNG, without exposing freed storage as a live
target. The entry calls a selected live target's real native clone and publishes its
result at +148 before virtual +10. An actual allocation-null clone remains
null at +148 and reports the reached `MissingClone` boundary; an unavailable
allocator is a different explicit service error. Type 2 independently
resolves its actor handle; type 16's original null virtual does not enable
the actor branch. Only a resolved actor sets +22C = 1 and +208 to the sum
of both unsigned size WORDs +28 plus 350.

A present +218 helper compares its prior +00 handle. A different handle
writes that handle and +04 = 0; an identical handle replaces signed +04 with
division by two truncated toward zero. Unwritten helper words fail at their
actual read after already reached +22C/+208 writes. Finally the signed WORD
sound binding +180 calls the required sound service only when positive,
retaining native registration/lazy acquisition, positional 00487F10 and
nullable 004923C0 cleanup as a named host boundary. No zero sound binding or
allocator word is invented to pass it.

The tracked Attack checker,
[54-case fixture](../../crates/inf-game/tests/fixtures/family_attack_native.json)
and [focused tests](../../crates/inf-game/src/object_runtime/actor/derived/attack/tests.rs)
pin this bounded entry against unchanged native instructions and independent
target/score/helper inputs. These receipts do not establish the separately
owned periodic target producers, later weapon effects,
descriptor score services, or enclosing actor +24 execution.

### Common Attack descriptor update

[`descriptor_attack_update.rs`](../../crates/inf-game/src/object_runtime/actor/derived/descriptor_attack_update.rs)
owns common `004298A0` through an explicit delta/settings request, shared
process RNG and required helper-record host. It shares the existing type-2/8/16
target owners, independently of mission or scene. This is a bounded task
kernel; it does not attach the enclosing actor `+24` or close its other services.

`0043F9B0` selects descriptor virtual `+0C`; wrapper `00435130` invokes the
actual actor's `+F4`. The four aligned executable-data references to that
wrapper retain different destinations:

| Descriptor/table selection | Singleton / vtable | Actual actor destination |
|---|---|---|
| Family mode 1/2, index 2 | `00546A60 / 00520AE4` | Common `004298A0` on represented family classes |
| Selector 12, nonnull actor virtual `+7C` result (behavior `+140`), index 4 | `0054ECC4 / 00527048` | Common `004298A0` |
| Selector 34, table `005329F8`, index 2 | `00546B2C / 00521E34` | Separate `00433CC0` |
| Selector 35, index 2 | `00547634 / 00522674` | Separate `0043D2E0` |

Family mode 0 selects StayStill. Selector-12's `00468D10` calls virtual
`+7C` at `00468D18`; `004027A0` returns actor `+140`. A null result omits
the Attack singleton. This is separate from type-2 refresh's process settings
DWORD `0054E8A8+7C`. Native common `+F4` bindings include base `00428650`
(`00520F34`), scalar base `004293D0` (`0052104C`), probe base `00429E20`
(`00521164`), family `004236D0` (`00520B84`), selectors 10/12/36
(`00520CC8/00527058/00527EE0`), selector 21 (`0051F7A8`, constructor
`00405590`) and selector 13 (`005269C4`, override `004606A7`). The three
base vtables have no-op `+D8` `00404840`; method binding alone does not
establish descriptor execution. Selectors 13/21 remain unrepresented final
classes. The bounded port admits its represented common family classes and
rejects selectors 34/35's different `+F4` owners, not a scene-number policy.

| Reached branch | Native writes and next source |
|---|---|
| Null current `+148` | OR low byte `+22E` with `0x10`, then return before timer/state/RNG reads |
| Signed `+18C > delta` and signed `+150 >= 3` | Wrapping timer subtraction; skip selection/RNG and continue to the helper tail |
| Rescan | Store 2,000,000, draw process RNG and add low WORD shifted four. Any present `+1D4/+1F4` source repeats this timer/draw sequence before reloading source selection |
| Different selected/current raw `+04`, signed `+150 < 3` | Delete current with flag 1, clone selected, publish actual clone/null; leave helper words unchanged |
| Different handles, signed `+150 >= 3` | Invoke selected virtual `+18`, retaining refresh writes. Signed return at most 1 skips replacement; otherwise test raw-point distance |
| Completed rescan, reread signed `+150 < 3` | One additional draw; unsigned low WORD below `0x2000` ORs low byte `+22E` with `0x10` |
| Signed `+14C < +164` and a present helper `+218` | Wrapping add delta to helper `+04`. Signed value above 8,000,000 reaches the required helper-record service; OR low byte `+22E` with `0x0C` only after success |

An existing `+22E` bit `0x10` does **not** bypass this routine. Source selection
compares unsigned `+1F8/+1D8`; ties choose `+1D4`. Presence and score reads
remain distinct from live allocation custody. It reads selected raw `+04`
before current raw `+04`; equal handles skip refresh and replacement even
when the allocations or target classes differ.

The distance path reads selected raw WORDs `+08/+0A/+0C`, not virtual `+1C`
point data: type 2's virtual query instead selects `+14`. Subtractions wrap
in WORDs, then sign-extend before wrapping DWORD square/sum arithmetic.
With the sum's high bit set, native `0048EFC0` receives the logical sum
shifted two and its result doubles; otherwise it receives the full sum.
Replacement requires signed wrapping `4*distance < 3*+14C`. The clone/null
store precedes helper publication. A reached helper gets the selected raw
handle at `+00` and zero at `+04`, including an actual null clone result.

The helper tail captures current raw point data and forms actor-minus-target
WORD displacement. The shared `FamilyHelperRecordRequest` supplies that vector,
raw handle and literal weight 90 to `004830B0`'s required mutable host with
the current helper as its native receiver. Failure preserves reached timer,
target and helper writes. Success retains host mutations and ORs `0x0C` into
the then-current actor flags; it does not replay an earlier flags snapshot.

Native construction leaves derived `+14C/+150` unwritten. Strict and the old
four-carry allocation mode preserve both as unknown until recovered writers
publish them. The explicitly selected production
[extended allocation-backing approximation](../../crates/inf-game/src/fresh_heap.rs)
supplies fresh `+14C=0` and `+150=0`. Infest01 bounds the latter: its two
spawned Selector12s read `+150` as zero at their first `004298CE` read,
before any tail store
(census).
Reads never recompute or reseed either field before dispatch:

| Selector | `+D8` call | Current-target virtual `+18` | `+150` store | `+14C` store |
|---|---|---|---|---|
| 10 | `004248D7` | `004248EE` | `004248F7` | `00424A6E` |
| 12 | `00467A71` | `00467A88` | `00467A91` | `00467C00` |
| 36 | `00481781` | `004817BE` | `004817CE` | `004819B3` |

Null current targets preserve both fields; selectors 10/12 can preserve
`+14C` on their separate `+154` branch. Constructor `00425E60` nulls `+148`
at `00425E9D` and `+154` at `00425EA3`, skipping `+14C/+150`; inspected
family and selector 10/12/36 constructors do not initialize those holes.
For a new actor's first Attack transition, `00429B00` clones the selected
target into `+148` without publishing `+150`. The common dispatcher calls
entry at `004297FF`, stores descriptor/duration, and immediately calls update
at `00429876`. Its signed `+150` read at `004298CE` can therefore precede the
first outer movement publication. Attachment `00405420` and selector66's
initial update change model, flags and shared identity only; they do not
initialize this derived carry. The accepted timer/history ledger covers
`+220/+230/+14C`, not this fresh child field. A new value requires allocator
custody or an explicitly named approximation; moving target refresh before
`+D8` would violate the authenticated order.
Current `+148` also carries a retained heap reference: deletion before a
stopped clone leaves the raw reference pointing to a `Released` entry,
preserving native pointer presence without exposing freed storage. A completed
clone or allocation-null store publishes the replacement reference or null.
The [target authority](TARGET_RUNTIME.md) owns that lifetime boundary.

The unchanged native checker,
[receipt fixture](../../crates/inf-game/tests/fixtures/actor_attack_update_native.json)
and [focused comparisons](../../crates/inf-game/src/object_runtime/actor/derived/descriptor_attack_update/native_receipts.rs)
cover 406 cases, with 390 admitted comparisons and 16 native-only diagnostics.
They separate supplied state from constructor defaults and retain stopped prefixes.
The admitted resource domain excludes arbitrary pointer aliases and slot
rebinding inside clone/refresh; those cannot be promoted from native-only
diagnostics. The mutable host retains that callback boundary;
[`FamilyAttackUpdateServices`](../../crates/inf-game/src/object_runtime/actor/derived/family_helper_history.rs)
supplies the recovered [history recorder](FAMILY_HELPER_HISTORY.md#record-producer-004830b0)
with the arena-owned actor-service heap and a live process-null getter. These checks do not establish
weapon effects, outer update ordering or a complete intro.

### Shared behavior damage predicate

The [cache authority](BEHAVIOR_DAMAGE_CACHE.md) owns `0048D820` and its three
ordered packet-copy phases. Ordinary actors share the contiguous executable
descriptor region by captured address; selector 2 uses its actual inline
copy. Signed row selection can alias an adjacent profile, so nominal
constructor counts do not become predicate range guards. Process defaults
and copied packets preserve independent byte custody, including an unknown
opaque fourth byte. Required mutable creation/query/deletion services retain
all earlier cache writes on failure. The body uses the current candidate's
canonical damage responses and the existing resisted-damage arithmetic.
The shared candidate gate `00429CB0` now composes that predicate with the
current helper query after callbacks. [Particle factories](PARTICLE_FACTORY_RUNTIME.md)
now compose their native construction, packet sources and deleting kernels
through the cache's live process/RNG context. Lower actor resources,
network/impact services and later class tails remain boundaries. The shared
[target-selection authority](FAMILY_TARGET_SELECTION.md) owns the scanner,
inline-table services and common `+100` body with live task/predicate custody.
These owners do not complete actor `+24` attachment.

### Periodic record wrapper and temporary input lifetime

[behavior_periodic.rs](../../crates/inf-game/src/object_runtime/actor/derived/behavior_periodic.rs)
owns `00425C60` at the existing dispatcher's periodic call. The literal
twelve-byte records at `005307B0`, `0053A0E0` and `00533D80` retain executable
callback identity, byte offset within the actual actor, and an optional input
factory separately. Family modes and selector 35 share three record shapes;
both selector-12 profiles use five. These are distinct from singleton
descriptor pointers. No scene number chooses their policy.

A present record `+08` factory receives the actual actor in ECX. A null
result returns false before probe or release. An admitted temporary input
reaches record `+00` with ECX equal to the actor and stack arguments
`actor + record+04` and that input. An absent factory instead invokes the
same callback with a null input. Only the callback's low return byte determines
the Boolean result; a nonzero high DWORD with zero AL is false. A present
input is released through cdecl `005116F4` after the callback returns, even
when the callback is false. Its required host retains allocation custody on
a failed callback; no destructor or successful empty service is synthesized.
The admitted records are original immutable executable data; mutable actor
effects remain synchronous before each subsequent service.

The tracked periodic checker executes
the unchanged wrapper against all seven profile views, with explicit supplied
factory/probe/release boundaries. Selector 34's `00532A20` keeps the two common
records, then Selector 12's `+1FC` record with factory `00427640`. Its [137 receipts](../../crates/inf-game/tests/fixtures/family_behavior_periodic_native.json)
and [Rust comparisons](../../crates/inf-game/src/object_runtime/actor/derived/behavior_periodic_tests.rs)
pin literal records, null allocation, low-byte returns, temporary release,
actor mutation before/after each boundary and stopped failure prefixes.
The attached dispatcher test also preserves its preceding RNG/timer/cursor
writes before a failed probe. The shared producer owner below supplies
`00427640` and the player/route target callbacks; collision callbacks retain
their own owners and host composition. Wrapper success does not establish
allocator `+220/+230`, enclosing `+24` attachment or
a completed cinematic.

### Periodic player and route target producers

[target_producers.rs](../../crates/inf-game/src/object_runtime/actor/derived/target_producers.rs)
owns `00427490`, `00427510`, `00427620`, factory `00427640` and its virtual
`00427660`. One `FamilyTargetProducerRequest` admits the actual periodic
profile/index/literal record and selects `FirstPlayerActor` or `RouteNodes`
plus its native destination. Scene IDs and common target-type numbers do not
select these policies. A concrete periodic host can invoke these owners from
the existing wrapper; enclosing actor +24 composition remains separate.

`00427490` uses `0046B030`: only the first player in `G+60`, only its +D4
handle, and the actual shared actor registry resolver. It never falls back
to +D8 or another player. Empty player lists avoid registry lookup; a stale
primary handle resolves null. The candidate/self XYZ subtraction wraps each
WORD before signed extension and squared DWORD addition. Native comparison
is unsigned and strictly less than DWORD-wrapped actor +164 squared. An
absent/outside candidate publishes `(null,0)` through the shared actor target
publisher. Only an admitted candidate dereferences the temporary score input.
No RNG is consumed by this producer.

Factory `00427640` requests exactly eight bytes. Actual allocation null makes
`00425C60` skip callback and release; unavailable allocation is a reached host
error. Success initializes every byte: static vtable `00520F2C` at +00 and
the actual actor identity at +04. The host's periodic token owns allocation
and `005116F4` release custody separately from this data. It is not a native
target resource. Virtual `00427660` ignores its candidate argument and uses
the supplied squared-distance argument, rereading its own actor +164 through
`00427620`. The score is DWORD-wrapped radius minus shared `0048EFC0` root.
That root interprets the distance DWORD as signed; negative values return
zero. Selector-12's route record also uses this factory, although its callback
never reads the input. Factory null therefore still skips that route scan.

`00427510` traverses the independent selector-51 route groups at `005468E0`,
not common-manager lists or arbitrary actor children. Existing
[route custody](ROUTE_RUNTIME.md) supplies tail-insertion group order and
linear member order. Root base flag `0x10` skips the entire group and consumes
no draw. Each examined node reads actor/node XYZ before exactly one shared
`0048F060` draw, then forms DWORD-wrapped squared distance plus low draw WORD.
Unsigned strict improvement updates the best metric; only a nonzero metric
updates the selected pointer. A zero metric consequently fixes best at zero
while retaining a previous selected node, if any. Group policy bit `0x02`
permits ordinary next-node traversal through `00404CA0`; group bit `0x01`
endpoint wrapping and individual child base flags do not affect this scan.
Root lookup and group advance use `004050E0`/`00405120`. No allocation or
virtual callback mutates lists during this traversal, and the original RNG
changes only the shared RNG DWORD. Publication follows the full scan through
the object-position publisher at common +1FC, with score recomputed from the
current actor radius. A null referent bypasses native score inspection and
retains the shared publisher's old-null stale-score policy.

Selector 12 independently owns +26C/+274/+27C/+284 resources and their
+270/+278/+280/+288 scores, apart from the common six pairs. Constructor
null/zero writes and +10C source preservation are retained. The shared
[selector12 destruction prefix](../../crates/inf-game/src/object_runtime/actor/derived/selector12/destruction.rs)
owns `004671E0`'s active descriptor exit under its installed `00527058`
vtable: signed negative +184 bypasses dispatch unchanged; otherwise actual
+140 presence selects `0053A098`/`0053A0B8`, and descriptor +08 receives
the actor and false. All valid table entries are owned. Eight unique callbacks
return without entry effects; `004692F0` forwards to `00467760`, whose false
arm clears +29C. The callback's normal return precedes +184 = -1. No update,
target allocation, RNG or entry particle service is required for these exits.
Unknown descriptor indices stop before clearing +184 or releasing resources.
The destructor then deletes +284/+27C/+274/+26C before
probe `00429E70`, common helper/source targets and current/behavior owners.
Released raw-slot custody stops at the reached virtual delete; preceding
higher resources retain `Released`, lower resources and actor registration
survive. Only completed lower destruction resets the four logical custody
markers to `Published`/null in a retired snapshot, matching the common six;
that retired ObjectId cannot be consumed through the live arena. Safely
blocked retry remains a portable custody guarantee, not an
exact native second-destructor claim.

check_selector12_destruction.py
executes the unchanged destructor, singleton initializers, wrappers and actual
class exits for both descriptor tables, negative indices and a stopped target
delete. Its 22 [native receipts](../../crates/inf-game/tests/fixtures/selector12_destruction_native.json)
retain the false argument, callback-before-clear and reverse release order.
Target deleting virtuals are supplied effects and lower `00429E70` is a stop
boundary; the shared arena tests separately exercise actual resource release,
registration and failure-prefix custody. This establishes selector12 cleanup
across missions, not descriptor entry/update or cinematic acceptance.

check_family_target_producers.py
executes the unchanged literal wrapper, factory, player registry lookup,
route helpers, producers, score root/RNG and shared publisher/resource
lifecycle. Its [43 receipts](../../crates/inf-game/tests/fixtures/family_target_producers_native.json)
are compared by [concrete wrapper integration tests](../../crates/inf-game/src/object_runtime/actor/derived/target_producers/tests.rs).
Only allocation/free are supplied host effects. Cases cover D4-only lookup,
stale/empty players, unsigned range boundaries, signed-root inputs, linear
group policy, root/child flag asymmetry, zero-metric retention, exact RNG,
factory null, target null and stopped allocator prefixes. They establish
these bounded owners with supplied state, not full descriptor selection,
allocator +220/+230, enclosing +24 attachment or intro retail acceptance.

## Enclosing actor update prefixes

[update_prefix.rs](../../crates/inf-game/src/object_runtime/actor/derived/update_prefix.rs)
owns the following bounded sequence. The concrete final class selects the
policy; the request does not permit substituting a selector. Required
`FamilyUpdateHost` effects receive the live mutable arena after preceding
writes. `FamilyUpdateProgress::BeforeBehaviorDispatch` identifies the next
native call, rather than claiming that the complete +24 update ran.

| Selector | Prefix before +144 | Nonzero +144 | Zero +144, through the next +D8 call |
| --- | --- | --- | --- |
| 10, `00424830` | Water `00429D60`, status `0047C990` | Negative returns; positive decrements/clears, runs `0047B1E0(delta,0x1F)`, returns | Decay +17A; nonnull +140 calls `0048D030`; virtual +D8 |
| 12, `004679F0` | Water `00429D60` | Same delay policy | Status `0047C990`; nonnull +140 calls `0048D030`; virtual +D8 |
| 36, `00481710` | Water `00429D60` | Same delay policy | Status `0047C990`; virtual +D8; +17A and weapon processing occur later |
| 35, `0043E6D0` | No water or status call | Same delay policy | Status `0047C990`; nonnull +140 calls `0048D030`; virtual +D8 |

The signed DWORD delay compares against signed delta. A positive initial delay
returns after physics even when this call clears it to zero. The next tick
enters the zero-delay tail. Selector 10 reads delay after the synchronous
status callback; the other three have already passed their guard when status
runs. Fields are reread after callbacks. No remaining target/motion tail is
executed by this prefix.

Selector 10's +17A WORD decay skips an already-zero value; otherwise it
subtracts `delta >> 10` with WORD wrapping and clears a negative signed result
to zero. Selector 36 performs that operation after +D8, so the bounded prefix
leaves it untouched.

### Retained weapon timers

The shared [weapon timer phase](../../crates/inf-game/src/object_runtime/actor/behavior/tick.rs)
owns `0048D030` and its `0048D770` helper on the actual retained actor+140
behavior. It clears group flag2 before walking entries. Group count+0D and
entry counts+0E/+0F are signed BYTEs; the entries remain flat32-byte backing,
including a supplied group count that reaches an adjacent entry. Checked
backing errors preserve every earlier flag and timer store.

Entry DWORDs at+04 onward and+14 use unsigned greater-or-equal decay;
+1C uses unsigned greater decay. The behavior's separate DWORD38 comparison
is signed against raw microseconds. Expiry clears38 before `0048D000`.
Construction initializes chain+00 and audio+28 to NULL. The
[shared firing owner](WEAPON_FIRING_RUNTIME.md) can now populate both. A NULL
audio expiry returns without changing2C; nonnull expiry stops with typed
`RequiredSoundStop` after clearing38 until the timer caller lends the actual
voice teardown service. Initial intro shots use one-shot audio and leave28 NULL.
The vehicle's update and its dying tick lend it: they delete the record
through their `00487FE0` and clear `+28` and `+2C`. The dying tick's world
host has no voice teardown, so a loop that is still voiced stops there.

`NativeFamilyUpdateHost` forwards `WeaponTick` to this intrinsic phase with
the actual receiver and raw delta. It does not allocate targets, consume RNG
or initialize family carries. Enclosing virtual+D8 and class tails retain
their own boundaries. [Focused tests](../../crates/inf-game/src/object_runtime/actor/behavior/tick/tests.rs)
guard all four original bodies, signed/unsigned boundaries, flat backing,
constructor-NULL audio and retained error prefixes; consumed host tests reach
the next genuine dispatcher boundary.

### Selector10 post-D8 continuation

The [class10 target/control owner](../../crates/inf-game/src/object_runtime/actor/derived/selector10_tail.rs)
starts at `004248DD` only after normal shared virtual `+D8` return.
Its prefix has already decayed `+17A` and run the reached weapon timer;
the tail repeats neither. Captured targets are refreshed and written back
before fresh pointer/control reads. A genuine NULL148 supplies `[0,0]`
and bypasses shared avoidance; the subsequent motion callback still runs.

Normal control return feeds [actual `00425400` motion](ACTOR_MOVEMENT_TAILS.md)
and then the distinct [class10 record suffix](AUDIO_RUNTIME.md).
Those phases lend current terrain, header, process, clock, shared basis,
target heap, RNG and sound owners. Motion calls the installed helper through
its actual mode/count gates, including mode1; it does not infer an empty
helper from a class label. Class10 retains its required native-address contact
policy rather than borrowing class36's bounded stack policy.

The [consumed adapter](../../crates/inf-game/src/session_platform/intro/runtime.rs)
distinguishes target/control, motion/helper and record failures. Each stopped
actor retains the reached field, target, cache, query and sound prefixes;
only normal callback completion remains eligible on later visits. Unknown
fields, shared-vector modes and extended helper services stay typed
requirements. [Shared firing](WEAPON_FIRING_RUNTIME.md) now owns the reached
direction/queue call and later draw-time drain, with explicit emission limits. [Tail cases](../../crates/inf-game/src/object_runtime/actor/derived/selector10_tail/tests.rs)
and [full native guards](../../crates/inf-game/src/object_runtime/actor/derived/selector10_tail/native_guards.rs)
bound supplied-state ordering; the movement/audio authorities own their
formulas and remaining dependencies. Source closure is not complete intro
or retail motion acceptance.

### Selector36 post-D8 prefix

[`tick_selector36_tail_prefix`](../../crates/inf-game/src/object_runtime/actor/derived/selector36_tail_prefix.rs)
owns `00481787` through the target/control and counter phases after actual
class36's prefix, status and virtual+D8 return. Nonzero WORD17A subtracts
`microseconds >> 10` with WORD wrapping and a signed clamp, once per visit.
The actual typed class and final table `00527EE0` select this owner.

The [target/control phase](../../crates/inf-game/src/object_runtime/actor/derived/selector36_tail_prefix/target.rs)
refreshes captured nonnull148 and writes its result back to the same retained
heap allocation before propagating an error. It then rereads148, publishes
full status150 and consumes direct focus08 plus actorXYZ. Signed WORD
displacements, wrapping DWORD squares and the original bit31 square-root
repair produce distance. Distance0 joins the native counter lane without
secondary geometry,14C or208 reads.

Nonzero distance enters complete `00481260` before secondary target work.
Its actual range/22C gate selects the shared weapon-position service below.
Yaw publishes only WORD268; pitch then captures the retained shared547590
basis and publishes only WORD26E, preserving neighboring packed halves.
Payload,27E/RNG, child and fire gates retain their native order. Reached
`004824A0` uses the [shared child factory and model transition](SHARED_ACTOR_FACTORY.md#dynamic-child-creation-and-selector36-model-transition),
then continues in the same visit without repeating the aim prefix. The shared `0048D340/0048D5A0`
producers now admit retained firing nodes before counters and WeaponTick;
class36 passes update-`+20` false. Later draw-time emission owns the shots. The borrowed shared-basis read covers the admitted
nonmutating target/math callback domain; a later recovered process-basis writer
would require a fresh lazy service at that phase.

A fresh secondary154 refreshes before158 publication and required15C. Normal
release precedes NULL154; failed release retains the field and reached heap
prefix. With no secondary, fresh primary geometry publishes14C before inverse
basis and signed208. Forward uses wrapping `(localZ - signed208)/2` with signed
truncation; Selector12's negative-spacing branch and class10's behind-Z repair
do not substitute. Signed turn division retains its own zero/fault gates.

The NULL148 and primary-distance0 lane logically halves WORD268 and WORD26E,
preserving their neighbors. All normal control lanes then execute `00481180`.
Signed velocity WORDs8E/90/92 project onto basisDWORD60/64/68 in
`(Z*Zcoef + Y*Ycoef) + X*Xcoef` order through the original CRT FTOL. The signed
low-DWORD product with the process multiplier truncates toward zero on division
by300. Fresh signedDWORD98 supplies the wrapping product and SAR15.
Independent wrapping WORD270/272 additions publish linear-minus-angular and
linear-plus-angular counts; carry never propagates across the packed halves.
Actual model getters5/6 read these retained words.

`ProcessActorMotionState` acquires authored file-backed DWORD0053BDB4 at startup
and survives world replacement. It is separate from raw tick microseconds;
no callback reparses the executable or substitutes delta. A recovered later
writer must update this owner.

Fresh nonnull140 after counter publication now executes the actual retained
`0048D030` WeaponTick. Only its normal return permits the fresh148 reread and
shared avoidance, using the [installed234 allocation](FAMILY4236D0_RUNTIME.md#captured-actor-cache-allocation).
A reached required probe/query or backing failure preserves all earlier writes.
Normal completion returns only `Motion { controls }` at actual class36
+118/`00481C10`. Production consumes it through
[shared terrain contact](ACTOR_CONTACT_RUNTIME.md) and
[complete class36 motion](ACTOR_MOVEMENT_TAILS.md#selector36-contact-alignment-and-motion),
then executes the [shared `00481B10` persistent record owner](AUDIO_RUNTIME.md#class36-persistent-actor-sound)
through the actual camera factory and process sound list. Playback remains a
separate required voice operation at world draw.
Reached errors retain the prefix and stop that actor feature without rerunning
earlier work or claiming complete virtual+24.

The [original counter cases](../../crates/inf-game/src/object_runtime/actor/derived/selector36_tail_prefix/tests.rs),
[target/control cases](../../crates/inf-game/src/object_runtime/actor/derived/selector36_tail_prefix/target_tests.rs)
and [complete native guards](../../crates/inf-game/src/object_runtime/actor/derived/selector36_tail_prefix/native_guards.rs)
define bounded supplied-state/source verification. Their presence alone does
not establish execution, loaded actor progression, motion/audio completion or
retail intro acceptance.

The 650-iteration paired authored route, with fresh settings, an empty
portable profile and no actor override, retains normal class36 Motion/audio
callback returns for placements51/78. Those joins establish terrain contact,
alignment, motors, physics and the record callback on that controlled route.
Shared persistent PCM playback now creates and updates voices through the
same SDL owner; supplied-state cases prove looping and synchronous deletion.

The normal 650-frame SDL route uses 125,000 microseconds per step, fresh
settings/profile storage and no actor override. Its selected
[`FreshHeapZeroExtendedCarryApproximation`](../../crates/inf-game/src/fresh_heap.rs)
supplies fresh derived DWORDs `+14C`/`+150`, helper DWORD `+00`, family
DWORDs `+18C`/`+19C`, Selector11 DWORD `+19C` and Selector30 DWORD `+158`
at allocation (evidence);
this models backing without proving native initialization. Later writers own
replacement values; equal-helper DWORD `+04` remains a required read.

| Placements on this portable route | Current reached boundary |
| --- | --- |
| 67/68 (class10) and 51/78 (class36) | Shared firing `0048D340` is admitted; a dying class36 creates four [selector 22](SHARED_ACTOR_FACTORY.md#selector-22-fading-visual) children; later movement/helper/contact and particle lifecycle limits remain |
| Class35 refs 60h/61h (slots 96/97) | Roam then follow-trail; the [world contact pass](WORLD_CONTACT.md) now holds them on the terrain, so the tail writes `+260` before the route update reads it ([evidence](SELECTOR35_RUNTIME.md#remaining-boundaries)) |
| Other intro class35 actors (slots 121..125) | Dispatch, escort scoring and [Herding](ACTOR_DESCRIPTOR_CHANGES.md#shared-herding-change-and-update) run |
| 6/21 (selector34 `pership`) | Prefix, `+D8 00435050`, periodic scan, `+110` bias, escort scoring, Herding, class Attack `00433FB0/00433CC0`, the class tail and, once shot down, Dying `00434130/004341A0` and the crash `00434ED0` run, with the selector67 ribbon and its draw |
| 47/48/49/50/82/83 (Selector12) | Prior `+14C` and `00435170` stops are not recorded; this does not establish complete callbacks |
| Respawned/dynamic Selector12 | Attack update `+F4` reads the zero `+150` carry, as both native spawned Selector12s did |

The selector35 timer receipt
disproves a universal zero initializer, so the zero above is a labeled
approximation chosen on 2026-10-06. Native ref60 first reads zero, while
ref61 first reads positive `00746665` with descriptor index `-1`; the latter
therefore retains that index while subtracting the signed delta. Both values
already exist at their respective `274h` allocation returns. Ref61's bytes
are the retained suffix of the H0 string `CPU Left`, copied before a later
small-block allocation reuses the region. A faithful repair belongs to shared
allocation and resource byte custody. Neither this captured word nor a
selector-specific sentinel is a runtime initialization rule. Until that owner
exists, `FreshHeapZeroExtendedCarryApproximation` supplies zero
(`ActorAllocationDwordCarry::Family19c`). Ref50/ref60 match. Ref61 selects
about 7.5 s early, right after its 1.5 s `+144` spawn delay. Strict arenas
still report the missing field.

Portable frame counts do not establish active common/class35 `+124` callbacks
or a class10 native-address contact return. The current production smoke and
acceptance boundary live in [INTRO_SEQUENCE.md](INTRO_SEQUENCE.md#verification-and-acceptance-boundary).
Matched retail timing, all contact/actor/effect paths and complete intro
acceptance remain open.

### Shared selected weapon position

[behavior/position.rs](../../crates/inf-game/src/object_runtime/actor/behavior/position.rs)
owns `0048D6E0` and the lazy group publication `0048CC00` on the actor's actual
retained140 behavior. The explicit actor-position request represents native
LOWBYTE flag0 and bypasses behavior backing. Selected-weapon-group request
represents the nonzero flag: signed BYTE10 selects an actual flat entry;
entry DWORD18 then selects the group. Missing behavior, entry, group or WORD
fails at that reached read. Rendered model/descriptor presence does not
initialize coordinates.

The group's sole optional WORD backing00..0A retains relative00/02/04 and
world06/08/0A. Cached-world mask0x02 bypasses relative/basis/actorXYZ reads and
returns actual known world words. With that mask clear, relative mask0x01
captures all local words before the exact `004909E0` transform. Transformed
world outputs are stored before fresh actorXYZ additions. The absolute path
copies actorXYZ, using the represented whole-XYZ admission at its first X read.
Only normal publication sets mask0x02; an error retains reached output words
and leaves the mask clear. WeaponTick invalidates this same cached-world mask.

The existing exact WORD transform was moved to
[`NativeBasis::transform_words`](../../crates/inf-formats/src/native_basis.rs)
and is shared by both existing Selector16 pose callers and this behavior owner.
It preserves `(X product + Y product) + Z`, nearest-even QWORD FISTP and low-WORD
outputs. The distinct DWORD transform's Z/Y/X accumulation does not substitute.
[Focused position cases](../../crates/inf-game/src/object_runtime/actor/behavior/position/tests.rs)
and [native guards](../../crates/inf-game/src/object_runtime/actor/behavior/position/native_guards.rs)
cover cached/relative/absolute/error-prefix domains; active downstream group,
effect, fire and audio services retain their own boundaries.

### Selector77 NULL-target update

[`tick_selector77`](../../crates/inf-game/src/object_runtime/actor/derived/selector77/update.rs)
owns `00407CA0`, idle `00407B00`, steering `004081B0`, extension `004080C0`
and the live `00407760` target controls. The shared Flag1 adapter admits actual class77 at
ordinary+24 visits in any world; `80000000/+3C` stays separate. It borrows
current process0054E894, status load factor, raw microseconds, executable sine
data and the same retained RNG.

The process-null comparison precedes status: unequal+1B0 requires virtual+B4
before any status write. Shared status then precedes the signed+C4<=2 return.
Unsigned DWORD198 equality subtracts to zero; expiry reaches `00407600`.
That phase requires release of an existing target before reading the first
player's primary+D4; no-player and far-player returns own their actual RNG
and timer stores. Near-player allocation now uses the shared retained heap, preserving allocation
null and incomplete-constructor prefixes.

After the timer phase, fresh nonnull140 executes the shared weapon timers.
The update captures148, publishes1AC=microseconds>>6, then preserves released
target custody, refreshes target+18 and runs `00407760`. Only a NULL target reaches
idle steering and+D4. The latter advances WORD19C by microseconds>>5 and
writes sine-derived188. It selects extension target/speed with two actual RNG
draws, then moves WORD190 on later visits; it assigns no captured pose.

Strict construction leaves WORD19C and DWORD1A0 unknown. The explicit
[allocation policy](../../crates/inf-game/src/fresh_heap.rs) supplies them
only in the selected production approximation;19E remains unknown and1AC has
its reached update writer. Required reads, service errors and RNG failures
retain earlier mutations. The adapter logs and stops only that actor until
world replacement. [Kernel tests](../../crates/inf-game/src/object_runtime/actor/derived/selector77/update/tests.rs)
pin widths/order and complete native guards;
[loaded dispatch tests](../../crates/inf-game/src/session_platform/intro/runtime/tests/selector77_tests.rs)
pin class admission and stopped custody. A loaded positive-delta schedule
advances actual190, reads it through packed getter6, then crosses the retained
timer and executes the real first-player far-distance return. It keeps actor
XYZ/basis, player, mission, target and process RNG custody; scheduling is
isolated and uses the selected allocation approximation.

The separate [target control](../../crates/inf-game/src/object_runtime/actor/derived/selector77/update/target.rs)
uses the saved eye origin and current target focus, publishes distance, then
applies the selected weapon's velocity lead, ballistic and spread policies.
It preserves the original basis/steering WORDs and direct shot admission at
`00407D59`, including signed entry selection and the retained queue callback.
The tracked generator reproduces 38 native
control cases; the production Follow-target test uses the actual target,
behavior table and firing queue. These checks preserve the accepted eye
appearance but do not establish the complete sequence. Exact heap history,
camera timing and matched visual/audio acceptance remain open.

### Production activation-delay service

[`NativeFamilyUpdateHost`](../../crates/inf-game/src/object_runtime/actor/derived/family_update_services.rs)
shares the current process RNG and executable sine table. Its delayed-physics
effect forwards the original microsecond DWORD and low-BYTE mask to the
existing `0047B1E0` owner after the prefix's `+144` write. Its weapon-timer
phase uses the intrinsic retained owner above. The live
[WorldFamilyUpdateHost](../../crates/inf-game/src/session_platform/world_actor_update.rs)
composes these services with actual timed damage, trail births and retirement,
borrowing the same mission, task, terrain, particle, sound, process and RNG owners.
It selects no mission, authored reference, actor model or captured timer.

The [production callback adapter](../../crates/inf-game/src/session_platform/intro/runtime.rs)
admits the four concrete prefix classes at ordinary virtual `+24` visits in
any loaded world, independently of intro-controller or constructor-93 presence.
`004875A0` calls the shared `00478F20` Flag1 pass before the separate camera
phase; neither native caller gates that pass on an intro attachment. Unsupported
ordinary `+24` callbacks are skipped and logged once per actor. Flag `80000000`
still selects unowned virtual `+3C` and bypasses the prefix.
The native node transfer precedes callback admission, so controller membership
changes may admit later actors in the same pass. A `Returned` prefix remains
eligible on later visits; positive delay expiry still runs physics and returns
before zero-delay work. `BeforeBehaviorDispatch` now reaches the genuine
shared `+D8` dispatcher through the production host. A required-service error
preserves reached writes, records the actor's stopped feature and logs its
reason once. After successful dispatch, Selector12 enters its distinct
post-D8 tail, Selector10 executes its
[target/control, motion and record continuation](#selector10-post-d8-continuation),
and Selector36 uses its [own continuation](#selector36-post-d8-prefix).
Selector35 has its [distinct target, clearance, motion and audio continuation](SELECTOR35_RUNTIME.md).
A normal class callback remains eligible; a reached failure retains its exact
typed phase. World replacement clears stop custody, and a stopped continuation
does not count as completed `+24` execution.

Selector 35's negative/positive delay arms need neither water `+230` nor
periodic `+220` and therefore execute in production. Strict water-dependent fixtures
still fail at their real unknown `+230` read before the delay gate. Selected
production allocation supplies a retained carry; later water/status/dispatch
phases keep their own boundaries. Zero-delay dispatch now borrows the actual
process/world owners; later motion tails remain separate. Reached weapon
timers execute through the shared retained phase.
Connection of this bounded prefix is not complete cinematic acceptance.
The [service tests](../../crates/inf-game/src/object_runtime/actor/derived/family_update_services/tests.rs)
check physics forwarding and partial errors; the [runtime corpus tests](../../crates/inf-game/src/session_platform/intro/runtime/tests/family_prefix_tests.rs)
use authored ordinary-world actors and controlled native-list schedules to pin
controller-independent dispatch, same-pass stage-9 activation, delay expiry,
alternate `+3C`, selected timer carries and stopped-feature custody. The class lookup
and prefix APIs are shared actor policies, not intro-specific entry points.

### Production behavior services

[world_actor_behavior.rs](../../crates/inf-game/src/session_platform/world_actor_behavior.rs)
composes the real dispatcher with retained process descriptor bytes, task
owner, particle factory state, process RNG and fresh animation-clock getter.
Display or screen replacement does not recreate the mutable behavior tables.
Periodic factories and target resources remain in the actual arena heap.
Common/selector-12 inline and actor-list selection borrow those live owners
through the existing particle/predicate adapter. Player/route producers and
[selector-12 grant grid](FAMILY_GRANT_GRID.md) retain their distinct policies.

Descriptor score, point changes, Dormant, StayStill, Home's empty update,
Roam and common Attack use their existing intrinsic owners. Selector35's
[distinct descriptor virtuals](SELECTOR35_RUNTIME.md#construction-and-descriptor-selection)
also have intrinsic owners, alongside its distinct dying/effect callbacks.
Current H9 registration gates sound position reads; the existing retained
sound queue defers playback to its world draw owner.

Actor-resource `00460310` receives a genuine nonnull eight-byte scratch source
whose bytes remain unwritten. `00479D40` copies the first six into the new
object's position, which the port keeps unknown; the
[resource classes](ACTOR_RUNTIME.md#behavior-registry-resource-classes)
show that nothing reads it before the object is destroyed. The session lends
the world factory to the vehicle's aim, the family target selections and the
damage reactions; a path without that loan keeps the explicit boundary.
Network/impact services, remaining nonzero score sources and enclosing class
tails remain required. Per-actor stops prevent replay of a partial prefix
while the rest of the phase proceeds.

### Water and allocation history

`00429D60` resolves H12 and consumes its signed DWORD +00 plus process
`005483BC`. The request retains those two inputs separately. The actor's Y
and the shifted plane are signed WORDs; size +28 is unsigned.

```text
plane_y = signed WORD(height_fixed >> (coordinate_shift & 31))
separation = signed WORD(actor.Y) - plane_y
deeply_submerged = separation < 0 && separation < -(unsigned WORD(size) / 2)
```

Every geometry branch then reads WORD +230. `00429E20` writes +234 and +238,
leaving +230 unwritten; the existing constructors, payloads, commands and
after-load methods for selectors 10, 12 and 36 do not establish it. The
portable field is `Option<u16>`: strict construction leaves it unknown; the
explicit selected allocation approximation supplies its retained initial value. Above the threshold,
native code tests the WORD for zero and otherwise subtracts `delta >> 9`
with WORD wrapping. There is no floor at zero, allocator-independent reset,
or above-water branch that permits discarding the prior value. Even a zero
delta must perform the read. For example, a retained value 1 with delta 1024
becomes 65535; a later deep-water tick can therefore reach damage immediately.

Under the strict half-size threshold, +230 <= 4000 adds `delta >> 10` with
WORD wrapping. A value above 4000 builds three packet bytes from maximum
health WORD +A2 and channel byte 5, then calls `0047C7C0(packet,process_null,
delta,4000000)`. That path does not reset +230. The request exposes only the
three written packet bytes, not a fabricated fourth byte from the native
stack. The shared [timed damage owner](ACTOR_RUNTIME.md#shared-timed-actor-damage)
executes the real resistance calculation and conditional virtual +B4 through
the live production damage services. The source getter reads current process
`0054E894` at `00429DBF`, after the water/timer and maximum-health reads;
missing storage fails there. The registry null handle never substitutes for it.
An unknown +230 fails at its reached read, before any timer write or damage.
Maximum health is required only by the actual damage branch.

Selector 12's `00460645` allocation requests `0x2A0` bytes through CRT
`00511708` → `0051247C` → `005124A8`. The small-heap path `00515BFC` rounds
this to a physical `0x2B0` chunk and searches shared bin 42 and larger eligible
free chunks; splitting and reuse are not selector-specific. It writes chunk
headers/footer without clearing the payload. The fallback calls `HeapAlloc`
with flags zero. `00515FB6` commits new pages, but the accepted
first-timer receipt
does not tie the observed allocation to a virgin page or exclude intervening
use by other process allocations. Its untouched zero +220/+230 values establish
custody only for authored ref 48's recorded allocation, not defaults for other
actors. Request size alone cannot establish their initial values; the portable
strict fields remain unknown until a source owner or the explicit labeled
allocation policy supplies them.

The expanded indexed history from recording start `31:0` to the same frozen
first-cursor endpoint adds four earlier BYTE reads per WORD, at `0050A2D6`
and `004F8D51`, before this actor's allocation. Both instructions read an
earlier receiver's indexed `+34` payload; they are not timer initializers.
There is no earlier CPU writer in the expanded interval. This excludes an
unseen recorded CPU assignment to those bytes, but does not establish page
initialization, allocation reuse custody or a portable zero policy. The
linked ledger owns exact positions, tracked query and decoder validation.

### Shared status arithmetic

`0047C990` first decays nonzero +C8 by `delta >> 5`, flooring at zero. If +CE
and +CC are both zero, it returns without effects. Otherwise it calls
the owned `0047DFD0` phase described below. After that phase returns, the
arithmetic rereads status fields, including mutations from a reached
synchronous trail-birth callback. A skipped emission still continues status
arithmetic.

With nonzero +CE, native code captures unsigned strength +CC and computes:

```text
product = DWORD wrapping((delta >> 5) * strength)
amount = sign-extended WORD(signed product >> 15)
if amount == 0: amount = 1
else if amount > unsigned WORD(+CE): amount = unsigned WORD(+CE)
+CE = WORD wrapping(+CE - amount)
```

A negative result is deliberately not clamped: its subtraction can increase
the remaining WORD and health. If +CE reaches zero, +CC becomes 2000 when
captured strength exceeds maximum health, otherwise
`strength * 2000 / maximum_health`. The division-by-zero native trap is a
typed error after the reached +CE write. Missing maximum health also fails
at that read. Amount below unsigned health subtracts with WORD wrapping;
otherwise the required `0047CE90` retirement callback runs. With +CE zero,
the post-visual +CC instead decays by `delta >> 10`, flooring at zero.

Focused [prefix tests](../../crates/inf-game/src/object_runtime/actor/derived/update_prefix_tests.rs)
cover allocation unknowns, signed units, exact branch thresholds, wraparound,
class ordering, callback mutation and reached writes on failure. They do not
establish a complete +24 update or runtime cinematic acceptance.

### Status visual phase

[status_visual.rs](../../crates/inf-game/src/object_runtime/actor/derived/update_prefix/status_visual.rs)
implements `0047DFD0` inside the existing `tick_actor_status` path. Native code
captures unsigned size WORD +28, sets dirty bit `0x0400`, then reads process
DWORD `0055F154`. This operand is explicit `status_load_factor: Option<i32>`;
unknown state fails at that reached read, after the dirty write. Its signed
probability is `DWORD wrapping(load_factor * 100) / (size << 14)`, with
division truncating toward zero. Size zero produces a typed native division
error after the preceding +C8 decay and dirty write. Probability below 2
admits without RNG; otherwise one actual `0048F060` draw, masked to its low
WORD, admits only when its remainder modulo probability is zero.

Only an admitted emission copies XYZ, captures newly read half-size and
the original raw delta shifted right by 5. A subsequent draw with low nibble
zero writes +C8 = 15625 and +CA = 55. Current unsigned size selects trail
type 9 through 200, type 10 above it. When both signed X/Z velocities have
magnitude below 300, two further draws jitter the copied X/Z by
`draw % half_size - half_size / 2`, with WORD wrapping. Size 1 can therefore
trap at the first jitter division, after its reached RNG draws and possible
+C8/+CA writes; fast motion bypasses that division. Current X/Z velocities
then subtract their wrapping DWORD product with the captured step, shifted
arithmetically by 15, from the copied position.

The final two draws form signed velocity WORDs: X/Z use truncation toward
zero of `velocity * 3 / 4`, plus `(draw & 127) - 64`; Y uses only the signed
three-quarter value. `FamilyUpdateEffect::StatusTrail` retains the actual
`00489D90(type, copied_position, copied_velocity, 0)` packet. Its required
host uses the existing process `birth_trail` owner, executable, birth
inputs and shared RNG. Retirement uses actual `retire_actor` and the same
production lifecycle host as lethal damage. A host failure leaves preceding RNG and actor writes committed;
a successful callback may mutate the arena before `0047C990` rereads status.

The native checker executes unchanged
`0047C990`, `0047DFD0` and `0048F060` against the authenticated executable.
Its [60 supplied-input receipts](../../crates/inf-game/tests/fixtures/actor_status_visual_native.json)
pin unsigned sizes, signed velocities, wrapping arithmetic, probability
skips, missing inputs, division faults, RNG/host failure prefixes and callback
mutations. `00489D90` and retirement are explicit supplied host boundaries.
[Rust comparisons](../../crates/inf-game/src/object_runtime/actor/derived/update_prefix/status_visual_tests.rs)
also feed type 9/10 packets to the real trail producer and verify its extra
shared draw, scale, constructor flags and pending publication. This bounded
composition does not prove allocator +220/+230 values, trail pixels or a
complete cinematic. Production now attaches these services to the shared family
and selector77 callbacks; focused live-owner tests cover actual trail publication,
timed family reaction and status retirement without substituting scene logic.

## Shared motion and physics

[kinematics.rs](../../crates/inf-game/src/object_runtime/actor/derived/kinematics.rs)
owns `00426860` forward control, `00426760` lateral correction and `00426AE0`
angular control. Requests retain signed control/power DWORDs, actual table
words, and the caller's already-shifted step. `00467DF0` uses `delta >> 5`;
other callers use different shifts or contact-adjusted steps, so the shared
API does not infer that conversion. Linear velocity +8E/+90/+92 consists of
signed WORDs; angular velocity +94/+98/+9C consists of signed DWORDs. Additions
retain their native bit-pattern wrapping.

Selector 12 constructor `00467070` stores these static executable bindings.
The arena verifies the retained binding at each reached consumer. A different
binding is an unsupported feature, not a reason to restore constructor values.
The [kinematics tests](../../crates/inf-game/src/object_runtime/actor/derived/kinematics_tests.rs)
also pin the words in the actual retail executable.

| Actor field | Table | Ordered DWORDs |
| --- | --- | --- |
| +290, forward | `0053A080` | maximum `0x3000`, minimum `-0x1000`, acceleration/deceleration `0x2000` |
| +294, angular | `0053A090` | maximum `0x10000`, response `0x20000` |
| +28C, lateral | `0052FE40` | threshold 10000, rate 1000 |

Forward control divides `wrapping(control << 12)` by unsigned size WORD +28,
clamps the signed result, projects current velocity onto the stored Z basis,
and normalizes that projection by size. The limited signed difference is
scaled by power and size before its world-vector products are added to the
velocity WORDs. Acceleration/deceleration rates are sign-extended low WORDs
after the product's arithmetic shift. The boost divisions and size divisions
retain their exact native trap sites; a zero power does not bypass those reads.

Lateral correction projects onto X. A zero projection returns before table
use; a projection within the power-scaled threshold is removed in full. Outside
the threshold, native code limits the correction by rate and step. Negative
correction negates the product before `SAR 16`, so otherwise matching positive
and negative inputs can produce different magnitudes.

Angular control returns immediately for zero power. Its nonzero path derives
the signed desired turn, reads current local Y through `0047A680` →
`00490870`, and limits the difference; opposing current/difference signs
multiply the rate by six then shift right by two. A null axis transforms the
local `[0,difference,0]` through `0047B4B0` → `0047A6E0` → `004907E0`.
A supplied world axis uses truncating products and direct `0047B470` addition.
The DWORD basis adapters use nearest-even `FISTP QWORD`, while projection and
world-axis products use CRT `00511BA8` truncation. Both retain the low DWORD
and native expression order. These are distinct from `00490900`'s point
transform order. The supported CRT's 53-bit arithmetic is modeled with f64;
stored basis values remain f32.

[physics.rs](../../crates/inf-game/src/object_runtime/actor/derived/physics.rs)
owns `0047B1E0` in its native order. The request supplies the original
microsecond delta, mask byte and actual sine table:

| Mask bit | Ordered operation |
| --- | --- |
| `01` | `0047AD10`: subtract `delta >> 10` from vertical velocity with WORD wrapping |
| `02` | Angular drag: guarded DWORD squared length and short-vector normalization, or the large/overflowed-product branch |
| `04` | Linear drag: signed WORD squared length, native integer square root, signed low-WORD decrement and axis normalization |
| `08` | Position: add `wrapping(signed_velocity * (delta >> 5)) >> 15`, then mark dirty bits `C0` |
| `10` | Nonzero angular vector: form world rotation WORDs, call `0047A790` → `00490FC0`, then mark dirty bit `10` |

Angular drag tests each wrapped square and the wrapped sum before choosing
its short-vector branch. That branch sign-extends the square root's low WORD
and adds two to the high-product decrement; the large branch does neither.
Linear drag tests whether the original magnitude is positive, not whether
the remaining magnitude is positive: a large delta may reverse velocity.
The rotation uses a world vector directly, preserving the existing axis-angle
kernel's stores and counter normalization; no second local-to-world transform
is inserted. A nonzero angular vector with zero delta still reaches its
zero-vector rotation, whereas an all-zero angular vector skips it.

The separate `0047B510` linear and `0047B470` angular impulse APIs publish
ordered wrapping additions without dirty writes. Integration owns dirty
publication. [Physics tests](../../crates/inf-game/src/object_runtime/actor/derived/physics_tests.rs)
cover overflow branches, negative remaining magnitude, world-axis rotation,
and preservation of gravity/drag writes when a later position read fails.

### Selector 12 complete motion consumer

[selector12_motion.rs](../../crates/inf-game/src/object_runtime/actor/derived/selector12_motion.rs)
owns complete virtual +118 `00467DF0`. `Selector12MotionRequest` supplies the
two native control DWORDs, actual process byte `0053A128`, basis math tables,
delta and a required terrain host. The concrete `TerrainRuntime` host uses
the existing native `00450A30` sample at the current actor X/Z. No sampled
height, flat ground, null-target control or process snap value is invented.

For signed lifecycle +C4 >= 3, a nonzero `0053A128` snaps Y to terrain and
sets vertical velocity to `wrapping(-separation << 13) / (delta >> 5)`.
The Y write precedes IDIV `00467E4D`; a delta below 32 therefore preserves
the snap before reporting the trap. A zero byte with nonzero separation
bypasses alignment and all three control tables, but still runs physics
with mask `1E`. Otherwise the consumer samples Z.y, rotates YZ, rereads the
reached X.y and rotates XY, then invokes forward, angular and lateral
consumers in that order at power `FFFF`, followed by physics `1E`.

For signed lifecycle below 3, separation above 10 runs physics `1F`.
Otherwise it snaps to terrain plus 10, bounces negative vertical velocity
through signed division by two, and decays the velocity axes with thresholds
64/24/64 before physics `1E`. Retail `00467F53/F8A/FC1` uses logical `SHR 14`
on the wrapped velocity/step product even for negative velocities. The
[motion tests](../../crates/inf-game/src/object_runtime/actor/derived/selector12_motion_tests.rs)
cover both lifecycle branches, exact thresholds, table-read bypasses,
post-alignment failure custody and the native snap/division ordering.

Success establishes this +118 consumer only. The enclosing +24 still owns
water/status, behavior/target/weapon callbacks and its prior velocity
projection; no production full-update callback is substituted here. The
new movement owners never read, initialize or suppress unknown +230/+220.

## Shared avoidance steering

[avoidance.rs](../../crates/inf-game/src/object_runtime/actor/derived/avoidance.rs)
owns `0042A2C0`, the family consumer after the native `00429FF0` probe scan.
The signed DWORD controls are `[turn, forward]`; zero forward returns false
before a scan or actor-state read. A reached scan remains a required,
synchronous host that rebuilds live +22E. No upper flag returns false without
cooldown decay. Neither result means the enclosing +24 can skip its motion
tail. This owner does not read or initialize allocator WORDs +220/+230.

The six near/far probe bits and three extra bits select the complete native
forward/turn matrix. Two branches set +22E bit 0020. Asymmetric side bits also
move XYZ along the actual virtual +48 local-X basis: capture unsigned size,
multiply by **signed** `delta >> 10`, retain wrapping DWORD products, shift
arithmetically by 11, then sign-extend the resulting low WORD. Each basis
product uses CRT truncation and each coordinate adds with WORD wrap. The
basis callback may mutate live state; position is read afterwards, and direct
XYZ stores deliberately do not set position dirty bits.

Only after displacement does nonzero +238 with neither flag 0040 nor 0200
decay by the **unsigned** `delta >> 10`, floor at zero, set forward to zero
and return true. Otherwise the native matrix adjusts signed control magnitude
and direction, including INT_MIN wrapping, and chooses turn from the current
angular Y DWORD or the preserved turn. Failures retain reached scan/basis
mutations and actor writes. The probe/collision producer remains separate;
a supplied flag mask is never a production scan or an intro-specific policy.

check_family_avoidance.py executes unchanged
`0042A2C0` and `00511BA8` against explicit scan/basis hosts. Its
[receipt](../../crates/inf-game/tests/fixtures/family_avoidance_native.json)
and [comparisons](../../crates/inf-game/src/object_runtime/actor/derived/avoidance/tests.rs)
cover all 512 native flag combinations in both directions, plus 88 threshold,
signed-delta, WORD-wrap, callback-mutation and failure-prefix cases. These
establish the shared consumer only, not complete +24 execution or a retail
cinematic comparison.

## Intro gates and first remaining owners

Selector-88 stage 1 accelerates a helper and has no actor-task gate. Stage 5
tests state 3 Secondary actors' +CC component and +22C. Common Attack entry
`00429B00` selects/clones +1F4 or +1D4 into target +148 and invokes target
virtual +10. Only a resolved actor sets +22C = 1 at `00429BC3`; it also writes
separation +208 from both size WORDs plus 350. Activation flags and +144 expiry
do not set +22C. Stage 7 resets on Secondary lifecycle +C4 = 3 without +22C;
the dispatcher does not invent a death transition. Movement, collision,
weapons/damage and lifecycle callbacks are independent owners. A controller
deadline can advance while missing callbacks change the authored event order.

The enclosing +24 callbacks remain incomplete beyond the bounded prefixes.
Selectors10/12/36 require allocator-carried water WORD230 before their delay
gate; strict fixtures preserve unknown state and selected production arenas
use the explicit allocation approximation. Required physics, visual, weapon and
damage effects preserve their real callback boundaries.

The accepted Primary timer receipt
now follows authored reference 48/native placement 47 from actual allocation
through its first consumers. This one allocation carries +220/+230 = 0/0 at
operator-new return and retains them through construction/load. Its +230 has
83 two-byte reads at `00429DFE`, one per selected +24 entry, with no indexed
CPU writer in the explicit loader-to-first-cursor interval. Its first +220
read at `004295AA` observes zero, then `004295BD` writes one. The observation
ends at `004295C4` before that periodic callback executes. Earlier heap/page
initialization and address reuse remain unowned; the observed values do not
define constructor initialization or a portable fresh-allocation policy.

After those prefixes, initial periodic dispatch reads the unwritten +220
WORD at `004295AA`, after RNG/+21C/+22E writes. Common/subclass constructors
do not initialize it. Strict fixtures preserve the unknown and report the
feature at that read. The separate [allocation policy](../../crates/inf-game/src/fresh_heap.rs)
supplies the represented carry in selected production arenas; allocator
history, target producers/classes, descriptor motion, audio and damage remain
separate owners. No complete+24 callback or automatic cinematic progression
is claimed.

The [Selector12 post-D8 tail](ACTOR_MOVEMENT_TAILS.md) now composes the
native nullable-target protocol, steering, avoidance and virtual `+118`.
A genuine null `+148` sets `[0,0]` before those consumers; it still reaches
terrain alignment and physics. Nonnull targets retain refresh/geometry and
secondary release prefixes. Only a reached miss, firing-service error or
unknown field stops the tail; admitted firing returns into movement. The retained authored process byte
`0053A128` supplies terrain-snap policy independently of steering. Production
calls this tail only after its real prefix and `+D8` return; failures in either
phase never bypass into motion. Selector10 has its
[distinct connected continuation](#selector10-post-d8-continuation), while
Selector36 owns its [target/control and counter phases](#selector36-post-d8-prefix),
shared motion and persistent-record callback. Selector35's
[class owner](SELECTOR35_RUNTIME.md) joins its separate tail, motion and audio.
Voice playback retains its separate shared owner.
The [descriptor target authority](ACTOR_DESCRIPTOR_CHANGES.md) owns Selector12's
virtual `+12C/+138/+14C/+144` changes and `+13C/+148` updates, plus shared
object-target `+E8` and route `+EC` callbacks. They use actual heap/process/RNG
loans. Constructor-unwritten actor DWORD `+14C` remains required in strict and
old four-carry fixtures until a reached writer publishes it. The selected
[extended backing policy](../../crates/inf-game/src/fresh_heap.rs) supplies
fresh `+14C` and helper `+00` at allocation only; helper `+04` stays independent.
Normal change completion still reaches its real update before motion.
Required allocator carries, descriptor/effect services and remaining class
tails bound complete intro progression; one completed visit is not retail acceptance.

### Test-only first-miss provenance

The [test-only observer](../../crates/inf-game/src/session_platform/intro/required_miss_observer.rs)
keeps two explicit portable stages. A genuine stopped Selector12 miss retains
its actual obstacle ordinal/XZ/error before logging/storage. At the actual
shared miss host, a real normal collision-prefix return records temporary
450BD0-snapped XYZ, reached guard and retained query state with no invented
error or ordinal. This inner return precedes 429EE0 restoration and captured-cache
completion. Post-step cache bytes and current allocation-address/epoch/radius
joins are separate read-only observations; snapped collision XZ must not be
attributed as the original cache-request point or completed sample.

Both stages record actual actor/class/H13, required actor-root H8, current H12,
placement and unknown-or-known +14C. The metadata census preserves lazy H11 and
counterpart skip cases, stopping at its first required resource/ACTIVE boundary.
It never runs an auxiliary program or supplies a collision result. The shared
[production entry/scan](FURNITURE_COLLISION.md) supplies actual execution
independently, including the connected [ACTIVE domain](MODEL_PROCESS_GRAPHICS.md).
Excluded operations and dependent contact responses remain typed boundaries;
the metadata census never substitutes for those executions.

The [paired linked producer](../../crates/inf-game/src/session_platform/linked_tests/required_miss_observer.rs)
drains after each unchanged step and compares actual post-step generation,
residency revision and process radius before attributing a snapshot to current
owners. Changed tuples remain in the raw receipt and fail the join assertion.
The producer publishes context before the scheduler's first-pass session tick,
where actor callbacks execute, and requires packet iteration +1 to match the
completed presentation count. The accepted stopped-callback version of the
650-iteration authored New Game producer (125,000 microseconds per step, fresh
settings and an empty portable profile sidecar) passed armed/unarmed parity
across three renderer resource epochs. That stopped-only receipt remains
historical source provenance. The current two-stage producer passed all six
armed/unarmed parity checks across three epochs on its actual loaded route.
It records ten inner normal collision-prefix returns: Selector12 placements
47/48/49/50/82/83 in iteration353, class10 placements67/68 in374, and class36
placements51/78 in385/405. No stopped-callback packet is emitted. Those inner
returns precede restoration, cache completion and later motion; the absence
of a stopped packet does not prove native contact, all query paths or complete
intro behavior.

The historical two stopped requests, placements47/82, occur in iteration353
at 44,250,000 microseconds. Both use current H8 model223, radius112 and retained process
radius512: their wrapped query radius is624. Each initial census reaches raw
H12 cell35590 (`0x8B06`) without sparse-record mask `0x0400`, then reads the
same [retained fallback owner](TERRAIN_RUNTIME.md#retained-furniture-query-process-state)
`005483C4` as furniture effects. A genuine zero lookup skips H11/counterpart
reads; the census still cannot execute an active auxiliary query. Their observed portable
+14C values8193/8192 are existing writes, never allocator defaults. This is a
bounded portable snapshot and read-only census; no native query, collision
result, runtime admission or complete-intro acceptance follows from it.

Before connected ACTIVE execution, placements47/82 returned normally in iteration353
with snapped collision centers `[17600, 45120]` and `[16576, 45248]`. Class36
placements51/78 return normally in iterations385/405 at `[22336, 47168]` and
`[22976, 47936]`. Temporary guard8000 is still present at each inner return;
429EE0 subsequently restores XYZ and clears the guard. Post-step XYZ is a later
observation, including class36's newly consumed Motion writes; it is not the
restoration sample. Read-only samples at
these snapped centers are not the original cache-request completion samples.
On that historical route, placement82 later reached a genuine ACTIVE candidate
in iteration401: actor
H8 model223, counterpart model51. Its original obstacle request is `[15225, 45584]`,
while actual temporary collision XYZ is `[15168, 192, 45632]`. The retained
process prefix and guard survive the required pair-context error. The stopped
metadata census at the original request does not describe that snapped scan.
Placement78 subsequently preserves the published B basis, alias and sparse-record
history through its own normal scan. No active contact result follows.

The full-frame paired producer is an explicit release-mode diagnostic; its
native source guards remain in the ordinary suite. Run it with
`cargo test --release -p inf-game --lib required_miss_observer -- --include-ignored --nocapture`
from the Infestation workspace.

Authority is bulk/game_logic.c plus the retail instructions
at these addresses; several +24 routines are absent from the decompile.
The accepted capture ledger
now pins one actor's first cursor writer and water-read cadence through that dispatch;
descriptor callbacks and complete enclosing updates remain outside that
receipt. No new recording was acquired. Focused tests establish policy and
partial custody, not cinematic or gameplay acceptance.
