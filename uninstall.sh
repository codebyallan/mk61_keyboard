#!/usr/bin/env bash
#
# uninstall.sh - remove the user-local `mk61` installation.
#
# Removes the binary from ${HOME}/.local/bin. State, backups and the flash
# guard under ${XDG_STATE_HOME:-$HOME/.local/state}/mk61 are kept by default;
# pass --purge to remove them as well.
#
# Usage:
#   ./uninstall.sh
#   ./uninstall.sh --purge

set -euo pipefail

PURGE=0
for arg in "$@"; do
    case "$arg" in
        --purge) PURGE=1 ;;
        -h|--help)
            sed -n '2,16p' "$0"
            exit 0
            ;;
        *)
            echo "unknown option: $arg" >&2
            exit 2
            ;;
    esac
done

INSTALL_DIR="${HOME}/.local/bin"
BINARY="${INSTALL_DIR}/mk61"

if [[ -e "$BINARY" ]]; then
    rm -f "$BINARY"
    echo "Removed ${BINARY}"
else
    echo "No binary found at ${BINARY}"
fi

STATE_DIR="${XDG_STATE_HOME:-${HOME}/.local/state}/mk61"
if [[ "$PURGE" -eq 1 ]]; then
    if [[ -e "$STATE_DIR" ]]; then
        rm -rf "$STATE_DIR"
        echo "Removed state directory ${STATE_DIR}"
    else
        echo "No state directory found at ${STATE_DIR}"
    fi
else
    if [[ -e "$STATE_DIR" ]]; then
        echo "State kept at ${STATE_DIR} (pass --purge to remove)."
    fi
fi

# Remove the udev rule if present.
UDEV_RULE="/etc/udev/rules.d/99-mk61.rules"
if [[ -e "$UDEV_RULE" ]]; then
    if [[ "$(id -u)" -eq 0 ]]; then
        rm -f "$UDEV_RULE"
    elif command -v sudo >/dev/null 2>&1; then
        sudo rm -f "$UDEV_RULE"
    fi
    if command -v udevadm >/dev/null 2>&1; then
        (udevadm control --reload-rules 2>/dev/null || sudo udevadm control --reload-rules 2>/dev/null || true)
    fi
    echo "Removed udev rule ${UDEV_RULE}"
fi

echo "Uninstall complete."
