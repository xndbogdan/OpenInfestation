<#
.SYNOPSIS
Put the upscalers' libraries beside the game's executables: FSR's and
XeSS's as they ship, and the NGX bridge (inf_ngx.dll) built for DLSS.

.DESCRIPTION
Each upscaler is optional: the game runs without any of them, and one whose
library is missing is unavailable.

- FSR 3.1: amd_fidelityfx_vk.dll from the FidelityFX SDK 1.1.4
  (-FidelityFxDll or INF_FFX_DLL).
- XeSS: libxess.dll from the XeSS SDK (-XessDll or INF_XESS_DLL).
- DLSS: NGX's Vulkan API ships only as an MSVC static library while the game
  builds with the GNU toolchain, so the bridge (ngx-bridge/inf_ngx.cpp) is
  built with Visual Studio's C++ compiler against the DLSS SDK (-DlssSdk or
  INF_DLSS_SDK) and Khronos' Vulkan headers (-VulkanHeaders or
  INF_VULKAN_HEADERS, the folder holding vulkan/vulkan.h); NVIDIA's
  nvngx_dlss.dll and nvngx_dlssd.dll go beside it.

Every source defaults to the repository's .tmp/sdks copies; a missing one
is skipped with a note.
#>
param(
    [string]$DlssSdk = $env:INF_DLSS_SDK,
    [string]$VulkanHeaders = $env:INF_VULKAN_HEADERS,
    [string]$FidelityFxDll = $env:INF_FFX_DLL,
    [string]$XessDll = $env:INF_XESS_DLL,
    [string[]]$Targets = @('target/release', 'target/debug', 'target/debug/deps')
)

$ErrorActionPreference = 'Stop'
$port = Split-Path $PSScriptRoot -Parent
$sdks = Join-Path $port '.tmp/sdks'
if (-not $DlssSdk) { $DlssSdk = Join-Path $sdks 'dlss' }
if (-not $VulkanHeaders) { $VulkanHeaders = Join-Path $sdks 'vulkan-headers/include' }
if (-not $FidelityFxDll) { $FidelityFxDll = Join-Path $sdks 'fidelityfx/PrebuiltSignedDLL/amd_fidelityfx_vk.dll' }
if (-not $XessDll) { $XessDll = Join-Path $sdks 'xess/bin/libxess.dll' }

$libraries = @()
foreach ($library in @($FidelityFxDll, $XessDll)) {
    if (Test-Path $library) { $libraries += $library } else { Write-Output "skipped: no $library" }
}

# The NGX bridge, when the DLSS SDK, the Vulkan headers and Visual Studio are there.
$ngx = Join-Path $DlssSdk 'lib/Windows_x86_64/x64/nvsdk_ngx_s.lib'
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
$studio = if (Test-Path $vswhere) {
    & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
}
if (-not (Test-Path $ngx)) {
    Write-Output "skipped DLSS: no DLSS SDK at $DlssSdk"
} elseif (-not (Test-Path (Join-Path $VulkanHeaders 'vulkan/vulkan.h'))) {
    Write-Output "skipped DLSS: no Vulkan headers at $VulkanHeaders"
} elseif (-not $studio) {
    Write-Output 'skipped DLSS: no Visual Studio with the x64 C++ tools'
} else {
    $vcvars = Join-Path $studio 'VC/Auxiliary/Build/vcvars64.bat'
    $build = Join-Path $port 'target/ngx'
    New-Item -ItemType Directory -Force $build | Out-Null
    $source = Join-Path $port 'ngx-bridge/inf_ngx.cpp'
    $compile = "cl /nologo /LD /MT /O2 /EHsc /std:c++17 /W3 /I`"$DlssSdk/include`" /I`"$VulkanHeaders`" `"$source`" /Fe`"$build/inf_ngx.dll`" /Fo`"$build/`" /link `"$ngx`" advapi32.lib user32.lib"
    cmd /c "call `"$vcvars`" >nul 2>nul && $compile"
    if ($LASTEXITCODE -ne 0) { throw "The NGX bridge did not build ($LASTEXITCODE)" }
    $runtime = Join-Path $DlssSdk 'lib/Windows_x86_64/rel'
    $libraries += (Join-Path $build 'inf_ngx.dll'), (Join-Path $runtime 'nvngx_dlss.dll'), (Join-Path $runtime 'nvngx_dlssd.dll')
}

foreach ($target in $Targets) {
    $directory = Join-Path $port $target
    if (-not (Test-Path $directory)) { continue }
    Copy-Item $libraries $directory -Force
    Write-Output "$($libraries.Count) libraries -> $directory"
}
