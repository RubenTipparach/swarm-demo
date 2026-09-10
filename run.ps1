#
# Build and run swarm-demo on Windows. run.sh is the Linux and macOS
# twin: same flags, same defaults, same output.
#
#   .\run.ps1                            a window, release profile
#   .\run.ps1 --headless                 no window: render, screenshot, exit
#   .\run.ps1 --debug                    the dev profile
#   .\run.ps1 --build-only               build, do not run
#   .\run.ps1 --test                     the suites, then build and run
#   .\run.ps1 --triple <triple>          cross compile for another platform
#   .\run.ps1 -- --hull karisen_cruiser --motes 5000
#
# Everything after -- goes to swarm_app itself: --motes, --hull, --frames,
# --out, --size, --zoom, --chewers, --target x,y,z. PowerShell eats the --,
# so the first token this script does not know starts the passthrough too.

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSCommandPath
Set-Location $root

$profileName = 'release'
$run = $true
$headless = $false
$suites = $false
$triple = ''
$appArgs = @()

function Show-Usage {
    $lines = @()
    foreach ($line in Get-Content $PSCommandPath) {
        if ($line -notmatch '^#') { break }
        $lines += ($line -replace '^# ?', '')
    }
    ($lines -join [Environment]::NewLine).Trim()
}

$i = 0
$stop = $false
while ($i -lt $args.Count -and -not $stop) {
    $arg = [string] $args[$i]
    if ($arg -eq '--debug')           { $profileName = 'debug' }
    elseif ($arg -eq '--release')     { $profileName = 'release' }
    elseif ($arg -eq '--build-only')  { $run = $false }
    elseif ($arg -eq '--headless')    { $headless = $true }
    elseif ($arg -eq '--test')        { $suites = $true }
    elseif ($arg -eq '--triple') {
        $i++
        if ($i -ge $args.Count) { Write-Host 'run.ps1: --triple needs a target triple' -ForegroundColor Red; exit 2 }
        $triple = [string] $args[$i]
    }
    elseif ($arg -eq '-h' -or $arg -eq '--help' -or $arg -eq '/?') { Show-Usage; exit 0 }
    elseif ($arg -eq '--') { $stop = $true; $i++; break }
    else { $stop = $true; break }
    $i++
}
while ($i -lt $args.Count) { $appArgs += [string] $args[$i]; $i++ }

function Say  { param([string] $m) Write-Host "== $m" -ForegroundColor Cyan }
function Warn { param([string] $m) Write-Host "!! $m" -ForegroundColor Yellow }

# ----------------------------------------------------------- the toolchain --

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Host 'run.ps1: no cargo on PATH. Install Rust from https://rustup.rs and reopen the shell.' -ForegroundColor Red
    exit 1
}

$hostTriple = ((& rustc -vV) | Select-String '^host: ') -replace '^host: ', ''

# The MSVC toolchain links with link.exe. rustc finds it through the Visual
# Studio installation and not through PATH, so ask vswhere before saying it is
# missing: warning on PATH alone cries wolf on every working machine. Warn
# rather than fail, because a missing linker is a build error that reads like a
# Rust problem and is not one.
if ($hostTriple -match 'msvc') {
    $linker = [bool] (Get-Command link.exe -ErrorAction SilentlyContinue)
    if (-not $linker) {
        $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
        if (Test-Path $vswhere) {
            $vc = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
            $linker = [bool] $vc
        }
    }
    if (-not $linker) {
        Warn 'no MSVC linker found. Install the Visual Studio Build Tools with the'
        Warn '  "Desktop development with C++" workload, then reopen the shell.'
    }
}

# The rustup target is the easy half of a cross build. The linker is the other
# half, and cargo does not say so when it fails.
if ($triple) {
    $rustup = Get-Command rustup -ErrorAction SilentlyContinue
    if ($rustup) {
        $installed = & rustup target list --installed
        if ($installed -notcontains $triple) { Warn "$triple is not installed. Run: rustup target add $triple" }
    }
    $hostOs = ($hostTriple -split '-')[2]
    $wantOs = ($triple -split '-')[2]
    if ($hostOs -ne $wantOs) {
        Warn "cross compiling from $hostOs to $wantOs needs a linker and system libraries for the"
        Warn '  target, which rustup does not install. cargo-zigbuild covers Linux and Windows;'
        Warn '  macOS needs a Mac. Building on each machine is the path this script is written for.'
    }
}

# ------------------------------------------------------------- the suites --

if ($suites) {
    Say 'cargo test -p swarm_core'
    & cargo test -p swarm_core
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    $python = Get-Command python -ErrorAction SilentlyContinue
    if (-not $python) { $python = Get-Command python3 -ErrorAction SilentlyContinue }
    if ($python) {
        Say 'the chitin has not drifted'
        & $python.Source tools/make_chitin_texture.py --check
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    } else {
        Warn 'no python on PATH, skipping the chitin check'
    }
}

# -------------------------------------------------------------- the build --

$build = @('build', '-p', 'swarm_app')
if ($profileName -eq 'release') { $build += '--release' }
if ($triple) { $build += @('--target', $triple) }

Say "cargo $($build -join ' ')"
& cargo @build
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$out = 'target'
if ($triple) { $out = Join-Path $out $triple }
$out = Join-Path (Join-Path $out $profileName) 'swarm_app'
if (-not $triple -or $triple -match 'windows') { $out = "$out.exe" }

Say "built $out"

# ---------------------------------------------------------------- the run --

if (-not $run) { exit 0 }

if ($triple -and $triple -ne $hostTriple) {
    Say "cross compiled for $triple, so not running it here"
    exit 0
}

$cmd = @()
if ($headless) { $cmd += '--headless' }
$cmd += $appArgs

Say "$out $($cmd -join ' ')"
if ($cmd.Count -gt 0) { & (Join-Path $root $out) @cmd } else { & (Join-Path $root $out) }
exit $LASTEXITCODE
