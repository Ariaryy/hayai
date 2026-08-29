use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    AnyElement, AnyWindowHandle, App, Bounds, Context, Entity, FocusHandle, Focusable, FontWeight,
    Half, HighlightStyle, IntoElement, MouseButton, RenderImage, SharedString, StyledText,
    Subscription, Task, Window, WindowBackgroundAppearance, WindowBounds, WindowKind,
    WindowOptions, div, img, px, relative, size,
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

const LAUNCHER_WIDTH: f32 = 720.0;
const LAUNCHER_HEIGHT: f32 = 400.0;

fn format_expression_for_display(expression: &str) -> String {
    const SUPERSCRIPT_DIGITS: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];

    let mut formatted = String::with_capacity(expression.len());
    let mut characters = expression.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '^' {
            formatted.push(character);
            continue;
        }

        let mut exponent = String::new();
        if characters.peek() == Some(&'-') {
            characters.next();
            exponent.push('⁻');
        }
        while let Some(digit) = characters.peek().and_then(|next| next.to_digit(10)) {
            characters.next();
            exponent.push(SUPERSCRIPT_DIGITS[digit as usize]);
        }

        if exponent.is_empty() || exponent == "⁻" {
            formatted.push('^');
            if exponent == "⁻" {
                formatted.push('-');
            }
        } else {
            formatted.push_str(&exponent);
        }
    }
    formatted
}

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
    let ix = (list.delegate().items_count(0, cx) > 0).then(IndexPath::default);
    list.set_selected_index(ix, window, cx);
}

#[derive(Clone)]
pub struct LauncherGlobal {
    state: Rc<RefCell<LauncherState>>,
}

impl LauncherGlobal {
    pub fn clone_handle(&self) -> Rc<RefCell<LauncherState>> {
        self.state.clone()
    }
}

impl gpui::Global for LauncherGlobal {}

#[derive(Default)]
pub struct LauncherState {
    window: Option<AnyWindowHandle>,
    list: Option<Entity<ListState<ResultListDelegate>>>,
    search_input: Option<Entity<InputState>>,
    /// Held so `show` can reset file mode / folder scope along with the
    /// query text — without this, re-showing a window that was left in file
    /// mode reset only the visible list (to apps) while the pill and mode
    /// flag stayed stuck on "Files", desyncing the two.
    view: Option<Entity<LauncherRoot>>,
    visible: bool,
}

impl LauncherState {
    pub fn install(cx: &mut App) -> Rc<RefCell<Self>> {
        let state = Rc::new(RefCell::new(Self::default()));
        cx.set_global(LauncherGlobal {
            state: state.clone(),
        });
        state
    }

    pub fn toggle(&mut self, cx: &mut App) {
        if self.visible {
            self.hide(cx);
        } else {
            self.show(cx);
        }
    }

