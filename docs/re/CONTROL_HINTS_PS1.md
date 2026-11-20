# Infestation control hints: PC retail and the PAL PlayStation release

This note answers how the PC game words its in-game control hints, what the
PAL PlayStation release (SLES-01870) says instead, and how the two line up. It
is research only: no game code changes.

**Summary.** PC hints are ordinary Handler-0 strings. They hard-code a mode
number, a key name or an action name; they are never formatted from the live
key configuration. The PlayStation release carries a byte-identical 73-word
hint table and the same string IDs, but its level-1 language banks replace six
hint records (IDs `0x43..0x48`, globals 865..870) with pad wording: `L1`,
`Select`, Cross and Circle. The ID mapping is the PC's, so the console text is
a per-ID content swap, not a different hint system. The same is true of the
four system prompts at globals 37..40. All six languages follow the same
pattern.

## 1. How the PC puts a hint on screen

### The hint command and its text

The game has one 73-word table at `0053A27C` (`0046C760`'s text source,
`0x49` words). A hint is a *player command*: `0046C760(player, id, silent)`.
`id` is the table index, 0 through 0x48.

1. `0046C760` returns early when the command is already set in the player's
   three-word seen mask `+130` (`0046C820`), or when the process network flag
   `0055F448` is set.
2. It sets the mask bit (`0046C7AD`).
3. If `silent` is zero, it reads the H0 string word at
   `0053A27C + id * 2` (`0046C7C4..0046C804`), resolves it through
   `0046D350` from the resident Handler-0 string bank, and sends it to player
   `+88` with flag 1 through `004135D0`.
4. Flag 1 selects the **hint box** `+8` (type 2,
   `00413A70(1, 300, 460, 810, 820)`, centred). `00413230` builds the box,
   `00413D70` copies the text into its four-slot ring, and `00413870` ages it.
   With the pause-on-hints setting `+78`, the send also requests the paused UI
   state 6.
5. The same text is drawn by the computer's **List Hints** page
   (`00414720`): its row draw `00414C20` reads `0053A27C[hint]` and prints
   that H0 string.

The Hint-0 bank itself is registered by `FUN_0050ECD0` from the current
level's language OVL. `PreLoad.dat`'s Handler-0 request maps its local
records to global IDs through `FUN_004EFC00`'s segment deltas; for level 1 the
180 local records map 1:1 to globals 692..871.

### The keys are literal, not formatted

This is the important part. The hint path has **no formatter and no key-name
lookup**:

- The six differing English hints contain no `%s` or `%d`, and the port's
  setter comment records that this phase has "no formatter or late +1C
  binding" (`object_runtime/player/commands.rs`).
- The strings name a **mode number** ("mode 2"), a **key** ("Enter"), or a
  **generic action** ("the turbo key", "the beam down key"). None of these is
  read from the key configuration at `00535658` / records
  `00534CA0`/`00534CA4` or from the joystick defaults at `00534E18`.
- The translations prove the key is literal. English says "Press mode 2"; the
  French says "Appuyez sur la touche 2" ("press key 2"), German "Drücken Sie
  Taste 2", Dutch "Druk je op stand 2". The number 2 is authored, not a
  formatted binding.
- Rebinding therefore does not change any hint text. The defaults merely make
  the text true: `Buggy` is DIK `0x02` (the `1` key), `Truck` DIK `0x03`
  (the `2` key), `Thrust` DIK `0x39` (`Space`, the turbo), `Beam Down` DIK
  `0x20` (`D`), `CPU Select` DIK `0x1C` (`Enter`). `Caps Lock` (DIK `0x3A`)
  is the separate `Mode` action (`+34 = -1`, "the next owned mode",
  `00469580`); `1..5` set modes 1..5 directly (`004695A0..00469620`).

The four system prompts in the common bank are literal too: `Press 'S' to
save`, `Press <Esc> or <Space> to continue`, `Press <Enter> to carry on, <R>
to retry`, `Press <Enter> to continue, <Esc> to cancel`.

### Files and IDs

