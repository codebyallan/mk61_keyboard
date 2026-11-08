#!/usr/bin/env bash
#
# install.sh - build and install the `mk61` binary for the current user.
#
# Installs the release binary into ${HOME}/.local/bin and makes sure that
# directory is on PATH. State, backups and the flash guard live under
# ${XDG_STATE_HOME:-$HOME/.local/state}/mk61 and are created on demand, so no
# extra directories need to be prepared here.
#
# Usage:
#   ./install.sh            build + install
#   ./install.sh --no-build skip the build and install an existing binary

set -euo pipefail

BUILD=1
for arg in "$@"; do
    case "$arg" in
        --no-build) BUILD=0 ;;
        -h|--help)
            sed -n '2,18p' "$0"
            exit 0
            ;;
        *)
            echo "unknown option: $arg" >&2
            exit 2
            ;;
    esac
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# When invoked via sudo, target the invoking user's home, not root's.
if [[ -n "${SUDO_USER:-}" && "$(id -u)" -eq 0 ]]; then
    TARGET_USER="$SUDO_USER"
    TARGET_HOME="$(getent passwd "$SUDO_USER" | cut -d: -f6)"
else
    TARGET_USER="$(id -un)"
    TARGET_HOME="$HOME"
fi
INSTALL_DIR="${TARGET_HOME}/.local/bin"
BINARY="${INSTALL_DIR}/mk61"

# Locate cargo (may not be on root's PATH).
CARGO_BIN=""
if command -v cargo >/dev/null 2>&1; then
    CARGO_BIN="$(command -v cargo)"
elif [[ -x "${TARGET_HOME}/.cargo/bin/cargo" ]]; then
    CARGO_BIN="${TARGET_HOME}/.cargo/bin/cargo"
elif [[ -n "${CARGO_HOME:-}" && -x "${CARGO_HOME}/bin/cargo" ]]; then
    CARGO_BIN="${CARGO_HOME}/bin/cargo"
fi

if [[ "$BUILD" -eq 1 ]]; then
    if [[ -z "$CARGO_BIN" ]]; then
        echo "cargo not found; install Rust first (https://rustup.rs)" >&2
        exit 1
    fi
    echo "Building mk61 (release)..."
    if [[ "$(id -u)" -eq 0 && -n "${SUDO_USER:-}" ]]; then
        sudo -u "$SUDO_USER" "$CARGO_BIN" build --release --manifest-path "${SCRIPT_DIR}/Cargo.toml"
    else
        "$CARGO_BIN" build --release --manifest-path "${SCRIPT_DIR}/Cargo.toml"
    fi
fi

SOURCE_BINARY="${SCRIPT_DIR}/target/release/mk61"
if [[ ! -x "$SOURCE_BINARY" ]]; then
    echo "release binary not found at ${SOURCE_BINARY}" >&2
    echo "run without --no-build first" >&2
    exit 1
fi

mkdir -p "$INSTALL_DIR"
install -m 0755 "$SOURCE_BINARY" "$BINARY"
if [[ "$(id -u)" -eq 0 && -n "${SUDO_USER:-}" ]]; then
    chown "$SUDO_USER:" "$INSTALL_DIR" "$BINARY"
fi
echo "Installed ${BINARY}"

# Install a udev rule so the current user can open the hidraw node without
# root. Requires root privileges; skipped (with a warning) otherwise.
UDEV_RULE="/etc/udev/rules.d/99-mk61.rules"
UDEV_CONTENT='SUBSYSTEM=="hidraw", ATTRS{idVendor}=="1ea7", ATTRS{idProduct}=="9018", TAG+="uaccess", MODE="0666"
SUBSYSTEM=="hidraw", ENV{ID_VENDOR_ID}=="1ea7", ENV{ID_MODEL_ID}=="9018", TAG+="uaccess", MODE="0666"'

install_udev() {
    if [[ "$(id -u)" -eq 0 ]]; then
        printf '%s\n' "$UDEV_CONTENT" > "$UDEV_RULE"
        if command -v udevadm >/dev/null 2>&1; then
            udevadm control --reload-rules 2>/dev/null || true
            udevadm trigger --subsystem-match=hidraw 2>/dev/null || true
        fi
        # Apply permissions immediately to already-connected devices.
        for node in /dev/hidraw*; do
            [[ -e "$node" ]] || continue
            name="$(basename "$node")"
            uevent="/sys/class/hidraw/${name}/device/uevent"
            if [[ -r "$uevent" ]] && grep -qi "1EA7" "$uevent" 2>/dev/null; then
                chmod 666 "$node" 2>/dev/null || true
            fi
        done
        echo "Installed udev rule ${UDEV_RULE}"
    elif command -v sudo >/dev/null 2>&1 && sudo -n true 2>/dev/null; then
        echo "$UDEV_CONTENT" | sudo tee "$UDEV_RULE" >/dev/null
        if command -v udevadm >/dev/null 2>&1; then
            sudo udevadm control --reload-rules 2>/dev/null || true
            sudo udevadm trigger --subsystem-match=hidraw 2>/dev/null || true
        fi
        echo "Installed udev rule ${UDEV_RULE} (via sudo)"
    else
        echo
        echo "WARNING: could not install the udev rule (no root/sudo)."
        echo "Run as root or install sudo, then re-run: sudo $0"
        echo "Or manually create ${UDEV_RULE} with:"
        echo "    ${UDEV_CONTENT}"
    fi
}

install_udev

# Make sure the install directory is reachable on PATH.
case ":${PATH}:" in
    *":${INSTALL_DIR}:"*) ;;
    *)
        echo
        echo "NOTE: ${INSTALL_DIR} is not on your PATH."
        echo "Add this line to your shell profile (~/.bashrc, ~/.zshrc, ...):"
        echo "    export PATH=\"${INSTALL_DIR}:\$PATH\""
        ;;
esac

echo
echo "Verify with: mk61 --version"
echo "Uninstall with: ${SCRIPT_DIR}/uninstall.sh"
