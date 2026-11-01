#!/usr/bin/env bash
# The Linux release's profile-guided build, run by release-linux.sh in its
# ubuntu:22.04 container with the repository at /src: an instrumented
# `ship` build plays a silent training run on a virtual display (the worlds
# of scripts/build-pgo.ps1 on Mesa's software OpenGL and the software
# raster; the container has no GPU for Vulkan), LLVM merges its profile and
# the `ship` build is made again from it, at $CARGO_TARGET_DIR/ship.
set -euo pipefail
cd /src
features=sdl2/bundled,sdl2/static-link
flags="-C target-cpu=x86-64-v2"
work=/tmp/pgo
rm -rf "$work"
mkdir -p "$work/data" "$work/runs"
rustup component add llvm-tools-preview > /dev/null
profdata="$(rustc --print sysroot)/lib/rustlib/x86_64-unknown-linux-gnu/bin/llvm-profdata"

RUSTFLAGS="$flags -Cprofile-generate=$work/data" CARGO_TARGET_DIR="$CARGO_TARGET_DIR/pgo-generate" \
  cargo build --profile ship -p inf-game --features "$features"
game="$CARGO_TARGET_DIR/pgo-generate/ship/inf-game"

echo '[{"frame":40,"key":"Up","pressed":true},{"frame":120,"key":"Z","pressed":true},{"frame":1250,"key":"Z","pressed":false},{"frame":1250,"key":"Up","pressed":false}]' > "$work/runs/drive.json"
echo '[{"frame":30,"key":"L","pressed":true},{"frame":34,"key":"L","pressed":false}]' > "$work/runs/lamp.json"
# world|extra start fields|keys|renderer|frames
train() {
  printf '{"world":%s,"start":0,"form":1,"known":"all","hints_seen":"all"%s}' "$1" "$2" > "$work/runs/start.json"
  local args=(--no-launcher --start "$work/runs/start.json" --data-root /src/retail
    --settings "$work/runs/settings.json" --profiles "$work/runs/profiles.sav" --renderer "$4"
    --hidden --width 1280 --height 720 --fixed-step-micros 16000 --frames "$5")
  [ -n "$3" ] && args+=(--input-replay "$work/runs/$3.json")
  SDL_AUDIODRIVER=dummy LIBGL_ALWAYS_SOFTWARE=1 xvfb-run -a -s "-screen 0 1280x720x24" \
    "$game" "${args[@]}" > /dev/null 2>&1 || true
}
pelagia=',"position":[32400,28200]'
scenes=(
  "2||"
  "1|$pelagia,\"heading\":90,\"weather\":false|"
  "1|$pelagia,\"heading\":90,\"weather\":true|"
  "1|,\"upgrades\":[20],\"day_clock_ms\":900000,\"weather\":false|lamp"
  "3||" "11||" "17||"
  "14|,\"upgrades\":[20],\"day_clock_ms\":900000|lamp"
)
for scene in "${scenes[@]}"; do
  IFS='|' read -r world extra keys <<< "$scene"
  train "$world" "$extra" "$keys" opengl 900
done
train 1 "$pelagia,\"heading\":180,\"weather\":true" drive opengl 1300
train 2 "" "" software 300
train 1 "$pelagia,\"heading\":180,\"weather\":true" drive software 300
train 17 "" "" software 300
shopt -s nullglob
raw=("$work/data"/*.profraw)
if [ ${#raw[@]} -eq 0 ]; then
  echo "The training run left no profile; is the retail installation in /src/retail?" >&2
  exit 1
fi
"$profdata" merge -o "$work/merged.profdata" "${raw[@]}"

RUSTFLAGS="$flags -Cprofile-use=$work/merged.profdata" \
  cargo build --profile ship -p inf-game --features "$features"
