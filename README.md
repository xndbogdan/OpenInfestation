# OpenInfestation

OpenInfestation is a Rust reimplementation of **Infestation** (Frontier
Developments, 2000), built from reverse engineering of the original
executable and data. It is not a matching decompilation: it reads the
original game's files and plays them with its own code.

Infestation is the continuation of V2000: its source tree was internally
named `V3000`, and it runs on the C++ port of V2000's engine. The V2000
reimplementation is [OpenV2K](https://github.com/xndbogdan/OpenV2K). This
port deliberately does not reuse V2000's OVL or model semantics: the games
have different `PreLoad.dat` grammars, handler framing and render-command
streams.

**You need a retail copy of Infestation.** No game data is included or
downloaded. Put the port's executable beside the installed game
(`Infest.exe`, `PreLoad.dat`, `Overlay/`), or pick the installation in the
launcher.

- **Renderers.** The software raster and OpenGL are retail-exact: they look,
  sound and play as close to the original as possible. Vulkan is the
  remaster: path tracing and the port's new looks and sounds live there
  only, in single player.
- **Fixes for retail bugs** are separate options, on by default; each can be
  turned off in the launcher's Options or by its command-line flag (see
  [the fixes](#runtime-bootstrap) below).
- **Network play is not working yet.** The menus and protocol are there,
  but a network game does not play correctly.
- Optional upscalers (AMD FSR, Intel XeSS, NVIDIA DLSS) load from their
  vendors' libraries placed beside the executable; none ship here
  ([`scripts/build-upscalers.ps1`](scripts/build-upscalers.ps1)).
- Remastered ambience comes from `Remastered.pak`, which you build from
  your own sound files ([`ambience/`](ambience/README.md)).

## Building

```sh
cargo build --release
cargo test -p inf-game --lib --release   # needs the game in retail/
cargo run --release -- --data-root retail
```

Windows builds use the `x86_64-pc-windows-gnu` toolchain with MinGW-w64 GCC
on `PATH` (see [`AGENTS.md`](AGENTS.md)). Linux release builds bundle SDL2:
`cargo build --release -p inf-game --features sdl2/bundled,sdl2/static-link`.
Most tests read a retail installation copied into the ignored
[`retail/`](retail/README.md) directory.

Reverse-engineering notes are in [`docs/re/`](docs/re/); progress against
the retail executable's functions is below.

<!-- progress:start -->
[![RE coverage](https://img.shields.io/badge/RE%20coverage-100.0%25%20code%20%2F%20100.0%25%20notes-orange)](docs/progress/README.md)

Of the **2,916** game functions in retail `Infest.exe` the port
covers (865 not needed are [omitted](docs/progress/README.md#omitted)),
**2,916 (100.0%)** are referenced by the port's code and
**2,916 (100.0%)** by its code or RE notes.
[How this is measured](docs/progress/README.md).

[![Map of the retail functions by RE note and state](docs/progress/treemap.svg)](docs/progress/README.md#map)
<!-- progress:end -->

The implementation is evidence-first. `inf-game` owns runtime preparation;
`inf-formats` follows the retail loader and renderer; `inf-extract` preserves
exact bytes and registration provenance; and `inf-viewer` reads `PreLoad.dat`
and OVLs directly. Historical Python investigators are not format authorities.

## Runtime bootstrap

The default workspace member is `inf-game`. Copy the release binary next
to the installed game (`PreLoad.dat`, `Overlay/`, `Infest.exe`) and
double-click it: it opens the [pre-game launcher](#pre-game-launcher), and
Play starts the frontend menu. The executable carries the port's own icon,
a generated rust black hole (its recipe), not
retail's. An explicit
`--data-root` always wins; otherwise the executable's directory is used when
it holds `PreLoad.dat`, then for a desktop start the launcher's remembered
installation, else `retail/` covers `cargo run` from the repository root. The
retail files (`Infest.exe`, `PreLoad.dat`, `Overlay/` and its overlays,
`Title.avi`, `cdaudio/`) are found in any ASCII case, as Windows finds
them, so a copy on a case-sensitive filesystem may hold `preload.dat` or
`OVERLAY/`; the port's own files keep the names it gives them. The
resource-inspection modes need an explicit `--mode`:

```sh
cargo run --release -- --data-root retail
cargo run --release -- --data-root retail --renderer software --hidden --frames 600 --fixed-step-micros 16000 --snapshot frontend.png
cargo run --release -- --data-root retail --mode independent --scene 0
cargo run --release -- --data-root retail --mode independent --scene 47 --json
cargo run --release -- --data-root retail --mode independent --scene 11 --difficulty 0 --language 0
cargo run --release -- --data-root retail --mode resident
cargo run --release -- --data-root retail --mode resident --activate 47,48,11 --json
cargo run --release -- --data-root retail --mode resident --activate 11 --inspect-world 0 --json
```

### Pre-game launcher

The executable is a Windows GUI program, so a double-click opens no console
window. Started from a terminal, it writes to that terminal, and redirected
output still reaches its pipe or file. PowerShell does not wait for a GUI
program; pipe its output (`| Out-Host`) to wait. `cargo run` waits as
before.

A desktop start opens the launcher first. A desktop start has no flags, or
only `--data-root`, the display, scaling, renderer and fix choices, or
`--session-log`. Direct runs skip it: inspection modes, `--settings`,
`--profiles`, `--input-replay`, `--frames`, `--snapshot`, `--snapshot-at`, `--record-session`,
`--hidden`, `--fixed-step-micros`, `--mission`, `--start`, `--event-log`
and `--replay-session`, so tests and harness runs never wait on a window.
A `--hidden` run is silent: it mixes its sound as usual (the log's audio
stats count it) but sends the speakers nothing.
`--launcher` opens it anyway, and `--no-launcher` starts as a direct run.
`--event-log`, `--snapshot-at FRAME:PATH` (repeatable: the frame presented
as the frame count reaches FRAME, saved as `--snapshot` saves the last),
`--replay-controls`, `--replay-rng` and `--write-start-save`
serve the retail scenario lab's comparisons (a private harness that runs
retail `Infest.exe` headless), as do the start
block's `rng`, `clock_us`, `briefing_frames` and `vehicle`.

Its banner is an animated header, as OpenV2K's is: new INFESTATION lettering
over a thin line, against the frontend's own starfield and a still nebula.
Every 3.84 s a shine runs along the line from left to right, in the menu
logo's halo colour, crossing in one of its pings (64 ticks of the
frontend's 20 ms clock). It is a baked flipbook compiled into the
executable (the artwork and its recipe).

The launcher shows the installation and checks it on a worker thread:
`Infest.exe`'s tables load; `PreLoad.dat` loads and every overlay its scenes
request, over all display tiers and languages, is present and loads as a
scene loads it (257 of the 1932 files on disk carry requests); the saved
settings load. Without these, Play stays off. The checksums only warn: an
`Infest.exe` other than the retail build, or a data file whose size and
SHA-256 no known release has ([`retail-checksums.json`](crates/inf-game/src/retail-checksums.json)),
may be another build that plays fine, so Play stays on. A missing title
video or music only warns too, as the game runs without them. Change picks
another folder. Play remembers the installation in
`%LOCALAPPDATA%\Infestation Port\launcher.json` (`--launcher-state PATH`
overrides the file).

A settings file the game cannot load (one written by a newer build, or
edited by hand) is the one thing the launcher fixes itself. With the game
files ready, its line says the file is malformed, and Play and Options give
way to one button. **Repair settings** keeps what this build reads: fields
it does not know are dropped (inside the fixes and the Vulkan settings
too), a newer schema is read as this one, a value past its setting's range
(Camera Style 10, say) takes its default, and key bindings it cannot use
are the originals again; the details list what it changes. When nothing can
be read (not JSON at all), the button is **Reset settings**: the game
starts from the original settings. Either keeps the old file as
`settings.json.broken`. The game itself never changes a file it cannot
load; it stops with the error.

Options writes the game's own settings, `settings.json` beside the
executable (`--settings PATH` chooses another file). It holds the Display and Resolution
(the primary monitor's modes, listed as the Video menu lists them), the
renderer (schema 9 adds it), the image scaling, the effects and music
volumes, and the eight fixes for retail bugs. Switching its Renderer to
Vulkan selects path tracing with Remaster lighting (as a fresh install
does), plus SMAA anti-aliasing and Anisotropic 8x filtering unless Vulkan
levels are already chosen; a saved Off stays saved. The command-line flags still
override them for one run. "Skip the launcher while this installation is
ready" starts the game directly; a missing or damaged file opens the
launcher again.

On Linux the launcher is the same window drawn with egui over OpenGL (X11
or Wayland), with the same rows, rules, banner and `launcher.json` (here
`$XDG_CONFIG_HOME/infestation-port/launcher.json`). Change opens the
desktop's folder chooser through the xdg portal; without a portal, type the
folder into the path field and press Enter. Notices show inside the window,
and a start error shows in an SDL message box. With no display to open it
on, the launcher checks the installation and starts the game when it is
ready. `INF_LAUNCHER_SNAPSHOT=PATH` saves a PNG of the window once the
check finishes (`INF_LAUNCHER_SNAPSHOT_PAGE=options` shows Options first).

With no `--mode` flag the binary opens the game (`frontend`). `--mode
frontend` spells the same explicitly: it boots through the real title
screen (playing transcoded `Title.avi` frames when `video/` is present, else
finishing to phase 2 without video), then opens the authored main menu, animated scene and footer in
an SDL window, with original music and navigation cues. Transcode once so the
video shows on a normal launch:

```powershell
.\scripts\ensure-title-video.ps1
```

Arrow keys navigate,
Return confirms, and Escape opens Back/Quit; both Quit answers work. Options
Table opens with its camera transition. Its value, player-identity and Sound
submenus draw live settings, and their adjust rows change them, including
live effects/music volume. Video opens the authored Display submenu: resolution,
fullscreen, output and bilinear filtering apply immediately while the screen
and audio continue. Rendering lists up to four SDL output displays in place of
retail DirectDraw adapters. Escape runs the native Back command and persists
settings; the authored Back row only returns to Options. Selecting an output
also saves its name immediately. Colour Depth preserves the native stored value
and reset behavior, but the recovered display constructor does not use it.
Gameplay-only rows store their values for the remaining world implementation.
Options Defaults, identity-row phases, network menu and missions remain under
development and report explicit unsupported boundaries. Idle demo playback after
60 seconds currently uses the labelled
[AttractDemoDeferredApproximation](docs/re/ATTRACT_DEMO_RUNTIME.md#attractdemodeferredapproximation):
the menu stays usable while demo preparation and playback remain unimplemented.
Start Game opens the profile menu;
choosing an empty slot opens name entry. Type a name, use Backspace to edit, or
Escape to return. Completing entry loads a world with the fresh-heap policy
(zero-fill WORDs, explicit approximation). The native New-Game sequence uses
mission selection 22 for the in-engine intro, then selection 18 for the
level-1 briefing and gameplay. The briefing now parses and plays the selected
H14 records, draws the original background, panel frames, supported model previews,
progress sprites and text/menu rows, and waits for Start's native
timer and text overlay; the empty intro briefing still finishes in its first
tick. Four of the seven Level1 preview paths retain explicit execution gaps.
The decorative globe draws original Far effect packets through their recovered pixel consumer.
The map backdrop follows the recovered briefing screen-camera movement and
native clip gates, with ordinary actor markers followed by independent numeric
labels and radius/RGB annotations after each wrapped tile. The annotations
use the loaded mission's selector-87 list and original font/style/sprite resources;
see [briefing annotations](docs/re/BRIEFING_RUNTIME.md#independent-map-annotations).
World and briefing ground share native terrain submission and pixel consumers.
Nonzero jitter's first stack input, water/furniture and terrain-emitter drawing,
briefing's static-light approximation and lightning remain logged boundaries.
The intro controller, route-helper movement and native camera pose are connected
to production tick/draw, using a named approximation only for the initial
allocator deadline. Referenced actor tasks and remaining retirement services remain open;
full moving cinematic acceptance is still pending. Escape follows its native completion
request through a bounded results phase to the visible Level 1 briefing;
choose Start Mission there to enter gameplay. The results and campaign reload
use explicitly named approximations; the moving cinematic remains incomplete.
Automatic completion also reaches that briefing through shared actor retirement;
the [repeatable intro smoke](docs/re/INTRO_SEQUENCE.md#verification-and-acceptance-boundary)
uses tracked input and isolated settings/profile paths.
The playable world renders arcade perspective terrain with the vehicle and
actors; arrows/WASD drive with a chase camera (native physics, bindings and
camera springs stay follow-ups). These bounded owners do not establish full
retail New-Game acceptance. Writing the new profile to disk remains
unimplemented. See [briefing runtime](docs/re/BRIEFING_RUNTIME.md).

The Video menu's Display row offers In a Window, Full Screen (a real mode
change, undone on exit or alt-tab) and Borderless (the desktop without a mode
change). Resolution offers the original 512x384 to 1024x768 sizes plus every
mode the chosen monitor reports, up to its largest, in physical pixels: a
window lists the sizes that fit the desktop, full screen only reported modes,
and Borderless shows the desktop's size, which can't be changed (the saved
resolution returns with the next mode). Switching the Display row to Full Screen
picks the monitor's highest reported mode; step the Resolution row down from there. A size other than the originals draws
the largest original inside it and enlarges the frame; OpenGL draws at the
output's full size, the CPU raster enlarges its native frame. Without an
explicit size or variant, startup searches from the saved display like retail
and applies saved filtering and scaling too. A fresh install starts Borderless
over the desktop; retail's authored default is Full Screen at 640x480.

`--width`/`--height` replace the saved resolution with an exact size of at
least 320x240 and keep the saved Display; `--difficulty` selects a tier's
original size, and a conflicting explicit pair is rejected. The
settings are stored at `settings.json` beside the executable, overridable with
`--settings PATH`. Saving writes retail's own `Infest.sav` beside the game's
data, so retail and the port share one save list; the Windows registry is
untouched. Music uses the
versioned original-disc files under `cdaudio/`; when that directory (or any
disc track) is absent the game boots silent and keeps running without music,
like retail with no CD in the drive. An available SDL audio device is still
required; there is no silent mute fallback.

`--scaling` chooses how the game fills an output that is not the native size:

```sh
cargo run --release -- --data-root retail --width 1920 --height 1080 --scaling widescreen
cargo run --release -- --data-root retail --width 1280 --height 720 --scaling four-three
cargo run --release -- --data-root retail --width 1920 --height 1080 --scaling stretched
cargo run --release -- --data-root retail --width 1920 --height 1080 --scaling integer
```

`widescreen`, the default, draws a wider frame at the output's aspect, so
the world shows more to each side and the frame fills the screen; the
language carousel and the briefing stay a centred 4:3 picture. `four-three`
fits the complete 4:3 frame with black bars. `stretched` fills the
window; `integer` uses the largest whole-number enlargement that fits, falling
back to fractional fit when the native frame is larger than the output. Final
scaling is point sampled and applies to title, frontend, name entry, intro,
briefing and gameplay through their shared presentation callback. The
resolution, Borderless and scaling live in the settings sidecar beside the
native settings words; an older sidecar's 720p or 1080p output loads as that
exact resolution; an older sidecar's `four-three`, then the default, loads
as `widescreen`. PNG exports include the output size and bars. See
[presentation policy](docs/re/PRESENTATION_POLICY.md) for the display
model, widescreen and the V2000 comparison.

The port fixes these retail bugs by default, and saves each choice in the
settings sidecar's `fixes`. Above about 60 frames a second, retail's ACAM
can hang its hull above the ground and stop driving, because its drive
drops each tick's fraction of a unit of movement; the port carries that
fraction instead. `--vehicle-position-carry false` restores retail's
behavior, and a patcher writes a retail executable with the same fix; see
[the high frame-rate hang](docs/re/VEHICLE_DRIVE_RUNTIME.md#the-high-frame-rate-hang).
While the ACAM turns into or out of the hovercraft with the plasmas or
another side-mounted weapon, retail's model data joins the right mount to
the left side of the hull and a black band crosses the roof; the port
joins it to the right. `--vehicle-gun-mounts false` restores retail's
mounts; see
[the black band over the roof](docs/re/VEHICLE_DRIVE_RUNTIME.md#the-black-band-over-the-roof).
Once the ACAM has turned into the hovercraft, retail's model data mirrors
the basis of its two rear fans, so they draw inside out, as black blobs;
the port gives them the basis they have while transforming.
`--hovercraft-fans false` restores retail's fans; see
[the hovercraft's rear fans](docs/re/VEHICLE_DRIVE_RUNTIME.md#the-hovercrafts-rear-fans).
Retail steps the world by each frame's delta and drops each step's
remainder, so at high frame rates gravity weakens and some timers stop; the
port steps the world in whole 16.384 ms quanta, carrying the rest, so it
ticks at most about 61 times a second whatever the display's rate.
`--exact-world-ticks false` restores retail's step, and
`--exact-frontend-ticks false` does the same for the name entry's glyph
effects and the language carousel; see
[high frame rates](docs/re/GAMEPLAY_RUNTIME.md#high-frame-rates).
Retail's computer database draws only the current world's models, so an
entry from another world shows its text alone; the port loads the model
from a world that has it. `--database-models false` restores retail's
draw; see
[the computer and the in-world menu](docs/re/PLAYER_MESSAGES.md#the-computer-and-the-in-world-menu).
The ground forms carry the guided missiles in a roof launcher and the other
forms in side launchers; retail swaps one for the other at once when the
ACAM changes between them. The port folds the old launcher away as the old
form morphs out and brings the new ones out as the new form morphs in,
changing only the drawing. `--guided-mounts false` restores retail's swap;
see
[the guided missiles' mounts](docs/re/VEHICLE_DRIVE_RUNTIME.md#the-guided-missiles-mounts).
Finishing the campaign gives every later game the Cranial Implanter at each
respawn, but retail keeps that bonus only as a flag in its settings, so
finished saves carried to another installation, or a settings reset, lose
it for new games. While any save in the list has finished the campaign,
the port gives every game the bonus, and the console counts the finished
saves at startup. `--completion-bonus false` restores retail's rule; see
[the completion bonus](docs/re/PROFILE_RUNTIME.md#the-completion-bonus).
Retail updates its scientists and alien ships every frame, and how they
move depends on the frame rate: at 60 frames a second a scientist's push
never beats the threshold that clears its speed, so it stands where at 30
it walks and runs, and the small alien ships fly slower. The port updates
them every other 16.384 ms world tick with both ticks' time, as retail did
at 30 frames a second, while the rest of the world keeps its own step.
`--paced-actors false` restores retail's update every tick; see
[the scientist's gait](docs/re/ACTOR_MOVEMENT_TAILS.md#the-scientists-gait-and-the-frame-rate).
Retail plays at most 24 sounds at once, and when they are all playing, each
new sound silences the oldest one-shot: in a busy fight a generator's
explosion stops halfway. Most of them are water splashes: hovering over
water keeps about 20 sounding. The port keeps retail's 24 but lets at most
12 of them be splashes, which changes only what is heard.
`--splash-voice-cap false` restores retail's uncapped splashes; see
[the voice limit](docs/re/AUDIO_RUNTIME.md#process-positional-one-shots).
Retail completes Pillier Stren's rescue, its message included, only as the
transport leaves, and it leaves only once the player is within 2500 of it:
a player who drives off after the drop-off waits for the message. The port
completes it as the last scientist boards, on every renderer, and keeps
retail's rule in a network game. `--mission-completion false` restores
retail's; see
[the rescue](docs/re/SELECTOR35_RUNTIME.md#rescue-and-go-to-space-descriptors-10-and-11).

One port option is off by default. The computer's Cheats Menu (Tab, Cheats
Menu) shows only the rows whose name cheats a profile has entered, as in
retail; `--all-cheats true`, or the launcher's Options under Cheats, shows
all eight without them and leaves the saved cheat word as it is; see
[the Cheats Menu](docs/re/PLAYER_MESSAGES.md#the-cheats-menu).

The renderer is the device that draws each frame. The launcher's Options
save it and `--renderer` replaces it for a run. The default `auto` uses
OpenGL and falls back to the CPU raster when OpenGL cannot start; `opengl`
fails instead of falling back. `vulkan` draws with Vulkan, falling back to
OpenGL and then the CPU raster; it carries the
[upgraded look](docs/INFESTATION-RT-IMPLEMENTATION.md) as it is built.
`software` is the CPU raster, the pixel reference: it gives identical frames
on every machine, so use it for snapshots, receipts and any decompilation or
retail comparison. All three draw the same clipped Direct3D submissions; see
[the OpenGL device](docs/re/PIXEL_RUNTIME.md#opengl-device) and
[the Vulkan device](docs/re/PIXEL_RUNTIME.md#vulkan-device).

Under Vulkan four more settings shape the traced world. They are in the
launcher's Options (its Vulkan group) and in the Video menu, after
retail's rows (the list scrolls), and they take effect from the next frame:

- Ray Tracing: Off draws the world as the raster does, Path Tracing traces
  it.
- Upscaler: Off, FSR, DLSS, DLSS RR (Ray Reconstruction, which also
  denoises the bounce light) or XeSS, rendering smaller and bringing the
  image up to the display.
- Upscaling: the upscaler's mode, Native AA, Quality, Balanced, Performance
  or Ultra Performance.
- Anti-aliasing: Off, FXAA or TAA, over the traced world before the
  raster's overlays and the HUD. While an upscaler runs, the row shows the
  upscaler instead and does not move: each upscaler brings its own.

They live in the settings sidecar's `modern` (schema 11). Each upscaler is
an optional library beside the executable:

- FSR 3.1 is AMD's `amd_fidelityfx_vk.dll` (FidelityFX SDK 1.1.4, MIT).
- XeSS is Intel's `libxess.dll` (XeSS SDK, Intel Simplified Software
  License, shipped unmodified).
- DLSS needs `inf_ngx.dll`, the port's bridge to NVIDIA's NGX, plus NVIDIA's
  `nvngx_dlss.dll` and `nvngx_dlssd.dll`. NGX's Vulkan API is an MSVC static
  library while the port builds with the GNU toolchain, so the bridge is
  built separately, with Visual Studio's C++ compiler and the DLSS SDK.

`scripts/build-upscalers.ps1` builds the bridge and copies all of them
beside the executables, skipping any whose SDK is missing. A missing library
makes its upscaler unavailable, and the game logs that once.
The Upscaler rows list only what the GPU runs: DLSS and DLSS RR on an
NVIDIA GPU whose driver offers them, XeSS where its runtime starts, FSR
wherever its library loads. A saved upscaler the GPU lacks (a settings file
from another PC) runs as FSR, the log says so, and the saved choice is kept.

The traced world's look takes its values from a tuning file. For the fog
it holds the mist's density, height, light and glow, how much of it the
painted sky takes, and how far the haze lets distant ground show. For the
sky it holds a modern sky to try in place of the painted one, off by
default. For the
water it holds how strongly splashes ring the surface and foam. For the
models it holds which are metal and how rough, by name. For thermal
vision (infrared under Vulkan) it holds each surface's heat,
models matched by name, and the camera's resolution, grain and halo. The
defaults are built in (`crates/inf-game/src/modern_tuning.json`). A
`modern-tuning.json` beside the settings file overrides any of them, as
`{"fog": {"density": 2.0}}`, and `{"worlds": {"9": {"fog": {...}}}}`
overrides them for one world. The game reads the file again within a second
of it changing, so values can be tuned while playing. A file that does not
read keeps the values before it, and the log says why.

Fixed-step clocks and PNG exports support repeatable visual checks.
`--input-replay PATH` accepts an ordered JSON array of physical notifications,
for example `[{"frame":500,"key":"Escape","pressed":true},
{"frame":501,"key":"Escape","pressed":false}]`. Frame numbers are one-based;
key names are SDL scancodes. Replay still uses the original bindings and queue.
Run the real-window/audio Quit and live Video reset checks explicitly with
`cargo test --release -p inf-game --test frontend_app_window -- --ignored`.

### Start harness

`--start [STATE.json]` skips the title, the menus and the briefing and
plays one world with an injected game state, loaded the way a saved game is
(a fresh player, then the state where the profile's would be restored), so
the respawn builds the vehicle from it. Without a file it is a new game's
player at Level 1's first start. Every field is optional:

```json
{
  "world": 5, "start": 0,
  "position": [24000, 45390], "heading": 90,
  "elapsed_seconds": 600, "world_status": {"18": 6},
  "upgrades": [3, 50], "known": "all", "hints_seen": "none",
  "tokens": 12, "crystals": [30, 30, 30], "turbo": 3,
  "energy": [1, 0, 0, 0, 0, 0, 2], "acams_lost": 0, "rank": 0,
  "form": 1, "primary": 0, "secondary": 8,
  "ammo": {"8": 20}, "hold": [0, 0, 0]
}
```

`world` is the mission world (Level 1 is 18; `inf-extract worlds` lists the
placements), `start` its start index, and `position`/`heading` put the first
spawn at an exact X/Z (on the ground) and facing in degrees; a later respawn
uses the world's start. `upgrades` are grant IDs owned, `known` grant IDs
known (blueprints and database entries, closed over the grant table),
`hints_seen` hint IDs, `crystals` the blue, green and red tanks, `energy` the
Energy Distribution levels, `form` the ACAM's form (1 buggy .. 5 skimmer),
`primary`/`secondary` the selected weapons, `ammo` each weapon's count (-1
unlimited) and `hold` each carried actor's resource. Lists also take "all"
or "none". [`harness_start`](crates/inf-game/src/harness_start.rs) maps
each field to its saved slot ([the save format](docs/re/SAVE_FORMAT.md#what-each-field-means)).
Use scratch `--settings` and `--profiles` copies: a results screen saves
into the real `Infest.sav`.

### Session logs for bug reports

Start the game with `--session-log` (optionally `--session-log DIR`) to
record a session. Everything the game prints goes to
`<data-root>/logs/run-<local time>.log` (or `DIR`), each line stamped with
the time since launch, while the console still shows it. The log starts with
the executable, command line, data root and platform. It adds each phase
change, every key press and release, and a status line every 600 frames
(frame, clock, frame rate, phase, and in a world its number, mission,
mission seconds, RNG word and camera eye). A failure ends with the error's
full detail and causes; a panic with its message, source location and stack.

Beside the log the run keeps `run-<time>.replay`, the session's clock
samples and keys in the order the game read them, and copies of the
settings and profiles it started from. Run

```sh
inf-game --replay-session logs/run-<time>.replay [--renderer software] [--hidden] [--session-log]
```

to replay it: the recorded command line, clock and keys, over working copies
of those settings and profiles (`.replay-settings.json`,
`.replay-profiles.sav`), until the recording ends. A replay reproduces the
session exactly (the status lines, RNG word included, match), whichever
renderer either run used, provided the title video ended by a key press or
was absent: the video's own timer and the CD music clock (which only paces
music) are host clocks the recording does not hold. Native code that writes
through the C runtime's own descriptors (a few SDL messages) is not captured
in the log.

Every desktop start keeps its replay too, without `--session-log`:
`<data-root>/logs/replays/run-<time>.replay` and the same copies, for the
newest ten runs. A crash log names its run's replay. Add `--resume` to
`--replay-session` to play on where the recording ends: after the replayed
frames the keys and the clock are live again, so a session that crashed can
be continued in a build that fixes the crash. Saves made after that go to
the replay's copy of the save (`.replay-profiles.sav`); copy it over
`Infest.sav` to keep them. A build that changes what happened
before the crash replays a different game from that point on.

### The crash log

Whenever the game stops on a failure, with or without a session log, it
writes a crash log in its log folder (`<data-root>/logs`, or beside the
session log), named for the local time of the crash:
`infest-crash-log-20261017-193715.txt`. Every crash keeps its own file, so
earlier ones stay for later investigation; a second crash within the same
second adds `-2`. The failures are a panic on any thread, on Windows an
unhandled native exception such as a driver's access violation, and an
error that ends a desktop start or a session-logged run (startup errors
included). A direct run's error only goes to its console. The log says what
failed (a panic's message, place and backtrace, an error's causes, or the
exception code and address). It then describes the run (time, executable,
command line, data root, platform, session log, and the replay a desktop
start keeps) and the game as it was at
the last world tick: the phase, the world and its name, the time into the
mission, and the player's unit (its position and health, and a vehicle's
form and its primary and secondary weapon rows). It ends with the last 300
lines the game printed. The game still prints everything to the console. A
desktop start's error box names the crash log.

The game also writes it when it gets stuck without crashing. A watchdog
thread logs a frame that has not finished after 15 s: the log names what
the frame was doing (the input, a world load, the world tick, an actor's
update, the contact passes, the furniture effects, the world draw or an
object's draw) with the object's arena slot and how long it has been there,
and a desktop start shows where the log is. If the frame then finishes, the
game carries on and prints how long it took. And the first time in a run the
player's own update stops (the vehicle, or a held camera, missile or
creature, stopped after a failure), it logs that failure: the game keeps
running, but nothing answers the controls.

`--mode independent` and `--mode resident` prepare scene resources and
print a headless report. `--scene` is the OVL scene
ID, not the session phase or campaign mission number. Omitted difficulty and
language use the `PreLoad.dat` header. Scene 0 contains startup resources,
47 contains frontend resources, and 11 is the first mission resource bank.
These independent-scene receipts do not include parent resources. Resident
mode executes the authored Group/Select tree: boot activates leaves 0 and 2;
requesting 47 replaces 2 and retains 0. The `47,48,11` sequence then releases
the frontend, enters the empty loading leaf 48, and selects the first world
with its shared banks. `--activate` accepts a comma-separated sequence and can
be repeated. The JSON report includes ordered transition events and the final
resident inventory; inventory order alone does not establish registration order.

`--inspect-world 0` additionally reads the resident H14 slot, its ordered
instance commands, resolved H13 constructor inputs, and initial H8 models.
It preserves missing references and uses actual registration winners, including
shared/family banks. It executes no constructors or instance commands. See
[world inspection](docs/re/WORLD_INSPECTION.md) for schema, provenance
and the two initial model references absent in their selected retail worlds.

The library's [`WorldPlacements`](crates/inf-game/src/world_placement.rs)
additionally prepares runtime terrain diagonals and samples native heights for
each ordered constructor input, using an explicit current terrain spacing.
It preserves raw coordinates and source assets, with WORD-wrapped height added
only to the constructor's Y input. The world loader and bounded constructor
owners use this foundation. World and briefing ground now use the same native
terrain bridge; selector 87 supplies independent map annotation state and
ordered list custody. Complete world composition remains bounded by the
unowned drawing and host features described above. See
[terrain runtime](docs/re/TERRAIN_RUNTIME.md) for policy and native checks.

[`TerrainMap`](crates/inf-game/src/terrain_map.rs) builds the initial 256-square
palette-index map from that prepared terrain, the world's plane policy and the
caller's shared RNG state. It preserves native traversal and RNG consumption;
the recorded first-world map matches byte for byte. Map rendering and later
updates remain separate owners.

The loader reads only phases with active requests, in base, difficulty,
language, then combined order. Required files must exist and parse exactly.
The prepared scene retains source bytes, typed banks where established,
typed Handler-4 fonts, proven Handler-13/14 prefixes, opaque class suffixes,
and the local-to-global registration history with
replacement provenance. Invalid mappings are rejected against the handler
capacities in `PreLoad.dat`. A failed preparation leaves the active prepared
scene intact; success replaces it as a whole. This transaction is a port
safety boundary, not a claim that retail loads or unloads atomically.
The session world loader additionally decodes the selected H14 briefing
program from its retained original allocation.

`ResidentRuntime::apply` accepts `Boot`, `Activate`, and `Select` requests.
Retained banks share their original immutable allocations. Resource release
clears every mapped slot, including a slot overwritten by another retained
bank, and never restores an older registration. Selector changes traverse all
active leaves for difficulty, then language, then combined reloads as needed;
base banks remain allocated. A failed multi-bank request preserves selectors,
tree state, resource slots, and the original allocations. Empty leaf 48 still
participates in native tree ownership even though it has no OVL bytes to load.

No executable, exported JSON, SDL window, or TTD file is required to run this
headless resource foundation. It reads the private installed `PreLoad.dat`
and `Overlay/` files without modifying them. Original pointer relocation,
screen/phase execution, rendering, entity construction, and simulation remain
open. Resource residency does not execute backend cleanup callbacks or resolve
sound aliases to playable backend objects. Source owners and next checks are in
[Runtime foundation](docs/re/RUNTIME_FOUNDATION.md).

The library also exposes native session control in
[`session`](crates/inf-game/src/session.rs) and two-pass microsecond timing in
[`scheduler`](crates/inf-game/src/scheduler.rs). `ScheduledSession` joins input,
phase transition, session tick, and the separate linked-screen callbacks.
It preserves screen reuse and constructor context, first-request-wins phase
arbitration, the 8 ms gate, the 125 ms delta cap, and suspension without clock
reset. Host callbacks are required: no screen, audio, simulation, or rendering
backend is silently replaced with a successful no-op. A callback failure stops
the combined owner because prior effects cannot be rolled back safely.

The frontend library now reads authored records directly from the supported
local `Infest.exe`, preserves shared mutable menu tables, and executes generic
menu navigation, authored row layout/drawing with shared height feedback, camera
springs, animation/timer order, and proven callbacks. `FrontendControl::draw_menu`
joins the camera gate and row camera callbacks to the generic draw owner.
[`frontend_screen`](crates/inf-game/src/frontend_screen.rs) composes native factory
and teardown order, including scene-47 residency, registry/input selection, shared
colors/animation/entry state, and late root-menu construction. Its live dispatch
adapter executes menu lookup, original basis arithmetic, analog/repeat input and
held clearing. The SDL host connects keyboard input, music and cues, portable
settings, the root, Options and Quit menus, the derived profile menu, and the
Options value, identity, Sound and Video rows with their live setting actions.
Display replacement reloads the selected resource tier and preserves screen
age, camera/menu state and audio. The network menu, Options Defaults and
remaining phase effects are still required;
see
[Frontend runtime](docs/re/FRONTEND_RUNTIME.md).

These kernels have independent tests and a real SDL window integration test.
`FrontendScreen::draw_frame` composes native
renderer selection, stars/models, footer, menu, lazy resource checks, and the
narrow native `0x301` recovery path. Borrowed resident resources resolve real
models, sprites, fonts and colors; font metrics drive generic/root label layout
and both native glyph passes. The Handler-8 executor preserves unknown fields,
callback ordering, child transforms, transient vertices, near/far projection,
and native lighting. Native fan/quad/effect handlers resolve materials and emit
defined callback payloads into the concrete deferred draw queue; flushed near
flat triangles use native Direct3D material/shade conversion, reciprocal depth,
and explicit inherited blend/depth state in a bounded CPU target. Native
viewport clipping preserves intersection order, depth and color rounding;
near shaded/UV triangles and ordinary/UV/effect/solid quads have concrete pixel
consumers, native upload and CPU sampling owners. Remaining pixel families and
specialized-menu/session wiring stay explicit boundaries. See
[text runtime](docs/re/TEXT_RUNTIME.md) and
[model runtime](docs/re/MODEL_RUNTIME.md) and
[primitive runtime](docs/re/PRIMITIVE_RUNTIME.md) and
[pixel runtime](docs/re/PIXEL_RUNTIME.md). The shared music
owner preserves CD selection, saved position, volume and retry deadlines against
the SDL transport and decoded CD tracks; see
[Audio runtime](docs/re/AUDIO_RUNTIME.md). The canonical save codec reads
redundant CRC blocks and variable-name mission/player records. `ProfileStore`
imports retail saves into a separate portable file and owns the directory,
working buffer, copy/delete and atomic writes. The SDL session now connects the
profile menu to name entry and Back through the real screen constructors and
destructors. Name-entry state, backdrop, overlay and outer draw use the actual
screen and residency. The outer draw unlocks the surface; presentation and
clearing run in the separate linked callback. One renderer serves the 2→9→2
round trip without a resource rebind. Completion serializes the offline working
save in memory, then stops at the unimplemented phase-3 loading boundary before
screen destruction or profile-file publication. Remaining loading/world and
pixel drawing work is described in
[profile runtime](docs/re/PROFILE_RUNTIME.md) and
[save format](docs/re/SAVE_FORMAT.md).

[`screen_registry`](crates/inf-game/src/screen_registry.rs) supplies concrete
session/screen/context identities, native list order, busy attachment checks,
and backend replacement. [`input`](crates/inf-game/src/input.rs) owns digital and
untimed analog consumers, shared binding sets, selection, the 31-event queue,
held-button timing, and signed device/binding calibration. Nonzero analog timer
periods remain explicitly unsupported. [`screen_input`](crates/inf-game/src/screen_input.rs)
owns held-mask dispatch, analog direction priority, and native repeat timing.
[`frontend_bindings`](crates/inf-game/src/frontend_bindings.rs) composes the
authored tables, base analog references, device sets, and staged teardown;
the caller supplies eligible calibrated devices and registry selection.
Callbacks and clocks are explicit host operations. The SDL keyboard adapter
preserves the authored physical-key bindings, queued notifications, shared
held counts and focus-loss releases. A game controller plays as retail's
joystick: the first one plugged in, numbered as Windows numbers an XInput
pad for retail (left stick to steer and pitch, the triggers for thrust, A
to fire); its buttons are the key configuration's Joystick 1 to 16, and the
Controls menu's Joystick row picks Analogue or Digital
([the joystick](docs/re/INPUT_RUNTIME.md#the-joystick-the-mouse-and-the-input-mode)).
The launcher's Controller layout (or `--controller-layout console`) plays it
as the PlayStation release's pad instead, whose tables the PC executable
carries unused
([the PlayStation release](docs/re/INPUT_RUNTIME.md#the-playstation-release)).
Retail never reads the mouse. Cross-owner
tests cover selection, repeat timing and destruction; the SDL test also exercises
both Quit paths through the actual screen and audio owners.

The format crate also decodes authored frontend menu, model-list, camera, and
digital/analog binding records from bounded file-backed executable `.data` and
`.rdata` sections. These preserve callback
identities and source spans without executing the original code. See
[input ownership](docs/re/INPUT_RUNTIME.md) and
[frontend runtime](docs/re/FRONTEND_RUNTIME.md) for evidence and limits.
The frontend corpus tests require the supported `retail/Infest.exe` in
addition to `PreLoad.dat` and `Overlay/`; those tests verify its PE profile
before mapping the tables.

From the repository root, validate the foundation with:

```powershell
cargo test --workspace --release
.\scripts\progress.ps1 -Check
```

## Canonical extraction

See [asset coverage](docs/re/ASSET_COVERAGE.md) for the complete audit and
remaining interpretation/viewing gaps. Complete OVL framing does not mean
complete model presentation or gameplay-data semantics.

Run from this directory:

```sh
cargo run --release -p inf-extract -- all
```

That command regenerates all 13 supported families beneath
`../extracted/`: installation metadata, sounds, sprites, strings,
missions, fixed-width word resources, materials, fixed record tables, opaque
resources, models, terrain, fonts, and world assets. Each family is staged and atomically replaces
only a directory marked as Rust-managed, so a failed run cannot publish a
half-written family. This is atomic per family, not one transaction spanning
the output root: if a late family fails, fix the cause and rerun `all` because
earlier families may already have been updated.

Useful focused commands are:

```sh
cargo run --release -p inf-extract -- inventory --data-root retail --summary-only
cargo run --release -p inf-extract -- coverage --data-root retail --summary-only
cargo run --release -p inf-extract -- inventory --data-root retail/infestation_demo --summary-only
cargo run --release -p inf-extract -- installation
cargo run --release -p inf-extract -- sprites --shade base
cargo run --release -p inf-extract -- sprites --shade 17 --ovl 0.ovl
cargo run --release -p inf-extract -- models --ovl 1.ovl
cargo run --release -p inf-extract -- terrain --ovl 11.ovl
cargo run --release -p inf-extract -- fonts
cargo run --release -p inf-extract -- worlds
cargo run --release -p inf-extract -- opaque
```

The retail corpus currently proves these visual-family totals:

- Handler 3: 86 OVLs, 9,832 registered logical sprites, 10,697 registered
  physical variants, three loader-ignored orphan variants, 10,613 indexed
  variants, 84 direct RGB555 variants, and 1,150 shared palettes. The exporter
  therefore emits 10,700 lossless PNG/raw-pixel variant pairs and 3,081 unique
  palette files.
- Handler 8: 36 OVL regions, 457 sub-blocks, 5,142 entries, and a stable global
  model namespace of IDs 0 through 759. Every entry and typed command program
  is preserved. These comprise 849 exact slot/content variants. The
  [model census](docs/re/MODEL_ASSET_COVERAGE.md) distinguishes available
  static diagnostics from runtime-generated geometry and child/effect gaps;
  `coverage` reproduces that census with every variant's provenance.
- Handler 7: 35 regions and 378 four-byte values. Levels 11 through 43 each
  have an exact eight-value terrain palette: entries 0 through 3 are surface
  materials, entry 4 is the plane/water material, and entries 5 through 7
  remain unresolved.
- Handler 12: one 512x512 terrain entry for each level 11 through 43
  (8,650,752 cells total), with 5,375 exact sparse records. The exporter writes
  exact payloads and reversible height, material, shade, marker, diagonal,
  blend-rank, texture-ID, and orientation channels. It does not claim to be a
  textured terrain renderer.
- Handler 4: five typed fonts with 1,152 glyph metrics and sprite arguments.
  The `fonts` family exports their decoded metadata, exact bytes and mappings.
- Handler 10: fifteen typed signed sample grids, including eight-WORD samples,
  four-sample masks and material indices. `tables` exports every sample with
  exact source spans; all 288,020 bytes after the headers are accounted for.
  See [sample assets](docs/re/SAMPLE_ASSETS.md) for consumer roles and limits.
- Handler 13: 162 constructor-resource prefixes across 85 selectors, with
  155 initial model bindings, 123 shared actor prefixes with ten damage-response
  pairs each, 52 derived prefixes with 208 sound arguments, 37 flags-only
  prefixes, one terrain-light prefix and one relocated sixteen-pair table.
  See [constructor assets](docs/re/CONSTRUCTOR_ASSETS.md).
- Handler 14: 33 world resources containing 4,486 authored placements and
  11,476 ordered instance commands with exact relocation/source provenance.
  The `worlds` command exports both handlers into `world_assets/`, retaining
  raw class/instance suffixes, header words, relocation records and mappings.
  See [world assets](docs/re/WORLD_ASSETS.md) for the bounded semantics.

The installation family preserves the byte-exact 18,280-byte `PreLoad.dat`, its
parsed scene tree, and the deterministic inventory of all 1,932 installed OVLs:
494 active handler regions covering 158,707,834 bytes.

Other proven families include Handler-9 PCM/alias resources, Handler-0 strings,
Handlers 11/15 raw tables (the historical `missions` command name does not
establish entity-placement semantics), Handlers 1/2/6 raw words, and Handlers
5/10 fixed record tables. The `record_tables` manifest uses schema version 2;
`world_assets` uses version 9. Resident world snapshots use version 5 and
resolve proven H9 sound inputs, initial models, selector-86 data and
selector-11 parameter commands, selector-30/34/35 payloads and commands,
and the selector-10/36 family's retained payload and shared command prefix,
including selector10/21's full helper-selection DWORD and no-read command 25.
See [format documentation](docs/re/FORMAT_DOCUMENTATION.md)
for layouts and provenance.

`extracted/` is wholly generated and ignored by Git. Delete it
freely and rerun `inf-extract all`; never treat files there as hand-maintained
evidence. Curated screenshots, preload reports, and other durable evidence are kept
privately.

## Direct OVL viewer

Launch the workbench without first extracting assets:

```sh
cargo run -p inf-viewer
cargo run -p inf-viewer -- --mode models --model towergun
cargo run -p inf-viewer -- --mode models --presentation neutral-composite --model comradar
cargo run -p inf-viewer -- --mode sprites --ovl 0.ovl
cargo run -p inf-viewer -- --mode terrain --ovl 11.ovl
```

The Models, Sprites, and Terrain tabs read the installed OVL corpus directly.
Tap or hold Left/Right to browse; held arrows repeat after a short delay, while
Page Up/Page Down remain single-step controls. F1/F2/F3 switch tabs. Model
search accepts a name, global ID such as `#123` or `0x7b`, or source OVL.
`--ovl` filters what the workbench lists, while model rendering still retains
the full H3/H7 dependency pool because global model resources can be supplied
by another OVL.
Click the `MODEL current/total` field (or press `G`), enter a 1-based catalog
position such as `500`, and press Enter to jump directly; Escape cancels the
edit. The model list is the recovered 760-slot global namespace rather than
5,142 repeated physical OVL registrations. Exact copies are collapsed; the 16
global slots with genuinely different content retain a `V VARIANT x/y` control
and source/local-slot provenance. `V` cycles those model variants, while a
single-OVL view defaults to the final local registration written to each global
slot. Handler-8 child references form a derived graph over those same slots;
they do not create a second assembly asset namespace. The viewer deliberately
rebuilds this in-memory catalog from `PreLoad.dat` and the installed OVLs rather
than treating generated JSON as format authority. A future persistent startup
cache would require corpus fingerprints and a schema/load-policy version.
Neutral child lookup is currently conservative within the selected source OVL;
`SceneCatalog::load` additionally resolves one explicit
level/difficulty/language scene to its winning Handler-8/3/7 registrations in
`FUN_004EE680` dispatch order, replacing the corpus-wide guess for
single-leaf tables. Authored Group/Select residency is documented in
[Runtime foundation](docs/re/RUNTIME_FOUNDATION.md); this catalog does
not execute that lifetime policy or represent all coexisting leaves.

In the model view, `A` cycles the selected root through its local shell, its
neutral composite, and the separate **Captured Draw** tier described below.
Without verified capture bundles the latter remains unavailable. Right-drag
orbits, the mouse wheel zooms, middle-drag
pans, and Reset Position (or `R`) becomes available after a pan; selecting
another model resets its variant and pan automatically. Models are rendered as
depth-tested Handler-7 RGB555 fills and Handler-3 textures instead of a blanket
wireframe. The software target follows the recovered Direct3D `LEQUAL` policy;
opaque/masked surfaces write depth, while additive and inverse-color-darken
layers blend without claiming depth. Fixed faces and triangle fans follow the
original cull-direction, first-triangle, per-triangle, fan-termination, and
mirror-pass rules in the orbit camera's handedness. `T` toggles texture
sampling, `D` selects the active `d0`..`d4` difficulty resource overlay, and
`M` selects Handler-3 mode 0 or 1. The fixed
triangle/quad UV corners and fan Q0.16 UV scaling come from the original
software-renderer backend. Missing resources and still-unproven Handler-3
source-alpha surfaces retain cyan diagnostic edges. `Q` selects the Direct3D
backend's runtime-dependent quad diagonal
(`0-2` or `1-3`) when inspecting non-planar quads. Sprite mode, physical
variant, and base/0..31 shade are inspectable,
while terrain cycles through the exact diagnostic channels.

Local-shell presentation is deliberately limited to polygon geometry proven
within the selected Handler-8 root. Neutral-composite presentation adds every
recursively linked child whose attachment slot and fixed/saved base orientation
are proven. Runtime-dependent local faces and unresolved siblings remain
explicit omissions, so the result is labeled a proven partial neutral
composite—not a canonical animation frame, a complete assembly, or a runtime
entity. When no child can be proven but the local shell is available, the
viewer visibly falls back to that shell instead of presenting an empty
category. Proven register expressions are evaluated; callback-backed joint
inputs use the explicitly labeled zero-value neutral policy, not a claim that
the retail callback always returned zero. Authored rotations use the original
`Infest.exe` sine table and shared native basis arithmetic; raw-zero angles
copy the proven base without a rotation call. Missing table data leaves
nonzero rotations unresolved and displays a warning. This exposes 105 partial
composite variants, including 25 without a local-shell diagnostic; see
[model composition](docs/re/MODEL_COMPOSITION.md).
Live joint values, callback-generated faces,
original lighting, Handler-13 runtime entity semantics, non-mesh effects, and
the retail shadow pass remain explicit future work; model surfaces currently
use their recovered base texture/material color.

Handler-8 opcode `0x05` nested transforms are projected only when their origin,
three packed rotations, and authored vertex range are all static. The
interpreter follows `FUN_004ED9C0`'s three basis rotations, transformed and
mirrored slot initialization, recursive command execution, and transform-only
state restoration. The retail corpus contains 58 such commands: 57 have
literal rotations, while global model 168 (`healthsmall`) retains a live
callback-derived angle and therefore stays unavailable in static presentation.
Its two verified draw observations can instead be inspected below. Opcode
`0x09` has no retail occurrence; its oriented live-slot basis remains
structurally decoded but is not fabricated as diagnostic geometry.

### Explicit inspection eye

The separate Explicit Eye tier supplies a diagnostic root-coordinate eye to
proven view-plane guards. Use `--model zonec --inspection-eye=0,0,-1024`
and optional `--eye-shift 0`; `E` edits exact signed XYZ, axis buttons step by
1024, and the shift control changes the native low-five-bit shift. Orthographic
orbit stays independent. Input values, source spans and branch decisions are
visible, and missing/unsupported inputs remain unresolved. The three zone signs
now select geometry, but their shared-bank textures are outside the viewer's
current source/startup resolver. This tier does not change the static census
or claim a captured retail camera. See
[view-plane signs](docs/re/MODEL_ASSET_COVERAGE.md#view-plane-signs-next-inspection-path).

### Captured draw inspection

The [canonical observation importer](crates/inf-formats/src/observed_model.rs)
reads completed `HealthSmallDraw` query bundles from the
accepted TTD ledger.
It checks ordered events, callback results, snapshot addresses and bytes,
and the complete canonical source fingerprint. Failed, partial or mismatched
bundles are rejected before a window opens. The legacy
[`RuntimeFrame` v1 schema](crates/inf-formats/src/replay.rs) is not used.

From ``, inspect both accepted observations:

```powershell
cargo run --release -p inf-viewer -- --data-root retail --capture .tmp/infestation-ttd/20260923-102834-171-HealthSmallDraw --capture .tmp/infestation-ttd/20260923-103318-490-HealthSmallDraw
```

Left/Right or Previous/Next switches draws. `C` switches between magnified
captured screen coordinates and orbitable source vertices oriented with the
observed basis. The latter omits translation for framing and does not invent
world coordinates for projection-only slots. Both are diagnostic wireframes
of all authored triangles; materials, native visibility and full-frame
appearance remain unverified. Wheel zooms, middle-drag pans, and right-drag
orbits only the source-geometry view. `R` resets the view.

`--capture-index 2` selects the second bundle initially;
`--capture-view source-geometry` selects the orbitable view.
`--snapshot <output.png>` writes one workbench frame through a hidden window
and exits; it also works for the ordinary model, sprite and terrain views.
The output directory must already exist. Capture selection and ordinary
`--model`/`--mode`/`--presentation` startup selectors are mutually exclusive.

## Read-only retail runtime inspector

`inf-original-inspect` attaches to the supported 32-bit retail `Infest.exe`
with `PROCESS_VM_READ`; it never writes retail memory or invokes a resource,
entity, render, or callback function. The executable is rejected unless its
PE32 machine, timestamp, fixed image base, and image size match the documented
structural profile. The corresponding repository executable has SHA-256
`BF09F66E3685D4990D95DA13D4BE1CA7E293FB31CDDCCC90BD575882921E4956`;
the inspector reports this expected file hash but does not pretend that a
process-memory PE check has hashed the file.

Run these from `` while the original game is running:

```sh
cargo run -p inf-inspect --bin inf-original-inspect -- models
cargo run -p inf-inspect --bin inf-original-inspect -- models --format json --output captures/models.json
cargo run -p inf-inspect --bin inf-original-inspect -- entities
cargo run -p inf-inspect --bin inf-original-inspect -- model-timeline --seconds 60 --hz 10 --output captures/models.jsonl
cargo run -p inf-inspect --bin inf-original-inspect -- entity-timeline --seconds 60 --hz 20 --output captures/entities.jsonl
```

Pass `--pid NUMBER` before the subcommand when more than one retail process is
running. Timeline commands also accept `--stop-file PATH`; the marker is only
observed and must be created by the caller.

The model commands read Handler 8 from the fixed 16-handler table at
`0x0053AA14`. Slot 8 contains the fixed object at `0x0055EEE0`; its `+0x04`
field points to the active registration array and `+0x08` contains the 760-slot
allocation read from `PreLoad.dat`. `FUN_004EFC00` and `FUN_0050F5A0` prove
that non-null cells are raw model-entry pointers. Bracketed table reads and
duplicate entry reads retain exact header fields, aligned byte counts, names,
and FNV-1a fingerprints so a runtime registration can be matched to its
physical OVL variant. The retail lookup at `FUN_0049FEA0` accepts ID 760 even
though only slots 0 through 759 were allocated; the inspector intentionally
does not copy that original off-by-one bug.

The entity commands traverse only the handle list proven by
`FUN_0046B030`, `FUN_0046B090`, and `FUN_00488AA0`: the world pointer at
`0x0055F160`, list head at `world+0x60`, node links at `+0/+4`, and entity at
`node+8`. They preserve raw positions, flags, model ID, ten transform words,
and both handle fields through `+0xD8`. Axis names, coordinate scale, entity
class, and transform-word semantics remain deliberately unlabeled. Every
traversal is bounded and duplicated; instability remains explicit in the
capture instead of being silently combined.

This passive path cannot observe the transient render context constructed at
`FUN_0049FEA0`/`FUN_004A00E0`. The bounded TTD harness observes actual callback
calls/results, native bases and transient slots for the subject above. Only
22 context WORDs are independent register storage; a raw 64-WORD window
overlaps the parent pointer and saved basis. Passive table samples cannot
substitute for this per-draw evidence, and no tool invokes retail callbacks.

## Validation

```sh
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The parsers are grounded in the retail scene-tree, OVL and resource
handler functions and the model render-command handlers. Handler 3, Handler 8, and Handlers 13/14 use their
recovered framing; there is no signature scan or heuristic fallback in the
canonical path.
