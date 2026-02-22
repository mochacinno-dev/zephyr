// ═══════════════════════════════════════════════════════════
// Zephyr FS — file system I/O, directory ops, path utilities
// Zero external dependencies — pure std::fs / std::path.
// ═══════════════════════════════════════════════════════════
//
// QUICK REFERENCE
// ───────────────────────────────────────────────────────────
//
// FILE READ
//   file_read(path)                   -> Result<String, String>
//   file_read_lines(path)             -> Result<List<String>, String>
//   file_read_bytes(path)             -> Result<List<Int>, String>
//   file_read_json(path)              -> Result<Value, String>
//
// FILE WRITE
//   file_write(path, content)         -> Result<Nil, String>
//   file_append(path, content)        -> Result<Nil, String>
//   file_write_lines(path, lines)     -> Result<Nil, String>
//   file_write_json(path, val)        -> Result<Nil, String>
//   file_write_json_pretty(path, val) -> Result<Nil, String>
//
// FILE INFO
//   file_exists(path)                 -> Bool
//   file_size(path)                   -> Result<Int, String>
//   file_ext(path)                    -> String         ("" if none)
//   file_name(path)                   -> String         ("" if none)
//   file_stem(path)                   -> String         ("" if none)
//   file_modified(path)               -> Result<Int, String>  (unix secs)
//
// FILE OPS
//   file_delete(path)                 -> Result<Nil, String>
//   file_copy(src, dst)               -> Result<Nil, String>
//   file_move(src, dst)               -> Result<Nil, String>
//
// DIRECTORY
//   dir_list(path)                    -> Result<List<String>, String>
//   dir_list_full(path)               -> Result<List<String>, String>
//   dir_list_info(path)               -> Result<List<Map>, String>
//   dir_create(path)                  -> Result<Nil, String>
//   dir_delete(path)                  -> Result<Nil, String>
//   dir_delete_all(path)              -> Result<Nil, String>
//   dir_exists(path)                  -> Bool
//   dir_copy(src, dst)                -> Result<Nil, String>
//
// PATH UTILITIES
//   path_join(a, b)                   -> String
//   path_abs(path)                    -> Result<String, String>
//   path_parent(path)                 -> String         ("." if none)
//   path_exists(path)                 -> Bool
//   path_is_file(path)                -> Bool
//   path_is_dir(path)                 -> Bool
//   path_expand(path)                 -> String         (expands ~)
//
// TEMP FILES
//   temp_file(prefix)                 -> Result<String, String>
//   temp_dir(prefix)                  -> Result<String, String>
//
// ═══════════════════════════════════════════════════════════

use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use crate::interpreter::Value;
use crate::json;

// ── Registration ──────────────────────────────────────────────────────────────

pub fn fs_functions() -> Vec<&'static str> {
    vec![
        // File read
        "file_read",
        "file_read_lines",
        "file_read_bytes",
        "file_read_json",
        // File write
        "file_write",
        "file_append",
        "file_write_lines",
        "file_write_json",
        "file_write_json_pretty",
        // File info
        "file_exists",
        "file_size",
        "file_ext",
        "file_name",
        "file_stem",
        "file_modified",
        // File ops
        "file_delete",
        "file_copy",
        "file_move",
        // Directory
        "dir_list",
        "dir_list_full",
        "dir_list_info",
        "dir_create",
        "dir_delete",
        "dir_delete_all",
        "dir_exists",
        "dir_copy",
        // Path utils
        "path_join",
        "path_abs",
        "path_parent",
        "path_exists",
        "path_is_file",
        "path_is_dir",
        "path_expand",
        // Temp
        "temp_file",
        "temp_dir",
    ]
}

// ── Dispatch ──────────────────────────────────────────────────────────────────

