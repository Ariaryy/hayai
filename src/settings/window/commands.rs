use super::*;
use crate::command_router::Command;
use gpui::Entity;
use gpui_component::{
    Sizable,
    input::{Input, InputEvent, InputState},
    switch::Switch,
};

/// Chips, the trigger and the add field share one height so rows stay level.
const INPUT_HEIGHT: f32 = 24.0;

pub(super) struct AliasEditor {
    pub(super) triggers: [Entity<InputState>; Command::ALL.len()],
    /// Pending text for a new alias; committed aliases render as chips.
    pub(super) additions: [Entity<InputState>; Command::ALL.len()],
    pub(super) error: Option<(Command, String)>,
    _subscriptions: Vec<Subscription>,
}

impl AliasEditor {
    pub(super) fn new(window: &mut Window, cx: &mut Context<SettingsView>) -> Self {
        let triggers = Command::ALL.map(|command| {
            let value = preferences(cx).commands.trigger(command).to_owned();
            cx.new(|cx| InputState::new(window, cx).default_value(value))
        });
        let additions = Command::ALL
            .map(|_| cx.new(|cx| InputState::new(window, cx).placeholder(text::ADD_ALIAS)));
        let subscriptions = Command::ALL
            .into_iter()
            .flat_map(|command| {
                [&triggers[command as usize], &additions[command as usize]]
                    .into_iter()
                    .map(|input| {
                        cx.subscribe_in(input, window, move |this, input, event, window, cx| {
                            if !matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                                return;
                            }
                            if this.commit_routes(Some(command), cx) {
                                let is_addition =
                                    this.alias_editor.as_ref().is_some_and(|editor| {
                                        editor.additions[command as usize] == *input
                                    });
                                if is_addition {
                                    input.update(cx, |input, cx| input.set_value("", window, cx));
                                }
                            }
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        Self {
            triggers,
            additions,
            error: None,
            _subscriptions: subscriptions,
        }
    }
}

fn conflict_message(
    aliases: &crate::command_router::Aliases,
    command: Command,
    entry: &str,
) -> Option<String> {
    let (other, theirs) = aliases.conflict(command, entry)?;
    let kind = if aliases.trigger(other) == theirs {
        "trigger"
    } else {
        "alias"
    };
    Some(text::route_conflict(other, theirs, kind))
}

impl SettingsView {
    pub(super) fn commit_aliases(&mut self, cx: &mut Context<Self>) -> bool {
        self.commit_routes(None, cx)
    }

    /// Format errors stay in the editor and block closing; overlaps between
    /// commands are valid to save and are flagged on the page instead. All
    /// requested commands are applied as one edit, so there is a single save.
    fn commit_routes(&mut self, only: Option<Command>, cx: &mut Context<Self>) -> bool {
        let Some(editor) = &mut self.alias_editor else {
            return true;
        };
        let mut next = preferences(cx).clone();
        for command in Command::ALL
            .into_iter()
            .filter(|command| only.is_none_or(|only| only == *command))
        {
            let trigger = editor.triggers[command as usize].read(cx).value();
            let added = editor.additions[command as usize].read(cx).value();
            let result = next.commands.set_trigger(command, &trigger).and_then(|()| {
                if added.trim().is_empty() {
                    return Ok(());
                }
                let all = [next.commands.aliases(command).join(","), added.to_string()];
                next.commands.set_aliases(command, &all.join(","))
            });
            if let Err(error) = result {
                editor.error = Some((command, error.to_string()));
                cx.notify();
                return false;
            }
        }
        editor.error = None;
        if next != *preferences(cx) {
            save(next, cx);
        }
        cx.notify();
        true
    }

    fn remove_alias(&mut self, command: Command, alias: &str, cx: &mut Context<Self>) {
        let mut next = preferences(cx).clone();
        let remaining = next
            .commands
            .aliases(command)
            .iter()
            .filter(|name| *name != alias)
            .cloned()
            .collect::<Vec<_>>()
            .join(",");
        if next.commands.set_aliases(command, &remaining).is_ok() {
            save(next, cx);
        }
    }

    fn alias_chip(
        &self,
        command: Command,
        index: usize,
        alias: &str,
        conflict: Option<String>,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let palette = crate::theme::settings_palette(cx);
        let removed = alias.to_owned();
        let id = command as usize * 8 + index;
        h_flex()
            .h(px(INPUT_HEIGHT))
            .pl_2()
            .pr_1()
            .gap_1p5()
            .rounded(px(6.0))
            .bg(palette.keycap)
            .when_some(conflict, |chip, message| {
                chip.child(components::conflict_glyph(("alias-conflict", id), message))
            })
            .child(alias.to_owned())
            .child(
                div()
                    .id(("alias-remove", id))
                    .size(px(20.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.0))
                    .cursor_pointer()
                    .hover(move |button| button.bg(palette.hover))
                    .child(
                        svg()
                            .path(icons::WINDOW_CLOSE)
                            .size(px(16.0))
                            .text_color(palette.muted),
                    )
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.remove_alias(command, &removed, cx)),
                    ),
            )
    }

    pub(super) fn commands_page(&self, cx: &Context<Self>) -> gpui::AnyElement {
        let mut body = page(self.page).child(section(text::COMMANDS)).child(
            card(cx).child(setting_row(
                text::SHOW_COMMAND_PICKER,
                text::SHOW_COMMAND_PICKER_HELP,
                Switch::new("show-command-picker")
                    .checked(preferences(cx).show_command_picker)
                    .color(crate::theme::settings_palette(cx).accent)
                    .cursor_pointer()
                    .on_click(|checked, _, cx| {
                        let mut next = preferences(cx).clone();
                        next.show_command_picker = *checked;
                        save(next, cx);
                    }),
                cx,
            )),
        );
        let Some(editor) = &self.alias_editor else {
            return body.into_any_element();
        };
        let routes = &preferences(cx).commands;
        let palette = crate::theme::settings_palette(cx);
        let mut rows = card(cx);
        for (index, command) in Command::ALL.into_iter().enumerate() {
            if index > 0 {
                rows = rows.child(components::separator(cx));
            }
            let error = editor
                .error
                .as_ref()
                .filter(|(failed, _)| *failed == command)
                .map(|(_, message)| message.clone());
            let chips = routes
                .aliases(command)
                .iter()
                .enumerate()
                .map(|(index, alias)| {
                    let conflict = conflict_message(routes, command, alias);
                    self.alias_chip(command, index, alias, conflict, cx)
                })
                .collect::<Vec<_>>();
            rows = rows.child(
                div()
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(text::command_label(command)),
                            )
                            .child(components::conflict_slot(
                                ("trigger-conflict", command as usize),
                                conflict_message(routes, command, routes.trigger(command)),
                            ))
                            .child(components::key_badge("space", cx))
                            .child(
                                Input::new(&editor.triggers[command as usize])
                                    .small()
                                    .h(px(INPUT_HEIGHT))
                                    .w(px(64.0))
                                    .text_size(px(13.0))
                                    .line_height(px(20.0)),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_4()
                            .items_start()
                            .child(
                                div()
                                    .w(px(50.0))
                                    .h(px(INPUT_HEIGHT))
                                    .flex()
                                    .items_center()
                                    .text_color(palette.muted)
                                    .child(text::ALIASES),
                            )
                            .child(
                                h_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .flex_wrap()
                                    .gap_2()
                                    .children(chips)
                                    // At the limit there is nothing left to add, so
                                    // the field goes away instead of erroring.
                                    .when(
                                        routes.aliases(command).len()
                                            < crate::command_router::MAX_ALIASES,
                                        |row| {
                                            row.child(
                                                Input::new(&editor.additions[command as usize])
                                                    .small()
                                                    .h(px(INPUT_HEIGHT))
                                                    .w(px(120.0))
                                                    .text_size(px(13.0))
                                                    .line_height(px(20.0)),
                                            )
                                        },
                                    ),
                            ),
                    )
                    .when_some(error, |column, message| {
                        column.child(
                            div()
                                .text_size(px(12.0))
                                .text_color(palette.accent)
                                .child(message),
                        )
                    }),
            );
        }
        body = body.child(rows);
        body.into_any_element()
    }
}
