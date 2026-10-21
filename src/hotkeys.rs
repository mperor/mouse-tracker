//! Global hotkey table. Every binding lives here, so remapping a key is a
//! one-line change. A binding's index in `BINDINGS` is its hotkey id.

use windows::core::Result;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS,
};

use crate::model::Action;

// Win32 RegisterHotKey modifier flags.
pub const MOD_ALT: u32 = 0x0001;
pub const MOD_CONTROL: u32 = 0x0002;
pub const MOD_SHIFT: u32 = 0x0004;
pub const MOD_NOREPEAT: u32 = 0x4000;

const CTRL_ALT: u32 = MOD_CONTROL | MOD_ALT;
const CTRL_ALT_SHIFT: u32 = CTRL_ALT | MOD_SHIFT;

const VK_ESCAPE: u32 = 0x1B;
const VK_F1: u32 = 0x70;

/// Virtual-key code of an uppercase letter or digit.
const fn key(c: char) -> u32 {
    c as u32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Record(Action),
    Undo,
    Clear,
    Play,
    PlayLoop,
    Stop,
    ToggleTrack,
    Save,
    Load,
    Help,
    Quit,
}

pub struct Binding {
    pub command: Command,
    pub modifiers: u32,
    pub vk: u32,
    pub keys: &'static str,
    pub description: &'static str,
}

// NOTE: Windows reports AltGr as Ctrl+Alt. On the Polish (Programmers) layout
// Ctrl+Alt+S/C/E/Z/O/A/L/N/X type diacritics (ś ć ę ż ó ą ł ń ź), so while
// these hotkeys are registered those characters cannot be typed elsewhere.
pub const BINDINGS: &[Binding] = &[
    Binding { command: Command::Help, modifiers: CTRL_ALT, vk: VK_F1, keys: "Ctrl+Alt+F1", description: "Help" },
    Binding { command: Command::Quit, modifiers: CTRL_ALT, vk: VK_ESCAPE, keys: "Ctrl+Alt+Esc", description: "Exit" },
    Binding { command: Command::Record(Action::Move), modifiers: CTRL_ALT, vk: key('S'), keys: "Ctrl+Alt+S", description: "Record 'move'" },
    Binding { command: Command::Record(Action::RightClick), modifiers: CTRL_ALT, vk: key('F'), keys: "Ctrl+Alt+F", description: "Record 'right button click'" },
    Binding { command: Command::Record(Action::Click), modifiers: CTRL_ALT, vk: key('C'), keys: "Ctrl+Alt+C", description: "Record 'left button click'" },
    Binding { command: Command::Record(Action::DoubleClick), modifiers: CTRL_ALT_SHIFT, vk: key('C'), keys: "Ctrl+Alt+Shift+C", description: "Record 'left button double-click'" },
    Binding { command: Command::Record(Action::Drag), modifiers: CTRL_ALT, vk: key('D'), keys: "Ctrl+Alt+D", description: "Record 'left button drag'" },
    Binding { command: Command::Undo, modifiers: CTRL_ALT, vk: key('Z'), keys: "Ctrl+Alt+Z", description: "Cancel last operation" },
    Binding { command: Command::Clear, modifiers: CTRL_ALT, vk: key('E'), keys: "Ctrl+Alt+E", description: "Clear all operations" },
    Binding { command: Command::Stop, modifiers: CTRL_ALT, vk: key('Q'), keys: "Ctrl+Alt+Q", description: "Stop mouse running" },
    Binding { command: Command::Play, modifiers: CTRL_ALT, vk: key('R'), keys: "Ctrl+Alt+R", description: "Run mouse pointer" },
    Binding { command: Command::PlayLoop, modifiers: CTRL_ALT_SHIFT, vk: key('R'), keys: "Ctrl+Alt+Shift+R", description: "Run mouse pointer in loop" },
    Binding { command: Command::ToggleTrack, modifiers: CTRL_ALT, vk: key('T'), keys: "Ctrl+Alt+T", description: "Switch track mode on/off" },
    Binding { command: Command::Save, modifiers: CTRL_ALT, vk: key('W'), keys: "Ctrl+Alt+W", description: "Save the way" },
    Binding { command: Command::Load, modifiers: CTRL_ALT, vk: key('O'), keys: "Ctrl+Alt+O", description: "Load the way" },
];

/// Extra stop key, registered only while playback runs, so it is not taken
/// from other programs the rest of the time.
pub const PLAYBACK_STOP: Binding = Binding {
    command: Command::Stop,
    modifiers: 0,
    vk: VK_ESCAPE,
    keys: "Esc",
    description: "Stop mouse running (only while running)",
};
pub const PLAYBACK_STOP_ID: i32 = BINDINGS.len() as i32;

pub fn command_for_id(id: i32) -> Option<Command> {
    if id == PLAYBACK_STOP_ID {
        return Some(PLAYBACK_STOP.command);
    }
    usize::try_from(id).ok().and_then(|i| BINDINGS.get(i)).map(|b| b.command)
}

/// Registers `binding` as a thread hotkey (no window) under `id`.
pub fn register(id: i32, binding: &Binding) -> Result<()> {
    let modifiers = HOT_KEY_MODIFIERS(binding.modifiers | MOD_NOREPEAT);
    unsafe { RegisterHotKey(None, id, modifiers, binding.vk) }
}

pub fn unregister(id: i32) {
    let _ = unsafe { UnregisterHotKey(None, id) };
}

pub fn help_text() -> String {
    let mut text = String::from("Shortcuts that you have to know:\n");
    for (i, binding) in BINDINGS.iter().chain([&PLAYBACK_STOP]).enumerate() {
        text.push_str(&format!("[{}] {} = {}.\n", i + 1, binding.keys, binding.description));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_combinations_are_unique() {
        let all: Vec<&Binding> = BINDINGS.iter().chain([&PLAYBACK_STOP]).collect();
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert!(
                    (a.modifiers, a.vk) != (b.modifiers, b.vk),
                    "{} and {} share a key combination",
                    a.keys,
                    b.keys
                );
            }
        }
    }

    #[test]
    fn ids_map_back_to_commands() {
        for (id, binding) in BINDINGS.iter().enumerate() {
            assert_eq!(command_for_id(id as i32), Some(binding.command));
        }
        assert_eq!(command_for_id(PLAYBACK_STOP_ID), Some(Command::Stop));
        assert_eq!(command_for_id(-1), None);
        assert_eq!(command_for_id(PLAYBACK_STOP_ID + 1), None);
    }
}
