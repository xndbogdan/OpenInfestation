# OpenInfestation

OpenInfestation is an open-source Rust reimplementation of **Infestation**
(Frontier Developments, 2000), rebuilt from reverse engineering of the
original executable, its data formats and observed retail runtime behavior.
It is not a matching decompilation: it reads the original game's files and
plays them with its own code.

> **Unofficial fan project.** OpenInfestation is not affiliated with,
> endorsed by, or connected to Frontier Developments or Ubisoft.
> "Infestation" and related names belong to their respective owners.
> **You need an original copy of Infestation to play.** This repository and
> its releases contain no original game data, executables, videos or music.
> The game loads those from your own installation, and the launcher checks
> them against known retail checksums.

## Status

The whole single-player campaign plays, from the title video through the
menus, briefings and every world. The software and OpenGL renderers play
retail's game; Vulkan adds a remastered look. **Network play is not working
yet**: the menus and protocol are there, but a network game does not play
correctly ([network layer](docs/re/NETWORK_RUNTIME.md)).

<!-- progress:start -->
[![RE coverage](https://img.shields.io/badge/RE%20coverage-100.0%25%20code%20%2F%20100.0%25%20notes-orange)](docs/progress/README.md)

Of the **2,916** game functions in retail `Infest.exe` the port
covers (865 not needed are [omitted](docs/progress/README.md#omitted)),
**2,916 (100.0%)** are referenced by the port's code and
**2,916 (100.0%)** by its code or RE notes.
[How this is measured](docs/progress/README.md).

[![Map of the retail functions by RE note and state](docs/progress/treemap.svg)](docs/progress/README.md#map)
<!-- progress:end -->

## Playing

Releases have Windows and Linux (x86-64) builds: a `windows-x64` zip with
`OpenInfestation.exe` and a Linux tarball with `openinfestation`. SDL2 is
built into the binary, so no DLLs are needed (the optional upscalers below
are the only exception).

**What you need from your copy.** The launcher checks the installation
before Play turns on:

- `Infest.exe`: the port reads the game's tables from it (it is never run);
- `PreLoad.dat`, which lists every scene and the overlays it loads;
- `Overlay/`, with every overlay the scenes request;
- optional: `Title.avi` for the intro video and `cdaudio/` with the disc's
  music tracks (2 to 9). Without them the game skips the video or runs
  silent, as retail does with no CD in the drive.

File-name case doesn't matter (`preload.dat` or `OVERLAY/` work too). A
data file whose checksum no known release has only warns, since another
build may play fine ([`retail-checksums.json`](crates/inf-game/src/retail-checksums.json)).

**Windows**

1. Extract the zip.
2. Run `OpenInfestation.exe`. Place it beside the installed game, or press
   Change in the launcher and pick the installation; it is remembered.
3. Press Play.

Releases are unsigned, so Windows SmartScreen may warn on first launch.

**Linux**

The launcher is the same window, drawn with egui over OpenGL (X11 or
Wayland). It has been tested on CachyOS under KDE Plasma (Wayland). Change
opens the desktop's folder chooser through the xdg portal; without one,
type the folder into the path field and press Enter. On integrated graphics
the OpenGL renderer is the recommended choice.

```sh
tar -xzf OpenInfestation-*-linux-x64.tar.gz && cd OpenInfestation-*-linux-x64
./openinfestation                                  # launcher
./openinfestation --data-root /path/to/Infestation  # or point at the game directly
```

**Saves and settings.** Saving writes retail's own `Infest.sav` beside the
game's data, so retail and the port share one save list. The port's
settings are `settings.json` beside the executable (`--settings PATH`
chooses another). A settings file the game cannot read stops the game with
the error; the launcher offers **Repair settings** (or **Reset settings**)
and keeps the old file as `settings.json.broken`.

**Controllers**

A game controller plays as retail's joystick: left stick to steer and
pitch, triggers for thrust, and its buttons are the key configuration's
Joystick 1 to 16. The launcher's Controller layout, or
`--controller-layout console`, switches to the PlayStation release's pad
layout, whose tables the PC executable carries unused
([the PlayStation release](docs/re/INPUT_RUNTIME.md#the-playstation-release)).
Retail never reads the mouse.

**Fixes for the original's bugs**

The game runs retail's own rules. Where retail breaks, mostly at high frame
rates, the port adds a fix, **on by default**. The launcher's Options list
each one; turning it off (or passing its flag as `false`) restores pure
retail:

| Fix | Flag |
|---|---|
| The ACAM keeps to the ground at high frame rates | `--vehicle-position-carry` |
| No black band crosses the ACAM's roof as the hovercraft morphs | `--vehicle-gun-mounts` |
| The hovercraft's rear fans are not drawn inside out (black) | `--hovercraft-fans` |
| The world keeps its speed at high frame rates | `--exact-world-ticks` |
| Name entry and language animations keep their speed | `--exact-frontend-ticks` |
| The computer's database draws every entry's model | `--database-models` |
| The guided missiles' launchers fold away between forms | `--guided-mounts` |
| A finished campaign in the saves gives every game its bonus | `--completion-bonus` |
| Scientists, alien ships and creatures move as at 30 fps | `--paced-actors` |
| Water splashes take at most 12 of the 24 sound voices | `--splash-voice-cap` |
| A rescue (Pillier Stren's) completes when its last scientist boards | `--mission-completion` |

The Cheats and extras group adds:

- **Every cheat in the Cheats Menu** (`--all-cheats`, off by default).
- **Pelagia**, the prototype world (`--prototype-world`, on): the Command
  Centre's dead warpgate wakes for a scientist dropped by it and leads to
  world 1, with a name, a briefing and a way back.
- **Pelagia rework** (`--pelagia-rework`, on, needs Pelagia): world 1
  gains its lagoon, crystals, return gate and factory stock.

## Renderers

Choose the renderer in the launcher's Options or
with `--renderer`:

- **Automatic** (the default) uses OpenGL and falls back to Software when
  OpenGL cannot start.
- **OpenGL** and **Software** play retail's game exactly. Software is the
  CPU pixel reference: identical frames on every machine. Widescreen shows
  more to each side, but there the extra view is only a picture: objects
  out at the sides stand still, because retail never simulated beyond its
  4:3 view ([widescreen](docs/re/PRESENTATION_POLICY.md#widescreen)).
- **Vulkan** is the remaster. With a ray-tracing GPU it path traces the
  world; on any Vulkan GPU it adds a modern sky and clouds, new water,
  rain with puddles and wet ground, and reflections on water and metal. It
  offers SMAA, FXAA, TAA and SSAA, and the upscalers your GPU supports:
  NVIDIA DLSS and DLSS Ray Reconstruction (RTX), Intel XeSS and AMD FSR 3.1.
  Only what the GPU can run is listed. Vulkan falls back to OpenGL, then
  Software. See [the remaster's design](docs/INFESTATION-RT-IMPLEMENTATION.md).

Network games play retail's game on every renderer.

The upscalers are optional libraries placed beside the executable; none
ship with the port. [`scripts/build-upscalers.ps1`](scripts/build-upscalers.ps1)
builds the DLSS bridge ([`ngx-bridge/`](ngx-bridge/)) and copies them:

- FSR 3.1: AMD's `amd_fidelityfx_vk.dll` (FidelityFX SDK 1.1.4, MIT);
- XeSS: Intel's `libxess.dll` (Intel Simplified Software License);
- DLSS: the port's `inf_ngx.dll` plus NVIDIA's `nvngx_dlss.dll` and
  `nvngx_dlssd.dll`.

A missing library makes its upscaler unavailable, and the log says so once.

The remastered ambience under Vulkan plays from `Remastered.pak` beside the
game's data. Without it retail's sound plays as always. The pak is built
from your own sound files ([`ambience/`](ambience/README.md)); see
[Building](#building).

**Display.** Display offers In a Window, Full Screen (a real mode change)
and Borderless. Resolution offers retail's 512x384 to 1024x768 plus every
mode the monitor reports, in physical pixels. Image scaling
(`--scaling`) is `widescreen` (the default), `four-three`, `stretched` or
`integer`. Camera Style offers Normal, Cinematic and Remastered.

## Performance

The game is CPU-bound. On an RTX 5080 desktop at 3840x2160 widescreen,
OpenGL and the Vulkan raster both run between 78 and 190 fps depending on
the scene, with the GPU mostly idle. Measurements and method are in
[`docs/PERFORMANCE.md`](docs/PERFORMANCE.md).

## Reverse-engineering notes

[`docs/re/`](docs/re/) holds the writeups behind the port: file formats,
the renderer, actors, missions, audio, input, saves and the network layer.
Start with [`FORMAT_DOCUMENTATION.md`](docs/re/FORMAT_DOCUMENTATION.md),
[`GAMEPLAY_RUNTIME.md`](docs/re/GAMEPLAY_RUNTIME.md) and
[`PIXEL_RUNTIME.md`](docs/re/PIXEL_RUNTIME.md). The notes cite retail
functions and addresses. [`docs/progress/`](docs/progress/README.md) counts
which retail functions the port's code and notes cite;
`scripts/progress.ps1` regenerates it.

Infestation is the successor of V2000: its source tree was named `V3000`,
and it runs on a C++ port of V2000's engine. The V2000 reimplementation is
the sibling project [OpenV2K](https://github.com/xndbogdan/OpenV2K). The
two ports share no format code: the games have different `PreLoad.dat`
grammars, handler framing and render-command streams.

## Building

Requirements on Windows: Rust (`stable-x86_64-pc-windows-gnu`) and MinGW-w64
GCC on `PATH`. Run everything from the repository root.

```sh
cargo build --release                 # target/release/inf-game(.exe)
cargo check --workspace --all-targets
```

On Linux, bundle SDL2 as the releases do (CMake and the X11, Wayland, GL and
audio development packages listed in
[`.github/workflows/release.yml`](.github/workflows/release.yml)):

```sh
cargo build --release -p inf-game --features sdl2/bundled,sdl2/static-link
```

**Remastered.pak.** `python -I scripts/build-remastered-pak.py` builds the
pak from the sound folders [`ambience/manifest.json`](ambience/manifest.json)
names (it needs ffmpeg). `--release` leaves out every sound no licence lets
the port ship. The sounds themselves are not in this repository.

**Releases.** Pushing a `v*` tag runs the release workflow, which builds
Windows and Linux packages with the third-party licences and a checksum
file.

### Tests and retail data

Most tests read a retail installation copied into the ignored
[`retail/`](retail/README.md) folder (a junction or symlink works). Tests
that can run without it skip; the full suite needs it.

```sh
cargo test -p inf-game --lib    # debug builds test faster
cargo test --workspace --release
```

After cloning, enable the repository hooks once, so each commit refreshes
the progress numbers:

```sh
git config core.hooksPath .githooks
```

## Running and inspecting

```sh
cargo run --release                                  # launcher
cargo run --release -- --data-root retail --no-launcher
cargo run --release -- --data-root retail --renderer software --hidden --frames 600 --fixed-step-micros 16000 --snapshot frontend.png
cargo run --release -- --data-root retail --mode resident --activate 47,48,11 --json
```

`--data-root` always wins; otherwise the game uses the executable's folder
when it holds `PreLoad.dat`, then the launcher's remembered installation,
then `retail/`. `.\scripts\ensure-title-video.ps1` transcodes `Title.avi`
once so the intro video shows in development runs.

Direct runs skip the launcher: `--hidden` (silent, no window),
`--frames`, `--fixed-step-micros`, `--snapshot`, `--input-replay` (a JSON
list of SDL scancode presses by frame) and `--start [STATE.json]`, which
drops straight into a world with a chosen position, inventory and progress
([`harness_start.rs`](crates/inf-game/src/harness_start.rs) maps each field
to [its save slot](docs/re/SAVE_FORMAT.md)). Use scratch `--settings` and
`--profiles` copies for these, since a results screen saves into the real
`Infest.sav`. `--mode independent` and `--mode resident` print headless
resource reports for one scene.

Other workspace tools: `inf-extract` exports every resource family to an
ignored `extracted/` folder ([asset coverage](docs/re/ASSET_COVERAGE.md));
`inf-viewer` browses models, sprites and terrain straight from the OVLs;
`inf-original-inspect` reads a running retail `Infest.exe`'s memory without
writing to it.

### Bug reports

Start the game with `--session-log` to write `<data-root>/logs/run-<time>.log`
with every key, phase change and a status line every 600 frames, plus a
`.replay` beside it. `--replay-session PATH` replays that run exactly;
`--resume` plays on live from where it ends. Every desktop start keeps a
replay of its own in `logs/replays/` (the newest ten). When the game stops
on a failure, or a frame hangs for 15 seconds, it writes
`infest-crash-log-<time>.txt` in the log folder, naming the failure, the
game state and the last 300 lines printed. Attach the crash log and replay
to a report.

## Verification harness

The [FGDK harness](https://github.com/xndbogdan/fgdk-harness) is an agent
skill shared by OpenV2K and OpenInfestation. It runs either port headless
and repeatable with the probe flags above, reproduces a player's bug report
from its replay, sweeps every world for regressions and, with
[REA](https://github.com/morluto/rea), compares the port against your own
copy of the retail executable.

## Crate structure

| Crate | Purpose |
|---|---|
| `inf-formats` | `PreLoad.dat`, OVL and resource handler parsers, following the retail loader |
| `inf-game` | The game: launcher, frontend, worlds, renderers, audio, input |
| `inf-extract` | Byte-exact bulk exporter |
| `inf-viewer` | Model, sprite and terrain workbench over the installed OVLs |
| `inf-inspect` | Read-only memory inspector for the retail executable |

## License

OpenInfestation's source code, documentation and authored artwork
(including its icon and launcher header) are licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE)), or
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. Unless you explicitly state otherwise, any contribution you
intentionally submit for inclusion in this work, as defined in the
Apache-2.0 license, is dual licensed as above, without any additional terms
or conditions.

This license does not cover Infestation itself. The original game's data,
executable, audio and video remain the property of their respective owners
and are not distributed with this project.

### Credits and third-party notices

- The Vulkan renderer's SMAA is the reference implementation by Jorge
  Jimenez, Jose I. Echevarria, Belen Masia, Fernando Navarro and Diego
  Gutierrez, under its MIT license
  ([details](crates/inf-game/shaders/smaa/README.md)).
- The remastered ambience is made from recordings in the Sonniss GDC Game
  Audio Bundles and from Splice's royalty-free sounds; the full list of
  artists is in [`ambience/manifest.json`](ambience/manifest.json) and goes
  into the pak's `credits.txt`.
- The optional upscalers stay under their vendors' terms: AMD FidelityFX
  SDK (MIT), Intel XeSS SDK (Intel Simplified Software License) and NVIDIA
  DLSS (NVIDIA RTX SDK license). NVIDIA's SDK headers are not in this
  repository.
- Release packages include `THIRD_PARTY_LICENSES.html` for the Rust
  dependencies.
- [`DISCLOSURE.md`](DISCLOSURE.md) lists the third-party content release
  archives carry (the remastered sounds in `Remastered.pak`, and the
  runtime components) and the terms each comes under.
