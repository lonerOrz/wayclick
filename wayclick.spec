# -*- mode: python ; coding: utf-8 -*-

from PyInstaller.utils.hooks import collect_all

block_cipher = None

# 关键：显式收集 pywin32
pywin32_datas, pywin32_binaries, pywin32_hidden = collect_all('pywin32')

a = Analysis(
    ['src/runner_cross_platform.py'],
    pathex=[],
    binaries=pywin32_binaries,   # 包含 pywin32 的二进制文件
    datas=[
        ('src/hook-runner_cross_platform.py', '.'),
        *pywin32_datas,           # 包含 pywin32 的数据文件
    ],
    hiddenimports=[
        *pywin32_hidden,          # 包含 pywin32 的隐藏导入
        'win32api',
        'win32con',
        'win32gui',
        'pythoncom',
        'pywintypes',
        'ctypes',
        'ctypes.wintypes',
        'input_handler',
        'linux_input',
        'windows_input',
        'macos_input',
    ],
    hookspath=['src'],
    hooksconfig={},
    runtime_hooks=[],
    excludes=[],
    win_no_prefer_redirects=False,
    win_private_assemblies=False,
    cipher=block_cipher,
    noarchive=False,
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