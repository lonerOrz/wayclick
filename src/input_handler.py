# input_handler.py
import platform

current_platform = platform.system().lower()

class InputHandler:
    def __init__(self, play_sound_callback):
        self.play_sound = play_sound_callback
        self.current_platform = current_platform
        
        if self.current_platform == "linux":
            import evdev
            from evdev import ecodes
            self.evdev = evdev
            self.ecodes = ecodes
        elif self.current_platform == "windows":
            import ctypes
            from ctypes import wintypes
            self.ctypes = ctypes
            self.wintypes = wintypes
        elif self.current_platform == "darwin":  # macOS
            import Quartz
            self.Quartz = Quartz

    def start_listening(self):
        if self.current_platform == "linux":
            return self._start_linux()
        elif self.current_platform == "windows":
            return self._start_windows()
        elif self.current_platform == "darwin":
            return self._start_macos()

    def _start_linux(self):
        import asyncio
        import signal
        from .linux_input import LinuxInputListener
        listener = LinuxInputListener(self.play_sound, self.evdev, self.ecodes)
        return listener.run()

    def _start_windows(self):
        from .windows_input import WindowsInputListener
        listener = WindowsInputListener(self.play_sound, self.ctypes, self.wintypes)
        return listener.run()

    def _start_macos(self):
        from .macos_input import MacOSInputListener
        listener = MacOSInputListener(self.play_sound, self.Quartz)
        return listener.run()