<div align="center">

<img src="assets/hayai.svg" alt="Hayai app icon" width="112" height="112">

# Hayai

A keyboard-first Windows launcher built for speed and low memory use.

Apps · File search · Calculator · Contextual actions

<p>
  <a href="#install">Install</a> ·
  <a href="docs/users/usage.md">User guide</a> ·
  <a href="https://github.com/Ariaryy/hayai/releases">Releases</a> ·
  <a href="#development">Development</a>
</p>

<sub>速い (hayai) is Japanese for "fast."</sub>

</div>

Press `Alt+Space`, type what you need, and launch an app, find a file, or
calculate a result. Hayai is inspired by Raycast and built with Rust, GPUI,
and native Windows APIs.

> **Early alpha:** app search, file search, the calculator, search history,
> and contextual actions are available. Behavior and packaging may change
> between releases.

## Features

| Feature | What you can do |
| --- | --- |
| App search | Fuzzy-search Start Menu shortcuts and packaged Microsoft Store apps. Recent launches appear first when the query is empty. |
| File search | Search files through the first-party [Scry Search](https://github.com/Ariaryy/scry-search) index and its persistent Windows daemon. |
| Calculator | Evaluate arithmetic, convert units and temperatures, use live currency rates, convert number bases, and work with clocks, timezones, and relative dates. |
| Contextual actions | Open a result, reveal it in Explorer, copy its path, or run it as administrator where supported. |
| Search history | Recall previous searches and calculations with `Up` when the query is empty. |

Hayai uses native Windows facilities for hotkeys, the tray, app discovery,
icons, and focus handling. It has no browser runtime. Dismissing the window
keeps it in the tray, ready to reopen without relaunching or rescanning.

## Install

Download the latest installer or portable ZIP from
[GitHub Releases](https://github.com/Ariaryy/hayai/releases).

| Package | Setup |
| --- | --- |
| Installer | Installs Hayai, starts it at login, and bundles the matching Scry Search daemon and setup scripts. |
| Portable ZIP | Extract the archive and run `hayai.exe`. No shortcuts or launch-at-login entry are installed. |

App search and the calculator work without elevation. When you enter file
mode, Hayai prompts you to install Scry Search if its daemon is not already
running in the background. Select the installation action and press `Enter`;
Windows requests elevation to install and start the per-user daemon.

Hayai is not code-signed yet, so Windows SmartScreen may warn about downloaded
builds. Verify that the file came from this repository's Releases page and, if
needed, compare its SHA-256 hash with the included `SHA256SUMS.txt` before
choosing **Run anyway**.

The [installation guide](docs/users/install.md) covers upgrades, removal,
portable use, and source builds.

## Usage

### Search apps and calculate

Type an app name to search applications. Calculator expressions are recognized
automatically. Use `Up` and `Down` to select a result, then press `Enter` to
launch an app or copy a calculator value.

### Search files

Press `Space`, then type `f` to enter file mode. The leading space activates
Hayai's command router; ordinary queries continue to search apps or calculate.
The router also leaves room for future modes, such as `Space` then `cp` for
clipboard history.

### Examples

In these examples, `␠` represents a literal leading space. Do not type the
`␠` symbol itself.

```text
discord
␠f annual report ext:pdf
15 km to miles
100 usd to inr
3pm est to ist
5 days from now
```

See the [usage guide](docs/users/usage.md),
[file-search guide](docs/users/file-search.md), and
[action reference](docs/users/actions.md) for details.

### Shortcuts

| Action | Shortcut |
| --- | --- |
| Open or toggle Hayai | `Alt+Space` |
| Move selection | `Up` / `Down` |
| Run the selected result | `Enter` |
| Open contextual actions | `Ctrl+K` |
| Dismiss actions or Hayai | `Esc` |
| Toggle from the tray | Left-click the tray icon |
| Open the Toggle/Quit tray menu | Right-click the tray icon |

Press `Esc` or focus another window to return Hayai to the tray. When the
action panel is open, `Up`, `Down`, and `Enter` apply to its actions, and
`Esc` closes the panel first.

## Development

Installed Windows releases download stable updates in the background and apply
them on request through a silent restart. Portable and development builds do not
automatically update.

Hayai keeps rendering and input on the UI thread. Native Windows work, Scry
queries, currency refreshes, and icon decoding run outside the render path.
A small `CommandProvider` registry connects app search, files, and the
calculator to the launcher.

Read the [architecture overview](docs/internal/architecture.md) and `AGENTS.md`
before changing GPUI window lifecycle, focus, input propagation, or Win32 code.
These areas depend on specific ordering and re-entrancy rules.

For source-build instructions, see the
[installation guide](docs/users/install.md). The
[contribution guide](CONTRIBUTING.md) lists formatting, Clippy, tests, manual
launcher checks, and the resource measurements required for runtime changes.

## Contributing

Issues and focused pull requests are welcome. Start with
[CONTRIBUTING.md](CONTRIBUTING.md) before submitting a change. Report security
issues as described in [SECURITY.md](SECURITY.md).

## License

Hayai is available under the [MIT License](LICENSE).
