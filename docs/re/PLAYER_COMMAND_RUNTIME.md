# Infestation live player commands

This authority owns player-command Set `0046C760` and Query `0046C820`.
The [live grant kernel](GRANT_RUNTIME.md),
[player-message gate](PLAYER_MESSAGES.md) and
[save format](SAVE_FORMAT.md) retain their separate contracts. Complete
intro and briefing acceptance belongs to
the Infestation objective.

## Actual storage and units

Commands occupy three DWORDs at player `+130/+134/+138`. Constructor
`00469D60` clears ECX at `00469F93` and stores these zeroes at
`00469F97/00469F9E/00469FA4`. This is distinct from the six grant words at
`+A0`, six `+100` words, `+12C`, eight bytes `+13C..143`, and
`+144/+148/+14C`. No additional command words are supplied by adjacency.

Both bodies interpret the full command DWORD as signed i32 for the quotient:
division by 32 truncates toward zero. The bit is `1 << (raw_id & 31)`, matching
x86's masked shift count. The represented triple admits nonnegative IDs
0 through 95 and signed IDs -31 through -1, whose quotient is zero. For
example -1 selects word 0/bit 31; -31 selects word 0/bit 1. ID -32 reaches
native `+12C`, while 96 reaches `+13C`; their physical aliases are separate
diagnostics. Bare PlayerId retains the existing list-local identity policy.

Query reads the reached actual word, including for ID zero. It returns AL
truth with RET 4. Zero is not implicitly owned, and the ID is never reduced
to LOWWORD. An unknown player is required before a represented-word boundary
can be reported. Query consumes no process, executable, H0 or message inputs.

## Set and retained prefixes

Set returns void with RET 8. Its order differs from grant Set:

1. Read the actual full process network DWORD at `0055F448`, before player,
   command or silent consumption. Any nonzero value, including `100h`, returns.
   Missing process therefore precedes even an owned-bit or unknown-player path.
2. Invoke Query on the requested player and full command DWORD. An owned bit
   returns before silent, text, H0 and message inputs, but after network.
3. Freshly read the word at `0046C7A4`, OR the bit and commit the complete
   DWORD at `0046C7AD`. Every later failure retains that store.
4. Read LOWBYTE of the silent DWORD at `0046C7B4`. Nonzero returns. Thus
   `101h` is silent while `100h` continues; the raw high bytes are retained.
5. Read the text WORD at `0046C7BC` using the original full raw command ID.
6. Capture the canonical H0 result, then invoke the same requested player's
   actual message subobject `+88`, with flag DWORD 1.

Retrying after a retained-bit failure takes the owned-bit return, still
requiring current network. No operation reports a changed-bit count from
incidental EAX. Current phase, RNG, process NULL and registry state do not
substitute for the reached network owner.

## Independent text-table boundary

The address is `0053A27C + wrap_u32(raw_id * 2)`. Only the fetched WORD is
interpreted as signed. The original table has 73 WORDs, IDs `0..=48h`, from
`0053A27C` through `0053A30D`. Its authenticated SHA256 is
`e400339c64e0b775e6d68f3a0476cd56ce175da3b016948a76f17f3bf1bdca5e`.
Caller `00409E89..00409E94` admits signed `0 <= WORD < 49h`; menu query
`00414632/00414658` has the corresponding `48h/49h` wrap.

Original mappings include `0 -> 2B4h`, `1 -> 2B5h`, `37h -> 2EBh`,
`42h -> 2F5h` and `48h -> 366h`. ID `41h -> 170h` and reordered entries
disprove a synthetic `692 + id` rule. Every original entry is positive.
The adjacent WORD zero at `0053A30E` is alignment backing before upgrade
limits; it does not add a command-text entry.

Silent or already-owned IDs 73 through 95 can complete within the command
mask without text. A fresh nonsilent ID outside the text domain commits its
bit, then requires the executable before reporting adjacent table storage.
Negative raw command IDs can likewise reach a preceding table address after
their valid word-zero commit. No preflight text bound may change that order.

