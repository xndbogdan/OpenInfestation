# Infestation input ownership

The input owner is recovered from `004EFD40` through `004F1080` in the
supported retail executable. Screen input dispatch/repeat is a separate owner
at `004392B0`/`00439430`/`00439470`. Original callback addresses identify port
callbacks; they are never invoked as host process addresses. These conclusions
come from decompiled source and the corresponding retail
disassembly; they do not establish platform key mappings.

## Logical consumers and bindings

Digital consumers are shared by action descriptor and callback-context identity.
Their `+0x18` word is the aggregate number of active binding records; `+0x1C`
is the published value. Consumer lists append in registration order. A digital
record combines two source groups; each group checks up to four required and
forbidden byte codes, with zero terminating each list. Device notification
updates all matching source-A groups before source-B groups. It emits an event
only when the aggregate crosses zero, so releasing one of two held bindings
does not emit a release. These boundaries contain no key-repeat generator.

Selection (`004F03B0`) temporarily clears the active context, zeros every
digital aggregate, evaluates the selected context's binding list from current
device state, publishes each aggregate as a boolean, then installs the active
context. Null selection clears digital held states silently. Neither path
flushes the queue, emits release callbacks, or resets duration accounting.
Queued callbacks retain their captured payload across selection changes.
Selection is **digital-only**: it does not recompute or clear analog aggregates,
published values, or per-binding previous values, including on null selection.

The screen registry's selected-screen identity and the input owner's active
binding context are distinct. Deselecting an unselected screen leaves the
selected-screen identity intact but still clears the active bindings. The
input owner must remain the sole authority for the active binding context.

## Authored frontend binding tables

[`inf-formats::input_tables`](../../crates/inf-formats/src/input_tables.rs)
decodes the two twelve-slot arrays supplied by `004838A0`. Digital roots are
slot 0 `005280B0`, slot 2 `00528218`, slots 6/7 `00528370`, and slots 8/9
`00528410`; all other slots are null. Analog roots are read through pointer
cells `00533398` for slot 2 and `005333E8` for slots 8/9. The supported file
contains two records per analog table. Table presence does not prove device
presence: `00439B30` constructs sets only for enumerated eligible devices.

Digital records are 20 bytes: descriptor pointer, required A/B code lists,
then forbidden A/B code lists. Analog records are the 24-byte layout below.
Both terminate on a zero descriptor **word**; there is no requirement for a
complete trailing record. Digital descriptors contain callback and flags;
analog descriptors add the raw microsecond period word. All identities and
opaque flags survive decoding. Source spans are executable **file** offsets,
including descriptors in a different section from their table. PE mapping
rejects section overflow, virtual overlaps, unmapped gaps, and virtual zero-fill.

Frontend base references use descriptors `00546F10` (`+CC`) and `00546F00`
(`+D0`), both callback zero, flags zero, period zero in the original file.
The word at `00546F1C` is outside the 12-byte descriptor. Digital bindings have
separate callback identities, including required system callbacks which the
screen held-mask dispatcher must not misclassify as screen actions.

[`FrontendBindings`](../../crates/inf-game/src/frontend_bindings.rs) owns
the frontend's context, base analog references, and instantiated sets. Its
metadata constructor is separate from `initialize` (context, `+CC`, `+D0`).
`install` accepts explicitly enumerated eligible devices with calibrated
logical adapters, creates digital then analog per device, and attaches all
digital slots before all analog slots. Digital source B is always absent at
`00439BFF`; its authored lists remain intact. Context selection belongs to the
registry and happens after installation. Table roots never imply a present
device, and duplicate native types are explicitly unsupported.

Teardown is staged around the frontend's other native effects: `release_sets`
destroys digital/analog sets interleaved by device slot, then the enclosing
screen publishes the returning-entry flag and clears menus before
`release_base` releases `+D0`, `+CC`, and context. Device adapters survive.
Failures retain completed acquisitions for explicit cleanup; final consumer
release can invalidate queued targets, so cleanup is not transactional rollback.

## Authored text-entry binding tables

