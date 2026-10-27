//! Device-pixel placement.
//!
//! Windows scales from 100% upward in 25% steps (and custom values), so a
//! logical size is a whole number of device pixels only sometimes, and
//! centering a child in a box leaves half a pixel whenever the free space is
//! odd. The renderer then rounds each edge on its own and the child lands a
//! pixel off-center. Anything fixed-size that is centered takes its offsets
//! from here, so they are whole device pixels at every scale; when the free
//! space is odd the extra pixel goes after the child.
//!
//! Text is the second source of drift: a line box centers text by font
//! metrics (ascent plus descent), not by the glyphs people see. [`text_top`]
//! corrects for that from the font's metrics instead of per-widget nudges.

use gpui::{App, Global, Pixels, Window, px};

struct Scale(f32);
impl Global for Scale {}

/// Record the window's scale. Call at the top of every window's render, before
/// any helper below runs.
pub fn sync(window: &Window, cx: &mut App) {
    let scale = window.scale_factor();
    if cx.try_global::<Scale>().map(|current| current.0) != Some(scale) {
        cx.set_global(Scale(scale));
    }
}

fn scale(cx: &App) -> f32 {
    cx.try_global::<Scale>().map_or(1.0, |scale| scale.0)
}

/// The nearest whole-device-pixel size to `logical`.
pub fn snap(cx: &App, logical: f32) -> Pixels {
    snap_at(scale(cx), logical)
}

/// One device pixel, the width of a crisp hairline.
pub fn hairline(cx: &App) -> Pixels {
    px(1.0 / scale(cx))
}

/// Leading offset that centers a `child` span in a `container` span.
pub fn lead(cx: &App, container: f32, child: f32) -> Pixels {
    lead_at(scale(cx), container, child)
}

/// Size and offset for an icon centered in a box `container` logical pixels
/// across with `border_dev` device pixels of border on each side. The icon
/// grows or shrinks by at most one device pixel so the free space is even:
/// a glyph can change size by a pixel without anyone noticing, but a half
/// pixel of offset makes it look shifted toward the top left.
pub fn fit(cx: &App, container: f32, border_dev: f32, child: f32) -> (Pixels, Pixels) {
    fit_at(scale(cx), container, border_dev, child)
}

/// Which glyphs the text shows. Fonts draw capitals, lowercase and symbols
/// around different heights, and the eye centers what it sees.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyphs {
    Capitals,
    Lowercase,
    Symbols,
}

impl Glyphs {
    pub fn of(label: &str) -> Self {
        let mut chars = label.chars();
        match (chars.next(), chars.next()) {
            (Some('=' | '+' | '-'), None) => Self::Symbols,
            (Some(first), None) if first.is_alphanumeric() => Self::Capitals,
            _ => Self::Lowercase,
        }
    }

    /// Height of the visible glyphs' middle above the baseline, in em.
    /// Segoe UI: capitals 0.70, ascenders 0.74, math signs about 0.54.
    fn middle(self) -> f32 {
        match self {
            Self::Capitals => 0.35,
            Self::Lowercase => 0.40,
            Self::Symbols => 0.27,
        }
    }
}

/// Where a line box centers text, above the baseline, in em: half the
/// difference of Segoe UI's ascent (1.079) and descent (0.251).
const LINE_MIDDLE: f32 = 0.414;

/// Top offset for a `line`-tall text box inside a box `container` tall (less
/// `inset_dev` device pixels of border on each side), so the glyphs read as
/// centered.
pub fn text_top(
    cx: &App,
    container: f32,
    inset_dev: f32,
    line: f32,
    size: f32,
    glyphs: Glyphs,
) -> Pixels {
    text_top_at(scale(cx), container, inset_dev, line, size, glyphs)
}

fn fit_at(scale: f32, container: f32, border_dev: f32, child: f32) -> (Pixels, Pixels) {
    let inner = (container * scale).round() - border_dev * 2.0;
    let mut size = (child * scale).round().min(inner).max(1.0);
    if (inner - size) % 2.0 != 0.0 {
        size = if size + 1.0 <= inner {
            size + 1.0
        } else {
            size - 1.0
        };
    }
    (px(size / scale), px((inner - size) / 2.0 / scale))
}

