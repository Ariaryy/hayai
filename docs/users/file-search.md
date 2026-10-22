# File search

## About Scry Search

Hayai uses [Scry Search](https://github.com/Ariaryy/scry-search), a first-party
realtime file index for Windows. Scry maintains a compact index in an elevated
per-user daemon; Hayai remains unelevated and queries it through Scry's local
client protocol.

## Set up the daemon

Open Hayai, press `Space`, and then type `f`. The leading space tells Hayai to
route the query as a command; `f` selects the file-search provider. Hayai
shows a command hint after the leading space, then a file-search placeholder
after `f`. Backspace on an empty file-search input returns to app search.
Press `Tab` on a folder to search inside it. A breadcrumb beside the input shows
the current location; click an earlier folder or **All files** to go back.
With an empty query, `Backspace` goes back one folder level.
If the daemon cannot be reached, Hayai shows a setup screen with the Scry icon.
The update page checks for a compatible release and displays installation progress.
Scry handles package verification, elevation, and daemon activation. Hayai verifies
the requested daemon version before returning to file search. A running legacy
daemon must be stopped before migration because it has no graceful-stop endpoint.

The underlined **Learn more** link opens this guide. Scry's source code and
releases are available in the [Scry Search repository](https://github.com/Ariaryy/scry-search).

Initial indexing is the exceptional heavy phase. Search results become complete
after Scry finishes reading the fixed NTFS volumes available to the current
machine.

## Search syntax

The command pattern is one literal leading space followed by a provider keyword.
For file search it is `Space` → `f`; everything after `f` becomes the Scry
query. The `␠` symbol below only makes the leading space visible:

```text
␠f budget
␠f annual report type:file ext:pdf,docx
␠f type:dir modified:<7d
␠f *.toml
```

Read Scry's maintained
[search-syntax guide](https://github.com/Ariaryy/scry-search/blob/main/docs/users/search-syntax.md)
for filters, wildcards, ranking, and path-term behavior.

## Version matching

Hayai checks IPC and shared-index compatibility independently of Scry's application
version. Compatible updates remain optional and do not interrupt searching.
Successful release checks are cached for 24 hours; **Check for updates** refreshes
both Hayai and Scry. Discovery and installation use Scry's explicit CLI commands;
the daemon performs no update-related network requests.
Required installations target Scry `v0.1.0`, matching Hayai's pinned SDK.
They use the exact release directly; compatible improvement checks remain separate.
Checks run without interrupting search. Optional Scry updates appear in the main
Notifications group alongside Hayai updates, with no optional notifications in
file mode. Dismissal lasts until Hayai restarts or another manual check begins.
Failed optional checks do not create notifications. Update actions include
**Copy diagnostic log**, preserving the original response and exit code for debugging.

The bundled CLI must support Scry's schema 1 updater contract. Releases predating
that contract require a newer Hayai package. Release archives without the updater
manifest cannot be installed through this path.

## Troubleshooting

- If Scry is unavailable, confirm the scheduled Scry task exists
  and is running, then reopen file mode.
- If results are incomplete immediately after setup, allow initial indexing to
  finish and use `scry report` from a new terminal to inspect its status.
- Complete raw indexing requires fixed NTFS volumes. Other filesystems use
  Scry's more limited fallback behavior.
- For daemon installation and data locations, see Scry's
  [daemon guide](https://github.com/Ariaryy/scry-search/blob/main/docs/users/daemon.md).