The phase 9/10/11 text-entry constructor `00459060` builds its own twelve-slot
arrays (`00459093..0045914B`). `parse_text_entry_input_tables` resolves digital
slot 0 `00525C58`, slot 2 `00525D48`, slots 6/7 `00525E88` and slots 8/9
`00525F40`. The analog arrays read the same pointer cells as the frontend
(`004590D2`, `004590E0`). Keyboard slot 0 has eleven records: Esc Back
`004451E0`, Tab confirm `0045AE90`, Return (forbidden with Alt `38`) Activate
`00445200`, the four arrow slots `00445220/40/60/80`, R retry `0045AEA0`, then
the three shared Alt system records. Slot 2's table has no terminator of its
own: sixteen Activate buttons run on into the slots 6/7 table, giving 24
records and ending at `00525F28`. Its eight descriptors `00537C00..00537C38`
are distinct identities from the frontend's.

The separate direct-keyboard translator `00445840` does not use bindings. It
reads the 256-bit state of device-adapter slot 0 (`00439610(0)`,
`[adapter+4]+0x2C`) and, with process latch `0054766C` clear and any bit set
(`00445090`), lists the set scan codes in ascending order through `004F5230`.
It then copies the lowest code's name from table `00535E54` into its caller's
15-byte buffer (`strncpy`, then `buf[14] = 0`) and sets the latch. With the
latch set it reports nothing until every bit is clear, then clears the latch.
An idle call writes only `buf[0] = 0`. `004F5230` has no capacity check, so
five or more held keys overrun its 4-byte list and then clear with a negative
count, which faults natively. The name table has 0xDE entries (`00445680`
searches the same count); code `DE` reads a NULL word and faults, while codes
`DF..FF` read the adjacent `{name, descriptor}` pair table without faulting:
they copy an action name or descriptor bytes and set the latch. The canonical
[`key_names`](../../crates/inf-formats/src/key_names.rs) decoder owns the
0xDE authored names, and [`KeyTextLatch`](../../crates/inf-game/src/key_text.rs)
implements the translator with explicit errors for both native faults and a
separate port boundary for codes `DF..FF`, which the SDL adapter never
produces. Unicorn
receipts from 310 unmodified `00445840` calls pin the latch, ascending order,
padding and idle writes. The tick's use of the result is in
[profile runtime](PROFILE_RUNTIME.md#text-entry-tick-0045a8f0).

## Authored title binding tables

The phase-1 title constructor `00404620` builds its twelve-slot arrays inline
as a `0x60`-byte stack struct: digital roots at `+0x00` (slot 0 `0051F300`,
slots 6/7 `0051F378`, slots 8/9 `0051F4A8`, the rest null) and twelve zero
analog cells at `+0x30`, so the title has no analog input.
`parse_title_input_tables` resolves those roots: 5 keyboard records, 14 + 14
gamepad records. Every non-system record drives the skip callback `00404870`
(descriptor `0052E638`, flags 2): on a press (`EDX != 0`) while screen `+EC`
is clear it finishes through `004049C0`; releases and post-finish presses
return. Slot 0 binds skip to Esc, Space and Return with no Alt guard, then
the shared Alt+Enter (`00437D30`, descriptor `00533410`) and Alt+Esc
(`00439260`, descriptor `00533408`) system records also used by the frontend.
Held-bit dispatch runs base `00439470` over empty `00401310` virtuals, so it
is a no-op for this screen.

## Analog consumers and device notifications

`004EFE90` shares analog consumers by descriptor and callback-context identity.
The `0x28`-byte consumer stores descriptor/context at `+0x0C/+0x10`, reference
count at `+0x14`, aggregate at `+0x18`, published value at `+0x1C`, timer handle
at `+0x20`, and emission gate at `+0x24`. New consumers have zero aggregate and
published value and an open gate. Descriptor `+8` is the timer period; a
nonzero value constructs a timer through `004A48C0`. Its `timeSetEvent`
callback, `004A4870`, clears the timer's event `+10` when its `+8` is zero (a
one-shot that has fired) and calls the timer's `+0` function; `004A4920`
kills a live event and frees the timer (from `004F0129`, `004F0BBE` and
`004F543E`). The clock's table `00528F88` starts with `004A47D0`, which reads
`timeGetDevCaps` and calls `timeBeginPeriod` with the finest period, at
least 1 ms (kept at `0055F8FC`); without the caps it returns error
`00528F80`.

An analog binding (`004F0A40`) contains 24-byte authored records and an
8-byte runtime cell per record: consumer reference plus previous transformed
value, initially zero. Record fields are descriptor, axis index, flags, signed
bias, signed Q31 multiplier, and signed integer scale, at offsets
`+0/+4/+8/+0x0C/+0x10/+0x14`. A null descriptor terminates the table.

