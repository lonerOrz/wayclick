# wayclick

A lightweight, cross-platform CLI application for triggering sound effects
with keyboard and mouse input — low latency, configuration driven.

```
key press ──▶ platform input backend ──▶ router ──▶ audio engine ──▶ speakers
              (evdev / hook / tap)      (O(1))     (pre-decoded PCM)
```

## Install

```bash
nix run github:lonerOrz/wayclick
# or build from source
cargo build --release
```

## Quick start

```bash
wayclick check              # verify config + decode all sounds (no device needed)
wayclick                    # run the listener
```

## Configuration

`config.json` lives in the platform config directory
(`~/.config/wayclick/` on Linux, `Application Support/wayclick/` on macOS,
`AppData/Roaming/wayclick/` on Windows), or pass `--config-dir`.

```json
{
  "version": 1,
  "default_policy": "random",
  "volume": 1.0,
  "max_voices": 32,
  "defaults": ["key1.wav", "key3.wav", "key4.wav"],
  "mappings": {
    "1": "esc.wav",
    "30": "key1.wav",
    "272": "mouse.wav"
  }
}
```

| Field | Meaning |
| --- | --- |
| `default_policy` | What an unmapped key does: `random` plays a default sound (duplicates in `defaults` are weighted), `ignore` stays silent |
| `volume` | Global output gain, `0.0`–`1.0` (default `1.0`) |
| `max_voices` | Maximum sounds playing at once (default `32`) |
| `defaults` | Sounds for unmapped keys; repeating a file increases its weight |
| `mappings` | `"key" -> "sound file"`; keys are numeric key codes (see table below), relative paths resolve under the config directory |

### Key codes

A bare number is a Linux `evdev` code — the historical dialect. To target a
platform's native codes directly, prefix it: `win:<scan code>`,
`mac:<CGKeyCode>`, `linux:<evdev>`. Hexadecimal (`0x1E`) is accepted
everywhere. Unknown keys keep their platform and raw code, so nothing is
silently dropped and codes cannot collide across platforms.

