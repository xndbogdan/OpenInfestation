# Infestation frontend runtime

This document owns the authored startup/title/menu evidence and the boundary
between decoded assets and executed presentation. The phase controller and
resource lifetime remain in [Runtime foundation](RUNTIME_FOUNDATION.md), and
digital/analog dispatch and screen repeat are documented in [Input ownership](INPUT_RUNTIME.md).

## Screen classes and virtual calls

Screen virtual `+0x18` returns a constant **class ID**, not a teardown action
and not the session phase. Transition `004383B0` calls it, discards the result,
captures the same screen's `+0xE8` context, then asks virtual `+0x1C` whether the
screen can serve the requested phase. Stored session `+0x64C` remains the source
of the previous-phase branches.

| Session phase | Vtable | Class query | Class ID |
|---|---|---|---:|
| 0 | `00521DA0` | `00401300` | 0 |
| 1 | `0051F600` | `00404740` | 1 |
| 2 | `005284D8` | `00483B70` | 2 |
| 3 | `00520338` | `0040AA00` | 3 |
| 8 | `00521724` | `00402700` | 4 |
| 4, 6 | `00523370` | `00440B50` | 5 |
| 5 | `005233D8` | `00443320` | 6 |
| 7 | `005255C8` | `004511B0` | 7 |
| 9, 10, 11 | `00526028` | `00459260` | 8 |
| 12 | `00524DC4` | `00444D50` | 9 |

The startup/title/frontend tick callbacks are `00433640`, `00404810`, and
`00484E00`; their draw callbacks are `004330E0`, `004047F0`, and `00484940`.
Their reuse callbacks accept only requested phases 0, 1, and 2 respectively.
`ScreenKind` in the port keeps these class IDs separate from `SessionPhase`.

## Phase-0 startup (language) screen

