use super::*;
use crate::command_router::Command;
use gpui_component::input::InputEvent;

fn set_inputs(
    view: &gpui::WeakEntity<SettingsView>,
    trigger: &str,
    aliases: &str,
    event: InputEvent,
    window: &mut Window,
    cx: &mut gpui::App,
) {
    let (trigger_input, alias_input) = view
        .update(cx, |view, _| {
            let editor = view.alias_editor.as_ref().unwrap();
            (
                editor.triggers[Command::Settings as usize].clone(),
                editor.additions[Command::Settings as usize].clone(),
            )
        })
        .unwrap();
    trigger_input.update(cx, |input, cx| input.set_value(trigger, window, cx));
    alias_input.update(cx, |input, cx| {
        input.set_value(aliases, window, cx);
        cx.emit(event);
    });
}

pub(super) async fn check(
    handle: gpui::AnyWindowHandle,
    view: &gpui::WeakEntity<SettingsView>,
    path: &std::path::Path,
    cx: &mut gpui::AsyncApp,
) {
    handle
        .update(cx, |_, window, cx| {
            view.update(cx, |view, cx| view.navigate(Page::Commands, window, cx))
                .unwrap();
            let enter = InputEvent::PressEnter {
                secondary: false,
                shift: false,
            };
            set_inputs(view, "p", "folders, preferences", enter, window, cx);
        })
        .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(250))
        .await;
    cx.update(|cx| {
        assert!(!cx.global::<SettingsGlobal>().saving);
        let commands = &crate::settings::preferences(cx).commands;
        assert_eq!(commands.resolve("s"), None);
        assert_eq!(commands.resolve("p"), Some((Command::Settings, "")));
    });
    let contents = cx
        .background_spawn({
            let path = path.to_owned();
            async move { std::fs::read_to_string(path).unwrap() }
        })
        .await;
    assert!(contents.contains("folders"));
    // A trigger shared with another command is saved and flagged, not refused.
    handle
        .update(cx, |_, window, cx| {
            set_inputs(view, "f", "folders", InputEvent::Blur, window, cx);
        })
        .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(250))
        .await;
    cx.update(|cx| {
        assert!(
            view.upgrade()
                .unwrap()
                .read(cx)
                .alias_editor
                .as_ref()
                .unwrap()
                .error
                .is_none()
        );
        let commands = &crate::settings::preferences(cx).commands;
        assert_eq!(commands.trigger(Command::Settings), "f");
        assert!(commands.conflict(Command::Settings, "f").is_some());
        assert_eq!(commands.resolve("f"), None);
    });
    // A malformed trigger stays in the editor with an inline error.
    handle
        .update(cx, |_, window, cx| {
            set_inputs(view, "has space", "folders", InputEvent::Blur, window, cx);
        })
        .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(100))
        .await;
    cx.update(|cx| {
        let view = view.upgrade().unwrap();
        let error = &view.read(cx).alias_editor.as_ref().unwrap().error;
        assert_eq!(
            error.as_ref().map(|(command, _)| *command),
            Some(Command::Settings)
        );
        assert_eq!(
            crate::settings::preferences(cx)
                .commands
                .trigger(Command::Settings),
            "f"
        );
    });
    handle
        .update(cx, |_, window, cx| {
            set_inputs(
                view,
                "s",
                "settings, preferences",
                InputEvent::Change,
                window,
                cx,
            );
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
