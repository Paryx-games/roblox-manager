@echo off
setlocal

rem run from the script's own folder so relative paths work from anywhere
pushd "%~dp0"
if errorlevel 1 (
    echo Could not switch to the script folder.
    set "result=1"
    goto :finish
)

node ram_ui/scripts/tauri.mjs version:bump
set "result=%errorlevel%"
popd

if not "%result%"=="0" (
    echo.
    echo Version bump failed. See the step and error above.
    echo Node.js, Cargo and pnpm must be installed.
)

:finish
rem only pause on double-click so terminal runs don't get stuck
echo %cmdcmdline% | find /i "/c" >nul && pause
exit /b %result%