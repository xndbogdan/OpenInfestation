@echo off
setlocal DisableDelayedExpansion
rem Keep the GCC driver: direct rust-lld also changes CRT and native-library lookup.
rem rustup propagates the selected toolchain, including cargo +toolchain invocations.
set "V2K_LINK_RUSTC=rustc"
if defined RUSTC set "V2K_LINK_RUSTC=%RUSTC%"
set "V2K_LINK_SYSROOT="
for /f "delims=" %%R in ('""%V2K_LINK_RUSTC%" --print sysroot"') do set "V2K_LINK_SYSROOT=%%R"
if not defined V2K_LINK_SYSROOT (
    >&2 echo Cannot locate the active Rust sysroot. Check RUSTC or rustc on PATH.
    exit /b 1
)
set "V2K_LINK_LLD_DIR=%V2K_LINK_SYSROOT%\lib\rustlib\x86_64-pc-windows-gnu\bin\gcc-ld"
if not exist "%V2K_LINK_LLD_DIR%\ld.lld.exe" (
    >&2 echo Rust's bundled GNU LLD is missing: "%V2K_LINK_LLD_DIR%\ld.lld.exe"
    exit /b 1
)
x86_64-w64-mingw32-gcc -fuse-ld=lld "-B%V2K_LINK_LLD_DIR%" %*
exit /b %errorlevel%
