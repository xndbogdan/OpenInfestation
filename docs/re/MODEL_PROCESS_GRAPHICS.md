# Infestation retained model graphics and ACTIVE auxiliary execution

This owner joins the current renderer publishers with the shared furniture
ACTIVE query. `ModelProcessOwners` retains one process scratch pool and one
`ProcessModelGraphics` history. Cloning the carrier shares both owners. The
session, loading draws, renderer epochs, display replacement and actual
Selector12/36 collision callers borrow that same carrier.

The [auxiliary format](MODEL_AUXILIARY.md) owns byte-stream decoding and the
full native grammar. [Furniture collision](FURNITURE_COLLISION.md) owns the
entry/scan and response routing. [Process scratch](MODEL_SCRATCH.md) owns the
combined 24-byte vertex / 4-byte shade arena and registered allocation marks.

## Startup, selected aliases and real publications

Original `00544740` holds Far local/world at `+00/+04`, Near local/world at
`+08/+0C`, and file-backed NULL at selected `+1C/+20`. Process startup checks
those nine DWORDs and the PE loader-zero range `0055F190..0055F254`: view,
world publication, composed transform, projection/fade and coordinate shift
are known zero exactly once. This establishes neither a camera pose nor a
selected Near/Far callback. Actor heap fields have a different owner.

`004386FC` copies the CURRENT default table into renderer table `00546E60`.
The carrier records this at the admitted initial renderer and successful
physical replacement, before publishing its installed renderer. Candidate
validation, `SoftwareScene::new`, epoch resume and current-resource loans do
not perform another copy. An unreached renderer copy is required backing;
a reached copy containing a NULL selected slot remains a native NULL.

Common entry and normal child return select through the actual context
`+3C` alias. Default `00544740` and inherited renderer `00546E60` are distinct
mutable histories, represented by the existing `ModelSurfaceTableOrigin`.
Culling precedes selection. Error returns retain the last reached child
selection; they do not restore a pair merely because the invocation stopped.

`ModelProcessPublication` forwards the existing backend operation before
recording its reached publication. `0048FB40` consumes actual global view
`0055F190/194/198`, basis `0055F19C` and shift `0055F250`. The adapter checks
that the forwarded transform agrees with those current owners. It records
world origin/basis and composed transform only after that check. It does not
adopt a model caller's unrelated view or a fresh scene configuration.

Actual Scene view/shift/setup/fade commands publish their corresponding
fields. Menu and loading projection-window publication changes only the six
focal/viewport/center DWORDs, preserving fade/factor and selected table.
Fully supplied isolated fixtures explicitly publish their stated inputs;
they do not create a production fallback or per-query reset.

## Local A and retained B

`0047C1D0` prepares stack A once at the first ACTIVE candidate, using the
saved signed actor XYZ relative to the current cell. It publishes B's actual
basis and resource/record aliases at their reached native phases. The
retained B origin at `0055EF38`, table alias, last allocation and raw-source
qualification survive queries and renderer/world transitions.

`004F2BE0` captures B minus A before either interpreter clears its working
origin. A is local to this request; B remains the same process owner.
`004F2C50` first clears only result flag, then tests unsigned header WORD0E.
Zero returns before context stores or allocation. Nonzero entry clears the
working origin, installs the actual default-table alias, allocates unsigned
header WORD08 vertices plus two shades, records allocation/raw XYZ and calls
`0048FB40`. It seeds no ambient, vertex range, saved basis or registers.

B's last allocation can be retired while its published descriptor still
exists. Retirement preserves bytes and does not make a stale descriptor a
live writable view. Raw-source identity uses the captured H8 registration,
weak allocation and vertex span; it does not keep a released allocation
alive or reinterpret it through a replacement resource table.

## Owned generic grammar and geometry

`model_auxiliary.rs` executes the following reached domain independently of
model IDs. Native raw-local vertices, integer world coordinates, screen
pixels and packed normal DWORD bits retain distinct units.

