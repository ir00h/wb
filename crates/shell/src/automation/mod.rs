#[cfg(target_os = "macos")]
mod macos;

use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::time::Duration;

use serde::Deserialize;
use tauri::webview::{PageLoadEvent, PlatformWebview};
use tauri::{App, Builder, Manager, State, WebviewWindow, WebviewWindowBuilder, Wry};
use tiny_http::{Header, Method, Request, Response, Server};

const PORT_VARIABLE: &str = "WB_AUTOMATION_PORT";
const BACKGROUND_VARIABLE: &str = "WB_AUTOMATION_BACKGROUND";
const LISTENING_ANNOUNCEMENT: &str = "AUTOMATION_LISTENING";
const SETTLE_COMMAND: &str = "settle_evaluation";
const EVALUATION_TIMEOUT: Duration = Duration::from_secs(10);
const NATIVE_TIMEOUT: Duration = Duration::from_secs(10);
const ROUTES: &str = "POST /evaluate, GET /snapshot, GET /window, POST /quit";

#[derive(Clone)]
pub struct Automation {
    channel: Option<Arc<Channel>>,
}

struct Channel {
    background: bool,
    server: Mutex<Option<Server>>,
}

impl Automation {
    pub fn from_env() -> Result<Automation, String> {
        let Ok(port) = std::env::var(PORT_VARIABLE) else {
            return Ok(Automation { channel: None });
        };
        let port: u16 = port.parse().map_err(|_| {
            format!("{PORT_VARIABLE} must be a port number from 0 to 65535, not `{port}`")
        })?;
        let background = match std::env::var(BACKGROUND_VARIABLE).as_deref() {
            Err(_) | Ok("0") => false,
            Ok("1") => true,
            Ok(other) => {
                return Err(format!(
                    "{BACKGROUND_VARIABLE} must be 1 or 0, not `{other}`"
                ));
            }
        };
        let server = Server::http((Ipv4Addr::LOCALHOST, port))
            .map_err(|error| format!("the automation channel cannot listen: {error}"))?;
        Ok(Automation {
            channel: Some(Arc::new(Channel {
                background,
                server: Mutex::new(Some(server)),
            })),
        })
    }

    fn background(&self) -> bool {
        self.channel
            .as_ref()
            .is_some_and(|channel| channel.background)
    }

    pub fn prepare_builder(&self, builder: Builder<Wry>) -> Builder<Wry> {
        let builder = builder.manage(Evaluations::default());
        #[cfg(target_os = "macos")]
        let builder = if self.background() {
            builder.activate_ignoring_other_apps(false)
        } else {
            builder
        };
        builder
    }

    pub fn prepare_window<'a>(
        &self,
        window: WebviewWindowBuilder<'a, Wry, App>,
    ) -> WebviewWindowBuilder<'a, Wry, App> {
        let Some(channel) = self.channel.clone() else {
            return window;
        };
        window
            .incognito(true)
            .focused(!channel.background)
            .on_page_load(move |window, payload| {
                if payload.event() == PageLoadEvent::Finished {
                    channel.start(window);
                }
            })
    }

    pub fn prepare_app(&self, app: App) -> App {
        #[cfg(target_os = "macos")]
        let app = {
            let mut app = app;
            if self.background() {
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            }
            app
        };
        app
    }
}

impl Channel {
    fn start(&self, window: WebviewWindow) {
        let Some(server) = self
            .server
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        else {
            return;
        };
        #[cfg(target_os = "macos")]
        if self.background {
            let _ = window.with_webview(|webview| macos::keep_rendering_when_hidden(&webview));
        }
        let address = server
            .server_addr()
            .to_ip()
            .map_or_else(String::new, |address| address.to_string());
        println!("{LISTENING_ANNOUNCEMENT} {address}");
        let background = self.background;
        std::thread::spawn(move || serve(&server, &window, background));
    }
}