    pub fn show(&mut self, cx: &mut App) {
        if let (Some(handle), Some(_list), Some(search_input), Some(view)) = (
            self.window,
            self.list.clone(),
            self.search_input.clone(),
            self.view.clone(),
        ) {
            let shown = handle
                .update(cx, |_, window, cx| {
                    // Reset all the way back to the default apps view (picks up
                    // any recents reordering from apps launched since the window
                    // was last hidden) *before* the Win32 show below. The window
                    // is still hidden here, so GPUI paints the final, reset frame
                    // while off-screen — doing this after ShowWindow would let
                    // the stale frame flash on screen for one frame first.
                    //
                    // Routed through the view (rather than poking search_input /
                    // list directly) so file mode and folder scope get reset too
                    // — otherwise a window hidden mid file-search reopened with
                    // the "Files" pill still showing while the list underneath
                    // had silently gone back to apps.
                    view.update(cx, |view, cx| view.reset_to_apps(window, cx));
                    // Win32 focus next (this re-shows the hidden window via SW_SHOW and
                    // calls SetForegroundWindow etc.), then GPUI focus. Reversed order
                    // (GPUI focus before Win32 focus) would have GPUI's WM_SETFOCUS
                    // handler clobber our focus state.
                    native::focus_launcher_window(window);
                    search_input.update(cx, |input, cx| input.focus(window, cx));
                })
                .is_ok();
            if shown {
                self.visible = true;
                return;
            }

            self.window = None;
            self.list = None;
            self.search_input = None;
            self.view = None;
        }

        let bounds = Bounds::centered(None, size(px(LAUNCHER_WIDTH), px(LAUNCHER_HEIGHT)), cx);

        // `open_window`'s closure returns the window's root view (a `Root`), so we
        // stash the inner entities here to read back out afterward.
        type OpenedEntities = (
            Entity<ListState<ResultListDelegate>>,
            Entity<InputState>,
            Entity<LauncherRoot>,
        );
        let entities_slot: Rc<RefCell<Option<OpenedEntities>>> = Rc::new(RefCell::new(None));
        let entities_slot_for_window = entities_slot.clone();

        let handle = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: None,
                    focus: true,
                    show: true,
                    kind: WindowKind::Normal,
                    is_movable: false,
                    is_resizable: false,
                    is_minimizable: false,
                    window_background: WindowBackgroundAppearance::Opaque,
                    ..Default::default()
                },
                move |window, cx| {
                    let catalog = cx.global::<CatalogGlobal>().clone_handle();
                    let registry = cx.global::<crate::plugins::RegistryGlobal>().clone_handle();
                    let list = cx.new(|cx| {
                        ListState::new(ResultListDelegate::new(registry, catalog), window, cx)
                            .searchable(false)
                    });
                    list.update(cx, |list, cx| {
                        let ix = (list.delegate().items_count(0, cx) > 0).then(IndexPath::default);
                        list.set_selected_index(ix, window, cx);
                    });
                    let search_input = cx.new(|cx| {
                        InputState::new(window, cx).placeholder("Search apps, files, commands...")
                    });
                    let view = cx.new(|cx| {
                        LauncherRoot::new(list.clone(), search_input.clone(), window, cx)
                    });

                    *entities_slot_for_window.borrow_mut() =
                        Some((list.clone(), search_input.clone(), view.clone()));

                    // Close on focus loss (clicking elsewhere, Alt+Tab, etc.).
                    // The observer fires for both activation and deactivation, so we
                    // only act on deactivation. We also gate on our own `visible`
                    // flag: hiding the window (SW_HIDE) itself deactivates it and
                    // re-enters this observer, so without the guard we'd recurse.
                    view.update(cx, |_view, cx| {
                        cx.observe_window_activation(window, |_view, window, cx| {
                            if window.is_window_active() {
                                return;
                            }
                            let visible = cx
                                .global::<LauncherGlobal>()
                                .clone_handle()
                                .borrow()
                                .visible;
                            if visible {
                                LauncherState::dismiss(window, cx);
                            }
                        })
                        .detach();
                    });

                    cx.new(|cx| Root::new(view, window, cx))
                },
            )
            .expect("failed to open launcher window");

        let (list, search_input, view) = entities_slot
            .borrow_mut()
            .take()
            .expect("entities created during open_window");

        // Win32 focus first, then GPUI focus (same ordering as the re-show path above).
        let _ = handle.update(cx, |_, window, cx| {
            native::focus_launcher_window(window);
            search_input.update(cx, |input, cx| input.focus(window, cx));
        });

        self.window = Some(handle.into());
        self.list = Some(list);
        self.search_input = Some(search_input);
        self.view = Some(view);
        self.visible = true;
    }

    /// Re-run the current query against the (possibly just-rescanned) catalog.
    /// Called from a bare `&mut App` context (the scan-completion task), which
    /// is NOT mid-window-update, so `handle.update` is safe here (pitfall #1).
    pub fn refresh_results(&self, cx: &mut App) {
        let (Some(handle), Some(list), Some(search_input)) =
            (self.window, self.list.clone(), self.search_input.clone())
        else {
            return;
        };
        let _ = handle.update(cx, |_, window, cx| {
            let query = search_input.read(cx).value().to_string();
            list.update(cx, |list, cx| {
                perform_search_and_select_first(list, &query, window, cx);
            });
        });
    }

    pub fn hide(&mut self, cx: &mut App) {
        let Some(handle) = self.window else {
            return;
        };

        // Mark hidden *before* touching the OS window: SW_HIDE deactivates the
        // window, which fires our activation observer; the `visible` flag is how
        // that observer knows not to re-dismiss.
        self.visible = false;
        let _ = handle.update(cx, |_, window, _| native::hide_launcher_window(window));
    }

    /// Dismiss the launcher from *inside* a window update (e.g. a list delegate
    /// callback or a window-activation observer).
    ///
    /// We hide rather than destroy the window: on Windows GPUI quits the whole app
    /// when its last window closes, but we want to stay resident in the tray.
    ///
    /// We also must not re-enter `handle.update` here: callbacks and observers
    /// already run while the window has been taken out of `App::windows`, so a
    /// nested update would fail with "window not found" and silently do nothing.
    /// Instead we hide through the `Window` we already hold and update the shared
    /// `visible` flag directly.
    fn dismiss(window: &mut Window, cx: &mut App) {
        cx.global::<LauncherGlobal>()
            .clone_handle()
            .borrow_mut()
            .visible = false;
        native::hide_launcher_window(window);
    }
}

/// A keycap badge for the Enter key, styled like `gpui_component::kbd::Kbd`
/// but showing the return-arrow glyph instead of the word "Enter" — `Kbd`'s
/// own text comes from a fixed, non-overridable platform format (the literal
/// word "Enter" on Windows).
fn enter_kbd(cx: &App) -> impl IntoElement {
    div()
        .text_color(cx.theme().muted_foreground)
        .bg(cx.theme().tokens.muted)
        .py_0p5()
        .px_1()
        .min_w_5()
        .text_center()
        .rounded(cx.theme().radius.half())
        .line_height(relative(1.0))
        .text_xs()
        .flex_shrink_0()
        .child("⏎")
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
#[derive(Clone)]
struct ActionMenu {
    path: PathBuf,
    is_dir: bool,
    can_run_as_admin: bool,
    selected: usize,
}

fn ctrl_k_kbd(cx: &App) -> impl IntoElement {
    div()
        .text_color(cx.theme().muted_foreground)
        .bg(cx.theme().tokens.muted)
        .py_0p5()
        .px_1()
        .text_center()
        .rounded(cx.theme().radius.half())
        .line_height(relative(1.0))
        .text_xs()
        .flex_shrink_0()
        .child("Ctrl K")
}

#[derive(Clone, Copy)]
enum MenuAction {
    Open,
    Reveal,
    CopyPath,
    RunAsAdmin,
}

fn can_run_as_admin(path: &std::path::Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some(extension)
            if extension.eq_ignore_ascii_case("exe")
                || extension.eq_ignore_ascii_case("com")
                || extension.eq_ignore_ascii_case("bat")
                || extension.eq_ignore_ascii_case("cmd")
                || extension.eq_ignore_ascii_case("msi")
    )
}

