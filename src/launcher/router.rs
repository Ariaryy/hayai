use super::*;
use crate::command_router::Command;

impl LauncherRoot {
    pub(super) fn route_command(&mut self, raw: &str, window: &mut Window, cx: &mut Context<Self>) {
        // Filtering changes which rows exist, so selection restarts on the first.
        self.picker_selected = 0;
        let typed = if self.waiting_for_command {
            raw
        } else {
            self.waiting_for_command = true;
            self.search_input.update(cx, |input, cx| {
                // Strip only the entry sentinel. Replacing the value on later
                // edits would move the caret while correcting a partial alias.
                input.set_value(&raw[1..], window, cx);
                input.set_placeholder(text::search::COMMAND_PLACEHOLDER, window, cx);
            });
            self.refresh_command_results(window, cx);
            &raw[1..]
        };
        if let Some((command, remainder)) = crate::settings::preferences(cx).commands.resolve(typed)
        {
            let remainder = remainder.to_owned();
            self.activate_command(command, &remainder, window, cx);
        } else if !typed.is_empty()
            && !crate::settings::preferences(cx).show_command_picker
            && !crate::settings::preferences(cx).commands.could_match(typed)
        {
            // Without the picker nothing explains a stalled command prompt, so
            // text that can no longer be a command searches normally.
            let query = typed.to_owned();
            self.leave_command_for_search(&query, window, cx);
        }
        cx.notify();
    }

    fn leave_command_for_search(
        &mut self,
        query: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.waiting_for_command = false;
        self.search_input.update(cx, |input, cx| {
            input.set_placeholder(text::search::APPS_PLACEHOLDER, window, cx);
        });
        self.list.update(cx, |list, cx| {
            perform_search_and_select_first(list, query, window, cx);
        });
    }

    pub(super) fn refresh_command_results(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let show_picker = crate::settings::preferences(cx).show_command_picker;
        self.list.update(cx, |list, cx| {
            if show_picker {
                // Invalidate pending work without dispatching a partial alias
                // to providers or retaining app rows behind the picker.
                let delegate = list.delegate_mut();
                delegate.registry.borrow_mut().invalidate();
                delegate.search_pending = false;
                delegate.results.clear();
                delegate.main_results = 0;
                delegate.recent_results = 0;
                delegate.file_search = false;
                delegate.apps_view = false;
                delegate.last_query = " ".into();
                delegate.empty_state = None;
                delegate.show_about = false;
                delegate.show_updates = false;
                delegate.show_scry_updates = false;
            } else {
                // Keep the normal launcher view when suggestions are hidden.
                // Dispatch only the empty apps query, never a partial alias.
                perform_search_and_select_first(list, "", window, cx);
            }
            list.set_selected_index(None, window, cx);
            list.delegate_mut().notification_selected = false;
            cx.notify();
        });
        cx.notify();
    }

    pub(super) fn activate_command(
        &mut self,
        command: Command,
        remainder: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let previous_picker = ((self.waiting_for_command
            && crate::settings::preferences(cx).show_command_picker)
            || self.pending_picker.is_some())
        .then(|| self.picker_commands(cx));
        self.pending_picker = None;
        self.cancel_wheel_animation();
        self.action_menu = None;
        match command {
            Command::Files => self.enter_file_mode(&format!(" f {remainder}"), window, cx),
            Command::Calculator => {
                self.waiting_for_command = false;
                self.file_mode = false;
                self.calculator_mode = true;
                self.folder_scope.clear();
                self.search_input.update(cx, |input, cx| {
                    input.set_value(remainder, window, cx);
                    input.set_placeholder(text::search::CALCULATOR_PLACEHOLDER, window, cx);
                });
                self.list.update(cx, |list, cx| {
                    perform_search_and_select_first(list, &format!(" = {remainder}"), window, cx)
                });
            }
            Command::Settings => {
                self.reset_to_apps(window, cx);
                LauncherState::dismiss(window, cx);
                // Activation can re-enter the launcher. Let this update unwind.
                cx.defer(crate::settings::open);
            }
        }
        let delegate = self.list.read(cx).delegate();
        if delegate.search_pending && delegate.results.is_empty() {
            self.pending_picker = previous_picker;
        }
    }

    /// Commands the picker lists for the text typed so far.
    pub(super) fn picker_commands(&self, cx: &App) -> Vec<Command> {
        if let Some(commands) = &self.pending_picker {
            return commands.clone();
        }
        let aliases = &crate::settings::preferences(cx).commands;
        let typed = self.search_input.read(cx).value().to_ascii_lowercase();
        Command::ALL
            .into_iter()
            .filter(|command| {
                aliases
                    .entries(*command)
                    .any(|name| name.starts_with(&typed))
            })
            .collect()
    }

    /// Up/Down inside the picker, wrapping at both ends.
    pub(super) fn move_picker_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        if !crate::settings::preferences(cx).show_command_picker {
            return;
        }
        let count = self.picker_commands(cx).len() as isize;
        if count > 0 {
            self.picker_selected =
                (self.picker_selected as isize + delta).rem_euclid(count) as usize;
            cx.notify();
        }
    }

    /// Enter inside the picker opens the highlighted command.
    pub(super) fn confirm_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !crate::settings::preferences(cx).show_command_picker {
            return;
        }
        if let Some(command) = self.picker_commands(cx).get(self.picker_selected).copied() {
            self.activate_command(command, "", window, cx);
            cx.notify();
        }
    }

    pub(super) fn command_picker(&self, cx: &Context<Self>) -> AnyElement {
        let aliases = &crate::settings::preferences(cx).commands;
        let mut body = div().flex_1().min_h_0().p_2().flex().flex_col();
        body = body.child(
            div()
                .h(px(24.0))
                .flex_none()
                .flex()
                .items_center()
                .px_1()
                .child(section_heading(text::search::COMMANDS, cx)),
        );
        let visible = self.picker_commands(cx);
        let selected = self.picker_selected.min(visible.len().saturating_sub(1));
        let mut count = 0;
        for (index, command) in visible.into_iter().enumerate() {
            count += 1;
            body = body.child(
                h_flex()
                    .id(command.id())
                    // Same height and radius as an app result row.
                    .h(px(36.0))
                    .px_3()
                    .gap_3()
                    .rounded(cx.theme().radius)
                    .when(index == selected, |row| row.bg(cx.theme().list_active))
                    .hover(|row| row.bg(cx.theme().list_hover))
                    .child(
                        gpui::svg()
                            .path(command.icon())
                            .size(px(20.0))
                            .text_color(cx.theme().muted_foreground),
                    )
                    .child(div().flex_1().child(text::settings::command_label(command)))
                    .child(crate::settings::trigger_badge(aliases.trigger(command), cx))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            this.activate_command(command, "", window, cx);
                            cx.notify();
                        }),
                    ),
            );
        }
        if count == 0 {
            return super::components::empty_screen(
                icon_assets::SEARCH_EMPTY,
                text::search::NO_COMMANDS,
                cx,
            )
            .into_any_element();
        }
        body.into_any_element()
    }
}
