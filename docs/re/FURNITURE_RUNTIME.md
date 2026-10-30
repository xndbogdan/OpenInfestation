# Infestation terrain furniture drawing

This authority owns the shared furniture submission reached by terrain draw
`0044AA10 → 0044CD00 → 0044CEF0 → 004359A0`. It is used by world views and
the briefing terrain pane, independently of campaign selection. The
[terrain owner](TERRAIN_RUNTIME.md#furniture-item-setup-0044cef0) owns sparse
record enumeration, position and signed ambient-light setup. Its
[pending effect queue](TERRAIN_RUNTIME.md#pending-furniture-effect-executor)
owns damage/program admission and execution. This document owns the later
basis, effect, fade and H1 draw contract.

Authority is the supported retail executable's unchanged instructions,
particularly `004354F0`, `00435690`, `00435870`, `004358A0`, `004359A0` and
`00437A70`. bulk/game_logic.c supplies the decompiled
guide. Routine fixtures and authored pixels remain distinct from a complete
retail scene comparison.

## Shared custody and callers

[terrain_furniture::draw](../../crates/inf-game/src/terrain_furniture/draw.rs)
owns the complete item orchestrator. The live software bridge receives the
actual mutable terrain, record identity, pending queue, process light grid,
particle pool, shared RNG, animation clock, load factor, executable, particle
birth inputs and retained basis state. H11/H8/material reads use the same
resident registration snapshot as the renderer.

| Caller | Reached native policy |
| --- | --- |
| World frame | Retained world camera/fade and coordinate shift; shared model/particle/terrain queue. Furniture births occur after particle transfer and remain pending until the next transfer. |
| Briefing terrain pane | Pane camera and retained context fade, separate from an earlier optional backdrop's `INT_MAX` fade. Its post-AA10 flush and depth clear remain explicit. |
| Menu, name-entry, panels and model previews | No AA10 furniture submission. Their callback, lighting and row-source policies remain separate. |
| Direct terrain/strip fixtures | Explicit required furniture service or a reported omission; no implicit process pool or successful effect fallback. |

Only the furniture phase borrows terrain mutably. Ground strips subsequently
reborrow it read-only. Bit-8 writes change the retained sparse record; drawing
does not clone terrain to absorb mutations. Furniture lighting reads the
actual grid/anchors even when settings disable ground-light accumulation.
AA10 first resolves H7/74 and writes its converted color to context +6C at
`0044AA8E`; the night branch replaces it with packed black. The shared
traversal publishes that value before furniture, so model primitives do not
inherit an unrelated earlier scene color.

## Basis and lookup

Native furniture is a process object at `00546B48`, with position at +04,
basis at +10 (`00546B58`), H1 slot at +38 and sparse-record pointer at +3C.
`00437A70` selects one of sixteen 40-byte bases at `00546B88` using the
record's unsigned state byte shifted right four.

Service `00533228`, callback table `00522050`, reaches `00435690` on its
zero-to-one acquisition through `00435810 → 0048EE20`. That basis phase
copies the read-only CRT identity axes from `0055F290/280/270` into the
current object without resetting its counter. It copies all ten DWORDs to
each table entry, then invokes `00491530` at angle `index * 4096`. First
acquisition starts from BSS counter zero; a basis rebuild must retain the
current counter. This is separate from ordinary world replacement. The
enclosing acquisition also initializes the queue header and registry.

The native session keeps that service acquired across screen and world
replacement. `004F4DF0` calls `00438E10` at `004F4E3D` before the message
loop and `00438E20` at `004F4F24` after it. Those thunks retain session
service `005333F0` through table `005220F0`. Its dependencies
`00522120/24` retain world wrappers `00488000/10`, service `0053C680`,
table `00528720`; dependencies `00528740/44` in turn retain furniture
wrappers `00435810/20`. The bootstrap reference therefore prevents furniture
from dropping to zero during the loop. `PortSession` acquires the basis
state once and retains it across world teardown, matching that ownership.

`004359A0` skips only lookup byte zero. Every nonzero byte indexes the first
WORD at H11/0 table base plus `lookup * 4`; the second WORD is not a model
slot. Lookup values `FD..FF` follow this same draw contract. The previous
port omission of those values was unsupported by this consumer; authored
H11/6 entries 253–255 bind model slots 69–71. A terrain payload's special
meaning at another consumer does not change this H1 lookup.

H1 `0049FEA0` treats a genuine lazy-loader null H8 as a normal no-draw return.
For a nonnull H8, `0049FED3` tests the command-count WORD at +04 alone;
zero skips model execution regardless of adjacent header fields. A pending
fade's earlier extent read still requires a nonnull H8 and cannot use that
later normal no-draw result as a successful substitute.

## Item decision matrix and effects

The orchestrator binds H1 and publishes the selected basis before caching
the record's low two state bits. It then handles bit 8:

| Pending state | Native effect |
| --- | --- |
| Bit 8 absent | Continue to the ordinary pre-H1 effects. |
| Bit 8 present, no matching queue key | Clear bit 8 in the actual sparse record, then continue. |
| Matching entry, remaining time zero | Preserve bit 8 and use ordinary fade. |
| Matching nonzero time, depth at or beyond threshold | Preserve bit 8 and use ordinary fade. |
| Matching nonzero time, depth below threshold | Resolve H8 extent and H7 material 32, replace context fade/color, publish fade, then continue. |

The key combines low-WORD X and Z aligned by the current spacing mask.
Depth first wraps each low-WORD position-minus-eye subtraction and interprets
it as signed. It computes `(Z product + Y product) + X product` against the
context's Z basis in retained precision, then truncates through `__ftol`.
The threshold is signed truncation of wrapping `(3 * old_far - old_near) / 2`.

For an admitted fade override, let `D` be the model's full first DWORD
multiplied by two with DWORD wrap. H7/32 supplies the converted far-color
DWORD. Native stores use:

```text
decay = i16(((((5_000_000 - remaining) >> 2) * D) * 2) >> 15)
far   = depth + D
near  = depth + D - decay
```

Subtraction/products wrap as signed DWORDs, shifts are arithmetic and final
additions wrap. The constant is the integer 5,000,000 microseconds, not an
address-derived policy. Both resource reads precede context writes.

Cached low bits zero skip the direct trail and all its RNG draws. Otherwise
the routine captures position, draws velocity Y as `RNG_low_WORD & 63`, then
computes signed `N = wrapping(0xC000 - load_factor) / 2048`. When `N >= 2`,
one further low-WORD RNG draw admits the birth only for remainder zero;
`N < 2` admits without that draw. The actual trail producer receives type 6,
captured XYZ, velocity `[0,Y,0]` and flags zero before H1 execution.

After a normal H1 return, an override restores the saved context color/fade
and republishes fade. A failed required resource, birth or model operation
retains its reached state, RNG, particle and queued-draw prefix. A later
failure does not synthesize the native normal-return restoration. The shared
bridge reports an unsupported item and continues subsequent furniture items
and terrain phases.

## Furniture model callbacks

Packed callback `00435870` reads clock low WORD for selector 0, rereads live
sparse state and returns `(state & 3) << 14` for selector 1, and returns zero
for every other selector. Record identity remains live through model execution.

Custom callback `004358A0` is separate from frontend/name-entry unsigned-row
callback `0043A2D0`. Ordinary World custom `00478950` remains unowned, with
its existing labeled nibble-0/2 skip policy; it is not a zero-output callback.
Native opcode `2D` sign-extends the complete selector WORD at `004EC685`. A frontend
callback deliberately masks its selector to one byte; furniture compares
the complete signed value with 7. Thus `FF07` does not select furniture 7.

Every selector other than 7 writes `[0,0,0]` without requiring particle/RNG
services. Selector 7 first resolves the unsigned source-row index and writes
the row's signed low-WORD XYZ to the temporary callback output. Then it
computes the separate signed divisor
`N = wrapping(0xC000 - load_factor) / 1024` and applies the same remainder-zero
admission test. An admitted row maps kind 1 to particle 6, 2 to 8, 3 to 11,
and all other kinds to 5. It draws Y velocity as
`128 + (RNG_low_WORD >> 9)`, supplies zero X/Z velocity, and calls the real
trail producer with flags `0x800` on the same RNG stream.

Temporary XYZ publication precedes gate/birth failures. Opcode mode-1
destination storage or mode-2 projection occurs only after the callback
returns successfully. Rejected gates retain the already written temporary
XYZ. `004EC670` reuses that same stack XYZ temporary across both mode passes;
a later callback failure before writing retains the earlier pass's output.
Direct pre-H1 `/2048` and custom `/1024` emissions deliberately preserve
their different draw order.

Particle allocation retains the actual phase/network inputs. Offline phase-3
briefing calls execute the real scratch constructor and RNG writes without
publishing pool particles. Phase-4 world births enter the pending list after
that frame's transfer; network-active briefing follows its separate pool
admission rule. Audio availability does not select a particle phase.

## Evidence and remaining bounds

The original-code checker
check_furniture_draw.py produces
[51 item, 5 basis and 1,280 packed-query receipts](../../crates/inf-game/tests/fixtures/furniture_draw_native.json).
It authenticates the executable and executes unchanged item, RNG, rotation,
fade publication and CRT `__ftol` instructions with the original `027F`
floating-point control word. Resident H11/H8/H7 inputs, terminal H1/trail
calls and basis registry allocation are explicit supplied hosts. Its reached
failure cases establish prefix preservation, including failed restoration;
they do not prove those terminal services' complete behavior. The
[Rust comparisons](../../crates/inf-game/src/terrain_furniture/draw/tests.rs)
check those receipts against the shared kernel.

The original-code checker
check_model_furniture_callback.py
produces [105 custom callback receipts](../../crates/inf-game/tests/fixtures/model_furniture_callback_native.json).
It authenticates the executable and executes unchanged callback/RNG/row-query
instructions with explicit source rows, load factor and terminal birth hosts.
The [Rust comparisons](../../crates/inf-game/src/model_vertices/furniture_tests.rs)
also exercise the real particle producer and callback failure prefixes.

Reproduce the receipts from the repository root with Python 3 and Unicorn 2:

```powershell
python check_furniture_draw.py --output .tmp/furniture-draw-native.json
python check_model_furniture_callback.py --output .tmp/model-furniture-callback-native.json
```

Both tools default to the supported local `infestation/Infest.exe` and reject
a different executable hash. Compare each result with its versioned fixture;
the executable and transient output remain local.

Kernel fixtures establish supplied-input arithmetic and ordering. Authored
world/briefing pixel checks establish reached production draws, not a matched
retail primitive census. Furniture now reaches the shared Near/Far
[`0x1A` producer](PRIMITIVE_RUNTIME.md#opcode-1a-sprite-ground-quads).
[E6's Near/Far clipped fallbacks](PRIMITIVE_RUNTIME.md#opcode-e6-shared-shade-quads-and-clipped-uv)
now reach their shaded UV consumers through the same resident model bridge.
The generalized [DE triangle and D6 quad clipping paths](PRIMITIVE_RUNTIME.md#de-and-d6-source-polygons)
retain their distinct shared versus per-corner color policy. Direct effects and
the reached Gouraud consumers retain primary pixels before a required secondary
pass fails. These owners apply to every resident furniture model and briefing
pane without mission-specific exceptions.
Mapped but unknown shade bytes and missing reached services still retain earlier
item draws and leave later records enabled. Complete primitive coverage, terrain-emitter drawing,
remaining actor callbacks and full cinematic/briefing comparison remain open.
