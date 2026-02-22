// ═══════════════════════════════════════════════════════════
// Zephyr Process — shell execution and environment access
// ═══════════════════════════════════════════════════════════
//
// QUICK REFERENCE
// ───────────────────────────────────────────────────────────
//
//  SIMPLE EXECUTION
//  exec(cmd)
//      String -> Result<String, String>
//      Run a shell command via /bin/sh. Returns Ok(stdout) on
//      exit code 0, Err(stderr) otherwise. Trailing newline stripped.
//
//  exec_out(cmd)
//      String -> Map {stdout, stderr, code, ok}
//      Like exec() but always succeeds. Returns all outputs.
//      Use when you need stderr or the exit code regardless of success.
//
//  exec_status(cmd)
//      String -> Int
//      Run a command, return only its exit code. Never errors.
//
//  exec_ok(cmd)
//      String -> Bool
//      True if the command exits with code 0. Good for checks.
//
//  EXPLICIT ARGUMENT LISTS (no shell, no injection risk)
//  shell(program, args)
//      String, List<String> -> Map {stdout, stderr, code, ok}
//      Run a program directly with an explicit argument list.
//      No /bin/sh involved — args are passed verbatim.
//
//  FULL CONTROL
//  process_run(program, args, env, cwd)
//      String, List<String>, Map<String,String>, String -> Map
//      Run a program with explicit args, extra env vars, and a
//      working directory. Pass nil/empty for defaults.
//      Returns {stdout, stderr, code, ok}.
//
//  STREAMING
//  process_spawn(cmd)
//      String -> Result<Int, String>
//      Run a shell command, streaming stdout/stderr directly to
//      the terminal in real time. Returns Ok(exit_code).
//      Use for long-running commands where you want live output.
//
//  ENVIRONMENT
//  env_get(key)      String -> Value (Str or Nil)
//  env_set(key, val) String, String -> Nil
//  env_all()         -> Map<String, String>
//
//  WORKING DIRECTORY
//  cwd()             -> String
//  set_cwd(path)     String -> Result<Nil, String>
//
// ═══════════════════════════════════════════════════════════

use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;
use std::process::{Command, Stdio};
use std::env;
use crate::interpreter::Value;

// ── Registration ──────────────────────────────────────────────────────────────

pub fn process_functions() -> Vec<&'static str> {
    vec![
        "exec",
        "exec_out",
        "exec_status",
        "exec_ok",
        "shell",
        "process_run",
        "process_spawn",
        "env_get",
        "env_set",
        "env_all",
        "cwd",
        "set_cwd",
    ]
}

// ── Dispatch ──────────────────────────────────────────────────────────────────

pub fn call_process(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "exec"          => proc_exec(args),
        "exec_out"      => proc_exec_out(args),
        "exec_status"   => proc_exec_status(args),
        "exec_ok"       => proc_exec_ok(args),
        "shell"         => proc_shell(args),
        "process_run"   => proc_process_run(args),
        "process_spawn" => proc_process_spawn(args),
        "env_get"       => proc_env_get(args),
        "env_set"       => proc_env_set(args),
        "env_all"       => proc_env_all(args),
        "cwd"           => proc_cwd(args),
        "set_cwd"       => proc_set_cwd(args),
        _               => Err(format!("Unknown process function '{}'", name)),
    }
}

// ── Simple execution ──────────────────────────────────────────────────────────

/// exec(cmd: String) -> Result<String, String>
///
/// Runs a shell command via /bin/sh -c. Supports pipes, redirects,
/// &&, ||, and all other shell features.
///
/// Returns Ok(stdout) if the exit code is 0, Err(stderr) otherwise.
/// Trailing newline is stripped from stdout (like $(...) in bash).
///
/// Example:
///   let files = exec("ls -la")
///   match files {
///       Ok(out) => println(out)
///       Err(e)  => println("Error: #{e}")
///   }
///
///   // Pipes work
///   let count = exec("ls | wc -l")
///
///   // Chain commands
///   let res = exec("git fetch && git status")
fn proc_exec(args: Vec<Value>) -> Result<Value, String> {
    let cmd = require_str(&args, 0, "exec(cmd)")?;
    let out = run_shell(&cmd)?;
    if out.code == 0 {
        Ok(ok_val(Value::Str(out.stdout)))
    } else {
        let msg = if out.stderr.is_empty() {
            format!("exited with code {}", out.code)
        } else {
            out.stderr
        };
        Ok(err_val(msg))
    }
}

