//! Notification-area (tray) icon with a context menu.
//!
//! The icon's hidden window only forwards shell notifications to the main
//! thread's message queue as `WM_TRAY` thread messages, so all application
//! logic stays in the main loop.

use std::sync::atomic::{AtomicU32, Ordering};

use windows::core::{w, Result, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconFromResourceEx, CreatePopupMenu, CreateWindowExW, DefWindowProcW,
    DestroyIcon, DestroyMenu, DestroyWindow, GetCursorPos, GetSystemMetrics, LoadIconW,
    PostMessageW, PostThreadMessageW, RegisterClassW, RegisterWindowMessageW, SetForegroundWindow,
    TrackPopupMenu, HICON, HMENU, IDI_APPLICATION, LR_DEFAULTCOLOR, MENU_ITEM_FLAGS, MF_CHECKED,
    MF_GRAYED, MF_SEPARATOR, MF_STRING, SM_CXSMICON, SM_CYSMICON, TPM_NONOTIFY, TPM_RETURNCMD,
    TPM_RIGHTBUTTON, WM_APP, WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP, WNDCLASSW, WS_EX_TOOLWINDOW,
    WS_POPUP,
};

/// Thread message carrying a tray event to the main loop.
pub const WM_TRAY: u32 = WM_APP + 1;
/// Callback message the shell sends to the tray window.
const WM_TRAY_CALLBACK: u32 = WM_APP + 2;

const CLASS_NAME: PCWSTR = w!("MouseTrackerTray");
const ICON_ID: u32 = 1;
const ICON_FILE: &[u8] = include_bytes!("../mt.ico");

/// "TaskbarCreated": broadcast when Explorer restarts and tray icons must
/// be added again.
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    /// The user clicked the icon; show the menu.
    Click,
    /// Explorer restarted; add the icon again.
    Recreate,
}

const EVENT_CLICK: usize = 1;
const EVENT_RECREATE: usize = 2;

/// Decodes a `WM_TRAY` thread message.
pub fn event_from(wparam: WPARAM) -> Option<TrayEvent> {
    match wparam.0 {
        EVENT_CLICK => Some(TrayEvent::Click),
        EVENT_RECREATE => Some(TrayEvent::Recreate),
        _ => None,
    }
}

pub enum MenuItem<T> {
    Action { label: String, value: T, checked: bool, enabled: bool },
    Info(String),
    Separator,
}

pub struct Tray {
    hwnd: HWND,
    icon: HICON,
    owns_icon: bool,
    tooltip: String,
}

impl Tray {
    pub fn new(tooltip: &str) -> Result<Self> {
        let instance = unsafe { GetModuleHandleW(None)? };
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance.into(),
            lpszClassName: CLASS_NAME,
            ..Default::default()
        };
        unsafe {
            RegisterClassW(&class);
            TASKBAR_CREATED.store(RegisterWindowMessageW(w!("TaskbarCreated")), Ordering::Relaxed);
        }
        // A hidden top-level window rather than a message-only one: the menu
        // needs a window that can become foreground.
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW,
                CLASS_NAME,
                w!("Mouse Tracker"),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                instance,
                None,
            )?
        };
        let (icon, owns_icon) = match load_icon() {
            Some(icon) => (icon, true),
            None => (unsafe { LoadIconW(None, IDI_APPLICATION)? }, false),
        };
        let tray = Self { hwnd, icon, owns_icon, tooltip: tooltip.to_owned() };
        tray.add();
        Ok(tray)
    }

    pub fn set_tooltip(&mut self, tooltip: &str) {
        if self.tooltip != tooltip {
            self.tooltip = tooltip.to_owned();
            let data = self.data();
            unsafe {
                let _ = Shell_NotifyIconW(NIM_MODIFY, &data);
            }
        }
    }

    /// Shows `items` at the cursor and returns the chosen action's value.
    pub fn show_menu<T: Copy>(&self, items: &[MenuItem<T>]) -> Option<T> {
        let menu = unsafe { CreatePopupMenu() }.ok()?;
        for (i, item) in items.iter().enumerate() {
            // Command id 0 means "cancelled", so ids are index + 1.
            let id = i + 1;
            let result = match item {
                MenuItem::Action { label, checked, enabled, .. } => {
                    let mut flags = MF_STRING;
                    if *checked {
                        flags |= MF_CHECKED;
                    }
                    if !*enabled {
                        flags |= MF_GRAYED;
                    }
                    append(menu, flags, id, label)
                }
                MenuItem::Info(text) => append(menu, MF_STRING | MF_GRAYED, id, text),
                MenuItem::Separator => unsafe { AppendMenuW(menu, MF_SEPARATOR, 0, None) },
            };
            if result.is_err() {
                unsafe {
                    let _ = DestroyMenu(menu);
                }
                return None;
            }
        }

        let mut cursor = Default::default();
        let chosen = unsafe {
            let _ = GetCursorPos(&mut cursor);
            // Without this the menu does not close when clicking elsewhere.
            let _ = SetForegroundWindow(self.hwnd);
            let chosen = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
                cursor.x,
                cursor.y,
                0,
                self.hwnd,
                None,
            );
            let _ = PostMessageW(self.hwnd, WM_NULL, WPARAM(0), LPARAM(0));
            let _ = DestroyMenu(menu);
            chosen.0
        };

        let index = usize::try_from(chosen).ok()?.checked_sub(1)?;
        match items.get(index)? {
            MenuItem::Action { value, enabled: true, .. } => Some(*value),
            _ => None,
        }
    }

    /// Adds the icon to the notification area; call again on `Recreate`.
    pub fn add(&self) {
        let data = self.data();
        unsafe {
            let _ = Shell_NotifyIconW(NIM_ADD, &data);
        }
    }

    fn data(&self) -> NOTIFYICONDATAW {
        let mut data = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: ICON_ID,
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
            uCallbackMessage: WM_TRAY_CALLBACK,
            hIcon: self.icon,
            ..Default::default()
        };
        // Leave room for the terminating NUL.
        let max = data.szTip.len() - 1;
        for (dst, src) in data.szTip.iter_mut().zip(self.tooltip.encode_utf16().take(max)) {
            *dst = src;
        }
        data
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        let data = self.data();
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &data);
            if self.owns_icon {
                let _ = DestroyIcon(self.icon);
            }
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

