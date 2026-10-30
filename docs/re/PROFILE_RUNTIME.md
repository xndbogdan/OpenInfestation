# Infestation profile menu and save owner

This document owns the derived profile menu reached by Start Game and the
save-owner contracts visible to that menu. Native contracts below come from
the local retail `Infest.exe`, with isolated native receipts and portable tests
identified separately. The implemented profile/name-entry flow does not establish
a matched retail menu sequence or gameplay support. The generic controller
and row walk remain in [FRONTEND_RUNTIME.md](FRONTEND_RUNTIME.md), and the
shared glyph/layout consumers remain in [TEXT_RUNTIME.md](TEXT_RUNTIME.md).
Physical save records and mission/player serialization belong to
[SAVE_FORMAT.md](SAVE_FORMAT.md).

## Derived menu ownership

`00449A00(bool)` allocates `0x8C` bytes, calls constructor `00448220`, then
pushes the instance through `00453440`. The constructor calls base
`00452310(NULL)` and stores `session+660` at instance `+5C`. Its vtable
`00524F28` contains:

| Offset | Target | Meaning |
|---|---|---|
| `00` | `004482B0` | Deleting destructor |
| `04` | `00452E20` | Base command dispatch |
| `08` | `00449690` | Derived tick |
| `0C` | `004483B0` | Derived draw |
| `10` | `00401310` | Return-only lifecycle callback |

The constructor sets `+60=1`, clears bytes `+61/+62/+88`, and clears DWORDs
`+64/+68/+6C/+70/+74/+78/+7C/+84`. A true argument then sets `+6C=2000`.
It does not initialize `+80`; actions write that pending slot index before use.

`00449410` rebuilds instance-owned rows from header template `00536D88`,
occupied-slot template `00536DA8`, and empty-slot template `00536DC8`.
The ordinary list has fourteen slots plus its header, selection 1, and nine
visible rows. Layouts at `00536D38` remain shared. The query is
`00449C90(!instance[+88])`, followed by setting `+88`. Exceptional query
states select other templates:

| Query state | Gameplay owner byte `+5C2` | Template / H0 label |
|---|---|---|
| 6 | Either | `00536DE8` / `0096` |
| 5 | Clear | `00536DE8` / `009D` |
| 5 | Set | `00536E08` / `0093`, edit lock `+2C=1`, choice `+2D=0` |
| Other exceptional states | Either | `00536DE8` / `0099` |

These are live derived objects, not another immutable PE menu table that can
be handed directly to the generic row walker.

## Tick, actions, and draw handshake

`00449690` checks `00449C80` invalidation, then consumes `delta_us/1000`
against the ordered timers `+64/+68/+6C/+70`. A positive, unexpired timer
returns immediately. Every positive timer sets `+60` before testing expiry,
so even its expiry tick waits for draw before ordinary ticking can proceed.
The tick returns while `+60` is set; draw clears that byte. Rebuild clears
the low half of screen `+C4`, calls input reset
`004F0340`, runs base tick `00452470`, and resets age through `00452440`.
Thus drawing is part of the state machine, including frames with no glyphs.

Pending operation `+7C` executes only when remaining delay `+78` is **less
than** the current millisecond delta; equality waits. The recovered operations
are:

| Value | Effect |
|---|---|
| 1 | If `index < slot_count`, `0044A170(index, false)`, then request phase 3; otherwise clear `+61` without loading |
| 2 | Request phase 9 |
| 3 | Delete through `0044A130` |
| 4 | Copy through `0044A150` |
| 5 | Call `0044A570`, then save owner `+28=1`, `+2C=0`, `+44=3` |
| 6 | Save owner `+30=index`, `+34=-1`; controller virtual `+04` |

