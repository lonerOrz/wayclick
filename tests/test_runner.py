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


def fake_mixer():
    m = mock.MagicMock()
    m.error = Exception
    m.Sound.side_effect = lambda p: FakeSound(p)
    return m


def make_engine(mappings, defaults, mixer=None):
    d = tempfile.mkdtemp()
    with open(os.path.join(d, "config.json"), "w") as f:
        json.dump({"mappings": mappings, "defaults": defaults}, f)
    return __import__("sound_engine").SoundEngine(d, mixer or fake_mixer())


class TestKeyMapParsing(unittest.TestCase):
    def test_negative_key_skipped_not_crash(self):
        e = make_engine({"-1": "a.wav", "30": "a.wav"}, [])
        e.play(0)
        e.play(30)
        self.assertEqual(e._cache[0], None)
        self.assertIsNotNone(e._cache[30])

    def test_out_of_range_key_skipped(self):
        e = make_engine({"999999": "a.wav"}, [])
        self.assertEqual(e._cache.count(None), len(e._cache))

    def test_play_sound_mapped(self):
        e = make_engine({"30": "a.wav"}, [])
        e.play(30)
        self.assertEqual(e._cache[30].plays, 1)

    def test_play_sound_unmapped_falls_to_default(self):
        e = make_engine({"30": "a.wav"}, ["default.wav"])
        e.play(9999)
        self.assertEqual(e._defaults[0].plays, 1)

    def test_out_of_range_play_silent(self):
        e = make_engine({"30": "a.wav"}, [])
        e.play(999999)
        e.play(-5)
        self.assertEqual(e._cache[30].plays, 0)

    def test_no_sounds_raises(self):
        d = tempfile.mkdtemp()
        with open(os.path.join(d, "config.json"), "w") as f:
            json.dump({"mappings": {}, "defaults": []}, f)
        with self.assertRaises(RuntimeError):
            __import__("sound_engine").SoundEngine(d, fake_mixer())


class TestTrackpadFilter(unittest.TestCase):
    def _listener(self, enable_trackpads):
        fake_evdev = mock.MagicMock()
        with mock.patch.dict(
            "sys.modules", {"evdev": fake_evdev, "evdev.ecodes": mock.MagicMock()}
        ):
            from linux_input import LinuxInputListener

            return LinuxInputListener(lambda c: None, enable_trackpads=enable_trackpads)

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
    def _handler(self, system, **kw):
        fake_evdev = mock.MagicMock()
        fake_quartz = mock.MagicMock()
        with mock.patch.dict(
            "sys.modules",
            {
                "evdev": fake_evdev,
                "evdev.ecodes": mock.MagicMock(),
                "Quartz": fake_quartz,
            },
        ):
            import input_handler

            return input_handler, input_handler.InputHandler(
                lambda c: None, system=system, **kw
            )

    def test_enable_trackpads_passed_through(self):
        mod, h = self._handler("linux", enable_trackpads=True)
        self.assertTrue(h.enable_trackpads)
        self.assertIs(h.Listener, mod.LinuxInputListener)

    def test_platform_selects_adapter(self):
        fake_evdev = mock.MagicMock()
        fake_quartz = mock.MagicMock()
        with mock.patch.dict(
            "sys.modules",
            {
                "evdev": fake_evdev,
                "evdev.ecodes": mock.MagicMock(),
                "Quartz": fake_quartz,
            },
        ):
            import input_handler

            win_h = input_handler.InputHandler(lambda c: None, system="windows")
            darwin_h = input_handler.InputHandler(lambda c: None, system="darwin")
        self.assertIs(win_h.Listener, input_handler.WindowsInputListener)
        self.assertIs(darwin_h.Listener, input_handler.MacOSInputListener)


if __name__ == "__main__":
    unittest.main()
