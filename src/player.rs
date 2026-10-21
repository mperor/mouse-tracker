//! Replays a recorded sequence on a worker thread so the hotkey loop stays
//! responsive. Stopping is checked during cursor movement, not only between
//! steps.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::input::{self, Button};
use crate::model::{Action, Step};

/// Smooth movement: one cursor update per `MOVE_STEP_PX` of distance.
const MOVE_STEP_PX: f64 = 12.0;
const MOVE_STEP_DELAY: Duration = Duration::from_millis(5);
/// Pause after pressing and before releasing the button during a drag
/// (AutoIt's MouseClickDragDelay); without it many drag targets ignore the drop.
const DRAG_PAUSE: Duration = Duration::from_millis(250);

#[derive(Default)]
pub struct Player {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Player {
    pub fn is_running(&self) -> bool {
        self.worker.as_ref().is_some_and(|w| !w.is_finished())
    }

    /// Starts playback unless it is already running or there is nothing to play.
    pub fn start(&mut self, steps: Vec<Step>, looped: bool) {
        if self.is_running() || steps.is_empty() {
            return;
        }
        self.stop.store(false, Ordering::Relaxed);
        let stop = Arc::clone(&self.stop);
        self.worker = Some(thread::spawn(move || play(&steps, looped, &stop)));
    }

    /// Stops playback and waits for the worker to release any held button.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop();
    }
}

fn play(steps: &[Step], looped: bool, stop: &AtomicBool) {
    loop {
        let mut prev = None;
        for step in steps {
            if !perform(step, prev, stop) {
                return;
            }
            prev = Some((step.x, step.y));
        }
        if !looped {
            return;
        }
    }
}

/// Returns `false` when playback was stopped.
fn perform(step: &Step, prev: Option<(i32, i32)>, stop: &AtomicBool) -> bool {
    let to = (step.x, step.y);
    match step.action {
        Action::Move => smooth_move(to, stop),
        Action::Click => click_at(to, Button::Left, 1, stop),
        Action::DoubleClick => click_at(to, Button::Left, 2, stop),
        Action::RightClick => click_at(to, Button::Right, 1, stop),
        Action::Drag => {
            // A drag recorded as the first step starts where the cursor is.
            if let Some(from) = prev {
                if !smooth_move(from, stop) {
                    return false;
                }
            }
            input::button_down(Button::Left);
            thread::sleep(DRAG_PAUSE);
            let completed = smooth_move(to, stop);
            if completed {
                thread::sleep(DRAG_PAUSE);
            }
            // Always release, even when stopped mid-drag.
            input::button_up(Button::Left);
            completed
        }
    }
}

fn click_at(to: (i32, i32), button: Button, count: u32, stop: &AtomicBool) -> bool {
    if !smooth_move(to, stop) {
        return false;
    }
    input::click(button, count);
    true
}

fn smooth_move(to: (i32, i32), stop: &AtomicBool) -> bool {
    let (from_x, from_y) = input::cursor_pos().unwrap_or(to);
    let dx = f64::from(to.0 - from_x);
    let dy = f64::from(to.1 - from_y);
    let count = ((dx.hypot(dy) / MOVE_STEP_PX).ceil() as u32).max(1);
    for i in 1..=count {
        if stop.load(Ordering::Relaxed) {
            return false;
        }
        let t = f64::from(i) / f64::from(count);
        input::set_cursor_pos(
            from_x + (dx * t).round() as i32,
            from_y + (dy * t).round() as i32,
        );
        thread::sleep(MOVE_STEP_DELAY);
    }
    !stop.load(Ordering::Relaxed)
}
