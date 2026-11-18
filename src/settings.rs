//! Tiny settings file: %LOCALAPPDATA%\AndroidEmulatorManager\settings.txt
use crate::paths;
use crate::theme::Mode;
use std::fs;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Settings {
    pub mode: Mode,
    pub updates: bool, // look for a newer version when the app starts
}

impl Default for Settings {
    fn default() -> Self {
        Settings { mode: Mode::Auto, updates: true }
    }
}

pub fn parse(text: &str) -> Settings {
    let mut s = Settings::default();
    for line in text.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("theme=") {
            s.mode = match v.trim() {
                "light" => Mode::Light,
                "dark" => Mode::Dark,
                _ => Mode::Auto,
            };
        } else if let Some(v) = line.strip_prefix("updates=") {
            s.updates = v.trim() != "off";
        }
    }
    s
}

pub fn serialize(s: &Settings) -> String {
    let theme = match s.mode {
        Mode::Light => "light",
        Mode::Dark => "dark",
        Mode::Auto => "auto",
    };
    format!("theme={theme}\nupdates={}\n", if s.updates { "on" } else { "off" })
}

fn file() -> std::path::PathBuf {
    paths::home().join("settings.txt")
}

pub fn load() -> Settings {
    fs::read_to_string(file()).map(|t| parse(&t)).unwrap_or_default()
}

pub fn save(s: &Settings) {
    let _ = fs::create_dir_all(paths::home());
    let _ = fs::write(file(), serialize(s));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip() {
        for mode in [Mode::Light, Mode::Dark, Mode::Auto] {
            for updates in [true, false] {
                let s = Settings { mode, updates };
                assert_eq!(parse(&serialize(&s)), s);
            }
        }
        assert_eq!(parse("garbage"), Settings::default());
        assert_eq!(parse("theme=dark\n"), Settings { mode: Mode::Dark, updates: true }); // old files still work
    }
}
