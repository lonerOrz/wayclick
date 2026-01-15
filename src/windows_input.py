# windows_input.py
import ctypes
import time

class WindowsInputListener:
    def __init__(self, play_sound_callback, ctypes_module, wintypes_module):
        self.play_sound = play_sound_callback
        self.ctypes = ctypes_module
        self.wintypes = wintypes_module
        
        # Windows input hook constants
        self.WH_KEYBOARD_LL = 13
        self.WH_MOUSE_LL = 14
        self.WM_KEYDOWN = 0x0100
        self.WM_SYSKEYDOWN = 0x0104
        self.WM_LBUTTONDOWN = 0x0201
        self.WM_RBUTTONDOWN = 0x0204
        self.WM_MBUTTONDOWN = 0x0207
        self.WM_XBUTTONDOWN = 0x020B

        # Global variables for Windows hooks
        self.user32 = self.ctypes.windll.user32
        self.kernel32 = self.ctypes.windll.kernel32
        
        self.hhook_keyboard = None
        self.hhook_mouse = None
        self.running = True

    def keyboard_proc(self, nCode, wParam, lParam):
        if nCode >= 0 and wParam in (self.WM_KEYDOWN, self.WM_SYSKEYDOWN):
            vk_code = self.ctypes.c_int.from_address(lParam).value
            self.play_sound(vk_code)
        
        return self.user32.CallNextHookEx(self.hhook_keyboard, nCode, wParam, lParam)

    def mouse_proc(self, nCode, wParam, lParam):
        if nCode >= 0 and wParam in (self.WM_LBUTTONDOWN, self.WM_RBUTTONDOWN, self.WM_MBUTTONDOWN, self.WM_XBUTTONDOWN):
            # Map mouse buttons to codes
            if wParam == self.WM_LBUTTONDOWN:
                button_code = 0x01  # Left mouse button
            elif wParam == self.WM_RBUTTONDOWN:
                button_code = 0x02  # Right mouse button
            elif wParam == self.WM_MBUTTONDOWN:
                button_code = 0x04  # Middle mouse button
            else:  # WM_XBUTTONDOWN
                button_code = 0x05  # Extra mouse button
            
            self.play_sound(button_code)
        
        return self.user32.CallNextHookEx(self.hhook_mouse, nCode, wParam, lParam)

    def setup_hooks(self):
        try:
            cmpfunc_kbd = self.ctypes.WINFUNCTYPE(
                self.ctypes.c_int,
                self.ctypes.c_int,
                self.wintypes.WPARAM,
                self.wintypes.LPARAM
            )
            cmpfunc_mse = self.ctypes.WINFUNCTYPE(
                self.ctypes.c_int,
                self.ctypes.c_int,
                self.wintypes.WPARAM,
                self.wintypes.LPARAM
            )

            keyboard_callback_ptr = cmpfunc_kbd(self.keyboard_proc)
            mouse_callback_ptr = cmpfunc_mse(self.mouse_proc)

            self.hhook_keyboard = self.user32.SetWindowsHookExW(
                self.WH_KEYBOARD_LL,
                keyboard_callback_ptr,
                self.kernel32.GetModuleHandleW(None),
                0
            )

            # Check if keyboard hook was installed successfully
            if not self.hhook_keyboard:
                error_code = self.kernel32.GetLastError()
                print(f"\033[1;31m[ERROR]\033[0m Failed to install keyboard hook. Error code: {error_code}")
                if error_code == 126:
                    print("\033[1;31m[ERROR]\033[0m Error 126 indicates a missing DLL.")
                    print("\033[1;31m[ERROR]\033[0m This is often related to PyInstaller and Windows security policies.")
                return False

            self.hhook_mouse = self.user32.SetWindowsHookExW(
                self.WH_MOUSE_LL,
                mouse_callback_ptr,
                self.kernel32.GetModuleHandleW(None),
                0
            )

            # Check if mouse hook was installed successfully
            if not self.hhook_mouse:
                error_code = self.kernel32.GetLastError()
                print(f"\033[1;31m[ERROR]\033[0m Failed to install mouse hook. Error code: {error_code}")
                if error_code == 126:
                    print("\033[1;31m[ERROR]\033[0m Error 126 indicates a missing DLL.")
                    print("\033[1;31m[ERROR]\033[0m This is often related to PyInstaller and Windows security policies.")
                # Clean up the keyboard hook if mouse hook failed
                if self.hhook_keyboard:
                    self.user32.UnhookWindowsHookEx(self.hhook_keyboard)
                    self.hhook_keyboard = None
                return False

            if not self.hhook_keyboard or not self.hhook_mouse:
                return False

            # Keep references to prevent garbage collection
            self.user32.keyboard_callback_ptr = keyboard_callback_ptr
            self.user32.mouse_callback_ptr = mouse_callback_ptr

            return True
        except Exception as e:
            print(f"\033[1;31m[ERROR]\033[0m Exception during hook setup: {str(e)}")
            import traceback
            traceback.print_exc()
            return False

    def remove_hooks(self):
        if self.hhook_keyboard:
            self.user32.UnhookWindowsHookEx(self.hhook_keyboard)
        if self.hhook_mouse:
            self.user32.UnhookWindowsHookEx(self.hhook_mouse)

    def run(self):
        print("\033[1;32m[INFO]\033[0m Starting WayClick for Windows...")

        if not self.setup_hooks():
            print("\033[1;31m[ERROR]\033[0m Could not install input hooks. This may be due to:")
            print("  - Insufficient privileges (try running as administrator)")
            print("  - Security software blocking the hooks")
            print("  - System policy restrictions")
            print("  - Antivirus software interfering with system hooks")
            print("\033[1;33m[WARNING]\033[0m Exiting due to inability to install hooks.")
            return 1

        try:
            # Message loop
            msg = self.wintypes.MSG()
            while True:
                ret = self.user32.GetMessageW(self.ctypes.byref(msg), None, 0, 0)
                if ret == 0:  # WM_QUIT
                    break
                elif ret == -1:  # Error
                    print("\033[1;31m[ERROR]\033[0m Message loop error")
                    break
                else:
                    self.user32.TranslateMessage(self.ctypes.byref(msg))
                    self.user32.DispatchMessageW(self.ctypes.byref(msg))
        except KeyboardInterrupt:
            print("\033[1;33m[INFO]\033[0m Shutting down...")
        finally:
            self.remove_hooks()

        return 0