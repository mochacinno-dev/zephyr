// ═══════════════════════════════════════════════════════════
// Zephyr JSON — parser, serializer, and access helpers
// ═══════════════════════════════════════════════════════════
//
// QUICK REFERENCE
//
//   CORE
//   json_parse(s)              String → Result<Value, String>
//   json_stringify(v)          Value  → Result<String, String>
//   json_pretty(v)             Value  → Result<String, String>
//
//   ACCESS
//   json_get(v, key)           Value, String → Value          (nil on miss)
//   json_get_path(v, path)     Value, String → Value          ("a.b.c" dot path)
//   json_set(v, key, val)      Value, String, Value → Value   (returns updated Map)
//   json_has(v, key)           Value, String → Bool
//
//   TYPE CHECKS
//   json_is_object(v)          Value → Bool
//   json_is_array(v)           Value → Bool
//   json_is_string(v)          Value → Bool
//   json_is_number(v)          Value → Bool
//   json_is_null(v)            Value → Bool
//   json_is_bool(v)            Value → Bool
//
//   INSPECTION
//   json_keys(v)               Value → Result<List, String>
//   json_values(v)             Value → Result<List, String>
//   json_len(v)                Value → Result<Int, String>
//
// ═══════════════════════════════════════════════════════════

use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;
use crate::interpreter::Value;

// ── Registration ──────────────────────────────────────────────────────────────

pub fn json_functions() -> Vec<&'static str> {
    vec![
        "json_parse",
        "json_stringify",
        "json_pretty",
        "json_get",
        "json_get_path",
        "json_set",
        "json_has",
        "json_is_object",
        "json_is_array",
        "json_is_string",
        "json_is_number",
        "json_is_null",
        "json_is_bool",
        "json_keys",
        "json_values",
        "json_len",
    ]
}

// ── Dispatch ──────────────────────────────────────────────────────────────────

pub fn call_json(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "json_parse"      => json_parse(args),
        "json_stringify"  => json_stringify(args),
        "json_pretty"     => json_pretty(args),
        "json_get"        => json_get(args),
        "json_get_path"   => json_get_path(args),
        "json_set"        => json_set(args),
        "json_has"        => json_has(args),
        "json_is_object"  => json_is_object(args),
        "json_is_array"   => json_is_array(args),
        "json_is_string"  => json_is_string(args),
        "json_is_number"  => json_is_number(args),
        "json_is_null"    => json_is_null(args),
        "json_is_bool"    => json_is_bool(args),
        "json_keys"       => json_keys(args),
        "json_values"     => json_values(args),
        "json_len"        => json_len(args),
        _                 => Err(format!("Unknown json function '{}'", name)),
    }
}

// ── Core functions ────────────────────────────────────────────────────────────

/// json_parse(s: String) -> Result<Value, String>
///
/// Parses a JSON string into Zephyr values.
///
///   JSON object  → Value::Map
///   JSON array   → Value::List
///   JSON string  → Value::Str
///   JSON number  → Value::Int (if no decimal/exponent) or Value::Float
///   JSON bool    → Value::Bool
///   JSON null    → Value::Nil
///
/// Example:
///   let res = json_parse("{\"name\": \"Alice\", \"age\": 30}")
///   match res {
///       Ok(data) => println(json_get(data, "name"))
///       Err(e)   => println("Parse error: #{e}")
///   }
fn json_parse(args: Vec<Value>) -> Result<Value, String> {
    let s = require_str(&args, 0, "json_parse(s)")?;
    match parse_value(s.trim(), &mut 0) {
        Ok(val) => Ok(ok_val(val)),
        Err(e)  => Ok(err_val(e)),
    }
}

/// json_stringify(v: Value) -> Result<String, String>
///
/// Serializes any Zephyr value to a compact JSON string.
/// Functions are not serializable and return Err.
///
/// Example:
///   var data = {}
///   data["name"] = "Alice"
///   data["scores"] = [10, 20, 30]
///   let s = json_stringify(data)
fn json_stringify(args: Vec<Value>) -> Result<Value, String> {
    let val = args.into_iter().next()
        .ok_or_else(|| "json_stringify(v) requires 1 argument".to_string())?;
    match serialize(&val, 0, false) {
        Ok(s)  => Ok(ok_val(Value::Str(s))),
        Err(e) => Ok(err_val(e)),
    }
}

