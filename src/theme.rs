//! Colors adapted for Hayai from Atom One Dark/Light, Catppuccin, Tokyo Night
//! and Gruvbox.nvim. Selection/hover/border roles are modified. Copyright and
//! license notices: assets/themes and THIRD_PARTY_NOTICES.
use gpui::{App, Hsla, rgb};
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
    ] = match (family, light) {
        (ThemeFamily::Hayai, true) => [
            0xf8f8fa, 0x202024, 0xd4d4da, 0x666670, 0xe7e7ec, 0xeeeef2, 0x666670,
        ],
        (ThemeFamily::Hayai, false) => [
            0x1c1c1e, 0xf2f2f2, 0x343437, 0x9a9a9e, 0x2f2f34, 0x27272a, 0xb0b0b8,
        ],
        (ThemeFamily::OneDark, true) => [
            0xfafafa, 0x383a42, 0xd5d7df, 0x696c77, 0xe5e7ee, 0xeff0f4, 0x4078f2,
        ],
        (ThemeFamily::OneDark, false) => [
            0x282c34, 0xabb2bf, 0x3e4451, 0x828997, 0x3e4451, 0x303640, 0x61afef,
        ],
        (ThemeFamily::Catppuccin, true) => [
            0xeff1f5, 0x4c4f69, 0xccd0da, 0x6c6f85, 0xdce0e8, 0xe6e9ef, 0x8839ef,
        ],
        (ThemeFamily::Catppuccin, false) => [
            0x1e1e2e, 0xcdd6f4, 0x45475a, 0xa6adc8, 0x45475a, 0x313244, 0xcba6f7,
        ],
        (ThemeFamily::TokyoNight, true) => [
            0xe1e2e7, 0x3760bf, 0xb4b5b9, 0x6172b0, 0xc4c8da, 0xd4d6e4, 0x2e7de9,
        ],
        (ThemeFamily::TokyoNight, false) => [
            0x1a1b26, 0xc0caf5, 0x3b4261, 0x9aa5ce, 0x292e42, 0x24283b, 0x7aa2f7,
        ],
        (ThemeFamily::Gruvbox, true) => [
            0xfbf1c7, 0x3c3836, 0xd5c4a1, 0x665c54, 0xebdbb2, 0xf2e5bc, 0x9d0006,
        ],
        (ThemeFamily::Gruvbox, false) => [
            0x282828, 0xebdbb2, 0x504945, 0xbdae93, 0x504945, 0x3c3836, 0xd79921,
        ],
    }
    .map(|color| rgb(color).into());
    LauncherPalette {
        background,
        foreground,
        border,
        muted,
        selected,
        hover,
        accent,
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
}

pub fn settings_palette(cx: &App) -> SettingsPalette {
    let theme = Theme::global(cx);
    SettingsPalette {
        canvas: theme.background,
        sidebar: theme.background,
        panel: theme.background,
        border: theme.border,
        hover: theme.list_hover,
        selected: theme.list_active,
        foreground: theme.foreground,
        muted: theme.muted_foreground,
        accent: theme.primary,
        keycap: theme.list_active,
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
    theme.list_active = palette.selected;
    theme.list_active_border = palette.selected;
    theme.list_hover = palette.hover;
    theme.list.active_highlight = true;
    theme.title_bar = palette.background;
    theme.title_bar_border = palette.border;
    // Widgets read the derived tokens on hot paths, not just colors.
    theme.tokens = ThemeTokens::from(&theme.colors);
}
