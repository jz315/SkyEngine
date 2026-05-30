@echo off
setlocal

for /f "usebackq delims=" %%i in (`rustc --print sysroot`) do set "RUST_SYSROOT=%%i"
set "RUST_LLD_LINK=%RUST_SYSROOT%\lib\rustlib\x86_64-pc-windows-msvc\bin\gcc-ld\lld-link.exe"

if not exist "%RUST_LLD_LINK%" (
    echo rust lld-link.exe was not found at "%RUST_LLD_LINK%" 1>&2
    exit /b 1
)

"%RUST_LLD_LINK%" %*
exit /b %ERRORLEVEL%
