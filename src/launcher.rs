use crate::assets::icons as icon_assets;
use crate::ui_text as text;
use futures::StreamExt;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    AnyElement, AnyWindowHandle, App, Bounds, Context, Entity, FocusHandle, Focusable, FontWeight,
    HighlightStyle, IntoElement, MouseButton, RenderImage, SharedString, StyledText, Subscription,
    Task, Window, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, div, img,
    px, relative, size,
};
use gpui_component::{
    ActiveTheme, IndexPath, Root, Selectable, Sizable, Size, h_flex,
    input::{Escape, Input, InputEvent, InputState, MoveDown, MoveUp},
    list::{List, ListDelegate, ListItem, ListState},
};

use crate::apps::{Catalog, CatalogGlobal, IconRequest};
use crate::commands::{CalculationDetail, CommandAction, CommandItem, IconSource};
use crate::native;
use crate::plugins::PluginRegistry;

mod actions;
mod components;
mod formatting;
mod icons;
mod input;
mod lifecycle;
mod results;
mod router;
mod row;
mod scrolling;
mod view;

use actions::*;
use formatting::*;
pub(crate) use icons::load_builtin_icon;
use icons::*;
pub use lifecycle::{LauncherGlobal, LauncherState};
use results::*;
use row::*;

#[cfg(test)]
pub(crate) use icons::prepare_row_icon;
#[cfg(test)]
mod tests;

const LAUNCHER_WIDTH: f32 = 720.0;
const LAUNCHER_HEIGHT: f32 = 400.0;

const SCRY_SETUP_ICON_SIZE: f32 = 56.0;
const HAYAI_ROW_ICON_SIZE: f32 = 22.0;

/// Run a query through the delegate, then reset `ListState`'s own selection
/// to the first row. `ListState::render_list_item` reads its *own*
/// `selected_index` (not the delegate's copy) to decide row highlighting, so
/// without this every fresh result set (a new query, a reopen, or a recents
/// re-rank) renders with nothing highlighted until an arrow key is pressed.
fn perform_search_and_select_first(
    list: &mut ListState<ResultListDelegate>,
    query: &str,
    window: &mut Window,
    cx: &mut Context<ListState<ResultListDelegate>>,
) {
    std::mem::drop(list.delegate_mut().perform_search(query, window, cx));
    refresh_result_layout(list, window, cx);
}

fn refresh_result_layout(
    list: &mut ListState<ResultListDelegate>,
    window: &mut Window,
    cx: &mut Context<ListState<ResultListDelegate>>,
) {
    let notification =
        list.delegate().last_query.is_empty() && list.delegate().notification_visible();
    let ix = if notification {
        Some(IndexPath::default())
    } else {
        (!list.delegate().results.is_empty()).then(|| list.delegate().index_path(0))
    };
    // gpui-component measures one row for the entire list. App-view rows
    // stay single-line; files retain their uniform two-line layout.
    let measured = if list.delegate().notification_visible() {
        IndexPath::default()
    } else {
        list.delegate().index_path(0)
    };
    list.set_item_to_measure_index(measured, window, cx);
    list.set_selected_index(ix, window, cx);
    list.delegate_mut().notification_selected = notification;
}

struct WheelAnimation {
    motion: crate::scroll::SmoothScroll,
    last_frame: std::time::Instant,
    results_generation: u64,
}

/// The window's root view: hosts our own search `Input` (not the `List`'s
/// built-in one — that hardcodes a search-icon prefix and clear button with
/// no way to remove them) plus the results `List`, and gives gpui-component's
/// `Root` something to wrap.
///
/// `List`'s own keyboard nav (Up/Down/Enter/Escape) only works when its
/// search input is nested inside its own element tree, which we've opted out
/// of. So this view re-implements that nav by listening for the same action
/// types (`MoveUp`/`MoveDown`/`Escape` from `gpui_component::input`, plus
/// `InputEvent::PressEnter`) on an ancestor of our input — `InputState`
/// already `cx.propagate()`s all of these in single-line mode specifically so
/// a containing view can do this.
struct LauncherRoot {
    wheel_animation: Option<WheelAnimation>,
    scroll_handoff: crate::scroll::ScrollHandoff,
    keyboard_heading: Option<(IndexPath, u64)>,
    menu_request: u64,
    list: Entity<ListState<ResultListDelegate>>,
    search_input: Entity<InputState>,
    /// The leading command space is state, not text, so the caret stays at
    /// the start while the placeholder explains which command to enter.
    waiting_for_command: bool,
    /// File mode reconstructs its provider prefix outside the visible input.
    file_mode: bool,
    calculator_mode: bool,
    /// Stack of folders entered via Tab-on-a-folder-result, outermost first.
    /// Empty means file mode is searching everywhere; the last entry scopes
    /// the query under it using the file provider's scoped-search form
    /// syntax. Each Tab push descends one level further; Backspace/Escape on
    /// an empty box pop one level back off, mirroring how you got there.
    folder_scope: Vec<PathBuf>,
    /// Session-only query history, most recent first, capped to
    /// `HISTORY_LIMIT` — recorded on Enter (see `on_search_event`), recalled
    /// with Up/Down when the search box is empty (see `on_move_up`).
    history: Vec<String>,
    /// `None` while typing live; `Some(i)` while browsing `history[i]`.
    history_cursor: Option<usize>,
    action_menu: Option<ActionMenu>,
    _subscriptions: Vec<Subscription>,
}

