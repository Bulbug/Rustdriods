//! Small line icons drawn with the painter (no icon font needed, so they look the same everywhere).
use eframe::egui::{self, pos2, vec2, Color32, Painter, Pos2, Rect, Rounding, Stroke};

#[derive(Clone, Copy)]
pub enum Icon {
    Setup,
    Images,
    Devices,
    Adb,
    Keymap,
    Sun,
    Moon,
    Monitor,
    Check,
}

/// `size` is the icon's width and height. `bg` is only used by the moon (it cuts a bite out of a disc).
pub fn paint(p: &Painter, icon: Icon, c: Pos2, size: f32, color: Color32, bg: Color32) {
    let u = size; // one unit
    let st = Stroke::new((size / 10.0).max(1.4), color);
    let at = |x: f32, y: f32| pos2(c.x + x * u, c.y + y * u);
    let line = |a: Pos2, b: Pos2| {
        p.line_segment([a, b], st);
    };
    let poly = |pts: &[(f32, f32)]| {
        p.add(egui::Shape::line(pts.iter().map(|&(x, y)| at(x, y)).collect(), st));
    };
    match icon {
        Icon::Setup => {
            p.circle_stroke(c, 0.18 * u, st);
            for (dx, dy) in [(0.0, -1.0), (0.0, 1.0), (-1.0, 0.0), (1.0, 0.0)] {
                line(at(dx * 0.3, dy * 0.3), at(dx * 0.48, dy * 0.48));
            }
        }
        Icon::Images => {
            p.rect_stroke(Rect::from_center_size(c, vec2(0.84 * u, 0.84 * u)), Rounding::same(0.14 * u), st);
            poly(&[(-0.34, 0.2), (-0.12, -0.04), (0.06, 0.14), (0.18, 0.04), (0.34, 0.22)]);
        }
        Icon::Devices => {
            p.rect_stroke(Rect::from_center_size(c, vec2(0.5 * u, 0.9 * u)), Rounding::same(0.1 * u), st);
            line(at(-0.07, 0.32), at(0.07, 0.32));
        }
        Icon::Adb => {
            poly(&[(-0.14, -0.26), (-0.4, 0.0), (-0.14, 0.26)]);
            poly(&[(0.14, -0.26), (0.4, 0.0), (0.14, 0.26)]);
        }
        Icon::Keymap => {
            p.rect_stroke(Rect::from_center_size(c, vec2(0.9 * u, 0.56 * u)), Rounding::same(0.1 * u), st);
            for x in [-0.26_f32, 0.0, 0.26] {
                p.circle_filled(at(x, -0.08), 0.04 * u, color);
            }
            line(at(-0.24, 0.12), at(0.24, 0.12));
        }
        Icon::Sun => {
            p.circle_stroke(c, 0.2 * u, st);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::FRAC_PI_4;
                let (s, co) = a.sin_cos();
                line(at(co * 0.34, s * 0.34), at(co * 0.48, s * 0.48));
            }
        }
        Icon::Moon => {
            p.circle_filled(at(-0.02, 0.0), 0.32 * u, color);
            p.circle_filled(at(0.14, -0.1), 0.28 * u, bg);
        }
        Icon::Monitor => {
            p.rect_stroke(Rect::from_center_size(at(0.0, -0.06), vec2(0.86 * u, 0.56 * u)), Rounding::same(0.08 * u), st);
            line(at(-0.2, 0.34), at(0.2, 0.34));
            line(at(0.0, 0.22), at(0.0, 0.34));
        }
        Icon::Check => poly(&[(-0.3, 0.0), (-0.08, 0.22), (0.32, -0.2)]),
    }
}