struct LauncherRoot {
    list: Entity<ListState<ResultListDelegate>>,
    search_input: Entity<InputState>,
    /// Whether the input is in " f " (file search) mode — rendered as a pill
    /// instead of literal prefix text in the box, per the keyword the same
    /// prefix routes to in `PluginRegistry::dispatch`.
    file_mode: bool,
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
        let this = cx.entity();
        // `InputState` owns Backspace and Tab as key *bindings* (its "Input"
        // key context is more specific than any context we could put on an
        // ancestor div), so a normal `on_key_down`/`on_action` listener up
        // here never sees them once InputState's handler consumes the event.
        // `intercept_keystrokes` runs before action dispatch entirely (see
        // its doc comment), so it's the only hook that can steal these two
        // keys away from the input while it still holds focus.
        let intercept = cx.intercept_keystrokes(move |event, window, cx| {
            let key = event.keystroke.key.as_str();
            let control_k = event.keystroke.modifiers.control && key == "k";
            if key != "backspace" && key != "tab" && !control_k {
                return;
            }
            this.update(cx, |this, cx| {
                this.on_intercepted_key(key, control_k, window, cx);
            });
        });
        let _subscriptions = vec![
            cx.subscribe_in(&search_input, window, Self::on_search_event),
            intercept,
        ];
        Self {
            list,
            search_input,
            file_mode: false,
            folder_scope: Vec::new(),
            history: Vec::new(),
            history_cursor: None,
            action_menu: None,
            _subscriptions,
        }
    }

    /// The raw query actually dispatched to `PluginRegistry`, reconstructed
    /// from the visible remainder text plus whatever mode/scope state the
    /// pill and folder navigation have accumulated (neither of which is
    /// visible in the input box itself).
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
        self.file_mode = true;
        self.folder_scope.clear();
        let remainder = raw
            .strip_prefix(" f")
            .unwrap_or("")
            .trim_start()
            .to_string();
        self.search_input.update(cx, |input, cx| {
            input.set_value(&remainder, window, cx);
            input.set_placeholder("Search files and folders...", window, cx);
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
        self.file_mode = false;
        self.folder_scope.clear();
        self.search_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.set_placeholder("Search apps, files, commands...", window, cx);
        });
        self.list.update(cx, |list, cx| {
            perform_search_and_select_first(list, "", window, cx);
        });
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
        self.search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        let query = self.build_query("");
        self.list.update(cx, |list, cx| {
            perform_search_and_select_first(list, &query, window, cx);
        });
    }

    fn selected_action_menu(&self, cx: &App) -> Option<ActionMenu> {
        let delegate = self.list.read(cx).delegate();
        delegate
            .selected_index
            .and_then(|ix| delegate.results.get(ix.row))
            .and_then(|item| match &item.action {
                CommandAction::OpenFile(path) => Some(ActionMenu {
                    path: path.clone(),
                    is_dir: path.is_dir(),
                    can_run_as_admin: can_run_as_admin(path),
                    selected: 0,
                }),
                CommandAction::LaunchApplication(path) => Some(ActionMenu {
                    path: path.clone(),
                    is_dir: false,
                    can_run_as_admin: true,
                    selected: 0,
                }),
                CommandAction::CopyToClipboard(_)
                | CommandAction::ShowText(_)
                | CommandAction::InstallFileSearch => None,
            })
    }

    fn selected_menu_action(&self) -> Option<MenuAction> {
        let menu = self.action_menu.as_ref()?;
        match menu.selected {
            0 => Some(MenuAction::Open),
            1 => Some(MenuAction::Reveal),
            2 => Some(MenuAction::CopyPath),
            3 if menu.can_run_as_admin => Some(MenuAction::RunAsAdmin),
            _ => None,
        }
    }

    fn move_menu_selection(&mut self, delta: isize, cx: &mut Context<Self>) -> bool {
        let Some(menu) = self.action_menu.as_mut() else {
            return false;
        };
        let count = if menu.can_run_as_admin { 4 } else { 3 };
        menu.selected = (menu.selected as isize + delta).rem_euclid(count) as usize;
        cx.notify();
        true
    }

    fn run_menu_action(&mut self, action: MenuAction, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = self.action_menu.take() else {
            return;
        };
        match action {
            MenuAction::Open => self.list.update(cx, |list, cx| {
                list.delegate_mut().confirm(false, window, cx);
            }),
            MenuAction::CopyPath => {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                    menu.path.to_string_lossy().into_owned(),
                ));
                LauncherState::dismiss(window, cx);
            }
            MenuAction::Reveal => {
                let path = menu.path.parent().unwrap_or(&menu.path).to_path_buf();
                LauncherState::dismiss(window, cx);
                cx.background_spawn(async move {
                    native::launch_path(&path);
                })
                .detach();
            }
            MenuAction::RunAsAdmin if menu.can_run_as_admin => {
                LauncherState::dismiss(window, cx);
                cx.background_spawn(async move {
                    native::launch_path_as_admin(&menu.path);
                })
                .detach();
            }
            MenuAction::RunAsAdmin => {}
        }
    }

    fn on_search_event(
        &mut self,
        input: &Entity<InputState>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Change => {
                self.action_menu = None;
                // Only real user edits reach this arm — our own history-recall
                // `set_value` calls below drive `perform_search_and_select_first`
                // directly and don't emit `Change`, so this can't clobber a
                // recall in progress.
                self.history_cursor = None;
                let raw = input.read(cx).value().to_string();
                if !self.file_mode && (raw == " f" || raw.starts_with(" f ")) {
                    self.enter_file_mode(&raw, window, cx);
                    return;
                }
                let query = if self.file_mode {
                    self.build_query(&raw)
                } else {
                    raw
                };
                self.list.update(cx, |list, cx| {
                    perform_search_and_select_first(list, &query, window, cx);
                });
            }
            InputEvent::PressEnter { .. } => {
                if let Some(action) = self.selected_menu_action() {
                    self.run_menu_action(action, window, cx);
                    return;
                }
                let text = input.read(cx).value().to_string();
                if !text.is_empty()
                    && self.history.first().map(String::as_str) != Some(text.as_str())
                {
                    self.history.insert(0, text);
                    self.history.truncate(HISTORY_LIMIT);
                }
                self.history_cursor = None;
                self.list.update(cx, |list, cx| {
                    list.delegate_mut().confirm(false, window, cx);
                });
            }
            _ => {}
        }
    }

    /// Tab on a folder result enters it: scope subsequent queries under it
    /// (via the scoped-search form in `build_query`) without
    /// leaving the launcher, so the same key can descend further into
    /// nested folders.
    ///
    /// Called from the app-wide `intercept_keystrokes` hook registered in
    /// `new` — `InputState` consumes Backspace and Tab itself as key
    /// bindings (with no `cx.propagate()`), so an ordinary `on_key_down`
    /// listener on an ancestor div never sees them while the input holds
    /// focus. Interception runs before action/keymap dispatch entirely, so
    /// it's the only hook that can steal these two keys away first.
    fn on_intercepted_key(
        &mut self,
        key: &str,
        control_k: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if control_k {
            cx.stop_propagation();
            self.action_menu = if self.action_menu.is_some() {
                None
            } else {
                self.selected_action_menu(cx)
            };
            cx.notify();
            return;
        }
        if !self.file_mode {
            return;
        }

        // Backspace on an already-empty box steps back out instead of
        // being a no-op: pop the folder scope first if we're inside one
        // (mirrors Escape), only exiting file mode entirely once there's
        // no scope left to back out of.
        if key == "backspace" && self.search_input.read(cx).value().is_empty() {
            cx.stop_propagation();
            if self.folder_scope.is_empty() {
                self.exit_file_mode(window, cx);
            } else {
                self.pop_folder_scope(window, cx);
            }
            return;
        }

        if key != "tab" {
            return;
        }
        let selected_dir = self.list.read(cx).selected_index().and_then(|ix| {
            self.list
                .read(cx)
                .delegate()
                .results
                .get(ix.row)
                .and_then(|item| match &item.action {
                    CommandAction::OpenFile(path) if path.is_dir() => Some(path.clone()),
                    _ => None,
                })
        });
        let Some(dir) = selected_dir else {
            return;
        };
        cx.stop_propagation();
        self.folder_scope.push(dir);
        self.search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        let query = self.build_query("");
        self.list.update(cx, |list, cx| {
            perform_search_and_select_first(list, &query, window, cx);
        });
    }

    fn move_selection(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        if self.action_menu.take().is_some() {
            cx.notify();
        }
        self.list.update(cx, |list, cx| {
            let count = list.delegate().items_count(0, cx);
            if count == 0 {
                return;
            }
            let current = list
                .selected_index()
                .map(|ix| ix.row as isize)
                .unwrap_or(-1);
            let next = (current + delta).rem_euclid(count as isize) as usize;
            list.set_selected_index(Some(IndexPath::new(next)), window, cx);
            list.scroll_to_selected_item(window, cx);
        });
    }

    fn on_move_up(&mut self, _: &MoveUp, window: &mut Window, cx: &mut Context<Self>) {
        if self.move_menu_selection(-1, cx) {
            return;
        }
        let at_top = self
            .list
            .read(cx)
            .selected_index()
            .map(|ix| ix.row)
            .unwrap_or(0)
            == 0;
        if self.search_input.read(cx).value().is_empty() && at_top {
            // Empty query *and* already on the top result: Up is history
            // recall (even when there's no history yet, so it never falls
            // through to `move_selection`, which would wrap Up on the first
            // row around to the last row and look like backwards nav).
            // Below the top result, Up should just move the selection up
            // like normal list navigation.
            self.recall_history(1, window, cx);
            return;
        }
        self.move_selection(-1, window, cx);
    }

    fn on_move_down(&mut self, _: &MoveDown, window: &mut Window, cx: &mut Context<Self>) {
        if self.move_menu_selection(1, cx) {
            return;
        }
        if self.history_cursor.is_some()
            && self.search_input.read(cx).value().is_empty()
            && self.recall_history(-1, window, cx)
        {
            return;
        }
        self.move_selection(1, window, cx);
    }

    /// Shell-history-style Up/Down: `delta = 1` steps to an older query,
    /// `delta = -1` to a newer one (or back to empty). Only engages while
    /// the search box is empty (see callers), so it never fights normal
    /// list navigation once the user is typing or browsing results.
    /// Returns `false` when there's nowhere to go, so the caller can fall
    /// back to ordinary list-selection movement.
    fn recall_history(
        &mut self,
        delta: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let next = match (self.history_cursor, delta) {
            (None, 1) if !self.history.is_empty() => Some(0),
            (Some(i), 1) if i + 1 < self.history.len() => Some(i + 1),
            (Some(i), -1) if i > 0 => Some(i - 1),
            (Some(_), -1) => None,
            _ => return false,
        };
        self.history_cursor = next;
        let text = next
            .and_then(|i| self.history.get(i))
            .cloned()
            .unwrap_or_default();
        self.search_input
            .update(cx, |input, cx| input.set_value(&text, window, cx));
        let query = if self.file_mode {
            self.build_query(&text)
        } else {
            text
        };
        self.list.update(cx, |list, cx| {
            perform_search_and_select_first(list, &query, window, cx);
        });
        true
    }

    fn on_escape(&mut self, _: &Escape, window: &mut Window, cx: &mut Context<Self>) {
        if self.action_menu.take().is_some() {
            cx.notify();
            return;
        }
        // Escape backs out one level at a time: folder scope first (same
        // stack as Backspace), then file mode entirely, then a non-empty
        // query just gets cleared, and only once all of those are already
        // clear does it fall through to dismissing the launcher.
        if !self.folder_scope.is_empty() {
            self.pop_folder_scope(window, cx);
            return;
        }
        if self.file_mode {
            self.exit_file_mode(window, cx);
            return;
        }
        if !self.search_input.read(cx).value().is_empty() {
            self.history_cursor = None;
            self.search_input
                .update(cx, |input, cx| input.set_value("", window, cx));
            self.list.update(cx, |list, cx| {
                perform_search_and_select_first(list, "", window, cx);
            });
            return;
        }
        self.list.update(cx, |list, cx| {
            list.delegate_mut().cancel(window, cx);
        });
    }

    /// Footer hint text for the currently selected row's Enter action —
    /// e.g. "Copy" for calculator rows instead of the app-launch default.
    fn selected_action_label(&self, cx: &App) -> &'static str {
        let delegate = self.list.read(cx).delegate();
        let action = delegate
            .selected_index
            .and_then(|ix| delegate.results.get(ix.row))
            .map(|item| &item.action);
        match action {
            Some(CommandAction::CopyToClipboard(_)) => "Copy",
            Some(CommandAction::OpenFile(path)) if path.is_dir() => "Open Folder",
            Some(CommandAction::OpenFile(_)) => "Open File",
            Some(CommandAction::ShowText(_)) => "Show",
            Some(CommandAction::InstallFileSearch) => "Enable",
            Some(CommandAction::LaunchApplication(_)) | None => "Open Application",
        }
    }
}

