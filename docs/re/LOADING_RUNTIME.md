# Infestation loading callback and presentation

This document owns the native process loading state, `0040EDB0` callback,
`0040E6B0` visual/counter policy, `0040ED50` presentation and their file-read
caller. The [world-load lifetime](WORLD_LOAD_LIFETIME.md) owns persistent
players, inputs and arena custody; [attract demo](ATTRACT_DEMO_RUNTIME.md)
owns the separate preparation, placement and transition boundaries. These are
source contracts and bounded ordinary loading implementation; they do not establish retail acceptance.

## Source and lifetime

The supported `Infest.exe` is 1,445,888 bytes, SHA-256
`bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956`.
The original game logic,
OVL handlers and
DirectDraw wrappers describe the callers; original
PE instructions decide register inputs, field widths, signed comparisons and
read/write order where the decompiler loses them.

| Complete body, end exclusive | Bytes | SHA-256 |
|---|---:|---|
| `0040E6B0..0040ED4D` | 1693 | `a9e087ff6e4f2d39b0f752e66442521abd8168ff9f997f44a6de544492cc8a89` |
| `0040ED50..0040EDA1` | 81 | `7537aaeed64462acc4f4175cb2ecfccef0b1812fc3e2d04d340f7baa3eb99aa2` |
| `0040EDB0..0040EF23` | 371 | `86d31dd3987388e0d7188ec223b27e727a6e7c271ab2818122de55fcd949ca8c` |
| `004F3DE0..004F405E` | 638 | `4375084d6c88fb9e078ae11eb5597f82d391983fa18648fce2161c0635d1bcee` |
| `004F40A0..004F40AC` | 12 | `9da62d61f51f350cddce7eeb3e0e93ef5e5fc356e472b8072bbf0ba8b46350ef` |

There is no loading-screen allocation or constructor in the reached
`0048DC30` prefix. Its `00487230` call suspends optional game music: it reads
game `+5D0`, returns on NULL, otherwise calls `004F7500` and `004923C0`.
That complete 23-byte body (`00487230..00487247`) has SHA-256
`9dc809f0141c895f79e3552a52956015441996c80395dac3ed127845a06cc829`.
The `0040D6A0` call inside ordinary loading constructs a temporary identity
basis, not a process loading screen.

The PE `.data` section has VA `0052E000`, raw size `00019000` and virtual
size `00132C38`. Its file-backed end is `00547000`; its virtual end is
`00660C38`. `[00546918,00546954)` is **60 authored zero bytes** at raw file
offset `00146918`, SHA-256
`5dcc1b5872dd9ff1c234501f1fefda01f664164e1583c3e1bb3dbea47588ab31`.
Thus the following initial values have an actual executable source. They are
retained process state, not a per-world reset or heap-default inference.

| Address | Reached field policy |
|---|---|
| `00546918/1C/20` | Three full-DWORD attract wait counters; zero is inactive |
| `00546928/2C` | Callback's previous clock low/high DWORDs |
| `00546930` | Wrapping raw callback progress accumulator |
| `00546938..47` | Sixteen shared effect BYTEs, read signed by the visual policy |
| `00546948` | Wrapping loading age DWORD |
| `0054694C` | Current visual mode BYTE: zero versus nonzero |
| `0054694D` | Pulse direction BYTE: zero versus nonzero |
| `0054694E` | Selector WORD chosen from masked RNG modulo 5 |
| `00546950` | Pulse LOWWORD; callback preserves the upper WORD |

The same span also initializes `00546924/34` and the upper pulse WORD;
initial zero alone does not assign these unused fields behavioral meanings.
The read chunk DWORD `00544CC8` is separately authored `8000`.
Callback pointer `0065ECA0`, read clock `0065ECA8/AC` and pending progress
`0065ECB0` lie in the loader-zero virtual tail. They likewise persist across
reads and callback replacement. The literal-reference census below does not
exclude arbitrary future computed aliases or native callback mutation.

## Mode and shared effect writers

