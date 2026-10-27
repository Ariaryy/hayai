use crate::launcher::*;

pub(super) async fn check(
    handle: AnyWindowHandle,
    root: &Entity<LauncherRoot>,
    cx: &mut gpui::AsyncApp,
) {
    let parent = std::env::temp_dir().join(format!("hayai-folder-footer-{}", std::process::id()));
    let child = parent.join("child");
    cx.background_spawn({
        let child = child.clone();
        async move {
            std::fs::create_dir_all(child).unwrap();
        }
    })
    .await;
    handle
        .update(cx, |_, window, cx| {
            root.update(cx, |root, cx| {
                root.enter_file_mode(" f", window, cx);
                root.folder_scope.push(parent.clone());
                root.list.update(cx, |list, cx| {
                    let delegate = list.delegate_mut();
                    delegate.registry.borrow_mut().invalidate();
                    let generation = delegate.registry.borrow().current_generation();
                    delegate.results_generation = generation;
                    delegate.search_pending = false;
                    delegate.results = vec![CommandItem {
                        id: "folder-footer:child".into(),
                        title: "child".into(),
                        subtitle: None,
                        file_size: None,
                        calculation_detail: None,
                        icon: IconSource::None,
                        action: CommandAction::OpenFile(child.clone()),
                    }];
                    delegate.main_results = 1;
                    delegate.selected_index = Some(delegate.index_path(0));
                    refresh_result_layout(list, window, cx);
                });
                assert!(root.can_enter_folder(root.selected_primary_action(cx)));
                assert!(!root.actions_disabled(cx));
                root.on_intercepted_key("tab", false, window, cx);
                assert_eq!(root.folder_scope.last(), Some(&child));
                assert!(root.list.read(cx).delegate().search_pending);
                assert!(
                    !root.actions_disabled(cx),
                    "Actions must not dim for the folder-loading frame"
                );
                assert!(
                    root.can_enter_folder(root.selected_primary_action(cx)),
                    "Tab hint must survive the folder-loading frame"
                );
                assert!(
                    root.selected_primary_action(cx) == PrimaryAction::OpenFolder,
                    "the right-hand Open Folder action must stay stable too"
                );
                root.on_intercepted_key("tab", false, window, cx);
                assert_eq!(
                    root.folder_scope.len(),
                    2,
                    "pending rows cannot be entered twice"
                );
            });
        })
        .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(100))
        .await;
    handle
        .update(cx, |_, window, cx| {
            root.update(cx, |root, cx| {
                assert!(!root.list.read(cx).delegate().search_pending);
                assert!(
                    !root.can_enter_folder(root.selected_primary_action(cx)),
                    "completed non-folder results replace the navigation hint"
                );
                root.reset_to_apps(window, cx);
                assert!(!root.can_enter_folder(root.selected_primary_action(cx)));
                root.list
                    .update(cx, |list, _| list.delegate_mut().show_about = true);
                assert!(root.selected_action_menu(cx).is_none());
                root.list
                    .update(cx, |list, _| list.delegate_mut().show_about = false);
            })
        })
        .unwrap();
    cx.background_spawn(async move {
        std::fs::remove_dir(child).unwrap();
        std::fs::remove_dir(parent).unwrap();
    })
    .await;
}
