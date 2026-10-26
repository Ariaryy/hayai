use super::*;
use std::time::Duration;

mod general;
mod scrolling;

/// Tests real pre-dispatch recording and asynchronous persistence. Run in a
/// child because the pinned Windows event loop exits the process on quit.
#[test]
#[ignore = "native GUI: cargo test settings_window_gui_smoke -- --ignored --test-threads=1"]
fn settings_window_gui_smoke() {
    const SUCCESS: &str = "settings_window_gui_smoke: passed";
    if std::env::var_os("HAYAI_SETTINGS_GUI_CHILD").is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "settings::window::tests::settings_window_gui_smoke",
                "--exact",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("HAYAI_SETTINGS_GUI_CHILD", "1")
            .env(
                "APPDATA",
                std::env::temp_dir().join(format!("hayai-general-gui-{}", std::process::id())),
            )
            .output()
            .unwrap();
        assert!(
            output.status.success() && String::from_utf8_lossy(&output.stderr).contains(SUCCESS),
            "GUI test failed: {:?}\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        for line in String::from_utf8_lossy(&output.stderr)
            .lines()
            .filter(|line| line.starts_with("SCROLL_FRAME_MS"))
        {
            eprintln!("{line}");
        }
        return;
    }

    struct KeepAlive;
    impl Render for KeepAlive {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    let path = std::env::temp_dir()
        .join(format!("hayai-settings-gui-{}", std::process::id()))
        .join("settings.json");
    gpui_platform::application()
        .with_assets(crate::assets::LauncherAssets)
        .run(move |cx| {
            gpui_component::init(cx);
            cx.set_global(SettingsGlobal::for_test(path.clone()));
            crate::theme::set_mode(false, cx);
            // The real app has a hidden launcher. This hidden sentinel gives the
            // same last-window lifetime without launching or indexing user apps.
            cx.open_window(
                WindowOptions {
                    show: false,
                    focus: false,
                    ..Default::default()
                },
                |_, cx| cx.new(|_| KeepAlive),
            )
            .unwrap();
            open(cx);
            let handle = cx.global::<SettingsGlobal>().window.unwrap();
            let view = cx.global::<SettingsGlobal>().view.clone().unwrap();
            cx.spawn(async move |cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                handle
                    .update(cx, |_, window, cx| {
                        view.update(cx, |view, cx| {
                            view.start_recording(Shortcut::Actions, window, cx)
                        })
                        .unwrap();
                        key(window, "ctrl-shift-k", cx);
                    })
                    .unwrap();
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                cx.update(|cx| {
                    assert_eq!(preferences(cx).binding(Shortcut::Actions), "ctrl-shift-k");
                    assert!(!cx.global::<SettingsGlobal>().saving);
                    assert!(
                        view.upgrade().unwrap().read(cx).recording.is_none(),
                        "valid recording must save without confirmation"
                    );
                });
                let saved = cx
                    .background_spawn({
                        let path = path.clone();
                        async move { std::fs::read_to_string(path).unwrap() }
                    })
                    .await;
                assert!(saved.contains("ctrl-shift-k"));
                handle
                    .update(cx, |_, window, cx| {
                        view.update(cx, |view, cx| {
                            view.start_recording(Shortcut::Settings, window, cx)
                        })
                        .unwrap();
                        key(window, "ctrl-shift-k", cx);
                        view.update(cx, |view, _| {
                            assert!(view.validation.is_some());
                            assert_eq!(view.recording, Some(Shortcut::Settings));
                        })
                        .unwrap();
                        key(window, "escape", cx);
                        view.update(cx, |view, _| assert!(view.recording.is_none()))
                            .unwrap();
                        assert_eq!(preferences(cx).binding(Shortcut::Settings), "ctrl-,");
                    })
                    .unwrap();
                handle
                    .update(cx, |_, window, cx| {
                        view.update(cx, |view, cx| {
                            view.start_recording(Shortcut::Settings, window, cx)
                        })
                        .unwrap();
                        key(window, "ctrl-win-f24", cx);
                    })
                    .unwrap();
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                cx.update(|cx| {
                    assert_eq!(preferences(cx).binding(Shortcut::Settings), "ctrl-win-f24");
                    assert!(view.upgrade().unwrap().read(cx).recording.is_none());
                    assert!(!cx.global::<SettingsGlobal>().saving);
                });
                general::check(&path, cx).await;
                scrolling::check(handle, &view, cx).await;
                for family in crate::theme::ThemeFamily::ALL {
                    for light in [false, true] {
                        cx.update(|cx| {
                            let mut next = preferences(cx).clone();
                            next.light = light;
                            next.set_theme_for(light, family);
                            save(next, cx);
                        });
                        cx.background_executor()
                            .timer(Duration::from_millis(150))
                            .await;
                        cx.update(|cx| {
                            assert!(!cx.global::<SettingsGlobal>().saving);
                            let expected = crate::theme::launcher_palette(family, light);
                            assert_eq!(
                                crate::theme::settings_palette(cx).canvas,
                                expected.background
                            );
                            assert_eq!(
                                gpui_component::Theme::global(cx).foreground,
                                expected.foreground
                            );
                        });
                    }
                }
                cx.update(|cx| {
                    assert_eq!(
                        gpui_component::Theme::global(cx).mode,
                        gpui_component::ThemeMode::Light
                    );
                    assert_eq!(
                        crate::theme::settings_palette(cx).canvas,
                        gpui_component::Theme::global(cx).background
                    );
                });
                let active_background = cx.update(|cx| crate::theme::settings_palette(cx).canvas);
                cx.update(|cx| {
                    let mut next = preferences(cx).clone();
                    next.set_theme_for(false, crate::theme::ThemeFamily::Hayai);
                    save(next, cx);
                });
                cx.background_executor()
                    .timer(Duration::from_millis(150))
                    .await;
                cx.update(|cx| {
                    assert!(
                        preferences(cx).light,
                        "choosing a dark palette must not change the active appearance"
                    );
                    assert_eq!(crate::theme::settings_palette(cx).canvas, active_background);
                    assert_eq!(
                        preferences(cx).theme_for(false),
                        crate::theme::ThemeFamily::Hayai
                    );
                });
                handle
                    .update(cx, |_, window, cx| {
                        view.update(cx, |view, cx| {
                            view.start_recording(Shortcut::Actions, window, cx)
                        })
                        .unwrap();
                        let mut next = preferences(cx).clone();
                        next.light = false;
                        save(next, cx);
                        window.remove_window();
                    })
                    .unwrap();
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                assert!(
                    view.upgrade().is_none(),
                    "closing must release recording subscriptions and view state"
                );
                cx.update(open);
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                cx.update(|cx| {
                    assert_ne!(cx.global::<SettingsGlobal>().window.unwrap(), handle);
                    assert_eq!(preferences(cx).binding(Shortcut::Actions), "ctrl-shift-k");
                    assert!(
                        !preferences(cx).light,
                        "a save must finish after closing settings"
                    );
                    eprintln!("{SUCCESS}");
                    cx.quit();
                });
            })
            .detach();
        });
}

fn key(window: &mut Window, binding: &str, cx: &mut App) {
    window.dispatch_event(
        gpui::PlatformInput::KeyDown(gpui::KeyDownEvent {
            keystroke: gpui::Keystroke::parse(binding).unwrap(),
            is_held: false,
            prefer_character_input: false,
        }),
        cx,
    );
}
