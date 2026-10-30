# Infestation live player-message gate

This authority owns the live suppression byte of the player's message
subobject and the early gate in `004135D0`. It is a prerequisite for live
grant notification and pickup messages. Complete intro and briefing acceptance
belongs to the Infestation objective.
The [grant predicate](GRANT_DIFFERENCE_RUNTIME.md) and
[actor broadcast](ACTOR_GRANT_NOTIFICATION.md) retain separate owners.

## Actual subobject and constructor

`00469E6A..00469E75` passes player `+88` to `00413140`. This is the message
receiver, distinct from the player allocation base. The message constructor
places the grant owner at message `+14` = player `+9C`, with six mask DWORDs
at player `+A0..+B7`. It passes the message itself to `00475020`; the grant
notification pointer at player `+B8` therefore names player `+88`.
This relation does not turn a portable player identity into a native address.

The constructor writes message BYTE `+0=0` at `0041314F` and BYTE `+1=1` at
`00413153`. These are distinct fields. BYTE `+1` is the live suppression gate;
player `+58`, grant bits and message BYTE `+0` do not select it.
Other constructor writes include DWORDs `+4/+8/+C=0`, the player/base argument
at `+10`, BYTE `+34=0`, DWORD `+38=0`, and BYTE `+3D=0`. BYTE `+3C` has no
constructor write there. These static writes do not own the enabled message
body or all later field mutations. The constructor itself also invokes grant
setters; retaining its proved gate initialization does not prove those calls.

## Live suppression writers

