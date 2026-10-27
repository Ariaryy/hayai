use super::{format_expression_for_display, format_result_for_display};
use crate::assets::icons as icon_assets;

mod routing;

#[cfg(windows)]
#[test]
fn keyboard_heading_entries_skip_empty_sections_and_include_headers() {
    assert_eq!(super::section_header_entry([0].into_iter()), 0);
    assert_eq!(super::section_header_entry([2].into_iter()), 4);
    assert_eq!(super::section_header_entry([0, 10].into_iter()), 12);
    assert_eq!(super::section_header_entry([2, 10].into_iter()), 16);
    assert_eq!(super::section_header_entry([0, 0].into_iter()), 0);
}

/// Exercises actual event capture, render phases and frame requests. Run
/// separately: this briefly opens a native window and needs a Windows desktop.
#[test]
#[ignore = "native GUI smoke: cargo test wheel_scroll_gui_smoke -- --ignored --test-threads=1"]
fn wheel_scroll_gui_smoke() {
    use super::*;
    const SUCCESS: &str = "wheel_scroll_gui_smoke: event, animation and settling passed";
    // The pinned Windows platform calls ExitProcess when its event loop
    // quits. Run it in a child so an early zero exit cannot fool libtest.
    if std::env::var_os("HAYAI_SCROLL_GUI_CHILD").is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "launcher::tests::wheel_scroll_gui_smoke",
                "--exact",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("HAYAI_SCROLL_GUI_CHILD", "1")
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success() && stderr.contains(SUCCESS),
            "GUI smoke failed: {:?}\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            stderr
        );
        return;
    }
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(10));
        eprintln!("GUI smoke timed out");
        std::process::exit(2);
    });
    struct Rows;
    impl crate::commands::CommandProvider for Rows {
        fn namespace(&self) -> &'static str {
            "apps"
        }
        fn search(&self, _: &str) -> Vec<CommandItem> {
            (0..200)
                .map(|i| CommandItem {
                    id: format!("smoke:{i}"),
                    title: format!("Smoke row {i}"),
                    subtitle: None,
                    file_size: None,
                    calculation_detail: None,
                    icon: IconSource::None,
                    action: CommandAction::LaunchApplication("unused.exe".into()),
                })
                .collect()
        }
    }
    struct Files;
    impl crate::commands::CommandProvider for Files {
        fn namespace(&self) -> &'static str {
            "files"
        }
        fn keyword(&self) -> Option<&'static str> {
            Some("f")
        }
        fn wants_debounce(&self) -> bool {
            true
        }
        fn search(&self, _: &str) -> Vec<CommandItem> {
            Rows.search("")
        }
        fn background_search(&self, _: &str) -> Option<crate::commands::BackgroundSearch> {
            Some(Box::new(|| {
                std::thread::sleep(Duration::from_millis(60));
                Rows.search("")
            }))
        }
    }
    gpui_platform::application()
        .with_assets(crate::assets::LauncherAssets)
        .run(move |cx| {
            gpui_component::init(cx);
            cx.set_global(crate::settings::SettingsGlobal::for_test(std::env::temp_dir().join(format!("hayai-scroll-gui-{}.json", std::process::id()))));
            gpui_component::Theme::change(gpui_component::ThemeMode::Dark, None, cx);
            crate::theme::apply_launcher_theme(cx);
            let launcher = LauncherState::install(cx);
            Catalog::install_test_catalog(cx);
            let mut registry = PluginRegistry::new();
            registry.register(Rows);
            registry.register(Files);
            registry.register(crate::calc::CalcProvider::new());
            crate::plugins::RegistryGlobal::install(cx, registry);
            launcher.borrow_mut().show(cx);
            let window = launcher.borrow().window.unwrap();
            let root = launcher.borrow().view.clone().unwrap();
            cx.spawn(async move |cx| {
                // Let the real list lay out before dispatching through its
                // capture listener. The original scheduler panicked right here.
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                routing::check(window, &root, cx).await;
                // The routing checks rebuild the result list after the picker.
                // Let it lay out before using its measured scroll bounds.
                cx.background_executor().timer(Duration::from_millis(200)).await;
                window
                    .update(cx, |_, window, cx| {
                        let bounds = root.read(cx).list.read(cx).scroll_handle().bounds();
                        window.dispatch_event(
                            gpui::PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                                position: bounds.origin + gpui::point(px(12.0), px(12.0)),
                                delta: gpui::ScrollDelta::Lines(gpui::point(0.0, -6.0)),
                                ..Default::default()
                            }),
                            cx,
                        );
                        assert!(root.read(cx).wheel_animation.is_some());
                    })
                    .unwrap();
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;
                window
                    .update(cx, |_, _, cx| {
                        let root = root.read(cx);
                        let handle = root.list.read(cx).scroll_handle();
                        assert!(handle.offset().y < px(0.0));
                        assert!(
                            root.wheel_animation.is_none(),
                            "animation must settle and stop requesting frames"
                        );
                    })
                    .unwrap();
                window.update(cx, |_, window, cx| {
                    root.update(cx, |root, cx| {
                        for _ in 0..20 { root.move_selection(1, window, cx); }
                    });
                }).unwrap();
                cx.background_executor().timer(Duration::from_millis(100)).await;
                window.update(cx, |_, window, cx| {
                    assert!(root.read(cx).list.read(cx).scroll_handle().offset().y < px(0.0));
                    root.update(cx, |root, cx| {
                        for _ in 0..20 { root.move_selection(-1, window, cx); }
                    });
                }).unwrap();
                cx.background_executor().timer(Duration::from_millis(100)).await;
                window.update(cx, |_, _, cx| {
                    assert_eq!(root.read(cx).list.read(cx).scroll_handle().offset().y, px(0.0), "keyboard navigation back to the first row must reveal its section heading");
                }).unwrap();
                window.update(cx, |_, window, cx| {
                    root.update(cx, |root, cx| {
                        root.move_selection(-1, window, cx);
                        assert_eq!(root.list.read(cx).selected_index().unwrap().row, 0);
                        for _ in 0..250 { root.move_selection(1, window, cx); }
                        let last = root.list.read(cx).selected_index();
                        root.move_selection(1, window, cx);
                        assert_eq!(root.list.read(cx).selected_index(), last, "Down must stop at the last result");
                    });
                }).unwrap();
                cx.background_executor().timer(Duration::from_millis(100)).await;
                window
                    .update(cx, |_, window, cx| {
                        let handle = root.read(cx).list.read(cx).scroll_handle().clone();
                        let mut offset = handle.offset();
                        offset.y = px(0.0);
                        handle.set_offset(offset);
                        let position = handle.bounds().origin + gpui::point(px(12.0), px(12.0));
                        // Touchpad flick, opposing wheel tick, then the old
                        // Direct Manipulation inertia continuing for many events.
                        window.dispatch_event(
                            gpui::PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                                position,
                                delta: gpui::ScrollDelta::Pixels(gpui::point(
                                    px(0.0),
                                    px(200.0),
                                )),
                                touch_phase: gpui::TouchPhase::Started,
                                ..Default::default()
                            }),
                            cx,
                        );
                        window.dispatch_event(
                            gpui::PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                                position,
                                delta: gpui::ScrollDelta::Lines(gpui::point(0.0, -3.0)),
                                ..Default::default()
                            }),
                            cx,
                        );
                        for _ in 0..20 {
                            window.dispatch_event(
                                gpui::PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                                    position,
                                    delta: gpui::ScrollDelta::Pixels(gpui::point(
                                        px(0.0),
                                        px(20.0),
                                    )),
                                    touch_phase: gpui::TouchPhase::Moved,
                                    ..Default::default()
                                }),
                                cx,
                            );
                        }
                        let animation = root
                            .read(cx)
                            .wheel_animation
                            .as_ref()
                            .expect("old touchpad inertia must not cancel mouse scrolling");
                        assert!(
                            animation.motion.target < 0.0,
                            "the first mouse tick must move down from the visible top"
                        );
                    })
                    .unwrap();
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;
                window
                    .update(cx, |_, window, cx| {
                        let handle = root.read(cx).list.read(cx).scroll_handle().clone();
                        assert!(
                            handle.offset().y < px(0.0),
                            "mouse must move before old touchpad gesture ends: offset={:?}, max={:?}, rows={}, visible={}", handle.offset(), handle.max_offset(), root.read(cx).list.read(cx).delegate().results.len(), cx.global::<LauncherGlobal>().clone_handle().borrow().visible
                        );
                        assert!(
                            root.read(cx).wheel_animation.is_none(),
                            "mixed animation failed to settle: {:?}, offset={:?}, visible={}",
                            root.read(cx).wheel_animation.as_ref().map(|a| (
                                a.motion.position,
                                a.motion.target,
                                a.last_frame.elapsed()
                            )),
                            handle.offset().y,
                            cx.global::<LauncherGlobal>()
                                .clone_handle()
                                .borrow()
                                .visible
                        );
                        let before = handle.offset().y;
                        let position = handle.bounds().origin + gpui::point(px(12.0), px(12.0));
                        window.dispatch_event(
                            gpui::PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                                position,
                                delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.0), px(3.0))),
                                touch_phase: gpui::TouchPhase::Started,
                                ..Default::default()
                            }),
                            cx,
                        );
                        assert_eq!(
                            handle.offset().y,
                            before + px(3.0),
                            "a new touchpad gesture must take over immediately"
                        );
                    })
                    .unwrap();
                cx.update(|cx| {
                    launcher.borrow_mut().hide(cx);
                    launcher.borrow_mut().show(cx);
                });
                cx.background_executor().timer(Duration::from_millis(100)).await;
                window.update(cx, |_, _, cx| {
                    let root = root.read(cx);
                    assert_eq!(root.list.read(cx).scroll_handle().offset().y, px(0.0), "reopening must reveal the heading");
                    assert!(root.search_input.read(cx).value().is_empty());
                }).unwrap();
                for (case, mode) in ["apps", "file", "pending_file", "calc", "explicit_calc", "about", "updates", "scry", "actions", "folder", "command"].into_iter().enumerate() {
                    window.update(cx, |_, window, cx| {
                        root.update(cx, |root, cx| {
                            match mode {
                                "file" | "pending_file" | "folder" => {
                                    root.enter_file_mode(" f report", window, cx);
                                    if mode == "folder" { root.folder_scope.push(r"C:\Windows".into()); }
                                }
                                "calc" => {
                                    let input = root.search_input.clone();
                                    input.update(cx, |input, cx| input.set_value("2+2", window, cx));
                                    root.on_search_event(&input, &InputEvent::Change, window, cx);
                                    assert!(!root.list.read(cx).delegate().apps_view);
                                }
                                "explicit_calc" => { root.route_command(" = 2+2", window, cx); }
                                "about" | "updates" | "scry" => root.list.update(cx, |list, cx| {
                                    let delegate = list.delegate_mut();
                                    delegate.show_about = mode == "about";
                                    delegate.show_updates = mode == "updates";
                                    delegate.show_scry_updates = mode == "scry";
                                    cx.notify();
                                }),
                                "actions" => { root.action_menu = root.selected_action_menu(cx); }
                                "command" => { root.waiting_for_command = true; }
                                _ => {}
                            }
                            cx.notify();
                        });
                    }).unwrap();
                    // Include both completed file results and a job that
                    // finishes after dismissal; no real daemon/IPC needed.
                    cx.background_executor().timer(Duration::from_millis(if mode == "pending_file" { 15 } else { 100 })).await;
                    if case % 3 == 0 {
                        cx.update(|cx| launcher.borrow_mut().hide(cx));
                    } else if case % 3 == 1 {
                        window.update(cx, |_, window, cx| LauncherState::dismiss(window, cx)).unwrap();
                    } else {
                        window.update(cx, |_, window, cx| {
                            root.update(cx, |root, cx| {
                                for _ in 0..5 { root.on_escape(&Escape, window, cx); }
                            });
                        }).unwrap();
                    }
                    cx.update(|cx| launcher.borrow_mut().show(cx));
                    cx.background_executor().timer(Duration::from_millis(100)).await;
                    window.update(cx, |_, _, cx| {
                        let root = root.read(cx);
                        let delegate = root.list.read(cx).delegate();
                        assert!(delegate.last_query.is_empty(), "{mode}: stale completion changed query");
                        assert!(!delegate.file_search && !root.file_mode, "{mode}: file mode survived reset");
                        assert_eq!(root.list.read(cx).scroll_handle().offset().y, px(0.0), "{mode}: stale scroll offset");
                    }).unwrap();
                }
                routing::settings(window, &root, cx).await;
                eprintln!("{SUCCESS}");
                cx.update(|cx| cx.quit());
            })
            .detach();
        });
}

