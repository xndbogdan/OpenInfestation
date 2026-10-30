# Infestation shared water rendering

This document owns `0044C6A0`, water rows `0044C9D0/0044CCC0`, shoreline
quads `0044DBB0`, water projection `0044EAA0` and the height callback
`0044DFE0`. [Terrain runtime](TERRAIN_RUNTIME.md) owns the enclosing AA10
domains, ground rows and shared E150 clipping. The wave-height kernel
`00410F60` belongs to
[`TerrainRuntime`](../../crates/inf-game/src/terrain_runtime.rs).

## Caller state and shared integration

[`terrain_draw::water`](../../crates/inf-game/src/terrain_draw/water.rs)
serves every reached AA10 near strip. Its request retains the caller's H12
terrain, H14 flags, process animation clock, view/coordinate shift,
projection, fade, tint and renderer resources. Gameplay and briefing reach
the same owner through
[`SoftwareScene`](../../crates/inf-game/src/software_scene/terrain.rs).
There are no phase, campaign selection, OVL leaf or mission whitelists.

The native gate is signed `lowWORD(plane >> plane_shift) > -4096` after
ground submission. H14 bit40 controls waves, not admission of the plane:
water without waves can still submit authored shoreline quads. Gameplay
publishes the clock before AA10 at `0048834C..00488359`; briefing does so
at `0040C1B9..0040C1D2`. Both consume the current process animation value.
The retained world owns flags independently from terrain scale and plane.

C6A0 forces the Near palette and adds `spacing*0xB0C/4000` to the sort bias
before BC30. It does not subtract that bias internally. AA10 resets the
global to its saved input before each next near or far strip, so the water
adjustment does not accumulate across strips. BC30 resolves H7/74 again;
its absence remains a required-service failure even for an empty row domain.
Near palette RGB remains `tint*index/15` at night. The night branch replaces
only the packed material using native palette channels and renderer shifts.

## Rows, projection and retained bytes

Ground C130 and water C6A0 share the verified alternating two-buffer
traversal: fifteen16-byte vertices, union fill spans, WORD offset/count
metadata, shorter-row retained tails and the final reused offset. Their
complete row producers and tile policies remain separate. A previously
unwritten vertex remains an explicit Rust failure; initialized zero pixels
are not substituted for native stack history.

C9D0 resolves H12 before H14. It seeds the preceding height from one cell
left even when the row fills no vertices. Each water vertex consumes the
native wave/clamp height with live clock/flags. Its wet byte is the signed
raw terrain height comparison against the low-WORD shifted plane; wave
height does not choose shoreline membership. CCC0 chooses palette index
`clamp(((signedWORD(currentY)-signedWORD(previousY)) >> 5)+6, 0, 15)`.
Water does not read ground's dynamic RGB grid.

EAA0 uses CRT `00511BA8` truncation for transformed coordinates and
perspective, while ground's model projector uses nearest-even FISTP.
X/Y dot products evaluate `(Z+Y)+X`; depth evaluates `(Y+Z)+X`. A depth
below64 writes XY/depth and clipC0 while leaving fog genuinely unwritten.
C9D0 additionally tests the signed low AX result and overwrites the X WORD
with eye-relative X when it is below64, even if the full depth was larger.
Buffers preserve the prior fog byte on this branch. Shared E150 clipping
retains its own projector and operation order.

## Shoreline commands and resource order

DBB0 forms wet combination `A + 2*B + 4*C + 8*D`. Combination0 exits before
the lookup table. `00537010` contains sixteen8-byte records: signed H3
sprite WORD at+0, unused WORD at+2, orientation DWORD at+4. These are read
from the authenticated executable through `PrimitiveTables.water`, separate
from the ground blend LUT. Consumed sprite slots are1331..1335.

There is no ground determinant cull. A direct tile first passes clip-table
admission, then rejects all-four fog255 before H3 lookup. Admitted H3
resolution precedes sorting and allocation. Negative signed slots and slots
greater than H3 capacity return native null; an accepted missing slot requires
its lazy loader, and the native unchecked one-past capacity access remains
an explicit failure. Missing valid residency is not an empty water tile.

All-zero fog selects EA60; any nonzero fog selects EA70. Both reserve
four16-byte blocks, mode+20 is0, and H3 relocation is at+24. Four RGB DWORDs
retain the wet byte in their high byte; orientation permutes vertices, shade
and fog together. The Far packet appends BC30 material at+38 and fog at+3C.
Neither callback substitutes an ordinary model-quad packet.

A clip-table rejection resolves H3 before invoking E150. The latter tests
actual near-plane crossing before dereferencing first-physical sprite
dimensions. DFE0 recomputes water heights from current row metadata and the
asymmetric C/D X inputs; it does not reuse stale vertex heights. The shared
clip request keeps signed eye-relative WORD coordinates, orientation,
material and context+62 bias. E150 retains its native retry/interpolation
order and EA50 triangle packets.

## Evidence and remaining boundaries

The original-instruction checker executes
unchanged C6A0/BC30/C9D0/CCC0/DBB0, EAA0, DFE0, 410F60, E150, CRT conversion
and texture sorting. Resident resources, published view/fade and process
clock are supplied inputs; renderer query and arena allocation are explicit
required hosts. The
[versioned receipt](../../crates/inf-game/tests/fixtures/terrain_water_native.json)
and [Rust comparisons](../../crates/inf-game/src/terrain_draw/water/tests.rs)
cover76 cases (50 strips,16 projectors,10 heights), including all shore
combinations, both wave/scale policies, WORD wrap, packets reached through
retained row tails/fog, clipping, null textures and failure prefixes. Two night
cases execute the original palette/layout getters with supplied RGB555/RGB565
channel shifts; two strips pin the extra signed low-AX X overwrite even at
admitted full-DWORD depth. E150's scratch shade DWORD has only three authenticated RGB
stores; its unknown high byte is excluded from equality rather than promoted
to a captured zero.
Reproduce with `python check_terrain_water.py
--output <local.json>` in a runtime containing Unicorn. The checker rejects
an unsupported executable hash; transient output stays local.

Production command receipts count actual water queue deltas, including a
completed prefix before a failed tile. Per-feature omission reporting keeps
ground prefixes and later strips/passes enabled, with newly reached causes
logged across frames until the terrain epoch changes. The
[world regression](../../crates/inf-game/src/world_scene_tests/water_pixels.rs)
uses supported campaign18/leaf28's ordinary water through the complete world
draw and retained receipt. The
[briefing regressions](../../crates/inf-game/src/briefing_renderer_tests/water_pixels.rs)
use authored wave worlds16/34 at640x480 and1024x768, comparing clock0/256.
Their view/pane framing is supplied, and mutable sparse lookups are disabled
to isolate water from furniture animation. Original H12/H3/H7 bytes remain
unchanged. Other campaign loads may still stop at unowned actor disposal;
successful water rendering does not establish that loader suffix.

Terrain-emitter C4A0,
remaining actor/model callbacks, briefing's static ground-light approximation,
portable texture allocation ordering and complete retail-frame comparison
remain separate bounds. Supplied native kernel receipts and port pixels do
not establish matched intro or mission-briefing acceptance.
