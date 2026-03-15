# Mockbox

A flexible HTTP proxy server powered by [Rune scripting](https://rune-rs.github.io/). Every incoming request is first handled by a Rune script, which can either respond directly or indicate that the request should be proxied to an upstream server.

## Features

- **Rune Scripting**: Handle HTTP requests with dynamic Rune scripts
- **Upstream Proxy**: Automatically proxy unhandled requests to another web server
- **Hot-reloadable**: Modify scripts without restarting
- **Full HTTP Support**: Access method, path, headers, and body in scripts

## Installation

### Pre-Built Binaries

You can download pre-built binaries from the [latest release](https://github.com/hardliner66/mockbox/releases).

### From Source

1. Clone the repository
2. Build the project:

```bash
cargo install mockbox
```

## Usage

### Basic Setup

1. Generate the an example script:

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

Your `mockbox.rn` must export a `handle_request` function that receives a request object and returns either a string, an object, a tuple (`(<statuscode>, <response>)`).

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
curl http://localhost:3333/hello

# Test the upstream proxy
curl http://localhost:3333/some/real/path
```

## Features

### `storage`

Enables the storage API to persist data between requests.

_This is **enabled** by default_

```rs
// store a rune value
storage::set(key: &str, value: rune::Value) -> Result<()>;

// load a stored value
storage::get(key: &str) -> Result<rune::Value>;

// delete a stored value
storage::delete(key: &str) -> Result<()>;

// check if a value exists
storage::has(key: &str) -> bool;

// clear the whole storage
storage::clear() -> Result<()>;

// get keys of all stored values
storage::keys() -> Result<()>;
```

### `rng`

Enables the rng API to create random values.

_This is **enabled** by default_

```rs
// choose one of the passed values at random
rng::choose(values: &[rune::Value]) -> rune::Value;

// choose <count> number from the passed values at random
rng::choose_many(values: &[rune::Value], count: usize) -> Vec<rune::Value>;

// alias for choose_many
rng::sample(values: &[rune::Value], count: usize) -> Vec<rune::Value>;

// get a random value between <start> and <end> (exclusive)
rng::range(start: usize, end: usize) -> usize;

// get <count> random values between <start> and <end> (exclusive)
rng::range_many(start: usize, end: usize, count: usize) -> Vec<usize>;

// get a random char value between <start> and <end> (exclusive)
rng::range_char(start: char, end: char) -> char;

// get <count> random char values between <start> and <end> (exclusive)
rng::range_char_many(start: char, end: char, count: usize) -> Vec<char>;

// get a random value between <start> and <end> (inclusive)
rng::range_inclusive(start: usize, end: usize) -> usize;

// get <count> random values between <start> and <end> (inclusive)
rng::range_inclusive_many(start: usize, end: usize, count: usize) -> Vec<usize>

// get a random char value between <start> and <end> (inclusive)
rng::range_char_inclusive(start: char, end: char) -> char;

// get <count> random char values between <start> and <end> (inclusive)
rng::range_char_inclusive_many(start: char, end: char, count: usize) -> Vec<char>

// get a random alpha numeric characters
rng::alpha_numeric() -> char;

// get <count> random alpha numeric numbers
rng::alpha_numeric_many(count: usize) -> Vec<char>;

// get a random bool
rng::bool() -> bool

// get a random u8 value
rng::u8() -> u8;

// get a random u16 value
rng::u16() -> u16;

// get a random u32 value
rng::u32() -> u32;

// get a random u64 value
rng::u64() -> u64;

// get a random u128 value
rng::u128() -> u128;

// get a random i8 value
rng::i8() -> i8;

// get a random i16 value
rng::i16() -> i16;

// get a random i32 value
rng::i32() -> i32;

// get a random i64 value
rng::i64() -> i64;

// get a random i128 value
rng::i128() -> i128;

// get a random f32 value
rng::f32() -> f32;

// get a random f64 value
rng::f64() -> f64;

// get <count> random bool values
rng::bool_many(count: usize) -> Vec<bool>;

// get <count> random u8 values
rng::u8_many()count: usize -> Vec<u8>;

// get <count> random u16 values
rng::u16_many(count: usize) -> Vec<u16>;

// get <count> random u32 values
rng::u32_many(count: usize) -> Vec<u32>;

// get <count> random u64 values
rng::u64_many(count: usize) -> Vec<u64>;

// get <count> random u128 values
rng::u128_many(count: usize) -> Vec<u128>;

// get <count> random i8 values
rng::i8_many(count: usize) -> Vec<i8>;

// get <count> random i16 values
rng::i16_many(count: usize) -> Vec<i16>;

// get <count> random i32 values
rng::i32_many(count: usize) -> Vec<i32>;

// get <count> random i64 values
rng::i64_many(count: usize) -> Vec<i64>;

// get <count> random i128 values
rng::i128_many(count: usize) -> Vec<i128>;

// get <count> random f32 values
rng::f32_many(count: usize) -> Vec<f32>;

// get <count> random f64 values
rng::f64_many(count: usize) -> Vec<f64>;
```

### `spec`

Enables the spec API to build descriptions for generating random data.

_This is **enabled** by default_

```rs
// creates a spec that evaluates to the passed value
spec::just(value: rune::Value) -> Spec;

// creates a spec that evaluates to a random boolean
spec::bool() -> Spec;

// creates a spec that evaluates to a random u128 between <min> and <max> (exclusive)
spec::uint(min: u128, max:  u128) -> Spec;

// creates a spec that evaluates to a random i128 between <min> and <max> (exclusive)
spec::int(min: i128, max:  i128) -> Spec;

// creates a spec that evaluates to a random f64 between <min> and <max> (exclusive)
spec::float(min: f32, max: f32) -> Spec;

// creates a spec that evaluates to a string of random alpha numeric characters that is <len> long
spec::alphanumeric(len: Spec) -> Spec;

// creates a spec that evaluates to a string of random characters between <min> and <max> (exclusive)
spec::string(min: usize, max: usize) -> Spec;

// creates a spec that evaluates to a random value from the passed vec
spec::one_of(values: Vec<Spec>) -> Spec;

// creates a spec that evaluates to a weighted random value from the passed vec
spec::weighted(values: Vec<(u32, Spec)>) -> Spec;

// creates a spec that evaluates to a vec of length <len>, filled with values defined by <item>
spec::array(len: Spec, item: Spec) -> Spec;

// creates a spec that evaluates to a an object
spec::object(fields: HashMap<String, Spec>) -> Spec;

// creates a spec that has a 0.0 < p < 1.0 chance to evaluate to an optional value defined by <item>
spec::optional(p: Spec, item: Spec) -> Spec;

// creates a spec that takes all items in a vec and evaluates them to values, according to their spec
spec::tuple(items: Vec<Spec>) -> Spec;

// evaluates a given spec
Spec::generate(&self) -> Result<rune::Value>;
```
