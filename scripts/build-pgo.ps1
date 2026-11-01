[CmdletBinding()]
param(
    # The retail installation the training run plays (see AGENTS.md).
    [string]$DataRoot = 'retail',
    # Rust's MinGW toolchain ships no profiler runtime; MSVC's, Rust's main
    # Windows target, does.
    [string]$Toolchain = '1.95-x86_64-pc-windows-msvc'
)

# Builds the shipped Windows game with profile-guided optimisation: an
# instrumented `ship` build plays a silent training run (eight worlds on
# OpenGL, the Vulkan raster and path traced, a drive and fire, three on the
# software raster), its profile is merged, and the `ship` build is made
# again from it, SDL2 built from its bundled sources and the C runtime
# linked in (no Visual C++ redistributable). Replays unchanged
# (docs/PERFORMANCE.md). Needs Visual Studio's C++ build tools and CMake.
# Prints the game's path last.

$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$target = 'x86_64-pc-windows-msvc'

function Invoke-Checked {
    param([string]$Name, [string]$Program, [string[]]$Arguments)
    Write-Host "`n== $Name" -ForegroundColor Cyan
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Name failed with exit code $LASTEXITCODE"
    }
}

Push-Location $repositoryRoot
try {
    $work = Join-Path $repositoryRoot 'target/pgo'
    $data = Join-Path $work 'data'
    $runs = Join-Path $work 'runs'
    foreach ($dir in $data, $runs) {
        if (Test-Path $dir) { Remove-Item -Recurse -Force $dir }
        New-Item -ItemType Directory -Force $dir | Out-Null
    }
    Invoke-Checked 'toolchain' 'rustup' @('toolchain', 'install', $Toolchain, '--profile', 'minimal')
    Invoke-Checked 'llvm-tools' 'rustup' @('component', 'add', 'llvm-tools-preview', '--toolchain', $Toolchain)
    $sysroot = (& rustup run $Toolchain rustc --print sysroot).Trim()
    $profdata = Join-Path $sysroot "lib/rustlib/$target/bin/llvm-profdata.exe"

    # MinGW's C standard flag (release.ps1) means nothing to MSVC's compiler.
    $env:CFLAGS = $null
    $build = @("+$Toolchain", 'build', '--profile', 'ship', '-p', 'inf-game', '--target', $target,
        '--features', 'sdl2/bundled,sdl2/static-link')
    # The C runtime linked in, so the game needs no Visual C++ redistributable.
    $flags = '-C target-cpu=x86-64-v2 -C target-feature=+crt-static'
    $env:CARGO_TARGET_DIR = Join-Path $work 'generate'
    $env:RUSTFLAGS = "$flags -Cprofile-generate=$data"
    Invoke-Checked 'instrumented build' 'cargo' $build
    $instrumented = Join-Path $env:CARGO_TARGET_DIR "$target/ship/inf-game.exe"

    $keys = @{
        drive = '[{"frame":40,"key":"Up","pressed":true},{"frame":120,"key":"Z","pressed":true},{"frame":1250,"key":"Z","pressed":false},{"frame":1250,"key":"Up","pressed":false}]'
        lamp  = '[{"frame":30,"key":"L","pressed":true},{"frame":34,"key":"L","pressed":false}]'
    }
    foreach ($name in $keys.Keys) {
        [IO.File]::WriteAllText((Join-Path $runs "$name.json"), $keys[$name])
    }
    # world, extra start fields, keys
    $scenes = @(
        @(2, '', ''), @(1, ',"position":[32400,28200],"heading":90,"weather":false', ''),
        @(1, ',"position":[32400,28200],"heading":90,"weather":true', ''),
        @(1, ',"upgrades":[20],"day_clock_ms":900000,"weather":false', 'lamp'),
        @(3, '', ''), @(11, '', ''), @(17, '', ''),
        @(14, ',"upgrades":[20],"day_clock_ms":900000', 'lamp')
    )
    $drive = @(1, ',"position":[32400,28200],"heading":180,"weather":true', 'drive')
    $settings = Join-Path $runs 'settings.json'
    function Invoke-Training {
        param($Scene, [string]$Renderer, [int]$Frames)
        $start = Join-Path $runs 'start.json'
        [IO.File]::WriteAllText($start,
            "{`"world`":$($Scene[0]),`"start`":0,`"form`":1,`"known`":`"all`",`"hints_seen`":`"all`"$($Scene[1])}")
        $arguments = @('--no-launcher', '--start', $start, '--data-root', $DataRoot, '--settings', $settings,
            '--profiles', (Join-Path $runs 'profiles.sav'), '--renderer', $Renderer, '--hidden',
            '--width', '1920', '--height', '1080', '--fixed-step-micros', '16000', '--frames', "$Frames")
        if ($Scene[2]) { $arguments += @('--input-replay', (Join-Path $runs "$($Scene[2]).json")) }
        # Piped, so PowerShell waits for the GUI program.
        & $instrumented @arguments 2>&1 | Out-Null
    }
    Write-Host "`n== training run" -ForegroundColor Cyan
    # A fresh settings file: Vulkan path traces where the GPU can.
    foreach ($scene in $scenes[0..2]) { Invoke-Training $scene 'vulkan' 600 }
    $json = Get-Content -Raw $settings | ConvertFrom-Json
    $json.modern.ray_tracing = $false
    [IO.File]::WriteAllText($settings, ($json | ConvertTo-Json -Depth 20))
    foreach ($scene in $scenes) {
        Invoke-Training $scene 'opengl' 900
        Invoke-Training $scene 'vulkan' 900
    }
    Invoke-Training $drive 'opengl' 1300
    Invoke-Training $drive 'vulkan' 1300
    foreach ($scene in @($scenes[0], $drive, $scenes[6])) { Invoke-Training $scene 'software' 300 }
    $raw = @(Get-ChildItem $data -Filter '*.profraw')
    if ($raw.Count -eq 0) {
        throw 'The training run left no profile; is the retail installation at -DataRoot?'
    }
    $merged = Join-Path $work 'merged.profdata'
    Invoke-Checked 'merge profile' $profdata (@('merge', '-o', $merged) + $raw.FullName)

    $env:CARGO_TARGET_DIR = Join-Path $work 'use'
    $env:RUSTFLAGS = "$flags -Cprofile-use=$merged"
    Invoke-Checked 'optimised build' 'cargo' $build
    Join-Path $env:CARGO_TARGET_DIR "$target/ship/inf-game.exe"
}
finally {
    Remove-Item Env:CARGO_TARGET_DIR, Env:RUSTFLAGS -ErrorAction SilentlyContinue
    Pop-Location
}
