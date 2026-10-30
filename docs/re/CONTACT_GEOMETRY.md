# Shared auxiliary contact geometry

This authority owns the native 20-byte contact fields and the primitive
`004F1420` sphere-pair, `004F1F90` sphere-box, `004F2B30` group-direction and
`0044EC90` terrain-sphere arithmetic. The
[auxiliary bytecode authority](MODEL_AUXILIARY.md) owns stream layout,
alignment and control flow. The Rust kernels are in
[collision_geometry.rs](../../crates/inf-game/src/object_runtime/actor/collision_geometry.rs),
with [contact storage](../../crates/inf-game/src/object_runtime/actor/collision_geometry/contact.rs)
and [primitive execution](../../crates/inf-game/src/object_runtime/actor/collision_geometry/primitives.rs).
They consume supplied integer world-coordinate inputs independently of
scene IDs, terrain leaves or renderer presentation.

## Contact fields and caller custody

| Native offset | Recovered field | Storage policy |
| --- | --- | --- |
| `+00` | Result flag DWORD | Exact bits, including a prior nonzero flag |
| `+04/+08/+0C` | Normal components, IEEE binary32 | Exact DWORD bits |
| `+10` | Displacement/score DWORD | Exact bits; a reached signed consumer interprets them as `i32` |

`CollisionContact` models these five native DWORDs as independently known or
unwritten values. Its Rust representation is a custody model rather than an
ABI-compatible 20-byte memory view. `unwritten()` invents no flag, normal or
displacement. `from_native_words()` accepts actual preceding writes or
explicitly supplied allocation state. A nonzero flag does not establish that
the normal or displacement has a writer.

The outer `004F2C50` clears only the flag at `004F2C5E`, before the header `+0E`
admission test. Its zero-gate return therefore does not own the other four
fields. The kernels below do not duplicate that caller-specific entry write.
Neither primitive returns a Boolean; existing contact fields remain the
caller's result. A sphere rejection can preserve a previous flag of one.

## Primitive decision matrix

| Routine/path | Reads and gates | Reached writes in order |
| --- | --- | --- |
| `004F1420`, any cube gate rejects | Full DWORD centers, wrapped radius sum and unsigned cube comparison | None |
| `004F1420`, squared gate rejects | Wrapped DWORD square sum, unsigned comparison with wrapped radius square | None |
| `004F1420`, admitted nonzero distance | Signed LOWWORD deltas divided by retained square root | Flag one; X/Y/Z normal; wrapped radius minus rounded distance |
| `004F1420`, admitted zero distance | Same integer gates; wrapped squared sum is zero | Flag one; normal `(1,0,0)`; radius |
| `004F1F90`, any strict interval gate rejects | Full DWORD centers, radius and box extents; signed wrapped comparisons | Flag zero only |
| `004F1F90`, admitted | Wrapped absolute face distances, signed nearest-face comparisons | Flag zero, flag one, retained cardinal normal, displacement |
| `004F2B30`, nonzero LOWWORD vector | Signed LOWWORD request coordinates; no collision gate | Flag one; displacement one; X/Y/Z normal |
| `004F2B30`, zero LOWWORD vector | Same input truncation | Flag one; displacement one; normal `(1,0,0)` |

### Sphere pair `004F1420`

The native ABI takes contact in ECX, request in EDX, and candidate XYZ pointer
and scalar DWORD as its two stack arguments. Request centers are full DWORDs
at `+10/+14/+18`; its scalar is the full DWORD at `+20`. `ContactSphere`
names these world coordinates and scalar inputs without substituting the
model header WORD or a captured radius.

Let `delta = request.center - candidate.center` and
`R = request.radius + candidate.radius`, with every operation wrapping at
32 bits. `004F144B..69` admits each component only when
`unsigned(R + delta) <= unsigned(2 * R)`. It uses `JA` for rejection; neither
signed comparisons nor an absolute-value check are equivalent. The low DWORD
of `deltaZ² + deltaY² + deltaX²` is compared unsigned and inclusively with
the low DWORD of `R²` at `004F146F..89`. Tangency is admitted. No clamp,
overflow repair, radius sign restriction or field clear occurs on rejection.

