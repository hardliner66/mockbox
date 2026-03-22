mod helper;
mod modules;

use rugen::rune;

use axum::{
    Router,
    body::Body,
    extract::Request,
    http::{HeaderMap, Method, StatusCode, Uri},
    response::{IntoResponse, Response as HttpResponse},
    routing::any,
};
use bytes::Bytes;
use directories::ProjectDirs;
use notify::{Event, EventKind, RecursiveMode, Watcher};
use parking_lot::RwLock;
use reqwest::Client;
#[cfg(feature = "rugen")]
use rugen::rune::alloc::clone::TryClone;
use rune::{
    Context, ContextError, Diagnostics, Module, Source, Sources, Vm,
    termcolor::{ColorChoice, StandardStream},
};
use rune::{
    Unit,
    runtime::{Object, Value},
};
use std::{collections::HashMap, fmt::Display, path::PathBuf, sync::Arc, time::SystemTime};
use tracing::{error, info};

struct AppStateShared {
    http_client: Client,
    upstream: Option<String>,
}

impl Clone for AppStateShared {
    fn clone(&self) -> Self {
        Self {
            http_client: self.http_client.clone(),
            upstream: self.upstream.clone(),
        }
    }
}

struct ScriptCache {
    context: Arc<Context>,
    unit: Arc<Unit>,
    source_path: PathBuf,
    modified_time: SystemTime,
}

struct AppStateLog {
    shared: AppStateShared,
}

impl Clone for AppStateLog {
    fn clone(&self) -> Self {
        Self {
            shared: self.shared.clone(),
        }
    }
}

#[derive(Clone)]
struct AppStateMock {
    local_script_path: PathBuf,
    global_script_path: PathBuf,
    script_cache: Arc<RwLock<Option<ScriptCache>>>,
    shared: AppStateShared,
    #[cfg(feature = "cache")]
    cache: Cache,
}

impl AppStateMock {
    fn new(script_path: Option<PathBuf>, shared: AppStateShared) -> Self {
        let global_script_path = ProjectDirs::from("com", "hardliner66", "mockbox")
            .unwrap()
            .data_local_dir()
            .join("mockbox.rn");
        let local_script_path = if let Some(path) = script_path {
            path
        } else {
            PathBuf::from("./mockbox.rn")
        };

        Self {
            local_script_path,
            global_script_path,
            script_cache: Arc::new(RwLock::new(None)),
            shared,
            #[cfg(feature = "cache")]
            cache: Cache::new(),
        }
    }
    fn get_active_script_path(&self) -> Option<PathBuf> {
        if self.local_script_path.exists() {
            Some(self.local_script_path.clone())
        } else if self.global_script_path.exists() {
            Some(self.global_script_path.clone())
        } else {
            None
        }
    }

    fn load_script(&self) -> Result<(Arc<Context>, Arc<Unit>), StatusCode> {
        let Some(active_path) = self.get_active_script_path() else {
            error!(
                "No script file found at {} or {}",
                self.local_script_path.display(),
                self.global_script_path.display()
            );
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        };

        // Check if we need to reload
        let modified_time = std::fs::metadata(&active_path)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);

        // Check cache
        {
            let script_cache = self.script_cache.read();
            if let Some(cached) = script_cache.as_ref()
                && cached.source_path == active_path
                && cached.modified_time == modified_time
            {
                // Cache hit
                return Ok((cached.context.clone(), cached.unit.clone()));
            }
        }