pub fn call_fs(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        // File read
        "file_read"             => fs_file_read(args),
        "file_read_lines"       => fs_file_read_lines(args),
        "file_read_bytes"       => fs_file_read_bytes(args),
        "file_read_json"        => fs_file_read_json(args),
        // File write
        "file_write"            => fs_file_write(args),
        "file_append"           => fs_file_append(args),
        "file_write_lines"      => fs_file_write_lines(args),
        "file_write_json"       => fs_file_write_json(args),
        "file_write_json_pretty"=> fs_file_write_json_pretty(args),
        // File info
        "file_exists"           => fs_file_exists(args),
        "file_size"             => fs_file_size(args),
        "file_ext"              => fs_file_ext(args),
        "file_name"             => fs_file_name(args),
        "file_stem"             => fs_file_stem(args),
        "file_modified"         => fs_file_modified(args),
        // File ops
        "file_delete"           => fs_file_delete(args),
        "file_copy"             => fs_file_copy(args),
        "file_move"             => fs_file_move(args),
        // Directory
        "dir_list"              => fs_dir_list(args),
        "dir_list_full"         => fs_dir_list_full(args),
        "dir_list_info"         => fs_dir_list_info(args),
        "dir_create"            => fs_dir_create(args),
        "dir_delete"            => fs_dir_delete(args),
        "dir_delete_all"        => fs_dir_delete_all(args),
        "dir_exists"            => fs_dir_exists(args),
        "dir_copy"              => fs_dir_copy(args),
        // Path utils
        "path_join"             => fs_path_join(args),
        "path_abs"              => fs_path_abs(args),
        "path_parent"           => fs_path_parent(args),
        "path_exists"           => fs_path_exists(args),
        "path_is_file"          => fs_path_is_file(args),
        "path_is_dir"           => fs_path_is_dir(args),
        "path_expand"           => fs_path_expand(args),
        // Temp
        "temp_file"             => fs_temp_file(args),
        "temp_dir"              => fs_temp_dir(args),
        _                       => Err(format!("Unknown fs function '{}'", name)),
    }
}

// ═══════════════════════════════════════════════════════════
// FILE READ
// ═══════════════════════════════════════════════════════════

/// file_read(path: String) -> Result<String, String>
///
/// Reads an entire file and returns its contents as a UTF-8 string.
/// Non-UTF-8 bytes are replaced with the Unicode replacement character.
///
/// Example:
///   let res = file_read("/etc/hostname")
///   match res {
///       Ok(contents) => println(contents)
///       Err(e)       => println("Read error: #{e}")
///   }
fn fs_file_read(args: Vec<Value>) -> Result<Value, String> {
    let path = require_str(&args, 0, "file_read(path)")?;
    match fs::read(&path) {
        Ok(bytes) => Ok(ok_val(Value::Str(String::from_utf8_lossy(&bytes).into_owned()))),
        Err(e)    => Ok(err_val(format!("file_read '{}': {}", path, e))),
    }
}

/// file_read_lines(path: String) -> Result<List<String>, String>
///
/// Reads a file and returns its lines as a List of strings.
/// Line endings (\n and \r\n) are stripped from each line.
///
/// Example:
///   let res = file_read_lines("/etc/hosts")
///   match res {
///       Ok(lines) => {
///           for line in lines {
///               if !line.starts_with("#") {
///                   println(line)
///               }
///           }
///       }
///       Err(e) => println(e)
///   }
fn fs_file_read_lines(args: Vec<Value>) -> Result<Value, String> {
    let path = require_str(&args, 0, "file_read_lines(path)")?;
    match fs::read_to_string(&path) {
        Ok(contents) => {
            let lines: Vec<Value> = contents
                .lines()
                .map(|l| Value::Str(l.to_string()))
                .collect();
            Ok(ok_val(Value::List(Rc::new(RefCell::new(lines)))))
        }
        Err(e) => Ok(err_val(format!("file_read_lines '{}': {}", path, e))),
    }
}

/// file_read_bytes(path: String) -> Result<List<Int>, String>
///
/// Reads a file as raw bytes and returns them as a List of Ints (0–255).
/// Useful for binary files, checksums, or byte-level manipulation.
///
/// Example:
///   let res = file_read_bytes("/bin/ls")
///   match res {
///       Ok(bytes) => println("File has #{bytes.len()} bytes")
///       Err(e)    => println(e)
///   }
fn fs_file_read_bytes(args: Vec<Value>) -> Result<Value, String> {
    let path = require_str(&args, 0, "file_read_bytes(path)")?;
    match fs::read(&path) {
        Ok(bytes) => {
            let vals: Vec<Value> = bytes.iter().map(|&b| Value::Int(b as i64)).collect();
            Ok(ok_val(Value::List(Rc::new(RefCell::new(vals)))))
        }
        Err(e) => Ok(err_val(format!("file_read_bytes '{}': {}", path, e))),
    }
}

