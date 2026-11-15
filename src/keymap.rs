//! Keymap profiles: which keyboard key taps which spot on the screen.
//! Positions are stored as fractions (0.0 - 1.0) of the screen, so a profile still works
//! if the device resolution changes. One small text file per profile (one per game).
use crate::{paths, Res};
use eframe::egui;
use std::fs;

/// Reserved: pauses and resumes mapping while the emulator window is focused.
pub const TOGGLE_LABEL: &str = "F10";

#[derive(Clone, Debug, PartialEq)]
pub struct Binding {
    pub key: String, // label like "W", "SPACE", "UP"
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Profile {
    pub name: String,
    pub landscape: bool,
    pub binds: Vec<Binding>,
}

impl Profile {
    /// Adds a binding; a key can only do one thing, so an old binding of the same key is replaced.
    pub fn set(&mut self, key: &str, x: f32, y: f32) {
        self.binds.retain(|b| b.key != key);
        self.binds.push(Binding { key: key.to_string(), x, y });
    }

    pub fn to_text(&self) -> String {
        let mut s = String::from("# Android Emulator Manager keymap v1\n");
        s += &format!("orientation {}\n", if self.landscape { "landscape" } else { "portrait" });
        for b in &self.binds {
            s += &format!("bind {} {:.4} {:.4}\n", b.key, b.x, b.y);
        }
        s
    }

    pub fn from_text(name: &str, text: &str) -> Profile {
        let mut p = Profile { name: name.to_string(), landscape: false, binds: Vec::new() };
        for line in text.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            match parts.as_slice() {
                ["orientation", o] => p.landscape = *o == "landscape",
                ["bind", key, x, y] => {
                    if let (Ok(x), Ok(y)) = (x.parse::<f32>(), y.parse::<f32>()) {
                        p.set(key, x.clamp(0.0, 1.0), y.clamp(0.0, 1.0));
                    }
                }
                _ => {}
            }
        }
        p
    }
}

pub fn valid_name(name: &str) -> bool {
    let n = name.trim();
    !n.is_empty()
        && n.len() <= 40
        && !n.starts_with('.')
        && n.chars().all(|c| c.is_alphanumeric() || " -_.".contains(c))
}

fn file_for(name: &str) -> std::path::PathBuf {
    paths::keymaps_dir().join(format!("{}.txt", name.trim()))
}

pub fn list_profiles() -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(paths::keymaps_dir())
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| e.file_name().to_string_lossy().strip_suffix(".txt").map(String::from))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

pub fn load(name: &str) -> Res<Profile> {
    if !valid_name(name) {
        return Err("Invalid profile name.".into());
    }
    Ok(Profile::from_text(name.trim(), &fs::read_to_string(file_for(name))?))
}

pub fn save(p: &Profile) -> Res<()> {
    if !valid_name(&p.name) {
        return Err("Invalid profile name (use letters, numbers, spaces, - _ .).".into());
    }
    fs::create_dir_all(paths::keymaps_dir())?;
    fs::write(file_for(&p.name), p.to_text())?;
    Ok(())
}

pub fn delete(name: &str) -> Res<()> {
    if !valid_name(name) {
        return Err("Invalid profile name.".into());
    }
    fs::remove_file(file_for(name))?;
    Ok(())
}

/// Windows virtual-key code for a label. F10 is not allowed (it is the pause key).
pub fn vk_for_label(label: &str) -> Option<u32> {
    let b = label.as_bytes();
    if b.len() == 1 && (b[0].is_ascii_uppercase() || b[0].is_ascii_digit()) {
        return Some(b[0] as u32); // 'A'-'Z' and '0'-'9' are their own codes
    }
    if let Some(n) = label.strip_prefix('F').and_then(|r| r.parse::<u32>().ok()) {
        if (1..=12).contains(&n) && n != 10 {
            return Some(0x6F + n);
        }
    }
    Some(match label {
        "SPACE" => 0x20,
        "ENTER" => 0x0D,
        "TAB" => 0x09,
        "BACKSPACE" => 0x08,
        "UP" => 0x26,
        "DOWN" => 0x28,
        "LEFT" => 0x25,
        "RIGHT" => 0x27,
        "INSERT" => 0x2D,
        "DELETE" => 0x2E,
        "HOME" => 0x24,
        "END" => 0x23,
        "PAGEUP" => 0x21,
        "PAGEDOWN" => 0x22,
        ";" => 0xBA,
        "=" => 0xBB,
        "," => 0xBC,
        "-" => 0xBD,
        "." => 0xBE,
        "/" => 0xBF,
        "`" => 0xC0,
        "[" => 0xDB,
        "\\" => 0xDC,
        "]" => 0xDD,
        _ => return None,
    })
}

