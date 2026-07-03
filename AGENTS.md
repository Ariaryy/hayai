# AGENTS.md — Hayai

Guidance for AI agents (and humans) working in this repo. Read this before touching
window/focus code — GPUI has sharp edges that already caused one shipped bug.

## What Hayai is

A Raycast-style launcher for Windows, optimized for **high performance and minimum
RAM**. Rust + [GPUI](https://www.gpui.rs/) (Zed's GUI framework). A global hotkey
(`Alt+Space`) toggles a frameless, centered, always-on-top search window. A tray icon
provides toggle (left-click) and quit (right-click).

Keep the memory/perf goal in mind for every dependency and allocation decision: prefer
zero-copy, lazy work, and OS-provided indexes over re-implementing heavy machinery.

## Architecture

| File | Responsibility |
|------|----------------|
| `src/main.rs` | App entry. Starts the native runtime thread, installs the global launcher state + keybindings, and runs the GPUI event loop. `poll_native_commands` bridges the native thread → GPUI via an `mpsc` channel polled on a GPUI timer. |
| `src/launcher.rs` | `LauncherState` (owns the window handle, show/hide/toggle) and `LauncherView` (the GPUI view: text input element, results list, rendering, key actions). |
| `src/apps.rs` | `Catalog` (behind the `CatalogGlobal` global): Start Menu `.lnk` scan, fuzzy search, lazy+cached icon decode, recents. The app-search vertical slice. |
| `src/native/mod.rs` | `NativeCommand` enum + `IconImage` + platform dispatch. Non-Windows builds get stubs so it still compiles. |
| `src/native/windows.rs` | Win32: hidden message-only window, `RegisterHotKey`, tray icon, `focus_launcher_window` (foreground/focus dance), `extract_icon_rgba` (HICON→BGRA DIB), `launch_path` (`ShellExecuteW`). |
| `src/commands.rs` | `CommandProvider` trait + `CommandItem`/`CommandAction` model. Not yet wired into the view (apps use a bespoke path for now). |
| `src/plugins.rs` | `PluginRegistry` over `CommandProvider`. Not yet wired in. |

### Threading / control flow
- The **native runtime runs on its own OS thread** with a classic Win32 message loop
  (`GetMessageW`). It cannot touch GPUI state directly.
- It communicates by sending `NativeCommand`s through an `mpsc::Sender`. GPUI drains the
  receiver every 40 ms inside a `cx.spawn` task (`poll_native_commands`).
- `LauncherState` lives in an `Rc<RefCell<…>>` exposed as a GPUI `Global`
  (`LauncherGlobal`). Reach it with `cx.global::<LauncherGlobal>().clone_handle()`.

## GPUI pitfalls (learned the hard way — do not repeat)

### 0. Closing the last window QUITS the app — hide, don't destroy
On Windows, GPUI calls `PostQuitMessage(0)` the moment its window count hits zero
(`WindowsPlatform::close_one_window` → `PostQuitMessage` when the list is empty). There
is **no setting** to keep the process alive. So `window.remove_window()` on our only
window terminates the whole app instead of leaving it in the tray.

A tray launcher must therefore **never destroy its window**. We `SW_HIDE` the OS window
(`native::hide_launcher_window`) and keep the single GPUI window alive for the next
toggle. `LauncherState` tracks a `visible: bool` (not `window.is_some()`) to decide
toggle direction. Re-showing goes through `native::focus_launcher_window`, which already
does `SW_SHOW` + foreground + focus. Bonus: reusing the window is cheaper than recreating
it every toggle, which suits the low-RAM goal.

Corollary — `WM_ACTIVATE` is dispatched **asynchronously**: GPUI's
`handle_activate_msg` does `executor.spawn(...).detach()`, so the activation observer
does *not* fire re-entrantly during `SW_HIDE`. That's why setting `visible = false`
*before* hiding is enough to make the observer no-op, and why it's safe even though the
toggle path holds a `borrow_mut` on `LauncherState` across the hide.

### 1. Never re-enter `WindowHandle::update` while already inside a window update
This was the **root cause of the "Escape won't close the launcher" bug.**

`App::update_window_id` *takes* the window out of `cx.windows` (`get_mut(id)?.take()?`)
for the duration of the update. Any **nested** `handle.update(cx, …)` on the *same*
window finds `None` in the slot and returns `Err("window not found")`. If you wrote
`let _ = handle.update(...)`, that error is **silently swallowed** and your intended
side effect (e.g. removing the window) never happens.

Action handlers (`on_action` listeners) and `observe_window_activation` callbacks
**already run inside a window update** and are handed a `&mut Window`. So:

- ✅ To dismiss from an action/observer, call `window.remove_window()` on the `Window`
  you were given. See `LauncherState::dismiss`.
- ❌ Do **not** route those paths through `LauncherState::hide`, which calls
  `handle.update`. `hide` is only safe from contexts holding a bare `&mut App` that is
  **not** mid-window-update — e.g. the `poll_native_commands` task's `cx.update`.

`remove_window()` only sets `window.removed = true`; the actual teardown
(removing from `cx.windows`/`window_handles`, firing close observers) happens when the
enclosing `update_window_id` unwinds. It is safe and idempotent.

### 2. Close on focus loss needs an explicit observer
There is no implicit "click outside closes it." Register
`cx.observe_window_activation(window, …)` when building the root view and dismiss when
`!window.is_window_active()`. The observer fires on **both** activation and
deactivation — always gate on `is_window_active()`, never assume "fired ==
deactivated". It *also* fires when we hide the window ourselves (`SW_HIDE` deactivates
it), so additionally gate on the `visible` flag to avoid re-dismissing/recursing.
`activate()` from `SubscriberSet::insert` only flips an active flag; it does **not**
invoke your callback, so it's safe to register even while holding a `RefCell` borrow of
`LauncherState`.