/// file_read_json(path: String) -> Result<Value, String>
///
/// Reads a file and parses its contents as JSON in one call.
/// Equivalent to: json_parse(file_read(path).unwrap_or(""))
///
/// Example:
///   let config = file_read_json("config.json")
///   match config {
///       Ok(data) => {
///           let host = json_get(data, "host")
///           let port = json_get(data, "port")
///           println("Connecting to #{host}:#{port}")
///       }
///       Err(e) => println("Config error: #{e}")
///   }
fn fs_file_read_json(args: Vec<Value>) -> Result<Value, String> {
    let path = require_str(&args, 0, "file_read_json(path)")?;
    let contents = match fs::read_to_string(&path) {
        Ok(s)  => s,
        Err(e) => return Ok(err_val(format!("file_read_json '{}': {}", path, e))),
    };
    // Delegate to json module
    crate::json::call_json("json_parse", vec![Value::Str(contents)])
}

// ═══════════════════════════════════════════════════════════
// FILE WRITE
// ═══════════════════════════════════════════════════════════

/// file_write(path: String, content: String) -> Result<Nil, String>
///
/// Writes a string to a file, creating it if it doesn't exist
/// and overwriting it if it does.
/// Parent directories must already exist.
///
/// Example:
///   let res = file_write("/tmp/hello.txt", "Hello, Zephyr!\n")
///   match res {
///       Ok(_)  => println("Written")
///       Err(e) => println("Write failed: #{e}")
///   }
fn fs_file_write(args: Vec<Value>) -> Result<Value, String> {
    let path    = require_str(&args, 0, "file_write(path, content)")?;
    let content = require_str(&args, 1, "file_write(path, content)")?;
    match fs::write(&path, content.as_bytes()) {
        Ok(_)  => Ok(ok_val(Value::Nil)),
        Err(e) => Ok(err_val(format!("file_write '{}': {}", path, e))),
    }
}

/// file_append(path: String, content: String) -> Result<Nil, String>
///
/// Appends a string to a file, creating the file if it doesn't exist.
///
/// Example:
///   file_append("/tmp/log.txt", "#{timestamp}: event happened\n")
fn fs_file_append(args: Vec<Value>) -> Result<Value, String> {
    let path    = require_str(&args, 0, "file_append(path, content)")?;
    let content = require_str(&args, 1, "file_append(path, content)")?;
    let result = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut f| f.write_all(content.as_bytes()));
    match result {
        Ok(_)  => Ok(ok_val(Value::Nil)),
        Err(e) => Ok(err_val(format!("file_append '{}': {}", path, e))),
    }
}

/// file_write_lines(path: String, lines: List<String>) -> Result<Nil, String>
///
/// Writes a list of strings to a file, joining them with newlines.
/// A trailing newline is added after the last line.
///
/// Example:
///   let lines = ["line one", "line two", "line three"]
///   file_write_lines("/tmp/output.txt", lines)
fn fs_file_write_lines(args: Vec<Value>) -> Result<Value, String> {
    let path = require_str(&args, 0, "file_write_lines(path, lines)")?;
    let lines = match args.get(1) {
        Some(Value::List(v)) => v.borrow().iter()
            .map(|x| format!("{}", x))
            .collect::<Vec<_>>()
            .join("\n") + "\n",
        Some(other) => return Ok(err_val(format!(
            "file_write_lines: expected List, got {}", crate::interpreter::value_type_name(other)
        ))),
        None => return Ok(err_val("file_write_lines: lines argument required".into())),
    };
    match fs::write(&path, lines.as_bytes()) {
        Ok(_)  => Ok(ok_val(Value::Nil)),
        Err(e) => Ok(err_val(format!("file_write_lines '{}': {}", path, e))),
    }
}

/// file_write_json(path: String, val: Value) -> Result<Nil, String>
///
/// Serializes a Zephyr value as compact JSON and writes it to a file.
///
/// Example:
///   var config = {}
///   config["host"] = "localhost"
///   config["port"] = 8080
///   file_write_json("config.json", config)
fn fs_file_write_json(args: Vec<Value>) -> Result<Value, String> {
    let path = require_str(&args, 0, "file_write_json(path, val)")?;
    let val  = args.into_iter().nth(1)
        .ok_or_else(|| "file_write_json: value argument required".to_string())?;
    let json_str = match crate::json::call_json("json_stringify", vec![val])? {
        Value::Result(Ok(v)) => match *v {
            Value::Str(s) => s,
            _ => return Ok(err_val("file_write_json: serialization returned non-string".into())),
        },
        Value::Result(Err(e)) => return Ok(Value::Result(Err(e))),
        _ => return Ok(err_val("file_write_json: unexpected return from json_stringify".into())),
    };
    match fs::write(&path, json_str.as_bytes()) {
        Ok(_)  => Ok(ok_val(Value::Nil)),
        Err(e) => Ok(err_val(format!("file_write_json '{}': {}", path, e))),
    }
}

