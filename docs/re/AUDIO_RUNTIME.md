# Infestation audio runtime

This document owns recovered music selection, CD control, frontend cues and
shared process sound records. The
[runtime foundation](RUNTIME_FOUNDATION.md) owns session/phase ordering; the
[frontend authority](FRONTEND_RUNTIME.md) owns screen construction and ticks.
Evidence is the original `Infest.exe` consumers and bounded static disassembly.
No playback or matched audio observation is implied by command-recording tests.

## Shared music selection

The session's `+5D0` is an optional eight-byte music selection, not an audio
device. `00487310` releases the old selection and supplies descriptor
`[1, requested_track + 1]` to `004F60B0`. The resulting selection stores the
full track word at `+0` and its low byte as initial packed TMSF position at `+4`.
Frontend helper `00487360` passes 7, selecting physical CD track 8: `004F68A0`
uses the full word as the MCI track-length query's `dwTrack`, the saved TMSF as
play-from, and the next track's low byte as play-to. The original disc-number
mapping is separately recorded in the installed CD audio:
physical track 8 is `cdaudio/track08.ogg`.

The selection persists across frontend/world screens. Process globals
`0065EE20/24/2C/30` own the shared CD transport. The Rust
[`FrontendAudio`](../../crates/inf-game/src/frontend_audio.rs) reconstructs
that single session-slot/process-transport combination. The concrete
[`PlatformAudio`](../../crates/inf-game/src/platform_audio.rs) supplies one
SDL audio device for that owner and frontend H9 cues. Multiple independently
competing music selections are not implemented.

## Pause, resume and retry

`00487230` delegates to `004F7500`: an active selection saves the current packed
TMSF position, then stops/closes the CD device while retaining the selected track,
retry flag and deadline. TMSF stores track/minute/second/frame in increasing byte
significance. Selection/release resets the saved position to the track's low byte;
it does not clear the retry/deadline globals.

`00487370` is an exact jump to `004872A0`. Resume reads the full configuration
word at `0054E8A8+4` on each call. Zero pauses; otherwise it expands volume as
`v | (v << 4) | (v << 8) | (v << 12)`, then the AUX consumer duplicates it into
both 16-bit channels. Do not narrow the configuration to a byte or assume the
normal range is an enforced clamp. Resume applies volume before starting playback.

CD opening selects TMSF format and checks media plus whether **track 2** is audio,
independently of the requested track. Native mode comparison recognizes the exact
playing state. Actual device handles, MCI strings and media access belong to the
required `FrontendAudioBackend`; its operations have no successful defaults.

Absent-disc policy (portable transport behavior, verified against the
an installed game's layout with no `cdaudio/`): a missing `cdaudio/`
directory opens the transport with no media, and a missing `track02.ogg`
probes as non-audio, so `004F68A0` takes its no-media exit and the
frontend factory boots silent instead of failing. The selection, volume
word and retry/deadline machinery survive for a later disc. A present
but unreadable directory or undecodable track stays an explicit backend
failure. The SDL playback device itself is still required.

The restart deadline comes from signed low-word CRT `time(0)` seconds at
`00513260`, independently of scheduler microseconds. `004F68A0` queries the full
track's MSF length and adds `minutes * 60 + seconds + 2`, including when resuming
mid-track. A failed length query deliberately substitutes 3600 seconds. Preserve
signed wrap and zero as the deadline sentinel.

`00487B90` services only when selection exists and the live volume word is nonzero.
`004F7E20` expires a nonzero deadline strictly when `deadline < now`, clears it,
and marks retry. Service clears retry **before** the next play attempt; it does not
reapply volume. Notifications handled by `004F7DE0` set retry for native codes
1/4/8; code 2 is handled without changing it, and other codes are unhandled.

The service is what loops the music: a finished track notifies, and the
next service plays it again. Four ticks run it, so every screen loops: the
frontend tick (`00484E23`), the world update's tail (`00487B3C`, reached
from every path through `004875A0`), the briefing tick's last call
(`0040CE25`) and the paused tick (`0043899D`). The port's backend never
restarts a track by itself, and the port serviced only the frontend, so a
world's or a briefing's track played once and stopped; the platform's
`service_music` now runs on all four, through the session's
`SessionMusicRequest::Service` in the world and paused ticks.

Native error paths dispose returned error objects; resume and service clear the
volume after failed playback. Rust exposes backend errors and stops the owner
after partial transport failures, preserving completed effects. This is explicit
containment rather than a claim to reproduce native error-object disposal or
nonlocal recovery. The length-query fallback above remains native behavior.

## Sound menu volume consumers

