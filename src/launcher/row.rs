use super::*;

#[derive(gpui::IntoElement)]
pub(super) struct ResultRow {
    pub(super) loading: bool,
    pub(super) compact: bool,
    pub(super) icon_size: f32,
    pub(super) base: ListItem,
    pub(super) title: SharedString,
    pub(super) subtitle: Option<SharedString>,
    pub(super) file_size: Option<u64>,
    pub(super) calculation_detail: Option<CalculationDetail>,
    pub(super) icon: Option<Arc<RenderImage>>,
    pub(super) selected: bool,
    /// Calculator results (see `ResultListDelegate::render_item`) render as
    /// a distinct "question small / answer big" card rather than the plain
    /// icon+title+subtitle row every other provider uses.
    pub(super) calculator: bool,
}

impl ResultRow {
    pub(super) fn new(
        id: IndexPath,
        title: String,
        subtitle: Option<String>,
        calculation_detail: Option<CalculationDetail>,
        icon: Option<Arc<RenderImage>>,
        selected: bool,
        calculator: bool,
    ) -> Self {
        Self {
            loading: false,
            compact: false,
            icon_size: 22.0,
            base: ListItem::new(id).selected(selected),
            title: title.into(),
            subtitle: subtitle.map(Into::into),
            file_size: None,
            calculation_detail,
            icon,
            selected,
            calculator,
        }
    }
}