impl Focusable for LauncherRoot {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.search_input.read(cx).focus_handle(cx)
    }
}

impl gpui::Render for LauncherRoot {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .border_1()
            .border_color(cx.theme().border)
            .shadow_lg()
            .on_action(cx.listener(Self::on_move_up))
            .on_action(cx.listener(Self::on_move_down))
            .on_action(cx.listener(Self::on_escape))
            .child(
                div()
                    .flex_none()
                    .h(px(52.0))
                    .w_full()
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .when(self.file_mode, |el| {
                        let label = match self.folder_scope.last() {
                            Some(dir) => dir
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_else(|| dir.to_string_lossy().into_owned()),
                            None => "Files".to_string(),
                        };
                        el.child(
                            div()
                                .flex_none()
                                .ml_3()
                                .flex()
                                .items_center()
                                .gap_1()
                                .h(px(28.0))
                                .px_2()
                                .rounded(cx.theme().radius.half())
                                .bg(cx.theme().tokens.muted)
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .cursor_pointer()
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _, window, cx| {
                                        this.exit_file_mode(window, cx);
                                    }),
                                )
                                .child(label)
                                .child(div().child("×")),
                        )
                    })
                    .child(
                        Input::new(&self.search_input)
                            .appearance(false)
                            .with_size(Size::Large)
                            .h(px(52.0))
                            .text_size(px(16.0))
                            // `Input`'s own internal render sets a fixed
                            // `line_height(1.25rem)` unconditionally; that's
                            // too tight for some glyphs' descenders (e.g. "g")
                            // at this font size and clips them. Overriding it
                            // here works because `refine_style` only replaces
                            // fields we actually set, and unlike `text_size`,
                            // we weren't setting this one before.
                            .line_height(relative(1.5)),
                    ),
            )
            .child(
                List::new(&self.list)
                    .scrollbar_visible(false)
                    .with_size(Size::Large)
                    .p_2()
                    .flex_1(),
            )
            .child(
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
                    .child(ctrl_k_kbd(cx))
                    .child("Actions")
                    .child(self.selected_action_label(cx))
                    .child(enter_kbd(cx)),
            )
            .when_some(self.action_menu.clone(), |root, menu| {
                let open_label = if menu.is_dir {
                    "Open folder"
                } else if menu.can_run_as_admin {
                    "Run"
                } else {
                    "Open"
                };
                let reveal_label = if menu.is_dir {
                    "Open parent folder"
                } else {
                    "Show in folder"
                };
                root.child(
                    div()
                        .absolute()
                        .right(px(12.0))
                        .bottom(px(48.0))
                        .w(px(236.0))
                        .p_2()
                        .bg(cx.theme().background)
                        .border_1()
                        .border_color(cx.theme().border)
                        .rounded(cx.theme().radius)
                        .shadow_lg()
                        .child(
                            h_flex()
                                .justify_between()
                                .px_1()
                                .pb_1()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("Actions")
                                .child("↑↓ choose · Enter run · Esc close"),
                        )
                        .child(
                            div()
                                .px_2()
                                .py_1()
                                .rounded(cx.theme().radius)
                                .when(menu.selected == 0, |row| row.bg(cx.theme().tokens.muted))
                                .child(open_label),
                        )
                        .child(
                            div()
                                .px_2()
                                .py_1()
                                .rounded(cx.theme().radius)
                                .when(menu.selected == 1, |row| row.bg(cx.theme().tokens.muted))
                                .child(reveal_label),
                        )
                        .child(
                            div()
                                .px_2()
                                .py_1()
                                .rounded(cx.theme().radius)
                                .when(menu.selected == 2, |row| row.bg(cx.theme().tokens.muted))
                                .child("Copy path"),
                        )
                        .when(menu.can_run_as_admin, |panel| {
                            panel.child(
                                div()
                                    .mt_1()
                                    .px_2()
                                    .py_1()
                                    .rounded(cx.theme().radius)
                                    .text_color(cx.theme().accent)
                                    .when(menu.selected == 3, |row| row.bg(cx.theme().tokens.muted))
                                    .child("Run as administrator"),
                            )
                        }),
                )
            })
    }
}

