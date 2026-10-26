# Changelog

Changes are grouped by release. Unreleased changes have been merged but are not
yet included in a published package.

## Unreleased

### Settings

- General settings control Windows login startup, launcher display, recent-item history, and automatic update checking/downloading. Turning history off clears saved recents; queued writes cannot restore them. Display placement uses a dropdown and on/off settings use themed toggles. Controls retain their state while changes save.
- Open Settings from the Hayai group in its own resizable window, with a themed titlebar, sidebar navigation, and About pinned at the bottom.
- Choose light and dark palettes independently from Hayai, One Dark/One Light, Catppuccin, Tokyo Night, and Gruvbox. The selected appearance applies to both windows and persists between launches.
- Record and reset shortcuts for opening Hayai, the action panel, file search, and settings. Bindings save immediately after recording. Key badges show the current bindings; conflicts keep the previous shortcut working.
- Scroll settings with the launcher's smooth wheel behavior and a thin right-edge scrollbar that appears only when content overflows.
- Browse expandable asset and theme notices from About, with full license texts and links to their sources. The notices are also included in packaged releases.

## 0.1.1 - 2026-10-24

### Launcher

- Smooth mouse-wheel scrolling follows display frames and stops animating when settled. Mouse input takes over an ongoing touchpad flick without waiting for its inertia to end.
- Arrow-key navigation stops at the first and last result. Returning to the first row reveals its group heading.
- Reopening resets the query, scroll position, folder scope, and action panels. The reset frame is rendered and synchronized with the Windows compositor before the window is shown, preventing the old view from flashing on reopen.
- App and file icons use a bounded 128-entry cache, with evicted images also released from the renderer. Nearby icons are prefetched to reduce loading during scrolling.
- DPI-aware icon sizing balances detail and sharpness at fractional Windows scaling. Notification icons use cached PNG artwork at the same row size as app icons.

### Calculator

- Shared parsing and evaluation make spacing, parentheses, unit case, and numeric precision consistent. Clipboard values retain precision independently of display rounding.
- Currency calculations use one rate snapshot per calculation, support more currency codes and aliases, and recover from interrupted or failed rate refreshes.
- Date and timezone calculations validate calendar dates and offsets, preserve seconds and day rollover, and distinguish calendar arithmetic from elapsed time across daylight-saving changes.
- Unfinished calculations preview their complete prefix while typing, including partial conversion targets and function arguments. Empty starts show a neutral prompt; invalid completed expressions remain errors.
- Conversion arrows render consistently as SVG artwork.

### Updates and maintenance

- Scry installation errors remain available after later update checks, including their diagnostic details.
- CI reuses debug and release build caches from main.
- Refreshed the README introduction and showcase screenshot.

## 0.1.0 - 2026-10-22

### Updates

- Hayai checks for updates in the background and offers a restart action when an update is ready.
- Updated Scry Search integration to `v0.1.1`, with compatibility checks and installation/update progress in notifications.
- Release checks and temporary failures are cached, and update discovery stays out of the search path.
- Notifications can be dismissed and their diagnostic logs copied through `Ctrl+K`.
- Check for updates and About Hayai are searchable in the Hayai group.

### File search

- Preserve Scry's relevance ranking instead of reordering results locally.
- Keep long filenames within their rows and clip scrolling to the results area.

## 0.1.0-alpha.3 - 2026-10-21

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

## 0.1.0-alpha.2 - 2026-08-30

### Calculator

- Expanded arithmetic and unit expressions through `fend-core`.
- Added relative-time calculations, timezone differences, dated city conversions, and conversion to the Windows system timezone when no destination is given.
- Named places follow Windows daylight-saving rules; explicit timezone abbreviations retain fixed offsets.
- Standardized calculator, currency, unit, and timezone presentation with badges and previews while typing.

## 0.1.0-alpha.1 - 2026-08-12

Initial published release.

- Global `Alt+Space` launcher with fuzzy app search, Start Menu and Store app discovery, recent apps, and asynchronously loaded icons.
- Scry Search file queries through a persistent daemon connection, with live refinement and elevated daemon setup when file search is enabled.
- Calculator with arithmetic, unit and temperature conversions, live currency conversion, number bases, and time/date calculations.
- Keyboard navigation and search history, plus contextual actions for apps and files through `Ctrl+K`.
- Tray toggle and quit controls, focus-loss dismissal, and positioning on the cursor's monitor.
- Windows installer and portable ZIP, bundled Scry daemon and CLI, launch-at-login registration, and download checksums.

Published packages and their release notes are available on
[GitHub Releases](https://github.com/Ariaryy/hayai/releases).
