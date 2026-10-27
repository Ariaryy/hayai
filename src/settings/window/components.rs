use super::*;

// Shared scale for every settings page, on a 4px grid. Radii step from inline
// glyph backgrounds (SM) through controls (MD) and list rows (LG) to cards (XL).
pub(super) const RADIUS_SM: f32 = 4.0;
pub(super) const RADIUS_MD: f32 = 6.0;
pub(super) const RADIUS_LG: f32 = 8.0;
pub(super) const RADIUS_XL: f32 = 12.0;
/// Rows meet the inside edge of `card`'s one-pixel border.
pub(super) const CARD_INNER_RADIUS: f32 = RADIUS_XL - 1.0;
/// Buttons and dropdowns. Text inputs keep the component's own small height.
pub(super) const CONTROL_HEIGHT: f32 = 28.0;
/// Keycaps in settings; the launcher footer uses a shorter cap.
const KEYCAP_HEIGHT: f32 = 20.0;
pub(super) const ROW_MIN_HEIGHT: f32 = 72.0;
pub(super) const CAPTION_SIZE: f32 = 12.0;
pub(super) const CAPTION_LINE: f32 = 20.0;
pub(super) const BODY_SIZE: f32 = 13.0;
pub(super) const BODY_LINE: f32 = 20.0;

pub(super) fn description(text: &str, cx: &App) -> impl IntoElement {
    div()
        .text_size(px(CAPTION_SIZE))
        .line_height(px(CAPTION_LINE))
        .text_color(crate::theme::settings_palette(cx).muted)
        .child(text.to_owned())
}

pub(super) fn section(title: &'static str) -> impl IntoElement {
    div()
        .text_size(px(15.0))
        .line_height(px(24.0))
        .font_weight(FontWeight::SEMIBOLD)
        .child(title)
}

/// A heading with the cards it introduces. Keeping them in one container lets
/// the page space groups apart while each heading stays close to its content.
pub(super) fn group(title: &'static str) -> gpui::Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_3()
        .child(section(title))
}

