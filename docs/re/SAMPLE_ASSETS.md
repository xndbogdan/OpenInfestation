# Infestation Handler-10 sample assets

Handler 10 contains the sample grids consumed by `00470170`. The structural
record decoder belongs to
[record_tables.rs](../../crates/inf-formats/src/record_tables.rs); the
[record-table exporter](../../crates/inf-extract/src/tables.rs) retains its
allocation bytes and global registration. This document owns the consumer
evidence and the limits of the typed sample interpretation.

These are screen-space terrain/map sampling inputs. Constructor `00471AE0`
resolves Handler-12 slot 0 and calls `00471C40` over a 256 by 256 grid, producing
the byte map at `0054ED78` from terrain cells. `00470170` samples that map and
writes 16-bit pixels into the display surface using its pitch and clipping
rectangle. This establishes a rendering use; it does not establish physics
kernels or a complete named HUD feature.

## Allocation and identities

Loader `0050F890` reads the size-prefixed allocation and registers each
`0x1C`-byte record through `004EFC00`. At `0050F8EC..0050F905` it adds the
allocation base to the words at record `+0x10`, `+0x14`, and `+0x18`.
Original bytes contain allocation-relative offsets, including zero if
authored; zero must not silently become a null pointer. No source fixups are
needed for an asset decoder.

The H10 handler object is `0055EEB0`, its global-slot array pointer is
`0055EEB4`, and capacity is `0055EEB8`. The consumer explicitly resolves
**global** slots 0, 1, and 2. Local indices must first pass through their
PreLoad registration mapping; a local record number does not independently
establish a sample role.

| Global slot | Proven consumer data | Consumer access |
| --- | --- | --- |
| 0 | Width and height DWORDs at the `+0x10` target; eight signed WORDs per stored sample at the `+0x18` target | Dimensions: `0047019A..004701C3`; data pointer: `004701C7`; eight WORD loads: `00470589..004705B0` |
| 1 | One signed-byte mask per stored sample at the `+0x18` target | Pointer: `00470230`; signed byte load: `004704E0`; mask branch: `0047074F..0047077C` |
| 2 | One signed-byte color-table index per output location at the `+0x18` target | Pointer: `004701FA`; signed byte load and table lookup: `00470571..00470576` |

Helpers `00471A80` and `00471AB0` return the first and second DWORD at slot
0's `+0x10` target. The rendering consumer uses signed arithmetic on those
dimensions, doubles them for its output extent, and subtracts the original
dimensions from the packed destination center. It does not consult the other
two records' dimension words when traversing their data.

## Sample widths, traversal, and meaning

Slot 0's sample cursor initially points six bytes into a record. Native
`MOVSX` reads at cursor offsets `-6,-4,-2,0,+2,+4,+6,+8`: exactly eight
**signed** 16-bit words. `0047095C` advances it by 16 bytes. The stored-row
stride is `width << 4` (`00470489`, applied at `00470E8F`). These are four
ordered pairs, not eight independent resource indices.

The pairs feed coordinate arithmetic for four map samples. Heading-derived
coefficients are scaled by `+32768.0` and `-32768.0` at `004702D3/004702E4`
(constants `005205A4` and `005274B0`). Individual products use signed
arithmetic shifts by 15, then combine with the caller's position words.
The map lookup wraps to 16 bits and selects bits 8 through 15 of each result
to address the 256 by 256 byte map. Preserve raw signed sample words; this
does not justify naming them world-space positions, assigning a universal
sample Q-format, or replacing the per-product shifts with one final shift.

Slot 1 has a one-byte stored-row stride per X coordinate and a row stride of
`width` (`00470E91..00470E94`). Although the original instructions sign-extend
the byte, the consumer tests its four low bits independently:

- Value 0 takes the complete four-sample path, including its separate noise
  branch selected from live state and RNG.
- Value 15 advances without writing a destination pixel
  (`0047074F..00470752`).
- Bits `1`, `2`, `4`, and `8` suppress the corresponding pair's map sample
  and substitute a contribution from the existing destination pixel.

These are sample-suppression masks, not alpha bytes. Their raw values and
native signed loads should remain visible even though the retail values are
all nonnegative.

