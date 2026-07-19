# sound_engine.py
import os
import json
import random

MAX_KEYCODE = 65536


class SoundEngine:
    def __init__(self, asset_dir, mixer, enable_trackpads=False):
        self.asset_dir = asset_dir
        self.mixer = mixer

        mixer.pre_init(44100, -16, 2, 256)
        mixer.init()
        mixer.set_num_channels(32)

        config_file = os.path.join(asset_dir, "config.json")
        with open(config_file, "r") as f:
            config_data = json.load(f)
            raw_key_map = {
                int(k): v for k, v in config_data.get("mappings", {}).items()
            }
            defaults = config_data.get("defaults", [])

        sound_files = set(raw_key_map.values()) | set(defaults)
        sounds = {}
        for filename in sound_files:
            path = os.path.join(asset_dir, filename)
            try:
                sounds[filename] = mixer.Sound(path)
            except mixer.error:
                print(f"\033[1;33m[WARN]\033[0m Invalid wav: {filename}")

        if not sounds:
            raise RuntimeError("No sounds loaded")

        self._cache = [None] * MAX_KEYCODE
        for code, filename in raw_key_map.items():
            if 0 <= code < MAX_KEYCODE and filename in sounds:
                self._cache[code] = sounds[filename]

        self._defaults = [sounds[f] for f in defaults if f in sounds]
        self._choice = random.choice

    def play(self, code):
        if isinstance(code, int) and 0 <= code < MAX_KEYCODE:
            sound = self._cache[code]
            if sound:
                sound.play()
                return
        if self._defaults:
            self._choice(self._defaults).play()

    def stop(self):
        self.mixer.quit()