Operation 1's phase-3 request enters `00487380` from the frontend. Like
New Game's `00487410`, it sets owner byte `+5C2` and constructs the player
through `00469D60`. It does not write `+5D8`, and it calls `0048EBA0(G+1E0, 0)`,
which keeps the current selection and elapsed time. Then `00449B80` restores
the working profile: the name through the name-as-format cursor, the mission
into `W` (projecting elapsed seconds into the session), the fresh first player,
and the `004633D0` name write. `0044A170` itself only reads the slot's nine
blocks into the working buffer. In the port, `PlayerPrepareSource::FrontendLoad`
selects that mission tail, and the session's `PrepareFromFrontend` effect runs
the same `ProfileStore::restore_current_working` as the results screen before
`LoadWorld` enters the saved selection. Unported worlds are not refused there.
They load with their unsupported placements skipped and stop at their own
reached boundaries. Both retail records with a world, Terra Nova (mission 2)
and Artoria (mission 11), load into gameplay: the respawn carries the saved
units back ([carried units](ACTOR_RUNTIME.md#carried-units)), and the
after-load link messages reach their receivers
([family links](FAMILY40D800_RUNTIME.md#link-messages)).

Controller vtable `0052561C` resolves that last callback to `004151C0`,
which sets controller `+24=0` for fade-out; it is not an immediate stack pop.
Operation 2 sets skip-one-draw byte `+62`; operations 1/3/4/5 set `+60`.
All clear the pending operation and delay after execution. Load/copy/delete
error catches clear `+61/+88` and set two-second timers `+64/+68/+70`
respectively. `0044A570` is an actual native return-true stub, not evidence
that a save-file reset was performed.

Action mode `+74` cycles load/copy/delete (0/1/2), except while editing or
when gameplay owner byte `+5C2` is set. Copy is skipped at fourteen slots.
`00448330` also changes the instance-owned header row's copied label argument
to H0 `00CD/00D0/00CF`; it does not change the original template.
The command wrappers are `004492E0` (header, only commands 2/3, index -1),
`00449300` (occupied slot), `004493F0` (confirmation), and `00449400`
(empty slot, Activate only).

Empty-slot activation `004486E0` uses the current slot **count**, not the
clicked empty row: with owner byte set it schedules operation 6 after 1 ms;
otherwise it sets save owner `+30=count`, global `0054694C=1`, schedules
operation 2 after 1000 ms, and selects camera 9. An empty store additionally
gets `+34=0`. Copy/delete activation enters edit lock `+2C` and clears
choice `+2D`. Confirmation commands 2/3 set/clear that choice, command 4
uses it, and commands 6/7 are Yes/No. Accepted delete/copy/load modes
schedule operations 3/4/6 respectively. The non-owner load path first calls
`0044A9F0(index)`: true emits `004391D0(4)`; false schedules operation 1
after 1000 ms, sets `0054694C=1`, and selects camera 9.

## Name-entry handoff

The offline profile operation 2 requests phase 9; session effect `00487410`
then sets gameplay owner `+5C2`, constructs a real player through `00469D60`,
and resets mission state through `0048EBA0(22, true)`. Selection 22 is a
mission selection, distinct from the phase and OVL namespaces. The serialized
defaults and nested grant-table dependency closure are recorded in
[SAVE_FORMAT.md](SAVE_FORMAT.md#mission-selection-and-new-player-evidence);
they do not replace the player's binding/list/notification ownership.

Factory `00459000` allocates the separate `0x154`-byte text-entry screen
`00459060`, vtable `00526028`; it is not a generic profile-menu row. The
constructor reads the requested phase (`+650`): phase 9 uses `00463550`,
phases 10/11 use `00463D10`, filling the name at `+11C` (at most 30 bytes) and
initials at `+13C` (at most 4 bytes). `00463550` starts with `Novice` if the
HKCU key cannot open; an empty name after an opened-key lookup falls back to
`Jameson`, with a legacy HKLM lookup on query failure. Normal startup
establishes that key: frontend constructor `004838A0` calls
`00463100` at `00483B41`, and that helper creates the HKCU key at `0046312D`
before writing settings (creation failure throws). Thus a first read with
no saved identity yields `Jameson` and offline `1up`; absent sidecar values
represent missing values, not the `Novice` key-open-failure path. The read goes
through the process caches, not straight to the store: the Options player
submenu's `004639D0` calls `00463D10` on every draw, which sets loaded flag
`0054EC59` (`00463EE7`) without writing the name cache `0054E930`. A later
constructor read `00463550` (`00459171`) then takes its cached branch
(`00463567` -> `00463725`) and copies the still-empty name cache, so name entry
starts with an empty buffer and cell 0 rather than cell 77; Tab then
substitutes `Jameson` at `0045AD4F`. Only a successful writer
`004633D0`/`00463490` clears the flag (`00463478`): a failed registry write
throws before it. The production platform holds one
[`PlayerIdentity`](../../crates/inf-game/src/player_identity.rs)
over the sidecar (`SidecarIdentityStore`: the key always opens, there is no
legacy HKLM key, and values are stored with their terminator, so a value fits
a query cap only when shorter than it); the constructor, both submenu rows and
the completion writer use it.
The concrete read contract (caches, loaded flag, key/value names, the `%dup`
initials default, and the 00463D10 quirks) is owned by
[`player_identity`](../../crates/inf-game/src/player_identity.rs);
the recovered initials default is `sprintf("%dup", 0055F2F4)`, i.e. `%d` plus
the literal `up`, which reads `1up` offline (`0055F2F4` is 1 from boot) and
the transport's slot in a network game's phase 9. The
portable sidecar owns both strings in
[`frontend_settings`](../../crates/inf-game/src/frontend_settings.rs)
and never touches the registry or the first save's name. The shared writer
uses one `IdentityWriteRequest`: selected player/net name, required name bytes
and explicit `IdentityInitialsWrite::Keep` or `Write`. Both native bodies scan
and write the name first. `0046343F/00463441` and `004634FF/00463501` then skip
initials when their pointer is NULL; a non-NULL empty string writes an empty
value. The portable `Keep` mode preserves both the saved initials and the
independently remembered live initials, including absence or empty values.
`Write` keeps raw bytes through the first NUL. Neither mode imposes the
text-entry display limits or rewrites cache payloads.

Phase 9/10/11 completion explicitly uses `Write`, preserving the last saved
ordinary settings and other identity. A live cheat word at settings `+84` is
not saved by this write. The portable sidecar replaces the requested identity
values together and updates remembered identity only after a successful write;
the concrete adapter then clears the loaded flag. A failed portable write
preserves both live identity and cache state. Native per-value registry errors
can retain a successful earlier name write, so atomic sidecar failure is a
narrower persistence contract rather than native partial-commit parity.
Phase 11 edits the initials with limit 4; phases 9/10 edit the name with limit
18. The editable copy at `+F8` is truncated to that limit and selects cell 77
when nonempty.

The pointer at `00537BF8` selects the 78-byte character grid at `00537C48`:
rows `ABCDEFGHIJKLMNOPQRSTUVWXYZ`, `abcdefghijklmnopqrstuvwxyz`, and
`1234567890  -.,:.` followed by nine spaces. Its three rows have 26 cells.
`0045AC30/50` move selection by `(index+52)%78` / `(index+26)%78`;
`0045AC70/D0` wrap left/right within the current row and change `+148` by
`-2520/+2520`. Activate `0045AB50` requests phase 3 while error byte `+14C`
is set; otherwise it deletes the last byte at cell 75 when nonempty, confirms
through `0045AEB0` at cell 77, or appends that cell's byte below the limit.
Back `0045AC20` requests phase 2. Tab (`0045AE90 -> 0045AC00`) runs `0045AEB0`
only while `+14C` is clear; `R` (`0045AEA0 -> 0045AC10`) reruns completion only
while it is set. Both fire on press only.

### Text-entry tick `0045A8F0`

The tick first runs `0045AD30` when attract flag `0055F14C` is set (the
frontend's `00484F6B` sets it together with its 60-second phase-9 request),
then again when network flag `0055F448` is set. Neither branch returns: both
fall through to the rest of the tick. While completion delay `+EC` is zero,
`00445840(buffer, 15)` writes `buffer[0] = 0`. For a newly reported key it
`strncpy`s that key's name from table `00535E54` with NUL padding and sets
`buffer[14] = 0`; latch `0054766C` suppresses repeats until every key is
released. Letters report single uppercase bytes, digits and main-block
punctuation single bytes, and keypad keys multi-byte names (`Keypad 7`) that the
block ignores; unnamed codes report empty names. The table covers codes
`00..DD`: `00445840` indexes it without a bound, so code `DE` reads a NULL
pointer and higher codes read the following action-name table. The tick then
tests `buffer[1]`:

- zero: select cell 77, then append `buffer[0]` below the limit (an empty
  name appends nothing);
- nonzero: `strncmp` prefixes `Delete`/6 and `Backspace`/9 erase the last byte,
  `Space`/5 appends `0x20` below the limit, and every other name is ignored.
  An empty buffer's erase writes NUL to `+F7`, the always-zero top byte of
  the selection DWORD; the port leaves state untouched.

With no reported key `buffer[1]` is stale stack. The retail recording shows it
is read: see the capture ledger.
In steady-state ticks its last writer is `push eax` at `004A48A2`: the
selected-input poll `004F0200` samples its clock through `004A4890` at
`004F0325` (reached from `004945E3` only while the scheduler's input selection
matches). The byte is therefore bits 8..15 of that `timeGetTime()`
millisecond sample, the clock the port's input poll already samples. When those
bits are zero (256 ms of every 65.5 s) an idle grid snaps to cell 77; the
recording observed a 0-to-77 snap before any character was typed. The first
block after construction had another, unidentified writer.

Row animation `+148` then decays by `*7/8` (truncating), base input `004392B0`
dispatches held bits through `00439470` into slots `+24..+38`, and `00439560`
advances the shared animation clock. Sixteen `0048F060` draws update effect
bytes `00546938`, a process global that another screen also updates and the
constructor never clears: a zero `draw % 32` stores `0x7F` and **falls through** into
the same decay, which subtracts the low byte of `value * (delta >> 13) / 32`
when `value > delta >> 13` and otherwise stores zero. Completion delay and
feedback then count down by `delta_us/1000` (unsigned compare). When the delay
expires it parks at 1 and calls `0045AD30` if `+14C` is clear; equality fires
here, unlike the profile menu's pending-operation timer.

For ordinary names, `0045AEB0` sets completion delay `+EC` to 1000 ms (1 ms
in phase 10). Its recognized-cheat branches modify settings, clear the name
and set a separate feedback timer rather than completing.

### Cheats

A recognized name sets one bit of settings `+84`, the cheat word, which the
settings writer `00463100` saves as the registry value `Cheats` (record
`00526D50`). `0045AEB0` compares the name with six strings at
`00537CC0..00537D14`, in this order:

| Name | Bit | It opens |
| --- | ---: | --- |
| `INVINCE ME` | `2` | The Cheats menu's Invulnerability |
| `GIVE IT TO ME BABY` | `1` | Skip to, Complete Level and Scavange Level |
| `SEMPER UBI SUB UBI` | `4` | Show Full Database, Get Full Research Tree and Get Full Ship Upgrades |
| `I AM HUNGRY` | `0x20` | Fill resource crystal tanks |
| `DR FRANKENSTEIN` | `0x40` | The Cranial Implanter at every respawn |
| `BUGGY RACING` | `0x80` | An eleventh entry in the list `0045EE20` draws, and the row `0045F560` steps past 9 |

The first four show their rows in
[the computer's Cheats menu](PLAYER_MESSAGES.md#the-computer-and-the-in-world-menu);
the last two never open it.

### The completion bonus

The campaign's end grants the `0x40` bit too. State 6 of `00406830`, in the
Alien Brain's ending (selector 78), once its `+158` timer is below 1000,
ends the world with `00487560(1, 1, 0)`, sets the bit and sets the world's
`+5C6`; while `+5C6` is set the world tick sets the bit again every tick
(`004875E3`). The end stores selection 1 as the next world, and world 1's
name (H0 `0xE1`) is "Completed": the results page takes it for the final
world (`+1E0 == 1`) and returns to the frontend, and the profile menu
refuses to load the profile from then on (`0044A9F0`). The frontend
constructor `004838A0` saves the settings each time the frontend opens
(`00463100` at `00483B41`), so the bit reaches the registry value `Cheats`
on that return and stays there: nothing clears a bit of `+84`, and the
settings load from the registry once, at startup (`004631E0`, from
`0043873F`). From then on, on that installation, each respawn (`0046A890`)
grants the Cranial Implanter (`0046B2B0(0x33, 0)`, then `004750C0(0x33)`,
`0046ADE8..0046AE0E`), the first spawn of every world included, and the
grant saves with the profile.

So the bonus belongs to the installation's settings, not to a save. A new
game builds a fresh player (`00469D60`) and mission (`0048EBA0(22, 1)`);
the directory scan leaves the list's last record in the working buffer, but
the serializer `00449C10` writes the new name, mission and player over it,
and nothing reads a neighbouring slot. Finished saves copied to another
installation, or settings reset, leave new games without the implanter.
A retail list shows this: of its seven profiles, a finished
Jameson lacks the implanter, the Jameson below it and a finished BOBSTER
have it, and four later games do not, while the registry's `Cheats` holds
only `0x80`, the world-4 gate's bit.

This is the only completion bonus. Every read or write of `+84` after the
settings getter `00463AB0` (all 203 calls, followed through the registers
that hold its result) is one of these: the ending, its world tick, the name
check, the world-4 gate (`0048C40C`, bit `0x80`; see
[gates](SELECTOR30_RUNTIME.md)), the Cheats menu build `00413410`,
`0045EE20`, `0045F560` and the respawn. Nothing counts playthroughs, and
no completion grants Sonic Shock.

The two upgrades come from these places only:

- The Cranial Implanter (`0x33`, "Possession missiles"; weapon row 7): the
  respawn under bit `0x40`, from the campaign's end or the name
  `DR FRANKENSTEIN`, and the Cheats menu's Get Full Ship Upgrades
  (`00414BA0`, opened by `SEMPER UBI SUB UBI`), which grants every ID from
  0 to `0xAE` but Invulnerability (`0x29`) through `0046B2B0(id, 8)`, after
  Get Full Research Tree (`00414AE0`) has set the same IDs in the
  computer's grant owner.
- Sonic Shock (`0x26`), the Detonator's upgrade ("This will detonate all
  explosives within an area"): with it the Detonator `0048CB00` sends the
  pulse `0048D980` instead of event 10 to the owner's explosives within
  4000. The pulse sets off every explosive within 4000, whoever's (grenades,
  mines, missiles in flight and Bombs), shakes the radar for about 1.6 s
  and whitens the screen ([the detonator](SELECTOR26_RUNTIME.md#messages)).
  Get Full Ship Upgrades grants it, and so does the network loadout
  `004698F0` for game type 0, class 2, which the respawn runs only in a
  network game (`0046AE13`). So a player gets it by naming a new profile
  `SEMPER UBI SUB UBI` (settings `+84` bit 4), which opens the computer's
  Cheats Menu (Tab, row 6), and choosing Get Full Ship Upgrades there; or
  in a network game. The port runs the
  [Cheats Menu](PLAYER_MESSAGES.md#the-cheats-menu); its start harness's
  `upgrades` gives it too.

No world grants either. The port's resident inspection of all 34 world
overlays finds no pickup kind, factory stock word (selector 15), key grant
(selector 18), gate word (selector 30), selector 48 grant, actor grant word
(`+7A`, from placement command 9 or the constructor payload's `+38`) or
placement condition (commands 13 and 14) naming `0x26` or `0x33`; the
furniture drop table `00437A48` has neither; and the only immediate pushes
of either ID before a grant or query call are the loadout, the respawn and
the Detonator's query (`0048CB12`).

The port's respawn grants the implanter under the live cheat word: the name
cheat, a gate's bit and the world load all reach it (the ending is not
ported). Its port fix `completion_bonus`, on by default, treats a save list
holding a finished campaign as the bit set, so every game gets the bonus
whatever the settings hold; `--completion-bonus false` or the launcher's
Options restore retail's rule.

`0045AD30` substitutes `Jameson` for a blank buffer. Phase 10 writes the
edited name as `Net Player Name` with the loaded initials through `00463490`;
phase 11 writes the loaded net name with the edited initials; both request
phase 2. Phase 9 writes the edited name with the loaded initials through
`004633D0`. It then calls save owner `0044A0E0(name)` to select the append
index, reads network flag `0055F448`, and unless it is set calls `00449C10`
for offline mission/player serialization **in memory**. A caught failure of
either store call sets `+14C`. Phase 3 is requested only if `+14C` is still
clear at `0045AE52`, so a successful `R` retry requests nothing; Activate then
moves on. This path never calls disk writer `0044A110/0044A580`. The name-entry
draw `00459310` has concrete outer-frame, overlay and backdrop owners below,
joined to the scheduled session's linked draw and presentation callbacks.
The SDL app reaches this screen through Start Game's actual derived menu.

The phase-9-to-3 transition already has the prepared player and does not run
the frontend-load restoration effect `00487380` again. It still requires
`004874D0 -> 0048DC30`: release frontend music, activate selection `0x30`
then `mission+10`, initialize mission/world resources and construct placement
owners. These world effects and the resulting screen must exist before a host
can report successful phase-3 loading. The [`name_entry`](../../crates/inf-game/src/name_entry.rs)
owner implements the constructor's editable selection, grid moves, the held
slots and Tab/R paths, the 0045AEB0 cheat/delay branch, the key block over a
typed `00445840` result (including the stale byte), the full tick with
fall-through forced completions, and 0045AD30 ordering through explicit
identity, mission/player, key, input and phase hosts. Unicorn receipts from
400 unmodified `0045A8F0` executions pin the key block, decay, RNG effects and
timers. The [key translator](INPUT_RUNTIME.md#authored-text-entry-binding-tables)
and text-entry tables now have owners, and the constructor's concrete identity
reads are owned by
[`player_identity`](../../crates/inf-game/src/player_identity.rs).
Live player construction and world transition remain open.

### Text-entry screen owner

[`TextEntryScreen`](../../crates/inf-game/src/text_entry_screen.rs) composes
`00459060` in native order. A requested phase outside 9..11 is rejected before
anything is acquired, matching the reuse test `00459270`; the class query
`00459260` returns 8. The base `00437F90` attaches the scheduler member,
creates the input context and `+CC`/`+D0` analog references, and links session
membership. The requested phase's identity read (a required host effect) then
feeds the editable copy. Device enumeration and the text-entry binding install
(`00439B30`/`00439D00`) follow, then context selection `004947E0` and the
shared animation reset `00439530`. The `+F0` player pointer from `00488AA0` has
no reader in this class and is not modelled. Construction is not
transactional: after a failed step the owner stays stopped with its completed
acquisitions for explicit teardown, as on the frontend. Teardown `004592B0`
releases the binding sets, then the base unlinks membership, releases `+D0`,
`+CC` and the context, and removes the scheduler member; unlike the frontend it
publishes no returning flag.

The tick composes `0045A8F0` with its host services, reading the network flag
`0055F448` every tick after the attract branch. The translator reads the
native-type-0 adapter's state through `00439610(0)`, which returns entry 0 of
the adapter cache `005474B0`. An entry is created lazily (checks `00439B94`,
`00439C2B`; stores `00439BC7`, `00439C62`) by the first enumeration of any
screen with a digital or analog table for that native type and an eligible
device of that type. It lives for the process and is cleared only by
`00439DA0` at exit, so the port takes it from the process rather than this
screen's own enumeration. A missing entry is an explicit error where retail
would fault. The idle stale byte is bits 8..15 of the `timeGetTime()`
millisecond word that `004A4890` samples for the latest selected-input poll
`004F0200`. The first tick after
construction had another, unidentified writer, for which the port substitutes
the same poll byte. Base input uses the `00439470` policy with the capped
delta for repeats and the uncapped time for age, and its slots `+24..+38` reach
the name-entry actions. `00439560` advances the shared animation clock, and the
latch `0054766C` and effect bytes `00546938` stay in process custody. Tab and
`R` are the screen's own press-only callbacks behind the error byte `+0x14C`;
system records and other contexts fail before mutation, as on the frontend.
The production session host constructs and destroys this screen and provides
scoped draw access to it. `TextEntryScreen::draw_frame` reads its current entry
and uncapped age with supplied process animation/selector inputs, without
advancing input, RNG or completion. Only a Ready screen draws; a host failure
stops it before another draw attempt. The production linked dispatcher routes
the selected-input poll and tick to this owner, using the process keyboard,
RNG and animation owners across the frontend handoff.

### Name-entry outer frame `00459310`

[`draw_name_entry_frame`](../../crates/inf-game/src/name_entry_frame.rs)
owns the preparation and successful tail around the two rendering phases.
It consumes the shared `00439270` preparation:
three distinct surface `+14` lookups through `00495800` select arena replacement
`004A43A0(0xC8000)`, batch reset `004A4400` and publication `00546E88`.
The outer draw then reads width followed by height, captures the identity of
session `+648` at `0045936B`, and reads the surface renderer a fourth time.
It begins that renderer's batch, calls its virtual `+14(0)` reset, and validates
the surface through `00495770` before invoking backdrop `0045A560`.

The backdrop makes no additional `00495800` renderer lookup. Its `0043B4A0`
copies the shared template and reads fresh surface dimensions, then `0045A626`
copies the captured caller context's **live** renderer field `+00` into that
local context. Replacing session `+648` after capture cannot redirect it;
changing the captured context's renderer field can. The caller context itself
is not overwritten. Overlay stars, flush, text and highlight use the fourth
surface renderer and the original dimensions, even when these differ from the
backdrop's. The generic frame host keeps these identities separate. Constructor
`00438780` initializes session `+648` to `00546E88`; this renderer context is
independent of the newest screen in the session's `+48` membership list.

The final `004957F0 -> 0050DCC0` only unlocks surface `+18` when its `+1C`
lock flag is set, then clears that flag. Presentation and the next colour clear
belong to the separate linked-screen `+0C` callback
`0040D1D0 -> 00438120(H7/32) -> 00495810 -> 0050D330/0050D960`.
There is no local `0x301` recovery scope or unconditional cleanup tail in this
draw: a failure preserves prior effects and skips the unlock.

[`SoftwareNameEntryFrame`](../../crates/inf-game/src/name_entry_renderer.rs)
joins the existing backdrop and overlay owners. Its one CPU backend requires
an explicit context-to-renderer association and rejects mismatches. Its drawing
registry owner represents that portable association, not a literal native context
address. Another screen becoming the newest session member cannot redirect it.
It resolves the captured registry owner again, uses independent shared-template
projection inputs, and validates window dimensions against the pixel surface
when supplied.
CPU pixels are permanently accessible, so its successful unlock is a no-op on
pixels and does not present or clear them. Session draw access supplies the real
screen and resources under the registry's context guard.
[`SoftwareLinkedFrame`](../../crates/inf-game/src/session_platform/software.rs)
selects that owner for linked `+08`; the separate `+0C` callback presents, then
clears through the current H7/32 material. The SDL app uses these same callbacks.

Two [native receipts](../../crates/inf-game/tests/fixtures/name_entry_frame_native.json)
execute unmodified outer draw, preparation, backdrop, text walkers and surface
unlock code with synthetic resources and hooked backend effects. The adversarial
case returns four distinct surface renderers, then changes dimensions, session
context and the captured context's renderer during validation. Tests reproduce
both ordered boundary streams and inject 69 failures across 23 boundaries,
including partial model/star/text submission and ordinary, `0x301` and `0x711`
errors. These checks establish dispatch custody and ordering, not hardware pixels
or a general surface-recreation implementation.

### Name-entry backdrop `0045A560`

The draw calls `0045A560` after its renderer begin and before the star field.
[`draw_name_entry_scene`](../../crates/inf-game/src/name_entry_scene.rs)
emits its commands in order. Fade is `min(age * 4, 0x7FFF)`, compared signed
after the wrap. The directional light is `00417DA0(4000, 4000, 6000 -
min(age, 16000))` with colour bytes `(255, 210, 170) * fade >> 15`. The ambient
light follows as the sign-extended words `(10, 6, 5) * fade >> 15`. `00490130`
keeps these signed values and separately clamps each to a byte, so the port's
ambient command carries signed values. `0043B4A0` fills a local context from
the shared template `005474E8`: focal `00546F20/24`, surface size and signed
half size. `0045A560` then copies context `+38..+4F` to `0055F22C` itself.
Unlike the frontend (`00483D41..00483D6A`), it does not double the focal pair
first. Template fade `0x100000/0x100000` gives fade factor `0x1000000`. The
view is the template's eye and basis. There is no batch begin or flush, and
coordinate shift `0055F250` is untouched.

One `0x3C`-byte stack object (`0043A150`, vtable `00522360`) is constructed
once and refilled before each of three `0049FEA0` calls. Each refill copies
the identity basis `00547590` afresh, counter included, so no rotation carries
over. The three fills are:

- 748 at `(0, 0, 15000)`, rotated in XZ by `age * 4`;
- 757 at `(min(30 * age / 10, 36000), min(20 * age / 10, 24000), 50000)`,
  rotated by `0x8000`;
- 744 at `(min(20 * (age - 2500), -28000), -6000, 30000)`, unrotated.

Products wrap before the signed division. The object callback `0043A270`
returns the clock `005471C0` for selector 0, word `00533760` for 1, `0xFFFF`
for 2 and 1 for 6. The frontend's `00483C40` returns zero for all but 0. The
retail backdrop programs evaluate no packed callback selector other than 0,
invoke no children and use no computed selector (`0xFA`), so the two
policies agree for these models.

A whole-trace replay (see the
capture ledger)
found that `00547590` and the template basis (counter and identity vectors)
are written only by their initializers `00439630`/`00439760`. The template
eye `005474EC..005474F4` is never written and stays zero-filled `.bss`. The
template's fade and pool words are written by `00439880`. `00533760` stays 1 through the phase-9 window
and is later set to 2 by `0042FFE9`, so a later name entry reads 2 for
selector 1. Twenty-six unmodified `0045A560` runs, seeded from those phase-9
values, are retained in
[`name_entry_scene_native.json`](../../crates/inf-game/tests/fixtures/name_entry_scene_native.json).
They cover the fade and approach limits, negative ages, the wrapped fade and
positions, and a light Z that overflows the signed word. A software-scene
smoke draws the three models through the resident model executor at
640×480. This is a portable rendering, not a matched retail frame.

### Name-entry overlay `00459310`

[`draw_name_entry_overlay`](../../crates/inf-game/src/name_entry_draw.rs)
owns `004593AC..0045A53A`: font/style resolution, stars, the backdrop flush,
feedback, three character rings, error lines and the name/completion animation.
It receives a read-only snapshot of the existing name-entry state. The caller
must first execute `00439270`, capture surface/context/renderer state, begin
the batch, reset/validate the renderer and call `0045A560`; final `004957F0`
surface unlock is also outside this layout owner. There is no native
`0x301` recovery scope here.

The 1000 stars start a **local** LCG at zero every draw, advancing twice per
star with `state * 0x13D56D + 0x13D573`. X and Y are signed remainders of
those successive words by width and height; negative coordinates are real
submissions, not wrapped back onto the screen. Twinkle uses the positive
half-cycle `sine_q15((state + 50*age) & 0x7FFF)`. Material selection is
`clamp(37 + abs(state)%7 + sign16(5*sin >> 15), 37, 45)`; before age 2000,
subtract `(2000-age)/100` and lower-clamp to 37. Products wrap before signed
division. These are one-pixel quads, H7 material slots 37..45, with vertex
DWORD `0x7FFFFFFF`. They call renderer `+84` **immediately**. The queued
backdrop flush follows all stars (`00459638`), so the models can cover them.
The frontend star path instead queues `00466170` and changes its model
context; the two submission policies must stay distinct.

The rings visit all 78 cells in index order unless completion delay is
exactly 1. Cells 75 and 77 use H0 strings `0x117/0x118`; all others use the
grid byte. Base radii are `7*width/20` and `radius_x*focal_y/focal_x`. During
entry they grow by `age/2000`; during completion each uses **unsigned**
`wrap(2*(delay-500)*radius)/1000`, including the large radii below delay 500.
The inner rows then scale by `17/20` and `14/20`. Angle is
`(((index-selection)%26)<<16)/26 + row_anim`; sine products narrow to signed
words before the final `3/4` scale and surface centering. Every signed division
truncates toward zero.

The selected cell gets a sprite-7 highlight only after age 2000. Its half
width is the measured constant `EN`, not the selected label; half height uses
the focal ratio. Center Y shifts up by one quarter of that height. The direct
`+A4` quad carries style 3 and vertex attribute `0x4000`. Each cell then draws
style-7 shadow text at `(x+1,y+1)` and foreground at the restored signed-word
position. Unselected colour evolves in two stages (age below 4000 and
4000..9000), with byte truncation between stages; the first stage starts
from style 7 below age 1000 and style 2 afterward. Its shade angle excludes
`row_anim`, and the selected cell uses style 3 directly after age 2000.

Feedback uses H0 `0xE0`, style 2 and the existing timed multiline walker at
`(width/20,3*height/20)`, with reveal age `1000-feedback`. Error lines use
H0 `0x367` and `0x27` at 65%/70% surface height; the first alternates style
2/3 every 250 ms. Retail leaves the descriptor's text pointer on the second
error label. Subsequent name formatting writes the local buffer without
restoring that pointer: the retry label is drawn again in the name line,
including both prefix/name calls during completion. The port retains this
observed executable behavior explicitly rather than fixing the original UI.

Ordinary name text is `H0[0x115] + ": " + name`, with a 300-ms grid-preview
blink. Delete preview removes the final byte of the whole formatted string,
even for an empty name; the original state is untouched. RGB fades with the
same signed wrapped `min(4*age,0x7FFF)` as the backdrop. During ordinary
completion, the full prefix is measured before truncation; after an error,
the retained retry label is measured instead. The prefix's visible byte count
is `max(0, wrap(prefix_length*(delay-500)*2)/1000)`, and the name travels toward
horizontal center and 40% screen height after 300 ms. H0 `0x162` starts timed
reveal once delay is
below 900. All text uses the existing native single-line/multiline consumers;
the high colour byte and inherited glyph base remain intact.

[`SoftwareNameEntry`](../../crates/inf-game/src/name_entry_presentation.rs)
joins these operations to resident H0/H3/H4/H6/H7 resources and the concrete
CPU renderer. Tests preserve direct-star/highlight submission while backdrop
commands are pending, and render editing, feedback and two completion states
at 640×480. Error string H0/`0x367` belongs to leaf 1's language phase
(local 179), absent from the frontend `{0,47}` set. The corpus test retains
that explicit missing-resource error after the earlier stars/flush/rings;
activating leaf 1 is not a remedy because it replaces leaf 47 and its models.
This is not an unimplemented H0 lazy loader: constructor `004740A0` publishes
vtable `00527598` at `004740E4`, whose first entry is `004EFBF0` (`xor eax,eax;
ret 4`). On a vacant slot, the call at `00459F8D` therefore returns null;
`00459F9D` passes it to text measurement `004A4240`, whose `004A4244` reads
`byte [ecx]` without a null check. Static retail code thus establishes a
null dereference on this missing-string path, rather than an alias or an
implicit bank activation. The port reports `NotResident` at that boundary.
The accepted recording does not establish execution of this error branch;
this conclusion is about the native code under the proven frontend residency.
Thirty-two [native fixtures](../../crates/inf-game/tests/fixtures/name_entry_draw_native.json)
execute unmodified `00459310` and its real text walkers/measurement over a
synthetic font. They retain ordered text/highlight/flush receipts and hashes
covering all 32,000 star packets, including error/completion combinations.
This checks layout and ordering, not native GPU pixels. The scheduled SDL path
uses this renderer; matched retail-frame comparison remains open.

## Text-entry screen input dispatch

Vtable `00526028` has sixteen slots followed by a zero terminator at
`00526068`: `+00` deleting destructor `00459290`, `+04` tick `0045A8F0`, `+08`
draw `00459310`, `+0C` presentation `0040D1D0`, `+10`/`+14` base input
select/deselect `004947F0`/`00494810`, `+18` class query `00459260` (class 8),
`+1C` reuse `00459270` (requested 9/10/11), `+20` base held-bit dispatcher
`00439470`, `+24` Activate `0045AB50`, `+28` Up `0045AC30`, `+2C` Down
`0045AC50`, `+30` Left `0045AC70`, `+34` Right `0045ACD0`, `+38` Back
`0045AC20`, and `+3C` the empty base slot `00401310`. The next table at
`00526070` belongs to another class. `00439470` tests screen byte `+0xC4` and
calls `+24/+28/+2C/+30/+34/+38` for bits `0x01/0x02/0x04/0x08/0x10/0x20`. The
Tab and `R` callbacks and confirm effect `0045AEB0` are internal callees, not
virtuals.

### Phase-8 screen held-bit wrapper (vtable `00521724`)

The phase-8-specific wrapper and controller ordering belong to
[results runtime](RESULTS_RUNTIME.md#phase-8-screen-held-bit-wrapper-vtable-00521724).
The results screen pushes this same menu onto its own controller `+100`, with
byte `+5C2` set, to choose the slot a save goes to
([the profile menu](RESULTS_RUNTIME.md#the-profile-menu)).

## Derived draw `004483B0`

The draw obtains H4 font 0 and H6 style 3. It calls generic `00452570` only
when `+60==0`, `+7C==0`, and `00449C90(save_owner, false)!=0`. If `+62`
is set at that point it clears the byte and skips the row walk once instead.
Both return paths clear `+60`.

After this gate, the first positive timer in `+64/+68/+6C/+70` selects H0
`00C6/00C8/00C7/00C9` respectively. Its RGB is interpolated from H6 style
3 toward style 2 using signed `timer % 400` divided by 400. Only the three
RGB bytes are assigned; the text descriptor's high color byte is not proven
initialized here. The measured `004A4240` width is narrowed to signed AX;
X is `(surface_width - measured_i16)/2` with native signed division, and
Y is `surface_height * 69 / 100`. This is a direct `004A3B20` text call
with font 0 and full-surface text width, not the shadow/reveal wrapper
`00453810`. Native integer wrapping and narrowing apply to these operations.

## Profile text callbacks

### Action header `00448E60`

The callback reads gameplay owner byte `+5C2`, obtains font 0/style 3,
and calls `00485320(1)`. That helper queries the current screen's virtual
`+1C(2)` and calls camera selector `00484AC0(1)` only if it succeeds.
This is a required camera side effect, not a model draw.

It queries `00449C90(save_owner, false)`. State 0 bypasses the action-label
chain; states 2/4 draw the supplied label in style 3. In the ordinary
non-owner branch it draws H0 `00CD`, `00D0`, `00CF` through `00453A80`,
selecting style 3 for the current mode and style 2 for the others. After
each of the first two calls, it mutates the shared row descriptor's X
(first by signed extent-X / 3, then by signed extent-X * 9 / 30) and
subtracts 300 ms from age. The selected label's starting X is retained for
confirmation placement in a `[esp+0x48]` slot: mode Copy (1) stores the
second-label start after the first advance, mode Delete (2) stores the
third-label start after the second advance. Mode Load (0) never writes that
slot, so its confirmation reuses whatever the slot held on entry (the
emulator harness observes `0x1000`). Load with edit lock `+2C` set and a
clear owner byte is unreachable through native transitions (Load activation
schedules its pending operation without locking; lock entry requires
Copy/Delete or the owner branch), so the port defines that unreachable state
as the selected label's starting X instead of replicating uninitialized
stack contents. With owner byte set it draws H0 `0090` instead.

When edit lock `+2C` is set, `00448990` draws confirmation at that retained
X, with H0 `00CA` for the owner branch and `00CE` otherwise. The owner
branch then changes clip to the full surface and directly draws H0 `0028`
through `004A3B20`: X is 10% of width, text width is 80%, and Y is 7/8
of height. The outer generic walk owns clip restoration.

### Slot label `00449320`

Query state 0 returns without drawing. Otherwise `0044A320(slot)` checks
the summary entry. Status 3 invokes `0044A480(buffer, slot, false)`;
if that also returns 3, the generated bytes go to the **left** argument of
`00453810`. Other statuses use `"%d. %s"` with one-based slot number and
H0 `00D1`. If such a row is selected (`slot == selection-1`) while mode
is nonzero, draw calls `00448330(-mode)` before formatting, resetting the
operation/header mode. This is a draw-time state mutation.

### Confirmation and exceptional rows

`00448990` starts at the selected X, adds `(extent_y*7)>>5` to signed-WORD
Y, and draws a style-3 prompt. It appends `" ("`, the first byte of H0
`010A`, `"/"`, the first byte of H0 `010B`, and `")"`. Each append uses
`00453810`; subsequent X advances by the low WORD of the preceding
descriptor `+18` result. The Yes/No character chosen by `+2D` uses style
3 and the other character uses style 2; punctuation uses style 2. The
character format is `%c` after signed-byte promotion.

`00448C60` draws its supplied generic label, restores original X, adds
5% of surface height to Y, then draws H0 `0026` when owner byte is set or
`0028` otherwise. `00448D10` is another live-state callback: state `+84`
0 chooses `0093` if locked, otherwise writes state 3 and chooses `009D`;
states 1/2/3 choose `009A/0098/009D`. Other values use the supplied label.
It uses style 3, optionally invokes confirmation `00448990`, and states
1/2/3 additionally place H0 `0028` 5% of surface height lower.

These functions contain text calls and camera/clip/state changes; they do
not directly submit model primitives. None is covered merely by accepting
its address as an alias for a generic label callback.

## Save summary and process token

The save owner has a `0x2A0` summary table at `+40`: fourteen `0x30`-byte
entries, with known byte `+00`, status DWORD `+04`, name at `+08`, mission
DWORD at `+28`, and elapsed-seconds DWORD at `+2C`. `0044A320` may lazily
scan/read when an entry is unknown; a portable draw must preserve that
query's owner effects. `0044A3E0` formats the working name into summary `+08`,
then reads source DWORDs at the formatter's emitted count plus one. This can
differ from the raw name's NUL offset; see the
[name-as-format contract](#profile-name-as-format-proof). Fixed offsets 8/12
are not valid for every name, and the second DWORD is elapsed time, not score.

`0044A480(..., false)` uses `"%d. %s - %s %02d:%02d"`: one-based slot,
name, mission label, unsigned `elapsed/3600`, and `(elapsed/60)%60`.
The mission label index is the low WORD of mission plus `00E0`, then
interpreted as signed. On a populated H0 slot the lookup uses that index.
On a lazy miss the native callback pushes full ECX, whose high half still
comes from the save-owner address; that branch requires explicit register
provenance, not invented zero high bits. The short-format branch uses
`"%d. %s"`. The native missing-data name is `"Internal Media Error"`.

Save-owner `+34` is not a portable CRC-only seed. Reader `0044A010` uses
the original FILE* argument stack slot as the four-byte checksum output
when destination is non-NULL. A NULL destination seeks past the copy and
returns that unchanged FILE* value. Pair reader `00449F50` runs twice:
after a successful first read the second call has a NULL destination, so
its return is normally FILE*. `0044A170` sums nine
`abs((return as i32) >> 6)` contributions into `+34`. If only the second
copy succeeds its returned checksum instead contributes to that sum.
The write path computes CRC contributions separately. These values and
the explicit `0`/`-1` action writes must remain distinct.

No read of save-owner `+34` occurs in the draw/header/slot/confirmation
functions above. A downstream visual or gameplay consumer of this token
has not been established by this investigation. A claimed deterministic
visual seed from save bytes alone would exceed the evidence.

## Profile name-as-format proof

Three save-owner calls pass names as the **format operand** to `00511B56`,
without a variadic argument. They share the CRT byte parser, but their cursor
consumers remain different:

| Caller | Destination | Format source | Emitted count plus one selects |
|---|---|---|---|
| `00449C1D`, serialize | Working buffer `owner+24` | Current name `owner+01` | Destination mission/player offset |
| `00449BAD`, restore | Current name `owner+01` | Working buffer `owner+24` | Source mission/player offset |
| `0044A400`, directory | Summary entry `+08` | Working buffer `owner+24` | Source mission/time DWORDs |

The unpassed first variadic word is actual caller support storage, including
saved EDI in the two working wrappers and saved EBP in the directory caller.
It is neither a supplied zero nor an implicit NULL argument. The bounded
no-argument path admits ordinary raw bytes and paired `%%`; conversions,
width/precision arguments and `%n` require separate stack/pointee ownership.

`00514598` reads the format incrementally. The first `%` enters parser state 1
and resets precision at `0051460D`, without classification or output. A second
`%` returns to the ordinary-byte arm and emits one percent. Each ordinary
byte separately loads **current** DWORD `00545C74` at `00514737`, then reads
the high BYTE at `binding + 2*u8(byte) + 1` at `00514743`. Only bit `80` selects
the extra-byte branch; the low classification BYTE is not consumed. High bytes
`80` and `FF` in the format are unsigned table indices and retain their raw
output bytes. Empty format bypasses this classification owner entirely.

The bit-80 branch emits its first byte before reading the next format byte at
`0051475A`. That next byte is emitted without a separate classification or
NUL test. A NUL there therefore does not establish format completion: the
parser subsequently reads past it. Missing format/classification backing can
occur after earlier output stores. Those reached prefixes are diagnostics,
not a portable successful result or a hardware-fault claim.

The CRT wrapper, parser and emitter are the unchanged bodies
`00511B56..00511BA8`, `00514598..00514CB9`, and `00514CD9..00514D0E`.
The intervening `00514CB9..00514CD9` jump words are data. The wrapper uses an
`INT_MAX` stream count, then clears the byte at its actual output cursor with
`AND BYTE [EAX],0` at `00511B91`. It returns the full DWORD emitted count,
excluding that terminator. A 31-byte completed-output admission plus NUL in
32 bytes is a caller-storage bound, not native truncation.

The parser initially reads unknown precision support at `005145BD` and again
at `005145C4` while literals precede the first percent. In state 0 its loaded
ECX is dead before the ordinary-byte arm replaces it with the classification
binding. The first percent's unknown `OR ...,FFFFFFFF` has a deterministic
known write; later precision reads are known. Final output `AND ...,0` likewise
has a deterministic zero write. These are finite instruction/state/address
exceptions, not initialization of the unknown stack or heap.

Raw input `A%%B\0` makes the cursor distinction concrete. It emits `A%B\0`,
returns 3 and gives the native restore/directory reader **source offset 4**,
which is the original NUL byte. The intrinsic
[record codec](SAVE_FORMAT.md#logical-payload-and-serializer-boundaries) uses
raw name length plus one, **offset 5**. Keep `SaveRecord.mission()/player()`
context-free; they cannot substitute for these live cursor consumers. A name
emitted with one percent can become a conversion on a later serialization.
The native callers do not escape it automatically.

The supported PE's startup binding is `00545C7E`; all 256 classification
high bytes in its file-backed WORD table are zero. This authenticates one
initialization, not a current-table fallback. Fourteen inspected literal text
references to `00545C74` are loads; that bounded search does not rule out
computed writes or aliases. The inspected code-page initializer at `005194AC`
populates separate `0065F8C0/0065F9C0` tables. Those tables, the OS code page,
and the grant formatter's two-entry classification input cannot replace this
current 256-entry semantic owner.

Native save-owner construction clears the working pointer before allocation,
then allocates 1,116 bytes without initializing their payload; it does not
initialize current-name bytes either. `0044A0E0` later copies at most 31 name
bytes and writes its terminal NUL, while successful record loading supplies
the complete working payload. A known retained record is a useful bounded
backing domain; it does not establish fresh-heap contents or a live CRT binding.

check_profile_name_format.py authenticates
the full supported EXE, unchanged `.text`, these three bodies, parser tables,
startup classification bytes and twenty individual instructions. It executes
only the three reached CRT bodies with exactly two supplied wrapper arguments.
The three outer caller fragments are static guards, not executed wrappers.
There are no recorded imports, services, supplied results or patched returns;
the inherited particle harness's preparation is outside every recorded call.

The [frozen fixture](../../crates/inf-game/tests/fixtures/profile_name_format_native.json)
contains 27 histories and 29 calls: 15 completed histories with 17 calls, and
12 stopped diagnostics. Completed cases cover empty, raw `80/FF`, high flags
`00/7F`, literal prefixes before `%%`, the exact emitted cursor discriminator,
31-byte output and retained destination tails. Two two-call histories supply
declared current-binding or format changes between requests; they establish
fresh input consumption and tail retention, not original writer execution.
Diagnostics retain earlier stores on missing input/classification, bit-80
trailing-byte paths, overlength output, `%s`, `%n` and width-star frontiers.

Actual cadence is 174 routine entries and 149 authentic returns, including
116 emitter entries, 115 emitter returns and 17 final wrapper NUL writes.
The output-limit diagnostic stops its last emitter before the store/return.
An independent declared-input oracle checks complete bytes/counts; separate
event checks preserve format-read, percent-reset, current-classification and
emitter ordering. Saved registers and known support accesses are checked online;
full masked stack state is retained without a redundant known-stack timeline.
Unknown bytes retain their masks, including finite precision and output RMW
reads. No expected output is applied to the CPU.

Exact verification and independent regeneration reproduced fixture SHA256
`2645cf0023cca41f9922d044f7c63dbe71a1df51185ff42cb9df91e9b1a83569`.
The verification command executes and validates before comparing bytes, and
does not rewrite the frozen fixture:

```text
python check_profile_name_format.py --verify-fixture crates/inf-game/tests/fixtures/profile_name_format_native.json
```

This proof does not attach any of the three enclosing wrappers. Their full
mission/player operations, fresh first-player lookup, failure masks, native
support aliases and registry/disk services retain their own boundaries.

## Results working-profile wrappers

After input selection `004947E0`, the results constructor reads BYTE
`G+1C5 = W+15` at `0042AD46`. This is the existing mission triple
`WorldMission.bytes_14_16[1]`, distinct from mission selection `W+30`.
Zero selects `00449C10`; every nonzero byte selects `00449B80`. Constructor
`0048DA5A` and loader `0048DD47` clear it; catastrophe expiry `0048E4ED`
sets it before completing the world. That expiry writer remains unowned by
the portable world tick, and unavailable mission storage does not mean zero.

Both branches obtain the same root profile owner through `004391B0`, then
root `+660`. Serialization writes the working memory record; this constructor
does not invoke the later disk save. Restore copies the working name, restores
mission state, performs a fresh optional first-player restore, then calls
`004633D0` with null initials. That last operation requires the shared writer's
explicit `Keep` mode; `Write` with an empty slice instead erases initials.
Identity write failure retains the earlier restored name/mission/player.

The [name-as-format proof](#profile-name-as-format-proof) owns the shared
bounded CRT phase and its emitted-length cursor distinction.
Serialization captures the first player only after mission serialization and
ignores serializer Boolean returns. A NULL player skips all player sources.
The intrinsic [record codec](SAVE_FORMAT.md),
[live player save copy](SAVE_FORMAT.md#live-player-save-copy-owner) and
[canonical state-copy phases](SAVE_FORMAT.md#canonical-state-copy-phases)
provide the shared data and signed capacity/cursor policies.

Native caught errors run `004923C0` and still return the constructed screen;
successful branches run `00494A00` to clean the native exception stack. These
are neither a portable rollback nor a disk-write request. The
[results screen construction](INTRO_SEQUENCE.md#results-screen-construction)
now attaches bounded mode0 Serialize and nonzero Restore. The results tick's
save (S) serializes and writes the working profile; its profile menu remains
an explicit skip.

### Live working-profile Serialize

The production constructor lends the same actual mutable `WorldMission` and
arena, then reads mode `bytes_14_16[1]` after input binding. Mode zero obtains
a short loan of `SdlPlatform.profiles` through the existing session services;
there is no replacement store, reopen or prepared snapshot. Missing mode or
store custody has a distinct logged feature boundary. Nonzero mode reaches
the separate Restore policy below.

[`ProfileStore::serialize_current_working`](../../crates/inf-game/src/profile_store.rs)
requires a complete existing working record and the separate current-name
owner. It formats that raw name, uses emitted count plus one, and publishes
the admitted name prefix before mission copying. The mission source reads the
actual current save fields with elapsed seconds projected from the sole
`SessionState` counter. That local projection writes neither a second clock
nor stale `WorldMission.save.elapsed_seconds`. After publishing the completed
mission, it performs fresh `arena.first_player()`, exports that actual player
and calls the shared Player copy only if present. It ignores both native
Boolean returns, retains the full absent-player suffix and preserves the
name31 nested-mask failure. The raw current FORMAT name stays unchanged.
This policy has no world-selection branch and is reusable by later callers.

The private [`profile_name_format`](../../crates/inf-game/src/profile_name_format.rs)
owner admits complete ordinary raw bytes and paired `%%` within 31 output
bytes plus NUL. Conversions, missing reached bytes/classification and the
bit80 branch remain named admission boundaries. A trailing lone percent is
also excluded by this narrower domain, although the native NUL-before-state
dispatch completes it. The formatter publishes complete output only; an
admission error does not establish parity with earlier native partial output
stores. Empty formatting succeeds, but the separate record domain rejects
directory-empty output before mission/player writes or working publication.

`PortSession` seeds one current semantic `00545C74` classification owner from
the same structurally supported, file-backed EXE before first consumption.
It maps the original `00545C7E` WORD table and retains its high-byte custody
across screen/world/display changes. Initialization failure logs and leaves
the binding unavailable, affecting only reached formatting; it supplies no
default table. Runtime parsing does not authenticate the full EXE hash. The
native proof authenticates that original source separately. This logical
startup lineage does not prove all cinit callbacks, computed aliases or global
immutability; a later modeled rebind must update/invalidate this same owner.
Neither SDL locale, the separate CRT code-page tables nor per-call static
EXE reads substitute for current state.

The current-name owner explicitly distinguishes unknown constructor storage
from initialized bytes. Open, scan and load preserve that distinction;
`prepare_new_name` initializes it before later query/Full failure, and the
live Restore operation requires working backing first.
The getter exposes `Option<&[u8]>`, including known empty separately from
unknown. The existing phase-9 prepared-state operation retains its EmptyName
admission, prepared player and named portable zero initialization. Results
does not inherit that fresh-buffer policy.

Reached Serialize failures are reported after releasing the store loan and
contained at this feature: the constructed menu, player-command/raw-WORD
prefix and selected input remain, and the screen becomes Ready. This mirrors
the constructor's continued-screen policy without emulating raw SEH or
unknown partial heap stores. Serialization changes working memory only;
directory cache, selection, disk contents and write-attempt state retain
their custody. The results tick and draw have their own owners
([results runtime](RESULTS_RUNTIME.md#the-general-tick-and-input)).

### Live working-profile Restore

Every nonzero fresh mode reaches the same retained store after constructor
input selection. [`ProfileStore::restore_current_working`](../../crates/inf-game/src/profile_store.rs)
first requires its existing complete working source, then formats that source
into the separate current-name owner. Unknown previous current-name storage
does not block this write. Formatter admission failures retain the previous
name and all state; completed formatting publishes the name before any read.
The original working payload stays unchanged. Emitted count plus one selects
the source cursor, including native cursor4 over raw `A%%B`, whose intrinsic
body starts at5. The emitted single percent is not silently escaped for a
later Serialize. The bounded formatter/classification lineage above applies
to both operations; neither raw aliasing nor partial CRT output is admitted.

The [shared state reader](SAVE_FORMAT.md#canonical-state-read-phases) mutates
the actual `WorldMission.save` fields in source order. Native `W+34` aliases
session+248; the retained `SessionState` is the elapsed authority. A pure
mission phase projects seconds after the second completed field and before
propagating a later error or looking up a player. A first-only range failure
preserves the actual session seconds even when the mission projection is
stale. The scheduler remainder at+658 is never reset. Current-world/header,
visibility, start ordinal and other runtime fields remain outside this read.
This synchronous projection excludes intermediate callbacks/physical alias
observers; extending that domain requires an alias-aware owner.

Only after Mission completes does a fresh `arena.first_player()` select an
optional actual player. Absence skips Player reads but still reaches the
identity writer. A present player seeds the read target from its complete
actual exported state; the existing destination owner publishes reached
blocks even on a later read error. Unreached values remain actual prior state.
No stored `world.player`, default, grant notification, handle reattachment,
loader reset or second player is substituted. The stable pure arena domain
excludes manager rebinding/dangling sources. Restore includes all24 nested
mask bytes at name31; Copy's capacity failure is deliberately separate.

The completed wrapper returns an owned current name so the store loan ends
before the required identity effect. The existing service/factory interfaces
carry `IdentityWriteRequest` with `PlayerName` and `IdentityInitialsWrite::Keep`.
SDL composes the same real `SettingsTextEntryPlatform` from its existing
settings, identity cache and exit flag. Success preserves saved and live
initials independently and clears only cache-loaded; all cache bytes, the
other identity, ordinary settings and display policy remain owned elsewhere.
Failure retains the newly restored name/mission/player and the prior
persistent/live identity and cache-loaded state. This established atomic
sidecar adapter does not claim native registry per-value partial writes.

Missing mode/store remains a logged boundary. Formatting, state-read and
identity-write failures are contained after the store loan ends, with an
explicit Restore label. The constructed menu, selected input and player
prefix remain and the screen becomes Ready. The profile source, disk file,
directory/cache/selection and write-attempt flag are not changed by Restore;
its successful late identity effect writes the separate settings sidecar.
No world22 condition exists in the shared store/reader/writer policies.
Frontend loading (`00487380`) now reaches the same restore. Results factory
eligibility, downstream tick/draw, native SEH/ABI and matched retail
acceptance retain their separate bounds.

## Portable integration boundary

[`ProfileStore`](../../crates/inf-game/src/profile_store.rs) now owns the
portable save file, native directory cache, current name, selected slot, working
record, and write-attempt flag. It validates and copies retail bytes once to a
distinct sidecar; an existing sidecar is authoritative. Missing retail data
starts an empty store. Invalid files never silently replace existing profiles.
The original save is never a write target, including resolved path aliases.

`0044A110` invalidates only the selected directory entry after saving. A normal
count/status query retains the just-saved working record; querying that invalid
entry rescans and leaves the last profile in the working buffer. Copy/delete
perform their native forced refresh. New-name preparation selects the profile
count, and serialization only changes memory. The caller must supply actual
mission/player state and request the later disk-save operation separately.

Native copy naming preserves bytes and the eighteen-byte suffix/digit rules.
The codec retains all owned payload bytes; an adjacent-memory read on a
shortened copy name becomes a zero-filled suffix. A fresh working buffer also
uses explicit zero initialization. Atomic replacement and two checks for
external edits protect prior sidecar bytes; those checks are not a file lock.
A stale staging file is an error, and failures after opening it retain the
native write-attempt flag. Strict portable import rejects malformed remainder
records; the separate codec directory scanner exposes the native accepted
prefix for diagnostics without silently discarding data during a write.

Focused tests cover original-file protection, import precedence, variable-name
serialization, cache/working-buffer lifetime, copy/delete ordering, full slots,
and failed writes. A corpus test imports the installed four profiles into an
isolated sidecar, copies then deletes the appended profile, and checks that both
the original file and the final sidecar retain the original bytes. The SDL
platform owns one store across the derived menu, name entry and return to the
frontend; `--profiles` selects its sidecar.

[`ProfileMenu`](../../crates/inf-game/src/profile_menu.rs) preserves the draw
gate, handshake, timers, live row rebuild, camera callback, selection changes
and confirmations. [`MenuOwner`](../../crates/inf-game/src/menu.rs) separates
`Authored(MenuState)` from `Profile(ProfileMenu)`, which retains its embedded
base plus derived state. Source custody stays explicit: authored definition
VA/span versus derived constructor/vtable and template buffers. Profile rows
are copied per instance; layouts remain shared for process lifetime. Dispatch
through the common controller retains these differences:

| Existing boundary | Authored owner | Profile owner |
|---|---|---|
| Push/replace/pop | Existing `00453440/480/4D0` behavior | Same controller behavior; destroy instance rows, retain shared layouts |
| Command | `00452E20`, existing action adapter | Same base dispatch; profile callbacks route to `004486E0/750/800` |
| Tick after controller fade/drawn gates | `00452470` | `00449690` ordered derived phases, then its base tick |
| Draw after `00453610` setup/fade gate | `00452570` | `004483B0`, including status overlay and successful-tail handshake |
| Back while camera ready | Cancel edit lock, otherwise persist/pop/open Quit | Same base lock policy; derived operation 6 separately requests fade-out |
| Camera-blocked age reset | Base row ages | Embedded base row ages; derived timers do not bypass the existing tick gate |

Base command dispatch retains its axis swap, navigation lock, and action
branch without a new visibility test. Controller fade completion continues
to destroy all owners; the profile operation does not invent an early pop.
Both draw paths retain the existing native-301 presentation behavior.

[`FrontendDispatch`](../../crates/inf-game/src/frontend_dispatch.rs) carries
the actual profile store, screen input, input runtime and effect host through
scoped command/tick calls. Rebuild clears only the screen input's low sixteen
bits before the `004497AD` baseline/FIFO reset. Deferred Start Game pushes the
derived owner; drawing borrows that same owner and store so its mutations reach
the next tick. Camera 9, `0054694C`, cues and controller fade use their concrete
frontend owners; phase requests reach the session controller.

[`profile_draw`](../../crates/inf-game/src/profile_draw.rs) joins the native
header, slot, help, format, confirmation and status consumers to `SoftwareMenu`
and its EffectQuad glyph path. Generated byte strings use the data-bearing
`00453810` row request with explicit left/right placement and descriptor advance;
timer text uses raw `004A3B20` with explicit position, width, font, color and
reveal/emit policy. It does not invent H0 slots for formatted names.
The [native draw fixture](../../crates/inf-game/tests/fixtures/profile_draw_native.json)
checks descriptor/state mutations and text requests with synthetic text extents;
it does not establish native GPU pixels.

[Linked-session tests](../../crates/inf-game/src/session_platform/linked_tests.rs)
exercise the production scheduler, screens, retail resources and CPU renderer.
The Start Game path reaches phase 9, edits the name, and returns through Back to
a freshly constructed phase-2 frontend. The same keyboard adapter, renderer
identity and residency survive that round trip. Completion writes identity,
selects the append index and serializes the prepared mission/player in memory;
the profile save remains unchanged and its disk-write flag stays clear.
The New Game continuation loads the intro and reaches the Level 1 briefing
through the documented
[deadline/reload approximations and the results screen](INTRO_SEQUENCE.md#results-screen-construction).
Other cases check Alt+Escape and rejection of services for a different linked
screen. These tests do not replace matched retail frames or the remaining
player/world behavior owners.
Full profile presentation acceptance still requires all five display tiers and
matched retail frames; the linked renderer tests exercise the 640×480 path.
