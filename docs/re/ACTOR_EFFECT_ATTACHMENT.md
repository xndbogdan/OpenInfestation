# Selector66 death-effect attachment

The `00405420` helper reached by shared dying `0042455E` creates H13 resource
46. The actual common `1.ovl` registration is local record 46, constructor
selector66, initial model zero and final table `0051F6C0`. It is a separate
actor which temporarily owns the original dying actor's model presentation.

The shared owner is
[`actor/selector66.rs`](../../crates/inf-game/src/object_runtime/actor/selector66.rs),
with [`attachment.rs`](../../crates/inf-game/src/object_runtime/actor/attachment.rs)
and the real world factory, update, draw and disposal adapters. There is no
intro-only actor implementation or field-only successful update substitute.

## Attachment and construction

`00405420` checks its source pointer before reading process NULL `0054E894`.
It calls `00460310` with full resource 46, constructor source EDX zero,
the parent's live `+32` position, NULL rotation, placement FFFFFFFF, mode zero
and the captured process NULL argument. The normal raw result and its actual
virtual `+18` actor result are separate. A NULL result/component skips the
suffix without deleting a returned nonactor.

After construction, the helper copies the current parent's `+6C`, source
handle `+84` into its `+88`, and current classification `+B4`, with the exact
dirty byte/WORD stores. It then calls the new actor's actual `+24(0)`.
Position is borrowed through the shared factory's allocation phase; it is
not an eager pre-allocation snapshot. Current process NULL is independently
loaned to the shared actor constructor. Typed construction failure retains
the allocated prefix under the existing stopped-construction policy.

Constructor `00405170` runs common actor construction, installs `0051F6C0`,
and writes zero to DWORDs `+13C/+140` only. Its `+08/+0C/+1C` are the ordinary
shared actor initialization bodies. `+138` and `+144/146/148` remain unwritten.

## Update, draw and cleanup

`+24 = 004051C0` resolves the actual owner handle each call. Missing owner
queues the helper for disposal. First update copies the owner's current model,
marks the model dirty and refreshes it before reading owner flags. It saves
those flags, disables owner flag0 then flag1, adjusts the separate raw flags,
copies the captured bit100 policy to the helper, clears helper bit20, writes
elapsed `+13C = 1` and returns. No particle birth occurs on that first call.

Later updates add the full wrapping frame DWORD. If bit100 is clear, actual
H12 terrain height clamps signed Y and cancels only negative vertical velocity.
The helper's current XYZ is copied to the captured owner. Lexicographic WORD
comparison against previous XYZ short-circuits at the first unequal axis.

The helper's table `0051F6C0` holds the shared furniture response `0047BFD0`
at `+A8` (`0051F768`), as Selector34/35's tables do, and the port's
furniture pass answers it the same way. A dying actor's debris on Terra Nova
reached it in the 2026-10-17 sweep, where the pass had stopped at "no owned
virtual +A8".
Unchanged position births smoke selector0. Changed position first saves all
three WORDs, births smoke selector1, then flash selector2. The smoke owner is
`00489BC0 / 00528874`; flash is `00489A90 / 00528854`, both using the live
particle allocator/process source/RNG. Birth callbacks can mutate the current
actor and the next birth rereads its position. Native unsigned elapsed/idle
gates either clear idle time, add the frame DWORD, or queue disposal.

`+2C = 004054C0` resolves the owner, saves its position, copies helper XYZ,
enables the owner's flag0 and calls that owner's real `+2C`. Only normal return
disables flag0 and restores the saved XYZ. The same near-draw dispatcher,
renderer, lighting, queue and weapon services are reused. Failures keep the
partial owner mutation and stop this allocation's helper draw; no finally-style
restoration or next-frame replay occurs. Cyclic ownership is also contained.

`+30 = 004053C0` resolves the owner, clears the saved flags' low two bits,
merges the remaining flags plus bit400000 into current owner flags, marks dirty
as required, then enables owner flag0 and flag1. It does **not** invoke common
actor cleanup `0047EC50`. The ordinary disposal manager then destroys the helper
through its common actor destructor. The referent remains separately owned.

`+98 = 00405490` adds an optional WORD displacement only for full selector5
and always returns false. Its intrinsic is present; the existing event10/22
notification dispatch correctly performs no writes for this final class.
A broader contact event5 dispatcher remains with the shared contact owner.

## Allocation and failure boundaries

The strict constructor preserves unknown saved flags and previous XYZ.
Production's explicitly selected **FreshHeapZeroExtendedCarryApproximation**
additionally supplies only the three previous-position WORDs at construction.
This extension is replaceable and can change the second tick's smoke branch;
it is not a native constructor write or retail allocator acceptance. Saved
flags `+138` always remain unknown until the first successful refresh suffix.
The native heap/history owner is the follow-up for removing this approximation.

Per-allocation update/draw stop markers are portable containment. A later
callback does not replay earlier model, position, particle or timer writes.
The existing disposal stop owner covers a failed teardown independently.

## Native comparison scope

The tracked generator reproduces the receipt;
the exact replay command and dependency requirements are in
NATIVE_RECEIPTS.md.

[`selector66_native.json`](../../crates/inf-game/tests/fixtures/selector66_native.json)
contains 36 original-instruction cases across attachment, update, draw and
cleanup. Actual membership setters and disposal execute. Factory, component,
owner resolution, refresh, ground-height, particle and referent draw returns
are explicitly supplied lower-service segments. Cases include signed terrain
clamps, wrapping clocks, source/classification mutation after factory, model
refresh mutation before saved flags, changed birth position, failed callbacks
and draw restoration only after normal return. These boundaries do not claim
that a supplied native lower factory or particle callback executed.

Rust composition also constructs actual resident resource46 through the shared
factory and guards the final vtable against the authenticated executable.
The dying production adapter loans the real services, and the ordinary manager
owns every later update and deferred destruction. Full scene acceptance remains
the matched retail sequence comparison in the active objective.