fn append(menu: HMENU, flags: MENU_ITEM_FLAGS, id: usize, text: &str) -> Result<()> {
    let text: Vec<u16> = text.encode_utf16().chain([0]).collect();
    unsafe { AppendMenuW(menu, flags, id, PCWSTR(text.as_ptr())) }
}

/// Creates the icon embedded from `mt.ico`, picking the image that best
/// fits the small-icon size.
fn load_icon() -> Option<HICON> {
    let (cx, cy) = unsafe { (GetSystemMetrics(SM_CXSMICON), GetSystemMetrics(SM_CYSMICON)) };
    let image = best_icon_image(ICON_FILE, cx.max(1) as u32)?;
    unsafe { CreateIconFromResourceEx(image, true, 0x0003_0000, cx, cy, LR_DEFAULTCOLOR) }.ok()
}

/// Returns the image data from an `.ico` file whose width is the smallest
/// one at least `size`, or the largest image when all are smaller.
fn best_icon_image(ico: &[u8], size: u32) -> Option<&[u8]> {
    let u16_at = |at: usize| Some(u16::from_le_bytes(ico.get(at..at + 2)?.try_into().ok()?));
    let u32_at = |at: usize| Some(u32::from_le_bytes(ico.get(at..at + 4)?.try_into().ok()?));
    if u16_at(0)? != 0 || u16_at(2)? != 1 {
        return None;
    }
    let mut images = Vec::new();
    for i in 0..usize::from(u16_at(4)?) {
        let entry = 6 + 16 * i;
        // A stored width of 0 means 256.
        let width = match *ico.get(entry)? {
            0 => 256,
            w => u32::from(w),
        };
        let len = u32_at(entry + 8)? as usize;
        let offset = u32_at(entry + 12)? as usize;
        images.push((width, ico.get(offset..offset.checked_add(len)?)?));
    }
    let fitting = images.iter().filter(|(w, _)| *w >= size).min_by_key(|(w, _)| *w);
    fitting.or_else(|| images.iter().max_by_key(|(w, _)| *w)).map(|(_, data)| *data)
}

extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let event = if msg == WM_TRAY_CALLBACK {
        // Legacy notify version: lParam holds the mouse message.
        let mouse = lparam.0 as u32 & 0xFFFF;
        (mouse == WM_LBUTTONUP || mouse == WM_RBUTTONUP).then_some(EVENT_CLICK)
    } else if msg != 0 && msg == TASKBAR_CREATED.load(Ordering::Relaxed) {
        Some(EVENT_RECREATE)
    } else {
        None
    };
    if let Some(event) = event {
        unsafe {
            let _ = PostThreadMessageW(GetCurrentThreadId(), WM_TRAY, WPARAM(event), LPARAM(0));
        }
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds an .ico with one entry per width; image data is the width byte.
    fn ico(widths: &[u8]) -> Vec<u8> {
        let mut out = vec![0, 0, 1, 0, widths.len() as u8, 0];
        let data_start = 6 + 16 * widths.len();
        for (i, w) in widths.iter().enumerate() {
            out.extend([*w, *w, 0, 0, 1, 0, 32, 0]);
            out.extend(1u32.to_le_bytes());
            out.extend(((data_start + i) as u32).to_le_bytes());
        }
        out.extend(widths);
        out
    }

    #[test]
    fn embedded_icon_parses() {
        assert!(best_icon_image(ICON_FILE, 16).is_some());
    }

    #[test]
    fn picks_smallest_image_at_least_size() {
        let file = ico(&[48, 16, 32]);
        assert_eq!(best_icon_image(&file, 20), Some(&[32u8][..]));
        assert_eq!(best_icon_image(&file, 16), Some(&[16u8][..]));
    }

    #[test]
    fn falls_back_to_largest_and_reads_zero_as_256() {
        assert_eq!(best_icon_image(&ico(&[16, 32]), 64), Some(&[32u8][..]));
        assert_eq!(best_icon_image(&ico(&[32, 0]), 64), Some(&[0u8][..]));
    }

    #[test]
    fn rejects_malformed_files() {
        assert_eq!(best_icon_image(&[], 16), None);
        assert_eq!(best_icon_image(&[0, 0, 2, 0, 0, 0], 16), None);
        let mut truncated = ico(&[16]);
        truncated.pop();
        assert_eq!(best_icon_image(&truncated, 16), None);
    }

    #[test]
    fn decodes_events() {
        assert_eq!(event_from(WPARAM(EVENT_CLICK)), Some(TrayEvent::Click));
        assert_eq!(event_from(WPARAM(EVENT_RECREATE)), Some(TrayEvent::Recreate));
        assert_eq!(event_from(WPARAM(0)), None);
    }
}
