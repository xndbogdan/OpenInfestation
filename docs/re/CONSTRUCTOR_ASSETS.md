# Infestation constructor asset inputs

This document owns the consumer-proven Handler-13 model bindings and shared
class payload fields. [WORLD_ASSETS.md](WORLD_ASSETS.md) owns H13 framing and
H14 placement graphs. The canonical parser is
[constructor_assets.rs](../../crates/inf-formats/src/constructor_assets.rs)
and its [derived actor view](../../crates/inf-formats/src/constructor_actor_derived.rs),
with [synthetic checks](../../crates/inf-formats/src/constructor_assets_tests.rs)
and [retail corpus checks](../../crates/inf-formats/tests/constructor_assets_corpus.rs).
These views describe authored initialization inputs. They do not construct
objects, execute flags, resolve a final rendered model, or decode a complete
class record.

[OBJECT_RUNTIME.md](OBJECT_RUNTIME.md) owns the separate live construction
order, list and handle custody, failure cleanup, retained payload mutation,
and the bounded implementation prerequisites.

## Authority and dispatch

The original executable and its decompilation in
bulk/game_logic.c are the source authority. Static reads
and executable disassembly establish this contract without a new recording.
The important ranges are `00460310..004603AE`, `00460120..004601DE`,
`0047A050..0047A104`, and `0047AC10..0047ACFF`.

`00460310` resolves the H13 resource from the signed low WORD of its resource
argument. It increments the first resource WORD with 16-bit wrapping and
passes that result to `004603B0`. The switch accepts selectors **1 through
94**, using the jump table at `00461A04`. Its cases construct objects and
sometimes install a derived vtable after the constructor returns. The parser
records the called constructor, final vtable, and virtual `+08` and `+0C`
initializers. For example, selectors 12 and 13 share constructor `00467070`
but use final vtables `00527058` and `005269C4`. Selector 27 ends with
`00525848`, after an intermediate vtable assignment.

The corpus test checks all 94 constructor calls against the original switch
cases and both initializer entries against the original vtables. Unsupported
selectors retain their prefix and an opaque payload view; they are not
silently replaced with a default class. Constructor selectors remain distinct
from local registrations, global H13/H8 slots, OVL leaf IDs, campaign
selections, and the payload's separate `+0C` selector.

## Initial H8 model binding

`00460120` reads the WORD at **H13 entry +2**, zero-extends it into a local
DWORD, and places a pointer to that local at initialization-context `+8`.
The class-data pointer is **H13 entry +4**, passed separately to virtual
`+0C`. The model argument therefore does not come from the first class-data
WORD.

The visual context initializer `0047A050` reads the WORD through context
`+8` and stores it at object `+0x42`. It treats this as a signed H8 resource
argument. `-1` takes the disabled branch and clears object flag `0x4`; other
negative values skip the lookup. A nonnegative argument is resolved through
the H8 handler object `0055EEE0` (slot array `0055EEE4`, capacity
`0055EEE8`). The initializer copies model WORD `+0x0E` to object `+0x40`
and model WORD `+0` to object `+0x28`, substituting `0x100` for a zero model
WORD. The asset parser retains the original argument, including negative
values, and does not execute these state changes.

Context initializers `0047A610` and `00404C30` call `0047A050` with the
same context. The former additionally handles rotation input; the latter
copies the context's `+0x0C` low WORD to object `+0x5C`. These three
initializer profiles prove the shared model binding. The seven selector
profiles 1, 67, 75, 81, 86, 87, and 94 use `00479D40` directly and do not
consume the header model argument. Absence of this binding says nothing
about other resources a specialized class might access.

The installed 162 H13 resources provide **155 initial model bindings across
79 selectors**. The seven other resources use the base context initializer.
Later class initialization or object methods may change model-related state;
these are not promises about the final live visual.

## Shared payload fields

`00479D70` reads the first class-data DWORD. It uses bits 0 and 1 for
`00479E90`/`00479F20` policy calls and writes object `+0x2C` as
`(old & 0xFF000000) | raw_flags`. The decoder exposes the original DWORD
as flags without applying the runtime policy.

`0047AC10` first passes the unchanged class-data base to `00479D70`, then
reads the following fields. Offsets below are relative to **entry +4**, not
the H13 resource header.

| Payload offset | Representation | Proven consumer use |
| --- | --- | --- |
| `+0` | `u32` flags | Shared `00479D70` input |
| `+4` | `u16` initial health | Copied to object `+0xA0` and `+0xA2`; damage/healing consumer `0047CBC0` treats these as current and maximum unsigned WORD health |
| `+8` | `u16`, `word_08` | Copied to object `+0x8C`; further semantics unnamed |
| `+0x0C` | `i32`, `selector_0c` | Input to the switch producing object `+0xB4`; separate from constructor dispatch |
| `+0x10..+0x38` | ten damage-response pairs | The address at `+0x10` is stored at object `+0xA4`; each channel contains an unsigned WORD threshold and unsigned WORD Q8 multiplier, consumed by `0042A8F0` |
| `+0x38` | `u16`, `word_38` | Copied to object `+0x7A`; runtime values at least 175 become zero |
| `+0x3C` | raw `u32`, `word_3c` | If nonzero, its low WORD replaces object `+0x28`; signedness, units, and full field semantics remain unproven |
| `+0x40` | `u16`, `word_40` | Copied to object `+0x76`; further semantics unnamed |

The parser preserves raw values before clamps, switch mappings, or low-WORD
overrides. Each decoded scalar has an exact source span. The embedded address
is an allocation-relative offset, not a relocated pointer field. The
`0x42`-byte prefix is the boundary of these proven reads, **not a complete
record length**. Gaps, derived fields, and suffix bytes remain in the original main-allocation span
retained by `ConstructorAssetBank.prefixes`.