/// json_pretty(v: Value) -> Result<String, String>
///
/// Serializes to pretty-printed JSON with 2-space indentation.
///
/// Example:
///   let raw = http_get_json("https://api.example.com/data")
///   match raw {
///       Ok(s) => {
///           let data = json_parse(s)
///           match data {
///               Ok(v) => println(json_pretty(v).unwrap_or("?"))
///               Err(e) => println(e)
///           }
///       }
///       Err(e) => println(e)
///   }
fn json_pretty(args: Vec<Value>) -> Result<Value, String> {
    let val = args.into_iter().next()
        .ok_or_else(|| "json_pretty(v) requires 1 argument".to_string())?;
    match serialize(&val, 0, true) {
        Ok(s)  => Ok(ok_val(Value::Str(s))),
        Err(e) => Ok(err_val(e)),
    }
}

// ── Access helpers ────────────────────────────────────────────────────────────

/// json_get(v: Value, key: String) -> Value
///
/// Gets a key from a JSON object (Map). Returns nil if the key is missing
/// or the value is not an object. Never errors — safe to chain.
///
/// Example:
///   let name = json_get(data, "name")   // Value::Str or Value::Nil
///   let age  = json_get(data, "age")    // Value::Int or Value::Nil
fn json_get(args: Vec<Value>) -> Result<Value, String> {
    let val = args.get(0).cloned().unwrap_or(Value::Nil);
    let key = require_str(&args, 1, "json_get(v, key)")?;
    Ok(get_key(&val, &key))
}

/// json_get_path(v: Value, path: String) -> Value
///
/// Traverses a nested structure using a dot-separated path.
/// Array indices can be used as numeric path segments.
/// Returns nil at the first missing key/index rather than erroring.
///
/// Example:
///   // {"user": {"address": {"city": "Berlin"}}}
///   let city = json_get_path(data, "user.address.city")
///
///   // {"users": [{"name": "Alice"}, {"name": "Bob"}]}
///   let first_name = json_get_path(data, "users.0.name")
fn json_get_path(args: Vec<Value>) -> Result<Value, String> {
    let val  = args.get(0).cloned().unwrap_or(Value::Nil);
    let path = require_str(&args, 1, "json_get_path(v, path)")?;
    let mut current = val;
    for segment in path.split('.') {
        current = match &current {
            Value::Map(_) => get_key(&current, segment),
            Value::List(v) => {
                match segment.parse::<usize>() {
                    Ok(i) => v.borrow().get(i).cloned().unwrap_or(Value::Nil),
                    Err(_) => Value::Nil,
                }
            }
            _ => Value::Nil,
        };
    }
    Ok(current)
}

/// json_set(v: Value, key: String, val: Value) -> Value
///
/// Returns a new Map with the given key set to val.
/// If v is not a Map, wraps it in a new single-key Map.
/// Does not mutate the original — returns a new Map.
///
/// Example:
///   var data = json_parse("{\"name\": \"Alice\"}").unwrap_or({})
///   data = json_set(data, "age", 31)
///   data = json_set(data, "active", true)
fn json_set(args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 3 {
        return Err("json_set(v, key, val) requires 3 arguments".into());
    }
    let key = require_str(&args, 1, "json_set")?;
    let new_val = args[2].clone();
    match &args[0] {
        Value::Map(m) => {
            let mut cloned = m.borrow().clone();
            cloned.insert(key, new_val);
            Ok(Value::Map(Rc::new(RefCell::new(cloned))))
        }
        _ => {
            let mut map = HashMap::new();
            map.insert(key, new_val);
            Ok(Value::Map(Rc::new(RefCell::new(map))))
        }
    }
}

/// json_has(v: Value, key: String) -> Bool
///
/// Returns true if v is an object containing the given key.
///
/// Example:
///   if json_has(data, "error") {
///       println("API returned an error")
///   }
fn json_has(args: Vec<Value>) -> Result<Value, String> {
    let key = require_str(&args, 1, "json_has(v, key)")?;
    match args.get(0) {
        Some(Value::Map(m)) => Ok(Value::Bool(m.borrow().contains_key(&key))),
        _ => Ok(Value::Bool(false)),
    }
}

// ── Type checks ───────────────────────────────────────────────────────────────

fn json_is_object(args: Vec<Value>) -> Result<Value, String> {
    Ok(Value::Bool(matches!(args.get(0), Some(Value::Map(_)))))
}

