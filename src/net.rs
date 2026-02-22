use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;
use crate::interpreter::Value;

// ── Registration ──────────────────────────────────────────────────────────────

pub fn net_functions() -> Vec<&'static str> {
    vec![
        // HTTP
        "http_get",
        "http_get_json",
        "http_post",
        "http_post_json",
        "http_put",
        "http_delete",
        "http_request",
        "http_status",
        // URL utilities
        "url_encode",
        "url_decode",
        "url_parse",
        "url_join",
        "url_query_string",
    ]
}

// ── Dispatch ──────────────────────────────────────────────────────────────────

pub fn call_net(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "http_get"         => net_http_get(args),
        "http_get_json"    => net_http_get_json(args),
        "http_post"        => net_http_post(args),
        "http_post_json"   => net_http_post_json(args),
        "http_put"         => net_http_put(args),
        "http_delete"      => net_http_delete(args),
        "http_request"     => net_http_request(args),
        "http_status"      => net_http_status(args),
        "url_encode"       => net_url_encode(args),
        "url_decode"       => net_url_decode(args),
        "url_parse"        => net_url_parse(args),
        "url_join"         => net_url_join(args),
        "url_query_string" => net_url_query_string(args),
        _                  => Err(format!("Unknown net function '{}'", name)),
    }
}

// ── HTTP functions ─────────────────────────────────────────────────────────────

/// http_get(url: String) -> Result<String, String>
/// Performs a GET request and returns the response body as a string.
///
///   let res = http_get("https://example.com/api/data")
///   match res {
///       Ok(body) => println(body)
///       Err(e)   => println("Error: #{e}")
///   }
fn net_http_get(args: Vec<Value>) -> Result<Value, String> {
    let url = require_str_arg(&args, 0, "http_get(url)")?;
    match ureq::get(&url).call() {
        Ok(resp) => {
            let body = resp.into_string().map_err(|e| e.to_string())?;
            Ok(ok_result(Value::Str(body)))
        }
        Err(e) => Ok(err_result(format!("{}", e))),
    }
}

/// http_get_json(url: String) -> Result<String, String>
/// Performs a GET request with Accept: application/json header.
/// Returns the raw JSON string on success.
///
///   let res = http_get_json("https://api.example.com/users")
fn net_http_get_json(args: Vec<Value>) -> Result<Value, String> {
    let url = require_str_arg(&args, 0, "http_get_json(url)")?;
    match ureq::get(&url)
        .set("Accept", "application/json")
        .call()
    {
        Ok(resp) => {
            let body = resp.into_string().map_err(|e| e.to_string())?;
            Ok(ok_result(Value::Str(body)))
        }
        Err(e) => Ok(err_result(format!("{}", e))),
    }
}

/// http_post(url: String, body: String) -> Result<String, String>
/// Performs a POST request with a plain text body.
///
///   let res = http_post("https://example.com/submit", "hello=world")
fn net_http_post(args: Vec<Value>) -> Result<Value, String> {
    let url  = require_str_arg(&args, 0, "http_post(url, body)")?;
    let body = require_str_arg(&args, 1, "http_post(url, body)")?;
    match ureq::post(&url)
        .set("Content-Type", "text/plain")
        .send_string(&body)
    {
        Ok(resp) => {
            let text = resp.into_string().map_err(|e| e.to_string())?;
            Ok(ok_result(Value::Str(text)))
        }
        Err(e) => Ok(err_result(format!("{}", e))),
    }
}

/// http_post_json(url: String, json_body: String) -> Result<String, String>
/// Performs a POST request with Content-Type: application/json.
///
///   let res = http_post_json("https://api.example.com/users", "{\"name\": \"Alice\"}")
fn net_http_post_json(args: Vec<Value>) -> Result<Value, String> {
    let url  = require_str_arg(&args, 0, "http_post_json(url, json_body)")?;
    let body = require_str_arg(&args, 1, "http_post_json(url, json_body)")?;
    match ureq::post(&url)
        .set("Content-Type", "application/json")
        .set("Accept", "application/json")
        .send_string(&body)
    {
        Ok(resp) => {
            let text = resp.into_string().map_err(|e| e.to_string())?;
            Ok(ok_result(Value::Str(text)))
        }
        Err(e) => Ok(err_result(format!("{}", e))),
    }
}

/// http_put(url: String, body: String) -> Result<String, String>
/// Performs a PUT request with a plain text body.
///
///   let res = http_put("https://api.example.com/users/1", "{\"name\": \"Bob\"}")
fn net_http_put(args: Vec<Value>) -> Result<Value, String> {
    let url  = require_str_arg(&args, 0, "http_put(url, body)")?;
    let body = require_str_arg(&args, 1, "http_put(url, body)")?;
    match ureq::put(&url)
        .set("Content-Type", "text/plain")
        .send_string(&body)
    {
        Ok(resp) => {
            let text = resp.into_string().map_err(|e| e.to_string())?;
            Ok(ok_result(Value::Str(text)))
        }
        Err(e) => Ok(err_result(format!("{}", e))),
    }
}

