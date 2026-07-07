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
use crate::native;

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
    list: Option<Entity<ListState<AppListDelegate>>>,
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
        type OpenedEntities = (Entity<ListState<AppListDelegate>>, Entity<InputState>);
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
                    let list = cx.new(|cx| {
                        ListState::new(AppListDelegate::new(catalog), window, cx).searchable(false)
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
    list: Entity<ListState<AppListDelegate>>,
    search_input: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}

impl LauncherRoot {
    fn new(
        list: Entity<ListState<AppListDelegate>>,
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

/// Drives the results list: fuzzy-searches the catalog on every keystroke,
/// renders one row per match, and launches the selected app on confirm.
struct AppListDelegate {
    catalog: Rc<RefCell<Catalog>>,
    /// Catalog indices matching the current query, in display order.
    results: Vec<usize>,
    selected_index: Option<IndexPath>,
}

impl AppListDelegate {
    fn new(catalog: Rc<RefCell<Catalog>>) -> Self {
        let results = catalog.borrow().search("");
        Self {
            catalog,
            results,
            selected_index: Some(IndexPath::default()),
        }
    }
}

impl ListDelegate for AppListDelegate {
    type Item = AppRow;

    fn perform_search(
        &mut self,
        query: &str,
        _window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Task<()> {
        self.results = self.catalog.borrow().search(query);
        self.selected_index = (!self.results.is_empty()).then(IndexPath::default);
        cx.notify();
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
        let &index = self.results.get(ix.row)?;
        let mut catalog = self.catalog.borrow_mut();
        let name = catalog.app(index)?.name.clone();
        let icon = match catalog.request_icon(index) {
            IconRequest::Ready(icon) => icon,
            IconRequest::Loading => None,
            IconRequest::Load(path) => {
                drop(catalog);
                spawn_icon_load(path, self.catalog.clone(), cx);
                None
            }
        };
        let selected = self.selected_index == Some(ix);
        Some(AppRow::new(ix, name, icon, selected))
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
        // *immediately* — ShellExecuteW can take hundreds of ms and must not
        // hold the launcher on screen after Enter. Both the shell call and
        // the recents write happen on the background pool.
        let launch = self
            .selected_index
            .and_then(|ix| self.results.get(ix.row).copied())
            .and_then(|index| self.catalog.borrow_mut().mark_launched(index));
        LauncherState::dismiss(window, cx);
        if let Some((path, recents)) = launch {
            cx.background_spawn(async move {
                native::launch_path(&path);
                crate::apps::write_recents_file(&recents);
            })
            .detach();
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
    cx: &mut Context<ListState<AppListDelegate>>,
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
struct AppRow {
    base: ListItem,
    name: SharedString,
    icon: Option<Arc<RenderImage>>,
    selected: bool,
}

impl AppRow {
    fn new(id: IndexPath, name: String, icon: Option<Arc<RenderImage>>, selected: bool) -> Self {
        Self {
            base: ListItem::new(id).selected(selected),
            name: name.into(),
            icon,
            selected,
        }
    }
}

impl Selectable for AppRow {
    fn selected(mut self, selected: bool) -> Self {
        self.base = self.base.selected(selected);
        self.selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl gpui::RenderOnce for AppRow {
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
                .child(div().flex_1().child(self.name)),
        )
    }
}
