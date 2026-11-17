//! Building blocks for the new look: buttons, chips, panels, rows, page headers.
use crate::icons::{self, Icon};
use crate::theme::{bold_family, Pal};
use eframe::egui::{
    self, pos2, vec2, Align, Button, Color32, FontId, Frame, Layout, Margin, Response, RichText, Rounding, Sense,
    Stroke, Ui, UiBuilder,
};
use std::hash::Hash;

pub fn bold(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).font(FontId::new(size, bold_family()))
}

fn pad(w: f32) -> egui::Vec2 {
    vec2(w, 40.0)
}

/// Filled accent button. `w` = minimum width (0 = fit the text).
pub fn primary(ui: &mut Ui, p: &Pal, text: &str, enabled: bool, w: f32) -> Response {
    let b = Button::new(bold(text, 13.5).color(p.on))
        .fill(p.ac)
        .stroke(Stroke::NONE)
        .rounding(12.0)
        .min_size(pad(w));
    ui.add_enabled(enabled, b)
}

/// Outlined button.
pub fn ghost(ui: &mut Ui, p: &Pal, text: &str, enabled: bool, w: f32) -> Response {
    let b = Button::new(bold(text, 13.5).color(p.tx))
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::new(1.0_f32, p.ln2))
        .rounding(12.0)
        .min_size(pad(w));
    ui.add_enabled(enabled, b)
}

/// Outlined red button (Stop, Delete).
pub fn danger(ui: &mut Ui, p: &Pal, text: &str, enabled: bool, w: f32) -> Response {
    let b = Button::new(bold(text, 13.5).color(p.dg))
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::new(1.0_f32, p.dgb))
        .rounding(12.0)
        .min_size(pad(w));
    ui.add_enabled(enabled, b)
}