/// http_delete(url: String) -> Result<String, String>
/// Performs a DELETE request.
///
///   let res = http_delete("https://api.example.com/users/1")
fn net_http_delete(args: Vec<Value>) -> Result<Value, String> {
    let url = require_str_arg(&args, 0, "http_delete(url)")?;
    match ureq::delete(&url).call() {
        Ok(resp) => {
            let text = resp.into_string().map_err(|e| e.to_string())?;
            Ok(ok_result(Value::Str(text)))
        }
        Err(e) => Ok(err_result(format!("{}", e))),
    }
}

/// http_request(method: String, url: String, headers: Map, body: String) -> Result<String, String>
/// Full-control HTTP request with custom method, headers, and body.
///
///   let headers = {}
///   headers["Authorization"] = "Bearer my-token"
///   headers["X-Custom-Header"] = "value"
///   let res = http_request("PATCH", "https://api.example.com/data", headers, "{\"key\": 1}")
fn net_http_request(args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 4 {
        return Err("http_request(method, url, headers, body) requires 4 arguments".into());
    }
    let method  = require_str_arg(&args, 0, "http_request")?;
    let url     = require_str_arg(&args, 1, "http_request")?;
    let body    = require_str_arg(&args, 3, "http_request")?;

    let mut req = ureq::request(&method, &url);

    // Apply headers from Map value
    if let Value::Map(map) = &args[2] {
        for (key, val) in map.borrow().iter() {
            req = req.set(key, &format!("{}", val));
        }
    }

    let result = if body.is_empty() {
        req.call()
    } else {
        req.send_string(&body)
    };

    match result {
        Ok(resp) => {
            let text = resp.into_string().map_err(|e| e.to_string())?;
            Ok(ok_result(Value::Str(text)))
        }
        Err(e) => Ok(err_result(format!("{}", e))),
    }
}

/// http_status(url: String) -> Result<Int, String>
/// Performs a HEAD request and returns the HTTP status code as an Int.
///
///   let code = http_status("https://example.com")
///   match code {
///       Ok(200) => println("OK!")
///       Ok(n)   => println("Got status: #{n}")
///       Err(e)  => println("Failed: #{e}")
///   }
fn net_http_status(args: Vec<Value>) -> Result<Value, String> {
    let url = require_str_arg(&args, 0, "http_status(url)")?;
    match ureq::head(&url).call() {
        Ok(resp) => Ok(ok_result(Value::Int(resp.status() as i64))),
        Err(ureq::Error::Status(code, _)) => Ok(ok_result(Value::Int(code as i64))),
        Err(e) => Ok(err_result(format!("{}", e))),
    }
}

// ── URL utility functions ─────────────────────────────────────────────────────

/// url_encode(s: String) -> String
/// Percent-encodes a string for safe use in a URL.
///
///   println(url_encode("hello world & foo=bar"))
///   // => "hello%20world%20%26%20foo%3Dbar"
fn net_url_encode(args: Vec<Value>) -> Result<Value, String> {
    let s = require_str_arg(&args, 0, "url_encode(s)")?;
    Ok(Value::Str(percent_encode(&s)))
}

/// url_decode(s: String) -> String
/// Decodes a percent-encoded URL string.
///
///   println(url_decode("hello%20world"))
///   // => "hello world"
fn net_url_decode(args: Vec<Value>) -> Result<Value, String> {
    let s = require_str_arg(&args, 0, "url_decode(s)")?;
    Ok(Value::Str(percent_decode(&s)))
}

/// url_parse(url: String) -> Map
/// Parses a URL into its components: scheme, host, path, query, fragment, port.
/// Returns a Map with string values (empty string if component is absent).
///
///   let parts = url_parse("https://example.com:8080/api/users?page=1#top")
///   println(parts["scheme"])   // "https"
///   println(parts["host"])     // "example.com"
///   println(parts["port"])     // "8080"
///   println(parts["path"])     // "/api/users"
///   println(parts["query"])    // "page=1"
///   println(parts["fragment"]) // "top"
fn net_url_parse(args: Vec<Value>) -> Result<Value, String> {
    let url = require_str_arg(&args, 0, "url_parse(url)")?;
    let parsed = parse_url(&url)?;
    Ok(Value::Map(Rc::new(RefCell::new(parsed))))
}

