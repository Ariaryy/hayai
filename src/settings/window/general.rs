use super::*;
use crate::settings::{DisplayTarget, General};
use gpui_component::{
    Sizable,
    button::Button,
    menu::{DropdownMenu, PopupMenuItem},
};

#[derive(Clone, Copy)]
enum Toggle {
    Login,
    Recents,
    CheckUpdates,
    DownloadUpdates,
}
impl Toggle {
    fn metadata(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Login => ("login", text::LOGIN, text::LOGIN_HELP),
            Self::Recents => (
                "recents",
                text::REMEMBER_RECENTS,
                text::REMEMBER_RECENTS_HELP,
            ),
            Self::CheckUpdates => ("check-updates", text::AUTO_CHECK, text::AUTO_CHECK_HELP),
            Self::DownloadUpdates => (
                "download-updates",
                text::AUTO_DOWNLOAD,
                text::AUTO_DOWNLOAD_HELP,
            ),
        }
    }
    fn value(self, prefs: &General) -> bool {
        match self {
            Self::Login => prefs.launch_at_login,
            Self::Recents => prefs.remember_recents,
            Self::CheckUpdates => prefs.check_updates,
            Self::DownloadUpdates => prefs.download_updates,
        }
    }
    fn set(self, prefs: &mut General, value: bool) {
        match self {
            Self::Login => prefs.launch_at_login = value,
            Self::Recents => prefs.remember_recents = value,
            Self::CheckUpdates => prefs.check_updates = value,
            Self::DownloadUpdates => prefs.download_updates = value,
        }
    }
}
impl SettingsView {
    fn toggle_row(&self, setting: Toggle, cx: &Context<Self>) -> impl IntoElement {
        let (id, label, detail) = setting.metadata();
        let prefs = &preferences(cx).general;
        let disabled = matches!(setting, Toggle::DownloadUpdates) && !prefs.check_updates;
        setting_row(
            label,
            detail,
            components::ToggleSwitch::new(id, setting.value(prefs))
                .disabled(disabled)
                .on_change(move |checked, _, cx| {
                    let mut next = preferences(cx).clone();
                    setting.set(&mut next.general, checked);
                    save(next, cx);
                }),
            cx,
        )
    }

    pub(super) fn general_page(&self, cx: &Context<Self>) -> gpui::AnyElement {
        let palette = crate::theme::settings_palette(cx);
        let prefs = &preferences(cx).general;
        let selected = prefs.display;
        // Keep a single width for both trigger and popup, including their border.
        const WIDTH: f32 = 180.0;
        let display_choices = Button::new("display-target")
            .small()
            .outline()
            .w(px(WIDTH))
            .h(px(CONTROL_HEIGHT))
            .bg(palette.canvas)
            .text_color(palette.foreground)
            .border_color(palette.border)
            .child(
                h_flex()
                    .w_full()
                    .h(px(20.0))
                    .items_center()
                    .justify_between()
                    .text_size(px(BODY_SIZE))
                    .line_height(px(BODY_LINE))
                    .child(display_label(selected))
                    .child(
                        svg()
                            .path(icons::CHEVRON_DOWN)
                            .size(px(12.0))
                            .text_color(palette.muted),
                    ),
            )
            .cursor_pointer()
            .dropdown_menu(move |mut menu, _, _| {
                menu = menu.min_w(px(WIDTH)).max_w(px(WIDTH));
                for target in DisplayTarget::ALL {
                    menu = menu.item(
                        PopupMenuItem::element(move |_, cx| {
                            let palette = crate::theme::settings_palette(cx);
                            h_flex()
                                .flex_1()
                                .min_w_0()
                                .h(px(28.0))
                                .mx_neg_2()
                                .px_2()
                                .items_center()
                                .rounded(px(RADIUS_SM))
                                .cursor_pointer()
                                .text_size(px(BODY_SIZE))
                                .line_height(px(BODY_LINE))
                                .when(target == selected, |row| row.bg(palette.selected))
                                .child(display_label(target))
                        })
                        .on_click(move |_, _, cx| {
                            let mut next = preferences(cx).clone();
                            next.general.display = target;
                            save(next, cx);
                        }),
                    );
                }
                menu
            });
        page(self.page)
            .child(
                group(text::LAUNCHER_GROUP).child(
                    card(cx)
                        .child(self.toggle_row(Toggle::Login, cx))
                        .child(components::separator(cx))
                        .child(setting_row(
                            text::DISPLAY,
                            text::DISPLAY_HELP,
                            display_choices,
                            cx,
                        )),
                ),
            )
            .child(
                group(text::HISTORY).child(
                    card(cx)
                        .child(self.toggle_row(Toggle::Recents, cx))
                        .child(components::separator(cx))
                        .child(setting_row(
                            text::CLEAR_HISTORY,
                            text::CLEAR_HISTORY_HELP,
                            div()
                                .id("clear-history")
                                .h(px(CONTROL_HEIGHT))
                                .px_3()
                                .flex()
                                .items_center()
                                .rounded(px(RADIUS_MD))
                                .border_1()
                                .border_color(palette.border)
                                .cursor_pointer()
                                .hover(move |button| button.bg(palette.hover))
                                .child(text::CLEAR_HISTORY)
                                .on_click(|_, _, cx| crate::settings::clear_history(cx)),
                            cx,
                        )),
                ),
            )
            .child(
                group(text::UPDATES_GROUP).child(
                    card(cx)
                        .child(self.toggle_row(Toggle::CheckUpdates, cx))
                        .child(components::separator(cx))
                        .child(self.toggle_row(Toggle::DownloadUpdates, cx)),
                ),
            )
            .into_any_element()
    }

    pub(super) fn preferences_location(&self, cx: &Context<Self>) -> impl IntoElement {
        let palette = crate::theme::settings_palette(cx);
        card(cx).child(setting_row(
            text::PREFERENCES_FILE,
            text::SETTINGS_LOCATION_HELP,
            h_flex()
                .id("preferences-folder")
                .h(px(CONTROL_HEIGHT))
                .px_2()
                .rounded(px(RADIUS_MD))
                .gap_1()
                .text_size(px(CAPTION_SIZE))
                .text_color(palette.muted)
                .cursor_pointer()
                .hover(move |link| link.bg(palette.hover).text_color(palette.foreground))
                .child(div().underline().child(text::SETTINGS_FILENAME))
                .child(
                    svg()
                        .path(icons::EXTERNAL_LINK)
                        .size(px(12.0))
                        .text_color(palette.muted),
                )
                .on_click(|_, _, cx| crate::settings::open_preferences_folder(cx)),
            cx,
        ))
    }
}

fn display_label(target: DisplayTarget) -> &'static str {
    match target {
        DisplayTarget::Pointer => text::DISPLAY_POINTER,
        DisplayTarget::ActiveWindow => text::DISPLAY_ACTIVE,
        DisplayTarget::Primary => text::DISPLAY_PRIMARY,
    }
}
