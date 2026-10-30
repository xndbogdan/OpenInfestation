#!/usr/bin/env bash
# Builds and packages the Linux release inside an ubuntu:22.04 container; run
# by scripts/release.ps1. The oldest supported Ubuntu keeps the binary's glibc
# requirement low, so it runs on more distributions. SDL2 is bundled and linked
# statically so players need no SDL install.
#
# Expects the repository at /src, THIRD_PARTY_LICENSES.html and Remastered.pak
# already in the output directory, and RELEASE_NAME and RELEASE_OUT in the
# environment.
set -euo pipefail

export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq --no-install-recommends ca-certificates curl \
  build-essential cmake pkg-config \
  libx11-dev libxext-dev libxrandr-dev libxcursor-dev libxi-dev \
  libxss-dev libxinerama-dev libxkbcommon-dev libwayland-dev \
  wayland-protocols libegl1-mesa-dev libgl1-mesa-dev libgles2-mesa-dev \
  libasound2-dev libpulse-dev libdbus-1-dev libudev-dev > /dev/null

if ! command -v cargo > /dev/null; then
  curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
fi
export PATH="$HOME/.cargo/bin:$PATH"

cd /src
cargo build --release -p inf-game --features sdl2/bundled,sdl2/static-link

# Stage inside the container: modes set on the Windows bind mount don't stick.
package="/tmp/$RELEASE_NAME"
rm -rf "$package"
mkdir -p "$package"
cp README.md DISCLOSURE.md LICENSE-MIT LICENSE-APACHE "$package/"
cp "$RELEASE_OUT/THIRD_PARTY_LICENSES.html" "$RELEASE_OUT/Remastered.pak" "$package/"
chmod 644 "$package"/*
cp "$CARGO_TARGET_DIR/release/inf-game" "$package/openinfestation"
chmod 755 "$package/openinfestation"
tar -C /tmp --owner=0 --group=0 -czf "$RELEASE_OUT/$RELEASE_NAME.tar.gz" "$RELEASE_NAME"
rm -rf "$package"
