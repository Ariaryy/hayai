#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod apps;
mod assets;
mod calc;
mod commands;
mod files;
mod launcher;
mod native;
mod plugins;
mod scroll;
mod scry;
mod scry_updates;
mod startup;
mod theme;
mod ui_text;
mod updates;

use apps::{AppsProvider, Catalog, CatalogGlobal};
use calc::CalcProvider;
use files::{FileRecentsGlobal, FileSearchProvider};
use futures::StreamExt;
use gpui::App;
use gpui_component::{Theme, ThemeMode};
use launcher::{LauncherGlobal, LauncherState};
use native::{NativeCommand, NativeRuntime};
use plugins::{PluginRegistry, RegistryGlobal};

fn main() {
    // Register/remove the HKCU Run-key startup entry as part of Velopack's
    // install/uninstall lifecycle. `run()` exits the process itself when
    // invoked for one of these lifecycle events, so nothing below it
    // executes on a first install/uninstall pass.
    #[cfg(windows)]
    velopack::VelopackApp::build()
        // Updates are applied only through our explicit silent restart path.
        // Velopack's automatic startup path uses its non-silent apply API.
        .set_auto_apply_on_startup(false)
        .on_after_install_fast_callback(|_| {
            let _ = startup::register();
        })
        .on_before_uninstall_fast_callback(|_| {
            let _ = startup::unregister();
        })
        .run();

    let (native_tx, native_rx) = futures::channel::mpsc::unbounded();
    updates::start(native_tx.clone());
    let native_runtime = NativeRuntime::start(native_tx);

    gpui_platform::application()
        .with_assets(assets::LauncherAssets)
        .run(move |cx: &mut App| {
            // Must be called before using any gpui-component features.
            gpui_component::init(cx);
            Theme::change(ThemeMode::Dark, None, cx);
            theme::apply_launcher_theme(cx);

            LauncherState::install(cx);
            // Scan runs on the background pool (Catalog::install kicks it off);
            // icons are decoded lazily on render.
            Catalog::install(cx);
            FileRecentsGlobal::install(cx);

            let catalog = cx.global::<CatalogGlobal>().clone_handle();
            let file_recents = cx.global::<FileRecentsGlobal>().clone_handle();
            let mut registry = PluginRegistry::new();
            registry.register(AppsProvider::new(catalog));
            registry.register(FileSearchProvider::new(file_recents));
            registry.register(CalcProvider::new());
            RegistryGlobal::install(cx, registry);

            drive_native_commands(cx, native_rx, native_runtime);
        });
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
                NativeCommand::UpdateReady => {
                    cx.update(|cx| {
                        let launcher = cx.global::<LauncherGlobal>().clone_handle();
                        launcher.borrow().refresh_update(cx);
                    });
                }
                NativeCommand::ToggleLauncher => {
                    cx.update(|cx| {
                        let launcher = cx.global::<LauncherGlobal>().clone_handle();
                        launcher.borrow_mut().toggle(cx);
                    });
                }
                NativeCommand::Quit => {
                    cx.update(|cx| cx.quit());
                    return;
                }
            }
        }
    })
    .detach();
}
