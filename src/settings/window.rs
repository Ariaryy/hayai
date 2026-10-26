use super::{SettingsGlobal, Shortcut, display_preferences as preferences, save};
use crate::{assets::icons, ui_text::settings as text};
use gpui::prelude::*;
use gpui::{
    App, Bounds, Context, FocusHandle, Focusable, FontWeight, IntoElement, Subscription, Window,
    WindowBounds, WindowOptions, div, px, size, svg,
};
use gpui_component::{Root, TitleBar, h_flex};

mod appearance;
mod components;
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

pub use components::key_badges;
use components::{card, description, page, section, setting_row};
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
    scrolling: scrolling::SettingsScroll,
    _subscriptions: Vec<Subscription>,
}

impl SettingsView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        let subscriptions = Self::subscriptions(window, cx);
        Self {
            brand: crate::launcher::load_builtin_icon(icons::BRAND_PNG),
            page: Page::General,
            focus,
            recording: None,
            validation: None,
            expanded_notice: None,
            scrolling: Default::default(),
            _subscriptions: subscriptions,
        }
    }

    fn navigation(&self, page: Page, cx: &Context<Self>) -> impl IntoElement {
        let palette = crate::theme::settings_palette(cx);
        let selected = self.page == page || (page == Page::About && self.page == Page::Licenses);
        h_flex()
            .id(page.label())
            .h(px(36.0))
            .px_3()
            .gap_3()
            .rounded(px(7.0))
            .text_color(if selected {
                palette.foreground
            } else {
                palette.muted
            })
            .font_weight(if selected {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::MEDIUM
            })
            .cursor_pointer()
            .when(selected, |row| row.bg(palette.selected))
            .hover(move |row| row.bg(palette.hover))
            .child(
                svg()
                    .path(page.icon())
                    .size(px(17.0))
                    .text_color(if selected {
                        palette.foreground
                    } else {
                        palette.muted
                    }),
            )
            .child(page.label())
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
                    .h(px(46.0))
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
                                        row.child(gpui::img(brand).size(px(22.0)))
                                    })
                                    .child(
                                        div()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(crate::ui_text::headings::HAYAI),
                                    ),
                            )
                            .child(
                                h_flex()
                                    .px_6()
                                    .gap_3()
                                    .text_size(px(13.0))
                                    .child(
                                        div().text_color(palette.muted).child(text::SETTINGS_ITEM),
                                    )
                                    .child(div().text_color(palette.muted).child("/"))
                                    .child(
                                        div()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(self.page.label()),
                                    ),
                            ),
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
                            .px_2()
                            .py_4()
                            .gap_1()
                            .bg(palette.sidebar)
                            .border_r_1()
                            .border_color(palette.border)
                            .children(
                                [
                                    Page::General,
                                    Page::Appearance,
                                    Page::KeyBindings,
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
