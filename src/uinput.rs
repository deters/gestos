#[cfg(target_os = "linux")]
use evdev::uinput::VirtualDevice;
#[cfg(target_os = "linux")]
use std::sync::{Arc, Mutex};

/// Maps a trigger button argument to its evdev code. Only mouse and digitizer buttons are
/// accepted: the service holding `input` group rights must not act as a way to observe
/// keyboard keys.
pub fn trigger_button_code(button: i32) -> Option<u16> {
    match button {
        1 => Some(0x110), // BTN_LEFT
        2 => Some(0x112), // BTN_MIDDLE
        3 => Some(0x111), // BTN_RIGHT
        8 => Some(0x113), // BTN_SIDE
        9 => Some(0x114), // BTN_EXTRA
        0x110..=0x11f | 0x140..=0x14f => Some(button as u16),
        _ => None,
    }
}

/// Shared handle to the virtual device. The lock is held only for each write, so an action
/// that is sleeping between key events never delays pointer forwarding.
#[cfg(target_os = "linux")]
#[derive(Clone)]
pub struct UinputDevice {
    device: Arc<Mutex<VirtualDevice>>,
}

#[cfg(target_os = "linux")]
impl UinputDevice {
    pub fn init_from_device(source_dev: &evdev::Device) -> Result<Self, std::io::Error> {
        let mut keys = evdev::AttributeSet::<evdev::KeyCode>::new();
        // Enable standard keyboard keys so virtual device can type shortcuts
        for code in 1..255 {
            if (0x110..=0x11f).contains(&code) {
                continue; // Skip mouse button range to preserve source bindings
            }
            keys.insert(evdev::KeyCode(code));
        }

        let mut builder = VirtualDevice::builder()?
            .name(source_dev.name().unwrap_or("Virtual Gestos Forwarder"));

        // Add keyboard keys
        builder = builder.with_keys(&keys)?;

        // Copy source device mouse buttons
        if let Some(src_keys) = source_dev.supported_keys() {
            let mut mouse_keys = evdev::AttributeSet::<evdev::KeyCode>::new();
            for k in src_keys.iter() {
                if k.0 >= 0x110 && k.0 <= 0x11f {
                    mouse_keys.insert(k);
                }
            }
            builder = builder.with_keys(&mouse_keys)?;
        }

        // Copy relative/absolute axes
        if let Some(src_rel) = source_dev.supported_relative_axes() {
            builder = builder.with_relative_axes(src_rel)?;
        }
        if let Ok(abs_axes) = source_dev.get_absinfo() {
            for (code, info) in abs_axes {
                let abs_setup = evdev::UinputAbsSetup::new(code, info);
                builder = builder.with_absolute_axis(&abs_setup)?;
            }
        }

        let device = builder.build()?;
        Ok(UinputDevice {
            device: Arc::new(Mutex::new(device)),
        })
    }

    // `VirtualDevice::emit` terminates each batch with its own SYN_REPORT.
    fn emit(&self, events: &[evdev::InputEvent]) {
        let mut device = self.device.lock().unwrap_or_else(|e| e.into_inner());
        let _ = device.emit(events);
    }

    pub fn click(&self, button: i32) {
        let ev_button = match button {
            1 => evdev::KeyCode::BTN_LEFT,
            2 => evdev::KeyCode::BTN_MIDDLE,
            3 => evdev::KeyCode::BTN_RIGHT,
            8 => evdev::KeyCode::BTN_SIDE,
            9 => evdev::KeyCode::BTN_EXTRA,
            other => evdev::KeyCode(other as u16),
        };

        let press = evdev::InputEvent::new(evdev::EventType::KEY.0, ev_button.0, 1);
        let release = evdev::InputEvent::new(evdev::EventType::KEY.0, ev_button.0, 0);

        self.emit(&[press]);
        std::thread::sleep(std::time::Duration::from_millis(50));
        self.emit(&[release]);
    }

    /// Presses or releases one key.
    pub fn key(&self, code: u16, pressed: bool) {
        self.emit(&[evdev::InputEvent::new(evdev::EventType::KEY.0, code, pressed as i32)]);
    }

    /// Presses and releases a single key.
    pub fn tap_key(&self, code: u16) {
        self.emit(&[evdev::InputEvent::new(evdev::EventType::KEY.0, code, 1)]);
        self.emit(&[evdev::InputEvent::new(evdev::EventType::KEY.0, code, 0)]);
    }

    /// Forwards one input frame, without its SYN_REPORT.
    pub fn forward_frame(&self, events: &[evdev::InputEvent]) {
        self.emit(events);
    }
}

#[cfg(not(target_os = "linux"))]
#[derive(Clone)]
pub struct UinputDevice;

#[cfg(not(target_os = "linux"))]
impl UinputDevice {
    pub fn click(&self, button: i32) {
        log::info!("uinput click stub: {}", button);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evdev::KeyCode;

    #[test]
    fn trigger_codes_are_the_real_button_codes() {
        assert_eq!(trigger_button_code(1), Some(KeyCode::BTN_LEFT.0));
        assert_eq!(trigger_button_code(2), Some(KeyCode::BTN_MIDDLE.0));
        assert_eq!(trigger_button_code(3), Some(KeyCode::BTN_RIGHT.0));
        assert_eq!(trigger_button_code(8), Some(KeyCode::BTN_SIDE.0));
        assert_eq!(trigger_button_code(9), Some(KeyCode::BTN_EXTRA.0));
    }

    #[test]
    fn trigger_accepts_mouse_and_digitizer_buttons() {
        assert_eq!(trigger_button_code(1), Some(0x110));
        assert_eq!(trigger_button_code(3), Some(0x111));
        assert_eq!(trigger_button_code(9), Some(0x114));
        assert_eq!(trigger_button_code(0x11f), Some(0x11f));
        assert_eq!(trigger_button_code(0x14a), Some(0x14a));
    }

    #[test]
    fn trigger_rejects_keyboard_codes() {
        for code in [-1, 0, 4, 7, 30, 0x10f, 0x120, 0x13f, 0x150, 0x1_0110] {
            assert_eq!(trigger_button_code(code), None, "code {code}");
        }
    }

}
