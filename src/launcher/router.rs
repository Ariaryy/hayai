use super::*;
use crate::command_router::Command;

impl LauncherRoot {
    pub(super) fn route_command(&mut self, raw: &str, window: &mut Window, cx: &mut Context<Self>) {
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
            // Invalidate pending work without dispatching a partial alias to
            // providers or retaining selectable app rows behind the picker.
            self.list.update(cx, |list, cx| {
                let delegate = list.delegate_mut();
                delegate.registry.borrow_mut().invalidate();
                delegate.search_pending = false;
                delegate.results.clear();
                delegate.main_results = 0;
                delegate.file_search = false;
                delegate.apps_view = false;
                delegate.last_query = " ".into();
                delegate.empty_state = None;
                delegate.show_about = false;
                delegate.show_updates = false;
                delegate.show_scry_updates = false;
                list.set_selected_index(None, window, cx);
                cx.notify();
            });
            &raw[1..]
        };
        if let Some((command, remainder)) = crate::settings::preferences(cx).commands.resolve(typed)
        {
            let remainder = remainder.to_owned();
            self.activate_command(command, &remainder, window, cx);
        }
        cx.notify();
    }

    fn activate_command(
        &mut self,
        command: Command,
        remainder: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
    }

    pub(super) fn command_picker(&self, cx: &Context<Self>) -> AnyElement {
        let aliases = &crate::settings::preferences(cx).commands;
        let typed = self.search_input.read(cx).value().to_ascii_lowercase();
        let mut body = div().flex_1().min_h_0().p_2().flex().flex_col().gap_1();
        let mut count = 0;
        for command in Command::ALL {
            if !aliases
                .names(command)
                .iter()
                .any(|name| name.starts_with(&typed))
            {
                continue;
            }
            count += 1;
            let triggers = aliases.triggers(command);
            body = body.child(
                h_flex()
                    .id(command.id())
                    .min_h(px(44.0))
                    .py_2()
                    .px_3()
                    .gap_3()
                    .rounded(px(6.0))
                    .cursor_pointer()
                    .hover(|row| row.bg(cx.theme().list_hover))
                    .child(
                        gpui::svg()
                            .path(match command {
                                Command::Files => icon_assets::SETTINGS_FILES,
                                Command::Settings => icon_assets::SETTINGS_GENERAL,
                                Command::Calculator => icon_assets::CALCULATOR,
                            })
                            .size(px(18.0))
                            .text_color(cx.theme().muted_foreground),
                    )
                    .child(div().flex_1().child(text::settings::command_label(command)))
                    .child(
                        h_flex()
                            .max_w(px(300.0))
                            .flex_wrap()
                            .justify_end()
                            .gap_2()
                            .children(
                                triggers
                                    .into_iter()
                                    .map(|trigger| crate::settings::command_badges(trigger, cx)),
                            ),
                    )
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
            body = body.child(
                div()
                    .p_3()
                    .text_color(cx.theme().muted_foreground)
                    .child(text::search::NO_COMMANDS),
            );
        }
        body.into_any_element()
    }
}
