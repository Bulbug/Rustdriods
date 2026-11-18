//! If the app crashes, write a report to crash.txt. The next start shows it in the log
//! (so "Copy log" sends it along). Without this, a crash would just make the window vanish.
use crate::{paths, update};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn format_report(version: &str, message: &str, location: &str, thread: &str, backtrace: &str, unix_secs: u64) -> String {
    format!(
        "Android Emulator Manager crashed\nVersion: {version}\nTime (unix seconds): {unix_secs}\nThread: {thread}\nWhere: {location}\nMessage: {message}\n\nDetails:\n{backtrace}\n"
    )
}

pub fn install_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "(no message)".to_string());
        let location = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
        let thread = std::thread::current().name().unwrap_or("unnamed").to_string();
        let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let backtrace = std::backtrace::Backtrace::force_capture().to_string();
        let report = format_report(&update::version_label(), &message, &location, &thread, &backtrace, secs);
        let _ = fs::create_dir_all(paths::home());
        let _ = fs::write(paths::home().join("crash.txt"), report);
        previous(info); // keep the normal behaviour (prints in debug builds)
    }));
}

/// A report left by the last run, if any. It is renamed so it is shown only once.
pub fn take_previous() -> Option<String> {
    take_from(&paths::home().join("crash.txt"))
}

fn take_from(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    let _ = fs::rename(path, path.with_file_name("crash-previous.txt"));
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_contains_the_important_parts() {
        let r = format_report("v1.2.3 beta", "boom", "src/app.rs:10", "main", "frame 0", 42);
        for part in ["v1.2.3 beta", "boom", "src/app.rs:10", "main", "frame 0"] {
            assert!(r.contains(part));
        }
    }

    #[test]
    fn a_report_is_shown_only_once() {
        let dir = std::env::temp_dir().join("aem-test-crash");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("crash.txt");
        assert!(take_from(&file).is_none());
        fs::write(&file, "report").unwrap();
        assert_eq!(take_from(&file).as_deref(), Some("report"));
        assert!(take_from(&file).is_none());
        assert!(dir.join("crash-previous.txt").exists());
    }
}
