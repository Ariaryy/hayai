use super::*;

pub(super) fn description(text: &str, cx: &App) -> impl IntoElement {
    div()
        .text_size(px(12.0))
        .line_height(px(19.0))
        .text_color(crate::theme::settings_palette(cx).muted)
        .child(text.to_owned())
}

pub(super) fn section(title: &'static str) -> impl IntoElement {
    div()
        .px_1()
        .text_size(px(15.0))
        .font_weight(FontWeight::SEMIBOLD)
        .child(title)
}

pub(super) fn card(cx: &App) -> gpui::Div {
    let palette = crate::theme::settings_palette(cx);
    div()
        .w_full()
        .flex()
        .flex_col()
        .rounded(px(12.0))
        .overflow_hidden()
        .border_1()
        .border_color(palette.border)
        .bg(palette.panel)
}

pub(super) fn setting_row(
    title: &'static str,
    detail: &'static str,
    control: impl IntoElement,
    cx: &App,
) -> gpui::Div {
    h_flex()
        .min_h(px(76.0))
        .px_4()
        .py_4()
        .gap_5()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child(title),
                )
                .child(description(detail, cx)),
        )
        .child(div().flex_none().child(control))
}

pub(super) fn separator(cx: &App) -> impl IntoElement {
    div()
        .h(px(1.0))
        .w_full()
        .bg(crate::theme::settings_palette(cx).border.opacity(0.6))
}

pub(super) fn external_link(
    id: &'static str,
    label: &'static str,
    url: &'static str,
    cx: &App,
) -> impl IntoElement + use<> {
    let palette = crate::theme::settings_palette(cx);
    h_flex()
        .id(id)
        .gap_1()
        .text_size(px(12.0))
        .text_color(palette.muted)
        .cursor_pointer()
        .hover(move |link| link.text_color(palette.foreground))
        .child(div().underline().child(label))
        .child(
            svg()
                .path(icons::EXTERNAL_LINK)
                .size(px(12.0))
                .text_color(palette.muted),
        )
        .on_click(move |_, _, cx| cx.open_url(url))
}

pub fn key_badges(binding: &str, cx: &App) -> impl IntoElement + use<> {
    h_flex().gap_1().children(
        binding
            .split('-')
            .map(|key| key_badge(key, cx))
            .collect::<Vec<_>>(),
    )
}

/// A trigger is typed text, so a multi-key one is a single badge.
pub fn trigger_badge(trigger: &str, cx: &App) -> impl IntoElement + use<> {
    key_badge(trigger, cx)
}

fn warning_glyph(
    id: impl Into<gpui::ElementId>,
    message: String,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    let message = gpui::SharedString::from(message);
    div()
        .id(id)
        .flex_none()
        .size(px(16.0))
        .child(
            svg()
                .path(icons::WARNING)
                .size(px(16.0))
                .text_color(crate::theme::settings_palette(cx).warning),
        )
        .tooltip(move |window, cx| {
            let message = message.clone();
            // Wrap long explanations instead of stretching across the window.
            gpui_component::tooltip::Tooltip::element(move |_, _| {
                div().max_w(px(260.0)).child(message.clone())
            })
            .build(window, cx)
        })
}

/// A warning glyph that always sits at the leading edge of what it flags. The
/// slot keeps its width when empty so rows stay aligned whether or not a
/// conflict exists.
pub(super) fn conflict_slot(
    id: impl Into<gpui::ElementId>,
    message: Option<String>,
    cx: &App,
) -> impl IntoElement {
    div()
        .flex_none()
        .size(px(16.0))
        .when_some(message, |slot, message| {
            slot.child(warning_glyph(id, message, cx))
        })
}

/// The same glyph for items that are not reserved a slot, such as alias chips.
pub(super) fn conflict_glyph(
    id: impl Into<gpui::ElementId>,
    message: String,
    cx: &App,
) -> impl IntoElement {
    warning_glyph(id, message, cx)
}

pub(super) fn key_badge(key: &str, cx: &App) -> impl IntoElement + use<> {
    let palette = crate::theme::settings_palette(cx);
    div()
        .h(px(22.0))
        .min_w(px(22.0))
        .px(px(6.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(4.0))
        .bg(palette.keycap)
        .text_color(palette.foreground)
        .text_size(px(11.0))
        .font_weight(FontWeight::MEDIUM)
        .child(match key {
            "ctrl" => "Ctrl".into(),
            "alt" => "Alt".into(),
            "shift" => "Shift".into(),
            "super" | "cmd" | "win" => "Win".into(),
            "space" => "Space".into(),
            key => key.to_uppercase(),
        })
}

pub(super) fn page(_: Page) -> gpui::Div {
    div().w_full().flex().flex_col().gap_5()
}
