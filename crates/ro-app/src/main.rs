//! RustOverlay agent: reads the player's own screen and log, shares it with their team, and draws
//! the team over the game. We never inject into or read game memory. See docs/architecture.md.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod background;
mod format;
mod host;
mod icons;
mod interop;
mod logging;
mod session;
mod settings;
mod startup;
mod state;
mod ui;
mod views;

use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use eframe::egui::ViewportBuilder;
use ro_core::hosting::data_migration::migrate_legacy_data;
use ro_core::hosting::settings_file::{data_directory, legacy_data_directory};

use crate::app::{AgentApp, MAIN_SIZE, MAIN_TITLE, SIGN_IN_TITLE};
use crate::host::Host;
use crate::interop::message_box::show_error;

/// How long exit waits for background work, as the C# host's stop timeout did.
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

fn main() -> ExitCode {
    // Install, update and uninstall are all delivered as a relaunch of this executable with hook
    // arguments, and `run` handles the hook and exits the process. Anything above it would run
    // several extra times per update, on a process that was never going to show a window.
    startup::mark("main");
    velopack::VelopackApp::build().run();
    startup::mark("velopack");

    let data_directory = data_directory();
    let migrated = migrate_legacy_data(&legacy_data_directory(), &data_directory);
    let options = match settings::load(&data_directory) {
        Ok(options) => options,
        Err(e) => return fatal(&data_directory, &format!("The agent's settings could not be read: {e}")),
    };

    let _log = logging::init(&data_directory, &options.log_filter);
    install_panic_hook(data_directory.clone());
    tracing::info!(version = ro_net::AGENT_VERSION, "Agent starting");
    for line in &migrated {
        tracing::info!("Data migration: {line}");
    }
    interop::thread_priority::lower_rayon_pool();
    startup::mark("settings+logging");

    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().thread_name("agent-worker").build() {
        Ok(runtime) => runtime,
        Err(e) => return fatal(&data_directory, &format!("The agent could not start its worker threads: {e}")),
    };

    startup::mark("runtime");

    let host = match Host::build(data_directory.clone(), options) {
        Ok(host) => Arc::new(host),
        Err(e) => return fatal(&data_directory, &format!("The agent could not open its data directory: {e}")),
    };
    startup::mark("host");

    // A local DPAPI read rather than a call to the API: a backend outage must not stop an
    // already-enrolled user from launching. A revoked token is only discovered in use.
    let enrolled = host.enrollment.is_enrolled();
    let viewport = if enrolled {
        ViewportBuilder::default().with_title(MAIN_TITLE).with_inner_size(MAIN_SIZE)
    } else {
        ViewportBuilder::default().with_title(SIGN_IN_TITLE).with_inner_size([views::sign_in::WIDTH, 300.0]).with_resizable(false)
    };
    let native = eframe::NativeOptions { viewport: with_app_icon(viewport), centered: true, ..Default::default() };

    let handle = runtime.handle().clone();
    // TODO(port): a UI-thread panic ends the agent, where C#'s dispatcher handler kept it running; catch per frame once egui is known to survive it.
    let run = panic::catch_unwind(AssertUnwindSafe(|| {
        eframe::run_native(MAIN_TITLE, native, Box::new(move |cc| Ok(Box::new(AgentApp::new(cc, host, handle, enrolled)))))
    }));
    let Ok(result) = run else {
        // The hook has logged it and shown the box. Not waiting: an update download would outlive the window.
        runtime.shutdown_background();
        tracing::error!("Agent stopped after a panic on the UI thread");
        return ExitCode::FAILURE;
    };

    runtime.shutdown_timeout(SHUTDOWN_TIMEOUT);
    tracing::info!("Agent stopped");

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fatal(&data_directory, &format!("The agent's window could not be created: {e}")),
    }
}

/// Unset, eframe draws egui's own small logo, which Windows blurs up to taskbar size.
fn with_app_icon(viewport: ViewportBuilder) -> ViewportBuilder {
    let png = include_bytes!("../assets/icon-256.png");
    match image::load_from_memory_with_format(png, image::ImageFormat::Png) {
        Ok(icon) => {
            let icon = icon.into_rgba8();
            let (width, height) = icon.dimensions();
            viewport.with_icon(eframe::egui::IconData { rgba: icon.into_raw(), width, height })
        }
        Err(e) => {
            tracing::warn!(error = %e, "The app icon did not decode; using eframe's");
            viewport
        }
    }
}

fn fatal(data_directory: &Path, message: &str) -> ExitCode {
    tracing::error!("{message}");
    show_error(MAIN_TITLE, &format!("{message}\n\nSee {}.", data_directory.join("logs").display()));
    ExitCode::FAILURE
}

/// A panic on the UI thread is logged and shown, as the C# dispatcher's unhandled-exception box
/// did, and then ends the agent (see `main`); background threads only log, since their failures
/// are already reported where they occur.
fn install_panic_hook(data_directory: PathBuf) {
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        tracing::error!(thread = thread.name().unwrap_or("?"), "Unhandled panic: {info}");

        if thread.name() == Some("main") {
            let message = info.payload_as_str().unwrap_or("The agent hit an unexpected error.");
            show_error(MAIN_TITLE, &format!("{message}\n\nSee {}.", data_directory.join("logs").display()));
        }
        previous(info);
    }));
}
