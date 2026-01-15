# -*- mode: python ; coding: utf-8 -*-

block_cipher = None

a = Analysis(
    ['src/runner_cross_platform.py'],
    pathex=[],
    binaries=[],
    datas=[
        ('src/hook-runner_cross_platform.py', '.'),
    ],
    hiddenimports=[
        'input_handler',
        'linux_input',
        'windows_input',
        'macos_input',
        'ctypes',
        'ctypes.wintypes',
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