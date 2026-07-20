//! Windows backend: low-level keyboard/mouse hooks via windows-sys.
//!
//! Implements `InputBackend` using `windows-sys` 0.59:
//! - `SetWindowsHookExW(WH_KEYBOARD_LL / WH_MOUSE_LL, Some(proc), null, 0)`
//! - HOOKPROC returns `LRESULT` (isize) — NOT c_int (the Python bug)
//! - `KBDLLHOOKSTRUCT.vkCode` -> `InputEvent::Key(vkCode)` on WM_KEYDOWN
//! - `MSLLHOOKSTRUCT` + wparam -> `InputEvent::Mouse(..)`
//! - message pump `GetMessageW` on a dedicated `std::thread`; bridged to the
//!   stream via a bounded tokio mpsc channel.
//!
//! Lifetime: the hooks live for the whole process. wayclick runs until SIGINT;
//! the App shuts down by dropping. We do not uninstall the hooks — the process
//! exit tears down the pump thread. The stream ends when all senders drop.

use futures::stream::BoxStream;

use crate::backend::{BackendError, InputBackend};
use crate::domain::InputEvent;

/// A platform input source using Win32 low-level hooks.
#[allow(dead_code)]
pub struct WindowsBackend {
    /// Kept for API parity with the Linux backend; trackpad filtering is N/A on
    /// Windows (the low-level mouse hook does not distinguish trackpads).
    enable_trackpads: bool,
}

#[allow(dead_code)]
impl WindowsBackend {
    pub fn new(enable_trackpads: bool) -> WindowsBackend {
        WindowsBackend { enable_trackpads }
    }
}

impl InputBackend for WindowsBackend {
    fn name(&self) -> &'static str {
        "windows-low-level-hook"
    }

    #[cfg(target_os = "windows")]
    fn events(&mut self) -> Result<BoxStream<'static, InputEvent>, BackendError> {
        start()
    }

    #[cfg(not(target_os = "windows"))]
    fn events(&mut self) -> Result<BoxStream<'static, InputEvent>, BackendError> {
        let _ = &self.enable_trackpads;
        Err(BackendError::Permission)
    }
}

#[cfg(target_os = "windows")]
use futures::StreamExt;
#[cfg(target_os = "windows")]
use futures::stream::BoxStream;
#[cfg(target_os = "windows")]
use tokio::sync::mpsc;
#[cfg(target_os = "windows")]
use tokio_stream::wrappers::ReceiverStream;

#[cfg(target_os = "windows")]
use windows_sys::Win32::Foundation::{GetLastError, HINSTANCE, LPARAM, LRESULT, WPARAM};
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT, SetWindowsHookExW,
    WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_RBUTTONDOWN,
    WM_SYSKEYDOWN, WM_XBUTTONDOWN,
};

#[cfg(target_os = "windows")]
use crate::backend::{BackendError, CHANNEL_CAP, GuardedSenderStream, emit, try_start_sender};
#[cfg(target_os = "windows")]
use crate::domain::{InputEvent, MouseButton};

#[cfg(target_os = "windows")]
unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // Collapsed guard avoids clippy::collapsible_if.
    if code >= 0 && (wparam as u32 == WM_KEYDOWN || wparam as u32 == WM_SYSKEYDOWN) {
        // SAFETY: lparam points to a KBDLLHOOKSTRUCT for WH_KEYBOARD_LL.
        let kb = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
        emit(InputEvent::Key(kb.vkCode as u16));
    }
    // SAFETY: passing the original hook arguments through unchanged.
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let msg = wparam as u32;
        let button = match msg {
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
        if let Some(b) = button {
            emit(InputEvent::Mouse(b));
        }
    }
    // SAFETY: passing the original hook arguments through unchanged.
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

pub fn start() -> Result<BoxStream<'static, InputEvent>, BackendError> {
    let (tx, rx) = mpsc::channel::<InputEvent>(CHANNEL_CAP);
    try_start_sender(tx)?;

    // Handshake so we surface hook-install failure from the pump thread.
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();

    std::thread::Builder::new()
        .name("wayclick-win-hooks".into())
        .spawn(move || {
            // SAFETY: hooks must be installed on the thread that runs the
            // message pump; that is this thread. null HINSTANCE + thread id 0
            // installs a global low-level hook.
            unsafe {
                let null_hmod: HINSTANCE = std::ptr::null_mut();
                let kb_hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), null_hmod, 0);
                if kb_hook.is_null() {
                    let _ = ready_tx.send(Err(format!(
                        "SetWindowsHookExW(WH_KEYBOARD_LL) failed: GetLastError={}",
                        GetLastError()
                    )));
                    return;
                }

                let mouse_hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), null_hmod, 0);
                if mouse_hook.is_null() {
                    let _ = ready_tx.send(Err(format!(
                        "SetWindowsHookExW(WH_MOUSE_LL) failed: GetLastError={}",
                        GetLastError()
                    )));
                    return;
                }

                let _ = ready_tx.send(Ok(()));
                tracing::info!("windows low-level hooks installed");

                // Message pump. GetMessageW returns BOOL: 0 = WM_QUIT, -1 = error.
                let mut msg: MSG = std::mem::zeroed();
                loop {
                    let ret = GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0);
                    if ret == 0 || ret == -1 {
                        break;
                    }
                }
                tracing::info!("windows hook message pump exited");
            }
        })
        .map_err(|e| BackendError::Start(format!("failed to spawn hook thread: {e}")))?;

    match ready_rx.recv() {
        Ok(Ok(())) => Ok(GuardedSenderStream::new(ReceiverStream::new(rx)).boxed()),
        Ok(Err(msg)) => {
            tracing::error!("{msg}");
            Err(BackendError::Start(msg))
        }
        Err(_) => Err(BackendError::Start(
            "hook thread died before installing hooks".into(),
        )),
    }
}
