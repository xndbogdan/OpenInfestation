# World mission state

[`WorldMission`](../../crates/inf-game/src/world_mission.rs) owns the represented
runtime fields of mission W = game G+1B0. Saved selection W+30, current runtime
world W+A0, start ordinal W+A4, session phase IDs, overlay leaves and resource
slots are distinct. The [save format](SAVE_FORMAT.md) owns serialized shape;
the [world load lifetime](WORLD_LOAD_LIFETIME.md) owns the enclosing retained
arena/player sequence.

## Native mission constructor

Complete original `0048D9F0` constructs the mission, allocates its statistics
record, installs the final vtable, resets the saved state, then calls
presentation-global initialization `0043A370`. The represented
successful-allocation projection is
`WorldMission::native_constructor`; it does not assume the rest of an original
heap allocation is zero. The supported executable SHA-256 is
`bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956`.
The complete `[0048D9F0,0048DAC9)` body is 217 bytes, SHA-256
`9d82f14271de24a96689483bbb4549568c906c82d605422933d11da06f7f21a6`.

| Represented field | Constructor source |
|---|---|
| W+08 inhibition | Zero at `0048DA1A` |
| W+0C statistics record | Allocate `0x158` bytes and construct/clear through `00480FC0`; allocator NULL is a separate native branch |
| W+12 / W+13 | Literal 0 / 1 |
| W+14..16 | Three zero BYTE stores at `0048DA57/5A/5D` |
| W+1C / W+20 | DWORD `0x2800` visibility / `0x400` |
| W+24 / W+28 / W+2C | Zero DWORD / WORD / DWORD; W+2A stays unwritten, until a [gate](SELECTOR30_RUNTIME.md#update)'s `0048B411` writes `0x119` there with its `+164` in W+28 |
| W+A0 current world | Literal DWORD 1 at `0048DA88`; EAX already holds 1 from `0048DA3F` |
| W+A4 / W+A6 | Zero WORD / BYTE |
| Saved mission and progress | Call `0048EBA0(22, Clear)` at `0048DAA7` |

The pre-reset W+30 value is not the completed saved selection. Shared reset
sets selection 22 and zeroes every progress array, including start ordinal;
elapsed is cleared because the native argument's low BYTE is nonzero. See
[the complete reset policy](SAVE_FORMAT.md#shared-mission-reset). Fields such
as W+38 that this constructor does not establish remain unavailable until
their genuine later writer. Portable record allocation owns the represented
successful statistics backing; it does not reproduce native allocation NULL,
heap-fault, unwind or physical padding behavior. WORD W+10 is also unwritten.
The later `0043A370` presentation-global initialization remains a separate
owner; the represented mission constructor does not claim that complete call.

## Load and retained runtime fields

`0048DC30` begins with mission BYTE+A6 clear, previous-world status update,
DWORD+38 clear and saved selection copied into current+A0. The previous-world
entry tests only the low BYTE of status bit 1 before ORing bit 2. The port
contains out-of-range indexing explicitly; native neighboring-field accesses
are not normalized into valid worlds. The mission prelude precedes activation
of leaf 48 and selection plus 10 in the retained backend.

The reached H14 header phase clears W+14..16, sign-extends WORD+0C into W+24,
conditionally clears W+13 on flag `0x1000`, clears W+28/+2C, then computes W+20
from signed header distance and three quarters of current visibility. W+1C
retains the [visibility/timing owner](WORLD_VISIBILITY.md) across loads. The
loader tail performs its signed WORD division into W+10, clears a present
statistics record through `00480FD0`, then stores W+12=1. A zero divisor keeps
the reached prefix as an explicit error.

`0048EA80` reads four completion DWORDs from W+240 for the current world.
`0048EAD0` records placement death using the live W+08 inhibition BYTE, explicit
force BYTE, signed quotient and raw low five index bits. Shared
[manager cleanup](WORLD_CLEANUP.md) sets inhibition before its walk and clears
it only on normal return. World reset, header/load writes, fade, completion,
statistics and placement bookkeeping therefore share this actual mission;
they do not maintain a duplicate saved or runtime field cache.

### The arena worlds

Worlds 25 to 33 are the network arenas (`0053C570`), past the 25 entries of
each per-world array, and retail indexes them unchecked:

- The status `W+3C + 4 * world`, which `0048DC30` tests for the world being
  left and the briefing (`0040C6A0`) for the current one, low byte only: 25
  reads `W+A0` itself (25, bit 1 clear), 26 the start ordinal `W+A4` (`+A6`
  is cleared first; a set bit 1 adds 4 to the ordinal), 27 and 28 the start
  override's X and Z words `W+A8`/`W+AC`, and 29 to 33 the first world's
  pickup words `W+B0..W+C0`. `0048C5FF` sets `+A6` only with fresh override
  words and only `+A6` lets `0048DAF0` read them, so leaving 27 or 28 marks
  nothing that is read; the port, which drops those words with `+A6`, stops
  if anything else reads them. The arenas' headers carry no briefing records
  (`+28` is 0), so their briefing finishes before it reads the status.
- The pickup words `W+B0 + 16 * world` are the completion words of worlds 0
  to 8, inside the span the loader passes.
- The completion words `W+240 + 16 * world` start at `W+3D0`, the loader's
  actor counter (`0048EB90`), and run on into the game object (`G+584`
  onwards: the timing ring, the pickup count, the camera's scale). In a
  network game `0048DC30` skips `0047F6C0`, so no placement takes a counter
  index and each keeps -1, which `0048EA80` still reads: bit 31 of the first
  word. World 26's words are the timing ring's samples 1 to 4
  (`G+590..G+59F`), whose bit 31 a frame's time never sets, and world 27's
  samples 5 to 7 and their average; the loader passes those. The other
  arenas' words are fields the port does not keep, and a reader (a
  placement's `+34`, a marker's finish, a placement's death) stops on them;
  none of their placements has one.

The supplied `WorldMission::new`/`WorldMissionInputs` path remains available for
tests and explicit external state. It keeps those declared inputs distinct
from the complete constructor projection. This ownership does not establish
whole mission destruction `0048DAD0`, game destruction or process shutdown.
