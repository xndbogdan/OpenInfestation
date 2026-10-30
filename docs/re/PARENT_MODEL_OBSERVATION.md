# Infestation parent-model observation

This owns the first observed parent-slot import and its byte-level writer
provenance. [Dynamic model observation](DYNAMIC_MODEL_OBSERVATION.md) owns the
broader boundary; the capture ledger
owns query bundles and exact replay commands.

The copy and writer history are validated. **The bundle remains partial and
is not eligible for captured presentation:** the parent model's separate
auxiliary stream does not yield a matching full source fingerprint.

## Subject selection

Native world selection 22 activates OVL leaf **32**, through `current + 10`
at `0048DC30`. Its difficulty-0/language-0 residency is `[0,1,32,5]`, with
118 authored placements. Phase IDs, world selections, OVL leaves and model
resource slots are different domains.

The nine variants assigned to the parent-dependency group are:

| Global slot | Model | Canonical parent | Resident for leaf 32 |
| --- | --- | --- | --- |
| 130 | stubaxlesimple | 118 wheelmode1a | Yes |
| 132 | bigstubaxlesimple | 119 wheelmode2a | Yes |
| 280 | f03silo1 | 279 f04base | Yes |
| 281 | f03silo2 | 279 f04base | Yes |
| 616 | mantisclaw2 | 615 mantisclaw | No |
| 617 | morphmantop | 614 mantomorph | No |
| 618 | metmid | 614 mantomorph | No |
| 655 | oarm01 | 654 orbitop | No |
| 657 | mantop | 656 mantron | No |

This is a classification by dependency priority, not a promise that parent
vertices alone recover every mesh. Axle polygons also need renderer `2C`
output. The silo import supplies an edge; its missing polygon vertices depend
on callback/register-driven `26` output. Only 616 has parent slots as its sole
listed dependency.

Read watches on the first `2A` words of the four frozen, header-matched
registered candidates reached the next phase request at `735E8F:26E6` without
a watched access, starting at `41743C:13CB`. This concerns those addresses in
that interval, not lifetime residency or global unreachability. Resident H13
slot 28 initially binds parent 279, but no leaf-32 placement or typed model
override selects it. Dynamic creation remains possible.

The reachable subject is **554 gunbar01**, under **553 imortalguntop**, under
**552 imortalgun**. All three already have partial meshes; this observation
does not reduce the nine-variant group or the 41 runtime-dependent variants.
Leaf 32 has eight typed model-552 overrides on placements 29..36. That does
not establish an observed placement-to-draw association.

## Source and invocation ancestry

Canonical leaf-32 content spans exclude alignment padding:

| Slot / local | Absolute bytes | Length | SHA-256 |
| --- | --- | --- | --- |
| 552 / 22 | `5B008..5B16B` | 355 | `53E7F92D9B481F37EB3F22D4E6DAE95A751CD05FDA922E33D8AAD1B016AA613A` |
| 553 / 23 | `5B16C..5B4A8` | 828 | `FB72B64C6B880C6FB772E403E843EE3ABB78749C66BC5B4EFB9EC725FA177000` |
| 554 / 24 | `5B4A8..5B619` | 369 | `20427EA5D4493850F89C57E64FC0BAD6354EF7DBD9B2084DE2665521C4F825AD` |

Reverse H2 observations on thread `C23C` establish these entries; the forward
import retains their parent pointers and authored state arrays:

| Invocation | H2 position | Context | Entry | H2 return |
| --- | --- | --- | --- | --- |
| Root 552 | `417EA0:1E04` | `001AF940` | `047C995C` | `004A00C6` |
| Parent 553 | `417EA1:1983` | `001AF718` | `047C9AC0` | `004A1103` |
| Child 554 | `417EA2:2393` | `001AF4E8` | `047C9DFC` | `004A1103` |

Root opcode `0E` occupies relative `14A..154`, orientation 6, model 553,
state `[6]`. Parent `0E` occupies `1B6..1C4`, orientation 6, model 554,
state `[14,12,10]`. Child `+6C = 047C9C7E` points to that three-WORD state
array, including the attachment at index zero. Following memory is subsequent
program code, not additional child state.

## Full-record copying and initialized bytes

At `004ED350`, ECX is child context, EDX arguments, and `[ESP+4]` the signed
command DWORD. Destination, selector and mapped parent-slot WORD are signed.
The observed command is `0A2A [24,1]`, arguments at entry `+A6`.

- Selector 1 reads state WORD 12. Source is parent bank
  `005649A0 + 12*24 = 00564AC0`; destination is child bank
  `00564E70 + 24*24 = 005650B0`.
- With a non-null parent, mode `0300` copies all six DWORDs; mode `0C00`
  copies parent `slot ^ 1` into destination+1. Here projection-request bits
  still copy complete records 12/13 into 24/25. No shade DWORD is copied.
- `004ED394` and `004ED3BE` follow `REP MOVSD`. ECX is zero and ESI/EDI have
  advanced 24 bytes. Imported world coordinates and screen projections are
  already transformed; the child basis must not transform them again.

Nonrender wrapper `004EDD60` clears `0A00` before calling the shared handler
and returns at `004EDD6D`. This observed invocation returns to render dispatch
`004A0227`; a `2A` hit alone does not distinguish the paths.

Parent header WORDs `+10..+18` are `[0,10,10,16,44]`. Initial projection
`004EBFE0(0,10,44)` and world initialization `004EC0A0(10,10,16)` cover both
slots 12/13. The indexed write query of `00564AC0..00564AF0` from parent H2
to import finds exactly **24 writes**: six projection writes per slot, then
six world writes per slot. The last is `004EC1BF` at `417EA2:145`. No later
write occurs before import `417EA3:786`, including through the parent's
packed callback selector 3.

