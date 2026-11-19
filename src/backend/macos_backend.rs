//! macOS backend: global CGEventTap (ListenOnly) via objc2, on a dedicated
//! thread with its own CFRunLoop. The tap's Mach port and run-loop source are
//! owned by an RAII guard on that thread; shutdown stops the run loop, which
//! unwinds the guard and disables/frees the tap.

use std::cell::Cell;
use std::ffi::c_void;
use std::time::Duration;

use objc2_core_foundation::{
    CFMachPort, CFRetained, CFRunLoop, CFRunLoopSource, kCFRunLoopDefaultMode,
};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventMask, CGEventTapLocation, CGEventTapOptions,
    CGEventTapPlacement, CGEventTapProxy, CGEventType,
};

use crate::backend::bridge::{self, emit};
use crate::backend::{BackendError, InputBackend};
use crate::domain::{InputEvent, MouseButton};
use crate::input::session::InputSession;
use crate::input::translator;

/// macOS input backend. The tap cannot tell a trackpad from a mouse.
pub struct MacosBackend;

impl InputBackend for MacosBackend {
    fn events(&mut self) -> Result<InputSession, BackendError> {
        let started =
            bridge::start_bridged("wayclick-cgeventtap", Some(Duration::from_secs(5)), run_tap)?;
        let (stop, join) = bridge::session_hooks();
        Ok(InputSession::new(started.events, stop, join))
    }

    fn name(&self) -> &'static str {
        "macos-cgeventtap"
    }
}

/// `CGEventMaskBit` is a C macro objc2 does not generate: bit `n` is event type `n`.
fn mask_bit(ty: CGEventType) -> CGEventMask {
    1u64 << (ty.0 as u64)
}

thread_local! {
    /// Last modifier flags on the run-loop thread; lets FlagsChanged events
    /// be reduced to press edges.
    static LAST_FLAGS: Cell<CGEventFlags> = const { Cell::new(CGEventFlags::empty()) };
}

/// The modifier flag bit raised by a given CGKeyCode, if it is a modifier.
fn modifier_flag_for_key(code: u16) -> Option<CGEventFlags> {
    match code {
        55 | 63 => Some(CGEventFlags::MaskCommand),
        56 | 60 => Some(CGEventFlags::MaskShift),
        57 => Some(CGEventFlags::MaskAlphaShift),
        58 | 61 => Some(CGEventFlags::MaskAlternate),
        59 | 62 => Some(CGEventFlags::MaskControl),
        _ => None,
    }
}

/// Map a CGEvent to a normalized `InputEvent`, or `None` if we don't care.
unsafe fn map_event(event_type: CGEventType, event: *const CGEvent) -> Option<InputEvent> {
    // SAFETY: the tap callback guarantees `event` is a valid CGEvent.
    let ev = unsafe { &*event };
    match event_type {
        CGEventType::KeyDown => {
            // field 8 is nonzero for OS auto-repeat; emit only the initial press.
            let repeat =
                CGEvent::integer_value_field(Some(ev), CGEventField::KeyboardEventAutorepeat);
            if repeat != 0 {
                return None;
            }
            let code = CGEvent::integer_value_field(Some(ev), CGEventField::KeyboardEventKeycode);
            Some(translator::macos::key_event(code as u16))
        }
        CGEventType::FlagsChanged => LAST_FLAGS.with(|last| {
            let code =
                CGEvent::integer_value_field(Some(ev), CGEventField::KeyboardEventKeycode) as u16;
            let Some(flag) = modifier_flag_for_key(code) else {
                return None;
            };
            let flags = CGEvent::flags(Some(ev));
            let prev = last.replace(flags);
            // CapsLock is a toggle: every state change is one physical press.
            // Other modifiers fire only on the off -> on (press) edge.
            let pressed = if code == 57 {
                flags.contains(flag) != prev.contains(flag)
            } else {
                flags.contains(flag) && !prev.contains(flag)
            };
            pressed.then(|| translator::macos::key_event(code))
        }),
        CGEventType::LeftMouseDown => Some(InputEvent::MouseDown(MouseButton::Left)),
        CGEventType::RightMouseDown => Some(InputEvent::MouseDown(MouseButton::Right)),
        CGEventType::OtherMouseDown => {
            let n = CGEvent::integer_value_field(Some(ev), CGEventField::MouseEventButtonNumber);
            Some(InputEvent::MouseDown(
                translator::macos::translate_button_number(n),
            ))
        }
        _ => None,
    }
}

