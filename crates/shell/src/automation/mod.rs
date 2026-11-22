#[cfg(target_os = "macos")]
mod macos;

use std::collections::HashMap;
use std::io::Write;
use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::webview::PageLoadEvent;
use tauri::{App, Builder, Manager, State, WebviewWindow, WebviewWindowBuilder, Wry};
use tiny_http::{Header, Method, Request, Response, Server};
use wb_model::Op;

const PORT_VARIABLE: &str = "WB_AUTOMATION_PORT";
const BACKGROUND_VARIABLE: &str = "WB_AUTOMATION_BACKGROUND";
const RUNNER_VARIABLE: &str = "WB_AUTOMATION_RUNNER";
const LISTENING_ANNOUNCEMENT: &str = "AUTOMATION_LISTENING";
const SETTLE_COMMAND: &str = "settle_evaluation";
const EVALUATION_TIMEOUT: Duration = Duration::from_secs(10);
const ROUTES: &str = "POST /evaluate, POST /type, POST /key, POST /click, GET /snapshot, GET /window, GET /applied, POST /close, POST /terminate, POST /quit";
#[cfg(unix)]
const RUNNER_POLL: Duration = Duration::from_millis(200);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Applied {
    kind: &'static str,
    note: Option<String>,
}

#[derive(Serialize)]
struct AppliedCount {
    #[serde(flatten)]
    applied: Applied,
    count: u32,
}

static APPLIED: Mutex<Vec<(Applied, u32)>> = Mutex::new(Vec::new());

impl Applied {
    pub fn of(op: &Op) -> Applied {
        match op {
            Op::AddSection { .. } => Applied {
                kind: "addSection",
                note: None,
            },
            Op::AddNote { .. } => Applied {
                kind: "addNote",
                note: None,
            },
            Op::EditNote { note, .. } => Applied {
                kind: "editNote",
                note: Some(note.to_string()),
            },
        }
    }

    pub fn record(self) {
        let mut counts = APPLIED.lock().unwrap_or_else(PoisonError::into_inner);
        match counts.iter_mut().find(|(applied, _)| *applied == self) {
            Some((_, count)) => *count += 1,
            None => counts.push((self, 1)),
        }
    }
}

fn applied_counts() -> Reply {
    let counts: Vec<AppliedCount> = APPLIED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .map(|(applied, count)| AppliedCount {
            applied: applied.clone(),
            count: *count,
        })
        .collect();
    match serde_json::to_string(&counts) {
        Ok(json) => Reply::Json(json),
        Err(error) => Reply::Failed(error.to_string()),
    }
}

#[derive(Clone)]
pub struct Automation {
    channel: Option<Arc<Channel>>,
}

struct Channel {
    background: bool,
    server: Mutex<Option<Server>>,
}

#[derive(Debug, PartialEq, Eq)]
struct Settings {
    port: u16,
    background: bool,
}

fn runner_pid(runner: Option<&str>) -> Result<Option<i32>, String> {
    let Some(runner) = runner else {
        return Ok(None);
    };
    match runner.parse::<i32>() {
        Ok(pid) if pid > 1 => Ok(Some(pid)),
        _ => Err(format!(
            "{RUNNER_VARIABLE} must be the process id of the program that runs the app, not `{runner}`"
        )),
    }
}

fn settings(port: Option<&str>, background: Option<&str>) -> Result<Option<Settings>, String> {
    let background = match background {
        None | Some("0") => false,
        Some("1") => true,
        Some(other) => {
            return Err(format!(
                "{BACKGROUND_VARIABLE} must be 1 or 0, not `{other}`"
            ));
        }
    };
    let Some(port) = port else {
        if background {
            return Err(format!(
                "{BACKGROUND_VARIABLE}=1 needs {PORT_VARIABLE} too, and the app does not start in the foreground instead"
            ));
        }
        return Ok(None);
    };
    let port = port.parse().map_err(|_| {
        format!("{PORT_VARIABLE} must be a port number from 0 to 65535, not `{port}`")
    })?;
    Ok(Some(Settings { port, background }))
}

fn variable(name: &str) -> Result<Option<String>, String> {
    match std::env::var(name) {
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => Err(format!("{name} is not valid Unicode")),
    }
}

