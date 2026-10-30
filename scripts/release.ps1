[CmdletBinding()]
param(
    # Create the GitHub Release from the packages. Without it the packages are
    # only built, under target/dist/v<version>.
    [switch]$Publish,
    # Build only the Windows package, for example when Docker isn't available.
    [switch]$SkipLinux,
    # The sound folders the pak's manifest names (ambience/manifest.json).
    [string]$Packs4 = $env:INF_SOUNDS_PACKS4,
    [string]$Splice = $env:INF_SOUNDS_SPLICE
)

# Builds the release packages locally, as OpenV2K's release.ps1 does:
# OpenInfestation-v<version>-windows-x64.zip, OpenInfestation-v<version>-linux-x64.tar.gz
# and SHA256SUMS.txt. Both carry Remastered.pak, built from your sound folders
# with only the sounds a release may ship; the Windows zip also carries the
# upscalers' libraries (scripts/build-upscalers.ps1, from .tmp/sdks). The
# Linux package is built in an ubuntu:22.04 Docker container
# (scripts/release-linux.sh). Versions with a suffix (0.2.0-rc1) are published
# as pre-releases.

$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path -Parent $PSScriptRoot

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
    $manifest = Get-Content -Raw 'crates/inf-game/Cargo.toml'
    if ($manifest -notmatch '(?m)^version = "([^"]+)"') {
        throw 'No version in crates/inf-game/Cargo.toml.'
    }
    $version = $Matches[1]
    $tag = "v$version"
    $commit = (git rev-parse HEAD).Trim()

    if ($Publish) {
        if (git status --porcelain) {
            throw 'The working tree has changes; publish from a clean checkout of main.'
        }
        Invoke-Checked 'fetch' 'git' @('fetch', '--quiet', '--tags', 'origin', 'main')
        if ($commit -ne (git rev-parse origin/main).Trim()) {
            throw 'HEAD is not origin/main; publish what main holds.'
        }
        git rev-parse -q --verify "refs/tags/$tag" > $null
        if ($LASTEXITCODE -eq 0) {
            throw "$tag is already released; bump crates/inf-game's version to publish."
        }
        if ($SkipLinux) {
            throw 'A published release needs the Linux package; drop -SkipLinux.'
        }
    }
    foreach ($root in @(@('packs4', $Packs4), @('splice', $Splice))) {
        if (-not $root[1] -or -not (Test-Path $root[1])) {
            throw "No $($root[0]) sound folder ('$($root[1])'); pass -$($root[0]) or set INF_SOUNDS_$($root[0].ToUpper())."
        }
    }

    $out = Join-Path $repositoryRoot "target/dist/$tag"
    if (Test-Path $out) { Remove-Item -Recurse -Force $out }
    New-Item -ItemType Directory -Force $out | Out-Null

    # The pak a release carries: no sound whose licence forbids shipping it.
    $pak = Join-Path $out 'Remastered.pak'
    Invoke-Checked 'Remastered.pak' 'python' @(
        '-I', 'scripts/build-remastered-pak.py', '--release', '--out', $pak,
        '--root', "packs4=$Packs4", '--root', "splice=$Splice"
    )

    # cargo-about 0.9 builds its CLI only with the `cli` feature.
    if (-not (Get-Command cargo-about -ErrorAction SilentlyContinue)) {
        Invoke-Checked 'install cargo-about' 'cargo' @(
            'install', 'cargo-about', '--locked', '--version', '0.9.2', '--features', 'cli'
        )
    }
    $licenses = Join-Path $out 'THIRD_PARTY_LICENSES.html'
    Invoke-Checked 'third-party licenses' 'cargo' @(
        'about', 'generate', '--fail', '-o', $licenses, 'about.hbs'
    )

    # Windows: the MinGW-w64 GNU toolchain, as for local builds. MSYS2's GCC 15
    # defaults to C23, which SDL2's bundled sources reject.
    $env:CFLAGS = '-std=gnu17'
    Invoke-Checked 'build (Windows)' 'cargo' @('build', '--release', '-p', 'inf-game')
    $upscalers = Join-Path $out 'upscalers'
    New-Item -ItemType Directory $upscalers | Out-Null
    Invoke-Checked 'upscalers' 'pwsh' @(
        '-NoProfile', '-File', 'scripts/build-upscalers.ps1', '-Targets', "target/dist/$tag/upscalers"
    )
    $libraries = 'amd_fidelityfx_vk.dll', 'libxess.dll', 'inf_ngx.dll', 'nvngx_dlss.dll', 'nvngx_dlssd.dll'
    foreach ($library in $libraries) {
        if (-not (Test-Path (Join-Path $upscalers $library))) {
            throw "The upscalers' $library is missing; see scripts/build-upscalers.ps1."
        }
    }
    $windowsName = "OpenInfestation-$tag-windows-x64"
    $windowsDir = Join-Path $out $windowsName
    New-Item -ItemType Directory $windowsDir | Out-Null
    Copy-Item README.md, DISCLOSURE.md, LICENSE-MIT, LICENSE-APACHE, $licenses, $pak $windowsDir
    Copy-Item ($libraries | ForEach-Object { Join-Path $upscalers $_ }) $windowsDir
    Copy-Item 'target/release/inf-game.exe' (Join-Path $windowsDir 'OpenInfestation.exe')
    Push-Location $out
    try {
        Invoke-Checked 'package (Windows)' (Join-Path $env:SystemRoot 'System32/tar.exe') @('-a', '-c', '-f', "$windowsName.zip", $windowsName)
    }
    finally {
        Pop-Location
    }
    Remove-Item -Recurse -Force $windowsDir, $upscalers

    if (-not $SkipLinux) {
        # Named volumes keep the toolchain and build cache between releases.
        Invoke-Checked 'build and package (Linux)' 'docker' @(
            'run', '--rm',
            '-v', "${repositoryRoot}:/src",
            '-v', 'openinfestation-release-cargo:/root/.cargo',
            '-v', 'openinfestation-release-target:/target',
            '-e', 'CARGO_TARGET_DIR=/target',
            '-e', "RELEASE_NAME=OpenInfestation-$tag-linux-x64",
            '-e', "RELEASE_OUT=/src/target/dist/$tag",
            'ubuntu:22.04',
            'bash', '-c', "tr -d '\r' < /src/scripts/release-linux.sh | bash"
        )
    }

    $packages = Get-ChildItem $out -File | Where-Object Name -like 'OpenInfestation-*' | Sort-Object Name
    $sums = $packages | ForEach-Object {
        "$((Get-FileHash -Algorithm SHA256 $_.FullName).Hash.ToLowerInvariant())  $($_.Name)"
    }
    [IO.File]::WriteAllText((Join-Path $out 'SHA256SUMS.txt'), (($sums -join "`n") + "`n"))
    Remove-Item $licenses, $pak

    if ($Publish) {
        $arguments = @('release', 'create', $tag, '--target', $commit, '--title', $tag, '--generate-notes')
        if ($version.Contains('-')) { $arguments += '--prerelease' }
        $arguments += @($packages.FullName) + (Join-Path $out 'SHA256SUMS.txt')
        Invoke-Checked "publish $tag" 'gh' $arguments
    }

    Write-Host "`nRelease packages for $tag are in $out." -ForegroundColor Green
}
finally {
    Pop-Location
}
