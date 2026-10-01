//! macOS backend: global event tap via objc2 (CGEventTap, ListenOnly).
//!
//! `CGEvent::tap_create` with `SessionEventTap`/`ListenOnly`; the callback pushes
//! normalized events through the shared [`bridge`]. `None` from `tap_create`
//! means the Accessibility permission is missing.

use std::ffi::c_void;
use std::time::Duration;

use futures::stream::BoxStream;
use objc2_core_foundation::{CFMachPort, CFRunLoop, kCFRunLoopDefaultMode};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventMask, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventTapProxy, CGEventType,
};

use crate::backend::bridge::{self, Ready, emit};
use crate::backend::{BackendError, InputBackend};
use crate::domain::{InputEvent, MouseButton};

/// macOS input backend using a CGEventTap.
///
/// The CGEventTap layer does not distinguish a trackpad from a mouse, so there
/// is no trackpad policy here (see `backend::evdev_backend`).
pub struct MacosBackend;

impl InputBackend for MacosBackend {
    fn events(&mut self) -> Result<BoxStream<'static, InputEvent>, BackendError> {
        bridge::start_bridged("wayclick-cgeventtap", Some(Duration::from_secs(5)), run_tap)
    }

    fn name(&self) -> &'static str {
        "macos-cgeventtap"
    }
}

/// `CGEventMaskBit` is a C macro objc2 does not generate: bit `n` of the mask is
/// event type `n`.
fn mask_bit(ty: CGEventType) -> CGEventMask {
    1u64 << (ty.0 as u64)
}

/// Map a CGEvent to a normalized `InputEvent`, or `None` if we don't care.
unsafe fn map_event(event_type: CGEventType, event: *const CGEvent) -> Option<InputEvent> {
    // SAFETY: the tap callback guarantees `event` is a valid CGEvent.
    let ev = unsafe { &*event };
    // `CGEventType` is a transparent struct of associated consts, not an enum.
    if event_type == CGEventType::KeyDown {
        // field 8 is nonzero for OS auto-repeat; emit only the initial press.
        let repeat = CGEvent::integer_value_field(Some(ev), CGEventField::KeyboardEventAutorepeat);
        if repeat != 0 {
            return None;
        }
        let code = CGEvent::integer_value_field(Some(ev), CGEventField::KeyboardEventKeycode);
        Some(InputEvent::Key(code as u16))
    } else if event_type == CGEventType::LeftMouseDown {
        Some(InputEvent::Mouse(MouseButton::Left))
    } else if event_type == CGEventType::RightMouseDown {
        Some(InputEvent::Mouse(MouseButton::Right))
    } else if event_type == CGEventType::OtherMouseDown {
        let n = CGEvent::integer_value_field(Some(ev), CGEventField::MouseEventButtonNumber);
        Some(InputEvent::Mouse(MouseButton::from_cg_button_number(n)))
    } else {
        None
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
}

/// Create the tap on the run-loop thread, report the outcome, then block in the
/// run loop for the process lifetime.
fn run_tap(ready: Ready) {
    // SAFETY: the FFI calls below follow the objc2 contracts.
    unsafe {
        let mach_port = CGEvent::tap_create(
            CGEventTapLocation::SessionEventTap,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::ListenOnly,
            event_mask(),
            Some(tap_callback),
            std::ptr::null_mut(),
        );

        let Some(mach_port) = mach_port else {
            ready.fail(BackendError::Permission);
            return;
        };

        let source = CFMachPort::new_run_loop_source(None, Some(&mach_port), 0);
        let run_loop = CFRunLoop::current();
        if let (Some(rl), Some(src)) = (run_loop.as_deref(), source.as_deref()) {
            rl.add_source(Some(src), kCFRunLoopDefaultMode);
        }
        CGEvent::tap_enable(&mach_port, true);

        ready.ok();

        // Blocks for the process lifetime; the stream ends when the sender drops.
        CFRunLoop::run();
    }
}
