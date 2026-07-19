#!/usr/bin/env python3
# wayclick.py - launcher / toggle for the WayClick input-sound engine.
import os
import platform
import subprocess
import sys

CONFIG_ENABLE_TRACKPADS = "false"
RUNNER = os.path.join(
    os.path.dirname(os.path.abspath(__file__)), "runner_cross_platform.py"
)

system = platform.system().lower()

if system == "darwin":
    CONFIG_DIR = os.path.expanduser("~/Library/Application Support/wayclick")
else:
    CONFIG_DIR = os.path.expanduser("~/.config/wayclick")


def notify(title, body):
    subprocess.run(
        ["notify-send", title, body],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )


def is_running():
    try:
        out = subprocess.run(
            ["pgrep", "-f", "python.*runner_cross_platform"],
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
        )
        return out.returncode == 0
    except FileNotFoundError:
        return False


def main():
    # Root check (only meaningful on Linux)
    if system == "linux" and os.geteuid() == 0:
        print("Do not run as root")
        return 1

    # Toggle off if already running
    if is_running():
        subprocess.run(["pkill", "-f", "python.*runner_cross_platform"])
        notify("WayClick", "Disabled")
        return 0

    # Permission check (only meaningful on Linux)
    if system == "linux":
        groups = subprocess.run(
            ["groups"], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL
        ).stdout.decode()
        if "input" not in groups.split():
            notify("WayClick", "User not in input group")
            return 1

    # Config check
    if not os.path.isfile(os.path.join(CONFIG_DIR, "config.json")):
        notify("WayClick", f"Missing config file at {CONFIG_DIR}/config.json")
        return 1

    notify("WayClick", "Enabled")
    env = dict(os.environ, ENABLE_TRACKPADS=CONFIG_ENABLE_TRACKPADS)
    python = os.environ.get("PYTHON", "python3")
    return subprocess.run([python, "-O", RUNNER, CONFIG_DIR], env=env).returncode


if __name__ == "__main__":
    sys.exit(main())
