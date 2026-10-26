# GPUI and Windows invariants

Read the relevant sections before changing launcher lifecycle, input, layout,
scrolling, themes, or shell icons. These constraints supplement the performance
contract in [AGENTS.md](../../AGENTS.md); API details come from the dependency
revision selected by `Cargo.lock`.

## Window lifecycle and focus

- Preserve the launcher GPUI window. Closing the last window quits the Windows
  event loop; hiding keeps hotkey and tray behavior alive.
- `LauncherState::hide` is for callers outside a window update.
  Actions, delegates, and activation observers already have `&mut Window`:
  use `LauncherState::dismiss` there. A nested update of the same window fails
  because GPUI temporarily removes it from the app's window map.
- Set logical visibility false before hiding. Activation is asynchronous;
  account for both the logical visibility flag and Win32 foreground ownership
  so an old deactivation cannot dismiss a window that has regained focus.
- Apply Win32 foreground/focus operations before GPUI input focus.
  `AllowSetForegroundWindow` belongs on the thread receiving `WM_HOTKEY`,
  while that thread holds the foreground grant. The approval-return path also
  handles foreground ownership after UAC; keep its temporary input attachment
  scoped to that activation.
- Reset input **and** provider results: `InputState::set_value` does not emit
  `InputEvent::Change`. Reset file/command mode, folder scope, transient panels,
  selection, scroll offset, and the pending virtual-list target together.
  Changing the query generation must invalidate obsolete background results.
- Complete reset rendering before showing the reused window. A synchronous
  forced Windows paint must run outside App, Window, entity, and shared-state
  borrows: GPUI's platform callback re-enters through `AsyncApp::try_borrow_mut`.
  `App::defer` still holds the App borrow and is insufficient. The current
  foreground-task path submits the reset frame, calls `DwmFlush`, then shows
  and focuses the window. DWM transitions are disabled for this window.
  Preserve this ordering; compositor synchronization can cost a refresh wait.
  The forced-paint message is specific to the pinned Windows backend; recheck
  it when upgrading GPUI.

For lifecycle changes, test reopening from apps, calculator, completed and
pending file searches, folder scope, and detail/action panels. Exercise hotkey,
focus loss, and Escape dismissal (Escape may first back out of a mode or panel).
Verify that the process remains in the tray, headings are visible, late results
stay discarded, and the **first visible frame** contains the reset view.
The ignored `wheel_scroll_gui_smoke` test covers state and prepaint ordering;
visual confirmation remains necessary for compositor flashes:

```powershell
cargo test wheel_scroll_gui_smoke -- --ignored --test-threads=1 --nocapture
```

## Input and action panels

The search input is built separately from `List` with `searchable(false)`.
The built-in search row fixes its prefix, clear-button layout, and sizing;
switching back loses the launcher's custom layout. Because the focused input
is a sibling of the list, its actions cannot bubble into the list's private
navigation handlers. `LauncherRoot` handles navigation through public list APIs.

GPUI handlers consume actions by default. Call `cx.propagate()` only when an
ancestor should handle the action. Bind launcher actions on an ancestor of the
focused input. Single-line input leaves Up/Down unhandled; Enter is observed
through `InputEvent::PressEnter` and Escape uses the input's action type.

While the action panel is open, route Up/Down and Enter to the panel. Paint it
as the final root child so later siblings do not cover it. The Open with
submenu has its own selection and returns to its parent before closing.

## Virtual lists and scrolling

- Result sets remain uncapped; virtualization limits rendering work.
- Arrow navigation stops at list boundaries. When selecting row zero, reveal
  its section heading, not just the first item. The pinned list flattens each
  nonempty section into header, rows, and footer entries, even with a
  zero-height footer. Preserve the header-target override during prepaint.
- Call `request_animation_frame` only during render/layout/prepaint/paint.
  Calling it from an input event or next-frame callback can panic on this rev.
  Input updates animation state and notifies; render advances it and requests
  another frame only while motion remains.
- Touchpad pixel input already has inertia. Preserve native precision motion
  and the handoff that prevents its old `Moved` tail from canceling a new
  wheel animation. A fresh `Started` gesture takes over immediately.
- Cancel wheel animation when keyboard navigation, query/results, visibility,
  or an action panel changes the scroll context.

## Themes and framework setup

Boot through `gpui_platform::application()` and initialize gpui-component before
using its widgets. After changing theme colors, regenerate
`theme.tokens = ThemeTokens::from(&theme.colors)`: widgets may read this
separate snapshot rather than the mutable color fields.

For framework upgrades, inspect the **locked gpui-component checkout's** GPUI
dependency and update Hayai's GPUI source/revision to match. Verify that Cargo
resolves one compatible GPUI type universe. The latest upstream main manifest
is not authoritative for an older locked checkout.

## Shell discovery and icons

Enumerate `shell:AppsFolder` alongside Start Menu shortcuts to include packaged
apps. A packaged app's bare AUMID needs the `shell:AppsFolder\` prefix for shell
launching and PIDL handling for icon lookup.

Preserve shortcut icon priority: explicit `IconLocation`, resolved target,
then the shortcut itself. Updater/proxy executables often lack the real icon;
reading the shortcut directly can bake in an arrow overlay. Use the current
size-aware extraction helper rather than hardcoding an obsolete API sequence.

Decode and resize shell icons on the background pool. Drop catalog borrows
before starting callbacks. Keep cache retention and speculative pending loads
bounded, preserve visible-row priority, and drop renderer images on eviction
or DPI replacement. See [icon measurements](../icon-cache-memory.md) for the
workload and the limits of CPU-only results.

Release every owned HICON, bitmap, DC, COM reference, and PIDL on success and
early-return paths, using the API appropriate to that resource's ownership.
