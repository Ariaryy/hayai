use super::*;
use crate::command_router::Command;
use gpui::Entity;
use gpui_component::{
    Sizable,
    input::{Input, InputEvent, InputState},
};

pub(super) struct AliasEditor {
    pub(super) inputs: [Entity<InputState>; Command::ALL.len()],
    _subscriptions: Vec<Subscription>,
}

impl AliasEditor {
    pub(super) fn new(window: &mut Window, cx: &mut Context<SettingsView>) -> Self {
        let inputs = Command::ALL.map(|command| {
            let value = preferences(cx).commands.names(command).join(", ");
            cx.new(|cx| InputState::new(window, cx).default_value(value))
        });
        let subscriptions = Command::ALL
            .into_iter()
            .map(|command| {
                cx.subscribe_in(
                    &inputs[command as usize],
                    window,
                    move |this, input, event, _, cx| {
                        if !matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                            return;
                        }
                        let value = input.read(cx).value().to_string();
                        let mut next = preferences(cx).clone();
                        match next.commands.set(command, &value) {
                            Ok(()) => {
                                this.validation = None;
                                if next != *preferences(cx) {
                                    save(next, cx);
                                }
                            }
                            Err(error) => this.validation = Some(error.to_string()),
                        }
                        cx.notify();
                    },
                )
            })
            .collect();
        Self {
            inputs,
            _subscriptions: subscriptions,
        }
    }
}

impl SettingsView {
    pub(super) fn commit_aliases(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(editor) = &self.alias_editor else {
            return true;
        };
        let mut next = preferences(cx).clone();
        for command in Command::ALL {
            let value = editor.inputs[command as usize].read(cx).value();
            if let Err(error) = next.commands.set(command, &value) {
                self.validation = Some(error.to_string());
                cx.notify();
                return false;
            }
        }
        self.validation = None;
        if next != *preferences(cx) {
            save(next, cx);
        }
        true
    }

    pub(super) fn commands_page(&self, cx: &Context<Self>) -> gpui::AnyElement {
        let mut body = page(self.page).child(section(text::COMMANDS));
        let Some(editor) = &self.alias_editor else {
            return body.into_any_element();
        };
        let aliases = &preferences(cx).commands;
        let palette = crate::theme::settings_palette(cx);
        let mut rows = card(cx);
        for (index, command) in Command::ALL.into_iter().enumerate() {
            if index > 0 {
                rows = rows.child(components::separator(cx));
            }
            let triggers = aliases.triggers(command);
            rows = rows.child(
                div()
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        h_flex()
                            .gap_3()
                            .items_start()
                            .child(
                                div()
                                    .flex_1()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(text::command_label(command)),
                            )
                            .child(
                                h_flex()
                                    .max_w(gpui::relative(0.65))
                                    .flex_wrap()
                                    .justify_end()
                                    .gap_2()
                                    .children(
                                        triggers
                                            .into_iter()
                                            .map(|trigger| command_badges(trigger, cx)),
                                    ),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_4()
                            .child(
                                div()
                                    .w(px(50.0))
                                    .text_color(palette.muted)
                                    .child(text::ALIASES),
                            )
                            .child(
                                Input::new(&editor.inputs[command as usize])
                                    .small()
                                    .flex_1()
                                    .min_w_0()
                                    .text_size(px(13.0))
                                    .line_height(px(20.0)),
                            ),
                    ),
            );
        }
        body = body.child(rows);
        body.into_any_element()
    }
}
