# Infestation save records

The canonical codec is [inf-formats/src/save.rs](../../crates/inf-formats/src/save.rs).
It owns physical redundancy and mission/player serialization. The
[profile runtime](PROFILE_RUNTIME.md) owns directory caching, menu actions,
portable file custody and the deferred name-entry flow. Original `Infest.sav`
bytes are read-only evidence; generated or edited saves belong to a separate
portable sidecar.

## Physical records and recovery

`00449A70` allocates a `0x45C` (1,116-byte) working payload and fourteen
`0x30` directory entries. It obtains the installation path through `00463800`
and constructs the `Infest.sav` filename; the codec does not reproduce registry
or filesystem location policy. `0044A170` scans consecutive profiles:

| Unit | Layout |
|---|---|
| Profile | Nine 256-byte block pairs; `0x900` physical bytes |
| Block pair | Primary 128-byte copy, then backup 128-byte copy |
| Copy | 124 payload bytes, then little-endian DWORD CRC |
| Logical profile | Nine concatenated 124-byte payloads; `0x45C` bytes |

There is no file header. The two `00404840` calls made before reading/writing
are empty in this executable. The UI capacity is fourteen profiles; the
portable codec bounds scanning there rather than reproducing the native
inclusive-index-14 directory overflow.

`00449E00/00449E40` build/use the reflected `EDB88320` CRC table at `00547680`.
Initialize CRC to zero, use
`crc = (crc >> 8) ^ table[(crc ^ byte) & 255]`, and do not apply a final XOR.
The checksum of ASCII `123456789` is `2DFD2D88`. `00449E90` writes two copies
with the same CRC; no padding remains when its payload length is 124.

`00449F50` attempts the primary copy through `0044A010`. A checksum mismatch
raises native error `8700` and permits the backup attempt. Other read failures
propagate immediately. Once any copy succeeds, the next copy is skipped by
seeking over it: the original does **not** compare copies or validate a backup
after primary success. If both CRCs fail, reading fails with `8700`.

After the first block, a zero first name byte stops the directory with `8702`,
without reading the other eight blocks. A failed profile is not included in
the completed directory prefix. Scan mode retains that prefix; direct loading
propagates the error. A seek over an absent final backup can succeed in native
stdio, so directory scanning and physical-file validation are distinct.

`SaveArchive::parse` requires whole physical records, no interrupted/empty
profile, and the fourteen-profile capacity. `scan_directory` separately
exposes `EndOfFile`, `EmptyName`, `Unreadable` and `CapacityExceeded`, the
completed prefix and selected-copy provenance. Even strict import follows
first-valid-copy recovery; it does not require unused backups to match.
`encode_save_records` emits canonical redundant copies for a deliberate
mutation. A verbatim import retains the original file bytes separately.

## Logical payload and serializer boundaries