/// exec_out(cmd: String) -> Map
///
/// Runs a shell command and always returns a Map regardless of exit code.
/// Useful when you need stderr, the exact exit code, or want to handle
/// failure yourself.
///
/// Returns: { stdout: String, stderr: String, code: Int, ok: Bool }
///
/// Example:
///   let res = exec_out("git log --oneline -5")
///   if res["ok"] {
///       println(res["stdout"])
///   } else {
///       println("Failed (#{res["code"]}): #{res["stderr"]}")
///   }
fn proc_exec_out(args: Vec<Value>) -> Result<Value, String> {
    let cmd = require_str(&args, 0, "exec_out(cmd)")?;
    let out = run_shell(&cmd)?;
    Ok(make_output_map(out))
}

/// exec_status(cmd: String) -> Int
///
/// Runs a shell command and returns only its exit code.
/// Never produces a Zephyr error — if the command cannot be started
/// at all, returns 127 (the conventional "command not found" code).
///
/// Example:
///   let code = exec_status("ping -c 1 google.com")
///   if code == 0 {
///       println("Network is up")
///   }
fn proc_exec_status(args: Vec<Value>) -> Result<Value, String> {
    let cmd = require_str(&args, 0, "exec_status(cmd)")?;
    let code = match run_shell(&cmd) {
        Ok(out) => out.code,
        Err(_)  => 127,
    };
    Ok(Value::Int(code as i64))
}

/// exec_ok(cmd: String) -> Bool
///
/// Returns true if the command exits with code 0, false otherwise.
/// Convenient for condition checks without match boilerplate.
///
/// Example:
///   if exec_ok("which git") {
///       println("git is installed")
///   } else {
///       println("git not found")
///   }
///
///   if exec_ok("test -f /etc/pacman.conf") {
///       println("Running on Arch")
///   }
fn proc_exec_ok(args: Vec<Value>) -> Result<Value, String> {
    let cmd = require_str(&args, 0, "exec_ok(cmd)")?;
    let ok = match run_shell(&cmd) {
        Ok(out) => out.code == 0,
        Err(_)  => false,
    };
    Ok(Value::Bool(ok))
}

// ── Explicit argument lists ───────────────────────────────────────────────────

/// shell(program: String, args: List<String>) -> Map
///
/// Runs a program directly with an explicit argument list.
/// No /bin/sh is involved — args are passed exactly as given.
/// This is the safe way to run commands with user-provided input
/// since there is no shell injection risk.
///
/// Returns: { stdout: String, stderr: String, code: Int, ok: Bool }
///
/// Example:
///   let res = shell("git", ["clone", "--depth", "1", url, dest])
///   if res["ok"] {
///       println("Cloned successfully")
///   } else {
///       println(res["stderr"])
///   }
///
///   // vs exec() which is vulnerable if url contains shell metacharacters:
///   // exec("git clone #{url}")  <-- dangerous with untrusted input!
///   // shell("git", ["clone", url])  <-- always safe
fn proc_shell(args: Vec<Value>) -> Result<Value, String> {
    let program  = require_str(&args, 0, "shell(program, args)")?;
    let arg_list = extract_string_list(&args, 1)?;
    let out = run_command(&program, &arg_list, &[], None)?;
    Ok(make_output_map(out))
}

// ── Full control ──────────────────────────────────────────────────────────────

