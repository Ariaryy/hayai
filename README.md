# Hayai

A [Raycast](https://www.raycast.com/)-inspired application launcher for
Windows, built to be fast and light: instant fuzzy search over your installed
apps, a global hotkey, and a minimal RAM footprint.

> 速い (hayai) — Japanese for "fast".

![platform](https://img.shields.io/badge/platform-Windows-blue)
![license](https://img.shields.io/badge/license-MIT-blue)
![powered by GPUI](https://img.shields.io/badge/powered%20by-GPUI-8A2BE2)

## Features

- **Global hotkey** (`Alt+Space`) opens a frameless, centered, always-on-top
  search window — repositions itself onto whichever monitor your cursor is on.
- **Fuzzy search** over Start Menu shortcuts *and* packaged/Store apps (via
  `shell:AppsFolder`), with prefix / word-boundary / consecutive-match scoring.
- **Recents-first**: an empty query shows your most recently launched apps
  before the rest of the catalog.
- **Sharp icons**: 48px Hi-DPI icon extraction straight from the shell's
  system image list, decoded off the UI thread so typing never stutters.
- **Keyboard-first**: `Up`/`Down` to navigate (first result is always
  pre-selected), `Enter` to launch, `Esc` or focus-loss to dismiss back to
  the tray.
- **Tray icon**: left-click toggles the launcher, right-click opens a small
  Toggle/Quit menu.
- Runs quietly in the background — dismissing the launcher hides it, it
  doesn't relaunch or rescan.

Under the hood, results are produced by a small provider pipeline
(`CommandProvider`/`PluginRegistry`) designed to grow beyond app search —
file search, a calculator, clipboard history, and more are planned next.

## Install / run

Requires the [Rust toolchain](https://rustup.rs/) and Windows.

```sh
git clone https://github.com/ariaryy/hayai.git
cd hayai
cargo run --release
```

`cargo build --release` produces `target/release/hayai.exe` with no console
window attached. There's no installer yet — copy the binary wherever you like
and run it; add it to your Startup folder if you want it running on login.

## Usage

| Action | Shortcut |
|---|---|
| Open / toggle the launcher | `Alt+Space` |
| Move selection | `Up` / `Down` |
| Launch selected result | `Enter` |
| Dismiss | `Esc`, click elsewhere, or `Alt+Space` again |
| Tray menu (Toggle / Quit) | right-click the tray icon |

## Project status

Hayai is under active development. App search and launch is the complete,
working vertical slice; file search, a calculator/unit conversion, clipboard
history, an action sub-menu, and a plugin system are planned but not yet
built. See `AGENTS.md` for architecture notes and known GPUI/Win32 pitfalls
if you're contributing.

## Why Windows-only, why Rust + GPUI

This project specifically targets Windows (Win32 APIs for the hotkey, tray,
shell icon extraction, and packaged-app enumeration are used directly, no
cross-platform abstraction layer). It's built on [GPUI](https://www.gpui.rs/)
(the GUI framework behind the [Zed](https://zed.dev/) editor) and
[gpui-component](https://github.com/longbridge/gpui-component) for widgets,
chosen over Electron/web-view approaches specifically to keep the memory
footprint small — see `AGENTS.md` for the measured RSS numbers behind that
decision.

## Contributing

Issues and PRs welcome, but this is a side project and I may not have time to
review or merge promptly — contribute at your own risk. Please read
`AGENTS.md` first — it documents several non-obvious GPUI/Win32 pitfalls
(window lifecycle, focus ordering, thread discipline) that have already
caused shipped bugs once.

## License

[MIT](LICENSE)
