#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod apps;
mod commands;
mod launcher;
mod native;
mod plugins;

use std::sync::mpsc;
use std::time::Duration;

use gpui::{App, Application, KeyBinding, Timer, actions};
use apps::Catalog;
use launcher::{LauncherGlobal, LauncherState};
use native::{NativeCommand, NativeRuntime};

actions!(
    hayai,
    [
        Backspace,
        DeleteWordLeft,
        CloseLauncher,
        SelectNext,
        SelectPrev,
        Activate,
        SelectAll,
        Copy,
        Cut,
        Paste,
        MoveLeft,
        MoveRight,
        MoveWordLeft,
        MoveWordRight,
        SelectLeft,
        SelectRight,
        SelectWordLeft,
        SelectWordRight,
    ]
);

fn main() {
    let (native_tx, native_rx) = mpsc::channel();
    let native_runtime = NativeRuntime::start(native_tx);

    Application::new().run(move |cx: &mut App| {
        LauncherState::install(cx);
        // Scan the Start Menu up front so the first open is instant; icons are
        // still decoded lazily on render.
        Catalog::install(cx);

        // `escape` dispatches `CloseLauncher`, which the focused `LauncherView`
        // handles via its `on_action(Self::close)` listener. We intentionally do
        // NOT register a global `cx.on_action` handler for it: that handler would
        // run with a bare `&mut App` and re-enter `handle.update`, which fails
        // while the window is mid-update (see `LauncherState::dismiss`).
        // All bindings are scoped to the "Launcher" key context so they only
        // fire while the launcher is focused.
        cx.bind_keys([
            KeyBinding::new("backspace", Backspace, Some("Launcher")),
            KeyBinding::new("escape", CloseLauncher, Some("Launcher")),
            KeyBinding::new("down", SelectNext, Some("Launcher")),
            KeyBinding::new("up", SelectPrev, Some("Launcher")),
            KeyBinding::new("enter", Activate, Some("Launcher")),
            KeyBinding::new("ctrl-a", SelectAll, Some("Launcher")),
            KeyBinding::new("ctrl-c", Copy, Some("Launcher")),
            KeyBinding::new("ctrl-x", Cut, Some("Launcher")),
            KeyBinding::new("ctrl-v", Paste, Some("Launcher")),
            KeyBinding::new("ctrl-backspace", DeleteWordLeft, Some("Launcher")),
            KeyBinding::new("left", MoveLeft, Some("Launcher")),
            KeyBinding::new("right", MoveRight, Some("Launcher")),
            KeyBinding::new("shift-left", SelectLeft, Some("Launcher")),
            KeyBinding::new("shift-right", SelectRight, Some("Launcher")),
            KeyBinding::new("ctrl-left", MoveWordLeft, Some("Launcher")),
            KeyBinding::new("ctrl-right", MoveWordRight, Some("Launcher")),
            KeyBinding::new("ctrl-shift-left", SelectWordLeft, Some("Launcher")),
            KeyBinding::new("ctrl-shift-right", SelectWordRight, Some("Launcher")),
        ]);

        poll_native_commands(cx, native_rx, native_runtime);
    });
}

fn poll_native_commands(
    cx: &mut App,
    native_rx: mpsc::Receiver<NativeCommand>,
    native_runtime: NativeRuntime,
) {
    cx.spawn(async move |cx| {
        let _native_runtime = native_runtime;

        loop {
            while let Ok(command) = native_rx.try_recv() {
                match command {
                    NativeCommand::ToggleLauncher => {
                        let _ = cx.update(|cx| {
                            let launcher = cx.global::<LauncherGlobal>().clone_handle();
                            launcher.borrow_mut().toggle(cx);
                        });
                    }
                    NativeCommand::Quit => {
                        let _ = cx.update(|cx| cx.quit());
                        return;
                    }
                }
            }

            Timer::after(Duration::from_millis(40)).await;
        }
    })
    .detach();
}
