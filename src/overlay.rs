//! On-screen track of recorded steps and the live cursor-coordinates label.
//!
//! Both are click-through, always-on-top layered windows (hidden from the
//! taskbar). Content is rendered into a 32-bit buffer and pushed with
//! `UpdateLayeredWindow`, so it survives repaints and other windows.

use std::ffi::c_void;
use std::mem::size_of;
use std::ptr;
use std::slice;

use windows::core::{w, Result, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, CreateFontW, CreateSolidBrush, DeleteDC, DeleteObject,
    FillRect, GdiFlush, GetTextExtentPoint32W, SelectObject, SetBkMode, SetTextColor, TextOutW,
    AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION,
    CLEARTYPE_QUALITY, DEFAULT_CHARSET, DIB_RGB_COLORS, FF_DONTCARE, FW_BOLD, HDC, HFONT, HGDIOBJ,
    OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForSystem;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetSystemMetrics, RegisterClassW, ShowWindow,
    UpdateLayeredWindow, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN, SW_HIDE, SW_SHOWNOACTIVATE, ULW_ALPHA, WNDCLASSW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

use crate::input;
use crate::model::{Action, Step};

const CLASS_NAME: PCWSTR = w!("MouseTrackerOverlay");
const FONT_NAME: PCWSTR = w!("Trebuchet MS");
const FONT_POINTS: i32 = 10;

/// Space between label text and border, and border width.
const PADDING: i32 = 2;
const BORDER: i32 = 2;
/// Step label offset from the recorded point.
const STEP_LABEL_OFFSET: (i32, i32) = (4, 0);
/// Cursor label offset from the cursor, as in the AutoIt ToolTip.
const CURSOR_LABEL_OFFSET: (i32, i32) = (10, 20);

const WHITE: u32 = 0xFFFFFF;
const CURSOR_LABEL_COLOR: u32 = 0x333333;

/// Label background colour per action (0xRRGGBB), as in the AutoIt version.
pub fn action_color(action: Action) -> u32 {
    match action {
        Action::Move => 0x18A1BF,
        Action::RightClick => 0x18BF36,
        Action::Click => 0xFFA500,
        Action::DoubleClick => 0xBF3618,
        Action::Drag => 0xBF18A1,
    }
}

pub struct Overlay {
    renderer: Renderer,
    track: HWND,
    cursor: HWND,
    last_cursor: Option<(i32, i32)>,
}

impl Overlay {
    pub fn new() -> Result<Self> {
        register_class()?;
        Ok(Self {
            renderer: Renderer::new()?,
            track: create_window()?,
            cursor: create_window()?,
            last_cursor: None,
        })
    }

    /// Shows numbered labels for `steps`, replacing anything drawn before.
    pub fn show(&mut self, steps: &[Step]) {
        if steps.is_empty() {
            self.hide();
            return;
        }
        let origin = POINT {
            x: unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) },
            y: unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) },
        };
        let size = SIZE {
            cx: unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) },
            cy: unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) },
        };
        let labels: Vec<Label> = steps
            .iter()
            .enumerate()
            .map(|(i, step)| {
                let at = (
                    step.x - origin.x + STEP_LABEL_OFFSET.0,
                    step.y - origin.y + STEP_LABEL_OFFSET.1,
                );
                self.renderer.label(&(i + 1).to_string(), at, action_color(step.action))
            })
            .collect();
        self.renderer.present(self.track, origin, size, &labels);
        unsafe {
            let _ = ShowWindow(self.track, SW_SHOWNOACTIVATE);
        }
    }

    pub fn hide(&mut self) {
        unsafe {
            let _ = ShowWindow(self.track, SW_HIDE);
        }
    }

    /// Moves the coordinates label to follow the cursor. Call periodically.
    pub fn update_cursor(&mut self) {
        let pos = input::cursor_pos();
        if pos == self.last_cursor {
            return;
        }
        self.last_cursor = pos;
        let Some((x, y)) = pos else {
            unsafe {
                let _ = ShowWindow(self.cursor, SW_HIDE);
            }
            return;
        };
        let text = format!("Current coordinates: [{x}, {y}]");
        let label = self.renderer.label(&text, (0, 0), CURSOR_LABEL_COLOR);
        let size = SIZE {
            cx: label.rect.right,
            cy: label.rect.bottom,
        };
        let origin = POINT {
            x: x + CURSOR_LABEL_OFFSET.0,
            y: y + CURSOR_LABEL_OFFSET.1,
        };
        self.renderer.present(self.cursor, origin, size, &[label]);
        unsafe {
            let _ = ShowWindow(self.cursor, SW_SHOWNOACTIVATE);
        }
    }
}

impl Drop for Overlay {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.track);
            let _ = DestroyWindow(self.cursor);
        }
    }
}

/// A label laid out in window coordinates: `rect` is the outer box
/// including the border.
struct Label {
    rect: RECT,
    text: Vec<u16>,
    color: u32,
}

/// GDI memory DC with the label font selected.
struct Renderer {
    dc: HDC,
    font: HFONT,
}

