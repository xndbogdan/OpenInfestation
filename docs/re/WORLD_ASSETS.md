# Infestation world asset decoding

This document owns the bounded semantic interpretation of Handler 13/14 and
the remaining related data-table investigation. Binary framing remains in
[FORMAT_DOCUMENTATION.md](FORMAT_DOCUMENTATION.md); priorities belong to
the Infestation objective.

The canonical decoder is
[world_assets.rs](../../crates/inf-formats/src/world_assets.rs), with
[complete retail corpus checks](../../crates/inf-formats/tests/world_assets_corpus.rs).
It exposes authored constructor prefixes and placement graphs.
[CONSTRUCTOR_ASSETS.md](CONSTRUCTOR_ASSETS.md) extends the H13 prefix with
proven initial model bindings and shared payload fields. Neither decoder
constructs live objects or completes per-class payload decoding.
[INSTANCE_ASSETS.md](INSTANCE_ASSETS.md) owns the ordered H14 command lists
and bounded class-specific views. [WORLD_INSPECTION.md](WORLD_INSPECTION.md)
joins these assets through actual resident resource registrations.

## Resource identities and evidence

The supported retail executable's table at `0053AA14` contains these handler
objects. Their `+4` words point to the registered global-slot arrays; `+8`
holds the pool capacity. Global resource slots, local registration indices,
OVL leaf IDs, campaign selections, and constructor selectors remain separate.

| Handler | Object | Slot-array pointer | Primary consumers |
| --- | --- | --- | --- |
| 5 | `0055EDC0` | `0055EDC4` | `00437CA0`, loader `0050F340` |
| 10 | `0055EEB0` | `0055EEB4` | `00470170`, `00471A80`, `00471AB0` |
| 11 | `0055EEF8` | `0055EEFC` | `004508E0`, terrain furniture submission |
| 13 | `0055ED90` | `0055ED94` | `00460310`, `00460120`, `004603B0` |
| 14 | `0055EDD8` | `0055EDDC` | `0048DC30`, `0048E170`, `0048EDE0` |
| 15 | `0055EDF0` | `0055EDF4` | `0044FDD0`, `00465070`, `004650E0` |

Source is in bulk/game_logic.c,
bulk/fgdk_engine.c, and
resource_handlers.c. Ghidra omits register arguments
at important calls. The H13/H14 contracts below are additionally checked
against original executable disassembly, especially `00460310..004603AE`,
`00460120..004601DE`, `0048DEB9..0048DF39`, and `0048E1BA..0048E269`.
No new runtime recording is needed to establish these file layouts.

## Handler 13: common constructor prefix

There are 162 registered resources in five base-phase banks: `1.ovl` has
131, `6.ovl` has one, `29.ovl` and `30.ovl` have twelve each, and `32.ovl`
has six. They contain **85 distinct constructor selectors**.

| Offset | Representation | Proven use |
| --- | --- | --- |
| `+0` | unsigned WORD, stored tag | `00460310` increments the WORD with 16-bit wrapping; `004603B0` dispatches on that incremented selector |
| `+2` | unsigned WORD | `00460120` zero-extends it into the initialization context passed to object virtual `+0C` |
| `+4` | start of class-specific bytes | `00460120` passes this address as the other virtual `+0C` argument |

`00460310` resolves the H13 resource using its argument's signed low WORD.
It does not use the constructor selector as a resource slot. The constructor
switch creates different object classes and allocation sizes. The bounded
[constructor views](CONSTRUCTOR_ASSETS.md) recover dispatch profiles,
155 initial model bindings, 123 shared actor payload prefixes (including
initial health), 32 flags-only prefixes, two terrain-light prefixes, one
selector-86 prefix, three selector-83 prefixes, and one specialized sixteen-pair
table. The remaining specialized
payload and class extensions, runtime behavior, and relationships remain open.

`ConstructorResource` retains the local index, entry-table word span, main
allocation offset, exact four-byte prefix span, stored tag, incremented
selector, second argument word, and class-data offset. The bank retains the
complete original main-payload span and relocation records. There is one
internal relocation in the full H13 corpus.

No payload length is inferred from the next entry pointer. Native entry
tables can alias or order resources independently of their allocation
positions, and referenced suffix data can live elsewhere in the allocation.

## Handler 14: authored placement graph

Every terrain leaf `11..43` contains one H14 resource in its base OVL. The
33 resources contain **4,486 placement pointers**, ranging from 22 to 264
per world. Their complete framing occupies 336,296 bytes.

The entry has a `0x3C`-byte header. `+0x38` gives the placement count;
the inline array of allocation-relative pointers starts at `+0x3C`.
These fields are covered by the native relocation table. The parser
preserves array order and aliases; it does not deduplicate placements.