/// Drives the results list: dispatches each query through the
/// [`PluginRegistry`], renders one row per [`CommandItem`], and runs the
/// selected item's action on confirm. Provider-agnostic — apps are just the
/// current default provider.
struct ResultListDelegate {
    registry: Rc<RefCell<PluginRegistry>>,
    /// Catalog handle retained ONLY for the icon cache (`IconSource::Path`).
    catalog: Rc<RefCell<Catalog>>,
    results: Vec<CommandItem>,
    /// Generation the current `results` belong to (see `PluginRegistry`).
    results_generation: u64,
    selected_index: Option<IndexPath>,
}

impl ResultListDelegate {
    fn new(registry: Rc<RefCell<PluginRegistry>>, catalog: Rc<RefCell<Catalog>>) -> Self {
        let dispatch = registry.borrow_mut().dispatch("");
        let results = registry.borrow().search(&dispatch);
        Self {
            registry,
            catalog,
            selected_index: (!results.is_empty()).then(IndexPath::default),
            results,
            results_generation: dispatch.generation,
        }
    }

    fn apply_results(
        &mut self,
        generation: u64,
        items: Vec<CommandItem>,
        cx: &mut Context<ListState<Self>>,
    ) {
        if crate::plugins::is_stale(self.results_generation, generation) {
            return;
        }
        self.results_generation = generation;
        self.results = items;
        self.selected_index = (!self.results.is_empty()).then(IndexPath::default);
        cx.notify();
    }
}

