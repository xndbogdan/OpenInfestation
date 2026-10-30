# Infestation selector 60 retained spawn rows

This document owns construction, command 19, after-load row allocation,
update/spawning and destruction for selector 60 (`00480800`, vtable `00527DE8`). Shared derived
actor state and registration remain in [ACTOR_RUNTIME.md](ACTOR_RUNTIME.md).
The authored record layout is owned by
[INSTANCE_ASSETS.md](INSTANCE_ASSETS.md#selector-60-placement-spawn-lists).
The runtime implementation is
[`selector60.rs`](../../crates/inf-game/src/object_runtime/actor/derived/selector60.rs),
with shared mutable allocation custody in
[`allocation.rs`](../../crates/inf-game/src/object_runtime/allocation.rs).

## Construction and callback order

`00480800` first calls `00425E60`, including its one shared RNG draw. It then
writes the following fields and installs its final vtable. Unwritten padding
and coordinates are not initialized by the portable class either.

| Object offset | Constructor value |
| --- | --- |
| `+184`, `+188`, `+189` | BYTE zero |
| `+18A` | WORD one |
| `+18C` | DWORD 1000 |
| `+190` | Null retained list pointer |
| `+194` | DWORD zero |
| `+198..+19C` | Three unwritten coordinate WORDs |
| `+19E`, `+1A0` | WORD zero |
| `+1A4` | DWORD zero |
| `+1A8` | BYTE zero |

The same retained184/198 fields serve the
[cached selector9 model callback](OBJECT_MODEL_CALLBACKS.md#selector60-cached9-capture).
That captured-source draw owner stores the first reached invocation row and
reuses it on later calls; construction and spawn-list lifetime remain here.

Payload initialization remains `00426BF0`; the `+1C` factory completion
callback is shared `0047AAD0`. The later `+34` callback is `00480BB0`, reached
through `00401320` at `+20`. The wrapper ORs `00800000` only after the class
callback returns. Normal virtual `+24=00480910` updates the same retained
rows through `00480510` and `00480610`; construction itself does not execute
those callbacks.

## Update and spawn ordering

[`selector60/update.rs`](../../crates/inf-game/src/object_runtime/actor/derived/selector60/update.rs)
owns the complete `00480910` arithmetic. Negative signed `+144` returns;
positive `+144` stores its signed countdown result, calls shared
`0047B1E0(delta,1F)` and returns even when that tick reaches zero. With zero
delay, `+1A4 <= 0 && BYTE+1A8 != 0` closes the update. Otherwise a nonnull
retained list resets `+188/+189` to one, `WORD+18A` to zero and `DWORD+18C`
to `FFFFFFFF`, then subtracts the unsigned `+194` cooldown with a zero floor.

Each `00480510` row scans its current signed capacity, resolving non-NULL
handles through the real registry. Missing actors and actors with signed
`+C4 <= 1` clear their slots to the current process NULL. Flag bit 0 adds
the live count into the wrapping aggregate WORD. The signed total limit,
threshold and capacity determine completion and the retained hysteresis byte;
an active row decrements its unsigned timer or increments its wrapping
priority when expired. The smallest active timer becomes `+18C`. The update
selects the first strictly greatest **signed** positive priority, preserving
ties and rereading the retained list/count after each row.

The maximum of cooldown and minimum row timer drives the `+1A0` indicator
and the unsigned-WORD opening value `+19E`. Both opening directions preserve
the original multiply-high constants and DWORD overflow, rather than using a
floating-point easing function. A selected row spawns only when cooldown is
zero and opening equals `FFFF`. After a normal spawn return, the update
rereads list WORD `+2`, sign-extends it and stores `value * 100000` into
`+194`, including normal factory rejection and zero-placement completion.

The live `00480610` adapter first resolves current H14/0, then reads the
row's one-based placement reference. A zero reference completes the row.
Otherwise it captures the placement's full resource DWORD, command-suffix
address and XYZ; H12/0 height is added to Y with WORD wrapping. It rereads
the placement pointer for rotation WORDs in order `+E,+A,+C`, then invokes
shared `00460310` with those position/rotation words, placement `FFFFFFFF`,
mode zero and current process NULL. Commands use the same mutable H14 image
retained by the world loader. Resource rejection returns without completing
the row or clearing its priority.

On a nonnull factory result the row priority is cleared **before** raw-object
virtual `+18`; virtual `+20` then runs on that same raw object regardless of
the component result. A missing component completes the row. A component
increments total count and consumes real `0048F060`, interpolating the signed
endpoint WORDs with wrapping multiplication and arithmetic shift before the
`*100000` timer store. Flag bit 2 retains the placement transform. Otherwise
the parent's first `+48` basis pointer is retained while `00480EA0` supplies
spawn coordinates, then the position and the current contents of that first
basis owner are copied into the component. The first process-NULL handle
slot receives the component's handle; no free slot is not factory failure.

`00480EA0` returns the existing `+198` triple when BYTE `+184` is nonzero.
When zero, it captures the wrapping negative radius WORD before a second
parent `+48` getter, truncates its products with the stored Z vector under
FPCW `027F`, and adds the parent's then-current XYZ. It does not set `+184`.
This shares the cached coordinates with the model callback without inventing
an initialized cache or copying a stale first-basis snapshot.

Every error preserves earlier object, row, allocation and RNG writes. The
portable `update_stopped` marker belongs to that actor allocation and rejects
later update visits, preventing duplicate births after a failed suffix. It
is containment, not an emulated native field or cinematic acceptance.

## Command 19 retains and mutates shared storage

`00480B30` delegates signed codes at most 18 to `00426030`, accepts 19, and
rejects larger codes. Code 19 first stores its operand address at `+190`,
then reads the signed DWORD count at operand `+4`. A nonpositive count
returns successfully without reading operand `+0` or the rows pointer `+8`.
The mode WORD is consumed by later behavior, not this command.

For each reached positive-count row, stride `0x18`:

1. Reload the retained list and its relocated `+8` rows pointer. If the
   row's capacity WORD `+6` is zero, write one. Negative capacities remain
   negative.
2. Reload the list and rows pointer again, then write DWORD zero to row
   `+14`. This clears a runtime pointer without freeing the pointed-to row.
3. Reread the list's count before deciding whether to continue.

These are reads from one shared allocation image, not parsed row snapshots.
For example, if the first row's `+14` aliases the list's count, the zero
write can terminate the walk immediately. Earlier normalization writes and
the retained address remain published if a later host boundary rejects.
The first mandatory count-read failure also preserves the retained address.

Inspection still decodes complete bounded authored rows. Dispatch uses an
explicit read scope and retains the operand address without eager decoding;
the runtime handler performs only the reads it actually reaches.

## After-load and the 20-byte row owner

For a null `+190`, `00480BB0` calls real disposal (`00479FA0`) and returns.
The wrapper subsequently sets `00800000`; this path does not set `01000000`.
A nonnull list walks its current positive signed count, allocating a
20-byte helper for each row and storing the returned helper pointer at
row `+14`. It reloads the rows pointer for that store and the count before
the next iteration. A nonpositive count allocates nothing. On successful
completion it ORs `01000000` without changing the low-nine-bit dirty state.

`00480460` initializes each helper as follows:

| Helper offset | Meaning established by constructor |
| --- | --- |
| `+0`, `+1` | BYTE zero |
| `+2`, `+3` | Unwritten padding |
| `+4` | Sign-extended source row WORD `+A` |
| `+8` | Owned handle array, signed source capacity WORD `+6` times four bytes |
| `+C` | DWORD live-handle count, initially zero |
| `+10` | DWORD zero |

The helper looks up the full signed source DWORD `+0` using `0047F7B0`,
then resolves that handle through `0047AD90`. With source flag bit 1 set
and a resolving actor, it performs the placement lookup again, stores the
handle in array element zero and increments the live count. With bit 1
clear and a resolving actor, it disposes that actor. Missing actors and
remaining array elements use the configured global null handle.

Placement references are signed, one-based comparisons against a
sign-extended placement WORD. Negative references fail; reference zero can
match placement `FFFF`. Lookup scans the published actor list in order,
including disposed actors until destruction removes their node. It does not
index constructors, models or an independently reconstructed placement map.

The port owns successful helper and handle-array allocations. Negative
capacity reaches an explicit unsupported allocation-size boundary. A zero
capacity with a resolving actor and flag bit 1 set reaches an explicit
out-of-capacity initial-handle boundary; ordinary command 19 normalizes
zero before after-load. Native allocation failure is not simulated as
successful callback execution.

## Aliasing and destruction

The class destructor `00480880` visits the **current** retained table and
current row pointers. Each nonnull pointer runs `00480500`, which releases
the helper's handle array. `00480880` then releases the helper itself. The
destructor does not clear the shared row pointer. Derived/shared actor destruction
follows that row pass.

The portable arena uses nonreused row IDs containing arena identity and
heap slot identity. The shared allocation stores typed pointer slots
separately from its scalar byte image. This preserves these native lifetime
asymmetries:

- Repeated after-load overwrites row pointers and leaves previous helpers
  orphaned. Command 19 also clears pointers without freeing helpers.
- Destroying one of two actors sharing a list frees the current pointed-to
  helper even if the other actor allocated it. The shared pointer remains
  dangling; a later cleanup reports that specific freed row ID.
- A later command 19 can clear the dangling pointer. It cannot resurrect
  the old heap allocation, and later allocation receives a new ID.
- Orphan helpers remain owned by the arena until its teardown. Dropping a
  single actor does not opportunistically reclaim them.

Factory initialization failures attempt this same class destruction. If a
retained-table boundary also stops cleanup, `ObjectConstructionError::Cleanup`
reports the still-published object ID and destruction error. Earlier command
effects, retained input, actor-list membership and handle registration remain
inspectable; the factory does not claim that cleanup succeeded. The focused
[`selector60_factory_tests.rs`](../../crates/inf-game/src/object_runtime/selector60_factory_tests.rs)
test verifies that boundary and explicit recovery through a valid empty list.

Typed pointer slots have opaque zero bytes in diagnostic scalar snapshots;
these zeros are never usable runtime scalar values. Every actual scalar
read rejects an overlapping typed pointer. An exact full-DWORD zero store
can clear a typed pointer, and an exact typed-pointer store can replace it;
partial scalar writes reject. Nonzero raw bits read as a runtime pointer
are rejected as unowned native pointers. Existing relocated-address write
boundaries also remain explicit. Thus arbitrary aliases that depend on a
native allocator's address fail at the reached read/write instead of using
a fabricated address.

## Damage: virtual +B4 00480C80

The spawner's `+B4` (table `00527DE8`) gates the derived reaction `004283E0`
on its retained spawn list `+190`. Without a list it reacts. With one it
first clears DWORD `+144`, then switches on the list's sign-extended first
WORD (an unsigned compare against 4 sends negative values to the default):

| WORD | Reacts |
| ---: | --- |
| 0 | never |
| 1 | always |
| 2 | while byte `+188` is set |
| 3 | while WORD `+18A` is zero |
| 4 | while byte `+189` is set |
| any other | always |

Spawners reach it when their own spawns touch them: the 25-world sweep's
pair contacts in worlds 15, 17, 20 and 21 (`tomspawn`, placements 83, 98
and 102, 83, and 80) stopped there until the port's world damage host ran
it ([`selector60_reacts_to_damage`](../../crates/inf-game/src/object_runtime/actor/derived/selector60.rs)).

## Evidence and verification limits

Static retail disassembly establishes the source order above. Host tests in
[`selector60_tests.rs`](../../crates/inf-game/src/object_runtime/actor/derived/selector60_tests.rs)
exercise lazy reads, dynamic count overlap, pointer custody, actor lookup,
disposal, orphan lifetime, dangling cleanup and explicit scalar/address
boundaries.

Versioned native receipts retain their distinct coverage claims:

- [`selector60_native.json`](../../crates/inf-game/tests/fixtures/selector60_native.json):
  ten cases and 96 snapshots. Complete unmodified calls cover constructor,
  payload, commands, factory post-initialization, null/nonpositive-count
  after-load and destruction with no reachable allocated row states. The
  positive-allocation case separately records twelve checked allocation
  boundaries and resumes retail tails with explicit supplied storage; it is not an uninterrupted
  native allocator receipt. It covers repeated after-load and command-19
  orphaning. SHA-256:
  `DE69862BAC432D8D502DF847B87AA28C4A6DA1E98D0C9402CC27B08D54CB5907`.
- [`selector60_row_allocation_tails_native.json`](../../crates/inf-game/tests/fixtures/selector60_row_allocation_tails_native.json):
  42 checked constructor prefixes, of which 38 have supplied-storage tail
  receipts, preserving the real placement lookup, registry resolution and
  disposal calls. Zero/negative capacities retain their own boundary
  coverage. SHA-256:
  `1FE6F57F350E98647C2A67E67749169952A0FCE88D7CBB642E034F855A3679EE`.

- [`selector60_update_native.json`](../../crates/inf-game/tests/fixtures/selector60_update_native.json):
  92 cases over unmodified `00480910`, `00480510`, `00480610` and `00480EA0`,
  with real RNG and truncation helpers. They cover signed/unsigned gates,
  timer and multiplication wrap, handle census, row priority ties, spawn
  rejection, component/initialization order, late basis mutation and stopped
  prefixes. H14/H12 data are supplied; registry results, physics, height,
  factory and virtual callbacks are explicit lower-service boundaries. The
  Rust consumer compares intrinsic state/order and separately verifies whole
  body/vtable guards; it does not claim those supplied lower bodies ran in Rust.
  Reproduce with `python check_selector60_update.py`
  (installed Unicorn 2; compare by default, `--write` regenerates LF JSON).

These receipts establish the bounded routines and side effects shown. The
production host composes the shared factory/after-load/physics owners, but
complete intro cadence and visible spawning still require the full video
comparison; a matching isolated row does not establish cinematic acceptance.