/// file_write_json_pretty(path: String, val: Value) -> Result<Nil, String>
///
/// Serializes a Zephyr value as pretty-printed JSON (2-space indent)
/// and writes it to a file. Useful for human-readable config files.
///
/// Example:
///   file_write_json_pretty("config.json", config)
fn fs_file_write_json_pretty(args: Vec<Value>) -> Result<Value, String> {
    let path = require_str(&args, 0, "file_write_json_pretty(path, val)")?;
    let val  = args.into_iter().nth(1)
        .ok_or_else(|| "file_write_json_pretty: value argument required".to_string())?;
    let json_str = match crate::json::call_json("json_pretty", vec![val])? {
        Value::Result(Ok(v)) => match *v {
            Value::Str(s) => s,
            _ => return Ok(err_val("file_write_json_pretty: serialization returned non-string".into())),
        },
        Value::Result(Err(e)) => return Ok(Value::Result(Err(e))),
        _ => return Ok(err_val("unexpected return from json_pretty".into())),
    };
    // Pretty JSON files get a trailing newline
    let json_str = if json_str.ends_with('\n') { json_str } else { json_str + "\n" };
    match fs::write(&path, json_str.as_bytes()) {
        Ok(_)  => Ok(ok_val(Value::Nil)),
        Err(e) => Ok(err_val(format!("file_write_json_pretty '{}': {}", path, e))),
    }
}

// ═══════════════════════════════════════════════════════════
// FILE INFO
// ═══════════════════════════════════════════════════════════

/// file_exists(path: String) -> Bool
///
/// Returns true if the path exists and is a regular file.
/// Returns false for directories, symlinks to directories, or missing paths.
///
/// Example:
///   if file_exists("/etc/pacman.conf") {
///       println("pacman.conf found")
///   }
fn fs_file_exists(args: Vec<Value>) -> Result<Value, String> {
    let path = require_str(&args, 0, "file_exists(path)")?;
    Ok(Value::Bool(Path::new(&path).is_file()))
}

/// file_size(path: String) -> Result<Int, String>
///
/// Returns the file size in bytes.
///
/// Example:
///   let size = file_size("/var/log/pacman.log")
///   match size {
///       Ok(n)  => println("Log is #{n} bytes")
///       Err(e) => println(e)
///   }
fn fs_file_size(args: Vec<Value>) -> Result<Value, String> {
    let path = require_str(&args, 0, "file_size(path)")?;
    match fs::metadata(&path) {
        Ok(m)  => Ok(ok_val(Value::Int(m.len() as i64))),
        Err(e) => Ok(err_val(format!("file_size '{}': {}", path, e))),
    }
}