/// process_run(program, args, env, cwd) -> Map
///
/// Maximum-control process execution. All parameters after program
/// can be nil/empty to use defaults.
///
///   program  String           — executable name or full path
///   args     List<String>     — argument list (can be nil or [])
///   env      Map<String,Str>  — extra environment variables to set
///                               (merged with current env, can be nil)
///   cwd      String           — working directory (can be nil or "")
///
/// Returns: { stdout: String, stderr: String, code: Int, ok: Bool }
///
/// Example:
///   var extra_env = {}
///   extra_env["PKGDEST"] = "/tmp/packages"
///   extra_env["MAKEFLAGS"] = "-j8"
///
///   let res = process_run(
///       "makepkg",
///       ["--noconfirm", "--needed"],
///       extra_env,
///       "/tmp/mypkg/src"
///   )
///   println("makepkg exited #{res["code"]}")
///   if !res["ok"] {
///       println(res["stderr"])
///   }
fn proc_process_run(args: Vec<Value>) -> Result<Value, String> {
    let program  = require_str(&args, 0, "process_run(program, args, env, cwd)")?;
    let arg_list = extract_string_list(&args, 1).unwrap_or_default();
    let env_vars = extract_string_map(&args, 2);
    let cwd      = match args.get(3) {
        Some(Value::Str(s)) if !s.is_empty() => Some(s.clone()),
        _ => None,
    };
    let out = run_command(&program, &arg_list, &env_vars, cwd.as_deref())?;
    Ok(make_output_map(out))
}

// ── Streaming ─────────────────────────────────────────────────────────────────

/// process_spawn(cmd: String) -> Result<Int, String>
///
/// Runs a shell command with stdout and stderr connected directly to
/// the terminal. Output streams in real time — nothing is captured.
/// Returns Ok(exit_code) when the process finishes.
///
/// Use this for interactive or long-running commands like:
///   makepkg, pacman, git clone (large repos), cargo build, etc.
///
/// Example:
///   let code = process_spawn("makepkg -si --noconfirm")
///   match code {
///       Ok(0) => println("Build succeeded")
///       Ok(n) => println("Build failed with code #{n}")
///       Err(e) => println("Could not start: #{e}")
///   }
fn proc_process_spawn(args: Vec<Value>) -> Result<Value, String> {
    let cmd = require_str(&args, 0, "process_spawn(cmd)")?;
    let status = Command::new("/bin/sh")
        .arg("-c")
        .arg(&cmd)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| format!("process_spawn: failed to start: {}", e))?;
    let code = status.code().unwrap_or(-1);
    Ok(ok_val(Value::Int(code as i64)))
}

// ── Environment ───────────────────────────────────────────────────────────────

/// env_get(key: String) -> String | Nil
///
/// Returns the value of an environment variable, or nil if not set.
///
/// Example:
///   let home = env_get("HOME")
///   let editor = env_get("EDITOR")
///   if editor == nil {
///       println("EDITOR not set, defaulting to vim")
///   }
fn proc_env_get(args: Vec<Value>) -> Result<Value, String> {
    let key = require_str(&args, 0, "env_get(key)")?;
    match env::var(&key) {
        Ok(val) => Ok(Value::Str(val)),
        Err(_)  => Ok(Value::Nil),
    }
}

/// env_set(key: String, val: String) -> Nil
///
/// Sets an environment variable for this process and any child
/// processes spawned after this call.
///
/// Example:
///   env_set("EDITOR", "nvim")
///   env_set("PAGER", "less")
fn proc_env_set(args: Vec<Value>) -> Result<Value, String> {
    let key = require_str(&args, 0, "env_set(key, val)")?;
    let val = require_str(&args, 1, "env_set(key, val)")?;
    env::set_var(&key, &val);
    Ok(Value::Nil)
}

/// env_all() -> Map<String, String>
///
/// Returns all current environment variables as a Map.
///
/// Example:
///   let env = env_all()
///   let path = env["PATH"]
fn proc_env_all(_args: Vec<Value>) -> Result<Value, String> {
    let mut map = HashMap::new();
    for (key, val) in env::vars() {
        map.insert(key, Value::Str(val));
    }
    Ok(Value::Map(Rc::new(RefCell::new(map))))
}

// ── Working directory ─────────────────────────────────────────────────────────

/// cwd() -> String
///
/// Returns the current working directory as an absolute path string.
///
/// Example:
///   let dir = cwd()
///   println("Working in: #{dir}")
fn proc_cwd(_args: Vec<Value>) -> Result<Value, String> {
    env::current_dir()
        .map(|p| Value::Str(p.to_string_lossy().to_string()))
        .map_err(|e| format!("cwd(): {}", e))
}

