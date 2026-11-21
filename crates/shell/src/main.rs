#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(feature = "automation")]
mod automation;

use std::error::Error;
use std::process::ExitCode;

use tauri::{App, WebviewWindowBuilder};

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
    #[cfg(feature = "automation")]
    let automation = automation::Automation::from_env()?;

    let builder = tauri::Builder::default().invoke_handler(tauri::generate_handler![
        #[cfg(feature = "automation")]
        automation::settle_evaluation
    ]);
    #[cfg(feature = "automation")]
    let builder = automation.prepare_builder(builder);
    #[cfg(feature = "automation")]
    let window_automation = automation.clone();

    let app = builder
        .setup(move |app| {
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
    WebviewWindowBuilder::from_config(app, config)
}