After writing flag one at `004F1495`, the routine zero-extends that squared
DWORD into a QWORD before `FILD` and `FSQRT`. The square root remains in x87;
it has no binary32 denominator spill. Each normal numerator separately
sign-extends the LOWWORD of the corresponding **full DWORD delta** at
`004F14AE..DB`. Large deltas can therefore pass full-width gates and produce
a normal whose signed numerators differ from those deltas. A wrapped zero
square sum selects `(1,0,0)` even if the full-width vector was nonzero.

`004F14ED` converts the retained distance with `FISTP QWORD`; only the low
DWORD is loaded and subtracted from `R` at `004F14F1..F7`. The port reuses
the existing exact [FISTP helper](../../crates/inf-game/src/model_context.rs)
for this conversion. Under supported CRT control word `027F`, x87 precision
is 53 bits and rounding is nearest-even. The unsigned squared DWORD bounds
the distance to a finite value below 65536. The final subtraction still
wraps as a DWORD.

### Group direction `004F2B30`

This callback receives contact in ECX and request as its first stack argument.
It ignores vertex and scalar operands. The writes at `004F2B37/3D` set flag
and displacement to one **before** its coordinate reads. Request `+10/+14/+18`
is read as signed LOWWORDs at `004F2B44..54`.

X remains in x87; Y and Z are spilled as binary32. Integer LOWWORD inputs are
exactly representable in binary32, so these input spills retain the values.
The square-root result of `(Z² + Y²) + X²` is spilled as binary32 at
`004F2B86`, however. `004F2B97..B1` divides the three numerators by that spilled
denominator before storing the binary32 normal. Sharing the sphere divisor
would change reachable normal bits. The zero vector produces `(1,0,0)`.

### Sphere-box `004F1F90`

[The sphere-box owner](../../crates/inf-game/src/object_runtime/actor/collision_geometry/sphere_box.rs)
receives request center/radius, candidate center, three authored half extents
and six cardinal normal vectors. It clears the contact flag before all input
gates. Every axis requires `query+radius > center-extent` and
`query-radius < center+extent`, with DWORD wrapping and signed comparisons;
tangency rejects. This is the actual native box policy, including its corner
behavior, rather than a substituted closest-point sphere test.

For each axis it compares the wrapped absolute distances
`abs(query-extent-center-radius)` and `abs(extent-center+query+radius)`.
The second strictly smaller value chooses the negative normal; equal faces
keep positive. Wrapped `INT_MIN` remains negative. Axis selection gives X
priority only when smaller than both Z and Y, then Z only when smaller than
Y; equal minima therefore select Y before Z before X. The chosen three normal
DWORDs are copied unchanged and the chosen distance becomes displacement.

The native vectors are constructed once by CRT routines `0048F5D0/0048F630/
0048F690/0048F6F0/0048F750/0048F7B0` at `0055F290/280/270/2C0/2D0/2B0`.
Complete executable text references contain no later direct stores. Every
other pointer reference is a read-only source operand of `00490370`,
`00491D10`, `0043B2B0`, `0040D1B0` or `0047D440` (including the `00426730`
wrapper); their writes target a distinct receiver or stack storage. The
production `ContactAxisNormals::NATIVE_STARTUP` therefore represents these
fixed post-CRT unit vectors, not a current model basis or captured scene pose.
Supplied normal bits remain explicit inputs to the primitive for independent
testing.

### Point-box `004F1D20`