const HISTORY_LIMIT: usize = 50;

impl LauncherRoot {
    fn new(
        list: Entity<ListState<ResultListDelegate>>,
        search_input: Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let intercept = Self::intercept_keys(window, cx);
        let _subscriptions = vec![
            cx.subscribe_in(&search_input, window, Self::on_search_event),
            cx.observe_in(&list, window, |this, list, window, cx| {
                let delegate = list.read(cx).delegate();
                let return_to_apps =
                    delegate.scry_installing && (this.file_mode || delegate.show_scry_updates);
                if return_to_apps {
                    this.reset_to_apps(window, cx);
                }
                cx.notify();
            }),
            intercept,
        ];
        Self {
            wheel_animation: None,
            scroll_handoff: Default::default(),
            keyboard_heading: None,
            menu_request: 0,
            list,
            search_input,
            waiting_for_command: false,
            file_mode: false,
            calculator_mode: false,
            folder_scope: Vec::new(),
            history: Vec::new(),
            history_cursor: None,
            action_menu: None,
            _subscriptions,
        }
    }

    /// The raw query actually dispatched to `PluginRegistry`, reconstructed
    /// from the visible remainder text plus the active file mode and folder
    /// scope, which are stored outside the input text.
    fn build_query(&self, remainder: &str) -> String {
        match self.folder_scope.last() {
            Some(folder) => {
                let folder_str = folder.to_string_lossy();
                let clean_folder = folder_str.trim_end_matches('\\');
                // `|` cannot occur in a Windows path, so it is a safe internal
                // scope separator. The visible input remains only the user's
                // remainder; FileSearchProvider reconstructs the Scry query.
                format!(" f {clean_folder}|{}", remainder.trim())
            }
            None => format!(" f {}", remainder),
        }
    }

    fn enter_file_mode(&mut self, raw: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.waiting_for_command = false;
        self.calculator_mode = false;
        self.file_mode = true;
        self.folder_scope.clear();
        let remainder = raw
            .strip_prefix(" f")
            .unwrap_or("")
            .trim_start()
            .to_string();
        self.search_input.update(cx, |input, cx| {
            input.set_value(&remainder, window, cx);
            input.set_placeholder(text::search::FILES_PLACEHOLDER, window, cx);
        });
        let query = self.build_query(&remainder);
        self.list.update(cx, |list, cx| {
            perform_search_and_select_first(list, &query, window, cx);
        });
    }

