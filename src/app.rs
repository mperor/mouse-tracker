//! Application state and the handling of hotkey commands.

use std::ops::ControlFlow;
use std::path::PathBuf;

use rfd::{FileDialog, MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};

use crate::hotkeys::{self, Command};
use crate::input;
use crate::model::{self, Action, Sequence, Step};
use crate::overlay::Overlay;
use crate::player::Player;

pub struct App {
    sequence: Sequence,
    player: Player,
    overlay: Overlay,
    track_on: bool,
    stop_key_registered: bool,
}

impl App {
    pub fn new() -> windows::core::Result<Self> {
        Ok(Self {
            sequence: Sequence::default(),
            player: Player::default(),
            overlay: Overlay::new()?,
            track_on: true,
            stop_key_registered: false,
        })
    }

    /// Periodic update, driven by the main loop's timer.
    pub fn tick(&mut self) {
        self.overlay.update_cursor();
        self.sync_stop_key();
    }

    /// Holds the playback stop key (Esc) exactly while playback runs.
    fn sync_stop_key(&mut self) {
        let running = self.player.is_running();
        if running == self.stop_key_registered {
            return;
        }
        if running {
            // If another program holds the key, Ctrl+Alt+Q still stops
            // playback; don't retry on every tick.
            if let Err(e) = hotkeys::register(hotkeys::PLAYBACK_STOP_ID, &hotkeys::PLAYBACK_STOP) {
                eprintln!("[hotkeys] cannot register {}: {e}", hotkeys::PLAYBACK_STOP.keys);
            }
        } else {
            hotkeys::unregister(hotkeys::PLAYBACK_STOP_ID);
        }
        self.stop_key_registered = running;
    }

    pub fn handle(&mut self, command: Command) -> ControlFlow<()> {
        match command {
            Command::Record(action) => self.record(action),
            Command::Undo => {
                self.player.stop();
                if self.sequence.pop().is_some() {
                    self.redraw();
                }
            }
            Command::Clear => {
                self.player.stop();
                self.sequence.clear();
                self.redraw();
            }
            Command::Play => self.player.start(self.sequence.steps().to_vec(), false),
            Command::PlayLoop => self.player.start(self.sequence.steps().to_vec(), true),
            Command::Stop => self.player.stop(),
            Command::ToggleTrack => {
                self.track_on = !self.track_on;
                self.redraw();
            }
            Command::Save => self.save(),
            Command::Load => self.load(),
            Command::Help => show_info("Help...", &hotkeys::help_text()),
            Command::Quit => {
                if confirm("Exit...", "Are you sure you want to exit the application?") {
                    self.player.stop();
                    return ControlFlow::Break(());
                }
            }
        }
        self.sync_stop_key();
        ControlFlow::Continue(())
    }

    fn record(&mut self, action: Action) {
        if let Some((x, y)) = input::cursor_pos() {
            self.sequence.push(Step { x, y, action });
            self.redraw();
        }
    }

    fn redraw(&mut self) {
        if self.track_on {
            self.overlay.show(self.sequence.steps());
        } else {
            self.overlay.hide();
        }
    }

    fn save(&mut self) {
        let Some(path) = mt_dialog().set_file_name("save.mt").save_file() else {
            return;
        };
        if let Err(e) = model::save(&path, &self.sequence) {
            show_error("Save failed", &format!("{}: {e}", path.display()));
        }
    }

    fn load(&mut self) {
        let Some(path) = mt_dialog().pick_file() else {
            return;
        };
        match model::load(&path) {
            Ok(sequence) => {
                self.player.stop();
                self.sequence = sequence;
                self.track_on = true;
                self.redraw();
            }
            Err(e) => show_error("Load failed", &format!("{}: {e}", path.display())),
        }
    }
}

fn mt_dialog() -> FileDialog {
    let dialog = FileDialog::new().add_filter("Mouse Tracker saves", &["mt"]);
    match exe_dir() {
        Some(dir) => dialog.set_directory(dir),
        None => dialog,
    }
}

fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(PathBuf::from)
}

pub fn show_info(title: &str, text: &str) {
    message(title, text, MessageLevel::Info);
}

pub fn show_error(title: &str, text: &str) {
    message(title, text, MessageLevel::Error);
}

fn message(title: &str, text: &str, level: MessageLevel) {
    MessageDialog::new()
        .set_title(title)
        .set_description(text)
        .set_level(level)
        .set_buttons(MessageButtons::Ok)
        .show();
}

fn confirm(title: &str, text: &str) -> bool {
    MessageDialog::new()
        .set_title(title)
        .set_description(text)
        .set_level(MessageLevel::Info)
        .set_buttons(MessageButtons::YesNo)
        .show()
        == MessageDialogResult::Yes
}
