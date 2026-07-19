# linux_input.py
import asyncio
import signal

import evdev
from evdev import ecodes


class LinuxInputListener:
    def __init__(self, play_sound_callback, enable_trackpads=False):
        self.play_sound = play_sound_callback
        self.enable_trackpads = enable_trackpads

    async def read_device(self, path, stop_event):
        dev = None
        try:
            dev = evdev.InputDevice(path)
            print(f"\033[1;32m[+]\033[0m {dev.name}")
            async for event in dev.async_read_loop():
                if stop_event.is_set():
                    break
                if (
                    event.type == ecodes.EV_KEY and event.value == 1
                ):  # EV_KEY and key press
                    self.play_sound(event.code)
        except Exception:
            print(f"\033[1;33m[-]\033[0m {path}")
        finally:
            if dev:
                dev.close()

    def should_skip(self, name):
        name = name.lower()
        return not self.enable_trackpads and ("touchpad" in name or "trackpad" in name)

    async def run(self):
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
                    if self.should_skip(name):
                        dev.close()
                        continue
                    if ecodes.EV_KEY in dev.capabilities():
                        tasks[path] = asyncio.create_task(self.read_device(path, stop))
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