fn json_is_array(args: Vec<Value>) -> Result<Value, String> {
    Ok(Value::Bool(matches!(args.get(0), Some(Value::List(_)))))
}

fn json_is_string(args: Vec<Value>) -> Result<Value, String> {
    Ok(Value::Bool(matches!(args.get(0), Some(Value::Str(_)))))
}

fn json_is_number(args: Vec<Value>) -> Result<Value, String> {
    Ok(Value::Bool(matches!(args.get(0), Some(Value::Int(_)) | Some(Value::Float(_)))))
}

fn json_is_null(args: Vec<Value>) -> Result<Value, String> {
    Ok(Value::Bool(matches!(args.get(0), Some(Value::Nil) | None)))
}

fn json_is_bool(args: Vec<Value>) -> Result<Value, String> {
    Ok(Value::Bool(matches!(args.get(0), Some(Value::Bool(_)))))
}

// ── Inspection ────────────────────────────────────────────────────────────────

/// json_keys(v: Value) -> Result<List<String>, String>
///
/// Returns the keys of a JSON object as a List of strings.
///
/// Example:
///   let keys = json_keys(data)
///   match keys {
///       Ok(ks) => for k in ks { println(k) }
///       Err(e) => println(e)
///   }
fn json_keys(args: Vec<Value>) -> Result<Value, String> {
    match args.get(0) {
        Some(Value::Map(m)) => {
            let mut keys: Vec<Value> = m.borrow().keys()
                .map(|k| Value::Str(k.clone()))
                .collect();
            keys.sort_by(|a, b| {
                if let (Value::Str(a), Value::Str(b)) = (a, b) { a.cmp(b) }
                else { std::cmp::Ordering::Equal }
            });
            Ok(ok_val(Value::List(Rc::new(RefCell::new(keys)))))
        }
        Some(other) => Ok(err_val(format!("json_keys() expects an object, got {}", crate::interpreter::value_type_name(other)))),
        None        => Ok(err_val("json_keys() requires 1 argument".into())),
    }
}

/// json_values(v: Value) -> Result<List, String>
///
/// Returns the values of a JSON object as a List.
fn json_values(args: Vec<Value>) -> Result<Value, String> {
    match args.get(0) {
        Some(Value::Map(m)) => {
            let vals: Vec<Value> = m.borrow().values().cloned().collect();
            Ok(ok_val(Value::List(Rc::new(RefCell::new(vals)))))
        }
        Some(other) => Ok(err_val(format!("json_values() expects an object, got {}", crate::interpreter::value_type_name(other)))),
        None        => Ok(err_val("json_values() requires 1 argument".into())),
    }
}

/// json_len(v: Value) -> Result<Int, String>
///
/// Returns the number of elements in a JSON array or object.
fn json_len(args: Vec<Value>) -> Result<Value, String> {
    match args.get(0) {
        Some(Value::List(v)) => Ok(ok_val(Value::Int(v.borrow().len() as i64))),
        Some(Value::Map(m))  => Ok(ok_val(Value::Int(m.borrow().len() as i64))),
        Some(other) => Ok(err_val(format!("json_len() expects array or object, got {}", crate::interpreter::value_type_name(other)))),
        None        => Ok(err_val("json_len() requires 1 argument".into())),
    }
}

// ═══════════════════════════════════════════════════════════
// JSON Parser — hand-written recursive descent, zero deps
// ═══════════════════════════════════════════════════════════

fn parse_value(src: &str, pos: &mut usize) -> Result<Value, String> {
    skip_ws(src, pos);
    match src.as_bytes().get(*pos) {
        Some(b'"')        => parse_string(src, pos),
        Some(b'{')        => parse_object(src, pos),
        Some(b'[')        => parse_array(src, pos),
        Some(b't')        => parse_literal(src, pos, "true",  Value::Bool(true)),
        Some(b'f')        => parse_literal(src, pos, "false", Value::Bool(false)),
        Some(b'n')        => parse_literal(src, pos, "null",  Value::Nil),
        Some(b'-') | Some(b'0'..=b'9') => parse_number(src, pos),
        Some(c) => Err(format!("Unexpected character '{}' at position {}", *c as char, pos)),
        None    => Err("Unexpected end of input".into()),
    }
}

