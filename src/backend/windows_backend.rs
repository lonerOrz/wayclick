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

    #[cfg(windows)]
    fn events(&mut self) -> Result<BoxStream<'static, InputEvent>, BackendError> {
        windows_impl::start()
    }

    #[cfg(not(windows))]
    fn events(&mut self) -> Result<BoxStream<'static, InputEvent>, BackendError> {
        let _ = &self.enable_trackpads;
        Err(BackendError::Start(
            "windows backend is only available on Windows".into(),
        ))
    }
}

#[cfg(windows)]
mod windows_impl {
    use std::sync::Mutex;

    use futures::stream::{BoxStream, StreamExt};
    use tokio::sync::mpsc::{self, Sender};
    use tokio_stream::wrappers::ReceiverStream;

    use windows_sys::Win32::Foundation::{GetLastError, HINSTANCE, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetMessageW, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT, SetWindowsHookExW,
        WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_RBUTTONDOWN,
        WM_SYSKEYDOWN, WM_XBUTTONDOWN,
    };

    use crate::backend::BackendError;
    use crate::domain::{InputEvent, MouseButton};

    /// The single sink the hook callbacks push into. Callbacks are free
    /// `extern "system"` functions and cannot capture, so the sender lives here.
    /// `Option` (not `OnceLock`) so the backend is re-entrant: a fresh `start()`
    /// replaces the sender, and shutdown clears it.
    static SENDER: Mutex<Option<Sender<InputEvent>>> = Mutex::new(None);

    fn emit(event: InputEvent) {
        if let Ok(guard) = SENDER.lock() {
            if let Some(tx) = guard.as_ref() {
                // Bounded channel: drop on full rather than block the hook.
                let _ = tx.try_send(event);
            }
        }
    }

    unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 {
            let msg = wparam as u32;
            if msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN {
                let kb = &*(lparam as *const KBDLLHOOKSTRUCT);
                emit(InputEvent::Key(kb.vkCode as u16));
            }
        }
        CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
    }

    unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 {
            let msg = wparam as u32;
            let button = match msg {
                WM_LBUTTONDOWN => Some(MouseButton::Left),
                WM_RBUTTONDOWN => Some(MouseButton::Right),
                WM_MBUTTONDOWN => Some(MouseButton::Middle),
                WM_XBUTTONDOWN => {
                    let ms = &*(lparam as *const MSLLHOOKSTRUCT);
                    let x_id = (ms.mouseData >> 16) & 0xFFFF;
                    // 1 = XBUTTON1 (Back), 2 = XBUTTON2 (Forward).
                    Some(if x_id == 1 {
                        MouseButton::Back
                    } else {
                        MouseButton::Forward
                    })
                }
                _ => None,
            };
            if let Some(b) = button {
                emit(InputEvent::Mouse(b));
            }
        }
        CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
    }

    pub fn start() -> Result<BoxStream<'static, InputEvent>, BackendError> {
        let (tx, rx) = mpsc::channel::<InputEvent>(1024);

        *SENDER
            .lock()
            .map_err(|_| BackendError::Start("sender lock poisoned".into()))? = Some(tx);

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
                    let kb_hook =
                        SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), null_hmod, 0);
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
            Ok(Ok(())) => Ok(ReceiverStream::new(rx).boxed()),
            Ok(Err(msg)) => {
                tracing::error!("{msg}");
                Err(BackendError::Start(msg))
            }
            Err(_) => Err(BackendError::Start(
                "hook thread died before installing hooks".into(),
            )),
        }
    }
}