The frontend [Sound menu](FRONTEND_RUNTIME.md#sound-menu) adjusts settings
`+00`/`+04` through `00453B80`, then runs a fixed tail that re-reads its own
word through `00463AB0`:

- `00463AC0` passes the nibble-expanded `+00` to `004F4780`. That routine stores
  the FX master at `0065ECC4` and walks the active voice list at `0065ECB8`.
  For each voice it computes `((master >> 8) * voice[+8]) / 255` with signed
  division, clamps to 65536, and uses -10000 for nonpositive levels. It applies
  the same logarithmic hundredths-of-dB attenuation as cue start (`004F4498`)
  through `00510600`, sharing `00544CE8` (ln 65536 stored by `004F40B0`) and
  `005293A8` (-10000). A SetVolume failure is returned and raised by `004949A0`.
- `00463B00` first computes the linear level `(v * 65535) / 15`. It uses
  wrapping `shl 16; sub` and signed division toward zero, so the level is fixed
  before `00487370` (exact jump to `004872A0`) resumes or pauses music with the
  same word, and a failed play there can clear that word. `004F7D80` then
  enumerates AUX devices and sets `level | level << 16` on every
  CD-technology device, ignoring results. This runs even without a selection.
  Below the multiply wrap the level is `v * 0x1111`. It matches the nibble
  expansion for 0..15 and for other words whose shifted copies share no set
  bits (16). Overlapping copies (17) or a wrapped multiply (0x10000) diverge.

The portable mixer derives every live voice's attenuation from one stored FX
word, so `PlatformAudio::set_effects_volume` is the `004F4780` re-attenuation;
it cannot fail. Cue start also re-stores the live settings word instead of
reading a separate master. That is equivalent for the connected writers of
`+00`: session construction (`00438768`) and `00463AC0` both follow the word
with `004F4780`. An unconnected writer such as the `00463B70` reset must keep
that pairing when it is implemented. `FrontendAudio::apply_volume_setting`
preserves the music ordering. A backend failure in its resume stops the owner
before the AUX write, matching the existing containment rather than native
error disposal. Unicorn receipts from the unmodified tails pin all arguments
for commands 2/3/4/6/7, including out-of-range words and cells other than the
tail's own.

## SDL transport and resident frontend cues

The portable backend decodes the installed 44.1 kHz stereo Ogg files with Lewton
and verifies each decoded sample count against the original INDEX1 sector length.
It caches decoded samples, converts the playback cursor to TMSF at 75 sectors per
second, and preserves the full track word in length queries. A completed track
reports notification 1 to the music owner; the mixer does not loop independently.
Logical CD stop/close leaves sound effects running on the shared SDL device.
Absent or incomplete disc banks report media unavailable through the native
probe. Malformed requested tracks and actual device-open failures remain errors.

`004391D0` selects an H9 resource and passes the four words at `005333F8`
(`[65536,65536,0,1]`: gain, pitch, pan, final control word) to
`004F46E0` / `004F4230`. The normal frontend uses slot 2 for previous/next,
3 for adjustment and 5 for confirmation. Type-5 aliases apply the proven
spread/pitch/gain fields documented in [the H9 format](FORMAT_DOCUMENTATION.md).
Each alias consumes one step of caller-owned RNG `0055F18C` through `0048F060`,
including when its spread is zero. Frequency uses wrapping `pitch * 22050 >> 16`,
then the `005105C0` clamp to 100..100000 Hz. Gain follows the native signed
SoundFX-volume arithmetic and logarithmic DirectSound attenuation. The original
24-voice cap first reclaims completed or earliest automatic voices; the
one-shot wrapper consumes any remaining `0x2202` capacity error.

Decoded PCM is shared by voices through retained sample allocations, so unloading
a bank cannot invalidate the audio callback. Same-allocation alias targets retain proven load-time
identity; cross-allocation aliases are rejected until an allocation-owned
relocation map can preserve the pointers fixed by `0050F760`. Linear sample
interpolation, floating-point summation and final clipping are explicit SDL
backend policies, not a claim of sample-identical DirectSound output. The
native fixture tests compare 130 original alias executions and 128 original
attenuation executions. Corpus tests decode the complete original track 8 and
mix the actual resident frontend cues; an opt-in device test exercises real
playback, pause and resume without a mute or dummy-device fallback.

## The port's ambience (Vulkan only)

The sound the port adds plays
on a bus of its own in the same SDL mixer, beside retail's music and
effects: its own voices, never retail's pool or its RNG.

- **The Ambience volume**, 0 to 15 on retail's own curve (the Sound
  slider's), saved with the Vulkan settings (default 10). It applies only
  under Vulkan; OpenGL and the software renderer keep retail's sound. The
  Sound menus (`00538E68`, in world `00538E80`) carry an Ambience row, a copy
  of the Sound slider on a port cell, on a layout chain of their own; the
  launcher has its slider; `--ambience N` sets it.
- **0 is retail's sound exactly**: no ambience voice plays and no limiter
  runs, so the mix is the same sample for sample.
- **The bus** (`platform_audio.rs`): each voice a decoded sound, looping
  ones crossfading their last 1.5 s (equal power) into their start so no
  recording needs a loop point; gain, pan and pitch ramp over about one
  buffer (1,024 frames), a pitched voice reading between frames; a mono
  voice pans at constant power; a stopped one fades out, then goes. While
  it plays, a peak limiter holds the whole mix under 0.95 (retail's effects
  alone reach full scale), letting go over 0.1 s, and the bus dips under
  retail's loud effects: from 0.4 of full scale (-8 dBFS: a blast, not the
  machines' steady hum), 0.85 of the excess, at most 6 dB, at once, coming
  back over about 0.8 s.
- **The beds** (`ambience.rs`, once a frame on the game thread): each world
  plays a bed of its kind of place (`modern_tuning.json`'s `atmosphere`,
  the plan's table), about 15 dB under retail's music, out of play and in
  the in-world menu fading out within half a second. A bed takes turns
  between two or three recordings of 2 to 3 minutes, each starting at a
  point the world and the turn choose, the next fading in over 9 s every 2
  to 4 minutes. Its sweeteners, one-shots, come every 8 to 40 s (a bed may
  give its own span: the Brain's groans 20 to 60 s, Oolbryte's far thunder
  45 to 120 s), panned somewhere around and nearer or farther, never the
  same one twice running, each up to 4% higher or lower and 2 dB louder or
  softer; the Brain's pulse beats ahead, 50 a minute. Artoria's drone
  follows the world's light on retail's tint schedule (`world_tint::light`,
  0 at its darkest style, 1 at its brightest): when the four generators
  die it falls to 0.15 of its level and 0.55 of its pitch over the
  schedule's second, over a bed of wind beneath it; world 1 grows quieter
  through its dusk (its schedule's styles 0x39 and 0x3A, from 400 s), and
  Kankutan breathes with its 90-second swing. The choices hash the world
  and a turn, never the game's RNG, so a fixed-step run sounds the same;
  the recordings decode whole on a thread when a world starts (about 50 MB,
  100 MB on Artoria with its wind beneath; an `--audio-out` render waits
  for them), the Brain's from the world's load (below). Each start,
  change, sweetener and pulse writes an `ambience` event-log line (`bed`,
  `recording`, `what`), and each second of play a `levels` one: retail's
  effects, its music and title, and the ambience, in dBFS RMS before the
  limiter, and the ambience's deepest dip.
- **The Brain's beds replace its music** (worlds 0 and 19): while the Ambience is above 0 and the pak holds a loop of the
  Brain's bed or the one beneath it (`ambience::replaces_music`), the
  frontend asks the mixer each frame, from the world's load (its
  briefing) through play and its in-world menu (phases 5 and 6), to hush
  the world's track (00487250's word, track 5;
  `PlatformAudio::hush_music`); the computer's Map and Objectives (phase
  7) and the results screen (phase 8), where the beds stop, have it back.
  In that menu the beds keep playing, unfaded, as the music would; every
  other world fades them there as before. The Ambience in effect follows
  the renderer running, not only the one chosen: a Vulkan that fails to
  start runs OpenGL, with retail's music and no beds
  (`FrontendSettings::set_running`). A voice of the hushed track starts
  hushed, and a change ramps 1/1024 a frame; the voice still runs its
  frames, so the position, mode and end the music owner reads, and its
  selection and retries, stay retail's. Any other track mixes sample for
  sample as before. The hush is set after each frame's steps; track 5
  starts at gameplay's entry from the briefing (00487250), frames after
  the load set it, so it starts hushed (a `--start` on world 19: the load
  at frame 3, gameplay at 64, the music bus at -120 dBFS from then). The
  Brain's recordings decode from the world's load, so its beds start
  with play: decoded at play's start, its bed began 26 to 28 frames
  (about 0.45 s) after play did in a hidden release run (its first
  recording, `beds/brain-2`, decodes after `brain-1`, about 0.25 s alone;
  the swirl beneath it after the bed's other sounds, by about 0.45 s);
  now it begins on play's first frame.
- **Placed sounds** (`ambience/placed.rs`): after each frame's draw a
  survey lists what is near the camera (retail's listener, the draw
  receipt's eye and right axis): the actors and furniture whose model names
  the tuning's `sources` give (generators, power transformers, Foldrak
  Noir's spheres, steam vents), every force field (selector 18, by its
  fade, heard from its dome's wall and all around inside it) and barrier
  (selector 69, from the nearest point between its posts), and the nearest
  shore in each quarter around the camera, from a 64-by-64 map made as a
  world loads: a cell is shore where a grid point below the plane (retail's
  wet test) meets a dry one, heard from the middle of those points, made
  again when the plane moves 8 units (world 12's reservoir). The sixteen
  loudest play, a sound playing ranking half as loud again so voices do
  not flicker; each loops from a point its place picks, pans along the
  camera's right (toward the middle within 300 units), falls off as the
  square of its share of its range, and loses its highs through a one-pole
  low-pass (16 kHz at it, 2.4 kHz at its range), fading in and out over
  half a second. A transformer's crackle and lava's pops play from their
  place now and then. `placed`, `shot` and `gone` event-log lines.
- **Occlusion**: each frame the survey tests the ground at fifteen points
  between the camera and each placed sound (raised 100 units off its
  place, the line given 32 units of room); a sound behind a ridge goes to
  half its loudness (6 dB) and a fifth of its low-pass's share (much
  duller) over 0.3 s.
- **The reverb** (`platform_audio/reverb.rs`): one feedback delay network
  on the ambience bus, eight lines of 1,117 to 2,887 frames (primes) fed
  back through a Householder matrix, each line damped by a one-pole
  low-pass and held to its share of the decay time. Placed sounds and
  sweeteners send 35% (the tuning's `reverb.send`), retail's placed sounds
  25%; beds and the pulse send none. Each world has a preset (`reverb`:
  open 0.9 s, canyon 2.2 s on 7 and 13, metal 1.6 s on 16, city 1.4 s on
  11, organic 1.2 s on 0 and 19), its wet scaled from 0.5 to 1.6 times by
  how walled in the camera is: the share of eight points 800 units around
  it whose ground stands above its eye, followed over a second.
- **Retail's sounds placed** (Ambience above 0): `00480110`'s rotated,
  listener-relative vector rides with a positional sound's parameters
  (`SoundPlacement`, with whether it is under the water's plane) to its
  voice. The mixer then pans it by its direction at constant power, scaled
  to retail's level in the middle (both channels full; one side 3 dB up),
  toward the middle within 300 units; dulls it from 16 kHz to 3 kHz over
  retail's 256 to 8,192-unit falloff, or to 400 Hz under the water; and
  sends its share to the reverb. Retail's own gains (attenuation, its
  distance falloff, the master volume) stay. At 0 the old path runs, so
  the mix is retail's sample for sample.
- **The crystals' shields**: a gun's bolt
  inside a scenery crystal strikes it (`crystal_flares.rs`, each frame in
  play, after the draw); each fresh strike (none in the 120 ms before)
  rings a shield sound from the crystal, placed as the
  placed sounds are.
- **First person** (F12, the camera's mode 2): the camera is inside the
  vehicle's shell, so the ambience bus and retail's placed sounds beyond
  300 units are 2 dB down and low-passed near 5 kHz.
- **Headphones** (Vulkan, off by default; a row after Ambience in both
  Sound menus, the launcher's combo, `--headphones BOOL`): the placed
  sounds, the shields' rings and retail's placed sounds reach each ear
  apart (`Ears`, a cheap model, not a measured set). The far ear hears a
  sound later, by up to 29 frames (0.66 ms) at the side, in the head's
  shadow: through a one-pole low-pass down to about 2 kHz and 3 dB down.
  The near ear is 2 dB up, so a sound keeps its loudness as the head
  turns (a 200 Hz hum within 1 dB). From behind both ears are duller,
  down to about 3.6 kHz. A sound ahead is what the pan gives it in the
  middle. The beds and the sweeteners keep their stereo. Off, the pan
  runs as before; at Ambience 0 the mix is retail's.
- **The sounds** come from `Remastered.pak` beside the game's data
  (`platform_audio/remastered_pak.rs`): our own archive (an index of named
  entries with their SHA-256, then the files), Ogg Vorbis at 44.1 kHz,
  built from the port's own sounds by `scripts/build-remastered-pak.py`
  through `ambience/manifest.json` (each trimmed, filtered,
  brought to its loudness and held under -2 dBFS by a limiter). Without it
  the remastered sounds are off and the row reads Unavailable.
- **Retail's gains** are worked out when a voice's attenuation or pan
  changes, not for each sample: the same numbers, three power functions
  less per voice per sample.
- **`--audio-out FILE.wav`** renders the mix from the frames (each its
  fixed step's share, the device paused), so a fixed-step run gives the
  same file every time, for measuring the sound without listening.

## Remaining integration

Frontend construction pauses music around leaf-47 activation and then resumes it;
each frontend tick services music after base input. The factory and dispatch
require those effects from their host. The SDL frontend now connects them
through `FrontendPlatform` to the shared audio owner and live portable settings.
Persistent world voices now use the same SDL device and shared voice owner.
NonNULL pitch modulation, native H9 pointer fixups, cross-allocation alias
relocation and matched original-game playback remain separate work.

## Process positional one-shots

[process_sound.rs](../../crates/inf-game/src/process_sound.rs) owns one retained,
non-Clone process list `0055F0E8/EC/F0` shared by one-shot and persistent
records. `00487F10` and
`00487EC0` resolve the signed low-WORD H9 selector and forward original XYZ,
gain, pitch and final flag through `0047FF60` to `0047FFF0`. The native
40-byte record copies its sound pointer at `+08`, gain/pitch at `+0C/+10`,
and world WORD XYZ at `+20/+22/+24`. `0047FF60` changes flags `+14` to 2,
ORing 4 when the final argument's low byte is nonzero. The return value is
a nullable error object; `004923C0` destroys that error, rather than playing
the sound. Birth and impact hosts share
[ParticleSoundRequest](../../crates/inf-game/src/process_particles/audio.rs)
and retain XYZ independently from later particle mutation or retirement.

The list captures immutable H9 allocation identity at enqueue without
consuming alias RNG. At later `00488020` world draw, `00480110` reads the
actual current render context after particle passes and pending transfer,
before terrain `0044AA10`. Its listener subtraction wraps to signed WORDs.
Gain zero or any absolute component at least executable `0053BB80` (8192)
rejects the sound. `00490A80` rotates the accepted WORD vector through the
actual float basis with X/Y/Z sums and nearest-even low-WORD stores. Pan uses
`0053BB84` (4096), saturated `0048EF30` and signed high-product arithmetic;
the positive inclusive endpoint is 9999, becoming 10000 only beyond it.
Distance uses the wrapped signed square sum and `0048EFC0`, retaining its
nonpositive-input zero result. Distance at most 256 keeps gain; farther
accepted sounds use the original Q31 falloff. Phase 5 produces zero gain and
retires an optional voice while retaining its record. Process `0055F0F4` skips
the list service altogether; the outer one-shot cleanup still runs.

Only an admitted sound reaches `ProcessSoundVoiceRequest::OneShot` through
the explicit shared audio loan. `PlatformAudio` sweeps the shared voice list
before resolving retained aliases, and repeats the enabled/sweep phase for
each recursive alias creation. It then applies the original piecewise
registration map, consumes actual shared RNG and starts PCM on the
frontend/title/music SDL device. One-shots and persistent voices use the same
24-voice admission, frequency, master gain and pan policy. The port's
`splash_voice_cap` fix (on by default since 2026-10-19, after a generator's
explosion was found cut short in retail too; called `more_sound_voices`
until 2026-10-26) caps the water splashes
(H9 slots 9 and 10, tagged by the slot the cue requested, before any alias)
at 12 of the 24 voices. It also raised the admission and the sweep's
eviction to 64 voices until 2026-10-26, when that was found to have
unintended effects; the pool is retail's 24 again. While
12 play, a new splash is refused as a full pool refuses a sound, after its
alias walk has drawn its RNG, so the playing ones finish (an earlier version, which
silenced the oldest splash instead, cut splashes short). Neither changes the RNG drawn, and no simulation reads the pool, so
only what is heard changes. The lab runs with it off.

Hovering over water is what fills the pool. The hull spray `00488E90` emits
drops every tick, as many as the load factor allows: process `0055F154` is
65536 while the average frame is under 34.464 ms, so above about 29 frames
a second each tick emits 8 drops, and the particle pool caps how many fly.
Each drop landing in water births a splash (`0048A8F0`) and, one in 32,
plays slot 9 or 10 at a pitch of `0xC000..0xFFFF`, 0.6 to 0.75 s long. The
lab measured retail at about 31 splash sounds a second with 16 ms frames and
the same with 33 ms ones, which keeps about 20 of its 24 voices busy, so
the oldest one-shot a new sound reclaims is often an explosion, the splashes
being younger. Pacing the spray to 30 fps would change nothing, and 64
voices alone let every splash sound, which sounded as a cacophony;
the cap leaves the spray audible and 52 voices to the rest.

Later residency changes cannot redirect the retained source. Cross-allocation alias pointers remain explicitly
unsupported. Both queue cleanup and completion's `004800E0` remove the owned
one-shot records without stopping already emitted PCM. On a service error,
the world caller still cleans the queue; earlier voices and consumed RNG
remain. This preserves the original prefix instead of retrying the entire
queue next frame.

Production particle-update impacts now enqueue into the retained session
sound list, and the world draw supplies its actual listener and shared SDL
audio owner before terrain submission. Draw receipts retain the successful
voice-start prefix even when a later voice fails. Particle birth/retirement
attachment remains separate; the producer's owned request boundary does not
itself establish those live callers.

check_process_sound.py executes original
enqueue, listener arithmetic and cleanup without instruction patches, with
only allocation/free and voice playback supplied. It compares 768 cases
plus three partial-failure prefixes and produces 75 bounded Rust parameter
fixtures. Corpus tests start and mix actual H9 slots 6/9/10 through the shared
voice path, retain aliases after source-owner disposal, and pin the 24-voice
cap. These establish the portable one-shot owner and PCM consumer, not
sample-identical DirectSound or matched retail scene audio.

## Shared record lifetime

[records.rs](../../crates/inf-game/src/process_sound/records.rs) owns
`0047FFF0`, `004800B0`, `0047FFA0`, `004800E0` and explicit process clear
`0047FF20`. Initializer `0047FEF0` installs a static head/terminator; it does
not allocate a sentinel. Portable next/previous relations represent the native
terminator `0055F0EC` and head `0055F0E8`, rather than fabricated pointer values.
Process-qualified serial references never revive released records or duplicate
their storage when an actor snapshot is cloned.

Construction uses a single request carrying the captured resource, kind,
scalars, control flag and lazy XYZ source. Fallible portable metadata allocation
precedes a real, retained 40-byte backing allocation. List linkage and resource
`+08` publication precede current DWORD XY and WORD Z reads, then gain, pitch,
flags and NULL voice/modulation stores. Caller publication occurs last on a
normal return. A source error preserves a linked pending record whose unwritten
fields remain unknown; service and cleanup demand flags `+14` first. The backing
is an allocation-lifetime adapter, never a raw native memory image or proof of
malloc address/reuse behavior.

`004800B0` reads optional current XYZ before gain/pitch, without reallocating or
reacquiring H9. `0047FFA0` unlinks before freeing modulation and deleting a
nonnull voice, then releases record backing. A portable custody/service error
retains the detached prefix; it does not relink or silently repeat deletion.
Constructor-NULL modulation and persistent voice creation, update and deletion
are owned. Unknown/nonNULL modulation remains a typed reached error.
No Drop callback, per-world replacement pool or hidden cloned owner runs these
native operations.

One-shot cleanup removes only flags `+14` bit 2. Persistent records retain their
birth order with one-shots across service, completion and world replacement.
Service captures NEXT before callbacks, computes listener admission, then
demands optional modulation for nonzero gain and fresh pitch `+10` even at zero
gain. Fresh flags precede the one-shot/persistent choice, and unknown voice
storage remains a required read rather than a constructor-NULL assumption.
Earlier started voices and
consumed RNG survive a later feature error. The outer cleanup remains explicit.

## Class10 helper and persistent actor sound

The [class10 child owner](../../crates/inf-game/src/object_runtime/actor/derived/selector10_helper/tick.rs)
runs after `00425400` physics against the actual installed `+26C` helper.
A completed child reads the current descriptor's sound WORD; only a nonzero
sound consumes shared `0048F060` RNG. Pitch is `((random & 0xFFFF) >> 2) +
0x1E000`, with gain `0x1000` and final control zero. The real H9 selector feeds
a one-shot in the same process record list. Actor XYZ is demanded by the
constructor after list linkage/resource publication, rather than preloaded
before allocation. A late source or sound error retains child copies, RNG and
record linkage; helper index `+08` advances only after normal completion.
Actual constructor-zero child flags/timers can bypass this sound arm; mode 1
alone cannot imply that bypass.

[selector10_audio.rs](../../crates/inf-game/src/object_runtime/actor/derived/selector10_audio.rs)
owns `00424BC5..00424C32` only after normal class10 Motion. Its actual resource
WORD `+30` must be 33, 34 or 106 and fresh signed C4 must exceed 1. Other values
return before H9, XYZ, sound `+168` or scalar `+16C/+170` access. This is the
native suffix's resource test, not placement or model admission to the tail.
It captures `+168` before publishing both full DWORDs `+16C/+170 = 0xAAAA`.
These carries remain unknown at construction until a reached native writer.

A present record updates through shared `004800B0`, with lazy XYZ before
gain/pitch. A NULL record acquires current H9 slot 20 and constructs a
persistent record with gain/pitch `0xAAAA` and final control zero. Actor `+168`
is published only on normal birth. XYZ failure retains the scalar stores;
a failed birth may retain a linked pending record without an actor reference,
whose unknown flags remain required at list service/cleanup. No replacement
PCM, unknown-field default or hidden cleanup is supplied.

Both arms use the same [record lifetime](#shared-record-lifetime),
[voice owner](#persistent-playback-boundary) and explicit `00425F70` destruction
phase as class36. Class10 does not reuse class36's camera cutoff, smoothing
`+280/+284` or slot 64 selection. Its
[audio cases](../../crates/inf-game/src/object_runtime/actor/derived/selector10_audio/tests.rs)
distinguish lazy resource/status returns, new birth, same-record update and
partial XYZ failures. Those supplied domains do not establish actual intro
reach, audible scene behavior or matched original-game playback.

## Class36 persistent actor sound

[selector36_audio.rs](../../crates/inf-game/src/object_runtime/actor/derived/selector36_audio.rs)
owns complete `00481B10`, after a normal
[class36 motion return](ACTOR_MOVEMENT_TAILS.md#selector36-contact-alignment-and-motion).
Its first operation is the actual `00478E00` camera factory. A genuine NULL
camera bypasses all later pose, control and record reads. Production supplies
the current factory process, residency, terrain, listeners and settings; it
never substitutes the first player or a cached successful render pose.

Signed wrapped WORD actor-minus-camera XYZ feeds a wrapping DWORD square sum.
The unsigned distance cutoff is inclusive `7680²`. Inside that range, signed
wrapping absolute control, clamp 1024 and arithmetic quarter smoothing update
DWORDs `+280/+284` in native order. Fresh `+168` then selects an update or lazy
H9 slot 64 acquisition and record birth. No callback clock or RNG read is added.
Outside the range, a present record is deleted before the normal-only `+168`
NULL store. Errors preserve reached smoothing, list and actor-reference writes.

The shared derived destructor `00425F70` releases targets `+148/+154` and behavior
`+140` before sound `+168`, then releases the actor handle and nodes. One
[ObjectDestructionRequest](../../crates/inf-game/src/object_runtime/sound_destruction.rs)
carries the object and explicit sound teardown service. Missing service is
inspected only at reached nonNULL `+168`. Actual load, retirement and Leave
callers lend the same session process sound owner; constructor/route cleanup
uses explicit `RequiredAtReach` and bypasses it for NULL sound.

## Persistent playback boundary

[voices.rs](../../crates/inf-game/src/process_sound/voices.rs) owns the
ordered 60-byte voice pool independently from the 40-byte record pool.
`PlatformAudio` retains that non-Clone process owner beside the SDL device;
the mixer retains physical buffers keyed by process-qualified voice identity.
One-shot and persistent records share Create/Start/Update through one explicit
request enum and the existing synchronous teardown loan. Loading, Results,
Leave and actor deletion lend that same actual audio owner. A missing loan is
required only at a reached nonNULL voice; it never means native disabled.

`004F4230` checks enabled and sweeps before inspecting the source, including
every alias recursion. PCM preparation precedes fallible 60-byte allocation;
the count-at-least-24 check follows allocation. Duplicate, frequency, pan and
gain operations precede linkage and output publication. Failed portable
cleanup retains an explicitly unlinked allocation and unreused identity,
without inventing unwritten native fields. Same-allocation source mappings
remain attached to their retained original allocation. The original corpus's
19 aliases stay within their own allocation; that static membership does not
prove the native loader's pointer-fixup/refcount lifecycle. Cross-allocation
aliases remain a typed custody boundary. Original slot64 is direct PCM from
`1.ovl`, Handler9 local34: 2,438 bytes / 1,219 signed samples.

Create publishes record `+18` before Start. Start's signed counter decrement
and sign crossing precede Restore/Play, so a failure retains the published
voice and counter for later Update. Persistent Restore/Play errors propagate.
The one-shot wrapper marks automatic release first, consumes native Restore
failure without Play, discards native Play failure and consumes capacity
`0x2202`; portable missing-service faults remain distinct. Update has no enabled
gate and publishes all parameters before frequency/pan/gain, preserving the
playback cursor. Delete checks enabled before its pointer, publishes counter
zero, consumes native Stop errors, then releases, unlinks and frees normally.
Release has no native HRESULT domain. Physical completion marks a one-shot
finished; only the explicit sweep retires its logical voice. Sweep captures
NEXT, skips persistent status checks, removes finished/error one-shots and,
at capacity, evicts the first remaining automatic voice.

`004F4780` publishes SoundFX master volume before visiting every linked voice
in order, including persistent and automatic voices. A later failure keeps
the global and earlier gains. Mixing uses each voice's committed attenuation,
so starting another cue does not silently change existing voices. Looping PCM
keeps its cursor through pitch, pan and gain changes. Allocation-qualified weak
cache keys share decoded samples without keeping released H9 banks alive.
Interpolation, resampling, floating summation and retained CPU buffers remain
SDL presentation policies rather than sample-identical DirectSound behavior.

Native sound startup and release now use the shared
[resource lifecycle](../../crates/inf-game/src/process_sound/lifecycle.rs)
and [device owner](../../crates/inf-game/src/process_sound/device.rs).
H9 construction `00474930` acquires `0050F740`/refcount `00545478`, whose
dependency `00510350` acquires `00545550` and initializer `00510250`.
Enabled, device identity and configuration remain independent fields.
Successful DirectSoundCreate publishes configuration zero then enabled
`0065F248 = 1`; failure consumes its error without changing prior enabled or
configuration. Teardown `00510330` clears the pointer while retaining enabled.
The former `SdlSuccessfulDeviceStartupApproximation` is removed.

The [production coordinator](../../crates/inf-game/src/platform_audio/lifecycle.rs)
opens and validates the actual SDL transport at the device callback. It next
loans the actual session record pool (`0053BB88`, table `00527DA8`) before the
application's direct voice reference (`00544CF0`). This preserves application
table `005220F0`: `00488000` reaches `0047FF40` through table `00528720` before
`004F4210` at `00522130`. The first voice acquisition initializes its list;
the second retains it. Shutdown releases the application voice reference,
clears the record list with `0047FF20` while voice services remain available,
then force-cleans remaining voices and releases H9/device. A foreign record
pool is rejected before release. A failed portable service retains its reached
prefix and prevents replay of that coordinator's teardown.

The graph preserves table-order acquisition, reverse release, nesting and
native error unwind. Allocator/cleanup dependencies retain their own counts;
`00494840`'s empty cleanup stack and CRT setjmp are represented by scoped Rust
error propagation, without copying native jump-buffer bytes. Unrelated
application dependencies remain with their subsystem owners. The native FX
device is a lease on the shared SDL mixer: releasing it preserves the separate
CD/title transport until the platform owner drops. SDL failures remain typed
portable service errors, rather than invented DirectSound HRESULT objects.
On a new raw PCM buffer, `00510450` checks configuration zero before its
normal shared-device setup. Success publishes configuration one before buffer
allocation; a setup failure leaves zero. The SDL source-preparation seam
preserves that order through explicit device validation; cached PCM does not
reconfigure the device. Shutdown retains this byte along with enabled.

[22 native lifecycle cases](../../crates/inf-game/tests/fixtures/sound_lifecycle_native.json)
authenticate unmodified `0048EE20/0048EEC0` and five original resource tables
with supplied device/list/cleanup bodies, including unwind and stopped prefixes.
[18 device cases](../../crates/inf-game/tests/fixtures/sound_device_native.json)
authenticate complete `00510250/00510330` with supplied platform/error services.
Record-table order guards and the actual SDL integration test separately cover
the sixth resource, record identity and real silent-buffer cleanup. These are
resource-lifetime evidence, not matched retail audio acceptance.

Reproduce those receipts with Python 3 and Unicorn 2 using the tracked
lifecycle generator and
device generator. Each accepts `--exe` and
`--output`; defaults select the private installed executable and its checked
fixture. For an investigative comparison, keep the output under ignored `.tmp`:

```text
python check_sound_lifecycle.py --output .tmp/sound-lifecycle.json
python check_sound_device.py --output .tmp/sound-device.json
```

The optional session recorder taps
the actual final float32 stereo mixer buffer, after mixing and clipping, with
callback-entry timestamps from the same monotonic clock as SDL presentations.
Its bounded nonblocking queue reports overflow or writer failure as an incomplete
capture. This observes supplied PCM without reconstructing sound from events;
physical output buffering and speaker latency remain unmeasured. The lossless
mux and explicitly labeled review derivative support the human-judged comparison,
not automatic audio acceptance.

[Voice semantic/native guards](../../crates/inf-game/src/process_sound/voices/tests.rs),
[record guards](../../crates/inf-game/src/process_sound/records/native_guards.rs)
and [actual PCM cases](../../crates/inf-game/src/platform_audio_tests.rs) pin
ordered failure prefixes, shared admission, retained allocations, audible
looping samples and synchronous teardown. The paired observer normalizes only
opaque `sound_process` and `sound_voice_pool` identities, preserving aliases,
serials and unrelated numeric state. Its parity establishes diagnostic
noninterference. NonNULL record `+1C` modulation, native H9 pointer fixups,
cross-allocation relocation, matched retail audio and complete intro acceptance
remain open.