`004F0F80` runs digital bindings first, transforms eight device axes, then runs
analog bindings through `004F0C00`. Both stages share the **same** event packet
and lazy timestamp. The analog pass re-reads the active context after the
digital pass. Each device axis and each binding record apply this transform:

```text
word = wrap_i32(wrap_i32(input + bias) * integer_scale)
value = low_i32((i64(word) * i64(q31_multiplier)) >> 31)
```

The first stage uses device triples at `+8/+0x0C/+0x10`, stride 12, on the
eight raw axis words beginning at notification `+0x20`. The second stage uses
the selected transformed axis and the binding record's fields. Retail uses
signed `IMUL`, then `SHL EAX`/`RCL EDX` to retain product bits 31–62. Ordinary
division truncating toward zero, saturation, or collapsing the two stages
changes negative values and overflow behavior.

When a record's value changes, `004F0C00` reads **mask `0x01` from the first
record's flags for every record**, proven at `004F0C66..004F0C6F`. A clear bit
replaces the consumer aggregate; a set bit adds the wrapping difference from
that record's previous value. It then stores the new per-record value. An
open emission gate submits the resulting aggregate to the common queue;
there is no additional aggregate-versus-published equality check. Analog
events carry no digital duration pointer. Nonzero periods additionally close
the gate and control the timer around emission (`004A4990`/`004A4940`).

No retail analog consumer has a period: the frontend's tables hold only
`00546F10` and `00546F00` (period zero in the file), and gameplay's
`00469D60` acquires `0054ECE8`, `0054ECF8` and `0054ED08`, BSS that no
instruction writes. So `004A48C0` builds only the joystick's 20 ms poll
timer, and the timed emission `004EFE00` with its gate `004A4990` and
`004A4940` never runs; the progress report omits those three as dead.

The implemented analog owner supports **period zero only**. It preserves both
transform stages, shared consumers, binding cells, queue behavior, and the
first-record flag policy. Nonzero periods fail explicitly with
`UnsupportedAnalogTimer`; timer delivery/rearming is not simulated.

## Queue and callback order

`004F0150` owns a 32-slot ring with one reserved slot: at most 31 queued
events. Full rings discard the new event. A queued event stores a target-state
pointer, callback address/context, raw value, 64-bit timestamp, and optional
duration-state pointer. Callback ABI is ECX=context, EDX=value, with timestamp
low/high on the stack; the decompiler omits the first two arguments.

One event packet is shared across each physical-device notification. Flag 2
samples its timestamp lazily once, even when the ring will subsequently reject
the event as full. Events without that flag copy the packet's current time,
including zero before any timestamped event. Flag 1 writes the target and
calls the callback immediately, bypassing both the ring and duration updates.
Flag 4 supplies duration metadata on the digital edge path.

Poll (`004F0200`) performs these operations in order:

1. Reset each consumer's duration baseline to the previous completed poll
   time and its accumulated duration to zero.
2. Read one ring entry and cache the next read position. Update its published
   value and duration before invoking its captured callback.
3. Advance the read cursor **after** the callback. Re-read the write cursor,
   allowing events appended by the callback to run during the same poll.
4. Sample the clock only after the queue has drained.

Consequently, a callback sees the previous poll clock. The event being handled
still occupies a ring slot during its callback, and a full ring cannot accept
another event at that point. Replacing this loop with a snapshot or popping
before the callback changes the behavior. Callback changes to the read cursor
are overwritten by the cached next position on return.

`004F0340` resets the same timing baselines and copies write to read to discard
queued input. It does not sample the clock or change published held values.
The session transition calls this before publishing the new phase.

## Held duration and destruction

Consumer `+0x20/+0x24` is a 64-bit baseline timestamp; `+0x28/+0x2C` is a
64-bit accumulated held duration. Arithmetic wraps at 64 bits. A down edge
sets the baseline to the event timestamp. An up edge adds event time minus
baseline. Query `004F0040`, while held, adds completed-poll time minus baseline
and advances that baseline; repeated queries do not double-count. Its return
is the low 32 bits of the accumulated microseconds.