#[test]
fn icon_warm_region_stays_bounded_at_result_edges() {
    assert_eq!(super::icon_neighborhood(0, 1000, 32), 0..33);
    assert_eq!(super::icon_neighborhood(500, 1000, 32), 468..533);
    assert_eq!(super::icon_neighborhood(999, 1000, 32), 967..1000);
    assert_eq!(super::icon_neighborhood(0, 0, 32), 0..0);
    assert_eq!(super::icon_neighborhood(usize::MAX, 10, 32), 10..10);
}

#[test]
fn row_icon_prefilter_averages_fine_details_at_physical_size() {
    assert_eq!(super::row_icon_pixels(1.0), 28);
    assert_eq!(super::row_icon_pixels(1.25), 35);
    assert_eq!(super::row_icon_pixels(1.5), 42);
    assert_eq!(super::row_icon_pixels(2.0), 48);
    let mut bgra = Vec::new();
    for y in 0..44 {
        for x in 0..44 {
            let value = if (x + y) % 2 == 0 { 0 } else { 255 };
            bgra.extend_from_slice(&[value, value, value, 255]);
        }
    }
    let icon = super::prepare_row_icon(
        crate::native::IconImage {
            width: 44,
            height: 44,
            bgra,
        },
        22,
    )
    .unwrap();
    assert_eq!(icon.size(0).width.0, 22);
    assert_eq!(icon.size(0).height.0, 22);
    let bytes = icon.as_bytes(0).unwrap();
    for y in 3..19 {
        for x in 3..19 {
            let pixel = &bytes[(y * 22 + x) * 4..][..4];
            assert!((127..=128).contains(&pixel[0]));
            assert_eq!(pixel[3], 255);
        }
    }
}

