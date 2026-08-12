<div align="center">

# Hayai

### A fast, keyboard-first launcher for Windows

Apps · realtime file search · calculator · contextual actions

<p>
  <a href="#install">Install</a> ·
  <a href="docs/users/usage.md">User guide</a> ·
  <a href="docs/users/file-search.md">File search</a> ·
  <a href="docs/users/actions.md">Actions</a> ·
  <a href="https://github.com/Ariaryy/hayai/releases">Releases</a>
</p>

</div>

Hayai is a Raycast-inspired launcher built with Rust, GPUI, and native Windows
APIs. Press `Alt+Space`, type what you need, and launch an app, find a file, or
calculate a result without leaving the keyboard.

> 速い (*hayai*) is Japanese for “fast.”

> **Status:** early alpha. Core launcher, app search, file search, calculator,
> search history, and contextual actions work; behavior and packaging may still
> change between releases.

## Why Hayai?

- **Instant app search:** fuzzy search across Start Menu shortcuts and packaged
  Microsoft Store apps.
- **Realtime file search:** powered by the first-party
  [Scry Search](https://github.com/Ariaryy/scry-search) index and its persistent,
  low-overhead Windows daemon.
- **Useful calculator:** arithmetic, unit and temperature conversion, live
  currency conversion, number bases, clock arithmetic, timezones, and relative
  dates.
- **Keyboard first:** navigate with `Up`/`Down`, run with `Enter`, and open
  contextual actions with `Ctrl+K`.
- **Native and lightweight:** no browser runtime; icons, hotkeys, tray behavior,
  application discovery, and focus handling use native Windows facilities.
- **Quiet in the background:** dismissing the window returns it to the tray
  without relaunching or rescanning.

## Install

Download the latest installer or portable ZIP from
[GitHub Releases](https://github.com/Ariaryy/hayai/releases).

Hayai is not code-signed yet, so Windows SmartScreen may warn about downloaded
builds. Verify that the file came from this repository's Releases page and, if
needed, compare its SHA-256 hash with the included `SHA256SUMS.txt` before
choosing **Run anyway**.

The installer starts Hayai at login and bundles the matching Scry Search daemon
and setup scripts. File search prompts once for elevation when its per-user
daemon has not yet been installed. App search and the calculator do not require
elevation.

For portable use, upgrades, removal, and source builds, see the
[installation guide](docs/users/install.md).

## Search

Start typing to search applications. Hayai automatically recognizes calculator
expressions. Prefix a query with `f ` to search files through Scry Search.

```text
discord
f annual report ext:pdf
15 km to miles
100 usd to inr
3pm est to ist
5 days from now
```

See the [usage guide](docs/users/usage.md), [file-search guide](docs/users/file-search.md),
and [action reference](docs/users/actions.md) for the complete behavior.

## Shortcuts

| Action | Shortcut |
|---|---|
| Open or toggle Hayai | `Alt+Space` |
| Move selection | `Up` / `Down` |
| Run the selected result | `Enter` |
| Open contextual actions | `Ctrl+K` |
| Dismiss actions or Hayai | `Esc` |
| Toggle from the tray | left-click the tray icon |
| Open Toggle/Quit tray menu | right-click the tray icon |

## Architecture

Hayai keeps its UI thread focused on rendering and input. Native Windows work,
Scry queries, network-backed currency refreshes, and icon decoding run outside
the render path. Results flow through a small `CommandProvider` registry so app
search, files, and calculator behavior share one launcher surface.

Read the [architecture overview](docs/internal/architecture.md) and `AGENTS.md`
before changing GPUI window lifecycle, focus, input propagation, or Win32 code.
Those areas have ordering and re-entrancy requirements that are easy to break.

## Contributing

Issues and focused pull requests are welcome. Start with
[CONTRIBUTING.md](CONTRIBUTING.md) and run formatting, Clippy, and tests before
submitting a change. Security issues should follow [SECURITY.md](SECURITY.md).

## License

Hayai is available under the [MIT License](LICENSE).
