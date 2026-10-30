# Infestation (2000) File Format Documentation

## Overview

This document describes the file formats used by Infestation (2000) by Frontier
Developments, based on reverse engineering of `Infest.exe`.

Infestation is the continuation of V2000 (1998). Its executable names its
source tree `V3000` and its engine `Fgdk2`, the C++ port of V2000's `FGDK`, and
many V2000 formats survive with the same layouts. The
FGDK cross-game map
holds the proof and the per-format correspondence; each transferred fact
still needs confirmation in `Infest.exe`.

The canonical implementation is this repository's Rust workspace. Its parsers,
corpus tests, staged exporters, and direct-OVL viewer supersede the former
Python extraction pipeline. `extracted/` is wholly generated,
Git-ignored output: it may be deleted and regenerated with
`cargo run -p inf-extract -- all`. Curated screenshots and durable analysis
artifacts are kept privately; nothing in `extracted/` is a
hand-maintained format golden.

## OVL (Overlay) File Format

### File Naming Convention

The game uses a hierarchical overlay system with the following naming patterns:

| Pattern | Description | Example |
|---------|-------------|---------|
| `overlay\%d.ovl` | Base overlay (world/level index) | `0.ovl`, `1.ovl` |
| `overlay\%dd%d.ovl` | Difficulty variant | `0d0.ovl`, `0d1.ovl` |
| `overlay\%dl%d.ovl` | Language variant | `0l0.ovl`, `0l1.ovl` |
| `overlay\%dd%dl%d.ovl` | Difficulty + Language variant | `0d0l0.ovl` |

### OVL Loading Function (0x004EE4E0)

The main OVL loading function (`__thiscall`) takes 4 parameters:
- `this` (ecx): Scene manager object (contains level index at offset 0x0C)
- `param_2`: Array of 16 `short*` pointers — one per handler, each pointing to an int16 resource count
- `param_3`: Output array for loaded resource pointers (16 entries)
- `param_4`: Load type (determines filename pattern):
  - `0`: Base overlay (`%d.ovl`)
  - `1`: Difficulty variant (`%dd%d.ovl`)
  - `2`: Language variant (`%dl%d.ovl`)
  - `3`: Full variant (`%dd%dl%d.ovl`)

**Dispatch logic:** The function iterates handlers 0–15. For each, it dereferences
`param_2[i]` to get the int16 resource count. If non-zero, it calls that handler's
Load function (vtable[+0x08]) with the count. Handlers with count=0 are skipped
entirely and consume no bytes from the file stream.

### OVL Loading Dispatch (0x004EE680) — DECOMPILED

```c
void __fastcall FUN_004ee680(int this) {
    // Loads all 4 OVL file types for the current level
    LoadOverlay(*(this + 0x10), *(this + 0x14), 0);  // base: N.ovl
    LoadOverlay(*(this + 0x18), *(this + 0x1c), 1);  // difficulty: NdM.ovl
    LoadOverlay(*(this + 0x20), *(this + 0x24), 2);  // language: NlM.ovl
    LoadOverlay(*(this + 0x28), *(this + 0x2c), 3);  // diff+lang: NdMlM.ovl
}
```

**Scene manager struct layout:**

| Offset | Type | Description |
|--------|------|-------------|
| +0x0C | uint32 | Level/world index |
| +0x10 | short** | Base OVL resource request array (16 pointers) |
| +0x14 | void** | Base OVL output array |
| +0x18 | short** | Difficulty OVL request array |
| +0x1C | void** | Difficulty OVL output array |
| +0x20 | short** | Language OVL request array |
| +0x24 | void** | Language OVL output array |
| +0x28 | short** | Diff+Lang OVL request array |
| +0x2C | void** | Diff+Lang OVL output array |

