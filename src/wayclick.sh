#!/usr/bin/env bash
set -euo pipefail

APP_NAME="wayclick"
CONFIG_ENABLE_TRACKPADS="false"

BASE_DIR="$HOME/.cache/wayclick"
RUNNER_SCRIPT="$(dirname "$0")/runner_cross_platform.py"

# Determine config directory based on platform
if [[ "$OSTYPE" == "linux-gnu"* ]]; then
    CONFIG_DIR="$HOME/.config/wayclick"
elif [[ "$OSTYPE" == "darwin"* ]]; then
    CONFIG_DIR="$HOME/Library/Application Support/wayclick"
else
    # Default to Linux-style config directory
    CONFIG_DIR="$HOME/.config/wayclick"
fi

cleanup() {
    tput cnorm 2>/dev/null || true
}
trap cleanup EXIT INT TERM

# Root check (only applicable on Linux)
if [[ $EUID -eq 0 ]] && [[ "$OSTYPE" == "linux-gnu"* ]]; then
    echo "Do not run as root"
    exit 1
fi

# Toggle
if pgrep -f "python.*runner_cross_platform" >/dev/null; then
    pkill -f "python.*runner_cross_platform"
    notify-send "WayClick" "Disabled" 2>/dev/null || true
    exit 0
fi

# Permission check (only applicable on Linux)
if [[ "$OSTYPE" == "linux-gnu"* ]]; then
    if ! groups | grep -q '\binput\b'; then
        notify-send "WayClick" "User not in input group" 2>/dev/null || true
        exit 1
    fi
fi

# Config check
if [[ ! -f "$CONFIG_DIR/config.json" ]]; then
    notify-send "WayClick" "Missing config file at $CONFIG_DIR/config.json"
    exit 1
fi

notify-send "WayClick" "Enabled" 2>/dev/null || true

ENABLE_TRACKPADS="$CONFIG_ENABLE_TRACKPADS" \
  "$PYTHON" -O "$RUNNER_SCRIPT" "$CONFIG_DIR"
