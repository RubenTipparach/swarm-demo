@echo off
REM Simple double-click launcher for swarm-demo on Windows.
REM Builds the release binary if needed, then runs it.
REM For headless mode, cross builds, or test runs, use .\run.ps1 instead.

cd /d "%~dp0"

cargo build --release -p swarm_app
if errorlevel 1 (
    echo Build failed.
    pause
    exit /b 1
)

target\release\swarm_app.exe %*
pause