/// url_join(base: String, path: String) -> String
/// Joins a base URL with a relative path, handling slashes correctly.
///
///   println(url_join("https://example.com/api", "users"))
///   // => "https://example.com/api/users"
///
///   println(url_join("https://example.com/api/", "/v2/users"))
///   // => "https://example.com/v2/users"  (absolute path replaces)
fn net_url_join(args: Vec<Value>) -> Result<Value, String> {
    let base = require_str_arg(&args, 0, "url_join(base, path)")?;
    let path = require_str_arg(&args, 1, "url_join(base, path)")?;

    let joined = if path.starts_with("http://") || path.starts_with("https://") {
        // Absolute URL — return as-is
        path
    } else if path.starts_with('/') {
        // Absolute path — replace path on base
        let scheme_end = base.find("://").map(|i| i + 3).unwrap_or(0);
        let host_end = base[scheme_end..]
            .find('/')
            .map(|i| i + scheme_end)
            .unwrap_or(base.len());
        format!("{}{}", &base[..host_end], path)
    } else {
        // Relative path — append to base
        let base = base.trim_end_matches('/');
        format!("{}/{}", base, path)
    };
    Ok(Value::Str(joined))
}

/// url_query_string(params: Map) -> String
/// Converts a Map of key-value pairs into a URL query string (percent-encoded).
///
///   let params = {}
///   params["name"] = "Alice Smith"
///   params["page"] = "2"
///   println(url_query_string(params))
///   // => "name=Alice%20Smith&page=2"
fn net_url_query_string(args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("url_query_string(map) requires 1 argument".into());
    }
    match &args[0] {
        Value::Map(m) => {
            let pairs: Vec<String> = m
                .borrow()
                .iter()
                .map(|(k, v)| format!("{}={}", percent_encode(k), percent_encode(&format!("{}", v))))
                .collect();
            Ok(Value::Str(pairs.join("&")))
        }
        other => Err(format!("url_query_string() expects a Map, got {}", other)),
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn require_str_arg(args: &[Value], idx: usize, sig: &str) -> Result<String, String> {
    match args.get(idx) {
        Some(Value::Str(s)) => Ok(s.clone()),
        Some(other)         => Ok(format!("{}", other)), // coerce to string
        None                => Err(format!("{} — argument {} is required", sig, idx + 1)),
    }
}

fn ok_result(val: Value) -> Value {
    Value::Result(std::result::Result::Ok(Box::new(val)))
}

fn err_result(msg: String) -> Value {
    Value::Result(std::result::Result::Err(Box::new(Value::Str(msg))))
}

/// Very simple URL parser — does not require an external crate.
/// Handles:  scheme://[host[:port]][path][?query][#fragment]
fn parse_url(url: &str) -> Result<HashMap<String, Value>, String> {
    let mut map: HashMap<String, Value> = HashMap::new();
    let empty = || Value::Str(String::new());

    // Extract scheme
    let (scheme, rest) = if let Some(idx) = url.find("://") {
        (&url[..idx], &url[idx + 3..])
    } else {
        ("", url)
    };
    map.insert("scheme".into(), Value::Str(scheme.to_string()));

    // Strip fragment
    let (rest, fragment) = if let Some(idx) = rest.find('#') {
        (&rest[..idx], &rest[idx + 1..])
    } else {
        (rest, "")
    };
    map.insert("fragment".into(), Value::Str(fragment.to_string()));

    // Strip query
    let (rest, query) = if let Some(idx) = rest.find('?') {
        (&rest[..idx], &rest[idx + 1..])
    } else {
        (rest, "")
    };
    map.insert("query".into(), Value::Str(query.to_string()));

    // Separate host[:port] from path
    let (authority, path) = if let Some(idx) = rest.find('/') {
        (&rest[..idx], &rest[idx..])
    } else {
        (rest, "")
    };
    map.insert("path".into(), Value::Str(path.to_string()));

    // Separate host from port
    let (host, port) = if let Some(idx) = authority.rfind(':') {
        let potential_port = &authority[idx + 1..];
        if potential_port.chars().all(|c| c.is_ascii_digit()) {
            (&authority[..idx], potential_port)
        } else {
            (authority, "")
        }
    } else {
        (authority, "")
    };
    map.insert("host".into(), Value::Str(host.to_string()));
    map.insert("port".into(), Value::Str(port.to_string()));

    Ok(map)
}

/// Percent-encodes a string (RFC 3986 unreserved chars are kept as-is).
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9'
            | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            _ => out.push_str(&format!("%{:02X}", byte)),
        }
    }
    out
}

/// Decodes a percent-encoded string.
fn percent_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            let h1 = chars.next().and_then(|c| c.to_digit(16));
            let h2 = chars.next().and_then(|c| c.to_digit(16));
            if let (Some(h1), Some(h2)) = (h1, h2) {
                let byte = ((h1 << 4) | h2) as u8;
                out.push(byte as char);
            } else {
                out.push('%');
            }
        } else if c == '+' {
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}