| Use | PC file | Local records | Global H0 IDs |
| --- | --- | --- | --- |
| Level-1 hints | `Overlay/1l0.ovl` .. `1l5.ovl` | 180 per language; hint block at local 173..178 | 865..870 (level-1 range 692..871) |
| Hint-list index | `Infest.exe` `0053A27C` | 73 words, ids 0..0x48 | 692..757, 368, 865..870 |
| System prompts | `Overlay/0l0.ovl` .. `0l5.ovl` | local 31..34 | 37..40 |

The PC hint command table at `0053A27C` is exactly:

```
692 693 694 695 696 697 698 699 700 701 702 703 704 705 706 707 708
709 710 711 712 713 714 715 716 717 718 719 720 721 722 723 724 725
726 727 728 729 730 731 732 733 734 736 737 738 735 739 741 740 745
744 742 743 746 747 748 749 750 751 752 753 754 755 756 368 757 865
866 867 868 869 870
```

(The table's order is authored, not sorted: hint 0x2B is 736, 0x2C is 737,
0x2D is 738, 0x2E is 735, and so on. The last six entries are 865..870.)

## 2. The strings

All 73 hint entries were compared in English and in the other five language
banks. Exactly six differ: command IDs `0x43` through `0x48`, globals 865
through 870. The other 67 entries are byte-identical, including the hints that
name an action without naming a key (for example `0x19` "Use the detonator on
your main weapon ring to detonate a bomb", `0x22` "Use big wheels to cross to
the other side", `0x3D` "Drop a man to activate the orbital array beacon").

### 2.1 The six differing hints (English)

| Command id | PC string ID (global) | PC `1l0.ovl` local | PC English | PS1 English | What differs |
| --- | --- | --- | --- | --- | --- |
| `0x43` | 865 | 173 | "Most factories require scientists to run them - beam them into the factory from your ship to staff it" | "Most factories contain scientists who will beam out when you get close. Beam them on to your ship with R1-\x1a." | Whole sentence reworded; PS1 adds the beam-up chord R1-Circle |
| `0x44` | 866 | 174 | "Press mode 2 to activate big wheeled mode" | "Press L1 to activate big wheeled mode" | Mode number vs pad button; PS1 is a toggle |
| `0x45` | 867 | 175 | "Press mode 1 to return to normal buggy mode" | "Press L1 again to return to normal buggy mode" | Mode number vs pad button; "again" makes the toggle explicit |
| `0x46` | 868 | 176 | "To select an item use up, down and Enter" | "To select an item use up and down and Select (\x18)" | Keyboard `Enter` vs pad Select/Cross; comma list reworded |
| `0x47` | 869 | 177 | "Use the turbo key to activate turbo booster" | "Use \x18 to activate turbo booster" | Generic action name vs Cross button |
| `0x48` | 870 | 178 | "Drop scientists close to a factory with the beam down key and they will go to work" | "Drop scientists close to a factory (\x1a) and they will go to work" | Generic action name vs Circle button |

`\x18` is the PS1's Cross glyph and `\x1a` its Circle glyph; see
[section 4](#4-how-each-layout-fits). The PS1's command table uses the same
global IDs, so these are content replacements at 865..870.

Bank record 179 / global 871 also differs, but it is **not** in the hint
table (the table ends at 0x48): PC "Save failed - please check write
privileges" becomes "Save failed - please check your memory card". It is a
platform wording change, not a control hint.

### 2.2 The four system prompts (common bank)

These are `0l0.ovl` globals 37..40 (PC local 31..34). They are control
prompts, not hint-list entries, and the PS1 rewords all four:

| Global | PC English | PS1 English | What differs |
| --- | --- | --- | --- |
| 37 | "Press 'S' to save" | "Press \x17 to save" | `S` key vs Start glyph |
| 38 | "Press <Esc> or <Space> to continue" | "Start or \x18 to continue" | `Esc`/`Space` vs Start/Cross; "Press" dropped |
| 39 | "Press <Enter> to carry on, <R> to retry" | "Press \x18 to carry on, \x17 to retry" | `Enter`/`R` vs Cross/Start |
| 40 | "Press <Enter> to continue, <Esc> to cancel" | "Press \x18 to continue, \x1a to cancel" | `Enter`/`Esc` vs Cross/Circle |

The PS1 shell bank has three extra IDs next to these: 46 "Press \x17 to
retry", and 47/48 about inserting or changing a MEMORY CARD. Global 39 in the
PS1 bank already combines carry-on and retry; 46 is a standalone retry string
with no PC counterpart.

### 2.3 The other five languages

All six level-1 banks have 180 records and the same local-to-global map
(record *i* -> 692 + *i*). In every language, records 173..179 are the only
differences, and records 174..178 carry the same five control substitutions.
The "mode 2" row shows the pattern:

| Language | PC | PS1 |
| --- | --- | --- |
| Français | "Appuyez sur la touche 2 pour passer en configuration camion." | "Appuyez sur L1 pour passer en configuration camion." |
| Deutsch | "Drücken Sie Taste 2, um den Truck-Modus zu aktivieren" | "Drücken Sie die L1-Taste, um den Truck-Modus zu aktivieren" |
| Italiano | "Seleziona la modalità 2 per attivare la modalità Bigfoot." | "Premi il tasto L1 per attivare la modalità Bigfoot." |
| Español | "Pulsa el modo 2 para activar el modo de ruedas grandes" | "Pulsa L1 para activar el modo de ruedas grandes" |
| Nederlands | "Druk je op stand 2 om de terreinwagen te activeren." | "Om de terreinwagen te activeren, druk je op de L1-toets." |

The shell prompts 37..40 also follow the same pattern in all six languages
(Start/Cross/Circle). One unrelated Italian shell difference exists at global
284: PC "Configura controlli da tastiera" ("configure keyboard controls")
becomes "Costruzioni principali" ("main constructions"), consistent with the
PS1 having no keyboard configuration. It is not a control hint.

### 2.4 Other control-worded strings (not hint-list entries)

A scan of every English OVL for "press", "key", "button", `<Esc>`, `<Space>`,
`<Enter>` and similar found no other in-game control hint. The remaining hits
are:

- `0l0.ovl` global 273 "Press Fire" — identical in the PS1 bank. It is an
  attract/network prompt, not a tutorial hint; the PS1 did not adapt it.
- `0l0.ovl` global 137 "Press F10 to start" — PC-only; the PS1 shell bank has
  no equivalent.
- `47l0.ovl` "Press new key" — the key-configuration capture prompt.
- Briefing prose that uses "start" as a verb (for example "you will start at
  the old transport ring"), not a control name.

## 3. Formats and IDs: the PS1 is a per-ID content swap

### Container

The PS1 disc's data track (raw 2352-byte Mode 2 sectors, user data at +24)
contains `SYSTEM.CNF`, `SLES_018.70`, `GAME.EXE`, `PRELOAD.DAT`, `MAIN.IDX`,
`MAIN.OVL`, `BANNER.STR`, `LICENSEE.DAT`. `GAME.EXE` is the PS-EXE (load
address `0x80010000`); `MAIN.OVL` (12,919,919 bytes) holds every overlay
section concatenated; `MAIN.IDX` is the section table.

`PRELOAD.DAT` parses with the PC `PreLoad.dat` grammar except that its header
omits the PC-only `handler_3_allocation` DWORD (PS1 header 35 bytes, PC 39).
It still has 46 leaves with the same overlay IDs and the same request-table
shape.

### String banks

The Handler-0 payload format is identical: a little-endian `u32` payload size,
then NUL-terminated records. The level-1 language banks are contiguous and
chain exactly by their size words:

| Language | `MAIN.OVL` size word | Strings start | End |
| --- | --- | --- | --- |
| English | `0x162482` (`0x1972`) | `0x162486` | `0x163DF8` |
| Français | `0x163DF8` (`0x1F0A`) | `0x163DFC` | `0x165D06` |
| Deutsch | `0x165D06` (`0x1DF4`) | `0x165D0A` | `0x167AFE` |
| Italiano | `0x167AFE` (`0x2133`) | `0x167B02` | `0x169C35` |
| Español | `0x169C35` (`0x1D48`) | `0x169C39` | `0x16B981` |
| Nederlands | `0x16B981` (`0x1DC4`) | `0x16B985` | `0x16D749` |

The PS1 `PRELOAD.DAT` leaf 1 language request is identical to the PC's:
180 records, one segment `(local_start 0, delta 692)`, so local record *i* is
global 692 + *i*. The PS1 bank order matches the PC bank order: 173 of the 180
English records are byte-identical, and the 7 reworded records sit at the same
local indices.

The PS1 executable carries the same ID mapping. `GAME.EXE` has a 73-word
table at file offset `0x10082C` (RAM `0x8011002C`) that is **byte-for-byte
identical** to the PC table at `0053A27C`, including the `368` and `865..870`
entries. Whatever the PS1 code calls this table, the hint IDs were not
renumbered; the console's bank supplies its own text at those same global IDs.
A per-ID swap is therefore the console's own data model (its setter's use of
this table is inferred from the identical contents, not disassembled).

### Shell bank caveat

The common/shell bank is not a straight mirror. PS1 English has 561 records
where PC English has 684, with a different local order and different segment
map (PS1 globals 37..39 come from locals 0..2, global 46 from local 3, global
40 from local 4, then 139..367, 355, 368..691). Of 557 common global IDs, 553
are byte-identical; only 37..40 differ. So per-ID replacement works for the
prompts, but a per-local-index copy between `0l0.ovl` and the PS1 shell bank
does not.

**Conclusion:** same string format and same global IDs; different container
and different shell-bank local ordering. A per-ID (global-slot) swap is safe
for the hints and prompts; a per-local-record swap is safe only inside the
level-1 language banks.

## 4. How each layout fits

### Console layout (PS1 pad)

`ControllerLayout::Console` already maps a mapped controller as the
DualShock type 9: bit 10 `L1` = left shoulder, bit 0 Select = Back, bit 14
Cross = A, bit 13 Circle = B, bit 3 Start = Start, and so on
(`platform_pad.rs`). The five PS1 hint records are therefore **exactly right**
for this layout, provided the port renders the inline glyphs:

- `L1` and `R1` are already ASCII in the PS1 text.
- `\x18` is Cross (A): used for "carry on", "continue", "select an item" and
  "turbo". The PS1 text `Select (\x18)` means "the Select action, on Cross",
  so it must not be drawn as the word "Select" plus a literal byte.
- `\x1a` is Circle (B): used for cancel and beam-down.
- `\x17` is the save/retry glyph. The parallel PC prompts map it from `S`,
  `Esc` and `R`; the PS1's menu tables accept with Cross or Start, so Start is
  the most consistent reading. `\x19` is not attested in the hint or shell
  banks and remains unidentified.

The PS1 text uses these bytes where the PC has key names; they are not ASCII,
and the PS1 renderer must draw them as button graphics (the parallel prompts
identify Cross/Circle). The exact glyph routine was not disassembled. PC text
(and the port's font path) has no such glyphs, so the bytes cannot be copied
through unchanged; the port must substitute button names or its own icons.
The PS1 `L1` toggle also matches the Console layout's PS1 gameplay tables, so
"Press L1 again to return" is literally true there. The system prompts depend
on the \x17 = Start reading; if that reading is wrong, those four need
rechecking even under Console layout.

### PC layout (Xbox pad as retail's WinMM joystick)

`ControllerLayout::Pc` reads an Xbox pad as retail's type-2 joystick:
`XINPUT_BUTTONS` = A, B, X, Y, LB, RB, Back, Start, L3, R3 as WinMM buttons
1..10, with the left stick, triggers and right stick on X/Y/Z/R/U. Retail's
**default** 16-button table (`00534E18`, via `[00534F70]`) binds:

| WinMM button | Xbox | Retail default action |
| --- | --- | --- |
| 1 | A | Fire |
| 2 | B | Grenade |
| 3 | X | Thrust (the turbo, Space on the keyboard) |
| 4 | Y | Beam Down |
| 5 | LB | Weapon Ring |
| 6 | RB | Grenade Ring |
| 7 | Back | CPU Access |
| 8 | Start | Mode (`+34 = -1`, next owned mode) |
| 9 | L3 | Beam Up |
| 10 | R3 | CargoRotR |

There is **no default pad button bound to Buggy or Truck directly**, and no
default pad binding for CPU Select (`Enter`). Mode is a cycle, not a direct
mode choice. The PS1 texts are therefore *not* right for this layout: L1 is Weapon
Ring here, Cross/Circle are A/B (Fire/Grenade), and "Select" has no default
pad button. The hints need PC-pad wording, for example:

| Hint | PC-layout pad wording (proposal) |
| --- | --- |
| `0x43` | "Most factories require scientists to run them - beam them into the factory from your ship to staff it" (keep PC), or "... with Beam Down (Y)" if the PS1 sentence is adopted |
| `0x44` | "Press Mode (Start) to change to big wheeled mode" (Mode cycles the next owned mode; from the buggy with only buggy/truck owned it reaches the truck) |
| `0x45` | "Press Mode (Start) until the buggy returns" — not "again", because Mode advances through every owned mode and need not toggle back |
| `0x46` | "To select an item use up and down and Fire (A)" — **only if** the port gives the message menu a pad confirm; retail's PC joystick defaults have none. Otherwise keep "Enter" or name the port's new binding |
| `0x47` | "Use Thrust (X) to activate turbo booster" |
| `0x48` | "Drop scientists close to a factory with Beam Down (Y) and they will go to work" |

The system prompts are worse for PC layout: retail's pad Start is Mode, A is
Fire, B is Grenade, and there is no pad save path. Applying the PS1 shell
prompts unchanged would produce false instructions. Keep the PC prompts for
PC layout, or author pad prompts only after the port defines pad menu
confirm/save bindings.

Caveat for both: the PC key configuration can rebind the 16 joystick buttons
and all 44 keyboard rows. Static button names can go stale after rebinding,
just as retail's static "mode 2" text does. Naming the action ("Mode",
"Thrust", "Beam Down") instead of the button is the retail-faithful
alternative.

## 5. Recommended smallest implementation

Do the swap as a **display-time text override keyed by H0 global ID**, not by
editing OVL bytes or the hint table:

1. New `crates/inf-game/src/control_hints.rs`: a table keyed by
   `(global_id, language)` holding the PS1 text for IDs 865..870 (and,
   optionally, 37..40), plus a PC-layout variant for the five control IDs.
   The table is tiny: six hints x six languages, plus four prompts if wanted.
2. Add `control_hints` to `crates/inf-game/src/lib.rs`.
3. Apply it at the two H0 consumers that show hints:
   - `crates/inf-game/src/object_runtime/player/commands.rs`, after
     `command_text_word` and before `capture_h0_text` (the message send).
   - `crates/inf-game/src/shown_menu_draw.rs`, in `hint_row`, after
     `hint_texts[hint]` (the List Hints page).
   Both read the same global ID, so one function covers message and list.
4. Gate it on `SdlInput::joystick_live()` and
   `frontend_settings::controller_layout()`. `joystick_live()` already exists
   (`platform_input.rs`) and `session_host.rs` already reads it. Pass the two
   values through `PlayerCommandContext` for the message send and through the
   computer draw context (`ComputerDraw`/`ShownMenuDraw`) for the List Hints
   row, rather than adding a new global.
5. If the four shell prompts are included, apply the same override at their
   frontend string lookups; keep them keyboard-worded under PC layout until
   pad menu bindings exist.
6. Tests: table lookup for all six languages; a `commands.rs` receipt test
   with a live Console-layout pad expecting the PS1 0x44 text; a test with no
   pad and with PC layout expecting the PC text; a `shown_menu_draw` test for
   the hint row.

Files it would touch: the new module, `lib.rs`, `commands.rs`,
`shown_menu_draw.rs`, `object_runtime/player/message_widgets.rs` (if the
environment carries the layout/pad flag), `session_platform`/`session_host.rs`
to publish it, and `frontend_settings.rs` plus the launcher only if the
behavior gets its own setting. No OVL or `PreLoad.dat` change is needed, and
the hint mask/command IDs stay untouched.

### Fidelity questions for the user

1. **When does pad wording apply?** PC retail always shows keyboard wording,
   even with a joystick; the PS1 always shows pad wording. Showing pad text
   when a controller is live is a new behavior, not either retail. Make it a
   setting, tie it to `ControllerLayout::Console`, or tie it to
   `joystick_live()`; the default must be chosen.
2. **PC-layout wording has no retail source.** The PS1 sentences name
   L1/Cross/Circle, which are not the PC-layout bindings. The PC-layout text
   above is authored. Approve it, or keep PC keyboard wording for PC layout
   and use PS1 text only under Console layout.
3. **Button glyphs or words?** The PS1 uses inline `\x17..\x1A` button graphics.
   The port's font has no such glyphs. Use names (`L1`, `Cross`, `Circle`,
   `Start`) or draw icons; if names, choose `Cross/Circle` (PlayStation) or
   `A/B` (Xbox) under Console layout.
4. **Adopt the PS1's non-control rewordings?** `0x43` changes "beam them into
   the factory" to "beam them on to your ship with R1-Circle"; global 871
   changes the save-failure text to a memory card. Only adopt these if the
   port's beam and save behavior matches; they are not control hints.
5. **Localization and provenance.** The PS1 text exists in all six languages,
   but it is retail game text. Decide whether to embed it (the repo commits no
   original game data today), load it from the user's disc, or ship English
   only.
6. **Rebinding.** Retail hints are static. Should pad wording name fixed
   buttons (the PS1 behavior) or the player's current bindings (a deviation)?
7. **PC-layout menu confirm.** Retail's PC joystick defaults have no CPU
   Select. The hint "Select (Cross)" is unactionable on a PC-layout pad until
   the port defines a confirm binding; decide that before wording `0x46`.

## Evidence and limits

- PC: `Infest.exe` (1,445,888 bytes, SHA-256
  `bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956`),
  `PreLoad.dat` (`d41b9eb6aceae2df8c53e802ee27c6e5fcf5fbc9608c3df4758171ab6e8931d3`).
- PC level-1 English bank: `Overlay/1l0.ovl`
  (`eab6d4bdf70cf2851ac8843cfcd0144964fef7dcd9e1ce7ec08aca1ed0f81fdb`);
  common English bank `Overlay/0l0.ovl`
  (`39097d1188a9573e619591b9e2b99be1191e31943f5ff3f501fe63b20e7d2b26`).
- PS1: SLES-01870, track 1 (`MODE2/2352`); `GAME.EXE` (1,187,840 bytes,
  SHA-256 `96fde8e66a6b70e47315b9aea0e8be362fa299dac4837c5efe7091a9d61b0dc2`),
  `MAIN.OVL` (12,919,919 bytes,
  `310b1ad50fcb597bc87f25366ab4c4858684f38ba2e05bd12fd2868637a569d6`),
  `PRELOAD.DAT`
  (`19c523c288b6049e4318297ea8e54b7c203e9f2baba62301e0fb7dfd188b8ffc`),
  `MAIN.IDX`
  (`9fb22eb6977e8f8c147a96fdc1c33fbfab4e83c073da37ea91f94b852b3528b3`).
- The PC hint path addresses come from `docs/re/PLAYER_MESSAGES.md`,
  `docs/re/PLAYER_COMMAND_RUNTIME.md` and the current port owners. The PS1
  container and bank offsets are direct reads of the disc; the PS1
  command-table comparison is a byte comparison at `0x10082C`.
- `\x18` = Cross and `\x1a` = Circle are established by the parallel PC
  prompts (Enter/Space -> Cross, Esc -> Circle) and by `Select (\x18)`.
  `\x17` = Start is inferred, not proven: it replaces `S`, `Esc` and `R`, and
  the PS1's own menu tables accept with Cross or Start. `\x19` is unattested.
  The exact PS1 glyph rendering was not disassembled; only the text bytes and
  their PC counterparts were compared.
- The PS1 shell bank's local order differs from the PC's, so the shell-prompt
  mapping was established by global ID, not by record position. The Italian
  284 anomaly is noted but not explained.
