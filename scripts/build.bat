@echo off
setlocal
set "ROOT=%~dp0.."
set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
set "VSPATH="

if exist "%VSWHERE%" (
  for /f "usebackq tokens=*" %%i in (`"%VSWHERE%" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set "VSPATH=%%i"
)

if defined VSPATH (
  call "%VSPATH%\VC\Auxiliary\Build\vcvars64.bat" >nul
) else (
  call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul
)

set "SDK=%ROOT%\.toolchain\sdk\extracted\c"
if exist "%SDK%\um\x64\kernel32.lib" set "LIB=%SDK%\um\x64;%SDK%\ucrt\x64;%LIB%"

set "BT=%ROOT%\.toolchain\buildtools\extracted\bin\10.0.26100.0\x64"
if exist "%BT%" set "PATH=%BT%;%PATH%"

cd /d "%ROOT%"
cargo build %*