| Operation | Actual phase |
| --- | --- |
| `05` | Retained H8 child acquisition, inherited callback source and query, normal-only parent restoration |
| `07`, `004EBF00` | Capture signed raw XYZ once using signed destination/2; world then mandatory local projection on pass0, mirrored-X world only on pass1 |
| `18/2F`, `004EC410/004EC950` | Explicit auxiliary command through shared midpoint/interpolation arithmetic and ordered projection |
| `30` | Saved-basis orientation and rotation with asymmetric first/second mode tests |
| `8E`, request table `+00` | Clear result flag/score before signed slot read and actual callback |
| Composite `00544C68+00`, `004F18B0` | Compute child relative origin and recurse on captured counterpart H8 using B and sphere table; republish parent A only on normal child return |
| Sphere `00544C08+00`, `004F1420` | Reuse the shared exact SpherePair contact geometry owner |
| Point `00544BF0+00`, `004F1400` | Zero query radius before the shared sphere-pair kernel |
| `8F`, sphere `00544C08+08`, `004F1F90` | Three authored DWORD extents and signed vertex slot through the strict sphere-box kernel |
| `89/8A`, table `+0C/+10` | [Group tests](MODEL_AUXILIARY.md#group-tests-898d) of the ray, sphere, point, composite and terrain tables; a cleared flag jumps to the group's end |
| `8B/8C/8D` | Require a hit; open a group at score -1 with its end; publish an open group as a hit |
| `95` | Branch unless flag is exactly1; successful callback promotion is suppressed when the next opcode is95 |
| `88` | Retire registered allocation before publishing a strictly positive best score; rejection clears only flag |

Callback score `-1` inherits previous score and all three normal DWORDs,
including unknown bits. Best promotion is strict signed greater, preserving
the first equal result. One draw scope contains all recursive allocations;
child retirement does not clear its bytes or B's publication.

The shared projector reads the selected `+1C/+20` pointer before inputs or
projection writes. Native NULL stops there. Near/Far arithmetic remains the
existing shared owner: partial depth/XY/clip/fog writes survive an error;
Near and below64 Far leave prior fog untouched. No auxiliary setup chooses
Near/Far, fills padding or resets the process arena.

Other opcodes and callback table offsets stop at named reached boundaries.
A reached backward95 requires an owned progress/termination contract and
fails explicitly; there is no invented instruction budget. The complete
5,142-header source census for the original `07/8E+00/95/88` execution domain
found 2,616 inactive headers, 2,059 complete programs and 467 first-unowned
prefixes; these counts do not claim coverage of the subsequently owned child,
rotation, interpolation or sphere-box operations. No inspected
owned program or owned prefix had backward95. That census is static source
coverage, not evidence that every program executes or completes in gameplay.

## Response and failure custody

The cell scan uses the actual contact result. Flag0 advances to the next
cell. Flag nonzero reaches actor `+A8`: final10/12/36 use the existing
`0042A760` family guard; final35 calls `0047BFD0` directly. Guard bit8000
clear returns AL1 before continuation operands. Dependent `0047BFD0` remains
a typed required response with the captured pair and contact; no response
or movement is synthesized.

Strict auxiliary errors retain registered marks, bytes, B, table selection
and contact prefix. The consumed outer adapter may select the existing
`RegisteredMarkUnwindApproximation` only for an explicitly required feature
boundary. It retires that stopped draw's registered marks LIFO, preserves
all committed state and RETURNS THE ORIGINAL ERROR. It does not manufacture
an `88` return, contact, cleared guard, restored actor XYZ or completed cache.
Pool, borrow, malformed input and publication custody failures stay hard.

Only normal collision return permits the existing `00429EE0` XYZ restoration
and cache completion. First ACTIVE preparation is not repeated for later
candidates in the same query; each candidate retains its fresh record/state,
height, basis and relative-origin reads.

## Verification bounds

Executed [auxiliary cases](../../crates/inf-game/src/model_auxiliary/tests.rs)
use unchanged original 223/51 and 492/392 programs with explicitly supplied
view/table publications, plus failure/order fixtures. They pin native NULL,
complete recursion, best-result rules, retired B identity, reached projection
prefixes and hard preexisting draw custody. Separate scan cases pin one-time
ACTIVE preparation and normal-only restoration. Whole original EXE bodies,
tables, PE zero-tail facts, OVL/PreLoad hashes and program spans are guarded.

The [paired linked observer](ACTOR_BEHAVIOR_RUNTIME.md#test-only-first-miss-provenance)
retains the same owners across three renderer epochs and passed all six
armed/unarmed parity checks. Its current loaded route records four actual inner
normal returns and no stopped-callback packet. These portable snapshots do not
establish native contact or complete coverage merely through absence of an error.
Further owners are the excluded auxiliary operations and callback policies,
full `0047BFD0` response, saved/register context producers, CRT allocation cadence,
actor heap fidelity and matched retail scene acceptance. Original overlays and
generated assets are unchanged.