Native clock `004A4890` sign-extends `timeGetTime` and multiplies by 1,000.
Portable clocks must specify their microsecond domain explicitly; raw input
timestamps are not coordinates or scheduler frame deltas.

Final consumer release (`004EFFC0`, analog counterpart `004F00C0`), including
the final-reference branch of binding destruction (`004F05C0`), clears
**every pending target pointer**. Retail
disassembly confirms an unconditional store, not a comparison against the
consumer being released. Callback address/context, value, timestamp, and
duration pointer remain. Poll skips both target and duration work when the
target is null, but still invokes the callback. Removing those callbacks or
invalidating only events for the released consumer would differ from retail.
The branch at `004F0651` skips invalidation when another reference survives.

Input-context destruction (`004F0D60`) clears active digital state when that
context is selected, unlinks it, and frees its registration links. Binding sets
have independent lifetimes and can appear in multiple contexts. Their own
destructor (`004F05C0`) detaches a set from all contexts and releases its consumer
references. Context destruction alone does not destroy the linked sets. Screen
destruction releases its input references and embedded context before removing
the scheduler attachment. Unsupported recursive dispatch and destructive
mutation during immediate device traversal must produce explicit Rust errors;
the original raw-pointer paths are not a safe ownership model to reproduce.

## Screen held input and repeat

[`ScreenInput`](../../crates/inf-game/src/screen_input.rs) owns screen age
`+0xC0`, held mask `+0xC4`, repeat countdown `+0xC8`, and analog latch `+0xD4`,
all initialized to zero by `00437F90`. The references at `+0xCC/+0xD0` point
to analog consumers; `004F0140` returns their signed published `+0x1C` words.
The screen owner consumes those values without inventing an OS axis mapping.

The direct digital callbacks ignore timestamps, press on any nonzero value,
and clear their one mask bit on zero:

| Callback | Held bit | Frontend dispatch |
|---|---|---|
| `00445200` | `0x01` | Confirm, virtual `+0x24` |
| `00445220` | `0x02` | Previous, virtual `+0x28` |
| `00445240` | `0x04` | Next, virtual `+0x2C` |
| `00445260` | `0x08` | Adjust previous, virtual `+0x30` |
| `00445280` | `0x10` | Adjust next, virtual `+0x34` |
| `004451E0` | `0x20` | Back, virtual `+0x38` |
| `00485300` | `0x40` | Auxiliary command 6 |
| `0042AA00` | `0x80` | Auxiliary command 7 |
| `0042A9D0` | `0x100` | Arm attract timer |
| `0042AA30` | `0x200` | No branch in the recovered base/frontend policies |

`00439430` publishes a newly pressed bit, dispatches **all currently held
bits**, then writes a 250 ms repeat countdown after callbacks return. An
already-held press does not dispatch or reset that countdown. `00439470`
checks the six base bits in table order; frontend `00484A80` then checks
`0x40/0x80/0x100`. Every check reads the live mask. A callback can clear held
input and suppress later actions, while the new-press countdown write still
follows it. Digital release and frontend Back's whole-mask clear leave the
repeat countdown and analog latch unchanged. The context-resolving wrappers
`00443490..00443590` are not direct-screen callbacks.

`004392B0` first adds raw elapsed microseconds divided by 1,000 to age. Analog
priority is `+0xD0 > 2500`, `+0xD0 < -2500`, `+0xCC < -2500`, then
`+0xCC > 2500`, selecting bits `0x02/0x04/0x08/0x10`. Only one branch runs.
It presses the chosen bit **before** setting the analog latch and clearing
the opposite bit, so a reversal can dispatch both directions. Neutral input
clears all four direction bits only when the latch was set; this can also
clear digitally held directions because the screen has no separate source mask.

Finally, capped delta microseconds divided by 1,000 advances repeat. If delta
is below the old countdown, it subtracts delta and returns. Otherwise it
dispatches once, first setting countdown to `old - delta + 100` when any bit
is held, or zero when none is held. Negative remainder survives; there is no
catch-up loop. Age/countdown use signed truncation per call and wrapping
32-bit arithmetic. Dispatch failures stop the Rust owner after retaining
completed effects; they do not replay callbacks or claim native error recovery.