/// set_cwd(path: String) -> Result<Nil, String>
///
/// Changes the current working directory.
///
/// Example:
///   let res = set_cwd("/tmp/build")
///   match res {
///       Ok(_)  => println("Changed directory")
///       Err(e) => println("Failed: #{e}")
///   }
fn proc_set_cwd(args: Vec<Value>) -> Result<Value, String> {
    let path = require_str(&args, 0, "set_cwd(path)")?;
    match env::set_current_dir(&path) {
        Ok(_)  => Ok(ok_val(Value::Nil)),
        Err(e) => Ok(err_val(format!("set_cwd: {}", e))),
    }
}

// ═══════════════════════════════════════════════════════════
// Internal implementation
// ═══════════════════════════════════════════════════════════

struct ProcessOutput {
    stdout: String,
    stderr: String,
    code:   i32,
}

/// Run via /bin/sh -c — supports all shell features.
fn run_shell(cmd: &str) -> Result<ProcessOutput, String> {
    run_command("/bin/sh", &["-c".to_string(), cmd.to_string()], &[], None)
}

/// Run a program directly with an explicit arg list.
fn run_command(
    program:  &str,
    args:     &[String],
    env_vars: &[(String, String)],
    cwd:      Option<&str>,
) -> Result<ProcessOutput, String> {
    let mut cmd = Command::new(program);
    cmd.args(args);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    for (k, v) in env_vars {
        cmd.env(k, v);
    }
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }

    let output = cmd.output()
        .map_err(|e| format!("Failed to run '{}': {}", program, e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    // Strip single trailing newline — matches shell $(...) behaviour
    let stdout = stdout.strip_suffix('\n')
        .unwrap_or(&stdout)
        .strip_suffix('\r')
        .unwrap_or(&stdout.strip_suffix('\n').unwrap_or(&stdout))
        .to_string();
    let stderr = stderr.strip_suffix('\n')
        .unwrap_or(&stderr)
        .strip_suffix('\r')
        .unwrap_or(&stderr.strip_suffix('\n').unwrap_or(&stderr))
        .to_string();

    let code = output.status.code().unwrap_or(-1);
    Ok(ProcessOutput { stdout, stderr, code })
}

/// Build the standard { stdout, stderr, code, ok } Map.
fn make_output_map(out: ProcessOutput) -> Value {
    let mut map = HashMap::new();
    map.insert("stdout".into(), Value::Str(out.stdout));
    map.insert("stderr".into(), Value::Str(out.stderr));
    map.insert("code".into(),   Value::Int(out.code as i64));
    map.insert("ok".into(),     Value::Bool(out.code == 0));
    Value::Map(Rc::new(RefCell::new(map)))
}

// ── Argument helpers ──────────────────────────────────────────────────────────

fn require_str(args: &[Value], idx: usize, sig: &str) -> Result<String, String> {
    match args.get(idx) {
        Some(Value::Str(s)) => Ok(s.clone()),
        Some(other) => Ok(format!("{}", other)), // coerce to string
        None => Err(format!("{}: argument {} is required", sig, idx + 1)),
    }
}

/// Extract a List of strings from args[idx]. Returns empty vec for nil/missing.
fn extract_string_list(args: &[Value], idx: usize) -> Result<Vec<String>, String> {
    match args.get(idx) {
        Some(Value::List(v)) => {
            v.borrow().iter()
                .map(|item| match item {
                    Value::Str(s) => Ok(s.clone()),
                    other         => Ok(format!("{}", other)), // coerce
                })
                .collect()
        }
        Some(Value::Nil) | None => Ok(vec![]),
        Some(other) => Err(format!("Expected List for argument {}, got {}", idx + 1,
            crate::interpreter::value_type_name(other))),
    }
}

/// Extract a Map<String, String> from args[idx] as env var pairs.
fn extract_string_map(args: &[Value], idx: usize) -> Vec<(String, String)> {
    match args.get(idx) {
        Some(Value::Map(m)) => {
            m.borrow().iter()
                .map(|(k, v)| (k.clone(), format!("{}", v)))
                .collect()
        }
        _ => vec![],
    }
}

fn ok_val(v: Value) -> Value {
    Value::Result(std::result::Result::Ok(Box::new(v)))
}

fn err_val(msg: String) -> Value {
    Value::Result(std::result::Result::Err(Box::new(Value::Str(msg))))
}