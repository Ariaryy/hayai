use super::*;

impl ResultListDelegate {
    pub(in crate::launcher) fn check_scry_updates(
        &mut self,
        force: bool,
        window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) {
        let interval = if self
            .scry_discovery
            .as_ref()
            .is_some_and(|result| result.state == crate::scry_updates::State::Failed)
        {
            Duration::from_secs(15 * 60)
        } else {
            Duration::from_secs(24 * 60 * 60)
        };
        if self.scry_checking
            || self.scry_installing
            || (!force
                && self
                    .scry_checked
                    .is_some_and(|time| time.elapsed() < interval))
        {
            return;
        }
        self.scry_checking = true;
        self.scry_install_attempted = false;
        if force {
            self.scry_notice = true;
            self.scry_update_dismissed = false;
        }
        self.scry_checked = Some(std::time::Instant::now());
        self.scry_ready = false;
        self.scry_message = text::updates::SCRY_CHECKING_UPDATES.into();
        cx.spawn_in(window, async move |list, cx| {
            let (result, ready) = cx
                .background_spawn(async move {
                    let result = crate::scry_updates::check(force);
                    let ready = result.state == crate::scry_updates::State::Current
                        && native::scry_compatibility() == crate::scry::Compatibility::Ready;
                    (result, ready)
                })
                .await;
            let _ = cx.update(|window, cx| {
                list.update(cx, |list, cx| {
                    let previous = list.delegate().selected_notification();
                    let delegate = list.delegate_mut();
                    delegate.scry_checking = false;
                    if !delegate.scry_install_attempted {
                        delegate.scry_ready = ready;
                        delegate.scry_message = result.message.clone();
                        delegate.scry_discovery = Some(result);
                    } else {
                        // Discovery started before installation describes the
                        // previous daemon. It cannot override activation results.
                        delegate.scry_discovery = None;
                        delegate.scry_checked = None;
                    }
                    reconcile_notification_selection(list, previous, window, cx);
                    cx.notify();
                })
            });
        })
        .detach();
        cx.notify();
    }

    pub(in crate::launcher) fn update_scry(&mut self, cx: &mut Context<ListState<Self>>) {
        if self.scry_installing || (self.scry_checking && !self.needs_pinned_scry()) {
            return;
        }
        let Some(target) = self.scry_target().map(str::to_owned) else {
            return;
        };
        self.scry_installing = true;
        self.scry_install_target = Some(target.clone());
        self.scry_install_attempted = true;
        self.scry_update_dismissed = false;
        self.scry_ready = false;
        self.scry_message = text::updates::SCRY_PREPARING.into();
        cx.spawn(async move |list, cx| {
            let (mut send, mut receive) = futures::channel::mpsc::channel(8);
            let job = cx.background_spawn(async move {
                crate::scry_updates::install(&target, |message| {
                    let _ = send.try_send(message);
                })
            });
            let mut restored_after_approval = false;
            while let Some(progress) = receive.next().await {
                let restore = progress.active && !restored_after_approval;
                let _ = list.update(cx, |list, cx| {
                    list.delegate_mut().scry_message = progress.message;
                    cx.notify();
                });
                if restore {
                    restored_after_approval = true;
                    // This runs after the entity update has unwound, so show
                    // can safely enter the window update and restore focus.
                    cx.update(|cx| {
                        let state = cx.global::<LauncherGlobal>().clone_handle();
                        state.borrow_mut().show_with_focus(true, cx);
                    });
                }
            }
            let result = job.await;
            let _ = list.update(cx, |list, cx| {
                let delegate = list.delegate_mut();
                delegate.scry_installing = false;
                delegate.scry_ready = result.is_ok();
                delegate.scry_message = result.unwrap_or_else(|message| message);
                // A failure/recovery may have changed the installed version.
                // Discover again before another installation attempt.
                delegate.scry_discovery = None;
                delegate.scry_checked = None;
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    pub(in crate::launcher) fn start_scry(
        &mut self,
        window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) {
        if self.starting_scry {
            return;
        }
        let dispatch = self.registry.borrow_mut().dispatch(&self.last_query);
        let generation = dispatch.generation;
        let registry = self.registry.clone();
        self.starting_scry = true;
        cx.notify();
        cx.spawn_in(window, async move |list, cx| {
            let started = cx
                .background_spawn(async { native::start_scry_daemon() })
                .await;
            // The query generation also changes on hide/reset. A pending start
            // must never restore a file query over a newer query or session.
            if registry.borrow().current_generation() != generation {
                return;
            }
            if started {
                let deadline = std::time::Instant::now() + Duration::from_secs(20);
                while std::time::Instant::now() < deadline {
                    if registry.borrow().current_generation() != generation {
                        return;
                    }
                    let job = registry.borrow().background_search(&dispatch);
                    let Some(job) = job else {
                        break;
                    };
                    let items = cx.background_spawn(async move { job() }).await;
                    if registry.borrow().current_generation() != generation {
                        return;
                    }
                    // An empty result set is a successful search, too. Setup
                    // rows mean the daemon is still unavailable, not ready.
                    if scry_search_is_ready(&items) {
                        let _ = cx.update(|window, cx| {
                            list.update(cx, |list, cx| {
                                let delegate = list.delegate_mut();
                                delegate.starting_scry = false;
                                delegate.apply_results(generation, items, cx);
                                refresh_result_layout(list, window, cx);
                            })
                        });
                        return;
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(250))
                        .await;
                }
            }
            if registry.borrow().current_generation() != generation {
                return;
            }
            let _ = list.update(cx, |list, cx| {
                let delegate = list.delegate_mut();
                delegate.starting_scry = false;
                if let Some(item) = delegate.results.first_mut() {
                    item.title = text::search::START_FAILED.into();
                    item.subtitle = Some(text::search::START_RETRY.into());
                }
                cx.notify();
            });
        })
        .detach();
    }
}