The [fresh results Tick](RESULTS_RUNTIME.md#implemented-fresh-first-tick)
reuses this same C0 and analog/repeat state. Narrow crate-private phases
allow its raw-age store before required live consumer reads, then the later
derived capped-age addition after base dispatch. The public base Tick keeps
its existing ordering; results does not maintain a second age counter.

## The joystick, the mouse and the input mode

`004F55A0` creates a native type-2 device for each WinMM joystick present at
startup (indices 0 and 1, `0065EE14`/`0065EE18`), with `joyGetDevCapsA`'s
button and axis counts, and a 20 ms timer (`004A48C0`). Each tick
`004F54A0` reads `joyGetPosEx` (`JOY_RETURNALL | JOY_RETURNCENTERED`): X,
Y, Z, R, U and V (0 to 65535) to `+4C..+60`, the buttons doubled to `+2C`
(button 1 is bit 1); a failed read sets `+2C` to bit 31 alone and leaves
the axes. `004F5300` creates the mouse (type 1, DirectInput, buttons in bits
1 to 4, X/Y/Z movement added up), `004F4880` a type-11 button device (26
buttons, no axes).

A device reaches bindings only through an adapter (`004F0EE0`), which
`00439B30` creates in the process cache `005474B0` for a type the screen's
sets have a table for, from the first device of that type. No screen has a
type-1 or type-11 table: in the lab's retail Artoria run the mouse exists
(`0065EE10`, type 1, 3 buttons, 3 axes, `+10`/`+14` zero) and `005474B4`
stays null, so the mouse and the button device have no effect. Every screen
but the title binds type 2; a second joystick's sets are made over the
first's adapter, so only the first drives input.

`00439AD0` calibrates each axis of an adapter's device: the bias of its
type from `00533764` (type 2 `−0x8000`, types 8 and 9 `−0x80`, the rest 0),
the multiplier at `005338F0`, scale 2. `00438850`, on each screen
selection, sets the multiplier from Sensitivity (settings `+54`):
`(+54 << 27) + 0x7FFFFFF`, then runs `00439AA0` over every adapter.

The joystick's tables: in the menus, slot 2's sixteen button records
accept, and the analog cell `00533398` (table `00533350`) sends X to the
screen's `+CC` and Y to `+D0` (Q31 `0x10000000`, scales 1 and −1). In play
the `+0xFC` sub-object binds the sixteen buttons of `00534CC0` (the key
configuration's Joystick 1 to 16) and the analog table `005231D8`: X to the
player's `+5C` at 4, Y to `+60` and Z to `+64` at −4, all at Q31 one half.

Settings `+8` is the Controls menu's Joystick row (`00539188`: Disabled,
Analogue, Digital; registry "JoyStick Enabled"). The gameplay tick
`00442B50` turns it to Disabled while no type 1, 2, 8 or 9 adapter is live,
and Disabled to Analogue while one is, so with a joystick Disabled never
holds ([tick detail](GAMEPLAY_RUNTIME.md#screen-tick-detail)). Only the
tick and `0046A260` read it. `0046A260`, after the keys, reads the three
analog references (`004F0140`, the published dword): Digital holds pitch
`+18` and turn `+14` at the uncapped delta against the sign of Y and X once
either passes `0x8000`; Analogue sets `+4` to 1, `+8` to X and `+C` to Y
(both clamped to ±`0x10000`) and `+10` to Z unclamped. The vehicles read
those when `+4` is set
([the control block](VEHICLE_DRIVE_RUNTIME.md#the-control-block-seen-from-the-vehicle)).

The port ([`platform_input`](../../crates/inf-game/src/platform_input.rs),
[`platform_pad`](../../crates/inf-game/src/platform_pad.rs)) opens the
first SDL game controller, or else the first joystick, and reads it once a
frame instead of on the 20 ms timer. A mapped controller is numbered as
Windows' legacy joystick API numbers an XInput pad, retail's own view of
it: the left stick on X and Y, the triggers together on Z, the right
stick's vertical on R and horizontal on U, and A, B, X, Y, the shoulders,
Back, Start and the stick clicks as buttons 1 to 10; the d-pad is the
point-of-view hat, which retail never reads. Every enumeration calibrates
the joystick with the live Sensitivity. A session log records each change
of the pad's reading, and its replay repeats the last one each frame; a
hidden run reads no pad. The mouse and the button device are not created.

### The PlayStation release

The PAL PlayStation release (SLES-01870, `GAME.EXE`) binds the same tables
(`psx_controls.py`). All 20 of its pad tables match
PC tables row for row (the PC keeps them in every screen's slots 6/7 and
8/9, unused), under one PC-to-PS1 descriptor pairing apart from the
frontend's `00528370`, whose same rows (Cross or Start accept, Triangle
back, the d-pad moves) share a PS1 descriptor with more screens. Its
gameplay pad analog table `80012DD8` is the PC's `00523238`. Its driver
(`800F3E38`, two ports of four multitap slots, polled every 8 ms by
`800F3AF8`) creates types 6 and 7 (digital, no axes) and 8 and 9 (analog,
four axes; 7 and 9 answer the extended-mode query) from `8012D068`. The
readers store the buttons as `~(raw << 1) & 0x1FFFE` (`800F43D4`,
`800F4310`), so codes 1 to 16 are Select, L3, R3, Start, Up, Right, Down,
Left, L2, R2, L1, R1, Triangle, Circle, Cross and Square, and the analog
reader stores the left stick's X and Y and the right stick's X and Y in
axes 0 to 3, 0 to 255; `00533764` biases them by `−0x80`. In play the
left stick steers and pitches and the right stick's vertical (axis 3)
thrusts.

That differs from PC retail's joystick (sixteen numbered buttons the key
configuration assigns, X, Y and Z), so the port offers it as a layout
(`ControllerLayout`, the launcher's Controller layout, `--controller-layout
console`): a mapped controller then plays as a DualShock in analog mode,
native type 9, its positional buttons standing for the PlayStation's (Cross
the bottom face button, Circle the right, Square the left, Triangle the
top) and its triggers, past half, for L2 and R2. An unmapped joystick stays
the PC joystick. Type 9 binds the status input, so an unplugged pad stops
counting for `00439A20` and the input mode drops to Disabled. The console's
vibration and its controller-removal pause are not ported: the PC has no
vibration code, and its keyboard keeps a live adapter.

## Scope and integration

[`input`](../../crates/inf-game/src/input.rs) owns digital and untimed analog
consumers, the common queue, timing, and bindings. The analog implementation is
in [`input_analog`](../../crates/inf-game/src/input_analog.rs). The
[player constructor](PLAYER_CONSTRUCTION.md#lazy-input-reference-sequence)
consumes this same runtime for its ordered references before message/grants
and FIFO publication; it creates no context, bindings or device. Its retained
[current descriptor source](PLAYER_CONSTRUCTION.md#lazy-input-reference-sequence)
seeds proven loader-zero analog records once and parses digital records lazily.
Published player destruction unlinks the actual FIFO player and destroys its
constructor-known empty message before releasing those same input references.
[`screen_registry`](../../crates/inf-game/src/screen_registry.rs) owns selection
identity and scheduler registration; `ScreenInput` supplies concrete screen
held/repeat state, and `FrontendBindings` owns the authored binding lifecycle.
An integration fixture follows original tables through registration, queued
analog publication, screen repeat, live Back clearing, and staged teardown.
[`FrontendScreen`](../../crates/inf-game/src/frontend_screen.rs) now composes
the frontend factory, selection, and destruction phases. Its
[`FrontendDispatch`](../../crates/inf-game/src/frontend_dispatch.rs) executes
base analog/repeat and direct digital actions against the live menu controller,
shared authored catalog and native basis math; Back clears the live held mask
before subsequent slots.

[`platform_input`](../../crates/inf-game/src/platform_input.rs) now maps SDL
physical scancodes to native DIK button offsets. `004F1080 -> 004F50F0`
constructs type0 with256 buttons, zero axes and eligibility fields+10/+14 zero;
`004F1100` packs offsets1..255 directly. Zero axes mean no calibration
iterations. Return/keypad Enter, cursor/keypad keys and left/right modifiers
remain distinct. Each edge enters `update_device` in order; duplicate downs
are harmless and focus loss publishes releases. There is no direct key-to-menu
shortcut. Original-table tests cover all17 keyboard records, shared Space/Return
held counts, Alt chords, native250/100ms repeats, failures and teardown.

The SDL frontend passes clock samples, polls the FIFO before its screen tick,
and routes direct screen callbacks through `FrontendDispatch`. The pad joins
it as the native joystick
([above](#the-joystick-the-mouse-and-the-input-mode)). Timed analog workers,
text entry, session/device callbacks and specialized actions remain separate
open owners.