        // Cache miss or invalidated - reload and recompile
        let script_content =
            std::fs::read_to_string(&active_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let (context, unit) = match self.compile_rune_script(&script_content) {
            Ok(result) => result,
            Err(e) => {
                error!("Failed to compile rune script: {e}");
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        };

        // Update cache
        let context_arc = Arc::new(context);
        let unit_arc = Arc::new(unit);

        {
            let mut script_cache = self.script_cache.write();
            *script_cache = Some(ScriptCache {
                context: context_arc.clone(),
                unit: unit_arc.clone(),
                source_path: active_path,
                modified_time,
            });
        }

        Ok((context_arc, unit_arc))
    }

    #[cfg_attr(not(feature = "cache"), expect(clippy::unused_self))]
    fn compile_rune_script(&self, script: &str) -> anyhow::Result<(Context, rune::Unit)> {
        let mut context = rune_modules::default_context()?;

        context.install(module()?)?;

        let source = Source::memory(script)?;

        #[cfg(feature = "rugen")]
        context.install(rugen::module(source.try_clone()?)?)?;

        // Install cache module
        #[cfg(feature = "cache")]
        context.install(cache_module(&self.cache)?)?;

        let mut sources = Sources::new();
        sources.insert(source)?;

        let mut diagnostics = Diagnostics::new();

        let result = rune::prepare(&mut sources)
            .with_context(&context)
            .with_diagnostics(&mut diagnostics)
            .build();

        if !diagnostics.is_empty() {
            let mut writer = StandardStream::stderr(ColorChoice::Always);
            diagnostics.emit(&mut writer, &sources)?;
        }

        let unit = result?;

        Ok((context, unit))
    }
}

// impl Clone for AppStateMock {
//     fn clone(&self) -> Self {
//         Self {
//             local_script_path: self.local_script_path.clone(),
//             global_script_path: self.global_script_path.clone(),
//             script_cache: self.script_cache.clone(),
//             shared: self.shared.clone(),
//             #[cfg(feature = "cache")]
//             cache: self.cache.clone(),
//         }
//     }
// }

fn setup_file_watcher(
    cache: Arc<RwLock<Option<ScriptCache>>>,
    local_path: PathBuf,
    global_path: PathBuf,
) -> notify::Result<()> {
    use notify::Config;
    use std::sync::mpsc::channel;

    let (tx, rx) = channel();

    let mut watcher = notify::RecommendedWatcher::new(
        move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                let _ = tx.send(event);
            }
        },
        Config::default(),
    )?;

    // Watch the local script file (or its parent directory if it doesn't exist)
    if let Some(parent) = local_path.parent()
        && parent.exists()
    {
        let _ = watcher.watch(parent, RecursiveMode::NonRecursive);
    }

    // Watch the global script file (or its parent directory if it doesn't exist)
    if let Some(parent) = global_path.parent()
        && parent.exists()
    {
        let _ = watcher.watch(parent, RecursiveMode::NonRecursive);
    }

    // Spawn a thread to handle file system events
    std::thread::spawn(move || {
        // Keep watcher alive
        let _watcher = watcher;

        for event in rx {
            match event.kind {
                EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                    // Check if the event is for one of our watched files
                    let relevant = event
                        .paths
                        .iter()
                        .any(|p| p == &local_path || p == &global_path);

                    if relevant {
                        info!("Script file changed, invalidating cache");
                        let mut cache = cache.write();
                        *cache = None;
                    }
                }
                _ => {}
            }
        }
    });

    Ok(())
}

use clap::{Parser, Subcommand};

use crate::helper::to_string;
#[cfg(feature = "cache")]
use crate::modules::cache::{Cache, cache_module};

#[derive(Parser)]
struct Cli {
    /// Address to listen on
    #[clap(short, long, global = true, default_value = "127.0.0.1:3333")]
    listen: String,
    /// Optional upstream server URL to proxy requests to if not handled by the script
    #[clap(long, global = true, env = "MOCKBOX_UPSTREAM")]
    upstream: Option<String>,
    /// Optional root directory to chroot into (Unix only)
    #[cfg(target_family = "unix")]
    #[clap(long, global = true, env = "MOCKBOX_ROOT_DIR")]
    root_dir: Option<PathBuf>,
    /// Optional user to drop privileges to (Unix only)
    #[cfg(target_family = "unix")]
    #[clap(short, long, global = true, env = "MOCKBOX_USER")]
    user: Option<String>,
    /// Optional group to drop privileges to (Unix only)
    #[cfg(target_family = "unix")]
    #[clap(short, long, global = true, env = "MOCKBOX_GROUP")]
    group: Option<String>,
    /// Mode of operation
    #[command(subcommand)]
    mode: Mode,
}

