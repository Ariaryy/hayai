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
        .h(px(166.0))
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
                .h(px(21.0))
                .flex_none()
                .px_2()
                .gap_1()
                .border_t_1()
                .border_color(palette.border)
                .child(
                    div()
                        .w(px(27.0))
                        .h(px(3.0))
                        .rounded(px(2.0))
                        .bg(palette.muted.opacity(0.65)),
                )
                .child(skeleton_key(palette))
                .child(div().flex_1())
                .child(
                    div()
                        .w(px(20.0))
                        .h(px(3.0))
                        .rounded(px(2.0))
                        .bg(palette.muted.opacity(0.65)),
                )
                .child(skeleton_key(palette))
                .child(div().mx_1().w(px(1.0)).h(px(7.0)).bg(palette.border))
                .child(
                    div()
                        .w(px(22.0))
                        .h(px(3.0))
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
        .h(px(19.0))
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
        .h(px(7.0))
        .rounded(px(2.0))
        .bg(palette.selected)
}

/// Show actual palette roles, rather than a gradient that invents colors.
/// The control's label sits outside this shape so nothing overlaps its corners.
pub(super) fn theme_swatch(
    family: ThemeFamily,
    light: bool,
    selected: bool,
    cx: &App,
) -> impl IntoElement {
    let palette = crate::theme::launcher_palette(family, light);
    let active = crate::theme::settings_palette(cx);
    div()
        .size(px(56.0))
        .p(px(3.0))
        .rounded(px(17.0))
        .border_1()
        .border_color(if selected {
            active.accent
        } else {
            active.border
        })
        .child(
            div()
                .size_full()
                .rounded(px(13.0))
                .border_1()
                .border_color(palette.border)
                .bg(palette.background)
                .flex()
                .flex_col()
                .justify_center()
                .px(px(6.0))
                .gap(px(5.0))
                .child(line(0.8, 3.0, palette.foreground.opacity(0.8)))
                .child(
                    div()
                        .h(px(10.0))
                        .px(px(3.0))
                        .rounded(px(3.0))
                        .bg(palette.selected)
                        .flex()
                        .items_center()
                        .child(line(0.7, 3.0, palette.accent)),
                )
                .child(line(0.6, 3.0, palette.muted)),
        )
}
