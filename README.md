# Mockbox

A flexible HTTP proxy server powered by Rune scripting. Every incoming request is first handled by a Rune script, which can either respond directly or indicate that the request should be proxied to a fallback server.

## Features

- **Rune Scripting**: Handle HTTP requests with dynamic Rune scripts
- **Fallback Proxy**: Automatically proxy unhandled requests to another web server
- **Hot-reloadable**: Modify scripts without restarting
- **Full HTTP Support**: Access method, path, headers, and body in scripts

## Installation

1. Clone the repository
2. Build the project:
```bash
cargo install --path .
```

## Usage

### Basic Setup

1. Run the server:
```bash
mockbox mock script.rn
```

2. The server will start on `http://0.0.0.0:333`

### Configuration

Configure the upstream server using the `MOCKBOX_UPSTREAM` environment variable:

```bash
MOCKBOX_UPSTREAM=http://localhost:8080 mockbox mock script.rn
```

or by using the appropriate cli option:

```bash
mockbox mock script.rn --upstream http://localhost:8080
```

## Rune Script API

Your `script.rn` must export a `handle_request` function that receives a request object and returns either a response object or the string `"UNHANDLED"`.

### Request Object

The request object passed to your handler contains:

- `method`: HTTP method (e.g., "GET", "POST")
- `path`: Request path (e.g., "/api/users")
- `body`: Request body as a string

### Response Options

#### 1. Handle the request

Return an object with `status` and `body` fields:

```rs
pub fn handle_request(request) {
    #{
        status: 200,
        body: "Hello, World!"
    }
}
```

#### 2. Return a simple string

Return just a string for a 200 OK response:

```rs
pub fn handle_request(request) {
    "Hello, World!"
}
```

#### 3. Proxy to fallback server

Return the string `"UNHANDLED"` to proxy the request:

```rs
pub fn handle_request(request) {
    "UNHANDLED"
}
```

## Example Scripts

### Mock API Endpoints

```rs
pub fn handle_request(request) {
    let path = request.path;
    let method = request.method;

    // Mock user API
    match path {
        "/api/users" if method == "GET" => {
            #{
                status: 200,
                body: json::to_string([#{ "id": 1, "name": "John" }, #{ "id": 2, "name": "Jane" }])?,
            }
        }

        // Mock authentication
        "/api/login" if method == "POST" => {
            #{ status: 200, body: json::to_string(#{ "token": "mock-jwt-token-12345" })? }
        }

        // Proxy everything else
        _ => "UNHANDLED",

    }
}
```

### Route-based Handling

```rs
pub fn handle_request(request) {
    let path = request.path;

    match path {
        // Echo endpoint
        "/echo" => #{ status: 200, body: request.body },

        // Handle all /mock/* routes
        _ if path.starts_with("/mock/") => #{ status: 200, body: `{"mocked": true, "path": "${path}"}` },

        // Default: proxy to real server
        _ => "UNHANDLED",
    }
}
```

### Conditional Mocking

```rs
pub fn handle_request(request) {
    let path = request.path;
    let body = request.body;

    // Mock only if body contains "test"
    if body.contains("test") {
        return #{ status: 200, body: json::to_string(#{ "message": "Test mode response" })? };
    }

    // Otherwise use real backend
    "UNHANDLED"
}
```

### Error Responses

```rs
pub fn handle_request(request) {
    let path = request.path;

    // Simulate errors for testing
    match path {
        "/error/500" => #{ status: 500, body: "Internal Server Error" },
        "/error/404" => #{ status: 404, body: "Not Found" },
        _ => "UNHANDLED",
    }
}
```

## Architecture

1. **Request Reception**: Axum receives the HTTP request
2. **Rune Execution**: The `handle_request` function in `script.rn` is called
3. **Response Decision**:
   - If the script returns a response object or string → respond directly
   - If the script returns `"UNHANDLED"` → proxy to fallback server
4. **Fallback Proxy**: Forward the original request to the configured fallback URL
5. **Response**: Return the response from either Rune or the fallback server

## Use Cases

- **Mock APIs**: Create mock responses for testing frontend applications
- **Request Interception**: Log, modify, or reject requests based on conditions
- **A/B Testing**: Route requests to different backends based on rules
- **Development Environment**: Override specific endpoints while keeping others real

### Testing

Test your Rune scripts by making HTTP requests:

```bash
# Test a mocked endpoint
curl http://localhost:3000/hello

# Test the fallback proxy
curl http://localhost:3000/some/real/path
```
