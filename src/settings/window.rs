use super::{SettingsGlobal, Shortcut, display_preferences as preferences, save};
use crate::{assets::icons, ui_text::settings as text};
use gpui::prelude::*;
use gpui::{
    App, Bounds, Context, FocusHandle, Focusable, FontWeight, IntoElement, Subscription, Window,
    WindowBounds, WindowOptions, div, px, size, svg,
};
use gpui_component::{Root, TitleBar, h_flex};

mod appearance;
mod commands;
mod components;
mod file_search;
mod general;
mod keybindings;
mod licenses;
mod lifecycle;
mod pages;
mod previews;
mod recording;
mod scrollbar;
mod scrolling;
#[cfg(test)]
mod tests;

use components::{
    BODY_LINE, BODY_SIZE, CAPTION_LINE, CAPTION_SIZE, CONTROL_HEIGHT, RADIUS_LG, RADIUS_MD,
    RADIUS_SM, RADIUS_XL, ROW_MIN_HEIGHT, card, description, group, page, section, setting_row,
};
pub use components::{key_badges, keycap_icon, keycap_label, trigger_badge};
pub use lifecycle::open;
use pages::Page;
pub use recording::capture_registered_shortcut;

pub(super) struct SettingsView {
    brand: Option<std::sync::Arc<gpui::RenderImage>>,
    page: Page,
    focus: FocusHandle,
    recording: Option<Shortcut>,
    validation: Option<String>,
    expanded_notice: Option<usize>,
    alias_editor: Option<commands::AliasEditor>,
    scrolling: scrolling::SettingsScroll,
    _subscriptions: Vec<Subscription>,
}

impl SettingsView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        let subscriptions = Self::subscriptions(window, cx);
        let view = cx.entity().downgrade();
        window.on_window_should_close(cx, move |_, cx| {
            view.update(cx, |view, cx| view.commit_aliases(cx))
                .unwrap_or(true)
        });
        Self {
            brand: crate::launcher::load_builtin_icon(icons::BRAND_PNG),
            page: Page::General,
            focus,
            recording: None,
            validation: None,
            expanded_notice: None,
            alias_editor: None,
            scrolling: Default::default(),
            _subscriptions: subscriptions,
        }
    }

    /// Ancestors are links; the current page is plain text. Settings opens
    /// the first page, and Third-party notices sits under About.
    fn breadcrumb(&self, cx: &Context<Self>) -> impl IntoElement {
        let palette = crate::theme::settings_palette(cx);
        let mut trail = vec![(text::SETTINGS_ITEM, Some(Page::General))];
        if self.page == Page::Licenses {
            trail.push((Page::About.label(), Some(Page::About)));
        }
        let current = self.page.label();
        let mut crumbs = h_flex().px_8().gap_3().text_size(px(BODY_SIZE));
        for (index, (label, target)) in trail.into_iter().enumerate() {
            crumbs = crumbs
                .child(
                    div()
                        .id(("crumb", index))
                        .text_color(palette.muted)
                        .when_some(target, |crumb, page| {
                            crumb
                                .cursor_pointer()
                                .hover(move |crumb| crumb.text_color(palette.foreground))
                                // The titlebar drags the window; a crumb is a click.
                                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                    cx.stop_propagation()
                                })
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.navigate(page, window, cx);
                                }))
                        })
                        .child(label),
                )
                .child(div().text_color(palette.muted).child("/"));
        }
        crumbs.child(div().font_weight(FontWeight::SEMIBOLD).child(current))
    }

    fn navigation(&self, page: Page, cx: &Context<Self>) -> impl IntoElement {
        let palette = crate::theme::settings_palette(cx);
        let selected = self.page == page || (page == Page::About && self.page == Page::Licenses);
        let color = if selected {
            palette.foreground
        } else {
            palette.muted
        };
        let line = crate::pixels::snap(cx, BODY_LINE);
        let (icon, icon_top) = crate::pixels::fit(cx, 36.0, 0.0, 16.0);
        h_flex()
            .id(page.label())
            .h(crate::pixels::snap(cx, 36.0))
            .items_start()
            .px_3()
            .gap_3()
            .rounded(px(RADIUS_LG))
            .text_color(color)
            .font_weight(if selected {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::MEDIUM
            })
            .cursor_pointer()
            .when(selected, |row| row.bg(palette.sidebar_selected))
            .hover(move |row| row.bg(palette.sidebar_hover))
            .child(
                svg()
                    .path(page.icon())
                    .size(icon)
                    .flex_none()
                    .mt(icon_top)
                    .text_color(color),
            )
            .child(
                div()
                    .mt(crate::pixels::text_top(
                        cx,
                        36.0,
                        0.0,
                        BODY_LINE,
                        BODY_SIZE,
                        crate::pixels::Glyphs::Lowercase,
                    ))
                    .h(line)
                    .line_height(line)
                    .child(page.label()),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.navigate(page, window, cx);
            }))
    }
}