impl Automation {
    pub fn from_env() -> Result<Automation, String> {
        let port = variable(PORT_VARIABLE)?;
        let background = variable(BACKGROUND_VARIABLE)?;
        let runner = runner_pid(variable(RUNNER_VARIABLE)?.as_deref())?;
        let Some(Settings { port, background }) = settings(port.as_deref(), background.as_deref())?
        else {
            return Ok(Automation { channel: None });
        };
        if background && !cfg!(target_os = "macos") {
            return Err(format!(
                "{BACKGROUND_VARIABLE}=1 works only on macOS; leave it unset on this system"
            ));
        }
        let server = Server::http((Ipv4Addr::LOCALHOST, port))
            .map_err(|error| format!("the automation channel cannot listen: {error}"))?;
        #[cfg(unix)]
        exit_with_runner(runner);
        #[cfg(not(unix))]
        let _ = runner;
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
        if self.background() {
            never_activate(builder)
        } else {
            builder
        }
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

    pub fn prepare_app(&self, mut app: App) -> App {
        if self.background() {
            stay_out_of_the_dock(&mut app);
        }
        app
    }
}

#[cfg(target_os = "macos")]
fn never_activate(builder: Builder<Wry>) -> Builder<Wry> {
    builder.activate_ignoring_other_apps(false)
}

#[cfg(not(target_os = "macos"))]
fn never_activate(builder: Builder<Wry>) -> Builder<Wry> {
    builder
}

#[cfg(target_os = "macos")]
fn stay_out_of_the_dock(app: &mut App) {
    app.set_activation_policy(tauri::ActivationPolicy::Accessory);
}

#[cfg(not(target_os = "macos"))]
fn stay_out_of_the_dock(_app: &mut App) {}

#[cfg(unix)]
fn exit_with_runner(runner: Option<i32>) {
    let watched = runner
        .unwrap_or_else(|| i32::try_from(std::os::unix::process::parent_id()).unwrap_or_default());
    std::thread::spawn(move || {
        while is_running(watched) {
            std::thread::sleep(RUNNER_POLL);
        }
        let _ = writeln!(
            std::io::stderr(),
            "Whiteboard exits because the program that started its automation channel has ended"
        );
        std::process::exit(1);
    });
}

#[cfg(unix)]
fn is_running(pid: i32) -> bool {
    let Some(pid) = rustix::process::Pid::from_raw(pid) else {
        return false;
    };
    match rustix::process::test_kill_process(pid) {
        Ok(()) => true,
        Err(error) => error != rustix::io::Errno::SRCH,
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
        let background = self.background;
        #[cfg(target_os = "macos")]
        if background {
            let listening_window = window.clone();
            let keep_rendering = move |webview| match macos::keep_rendering_when_hidden(&webview) {
                Ok(()) => listen(server, listening_window, background),
                Err(reason) => refuse_background(&reason),
            };
            if let Err(error) = window.with_webview(keep_rendering) {
                refuse_background(&format!("the web view cannot be reached: {error}"));
            }
            return;
        }
        listen(server, window, background);
    }
}

#[cfg(target_os = "macos")]
fn refuse_background(reason: &str) -> ! {
    eprintln!(
        "Whiteboard could not start: background mode cannot keep the hidden window rendering, because {reason}."
    );
    std::process::exit(1);
}

fn listen(server: Server, window: WebviewWindow, background: bool) {
    let address = server
        .server_addr()
        .to_ip()
        .map_or_else(String::new, |address| address.to_string());
    announce(&mut std::io::stdout().lock(), &address);
    std::thread::spawn(move || serve(&server, &window, background));
}

fn announce(out: &mut impl Write, address: &str) {
    let _ = writeln!(out, "{LISTENING_ANNOUNCEMENT} {address}").and_then(|()| out.flush());
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
    #[cfg(target_os = "macos")]
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
            #[cfg(target_os = "macos")]
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
    Type,
    Key,
    Click,
    Snapshot,
    Window,
    Applied,
    Close,
    Terminate,
    Quit,
}

impl Route {
    fn of(method: &Method, url: &str) -> Option<Route> {
        match (method, url) {
            (Method::Post, "/evaluate") => Some(Route::Evaluate),
            (Method::Post, "/type") => Some(Route::Type),
            (Method::Post, "/key") => Some(Route::Key),
            (Method::Post, "/click") => Some(Route::Click),
            (Method::Get, "/snapshot") => Some(Route::Snapshot),
            (Method::Get, "/window") => Some(Route::Window),
            (Method::Get, "/applied") => Some(Route::Applied),
            (Method::Post, "/close") => Some(Route::Close),
            (Method::Post, "/terminate") => Some(Route::Terminate),
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
                Some(Route::Type) => match read_body(&mut request) {
                    Ok(text) => type_text(window, text),
                    Err(reply) => reply,
                },
                Some(Route::Key) => match read_body(&mut request) {
                    Ok(name) => press(window, &name),
                    Err(reply) => reply,
                },
                Some(Route::Click) => match read_body(&mut request) {
                    Ok(body) => click(window, &body),
                    Err(reply) => reply,
                },
                Some(Route::Snapshot) => snapshot(window),
                Some(Route::Window) => window_facts(window, background),
                Some(Route::Applied) => applied_counts(),
                Some(Route::Close) => match window.close() {
                    Ok(()) => Reply::Json("null".to_owned()),
                    Err(error) => Reply::Failed(error.to_string()),
                },
                Some(Route::Terminate) => terminate(window),
                Some(Route::Quit) => {
                    window.app_handle().exit(0);
                    Reply::Json("null".to_owned())
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

#[cfg(target_os = "macos")]
fn on_main_thread<T: Send + 'static>(
    window: &WebviewWindow,
    work: impl FnOnce(&tauri::webview::PlatformWebview, mpsc::Sender<T>) + Send + 'static,
) -> Result<T, String> {
    const NATIVE_TIMEOUT: Duration = Duration::from_secs(10);
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
#[derive(Deserialize)]
struct ClickRequest {
    x: f64,
    y: f64,
    count: Option<isize>,
}

#[cfg(target_os = "macos")]
const KEYS: [(&str, macos::Key); 4] = [
    (
        "Escape",
        macos::Key {
            code: 53,
            characters: "\u{1b}",
        },
    ),
    (
        "Enter",
        macos::Key {
            code: 36,
            characters: "\r",
        },
    ),
    (
        "Tab",
        macos::Key {
            code: 48,
            characters: "\t",
        },
    ),
    (
        "Backspace",
        macos::Key {
            code: 51,
            characters: "\u{7f}",
        },
    ),
];

#[cfg(target_os = "macos")]
fn done(outcome: Result<Result<(), String>, String>) -> Reply {
    match outcome.and_then(|done| done) {
        Ok(()) => Reply::Json("null".to_owned()),
        Err(error) => Reply::Failed(error),
    }
}

#[cfg(target_os = "macos")]
fn type_text(window: &WebviewWindow, text: String) -> Reply {
    done(on_main_thread(window, move |webview, reply| {
        let _ = reply.send(macos::type_text(webview, &text));
    }))
}

#[cfg(target_os = "macos")]
fn press(window: &WebviewWindow, name: &str) -> Reply {
    let Some(index) = KEYS.iter().position(|(known, _)| *known == name.trim()) else {
        let names: Vec<&str> = KEYS.iter().map(|(known, _)| *known).collect();
        return Reply::Failed(format!(
            "`{name}` is not a key the channel presses; the keys are {}",
            names.join(", ")
        ));
    };
    done(on_main_thread(window, move |webview, reply| {
        let _ = reply.send(macos::press(webview, &KEYS[index].1));
    }))
}

#[cfg(target_os = "macos")]
fn click(window: &WebviewWindow, body: &str) -> Reply {
    let request: ClickRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => {
            return Reply::Failed(format!(
                "a click is {{\"x\": <number>, \"y\": <number>, \"count\": <number>}}: {error}"
            ));
        }
    };
    let at = macos::Click {
        x: request.x,
        y: request.y,
        count: request.count.unwrap_or(1).max(1),
    };
    done(on_main_thread(window, move |webview, reply| {
        let _ = reply.send(macos::click(webview, &at));
    }))
}

#[cfg(target_os = "macos")]
fn terminate(window: &WebviewWindow) -> Reply {
    done(on_main_thread(window, |_, reply| {
        let _ = reply.send(Ok(()));
        macos::terminate();
    }))
}

#[cfg(not(target_os = "macos"))]
fn terminate(_window: &WebviewWindow) -> Reply {
    Reply::Failed("asking the app to quit as the menu does exists only on macOS".to_owned())
}

#[cfg(not(target_os = "macos"))]
fn type_text(_window: &WebviewWindow, _text: String) -> Reply {
    Reply::Failed("typing through the channel exists only on macOS".to_owned())
}

#[cfg(not(target_os = "macos"))]
fn press(_window: &WebviewWindow, _name: &str) -> Reply {
    Reply::Failed("pressing keys through the channel exists only on macOS".to_owned())
}

#[cfg(not(target_os = "macos"))]
fn click(_window: &WebviewWindow, _body: &str) -> Reply {
    Reply::Failed("clicking through the channel exists only on macOS".to_owned())
}

#[cfg(target_os = "macos")]
fn window_number(window: &WebviewWindow) -> Result<isize, String> {
    on_main_thread(window, |webview, reply| {
        let _ = reply.send(macos::window_number(webview));
    })?
}

fn window_facts(window: &WebviewWindow, background: bool) -> Reply {
    let facts = || -> Result<String, String> {
        let scale = window.scale_factor().map_err(|error| error.to_string())?;
        let size = window
            .inner_size()
            .map_err(|error| error.to_string())?
            .to_logical::<f64>(scale);
        let title = window.title().map_err(|error| error.to_string())?;
        let visible = window.is_visible().map_err(|error| error.to_string())?;
        #[cfg(target_os = "macos")]
        let window_number = Some(window_number(window)?);
        #[cfg(not(target_os = "macos"))]
        let window_number: Option<isize> = None;
        let facts = serde_json::json!({
            "title": title,
            "width": size.width,
            "height": size.height,
            "scale": scale,
            "visible": visible,
            "windowNumber": window_number,
            "background": background,
        });
        Ok(facts.to_string())
    };
    match facts() {
        Ok(json) => Reply::Json(json),
        Err(error) => Reply::Failed(error),
    }
}

#[cfg(test)]
mod tests {
    use super::{Settings, announce, runner_pid, settings};
    use std::io::{self, Write};

    struct ClosedPipe;

    impl Write for ClosedPipe {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::from(io::ErrorKind::BrokenPipe))
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::from(io::ErrorKind::BrokenPipe))
        }
    }

