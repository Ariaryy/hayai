# Architecture

Hayai is a single-process GPUI application with a separate native Windows
runtime thread. It also talks to Scry Search's separate elevated daemon for file
queries.

```text
Alt+Space / tray
       │
Win32 runtime thread ── NativeCommand channel ── GPUI launcher window
                                                   │
                                      CommandProvider registry
                                        ╱         │         ╲
                                  app catalog  calculator  file provider
                                                               │
                                                     Scry SearchSession
                                                               │
                                                    elevated scryd daemon
```

## Process and thread boundaries

- `src/main.rs` starts GPUI, initializes the theme and global state, and bridges
  commands from the native runtime into the event loop.
- `src/native/windows.rs` owns the hidden Win32 message window, global hotkey,
  tray icon, shell integration, and the persistent Scry search worker.
- `src/launcher.rs` owns launcher visibility, the search input, result list,
  keyboard routing, history, and contextual action panel.
- `src/plugins.rs` routes queries across built-in `CommandProvider`s.
- `src/apps.rs` scans installed applications and lazily decodes icons on the
  background pool.
- `src/files.rs` translates file mode into debounced Scry queries.
- `src/calc/` contains small, ordered evaluators for calculator-shaped input.

The native thread never accesses GPUI state directly. Expensive shell, icon,
network, and file-search work must stay outside rendering and input callbacks.

## Window lifetime

Hayai hides its only GPUI window instead of destroying it. On Windows, closing
the last GPUI window quits the process, which would break tray and hotkey
behavior. Callers already inside a window update must use the supplied
`&mut Window`; re-entering the same `WindowHandle::update` fails.

These and other framework-specific constraints are maintained in `AGENTS.md`.
Treat that file as the authoritative implementation guide for window focus,
action propagation, `RefCell` borrowing, and native resource cleanup.

## Scry integration

Hayai depends on `scry-client` through a locked, tagged Git dependency. Scry's
alpha client, IPC, daemon, and snapshot formats are release-coupled, so Hayai's
packaging downloads the corresponding artifact produced by Scry's own release
workflow. Hayai builds independently; it embeds Scry's supported Rust client and
serves as a practical example of using Scry as another Windows application's
search engine.

## Resource budget

Hayai is intended to remain a small background utility. Before accepting a new
runtime dependency or architectural cache, compare idle and active RSS with a
release build. Prefer lazy work, bounded collections, existing background
workers, and OS-maintained indexes. A latency improvement that materially
increases idle CPU, memory, or background I/O needs measurements and an
explicitly documented tradeoff.

### Calculator engine measurement (2026-08-29)

The calculator expansion adds `fend-core`, a dependency with no transitive
dependencies, for broad arithmetic and unit expressions. Release binaries at
`main` (`3e2e67c`) and `feature/calculator-engine` were each started with the
launcher hidden, allowed 15 seconds for the application catalog scan to settle,
sampled three times, opened through `Alt+Space`, and sampled three more times.
The minimum working set from each settled group was:

| Build | Idle RSS | Active RSS |
|---|---:|---:|
| `main` | 60.44 MiB | 106.00 MiB |
| Calculator branch | 60.36 MiB | 94.78 MiB |

The measurement shows no idle RSS regression. Active working-set values vary
with GPUI and catalog paging, but the calculator branch was also lower in this
comparison. The Windows timezone implementation uses OS APIs and adds no new
runtime dependency or resident timezone database.
