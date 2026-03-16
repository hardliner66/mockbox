# Mockbox

A flexible HTTP proxy server powered by [Rune scripting](https://rune-rs.github.io/). Every incoming request is first handled by a Rune script, which can either respond directly or indicate that the request should be proxied to an upstream server.

## Table of Contents
- [Features](#features)
- [Installation](#installation)
  - [Pre-Built Binaries (via cargo-binstall)](#pre-built-binaries-via-cargo-binstall)
  - [Pre-Built Binaries (manual download)](#pre-built-binaries-manual-download)
  - [From Source](#from-source)
- [Usage](#usage)
  - [Basic Setup](#basic-setup)
  - [Configuration](#configuration)
- [Rune Script API](#rune-script-api)
  - [Request Object](#request-object)
  - [Response Options](#response-options)
    - [1. Return a simple string](#1-return-a-simple-string)
    - [2. Return an object](#2-return-an-object)
    - [3. Return status and response](#3-return-status-and-response)
    - [4. Proxy to upstream server](#4-proxy-to-upstream-server)
- [Example Scripts](#example-scripts)
  - [Mock API Endpoints](#mock-api-endpoints)
  - [Route-based Handling](#route-based-handling)
  - [Conditional Mocking](#conditional-mocking)
  - [Error Responses](#error-responses)
- [Architecture](#architecture)
- [Use Cases](#use-cases)
  - [Testing](#testing)
- [Features](#features-1)
  - [`cache`](#cache)
    - [Cache API](#cache-api)
    - [Cache Example](#cache-example)
  - [`rugen`](#rugen)
    - [Rugen Example](#rugen-example)

## Features

- **Rune Scripting**: Handle HTTP requests with dynamic Rune scripts
- **Upstream Proxy**: Automatically proxy unhandled requests to another web server
- **Hot-reloadable**: Modify scripts without restarting
- **Full HTTP Support**: Access method, path, headers, and body in scripts

## Installation

### Pre-Built Binaries (via [cargo-binstall](https://github.com/cargo-bins/cargo-binstall))

```sh
cargo binstall mockbox
```

### Pre-Built Binaries (manual download)

You can download pre-built binaries from the [latest release](https://github.com/hardliner66/mockbox/releases).

### From Source

1. Clone the repository
2. Build the project:

```bash
cargo install mockbox
```

## Usage

### Basic Setup

1. Generate the an example script ([online version](./mockbox.rn)):

```bash
mockbox example > mockbox.rn
```

_If you want to learn more about rune, check out the [rune book](https://rune-rs.github.io/book/)._

2. Run the server:

```bash
mockbox mock
```

If no script path is passed to mockbox, it will check for a file called `mockbox.rn` in the current directory and if it doesn't exist, it will check in `$HOME/.local/share/mockbox` for a `mockbox.rn`.

3. The server will start on `http://127.0.0.1:3333`

### Configuration

Configure the upstream server using the `MOCKBOX_UPSTREAM` environment variable:

```bash
MOCKBOX_UPSTREAM=http://localhost:8080 mockbox mock mockbox.rn
```

or by using the appropriate cli option:

```bash
mockbox mock mockbox.rn --upstream http://localhost:8080
```

## Rune Script API

Your `mockbox.rn` must export a `handle_request` function that receives a request object and returns either a string, an object, a tuple (`(<status_code>, <response>)`).

### Request Object

The request object passed to your handler contains:

- `method`: HTTP method (e.g., "GET", "POST")
- `path`: Request path (e.g., "/api/users")
- `body`: Request body as a string

### Response Options

#### 1. Return a simple string

Return just a string for a 200 OK response:

```rs
pub fn handle_request(request) {
    "Hello, World!"
}
```

#### 2. Return an object

Return an object for a 200 OK response:

```rs
pub fn handle_request(request) {
    #{some: 1, values: 2}
}
```

The object will automatically be converted to json.

#### 3. Return status and response

Return a tuple with `status` and `response` (string or object):

```rs
pub fn handle_request(request) {
    (200, "Hello, World!")
}
```

#### 4. Proxy to upstream server

Explicitly return nothing to proxy the request:

```rs
pub fn handle_request(request) {
    ()
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
            [#{ "id": 1, "name": "John" }, #{ "id": 2, "name": "Jane" }]
        }

        // Mock authentication
        "/api/login" if method == "POST" => {
            #{ "token": "mock-jwt-token-12345" }
        }
    }
}
```

### Route-based Handling

```rs
pub fn handle_request(request) {
    let path = request.path;

    match path {
        // Echo endpoint
        "/echo" => request.body,

        // Handle all /mock/* routes
        _ if path.starts_with("/mock/") => #{mocked: true, path: path},
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
        return #{ "message": "Test mode response" };
    }
}
```

### Error Responses

```rs
pub fn handle_request(request) {
    let path = request.path;

    // Simulate errors for testing
    match path {
        "/error/500" => (500, "Internal Server Error"),
        "/error/404" => (404, "Not Found"),
    }
}
```

## Architecture

1. **Request Reception**: Axum receives the HTTP request
2. **Rune Execution**: The `handle_request` function in `mockbox.rn` is called
3. **Response Decision**:
   - If the script returns a string, object or response tuple → respond directly
   - If the script doesn't return anything or explicitly returns `()` → proxy to upstream server
4. **Upstream Proxy**: Forward the original request to the configured upstream URL
5. **Response**: Return the response from either Rune or the upstream server

## Use Cases

- **Mock APIs**: Create mock responses for testing frontend applications
- **Request Interception**: Log, modify, or reject requests based on conditions
- **A/B Testing**: Route requests to different backends based on rules
- **Development Environment**: Override specific endpoints while keeping others real

### Testing

Test your Rune scripts by making HTTP requests:

```bash
# Test a mocked endpoint
curl http://localhost:3333/demo

# Test the upstream proxy
curl http://localhost:3333/some/unhandled/path
```

## Features

### `cache`

Enables the cache API to persist data between requests.

_This is **enabled** by default_

#### Cache API

```rs
// store a rune value
cache::set(key: &str, value: rune::Value) -> Result<()>;

// load a stored value
cache::get(key: &str) -> Result<rune::Value>;

// delete a stored value
cache::delete(key: &str) -> Result<()>;

// check if a value exists
cache::has(key: &str) -> bool;

// clear the whole cache
cache::clear() -> Result<()>;

// get keys of all stored values
cache::keys() -> Result<()>;
```

#### Cache Example

```rs
["demo"] => {
    let demo_count = cache::get("demo_count")?.unwrap_or(0);
    let new_demo_count = demo_count + 1;
    cache::set("demo_count", new_demo_count)?;
    #{ message: `This demo endpoint has been called ${new_demo_count} times` }
},
```

### `rugen`

Enables the [rugen](https://github.com/hardliner66/rugen) API to build descriptions for generating random data.

_This is **enabled** by default_

#### Rugen Example

This is what describing data with `RuGen` looks like:
```rs
use rugen::*;
describe(
    #{
        asdf: 1..10,
        values: 5.values(55.0..128.0),
        range_from: 100..,
        range_to: ..100,
        choice: [
            #{ A: 100..=200 },
            #{ B: -100..100 },
            #{ C: 0.5..2.5 },
            #{ D: alphanumeric(10) },
        ].pick(),
    },
)?
```

For more information on `RuGen`, please visit the [project repository](https://github.com/hardliner66/rugen).