#[test]
fn empty_results_keep_a_valid_measurement_section_when_search_is_cleared() {
    for (main, total, section) in [(0, 0, 1), (5, 5, 1), (0, 2, 2), (5, 7, 1)] {
        assert_eq!(super::result_index_path(0, main, total).section, section);
    }
    assert_eq!(super::result_index_path(5, 5, 7).section, 2);
    assert_eq!(super::result_index_path(5, 5, 7).row, 0);
}

#[test]
fn failed_checks_are_not_notifications_even_after_a_manual_check() {
    use crate::scry_updates::State;
    assert!(!super::scry_check_notice(Some(&State::Failed), false, true));
    assert!(!super::scry_check_notice(
        Some(&State::Failed),
        false,
        false
    ));
    assert!(super::scry_check_notice(
        Some(&State::Available),
        false,
        false
    ));
    assert!(super::scry_check_notice(Some(&State::Current), false, true));
    assert!(!super::scry_check_notice(
        Some(&State::Current),
        false,
        false
    ));
}

#[test]
fn update_notifications_keep_distinct_actions_in_a_shared_section() {
    use super::{
        UpdateNotification::{Hayai, Scry},
        notification_kind,
    };
    assert_eq!(notification_kind(0, true, true), Some(Hayai));
    assert_eq!(notification_kind(1, true, true), Some(Scry));
    assert_eq!(notification_kind(2, true, true), None);
    assert_eq!(notification_kind(0, false, true), Some(Scry));
    assert_eq!(notification_kind(1, false, true), None);
    assert_eq!(notification_kind(0, true, false), Some(Hayai));
    assert_eq!(notification_kind(0, false, false), None);
}

