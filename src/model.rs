//! Recorded sequence of mouse steps and the `.mt` save format.
//!
//! The `.mt` format is inherited from the AutoIt version: plain text, three
//! lines per step (`x`, `y`, action code), CRLF line endings.

use std::fmt::{self, Write as _};
use std::fs;
use std::io;
use std::path::Path;
use std::str::FromStr;

/// Mouse action performed at a step. Codes must stay stable: they are
/// written to `.mt` files and match the enum in the AutoIt version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Action {
    Move = 0,
    Click = 1,
    DoubleClick = 2,
    RightClick = 3,
    /// Left-button drag from the previous step's position to this one.
    Drag = 4,
}

impl Action {
    pub fn code(self) -> u8 {
        self as u8
    }

    pub fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Move),
            1 => Some(Self::Click),
            2 => Some(Self::DoubleClick),
            3 => Some(Self::RightClick),
            4 => Some(Self::Drag),
            _ => None,
        }
    }
}

/// One recorded step. Coordinates are virtual-desktop pixels and may be
/// negative on multi-monitor setups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    pub x: i32,
    pub y: i32,
    pub action: Action,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Sequence {
    steps: Vec<Step>,
}

impl Sequence {
    pub fn push(&mut self, step: Step) {
        self.steps.push(step);
    }

    pub fn pop(&mut self) -> Option<Step> {
        self.steps.pop()
    }

    pub fn clear(&mut self) {
        self.steps.clear();
    }

    pub fn steps(&self) -> &[Step] {
        &self.steps
    }
}

#[derive(Debug)]
pub enum MtError {
    Io(io::Error),
    /// `line` is 1-based.
    Parse { line: usize, message: String },
}

impl fmt::Display for MtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Parse { line, message } => write!(f, "line {line}: {message}"),
        }
    }
}

impl std::error::Error for MtError {}

impl From<io::Error> for MtError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

pub fn to_mt_string(sequence: &Sequence) -> String {
    let mut out = String::new();
    for step in sequence.steps() {
        let _ = write!(out, "{}\r\n{}\r\n{}\r\n", step.x, step.y, step.action.code());
    }
    out
}

pub fn parse_mt(text: &str) -> Result<Sequence, MtError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines: Vec<(usize, &str)> = text
        .lines()
        .map(str::trim)
        .enumerate()
        .map(|(i, line)| (i + 1, line))
        .collect();
    while lines.last().is_some_and(|(_, line)| line.is_empty()) {
        lines.pop();
    }
    if !lines.len().is_multiple_of(3) {
        return Err(MtError::Parse {
            line: lines.len(),
            message: format!("expected a multiple of 3 lines, found {}", lines.len()),
        });
    }

    let mut sequence = Sequence::default();
    for chunk in lines.chunks(3) {
        let x = parse_field(chunk[0])?;
        let y = parse_field(chunk[1])?;
        let code: u8 = parse_field(chunk[2])?;
        let action = Action::from_code(code).ok_or_else(|| MtError::Parse {
            line: chunk[2].0,
            message: format!("unknown action code {code}"),
        })?;
        sequence.push(Step { x, y, action });
    }
    Ok(sequence)
}

fn parse_field<T: FromStr>((line, text): (usize, &str)) -> Result<T, MtError> {
    text.parse().map_err(|_| MtError::Parse {
        line,
        message: format!("expected a number, found {text:?}"),
    })
}

pub fn save(path: &Path, sequence: &Sequence) -> Result<(), MtError> {
    fs::write(path, to_mt_string(sequence))?;
    Ok(())
}

pub fn load(path: &Path) -> Result<Sequence, MtError> {
    parse_mt(&fs::read_to_string(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(x: i32, y: i32, action: Action) -> Step {
        Step { x, y, action }
    }

    #[test]
    fn action_codes_round_trip() {
        for code in 0..=4 {
            assert_eq!(Action::from_code(code).unwrap().code(), code);
        }
        assert_eq!(Action::from_code(5), None);
    }

    #[test]
    fn reads_autoit_save() {
        let text = "100\r\n200\r\n0\r\n-1920\r\n35\r\n4\r\n";
        let sequence = parse_mt(text).unwrap();
        assert_eq!(
            sequence.steps(),
            &[step(100, 200, Action::Move), step(-1920, 35, Action::Drag)]
        );
    }

    #[test]
    fn round_trips() {
        let mut sequence = Sequence::default();
        sequence.push(step(1, 2, Action::Click));
        sequence.push(step(3, 4, Action::DoubleClick));
        sequence.push(step(5, 6, Action::RightClick));
        assert_eq!(parse_mt(&to_mt_string(&sequence)).unwrap(), sequence);
    }

    #[test]
    fn writes_crlf_three_lines_per_step() {
        let mut sequence = Sequence::default();
        sequence.push(step(10, 20, Action::RightClick));
        assert_eq!(to_mt_string(&sequence), "10\r\n20\r\n3\r\n");
    }

    #[test]
    fn tolerates_trailing_blank_lines_bom_and_lf() {
        let sequence = parse_mt("\u{feff}1\n2\n1\n\n\r\n").unwrap();
        assert_eq!(sequence.steps(), &[step(1, 2, Action::Click)]);
    }

    #[test]
    fn empty_file_is_empty_sequence() {
        assert_eq!(parse_mt("").unwrap(), Sequence::default());
    }

    #[test]
    fn rejects_incomplete_step() {
        assert!(matches!(parse_mt("1\r\n2\r\n"), Err(MtError::Parse { .. })));
    }

    #[test]
    fn rejects_unknown_action() {
        let err = parse_mt("1\r\n2\r\n9\r\n").unwrap_err();
        assert!(matches!(err, MtError::Parse { line: 3, .. }));
    }

    #[test]
    fn rejects_non_numeric_coordinate() {
        let err = parse_mt("1\r\nabc\r\n0\r\n").unwrap_err();
        assert!(matches!(err, MtError::Parse { line: 2, .. }));
    }
}
