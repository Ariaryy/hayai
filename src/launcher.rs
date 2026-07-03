use std::cell::RefCell;
use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use gpui::prelude::*;
use gpui::{
    AnyWindowHandle, App, Bounds, ClipboardItem, Context, Element, ElementId, ElementInputHandler,
    Entity, EntityInputHandler, FocusHandle, Focusable, GlobalElementId, IntoElement, LayoutId,
    MouseButton, MouseDownEvent, MouseMoveEvent, PaintQuad, Pixels, Render, RenderImage,
    ShapedLine, SharedString, Style, TextRun, UTF16Selection, Window, WindowBackgroundAppearance,
    WindowBounds, WindowKind, WindowOptions, div, fill, img, point, px, relative, rgb, size,
};

use crate::apps::{Catalog, CatalogGlobal, IconRequest};
use crate::native;
use crate::{
    Activate, Backspace, CloseLauncher, Copy, Cut, DeleteWordLeft, MoveLeft, MoveRight,
    MoveWordLeft, MoveWordRight, Paste, SelectAll, SelectLeft, SelectNext, SelectPrev, SelectRight,
    SelectWordLeft, SelectWordRight,
};

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
        if let Some(handle) = self.window {
            if handle
                .update(cx, |root, window, cx| {
                    // Win32 focus first (this re-shows the hidden window via SW_SHOW and
                    // calls SetForegroundWindow etc.), then GPUI focus. Reversed order
                    // would have GPUI's WM_SETFOCUS handler clobber our focus state.
                    native::focus_launcher_window(window);
                    if let Ok(view) = root.downcast::<LauncherView>() {
                        view.update(cx, |view, cx| {
                            view.reset(cx);
                            window.focus(&view.focus_handle(cx));
                        });
                    }
                })
                .is_ok()
            {
                self.visible = true;
                return;
            }

            self.window = None;
        }

        let bounds = Bounds::centered(None, size(px(LAUNCHER_WIDTH), px(LAUNCHER_HEIGHT)), cx);

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
                |window, cx| {
                    let view = cx.new(|cx| {
                        let mut view = LauncherView {
                            focus_handle: cx.focus_handle(),
                            query: String::new(),
                            selected_range: 0..0,
                            select_anchor: None,
                            last_layout: None,
                            last_bounds: None,
                            results: Vec::new(),
                            selected: 0,
                        };
                        // Populate the recents view so the first open isn't blank.
                        view.update_results(cx);
                        view
                    });

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

                    view
                },
            )
            .expect("failed to open launcher window");

        // Win32 focus first, then GPUI focus (same ordering as the re-show path above).
        let _ = handle.update(cx, |view, window, cx| {
            native::focus_launcher_window(window);
            window.focus(&view.focus_handle(cx));
        });

        self.window = Some(handle.into());
        self.visible = true;
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

    /// Dismiss the launcher from *inside* a window update (e.g. an action handler
    /// or a window-activation observer).
    ///
    /// We hide rather than destroy the window: on Windows GPUI quits the whole app
    /// when its last window closes, but we want to stay resident in the tray.
    ///
    /// We also must not re-enter `handle.update` here: action handlers and
    /// observers already run while the window has been taken out of `App::windows`,
    /// so a nested update would fail with "window not found" and silently do
    /// nothing. Instead we hide through the `Window` we already hold and update the
    /// shared `visible` flag directly.
    fn dismiss(window: &mut Window, cx: &mut App) {
        cx.global::<LauncherGlobal>()
            .clone_handle()
            .borrow_mut()
            .visible = false;
        native::hide_launcher_window(window);
    }
}

pub struct LauncherView {
    focus_handle: FocusHandle,
    query: String,
    selected_range: Range<usize>,
    /// The fixed end of an in-progress selection (mouse drag or a chain of
    /// Shift+arrow presses). `None` means the next shift-select or drag
    /// should start fresh from the current (collapsed) cursor position.
    select_anchor: Option<usize>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    /// Catalog indices of the currently-displayed results.
    results: Vec<usize>,
    /// Index into `results` of the highlighted row.
    selected: usize,
}

