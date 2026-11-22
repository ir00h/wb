#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(feature = "automation")]
mod automation;
mod host;
mod settings;
mod watch;

use std::error::Error;
use std::process::ExitCode;
use std::sync::Arc;

use tauri::{App, Manager, WebviewWindowBuilder};
use wb_core::Workspace;

use crate::host::Host;
use crate::settings::{BOARD_VARIABLE, WORKSPACE_VARIABLE, settings};

const MAIN_WINDOW: &str = "main";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Whiteboard could not start: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let settings = settings(
        std::env::args_os().skip(1),
        std::env::var_os(WORKSPACE_VARIABLE),
        std::env::var_os(BOARD_VARIABLE),
    )?;
    let workspace = Workspace::open(&settings.workspace)?;

    #[cfg(feature = "automation")]
    let automation = automation::Automation::from_env()?;

    let builder = tauri::Builder::default().invoke_handler(tauri::generate_handler![
        host::call,
        #[cfg(feature = "automation")]
        automation::settle_evaluation
    ]);
    #[cfg(feature = "automation")]
    let builder = automation.prepare_builder(builder);
    #[cfg(feature = "automation")]
    let window_automation = automation.clone();

    let app = builder
        .setup(move |app| {
            app.manage(Arc::new(Host::new(
                workspace,
                settings.board,
                app.handle().clone(),
            )));
            let window = main_window(app)?;
            #[cfg(feature = "automation")]
            let window = window_automation.prepare_window(window);
            window.build()?;
            Ok(())
        })
        .build(tauri::generate_context!())?;

    #[cfg(feature = "automation")]
    let app = automation.prepare_app(app);

    app.run(|_, _| {});
    Ok(())
}

fn main_window(app: &App) -> tauri::Result<WebviewWindowBuilder<'_, tauri::Wry, App>> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == MAIN_WINDOW)
        .ok_or(tauri::Error::WindowNotFound)?;
    let window = WebviewWindowBuilder::from_config(app, config)?;
    #[cfg(target_os = "macos")]
    let window = window.traffic_light_position(tauri::LogicalPosition::new(
        wb_model::TITLE_BAR.buttons_x,
        wb_model::TITLE_BAR.buttons_y,
    ));
    Ok(window)
}
