use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use gpui::prelude::*;
use gpui::{
    AnyWindowHandle, App, Bounds, Context, Entity, FocusHandle, Focusable, Half, IntoElement,
    RenderImage, SharedString, Subscription, Task, Window, WindowBackgroundAppearance,
    WindowBounds, WindowKind, WindowOptions, div, img, px, relative, size,
};
use gpui_component::{
    ActiveTheme, IndexPath, Root, Selectable, Sizable, Size, h_flex,
    input::{Escape, Input, InputEvent, InputState, MoveDown, MoveUp},
    list::{List, ListDelegate, ListItem, ListState},
};

use crate::apps::{Catalog, CatalogGlobal, IconRequest};
use crate::commands::{CommandAction, CommandItem, IconSource};
use crate::native;
use crate::plugins::PluginRegistry;

const LAUNCHER_WIDTH: f32 = 720.0;
const LAUNCHER_HEIGHT: f32 = 400.0;

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
        if let (Some(handle), Some(list), Some(search_input)) =
            (self.window, self.list.clone(), self.search_input.clone())
        {
            let shown = handle
                .update(cx, |_, window, cx| {
                    // Win32 focus first (this re-shows the hidden window via SW_SHOW and
                    // calls SetForegroundWindow etc.), then GPUI focus. Reversed order
                    // would have GPUI's WM_SETFOCUS handler clobber our focus state.
                    native::focus_launcher_window(window);
                    // `InputState::set_value` is a programmatic change and does not
                    // itself emit `InputEvent::Change`, so the results must be reset
                    // explicitly here too — otherwise the box goes blank but the
                    // previous query's results stay on screen.
                    search_input.update(cx, |input, cx| input.set_value("", window, cx));
                    list.update(cx, |list, cx| {
                        let _ = list.delegate_mut().perform_search("", window, cx);
                    });
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
        }

        let bounds = Bounds::centered(None, size(px(LAUNCHER_WIDTH), px(LAUNCHER_HEIGHT)), cx);

        // `open_window`'s closure returns the window's root view (a `Root`), so we
        // stash the inner entities here to read back out afterward.
        type OpenedEntities = (Entity<ListState<ResultListDelegate>>, Entity<InputState>);
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
                    let search_input = cx.new(|cx| {
                        InputState::new(window, cx)
                            .placeholder("Search apps, files, commands...")
                    });
                    *entities_slot_for_window.borrow_mut() =
                        Some((list.clone(), search_input.clone()));

                    let view =
                        cx.new(|cx| LauncherRoot::new(list.clone(), search_input.clone(), window, cx));

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

        let (list, search_input) = entities_slot
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
                let _ = list.delegate_mut().perform_search(&query, window, cx);
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
struct LauncherRoot {
    list: Entity<ListState<ResultListDelegate>>,
    search_input: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}

impl LauncherRoot {
    fn new(
        list: Entity<ListState<ResultListDelegate>>,
        search_input: Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let _subscriptions = vec![cx.subscribe_in(&search_input, window, Self::on_search_event)];
        Self {
            list,
            search_input,
            _subscriptions,
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
                let query = input.read(cx).value().to_string();
                self.list.update(cx, |list, cx| {
                    let _ = list.delegate_mut().perform_search(&query, window, cx);
                });
            }
            InputEvent::PressEnter { .. } => {
                self.list.update(cx, |list, cx| {
                    list.delegate_mut().confirm(false, window, cx);
                });
            }
            _ => {}
        }
    }

    fn move_selection(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        self.list.update(cx, |list, cx| {
            let count = list.delegate().items_count(0, cx);
            if count == 0 {
                return;
            }
            let current = list.selected_index().map(|ix| ix.row as isize).unwrap_or(-1);
            let next = (current + delta).rem_euclid(count as isize) as usize;
            list.set_selected_index(Some(IndexPath::new(next)), window, cx);
            list.scroll_to_selected_item(window, cx);
        });
    }

    fn on_move_up(&mut self, _: &MoveUp, window: &mut Window, cx: &mut Context<Self>) {
        self.move_selection(-1, window, cx);
    }

    fn on_move_down(&mut self, _: &MoveDown, window: &mut Window, cx: &mut Context<Self>) {
        self.move_selection(1, window, cx);
    }

    fn on_escape(&mut self, _: &Escape, window: &mut Window, cx: &mut Context<Self>) {
        self.list.update(cx, |list, cx| {
            list.delegate_mut().cancel(window, cx);
        });
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
                    .px_3()
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(cx.theme().border)
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
                    .child("Open Application")
                    .child(enter_kbd(cx)),
            )
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

    fn apply_results(&mut self, generation: u64, items: Vec<CommandItem>, cx: &mut Context<ListState<Self>>) {
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
        _window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Task<()> {
        let dispatch = self.registry.borrow_mut().dispatch(query);
        // All current providers are synchronous & in-memory: run inline.
        // (When an async/debounced provider lands, this branches on
        // dispatch.debounce: spawn, sleep ~50ms, run, then apply only if
        // dispatch.generation == registry.current_generation().)
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
        Some(ResultRow::new(ix, item.title, item.subtitle, icon, selected))
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

    fn confirm(&mut self, _secondary: bool, window: &mut Window, cx: &mut Context<ListState<Self>>) {
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
                LauncherState::dismiss(window, cx);
                cx.background_spawn(async move {
                    native::launch_path(&path);
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
    icon: Option<Arc<RenderImage>>,
    selected: bool,
}

impl ResultRow {
    fn new(
        id: IndexPath,
        title: String,
        subtitle: Option<String>,
        icon: Option<Arc<RenderImage>>,
        selected: bool,
    ) -> Self {
        Self {
            base: ListItem::new(id).selected(selected),
            title: title.into(),
            subtitle: subtitle.map(Into::into),
            icon,
            selected,
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
        self.base.py_1p5().rounded(cx.theme().radius).child(
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
                        .when_some(self.icon, |slot, image| {
                            slot.child(img(image).size(px(22.0)))
                        }),
                )
                .child(
                    div().flex_1().flex().flex_col().child(self.title).when_some(
                        self.subtitle,
                        |col, subtitle| {
                            col.child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(subtitle),
                            )
                        },
                    ),
                ),
        )
    }
}