The point table `00544BF0`'s `+08`, which an `8F` box reaches under a
zero-radius query (a radius-zero shot's actor or furniture test). The request's
point is `+10/+14/+18`; the arguments are the box's vertex and a pointer to the
`8F`'s half extents. It clears the flag, then rejects unless every axis's
`|centre - point|` (the difference, or its reverse when negative) is below the
half extent. On a hit it sets the flag and, per axis, takes the low face's
distance `half + (point - centre)` (when negative, `centre - half - point`) and
the high face's `(point - centre) - half` (when negative, `half - point +
centre`), keeping the high face only when strictly nearer. The axis is chosen as
the sphere-box's is (equal X and Z: Z; equal with Y: Y), its normal is the
positive axis for the high face and the negative one for the low, and the
displacement is that distance. Unlike the sphere-box, a tie between faces keeps
the low face. check_point_box.py runs it over 209
cases (gates on both sides, every face, the ties, wrapped sums and differences)
and [the port](../../crates/inf-game/src/object_runtime/actor/collision_geometry/sphere_box.rs)
matches every one.

### Box-sphere `004F15F0`

The box table `00544C50`'s `+00`, reached when the composite's box wrapper
runs the counterpart's program and that program's `8E` sphere meets the
carried box. ECX is the contact, EDX the request (the box's center
`+10/+14/+18` and half extents `+28/+2C/+30`), and the stack the sphere's
vertex and radius. It is [the sphere-box](#sphere-box-004f1f90) seen from
the box: the same strict gates (tangency rejects), and per axis the wrapped
absolute distances `abs(center-query-extent-radius)` for the positive
normal and `abs(center-query+extent+radius)` for the negative one, a
strictly smaller second choosing the negative; the same axis order (Y
before Z before X on ties) and the same six fixed vectors. A rejection
writes only the cleared flag.
check_box_sphere.py runs it over 207 cases
(gates on each side of each axis, each face and sign, the ties, wrapped sums
and `INT_MIN`, 160 random) and
[the port](../../crates/inf-game/src/object_runtime/actor/collision_geometry/box_sphere.rs)
matches all of them.

### Box-box `004F23B0`

The box table's `+08`: the carried box against a counterpart `8F` box,
whose vertex and a pointer to its half extents are the stack arguments. It
is [the sphere-box](#sphere-box-004f1f90) with the request's half extent of
each axis in place of the radius: the strict gates
`center + extent > other - other_extent` and
`center - extent < other + other_extent`, the wrapped absolute face
distances, a strictly smaller negative face choosing the negative normal,
the same axis ties and the six fixed vectors. A rejection writes only the
cleared flag. check_box_box.py runs it over 208
cases (60 contacts) and
[the port](../../crates/inf-game/src/object_runtime/actor/collision_geometry/sphere_box.rs)
matches all of them, through the auxiliary interpreter's `8F` under the box
table.

### Terrain sphere `0044EC90`

Terrain table `00537090` slot `+00` is `0044EC70`, which forwards ECX/EDX
and both stack arguments to `0044EC90`. Contact is in ECX and request in EDX;
the stack holds the callback vertex pointer and the 8E scalar radius. The
[terrain sphere owner](../../crates/inf-game/src/object_runtime/actor/collision_geometry/terrain_sphere.rs)
takes the prepared H12 cells, the separately read `005372F0` spacing,
`005372F4` mask and `005372F8` height shift, the summed center and radius.
The center is request `+10/+14/+18` plus the vertex, wrapped per DWORD. The
`0044ECB2` lazy H12 load belongs to the caller, which supplies resident cells.

The walk covers `(c - r) & ~mask` up to, but excluding,
`(c + spacing + r) & ~mask` on X and Z, stepping by the spacing and comparing
with `!=`. Grid indices are each bound's low WORD shifted right by 7 and
masked with the arithmetic `~mask >> 7`. The next index adds 1 for spacing
`0x80`, otherwise 2, modulo 512. Heights are the cell's signed low byte
shifted left, truncated to a signed WORD. A cell is skipped when its highest
corner is at most `Y - r`. The high byte's bit 3 (cell bit 11) selects the
diagonal:

| Layout | First triangle | Second triangle | Shared diagonal |
| --- | --- | --- | --- |
| Bit clear | `(0,0) (0,1) (1,1)`, inside when `u <= v` | `(0,0) (1,0) (1,1)`, inside when `u > v` | `(0,0)-(1,1)` |
| Bit set | `(0,0) (0,1) (1,0)`, inside when `u <= s - v` | `(1,0) (0,1) (1,1)`, inside when `u > s - v` | `(0,1)-(1,0)` |

Each triangle takes its plane normal from `0044FBF0`. That routine's cross
product spills X and Y through binary32, while Z stays in x87 for both the
length and its own division. The signed plane distance is a truncated dot
product. The default layout spills the relative Z, Y and X inputs through
binary32 and sums `(Z*nz + Y*ny) + X*nx`. The alternate layout spills only Z,
sums `(Y*ny + X*nx) + Z*nz`, and measures its second triangle from corner
`(1,0)`. A distance at or above the radius misses. Otherwise the signed WORD
distance projects the center. The unsigned `u`/`v` tests use the cell origin;
any value above the spacing is outside.

An outside projection reaches `0044F750`, which keeps the nearer of the two
non-diagonal edges (the second on a tie). The shared diagonal wins ties
against that choice, but is evaluated only while argument 11's low byte is
set. That byte is 1 for the first triangle. For the second triangle it is
cleared once the first triangle reached `0044F750`. A skipped diagonal is
`0x7FFFFFFF`; it could tie only a wrapped square of `2^31-1`, which is not a
sum of three squares. `0044F9A0` measures each segment from the point's WORD
offset. Its length is retained in x87, but the binary32 spill is the unit
divisor and the truncated end bound. A negative projection keeps the point,
one past the end subtracts the segment, and otherwise each truncated
`along*unit` component is subtracted. Its squared WORD sum wraps as an i32.

A squared edge distance at or above `r²` misses. Zero hits with the plane
normal and distance zero. Otherwise the QWORD-zero-extended square's root
divides the WORD vector into the binary32 normal, and the truncated root is
the distance. Of two hit triangles, the second wins ties. The first contact
of the call always writes flag one, the normal and `r - d`, whatever the
caller left in `+10`. A later cell writes only when its displacement is
strictly greater than the stored `+10`. A call without contact writes
nothing. Walks that never terminate natively (a span not divisible by the
spacing) or exceed 1024 cells per axis return a typed port error instead of
spinning.

The data-bearing `ContactGeometryRequest` explicitly chooses these
policies. It does not select behavior through a numeric sentinel, an empty
vertex slice or coincidental radius value. Inputs are supplied values, so the
API does not claim native invalid-pointer faults or partially readable input
storage. It preserves the recovered reached-write order inside each kernel.

## Native receipts and validation bounds

check_contact_geometry.py loads the fixed
retail EXE SHA256
`bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956`,
executes the two unchanged entry points with FPCW `027F`, and hooks only contact
memory writes. It substitutes no instruction, callback or arithmetic result.
The instruction limit is 1000 per primitive call. The generated
[native fixture](../../crates/inf-game/tests/fixtures/contact_geometry_native.json)
and [Rust comparison tests](../../crates/inf-game/src/object_runtime/actor/collision_geometry/tests.rs)
cover 540 cases / 548 calls: 162 sphere hits, 211 sphere rejections and 175
group calls.

The checker asserts sphere write offsets `[0,4,8,12,16]` on admission and no
output writes on rejection. Group offsets are `[0,16,4,8,12]`. It supplies
distinct poison bits for unknown caller fields solely to verify untouched
native bytes. JSON `null` retains their lack of a reached writer; those poison
bits never initialize port behavior. Sequences cover previous successful
contacts followed by rejection, unknown contact followed by admission, and
group replacement. Paired vectors discriminate the retained sphere divisor
from the group binary32 spill. Other cases include inclusive tangency, signed
LOWWORD wrap, DWORD center/radius wrap, unsigned squared values above `2³¹`,
wrapped zero squares, nearest-even conversion and deterministic full-width
inputs.

Reproduction with the private checker and a prepared Unicorn runtime:

```powershell
$env:PYTHONPATH='.tmp\bee-emulator'
python `
  check_contact_geometry.py `
  --output .tmp/contact-geometry-native-review.json
```

These are primitive differential receipts. They establish neither outer
auxiliary dispatch nor live intro collision acceptance.

The [sphere-box receipts](../../crates/inf-game/tests/fixtures/combat_auxiliary_sphere_box_native.json)
add 25 complete unchanged `004F1F90` calls, covering all faces, strict tangents,
axis/face ties, zero and negative inputs, signed wrapping and retained fields.
They also execute all six CRT vector constructors and compare their outputs
to the production startup owner. Distinct supplied vector DWORDs prove that
the callback copies its chosen vector. Four additional
[8F dispatch receipts](../../crates/inf-game/tests/fixtures/combat_auxiliary_box_dispatch_native.json)
authenticate DWORD argument alignment, signed selected vertex, flag/score
reset, the full box callback and the next instruction. Rust tests execute the
decoded operation using genuinely retained scratch bytes; the supplied
scratch domain does not establish authored model or scene acceptance.

check_terrain_sphere_contact.py
runs the unchanged `0044EC90` with its helpers and `__ftol`. Its inputs are a
resident synthetic H12 slot, the three grid globals and poisoned contacts;
stray writes outside the stack and contact abort the run. The
[terrain sphere receipt](../../crates/inf-game/tests/fixtures/terrain_sphere_contact_native.json)
holds 971 cases (724 contacts) over 32 patches of 24x24 cells:

- both spacing/mask pairs and both height shifts, plus a `0x80`/`0xFF` mix;
- flat, sloped, rough, cliff and extreme heights;
- default, alternate and mixed diagonals, with noise in the other high-byte
  bits;
- patches that wrap the 512-cell grid;
- centers beyond 16 bits and negative;
- exact grid-line and diagonal placements.

The checker requires every recorded branch: each triangle's inside and edge
path, each `0044F750` and `0044F9A0` outcome, both skipped-diagonal arms, zero
squared edges, both triangle choices, kept-earlier contacts and floor skips.
A deterministic search adds eleven contacts whose published normal came from
an edge tie the opposite rule would have resolved differently. Without them,
both tie rules would survive mutation. The
[Rust comparison](../../crates/inf-game/src/object_runtime/actor/collision_geometry/tests/terrain_sphere.rs)
matches all five DWORDs of every case.

## Actual callers and remaining owners

`0047B610` submits actor-pair geometry through `004F2BE0` at `0047B7E3`.
`0047C1D0` submits furniture geometry through the same entry at `0047C658`;
a nonzero result reaches actor vtable `+A8` at `0047C67F`, followed by its
low-AL acceptance test. The shared auxiliary request computes relative origin
from context B `+08/+0C/+10` minus context A and installs outer callback table
`00544C68` before calling `004F2C50`.

| Outer table `00544C68` slot | Callback | Reached inner policy |
| --- | --- | --- |
| `+00` | Sphere wrapper `004F18B0` | Recursive auxiliary call with table `00544C08` |
| `+04` | Cylinder wrapper `004F1CA0` | Recursive auxiliary call with table `00544C38` |
| `+08` | Box wrapper `004F2690` | Recursive auxiliary call with table `00544C50`, carrying the 8F half extents at request `+28/+2C/+30` in place of the radius (`+20` stays unwritten); ported, its `+00` being [the box-sphere](#box-sphere-004f15f0) |
| `+0C/+10` | Group direction `004F2B30` | Unconditional supplied-request normalization |

Inner sphere table `00544C08 +00` points to `004F1420` and `+08` to the owned
`004F1F90`; its other entries are `004F1940`, and `004F2720` and `004F2930`,
the plane test of the [group opcodes](MODEL_AUXILIARY.md#group-tests-898d). Cylinder and box tables also
use group direction for `+0C/+10`, while their first three policies differ.
The cylinder table `00544C38` and the box table `00544C50` hold the other
shape's tests at `+00` (sphere), `+04` (cylinder) and `+08` (box). In each,
the request (EDX) is the wrapper's own shape: its centre `+10/+14/+18`, a
cylinder's radius `+20` and height `+24`, a box's half extents
`+28/+2C/+30`. The argument is the other shape's centre, then its radius
and height, or a pointer to its half extents. Each first clears the flag.

| Table slot | Routine | Contact when | Normal and displacement |
| --- | --- | --- | --- |
| cylinder `+00` | `004F1510`, cylinder against sphere | the sphere's centre height lies between the cylinder's base `+14` and top `+14 + +24`, each widened by the cylinder's radius `+20`, and the squared horizontal distance is below the square of the radii's sum | horizontal: X and Z over the f32 distance (Y 0; `(1,0,0)` at zero), displacement the radii's sum less the rounded distance |
| cylinder `+04` | `004F1A20`, cylinder against cylinder | each axis's difference within the sum of the extents: the radii across X and Z, the heights `+24` and the argument's along Y (strict on both sides) | the differences' low WORDs as f32 over their length (`(1,0,0)` at zero), displacement 1 |
| cylinder `+08` | `004F2260`, cylinder against box | as `004F1A20` with the box's half extents in place of the other cylinder's radius and height | as `004F1A20` |
| box `+04` | `004F1B60`, box against cylinder | as `004F1A20` with the request's half extents `+28/+2C/+30` in place of its radius and height | as `004F1A20` |
| box `+08` | `004F23B0`, box against box | each axis's centre difference within the half extents' sum | the axis normal and distance chosen as the [sphere-box](#primitive-decision-matrix)'s are, with the half extents' sum in place of the radius |

The box table's `+00` is the [box-sphere](#box-sphere-004f15f0) and its
`+08` the [box-box](#box-box-004f23b0); both tables use group direction
`004F2B30` at `+0C/+10`. The port has no cylinder table: the cylinder
opcode `90` occurs in no shipped auxiliary program
([census](MODEL_AUXILIARY.md#corpus-and-parent-source-boundary)), so neither
the cylinder wrapper nor a box's `+04` is reached; a contact that did would
stop there ([`CallbackTable`](../../crates/inf-game/src/model_auxiliary.rs)).

Other direct `004F2C40` callers at `00436003`, `0043A943`, `0044EC62`,
`0047D056` and `0047D2D2` retain their own callback policies. This kernel does
not generalize one table into all callers. `0044EC62` is `0044EC50`, which
installs terrain table `00537090` for the world contact pass `004791F0`:

| Terrain table slot | Callback | Policy |
| --- | --- | --- |
| `+00` | `0044EC70` → `0044EC90` | Terrain sphere, above |
| `+04`, `+1C` | `0044FBC0` (`RET 0C`) | Flag one, `+10` one |
| `+08` | `0044FBD0` (`RET 8`) | Flag one, `+10` one |
| `+0C`, `+10` | `0044FBE0` (`RET 4`) | Flag one, `+10` one |
| `+14` | NULL | None |
| `+18` | `0044FB50` | Water plane; not an auxiliary opcode slot |

The three constant callbacks leave the normal words untouched. The
interpreter owns this table for 8E (the kernel, centered on request
`+10/+14/+18` plus the vertex) and for 8F/90's constant slots; group
callbacks 89/8A take `0044FBE0` ([group tests](MODEL_AUXILIARY.md#group-tests-898d)).
The [world contact pass](WORLD_CONTACT.md)
is its caller.

The [connected auxiliary owner](MODEL_PROCESS_GRAPHICS.md#owned-generic-grammar-and-geometry)
owns reached recursive interpretation, model/resource acquisitions, header
admission, vertex-slot allocation and best-result publication. Grouping and
other primitive callbacks remain required boundaries. Renderer
`ModelVertices::initialize()` is not valid auxiliary setup: `004F2C50` does
not perform the renderer's header-range initialization or seed all vertex and
register fields. Its auxiliary `07` opcode reaches `004EBF00` with command
`0700`; world XYZ writes precede the projection callback through context
`+3C` table `+1C` at `004EBFA3`. Static table `00544740 +1C/+20` contains null
entries, but the renderer publishes them through its context table alias and
restores them after child draws; the [projection publication owner](MODEL_AUXILIARY.md#projection-table-publication-and-alias-custody)
records the exact policy and remaining persistent table custody. Static null
bytes do not authorize a no-op callback, stripped command bits or invented
projection. Primitive receipts alone do not claim completed actor/furniture
collision; connected query custody belongs to that auxiliary owner and
[PROJECTILE_COLLISION_RUNTIME.md](PROJECTILE_COLLISION_RUNTIME.md).