fn skip_ws(src: &str, pos: &mut usize) {
    let bytes = src.as_bytes();
    while *pos < bytes.len() && matches!(bytes[*pos], b' ' | b'\t' | b'\n' | b'\r') {
        *pos += 1;
    }
}

fn expect_byte(src: &str, pos: &mut usize, byte: u8) -> Result<(), String> {
    if src.as_bytes().get(*pos) == Some(&byte) {
        *pos += 1;
        Ok(())
    } else {
        let got = src.as_bytes().get(*pos).copied().unwrap_or(0);
        Err(format!("Expected '{}' but got '{}' at position {}",
            byte as char, got as char, pos))
    }
}

fn parse_literal(src: &str, pos: &mut usize, text: &str, val: Value) -> Result<Value, String> {
    if src[*pos..].starts_with(text) {
        *pos += text.len();
        Ok(val)
    } else {
        Err(format!("Expected '{}' at position {}", text, pos))
    }
}

fn parse_string(src: &str, pos: &mut usize) -> Result<Value, String> {
    expect_byte(src, pos, b'"')?;
    let mut result = String::new();
    let bytes = src.as_bytes();
    loop {
        if *pos >= bytes.len() {
            return Err("Unterminated string".into());
        }
        match bytes[*pos] {
            b'"' => { *pos += 1; break; }
            b'\\' => {
                *pos += 1;
                if *pos >= bytes.len() { return Err("Unterminated escape".into()); }
                match bytes[*pos] {
                    b'"'  => { result.push('"');  *pos += 1; }
                    b'\\' => { result.push('\\'); *pos += 1; }
                    b'/'  => { result.push('/');  *pos += 1; }
                    b'n'  => { result.push('\n'); *pos += 1; }
                    b'r'  => { result.push('\r'); *pos += 1; }
                    b't'  => { result.push('\t'); *pos += 1; }
                    b'b'  => { result.push('\x08'); *pos += 1; }
                    b'f'  => { result.push('\x0C'); *pos += 1; }
                    b'u'  => {
                        // \uXXXX unicode escape
                        *pos += 1;
                        if *pos + 4 > src.len() {
                            return Err("Incomplete \\u escape".into());
                        }
                        let hex = &src[*pos..*pos + 4];
                        let codepoint = u32::from_str_radix(hex, 16)
                            .map_err(|_| format!("Invalid \\u escape: {}", hex))?;
                        let ch = char::from_u32(codepoint)
                            .ok_or_else(|| format!("Invalid unicode codepoint: {}", codepoint))?;
                        result.push(ch);
                        *pos += 4;
                    }
                    c => {
                        result.push('\\');
                        result.push(c as char);
                        *pos += 1;
                    }
                }
            }
            c => {
                // Decode UTF-8 manually to handle multi-byte chars
                let s = &src[*pos..];
                let ch = s.chars().next().unwrap();
                result.push(ch);
                *pos += ch.len_utf8();
            }
        }
    }
    Ok(Value::Str(result))
}

fn parse_number(src: &str, pos: &mut usize) -> Result<Value, String> {
    let start = *pos;
    let bytes = src.as_bytes();
    let mut is_float = false;

    // Optional minus
    if bytes.get(*pos) == Some(&b'-') { *pos += 1; }

    // Integer part
    match bytes.get(*pos) {
        Some(b'0') => { *pos += 1; }
        Some(b'1'..=b'9') => {
            while matches!(bytes.get(*pos), Some(b'0'..=b'9')) { *pos += 1; }
        }
        _ => return Err(format!("Invalid number at position {}", start)),
    }

    // Fractional part
    if bytes.get(*pos) == Some(&b'.') {
        is_float = true;
        *pos += 1;
        if !matches!(bytes.get(*pos), Some(b'0'..=b'9')) {
            return Err(format!("Expected digit after decimal point at {}", *pos));
        }
        while matches!(bytes.get(*pos), Some(b'0'..=b'9')) { *pos += 1; }
    }

    // Exponent
    if matches!(bytes.get(*pos), Some(b'e') | Some(b'E')) {
        is_float = true;
        *pos += 1;
        if matches!(bytes.get(*pos), Some(b'+') | Some(b'-')) { *pos += 1; }
        if !matches!(bytes.get(*pos), Some(b'0'..=b'9')) {
            return Err(format!("Expected digit in exponent at {}", *pos));
        }
        while matches!(bytes.get(*pos), Some(b'0'..=b'9')) { *pos += 1; }
    }

    let num_str = &src[start..*pos];
    if is_float {
        num_str.parse::<f64>()
            .map(Value::Float)
            .map_err(|_| format!("Invalid float: {}", num_str))
    } else {
        // Try i64 first, fall back to f64 for large numbers
        if let Ok(n) = num_str.parse::<i64>() {
            Ok(Value::Int(n))
        } else {
            num_str.parse::<f64>()
                .map(Value::Float)
                .map_err(|_| format!("Invalid number: {}", num_str))
        }
    }
}

