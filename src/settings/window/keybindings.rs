use super::*;

impl SettingsView {
    pub(super) fn keybindings_page(&self, cx: &Context<Self>) -> gpui::AnyElement {
        let palette = crate::theme::settings_palette(cx);
        let mut body = page(self.page);
        for (group, actions) in [
            (text::SYSTEM_GROUP, &Shortcut::ALL[..1]),
            (text::LAUNCHER_GROUP, &Shortcut::ALL[1..]),
        ] {
            let mut rows = card(cx);
            for (index, action) in actions.iter().copied().enumerate() {
                if index > 0 {
                    rows = rows.child(components::separator(cx));
                }
                rows = rows.child(self.shortcut_row(action, cx));
            }
            body = body.child(div().mt_2().child(section(group))).child(rows);
        }
        body.child(
            div()
                .text_size(px(12.0))
                .text_color(palette.muted)
                .child(text::KEYBINDINGS_DESCRIPTION),
        )
        .into_any_element()
    }

    fn shortcut_row(&self, action: Shortcut, cx: &Context<Self>) -> impl IntoElement {
        let palette = crate::theme::settings_palette(cx);
        let recording = self.recording == Some(action);
        let detail = if recording {
            text::RECORDING_HELP
        } else if action == Shortcut::ToggleLauncher {
            text::SYSTEM_SCOPE
        } else {
            text::LAUNCHER_SCOPE
        };
        div()
            .flex()
            .flex_col()
            .child(
                h_flex()
                    .min_h(px(72.0))
                    .px_4()
                    .py_3()
                    .gap_3()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(text::shortcut_label(action)),
                            )
                            .child(description(detail, cx)),
                    )
                    .child(
                        div()
                            .id(action.id())
                            .px_2()
                            .py_2()
                            .rounded(px(6.0))
                            .cursor_pointer()
                            .when(recording, |slot| {
                                slot.bg(palette.hover)
                                    .border_1()
                                    .border_color(palette.accent)
                            })
                            .hover(move |slot| slot.bg(palette.hover))
                            .child(if recording {
                                div()
                                    .text_size(px(12.0))
                                    .text_color(palette.accent)
                                    .child(text::PRESS_SHORTCUT)
                                    .into_any_element()
                            } else {
                                key_badges(preferences(cx).binding(action), cx).into_any_element()
                            })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.start_recording(action, window, cx)
                            })),
                    )
                    .child(
                        div()
                            .id(("reset", action as usize))
                            .p_2()
                            .rounded(px(5.0))
                            .cursor_pointer()
                            .text_size(px(11.0))
                            .text_color(palette.muted)
                            .hover(move |button| {
                                button.bg(palette.hover).text_color(palette.foreground)
                            })
                            .child(text::RESET)
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.reset_binding(action, cx)),
                            ),
                    ),
            )
            .when(recording, |row| {
                row.when_some(self.validation.clone(), |row, error| {
                    row.child(
                        div()
                            .px_4()
                            .pb_3()
                            .text_size(px(12.0))
                            .text_color(palette.accent)
                            .child(error),
                    )
                })
            })
    }
}