The profile begins with a variable-length, NUL-terminated byte name. Save
owner `0044A0E0` copies at most 31 bytes; phase-9 name entry limits names to 18.
Mission and player data follow the NUL immediately, without alignment padding.
Names are not fixed eight-byte fields, and the following DWORD is not a score.
This is the intrinsic raw record boundary. The live
[name-as-format callers](PROFILE_RUNTIME.md#profile-name-as-format-proof)
instead select their next field by emitted count plus one; escaped percent
input can make that cursor differ from raw name length plus one. The codec
does not acquire presentation or CRT context.

| Component | Size | Original serializer / deserializer |
|---|---:|---|
| Name plus NUL | Variable | `00449C10` / `00449B80` |
| Mission state | 910 bytes | `0048EC10` / `0048ECC0` |
| Player prefix | 150 bytes | `0046C870` / `0046C9F0` |
| Player nested mask | 24 bytes | `00475040` / `00475080` |
| Unused tail | Remaining bytes up to 1,116 | Retained raw |

Mission serialization order is owner `+30` DWORD mission selection, `+34`
DWORD elapsed seconds, 100 DWORDs at `+B0`, 100 DWORDs at `+240`, 25 DWORDs at
`+3C`, then WORD `+A4`. The codec keeps offset names; [what each field
means](#what-each-field-means) follows from its consumers. The directory
copies the first two DWORDs after the name. `0044A480` displays elapsed hours
and minutes: `seconds / 3600`, `(seconds / 60) % 60`.

Player prefix serialization order is `+100[24]`, `+118[4]`, `+11C[2]`,
`+11E[2]`, `+120[16]`, `+144[4]`, `+148[4]`, `+14C[4]`, `+150[64]`,
`+13C[8]`, `+190[6]`, and `+130[12]`. The nested serializer at player `+9C`
then copies six DWORDs from `+A0`. These are values, not pointers or live
object ownership.

`00449C10` writes into the existing working buffer; it does not write a file.
It ignores the serializers' Boolean results. The mission writer requires
remaining space at least 910; the player prefix requires more than 150; the
nested mask requires more than 24. With a 31-byte name, the mask has exactly
24 bytes available and is **not written**. `SaveRecord::with_state` preserves
those prior mask bytes through the [shared native copy phases](#canonical-state-copy-phases),
including the ignored-failure behavior. At 30 name
bytes the mask is written. `from_state` is the complete intrinsic format
encoder and accepts all 174 player bytes explicitly.

`with_state` retains bytes beyond the new serialization end at their existing
offsets. `renamed_copy` instead shifts the old state and owned tail after a
new name, as required by copy mode in `0044A580`. When shortening a name, native
copy code can read beyond the original record; the portable operation retains
all owned tail bytes and zero-fills only the newly exposed suffix. Longer
names discard only tail bytes that no longer fit. These are explicit memory
containment rules, not evidence of a native zero-filled heap. A new working
buffer's initialization policy belongs to its runtime owner.

## What each field means

Each saved field has consumers that fix its meaning. The
[report](../../crates/inf-formats/src/save_report.rs) names them, and
`inf-extract save <file> [--data-root <installation>]` prints every profile
of a save with grants, hints and weapons by name (the port's `profiles.sav`
has the same layout).

The mission's arrays hold one entry per world (25, indexed by the world's
current `+A0`):

| Field | Meaning | Consumers |
| --- | --- | --- |
| `+30` | The world the next load starts: the loader activates overlay selection + 10 | `0048DC30`; a gate's end stores its destination (`0048E820`) |
| `+34` | Play time in whole seconds | The session clock; the profile menu (`0044A480`) |
| `+3C[w]` | World status bits: 1 left by its gate (`0048E820`), 2 completed (a gate with `+167 = 1`, `0048BE70`), 4 completed and left (`0048DC30` marks the world it leaves), 8 "Mission stage 1 complete" (a gate with another nonzero `+167`) | The results page: "World already completed" with bit 4 (no score), the medal with 2, "Mission stage 1 complete" with 8, else "Mission Incomplete" (`0042B6D0`, `0042BBF0`) |
| `+B0[w*4..]` | 128 bits: the pickup serials collected | `0048E930/80/C0`; the pickup's load disposes a collected one ([pickups](PICKUP_RUNTIME.md)) |
| `+240[w*4..]` | 128 bits: the placements finished, destroyed or (a factory) staffed | `0048EAD0` sets, `0048EB30` clears, the loader skips them (`0048EA80`) |
| `+A4` | The start in the next world's H1 table: slot `2 * A4` holds the packed X/Z, slot `2 * A4 + 1` the heading | `0048DAF0`, `0048DBC0`; a gate stores its ordinal less one |

The TTD write census of the three recordings (`mission-writes.js`, bundle
`.tmp/infestation-ttd/20261011-missionwrites/`) finds only these writers:
the reset `0048EBA0`, the save reader `0048ECC0`, the gate's `0048C3FA`
(bit 2 at Level 1's end) and `0048E85D` (bit 1), and the loader's
`0048DC74` (bit 4). The profiles Infest03 loads carry bit 8 on worlds 4 and
11; the corpus's first Jameson has it on 4, 11 and 14.

The player's fields, all copied by `0046C870`/`0046C9F0`:

| Field | Meaning | Consumers |
| --- | --- | --- |
| `+100[6]` | Every grant ID the player owns, upgrades and collected kinds alike | `0046BDC0`; the grant switch `0046B2B0` sets the bit |
| `+118` | Tokens (grant 7) | Energy Distribution spends them (`0046C6D0`, "Spare Tokens"); the results page's "Tokens remaining" |
| `+11C` | ACAMs lost | `0046A860`; the respawn's loss limit (H14 `+24`); the results page |
| `+11E` | Overall rank: the medals awarded, summed | `0042BBF0` |
| `+120/+124/+128` | The blue, green and red resource crystal tanks, up to 30 | Grants 0xE..0x10 add two per crystal; the shop's costs (`0046C6D0`); the HUD's three counters |
| `+12C` | Turbo charges | Grant 3; the HUD; the drive's turbo |
| `+130[3]` | The hints already shown, IDs 0..72 | `0046C760`/`0046C820`; the computer's List Hints |
| `+13C..+142` | Each Energy Distribution row's level: Cannons, Plasmas, Lasers, Flamethrower, Rockets, Grenade, Shielding (signed bytes) | The shop's token rows (`00430AB0`); Shielding divides damage (`004212F0`) |
| `+143` | Copied with the levels; nothing reads it | |
| `+144` | The ACAM's form (vehicle `+1E8`) | `0046CBB0`; the respawn |
| `+148/+14C` | The selected primary (0..7) and secondary (8..15) weapons | `0046CBB0` from behavior `+10/+11`; the respawn |
| `+150[16]` | Each weapon's count (behavior entry `+0`), -1 unlimited | `0046CBB0`; grants add ammunition (0x1A grenades, 0x1B mines) |
| `+190[3]` | The resource WORD (actor `+30`) of each actor in the hold, 0 empty | `0046CBB0`; the respawn rebuilds the hold |
| nested `+A0[6]` | The grant IDs known: blueprints, database entries and messages | The factories' product lists, the computer's database, `004752E0` |

"Tokens" is the game's own name for grant 7 ("Use these to upgrade your
craft weapons at factories"); grants 0xE..0x10 are the resource crystals.
Every profile of the installed corpus ends with 24 bytes of `0F` past its
state, which saving keeps where they are.

## Canonical state-copy phases

`copy_save_state` takes one request containing the complete known working
payload, mutable DWORD cursor, raw DWORD remaining capacity and an explicit
Mission or Player source. It owns `0048EC10`, `0046C870` and the latter's
private nested `00475040` policy. Each capacity gate interprets its DWORD as
signed, while cursor stores and arithmetic retain DWORD wrapping semantics.
It does not select a player, format a name, acquire a working buffer or write
a file.

| Reached phase | Capacity test | Insufficient-capacity result | Successful copy |
|---|---|---|---|
| Mission `0048EC16/1B` | Signed remaining >= 910 | Cursor becomes absolute 910, false; old cursor/source/output unread | 910 bytes in the field order above, true |
| Player prefix `0046C87D/883` | Signed remaining > 150 | Cursor becomes absolute 150, false; source/output unread | 150-byte prefix, then nested mask call |
| Nested mask `00475040/45` | Signed remaining > 24 | Cursor becomes absolute 24, false; mask/output unread | Six DWORDs, true |

The Player entry captures the old cursor at `0046C87B` before its capacity
test. After each admitted scalar or array block, the cursor advances by that
block's byte size. Its nested capacity is the raw wrapping expression
`old_cursor - current_cursor + initial_remaining` at `0046C9D4..E0`, and
Player propagates the nested Boolean. An insufficient capacity is a native
false return with the specified cursor store, rather than a portable error.
The Player `+120/+124/+128/+12C` span advances after each DWORD at
`0046C8E7..0046C92B`; its 16-byte format grouping is not one cursor advance.

The typed payload excludes raw pointer aliasing and adjacent-memory accesses.
A reached copy outside its physical backing is a range error; storage checks
follow the applicable capacity gate. The Player prefix and nested mask have
separate reached checks, retaining a completed prefix if the later mask span
is out of range. These checks do not establish native fault or alias behavior.

`SaveRecord::with_state` copies name plus NUL, calls Mission and Player with
the current cursor/capacity, and ignores both Boolean returns as `00449C10`
does. At name31 the cursor moves 32 -> 942 -> 1092, then the nested failure
sets it to absolute 24 without changing bytes 1092..1115. At name30 it moves
31 -> 941 -> 1115, including the mask, and retains byte 1115. The complete
intrinsic `from_state` encoder shares the byte encoders but writes every
player field regardless of that runtime capacity policy. Rename retains its
separate body/tail movement rule.

The existing phase-9 prepared-state operation consumes this shared kernel
through `with_state`; its snapshot and portable fresh-buffer policies are
unchanged. The live [working-profile Serialize](PROFILE_RUNTIME.md#live-working-profile-serialize)
calls Mission, then obtains the fresh first player and calls Player only if
one exists. With no Player call, all bytes
after the mission end remain at their original offsets. Source selection and
results attachment remain owned by the
[working-profile wrapper](PROFILE_RUNTIME.md#results-working-profile-wrappers).
The CRT fixture proves its three formatting bodies; it does not execute these
state-copy bodies or the enclosing `00449C10` wrapper.

## Canonical state-read phases

`read_save_state` takes the complete known 1,116-byte source, mutable DWORD
cursor and an explicit mutable Mission or Player target. It owns
`0048ECC0`, `0046C9F0` and the latter's private nested `00475080` phase.
These two-argument `RET8` readers have no remaining-capacity argument or
failure Boolean arm. Completed reads represent the constant `AL=1` with
`Ok(())`; no absolute910/150/24 failure cursor from Copy applies.

| Mission destination | Original store/copy | Completed advance |
|---|---|---:|
| Selection `+30` | `0048ECD2` | 4 |
| Elapsed `+34` | `0048ECE7` | 4 |
| `+B0[100]` | `REP0048ECFB` | 400 |
| `+240[100]` | `REP0048ED15` | 400 |
| `+3C[25]` | `REP0048ED2E` | 100 |
| WORD `+A4` | `0048ED40` | 2 |

Player reads the same ordered fields as its format above. The four
`+120/+124/+128/+12C` DWORDs advance separately at
`0046CA5D/6F/81/91`. The `+13C` eight-byte, `+190` six-byte and `+130`
twelve-byte groups each have one later cursor advance; nested `00475080`
always copies all24 mask bytes. At a31-byte emitted name, the full Player
read therefore ends exactly at1116, including its mask, unlike Copy.

Each reached block checks its actual source span before decoding/mutating
the target and advancing the cursor. `StateReadRange` retains earlier
completed blocks and the current cursor, including the first Mission
scalar at1112 or both scalars at1108. Partial native REP/multi-store faults
inside one block, unknown bytes, physical aliases and raw pointer wrapping
are excluded. These are portable containment errors, not native fault
receipts. No name formatting, player selection, owner replacement or disk
effect belongs to this reader.

Intrinsic `SaveRecord::mission()/player()` share these leaves while keeping
raw-name offsets and their existing infallible complete-record contract.
The [live Restore wrapper](PROFILE_RUNTIME.md#live-working-profile-restore)
instead supplies emitted count plus one over the unchanged original source.
For `A%%B`, its cursor4 differs from intrinsic5; no parser context or body
shift repairs that source. Existing copy, rename and phase9 policies remain
distinct. Original bodies and call order are established by static source;
typed tests do not claim native reader ABI/cadence or whole-wrapper execution.

## Live player save-copy owner

The live `WorldPlayer` retains every field in `PlayerSaveState`, including
the formerly omitted raw spans below. Their constructor zeroes are actual
stores in `00469D60`, rather than a fresh-heap assumption. Names remain offset
names where gameplay meanings have not been established.

| Player span | Constructor zero writer | Save source | Restore destination |
|---|---|---|---|
| DWORD `+118` | `00469ECE` | `0046C8AF` | `0046CA1A` |
| WORD `+11E` | `00469EDB` | `0046C8D5` | `0046CA3F` |
| DWORD `+120` | `00469EE2` | `0046C8E7` | `0046CA52` |
| DWORD `+124` | `00469EE8` | `0046C8FB` | `0046CA62` |
| DWORD `+128` | `00469EEE` | `0046C90B` | `0046CA74` |
| Eight bytes `+13C..143` | `00469F3C`, loop indices 0 through 7 | `0046C97A/983` | `0046CAE3/EE` |

The eight-byte span is opaque save data. Conversion to codec `owner_13c`
uses two explicit little-endian DWORD groups and preserves embedded NULs,
high bytes and the entire tail. This copy does not interpret it as an identity
string or extend the adjacent command mask. The damage-reaction host's signed
six-byte consumer remains a separate reached service boundary.

`ObjectArena::export_player_save_state` reads one actual player into the
existing complete data shape. `restore_player_save_prefix` copies every
destination in original order, including `+12C` as `owner_120[3]`, then the
nested grant mask. Neither operation changes handles, checkpoints, message
state, other players or actor/list custody. Missing player identity is an
error before mutation. This data copy does not execute native serializer
capacity/cursor/Boolean-return policy, a whole constructor or later gameplay
writers.

Campaign reload uses the same export, with its existing stored `world.player`
policy; it no longer overlays fields over a retained save template. The
future working-profile wrapper has a different source rule: fresh first-player
lookup after mission serialization/restoration. Those rules remain distinct.
Restoration still precedes loader player setup, which resets `+11C`; respawn
subsequently writes `+12C = 3` when `+100` bit 3 is set, otherwise preserving
the restored DWORD. The named
[intro reload approximation](INTRO_SEQUENCE.md#results-screen-construction)
still rebuilds the arena and does not retain original actor handles.

## Mission selection and new-player evidence

### Shared mission reset

The complete `0048EBA0..0048EC07` body stores the full selection DWORD at
mission `+30` (`0048EBA6`) and zeros WORD `+A4` (`0048EBAA`). Its 25-iteration
loop (`0048EBC9..0048EBED`) always clears all 25 DWORDs at `+3C`, 100 DWORDs
at `+B0` and 100 DWORDs at `+240`. Only elapsed DWORD `+34` is conditional:
`0048EBF2` loads the incoming argument's **low BYTE**, `0048EBF9` tests it,
and `0048EBFD` clears elapsed when that byte is nonzero. A raw argument
`0x00000100` preserves elapsed; `0x00000101` clears it. Neither choice preserves
any progress array or `+A4`.

The canonical [`MissionSaveState`](../../crates/inf-formats/src/save.rs)
owns this complete save-field phase through `reset`, consuming
`MissionResetRequest { selection, elapsed_policy }` with
`MissionElapsedPolicy::Preserve` or `Clear`. Existing `new_game()` consumes
selection 22 and `Clear`. The native call at `004873DF` uses the current
selection and Preserve; `004874B3` uses
the selected offline/attract/network value and Clear; mission constructor
`0048D9F0` calls it at `0048DAA7` with 22 and Clear.

This reset changes save fields only. Current world `+A0`, visibility and
other retained [`WorldMission`](../../crates/inf-game/src/world_mission.rs)
fields remain independent. It does not clear `SessionState` elapsed seconds
or its subsecond remainder, construct a player, establish attract timer/handle
backing, or attach the initial `00487410` preparation branch.

### Mission loader and initial player bounds

The loader `0048DC30` reads `+240 + A0*16` through `0048EA80`, where runtime
`+A0` is not serialized: its prelude clears `+A6`, sets bit 2 of the previous
world's `+3C` word when that word's low byte has bit 1, clears `+38`, and then
copies `+30` into `+A0`. The recorded first offline load entered with `A0 = 1`.
[`world_mission`](../../crates/inf-game/src/world_mission.rs) owns these
load-time fields.
Ordinary offline `00487410` reaches the shared reset with selection 22 and
Clear after its separate player/armed prefix. The phase-3 loader `0048DC30`
activates selection `0x30`, then **mission selection + 10**, before constructing
the world. This is direct source mapping, not an inferred campaign lookup table.
An arbitrary mission value does not establish valid world state or supported
gameplay. Resource activation alone does not implement the loader.

Earlier investigation notes reported that patched selections above 23 displayed
`Caradion` and then crashed, and that edits did not reach the proposed developer
range 33–43 or multiplayer range 24–32. They labelled the mission-1 samples
completed games, mission 11 Artoria, and mission 2 Terra Nova. These historical
observations have no linked matched-retail receipt here and are retained as
unverified evidence. They do not establish a campaign lookup table: the recovered
loader directly activates `mission+10`. A crash after editing one DWORD cannot
separate resource selection from incompatible saved state or later world owners.

The saved player fields recovered from `00469D60` include its explicit zero
stores, `+144 = 1`, `+14C = 8` and the first DWORD at `+150 = FFFFFFFF`.
Its nested constructor `00413140 -> 004750C0` grants IDs
`01,48,04,83,05,06,76,0D,0A,7A,7C,4E`
(hexadecimal), in that order. `004750C0` also recursively grants IDs `95..AE`
when any of three nonzero dependency WORDs is already owned. `00475320` maps
those IDs to 16-byte records at `0053B088`, with dependencies at `+8,+A,+C`.
The native `004750C0` loads these as WORDs and `004752E0` queries them as
unsigned 16-bit IDs; high-bit values are not negative dependency sentinels.
The canonical [`grant_table`](../../crates/inf-formats/src/grant_table.rs)
owns that table and incremental closure: ID 0 always reads owned and out-of-mask
IDs are explicit errors instead of native out-of-bounds accesses.
`00475350` merges into an existing owner by testing source IDs `0..AE` in
ascending order and granting each owned ID through the recursive operation.
It neither copies source bits above `AE` nor clears existing target bits.
The installed executable's exact recursive closure adds no IDs; the
resulting six DWORDs are `00002472,00000000,00004100,14400000,00000008,00000000`,
pinned by a corpus test rather than assumed. `PlayerSaveState::new_player`
assembles the serialized prefix constants around this completed mask; it does
not perform the native constructor's input acquisitions, live grant phases or
later player FIFO publication. `00469E6A`/`00469E75` pass player `+88` to
`00413140`, which supplies that actual message receiver to `00475020` before
the initial grants. `00475027` binds it at grant-owner `+1C`: offline does
not imply a NULL recipient. The constructor's suppression BYTE is 1, but its
message gate follows the reached grant record/text/format phases. Those live
effects retain the [`grant`](GRANT_RUNTIME.md) and
[`player-message`](PLAYER_MESSAGES.md) owners. `PlayerSaveState::default`
is simply a zero data value and does not claim to construct that player owner.

## Native owner fields and errors

Save owner `+00` is initially false and set at `0044A580` write attempts; it is
not a profile-existence flag. `+28` is directory-cache validity, `+2C` count,
`+30` selected index and `+44` query status. The desktop media-change function
`00449C80` returns false, and `00449DF0` assigns status 3 regardless of its
argument. Menu status branches do not authorize invented mappings from
portable filesystem errors.

The accumulator at `+34` must not be presented as a portable CRC sum.
`0044A010` stores a read CRC into its incoming `FILE*` argument's stack slot;
on a skipped copy that slot still contains the pointer. Therefore ordinary
first-copy success makes `00449F50` return a file pointer on its second pass.
The read-side accumulator includes `abs((value as i32) >> 6)`, with wrapping
addition. The codec exposes neither invented addresses nor a replacement
token. Native format-string and unchecked name-copy bugs are also contained.

## Installed corpus receipt

The read-only corpus test checks typed re-encoding and canonical record round
trips for the current `Infest.sav`, and explicitly skips when this optional
user file is absent. It never writes the retail file. For the known SHA below
it additionally verifies 9,216 bytes, both copies of all 36 pairs, all 72 CRCs,
an exact byte round trip, and these four directory entries:

| Name | Mission selection | Elapsed seconds |
|---|---:|---:|
| Jameson | 1 | 14,090 |
| Jameson | 11 | 5,033 |
| BOBSTER | 1 | 2,634 |
| Jameson | 2 | 1,079 |

Each record uses 1,092 logical bytes and retains 24 bytes of value `0F`.
SHA-256: `83e86f73811d0a8f337464780e6d6cc9499d59080fe09c9956530061ed8514d9`.
These are structural/serialization checks, not proof that all saved gameplay
owners are implemented.
