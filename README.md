# mouse-tracker

Records mouse positions and actions with global hotkeys and replays them —
once or in a loop. Windows only.

Point the cursor somewhere, press a hotkey to record a step (move, click,
double-click, right-click or drag), repeat, then run the whole sequence.
Recorded steps are shown on screen as numbered labels, and sequences can be
saved to and loaded from `.mt` files.

### How it works?
#### - Move and click
![Mouse move](img/move.gif "Move")
#### - Double click
![Mouse open file](img/doubleclick.gif "Double click")
#### - Drag and drop
![Mouse drag directory](img/draganddrop.gif "Drag and drop")

## Download

Prebuilt `mouse-tracker.exe` files are attached to the
[releases](https://github.com/mperor/mouse-tracker/releases). It is a single
file with no installer; run it and look for the icon in the notification
area (system tray). Only one instance runs at a time.

## Usage

### Hotkeys

| Hotkey | Action |
|---|---|
| `Ctrl+Alt+S` | Record a **move** to the cursor position |
| `Ctrl+Alt+C` | Record a **left click** |
| `Ctrl+Alt+Shift+C` | Record a **double-click** |
| `Ctrl+Alt+F` | Record a **right click** |
| `Ctrl+Alt+D` | Record a **drag** from the previous step to the cursor position |
| `Ctrl+Alt+Z` | Cancel the last step |
| `Ctrl+Alt+E` | Clear all steps |
| `Ctrl+Alt+R` | Run the sequence once |
| `Ctrl+Alt+Shift+R` | Run the sequence in a loop |
| `Ctrl+Alt+Q` or `Esc` | Stop running (`Esc` is only taken while running) |
| `Ctrl+Alt+T` | Show / hide the recorded steps on screen |
| `Ctrl+Alt+W` | Save the sequence |
| `Ctrl+Alt+O` | Load a sequence |
| `Ctrl+Alt+F1` | Help |
| `Ctrl+Alt+Esc` | Exit |

> **Note for Polish keyboard layouts:** Windows treats `AltGr` as `Ctrl+Alt`,
> so while the app runs, `AltGr+S/C/E/Z/O` (ś, ć, ę, ż, ó) trigger the
> hotkeys instead of typing the letters. All bindings live in one table in
> [`src/hotkeys.rs`](src/hotkeys.rs) if you want to remap them.

### Tray menu

Clicking the tray icon opens a menu with the same commands (except
recording, which needs the cursor position) and the number of recorded
steps. While a sequence is running the cursor keeps moving, so stop it with
`Esc`, or open the menu and press `S`.

### On-screen labels

Each recorded step is labelled with its number. The colour shows the action:

| Colour | Action |
|---|---|
| Blue | Move |
| Orange | Left click |
| Red | Double-click |
| Green | Right click |
| Purple | Drag |

A label next to the cursor shows its current coordinates.

## Save file format (`.mt`)

Plain text, three lines per step: `x`, `y` and the action code, with CRLF
line endings. Coordinates are virtual-desktop pixels and can be negative on
multi-monitor setups.

| Code | Action |
|---|---|
| `0` | Move |
| `1` | Left click |
| `2` | Double-click |
| `3` | Right click |
| `4` | Drag (from the previous step) |

```text
100
200
0
640
480
1
```

The format is the same as in the original AutoIt version, so old saves load
unchanged.

## Building

Requires the Rust toolchain. Either target works:

- **GNU** — no Visual Studio needed, e.g. with [Scoop](https://scoop.sh):
  `scoop install rustup-gnu`
- **MSVC** — the default from [rustup.rs](https://rustup.rs); needs the
  Visual Studio Build Tools (C++ workload)

```sh
cargo build --release      # target/release/mouse-tracker.exe
cargo test
cargo run                  # debug build, keeps a console window for logs
```

### Project layout

| File | Contents |
|---|---|
| `src/main.rs` | Startup, single-instance check, hotkey registration, message loop |
| `src/app.rs` | Application state; handles hotkey and tray commands |
| `src/hotkeys.rs` | The hotkey table |
| `src/model.rs` | Steps, sequences and the `.mt` format |
| `src/player.rs` | Playback on a worker thread |
| `src/input.rs` | Cursor and mouse-button input (Win32) |
| `src/overlay.rs` | On-screen step labels and the cursor-coordinates label |
| `src/tray.rs` | Tray icon and its menu |

## Original AutoIt version

The app started as an AutoIt script, kept in
[`mouse-tracker.au3`](mouse-tracker.au3) for reference. The Rust rewrite
keeps its behaviour and save format, and changes the following:

- Hotkeys use `Ctrl+Alt`, so they no longer take over `Ctrl+C`, `Ctrl+S`,
  `Ctrl+Z` and others from every application.
- Stopping takes effect immediately, also in the middle of a movement, and a
  drag interrupted halfway always releases the mouse button.
- A drag recorded as the first step starts from the current cursor position
  instead of the screen corner.
- On-screen labels survive repaints and stay on top; all monitors are
  covered and display scaling is handled.
- Invalid save files are reported with the offending line instead of
  crashing.
- A tray icon shows that the app is running.