The language carousel is constructed specially by application startup
(`004389C0`, while current/requested phases remain 1) and by the Options
Language row (`00453C70(0)`). The port owns it from the Options row:
[startup_screen.rs](../../crates/inf-game/src/startup_screen.rs) holds the
state machine and [startup_draw.rs](../../crates/inf-game/src/startup_draw.rs)
the carousel. The port still boots into the title directly (see
[First-run boundary](#first-run-boundary)).

| Address | Role |
|---|---|
| `00433810` | Factory: allocates `0x1B0` bytes, constructs through `00432500` |
| `00432500` | Constructor over base `00437F90`, input context `004F0D00` at `+0xEC` |
| `00521DA0` | Vtable: destructor `00432720`, tick `00433640`, draw `004330E0`, presentation `00433620`, class query `00401300` (0), reuse `004326F0` (phase 0 only), held dispatcher `00439470` with setters `00432700`/`00432710` at `+30/+34` |
| `004327B0`/`004327C0`/`004327E0` | Digital callbacks: accept, previous, next |
| `00433730` | Accept |

### Construction

The constructor zeroes direction `+194`, angle `+198`, age `+19C`, idle
`+1A0` and byte `+1AC`. It copies settings `+64` raw into the selection
`+190` and the H7 slot `0x20` word into the clear colour `+1A8`. It then
enumerates devices (`00439B30`) over the screen's `+0x130` array, attaches
them (`00439D00`) and selects the context (`004947E0`). It saves and stops
the music through `00487230` and requests resource leaf 2 (`004EE1E0(2)`).
Leaf 2 and the frontend's leaf 47 share one Select branch, so leaf 47
unloads; the rebuilt frontend reloads it.

Process byte `00546B24` makes every construction after the first start
with the age at `00546B20 + 00532890 + 00532894` (2,000,000 µs), both
fades complete. The byte is then set. Retail's boot constructs this screen
first, so an Options visit always starts faded in; the port's title boot
sets the byte in its place.

Finally, session byte `+65C` requests phase 1 through `00438380`.
`00438749` sets that byte only when the `004631E0` registry settings load
returns, so it means "settings existed at startup". From the Options row
the creating transition is still pending and drops the request; the
carousel stays until its accept.

### Tick and input

The tick runs base input `004392B0`, then adds the delta to the age. Once
the age passes `00546B20 + 00532890` (1,000,000 µs), a pending direction
is applied: the selection wraps over 0..5 and the angle gains
`(direction << 16) / 6`. The angle then decays to seven eighths on every
tick. The idle time grows by the same delta; past `00532898`
(30,000,000 µs) the tick accepts. `00546B20` is initialized-data zero and
has no writer.

Corrected input tables (the earlier reading started one slot late):
slot 0 `00521AE0` is the keyboard, with Return (not with Alt) and Esc
accepting, Left and Right turning, plus Alt+Enter. It has no Alt+Esc
row. Slot 2 `00521B58` maps sixteen joystick buttons to accept. Slots 6/7
`00521CB0` and 8/9 `00521D28` hold five rows each. Every descriptor uses
flags 0. `004327B0` accepts on a press. `004327C0`/`004327E0` zero the idle
time on any edge and store direction -1/+1 on a press. Held analog bits
reach the same setters through `00439470`.

### Accept

`00433730` does nothing until the age gate passes. Inside a setjmp scope:

- A changed language is written to settings `+64` before `004EE160`
  reselects it, with settings `+0C` clamped to 4 as the display tier.
- Settings persist (`00463100`) only when `+65C` is clear. Otherwise the
  rebuilt frontend's own `00463100` saves them.
- It requests phase 1.

A caught exception deletes the error object (`004923C0`) and still
requests phase 1. The title factory then finishes at once (process byte
`005468D8`) or on its first tick, and phase 2 rebuilds the frontend.
Startup display setup reselects the residency with settings `+64`
(`004389B0`: above 5 selects 0), so the port boots in the saved language.

### Carousel draw

`004330E0` runs the `00439270` preamble, then returns while the age is at
most `00546B20`. Otherwise it resets the batch and the depth surface
(renderer `+14(0)`). It refreshes the shadow colour `+1A4` (H7 `0x22`) and
the clear colour `+1A8`:

| Age | `+1A4` | `+1A8` |
|---|---|---|
| Up to 1 s | H7 `0x22` faded in by age/1 s | H7 `0x20` |
| 1 s to 2 s | H7 `0x22` | Blue (`F8` blue, `40` red and green) faded in from black |
| From 2 s | H7 `0x22` | Blue |

Each fade masks a display channel's top five bits as a byte, scales it and
masks it again. Six flags sit on an ellipse with radii 300 and 220 around
(500, 280), each radius growing over the first second. The flag at the
selection is in front, and the angle term turns the ring. Each flag's
closeness (0..255 from the angle) scales it and sets the wave amplitude.

The shadow strip `00432940` submits renderer `+84` quads in colour
`+1A4`. The cloth strip `00432DB0` submits renderer `+C4` quads with mode
`0x100` (affine), sprite H3 `0x4A0 + index` and per-corner shades H6
`0x10 + n`. `n` is clamped to 0..16 from the wave's cosine, and the shade
fades with the age. Both strips walk the same corner sequence: edge A
along the top then the right side, edge B down the left then the bottom.
That gives 88 quads per strip. The vertex helpers `00432800`/`00432B60`
bend each corner by a sine wave of `u·0x234 − age/16 + v·0x80`. The
shadow helper also pushes its corner right and down by the flag height.
H0 string `selection` is centred at nine tenths of the height through
`004A3AB0`, in H4 font 0 and H6 style `0x2A` (faded in), with descriptor
`+1C` zero (the shared glyph base word). Presentation `00433620` presents,
then clears the next surface with `+1A8`.

Infest01 visited the carousel from the Options menu: a constructor at
`36621D`, seven ticks of 125,000 µs (first at `366291`), then accept
`004327B0` at `36A4CE` with no turn. Its first draw submitted 528 `+84` and
528 `+C4` packets. Their flag positions and the first shadow and cloth
corners, UVs and shades match the port exactly
([startup_draw_tests.rs](../../crates/inf-game/src/startup_draw_tests.rs)).
Infest01's display was RGB565, so H7 `0x22` (grey 4,4,4) travelled as
`0x2104`; the port's RGB555 target holds the same colour as `0x1084`.

**StartupCarouselDepthResidueApproximation.** Neither vertex helper
writes a corner's depth DWORD; the packets carry stack residue. Infest01
shows `0050DCB9`/`1` for shadows and `038286C0`/`038320F0` for cloths.
With the renderer's saved depth policy (enable and write 1), the retail
occlusion between a shadow and its cloth is unknown. A depth of 1 gives z
far below zero, and the hardware's clip or clamp is not recorded. The port
submits `0x7FFFFFFF` for every carousel corner, so submission order
decides overlap. Geometry, colours, shades and UVs are native.

### First-run boundary

The port boots through the title factory. Retail's boot builds this screen
first: with settings present its constructor requests phase 1 at once;
without them (first run) the carousel waits for a language. Routing the
port's boot through factory 0 would show the carousel to every run
without a settings sidecar, including the input-replay fixtures, so it
stays a separate decision.

## Phase-12 key configuration screen

Options > Controls row 3 (`00453C70(0x0C)`) requests phase 12. The port
owns the screen:
[key_config_screen.rs](../../crates/inf-game/src/key_config_screen.rs)
holds the state machine, [key_bindings.rs](../../crates/inf-game/src/key_bindings.rs)
the records it edits, and
[key_config_draw.rs](../../crates/inf-game/src/key_config_draw.rs) the
binding rows.

| Address | Role |
|---|---|
| `00444AC0` | Factory, constructs through `00444BD0` |
| `00444BD0` | Constructor over base `00437F90` and menu controller `00453380(FFFF, 0, 0, 400, 400)` at `+0xEC` |
| `00524DC4` | Vtable: destructor `00444D70`, tick `00444E80`, draw `004450B0`, shared presentation `0040D1D0`, class query `00444D50` (9), reuse `00444D60` (phase 12 only), held dispatcher `00439470` and the sixteen slots below |
| `00444B20` | Row action: on Activate, `004450F0` starts a capture of row `n` |
| `00444B50` | Restore Defaults: copies the default records |
| `004452A0` | Binding row draw |

The constructor clears the capture byte `+120` and stores the session's
current phase `+64C`, still the previous phase mid-transition, at `+12C`.
It enumerates one inline array set, attaches, selects, and pushes the root
page `00535E40` (`00452310`, `00453440`). The earlier slot reading named
`00522B48` (phase 5's keyboard); the constructor writes:

- slot 0 `00524B90`: back `004451E0` on Esc and G, Return without Alt
  `00445200`, the arrows `00445220/40/60/80`, plus Alt+Enter, Alt+F1 (no-op)
  and Alt+Esc (exit);
- slot 2 `00524C70`: sixteen joystick buttons on `00445200`;
- analog slot 2 snapshots `00533398`; everything else is zero.

### Input and pages

Every held slot is inert during a capture. Outside one, `+24` activates
(`004537E0(4)`), `+28/+30` move up and `+2C/+34` move down. Back `+38`
(`00445140`) pops a page while the controller holds more than one
(`00453540`, `004534D0`), otherwise requests the phase at `+12C`. The root
(21 rows, initial selection 1) has a header, seventeen binding rows (row
arguments 0..16), the two page rows `00453BE0` (H0 `0x3B4` to `00535B88`,
rows 0x23..0x2B; H0 `0x3B5` to `00535B70`, rows 0x11..0x22) and Restore
Defaults (H0 `0xC3`). Each page ends in a Back row `00453C50`.

Infest01 visited the screen from `32C1F5` to `3529C9` (41 ticks). Eighteen
Next commands moved from row 1 to row 19, Activate pushed `00535B70`, the
first Back popped it and the second requested phase 2 from `0044517F`. The
[screen tests](../../crates/inf-game/src/key_config_screen_tests.rs)
replay that sequence.

### Bindings

The 44 row-table entries at `00535658` (six bytes: H0 label, action index,
capture byte) name gameplay records through `00444E50`: actions below 38 at
`[00534CA0]` (`00534570`), the rest at `[00534410]` (`00534290`). A capture
(`004450F0`) sets `+120`, clears the release wait `+128`, stores the row at
`+124` and sets the row's capture byte. The tick waits until no key and no
joystick button is held, then takes the next press. Held keys go into
the record's first required list in ascending order through `004F5230`;
a fifth overruns its four bytes natively. A joystick button
takes the action's descriptor, every other button holding it becomes the
no-op `00535650`, and binding Fire (`0053A388`) copies the button code into
the joystick table's terminator byte, which nothing reads. Restore
Defaults copies whole records from `[00534CA4]`/`[00534414]` and
`[00534F70]`; the first default table also forbids Alt (`+0C = 38`), which
the startup image does not.

The registry pair `00445570`/`00445680` stores each row's first code as its
key name and each bound button as its action name (`005361D0`). The load
resolves a key name to the first matching code, so unnamed codes reload as
code 0 and only the first code of a chord survives. An unbound button has
no name and skips `RegSetValueExA`, so its previous value stays. The
[settings owner](../../crates/inf-game/src/frontend_settings.rs) applies the
same rules to its sidecar. `00463100` calls `00445570`, so the rebuilt
frontend saves the edits.

### Draw

`004450B0` runs `00439270`, unlocks the surface and draws the controller
over the session's caller context (`00453610`). Header and page rows are
the generic `00453A80`. A binding row copies its seven descriptor words and
narrows the copy to three quarters of the row width. It then passes the
action name left and, right:

- while capturing, H0 `0x3A4` on odd 300 ms steps of `005471C0 / 15`;
- otherwise each of the four codes as `"%s "` of its key name, in H6 style
  `0x25` (`0x26` on the selected row) when another action shares the first
  code. `00444E00` exempts the CPU cursor actions `0x20..0x24`.

A joystick button bound to the action's descriptor adds `"Joystick %d"` at
the original width and colour. The copy is never written back, so each
binding row spans one row spacing. The gameplay constructor installs these
live records: see [the gameplay input dispatch](GAMEPLAY_RUNTIME.md#input-dispatch-architecture).

## Title handoff

Factory `00404A10` creates a `0xF4`-byte screen through constructor `00404620`
(base `00437F90`, `+3B/+3C` cleared, vtable `0051F600`, device enumeration
`00439B30`, slot attach `00439D00`, context select `004947E0`). `00404890`
opens `Title.avi`; tick `00404810` finishes when the handle is absent or
playback status (`00494DB0`, handle `+18`) is 2. Finish `004049C0` closes
playback (`00494DC0`), restores the display setting (`00463AB0` +
`00437DF0`), requests phase 2, and sets screen `+0xEC` to 1.
Process-global `005468D8` skips subsequent title invocations. Requests during
factory construction remain subject to the session's pending flag; the
accepted phase trace proves one such phase-2 request is ignored. A bounded
replay from the accepted `0->1` pending position (`36A4CE:141D`, bundle
`.tmp/infestation-ttd/20260927-192033-551-PhaseTransitions/`) pins the
second-title sequence: factory finish `004049C0` requests phase 2 while
current=0/pending=1 (return address `00404A00`, ignored), then the title tick
requests phase 2 again at current=1 (accepted) and the `1->2` transition
follows. Vtable `0051F600` is destructor `00404760`, tick `00404810`, draw
`004047F0` (arena preamble `00439270`, then one `00494F80` frame when a
handle is held), empty presentation `00404840`, select/deselect
`004947F0`/`00494810`, class query `00404740` (returns 1), reuse `00404750`
(accepts only requested phase 1) and held dispatcher `00439470`. Teardown
`00404780` closes a present handle, releases sets `00439D50` and destroys the
base. The [`title_screen`](../../crates/inf-game/src/title_screen.rs)
owner implements this state machine behind a required video/display/phase
host, and the production session serves it end to end: `boot_title`
constructs it through `00404A10` over the initial Title state, linked poll
drains notifications and routes the skip callback (`00404870`) plus the
shared system callbacks with no held-bit dispatch, linked tick runs
`00404810` (the absent video backend takes the failed-open path, so the
factory makes no request and the first tick finishes to phase 2), linked
draw runs only the `00439270` arena preamble through the CPU renderer, and
linked presentation stays empty (`00404840`). The scheduler then transitions
Title→Frontend through the real effects (frontend music, leaf-47 factory,
input baselines); that transition is the first leaf-47 activation, so the
residency revision advances and renderers rebind. The linked round trip is
proven by `boot_title_reaches_frontend_through_tick_and_transition`.
DirectShow graph construction (`00494AE0`) and IV50 frame decoding and
presentation stay explicit boundaries. A video host must implement this
lifecycle before title playback can be claimed. The AVI container itself now
has a canonical owner: [`inf-formats::avi`](../../crates/inf-formats/src/avi.rs)
frames `Title.avi` (640x480, 15 fps, 315 `00dc` chunks, 21 x 88200-byte PCM
audio chunks) and exposes raw chunk bytes plus stream metadata with a retail
corpus test, while IV50 pixel decoding stays open. Bounded replay proves the
recording's first title invocation opened the graph (`03828700`) and drew
frames with status 0 through `00494F80`, while the second invocation took the
skip-once path with a null handle; the draw tail calls `0050D330` with a COM
object, so decoded pixels never sit in game-owned buffers and reference
recovery must target the DirectShow/DirectDraw surface (see the capture
ledger). The graph layout is now observed, not just decompiled: at the
first draws the 0x40-byte struct holds `Title.avi` in UTF-16 at `+0`,
status 0 at `+18`, the created interface at `+1C`, dimensions 640/480 at
`+28/+2C`, and four heap COM interfaces at `+30/+34/+38/+3C`; globals
`0065EE4C`/`0065EE94`/`0065F110` read nonzero/0/2 at the draw stop
(`6B945:13C`). Decoder identity is observed too: the graph loads the
K-Lite LAV Video chain (`avcodec-lav-58` + `LAVVideo.ax`, FFmpeg
lineage) over the quartz AVI Splitter, with audio through
DSound/MSACM; `ir50_32.dll` never loads in the recorded session, so
the M1 pixel reference must come from the FFmpeg Indeo 5 lineage and
a port-side FFmpeg decode mirrors the observed retail path (see the
capture ledger). Decoder-strategy decision (recorded 2026-09-28):
transcode `Title.avi` offline with a documented ffmpeg into lossless
frames plus WAV and play those in SDL, with no runtime codec
dependency. Rationale: the recording proves retail itself decoded
through FFmpeg (LAV Video), so the reference lineage and the
transcode tooling coincide; linking FFmpeg libraries would add a
heavy system dependency for one 21-second clip, and a pure-Rust
Indeo 5 would have to prove equivalence to ffmpeg-ivi from scratch.
TTD spot-verification of LAV output against the local transcode
still owns the documented tolerance before pixel acceptance can be
claimed.

M1 acceptance smokes (all headless deterministic: hidden 640x480,
fixed-step 16 ms, dummy SDL drivers, from ``):

- Full play: `--frames 1700` presents the whole 21 s video, then the
  menu draws 378 rows with 10348 glyph passes, frontend music starts
  (track 8), and the snapshot shows the complete main menu — the
  natural-finish handoff with video held.
- Skip: `--frames 400` with input replay `Space` at frame 100 stops
  title audio at ~1.25 s of mixer output (vs ~20 s full-play), starts
  frontend music and scene (2709 model draws) — the skip-callback path
  in the real app.
- Pixels: presented snapshots hash-match the transcoder's own `rgb24
  framemd5` entries bit-exactly (mid-play frame 77 `be6690…`, end-card
  hold 292–314 `45cd50…`), and the transcode PNGs round-trip the
  transcoder RGB output exactly (frame 0 `bfe5c5…`). Our pixels therefore
  equal ffmpeg-7.1 RGB presentation with zero tolerance consumed.
- Second invocation: unit-covered (`repeat_factory_finishes_immediately_without_reopening`).
  Boot/handoff suite: full `inf-game` lib green.

Open M1 boundaries: A/V sync is by construction (wall-clocked video,
device-clocked voice) but observed only through dummy drivers, whose
pacing is not wall-comparable — real-hardware observation pending.
The lav-58 vs ffmpeg-7.1 ivi-drift spot check from the recording is
still open; it bounds the only remaining pixel risk.

## Authored menu records

Canonical decoders live in
[`inf-formats::frontend`](../../crates/inf-formats/src/frontend.rs). They accept
a bounded mapped byte range with its preferred virtual address; they neither
load executable code nor invoke stored callback pointers. Each record retains
its source span and virtual address, including runtime-mutated fields.

The root definition at `0053C088` is 20 bytes: item pointer `0053C008`, signed
item count 3, layout pointer `0053C068`, initial selection 0, and flags 0.
`00452310` installs it; `00452570` draws and `00452E20` dispatches actions.

| Offset | 32-byte item field |
|---|---|
| `+00` | Flags |
| `+04`, `+08`, `+0C` | Draw callback and two raw arguments |
| `+10`, `+14`, `+18` | Action callback and two raw arguments |
| `+1C`, `+1E` | Mutable row height and age, two unsigned 16-bit words |

The root's text arguments are string IDs 204 (Start Game), 158 (Options Table),
and 210 (Network). Actions are `00483490`, `004834B0`, and `0045FD40`.
These identify the port's explicit callback dispatcher, not host function
pointers. Callback-specific argument meaning belongs to its consumer.

Layouts are 16-byte records: four bytes for flags, item count, visible rows,
and a preserved reserved byte; signed 16-bit x/y/width/height; unsigned 16-bit
row spacing; signed 16-bit scroll animation. The list ends when its first
byte is zero, even if other bytes in that record are nonzero. Coordinates use
the menu consumer's 1024-unit viewport. Root geometry is `(500,640,450,900)`,
three items/visible rows, spacing 50. Decoding does not reset mutable fields
or claim that this layout can be rendered without its owning menu state.

[`FrontendExecutable`](../../crates/inf-formats/src/frontend_executable.rs)
maps only file-backed `.data` and `.rdata` sections after checking the supported PE32
machine, timestamp, preferred base, and image size. This is a structural profile
check, not hash authentication or executable loading. The runtime's
[`FrontendAssets`](../../crates/inf-game/src/frontend_assets.rs) owns those
original bytes and reads the root, nine model records, ten camera presets,
4096 original sine samples, both scale words, and the authored digital/analog
input tables directly. Input-table spans retain executable file offsets; menu
table spans retain their original section-relative convention. Its process-lifetime `FrontendMenuCatalog` interns
live menu records by definition address. Distinct definitions that alias mutable
arrays are an explicit unsupported case; the root, Quit, Options, and four
generic Options children use distinct arrays in the supported corpus.

## Executed generic menu ownership

[`menu`](../../crates/inf-game/src/menu.rs) implements base menu vtable
`00525608` and its controller, with concrete row execution and a required
rendering host; derived menu classes remain separate. `00452310` stores shared
item/layout pointers: constructing another
instance resets shared item ages and layout scroll animation, but preserves
row heights written by drawing. Selection, menu age, basis, navigation lock,
and scroll start belong to the individual instance. The seed basis at
`00547590` is initialized by `00439630` to a zero rotation counter and float
identity matrix; `00402680/00402690` establish its signed-ratio conversion and
component order.

`00452E20` swaps commands 0/1 with 2/3 when definition flag 1 is set. Previous
and Next honor the navigation lock, owner-derived visibility mask (4 or 1),
nonselectable item bit 8, and layout-local wrap flag 8. The action path does
not repeat those checks; callbacks may replace or remove their current menu.
Scroll calculations consume measured unsigned row heights and signed animation
words. Entering/leaving age resets use whole-menu modulo, with distinct upward
and downward traversal. Malformed cycles and out-of-bounds tables fail explicitly.

`00452470` accumulates truncated milliseconds per call, saturates item ages
only at the unsigned upper limit, decays signed scroll animation through native
Q31 extraction, then rotates the persistent basis by `delta_micros / 100`.
The concrete [`menu_basis`](../../crates/inf-game/src/menu_basis.rs) owner
executes that rotation using the original unsigned quarter-wave table at
`0053DF00`, four-unit angle quantization, quadrant reflection/sign, and native
scale bits `0x38000100` (approximately 1/32767). Renormalization occurs on the
**101st** call: the incremented unsigned counter must exceed 100. It preserves
the distinct retained/spilled normalization lengths and nearest-even `FISTP`
quantization through signed 16-bit components scaled by 4096. This math is
available to the required host; an empty rotation callback is not execution.

Controller `00453380` takes a **fade rate**, not a resource ID. `00453560`
advances fade using `(signed_delta / 128) * rate`, native wrapping arithmetic,
and clamping to 0..65535. It ticks the current menu only at full fade and after
an actual controller draw has set `+25`. Zero fade destroys the stack.
Push/replace/pop clear the drawn flag; pop resets the newly revealed menu's
ages, even for a zero pop count. The draw owner below sets the acknowledgement
only after successful controller execution; a tick never fabricates it.

### Generic row drawing

[`menu_draw`](../../crates/inf-game/src/menu_draw.rs) executes `00452570` and
the base-controller `00453610` boundary. `MenuStack::draw` requires actual
controller setup, clip operations, basis publication, and row callbacks through
`MenuDrawHost`. Controller vtable `0052561C` has a proven empty background
virtual `+0C` at `004537D0`; the current menu draws only at fade 65535. Successful
controller setup still sets the drawn flag during partial fade or with an empty
stack, matching `004537B0`. A failing host does not create an acknowledgement.

Controller setup copies the renderer's original context template and projection
globals, retains the caller's context `+0` and `+6C`, and writes surface width,
height, and half-width into `+40/+44/+48`. It overrides `+4C=0`, `+64=2048`,
and `+68=4096`. `0043B5D0` sets direction `(-2365,2365,-2365)/4096`, light RGB
128/128/128 and ambient RGB 32/32/32. Context fields whose rendering consumers
are not yet owned retain offset names in the API. The required host performs
these backend operations; constructing a request alone is not a draw.

The row walker samples visibility mask 4 or 1 once, publishes all ten menu basis
words to the `005483D0` domain, and saves/restores the current clip. Each authored
layout scales its signed coordinates through the controller's 1024-unit viewport
using wrapping i32 products and division truncating toward zero. Descriptor and
clip coordinates narrow to signed i16; context origin coordinates remain i32.

| Layout flag | Draw behavior |
|---|---|
| `02` | Per-item age and reveal `min(age*50,32767)`; otherwise menu age and reveal `65535` |
| `04` | Half-width shift for the clip; callback descriptor flag `02` |
| `08` | Item-index wrap by this layout's count when the whole-menu end is reached |
| `10` | Retain selection-driven scroll even when visible items fit |
| `20` | Circular integer sine placement, saved clip, callback descriptor flag `10` |
| `80` | Callback descriptor flag `40`, except for item flag `10` |

Linear scrolling adds `spacing*scroll_animation/200` to the local signed-word Y.
Nonzero animation adds one visible row; positive animation begins at the previous
item and subtracts its stored height times spacing. The skip counter advances
only over visible items, while tail backfill sums unsigned heights without a
visibility filter. Animated outgoing rows receive descriptor flag `20` when
layout flag `02` is set. Circular angle is
`(scroll_animation + (item_index-selection)*200) * (65535/(item_count*200))`;
item flag `08` fixes that angle to zero. Circular X and negative Y use the
original u16 sine sample replicated into both halves of an i32, signed Q31
products, spacing and `spacing*3/2`. The descriptor holds these scaled offsets;
the context origin holds the authored layout center separately.

Options `00539378` has eight logical items but layout counts 1 and 13, with
the latter's visible-row count also 13. Native visibility census reads adjacent
authored records before dispatch later stops at the logical menu end. Without
layout flag `10`, `visible <= item_count <= visible_rows` proves the initial
skip is zero for every possible census result. The port takes this equivalent
early-zero path without assigning adjacent mutable records to this menu. The
original layout counts remain intact; the Options corpus test exercises them.

Selected color interpolates RGB over `menu_age % 400`, a sawtooth. Item flag
`10` uses ordinary color and clears selection/text flags; item `40` uses the
selected-end color unless a selectable selected row takes the interpolation
branch. The combined `08|40` path uses selected-end directly. The animated
selected branch initializes only RGB; complete color-word copies preserve the
fourth byte without assigning alpha semantics.

Each `MenuRowDrawRequest` carries the original callback address and arguments,
pixel geometry, origin, placement mode, flags, age, reveal and color. Whole-word
color copies retain their fourth byte; animated selected rows leave it explicitly
unknown because the native branch initializes only RGB. The host
executes the actual text/model callback with mutable menu access. No shared-table
borrow spans it. The callback's mutated descriptor Y then supplies the stored
unsigned height:

```text
row_y = ((returned_pixel_y - viewport_origin_y) * 1024) / viewport_height
height = (row_y + spacing/2 - local_authored_y) / spacing + 1
next_authored_y = local_authored_y + spacing * low16(height)
```

Both divisions truncate toward zero and native word/i32 wrapping is preserved.
Spacing is re-read after the callback; later rows see age resets, colors,
visibility and layout mutations. The owner commits completed rows before a
later failure. Public `MenuStack::draw` restores the saved clip before returning
an error; the enclosing frontend frame's native `0x301` path skips that restore,
matching the retail longjmp, and creates no draw acknowledgement. Invalid divisors,
out-of-bounds records and malformed traversal fail explicitly. Tests cover the
retail root callbacks and 800x600 geometry, circular table arithmetic, scrolling,
live mutations, and failure boundaries. They do not establish rendered pixels.

Concrete label layout for root `004834F0` and generic `00453A80` lives in
[`menu_text`](../../crates/inf-game/src/menu_text.rs). It consumes original
string bytes and H4 fonts, preserves measurement/drawing differences, wrapping,
reveal, shadow/color passes, and callback Y feedback. Glyph sprite submission
through renderer virtual `+A4` remains required. Font structure, the supported
callback families, and their unresolved boundaries belong to
[Text runtime](TEXT_RUNTIME.md).

## Frontend control and callbacks

[`frontend_control`](../../crates/inf-game/src/frontend_control.rs) joins the
menu stack, camera, animation clock, input commands, and timers. Its required
host owns actual base-input processing, music, settings, sound playback,
phase/exit requests, and specialized menus. Uncaught failures stop the owner after any
partial effects, preventing an unsafe retry of dispatched actions.

| Callback | Recovered effect |
|---|---|
| `00483490` | Command 4 sets deferred Start Game; a later tick calls `00449A00(false)` to create derived profile menu `00448220`, not a gameplay phase |
| `004834B0` | Command 4 selects camera preset 2, then performs generic child push |
| `00453BE0` | Command 4 pushes the definition in the **second** action argument |
| `00453C50` | Command 4 pops one menu |
| `00453C70` | Command 4 requests the session phase in the first action argument |
| `0045FD40` | Command 4 creates derived network menu `0045F9C0(false)` / vtable `0052621C`; generic menus cannot substitute for it. Network play has no port owner, so `NetworkMenuRefusalApproximation` plays refusal sound 4 (the completed-slot `004391D0(4)`) and keeps the root menu; the dispatcher's confirmation 5 follows |
| `00483620` | Quit No: command 6 exits; commands 4/7 restore the root |
| `004836C0` | Quit Yes: commands 4/6 exit; command 7 restores the root |
| `00453B80` | Commands 2/3/4 adjust the settings DWORD at the second action argument with the first argument as maximum; other commands are ignored |
| `00463B50` | Forwards the same command/maximum/address to `00453B80`, then runs the `0040F740` world refresh, which returns immediately with no loaded world |
| `00462F40` | Display-aware adjust of the row's address/maximum with variant handling and the backend-reset/rollback contract (below) |
| `00463AC0` | Sound row: the same `00453B80` adjust, then the FX master from settings `+00` (below) |
| `00463B00` | Music row: the same `00453B80` adjust, then music resume and AUX volume from settings `+04` (below) |

Back clears the base held mask before the camera gate. When settled, it cancels
editing or saves settings and pops; an empty stack then opens Quit at `0053C130`.
Confirmation and navigation play resources 5, 2, or 3 after dispatch; Back has
no sound. Commands 6/7 bypass the camera gate and do not reset idle time.
Root draw `004835E0` selects camera 0 before text callback `004834F0` **on every
draw**. Options header `0045F520` selects the preset in draw argument 1 through
`00485320`, then delegates to `00453A80`. These effects require actual row-draw
dispatch, not a synthetic tick notification.

### Settings value rows

Draw `00453AE0` shows a fixed H0 label on the left and the current setting's
string on the right. Its draw arguments are the H0 base ID and the setting's
absolute DWORD-cell address. The cell's low WORD selects the right string as
`base + value + 1` with native 16-bit wrapping; the label draws `H0[base]`.
Every side lookup tests the signed low WORD (`test si,si`) and indexes with its
sign extension, so upper argument bits never select a string. A side renders
null when that index is negative or above the H0 pool count (`0055EE58`);
addressing exactly one past the pool is an explicit error instead of the
native overread. The value string (EDI) resolves before the label (EAX), and
both precede either text call, so a later failure submits no glyphs. The left
call uses ordinary placement and the right call uses right placement through
the shared descriptor, matching the single native `00453810` call's sequencing.
Row draw callbacks receive the walker's first three arguments, the row
descriptor and the two draw arguments (`00452D5B..00452D73`); these consumers
pass the walker's second argument in ECX and the descriptor in EDX to
`00453810`.

Action `00453B80` takes the row maximum in its first action argument and the
same cell address in its second. Command 2 decrements with a zero floor,
command 3 increments clamped to the maximum, and command 4 increments
wrapping to zero once past the maximum, all over DWORDs with signed
comparisons. Live words resolve through the portable settings state's
`0054E8A8`-relative addresses. The Video device selector has its own owner
below; other cells outside the 34 words remain explicit boundaries. Ordinary
adjusts mutate live words only; the Back command (Escape) owns their persistence.

The first Options submenu (`00539088`) is the Video menu below. The second
child (`00538E68`) is the authored Sound menu. The third child (`00539188`)
needs no new owners: its two value rows reuse the `00453AE0`/`00453B80` pair
against their own cells. The
fourth child (`00539458`, Options row 5; row 4 is the `00453C70` phase-0
action, which reaches the session admission boundary) shows the player submenu:
draws `004639D0`/`00463A30` pair fixed H0 labels (`007A`/`007B`) with the
net-name and initials caches through the same left/right composition as value
rows (their draw arguments are unused). Their phase 10/11 actions enter the
shared text-entry screen, whose completion writes the net name or initials
and requests phase 2. `004639D0` calls `00463D10` into discarded
8-byte buffers on every draw, then draws the net-name cache; `00463A30` reads
nothing and draws the initials cache as the last read left it. Both rows use the
process caches shared with the text-entry constructor (see
[profile runtime](PROFILE_RUNTIME.md#name-entry-handoff)). The caches live at `0054EBC8` (net name: registry `Net Player Name`, then
the name cache, then `Jameson`) and `0054EBC0` (initials). The
[`player_identity`](../../crates/inf-game/src/player_identity.rs) owner now
recovers the full read contract: the HKCU key is
`Software\Frontier Developments Ltd\Infestation`, the legacy HKLM key carries
the `\1.0` suffix, and the shared loaded flag `0054EC59` selects cache copies
over fresh registry reads (set by a successful HKCU open, cleared by the
writers `004633D0`/`00463490`, and left clear when the open fails, so reads
retry). The previously unrecovered indexed initials default is
`sprintf("%dup", 0055F2F4)` at 004636AE/00463E89: standard CRT `%d` plus the
literal `up`, with `0055F2F4` the local slot: BSS, but 1 from boot (the
lab's retail read at the first pass, 2026-10-21; the transport's shutdown
`00493650` sets it to 1, and only the transport writes it), and in a
network game the slot the transport gave. So the offline default is `1up`
(retail's own registry image stores `1up`), and a network game's phase 9
numbers it by the slot: the host's `1up`. The port took 0 until 2026-10-21
and showed `0up`, and a name entry saved that. The owner also
preserves the 00463D10 quirk that queries HKLM initials into the net-name
buffer `0054EBC8` (so those bytes are swallowed and later cached reads return
them as the net name) and its empty-net-name fallback that copies the name
**cache**, never the registry. The entry screen's editable buffer separately
starts as `Novice` when the key cannot be opened. Successful frontend construction
calls `00463100` at `00483B41`, which creates the HKCU key before name entry.
A first read of absent identity values therefore yields `Jameson` and offline
`1up` in normal startup; later reads copy the caches until a writer clears the
flag. Portable custody treats missing sidecar fields as missing values
in that usable store; it does not turn them into registry-open failures.

### Video menu

Options item 1 pushes `00539088`, fourteen items headed **Display** and ending
in **Back**. Every value row except device item 2, labelled **Rendering**, uses
`00453AE0`. The authored labels, settings cells and action families are below;
parenthetical names identify native setting metadata when its name differs
from the visible label. Activating the final Back row only pops the menu;
the separate Back command, mapped to Escape, saves settings before popping.

| Item | Setting offset | Maximum | Action |
|---|---|---:|---|
| 1 Targetter (HUD) | `+60` | 1 | Plain `00453B80` |
| 3 Resolution | `+0C` | 4 (port: its list) | Display reset `00462F40` |
| 4 Colour Depth | `+44` | 2 | Display reset `00462F40` |
| 5 Bilinear Filtering (port: Filtering) | `+3C` | 1 (port: 6 on Vulkan) | Display reset `00462F40` |
| 6 Z Buffering | `+50` | 1 | Plain `00453B80` |
| 7 Display (Full Screen) | `+40` | 1 (port: 2) | Display reset `00462F40` |
| 8 Detail Level | `+70` | 1 | World refresh `00463B50` |
| 9 First Person | `+68` | 1 | World refresh `00463B50` |
| 10 Camera Style (Camera Speed) | `+6C` | 1 | World refresh `00463B50` |
| 11 Dynamic Lighting | `+80` | 1 | World refresh `00463B50` |
| 12 Pause On Hints | `+78` | 1 | Plain `00453B80` |

The authored word at `00538CD0 +70` is 0, Low. The port's startup and Restore
Defaults use High (1) instead (2026-10-27; `DETAIL_DEFAULT` in
`frontend_settings.rs`); a saved sidecar keeps whatever word it holds, and
H14 flag `0x2000` still forces coarse.

Device draw `00462DE0` initializes a list of at most four DirectDraw adapters
through `00462EE0`, then publishes its inclusive maximum at `0054EC4C`.
[`frontend_video`](../../crates/inf-game/src/frontend_video.rs) substitutes
the first four SDL output displays for that platform list. This is a portable
output-selection policy, not an enumeration of original renderer backends.
Startup's separate saved-name lookup `00463320` scans the full output list and
keeps its last match. First row draw clamps only the displayed ordinal to the
four-entry list; it does not republish the active device until an action runs.
The selected device name is copied up to 25 bytes; a copied length of exactly
25 receives `...`, including when the full name was exactly 25 bytes. SDL names
use their Latin-1 bytes where representable and `?` for other code points.

Device action `00463000` adjusts cell `0054EC50` using that dynamic maximum,
publishes the selected device, writes its full `Current Display` description,
then requests reset. This tail runs even for an unchanged or ignored command.
The portable sidecar stores the description immediately without saving other
unsaved setting words. A failed description write does not bypass reset.
On reset failure only the selected row value rolls back: the published device
and any completed description persistence remain changed. A later reset uses
that published device. Its local reset variant is 5 when signed settings
`+0C` exceeds 4, without writing the settings word; the normal authored range
is 0..4. Native display construction still indexes H5 using the actual
settings selector's low WORD, independently of that reset argument.

`00462F40` first runs a plain adjust and returns if the cell is unchanged.
Otherwise it normalizes the signed resolution word `+0C`: nonpositive becomes 1
without a reset, values above 4 become 4, and other values request reset.
A failed reset restores the adjusted row cell like the native setjmp handler;
normalization of a different cell remains. Caught reset errors leave the menu
operable and retain diagnostic custody of the error. If SDL cannot restore
the previous physical window after a failure, the host reports a fatal
rollback error rather than acknowledging a usable old display.

The port extends two of these rows (see
[display modes and resolutions](PRESENTATION_POLICY.md#display-modes-and-resolutions)).
Resolution steps through the original sizes and the monitor's reported modes
(in Borderless, only the desktop's size), and Display adds Borderless after
In a Window and Full Screen. Both keep this
contract and its tier-0 normalization over the port's lists, and draw their
authored strings for original sizes and the two retail Display values.

Native `00437CA0` copies a 20-byte H5 display record, then patches its renderer,
filtering and fullscreen bytes from settings `+38`, `+3C` and `+40`.
The portable reset supports the authored tiers 0..4: 320x240, 512x384,
640x480, 800x600 and 1024x768. Original H5 records 5/6 are 1280x1024 and
640x480; those extra selectors remain outside the portable display boundary.
[`frontend_app::display`](../../crates/inf-game/src/frontend_app/display.rs)
prepares replacement residency, H2 focal inputs, pixels, validated renderer
configuration and upload texture before acknowledging reset. It changes the
actual SDL window: its size, output display, border and exclusive or
desktop full screen state.
Later cues in the same input pass resolve the replacement residency. After
that pass, the old renderer's borrowed resources are retired before installing
the replacement and drawing; the admitted input pass is never replayed.

The display-mode adapter keeps a local native `SDL_DisplayMode` alive through
the synchronous `SDL_SetWindowDisplayMode` call, including rollback. This
avoids the `sdl2` 0.38 wrapper's pointer to a match-arm temporary that expires
before the FFI call. It queries the selected mode before entering fullscreen
and, for a shown nonminimized window, checks the active display mode as well.
Both must retain the requested dimensions; accepting SDL's nearest different
mode would disagree with the chosen resolution, so Full Screen also refuses a
size the monitor does not report. The port declares per-monitor DPI
awareness before SDL starts video, so these are physical pixels without DPI
compensation.

Bilinear `+3C` selects matching native D3D MIN/MAG states 17/18: byte zero
means point (1), nonzero means linear (2), consumed by the CPU texture sampler.
The port relabels the row Filtering, with its own text in the six languages
([`port_text`](../../crates/inf-game/src/port_text.rs), Latin-1 as the
retail strings are; Display's Borderless too). Values 0 Point and 1 Bilinear
are retail's; Vulkan adds 2 Trilinear and 3 to 6 Anisotropic 2x to 16x for
the traced world. Under OpenGL or Software a saved Vulkan value stays saved
but draws, and shows, as Bilinear; changing the row there starts from
Bilinear. The launcher offers the same list for the chosen renderer.
Renderer `+38` is not a Video row. Startup `004631E0` changes an exactly-zero
renderer DWORD to 1; the app applies that repair before narrowing the selector
to a byte. The portable backend then requires native renderer 1, whose
Direct3D submission policy it implements. The retail software renderer is a
separate unsupported path. Colour Depth `+44` still adjusts and resets, but
the proven `00437CA0` path never reads that word:
every original H5 descriptor retains colour-depth byte `+0C = 16`. The portable
RGB555 source policy therefore does not invent a colour-format switch for it.

Z Buffering `+50` is consumed by world draw `00488020`, which sets renderer
depth enable/write through virtual `+10`; it does not change frontend depth
state. The remaining gameplay settings retain their live words and persist on
Escape. The four refresh rows additionally call `0040F740`, which returns early
with no loaded world. A future world-attached host must implement its entity
pointer updates instead of inheriting that frontend early return. The parent
Options Defaults action `00463B70` now has its own owner (see
[Restore Defaults](#restore-defaults-00463b70)).

The [native callback fixture](../../crates/inf-game/tests/fixtures/video_menu_native.json)
contains 83 receipts: 16 device draws, six enumeration callbacks, 39 device
actions and 22 display adjusts. Its harness executes the original bytes of
`00462DE0`, `00462EE0`, `00463000` and `00462F40`, including the original
adjust/getter/string-copy callees. Text composition, OS enumeration/registry
calls and display reset are hooked boundaries. Injected reset failures restore
the captured setjmp caller state with error `0x987` and execute the original
catch bodies. Rust tests compare the resulting selections, names, publication,
persistence/reset order and rollback, then submit the resolved names through
the actual glyph composer. These receipts establish callback behavior; they
do not exercise DirectDraw, a native window or a retail framebuffer.

The explicit release [SDL tests](../../crates/inf-game/tests/frontend_app_window.rs)
also passed with the real window/audio backend. The hidden Video replay presented
460 frames across six resets: device confirmation, Resolution from 640x480 to
the next listed window size, back and forward again, Colour Depth adjustment
and linear→point filtering.
It retained one music start, the original screen age without duplicate ticks,
live cues and the saved settings; its PNG dimensions matched the final surface.
Relaunch without dimension arguments restored the saved size; an explicit
640x480 override then selected and persisted that tier. The existing Quit No/Yes
replay also passed after the display changes. Hidden fullscreen tests establish
flag changes only: SDL requires a shown, nonminimized window to activate an
exclusive mode.

The shown fullscreen integration now presents a frame at 800x600 through the
exclusive surface (the title's when the title video is installed), then
separately runs 360 frames through the authored Display→windowed action and
Escape persistence. The [direct display regression](../../crates/inf-game/src/frontend_app/display_tests.rs)
checks the shown flag, requested mode, actual active 800x600 mode and window
dimensions. An unreported 801x600 mode fails before touching the window, and
a controlled invalid `[0,600]` window exercises recovery to the old active
800x600 mode; switching off fullscreen then restores the original desktop
dimensions and refresh rate. Both hardware cases passed again on
2026-10-09 with 800x600 executed, from ``:

```sh
cargo test --release -p inf-game --lib visible_exclusive_mode_matches_requested_pixels_and_restores_desktop -- --ignored --nocapture --test-threads=1
cargo test --release -p inf-game --test frontend_app_window video_fullscreen_startup_can_return_to_windowed_through_the_authored_row -- --ignored --nocapture --test-threads=1
```

The hardware cases explicitly skip displays without the required 800x600 mode.
These portable checks cover a controlled invalid candidate, not OS/driver
failure during rollback, every monitor layout or matched retail frames.

### Sound menu

Options item 2 pushes authored menu `00538E68`: header `0045F520` (label
`00A2`, camera preset 5), Sound and Music slider rows, and Back. The slider
draw `00453D50` takes argument `(width << 16) | label` (`000F008C` and
`000F008E`) and the settings cell (`+00` or `+04`). A cell DWORD of exactly
zero draws `H0[label]` left and `H0[low16(argument + 1)]` right ("Off"), with
the value-row lookup rules above. Any other value, negative included, calls
`00453C90`: it builds the right text in a 64-byte stack buffer from the
arithmetic high word. Widths above one produce `1B`, `1C` fill and `1D`; other
widths produce a lone `1D`. A positive value then overwrites that many leading
bytes with `1E`, with no width check. Retail font 0 draws these bytes as the
empty and filled bar segments. A width whose terminator leaves the buffer, or a
fill that overwrites the terminator, is an explicit error instead of the native
stack overrun/overread. The label still uses the argument's low WORD.

Actions `00463AC0` (Sound) and `00463B00` (Music) forward command, maximum 15
and cell to `00453B80`, then run a fixed tail for **every** action command,
including ones the adjust ignores. The tails re-read settings `+00`/`+04`
through `00463AB0`, never the row's cell. See the [audio
consumers](AUDIO_RUNTIME.md#sound-menu-volume-consumers). The port resolves the
draw cell like value rows and routes the tails through one required
`apply_volume(VolumeChannel)` effect. Native receipts from the unmodified PE
callbacks pin 510 draw side selections (including `00453AE0` high-word bases)
and 300 adjust/tail argument sets. A hidden-window release smoke replayed the
retail keyboard path into Sound. It adjusted Sound 7→8 and Music 7→5, drew 8/15
and 5/15 bars, drew "Off" for Sound 0 and a full bar for Music 15, and
persisted both words on Back.

### Restore Defaults `00463B70`

Options row 6 binds action `00463B70`, which takes no command. The generic
dispatcher forwards every non-navigation command to the selected row, so
Left, Right and Return all restore the defaults. Offline, it saves `+64`
(language) and `+84` (cheat word), copies all 34 words from `00538CD0`
(`rep movs` at `00463C95`), then writes both saved words back. The
decompiler drops that save and restore. `0048C850` is an empty call. Then,
under setjmp, `004391C0` supplies the display owner and `00437DF0` reinitializes
the display at the default `+0C` tier, as `00462F40` does but without its
variant clamp. A caught reset failure writes the old `+0C`, captured at
`[ebp-4]` on entry, back over the default (`00463CF9`). The other defaults
stay. No volume tail runs, so the FX master and music keep their device
levels until their rows adjust them, and nothing is persisted until Back.
A network session copies only a fixed subset instead, which the offline
port never reaches. The port's copy keeps retail's words except Detail Level:
`+70` becomes High (1), the port's startup default, rather than the authored
Low.

`FrontendSettings::restore_startup_defaults` owns the word copy, and the
platform effect reuses the Video rows' reset host and rollback. A hidden
800x600 replay ends on a 640x480 surface (the default tier 2) while the
saved file still holds tier 3.

### Draw handshake, tick order and factory

`00484940` checks readiness before the controller draw. Successful drawing then
sets process flag `0055F14D` and clears screen acceleration `+11C`, even if a row
changed the target during that draw. `FrontendControl::draw_menu` preserves this boundary
and updates the live motion policy; constructor entry mode cannot remain its
source forever. Teardown `00483BD0` also sets the process flag after releasing
binding sets, then destroys the menu controller and base screen.
`FrontendProcessState` shares that live flag across frontend instances, including
changes from nested host callbacks; each control retains the same owner.

Tick order is animation, base input, music service, preset-9 timer, camera-gated
menu tick (or age reset), delayed settings save/pop, deferred profile menu,
attract decision, then camera integration. The delayed-save timer fires when
its **previous** value exceeds 1000 ms; an empty profile starts a 3000 ms pop
countdown that is decremented in that same tick. A single-menu stack accumulates
uncapped scheduler elapsed time and requests phase 9 strictly after 60,000 ms.
Other stack sizes reset idle time without clearing the process attract flag.
The production session retains this timer but defers unavailable demo playback
through the named [attract approximation](ATTRACT_DEMO_RUNTIME.md#attractdemodeferredapproximation)
before screen teardown, keeping the menu usable across repeated idle deadlines.

Factory `004838A0` has distinct construction and selection phases. Base
`00437F90` attaches the scheduler member, creates its input context, acquires analog
references `+CC` then `+D0` using descriptors `00546F10` and `00546F00`, then links
the distinct session-screen membership. Failed base initialization can therefore
leave a scheduler member without a session-screen entry.
After controller/camera initialization, required music operations surround
activation of resource leaf 47 (`00487230`, load, `00487370`/`004872A0`).
The factory then reads **H6 menu color resources** in lookup order **2,2,3**
and publishes selected-start/ordinary/selected-end colors **3,2,2** through
`00453E10`. These are RGB words, not font bindings.

`00439B30` enumerates eligible physical devices in native order, creating
digital then analog sets per device. Their persistent adapters and calibration
belong to the device owner. `00439D00` subsequently attaches all twelve digital
slots in ascending order, then all twelve analog slots. Only then does
`004947E0` bind/select the context. The factory pushes the root, persists
settings, resets animation, and calls `0043A370` last. Its final color has
RGB `(128,128,128)`; the fourth byte retains the high byte of the factory's raw
context argument. The port accepts that byte explicitly as provenance; it is
not established alpha.

Teardown `00483BD0` releases sets in interleaved device-slot order
`digital[i], analog[i]`, sets process returning flag `0055F14D`, clears menus,
unlinks session-screen membership, releases analog `+D0` then `+CC`, destroys
the input context, and finally removes the scheduler link. It does not unload
leaf 47 or free persistent device adapters; adapter cleanup `00439DA0` is
separate. Destroying an unselected screen must not call the generic deselection
helper, which would clear another screen's active bindings.

[`FrontendBindings`](../../crates/inf-game/src/frontend_bindings.rs) now owns
the context/base references and binding phases above, with explicit eligible
device input and required external calibration. It retains partial ownership
on errors for explicit teardown. `FrontendAssets` supplies its original input
tables and mandatory base descriptors even when no analog device is present.
[`FrontendScreen`](../../crates/inf-game/src/frontend_screen.rs) composes that
lifecycle with the real registry, resident loader, controller, and shared menu
catalog in native order. The controller is initially empty: creating the root
earlier would prematurely reset shared authored rows. H6 color resolution uses
the registration's local index in its owning overlay, and publishes live process
colors only after all three lookups. Required factory effects cover music,
eligible calibrated device enumeration, and settings persistence. Unsupported
effects fail explicitly. Errors retain completed acquisitions for ordered cleanup;
they do not restore audio, residency, colors, or invalidated queued input targets.
Partial-constructor cleanup is explicit Rust containment, not reconstructed SEH
or longjmp recovery. It does not publish the complete destructor's returning flag;
the native base/controller unwind helpers at `0051DE00/0051DE08` omit that write.

[`FrontendDispatch`](../../crates/inf-game/src/frontend_dispatch.rs) joins
digital callbacks and base analog/repeat ticking to the controller. Shared menu
lookup, original basis arithmetic, and held clearing execute concretely. Nested
dispatch uses the live `ScreenInput`, so Back can suppress later held slots in the
same pass. Wrong contexts/unknown callbacks reject before mutation. Audio,
specialized menus, settings, and phase effects remain required external owners.

## Outer frontend frame

[`frontend_frame`](../../crates/inf-game/src/frontend_frame.rs) composes
`00484940` through `FrontendScreen::draw_frame`. The screen supplies its actual
base-input age, camera, and shared animation state. The required host retains
the caller-context identity from session `+648`, uses its live fields, and owns
the surface, renderer, command arena, resource lazy loaders, and draw operations.
Rendering requests and recording fixtures alone do not establish pixels.

The frame consumes the shared `00439270` preparation
before capturing its caller context. `00495800` reads the surface's current
renderer. Its separate lookups remain observable throughout the frame:

| Phase | Native order and renderer ownership |
|---|---|
| Before the error scope: `00439270` | Lookup 1 replaces the command arena with `0xC8000` bytes through `004A43A0`; lookup 2 resets the batch through `004A4400`; lookup 3 publishes `00546E88`. Arena replacement frees the old allocation even when its size already matches. |
| Scope entry | Capture session `+648`, then install the native error frame. |
| Initial renderer setup | Capture renderer **B**, begin its batch, call its virtual `+14(0)`, then validate the surface through `00495770`. Surface recreation raises `0x711`; it is not successful validation. |
| Scene: `00483C60` | Capture fresh renderer **C**. Stars and models use C; after the final model, `004843B2` begins another batch on C. |
| Footer: `004843D0` | Use captured **B**. Resolve H6 slot 14 first, then read live surface dimensions and submit sprites. This phase neither begins nor flushes a batch. |
| Flush and menu | Lookup fresh renderer **D** and flush it through `004A4460`; only then evaluate the camera gate and draw the controller. Successful controller drawing sets `0055F14D` and clears screen `+11C`. |
| After the error scope | Check H4 slot 0, invoking its lazy loader if vacant. Then check H6 slot 3 only when the pool's inclusive bound permits it, loading it if vacant. These ensure resource availability; they do not restore font or palette render state. |

Only native error **`0x301` raised inside the guarded body** is consumed:
`004923C0` destroys that error, the remaining body is skipped, and both tail
checks still run. Other errors propagate before the tail. Errors from the
initial `00439270` or tail checks are outside the catch, including code `0x301`.
No catch rolls back already submitted geometry, view publication, row writes,
or resource effects. An interrupted menu creates no acknowledgement or returning
flag update. The narrow draw policy leaves it available for the next tick/frame;
ordinary failures stop the owning screen. Reentrant drawing and an independently
stopped controller cannot be revived by classifying a later error as `0x301`.

`FrontendScene::draw_scene` and `draw_footer` expose those separate native
phases; the frame owns the final live-renderer flush. Tests exercise distinct
renderer identities, resource-tail conditions, failures on both sides of the
catch, partial rows and clip state, and a retail-backed screen continuing after
`0x301` but stopping after `0x711`. Backend effects are explicitly recorded in
those tests, not treated as presentation acceptance.

### Frame reset and presentation tail

Scheduler `004943A0` calls the frontend's tick virtual `+04`, draw `+08`
(`00484940`), then presentation virtual `+0C` (`0040D1D0`). The latter calls
`00438120` with H7 material slot **32**. It selects packed black only when the
active gameplay object's `+5C4` flag requires it; otherwise it resolves the
actual material. `00495810` then calls `0050D330` to present **before**
`0050D960` clears the next color surface. Clearing black at every draw entry
would erase this resource choice and reverse the native presentation order.

Direct3D reset virtual `+14(0)` is `004BCA20` -> `0050DC60`: when surface
`+0E` is nonzero, it clears depth and writes device ZENABLE=0 and
ZWRITEENABLE=1. It leaves color and saved renderer `+38/+3C/+40` unchanged.
The post-presentation `0050D960` clears color and, under the same surface gate,
also clears depth and writes those device states. Its DirectDraw branch only
fills color. Virtual `+10`, `004BC9B0`, is the separate setter for device and
saved depth-enable/write values; gameplay draw `00488020` calls it with
`(0,0)` or `(1,1)`. Frontend draw does not reset these saved values each frame.

[`SoftwareScene`](../../crates/inf-game/src/software_scene.rs) exposes this
split as `begin_frame()` and `after_present(clear_rgba)`. Begin replaces the
command arena with `0xC8000` bytes and performs the depth-only reset. After
presentation clears the explicitly selected color. Lighting, published view,
texture custody, portable material identities and saved renderer policy survive
both operations. Two-frame tests check these persistent values, old-ambient
lighting, arena replacement, and the non-depth color-only branch. This concrete
background adapter uses one stable CPU renderer; it does not replace the full
outer frame's observable renderer lookups, surface validation or error scope.

### Resident presentation resources

[`RenderResources`](../../crates/inf-game/src/render_resources.rs) resolves
the current registration's owning overlay, region, and **local index**. Borrowed
bytes and provenance cannot survive a mutable residency transition. Pool identity
comes from `0053AA14`: H0 strings `0055EE50`, H3 sprites `0055EDA8`, H4 fonts
`0055EE20`, H6 style words `0055EEC8`, H7 materials `0055EE68`, and H8 models
`0055EEE0`. Lookups retain the full callback argument separately from its signed
low-word slot. Missing resources remain explicit errors; canonical decoding does
not perform lazy loading or backend allocation.

[`ResidentSceneHost`](../../crates/inf-game/src/scene_resources.rs) connects
those resources to a caller-selected backend. It resolves H7 star colors,
H8 model programs, and H3 footer sprites without substituting diagnostic meshes.
Both RGB555 halfwords of an H7 DWORD are converted using the backend's live
component shifts, wrapping addition, and low-word stores. Star command `+20`
is zero and `+24` holds that converted resource DWORD. Footer placement uses
the first physical sprite variant's unsigned dimensions; actual submission
selects the variant through the backend's live render mode.

H4 now has a canonical [font parser](../../crates/inf-formats/src/fonts.rs):
the original 64-byte headers, signed metrics, direct/position-grid sprite tables,
and source spans are preserved without native pointer fixups. The renderer must
still execute the resulting glyph and sprite submissions; see
[Text runtime](TEXT_RUNTIME.md).

## Scene and animation inputs

`00483C60` draws nine 24-byte descriptors at `0053BED8`, with count at
`0053BFB0` and position scale 10 at `0053BFB4`. Their model slots, in order,
are **744, 749, 754, 756, 745, 748, 747, 755, 757**. Each descriptor holds a
signed 16-bit model slot, padding, three signed 32-bit rotation controls,
three signed 16-bit positions, and trailing padding. The rotation controls at
`+4/+8/+12` feed `00491290`/`00491530`/`004917D0`: positive values are fixed
angles; negatives request the animation clock shifted by their negation
modulo 32. Positive controls run first in `+12/+8/+4` order, followed by
negative controls in `+12/+8/+4` order. Mixed controls therefore require two
passes rather than one axis loop.

The draw's packed-value callback is vtable `00528518` -> `00483C40`.
`004ED4B0` masks the callback operand with `0xFF3F`; selector zero returns
the low 16 bits of global animation clock `005471C0`, and all other selectors
return zero. This is a recovered callback policy, not a neutral-value guess.
The name-entry backdrop's objects use `0043A270` instead (see
[profile runtime](PROFILE_RUNTIME.md#name-entry-backdrop-0045a560)); the port
selects the policy per model submission.

`00439530` resets that clock and its 64-bit accumulator `00546F30/34`.
`00439560` adds its unsigned 32-bit argument to the accumulator with carry,
performs signed 64-bit division by 20,000, and stores the low 32-bit quotient.
Frontend tick `00484E00` passes its first, capped scheduler delta to this
routine. The factory resets the shared clock after root construction and successful
settings persistence. Existing frontend controls see that reset immediately;
a failed persistence operation leaves the previous clock intact.
The concrete model executor preserves unknown top-level value words, four-word
child inheritance, callback mutation order, saved bases, runtime vertices,
projection, and normal shades. It owns the 22 independent register words;
larger packed indices alias other context fields and remain explicitly
unsupported. Primitive clipping and backend commands are separate required
operations. See [Model runtime](MODEL_RUNTIME.md).

Camera presets at `0053C148` contain two signed 32-bit three-vectors each.
`00484AC0` scales them by the same factor before installing targets. Actual
camera movement is owned by spring helper `00484B60`. Menu drawing, ticking,
normal navigation (commands 0–3), and confirmation (command 4) are gated by
`00483750` until all three positional deltas are below 100. Commands 6/7
forwarded by `004852A0/004852C0` do not use that gate. A port must preserve
these distinct paths and the camera motion.

[`frontend_camera`](../../crates/inf-game/src/frontend_camera.rs) now owns
those springs and the animation clock. First entry begins at eye
`(-1200,0,7000)` and look-at `(2000,0,5000)`, multiplied by position scale;
returning entry starts at preset 0. Target changes retain velocity. Preset 9
restarts screen timer `+120`; other presets leave it alone. Eye/look speed
multipliers are 15/12 on first entry, 40/25 on return, and 200/140 after an
eligible acceleration request.

`00484B60` takes the integer square root of the scaled integer vector length,
then multiplies by speed to choose desired velocity. Acceleration and position
steps extract signed product bits 31..62 with `delta_micros << 11`; neither
saturating casts nor truncating signed division is equivalent. Overshoot uses
strict signed wrapping-absolute magnitude comparison and retains velocity on
snap. Normalization preserves X in a 53-bit x87 register but spills Y/Z,
denominator, and Y/Z scale to f32. `__ftol` truncates to signed 64-bit and the
caller retains the low 32 bits. These asymmetric spills affect integer results.
CRT `00513FE1` selects 53-bit precision. Kernel fixtures cover the arithmetic
and native table integration; matched retail frame/timing acceptance is open.

The surrounding scene includes a deterministic starfield with seed zero and
LCG multiplier/addend `0x13D56D/0x13D573`, colors 37 through 48, and footer
sprites in order 1807, 1806, 1809, 1808, 1810, 1811 (`004843D0`). The first model has special camera
position and backend state handling. Shared rendering must preserve those
submission policies rather than modifying intrinsic model decoding.

[`FrontendScene`](../../crates/inf-game/src/frontend_scene.rs) executes the
star generation/projection, original sine-table model transforms, per-model
camera publication, and six footer submissions with required renderer commands.
Stars consume the inherited view **before** this frame publishes a model camera.
The first model substitutes eye `(0,0,-70000)` while retaining the direction
computed from the screen camera, then flushes and resets the backend. Each model
submission carries the recovered animation callback policy. Resource resolution
through `ResidentSceneHost` is concrete. `SoftwareScene` passes that resident
model, original sine table, submission transform and live render environment to
[`ModelExecutor`](../../crates/inf-game/src/model_execution.rs). Its concrete
path publishes the view, applies outer admission and near/far selection,
preprocesses distinct world/projection ranges and shades, and executes the
frontend closure's value, branch, saved-basis, child, midpoint and interpolation
instructions. This preserves runtime geometry rather than substituting the
diagnostic viewer's mesh. Its concrete
[`ModelCommandBackend`](../../crates/inf-game/src/model_commands.rs) resolves
native primitive inputs and feeds the deferred queue. `SoftwareScene` owns the
live state, allocation identities and queue replay into CPU pixels. Stars use
the solid-quad callback; footer sprites call the effect-quad consumer directly,
as the original `+A4` call does. A resident integration test draws all nine
authored roots, stars and six footer sprites. Consumer arithmetic, supported
device policy and remaining pixel boundaries belong to
[Pixel runtime](PIXEL_RUNTIME.md#resident-scene-backend).

## Concrete frontend presentation

[`frontend_app`](../../crates/inf-game/src/frontend_app.rs) runs
[`SessionDriver`](../../crates/inf-game/src/session_driver.rs) and
[`PortSession`](../../crates/inf-game/src/session_platform.rs) in an SDL window
for `inf-game --mode frontend`. It boots the resident root, constructs the
title screen through the real factory (the absent video finishes to phase 2
on the first linked tick), and reaches the frontend through the real
Title-to-frontend transition effects and factory. The startup (language)
screen is reached from Options; the boot does not construct it (see the
[first-run boundary](#first-run-boundary)).
The scheduler owns input polling, transition-before-tick ordering, linked tick,
draw and presentation. Start Game opens the actual derived profile menu; its
empty-slot action reaches the phase-9 name-entry screen, and Back constructs a
returning phase-2 frontend. The registry retains its caller context and the
linked callbacks resolve the screen selected there.

[`frontend_renderer`](../../crates/inf-game/src/frontend_renderer.rs) connects
arena replacement, renderer publication, context capture, depth reset, SDL
surface validation, scene/footer submission, final flush, camera-gated menu,
and tail resource checks at their original boundaries. Arena exhaustion from
model execution retains its native `0x301` through the error wrappers. A caught
301 permits one presentation/clear of the partial surface, while further draws
stay stopped until a real batch/arena reset. Unsupported operations never become
301. Missing lazy resources and changed surface dimensions remain explicit
boundaries. The backend retains the previous published view for next-frame stars.
[`session_platform::software`](../../crates/inf-game/src/session_platform/software.rs)
routes linked `+08` to this outer frame or the complete name-entry outer frame.
The separate linked `+0C` callback resolves the current clear material,
presents through SDL, then clears the CPU surface. Draw/unlock never presents.

The renderer survives ordinary 2→9→2 transitions, retaining its identity,
published view/lighting, primitive state and saved rendering policy. A portable
residency revision changes only when an operation loads or releases resources;
an unchanged revision preserves the current resource borrows and texture cache.
After a residency change, the app retires those borrows and rebinds the same
renderer before the admitted iteration's draw, without polling or ticking
twice. Rebinding drops the old command arena, texture identities and uploads,
while retaining renderer state. A successful Video display reset instead
creates the new renderer/surface required by `00437DF0`. These lifetime rules
are portable custody, not extra native counters or screen transitions.

[`frontend_display`](../../crates/inf-game/src/frontend_display.rs) supplies
explicit **fresh-process** startup inputs. H2 slots 0/1, resolved through their
actual resident local indices, provide `00546F20/24` focal words. The display
selector maps 320x240, 512x384, 640x480, 800x600 and 1024x768 to OVL variants
0, 1, 2, 3 and 4 respectively. Startup uses the saved resolution unless
dimensions or a display-tier override are supplied; absent a sidecar, the
authored default is Full Screen at 640x480. Explicit dimensions and tier must
agree. Other [resolutions](PRESENTATION_POLICY.md#display-modes-and-resolutions)
keep these native dimensions and resource tiers, using the largest original
that fits, and enlarge only the completed frame through the shared session
presentation callback; native wide-camera projection remains unimplemented.
`00439880` supplies context fade words `0x100000`, and `004A49B0` supplies
near depth 64 and saved depth-enable/write values 1. Initial view/lighting
come from PE loader-zero storage, not a captured post-title phase-2 receipt.
The portable factory tint retains known RGB128 and an unknown high byte because
there is no native pointer receipt; that byte is never invented as alpha.

The helper chooses canonical RGB555 source, ARGB8888 uploads, Gouraud shading
and a depth surface. The app applies settings `+3C` to point/linear sampling,
repairs a zero renderer DWORD to 1 and requires native renderer selector 1.
Startup selects the last SDL output matching a nonempty saved `Current Display`
description, otherwise the first output. On it, the
[`00438AD0` search](PRESENTATION_POLICY.md#startup) finds the first display
the window takes, from the saved Display and resolution down, before the
residency follows its tier. The [Video menu](#video-menu) owns later SDL
display replacements. H7/32 supplies the initial/post-presentation clear color.
[Pixel runtime](PIXEL_RUNTIME.md) owns upload, blending, clipping and depth;
[Text runtime](TEXT_RUNTIME.md) owns actual root/generic/Quit glyph packets.
The original [music and cue consumers](AUDIO_RUNTIME.md) now feed SDL audio.

Arrow keys, Return and Escape flow through the physical SDL keyboard adapter,
original DIK bindings, `InputRuntime` FIFO and screen held/repeat owner. Escape
opens the native Back/Quit flow, or leaves name entry for phase 2. Root labels,
the derived profile menu, Sound/Video settings and both Quit answers execute;
unsupported specialized rows/actions fail at their actual callback. Tab runs
name completion through its native delay and persistence path, then the app
reports the explicit unimplemented phase-3 transition. Window close exits the
portable application. A fixed-step clock,
ordered physical-key replay and hidden PNG exports support reproducible tests;
they do not claim a retail input recording. The app retains the last presented
pixels before the post-presentation clear.

The root/Options window smokes rendered hundreds of consecutive frames with
original music and cues. Root and Quit labels draw from original resources at
all five display tiers; the explicit release SDL test covers Quit No returning
to root and Quit Yes exiting. The separate 600-frame background trajectory
continues to cover the near-plane model paths. Visual checks are portable
smokes, not matched retail-frame acceptance.

The [session window tests](../../crates/inf-game/tests/frontend_app_window.rs)
also use an isolated empty profile sidecar and physical-key replay at 125 ms
per admitted iteration. They exercise Start Game, empty-slot selection,
`Jameson` → `JamesoA` typing and Back to the frontend, plus Tab completion through
its delay to the structured phase-3 boundary. Each successful frame draws and
presents once with no unsupported pixel operations. The profile file stays
empty; only the completion run saves the edited identity. This is a portable
SDL execution check, not a recording of retail input or matched retail pixels.

## Portable settings custody

[`frontend_settings`](../../crates/inf-game/src/frontend_settings.rs) reads
the actual startup defaults: pointer `0052E0D8` calls `00463A80 -> 00463A90`,
which copies 34 DWORDs from `00538CD0` to `0054E8A8`. This is distinct from the
conditional reset helper `00463B70`. Native persist `00463100` walks 34
name/offset records at `00526C50..00526D60`, naming +10 twice with different
capitalization and omitting +74. The portable sidecar retains all 34 words.

Its child-key callback `00445570` stores 44 keyboard choices (names selected
through `00444E50` and `00535E54`) and 16 button actions (descriptor match in
`005361D0`). The sidecar follows the [phase-12 binding rules](#bindings): key
names reload as their first code, and an unbound button keeps its previous
value. The factory and Back command persist this state with
write/sync/replace.
A malformed existing sidecar errors rather than resetting it. Default storage is
`<data-root>/port-state/settings.json`; `--settings` selects another path.
The original `Infest.sav` and registry are untouched. FX/music words +00/+04
feed real audio; explicit display/language arguments override +0C/+64, and
the game boots with +64 as its language.
Persisted words whose consumers are unconnected remain raw, not guessed modes.
The Sound menu adjusts +00/+04 live through their native audio tails; Escape
persists them with the other words. Video's device action independently saves
the full `Current Display` description before attempting a reset, preserving
the last persisted values of other setting words. Name-entry completion's
`004633D0`/`00463490` write likewise updates only the selected player/net name
and initials; it preserves the other identity, display name and saved settings.
It does not persist a live cheat-word change at `+84`. A failed sidecar write
leaves the remembered identity unchanged.

## Remaining production boundary

The [model runtime](MODEL_RUNTIME.md) executes the nine-root, thirteen-model
frontend closure through concrete transform, vertex, projection, shade and
control owners. The [primitive and queue owners](PRIMITIVE_RUNTIME.md) now
implement 01/E2/1B rejection, E2 near-plane clipping, material resolution,
effect geometry and defined callback payloads. EE/EF enter and close native
deferred groups; queue flush preserves signed ordering and replay behavior.
The [pixel runtime](PIXEL_RUNTIME.md) executes near flat/textured triangles,
ordinary/effect quads, E2 clipped UV triangles/quads and solid star quads. The
generic menu, camera, control, outer frame, resident resource adapter,
[label layout](TEXT_RUNTIME.md), and base digital/untimed analog/repeat input
also have concrete owners, joined by the frontend factory and dispatch.

Every authored frontend path was replayed on 2026-10-08:

| Path | Result |
| --- | --- |
| Root Start Game, profile load/copy/delete, completed-slot refusal, empty slot | Owned |
| Root Options: Display, Sounds, Controls value rows, Back | Owned |
| Options Restore Defaults `00463B70` | Owned (above) |
| Options Network: name (phase 10) and initials (phase 11) | Owned through the text-entry screen |
| Profile load of a saved slot (phase 3 from the frontend) | Owned: `00487380`, then the saved world loads; the retail Terra Nova and Artoria records reach gameplay (see [profile runtime](PROFILE_RUNTIME.md#tick-actions-and-draw-handshake)) |
| Root Network `0045FD40` | Refused: `NetworkMenuRefusalApproximation` |
| Options Language (`00453C70(0)`, phase 0) | Owned: the [language carousel](#phase-0-startup-language-screen); its accept returns through the title |
| Controls row 3 (`00453C70(C)`, phase 12) | Owned: the [key configuration screen](#phase-12-key-configuration-screen) |
| Root Quit (Escape) | Owned |

Remaining work includes the first-run boot through phase 0, other
specialized settings/actions, required world pixel families, live player and
world simulation, and matched retail frames. The title backend and offline
New Game now reach the bounded [engine intro and mission briefing](INTRO_SEQUENCE.md);
complete gameplay remains open. The native 60-second idle request currently
uses [AttractDemoDeferredApproximation](ATTRACT_DEMO_RUNTIME.md#attractdemodeferredapproximation)
to skip unavailable demo playback while preserving the menu. Native demo
selection, loading and camera ownership have their separate source bounds in
[attract runtime](ATTRACT_DEMO_RUNTIME.md). The app does not claim a playable
mission or complete frontend acceptance. Continue from static consumers; use
existing TTD for execution/transient state that source cannot settle.
Independent/resident resource-inspection modes remain headless.

The diagnostic model interpreter still reconstructs a sine curve with a
1/32768 scale. It remains an approximate workbench path; production must use
the original table and native basis arithmetic above, not copy that helper.
