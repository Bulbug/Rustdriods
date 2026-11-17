//! Colours, fonts and the overall look (light and dark). One accent colour, lots of white space.
use eframe::egui::{
    self, style::Selection, Color32, FontData, FontDefinitions, FontFamily, FontId, Rounding, Stroke,
    TextStyle, Visuals,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Light,
    Dark,
    Auto, // follow Windows
}

/// Every colour the app uses.
#[derive(Clone, Copy)]
pub struct Pal {
    pub bg: Color32,    // page background
    pub sd: Color32,    // sidebar and log bar
    pub pn: Color32,    // panels and rows
    pub sel: Color32,   // selected row
    pub ln: Color32,    // hairlines
    pub ln2: Color32,   // stronger lines (buttons, inputs)
    pub tx: Color32,    // text
    pub mu: Color32,    // muted text
    pub ac: Color32,    // accent
    pub on: Color32,    // text on the accent colour
    pub tint: Color32,  // soft accent background
    pub tintx: Color32, // text on the soft accent background
    pub okb: Color32,
    pub ok: Color32,
    pub wb: Color32,
    pub wn: Color32,
    pub sb: Color32, // "stopped" chip
    pub st: Color32,
    pub dg: Color32, // danger text
    pub dgb: Color32, // danger outline
    pub g1: Color32, // phone picture
    pub pf: Color32, // phone frame
}

const fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

pub const LIGHT: Pal = Pal {
    bg: hex(0xF7F8FC), sd: hex(0xFFFFFF), pn: hex(0xFFFFFF), sel: hex(0xF1F1FE),
    ln: hex(0xE8E9F2), ln2: hex(0xD5D8EA), tx: hex(0x14172B), mu: hex(0x5E6482),
    ac: hex(0x5B5BF0), on: hex(0xFFFFFF), tint: hex(0xEEEEFF), tintx: hex(0x3535C4),
    okb: hex(0xE4F6EE), ok: hex(0x0E7A55), wb: hex(0xFFF1DC), wn: hex(0x8A5300),
    sb: hex(0xEEF0F8), st: hex(0x4A5170), dg: hex(0xB3261E), dgb: hex(0xF0C4C0),
    g1: hex(0xB4B4FF), pf: hex(0x14172B),
};

pub const DARK: Pal = Pal {
    bg: hex(0x0D0F1C), sd: hex(0x12152A), pn: hex(0x161A31), sel: hex(0x1D2247),
    ln: hex(0x252A49), ln2: hex(0x363C66), tx: hex(0xECEEFB), mu: hex(0xA5ABCF),
    ac: hex(0x8B8BFF), on: hex(0x0D0F1C), tint: hex(0x232857), tintx: hex(0xC3C4FF),
    okb: hex(0x123628), ok: hex(0x6EE7B7), wb: hex(0x3A2E12), wn: hex(0xFBBF24),
    sb: hex(0x202544), st: hex(0xB8BEE2), dg: hex(0xFF8A80), dgb: hex(0x5A2E3A),
    g1: hex(0x4B4BD0), pf: hex(0x3B4170),
};

/// The palette for the theme that is active right now.
pub fn pal(ui: &egui::Ui) -> Pal {
    if ui.visuals().dark_mode { DARK } else { LIGHT }
}

pub fn bold_family() -> FontFamily {
    FontFamily::Name("bold".into())
}

fn install_fonts(ctx: &egui::Context) {
    let mut f = FontDefinitions::default();
    f.font_data.insert(
        "jakarta".into(),
        FontData::from_static(include_bytes!("../assets/fonts/PlusJakartaSans-Regular.ttf")),
    );
    f.font_data.insert(
        "jakarta-bold".into(),
        FontData::from_static(include_bytes!("../assets/fonts/PlusJakartaSans-Bold.ttf")),
    );
    let prop = f.families.get_mut(&FontFamily::Proportional).expect("proportional family");
    prop.insert(0, "jakarta".into());
    let mut bold = vec!["jakarta-bold".to_string()];
    bold.extend(prop.iter().cloned()); // fall back to the default fonts for missing symbols
    f.families.insert(bold_family(), bold);
    ctx.set_fonts(f);
}

fn make_style(p: &Pal, dark: bool) -> egui::Style {
    let mut s = egui::Style::default();
    let mut v = if dark { Visuals::dark() } else { Visuals::light() };

    v.panel_fill = p.bg;
    v.window_fill = p.pn;
    v.window_stroke = Stroke::new(1.0_f32, p.ln);
    v.window_rounding = Rounding::same(16.0);
    v.menu_rounding = Rounding::same(12.0);
    v.extreme_bg_color = p.pn; // text boxes
    v.faint_bg_color = p.sb;
    v.code_bg_color = p.sb;
    v.hyperlink_color = p.ac;
    v.warn_fg_color = p.wn;
    v.error_fg_color = p.dg;
    v.selection = Selection { bg_fill: p.tint, stroke: Stroke::new(1.0_f32, p.ac) };
    v.slider_trailing_fill = true;

    let r = Rounding::same(12.0);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.pn;
    w.noninteractive.weak_bg_fill = p.pn;
    w.noninteractive.bg_stroke = Stroke::new(1.0_f32, p.ln);
    w.noninteractive.fg_stroke = Stroke::new(1.0_f32, p.tx);
    w.noninteractive.rounding = r;

    w.inactive.bg_fill = p.pn;
    w.inactive.weak_bg_fill = p.pn;
    w.inactive.bg_stroke = Stroke::new(1.0_f32, p.ln2);
    w.inactive.fg_stroke = Stroke::new(1.0_f32, p.tx);
    w.inactive.rounding = r;

    for st in [&mut w.hovered, &mut w.active, &mut w.open] {
        st.bg_fill = p.tint;
        st.weak_bg_fill = p.tint;
        st.bg_stroke = Stroke::new(1.0_f32, p.ac);
        st.fg_stroke = Stroke::new(1.5_f32, p.tx);
        st.rounding = r;
    }
    s.visuals = v;

    s.spacing.item_spacing = egui::vec2(10.0, 10.0);
    s.spacing.button_padding = egui::vec2(16.0, 9.0);
    s.spacing.interact_size = egui::vec2(36.0, 36.0);
    s.spacing.window_margin = egui::Margin::same(20.0);
    s.spacing.slider_width = 200.0;

    let b = bold_family();
    s.text_styles = [
        (TextStyle::Heading, FontId::new(26.0, b.clone())),
        (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(13.5, b)),
        (TextStyle::Small, FontId::new(12.0, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
    ]
    .into();
    s
}

pub fn preference(mode: Mode) -> egui::ThemePreference {
    match mode {
        Mode::Light => egui::ThemePreference::Light,
        Mode::Dark => egui::ThemePreference::Dark,
        Mode::Auto => egui::ThemePreference::System,
    }
}

/// Call once at start-up.
pub fn install(ctx: &egui::Context, mode: Mode) {
    install_fonts(ctx);
    ctx.set_style_of(egui::Theme::Light, make_style(&LIGHT, false));
    ctx.set_style_of(egui::Theme::Dark, make_style(&DARK, true));
    ctx.set_theme(preference(mode));
}

/// Call when the user picks Light / Dark / Match Windows.
pub fn set_mode(ctx: &egui::Context, mode: Mode) {
    ctx.set_theme(preference(mode));
}