Only the header's `+4` DWORD is currently named `flags`. `0048EDE0`
queries it, and `0048DC30` uses its bits for world setup. The other
fourteen-DWORD header representation preserves bytes at `+0..+0x37`
without inventing field meanings. For example, `0048DC30` reads the signed
WORD at `+0x0C`, configures terrain shifts from flag `0x80`, and uses
the signed WORD at `+0x16` in a division; these observations alone do not
justify naming every neighboring field. The consumers on the mission owner
`W` are exact: `+24` receives sign-extended WORD `+0C`, flag `0x1000` clears
byte `+13`, `+20 = min(DWORD +8, trunc(W+1C * 3 / 4))` as signed DWORDs, and
the loader tail stores `0x10000 / (i16)WORD +16` in WORD `+10` (a zero divisor
faults natively). The first world's `+8 = 0x500` and `+16 = 0x180` give the
recorded `+20 = 0x500` and `+10 = 0xAA`; see
[`world_mission`](../../crates/inf-game/src/world_mission.rs).

Each pointed placement has this proven prefix:

| Offset | Representation | Proven use |
| --- | --- | --- |
| `+0` | full DWORD | Passed in ECX to `00460310`; the signed low WORD selects H13; the upper word is retained unchanged |
| `+4/+6/+8` | three raw WORDs | X/Y/Z input copied onto the constructor stack |
| `+0x0A/+0x0C/+0x0E` | three raw WORDs | Rotation source words; constructor input order is `+0x0E`, `+0x0A`, `+0x0C` |
| `+0x10` | signed command count, followed by relocated command pointers | Address passed in EDX to `00460310`; after class initialization, `004602D0`/`00479E10` dispatches the ordered list through object virtual `+4` |

`0048DC30` calls terrain sampler `00450A30` and adds its WORD result to
the copied Y WORD at `+6`, with WORD wrapping, before constructor dispatch.
The parser deliberately preserves file positions, not terrain-adjusted
runtime positions. No general signed coordinate interpretation, scale,
rotation-axis naming, or frontend presentation adapter is inferred.
`WorldPlacement::constructor_rotation_words` only performs the evidenced
ordering operation. `constructor_slot` exposes the signed low WORD without
discarding the original full `resource_argument`.

The loop's array index is separately passed to the constructor. Reload path
`0048E170` looks up that index with an unsigned count comparison and uses
the same resource, geometry, and instance-data layout. World construction
uses a signed positive-count loop. Valid installed counts satisfy both;
malformed counts fail the bounded parser before any allocation-sized loop.

## Bounds and relocation contract

The shared loader `004480B0` obtains the main allocation, internal relocation
table, and entry-pointer table through `004EDDB0`. The Rust semantic parser
does not patch original OVL bytes. `AssetRelocation` records both the
relocation-table word span and pointed field span, together with their
allocation-relative offsets and original target value.

All framing spans and table lengths are checked, as are every relocation
field, target, entry prefix, world header, pointer array, and placement
prefix. Placement pointer fields must actually occur in the native
relocation table. A zero target offset means the allocation base after
relocation; it is not automatically null. A generic one-past target may be
preserved, but dereferencing it as a prefix fails the required range check.
Duplicate relocation fields are rejected: native double-base addition has
no supported portable asset interpretation. These errors are Rust
containment for malformed data, not claimed retail recovery behavior.

## Remaining related asset gaps

| Family | Current coverage | Next exact consumer work |
| --- | --- | --- |
| H5 | Seven `0x14`-byte records, exact raw words | Display presets: `0050F340` replaces `+8` with runtime HWND; `00437CA0` copies the selected five words and changes packed policy bytes from settings. Decode the display request consumer before naming all packed bytes. |
| H10 | Fifteen typed sample grids account for all 288,020 trailing bytes, including dimensions/metadata | [Sample assets](SAMPLE_ASSETS.md) owns signed eight-WORD samples, four-sample masks and material indices used by `00470170`. Live terrain-map/display composition remains separate. |
| H11 | Eight 256-slot tables, two raw WORDs per slot | `004508E0` indexes by H12 sparse byte `+3`, resolves the first signed WORD through H8, then reads the model radius at `+0x0E`. This is at least a terrain/furniture model lookup, not proof of an entity-placement table. Remaining second-WORD consumers need exact classification. |
| H15 | Eight five-DWORD records | `0044FDD0` publishes the selected resource at `00548394`; `00465070` selects the first four DWORDs using terrain material bits, while `004650E0` returns `+0x10`. Follow downstream effect consumers before naming the values. |

Historical “mission placements”, “entity references”, and similar labels
must not replace these consumer contracts. Derived H13 class data and
specialized/conditional H14 command payloads remain gameplay asset boundaries.

Non-OVL source media is already present: `Title.avi` contains 640x480 IV50
video, and cdaudio owns eight canonical Ogg (or extracted WAV) tracks
with original CD track numbers. Codec/playback integration is distinct from
recovering proprietary asset semantics. `Infestation.swg` is the shipped
SideWinder controller profile; `Infest.sav` files are save/settings state,
not additional unidentified OVL banks.
