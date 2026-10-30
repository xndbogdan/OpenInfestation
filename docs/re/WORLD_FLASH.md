# World flash and retained draw allocation

The shared death call at `004242E7` and furniture camera callback `004375D0`
(radius `F00h`, style zero) invoke `00487D70`; world draw consumes
its retained sequence/color at `004881B8..00488326`. The owners are
[`world_flash.rs`](../../crates/inf-game/src/world_flash.rs), the
[`SoftwareScene` bridge](../../crates/inf-game/src/software_scene/flash.rs),
and the ordinary world draw immediately after pending-particle transfer.
This is independent of the per-actor hit flash at `0047DEA0`.

Twelve calls in eleven functions reach `00487D70`:

| Call | Caller |
| --- | --- |
| `00421678` | The vehicle's damage `004212F0` |
| `004242E7` | The shared death `00424280` |
| `00437637` | The furniture camera callback `004375D0` |
| `00456B2E` | Selector 25's lifecycle state 1 `00456B10`, for a viewed missile |
| `00457AE9` | Selector 25's player-steered flight `00457800` |
| `00464131`, `00464627` | `00464050`, unsearched |
| `004765DC` | The terrain update `004765A0` |
| `0047FB58` | The update `0047FAC0` of the family table `00527CD4` |
| `0048C2A7` | The gate pass `0048BE70` |
| `0048D9B8` | The Detonator's pulse `0048D980` (upgrade `0x26`) |
| `0048E4C0` | The mission clock `0048E2E0` |

## Trigger

World construction `00487C4C` initializes sequence `+5B8` to zero and leaves
color `+5BC` unwritten. A NULL input position bypasses camera acquisition.
Otherwise `00478E00` resolves/creates the actual camera; its `+48` supplies
the current basis. The signed low WORD radius multiplies the forward row,
with x87 truncation to the low WORD. Camera addition and source subtraction
wrap as WORDs. The distance is the signed low DWORD sum of three squares,
compared with the signed low DWORD square of the full radius argument.

An accepted trigger writes sequence one, or four for any nonzero full style
argument, before H6 lookup. Zero style becomes 14. H6 consumes a signed low
WORD index, while an actual lazy supplier would receive the full DWORD.
The returned style DWORD becomes `+5BC`. A style failure retains the earlier
sequence write and previous color. The NULL-camera path requires unwritten
native stack coordinates and remains an explicit typed boundary.

## Deferred draw

Nonzero sequence reads the signed low WORD at `0053C684 + sequence*4` as the
H3 sprite. Renderer sorting runs after lookup. A masked sprite or selector
FF gets C0000000; an opaque sprite uses the existing texture identity policy.
The actual packet is callback `0040EF30`, three payload blocks, four screen
corners, depth 40, texture at payload `+24`, and retained color at `+28`.
The normal suffix increments the current sequence and resets it when the next
full DWORD table entry is zero. A failed submission does not increment it.

The sequence's sprites are 2x2 images of one palette index (OVLs `1D0`..`1D4`;
`47` holds 555): 554 opaque `7FFF`, then 555 three times and 556..559
additive (`7FFF`, `4631`, `39CE`, `294A`, `14A5`). A style-zero flash is
one white frame, three additive white frames and a four-frame fade.

**Payload `+20` is never written by this producer.** Pixel consumer
`004D78A0` reads that DWORD. Neither its native allocator `004A42B0` nor
rewind `004A4400` clears the arena, so the word is whatever an earlier
packet left there. The port does not assume mode zero or read a fixture
value as runtime behavior. The shared queue's optional byte-provenance
owner supplies the word when earlier admitted stores establish every byte.
When they do not, none of the consumer's reads changes this quad:

| Read | Bits | Use | The flash quad |
| --- | --- | --- | --- |
| `004D78AD` | 1 | Sprite selector byte | 554..559 select descriptor 0 in both modes |
| `004D78FD` | 100 | TEXTUREPERSPECTIVE | One texel color, one depth (40) |
| `004D8568` | 4000 | Fan diagonal | A rectangle of one color and depth |
| `004D8568` | 20 | Skips clipper `004BCCE0` | Corners on the inclusive clip rectangle |
| `004D874E` | all | Descriptor-0200 secondary pass | Flags 4 and 14 never take it |