impl LauncherView {
    fn close(&mut self, _: &CloseLauncher, window: &mut Window, cx: &mut Context<Self>) {
        // We are inside a window update; dismiss via the held `Window`, never by
        // re-entering `LauncherState::hide`. See `LauncherState::dismiss`.
        LauncherState::dismiss(window, cx);
    }

    /// Clear the query so the launcher opens fresh on the next toggle.
    fn reset(&mut self, cx: &mut Context<Self>) {
        self.query.clear();
        self.selected_range = 0..0;
        self.select_anchor = None;
        self.update_results(cx);
        cx.notify();
    }

    /// Recompute the results list from the current query and reset the
    /// highlighted row to the top. An empty query yields the recents view.
    fn update_results(&mut self, cx: &mut App) {
        let catalog = cx.global::<CatalogGlobal>().clone_handle();
        self.results = catalog.borrow().search(&self.query);
        self.selected = 0;
    }

    fn select_next(&mut self, _: &SelectNext, _window: &mut Window, cx: &mut Context<Self>) {
        if self.results.is_empty() {
            return;
        }
        self.selected = (self.selected + 1).min(self.results.len() - 1);
        cx.notify();
    }

    fn select_prev(&mut self, _: &SelectPrev, _window: &mut Window, cx: &mut Context<Self>) {
        self.selected = self.selected.saturating_sub(1);
        cx.notify();
    }

    fn activate(&mut self, _: &Activate, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(&index) = self.results.get(self.selected) {
            self.activate_index(index, window, cx);
        }
    }

    /// Launch the catalog app at `index` and dismiss the launcher.
    fn activate_index(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let catalog = cx.global::<CatalogGlobal>().clone_handle();
        catalog.borrow_mut().launch(index);
        LauncherState::dismiss(window, cx);
    }

    // `use<>`: the returned element is fully owned (the click listener captures a
    // weak entity handle, not `cx`), so it borrows neither `self` nor `cx`.
    // Without this, edition 2024's default RPIT capture would tie the element's
    // lifetime to `cx`, and it couldn't escape the `FnMut` map closure in render.
    fn render_row(&self, row: ResultRow, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let ResultRow {
            index,
            name,
            icon,
            selected,
        } = row;

        div()
            // Stable id keyed on the catalog index so click routing is correct.
            .id(("app-row", index))
            .flex()
            .items_center()
            .gap_3()
            .px_2()
            .py_1p5()
            .rounded(px(6.0))
            .when(selected, |row| row.bg(rgb(0x2f2f34)))
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
            .child(
                div()
                    .flex_1()
                    .text_color(rgb(0xf2f2f2))
                    .child(SharedString::from(name)),
            )
            .on_click(cx.listener(move |view, _event, window, cx| {
                view.activate_index(index, window, cx);
            }))
    }

    fn select_all(&mut self, _: &SelectAll, _window: &mut Window, cx: &mut Context<Self>) {
        self.selected_range = 0..self.query.len();
        cx.notify();
    }

