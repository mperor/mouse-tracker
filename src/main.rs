// Release builds have no console window; debug builds keep it for logs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod hotkeys;
mod input;
mod model;
mod overlay;
mod player;
mod tray;

use windows::core::w;
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;
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

    // A second instance would only fail to register the hotkeys.
    let Some(_instance) = acquire_single_instance() else {
        app::show_info("Mouse Tracker", "Mouse Tracker is already running.");
        return;
    };

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

/// Returns the per-session instance mutex, or `None` when another instance
/// already holds it. The handle is closed by the OS when the process exits.
fn acquire_single_instance() -> Option<HANDLE> {
    let mutex = unsafe { CreateMutexW(None, true, w!("Local\\MouseTracker.SingleInstance")) }.ok()?;
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        return None;
    }
    Some(mutex)
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
        if msg.message == tray::WM_TRAY && msg.hwnd.is_invalid() {
            if let Some(event) = tray::event_from(msg.wParam) {
                if app.on_tray(event).is_break() {
                    break;
                }
            }
            continue;
        }
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