impl Focusable for SettingsView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crate::pixels::sync(window, cx);
        if self.scrolling.advance(window.is_window_active()) {
            window.request_animation_frame();
        }
        let palette = crate::theme::settings_palette(cx);
        let state = cx.global::<SettingsGlobal>();
        let status = state.error.clone().or_else(|| {
            if self.recording.is_none() {
                self.validation.clone()
            } else {
                None
            }
        });
        div()
            .size_full()
            .flex()
            .flex_col()
            .track_focus(&self.focus)
            .font_family("Segoe UI")
            .text_size(px(13.0))
            .bg(palette.canvas)
            .text_color(palette.foreground)
            .child(
                TitleBar::new()
                    .on_close_window(cx.listener(|view, _, window, cx| {
                        if view.commit_aliases(cx) {
                            window.remove_window();
                        }
                    }))
                    .h(px(48.0))
                    .pl_0()
                    .bg(palette.canvas)
                    .border_color(palette.border)
                    .child(
                        h_flex()
                            .h_full()
                            .flex_1()
                            .min_w_0()
                            .child(
                                h_flex()
                                    .w(px(216.0))
                                    .h_full()
                                    .flex_none()
                                    .px_5()
                                    .gap_2()
                                    .bg(palette.sidebar)
                                    .border_r_1()
                                    .border_color(palette.border)
                                    .when_some(self.brand.clone(), |row, brand| {
                                        row.child(gpui::img(brand).size(px(24.0)))
                                    })
                                    .child(
                                        div()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(crate::ui_text::headings::HAYAI),
                                    ),
                            )
                            .child(self.breadcrumb(cx)),
                    ),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .items_stretch()
                    .child(
                        div()
                            .w(px(216.0))
                            .flex_none()
                            .flex()
                            .flex_col()
                            .p_2()
                            .gap_1()
                            .bg(palette.sidebar)
                            .border_r_1()
                            .border_color(palette.border)
                            .children(
                                [
                                    Page::General,
                                    Page::Appearance,
                                    Page::KeyBindings,
                                    Page::Commands,
                                    Page::FileSearch,
                                ]
                                .map(|page| self.navigation(page, cx)),
                            )
                            .child(div().flex_1())
                            .child(self.navigation(Page::About, cx)),
                    )
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .id("settings-content")
                                    .track_scroll(&self.scrolling.handle)
                                    .flex_1()
                                    .min_h_0()
                                    .overflow_y_scroll()
                                    .px_8()
                                    .py_8()
                                    .child(
                                        div()
                                            .w_full()
                                            .max_w(px(800.0))
                                            .mx_auto()
                                            .when_some(status, |page, error| {
                                                page.child(
                                                    div()
                                                        .mb_4()
                                                        .text_size(px(12.0))
                                                        .text_color(palette.muted)
                                                        .child(error),
                                                )
                                            })
                                            .child(self.body(window, cx)),
                                    ),
                            )
                            .child(scrollbar::overlay(cx, &self.scrolling.handle)),
                    ),
            )
    }
}