#[derive(Subcommand)]
enum Mode {
    /// Run a Rune script to generate data using `RuGen`, without starting the server
    #[cfg(feature = "rugen")]
    Gen {
        #[arg(short, long)]
        pretty: bool,
        script: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Format a rune script
    #[cfg(feature = "rugen")]
    Format { script: PathBuf },
    /// Print the example script and exit
    Example {
        #[command(subcommand)]
        example_type: Option<ExampleType>,
    },
    /// Log incoming requests without running a script
    Log,
    /// Run a Rune script for each request
    Mock {
        /// Path to the Rune script to execute for each request
        script: Option<PathBuf>,
    },
}

#[derive(Default, Subcommand)]
enum ExampleType {
    #[default]
    Mock,
    #[cfg(feature = "rugen")]
    Gen,
}

#[cfg(target_family = "unix")]
fn drop_privileges(root_dir: Option<PathBuf>, user: Option<String>, group: Option<String>) {
    if is_root::is_root() {
        let mut builder = privdrop::PrivDrop::default();
        if let Some(root) = root_dir {
            builder = builder.chroot(root);
        }

        if let Some(user) = user {
            builder = builder.user(user);
        }

        if let Some(group) = group {
            builder = builder.group(group);
        }

        builder
            .fallback_to_ids_if_names_are_numeric()
            .apply()
            .unwrap_or_else(|e| panic!("Failed to drop privileges: {e}"));
    }
}

#[tokio::main]
#[expect(clippy::too_many_lines)]
async fn main() -> anyhow::Result<()> {
    let Cli {
        upstream,
        mode,
        listen,
        #[cfg(target_family = "unix")]
        root_dir,
        #[cfg(target_family = "unix")]
        user,
        #[cfg(target_family = "unix")]
        group,
    } = Cli::parse();
    // Initialize tracing
    tracing_subscriber::fmt::init();

    let app = match mode {
        Mode::Example { example_type } => {
            match example_type.unwrap_or_default() {
                ExampleType::Mock => {
                    println!("{}", include_str!("../mockbox.rn"));
                }
                #[cfg(feature = "rugen")]
                ExampleType::Gen => {
                    println!("{}", include_str!("../examples/gen_example.rn"));
                }
            }
            return Ok(());
        }

        Mode::Format { script } => {
            rugen::format_rune_script(&script).expect("Could not format script!");
            return Ok(());
        }

        Mode::Log => {
            let state = AppStateLog {
                shared: AppStateShared {
                    http_client: Client::new(),
                    upstream,
                },
            };
            let state_clone = state.clone();
            Router::new()
                .fallback(any(move |request: Request| {
                    let state = state_clone.clone();
                    async move { log_request(state, request).await }
                }))
                .into_make_service()
        }
        Mode::Mock { script } => {
            // Create shared state
            let state = AppStateMock::new(
                script,
                AppStateShared {
                    http_client: Client::new(),
                    upstream,
                },
            );

            // Set up file watcher
            if let Err(e) = setup_file_watcher(
                state.script_cache.clone(),
                state.local_script_path.clone(),
                state.global_script_path.clone(),
            ) {
                error!("Failed to set up file watcher: {}", e);
                info!(
                    "Continuing without file watching - scripts will be reloaded on every request"
                );
            } else {
                info!("File watcher initialized for script files");
            }

            // Build router using closure to capture state
            let state_clone = state.clone();
            info!("Upstream URL: {:?}", state.shared.upstream);
            Router::new()
                .fallback(any(move |request: Request| {
                    let state = state_clone.clone();
                    async move { handle_with_rune(state, request).await }
                }))
                .into_make_service()
        }

        #[cfg(feature = "rugen")]
        Mode::Gen {
            pretty,
            script,
            output,
        } => {
            let mut context = rune_modules::default_context()?;
            let source = Source::from_path(script)?;
            context.install(rugen::module(source.try_clone()?)?)?;
            let mut sources = Sources::new();
            sources.insert(source)?;
            let mut diagnostics = Diagnostics::new();

            let result = rune::prepare(&mut sources)
                .with_context(&context)
                .with_diagnostics(&mut diagnostics)
                .build();

            if !diagnostics.is_empty() {
                let mut writer = StandardStream::stderr(ColorChoice::Always);
                diagnostics.emit(&mut writer, &sources)?;
            }

            let unit = Arc::new(result?);
            let runtime = Arc::new(context.runtime()?);

            let mut vm = Vm::new(runtime.clone(), unit);

            let result = vm.call(rune::Hash::type_hash(["main"]), ())?;
            let output_string = if let Ok(string_result) = rune::from_value::<String>(&result) {
                string_result
            } else {
                let value = rugen::generate(result)?;
                if pretty {
                    serde_json::to_string_pretty(&value)?
                } else {
                    serde_json::to_string(&value)?
                }
            };
            if let Some(output_path) = output {
                std::fs::write(output_path, output_string)?;
            } else {
                println!("{output_string}");
            }
            return Ok(());
        }
    };

    info!("Starting Mockbox...");

    match listen.parse::<std::net::SocketAddr>() {
        Ok(addr) => {
            let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
            println!("listening on {}", listener.local_addr().unwrap());

            #[cfg(target_family = "unix")]
            drop_privileges(root_dir, user, group);

            axum::serve(listener, app).await?;
        }
        #[cfg(target_family = "unix")]
        Err(_) => {
            let listener = tokio::net::UnixListener::bind(&listen).unwrap();
            tokio::process::Command::new("chmod")
                .args(["777", listen.as_str()])
                .spawn()?;
            println!("listening on {:?}", listener.local_addr().unwrap());

            #[cfg(target_family = "unix")]
            drop_privileges(root_dir, user, group);

            axum::serve(listener, app).await?;
        }
        #[cfg(not(target_family = "unix"))]
        Err(e) => {
            error!("Failed to parse listen address: {e}");
            return Err(anyhow::anyhow!("Invalid listen address"));
        }
    }

    Ok(())
}

async fn log_request(state: AppStateLog, request: Request) -> HttpResponse {
    info!("{request:?}");

    if state.shared.upstream.is_none() {
        return (StatusCode::OK, "OK").into_response();
    }

    let method = request.method().clone();
    let uri = request.uri().clone();
    let headers = request.headers().clone();

    // Extract request body
    let body_bytes = match axum::body::to_bytes(request.into_body(), usize::MAX).await {
        Ok(bytes) => bytes,
        Err(e) => {
            error!("Failed to read request body: {e}");
            return (StatusCode::BAD_REQUEST, "Failed to read request body").into_response();
        }
    };

    proxy_to_upstream(state.shared, method, uri, headers, body_bytes).await
}

async fn handle_with_rune(state: AppStateMock, request: Request) -> HttpResponse {
    let method = request.method().clone();
    let uri = request.uri().clone();
    let headers = request.headers().clone();

    info!("Handling request: {method} {uri}");

    // Extract request body
    let body_bytes = match axum::body::to_bytes(request.into_body(), usize::MAX).await {
        Ok(bytes) => bytes,
        Err(e) => {
            error!("Failed to read request body: {e}");
            return (StatusCode::BAD_REQUEST, "Failed to read request body").into_response();
        }
    };

    let body_string = String::from_utf8_lossy(&body_bytes).to_string();

    // Execute rune script in blocking task to avoid Send issues
    // Pass simple strings instead of rune Values
    let method_string = method.as_str().to_string();
    let path_string = uri.path().to_string();
    let query_map = uri.query().map(|q| {
        q.split('&')
            .filter_map(|p| p.split_once('='))
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect::<HashMap<String, String>>()
    });
    let state_clone = state.clone();

    let result = tokio::task::spawn_blocking(move || {
        execute_and_parse_rune_script(
            &state_clone,
            &method_string,
            &path_string,
            &body_string,
            query_map.unwrap_or_default(),
        )
    })
    .await
    .unwrap_or_else(|e| {
        error!("Rune task panicked: {e}");
        Err(anyhow::anyhow!("Script task failed"))
    });

    // Handle result
    match result {
        Ok(Some(response_data)) => {
            // Build response from Send-safe data
            let response = HttpResponse::builder()
                .status(StatusCode::from_u16(response_data.status).unwrap_or(StatusCode::OK));

            response
                .header("Content-Type", response_data.mime_type.to_string())
                .body(Body::from(response_data.body))
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
        Ok(None) => {
            // Proxy to upstream
            info!("Rune script did not handle request, proxying to upstream");
            proxy_to_upstream(state.shared, method, uri, headers, body_bytes).await
        }
        Err(e) => {
            error!("Rune script execution failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
        }
    }
}

async fn proxy_to_upstream(
    state: AppStateShared,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> HttpResponse {
    let Some(upstream) = state.upstream else {
        return (StatusCode::BAD_GATEWAY, "No upstream server configured").into_response();
    };
    let schema = if upstream.starts_with("http://") || upstream.starts_with("https://") {
        ""
    } else {
        "http://"
    };
    let upstream_url = format!("{schema}{upstream}{uri}");

    info!("Proxying to: {upstream_url}");

    // Convert axum Method to reqwest Method
    let reqwest_method = match method.as_str() {
        "POST" => reqwest::Method::POST,
        "PUT" => reqwest::Method::PUT,
        "DELETE" => reqwest::Method::DELETE,
        "HEAD" => reqwest::Method::HEAD,
        "OPTIONS" => reqwest::Method::OPTIONS,
        "PATCH" => reqwest::Method::PATCH,
        "TRACE" => reqwest::Method::TRACE,
        _ => reqwest::Method::GET,
    };

    let mut request_builder = state.http_client.request(reqwest_method, &upstream_url);

    // Copy headers (excluding host and other problematic headers)
    for (key, value) in &headers {
        if key != "host" && key != "connection" {
            // Convert header name and value to strings and back
            if let Ok(value_str) = value.to_str() {
                request_builder = request_builder.header(key.as_str(), value_str);
            }
        }
    }

    // Add body if present
    if !body.is_empty() {
        request_builder = request_builder.body(body.to_vec());
    }

    match request_builder.send().await {
        Ok(resp) => {
            let status_code = resp.status().as_u16();
            let resp_headers = resp.headers().clone();
            let body_bytes = resp.bytes().await.unwrap_or_default();

            let mut response = HttpResponse::builder()
                .status(StatusCode::from_u16(status_code).unwrap_or(StatusCode::OK));

            // Copy response headers
            for (key, value) in &resp_headers {
                if let Ok(value_str) = value.to_str() {
                    response = response.header(key.as_str(), value_str);
                }
            }

            response.body(Body::from(body_bytes)).unwrap()
        }
        Err(e) => {
            error!("Failed to proxy request: {e:#?}");
            (
                StatusCode::BAD_GATEWAY,
                format!("Failed to reach upstream server: {e}"),
            )
                .into_response()
        }
    }
}

#[derive(Debug)]
enum MimeType {
    TextPlain,
    ApplicationJson,
}

impl Display for MimeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                MimeType::TextPlain => "text/plain",
                MimeType::ApplicationJson => "application/json",
            }
        )
    }
}