Far projection writes fog at `004EBA2F`, X/Y at `004EBA38/3B`, depth at
`004EBA3F`, clip at `004EBA74`, and returned depth again at `004EC069/7F`.
World mapping writes oriented XYZ at `0049092E/4B/67`, then adds the origin
at `004EC162/170/17D` and `004EC1A3/1B4/1BF`. Only the low `Size` bytes of
each TTD `Value` establish custody; its neighboring upper bytes do not.

| Parent slot | World XYZ | Screen XY | Depth | Clip / fog (hex) |
| --- | --- | --- | --- | --- |
| 12 | `[15457,682,-19163]` | `[392,356]` | 8358 | `12 / C5` |
| 13 | `[15473,682,-19076]` | `[387,356]` | 8305 | `12 / C3` |

Offsets `00..15` are initialized in each 24-byte record: **44 of 48 bytes**.
Offsets `16/17` have no writer in this invocation and remain unknown. All
48 retained bytes copy exactly; equality does not initialize the four unknown
bytes. The validator retains an explicit known-byte mask.

## Partial source identity and next boundary

The six-event chain reaches original copy `417EA3:7A4`, mirror copy
`417EA3:7B5`, child end `417EA3:144F`, parent end `417EA4:6E6`, and root end
`417EA4:9E3`. Nine address- and event-bound dumps retain contexts, before/after
records and all three source entries. Root and child hashes match. Parent
snapshot SHA-256 is
`F55DE4797559334833D842307C53684CD485A9DBC08FF79D757B2DFBCF3B3676`.
Its only differing span is auxiliary region `20..40`, returned as zeros;
14 source bytes there are nonzero. All other parent bytes match.

`004A00E0` skips these auxiliary records when finding vertices, normals and
render commands. Separate bytecode consumer `004F2C50` starts at entry+20
when header WORD+0E is nonzero; vertices begin after `4 * header BYTE+6`.
Its `88` terminator reaches `004F3502`, with model at `[ESP+44]`. Rendering
to completion therefore does not prove this stream was read. Examined
registration code `0050F5A0` and this consumer do not rewrite auxiliary bytes.

The indexed auxiliary read/write query from world-loader entry
`412C94:1CBC` to phase request `735E8F:26E6` reports no accesses. Snapshot
`5B2A91:1407` still returns zeros. This establishes an observation limit,
not zero-filled retail source; CPU access history does not establish OS/kernel
writes. No memory policy, index, retail memory or source bytes were changed.

The canonical [auxiliary decoder](MODEL_AUXILIARY.md) now accounts for all
32 source bytes: vertex helper for slot 0, callback `8E` with scalar 418,
`95` failure branch to end, vertex helper for slot 34, callback `8E` with
scalar 22, then `88` and one padding byte. All 354 active model variants
decode structurally. This closes the grammar gap, not the observation gap:
there is still no source-matched parent snapshot in the accepted evidence.

A whole-recording indexed read/write query of `047C9AE0..047C9B00`, capped
at 65 rows, returns only 32 events at `73B787:207A..21B8`, after the next
phase request. These are sixteen WORD reads at `0050E6B5` and sixteen writes
at `0050E6E3`. The first-access snapshot no longer has a model header at
`047C9AC0`: it begins `4A 4A 8A 52 8B 52 CB 5A`. Routine `0050E690`
repackages RGB555 channels using observed shifts `[11,6,0]`; for example,
`0421` becomes `0841`. This is later buffer reuse, not a missing auxiliary
execution. The query does not observe OS/kernel writes or the exact allocation
release. Repeating snapshots of this original address cannot supply the
missing source bytes from an indexed consumer in this recording.

All thirteen installed physical copies of global 553 (leaves 15, 26, 28,
32 and 35..43) independently hash to the canonical parent SHA-256 above.
The corpus therefore supplies no alternate zero-auxiliary parent variant.
This fixed-address investigation is exhausted; further parent presentation
needs another source-matched invocation or independent literal acquisition,
while other asset-inspection work can proceed without weakening this gate.

Static evidence from REA (2026-10-12) explains where the
zeros come from. The Handler-8 loader `0050F5A0` reads each block through
`004F3DE0` straight into its allocation, then registers each entry with
`004EFC00`, which writes only the slot pointer `*(ctx+4) + index*4` and
never the entry's bytes. `004F3DE0` calls `fread` (`00512E39`); with an
empty stream buffer, `00512E68` passes every whole multiple of the buffer
size to `_read` (`00517360`) aimed at the destination itself, and copies
only the remainder from the stream buffer (`0051641C`, `005175A0`). Those
bytes therefore arrive through `ReadFile`, outside the recorded CPU
instructions, and TTD reconstructs such memory only once the process reads
it. The only reader of the span is the collision interpreter `004F2C50`,
reached through `004F2BE0` (actor pairs `0047B610`, furniture `0047C1D0`),
`004F2C40` (`00435D50`, `0043A7E0`, `0044EC50`, `0047CEB0`, `0047D080`) and
its own wrappers; the draw path skips it. The indexed query above saw no
access before the reuse. So the snapshot's zeros are TTD's value for bytes
nothing read, and the loaded bytes are the file's: every byte the captured
draw consumed matches the canonical parent. Promoting this to source identity
remains the gate owner's decision; it is static and replay evidence, not a
literal dump of the span.

The evidence adapter returns `Complete=false`; the runner retains literal
dumps and records `partial / unverified_source_identity`. The viewer still
accepts only the two verified healthsmall captures. Next establish parent
source identity through literal evidence, then add canonical parent-capture
inspection. Do not fill replay
gaps from source or promote this partial bundle to a completed mesh.
