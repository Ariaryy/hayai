use super::*;

pub(super) async fn check(path: &std::path::Path, cx: &mut gpui::AsyncApp) {
    let history_folder =
        std::path::PathBuf::from(std::env::var_os("APPDATA").unwrap()).join("hayai");
    cx.background_spawn({
        let folder = history_folder.clone();
        async move {
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(folder.join("recents.txt"), "app").unwrap();
            std::fs::write(folder.join("file_recents.txt"), "file").unwrap();
        }
    })
    .await;
    let delayed = crate::history::prepare("old queued launch".into()).unwrap();
    cx.update(|cx| {
        let mut next = preferences(cx).clone();
        next.general.remember_recents = false;
        next.general.check_updates = false;
        next.general.download_updates = false;
        next.general.display = crate::settings::DisplayTarget::Primary;
        save(next, cx);
        assert!(!crate::history::enabled());
    });
    cx.background_executor()
        .timer(Duration::from_millis(250))
        .await;
    cx.update(|cx| {
        assert!(!cx.global::<SettingsGlobal>().saving);
        assert!(cx.global::<SettingsGlobal>().error.is_none());
        assert!(!preferences(cx).general.remember_recents);
        assert_eq!(
            preferences(cx).general.display,
            crate::settings::DisplayTarget::Primary
        );
        #[cfg(debug_assertions)]
        {
            let (tx, mut rx) = futures::channel::mpsc::unbounded();
            crate::updates::start(tx);
            assert!(
                rx.try_recv().is_err(),
                "disabled startup checks must not publish a preview"
            );
            assert_eq!(
                crate::updates::check(),
                crate::updates::CheckResult::Available,
                "manual checks must not bypass the automatic-download preference"
            );
            assert_eq!(
                crate::updates::download(),
                crate::updates::CheckResult::Ready,
                "explicit download remains available with automatic checks disabled"
            );
        }
        let mut next = preferences(cx).clone();
        next.general.remember_recents = true;
        save(next, cx);
    });
    cx.background_executor()
        .timer(Duration::from_millis(250))
        .await;
    cx.background_spawn(async move {
        delayed.save(crate::history::Kind::Apps);
    })
    .await;
    cx.background_spawn({
        let folder = history_folder.clone();
        async move {
            assert!(!folder.join("recents.txt").exists());
            assert!(!folder.join("file_recents.txt").exists());
        }
    })
    .await;
    cx.update(crate::settings::clear_history);
    cx.background_executor()
        .timer(Duration::from_millis(250))
        .await;
    cx.update(|cx| {
        assert!(crate::history::enabled());
        assert!(
            preferences(cx).general.remember_recents,
            "clearing must preserve the recording preference"
        );
        assert!(cx.global::<SettingsGlobal>().error.is_none());
    });
    cx.background_spawn({
        let folder = history_folder.clone();
        async move {
            std::fs::write(folder.join("recents.txt"), "queued clear").unwrap();
            std::fs::write(folder.join("file_recents.txt"), "queued clear").unwrap();
        }
    })
    .await;
    // Rapid edits are queued from the latest displayed draft;
    // controls must not disable or lose the second edit mid-save.
    cx.update(|cx| {
        let mut next = preferences(cx).clone();
        next.general.display = crate::settings::DisplayTarget::ActiveWindow;
        save(next, cx);
        let mut next = preferences(cx).clone();
        next.general.remember_recents = false;
        save(next, cx);
        let mut next = preferences(cx).clone();
        next.general.remember_recents = true;
        next.general.display = crate::settings::DisplayTarget::Pointer;
        next.general.check_updates = true;
        save(next, cx);
        assert_eq!(
            preferences(cx).general.display,
            crate::settings::DisplayTarget::Pointer
        );
    });
    cx.background_executor()
        .timer(Duration::from_millis(400))
        .await;
    cx.update(|cx| {
        assert!(!cx.global::<SettingsGlobal>().saving);
        assert!(cx.global::<SettingsGlobal>().desired.is_none());
        assert_eq!(
            crate::settings::preferences(cx).general.display,
            crate::settings::DisplayTarget::Pointer
        );
        assert!(crate::settings::preferences(cx).general.check_updates);
    });
    cx.background_spawn({
        let folder = history_folder.clone();
        async move {
            assert!(
                !folder.join("recents.txt").exists(),
                "off/on must not drop a queued clear"
            );
            assert!(!folder.join("file_recents.txt").exists());
        }
    })
    .await;
    cx.update(|_| assert!(crate::history::enabled()));
    // A failed write rolls the displayed draft back to committed
    // values without applying a theme reset to unrelated controls.
    cx.background_spawn({
        let temporary = path.with_extension("json.tmp");
        async move {
            std::fs::create_dir(temporary).unwrap();
        }
    })
    .await;
    cx.update(|cx| {
        let mut next = preferences(cx).clone();
        next.general.display = crate::settings::DisplayTarget::Primary;
        save(next, cx);
    });
    cx.background_executor()
        .timer(Duration::from_millis(250))
        .await;
    cx.update(|cx| {
        assert!(cx.global::<SettingsGlobal>().error.is_some());
        assert_eq!(
            preferences(cx).general.display,
            crate::settings::DisplayTarget::Pointer
        );
        assert!(!cx.global::<SettingsGlobal>().saving);
        cx.global_mut::<SettingsGlobal>().error = None;
    });
    cx.background_spawn({
        let temporary = path.with_extension("json.tmp");
        async move {
            std::fs::remove_dir(temporary).unwrap();
        }
    })
    .await;
}