    /// Reset all the way back to the default apps view: not in file mode, no
    /// folder scope, empty box, default placeholder. Does *not* focus the
    /// input — callers that re-show a hidden window handle OS-vs-GPUI focus
    /// ordering themselves (see `LauncherState::show`).
    fn reset_to_apps(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_wheel_animation();
        self.scroll_handoff = Default::default();
        // The window is reused after hiding; transient overlays must not
        // survive into the next Alt+Space session.
        self.action_menu = None;
        self.history_cursor = None;
        self.waiting_for_command = false;
        self.file_mode = false;
        self.calculator_mode = false;
        self.folder_scope.clear();
        self.search_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.set_placeholder(text::search::APPS_PLACEHOLDER, window, cx);
        });
        self.keyboard_heading = self.list.update(cx, |list, cx| {
            perform_search_and_select_first(list, "", window, cx);
            let first = if list.delegate().notification_visible() {
                IndexPath::default()
            } else {
                list.delegate().index_path(0)
            };
            list.scroll_to_item(first, gpui::ScrollStrategy::Top, window, cx);
            let handle = list.scroll_handle();
            handle.set_offset(gpui::point(px(0.0), px(0.0)));
            handle.scroll_to_item(0, gpui::ScrollStrategy::Top);
            Some((first, list.delegate().results_generation))
        });
        cx.notify();
    }

    fn exit_file_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.reset_to_apps(window, cx);
        self.search_input
            .update(cx, |input, cx| input.focus(window, cx));
    }

    /// Pop one level off the folder-scope stack (Tab's inverse) and re-run
    /// the search at whatever level that leaves — the empty root of file
    /// mode if the stack is now empty. Clears the box too: whatever filter
    /// text was typed at the level we're leaving doesn't carry over.
    fn pop_folder_scope(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.folder_scope.pop();
        self.refresh_folder_scope(window, cx);
    }

    fn refresh_folder_scope(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let placeholder = if self.folder_scope.is_empty() {
            text::search::FILES_PLACEHOLDER
        } else {
            text::search::FOLDER_PLACEHOLDER
        };
        self.search_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.set_placeholder(placeholder, window, cx);
        });
        let query = self.build_query("");
        self.list.update(cx, |list, cx| {
            perform_search_and_select_first(list, &query, window, cx);
        });
        cx.notify();
    }

    /// Footer hint text for the currently selected row's Enter action —
    /// e.g. "Copy" for calculator rows instead of the app-launch default.
    fn selected_primary_action(&self, cx: &App) -> PrimaryAction {
        let delegate = self.list.read(cx).delegate();
        // The visible page owns the footer even while typing starts a
        // background search. Pending results must not replace its action.
        if delegate.show_scry_updates || delegate.needs_pinned_scry() {
            return if delegate.scry_installing {
                PrimaryAction::Updating
            } else if delegate.scry_checking && !delegate.needs_pinned_scry() {
                PrimaryAction::Checking
            } else if delegate.scry_ready {
                PrimaryAction::BackToSearch
            } else if delegate.scry_target().is_some() {
                PrimaryAction::UpdateScry
            } else {
                PrimaryAction::CheckAgain
            };
        }
        if delegate.show_updates {
            return if delegate.restarting_update {
                PrimaryAction::Restarting
            } else if delegate.checking_update {
                PrimaryAction::Checking
            } else if delegate.update_result == Some(crate::updates::CheckResult::Ready) {
                PrimaryAction::RestartToUpdate
            } else if delegate.update_result == Some(crate::updates::CheckResult::Available) {
                PrimaryAction::DownloadUpdate
            } else {
                PrimaryAction::CheckAgain
            };
        }
        if delegate.show_about {
            return PrimaryAction::Back;
        }
        if delegate.file_search && delegate.search_pending {
            return PrimaryAction::SearchFiles;
        }
        if delegate.notification_selected {
            if delegate.selected_notification() == Some(UpdateNotification::Scry) {
                return if delegate.scry_installing {
                    PrimaryAction::Updating
                } else if delegate.scry_install_attempted && delegate.scry_ready {
                    PrimaryAction::Dismiss
                } else if delegate.scry_install_attempted {
                    PrimaryAction::RetryUpdate
                } else if delegate.scry_checking {
                    PrimaryAction::Checking
                } else if delegate
                    .scry_discovery
                    .as_ref()
                    .is_some_and(|r| r.state == crate::scry_updates::State::Available)
                {
                    PrimaryAction::ViewUpdate
                } else {
                    PrimaryAction::CheckAgain
                };
            }
            return if delegate.checking_update {
                PrimaryAction::Checking
            } else if delegate.restarting_update {
                PrimaryAction::Restarting
            } else {
                if crate::updates::ready_version().is_some() {
                    PrimaryAction::ViewUpdate
                } else {
                    PrimaryAction::CheckAgain
                }
            };
        }
        if delegate.starting_scry {
            return PrimaryAction::Starting;
        }
        if delegate.file_search && delegate.results.is_empty() {
            return PrimaryAction::SearchFiles;
        }
        if let Some(item) = delegate.results.first()
            && item.id.starts_with("files:compatibility:")
        {
            return if item.id.ends_with(":error") {
                PrimaryAction::Unavailable
            } else if item.id.ends_with(":unavailable") {
                PrimaryAction::CheckForUpdates
            } else if item.id.ends_with(":newer") {
                PrimaryAction::UpdateHayai
            } else {
                PrimaryAction::UpdateRequired
            };
        }
        let action = delegate
            .selected_index
            .and_then(|ix| delegate.item(ix))
            .map(|item| &item.action);
        match action {
            Some(CommandAction::CopyToClipboard(_)) => PrimaryAction::Copy,
            Some(CommandAction::OpenFile(path)) if path.is_dir() => PrimaryAction::OpenFolder,
            Some(CommandAction::OpenFile(_)) => PrimaryAction::OpenFile,
            Some(CommandAction::ShowText(_)) => PrimaryAction::Show,
            Some(CommandAction::EmptyState | CommandAction::PendingState) => PrimaryAction::None,
            Some(CommandAction::InstallFileSearch) => PrimaryAction::Install,
            Some(CommandAction::StartFileSearch) => PrimaryAction::Start,
            Some(CommandAction::UpdateFileSearch) => PrimaryAction::UpdateScry,
            Some(CommandAction::CheckFileSearchUpdates) => PrimaryAction::CheckForUpdates,
            Some(CommandAction::RestartToUpdate) => PrimaryAction::RestartToUpdate,
            Some(CommandAction::CheckForUpdates) => PrimaryAction::CheckForUpdates,
            Some(CommandAction::DownloadHayaiUpdate) => PrimaryAction::DownloadUpdate,
            Some(CommandAction::AboutHayai) => PrimaryAction::AboutHayai,
            Some(CommandAction::OpenSettings) => PrimaryAction::OpenSettings,
            Some(CommandAction::LaunchApplication(_)) | None => PrimaryAction::OpenApplication,
        }
    }
}

impl Focusable for LauncherRoot {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.search_input.read(cx).focus_handle(cx)
    }
}
