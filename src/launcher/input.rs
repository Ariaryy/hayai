use super::*;

impl LauncherRoot {
    pub(super) fn on_search_event(
        &mut self,
        input: &Entity<InputState>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Change => {
                self.pending_folder_footer = None;
                self.cancel_wheel_animation();
                self.action_menu = None;
                // Only real user edits reach this arm — our own history-recall
                // `set_value` calls below drive `perform_search_and_select_first`
                // directly and don't emit `Change`, so this can't clobber a
                // recall in progress.
                self.history_cursor = None;
                let raw = input.read(cx).value().to_string();
                if !self.file_mode
                    && !self.calculator_mode
                    && (self.waiting_for_command || raw.starts_with(' '))
                {
                    self.route_command(&raw, window, cx);
                    return;
                }
                let query = if self.file_mode {
                    self.build_query(&raw)
                } else if self.calculator_mode {
                    format!(" = {raw}")
                } else {
                    raw
                };
                self.list.update(cx, |list, cx| {
                    perform_search_and_select_first(list, &query, window, cx);
                });
            }
            InputEvent::PressEnter { .. } => {
                // Partial/unknown aliases must never confirm old app results.
                if self.waiting_for_command {
                    self.confirm_picker(window, cx);
                    return;
                }
                if self.list.read(cx).delegate().empty_state.is_some() {
                    return;
                }
                if self.list.read(cx).delegate().show_about {
                    self.list.update(cx, |list, cx| {
                        list.delegate_mut().show_about = false;
                        cx.notify();
                    });
                    return;
                }
                if self.action_menu.is_some() {
                    if let Some(action) = self.selected_menu_action() {
                        self.run_menu_action(action, window, cx);
                    }
                    return;
                }
                let text = input.read(cx).value().to_string();
                if !text.is_empty()
                    && self.history.first().map(String::as_str) != Some(text.as_str())
                {
                    self.history.insert(0, text);
                    self.history.truncate(HISTORY_LIMIT);
                }
                self.history_cursor = None;
                self.list.update(cx, |list, cx| {
                    list.delegate_mut().confirm(false, window, cx);
                });
            }
            _ => {}
        }
    }

    /// Tab on a folder result enters it: scope subsequent queries under it
    /// (via the scoped-search form in `build_query`) without
    /// leaving the launcher, so the same key can descend further into
    /// nested folders.
    ///
    /// Called from the app-wide `intercept_keystrokes` hook registered in
    /// `new` — `InputState` consumes Backspace and Tab itself as key
    /// bindings (with no `cx.propagate()`), so an ordinary `on_key_down`
    /// listener on an ancestor div never sees them while the input holds
    /// focus. Interception runs before action/keymap dispatch entirely, so
    /// it's the only hook that can steal these two keys away first.
    pub(super) fn on_intercepted_key(
        &mut self,
        key: &str,
        toggle_actions: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.waiting_for_command && toggle_actions {
            return;
        }
        if toggle_actions {
            cx.stop_propagation();
            self.action_menu = if self.action_menu.is_some() {
                None
            } else {
                self.selected_action_menu(cx)
            };
            cx.notify();
            return;
        }
        if self.waiting_for_command
            && key == "backspace"
            && self.search_input.read(cx).value().is_empty()
        {
            cx.stop_propagation();
            self.reset_to_apps(window, cx);
            return;
        }
        if self.calculator_mode
            && key == "backspace"
            && self.search_input.read(cx).value().is_empty()
        {
            cx.stop_propagation();
            self.reset_to_apps(window, cx);
            return;
        }
        if !self.file_mode {
            return;
        }

        // Backspace on an already-empty box steps back out instead of
        // being a no-op: pop the folder scope first if we're inside one
        // (mirrors Escape), only exiting file mode entirely once there's
        // no scope left to back out of.
        if key == "backspace" && self.search_input.read(cx).value().is_empty() {
            cx.stop_propagation();
            if self.folder_scope.is_empty() {
                self.exit_file_mode(window, cx);
            } else {
                self.pop_folder_scope(window, cx);
            }
            return;
        }

        if key != "tab" {
            return;
        }
        if self.list.read(cx).delegate().search_pending {
            cx.stop_propagation();
            return;
        }
        let selected_dir = self.list.read(cx).selected_index().and_then(|ix| {
            self.list
                .read(cx)
                .delegate()
                .results
                .get(self.list.read(cx).delegate().flat_index(ix))
                .and_then(|item| match &item.action {
                    CommandAction::OpenFile(path) if path.is_dir() => Some(path.clone()),
                    _ => None,
                })
        });
        let Some(dir) = selected_dir else {
            return;
        };
        cx.stop_propagation();
        self.folder_scope.push(dir);
        self.refresh_folder_scope(window, cx);
    }

    pub(super) fn move_selection(
        &mut self,
        delta: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pending_folder_footer = None;
        if self.action_menu.take().is_some() {
            cx.notify();
        }
        self.keyboard_heading = self.list.update(cx, |list, cx| {
            let count = list.delegate().results.len();
            if list.delegate().notification_visible() {
                let notices = list.delegate().notification_count();
                let current = if list.delegate().notification_selected {
                    list.selected_index().map_or(0, |ix| ix.row)
                } else {
                    list.selected_index()
                        .map(|ix| list.delegate().flat_index(ix) + notices)
                        .unwrap_or(0)
                };
                let next =
                    (current as isize + delta).clamp(0, (count + notices - 1) as isize) as usize;
                list.set_selected_index(
                    Some(
                        next.checked_sub(notices)
                            .map(|flat| list.delegate().index_path(flat))
                            .unwrap_or(IndexPath {
                                section: 0,
                                row: next,
                                ..Default::default()
                            }),
                    ),
                    window,
                    cx,
                );
                list.delegate_mut().notification_selected = next < notices;
                list.scroll_to_selected_item(window, cx);
                cx.notify();
                return list
                    .selected_index()
                    .filter(|ix| ix.row == 0)
                    .map(|ix| (ix, list.delegate().results_generation));
            }
            if count == 0 {
                return None;
            }
            let current = list
                .selected_index()
                .map(|ix| list.delegate().flat_index(ix) as isize)
                .unwrap_or(-1);
            let next = (current + delta).clamp(0, count as isize - 1) as usize;
            let ix = list.delegate().index_path(next);
            list.set_selected_index(Some(ix), window, cx);
            list.scroll_to_selected_item(window, cx);
            (ix.row == 0).then_some((ix, list.delegate().results_generation))
        });
    }

    pub(super) fn reveal_keyboard_heading(&mut self, cx: &mut Context<Self>) {
        let Some((selected, generation)) = self.keyboard_heading.take() else {
            return;
        };
        let list = self.list.read(cx);
        if list.selected_index() != Some(selected)
            || list.delegate().results_generation != generation
        {
            return;
        }
        // The pinned List includes a header and footer entry for each nonempty
        // section, including zero-height footers. Target the header, not row 0.
        let header = section_header_entry(
            (0..selected.section).map(|section| list.delegate().items_count(section, cx)),
        );
        list.scroll_handle()
            .scroll_to_item(header, gpui::ScrollStrategy::Top);
    }

    pub(super) fn on_move_up(&mut self, _: &MoveUp, window: &mut Window, cx: &mut Context<Self>) {
        if self.waiting_for_command {
            self.move_picker_selection(-1, cx);
            return;
        }
        self.cancel_wheel_animation();
        if self.list.read(cx).delegate().show_about
            || self.list.read(cx).delegate().show_updates
            || self.list.read(cx).delegate().show_scry_updates
        {
            return;
        }
        if self.move_menu_selection(-1, cx) {
            return;
        }
        if self.list.read(cx).delegate().notification_visible() {
            if self.list.read(cx).delegate().notification_selected
                && self
                    .list
                    .read(cx)
                    .selected_index()
                    .is_some_and(|ix| ix.row == 0)
                && self.search_input.read(cx).value().is_empty()
            {
                if !self.recall_history(1, window, cx) {
                    self.move_selection(0, window, cx);
                }
                return;
            }
            self.move_selection(-1, window, cx);
            return;
        }
        let at_top = self
            .list
            .read(cx)
            .selected_index()
            .map(|ix| self.list.read(cx).delegate().flat_index(ix))
            .unwrap_or(0)
            == 0;
        if self.search_input.read(cx).value().is_empty() && at_top {
            // Empty query *and* already on the top result: Up is history
            // recall (even when there's no history yet, so it never falls
            // through to `move_selection`, which would wrap Up on the first
            // row around to the last row and look like backwards nav).
            // Below the top result, Up should just move the selection up
            // like normal list navigation.
            if !self.recall_history(1, window, cx) {
                self.move_selection(0, window, cx);
            }
            return;
        }
        self.move_selection(-1, window, cx);
    }

    pub(super) fn on_move_down(
        &mut self,
        _: &MoveDown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.waiting_for_command {
            self.move_picker_selection(1, cx);
            return;
        }
        self.cancel_wheel_animation();
        if self.list.read(cx).delegate().show_about
            || self.list.read(cx).delegate().show_updates
            || self.list.read(cx).delegate().show_scry_updates
        {
            return;
        }
        if self.move_menu_selection(1, cx) {
            return;
        }
        if self.history_cursor.is_some()
            && self.search_input.read(cx).value().is_empty()
            && self.recall_history(-1, window, cx)
        {
            return;
        }
        self.move_selection(1, window, cx);
    }

    /// Shell-history-style Up/Down: `delta = 1` steps to an older query,
    /// `delta = -1` to a newer one (or back to empty). Only engages while
    /// the search box is empty (see callers), so it never fights normal
    /// list navigation once the user is typing or browsing results.
    /// Returns `false` when there's nowhere to go, so the caller can fall
    /// back to ordinary list-selection movement.
    fn recall_history(
        &mut self,
        delta: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.waiting_for_command {
            return false;
        }
        let next = match (self.history_cursor, delta) {
            (None, 1) if !self.history.is_empty() => Some(0),
            (Some(i), 1) if i + 1 < self.history.len() => Some(i + 1),
            (Some(i), -1) if i > 0 => Some(i - 1),
            (Some(_), -1) => None,
            _ => return false,
        };
        self.history_cursor = next;
        self.pending_folder_footer = None;
        let text = next
            .and_then(|i| self.history.get(i))
            .cloned()
            .unwrap_or_default();
        self.search_input
            .update(cx, |input, cx| input.set_value(&text, window, cx));
        let query = if self.file_mode {
            self.build_query(&text)
        } else if self.calculator_mode {
            format!(" = {text}")
        } else {
            text
        };
        self.list.update(cx, |list, cx| {
            perform_search_and_select_first(list, &query, window, cx);
        });
        true
    }

    pub(super) fn on_escape(&mut self, _: &Escape, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(menu) = self.action_menu.as_mut()
            && menu.return_to_parent()
        {
            cx.notify();
            return;
        }
        if self.action_menu.take().is_some() {
            cx.notify();
            return;
        }
        if self.list.read(cx).delegate().show_about
            || self.list.read(cx).delegate().show_updates
            || self.list.read(cx).delegate().show_scry_updates
        {
            self.list.update(cx, |list, cx| {
                list.delegate_mut().show_updates = false;
                list.delegate_mut().show_scry_updates = false;
                list.delegate_mut().show_about = false;
                cx.notify();
            });
            return;
        }
        // Escape backs out one level at a time: folder scope first (same
        // stack as Backspace), then file mode entirely, then a non-empty
        // query just gets cleared, and only once all of those are already
        // clear does it fall through to dismissing the launcher.
        if !self.folder_scope.is_empty() {
            self.pop_folder_scope(window, cx);
            return;
        }
        if self.file_mode || self.calculator_mode || self.waiting_for_command {
            self.exit_file_mode(window, cx);
            return;
        }
        if !self.search_input.read(cx).value().is_empty() {
            self.history_cursor = None;
            self.search_input
                .update(cx, |input, cx| input.set_value("", window, cx));
            self.list.update(cx, |list, cx| {
                perform_search_and_select_first(list, "", window, cx);
            });
            return;
        }
        self.list.update(cx, |list, cx| {
            list.delegate_mut().cancel(window, cx);
        });
    }

    pub(super) fn intercept_keys(window: &Window, cx: &mut Context<Self>) -> Subscription {
        let this = cx.entity();
        let owner = window.window_handle();
        // `InputState` owns Backspace and Tab as key *bindings* (its "Input"
        // key context is more specific than any context we could put on an
        // ancestor div), so a normal `on_key_down`/`on_action` listener up
        // here never sees them once InputState's handler consumes the event.
        // `intercept_keystrokes` runs before action dispatch entirely (see
        // its doc comment), so it's the only hook that can steal these two
        // keys away from the input while it still holds focus.
        cx.intercept_keystrokes(move |event, window, cx| {
            if window.window_handle() != owner {
                return;
            }
            let key = event.keystroke.key.as_str();
            let preferences = crate::settings::preferences(cx);
            let toggle_actions =
                preferences.matches(crate::settings::Shortcut::Actions, &event.keystroke);
            let files =
                preferences.matches(crate::settings::Shortcut::FileSearch, &event.keystroke);
            let settings =
                preferences.matches(crate::settings::Shortcut::Settings, &event.keystroke);
            if files || settings {
                cx.stop_propagation();
                this.update(cx, |this, cx| {
                    if files {
                        this.cancel_wheel_animation();
                        this.action_menu = None;
                        this.enter_file_mode(" f", window, cx);
                        cx.notify();
                    } else {
                        LauncherState::dismiss(window, cx);
                        cx.defer(crate::settings::open);
                    }
                });
                return;
            }
            let action_number = if event.keystroke.modifiers == gpui::Modifiers::default() {
                match key {
                    "1" => Some(0),
                    "2" => Some(1),
                    "3" => Some(2),
                    "4" => Some(3),
                    "5" => Some(4),
                    "6" => Some(5),
                    "7" => Some(6),
                    _ => None,
                }
            } else {
                None
            };
            if key != "backspace" && key != "tab" && !toggle_actions && action_number.is_none() {
                return;
            }
            this.update(cx, |this, cx| {
                // Intercept before InputState inserts the digit. Outside the
                // panel, number keys must remain ordinary search text.
                if let Some(index) = action_number
                    && let Some(menu) = this.action_menu.as_mut()
                {
                    cx.stop_propagation();
                    if index < menu.count() {
                        menu.selected = index;
                        if let Some(action) = this.selected_menu_action() {
                            this.run_menu_action(action, window, cx);
                        }
                    }
                    return;
                }
                this.on_intercepted_key(key, toggle_actions, window, cx);
            });
        })
    }
}
