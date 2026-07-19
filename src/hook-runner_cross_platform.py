import sys

from PyInstaller.utils.hooks import collect_submodules

import input_handler

hiddenimports = collect_submodules(".")

# Derive the platform adapters from the single registry so a new adapter is
# auto-included. input_handler itself plus Windows-only ctypes deps stay explicit.
hiddenimports += ["input_handler"]
hiddenimports += [listener.__module__ for listener in input_handler.LISTENERS.values()]
hiddenimports += [
    "ctypes",
    "ctypes.wintypes",
]

# Windows-only system libraries for hooks.
datas = []
binaries = []

if sys.platform == "win32":
    from PyInstaller.utils.hooks import collect_dynamic_libs

    # Collect all dynamic libraries from the packages used
    binaries.extend(collect_dynamic_libs("pywin32"))

    # Add specific Windows DLLs that might be required for hooks
    win_binaries = [
        ("user32.dll", "."),
        ("kernel32.dll", "."),
        ("gdi32.dll", "."),
    ]

    for dll_name, dest_dir in win_binaries:
        try:
            import ctypes.util

            dll_path = ctypes.util.find_library(dll_name)
            if dll_path:
                binaries.append((dll_path, dest_dir))
        except Exception:
            # If DLL can't be found, skip it
            pass