## Shared canonical H0 capture

The inline capture policy matches the grant caller's complete proven phase.
A signed-negative text WORD yields NULL before requiring residency.
For a nonnegative index, require the current actual capacity. Unsigned
`index > count` yields NULL; equality reaches one-past storage and remains a
distinct boundary. An occupied slot captures its current registration and
immutable allocation, resolving handler region/local StringEntry and its
complete NUL record. Acquisition itself does not scan the text bytes.

An in-range hole uses final H0 table `00527598`, published at `004740E4`.
Its virtual `+0 = 004EFBF0` is exactly `xor eax,eax; ret 4`: a constant-NULL
callback consuming neither receiver nor supplied argument. Canonical capture
does not invoke a loader or substitute an empty string. Other resource
consumers retain their own missing-slot and callback policies.

At `0046C7BC`, MOV SI replaces only ESI's lower WORD. The native miss argument
therefore retains the command's original high WORD. That physical operand
remains ABI evidence; canonical `004EFBF0` does not consume it, and portable
player IDs or source slice addresses must not fabricate it.

The private shared H0 phase takes only the index WORD and borrowed residency.
Record/category selection, process, message identity, suppression and CRT
data remain caller-owned. Grant Set captures record before category, formats,
then rereads its normal `+1C` recipient and sends flag 0. Command Set captures
one table-selected source, then uses the requested player `+88` and flag 1.
It executes no formatter, NULL substitution, locale lookup or first-player
selection. The source remains borrowed through the synchronous call; later
requests acquire fresh current residency.

Only the reached message gate reads suppression. A nonzero suppression byte
returns void without consuming caller text or flag. Enabled zero stops before
unowned message byte `+3D`; the earlier command bit remains committed. Missing
text or H0 cannot be skipped by inspecting suppression early.

## Constructor and persistence

Canonical PlayerSaveState already owns `owner_130: [u32; 3]`. Original save
`0046C9BB/0046C9BF/0046C9C5` exports the triple, and restore
`0046CB23/0046CB28/0046CB2E` imports it. The runtime command owner uses
both existing directions: restoring the triple in `restore_player_save_prefix`
and exporting the live triple in `WorldSession::offline_reload_request`.
No save-format expansion or duplicate mask is needed. Message, checkpoint,
handles, grant words and other saved-prefix fields retain their contracts.

So a one-time hint stays shown across a save and its load: the mask is the
"hints seen" list the save carries, and only hints seen after the last save
come back after reloading it. `hints.js` (bundle
`.tmp/infestation-ttd/20261013-hints`) lists every command call, every
write to the mask and the loads, exports and restores in all four
recordings. Infest01 and Infest02 each run one player through their worlds
and saves; its mask only gains bits (`0046C7AD`), and each world change
exports it with no restore. The seven loads from the profile menu in
Infest03 (3) and Infest04 (4) each construct a fresh player, whose
`00469F97..00469FA4` zero the mask, then restore the loaded profile's
(`0046CB23..0046CB2E`), so a hint the profile had seen is not shown again;
the lock hint `0x21`, for one, shows in Infest04's first load, whose profile
had not seen it. Nothing else clears the mask. The port's load restores it
the same way, so it shows the same hints.

Original constructor `00469D9D` separately writes BYTE player `+58 = 1`,
before its `+5C` subobject constructor. Loader reset `0046A889` writes 1 again.
The portable constructor's previous None/no-earlier-writer interpretation is
disproved by that instruction; the bounded correction is Some(1), preserving
the optional field type. Its later `0046A0E0` attachment-update consumer and
enclosing `00478F20` update remain separate behavior owners. `+58` is not
serialized in the player prefix and does not gain an invented save field.

