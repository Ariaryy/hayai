use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Page {
    General,
    Appearance,
    KeyBindings,
    FileSearch,
    About,
    Licenses,
}

impl Page {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::General => text::GENERAL,
            Self::Appearance => text::APPEARANCE,
            Self::KeyBindings => text::KEYBINDINGS,
            Self::FileSearch => text::FILE_SEARCH,
            Self::About => text::ABOUT,
            Self::Licenses => text::THIRD_PARTY_NOTICES,
        }
    }
    pub(super) fn icon(self) -> &'static str {
        match self {
            Self::General => icons::SETTINGS_GENERAL,
            Self::Appearance => icons::SETTINGS_APPEARANCE,
            Self::KeyBindings => icons::SETTINGS_KEYBOARD,
            Self::FileSearch => icons::SETTINGS_FILES,
            Self::About => icons::SETTINGS_ABOUT,
            Self::Licenses => icons::SETTINGS_ABOUT,
        }
    }
}

impl SettingsView {
    pub(super) fn general_page(&self, cx: &Context<Self>) -> gpui::AnyElement {
        let palette = crate::theme::settings_palette(cx);
        page(self.page)
            .child(section(text::LAUNCHER_GROUP))
            .child(
                card(cx)
                    .child(
                        div()
                            .id("general-shortcut")
                            .rounded_t(px(11.0))
                            .cursor_pointer()
                            .hover(move |row| row.bg(palette.hover))
                            .child(setting_row(
                                text::LAUNCHER_SHORTCUT,
                                text::LAUNCHER_SHORTCUT_HELP,
                                key_badges(preferences(cx).binding(Shortcut::ToggleLauncher), cx),
                                cx,
                            ))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.navigate(Page::KeyBindings, window, cx);
                            })),
                    )
                    .child(components::separator(cx))
                    .child(
                        div()
                            .id("general-appearance")
                            .rounded_b(px(11.0))
                            .cursor_pointer()
                            .hover(move |row| row.bg(palette.hover))
                            .child(setting_row(
                                text::APPEARANCE,
                                text::APPEARANCE_DESCRIPTION,
                                div()
                                    .text_color(palette.muted)
                                    .child(if preferences(cx).light {
                                        text::LIGHT
                                    } else {
                                        text::DARK
                                    }),
                                cx,
                            ))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.navigate(Page::Appearance, window, cx);
                            })),
                    ),
            )
            .child(div().mt_4().child(section(text::SETTINGS_LOCATION)))
            .child(
                card(cx).child(setting_row(
                    text::PREFERENCES_FILE,
                    text::SETTINGS_LOCATION_HELP,
                    h_flex()
                        .id("preferences-folder")
                        .px_2()
                        .py_1()
                        .rounded(px(4.0))
                        .gap_1()
                        .text_size(px(12.0))
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
                )),
            )
            .into_any_element()
    }

    pub(super) fn file_search_page(&self, cx: &Context<Self>) -> gpui::AnyElement {
        page(self.page)
            .child(section(text::SEARCH_ENGINE))
            .child(
                card(cx).child(setting_row(
                    text::SCRY_NAME,
                    text::FILE_DESCRIPTION,
                    div()
                        .text_size(px(12.0))
                        .text_color(crate::theme::settings_palette(cx).muted)
                        .child(text::WINDOWS_INDEX),
                    cx,
                )),
            )
            .child(div().mt_4().child(section(text::LAUNCHER_GROUP)))
            .child(card(cx).child(setting_row(
                text::FILE_SEARCH,
                text::FILE_SEARCH_HELP,
                key_badges(preferences(cx).binding(Shortcut::FileSearch), cx),
                cx,
            )))
            .child(components::external_link(
                "file-search-guide",
                text::FILE_GUIDE,
                crate::links::FILE_SEARCH_GUIDE,
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn about_page(&self, cx: &Context<Self>) -> gpui::AnyElement {
        page(self.page)
            .child(section(text::ABOUT))
            .child(
                card(cx).child(
                    div()
                        .p_6()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .when_some(self.brand.clone(), |body, brand| {
                            body.child(gpui::img(brand).size(px(64.0)))
                        })
                        .child(
                            div()
                                .text_size(px(22.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(crate::ui_text::headings::HAYAI),
                        )
                        .child(description(
                            &crate::ui_text::about::version(env!("CARGO_PKG_VERSION")),
                            cx,
                        ))
                        .child(description(crate::ui_text::about::DESCRIPTION, cx))
                        .child(components::external_link(
                            "project",
                            crate::ui_text::about::PROJECT_LINK,
                            crate::links::PROJECT,
                            cx,
                        )),
                ),
            )
            .child(
                div()
                    .id("third-party-notices")
                    .cursor_pointer()
                    .text_size(px(12.0))
                    .text_color(crate::theme::settings_palette(cx).muted)
                    .hover(|link| link.underline())
                    .child(text::THIRD_PARTY_NOTICES)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.navigate(Page::Licenses, window, cx);
                    })),
            )
            .into_any_element()
    }

    pub(super) fn body(&self, window: &Window, cx: &Context<Self>) -> gpui::AnyElement {
        match self.page {
            Page::General => self.general_page(cx),
            Page::Appearance => self.appearance_page(window, cx),
            Page::KeyBindings => self.keybindings_page(cx),
            Page::FileSearch => self.file_search_page(cx),
            Page::About => self.about_page(cx),
            Page::Licenses => self.licenses_page(cx),
        }
    }
}
