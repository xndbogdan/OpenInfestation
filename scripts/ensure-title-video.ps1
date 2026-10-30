<#
.SYNOPSIS
  Ensure the transcoded Title.avi frame store exists for the Infestation port.

.DESCRIPTION
  Retail decodes `Title.avi` (640x480 IV50, 15 fps, 315 frames) through
  DirectShow. The port plays pre-transcoded RGB PNGs with no runtime codec
  dependency (see `crates/inf-game/src/title_playback.rs`).

  This script runs the one documented ffmpeg command:

    ffmpeg -hide_banner -i Title.avi -map 0:v -vsync passthrough `
      -start_number 0 <data-root>/video/title_%03d.png

  Under ffmpeg 7.1.1 that yields 315 PNGs (`title_000.png`..`title_314.png`).
  Frames live next to `Title.avi`, local and uncommitted; a missing store
  takes the native failed-open path (first title tick finishes to phase 2
  without video).

.PARAMETER DataRoot
  Directory containing `Title.avi`. Defaults to the repository's `retail/`;
  pass the installed game's folder to transcode there instead.

.EXAMPLE
  .\scripts\ensure-title-video.ps1
  .\scripts\ensure-title-video.ps1 -DataRoot <install-dir>
#>
param(
  [string]$DataRoot = (Join-Path $PSScriptRoot ".." "retail")
)

$ErrorActionPreference = "Stop"

$avi = Join-Path $DataRoot "Title.avi"
$video = Join-Path $DataRoot "video"

if (-not (Test-Path $avi)) {
  throw "Title.avi not found at $avi. Pass the retail install dir explicitly: .\scripts\ensure-title-video.ps1 -DataRoot <dir-containing-Title.avi>"
}
New-Item -ItemType Directory -Force -Path $video | Out-Null

$existing = (Get-ChildItem $video -Filter "title_*.png" -ErrorAction SilentlyContinue | Measure-Object).Count
if ($existing -eq 315) {
  Write-Host "Title video store already complete: $video (315 frames)"
  exit 0
}
if ($existing -gt 0) {
  Write-Host "Title video store has $existing/315 frames; re-transcoding..."
}

$ffmpeg = Get-Command ffmpeg -ErrorAction SilentlyContinue
if (-not $ffmpeg) {
  throw "ffmpeg not found on PATH (required: ffmpeg 7.1.1 for the documented bit-exact transcode)"
}

& ffmpeg -hide_banner -i $avi -map 0:v -vsync passthrough -start_number 0 (Join-Path $video "title_%03d.png")
if ($LASTEXITCODE -ne 0) {
  throw "ffmpeg transcode failed with exit code $LASTEXITCODE"
}

$count = (Get-ChildItem $video -Filter "title_*.png" | Measure-Object).Count
if ($count -ne 315) {
  throw "Expected 315 transcoded frames, found $count in $video"
}
Write-Host "Title video store ready: $video (315 frames)"
