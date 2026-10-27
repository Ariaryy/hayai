use super::footer::shortcut_keycap;
use super::*;

pub(in crate::launcher) fn action_panel(
    menu: ActionMenu,
    cx: &Context<LauncherRoot>,
) -> impl IntoElement {
    let mut panel = div()
        .absolute()
        // Painting above the list does not block its hitboxes.
        // Occlude the entire panel, including padding, so pointer
        // movement and scrolling cannot reach rows underneath.
        .occlude()
        .right(px(8.0))
        .bottom(px(44.0))
        .w(px(224.0))
        .p_1()
        .bg(cx.theme().popover)
        .border_1()
        .border_color(cx.theme().border)
        .rounded(px(8.0))
        .shadow_lg()
        .text_size(px(13.0));
    let shortcuts = ["1", "2", "3", "4", "5", "6", "7"];
    for (index, shortcut) in shortcuts.into_iter().enumerate().take(menu.count()) {
        let Some((_, label, icon)) = menu.row(index) else {
            continue;
        };
        panel = panel.child(
            h_flex()
                .id(("context-action", index))
                .h(px(32.0))
                .px_2()
                .gap_2()
                .items_center()
                .justify_between()
                .rounded(px(5.0))
                .cursor_pointer()
                .when(menu.selected == index, |row| {
                    row.bg(cx.theme().tokens.list_hover)
                })
                .hover(|row| row.bg(cx.theme().tokens.list_hover))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        // Consume the click so it cannot reach the
                        // result list underneath the floating panel.
                        cx.stop_propagation();
                        if let Some(menu) = this.action_menu.as_mut() {
                            menu.selected = index;
                        }
                        if let Some(action) = this.selected_menu_action() {
                            this.run_menu_action(action, window, cx);
                        }
                    }),
                )
                .when(
                    menu.submenu
                        .as_ref()
                        .is_some_and(|submenu| !submenu.loading),
                    |row| {
                        row.child(
                            div().size(px(18.0)).flex_none().child(match icon {
                                Some(icon) => img(icon).size(px(18.0)).into_any_element(),
                                None => gpui::svg()
                                    .path(icon_assets::APPLICATION)
                                    .size(px(18.0))
                                    .text_color(cx.theme().muted_foreground)
                                    .into_any_element(),
                            }),
                        )
                    },
                )
                .child(div().flex_1().min_w_0().truncate().child(label))
                .child(shortcut_keycap(shortcut, cx)),
        );
    }
    panel
}