#[derive(Debug)]
struct ResponseData {
    status: u16,
    body: String,
    mime_type: MimeType,
}

fn execute_and_parse_rune_script(
    state: &AppStateMock,
    method: &str,
    path: &str,
    body: &str,
    query: HashMap<String, String>,
) -> anyhow::Result<Option<ResponseData>> {
    // Build rune request data inside this non-async context
    let mut request_data = Object::new();

    // Convert strings to rune strings
    let method_str = rune::alloc::String::try_from(method)?;
    let path_str = rune::alloc::String::try_from(path)?;
    let body_str = rune::alloc::String::try_from(body)?;

    // Insert into object
    request_data.insert(
        rune::alloc::String::try_from("method")?,
        rune::to_value(method_str)?,
    )?;

    request_data.insert(
        rune::alloc::String::try_from("path")?,
        rune::to_value(path_str)?,
    )?;

    request_data.insert(
        rune::alloc::String::try_from("query")?,
        rune::to_value(query)?,
    )?;

    request_data.insert(
        rune::alloc::String::try_from("body")?,
        rune::to_value(body_str)?,
    )?;

    let request = Value::new(request_data)?;
    let (context, unit) = match state.load_script() {
        Ok(result) => result,
        Err(e) => {
            error!("Failed to load script: {e}");
            return Err(anyhow::anyhow!("Failed to load script"));
        }
    };

    let runtime = Arc::new(context.runtime()?);

    let mut vm = Vm::new(runtime.clone(), unit);

    let result = vm.call(rune::Hash::type_hash(["handle_request"]), (request,))?;

    if let Ok(()) = rune::from_value(&result) {
        return Ok(None);
    }

    if let Ok((status, body)) = rune::from_value::<(u16, Value)>(&result) {
        let response = if let Ok(body) = rune::from_value(&body) {
            ResponseData {
                status,
                body,
                mime_type: MimeType::TextPlain,
            }
        } else {
            ResponseData {
                status,
                mime_type: MimeType::ApplicationJson,
                #[cfg(feature = "rugen")]
                body: serde_json::to_string(&rugen::generate(result)?)?,
                #[cfg(not(feature = "rugen"))]
                body: serde_json::to_string(&body)?,
            }
        };
        return Ok(Some(response));
    }

    if let Ok(body) = rune::from_value::<String>(&result) {
        return Ok(Some(ResponseData {
            status: 200,
            body,
            mime_type: MimeType::TextPlain,
        }));
    }

    Ok(Some(ResponseData {
        status: 200,
        #[cfg(feature = "rugen")]
        body: serde_json::to_string(&rugen::generate(result)?)?,
        #[cfg(not(feature = "rugen"))]
        body: serde_json::to_string(&result)?,
        mime_type: MimeType::ApplicationJson,
    }))
}

#[rune::function(instance)]
fn parts(value: &str) -> Vec<String> {
    value
        .split('/')
        .filter(|s| !s.is_empty())
        .map(to_string)
        .collect()
}

fn module() -> Result<Module, ContextError> {
    let mut m = Module::new();
    m.function("cfg", |key: &str| {
        (cfg!(feature = "cache") && key == "cache") || (cfg!(feature = "rugen") && key == "rugen")
    })
    .build()?;
    m.function_meta(parts)?;
    Ok(m)
}
