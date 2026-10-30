# Infestation world instance commands

This document owns the Handler-14 placement suffix and its bounded shared
and specialized command views. [WORLD_ASSETS.md](WORLD_ASSETS.md) owns placement prefixes;
[CONSTRUCTOR_ASSETS.md](CONSTRUCTOR_ASSETS.md) owns Handler-13 class inputs.
The canonical decoders are
[instance_assets.rs](../../crates/inf-formats/src/instance_assets.rs) and
[instance_command_views.rs](../../crates/inf-formats/src/instance_command_views.rs).
[Corpus checks](../../crates/inf-formats/tests/instance_assets_corpus.rs)
cover all installed worlds and every audited virtual-method profile.

## Source authority and constructor path

The source authority is the supported original `Infest.exe`, its
game-logic decompilation, and original OVL bytes.
Ghidra omits arguments and widens some loads; native disassembly determines
the contracts below. No runtime recording is required to establish the
authored graph or these bounded input reads.

`0048DC30` passes the placement suffix address, **placement +0x10**, in EDX
to `00460310`. The latter saves EDX at `00460317` and forwards it as the
first stack argument to `00460120` at `00460388..00460393`.
`00460120` separately constructs its four-DWORD context for class virtual
`+0C`: position pointer, rotation pointer, pointer to the H13 header's model
argument, and placement index. The instance suffix is **not** class payload
or part of that context.

After virtual `+0C` returns, `004601D3..004601DA` passes the saved suffix to
`004602D0`; that function forwards it to **`00479E10`**. The latter stores
the low WORD of the H13 resource argument at object `+0x30` and invokes
virtual `+0x38`. Only a successful result permits the instance-command walk.
Each command then invokes virtual **`+4`**; a false result aborts the walk.
The asset parser describes the authored graph without claiming that these
guards passed or any command executed.

## Class-independent suffix graph

`00479E10..00479E6A` establishes this layout:

| Location | Representation | Consumer |
| --- | --- | --- |
| Suffix `+0` | signed DWORD count | `00479E35..00479E37` skips the walk for values at most zero |
| Suffix `+4` | inline array of allocation-relative DWORD pointers | The loader relocates each walked pointer; `00479E3C` loads it |
| Pointed command `+0` | DWORD command code | Passed as the first virtual `+4` argument; shared consumers use signed comparisons |
| Pointed command `+4` | start of command payload | Address passed as the second virtual `+4` argument; complete extent is unproven |

The H14 loader applies the native internal relocation table to the main
allocation. Every walked command pointer in the installed corpus is present
in that table. The parser requires the same obligation, records the exact
pointer-word span, and preserves the original target offset rather than
constructing host pointers. A target of zero denotes the allocation base
after relocation; it is not automatically null.

`parse_world_instance_bank` returns an `InstanceAssetBank` containing the
unchanged `WorldBank` as `worlds` and parallel `instances` in local world
registration order. Each `WorldInstanceAssets` contains placement views in
authored array order. Each `PlacementInstanceAssets` retains its placement
index, allocation-relative suffix offset, signed count and source span,
pointer-array span, and ordered commands. Each `InstanceCommand` retains
its local command index, pointer span, allocation-relative command and
payload offsets, and command code with its source span.

Aliased world, placement, and command pointers remain separate ordered
references. No record extent comes from the next pointer, and gaps or
unknown payload bytes remain available through the complete main-allocation
span and framed source. Nonpositive counts retain their original signed
value and yield an empty command list. Positive counts and every pointed
command header are bounded before allocating or reading the corresponding
list. Relocation-tail bytes cannot satisfy a short main-allocation record.

All **33 worlds** and **4,486 placement suffixes** parse. Their lists contain
**11,476 commands**, with codes 1 through 32. There are 246 zero-count
suffixes; the largest count is eight. All installed command targets are
distinct within each world, while synthetic tests explicitly exercise
aliases. These are graph-coverage counts, not counts of decoded command
behavior.

## Explicit constructor context for payload views