/// file_ext(path: String) -> String
///
/// Returns the file extension without the leading dot, or "" if none.
///
/// Examples:
///   file_ext("archive.tar.gz")  => "gz"
///   file_ext("README")          => ""
///   file_ext(".hidden")         => ""
fn fs_file_ext(args: Vec<Value>) -> Result<Value, String> {
    let path = require_str(&args, 0, "file_ext(path)")?;
    let ext = Path::new(&path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_string();
    Ok(Value::Str(ext))
}

/// file_name(path: String) -> String
///
/// Returns the final component of the path (filename + extension).
/// Returns "" if the path ends in "..".
///
/// Examples:
///   file_name("/home/user/notes.txt")  => "notes.txt"
///   file_name("/home/user/")           => "user"
fn fs_file_name(args: Vec<Value>) -> Result<Value, String> {
    let path = require_str(&args, 0, "file_name(path)")?;
    let name = Path::new(&path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    Ok(Value::Str(name))
}

/// file_stem(path: String) -> String
///
/// Returns the filename without its extension.
///
/// Examples:
///   file_stem("/home/user/notes.txt")  => "notes"
///   file_stem("archive.tar.gz")        => "archive.tar"
fn fs_file_stem(args: Vec<Value>) -> Result<Value, String> {
    let path = require_str(&args, 0, "file_stem(path)")?;
    let stem = Path::new(&path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    Ok(Value::Str(stem))
}

/// file_modified(path: String) -> Result<Int, String>
///
/// Returns the last-modified time of a file as a Unix timestamp (seconds).
///
/// Example:
///   let ts = file_modified("/etc/hostname")
///   match ts {
///       Ok(t)  => println("Modified at Unix time #{t}")
///       Err(e) => println(e)
///   }
fn fs_file_modified(args: Vec<Value>) -> Result<Value, String> {
    let path = require_str(&args, 0, "file_modified(path)")?;
    match fs::metadata(&path) {
        Ok(m) => match m.modified() {
            Ok(time) => {
                let secs = time.duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                Ok(ok_val(Value::Int(secs)))
            }
            Err(e) => Ok(err_val(format!("file_modified '{}': {}", path, e))),
        },
        Err(e) => Ok(err_val(format!("file_modified '{}': {}", path, e))),
    }
}

// ═══════════════════════════════════════════════════════════
// FILE OPS
// ═══════════════════════════════════════════════════════════

/// file_delete(path: String) -> Result<Nil, String>
///
/// Deletes a file. Returns Err if the file doesn't exist or is a directory.
///
/// Example:
///   let res = file_delete("/tmp/old_cache.dat")
///   match res {
///       Ok(_)  => println("Deleted")
///       Err(e) => println("Delete failed: #{e}")
///   }
fn fs_file_delete(args: Vec<Value>) -> Result<Value, String> {
    let path = require_str(&args, 0, "file_delete(path)")?;
    match fs::remove_file(&path) {
        Ok(_)  => Ok(ok_val(Value::Nil)),
        Err(e) => Ok(err_val(format!("file_delete '{}': {}", path, e))),
    }
}

/// file_copy(src: String, dst: String) -> Result<Nil, String>
///
/// Copies a file from src to dst, overwriting dst if it exists.
/// The destination's parent directory must already exist.
///
/// Example:
///   file_copy("/etc/pacman.conf", "/tmp/pacman.conf.bak")
fn fs_file_copy(args: Vec<Value>) -> Result<Value, String> {
    let src = require_str(&args, 0, "file_copy(src, dst)")?;
    let dst = require_str(&args, 1, "file_copy(src, dst)")?;
    match fs::copy(&src, &dst) {
        Ok(_)  => Ok(ok_val(Value::Nil)),
        Err(e) => Ok(err_val(format!("file_copy '{}' -> '{}': {}", src, dst, e))),
    }
}

/// file_move(src: String, dst: String) -> Result<Nil, String>
///
/// Moves (renames) a file from src to dst.
/// Works across directories on the same filesystem.
/// For cross-filesystem moves, copies then deletes.
///
/// Example:
///   file_move("/tmp/build/pkg.tar.zst", "/var/cache/pacman/pkg/pkg.tar.zst")
fn fs_file_move(args: Vec<Value>) -> Result<Value, String> {
    let src = require_str(&args, 0, "file_move(src, dst)")?;
    let dst = require_str(&args, 1, "file_move(src, dst)")?;
    // Try rename first (fast, same-filesystem)
    if fs::rename(&src, &dst).is_ok() {
        return Ok(ok_val(Value::Nil));
    }
    // Fall back to copy + delete (cross-filesystem)
    match fs::copy(&src, &dst) {
        Ok(_) => match fs::remove_file(&src) {
            Ok(_)  => Ok(ok_val(Value::Nil)),
            Err(e) => Ok(err_val(format!("file_move: copied but could not remove src '{}': {}", src, e))),
        },
        Err(e) => Ok(err_val(format!("file_move '{}' -> '{}': {}", src, dst, e))),
    }
}

// ═══════════════════════════════════════════════════════════
// DIRECTORY
// ═══════════════════════════════════════════════════════════

/// dir_list(path: String) -> Result<List<String>, String>
///
/// Returns the names of entries in a directory (not full paths).
/// Sorted alphabetically. Does not recurse into subdirectories.
///
/// Example:
///   let entries = dir_list("/etc")
///   match entries {
///       Ok(names) => for name in names { println(name) }
///       Err(e)    => println(e)
///   }
fn fs_dir_list(args: Vec<Value>) -> Result<Value, String> {
    let path = require_str(&args, 0, "dir_list(path)")?;
    read_dir_entries(&path, false, false)
}

/// dir_list_full(path: String) -> Result<List<String>, String>
///
/// Like dir_list() but returns absolute paths instead of just names.
///
/// Example:
///   let paths = dir_list_full("/etc/pacman.d")
///   match paths {
///       Ok(ps) => for p in ps { println(p) }
///       Err(e) => println(e)
///   }
fn fs_dir_list_full(args: Vec<Value>) -> Result<Value, String> {
    let path = require_str(&args, 0, "dir_list_full(path)")?;
    read_dir_entries(&path, true, false)
}

/// dir_list_info(path: String) -> Result<List<Map>, String>
///
/// Returns directory entries with metadata. Each entry is a Map:
///   {
///     name:   String   — filename only
///     path:   String   — full path
///     is_dir: Bool     — true if this entry is a directory
///     is_file: Bool    — true if this entry is a regular file
///     size:   Int      — file size in bytes (0 for directories)
///   }
///
/// Example:
///   let entries = dir_list_info(".")
///   match entries {
///       Ok(items) => {
///           for item in items {
///               let name   = item["name"]
///               let is_dir = item["is_dir"]
///               let size   = item["size"]
///               println("#{name} (dir=#{is_dir}, size=#{size})")
///           }
///       }
///       Err(e) => println(e)
///   }
fn fs_dir_list_info(args: Vec<Value>) -> Result<Value, String> {
    let path = require_str(&args, 0, "dir_list_info(path)")?;
    read_dir_entries(&path, true, true)
}

/// dir_create(path: String) -> Result<Nil, String>
///
/// Creates a directory and all necessary parent directories.
/// Equivalent to `mkdir -p`. Does not error if the directory already exists.
///
/// Example:
///   dir_create("/tmp/myapp/cache/v2")
fn fs_dir_create(args: Vec<Value>) -> Result<Value, String> {
    let path = require_str(&args, 0, "dir_create(path)")?;
    match fs::create_dir_all(&path) {
        Ok(_)  => Ok(ok_val(Value::Nil)),
        Err(e) => Ok(err_val(format!("dir_create '{}': {}", path, e))),
    }
}

/// dir_delete(path: String) -> Result<Nil, String>
///
/// Deletes an empty directory. Returns Err if the directory is not empty.
/// Use dir_delete_all() to remove a directory and its contents.
///
/// Example:
///   dir_delete("/tmp/empty_dir")
fn fs_dir_delete(args: Vec<Value>) -> Result<Value, String> {
    let path = require_str(&args, 0, "dir_delete(path)")?;
    match fs::remove_dir(&path) {
        Ok(_)  => Ok(ok_val(Value::Nil)),
        Err(e) => Ok(err_val(format!("dir_delete '{}': {}", path, e))),
    }
}

/// dir_delete_all(path: String) -> Result<Nil, String>
///
/// Recursively deletes a directory and all its contents.
/// Equivalent to `rm -rf`. Use with caution — this is irreversible.
///
/// Example:
///   dir_delete_all("/tmp/build_artifacts")
fn fs_dir_delete_all(args: Vec<Value>) -> Result<Value, String> {
    let path = require_str(&args, 0, "dir_delete_all(path)")?;
    match fs::remove_dir_all(&path) {
        Ok(_)  => Ok(ok_val(Value::Nil)),
        Err(e) => Ok(err_val(format!("dir_delete_all '{}': {}", path, e))),
    }
}

/// dir_exists(path: String) -> Bool
///
/// Returns true if the path exists and is a directory.
///
/// Example:
///   if !dir_exists("/tmp/cache") {
///       dir_create("/tmp/cache")
///   }
fn fs_dir_exists(args: Vec<Value>) -> Result<Value, String> {
    let path = require_str(&args, 0, "dir_exists(path)")?;
    Ok(Value::Bool(Path::new(&path).is_dir()))
}

/// dir_copy(src: String, dst: String) -> Result<Nil, String>
///
/// Recursively copies a directory from src to dst.
/// dst will be created if it doesn't exist.
/// Existing files in dst will be overwritten.
///
/// Example:
///   dir_copy("/home/user/.config/app", "/tmp/config_backup")
fn fs_dir_copy(args: Vec<Value>) -> Result<Value, String> {
    let src = require_str(&args, 0, "dir_copy(src, dst)")?;
    let dst = require_str(&args, 1, "dir_copy(src, dst)")?;
    match copy_dir_all(Path::new(&src), Path::new(&dst)) {
        Ok(_)  => Ok(ok_val(Value::Nil)),
        Err(e) => Ok(err_val(format!("dir_copy '{}' -> '{}': {}", src, dst, e))),
    }
}

// ═══════════════════════════════════════════════════════════
// PATH UTILITIES
// ═══════════════════════════════════════════════════════════

/// path_join(a: String, b: String) -> String
///
/// Joins two path segments correctly, handling trailing/leading slashes.
/// If b is an absolute path, it replaces a entirely (POSIX semantics).
///
/// Examples:
///   path_join("/home/user", "docs")       => "/home/user/docs"
///   path_join("/home/user/", "docs")      => "/home/user/docs"
///   path_join("/home/user", "/etc")       => "/etc"  (absolute replaces)
fn fs_path_join(args: Vec<Value>) -> Result<Value, String> {
    let a = require_str(&args, 0, "path_join(a, b)")?;
    let b = require_str(&args, 1, "path_join(a, b)")?;
    let joined = Path::new(&a).join(&b);
    Ok(Value::Str(joined.to_string_lossy().to_string()))
}

/// path_abs(path: String) -> Result<String, String>
///
/// Resolves a path to its absolute, canonical form.
/// Resolves . and .. and symlinks. The path must exist.
///
/// Example:
///   let abs = path_abs("../other/dir")
///   match abs {
///       Ok(p)  => println(p)
///       Err(e) => println(e)
///   }
fn fs_path_abs(args: Vec<Value>) -> Result<Value, String> {
    let path = require_str(&args, 0, "path_abs(path)")?;
    match fs::canonicalize(&path) {
        Ok(p)  => Ok(ok_val(Value::Str(p.to_string_lossy().to_string()))),
        Err(e) => Ok(err_val(format!("path_abs '{}': {}", path, e))),
    }
}

/// path_parent(path: String) -> String
///
/// Returns the parent directory of a path.
/// Returns "." if the path has no parent component.
///
/// Examples:
///   path_parent("/home/user/file.txt")  => "/home/user"
///   path_parent("/home/user/")          => "/home/user"
///   path_parent("file.txt")             => "."
fn fs_path_parent(args: Vec<Value>) -> Result<Value, String> {
    let path = require_str(&args, 0, "path_parent(path)")?;
    let parent = Path::new(&path)
        .parent()
        .map(|p| if p.as_os_str().is_empty() { ".".to_string() } else { p.to_string_lossy().to_string() })
        .unwrap_or_else(|| ".".to_string());
    Ok(Value::Str(parent))
}

/// path_exists(path: String) -> Bool
///
/// Returns true if the path exists (file, directory, symlink, or other).
fn fs_path_exists(args: Vec<Value>) -> Result<Value, String> {
    let path = require_str(&args, 0, "path_exists(path)")?;
    Ok(Value::Bool(Path::new(&path).exists()))
}

/// path_is_file(path: String) -> Bool
///
/// Returns true if the path exists and is a regular file (not a directory).
fn fs_path_is_file(args: Vec<Value>) -> Result<Value, String> {
    let path = require_str(&args, 0, "path_is_file(path)")?;
    Ok(Value::Bool(Path::new(&path).is_file()))
}

/// path_is_dir(path: String) -> Bool
///
/// Returns true if the path exists and is a directory.
fn fs_path_is_dir(args: Vec<Value>) -> Result<Value, String> {
    let path = require_str(&args, 0, "path_is_dir(path)")?;
    Ok(Value::Bool(Path::new(&path).is_dir()))
}

/// path_expand(path: String) -> String
///
/// Expands a leading ~ to the user's home directory.
/// Uses the HOME environment variable, falls back to /root.
/// Does not resolve other shell expansions like $VAR.
///
/// Examples:
///   path_expand("~/.config/zephyr")  => "/home/alice/.config/zephyr"
///   path_expand("~")                 => "/home/alice"
///   path_expand("/absolute/path")    => "/absolute/path"  (unchanged)
fn fs_path_expand(args: Vec<Value>) -> Result<Value, String> {
    let path = require_str(&args, 0, "path_expand(path)")?;
    let expanded = if path == "~" {
        home_dir()
    } else if path.starts_with("~/") {
        format!("{}/{}", home_dir(), &path[2..])
    } else if path.starts_with("~\\") {
        format!("{}\\{}", home_dir(), &path[2..])
    } else {
        path
    };
    Ok(Value::Str(expanded))
}

// ═══════════════════════════════════════════════════════════
// TEMP FILES
// ═══════════════════════════════════════════════════════════

/// temp_file(prefix: String) -> Result<String, String>
///
/// Creates an empty temporary file and returns its path.
/// The file is created in the system temp directory.
/// The caller is responsible for deleting it when done.
///
/// Example:
///   let tmp = temp_file("zephyr_build_")
///   match tmp {
///       Ok(path) => {
///           file_write(path, "temp data")
///           // ... use the file ...
///           file_delete(path)
///       }
///       Err(e) => println(e)
///   }
fn fs_temp_file(args: Vec<Value>) -> Result<Value, String> {
    let prefix = args.get(0)
        .map(|v| format!("{}", v))
        .unwrap_or_else(|| "zephyr_".to_string());
    let tmp_dir = std::env::temp_dir();
    // Use a timestamp + simple counter for uniqueness
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let path = tmp_dir.join(format!("{}{}", prefix, ts));
    match fs::File::create(&path) {
        Ok(_)  => Ok(ok_val(Value::Str(path.to_string_lossy().to_string()))),
        Err(e) => Ok(err_val(format!("temp_file: {}", e))),
    }
}

/// temp_dir(prefix: String) -> Result<String, String>
///
/// Creates a temporary directory and returns its path.
/// The caller is responsible for deleting it when done (use dir_delete_all).
///
/// Example:
///   let build_dir = temp_dir("zephyr_build_")
///   match build_dir {
///       Ok(dir) => {
///           // build things in dir
///           dir_delete_all(dir)
///       }
///       Err(e) => println(e)
///   }
fn fs_temp_dir(args: Vec<Value>) -> Result<Value, String> {
    let prefix = args.get(0)
        .map(|v| format!("{}", v))
        .unwrap_or_else(|| "zephyr_".to_string());
    let tmp_dir = std::env::temp_dir();
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let path = tmp_dir.join(format!("{}{}", prefix, ts));
    match fs::create_dir_all(&path) {
        Ok(_)  => Ok(ok_val(Value::Str(path.to_string_lossy().to_string()))),
        Err(e) => Ok(err_val(format!("temp_dir: {}", e))),
    }
}

// ═══════════════════════════════════════════════════════════
// Internal helpers
// ═══════════════════════════════════════════════════════════

fn require_str(args: &[Value], idx: usize, sig: &str) -> Result<String, String> {
    match args.get(idx) {
        Some(Value::Str(s)) => Ok(s.clone()),
        Some(other) => Ok(format!("{}", other)),
        None => Err(format!("{}: argument {} is required", sig, idx + 1)),
    }
}

fn ok_val(v: Value) -> Value {
    Value::Result(std::result::Result::Ok(Box::new(v)))
}

fn err_val(msg: String) -> Value {
    Value::Result(std::result::Result::Err(Box::new(Value::Str(msg))))
}

fn home_dir() -> String {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| "/root".to_string())
}

/// Shared implementation for dir_list, dir_list_full, dir_list_info.
fn read_dir_entries(path: &str, full_paths: bool, with_info: bool) -> Result<Value, String> {
    let read = match fs::read_dir(path) {
        Ok(r)  => r,
        Err(e) => return Ok(err_val(format!("dir_list '{}': {}", path, e))),
    };

    let mut entries: Vec<(String, PathBuf)> = Vec::new();
    for entry in read {
        match entry {
            Ok(e) => {
                let name = e.file_name().to_string_lossy().to_string();
                let full = e.path();
                entries.push((name, full));
            }
            Err(e) => return Ok(err_val(format!("dir_list entry error: {}", e))),
        }
    }

    // Sort alphabetically by name
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    if with_info {
        let items: Vec<Value> = entries.into_iter().map(|(name, full_path)| {
            let meta = fs::metadata(&full_path);
            let is_dir  = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
            let is_file = meta.as_ref().map(|m| m.is_file()).unwrap_or(false);
            let size    = meta.as_ref().map(|m| m.len() as i64).unwrap_or(0);
            let mut map = HashMap::new();
            map.insert("name".into(),    Value::Str(name));
            map.insert("path".into(),    Value::Str(full_path.to_string_lossy().to_string()));
            map.insert("is_dir".into(),  Value::Bool(is_dir));
            map.insert("is_file".into(), Value::Bool(is_file));
            map.insert("size".into(),    Value::Int(size));
            Value::Map(Rc::new(RefCell::new(map)))
        }).collect();
        Ok(ok_val(Value::List(Rc::new(RefCell::new(items)))))
    } else {
        let items: Vec<Value> = entries.into_iter().map(|(name, full_path)| {
            if full_paths {
                Value::Str(full_path.to_string_lossy().to_string())
            } else {
                Value::Str(name)
            }
        }).collect();
        Ok(ok_val(Value::List(Rc::new(RefCell::new(items)))))
    }
}

/// Recursively copy a directory tree.
fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry   = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_all(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}