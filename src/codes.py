# codes.py
# Shared mouse-button -> sound-code mapping.
# Single source of truth so every platform adapter emits the same codes.
BUTTON_CODES = {
    "left": 0x01,
    "right": 0x02,
    "middle": 0x04,
    "other": 0x05,
}
