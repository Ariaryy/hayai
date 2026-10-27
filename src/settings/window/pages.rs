use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Page {
    General,
    Appearance,
    KeyBindings,
    Commands,
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
            Self::Commands => text::COMMANDS,
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
            Self::Commands => icons::ARROW_RIGHT,
            Self::FileSearch => icons::SETTINGS_FILES,
            Self::About => icons::SETTINGS_ABOUT,
            Self::Licenses => icons::SETTINGS_ABOUT,
        }
    }
}

impl SettingsView {
    pub(super) fn file_search_page(&self, cx: &Context<Self>) -> gpui::AnyElement {
        let palette = crate::theme::settings_palette(cx);
        let (chevron, chevron_top) = crate::pixels::fit(cx, 24.0, 0.0, 12.0);
        let glyph = |path: &'static str| {
            svg()
                .path(path)
                .size(chevron)
                .flex_none()
                .mt(chevron_top)
                .text_color(palette.muted)
        };
        // Rows that lead somewhere end in a glyph and highlight under the pointer.
        // Rows fill the card interior. Only corners touching the outside of
        // the card are rounded; the shared separator stays straight.
        let link = |row: gpui::Div, id: &'static str| {
            row.id(id)
                .cursor_pointer()
                .hover(move |row| row.bg(palette.hover))
        };
        let engine = setting_row(
            text::SCRY_NAME,
            text::FILE_DESCRIPTION,
            h_flex()
                .h(crate::pixels::snap(cx, 24.0))
                .items_start()
                .px_2()
                .rounded(px(RADIUS_MD))
                .bg(palette.keycap)
                .text_size(px(CAPTION_SIZE))
                .child(
                    div()
                        .mt(crate::pixels::text_top(
                            cx,
                            24.0,
                            0.0,
                            18.0,
                            CAPTION_SIZE,
                            crate::pixels::Glyphs::Lowercase,
                        ))
                        .h(crate::pixels::snap(cx, 18.0))
                        .line_height(crate::pixels::snap(cx, 18.0))
                        .child(text::WINDOWS_INDEX),
                ),
            cx,
        );
        let guide = link(
            setting_row(
                text::GUIDE,
                text::FILE_GUIDE,
                h_flex()
                    .h(crate::pixels::snap(cx, 24.0))
                    .items_start()
                    .child(glyph(icons::EXTERNAL_LINK)),
                cx,
            )
            .rounded_b(px(components::CARD_INNER_RADIUS)),
            "file-search-guide",
        )
        .on_click(|_, _, cx| cx.open_url(crate::links::FILE_SEARCH_GUIDE));
        let shortcut = link(
            setting_row(
                text::SHORTCUT,
                text::SHORTCUT_HELP,
                h_flex()
                    .h(crate::pixels::snap(cx, 24.0))
                    .items_start()
                    .gap_2()
                    .child(
                        div()
                            .mt(crate::pixels::lead(cx, 24.0, 20.0))
                            .child(key_badges(
                                preferences(cx).binding(Shortcut::FileSearch),
                                cx,
                            )),
                    )
                    .child(glyph(icons::CARET_RIGHT)),
                cx,
            )
            .rounded_t(px(components::CARD_INNER_RADIUS)),
            "file-search-shortcut",
        )
        .on_click(cx.listener(|this, _, window, cx| {
            this.navigate(Page::KeyBindings, window, cx);
        }));
        let trigger = link(
            setting_row(
                text::COMMAND,
                text::COMMAND_HELP,
                h_flex()
                    .h(crate::pixels::snap(cx, 24.0))
                    .items_start()
                    .gap_2()
                    .child(
                        div().mt(crate::pixels::lead(cx, 24.0, 20.0)).child(
                            components::trigger_keys(
                                preferences(cx)
                                    .commands
                                    .trigger(crate::command_router::Command::Files),
                                cx,
                            ),
                        ),
                    )
                    .child(glyph(icons::CARET_RIGHT)),
                cx,
            )
            .rounded_b(px(components::CARD_INNER_RADIUS)),
            "file-search-trigger",
        )
        .on_click(cx.listener(|this, _, window, cx| {
            this.navigate(Page::Commands, window, cx);
        }));
        page(self.page)
            .child(
                group(text::SEARCH_ENGINE).child(
                    card(cx)
                        .child(engine)
                        .child(components::separator(cx))
                        .child(guide),
                ),
            )
            .child(
                group(text::OPEN_FILE_SEARCH).child(
                    card(cx)
                        .child(shortcut)
                        .child(components::separator(cx))
                        .child(trigger),
                ),
            )
            .into_any_element()
    }

    pub(super) fn about_page(&self, cx: &Context<Self>) -> gpui::AnyElement {
        let palette = crate::theme::settings_palette(cx);
        // Logo and text block share one height, so their edges line up.
        const HERO_HEIGHT: f32 = 80.0;
        let hero = h_flex()
            .p_6()
            .gap_5()
            .items_center()
            .when_some(self.brand.clone(), |body, brand| {
                body.child(gpui::img(brand).size(px(HERO_HEIGHT)).flex_none())
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h(px(HERO_HEIGHT))
                    .flex()
                    .flex_col()
                    .justify_between()
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(20.0))
                                    .line_height(px(28.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(crate::ui_text::headings::HAYAI),
                            )
                            // Centered on the title's line box, which sits a pixel
                            // above the text; nudge the badge down to the glyphs.
                            .child(div().mt(px(2.0)).child(components::keycap_label(
                                crate::ui_text::about::version_tag(env!("CARGO_PKG_VERSION")),
                                20.0,
                                cx,
                            ))),
                    )
                    .child(description(crate::ui_text::about::DESCRIPTION, cx))
                    .child(div().child(components::external_link(
                        "project",
                        crate::ui_text::about::PROJECT_LINK,
                        crate::links::PROJECT,
                        cx,
                    ))),
            );
        let notices = setting_row(
            text::THIRD_PARTY_NOTICES,
            text::NOTICES_HELP,
            h_flex()
                .gap_2()
                .text_size(px(CAPTION_SIZE))
                .text_color(palette.muted)
                .child(text::notice_count(crate::licenses::NOTICES.len()))
                .child(
                    svg()
                        .path(icons::CARET_RIGHT)
                        .size(px(12.0))
                        .text_color(palette.muted),
                ),
            cx,
        )
        .id("third-party-notices")
        .rounded(px(components::CARD_INNER_RADIUS))
        .cursor_pointer()
        .hover(move |row| row.bg(palette.hover))
        .on_click(cx.listener(|this, _, window, cx| {
            this.navigate(Page::Licenses, window, cx);
        }));
        group(text::ABOUT)
            .child(card(cx).child(hero))
            .child(self.preferences_location(cx))
            .child(card(cx).child(notices))
            .into_any_element()
    }

    pub(super) fn body(&self, window: &Window, cx: &Context<Self>) -> gpui::AnyElement {
        match self.page {
            Page::General => self.general_page(cx),
            Page::Appearance => self.appearance_page(window, cx),
            Page::KeyBindings => self.keybindings_page(cx),
            Page::Commands => self.commands_page(cx),
            Page::FileSearch => self.file_search_page(cx),
            Page::About => self.about_page(cx),
            Page::Licenses => self.licenses_page(cx),
        }
    }
}
