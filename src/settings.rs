//! Tiny settings file (just the colour theme for now): %LOCALAPPDATA%\AndroidEmulatorManager\settings.txt
use crate::paths;
use crate::theme::Mode;
use std::fs;

pub fn parse(text: &str) -> Mode {
    for line in text.lines() {
        if let Some(v) = line.trim().strip_prefix("theme=") {
            return match v.trim() {
                "light" => Mode::Light,
                "dark" => Mode::Dark,
                _ => Mode::Auto,
            };
        }
    }
    Mode::Auto
}

pub fn serialize(mode: Mode) -> String {
    let v = match mode {
        Mode::Light => "light",
        Mode::Dark => "dark",
        Mode::Auto => "auto",
    };
    format!("theme={v}\n")
}

fn file() -> std::path::PathBuf {
    paths::home().join("settings.txt")
}

pub fn load_mode() -> Mode {
    fs::read_to_string(file()).map(|t| parse(&t)).unwrap_or(Mode::Auto)
}

pub fn save_mode(mode: Mode) {
    let _ = fs::create_dir_all(paths::home());
    let _ = fs::write(file(), serialize(mode));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_setting_round_trip() {
        for m in [Mode::Light, Mode::Dark, Mode::Auto] {
            assert!(parse(&serialize(m)) == m);
        }
        assert!(parse("garbage") == Mode::Auto);
    }
}
