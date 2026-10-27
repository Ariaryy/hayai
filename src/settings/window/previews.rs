use super::*;
use crate::theme::{LauncherPalette, ThemeFamily};

/// Skeleton of Hayai's actual launcher: search, grouped rows and footer.
/// Each preview uses its own palette, independent of the active appearance.
pub(super) fn launcher_preview(family: ThemeFamily, light: bool) -> impl IntoElement {
    let palette = crate::theme::launcher_palette(family, light);
    let heading = move || {
        div()
            .px_2()
            .mt_1()
            .mb_1()
            .child(line(0.2, 3.0, palette.muted.opacity(0.6)))
    };
    div()
        .h(px(168.0))
        .w_full()
        .flex()
        .flex_col()
        .rounded(px(7.0))
        .border_1()
        .border_color(palette.border)
        .overflow_hidden()
        .bg(palette.background)
        .child(
            div()
                .h(px(28.0))
                .flex_none()
                .px_3()
                .flex()
                .items_center()
                .border_b_1()
                .border_color(palette.border)
                .child(line(0.5, 4.0, palette.muted.opacity(0.65))),
        )
        .child(
            div()
                .flex_1()
                .min_h_0()
                .px_1()
                .child(heading())
                .child(skeleton_row(palette, true, 0.58))
                .child(heading())
                .children([0.36, 0.62, 0.45].map(|width| skeleton_row(palette, false, width))),
        )
        .child(
            h_flex()
                .h(px(20.0))
                .flex_none()
                .px_2()
                .gap_1()
                .border_t_1()
                .border_color(palette.border)
                .child(
                    div()
                        .w(px(28.0))
                        .h(px(4.0))
                        .rounded(px(2.0))
                        .bg(palette.muted.opacity(0.65)),
                )
                .child(skeleton_key(palette))
                .child(div().flex_1())
                .child(
                    div()
                        .w(px(20.0))
                        .h(px(4.0))
                        .rounded(px(2.0))
                        .bg(palette.muted.opacity(0.65)),
                )
                .child(skeleton_key(palette))
                .child(div().mx_1().w(px(1.0)).h(px(8.0)).bg(palette.border))
                .child(
                    div()
                        .w(px(24.0))
                        .h(px(4.0))
                        .rounded(px(2.0))
                        .bg(palette.muted.opacity(0.65)),
                )
                .child(skeleton_key(palette))
                .child(skeleton_key(palette)),
        )
}

fn line(width: f32, height: f32, color: gpui::Hsla) -> impl IntoElement {
    div()
        .w(gpui::relative(width))
        .h(px(height))
        .rounded(px(3.0))
        .bg(color)
}

fn skeleton_row(palette: LauncherPalette, selected: bool, width: f32) -> impl IntoElement {
    h_flex()
        .h(px(20.0))
        .px_2()
        .gap_2()
        .rounded(px(4.0))
        .when(selected, |row| row.bg(palette.selected))
        .child(div().size(px(8.0)).rounded(px(2.0)).bg(if selected {
            palette.accent
        } else {
            palette.muted.opacity(0.5)
        }))
        .child(line(
            width,
            4.0,
            palette
                .foreground
                .opacity(if selected { 0.75 } else { 0.45 }),
        ))
}

fn skeleton_key(palette: LauncherPalette) -> impl IntoElement {
    div()
        .w(px(8.0))
        .h(px(8.0))
        .rounded(px(2.0))
        .bg(palette.selected)
}

/// The badge is a sibling of the bordered image, painted afterward and fully
/// inside a larger hit target. Its opaque backing erases the outline beneath
/// it; no negative bounds or rectangular hover fill can clip or cross the icon.
pub(super) fn theme_swatch(
    family: ThemeFamily,
    light: bool,
    selected: bool,
    cx: &App,
) -> impl IntoElement {
    let active = crate::theme::settings_palette(cx);
    let colors = crate::theme::swatch_colors(family, light);
    let outer_radius = 14.0;
    let inner_radius = outer_radius - 3.0 - if selected { 2.0 } else { 1.0 };
    div()
        .relative()
        .w_full()
        .h(px(84.0))
        .px_1()
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .id((
                    if light { "light-swatch" } else { "dark-swatch" },
                    family as usize,
                ))
                .w_full()
                .h(px(72.0))
                .p(px(3.0))
                .rounded(px(outer_radius))
                .border_1()
                .when(selected, |image| image.border_2())
                .border_color(if selected {
                    active.accent
                } else {
                    active.border
                })
                .hover(move |image| image.border_color(active.foreground))
                .child(
                    div()
                        .size_full()
                        .rounded(px(inner_radius))
                        .bg(gpui::linear_gradient(
                            135.0,
                            gpui::linear_color_stop(colors.highlight, 0.0),
                            gpui::linear_color_stop(colors.shadow, 1.0),
                        ))
                        .child(div().size_full().rounded(px(inner_radius)).bg(
                            gpui::linear_gradient(
                                20.0,
                                gpui::linear_color_stop(colors.base.opacity(0.0), 0.0),
                                gpui::linear_color_stop(colors.base.opacity(0.45), 1.0),
                            ),
                        )),
                ),
        )
        .child(
            div()
                .absolute()
                .right(crate::pixels::hairline(cx))
                .bottom(crate::pixels::hairline(cx))
                .size(crate::pixels::snap(cx, 24.0))
                .p(crate::pixels::snap(cx, 4.0))
                .rounded_full()
                .bg(active.panel)
                .child(
                    div()
                        .size_full()
                        .rounded_full()
                        .border(crate::pixels::hairline(cx))
                        .border_color(if selected {
                            active.accent
                        } else {
                            active.border
                        })
                        .bg(active.canvas)
                        .flex()
                        .items_start()
                        .child({
                            // The 16px disc holds a 12px glyph; center it on whole pixels.
                            let (size, inset) = crate::pixels::fit(cx, 16.0, 1.0, 12.0);
                            svg()
                                .path(if light { icons::SUN } else { icons::MOON })
                                .size(size)
                                .flex_none()
                                .ml(inset)
                                .mt(inset)
                                .text_color(active.foreground)
                        }),
                ),
        )
}
