use super::*;

#[derive(Clone)]
pub struct LauncherGlobal {
    state: Rc<RefCell<LauncherState>>,
}

impl LauncherGlobal {
    pub fn clone_handle(&self) -> Rc<RefCell<LauncherState>> {
        self.state.clone()
    }
}

impl gpui::Global for LauncherGlobal {}

#[derive(Default)]
pub struct LauncherState {
    pub(super) window: Option<AnyWindowHandle>,
    list: Option<Entity<ListState<ResultListDelegate>>>,
    search_input: Option<Entity<InputState>>,
    /// Retained to reset transient UI state after hiding the reused window.
    pub(super) view: Option<Entity<LauncherRoot>>,
    pub(super) visible: bool,
    reset_pending: bool,
}

impl LauncherState {
    pub fn clear_recent_history(cx: &mut App) {
        let Some(state) = cx
            .try_global::<LauncherGlobal>()
            .map(LauncherGlobal::clone_handle)
        else {
            return;
        };
        let (handle, view) = {
            let state = state.borrow();
            (state.window, state.view.clone())
        };
        if let (Some(handle), Some(view)) = (handle, view) {
            let _ = handle.update(cx, |_, window, cx| {
                view.update(cx, |view, cx| {
                    view.cancel_wheel_animation();
                    view.action_menu = None;
                    view.history.clear();
                    view.history_cursor = None;
                    let raw = view.search_input.read(cx).value().to_string();
                    let query = if view.file_mode {
                        view.build_query(&raw)
                    } else if view.waiting_for_command {
                        format!(" {raw}")
                    } else {
                        raw
                    };
                    view.list.update(cx, |list, cx| {
                        perform_search_and_select_first(list, &query, window, cx)
                    });
                    cx.notify();
                })
            });
        }
    }

    pub fn refresh_update(&self, cx: &mut App) {
        if let Some(view) = &self.view {
            // Notification arrival must not reset the query or steal selection.
            view.update(cx, |_, cx| cx.notify());
        }
    }
    pub fn install(cx: &mut App) -> Rc<RefCell<Self>> {
        let state = Rc::new(RefCell::new(Self::default()));
        cx.set_global(LauncherGlobal {
            state: state.clone(),
        });
        state
    }

    pub fn toggle(&mut self, cx: &mut App) {
        if self.visible {
            self.hide(cx);
        } else {
            self.show(cx);
        }
    }

    pub fn show(&mut self, cx: &mut App) {
        self.show_with_focus(false, cx);
    }

