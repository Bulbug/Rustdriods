//! Turns "tap here" requests into touches on the emulator.
//! This version uses `input tap` through ONE long-running `adb shell` session
//! (no new adb process per tap). Each tap is started in the background (`&`), so quick
//! presses overlap instead of queueing. The method is isolated here so it can be swapped
//! for a faster one later without touching the rest of the app.
use crate::paths;
use crate::runner::no_window;
use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Sender};
use std::thread;

pub struct Injector {
    /// Send a position (fractions of the screen, 0.0 - 1.0) to tap it.
    pub tx: Sender<(f32, f32)>,
}

/// `size` = the screen size in pixels, in the orientation the profile was made for.
pub fn start(size: (u32, u32)) -> Injector {
    let (tx, rx) = mpsc::channel::<(f32, f32)>();
    thread::spawn(move || {
        let mut shell = spawn_shell();
        // Ends when every sender is gone (mapping turned off).
        for (fx, fy) in rx {
            let (x, y) = to_pixels(fx, fy, size);
            let line = format!("input tap {x} {y} &\n");
            if !write_line(&mut shell, &line) {
                shell = spawn_shell(); // the shell died: start a new one and retry once
                let _ = write_line(&mut shell, &line);
            }
        }
        if let Some(mut c) = shell.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    });
    Injector { tx }
}

pub fn to_pixels(fx: f32, fy: f32, size: (u32, u32)) -> (u32, u32) {
    let x = (fx.clamp(0.0, 1.0) * size.0 as f32).round() as u32;
    let y = (fy.clamp(0.0, 1.0) * size.1 as f32).round() as u32;
    (x.min(size.0.saturating_sub(1)), y.min(size.1.saturating_sub(1)))
}

fn spawn_shell() -> Option<Child> {
    let mut cmd = Command::new(paths::adb());
    cmd.args(["-e", "shell"]).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null());
    no_window(&mut cmd);
    cmd.spawn().ok()
}

fn write_line(shell: &mut Option<Child>, line: &str) -> bool {
    match shell.as_mut().and_then(|c| c.stdin.as_mut()) {
        Some(stdin) => stdin.write_all(line.as_bytes()).and_then(|_| stdin.flush()).is_ok(),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::to_pixels;

    #[test]
    fn converts_fractions_to_pixels() {
        assert_eq!(to_pixels(0.5, 0.5, (1080, 1920)), (540, 960));
        assert_eq!(to_pixels(1.0, 1.0, (1080, 1920)), (1079, 1919)); // stays on screen
        assert_eq!(to_pixels(-1.0, 2.0, (100, 200)), (0, 199));
    }
}
