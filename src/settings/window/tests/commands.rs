use super::*;
use crate::command_router::Command;
use gpui_component::input::InputEvent;

pub(super) async fn check(
    handle: gpui::AnyWindowHandle,
    view: &gpui::WeakEntity<SettingsView>,
    path: &std::path::Path,
    cx: &mut gpui::AsyncApp,
) {
    handle
        .update(cx, |_, window, cx| {
            let input = view
                .update(cx, |view, cx| {
                    view.navigate(Page::Commands, window, cx);
                    view.alias_editor.as_ref().unwrap().inputs[Command::Settings as usize].clone()
                })
                .unwrap();
            input.update(cx, |input, cx| {
                input.set_value("folders, preferences", window, cx);
                cx.emit(InputEvent::PressEnter {
                    secondary: false,
                    shift: false,
                });
            });
        })
        .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(250))
        .await;
    cx.update(|cx| {
        assert!(!cx.global::<SettingsGlobal>().saving);
        assert_eq!(crate::settings::preferences(cx).commands.resolve("f"), None);
        assert_eq!(
            crate::settings::preferences(cx).commands.resolve("fo"),
            Some((Command::Settings, ""))
        );
    });
    let contents = cx
        .background_spawn({
            let path = path.to_owned();
            async move { std::fs::read_to_string(path).unwrap() }
        })
        .await;
    assert!(contents.contains("folders"));
    handle
        .update(cx, |_, window, cx| {
            let input = view
                .upgrade()
                .unwrap()
                .read(cx)
                .alias_editor
                .as_ref()
                .unwrap()
                .inputs[Command::Settings as usize]
                .clone();
            input.update(cx, |input, cx| {
                input.set_value("files", window, cx);
                cx.emit(InputEvent::Blur);
            });
        })
        .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(100))
        .await;
    cx.update(|cx| {
        assert!(view.upgrade().unwrap().read(cx).validation.is_some());
        assert_eq!(
            crate::settings::preferences(cx).commands.resolve("fo"),
            Some((Command::Settings, ""))
        );
    });
    handle
        .update(cx, |_, window, cx| {
            let input = view
                .upgrade()
                .unwrap()
                .read(cx)
                .alias_editor
                .as_ref()
                .unwrap()
                .inputs[Command::Settings as usize]
                .clone();
            input.update(cx, |input, cx| {
                input.set_value("settings, preferences", window, cx)
            });
            // Sidebar navigation must flush even before a deferred Blur arrives.
            view.update(cx, |view, cx| view.navigate(Page::General, window, cx))
                .unwrap();
        })
        .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(250))
        .await;
    cx.update(|cx| {
        assert_eq!(view.upgrade().unwrap().read(cx).page, Page::General);
        assert_eq!(
            crate::settings::preferences(cx).commands.resolve("s"),
            Some((Command::Settings, ""))
        );
        assert!(!cx.global::<SettingsGlobal>().saving);
    });
}
