import os
import sys
import json
import random
import platform
import pygame

# ANSI Colors (Windows compatible)
C_GREEN = "\033[1;32m"
C_YELLOW = "\033[1;33m"
C_BLUE = "\033[1;34m"
C_RED = "\033[1;31m"
C_RESET = "\033[0m"

# Determine the config directory based on the platform
current_platform = platform.system().lower()

if current_platform == "linux":
    CONFIG_DIR = os.path.expanduser("~/.config/wayclick")
elif current_platform == "windows":
    CONFIG_DIR = os.path.expanduser("~\\.wayclick")
elif current_platform == "darwin":  # macOS
    CONFIG_DIR = os.path.expanduser("~/.config/wayclick")
else:
    # Default fallback
    CONFIG_DIR = os.path.expanduser("~/.config/wayclick")

# Allow override via command line argument
if len(sys.argv) > 1:
    ASSET_DIR = sys.argv[1]
else:
    ASSET_DIR = CONFIG_DIR

ENABLE_TRACKPADS = os.environ.get("ENABLE_TRACKPADS", "false").lower() == "true"

# === PERFORMANCE FLAGS ===
os.environ['PYGAME_HIDE_SUPPORT_PROMPT'] = '1'
os.environ['SDL_BUFFER_CHUNK_SIZE'] = '256'

# === AUDIO INIT ===
try:
    pygame.mixer.pre_init(44100, -16, 2, 256)
    pygame.mixer.init()
    pygame.mixer.set_num_channels(32)
except pygame.error as e:
    print(f"{C_RED}[AUDIO ERROR]{C_RESET} {e}")
    sys.exit(1)

# === CONFIG LOADING ===
CONFIG_FILE = os.path.join(ASSET_DIR, "config.json")
print(f"{C_BLUE}[INFO]{C_RESET} Loading assets from {ASSET_DIR}...")

try:
    with open(CONFIG_FILE, "r") as f:
        config_data = json.load(f)
        RAW_KEY_MAP = {int(k): v for k, v in config_data.get("mappings", {}).items()}
        DEFAULTS = config_data.get("defaults", [])
except Exception as e:
    print(f"{C_RED}[CONFIG ERROR]{C_RESET} {e}")
    sys.exit(1)

SOUND_FILES = list(set(RAW_KEY_MAP.values()) | set(DEFAULTS))
SOUNDS = {}

for filename in SOUND_FILES:
    path = os.path.join(ASSET_DIR, filename)
    if os.path.exists(path):
        try:
            SOUNDS[filename] = pygame.mixer.Sound(path)
        except pygame.error:
            print(f"{C_YELLOW}[WARN]{C_RESET} Invalid wav: {filename}")
    else:
        print(f"{C_YELLOW}[WARN]{C_RESET} Missing wav: {filename}")

if not SOUNDS:
    sys.exit("No sounds loaded")

MAX_KEYCODE = 65536
SOUND_CACHE = [None] * MAX_KEYCODE
DEFAULT_SOUND_OBJS = [SOUNDS[f] for f in DEFAULTS if f in SOUNDS]

for code, filename in RAW_KEY_MAP.items():
    if code < MAX_KEYCODE and filename in SOUNDS:
        SOUND_CACHE[code] = SOUNDS[filename]

_random_choice = random.choice

def play_sound(code):
    if code < MAX_KEYCODE:
        sound = SOUND_CACHE[code]
        if sound:
            sound.play()
            return
    if DEFAULT_SOUND_OBJS:
        _random_choice(DEFAULT_SOUND_OBJS).play()

def main():
    # Determine the current platform
    current_platform = platform.system().lower()

    if current_platform == "linux":
        # Import and run the Linux-specific input handler directly
        import asyncio
        import signal
        import evdev
        from evdev import ecodes

        async def read_device(path, stop_event):
            dev = None
            try:
                dev = evdev.InputDevice(path)
                print(f"{C_GREEN}[+]{C_RESET} {dev.name}")
                async for event in dev.async_read_loop():
                    if stop_event.is_set():
                        break
                    if event.type == 1 and event.value == 1:  # EV_KEY and key press
                        play_sound(event.code)
            except Exception:
                print(f"{C_YELLOW}[-]{C_RESET} {path}")
            finally:
                if dev:
                    dev.close()

        async def linux_main():
            stop = asyncio.Event()
            loop = asyncio.get_running_loop()

            for sig in (signal.SIGINT, signal.SIGTERM):
                loop.add_signal_handler(sig, stop.set)

            tasks = {}

            while not stop.is_set():
                for path in evdev.list_devices():
                    if path in tasks:
                        continue
                    try:
                        dev = evdev.InputDevice(path)
                        name = dev.name.lower()
                        if not ENABLE_TRACKPADS and ("touchpad" in name or "trackpad" in name):
                            dev.close()
                            continue
                        if ecodes.EV_KEY in dev.capabilities():
                            tasks[path] = asyncio.create_task(read_device(path, stop))
                        dev.close()
                    except Exception:
                        pass

                for p in [p for p, t in tasks.items() if t.done()]:
                    del tasks[p]

                try:
                    await asyncio.wait_for(stop.wait(), timeout=3)
                except asyncio.TimeoutError:
                    pass

            for t in tasks.values():
                t.cancel()
            await asyncio.gather(*tasks.values(), return_exceptions=True)

        # Run the Linux-specific event loop
        result = asyncio.run(linux_main())
    else:
        # For non-Linux platforms, use the input handler
        from input_handler import InputHandler
        handler = InputHandler(play_sound)
        result = handler.start_listening()

    # Properly quit pygame when done
    pygame.mixer.quit()

    return result

if __name__ == "__main__":
    main()