#[derive(Default)]
pub struct Evaluations {
    next: AtomicU64,
    waiting: Mutex<HashMap<u64, mpsc::Sender<Settled>>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Settled {
    Returned(String),
    Threw(String),
}

#[tauri::command]
#[expect(clippy::needless_pass_by_value)]
pub fn settle_evaluation(evaluations: State<'_, Evaluations>, id: u64, settled: Settled) {
    let waiting = evaluations
        .waiting
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(&id);
    if let Some(waiting) = waiting {
        let _ = waiting.send(settled);
    }
}

enum Reply {
    Json(String),
    Png(Vec<u8>),
    Threw(String),
    Failed(String),
    Forbidden,
    NotFound,
}

impl Reply {
    fn into_response(self) -> Response<std::io::Cursor<Vec<u8>>> {
        let (status, content_type, body) = match self {
            Reply::Json(json) => (200, "application/json", json.into_bytes()),
            Reply::Png(png) => (200, "image/png", png),
            Reply::Threw(error) => (422, "text/plain", error.into_bytes()),
            Reply::Failed(error) => (500, "text/plain", error.into_bytes()),
            Reply::Forbidden => (
                403,
                "text/plain",
                b"the automation channel answers only local programs that are not web pages"
                    .to_vec(),
            ),
            Reply::NotFound => (
                404,
                "text/plain",
                format!("unknown route; the routes are {ROUTES}").into_bytes(),
            ),
        };
        let response = Response::from_data(body).with_status_code(status);
        match Header::from_bytes("content-type", content_type) {
            Ok(header) => response.with_header(header),
            Err(()) => response,
        }
    }
}

enum Route {
    Evaluate,
    Snapshot,
    Window,
    Quit,
}

impl Route {
    fn of(method: &Method, url: &str) -> Option<Route> {
        match (method, url) {
            (Method::Post, "/evaluate") => Some(Route::Evaluate),
            (Method::Get, "/snapshot") => Some(Route::Snapshot),
            (Method::Get, "/window") => Some(Route::Window),
            (Method::Post, "/quit") => Some(Route::Quit),
            _ => None,
        }
    }
}

fn serve(server: &Server, window: &WebviewWindow, background: bool) {
    for mut request in server.incoming_requests() {
        let route = Route::of(request.method(), request.url());
        let reply = if is_local_program(&request) {
            match route {
                Some(Route::Evaluate) => match read_body(&mut request) {
                    Ok(script) => evaluate(window, &script),
                    Err(reply) => reply,
                },
                Some(Route::Snapshot) => snapshot(window),
                Some(Route::Window) => window_facts(window, background),
                Some(Route::Quit) => {
                    let _ = request.respond(Reply::Json("null".to_owned()).into_response());
                    window.app_handle().exit(0);
                    return;
                }
                None => Reply::NotFound,
            }
        } else {
            Reply::Forbidden
        };
        let _ = request.respond(reply.into_response());
    }
}

fn is_local_program(request: &Request) -> bool {
    let from_loopback = request
        .remote_addr()
        .is_some_and(|address| address.ip().is_loopback());
    let from_web_page = request
        .headers()
        .iter()
        .any(|header| header.field.equiv("origin"));
    from_loopback && !from_web_page
}

fn read_body(request: &mut Request) -> Result<String, Reply> {
    let mut body = String::new();
    request
        .as_reader()
        .read_to_string(&mut body)
        .map_err(|error| Reply::Failed(format!("cannot read the request body: {error}")))?;
    Ok(body)
}

fn evaluation_script(id: u64, body: &str) -> String {
    [
        "(async () => {\n",
        "const settle = (settled) => window.__TAURI_INTERNALS__.invoke(\"",
        SETTLE_COMMAND,
        "\", { id: ",
        &id.to_string(),
        ", settled });\n",
        "try {\n",
        "const value = await (async () => {\n",
        body,
        "\n})();\n",
        "await settle({ returned: JSON.stringify(value === undefined ? null : value) });\n",
        "} catch (error) {\n",
        "await settle({ threw: error instanceof Error ? `${error}\\n${error.stack}` : String(error) });\n",
        "}\n",
        "})();\n",
    ]
    .concat()
}

fn evaluate(window: &WebviewWindow, body: &str) -> Reply {
    let evaluations = window.state::<Evaluations>();
    let id = evaluations.next.fetch_add(1, Ordering::Relaxed);
    let (settle, settled) = mpsc::channel();
    evaluations
        .waiting
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(id, settle);
    if let Err(error) = window.eval(evaluation_script(id, body)) {
        return Reply::Failed(format!("the web view did not take the script: {error}"));
    }
    match settled.recv_timeout(EVALUATION_TIMEOUT) {
        Ok(Settled::Returned(json)) => Reply::Json(json),
        Ok(Settled::Threw(error)) => Reply::Threw(error),
        Err(_) => {
            evaluations
                .waiting
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&id);
            Reply::Failed(format!(
                "the script did not settle within {} seconds",
                EVALUATION_TIMEOUT.as_secs()
            ))
        }
    }
}

fn on_main_thread<T: Send + 'static>(
    window: &WebviewWindow,
    work: impl FnOnce(&PlatformWebview, mpsc::Sender<T>) + Send + 'static,
) -> Result<T, String> {
    let (reply, replied) = mpsc::channel();
    window
        .with_webview(move |webview| work(&webview, reply))
        .map_err(|error| format!("cannot reach the web view: {error}"))?;
    replied
        .recv_timeout(NATIVE_TIMEOUT)
        .map_err(|_| "the web view did not answer".to_owned())
}

#[cfg(target_os = "macos")]
fn snapshot(window: &WebviewWindow) -> Reply {
    match on_main_thread(window, macos::snapshot).and_then(|png| png) {
        Ok(png) => Reply::Png(png),
        Err(error) => Reply::Failed(error),
    }
}

#[cfg(not(target_os = "macos"))]
fn snapshot(_window: &WebviewWindow) -> Reply {
    Reply::Failed("snapshots of the web view exist only on macOS".to_owned())
}

#[cfg(target_os = "macos")]
fn window_number(window: &WebviewWindow) -> Result<Option<isize>, String> {
    on_main_thread(window, |webview, reply| {
        let _ = reply.send(macos::window_number(webview));
    })
    .map(Some)
}

#[cfg(not(target_os = "macos"))]
fn window_number(_window: &WebviewWindow) -> Result<Option<isize>, String> {
    Ok(None)
}

fn window_facts(window: &WebviewWindow, background: bool) -> Reply {
    let facts = || -> Result<String, String> {
        let scale = window.scale_factor().map_err(|error| error.to_string())?;
        let size = window
            .inner_size()
            .map_err(|error| error.to_string())?
            .to_logical::<f64>(scale);
        let title = window.title().map_err(|error| error.to_string())?;
        let facts = serde_json::json!({
            "title": title,
            "width": size.width,
            "height": size.height,
            "scale": scale,
            "windowNumber": window_number(window)?,
            "background": background,
        });
        Ok(facts.to_string())
    };
    match facts() {
        Ok(json) => Reply::Json(json),
        Err(error) => Reply::Failed(error),
    }
}
