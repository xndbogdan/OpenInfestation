# The Remastered pad

A third controller layout for modern game pads: the same physical bindings on
any SDL game controller (Xbox, PlayStation, 8BitDo), positional by SDL's
button names. This document records the retail actions each agreed binding
means, the evidence for them, the recommended implementation in this port, the
hint-wording plan, and what cannot be done as agreed. The hint wording of
section 9 is implemented; the layout, its device and its tables are not yet
(the branch that adds them follows this document).

The agreed scheme:

- **Play:** left stick steer and pitch (and the cursor in the CPU panel); LT
  analog thrust; RT fire; A beam down; B beam up; X throw grenade; Y mode
  (next owned mode); LB/RB previous/next gun; d-pad left weapon ring, right
  grenade ring, up detonator, down lights; L3 CPU access; R3 map; View first
  person; Start pause; Y held + d-pad left/right cargo rotate. The right stick
  is reserved and unbound.
- **Menus** (frontend, briefing, results, name entry, in-world pause, map, key
  configuration): A confirm, B back, left stick and d-pad move, LB/RB page
  where a screen pages, Start closes the in-world menu, View opens the map
  where the screen has one.

Every conclusion below cites the executable's addresses and the authored table
rows. The tables were dumped from the retail `Infest.exe` with a throwaway
`#[test]` over `parse_*_input_tables` and `parse_screen_input_tables_direct`;
the test was deleted before this commit.

## 1. The port's input path, and what a layout may change

```
SDL pad ──SdlPads::poll──▶ InputNotification::Pad{native_type, axes, sample}
         ──SdlInput::update──▶ InputRuntime device
         ──screen sets──▶ digital / analog bindings ──▶ retail callbacks
```

`SdlPads` (`platform_pad.rs`) latches the native type and axis count from the
first pad it opens, and every reading carries them. `SdlInput`
(`platform_input.rs`) creates one `NativePad` from the first `Pad`
notification and calibrates its axes with
[`pad_axis_transform`](../../crates/inf-game/src/platform_input.rs) (`00439AD0`,
`00433764`, `00438850`). A screen's `FrontendBindings` then installs its
`ScreenInputTables` **per native type**: slot `n` is the table set for a
device of native type `n` (`00439B30`). There are twelve slots
(`INPUT_DEVICE_SLOTS`), and the retail file fills 0 (keyboard), 2 (WinMM
joystick), 6/7 (digital PlayStation pad), 8/9 (analog PlayStation pad); 1
(mouse), 3–5, 10 and 11 (button device) have no authored table.

Two consequences matter here:

1. **A layout is its native type plus the tables installed for that slot.**
   Two layouts that pick different native types cannot affect each other's
   bindings, so PC (`2`) and Console (`9`) stay byte-identical if Remastered
   owns another slot.
2. **The native type is in the recorded stream.** `SessionRecorder` writes
   `p <type> <axes> ...` (`session_log.rs`), and a replay rebuilds the same
   device from it. A replay therefore does not depend on the live
   `--controller-layout`: a type-2 recording always plays slot 2, a type-9
   recording slot 9. A Remastered recording must have its own type for the
   same reason.

`ControllerLayout` and `SdlPads` currently create only types 2 and 9.
`axis_off_centre` already treats every non-2 type as byte axes centred on
`0x80`, and `pad_axis_transform` already biases every non-2 type by `-0x80`,
so the byte-axis PlayStation domain is already general in the port.

## 2. The Remastered device

### 2.1 Recommended native type: 8

Type 8 is retail's analog PlayStation pad that does not answer the
extended-mode query (`8012D068`, read by `800F4310`); types 8 and 9 have the
same codes and axes. The port never creates a type 8 today (`Console` uses
9), so **slot 8 is free**, and every retail mechanism the Remastered pad needs
is already type-8 behaviour:

| Mechanism | Type 8 evidence |
| --- | --- |
| Byte axes 0..255, centre `0x80` | `00533764` biases 8 and 9 by `-0x80`; `axis_off_centre` already says so |
| Buttons 1..16, active-high bits | `800F43D4`/`800F4310`: codes 1..16 are Select, L3, R3, Start, Up, Right, Down, Left, L2, R2, L1, R1, Triangle, Circle, Cross, Square |
| Failed read is the status input | `005222B0` binds code `0x1F` for types 1 and 6..9; `SdlInput` stores exactly `0x8000_0000` (bit 31 = code 31 = `0x1F`) |
| Counts as a live pad | `00439A20` counts types 1, 2, 8 and 9 |
| `joystick_live` after an unplug | `SdlInput::joystick_live` must test `matches!(native_type, 8 | 9) && absent`; today it tests only 9 |

The alternative is a port-owned type 10. It costs one more branch in
`joystick_live` and `released_sample`, and it makes the type unambiguous if a
future PS1-pad feature wants type 8. It gives up retail's bias, status and
count for free. Section 11 asks the user to choose; the rest of this document
assumes type 8, and every screen-side statement is the same for either.

### 2.2 The sample is the Console sample plus a thrust axis

The Remastered sample is **byte-for-byte the Console sample's button bits**
(`CONSOLE_BUTTONS`): SDL's positional face buttons stand for the PlayStation's
— `Button::A` (bottom) = Cross = code 15, `B` (right) = Circle = 14, `X`
(left) = Square = 16, `Y` (top) = Triangle = 13, `LeftShoulder` = L1 = 11,
`RightShoulder` = R1 = 12, `Back` = Select = 1, `Start` = 4, `LeftStick` = 2,
`RightStick` = 3, the d-pad = 5/6/7/8, and a trigger past `0x4000` = L2/R2 =
9/10. The d-pad is a real digital input (retail's type-2 joystick never reads
its point-of-view hat; a mapped controller's d-pad here is four buttons).

The only changes are the axis count and the last two axes:

| Axis | Content |
| --- | --- |
| 0 | left stick X, `position(v) >> 8` (0..255) |
| 1 | left stick Y, `position(v) >> 8` |
| 2 | right stick X, `position(v) >> 8` (reserved, unbound) |
| 3 | right stick Y, `position(v) >> 8` (reserved, unbound) |
| 4 | thrust: right trigger forward, left trigger reverse, `0x80 + (RT − LT) × 0x7F / 0xFFFF` |
| 5 | `0x80` (unused) |

`position(v) = (i32::from(v) + 0x8000) as u32`, and RT and LT are its value
for the two triggers. SDL reports a trigger as `-32768 .. 32767` (released ..
full), so a raw `position` byte would rest at `0x00`, not the `0x80` the retail
bias expects; summing the two triggers around `0x80` puts rest at `0x80`, full
right at `0xFF` and full left at `0x01`. Section 3.1's re-axised play record
multiplies axis 4 by `+1024`, so a raised axis is forward power: the right
trigger drives forward and the left reverses. The right stick keeps axes 2/3
so a later camera feature can bind them without moving the thrust.

Reported axis count: **6**. Retail's type 8 reports 4 (`8012D068`), but the
port's device is its own; the count is latched from the first reading and only
drives the calibration loop.

An unmapped raw joystick stays the PC layout (`ControllerLayout::Pc`, type 2),
as it does today: its button positions are unknown, so a positional layout
cannot be claimed. 8BitDo pads therefore need a mode SDL maps as a game
controller (XInput/Switch/PS), which is their normal mode.

## 3. The play actions

Legend: **KB** is the gameplay sub-object's slot 0 (the live keyboard records
that the key configuration edits, `00534570`); **T2** is slot 2 (`00534CC0`,
WinMM buttons 1..16); **T9** is slots 8/9 (`00522FB8`, the PS1 pad). Numbers
are native button codes (1-based; code `n` is bit `n`). The descriptor is the
authored record whose identity equals its address; its flags and callback are
part of the record.

