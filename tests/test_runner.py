import os
import sys
import json
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "src"))


class FakeSound:
    def __init__(self, name):
        self.name = name
        self.plays = 0

    def play(self):
        self.plays += 1


# Stub pygame before the runner imports it at module load time
fake_pygame = mock.MagicMock()
# Sound() returns a FakeSound keyed by the path so .play() is observable
fake_pygame.mixer.Sound.side_effect = lambda p: FakeSound(p)
# pygame.error must be a real exception class so `except pygame.error` works
fake_pygame.error = Exception
sys.modules["pygame"] = fake_pygame
sys.modules["pygame.mixer"] = fake_pygame.mixer

# Prevent the runner's import-time config load from touching the real FS:
# point ASSET_DIR at a temp dir and give it a minimal config.
_tmp = tempfile.mkdtemp()
with open(os.path.join(_tmp, "config.json"), "w") as _f:
    json.dump({"mappings": {"30": "a.wav"}, "defaults": ["default.wav"]}, _f)
sys.argv = ["runner_cross_platform.py", _tmp]

sys.argv = ["runner_cross_platform.py", _tmp]

# Pretend the referenced wav files exist so sounds register
rc_os = __import__("os")
_orig_exists = rc_os.path.exists
rc_os.path.exists = lambda p: True

import runner_cross_platform as rc


class TestKeyMapParsing(unittest.TestCase):
    def _build(self, mappings, defaults):
        d = tempfile.mkdtemp()
        with open(os.path.join(d, "config.json"), "w") as f:
            json.dump({"mappings": mappings, "defaults": defaults}, f)
        # Stub pygame so import-time audio init is a no-op
        fake_mixer = mock.MagicMock()
        fake_mixer.Sound.side_effect = lambda p: FakeSound(p)
        rc.pygame = fake_mixer
        rc.SOUNDS = {}
        rc.SOUND_CACHE = [None] * rc.MAX_KEYCODE
        rc.DEFAULT_SOUND_OBJS = []
        with open(os.path.join(d, "config.json")) as f:
            data = json.load(f)
        rc.RAW_KEY_MAP = {int(k): v for k, v in data.get("mappings", {}).items()}
        rc.DEFAULTS = data.get("defaults", [])
        for fn in set(rc.RAW_KEY_MAP.values()) | set(rc.DEFAULTS):
            rc.SOUNDS[fn] = FakeSound(fn)
        rc.DEFAULT_SOUND_OBJS = [rc.SOUNDS[f] for f in rc.DEFAULTS if f in rc.SOUNDS]
        for code, fn in rc.RAW_KEY_MAP.items():
            if 0 <= code < rc.MAX_KEYCODE and fn in rc.SOUNDS:
                rc.SOUND_CACHE[code] = rc.SOUNDS[fn]

    def test_negative_key_skipped_not_crash(self):
        self._build({"-1": "a.wav", "30": "a.wav"}, [])
        self.assertIsNone(rc.SOUND_CACHE[0])
        self.assertIsNotNone(rc.SOUND_CACHE[30])

    def test_out_of_range_key_skipped(self):
        self._build({"999999": "a.wav"}, [])
        self.assertEqual(rc.SOUND_CACHE.count(None), rc.MAX_KEYCODE)

    def test_play_sound_mapped(self):
        self._build({"30": "a.wav"}, [])
        rc.play_sound(30)
        self.assertEqual(rc.SOUNDS["a.wav"].plays, 1)

    def test_play_sound_unmapped_falls_to_default(self):
        self._build({"30": "a.wav"}, ["default.wav"])
        rc.play_sound(9999)
        self.assertEqual(rc.SOUNDS["default.wav"].plays, 1)


class TestTrackpadFilter(unittest.TestCase):
    def _listener(self, enable_trackpads):
        fake_evdev = mock.MagicMock()
        fake_ecodes = mock.MagicMock()
        from linux_input import LinuxInputListener

        return LinuxInputListener(
            lambda c: None, fake_evdev, fake_ecodes, enable_trackpads=enable_trackpads
        )

    def test_trackpad_skipped_when_disabled(self):
        l = self._listener(enable_trackpads=False)
        self.assertTrue(l.should_skip("SynPS/2 Synaptics Touchpad"))
        self.assertTrue(l.should_skip("Apple Trackpad"))

    def test_trackpad_allowed_when_enabled(self):
        l = self._listener(enable_trackpads=True)
        self.assertFalse(l.should_skip("SynPS/2 Synaptics Touchpad"))

    def test_non_trackpad_never_skipped(self):
        l = self._listener(enable_trackpads=False)
        self.assertFalse(l.should_skip("AT Translated Set 2 keyboard"))


class TestInputHandlerFlag(unittest.TestCase):
    def test_enable_trackpads_passed_through(self):
        # Stub the platform and the platform-specific imports so the handler
        # can be constructed off-Linux without real evdev/ctypes/Quartz.
        import input_handler

        fake_platform = mock.MagicMock()
        fake_platform.system.return_value = "linux"
        input_handler.platform = fake_platform
        fake_evdev = mock.MagicMock()
        with mock.patch.dict(
            "sys.modules", {"evdev": fake_evdev, "evdev.ecodes": mock.MagicMock()}
        ):
            h = input_handler.InputHandler(lambda c: None, enable_trackpads=True)
        self.assertTrue(h.enable_trackpads)


if __name__ == "__main__":
    unittest.main()
