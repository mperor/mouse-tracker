// Release builds have no console window; debug builds keep it for logs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod hotkeys;
mod input;
mod model;
mod overlay;
mod player;

use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, KillTimer, SetTimer, TranslateMessage, MSG, WM_HOTKEY,
    WM_TIMER,
};

use app::App;

/// How often the cursor-coordinates label is refreshed.
const TICK_MS: u32 = 100;

fn main() {
    // Physical pixel coordinates on every monitor, regardless of scaling.
    // Must run before any window is created.
    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };

    let mut app = match App::new() {
        Ok(app) => app,
        Err(e) => {
            app::show_error("Mouse Tracker", &format!("Failed to create the overlay: {e}"));
            return;
        }
    };
    let registered = register_hotkeys();
    let timer = unsafe { SetTimer(None, 0, TICK_MS, None) };
    run_message_loop(&mut app);

    unsafe {
        let _ = KillTimer(None, timer);
    }

    for id in registered {
        hotkeys::unregister(id);
    }
}

/// Registers every binding. A combination already taken by another
/// program is reported and skipped.
fn register_hotkeys() -> Vec<i32> {
    let mut registered = Vec::new();
    let mut failed = Vec::new();
    for (id, binding) in hotkeys::BINDINGS.iter().enumerate() {
        let id = id as i32;
        match hotkeys::register(id, binding) {
            Ok(()) => registered.push(id),
            Err(e) => failed.push(format!("{} ({}): {e}", binding.keys, binding.description)),
        }
    }
    if !failed.is_empty() {
        app::show_error(
            "Hotkeys unavailable",
            &format!("These hotkeys could not be registered:\n{}", failed.join("\n")),
        );
    }
    registered
}

fn run_message_loop(app: &mut App) {
    let mut msg = MSG::default();
    // GetMessageW returns 0 on WM_QUIT and -1 on error.
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {
        if msg.message == WM_HOTKEY {
            if let Some(command) = hotkeys::command_for_id(msg.wParam.0 as i32) {
                if app.handle(command).is_break() {
                    break;
                }
            }
            continue;
        }
        // Thread timer (no window): the only WM_TIMER without an hwnd.
        if msg.message == WM_TIMER && msg.hwnd.is_invalid() {
            app.tick();
            continue;
        }
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