#[test]
fn notification_actions_include_dismiss_without_file_operations() {
    let menu = super::ActionMenu {
        diagnostics_only: false,
        notification: true,
        notification_action: Some(super::PrimaryAction::RestartToUpdate),
        submenu: None,
        path: Default::default(),
        is_dir: false,
        is_app: false,
        can_run_as_admin: false,
        selected: 0,
    };
    assert_eq!(
        menu.actions().map(|(_, label)| label).collect::<Vec<_>>(),
        ["Restart to update", "Dismiss", "Copy diagnostic log"]
    );
}

#[test]
fn update_page_actions_only_copy_diagnostics() {
    let menu = super::ActionMenu {
        diagnostics_only: true,
        notification: false,
        notification_action: None,
        submenu: None,
        path: Default::default(),
        is_dir: false,
        is_app: false,
        can_run_as_admin: false,
        selected: 0,
    };
    assert_eq!(
        menu.actions().map(|(_, label)| label).collect::<Vec<_>>(),
        ["Copy diagnostic log"]
    );
}

#[test]
fn notification_progress_is_status_not_an_action() {
    for status in [
        super::PrimaryAction::Updating,
        super::PrimaryAction::Checking,
        super::PrimaryAction::Starting,
        super::PrimaryAction::Restarting,
    ] {
        let menu = super::ActionMenu {
            diagnostics_only: false,
            notification: true,
            notification_action: Some(status),
            submenu: None,
            path: Default::default(),
            is_dir: false,
            is_app: false,
            can_run_as_admin: false,
            selected: 0,
        };
        assert_eq!(
            menu.actions().map(|(_, label)| label).collect::<Vec<_>>(),
            ["Dismiss", "Copy diagnostic log"]
        );
    }
}