<!-- key-table:start -->
| Key | Linux (`evdev`) | Windows (`scan code`) | macOS (`CGKeyCode`) |
| --- | --------------: | --------------------: | ------------------: |
| Escape | 1 | 1 | 53 |
| Digit1 | 2 | 2 | 18 |
| Digit2 | 3 | 3 | 19 |
| Digit3 | 4 | 4 | 20 |
| Digit4 | 5 | 5 | 21 |
| Digit5 | 6 | 6 | 23 |
| Digit6 | 7 | 7 | 22 |
| Digit7 | 8 | 8 | 26 |
| Digit8 | 9 | 9 | 28 |
| Digit9 | 10 | 10 | 25 |
| Digit0 | 11 | 11 | 29 |
| Minus | 12 | 12 | 27 |
| Equal | 13 | 13 | 24 |
| Backspace | 14 | 14 | 51 |
| Tab | 15 | 15 | 48 |
| Q | 16 | 16 | 12 |
| W | 17 | 17 | 13 |
| E | 18 | 18 | 14 |
| R | 19 | 19 | 15 |
| T | 20 | 20 | 17 |
| Y | 21 | 21 | 16 |
| U | 22 | 22 | 32 |
| I | 23 | 23 | 34 |
| O | 24 | 24 | 31 |
| P | 25 | 25 | 35 |
| LeftBracket | 26 | 26 | 33 |
| RightBracket | 27 | 27 | 30 |
| Enter | 28 | 28 | 36 |
| LeftCtrl | 29 | 29 | 59 |
| A | 30 | 30 | 0 |
| S | 31 | 31 | 1 |
| D | 32 | 32 | 2 |
| F | 33 | 33 | 3 |
| G | 34 | 34 | 5 |
| H | 35 | 35 | 4 |
| J | 36 | 36 | 38 |
| K | 37 | 37 | 40 |
| L | 38 | 38 | 37 |
| Semicolon | 39 | 39 | 41 |
| Apostrophe | 40 | 40 | 39 |
| Grave | 41 | 41 | 50 |
| LeftShift | 42 | 42 | 56 |
| Backslash | 43 | 43 | 42 |
| Z | 44 | 44 | 6 |
| X | 45 | 45 | 7 |
| C | 46 | 46 | 8 |
| V | 47 | 47 | 9 |
| B | 48 | 48 | 11 |
| N | 49 | 49 | 45 |
| M | 50 | 50 | 46 |
| Comma | 51 | 51 | 43 |
| Period | 52 | 52 | 47 |
| Slash | 53 | 53 | 44 |
| RightShift | 54 | 54 | 60 |
| NumpadMultiply | 55 | 55 | 67 |
| LeftAlt | 56 | 56 | 58 |
| Space | 57 | 57 | 49 |
| CapsLock | 58 | 58 | 57 |
| F1 | 59 | 59 | 122 |
| F2 | 60 | 60 | 120 |
| F3 | 61 | 61 | 99 |
| F4 | 62 | 62 | 118 |
| F5 | 63 | 63 | 96 |
| F6 | 64 | 64 | 97 |
| F7 | 65 | 65 | 98 |
| F8 | 66 | 66 | 100 |
| F9 | 67 | 67 | 101 |
| F10 | 68 | 68 | 109 |
| NumLock | 69 | 69 | 71 |
| ScrollLock | 70 | 70 | – |
| Numpad7 | 71 | 71 | 89 |
| Numpad8 | 72 | 72 | 91 |
| Numpad9 | 73 | 73 | 92 |
| NumpadSubtract | 74 | 74 | 78 |
| Numpad4 | 75 | 75 | 86 |
| Numpad5 | 76 | 76 | 87 |
| Numpad6 | 77 | 77 | 88 |
| NumpadAdd | 78 | 78 | 69 |
| Numpad1 | 79 | 79 | 83 |
| Numpad2 | 80 | 80 | 84 |
| Numpad3 | 81 | 81 | 85 |
| Numpad0 | 82 | 82 | 82 |
| NumpadDecimal | 83 | 83 | 65 |
| IntlBackslash | 86 | 86 | – |
| F11 | 87 | 87 | 103 |
| F12 | 88 | 88 | 111 |
| NumpadEnter | 96 | 284 | 76 |
| RightCtrl | 97 | 285 | 62 |
| NumpadDivide | 98 | 309 | 75 |
| PrintScreen | 99 | 311 | – |
| RightAlt | 100 | 312 | 61 |
| Home | 102 | 327 | 115 |
| ArrowUp | 103 | 328 | 126 |
| PageUp | 104 | 329 | 116 |
| ArrowLeft | 105 | 331 | 123 |
| ArrowRight | 106 | 333 | 124 |
| End | 107 | 335 | 119 |
| ArrowDown | 108 | 336 | 125 |
| PageDown | 109 | 337 | 121 |
| Insert | 110 | 338 | 114 |
| Delete | 111 | 339 | 117 |
| LeftMeta | 125 | 347 | 55 |
| RightMeta | 126 | 348 | 63 |
| Application | 127 | 349 | – |
| F13 | 183 | – | 105 |
| F14 | 184 | – | 107 |
| F15 | 185 | – | 113 |
| F16 | 186 | – | 106 |
| F17 | 187 | – | 64 |
| F18 | 188 | – | 79 |
| F19 | 189 | – | 80 |
| F20 | 190 | – | 90 |
| F21 | 191 | – | – |
| F22 | 192 | – | – |
| F23 | 193 | – | – |
| F24 | 194 | – | – |
<!-- key-table:end -->

Mouse buttons: `272`/`273`/`274` (left/right/middle on Linux), X1/X2 on
Windows (`win:` + X-button ids), and `0`–`4` button numbers on macOS.

## CLI options

| Option | Meaning |
| --- | --- |
| `--config-dir <DIR>` | Configuration directory |
| `--buffer-frames <N>` | Fixed audio period; omitted = Auto, `0` = lowest safe supported period |
| `--audio-device <NAME>` | Output device matched by case-insensitive name substring |
| `--enable-trackpads` | Also react to trackpad input (Linux) |
| `check` | Headless self-test: validate config and decode every sound |

## Latency

The event path (kernel → router → mixer submit) costs about **0.1 µs**;
end-to-end latency is dominated by the audio buffer:

| `--buffer-frames` | Period | Audible latency (48 kHz, 2-period ring) |
| ---: | ---: | ---: |
| omitted (Auto) | 256 frames target, device-clamped | ~10.7 ms |
| `0` (LowLatency) | smallest supported period, minimum 64 | device-dependent |
| `64` | 1.33 ms | ~2.7 ms |
| `128` | 2.67 ms | ~5.3 ms |
| `256` | 5.33 ms | ~10.7 ms |
| `512` | 10.67 ms | ~21 ms |

Lower periods raise the risk of buffer underruns (clicks) on a loaded
system. Auto is a device-capability-driven initial choice: it targets 256
frames clamped to the device's advertised range, and falls back to Rodio's
safe default when the range is unknown. A device advertising 64 frames does
not guarantee the system can sustain it under real scheduling load — test
the lowest stable period on the target machine rather than assuming the
hardware minimum is safe.

## Hot reload

`config.json` is reloaded automatically (polled every 2 s): routing changes
apply immediately, a broken edit keeps the previous config, and new sound
files require a restart to decode.

## License

BSD-3-Clause
