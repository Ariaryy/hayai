use crate::launcher::*;

fn edit(root: &mut LauncherRoot, value: &str, window: &mut Window, cx: &mut Context<LauncherRoot>) {
    let input = root.search_input.clone();
    input.update(cx, |input, cx| input.set_value(value, window, cx));
    root.on_search_event(&input, &InputEvent::Change, window, cx);
}

pub(super) async fn check(
    handle: AnyWindowHandle,
    root: &Entity<LauncherRoot>,
    cx: &mut gpui::AsyncApp,
) {
    handle
        .update(cx, |_, window, cx| {
            let preferences = &mut cx
                .global_mut::<crate::settings::SettingsGlobal>()
                .preferences;
            preferences.show_command_picker = true;
            preferences
                .commands
                .set_trigger(crate::command_router::Command::Files, "fi")
                .unwrap();
            root.update(cx, |root, cx| {
                edit(root, " ", window, cx);
                edit(root, "f", window, cx);
                assert!(root.waiting_for_command && !root.file_mode);
                assert!(root.list.read(cx).delegate().results.is_empty());
                let input = root.search_input.clone();
                root.on_search_event(
                    &input,
                    &InputEvent::PressEnter {
                        secondary: false,
                        shift: false,
                    },
                    window,
                    cx,
                );
                assert!(root.waiting_for_command && root.history.is_empty());
                edit(root, "fi", window, cx);
                assert!(root.file_mode && !root.waiting_for_command);
                assert!(root.search_input.read(cx).value().is_empty());
                root.on_intercepted_key("backspace", false, window, cx);
                assert!(!root.file_mode);

                edit(root, " files report", window, cx);
                assert!(root.file_mode);
                assert_eq!(root.search_input.read(cx).value().as_str(), "report");
                root.on_escape(&gpui_component::input::Escape, window, cx);
                edit(root, " = 2+2", window, cx);
                assert!(root.calculator_mode && !root.waiting_for_command);
                assert_eq!(root.search_input.read(cx).value().as_str(), "2+2");
                assert_eq!(root.list.read(cx).delegate().last_query, " = 2+2");
                root.on_escape(&gpui_component::input::Escape, window, cx);
                edit(root, " =", window, cx);
                assert!(root.calculator_mode);
                root.on_intercepted_key("backspace", false, window, cx);
                assert!(!root.calculator_mode);

                // A file job already running must not repopulate the picker.
                root.enter_file_mode(" f report", window, cx);
                root.reset_to_apps(window, cx);
                edit(root, " f", window, cx);
                assert!(root.waiting_for_command);
                cx.notify();
            });
        })
        .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(100))
        .await;
    handle
        .update(cx, |_, window, cx| {
            root.update(cx, |root, cx| {
                assert!(root.waiting_for_command);
                assert!(root.list.read(cx).delegate().results.is_empty());
                edit(root, "unknown", window, cx);
                assert!(root.waiting_for_command);
                root.on_escape(&gpui_component::input::Escape, window, cx);
                assert!(!root.waiting_for_command);
                edit(root, " ", window, cx);
                root.on_intercepted_key("backspace", false, window, cx);
                assert!(!root.waiting_for_command);
                root.reset_to_apps(window, cx);
            });
            root.update(cx, |root, cx| edit(root, " f", window, cx));
        })
        .unwrap();
    // Commit both directions while a partial alias is already visible. This
    // exercises the save completion path, not just freshly opened sessions.
    for show in [false, true] {
        cx.update(|cx| {
            let mut next = crate::settings::preferences(cx).clone();
            next.show_command_picker = show;
            crate::settings::save(next, cx);
        });
        cx.background_executor()
            .timer(Duration::from_millis(200))
            .await;
        if !show {
            // Clearing recents must also refresh the apps beneath hidden
            // suggestions without losing the partially typed alias.
            cx.update(LauncherState::clear_recent_history);
        }
        handle
            .update(cx, |_, window, cx| {
                assert_eq!(crate::settings::preferences(cx).show_command_picker, show);
                assert!(!cx.global::<crate::settings::SettingsGlobal>().saving);
                root.update(cx, |root, cx| {
                    assert!(root.waiting_for_command);
                    assert_eq!(root.search_input.read(cx).value().as_str(), "f");
                    let list = root.list.read(cx);
                    assert_eq!(list.delegate().apps_view, !show);
                    assert_eq!(list.delegate().results.is_empty(), show);
                    assert!(list.selected_index().is_none());
                    let input = root.search_input.clone();
                    root.on_search_event(
                        &input,
                        &InputEvent::PressEnter {
                            secondary: false,
                            shift: false,
                        },
                        window,
                        cx,
                    );
                    assert!(root.waiting_for_command && root.history.is_empty());
                    edit(root, "fi", window, cx);
                    assert!(root.file_mode && !root.waiting_for_command);
                    root.on_escape(&gpui_component::input::Escape, window, cx);
                    // Leave file work pending while returning to command mode;
                    // obsolete completions must not replace either presentation.
                    root.enter_file_mode(" f report", window, cx);
                    root.reset_to_apps(window, cx);
                    edit(root, " f", window, cx);
                });
            })
            .unwrap();
    }
    handle
        .update(cx, |_, window, cx| {
            root.update(cx, |root, cx| {
                root.on_escape(&gpui_component::input::Escape, window, cx);
                root.reset_to_apps(window, cx);
            });
            cx.global_mut::<crate::settings::SettingsGlobal>()
                .preferences
                .commands = Default::default();
            // Hidden picker: text that cannot be a command is a normal search.
            cx.global_mut::<crate::settings::SettingsGlobal>()
                .preferences
                .show_command_picker = false;
            root.update(cx, |root, cx| {
                edit(root, " ", window, cx);
                assert!(root.waiting_for_command);
                edit(root, "c", window, cx);
                assert!(root.waiting_for_command);
                edit(root, "chrome", window, cx);
                assert!(!root.waiting_for_command);
                assert_eq!(root.search_input.read(cx).value().as_str(), "chrome");
                assert_eq!(root.list.read(cx).delegate().last_query, "chrome");
                root.reset_to_apps(window, cx);
            });
        })
        .unwrap();
}

pub(super) async fn settings(
    handle: AnyWindowHandle,
    root: &Entity<LauncherRoot>,
    cx: &mut gpui::AsyncApp,
) {
    handle
        .update(cx, |_, window, cx| {
            root.update(cx, |root, cx| edit(root, " s", window, cx))
        })
        .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(150))
        .await;
    let settings = cx.update(|cx| {
        assert!(
            !cx.global::<LauncherGlobal>()
                .clone_handle()
                .borrow()
                .visible
        );
        cx.global::<crate::settings::SettingsGlobal>()
            .window
            .expect("settings alias opens its own window")
    });
    settings
        .update(cx, |_, window, _| window.remove_window())
        .unwrap();
    cx.update(|cx| {
        cx.global::<LauncherGlobal>()
            .clone_handle()
            .borrow_mut()
            .show(cx)
    });
    cx.background_executor()
        .timer(Duration::from_millis(100))
        .await;
    handle
        .update(cx, |_, _, cx| {
            let root = root.read(cx);
            assert!(!root.waiting_for_command && !root.file_mode && !root.calculator_mode);
            assert!(root.search_input.read(cx).value().is_empty());
        })
        .unwrap();
}
