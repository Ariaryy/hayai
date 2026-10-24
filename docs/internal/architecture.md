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
- `src/commands.rs` defines result/action models and the `CommandProvider`
  interface; `src/plugins.rs` routes queries across built-in providers.
- `src/apps.rs` scans installed applications and lazily decodes icons on the
  background pool.
- `src/files.rs` translates file mode into debounced Scry queries.
- `src/calc/` classifies calculator intent and shares one bounded numeric
  engine across physical and currency conversions. See [calculator invariants](calculator.md).
- `src/scroll.rs` contains pure wheel animation and input-handoff logic.
- `src/updates.rs` and `src/scry_updates.rs` manage release discovery and update
  state; `src/startup.rs` handles login registration through Velopack lifecycle
  hooks.

The native thread never accesses GPUI state directly. Expensive shell, icon,
network, and file-search work must stay outside rendering and input callbacks.
Native commands reach GPUI through a futures unbounded channel; the receiver
wakes on messages rather than polling a timer. Background search completion is
checked against the registry's current query generation before publishing.

## Window lifetime

Hayai hides its only GPUI window instead of destroying it. On Windows, closing
the last GPUI window quits the process, which would break tray and hotkey
behavior. Callers already inside a window update must use the supplied
`&mut Window`; re-entering the same `WindowHandle::update` fails.

For focus, reset/presentation ordering, action propagation, borrowing, and shell
resource ownership, read [GPUI and Windows invariants](gpui-windows.md).

## Scry integration

Hayai depends on `scry-client` through a locked, tagged Git dependency. Scry's
alpha client, IPC, daemon, and snapshot formats are release-coupled, so Hayai's
packaging downloads the corresponding artifact produced by Scry's own release
workflow. Hayai builds independently; it embeds Scry's supported Rust client and
serves as a practical example of using Scry as another Windows application's
search engine.

## Resource budget

The performance contract and measurement requirements live in
[AGENTS.md](../../AGENTS.md). The measurements below are historical observations,
not a current memory ceiling or a substitute for measuring a new change.

### GPUI migration (2026-07-03)

The move from crates.io GPUI 0.2.2 to git-sourced GPUI and gpui-component was
reported at roughly 40 MB before and 45–57 MB afterward in normal idle/active
use. This was an accepted widget-maintenance tradeoff. The original record
does not give a reproducible workload or separate idle/active samples, so use
it as historical context rather than a current benchmark.

Icon cache and DPI follow-up measurements, including their isolated-process
and GPU limitations, are recorded in [icon cache memory](../icon-cache-memory.md).

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

### Calculator pipeline measurement (2026-10-23)

The redesign adds no dependencies. `scripts/measure-calculator.ps1` measures a
release executable after a 15-second startup wait, then opens and hides its
owned window around five identical queries: `1 Mb to MB`, `1 lb to kg`,
`12pm newyork to ist`, `12pm london to ist`, and `5 kg * 9.8 m/s^2 to N`.
Active RSS is the median of five working-set samples; warmed idle is sampled
two seconds after the final hide. The original executable was built from
`fb27f81` in a detached checkout and copied before building the branch.

Steady cold-idle samples were 56.20 MiB before and 57.46 MiB after. Repeated
warmed active medians varied from 90.02–107.72 MiB before and 91.58–102.35 MiB
after, with overlapping ranges; these are spot measurements, not evidence of
a memory reduction. A representative warmed private-memory comparison was
86.23 MiB before and 82.02 MiB after. Record the small observed cold-idle
increase rather than treating the redesign as free. New retained state is
bounded to the current query, shared immutable rates, and four expiring native
timezone definitions per thread.
