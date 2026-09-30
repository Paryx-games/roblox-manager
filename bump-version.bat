@echo off
setlocal
pushd "%~dp0"
if errorlevel 1 exit /b 1
node ram_ui/scripts/tauri.mjs version:bump
set "bumpResult=%errorlevel%"
popd
if not "%bumpResult%"=="0" echo Version bump failed. See the step and error above. Node.js, Cargo and pnpm must be installed.
exit /b %bumpResult%
