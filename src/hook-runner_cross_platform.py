from PyInstaller.utils.hooks import collect_submodules

hiddenimports = collect_submodules('.')

# Explicitly add the modules that are imported dynamically
hiddenimports += [
    'input_handler',
    'linux_input',
    'windows_input',
    'macos_input',
]