# macos_input.py
import Quartz

class MacOSInputListener:
    def __init__(self, play_sound_callback, quartz_module):
        self.play_sound = play_sound_callback
        self.Quartz = quartz_module
        self.event_tap_ref = None

    def _handle_input_event(self, proxy, event_type, event, refcon):
        """Callback for input events on macOS"""
        # Get the keycode for keyboard events
        if event_type == Quartz.kCGEventKeyDown or event_type == Quartz.kCGEventKeyUp:
            keycode = Quartz.CGEventGetIntegerValueField(event, Quartz.kCGKeyboardEventKeycode)
            self.play_sound(keycode)
        # Handle mouse events
        elif event_type in [Quartz.kCGEventLeftMouseDown, 
                           Quartz.kCGEventRightMouseDown, 
                           Quartz.kCGEventOtherMouseDown]:
            # Map mouse buttons to codes
            button_number = Quartz.CGEventGetIntegerValueField(event, Quartz.kCGMouseEventButtonNumber)
            if button_number == 0:  # Left button
                button_code = 0x01
            elif button_number == 1:  # Right button
                button_code = 0x02
            elif button_number == 2:  # Middle button
                button_code = 0x04
            else:
                button_code = 0x05  # Other buttons
                
            self.play_sound(button_code)
        
        return event

    def run(self):
        print("\033[1;32m[INFO]\033[0m Starting WayClick for macOS...")
        
        # Create an event tap to monitor input events
        self.event_tap_ref = Quartz.CGEventTapCreate(
            Quartz.kCGSessionEventTap,
            Quartz.kCGHeadInsertEventTap,
            Quartz.kCGEventTapOptionListenOnly,
            Quartz.CGEventMaskBit(Quartz.kCGEventKeyDown) |
            Quartz.CGEventMaskBit(Quartz.kCGEventKeyUp) |
            Quartz.CGEventMaskBit(Quartz.kCGEventLeftMouseDown) |
            Quartz.CGEventMaskBit(Quartz.kCGEventRightMouseDown) |
            Quartz.CGEventMaskBit(Quartz.kCGEventOtherMouseDown),
            self._handle_input_event,
            None
        )
        
        if self.event_tap_ref is None:
            print("\033[1;31m[ERROR]\033[0m Failed to create event tap")
            return 1
        
        # Enable the event tap
        Quartz.CFRunLoopAddSource(
            Quartz.CFRunLoopGetCurrent(),
            Quartz.CGEventTapCreateRunLoopSource(None, self.event_tap_ref, 0),
            Quartz.kCFRunLoopDefaultMode
        )
        
        Quartz.CGEventTapEnable(self.event_tap_ref, True)
        
        try:
            # Start the run loop to listen for events
            Quartz.CFRunLoopRun()
        except KeyboardInterrupt:
            print("\033[1;33m[INFO]\033[0m Shutting down...")
        finally:
            if self.event_tap_ref:
                Quartz.CGEventTapEnable(self.event_tap_ref, False)
                Quartz.CFRelease(self.event_tap_ref)
        
        return 0