pub(super) fn card(cx: &App) -> gpui::Div {
    let palette = crate::theme::settings_palette(cx);
    div()
        .w_full()
        .flex()
        .flex_col()
        .rounded(px(RADIUS_XL))
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
        .min_h(px(ROW_MIN_HEIGHT))
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
                        .text_size(px(BODY_SIZE))
                        .line_height(px(BODY_LINE))
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

const KEYCAP_TEXT: f32 = 11.0;
const KEYCAP_LINE: f32 = 14.0;

/// The one keycap every surface uses: a faint hairline border (one device
/// pixel, inside the fixed height) around centered content. (An inset shadow
/// would avoid the border box, but this GPUI revision's DirectX renderer does
/// not draw inset shadows; a drop shadow reads as padding around the cap.)
fn keycap_frame(height: f32, cx: &App) -> gpui::Div {
    let palette = crate::theme::settings_palette(cx);
    div()
        .h(crate::pixels::snap(cx, height))
        .min_w(crate::pixels::snap(cx, height))
        .px(crate::pixels::snap(cx, 8.0))
        .flex_none()
        .flex()
        .items_start()
        .justify_center()
        .rounded(px(RADIUS_SM))
        .bg(palette.keycap)
        .text_color(palette.foreground)
        .text_size(px(KEYCAP_TEXT))
        .font_weight(FontWeight::MEDIUM)
        .border(crate::pixels::hairline(cx))
        .border_color(palette.foreground.opacity(0.05))
}

/// A keycap showing text, with the glyphs centered by `pixels::text_top`.
pub fn keycap_label(label: impl Into<gpui::SharedString>, height: f32, cx: &App) -> gpui::Div {
    let label: gpui::SharedString = label.into();
    let top = crate::pixels::text_top(
        cx,
        height,
        1.0,
        KEYCAP_LINE,
        KEYCAP_TEXT,
        crate::pixels::Glyphs::of(&label),
    );
    keycap_frame(height, cx).child(
        div()
            .mt(top)
            .h(crate::pixels::snap(cx, KEYCAP_LINE))
            .line_height(crate::pixels::snap(cx, KEYCAP_LINE))
            .child(label),
    )
}

/// A keycap showing an icon, centered on whole device pixels.
pub fn keycap_icon(path: &'static str, size: f32, height: f32, cx: &App) -> gpui::Div {
    let color = crate::theme::settings_palette(cx).foreground;
    let (size, inset) = crate::pixels::fit(cx, height, 1.0, size);
    keycap_frame(height, cx).child(
        svg()
            .path(path)
            .size(size)
            .flex_none()
            .mt(inset)
            // Svg::paint needs its own color; it does not inherit the keycap's.
            .text_color(color),
    )
}

pub(super) fn key_badge(key: &str, cx: &App) -> impl IntoElement + use<> {
    let label: &str = match key {
        "ctrl" => "Ctrl",
        "alt" => "Alt",
        "shift" => "Shift",
        "super" | "cmd" | "win" => "Win",
        "space" => "Space",
        key => return keycap_label(key.to_uppercase(), KEYCAP_HEIGHT, cx),
    };
    keycap_label(label, KEYCAP_HEIGHT, cx)
}

/// An on/off switch. gpui-component's Switch insets its thumb with a 2px
/// border, which is 2.5 device pixels at 125% scaling, so the thumb lands a
/// pixel off-center. Track 24 and thumb 16 leave a 4px inset, which stays a
/// whole number of device pixels at 100%, 125%, 150% and 200%.
type ChangeHandler = std::rc::Rc<dyn Fn(bool, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub(super) struct ToggleSwitch {
    id: &'static str,
    checked: bool,
    disabled: bool,
    on_change: Option<ChangeHandler>,
}

impl ToggleSwitch {
    pub(super) fn new(id: &'static str, checked: bool) -> Self {
        Self {
            id,
            checked,
            disabled: false,
            on_change: None,
        }
    }

    pub(super) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub(super) fn on_change(
        mut self,
        handler: impl Fn(bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(std::rc::Rc::new(handler));
        self
    }
}

impl RenderOnce for ToggleSwitch {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        use gpui::{Animation, AnimationExt as _, ElementId};
        use gpui_component::ActiveTheme as _;
        // Track 40x24, thumb 16. The inset is whatever whole device pixels center
        // the thumb, the same on every side, so it never lands half a pixel off.
        let track_width = crate::pixels::snap(cx, 40.0);
        let track_height = crate::pixels::snap(cx, 24.0);
        let (thumb_size, inset) = crate::pixels::fit(cx, 24.0, 0.0, 16.0);
        let travel = track_width - thumb_size - inset * 2.0;
        let (checked, disabled, on_change) = (self.checked, self.disabled, self.on_change);
        let palette = crate::theme::settings_palette(cx);
        let theme = cx.theme();
        let (track, thumb) = (
            if checked {
                palette.accent
            } else {
                theme.switch
            },
            theme.switch_thumb,
        );
        // The thumb slides from the state last painted to the requested one.
        let painted = window.use_keyed_state(self.id, cx, |_, _| checked);
        let thumb = div()
            .size(thumb_size)
            .rounded(thumb_size / 2.0)
            .mt(inset)
            .ml(inset)
            .bg(thumb)
            .map(|thumb| {
                let duration = std::time::Duration::from_millis(150);
                if !disabled && *painted.read(cx) != checked {
                    cx.spawn({
                        let painted = painted.clone();
                        async move |cx| {
                            cx.background_executor().timer(duration).await;
                            painted.update(cx, |state, _| *state = checked);
                        }
                    })
                    .detach();
                    thumb
                        .with_animation(
                            ElementId::NamedInteger("slide".into(), checked as u64),
                            Animation::new(duration),
                            move |thumb, progress| {
                                thumb.left(travel * if checked { progress } else { 1.0 - progress })
                            },
                        )
                        .into_any_element()
                } else {
                    thumb
                        .left(if checked { travel } else { px(0.0) })
                        .into_any_element()
                }
            });
        div()
            .id(self.id)
            .w(track_width)
            .h(track_height)
            .flex_none()
            .flex()
            .items_start()
            .rounded(track_height / 2.0)
            .bg(track)
            .when(disabled, |track| track.opacity(0.5))
            .when_some(on_change.filter(|_| !disabled), |track, on_change| {
                track
                    .cursor_pointer()
                    .on_click(move |_, window, cx| on_change(!checked, window, cx))
            })
            .child(thumb)
    }
}

pub(super) fn page(_: Page) -> gpui::Div {
    div().w_full().flex().flex_col().gap_8()
}