The request arrays are stored in `PreLoad.dat` and loaded at startup by a recursive
scene tree parser (`FUN_004ef490`). Each level node contains four serialized
arrays of 16 resource request entries (base, difficulty, language, diff+lang).
See the [PreLoad.dat Format](#preloaddat-format) section for the full binary structure.

**Callers:** `ghidra_trace_load_callers.py` found 4 direct callers of LoadOverlay —
`FUN_004ee680` (the 4-type dispatcher) and 3 inlined call sites within a larger
initialization function.

**Scene load context:** `port/crates/inf-formats/src/scene.rs` names the
`SceneContext { level, difficulty, language }` selection, emits the four OVL
filenames/keys/request tables in the dispatch order above, and resolves one
global slot to the last registering phase (`FUN_004EFC00` overwrite). Proven
from retail `PreLoad.dat`: Handler 8 is base-only in all 46 leaves (36 levels
register 2-388 models; no difficulty/language/combined phase requests Handler
8), so each leaf's model contributions come entirely from its base registrations.
The complete resident table also depends on coexisting leaves and activation
order. The authored Group/Select ownership is now established in
[runtime foundation](RUNTIME_FOUNDATION.md); the single-leaf `SceneCatalog`
does not execute that lifetime policy.

### Global Variables

| Address | Description |
|---------|-------------|
| `DAT_0053aa10` | Number of resource handlers |
| `DAT_0065e72c` | Current difficulty level |
| `DAT_0065e72d` | Current language index |
| `DAT_0065edec` | Base game path |
| `DAT_0065f134` | Handler 3 sprite-atlas buffer pointer (allocated by `FUN_0050f1f0`) |
| `DAT_0065f130` | Handler 3 sprite-atlas buffer key (cached to avoid redundant reallocation) |
| `DAT_0065ee70` | Head of loaded-resource linked list (Handler 3/7 blocks) |
| `DAT_0065f114` | Resource-active flag (nonzero after initial load; triggers callbacks) |

### OVL Sequential Block Format

All OVL files use a sequential block format. Each resource handler reads one or more
blocks from the file stream in handler index order (0–15). The LoadOverlay function
(0x004EE4E0) iterates handlers 0–15 and calls each active handler's Load function.

**Standard block format (most handlers):**
```
[uint32 alloc_size][alloc_size bytes of data]
```

**Handler 8 sub-block format (models):**
```
[uint16 alloc_size][uint16 entry_count][alloc_size bytes of data]
```
Handler 8 reads multiple sub-blocks in a loop until total entries >= resource count.

**Inactive handlers:** Handlers with resource count = 0 are NOT called and consume
no bytes from the file stream. However, some files contain long chains of zero-size
blocks (e.g., 0d0.ovl has 184 × 4-byte zeros) which may come from handlers that are
active but have empty allocations.

**Handler 3 extra reads:** After the main `[uint32 size][data]` block, Handler 3
enters a loop reading 2-byte records until a `0xFFFF` terminator. Each iteration
reads 8 bytes (x_start, y_start, x_end, y_end as uint16) defining a texture-atlas
byte rectangle, followed by `(x_end-x_start)*(y_end-y_start)` raw bytes.

**Handler 3 entry format:** See [Handler 3 Logical Entries and Physical Variants](#handler-3-logical-entries-and-physical-variants) below.

**Coverage with PreLoad.dat dispatch mapping (257 non-empty OVL files):**

| Metric | Value |
|--------|-------|
| Files at 100% coverage | 257/257 (100%) |
| Total bytes mapped | 158,707,834 / 158,707,834 (100.0%) |

Previous heuristic-only coverage was 3–13% for large files. With the PreLoad.dat
resource counts providing exact handler dispatch order, every block in every OVL
file is now identified and assigned to its handler. Handler 13/14's non-standard
read pattern (`FUN_004eddb0`) is fully decoded — see [Handler 13/14 Tail Format](#handler-1314-tail-format-decoded) below.

**String content by OVL type:**

Not all OVL files contain text. Handler 0 (strings) is only present when the OVL has
string resources in the request array.

| OVL | Strings? | Content |
|-----|----------|---------|
| `0.ovl` | Yes (6) | Language names: "English", "Français", "Deutsch", "Italiano", "Español", "Nederlands" |
| `0l0.ovl` | Yes (657) | Full UI string table: resolutions, menu labels, game text |
| `2.ovl` | Yes (2) | Video filenames: "BannerLo.avi", "BannerHi.avi" |
| `0d0.ovl` | Yes (2) | Minimal placeholders: " ", "?" |
| `4.ovl` | Yes (47) | Single-character entries ("+", "K", "7", etc.) |
| `38.ovl` | Yes (2) | Short strings: "^", "TZ" |
| `1.ovl` | No | Model data only (index table + Handler 8 sub-blocks) |
| `5-9.ovl` | No | Small structured data (2 blocks, ~1KB each, no ASCII) |
| `30-43.ovl` | No | Level data — first block is 8–48 bytes of config, rest is geometry/map |
| `47.ovl` | No | Large data block (757KB), no string block |

Language overlays (`NlM.ovl`) are dedicated string files — 100% text content.
Difficulty overlays (`NdM.ovl`) have small string blocks (often just placeholders).
Most base/level overlays contain no strings at all.

These counts are no longer inferred by printable-byte scanning. The Rust parser
implements `FUN_0050ECD0`'s exact `count`-times NUL-terminated walk; all 164
retail Handler-0 banks (8,126 strings, 375,362 payload bytes) consume their
framed allocation exactly. Unusual short entries are therefore authored string
resources, not mapper false positives.

---

### OVL File Types

#### 0.ovl - UI Sprites and Data (983,065 bytes)

The exact PreLoad dispatch maps every byte in this file to a handler. Handler 0
registers the six language-name strings. Handler 3 registers eight logical
sprites, each with one 32x32 indexed variant; all eight intentionally share one
atlas record while using distinct 62-color palettes. The main payload contains
the selector/variant descriptors and palette data, and the post-payload atlas
record contains the 1,024 index bytes. There is no separate five-byte inline
sprite header.

The descriptor fields, palette layout, flags, and atlas framing are documented
in the canonical Handler-3 section below. In particular, the palettes are
RGB555, consist of contiguous `[color][32 shades]` ramps followed by a
contiguous base-color table, and are not RGB565. The old offsets around
`0xCFA68`/`0x85D8` and scans for additional inline images are not Handler-3
format evidence.

**Key Finding (from decompiled handlers):** The bulk data is **NOT compressed**.
All 16 resource handler Load functions were decompiled (see `resource_handlers.c`).
None contain decompression logic — they all read raw data via buffered `fread()`.
The ~960KB of data is raw game resources in various binary formats, read sequentially
by whichever handlers are active for this OVL. The entropy (~6.65 bits/byte) is high
because the data includes RGB pixel data and binary structs, not because it's compressed.

**Canonical parser/exporter:** `port/crates/inf-formats/src/sprites.rs` and
`cargo run -p inf-extract -- sprites --ovl 0.ovl`

#### 1.ovl - 3D Model Data (1,852,970 bytes)

This file contains 3D model/mesh data for the game's units, buildings, and objects.

**Sequential blocks:**
1. Block 0 (offset 0x0, 64 bytes): Index table (16 × uint32 offsets)
2. Handler 8 sub-blocks (offset 0x44): 28 sub-blocks containing 278 model entries
3. Remaining bytes: exactly framed resources owned by later active handlers

**Handler 8 Sub-Block Structure:**

Each sub-block has a 4-byte header: `[uint16 alloc_size][uint16 entry_count]`

| Sub-block | Offset | Alloc | Entries | First Model Names |
|-----------|--------|-------|---------|-------------------|
| 0 | 0x44 | 10,120 | 29 | beacon, missile, ammobox, powergen... |
| 1 | 0x27D0 | 10,176 | 24 | (continued) |
| ... | ... | ... | ... | ... |
| 27 | 0x3A3DC | 8,196 | 3 | (last sub-block) |

**Totals:** `1.ovl` contains 28 sub-blocks and 278 named entries. Across the
complete retail corpus, Handler 8 occupies 36 OVL regions, 457 sub-blocks, and
5,142 entries; the recovered registration mapping gives a stable named global
namespace of IDs 0 through 759.

**Model Entry Structure (variable length, within sub-block data):**

**Entry Header (32 bytes, 0x20):**

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0x00 | 4 | uint32 | Visibility extent used by near/far selection |
| 0x04 | 2 | uint16 | Command word count |
| 0x06 | 1 | uint8 | Auxiliary byte-span length in DWORD units (not instruction count) |
| 0x07 | 1 | uint8 | Flags (`0x20` forces near table; `0x40` omits name) |
| 0x08 | 2 | uint16 | Runtime vertex-slot count |
| 0x0A | 2 | uint16 | Raw stored vertex count |
| 0x0C | 2 | uint16 | Runtime shade-slot count |
| 0x0E | 2 | uint16 | Nonzero admits the separate `004F2C50` auxiliary consumer; further numeric meaning remains open |
| 0x10 | 2 | uint16 | Raw-projection start at `FUN_004EBFE0` (zero in retail; retained as `reserved_10` in the raw schema) |
| 0x12–0x18 | 8 | 4 × uint16 | Even monotone boundaries: root world XYZ uses indices 0/1/2 (`004EC0A0`); raw projection uses `+10`, index 1, index 3 (`004EBFE0`) |
| 0x1A | 2 | uint16 | Reserved (zero in retail) |
| 0x1C–0x1E | 4 | 2 × uint16 | Even monotone shade preprocessing ranges |

**Entry data layout:**

```
32-byte header
auxiliary_count * 4 bytes of separately terminated auxiliary bytecode
raw_vertex_count * 6-byte signed int16 vectors
((shade_slot_count - 2) / 2) * 6-byte signed int16 normal vectors
command_word_count * 2-byte wordcode program
optional NUL-terminated name (unless flags & 0x40)
zero padding to a four-byte boundary
```

The separate [auxiliary bytecode](MODEL_AUXILIARY.md) has byte opcodes,
entry-relative WORD/DWORD argument alignment, signed branch displacements
and an `88` terminator. Canonical parsing preserves its raw DWORD framing
and exports typed active programs; it does not execute the callback policy.

**Vertex data:**
- Format: signed 16-bit integers (int16 x, y, z), 6 bytes per vertex
- Coordinate scale remains raw; the canonical parser does not invent world units
- Raw vertex count is at entry offset +0x0A

**Normal pool:** signed int16 x/y/z vectors, six bytes each. Its exact raw count
is `(shade_slot_count - 2) / 2`; it is not a triangle-count estimate.

**Command Stream:** ✓ DECODED FROM GAME CODE (dispatch handlers decompiled)

The `command_word_count * 2` byte region is a bytecode program executed by the renderer.
**Ground truth located 2026-06-09:** the render entry `FUN_004a00e0` walks the
stream dispatching on the **LOW BYTE** of each word through 256-entry function
pointer tables. Each handler receives `(context, args_ptr, cmd_word)` and
returns the pointer to the next command (NULL = stop). Five tables exist:

| Table VA | Role |
|----------|------|
| 0x00540DC0 | Render pass A (near / full detail) |
| 0x005411C0 | Render pass B (far LOD; selected by distance vs header word 0; entry flags bit 5 forces A) |
| 0x00543E00 / 0x00544200 | Second pair (secondary pass, same record formats) |
| 0x00544764 | Measure/skip walker (`FUN_004ed610`) |

All distinct handlers were decompiled with Ghidra; the exports are kept
privately, outside this repository.

**Runtime vertex buffer — the half-model mechanism:**

Vertex references in all commands are **slot indices** into a runtime buffer of
24-byte transformed-vertex slots:

```
slot 2*i   = transformed model vertex i
slot 2*i+1 = X-NEGATED (mirrored) copy of vertex i
```

This is why all refs appear "doubled". Symmetric models store only one half:
- Odd refs (`2*i+1`) select the mirrored copy directly (seam-spanning faces
  pair a vertex with its own mirror, e.g. factory1 wall quad `v52, v52m, v37m, v37`).
- **Face command bit 10 (0x0400) = mirror instancing**: the face is drawn twice,
  second time with every vertex ref XOR 1 (and flipped cull sense). This is how
  whole mirrored halves are produced — per face, not per model.
- Face command bit 9 = backface cull direction; bit 8 = backface cull enable.
  (The old "variant bits 9-10 = winding/quad subdivision" reading was wrong.)

Validation: quads decoded with mirror-bit semantics are planar (mean error
0.003 vs 0.122 under the old ceiling-division guess; 759 quads tested).

**Recovered opcode framing** (record length = 1 command word + N argument
words; runtime-dependent effects remain explicit dependencies):

| Op (low byte) | Args | Handler | Meaning |
|------|------|---------|---------|
| 0x00 | — | FUN_004efbf0 | End of stream (pops nested sub-object stream) |
| 0x01 | var | FUN_004966f0 | Execute-buffer triangle fan: `[cmd][flags][material][entries...]`; flags bit 3: 6-word entries (else 4), bit 4: mirror instancing; entry = `[vref, nref, ...]`, fan ends when high byte of last entry word != 0 |
| 0x02-0x04 | 3/3/6 | — | State/setup records |
| 0x05 | 12+nested | FUN_004ED9C0 | Sub-object: origin from vertex slot args[0], three packed basis rotations in args[1], args[2], and args[3], static/mirrored/end ranges in args[5], args[6], and args[7], then NESTED command stream (own 0x00 terminator); only origin/basis are restored |
| 0x06 | scan | FUN_004a3880 | List, scan to 0xFFFF terminator |
| 0x07 | 4 | FUN_00495ca0 | Edge/line: `[cmd][type][param][v0][v1]` |
| 0x08 | `cmd>>8` words (`(cmd>>8)*2` bytes) | FUN_004a0250 | Variable skip |
| 0x09 | 11+nested | FUN_004EDB10 | Sub-object with rotation basis from 2 live vertex slots; cmd bit 8 mirrors the basis (structurally decoded, zero retail occurrences) |
| 0x0B | 3 | FUN_004a0ce0 | Conditional jump on plane test (view-dependent): jump args[0] words from args, else fall through |
| 0x0E | var | FUN_004a0e70 | Inline instance of another model (model id args[1]); next cmd at args + args[2] BYTES |
| 0x0F / 0x10 | 0 | — | Transform matrix save / restore |
| 0x13, 0x15, 0x1C | 3/1/2 | — | State records |
| 0x16 | 3 | FUN_004a0c90 | **Conditional jump**: `[cmd][jump][a][b]`; if `(decode(a) != decode(b)) != (cmd>>8)` jump `jump` words from args. Used for animation frame selection (compares animation variable vs frame id) — there are no "frame markers", frames are just guarded face runs |
| 0x18-0x1B | 4 | FUN_004a1530+ | Textured beam/sprite records |
| 0x20 | 1 | FUN_004ebf00 | Re-transform base vertex into slot (negates X for the odd/mirror copy) |
| 0x21 | 2 | FUN_004ec1e0 | dest = point reflection of src through object origin |
| 0x22 | 3 | FUN_004ec300 | dest = src + random jitter >> args[2] |
| 0x23 | 3 | FUN_004ec410 | dest = midpoint(src1, src2) |
| 0x24 | 4 | FUN_004ec710 | dest = a + b − c (parallelogram completion) |
| 0x25 | 3 | FUN_004ec810 | dest = s1 + s2 − origin |
| 0x26 / 0x28 | 4/5 | FUN_004ec950 | dest = lerp(s1, s2, factor/65536); factor operand packed (see below) |
| 0x27 / 0x29 | 6/7 | FUN_004ecce0 | Spline interpolation (quadratic/cubic, packed factors) |
| 0x2A | 2 | FUN_004ed350 | dest = object origin, or imported parent-model vertex (attachments) |
| 0x2B / 0x2C | 2 | FUN_004ec510 | dest = projected/clamped copy of src |
| 0x2D | (cmd>>12)+2 | FUN_004ec670 | Custom vertex generator (vtable callback); LENGTH IN CMD BITS 12-15 |
| 0xD0/0xD2 | 7 | FUN_0049e790 | Triangle, per-vertex normals: `[mat][v0..v2][n0..n2]` |
| 0xD4/0xD6 | 9 | FUN_0049f1e0 | Quad, per-vertex normals: `[mat][v0..v3][n0..n3]` |
| 0xD8/0xDA/0xDC/0xDE | 5 | FUN_0049be10+ | Triangle, per-face normal: `[mat][norm][v0][v1][v2]` |
| 0xE0/0xE2/0xE4/0xE6 | 6 | FUN_0049d110+ | Quad, per-face normal: `[mat][norm][v0..v3]` |
| 0xE8/0xE9 | scan | FUN_004a0770 | Depth sort key from vertex list, 0xFFFF-terminated |
| 0xEA/0xEB/0xED/0xEE | 1/0/2/3 | — | Depth sort key (vertex / camera / immediate / vertex+offset) |
| 0xEC/0xEF | 0 | — | No-op / bracket |
| 0xF0-0xFF | 3 | FUN_004a0260+ | Animation variable ops on 64-entry register file at ctx+0x70 (set/shift/max/...); the per-vertex-op and jump operands read these |

The D0-E6 variants differ in shading/texturing mode (which `FUN_004a42b0`
primitive type they emit); record layout within each group is identical.
All vertex-generator ops (0x20-0x2D) compute the destination slot AND
optionally the mirrored partner slot dest+1 from mirrored sources (refs XOR 1),
controlled by cmd bits 8-11 (bit pairs: write-world-XYZ / project for
original then mirror copy).

**Packed value operands** (lerp factors, jump comparands — `_decode_value`):
- bit 7 clear: immediate = rotate-left-16(`f & 0xFF03`, `(f>>2) & 0xF`),
  inverted if bit 6 set
- bit 7 set + bit 6 set: animation variable `f & 0x3F` (register file ctx+0x70)
- bit 7 set + bit 6 clear: dynamic engine callback

**Old empirical readings now corrected:**
- "9-bit base + variant bits" → dispatch is on low byte; bits 8-10 of face
  commands are cull-enable / cull-direction / mirror-instance flags.
- "Ceiling division for doubled vertex indices" → odd refs are mirrored-copy
  slot references, never index+1.
- "Auto-mirroring heuristic (>75% positive-X)" → replaced by exact per-face
  bit-10 instancing. (Old heuristic missed `world` — a stored half-sphere —
  and wrongly required none of the conditions the engine actually uses.)
- "Mirror vertex expansion commands 0x2A/0x2B/0x2C" → these are NOT mirror
  ops (origin import / projected copies). Mirrored vertices live in odd slots.
- "Animation frame markers `[0x16, 0x2D, 0xCB, id]`" → 0x16 is a general
  conditional jump; the old pattern was factory1's specific operand bytes.
- "Alt-layout face command 0x027 (powerup quads)" → 0x27 is a spline vertex
  op; powerup pickups have no mesh faces (sprite-based).

The canonical Rust decoder walks the complete recursive grammar and preserves
typed triangles, quads, fans, edges, model instances, nested programs, register
operations, dynamic dependencies, and still-unresolved commands. It does not
force runtime registers to zero, call a guessed result "frame 0," synthesize a
shadow pass, or conflate structural decoding with resource presentation. The
diagnostic analyzer then performs a separate conservative data-flow pass: it
follows statically decidable `0x16` branches and equal-target comparisons,
keeps world XYZ separate from proven projection inputs, evaluates vertex generators such as
`0x23` midpoint, and retains polygon geometry independent of unrelated
register, edge, depth-order, or beam/sprite work. Runtime-dependent faces are
reported as omissions. Opcode `0x05` is interpreted only when its origin,
packed rotations, and authored ranges are static. The interpreter applies the
retail `FUN_00491530` → `FUN_00491290` → `FUN_004917D0` basis order, initializes
normal and mirrored slots through the same transformed raw-vertex pool,
recurses into the nested stream, and restores only the enclosing origin/basis.
The 58 retail occurrences comprise 57 literal transforms and one
callback-driven angle in global model 168 (`healthsmall`); the latter remains
unavailable. Opcode `0x09` has no retail occurrence and remains explicitly
unprojected rather than receiving synthetic basis semantics. Exact
source-identical faces repeated across unresolved
conditional alternatives may contribute one explicitly partial representative;
unique alternatives are never unioned into a fabricated frame. Per-face source
opcode, global Handler-3 texture or Handler-7 material ID, command flags, and
lighting-slot references are preserved for later rendering. The domain and
global-ID linkage is recovered. This decoder/analyzer policy is distinct from
the viewer's optional, explicitly labeled neutral compositor. That presentation
layer substitutes zero for callback-backed joint inputs and may carry the value
through proven register operations (currently the retail `0xF5`/`0xF1` chain
used by COMRADAR before attaching COMDISH). It does not claim that the original
callback always returned zero or call the result a canonical frame. Recursive
children begin with unknown diagnostic registers, so inherited runtime register
state remains omitted rather than guessed. Authored literal-zero rotations are
not reclassified as joint channels. The compositor emits only child instances
whose attachment and orientation are proven, subtracts those from the
model-instance omission count, and retains unresolved siblings and
runtime-dependent local faces as explicit omissions. Its output is therefore a
proven partial neutral presentation rather than a complete assembly. The viewer
resolves global resources and uses the concrete software-renderer vtable at
`0x00528FF0` as texture evidence:
`0x004B1D20` constructs fixed triangle corners `(0,0)`,
`((W<<16)-1,0)`, `((W<<16)-1,(H<<16)-1)` in 16.16 texel space;
`0x004B8330` adds `(0,(H<<16)-1)` for quads; and
`0x004B4660` scales fan UV words as unsigned Q0.16 values. Evaluated lighting
and shadow semantics remain a separate rendering concern. Mirror instances keep
UV command order and substitute every geometry/lighting slot with `slot ^ 1`;
they do not flip the image. Fixed-face culling follows `FUN_0048FA60` plus the
command's enable/reverse bits and the mirror-pass bit. Fan bits 0/1/2 select
reverse direction, whole-fan gating from the first triangle, and per-triangle
culling respectively. When bits 1 and 2 are combined, the first rejected
triangle terminates the remaining fan. The diagnostic retains each fan
triangle's source index so the viewer can reproduce these modes after
flattening the topology. The software backend accepts native quads, while the
Direct3D backend chooses diagonal `0-2` or `1-3` from unresolved renderer state
bit 14, so the viewer exposes both.

The complete diagnostic census and presentation gaps are owned by
[model asset coverage](MODEL_ASSET_COVERAGE.md), reproducible with
`inf-extract coverage`. Unavailable projections retain explicit classifications
and dependency provenance. World-slot initialization uses header ranges
`[0,1,2]` at `FUN_004EC0A0`; the historical `[1,2,3]` interpretation incorrectly
treated runtime-populated world slots as authored coordinates. Raw projection
initialization is a separate consumer contract and must not repair world slots.
`inf-extract models` writes every raw entry and typed program regardless of
diagnostic availability.

The 5,142 entries are physical OVL registrations into a 760-slot global model
namespace, not 5,142 logical models. Exact content comparison groups them into
849 `(global slot, content)` variants: 744 slots have one payload and 16 slots
have genuine level-specific alternatives. The direct viewer therefore browses
760 global slots, retains those 16 variant sets, and records all source
OVL/local-slot occurrences. It does not infer a winner across unrelated level
OVLs; when restricted to one OVL, the final local registration is the value
left in the runtime global slot by `FUN_004EFC00`.

**Canonical implementation:**
- `port/crates/inf-formats/src/models.rs` — strict entry and command decoder
- `cargo run -p inf-extract -- models` — lossless staged export
- `cargo run -p inf-viewer -- --mode models` — direct-OVL diagnostic view

**Provenance tools and evidence:**
- Private Ghidra scripts dump observed dispatch tables and handler
  bodies; table structure alone does not establish runtime presentation policy.
- Their decompilations are kept privately. The former Python model projector and
  command surveys were retired after the typed Rust parser superseded them.

**Examples from the 278 named entries in `1.ovl`:**

*Units/Characters:*
- humanrun1, humanstand1, humanwalk1 (human animations)
- muthumanrun1, muthumanstand1 (mutant animations)
- scientst (scientist)

*Vehicles:*
- vtol1, vtol2 (aircraft)
- wheelmode1a, wheelmode2a (wheeled vehicles)
- hoverjet1, hoverjet2 (hover vehicles)

*Buildings:*
- baracks (barracks), bunkers, factory1
- powergen (power generator), powerpad
- sambase, towergun, gatebod

*Weapons/Effects:*
- missile, laser, grenade, bomb, smoke
- frag, proximity, remote
- exwave, exwave2, exwave3 (explosion effects)

*Pickups:*
- powerup, powerup2, powerup9, etc.
- healthsmall, healthsmallspin
- token01, token02, token03

*Model Parts:*
- midsec, topsec (vehicle sections)
- leftarm, rightarm, limb (character parts)
- head1, muthead1

#### 2.ovl - Video References (64,880 bytes)
- Header: First DWORD = 0x1A (26) - entry count
- Contains video filenames:
  - "BannerLo.avi" (low quality banner)
  - "BannerHi.avi" (high quality banner)

#### NdM.ovl - Difficulty Data (e.g., 0d0.ovl = 792,875 bytes)

**Corrected status:** an early Python block scan labeled a 730,832-byte region in
`0d0.ovl` as a 160×160 Handler-12 map. That interpretation is disproven:
`0d0.ovl` does not request Handler 12, and the region does not follow the exact
Handler-12 entry layout documented below. Treat the region as unidentified
difficulty data until its owning handler is decoded; do not use the old 160×160
images as terrain-format evidence.

#### NlM.ovl - Language/Settings Data (e.g., 0l0.ovl = 16,091 bytes)
- Header: First DWORD = 0x3ED7 (16087) - string table size
- Contains localized UI strings:
  - Resolution options: "320x240", "512x384", "640x480", "800x600", "1024x768"
  - Display modes: "In a Window", "Full Screen"
  - Rendering: "Software", "Direct3D"
  - Color depth: "Default", "16 - Bit", "24 - Bit"
  - Input: "Joystick", "Keyboard", "Mouse"
  - Multiplayer: "Join Session", "Host session on this machine"

---

## PreLoad.dat Format

**File:** `PreLoad.dat` (18,280 bytes)
**Loader:** `FUN_004edea0` opens the file, then calls `FUN_004ef490` (recursive scene tree parser)

PreLoad.dat contains the resource request arrays that tell LoadOverlay how many resources each handler should load per level. Without this data, the game cannot parse OVL files.

### Header

| Offset | Size | Type | Description |
|--------|------|------|-------------|
| 0x0000 | 1 | byte | `DAT_0065e72c` — difficulty flag (0) |
| 0x0001 | 1 | byte | `DAT_0065e72d` — language flag (0) |
| 0x0002 | 32 | uint16[16] | Handler capacities (max resources across all levels) |
| 0x0022 | 4 | uint32 | Handler 3 sprite-atlas buffer allocation (5,273,600 bytes = ~5MB) |
| 0x0026 | 1 | byte | Node table allocation count (49) |
| 0x0027 | ... | tree | Recursive scene tree (variable length) |

**Handler capacities** (max resources per handler type, used for memory pre-allocation):

| Handler | Capacity | Type |
|---------|----------|------|
| 0 | 950 | Strings |
| 1 | 12 | Four-byte resources (semantics unresolved) |
| 2 | 2 | Four-byte resources (semantics unresolved) |
| 3 | 1,828 | Sprites and physical variants |
| 4 | 1 | Opaque standard-framed resources |
| 5 | 7 | Fixed records (semantics unresolved) |
| 6 | 60 | Four-byte resources (semantics unresolved) |
| 7 | 122 | Raw four-byte resources; terrain material-palette view where supported |
| 8 | 760 | 3D model command programs |
| 9 | 76 | Sounds and aliases |
| 10 | 3 | Fixed records (semantics unresolved) |
| 11 | 1 | Mission placement table |
| 12 | 1 | Terrain grid and sparse records |
| 13 | 146 | Opaque relocation-framed resources |
| 14 | 1 | Opaque relocation-framed resources |
| 15 | 1 | Mission parameters |

### Scene Tree Structure

The tree is parsed by 3 mutually-recursive functions:

| Function | Address | Type 0 | Type 1 | Type 2 |
|----------|---------|--------|--------|--------|
| `FUN_004ef490` | 0x004EF490 | Leaf (4-array) | Select node | Group node |
| `FUN_004ef7e0` | 0x004EF7E0 | Leaf (4-array) | Select node | Group node |
| `FUN_004efb30` | 0x004EFB30 | `FUN_004ef0e0` | `FUN_004ef7e0` | `FUN_004ef490` |

Each **leaf node** contains:
- 1 byte: `level_id` (0–48, maps to OVL file number)
- 4 arrays × 16 entries: base, difficulty, language, and combined OVL requests

The implicit root and tag-2 nodes are additive groups: vtable `0x00529290`
loads every child through `FUN_004EEE70`. Tag-1 nodes are exclusive selects:
vtable `0x00529274` loads its first child through `FUN_004EED50` and unloads
active siblings when a different descendant is requested. The canonical
`BranchKind` labels were corrected to these meanings; earlier exported JSON
had the two names reversed. Exact bytes and leaf request mappings are unchanged.
See [runtime foundation](RUNTIME_FOUNDATION.md) for activation ownership.

### Resource Request Entry Format (`FUN_004eef30`)

Each of the 16 entries per array:
```
[int16 count][uint16 segment_count][segment_count × uint32 packed_segments]
```

- `count`: Number of resources to load for this handler (0 = handler inactive)
- `segment_count`: Number of local-to-global mapping segments
- `packed_segments`: Low word is the unsigned local threshold; high word is a
  signed delta. `FUN_004EFC00` selects the last segment whose threshold does
  not exceed the local index and adds its delta. Backward mappings and aliases
  are intentional; these words are not allocation hints or buffer sizes.

### Key Level Examples

**Level 0 (base — `0.ovl`):**
| Handler | Count | Type |
|---------|-------|------|
| 0 | 6 | Language name strings |
| 3 | 8 | Sprite descriptor entries |
| 5 | 7 | Fixed records (semantics unresolved) |
| 6 | 55 | Four-byte resources (semantics unresolved) |
| 7 | 98 | Raw four-byte resources (roles unresolved for this bank) |
| 8 | 2 | 3D models |
| 9 | 30 | PCM/alias sound resources |

**Level 1 (base — `1.ovl`):**
| Handler | Count | Type |
|---------|-------|------|
| 7 | 16 | Four-byte resources (roles unresolved for this bank) |
| 8 | 278 | 3D models (28 sub-blocks) |
| 9 | 47 | PCM/alias sound resources |
| 13 | 131 | Constructor prefixes decoded; class-specific suffixes open |

**46 levels** defined (IDs 0–43, 47, 48; IDs 44–46 absent).

### Canonical Export

`cargo run -p inf-extract -- installation` preserves exact `PreLoad.dat`, emits
its parsed 46-level scene tree, and inventories all 1,932 physical OVLs. The
older request report is retained only as curated evidence under
`reference/preload_resource_requests.json`.
Decompiled functions: `scene_tree_functions.c`, `scene_tree_helpers.c`

---

## SAV (Save Game) File Format

The canonical save framing, first-valid redundant-copy recovery, variable-length
names, mission/player serializers, native capacity quirks and installed corpus
receipt are documented in [SAVE_FORMAT.md](SAVE_FORMAT.md). The implementation
is [inf-formats/src/save.rs](../../crates/inf-formats/src/save.rs).
[PROFILE_RUNTIME.md](PROFILE_RUNTIME.md) owns runtime caching, sidecar custody,
the derived profile menu and name-entry preparation.

The first DWORD after the name is the mission selection; the next is elapsed
seconds. Neither has a fixed file offset independent of name length. Original
`Infest.sav` is read-only evidence; portable writes target a separate sidecar.

---

## Key Functions

### File I/O

| Address | Function | Description |
|---------|----------|-------------|
| 0x00511B56 | sprintf wrapper | Formats strings |
| 0x00511C7D | fopen wrapper | Opens files |
| 0x00511BCF | fclose wrapper | Closes files |
| 0x00463800 | get_game_path | Gets install path from registry |

### OVL Loading

| Address | Function | Description |
|---------|----------|-------------|
| 0x004EE4E0 | LoadOverlay | Main OVL loading function |
| 0x004948A0 | PushContext | Exception handling setup |
| 0x004949A0 | PopContext | Exception handling cleanup |
| 0x004F3D80 | OpenFile | Opens file with mode |

### Save System

| Address | Function | Description |
|---------|----------|-------------|
| 0x00449A70 | InitSaveSystem | Initializes save game system |
| 0x00449E00 | InitCRC32Table | Builds CRC32 lookup table |

---

## Data Structures

### Resource Handler Table (0x0053AA14) — FULLY DECOMPILED

16 resource handlers registered via MSVC static initializers. Each handler is a C++ object
with a vtable containing 4 function pointers:

| Vtable Offset | Purpose | Shared? |
|---------------|---------|---------|
| +0x00 | Destructor/release | All: `0x004EFBF0` (returns 0) |
| +0x04 | Init/setup | Per-handler |
| +0x08 | **Load function** | Per-handler (14 unique) |
| +0x0C | Unload/cleanup | Most: `0x0050ED60` (calls free) |

Handler objects are in BSS (0x0055ED90–0x0055EEF8), constructed at startup.

**All Load functions decompiled** (private Ghidra exports).

| Handler | Load Address | Record Stride | Purpose |
|---------|-------------|---------------|---------|
| 0 | `0x0050ECD0` | variable (strings) | **String resources** (null-terminated) |
| 1 | `0x0050ED90` | 4 bytes | Four-byte resources; semantics unresolved |
| 2,6 | `0x0050F3F0` | 4 bytes | Four-byte resources; semantics unresolved |
| 3 | `0x0050EE80` | variable (0x1C sub) | **Sprite descriptors, variants, palettes, and atlas records** |
| **4** | `0x0050F250` | 0x40 header + variable tables | **Fonts, signed glyph metrics, and sprite arguments** |
| 5 | `0x0050F340` | 0x14 (20 bytes) | Display presets; packed policy fields partly unresolved |
| **7** | `0x0050F490` | **4 bytes** | **Raw uint32 resources; terrain levels use an 8-entry RGB555 palette view** |
| **8** | `0x0050F5A0` | **variable** | **3D model command programs and names** |
| 9 | `0x0050F760` | variable | **PCM sound entries and aliases** |
| 10 | `0x0050F890` | 0x1C (28 bytes) + buffers | Signed terrain-map sample grids; see [sample assets](SAMPLE_ASSETS.md) |
| 11 | `0x00437B50` | 0x400 (1024 bytes) | Terrain/furniture model lookup; raw second words |
| **12** | `0x004507D0` | **0x80C14+ bytes** | **Terrain grid: 512×512 cells + sparse records** |
| 13,14 | `0x004480B0` | relocation framed | Constructor/model inputs, shared damage responses, world placements and ordered instance commands; derived/command payloads partly open |
| 15 | `0x00481090` | 0x14 (20 bytes) | Terrain effect selection; downstream value meanings open |

**Key shared functions:**
- `FUN_004f3de0` — Buffered `fread()` with adaptive block sizing
- `FUN_00495970` — `malloc()` wrapper
- `FUN_004efc00` — Resource registration into lookup table
- `FUN_004959c0` — `free()` wrapper

**Handler 4 — fonts and glyph tables:**

The loader and text consumers establish 0x40-byte font headers followed by
signed 8-byte glyph metrics and full-width sprite arguments. The canonical
[font parser](../../crates/inf-formats/src/fonts.rs) preserves original header
words and bounded table spans without rewriting disk pointer placeholders.
Retail has five fonts in `0d0.ovl` through `0d4.ovl`, occupying 14,164 framed
bytes with no trailing payload. Layout, field meanings, exact corpus shapes,
and runtime text rules are owned by [text runtime](TEXT_RUNTIME.md).
The historical `opaque_resources/handler_04/` export still preserves the raw
bytes losslessly; its directory name no longer describes runtime semantics.

**Handler 8 sub-block format:**

Handler 8 reads multiple sub-blocks in a loop. Each sub-block:
```
Offset  Size  Field
0x00    2     uint16 alloc_size (bytes of model data)
0x02    2     uint16 entry_count (model entries in this sub-block)
0x04    var   alloc_size bytes of model entry data
```

The exact 0x20-byte entry header, pool-length rules, optional name, alignment,
and recursive command grammar are documented in the `1.ovl` section above and
implemented by `port/crates/inf-formats/src/models.rs`. The full retail corpus
is 36 regions, 457 blocks, and 5,142 parsed entries; global IDs 0 through 759
have stable names across repeated registrations.

**Handler 9 — Sound Effects:** ✓ EXTRACTED

Handler 9 (`FUN_0050f760`) stores **raw signed 16-bit mono PCM audio** at 22050 Hz.
It reads `count` entries of 0x14 bytes each, then performs a pointer fixup pass.
Only present in `0.ovl` (30 entries, 947KB) and `1.ovl` (47 entries, 1.6MB).

Audio format confirmed from WAVEFORMATEX structure in Infest.exe @ 0x00145558:
WAVE_FORMAT_PCM, 1 channel, 22050 Hz, 16-bit signed LE, 2 bytes/sample.

```
Offset  Size  Field
0x00    4     Type (1=data blob, 5=alias/cross-ref)
0x04    4     Type=1: byte offset from alloc base to PCM data
              Type=5: resource slot index (low 16 bits)
0x08    4     Type=1: data blob size in bytes (exact, verified)
              Type=5: 16.16 random pitch spread
0x0C    4     Type=1: 0
              Type=5: 16.16 pitch multiplier
0x10    4     Type=1: 0
              Type=5: 16.16 gain multiplier
```

**Pointer fixup (second pass):**
- Type=1: `entry[0x04] += allocation_base` — resolves to a PCM data blob within the
  Handler 9 block. Entries at the start form a directory; data fills the remaining ~95%.
- Type=5: `entry[0x04] = lookup(short_value)` — alias referencing another sound by
  resource slot index, with playback parameter overrides (pitch spread, pitch,
  gain). These are not left/right channel gains: consumer `004F4230` multiplies
  pitch by `+0C`, consumes one `0048F060` RNG result to vary it using `+08`,
  then multiplies gain by `+10`. It recursively applies aliases before creating
  the DirectSound voice. The unusual gain branch for inputs at least `65536`
  uses the low **byte**, not low WORD; retain the literal native arithmetic.
  In 0.ovl, xref indices are within 0-29 (same OVL). In 1.ovl, xref indices > 46
  reference slots registered by 0.ovl (resource array accumulates across OVL loads).

**Extraction results:**
- 58 WAV files (24 from 0.ovl + 34 from 1.ovl), 2,541,972 bytes, 57.6s total
- 19 aliases (6 from 0.ovl + 13 from 1.ovl) with pitch/volume parameters
- Individual durations range from 0.04s to 9.6s
- Some blobs duplicated across OVLs (shared SFX like UI sounds)

**Tools:**
- `port/crates/inf-formats/src/sound.rs` — strict Handler-9 decoder and WAV wrapper
- `cargo run -p inf-extract -- sounds` (from `port/`) — staged Rust export with
  local/global slot provenance and alias metadata

**Critical finding: NO COMPRESSION in any handler.** The OVL file is read sequentially
with raw `fread()` calls. Data is not compressed.

### Handler 3 Logical Entries and Physical Variants

Handler 3 (`FUN_0050ee80`) stores the exact number of logical entries requested
by `PreLoad.dat`. Each begins with two render-mode selectors and is followed by
the selector-derived number of 0x1c-byte physical variants. No probing is used:
mode 0 establishes `selector + 1` variants unless it is `0xff`, and a present
mode-1 selector replaces that count with its own `selector + 1`, exactly as the
loader does. In the retail corpus every entry has one or two variants.

**Handler 3 vtable** at `0x00527610` has 5 entries (not the standard 4):

| Vtable Offset | Address | Purpose |
|---------------|---------|---------|
| +0x00 | `0x004EFBF0` | Destructor (shared, returns 0) |
| +0x04 | `0x00474F50` | Init |
| +0x08 | `0x0050EE80` | **Load** (logical-entry parsing + physical-variant fixups) |
| +0x0C | `0x0050F150` | **Unload** (iterates variants, releases runtime surfaces) |
| +0x10 | `0x0050F1F0` | **Texture buffer allocator** (called BEFORE Load) |

**Texture buffer allocation (`FUN_0050f1f0`):**

Before Handler 3's Load function is called, the engine invokes `vtable[+0x10]` to
allocate the texture data buffer. This reads a `uint32` buffer size from the file
stream and calls `malloc()`. The pointer is stored in `DAT_0065f134`. PreLoad.dat
specifies 5,273,600 bytes (~5MB) at offset 0x22 as the texture buffer size.

**Entry header (4 bytes):**

```
Offset  Size  Field
0x00    1     mode-0 physical-variant selector (`0xff` means absent)
0x01    1     mode-1 physical-variant selector (`0xff` means absent)
0x02    2     reserved (zero throughout retail and demo corpora)
```

Retail has 8,967 one-variant entries and 865 two-variant entries. A logical
entry therefore occupies either 32 bytes (`4 + 1*0x1c`) or 60 bytes
(`4 + 2*0x1c`). The selectors also say which physical variant each rendering
mode chooses; they are not merely a count encoding.

**Sub-entry (0x1C = 28 bytes, repeated sub_count times):**

```
Offset  Size  Field         Description
0x00    2     sprite_id     Descriptor/resource identity
0x02    2     reserved      Zero in the proven corpora
0x04    2     flags         Encoding, mask, blend, and shared-palette flags
0x06    2     color_count   Indexed palette size; zero for direct RGB555
0x08    4     atlas_offset  Byte offset = `y * 2048 + x_bytes`
0x0C    4     palette_off   Offset from the Handler-3 main-payload start
0x10    2     width         Width in pixels
0x12    2     height        Height in pixels
0x14    4     runtime_0     Runtime surface/object slot; zero on disk
0x18    4     runtime_1     Runtime surface/object slot; zero on disk
```

`atlas_offset` is measured in bytes, so a direct-RGB555 record spans
`width * 2` atlas X bytes while an indexed record spans `width`. The exact
`(x_bytes, y, x_end, y_end)` rectangle, rather than origin alone, is used to
match the post-payload atlas record.

**Runtime fixup code** (from `FUN_0050ee80`, `resource_handlers.c`):
```c
variant[+0x08] += DAT_0065f134;             // atlas buffer byte offset -> pointer
if (variant[+0x0C] != 0)
    variant[+0x0C] += (int)piVar2 + 4;      // main-payload palette offset -> pointer
```

The disk representation keeps both relative values. Width and height are the
two uint16 fields at +0x10/+0x12; +0x0c is not a packed dimension word.

**Field semantics (confirmed from 0.ovl):**

| Field | 0.ovl Value | Interpretation |
|-------|-------------|----------------|
| sprite_id | 0..7 | Descriptor identity |
| flags | 0x0014 | Additive indexed variant, not a type enum |
| color_count | 62 (0x3e) | RGB555 palette color count |
| atlas_offset | 0 | All eight variants share atlas rectangle `(0,0)-(32,32)` |
| palette_off | 0x100, 0x10fc, ... | Main-payload-relative palette offsets |
| width/height | 32/32 | Authored pixel dimensions |
| runtime_0/1 | 0 | Filled/released by the runtime |

**Post-load linked list registration (`FUN_0050e5b0`):**

After parsing all entries, Handler 3 calls `FUN_0050e5b0` to register the loaded
block into a global doubly-linked list anchored at `DAT_0065ee70`. The first 0x14
bytes of the allocation are overwritten with list metadata (self-pointer, 2 metadata
words, prev/next pointers). If `DAT_0065f114` is nonzero (resources already active),
a callback is triggered. Handler 7 also uses this same registration function.

**Unload (`FUN_0050f150`):**

Iterates all logical entries and physical variants. For each variant, checks +0x14 and +0x18:
if nonzero, calls `FUN_004f8c80()` and `FUN_004f8cb0()` respectively to release
DirectDraw surfaces/texture objects created at runtime. Then unlinks from the linked
list and frees the allocation.

**Cross-OVL batch analysis (86 OVLs with Handler 3 data):**

| Metric | Value |
|--------|-------|
| Total OVLs | 86 |
| Registered logical entries | 9,832 |
| Registered physical variants | 10,697 |
| Indexed / direct RGB555 variants | 10,613 / 84 |
| One-variant / two-variant entries | 8,967 / 865 |
| Shared-palette variants | 1,150 |
| Atlas records | 8,419 |
| Loader-ignored orphan entries/records | 3 / 3 |

The three additional structurally complete variants occur after the requested
tables in `9d2.ovl`, `9d3.ovl`, and `9d4.ovl`; the retail count ignores them, so
the Rust exporter labels them as orphans rather than silently registering them.
`0.ovl` has eight entries and eight physical variants.

**Inline functions near Handler 3 (at `0x0050EE30` and `0x0050EE40`):**

Two small helper functions exist as inline code (too small for Ghidra's function
detection):
- `0x0050EE30`: Resets `DAT_0065f130` and `DAT_0065f134` to 0 (clears texture buffer globals)
- `0x0050EE40`: Frees texture buffer if `DAT_0065f134` is nonzero

These totals are asserted by `port/crates/inf-formats/tests/sprites.rs` against
both retail and demo installations.

### Handler 3 Texture Buffer Architecture (Decoded)

The Handler-3 main payload and post-payload loop together form a complete
sprite/texture system. Logical entry strides are determined by their selectors,
not by probing. The renderer has two explicit modes, and a two-variant logical
entry can select a different physical descriptor for each mode.

**Main block layout:**

```
[entry_0: 4B selectors + N × 0x1C variants]
[entry_1: 4B selectors + N × 0x1C variants]
...
[palette tables referenced by indexed variants]
```

The loader allocates an extra runtime prefix containing the registered count;
that prefix is not part of the OVL main payload. Palette offsets are relative
to the on-disk main-payload start.

**Shade palette table format (per entry):**

Each indexed variant's palette is located at
`main_payload_start + palette_offset`. It consists of all shade ramps first,
then a contiguous base palette. Both sections contain RGB555 words:

```
Offset                         Size                    Field
0                              color_count * 64        [color][32] RGB555 shade ramps
color_count * 64               color_count * 2         base RGB555 palette
```

**Pixel-major organization** (confirmed for ALL 86 OVLs):
- shade word for index `b`, level `s`: `ramps[b * 32 + s]`
- base word for index `b`: `base[b]`
- valid indexed pixels are strictly less than `color_count`
- base and each explicit shade 0 through 31 are separate faithful projections

**RGB555 format** (15-bit, NOT RGB565):
```
R = ((val >> 10) & 0x1F) * 255 // 31
G = ((val >>  5) & 0x1F) * 255 // 31
B = ((val >>  0) & 0x1F) * 255 // 31
```

Note: The earlier 0.ovl documentation describes palettes as "RGB565" — this was incorrect.
All Handler 3 palettes use RGB555 (5-5-5 bit layout, 15 bits used of 16).

**Extra loop (pixel data records):**

After the main block, Handler 3 reads an 0xFFFF-terminated loop of variable-length records.
Each record contains the exact byte rectangle uploaded into the texture atlas:

```
Record format (texture atlas rectangle):
  Offset  Size          Field
  0x00    2             x_start (uint16) — atlas X position; terminates loop when 0xFFFF
  0x02    2             y_start (uint16) — atlas Y position
  0x04    2             x_end (uint16)   — atlas X + width
  0x06    2             y_end (uint16)   — atlas Y + height
  0x08    x_byte_width×height  raw pixel bytes

  x_byte_width = x_end - x_start
  height = y_end - y_start
  atlas_offset_bytes = y_start * 2048 + x_start
```

For indexed variants `x_byte_width == descriptor width`; for direct RGB555
variants it is twice the descriptor width and the bytes decode as little-endian
uint16 RGB555 words.

**Entry-to-record mapping (atlas offset matching):**

Variants are matched against the complete expected byte rectangle derived from
`atlas_offset`, width, height, and encoding. Multiple variants may intentionally
share one record while selecting different palettes, as all eight `0.ovl`
sprites do.

**Extraction statistics (86 OVLs with Handler 3 data):**

| Metric | Value |
|--------|-------|
| Registered logical entries | 9,832 |
| Exported physical variants | 10,700 (10,697 registered + 3 orphans) |
| Indexed / direct RGB555 | 10,616 / 84 including orphans |
| Atlas records | 8,419 |
| Unique emitted palettes | 3,081 |

Transparency and composition come from flags, not a palette-color heuristic:
bit 0 masks index/RGB555 value zero; bit 1 selects direct RGB555; bits 3, 4,
and 5 select source-alpha, additive, and inverse-source-color darken behavior;
bit 15 reuses the preceding physical variant's palette.

**Canonical tool:** `cargo run -p inf-extract -- sprites --shade base`

**Generated output:** `extracted/sprites/` contains a manifest,
byte-exact framed-region files, lossless raw pixels, deduplicated RGB555
palette files, and PNG projections.

### Cross-Handler Patterns (Learned from Handler 3 Decoding)

These patterns emerged from the Handler 3 sprite extraction work. They are
Handler-3 observations, not defaults to impose on another handler without C evidence.

**1. Main block = directory/metadata, extra loop = bulk data.**
Handler 3's main block holds entry descriptors and palette tables. The actual pixel data
is in the extra loop (0xFFFF-terminated records after the main block). Other handlers with
extra reads may follow this same split. Handler 7 uses the same linked-list registration
via `FUN_0050e5b0`, but its file payload is a flat array of four-byte resources;
registration alone is not evidence of an atlas-style extra-data loop. Don't assume all
useful data is in the main block.

**2. Runtime allocation prefixes are not on-disk payload fields.**
Handler 3 allocates bookkeeping before the bytes read from the OVL and registers
that allocation with `FUN_0050e5b0`. The on-disk main payload begins with the
first logical entry, and palette offsets use that payload start.

**3. Registration and physical storage are distinct.**
Different logical variants can share the same atlas record or palette. Preserve
the selector, descriptor, atlas-record, and global-registration identities rather
than deduplicating them into a single guessed sprite.

**4. Texture atlas architecture (shared with V2000).**
The engine uses a texture atlas with row stride 2048 bytes. Post-payload records
are placed at byte coordinates `(x_start, y_start)`; variants reference them
through `atlas_offset` at variant +0x08 and store width and height separately at
+0x10/+0x12.

**5. RGB555 (not RGB565) for palettes.**
The engine uses 15-bit RGB555 (5-5-5) for palette entries, not 16-bit RGB565 (5-6-5).
This was initially misidentified. When encountering uint16 color values in other handlers,
try RGB555 first.

**6. Shade tables = pre-computed lighting.**
Rather than storing a single palette and computing lighting at runtime, the engine stores
32 pre-computed shade variants for each color. This "baked lighting" pattern may appear
wherever the engine needs per-pixel lighting without a shader — e.g., terrain textures,
particle effects, or UI elements with highlight/shadow states.

### Handler 7 Four-Byte Resources and Terrain Palette (Decoded)

Handler 7 (`FUN_0050f490`) is generically a flat array of four-byte resources,
not intrinsically an eight-material terrain format. Its standard block is:

```
[uint32 alloc_size][requested_count × uint32 resource]
alloc_size = requested_count × 4
```

`Handler7_Load` registers each complete `uint32`. Do not discard or synthesize
the high word merely because it is zero in the terrain corpus. Retail has 35 H7
regions and 378 entries: level 0 requests 98, level 1 requests 16, and each
terrain level 11–43 requests eight. All eight-entry terrain banks have zero high
words; their low words use RGB555 (`0rrrrrgggggbbbbb`).

For a terrain bank, only these roles are proven:

| H7 entry | Proven runtime role |
|----------|---------------------|
| 0–3 | Selected by the H12 cell's two-bit material (`FUN_00465070`, `FUN_00489160`) |
| 4 | Selected by the separate plane/water path (`FUN_004650e0`) |
| 5–7 | Raw RGB555 values; distinct roles unresolved |

The old claim that H12 supplied a three-bit material index selecting all eight
H7 entries is disproven. H12 has only a two-bit material field.

**Example (level 11):**

| H7 entry | RGB555 | Role |
|----------|--------|------|
| 0 | `0x2128` | Surface material 0 |
| 1 | `0x318C` | Surface material 1 |
| 2 | `0x318C` | Surface material 2 |
| 3 | `0x08CC` | Surface material 3 |
| 4 | `0x3DE7` | Plane/water value |
| 5 | `0x2529` | Unresolved |
| 6 | `0x1D84` | Unresolved |
| 7 | `0x314A` | Unresolved |

The strict Rust implementation is `crates/inf-formats/src/materials.rs`.
It parses every H7 bank as raw `uint32` entries and only exposes the terrain view
when the bank contains exactly eight entries.

### Handler 12 Terrain Grid Format (Decoded)

Handler 12 (`FUN_004507d0`) is present in the 33 base OVLs for levels 11–43,
always with one entry. The three demo terrain regions (levels 12, 28, and 35)
are byte-identical to retail. A standard handler allocation can contain more
than one variable-length entry, so parsers must advance by each entry's summed
sparse-bucket sizes rather than assume one global sentinel.

**Entry structure (fixed `0x80C14` bytes plus sparse data):**

```
Offset     Size        Content
+0x00000   0x14        Prefix (decoded below)
+0x00014   0x80000     512×512 little-endian uint16 cells
+0x80014   0x400       512 × uint16 sparse-bucket byte sizes
+0x80414   0x800       512 × uint32 runtime pointer placeholders
+0x80C14   variable    Concatenated four-byte sparse records, bucket order 0..511
```

The loader replaces every pointer placeholder with a runtime pointer, changes
each bucket size from bytes to record count with `size >> 2`, and advances by
the original byte size. Retail pointer placeholders are all zero, but the C
does not validate them before overwriting them; a provenance-preserving parser
should retain rather than interpret them. Every retail bucket size is divisible
by four and the 512 sizes sum exactly to the remaining entry bytes.

#### Prefix (`+0x00..+0x13`)

| Offset | Type | Proven use |
|--------|------|------------|
| `+0x00` | `int32` | Signed fixed-point plane threshold, used by plane/water comparisons |
| `+0x04` | `int32` | Terrain light vector X |
| `+0x08` | `int32` | Terrain light vector Y |
| `+0x0C` | `int32` | Terrain light vector Z |
| `+0x10` | `uint32` | Unresolved; preserve raw |

`FUN_0044fdd0` converts the three signed light-vector components to floats and
normalizes them. The plane and cell heights require scene state that is **not
stored in H12**. An adjacent level-descriptor flag (`+0x04 & 0x80`) selects:

| Descriptor bit | Cell height | Plane threshold |
|----------------|-------------|-----------------|
| clear | `height_i8 << 5` | `plane_height_fixed >> 8` |
| set | `height_i8 << 4` | `plane_height_fixed >> 9` |

Do not bake one shift into an extractor or label the prefix as a universal Q
format. A standalone dump should expose signed `int8` heights and raw prefix
values; world-space height and a plane/water mask require the corresponding
scene descriptor.

The stored grid is always 512×512 and indexed `cell[x * 512 + y]`. World X/Y
coordinates are 16-bit and wrap. Runtime grid mode is also external state:

| Mode | World step/mask | Stored neighbour | Interpolation shift |
|------|-----------------|-------------------|---------------------|
| fine | `128 / 0x7F` | `+1` | 7 |
| coarse | `256 / 0xFF` | `+2` | 8 |

H12 contains no separate finite map origin or dimensions beyond this toroidal
512×512 storage.

#### Cell encoding (`uint16`)

| Bits | Content |
|------|---------|
| 0–7 | Signed `int8` height |
| 8–9 | Surface material 0–3 (selects H7 entries 0–3) |
| 10 | Exact marker that a matching sparse record exists |
| 11 | Quad diagonal; zero in every retail file, recomputed by `FUN_004506a0` |
| 12–15 | One four-bit shade/intensity scalar, **not four independent flags** |

`FUN_00450a30` samples `(int8)cell << height_shift` and uses bit 11 to select
the triangle diagonal. `FUN_00450890` writes terrain deformation back to the low
byte. `game_logic.c:14008-14020` reads the high nibble, clamps values above 8,
and scales RGB channels by `shade / 8`; `game_logic.c:57675-57695` also treats
it as one scalar.

Across all 8,650,752 retail cells, the exact shade histogram for values 0..15 is:

```
503252, 48368, 892258, 338014, 504214, 1059692, 1655897, 1014366,
1255783, 604617, 408633, 360197, 3594, 1756, 21, 90
```

The `1,059,692` count for shade 5 corrects the earlier transcribed
`1,055,969`. There are exactly 5,375 bit-10 cells and zero stored bit-11 cells.
Material values 0..3 occur `2,888,527`, `1,889,015`, `1,896,353`, and
`1,976,857` times respectively.
The earlier "9-bit tile index = three-bit material + six-bit variant" and
"bits 12–15 are water/visibility flags" interpretations are disproven.

#### Sparse buckets and records

Bucket index is the grid's second coordinate `y`. Each bucket contains records
sorted strictly by `x`, which `FUN_00435260` binary-searches. Each record is:

```
+0x00  uint16 x
+0x02  uint8  state
+0x03  uint8  resource_lookup
```

Both nibbles of `state` are used or mutated by runtime code, but their full
semantics remain unresolved. `resource_lookup` indexes a separately loaded
lookup at `DAT_0055eefc`; it is **not** a direct object/effect resource id, and
values `0xFD`, `0xFE`, and `0xFF` have special handling. Retail has 5,375 records,
one unique record for every bit-10 cell, with no duplicates and every bucket
strictly sorted. The old adjacency-record and direct-effect-id descriptions are
disproven.

#### Exact 55-tile blend canonicalization

For four two-bit materials packed low-to-high as `[a,b,c,d]`:

```c
combo = a + 4*b + 16*c + 64*d; // 0..255
```

`FUN_004506a0` supplies those digits from the executable's `cell[x * 512 + y]`
storage as `a=(x,y)`, `b=(x,y+step)`, `c=(x+step,y)`, and
`d=(x+step,y+step)`, with the coordinates wrapping. `step` is one stored cell
in fine mode and two in coarse mode. The renderer's four sampled corners must
retain this packing order; do not infer an image-library rotation convention
from the letters above. At startup `FUN_0044b5c0` evaluates these eight
low-to-high digit sequences:

```
0 [a,b,c,d]   1 [c,b,a,d]   2 [b,c,d,a]   3 [d,c,b,a]
4 [c,d,a,b]   5 [a,d,c,b]   6 [d,a,b,c]   7 [b,a,d,c]
```

It chooses the numerically smallest packed value; comparison is strict, so the
first case wins ties. The 55 representatives, in texture-rank order, are:

```
0,1,2,3,5,6,7,10,11,15,17,18,19,21,22,23,25,26,27,29,30,31,
34,35,38,39,42,43,46,47,51,55,59,63,85,86,87,90,91,95,102,
103,106,107,110,111,119,123,127,170,171,175,187,191,255
```

Texture resource id is `0x4FC + rank(representative)`, producing exactly
`0x4FC..0x532`. For the winning symmetry case `k`, orientation is:

```c
words = [0x33221100, 0x11223300, 0x22330011, 0x02312013,
         0x33001122, 0x11003322, 0x20130231, 0x00112233];
remap = [7, 6, 1, 0, 2, 3, 4, 5];
orientation = words[remap[k]];
```

`FUN_004506a0` clears cell bit 11 when `orientation & 0xF` is 0 or 2 and sets it
otherwise. Preserve the complete orientation word; reducing it to a guessed
PIL flip/rotation loses the original vertex routing.

#### Texture linkage and extraction status

The blend table proves the global resource range. PreLoad ancestry associates
terrain levels with selector leaves 3–10, whose H3 segment starting at local
index 14 has signed registration delta 1262. Therefore local H3 records 14..68
map to globals `local + 1262 = 0x4FC..0x532`. The exact level grouping is:

```
3: 18,24,30                 4: 26,41
5: 15,16,21,32,36,37,39,40 6: 11,19,22,25,28,35,43
7: 17,27,34                 8: 14,23,31,42
9: 12,13,20,33,38           10: 29
```

That establishes resource identity. The Rust Handler-3 decoder now recovers the
corresponding physical variants and pixels, but the exact runtime choice of
render mode/shade, orientation-word vertex routing, sampling, and material
composition has not yet been integrated into a faithful textured-terrain
renderer. Texture dimensions such as 40x40 or 100x100 from former Python
previews were never proven and are not retained as format facts.

Current tool status:

- Rust `inf-formats/src/terrain.rs` and `materials.rs` are the strict sources
  for raw H12/H7 parsing and the 256-entry blend/orientation table.
- Rust `inf-extract terrain` stages output beneath `terrain/` and supports
  repeatable exact `--ovl` filters. It preserves each complete H12 payload and
  manifest provenance, and writes lossless PNG channels for signed height
  (bias 128), material, shade, sparse marker, stored/derived fine-grid
  diagonal, canonical blend texture ID/rank, and orientation diagnostics. A
  material-color preview is emitted only when the same OVL contains the exact
  eight-entry H7 terrain palette; it is explicitly not textured terrain.
- `inf-viewer` reads H12 and the same-OVL H7 palette directly, cycling through
  height, material, shade, sparse-record, and stored-diagonal channels. It does
  not display the deleted historical map/tile previews.


### Handler 13/14 Tail Format (Decoded)

Handlers 13 and 14 share a Load function (`FUN_004480b0` at `0x004480B0`) that
calls `FUN_004eddb0` instead of the standard `[uint32 size][data]` read. This
function reads the main data block plus a "tail" containing relocation offsets
and per-entry pointer tables.

**Read sequence (FUN_004eddb0 at `0x004EDDB0`):**

```
Offset   Size              Content
0x00     4                 uint32 main_size (size of main data block)
0x04     main_size         Main data block (contains structured entries)
—        4                 uint32 N (relocation count)
—        N × 4             N × uint32 relocation offsets into main block
—        count × 4         count × uint32 entry pointer offsets
```

Total bytes consumed: `4 + main_size + 4 + N*4 + count*4`

After reading, two relocation passes patch raw file offsets into runtime pointers:
1. **Reloc pass:** For each reloc_offset[i], patch `*(base + reloc[i]) += base`
2. **Entry pass:** For each entry_ptr[i], patch `*ptr += base`

**Distribution:**

| Handler | OVLs | Count range | Tail typical | Description |
|---------|------|-------------|--------------|-------------|
| 13 | 5 (levels 1, 6, 29, 30, 32) | 1–131 | 8–532 B | Structured entries with pointer relocations |
| 14 | 33 (levels 11–43) | 1 | 772–1,696 B | Level-specific data with many relocations |

**Examples:**

| OVL | Handler | Count | Main size | N (relocs) | Tail size | Total |
|-----|---------|-------|-----------|------------|-----------|-------|
| `1.ovl` | h13 | 131 | 9,716 | 1 | 532 | 10,252 |
| `6.ovl` | h13 | 1 | 20 | 0 | 8 | 32 |
| `11.ovl` | h14 | 1 | 3,224 | 191 | 772 | 4,000 |
| `32.ovl` | h13+h14 | 6+1 | 548+6,636 | 0+422 | 28+1,696 | 8,916 |

**Machine-checked census:** `port/crates/inf-formats/src/handler13.rs`
pins this distribution from retail `PreLoad.dat` (five Handler-13 regions
holding 162 resources, one Handler-14 resource per level 11–43, all base
phase). The [world asset decoder](WORLD_ASSETS.md) additionally follows the
proven constructor and placement consumers while retaining opaque class data.

The canonical raw opaque export also retains Handler 4, now decoded as
[fonts](TEXT_RUNTIME.md). Across Handlers 4/13/14
it preserves 43 regions from 40 OVLs, 200 requested resources, and 362,964
exact framed bytes (`14,164 / 12,504 / 336,296` bytes by handler).

**Decompiled source:** `resource_handlers.c` (Section 4: FUN_004eddb0)
**Decompilation script:** `tools/ghidra_decompile_handler13.py`

### Handler 14 World Placement Graph

Handler 14 is present in all 33 terrain levels (11–43), always with count=1.
The exact `FUN_004eddb0` framing occupies 336,296 bytes across those regions.
`inf-extract worlds` decodes all 4,486 ordered placement prefixes and their
relocation provenance. Native `0048DC30`/`0048E170` establish the header's
count at `+0x38`, pointer array at `+0x3C`, and fixed placement inputs. Exact
fields, bounds, coordinate limitations and opaque instance suffixes are owned
by [WORLD_ASSETS.md](WORLD_ASSETS.md#handler-14-authored-placement-graph).
The retired Python investigator's guessed `tile_index` and other terrain
meanings are not format authority.

### Handler 13 Constructor Resources

Five OVLs contain Handler 13 and request 162 resources in total. The shared
`FUN_004eddb0` framing is exact. Native `00460310`/`00460120` establish the
common four-byte prefix, including 85 distinct constructor selectors. The
canonical decoder preserves class-specific suffix offsets without guessing
their extent; see [WORLD_ASSETS.md](WORLD_ASSETS.md#handler-13-common-constructor-prefix).
The [constructor asset decoder](CONSTRUCTOR_ASSETS.md) resolves all installed
dispatch profiles and exposes 155 initial model bindings, 123 shared actor
prefixes (52 with derived fields and sound arguments), 32 flags-only prefixes,
two terrain-light prefixes, one selector-86 prefix, three selector-83
prefixes and one specialized relocated sixteen-pair table.
These bounded reads do not establish complete class schemas or final visuals.
Former visual-reference, hit-point, capability and weapon labels from deleted
Python projections are not recovered class schemas.

`inf-extract worlds` publishes proven constructor fields, damage responses,
and [H14 instance command graphs](INSTANCE_ASSETS.md) with exact framed bytes
in `world_assets/` (schema 9). [Resident inspection](WORLD_INSPECTION.md) resolves
class/model references and bounded command views through actual registrations.
`inf-extract opaque` also retains the archival copy of all
five regions (12,504 framed bytes), with local/global registrations.

### Handler 11 Terrain Lookup Table

Handler 11 stores eight fixed tables in shared family banks 3–10.
Stride = `0x400` (1,024 bytes per resource), containing 256 four-byte slots.
`004508E0` indexes this table using H12 sparse-record byte `+3`, resolves
the first signed WORD through H8, and reads the model radius at `+0x0E`.
This proves a terrain/furniture model lookup, not an entity-placement table.

```
256 × 4-byte slots:
  Offset  Size    Field       Description
  +0x00   uint16  raw_word_0  Signed H8 slot at the proven consumer
  +0x02   uint16  raw_word_1  Remaining consumer semantics open
```

Slot 0 is zero in all eight tables; first-word values 69/70/71 repeat with
second words 26/25/24. These are corpus observations, not proof of an entity
class or a general `(group << 8) | value` encoding.

**Extraction tool:** `cargo run --release -p inf-extract -- missions` (from `port/`).
The historical command/type names preserve raw tables, not inferred meanings.

### Handler 15 Terrain Effect Selection

Handler 15 stores five DWORDs per resource in shared family banks 3–10.
Stride = `0x14`. `0044FDD0` publishes the selected resource; `00465070`
selects the first four words using terrain material bits, and `004650E0`
returns the fifth word. Downstream effect meanings remain open.

```
Offset  Size    Field    Description
+0x00   uint32  field_0  Terrain-material selection 0
+0x04   uint32  field_1  Terrain-material selection 1
+0x08   uint32  field_2  Terrain-material selection 2
+0x0C   uint32  field_3  Terrain-material selection 3
+0x10   uint32  field_4  Almost always 6 (level 7 = 7)
```

**Extraction tool:** `cargo run --release -p inf-extract -- missions` (from `port/`)
**Output:** staged `extracted/missions/manifest.json`

### Level Object Structure

- Offset 0x0C: Level/world index

### Direct OVL Viewer

`cargo run -p inf-viewer` opens the SDL workbench directly from the installed
`PreLoad.dat` and `Overlay/` corpus; it has no dependency on `extracted/`.
Models, Sprites, and Terrain are separate tabs (F1/F2/F3), each with previous
and next navigation (Left/Right or Page Up/Page Down). `--mode`, `--model`,
`--presentation`, `--ovl`, and `--data-root` provide direct startup selection.
Model search accepts a name, global ID (`#123` or `0x7b`), or source OVL.
An `--ovl` filter limits the assets presented by the workbench but retains the
complete H3/H7 registration pool: Handler-8 faces address global resource
tables and can legitimately depend on another OVL (for example, `17.ovl`'s
BIGDISH uses H3 #221 from the active `1dN.ovl`). Resource selection replays the
proven base -> difficulty -> language -> combined replacement order; the model
view exposes difficulty and currently uses retail language index zero.

In Models, right-drag orbits, the mouse wheel zooms, and middle-drag pans.
Reset Position/`R` is disabled until a pan occurs, and changing or searching for
a model automatically resets its position. The display remains explicitly a
diagnostic static projection. Unresolved runtime dependencies are reported; `A`
switches the one model catalog between local-shell and neutral-composite
presentations. The neutral presentation substitutes zero only for callback-backed
joint inputs whose attachment path and resulting saved basis are otherwise
proven, and labels that policy on screen. Recovered faces now use a software color/depth target with
Handler-7 RGB555 fills and Handler-3 decoded textures. `T` toggles texture
sampling, `D` cycles the active difficulty overlay (`d0`..`d4`), and `M` cycles
the Handler-3 selector mode. `Q` exposes the Direct3D backend's state-bit-14
quad diagonal (`0-2` or `1-3`) rather than silently choosing one. The original
software backend supplies both fixed triangle/quad texture corners and fan
Q0.16 UV scaling. Missing resources and Handler-3 source-alpha surfaces remain
as cyan edges; the latter is deliberate until authored non-binary alpha and its
blend path are proven. Lighting-slot evaluation and the shadow pass are still
deliberately absent, so recovered base colors are not presented as a canonical
lit animation frame.

Sprites expose the two runtime modes, every physical variant, and the base plus
all 32 shade palettes (`M`, `V`, `S`). Terrain exposes exact height, material,
shade, sparse-record, and stored-diagonal channels (`C`); it is not a guessed
textured map renderer.

---

## Notes

1. The game uses a custom exception handling mechanism with a stack-based context system
2. File paths are constructed using sprintf-style formatting
3. The overlay system supports hot-loading of resources based on difficulty and language
4. Save files use CRC32 checksums for integrity verification
5. **No compression** is used in OVL files — all data is stored raw

## Tools

### Canonical Rust Pipeline

- `../port/crates/inf-formats` contains the strict PreLoad, framing, Handler
  0/3/4/7/8/9/11/12/13/14/15, fixed-word, and fixed-record decoders.
- `cargo run --release -p inf-extract -- all` regenerates 13 families: installation,
  sounds, sprites, strings, missions, word resources, materials, record tables,
  raw Handler-4/13/14 regions, models, terrain, fonts and world assets.
  [Asset coverage](ASSET_COVERAGE.md) distinguishes structural decoding,
  bounded semantics and complete presentation; class payloads remain open.
- `inf-extract installation` preserves the exact 18,280-byte `PreLoad.dat`, its
  parsed tree, and the complete deterministic inventory of 1,932 installed OVLs
  (494 active handler regions covering 158,707,834 bytes).
- Publication is atomic **per family**, not one transaction spanning the output
  root. If a late family fails, earlier families from that invocation may already
  be current; fix the cause and rerun `all`.
- `cargo run -p inf-viewer` opens the direct-OVL Models/Sprites/Terrain
  workbench. It never reads generated extraction output.

### Ghidra / Reverse Engineering
- `tools/vtable_analyzer.py` - Forensic PE survey of handler vtable addresses;
  Rust framing is canonical.
- `tools/ghidra_decompile_handlers.py` - Decompiles handler Load functions for
  provenance; it is not an alternate extractor.
- `tools/ghidra_decompile_handler13.py` - Decompiles `FUN_004eddb0`, the shared
  Handler-13/14 relocation-framed reader.
- `tools/ghidra_decompile_cmd_handlers.py` - Dumps an observed 256-entry word
  dispatch table and its handler bodies without asserting presentation semantics.
- `tools/ghidra_find_model_dispatch_v2.py` - Historical broad candidate survey;
  its hits are not identified model-renderer functions.
- `tools/ghidra_trace_load_callers.py` - Forensic LoadOverlay caller trace. Its
  default reports go to ignored `forensic-output/`, never `extracted/` or a
  curated checked-in path.
- `tools/ghidra_trace_scene_tree.py` - Forensic scene-tree/resource-count trace.
  - Discovers PreLoad.dat as source of resource request arrays
  - Parses binary format: header, handler capacities, recursive scene tree
  - Historical reports are curated under `reference/`; the Rust installation
    export now emits the canonical parsed tree and inventory

### Decompiled Source
- `resource_handlers.c` - All 16 handler Load functions + 6 helpers + Handler 3 post-processor/sprite-atlas buffer/unload + Handler 13/14 reader (FUN_004eddb0)
- `handler13_reader.c` - Raw Ghidra decompilation of FUN_004eddb0 + FUN_004480b0 + FUN_00494910
- `ovl_functions.c` - LoadOverlay main function
- `ovl_callers.c` - LoadOverlay caller functions (struct-based dispatch)
- `scene_tree_functions.c` - Scene tree parser (3 mutually-recursive functions for PreLoad.dat)
- `scene_tree_helpers.c` - 12 helper functions (malloc wrapper, error handler, file I/O)

### Generated Data and Curated Evidence

- `extracted/` is entirely Rust-generated and ignored by Git. It is disposable;
  regenerate it with `inf-extract all` rather than editing or citing it as a
  source of truth.
- `reference/` contains curated screenshots, preload/request reports, and other
  durable evidence that should remain versioned.
- `forensic-output/` is ignored scratch space for rerunnable Ghidra probes; move
  reviewed evidence into `reference/` intentionally rather than overwriting it.
