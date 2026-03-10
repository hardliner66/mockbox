use axum::{
    Router,
    body::Body,
    extract::Request,
    http::{HeaderMap, Method, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::any,
};
use bytes::Bytes;
use reqwest::Client;
use rune::{
    Context, Diagnostics, Source, Sources, Vm,
    termcolor::{ColorChoice, StandardStream},
};
use rune::{
    Unit,
    runtime::{Object, Value},
};
use std::{fmt::Display, path::PathBuf, sync::Arc};
use tracing::{error, info};

struct AppState {
    script_path: PathBuf,
    http_client: Client,
    upstream: Option<String>,
}

impl Clone for AppState {
    fn clone(&self) -> Self {
        Self {
            script_path: self.script_path.clone(),
            http_client: self.http_client.clone(),
            upstream: self.upstream.clone(),
        }
    }
}
use clap::{Parser, Subcommand};

#[derive(Parser)]
struct Cli {
    #[clap(short, long, global = true, default_value = "127.0.0.1:3333")]
    listen: String,
    #[clap(short, long, global = true, env = "MOCKBOX_UPSTREAM")]
    upstream: Option<String>,
    #[command(subcommand)]
    mode: Mode,
}

#[derive(Subcommand)]
enum Mode {
    Log,
    Mock { script: PathBuf },
}

fn load_script(path: &PathBuf) -> Result<(Context, Unit), StatusCode> {
    // Load rune script
    let script_content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => {
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Compile rune script
    let (context, unit) = match compile_rune_script(&script_content) {
        Ok(result) => result,
        Err(e) => {
            error!("Failed to compile rune script: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    Ok((context, unit))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let Cli {
        upstream,
        mode,
        listen,
    } = Cli::parse();
    // Initialize tracing
    tracing_subscriber::fmt::init();

    info!("Starting Mockbox server...");

    let app = match mode {
        Mode::Log => Router::new().fallback(any(log_request)).into_make_service(),
        Mode::Mock { script } => {
            // Create shared state
            let state = AppState {
                script_path: script,
                http_client: Client::new(),
                upstream,
            };
            // Build router using closure to capture state
            let state_clone = state.clone();
            info!("Upstream URL: {:?}", state.upstream);
            Router::new()
                .fallback(any(move |request: Request| {
                    let state = state_clone.clone();
                    async move { handle_with_rune(state, request).await }
                }))
                .into_make_service()
        }
    };

    match listen.parse::<std::net::SocketAddr>() {
        Ok(addr) => {
            let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
            println!("listening on {}", listener.local_addr().unwrap());
            axum::serve(listener, app).await?;
        }
        #[cfg(target_family = "unix")]
        Err(_) => {
            let listener = tokio::net::UnixListener::bind(&listen).unwrap();
            tokio::process::Command::new("chmod")
                .args(["777", listen.as_str()])
                .spawn()?;
            println!("listening on {}", listener.local_addr().unwrap());
            axum::serve(listener, app).await?;
        }
        #[cfg(not(target_family = "unix"))]
        Err(e) => {
            error!("Failed to parse listen address: {}", e);
            return Err(anyhow::anyhow!("Invalid listen address"));
        }
    }

    Ok(())
}

async fn log_request(req: Request) {
    info!("{req:?}");
}

async fn handle_with_rune(state: AppState, request: Request) -> Response {
    let method = request.method().clone();
    let uri = request.uri().clone();
    let headers = request.headers().clone();

    info!("Handling request: {} {}", method, uri);

    // Extract request body
    let body_bytes = match axum::body::to_bytes(request.into_body(), usize::MAX).await {
        Ok(bytes) => bytes,
        Err(e) => {
            error!("Failed to read request body: {}", e);
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
        execute_and_parse_rune_script(&state_clone, method_string, path_string, body_string)
    })
    .await
    .unwrap_or_else(|e| {
        error!("Rune task panicked: {}", e);
        Err("Script task failed".to_string())
    });

    // Handle result
    match result {
        Ok(Some(response_data)) => {
            // Build response from Send-safe data
            let response = Response::builder()
                .status(StatusCode::from_u16(response_data.status).unwrap_or(StatusCode::OK));

            response.body(Body::from(response_data.body)).unwrap()
        }
        Ok(None) => {
            // Proxy to fallback
            info!("Rune script did not handle request, proxying to fallback");
            proxy_to_fallback(state, method, uri, headers, body_bytes).await
        }
        Err(e) => {
            error!("Rune script execution failed: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
        }
    }
}

async fn proxy_to_fallback(
    state: AppState,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Some(upstream) = state.upstream else {
        return (StatusCode::BAD_GATEWAY, "No fallback server configured").into_response();
    };
    let fallback_url = format!("{}{}", upstream, uri);

    info!("Proxying to: {}", fallback_url);

    // Convert axum Method to reqwest Method
    let reqwest_method = match method.as_str() {
        "GET" => reqwest::Method::GET,
        "POST" => reqwest::Method::POST,
        "PUT" => reqwest::Method::PUT,
        "DELETE" => reqwest::Method::DELETE,
        "HEAD" => reqwest::Method::HEAD,
        "OPTIONS" => reqwest::Method::OPTIONS,
        "PATCH" => reqwest::Method::PATCH,
        "TRACE" => reqwest::Method::TRACE,
        _ => reqwest::Method::GET,
    };

    let mut request_builder = state.http_client.request(reqwest_method, &fallback_url);

    // Copy headers (excluding host and other problematic headers)
    for (key, value) in headers.iter() {
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

            let mut response = Response::builder()
                .status(StatusCode::from_u16(status_code).unwrap_or(StatusCode::OK));

            // Copy response headers
            for (key, value) in resp_headers.iter() {
                if let Ok(value_str) = value.to_str() {
                    response = response.header(key.as_str(), value_str);
                }
            }

            response.body(Body::from(body_bytes)).unwrap()
        }
        Err(e) => {
            error!("Failed to proxy request: {}", e);
            (
                StatusCode::BAD_GATEWAY,
                format!("Failed to reach fallback server: {}", e),
            )
                .into_response()
        }
    }
}

fn compile_rune_script(script: &str) -> Result<(Context, rune::Unit), String> {
    let context =
        rune_modules::default_context().map_err(|e| format!("Failed to create context: {}", e))?;

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
            .map_err(|e| format!("Failed to emit diagnostics: {}", e))?;

        return Err("Script compilation failed".to_string());
    }

    let unit = result.map_err(|e| format!("Build failed: {}", e))?;

    Ok((context, unit))
}

#[derive(Debug)]
struct ResponseData {
    status: u16,
    body: String,
}

fn to_string<T: Display>(value: T) -> String {
    value.to_string()
}

fn execute_and_parse_rune_script(
    state: &AppState,
    method: String,
    path: String,
    body: String,
) -> Result<Option<ResponseData>, String> {
    // Build rune request data inside this non-async context
    let mut request_data = Object::new();

    // Convert strings to rune strings
    let method_str = rune::alloc::String::try_from(method.as_str())
        .map_err(|e| format!("Failed to allocate method string: {}", e))?;
    let path_str = rune::alloc::String::try_from(path.as_str())
        .map_err(|e| format!("Failed to allocate path string: {}", e))?;
    let body_str = rune::alloc::String::try_from(body.as_str())
        .map_err(|e| format!("Failed to allocate body string: {}", e))?;

    // Insert into object
    request_data
        .insert(
            rune::alloc::String::try_from("method").map_err(to_string)?,
            Value::new(method_str).map_err(to_string)?,
        )
        .map_err(|e| format!("Failed to insert method: {}", e))?;

    request_data
        .insert(
            rune::alloc::String::try_from("path").map_err(to_string)?,
            Value::new(path_str).map_err(to_string)?,
        )
        .map_err(|e| format!("Failed to insert path: {}", e))?;

    request_data
        .insert(
            rune::alloc::String::try_from("body").map_err(to_string)?,
            Value::new(body_str).map_err(to_string)?,
        )
        .map_err(|e| format!("Failed to insert body: {}", e))?;

    let request = Value::new(request_data).map_err(to_string)?;

    let (context, unit) =
        load_script(&state.script_path).map_err(|e| format!("Failed to load script: {}", e))?;

    let mut vm = Vm::new(
        Arc::new(context.runtime().map_err(|e| e.to_string())?),
        Arc::new(unit),
    );

    let result = vm
        .call(rune::Hash::type_hash(["handle_request"]), (request,))
        .map_err(|e| format!("Execution error: {}", e))?;

    // Check if unhandled
    if is_unhandled(&result) {
        return Ok(None);
    }

    if let Ok(str_ref) = result.borrow_string_ref() {
        let body = str_ref.to_string();
        return Ok(Some(ResponseData { status: 200, body }));
    }

    // Parse response
    if let Ok(obj) = rune::from_value::<Object>(result) {
        let status = if let Some(v) = obj.get(
            rune::alloc::String::try_from("status")
                .map_err(to_string)?
                .as_str(),
        ) {
            u16::try_from(
                v.as_integer::<u64>()
                    .map_err(|e| format!("Invalid status code: {}", e))?,
            )
            .map_err(|e| format!("Invalid status code: {}", e))?
        } else {
            200
        };

        let body = if let Some(v) = obj.get(
            rune::alloc::String::try_from("body")
                .map_err(to_string)?
                .as_str(),
        ) {
            v.borrow_string_ref()
                .map_err(|e| format!("Invalid body: {}", e))?
                .to_string()
        } else {
            String::new()
        };

        return Ok(Some(ResponseData { status, body }));
    }

    Err("Invalid response format from script".to_string())
}

fn is_unhandled(value: &Value) -> bool {
    // Check if the value is the string "UNHANDLED" or null
    if let Ok(str_ref) = value.borrow_string_ref() {
        return str_ref.to_string() == "UNHANDLED";
    }
    false
}

// fn create_default_script() -> String {
//     r#"
// pub fn handle_request(request) {
//     // Get request details
//     let method = request.method;
//     let path = request.path;
//     let body = request.body;

//     // Example: Handle specific endpoints
//     if path == "/hello" {
//         return #{
//             status: 200,
//             body: "Hello from Rune!"
//         };
//     }

//     if path == "/echo" {
//         return #{
//             status: 200,
//             body: `Echoing: ${body}`
//         };
//     }

//     if path.starts_with("/api/mock") {
//         return #{
//             status: 200,
//             body: `{"message": "Mocked response", "path": "${path}"}`
//         };
//     }

//     // Return UNHANDLED to proxy to fallback server
//     "UNHANDLED"
// }
// "#
//     .to_string()
// }
