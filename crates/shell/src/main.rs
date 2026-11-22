#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(feature = "automation")]
mod automation;
mod closing;
mod host;
#[cfg(target_os = "macos")]
mod macos;
mod settings;
mod watch;

use std::error::Error;
use std::process::ExitCode;
use std::sync::Arc;

use tauri::{App, AppHandle, Emitter, Manager, RunEvent, WebviewWindowBuilder, WindowEvent};
use wb_core::Workspace;

use crate::closing::{Closing, ExitRequest, Settle};
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
            let asking = app.handle().clone();
            let settling = app.handle().clone();
            let closing = Closing::new(
                move || {
                    let _ = asking.emit(wb_model::CLOSE_REQUESTED_EVENT, ());
                },
                move |settle| settle_close(&settling, settle),
                closing::WINDOW_ANSWER_LIMIT,
            );
            #[cfg(target_os = "macos")]
            macos::route_quit_requests(&closing)?;
            app.manage(Arc::clone(&closing));
            app.manage(Arc::new(Host::new(
                workspace,
                settings.board,
                app.handle().clone(),
                closing,
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

    app.run(follow_close_requests);
    Ok(())
}

fn follow_close_requests(app: &AppHandle, event: RunEvent) {
    let closing = || Arc::clone(&app.state::<Arc<Closing>>());
    match event {
        RunEvent::ExitRequested { api, .. } => {
            if let ExitRequest::Prevent = closing().exit_requested() {
                api.prevent_exit();
            }
        }
        RunEvent::WindowEvent {
            event: WindowEvent::CloseRequested { api, .. },
            ..
        } => {
            api.prevent_close();
            closing().ask_window();
        }
        _ => {}
    }
}

fn settle_close(app: &AppHandle, settle: Settle) {
    match settle {
        Settle::Exit => app.exit(0),
        #[cfg(target_os = "macos")]
        Settle::ReplyToTerminate(terminate) => macos::reply_to_terminate(app, terminate),
        #[cfg(not(target_os = "macos"))]
        Settle::ReplyToTerminate(_) => {}
    }
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
