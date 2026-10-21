# Changelog

## 0.1.0-alpha.3

### Launcher

- New Hayai app and tray icons, plus a reorganized README and user guides.
- Compact shortcut badges with SVG key icons and contextual folder navigation hints.
- Numbered action menus with file-specific labels and an Open with submenu. Registered applications show their icons, with the default application first when Windows identifies it.
- Centered empty states for app and file searches.
- Search and action menus reset after hiding the launcher, without adding a delay when opening it.

### File search

- Updated the bundled Scry Search daemon and Rust client to `v0.1.0-alpha.4`.
- File sizes appear when Scry supplies them.
- Folder navigation stays within the existing search row, with Tab to enter and Backspace to go up.
- Setup explains whether Scry needs installing or starting and links to the file-search guide.
- Starting an installed daemon keeps Hayai open with a loading spinner and returns to search when Scry is ready.

This is an early alpha. Scry installation requires elevation; app search and the calculator work without it.

For earlier releases, see [GitHub Releases](https://github.com/Ariaryy/hayai/releases).
