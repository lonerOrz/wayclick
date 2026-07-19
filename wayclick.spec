# -*- mode: python ; coding: utf-8 -*-

from PyInstaller.utils.hooks import collect_all

import sys

block_cipher = None

# pywin32 + system DLLs are Windows-only. Collect them only there so the same
# spec builds on Linux/macOS without pywin32 installed.
if sys.platform == "win32":
    pywin32_datas, pywin32_binaries, pywin32_hidden = collect_all("pywin32")
else:
    pywin32_datas, pywin32_binaries, pywin32_hidden = [], [], []

a = Analysis(
    ['src/runner_cross_platform.py'],
    pathex=[],
    binaries=pywin32_binaries,   # 包含 pywin32 的二进制文件
    datas=[
        ('src/hook-runner_cross_platform.py', '.'),
        *pywin32_datas,           # 包含 pywin32 的数据文件
    ],
    hiddenimports=[
        *pywin32_hidden,          # 包含 pywin32 的隐藏导入（仅 Windows）
        'ctypes',
        'ctypes.wintypes',
        # Windows-only pywin32 modules: only present on Windows.
        *(['win32api', 'win32con', 'win32gui', 'pythoncom', 'pywintypes']
          if sys.platform == "win32" else []),
        # input_handler / linux_input / windows_input / macos_input are injected
        # by hook-runner_cross_platform.py (derived from LISTENERS).
    ],
    hookspath=['src'],
    hooksconfig={},
    runtime_hooks=[],
    excludes=[],
)

pyz = PYZ(a.pure, a.zipped_data, cipher=block_cipher)

exe = EXE(
    pyz,
    a.scripts,
    a.binaries,
    a.zipfiles,
    a.datas,
    [],
    name='wayclick',
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=True,
    upx_exclude=[],
    runtime_tmpdir=None,
    console=True,
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
)