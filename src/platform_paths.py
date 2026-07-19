# platform_paths.py
# Single source of truth for the per-platform config directory.
# Both the launcher (wayclick.py) and the entry point (runner_cross_platform.py)
# resolve the config dir through config_dir(); no platform branch lives anywhere else.
import os


def config_dir(system=None):
    system = (system or __import__("platform").system()).lower()
    if system == "darwin":
        return os.path.expanduser("~/Library/Application Support/wayclick")
    if system == "windows":
        return os.path.expanduser("~\\.wayclick")
    return os.path.expanduser("~/.config/wayclick")
