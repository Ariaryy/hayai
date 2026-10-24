use gpui::{App, rgb};
use gpui_component::{Theme, ThemeTokens};

// Launcher-specific colors supplement gpui-component's shared widget palette.
pub const KEYCAP_FOREGROUND: u32 = 0xd4d4d8;
pub const KEYCAP_BACKGROUND: u32 = 0x3b3b40;
pub const OPEN_ACTIONS_BACKGROUND: u32 = 0x2b2b30;

/// Override gpui-component's default (blue-accented) dark theme with a
/// neutral grey palette — in particular, the selected-row highlight and its
/// border both become the same grey so there's no blue outline on
/// selection.
pub fn apply_launcher_theme(cx: &mut App) {
    let theme = Theme::global_mut(cx);

    let surface = rgb(0x2f2f34).into();
    theme.background = rgb(0x1c1c1e).into();
    theme.foreground = rgb(0xf2f2f2).into();
    theme.border = rgb(0x343437).into();
    theme.muted_foreground = rgb(0x9a9a9e).into();
    theme.accent = surface;
    theme.list_active = surface;
    theme.list_active_border = surface;
    theme.list_hover = rgb(0x27272a).into();
    theme.list.active_highlight = true;

    // `tokens` is a derived snapshot of `colors` for hot paths (e.g. the
    // ListItem hover fill reads `tokens.list_hover`, not `colors.list_hover`
    // directly), so it must be regenerated after mutating colors above.
    theme.tokens = ThemeTokens::from(&theme.colors);
}
