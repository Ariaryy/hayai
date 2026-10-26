# Settings window resource check

Baseline: `056fff9e9bbf75c53f1bd94e5feb2277a6f196d6`. Candidate: settings window,
palette and notices changes on `feature/settings-window`; the executable hash
below identifies the measured build independently of subsequent documentation
and test commits. Both executables were built with `cargo build --release --locked`.

## Reproduction and workload

Preserve the baseline release executable before building the candidate, then run:

```powershell
./scripts/measure-settings.ps1 -BaselineExecutable <baseline.exe> -CandidateExecutable target/release/hayai.exe
```

The script starts one process at a time with isolated `LOCALAPPDATA` and
`HAYAI_SETTINGS_PATH`. It waits 15 seconds with the launcher hidden, samples
three times 300 ms apart, sends WM_HOTKEY only to that process's message window,
waits one second, then repeats the samples with the launcher visible and an
empty query. It stops only the process it started. This is full-process Windows
working set (RSS), with private committed bytes reported separately; GPU
allocations are not separately measured. Settings remains unopened during this
comparison because no equivalent baseline settings window exists.

## Results (MiB)

| Build | Hidden RSS | Hidden private | Visible RSS | Visible private | Executable bytes |
| --- | --- | --- | --- | --- | --- |
| Baseline | 54.31 | 46.77 | unavailable | unavailable | 27911168 |
| Candidate | 55.01 | 47.39 | 97.91–98.07 | 102.86–103.02 | 28410368 |

Baseline SHA256: `d0a4a7d247e6ee5778fd8c695f454ff12323473a9e4764e70771965c0aef2c46`.
Candidate SHA256: `a622a92b200de7afee3710467f03b3adc8ee3d2180915910c2d8940c3555ccc9`.

An existing user's Hayai instance owns Alt+Space. The baseline exits its native
thread on that registration conflict, so it cannot enter the active workload.
The candidate uses an isolated Ctrl+Alt+F24 binding and keeps its native thread.
Consequently the idle samples also have different native-thread lifetimes;
these figures do **not** establish a comparable memory regression or improvement.
A complete active before/after comparison remains unverified until the baseline
can own its shortcut. Existing user instances were left running.

## Resource ownership and tradeoffs

Settings is created lazily and destroyed on close. Its recording subscriptions,
scroll handle and animation state are released with the view. Preferences retain
four parsed shortcuts and two palette identities. Palettes and license texts
are embedded static data. The release executable grew by
499200 bytes (about 1.8%).

No runtime crate or polling task was added. Mouse-wheel animation reuses the
launcher's physics and input handoff, requests display frames only while moving,
and cancels on navigation or deactivation. Touchpad deltas retain native
precision handling. The six-pixel scrollbar is painted only for overflowing
content. Saves, native hotkey replacement and folder creation/opening use the
existing background executor, adding disk I/O only when saving or opening the
preferences folder. CPU usage, GPU memory, settings-open RSS, save latency and
installer size were not measured.

## Validation boundaries

Automated: formatting, Clippy with warnings denied, 139 unit tests; separate
settings and launcher GUI smoke tests. The settings test exercises real GPUI
pre-dispatch input, immediate persistence, conflict and Escape handling,
Ctrl+Win+F24 modifier preservation, all palette pairs, scrolling settling and
navigation reset, close during saving and reopening. Synthetic key events do
not establish that Windows will deliver a physically pressed reserved shortcut.

Release capture harness: independent light/dark selection, theme propagation,
close without quitting, badge geometry, titlebar controls, notices expansion
and right-edge scrollbar inspected in captures. Four distinct scroll positions
were captured over 1.33 seconds; the capture overhead limits this evidence to
motion and settling, not display-rate smoothness. The 14-frame sequence and GIF
are local artifacts under `target/settings-scroll-*` and `target/settings-scroll.gif`.

The packaging notice-copy loop was checked in isolation; a full installer build,
physical touchpad/drag checks, all monitor/DPI combinations and the complete
manual launcher reopen matrix remain unverified.
