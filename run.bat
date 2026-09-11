@echo off
REM Simple double-click launcher for swarm-demo on Windows.
REM Builds the release binary if needed, then runs it.
REM For headless mode, cross builds, or test runs, use .\run.ps1 instead.

cd /d "%~dp0"

echo Building. The last step is LINKING swarm_app.exe, which is a very large
echo link and is the slow part: it can sit on "398/399: swarm_app(bin)" for a
echo while with no output. That is the linker working, not a hang. See
echo .cargo\config.toml for how to make it much faster.
echo.

cargo build --release -p swarm_app
if errorlevel 1 (
    echo Build failed.
    pause
    exit /b 1
)

target\release\swarm_app.exe %*
pause
