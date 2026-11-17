//! Talking to the running emulator through adb. Every command uses `-e`
//! (= "the one running emulator") so it never touches a real phone plugged into the PC.
use crate::runner::{self, no_window, Log};
use crate::{paths, Res};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn adb(args: &[&str]) -> Command {
    let mut c = Command::new(paths::adb());
    c.args(args);
    no_window(&mut c);
    c
}

/// Is an emulator running and fully booted enough for adb?
pub fn is_connected() -> bool {
    match adb(&["devices"]).output() {
        Ok(o) => String::from_utf8_lossy(&o.stdout)
            .lines()
            .any(|l| l.starts_with("emulator-") && l.trim_end().ends_with("device")),
        Err(_) => false,
    }
}

pub fn require_connected() -> Res<()> {
    if is_connected() {
        Ok(())
    } else {
        Err("No running emulator found. Start a device on the Devices tab and wait until it finishes booting.".into())
    }
}

/// Turns "package:com.foo\npackage:com.bar" into a sorted list of names.
pub fn parse_packages(text: &str) -> Vec<String> {
    let mut v: Vec<String> = text
        .lines()
        .filter_map(|l| l.trim().strip_prefix("package:"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    v.sort();
    v
}

pub fn list_packages(include_system: bool) -> Res<Vec<String>> {
    let mut args = vec!["-e", "shell", "pm", "list", "packages"];
    if !include_system {
        args.push("-3"); // third-party apps only
    }
    let out = adb(&args).output()?;
    if !out.status.success() {
        return Err(format!("adb failed: {}", String::from_utf8_lossy(&out.stderr).trim()).into());
    }
    Ok(parse_packages(&String::from_utf8_lossy(&out.stdout)))
}

/// Apps that are currently turned off.
pub fn list_disabled() -> Res<Vec<String>> {
    let out = adb(&["-e", "shell", "pm", "list", "packages", "-d"]).output()?;
    if !out.status.success() {
        return Err(format!("adb failed: {}", String::from_utf8_lossy(&out.stderr).trim()).into());
    }
    Ok(parse_packages(&String::from_utf8_lossy(&out.stdout)))
}

/// (connected?, installed apps). Safe to call any time.
pub fn snapshot(include_system: bool) -> (bool, Vec<String>) {
    if !is_connected() {
        return (false, Vec::new());
    }
    (true, list_packages(include_system).unwrap_or_default())
}

pub fn install(apk: &Path, log: &Log) -> Res<()> {
    log(format!("Installing {} ...", apk.display()));

    // Watch the output for the most common failure so we can explain it.
    let no_abi = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&no_abi);
    let inner = Arc::clone(log);
    let watching: Log = Arc::new(move |s: String| {
        if s.contains("NO_MATCHING_ABIS") {
            flag.store(true, Ordering::SeqCst);
        }
        inner(s);
    });

    let args: Vec<String> = vec![
        "-e".into(),
        "install".into(),
        "-r".into(), // replace if already installed
        apk.to_string_lossy().to_string(),
    ];
    let code = runner::run(paths::adb(), &args, &[], None, &watching)?;
    if no_abi.load(Ordering::SeqCst) {
        log("Hint: this app has no build for the emulator's CPU (x86_64). Try an Android 11+ image, or a different APK.".to_string());
    }
    if code != 0 {
        return Err("Install failed (see the messages above).".into());
    }
    log("Installed.".to_string());
    Ok(())
}

pub fn uninstall(package: &str, log: &Log) -> Res<()> {
    log(format!("Uninstalling {package} ..."));
    let args: Vec<String> = ["-e", "uninstall", package].iter().map(|s| s.to_string()).collect();
    let code = runner::run(paths::adb(), &args, &[], None, log)?;
    if code != 0 {
        return Err(format!("Could not uninstall {package}.").into());
    }
    log("Uninstalled.".to_string());
    Ok(())
}

/// Saves a PNG of the emulator screen and returns where it went.
pub fn screenshot() -> Res<PathBuf> {
    let out = adb(&["-e", "exec-out", "screencap", "-p"]).output()?;
    if !out.status.success() || !out.stdout.starts_with(b"\x89PNG") {
        return Err(format!(
            "Screenshot failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )
        .into());
    }
    let dir = paths::screenshots_dir();
    fs::create_dir_all(&dir)?;
    let secs = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let path = dir.join(format!("screenshot-{secs}.png"));
    fs::write(&path, &out.stdout)?;
    Ok(path)
}

/// Removes spaces and the quotes Windows adds with "Copy as path".
pub fn clean_path(s: &str) -> String {
    s.trim().trim_matches('"').trim().to_string()
}

/// Runs one command inside the emulator (`adb shell ...`) and prints its output to the log.
/// Commands that never end (like `logcat` without -d) are stopped after 30 seconds.
pub fn shell(command: &str, log: &Log) -> Res<()> {
    let finished = Arc::new(AtomicBool::new(false));
    let pid = Arc::new(AtomicU32::new(0));

    // Watchdog: gives up on a command that is still running after 30 s.
    {
        let (finished, pid, log) = (Arc::clone(&finished), Arc::clone(&pid), Arc::clone(log));
        thread::spawn(move || {
            for _ in 0..60 {
                thread::sleep(Duration::from_millis(500));
                if finished.load(Ordering::SeqCst) {
                    return;
                }
            }
            log("Command stopped after 30 s. (Tip: use `logcat -d -t 100` to print the log and exit.)".to_string());
            crate::launcher::kill_tree(pid.load(Ordering::SeqCst));
        });
    }

    let args: Vec<String> = vec!["-e".into(), "shell".into(), command.into()];
    let pid2 = Arc::clone(&pid);
    let result = runner::run_notify(paths::adb(), &args, &[], None, log, &move |p| {
        pid2.store(p, Ordering::SeqCst)
    });
    finished.store(true, Ordering::SeqCst);

    let code = result?;
    if code != 0 {
        log(format!("(command finished with exit code {code})"));
    }
    Ok(())
}

/// Copies a file or folder from the PC into the emulator.
pub fn push(src: &Path, dest: &str, log: &Log) -> Res<()> {
    if !src.exists() {
        return Err(format!("File not found: {}", src.display()).into());
    }
    log(format!("Copying {} to the emulator ({dest}) ...", src.display()));
    let args: Vec<String> = vec![
        "-e".into(),
        "push".into(),
        src.to_string_lossy().to_string(),
        dest.to_string(),
    ];
    let code = runner::run(paths::adb(), &args, &[], None, log)?;
    if code != 0 {
        return Err("Copy failed (see the messages above).".into());
    }
    log("Copied to the emulator.".to_string());
    Ok(())
}

/// Copies a file or folder from the emulator into `dest_dir` on the PC.
pub fn pull(src: &str, dest_dir: &Path, log: &Log) -> Res<()> {
    fs::create_dir_all(dest_dir)?;
    log(format!("Copying {src} from the emulator ..."));
    let args: Vec<String> = vec![
        "-e".into(),
        "pull".into(),
        src.to_string(),
        dest_dir.to_string_lossy().to_string(),
    ];
    let code = runner::run(paths::adb(), &args, &[], None, log)?;
    if code != 0 {
        return Err("Copy failed (see the messages above).".into());
    }
    log(format!("Saved in {}", dest_dir.display()));
    Ok(())
}

/// A screenshot that is already shrunk for display. `w` x `h` is the real screen size.
pub struct Screen {
    pub w: u32,
    pub h: u32,
    pub sw: usize,
    pub sh: usize,
    pub rgba: Vec<u8>,
}

/// Reads the raw output of `screencap` (a small header, then RGBA pixels) and shrinks it so that
/// its longest side is at most `max_dim` pixels. No image library needed.
pub fn parse_raw_capture(data: &[u8], max_dim: u32) -> Res<Screen> {
    if data.len() < 12 {
        return Err("The screenshot data was too short.".into());
    }
    let rd = |i: usize| u32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
    let (w, h, format) = (rd(0), rd(4), rd(8));
    if w == 0 || h == 0 || format != 1 {
        return Err(format!("Unsupported screenshot format ({w}x{h}, type {format}).").into());
    }
    let pixels = w as usize * h as usize * 4;
    // Newer Android versions add one more number to the header.
    let header = if data.len() == 12 + pixels {
        12
    } else if data.len() == 16 + pixels {
        16
    } else {
        return Err("The screenshot data had an unexpected size.".into());
    };

    let step = ((w.max(h) + max_dim - 1) / max_dim).max(1) as usize;
    let (sw, sh) = ((w as usize + step - 1) / step, (h as usize + step - 1) / step);
    let mut rgba = Vec::with_capacity(sw * sh * 4);
    for y in 0..sh {
        for x in 0..sw {
            let i = header + ((y * step) * w as usize + x * step) * 4;
            rgba.extend_from_slice(&[data[i], data[i + 1], data[i + 2], 255]);
        }
    }
    Ok(Screen { w, h, sw, sh, rgba })
}

pub fn capture_small(max_dim: u32) -> Res<Screen> {
    let out = adb(&["-e", "exec-out", "screencap"]).output()?;
    if !out.status.success() {
        return Err(format!("Screenshot failed: {}", String::from_utf8_lossy(&out.stderr).trim()).into());
    }
    parse_raw_capture(&out.stdout, max_dim)
}

fn parse_wh(s: &str) -> Option<(u32, u32)> {
    let (w, h) = s.trim().split_once('x')?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
}

/// Reads the output of `wm size`. An "Override size" wins over the "Physical size".
pub fn parse_screen_size(text: &str) -> Option<(u32, u32)> {
    let (mut physical, mut over) = (None, None);
    for line in text.lines() {
        let line = line.trim();
        if let Some(r) = line.strip_prefix("Physical size:") {
            physical = parse_wh(r);
        } else if let Some(r) = line.strip_prefix("Override size:") {
            over = parse_wh(r);
        }
    }
    over.or(physical)
}

pub fn screen_size() -> Res<(u32, u32)> {
    let out = adb(&["-e", "shell", "wm", "size"]).output()?;
    parse_screen_size(&String::from_utf8_lossy(&out.stdout))
        .ok_or_else(|| "Could not read the emulator's screen size.".into())
}

/// One tap at pixel (x, y).
pub fn tap(x: u32, y: u32) -> Res<()> {
    let (xs, ys) = (x.to_string(), y.to_string());
    let out = adb(&["-e", "shell", "input", "tap", &xs, &ys]).output()?;
    if !out.status.success() {
        return Err(format!("Tap failed: {}", String::from_utf8_lossy(&out.stderr).trim()).into());
    }
    Ok(())
}

/// Opens a folder in Windows Explorer.
pub fn open_folder(dir: &Path) {
    let _ = fs::create_dir_all(dir);
    let _ = Command::new("explorer").arg(dir).spawn();
}

#[cfg(test)]
mod tests {
    use super::{clean_path, parse_packages};

    #[test]
    fn cleans_pasted_paths() {
        assert_eq!(clean_path("  \"C:\\a b\\x.apk\" "), "C:\\a b\\x.apk");
        assert_eq!(clean_path("/sdcard/Download/"), "/sdcard/Download/");
    }

    #[test]
    fn parses_pm_output() {
        let text = "package:com.zeta.app\r\npackage:com.alpha.game\r\n\r\nWarning: something\r\n";
        assert_eq!(parse_packages(text), ["com.alpha.game", "com.zeta.app"]);
    }
}

#[cfg(test)]
mod capture_tests {
    use super::{parse_raw_capture, parse_screen_size};

    fn fake(header_len: usize) -> Vec<u8> {
        // 2x2 image: header (w, h, format=1 [, colorspace]) then 4 RGBA pixels
        let mut d = Vec::new();
        for v in [2u32, 2, 1] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        if header_len == 16 {
            d.extend_from_slice(&0u32.to_le_bytes());
        }
        for p in 0..4u8 {
            d.extend_from_slice(&[p, p + 10, p + 20, 0]);
        }
        d
    }

    #[test]
    fn reads_both_header_sizes() {
        for hl in [12, 16] {
            let s = parse_raw_capture(&fake(hl), 960).unwrap();
            assert_eq!((s.w, s.h, s.sw, s.sh), (2, 2, 2, 2));
            assert_eq!(&s.rgba[4..8], &[1, 11, 21, 255]);
        }
    }

    #[test]
    fn shrinks_big_screens_and_rejects_garbage() {
        let s = parse_raw_capture(&fake(12), 1).unwrap();
        assert_eq!((s.sw, s.sh), (1, 1));
        assert!(parse_raw_capture(&[1, 2, 3], 960).is_err());
    }

    #[test]
    fn override_size_wins() {
        assert_eq!(parse_screen_size("Physical size: 1080x1920\n"), Some((1080, 1920)));
        assert_eq!(
            parse_screen_size("Physical size: 1080x1920\nOverride size: 720x1280\n"),
            Some((720, 1280))
        );
        assert_eq!(parse_screen_size("nothing"), None);
    }
}
