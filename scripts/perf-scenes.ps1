# Times a fixed set of traced scenes (Vulkan, path tracing) and prints each
# run's timing lines: the tracer's GPU stages and the device's frames (CPU,
# GPU wait, GPU). With INF_CPU_PROFILE=1 the runs also print the CPU profile
# (crate::cpu_profile). See docs/PERFORMANCE.md.
param(
    [Parameter(Mandatory)] [string]$Settings,
    [Parameter(Mandatory)] [string]$Profiles,
    [string]$Exe = (Join-Path $PSScriptRoot '..\target\release\inf-game.exe'),
    [string]$DataRoot = (Join-Path $PSScriptRoot '..\retail'),
    [string]$WorkDir = (Join-Path $PSScriptRoot '..\.tmp\perf'),
    [int]$Width = 1920,
    [int]$Height = 1080,
    [int]$Frames = 400,
    [string]$Tag = 'base',
    [string[]]$Only = @(),
    [switch]$Snapshot
)
$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Force $WorkDir | Out-Null
Push-Location $WorkDir
$scenes = [ordered]@{
    'pelagia-rain'  = @{ start = '{"world":1,"start":0,"form":1,"position":[32400,28200],"heading":90,"known":"all","hints_seen":"all","weather":true}' }
    'pelagia-dry'   = @{ start = '{"world":1,"start":0,"form":1,"position":[32400,28200],"heading":90,"known":"all","hints_seen":"all","weather":false}' }
    'pelagia-night' = @{ start = '{"world":1,"start":0,"form":1,"heading":0,"known":"all","hints_seen":"all","upgrades":[20],"day_clock_ms":900000,"weather":false}'; replay = 'keys-lamp.json' }
    'pelagia-ir'    = @{ start = '{"world":1,"start":0,"form":1,"position":[32400,27600],"heading":270,"known":"all","hints_seen":"all","weather":true,"upgrades":[61]}'; replay = 'keys-lamp.json' }
    'pelagia-drive' = @{ start = '{"world":1,"start":0,"form":1,"position":[32400,28200],"heading":180,"known":"all","hints_seen":"all","weather":true}'; replay = 'keys-drive.json' }
    'terra-nova'    = @{ start = '{"world":2,"start":0,"form":1,"known":"all","hints_seen":"all"}' }
    'stendahl'      = @{ start = '{"world":3,"start":0,"form":1,"known":"all","hints_seen":"all"}' }
    'artoria'       = @{ start = '{"world":11,"start":0,"form":1,"known":"all","hints_seen":"all"}' }
    'foldrak-night' = @{ start = '{"world":14,"start":0,"form":1,"known":"all","hints_seen":"all","upgrades":[20],"day_clock_ms":900000}'; replay = 'keys-lamp.json' }
    'lava'          = @{ start = '{"world":17,"start":0,"form":1,"known":"all","hints_seen":"all"}' }
    # Sinks into Tabuchi's deep water: under the Remastered camera (settings
    # native word 27 = 2) the eye follows it under from the first frames.
    'tabuchi-deep'  = @{ start = '{"world":9,"start":0,"form":1,"position":[51978,37383],"heading":0,"known":"all","hints_seen":"all","weather":true,"upgrades":[41]}' }
}
[IO.File]::WriteAllText("$WorkDir\keys-lamp.json", '[{"frame":30,"key":"L","pressed":true},{"frame":34,"key":"L","pressed":false}]')
[IO.File]::WriteAllText("$WorkDir\keys-drive.json", '[{"frame":40,"key":"Up","pressed":true},{"frame":120,"key":"Z","pressed":true},{"frame":400,"key":"Z","pressed":false},{"frame":400,"key":"Up","pressed":false}]')
$rows = @()
foreach ($name in $scenes.Keys) {
    if ($Only.Count -gt 0 -and $name -notin $Only) { continue }
    $scene = $scenes[$name]
    $start = "$name.json"
    [IO.File]::WriteAllText("$WorkDir\$start", $scene.start)
    $out = "$Tag-$name"
    $a = @('--no-launcher', '--start', $start, '--data-root', $DataRoot, '--settings', $Settings,
        '--profiles', $Profiles, '--renderer', 'vulkan', '--hidden', '--width', "$Width", '--height', "$Height",
        '--fixed-step-micros', '16000', '--frames', "$Frames")
    if ($Snapshot) { $a += @('--snapshot-at', "$($Frames - 10):$out.png") }
    if ($scene.ContainsKey('replay')) { $a += @('--input-replay', $scene.replay) }
    $clock = [Diagnostics.Stopwatch]::StartNew()
    Start-Process -FilePath $Exe -ArgumentList $a -RedirectStandardOutput "$out.out" -RedirectStandardError "$out.err" -Wait -NoNewWindow
    $wall = $clock.Elapsed.TotalSeconds
    $lines = Get-Content "$out.err" | Where-Object { $_ -match '^Vulkan (tracer|frames)' }
    $rows += [pscustomobject]@{ scene = $name; wall = [math]::Round($wall, 1); lines = ($lines -join "`n") }
}
foreach ($row in $rows) { "== $($row.scene) (wall $($row.wall) s)"; $row.lines }
Pop-Location