impl Renderer {
    fn new() -> Result<Self> {
        let height = -(FONT_POINTS * unsafe { GetDpiForSystem() } as i32 / 72);
        unsafe {
            let dc = CreateCompatibleDC(None);
            let font = CreateFontW(
                height,
                0,
                0,
                0,
                FW_BOLD.0 as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET.0 as u32,
                OUT_DEFAULT_PRECIS.0 as u32,
                CLIP_DEFAULT_PRECIS.0 as u32,
                CLEARTYPE_QUALITY.0 as u32,
                FF_DONTCARE.0 as u32,
                FONT_NAME,
            );
            SelectObject(dc, font);
            SetBkMode(dc, TRANSPARENT);
            SetTextColor(dc, colorref(WHITE));
            Ok(Self { dc, font })
        }
    }

    fn label(&self, text: &str, at: (i32, i32), color: u32) -> Label {
        let text: Vec<u16> = text.encode_utf16().collect();
        let mut extent = SIZE::default();
        unsafe {
            let _ = GetTextExtentPoint32W(self.dc, &text, &mut extent);
        }
        let inset = PADDING + BORDER;
        Label {
            rect: RECT {
                left: at.0,
                top: at.1,
                right: at.0 + extent.cx + 2 * inset,
                bottom: at.1 + extent.cy + 2 * inset,
            },
            text,
            color,
        }
    }

    /// Renders `labels` onto a transparent buffer of `size` and shows it in
    /// `hwnd` at screen position `origin`.
    fn present(&self, hwnd: HWND, origin: POINT, size: SIZE, labels: &[Label]) {
        if size.cx <= 0 || size.cy <= 0 {
            return;
        }
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size.cx,
                biHeight: -size.cy, // top-down rows
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut c_void = ptr::null_mut();
        let Ok(bitmap) =
            (unsafe { CreateDIBSection(self.dc, &info, DIB_RGB_COLORS, &mut bits, None, 0) })
        else {
            return;
        };

        unsafe {
            let previous = SelectObject(self.dc, bitmap);
            let pixels = slice::from_raw_parts_mut(bits.cast::<u32>(), (size.cx * size.cy) as usize);
            pixels.fill(0);

            for label in labels {
                self.draw_label(label);
            }
            let _ = GdiFlush();
            // GDI leaves alpha at 0; labels are opaque, so mark their pixels.
            for label in labels {
                set_opaque(pixels, size, &label.rect);
            }

            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let _ = UpdateLayeredWindow(
                hwnd,
                None,
                Some(&origin),
                Some(&size),
                self.dc,
                Some(&POINT::default()),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );

            SelectObject(self.dc, previous);
            let _ = DeleteObject(bitmap);
        }
    }

    fn draw_label(&self, label: &Label) {
        let r = label.rect;
        let inner = RECT {
            left: r.left + BORDER,
            top: r.top + BORDER,
            right: r.right - BORDER,
            bottom: r.bottom - BORDER,
        };
        unsafe {
            fill(self.dc, &r, WHITE);
            fill(self.dc, &inner, label.color);
            let _ = TextOutW(self.dc, inner.left + PADDING, inner.top + PADDING, &label.text);
        }
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteDC(self.dc);
            let _ = DeleteObject(self.font);
        }
    }
}

unsafe fn fill(dc: HDC, rect: &RECT, color: u32) {
    let brush = CreateSolidBrush(colorref(color));
    FillRect(dc, rect, brush);
    let _ = DeleteObject(HGDIOBJ::from(brush));
}

fn set_opaque(pixels: &mut [u32], size: SIZE, rect: &RECT) {
    let (left, right) = (rect.left.max(0), rect.right.min(size.cx));
    let (top, bottom) = (rect.top.max(0), rect.bottom.min(size.cy));
    for y in top..bottom {
        let row = (y * size.cx) as usize;
        for pixel in &mut pixels[row + left as usize..row + right.max(left) as usize] {
            *pixel |= 0xFF00_0000;
        }
    }
}

/// 0xRRGGBB to a GDI COLORREF (0x00BBGGRR).
fn colorref(rgb: u32) -> COLORREF {
    let (r, g, b) = ((rgb >> 16) & 0xFF, (rgb >> 8) & 0xFF, rgb & 0xFF);
    COLORREF(b << 16 | g << 8 | r)
}

fn register_class() -> Result<()> {
    let class = WNDCLASSW {
        lpfnWndProc: Some(window_proc),
        hInstance: unsafe { GetModuleHandleW(None)? }.into(),
        lpszClassName: CLASS_NAME,
        ..Default::default()
    };
    unsafe { RegisterClassW(&class) };
    Ok(())
}

fn create_window() -> Result<HWND> {
    unsafe {
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            CLASS_NAME,
            w!(""),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            GetModuleHandleW(None)?,
            None,
        )
    }
}

extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colorref_swaps_red_and_blue() {
        assert_eq!(colorref(0x112233), COLORREF(0x332211));
    }

    #[test]
    fn set_opaque_clips_to_buffer() {
        let size = SIZE { cx: 4, cy: 3 };
        let mut pixels = vec![0u32; 12];
        let rect = RECT { left: 2, top: -1, right: 10, bottom: 2 };
        set_opaque(&mut pixels, size, &rect);
        let opaque: Vec<usize> = (0..12).filter(|&i| pixels[i] != 0).collect();
        assert_eq!(opaque, vec![2, 3, 6, 7]);
    }
}
