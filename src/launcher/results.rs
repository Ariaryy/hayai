use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum UpdateNotification {
    Hayai,
    Scry,
}

pub(super) fn notification_kind(row: usize, hayai: bool, scry: bool) -> Option<UpdateNotification> {
    match (row, hayai, scry) {
        (0, true, _) => Some(UpdateNotification::Hayai),
        (0, false, true) | (1, true, true) => Some(UpdateNotification::Scry),
        _ => None,
    }
}

pub(super) fn section_header_entry(previous_sections: impl Iterator<Item = usize>) -> usize {
    previous_sections
        .filter(|count| *count > 0)
        .map(|count| count + 2)
        .sum()
}

pub(super) fn result_index_path(flat: usize, main: usize, total: usize) -> IndexPath {
    // An empty list still measures section one, where future file results go.
    // Section two exists only when there are actual Hayai command results.
    let secondary = flat >= main && main < total;
    IndexPath {
        section: 1 + usize::from(secondary),
        row: if secondary { flat - main } else { flat },
        ..Default::default()
    }
}

pub(super) fn scry_check_notice(
    state: Option<&crate::scry_updates::State>,
    checking: bool,
    manual: bool,
) -> bool {
    use crate::scry_updates::State;
    (checking && manual)
        || matches!(state, Some(State::Available))
        || (manual && matches!(state, Some(State::Current | State::NoCompatibleRelease)))
}

fn reconcile_notification_selection(
    list: &mut ListState<ResultListDelegate>,
    previous: Option<UpdateNotification>,
    window: &mut Window,
    cx: &mut Context<ListState<ResultListDelegate>>,
) {
    let Some(previous) = previous else {
        return;
    };
    let delegate = list.delegate();
    let ix = (0..delegate.notification_count())
        .find(|row| delegate.notification_kind(*row) == Some(previous))
        .map(|row| IndexPath {
            section: 0,
            row,
            ..Default::default()
        })
        .or_else(|| (!delegate.results.is_empty()).then(|| delegate.index_path(0)))
        .or_else(|| delegate.notification_visible().then(IndexPath::default));
    list.set_selected_index(ix, window, cx);
}

/// Owns the complete result set and its selection/search generation state.
/// Private modules implement dispatch, rendering, confirmation, and Scry updates.
pub(super) struct ResultListDelegate {
    pub(super) search_pending: bool,
    pub(super) update_notice: bool,
    pub(super) scry_notice: bool,
    pub(super) show_scry_updates: bool,
    pub(super) scry_checked: Option<std::time::Instant>,
    pub(super) scry_checking: bool,
    pub(super) scry_installing: bool,
    pub(super) scry_install_attempted: bool,
    pub(super) scry_install_target: Option<String>,
    pub(super) scry_ready: bool,
    pub(super) scry_update_dismissed: bool,
    pub(super) scry_discovery: Option<crate::scry_updates::Discovery>,
    pub(super) scry_message: String,
    pub(super) main_results: usize,
    pub(super) update_dismissed: bool,
    pub(super) checking_update: bool,
    pub(super) update_message: Option<&'static str>,
    pub(super) show_about: bool,
    pub(super) show_updates: bool,
    pub(super) update_result: Option<crate::updates::CheckResult>,
    pub(super) apps_view: bool,
    pub(super) brand_icon: Option<Arc<RenderImage>>,
    pub(super) update_icon: Option<Arc<RenderImage>>,
    pub(super) notification_selected: bool,
    pub(super) update_failed: bool,
    pub(super) restarting_update: bool,
    pub(super) starting_scry: bool,
    pub(super) last_query: String,
    pub(super) scry_icon: Option<Arc<RenderImage>>,
    pub(super) builtin_row_icons: [Option<Arc<RenderImage>>; 4],
    settings_icon: Option<Arc<RenderImage>>,
    pub(super) builtin_row_pixels: u32,
    pub(super) file_search: bool,
    pub(super) registry: Rc<RefCell<PluginRegistry>>,
    /// Catalog handle retained ONLY for the icon cache (`IconSource::Path`).
    pub(super) catalog: Rc<RefCell<Catalog>>,
    pub(super) results: Vec<CommandItem>,
    pub(super) empty_state: Option<CommandItem>,
    /// Generation the current `results` belong to (see `PluginRegistry`).
    pub(super) results_generation: u64,
    pub(super) selected_index: Option<IndexPath>,
}