/// Text link.
pub fn link(ui: &mut Ui, p: &Pal, text: &str) -> Response {
    ui.add(egui::Label::new(RichText::new(text).color(p.ac).underline()).sense(Sense::click()))
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

#[derive(Clone, Copy)]
pub enum Kind {
    Ok,
    Stopped,
    Warn,
    Accent,
}

pub fn chip(ui: &mut Ui, p: &Pal, kind: Kind, text: &str) {
    let (bg, fg) = match kind {
        Kind::Ok => (p.okb, p.ok),
        Kind::Stopped => (p.sb, p.st),
        Kind::Warn => (p.wb, p.wn),
        Kind::Accent => (p.tint, p.tintx),
    };
    Frame::none()
        .fill(bg)
        .rounding(Rounding::same(14.0))
        .inner_margin(Margin::symmetric(12.0, 5.0))
        .show(ui, |ui| {
            ui.label(bold(text, 12.0).color(fg));
        });
}

/// A white (or dark) rounded panel with a hairline border. Fills the available width.
pub fn panel<R>(ui: &mut Ui, p: &Pal, add: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::none()
        .fill(p.pn)
        .stroke(Stroke::new(1.0_f32, p.ln))
        .rounding(Rounding::same(18.0))
        .inner_margin(Margin::same(18.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// A soft accent-coloured banner.
pub fn banner<R>(ui: &mut Ui, p: &Pal, add: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::none()
        .fill(p.tint)
        .rounding(Rounding::same(16.0))
        .inner_margin(Margin::symmetric(18.0, 12.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// Big title, one line saying what the page is for, and actions on the right.
pub fn page_header(ui: &mut Ui, p: &Pal, title: &str, sub: &str, right: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.label(bold(title, 26.0));
            ui.label(RichText::new(sub).color(p.mu));
        });
        ui.with_layout(Layout::right_to_left(Align::Center), right);
    });
    ui.add_space(6.0);
}

pub fn stat(ui: &mut Ui, p: &Pal, big: &str, label: &str) {
    panel(ui, p, |ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.label(bold(big, 28.0));
        ui.label(RichText::new(label).color(p.mu).font(FontId::new(12.5, bold_family())));
    });
}

/// A clickable row (device, image). Content is laid out left to right, vertically centred.
pub fn row(ui: &mut Ui, p: &Pal, id: impl Hash, selected: bool, height: f32, add: impl FnOnce(&mut Ui)) -> Response {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    // The row's own click area is created first, so buttons inside it are on top and win clicks.
    let resp = ui.interact(rect, ui.id().with(id), Sense::click());
    let (fill, stroke) = if selected {
        (p.sel, Stroke::new(2.0_f32, p.ac))
    } else if resp.hovered() {
        (p.pn, Stroke::new(1.0_f32, p.ln2))
    } else {
        (p.pn, Stroke::new(1.0_f32, p.ln))
    };
    ui.painter().rect(rect, Rounding::same(16.0), fill, stroke);
    ui.allocate_new_ui(
        UiBuilder::new().max_rect(rect.shrink2(vec2(16.0, 0.0))).layout(Layout::left_to_right(Align::Center)),
        add,
    );
    resp
}

/// A small phone picture used for devices.
pub fn phone_thumb(ui: &mut Ui, p: &Pal, size: egui::Vec2) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let r = Rounding::same(size.x * 0.28);
    ui.painter().rect(rect, r, p.g1, Stroke::new(3.0_f32, p.pf));
    let bar = egui::Rect::from_center_size(pos2(rect.center().x, rect.bottom() - 7.0), vec2(size.x * 0.4, 3.0));
    ui.painter().rect_filled(bar, Rounding::same(1.5), p.pf);
}

/// Round status icon: a tick when ready, "!" when something is missing.
pub fn status_icon(ui: &mut Ui, p: &Pal, ok: bool) {
    let (rect, _) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::hover());
    let (bg, fg) = if ok { (p.okb, p.ok) } else { (p.wb, p.wn) };
    ui.painter().circle_filled(rect.center(), 15.0, bg);
    if ok {
        icons::paint(ui.painter(), Icon::Check, rect.center(), 15.0, fg, bg);
    } else {
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "!", FontId::new(15.0, bold_family()), fg);
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Seg {
    Done,
    Active,
    Todo,
}

/// A bar made of separate segments (one per setup step).
pub fn segments(ui: &mut Ui, p: &Pal, segs: &[Seg]) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 8.0), Sense::hover());
    let gap = 6.0;
    let n = segs.len().max(1) as f32;
    let w = (rect.width() - gap * (n - 1.0)) / n;
    for (i, s) in segs.iter().enumerate() {
        let x = rect.left() + i as f32 * (w + gap);
        let r = egui::Rect::from_min_size(pos2(x, rect.top()), vec2(w, 8.0));
        let c = match s {
            Seg::Done => p.ok,
            Seg::Active => p.ac,
            Seg::Todo => p.ln,
        };
        ui.painter().rect_filled(r, Rounding::same(4.0), c);
    }
}

/// A dashed "drop it here" box.
pub fn drop_zone(ui: &mut Ui, p: &Pal, title: &str, sub: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 84.0), Sense::hover());
    ui.painter().rect_filled(rect, Rounding::same(14.0), p.tint);
    let r = rect.shrink(1.0);
    let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
    ui.painter().extend(egui::Shape::dashed_line(&pts, Stroke::new(1.5_f32, p.ac), 8.0, 6.0));
    ui.painter().text(
        pos2(rect.center().x, rect.center().y - 10.0),
        egui::Align2::CENTER_CENTER,
        title,
        FontId::new(14.0, bold_family()),
        p.tintx,
    );
    ui.painter().text(
        pos2(rect.center().x, rect.center().y + 12.0),
        egui::Align2::CENTER_CENTER,
        sub,
        FontId::new(12.5, egui::FontFamily::Proportional),
        p.mu,
    );
}

/// A small clickable pill (used for example commands).
pub fn chip_button(ui: &mut Ui, p: &Pal, text: &str) -> Response {
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = vec2(12.0, 4.0);
        ui.add(
            Button::new(bold(text, 12.0).color(p.tintx))
                .fill(p.tint)
                .stroke(Stroke::NONE)
                .rounding(14.0)
                .min_size(vec2(0.0, 28.0)),
        )
    })
    .inner
}
