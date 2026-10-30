# Infestation results-screen runtime

This document owns the phase-8 screen's constructor, held-input wrapper,
Tick/Draw ordering and remaining feature boundaries. The
[results screen construction](INTRO_SEQUENCE.md#results-screen-construction) owns its
production integration. Shared profile buffers and identity effects
remain in [profile runtime](PROFILE_RUNTIME.md#results-working-profile-wrappers),
base held/repeat/analog state in [input runtime](INPUT_RUNTIME.md#screen-held-input-and-repeat),
and phase arbitration/resource lifetime in [runtime foundation](RUNTIME_FOUNDATION.md).
Original executable code establishes the native matrix below; portable
implementation and retail observations have separate bounds.

## Phase-8 results screen

The world update reaches phase 8 through attract/demo completion and gameplay
results paths; the results screen owns the menu controller, save operations
and the campaign-loop request back to briefing:

| Address | Role |
|---|---|
| `0042A970` | Factory, constructs through `0042AB00` |
| `0042AB00` | Constructor over base `00437F90` (own frame: `push ebp`) |
| `00521724` | Screen vtable (mapped from the image: destructor `0042AE10`, tick `0042CFC0`, draw `0042C2B0`, shared presentation `0040D1D0`, standard select/deselect, class query `00402700` returning 4, reuse `0042AE00` accepting only phase 8, held wrapper `0042D460`, held slots `0042D3B0/0042D370/0042D390/0042D330/0042D350`; when `+F7` is set these forward menu commands `4/0/1/2/3`, otherwise they do nothing; `+38/+3C` are the authentic bare RET `00401310`) |
| `0042D460` | Held-bit wrapper `+20`: nonzero `+0x150` clears `+0xC4/+0x150` and returns; otherwise base dispatch, then bits `0x40/0x80/0x100/0x200` run `0042D3D0/0042D440/0042D3F0/0042D510` (profile-menu push, fade control and timer arming; see the held-wrapper matrix below) |

The constructor builds a menu controller at `+0x100` (`00453380` with
fade rate `0xFFFF` and WORD viewport `[0, 0, 0x400, 0x400]`), clears `+EC..+F9`
with `+FC=3`, initializes the
`+0x130..+0x150` result fields and publishes the vtable. `0042AC48`
then clears the shared loading mode BYTE `0054694C`, before device enumeration
at `0042AC4E`. The port applies this write to the actual platform owner, retaining
it even if enumeration fails. It then enumerates a
single inline array set (slot 0 `005212A8`, slot 2 `00521410`, slots 6/7
`00521568`, slots 8/9 `00521648`, every analog cell zero with no shared-cell
reads). Conditional identity/score setup follows installation, gated on
gameplay `+0x1E0 == 1` (mission `W+30`, the live mission-selection DWORD):
`00463550` reads name/initials with caps 31/4,
then `+F3/+F5` are set and H0 slot `330h` supplies the format string to
`00511B56`; `[0053146C]` supplies the output destination. The
[player prefix](INTRO_SEQUENCE.md#results-screen-construction) then performs
three fresh first-player lookups, silent command `37h` and the raw `+11C`
WORD copy to screen `+144`. Input attachment/selection `004947E0` occurs
after that prefix at `0042AD3C`; later save/profile work remains separate.
The shared mode and its loading consumers are owned by [loading runtime](LOADING_RUNTIME.md#mode-and-shared-effect-writers).
The [results owner](INTRO_SEQUENCE.md#results-screen-construction)
uses the existing shared `MenuStack` for this constructor and device-before-menu
teardown order; menu ticking, rows and results pixels remain unowned.
Tick `0042CFC0`
(`0042CFC0..0042D31F`, thiscall, `ret 8`) is mapped end to end from
retail disassembly; inner-call semantics stay open where noted:

1. Campaign-loop head: world field `+0x250 == 0x16` or attract set
   requests phase 3 (`00438380(3)` via screen `+E4`) with **fall-through**
   (no return) — the live 8→3 hit (caller `0042CFF5`) matches this
   return address exactly.
2. `+F7`-gated controller/status block: when `+F7` is set, the menu
   controller at `+0x100` runs virtual `+0x10` with the scheduler timing
   word; a nonzero full EAX skips status refresh but still reaches step 3.
   Otherwise `+F7` clears and the
   save owner (`004391B0` → session `+0x660`) runs `00449C90(0)` into
   `+FC` — status 3 sets `+EC=1`, any other status sets `+F8=1`.
3. `+F6` clears when set; base input `004392B0` runs with the timing pair.
   Its first phase adds signed raw elapsed microseconds divided by 1000
   into the screen's authoritative `+0xC0` age before analog/repeat dispatch.
4. Timer accumulation: signed capped arg1 scaled by the `0x10624DD3>>6`
   reciprocal adds wrapping into `+0x14C` and the same `+0xC0`, then
   process clock `00439560` advances with original arg1. Strict signed
   `+0xC0 > 4000` sets `+F4=1`. With `+F8` and `+F7` both clear,
   desktop leaf `00449C80` returns AL false (`XOR AL,AL; RET`) and
   stores it to `+F8`; it reads no receiver or save backing. This is
   separate from status query `00449C90` and disk-write attempt state.
5. `+EC` countdown: when nonzero it grows by the scaled timing past 1000,
   opening the save/profile block — a `00511C90` dispatch over the
   `0055F4E8` region (gated on `+F0==0`, `+F8==0`, `+FC==3`) selects
   between save completion (`00449C10`/`0044A110`, `00494A00`,
   `+F0=1/+F1=0`) and error-scope destroy (`004923C0`, `+F1=1/+F8=1`);
   then `+EC=0` and, with `+F1` set and `+F7` clear, the derived profile
   menu `00449A00` over `+0x100` runs with `+F7/+150` armed and
   `+F1/+F0/+F6` cleared.
6. Present `+0x130/+0x134/+0x138` objects tick through `004554E0`.
7. With `+F2` set and `+F3` clear: `+0xC0=4000`, `0042B190`/`0042B350`/
   `0042B510` run, `+F3=1`.
8. Tail exits on nonzero `+0x148`: after clearing `+0x4E/+0x4F` on the
   three objects, `+0x148` is compared unsigned against the unsigned
   reciprocal of arg1. If greater, it subtracts the separately recomputed
   signed reciprocal with DWORD wrapping and returns. Otherwise it
   resets to 1 and requests phase 2 when `+F5` is set, else phase 3 —
   the 8→2 / 8→3 tick exits. A zero `+0x148` skips to the epilogue.

The [intro Draw branch](#intro-results-draw) has a complete bounded owner;
general results content remains open. The [`results_tick`](../../crates/inf-game/src/results_tick.rs)
owner implements the head gate and both reciprocal scalers as pure
decisions. Its one-scaled-value `+0x148` helper is equivalent only where
signed and unsigned scaling agree, including nonnegative scheduler timing;
it does not model the complete negative-argument tail. Save/profile/object
effects retain their separate required owners.

Input tables decode statically with the generic digital parser (retail
corpus test pins counts 17/16/10/10, shared pairs and the exact 13-entry
descriptor set): results callbacks `0042AA30/0042AA60/0042AAA0`,
`0042A9D0`, the class query address itself as a callback, frontend
navigation rows, `00485300`, and the shared Alt+Esc/Alt+Enter pair without
the Alt+Enter-noop record. Slot 2 is sixteen joystick buttons on the
dominant `0042AA60`. The scalar `0042AA60`/`0042AAA0` callbacks and
exit arm `0042D4D0` have a static source map; production delivery and their
reached score/save/menu effects remain unowned.

Observed in the recording: the results tick requests phase 3 with caller
`0042CFF5` (the return address inside tick `0042CFC0` after its
`00438380(3)` call) when the world field `+0x250` reads `0x16` or attract is
set. See the briefing completion ledger
for the full 4→8→3 trail.

### Phase-8 screen held-bit wrapper (vtable `00521724`)

An earlier reading attributed the following to the text-entry screen, but
`0042D460` is slot `+20` of the phase-8 screen's vtable `00521724` (referenced
at `00521744`). A nonzero byte at that screen's `+0x150` (meaning
unestablished) clears `+0xC4` and `+0x150` and returns without dispatching.
Otherwise, after the virtual dispatch, bits `0x40/0x80/0x100/0x200` run
`0042D3D0`/`0042D440`/`0042D3F0`/`0042D510`. Screen `+0x100` is a menu
controller constructed by `00453380` (`0042AB48`, `0042AB9D`). The first two
handlers call `004537E0` with commands 6 and 7 when `+0xF7` is set, forwarding
them to the controller's current menu. `0042D3F0` passes that controller to
`00449A00(false)`, which heap-allocates, constructs and pushes the derived
profile menu, when `+0xF7` is clear. It then calls controller virtual `+08`,
writes `+0xF7=1` and clears `+0xF1`, `+0xEC`, `+0xF0`, and `+0xF6`.
`0042D510` calls controller virtual `+04` when `+0xF7` is set, then writes
`+0xF8=1` and clears `+0xF7`. When `+F7` is
clear it instead calls `0042D4D0` to arm `+148`, leaving both bytes unchanged.
The controller's vtable is `0052561C`: `+04` is `004151C0` (`+24 = 0`, fade
out) and `+08` is `004151D0` (`+24 = 1`, fade in). The base slots `+24..+34`
(`0042D3B0/370/390/330/350`) send commands 4/0/1/2/3 the same way while
`+F7` is set.
These offsets are the phase-8
screen's own fields. Source `0042D4D0` bypasses when `+F7` or `+148` is
nonzero. Otherwise nonzero `+F5` bypasses the `+F9` read and arms `+148=1`.
With `+F5` clear, nonzero `+F9` also arms 1; both bytes clear arms 1000.
This leaf writes only `+148`; it does not modify either flag.

### The profile menu

While `+F7` is set, the tick's head runs the controller's `+10`
(`00453560`) with the tick's delta. Once it returns false (its fade has
reached zero, which destroys its menus), the tick clears `+F7`, stores status
query `00449C90(0)` in `+FC`, and starts the save countdown (`+EC = 1`) for
status 3, or sets `+F8`. The draw runs the controller draw `00453610` instead
of the page's text (`0042CA85`); the batch before it (the planet, the models
and the panels' frames) still draws, and the tail still builds panel 1.

The menu is the frontend's [profile menu](PROFILE_RUNTIME.md) with gameplay
byte `+5C2` set, so its header reads "Save Game" and it cannot change mode.
Activating an empty slot, or confirming an occupied one, runs operation 6,
which selects the slot (`+30`) and fades the controller out; the save that
follows writes the working profile there.
A failed save opens it as `00449A00(true)`, whose two-second warning (H0
`0xC7`) shows first; a save that could not be tried (`+F8`, or `+FC` not 3)
passes `[EBP-1] = 0` instead. Esc hides it at once (`0042D510`): the hidden
menu stays on the controller, unticked and undrawn, and a later open pushes
the new one above it.

The lab's `results-profile-menu-terranova` scenario reaches the page through
the Cheats Menu's Skip to, opens the menu with Backspace (`profile_menu` at
pass 340), then Down and Return save the game into the second slot.

## Implemented fresh first Tick

The bounded [intro results screen](../../crates/inf-game/src/intro_results_screen.rs)
now owns the complete reached first `0042CFC0` pass. Scheduler ordering is
the admission: input Poll targets the old gameplay screen, the transition
constructs results, and linked Tick then targets that new screen. No results
input Poll intervenes. Explicit Fresh/InputPass/Completed/Stopped progress
records this custody; the diagnostic Tick count does not select a mode.
The actual linked Poll invalidates fresh admission before delivering input.
Later/input-exposed passes retain distinct logged feature boundaries.

The source-backed fresh fields are `+EC/+F6/+F7/+F2/+F8/+148/+150 = 0`
and NULL `+130/+134/+138`. Base constructor `00437F90` supplies one
`ScreenInput` with `+C0/+C4/+C8/+D4 = 0`. The first pass performs:

1. Submit the genuine world-22 phase-3 head request through the existing
   first-request-wins session owner, then continue the current Tick.
2. Bypass menu/status refresh and F6 clearing under their actual zero gates.
   Add raw elapsed milliseconds to the same ScreenInput C0 before required
   analog reads. Read genuine D0 at its strict `>2500`/`<-2500` comparisons;
   reach CC only while D0 remains in range. Execute the shared analog,
   held, repeat and latch phases against those current consumers.
3. The resulting direction bits can reach the five native held slots, all
   of which are F7-gated no-ops here. Back's slot is `00401310`'s authentic
   bare RET. Fresh C4/150 and the absence of a results input pass exclude
   the derived wrapper's extension/preclear arms. `Base` dispatch alone
   is not claimed as the complete general `0042D460` implementation.
4. Add signed capped milliseconds wrapping into `+14C` and that same C0,
   then advance the retained `FrontendProcessState` animation with capped
   original microseconds. Strict signed C0 greater than 4000 sets F4.
   The lazily reached constant-false `00449C80` leaves F8 clear without
   borrowing a profile store or querying saved state.
5. Actual EC zero skips Serialize/disk/menu work; NULL children, F2 zero
   and tail148 zero bypass their separate object, score and exit arms.

For raw elapsed 250000 and capped 125000 microseconds, C0 becomes 375 ms,
14C becomes 125 ms and the existing process animation advances 125000 us.
Gameplay seconds/remainder, profile disk bytes and directory/working-buffer
custody are separate owners and remain unchanged by this Tick. No new clock,
scene-specific timing table, default analog backing or profile-save success
substitute is introduced.

A failed required head request retains the diagnostic Tick prefix and stops
before base input. A missing reached analog consumer retains the completed
raw-age store and stops before 14C/animation. Stopped progress prevents
replaying either partial pass. These are typed failure bounds, not native
raw-pointer fault or nonlocal-error parity. The later passes, input, save,
panels and page follow in [the general Tick](#the-general-tick-and-input) and
[the results page](#the-results-page), whose final world's page rolls the
[credits](#the-final-worlds-page).

## The general Tick and input

After the first pass, the port runs the whole Tick above, for the intro and
for every world. The intro's world 22 still asks for phase 3 at the head;
any other world waits for the player.

- **Input.** Space and G reach `0042AA60`. It shows the stats (`+F2`), or
  once they are shown or 4 s have passed (`+F4`), it arms the exit
  `0042D4D0`: `+148 = 1000`, or 1 for the final world or with `+F9`. S
  reaches `0042AAA0`, which starts the save countdown `+EC = 1` (`0x3E9`
  with `+F8`) and shows the stats before 4 s. The other keys set held bits.
  Esc sets bit 0x200 and Backspace bit 0x100; the arrows, Enter, Y/O/J/S and
  N set the base and extension bits. A newly held bit runs the held pass
  `0042D460`. Its head clears `+C4` once after a save (`+150`). The
  menu-gated slots and bits 0x40 and 0x80 command the profile menu while it
  is shown. Bit 0x100 opens it (`0042D3F0`). Bit 0x200 hides it or arms the
  exit (`0042D510`). See [the profile menu](#the-profile-menu).
- **Save.** Past 1000 ms of `+EC`, unless already saved, a save that can go
  ahead (`+F8` clear, `+FC = 3`) serializes (`00449C10`) and writes
  (`0044A110`) the working profile, giving "Game Saved" (`+F0`). A failure
  sets `+F1` and `+F8` and opens the profile menu `00449A00` with its
  warning.
- **Panels.** Once the stats are shown, the age restarts at 4000 ms and
  `0042B190`, `0042B350` and `0042B510` build the three panels: 500 ms
  stages at scale `0x6000`, at x 900 and y 150, 500 and 850. They ask for
  stage 2, and `004554E0` steps them each tick.
- **Exit.** The countdown drops the panels to stage 0. Once it has run down,
  it asks for the briefing (phase 3), or for the frontend for the final
  world.
- **Draw.** The draw `0042C2B0` changes state as well as drawing, and the
  port runs those changes each frame before its pixels. With no panel, the
  draw's tail builds panel 1 (`0042B190`), so the panels come in without a
  key. A panel 90% into its stage builds the next one: panel 1's draw
  `0042B6D0` builds panel 2, and the draw builds panel 3 from panel 2.
  The builders keep an existing panel and only refill an exhausted timer.
- **Score.** Once panel 1 reaches stage 1, its draw sets the score `+140`
  each frame, unless the world's status has bit 4. The score is 150 for each
  of two ACAMs not lost (`+144` counts the lost ones), plus 10 per kill, less
  30 per collateral loss. Kills are the mission record's `+A0`, `+A8`, `+AC`
  and `+B0`; collateral losses are `+8C`, `+B4` and `+C8`.
- **Medal.** Once panel 2 reaches stage 1, its draw `0042BBF0` awards the
  medal once (`+13C`, -1 until then). The medal is the score in hundreds,
  from 0 to 3, and is added to the first player's rank `+11E`. The award
  needs the status line to be showing: after 4 s that line blinks off every
  other half second. It also needs a completed world (status bit 2 without
  bit 4) and `G+1C5` clear. A save made before the award keeps the old rank,
  as in retail.
- **Menu and final world.** The profile menu's draw replaces the panels'
  draws, but the tail still builds panel 1. The final world (`+F5`) builds
  none.

The session admits phase 8 after any world's end (`00487560`). Its phase 3
loads the selection the completion stored and shows that world's briefing.
`a_world_end_reaches_the_next_worlds_briefing_through_the_results` drives
this from a loaded world 11 to world 2's briefing. The page itself is
[drawn](#the-results-page), the final world's [its own way](#the-final-worlds-page).

In Infest02, after Level 1's gate, a key at about 1.1 s started the save and
showed the stats. The save landed at tick 17 and a second key armed the
exit at tick 22; see the gate ledger entry.
The page read "Kills: 8", "Collateral Damage: 0", "01:47", "Tokens
remaining: 71", "Medal awarded : Silver", "Overall Rank : Black", "ACAMs
Lost: 1", "Game Saved" and "Press <Esc> or <Space> to continue". That score
is (2 - 1) x 150 + 8 x 10 = 230, so the medal is 2, Silver. The trace's
draws built panel 1 at the first draw, set the score `0xE6` at the sixth and
awarded the medal at the fifteenth. At the fourteenth, at 5213 ms, the
status line had blinked off. The medal was in before the save at tick 17.
`infest02_results_match_the_trace_tick_by_tick` replays the 30 ticks with
the trace's deltas and keys. Every tick's `+C0`, `+14C`, `+EC` and `+148`
match, as do `+C8` after the S key and those draw writes.

## Intro results Draw

Draw `0042C2B0..0042CFBA` is thiscall with one surface argument and authentic
`RET 4` at `0042CFB8`. Its admitted intro branch draws no content:

1. Read actual attract BYTE `0055F14C` at `0042C2B0`. Nonzero jumps directly
   to the epilogue before any renderer, surface-width or mission operation.
2. Execute complete renderer preparation `00439270`
   at `0042C2D1`: replace the arena, reset the batch and publish the renderer
   using three fresh lookups.
3. Read surface width through `00495900` at `0042C2D8` as a full DWORD in
   pixels. `0042C2E8` captures game `+1B0` with **LEA**; it does not
   dereference a mission field or justify an early required-mission check.
4. Read width again at `0042C2F2`. Compute wrapping DWORD `3 * width`,
   interpret it signed, then divide by 5 with truncation toward zero
   (`66666667h` reciprocal). The value is discarded on the intro branch.
5. Read the actual current-world DWORD at game `+250` afresh (`0042C314`).
   Equality to `22` returns before session `+648`, height, fonts, model
   state, score/menu content, flush or surface-release effects.

The screen's data-bearing Draw request supplies live attract and an optional
loan to the actual `WorldMission`. Portable Ready/missing-screen validation
precedes effects; it is a custody check rather than native raw-fault parity.
Attract bypasses even missing draw-only mission data. Without attract, a
missing mission is reached only after both widths and retains the completed
preparation. Widths are not WORD-narrowed or converted to authored layout
units. Optional HD output retains the actual classic framebuffer width.

The production software host borrows the real scene, surface and existing
renderer-publication state. It replaces the actual command arena and submits
the actual batch reset, without eager depth clearing or surface validation.
The intro exit preserves pixels, depth and unrelated model/render globals;
the separate linked presentation callback still presents and clears colour.
Host failures retain their reached prefix and propagate. A later non-22 Draw
goes on to [the results page](#the-results-page) after both widths. Backend,
missing-owner and lifecycle errors are not consumed as successful drawing.
The intro's branch changes no state; any other world's draw changes the
screen's (the panels, score and medal of [the general Tick](#the-general-tick-and-input)).
The first Tick's phase-3 request does not cancel this same-pass Draw or Present.
The next genuine transition destroys Results and loads briefing.

Tests exercise both width reads, full-DWORD values, attract bypass, world22,
the later world's page outcome and every host failure prefix. Production
scene tests verify real arena/batch/publication and preserved
pixels/depth/globals; the linked intro-to-briefing test verifies
pending-request lifetime. These checks do not close matched retail
cinematic acceptance.

## The results page

After the shared prefix, any world but the intro draws the page
([results_page.rs](../../crates/inf-game/src/results_page.rs)):

1. `004A4400` opens a batch. The planet background `004737A0` is centred at
   (3w/4, h/2) with radius 3w/5, scaled by `+148`/1000 while the exit
   counts down, and turned by `+14C`. `004737A0` reads only its center,
   radius and age, so the briefing's port of it serves both. Renderer
   `+14(0)` then resets depth.
2. Style 5 lights model 98, as the briefing's decoration does: directional
   (-2365, 2365, -2365)/4096 in its colour, ambient a quarter of it. The
   model is drawn at depth 1300 and centred at (60w/256, 65h/256), turned
   `0x1000` about X-Y and `+C0` x 4 about X-Z. Over the first second its
   far fade closes in by `extent * 3 * zoom >> 15`. A second copy at
   (60w/256, 190h/256) undoes those turns, adds a quarter turn about Y-Z and
   turns again. Only the projection is published for it.
3. The frames (`00455080`) of the panels that exist when the draw starts:
   H8 slot 99, extent `0xB000`, orientation 3, at (900, 150), (900, 500) and
   (900, 850) in 1024ths of the surface. A panel the draw builds gets its
   frame from the next draw.
4. `004A4460` draws the batch. The text follows in font 0 and style 5,
   right-aligned at 91w/100 in steps of h/20. Each line's reveal (descriptor
   flags 2) starts 300 ms after the one before:
   - While the exit counts down, H0 `0x162` ("Stand by... Warp in
     progress") is centred at h/2, revealed from 1000 - `+148`. It clears
     the descriptor's flags, so the later lines draw whole.
   - Panel 1, at stage 1 or more, from h/10 and `+C0` - 1000: the kills,
     the collateral losses, the time and the tokens left. The time is H0
     `0x30B` ("Duration: %d secs ") cut at its first '0', then G+1E8 as
     `%02d:%02d`. The tokens are G+5B4, or H0 `0x32A` when none are left.
     An abandoned world (status bit 4) shows H0 `0x31F` alone.
   - Panel 2, from 38h/100 and `+C0` - 2000. First the aliens-cleared pair
     (H0 `0x314`/`0x315`), every other half second while G+1C6 is set. Then
     the status line, blinking after 4 s: H0 `0x15B` with G+1C5, otherwise
     `0x31A` for a completed world, `0x328` with bit 8, else `0x31B`. Then
     the medal (H0 `0x313` and `0x316` + medal), the rank (H0 `0x31E` and
     `0x355` + clamp(`+11E`/3, 0, 11)) and the ACAMs lost (H0 `0x326`).
   - Panel 3, at 82h/100 and `+C0` - 3000: the save line, wrapped at 4w/5.
     It is H0 `0xA6` blinking while the save counts down, `0xC7` after a
     failure, `0x327` once saved and `0x25` before. Without a profile it is
     `0x2E`, with `0x97` a step above. Then H0 `0x26`, the prompt, wrapped
     at w/2 and no further left than 2w/5.

Infest02's draws at `4EB0FC`, `4ED669`, `4EF2BE` and `4F0F24` give every
text's string, position, wrap width and reveal age, and the background's
arguments (`pageText` in the gate ledger entry).
The `infest02_*_page_matches_the_trace` tests reproduce them with the
retail fonts at 1024x768. `a_worlds_results_page_draws_with_the_sessions_residency`
draws the page after Level 1 through the session. The profile menu draws
over the page's batch in place of its text.

### The final world's page

When the campaign ends (the completion stores selection 1), the constructor
reads the player's name (`00463550`, caps 31 and 4) and formats H0 `0x330`
("Commander %s was posthumously awarded") with it into `00546A98`, the table
entry that names the player. The page (`+F5`) then draws differently:

1. No planet and no depth reset. Style `0xE` lights model `0x2B3` alone, at
   depth 1300 and centred at (w/2, h/3), both ends of its fade `0xFFFF`. It
   faces the camera (half a turn about X-Z) and sways by
   `(sine(+C0 x 4) << 13) >> 15`, `0x2000` at most either way, a cycle
   every 16.4 s. No panel frames.
2. The credits roll (`0042AED0`) instead of the exit line and the panels.
   The table `005313F0..00531AA4` has 143 entries of twelve bytes: an H0
   slot (811 to 852, the epilogue and the headings), or with slot 0 the
   address of a name in the executable or of the player's line (none: a
   blank row), and the bold byte `+8` (H6 style 4, else 5). Entry `i` sits
   at `h + i * row - offset`, where `row = h * 5 / 100` and `offset` is
   `(h * +C0 / 14400) mod (139 * row + h/2 + 2 * row)`, unsigned. A line at
   or above `h/2 - 2 * row` is skipped and the first below `h` ends the
   roll. Within four rows of `h/2 + 2 * row` above, or of `h - 4 * row`
   below, the colour's three bytes fade by `256 + 256 * depth / (4 * row)`
   (at least 0). Each line is centred on the width in font 0 through
   `004A3AB0`, its descriptor's flags 2 (the shared glyph base).
3. The save line at h/10 and `+C0` - 3000, still timed: centred unless
   wider than 4w/5, then from 91w/100 less 4w/5; without a profile H0
   `0x97` a step above it, centred. No prompt follows, and the tick's exit
   goes to the frontend.

check_credits.py runs `0042AED0` natively over 72
surfaces and ages; [results_credits.rs](../../crates/inf-game/src/results_credits.rs)
matches every line, and `the_final_worlds_page_rolls_the_credits` draws the
page with the retail fonts: at 11.25 s on 1024x768, entries 4 to 15 show the
epilogue's end ("...retaken by the year 2241.", "Commander Tester was
posthumously awarded", "the highest honour bestowed by MilCon -", "The Gold
Cross") and "Game Saved" sits at y 76.
