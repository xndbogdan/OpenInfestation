<#
.SYNOPSIS
Compile the Vulkan renderer's GLSL shaders to the SPIR-V the port embeds.

.DESCRIPTION
The port includes crates/inf-game/shaders/*.spv at build time, so building
needs no Vulkan SDK; only editing a shader does. Run this after changing a
.vert, .frag or .comp file, and commit both. It needs glslc from the Vulkan SDK
(VULKAN_SDK, or the newest C:\VulkanSDK\<version>).
#>
$ErrorActionPreference = 'Stop'
$sdk = $env:VULKAN_SDK
if (-not $sdk) { $sdk = [Environment]::GetEnvironmentVariable('VULKAN_SDK', 'Machine') }
if (-not $sdk) { $sdk = (Get-ChildItem 'C:\VulkanSDK' -Directory | Sort-Object Name | Select-Object -Last 1).FullName }
$glslc = Join-Path $sdk 'Bin\glslc.exe'
if (-not (Test-Path $glslc)) { throw "glslc not found under $sdk; install the Vulkan SDK" }
$shaders = Join-Path $PSScriptRoot '..\crates\inf-game\shaders'
Get-ChildItem $shaders -Include *.vert, *.frag, *.comp -File -Recurse | ForEach-Object {
    $out = "$($_.FullName).spv"
    & $glslc --target-env=vulkan1.3 -O $_.FullName -o $out
    if ($LASTEXITCODE -ne 0) { throw "glslc failed on $($_.Name)" }
    Write-Output "compiled $($_.Name)"
}
# The trace's source also builds the path pass (its PATHS_PASS main).
$trace = Join-Path $shaders 'pt_trace.comp'
& $glslc --target-env=vulkan1.3 -O -DPATHS_PASS $trace -o (Join-Path $shaders 'pt_paths.comp.spv')
if ($LASTEXITCODE -ne 0) { throw 'glslc failed on pt_trace.comp as the path pass' }
Write-Output 'compiled pt_trace.comp as pt_paths.comp.spv'
# And the emitters' pass (its EMITTERS_PASS main).
& $glslc --target-env=vulkan1.3 -O -DEMITTERS_PASS $trace -o (Join-Path $shaders 'pt_emitters.comp.spv')
if ($LASTEXITCODE -ne 0) { throw 'glslc failed on pt_trace.comp as the emitters pass' }
Write-Output 'compiled pt_trace.comp as pt_emitters.comp.spv'
# The denoiser's glossy passes (their GLOSSY_PASS mains).
foreach ($pass in 'pt_temporal', 'pt_atrous') {
    & $glslc --target-env=vulkan1.3 -O -DGLOSSY_PASS (Join-Path $shaders "$pass.comp") -o (Join-Path $shaders "${pass}_glossy.comp.spv")
    if ($LASTEXITCODE -ne 0) { throw "glslc failed on $pass.comp as its glossy pass" }
    Write-Output "compiled $pass.comp as ${pass}_glossy.comp.spv"
}
