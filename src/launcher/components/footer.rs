use super::*;

/// Fixed-height keycaps keep shortcut labels centered independently of the
/// input widget's text sizing. Each physical key gets its own cap.
pub(super) fn shortcut_keycap(content: impl IntoElement, cx: &App) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_center()
        .text_color(cx.theme().foreground)
        .bg(cx.theme().list_active)
        .h(px(20.0))
        .min_w(px(20.0))
        .px(px(5.0))
        .rounded(px(4.0))
        .line_height(relative(1.0))
        .text_size(px(11.0))
        .font_weight(FontWeight::MEDIUM)
        .flex_shrink_0()
        .child(content)
}

pub(in crate::launcher) fn footer(
    root: &LauncherRoot,
    setup_screen: bool,
    updates_page: bool,
    actions_disabled: bool,
    empty_notice: bool,
    cx: &App,
) -> impl IntoElement {
    let primary_action = root.selected_primary_action(cx);
    let primary_label = primary_action.label();
    let can_enter_folder = root.file_mode && primary_action == PrimaryAction::OpenFolder;
    // Backspace edits non-empty queries; advertise navigation only when
    // the empty-input handler can actually leave the current scope/mode.
    let can_go_back = (root.file_mode || root.waiting_for_command)
        && root.search_input.read(cx).value().is_empty();
    let show_commands_hint = !setup_screen
        && !root.file_mode
        && !root.waiting_for_command
        && root.search_input.read(cx).value().is_empty();
    let backspace_label = if !root.folder_scope.is_empty() {
        text::search::UP_ONE_LEVEL
    } else if root.file_mode {
        text::search::EXIT_FILE_SEARCH
    } else {
        text::search::CANCEL_COMMAND
    };
    h_flex()
        .flex_none()
        .h(px(36.0))
        .w_full()
        .px_3()
        .items_center()
        .justify_end()
        .gap_2()
        .border_t_1()
        .border_color(cx.theme().border)
        .text_color(cx.theme().muted_foreground)
        .text_xs()
        .child(
            h_flex()
                .flex_1()
                .min_w_0()
                .gap_3()
                .when(root.action_menu.is_none() && show_commands_hint, |hints| {
                    hints.child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(text::search::COMMANDS)
                            .child(shortcut_keycap(text::keys::SPACE, cx)),
                    )
                })
                .when(root.action_menu.is_none() && can_enter_folder, |hints| {
                    hints.child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(text::search::ENTER_FOLDER)
                            .child(shortcut_keycap(text::keys::TAB, cx)),
                    )
                })
                .when(root.action_menu.is_none() && can_go_back, |hints| {
                    hints.child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(backspace_label)
                            .child(shortcut_keycap(text::keys::BACKSPACE, cx)),
                    )
                }),
        )
        .when(!empty_notice, |footer| {
            footer.child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .text_color(cx.theme().foreground)
                    .font_weight(FontWeight::MEDIUM)
                    .child(primary_label)
                    .child(shortcut_keycap(
                        gpui::svg()
                            .path(icon_assets::ENTER)
                            // Svg::paint requires its own style color;
                            // inheriting the keycap's text color does
                            // not populate that field on this GPUI rev.
                            .text_color(cx.theme().foreground)
                            .size(px(14.0))
                            .flex_none(),
                        cx,
                    )),
            )
        })
        .when((!setup_screen || updates_page) && !empty_notice, |footer| {
            footer
                .child(div().mx_2().w(px(1.0)).h(px(16.0)).bg(cx.theme().border))
                .child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .h(px(26.0))
                        .px(px(2.5))
                        // The 20px caps retain 3px of vertical breathing room.
                        .rounded(px(7.0))
                        .when(actions_disabled, |group| group.opacity(0.4))
                        .when(root.action_menu.is_some(), |group| {
                            group
                                .bg(cx.theme().list_hover)
                                .text_color(cx.theme().foreground)
                        })
                        .child(div().pl(px(3.0)).child(text::search::ACTIONS))
                        .child(crate::settings::key_badges(
                            crate::settings::preferences(cx)
                                .binding(crate::settings::Shortcut::Actions),
                            cx,
                        )),
                )
        })
}
