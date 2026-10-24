use super::*;

pub(in crate::launcher) fn results_view(
    list: &Entity<ListState<ResultListDelegate>>,
    cx: &Context<LauncherRoot>,
) -> AnyElement {
    // Clip virtualized rows to the results viewport so scrolling
    // cannot paint text over the search input or footer.
    div()
        .relative()
        .flex_1()
        .min_h_0()
        .w_full()
        .overflow_hidden()
        .child({
            let root = cx.entity().downgrade();
            let layout_root = root.clone();
            gpui::canvas(
                move |_, _, cx| {
                    // Layout has queued List's selected-row scroll. Replace
                    // it here so the section heading appears in this frame.
                    let _ = layout_root.update(cx, |root, cx| root.reveal_keyboard_heading(cx));
                },
                move |bounds, _, window, _| {
                    window.on_mouse_event(
                        move |event: &gpui::ScrollWheelEvent, phase, window, cx| {
                            if phase == gpui::DispatchPhase::Capture
                                && bounds.contains(&event.position)
                            {
                                let _ = root
                                    .update(cx, |root, cx| root.on_result_wheel(event, window, cx));
                            }
                        },
                    );
                },
            )
            .absolute()
            .size_full()
        })
        .child(
            List::new(list)
                .scrollbar_visible(false)
                .with_size(Size::Large)
                .p_2()
                .size_full(),
        )
        .into_any_element()
}
