# Selector12 grant-grid targets

`00468E30` is the fifth periodic callback in Selector12's table `0053A0E0`.
Its record at `0053A110` contains callback `00468E30`, actor offset `+284`
and a NULL input factory. It supplies the `+284/+288` weighted point target.
The parent full scan retains order `1,2,3,4,0`; this owner neither schedules
that scan nor supplies allocation-carried timers or descriptor state.

The implementation is the shared
`object_runtime/actor/derived/target_producers/grant_grid.rs` kernel and the
production `FamilyGrantGridHost` loan. Its resource lookup `00436BC0` also has
other native callers and remains reusable independently of Selector12.
There are no level IDs, synthetic referents, RNG draws or H8 model reads here.

## Resource and coordinate custody

Entry captures the current H12/0 pointer, invoking its virtual `+0` getter
when the cache is NULL. That captured identity supplies candidate heights
throughout the scan. Every grant query independently acquires current H12;
a later resource selection must not substitute its terrain for entry's
captured height allocation.

The initial read order is actor WORD X `+32`, current spacing `005372F0`,
actor WORD Z `+36`, then current fraction mask `005372F4`. With spacing `s`
and mask `m`, each axis starts at `(origin + s - 1201) & ~m` and ends at
`(origin + 1200) & ~m`, using wrapping DWORD arithmetic. The count is
`(((end - start) & 0xFFFF) / s) + 1`; native reads spacing separately for
Z and X division. X is outer and Z inner. Each inner iteration reloads
spacing even when no grant is found or it is already owned. The outer
increment uses the final inner spacing read.

This port domain uses the loader's spacing/mask pairs `(128,0x7F)` and
`(256,0xFF)`. In this domain native retained upper register bytes do not
change any LOWWORD coordinate, masked count difference or terrain index.
Coordinates wrap at WORD boundaries. A fine axis has 18 or 19 points; a
coarse axis has 9 or 10. Unsupported corrupt globals and native IDIV fault
parity remain outside this enum domain. Current actor XYZ backing is known
or unknown as a whole; independently partial-known actor coordinates are
not synthesized.

## Terrain grant lookup

`00436BC0` captures candidate X/Z LOWWORDs before its mutable H12 acquisition.
It reads current fraction mask and computes aligned grid coordinates using
unsigned WORD shifts by seven and signed `(~mask) >> 7`. The H12 index is
`grid_x * 512 + grid_z`. The cell's marker `0x0400` gates the sparse bucket
lookup. Canonical H12 decoding proves sorted unique X records and fixes
record counts from byte sizes; binary search is equivalent to the native
one-based search in that domain. Mutable sparse payload writes preserve
those X/order/count invariants.

No marker, missing record or lookup byte zero returns grant AX zero before
H11 access. A nonzero sparse lookup first causes a fresh H12 cache check
and reached getter. This second result is ignored, including genuine NULL.
It then lazily acquires actual H11/0 and reads byte `+2` of that lookup's
four-byte row: the LOWBYTE of `MissionPlacementSlot.parameter_word`.
All nonzero lookup bytes, including `FD/FE/FF`, are ordinary H11 indices in
this consumer; their special meanings in other furniture/effect consumers
are not imported here.

The resulting furniture-kind byte selects the grant WORD at
`00532F88 + kind * 16 + 0x0C` (`00532F94 + kind * 16`). This is separate from
the record's damage resistance, threshold and effect program. H8 resource
and model metadata are not needed to find the grant. Missing required H11
backing must not turn into grant zero.

## Selection and publication

Grant zero bypasses the embedded grant owner. For a positive grant,
`004752E0` freshly reads Selector12's six-DWORD mask at `+250`; an owned grant
skips all candidate height and score reads. IDs outside the retained
0..191 mask remain a typed required-backing error rather than unowned=false.

For an unowned grant, current mask realigns candidate X/Z. Native rereads
actor X before sampling entry's captured H12 signed low height byte, then
reads the current height shift `005372F8` (supported 4/5). Actor Y and Z are
reread afterward. Each coordinate difference wraps to WORD, sign-extends
to `i16`, squares, and contributes to a wrapping DWORD sum. This is raw
cell height; interpolation, plane projection and waves do not participate.

`00427620` freshly reads radius DWORD `+164` and subtracts its existing
signed-input `0048EFC0` square-root result. The winner must be strictly
greater under signed comparison than initial zero; equal scores retain
the first X-outer/Z-inner point. No positive winner passes NULL point and
score zero to `00427390`, then returns AL zero. A positive winner rereads
radius, publishes `(best >> 1) + 2 * radius` with wrapping DWORD arithmetic,
and returns AL one independently of the publisher's Boolean result.

The shared point publisher owns `+284/+288`, target release/reuse,
allocation and constructor fields. Its result indicates a previously empty
pointer; genuine allocation NULL or same-class updatefalse does not change
the grant-grid producer's positive return. The production request supplies
the live process NULL source so `0054E894` is read only after successful
allocation. Pointer copies, pending reservations and explicit release retain
the shared target heap's custody rules.

## Errors and evidence bounds

Short terrain loans use the prepared mutable world copy and authenticate
current registration plus allocation identity at each acquisition. They do
not clone or re-decode history. Required NULL surface fails only at its
reached marker or captured-height read; refresh-only NULL is ignored.
Registration/prepared-terrain replacement and target heap custody faults
remain hard. Reached host effects and publisher score/release/allocation
prefixes survive errors; this producer does not unwind them or exchange an
actor list. Failures before publication retain the previous target pair.

The native guard tests cover complete original bodies `00468E30..0046902A`
(506 bytes), `00436BC0..00436CF4` (308), `004752E0..0047531A` (58),
`00427620..00427638` (24), `00427390..0042748B` (251),
`004502C0..00450342` (130) and the literal periodic record, under the
original EXE BF09 hash and canonical PE profile. Focused supplied fixtures
exercise capture/fresh resources, lazy H11, ownership, signed selection,
wrapped coordinates and actual shared heap publication prefixes.

Those are source and fixture bounds. This owner does not establish complete
Selector12 virtual `+24`, later movement/attack services, blue-soldier
state-machine timing or a complete visible intro. Actual loaded host routing
and matched retail acceptance remain separate from intrinsic kernel tests.