### 3.1 The analog play table

The sub-object's analog table `00523238` (slots 8/9) binds the player's three
analog consumers (`0054ED08`, `0054ECE8`, `0054ECF8` = player `+64`, `+60`,
`+5C`). The same three consumers are `005231D8` for the WinMM joystick
(slot 2), at a quarter of the gain.

| Player | Descriptor | Axis (T9) | Q31 | Multiplier | Meaning |
| --- | --- | --- | --- | --- | --- |
| `+64` | `0054ED08` | 3 (right stick Y) | `0x40000000` | `-1024` | analog throttle (`00418F60`) |
| `+60` | `0054ECE8` | 1 (left stick Y) | `0x40000000` | `-1024` | pitch / lift |
| `+5C` | `0054ECF8` | 0 (left stick X) | `0x40000000` | `1024` | turn |

`0046A260` publishes X/Y to `+8/+C` (clamped to a unit) and Z to `+10`
unclamped; `00418F60` reads `+10` as the forward throttle and `+C` as lift,
dead-zoned at `0x1C20` and `0x2A30`. So **analog thrust is the `0054ED08`
consumer on the thrust axis**: the Remastered play table clones `00523238`
and changes that one record to axis 4 with multiplier `+1024` (positive: the
right trigger raises the axis above `0x80` while the PS1 stick's Y falls
upward, and the left trigger lowers it, so the same record reads the left
trigger as reverse). The axis and the sign are the only edits; the Q31
half-scale is retail's.

With the triggers at rest (`0x80`) the stage-1 bias gives 0, so the record
produces 0. At full right trigger and Sensitivity 10 it produces about
`89088` (full left gives the same magnitude reversed; the PS1's full right
stick gives about `90112` at the same sensitivity), which `analog_throttle`
clamps to full power. Sensitivity scales it, as it scales every pad axis in
retail.

Digital thrust (`+78`, the turbo/jump request, `0046A260` step 2's
`+3A`), is a separate consumer (`0053A378`). See section 8.3.

### 3.2 The digital play rows

| Remastered role | Retail action | Callback | Descriptor | KB | T2 | T9 | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Fire | `+80` | — | `0053A388` f6 | Z `2C` | 1 | 16 Square, 2 L3 | RT (code 10) |
| Grenade / detonate | `+84` | `004694D0` | `0053A390` f6 | X `2D` | 2 | 14 Circle, 3 R3 | X (16); double tap sets `+39` |
| Beam down (drop) | `+3C` | `00469670` | `0053A3D8` f0 | D `20` | 4 | 13 Triangle | A (15) |
| Beam up (pick up) | `+3B` | `00469660` | `0053A3D0` f0 | C `2E` | 9 | 13+12 | B (14) |
| Mode: next owned | `+34 = -1` | `00469580` | `0053A398` f0 | Caps Lock `3A` | 8 | 11 L1 | Y (13) |
| Primary ring (held) | `+40` | `00469760` | `0053A428` f0 | R Ctrl `9D` | 5,15 | 9 L2 | d-pad left (8) |
| Secondary ring (held) | `+41` | `00469770` | `0053A430` f0 | Kp0 `52` | 6,16 | 10 R2 | d-pad right (6) |
| Primary step +1 | `+28 = 1` | `004696C0` | `0053A3F0` f0 | Home `C7` | — | — | RB (12) |
| Primary step −1 | `+28 = -1` | `004696E0` | `0053A3F8` f0 | End `CF` | — | — | LB (11) |
| Detonator (row 6) | `+28 = 8` | `004698C0` | `0053A4A0` f0 | F7 `41` | — | — | d-pad up (5) |
| Lights / infra-red | `+3D = 1` | `00469740` | `0053A418` f0 | L `26` | 13 | 16+12 | d-pad down (7) |
| CPU access | computer toggle | `00469450` | `0053A438` f0 | Tab `0F` | 7 | 1 Select | L3 (2) |
| Cargo rotate + | `+30 = 1` | `00469680` | `0053A3E0` f0 | A `1E` | — | 8+12 | Y+Left |
| Cargo rotate − | `+30 = -1` | `004696A0` | `0053A3E8` f0 | — | 10,14 | 6+12 | Y+Right |
| Message select | `+50` bit 1 | `004697F0` | `0053A468` f0 | Return `1C` | — | 15,2,3 | A (15), and RT through `0046A27D` |
| Digital thrust | `+78` | — | `0053A378` f6 | Space `39` | 3 | 15 Cross | LT past half (question) |
| Map (phase 7) | screen `+0x40` | `004436D0` | `00534230` f2 | M `32` | 11 | 1+13 | R3 (3), screen table |
| First person | screen `+0x44` | `00443650` | `00534260` f2 | F12 `58` | — | 11+12 | View (1), screen table |
| Pause / menu key | screen `+0x64` | `00443700` | `00534238` f2 | P `19`, Esc `01` | — | 4 Start | Start (4), screen table |
| Quit to frontend | screen `+0x54` | `00443470` | `00534210` f2 | Shift+Esc | — | 4+1 | not bound; pause menu has Quit |

