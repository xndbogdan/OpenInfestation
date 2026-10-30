# Infestation map allocation and process markers

This document owns the map constructor `00471AE0`, its paired-marker local
cleanup `00471BD0`, and their bounded production load/retirement composition.
The [runtime](../../crates/inf-game/src/world_map.rs) keeps allocated map
rows and markers in a retained process owner. The mission slot and a stopped
constructor allocation remain explicit. This does not complete `0048DC30`,
`0048E890`, native heap history, map refresh, or global shutdown. The
[terrain radar](VEHICLE_DRIVE_RUNTIME.md#the-terrain-radar-00470170) draws
the process map.

Authority is the unmodified supported `Infest.exe`, SHA-256
`bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956`,
and bulk/game_logic.c. Complete constructor
`00471AE0..00471BC9` is 233 bytes, SHA-256
`ff7959489b18a5f698a94806ca4e004f243aace0085f7ce9a153a5c37f3e78b4`;
local cleanup `00471BD0..00471C33` is 99 bytes, SHA-256
`4098c9c82b9a92ed6bd0efc6436c1d4476349724a95f07f9cff0d3e20c3d23da`.
These are static source bounds, not retail execution receipts.

## Allocation and ordered construction

The caller allocates 40 bytes and passes that receiver in ECX. The constructor
ignores its first DWORD argument and consumes the low byte of the second.
It seeds distinct local head `+04` and tail `+10` sentinels, writes their NULL
payloads and exterior links, then initializes BYTE `+1C`, DWORD `+20` and
signed jitter DWORD `+24` to zero. Bytes `+1D..1F` have no constructor write;
the port does not pretend the entire DWORD at `+1C` was cleared.

It captures the H12 slot-array pointer, writes raw process plane BYTE
`0055ED88`, then reads actual H12/0. The canonical pool virtual `+0` on a NULL
slot is `004EFBF0`, which returns NULL; there is no successful lazy load and
no subsequent NULL bypass. A required-terrain boundary therefore retains the
local/scalar/plane prefix and the pending allocation. The old mission slot,
index grid, RNG and marker memberships remain unchanged at that boundary.
Fresh source authentication checks both registration and retained overlay
allocation before using prepared terrain; a replaced allocation never becomes
an alternate source for the same preparation.

The complete known-terrain loop then advances the caller's actual shared RNG
65,536 times, X outside Z, through `00471C40`. Its byte array at
`0054ED78..0055ED78` belongs to the process, independently of any map allocation.
The existing [terrain-map arithmetic and native receipts](TERRAIN_RUNTIME.md#initial-terrain-map-indices-00471ae000471c40)
own signed heights, shade/material policy, plane shifts and storage order.
The port publishes the completed intrinsic snapshot into that process array
under an exclusive, infallible cell-loop domain. This proves output and RNG
order, not physical byte-store cadence under arbitrary memory faults/aliases.

After the loop, the native successful marker-allocation arm writes its two
self-payloads and local-owner pointer, inserts local links first, then rereads
the process tail predecessor and inserts global links. The allocation has
two independent doubly linked memberships. A reached global-link error must
preserve the completed grid/RNG and local insertion; no preflight, rollback,
list clear or retry repairs that prefix. Native marker malloc returning NULL
skips registration; that allocator-failure arm is outside the port's successful
typed allocation domain, and is never fabricated to bypass linkage.

Only constructor return permits the caller's `0048DE41` mission `+04` store.
`MapLoadState` owns that publication and a port-only pending constructor stop;
`WorldSession` captures the published allocation identity. There is one actual
jitter leaf in the retained row. Briefing drawing and gameplay decay borrow
that same leaf, rather than copying it into a second world-owned value.

## Paired lists and local retirement

CRT `0044FCF0` seeds the process two-sentinel header at `00548398` once. It is
shared: `00435777` registers a genuine compatible local owner at `00546E18`
through `00437A90`. A map-local cleanup must preserve such foreign memberships,
other maps, process plane/grid, and allocation history. Screen, display and
world replacement never rebuild this process header.

Local cleanup starts at the actual local head successor. For each selected
node, `00471BF1` captures its successor, `00471BF7` reads its payload and
`00471BFA` qualifies the successor's next link before the deletion branch.
A nonnull marker payload unlinks global-next.prev, global-prev.next,
local-next.prev, local-prev.next, then frees that exact marker. Traversal uses
the captured successor. A NULL payload advances without freeing. Typed backing
or link errors retain every earlier unlink/free; corrupt raw pointers and
unmodeled allocator reuse are separate domains, never successful empty drains.

In `0048E890`, actual mission `+04` NULL bypasses the map phase. Otherwise,
local cleanup, map free and slot NULL precede BYTE `+12`, `00450210`, player
vehicle/message work and manager `00478CD0`. Its three callers are
LeaveWorld `004871C9`, PrepareResults `00487550`, and mission destruction
`0048DAD9`. The session consumes this local phase before its existing
results/leave effect clearing. Host-free shutdown also consumes that bounded
mission-local phase after screen teardown; it does not claim the earlier full
game-destruction sequence or CRT global drain `0044FD30`.

`0048DC30` itself does not call `0048E890`. It publishes the new map before
manager cleanup at `0048DE5D`. A direct constructor can overwrite the slot
while the earlier map and its memberships remain retained; the port neither
silently retires them nor drops their backing. Natural results/leave callers
clear their current map first. Arbitrary repeated direct loads do not establish
complete campaign retirement or eventual global ownership of those old rows.

## Consumed production boundary

The offline loader constructs after mission/header preparation and before
placement reset/construction. The process store and publication carrier live
on `PortSession`, so constructor stops and later backend/music failures cannot
orphan registered markers. Existing eager `WorldPlacements` preparation can
fail before this constructor is reached; it is not evidence of the constructor's
missing-H12 prefix. The core tests exercise that reached read separately.

The backend receives a mutable caller RNG loan, rather than a seed copied
into a success-only local. Existing world callbacks temporarily retain that
word in `WorldSession`. Explicit port custody hands it back once before a new
load; failure leaves the platform authoritative, so retry or prepared-world
clearing cannot overwrite reached advances with the old world's snapshot.
No seed is restored to preserve an older rendering/test result.

The [offline world lifetime](WORLD_LOAD_LIFETIME.md) composes this map phase,
[ordered vehicle copy](PLAYER_VEHICLE_STATE.md), captured player messages and
[manager cleanup](WORLD_CLEANUP.md) against retained arena/input owners.
`SavedImport`/`CleanImport` fixtures and the remaining first-load parameter
policy in `IntroCampaignReloadApproximation` stay explicit. Attract-demo
playback, arbitrary campaign reload and retail acceptance remain separate.
