# File search

Hayai uses [Scry Search](https://github.com/Ariaryy/scry-search), a first-party
realtime file index for Windows. Scry maintains a compact index in an elevated
per-user daemon; Hayai remains unelevated and queries it through Scry's local
client protocol.

## Set up the daemon

Open Hayai and type `f `. If the daemon cannot be reached, the result list shows
an installation action. Select it and press `Enter`. Windows requests elevation
once, installs a highest-privilege per-user scheduled task, and starts the
daemon.

Initial indexing is the exceptional heavy phase. Search results become complete
after Scry finishes reading the fixed NTFS volumes available to the current
machine.

## Search syntax

The `f ` prefix belongs to Hayai; the remainder is a Scry query.

```text
f budget
f annual report type:file ext:pdf,docx
f type:dir modified:<7d
f *.toml
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
