use super::*;

pub(in crate::launcher) fn updates_page(
    delegate: &ResultListDelegate,
    cx: &Context<LauncherRoot>,
) -> AnyElement {
    let scry_page = delegate.show_scry_updates || delegate.needs_pinned_scry();
    let scry_required = delegate.file_search
        && delegate
            .results
            .iter()
            .any(|item| item.id == "files:compatibility:update");
    let scry_setup = delegate.file_search
        && delegate
            .results
            .iter()
            .any(|item| item.id == "files:compatibility:unavailable");
    let update_loading = if scry_page {
        delegate.scry_installing || (delegate.scry_checking && !delegate.needs_pinned_scry())
    } else {
        delegate.checking_update || delegate.restarting_update
    };
    let update_ready = if scry_page {
        delegate.scry_target().is_some()
    } else {
        delegate.update_result == Some(crate::updates::CheckResult::Ready)
    };
    let restarting_update = delegate.restarting_update;
    let update_failed = delegate.update_failed;
    let required_message = delegate
        .needs_pinned_scry()
        .then(|| text::updates::scry_ready_to_install(crate::scry_updates::REQUIRED_VERSION));
    let update_message = if scry_page {
        if delegate.needs_pinned_scry() && !delegate.scry_install_attempted {
            required_message.as_deref().unwrap_or_default()
        } else {
            delegate.scry_message.as_str()
        }
    } else {
        delegate
            .update_message
            .unwrap_or(text::updates::HAYAI_PROMPT)
    };
    let update_icon = if scry_page {
        delegate.scry_icon.clone()
    } else {
        delegate.update_icon.clone()
    };
    let scry_installing = delegate.scry_installing;
    let scry_ready = delegate.scry_ready;
    let scry_status_label = if delegate.scry_checking {
        text::updates::SCRY_CHECKING
    } else {
        match delegate.scry_discovery.as_ref().map(|result| &result.state) {
            Some(crate::scry_updates::State::Available) => text::updates::SCRY_AVAILABLE,
            Some(crate::scry_updates::State::Current) => text::updates::SCRY_CURRENT,
            Some(crate::scry_updates::State::NoCompatibleRelease) => text::updates::SCRY_NO_RELEASE,
            _ => text::updates::SCRY_CHECK_FAILED,
        }
    };

    div()
        .flex_1()
        .w_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_3()
        .text_center()
        .child(
            div()
                .size(px(56.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .when(update_loading, |slot| {
                    slot.child(
                        gpui_component::spinner::Spinner::new()
                            .icon(gpui_component::Icon::default().path(icon_assets::SPINNER))
                            .with_size(px(32.0))
                            .color(cx.theme().muted_foreground),
                    )
                })
                .when(!update_loading, |slot| {
                    slot.when_some(update_icon, |slot, icon| {
                        slot.child(img(icon).size(px(56.0)))
                    })
                }),
        )
        .child(
            div()
                .text_size(px(14.0))
                .font_weight(FontWeight::MEDIUM)
                .child(if scry_page {
                    if scry_installing {
                        text::updates::SCRY_UPDATING
                    } else if scry_ready {
                        text::updates::SCRY_READY
                    } else if scry_required {
                        text::updates::SCRY_REQUIRED
                    } else if scry_setup {
                        text::updates::SCRY_SETUP
                    } else if update_loading {
                        text::updates::CHECKING
                    } else if update_ready {
                        text::updates::SCRY_AVAILABLE
                    } else {
                        text::updates::SCRY_TITLE
                    }
                } else if restarting_update {
                    text::updates::RESTARTING
                } else if update_loading {
                    text::updates::CHECKING
                } else if update_failed {
                    text::updates::RESTART_FAILED
                } else if update_ready {
                    text::updates::READY
                } else {
                    text::updates::HAYAI_TITLE
                }),
        )
        .child(
            div()
                .max_w(px(440.0))
                .text_size(px(13.0))
                .text_color(cx.theme().muted_foreground)
                .child(if scry_page && scry_required && !scry_ready {
                    text::updates::scry_requirement(update_message)
                } else if scry_page {
                    update_message.to_string()
                } else if restarting_update {
                    text::updates::APPLYING.to_string()
                } else if update_loading {
                    text::updates::DOWNLOADING.to_string()
                } else if update_failed {
                    text::updates::RETRY_RESTART.to_string()
                } else if update_ready {
                    text::updates::hayai_ready_to_install(
                        &crate::updates::ready_version().unwrap_or_default(),
                    )
                } else {
                    update_message.to_string()
                }),
        )
        .when(!update_loading, |page| {
            page.child(
                div()
                    .id("update-page-action")
                    .mt_1()
                    .px_4()
                    .py_2()
                    .rounded(px(6.0))
                    .bg(cx.theme().list_active)
                    .cursor_pointer()
                    .hover(|button| button.bg(cx.theme().tokens.list_hover))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            cx.stop_propagation();
                            this.list.update(cx, |list, cx| {
                                let delegate = list.delegate_mut();
                                if delegate.needs_pinned_scry() {
                                    delegate.show_scry_updates = true;
                                }
                                delegate.confirm(false, window, cx);
                                cx.notify();
                            });
                        }),
                    )
                    .child(if scry_page {
                        if scry_ready {
                            text::actions::BACK_TO_SEARCH
                        } else if update_ready {
                            text::actions::UPDATE_SCRY
                        } else {
                            text::actions::CHECK_AGAIN
                        }
                    } else if update_ready {
                        text::actions::RESTART_TO_UPDATE
                    } else {
                        text::actions::CHECK_AGAIN
                    }),
            )
        })
        .when(!scry_page, |page| {
            page.child(
                div()
                    .id("scry-update-status")
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .cursor_pointer()
                    .underline()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            this.list.update(cx, |list, cx| {
                                let delegate = list.delegate_mut();
                                if delegate.scry_discovery.as_ref().is_some_and(|result| {
                                    result.state == crate::scry_updates::State::Available
                                }) && !delegate.scry_checking
                                {
                                    delegate.show_updates = false;
                                    delegate.show_scry_updates = true;
                                } else {
                                    delegate.check_scry_updates(true, window, cx);
                                }
                                cx.notify();
                            });
                        }),
                    )
                    .child(scry_status_label),
            )
        })
        .into_any_element()
}

