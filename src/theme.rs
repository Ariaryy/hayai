//! Colors adapted for Hayai from Atom One Dark/Light, Catppuccin, Tokyo Night
//! and Gruvbox.nvim. Selection/hover/border roles are modified. Copyright and
//! license notices: assets/themes and THIRD_PARTY_NOTICES.
use gpui::{App, Hsla, Rgba, rgb};
use gpui_component::{Colorize, Theme, ThemeMode, ThemeTokens};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeFamily {
    #[default]
    Hayai,
    OneDark,
    Catppuccin,
    TokyoNight,
    Gruvbox,
}

impl ThemeFamily {
    pub const ALL: [Self; 5] = [
        Self::Hayai,
        Self::OneDark,
        Self::Catppuccin,
        Self::TokyoNight,
        Self::Gruvbox,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Hayai => "hayai",
            Self::OneDark => "one-dark",
            Self::Catppuccin => "catppuccin",
            Self::TokyoNight => "tokyo-night",
            Self::Gruvbox => "gruvbox",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|family| family.id() == id)
    }
}

/// The launcher, settings and appearance previews all use this same palette.
#[derive(Clone, Copy)]
pub struct LauncherPalette {
    pub background: Hsla,
    pub foreground: Hsla,
    pub border: Hsla,
    pub muted: Hsla,
    pub selected: Hsla,
    pub hover: Hsla,
    pub accent: Hsla,
    /// Icon-sized alerts, at least 3:1 against every surface.
    pub warning: Hsla,
    /// Error text, at least 4.5:1 against every surface.
    pub danger: Hsla,
}