impl ResultListDelegate {
    fn refresh_builtin_row_icons(&mut self, window: &mut Window) {
        let pixels = row_icon_pixels(window.scale_factor());
        if self.builtin_row_pixels == pixels {
            return;
        }
        // Keep the 56px detail-card images separate from the 22px row rasters.
        // Rebuild only on DPI changes and release the previous atlas entries.
        for icon in &mut self.builtin_row_icons {
            if let Some(icon) = icon.take() {
                let _ = window.drop_image(icon);
            }
        }
        self.builtin_row_icons = [
            builtin_row_icon(&self.brand_icon, pixels),
            builtin_row_icon(&self.update_icon, pixels),
            builtin_row_icon(&self.scry_icon, pixels),
            builtin_row_icon(&self.settings_icon, pixels),
        ];
        self.builtin_row_pixels = pixels;
    }

    fn prefetch_icons_near(
        &self,
        row: usize,
        window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) {
        // render_item is driven by the virtual list, so this follows mouse
        // scrolling as well as keyboard navigation. Protect two screens around
        // the rendered row and warm the nearest offscreen rows in both directions.
        // The cache admits at most eight speculative pending decodes; completions
        // repaint the list and refill the warm region without a timer or queue.
        let protected = icon_neighborhood(row, self.results.len(), 32);
        let protected = &self.results[protected];
        let pixels = row_icon_pixels(window.scale_factor());
        for distance in 1..=16 {
            for index in [row.checked_add(distance), row.checked_sub(distance)]
                .into_iter()
                .flatten()
            {
                let Some(CommandItem {
                    icon: IconSource::Path(path),
                    ..
                }) = self.results.get(index)
                else {
                    continue;
                };
                let request = self.catalog.borrow_mut().prefetch_icon(
                    path,
                    pixels,
                    |candidate| protected.iter().any(|item| matches!(&item.icon, IconSource::Path(path) if path == candidate)),
                    |image| { let _ = window.drop_image(image); },
                );
                if let IconRequest::Load(path) = request {
                    spawn_icon_load(path, pixels, self.catalog.clone(), cx);
                }
            }
        }
    }

    pub(super) fn needs_pinned_scry(&self) -> bool {
        self.file_search
            && self.results.iter().any(|item| {
                matches!(
                    item.id.as_str(),
                    "files:compatibility:update" | "files:compatibility:unavailable"
                )
            })
    }
    pub(super) fn scry_target(&self) -> Option<&str> {
        if self.needs_pinned_scry() {
            Some(crate::scry_updates::REQUIRED_VERSION)
        } else if self.scry_install_attempted && !self.scry_ready {
            self.scry_install_target.as_deref()
        } else {
            self.scry_discovery
                .as_ref()
                .filter(|result| result.state == crate::scry_updates::State::Available)
                .and_then(|result| result.target.as_deref())
        }
    }
    pub(super) fn calculator_view(&self) -> bool {
        !self.apps_view && !self.file_search
    }
    pub(super) fn notification_visible(&self) -> bool {
        self.notification_count() != 0
    }
    fn hayai_notification_visible(&self) -> bool {
        self.apps_view
            && !self.show_about
            && !self.update_dismissed
            && (self.checking_update
                || crate::updates::available_version().is_some()
                || (self.update_notice
                    && self.update_result == Some(crate::updates::CheckResult::Current)))
    }
    fn scry_notification_visible(&self) -> bool {
        self.apps_view
            && !self.show_about
            && !self.scry_update_dismissed
            && (self.scry_installing
                || self.scry_install_attempted
                || scry_check_notice(
                    self.scry_discovery.as_ref().map(|result| &result.state),
                    self.scry_checking,
                    self.scry_notice,
                ))
    }
    pub(super) fn notification_count(&self) -> usize {
        usize::from(self.hayai_notification_visible())
            + usize::from(self.scry_notification_visible())
    }
    pub(super) fn notification_kind(&self, row: usize) -> Option<UpdateNotification> {
        notification_kind(
            row,
            self.hayai_notification_visible(),
            self.scry_notification_visible(),
        )
    }
    pub(super) fn selected_notification(&self) -> Option<UpdateNotification> {
        self.notification_selected
            .then(|| self.notification_kind(self.selected_index.map_or(0, |ix| ix.row)))
            .flatten()
    }

