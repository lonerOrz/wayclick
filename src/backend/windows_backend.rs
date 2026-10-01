//! Windows backend: low-level keyboard/mouse hooks via `windows-sys`. The hooks
//! run on a dedicated `GetMessageW` pump thread and push through [`bridge`].

use std::collections::HashSet;
use std::sync::{LazyLock, Mutex};

use futures::stream::BoxStream;
use windows_sys::Win32::Foundation::{GetLastError, HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT, SetWindowsHookExW,
    WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_MBUTTONDOWN,
    WM_RBUTTONDOWN, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_XBUTTONDOWN,
};

use crate::backend::bridge::{self, emit};
use crate::backend::{BackendError, InputBackend};
use crate::domain::{InputEvent, MouseButton};

/// WH_KEYBOARD_LL does not surface OS auto-repeat, so track held keys to emit
/// exactly one press per physical key-down.
static PRESSED: LazyLock<Mutex<HashSet<u16>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

/// Windows input backend. The mouse hook cannot tell a trackpad from a mouse, so
/// there is no trackpad policy here (see `backend::evdev_backend`).
pub struct WindowsBackend;

impl InputBackend for WindowsBackend {
    fn events(&mut self) -> Result<BoxStream<'static, InputEvent>, BackendError> {
        start()
    }

    fn name(&self) -> &'static str {
        "windows-low-level-hook"
    }
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        // SAFETY: lparam points to a KBDLLHOOKSTRUCT for WH_KEYBOARD_LL.
        let kb = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
        let vk = kb.vkCode as u16;
        match wparam as u32 {
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                if let Ok(mut set) = PRESSED.lock()
                    && set.insert(vk)
                {
                    emit(InputEvent::Key(vk));
                }
            }
            WM_KEYUP | WM_SYSKEYUP => {
                if let Ok(mut set) = PRESSED.lock() {
                    set.remove(&vk);
                }
            }
            _ => {}
        }
    }
    // SAFETY: pass the original hook arguments through unchanged.
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let button = match wparam as u32 {
            WM_LBUTTONDOWN => Some(MouseButton::Left),
            WM_RBUTTONDOWN => Some(MouseButton::Right),
            WM_MBUTTONDOWN => Some(MouseButton::Middle),
            WM_XBUTTONDOWN => {
                // SAFETY: lparam points to an MSLLHOOKSTRUCT for WH_MOUSE_LL.
                let ms = unsafe { &*(lparam as *const MSLLHOOKSTRUCT) };
                let x_id = ((ms.mouseData >> 16) & 0xFFFF) as u16;
                Some(MouseButton::from_windows_xbutton(x_id))
            }
            _ => None,
        };
        if let Some(button) = button {
            emit(InputEvent::Mouse(button));
        }
    }
    // SAFETY: pass the original hook arguments through unchanged.
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

fn start() -> Result<BoxStream<'static, InputEvent>, BackendError> {
    bridge::start_bridged("wayclick-win-hooks", None, |ready| {
        // SAFETY: hooks must be installed on the thread that runs the message
        // pump; null HINSTANCE + thread id 0 installs a global low-level hook.
        unsafe {
            let null_hmod: HINSTANCE = std::ptr::null_mut();

            if SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), null_hmod, 0).is_null() {
                ready.fail(BackendError::Start(format!(
                    "SetWindowsHookExW(WH_KEYBOARD_LL) failed: GetLastError={}",
                    GetLastError()
                )));
                return;
            }
            if SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), null_hmod, 0).is_null() {
                ready.fail(BackendError::Start(format!(
                    "SetWindowsHookExW(WH_MOUSE_LL) failed: GetLastError={}",
                    GetLastError()
                )));
                return;
            }

            ready.ok();
            tracing::info!("windows low-level hooks installed");

            // GetMessageW: 0 = WM_QUIT, -1 = error.
            let mut msg: MSG = std::mem::zeroed();
            while !matches!(GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0), 0 | -1) {}
            tracing::info!("windows hook message pump exited");
        }
    })
}
