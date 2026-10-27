# Hayai agent guide

Hayai is a Windows launcher built with Rust, GPUI, and gpui-component.
Fast input, low idle cost, and minimum retained memory are product requirements.

## Working defaults

- Find the actual constraint, then choose the smallest correct solution.
  Remove unnecessary complexity; add abstractions when the task needs them.
- For behavior changes, account for affected entry points, query modes, input
  devices, and reverse transitions. Verify entering and leaving a state,
  including background work that can finish after leaving it.
- Stop only test processes whose PIDs you captured when starting them.
  Protect existing app instances and user data; isolate test state when needed.
- Keep local implementation reasons beside the code. Internal docs should
  explain durable decisions, constraints across components, or traps that
  reading the source would not reveal. Rewrite stale guidance rather than
  appending feature inventories or PR histories.

## Performance contract

- Prefer lazy work, bounded caches, shared data, and OS-maintained indexes.
  Keep blocking shell, GDI/COM, disk, IPC, and network work off rendering and
  input callbacks; use the existing background workers.
- Prefer event-driven wakeups. Justify any new polling, resident task, cache,
  or dependency with the workload it improves and the resources it retains.
- For runtime dependency or architecture changes, measure **before/after idle
  and active RSS in release builds** with the same workload and settling time.
  Report the baseline commit, commands, workload, and results in the PR.
  Explain CPU, I/O, latency, and package-size tradeoffs where affected.
- Distinguish full-process measurements from isolated cache benchmarks and
  CPU memory from GPU allocations. State measurement gaps explicitly; avoid
  inferring an improvement from overlapping samples.
- Keep result sets complete and virtualized. Bound icon retention and pending
  speculative work; release evicted renderer images as well as CPU entries.

## Implementation guardrails

- Keep launcher copy in `src/ui_text.rs`, embedded icon definitions in
  `src/assets.rs`, and palette definitions in `src/theme.rs`. Use typed action
  identities for behavior so editing display text cannot change navigation.
- Keep the single launcher window alive in the tray: hide and reuse it.
  Use the supplied `&mut Window` inside window callbacks; use handle updates
  only outside an existing update of that window.
- Keep native-thread communication separate from GPUI state. Release shared
  `RefCell` borrows before callbacks, entity updates, or rendering can re-enter.
- Keep Windows code behind `#[cfg(windows)]` with matching non-Windows stubs.
  Release builds have no console; user-visible diagnostics need another path.
- Update GPUI dependencies deliberately. `gpui`, `gpui_platform`, and
  gpui-component's GPUI dependency must resolve to the same Git source/revision.
  Inspect the locked dependency checkout before assuming an API is available.
- Preserve comments explaining Win32 focus order, GPUI re-entry, action routing,
  or resource ownership. Update the explanation when the behavior changes.

## Read before changing an area

| Change | Required reference |
| --- | --- |
| Window visibility/focus, input/actions, list layout, scrolling, themes, or shell icons | [GPUI and Windows invariants](docs/internal/gpui-windows.md) |
| UI composition, feature enable/disable, threading, provider dispatch, background search, or Scry integration | [Architecture](docs/internal/architecture.md) |
| Settings window layout, spacing, type, colors, theme palettes, conflict markers, or command trigger/alias UI | [Design](DESIGN.md) |
| Calculator detection, parsing, previews, numeric output, FX, or timezones | [Calculator invariants](docs/internal/calculator.md) |
| Icon cache sizing, prefetch, or DPI rendering | [Icon measurements and limitations](docs/icon-cache-memory.md), plus the GPUI reference above |
| Version bumps, packaging, tags, or publication | [Release procedure](docs/internal/releasing.md) |

For unfamiliar GPUI APIs, read the revision selected by `Cargo.lock` under
`~/.cargo/git/checkouts/zed-*/*/crates/gpui` (or `gpui_windows`) and
`~/.cargo/git/checkouts/gpui-component-*/*/crates/ui/src`. Current upstream docs
may describe a different API.

## Completion

- Run the applicable checks in [CONTRIBUTING.md](CONTRIBUTING.md). For behavior
  bugs, first reproduce the failure with a focused regression test. For purely
  visual bugs, use before/after captures instead of implementation-shaped tests;
  motion and frame timing need video or frame evidence. Report automated and
  manual results separately. Lifecycle changes also need the reopen checks in
  the GPUI reference; view state alone cannot verify the first visible frame.
- Add user-visible changes to `CHANGELOG.md` under Unreleased in the same PR.
- Include resource measurements when the performance contract applies. If a
  required measurement or manual check is unavailable, state what remains
  unverified instead of marking it passed.