pub(in crate::launcher) fn about_page(
    brand_icon: Option<Arc<RenderImage>>,
    cx: &Context<LauncherRoot>,
) -> AnyElement {
    div()
        .flex_1()
        .w_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_3()
        .when_some(brand_icon, |card, icon| {
            card.child(img(icon).size(px(56.0)))
        })
        .child(
            div()
                .text_xl()
                .font_weight(FontWeight::SEMIBOLD)
                .child(text::headings::HAYAI),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(text::about::version(env!("CARGO_PKG_VERSION"))),
        )
        .child(text::about::DESCRIPTION)
        .child(
            div()
                .id("hayai-project-link")
                .text_xs()
                .underline()
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_, _, window, cx| {
                        LauncherState::dismiss(window, cx);
                        cx.open_url("https://github.com/Ariaryy/hayai");
                    }),
                )
                .child(text::about::PROJECT_LINK),
        )
        .into_any_element()
}

pub(in crate::launcher) fn file_setup_page(
    title: String,
    description: String,
    icon: Option<Arc<RenderImage>>,
    starting_scry: bool,
    updating_scry: bool,
    cx: &Context<LauncherRoot>,
) -> AnyElement {
    div()
        .flex_1()
        .w_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_3()
        .text_center()
        .child(
            div()
                .size(px(SCRY_SETUP_ICON_SIZE))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .when(starting_scry, |slot| {
                    slot.child(
                        gpui_component::spinner::Spinner::new()
                            .icon(gpui_component::Icon::default().path(icon_assets::SPINNER))
                            .with_size(px(32.0))
                            .color(cx.theme().muted_foreground),
                    )
                })
                .when(!starting_scry, |slot| {
                    slot.when_some(icon, |slot, icon| {
                        slot.child(img(icon).size(px(SCRY_SETUP_ICON_SIZE)))
                    })
                }),
        )
        .child(
            div()
                .text_size(px(14.0))
                .font_weight(FontWeight::MEDIUM)
                .child(if updating_scry {
                    text::updates::SCRY_UPDATING_SHORT.to_string()
                } else if starting_scry {
                    text::updates::SCRY_STARTING.to_string()
                } else {
                    title
                }),
        )
        .child(
            div()
                .max_w(px(380.0))
                .text_size(px(13.0))
                .text_color(cx.theme().muted_foreground)
                .child(if starting_scry {
                    text::updates::SCRY_WAITING.to_string()
                } else {
                    description
                }),
        )
        .child(
            div()
                .id("file-search-learn-more")
                .text_xs()
                .underline()
                .cursor_pointer()
                .text_color(cx.theme().muted_foreground)
                .hover(|link| link.text_color(cx.theme().foreground))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_, _, window, cx| {
                        cx.stop_propagation();
                        LauncherState::dismiss(window, cx);
                        cx.open_url(
                            "https://github.com/Ariaryy/hayai/blob/main/docs/users/file-search.md",
                        );
                    }),
                )
                .child(text::search::LEARN_MORE),
        )
        .into_any_element()
}