| Actual caller | Current write and gate |
|---|---|
| Phase-8 results constructor `0042AB00` | `0042AC48` writes BYTE 0 to `0054694C` |
| Profile empty-slot activation `004486E0` | `0044871E` writes BYTE 1 on its non-owner, no-pending path |
| Profile command `00448800` | `00448922` writes BYTE 1 on eligible non-owner Load activation |
| Network action `0045F320` | `0045F369` writes BYTE 1 after its early existing-connection return |
| Network command `0045F740` | `0045F935` writes BYTE 1 on its reached successful connection branch |

The profile and results gates are owned by
[profile runtime](PROFILE_RUNTIME.md#tick-actions-and-draw-handshake) and
[results runtime](RESULTS_RUNTIME.md#phase-8-results-screen).
`0045F320/0045F740` are actual instruction entries omitted by the decompiler's
function inventory; nearby `0045E910/0045F650` are unrelated routines.
No reached loading constructor resets this mode. Nonzero is the native test;
it is not an equality-to-1 behavioral encoding.

Loading's indexed writes at `0040EB1F/3F/44` and name entry's
`0045AA74/96/9E` address the same `00546938` effect storage. The latter's
RNG gate uses low bits `31`, while loading uses `15`. They share stable
storage and the process RNG, not one interchangeable update policy. The
[text-entry process state](../../crates/inf-game/src/text_entry_screen.rs)
already represents the effect array. It must not be copied or reset when a
loading owner is attached. The separate key latch keeps its own input lifetime.

## Loader and callback order

The exact `0048DC84..0048DD16` loader span is 146 bytes, SHA-256
`42309df80d87d29e53707888b8bf28b70664f1134d974fc17a1fdf099b3f5ab6`.
After music suspension it writes `00546920=1` at `0048DC99`, installs
`0040EDB0` through `004F40A0`, activates leaf 48, then activates fresh mission
`+A0 + 10`. Only afterward does it read current attract BYTE `0055F14C`.
Phase IDs, mission selections and OVL leaf IDs remain distinct.

| Fresh attract branch | Reached policy before H14 world-data acquisition |
|---|---|
| Zero | Jump directly to normal callback removal; **does not clear `00546920`** |
| Nonzero | While `00546920` is nonzero, invoke callback with raw progress 0 and stack argument 0; then set `0054691C=1` and use a do-while callback; then set `00546918=1` and use another do-while callback |

Normal removal at `0048DD11` exchanges in literal NULL; it does not restore
the previously returned callback pointer. `004F40A0` only returns the old
pointer and stores the new one, without resetting time or progress. Exceptional
exchange unwinding has not been established. Attract loops require the actual
Tick writers; guessed zero handshakes do not establish their completion.

`0040EDB0` captures incoming EDX as raw progress; incoming ECX is unused.
Its one stack DWORD is forwarded to `0040E6B0`. The file-read caller supplies
1 and the attract waits supply 0. The callback always returns EAX 0, `RET 4`.

1. `004A4890` samples `timeGetTime`, sign-extends the DWORD and multiplies by
   1000 into a 64-bit clock. Wrapping low-DWORD subtraction against
   `00546928` is interpreted signed, capped at 125000, then floored at 7800.
   This is a callback clock, not scheduler admission or session elapsed time.
2. Store sampled low/high clock at `0040EDF6/0040EE05`; add raw progress to
   `00546930`, then clamped delta to age `00546948`, both wrapping DWORDs.
3. Nonzero direction uses delta arithmetic-shifted by 4: pulse strictly below
   `65535-step` adds the LOWWORD, otherwise stores `FFFF` then direction 0.
   Zero direction uses delta shifted by 3: pulse strictly greater than step
   subtracts; otherwise store pulse 0, call real `0048F060`, take masked
   LOWWORD modulo 5, store direction 1 then selector WORD. The upper pulse
   WORD survives. These writes and RNG occur even with no renderer.
4. Enter the custom error guard, then obtain current renderer through
   `004391C0` and read its `+4`. NULL skips both Tick and Draw. NonNULL obtains
   fresh session `004391B0` and its actual `+648` render context; Tick receives
   ECX=context, EDX=`00546938`, stack(clamped delta, original stack argument).
5. Call `0040ED50`, which reacquires the renderer independently. Normal and
   guarded-error exits preserve completed prefixes. The latter destroys the
   error through `004923C0`, without retry or rollback. No input poll or
   Win32 message pump occurs in this callback body.

## Tick visual and counter matrix

`0040E6B0` captures current renderer, derives signed delta `SAR 13`, reads
width then height, prepares renderer/context through `00495800`, clears the
primitive queue (`004A4400`), and invokes context virtual `+14(0)` before
the current attract/mode tests. Both native exits use `RET 8`.

| Attract BYTE | Mode BYTE | Drawing | Wait-counter mutation |
|---|---|---|---|
| Nonzero | Zero | `0040E140` fading four-sprite phase, then `00495770/004A4460` | Yes, after the draw phase |
| Nonzero | Nonzero | Skip that draw phase | Yes |
| Zero | Zero | Ordinary model background, common `00495770/004A4460`, title | None |
| Zero | Nonzero | Skip model background, common flush, sixteen equalizer quads, title | None |

The attract counter phase uses signed truncation of delta/1000. Each nonzero
full DWORD receives wrapping addition in order `00546920`, `0054691C`,
`00546918`. A subsequent signed strict `>2000` test clears expired counters
in that same order. Zero remains inactive. Ordinary loading does not consume
these counters; forcing it to do so would change the native matrix.

`0040E140` uses H6 word 14 and H3 sprites 8 through 11. Its fade starts at
256, uses active counter `00546920`'s shifted signed /2000 ramp, is overridden
to 256 by counter `0054691C`, then by the `(2000-counter18)` shifted /2000
ramp when `00546918` is active. DWORD wrapping precedes signed truncation;
the result is clamped to 0..256 and scales the three color BYTEs by /256.
Actual sprite availability and packet/material policy remain required inputs;
a missing first sprite returns before later quads.

The ordinary mode-zero background uses temporary basis `0040D6A0`, current
context/template work `0043B5D0/0043B4A0/0043A150`, age and selector/pulse
fields, shared transform scratch and actual model slot 117 (`75h`). It applies
`0040D1B0`, yaw `00491530`, fade `0048FB00`, current view fields and
`0049FEA0` model submission. Name entry's separate model composition is not
a substitute for this path.

Ordinary nonzero mode rereads current H3 sprite 7 and H6 word 1 per bar.
Dimensions and signed effect BYTE determine packed WORD quad coordinates;
the submission uses depth/attribute value 64. **Submit each quad before**
calling RNG and mutating its effect BYTE. After RNG LOWWORD `&15 == 0`
stores `7F`, the fresh signed BYTE is compared to retained delta `SAR 13`.
If greater, subtract the LOWBYTE of signed truncation of their wrapping
product /32; otherwise store zero. Sixteen ordered iterations retain any
already reached submission/RNG/store prefix on a later required-source stop.

All ordinary modes then resolve H4 font 0 and H6 word 4. Mode zero uses
optional H0 string 354 (`162h`) at height/2; nonzero mode uses H0 string 283
(`11Bh`) at height*33/100. NULL text skips glyph submission. Signed-WORD
text width from `004A4240` centers the text before `004A3AB0`. Only a signed
second Tick argument `>=3` requests optional H0 string 218 (`DAh`) at an
additional height*5/100; the canonical file/wait callers supply 1/0.

The handler dispatch count at `0053AA10` is 16, followed by pointers at
`0053AA14`. Relevant pool owners are H0=`0055EE50`, H3=`0055EDA8`,
H4=`0055EE20`, H6=`0055EEC8`, H7=`0055EE68`, H8=`0055EEE0`.

Each pool is a static object the CRT builds and registers for teardown:
`mov ecx, pool; jmp constructor` behind a two-instruction initializer, and
an `atexit` (`00511D89`) of `mov ecx, pool; jmp destructor` (see
the small functions). Every constructor runs the base
`004EFB90`, installs an intermediate table, calls `0050ECB0` (`0048EE20`;
a nonzero answer exits through `005122A5`) and installs its final table;
every destructor calls `0050ECC0` and the base `004EFBD0`. Every final
table's `+0` is `004EFBF0` (`xor eax, eax; ret 4`): no pool loads a
missing slot on demand, so a vacant slot reads NULL. `+4` returns the
handler's descriptor (`0053AA54` and on, `005395F4` for H9, `005363A0` for
H12). `+8` is the handler's bank reader, as the
[format notes](FORMAT_DOCUMENTATION.md) list them (H0's `0050ECD0` reads a
count, then installs each string through `004EFC00`), `+0C` frees the bank (`0050ED60`,
through `004959C0`, unless overridden) and `+10` is empty (`00466D40`)
except for H3:

| Handler | Pool | Constructor | Destructor | Table | `+8` | `+0C` | `+10` |
| --- | --- | --- | --- | --- | --- | --- | --- |
| H0 | `0055EE50` | `004740A0` | `00474050` | `00527598` | `0050ECD0` | `0050ED60` | `00466D40` |
| H1 | `0055EE80` | `00474190` | `00474140` | `005275C0` | `0050ED90` | `0050ED60` | `00466D40` |
| H2 | `0055EE08` | `00474280` | `00474230` | `005275E8` | `0050F3F0` | `0050ED60` | `00466D40` |
| H3 | `0055EDA8` | `00474380` | `00474330` | `00527610` | `0050EE80` | `0050F150` | `0050F1F0` |
| H4 | `0055EE20` | `00474470` | `00474420` | `00527638` | `0050F250` | `0050ED60` | `00466D40` |
| H5 | `0055EDC0` | `00474560` | `00474510` | `00527660` | `0050F340` | `0050ED60` | `00466D40` |
| H6 | `0055EEC8` | `00474650` | `00474600` | `00527688` | `0050F3F0` | `0050ED60` | `00466D40` |
| H7 | `0055EE68` | `00474750` | `00474700` | `005276B0` | `0050F490` | `0050F560` | `00466D40` |
| H8 | `0055EEE0` | `00474840` | `004747F0` | `005276D8` | `0050F5A0` | `0050F720` | `00466D40` |
| H9 | `0055EE98` | `00474930` | `004748E0` | `00527700` | `0050F760` | `0050F830` | `00466D40` |
| H10 | `0055EEB0` | `00474A30` | `004749D0` | `00527728` | `0050F890` | `0050ED60` | `00466D40` |
| H11 | `0055EEF8` | `00474B30` | `00474AE0` | `00527750` | `00437B50` | `0050ED60` | `00466D40` |
| H12 | `0055EE38` | `00474C20` | `00474BD0` | `00527778` | `004507D0` | `0050ED60` | `00466D40` |
| H13 | `0055ED90` | `00474D10` | `00474CC0` | `005277A0` | `004480B0` | `0050ED60` | `00466D40` |
| H14 | `0055EDD8` | `00474E00` | `00474DB0` | `005277C8` | `004480B0` | `0050ED60` | `00466D40` |
| H15 | `0055EDF0` | `00474EF0` | `00474EA0` | `005277F0` | `00481090` | `0050ED60` | `00466D40` |
H4 font 0 is not H7; H7 slot 32 is the presentation word below. Native
capacity comparisons and NULL dereferences do not justify typed fallback
font, color, material or surface data.

## Presentation and physical file reads

`0040ED50` freshly captures renderer `+4`, unlocks through
`004957F0/0050DCC0`, then resolves current H7 slot 32 and reads its first
DWORD for `00495810`. Missing capacity/residency has no native black/default
success. `00495810` presents through `0050D330` **before** clearing the next
color/depth surface through `0050D960`. A late clear failure retains the
presentation. Its own guard consumes error `A04`; other errors reach the
outer callback guard. This is separate from title/briefing/world predicates
and their forced-black decisions. The port CPU renderer has no DirectDraw
surface lock, so that backend operation needs an explicit bounded mapping.

`004F3DE0` reads destination ECX, file EDX and one total-length stack DWORD.
With no callback it performs one exact `fread(dest,1,total,file)`. With a
callback, chunk lengths/comparisons are unsigned and the elapsed low-clock
subtraction is signed wrapping:

- While current chunk + bytes-read < total, exact-read that chunk, reread
  current chunk and advance. Fresh callback NULL bypasses timing/progress.
  Otherwise sample time and add chunk to pending progress. Elapsed >40000
  calls the fresh callback with raw pending progress and stack argument 1.
  Nonzero callback return exits before resetting pending/time/chunk.
- Success clears pending and stores sampled time. A strict signed positive
  margin >4000 below/above 40000 grows/shrinks chunk by 1000; shrink additionally
  requires unsigned chunk >2000. Updated chunk controls the next read.
- The final exact read uses total minus bytes-read, then pending adds the
  **requested total**, not the remainder. Pending <= chunk/4 returns retained.
  Otherwise capture pending, check fresh callback, sample time and add captured
  pending **again** before the elapsed gate. This doubled progress is original
  behavior, not correct byte accounting. Normal completion eventually clears
  pending even when no timed callback was eligible; late callback failure
  retains its reached prefix. Fresh callback NULL clears pending without time.

Short reads return native error `1002` before unreached progress/clock work.
Loading callback contained rendering errors return 0, allowing reads to
continue; actual resource-read errors remain distinct. Standard handler reads,
atlas/model readers and relocations have different scalar/block boundaries.
One callback per overlay or handler does not establish this cadence.

## Consumed ordinary loading and remaining boundaries

The retained [loading process](../../crates/inf-game/src/loading_runtime.rs)
reads the authenticated executable startup span once. The
[read state](../../crates/inf-game/src/loading_read.rs) retains its authored
8000-byte chunk and loader-zero callback, clock and pending progress.
[Frontend platform](../../crates/inf-game/src/frontend_runtime.rs)
`loading_mode` is the existing profile transition BYTE owner; profile actions
set it to 1, and Results writes 0 before device enumeration at `0042AC48`.
The sixteen effects remain `PortSession.text_process.effect_bytes`, shared
with name entry. Callback installation and repeated loads reset none of the
clock, pulse, selector, effects or process RNG.

The shared [world loading bridge](../../crates/inf-game/src/loading_integration.rs)
sets `00546920=1`, installs the callback, then supplies the same retained
owners across both leaf48 and selected mission+10 activations. Successful
normal exit exchanges literal NULL; an earlier resource/load failure retains
the installed binding and completed state prefix. It does not infer a native
exceptional detach. Passive preparation and headless session callers explicitly
select `NoCallbacks` and acquire no application clock or renderer.

[Installation](../../crates/inf-game/src/installation.rs) now invokes the
adaptive kernel over an actual `File` and requested destination before the
existing parser/handler path. **WholeFileReadCadenceApproximation** groups one
physical OVL into one read request; it preserves the recovered inner read
algorithm but cannot establish native per-handler scalar/block boundaries.
A zero-request leaf still opens no file. Actual I/O failures propagate separately
from contained rendering failures. The follow-up owner is Installation's
handler read API; replace this grouping when readers own those native requests.

[Residency](../../crates/inf-game/src/residency.rs) still stages complete
transactions. **TransactionalResidencyViewApproximation** lends the actual
committed table during each physical read, then the newly committed leaf48
view during the next activation. It never reuses the application's old renderer
resource snapshot. Native `004EE4E0` partial handler publication remains open;
its follow-up owner is the residency transaction/handler registration boundary.

The [SDL loading adapter](../../crates/inf-game/src/frontend_app/loading.rs)
borrows the same application clock, renderer ID and framebuffer used by the
scheduled session. Both world activations can present while the renderer epoch
is suspended. A [scoped current-resource loan](../../crates/inf-game/src/software_scene/current_resources.rs)
keeps old borrowed caches in custody, clears the real queued batch only at
reached `0040E6FD`, and copies reached scalar/pixel state back on success or
error. Fresh CPU uploads have temporary cache custody; no deferred
resource-backed command can escape. **CompletedResourceBatchRetirementApproximation**
allows an explicitly selected loading batch to retire its temporary payloads
only after the actual flush completed every payload, all groups closed, the
queue remained running, and the entire callback succeeded. Group headers are
excluded from payload counts, and manually closing an empty batch does not
qualify. Native `004A4460` retains its arena nodes; repeated replay, a later
early error before the next `0040E6FD`, and retained resource/packet custody
remain different. Pending commands or an error after replay retain a typed
`DeferredCommands` boundary, the reached scalar/pixel prefix and stopped
containment; borrowed payloads cannot survive scope exit. The follow-up owner
is SoftwareScene's current-resource batch custody. This is a CPU cache policy, not native
atlas identity or allocation parity. The renderer and framebuffer are retained.
Session+648 is resolved from the actual registry inside the callback guard,
after renderer presence; an absent renderer bypasses that lookup. The shared
app sampler supplies independent read and callback samples. Its process-relative
portable clock and explicit fixed-step testing policy do not prove native
Windows uptime/timeGetTime wrap behavior. Under the fixed step only a frame's
own sample advances; the reader's and the callback's samples inside that frame
read its time, as the scenario lab's scripted clock stands still within a
retail pass. A load then runs one callback: in a new game's intro load retail
draws its 16 equalizer bars once (`0040EB0C`), and the port did 120,640 draws
while every sample advanced a whole step
(lab).

[Loading draw](../../crates/inf-game/src/loading_draw.rs) owns ordinary
nonzero-mode equalizer/title, zero-mode background/title, common flush and signed attract
counter policy. [Model117](../../crates/inf-game/src/loading_model.rs)
uses the shared H8 interpreter and queue. Its retained successful-startup
template authenticates the PE loader-zero tail, unit-basis initializers,
fade/table stores and object vtable. Lighting precedes fresh captured-renderer
dimensions; a further height read supplies signed `height*80 >> 8` center Y.
Focal, viewport and center publish before yaw, fade, view and model submission.
Position is `(0,0,500)` and the full signed age DWORD supplies `/32` yaw.
Vtable `00520494` has `+4=0043A2D0`, sharing the existing unsigned-row vertex
consumer, and `+0=0040E0D0`, with its distinct packed-value policy: selector0
is low WORD of signed age/200, 1 is the captured selector, 2 is pulse low WORD,
3 is selector0 times100 with WORD wrap, 6 is1, and all others0. AnimationClock
does not supply these values. Context+6C/+70 supply invocation-local far-color
and mode through an explicit model submission policy, preserving inherited
renderer values. The actual process RNG is lent to the model and its reached
prefix is committed on errors. Incoming context+0 aliases the same captured
renderer under this admitted typed domain; arbitrary pointer aliases, native
individual scalar faults and unproven later computed template writers remain
outside it. Complete original-body guards and supplied-domain/error tests live
with the model owner; full scene/primitive and retail visual acceptance remain separate.
Selector2 can reach child131's inherited register3 read before parent119
initializes that word. Loading now selects
**LoadingModel117StackHistoryApproximation**, which starts the root's register
3 at `6EBB`, the stack value Infest01's post-intro loading screen read
([model runtime](MODEL_RUNTIME.md#loadingmodel117stackhistoryapproximation)).
Any other unknown register falls back to
**UninitializedValuePrefixApproximation** from the shared model submission
policy. It retains only the completed model command/RNG/publication prefix
when the actual implicit root remains intact, then reaches the ordinary
finish, flush, title and presentation tail. Other errors retain the existing
callback containment. No missing register is zero-filled or carried between
root invocations. The separate model-prefix omission counter and first-use
log expose this gap; its source path, queue bounds and follow-up ownership are
documented in [model runtime](MODEL_RUNTIME.md#feature-level-unowned-value-containment).
The attract loading draws its four-sprite fade (`0040E140`, ported
2026-10-19): H3 sprites 8 to 11 as a two-by-two grid of quads a quarter of
the surface wide and 5/16 tall around its centre, each an effect packet
(`004A42B0`, callback `0040EF30`, renderer `+A4`) in H6 style 14 with its
colour bytes scaled by the fade and its alpha kept. A sprite past the pool's
count or vacant ends the fade there. The quads do not overlap, so the queue's
sort order changes nothing, and the port draws them in place.
check_attract_fade.py runs `0040E140` over 23
cases with the packet allocation recorded, and the port matches each.
Present freshly resolves H7/32, presents
before the next clear, and keeps the port's [output/scaling policy](PRESENTATION_POLICY.md):
native at an original resolution, the classic framebuffer enlarged otherwise. CPU
surface validation maps DirectDraw preparation/unlock without claiming lock
parity. Diagnostics report actual callback, presentation and contained-error
counts; no overlay pulse is synthesized.

These boundaries do not remove the production idle-demo deferral, supply
constructor-unwritten game handles/timers, or admit the three attract handshakes.
Counter arithmetic alone does not prove their drawing/wait execution. Native
partial publication, attract background draws, attract preparation/playback and phase8
return remain owned by [attract runtime](ATTRACT_DEMO_RUNTIME.md) and
[runtime foundation](RUNTIME_FOUNDATION.md).
