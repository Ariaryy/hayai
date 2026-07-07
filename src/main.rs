#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod apps;
mod commands;
mod launcher;
mod native;
mod plugins;

use futures::StreamExt;
use gpui::{App, rgb};
use gpui_component::{Theme, ThemeMode, ThemeTokens};
use apps::Catalog;
use launcher::{LauncherGlobal, LauncherState};
use native::{NativeCommand, NativeRuntime};

fn main() {
    let (native_tx, native_rx) = futures::channel::mpsc::unbounded();
    let native_runtime = NativeRuntime::start(native_tx);

    gpui_platform::application().run(move |cx: &mut App| {
        // Must be called before using any gpui-component features.
        gpui_component::init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
        apply_raycast_theme(cx);

        LauncherState::install(cx);
        // Scan runs on the background pool (Catalog::install kicks it off);
        // icons are decoded lazily on render.
        Catalog::install(cx);

        drive_native_commands(cx, native_rx, native_runtime);
    });
}

/// Override gpui-component's default (blue-accented) dark theme with a
/// neutral, Raycast-like grey palette — in particular, the selected-row
/// highlight and its border both become the same grey so there's no blue
/// outline on selection.
fn apply_raycast_theme(cx: &mut App) {
    let theme = Theme::global_mut(cx);

    let surface = rgb(0x2f2f34).into();
    theme.background = rgb(0x1c1c1e).into();
    theme.foreground = rgb(0xf2f2f2).into();
    theme.border = rgb(0x343437).into();
    theme.muted_foreground = rgb(0x9a9a9e).into();
    theme.accent = surface;
    theme.list_active = surface;
    theme.list_active_border = surface;
    theme.list_hover = rgb(0x27272a).into();
    theme.list.active_highlight = true;

    // `tokens` is a derived snapshot of `colors` for hot paths (e.g. the
    // ListItem hover fill reads `tokens.list_hover`, not `colors.list_hover`
    // directly), so it must be regenerated after mutating colors above.
    theme.tokens = ThemeTokens::from(&theme.colors);
}

fn drive_native_commands(
    cx: &mut App,
    mut native_rx: futures::channel::mpsc::UnboundedReceiver<NativeCommand>,
    native_runtime: NativeRuntime,
) {
    cx.spawn(async move |cx| {
        let _native_runtime = native_runtime;

        // Wakes only when the native thread actually sends a command — no
        // periodic polling. Ends when the native thread (sender) goes away.
        while let Some(command) = native_rx.next().await {
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
    })
    .detach();
}
