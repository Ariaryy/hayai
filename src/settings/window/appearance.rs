use super::previews::{launcher_preview, theme_swatch};
use super::*;
use crate::theme::ThemeFamily;

impl SettingsView {
    pub(super) fn appearance_page(&self, cx: &Context<Self>) -> gpui::AnyElement {
        let palette = crate::theme::settings_palette(cx);
        let mut choices = h_flex().gap_3().w_full();
        for (light, label) in [(true, text::LIGHT), (false, text::DARK)] {
            let selected = preferences(cx).light == light;
            choices = choices.child(
                div()
                    .id(label)
                    .flex_1()
                    .min_w_0()
                    .p_2()
                    .rounded(px(12.0))
                    .border_2()
                    .border_color(if selected {
                        palette.accent
                    } else {
                        palette.border
                    })
                    .bg(palette.panel)
                    .cursor_pointer()
                    .hover(move |card| card.bg(palette.hover))
                    .child(launcher_preview(preferences(cx).theme_for(light), light))
                    .child(
                        h_flex()
                            .h(px(34.0))
                            .justify_center()
                            .text_size(px(12.0))
                            .text_color(if selected {
                                palette.foreground
                            } else {
                                palette.muted
                            })
                            .font_weight(FontWeight::MEDIUM)
                            .child(label),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if cx.global::<SettingsGlobal>().saving {
                            return;
                        }
                        this.cancel_recording();
                        let mut next = preferences(cx).clone();
                        next.light = light;
                        save(next, cx);
                        cx.notify();
                    })),
            );
        }
        page(self.page)
            .child(section(text::COLOR_SCHEME))
            .child(choices)
            .child(div().mt_5().child(section(text::THEMES)))
            .child(
                div()
                    .grid()
                    .grid_cols(3)
                    .gap_3()
                    .children(ThemeFamily::ALL.map(|family| self.theme_card(family, cx))),
            )
            .into_any_element()
    }

    fn theme_card(&self, family: ThemeFamily, cx: &Context<Self>) -> impl IntoElement {
        let palette = crate::theme::settings_palette(cx);
        div()
            .min_w_0()
            .p_3()
            .rounded(px(12.0))
            .border_1()
            .border_color(palette.border)
            .bg(palette.panel)
            .child(
                h_flex()
                    .justify_center()
                    .gap_3()
                    .child(self.theme_variant(family, true, cx))
                    .child(self.theme_variant(family, false, cx)),
            )
            .child(
                div()
                    .mt_3()
                    .font_weight(FontWeight::MEDIUM)
                    .child(text::theme_label(family)),
            )
    }

    fn theme_variant(
        &self,
        family: ThemeFamily,
        light: bool,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let palette = crate::theme::settings_palette(cx);
        let selected = preferences(cx).theme_for(light) == family;
        div()
            .id((family.id(), usize::from(light)))
            .p_1()
            .rounded(px(8.0))
            .cursor_pointer()
            .hover(move |control| control.bg(palette.hover))
            .child(theme_swatch(family, light, selected, cx))
            .child(
                h_flex()
                    .mt_2()
                    .justify_center()
                    .gap_1()
                    .text_size(px(10.0))
                    .text_color(palette.muted)
                    .child(
                        svg()
                            .path(if light { icons::SUN } else { icons::MOON })
                            .size(px(10.0))
                            .text_color(palette.muted),
                    )
                    .child(if light { text::LIGHT } else { text::DARK }),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                if cx.global::<SettingsGlobal>().saving {
                    return;
                }
                this.cancel_recording();
                let mut next = preferences(cx).clone();
                next.set_theme_for(light, family);
                save(next, cx);
                cx.notify();
            }))
    }
}
