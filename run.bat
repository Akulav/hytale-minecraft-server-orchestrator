@echo off
rem Hyve - one-click Windows build. Made by Akulav.
rem   run.bat           build the program + setup, copy to dist\, launch
rem   run.bat build     the same without launching
rem   run.bat quick     the program only (no setup), then launch
rem   run.bat headless  build, then run the web UI only (prints its address)
setlocal
cd /d "%~dp0"
set MODE=%1

where cargo >nul 2>nul || set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
where cargo >nul 2>nul || (echo Rust is missing: install it from https://rustup.rs and run this again. & exit /b 1)

echo Building Hyve...
cargo build --release || exit /b 1
if not exist dist mkdir dist
copy /y target\release\hyve.exe dist\Hyve.exe >nul

if /i "%MODE%"=="quick" goto launch
if /i "%MODE%"=="headless" goto headless
set "ISCC=%LOCALAPPDATA%\Programs\Inno Setup 6\ISCC.exe"
if not exist "%ISCC%" set "ISCC=%ProgramFiles(x86)%\Inno Setup 6\ISCC.exe"
if exist "%ISCC%" (
  "%ISCC%" /Q installer\hyve.iss || exit /b 1
  copy /y installer\Output\Hyve-setup.exe dist\ >nul
  echo Setup: dist\Hyve-setup.exe
) else (
  echo Inno Setup 6 not found - skipping the setup ^(winget install JRSoftware.InnoSetup^)
)
if /i "%MODE%"=="build" exit /b 0

:launch
start "" dist\Hyve.exe
exit /b 0

:headless
dist\Hyve.exe --headless
