use super::*;

pub fn open(cx: &mut App) {
    if let Some(handle) = cx.global::<SettingsGlobal>().window
        && handle
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        return;
    }
    let bounds = Bounds::centered(None, size(px(1060.0), px(720.0)), cx);
    let result = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitleBar::title_bar_options()),
            window_min_size: Some(size(px(760.0), px(520.0))),
            ..Default::default()
        },
        |window, cx| {
            window.set_window_title(text::WINDOW_TITLE);
            let view = cx.new(|cx| SettingsView::new(window, cx));
            cx.global_mut::<SettingsGlobal>().view = Some(view.downgrade());
            window.focus(&view.focus_handle(cx), cx);
            cx.new(|cx| Root::new(view, window, cx))
        },
    );
    match result {
        Ok(handle) => cx.global_mut::<SettingsGlobal>().window = Some(handle.into()),
        Err(error) => cx.global_mut::<SettingsGlobal>().error = Some(error.to_string()),
    }
}