    #[test]
    fn announcing_to_a_closed_pipe_does_not_abort() {
        announce(&mut ClosedPipe, "127.0.0.1:1");
        let mut written = Vec::new();
        announce(&mut written, "127.0.0.1:2");
        assert_eq!(written, b"AUTOMATION_LISTENING 127.0.0.1:2\n");
    }

    #[test]
    fn the_runner_is_a_process_id() {
        assert_eq!(runner_pid(None), Ok(None));
        assert_eq!(runner_pid(Some("4242")), Ok(Some(4242)));
        assert!(runner_pid(Some("1")).is_err());
        assert!(runner_pid(Some("node")).is_err());
    }

    #[test]
    fn the_channel_is_off_without_a_port() {
        assert_eq!(settings(None, None), Ok(None));
        assert_eq!(settings(None, Some("0")), Ok(None));
    }

    #[test]
    fn background_mode_without_a_port_is_refused() {
        let refused = settings(None, Some("1")).expect_err("background mode needs a port");
        assert!(refused.contains("WB_AUTOMATION_PORT"), "{refused}");
    }

    #[test]
    fn a_port_turns_the_channel_on_in_the_mode_asked_for() {
        assert_eq!(
            settings(Some("0"), Some("1")),
            Ok(Some(Settings {
                port: 0,
                background: true
            }))
        );
        assert_eq!(
            settings(Some("4870"), None),
            Ok(Some(Settings {
                port: 4870,
                background: false
            }))
        );
    }

    #[test]
    fn values_that_are_not_understood_are_refused() {
        assert!(settings(Some("http"), None).is_err());
        assert!(settings(Some("0"), Some("yes")).is_err());
        assert!(settings(None, Some("yes")).is_err());
    }
}
