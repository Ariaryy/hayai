# Contributing

Thank you for improving Hayai. Keep changes focused and explain any Windows,
GPUI, performance, or memory tradeoffs in the pull request.

Before editing window lifecycle, focus, input actions, icon extraction, or the
GPUI dependencies, read `AGENTS.md`. Those areas contain non-obvious ordering,
re-entrancy, and resource-lifetime requirements.

## Development checks

Use Rust 1.89 or newer on Windows and run:

```powershell
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

CI runs the same commands. Tests that do not depend on Win32 should remain
cross-platform where practical, but release behavior must be verified on
Windows.

## Manual launcher checks

For changes affecting the UI, native runtime, search providers, or packaging:

1. `Alt+Space` opens Hayai centered and focused on the pointer's monitor.
2. `Esc`, focus loss, and `Alt+Space` while open hide it without quitting; the
   tray icon remains and the launcher reopens.
3. Reopening clears the query and refreshes the corresponding result set.
4. Tray left-click toggles; tray right-click can quit the process.
5. App results include packaged apps and load icons without blocking typing.
6. `Up`, `Down`, `Enter`, and search-history recall behave correctly.
7. `Ctrl+K` routes navigation and confirmation to the action panel while open.
8. File mode remains responsive and reports unavailable Scry setup clearly.

## Resource budget

Hayai targets minimum background cost. Any new runtime dependency, resident
cache, periodic task, or architecture change must include before/after idle and
active RSS measurements from a release build. Avoid synchronous GDI, COM,
network, or Scry work on the UI thread.

By submitting a contribution, you agree that it may be distributed under the
repository's MIT License.