The existing live DWORD `+12C` uses `owner_120[3]`: original save reads it
at `0046C91B` and stores all 32 bits at `0046C923`; restore reads that saved
word at `0046CA83` and writes `+12C` at `0046CA86`. The runtime imports and
exports that fourth word through the
[complete live player save-copy owner](SAVE_FORMAT.md#live-player-save-copy-owner),
which also retains the first three raw words. No signed conversion or format
expansion is needed. Load still restores before respawn: `0046A945`
subsequently writes 3 when bit 3 of `+100` is set, otherwise preserving the
restored DWORD.
This does not admit the command -32 alias or extend command storage.

## Residency and verification boundaries

Boot selects leaves 0 and 2. Entering the narrow occupied-source profile with
Activate 48 unloads 2, loads 1 and 48, and retains final leaves 0/1/48.
Direct Activate 1 additionally selects leaves 11 and 6. These are authored
scene-tree operations, distinct from H0 registration order. Current leaf1
Language contains 180 original NUL records, mapping global slots 692..871;
command 48h uses local 178. A finite asset declaration must preserve released
bank provenance without treating its slots as current registrations.

The shared [command API](../../crates/inf-game/src/object_runtime/player/commands.rs)
takes an explicit Query or Set request, actual player identity and borrowed
process/executable/residency context. The private
[H0 phase](../../crates/inf-game/src/object_runtime/h0_text.rs) is shared
with grants without sharing their notification policy. Fourteen focused
[command tests](../../crates/inf-game/src/object_runtime/player/commands/tests.rs)
cover constructor state, signed mask boundaries, full network and low-byte
silent units, retained failures and retries, original nonlinear text mapping,
current H0 registrations, same-player messages and save-prefix custody.

Release checks passed `cargo test --locked --release -p inf-game --lib
object_runtime::player` (32 tests), `object_runtime::grants` (19 tests), and
`offline_reload_preserves_mission_player_and_process_rng_custody` (one test).
The latter changes the live triple, exports and reloads it through the actual
world path, and checks that caller-retained stale words do not override it.
The original-byte producer executes only
unchanged Set, Query, H0 miss and message bodies while armed. Its
[fixture](../../crates/inf-game/tests/fixtures/player_commands_native.json)
contains 25 histories, 62 calls, 130 routine entries and 118 returns.
Seventeen histories/54 calls are admitted to the portable comparison:
49 complete calls and five stops at reached semantic boundaries. Eight named
raw diagnostics are excluded before portable preparation; four complete and
four stop. Original constructor/save fragments and table/caller guards are
static evidence, not executed constructor/save proof.

The producer was independently rerun against the authenticated executable
and regenerated the fixture byte-for-byte. Producing source SHA256 is
`f7b0badafdd7844fb011a9d2985f3ad0e8158ca2a56a60e276fae273440037d0`;
fixture SHA256 is
`f39f7ea82ceb139df9d2ac07c100311cffaa3b4320c88a3c9a1c7997ef36dc59`.
Native nonstack phases, return sites, nested arguments, FS support and current
backing are recorded. Other stack reads/writes and nonvolatile registers are
checked online; serialized stack state preserves sparse knownness and does
not claim a complete access timeline or measured Rust cadence. Inherited
particle-pool preparation is unarmed and excluded from command counts.

Both independent native checks passed `cargo test --locked --release
-p inf-game --lib object_runtime::player::commands::native` (two tests).
The [original-byte/protocol audit](../../crates/inf-game/src/object_runtime/player/commands/native_evidence.rs)
never calls the portable command implementation. The
[portable comparison](../../crates/inf-game/src/object_runtime/player/commands/native_receipts.rs)
calls the public API on actual players, comparing Query truth, void Set,
reached errors and complete typed custody after each admitted call.
Native output updates only an expected snapshot after the public call.

The comparison independently verifies all original NUL records and every
current H0 slot in real inactive, Boot and Boot-to-Activate-48 profiles.
Boot has 692 records/691 occupied slots; the activated profile has 870
records/869 occupied slots, with 872/873 released. It checks residency
revision, registrations and immutable allocation identity within Rust before
and after commands. The native input declaration keeps original allocations
available as physical backing even when a profile releases or lacks them;
that availability does not establish current H0 registration or portable
allocation presence, and is never applied to actual residency.

No original constructor/save/loader execution, arbitrary callback or pointer
rebinding, CRT path, enabled UI, outer caller attachment or retail acceptance
follows from these checks. The
[intro-results player prefix](INTRO_SEQUENCE.md#results-screen-construction)
now composes silent command 37h and its later raw WORD copy before input
selection; other constructor phases remain explicit skips. Selector-12
commands 0/1/48h and shared capture command 42h still need their complete
caller sequences.

## Level 1 command sources

Infest01 makes 909 Set calls on its one player from the briefing teardown
(`AF0B4B`) through Level 1. Fourteen of them set a new, non-silent command
and send its hint. The setter sees the same commands again every frame
while their source holds, returning at the owned bit.

| Return | Source | Commands that sent |
| --- | --- | --- |
| `0043FF35` | selector 50 hint triggers | `25h`, `2Dh`, `03`, `04`, `45h`, `08`, `34h`, `20h`, `47h` |
| `0042F655`, `0042F862` | factory shop (`0042F2A0`) | `27h`, `28h` |
| `0046B388` | pickup grant `0046B2B0` | `44h` |
| `00416CDB` | vehicle deep-water check | `06` |
| `0046826C`..`00468282` | capture reaction `00468220` | `48h` (after silent `00`, `01`) |

Bundle `.tmp/infestation-ttd/20261008-144633-044-commands/`. Positions
that share a TTD sequence with a later send report a false "sent" for the
earlier call; the owned bit before the call settles them.

### Selector 50 hint triggers

Factory case 50 (`00460F95`) allocates `0x58` bytes and calls `0043FE60`:
the visual base `0047A030` (`+40 = 0`, `+42 = FFFF`), final vtable
`00522888`, then `+48 = 0`, `+50 = 0`, `+54 = 0`. `+44` and `+4C` stay
unwritten until the placement's commands set them. The vtable differs from
the visual base `00527B68` only in the destructor, commands `+04` and
update `+24`; `+3C` is `00405140`, and the flags payload (`00479DB0`) puts
it in the update list.

Commands `0043FF40` pass codes up to 2 to the visual base `0047A1A0`, then:

| Code | Operand and effect |
| --- | --- |
| 3 | WORD radius; `+44` = radius² |
| 4 | WORD seconds; `+48` = seconds × 1,000,000, in 32 bits |
| 5 | DWORD command to `+4C`; when the first player (`00488AA0`) already owns it (`0046C820`), the trigger disposes itself (`00479FA0`) |
| 6 | DWORD; `+50` = value ≠ 0, the silent byte |

The update `0043FEB0(delta)` returns without a first player or its
primary actor (`0046B030`). It sums the squares of the WORD-wrapped
signed differences between the trigger's and the actor's `+32..+36`. Above
`+44` (unsigned) it clears the timer `+54`. Otherwise it adds the delta to
`+54` and, once that passes `+48` (unsigned), calls `0046C760(+4C, ECX)`
every update. ECX holds `+48` with CL replaced by `+50`; the setter reads
only CL.

Level 1 places 22 triggers. Infest01 updates each 707 times (bundle
`.tmp/infestation-ttd/20261008-145315-615-triggers/`).
[selector50_triggers_native.json](../../crates/inf-game/tests/fixtures/selector50_triggers_native.json)
keeps each trigger's first update and the 1,138 in-range or resetting
ones, 831 of which set the command; the
[port's step](../../crates/inf-game/src/object_runtime/selector50.rs)
matches every stored timer and every set. The port constructs, commands
and updates the triggers; their hints reach the message boxes.
