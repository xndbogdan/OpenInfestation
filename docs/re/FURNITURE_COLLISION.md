# Infestation furniture collision entry and scan

This owner covers the shared `00429EE0` cache-miss service and the reached
entry, cell scan and first-pair preparation of `0047C1D0`. It consumes the
actual actor, installed `+234` cache, prepared H12 and retained process state.
It is independent of level IDs. The current runtime routes the real
Selector12 and Selector36 avoidance callers through the same service loan.

The [probe/cache owner](FAMILY4236D0_RUNTIME.md#captured-actor-cache-allocation)
owns window updates and captured-allocation validation. The
[retained process owner](TERRAIN_RUNTIME.md#retained-furniture-query-process-state)
owns fallback `005483C4`, lazy mask `0055F0DC` and five query bases. The
[furniture basis owner](FURNITURE_RUNTIME.md#basis-and-lookup) supplies the
actual sixteen state-selected bases. The later
[auxiliary grammar](MODEL_AUXILIARY.md) and
[contact response](CONTACT_GEOMETRY.md) remain separate owners.

## Required miss and normal return

`00429EE0` reads current actor `+160`, then acquires the actual H12 slope
service. A blocked slope returns before query initialization, guard or XYZ
stores. Otherwise it sets `WORD22E.8000`, requires the old position, installs
the temporary probe XYZ and calls the collision service. Only normal return
restores XYZ. If the guard survives, restoration precedes clearing that bit
and the miss returns false; a response that cleared it returns true. A typed
service failure retains the temporary XYZ, guard and earlier cache writes.
The captured cache stores a completed result only after normal miss return.

Both tails reuse the existing `FamilyTerrainProbeMissHost`, with an explicit
miss service in intrinsic fixtures. There is no default successful collision
or concealed replacement of the installed cache. A normal miss permits the
caller's owned avoidance and motion phases; a typed failure retains its reached
prefix. Completing a query does not establish a complete actor callback or intro.

## Entry and registration

`0047C1D0` captures its H12 pointer before the mask read and signed process
radius read. The bit test follows both reads. With mask bit01 clear it
publishes the bit, constructs five complete `0040D6A0` bases, then attempts
`00511D89(0047C6D0)`. With bit01 set it preserves the existing bases and
record history. Query entry is reached after the slope/temporary-XYZ phase;
it is never moved earlier to process startup or every cache probe.

`0047C6D0` is a literal `RET`. The native caller ignores the registration
wrapper's EAX for both CRT allocation success and failure. The explicit
`LiteralRetExitRegistrationApproximation` records the reached attempt and
continues, preserving that caller decision. It does not model CRT vector
allocation/reallocation, heap cadence or process-exit fidelity. Those remain
replaceable CRT service work; this approximation is not retail acceptance
and supplies no actor carry values.

Actor H8 is a required root: resolve fresh actor model WORD42, then read
unsigned header WORD0E before either true zero exit. Header zero is tested
before captured process radius zero. Missing actor H8 is a reached error,
including for a zero process radius; it is never radius zero.

## Scan units and order

Captured process radius is signed WORD; header radius is unsigned WORD.
Combined radius uses DWORD wrap. Current spacing is source-admitted128/256.
For actor low-WORD coordinate `c`, spacing `s`, and mask `m = s - 1`:

```text
lower = LOWWORD((c - combined_radius + s - 1) & ~m)
upper = LOWWORD((c + combined_radius) & ~m)
count = unsigned_LOWWORD(upper - lower) / current_positive_spacing + 1
```

Native MOVAX preserves high register residue, but every endpoint/count cell
consumer reduces it modulo65536. The admitted positive divisor makes its
signed IDIV equivalent to this nonnegative division. The portable domain
uses canonical sorted512x512 H12 buckets; it does not extrapolate invalid
spacing or malformed record layouts.

X is outer, Z inner. Initial X is read before spacing/mask; Z is read later.
Each cell reads a fresh mask, tests raw H12 flag04, then selects either its
required sparse row or the same retained fallback. Lookup BYTE3 zero skips
H11, counterpart H8, saved XYZ, basis and auxiliary execution. Nonzero lookup
uses current H11's first WORD. Negative, strictly out-of-capacity, canonical
loader-null or zero-header counterpart H8 genuinely skips. The inclusive
capacity boundary retains the existing hard typed H8 lookup error.

Every completed inner cell rereads spacing into the native retained step and
adds it to Z. The outer X increment reuses that last inner step; it performs
no extra spacing read. An exhausted skip-only window returns normally from
the collision service. That result follows executed skips, not absence of
required storage or an assumed clear-query callback.

## First active candidate and retained prefix

The first ACTIVE candidate clears local contact flag and prepares the local
actor request. It saves XYZ before actual virtual48 reads all ten actor basis
words. Canonical final10/12/36/35 tables bind that virtual to `004026B0`; shared
field presence alone cannot admit a different class callback.

It publishes the actual shared `00547590` basis to B at `0055EF44`, then the
exact B+44 alias `0055EF74 -> 00546B48`. It publishes current record pointer
`00546B84` before rereading state BYTE2 and copying the live high-nibble basis.
Sparse identity is qualified by original H12 registration, allocation and
record index. A weak identity does not keep retired H12 alive or resolve an
old record against a new terrain. Initial record pointer is read from the
actual file-backed DWORD; it is not synthesized null.

Aligned cell X/Z and signed H12 height BYTE shifted by current `005372F8`
retain low-WORD truncation. A's origin is signed low-WORD saved actor XYZ
minus cell XYZ in world integer units. Request+08 is actor H8 and +0C is the
actual counterpart. Model-local vertices, screen pixels and fixed16.16 values
are not interchangeable with this origin.

The [retained ACTIVE owner](MODEL_PROCESS_GRAPHICS.md) consumes those exact
process/request publications and executes its explicit generic auxiliary
domain. It captures current B-A before interpreter zero stores, uses the
actual default-table history and preserves nested allocation bytes. It
supplies no query camera, Near/Far default or clear-contact substitute.

Contact flag0 advances the scan. Nonzero contact reaches actual actor+A8:
final10/12/36 use the existing42A760 guard, while34/35 reach47BFD0 directly.
The [world contact pass](WORLD_CONTACT.md#furniture-response-0047bfd0) owns
47BFD0 for its flag-4 phase, through both arms. Class-tail callers still stop
at it as a typed response boundary: their hosts lend no 0047C6E0 owners.
First ACTIVE preparation occurs once per query; later candidates
retain fresh per-cell reads. An explicit outer registered-mark approximation
may retire a stopped auxiliary feature while returning its original error.
Actor XYZ restoration and cache completion remain normal-return-only.

## Verification scope

The [scan and miss cases](../../crates/inf-game/src/object_runtime/furniture_collision/tests.rs)
cover lazy resource ordering, true zero exits, skip-only loops, fresh state and
spacing reads, basis/error prefixes, repeated initialization, WORD arithmetic,
and the actual cache/miss normal/error protocol. Complete original bodies and
final virtual slots are byte-guarded. The connected auxiliary cases execute
unchanged original 223/51 and 492/392 programs through the shared
[retained owner](MODEL_PROCESS_GRAPHICS.md#verification-bounds). Supplied view
and table inputs in intrinsic cases are explicit fixtures, never runtime defaults.

The [two-stage linked observer](ACTOR_BEHAVIOR_RUNTIME.md#test-only-first-miss-provenance)
passed all six armed/unarmed parity checks over three renderer epochs. Its
current loaded route records actual inner normal returns for placements47/82/51/78
and no stopped-callback packet. That narrow observer result is portable execution
provenance; it does not by itself establish a contact result or every query path.

Before the connected ACTIVE owner, the observer reached actor H8/223 versus
counterpart H8/51 at cell `[14592, 192, 45824]`, with A origin `[576, 0, -192]`.
B basis, record and B+44 alias were published before required context failure;
a later normal query preserved that history. These historical reached-prefix
observations remain distinct from current normal returns. Native contact,
exact CRT allocation cadence, excluded grammar/response branches and matched
full-intro acceptance remain open.