    fn copy(&mut self, _: &Copy, _window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            return;
        }
        let text = self.query[self.selected_range.clone()].to_string();
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            return;
        }
        let text = self.query[self.selected_range.clone()].to_string();
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.replace_text_in_range(None, "", window, cx);
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        self.replace_text_in_range(None, &text, window, cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.query.is_empty() {
            return;
        }

        if self.selected_range.is_empty() {
            self.selected_range =
                self.previous_boundary(self.cursor_offset())..self.cursor_offset();
        }

        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete_word_left(&mut self, _: &DeleteWordLeft, window: &mut Window, cx: &mut Context<Self>) {
        if self.query.is_empty() {
            return;
        }

        if self.selected_range.is_empty() {
            self.selected_range =
                self.previous_word_boundary(self.cursor_offset())..self.cursor_offset();
        }

        self.replace_text_in_range(None, "", window, cx);
    }

    fn move_left(&mut self, _: &MoveLeft, _window: &mut Window, cx: &mut Context<Self>) {
        let target = if self.selected_range.is_empty() {
            self.previous_boundary(self.cursor_offset())
        } else {
            self.selected_range.start
        };
        self.collapse_to(target);
        cx.notify();
    }

    fn move_right(&mut self, _: &MoveRight, _window: &mut Window, cx: &mut Context<Self>) {
        let target = if self.selected_range.is_empty() {
            self.next_boundary(self.cursor_offset())
        } else {
            self.selected_range.end
        };
        self.collapse_to(target);
        cx.notify();
    }

    fn move_word_left(&mut self, _: &MoveWordLeft, _window: &mut Window, cx: &mut Context<Self>) {
        let target = self.previous_word_boundary(self.cursor_offset());
        self.collapse_to(target);
        cx.notify();
    }

    fn move_word_right(&mut self, _: &MoveWordRight, _window: &mut Window, cx: &mut Context<Self>) {
        let target = self.next_word_boundary(self.cursor_offset());
        self.collapse_to(target);
        cx.notify();
    }

    fn select_left(&mut self, _: &SelectLeft, _window: &mut Window, cx: &mut Context<Self>) {
        let head = self.previous_boundary(self.selection_head());
        self.extend_selection_to(head);
        cx.notify();
    }

    fn select_right(&mut self, _: &SelectRight, _window: &mut Window, cx: &mut Context<Self>) {
        let head = self.next_boundary(self.selection_head());
        self.extend_selection_to(head);
        cx.notify();
    }

    fn select_word_left(&mut self, _: &SelectWordLeft, _window: &mut Window, cx: &mut Context<Self>) {
        let head = self.previous_word_boundary(self.selection_head());
        self.extend_selection_to(head);
        cx.notify();
    }

    fn select_word_right(
        &mut self,
        _: &SelectWordRight,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let head = self.next_word_boundary(self.selection_head());
        self.extend_selection_to(head);
        cx.notify();
    }

    /// The end of the selection that a shift/drag gesture should move: the
    /// non-anchor bound if a selection is active, otherwise the cursor.
    fn selection_head(&self) -> usize {
        match self.select_anchor {
            Some(anchor) if anchor == self.selected_range.start => self.selected_range.end,
            Some(anchor) if anchor == self.selected_range.end => self.selected_range.start,
            _ => self.cursor_offset(),
        }
    }

    fn collapse_to(&mut self, offset: usize) {
        self.selected_range = offset..offset;
        self.select_anchor = None;
    }

    /// Move the selection's head to `offset`, establishing a fresh anchor at
    /// the current (collapsed) cursor if one isn't already active.
    fn extend_selection_to(&mut self, offset: usize) {
        let anchor = self.select_anchor.unwrap_or_else(|| self.cursor_offset());
        self.selected_range = anchor.min(offset)..anchor.max(offset);
        self.select_anchor = Some(anchor);
    }

    fn cursor_offset(&self) -> usize {
        self.selected_range.end
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.query
            .char_indices()
            .rev()
            .find_map(|(index, _)| (index < offset).then_some(index))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.query
            .char_indices()
            .find_map(|(index, ch)| (index >= offset).then_some(index + ch.len_utf8()))
            .unwrap_or(self.query.len())
    }

    /// Skip back over any whitespace immediately before `offset`, then over
    /// one run of the same character class (word chars or punctuation).
    fn previous_word_boundary(&self, offset: usize) -> usize {
        let is_word = |c: char| c.is_alphanumeric() || c == '_';
        let before: Vec<(usize, char)> = self.query[..offset].char_indices().collect();
        let mut i = before.len();
        while i > 0 && before[i - 1].1.is_whitespace() {
            i -= 1;
        }
        if i > 0 {
            let word_run = is_word(before[i - 1].1);
            while i > 0 && !before[i - 1].1.is_whitespace() && is_word(before[i - 1].1) == word_run
            {
                i -= 1;
            }
        }
        before.get(i).map(|&(byte, _)| byte).unwrap_or(0)
    }

    /// Skip forward over any whitespace right after `offset`, then over one
    /// run of the same character class (word chars or punctuation).
    fn next_word_boundary(&self, offset: usize) -> usize {
        let is_word = |c: char| c.is_alphanumeric() || c == '_';
        let after: Vec<(usize, char)> = self.query[offset..]
            .char_indices()
            .map(|(index, ch)| (offset + index, ch))
            .collect();
        let mut i = 0;
        while i < after.len() && after[i].1.is_whitespace() {
            i += 1;
        }
        if i < after.len() {
            let word_run = is_word(after[i].1);
            while i < after.len() && !after[i].1.is_whitespace() && is_word(after[i].1) == word_run
            {
                i += 1;
            }
        }
        after
            .get(i)
            .map(|&(byte, _)| byte)
            .unwrap_or(self.query.len())
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;

        for ch in self.query.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }

        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;

        for ch in self.query.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }

        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
    }
}

