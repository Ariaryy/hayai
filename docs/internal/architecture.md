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

The native thread never accesses GPUI state directly. Expensive shell, icon,
network, and file-search work must stay outside rendering and input callbacks.
Native commands reach GPUI through a futures unbounded channel; the receiver
wakes on messages rather than polling a timer. Background search completion is
checked against the registry's current query generation before publishing.

Calculator detection and search share a bounded evaluation pipeline; its
numeric, preview, and rate-lifecycle constraints are maintained in
[calculator invariants](calculator.md).

## Window lifetime

Hayai hides its launcher GPUI window instead of destroying it. On Windows, closing
the last GPUI window quits the process, which would break tray and hotkey
behavior. Callers already inside a window update must use the supplied
`&mut Window`; re-entering the same `WindowHandle::update` fails.

Settings opens a separate, ordinary window lazily. Closing it releases its view
and recording subscriptions; the hidden launcher keeps the tray process alive.
App-wide keystroke interceptors must check their owning window before handling
input. Otherwise the hidden launcher's interceptor can consume settings keys.

Preferences are loaded once before the native runtime starts. Settings controls
show the latest requested draft while a single background save runs. Rapid edits
replace the queued draft; failures restore the committed values and report the
error. Runtime routing uses committed preferences. Only a change to the active
appearance reapplies the theme, so ordinary saves do not reset widget state.
The native thread acquires a replacement hotkey before releasing the old one;
startup registration is restored if persistence fails. Startup reads the actual
Windows Run entry instead of rewriting it on unrelated saves.
`HAYAI_SETTINGS_PATH` isolates preferences for tests. No file watcher or polling
is retained; external edits take effect at the next launch.

Providers receive committed preferences at startup and after relevant saves.
Configuration hooks only copy in-memory values; each background file search
captures its own options. A changed file-search preference reruns the visible
file query with its input and folder scope intact, invalidating older work.
Hidden file searches are invalidated without starting replacement I/O; other
providers keep their in-flight queries. Registry borrows end before window or
entity updates, which can re-enter GPUI.

Providers mark empty-query recents as a contiguous prefix. The result delegate
keeps Recent, main results and Hayai commands as sections of one flat result
set, so headings never consume selection or action indexes. File-mode entry
merges validated recents with the home listing and removes duplicate paths;
scoped browsing and typed searches do not add a recent section.

Recent-item persistence captures a generation before queueing background writes.
Disabling or clearing history advances that generation and pauses recording;
a worker-only disk lock orders deletion against writes already in progress.
Re-enabling cannot admit snapshots from before the clear. A queued clear request
survives rapid off/on edits until its disk work completes. Clearing memory also
invalidates launcher search generations so old recent results cannot reappear.

Automatic Hayai update discovery and downloads are separate preferences. Startup
loads them before creating the updater. An in-flight discovery rechecks policy
before downloading or announcing availability. Discovery without downloading
exposes a Download update action; only staged updates expose Restart to update.
Manual checks remain available with automatic checks off. No timer is added.

Settings scrolling owns a per-window scroll handle and reuses launcher wheel
physics and precision-input handoff. Navigation cancels pending motion and
resets the offset; deactivation cancels motion and dragging. Frames are requested
only until the wheel motion settles. The scrollbar reads layout after the
scrollable content, so its thumb appears only when content overflows.

Command aliases resolve in `command_router`, before provider dispatch. They
identify typed destinations; internal ` f ` and ` = ` prefixes remain stable.
Typing Space enters a picker, invalidates pending provider generations and
removes selectable results. Partial or unknown aliases cannot confirm old apps.
The optional picker controls suggestions only. With it disabled, the launcher
shows a fresh default app query while collecting the same command prefix. With it
disabled, text that can no longer become an active trigger or alias leaves
command mode and is searched as an ordinary query, since nothing else would
explain the stalled prompt.
Saving a visibility change refreshes an active command session without editing
its input; both directions invalidate older provider generations. No partial
alias is dispatched as an app or calculator query, and Enter stays guarded.
Each command has one user-set trigger (one to four keys, activating without
Enter) and optional full-word aliases. A complete alias wins over the trigger it
may begin with, so pasted text keeps its query. Entries of different commands may
not prefix one another: instant activation would make the longer one unreachable.
Such overlaps are saved and flagged, never refused, but every overlapping entry
stays inactive so the router never guesses. Shortcut conflicts follow the same
rule; the system-wide launcher hotkey is registered with Windows and keeps
working, so it only disables the in-launcher action it shares a key with.
The alias editor is created only while its settings page is open. Format
errors stay inline in the editor; persistence shares the normal settings
draft/save path, and routing reads only committed entries. No timer, file watcher, or additional provider is involved.

Windows-owned combinations may never enter GPUI's key stream. In particular,
Win+B focuses the Windows notification area. Preserve the Win modifier on keys
that reach the recorder, but do not install a global keyboard hook to intercept
OS shortcuts. Registered Hayai hotkeys take a separate WM_HOTKEY path while
recording; they must not open the launcher mid-recording.

Appearance mode and palette choices are separate preferences. Light and dark
each retain their own theme family; changing the inactive mode's palette must
not switch appearances. Both windows and skeleton previews use the palette
definitions in `theme.rs`, with derived widget tokens refreshed after applying.

For focus, reset/presentation ordering, action propagation, borrowing, and shell
resource ownership, read [GPUI and Windows invariants](gpui-windows.md).

The launcher root owns transient input, navigation, and panel state. Its UI
components are stateless render builders: they use the root's `Context` so
keyboard actions and pointer callbacks keep the same focus and event routing.
Window reuse and focus sequencing belong to `launcher/lifecycle.rs`, outside
those components. Do not introduce extra entities or retained copies of provider
results merely to compose a page.

Provider registration remains explicit in startup; dispatch and query generation
belong to `PluginRegistry`. A future feature toggle must invalidate pending work
and reset the affected mode, rather than only hiding its UI. Theme application,
UI copy, and embedded icon definitions live in `theme`, `ui_text`, and `assets`;
changing a display label must not change navigation or action identity.

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
