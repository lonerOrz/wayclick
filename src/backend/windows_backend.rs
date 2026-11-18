//! Windows backend: low-level keyboard/mouse hooks via `windows-sys`.
//!
//! The hooks run on a dedicated `GetMessageW` pump thread. HHOOK handles are
//! owned by an RAII guard created on that thread; shutdown posts `WM_QUIT` to
//! the thread so the loop exits and the guard unhooks.

use std::cell::RefCell;
use std::collections::HashSet;

use windows_sys::Win32::Foundation::{GetLastError, HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, HHOOK, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT, PostThreadMessageW,
    SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_KEYUP,
    WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_XBUTTONDOWN,
};

use crate::backend::bridge::{self, emit};
use crate::backend::{BackendError, InputBackend};
use crate::domain::InputEvent;
use crate::input::session::InputSession;
use crate::input::translator;

// Dedupe held keys by physical scan code + extended flag. Hook callbacks
// run serially on the pump thread, so this state is thread-local and needs
// no locking in the real-time callback path.
thread_local! {
    static PRESSED: RefCell<HashSet<u32>> = RefCell::new(HashSet::new());
}

/// Hooks installed on the pump thread; dropped on that same thread, so
/// `UnhookWindowsHookEx` runs where the hooks were installed.
struct HookGuard {
    keyboard: HHOOK,
    mouse: HHOOK,
}

impl HookGuard {
    fn install() -> Result<Self, BackendError> {
        let null_hmod: HINSTANCE = std::ptr::null_mut();
        // SAFETY: global low-level hooks need null HINSTANCE + thread id 0 and
        // must be set from the thread running the message pump.
        let keyboard =
            unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), null_hmod, 0) };
        if keyboard.is_null() {
            return Err(BackendError::Start(format!(
                "SetWindowsHookExW(WH_KEYBOARD_LL) failed: GetLastError={}",
                unsafe { GetLastError() }
            )));
        }
        // If the mouse hook fails, `keyboard` is not yet owned by a guard;
        // unhook it here so partial installation never leaks the first hook.
        let mouse = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), null_hmod, 0) };
        if mouse.is_null() {
            // SAFETY: keyboard is a valid HHOOK from the successful call above.
            unsafe { UnhookWindowsHookEx(keyboard) };
            return Err(BackendError::Start(format!(
                "SetWindowsHookExW(WH_MOUSE_LL) failed: GetLastError={}",
                unsafe { GetLastError() }
            )));
        }
        Ok(Self { keyboard, mouse })
    }
}

impl Drop for HookGuard {
    fn drop(&mut self) {
        // SAFETY: both handles were installed on this thread and are still valid
        // until unhooked here.
        unsafe {
            if !self.keyboard.is_null() {
                UnhookWindowsHookEx(self.keyboard);
            }
            if !self.mouse.is_null() {
                UnhookWindowsHookEx(self.mouse);
            }
        }
    }
}

/// Windows input backend. The mouse hook cannot tell a trackpad from a mouse.
pub struct WindowsBackend;

impl InputBackend for WindowsBackend {
    fn events(&mut self) -> Result<InputSession, BackendError> {
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
        let scan = (kb.scanCode & 0xFF) as u16;
        let extended = (kb.flags & 0x01) != 0; // LLKHF_EXTENDED
        let key = translator::windows::translate_key(scan, extended);
        let dedup_key = (u32::from(scan) << 1) | u32::from(extended);
        match wparam as u32 {
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                PRESSED.with(|pressed| {
                    if pressed.borrow_mut().insert(dedup_key) {
                        emit(InputEvent::KeyDown(key));
                    }
                });
            }
            WM_KEYUP | WM_SYSKEYUP => {
                PRESSED.with(|pressed| {
                    pressed.borrow_mut().remove(&dedup_key);
                });
            }
            _ => {}
        }
    }
    // SAFETY: pass the original hook arguments through unchanged.
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let message = wparam as u32;
        let x_id = if message == WM_XBUTTONDOWN {
            // SAFETY: lparam points to an MSLLHOOKSTRUCT for WH_MOUSE_LL.
            let ms = unsafe { &*(lparam as *const MSLLHOOKSTRUCT) };
            ((ms.mouseData >> 16) & 0xFFFF) as u16
        } else {
            0
        };
        if let Some(button) = translator::windows::translate_mouse_message(message, x_id) {
            emit(InputEvent::MouseDown(button));
        }
    }
    // SAFETY: pass the original hook arguments through unchanged.
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

fn start() -> Result<InputSession, BackendError> {
    // Dedupe state is thread-local to the pump thread: each start gets a
    // fresh set, so no cross-thread clearing is needed here.

    let started = bridge::start_bridged("wayclick-win-hooks", None, |ready| {
        // SAFETY: hooks and message loop must be on the same thread.
        unsafe {
            // Clear this thread's dedupe state before hooks go live, so a
            // reused pump thread never sees stale held keys.
            PRESSED.with(|pressed| pressed.borrow_mut().clear());
            let _guard = match HookGuard::install() {
                Ok(guard) => guard,
                Err(err) => {
                    ready.fail(err);
                    return;
                }
            };
            let thread_id = GetCurrentThreadId();
            // Register stop before reporting ready so no stop is lost.
            bridge::register_stop(Box::new(move || {
                // SAFETY: PostThreadMessageW to the live pump thread; this
                // closure executes inside the outer `unsafe` scope.
                PostThreadMessageW(thread_id, WM_QUIT, 0, 0);
            }));
            ready.ok();

            tracing::info!("windows low-level hooks installed");
            let mut msg: MSG = std::mem::zeroed();
            loop {
                // Leave the loop on error (-1) or WM_QUIT (0); either way the
                // closure returns and the bridge closes the event stream.
                let result = GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0);
                if result == -1 {
                    tracing::warn!(error = GetLastError(), "GetMessageW failed; stopping hooks");
                    break;
                }
                if result == 0 {
                    break;
                }
            }
            tracing::info!("windows hook message pump exited");
        }
    })?;
    let (stop, join) = bridge::session_hooks();
    Ok(InputSession::new(started.events, stop, join))
}
