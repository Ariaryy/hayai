## What Hayai is

A Raycast-style launcher for Windows, optimized for **high performance and minimum
RAM**. Rust + [GPUI](https://www.gpui.rs/) (Zed's GUI framework) + gpui-component
(longbridge's widget library on top of GPUI). A global hotkey (`Alt+Space`) toggles a
frameless, centered, always-on-top search window. A tray icon provides toggle
(left-click) and quit (right-click).

Keep the memory/perf goal in mind for every dependency and allocation decision: prefer
zero-copy, lazy work, and OS-provided indexes over re-implementing heavy machinery.
**Measure idle + active RSS before/after any dependency or architecture change** — the
gpui-component migration alone cost real memory (see "The gpui-component migration"
below); don't add more without checking the number.

## Architecture

| File | Responsibility |
|------|----------------|
| `src/main.rs` | App entry. Boots via `gpui_platform::application()` (not `gpui::Application::new()` — see below), initializes gpui-component + a custom dark theme, starts the native runtime thread, installs the global launcher state, and runs the GPUI event loop. `poll_native_commands` bridges the native thread → GPUI via an `mpsc` channel polled on a GPUI background-executor timer. |
| `src/launcher.rs` | `LauncherState` (owns the window handle, show/hide/toggle) and `LauncherRoot` (the window's root view: a hand-built search `Input` + gpui-component `List`/`ListState<AppListDelegate>` + a pinned footer). See "gpui-component integration" below for why the input is hand-built instead of using `List`'s built-in one. |
| `src/apps.rs` | `Catalog` (behind the `CatalogGlobal` global): Start Menu `.lnk` scan + `shell:AppsFolder` enumeration (packaged/Store apps), fuzzy search, lazy+cached **background-thread** icon decode, recents. The app-search vertical slice. |
| `src/native/mod.rs` | `NativeCommand` enum + `IconImage` + platform dispatch. Non-Windows builds get stubs so it still compiles. |
| `src/native/windows.rs` | Win32: hidden message-only window, `RegisterHotKey`, tray icon, `focus_launcher_window` (foreground/focus dance), `extract_icon_rgba` (icon extraction with shortcut/PIDL handling — see below), `list_apps_folder` (raw COM shell-namespace enumeration), `launch_path` (`ShellExecuteW`). |
| `src/commands.rs` | `CommandProvider` trait + `CommandItem`/`CommandAction` model. |
| `src/plugins.rs` | `PluginRegistry` over `CommandProvider`, dispatched from `LauncherRoot`'s search path (auto-claim NL detection, then keyword routing). |
| `src/files.rs` | `FileSearchProvider` — Scry Search (`scryd`)-backed file search (`native::scry_query`), debounced via `CommandProvider::wants_debounce`/`background_search`. |
| `src/calc/` | `CalcProvider`, claimed via the ` = ` keyword or NL auto-claim (`is_candidate` cheap gate + each evaluator's own parse). Evaluator chain in `calc::evaluate`, tried in order: `units` (dimension/temperature conversion), `currency` (live FX rates, symbol/code forms, regional-default bare amounts), `bases` (hex/dec/oct/bin), `time` (clock arithmetic, timezone conversion, relative-date arithmetic — hand-rolled civil-calendar math, no `chrono`), `arithmetic` (plain expressions, recursive-descent). `grammar::parse_amount` is the shared amount parser (handles the `k`-thousands shorthand, e.g. `"5k"` = 5000) used by `units`/`currency`/the conversion half of `grammar::parse_conversion` — deliberately *not* used by `arithmetic`, so `k` only works in conversion contexts, never in plain calculator expressions. |
| `src/startup.rs` | `register()`/`unregister()` — writes/deletes the `HKCU\...\Run` entry for launch-on-login, via `winreg` (not raw FFI, matching the sibling `Panora` project's established pattern). Called from `main.rs`'s Velopack install/uninstall fast-callback hooks, never on a normal run. |

### Threading / control flow
- The **native runtime runs on its own OS thread** with a classic Win32 message loop
  (`GetMessageW`). It cannot touch GPUI state directly.
- It communicates by sending `NativeCommand`s through an `mpsc::Sender`. GPUI drains the
  receiver every 40 ms inside a `cx.spawn` task (`poll_native_commands`), sleeping via
  `cx.background_executor().timer(duration)` (not a top-level `Timer` type — see below).
- `LauncherState` lives in an `Rc<RefCell<…>>` exposed as a GPUI `Global`
  (`LauncherGlobal`). Reach it with `cx.global::<LauncherGlobal>().clone_handle()`.
- Icon decoding runs on gpui's **background thread pool** (`cx.spawn` +
  `cx.background_spawn`), not inline in `render`. `Catalog::request_icon` returns an
  `IconRequest::{Ready, Loading, Load}` tri-state; a `Load(path)` result means the caller
  must kick off the background decode and call `Catalog::set_icon_ready` + `cx.notify()`
  when done. Doing this synchronously in `render` was the original design and it visibly
  blocked keystrokes — GDI/COM icon extraction is too slow for the UI thread.

## The gpui-component migration (2026-07-03)

We migrated from crates.io `gpui = "0.2.2"` to git-sourced `gpui` + `gpui_platform` +
`gpui-component` (all from the same `zed-industries/zed` rev, since Cargo only treats
two git dependencies as *the same crate* when their `(url, rev)` match exactly — mixing
a crates.io release with gpui-component's git-sourced `gpui` produces two incompatible
copies of every GPUI type).

**Cost, measured**: idle/active RSS went from ~40MB (lean 0.2.2) to ~45–57MB in normal
use. This was an explicit, informed tradeoff (the user prioritized the better-maintained
input/list widgets over the last ~15MB), not an oversight — but it means **this budget
has no more slack**; don't add further heavy dependencies without checking RSS again.

**Toolchain**: this rev needs rustc **1.89+** (`rustup update stable` if you hit
`smol_str@0.3.6 requires rustc 1.89` or similar during `cargo build`).

**Pinning discipline**: `gpui` and `gpui_platform` are pinned to an explicit `rev =` in
`Cargo.toml`, matching whatever gpui-component's own `Cargo.lock`/`Cargo.toml` pins for
its internal `gpui` dependency (check
`https://raw.githubusercontent.com/longbridge/gpui-component/main/Cargo.toml`). Bumping
one without the other reintroduces the "two incompatible gpui copies" problem. Bump both
together, deliberately — never let `cargo update` float the rev.

### API differences vs. crates.io gpui 0.2.2
This rev is meaningfully newer than 0.2.2. Things that changed (found by trial/error —
check the vendored source under `~/.cargo/git/checkouts/zed-*/*/crates/gpui` before
assuming an 0.2.2-era API still exists):
- `gpui::Application::new()` doesn't exist — use `gpui_platform::application()` (it
  returns a `gpui::Application`, wired to the right platform backend).
- `gpui::Timer` doesn't exist — use `cx.background_executor().timer(duration).await`.
- `Window::focus(&mut self, handle: &FocusHandle, cx: &mut App)` now takes `cx` too
  (was `focus(&handle)` in 0.2.2).
- `ShapedLine::paint` gained two params: `paint(origin, line_height, align: TextAlign,
  align_width: Option<Pixels>, window, cx)`.

### Action propagation: `cx.propagate()`, not the reverse
**GPUI action handlers consume/stop an action by default; they must call
`cx.propagate()` explicitly to let it keep bubbling to ancestor `on_action` handlers.**
This is the opposite of what you'd guess from "returning early = doing nothing", and it
is *the* mechanism that makes `LauncherRoot`'s hand-built keyboard nav possible: reading
`gpui_component`'s own `input/state.rs` and `input/movement.rs` confirms
`InputState::escape`/`InputState::enter` both call `cx.propagate()` in their default
(single-line, no context menu, no inline completion) branches. Separately,
`MoveUp`/`MoveDown` are conditionally *not even registered* as `on_action` handlers on
the `Input` element unless `state.mode.is_multi_line()` — so for our single-line search
box, those actions never get handled at the input at all and bubble straight to
whatever ancestor listens for them. Both facts combined are why binding
`gpui_component::input::{MoveUp, MoveDown, Escape}` action types (plus subscribing to
`InputEvent::PressEnter`) on `LauncherRoot`'s root `div` — an ancestor of the focused
search input — reliably intercepts Up/Down/Enter/Escape without fighting the library.

### `List`'s built-in search input can't be customized — hence the hand-built one
`gpui_component::list::List` can render its own search row (`ListState::searchable(true)`),
but that row's `Input` is built with a hardcoded `.prefix(Icon::new(IconName::Search)…)`
and `.cleanable(true)`, with no public way to remove either — and their internal padding
math has a real quirk (the clear button's outer container adds `pr(size.input_px())`
*in addition to* the button's own width, so at `Size::Large` there's a visible ~16px
dead gap after the X). There is also no way to control the search row's height beyond
`Size`'s four fixed presets, and rows can visibly clip descenders (e.g. "g") at some
sizes.

We now build the search `Input`/`InputState` ourselves (`LauncherRoot`) instead, with
`searchable(false)` on the `ListState`. The catch: `List`'s Up/Down/Enter/Escape nav
*only* fires because its search input is nested inside `List`'s own tracked-focus
element tree — bubbling only travels up the ancestor chain of the *focused* element, so
a hand-built input that's a *sibling* of `List` (not nested inside it) never reaches
`List`'s internal nav handlers, which are `pub(super)`/private anyway. `LauncherRoot`
re-implements this nav itself using `ListState`'s public API
(`selected_index()`/`set_selected_index()`/`scroll_to_selected_item()`/`delegate()`/
`delegate_mut()`) — see the "Action propagation" section above for how the keystrokes
actually reach it. This is a deliberate, working design, not a stopgap; don't try to
switch back to `searchable(true)` to "simplify" without re-reading why it was dropped.

### Theme customization
`gpui_component::Theme::change(ThemeMode::Dark, None, cx)` picks a mode, but the default
palette is blue-accented. To override specific colors (`Theme::global_mut(cx)`, which
derefs to `ThemeColor`, e.g. `theme.list_active = …`), you **must also** regenerate
`theme.tokens = ThemeTokens::from(&theme.colors)` afterward — `tokens` is a separate,
pre-resolved snapshot that hot paths like `ListItem`'s hover fill actually read
(`cx.theme().tokens.list_hover`, not `cx.theme().list_hover`), and it does not
auto-resync when you mutate `colors` directly.

### Packaged/Store apps: `shell:AppsFolder`, not just `.lnk`
`.lnk`-only scanning misses MSIX/UWP/Store apps entirely (they have no shortcut file on
disk) — e.g. NanaZip. `native::list_apps_folder` enumerates the virtual `shell:AppsFolder`
namespace via raw COM (`IShellFolder`/`IEnumIDList`, addressed by absolute vtable slot
index rather than full interface structs, matching the existing `IShellLinkW` pattern in
the same file) — this is exactly what Explorer's own Start Menu search reads, and it's a
*superset* of the `.lnk` scan, so `Catalog::scan` runs it second and only adds names not
already seen. Packaged apps resolve to a bare AUMID string (e.g.
`Publisher.App_hash!App`), not a real path — `ShellExecuteW` needs the `shell:AppsFolder\`
prefix to resolve it (a raw AUMID alone fails silently), and `SHGetFileInfoW` needs the
PIDL form (`SHGFI_PIDL`) rather than the string form to extract its icon correctly.

### Icon extraction priority
Squirrel/Electron-style installers (Discord, WhatsApp, most Chrome PWA shortcuts) point
their `.lnk` at a small updater/proxy exe with no icon of its own, and set the real icon
separately via the shortcut's `IconLocation`. `extract_icon_rgba` therefore tries, in
order: (1) the shortcut's explicit `IconLocation` (via `IShellLinkW::GetIconLocation` +
`ExtractIconExW`), (2) the resolved target exe/file (avoids `SHGetFileInfoW`'s
shortcut-arrow overlay, which is baked into the returned bitmap with no flag to disable
it), (3) the raw `.lnk` path as a last resort. Don't collapse this back to a single
`SHGetFileInfoW(path)` call — that's the Discord/WhatsApp icon regression this fixed.

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

Action handlers (`on_action` listeners), list-delegate callbacks (`confirm`/`cancel`),
and `observe_window_activation` callbacks **already run inside a window update** and are
handed a `&mut Window`. So:

- ✅ To dismiss from an action/delegate callback/observer, call
  `LauncherState::dismiss(window, cx)` using the `Window` you were given.
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
- `cx.bind_keys` maps a key → an `Action`. The action must still be *handled* somewhere,
  and (per the propagation section above) that handler must call `cx.propagate()` if it
  wants an ancestor to also see it.
- Prefer handling actions on the **focused view's ancestor**
  (`div().on_action(cx.listener(...))`) over a global `cx.on_action`. A global handler
  runs with a bare `&mut App` that is typically mid-window-update, so it hits pitfall #1
  if it tries to touch the window.
- We currently reuse `gpui_component::input`'s own action types (`MoveUp`, `MoveDown`,
  `Escape`) rather than defining our own — see "Action propagation" above for why that
  works without extra `KeyBinding`/`key_context` registration on our side.

### Context action panel: input routing and paint order

`Ctrl+K` opens the selected-result action panel. While it is open, Up/Down
must change the panel's action selection and Enter must execute that action;
never let those keys fall through to the results list. Render the absolute
panel as the final child of `LauncherRoot`'s root element: GPUI paints later
siblings on top, while putting it before `List` leaves it behind the selected
row.

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
briefly (`borrow_mut().visible = false`) and is only called from delegate/observer/action
contexts, which do not hold an outstanding borrow. Same discipline applies to
`CatalogGlobal`'s `RefCell`: `AppListDelegate::render_item` borrows it briefly per row
and must drop the borrow before spawning a background icon load that will later borrow
it again from a different call stack.

### 6. Icons + results list (app search)
- **GDI handle hygiene.** `extract_icon_rgba` and friends create several GDI/COM objects
  per call (`HICON`s, `ICONINFO` bitmaps, a DC, COM interface pointers, PIDLs). Every one
  must be released on **all** paths (including early returns) or we leak kernel objects
  on every icon decode. Keep that discipline if you edit `native/windows.rs`.
- Icons decode on a **background thread** (see "Threading / control flow" above), not
  inline in `render` — that was the original design and it visibly blocked keystrokes.
- The results list has **no cap** (`Catalog::search`/`recent_indices` return the whole
  matching/catalog set) — `gpui_component::list::List` is virtualized, so this is cheap;
  don't reintroduce a `MAX_RESULTS`-style truncation to "optimize" this.

## Conventions
- Edition 2024.
- Keep platform-specific code behind `#[cfg(windows)]` with non-Windows stubs in
  `native/mod.rs` so the crate keeps compiling cross-platform.
- Release builds use `windows_subsystem = "windows"` (no console window). Don't rely on
  `println!`/`eprintln!` being visible in release; use them only as dev diagnostics.
- Comment the *why* for any Win32 / GPUI ordering, re-entrancy, or action-propagation
  subtlety — these are not obvious and will be "cleaned up" by a future agent otherwise.
- When investigating an unfamiliar gpui/gpui-component API on this rev, **read the
  vendored source** (`~/.cargo/git/checkouts/zed-*/*/crates/gpui`,
  `~/.cargo/git/checkouts/gpui-component-*/*/crates/ui/src`) rather than assuming
  crates.io-era docs or memory apply — this rev has diverged meaningfully (see the
  migration section above).

## Build / run
```sh
cargo build           # dev — first build after a rev bump can take several minutes
cargo build --release # no console window, optimized
cargo run             # launches; press Alt+Space to toggle, tray right-click to quit
```
There are no automated tests yet. Manual verification for window behavior:
1. `Alt+Space` opens the launcher centered and focused.
2. `Esc` hides it **back to the tray** (process keeps running — verify the tray icon is
   still there and `Alt+Space` reopens it).
3. Clicking another window (focus loss) hides it back to the tray.
4. `Alt+Space` while open hides it (toggle).
5. Reopening shows a fresh, empty query **and** resets the results to match (both the
   input's `set_value` and the delegate's `perform_search("")` must run on show — setting
   the input alone doesn't emit a change event, which was a shipped bug).
6. Tray left-click toggles; right-click quits the process entirely.
7. With an empty query the list shows recent/known apps (then the rest of the catalog
   alphabetically) **with icons** (loaded async — expect a brief blank icon on first
   sight of a new app); typing filters them fuzzily, including packaged/Store apps.
   `Up`/`Down` move the highlight and auto-scroll, `Enter` or a click launches the app
   and hides the launcher; the launched app then appears at the top of the recents on
   the next open.

## Roadmap
Built: app search/launch, file search (scry daemon), calculator (arithmetic, unit/temperature
conversion, currency conversion with live rates and a regional default, hex/dec/oct/bin base
conversion, clock/timezone/relative-date arithmetic), Up/Down search history recall, a Velopack-based
installer (`scripts/build-installer.ps1`) that registers/unregisters launch-on-login via `src/startup.rs`.
Also built: a contextual action panel for app/file results (`Ctrl+K`, with
open/reveal/copy-path/run-as-admin actions). Not yet built: clipboard history
(text + images, `rusqlite` dependency already added) and a broader plugin system
beyond the current built-in providers.
