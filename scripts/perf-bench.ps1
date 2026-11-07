<#
.SYNOPSIS
Times headless gameplay runs for compiler-build comparison.

.DESCRIPTION
Runs each training scene hidden at 1920x1080 with a 16 ms fixed step for
-Frames frames, -Repeats times, and records wall seconds, fps and the end
snapshot under .tmp/perf/<Tag>/. The executable is copied there first, so
cargo can rebuild between runs. -Baseline compares a later tag against an
earlier one and checks the end snapshots are byte-identical: compiler builds
must not change results (docs/PERFORMANCE.md). Run one tag per build
(release ship, PGO).

.EXAMPLE
pwsh -NoProfile -File scripts/perf-bench.ps1 -Tag ship-now
pwsh -NoProfile -File scripts/perf-bench.ps1 -Tag pgo-now -Exe target/pgo/use/x86_64-pc-windows-msvc/ship/inf-game.exe -Baseline ship-now
#>
[CmdletBinding()]
param(
    [string]$DataRoot = 'retail',
    [int]$Frames = 3000,
    [int]$Repeats = 2,
    [string]$Tag = (Get-Date -Format 'yyyyMMdd-HHmmss'),
    [string]$Exe = 'target/ship/inf-game.exe',
    [string]$Scenes = '02,1a,1b,1c,03,11,17,14,1drive',
    [string]$Renderers = 'opengl,vulkan,software',
    [int]$Width = 1920,
    [int]$Height = 1080,
    # Pre-seed the tag's settings.json from this file (e.g. ray tracing off
    # for raster runs); a fresh game-written file otherwise.
    [string]$SettingsSeed = '',
    [string]$Baseline
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$perf = Join-Path $root '.tmp/perf'
$out = Join-Path $perf $Tag
New-Item -ItemType Directory -Force $out | Out-Null
$sceneList = @($Scenes -split ',' | ForEach-Object { $_.Trim() })
$rendererList = @($Renderers -split ',' | ForEach-Object { $_.Trim() })
$settingsFile = Join-Path $out 'settings.json'
$profilesFile = Join-Path $out 'profiles.sav'
# Scene name -> world, extra start fields, input-replay keys, as in
# scripts/build-pgo.ps1.
$sceneDef = @{
    '02'     = @(2, '', '')
    '1a'     = @(1, ',"position":[32400,28200],"heading":90,"weather":false', '')
    '1b'     = @(1, ',"position":[32400,28200],"heading":90,"weather":true', '')
    '1c'     = @(1, ',"upgrades":[20],"day_clock_ms":900000,"weather":false', 'lamp')
    '03'     = @(3, '', '')
    '11'     = @(11, '', '')
    '17'     = @(17, '', '')
    '14'     = @(14, ',"upgrades":[20],"day_clock_ms":900000', 'lamp')
    '1drive' = @(1, ',"position":[32400,28200],"heading":180,"weather":true', 'drive')
}
$keyJson = @{
    drive = '[{"frame":40,"key":"Up","pressed":true},{"frame":120,"key":"Z","pressed":true},{"frame":1250,"key":"Z","pressed":false},{"frame":1250,"key":"Up","pressed":false}]'
    lamp  = '[{"frame":30,"key":"L","pressed":true},{"frame":34,"key":"L","pressed":false}]'
}

if (-not $Baseline) {
    $exeCopy = Join-Path $out 'inf-game.exe'
    Copy-Item -LiteralPath (Join-Path $root $Exe) -Destination $exeCopy -Force
    if ($SettingsSeed) { Copy-Item -LiteralPath $SettingsSeed -Destination $settingsFile -Force }
    $data = if ([IO.Path]::IsPathRooted($DataRoot)) { $DataRoot } else { Join-Path $root $DataRoot }
    $summary = @()
    foreach ($renderer in $rendererList) {
        foreach ($scene in $sceneList) {
            $def = $sceneDef[$scene]
            if (-not $def) { throw "unknown scene '$scene'" }
            $fpsRuns = @()
            for ($rep = 1; $rep -le $Repeats; $rep++) {
                $name = $scene + '_' + $renderer + '_' + $rep
                $start = Join-Path $out ('start' + $name + '.json')
                $snap = Join-Path $out ('end' + $name + '.png')
                $elog = Join-Path $out ('events' + $name + '.jsonl')
                $outlog = Join-Path $out ('out' + $name + '.txt')
                $errlog = Join-Path $out ('err' + $name + '.txt')
                Set-Content -LiteralPath $start -Value ('{"world":' + $def[0] + ',"start":0,"form":1,"known":"all","hints_seen":"all"' + $def[1] + '}') -Encoding ascii
                $arguments = @('--no-launcher', '--start', $start, '--data-root', $data,
                    '--settings', $settingsFile, '--profiles', $profilesFile,
                    '--renderer', $renderer, '--hidden',
                    '--width', "$Width", '--height', "$Height", '--fixed-step-micros', '16000',
                    '--frames', "$Frames", '--snapshot', $snap, '--event-log', $elog)
                if ($def[2]) {
                    $keys = Join-Path $out ('keys' + $name + '.json')
                    [IO.File]::WriteAllText($keys, $keyJson[$def[2]])
                    $arguments += @('--input-replay', $keys)
                }
                $began = Get-Date
                $process = Start-Process -FilePath $exeCopy -PassThru -Wait -NoNewWindow `
                    -RedirectStandardOutput $outlog `
                    -RedirectStandardError $errlog -ArgumentList $arguments
                $seconds = ((Get-Date) - $began).TotalSeconds
                $fps = if ($process.ExitCode -eq 0) { [math]::Round($Frames / $seconds, 1) } else { 0 }
                $fpsRuns += $fps
                Write-Host ('scene {0} {1} rep {2}: exit {3}, {4} s, {5} fps' -f `
                        $scene, $renderer, $rep, $process.ExitCode, [math]::Round($seconds, 1), $fps)
            }
            $sorted = @($fpsRuns | Sort-Object)
            $median = $sorted[[math]::Floor($sorted.Count / 2)]
            $summary += [pscustomobject]@{ scene = $scene; renderer = $renderer; fps = $fpsRuns; median_fps = $median }
            $summary | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $out 'summary.json')
        }
    }
    Write-Host "`nmedian fps:"
    $summary | ForEach-Object { Write-Host ('  scene {0} {1}: {2}' -f $_.scene, $_.renderer, $_.median_fps) }
}
else {
    $old = Get-Content -Raw -LiteralPath (Join-Path $perf "$Baseline/summary.json") | ConvertFrom-Json
    $new = Get-Content -Raw -LiteralPath (Join-Path $out 'summary.json') | ConvertFrom-Json
    $mismatched = @()
    foreach ($row in $new) {
        $before = @($old | Where-Object { $_.scene -eq $row.scene -and $_.renderer -eq $row.renderer })[0]
        $gain = if ($before.median_fps -gt 0) {
            [math]::Round(100.0 * ($row.median_fps - $before.median_fps) / $before.median_fps, 1)
        } else { 0 }
        Write-Host ('scene {0} {1}: {2} -> {3} fps ({4}{5}%)' -f `
                $row.scene, $row.renderer, $before.median_fps, $row.median_fps,
            $(if ($gain -ge 0) { '+' } else { '' }), $gain)
        for ($rep = 1; $rep -le $Repeats) {
            $name = '{0}_{1}_{2}' -f $row.scene, $row.renderer, $rep
            # Snapshots only where the renderer is deterministic (Vulkan
            # differs run to run even for one binary); the event log covers
            # every renderer.
            if ($row.renderer -ne 'vulkan') {
                $a = Join-Path $perf "$Baseline/end$name.png"
                $b = Join-Path $out "end$name.png"
                if ((Test-Path -LiteralPath $a) -and (Test-Path -LiteralPath $b)) {
                    $ha = (Get-FileHash -Algorithm SHA256 -LiteralPath $a).Hash
                    $hb = (Get-FileHash -Algorithm SHA256 -LiteralPath $b).Hash
                    if ($ha -ne $hb) { $mismatched += "snapshot:$name" }
                }
            }
            $ea = Join-Path $perf "$Baseline/events$name.jsonl"
            $eb = Join-Path $out "events$name.jsonl"
            if ((Test-Path -LiteralPath $ea) -and (Test-Path -LiteralPath $eb)) {
                # Skip the first line (it echoes this run's own file paths)
                # and ambience lines (the audio mix runs on the audio clock,
                # so its levels differ run to run even for one binary).
                $sha = [System.Security.Cryptography.SHA256]::Create()
                $norm = { param($p) ((Get-Content -LiteralPath $p) | Select-Object -Skip 1 |
                    Where-Object { $_ -notmatch '"ev":"ambience"' }) -join "`n" }
                $ha = [BitConverter]::ToString($sha.ComputeHash(
                    [Text.Encoding]::UTF8.GetBytes((& $norm $ea))))
                $hb = [BitConverter]::ToString($sha.ComputeHash(
                    [Text.Encoding]::UTF8.GetBytes((& $norm $eb))))
                if ($ha -ne $hb) { $mismatched += "events:$name" }
            }
        }
    }
    if ($mismatched.Count -gt 0) {
        Write-Warning ('differences: ' + ($mismatched -join ', '))
    }
    else {
        Write-Host 'all end snapshots and event logs byte-identical.' -ForegroundColor Green
    }
}