fn parse_array(src: &str, pos: &mut usize) -> Result<Value, String> {
    expect_byte(src, pos, b'[')?;
    skip_ws(src, pos);
    let mut items = Vec::new();

    if src.as_bytes().get(*pos) == Some(&b']') {
        *pos += 1;
        return Ok(Value::List(Rc::new(RefCell::new(items))));
    }

    loop {
        skip_ws(src, pos);
        items.push(parse_value(src, pos)?);
        skip_ws(src, pos);
        match src.as_bytes().get(*pos) {
            Some(b',') => { *pos += 1; }
            Some(b']') => { *pos += 1; break; }
            _ => return Err(format!("Expected ',' or ']' in array at position {}", pos)),
        }
    }
    Ok(Value::List(Rc::new(RefCell::new(items))))
}

fn parse_object(src: &str, pos: &mut usize) -> Result<Value, String> {
    expect_byte(src, pos, b'{')?;
    skip_ws(src, pos);
    let mut map = HashMap::new();

    if src.as_bytes().get(*pos) == Some(&b'}') {
        *pos += 1;
        return Ok(Value::Map(Rc::new(RefCell::new(map))));
    }

    loop {
        skip_ws(src, pos);

        // Key must be a string
        let key = match parse_string(src, pos) {
            Ok(Value::Str(s)) => s,
            Ok(_) => return Err("Object key must be a string".into()),
            Err(e) => return Err(e),
        };

        skip_ws(src, pos);
        expect_byte(src, pos, b':')?;
        skip_ws(src, pos);

        let val = parse_value(src, pos)?;
        map.insert(key, val);
        skip_ws(src, pos);

        match src.as_bytes().get(*pos) {
            Some(b',') => { *pos += 1; }
            Some(b'}') => { *pos += 1; break; }
            _ => return Err(format!("Expected ',' or '}}' in object at position {}", pos)),
        }
    }
    Ok(Value::Map(Rc::new(RefCell::new(map))))
}

// ═══════════════════════════════════════════════════════════
// JSON Serializer
// ═══════════════════════════════════════════════════════════

