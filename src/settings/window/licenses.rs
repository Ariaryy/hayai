use super::*;
use crate::licenses::{NOTICES, Notice};

impl SettingsView {
    pub(super) fn licenses_page(&self, cx: &Context<Self>) -> gpui::AnyElement {
        let palette = crate::theme::settings_palette(cx);
        page(self.page)
            .child(
                h_flex()
                    .justify_between()
                    .gap_4()
                    .child(section(text::THIRD_PARTY_NOTICES))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(palette.muted)
                            .child(text::notice_count(NOTICES.len())),
                    ),
            )
            .child(
                card(cx).p_1().children(
                    NOTICES
                        .iter()
                        .enumerate()
                        .map(|(index, notice)| self.license_row(index, notice, cx)),
                ),
            )
            .into_any_element()
    }

    fn license_row(
        &self,
        index: usize,
        notice: &'static Notice,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let palette = crate::theme::settings_palette(cx);
        let expanded = self.expanded_notice == Some(index);
        div()
            .flex()
            .flex_col()
            .child(
                h_flex()
                    .id(("license", index))
                    .min_h(px(42.0))
                    .px_3()
                    .py_2()
                    .gap_3()
                    .rounded(px(8.0))
                    .cursor_pointer()
                    .hover(move |row| row.bg(palette.hover))
                    .child(
                        svg()
                            .path(if expanded {
                                icons::CARET_DOWN
                            } else {
                                icons::CARET_RIGHT
                            })
                            .size(px(12.0))
                            .flex_none()
                            .text_color(palette.muted),
                    )
                    .child(
                        h_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_2()
                            .child(div().font_weight(FontWeight::MEDIUM).child(notice.name))
                            .when_some(notice.version, |row, version| {
                                row.child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(palette.muted)
                                        .child(version),
                                )
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(palette.muted)
                            .child(format!("{} · {}", notice.license_id, notice.category)),
                    )
                    .child(
                        div()
                            .id(("license-source", index))
                            .p_1()
                            .rounded(px(4.0))
                            .cursor_pointer()
                            .hover(move |source| source.bg(palette.selected))
                            .child(
                                svg()
                                    .path(icons::EXTERNAL_LINK)
                                    .size(px(13.0))
                                    .text_color(palette.muted),
                            )
                            .on_click(move |_, _, cx| {
                                cx.stop_propagation();
                                cx.open_url(notice.source);
                            }),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.scrolling.cancel();
                        this.expanded_notice = if expanded { None } else { Some(index) };
                        cx.notify();
                    })),
            )
            .when(expanded, |row| {
                row.child(
                    div()
                        .pl(px(36.0))
                        .pr_4()
                        .py_4()
                        .flex()
                        .flex_col()
                        .gap_4()
                        .child(description(notice.attribution, cx))
                        .child(
                            div()
                                .font_family("Consolas")
                                .text_size(px(11.0))
                                .line_height(px(18.0))
                                .text_color(palette.muted)
                                .child(notice.license_text.trim()),
                        ),
                )
            })
    }
}