impl ListDelegate for ResultListDelegate {
    type Item = ResultRow;

    fn perform_search(
        &mut self,
        query: &str,
        window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Task<()> {
        let dispatch = self.registry.borrow_mut().dispatch(query);
        if dispatch.debounce
            && let Some(job) = self.registry.borrow().background_search(&dispatch)
        {
            let generation = dispatch.generation;
            let registry = self.registry.clone();
            cx.spawn_in(window, async move |list, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(8))
                    .await;
                // Foreground again after the await: the query may have moved
                // on while we slept, so drop this result set if so.
                if registry.borrow().current_generation() != generation {
                    return;
                }
                let items = cx.background_spawn(async move { job() }).await;
                if registry.borrow().current_generation() != generation {
                    return;
                }
                let _ = list.update(cx, |list, cx| {
                    list.delegate_mut().apply_results(generation, items, cx);
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

    fn items_count(&self, _section: usize, _cx: &App) -> usize {
        self.results.len()
    }

    fn render_item(
        &mut self,
        ix: IndexPath,
        _window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Option<Self::Item> {
        let item = self.results.get(ix.row)?.clone();
        let icon = match &item.icon {
            IconSource::Path(path) => {
                let mut catalog = self.catalog.borrow_mut();
                match catalog.request_icon(path) {
                    IconRequest::Ready(icon) => icon,
                    IconRequest::Loading => None,
                    IconRequest::Load(path) => {
                        drop(catalog);
                        spawn_icon_load(path, self.catalog.clone(), cx);
                        None
                    }
                }
            }
            IconSource::None => None,
        };
        let selected = self.selected_index == Some(ix);
        // Only `CalcProvider` produces `CopyToClipboard` today, so this
        // doubles as "is this a calculator result" without adding a field
        // to `CommandItem` that every other provider would have to fill in.
        let calculator = matches!(item.action, CommandAction::CopyToClipboard(_));
        Some(ResultRow::new(
            ix,
            item.title,
            item.subtitle,
            item.calculation_detail,
            icon,
            selected,
            calculator,
        ))
    }

    fn set_selected_index(
        &mut self,
        ix: Option<IndexPath>,
        _window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) {
        self.selected_index = ix;
        cx.notify();
    }

    fn confirm(
        &mut self,
        _secondary: bool,
        window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) {
        // Bookkeeping first (cheap, in-memory), then hide the window
        // *immediately* — a shell call can take hundreds of ms and must not
        // hold the launcher on screen after Enter.
        let action = self
            .selected_index
            .and_then(|ix| self.results.get(ix.row))
            .map(|item| item.action.clone());
        match action {
            Some(CommandAction::LaunchApplication(path)) => {
                let recents = self.catalog.borrow_mut().mark_launched_path(path.clone());
                LauncherState::dismiss(window, cx);
                cx.background_spawn(async move {
                    native::launch_path(&path);
                    crate::apps::write_recents_file(&recents);
                })
                .detach();
            }
            Some(CommandAction::OpenFile(path)) => {
                let recents = cx
                    .global::<crate::files::FileRecentsGlobal>()
                    .clone_handle();
                let contents = recents.borrow_mut().mark_opened(path.clone());
                LauncherState::dismiss(window, cx);
                cx.background_spawn(async move {
                    native::launch_path(&path);
                    crate::files::write_file_recents_file(&contents);
                })
                .detach();
            }
            Some(CommandAction::CopyToClipboard(text)) => {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                LauncherState::dismiss(window, cx);
            }
            Some(CommandAction::ShowText(_)) => {
                // Stays on screen; no-op.
            }
            Some(CommandAction::InstallFileSearch) => {
                LauncherState::dismiss(window, cx);
                cx.background_spawn(async move {
                    native::install_scry_daemon();
                })
                .detach();
            }
            None => {
                LauncherState::dismiss(window, cx);
            }
        }
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<ListState<Self>>) {
        LauncherState::dismiss(window, cx);
    }
}

/// Decode `path`'s icon on the background thread pool, then hand the result
/// back to the catalog and repaint. GDI/COM icon extraction is too slow to do
/// synchronously in the render path (it blocks keystrokes).
fn spawn_icon_load(
    path: PathBuf,
    catalog: Rc<RefCell<Catalog>>,
    cx: &mut Context<ListState<ResultListDelegate>>,
) {
    cx.spawn(async move |list, cx| {
        let decode_path = path.clone();
        let rendered = cx
            .background_spawn(async move { native::extract_icon_rgba(&decode_path) })
            .await
            .and_then(|icon| {
                let buffer = image::RgbaImage::from_raw(icon.width, icon.height, icon.bgra)?;
                Some(Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])))
            });

        catalog.borrow_mut().set_icon_ready(path, rendered);
        let _ = list.update(cx, |_, cx| cx.notify());
    })
    .detach();
}

#[derive(gpui::IntoElement)]
struct ResultRow {
    base: ListItem,
    title: SharedString,
    subtitle: Option<SharedString>,
    calculation_detail: Option<CalculationDetail>,
    icon: Option<Arc<RenderImage>>,
    selected: bool,
    /// Calculator results (see `ResultListDelegate::render_item`) render as
    /// a distinct "question small / answer big" card rather than the plain
    /// icon+title+subtitle row every other provider uses.
    calculator: bool,
}

impl ResultRow {
    fn new(
        id: IndexPath,
        title: String,
        subtitle: Option<String>,
        calculation_detail: Option<CalculationDetail>,
        icon: Option<Arc<RenderImage>>,
        selected: bool,
        calculator: bool,
    ) -> Self {
        Self {
            base: ListItem::new(id).selected(selected),
            title: title.into(),
            subtitle: subtitle.map(Into::into),
            calculation_detail,
            icon,
            selected,
            calculator,
        }
    }
}

impl Selectable for ResultRow {
    fn selected(mut self, selected: bool) -> Self {
        self.base = self.base.selected(selected);
        self.selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl gpui::RenderOnce for ResultRow {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        // `ListItem` wraps whatever we give it in a plain (column-stacking)
        // `div`, so icon + name must be a single flex-row child, not two
        // separate children — otherwise they stack on top of each other.
        //
        // `rounded` here (rather than on the list itself) is what makes the
        // hover/selected fill read as an inset card: `ListItem` refines its
        // own style with ours before painting the highlight, so the radius
        // applies to that fill too. The list's own `.p_2()` provides the
        // inset on all four sides uniformly.
        let ResultRow {
            base,
            title,
            subtitle,
            calculation_detail,
            icon,
            selected: _,
            calculator,
        } = self;
        let content: AnyElement = if calculator {
            ResultRow::render_calculator(title, subtitle, calculation_detail, cx)
        } else {
            ResultRow::render_plain(title, subtitle, icon, cx)
        };
        if calculator {
            // Calculator results are presentation cards rather than hoverable list rows.
            // Disabling the wrapper suppresses ListItem's built-in hover fill; the card
            // supplies all of its own text colors, so the disabled text style cannot leak in.
            base.selected(false)
                .disabled(true)
                .px_0()
                .py_1()
                .child(content)
        } else {
            base.py_1p5().rounded(cx.theme().radius).child(content)
        }
    }
}

impl ResultRow {
    /// The plain icon + title + subtitle row every non-calculator provider
    /// uses.
    fn render_plain(
        title: SharedString,
        subtitle: Option<SharedString>,
        icon: Option<Arc<RenderImage>>,
        cx: &mut App,
    ) -> AnyElement {
        h_flex()
            .items_center()
            .gap_3()
            .w_full()
            .child(
                div()
                    .w(px(22.0))
                    .h(px(22.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .when_some(icon, |slot, image| slot.child(img(image).size(px(22.0)))),
            )
            .child(div().flex_1().flex().flex_col().child(title).when_some(
                subtitle,
                |col, subtitle| {
                    col.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(subtitle),
                    )
                },
            ))
            .into_any_element()
    }

    /// A featured calculator card: small "Calculator" label + the
    /// question in muted text on the left, the answer large and bold on the
    /// right — visually distinct from a normal search result row.
    fn render_calculator(
        title: SharedString,
        subtitle: Option<SharedString>,
        calculation_detail: Option<CalculationDetail>,
        cx: &mut App,
    ) -> AnyElement {
        let conversion_color = cx.theme().muted_foreground;
        let title = SharedString::from(format_expression_for_display(&title));
        let source_label = calculation_detail
            .as_ref()
            .map(|detail| SharedString::from(detail.source_label.clone()));
        let target_label = calculation_detail.map(|detail| SharedString::from(detail.target_label));
        let zone_chip = |label: SharedString, cx: &App| {
            div()
                .mt_1()
                .px_2()
                .py_0p5()
                .rounded(cx.theme().radius)
                .border_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().tokens.secondary)
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(label)
        };

        div()
            .flex()
            .flex_col()
            .gap_1()
            .w_full()
            .child(
                div()
                    .px_1()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(cx.theme().muted_foreground)
                    .child("Calculator"),
            )
            .child(
                h_flex()
                    .items_center()
                    .w_full()
                    .min_h(px(96.0))
                    .rounded(cx.theme().radius)
                    .bg(cx.theme().list_active)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .px_2()
                            .when_some(subtitle, |col, expression| {
                                let expression = format_expression_for_display(&expression);
                                let mut search_start = 0;
                                let highlights = expression
                                    .split_whitespace()
                                    .filter_map(|word| {
                                        let offset =
                                            expression[search_start..].find(word)? + search_start;
                                        search_start = offset + word.len();
                                        let connector = word
                                            .trim_matches(|character: char| {
                                                !character.is_alphabetic()
                                            })
                                            .to_ascii_lowercase();

                                        matches!(
                                            connector.as_str(),
                                            "to" | "in"
                                                | "as"
                                                | "from"
                                                | "into"
                                                | "after"
                                                | "before"
                                        )
                                        .then(|| {
                                            (
                                                offset..offset + word.len(),
                                                HighlightStyle::color(conversion_color),
                                            )
                                        })
                                    })
                                    .collect::<Vec<_>>();

                                col.child(
                                    div()
                                        .text_xl()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(cx.theme().foreground)
                                        .child(
                                            StyledText::new(expression).with_highlights(highlights),
                                        ),
                                )
                            })
                            .when_some(source_label, |col, label| col.child(zone_chip(label, cx))),
                    )
                    .child(
                        div()
                            .w(px(44.0))
                            .h(px(70.0))
                            .flex_none()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .child(div().w(px(1.0)).flex_1().bg(conversion_color))
                            .child(
                                div()
                                    .px_1()
                                    .text_lg()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(conversion_color)
                                    .child("→"),
                            )
                            .child(div().w(px(1.0)).flex_1().bg(conversion_color)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .px_2()
                            .child(
                                div()
                                    .text_xl()
                                    .text_center()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(cx.theme().foreground)
                                    .child(title),
                            )
                            .when_some(target_label, |col, label| col.child(zone_chip(label, cx))),
                    ),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::format_expression_for_display;

    #[test]
    fn displays_numeric_exponents_as_superscripts() {
        assert_eq!(format_expression_for_display("43560 ft^2"), "43560 ft²");
        assert_eq!(format_expression_for_display("m^-12"), "m⁻¹²");
        assert_eq!(format_expression_for_display("2^x"), "2^x");
    }
}