`+28 = 8` selects primary row 6. `0046A260`'s request index is `request - 2`
(the vehicle update's primary request store), so row 0 is Cannon, row 1
Plasma, row 2 Lasers, row 3 Flamethrower, row 4 Rocket, row 5 Homing missile,
**row 6 Detonator** (`00530158`, `005305B0`). F7 stores 8, so F7 is the
detonator; the Detonator row is always owned (`005305B0` ID 0). The d-pad-up
binding is the same request, not a new action.

`00469740` toggles infra-red vision when that upgrade is owned, else the
headlights (`00420080`); "lights" is the common case.

`00469760`/`00469770` are **modifiers**, not selectors. The vehicle reads
`+40`/`+41` as held levels and only turns the ring when `turn_14 != 0`
(`004166CD..0041685A`); `turn_14` is the *digital* Left−Right pair
(`0053A368`/`0053A370`), not the analog stick. The PS1's gesture is
"hold L2/R2 and tap a direction". Section 8.2 explains why this matters.

### 3.3 What the PS1 table already binds, and what it does not

The type-9 play table `00522FB8` (26 rows) is the closest authored analogue.
It binds: Square = fire, L3 = fire, Circle = grenade, R3 = grenade, Cross =
digital thrust, L1 = mode, Triangle = beam down and the Escape (unit
broadcast) action, Triangle+R1 = beam up/pick up, L2/R2 = the two ring keys,
Select = CPU, Left/Right+R1 = cargo rotate, Square+R1 = lights, and the
arrows/Cross as message keys. It has **no** primary steps, **no** detonator,
**no** first person, **no** map, **no** pause and **no** quit.

The WinMM table `00534CC0` binds ten reachable buttons: 1 fire, 2 grenade, 3
thrust, 4 beam down, 5/6 ring keys, 7 CPU, 8 mode, 9 beam up, 10 rotate −.
Buttons 11..16 add map (11), free look (12), lights (13), rotate − (14) and
the ring keys again (15/16) — but an XInput pad only reports ten buttons, so
**map, lights and free look are unreachable on a real Xbox pad in the PC
layout**, and the board's "first person" is absent entirely.

**Keyboard-only actions** (no row in T2 or T9, or only beyond ten WinMM
buttons): detonator (F7), primary steps (Home/End), secondary steps
(PgUp/PgDn), HUD (U), player list (Backspace), network team/scores (T/Y),
terrain detail (Alt+F3), next world (Alt+N), network restart (F10), direct
modes 1..5 (mode 6 has no default key at all), and free look (V) beyond WinMM
button 12. Map (M), lights (L) and first person (F12) exist on pads only
through the unreachable WinMM buttons or the PS1 chords.

## 4. The menu actions

The menu screens each have their own callbacks and their own type-9 rows. The
Remastered table for a screen is the retail type-9 row set **re-coded** to the
Remastered buttons, plus rows for actions retail's pad lacked. Descriptors are
cloned from the screen's own authored rows, so their callbacks and flags stay
retail's. Slots 6/7 (the digital PlayStation pad) repeat slots 8/9's rows in
every menu constructor, and the play sub's slots 6/7 repeat its 8/9 table, so
the two PlayStation pairs need no separate treatment; the Remastered table is
built once per screen and installed in slot 8 only.

Legend: **A 15, B 14, X 16, Y 13, LB 11, RB 12, LT 9, RT 10, Select 1, Start
4, L3 2, R3 3, Up 5, Right 6, Down 7, Left 8.**

### 4.1 Frontend (004838A0, tables 005280B0 / 00528218 / 00528370 / 00528410)

| Role | Action | Callback | Descriptor | KB | T2 | T9 |
| --- | --- | --- | --- | --- | --- | --- |
| Confirm | `00445200` | `00445200` | `0053BFC0` f2 | Space, Return | all 16 | 15 Cross, 4 Start |
| Back | `004451E0` | `004451E0` | `0053BFE8` f2 | Esc, G | — | 13 Triangle |
| Previous | `00445220` | `00445220` | `0053BFC8` f2 | Up | — | 5 Up |
| Next | `00445240` | `00445240` | `0053BFD0` f2 | Down | — | 7 Down |
| Adjust previous | `00445260` | `00445260` | `0053BFD8` f2 | Left | — | 8 Left |
| Adjust next | `00445280` | `00445280` | `0053BFE0` f2 | Right | — | 6 Right |

Remastered: A = confirm, B = back, Start = confirm (retail's Start row; the
"Start or A" prompt), d-pad and stick = previous/next/adjust. LB/RB = an
added duplicate of Previous/Next (section 8.4). The type-9 analog table
`005333A0` (axes 0/1, Q31 `0x40000000`, mult `64`/`-64`) is cloned for slot 8,
so the left stick moves the selection.

### 4.2 Briefing (0040A420, tables 00520028 / 005200E0 / 00520238 / 005202A0)

| Role | Action | Callback | Descriptor | KB | T2 | T9 |
| --- | --- | --- | --- | --- | --- | --- |
| Back | `0040D660` | `0040D660` | `0052EFF0` f2 | Esc, G | — | 4 Start |
| Confirm | `0040D670` | `0040D670` | `0052EFF8` f2 | Return | all 16 | 15 Cross |
| Previous | `0040D680` | `0040D680` | `0052F000` f2 | Up | — | 5 Up |
| Next | `0040D690` | `0040D690` | `0052F008` f2 | Down | — | 7 Down |

Remastered: A = confirm, B = back, d-pad up/down, LB/RB = the same, stick
through `005333A0`. Retail's Start-as-back row is kept as a second back.

### 4.3 Results (0042AB00, tables 005212A8 / 00521410 / 00521568 / 00521648)

| Role | Action | Callback | Descriptor | KB | T2 | T9 |
| --- | --- | --- | --- | --- | --- | --- |
| Back | `0042AA30` (bit `0x200`) | `0042AA30` | `005313A0` f2 | Esc | — | 13 Triangle |
| Continue | `0042AA60` | `0042AA60` | `00531398` f2 | Space, G | all 16 | 15 Cross, 4 Start |
| Save | `0042AAA0` | `0042AAA0` | `005313A8` f2 | S | — | 14 Circle |
| Confirm (held) | `00445200` | `00445200` | `005313E8` f2 | Return | — | 15 Cross |
| Previous / Next | `00445220/40` | same | `005313B0/B8` | Up/Down | — | 5/7 |
| Adjust prev/next | `00445260/80` | same | `005313C0/C8` | Left/Right | — | 8/6 |

Remastered: A = continue + held confirm (two rows, as the PS1 binds Cross to
both), B = back, X = save, Start = continue, d-pad + stick, LB/RB = previous/
next. The results screen has **no analog cells at all**
(`PHASE8_ANALOG_TABLE_POINTERS` is zero everywhere), so the stick is a port
addition there.

### 4.4 Name entry / text entry (00459060, tables 00525C58 / 00525D48 / 00525E88 / 00525F40)

| Role | Action | Callback | Descriptor | KB | T2 | T9 |
| --- | --- | --- | --- | --- | --- | --- |
| Back | `004451E0` | `004451E0` | `00537C00` f2 | Esc | — | 13 Triangle |
| Confirm (Tab) | `0045AE90` | `0045AE90` | `00537C30` f2 | Tab | — | 4 Start |
| Confirm | `00445200` | `00445200` | `00537C08` f2 | Return | all 16 | 15 Cross |
| Previous / Next | `00445220/40` | same | `00537C10/18` | Up/Down | — | 5/7 |
| Adjust prev/next | `00445260/80` | same | `00537C20/28` | Left/Right | — | 8/6 |
| Retry | `0045AEA0` | `0045AEA0` | `00537C38` f2 | R | — | 14 Circle |

Remastered: A = confirm, B = back, X = retry, Start = confirm (retail's Start
row), d-pad + stick, LB/RB = previous/next. The pad still cannot type a name;
that is a pre-existing limit of the keyboard-only entry (the Console layout
has it too).

### 4.5 In-world pause (00443190, tables 00522B48 / 00534418 / 00522C28 / 00522CE0)

| Role | Action | Callback | Descriptor | KB | T2 | T9 |
| --- | --- | --- | --- | --- | --- | --- |
| Quit to frontend | `00443470` (`+54`) | `00443470` | `00534210` f2 | Shift+Esc | — | 4+1 |
| Close menu | `00443700` (`+64`) | `00443700` | `00534238` f2 | P, Shift+Esc | — | 4 Start (forb 1) |
| Back | `00443590` | `00443590` | `00534278` f2 | Esc, Tab | — | 13 Triangle |
| Previous | `004434A0` | `004434A0` | `00534240` f2 | Up | all 16 | 5 Up |
| Next | `004434D0` | `004434D0` | `00534248` f2 | Down | all 16 | 7 Down |
| Adjust prev | `00443500` | `00443500` | `00534250` f2 | Left | all 16 | 8 Left |
| Adjust next | `00443530` | `00443530` | `00534258` f2 | Right | all 16 | 6 Right |
| Confirm | `00443560` | `00443560` | `00534270` f2 | Return | all 16 | 15 Cross |

Remastered: A = confirm, B = back, Start = close (`00443700`), d-pad + stick,
LB/RB = previous/next. The type-9 constructor snapshots the shared analog
cells; the parser cannot, so `parse_phase5_input_tables` has no analog rows.
`FrontendAssets` would install `005333A0` in slot 8 for Remastered (a port
addition; section 8.8).

### 4.6 Map (00451020, tables 00525120 / 00525238 / 00525390 / 00525498)

| Role | Action | Callback | Descriptor | KB | T2 | T9 |
| --- | --- | --- | --- | --- | --- | --- |
| Close | `004520A0` | `004520A0` | `00537430` f2 | Esc, Space, Tab, Return, M | all 16 | many |
| Up / Down | `004520C0/F0` | same | `00537438/40` | Up/Down | — | 5/7 |
| Left / Right | `00452120/50` | same | `00537448/50` | Left/Right | — | 8/6 |
| Zoom out | `00452180` | `00452180` | `00537458` f2 | Insert | — | 11 L1 |
| Zoom in | `004521B0` | `004521B0` | `00537460` f2 | Delete | — | 12 R1 |

Remastered: A/B/Start/Select/R3 = close (so R3 and View both toggle the map),
d-pad + stick, LB/RB = zoom. The map's analog cells are runtime snapshots too,
so its slot-8 table is added from `005333A0` for Remastered.

### 4.7 Key configuration (00444BD0, tables 00524B90 / 00524C70 only)

| Role | Action | Callback | Descriptor | KB | T2 | T9 |
| --- | --- | --- | --- | --- | --- | --- |
| Back | `004451E0` | `004451E0` | `00535620` f2 | Esc, G | — | no type-9 table |
| Confirm | `00445200` | `00445200` | `00535628` f2 | Return | all 16 | — |
| Previous / Next | `00445220/40` | same | `00535630/38` | Up/Down | — | — |
| Adjust prev/next | `00445260/80` | same | `00535640/48` | Left/Right | — | — |

Remastered: A = confirm, B = back, d-pad, LB/RB duplicates. This is a fresh
table: retail has no pad rows here, so there is nothing to re-code.
**Warning:** A on a binding row starts a capture (`004450F0`) that only the
keyboard can finish (`KeyConfigScreen::tick` reads adapter slot 0 alone).
With no keyboard attached that is a trap; section 8.6.

### 4.8 Startup / language (00432500)

| Role | Action | Callback | Descriptor | KB | T2 | T9 |
| --- | --- | --- | --- | --- | --- | --- |
| Accept | `004327B0` | `004327B0` | `005328B0/A8` f0 | Return, Esc | all 16 | 4, 15, 13 |
| Left | `004327C0` | `004327C0` | `005328B8` f0 | Left | — | 8 |
| Right | `004327E0` | `004327E0` | `005328C0` f0 | Right | — | 6 |

Remastered: A = accept, Start = accept (retail's Start row), d-pad
left/right, LB/RB = left/right, stick through `005333A0` (the retail startup
screen has no analog rows).

### 4.9 Title (00404620)

Every non-system row calls skip `00404870` (descriptor `0052E638` f2); slot 0
binds Esc, Space and Return, slots 6..9 bind 14 codes each
(`0051F378`/`0051F4A8`). Codes 14 (B/Circle) and 15 (A/Cross) are among those
14, so the Remastered table can keep the retail rows unchanged: A or B skips.
Codes 2/3 (L3/R3) and 10 (R2) are absent; keeping that is retail.

## 5. How A, B and Y get different meanings per screen

Because the tables are **per native type and per screen**, no input remap is
needed:

- Slot 8 of the frontend table maps code 15 to `00445200` (confirm); slot 8 of
  the gameplay sub table maps code 15 to `00469670` (beam down) and
  `004697F0` (message select). Same physical A, different rows.
- Slot 8 of the menu tables maps code 14 to the screen's back callback; the
  play table maps code 14 to `00469660` (beam up).
- Code 13 is "mode" in play and unbound in menus.

This is exactly how retail already gives Cross one meaning per screen. It is
also why "A = confirm in menus, beam down in play" needs no context in
`SdlInput`: the screen chooses its own table set when it installs its
bindings.

## 6. Implementation options

### (a) A distinct native type with synthesized per-screen tables — recommended

`ControllerLayout::Remastered` → native type 8 (or 10), six axes, the Console
button bits. `FrontendAssets::from_executable` adds slot 8 to every screen's
`ScreenInputTables` from a new `remastered_pad` module, so every screen that
already installs retail tables installs the Remastered one for a type-8
device. Nothing in the input runtime knows the layout.

- **Keyboard-only actions** get real pad rows with the retail descriptor and
  callback the keyboard row uses (detonator `0053A4A0`, primary steps
  `0053A3F0/F8`, map `00534230`, first person `00534260`, pause
  `00534238`). No new action semantics are invented, only new bindings.
- **A in menus vs play** is the per-screen table (section 5).
- **Y-held chords** use retail's own mechanism: the single rows forbid code 13
  (`Y`) and the chord rows require `[13, 8]`/`[13, 6]`. Retail already encodes
  R1 chords this way: `00523094` requires `[0D,0C]` (Triangle+R1), `005230A8`
  requires `[0D]` with `[0C]` forbidden (Triangle alone), and `005230BC`
  requires `[08,0C]` (Left+R1).
- **Simulation and replays:** nothing changes. The Pad notification carries
  type 8 and the raw PS1-coded sample; the screen state selects the tables,
  exactly as it selects retail tables today. A replay reproduces it without
  the live layout.

Cost: one new module, one injection point in `frontend_assets.rs`, small
changes in `platform_pad.rs`/`platform_input.rs`, the launcher row, and the
hint plumbing. No simulation or replay code changes.

### (b) Native type 9 plus a context-dependent button remap in SdlInput

`SdlInput` would translate the same physical button to a different PS1 code
per screen (A→Cross in menus, A→Triangle in play), and extra tables would
still be needed for the keyboard-only actions.

- It cannot add a row, so map/first person/detonator/primary steps still need
  synthesized rows or a second mechanism.
- `SdlInput` would need the screen phase and the shown-message state each
  frame; the mapping becomes a hidden input mode that the recorded stream does
  not name. A replay would have to re-derive the same screen state, and a
  recording played from a different point or with a modified screen would
  diverge.
- It would share type 9 with `Console`, so a recording's native type would no
  longer identify the tables, and playing a Console recording under
  Remastered (or vice versa) would silently reuse the wrong rows.

Rejected: more coupling, weaker replay identity, and it does not remove the
need for synthesized rows.

### (c) Anything better?

The best refinement of (a) is to **re-code the retail type-9 rows rather than
author whole tables**: clone the screen's own slot-9 records (real
descriptors, callbacks, flags) and replace only their `required`/`forbidden`
lists, then append the rows retail's pad lacked. That keeps provenance
attached (every menu row is a retail row with new codes) and keeps the module
small. Section 7 uses this shape.

## 7. Recommended implementation

### 7.1 Files

**New `crates/inf-game/src/remastered_pad.rs`** — port-authored, no retail
bytes:

- `pub const NATIVE_TYPE: u8 = 8;`, `pub const AXES: u8 = 6;`
- code constants (A 15, B 14, …), the table roles, and the descriptor
  identities it appends.
- `pub fn install(tables: &mut ScreenInputTables, screen: Screen,
  source: FrontendExecutable<'_>) -> Result<(), InputTableError>`:
  - clones `tables.digital[9]` when the screen has one, re-codes its rows by
    callback per `screen`, and writes the result to `digital[8]`;
  - builds `digital[8]` from explicit rows for the screens retail's pad never
    handled (gameplay screen + sub, key configuration);
  - writes `analog[8]`: `parse_analog_input_table(source, 0x0053_33A0)` for
    menus, or the re-axised `00523238` for the play sub.
  - Synthetic row addresses use a reserved `ByteSpan`/`virtual_address` range
    outside the image; descriptors are parsed from the executable by their
    real addresses through `parse_digital_action_descriptor`, so only the
    row bytes are port-authored. `FrontendBindings`' provenance checks accept
    both (they test self-consistency, as the existing unit fixtures do).

**`frontend_assets.rs`** — after each `parse_*_input_tables`, call
`remastered_pad::install(&mut tables, Screen::…, mapped)`. The bindings are
`let mut` where needed. This is the single injection point: every screen
already installs `assets.<screen>_input_tables()`.

**`platform_pad.rs`**:
- `ControllerLayout::Remastered` (`#[serde(rename = "remastered")]`,
  `ValueEnum` gives `--controller-layout remastered`).
- `native_type()` → 8; `axes()` → 6 for type 8.
- `Open::sample` gets the type-8 arm (section 2.2); `CONSOLE_BUTTONS` and
  `TRIGGER_PRESSED` are reused.
- `released_sample` centre: `matches!(native_type, 8 | 9)`.
- A `family()` on `SdlPads`/`OpenPad` (section 9.2).

**`platform_input.rs`**:
- `joystick_live`: `matches!(pad.native_type, 8 | 9) && pad.absent` (retail's
  `005222B0` status types), with a doc note.
- Docs: `PadSample` and `InputNotification` mention six axes for the
  Remastered type.

**`launcher/choices.rs`** — add
`(ControllerLayout::Remastered, "Remastered (modern pad)")` to
`CONTROLLER_LAYOUTS`; both launchers index the array by position and already
use `.len()`.

**`control_hints.rs`** and its call sites — section 9.

**Docs:** this file; a link from
[`INPUT_RUNTIME.md`](INPUT_RUNTIME.md#the-playstation-release), and the
per-function judgments in `docs/progress/status.csv` if any retail function's
status changes (none should: no retail function is reimplemented).

### 7.2 Screen transforms

| Screen | Recodes (retail callback → Remastered codes) | Added rows |
| --- | --- | --- |
| Frontend | confirm 15; Start 4; back 14; prev/next/adjust 5/7/8/6 | LB 11 → prev, RB 12 → next |
| Briefing | confirm 15; back 14 and 4 (retail Start kept); prev/next 5/7 | LB/RB → prev/next |
| Results | continue 15 and 4; back 14; save 16; prev/next/adjust 5/7/8/6 | confirm 15 (`00445200`); LB/RB |
| Text entry | confirm 15 and 4; back 14; retry 16; prev/next/adjust 5/7/8/6 | LB/RB |
| Pause | confirm 15; back 14; close 4 (forb 1); prev/next/adjust 5/7/8/6 | LB/RB |
| Map | close 15, 14, 4, 1, 3; up/down/left/right 5/7/8/6; zoom 11/12 | — (R3 and Select close) |
| Startup | accept 15 and 4 (retail Start kept); left/right 8/6 | LB/RB; stick |
| Title | unchanged (14 rows already include 14 and 15) | — |
| Key config | fresh: confirm 15, back 14, prev/next/adjust 5/7/8/6 | LB/RB |
| Gameplay screen | fresh: pause 4 (forb 1), first person 1, map 3 | — (no quit chord) |
| Gameplay sub | fresh: section 3.2 | — |

The play sub's rows, with forbidden lists:

```
code 15      → 0053A3D8 (beam down); also 15 → 0053A468 (message select)
code 14      → 0053A3D0 (beam up)
code 16      → 0053A390 (grenade)
code 10      → 0053A388 (fire)
code 9       → 0053A378 (digital thrust; question 8.3)
code 11      → 0053A3F8 (primary −1)
code 12      → 0053A3F0 (primary +1)
code 5       → 0053A4A0 (+28 = 8, detonator)
code 7       → 0053A418 (+3D, lights)
code 8       → 0053A428 (+40, primary ring), forb [13]
code 6       → 0053A430 (+41, secondary ring), forb [13]
code 2       → 0053A438 (computer)
code 13      → 0053A398 (mode), forb [8, 6]
code [13, 8] → 0053A3E0 (+30 = 1, rotate +)
code [13, 6] → 0053A3E8 (+30 = -1, rotate −)
```

### 7.3 Tests

- `remastered_pad_tests.rs`:
  - `every_screen_installs_a_type_eight_digital_table_and_keeps_slot_nine`:
    for all eleven screen sets, `digital[8].is_some()`, `analog[8]` per role,
    and `digital[9] == parse_*_input_tables(…).digital[9]` (the retail table
    is untouched).
  - `play_roles_bind_the_agreed_buttons`: each code row above resolves to the
    named callback/descriptor.
  - `menu_confirm_and_back_are_a_and_b_on_every_menu`.
  - `cargo_rotate_needs_y_and_a_direction`: the single direction rows forbid
    13 and the chord rows require `[13, 8]`/`[13, 6]`.
  - `the_left_trigger_is_the_analog_thrust_axis`: the play analog's first
    record is `0054ED08`, axis 4, Q31 `0x40000000`, multiplier `+1024`.
  - `the_map_screen_binds_zoom_to_the_shoulders`.
- `platform_pad_tests.rs`: `the_remastered_pad_reports_ps1_codes_and_trigger_axes`
  (A→15, LT→9 and axis 4 `0x80`/`0xFF`), and a matrix asserting the PC and
  Console samples are unchanged.
- `platform_input_tests.rs`:
  `a_type_eight_pad_is_live_until_its_read_fails` (the status input),
  `a_remastered_recording_replays_the_same_pad_edges` (record a Pad stream,
  replay into a fresh runtime, compare consumer edges).
- `frontend_assets` integration: `the_assets_keep_the_retail_type_nine_tables`
  and `a_type_eight_device_finds_its_tables_on_every_screen` (install with a
  synthetic type-8 device in the frontend, briefing, results, text entry,
  pause, map, key config, gameplay screen and sub).
- `control_hints.rs`: `remastered_hints_name_each_family` (all nine IDs, six
  languages, Xbox/PlayStation/Nintendo/generic names) and the existing font
  glyph sweep extended to the Remastered strings.
- `frontend_settings_tests.rs`: `remastered_persists_and_loads`.
- `session_log`/`frontend_app`: `a_recorded_pad_family_replays` (section 9.3).

## 8. The gaps the evidence exposes

These are the points where the agreed scheme and retail's own data model do
not meet. Each needs a user decision; none is silently approximated.

### 8.1 B cannot back out of the CPU panel

Retail has no panel-back input. The computer is a shown menu; `00469450` (Tab)
opens it and closes it, `0046A27D` makes Fire select, and the panel's cursor is
the message-key codes 0–4. The menu key `00443700` (P, and Esc without Shift)
reaches the screen's `+0x64`, `00443070`: with a shown message it calls
`004133C0`, which hides the whole panel; with no message it requests phase 5.
There is no code that means "pop one level" (`004133E0` is the phase-5 screen's
Escape, not a message code).

B is already Beam Up (`00469660`). Binding code 14 to `00443700` as well would
make **every B press in play open the in-world menu** when no panel is shown.
Binding it only while a message is shown is not expressible in a static
table: the required/forbidden lists hold device codes only, not screen state.

Options:

1. **L3 toggles the panel closed** (retail Tab: "a shown message hides it").
   No dedicated back is needed; keep B = beam up only. Recommended.
2. Move Beam Up to another button and give B the `00443700` row (then B opens
   the pause menu when no panel is shown — still wrong).
3. A port-level Remastered gesture: while a message is shown, translate B to
   `00443700`. Deterministic from the recorded stream plus the shown byte,
   but it is new input behavior, not a retail table.

### 8.2 D-pad rings cannot cycle by themselves

`00469760`/`00469770` (`+40`/`+41`) are held modifiers; `004166CD` turns a
ring only when `turn_14 != 0`, and `turn_14` comes from the **digital**
Left/Right consumers (`0053A368`/`0053A370`) — the left stick's turn goes to
the analog turn `00423200` and never reaches `turn_14`. The PS1's gesture is
"hold L2/R2, tap a direction".

With d-pad left/right as the ring keys there is no digital turn left, so a
d-pad tap only slides the panel open and cycles nothing. Options:

1. **One-shot steps (recommended, usable):** d-pad left = `004696E0` (primary
   −1), d-pad right = `00469700` (secondary +1); both slide the panel open and
   step once per press. This uses retail's `+28`/`+2C` requests, not the ring
   keys.
2. **Faithful gesture, steers:** bind each d-pad code to *two* rows — the ring
   key (`+40`/`+41`) and the digital turn (`0053A368`/`0053A370`). Holding
   left then cycles the primary ring every 20 clock ticks and also turns the
   vehicle left. This is retail's own gesture with the d-pad standing in for
   both L2/R2 and the direction.
3. LB/RB hold the ring keys and the stick turn is wired into `turn_14` — that
   would change `0046A260`/`00423200` and is not retail.

This is also where the LB/RB "previous/next gun" overlaps: those are the
one-shot primary steps, the same action option 1 gives d-pad left. The user
should say whether the duplication is intended (two ways to step the primary)
or whether LB/RB should be the ring modifiers instead.

### 8.3 LT analog thrust loses the turbo unless the trigger is also digital

`+64` is the analog throttle; the turbo/jump request (`+3A`) comes from the
**digital** thrust consumer (`0053A378`, `+78`): `0046A260` sets `+3A` while
`+78 > +7C`, and the vehicle's jump request spends a `+12C` charge. A purely analog LT therefore
drives forward but never triggers the boost.

Recommendation: bind code 9 (LT past `0x4000`, the same half-trigger rule the
Console layout uses for L2) to `0053A378` as well. That is two bindings for
one physical trigger — analog on axis 4, digital at half travel — which is
exactly how the PS1 has both Cross (digital thrust) and the right stick
(analog). It also makes hint 869 ("Use LT to activate turbo booster") true.
Without it, hint 869 must stay keyboard-worded and the turbo is unreachable
on the Remastered pad.

### 8.4 "LB/RB page" has no retail page action

No input table binds a page command. The frontend's "pages" are menu rows
(`00453BE0` pushes a sub-page; Up/Down choose, Confirm activates); the phase-6
database and hint list step one entry with codes 0/1 from Up/Down; the map's
page-like action is zoom (`00452180/B0`, Insert/Delete, already L1/R1 in the
PS1 table). The auxiliary commands (`00485300` = exit, `0042AA00` = restore
defaults) are not pages.

Recommendation: LB/RB duplicate Previous/Next in the menus (a retail row
step, one row per press) and zoom on the map. If the user wants a real page
jump of N rows, that is new port behavior, not a retail action.

### 8.5 "View opens the map where the screen has one" has no target

The only map entry is the gameplay screen's `004436D0` (M / WinMM button 11 /
Select+Triangle), which is R3 here. None of the listed menu screens has a map
callback; the in-world pause's rows are Continue / Options / Quit, and the CPU
panel's Map row is a menu row inside `00413410`. On the map screen itself View
closes it (the any-button Close row). If "where the screen has one" meant the
CPU panel's Map row, that is a new action for the panel, not a retail binding.

### 8.6 A on a key-configuration row starts an unescapable capture

`KeyConfigScreen::tick` reads only adapter slot 0 (the keyboard) for a
capture; no pad can complete or cancel one. A confirms the selected row,
which starts the capture, and the screen's held slots are inert during it. A
pad-only player would be stuck until a keyboard key is pressed. The Console
layout has the same limit, but Remastered invites pad-only play.

Options: let a Remastered B/pad press cancel the capture (new behavior), make
A do nothing on binding rows for Remastered, or leave it and document the
keyboard requirement. This needs a decision.

### 8.7 A selects the CPU row and also drops cargo

A is `00469670` (beam down / drop) and, for "A selects", `004697F0` (message
select). The drop is not gated by a shown message: `00417731..004178ED` tests
only the 250 ms `+204` cooldown and network, so pressing A on a CPU row also
drops a carried unit. Retail never had this pair: its select is Cross/Enter
and its drop is Triangle/D.

Options:

1. **RT selects** (retail's own behavior: `0046A27D` turns a shown message's
   Fire into select) and A keeps only Beam Down. This is the
   side-effect-free path and is what retail players use.
2. Bind A to both and accept the drop (it is a real, visible consequence:
   scientists/cargo fall out while using the computer).
3. A port-level suppression of `+3C` while a shown message is present,
   Remastered-only. That changes vehicle behavior, so it needs to be a named,
   agreed deviation.

### 8.8 Existing analog gaps the Remastered table would paper over

`parse_phase5_input_tables` and `parse_phase7_input_tables` encode no analog
rows because the constructors snapshot runtime cells (`00533398`,
`005333E8`); the assets therefore install no analog sets for the pause and
map screens, and the **Console layout's stick does not move those menus**
either. The Remastered slot can carry `005333A0` (a real authored table) to
fix its own case, but the Console case stays broken. This is a pre-existing
fidelity gap: the constructors' snapshots are retail evidence, so filling the
type-9 slot for every layout would be a correction, not a Remastered feature.
It is out of scope here; the user may want it as a separate change.

## 9. Hint wording

`control_hints.rs` holds the PS1 release's sentences for the nine control
global IDs (37–40, 866–870) as templates filled for the layout and pad family.
Console + Xbox keeps the port's original bytes, Console + PlayStation uses the
PS1's own names, and the Remastered layout names its own bindings in either
family.

### 9.1 The wording selector

The message environment and every draw context carry a small `Copy` enum
(`crates/inf-game/src/control_hints.rs`) instead of the old `pad_hints` bool:

```rust
pub enum PadWording {
    Retail,                    // PC text, byte for byte (the default)
    Console(PadFamily),        // PS1 sentences, named for the Console pad
    Remastered(PadFamily),     // PS1 sentences, named for the Remastered pad
}

pub enum PadFamily { Xbox, PlayStation }

pub fn pad_hint(wording: PadWording, global_id: u32, language: u8)
    -> Option<Cow<'static, [u8]>>;
```

`ControllerHints::wording(…)` keeps the existing gate's shape: `Retail` for
the Retail setting, a missing pad, the PC layout or a network game
(`0055F448`); `Console(family)` for the Console layout and
`Remastered(family)` for the Remastered layout otherwise.

The table stores the PS1 sentence per language as a template with button
tokens (`{MODE}`, `{TURBO}`, `{SELECT}`, `{DROP}`, `{CONFIRM}`, `{SAVE}`,
`{BACK}`, `{START}`). Filling resolves each token to the layout's physical
button, then names it for the family:

| Token | Console layout | Remastered layout |
| --- | --- | --- |
| confirm | Cross | A |
| back | Triangle | B |
| save / retry | Circle | X |
| mode | L1 | Y |
| turbo | Cross | A |
| select | Cross | A |
| drop / beam down | Triangle | Y |
| start | Start | Start |

Console + Xbox reproduces the old table's bytes exactly
(`the_console_xbox_table_keeps_its_bytes` pins them). Console + PlayStation
names the PS1's own buttons: Cross, Circle, Square, Triangle, L1, R1, L2, R2,
Start, Select. Remastered + Xbox names A, B, X, Y, LB, RB, LT, RT, View,
Start; Remastered + PlayStation names Cross, Circle, Square, Triangle, L1,
R1, L2, R2, Create, Options (Start is Options). The six non-English banks are
retail's translations with the button word substituted; strings stay Latin-1
and the H4 glyph sweep covers every layout and family.

ID 865 stays the PC text (it is not a control hint and the port's factory
beam behavior is not the PS1's). The old table's wording choices stay: 866
and 867 name the mode button (Y on Remastered; LB or L1 on Console), not the
word "mode".

### 9.2 Deriving the pad family

`sdl2` 0.38 (Cargo.lock) does **not** wrap the query: `GameController` has no
`r#type`, no `raw()`, and `controller.rs` has no `GameControllerGetType`
binding. `sdl2-sys` 0.38 does expose it in its generated bindings, and
`sdl2` re-exports the crate (`pub extern crate sdl2_sys as sys`), so no new
dependency is needed:

```rust
let kind = unsafe { sdl2::sys::SDL_GameControllerTypeForIndex(index as i32) };
```

`index` is the device index already passed to `ControllerSubsystem::open` in
`SdlPads::open_at`. `SDL_GameControllerType` is available since SDL 2.0.12;
the bundled SDL2 is newer. The port has two families:

| SDL type | Family | Names |
| --- | --- | --- |
| PS3, PS4, PS5 | PlayStation | Cross, Circle, Square, Triangle, L1, R1, L2, R2 |
| everything else (XBOX360, XBOXONE, SWITCH_PRO, JOYCON_*, Luna, Stadia, Shield, unknown) | Xbox | A, B, X, Y, LB, RB, LT, RT |

Everything but SDL's three PlayStation types names Microsoft's positional
letters, which is what an 8BitDo in XInput mode and SDL's generic pads
report. `SdlPads` stores the family with each `OpenPad` at open and exposes
`family()` for the pad in use.

### 9.3 Replay determinism

The family is live SDL state, not in the pad sample, so a replay must not read
it live. `SessionRecorder::family(family)` writes `c <family>` (`c xbox` or
`c playstation`) whenever the active pad's family changes; the first call
writes it too, before the first `p` line. `SessionReplay` parses it into
`ReplayEntry::PadFamily`, and the replay cursor's latest value is what the
wording uses. `frontend_app` passes the replay's family when replaying, else
`SdlPads::family()`, into `SdlPlatform`'s `pad_family`, which
`world_tick_inputs` reads. A replay without a `c` line (all older files) reads
as Xbox.

Hints are display text only: the family never reaches the simulation, and a
mid-session pad swap only changes later hints. A recording that changes pads
writes one `c` line per family change, so the replay matches.

## 10. Fidelity and network

- **PC and Console stay byte-identical.** Their devices still report types 2
  and 9; their slots 2 and 9 still hold the retail parser's tables; the
  parser and `ScreenInputTables` for those slots are untouched. The Remastered
  tables occupy slot 8, which no PC or Console device ever selects. The only
  shared edits are additive: a new enum variant, a new `SdlPads` family query,
  and `joystick_live`'s type-8 test (which is retail's own status-input set
  and cannot change a type-2 or type-9 pad's result).
- **Opt-in and default.** `ControllerLayout::Pc` stays the default (serde
  `#[default]`), the launcher row gains a third entry, and `--controller-layout
  remastered` exists. A remastered choice keeps retail's as its default, as
  the repository rules require.
- **Network games.** A Remastered pad sends only retail callbacks and retail
  control-block values; it adds no message, no actor state and no simulation
  branch. So it is compatible with the network game's "retail's game on every
  renderer" rule, like the PC and Console pads. The hint wording stays PC in
  network games through the existing `0055F448` gate. If the user prefers a
  stricter reading ("network games play retail's input device too"), the
  launcher can hide Remastered while a network session is active; that is a
  policy choice, not a technical constraint.
- **No retail function changes status.** Every callback the Remastered tables
  bind is already ported; the new module only binds existing actions to new
  device codes, so `docs/progress/status.csv` needs no new rows.
- **Labeled port data.** The Remastered tables are port-authored binding data
  (not retail bytes). The module doc must say so, every descriptor is parsed
  from the retail image, and the tests pin both facts: the retail slot 9
  tables are unchanged and the Remastered rows name retail descriptors.

## 11. Open questions for the user

1. **B in the CPU panel.** Accept L3-as-toggle instead of B-back, or add the
   message-shown B→`00443700` gesture (section 8.1)?
2. **D-pad rings.** One-shot steps (usable), or the faithful ring-key plus
   digital-turn double row (cycles and steers, section 8.2)? If one-shot, is
   the LB/RB primary duplication intended?
3. **LT digital thrust.** Bind code 9 to `0053A378` at half travel so the
   turbo and hint 869 work, or keep LT analog only?
4. **A select.** Keep A on both message select and beam down and accept the
   drop, use RT as the select (retail's own path), or add a Remastered-only
   suppression while a message is shown (section 8.7)?
5. **Save and retry.** Name X (Square) for the results save and the name-entry
   retry (recommended), or keep those keyboard-only and keep prompts 37/39
   keyboard-worded?
6. **Start in menus.** The spec says Start only pauses/closes. Retail's PS1
   tables also confirm with Start in the frontend, results, text entry and
   startup; keep those rows (and "Start or A" prompts) or drop them?
7. **LB/RB paging.** Row-step duplicate, or a new page-jump behavior? The map
   uses them for zoom either way (section 8.4).
8. **View's menu role.** Nothing in the listed menus opens a map; is View
   only "close the map" there, or should the pause/CPU menu grow a Map row
   (new behavior, section 8.5)?
9. **Type 8 vs type 10.** Reuse retail's analog PlayStation type and its
   unused slot 8 (recommended, least code), or mint a port-owned type 10?
10. **Key configuration.** Let a pad press cancel a capture, disable A on
    binding rows, or document the keyboard requirement (section 8.6)?
11. **Phase-5/7 analog.** Should the existing Console-layout stick gap be
    fixed in the same change or separately (section 8.8)?
12. **Network.** Permit Remastered in network games (recommended, it is a pure
    input remap) or hide it while a network session is active?

## Evidence and limits

- Retail executable: `Infest.exe` (1,445,888 bytes, SHA-256
  `bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956`),
  mapped by `inf_formats::FrontendExecutable`. Every table row and descriptor
  above was dumped from it with the throwaway test described at the top.
- Input runtime: [INPUT_RUNTIME.md](INPUT_RUNTIME.md) (`004EFD40`–`004F1080`,
  `004F0F80`, `00439B30`, `00439AD0`), [GAMEPLAY_RUNTIME.md](GAMEPLAY_RUNTIME.md)
  (`00440990`, `00442B50`, `00469450..004698E0`),
  [VEHICLE_DRIVE_RUNTIME.md](VEHICLE_DRIVE_RUNTIME.md) (`00416170`,
  `00418F60`, `004166CD`, `00417731`), and
  [PLAYER_MESSAGES.md](PLAYER_MESSAGES.md) (`00413410`, `00413950`).
- The PS1 code identities and the type-8/9 driver are in
  [CONTROL_HINTS_PS1.md](CONTROL_HINTS_PS1.md) section 4/6 and
  [INPUT_RUNTIME.md](INPUT_RUNTIME.md#the-playstation-release).
- The SDL API statement is from the locked crates: `sdl2` 0.38.0's
  `src/sdl2/controller.rs` has no type query or public raw pointer;
  `sdl2-sys` 0.38.0's generated bindings declare
  `SDL_GameControllerGetType`, `SDL_GameControllerTypeForIndex` and
  `SDL_GameControllerType`.
- Not established here: the exact PS1 ring-key gesture was inferred from the
  table rows and `004166CD` (the digital `turn_14` path), not from a retail
  recording; the "ship only" cargo-rotate wording has no mode gate in any
  callback (the rotate request is consumed by any vehicle and is a no-op when
  the hold is empty); and the family names for Nintendo/8BitDo generic mode
  are a wording choice, not retail text.
