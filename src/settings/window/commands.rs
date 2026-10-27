use super::*;
use crate::command_router::Command;
use gpui::Entity;
use gpui_component::{
    Sizable,
    input::{Input, InputEvent, InputState},
};

/// Chips, the trigger and the add field share one height so rows stay level.
const INPUT_HEIGHT: f32 = 24.0;
// Column widths shared by the header and every row so the table lines up.
const GUTTER_WIDTH: f32 = 16.0;
const COMMAND_WIDTH: f32 = 148.0;
const TRIGGER_WIDTH: f32 = 124.0;
const COLUMN_GAP: f32 = 12.0;

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
        // Children are placed by whole-device-pixel offsets, not flex centering.
        let line = crate::pixels::snap(cx, 20.0);
        let text_top = crate::pixels::text_top(
            cx,
            INPUT_HEIGHT,
            0.0,
            20.0,
            13.0,
            crate::pixels::Glyphs::Lowercase,
        );
        let (close, close_top) = crate::pixels::fit(cx, INPUT_HEIGHT, 0.0, 16.0);
        h_flex()
            .h(crate::pixels::snap(cx, INPUT_HEIGHT))
            .items_start()
            .pl_2()
            // The close glyph has padding inside its button, so a 4px right inset
            // makes the visible gaps either side of the chip equal.
            .pr_1()
            .gap_1()
            .rounded(px(RADIUS_MD))
            .bg(palette.keycap)
            .when_some(conflict, |chip, message| {
                chip.child(div().mt(crate::pixels::lead(cx, INPUT_HEIGHT, 16.0)).child(
                    components::conflict_glyph(("alias-conflict", id), message, cx),
                ))
            })
            .child(
                div()
                    .mt(text_top)
                    .h(line)
                    .line_height(line)
                    .child(alias.to_owned()),
            )
            .child(
                div()
                    .id(("alias-remove", id))
                    .mt(close_top)
                    .size(close)
                    .rounded(px(RADIUS_SM))
                    .cursor_pointer()
                    .hover(move |button| button.bg(palette.hover))
                    .child(
                        svg()
                            .path(icons::WINDOW_CLOSE)
                            .size(close)
                            .text_color(palette.muted),
                    )
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.remove_alias(command, &removed, cx)),
                    ),
            )
    }

    pub(super) fn commands_page(&self, cx: &Context<Self>) -> gpui::AnyElement {
        let mut body = group(text::COMMANDS).child(
            card(cx).child(setting_row(
                text::SHOW_COMMAND_PICKER,
                text::SHOW_COMMAND_PICKER_HELP,
                components::ToggleSwitch::new(
                    "show-command-picker",
                    preferences(cx).show_command_picker,
                )
                .on_change(|checked, _, cx| {
                    let mut next = preferences(cx).clone();
                    next.show_command_picker = checked;
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
        let header_cell = |label: &'static str| {
            div()
                .text_size(px(CAPTION_SIZE))
                .line_height(px(CAPTION_LINE))
                .text_color(palette.muted)
                .child(label)
        };
        let mut rows = card(cx).child(
            h_flex()
                .px_4()
                .py_2()
                .gap(px(COLUMN_GAP))
                .border_b_1()
                .border_color(palette.border)
                .child(div().w(px(GUTTER_WIDTH)).flex_none())
                .child(header_cell(text::COMMAND).w(px(COMMAND_WIDTH)).flex_none())
                .child(header_cell(text::TRIGGER).w(px(TRIGGER_WIDTH)).flex_none())
                .child(header_cell(text::ALIASES).flex_1()),
        );
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
            // Every cell is one input tall, so the first line of a wrapped alias
            // list stays level with the name and trigger beside it.
            let cell = || h_flex().h(px(INPUT_HEIGHT)).flex_none();
            rows = rows.child(
                div()
                    .px_4()
                    .py_3()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        h_flex()
                            .items_start()
                            .gap(px(COLUMN_GAP))
                            .child(cell().w(px(GUTTER_WIDTH)).justify_center().child(
                                components::conflict_slot(
                                    ("trigger-conflict", command as usize),
                                    conflict_message(routes, command, routes.trigger(command)),
                                    cx,
                                ),
                            ))
                            .child(
                                cell()
                                    .w(px(COMMAND_WIDTH))
                                    .gap_2()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(
                                        svg()
                                            .path(command.icon())
                                            .size(px(16.0))
                                            .flex_none()
                                            .text_color(palette.muted),
                                    )
                                    .child(text::command_label(command)),
                            )
                            .child(
                                cell()
                                    .w(px(TRIGGER_WIDTH))
                                    .gap_1()
                                    .child(components::key_badge("space", cx))
                                    .child(
                                        Input::new(&editor.triggers[command as usize])
                                            .small()
                                            .h(px(INPUT_HEIGHT))
                                            .w(px(56.0))
                                            .text_size(px(BODY_SIZE))
                                            .line_height(px(BODY_LINE)),
                                    ),
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
                                                    .w(px(112.0))
                                                    .text_size(px(BODY_SIZE))
                                                    .line_height(px(BODY_LINE)),
                                            )
                                        },
                                    ),
                            ),
                    )
                    .when_some(error, |column, message| {
                        column.child(
                            div()
                                .pl(px(GUTTER_WIDTH + COLUMN_GAP))
                                .text_size(px(CAPTION_SIZE))
                                .line_height(px(CAPTION_LINE))
                                .text_color(palette.danger)
                                .child(message),
                        )
                    }),
            );
        }
        body = body.child(rows);
        body.into_any_element()
    }
}