    pub(super) fn show_with_focus(&mut self, after_approval: bool, cx: &mut App) {
        let monitor = native::launcher_monitor(crate::settings::preferences(cx).general.display);
        let reset_view = self.reset_pending.then(|| self.view.clone()).flatten();
        self.reset_pending = false;
        if let (Some(handle), Some(search_input)) = (self.window, self.search_input.clone()) {
            // The forced platform paint re-enters GPUI through AsyncApp. Run
            // outside *all* App updates, not cx.defer (which retains the App
            // borrow and makes the frame callback silently fail try_borrow_mut).
            self.visible = true;
            cx.spawn(async move |cx| {
                let state = cx.update(|cx| cx.global::<LauncherGlobal>().clone_handle());
                if !state.borrow().visible {
                    return;
                }
                let flush = handle.update(cx, |_, window, cx| {
                    if let Some(view) = reset_view {
                        view.update(cx, |view, cx| view.reset_to_apps(window, cx));
                    }
                    window.refresh();
                    native::hidden_launcher_frame_flush(window)
                });
                let Ok(flush) = flush else {
                    return;
                };
                state.borrow_mut().visible = false;
                // No App, Window, entity, or LauncherState borrow may cross this
                // call: SendMessage synchronously invokes GPUI's frame callback.
                flush();
                #[cfg(test)]
                handle
                    .update(cx, |_, _, cx| {
                        // The heading request is consumed during prepaint, proving
                        // the reset frame actually rendered before SW_SHOW, rather
                        // than merely checking state after a later visible frame.
                        let view = state.borrow().view.clone().unwrap();
                        let root = view.read(cx);
                        let delegate = root.list.read(cx).delegate();
                        assert!(!root.file_mode && !root.waiting_for_command);
                        assert!(root.folder_scope.is_empty() && root.action_menu.is_none());
                        assert!(root.search_input.read(cx).value().is_empty());
                        assert!(
                            delegate.last_query.is_empty()
                                && !delegate.file_search
                                && delegate.apps_view
                        );
                        assert!(
                            !delegate.show_about
                                && !delegate.show_updates
                                && !delegate.show_scry_updates
                        );
                        assert!(
                            root.keyboard_heading.is_none(),
                            "reset prepaint must finish before showing the window"
                        );
                    })
                    .unwrap();
                let _ = handle.update(cx, |_, window, cx| {
                    if after_approval {
                        native::focus_launcher_after_approval(window, monitor);
                    } else {
                        native::focus_launcher_window(window, monitor);
                    }
                    // Win32 focus must precede GPUI focus (WM_SETFOCUS).
                    search_input.update(cx, |input, cx| input.focus(window, cx));
                });
                state.borrow_mut().visible = true;
            })
            .detach();
            return;
        }

        let bounds = Bounds::centered(None, size(px(LAUNCHER_WIDTH), px(LAUNCHER_HEIGHT)), cx);

        // `open_window`'s closure returns the window's root view (a `Root`), so we
        // stash the inner entities here to read back out afterward.
        type OpenedEntities = (
            Entity<ListState<ResultListDelegate>>,
            Entity<InputState>,
            Entity<LauncherRoot>,
        );
        let entities_slot: Rc<RefCell<Option<OpenedEntities>>> = Rc::new(RefCell::new(None));
        let entities_slot_for_window = entities_slot.clone();

        let handle = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: None,
                    focus: true,
                    show: true,
                    kind: WindowKind::Normal,
                    is_movable: false,
                    is_resizable: false,
                    is_minimizable: false,
                    window_background: WindowBackgroundAppearance::Opaque,
                    ..Default::default()
                },
                move |window, cx| {
                    let catalog = cx.global::<CatalogGlobal>().clone_handle();
                    let registry = cx.global::<crate::plugins::RegistryGlobal>().clone_handle();
                    let list = cx.new(|cx| {
                        ListState::new(ResultListDelegate::new(registry, catalog), window, cx)
                            .searchable(false)
                    });
                    list.update(cx, |list, cx| {
                        perform_search_and_select_first(list, "", window, cx);
                    });
                    let search_input = cx.new(|cx| {
                        InputState::new(window, cx).placeholder(text::search::APPS_PLACEHOLDER)
                    });
                    let view = cx.new(|cx| {
                        LauncherRoot::new(list.clone(), search_input.clone(), window, cx)
                    });

                    *entities_slot_for_window.borrow_mut() =
                        Some((list.clone(), search_input.clone(), view.clone()));

                    // Close on focus loss (clicking elsewhere, Alt+Tab, etc.).
                    // The observer fires for both activation and deactivation, so we
                    // only act on deactivation. We also gate on our own `visible`
                    // flag: hiding the window (SW_HIDE) itself deactivates it and
                    // re-enters this observer, so without the guard we'd recurse.
                    view.update(cx, |_view, cx| {
                        cx.observe_window_activation(window, |view, window, cx| {
                            // Windows activation is delivered asynchronously; an old hide
                            // event must not dismiss a window that already regained focus.
                            if window.is_window_active() || native::launcher_has_foreground(window)
                            {
                                return;
                            }
                            view.action_menu = None;
                            cx.notify();
                            let visible = cx
                                .global::<LauncherGlobal>()
                                .clone_handle()
                                .borrow()
                                .visible;
                            if visible {
                                LauncherState::dismiss(window, cx);
                            }
                        })
                        .detach();
                    });

                    cx.new(|cx| Root::new(view, window, cx))
                },
            )
            .expect("failed to open launcher window");

        let (list, search_input, view) = entities_slot
            .borrow_mut()
            .take()
            .expect("entities created during open_window");

        // Win32 focus first, then GPUI focus (same ordering as the re-show path above).
        let _ = handle.update(cx, |_, window, cx| {
            native::focus_launcher_window(window, monitor);
            search_input.update(cx, |input, cx| input.focus(window, cx));
        });

        self.window = Some(handle.into());
        self.list = Some(list);
        self.search_input = Some(search_input);
        self.view = Some(view);
        self.visible = true;
    }

    /// Re-run the current query against the (possibly just-rescanned) catalog.
    /// Called from a bare `&mut App` context (the scan-completion task), which
    /// is NOT mid-window-update, so `handle.update` is safe here (pitfall #1).
    pub fn refresh_results(&self, cx: &mut App) {
        let (Some(handle), Some(list), Some(search_input)) =
            (self.window, self.list.clone(), self.search_input.clone())
        else {
            return;
        };
        let _ = handle.update(cx, |_, window, cx| {
            let query = search_input.read(cx).value().to_string();
            list.update(cx, |list, cx| {
                perform_search_and_select_first(list, &query, window, cx);
            });
        });
    }

    pub fn hide(&mut self, cx: &mut App) {
        let Some(handle) = self.window else {
            return;
        };

        // Mark hidden *before* touching the OS window: SW_HIDE deactivates the
        // window, which fires our activation observer; the `visible` flag is how
        // that observer knows not to re-dismiss.
        self.visible = false;
        self.reset_pending = true;
        let view = self.view.clone();
        let _ = handle.update(cx, |_, window, cx| {
            native::hide_launcher_window(window);
            Self::reset_after_hide(view, window, cx);
        });
    }

    /// Dismiss the launcher from *inside* a window update (e.g. a list delegate
    /// callback or a window-activation observer).
    ///
    /// We hide rather than destroy the window: on Windows GPUI quits the whole app
    /// when its last window closes, but we want to stay resident in the tray.
    ///
    /// We also must not re-enter `handle.update` here: callbacks and observers
    /// already run while the window has been taken out of `App::windows`, so a
    /// nested update would fail with "window not found" and silently do nothing.
    /// Instead we hide through the `Window` we already hold and update the shared
    /// `visible` flag directly.
    pub(super) fn dismiss(window: &mut Window, cx: &mut App) {
        let state = cx.global::<LauncherGlobal>().clone_handle();
        let view = {
            let mut state = state.borrow_mut();
            state.visible = false;
            state.reset_pending = true;
            state.view.clone()
        };
        native::hide_launcher_window(window);
        Self::reset_after_hide(view, window, cx);
    }

    fn reset_after_hide(view: Option<Entity<LauncherRoot>>, window: &mut Window, cx: &mut App) {
        let Some(view) = view else { return };
        // Dismiss can run inside this view's or its list's update. Defer until
        // those borrows unwind; never re-enter the same entity/window update.
        // No timer is needed: SW_HIDE has already removed the old frame.
        window.defer(cx, move |window, cx| {
            {
                let state = cx.global::<LauncherGlobal>().clone_handle();
                let mut state = state.borrow_mut();
                if state.visible || !state.reset_pending {
                    return;
                }
                state.reset_pending = false;
            }
            view.update(cx, |view, cx| view.reset_to_apps(window, cx));
            window.refresh();
            native::render_hidden_launcher_frame(window);
        });
    }
}
