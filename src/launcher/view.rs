use super::components::*;
use super::*;

impl gpui::Render for LauncherRoot {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.wheel_animation.is_some() && self.advance_wheel_frame(cx) {
            // current_view exists here during layout. Calling this from wheel
            // events or on_next_frame callbacks panics on the pinned GPUI rev.
            _window.request_animation_frame();
        }
        let delegate = self.list.read(cx).delegate();
        let about = delegate.show_about;
        let empty_notice = delegate.empty_state.is_some();
        // Compatibility results and explicit update navigation share one page.
        // A query refresh must not briefly expose the older setup renderer.
        let scry_page = delegate.show_scry_updates || delegate.needs_pinned_scry();
        let showing_updates = delegate.show_updates || scry_page;
        let show_main_heading = about || showing_updates;
        let actions_disabled = (delegate.search_pending && !showing_updates)
            || about
            || (!showing_updates
                && !delegate.notification_selected
                && !delegate
                    .selected_index
                    .and_then(|ix| delegate.item(ix))
                    .is_some_and(|item| {
                        matches!(
                            item.action,
                            CommandAction::LaunchApplication(_)
                                | CommandAction::OpenFile(_)
                                | CommandAction::CheckForUpdates
                        )
                    }));
        let file_setup = {
            let delegate = self.list.read(cx).delegate();
            delegate
                .results
                .first()
                .filter(|item| {
                    delegate.results.len() == 1
                        && (item.id.starts_with("files:compatibility:")
                            || matches!(
                                item.action,
                                CommandAction::InstallFileSearch | CommandAction::StartFileSearch
                            ))
                })
                .map(|item| {
                    (
                        item.title.clone(),
                        item.subtitle.clone().unwrap_or_default(),
                        delegate.scry_icon.clone(),
                    )
                })
        };
        let setup_screen = file_setup.is_some() || showing_updates || about;
        let updating_scry =
            self.list.read(cx).delegate().scry_installing || crate::scry_updates::is_updating();
        let starting_scry = self.list.read(cx).delegate().starting_scry || updating_scry;
        div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .border_1()
            .border_color(cx.theme().border)
            // Keep the border curve small: a larger inset curve exposes the
            // Root background between our border and the native window edge.
            .rounded(px(4.0))
            .shadow_lg()
            .on_action(cx.listener(Self::on_move_up))
            .on_action(cx.listener(Self::on_move_down))
            .on_action(cx.listener(Self::on_escape))
            .child(search_bar(self, cx))
            .when(show_main_heading, |root| {
                root.child(div().flex_none().px_2().pt_2().child(section_heading(
                    if scry_page {
                        text::headings::FILES
                    } else {
                        text::headings::HAYAI
                    },
                    cx,
                )))
            })
            .child(if self.waiting_for_command {
                self.command_picker(cx)
            } else if showing_updates {
                updates_page(self.list.read(cx).delegate(), cx)
            } else if about {
                about_page(delegate.brand_icon.clone(), cx)
            } else if let Some((title, description, icon)) = file_setup {
                file_setup_page(title, description, icon, starting_scry, updating_scry, cx)
            } else {
                results_view(&self.list, cx)
            })
            .child(footer(
                self,
                setup_screen,
                showing_updates,
                actions_disabled,
                empty_notice || self.waiting_for_command,
                cx,
            ))
            // Keep the panel last so it paints above the list and footer.
            .when_some(self.action_menu.clone(), |root, menu| {
                root.child(action_panel(menu, cx))
            })
    }
}
