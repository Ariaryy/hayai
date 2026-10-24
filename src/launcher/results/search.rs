use super::*;

impl ResultListDelegate {
    pub(super) fn search(
        &mut self,
        query: &str,
        window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Task<()> {
        let was_file_search = self.file_search;
        self.search_pending = false;
        self.starting_scry = false;
        self.last_query = query.to_string();
        self.show_about = false;
        self.show_updates = false;
        self.file_search = query == " f" || query.starts_with(" f ");
        self.show_scry_updates = self.file_search
            && was_file_search
            && (self.show_scry_updates || self.needs_pinned_scry());
        // File mode only probes local compatibility. Required setup already
        // has an exact release target; optional discovery is user initiated.
        let dispatch = self.registry.borrow_mut().dispatch(query);
        self.apps_view = self.registry.borrow().namespace(&dispatch) == "apps";
        if !self.apps_view {
            self.notification_selected = false;
        }
        let background_job = if dispatch.debounce {
            self.registry.borrow().background_search(&dispatch)
        } else {
            None
        };
        if let Some(job) = background_job {
            self.search_pending = true;
            if self.file_search
                && (!was_file_search || dispatch.query.is_empty())
                && !self.show_scry_updates
            {
                // Render in-memory recents in the same frame as the mode
                // switch. Disk validation and compatibility remain background
                // work; confirmation stays disabled until that work finishes.
                let preview = self.registry.borrow().search(&dispatch);
                self.apply_results(dispatch.generation, preview, cx);
                self.search_pending = true;
                self.notification_selected = false;
            }
            cx.notify();
            let generation = dispatch.generation;
            let registry = self.registry.clone();
            let debounce = !dispatch.query.is_empty();
            cx.spawn_in(window, async move |list, cx| {
                if debounce {
                    cx.background_executor()
                        .timer(Duration::from_millis(8))
                        .await;
                }
                // Foreground again after the await: the query may have moved
                // on while we slept, so drop this result set if so.
                if registry.borrow().current_generation() != generation {
                    return;
                }
                let items = cx.background_spawn(async move { job() }).await;
                if registry.borrow().current_generation() != generation {
                    return;
                }
                let _ = cx.update(|window, cx| {
                    list.update(cx, |list, cx| {
                        list.delegate_mut().apply_results(generation, items, cx);
                        // An empty query can switch the first populated section.
                        // Remeasure after the async results arrive; measuring an
                        // empty section collapses every virtual row to zero height.
                        refresh_result_layout(list, window, cx);
                    })
                });
            })
            .detach();
            return Task::ready(());
        }
        // Synchronous & in-memory providers (including debounced ones with
        // nothing to debounce, e.g. an empty query) run inline.
        let items = self.registry.borrow().search(&dispatch);
        self.apply_results(dispatch.generation, items, cx);
        Task::ready(())
    }
}