impl EntityInputHandler for LauncherView {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.query[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: false,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        None
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {}

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .unwrap_or_else(|| self.selected_range.clone());

        self.query = format!(
            "{}{}{}",
            &self.query[..range.start],
            new_text,
            &self.query[range.end..]
        );
        let cursor = range.start + new_text.len();
        self.selected_range = cursor..cursor;
        self.select_anchor = None;
        self.update_results(cx);
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace_text_in_range(range_utf16, new_text, window, cx);

        if let Some(range) = new_selected_range_utf16 {
            let range = self.range_from_utf16(&range);
            self.selected_range =
                range.start.min(self.query.len())..range.end.min(self.query.len());
        }
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let line = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        Some(Bounds::from_corners(
            point(bounds.left() + line.x_for_index(range.start), bounds.top()),
            point(bounds.left() + line.x_for_index(range.end), bounds.bottom()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let bounds = self.last_bounds?;
        let line = self.last_layout.as_ref()?;
        let utf8_index = line.index_for_x(point.x - bounds.left())?;
        Some(self.offset_to_utf16(utf8_index))
    }
}

impl Focusable for LauncherView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// Pre-computed display data for one result row (decoupled from the catalog
/// borrow so rows can be built without holding it).
struct ResultRow {
    index: usize,
    name: String,
    icon: Option<Arc<RenderImage>>,
    selected: bool,
}

/// Decode `path`'s icon on the background thread pool, then hand the result
/// back to the catalog and repaint. Split out of `request_icon`'s caller so
/// the (slow, GDI/COM-heavy) decode never runs on the thread handling
/// keystrokes.
fn spawn_icon_load(path: PathBuf, catalog: Rc<RefCell<Catalog>>, cx: &Context<LauncherView>) {
    cx.spawn(async move |view, cx| {
        let decode_path = path.clone();
        let rendered = cx
            .background_spawn(async move { native::extract_icon_rgba(&decode_path) })
            .await
            .and_then(|icon| {
                let buffer = image::RgbaImage::from_raw(icon.width, icon.height, icon.bgra)?;
                Some(Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])))
            });

        catalog.borrow_mut().set_icon_ready(path, rendered);
        let _ = view.update(cx, |_, cx| cx.notify());
    })
    .detach();
}

