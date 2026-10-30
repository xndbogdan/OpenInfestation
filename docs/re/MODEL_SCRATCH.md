# Infestation retained model scratch

This document owns byte storage and allocation custody shared by native model
consumers. Command grammar, values and dispatch remain in
[model runtime](MODEL_RUNTIME.md); briefing composition belongs to
[briefing runtime](BRIEFING_RUNTIME.md). The portable owner is
[`ProcessModelScratch`](../../crates/inf-game/src/model_scratch.rs).

## Native storage and initialization

`004EBE60` allocates unsigned WORD vertex counts at 24 bytes each, followed by
unsigned WORD shade counts at four bytes each, in one arena
`[00564728,0065E728)`, capacity `FA000`. Cursor DWORD `0065E728` immediately
follows it. The supported executable's `.data` has initialized/raw end
`00547000` and virtual end `00660C38`: arena and cursor are entirely in the
loader-zero tail. Their initial zero bytes are process startup evidence, not
an allocation, frame, scene, resource-load or display-reset policy.

The intrinsic executable mapper proves a requested `.data` virtual-tail range
without returning virtual zeros as original file bytes. It rejects image-bound
violations and overlap with other section extents. The process constructor
requires proof covering both arena and cursor. Explicit isolated fixtures use
unknown storage; their unknown fog/RGB does not describe a fresh native process.
Independent model value registers remain uninitialized stack history. This
arena proof does not initialize them or any unowned callback output.

`004EBE60` reserves vertices then shades in the same bytes. Its signed capacity
comparison admits the exact end. Capacity failure reaches managed error before
registering its own retirement callback; parent marks can already exist. The
portable normal domain uses checked arithmetic and rejects corrupt/wrapped
allocation arithmetic. Successful allocation registers `004EBEE0` with the old
cursor. Normal retirement unregisters that mark and restores the cursor without
clearing bytes. Different later counts can place a shade DWORD over a previous
vertex field, or a vertex over old shades.

## Exact write policies

The common owner `004A00E0` writes only ambient shade DWORDs 0/1 before normal
initialization. `004EBD30`/`004901B0` write three RGB bytes and preserve shade
high bytes. Other shade fields retain actual prior bytes. Late `004ED610`/
`004ED7E0` reserves the same vertex/shade layout but performs only its recovered
world-vertex initialization; it does not seed common projection or shades.

Near projection `004EB5A0` leaves vertex fog at `+15` untouched. Far projection
`004EB8E0` writes it only after the depth gate; its early path retains it. World
stores write three DWORDs. Projection writes commit the reached field bytes,
including a partial error prefix; only normal return permits the caller's EAX
depth DWORD store. `2C` applies its separate depth-minus-20 policy there.
Parent `2A` (`004ED350`) copies all 24 bytes, including partial world knownness,
projection and padding. Signed shade consumers can read a mapped DWORD beyond
local shade count, including retained vertex bytes, and validate all four bytes
against the whole arena. Intrinsic address-only shade fixtures retain unknown
surrounding bytes as an explicit fixture domain.

A negative reference can also reach below the pool. Terra Nova's furniture
record (bucket 160, index 0, lookup 31) draws a top-level model whose shade
-900 lands 256 bytes before `00564728`, at `00564628`. Native
`[005645E0, 00564728)` is the software rasterizer's span state in the
zero-filled tail of `.data`; its only writers (`004B0730`, `004B0AE0`,
`004B1110`, `004B1660`, `004B1D20`, `004B2760`, `004B6B10`, `004B6F20`,
`004B75F0`, `004B7BB0`, `004B8330`, `004B8FA0`) are software-renderer vtable
(`00528FF0`) methods with no direct caller, and the Direct3D vtable
`005290E8` shares none. Retail always builds the Direct3D renderer, so those
0x148 bytes read zero. The port reads them as zero; a reference further down
is refused before any write and omits only its model, like other model
features, instead of stopping the terrain pass. Before this, that refusal
counted as renderer custody: the stopped scene then failed the frame's
presentation and ended the session (a user's Terra Nova run, frame 7096, as
the scientist's radar blueprint arrived).

The portable vertex/shade decoded caches preserve the existing primitive slot
interface while writing reached fields through immediately. Allocation views
share one owner; no whole-model copyback or clone of byte history occurs.
Short checked borrows end before callbacks and projectors. Current active
allocations are disjoint; an eventual arbitrary same-slot alias writer must
provide explicit cache synchronization before being admitted.

## Process lifetime and typed stops

The application creates one owner after proving original executable storage.
Every ordinary, furniture and child model uses it through the same executor.
Renderer state transfer, resource epochs, loading/current-resource loans and
display candidate validation retain that owner. Renderer recreation and queue
reset do not create or clear model scratch.

The owner distinguishes Ready, Running and Stopped draws. Opaque process-bound
scope/registration marks enforce live range ownership and LIFO normal retirement.
Errors retain reached bytes, cursor and outstanding marks. Drop does not retire,
clear or recover anything. A subsequent draw cannot be admitted while stopped.

Native `004948C0` registers callback/argument pairs; `00494910` unregisters by
tombstoning without invoking them. Normal frame pop `00494A00` skips callbacks.
Managed nonlocal error `004949A0` advances the global cleanup pointer before
invoking each tag-2 callback with its raw argument, then rereads the pointer;
its marker branch transfers through longjmp. These facts do not make every Rust
feature error a native managed nonlocal error.

`RegisteredMarkUnwindApproximation` is the explicit portable containment policy
at admitted model-feature continuation and the existing uninitialized-value
prefix omission. It restores the actually registered old marks in LIFO order
and preserves all bytes. It is neither successful model completion nor retail
exception acceptance. Hard failures retain stopped custody. Follow-up ownership
is the full native guarded error/frame stack, rather than a blanket Err/Drop
reset. Queue retirement and value-prefix policies remain separate authorities.

## Producer coverage and evidence bounds

Auxiliary query/result interpreter `004F2C50` also uses `004EBE60`, with its
actual header vertex count and two shades; opcode05 recursion and normal88
retirement share this arena. It can write projected fields and full parent
copies. Shared table `00544740` has authored null projection entries that the
common dispatcher mutates and restores; auxiliary work uses retained selected
projection history. Neither authored nulls nor current model extent establishes
that history. Its remaining production interpreter/callback owner must borrow
this same arena when implemented. An unowned native producer is not assumed to
have run or to have no effects in the port's represented history.

Original briefing root686/child687 is the motivating source case: child `0A2A`
at OVL28 offset `515970` imports parent6/7 into child18/19; far edge at `516128`
consumes18/16. Supplied age2000 selects Near parent/Far child, so the parent fog
is retained storage and full-copy input. A supplied age0 is a distinct Far/Far
fixture; neither pose establishes a complete retail animation timeline.

[`model_scratch_tests`](../../crates/inf-game/src/model_scratch_tests.rs)
authenticates the original executable, `.data` metadata and complete allocator,
retirement and managed-cleanup bodies. Tests distinguish startup storage,
reallocation overlap, mapped signed reads, exact capacity, LIFO custody and
stopped prefixes. Adapter and original-model tests own reached writer/copy
coverage; whole-scene visual acceptance still requires the actual runtime
boundaries and matched observations.
