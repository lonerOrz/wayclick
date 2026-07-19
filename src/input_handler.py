# input_handler.py
import platform

from linux_input import LinuxInputListener
from windows_input import WindowsInputListener
from macos_input import MacOSInputListener

LISTENERS = {
    "linux": LinuxInputListener,
    "windows": WindowsInputListener,
    "darwin": MacOSInputListener,
}


class InputHandler:
    def __init__(self, play_sound_callback, enable_trackpads=False, system=None):
        self.play_sound = play_sound_callback
        self.enable_trackpads = enable_trackpads
        self.current_platform = (system or platform.system()).lower()
        self.Listener = LISTENERS[self.current_platform]

    def start_listening(self):
        return self.Listener(self.play_sound, self.enable_trackpads).run()
