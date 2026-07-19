# process_manager.py
# Owns the pgrep/pkill lifecycle for the running engine.
# A single ProcessManager instance answers "is it running?" and can toggle it off.
import subprocess

RUNNER_PATTERN = "python.*runner_cross_platform"


class ProcessManager:
    def __init__(self, runner_pattern=RUNNER_PATTERN):
        self.runner_pattern = runner_pattern

    def is_running(self):
        try:
            out = subprocess.run(
                ["pgrep", "-f", self.runner_pattern],
                stdout=subprocess.PIPE,
                stderr=subprocess.DEVNULL,
            )
            return out.returncode == 0
        except FileNotFoundError:
            return False

    def toggle_off(self):
        subprocess.run(["pkill", "-f", self.runner_pattern])
