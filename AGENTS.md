# Repository Guidelines

These guidelines apply to human contributors and coding agents alike.

## Project Structure

OpenInfestation is a Rust reimplementation of Infestation (2000). It is built
from reverse engineering of the original executable and data. The Cargo
workspace lives at the repository root, with these crates under `crates/`:

- `inf-formats`: `PreLoad.dat`, OVL and resource parsers that follow the
  retail loader and renderer.
- `inf-game`: launcher, frontend, gameplay and the three renderers; the
  default member.
- `inf-extract`: bulk asset exporter that keeps exact bytes and provenance.
- `inf-viewer`: direct-OVL asset workbench.
- `inf-inspect`: read-only probes of the retail executable.

Integration tests live in each crate's `tests/` directory; native receipts
(the retail code's results under emulation) are JSON fixtures in
`crates/inf-game/tests/fixtures/`. Reverse-engineering writeups live in
`docs/re/`, progress in `docs/progress/`, renderer plans in `docs/` and
`docs/rt-plans/`. `ngx-bridge/` is the DLSS bridge's C++ source; `ambience/`
is the manifest `Remastered.pak` is built from. Only exported branding (the
icon and the launcher banner) is tracked; their sources are kept outside this
repository.

Original game data is never committed. Tests read a retail installation
copied into the ignored `retail/` directory (`Infest.exe`, `PreLoad.dat`,
`Overlay/`, and for some tests `Infest.sav`, `Title.avi`, `cdaudio/`,
`video/` and the demo's `infestation_demo/`). A junction or symlink to an
installation works too. Most tests fail without it.

## Build, Test, and Development Commands

Run everything from the repository root. Use `--release` for `cargo build`,
`cargo run` and `cargo test` unless you are debugging a build-specific
problem.

```sh
cargo build --release                      # optimized build
cargo check                                # fast type check
cargo test -p inf-game --lib --release     # game unit tests
cargo test --workspace --release           # complete workspace
cargo run --release -- --data-root retail  # launcher, then the game
cargo run --release -p inf-extract -- all  # regenerate extracted/
```

On `x86_64-pc-windows-gnu`, `.cargo/config.toml` selects
`scripts/windows-gnu-gcc.cmd`. That script keeps MinGW GCC's CRT and SDL2
lookup and uses the active toolchain's bundled LLD, so MinGW GCC must stay on
`PATH`. Windows builds link SDL2 statically; Linux release builds need
`--features sdl2/bundled,sdl2/static-link`.

The Vulkan shaders are committed as SPIR-V, so building needs no Vulkan SDK.
After editing a `.vert`, `.frag` or `.comp` file, run
`scripts/compile-shaders.ps1` and commit both.

Enable the repository hooks once per clone with
`git config core.hooksPath .githooks`. The pre-commit hook regenerates the
progress numbers in `README.md` and `docs/progress/` from the staged files;
never edit those by hand. Record per-function judgments (implemented,
approximated, documented, not-needed) in `docs/progress/status.csv`. Cite
retail functions by their start address (`FUN_004476D0`, `004476D0`), so the
progress count stays meaningful.

### Test runs

Repeatable, unattended runs of the game skip the launcher: any of
`--settings`, `--profiles`, `--start`, `--input-replay`, `--frames`,
`--snapshot`, `--hidden`, `--fixed-step-micros` or `--event-log` makes a
direct run (see [README](README.md#pre-game-launcher)).

- Pass `--renderer software` to every snapshot, probe or retail comparison:
  the CPU raster is the deterministic pixel reference. The default renderer
  is OpenGL.
- `--hidden` runs are silent; keep probes and tests silent unless a test is
  about sound.
- The executable is a Windows GUI program: PowerShell does not wait for it
  unless its output is piped (`| Out-Host`).
- Probes exit 0 through contained stops; read their logs rather than the
  exit code.
- `scripts/world_sweep.py` loads every world headless and summarizes each
  one's notable log lines; `scripts/perf-scenes.ps1` times traced scenes.

## Coding Style & Naming Conventions

Follow standard `rustfmt` formatting: four-space indentation, `snake_case`
functions and modules, `CamelCase` types, `SCREAMING_SNAKE_CASE` constants.
Prefer small modules that match existing crate boundaries. Keep RE names,
addresses and original-function references when they clarify provenance.

## Testing Guidelines

Add or update focused tests when you change parsers, menus, renderer data
paths, saves, or world logic. Run at least the touched crate's tests, and
`cargo test -p inf-game --lib --release` before handing off game changes.

## Fidelity Rules

Faithfulness to the original game comes first. When decompiled code,
extracted assets or original-game observations can verify behavior, timing,
visuals, audio or data interpretation, prefer that evidence over convenience
approximations. Gameplay and presentation match retail, even where retail
looks bad; no quality-of-life deviation goes in without being agreed first.

The renderers have fixed roles:

- The software raster and OpenGL always play retail's game, whatever the
  camera style or scaling: they look, sound and behave as close to the
  original as possible. Camera Style's Remastered and the widescreen frame
  change only their picture; see
  [the game's draw and the shown draw](docs/re/PRESENTATION_POLICY.md#the-games-draw-and-the-shown-draw).
- Vulkan, plain and path traced, is the remaster in single player: every
  modern look or sound lives there only, and its game aims for retail except
  where that costs functionality.
- Network games play retail's game on every renderer, so every machine in a
  game agrees. Network play does not work yet.
- Fixes for retail bugs are separate settings, on by default, and follow
  neither renderer. A remastered choice the player picks in a menu keeps
  retail's as its default.

Some primary evidence is private and not part of this repository:
decompiler output, the Unicorn checkers that produce the native receipts
(`check_*.py`, named in tests and notes), the retail scenario lab
(`runtime_re/`) and time-travel debugging recordings (`.tmp/infestation-ttd/`).
`docs/re/` holds the conclusions drawn from them, and each fixture records the
SHA-256 of the executable and checker that produced it. If a change needs
evidence that the public notes don't contain, say so instead of guessing.

Caution is welcome; a silent stall is not. If a fidelity gap blocks the work
and can't be closed within the task, do one of two things:

- Adopt a **labeled approximation**. Name it in code and in the owning
  `docs/re/` document, record the evidence bounds, and keep it replaceable.
  Never bake captured values in as behavior. An approximation never counts
  as retail acceptance.
- Or stop, report the blocker, and propose the approximation you would use.

Fail closed per feature: skip and log the unowned actor, effect or draw.
Don't fail a whole phase. Keep phase IDs, OVL leaf IDs, campaign selections
and per-handler resource slots distinct; see
[the runtime foundation](docs/re/RUNTIME_FOUNDATION.md).

## Maintainability and RE Safety

- Treat original OVL bytes as fixed-layout evidence. Never insert bytes or
  otherwise shift offsets inside an original overlay.
- Before adding a renderer, parser or gameplay API, search all call sites and
  existing variants. Prefer one data-bearing request type plus an explicit
  enum for genuine modes over a ladder of near-identical methods.
- Never encode a behavioral mode through a coincidental numeric value or
  other sentinel unless the original data format requires it. Verify
  signedness and units of each legacy field at its retail consumer.
- Keep RE provenance attached to the code that implements it. When evidence
  disproves an old interpretation, update the relevant `docs/re/` notes in
  the same change.
- Don't commit generated extracted output, local captures, game data, sound
  files, `Remastered.pak` or upscaler libraries.