impl Selectable for ResultRow {
    fn selected(mut self, selected: bool) -> Self {
        self.base = self.base.selected(selected);
        self.selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl gpui::RenderOnce for ResultRow {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        // `ListItem` wraps whatever we give it in a plain (column-stacking)
        // `div`, so icon + name must be a single flex-row child, not two
        // separate children — otherwise they stack on top of each other.
        //
        // `rounded` here (rather than on the list itself) is what makes the
        // hover/selected fill read as an inset card: `ListItem` refines its
        // own style with ours before painting the highlight, so the radius
        // applies to that fill too. The list's own `.p_2()` provides the
        // inset on all four sides uniformly.
        let ResultRow {
            loading,
            compact,
            icon_size,
            base,
            title,
            subtitle,
            file_size,
            calculation_detail,
            icon,
            selected: _,
            calculator,
        } = self;
        let content: AnyElement = if calculator {
            ResultRow::render_calculator(title, subtitle, calculation_detail, cx)
        } else {
            ResultRow::render_plain(
                title,
                subtitle,
                file_size,
                icon,
                compact,
                (icon_size, loading),
                cx,
            )
        };
        if calculator {
            // Calculator results are presentation cards rather than hoverable list rows.
            // Disabling the wrapper suppresses ListItem's built-in hover fill; the card
            // supplies all of its own text colors, so the disabled text style cannot leak in.
            base.selected(false)
                .disabled(true)
                .px_0()
                .py_1()
                .child(content)
        } else {
            base.py_1p5().rounded(cx.theme().radius).child(content)
        }
    }
}

impl ResultRow {
    /// The plain icon + title + subtitle row every non-calculator provider
    /// uses.
    fn render_plain(
        title: SharedString,
        subtitle: Option<SharedString>,
        file_size: Option<u64>,
        icon: Option<Arc<RenderImage>>,
        compact: bool,
        icon_state: (f32, bool),
        cx: &mut App,
    ) -> AnyElement {
        let (icon_size, loading) = icon_state;
        h_flex()
            .when(compact, |row| row.h(px(24.0)))
            .items_center()
            .gap_3()
            .w_full()
            .min_w_0()
            .child(
                div()
                    .w(px(22.0))
                    .h(px(22.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(loading, |slot| {
                        slot.child(
                            gpui_component::spinner::Spinner::new()
                                .icon(gpui_component::Icon::default().path(icon_assets::SPINNER))
                                .with_size(px(icon_size))
                                .color(cx.theme().muted_foreground),
                        )
                    })
                    .when_some(icon.filter(|_| !loading), |slot, image| {
                        slot.child(img(image).size(px(icon_size)).flex_none())
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    // Every item in this virtual list shares one measured
                    // height. Neither a filename nor a path may wrap and
                    // extend into the following row.
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .h(px(24.0))
                            .line_height(px(24.0))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis_middle()
                            .child(title),
                    )
                    .when_some(
                        if compact { None } else { subtitle.clone() },
                        |col, subtitle| {
                            col.child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .w_full()
                                    .min_w_0()
                                    .h(px(18.0))
                                    .line_height(px(18.0))
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis_middle()
                                    .child(subtitle),
                            )
                        },
                    ),
            )
            .when_some(if compact { subtitle } else { None }, |row, subtitle| {
                row.child(
                    div()
                        .min_w_0()
                        .max_w(px(360.0))
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .truncate()
                        .child(subtitle),
                )
            })
            .when_some(file_size, |row, bytes| {
                row.child(
                    div()
                        .flex_none()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(crate::files::format_size(bytes)),
                )
            })
            .into_any_element()
    }

    /// A featured calculator card: small "Calculator" label + the
    /// question in muted text on the left, the answer large and bold on the
    /// right — visually distinct from a normal search result row.
    fn render_calculator(
        title: SharedString,
        subtitle: Option<SharedString>,
        calculation_detail: Option<CalculationDetail>,
        cx: &mut App,
    ) -> AnyElement {
        let conversion_color = cx.theme().muted_foreground;
        let title = SharedString::from(format_result_for_display(&title));
        let (source_label, target_label, note) = match calculation_detail {
            Some(CalculationDetail::Units {
                source_label,
                target_label,
            }) => (
                Some(SharedString::from(source_label)),
                Some(SharedString::from(target_label)),
                None,
            ),
            Some(CalculationDetail::Timezones {
                source_label,
                target_label,
            }) => (
                Some(SharedString::from(source_label)),
                Some(SharedString::from(target_label)),
                None,
            ),
            Some(CalculationDetail::Currency {
                source_label,
                target_label,
                updated_label,
            }) => (
                Some(SharedString::from(source_label)),
                Some(SharedString::from(target_label)),
                Some(SharedString::from(updated_label)),
            ),
            None => (None, None, None),
        };
        let center_width = if note.is_some() { 176.0 } else { 44.0 };
        let zone_chip = |label: SharedString, cx: &App| {
            div()
                .mt_1()
                .px_2()
                .py_0p5()
                .rounded(cx.theme().radius)
                .border_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().tokens.secondary)
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(label)
        };

        div()
            .flex()
            .flex_col()
            .gap_1()
            .w_full()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .min_h(px(96.0))
                    .rounded(cx.theme().radius)
                    .bg(cx.theme().list_active)
                    .child(
                        h_flex()
                            .items_stretch()
                            .w_full()
                            .flex_1()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .items_center()
                                    .justify_center()
                                    .px_2()
                                    .when_some(subtitle, |col, expression| {
                                        let expression = format_expression_for_display(&expression);
                                        let mut search_start = 0;
                                        let highlights = expression
                                            .split_whitespace()
                                            .filter_map(|word| {
                                                let offset = expression[search_start..]
                                                    .find(word)?
                                                    + search_start;
                                                search_start = offset + word.len();
                                                let connector = word
                                                    .trim_matches(|character: char| {
                                                        !character.is_alphabetic()
                                                    })
                                                    .to_ascii_lowercase();

                                                matches!(
                                                    connector.as_str(),
                                                    "to" | "in"
                                                        | "as"
                                                        | "from"
                                                        | "into"
                                                        | "after"
                                                        | "before"
                                                )
                                                .then(|| {
                                                    (
                                                        offset..offset + word.len(),
                                                        HighlightStyle::color(conversion_color),
                                                    )
                                                })
                                            })
                                            .collect::<Vec<_>>();

                                        col.child(
                                            div()
                                                .text_xl()
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(cx.theme().foreground)
                                                .child(
                                                    StyledText::new(expression)
                                                        .with_highlights(highlights),
                                                ),
                                        )
                                    })
                                    .when_some(source_label, |col, label| {
                                        col.child(zone_chip(label, cx))
                                    }),
                            )
                            .child(
                                div()
                                    .w(px(center_width))
                                    .self_stretch()
                                    .flex_none()
                                    .flex()
                                    .flex_col()
                                    .items_center()
                                    .justify_center()
                                    .child(div().w(px(1.0)).flex_1().bg(conversion_color))
                                    .child(
                                        div()
                                            // A fixed SVG slot avoids font-baseline offsets;
                                            // equal flex rules above/below keep its gaps symmetric.
                                            .size(px(28.0))
                                            .flex_none()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child(
                                                gpui::svg()
                                                    .path(icon_assets::ARROW_RIGHT)
                                                    .size(px(20.0))
                                                    .flex_none()
                                                    .text_color(conversion_color),
                                            ),
                                    )
                                    .when_some(note, |center, label| {
                                        center.child(
                                            div()
                                                .relative()
                                                .top(px(-7.0))
                                                .text_xs()
                                                .text_center()
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(conversion_color)
                                                .child(label),
                                        )
                                    })
                                    .child(div().w(px(1.0)).flex_1().bg(conversion_color)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .items_center()
                                    .justify_center()
                                    .px_2()
                                    .child(
                                        div()
                                            .text_xl()
                                            .text_center()
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(cx.theme().foreground)
                                            .child(title),
                                    )
                                    .when_some(target_label, |col, label| {
                                        col.child(zone_chip(label, cx))
                                    }),
                            ),
                    ),
            )
            .into_any_element()
    }
}