`decode_instance_command` takes an `InstanceCommandRequest` with the original
source, main-allocation span, the canonical H14 relocation records,
allocation-relative command offset, an explicit `InstanceCommandTarget`, and
`InstanceCommandReadScope`. `Inspection` decodes bounded authored values;
`Dispatch` defers selector-60 retained-table reads to the mutable runtime owner.
`Constructor(selector)` selects an actual H13 class; `SharedActor` selects
the intermediate actor's exact virtual method `0047ADA0` without a fabricated
constructor selector. It rereads the command code from that address. For a
constructor target, the caller must resolve the placement's H13 global slot
using the selected scene's residency before supplying its selector. H13 slots, local
registrations, constructor selectors, command codes, OVL leaf IDs, and
campaign selections remain distinct.

The returned `InstanceCommandView` retains that target and names the audited virtual `+4` method
and the exact shared payload consumer when known. `proven_method: None`
means that this module has no audited method profile for the target.
`Opaque` includes unclassified specialized commands and
native rejection paths; it does not imply that retail accepts the command.
No runtime state is produced and commands are not folded into a final model
or object state.

Specialized policies only apply to their explicit `Constructor` targets.
`SharedActor` accepts the exact shared 0..15 decoding profile: code 3 has no
operand, codes 13/14 preserve conditional availability, and negative codes or
codes above 15 remain opaque native rejection paths. In particular, its code
16 does not inherit a derived actor's accepted no-op or specialized operand.

All **94 constructor selectors** have an audited method profile. This proves
their shared forwarding paths, not every specialized command handled by them:

