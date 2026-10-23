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

const LAUNCHER_WIDTH: f32 = 720.0;
const LAUNCHER_HEIGHT: f32 = 400.0;

const SCRY_SETUP_ICON_SIZE: f32 = 56.0;
const HAYAI_ROW_ICON_SIZE: f32 = 22.0;

fn load_builtin_icon(bytes: &[u8]) -> Option<Arc<RenderImage>> {
    let mut buffer = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .ok()?
        .into_rgba8();
    // PNG decodes as straight RGBA; GPUI consumes straight BGRA.
    for pixel in buffer.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])))
}

fn builtin_row_icon(icon: &Option<Arc<RenderImage>>, pixels: u32) -> Option<Arc<RenderImage>> {
    let icon = icon.as_ref()?;
    let dimensions = icon.size(0);
    prepare_row_icon(
        native::IconImage {
            width: dimensions.width.0 as u32,
            height: dimensions.height.0 as u32,
            bgra: icon.as_bytes(0)?.to_vec(),
        },
        pixels,
    )
}

fn format_expression_for_display(expression: &str) -> String {
    const SUPERSCRIPT_DIGITS: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];

    let mut superscripted = String::with_capacity(expression.len());
    let mut characters = expression.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '^' {
            superscripted.push(character);
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
            superscripted.push('^');
            if exponent == "⁻" {
                superscripted.push('-');
            }
        } else {
            superscripted.push_str(&exponent);
        }
    }

    let mut formatted = String::with_capacity(superscripted.len());
    let mut characters = superscripted.chars().peekable();
    while let Some(character) = characters.next() {
        if !matches!(character, '+' | '-' | '*' | '/' | '%' | '=') {
            formatted.push(character);
            continue;
        }

        let previous = formatted
            .chars()
            .rev()
            .find(|character| !character.is_whitespace());
        let next = characters
            .clone()
            .find(|character| !character.is_whitespace());
        let is_binary = previous.is_some_and(|previous| {
            !matches!(previous, '+' | '-' | '*' | '/' | '%' | '=' | '(' | '^')
        }) && next.is_some();

        if is_binary {
            while formatted.ends_with(char::is_whitespace) {
                formatted.pop();
            }
            formatted.push(' ');
            formatted.push(character);
            formatted.push(' ');
            while characters.peek().is_some_and(|next| next.is_whitespace()) {
                characters.next();
            }
        } else {
            formatted.push(character);
        }
    }
    let tokens = formatted.split_whitespace().collect::<Vec<_>>();
    tokens
        .iter()
        .enumerate()
        .map(|(index, token)| {
            let bare_token = token.trim_end_matches([',', '.']);
            let normalized_token = bare_token.to_ascii_lowercase();
            let punctuation = &token[bare_token.len()..];
            let follows_clock = index > 0 && tokens[index - 1].contains(':');
            let precedes_timezone = tokens.get(index + 1).is_some_and(|next| {
                matches!(
                    next.to_ascii_lowercase().as_str(),
                    "utc"
                        | "gmt"
                        | "est"
                        | "edt"
                        | "cst"
                        | "cdt"
                        | "mst"
                        | "mdt"
                        | "pst"
                        | "pdt"
                        | "ist"
                        | "jst"
                        | "kst"
                )
            });
            if (follows_clock || precedes_timezone)
                && matches!(normalized_token.as_str(), "am" | "pm")
            {
                return format!("{}{punctuation}", normalized_token.to_ascii_uppercase());
            }
            if let Some((clock, meridiem)) = normalized_token
                .strip_suffix("am")
                .map(|clock| (clock, "AM"))
                .or_else(|| {
                    normalized_token
                        .strip_suffix("pm")
                        .map(|clock| (clock, "PM"))
                })
                && !clock.is_empty()
                && clock
                    .chars()
                    .all(|character| character.is_ascii_digit() || character == ':')
            {
                return format!("{clock} {meridiem}{punctuation}");
            }
            if matches!(
                normalized_token.as_str(),
                "utc"
                    | "gmt"
                    | "bst"
                    | "cet"
                    | "cest"
                    | "eet"
                    | "msk"
                    | "est"
                    | "edt"
                    | "cst"
                    | "cdt"
                    | "mst"
                    | "mdt"
                    | "pst"
                    | "pdt"
                    | "gst"
                    | "ist"
                    | "sgt"
                    | "jst"
                    | "kst"
                    | "aest"
                    | "aedt"
                    | "nzst"
            ) {
                return format!("{}{punctuation}", normalized_token.to_ascii_uppercase());
            }

            let amount = index
                .checked_sub(1)
                .and_then(|previous| tokens[previous].parse::<f64>().ok());
            let canonical = match normalized_token.as_str() {
                "year" | "years" | "yr" | "yrs" => Some(("year", "years")),
                "month" | "months" | "mo" => Some(("month", "months")),
                "week" | "weeks" | "wk" | "wks" => Some(("week", "weeks")),
                "day" | "days" | "d" => Some(("day", "days")),
                "hour" | "hours" | "hr" | "hrs" | "h" => Some(("hour", "hours")),
                "minute" | "minutes" | "min" | "mins" => Some(("minute", "minutes")),
                "second" | "seconds" | "sec" | "secs" | "s" => Some(("second", "seconds")),
                _ => None,
            };
            if let (Some(amount), Some((one, many))) = (amount, canonical) {
                let singular = (amount.abs() - 1.0).abs() < f64::EPSILON;
                return format!("{}{punctuation}", if singular { one } else { many });
            }

            let adjacent_to_conversion = index
                .checked_sub(1)
                .and_then(|previous| tokens.get(previous))
                .is_some_and(|token| {
                    matches!(token.to_ascii_lowercase().as_str(), "to" | "in" | "as")
                        || token.parse::<f64>().is_ok()
                })
                || tokens.get(index + 1).is_some_and(|token| {
                    matches!(token.to_ascii_lowercase().as_str(), "to" | "in" | "as")
                });
            if normalized_token.len() == 3
                && normalized_token
                    .chars()
                    .all(|character| character.is_ascii_alphabetic())
                && adjacent_to_conversion
            {
                return format!("{}{punctuation}", normalized_token.to_ascii_uppercase());
            }

            (*token).to_string()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn format_result_for_display(result: &str) -> String {
    let mut formatted = format_expression_for_display(result);
    let mut clock_positions = [" AM", " PM"]
        .into_iter()
        .flat_map(|suffix| formatted.match_indices(suffix).map(|(index, _)| index))
        .filter_map(|meridiem_space| {
            let clock = formatted[..meridiem_space].split_whitespace().next_back()?;
            let (hour, minute) = clock.split_once(':')?;
            (hour.parse::<u8>().is_ok() && minute.parse::<u8>().is_ok()).then(|| {
                let clock_start = formatted[..meridiem_space].rfind(clock).unwrap_or(0);
                (meridiem_space, clock_start)
            })
        })
        .collect::<Vec<_>>();
    clock_positions.sort_unstable_by_key(|position| std::cmp::Reverse(position.0));

    for (meridiem_space, clock_start) in clock_positions {
        if clock_start > 0 && formatted[..clock_start].trim_end().ends_with(',') {
            formatted.replace_range(clock_start - 1..clock_start, "\n");
        }
        formatted.replace_range(meridiem_space..meridiem_space + 1, "\u{00a0}");
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
    /// Retained to reset transient UI state after hiding the reused window.
    view: Option<Entity<LauncherRoot>>,
    visible: bool,
    reset_pending: bool,
}

impl LauncherState {
    pub fn refresh_update(&self, cx: &mut App) {
        if let Some(view) = &self.view {
            // Notification arrival must not reset the query or steal selection.
            view.update(cx, |_, cx| cx.notify());
        }
    }
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
        self.show_with_focus(false, cx);
    }

    fn show_with_focus(&mut self, after_approval: bool, cx: &mut App) {
        let reset_view = self.reset_pending.then(|| self.view.clone()).flatten();
        self.reset_pending = false;
        if let (Some(handle), Some(search_input)) = (self.window, self.search_input.clone()) {
            let shown = handle
                .update(cx, |_, window, cx| {
                    // Covers a hide/reopen pair drained in the same native poll
                    // before the deferred hidden reset has had a chance to run.
                    if let Some(view) = reset_view {
                        view.update(cx, |view, cx| view.reset_to_apps(window, cx));
                    }
                    window.refresh();
                    // Reopening must be immediate. Never gate SW_SHOW on a
                    // timer, frame callback, or compositor cloak; hidden-state
                    // cleanup belongs to reset_after_hide, off the show path.
                    // Win32 focus first (this re-shows the hidden window via SW_SHOW and
                    // calls SetForegroundWindow etc.), then GPUI focus. Reversed order
                    // (GPUI focus before Win32 focus) would have GPUI's WM_SETFOCUS
                    // handler clobber our focus state.
                    if after_approval {
                        native::focus_launcher_after_approval(window);
                    } else {
                        native::focus_launcher_window(window);
                    }
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
                        perform_search_and_select_first(list, "", window, cx);
                    });
                    let search_input = cx.new(|cx| {
                        InputState::new(window, cx).placeholder("Search apps or calculate...")
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
                        cx.observe_window_activation(window, |view, window, cx| {
                            if window.is_window_active() {
                                return;
                            }
                            view.action_menu = None;
                            cx.notify();
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
        self.reset_pending = true;
        let view = self.view.clone();
        let _ = handle.update(cx, |_, window, cx| {
            native::hide_launcher_window(window);
            Self::reset_after_hide(view, window, cx);
        });
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
        let state = cx.global::<LauncherGlobal>().clone_handle();
        let view = {
            let mut state = state.borrow_mut();
            state.visible = false;
            state.reset_pending = true;
            state.view.clone()
        };
        native::hide_launcher_window(window);
        Self::reset_after_hide(view, window, cx);
    }

    fn reset_after_hide(view: Option<Entity<LauncherRoot>>, window: &mut Window, cx: &mut App) {
        let Some(view) = view else { return };
        // Dismiss can run inside this view's or its list's update. Defer until
        // those borrows unwind; never re-enter the same entity/window update.
        // No timer is needed: SW_HIDE has already removed the old frame.
        window.defer(cx, move |window, cx| {
            {
                let state = cx.global::<LauncherGlobal>().clone_handle();
                let mut state = state.borrow_mut();
                if state.visible || !state.reset_pending {
                    return;
                }
                state.reset_pending = false;
            }
            view.update(cx, |view, cx| view.reset_to_apps(window, cx));
            window.refresh();
            native::render_hidden_launcher_frame(window);
        });
    }
}

/// Fixed-height keycaps keep shortcut labels centered independently of the
/// input widget's text sizing. Each physical key gets its own cap.
fn shortcut_keycap(content: impl IntoElement, _cx: &App) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_center()
        .text_color(gpui::rgb(0xd4d4d8))
        .bg(gpui::rgb(0x3b3b40))
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
    diagnostics_only: bool,
    notification: bool,
    notification_action: &'static str,
    submenu: Option<OpenWithMenu>,
    path: PathBuf,
    is_dir: bool,
    is_app: bool,
    can_run_as_admin: bool,
    selected: usize,
}

#[derive(Clone)]
struct OpenWithApp {
    id: String,
    name: SharedString,
    icon: Option<Arc<RenderImage>>,
}

#[derive(Clone)]
struct OpenWithMenu {
    request: u64,
    loading: bool,
    apps: Arc<[OpenWithApp]>,
}

#[derive(Clone, Copy)]
enum MenuAction {
    CopyDiagnosticLog,
    DismissNotification,
    Open,
    OpenWith,
    AssociatedApp(usize),
    ChooseApp,
    Reveal,
    CopyPath,
    RunAsAdmin,
}

impl ActionMenu {
    fn return_to_parent(&mut self) -> bool {
        if self.submenu.take().is_none() {
            return false;
        }
        self.selected = 0;
        true
    }
    fn count(&self) -> usize {
        match &self.submenu {
            Some(submenu) if submenu.loading => self.actions().count(),
            Some(submenu) => submenu.apps.len() + 1,
            None => self.actions().count(),
        }
    }

    fn row(&self, index: usize) -> Option<(MenuAction, SharedString, Option<Arc<RenderImage>>)> {
        if let Some(submenu) = &self.submenu
            && !submenu.loading
        {
            if let Some(app) = submenu.apps.get(index) {
                return Some((
                    MenuAction::AssociatedApp(index),
                    app.name.clone(),
                    app.icon.clone(),
                ));
            }
            return (index == submenu.apps.len())
                .then(|| (MenuAction::ChooseApp, "Choose another app…".into(), None));
        }
        self.actions()
            .nth(index)
            .map(|(action, label)| (action, label.into(), None))
    }
    /// One source of truth for visible rows, number shortcuts, and navigation.
    /// Open-with and elevation are mutually exclusive, keeping this to four rows.
    fn actions(&self) -> impl Iterator<Item = (MenuAction, &'static str)> + '_ {
        let local_path = self.path.is_absolute();
        let open_with = local_path
            && !self.is_app
            && !self.is_dir
            && !self.can_run_as_admin
            && !self.path.extension().is_some_and(|ext| {
                ext.eq_ignore_ascii_case("lnk") || ext.eq_ignore_ascii_case("url")
            });
        [
            (!self.diagnostics_only).then_some((
                MenuAction::Open,
                if self.notification {
                    self.notification_action
                } else if self.is_app {
                    "Open application"
                } else if self.is_dir {
                    "Open folder"
                } else {
                    "Open"
                },
            )),
            (self.notification && !self.diagnostics_only)
                .then_some((MenuAction::DismissNotification, "Dismiss")),
            (self.notification || self.diagnostics_only)
                .then_some((MenuAction::CopyDiagnosticLog, "Copy diagnostic log")),
            open_with.then_some((MenuAction::OpenWith, "Open with…")),
            local_path.then_some((
                MenuAction::Reveal,
                if self.is_dir {
                    "Open parent folder"
                } else {
                    "Show in Explorer"
                },
            )),
            local_path.then_some((MenuAction::CopyPath, "Copy path")),
            self.can_run_as_admin
                .then_some((MenuAction::RunAsAdmin, "Run as administrator")),
        ]
        .into_iter()
        .flatten()
    }
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

struct WheelAnimation {
    motion: crate::scroll::SmoothScroll,
    last_frame: std::time::Instant,
    results_generation: u64,
}

struct LauncherRoot {
    wheel_animation: Option<WheelAnimation>,
    wheel_animation_id: u64,
    menu_request: u64,
    list: Entity<ListState<ResultListDelegate>>,
    search_input: Entity<InputState>,
    /// The leading command space is state, not text, so the caret stays at
    /// the start while the placeholder explains which command to enter.
    waiting_for_command: bool,
    /// File mode reconstructs its provider prefix outside the visible input.
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
    fn cancel_wheel_animation(&mut self) {
        self.wheel_animation = None;
        self.wheel_animation_id = self.wheel_animation_id.wrapping_add(1);
    }

    fn on_result_wheel(
        &mut self,
        event: &gpui::ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.action_menu.is_some() {
            self.cancel_wheel_animation();
            return;
        }
        // Precision devices already supply continuous pixel motion and inertia.
        // Let the virtual list consume those events without a second filter.
        if event.delta.precise() {
            self.cancel_wheel_animation();
            return;
        }
        let delta = event.delta.pixel_delta(window.line_height());
        if delta.y == px(0.0) || delta.x.abs() > delta.y.abs() {
            return;
        }
        let list = self.list.read(cx);
        let handle = list.scroll_handle();
        let current = f32::from(handle.offset().y);
        let max_offset = f32::from(handle.max_offset().y);
        let generation = list.delegate().results_generation;
        if self.wheel_animation.as_ref().is_some_and(|animation| {
            animation.results_generation != generation
                || (animation.motion.position - current).abs() > 0.5
        }) {
            self.cancel_wheel_animation();
        }
        let start = self.wheel_animation.is_none();
        let animation = self.wheel_animation.get_or_insert_with(|| WheelAnimation {
            motion: crate::scroll::SmoothScroll::new(current),
            last_frame: std::time::Instant::now(),
            results_generation: generation,
        });
        animation.motion.push(f32::from(delta.y), max_offset);
        // Capture before the child's default bubble-phase scroll handler, or
        // every wheel tick would also jump the native scroll offset immediately.
        cx.stop_propagation();
        if start {
            self.schedule_wheel_frame(window, cx);
        }
    }

    fn schedule_wheel_frame(&self, window: &mut Window, cx: &mut Context<Self>) {
        let root = cx.entity().downgrade();
        let id = self.wheel_animation_id;
        // GPUI invokes this through its display-frame loop; no 60Hz timer.
        window.on_next_frame(move |window, cx| {
            let _ = root.update(cx, |root, cx| {
                if root.wheel_animation_id == id {
                    root.advance_wheel_frame(window, cx);
                }
            });
        });
        window.request_animation_frame();
    }

    fn advance_wheel_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let visible = cx
            .global::<LauncherGlobal>()
            .clone_handle()
            .borrow()
            .visible;
        let list = self.list.read(cx);
        let handle = list.scroll_handle().clone();
        let generation = list.delegate().results_generation;
        let Some(animation) = self.wheel_animation.as_mut() else {
            return;
        };
        if !visible
            || self.action_menu.is_some()
            || animation.results_generation != generation
            || (f32::from(handle.offset().y) - animation.motion.position).abs() > 0.5
        {
            self.cancel_wheel_animation();
            return;
        }
        let now = std::time::Instant::now();
        let active = animation.motion.advance(
            now.duration_since(animation.last_frame).as_secs_f32(),
            f32::from(handle.max_offset().y),
        );
        animation.last_frame = now;
        let mut offset = handle.offset();
        offset.y = px(animation.motion.position);
        handle.set_offset(offset);
        self.list.update(cx, |_, cx| cx.notify());
        cx.notify();
        if active {
            self.schedule_wheel_frame(window, cx);
        } else {
            self.cancel_wheel_animation();
        }
    }

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
            let action_number = if event.keystroke.modifiers == gpui::Modifiers::default() {
                match key {
                    "1" => Some(0),
                    "2" => Some(1),
                    "3" => Some(2),
                    "4" => Some(3),
                    "5" => Some(4),
                    "6" => Some(5),
                    "7" => Some(6),
                    _ => None,
                }
            } else {
                None
            };
            if key != "backspace" && key != "tab" && !control_k && action_number.is_none() {
                return;
            }
            this.update(cx, |this, cx| {
                // Intercept before InputState inserts the digit. Outside the
                // panel, number keys must remain ordinary search text.
                if let Some(index) = action_number
                    && let Some(menu) = this.action_menu.as_mut()
                {
                    cx.stop_propagation();
                    if index < menu.count() {
                        menu.selected = index;
                        if let Some(action) = this.selected_menu_action() {
                            this.run_menu_action(action, window, cx);
                        }
                    }
                    return;
                }
                this.on_intercepted_key(key, control_k, window, cx);
            });
        });
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
            wheel_animation_id: 0,
            menu_request: 0,
            list,
            search_input,
            waiting_for_command: false,
            file_mode: false,
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
        self.file_mode = true;
        self.folder_scope.clear();
        let remainder = raw
            .strip_prefix(" f")
            .unwrap_or("")
            .trim_start()
            .to_string();
        self.search_input.update(cx, |input, cx| {
            input.set_value(&remainder, window, cx);
            input.set_placeholder("Search files...", window, cx);
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
        // The window is reused after hiding; transient overlays must not
        // survive into the next Alt+Space session.
        self.action_menu = None;
        self.history_cursor = None;
        self.waiting_for_command = false;
        self.file_mode = false;
        self.folder_scope.clear();
        self.search_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.set_placeholder("Search apps or calculate...", window, cx);
        });
        self.list.update(cx, |list, cx| {
            perform_search_and_select_first(list, "", window, cx);
            let first = if list.delegate().notification_visible() {
                IndexPath::default()
            } else {
                list.delegate().index_path(0)
            };
            list.scroll_to_item(first, gpui::ScrollStrategy::Top, window, cx);
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
            "Search files..."
        } else {
            "Search this folder..."
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

    fn selected_action_menu(&self, cx: &App) -> Option<ActionMenu> {
        let delegate = self.list.read(cx).delegate();
        if delegate.show_updates || delegate.show_scry_updates || delegate.needs_pinned_scry() {
            return Some(ActionMenu {
                diagnostics_only: true,
                notification: false,
                notification_action: "",
                submenu: None,
                path: PathBuf::new(),
                is_dir: false,
                is_app: false,
                can_run_as_admin: false,
                selected: 0,
            });
        }
        if delegate.calculator_view() {
            return None;
        }
        if delegate.search_pending {
            return None;
        }
        if delegate.notification_selected && delegate.notification_visible() {
            return Some(ActionMenu {
                diagnostics_only: false,
                notification: true,
                notification_action: self.selected_action_label(cx),
                submenu: None,
                path: PathBuf::new(),
                is_dir: false,
                is_app: false,
                can_run_as_admin: false,
                selected: 0,
            });
        }
        delegate
            .selected_index
            .and_then(|ix| delegate.item(ix))
            .and_then(|item| match &item.action {
                CommandAction::OpenFile(path) => Some(ActionMenu {
                    diagnostics_only: false,
                    notification: false,
                    notification_action: "",
                    submenu: None,
                    path: path.clone(),
                    is_dir: path.is_dir(),
                    is_app: false,
                    can_run_as_admin: can_run_as_admin(path),
                    selected: 0,
                }),
                CommandAction::LaunchApplication(path) => Some(ActionMenu {
                    diagnostics_only: false,
                    notification: false,
                    notification_action: "",
                    submenu: None,
                    path: path.clone(),
                    is_dir: false,
                    is_app: true,
                    can_run_as_admin: path.is_absolute()
                        && (can_run_as_admin(path)
                            || path
                                .extension()
                                .is_some_and(|ext| ext.eq_ignore_ascii_case("lnk"))),
                    selected: 0,
                }),
                CommandAction::CheckForUpdates => Some(ActionMenu {
                    diagnostics_only: true,
                    notification: false,
                    notification_action: "",
                    submenu: None,
                    path: PathBuf::new(),
                    is_dir: false,
                    is_app: false,
                    can_run_as_admin: false,
                    selected: 0,
                }),
                CommandAction::CopyToClipboard(_)
                | CommandAction::ShowText(_)
                | CommandAction::EmptyState
                | CommandAction::PendingState
                | CommandAction::InstallFileSearch
                | CommandAction::StartFileSearch
                | CommandAction::UpdateFileSearch
                | CommandAction::CheckFileSearchUpdates
                | CommandAction::AboutHayai
                | CommandAction::RestartToUpdate => None,
            })
    }

    fn selected_menu_action(&self) -> Option<MenuAction> {
        let menu = self.action_menu.as_ref()?;
        menu.row(menu.selected).map(|(action, _, _)| action)
    }

    fn move_menu_selection(&mut self, delta: isize, cx: &mut Context<Self>) -> bool {
        let Some(menu) = self.action_menu.as_mut() else {
            return false;
        };
        let count = menu.count();
        if count == 0 {
            return true;
        }
        menu.selected = (menu.selected as isize + delta).rem_euclid(count as isize) as usize;
        cx.notify();
        true
    }

    fn run_menu_action(&mut self, action: MenuAction, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(action, MenuAction::OpenWith) {
            self.open_with_submenu(window, cx);
            return;
        }
        let Some(menu) = self.action_menu.take() else {
            return;
        };
        match action {
            MenuAction::CopyDiagnosticLog => {
                let delegate = self.list.read(cx).delegate();
                let combined = !delegate.show_updates
                    && !delegate.show_scry_updates
                    && !delegate.notification_selected
                    && delegate
                        .selected_index
                        .and_then(|ix| delegate.item(ix))
                        .is_some_and(|item| matches!(item.action, CommandAction::CheckForUpdates));
                let scry = delegate.show_scry_updates
                    || delegate.selected_notification() == Some(UpdateNotification::Scry);
                let log = if combined {
                    format!(
                        "=== Hayai ===\n{}\n=== Scry Search ===\n{}",
                        crate::updates::DIAGNOSTICS.snapshot(),
                        crate::scry_updates::diagnostic_snapshot()
                    )
                } else if scry {
                    crate::scry_updates::diagnostic_snapshot()
                } else {
                    crate::updates::DIAGNOSTICS.snapshot()
                };
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(log));
            }
            MenuAction::DismissNotification => {
                self.list.update(cx, |list, cx| {
                    if list.delegate().selected_notification() == Some(UpdateNotification::Scry) {
                        list.delegate_mut().scry_update_dismissed = true;
                    } else {
                        list.delegate_mut().update_dismissed = true;
                    }
                    list.delegate_mut().notification_selected = false;
                    let ix = (!list.delegate().results.is_empty())
                        .then(|| list.delegate().index_path(0));
                    let measured = list.delegate().index_path(0);
                    list.set_item_to_measure_index(measured, window, cx);
                    list.set_selected_index(ix, window, cx);
                    cx.notify();
                });
            }
            MenuAction::Open => self.list.update(cx, |list, cx| {
                list.delegate_mut().confirm(false, window, cx);
            }),
            MenuAction::ChooseApp => {
                LauncherState::dismiss(window, cx);
                cx.background_spawn(async move {
                    native::open_with(&menu.path);
                })
                .detach();
            }
            MenuAction::AssociatedApp(index) => {
                if let Some(app) = menu
                    .submenu
                    .and_then(|submenu| submenu.apps.get(index).cloned())
                {
                    LauncherState::dismiss(window, cx);
                    cx.background_spawn(async move {
                        native::open_with_app(&menu.path, &app.id);
                    })
                    .detach();
                }
            }
            MenuAction::OpenWith => unreachable!(),
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

    fn open_with_submenu(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = self.action_menu.as_mut() else {
            return;
        };
        if menu.submenu.as_ref().is_some_and(|submenu| submenu.loading) {
            return;
        }
        self.menu_request = self.menu_request.wrapping_add(1);
        let request = self.menu_request;
        let path = menu.path.clone();
        menu.submenu = Some(OpenWithMenu {
            request,
            loading: true,
            apps: Arc::from([]),
        });
        cx.notify();
        cx.spawn(async move |this, cx| {
            let apps = cx
                .background_spawn(async move {
                    let apps = native::associated_apps(&path)
                        .into_iter()
                        .map(|app| OpenWithApp {
                            id: app.id,
                            name: app.name.into(),
                            icon: app.icon.and_then(|icon| {
                                let pixels =
                                    image::RgbaImage::from_raw(icon.width, icon.height, icon.bgra)?;
                                Some(Arc::new(RenderImage::new(vec![image::Frame::new(pixels)])))
                            }),
                        })
                        .collect::<Arc<[OpenWithApp]>>();
                    (path, apps)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Some(menu) = this.action_menu.as_mut()
                    && menu.path == apps.0
                    && let Some(submenu) = menu.submenu.as_mut()
                    && submenu.request == request
                {
                    submenu.apps = apps.1;
                    submenu.loading = false;
                    menu.selected = 0;
                    cx.notify();
                }
            });
        })
        .detach();
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
                self.cancel_wheel_animation();
                self.action_menu = None;
                // Only real user edits reach this arm — our own history-recall
                // `set_value` calls below drive `perform_search_and_select_first`
                // directly and don't emit `Change`, so this can't clobber a
                // recall in progress.
                self.history_cursor = None;
                let raw = input.read(cx).value().to_string();
                if !self.file_mode && !self.waiting_for_command && raw == " " {
                    self.waiting_for_command = true;
                    // set_value does not emit Change. Remove the trigger before
                    // paint while retaining command routing in our own state.
                    input.update(cx, |input, cx| {
                        input.set_value("", window, cx);
                        input.set_placeholder("Type a command, e.g. f for files", window, cx);
                    });
                    self.action_menu = None;
                    cx.notify();
                    return;
                }
                if self.waiting_for_command && (raw == "f" || raw.starts_with("f ")) {
                    self.enter_file_mode(&format!(" {raw}"), window, cx);
                    return;
                }
                if !self.file_mode && (raw == " f" || raw.starts_with(" f ")) {
                    self.enter_file_mode(&raw, window, cx);
                    return;
                }
                let query = if self.file_mode {
                    self.build_query(&raw)
                } else if self.waiting_for_command {
                    format!(" {raw}")
                } else {
                    raw
                };
                self.list.update(cx, |list, cx| {
                    perform_search_and_select_first(list, &query, window, cx);
                });
            }
            InputEvent::PressEnter { .. } => {
                if self.list.read(cx).delegate().empty_state.is_some() {
                    return;
                }
                if self.list.read(cx).delegate().show_about {
                    self.list.update(cx, |list, cx| {
                        list.delegate_mut().show_about = false;
                        cx.notify();
                    });
                    return;
                }
                if self.action_menu.is_some() {
                    if let Some(action) = self.selected_menu_action() {
                        self.run_menu_action(action, window, cx);
                    }
                    return;
                }
                let text = input.read(cx).value().to_string();
                if self.waiting_for_command && text.is_empty() {
                    return;
                }
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
        if self.waiting_for_command
            && key == "backspace"
            && self.search_input.read(cx).value().is_empty()
        {
            cx.stop_propagation();
            self.reset_to_apps(window, cx);
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
                .get(self.list.read(cx).delegate().flat_index(ix))
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
        self.refresh_folder_scope(window, cx);
    }

    fn move_selection(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        if self.action_menu.take().is_some() {
            cx.notify();
        }
        self.list.update(cx, |list, cx| {
            let count = list.delegate().results.len();
            if list.delegate().notification_visible() {
                let notices = list.delegate().notification_count();
                let current = if list.delegate().notification_selected {
                    list.selected_index().map_or(0, |ix| ix.row)
                } else {
                    list.selected_index()
                        .map(|ix| list.delegate().flat_index(ix) + notices)
                        .unwrap_or(0)
                };
                let next =
                    (current as isize + delta).rem_euclid((count + notices) as isize) as usize;
                list.set_selected_index(
                    Some(
                        next.checked_sub(notices)
                            .map(|flat| list.delegate().index_path(flat))
                            .unwrap_or(IndexPath {
                                section: 0,
                                row: next,
                                ..Default::default()
                            }),
                    ),
                    window,
                    cx,
                );
                list.delegate_mut().notification_selected = next < notices;
                list.scroll_to_selected_item(window, cx);
                cx.notify();
                return;
            }
            if count == 0 {
                return;
            }
            let current = list
                .selected_index()
                .map(|ix| list.delegate().flat_index(ix) as isize)
                .unwrap_or(-1);
            let next = (current + delta).rem_euclid(count as isize) as usize;
            let ix = list.delegate().index_path(next);
            list.set_selected_index(Some(ix), window, cx);
            list.scroll_to_selected_item(window, cx);
        });
    }

    fn on_move_up(&mut self, _: &MoveUp, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_wheel_animation();
        if self.list.read(cx).delegate().show_about
            || self.list.read(cx).delegate().show_updates
            || self.list.read(cx).delegate().show_scry_updates
        {
            return;
        }
        if self.move_menu_selection(-1, cx) {
            return;
        }
        if self.list.read(cx).delegate().notification_visible() {
            if self.list.read(cx).delegate().notification_selected
                && self
                    .list
                    .read(cx)
                    .selected_index()
                    .is_some_and(|ix| ix.row == 0)
                && self.search_input.read(cx).value().is_empty()
            {
                self.recall_history(1, window, cx);
                return;
            }
            self.move_selection(-1, window, cx);
            return;
        }
        let at_top = self
            .list
            .read(cx)
            .selected_index()
            .map(|ix| self.list.read(cx).delegate().flat_index(ix))
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
        self.cancel_wheel_animation();
        if self.list.read(cx).delegate().show_about
            || self.list.read(cx).delegate().show_updates
            || self.list.read(cx).delegate().show_scry_updates
        {
            return;
        }
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
        } else if self.waiting_for_command {
            format!(" {text}")
        } else {
            text
        };
        self.list.update(cx, |list, cx| {
            perform_search_and_select_first(list, &query, window, cx);
        });
        true
    }

    fn on_escape(&mut self, _: &Escape, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(menu) = self.action_menu.as_mut()
            && menu.return_to_parent()
        {
            cx.notify();
            return;
        }
        if self.action_menu.take().is_some() {
            cx.notify();
            return;
        }
        if self.list.read(cx).delegate().show_about
            || self.list.read(cx).delegate().show_updates
            || self.list.read(cx).delegate().show_scry_updates
        {
            self.list.update(cx, |list, cx| {
                list.delegate_mut().show_updates = false;
                list.delegate_mut().show_scry_updates = false;
                list.delegate_mut().show_about = false;
                cx.notify();
            });
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
        if self.file_mode || self.waiting_for_command {
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
        // The visible page owns the footer even while typing starts a
        // background search. Pending results must not replace its action.
        if delegate.show_scry_updates || delegate.needs_pinned_scry() {
            return if delegate.scry_installing {
                "Updating…"
            } else if delegate.scry_checking && !delegate.needs_pinned_scry() {
                "Checking…"
            } else if delegate.scry_ready {
                "Back to search"
            } else if delegate.scry_target().is_some() {
                "Update Scry Search"
            } else {
                "Check again"
            };
        }
        if delegate.show_updates {
            return if delegate.restarting_update {
                "Restarting…"
            } else if delegate.checking_update {
                "Checking…"
            } else if delegate.update_result == Some(crate::updates::CheckResult::Ready) {
                "Restart to update"
            } else {
                "Check again"
            };
        }
        if delegate.show_about {
            return "Back";
        }
        if delegate.file_search && delegate.search_pending {
            return "Search files";
        }
        if delegate.notification_selected {
            if delegate.selected_notification() == Some(UpdateNotification::Scry) {
                return if delegate.scry_installing {
                    "Updating…"
                } else if delegate.scry_install_attempted && delegate.scry_ready {
                    "Dismiss"
                } else if delegate.scry_install_attempted {
                    "Retry update"
                } else if delegate.scry_checking {
                    "Checking…"
                } else if delegate
                    .scry_discovery
                    .as_ref()
                    .is_some_and(|r| r.state == crate::scry_updates::State::Available)
                {
                    "View update"
                } else {
                    "Check again"
                };
            }
            return if delegate.checking_update {
                "Checking…"
            } else if delegate.restarting_update {
                "Restarting…"
            } else {
                if crate::updates::ready_version().is_some() {
                    "View update"
                } else {
                    "Check again"
                }
            };
        }
        if delegate.starting_scry {
            return "Starting…";
        }
        if delegate.file_search && delegate.results.is_empty() {
            return "Search files";
        }
        if let Some(item) = delegate.results.first()
            && item.id.starts_with("files:compatibility:")
        {
            return if item.id.ends_with(":error") {
                "Unavailable"
            } else if item.id.ends_with(":unavailable") {
                "Check for updates"
            } else if item.id.ends_with(":newer") {
                "Update Hayai"
            } else {
                "Update required"
            };
        }
        let action = delegate
            .selected_index
            .and_then(|ix| delegate.item(ix))
            .map(|item| &item.action);
        match action {
            Some(CommandAction::CopyToClipboard(_)) => "Copy",
            Some(CommandAction::OpenFile(path)) if path.is_dir() => "Open Folder",
            Some(CommandAction::OpenFile(_)) => "Open File",
            Some(CommandAction::ShowText(_)) => "Show",
            Some(CommandAction::EmptyState | CommandAction::PendingState) => "",
            Some(CommandAction::InstallFileSearch) => "Install",
            Some(CommandAction::StartFileSearch) => "Start",
            Some(CommandAction::UpdateFileSearch) => "Update Scry Search",
            Some(CommandAction::CheckFileSearchUpdates) => "Check for updates",
            Some(CommandAction::RestartToUpdate) => "Restart to update",
            Some(CommandAction::CheckForUpdates) => "Check for updates",
            Some(CommandAction::AboutHayai) => "About Hayai",
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
        let delegate = self.list.read(cx).delegate();
        let about = delegate.show_about;
        let empty_notice = delegate.empty_state.is_some();
        // Compatibility results and explicit update navigation share one page.
        // A query refresh must not briefly expose the older setup renderer.
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
        let updates_page = delegate.show_updates || scry_page;
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
        let required_message = delegate.needs_pinned_scry().then(|| {
            format!(
                "Scry Search {} is ready to install.",
                crate::scry_updates::REQUIRED_VERSION
            )
        });
        let update_message = if scry_page {
            if delegate.needs_pinned_scry() && !delegate.scry_install_attempted {
                required_message.as_deref().unwrap_or_default()
            } else {
                delegate.scry_message.as_str()
            }
        } else {
            delegate
                .update_message
                .unwrap_or("Check for a new Hayai release.")
        };
        let update_icon = if scry_page {
            delegate.scry_icon.clone()
        } else {
            delegate.update_icon.clone()
        };
        let scry_installing = delegate.scry_installing;
        let scry_ready = delegate.scry_ready;
        let scry_status_label = if delegate.scry_checking {
            "Checking Scry Search…"
        } else {
            match delegate.scry_discovery.as_ref().map(|result| &result.state) {
                Some(crate::scry_updates::State::Available) => "Scry Search update available",
                Some(crate::scry_updates::State::Current) => "Scry Search is up to date",
                Some(crate::scry_updates::State::NoCompatibleRelease) => {
                    "No compatible Scry Search release found"
                }
                _ => "Couldn't check Scry Search for updates",
            }
        };
        let show_main_heading = about || updates_page;
        let actions_disabled = (delegate.search_pending && !updates_page)
            || about
            || (!updates_page
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
        let brand_icon = delegate.brand_icon.clone();
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
        let setup_screen = file_setup.is_some() || updates_page || about;
        let updating_scry =
            self.list.read(cx).delegate().scry_installing || crate::scry_updates::is_updating();
        let starting_scry = self.list.read(cx).delegate().starting_scry || updating_scry;
        let primary_label = self.selected_action_label(cx);
        let can_enter_folder = self.file_mode && primary_label == "Open Folder";
        // Backspace edits non-empty queries; advertise navigation only when
        // the empty-input handler can actually leave the current scope/mode.
        let can_go_back = (self.file_mode || self.waiting_for_command)
            && self.search_input.read(cx).value().is_empty();
        let show_commands_hint = !setup_screen
            && !self.file_mode
            && !self.waiting_for_command
            && self.search_input.read(cx).value().is_empty();
        let backspace_label = if !self.folder_scope.is_empty() {
            "Up one level"
        } else if self.file_mode {
            "Exit file search"
        } else {
            "Cancel command"
        };
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
            .child(
                div()
                    .flex_none()
                    .h(px(52.0))
                    .w_full()
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        Input::new(&self.search_input)
                            .flex_1()
                            .min_w_0()
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
                    )
                    .when(!self.folder_scope.is_empty(), |header| {
                        let mut breadcrumbs = h_flex()
                            .flex_none()
                            .h(px(32.0))
                            .max_w(px(240.0))
                            .px_3()
                            .gap_1()
                            .overflow_hidden()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(
                                div()
                                    .id("scope-all-files")
                                    .flex_none()
                                    .cursor_pointer()
                                    .hover(|el| el.text_color(cx.theme().foreground))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _, window, cx| {
                                            this.folder_scope.clear();
                                            this.refresh_folder_scope(window, cx);
                                            this.search_input
                                                .update(cx, |input, cx| input.focus(window, cx));
                                        }),
                                    )
                                    .child("All files"),
                            );
                        for (index, folder) in self.folder_scope.iter().enumerate() {
                            let name = folder
                                .file_name()
                                .unwrap_or(folder.as_os_str())
                                .to_string_lossy()
                                .into_owned();
                            let current = index + 1 == self.folder_scope.len();
                            breadcrumbs = breadcrumbs.child("/").child(
                                div()
                                    .id(("scope-folder", index))
                                    .min_w_0()
                                    .truncate()
                                    .cursor_pointer()
                                    .when(current, |el| el.text_color(cx.theme().foreground))
                                    .hover(|el| el.text_color(cx.theme().foreground))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |this, _, window, cx| {
                                            this.folder_scope.truncate(index + 1);
                                            this.refresh_folder_scope(window, cx);
                                            this.search_input
                                                .update(cx, |input, cx| input.focus(window, cx));
                                        }),
                                    )
                                    .child(name),
                            );
                        }
                        header.child(breadcrumbs)
                    }),
            )
            .when(show_main_heading, |root| root.child(div().flex_none().px_2().pt_2().child(section_heading(if scry_page { "Files" } else { "Hayai" }, cx))))
            .child(if updates_page {
                div().flex_1().w_full().flex().flex_col().items_center().justify_center()
                    .gap_3().text_center()
                    .child(div().size(px(56.0)).flex_none().flex().items_center().justify_center()
                        .when(update_loading, |slot| slot.child(
                            gpui_component::spinner::Spinner::new()
                                .icon(gpui_component::Icon::default().path("icons/spinner.svg"))
                                .with_size(px(32.0)).color(cx.theme().muted_foreground)))
                        .when(!update_loading, |slot| slot.when_some(update_icon,
                            |slot, icon| slot.child(img(icon).size(px(56.0))))))
                    .child(div().text_size(px(14.0)).font_weight(FontWeight::MEDIUM)
                        .child(if scry_page {
                            if scry_installing { "Updating Scry Search…" }
                            else if scry_ready { "File search is ready" }
                            else if scry_required { "Scry Search update required" }
                            else if scry_setup { "Set up Scry Search" }
                            else if update_loading { "Checking for updates…" }
                            else if update_ready { "Scry Search update available" }
                            else { "Scry Search updates" }
                        } else if restarting_update { "Restarting to update…" }
                            else if update_loading { "Checking for updates…" }
                            else if update_failed { "Couldn't restart to update" }
                            else if update_ready { "Update ready" } else { "Hayai updates" }))
                    .child(div().max_w(px(440.0)).text_size(px(13.0))
                        .text_color(cx.theme().muted_foreground)
                        .child(if scry_page && scry_required && !scry_ready {
                            format!("File search requires a newer Scry Search version.\n{update_message}")
                        } else if scry_page { update_message.to_string() }
                            else if restarting_update { "Applying the update and restarting Hayai.".to_string() }
                            else if update_loading { "Checking and downloading the latest release.".to_string() }
                            else if update_failed { "Try restarting again.".to_string() }
                            else if update_ready { format!("Hayai {} is ready to install.", crate::updates::ready_version().unwrap_or_default()) }
                            else { update_message.to_string() }))
                    .when(!update_loading, |page| page.child(
                        div().id("update-page-action").mt_1().px_4().py_2().rounded(px(6.0))
                            .bg(cx.theme().list_active).cursor_pointer()
                            .hover(|button| button.bg(cx.theme().tokens.list_hover))
                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                                cx.stop_propagation();
                                this.list.update(cx, |list, cx| {
                                    let delegate = list.delegate_mut();
                                    if delegate.needs_pinned_scry() {
                                        delegate.show_scry_updates = true;
                                    }
                                    delegate.confirm(false, window, cx);
                                    cx.notify();
                                });
                            }))
                            .child(if scry_page {
                                if scry_ready { "Back to search" } else if update_ready { "Update Scry Search" } else { "Check again" }
                            } else if update_ready { "Restart to update" } else { "Check again" })))
                    .when(!scry_page, |page| page.child(
                        div().id("scry-update-status").text_xs().text_color(cx.theme().muted_foreground)
                            .cursor_pointer().underline()
                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                                this.list.update(cx, |list, cx| {
                                    let delegate = list.delegate_mut();
                                    if delegate.scry_discovery.as_ref().is_some_and(|result| result.state == crate::scry_updates::State::Available) && !delegate.scry_checking {
                                        delegate.show_updates = false;
                                        delegate.show_scry_updates = true;
                                    } else { delegate.check_scry_updates(true, window, cx); }
                                    cx.notify();
                                });
                            }))
                            .child(scry_status_label)))
                    .into_any_element()
            } else if about {
                div().flex_1().w_full().flex().flex_col().items_center().justify_center().gap_3()
                    .when_some(brand_icon, |card, icon| card.child(img(icon).size(px(56.0))))
                    .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child("Hayai"))
                    .child(div().text_xs().text_color(cx.theme().muted_foreground)
                        .child(format!("Version {}", env!("CARGO_PKG_VERSION"))))
                    .child("A fast, lightweight launcher for Windows.")
                    .child(div().id("hayai-project-link").text_xs().underline().cursor_pointer()
                        .on_mouse_down(MouseButton::Left, cx.listener(|_, _, window, cx| {
                            LauncherState::dismiss(window, cx);
                            cx.open_url("https://github.com/Ariaryy/hayai");
                        })).child("View project on GitHub"))
                    .into_any_element()
            } else if let Some((title, description, icon)) = file_setup {
                div()
                    .flex_1().w_full().flex().flex_col()
                    .items_center().justify_center().gap_3().text_center()
                    .child(div().size(px(SCRY_SETUP_ICON_SIZE)).flex_none()
                        .flex().items_center().justify_center()
                        .when(starting_scry, |slot| slot.child(
                            gpui_component::spinner::Spinner::new()
                                .icon(gpui_component::Icon::default().path("icons/spinner.svg"))
                                .with_size(px(32.0))
                                .color(cx.theme().muted_foreground)))
                        .when(!starting_scry, |slot| slot.when_some(icon,
                            |slot, icon| slot.child(img(icon).size(px(SCRY_SETUP_ICON_SIZE))))))
                    .child(div().text_size(px(14.0)).font_weight(FontWeight::MEDIUM)
                        .child(if updating_scry { "Updating Scry Search".to_string() }
                            else if starting_scry { "Starting file search".to_string() } else { title }))
                    .child(div().max_w(px(380.0)).text_size(px(13.0))
                        .text_color(cx.theme().muted_foreground)
                        .child(if starting_scry { "Waiting for Scry Search to become ready…".to_string() } else { description }))
                    .child(div().id("file-search-learn-more")
                        .text_xs().underline().cursor_pointer()
                        .text_color(cx.theme().muted_foreground)
                        .hover(|link| link.text_color(cx.theme().foreground))
                        .on_mouse_down(MouseButton::Left, cx.listener(|_, _, window, cx| {
                            cx.stop_propagation();
                            LauncherState::dismiss(window, cx);
                            cx.open_url("https://github.com/Ariaryy/hayai/blob/main/docs/users/file-search.md");
                        }))
                        .child("Learn more"))
                    .into_any_element()
            } else {
                // Clip virtualized rows to the results viewport so scrolling
                // cannot paint text over the search input or footer.
                div().relative().flex_1().min_h_0().w_full().overflow_hidden()
                    .child({
                        let root = cx.entity().downgrade();
                        gpui::canvas(|_, _, _| (), move |bounds, _, window, _| {
                            window.on_mouse_event(move |event: &gpui::ScrollWheelEvent, phase, window, cx| {
                                if phase == gpui::DispatchPhase::Capture && bounds.contains(&event.position) {
                                    let _ = root.update(cx, |root, cx| root.on_result_wheel(event, window, cx));
                                }
                            });
                        }).absolute().size_full()
                    })
                    .child(List::new(&self.list)
                        .scrollbar_visible(false)
                        .with_size(Size::Large)
                        .p_2()
                        .size_full())
                    .into_any_element()
            })
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
                    .child(
                        h_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_3()
                            .when(self.action_menu.is_none() && show_commands_hint, |hints| {
                                hints.child(
                                    h_flex()
                                        .items_center()
                                        .gap_2()
                                        .child("Commands")
                                        .child(shortcut_keycap("Space", cx)),
                                )
                            })
                            .when(self.action_menu.is_none() && can_enter_folder, |hints| {
                                hints.child(
                                    h_flex()
                                        .items_center()
                                        .gap_2()
                                        .child("Enter folder")
                                        .child(shortcut_keycap("Tab", cx)),
                                )
                            })
                            .when(self.action_menu.is_none() && can_go_back, |hints| {
                                hints.child(
                                    h_flex()
                                        .items_center()
                                        .gap_2()
                                        .child(backspace_label)
                                        .child(shortcut_keycap("Backspace", cx)),
                                )
                            }),
                    )
                    .when(!empty_notice, |footer| footer.child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .text_color(cx.theme().foreground)
                            .font_weight(FontWeight::MEDIUM)
                            .child(primary_label)
                            .child(shortcut_keycap(
                                gpui::svg()
                                    .path("icons/enter.svg")
                                    // Svg::paint requires its own style color;
                                    // inheriting the keycap's text color does
                                    // not populate that field on this GPUI rev.
                                    .text_color(gpui::rgb(0xd4d4d8))
                                    .size(px(14.0))
                                    .flex_none(),
                                cx,
                            )),
                    ))
                    .when((!setup_screen || updates_page) && !empty_notice, |footer| { footer
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
                            .when(self.action_menu.is_some(), |group| {
                                group
                                    .bg(gpui::rgb(0x2b2b30))
                                    .text_color(cx.theme().foreground)
                            })
                            .child(div().pl(px(3.0)).child("Actions"))
                            .child(
                                h_flex()
                                    .gap_1()
                                    .child(shortcut_keycap("Ctrl", cx))
                                    .child(shortcut_keycap("K", cx)),
                            ),
                    )
                    }),
            )
            .when_some(self.action_menu.clone(), |root, menu| {
                let mut panel = div()
                    .absolute()
                    // Painting above the list does not block its hitboxes.
                    // Occlude the entire panel, including padding, so pointer
                    // movement and scrolling cannot reach rows underneath.
                    .occlude()
                    .right(px(6.0))
                    .bottom(px(44.0))
                    .w(px(224.0))
                    .p_1()
                    .bg(cx.theme().background)
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
                            .when(menu.submenu.as_ref().is_some_and(|submenu| !submenu.loading), |row| {
                                row.child(
                                    div().size(px(18.0)).flex_none().child(match icon {
                                        Some(icon) => img(icon).size(px(18.0)).into_any_element(),
                                        None => gpui::svg()
                                            .path("icons/application.svg")
                                            .size(px(18.0))
                                            .text_color(cx.theme().muted_foreground)
                                            .into_any_element(),
                                    }),
                                )
                            })
                            .child(div().flex_1().min_w_0().truncate().child(label))
                            .child(shortcut_keycap(shortcut, cx)),
                    );
                }
                root.child(panel)
            })
    }
}

