#!/usr/bin/env bash
#
# Build and run swarm-demo on Linux or macOS. run.ps1 is the Windows
# twin: same flags, same defaults, same output.
#
#   ./run.sh                                     a window, release profile
#   ./run.sh --headless                          no window: render, screenshot, exit
#   ./run.sh --debug                             the dev profile
#   ./run.sh --build-only                        build, do not run
#   ./run.sh --test                              the suites, then build and run
#   ./run.sh --triple <triple>                   cross compile for another platform
#   ./run.sh -- --hull karisen_cruiser --motes 5000
#
# Everything after -- goes to swarm_app itself: --motes, --hull, --frames,
# --out, --size, --zoom, --chewers, --target x,y,z. The first token this
# script does not know starts the passthrough too, which is what run.ps1 has
# to do because PowerShell eats the --.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$root"

profile=release
run=1
headless=0
suites=0
triple=""
app_args=()

usage() {
    awk 'NR <= 2 { next } /^#/ { sub(/^# ?/, ""); print; next } { exit }' "${BASH_SOURCE[0]}"
}

while [ $# -gt 0 ]; do
    case "$1" in
        --debug)      profile=debug ;;
        --release)    profile=release ;;
        --build-only) run=0 ;;
        --headless)   headless=1 ;;
        --test)       suites=1 ;;
        --triple)     triple="${2:-}"; [ -n "$triple" ] || { echo "run.sh: --triple needs a target triple" >&2; exit 2; }; shift ;;
        -h|--help)    usage; exit 0 ;;
        --)           shift; while [ $# -gt 0 ]; do app_args+=("$1"); shift; done; break ;;
        *)            while [ $# -gt 0 ]; do app_args+=("$1"); shift; done; break ;;
    esac
    shift
done

say()  { printf '\033[36m==\033[0m %s\n' "$*"; }
warn() { printf '\033[33m!!\033[0m %s\n' "$*" >&2; }

# ------------------------------------------------------------ the toolchain --

if ! command -v cargo >/dev/null 2>&1; then
    echo "run.sh: no cargo on PATH. Install Rust from https://rustup.rs and reopen the shell." >&2
    exit 1
fi

os="$(uname -s)"
case "$os" in
    Linux)
        # Bevy links alsa and udev on Linux and wants a Vulkan driver at run
        # time. Missing headers fail the build with a linker error that does
        # not name the package, so say the package here instead.
        if command -v pkg-config >/dev/null 2>&1; then
            missing=""
            for lib in alsa libudev; do
                pkg-config --exists "$lib" || missing="$missing $lib"
            done
            if [ -n "$missing" ]; then
                warn "pkg-config cannot find:$missing"
                warn "  Debian, Ubuntu: sudo apt install libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev mesa-vulkan-drivers"
                warn "  Fedora:         sudo dnf install alsa-lib-devel systemd-devel wayland-devel libxkbcommon-devel mesa-vulkan-drivers"
                warn "  Arch:           sudo pacman -S alsa-lib systemd-libs wayland libxkbcommon vulkan-radeon"
            fi
        fi
        if [ "$headless" = 1 ] && ! ls /usr/share/vulkan/icd.d/*.json >/dev/null 2>&1; then
            warn "no Vulkan ICD in /usr/share/vulkan/icd.d. Headless still needs a device."
            warn "  With no GPU, mesa-vulkan-drivers gives lavapipe, which is correct and slow."
        fi
        ;;
    Darwin)
        # Metal through wgpu, nothing to install past the command line tools.
        if ! xcode-select -p >/dev/null 2>&1; then
            warn "no Xcode command line tools. Run: xcode-select --install"
        fi
        ;;
    *)
        warn "unrecognised uname '$os'. Carrying on, but this script is written for Linux and macOS."
        ;;
esac

# The rustup target is the easy half of a cross build. The linker is the other
# half, and cargo does not say so when it fails.
if [ -n "$triple" ]; then
    if command -v rustup >/dev/null 2>&1 && ! rustup target list --installed | grep -qx "$triple"; then
        warn "$triple is not installed. Run: rustup target add $triple"
    fi
    host="$(rustc -vV | sed -n 's/^host: //p')"
    host_os="$(echo "$host" | cut -d- -f3)"
    want_os="$(echo "$triple" | cut -d- -f3)"
    if [ "$host_os" != "$want_os" ]; then
        warn "cross compiling from $host_os to $want_os needs a linker and system libraries for the"
        warn "  target, which rustup does not install. cargo-zigbuild covers Linux and Windows;"
        warn "  macOS needs a Mac. Building on each machine is the path this script is written for."
    fi
fi

# ------------------------------------------------------------- the suites --

if [ "$suites" = 1 ]; then
    say "cargo test -p swarm_core"
    cargo test -p swarm_core
    if command -v python3 >/dev/null 2>&1; then
        say "the chitin has not drifted"
        python3 tools/make_chitin_texture.py --check
    else
        warn "no python3, skipping the chitin check"
    fi
fi

# -------------------------------------------------------------- the build --

build=(cargo build -p swarm_app)
[ "$profile" = release ] && build+=(--release)
[ -n "$triple" ] && build+=(--target "$triple")

say "${build[*]}"
"${build[@]}"

out="target"
[ -n "$triple" ] && out="$out/$triple"
out="$out/$profile/swarm_app"
case "${triple:-$(rustc -vV | sed -n 's/^host: //p')}" in *windows*) out="$out.exe" ;; esac

say "built $out"

# ---------------------------------------------------------------- the run --

if [ "$run" = 0 ]; then
    exit 0
fi

if [ -n "$triple" ] && [ "$triple" != "$(rustc -vV | sed -n 's/^host: //p')" ]; then
    say "cross compiled for $triple, so not running it here"
    exit 0
fi

cmd=("$out")
[ "$headless" = 1 ] && cmd+=(--headless)
cmd+=(${app_args[@]+"${app_args[@]}"})

say "${cmd[*]}"
exec "${cmd[@]}"
