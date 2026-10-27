use super::*;
use crate::settings::FileSearch;

#[derive(Clone, Copy)]
enum Toggle {
    NoisyFolders,
    Subfolders,
    Recents,
    FoldersFirst,
}

impl Toggle {
    fn metadata(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::NoisyFolders => (
                "file-noisy-folders",
                text::HIDE_NOISY_FOLDERS,
                text::HIDE_NOISY_FOLDERS_HELP,
            ),
            Self::Subfolders => (
                "file-subfolders",
                text::SEARCH_SUBFOLDERS,
                text::SEARCH_SUBFOLDERS_HELP,
            ),
            Self::Recents => (
                "file-recents",
                text::SHOW_FILE_RECENTS,
                text::SHOW_FILE_RECENTS_HELP,
            ),
            Self::FoldersFirst => (
                "file-folders-first",
                text::FOLDERS_FIRST,
                text::FOLDERS_FIRST_HELP,
            ),
        }
    }
    fn value(self, options: &FileSearch) -> bool {
        match self {
            Self::NoisyFolders => options.hide_noisy_folders,
            Self::Subfolders => options.search_subfolders,
            Self::Recents => options.show_recents,
            Self::FoldersFirst => options.folders_first,
        }
    }
    fn set(self, options: &mut FileSearch, value: bool) {
        match self {
            Self::NoisyFolders => options.hide_noisy_folders = value,
            Self::Subfolders => options.search_subfolders = value,
            Self::Recents => options.show_recents = value,
            Self::FoldersFirst => options.folders_first = value,
        }
    }
}

impl SettingsView {
    fn file_search_toggle(&self, setting: Toggle, cx: &Context<Self>) -> gpui::Div {
        let (id, title, help) = setting.metadata();
        setting_row(
            title,
            help,
            components::ToggleSwitch::new(id, setting.value(&preferences(cx).file_search))
                .on_change(move |enabled, _, cx| {
                    let mut next = preferences(cx).clone();
                    setting.set(&mut next.file_search, enabled);
                    save(next, cx);
                }),
            cx,
        )
    }

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
                group(text::FILE_FILTERING).child(
                    card(cx)
                        .child(self.file_search_toggle(Toggle::NoisyFolders, cx))
                        .child(components::separator(cx))
                        .child(self.file_search_toggle(Toggle::Subfolders, cx)),
                ),
            )
            .child(
                group(text::FILE_BROWSING).child(
                    card(cx)
                        .child(self.file_search_toggle(Toggle::Recents, cx))
                        .child(components::separator(cx))
                        .child(self.file_search_toggle(Toggle::FoldersFirst, cx)),
                ),
            )
            .into_any_element()
    }
}
