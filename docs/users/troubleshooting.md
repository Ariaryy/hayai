# Troubleshooting

## `Alt+Space` does not open Hayai

Another program may own the global shortcut. Close or reconfigure the other
program, then restart Hayai. Also check the tray: Hayai normally keeps one
hidden window alive instead of creating a new process each time.

## Hayai disappeared but is still running

This is normal after `Esc` or focus loss. Left-click the tray icon or press
`Alt+Space` to show it again. Use the tray's right-click menu to quit fully.

## File search offers installation repeatedly

Confirm that Scry's per-user scheduled task was installed with elevation and is
running. The bundled client and daemon must come from the same Hayai release;
mixing standalone binaries from different Scry versions can be incompatible.

## File results are incomplete

Initial Scry indexing can take time. Run `scry report` in a new terminal to see
the daemon state. Fixed NTFS volumes provide the complete indexing path.

## Windows warns about the installer

Hayai releases are not code-signed yet. Download only from this repository's
Releases page and verify the SHA-256 checksum published with the release.

When reporting a problem, include the Hayai version, Windows version, whether
you used the installer or portable ZIP, and exact steps to reproduce it. Do not
include private filenames or file-search results in a public issue.