The clip rectangle is the display's (`004A49FF` stores `+4C/+4E` from its
dimensions), and the quad's corners are the view's dimensions, so
`004BCCE0`'s identity path returns every fan unchanged; it takes the whole
word but reads it only past that path. Effect quads carry `+20` as an
`Option`, and
[`settle_effect_mode`](../../crates/inf-game/src/d3d_effect.rs) admits an
unknown word only for a near quad with both selectors equal, a single-color
texture, one positive depth, a convex outline and every corner inside the
clip bounds. Such a quad draws under 4120, and a test draws it alike under
all 16 combinations of the four bits. The flash checks this before it
enqueues, so a quad the word would change still stops the flash feature
without stopping unrelated rendering. The world's arena, though, is
`malloc(0xC8000)` (`004A43A0`), above the Windows heap's `0x7F000` threshold,
so it comes from fresh zero pages and the packet's `+20` reads 0 there
([`FRESH_PAGES_THRESHOLD`](../../crates/inf-game/src/draw_queue/memory.rs)):
the flash draws mode 0. Only a smaller arena keeps the word unknown, and
there later packets never read a word the port chose. The gate pass calls the flash at `0048C2A7`; the port's surface gate
in world 23 now flashes white in both renderers and under widescreen.
Infest01 cannot supply the word: neither trigger (`004375D0`, `004242E7`),
nor `00487D70`, nor the draw's allocation return `0048828D` ever executes
in it (counts).
The port's fixed-step intro smoke reaches the flash only after its
125 ms steps have diverged into furniture destruction.

## Draw allocation decision matrix

[`draw_queue/memory.rs`](../../crates/inf-game/src/draw_queue/memory.rs)
tracks the same single native arena and its existing aligned front/back
cursors. It does not invent native heap addresses.

| Operation | Retained-byte policy |
| --- | --- |
| Ordinary generic `DrawQueue::new` | No optional byte recorder; original typed queue behavior |
| New/replaced arena, `004A4320/004A43A0` | Fresh malloc bytes unknown; no presumed zero/reuse |
| Rewind `004A4400` | Reset cursors and lists; retain byte custody |
| Pending allocation before capacity check | Pointer word invalidated; sorted depth DWORD written even if capacity fails |
| Successful packet allocation | Consumer's actual known header/payload stores overwrite prior bytes; untouched bytes survive |
| Relocated pointer writes | Invalidate four bytes rather than serialize a fabricated address |
| Group `004A4740` | Exact header, one sorting byte, and unknown native pointer fields; padding survives |
| Unsorted close `004A44CB` | Copy the complete pending cells, including their 12 padding bytes |
| Sorted close | Write pointer DWORD per output cell; preserve its remaining bytes; touched scratch key/pointer DWORDs conservatively unknown |
| Pixel callback success or failure | Preserve reached packet mutations before propagating failure |

Sorted scratch permutations are deliberately not reconstructed by this byte
owner. Unknown bytes may therefore withhold a flash quad that the word
would change and a fully owned heap could admit; the retail flash sprites
are not such quads. Allocation replacement also loses any prior allocation custody;
heap reuse across separate malloc calls remains the allocator follow-up.
The existing fresh-actor allocation approximation does not supply draw bytes.

## Evidence bounds

The tracked generator reproduces the receipt;
the exact replay command and dependency requirements are in
NATIVE_RECEIPTS.md.

[`world_flash_native.json`](../../crates/inf-game/tests/fixtures/world_flash_native.json)
contains 15 unmodified trigger segments and nine draw segments from the
authenticated BF09 executable. Camera/sorting returns are explicit supplied
boundaries; the draw receipts execute the actual `004A42B0` allocator over
declared incoming bytes. They pin unknown-mode carry, colors, sorting, sequence
termination, signed arithmetic and callback-side position changes. They do not
establish the retail arena's initial heap contents.

Focused tests additionally exercise an actual star packet writing mode zero,
the same arena rewinding, and flash reaching the existing pixel consumer.
Over fresh bytes, the resident sprite 554 flash fills a 640x480 display with
its one texel and leaves the next allocation's `+20` unknown; under a clip
rectangle one pixel short it still stops before enqueueing.
Fresh/replaced bytes remain unknown. Queue tests cover partial writes, failed
pixel mutation, pre-failure allocator scratch and sorted/unsorted padding.
These are kernel/composition checks; cinematic visual acceptance still needs
the matched retail comparisons owned by the active objective.