/// Drives the results list: dispatches each query through the
/// [`PluginRegistry`], renders one row per [`CommandItem`], and runs the
/// selected item's action on confirm. Provider-agnostic — apps are just the
/// current default provider.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UpdateNotification {
    Hayai,
    Scry,
}

fn notification_kind(row: usize, hayai: bool, scry: bool) -> Option<UpdateNotification> {
    match (row, hayai, scry) {
        (0, true, _) => Some(UpdateNotification::Hayai),
        (0, false, true) | (1, true, true) => Some(UpdateNotification::Scry),
        _ => None,
    }
}

fn result_index_path(flat: usize, main: usize, total: usize) -> IndexPath {
    // An empty list still measures section one, where future file results go.
    // Section two exists only when there are actual Hayai command results.
    let secondary = flat >= main && main < total;
    IndexPath {
        section: 1 + usize::from(secondary),
        row: if secondary { flat - main } else { flat },
        ..Default::default()
    }
}

fn scry_check_notice(
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

struct ResultListDelegate {
    search_pending: bool,
    update_notice: bool,
    scry_notice: bool,
    show_scry_updates: bool,
    scry_checked: Option<std::time::Instant>,
    scry_checking: bool,
    scry_installing: bool,
    scry_install_attempted: bool,
    scry_install_target: Option<String>,
    scry_ready: bool,
    scry_update_dismissed: bool,
    scry_discovery: Option<crate::scry_updates::Discovery>,
    scry_message: String,
    main_results: usize,
    update_dismissed: bool,
    checking_update: bool,
    update_message: Option<&'static str>,
    show_about: bool,
    show_updates: bool,
    update_result: Option<crate::updates::CheckResult>,
    apps_view: bool,
    brand_icon: Option<Arc<RenderImage>>,
    update_icon: Option<Arc<RenderImage>>,
    notification_selected: bool,
    update_failed: bool,
    restarting_update: bool,
    starting_scry: bool,
    last_query: String,
    scry_icon: Option<Arc<RenderImage>>,
    builtin_row_icons: [Option<Arc<RenderImage>>; 3],
    builtin_row_pixels: u32,
    file_search: bool,
    registry: Rc<RefCell<PluginRegistry>>,
    /// Catalog handle retained ONLY for the icon cache (`IconSource::Path`).
    catalog: Rc<RefCell<Catalog>>,
    results: Vec<CommandItem>,
    empty_state: Option<CommandItem>,
    /// Generation the current `results` belong to (see `PluginRegistry`).
    results_generation: u64,
    selected_index: Option<IndexPath>,
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

    fn needs_pinned_scry(&self) -> bool {
        self.file_search
            && self.results.iter().any(|item| {
                matches!(
                    item.id.as_str(),
                    "files:compatibility:update" | "files:compatibility:unavailable"
                )
            })
    }
    fn scry_target(&self) -> Option<&str> {
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
    fn calculator_view(&self) -> bool {
        !self.apps_view && !self.file_search
    }
    fn notification_visible(&self) -> bool {
        self.notification_count() != 0
    }
    fn hayai_notification_visible(&self) -> bool {
        self.apps_view
            && !self.show_about
            && !self.update_dismissed
            && (self.checking_update
                || crate::updates::ready_version().is_some()
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
    fn notification_count(&self) -> usize {
        usize::from(self.hayai_notification_visible())
            + usize::from(self.scry_notification_visible())
    }
    fn notification_kind(&self, row: usize) -> Option<UpdateNotification> {
        notification_kind(
            row,
            self.hayai_notification_visible(),
            self.scry_notification_visible(),
        )
    }
    fn selected_notification(&self) -> Option<UpdateNotification> {
        self.notification_selected
            .then(|| self.notification_kind(self.selected_index.map_or(0, |ix| ix.row)))
            .flatten()
    }

    fn check_scry_updates(
        &mut self,
        force: bool,
        window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) {
        let interval = if self
            .scry_discovery
            .as_ref()
            .is_some_and(|result| result.state == crate::scry_updates::State::Failed)
        {
            Duration::from_secs(15 * 60)
        } else {
            Duration::from_secs(24 * 60 * 60)
        };
        if self.scry_checking
            || self.scry_installing
            || (!force
                && self
                    .scry_checked
                    .is_some_and(|time| time.elapsed() < interval))
        {
            return;
        }
        self.scry_checking = true;
        self.scry_install_attempted = false;
        if force {
            self.scry_notice = true;
            self.scry_update_dismissed = false;
        }
        self.scry_checked = Some(std::time::Instant::now());
        self.scry_ready = false;
        self.scry_message = "Checking for Scry Search updates…".into();
        cx.spawn_in(window, async move |list, cx| {
            let (result, ready) = cx
                .background_spawn(async move {
                    let result = crate::scry_updates::check(force);
                    let ready = result.state == crate::scry_updates::State::Current
                        && native::scry_compatibility() == crate::scry::Compatibility::Ready;
                    (result, ready)
                })
                .await;
            let _ = cx.update(|window, cx| {
                list.update(cx, |list, cx| {
                    let previous = list.delegate().selected_notification();
                    let delegate = list.delegate_mut();
                    delegate.scry_checking = false;
                    if !delegate.scry_install_attempted {
                        delegate.scry_ready = ready;
                        delegate.scry_message = result.message.clone();
                        delegate.scry_discovery = Some(result);
                    } else {
                        // Discovery started before installation describes the
                        // previous daemon. It cannot override activation results.
                        delegate.scry_discovery = None;
                        delegate.scry_checked = None;
                    }
                    reconcile_notification_selection(list, previous, window, cx);
                    cx.notify();
                })
            });
        })
        .detach();
        cx.notify();
    }

    fn update_scry(&mut self, cx: &mut Context<ListState<Self>>) {
        if self.scry_installing || (self.scry_checking && !self.needs_pinned_scry()) {
            return;
        }
        let Some(target) = self.scry_target().map(str::to_owned) else {
            return;
        };
        self.scry_installing = true;
        self.scry_install_target = Some(target.clone());
        self.scry_install_attempted = true;
        self.scry_update_dismissed = false;
        self.scry_ready = false;
        self.scry_message = "Preparing the Scry Search update…".into();
        cx.spawn(async move |list, cx| {
            let (mut send, mut receive) = futures::channel::mpsc::channel(8);
            let job = cx.background_spawn(async move {
                crate::scry_updates::install(&target, |message| {
                    let _ = send.try_send(message);
                })
            });
            let mut restored_after_approval = false;
            while let Some(progress) = receive.next().await {
                let restore = progress.active && !restored_after_approval;
                let _ = list.update(cx, |list, cx| {
                    list.delegate_mut().scry_message = progress.message;
                    cx.notify();
                });
                if restore {
                    restored_after_approval = true;
                    // This runs after the entity update has unwound, so show
                    // can safely enter the window update and restore focus.
                    cx.update(|cx| {
                        let state = cx.global::<LauncherGlobal>().clone_handle();
                        state.borrow_mut().show_with_focus(true, cx);
                    });
                }
            }
            let result = job.await;
            let _ = list.update(cx, |list, cx| {
                let delegate = list.delegate_mut();
                delegate.scry_installing = false;
                delegate.scry_ready = result.is_ok();
                delegate.scry_message = result.unwrap_or_else(|message| message);
                // A failure/recovery may have changed the installed version.
                // Discover again before another installation attempt.
                delegate.scry_discovery = None;
                delegate.scry_checked = None;
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn main_count(&self) -> usize {
        self.main_results
    }
    fn section_offset(&self) -> usize {
        // Reserve section zero even when empty. An update arriving or being
        // dismissed must not change the indexes of already-selected results.
        1
    }
    fn flat_index(&self, ix: IndexPath) -> usize {
        // Notifications are a separate first section, not part of results.
        if ix.section == self.section_offset() {
            ix.row
        } else {
            self.main_count() + ix.row
        }
    }
    fn index_path(&self, flat: usize) -> IndexPath {
        result_index_path(flat, self.main_count(), self.results.len())
    }
    fn item(&self, ix: IndexPath) -> Option<&CommandItem> {
        if ix.section == 0 {
            return None;
        }
        self.results.get(self.flat_index(ix))
    }
    fn start_scry(&mut self, window: &mut Window, cx: &mut Context<ListState<Self>>) {
        if self.starting_scry {
            return;
        }
        let dispatch = self.registry.borrow_mut().dispatch(&self.last_query);
        let generation = dispatch.generation;
        let registry = self.registry.clone();
        self.starting_scry = true;
        cx.notify();
        cx.spawn_in(window, async move |list, cx| {
            let started = cx
                .background_spawn(async { native::start_scry_daemon() })
                .await;
            // The query generation also changes on hide/reset. A pending start
            // must never restore a file query over a newer query or session.
            if registry.borrow().current_generation() != generation {
                return;
            }
            if started {
                let deadline = std::time::Instant::now() + Duration::from_secs(20);
                while std::time::Instant::now() < deadline {
                    if registry.borrow().current_generation() != generation {
                        return;
                    }
                    let job = registry.borrow().background_search(&dispatch);
                    let Some(job) = job else {
                        break;
                    };
                    let items = cx.background_spawn(async move { job() }).await;
                    if registry.borrow().current_generation() != generation {
                        return;
                    }
                    // An empty result set is a successful search, too. Setup
                    // rows mean the daemon is still unavailable, not ready.
                    if scry_search_is_ready(&items) {
                        let _ = cx.update(|window, cx| {
                            list.update(cx, |list, cx| {
                                let delegate = list.delegate_mut();
                                delegate.starting_scry = false;
                                delegate.apply_results(generation, items, cx);
                                refresh_result_layout(list, window, cx);
                            })
                        });
                        return;
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(250))
                        .await;
                }
            }
            if registry.borrow().current_generation() != generation {
                return;
            }
            let _ = list.update(cx, |list, cx| {
                let delegate = list.delegate_mut();
                delegate.starting_scry = false;
                if let Some(item) = delegate.results.first_mut() {
                    item.title = "Couldn't start file search".into();
                    item.subtitle =
                        Some("Press Enter to retry, or use Learn more for setup help.".into());
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn new(registry: Rc<RefCell<PluginRegistry>>, catalog: Rc<RefCell<Catalog>>) -> Self {
        // These tiny embedded PNGs are decoded before the first frame, rather
        // than appearing after an asynchronous SVG raster job completes.
        let brand_icon = load_builtin_icon(include_bytes!("../assets/hayai.png"));
        let update_icon = load_builtin_icon(include_bytes!("../assets/icons/update.png"));
        let scry_icon = load_builtin_icon(include_bytes!("../assets/icons/scry.png"));
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
            notification_selected: crate::updates::ready_version().is_some(),
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
fn take_empty_state(items: &mut Vec<CommandItem>) -> Option<CommandItem> {
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

fn scry_search_is_ready(items: &[CommandItem]) -> bool {
    !items.iter().any(|item| {
        item.id.starts_with("files:compatibility:")
            || matches!(
                item.action,
                CommandAction::InstallFileSearch | CommandAction::StartFileSearch
            )
    })
}

fn section_heading(label: &'static str, cx: &App) -> impl IntoElement {
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
        CommandAction::CheckForUpdates | CommandAction::AboutHayai
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
        let label = if self.notification_visible() && section == 0 {
            "Notifications"
        } else if section == self.section_offset() + 1 {
            "Hayai"
        } else if self.apps_view {
            "Applications"
        } else if self.file_search {
            "Files"
        } else {
            "Calculator"
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
                                "icons/calculator.svg"
                            } else if self.empty_state.is_some() {
                                "icons/calculator-invalid.svg"
                            } else {
                                "icons/search-empty.svg"
                            },
                        )
                        .size(px(40.0))
                        .flex_none()
                        .text_color(cx.theme().muted_foreground.opacity(0.65)),
                )
                .child(if let Some(notice) = self.empty_state.as_ref() {
                    notice.title.clone()
                } else if self.file_search {
                    "No files found. Try another search.".to_string()
                } else {
                    "No apps found. Try another search.".to_string()
                })
            })
    }

    fn render_item(
        &mut self,
        ix: IndexPath,
        _window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Option<Self::Item> {
        self.refresh_builtin_row_icons(_window);
        if self.notification_visible() && ix.section == 0 {
            let scry = self.notification_kind(ix.row)? == UpdateNotification::Scry;
            let title = if scry {
                if self.scry_installing {
                    "Updating Scry Search"
                } else if self.scry_install_attempted && self.scry_ready {
                    "Scry Search updated"
                } else if self.scry_install_attempted {
                    "Couldn't update Scry Search"
                } else if self.scry_checking {
                    "Checking for Scry Search updates…"
                } else {
                    match self.scry_discovery.as_ref().map(|result| &result.state) {
                        Some(crate::scry_updates::State::Available) => {
                            "Scry Search update available"
                        }
                        Some(crate::scry_updates::State::Current) => "Scry Search is up to date",
                        Some(crate::scry_updates::State::NoCompatibleRelease) => {
                            "No compatible Scry Search update"
                        }
                        _ => "Couldn't check Scry Search for updates",
                    }
                }
            } else if self.checking_update {
                "Checking for Hayai updates…"
            } else if self.restarting_update {
                "Restarting to update…"
            } else if crate::updates::ready_version().is_some() {
                "Hayai update available"
            } else if self.update_result == Some(crate::updates::CheckResult::Current) {
                "Hayai is up to date"
            } else {
                "Couldn't check Hayai for updates"
            };
            let subtitle = if scry {
                if self.scry_installing {
                    "Installation in progress.".into()
                } else if self.scry_install_attempted && self.scry_ready {
                    "Enter to dismiss.".into()
                } else if self.scry_install_attempted && self.scry_message.chars().count() > 55 {
                    "Open details to retry.".into()
                } else if self.scry_checking && !self.scry_install_attempted {
                    "You can keep searching.".into()
                } else {
                    self.scry_message.clone()
                }
            } else if self.checking_update {
                "You can keep searching.".into()
            } else if self.update_failed {
                "Couldn't start the update. Select to retry.".into()
            } else {
                crate::updates::ready_version()
                    .map(|version| format!("Hayai {version}"))
                    .unwrap_or_else(|| self.update_message.unwrap_or("Try again later.").into())
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
                    .unwrap_or("Manually check for a new Hayai release")
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
        // Bookkeeping first (cheap, in-memory), then hide the window
        // *immediately* — a shell call can take hundreds of ms and must not
        // hold the launcher on screen after Enter.
        if self.search_pending && !self.show_scry_updates && !self.show_updates {
            return;
        }
        if self.show_scry_updates && self.scry_ready {
            let query = self.last_query.clone();
            self.show_scry_updates = false;
            // These compatibility rows describe the pre-install daemon.
            // Drop them before refreshing so they cannot reopen setup.
            self.results.clear();
            self.perform_search(&query, window, cx).detach();
            return;
        }
        if let Some(notification) = self.selected_notification()
            && !self.show_updates
            && !self.show_scry_updates
        {
            match notification {
                UpdateNotification::Scry => {
                    if self.scry_installing {
                        return;
                    }
                    if self.scry_install_attempted {
                        if self.scry_ready {
                            self.scry_update_dismissed = true;
                            self.notification_selected = false;
                            self.selected_index =
                                (!self.results.is_empty()).then(|| self.index_path(0));
                            cx.notify();
                        } else {
                            self.show_scry_updates = true;
                            cx.notify();
                        }
                        return;
                    }
                    if self.scry_checking {
                        return;
                    }
                    if self
                        .scry_discovery
                        .as_ref()
                        .is_some_and(|result| result.state == crate::scry_updates::State::Available)
                    {
                        self.show_scry_updates = true;
                    } else {
                        self.check_scry_updates(true, window, cx);
                    }
                    cx.notify();
                    return;
                }
                UpdateNotification::Hayai
                    if !self.checking_update && crate::updates::ready_version().is_some() =>
                {
                    self.show_updates = true;
                    self.update_result = Some(crate::updates::CheckResult::Ready);
                    cx.notify();
                    return;
                }
                _ => {}
            }
        }
        let action = if self.show_scry_updates {
            if self.scry_installing || (self.scry_checking && !self.needs_pinned_scry()) {
                return;
            }
            Some(if self.scry_target().is_some() {
                CommandAction::UpdateFileSearch
            } else {
                CommandAction::CheckFileSearchUpdates
            })
        } else if self.show_updates {
            if self.checking_update || self.restarting_update {
                return;
            }
            Some(
                if self.update_result == Some(crate::updates::CheckResult::Ready) {
                    CommandAction::RestartToUpdate
                } else {
                    CommandAction::CheckForUpdates
                },
            )
        } else if self.notification_selected {
            Some(
                if self.selected_notification() == Some(UpdateNotification::Scry) {
                    CommandAction::CheckFileSearchUpdates
                } else {
                    CommandAction::CheckForUpdates
                },
            )
        } else {
            self.selected_index
                .and_then(|ix| self.item(ix))
                .map(|item| item.action.clone())
        };
        match action {
            Some(CommandAction::CheckFileSearchUpdates) => {
                // Only unavailable/incompatible file search needs a blocking
                // setup page. Optional discovery never replaces search results.
                self.show_scry_updates = self.file_search && !scry_search_is_ready(&self.results);
                self.show_updates = false;
                self.show_about = false;
                self.check_scry_updates(true, window, cx);
            }
            Some(CommandAction::UpdateFileSearch) => {
                self.update_scry(cx);
            }
            Some(CommandAction::CheckForUpdates) => {
                if self.file_search
                    && self
                        .results
                        .iter()
                        .any(|item| item.id == "files:compatibility:newer")
                    && crate::updates::ready_version().is_some()
                {
                    self.show_updates = true;
                    self.update_result = Some(crate::updates::CheckResult::Ready);
                    cx.notify();
                    return;
                }
                self.check_scry_updates(true, window, cx);
                self.show_updates = false;
                self.update_notice = true;
                self.update_dismissed = false;
                self.show_about = false;
                cx.notify();
                if self.checking_update {
                    return;
                }
                self.checking_update = true;
                self.update_failed = false;
                self.update_result = None;
                self.update_message = Some("Checking for updates…");
                cx.notify();
                cx.spawn_in(window, async move |list, cx| {
                    let result = cx.background_spawn(async { crate::updates::check() }).await;
                    let _ = cx.update(|window, cx| {
                        list.update(cx, |list, cx| {
                            let previous = list.delegate().selected_notification();
                            let delegate = list.delegate_mut();
                            delegate.checking_update = false;
                            delegate.update_result = Some(result);
                            delegate.update_message = Some(result.message());
                            reconcile_notification_selection(list, previous, window, cx);
                            cx.notify();
                        })
                    });
                })
                .detach();
            }
            Some(CommandAction::AboutHayai) => {
                self.show_about = true;
                cx.notify();
            }
            Some(CommandAction::RestartToUpdate) => {
                if cfg!(debug_assertions) {
                    return;
                }
                if self.restarting_update {
                    return;
                }
                self.restarting_update = true;
                cx.notify();
                cx.spawn(async move |list, cx| {
                    let ready = cx
                        .background_spawn(async { crate::updates::prepare_restart() })
                        .await;
                    if ready {
                        cx.update(|cx| cx.quit());
                    } else {
                        let _ = list.update(cx, |list, cx| {
                            let delegate = list.delegate_mut();
                            delegate.restarting_update = false;
                            delegate.update_failed = true;
                            cx.notify();
                        });
                    }
                })
                .detach();
            }
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
            Some(
                CommandAction::ShowText(_)
                | CommandAction::EmptyState
                | CommandAction::PendingState,
            ) => {
                // Stays on screen; no-op.
            }
            Some(CommandAction::InstallFileSearch) => {
                LauncherState::dismiss(window, cx);
                cx.background_spawn(async move {
                    native::install_scry_daemon();
                })
                .detach();
            }
            Some(CommandAction::StartFileSearch) => {
                self.start_scry(window, cx);
            }
            None => {
                if self.empty_state.is_none() {
                    LauncherState::dismiss(window, cx);
                }
            }
        }
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<ListState<Self>>) {
        LauncherState::dismiss(window, cx);
    }
}

fn icon_neighborhood(row: usize, count: usize, radius: usize) -> std::ops::Range<usize> {
    row.saturating_sub(radius).min(count)..row.saturating_add(radius).saturating_add(1).min(count)
}

fn row_icon_pixels(scale: f32) -> u32 {
    // Exact-size filtering followed by fractional-DPI GPU sampling softened
    // small glyphs. Keep 25% sampling headroom, capped at the shell's 48px
    // source, to preserve detail without retaining oversized rasters.
    (22.0 * scale * 1.25).ceil().clamp(1.0, 48.0) as u32
}

pub(crate) fn prepare_row_icon(icon: native::IconImage, pixels: u32) -> Option<Arc<RenderImage>> {
    let buffer = image::RgbaImage::from_raw(icon.width, icon.height, icon.bgra)?;
    // The GPU uses bilinear sampling without mipmaps. Prefilter on the worker
    // before upload so minifying a 48px source does not skip fine icon details.
    // Lanczos preserves small edges better than the previous box average, which
    // visibly softened app glyphs at low/fractional DPI.
    // Preserve undersized sources rather than allocating an upscaled bitmap.
    let buffer = if buffer.width() > pixels || buffer.height() > pixels {
        let ratio = pixels as f64 / buffer.width().max(buffer.height()) as f64;
        let width = (buffer.width() as f64 * ratio).round().max(1.0) as u32;
        let height = (buffer.height() as f64 * ratio).round().max(1.0) as u32;
        image::imageops::resize(
            &buffer,
            width,
            height,
            image::imageops::FilterType::Lanczos3,
        )
    } else {
        buffer
    };
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])))
}

/// Decode `path`'s icon on the background thread pool, then hand the result
/// back to the catalog and repaint. GDI/COM icon extraction is too slow to do
/// synchronously in the render path (it blocks keystrokes).
fn spawn_icon_load(
    path: PathBuf,
    pixel_size: u32,
    catalog: Rc<RefCell<Catalog>>,
    cx: &mut Context<ListState<ResultListDelegate>>,
) {
    cx.spawn(async move |list, cx| {
        let decode_path = path.clone();
        let rendered = cx
            .background_spawn(async move {
                native::extract_icon_rgba(&decode_path)
                    .and_then(|icon| prepare_row_icon(icon, pixel_size))
            })
            .await;

        catalog
            .borrow_mut()
            .set_icon_ready(path, pixel_size, rendered);
        let _ = list.update(cx, |_, cx| cx.notify());
    })
    .detach();
}

#[derive(gpui::IntoElement)]
struct ResultRow {
    loading: bool,
    compact: bool,
    icon_size: f32,
    base: ListItem,
    title: SharedString,
    subtitle: Option<SharedString>,
    file_size: Option<u64>,
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
            loading: false,
            compact: false,
            icon_size: 22.0,
            base: ListItem::new(id).selected(selected),
            title: title.into(),
            subtitle: subtitle.map(Into::into),
            file_size: None,
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
            loading,
            compact,
            icon_size,
            base,
            title,
            subtitle,
            file_size,
            calculation_detail,
            icon,
            selected: _,
            calculator,
        } = self;
        let content: AnyElement = if calculator {
            ResultRow::render_calculator(title, subtitle, calculation_detail, cx)
        } else {
            ResultRow::render_plain(
                title,
                subtitle,
                file_size,
                icon,
                compact,
                (icon_size, loading),
                cx,
            )
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
        file_size: Option<u64>,
        icon: Option<Arc<RenderImage>>,
        compact: bool,
        icon_state: (f32, bool),
        cx: &mut App,
    ) -> AnyElement {
        let (icon_size, loading) = icon_state;
        h_flex()
            .when(compact, |row| row.h(px(24.0)))
            .items_center()
            .gap_3()
            .w_full()
            .min_w_0()
            .child(
                div()
                    .w(px(22.0))
                    .h(px(22.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(loading, |slot| {
                        slot.child(
                            gpui_component::spinner::Spinner::new()
                                .icon(gpui_component::Icon::default().path("icons/spinner.svg"))
                                .with_size(px(icon_size))
                                .color(cx.theme().muted_foreground),
                        )
                    })
                    .when_some(icon.filter(|_| !loading), |slot, image| {
                        slot.child(img(image).size(px(icon_size)).flex_none())
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    // Every item in this virtual list shares one measured
                    // height. Neither a filename nor a path may wrap and
                    // extend into the following row.
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .h(px(24.0))
                            .line_height(px(24.0))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis_middle()
                            .child(title),
                    )
                    .when_some(
                        if compact { None } else { subtitle.clone() },
                        |col, subtitle| {
                            col.child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .w_full()
                                    .min_w_0()
                                    .h(px(18.0))
                                    .line_height(px(18.0))
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis_middle()
                                    .child(subtitle),
                            )
                        },
                    ),
            )
            .when_some(if compact { subtitle } else { None }, |row, subtitle| {
                row.child(
                    div()
                        .min_w_0()
                        .max_w(px(360.0))
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .truncate()
                        .child(subtitle),
                )
            })
            .when_some(file_size, |row, bytes| {
                row.child(
                    div()
                        .flex_none()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(crate::files::format_size(bytes)),
                )
            })
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
        let title = SharedString::from(format_result_for_display(&title));
        let (source_label, target_label, note) = match calculation_detail {
            Some(CalculationDetail::Units {
                source_label,
                target_label,
            }) => (
                Some(SharedString::from(source_label)),
                Some(SharedString::from(target_label)),
                None,
            ),
            Some(CalculationDetail::Timezones {
                source_label,
                target_label,
            }) => (
                Some(SharedString::from(source_label)),
                Some(SharedString::from(target_label)),
                None,
            ),
            Some(CalculationDetail::Currency {
                source_label,
                target_label,
                updated_label,
            }) => (
                Some(SharedString::from(source_label)),
                Some(SharedString::from(target_label)),
                Some(SharedString::from(updated_label)),
            ),
            None => (None, None, None),
        };
        let center_width = if note.is_some() { 176.0 } else { 44.0 };
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
                    .flex()
                    .flex_col()
                    .w_full()
                    .min_h(px(96.0))
                    .rounded(cx.theme().radius)
                    .bg(cx.theme().list_active)
                    .child(
                        h_flex()
                            .items_stretch()
                            .w_full()
                            .flex_1()
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
                                                let offset = expression[search_start..]
                                                    .find(word)?
                                                    + search_start;
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
                                                    StyledText::new(expression)
                                                        .with_highlights(highlights),
                                                ),
                                        )
                                    })
                                    .when_some(source_label, |col, label| {
                                        col.child(zone_chip(label, cx))
                                    }),
                            )
                            .child(
                                div()
                                    .w(px(center_width))
                                    .self_stretch()
                                    .flex_none()
                                    .flex()
                                    .flex_col()
                                    .items_center()
                                    .justify_center()
                                    .child(div().w(px(1.0)).flex_1().bg(conversion_color))
                                    .child(
                                        div()
                                            // A fixed SVG slot avoids font-baseline offsets;
                                            // equal flex rules above/below keep its gaps symmetric.
                                            .size(px(28.0))
                                            .flex_none()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child(
                                                gpui::svg()
                                                    .path("icons/arrow-right.svg")
                                                    .size(px(20.0))
                                                    .flex_none()
                                                    .text_color(conversion_color),
                                            ),
                                    )
                                    .when_some(note, |center, label| {
                                        center.child(
                                            div()
                                                .relative()
                                                .top(px(-7.0))
                                                .text_xs()
                                                .text_center()
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(conversion_color)
                                                .child(label),
                                        )
                                    })
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
                                    .when_some(target_label, |col, label| {
                                        col.child(zone_chip(label, cx))
                                    }),
                            ),
                    ),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{format_expression_for_display, format_result_for_display};

    #[test]
    fn icon_warm_region_stays_bounded_at_result_edges() {
        assert_eq!(super::icon_neighborhood(0, 1000, 32), 0..33);
        assert_eq!(super::icon_neighborhood(500, 1000, 32), 468..533);
        assert_eq!(super::icon_neighborhood(999, 1000, 32), 967..1000);
        assert_eq!(super::icon_neighborhood(0, 0, 32), 0..0);
        assert_eq!(super::icon_neighborhood(usize::MAX, 10, 32), 10..10);
    }

    #[test]
    fn row_icon_prefilter_averages_fine_details_at_physical_size() {
        assert_eq!(super::row_icon_pixels(1.0), 28);
        assert_eq!(super::row_icon_pixels(1.25), 35);
        assert_eq!(super::row_icon_pixels(1.5), 42);
        assert_eq!(super::row_icon_pixels(2.0), 48);
        let mut bgra = Vec::new();
        for y in 0..44 {
            for x in 0..44 {
                let value = if (x + y) % 2 == 0 { 0 } else { 255 };
                bgra.extend_from_slice(&[value, value, value, 255]);
            }
        }
        let icon = super::prepare_row_icon(
            crate::native::IconImage {
                width: 44,
                height: 44,
                bgra,
            },
            22,
        )
        .unwrap();
        assert_eq!(icon.size(0).width.0, 22);
        assert_eq!(icon.size(0).height.0, 22);
        let bytes = icon.as_bytes(0).unwrap();
        for y in 3..19 {
            for x in 3..19 {
                let pixel = &bytes[(y * 22 + x) * 4..][..4];
                assert!((127..=128).contains(&pixel[0]));
                assert_eq!(pixel[3], 255);
            }
        }
    }

    #[test]
    fn empty_results_keep_a_valid_measurement_section_when_search_is_cleared() {
        for (main, total, section) in [(0, 0, 1), (5, 5, 1), (0, 2, 2), (5, 7, 1)] {
            assert_eq!(super::result_index_path(0, main, total).section, section);
        }
        assert_eq!(super::result_index_path(5, 5, 7).section, 2);
        assert_eq!(super::result_index_path(5, 5, 7).row, 0);
    }

    #[test]
    fn failed_checks_are_not_notifications_even_after_a_manual_check() {
        use crate::scry_updates::State;
        assert!(!super::scry_check_notice(Some(&State::Failed), false, true));
        assert!(!super::scry_check_notice(
            Some(&State::Failed),
            false,
            false
        ));
        assert!(super::scry_check_notice(
            Some(&State::Available),
            false,
            false
        ));
        assert!(super::scry_check_notice(Some(&State::Current), false, true));
        assert!(!super::scry_check_notice(
            Some(&State::Current),
            false,
            false
        ));
    }

    #[test]
    fn update_notifications_keep_distinct_actions_in_a_shared_section() {
        use super::{
            UpdateNotification::{Hayai, Scry},
            notification_kind,
        };
        assert_eq!(notification_kind(0, true, true), Some(Hayai));
        assert_eq!(notification_kind(1, true, true), Some(Scry));
        assert_eq!(notification_kind(2, true, true), None);
        assert_eq!(notification_kind(0, false, true), Some(Scry));
        assert_eq!(notification_kind(1, false, true), None);
        assert_eq!(notification_kind(0, true, false), Some(Hayai));
        assert_eq!(notification_kind(0, false, false), None);
    }

    #[test]
    fn notification_actions_include_dismiss_without_file_operations() {
        let menu = super::ActionMenu {
            diagnostics_only: false,
            notification: true,
            notification_action: "Restart to update",
            submenu: None,
            path: Default::default(),
            is_dir: false,
            is_app: false,
            can_run_as_admin: false,
            selected: 0,
        };
        assert_eq!(
            menu.actions().map(|(_, label)| label).collect::<Vec<_>>(),
            ["Restart to update", "Dismiss", "Copy diagnostic log"]
        );
    }

    #[test]
    fn update_page_actions_only_copy_diagnostics() {
        let menu = super::ActionMenu {
            diagnostics_only: true,
            notification: false,
            notification_action: "",
            submenu: None,
            path: Default::default(),
            is_dir: false,
            is_app: false,
            can_run_as_admin: false,
            selected: 0,
        };
        assert_eq!(
            menu.actions().map(|(_, label)| label).collect::<Vec<_>>(),
            ["Copy diagnostic log"]
        );
    }

    #[test]
    fn embedded_notification_icon_decodes_without_svg_work() {
        let icon = super::load_builtin_icon(include_bytes!("../assets/icons/update.png"))
            .expect("decode baked update icon");
        assert_eq!(super::HAYAI_ROW_ICON_SIZE, 22.0);
        assert_eq!(icon.size(0).width.0, 128);
        assert_eq!(icon.size(0).height.0, 128);
        assert!(
            icon.as_bytes(0)
                .unwrap()
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| pixel[3] != 0)
        );
    }

    #[test]
    fn scry_readiness_accepts_empty_searches_but_not_setup_prompts() {
        use crate::commands::{CommandAction, CommandItem, IconSource};
        let item = |action| CommandItem {
            id: "readiness-test".into(),
            title: String::new(),
            subtitle: None,
            file_size: None,
            calculation_detail: None,
            icon: IconSource::None,
            action,
        };
        assert!(super::scry_search_is_ready(&[]));
        assert!(!super::scry_search_is_ready(&[item(
            CommandAction::StartFileSearch
        )]));
        assert!(!super::scry_search_is_ready(&[item(
            CommandAction::InstallFileSearch
        )]));
        assert!(super::scry_search_is_ready(&[item(
            CommandAction::OpenFile("example.txt".into())
        )]));
    }

    #[test]
    fn baked_builtin_icons_have_separate_row_and_detail_sizes() {
        for bytes in [
            include_bytes!("../assets/hayai.png").as_slice(),
            include_bytes!("../assets/icons/update.png").as_slice(),
            include_bytes!("../assets/icons/scry.png").as_slice(),
        ] {
            let source = super::load_builtin_icon(bytes).expect("decode baked icon");
            for scale in [1.0, 1.25, 1.5, 2.0] {
                let pixels = super::row_icon_pixels(scale);
                let row = super::builtin_row_icon(&Some(source.clone()), pixels).unwrap();
                assert_eq!(row.size(0).width.0, pixels as i32);
                assert_eq!(row.size(0).height.0, pixels as i32);
            }
            assert_eq!(source.size(0).width.0, 128);
        }
    }

    #[test]
    fn open_with_submenu_has_its_own_numbering_and_returns_to_parent() {
        let mut menu = super::ActionMenu {
            diagnostics_only: false,
            notification: false,
            notification_action: "",
            submenu: Some(super::OpenWithMenu {
                request: 1,
                loading: true,
                apps: vec![super::OpenWithApp {
                    id: "test-handler".into(),
                    name: "Text editor".into(),
                    icon: None,
                }]
                .into(),
            }),
            path: std::env::current_dir().unwrap().join("notes.txt"),
            is_dir: false,
            is_app: false,
            can_run_as_admin: false,
            selected: 0,
        };
        assert_eq!(menu.count(), 4); // Keep the parent panel intact until apps are ready.
        assert!(matches!(menu.row(0), Some((super::MenuAction::Open, _, _))));
        menu.submenu.as_mut().unwrap().loading = false;
        assert_eq!(menu.count(), 2);
        assert!(matches!(
            menu.row(0),
            Some((super::MenuAction::AssociatedApp(0), _, _))
        ));
        assert!(matches!(
            menu.row(1),
            Some((super::MenuAction::ChooseApp, _, _))
        ));
        menu.selected = 1;
        assert!(menu.return_to_parent());
        assert_eq!(menu.selected, 0);
        assert_eq!(menu.count(), 4);
        assert!(matches!(menu.row(0), Some((super::MenuAction::Open, _, _))));
        assert!(!menu.return_to_parent());
    }

    #[test]
    fn context_actions_follow_result_type_and_stay_compact() {
        let root = std::env::current_dir().unwrap();
        for (name, is_dir, is_app, admin, expected) in [
            (
                "report.pdf",
                false,
                false,
                false,
                vec!["Open", "Open with…", "Show in Explorer", "Copy path"],
            ),
            (
                "tool.exe",
                false,
                false,
                true,
                vec![
                    "Open",
                    "Show in Explorer",
                    "Copy path",
                    "Run as administrator",
                ],
            ),
            (
                "Photos",
                true,
                false,
                false,
                vec!["Open folder", "Open parent folder", "Copy path"],
            ),
            (
                "browser.lnk",
                false,
                true,
                true,
                vec![
                    "Open application",
                    "Show in Explorer",
                    "Copy path",
                    "Run as administrator",
                ],
            ),
        ] {
            let menu = super::ActionMenu {
                diagnostics_only: false,
                notification: false,
                notification_action: "",
                submenu: None,
                path: root.join(name),
                is_dir,
                is_app,
                can_run_as_admin: admin,
                selected: 0,
            };
            assert_eq!(
                menu.actions().map(|(_, label)| label).collect::<Vec<_>>(),
                expected
            );
        }
        let packaged = super::ActionMenu {
            diagnostics_only: false,
            notification: false,
            notification_action: "",
            submenu: None,
            path: "shell:AppsFolder/Publisher.App!App".into(),
            is_dir: false,
            is_app: true,
            can_run_as_admin: false,
            selected: 0,
        };
        assert_eq!(
            packaged
                .actions()
                .map(|(_, label)| label)
                .collect::<Vec<_>>(),
            ["Open application"]
        );
    }

    #[test]
    fn displays_numeric_exponents_as_superscripts() {
        assert_eq!(format_expression_for_display("43560 ft^2"), "43560 ft²");
        assert_eq!(format_expression_for_display("m^-12"), "m⁻¹²");
        assert_eq!(format_expression_for_display("2^x"), "2^x");
    }

    #[test]
    fn spaces_binary_operators_without_splitting_unary_signs() {
        assert_eq!(format_expression_for_display("12 -2"), "12 - 2");
        assert_eq!(format_expression_for_display("12+2* 3"), "12 + 2 * 3");
        assert_eq!(format_expression_for_display("5*-2"), "5 * -2");
        assert_eq!(format_expression_for_display("-12/3"), "-12 / 3");
    }

    #[test]
    fn expands_and_pluralizes_duration_units() {
        assert_eq!(
            format_expression_for_display("12 hr from now"),
            "12 hours from now"
        );
        assert_eq!(
            format_expression_for_display("12 hrs from now"),
            "12 hours from now"
        );
        assert_eq!(format_expression_for_display("1 hrs ago"), "1 hour ago");
        assert_eq!(
            format_expression_for_display("in 2 wk 1 d"),
            "in 2 weeks 1 day"
        );
    }

    #[test]
    fn keeps_clock_and_meridiem_on_the_same_line() {
        assert_eq!(
            format_result_for_display("Sunday, August 30, 2026, 2:35 AM"),
            "Sunday, August 30, 2026,\n2:35\u{00a0}AM"
        );
        assert_eq!(
            format_result_for_display("9:05 PM IST"),
            "9:05\u{00a0}PM IST"
        );
    }

    #[test]
    fn displays_timezone_abbreviations_in_uppercase() {
        assert_eq!(
            format_expression_for_display("12pm ist to cst"),
            "12 PM IST to CST"
        );
        assert_eq!(
            format_expression_for_display("12 pm ist to cst"),
            "12 PM IST to CST"
        );
        assert_eq!(format_expression_for_display("utc to jst"), "UTC to JST");
        assert_eq!(format_expression_for_display("1 pm"), "1 pm");
        assert_eq!(
            format_expression_for_display("1 usd to inr"),
            "1 USD to INR"
        );
    }
    #[test]
    fn empty_notices_are_not_selectable_results_and_recovery_clears_them() {
        use super::{CommandAction, CommandItem, IconSource, take_empty_state};
        let notice = CommandItem {
            id: "calc:error".into(),
            title: "Invalid expression".into(),
            subtitle: None,
            file_size: None,
            calculation_detail: None,
            icon: IconSource::None,
            action: CommandAction::EmptyState,
        };
        let mut items = vec![notice];
        assert_eq!(
            take_empty_state(&mut items).unwrap().title,
            "Invalid expression"
        );
        assert!(items.is_empty());
        assert!(take_empty_state(&mut items).is_none());
        items.push(CommandItem {
            id: "calc:2+2".into(),
            title: "4".into(),
            subtitle: None,
            file_size: None,
            calculation_detail: None,
            icon: IconSource::None,
            action: CommandAction::CopyToClipboard("4".into()),
        });
        assert!(take_empty_state(&mut items).is_none());
        assert_eq!(items.len(), 1);
    }
}