/// CGEventTap callback (ListenOnly): forward the event unchanged.
unsafe extern "C-unwind" fn tap_callback(
    _proxy: CGEventTapProxy,
    event_type: CGEventType,
    event: std::ptr::NonNull<CGEvent>,
    _user_info: *mut c_void,
) -> *mut CGEvent {
    let event_ptr = event.as_ptr();
    // SAFETY: event is valid for the callback's duration; map_event never panics
    // and emit is non-blocking, so nothing can unwind across this FFI boundary.
    unsafe {
        if let Some(input) = map_event(event_type, event_ptr) {
            emit(input);
        }
    }
    event_ptr
}

fn event_mask() -> CGEventMask {
    mask_bit(CGEventType::KeyDown)
        | mask_bit(CGEventType::LeftMouseDown)
        | mask_bit(CGEventType::RightMouseDown)
        | mask_bit(CGEventType::OtherMouseDown)
        | mask_bit(CGEventType::FlagsChanged)
}

/// Owns the tap resources on the run-loop thread; the guard never leaves it,
/// so the drop below satisfies Core Foundation's thread-affinity rules.
struct TapGuard {
    port: CFRetained<CFMachPort>,
    source: CFRetained<CFRunLoopSource>,
    loop_ref: CFRetained<CFRunLoop>,
}

impl Drop for TapGuard {
    fn drop(&mut self) {
        // `tap_enable` and `remove_source` are safe objc2 wrappers; only the
        // extern static needs an unsafe block. All three must run on the
        // thread that created the guard (this guard never leaves it).
        CGEvent::tap_enable(&self.port, false);
        // SAFETY: `kCFRunLoopDefaultMode` is a valid constant for the process
        // lifetime.
        let mode = unsafe { kCFRunLoopDefaultMode };
        self.loop_ref.remove_source(Some(&self.source), mode);
    }
}

/// A thread-safe handle to stop a CFRunLoop from another thread.
struct RunLoopStopper(*const CFRunLoop);
unsafe impl Send for RunLoopStopper {}

impl RunLoopStopper {
    fn stop(&self) {
        // SAFETY: pointer came from a retained CFRunLoop kept alive by the
        // owning thread; CFRunLoopStop is thread-safe.
        unsafe {
            (&*self.0).stop();
        }
    }
}

/// Create the tap on the run-loop thread, report the outcome, then block in the
/// run loop until the session stops it.
fn run_tap(ready: bridge::Ready) {
    // SAFETY: `tap_create` requires the caller to uphold CGEventTap's FFI
    // contract (valid callback, live run loop); the callback never unwinds.
    let port = unsafe {
        CGEvent::tap_create(
            CGEventTapLocation::SessionEventTap,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::ListenOnly,
            event_mask(),
            Some(tap_callback),
            std::ptr::null_mut(),
        )
    };
    let Some(port) = port else {
        ready.fail(BackendError::Permission);
        return;
    };

    let Some(source) = CFMachPort::new_run_loop_source(None, Some(&port), 0) else {
        ready.fail(BackendError::Start(
            "failed to create run-loop source for event tap".into(),
        ));
        return;
    };
    let Some(loop_ref) = CFRunLoop::current() else {
        ready.fail(BackendError::Start("no current run loop".into()));
        return;
    };

    // SAFETY: `kCFRunLoopDefaultMode` is a valid constant for the process
    // lifetime.
    let mode = unsafe { kCFRunLoopDefaultMode };
    loop_ref.add_source(Some(&source), mode);
    CGEvent::tap_enable(&port, true);

    let guard = TapGuard {
        port,
        source,
        loop_ref: CFRetained::clone(&loop_ref),
    };

    let stopper = RunLoopStopper(CFRetained::as_ptr(&loop_ref).as_ptr());
    bridge::register_stop(Box::new(move || stopper.stop()));

    ready.ok();
    tracing::info!("macOS CGEventTap installed");

    CFRunLoop::run();
    drop(guard);
    tracing::info!("macOS event tap run loop exited");
}
