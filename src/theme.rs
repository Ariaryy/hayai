//! Shared palettes for launcher, settings and appearance previews.
use gpui::{App, Hsla, rgb};
use gpui_component::{Theme, ThemeMode, ThemeTokens};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeFamily {
    #[default]
    Hayai,
}

impl ThemeFamily {
    pub const ALL: [Self; 1] = [Self::Hayai];

    pub fn id(self) -> &'static str {
        match self {
            Self::Hayai => "hayai",
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
