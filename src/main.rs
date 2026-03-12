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
use reqwest::Client;
use rune::{
    Context, ContextError, Diagnostics, Module, Source, Sources, Vm,
    termcolor::{ColorChoice, StandardStream},
};
use rune::{
    Unit,
    runtime::{Object, Value},
};
use std::{
    fmt::Display,
    path::{Path, PathBuf},
    sync::Arc,
};
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

struct AppStateMock {
    script_path: Option<PathBuf>,
    shared: AppStateShared,
}

impl Clone for AppStateMock {
    fn clone(&self) -> Self {
        Self {
            script_path: self.script_path.clone(),
            shared: self.shared.clone(),
        }
    }
}

use clap::{Parser, Subcommand};

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
    /// Print the example script and exit
    Example,
    /// Log incoming requests without running a script
    Log,
    /// Run a Rune script for each request
    Mock {
        /// Path to the Rune script to execute for each request
        script: Option<PathBuf>,
    },
}

fn load_script<P: AsRef<Path>>(path: P) -> Result<(Context, Unit), StatusCode> {
    let path = path.as_ref();
    // Load rune script
    let Ok(script_content) = std::fs::read_to_string(path) else {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    };

    // Compile rune script
    let (context, unit) = match compile_rune_script(&script_content) {
        Ok(result) => result,
        Err(e) => {
            error!("Failed to compile rune script: {e}");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    Ok((context, unit))
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
        Mode::Example => {
            println!("{}", include_str!("../mockbox.rn"));
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
            let state = AppStateMock {
                script_path: script,
                shared: AppStateShared {
                    http_client: Client::new(),
                    upstream,
                },
            };
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
    let state_clone = state.clone();

    let result = tokio::task::spawn_blocking(move || {
        execute_and_parse_rune_script(&state_clone, &method_string, &path_string, &body_string)
    })
    .await
    .unwrap_or_else(|e| {
        error!("Rune task panicked: {e}");
        Err("Script task failed".to_string())
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

fn compile_rune_script(script: &str) -> Result<(Context, rune::Unit), String> {
    let mut context =
        rune_modules::default_context().map_err(|e| format!("Failed to create context: {e}"))?;

    context
        .install(module().map_err(to_string)?)
        .map_err(to_string)?;

    let mut sources = Sources::new();
    sources
        .insert(Source::memory(script).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;

    let mut diagnostics = Diagnostics::new();

    let result = rune::prepare(&mut sources)
        .with_context(&context)
        .with_diagnostics(&mut diagnostics)
        .build();

    if !diagnostics.is_empty() {
        let mut writer = StandardStream::stderr(ColorChoice::Always);
        diagnostics
            .emit(&mut writer, &sources)
            .map_err(|e| format!("Failed to emit diagnostics: {e}"))?;

        return Err("Script compilation failed".to_string());
    }

    let unit = result.map_err(|e| format!("Build failed: {e}"))?;

    Ok((context, unit))
}

#[derive(Debug)]
enum MimeType {
    TextPlain,
    ApplicationJson,
}

impl ToString for MimeType {
    fn to_string(&self) -> String {
        match self {
            MimeType::TextPlain => "text/plain".to_string(),
            MimeType::ApplicationJson => "application/json".to_string(),
        }
    }
}

#[derive(Debug)]
struct ResponseData {
    status: u16,
    body: String,
    mime_type: MimeType,
}

fn to_string<T: Display>(value: T) -> String {
    value.to_string()
}

fn execute_and_parse_rune_script(
    state: &AppStateMock,
    method: &str,
    path: &str,
    body: &str,
) -> Result<Option<ResponseData>, String> {
    // Build rune request data inside this non-async context
    let mut request_data = Object::new();

    // Convert strings to rune strings
    let method_str = rune::alloc::String::try_from(method)
        .map_err(|e| format!("Failed to allocate method string: {e}"))?;
    let path_str = rune::alloc::String::try_from(path)
        .map_err(|e| format!("Failed to allocate path string: {e}"))?;
    let body_str = rune::alloc::String::try_from(body)
        .map_err(|e| format!("Failed to allocate body string: {e}"))?;

    // Insert into object
    request_data
        .insert(
            rune::alloc::String::try_from("method").map_err(to_string)?,
            Value::new(method_str).map_err(to_string)?,
        )
        .map_err(|e| format!("Failed to insert method: {e}"))?;

    request_data
        .insert(
            rune::alloc::String::try_from("path").map_err(to_string)?,
            Value::new(path_str).map_err(to_string)?,
        )
        .map_err(|e| format!("Failed to insert path: {e}"))?;

    request_data
        .insert(
            rune::alloc::String::try_from("body").map_err(to_string)?,
            Value::new(body_str).map_err(to_string)?,
        )
        .map_err(|e| format!("Failed to insert body: {e}"))?;

    let request = Value::new(request_data).map_err(to_string)?;

    let (context, unit) = if let Some(path) = state.script_path.as_ref() {
        load_script(path)
    } else {
        if std::fs::exists("./mockbox.rn").unwrap() {
            load_script("./mockbox.rn")
        } else {
            load_script(
                ProjectDirs::from("com", "hardliner66", "mockbox")
                    .unwrap()
                    .data_local_dir()
                    .join("mockbox.rn"),
            )
        }
    }
    .map_err(|e| format!("Failed to load script: {e}"))?;

    let runtime = Arc::new(context.runtime().map_err(to_string)?);

    let mut vm = Vm::new(runtime.clone(), Arc::new(unit));

    let result = vm
        .call(rune::Hash::type_hash(["handle_request"]), (request,))
        .map_err(|e| format!("Execution error: {e}"))?;

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
                body: serde_json::to_string(&body)
                    .map_err(|e| format!("Failed to serialize response object: {e}"))?,
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

    // Parse response
    return Ok(Some(ResponseData {
        status: 200,
        body: serde_json::to_string(&result)
            .map_err(|e| format!("Invalid response object: {e}"))?,
        mime_type: MimeType::ApplicationJson,
    }));
}

#[rune::function(instance)]
fn parts(value: String) -> Vec<String> {
    value
        .split('/')
        .filter(|s| !s.is_empty())
        .map(to_string)
        .collect()
}

fn module() -> Result<Module, ContextError> {
    let mut m = Module::new();
    m.function_meta(parts)?;
    Ok(m)
}