Selector 88 uses final vtable `00524AC0` with shared wrapper `0047AD70`
and context `0047A610`. Its H13 initializer adds no class-specific payload
reads beyond this shared prefix. Constructor `00443890` initializes six
extension DWORDs independently; command `00443900` code 16 instead retains
an H14 operand pointer at object `+13C`. That input belongs to the
[instance-command owner](INSTANCE_ASSETS.md#selector-88-retained-command-pointer),
not an extended H13 record. The [mode-0 actor factory](ACTOR_RUNTIME.md#selector-88-construction-and-retained-commands)
retains the canonical shared payload identity for its response table;
later selector-88 callbacks remain separate boundaries.

These `+0C` initializer paths reach `0047AC10` with the unchanged class-data
base. The final column names the class-data helper chain after any context
initialization; optional non-null branches retain the same base.

| Virtual `+0C` initializer | Class-data helper chain |
| --- | --- |
| `004038E0` | `00405DE0` -> `0047AC10` |
| `00405DE0`, `0047AD70` | `0047AC10` |
| `0040D920` | `0040D950` -> `0047AC10` |
| `004239C0` | `00423850` -> `00426C10` -> `0047AC10` |
| `00425270` | `00425290` -> `00423850` -> `00426C10` -> `0047AC10` |
| `00426BF0` | `00426C10` -> `0047AC10` |
| `0042D760` | `0042D780` -> `0047AC10` |
| `0042EDD0` | `0042EE00` -> `0040D950` -> `0047AC10` |
| `00433C70` | `00433C00` -> `00426C10` -> `0047AC10` |
| `0043D250` | `0043D1C0` -> `00426C10` -> `0047AC10` |
| `00455720` | `004556F0` -> `0047AC10` |
| `004557D0` | `00455770` -> `004556F0` -> `0047AC10` |
| `00456840` | `00456740` -> `00455770` -> `004556F0` -> `0047AC10` |
| `00458380` | `00458330` -> `004556F0` -> `0047AC10` |
| `00458CA0` | `00455720` -> `004556F0` -> `0047AC10` |
| `0045B9A0`, `0045BEE0` | `0047AD70` -> `0047AC10` |
| `00468310` | `00468330` -> `00426C10` -> `0047AC10` |
| `00482480` | `00482460` -> `00423850` -> `00426C10` -> `0047AC10` |
| `00485460` | `00485440` -> `0047AC10` |
| `0048B390` | `0048B3C0` -> `0040D950` -> `0047AC10` |

The initializers `00403EF0`,
`0045BFA0`, `0046F870`, `0046FB00`, `004756F0`, and
`00479DB0` reach `00479D70` at the original base without the shared actor
layout. They receive a flags-only view. Selector 83's `0043C3B0` reaches it
too, after keeping the payload itself, whose WORDs `+4..+E` its update reads:
it receives the flags and those words
([the sinking patch](#the-sinking-patch-selector-83)).

Selector **86** has a separate twelve-byte prefix: `0042DE80` reaches the
same flags setter, then `0042DEC0` retains payload `+4`, reads signed WORD
payload `+6` and writes the normalized interval at payload `+10`.
`Selector86ConstructorPayload` retains the flags, signed rate, source span
and allocation-relative retained address. Parsing preserves the authored
interval bytes; the [live allocation owner](OBJECT_RUNTIME.md#selector-86-retained-emitter-data)
performs the shared write.

Selector **70** is a deliberate exception: `0046E860` invokes `0046E830`,
which stores its own payload pointer at object `+0x350` and reads payload
`+4` into object `+0x338`. It does not call `00479D70` or `0047AC10`.
Its model binding is proven through context initializer `0047A610`. Its
specialized view exposes the raw `+4` DWORD and the relocated point table
below. Treating every class-data first DWORD as flags would misclassify this
path.

### Terrain-light payload

Selector **19** installs vtable `00526978` after the visual constructor
`0047A030`. Its initializer `00448200` runs the visual context initializer
and then `004481C0`, which applies shared flags and copies this prefix:

| Payload offset | Representation | Proven consumer use |
| --- | --- | --- |
| `+0` | `u32` flags | Shared `00479D70` input |
| `+4..+6` | three `i8` channels | Copied to object `+0x44..+0x46`; `0045B1A0` sign-extends each channel byte for the later terrain-light update |
| `+7` | `u8` draw gate | Copied to object `+0x4A`; gates the later `00461C30` model draw |
| `+8` | `u16` radius | Copied to object `+0x48`; `004481A0` zero-extends it for `0045B1A0` |

`TerrainLightConstructorPayload` retains exact field spans and the ten-byte
proven prefix. No fourth channel is initialized or consumed: object byte
`+0x47` is not an alpha field. The draw gate retains its full byte, and the
radius is not narrowed to a signed WORD. The asset view applies no runtime
clamping, lighting, or drawing. The one installed resource is shared H13
slot **53**, resolved by its actual registration, not by its asset name.

Selector **54**'s initializer `00464A20` runs the rotating context
`0047A610` and then `004649E0`, which is `004481C0` copying the same prefix
to object `+0x6C..+0x6E` (channels), `+0x72` (gate) and `+0x70` (radius); it
receives the same view. Its one installed resource is shared H13 slot
**35**. The
[rotating terrain light](OBJECT_RUNTIME.md#selector-54-rotating-terrain-light)
owns the class.

### Common derived actor extension and H9 sound inputs

`00426C10` invokes `0047AC10` with the unchanged class-data pointer and then
reads the following extension. These exact widths come from native
`00426C1E..00426C74`; neither decompiler casts nor neighboring values define
the schema.

| Payload offset | Representation | Proven consumer use |
| --- | --- | --- |
| `+0x42..+0x44` | raw gap | No read assigned here |
| `+0x44` | `u8`, `byte_44` | Copied to object `+0x13D`; `00426C80` uses its low seven bits as a switch selector and bit 7 to choose a count. This is not a constructor selector or resource argument. |
| `+0x45..+0x48` | raw gap | No read assigned here |
| `+0x48` | `u16`, `word_48` | Tested against zero; nonzero sets object `+0x13D` bit 7. The original full WORD is retained. |
| `+0x4A` | `i16`, `signed_word_4a` | `MOVSX` into object `+0x164`; `00427490` squares this value for a position-distance comparison. No world-unit conversion is applied. |
| `+0x4C` | raw `u32`, `word_4c` | Copied to object `+0x174`; `00424280` and `00424590` compare it against 2 and 1. Equality tests do not establish signedness. |
| `+0x50` | raw WORD plus signed H9 argument | Copied to object `+0x17C`, consumed in `004294A0` at `00429655..004296A9` |
| `+0x52` | raw WORD plus signed H9 argument | Copied to object `+0x17E`, consumed in `004294A0` at `004296AE..004296F6` |
| `+0x54` | raw WORD plus signed H9 argument | Copied to object `+0x180`, consumed in `004294A0` at `00429C09..00429C55` |
| `+0x56` | raw WORD plus signed H9 argument | Copied to object `+0x182`, consumed in `00424280` at `00424305..0042433D` |

The header's initial model binding still names H8. The four sound arguments
belong to **H9**: original handler table `0053AA14 + 9*4` contains
`0055EE98`, whose slot array and capacity are at `0055EE9C`/`0055EEA0`.
The first three consumers explicitly skip zero and signed negative WORDs,
then check capacity and perform an inline lookup. The fourth skips zero
before calling `00405540` with ECX=`0055EE98`; that generic lookup rejects
signed negatives and checks capacity. Successful caller paths subsequently
pass the sign-extended WORD to `00487F10`. This establishes the same
strictly-positive argument eligibility for these four particular bindings,
without claiming that zero has the same meaning for every H9 caller.

Each `ConstructorSoundBinding` exposes the original WORD and its source
span, signed interpretation, destination object offset, proven consumer,
and `lookup_slot`. The latter is `Some(raw)` only for a positive signed
WORD. It is an eligible authored argument, **not** a successful resource
lookup: current capacity/residency, alias resolution, lazy loading and
runtime branch execution remain outside this asset view. No sound event
names are inferred from the four adjacent fields.

The `signed_word_4a` initialization is deliberately different from H14
code 17 in `00426030`, which zero-extends its WORD override into the same
object field. Source signedness follows each consumer separately.

`ActorConstructorPayload.derived` is present only for initializers
`004239C0`, `00425270`, `00426BF0`, `00433C70`, `0043D250`, `00468310`,
and `00482480`. Their helper chains above prove the unchanged base. This
corresponds to constructor selectors **2, 7, 8, 9, 10, 11, 12, 13, 21, 33,
34, 35, 36, 60, 77, 79, 80**. The original `SharedActor` payload and its
`0x42` read boundary remain intact; the optional extension spans
`+0x42..+0x58` and separately identifies both raw gaps. The complete
`0x58` prefix is bounded against the main allocation before extension reads.
There are no new relocated pointers in these fields.

The installed corpus contains **52** such resources: 43 in `1.ovl` and
three each in `29.ovl`, `30.ovl` and `32.ovl`. They cover 14 installed
selectors and expose **208** sound arguments. Of those, 67 are zero and
**141** are positive, referencing 14 distinct H9 slots. These are authored
reference counts, not sounds played. Corpus checks compare every decoded
field and sound span to the original OVL; executable checks pin the helper
forwarding paths, read widths, handler identity and signed lookup guards.
Synthetic checks exercise high raw bits, signed extremes, all sound
eligibility cases, aliases, source preservation and incomplete prefixes.

Later data is still class-specific. `00423850` and `00468330` retain the
embedded address `payload+0x58` at object `+0x23C`; `00433C00` and
`0043D1C0` also use that address, with different further consumers.
`00423850` additionally reads `+0x68/+0x6C/+0x6E/+0x70`; those inputs and
the various embedded schemas remain open. Storing the address does not
establish a shared table length or a relocated pointer field.

### Selector 70 point table

`0046E830` calls `0046F300`, which follows the pointer at payload `+0x0C`
and reads **sixteen pairs of raw WORDs**, a fixed 64-byte table. The count
comes from the consumer's loop bound; payload `+8` happens to contain 16 in
the installed resource but is not used to establish that bound. The meanings
of payload `+0` and `+8` remain unassigned.

For each pair, `0046F300` multiplies both WORDs by 220, retaining the low
16 bits, forms `[first, 0, second]`, calls object rotation `0047A6C0`, and
adds object position WORDs `+32/+34/+36` with WORD wrapping. The asset view
retains the original unsigned bit patterns and executes none of these
transforms. Low-WORD multiplication alone does not establish source signedness.
The `+4` DWORD is similarly retained as `word_04`, without a guessed role.

The installed resource is H13 local/global slot 77 in `1.ovl`, with initial
H8 argument 262. Its payload begins at allocation offset 4240; the pointer
field is at 4252 and targets 9652. This is the only H13 relocation in the
installed corpus. The decoder requires that relocation and bounds the full
64-byte target within the owning allocation. Header, pointer, and all sixteen
pairs retain exact absolute OVL spans. Synthetic checks cover high raw WORDs,
an unrelated `+8` value, missing relocation, a target one byte too short,
nonzero allocation bases, and unchanged source bytes.

### Embedded damage responses

The shared actor's object `+0xA4` is read at `0047C6EE` and `0047C80C`.
Both callers pass this address in EDX to `0042A8F0`, with an incoming damage
descriptor in ECX. The descriptor's first WORD is signed damage; its byte
`+2` is an unsigned channel index. The consumer reads two **zero-extended
WORDs** at table `+4*channel` and `+4*channel+2`. These are a threshold and
multiplier scaled by 256. For a positive amount above the threshold, its
unscaled result is `((amount - threshold) * multiplier) >> 8`, using the
native logical shift. Positive amounts at or below the threshold yield zero;
negative amounts pass through unchanged. `0047C7C0` can additionally supply
a signed fixed-point scale, and downstream virtual `+0xB4` damage policy can
change the result further. The asset parser executes none of that policy.

The ten-channel layout is supported by independent native evidence:

- Actor construction at `0047AA1E` installs default table `00531260` with
  ten four-byte responses: channel 0 is `(0, 0)`, channels 1..9 are
  `(2000, 256)`.
- Damage owner `0047CBC0`, at `0047CC6C..0047CCB3`, uses the same descriptor
  byte to index a ten-element mapping `[-1,-1,3,-1,4,-1,0,1,2,5]`.
- Native descriptor `00531250` contains amount 1000 and channel 9 and is
  passed through `004887D0`; the last response is therefore used.
- The authored ten responses occupy payload `+0x10..+0x38`, immediately
  before the independently consumed WORD at `+0x38`.

`damage_responses` retains channel ordinals, exact threshold/multiplier source
spans, and full unsigned values. It does not assign damage-kind names or
classify either WORD as a resource reference. The native consumer has **no
channel bounds check**: the ten stored responses establish the supported
asset layout, not validation of arbitrary runtime descriptor bytes or a
promise that a larger index is safe. The full class-record extent is still
unproven.

All 123 shared actor resources expose 1,230 responses (4,920 source bytes),
with 54 distinct ten-response profiles across the five H13 banks. Corpus
checks compare each decoded value and span with its source bytes. Synthetic
checks cover high unsigned values, channel 9's boundary against `+0x38`,
nonzero allocation bases, aliases, and source preservation. Executable
checks pin the original default table and the consumer's WORD widths,
four-byte stride, and logical eight-bit shift.

## Selector 30 and 35 payload tails

Selector 34's `00433C00` additionally retains payload `+58` without reading
any tail operands. Its typed view preserves that allocation-relative address;
the later denominator/numerator reads belong to its runtime postinitializer.

The [specialized views](../../crates/inf-formats/src/constructor_actor_specialized.rs)
preserve additional proven reads without inferring complete record lengths.
Selector 30's `0040D950/0048B3C0` reads signed DWORD `+44`, three WORDs
`+48..+4D`, and full DWORDs `+50/+54`. These are distinct from its later
BYTE command overrides. The `0040D920` and `0042EDD0` wrappers reach the
same `0040D950` reads without `+50/+54`; `Family40d950Payload` keeps them for
selectors 15, 23, 37, 39, 47 and 49 ([the family](FAMILY40D800_RUNTIME.md)). Selector 35's `0043D1C0` retains the allocation
address at `+58` without reading its ratio WORDs, then reads DWORDs `+6C`
and `+70`. The parser does not execute scaling, clamping, static-table
selection or the later retained ratio calculation.

These fields are selected by the actual H13 class, not shared inheritance
alone. All spans remain absolute source offsets; the retained address remains
allocation-relative. Input gaps and original bytes are unchanged. Runtime
owners are documented in [selector 30](SELECTOR30_RUNTIME.md) and
[the 00428650 family](FAMILY428650_RUNTIME.md).

## 004236D0 family payload prefix

The [004236D0 family](FAMILY4236D0_RUNTIME.md) now has a typed `00423850`
prefix for selectors 8/9/10/21/36: retained address `+58`, DWORD `+68`,
WORDs `+6C/+6E` and DWORD `+70`. The common prefix stops at `+74`;
selector10/21's `00425290` additionally exposes the full DWORD `+74` through
`Family425190Payload` and requires the prefix through `+78`. This source
selects the later helper without truncating high bits. The parser preserves full switch inputs and WORD bits
without choosing static tables or reading the later retained ratio. Selector36
also retains the complete payload at runtime through `00482460`.

## Coverage and remaining boundary

All 162 installed resources have a supported dispatch profile. Of those,
**123 expose shared actor fields**, **35 expose flags only**, **two expose
terrain-light fields**, **one exposes retained selector-86 data**, and **one
exposes its specialized sixteen-pair table**.
These counts are tested across all
five H13 banks. **52** of the shared actor views additionally expose the
common `00426C10` extension and its sound arguments. These counts measure
bounded views, not fully decoded classes.

Required fields are checked against the main allocation before reads. Bytes
in relocation or entry tables cannot satisfy a truncated class prefix. Input
bytes are never patched, alias order stays intact, and payload lengths are
not inferred from neighboring entry pointers. Malformed and synthetic tests
cover negative model arguments, high unsigned health, unclamped raw fields,
unknown selectors, relocated point tables, and truncated prefixes.

Further class-specific extensions, embedded `+0x58` schemas, other resource
references, and the remaining classes' runtime constructor policy are open. H14 instance suffixes
and their recovered command views are owned by
[INSTANCE_ASSETS.md](INSTANCE_ASSETS.md). The next work should follow exact consumers. A shared initial
model and health field do not establish object behavior or completed world
spawning.

### Placed selectors the port does not construct

A census of 2026-10-12 joins every world's placements (resident inspection,
`--inspect-world 0` after activating `47,48,<leaf>` for leaves 11..43) to
REA's decompilation of the switch `004603B0`. The intro
(leaf 32) and, since the port builds the
[barrier](FAMILY40D800_RUNTIME.md#the-targets-life) (selector 69), Level 1
(leaf 28) place only selectors the port builds, as, since the family's
[trucks and assault craft](FAMILY4236D0_RUNTIME.md#selector-9-the-familys-own-class)
(selector 9), does Terra Nova (leaf 12), and, since the
[rotating terrain light](OBJECT_RUNTIME.md#selector-54-rotating-terrain-light)
(selector 54) and the [Force Field](SELECTOR18_RUNTIME.md) (selector 18),
does Stendahl Alpha (leaf 13). The [Sirens](FAMILY40D800_RUNTIME.md#selector-47-the-siren)
(selector 47) and the [Nemians](FAMILY4236D0_RUNTIME.md#selector-21-the-nemian)
(selector 21) are built too, as are the
[Powerball](ACTOR_RUNTIME.md#selector-57-the-powerball) (selector 57) and
the [Boulders](ACTOR_RUNTIME.md#selector-59-the-boulders) (selector 59),
so leaf 30 places only selectors the port builds.
Across the campaign 24 placed
selectors have no construction plan; the [ECM Unit](HOLD_ITEMS.md#ecm-unit-selector-73)
(73) is built; the Seismic Stabiliser (49) is
[built](TERRAIN_RUNTIME.md#near-strip-terrain-particle-emission), and so are
the [altitude ceiling](#the-altitude-ceiling-selector-48) (48), the
[timed spawner](#the-timed-spawner-selector-43-and-its-rocks-selector-44)
(43), the [Bomb](HOLD_ITEMS.md#bomb-selector-29) (29),
[Helniazine's zone](#helniazines-zone-selector-74) (74), its
[furniture marker](#helniazines-furniture-marker-selector-75) (75) and the
Alien Brain's [sinking patches](#the-sinking-patch-selector-83) (83); Kankutan's
placed laser mines (38) now load as the
[ACAM's mine](../../crates/inf-game/src/object_runtime/actor/behavior_resource/mine38.rs)
does: `004603B0`'s case `0x26` (`00460CA9`) is the resource factory's own
(`0x164` bytes, `00455560(2000)`, `+13A` = 2000, table `00525914`), and
their after-load `+34` is the shared `00401310`. The name is the H0 string at
payload `+40` where that word is one.

| Selector | Constructor (final vtable) | Placements | Leaves | Name |
|---:|---|---:|---|---|
| 40 | `0045B4A0` | 14 | 36, 40, 42, 43 | [banner](NETWORK_RUNTIME.md#the-banner-and-its-base), built |
| 41 | `0047A7A0` (`005267E0`) | 10 | 36, 40, 42, 43 | base, built |
| 45 | `0045BF00` | 29 | 35..37, 40..43 | [spawn marker](NETWORK_RUNTIME.md#the-network-game-in-the-session), built |
| 46 | `00475610` | 27 | 37, 41 | [territory](NETWORK_RUNTIME.md#the-territory), built |
| 70 | `0046E770` | 2 | 38, 39 | [race](NETWORK_RUNTIME.md#the-race), built |
| 71 | `0046F7F0` | 56 | 38, 39 | wrong-way marker, built |
| 72 | `0046F9E0` | 31 | 38, 39 | gate, built |
| 91 | `004027E0` | 1 | 35 | [Powerball's ball](NETWORK_RUNTIME.md#powerball), built |
| 92 | `00403EC0` | 4 | 35 | Powerball's goal, built |
| 94 | `004458E0` | 1 | 43 | missile launcher, built |

Selectors 40, 41 and 45 are mapped in the
[network notes](NETWORK_RUNTIME.md#the-banner-and-its-base): 40 is Capture the
Banner's banner and 41 its base. Selector 94's `+24` (`00445930`, table
`00524E20`) fires a [selector 25 missile](SELECTOR25_RUNTIME.md) at the player
every 1.5 seconds (`+44` counts down), unless the last of its zones (`+40`:
rectangles of a kind, a centre and half sizes) that holds the player is of
another kind than 1. The port builds it
([`selector94`](../../crates/inf-game/src/object_runtime/selector94.rs)):
its command 2 keeps the zones' operand (a count and a relocated address of
12-byte rectangles: world 33's are a wide kind-0 one and a kind-1 one
inside it), and each missile is `00458A50`'s.

Factories also spawn selectors 17, 29, 52, 55, 56 and 73 as
[hold items](HOLD_ITEMS.md); the placed mining rigs (selector 52) are
[built](ACTOR_RUNTIME.md#selector-52-mining-rigs).

### What each unbuilt class needs

A REA pass of 2026-10-13 read each constructor's final
vtable against the shared actor's (`00527C00`) and the visual base's
(`00527B68`), sized the methods it adds from the function census, and
checked their callees against the addresses the port's sources cite. Leaves
35 to 43 are the multiplayer arenas, worlds 25 to 33 (H0 `0xE0` + world):
Powerball, Four Play Heaven, Buggy Madness, Jet Race 1 and 2, Aztec
Encounter, Buggy Platforms, Fragtastic and Capture The Banner. They load
only in a network game, whose port is under way: selectors 40, 41, 45, 46,
70, 71 and 72 are built, the vehicle plays Buggy Madness, Capture the
Banner, Powerball and the races between machines, and 91, 92 and 94 are
built (retail's [DirectPlay layer](NETWORK_RUNTIME.md) is the port's own
transport). Every campaign
class is built; the last was the
[sinking patch](#the-sinking-patch-selector-83) (selector 83).

The campaign's end, selector 78, is [built](ALIEN_BRAIN_RUNTIME.md#the-director-selector-78).

What the recordings run of them
(census,
`unbuilt-classes.js`): Infest03 and Infest04 both visit Helniazine (leaf
16), where the Bombs update 92 and 82 times, selector 74 93 and 641 times
and the ECM Unit 93 and 574 times, and in Infest04 the player carries the
ECM Unit (its attach and detach `+8C` run 4 times and its hold `+90` 54
updates), which is why the ECM Unit is now built. Nothing else there is
touched: selector 74's zone never retires an actor (its `0047CE90` calls
at `0042D5A9` and `0042D68A` never run) though its `+2C` draws 101 times
in Infest04; Infest03's Bombs stay at rest, and in Infest04 a laser mine
[sets them off](HOLD_ITEMS.md#bomb-selector-29). The Bombs and the
[zone](#helniazines-zone-selector-74) are now built too.
Both recordings build Artoria's Pursuit Ship and altitude ceiling, which
never update. Infest01 and Infest02 meet none of them. The Pursuit Ship is
now [built](#the-pursuit-ship-selector-61).

Apart from each class's own helpers and the ending, every callee is one
the port's sources already cite; still, none is a short port. The Force
Field, in eleven campaign worlds, was the largest, and is now
[built](SELECTOR18_RUNTIME.md).

### The sinking patch (selector 83)

Leaf 29 (the Alien Brain's world 19, and world 0 on its map) places two
patches of ground that sink below the water plane and rise again. Case
`0x53` of `004603B0` allocates `0x7C` bytes and runs `0043C300`: the
rotating visual base `0047A490`, vtable `00522570`, and `+6C`, `+70`,
`+74`, `+78` zero. The table is the rotating base's (`00527BB4`) but for
the destructor `0043C320` (it frees `+70`), the payload reader `+0C`
`0043C3B0` (the context `+08`, then `0043C3A0` keeps the payload at `+78`
before the flags `00479D70`) and the update `+24` `0043C3D0`. Both
placements carry the same payload: extent 1536 by 2560 (`+4`, `+6`),
depth 200 (`+8`), travel 2500 (`+A`), risen 5000 (`+C`) and sunk 2000
(`+E`), the times in steps of 1,024 µs.

The update adds the delta to the timer `+6C`, takes its steps (`SHR 10`)
and runs the state `+74`; a new state restarts the timer:

| State | Does | Then |
|---:|---|---|
| 0 | `0043C4C0` saves the cells | 3 once saved |
| 3 | sinks: `0043C620(steps * 65536 / travel)`, or `0043C620(65536)` once the steps reach the travel | 4 at the travel |
| 4 | waits sunk | 1 at `+E` steps |
| 1 | rises: `0043C620(65536 - steps * 65536 / travel)`, or `0043C620(0)` at the travel | 2 at the travel |
| 2 | waits risen | 3 at `+C` steps |

The cells are the extent over 128 along each axis (toward zero), from the
position less half their span. `0043C4C0` frees any earlier save, sets
128-unit addressing (`00450350`: `005372F0` = 0x80, `005372F4` = 0x7F)
whatever the terrain detail, keeps the low byte of each cell's H12 WORD,
row by row along X (an `operator new` of the count), and restores the
addressing for the detail (`00450370`). `0043C620` does nothing before a
save or without H12. Its target is the water plane (`[0055EE3C]->[0] >>
[005483BC]`, a signed WORD) less the depth. Between the ends (a fraction
neither 0 nor 65536) the patch moves only while the world camera
(`00478E00`) is within 8000 (the squared WORD differences, unsigned,
against `0x3D09000`). Each cell then becomes the saved height (the byte
sign-extended, `<< [005372F8]`) plus the difference times the fraction,
`SAR 16`, through `00450890` at 128-unit addressing.

The port builds the class
([selector83.rs](../../crates/inf-game/src/object_runtime/selector83.rs))
and updates it on the world tick (`WorldPatchHost`); its terrain writes
use `TerrainRuntime::write_raw_height_128`. In the lab
(`patch-world19.json`, with `patch_update` and `patch_move` logging each
`0043C3D0` and `0043C620` and a dump of the first patch's first row of
cells), retail's first patch saves on tick 1, is sunk on tick 22 with
every cell of that row at byte `AB`, rises from tick 39, is back on tick
60 with every byte restored, and sinks again from tick 101; with the
camera beyond 8000 nothing moves between the ends.
`a_patch_keeps_retails_ticks` runs the port's patch through the same 201
ticks and finds the same states, bytes and ticks, and `run.py both`
matches every tick and vehicle update over 201 ticks. Its `patch_update`
hook runs at the update walk's depth and changes the spawner's unwritten Y
([below](#world-19s-spawner-selector-81)), so retail's spawns in that run
are the hook's.

### The Pursuit Ship (selector 61)

The port [builds it](../../crates/inf-game/src/object_runtime/actor/selector61.rs):
case `0x3D` of `004603B0` allocates `0x150` bytes for `004128C0`, the shared
`0047A7A0` and then `+140 = 0`, `+144 = (0048F060 & 0x7F) + 0x50` (the gap
it keeps to its target), `+148 = 0`, WORD `+14C = 0` and at `+138` the
weapon behavior `0048CC70(2, 1, 0, 0052F748)` with `+04 = 10000`,
`+08 = 150000` and `+0C = 1`. Its table `00520700` is the shared actor's but
for `+00` (the destructor `00412990`: the target `+148`, then the behavior's
`0048CD60`), `+10` (the custom vertex `00413060`: low byte 2 publishes model
row extra 0, which `0048CF70` first captures into behavior group extra 1;
any other, the ground under it), `+24` (the update `00412A10`), `+40`
(`00413120`, which the Boulders share), `+7C` (`00413130`, the behavior) and
`+AC` (the death `00412FA0`: visual `0x37` through `+4C`, sixteen type-1
debris round a type-1 centre, the radius block `00531250` unless flag bit 31,
the type-1 explosion `0048A0F0`, then disposal). BYTE `+13C`, the bank's
direction, is constructor residue, a fresh-heap carry.

The update does nothing without a target but roll, one update in four
(`0048F060`'s low WORD under `0x4000`), for the first player's unit
(`0046B030`): within 2000 and inside the cone ahead (local z above zero,
x² + y² under the squared distance) it becomes a follow target (`004825D0`).
With a target it refreshes it (`+18`): a dead one turns it to climb at 2600
toward (0, 2000, 0); within 2000 it wants the target's forward speed (its
velocity `+1A` along its basis row 2) plus twice the gap past `+144`, which
a target faster than 1500 widens by half its excess, held to 50..2600;
farther off it wants `+14C`. Yaw and pitch each turn at most delta >> 4
(`0048EF30`, `0048F520`, `0x2000` for a target behind) and the larger goes in
at half. Its forward speed rises toward the wanted one by
`(delta * gap << 13) >> 32` or drops to it at once; the bank `+140` swings
by `(delta >> 4) * 500 >> 15` between plus and minus 500, and it rolls to
`(bank << 12) / 500` less the roll it has (`0048F4D0` of its basis `+4C`).
The bank, its sixteenth and the speed are its velocity in its own frame
(`0047A6C0`), then `0047B1E0(delta, 8)` moves it, and ground above it
retires it (`0047CE90`). Its weapon ticks (`0048D030`), and within 750,
ahead, the entry ready (`+14` zero, `+0` set, `+1C` under 6,000,000) and
(x² + y²) / 4 under the squared distance, it fires (`0048D340`, lead
published). Its shots leave from its draw, as every behavior's do.

Artoria (world 11) and the Defence Satellite (16) now match retail's
fresh-heap runs in every logged kind and every tick's RNG steps from a
start block: their selector-60 spawners make ten and seven Pursuit Ships
at tick 13, and the ships roll, chase and fire as retail's do.

### The Alien Brain's attack (selector 62)

The port [builds it](../../crates/inf-game/src/object_runtime/actor/selector62.rs):
case `0x3E` of `004603B0` allocates `0x178` bytes for `00463F60`, the shared
`0047A7A0` and then `+138 = +13C = 0`, `+14C = +150 = 4000`,
`+158 = +15C = 0`, BYTE `+160 = 1` and the five ribbon slots
`+164..+174 = 0`. Its table `00526DC8` is the shared actor's but for `+00`
(the destructor `00463FF0`, which deletes the target `+15C` and leaves the
ribbons), `+24` (the update `00464050`), `+28` (the light `004647E0`) and
`+90` (`00464930`, the update for a holder, answering 1). The Alien Brain
(selector 85, case `0x55`, `0x19C` bytes through `00405F10`) builds on it
and is [built](ALIEN_BRAIN_RUNTIME.md#the-brain-selectors-85-and-62): the
port keeps the brain's fields beside the attack's and runs the brain's
update `00405FC0` in place of `00464050`. Foldrak Noir (world 14) places
three attacks on their own.

The update steps `+138` through four states on the durations at
`00539808` (3, 2, 2 and 2 s). Entering state 1 it sets flag 8 and clears
`0x40`, takes the first player's unit (`+C4` 3) within 3000 across the
ground as a follow target (`004825D0`), or else a fixed point at itself,
aims an offset of `+14C` along the line from the unit (its length an `f32`
under the CRT's 53-bit precision), lifts it by `+150` and raises five
ribbons (`00486600`, each `+44 = 1`), which then swing round the target
point, rising with the square root of the clock left (`0048EFC0`).
Entering state 2 clears the ribbons' `+40` and sets the blast clock `+154`;
the ribbons fall to the point, and each 500 ms resource `0x38` goes off
there (`00460310`) with the next of the four radius blocks at `00539820`
(`004887D0`) and the world flash at the attack (`00487D70`, `0xF00`).
Leaving state 2 sets off resource `0x36`, the blocks left and a flash;
entering 3 disposes the ribbons and deletes the target, and entering 4
clears flag 8 and sets `0x40`. From state 4 a clear flag `0x80` starts
again at 1; a set one holds state 4, restarting its clock each update. A
target that dies in states 1 and 2 becomes a fixed point where it was. The
light (`0045B1A0`) fades with each state's clock.

Without the unit, the aim reads a stack slot the update never wrote
(`004642B4`, beside the delta's low WORD). The brain's attack never has the
unit; there the lab found the world object's address in the slot, and the
port takes its low WORD ([the brain's aim](ALIEN_BRAIN_RUNTIME.md#aiming-without-a-unit)).
A placed attack still holds there (`ResidueDirection`). Foldrak Noir's three attacks now match retail's
fresh-heap run in every logged kind and every tick's RNG steps from a start
block: each blasts five times from tick 26, and again from tick 102.

Foldrak Noir's attacks (placements 13, 30 and 49) guard it until the
Defence Satellite falls: each carries command 14 on grant 0x35, "Defence
satellite captured" (H0 `0x216`), so the load disposes of them once the
first player owns it. The Defence Satellite's hangar grants it when a
scientist is dropped off in it, or when it dies (26.ovl placement 40,
selector 39, `+7A` 0x35;
[the hangar](FAMILY40D800_RUNTIME.md#the-hangar-takes-a-scientist-0043bea0)); its gate at
placement 30 asks for it (command 32), and Foldrak Noir's gate pair at
start 3 swaps on it (placement 52 without, command 14; 53 with, command
13). Complete Level does not reach the target, and retail's run of it on
the Defence Satellite grants no 0x35. Within 3000 of an attack the first
blast kills the buggy: from start 2, 1,050 from attack 13, retail loses
five buggies (ticks 26, 102, 178, 254 and 330) and the loss limit's
countdown ends the world into itself at tick 395, Mission Failed; the
port matches it in every compared event
(`runtime_re/lab/scenarios/foldrak-attacks.json`), and with the grant
owned (`foldrak-attacks-captured.json`) neither has an attack or a death.
Each blast's wave, resource `0x38`'s model 112, draws through
[a register seed](MODEL_RUNTIME.md#explosionwavemodel112stackhistoryapproximation);
before it the port omitted every wave in view.

An attack chooses its target only as it enters state 1, once every 9 s
(`004641BD..004641FE`: the first player's unit `0046B030` at `+C4` 3,
WORD differences in X and Z, the squared sum below `0x895440` unsigned).
A unit that comes within 3000 later is struck from the next cycle on, and
a target once taken is followed wherever it goes until state 3. In the
lab (`foldrak-attacks-cross.json`) a buggy started 4,200 from attack 13
drives in to 2,270 from it: the first cycle's blasts (ticks 26 to 43)
strike the attack's own spot, and the next cycle's (ticks 102 to 118) land
on the buggy as they push it about, in retail and the port alike in
every compared event. In a session of 2026-10-21 the ACAM was
never within 3000 of an attack as one aimed (6,783 at the closest, 69
aims), so all 273 blasts struck the attacks' own spots, which nothing
marks.

### World 19's eyes (selector 79)

The port [builds them](../../crates/inf-game/src/object_runtime/actor/derived/selector77.rs)
as selector 77s with a pair's fields: `00408740` runs selector 77's
`00407410`, then `+1C4 = +1C8 = +1CC = 0054E894`, `+1D0 = 0` and BYTE
`+1D4 = 1`, table `0051FC8C`. Its own virtuals are `+04` (`00408880`: codes
through 18 are selector 77's `00426030`, 19's DWORD names the partner's
placement), `+24` (`00408AB0`), `+28` (`00408790`, a light of radius `0x200`
coloured by three sines of `+19C` a third of a turn apart, each `>> 12`),
`+34` (`004088C0`) and `+98` (`00408A70`: message 3 makes it a follower,
message 2 has a follower strike); the rest are selector 77's, its `+58`
(`00404610`) answering itself and `+D0` (`00402790`) its target `+148`.
World 19 places ten, in five pairs about 1100 apart (placements 6/7, 8/9,
51/52, 89/90 and 94/95), each naming the other.

The after-load resolves the partner (`0047F7B0`; an eye without one
disposes itself). The first of a pair to run it leads: it faces across the
line to its partner (`00490370` with the world's up, the forward
(-dz, 0, dx) over its `f32` length), keeps the midpoint `+1B4` and three
quarters of the distance `+1BA`, and makes the partner its follower.

The leader aims for the first player's unit as selector 77 does
(`00407600`, then `+198 = 333333` when it finds none); the follower copies
the leader's target each update (`+148`'s `+04`), and leads itself once its
partner is gone. With a target the leader keeps its handle `+1C8`, resetting
`+1C0` and `+1BC` when it changes, and measures the target in its own frame
about the midpoint (`0047A660`): within `+1BA` across and 1000 up or down, a
change of side (local Z) arms the strike `+1D0`. A second later `00408DA0`
draws a line of type-12 trail particles (`00489D90`, one per 50 of the
distance, 1 to 20 and both ends) from each eye's origin `+192` to the target,
moving at the line plus the target's velocity's share, and the leader deals
20000 on channel 2 (`0047C6E0`); the clock cools to 7.5 s. Ten seconds
aimed at one target (`00407760`'s alignment) strike it too. Without a
target an eye idles as selector 77 does (`00407B00`).

Its `+1BC` and `+1C0` are constructor residue, fresh-heap carries; no
recording reaches world 19. Worlds 0 and 19 now load as retail's do, and
their RNG steps match retail's to tick 33, where selector 81 first spawns.

An eye dies (`00408560`) into selector 82, a boulder with a 5 s fuse that
nothing places; see [the eyes' death](ALIEN_BRAIN_RUNTIME.md#the-eyes-death).

### World 19's spawner (selector 81)

The port [builds it](../../crates/inf-game/src/object_runtime/selector81.rs):
case `0x51` of `004603B0` allocates `0x5C` bytes for `00409250`, the base
object `00479C60` and then `+40 = +50 = +54 = +58 = 0`, `+44 = 5`,
`+48 = 1000`, `+4C = 20000`, table `0051FE44`. The table is the base
object's `00527B24` but for the destructor (`00409290`, which frees `+58`),
the commands `+04` (`00409320`: 2 sets the member count `+44`, 3 the
distance `+48`; through 1 the base object's, above 3 refused), `+1C`
(`00409360`: `+58` gets `+44` DWORDs it never writes, a fresh-heap carry)
and the update `+24` (`00409390`).

The update keeps the first player's unit in view: each time the unit gets
farther than `+48` (`00462DA0`) from the spawner's position, the spawner
moves onto the unit and `+4C` becomes its speed, `(distance << 10) /
(+50 >> 10)` or the distance times 1000 within 2 ms; five seconds without
such a move set `+4C` to zero. When `+40` runs out and the unit is slow
(`+4C` under `+48`), the last missing member (a `+58` handle that no longer
resolves) gets an attempt: three draws pick a bearing bucket from the
percentages at `0052EDC0` (15, 35, 45, 50, 55, 65, 85), the turn within the
eighth and a range of 500 to 1011; the offset is turned by the unit's
basis (`0047A6C0`) and moved by its velocity. The point must lie more than
32 below the liquid (the highest of its cell's four corners against
`[0055EE3C]` shifted by `005483BC`), 256 from every member and 1300 from
every objective (flags `0x28` of `0xA8`, a kept `+54`); then
`00460310(0x86)` raises a member (selector 80) and its handle goes into
`+58`, and the next attempt waits `((draw & 0xFFFF) + 31250) << 5` (or
`+ 78125) << 7` should the aliens' node count `00547620` read zero by
then). With that count at zero before the spawn the spawner disposes
itself instead. The retry flag is the delta
argument's own slot: only a delta whose low byte is zero allows a second
attempt (up to eight).

The first attempt's offset has no Y: `0047A6C0` reads the stack slot as
the update before left it, and the unit's tilt carries it into the point.
In world 19 retail read 1 and 0 on alternate ticks, then 10241 (`0x2801`)
once eyes stood near the spawner in the update list, so the slot belongs to
whichever object updates just before the spawner. The port starts the
word at 0 (`SPAWNER_FIRST_HEIGHT_APPROXIMATION`, 2026-10-18), retail's
while it is 0 or 1: in the lab (`spawner-y-world19.json`, only
`local_to_world` hooked) retail's first 200 ticks read 0 and 1 and raised
no member, and the port, with the patches built, matches retail in every
compared event and every tick's RNG over 200 ticks (with only the
comparison's default and `vehicle_update` hooks). A hook at the update
walk's depth can change the word: with
`patch-world19.json`'s `patch_update` hooked, retail's attempts landed
thousands of units away in the liquid and raised members from tick 66.
Once the word is 0x2801 the port's attempts part from retail's.

### World 19's members (selector 80)

The port [builds them](../../crates/inf-game/src/object_runtime/actor/derived/selector77/update/member.rs)
as selector 77s with three fields of their own: `00408FC0` runs selector
77's `00407410`, then `+1BC = 1`, `+1B4 = +1B8 = 0` and table `0051FD68`,
which is selector 77's but for `+24` and `+D4`.

The update `00409070` keeps a state `+1BC`: rising (1), up (0) and sinking
(-1). Its first rising update puts the member a radius (`+28`) under the
liquid (H12 slot 0's first DWORD shifted by `005483BC`) and sets `+1B8`;
later ones raise it `delta >> 11` until it reaches the liquid, where it is
up. Sinking, it goes down as fast, and it disposes of itself (`00479FA0`)
when it is not drawn near (`+2C` bit `0x2000000`) or its origin `+194` lies
100 under the liquid. Then selector 77's update runs (`00407CA0`), and
ground above the liquid under it (`00450A30`) retires it (`0047CE90`). Up,
`+1B8` counts its time and `+1B4` its time without a target (`+148`); past
15 s, or 5 s without a target, it sinks. The update that brings it up
already counts. Its `+D4` (`00409010`) first picks a farther reach when
`+1A4` is zero: half a draw less the unsigned extension `+190`, plus
`0x2000`, and a speed as selector 77's does, before `004080C0`.

check_selector80.py runs `00409070` over 61
states, with selector 77's update native (state 2's status, and state 3's
idle through the member's `+D4`), the ground's height supplied and the
retirement and disposal recorded; the port matches every one. A world 19
test raises a member through the factory where the ground lies deep under
the liquid: it rises to the surface, gives up after five seconds with
nothing to aim at, and sinks away.

### The altitude ceiling (selector 48)

The port [builds it](../../crates/inf-game/src/object_runtime/selector48.rs)
as it builds selector 50: `0x4C` bytes over the visual base (`0047A030`),
whose table `005225D0` differs from the visual base's only in the destructor,
the commands `+04` and the update `+24`. Its constructor `0043C7F0` zeroes
`+44`, `+46` and `+48`, and the world loader now places Artoria's and the
Defence Satellite's. The update is decoded against the unchanged executable:
check_selector48_update.py runs
`0043C8F0` over supplied units (15 cases, 22 shots,
`selector48_update_native.json`). Its commands (`0043C840`) set the ceiling
`+44` (3), the radius `+46` (4) and the relative flag `+48` (5); 6 disposes
it when the first player owns the grant (`004752E0`), and higher codes are
refused. While the first player's unit (`00488AA0`, `0046AE60`) is inside
the radius, if one is set (a signed 3D distance), and above the ceiling,
each update fires one shot for every `(margin * 5 + 20000) * 2` of its
time, rounded up (none when no time passes), the margin being the ceiling
less the height plus 2000, at least 1; then it sends hint 0x2E
(`0046C760(0x2E, 0)`). Each shot takes seven process RNG draws (`0048F060`):
it starts within half of `max(4 * margin, 2000)` of the unit across and
`margin / 8` plus up to 1000 above it, and flies at 4000 (an f32
normalization) toward the unit led by a fortieth of its velocity, within
half the margin, as particle kind 2 or 5 (`00466710`). Infest03 and
Infest04 both build Artoria's (leaf 21, placement 104), but its first
command, 6 with grant `0x30`, disposes it at once
(ledger),
so no recording runs the update. The port's update matches all 15 cases,
every shot's kind, start, velocity and RNG word at the factory call, the
hint and the final RNG. Artoria's ceiling is absolute (-1700, no radius);
the Defence Satellite's is relative (-1800 below its own 2977, so above
1177), and a start without the grant lies above it.

### The timed spawner (selector 43) and its rocks (selector 44)

Luna Azine (leaf 18), Shak'tahh (leaf 27, three) and Caradion (leaf 34)
place the timed spawner. The port
[builds it](../../crates/inf-game/src/object_runtime/selector43.rs) over
the visual base, as it builds selector 48: `004541C0` (100 bytes, table
`00525730`) differs from the visual base `00527B68` only in the
destructor, the commands `+04` (`00454220`), the update `+24` (`00454310`)
and the network slot `+40` (`004542E0`), which only a network game
reaches. Its commands set the on and off phases (codes 3 and 4, WORD
seconds times 1,000,000, at `+44` and `+48`), the spread (5, `+4C`), the
ring (6, BYTE `+50`) and `+51` (7, the DWORD's truth); codes up to 2 are
the visual base's.

The update alternates the phases. When `+54` runs out (unsigned, below the
delta) it gains the next phase's length and `+5C` clears, and a phase
turning on raises event 0xE with its length to every actor: `0047DAF0`'s
zero-radius arm, with no sender, which sets the Sirens' alarm. In the on
phase `+5C` counts the time since the last rock, and a rock is due when
the low byte of a process RNG draw (`0048F060`) times `+5C`, its own low
byte cleared, passes `0x493E000`. The next draw is the angle. A ring
spawner (`+50` set) throws the rock from 256 around itself, 256 above the
ground cell there, outward at 0x400 and up at a third draw's low 11 bits.
Otherwise the rock starts within twice the spread (a third draw), moved
along X by the static `005377B0`'s fall (0x6000), 0x3000 above the ground,
and falls at (-0x400, -0x200, 0). Each rock is resource 0x18 (`00460310`
with the process NULL), spun by `005377C8`'s local (0x400, 0x400, 0x400)
(`0047B4B0`) and set moving (`0047DDF0`).

The rock is a shared actor ([selector 44](../../crates/inf-game/src/object_runtime/actor/selector44.rs),
`00453E70`, `0x140` bytes, table `00525648`). Its first update keeps
gravity only for a rock thrown upward (`+90` above zero, mask 0x19, which
sets `+139`); any other gets 0x18, so a falling rock, or a ring rock
thrown flat, moves in a straight line. Each update runs the physics with
that mask and smokes (`0047DFD0`). The ground (`+94`, `00458C30`), an actor
(`+A4`, `00454180`) or furniture (`+A8`, `00454190`, which ends the
furniture walk) retires it (`0047CE90`), as message 5 does (`00454160`).
Its death blasts the blocks `00537688` and `00537698` from its position
with its own handle unless flag bit 31 is set, bursts sixteen type-4
debris round a type-2 centre (radius 20, speed 8), builds the visuals 0x36
and 0x37 through `+4C` and disposes of it. It lights the ground below it
in `005483FC`'s (110, 76, 32), which its module start `00453E30` sets
when WinMain's game module (`00438E10`) acquires every object module:
each channel shifted up 10 is divided by the square of its height above
the higher of the ground and the water plus 0x400, so the glow shows only
when the rock is below the surface. It draws through the fog: `00454000`
sets the context's fade bounds both to 0x3FFF around the common draw
`0047D910`. Its `+74` answers itself (`00404610`), the only represented
table whose does, so a turret on scanner B (selector 11's command 22) can
now find rocks; filter B scores a candidate the turret's DWORD `+164` less
the root of the squared distance (`00427620`), and the strictly best
score wins.

check_selector43_update.py runs
`00454310` with the real RNG, sine table, static and H12 cell read over 22
update sequences (713 updates, `selector43_update_native.json`), and
check_selector44.py the rock's bodies over 111
cases (`selector44_native.json`). The port matches every update's event,
rock (start and the RNG at the factory call), spin, velocity, timer, phase
and RNG; the rock's masks, message replies, death order, all 84 glows and
the draw's fade bounds. The contacts only retire, which the world's
contact pass does. The death runs inside the lifecycle, whose child
factory now goes without the world start and the camera settings: only
the gate (selector 30) and the camera (selector 16) read them, and a
factory without them refuses those two. No recording visits the three
worlds. In Luna Azine and Shak'tahh the first eruption sets the Sirens'
alarm before their first update, which then
[depends on heap residue](FAMILY40D800_RUNTIME.md#selector-47-the-siren).

### Helniazine's zone (selector 74)

Helniazine (leaf 16) places one. The port
[builds it](../../crates/inf-game/src/object_runtime/actor/selector74.rs):
case `0x4A` (`0046157D`, `0x158` bytes) runs the shared `0047A7A0`, then
`0042D550` sets the table `00521764` and clears the link byte `+148` and
the link count `+14C`. The table is the shared actor's but for the
teardown `+00` (`004110E0`, the same `0047AA60`), the commands `+04`
(`0042D6F0`), the initializer `+0C` (`0042D760`), the finish `+1C`
(`0042D950`), the update `+24` (`0042D570`), the draw `+2C` (`0042D7D0`)
and the messages `+98` (`0042D6B0`).

The zone is a box in its own frame. The initializer runs the shared one,
then `0042D780` takes the half height `+150` from the payload's WORD `+44`,
and the half width `+152` (also `+28`) and half depth `+154` from its
DWORDs `+48` and `+4C`, halved toward zero. Commands 16 and 17 halve a WORD
into the width and the depth and 18 stores the height; codes through 15 are
the shared actor's, and any other code reads its WORD and refuses. The
finish, in place of the shared `0047AAD0` (so `+74` keeps the
constructor's 0), sets the sphere through the box's corners, its squared
radius `+138` (the DWORD sum of the extents' squares) and its root `+28`
(`0048EFC0`), the circle `+140`, the root of the width's and depth's
squares, and `+144`, their sum over 800 (signed), the draw's points.

Each update adds `delta >> 6` to the timer `+13C`. A linked zone (`+148`)
with no links left retires itself (`0047CE90`). Otherwise, over the actor
list, each actor whose `+68` answers is retired when it lies inside the
sphere and, turned into the zone's frame (`0047A660`), strictly inside the
box. Only the `00455560` family's tables answer `+68` (`00404610`): the
grenade, charge and mine, the missiles, the Bomb and the implant; every
other actor table binds the null `00401300`. Retirement is the
lifecycle's state 1, so the zone sets off what is thrown or fired into it:
a grenade, charge or mine goes off at its next update, a missile or a Bomb
at once, and an implant ends (`00458E40`). The world's
[host](../../crates/inf-game/src/session_platform/world_actor_update/zone.rs)
runs those deaths through the missiles' states. The draw draws no model:
at each of the `+144` points round the circle, turned by the timer, it
clamps the offset to the box, carries it into the world (`0047A6C0`), sets
it 0x32 over the ground (`00450A30`) and births particle type 2
(`00489A90(2, point, 0)`). Message 7 sets `+148` and counts a link, 8
takes one; the zone answers 0.

On Helniazine a course target holds it: the generator at (35045, 288,
43121) (selector 39, resource 19, grant 0x20) links it from its after-load
(`zone-life.js` `link()`, at
`2838FC` in Infest03 and `6825F5` in Infest04), and that generator's death
(`0043B910`) sends 8, after which the zone retires at its next update.
Both recordings build it the same way (`build()`, Infest03 from `2838E6`):
at (34016, -960, 23433), the payload's 256, 4000 and 2000, then commands
16 (1000) and 17 (600), give the half extents 256, 500 and 300, the sphere
`0x63020`, the circle `0x247` and one point. Neither recording destroys
the generator or sends anything into the zone.

check_selector74.py runs its bodies over 66
cases (`selector74_native.json`) with the real transforms, square root,
sine table and H12 height, and the port matches every one: the commands'
extents and answers, the payload reads, the finish, the messages, the
update's 23 retirements over grenades and shared actors at three turns,
and the draw's 28 births. The world tests build Infest03's zone from
Helniazine's data, set off a grenade thrown into its middle, retire a zone
whose link drops, and, from each recorded timer, draw all 111 of the
recordings' points (`draws()`: 10 in Infest03, 101 in Infest04) where
retail drew them.

The timer's start is open. Nothing writes `+13C` before the first update
adds to it, so retail's starts from allocator residue: Infest03's block
held 0 (an earlier object's store at `004286DF`) and Infest04's
`0x3B800000`, a float an earlier object left (`0050A279`), whose low WORD,
all the draw reads, is 0 as well (`timer()`). The timer is one of the
fresh-heap carries (`FreshHeapZeroExtendedCarryApproximation`, the
world loader's policy): it starts at zero, which gives both recordings'
draws, and world 6 then matches retail's fresh-heap run in every kind. A
strict arena leaves it unknown: the update advances a known timer only,
and the draw holds (`Selector74ZoneTimer`, logged once per zone).

### Helniazine's furniture marker (selector 75)

Helniazine places one more: a base object (case `0x4B`, `004352F0` over
`00479C60`) whose table `00522008` differs from the base's `00527B24` only
in the teardown `+00` (`00475400`, the same `00479D00`), the finish `+1C`
(`00435310`) and the world cleanup `+30` (`00435360`). The constructor sets
the kind `+40` to `0x0E`, the fence segments', and the index `+44` to
`0xFF`. The port
[builds it](../../crates/inf-game/src/object_runtime/selector75.rs) as
the base object with those three bodies. It keeps a destroyed fence down
when its world loads again.

- **Finish `00435310`.** A placement word `+2A` of -1 disposes it.
  Otherwise `+44` takes the low byte of the world's placement serial
  (`0048EB90`, `[G+1B0]+3D0`, which each loaded actor's `0047F6C0` also
  takes), and when the save has that completion bit (`0048EA80`, the signed
  index into the current world's four words at `W+240`) it removes the
  chain at its position (`00437110`).
- **Cleanup `00435360`.** Leaving the world it runs the same removal; when
  that takes nothing, it marks `+44` (`0048EAD0`, force 1, so the cleanup's
  inhibit `W+08` cannot stop it). It never reaches the common `0047EC50`.
- **The removal `00437110`.** Under the temporary fine spacing
  (`00450350`, then `00450370` rereads settings `+70`),
  [`00437150`](../../crates/inf-game/src/world_furniture_effect/marker.rs)
  searches squares round the point, `0x80` a side larger each time out to
  `0x200` (X outer, Z inner). A flagged cell reads its sparse record, an
  unflagged one the process record `005483C4`; a record whose H11 entry
  has the kind loses its bytes `+2` and `+3` and counts. A square's first
  find becomes the next point, and every later find in it starts a search
  of its own there; after a square with a find the search starts over from
  the next point, and after the `0x200` square without one it ends. So it
  follows furniture up to four cells apart and returns how many pieces it
  took.

In both recordings the marker stands at (39350, -1216, 19947) by the pad's
fence and takes serial 41 (`2838EB` in Infest03, `6825E5` in Infest04,
`marker-life.js` `marker()`).
Both players brought the fence down: by Helniazine's cleanup
(`34BA7B`, `882435`) every segment record near the marker is clear
(`cells()`; Infest03's were cleared at `2E55EC..2E55F0` by the furniture
clear `00436E00`), the removal takes nothing, and the cleanup marks bit 41
with force 1. Neither recording loads Helniazine again, which would take
the fence down at the finish.

check_selector75.py runs `00437150` whole over
13 supplied H12 entries (lines one to five cells apart, a branch, other
kinds, gaps, the fallback, wrapped cells) and the finish and cleanup with
the real `0048EB90`, `0048EA80` and `0048EAD0` (16 cases,
`selector75_native.json`); the port matches all 29. The marker's finish
runs in its construction; its removal writes the world's terrain, which the
loader lends right after the marker's placement, before the next, so the
order is native. The world tests build Helniazine's marker with serial 41,
load a save with bit 41 with the pad's 30 segments already removed, and
leave the world: with the fence standing the cleanup takes the 30 and
marks nothing; with it gone it marks bit 41, as both recordings did.