impl Render for LauncherView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Collect the display data for each result row up front so we don't hold
        // a borrow of the catalog across element construction. Icons are decoded
        // off the main thread on a cache miss (GDI/COM icon extraction is too
        // slow to do synchronously without blocking keystrokes) and cached
        // inside the catalog once ready.
        let rows: Vec<ResultRow> = {
            let catalog_handle = cx.global::<CatalogGlobal>().clone_handle();
            let mut catalog = catalog_handle.borrow_mut();
            self.results
                .iter()
                .enumerate()
                .map(|(position, &index)| {
                    let name = catalog
                        .app(index)
                        .map(|app| app.name.clone())
                        .unwrap_or_default();
                    let icon = match catalog.request_icon(index) {
                        IconRequest::Ready(icon) => icon,
                        IconRequest::Loading => None,
                        IconRequest::Load(path) => {
                            spawn_icon_load(path, catalog_handle.clone(), cx);
                            None
                        }
                    };
                    ResultRow {
                        index,
                        name,
                        icon,
                        selected: position == self.selected,
                    }
                })
                .collect()
        };

        // Neutral, untinted dark palette (Raycast-like): a single card with a
        // search row, a full-width hairline divider, and a results list below.
        div()
            .key_context("Launcher")
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(Self::close))
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_prev))
            .on_action(cx.listener(Self::activate))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::delete_word_left))
            .on_action(cx.listener(Self::move_left))
            .on_action(cx.listener(Self::move_right))
            .on_action(cx.listener(Self::move_word_left))
            .on_action(cx.listener(Self::move_word_right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_word_left))
            .on_action(cx.listener(Self::select_word_right))
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x1c1c1e))
            .text_color(rgb(0xf2f2f2))
            .border_1()
            .border_color(rgb(0x343437))
            .shadow_lg()
            // Search row
            .child(
                div()
                    .h(px(44.0))
                    .w_full()
                    .px_4()
                    .flex()
                    .items_center()
                    .text_lg()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|view, event: &MouseDownEvent, window, cx| {
                            // `character_index_for_point` answers in UTF-16 offsets (the
                            // IME contract); `selected_range` is byte-offset based.
                            if let Some(utf16_index) =
                                view.character_index_for_point(event.position, window, cx)
                            {
                                let index = view.offset_from_utf16(utf16_index);
                                view.select_anchor = Some(index);
                                view.selected_range = index..index;
                                cx.notify();
                            }
                        }),
                    )
                    .on_mouse_move(cx.listener(|view, event: &MouseMoveEvent, window, cx| {
                        if !event.dragging() {
                            return;
                        }
                        let Some(anchor) = view.select_anchor else {
                            return;
                        };
                        if let Some(utf16_index) =
                            view.character_index_for_point(event.position, window, cx)
                        {
                            let index = view.offset_from_utf16(utf16_index);
                            view.selected_range = anchor.min(index)..anchor.max(index);
                            cx.notify();
                        }
                    }))
                    .child(LauncherInputElement { input: cx.entity() }),
            )
            // Divider
            .child(div().h(px(1.0)).w_full().bg(rgb(0x2c2c2e)))
            // Results list
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .p_1p5()
                    .text_sm()
                    .when(rows.is_empty(), |list| {
                        list.child(
                            div()
                                .px_2()
                                .py_1p5()
                                .text_color(rgb(0x7c7c80))
                                .child("No results"),
                        )
                    })
                    .children(rows.into_iter().map(|row| self.render_row(row, cx))),
            )
    }
}

struct LauncherInputElement {
    input: Entity<LauncherView>,
}

struct InputPrepaintState {
    line: ShapedLine,
    cursor: PaintQuad,
    selection: Option<PaintQuad>,
}

impl IntoElement for LauncherInputElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for LauncherInputElement {
    type RequestLayoutState = ();
    type PrepaintState = InputPrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let display_text: SharedString = if input.query.is_empty() {
            "Search apps, files, commands...".into()
        } else {
            input.query.clone().into()
        };
        let text_color = if input.query.is_empty() {
            rgb(0x767676)
        } else {
            rgb(0xf2f2f2)
        };

        let style = window.text_style();
        let run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: text_color.into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };

        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display_text, font_size, &[run], None);
        let cursor_x = if input.query.is_empty() {
            px(0.0)
        } else {
            line.x_for_index(input.cursor_offset())
        };
        let cursor = fill(
            Bounds::new(
                point(bounds.left() + cursor_x, bounds.top()),
                size(px(2.0), bounds.bottom() - bounds.top()),
            ),
            rgb(0xd0d0d0),
        );

        let selection = (!input.selected_range.is_empty()).then(|| {
            let start_x = line.x_for_index(input.selected_range.start);
            let end_x = line.x_for_index(input.selected_range.end);
            fill(
                Bounds::new(
                    point(bounds.left() + start_x, bounds.top()),
                    size(end_x - start_x, bounds.bottom() - bounds.top()),
                ),
                rgb(0x3b5170),
            )
        });

        InputPrepaintState {
            line,
            cursor,
            selection,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );

        if let Some(selection) = prepaint.selection.clone() {
            window.paint_quad(selection);
        }

        prepaint
            .line
            .paint(bounds.origin, window.line_height(), window, cx)
            .unwrap();

        if focus_handle.is_focused(window) && prepaint.selection.is_none() {
            window.paint_quad(prepaint.cursor.clone());
        }

        self.input.update(cx, |input, _cx| {
            input.last_layout = Some(prepaint.line.clone());
            input.last_bounds = Some(bounds);
        });
    }
}