    fn main_count(&self) -> usize {
        self.main_results
    }
    fn section_offset(&self) -> usize {
        // Reserve section zero even when empty. An update arriving or being
        // dismissed must not change the indexes of already-selected results.
        1
    }
    pub(super) fn flat_index(&self, ix: IndexPath) -> usize {
        // Notifications are a separate first section, not part of results.
        if ix.section == self.section_offset() {
            ix.row
        } else {
            self.main_count() + ix.row
        }
    }
    pub(super) fn index_path(&self, flat: usize) -> IndexPath {
        result_index_path(flat, self.main_count(), self.results.len())
    }
    pub(super) fn item(&self, ix: IndexPath) -> Option<&CommandItem> {
        if ix.section == 0 {
            return None;
        }
        self.results.get(self.flat_index(ix))
    }
    pub(super) fn new(
        registry: Rc<RefCell<PluginRegistry>>,
        catalog: Rc<RefCell<Catalog>>,
    ) -> Self {
        // These tiny embedded PNGs are decoded before the first frame, rather
        // than appearing after an asynchronous SVG raster job completes.
        let brand_icon = load_builtin_icon(icon_assets::BRAND_PNG);
        let update_icon = load_builtin_icon(icon_assets::UPDATE_PNG);
        let scry_icon = load_builtin_icon(icon_assets::SCRY_PNG);
        let dispatch = registry.borrow_mut().dispatch("");
        let results = registry.borrow().search(&dispatch);
        let main_results = results
            .iter()
            .position(|item| is_hayai_action(&item.action))
            .unwrap_or(results.len());
        Self {
            search_pending: false,
            update_notice: false,
            scry_notice: false,
            show_scry_updates: false,
            scry_checked: None,
            scry_checking: false,
            scry_installing: false,
            scry_install_attempted: false,
            scry_install_target: None,
            scry_ready: false,
            scry_update_dismissed: false,
            scry_discovery: None,
            scry_message: String::new(),
            main_results,
            notification_selected: crate::updates::available_version().is_some(),
            update_dismissed: false,
            checking_update: false,
            update_message: None,
            show_about: false,
            show_updates: false,
            update_result: None,
            apps_view: true,
            brand_icon,
            update_icon,
            update_failed: false,
            restarting_update: false,
            starting_scry: false,
            last_query: String::new(),
            scry_icon,
            settings_icon: load_builtin_icon(icon_assets::SETTINGS_PNG),
            builtin_row_icons: Default::default(),
            builtin_row_pixels: 0,
            file_search: false,
            registry,
            catalog,
            selected_index: (!results.is_empty()).then(IndexPath::default),
            results,
            results_generation: dispatch.generation,
            empty_state: None,
        }
    }

