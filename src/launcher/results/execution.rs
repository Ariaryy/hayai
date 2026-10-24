use super::*;

impl ResultListDelegate {
    pub(super) fn confirm_result(
        &mut self,
        _secondary: bool,
        window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) {
        // Bookkeeping first (cheap, in-memory), then hide the window
        // *immediately* — a shell call can take hundreds of ms and must not
        // hold the launcher on screen after Enter.
        if self.search_pending && !self.show_scry_updates && !self.show_updates {
            return;
        }
        if self.show_scry_updates && self.scry_ready {
            let query = self.last_query.clone();
            self.show_scry_updates = false;
            // These compatibility rows describe the pre-install daemon.
            // Drop them before refreshing so they cannot reopen setup.
            self.results.clear();
            self.perform_search(&query, window, cx).detach();
            return;
        }
        if let Some(notification) = self.selected_notification()
            && !self.show_updates
            && !self.show_scry_updates
        {
            match notification {
                UpdateNotification::Scry => {
                    if self.scry_installing {
                        return;
                    }
                    if self.scry_install_attempted {
                        if self.scry_ready {
                            self.scry_update_dismissed = true;
                            self.notification_selected = false;
                            self.selected_index =
                                (!self.results.is_empty()).then(|| self.index_path(0));
                            cx.notify();
                        } else {
                            self.show_scry_updates = true;
                            cx.notify();
                        }
                        return;
                    }
                    if self.scry_checking {
                        return;
                    }
                    if self
                        .scry_discovery
                        .as_ref()
                        .is_some_and(|result| result.state == crate::scry_updates::State::Available)
                    {
                        self.show_scry_updates = true;
                    } else {
                        self.check_scry_updates(true, window, cx);
                    }
                    cx.notify();
                    return;
                }
                UpdateNotification::Hayai
                    if !self.checking_update && crate::updates::ready_version().is_some() =>
                {
                    self.show_updates = true;
                    self.update_result = Some(crate::updates::CheckResult::Ready);
                    cx.notify();
                    return;
                }
                _ => {}
            }
        }
        let action = if self.show_scry_updates {
            if self.scry_installing || (self.scry_checking && !self.needs_pinned_scry()) {
                return;
            }
            Some(if self.scry_target().is_some() {
                CommandAction::UpdateFileSearch
            } else {
                CommandAction::CheckFileSearchUpdates
            })
        } else if self.show_updates {
            if self.checking_update || self.restarting_update {
                return;
            }
            Some(
                if self.update_result == Some(crate::updates::CheckResult::Ready) {
                    CommandAction::RestartToUpdate
                } else {
                    CommandAction::CheckForUpdates
                },
            )
        } else if self.notification_selected {
            Some(
                if self.selected_notification() == Some(UpdateNotification::Scry) {
                    CommandAction::CheckFileSearchUpdates
                } else {
                    CommandAction::CheckForUpdates
                },
            )
        } else {
            self.selected_index
                .and_then(|ix| self.item(ix))
                .map(|item| item.action.clone())
        };
        match action {
            Some(CommandAction::CheckFileSearchUpdates) => {
                // Only unavailable/incompatible file search needs a blocking
                // setup page. Optional discovery never replaces search results.
                self.show_scry_updates = self.file_search && !scry_search_is_ready(&self.results);
                self.show_updates = false;
                self.show_about = false;
                self.check_scry_updates(true, window, cx);
            }
            Some(CommandAction::UpdateFileSearch) => {
                self.update_scry(cx);
            }
            Some(CommandAction::CheckForUpdates) => {
                if self.file_search
                    && self
                        .results
                        .iter()
                        .any(|item| item.id == "files:compatibility:newer")
                    && crate::updates::ready_version().is_some()
                {
                    self.show_updates = true;
                    self.update_result = Some(crate::updates::CheckResult::Ready);
                    cx.notify();
                    return;
                }
                self.check_scry_updates(true, window, cx);
                self.show_updates = false;
                self.update_notice = true;
                self.update_dismissed = false;
                self.show_about = false;
                cx.notify();
                if self.checking_update {
                    return;
                }
                self.checking_update = true;
                self.update_failed = false;
                self.update_result = None;
                self.update_message = Some(text::updates::CHECKING);
                cx.notify();
                cx.spawn_in(window, async move |list, cx| {
                    let result = cx.background_spawn(async { crate::updates::check() }).await;
                    let _ = cx.update(|window, cx| {
                        list.update(cx, |list, cx| {
                            let previous = list.delegate().selected_notification();
                            let delegate = list.delegate_mut();
                            delegate.checking_update = false;
                            delegate.update_result = Some(result);
                            delegate.update_message = Some(result.message());
                            reconcile_notification_selection(list, previous, window, cx);
                            cx.notify();
                        })
                    });
                })
                .detach();
            }
            Some(CommandAction::AboutHayai) => {
                self.show_about = true;
                cx.notify();
            }
            Some(CommandAction::RestartToUpdate) => {
                if cfg!(debug_assertions) {
                    return;
                }
                if self.restarting_update {
                    return;
                }
                self.restarting_update = true;
                cx.notify();
                cx.spawn(async move |list, cx| {
                    let ready = cx
                        .background_spawn(async { crate::updates::prepare_restart() })
                        .await;
                    if ready {
                        cx.update(|cx| cx.quit());
                    } else {
                        let _ = list.update(cx, |list, cx| {
                            let delegate = list.delegate_mut();
                            delegate.restarting_update = false;
                            delegate.update_failed = true;
                            cx.notify();
                        });
                    }
                })
                .detach();
            }
            Some(CommandAction::LaunchApplication(path)) => {
                let recents = self.catalog.borrow_mut().mark_launched_path(path.clone());
                LauncherState::dismiss(window, cx);
                cx.background_spawn(async move {
                    native::launch_path(&path);
                    crate::apps::write_recents_file(&recents);
                })
                .detach();
            }
            Some(CommandAction::OpenFile(path)) => {
                let recents = cx
                    .global::<crate::files::FileRecentsGlobal>()
                    .clone_handle();
                let contents = recents.borrow_mut().mark_opened(path.clone());
                LauncherState::dismiss(window, cx);
                cx.background_spawn(async move {
                    native::launch_path(&path);
                    crate::files::write_file_recents_file(&contents);
                })
                .detach();
            }
            Some(CommandAction::CopyToClipboard(text)) => {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                LauncherState::dismiss(window, cx);
            }
            Some(
                CommandAction::ShowText(_)
                | CommandAction::EmptyState
                | CommandAction::PendingState,
            ) => {
                // Stays on screen; no-op.
            }
            Some(CommandAction::InstallFileSearch) => {
                LauncherState::dismiss(window, cx);
                cx.background_spawn(async move {
                    native::install_scry_daemon();
                })
                .detach();
            }
            Some(CommandAction::StartFileSearch) => {
                self.start_scry(window, cx);
            }
            None => {
                if self.empty_state.is_none() {
                    LauncherState::dismiss(window, cx);
                }
            }
        }
    }
}