#[test]
fn scry_readiness_accepts_empty_searches_but_not_setup_prompts() {
    use crate::commands::{CommandAction, CommandItem, IconSource};
    let item = |action| CommandItem {
        id: "readiness-test".into(),
        title: String::new(),
        subtitle: None,
        file_size: None,
        calculation_detail: None,
        icon: IconSource::None,
        action,
    };
    assert!(super::scry_search_is_ready(&[]));
    assert!(!super::scry_search_is_ready(&[item(
        CommandAction::StartFileSearch
    )]));
    assert!(!super::scry_search_is_ready(&[item(
        CommandAction::InstallFileSearch
    )]));
    assert!(super::scry_search_is_ready(&[item(
        CommandAction::OpenFile("example.txt".into())
    )]));
}

#[test]
fn baked_builtin_icons_have_separate_row_and_detail_sizes() {
    for bytes in [
        icon_assets::BRAND_PNG,
        icon_assets::UPDATE_PNG,
        icon_assets::SCRY_PNG,
    ] {
        let source = super::load_builtin_icon(bytes).expect("decode baked icon");
        assert!(
            source
                .as_bytes(0)
                .unwrap()
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| pixel[3] != 0)
        );
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let pixels = super::row_icon_pixels(scale);
            let row = super::builtin_row_icon(&Some(source.clone()), pixels).unwrap();
            assert_eq!(row.size(0).width.0, pixels as i32);
            assert_eq!(row.size(0).height.0, pixels as i32);
        }
        assert_eq!(source.size(0).width.0, 128);
    }
}

#[test]
fn open_with_submenu_has_its_own_numbering_and_returns_to_parent() {
    let mut menu = super::ActionMenu {
        diagnostics_only: false,
        notification: false,
        notification_action: None,
        submenu: Some(super::OpenWithMenu {
            request: 1,
            loading: true,
            apps: vec![super::OpenWithApp {
                id: "test-handler".into(),
                name: "Text editor".into(),
                icon: None,
            }]
            .into(),
        }),
        path: std::env::current_dir().unwrap().join("notes.txt"),
        is_dir: false,
        is_app: false,
        can_run_as_admin: false,
        selected: 0,
    };
    assert_eq!(menu.count(), 4); // Keep the parent panel intact until apps are ready.
    assert!(matches!(menu.row(0), Some((super::MenuAction::Open, _, _))));
    menu.submenu.as_mut().unwrap().loading = false;
    assert_eq!(menu.count(), 2);
    assert!(matches!(
        menu.row(0),
        Some((super::MenuAction::AssociatedApp(0), _, _))
    ));
    assert!(matches!(
        menu.row(1),
        Some((super::MenuAction::ChooseApp, _, _))
    ));
    menu.selected = 1;
    assert!(menu.return_to_parent());
    assert_eq!(menu.selected, 0);
    assert_eq!(menu.count(), 4);
    assert!(matches!(menu.row(0), Some((super::MenuAction::Open, _, _))));
    assert!(!menu.return_to_parent());
}