The timed-pickup draw now delivers its reached message through this same live
owner. `0046B2B0` first acquires the optional detail string, then name and
prefix, and formats the original `%s %s` or `%s %s - %s` into its 256-byte
temporary. The port preserves those H0 reads, raw bytes, current CRT literal
classification and output bounds before checking suppression. A suppressed
message does not excuse missing text or formatter inputs. An enabled
message reaches the [line and hint boxes](#controllers-update-and-display).
A grant's `004750C0` notice
comes first, through the same owner: the record text, then (unless record
flag bit 0) category `0x13B` or `0x13C`, formatted by `00511B56` as `%s` or
`%s: %s`. Healing pickup excess uses
the shared `0047CAE0` nonpositive status arm, including its wrapping LOWWORD
arithmetic; full health bypasses that arm. The existing grant-publication and
class-specific pickup limits remain separate from this message suffix.

The constructor's `+1=1` is not a permanent policy. Original code later writes
both values through real captured receivers:

| Writer | Recovered leaf effect | Enclosing ownership boundary |
| --- | --- | --- |
| `0040631A` | Captured player `+89=1` | Follows first-player, actor and source-handle guards; precedes pickup `0046B2B0` |
| `0040632A` | Same captured player `+89=0` | Follows the returned pickup and player `+58=0` write |
| `0040A947` | Captured message `+1=1` | Screen construction has already registered its widget through `004137D0` |
| `0040AA79` | Same captured message `+1=0` | Screen teardown still has later `00439D50`/`00453400`/`00438080` calls |

The pickup branch captures first player into EDI at `004062CA`. Suppression
`1` precedes the call at `00406321`; suppression `0` occurs only after that
call returns. A lower failure cannot invent a final cleanup that restores it.
The screen constructor captures a player at `0040A8CC` into screen `+EC`;
its teardown uses that same captured player, rather than a later first-player
query. The message address is computed by adding `88h` before each screen
writer. A displacement-only search for player `+89` misses these writers.

Owning the two literal byte-store policies does not execute those enclosing
pickup/screen branches, their guards, registration or teardown. Neither a
scene whitelist nor a constructor-only skip can replace the live byte.

## Send gate and required enabled phase

`004135D0` is a void thiscall receiver with two stack operands, text pointer
and raw flag DWORD, and authentic `RET 8` at `004137C0`. Its full EAX is
incidental; successful delivery at this gate has no numeric return contract.
Portable result parity therefore compares successful void or a required
boundary, without treating incidental native EAX as zero.

The original first installs its SEH frame through FS:0 at
`004135D0..004135DE`, then reads only message BYTE `+1` at `004135EE`.
Any nonzero byte takes `004135F1 -> 004137B0`, restores the SEH chain, and
executes the authentic return. That arm reads no text, flag byte, replay
state, cached string, widget, network or settings, and writes no message
storage. `80h` and `FFh` suppress just as `1` does.

Live zero reaches the BYTE `+3D` read at `004135F7`. The portable gate stops
before this unowned enabled phase. The later native sequence remains:

1. If `+3D=0`, read the flag LOWBYTE at `004135FC` and write it to `+3C`
   at `00413600`, then read cached string `+38`. Free a present old string
   through `005116F4`, scan the input, allocate through `00511708`, publish
   the new pointer, and copy the complete string. Earlier writes and frees
   remain if a later required phase cannot complete.
2. Read the flag LOWBYTE again at `00413653`. Grant notification supplies
   literal zero. Its branch reads widget `+4`; a missing widget then reads
   fresh network DWORD `0055F448` at `00413721`, allocates/constructs a widget
   through `00452310`, writes its returned rows, and registers type 1 through
   `00413230`.
3. Reread message widget `+4` at `004137A7` and call `00413D70` with the text.
   Retaining a string alone does not own this registration/queue phase.

Nonzero flag instead selects widget `+8` and type 2, with further settings
and UI calls. The replay wrapper `004135B0` temporarily writes `+3D=1`, calls
the same message body, then writes `+3D=0` only after return. These fields
and lower heap/widget/text owners remain distinct from the suppression gate.

The grant setter `004750C0` acquires record/category strings and executes
CRT formatting before its late recipient read at `00475270` and message
call at `00475273`. Suppression cannot be inspected early to skip those
effects. The separate grant formatter owns bounded
raw-byte results and reached current NULL/classification data. The shared
[live grant kernel](GRANT_RUNTIME.md) now composes the setter and canonical
H0 acquisition with this gate. Current `PickupMessage` and
respawn message vectors are data requests; they do not execute the enabled
`004135D0` body.

## Controllers, update and display

The three widgets are menu controllers (`00453380` family), created by
`00413230(type, flag, menu)`, which then pushes the menu (`00453440`):

| Widget | Type | Controller | Viewport (1024 units) | Menu |
| --- | --- | --- | --- | --- |
| `+4` | 1, lines | `00413A70(0, 20, 0, 450, 500)`; online `(0, 20, 120, 900, 500)` | top left | rows `0052FB60` (4), header `0052FB20` (`0052FB40` online) |
| `+8` | 2, hints | `00413A70(1, 300, 460, 810, 820)` | centre | rows `0052FBE0` (4), header `0052FB00` |
| `+C` | 3, shown menu | `004140A0(flag)`: `00453380(0x20, 0, 0, 1024, 1024)`, vtable `005207E0` | full screen with a flag, else `[470, 0, 980, 700]` (`00414140`) | the caller's |

`00413A70` controllers keep a four-slot text ring (`+34..+40`, next slot
`+44`). `00413D70(text)` copies the text into the next slot, points that
row's text at it and starts the display time `+30 = max(3 s, 80 ms ×
length)`; a line controller also scrolls `+96` by `0x1600`. `00413F70(delta)`
counts `+30` down, then steps to the next row (`004537E0(1, 0)`) and frees
the slot it left. The update `00413870(delta, flag)` ticks the shown menu
through its virtual `+10` (a zero result deletes it and clears `+0`), then
the lines and hints through `00413F70`; an expired controller is deleted,
and an expired hint under UI state 6 requests phase 4 back. A hint send
(flag 1) requests UI state 6, the paused gameplay phase, when the settings'
pause-on-hints DWORD `+78` is 1 outside attract play. `004137D0(flag, menu)`
shows a menu as widget `+C` and sets `+0`; `004133C0` hides it through its
virtual `+4`, and `004133E0` pops one menu level first.

The draw `00413820(a, b)` draws widget `+C`, then, unless `+1` suppresses
them, `+4` and `+8`, each through the controller draw `00453610`. Its
callers are the briefing draw (`0040B329`) and the player HUD `0046BE00`.
In gameplay the screen draw calls the vehicle's virtual `+84`, `0041ED40`,
at `004414D7`; that runs the vehicle HUD `0041C320` (or
`0041B230`) and then `0046BE00`. Infest01 makes 1255 `+84` calls; the 717 in
Level 1 reach `0046BE00`, always after `0041C320`.

Infest01 sends 34 messages. The first twelve run while `+1` suppresses them:
eleven "New info" notices from the constructor's grants and the respawn's
"Upgrade" pickup text. The briefing constructor suppresses again at
`0040A947`, and its teardown enables at `0040AA79` (`AF0B4B`), just before
Level 1. The other 22 arrive enabled: tutorial hints (flag 1) such as
"Welcome to the MilCon training world" and "Drive over the interface pad to
connect to a factory", all from the command setter `0046C760` (return
`0046C814`), plus notices (`00475278`) and pickup texts (`0046BBDA`) such as
"Collected Green resource crystal - Resources are required for
manufacturing upgrades". Most of the hints' commands come from placed
[selector 50 triggers](PLAYER_COMMAND_RUNTIME.md#selector-50-hint-triggers),
which the port now runs.

The [port](../../crates/inf-game/src/object_runtime/player/message_widgets.rs)
owns the line and hint boxes: the enabled send (`+3C`, the `+38` copy, a new
box over its static menu, `00413D70`), the update `00413870` from the player
update, the select `00413950` (code 4 dismisses a live hint by setting its
display time to 1 and leaves UI state 6), the world cleanup and the
destructor. The session runs the briefing's two stores and supplies the
process values the message reads: the network flag, `G+5C2` for the menu
visibility, the pause-on-hints setting, attract mode and the UI state. The
paused hint phase is described below.

The shown menu `+C` is the type-3 controller
([shown_menu.rs](../../crates/inf-game/src/object_runtime/player/shown_menu.rs)).
`004137D0` builds it (`004140A0`) or gives the existing one the flag's
viewport and reactivates it (`+8`), then pushes the menu; `004133C0` clears
`+0` and fades it out (`+4`). Its tick `00414180` adds the delta's
milliseconds to `+30` and runs the base tick, even in the paused update;
once faded out the update destroys it with its menus and clears `+0C` and
`+0`. The world cleanup `00413A20` deletes a live one the same way. In
gameplay it holds the [factory shop](FAMILY40D800_RUNTIME.md#the-shop-menu-00431000),
whose destroyed menu undocks its factory unless a rebuild replaced it. Under
a shown message `00413950` plays an interface cue (`004391D0`: 2 for codes 0
and 1, 3 for 2 and 3, 5 for the select) and passes the code to the current
menu (`004537E0`). The port queues both: the session plays the cues and
runs the shop's commands with its services at the start of the next world
tick and after the player update, the points where the keys and the `+54`
repeat reach them.
[The box receipt test](../../crates/inf-game/src/object_runtime/player/message_widgets_tests.rs)
replays Infest01's Level 1 from the briefing teardown in recording order:
22 enabled sends, 717 updates and 523 message commands
([player_message_boxes_native.json](../../crates/inf-game/tests/fixtures/player_message_boxes_native.json)).
Every box matches after every call in fade, activity, display time, ring,
panel, menu selection, age, rotating basis and rows. The draw's writes (the
drawn byte `+25` and the row heights) are mirrored between calls, and the
three selects that nest a send from the factory's shop menu are compared
after their nested send.

### Paused hint phase

A hint pauses the game when settings `+78` is 1, outside attract play. The
send requests UI state 6 (`004391B0` → `00438380(6)`), and phase 6 reuses
the gameplay screen (its `+1C` accepts phases 4 and 6). Offline, its tick
callback is `00438950` instead of the world update `004388D0`. It updates
the first player's message with flag 1 (`00413870(delta, 1)`): the shown
menu ticks; the line box does not; the hint box ages only while the UI
state is 6. Then it releases the controlled unit's behavior pair (`+7C` →
`0048D000`) and runs the world's `00487B90`. The world, its clock and the
player update stay still, while the draw and the screen's input go on.
A dismissal (`00413950` code 4) sets the hint's display time to 1 and
requests phase 4 under UI state 6; an expiring hint box requests phase 4 as
well (`00413929..00413940`). Entering phase 4 resumes the world music
(`004872A0`) and stores 500 ms at the first player's `+F4` lockout.

The port admits the 4 ↔ 6 transitions, runs `00438950`'s message update
in the paused tick and turns the message's UI-state requests into phase
requests after the world tick, after player keys and in the paused tick.
The behavior release runs too: `00438950` calls the unit's virtual `+7C`
twice, once for the test and once with the delta pushed early, which is
`0048D000`'s ignored argument (it returns with `RET 4`); `0048D000` then
deletes a held looping sound (`00487FE0`) and clears `+28` and `+2C`, so a
weapon's loop stops while the game is paused. The port's
`take_unit_behavior_sound` does the same on the `+140` behavior that `+7C`
returns, and the tick ends in the music service `00487B90`
([audio](AUDIO_RUNTIME.md)). A Level 1 probe pauses on the first trigger
hint and resumes when it expires.

### The computer and the in-world menu

Both show on the message's type-3 child `+0C` through `004137D0(0, menu)`,
like the factory shop, and run the base menu command `00452E20` there;
`00413950`'s cues and codes reach them as they reach the shop.

Tab (`00469450`) calls `00413410(0)` and sets the player's 500 ms `+F4`
hold. A shown message hides (`+0` cleared, the controller's `+4` fade). Otherwise
`00413410` builds the computer, "Shipboard CPU" (definition `0052FD70`, the
seven rows at `0052FC60`), with `00452310` and shows it; the world keeps
running. Its rows:

| Row | Text | Action |
| --- | --- | --- |
| 1 | Map and Objectives (H0 `0x140`) | `00415170`: `004133C0`, then phase 7 |
| 2 | Image enhancement mode (`0x15A`) | `004151A0`: the owner's `+3F` = 1, then `004133C0` |
| 3 | Repeat last message (`0x165`) | `00415150`: `004135B0`, which resends `+38` with flag `+3C` under the replay gate `+3D` |
| 4 | Access Database (`0x122`) | `00414830`: database menu `00414420` over `0052FA60`, then phase 6 |
| 5 | List Hints (`0x16B`) | `00414720`: hint list (vtable `00520808`, `+5C` = 0x25) over `0052FAE8`, then phase 6 |
| 6 | Cheats Menu (`0x12D`) | `004146B0` over `0052F9D8` ([below](#the-cheats-menu)) |

The first three act on Select (code 4) alone; Access Database, List Hints
and the Cheats Menu check no code, so Left and Right open them too. Each
child shows through `004137D0(0, menu)`, whose unit broadcast follows.

`00413410` rewrites the static row flags first from settings `+84`, the
cheat word: with no bit outside `0xC0` the cheats row's flags (`0052FD20`)
are 0, otherwise `| 1`. The cheats submenu's rows (`0052F888`, a title and
eight rows) follow single bits: bit 1 Skip to, Complete Level and Scavange
Level; bit 4 Show Full Database, Get Full Research Tree and Get Full Ship
Upgrades; bit 2 Invulnerability; bit 0x20 Fill resource crystal tanks. Its
selection (`0052F9E4`) becomes the first enabled row.

Access Database builds `00414420` (vtable `005207F4`) over `0052FA60`: its
entry `+5C` starts at 1, and it writes the title row's text to `0x180` and
the entry row's argument to 1. Its command `00414520` steps Previous (0) and
Next (1) over entries 1 to `0xAE`, skipping each `004144E0` refuses: an
entry the player does not know (`004752E0` on the grant mask `+9C`, entry 0
always known) unless the full database cheat `00546A04` is set, or one
whose flags have 2 (bit 1): 121 entries under the cheat. It then writes
the entry to the row and the
entry's text to the title, and runs the base command. The base command
`00452E20` then moves the selection within the menu's two rows, the title
and the entry. The second layout counts two rows from the entry and wraps,
so Previous wraps from the entry to row 2, past the rows: the definition
`0052FA60` itself, whose first DWORD (the rows' address `0052FA20`) has
neither visibility bit, so it steps on to the entry again. The port reads
those flags as the catalog keeps them for the layout census
([`menu_navigation`](../../crates/inf-game/src/menu_navigation.rs)).
Before, it stopped at row 2 on the first Up, and the shop drain's
containment then stopped every shown menu's keys for the rest of the
world: on an Artoria save the computer looked frozen. That stop
now writes a crash log. Select (4) grants
the entry (`004750C0`, `0046B2B0(entry, 8)`) only under that cheat; every
other code does nothing. The entries are the eight-byte records at
`0053ABD8` below `0x95` and sixteen-byte ones at `0053B088` above
(`00475320`): text, H8 model, flags. The row draw `00414C80` prints the
entry's text (H0 `text + 1`) and, for a known entry or one draw in three
of an unknown one (`0048F060`), its model: at depth three times its extent
plus 500, an eye raised two fifths of that, the fade 2000 and 2500 beyond
it, centred on the controller's viewport (`+26`..`+2C`) top and turned by
the menu's spinning basis. The ACAM's model (`0x75`) sits 50 higher in the
form of entries 2, `0xA5`, `0xA6` and `0xA7` (`00533760` = 2, 4, 5, 3,
otherwise 1). A model the H8 pool cannot load draws nothing: the lazy load
finds only the current world's models, so an entry for a model of
another world shows its text alone. In Level 1 that is eight listed
entries, the Myoon (`0x195`, overlay 35) among them.

The port fixes that by default (`fixes.database_models`,
`--database-models false` for retail's draw). While the database shows an
entry whose model the current residency lacks, the session's tick keeps a
second residency booted and activated at the first overlay whose PreLoad
requests register that H8 slot
([`leaves_registering`](../../crates/inf-game/src/residency.rs), about
50 ms), and the row draws the model on a scene lent over it
(`with_current_resources`), the model's own materials included. The
renderer rebinds when that residency changes, as it does for the main
one.

Every model sits where retail's row puts it, so a large one covers the
description below it, in retail too. On world 2, which loads it, retail's
Myoon stands over the first two lines of its text, and the Spirah's legs
(`0x5A`) reach the line below it. On a world without the model retail
shows the text alone, and the port's fix draws the model in that same
place, so on Artoria the Myoon covers its text as it does on world 2. At
the 4K widescreen output the same place falls on the one line the smaller
text takes. The lab's `database.py`
compares every listed entry's frame with retail's on every world, the
port's fixes off, and [a test](../../crates/inf-game/src/shown_menu_draw_tests.rs)
draws every entry the frontend lacks from another world through the second
residency and from its own world, and requires the same pixels.

The database's models are the world objects' models drawn outside the
world, so they start with the object draw's register seeds
([`ObjectStackHistoryApproximations`](MODEL_RUNTIME.md#database-models)):
the Skimmer's entry (`0xA6`) draws its side wings. A model that still stops
keeps the pixels it reached, as the shown menu always has, and the port
contains the rest: the row restores its clip, the menu's other rows and the
frame's later draws go on, and the log names the model once. Before that,
the stopped scene stayed stopped and the session ended at the next draw
(`session frame N failed: ... scene backend stopped after an earlier
failure`), which is the crash a player saw on opening one entry
([tests](../../crates/inf-game/src/shown_menu_draw_tests.rs)). A live
sweep of Level 1's database with every entry known left two models short.
The Generator (`0x4E`, the thumper H8/46) reads register 1 before it writes
it (in the briefing, the map frame drawn just before sets it; no recording
shows the database's stack). It now takes the object draw's seed
([ThumperModel46StackHistoryApproximation](MODEL_RUNTIME.md#thumpermodel46stackhistoryapproximation))
and draws whole in the tests. The Demons (`0x5F`, H8/638) keep their prefix
at register 10. The Nemian (`0x5D`, H8/308) and the Ichneu (`0x60`, H8/597)
draw their [material faces](PRIMITIVE_RUNTIME.md#material-faces-d0-d4-d8-and-e0).

List Hints builds a hint list (vtable `00520808`) over `0052FAE8` with
`+5C` = 0x25. Its command `00414610` steps Previous (0, 2) and Next (1, 3,
4) over hints 0 to 0x48, skipping the ones the player's `+130` mask has not
seen (`0046C820`), then writes the row and runs the base command; other
codes do nothing. The row draw `00414C20` prints the hint's text, the H0
string `0053A27C` gives it (`0046D350`). The row's argument stays 0 until a
step, so the list opens on hint 0. Both menus' destructors (`00414480`,
`004147D0`) request phase 4, whatever the phase. Opened from the computer
over the running game, they ask for the phase it is already in: `00438380`
accepts a request for the current phase, and `004383B0` keeps the gameplay
screen (its `+1C` accepts phase 4), stores the 500 ms lockout at the first
player's `+F4` and resets the input baselines. The port admits that 4 → 4
transition; before, closing either menu during play ended the session
(`unimplemented transition Phase { previous: Gameplay, requested:
Gameplay }`).

Escape in gameplay (`00443070`) requests phase 5 when no message is shown.
The phase-5 screen `00443190` builds `004433C0(00535348, session +64C)`,
"Paused" with Continue, Options Table and Quit (vtable `00523440`, the phase
it was built over at `+5C`), and shows it. Its key slots send codes 0 to 4
to `00413950` and its Escape `004133E0` pops a menu (`004534D0(1)`),
hiding the message when none is left (`00453540`). Continue (`00443150`)
calls the screen's `+64`, `004133C0` on its message; Options Table pushes
`00539390` (`00453BE0`); Quit pushes its Yes/No `00535280`, whose Yes
(`00443170`) calls the screen's `+54(1, 0, 0)`, a request for phase 2, and
whose No pops (`00453C50`). When the hidden controller fades out,
`00413870` destroys it with its menus, and the in-world menu's destructor
`00443400` requests its stored phase, 6 as 4.

The Options Table there is the frontend's settings tree under its own
six-row definition. Its title row `0045F520` selects a frontend camera
(`00485320`) only under a class-2 screen, so here it is a text row. Its
second layout (`00538D58`) counts thirteen rows from item one, so the
layout census `00452B60` reads the flags of the eight records past the
array; [`MenuData::census_flags`](../../crates/inf-game/src/menu_data.rs)
carries them from the executable, and the census selects no scroll for
every visibility mask. Display, Sounds and Controls bind that same chain,
so they share one copy of its records, scroll state included. Display
(`005390A0`) binds the frontend Video menu's rows (`00538EC8`) too. Under
Vulkan it takes the port's four rows, as the frontend's does (since
2026-10-23), and lays them out on a copy of the chain of its own:
seventeen rows, ten visible. The Options, Sounds and Controls keep the
authored chain.

The [port](../../crates/inf-game/src/session_platform/shown_menus.rs)
shows both menus. The arena runs Tab whole, so the computer shows within the
key callback; the rows' actions go to the session with the menu's owner
(`ShownMenuOwner::InWorld` or `Plain`). Phase 5 keeps the gameplay screen
(`InWorldMenuScreenReuse`): entering it shows the in-world menu with the
phase it left, and the destructor's request reaches the session as a
message UI-state request. The reused screen takes the phase-5 screen's
input
([`enter_in_world_menu`](../../crates/inf-game/src/gameplay_screen.rs)):
a context of its own over the tables `00522B48`, `00534418`, `00522C28`
and `00522CE0`, selected, so the player's message keys go quiet as they do
behind retail's phase-5 screen. The wrappers `004434A0`..`00443590` press
held bits (Up `0x02`, Down `0x04`, Left `0x08`, Right `0x10`, Return
`0x01`, Escape and Tab `0x20`) through `00439430`, whose dispatch `00439470`
reaches the held slots: codes 4 and 0..3 to `00413950`, and Escape's Back
`004133E0`. The screen tick's `004392B0` repeats every held bit each 100 ms
after the first 250. P and Shift+Esc reach the menu key `00443700`, the
screen's `+64` (`004430E0`, `004133C0`), which hides the message whole.
Before, the port sent phase-5 keys through the player's message keys, whose
repeat belongs to the player update, which the pause doesn't run: a held
Right moved the volume once (2026-10-19). The lab holding Right
on retail's Sound row for 16 passes heard 16 adjust cues. Retail's paused
computer pages (phase 6, the database and the hint list) don't repeat: the
lab's held Down there gave one cue, as the port does. Continue, Quit, the
pushes and pops, Repeat last message, Image enhancement mode and Map and
Objectives run; the last opens the
[map screen](GAMEPLAY_RUNTIME.md#phase-7-map-screen). The Options' rows
draw from the live settings as the
frontend's do (value rows `00453AE0`, sliders `00453D50`, the device row
`00462DE0`); the plain rows (`00453B80`) and the volume sliders (`00463AC0`,
`00463B00`) adjust there, and so do the rows with a world refresh
(`00463B50`: Detail Level, First Person, Camera Style and Dynamic
Lighting), whose [`0040F740`](GAMEPLAY_RUNTIME.md#screen-tick-detail) then
gives the world's camera the First Person and Camera Style settings (the
other two are reread where they are used). Infest02 changed Detail Level
this way on Level 1 (`16891E`); before, the port refused those rows.
Retail's Detail Level waits for the next world (`0040F740` only refreshes
the camera); the port reselects the terrain's spacing at once, as Alt+F3
does (`00443020`, `004502C0`; 2026-10-22). Of the rows that
reset the display (`00462F40`), Filtering and the Vulkan rows move as the
frontend's do and take effect from the next frame, under OpenGL and Vulkan
alike: the app applies a change to the device and to the raster's
sampling (`SoftwareScene::set_filtering`) after the frame
(`frontend_video::playing_display_row`). Retail's display reset applied
them at once too. Resolution, colour depth, display mode and the device
(`00463000`, `00463B70`) still need the frontend's reset, refused in
gameplay and logged. The settings file is saved after each change made
while playing; retail saves only on the frontend's Back. The database and
the hint list run as above, and so does the [Cheats Menu](#the-cheats-menu).
`004137D0`'s broadcast of event 6 from the unit (`0047DD20`) runs as the
[factory's Tab and Escape notes](FAMILY40D800_RUNTIME.md#tab-escape-and-the-pad)
describe: a factory within 0x300 re-arms its dock. The computer's children
broadcast too.

The message's keys reach these menus inside the input poll: `0046D2E0`
calls `00413950` in the key callback, and a row's action runs there, before
the pass's transition (`004945A0`: poll, transition, ticks). So a row that
asks for a phase (the database's 6, Skip to's 8) has it taken before the
world updates again. The port queues the key's command in the callback
and runs the queue, and the rows it reaches, at the end of the poll
([`after_input_poll`](../../crates/inf-game/src/session_platform/shown_menus.rs));
before, it ran them inside the next world tick, one tick late.

### The Cheats Menu

`004146B0` builds a plain menu (`00452310`) over its row's argument 1,
`0052F9D8`, and shows it through `004137D0(0, menu)`, whatever the code.
Its rows (`0052F888`) are a title and eight rows, whose flags `00413410`
rewrote when Tab built the computer: with any cheat bit outside `0xC0`,
each row keeps bit 0 when its bit is set and is 0 otherwise, and the
selection `0052F9E4` becomes the first row whose flags are exactly 1. With
none, the rows and the selection keep what the last build left.

| Row | Text | Bit | Action | It does |
| --- | --- | ---: | --- | --- |
| 1 | Skip to (`0x12E`), with a world's name | 1 | `004148B0` | Left and Right step the world `0052F880` (2 at start) through 2 to 24; Select goes there (below) |
| 2 | Show Full Database (`0x132`) | 4 | `00414AD0` | Flips the byte `00546A04`, whatever the code |
| 3 | Get Full Research Tree (`0x133`) | 4 | `00414AE0` | `004750C0` of every ID 0 to `0xAE` on the message's `+14`, the player's `+9C`, whatever the code |
| 4 | Get Full Ship Upgrades (`0x134`) | 4 | `00414BA0` | Select: the research tree, then `0046B2B0(id, 8)` for every ID but Invulnerability (`0x29`) |
| 5 | Invulnerability (`0x167`) | 2 | `00414B80` | Select: `0046B2B0(0x29, 0)`; owning `0x29` makes the vehicle immune |
| 6 | Complete Level (`0x168`) | 1 | `00414B00` | Select: `00488AC0` with CL 0 |
| 7 | Scavange Level (`0x169`) | 1 | `00414B20` | Select: `00488AC0` with CL 1 |
| 8 | Fill resource crystal tanks (`0x16F`) | `0x20` | `00414B40` | Select: `0046B2B0(0x0E/0x0F/0x10, 20)` on the first player, the three tanks to 30 |

Skip to's row draws `00415090`: "%s %s" (`0052FE2C`) of its text and the
name of the world in its argument 1's WORD (H0 `0xE0 + world`).

Under `00546A04` the database (`004144E0`) lists every entry but those
with flag bit 2, and its Select (`00414520`, code 4) grants the shown
entry: `004750C0(entry)` on the player's `+9C`, then `0046B2B0(entry, 8)`.
Nothing else reads the byte.

Skip to's Select (`004148B0`), for `00488AA0`'s first player:

1. The message's `+1` (EDX's) silences it.
2. The world's stage, from `0052FDCC`: worlds below 100, each stage opened
   by `100 + stage` and the list ended by a byte from 200; a world no stage
   names takes the last stage opened (20). Command Centre (18) and Terra
   Nova (2) are stage 1, Oolbryte (20) 2, Exxodus 3, Altair Prime 4, and so
   on to the Alien Brain (19), stage 19.
3. Every grant ID of the stages up to it (`0052FD84`: IDs, each stage closed
   by `0xDC + stage`, the list by a byte from `0xFA`): `004750C0(id)` and
   `0046B2B0(id, 8)` each.
4. The seven token levels (player `+13C` plus each offset of `0052FE0C`,
   ended by -1) become the first `stage` tokens of `0052FDF8` summed, over
   7, at most 100.
5. The unit of the player's `+D4` (`0046B030`; no NULL test), through its
   `+64` vehicle view: while its hold has a slot with the process NULL
   (`004120E0`), a scientist (H13 resource 2, `00460310` with no rotation,
   placement -1, mode 0) is built 0x200 along X and Z from the vehicle and
   taken into the hold (`00421810`).
6. `0046B2B0` of the three crystal counters, 20 each; the message's `+1`
   back to 0; `00487560(world, 1, 0)`, the world's end into the chosen one;
   `004133C0` hides the message.

Complete Level and Scavange Level (`00488AC0`) walk the world's flag-0
list (`[G+10]`) for the first player, reading each node's next link before
its object, so the walk goes on from that node in whatever list a death
has moved it to: a disposed next node sits at the end of the parking
list, and the walk takes it and stops. On Level 1 a death in the sweep
disposes the barrier (selector 69) that follows it in the list, so the walk
ends there. For each object:

- An actor (`+18`): under Scavange Level (CL 1) its `+7A` goes to
  `004750C0` and `0046B2B0(+7A, 4)`. Resources 0x12 and 0x13 (course
  targets) teach their `+7A`. A factory (`+50`) gives its grants
  (`00475350` of its `+198`) and `0046B2B0(id, 4)` of each ID its `+198`
  holds (ID 0 included). An actor of the aliens' side (`+B4` 0x12) that is
  no factory and not resource 0xF is retired (`0047CE90`).
  Otherwise a scientist (`+5C`) teaches and gives its `+7A`, and boards the
  vehicle of the player's unit (`0046AE60`, no NULL test; `00421810`).
- A pickup (resource 9): `0046B0F0`, the pickup's `0046B2B0(+44, +46)`;
  when it is taken, `0048E980` marks its `+48` collected in the world's
  mission bits, `00489A90(3)` flashes at it and it is disposed.

The port runs the menu and every row
([cheats](../../crates/inf-game/src/session_platform/cheats.rs)), with
the session owning `00546A04` and `0052F880` across worlds
([`ComputerCheats`](../../crates/inf-game/src/object_runtime/player/message_widgets.rs)).
The sweep's retirements reach deaths the port had not run: selector 60's
and the barrier's (`00404600`, a disposal) and the gate's (`0048C630`,
[gates](SELECTOR30_RUNTIME.md#death)) and, since 2026-10-18, the Alien
Brain's eyes' (`00408560`, [the eyes' death](ALIEN_BRAIN_RUNTIME.md#the-eyes-death)).
One stops where it reaches a held piece, after the writes before it:
Shak'tahh's stabiliser, whose link `+16C` retail leaves to heap residue
(`StabiliserLinkResidueBoundary`, [the stabiliser](TERRAIN_RUNTIME.md)).
Every other world runs every row to its end
([tests](../../crates/inf-game/src/session_platform/cheats_tests.rs)).
In the lab (`scenarios/cheats-*.json`, the cheat word `0x27`, keys from
Tab through the row), retail and the port match in every compared event,
with `--fresh-heap --lockstep-rng`: Complete Level on Terra Nova, Level 1
and Sanrel-Dek, Scavange Level and Skip to (to Altair Prime) on Terra
Nova, the three upgrade rows, and Show Full Database with the database's
Select.

The port's option `all_cheats` (`--all-cheats true`, or the launcher's
Options, Cheats; off by default) opens all eight rows as if the four
name cheats had been entered (`CHEATS_MENU_BITS`, `0x27`), without
changing the saved cheat word
([`PortFixes::menu_cheat_word`](../../crates/inf-game/src/frontend_settings.rs)).

With all eight open the port shows a copy of the menu with a ninth row,
its own Teleport to world, in the row group retail sized for nine
(`0052F9B8`: nine rows, eight used). Its list reuses the menu's title and
layouts, the row group scrolling nine of 25 worlds. A world's Return
stores the selection with start ordinal 1 and requests phase 3, which
the port accepts from gameplay for this alone: the world loads and its
briefing shows ([`teleport`](../../crates/inf-game/src/session_platform/cheats.rs)).
Entering phase 3 from gameplay, the world left first ends as `00487540`
ends one before its results (`0048E890`), so its objects' cleanups run
while its terrain is still the world's: a furniture marker's `00435360`
looks there for its fence. Without that, the next world's load drained
the world left over the new terrain, and Helniazine's marker stopped the
session (a crash, 2026-10-24).
`00487560` would mark the world left done and show the results page,
whose selection 1 is the campaign's final page (its model `0x2B3`).

### Box draw

The controller draw `00453610` sets up its context and light, calls the
controller's virtual `+0C` (at any fade), then draws the current menu at
full fade and sets `+25`. A box's virtual `+0C` is `00413F30`: offline it
begins a batch (`004A4400`), draws the panel frame `00455080` from the panel
at `+4C` and flushes (`004A4460`). The constructor gives that panel H8 slot
99 (`+9C = 1`), callback flag `+9D = 0`, centre `+88/+8A` and orientation
`+A0`. After the frame's fixed YZ quarter turn, orientations 0, 2 and 3 turn
it in XZ (`00491530`) by -0x4000, 0x4000 and 0x8000; 1 leaves it. Lines
(orientation 2) are centred at the top of their box and hints (orientation 1)
at its left edge. The rows' callback `00414BF0(…, slot, ring)` draws the
ring text in the row's slot with `00453810`, left-aligned, when the row
points at the ring.

Infest01 (1024×768, focal words 512) confirms the frame inputs at the end
of `00455080` on a box's panel. The hint box at `AF5E35:11EB` draws slot
`0x63` at `(0, 0, 0x2580)` with basis `1 | 1 0 0 | 0 0 -1 | 0 1 0` and
publishes focal `0x4000`, viewport 1024×768 and centre `(300, 480)`. The
line box at `B0D81D:14C1`, with two lines (scale `0x100` toward `0x2D00`),
draws the same slot and position with basis `2 | 0 -1 0 | 0 0 -1 | 1 0 0`
and centre `(235, 0)`.
[panel_frame_tests.rs](../../crates/inf-game/src/panel_frame_tests.rs)
pins the port's frame submissions to these words.

The port draws the boxes at the end of the gameplay draw
([player_hud.rs](../../crates/inf-game/src/player_hud.rs),
[player_message_draw.rs](../../crates/inf-game/src/player_message_draw.rs)).
For each player's unit (`0046AE60`), the vehicle's `0041ED40` looks up its
player (`0046B090`) and, when the vehicle is in the draw list (`+2C` bit 0),
draws that player's message (`00413820`). The
[panel frame](../../crates/inf-game/src/panel_frame.rs) is the briefing
panels' `00455080`, now shared. The vehicle's own HUD `0041C320` draws first
([vehicle HUD](VEHICLE_DRIVE_RUNTIME.md#the-vehicle-hud-0041c320)). The
shown menu `+C` draws first, even when `+1` suppresses the boxes
([shown_menu_draw.rs](../../crates/inf-game/src/shown_menu_draw.rs)). The
controller's `004141B0`, at any fade, frames a viewport narrower than
0x400 with a panel: the viewport widened by 0x78 left, 0x12C up, 0xC8
right and 0x78 down, scaled to the surface (truncating), as four quarters
of sprite `0x2A6` (+9C, vertex DWORD `0x4000`, descriptor 2) meeting at its
centre. `004141B0` writes neither X of the third quarter's first two
corners; Infest01's draws show both at the right edge, where the second
quarter's +9C left the shared packet. Text rows (`00453A80`) draw as every
menu's; the shop's own rows draw as the
[shop notes](FAMILY40D800_RUNTIME.md#drawing-the-shop) describe, and the
player commands they set reach the message before its boxes draw, as
`0046C760` does during the native draw. Free look's view `0041B230`
draws instead of the vehicle's HUD
([free look](VEHICLE_DRIVE_RUNTIME.md#free-looks-panel-and-the-guided-missiles-view)). The rest of `0046BE00` draws nothing in the campaign. Past
`00413820`, a count of the world's type `0x12`/`0x13` objects ("%s %d"
with H0 `0x112`) needs H14 world flag `0x800` (`PUSH 0x800` at `0046BEF4`,
`0048EDE0`), which no retail world sets (leaves 1 and 3 to 43); the rest
needs online play (`0055F448`, `0046C010`): the network message lines and
the race's status
([NETWORK_RUNTIME.md](NETWORK_RUNTIME.md#the-network-game-on-screen)).

## Constructor-owned empty destruction

`00413140` explicitly clears children `+04/+08/+0C` and backing `+38`.
The message owner retains those known NULL fields independently from BYTE+1.
The [published player destructor](PLAYER_CONSTRUCTION.md#published-destruction-and-component-cleanup)
unlinks the actual player before invoking `004131F0` on its retained message.
That destructor checks `+0C`, `+04`, `+08`, then `+38`; its NULL branches skip
child deletion/backing free and perform no NULL stores. Unknown storage remains
a required boundary. The current enabled gate stops before storage writes,
and Enable itself only changes BYTE+1, so empty destruction also remains
available after a failed enabled send. Suppression is never a storage sentinel.

The distinct world-cleanup body `00413A20` frees backing first and then deletes
and clears children `+0C/+04/+08`; its BYTE+0 clear is conditional on `+0C`.
`PlayerMessageOperation::CleanupWorld` owns its complete constructor-known
NULL domain independently of `004131F0`: backing `+38` is reached before
children `+0C/+04/+08`, with an explicit error for the first unavailable field.
Every free, pointer clear and BYTE+0 clear is inside a nonnull branch, so the
empty operation performs none of those stores. BYTE+1 is unchanged even after
an enabled send has stopped. The same authenticated live message/player FIFO
survives repeated cleanup; no constructor or deleting destructor supplies it.
Complete widgets, cached text, nonempty destruction/cleanup and enclosing
production world-cleanup attachment remain separate owners.

## Portable custody and boundaries

The [message owner](../../crates/inf-game/src/object_runtime/player/message.rs)
belongs to the actual live `WorldPlayer`. Its message identity retains the
arena identity and actual player identity, so tokens from different arenas
cannot collide merely because both player lists begin at zero. Appending
another player does not retarget a captured message to a new first player.
The existing player identity and other APIs retain their current semantics.

A single request names the message and an explicit operation: suppress,
enable, send or world cleanup. Suppress/enable write the proved literal values 1/0; send
borrows text backing and retains the raw flag operand. Receiver authentication
precedes the gate, and unused lower inputs stay unused on suppression.
An enabled message requires its separate state/service owner. The gate
introduces no arbitrary raw suppression setter, heap default, successful
message stub, broad player mutator or production caller attachment.

Native address provenance, grant notification-pointer rebinding, late NULL
recipients, raw pointer aliases, player allocation reuse and foreign raw
message storage remain separate evidence domains. The native SEH carrier
is execution support; portable game-state custody does not model that stack
frame or assert native incidental-register parity. Shared Set/Merge/Copy has
its separate live kernel. The shared
[player-command kernel](PLAYER_COMMAND_RUNTIME.md) uses the requested player's
actual `+88` and flag 1 after H0 capture. Enclosing command/capture/pickup/screen
composition, enabled messages and matched retail acceptance remain open.

## Focused verification

The [owning tests](../../crates/inf-game/src/object_runtime/player/message/tests.rs)
exercise the actual message owner alongside the existing player regressions.
Actual constructors produce distinct arena-scoped message identities and
BYTE `+1=1`. The tests cover foreign coincident player IDs, an absent message,
captured second-player identity after another append, live literal controls,
and suppressed sends with absent or unterminated text and high raw flag bits.
Known bytes `1/80h/FFh` all bypass lower inputs. Live zero retains its state
on the required enabled boundary; no cleanup restores suppression.

Actual save-prefix restore and the owned `0046AF70` completion-checkpoint reset
preserve the same enabled message. They do not simulate world cleanup.
The distinct cleanup test invokes the same authenticated retained messages,
checks repeated constructor-NULL cleanup and preserves the enabled-send error.
Private scalar-backing tests distinguish cleanup's backing-first error order
from the deleting destructor's child-first order, independently of suppression.
Full player records and actor/list/registry/cache/allocation custody are checked
with only the requested suppression byte allowed to change. The gate needs
no registry or process owner, and does not publish a numeric return value.

```text
cargo test --release -p inf-game --lib object_runtime::player
```

## Original-byte evidence

check_player_message_gate.py authenticates
the supported EXE, full unchanged `.text`, FPCW `027F`, complete
`004135D0..004137C3` body, and ten instruction guards. It executes the original
`0040A947`/`0040AA79` single byte-store instructions and stops at their next
instruction. Constructor `00413153` and pickup `0040631A`/`0040632A` remain
static provenance. There is no instruction patch, service stub or supplied
formatter result; inherited harness preparation is outside the recorded calls.

The [native fixture](../../crates/inf-game/tests/fixtures/player_message_gate_native.json)
contains ten histories and nineteen requests. Eight admitted histories contain
nine completed suppressed sends, five completed original writer fragments, and
three deliberately stopped enabled-message prefixes. The two exact diagnostics
cover unavailable suppression backing and an unmapped raw receiver. Neither
establishes a portable default or hardware-fault parity, and no fault is observed.
The enabled prefix stops before `004135F7`, even in a case with known replay
BYTE `+3D=1`; it is a required-owner boundary, rather than an unknown-field claim.

There are fourteen original routine entries and nine authentic `RET 8`
completions, thirteen payload reads and five literal payload writes. The five
single-instruction writer completions are counted separately from routine
returns. Full supplied byte masks and unused player/list/process sentinels are
retained. Native NULL/unmapped text operands remain unread; absent portable byte
backing does not imply native pointer identity or enabled NULL semantics.

The original SEH protocol records fifty support reads and ninety-three support
writes. Each send reads FS:0, pushes the original frame/handler and supplied
saved registers, installs its frame, then reaches the live message byte.
Completed sends restore the original chain and EBX/EBP before authentic
`RET 8` and ESP delta 12. Stopped prefixes retain their original setup writes.
The next independent request explicitly supplies a new invocation carrier;
there is no hidden prefix rollback. This is a flat supplied FS:0 carrier,
without exception-handler, Windows TIB or dispatcher execution.

Repeated completed sends preserve incidental EAX values `12345678h` and `0`
from their respective supplied old chains. Both are successful native void
returns; they do not become a portable numeric result contract. Native support
timelines remain separate from portable game-state custody and cadence claims.
Player/message constructors, enclosing screen/pickup bodies, lookup, resource
loading, CRT formatting, enabled UI and production attachment are not executed.

Independent regeneration reproduced fixture SHA-256
`56841BB28DE3C4C73B79F4B4BF16D48DA0068195C2848AE31AB35768DA76A1F8`.

```text
python check_player_message_gate.py --output .tmp/player-message-gate.json
```

The [Rust comparison](../../crates/inf-game/src/object_runtime/player/message/native_receipts.rs)
passes in release mode. It first audits every native receipt's exact ordered
payload, SEH support and return events against complete supplied masked backing.
It then executes the public message operations on two genuinely constructed
portable players for the eight admitted histories and seventeen operations:
nine successful void sends, five literal controls and three required enabled
boundaries. Both diagnostics are excluded before portable player construction.
Whole player records and all other arena owners retain their custody, allowing
only the requested control byte to change. Expected receipt outputs are never
applied to the actual live owners. This establishes result/state parity within
that domain, without a Rust cadence observer, native pointer/register parity or
enclosing caller execution.

```text
cargo test --release -p inf-game --lib player_message_gate_matches_native_suppression_histories
```