### 3. Keybindings vs. action handlers
- `cx.bind_keys` maps a key → an `Action`. The action must still be *handled* somewhere.
- Prefer handling actions on the **focused view** (`div().on_action(cx.listener(...))`)
  over a global `cx.on_action`. A global handler runs with a bare `&mut App` that is
  typically mid-window-update, so it hits pitfall #1 if it tries to touch the window.
- Scope bindings to the view's `key_context` (here `Some("Launcher")`, matching
  `div().key_context("Launcher")`) so they only fire when the launcher is focused.

### 4. Win32 focus ordering
In `focus_launcher_window`, the Win32 foreground/focus calls must run **before** GPUI's
`window.focus(...)`. Reversed, GPUI's `WM_SETFOCUS` handling clobbers the focus state.
`AllowSetForegroundWindow(ASFW_ANY)` must be called from the thread that received
`WM_HOTKEY` while we still hold the foreground grant — that's why it lives in the Win32
`window_proc`, not in the GPUI side.

### 5. `RefCell` borrow discipline
`LauncherState` is shared via `Rc<RefCell>`. `toggle` borrows it mutably and then calls
`show`/`hide`, so anything reachable from those paths must **not** try to borrow the
same `RefCell` again, or you'll panic with "already borrowed". `dismiss` borrows only
briefly (`borrow_mut().window = None`) and is only called from observer/action contexts,
which do not hold an outstanding borrow.

### 6. Icons + results list (app search)
- **GDI handle hygiene.** `extract_icon_rgba` creates several GDI objects
  (`HICON` from the shell, the two `ICONINFO` bitmaps, a DC). Every one must be
  freed on **all** paths or we leak kernel objects on every keystroke's render.
  The code deletes the color/mask bitmaps after the inner closure returns and
  `DestroyIcon`s the shell icon after use — keep that discipline if you edit it.
- **Icons are decoded lazily during `render` and cached** in `Catalog.icons`
  (keyed by path; `None` = "tried, failed" so we don't re-hit the shell). Icon
  extraction runs on the **main/UI thread** — fine for a handful of visible rows,
  but if it ever janks, move it to a background executor and fill in async.
- **Don't hold the `CatalogGlobal` `RefCell` borrow across element building.**
  `render` collects a `Vec<ResultRow>` (name + decoded icon) inside a scoped
  `borrow_mut()`, then drops the borrow before constructing the list elements
  and their click listeners. A listener that later calls back into the catalog
  (launch) would panic "already borrowed" if the render borrow were still live.
- Result-list keys are actions bound in the `Launcher` context (`down`→
  `SelectNext`, `up`→`SelectPrev`, `enter`→`Activate`) and handled on the view
  (pitfall #3). `Enter`/click go through `activate_index`, which launches then
  `LauncherState::dismiss`es via the held `Window` (pitfall #1).

## Conventions
- Edition 2024.
- Keep platform-specific code behind `#[cfg(windows)]` with non-Windows stubs in
  `native/mod.rs` so the crate keeps compiling cross-platform.
- Release builds use `windows_subsystem = "windows"` (no console window). Don't rely on
  `println!`/`eprintln!` being visible in release; use them only as dev diagnostics.
- Comment the *why* for any Win32 / GPUI ordering or re-entrancy subtlety — these are not
  obvious and will be "cleaned up" by a future agent otherwise.

## Build / run
```sh
cargo build           # dev
cargo build --release # no console window, optimized
cargo run             # launches; press Alt+Space to toggle, tray right-click to quit
```
There are no automated tests yet. Manual verification for window behavior:
1. `Alt+Space` opens the launcher centered and focused.
2. `Esc` hides it **back to the tray** (process keeps running — verify the tray icon is
   still there and `Alt+Space` reopens it).
3. Clicking another window (focus loss) hides it back to the tray.
4. `Alt+Space` while open hides it (toggle).
5. Reopening shows a fresh, empty query (see `LauncherView::reset`).
6. Tray left-click toggles; right-click quits the process entirely.
7. With an empty query the list shows recent/known apps **with icons**; typing
   filters them fuzzily. `Up`/`Down` move the highlight, `Enter` or a click
   launches the app and hides the launcher; the launched app then appears at the
   top of the recents on the next open.

## Roadmap
See `PLANS.md` for the feature roadmap (app search/launch, file search, calculator/unit
& currency conversion, clipboard history).
