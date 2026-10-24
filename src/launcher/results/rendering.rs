use super::*;

impl ResultListDelegate {
    pub(super) fn section_header(
        &mut self,
        section: usize,
        _window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Option<impl IntoElement> {
        let label = if self.notification_visible() && section == 0 {
            text::headings::NOTIFICATIONS
        } else if section == self.section_offset() + 1 {
            text::headings::HAYAI
        } else if self.apps_view {
            text::headings::APPLICATIONS
        } else if self.file_search {
            text::headings::FILES
        } else {
            text::headings::CALCULATOR
        };
        // The list shares this measurement across headers. Keep it compact
        // so the first section does not add a blank band below the search box.
        Some(
            div()
                .h(px(if self.apps_view { 24.0 } else { 20.0 }))
                .flex_none()
                .flex()
                .items_center()
                .px_1()
                .child(section_heading(label, cx)),
        )
    }

    pub(super) fn empty_view(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_3()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .when(!self.search_pending, |view| {
                view.child(
                    gpui::svg()
                        .path(
                            if self.empty_state.as_ref().is_some_and(|notice| {
                                matches!(notice.action, CommandAction::PendingState)
                            }) {
                                icon_assets::CALCULATOR
                            } else if self.empty_state.is_some() {
                                icon_assets::CALCULATOR_INVALID
                            } else {
                                icon_assets::SEARCH_EMPTY
                            },
                        )
                        .size(px(40.0))
                        .flex_none()
                        .text_color(cx.theme().muted_foreground.opacity(0.65)),
                )
                .child(if let Some(notice) = self.empty_state.as_ref() {
                    notice.title.clone()
                } else if self.file_search {
                    text::search::NO_FILES.to_string()
                } else {
                    text::search::NO_APPS.to_string()
                })
            })
    }

    pub(super) fn result_row(
        &mut self,
        ix: IndexPath,
        _window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Option<ResultRow> {
        self.refresh_builtin_row_icons(_window);
        if self.notification_visible() && ix.section == 0 {
            let scry = self.notification_kind(ix.row)? == UpdateNotification::Scry;
            let title = if scry {
                if self.scry_installing {
                    text::updates::SCRY_UPDATING_SHORT
                } else if self.scry_install_attempted && self.scry_ready {
                    text::updates::SCRY_UPDATED
                } else if self.scry_install_attempted {
                    text::updates::SCRY_UPDATE_FAILED
                } else if self.scry_checking {
                    text::updates::SCRY_CHECKING_UPDATES
                } else {
                    match self.scry_discovery.as_ref().map(|result| &result.state) {
                        Some(crate::scry_updates::State::Available) => {
                            text::updates::SCRY_AVAILABLE
                        }
                        Some(crate::scry_updates::State::Current) => text::updates::SCRY_CURRENT,
                        Some(crate::scry_updates::State::NoCompatibleRelease) => {
                            text::updates::SCRY_NO_UPDATE
                        }
                        _ => text::updates::SCRY_CHECK_FAILED,
                    }
                }
            } else if self.checking_update {
                text::updates::HAYAI_CHECKING
            } else if self.restarting_update {
                text::updates::RESTARTING
            } else if crate::updates::ready_version().is_some() {
                text::updates::HAYAI_AVAILABLE
            } else if self.update_result == Some(crate::updates::CheckResult::Current) {
                text::updates::HAYAI_CURRENT
            } else {
                text::updates::HAYAI_CHECK_FAILED
            };
            let subtitle = if scry {
                if self.scry_installing {
                    text::updates::INSTALLING.into()
                } else if self.scry_install_attempted && self.scry_ready {
                    text::updates::DISMISS_HINT.into()
                } else if self.scry_install_attempted && self.scry_message.chars().count() > 55 {
                    text::updates::RETRY_DETAILS.into()
                } else if self.scry_checking && !self.scry_install_attempted {
                    text::updates::SEARCH_HINT.into()
                } else {
                    self.scry_message.clone()
                }
            } else if self.checking_update {
                text::updates::SEARCH_HINT.into()
            } else if self.update_failed {
                text::updates::START_UPDATE_FAILED.into()
            } else {
                crate::updates::ready_version()
                    .map(|version| text::updates::hayai_version(&version))
                    .unwrap_or_else(|| {
                        self.update_message
                            .unwrap_or(text::updates::TRY_LATER)
                            .into()
                    })
            };
            return Some(ResultRow {
                compact: self.apps_view,
                icon_size: HAYAI_ROW_ICON_SIZE,
                loading: scry && self.scry_installing,
                ..ResultRow::new(
                    ix,
                    title.into(),
                    Some(subtitle),
                    None,
                    if scry {
                        self.builtin_row_icons[2].clone()
                    } else {
                        self.builtin_row_icons[1].clone()
                    },
                    self.notification_selected && self.selected_index == Some(ix),
                    false,
                )
            });
        }
        let mut item = self.item(ix)?.clone();
        if matches!(item.action, CommandAction::CheckForUpdates) {
            item.subtitle = Some(
                self.update_message
                    .unwrap_or(text::updates::MANUAL_CHECK)
                    .into(),
            );
        }
        let icon = if matches!(item.action, CommandAction::CheckForUpdates) {
            self.builtin_row_icons[1].clone()
        } else if is_hayai_action(&item.action) {
            self.builtin_row_icons[0].clone()
        } else {
            match &item.icon {
                IconSource::Path(path) => {
                    let mut catalog = self.catalog.borrow_mut();
                    match catalog.request_icon(
                        path,
                        row_icon_pixels(_window.scale_factor()),
                        |image| {
                            let _ = _window.drop_image(image);
                        },
                    ) {
                        IconRequest::Ready(icon) => icon,
                        IconRequest::Loading => None,
                        IconRequest::Load(path) => {
                            drop(catalog);
                            spawn_icon_load(
                                path,
                                row_icon_pixels(_window.scale_factor()),
                                self.catalog.clone(),
                                cx,
                            );
                            None
                        }
                    }
                }
                IconSource::None => None,
            }
        };
        self.prefetch_icons_near(self.flat_index(ix), _window, cx);
        let selected = self.selected_index == Some(ix);
        // Only `CalcProvider` produces `CopyToClipboard` today, so this
        // doubles as "is this a calculator result" without adding a field
        // to `CommandItem` that every other provider would have to fill in.
        let calculator = matches!(item.action, CommandAction::CopyToClipboard(_));
        Some(ResultRow {
            compact: self.apps_view,
            icon_size: if is_hayai_action(&item.action) {
                HAYAI_ROW_ICON_SIZE
            } else {
                22.0
            },
            file_size: item.file_size,
            ..ResultRow::new(
                ix,
                item.title,
                item.subtitle,
                item.calculation_detail,
                icon,
                selected,
                calculator,
            )
        })
    }
}