fn snap_at(scale: f32, logical: f32) -> Pixels {
    px((logical * scale).round() / scale)
}

fn lead_at(scale: f32, container: f32, child: f32) -> Pixels {
    let free = ((container * scale).round() - (child * scale).round()).max(0.0);
    px((free / 2.0).floor() / scale)
}

fn text_top_at(
    scale: f32,
    container: f32,
    inset_dev: f32,
    line: f32,
    size: f32,
    glyphs: Glyphs,
) -> Pixels {
    let lift = ((LINE_MIDDLE - glyphs.middle()) * size * scale)
        .round()
        .max(0.0);
    let free = (container * scale).round() - inset_dev * 2.0 - (line * scale).round();
    px(((free / 2.0).floor() - lift).max(0.0) / scale)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCALES: [f32; 12] = [
        1.0, 1.1, 1.25, 1.33, 1.5, 1.75, 2.0, 2.25, 2.5, 3.0, 3.5, 4.0,
    ];

    fn device(pixels: Pixels, scale: f32) -> f32 {
        f32::from(pixels) * scale
    }

    fn is_whole(value: f32) -> bool {
        (value - value.round()).abs() < 1e-3
    }

    #[test]
    fn offsets_are_whole_device_pixels_and_balanced_at_every_scale() {
        for scale in SCALES {
            for (container, child) in [(24.0, 16.0), (36.0, 16.0), (20.0, 12.0), (40.0, 16.0)] {
                let lead = device(lead_at(scale, container, child), scale);
                let free = (container * scale).round() - (child * scale).round();
                assert!(is_whole(lead), "{scale}: {container}/{child}");
                assert!(
                    (free - lead * 2.0).abs() <= 1.0 + 1e-3,
                    "{scale}: {container}/{child} leaves {free} around {lead}"
                );
            }
        }
    }

    #[test]
    fn snapped_sizes_and_text_offsets_are_whole_device_pixels() {
        for scale in SCALES {
            assert!(is_whole(device(snap_at(scale, 20.0), scale)));
            for glyphs in [Glyphs::Capitals, Glyphs::Lowercase, Glyphs::Symbols] {
                let top = text_top_at(scale, 20.0, 1.0, 14.0, 11.0, glyphs);
                assert!(is_whole(device(top, scale)), "{scale} {glyphs:?}");
            }
        }
    }

    #[test]
    fn fitted_icons_center_exactly_at_every_scale() {
        for scale in SCALES {
            for (container, border, child) in [
                (16.0, 1.0, 12.0),
                (24.0, 0.0, 16.0),
                (36.0, 0.0, 16.0),
                (20.0, 1.0, 16.0),
            ] {
                let (size, lead) = fit_at(scale, container, border, child);
                let inner = (container * scale).round() - border * 2.0;
                let (size_dev, lead_dev) = (device(size, scale), device(lead, scale));
                assert!(is_whole(size_dev) && is_whole(lead_dev), "{scale}");
                assert!(
                    (inner - size_dev - lead_dev * 2.0).abs() < 1e-3,
                    "{scale}: not centered"
                );
                assert!(
                    (size_dev - (child * scale).round()).abs() <= 1.0 + 1e-3,
                    "{scale}: resized too much"
                );
            }
        }
    }

    #[test]
    fn symbols_lift_more_than_capitals() {
        let caps = text_top_at(1.25, 20.0, 1.0, 14.0, 11.0, Glyphs::Capitals);
        let symbols = text_top_at(1.25, 20.0, 1.0, 14.0, 11.0, Glyphs::Symbols);
        assert!(symbols < caps);
        assert_eq!(Glyphs::of("="), Glyphs::Symbols);
        assert_eq!(Glyphs::of("K"), Glyphs::Capitals);
        assert_eq!(Glyphs::of("Ctrl"), Glyphs::Lowercase);
    }
}