#[test]
fn context_actions_follow_result_type_and_stay_compact() {
    let root = std::env::current_dir().unwrap();
    for (name, is_dir, is_app, admin, expected) in [
        (
            "report.pdf",
            false,
            false,
            false,
            vec!["Open", "Open with…", "Show in Explorer", "Copy path"],
        ),
        (
            "tool.exe",
            false,
            false,
            true,
            vec![
                "Open",
                "Show in Explorer",
                "Copy path",
                "Run as administrator",
            ],
        ),
        (
            "Photos",
            true,
            false,
            false,
            vec!["Open folder", "Open parent folder", "Copy path"],
        ),
        (
            "browser.lnk",
            false,
            true,
            true,
            vec![
                "Open application",
                "Show in Explorer",
                "Copy path",
                "Run as administrator",
            ],
        ),
    ] {
        let menu = super::ActionMenu {
            diagnostics_only: false,
            notification: false,
            notification_action: None,
            submenu: None,
            path: root.join(name),
            is_dir,
            is_app,
            can_run_as_admin: admin,
            selected: 0,
        };
        assert_eq!(
            menu.actions().map(|(_, label)| label).collect::<Vec<_>>(),
            expected
        );
    }
    let packaged = super::ActionMenu {
        diagnostics_only: false,
        notification: false,
        notification_action: None,
        submenu: None,
        path: "shell:AppsFolder/Publisher.App!App".into(),
        is_dir: false,
        is_app: true,
        can_run_as_admin: false,
        selected: 0,
    };
    assert_eq!(
        packaged
            .actions()
            .map(|(_, label)| label)
            .collect::<Vec<_>>(),
        ["Open application"]
    );
}

#[test]
fn displays_numeric_exponents_as_superscripts() {
    assert_eq!(format_expression_for_display("43560 ft^2"), "43560 ft²");
    assert_eq!(format_expression_for_display("m^-12"), "m⁻¹²");
    assert_eq!(format_expression_for_display("2^x"), "2^x");
}

#[test]
fn spaces_binary_operators_without_splitting_unary_signs() {
    assert_eq!(format_expression_for_display("12 -2"), "12 - 2");
    assert_eq!(format_expression_for_display("12+2* 3"), "12 + 2 * 3");
    assert_eq!(format_expression_for_display("5*-2"), "5 * -2");
    assert_eq!(format_expression_for_display("-12/3"), "-12 / 3");
}

#[test]
fn expands_and_pluralizes_duration_units() {
    assert_eq!(
        format_expression_for_display("12 hr from now"),
        "12 hours from now"
    );
    assert_eq!(
        format_expression_for_display("12 hrs from now"),
        "12 hours from now"
    );
    assert_eq!(format_expression_for_display("1 hrs ago"), "1 hour ago");
    assert_eq!(
        format_expression_for_display("in 2 wk 1 d"),
        "in 2 weeks 1 day"
    );
}

#[test]
fn keeps_clock_and_meridiem_on_the_same_line() {
    assert_eq!(
        format_result_for_display("Sunday, August 30, 2026, 2:35 AM"),
        "Sunday, August 30, 2026,\n2:35\u{00a0}AM"
    );
    assert_eq!(
        format_result_for_display("9:05 PM IST"),
        "9:05\u{00a0}PM IST"
    );
}

#[test]
fn displays_timezone_abbreviations_in_uppercase() {
    assert_eq!(
        format_expression_for_display("12pm ist to cst"),
        "12 PM IST to CST"
    );
    assert_eq!(
        format_expression_for_display("12 pm ist to cst"),
        "12 PM IST to CST"
    );
    assert_eq!(format_expression_for_display("utc to jst"), "UTC to JST");
    assert_eq!(format_expression_for_display("1 pm"), "1 pm");
    assert_eq!(
        format_expression_for_display("1 usd to inr"),
        "1 USD to INR"
    );
}
#[test]
fn empty_notices_are_not_selectable_results_and_recovery_clears_them() {
    use super::{CommandAction, CommandItem, IconSource, take_empty_state};
    let notice = CommandItem {
        id: "calc:error".into(),
        title: "Invalid expression".into(),
        subtitle: None,
        file_size: None,
        calculation_detail: None,
        icon: IconSource::None,
        action: CommandAction::EmptyState,
    };
    let mut items = vec![notice];
    assert_eq!(
        take_empty_state(&mut items).unwrap().title,
        "Invalid expression"
    );
    assert!(items.is_empty());
    assert!(take_empty_state(&mut items).is_none());
    items.push(CommandItem {
        id: "calc:2+2".into(),
        title: "4".into(),
        subtitle: None,
        file_size: None,
        calculation_detail: None,
        icon: IconSource::None,
        action: CommandAction::CopyToClipboard("4".into()),
    });
    assert!(take_empty_state(&mut items).is_none());
    assert_eq!(items.len(), 1);
}