| Constructor selectors | Virtual `+4` method | Shared path covered here |
| --- | --- | --- |
| 1, 67, 75 | `00479E70` | Codes 0 and 1 accepted without a payload read |
| 22, 58, 63, 65, 70, 76, 83 | `0047A1A0` | Base codes, plus model code 2 |
| 3, 4, 5, 6, 20, 24, 25, 26, 27, 28, 31, 32, 38, 40, 41, 44, 56, 57, 61, 62, 66, 68, 73, 85, 90, 93 | `0047ADA0` | Shared actor commands |
| 2, 7, 33, 77, 80 | `00426030` | Shared actor commands plus codes 16 through 18 below |
| 8, 9, 36 | `004239E0` | Through `004276B0` and `00426030` |
| 12 | `004672C0` | Same `004276B0`/`00426030` shared path |
| 23 | `0040D9C0` | Codes at most 15 forwarded unchanged to `0047ADA0` |
| 10, 21 | `00425240` | Through `004239E0` |
| 11 | `00402330` | Through `00426030` |
| 13 | `004689D0` | Through `004672C0` |
| 14, 89 | `0046D780` | Through `0047A1A0` |
| 15 | `0042F100` | Through `0040D9C0` |
| 16 | `0040F7D0` | Through `0047A1A0` |
| 17 | `0046E4F0` | Through `0047ADA0` |
| 18 | `00478860` | Through `0047ADA0` |
| 19 | `00448120` | Through `0047A1A0` for codes 0 through 2; own terrain-light codes 3 through 7 |
| 29 | `00458C40` | Through `0047ADA0` |
| 30 | `0048BBF0` | Through `0040D9C0` |
| 34 | `00433C90` | Through `00429440`, `004276B0`, `00426030` |
| 35 | `0043D270` | Through `00429440`, `004276B0`, `00426030` |
| 37 | `0040A1D0` | Through `0040D9C0`; own scalar codes 21 through 28 ([family](FAMILY40D800_RUNTIME.md#selector-37-terrain-bridge)) |
| 39 | `0043B7D0` | Through `0040D9C0`; code 21 retains the operand address, own codes 22 through 25, 27 and 28 ([family](FAMILY40D800_RUNTIME.md#selector-39-course-target)) |
| 42 | `00401080` | Through `0047A1A0` |
| 43 | `00454220` | Through `0047A1A0` |
| 45 | `0045C010` | Through `0047A1A0` |
| 46 | `00475CB0` | Through `0047A1A0` |
| 47 | `0047FDC0` | Through `0040D9C0`; code 21 accepts without a read, 22 copies the alarm seconds BYTE to `+0x16C`, 23 sign-extends a WORD to the radius `+0x164` |
| 48 | `0043C840` | Through `0047A1A0` |
| 49 | `004763F0` | Through `0040D9C0` |
| 50 | `0043FF40` | Through `0047A1A0` |
| 51 | `00404CC0` | Through `0047A1A0` for codes 0 through 2; own route-node codes 3 through 8 |
| 52 | `00454880` | Through `0047ADA0`; code 16 stores BYTE == 0 at `+0x138` and clears flag `0x40` |
| 53 | `00454A30` | Through `0047ADA0` |
| 54 | `00464940` | Through `0047A1A0` for codes 0 through 2; selector 19's terrain-light codes 3 through 7 at `+0x6C..+0x72` |
| 55 | `00473F70` | Through `0047ADA0` |
| 59, 82, 91 | `00405D10` | Through `0047ADA0`; for selector 59 code 16 with a nonzero BYTE wakes the boulder (`+140 = 0`, flags `0x04200000` cleared), 17 stores BYTE == 0 at `+144`, 18 and 19 BYTE != 0 at `+145` and `+146`; higher codes refuse |
| 60 | `00480B30` | Through `00426030`; own code 19 below |
| 64 | `004407E0` | Through `0047A1A0` |
| 69 | `00411100` | Through `0047ADA0`; own codes 16 and 17 below |
| 71 | `0046F810` | Through `0047A1A0` |
| 72 | `0046FA70` | Through `0047A1A0` |
| 74 | `0042D6F0` | Through `0047ADA0` |
| 78 | `00406800` | Through `0047ADA0` |
| 79 | `00408880` | Through `00426030` |
| 81 | `00409320` | Through `00479E70` |
| 84 | `0043C2A0` | Through `0047ADA0` |
| 86 | `0042DEF0` | Through `00479E70`; code 2 retains eight data bytes and rewrites the interval WORD |
| 87 | `0048C7A0` | Base codes 0/1 through `00479E70`; own unsigned label/radius WORDs (2/6), RGB bytes (3–5), and label-command list movement; see [map annotations](OBJECT_RUNTIME.md#selector-87-map-annotations) |
| 88 | `00443900` | Through `0047ADA0`; code 16 retains the operand address without a read |
| 92 | `00404420` | Through `0047A1A0` |
| 94 | `00445900` | Through `00479E70` |

The corpus test checks each audited address against virtual `+4` in the
original final vtable from the constructor dispatch. Native regression
checks additionally pin the expanded wrappers' signed comparison, unchanged
argument forwarding, exact callee and immediate return. These paths are
independent of the initial-model helper: virtual `+08`, `+0C`, and `+4` have
different contracts. Selectors outside the original 1 through 94 range stay
unclassified.

## Shared command input reads

Offsets in this table are relative to **command +4**. These views preserve
authored inputs before any runtime switch, mutation, shift, or guard.

| Code | Shared consumer | Read and proven use |
| --- | --- | --- |
| 0, 1 | `00479E70` | Accepts the code with no payload read |
| 2 | `0047A1A0` | One raw WORD stored at object `+0x42`; `0047A110` consumes it as a signed H8 argument |
| 3 | `0047ADA0` | Accepts the code with no payload read |
| 4 | `0047ADA0` | Six bytes, copied as DWORD plus WORD to object `+0x8E..+0x94`; exposed as three raw WORDs without coordinate naming or signedness inference |
| 5 | `0047ADA0` | One raw WORD stored at object `+0x8C` |
| 6 | `0047ADA0` | Signed DWORD switch input producing object `+0xB4`, matching the shared class payload's separate selector |
| 7 | `0047ADA0` | Unsigned WORD maximum-health input stored at object `+0xA2` |
| 8 | `0047ADA0` | Unsigned WORD current-health input stored at object `+0xA0` |
| 9 | `0047ADA0` | One raw WORD stored at object `+0x7A`; this setter does not apply the class initializer's 175 clamp |
| 10, 11 | `0047ADA0` | Signed DWORD, arithmetic right shift by eight, then low-byte store at object `+0x7C` or `+0x7D`; the view retains the full input |
| 12 | `0047ADA0` | Signed DWORD stored at object `+0xC4`; values below three OR sticky flag `0x8000`, which does not change the low-nine-bit dirty mask |
| 13, 14 | `0047ADA0` | Conditional unsigned WORD bit-index input; opposite disable predicates, as detailed below |
| 15 | `0047ADA0` | One raw WORD stored at object `+0x76` |
| 16 | `00426030` | Accepts the code with no payload read |
| 17 | `00426030` | One unsigned WORD, zero-extended into object `+0x164` |
| 18 | `00426030` | One raw DWORD, negated into object `+0x144`; no input signedness inferred from `NEG` |

Native `0047ADA0..0047AF79` is decisive for the read widths. Ghidra's
`(short)*int_pointer` spelling for codes 5, 7, 8, 9 and 15 does not mean
that four source bytes are read. Code 4 reads six bytes, not eight. The
health names additionally follow the unsigned current/max consumers in
`0047CBC0`; other fields retain neutral object-offset names.

Model code 2 is an authored override input, not a resolved final visual.
At `0047A110`, argument `-1` resets object `+0x28` to `0x100` and `+0x40`
to zero, then clears the visual flag through the zero-radius policy. Other
negative values skip the model lookup and retain those fields before that
same policy runs. Nonnegative arguments use the H8 resource pool. The
decoder preserves both raw WORD and signed argument and executes none of
these differences.

The `00426030` rows apply only to selectors **2, 7, 8, 9, 10, 11, 12, 13,
21, 33, 34, 35, 36, 60, 77, 79, 80**. Other classes may give these same
command codes unrelated meanings.

### Terrain-light command inputs

Selector **19** has its own meanings for codes 3 through 7 in `00448120`:

| Code | Payload read | Stored field |
| --- | --- | --- |
| 3, 4, 5 | One byte, retained as `i8` because `0045B1A0` sign-extends the channel | Object `+0x44`, `+0x45`, `+0x46`, respectively |
| 6 | One unsigned byte | Object `+0x4A`, the later model-draw gate |
| 7 | One unsigned WORD | Object `+0x48`, the terrain-light radius |

Codes 0 and 1 retain the base no-payload behavior; code 2 retains the visual
model-WORD input. Other codes remain opaque in the asset view. The decoder
does not reinterpret the channel commands as shared actor fields or execute
their writes. Exact-width tests include negative channel bytes, the full
unsigned gate/radius range, and allocations too short for the required read.

The actual shared H13 registration is slot **53**, with no replacement in
the other installed H13 banks. Across all 33 worlds it supplies **1,125
placements**, including **42** in `32.ovl`. Their ordered commands contain
67 code-1, 374 code-2, 1,124 code-3, 1,122 code-4, 1,091 code-5, 267 code-6,
and 1,125 code-7 inputs. The corpus check compares every decoded operand
with its original H14 source span. These counts establish authored inputs,
not successful command execution or resulting terrain pixels.

Selector **54**'s `00464940` is `00448120` writing object `+0x6C..+0x6E`,
`+0x72` and `+0x70`; the decoder gives it the same views. Its shared H13
slot **35** supplies **133 placements** in leaves 13, 20, 30, 38 and 39,
none in `32.ovl`: 2 code-1, 133 code-2, 126 each of codes 3 to 5, 133 code-6
and 126 code-7 inputs. Every placement sets its model and draw gate.

### Route-node command inputs

Selector **51** uses `00404CC0`. Its signed `code <= 2` branch forwards
unchanged to `0047A1A0`; codes 0 and 1 accept without payload reads and
code 2 keeps the visual model-WORD input. Its own command table is:

| Code | Payload read | Native effect |
| --- | --- | --- |
| 3 | One raw WORD | Decrement with WORD wrapping, then store at object `+0x5E` |
| 4 | One raw DWORD | Zero ORs `+0x48` with `1`; one accepts unchanged; every other value rejects |
| 5 | None | OR object byte `+0x48` with `2` |
| 6, 7 | None | Accept unchanged |
| 8 | None | OR object byte `+0x48` with `0x10` |

Other codes reject without reading an operand. The code-3 value is an
authored one-based placement reference: `00404F40` compares its decremented
WORD against another route node's placement WORD at `+0x5C`. The decoder
retains the original `u16` bits, including zero and high-bit values, without
performing that decrement or claiming the reference resolves. The code-4
view retains the full `u32` equality-switch input, even when it would reject;
equality against zero and one does not establish signed arithmetic.
`RouteNodeFlagOr` exposes the recovered instruction-immediate mask for codes
5 and 8, not a nonexistent authored operand or a guessed flag meaning.

The constructor payload remains the existing flags-only view. Virtual
`+0C` (`00479DB0`) invokes the context method and shared flags setter;
context method `00404C30` first calls visual initialization and then copies
the context placement index's low WORD into `+0x5C`. No extra H13 field is
inferred from this object extension. Route construction, list custody and
later linkage are separate from these asset views.

The sole selector-51 registration is shared H13 slot **30**, with no
replacement in the other original banks. It supplies **369 placements**
across the 33 worlds, including **40** in `32.ovl`. The corpus contains
369 code-3, 282 code-4, 58 code-5, 11 code-6, eight code-7 and 36 code-8
inputs. Code 4 has 268 zero inputs and 14 one inputs. The corpus check
compares exact operands and spans with their original H14 bytes; these
counts do not establish native command execution or completed routes.
Synthetic checks cover every shorter WORD/DWORD operand, high-bit values,
unread operand-free paths and class-specific command meanings. Executable
checks pin the jump table, WORD decrement, DWORD switch and immediate masks.

### Conditional bit-index inputs

Shared actor codes 13 and 14 call `00488AA0` before reading their operand.
That accessor returns the first object from the world owner's list at
`[0055F160] +0x60`. If it returns zero, neither command reads payload bytes.
Otherwise each reads exactly one unsigned WORD and passes it to `004752E0`
with the returned object's embedded `+0x9C` bitset owner, whose six mask DWORDs
start at `+0xA0`. This uses `LEA`, not a pointer read from `+0x9C`. The bitset belongs to that first
world object, not to the actor receiving the command.

`004752E0` returns true for index zero. Other unsigned WORD indices query
bit `index & 31` in DWORD `bitset +4 +4*(index >> 5)`. Native performs no
range check; the parser neither bounds the index to an invented item count
nor names individual bits. Code 13 calls actor policy `00479FA0` when the
query is false; code 14 calls it when true. That policy disables list/flag
participation and moves the actor's link; it is not an immediate destructor.
The asset view does not query a live bitset or execute any of those effects.

`ActorDisableUnlessBit` and `ActorDisableIfBit` retain these distinct predicates.
Their `ConditionalInstanceWord` is `Available` when two bytes exist inside the
main allocation, with exact unsigned value and source span. If the allocation
ends before those two bytes, it reports `OutsideAllocation` and the attempted
offset. This does not certify valid execution: a nonnull runtime object would
make that read invalid. It also does not reject a path whose native guard may
skip the read, borrow relocation-tail bytes, or substitute index zero.

Native checks pin the guards and WORD reads at `0047AF0A..0047AF1B` and
`0047AF38..0047AF49`, opposite branch senses at `0047AF21`/`0047AF4F`, and
the zero-index/unsigned mask at `004752E6..004752F4`. Synthetic fixtures cover
missing and one-byte operands, high unsigned indices, zero, and class paths
that do not reach the actor consumer. Resident corpus inspection finds all
120 operands available: **81 code-13 inputs and 39 code-14 inputs**, spanning
eight distinct indices. Every decoded value is compared with its H14 bytes;
availability and authored order do not establish that a guard passed.

The [shared actor runtime](ACTOR_RUNTIME.md#shared-payload-commands-and-completion)
now consumes this guarded view using the actual first player's borrowed
`PlayerSaveState::owner_a0` mask. The existing canonical `grant_is_owned`
query preserves the zero special case and rejects indexes at least 192 as a
host storage boundary; it does not change the parser's raw unsigned operand
or model native out-of-bounds reads as false. Disposal still accepts the
command, so subsequent commands execute. The ordered runtime walk preserves
earlier effects on rejection and leaves cleanup to its completion owner.

## Selector 11 parameter commands

`00402330` inherits the central derived commands through signed code 18.
Its specialized views preserve exact widths: code 19 reads a DWORD boolean
for low-byte bit 2 at actor `+1AE`; code 20 copies all ten bytes into
`+1A0..+1A9`; codes 22/23 read WORD booleans for bits 4/16. Code 21 uses
`ConditionalInstanceDword`: when behavior `+140` is absent, its operand is
not read. Otherwise it writes behavior `+04`, preserves `+08`, and sets
byte `+0C=1`. Missing bytes remain explicitly conditional, matching the
existing WORD guards for shared actor codes 13/14. These view types do not
claim that a particular runtime guard was reached. See
[actor runtime](ACTOR_RUNTIME.md#selector-11-construction-and-commands).

## Selector 86 retained mutable data

Selector **86** code 2 has a distinct retained-data policy: `0042DEC0` reads
the signed WORD at operand `+2` and writes a derived interval at operand
`+6`, retaining the operand address. `Selector86Data` validates the complete
eight-byte read/write span and exposes the signed rate without changing any
authored bytes. The [runtime owner](OBJECT_RUNTIME.md#selector-86-retained-emitter-data)
preserves allocation aliases and performs the write; a plain immutable
suffix cannot supply ownership for this command.

## Selector 88 retained command pointer

`00443900` forwards signed codes at most 15 to the shared actor method,
rejects codes greater than 16, and handles code 16 by storing the operand
address at object `+13C`. It does not read a DWORD, header or array there.
`RetainedPayloadPointer` therefore exposes only the allocation-relative
operand offset, with consumer `00443900`. A command whose four-byte code
ends exactly at the main-allocation boundary has a valid retained one-past
pointer; relocation-tail bytes must not be read to extend it.

The [selector-88 runtime](ACTOR_RUNTIME.md#selector-88-construction-and-retained-commands)
retains the shared `InstanceCommandAllocation` owner when that command is
reached. Its borrowed suffix must use the same source allocation, not merely
equal bytes. Repeated code 16 replaces the pointer. The later `00443930`
callback reads counts and rewrites referenced actor handles in that shared
allocation; its record interpretation is not an eager command-16 read.
The mutable allocation, actor lookup and player binding now exist, and the
[actor runtime](ACTOR_RUNTIME.md#player-attachment-and-world-camera) performs
these ordered rewrites during the loader's after-load pass.

In leaf 32, placement 117 resolves to selector 88 through H13 slot 143.
Its sole command is 16 at H14 main offset 6520, retaining offset 6524.
That retained header has actor reference 53 at `+10`; its four signed
count/pointer pairs contain 6, 2, 5 and 2 references. These are authored
inputs, not proof that the later callback or update has executed.

## Selector 30 and 35 command extensions

The [extended actor decoder](../../crates/inf-formats/src/instance_command_views_extended_actors.rs)
owns selector-30 codes 16..32, selector-34 codes 19..23 and selector-35 codes 19..24. The independent
[selector-30 chain](SELECTOR30_RUNTIME.md#instance-commands) keeps its signed
BYTE maximum, raw versus boolean BYTE flags, unsigned WORD references and
exact six-byte copies separate. Its inherited codes at most 15 still use
the shared actor decoder.

`Family428650` explicitly names intermediate `004276B0`, avoiding a fabricated
constructor selector. Its codes 20/21 read two/three WORDs for terrain-relative
home coordinates; 22 reads a full DWORD placement reference. Code 19 reads
nothing. [Selector 34](FAMILY428650_RUNTIME.md#selector-34) adds a no-read
retained operand address at code 23 and rejects higher codes.
[Selector 35](FAMILY428650_RUNTIME.md#selector-35) adds a no-read
retained operand address at code 23 and DWORD packed input at code 24. The
decoder executes none of the terrain queries, flag changes or retained reads.
One-past retained addresses are accepted until an actual consumer reads them.

## 004236D0 family command policy

`Family4236d0` names the separate `004239E0` command policy used by
selectors 8/9/10/21/36. Signed codes at most 22 use the family forwarding
path. Code 23 succeeds without reading an operand; code 24 retains its
address without a read. Higher codes remain opaque/rejected by this method.
Selectors 10/21's `00425240` wrapper additionally accepts code 25 without
reading its operand and rejects codes above 25.
Runtime custody and later consumers belong to the
[004236D0 family](FAMILY4236D0_RUNTIME.md).

## Selector 69 inputs and specialized visual path

`00411100` forwards codes at most 15 to `0047ADA0`. Code 16 reads one DWORD,
doubles it with `SHL 1`, and stores object `+0x14C`; code 17 reads one DWORD
and stores object `+0x144`. The view retains the original inputs as signed
DWORDs, following the later signed use in `00411150` and `00411570`, and
does not perform the shift.

Virtual `+0x1C` in the final `005205D8` vtable is `00411150`. It consumes
`+0x14C` through signed half/truncation, stores the result at `+0x150` and
the low WORD at `+0x28`, and uses signed tests on `+0x144` while constructing
terrain-related geometry. Virtual `+0x2C` is `00411570`: that draw method
uses H3 resource slot zero, constructs positions and quads, and submits
them through `004A42B0` with `0040EF30`/`00411D50` callbacks. This is a
specialized procedural visual path; that bounded procedure does not issue
a normal H8 model draw. It does not establish complete runtime rendering or
make a missing initial H8 reference disappear from inspection. The runtime
class, its draw and its receipt are in
[the barrier](FAMILY40D800_RUNTIME.md#the-targets-life).

The installed corpus contains **21 placements** using selector 69,
registered through `1.ovl` H13 global slot **5**. There are 21 code-16 inputs,
seven code-17 inputs, and eight shared code-9 inputs. `13.ovl` placement
**28** has ordered codes **17 (8)** and **16 (700)** and no model command.
The values are authored inputs, not measurements of the resulting object.

## Selector 60 placement spawn lists

Selector 60's virtual `+4` method `00480B30` forwards codes at most 18 to
`00426030`. Code 19 stores the payload address at object `+0x190`. Its
inspection view is a `PlacementSpawnList`:

| Payload offset | Exact input | Native evidence |
| --- | --- | --- |
| `+0` | signed WORD mode | `00480C80` sign-extends it for its switch; no mode names inferred |
| `+2` | unnamed WORD | Preserved by the header source span |
| `+4` | signed DWORD row count | `00480B63..00480B68` skips the row walk when at most zero |
| `+8` | relocated pointer to rows, read only for positive count | `00480B72`; parser requires the exact field and target in canonical H14 relocations |

`00480B8D` advances by **0x18 bytes**. Each `PlacementSpawnRow` retains this
exact span and these consumer-proven fields:

| Row offset | Representation and use |
| --- | --- |
| `+0` | Full DWORD one-based H14 placement reference. `0047F7B0` rejects negative values, subtracts one, and compares against the sign-extended object placement index at `+0x2A`. `00480610` uses a nonzero value to index H14 resource zero at `header+0x38+4*reference`; the real pointer array begins at `+0x3C`. This is not an H13/H8 resource ID or campaign index. |
| `+4` | Signed WORD threshold in `00480510` |
| `+6` | Signed WORD capacity used by `00480460` allocation/iteration and `00480510` |
| `+8` | Signed WORD limit comparison in `00480510` |
| `+A` | Signed WORD sign-extended into the runtime row's `+4` by `00480460` |
| `+C`, `+E` | Signed WORD endpoints used by `00480610` random interpolation; the result is multiplied by 100000 for a runtime countdown |
| `+10` | BYTE flags: bit 0 participates in `00480510` aggregation, bit 1 is tested by `00480460`, bit 2 selects the `00480610` transform override |
| `+11..+13` | Unnamed bytes, preserved by the row source span |
| `+14` | Original DWORD runtime-pointer placeholder; `00480B30` zeros it and `00480BB0` replaces it with a pointer allocated by `00480460` |

The parser does not apply the initializer's writes: zero capacity stays zero,
and the runtime-pointer placeholder retains its original bits. It does not
resolve or execute placement spawning, normalize modes or flags, interpolate
delays, or assign a unit name to the unnamed signed inputs. Aliases remain
independent views over the same source. A nonpositive count exposes only the
eight-byte header prefix and does not require or read the `+8` pointer.
Positive counts require that pointer's relocation and bound the complete
`count*0x18` row span before allocating. Header and row spans never include
relocation-tail bytes as part of the main allocation.

Runtime dispatch instead returns `RetainedPayloadPointer`; command 19 reads
the current count and each reached row through the shared mutable owner.
This preserves partial normalization and overlap effects that an eager
inspection snapshot cannot represent. After-load allocation, placement
lookup and row cleanup belong to [selector-60 runtime](SELECTOR60_RUNTIME.md).

There are **55 selector-60 placements**, all registered through `1.ovl`
H13 global slot **45**, and their 55 code-19 lists contain **56 rows**.
The corpus check proves that the other installed H13 banks do not register
slots 5 or 45. `20.ovl` placement **102** has ordered codes **2 (H8 argument
577)**, **18 (raw DWORD 1)**, and **19 (spawn list)**. This gives an authored
model override after its initial model input; it does not prove execution
of the command, resource availability at that later time, or a final model.

## Remaining boundaries

The class-independent graph and all shared dispatch profiles are covered.
Most specialized commands above each class's forwarding limit and derived
execution guards remain open. The exact shared actor command walk, its
always-true admission guard and scalar/conditional effects have a separate
runtime owner. Selector 88 has a concrete mode-0 factory and retained code-16
owner and its after-load rewrite; its update remains gated. Remaining bytes and
source spans are retained without guessed record extents. Further asset
work should extend exact consumers; other class factories, concrete
creation-listener callbacks, manager updates and complete world admission
remain runtime boundaries.