Slot 2 advances by one byte per output pixel and `2 * width` bytes per row
(`00470E7E..00470E89`). Its sign-extended value indexes the 16-word table
loaded from Handler-7 global slots `0x62..0x71` at `004702FD..0047033B`.
That color contribution is used by the full-sample/noise branches. It is
separate from the palette generated for the terrain-map samples by
`00471D40`. It is not a sprite-palette index or an assumed alpha channel.

The native routine reflects the slot-0 and slot-1 stored quadrant across X
and Y while using slot 2 directly across the complete doubled output grid.
The bottom-half row calculation at `00470EF0..00470F0C` uses the doubled
**width** in its reflection expression. Every retail grid is square;
rectangular compatibility is not established and must not be generalized
from the two dimension fields alone.

## Metadata observations and exact retail extent

The loader only relocates pointers. The identified sample consumer does not
read record `+0x00..+0x0C` or the `+0x14` table. The following relationships
hold for all fifteen retail records, and can validate this known family.
They are **observed shape checks**, not proof of a generic N-dimensional
array format or universal component-type numbering.

| Slot | Raw words `+00,+04,+08,+0C` | `+10` target words | `+14` target words | `+18` data length |
| --- | --- | --- | --- | --- |
| 0 | `[2,8,W*H,16]` | `[W,H]` | Eight DWORDs, each `2` | `16*W*H` bytes: `W*H` groups of eight signed WORDs |
| 1 | `[2,1,W*H,1]` | `[W,H]` | One DWORD, `1` | `W*H` signed bytes |
| 2 | `[2,1,4*W*H,1]` | `[2*W,2*H]` | One DWORD, `1` | `4*W*H` signed bytes |

Each of the five difficulty OVLs contains three local records registered as
global slots 0, 1, and 2. Their allocation starts with 84 bytes of records,
then each record's dimension words, metadata words, and sample bytes in that
order. The offsets remain authoritative; there is no padding or extra tail
in these fifteen records' regions.

| OVL | `W=H` | Slot-0 bytes | Slot-1 bytes | Slot-2 bytes | Full allocation bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| `1d0.ovl` | 24 | 9,216 | 576 | 2,304 | 12,244 |
| `1d1.ovl` | 38 | 23,104 | 1,444 | 5,776 | 30,472 |
| `1d2.ovl` | 48 | 36,864 | 2,304 | 9,216 | 48,532 |
| `1d3.ovl` | 60 | 57,600 | 3,600 | 14,400 | 75,748 |
| `1d4.ovl` | 76 | 92,416 | 5,776 | 23,104 | 121,444 |

The corpus contains 13,700 eight-WORD samples (109,600 signed WORDs),
13,700 mask bytes, and 54,800 color-index bytes: **287,700 sample bytes**.
Dimension and metadata tables add 320 bytes. These account exactly for the
formerly opaque 288,020 bytes after the fixed record tables. Including
the fifteen 28-byte headers gives 288,440 allocation bytes; the five size
prefixes bring the complete framed regions to 288,460 bytes.

Slot-0 signed extrema across the five banks range from `-15825` to `15825`.
Slot-1's observed values are `{0,4,5,12,13,15}`; slot 2 contains values 0
through 15. These observations do not change the signed native loads.

## The consumer

The [terrain radar](VEHICLE_DRIVE_RUNTIME.md#the-terrain-radar-00470170)
port ([terrain_radar.rs](../../crates/inf-game/src/terrain_radar.rs))
draws `00470170` from these three grids through
`RenderResources::handler10_sample_grid`: its heading and position inputs,
noise cadence, colour table `00471D40`, destination blending and clipping,
checked against the original routine under Unicorn. The map it samples has
no later writer: `00471C40` is called only from the constructor's loop.

Unknown registrations or alternate header/metadata shapes must retain their
raw structural record rather than acquire one of these global-slot roles
by coincidence. Bounds failures and unsupported shape errors are portable
decoder containment, not claims that retail gracefully recovered from the
same malformed allocation.

The authoritative static sources are
resource_handlers.c and
bulk/game_logic.c, with signed loads, strides, and
register-based arguments checked against the original executable at the
addresses above. No new recording was needed to establish these contracts.
