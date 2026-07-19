import os
import sys
import pygame

from platform_paths import config_dir

# ANSI Colors (Windows compatible)
C_GREEN = "\033[1;32m"
C_YELLOW = "\033[1;33m"
C_BLUE = "\033[1;34m"
C_RED = "\033[1;31m"
C_RESET = "\033[0m"

# Config dir when run standalone (the launcher passes it as argv instead).
CONFIG_DIR = config_dir()

# Allow override via command line argument
if len(sys.argv) > 1:
    ASSET_DIR = sys.argv[1]
else:
    ASSET_DIR = CONFIG_DIR

ENABLE_TRACKPADS = os.environ.get("ENABLE_TRACKPADS", "false").lower() == "true"

# === PERFORMANCE FLAGS ===
os.environ["PYGAME_HIDE_SUPPORT_PROMPT"] = "1"
os.environ["SDL_BUFFER_CHUNK_SIZE"] = "256"


def main():
    import input_handler
    from sound_engine import SoundEngine

    try:
        engine = SoundEngine(ASSET_DIR, pygame.mixer)
    except Exception as e:
        print(f"{C_RED}[ENGINE ERROR]{C_RESET} {e}")
        return 1

    handler = input_handler.InputHandler(engine.play, enable_trackpads=ENABLE_TRACKPADS)
    result = handler.start_listening()
    engine.stop()
    return result


if __name__ == "__main__":
    sys.exit(main())