    fn apply_results(
        &mut self,
        generation: u64,
        mut items: Vec<CommandItem>,
        cx: &mut Context<ListState<Self>>,
    ) {
        if crate::plugins::is_stale(self.results_generation, generation) {
            return;
        }
        self.results_generation = generation;
        self.search_pending = false;
        self.empty_state = take_empty_state(&mut items);
        self.main_results = items
            .iter()
            .position(|item| is_hayai_action(&item.action))
            .unwrap_or(items.len());
        self.results = items;
        if self.file_search
            && self.results.iter().any(|item| {
                matches!(
                    item.id.as_str(),
                    "files:compatibility:update"
                        | "files:compatibility:unavailable"
                        | "files:compatibility:updating"
                )
            })
        {
            self.show_scry_updates = true;
            self.show_updates = false;
        }
        self.selected_index = (!self.results.is_empty()).then(|| self.index_path(0));
        cx.notify();
    }
}

/// Empty notices never enter list indexing, keyboard selection, or actions.
/// A later successful query clears the notice through this same function.
pub(super) fn take_empty_state(items: &mut Vec<CommandItem>) -> Option<CommandItem> {
    let notice = items
        .iter()
        .position(|item| {
            matches!(
                item.action,
                CommandAction::EmptyState | CommandAction::PendingState
            )
        })
        .map(|index| items.remove(index));
    items.retain(|item| {
        !matches!(
            item.action,
            CommandAction::EmptyState | CommandAction::PendingState
        )
    });
    if items.is_empty() { notice } else { None }
}

pub(super) fn scry_search_is_ready(items: &[CommandItem]) -> bool {
    !items.iter().any(|item| {
        item.id.starts_with("files:compatibility:")
            || matches!(
                item.action,
                CommandAction::InstallFileSearch | CommandAction::StartFileSearch
            )
    })
}

pub(super) fn section_heading(label: &'static str, cx: &App) -> impl IntoElement {
    div()
        .px_1()
        .pb_1()
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .text_color(cx.theme().muted_foreground)
        .child(label)
}

fn is_hayai_action(action: &CommandAction) -> bool {
    matches!(
        action,
        CommandAction::CheckForUpdates | CommandAction::AboutHayai | CommandAction::OpenSettings
    )
}

impl ListDelegate for ResultListDelegate {
    type Item = ResultRow;

    fn perform_search(
        &mut self,
        query: &str,
        window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Task<()> {
        self.search(query, window, cx)
    }

    fn sections_count(&self, _cx: &App) -> usize {
        self.section_offset()
            + if self.main_count() < self.results.len() {
                2
            } else {
                1
            }
    }
    fn items_count(&self, section: usize, _cx: &App) -> usize {
        if section == 0 {
            return self.notification_count();
        }
        if section == self.section_offset() {
            self.main_count()
        } else {
            self.results.len() - self.main_count()
        }
    }
    fn render_section_header(
        &mut self,
        section: usize,
        _window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Option<impl IntoElement> {
        self.section_header(section, _window, cx)
    }

    fn render_section_footer(
        &mut self,
        _section: usize,
        _window: &mut Window,
        _cx: &mut Context<ListState<Self>>,
    ) -> Option<impl IntoElement> {
        // The virtual list measures footer zero and reuses that height for
        // each section, including the final group.
        self.apps_view.then(|| div().h(px(8.0)).flex_none())
    }

    fn render_empty(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> impl IntoElement {
        self.empty_view(_window, cx)
    }

    fn render_item(
        &mut self,
        ix: IndexPath,
        _window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Option<Self::Item> {
        self.result_row(ix, _window, cx)
    }

    fn set_selected_index(
        &mut self,
        ix: Option<IndexPath>,
        _window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) {
        self.selected_index = ix;
        self.notification_selected =
            self.notification_visible() && ix.is_some_and(|ix| ix.section == 0);
        cx.notify();
    }

    fn confirm(
        &mut self,
        _secondary: bool,
        window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) {
        self.confirm_result(_secondary, window, cx)
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<ListState<Self>>) {
        LauncherState::dismiss(window, cx);
    }
}

mod execution;
mod rendering;
mod search;
mod updates;