fn serialize(val: &Value, depth: usize, pretty: bool) -> Result<String, String> {
    let indent = if pretty { "  ".repeat(depth) } else { String::new() };
    let inner_indent = if pretty { "  ".repeat(depth + 1) } else { String::new() };
    let nl = if pretty { "\n" } else { "" };
    let sp = if pretty { " " } else { "" };

    match val {
        Value::Nil                     => Ok("null".into()),
        Value::Bool(b)                 => Ok(b.to_string()),
        Value::Int(n)                  => Ok(n.to_string()),
        Value::Float(f) => {
            if f.is_nan() || f.is_infinite() {
                Ok("null".into()) // JSON has no NaN/Infinity
            } else if f.fract() == 0.0 && f.abs() < 1e15 {
                Ok(format!("{:.1}", f))
            } else {
                Ok(f.to_string())
            }
        }
        Value::Str(s) => Ok(escape_string(s)),

        Value::List(v) => {
            let items = v.borrow();
            if items.is_empty() {
                return Ok("[]".into());
            }
            let mut out = format!("[{}", nl);
            for (i, item) in items.iter().enumerate() {
                out.push_str(&inner_indent);
                out.push_str(&serialize(item, depth + 1, pretty)?);
                if i + 1 < items.len() { out.push(','); }
                out.push_str(nl);
            }
            out.push_str(&indent);
            out.push(']');
            Ok(out)
        }

        Value::Map(m) => {
            let map = m.borrow();
            if map.is_empty() {
                return Ok("{}".into());
            }
            // Sort keys for deterministic output
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut out = format!("{{{}", nl);
            for (i, key) in keys.iter().enumerate() {
                let v = &map[*key];
                out.push_str(&inner_indent);
                out.push_str(&escape_string(key));
                out.push(':');
                out.push_str(sp);
                out.push_str(&serialize(v, depth + 1, pretty)?);
                if i + 1 < keys.len() { out.push(','); }
                out.push_str(nl);
            }
            out.push_str(&indent);
            out.push('}');
            Ok(out)
        }

        Value::Tuple(v) => {
            // Tuples serialize as arrays
            let mut out = format!("[{}", nl);
            for (i, item) in v.iter().enumerate() {
                out.push_str(&inner_indent);
                out.push_str(&serialize(item, depth + 1, pretty)?);
                if i + 1 < v.len() { out.push(','); }
                out.push_str(nl);
            }
            out.push_str(&indent);
            out.push(']');
            Ok(out)
        }

        Value::Option(Some(v)) => serialize(v, depth, pretty),
        Value::Option(None)    => Ok("null".into()),

        Value::Result(Ok(v)) => {
            let inner = serialize(v, depth + 1, pretty)?;
            if pretty {
                Ok(format!("{{\n{}\"ok\":{}{}\n{}}}", inner_indent, sp, inner, indent))
            } else {
                Ok(format!("{{\"ok\":{}}}", inner))
            }
        }
        Value::Result(Err(e)) => {
            let inner = serialize(e, depth + 1, pretty)?;
            if pretty {
                Ok(format!("{{\n{}\"err\":{}{}\n{}}}", inner_indent, sp, inner, indent))
            } else {
                Ok(format!("{{\"err\":{}}}", inner))
            }
        }

        Value::Struct(name, fields) => {
            let map = fields.borrow();
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut out = format!("{{{}", nl);
            // Include __type for round-trip awareness
            out.push_str(&inner_indent);
            out.push_str(&format!("\"__type\":{}{}", sp, escape_string(name)));
            if !keys.is_empty() { out.push(','); }
            out.push_str(nl);
            for (i, key) in keys.iter().enumerate() {
                out.push_str(&inner_indent);
                out.push_str(&escape_string(key));
                out.push(':');
                out.push_str(sp);
                out.push_str(&serialize(&map[*key], depth + 1, pretty)?);
                if i + 1 < keys.len() { out.push(','); }
                out.push_str(nl);
            }
            out.push_str(&indent);
            out.push('}');
            Ok(out)
        }

        Value::Enum(type_name, variant, fields) => {
            let mut out = format!("{{{}", nl);
            out.push_str(&format!("{}\"__type\":{}{},{}", inner_indent, sp, escape_string(type_name), nl));
            out.push_str(&format!("{}\"__variant\":{}{}", inner_indent, sp, escape_string(variant)));
            if !fields.is_empty() {
                out.push(',');
                out.push_str(nl);
                out.push_str(&inner_indent);
                out.push_str("\"fields\":");
                out.push_str(sp);
                let arr: Result<Vec<String>, String> = fields.iter()
                    .map(|f| serialize(f, depth + 1, pretty))
                    .collect();
                out.push_str(&format!("[{}]", arr?.join(",")));
            }
            out.push_str(nl);
            out.push_str(&indent);
            out.push('}');
            Ok(out)
        }

        Value::Ref(r) => serialize(&r.borrow(), depth, pretty),

        Value::Function(_) => Err("Functions cannot be serialized to JSON".into()),
    }
}

fn escape_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"'  => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\x08' => out.push_str("\\b"),
            '\x0C' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04X}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn get_key(val: &Value, key: &str) -> Value {
    match val {
        Value::Map(m) => m.borrow().get(key).cloned().unwrap_or(Value::Nil),
        _             => Value::Nil,
    }
}

fn require_str(args: &[Value], idx: usize, sig: &str) -> Result<String, String> {
    match args.get(idx) {
        Some(Value::Str(s)) => Ok(s.clone()),
        Some(other)         => Err(format!("{}: argument {} must be a String, got {}", sig, idx + 1, crate::interpreter::value_type_name(other))),
        None                => Err(format!("{}: argument {} is required", sig, idx + 1)),
    }
}

fn ok_val(v: Value) -> Value {
    Value::Result(std::result::Result::Ok(Box::new(v)))
}

fn err_val(msg: String) -> Value {
    Value::Result(std::result::Result::Err(Box::new(Value::Str(msg))))
}