fn luminance(color: Hsla) -> f32 {
    let color: Rgba = color.into();
    let linear = |channel: f32| {
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
}

/// `amount` of the way from `from` to `to` in Oklab. (`mix_oklab`'s factor
/// weights the receiver, which reads backwards for this use.)
fn blend(from: Hsla, to: Hsla, amount: f32) -> Hsla {
    from.mix_oklab(to, 1.0 - amount)
}

/// WCAG contrast ratio.
pub fn contrast(a: Hsla, b: Hsla) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// Move `color` toward `toward` just far enough to reach `minimum` contrast
/// against every surface. Upstream palettes were designed for one background;
/// Hayai places the same roles on raised and recessed surfaces too.
fn readable(color: Hsla, surfaces: &[Hsla], toward: Hsla, minimum: f32) -> Hsla {
    (0..=20)
        .map(|step| blend(color, toward, step as f32 / 20.0))
        .find(|candidate| {
            surfaces
                .iter()
                .all(|surface| contrast(*candidate, *surface) >= minimum)
        })
        .unwrap_or(toward)
}

/// Settings surfaces derived from a theme's background, so every theme gets
/// the same elevation order: sidebar recessed, canvas, then raised cards.
#[derive(Clone, Copy)]
struct Surfaces {
    sidebar: Hsla,
    panel: Hsla,
}

fn surfaces(background: Hsla, foreground: Hsla, light: bool) -> Surfaces {
    if light {
        Surfaces {
            sidebar: blend(background, foreground, 0.035),
            panel: blend(background, rgb(0xffffff).into(), 0.5),
        }
    } else {
        Surfaces {
            sidebar: blend(background, rgb(0x000000).into(), 0.2),
            panel: blend(background, foreground, 0.03),
        }
    }
}

pub fn launcher_palette(family: ThemeFamily, light: bool) -> LauncherPalette {
    // Base colors come from the upstream palettes; hover/selection surfaces
    // adapt those colors to Hayai's list UI. Sources and licenses: assets/themes.
    let [
        background,
        foreground,
        border,
        muted,
        selected,
        hover,
        accent,
        warning,
        danger,
    ] = match (family, light) {
        (ThemeFamily::Hayai, true) => [
            0xf8f8fa, 0x202024, 0xd4d4da, 0x666670, 0xe7e7ec, 0xeeeef2, 0x666670, 0xb45309,
            0xdc2626,
        ],
        (ThemeFamily::Hayai, false) => [
            0x1c1c1e, 0xf2f2f2, 0x343437, 0x9a9a9e, 0x2f2f34, 0x27272a, 0xb0b0b8, 0xf5a524,
            0xf87171,
        ],
        (ThemeFamily::OneDark, true) => [
            0xfafafa, 0x383a42, 0xd5d7df, 0x696c77, 0xe5e7ee, 0xeff0f4, 0x4078f2, 0xc18401,
            0xe45649,
        ],
        (ThemeFamily::OneDark, false) => [
            0x282c34, 0xabb2bf, 0x3e4451, 0x828997, 0x3e4451, 0x303640, 0x61afef, 0xe5c07b,
            0xe06c75,
        ],
        (ThemeFamily::Catppuccin, true) => [
            0xeff1f5, 0x4c4f69, 0xccd0da, 0x6c6f85, 0xdce0e8, 0xe6e9ef, 0x8839ef, 0xdf8e1d,
            0xd20f39,
        ],
        (ThemeFamily::Catppuccin, false) => [
            0x1e1e2e, 0xcdd6f4, 0x45475a, 0xa6adc8, 0x45475a, 0x313244, 0xcba6f7, 0xfab387,
            0xf38ba8,
        ],
        (ThemeFamily::TokyoNight, true) => [
            0xe1e2e7, 0x3760bf, 0xb4b5b9, 0x6172b0, 0xc4c8da, 0xd4d6e4, 0x2e7de9, 0x8c6c3e,
            0xf52a65,
        ],
        (ThemeFamily::TokyoNight, false) => [
            0x1a1b26, 0xc0caf5, 0x3b4261, 0x9aa5ce, 0x292e42, 0x24283b, 0x7aa2f7, 0xe0af68,
            0xf7768e,
        ],
        (ThemeFamily::Gruvbox, true) => [
            0xfbf1c7, 0x3c3836, 0xd5c4a1, 0x665c54, 0xebdbb2, 0xf2e5bc, 0x9d0006, 0xb57614,
            0xcc241d,
        ],
        (ThemeFamily::Gruvbox, false) => [
            0x282828, 0xebdbb2, 0x504945, 0xbdae93, 0x504945, 0x3c3836, 0xd79921, 0xfabd2f,
            0xfb4934,
        ],
    }
    .map(|color| rgb(color).into());
    let Surfaces { sidebar, panel } = surfaces(background, foreground, light);
    let all = [background, sidebar, panel];
    let extreme = rgb(if light { 0x000000 } else { 0xffffff }).into();
    let foreground = readable(foreground, &all, extreme, 4.5);
    LauncherPalette {
        background,
        foreground,
        border,
        muted: readable(muted, &all, foreground, 4.5),
        selected,
        hover,
        accent: readable(accent, &all, foreground, 3.0),
        warning: readable(warning, &all, foreground, 3.0),
        danger: readable(danger, &all, foreground, 4.5),
    }
}

pub struct SettingsPalette {
    pub canvas: Hsla,
    pub sidebar: Hsla,
    pub panel: Hsla,
    pub border: Hsla,
    pub hover: Hsla,
    pub selected: Hsla,
    pub foreground: Hsla,
    pub muted: Hsla,
    pub accent: Hsla,
    pub keycap: Hsla,
    pub warning: Hsla,
    pub danger: Hsla,
}

pub fn settings_palette(cx: &App) -> SettingsPalette {
    let theme = Theme::global(cx);
    let light = !theme.mode.is_dark();
    let roles = launcher_palette(
        cx.try_global::<crate::settings::SettingsGlobal>()
            .map(|settings| settings.preferences.theme_for(light))
            .unwrap_or_default(),
        light,
    );
    let Surfaces { sidebar, panel } =
        surfaces(theme.background, theme.foreground, !theme.mode.is_dark());
    SettingsPalette {
        canvas: theme.background,
        sidebar,
        panel,
        border: theme.border,
        hover: theme.list_hover,
        selected: theme.list_active,
        foreground: theme.foreground,
        muted: theme.muted_foreground,
        accent: theme.primary,
        keycap: theme.list_active,
        warning: roles.warning,
        danger: roles.danger,
    }
}

pub struct SwatchColors {
    pub highlight: Hsla,
    pub base: Hsla,
    pub shadow: Hsla,
}

pub fn swatch_colors(family: ThemeFamily, light: bool) -> SwatchColors {
    let palette = launcher_palette(family, light);
    // Foreground-to-background ramps invert a theme's apparent brightness.
    // Keep the whole ramp near its background; add hue through the accent.
    if light {
        SwatchColors {
            highlight: palette.background.mix_oklab(rgb(0xffffff).into(), 0.65),
            base: palette.background,
            shadow: palette.background.mix_oklab(palette.accent, 0.18),
        }
    } else {
        SwatchColors {
            highlight: palette.background.mix_oklab(palette.accent, 0.4),
            base: palette.background.mix_oklab(palette.accent, 0.08),
            shadow: palette.background.mix_oklab(rgb(0x000000).into(), 0.35),
        }
    }
}

pub fn set_mode(light: bool, cx: &mut App) {
    Theme::change(
        if light {
            ThemeMode::Light
        } else {
            ThemeMode::Dark
        },
        None,
        cx,
    );
    apply_launcher_theme(cx);
    cx.refresh_windows();
}

/// Keep selected-row fills and borders identical to avoid a blue outline.
pub fn apply_launcher_theme(cx: &mut App) {
    let light = Theme::global(cx).mode == ThemeMode::Light;
    let family = cx
        .try_global::<crate::settings::SettingsGlobal>()
        .map(|settings| settings.preferences.theme_for(light))
        .unwrap_or_default();
    let theme = Theme::global_mut(cx);
    let palette = launcher_palette(family, light);
    theme.background = palette.background;
    theme.foreground = palette.foreground;
    theme.border = palette.border;
    theme.muted_foreground = palette.muted;
    theme.accent = palette.selected;
    theme.primary = palette.accent;
    theme.popover = palette.background;
    theme.popover_foreground = palette.foreground;
    theme.accent_foreground = palette.foreground;
    theme.input = palette.border;
    // The off track is a control, not a surface: keep it 3:1 clear of both the
    // card it sits on and the thumb.
    let Surfaces { sidebar, panel } = surfaces(palette.background, palette.foreground, light);
    theme.switch = readable(
        palette.selected,
        &[palette.background, sidebar, panel],
        palette.foreground,
        3.0,
    );
    // The titlebar's close button reads these: Windows-style red, white glyph.
    // Error text uses `SettingsPalette::danger`, which adapts to each theme.
    theme.danger = rgb(0xe81123).into();
    theme.danger_active = rgb(0x8a0a15).into();
    theme.danger_foreground = rgb(0xffffff).into();
    theme.switch_thumb = palette.background;
    theme.list_active = palette.selected;
    theme.list_active_border = palette.selected;
    theme.list_hover = palette.hover;
    theme.list.active_highlight = true;
    theme.title_bar = palette.background;
    theme.title_bar_border = palette.border;
    // Widgets read the derived tokens on hot paths, not just colors.
    theme.tokens = ThemeTokens::from(&theme.colors);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_palette_keeps_text_icons_and_surfaces_legible() {
        for family in ThemeFamily::ALL {
            for light in [false, true] {
                let name = format!("{} {}", family.id(), if light { "light" } else { "dark" });
                let palette = launcher_palette(family, light);
                let Surfaces { sidebar, panel } =
                    surfaces(palette.background, palette.foreground, light);
                for surface in [palette.background, sidebar, panel] {
                    assert!(contrast(palette.foreground, surface) >= 4.5, "{name} text");
                    assert!(contrast(palette.muted, surface) >= 4.5, "{name} muted");
                    assert!(contrast(palette.danger, surface) >= 4.5, "{name} danger");
                    assert!(contrast(palette.warning, surface) >= 3.0, "{name} warning");
                    assert!(contrast(palette.accent, surface) >= 3.0, "{name} accent");
                }
                // Recessed, canvas, raised: a visible but quiet elevation order.
                let low = luminance(sidebar);
                let mid = luminance(palette.background);
                let high = luminance(panel);
                assert!(low < mid && mid < high, "{name} elevation");
                assert!(panel != palette.hover, "{name} hover differs from cards");
            }
        }
    }
}
