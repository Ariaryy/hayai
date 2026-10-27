use super::*;

pub(super) async fn check(
    handle: gpui::AnyWindowHandle,
    view: &gpui::WeakEntity<SettingsView>,
    cx: &mut gpui::AsyncApp,
) {
    handle
        .update(cx, |_, window, cx| {
            view.update(cx, |view, cx| {
                view.navigate(Page::Licenses, window, cx);
                view.expanded_notice = Some(6);
            })
            .unwrap();
        })
        .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(200))
        .await;
    handle
        .update(cx, |_, window, cx| {
            let scroll = view.upgrade().unwrap().read(cx).scrolling.handle.clone();
            assert!(scroll.max_offset().y > px(0.0));
            window.dispatch_event(
                gpui::PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                    position: scroll.bounds().origin + gpui::point(px(30.0), px(30.0)),
                    delta: gpui::ScrollDelta::Lines(gpui::point(0.0, -5.0)),
                    ..Default::default()
                }),
                cx,
            );
            assert!(view.upgrade().unwrap().read(cx).scrolling.animating());
        })
        .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(600))
        .await;
    handle
        .update(cx, |_, window, cx| {
            view.update(cx, |view, cx| {
                assert!(view.scrolling.handle.offset().y < px(0.0));
                assert!(!view.scrolling.animating(), "wheel animation must settle");
                eprintln!(
                    "SCROLL_FRAME_MS {:?}",
                    view.scrolling
                        .frame_samples
                        .windows(2)
                        .map(|pair| pair[1].duration_since(pair[0]).as_secs_f64() * 1000.0)
                        .collect::<Vec<_>>()
                );
                view.navigate(Page::General, window, cx);
                assert_eq!(view.scrolling.handle.offset().y, px(0.0));
                assert!(!view.scrolling.animating());
            })
            .unwrap();
        })
        .unwrap();
    for page in [Page::General, Page::Appearance, Page::KeyBindings] {
        handle
            .update(cx, |_, window, cx| {
                view.update(cx, |view, cx| {
                    view.navigate(page, window, cx);
                    view.scrolling.frame_samples.clear();
                })
                .unwrap();
            })
            .unwrap();
        cx.background_executor()
            .timer(Duration::from_millis(150))
            .await;
        handle
            .update(cx, |_, window, cx| {
                let scroll = view.upgrade().unwrap().read(cx).scrolling.handle.clone();
                window.dispatch_event(
                    gpui::PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                        position: scroll.bounds().origin + gpui::point(px(30.0), px(30.0)),
                        delta: gpui::ScrollDelta::Lines(gpui::point(0.0, -5.0)),
                        ..Default::default()
                    }),
                    cx,
                );
            })
            .unwrap();
        cx.background_executor()
            .timer(Duration::from_millis(500))
            .await;
        cx.update(|cx| {
            let view = view.upgrade().unwrap();
            let view = view.read(cx);
            eprintln!(
                "SCROLL_FRAME_MS {} {:?}",
                page.label(),
                view.scrolling
                    .frame_samples
                    .windows(2)
                    .map(|pair| pair[1].duration_since(pair[0]).as_secs_f64() * 1000.0)
                    .collect::<Vec<_>>()
            );
        });
    }
}
