//! Thin wrappers over the Win32 cursor and mouse input APIs.

use std::mem::size_of;
use std::thread;
use std::time::Duration;

use windows::Win32::Foundation::POINT;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEINPUT, MOUSE_EVENT_FLAGS,
};
use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, SetCursorPos};

/// Pause between clicks of a double-click; well under the default
/// double-click time (500 ms).
const CLICK_GAP: Duration = Duration::from_millis(30);
/// How long a button is held during a click (AutoIt's MouseClickDownDelay).
const CLICK_HOLD: Duration = Duration::from_millis(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
}

/// Current cursor position, or `None` when it cannot be read
/// (e.g. while the secure desktop is shown).
pub fn cursor_pos() -> Option<(i32, i32)> {
    let mut point = POINT::default();
    unsafe { GetCursorPos(&mut point) }.ok().map(|_| (point.x, point.y))
}

pub fn set_cursor_pos(x: i32, y: i32) {
    let _ = unsafe { SetCursorPos(x, y) };
}

pub fn button_down(button: Button) {
    send_mouse(match button {
        Button::Left => MOUSEEVENTF_LEFTDOWN,
        Button::Right => MOUSEEVENTF_RIGHTDOWN,
    });
}

pub fn button_up(button: Button) {
    send_mouse(match button {
        Button::Left => MOUSEEVENTF_LEFTUP,
        Button::Right => MOUSEEVENTF_RIGHTUP,
    });
}

pub fn click(button: Button, count: u32) {
    for i in 0..count {
        if i > 0 {
            thread::sleep(CLICK_GAP);
        }
        button_down(button);
        thread::sleep(CLICK_HOLD);
        button_up(button);
    }
}

/// Sends a button event at the current cursor position. Movement is done
/// with `SetCursorPos`, which avoids normalising absolute coordinates.
fn send_mouse(flags: MOUSE_EVENT_FLAGS) {
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    unsafe { SendInput(&[input], size_of::<INPUT>() as i32) };
}
