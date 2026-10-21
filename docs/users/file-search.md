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
Press `Enter` to install Scry Search, or to start it if it is already installed.
Installation requests elevation, registers a per-user scheduled task, and starts
the daemon. Starting an existing installation runs that task while Hayai stays
open and shows a spinner. Once Scry can answer a search, Hayai displays results
for the current query automatically. If startup fails, press `Enter` to retry.
If the scheduled task is missing, reinstall Scry using the bundled setup script.

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

Scry's client, daemon, snapshot, and IPC formats are currently release-coupled.
Hayai releases therefore bundle `scryd.exe`, `scry.exe`, and the setup scripts
from the published Scry release tag recorded by Hayai's locked Rust dependency.
Scry builds those artifacts in its own repository; Hayai consumes them as an
independent application and practical example of the supported Rust client.
Do not replace only one component with an arbitrary build.

## Troubleshooting

- If Hayai still offers installation, confirm the scheduled Scry task exists
  and is running, then reopen file mode.
- If results are incomplete immediately after setup, allow initial indexing to
  finish and use `scry report` from a new terminal to inspect its status.
- Complete raw indexing requires fixed NTFS volumes. Other filesystems use
  Scry's more limited fallback behavior.
- For daemon installation and data locations, see Scry's
  [daemon guide](https://github.com/Ariaryy/scry-search/blob/main/docs/users/daemon.md).