macro_rules! by_name {
    ($k:expr; $($v:ident),*) => {
        match $k { $(egui::Key::$v => return Some(stringify!($v)),)* _ => {} }
    };
}

/// The label for a key pressed in our window (used while recording a binding).
pub fn label_for_egui(key: egui::Key) -> Option<&'static str> {
    by_name!(key; A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
        F1, F2, F3, F4, F5, F6, F7, F8, F9, F11, F12);
    Some(match key {
        egui::Key::Num0 => "0",
        egui::Key::Num1 => "1",
        egui::Key::Num2 => "2",
        egui::Key::Num3 => "3",
        egui::Key::Num4 => "4",
        egui::Key::Num5 => "5",
        egui::Key::Num6 => "6",
        egui::Key::Num7 => "7",
        egui::Key::Num8 => "8",
        egui::Key::Num9 => "9",
        egui::Key::Space => "SPACE",
        egui::Key::Enter => "ENTER",
        egui::Key::Tab => "TAB",
        egui::Key::Backspace => "BACKSPACE",
        egui::Key::ArrowUp => "UP",
        egui::Key::ArrowDown => "DOWN",
        egui::Key::ArrowLeft => "LEFT",
        egui::Key::ArrowRight => "RIGHT",
        egui::Key::Insert => "INSERT",
        egui::Key::Delete => "DELETE",
        egui::Key::Home => "HOME",
        egui::Key::End => "END",
        egui::Key::PageUp => "PAGEUP",
        egui::Key::PageDown => "PAGEDOWN",
        egui::Key::Semicolon => ";",
        egui::Key::Equals => "=",
        egui::Key::Comma => ",",
        egui::Key::Minus => "-",
        egui::Key::Period => ".",
        egui::Key::Slash => "/",
        egui::Key::Backtick => "`",
        egui::Key::OpenBracket => "[",
        egui::Key::CloseBracket => "]",
        egui::Key::Backslash => "\\",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_text_round_trip() {
        let mut p = Profile { name: "Game".into(), landscape: true, binds: vec![] };
        p.set("W", 0.25, 0.5);
        p.set("SPACE", 0.9, 0.8);
        p.set("W", 0.3, 0.6); // replaces the first W
        let back = Profile::from_text("Game", &p.to_text());
        assert_eq!(back.binds.len(), 2);
        assert!(back.landscape);
        assert!(back.binds.iter().any(|b| b.key == "W" && (b.x - 0.3).abs() < 1e-4));
    }

    #[test]
    fn broken_lines_are_ignored() {
        let p = Profile::from_text("x", "bind W nope 0.5\nbind Q 2.0 -1.0\nrandom\n");
        assert_eq!(p.binds.len(), 1);
        assert_eq!((p.binds[0].x, p.binds[0].y), (1.0, 0.0)); // clamped to the screen
    }

    #[test]
    fn key_codes() {
        assert_eq!(vk_for_label("W"), Some(0x57));
        assert_eq!(vk_for_label("5"), Some(0x35));
        assert_eq!(vk_for_label("F1"), Some(0x70));
        assert_eq!(vk_for_label("F12"), Some(0x7B));
        assert_eq!(vk_for_label("F10"), None); // reserved
        assert_eq!(vk_for_label("SPACE"), Some(0x20));
        assert_eq!(vk_for_label("nope"), None);
    }

    #[test]
    fn egui_keys_have_labels() {
        assert_eq!(label_for_egui(egui::Key::W), Some("W"));
        assert_eq!(label_for_egui(egui::Key::Num3), Some("3"));
        assert_eq!(label_for_egui(egui::Key::ArrowLeft), Some("LEFT"));
        assert_eq!(label_for_egui(egui::Key::F10), None);
        assert_eq!(label_for_egui(egui::Key::Escape), None);
    }

    #[test]
    fn profile_names() {
        assert!(valid_name("My Game 2"));
        assert!(!valid_name("../evil"));
        assert!(!valid_name(""));
    }
}
