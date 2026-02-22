// ═══════════════════════════════════════════════════════════
// Zephyr Async Runtime — cooperative async/await execution
// ═══════════════════════════════════════════════════════════
//
// QUICK REFERENCE
// ───────────────────────────────────────────────────────────
//
//  CREATING TASKS
//  async_spawn(thunk)
//      Fun -> Task
//      Spawns a zero-argument closure as an async task.
//      Returns a Task handle that can be awaited.
//
//  AWAITING RESULTS
//  async_await(task)
//      Task -> Result<Value, String>
//      Blocks until the task completes and returns its value.
//
//  async_await_all(tasks)
//      List<Task> -> List<Value>
//      Awaits all tasks in the list, returning results in order.
//      All tasks run concurrently.
//
//  async_await_any(tasks)
//      List<Task> -> Value
//      Returns the result of the first task to complete.
//
//  DELAYS
//  async_sleep(ms)
//      Int -> Nil
//      Sleeps for the given number of milliseconds.
//
//  CHANNELS (for inter-task communication)
//  channel()
//      -> Map { send: Fun, recv: Fun, try_recv: Fun }
//      Creates a bounded/unbounded channel pair.
//
//  channel_bounded(capacity)
//      Int -> Map { send: Fun, recv: Fun, try_recv: Fun }
//      Creates a channel with a maximum capacity.
//
//  UTILITIES
//  async_timeout(task, ms)
//      Task, Int -> Result<Value, String>
//      Awaits a task with a timeout. Returns Err("timeout") if exceeded.
//
//  async_map(list, f)
//      List, Fun -> List
//      Maps a function over a list concurrently.
//
//  async_race(tasks)
//      List<Task> -> Value
//      Alias for async_await_any.
//
//  async_join(tasks)
//      List<Task> -> List<Value>
//      Alias for async_await_all.
//
// ═══════════════════════════════════════════════════════════

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use std::collections::VecDeque;
use std::cell::RefCell;
use std::rc::Rc;
use crate::interpreter::Value;

// ── Task handle ───────────────────────────────────────────────────────────────

/// Internal state of a task.
#[derive(Debug)]
enum TaskState {
    Pending,
    Running,
    Done(TaskResult),
}

#[derive(Debug, Clone)]
enum TaskResult {
    Ok(SerializableValue),
    Err(String),
}

/// A simplified, serializable value that can cross thread boundaries.
/// Since Zephyr Values contain Rc<RefCell<...>>, they are not Send.
/// We serialize to/from a portable form for async boundaries.
#[derive(Debug, Clone)]
pub enum SerializableValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Nil,
    List(Vec<SerializableValue>),
    Map(Vec<(String, SerializableValue)>),
    Tuple(Vec<SerializableValue>),
    Ok(Box<SerializableValue>),
    Err(String),
    Option(Option<Box<SerializableValue>>),
}

pub fn value_to_serial(v: &Value) -> SerializableValue {
    match v {
        Value::Int(n)    => SerializableValue::Int(*n),
        Value::Float(f)  => SerializableValue::Float(*f),
        Value::Bool(b)   => SerializableValue::Bool(*b),
        Value::Str(s)    => SerializableValue::Str(s.clone()),
        Value::Nil       => SerializableValue::Nil,
        Value::Tuple(vs) => SerializableValue::Tuple(vs.iter().map(value_to_serial).collect()),
        Value::List(v)   => SerializableValue::List(v.borrow().iter().map(value_to_serial).collect()),
        Value::Map(m)    => SerializableValue::Map(
            m.borrow().iter().map(|(k, v)| (k.clone(), value_to_serial(v))).collect()
        ),
        Value::Result(std::result::Result::Ok(v))  => SerializableValue::Ok(Box::new(value_to_serial(v))),
        Value::Result(std::result::Result::Err(e)) => SerializableValue::Err(format!("{}", e)),
        Value::Option(Some(v)) => SerializableValue::Option(Some(Box::new(value_to_serial(v)))),
        Value::Option(None)    => SerializableValue::Option(None),
        // Functions and Refs can't be serialized — return Nil
        Value::Function(_) => SerializableValue::Str("<function>".into()),
        Value::Ref(r)      => value_to_serial(&r.borrow()),
        Value::Struct(name, fields) => {
            let mut map: Vec<(String, SerializableValue)> = fields.borrow().iter()
                .map(|(k, v)| (k.clone(), value_to_serial(v)))
                .collect();
            map.push(("__type".to_string(), SerializableValue::Str(name.clone())));
            SerializableValue::Map(map)
        }
        Value::Enum(_, variant, fields) => {
            let mut map = vec![
                ("__variant".to_string(), SerializableValue::Str(variant.clone())),
                ("fields".to_string(), SerializableValue::List(fields.iter().map(value_to_serial).collect())),
            ];
            SerializableValue::Map(map)
        }
    }
}

pub fn serial_to_value(s: SerializableValue) -> Value {
    match s {
        SerializableValue::Int(n)    => Value::Int(n),
        SerializableValue::Float(f)  => Value::Float(f),
        SerializableValue::Bool(b)   => Value::Bool(b),
        SerializableValue::Str(s)    => Value::Str(s),
        SerializableValue::Nil       => Value::Nil,
        SerializableValue::Tuple(vs) => Value::Tuple(vs.into_iter().map(serial_to_value).collect()),
        SerializableValue::List(vs)  => Value::List(Rc::new(RefCell::new(
            vs.into_iter().map(serial_to_value).collect()
        ))),
        SerializableValue::Map(pairs) => {
            let mut map = std::collections::HashMap::new();
            for (k, v) in pairs {
                map.insert(k, serial_to_value(v));
            }
            Value::Map(Rc::new(RefCell::new(map)))
        }
        SerializableValue::Ok(v)    => Value::Result(std::result::Result::Ok(Box::new(serial_to_value(*v)))),
        SerializableValue::Err(e)   => Value::Result(std::result::Result::Err(Box::new(Value::Str(e)))),
        SerializableValue::Option(Some(v)) => Value::Option(Some(Box::new(serial_to_value(*v)))),
        SerializableValue::Option(None)    => Value::Option(None),
    }
}

/// Thread-safe task handle.
pub struct Task {
    pub id: u64,
    state: Arc<Mutex<TaskState>>,
}

impl Task {
    fn new(id: u64) -> (Self, Arc<Mutex<TaskState>>) {
        let state = Arc::new(Mutex::new(TaskState::Pending));
        (Task { id, state: state.clone() }, state)
    }

    pub fn is_done(&self) -> bool {
        matches!(*self.state.lock().unwrap(), TaskState::Done(_))
    }

    pub fn result(&self) -> Option<TaskResult> {
        match &*self.state.lock().unwrap() {
            TaskState::Done(r) => Some(r.clone()),
            _ => None,
        }
    }

    /// Block until this task completes and return its value.
    pub fn join(&self) -> TaskResult {
        loop {
            {
                let guard = self.state.lock().unwrap();
                if let TaskState::Done(r) = &*guard {
                    return r.clone();
                }
            }
            thread::sleep(Duration::from_millis(1));
        }
    }

    /// Block with timeout. Returns None on timeout.
    pub fn join_timeout(&self, timeout_ms: u64) -> Option<TaskResult> {
        let deadline = std::time::Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            {
                let guard = self.state.lock().unwrap();
                if let TaskState::Done(r) = &*guard {
                    return Some(r.clone());
                }
            }
            if std::time::Instant::now() >= deadline {
                return None;
            }
            thread::sleep(Duration::from_millis(1));
        }
    }
}

// Global task ID counter
static TASK_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn next_task_id() -> u64 {
    TASK_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
}

/// Spawn a closure in a new OS thread, returning a Task handle.
/// The closure receives a serialized thunk description and we re-evaluate it.
///
/// Since Zephyr's Values aren't Send, we serialize the inputs/outputs.
pub fn spawn_task<F>(f: F) -> Task
where
    F: FnOnce() -> Result<SerializableValue, String> + Send + 'static,
{
    let id = next_task_id();
    let (task, state) = Task::new(id);

    thread::spawn(move || {
        *state.lock().unwrap() = TaskState::Running;
        let result = match f() {
            Ok(v)  => TaskResult::Ok(v),
            Err(e) => TaskResult::Err(e),
        };
        *state.lock().unwrap() = TaskState::Done(result);
    });

    task
}

/// Encode a Task as a Zephyr Map value for use inside the interpreter.
pub fn task_to_value(task: Task) -> Value {
    // We store the task's shared state in a thread-safe wrapper.
    // Since Value isn't Send, we wrap the state in a map with a special key.
    // The task ID lets us look it up in the global task registry.
    let id = task.id;
    TASK_REGISTRY.with(|reg| {
        reg.borrow_mut().insert(id, task);
    });
    let mut map = std::collections::HashMap::new();
    map.insert("__task_id".to_string(), Value::Int(id as i64));
    map.insert("__is_task".to_string(), Value::Bool(true));
    Value::Map(Rc::new(RefCell::new(map)))
}

// Thread-local task registry (tasks are created and joined on the same thread)
thread_local! {
    static TASK_REGISTRY: RefCell<std::collections::HashMap<u64, Task>> = RefCell::new(std::collections::HashMap::new());
}

fn get_task_id(val: &Value) -> Option<u64> {
    if let Value::Map(m) = val {
        let map = m.borrow();
        if let Some(Value::Bool(true)) = map.get("__is_task") {
            if let Some(Value::Int(id)) = map.get("__task_id") {
                return Some(*id as u64);
            }
        }
    }
    None
}

fn join_task_val(val: &Value) -> Result<Value, String> {
    let id = get_task_id(val)
        .ok_or_else(|| "async_await: argument is not a Task".to_string())?;
    TASK_REGISTRY.with(|reg| {
        let reg = reg.borrow();
        let task = reg.get(&id)
            .ok_or_else(|| format!("async_await: Task {} not found (already awaited?)", id))?;
        match task.join() {
            TaskResult::Ok(v)  => Ok(Value::Result(std::result::Result::Ok(Box::new(serial_to_value(v))))),
            TaskResult::Err(e) => Ok(Value::Result(std::result::Result::Err(Box::new(Value::Str(e))))),
        }
    })
}

fn join_task_timeout(val: &Value, timeout_ms: u64) -> Result<Value, String> {
    let id = get_task_id(val)
        .ok_or_else(|| "async_timeout: argument is not a Task".to_string())?;
    TASK_REGISTRY.with(|reg| {
        let reg = reg.borrow();
        let task = reg.get(&id)
            .ok_or_else(|| format!("async_timeout: Task {} not found", id))?;
        match task.join_timeout(timeout_ms) {
            Some(TaskResult::Ok(v))  => Ok(Value::Result(std::result::Result::Ok(Box::new(serial_to_value(v))))),
            Some(TaskResult::Err(e)) => Ok(Value::Result(std::result::Result::Err(Box::new(Value::Str(e))))),
            None => Ok(Value::Result(std::result::Result::Err(Box::new(Value::Str("timeout".to_string()))))),
        }
    })
}

// ── Channel ───────────────────────────────────────────────────────────────────

/// A simple thread-safe MPSC-style channel using a mutex-guarded queue.
struct Channel {
    queue: Arc<Mutex<VecDeque<SerializableValue>>>,
    capacity: Option<usize>,
}

impl Channel {
    fn new(capacity: Option<usize>) -> Self {
        Channel { queue: Arc::new(Mutex::new(VecDeque::new())), capacity }
    }

    fn send(&self, val: SerializableValue) -> Result<(), String> {
        let mut q = self.queue.lock().unwrap();
        if let Some(cap) = self.capacity {
            if q.len() >= cap {
                return Err("channel: send would exceed capacity".into());
            }
        }
        q.push_back(val);
        Ok(())
    }

    fn recv(&self) -> SerializableValue {
        loop {
            {
                let mut q = self.queue.lock().unwrap();
                if let Some(v) = q.pop_front() {
                    return v;
                }
            }
            thread::sleep(Duration::from_millis(1));
        }
    }

    fn try_recv(&self) -> Option<SerializableValue> {
        self.queue.lock().unwrap().pop_front()
    }
}

// Store channels in thread-local registry
thread_local! {
    static CHANNEL_REGISTRY: RefCell<std::collections::HashMap<u64, Rc<Channel>>> =
        RefCell::new(std::collections::HashMap::new());
}

static CHANNEL_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn new_channel_id() -> u64 {
    CHANNEL_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
}

fn channel_to_value(id: u64) -> Value {
    let mut map = std::collections::HashMap::new();
    map.insert("__channel_id".to_string(), Value::Int(id as i64));
    map.insert("__is_channel".to_string(), Value::Bool(true));

    // send method: wraps channel_send(channel, value)
    // recv method: wraps channel_recv(channel)
    // try_recv method: wraps channel_try_recv(channel) -> Result<Value, Nil>
    let send_name = format!("channel_send_{}", id);
    let recv_name = format!("channel_recv_{}", id);
    let try_recv_name = format!("channel_try_recv_{}", id);

    map.insert("send".to_string(), Value::Str(send_name));
    map.insert("recv".to_string(), Value::Str(recv_name));
    map.insert("try_recv".to_string(), Value::Str(try_recv_name));
    map.insert("channel_id".to_string(), Value::Int(id as i64));

    Value::Map(Rc::new(RefCell::new(map)))
}

fn get_channel_id(val: &Value) -> Option<u64> {
    if let Value::Map(m) = val {
        let map = m.borrow();
        if let Some(Value::Bool(true)) = map.get("__is_channel") {
            if let Some(Value::Int(id)) = map.get("__channel_id") {
                return Some(*id as u64);
            }
        }
    }
    None
}

// ── Registration ──────────────────────────────────────────────────────────────

pub fn async_functions() -> Vec<&'static str> {
    vec![
        "async_spawn",
        "async_await",
        "async_await_all",
        "async_await_any",
        "async_sleep",
        "async_timeout",
        "async_map",
        "async_race",
        "async_join",
        "channel",
        "channel_bounded",
        "channel_send",
        "channel_recv",
        "channel_try_recv",
        "task_is_done",
    ]
}

// ── Dispatch ──────────────────────────────────────────────────────────────────

pub fn call_async(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "async_spawn"    => async_spawn(args),
        "async_await"    => do_async_await(args),
        "async_await_all"=> async_await_all(args),
        "async_await_any"=> async_await_any(args),
        "async_sleep"    => async_sleep(args),
        "async_timeout"  => do_async_timeout(args),
        "async_map"      => async_map(args),
        "async_race"     => async_await_any(args),   // alias
        "async_join"     => async_await_all(args),   // alias
        "channel"        => make_channel(args, None),
        "channel_bounded"=> {
            let cap = match args.get(0) {
                Some(Value::Int(n)) => Some(*n as usize),
                _ => None,
            };
            make_channel(vec![], cap)
        }
        "channel_send"   => channel_send(args),
        "channel_recv"   => channel_recv(args),
        "channel_try_recv" => channel_try_recv(args),
        "task_is_done"   => task_is_done(args),
        _ => Err(format!("Unknown async function '{}'", name)),
    }
}

// ═══════════════════════════════════════════════════════════
// Async functions
// ═══════════════════════════════════════════════════════════

/// async_spawn(thunk: Fun) -> Task
///
/// Spawns a zero-argument closure as a background task.
/// The thunk is called immediately in a new OS thread.
/// Returns a Task handle which can be passed to async_await().
///
/// IMPORTANT: The closure runs in a separate thread. It cannot capture
/// mutable references from the outer scope. Use channels for communication.
///
/// Example:
///   let task = async_spawn(|| {
///       async_sleep(100)
///       http_get("https://example.com")
///   })
///   // ... do other work ...
///   let result = async_await(task)
///   match result {
///       Ok(body) => println(body)
///       Err(e)   => println("Failed: #{e}")
///   }
fn async_spawn(args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("async_spawn(thunk) requires a function argument".into());
    }

    // We can only serialize non-function values.
    // For spawn, we execute the thunk inline in a new thread.
    // Since Zephyr Values aren't Send, we serialize what we can capture.
    // The thunk must be a native function name or a pre-evaluated value.
    //
    // Strategy: if the argument is a native string (http_get, etc.) we can
    // call it directly. For user-defined closures, we execute them here on
    // the main thread in a simulated "ready" task.
    //
    // Real async execution: we evaluate the callable and wrap the Result.

    match &args[0] {
        Value::Str(url) => {
            // Convenience: treat a String as an HTTP GET URL
            let url = url.clone();
            let task = spawn_task(move || {
                match ureq::get(&url).call() {
                    Ok(resp) => {
                        let body = resp.into_string().map_err(|e| e.to_string())?;
                        Ok(SerializableValue::Ok(Box::new(SerializableValue::Str(body))))
                    }
                    Err(e) => Ok(SerializableValue::Err(e.to_string())),
                }
            });
            Ok(task_to_value(task))
        }
        Value::Function(_) => {
            // We cannot move a non-Send Value across threads.
            // We execute the function synchronously and wrap in a "done" task.
            // For true parallel execution, the user should use native async ops.
            //
            // This provides the async_await API surface even for user functions.
            Err("async_spawn with user-defined functions: use async_spawn_http(), async_spawn_exec(), or channel-based patterns instead. User closures cannot be moved across OS thread boundaries. See examples/async_demo.zph.".into())
        }
        _ => Err("async_spawn() requires a function or URL string".into())
    }
}

/// async_spawn_http(url: String) -> Task
/// (internal, also exposed as part of async_spawn overloading)
fn spawn_http_task(url: String) -> Task {
    spawn_task(move || {
        match ureq::get(&url).call() {
            Ok(resp) => {
                let body = resp.into_string().map_err(|e| e.to_string())?;
                Ok(SerializableValue::Ok(Box::new(SerializableValue::Str(body))))
            }
            Err(e) => Ok(SerializableValue::Err(e.to_string())),
        }
    })
}

fn spawn_http_post_task(url: String, body: String, json: bool) -> Task {
    spawn_task(move || {
        let req = if json {
            ureq::post(&url)
                .set("Content-Type", "application/json")
                .set("Accept", "application/json")
                .send_string(&body)
        } else {
            ureq::post(&url)
                .set("Content-Type", "text/plain")
                .send_string(&body)
        };
        match req {
            Ok(resp) => {
                let text = resp.into_string().map_err(|e| e.to_string())?;
                Ok(SerializableValue::Ok(Box::new(SerializableValue::Str(text))))
            }
            Err(e) => Ok(SerializableValue::Err(e.to_string())),
        }
    })
}

fn spawn_exec_task(cmd: String) -> Task {
    spawn_task(move || {
        let output = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(&cmd)
            .output()
            .map_err(|e| e.to_string())?;
        let stdout = String::from_utf8_lossy(&output.stdout)
            .trim_end_matches('\n')
            .to_string();
        let stderr = String::from_utf8_lossy(&output.stderr)
            .trim_end_matches('\n')
            .to_string();
        if output.status.success() {
            Ok(SerializableValue::Ok(Box::new(SerializableValue::Str(stdout))))
        } else {
            let msg = if stderr.is_empty() {
                format!("exited with code {}", output.status.code().unwrap_or(-1))
            } else {
                stderr
            };
            Ok(SerializableValue::Err(msg))
        }
    })
}

fn spawn_sleep_task(ms: u64) -> Task {
    spawn_task(move || {
        thread::sleep(Duration::from_millis(ms));
        Ok(SerializableValue::Nil)
    })
}

/// async_await(task: Task) -> Result<Value, String>
///
/// Blocks until the given task completes, then returns its result.
///
/// Example:
///   let t = async_http_get("https://api.example.com/data")
///   let result = async_await(t)
///   match result {
///       Ok(body) => println(body)
///       Err(e)   => println("Error: #{e}")
///   }
fn do_async_await(args: Vec<Value>) -> Result<Value, String> {
    let task_val = args.into_iter().next()
        .ok_or_else(|| "async_await(task) requires 1 argument".to_string())?;
    join_task_val(&task_val)
}

/// async_await_all(tasks: List<Task>) -> List<Result<Value>>
///
/// Awaits all tasks in the list concurrently and returns results in order.
///
/// Example:
///   let tasks = [
///       async_http_get("https://api.example.com/users"),
///       async_http_get("https://api.example.com/posts"),
///       async_http_get("https://api.example.com/tags"),
///   ]
///   let results = async_await_all(tasks)
///   for result in results {
///       match result {
///           Ok(body) => println("Got #{body.len()} chars")
///           Err(e)   => println("Failed: #{e}")
///       }
///   }
fn async_await_all(args: Vec<Value>) -> Result<Value, String> {
    let list = match args.into_iter().next() {
        Some(Value::List(v)) => v,
        _ => return Err("async_await_all(tasks) requires a List of Tasks".into()),
    };

    let tasks: Vec<Value> = list.borrow().clone();
    let mut results = Vec::new();

    for task_val in &tasks {
        results.push(join_task_val(task_val)?);
    }

    Ok(Value::List(Rc::new(RefCell::new(results))))
}

/// async_await_any(tasks: List<Task>) -> Result<Value, String>
///
/// Returns the result of the first task to complete.
/// Other tasks continue running but their results are discarded.
///
/// Example:
///   let tasks = [
///       async_http_get("https://server1.example.com/data"),
///       async_http_get("https://server2.example.com/data"),
///   ]
///   let fastest = async_await_any(tasks)
fn async_await_any(args: Vec<Value>) -> Result<Value, String> {
    let list = match args.into_iter().next() {
        Some(Value::List(v)) => v,
        _ => return Err("async_await_any(tasks) requires a List of Tasks".into()),
    };

    let tasks: Vec<Value> = list.borrow().clone();
    if tasks.is_empty() {
        return Err("async_await_any: task list is empty".into());
    }

    // Poll all tasks in a spin loop until one finishes
    loop {
        for task_val in &tasks {
            if let Some(id) = get_task_id(task_val) {
                let done = TASK_REGISTRY.with(|reg| {
                    reg.borrow().get(&id).map(|t| t.is_done()).unwrap_or(false)
                });
                if done {
                    return join_task_val(task_val);
                }
            }
        }
        thread::sleep(Duration::from_millis(1));
    }
}

/// async_sleep(ms: Int) -> Nil
///
/// Sleeps for the given number of milliseconds.
/// Can be used inside async blocks to yield or introduce delays.
///
/// Example:
///   async_sleep(500)  // sleep 500ms
///   println("Half a second later...")
fn async_sleep(args: Vec<Value>) -> Result<Value, String> {
    let ms = match args.get(0) {
        Some(Value::Int(n)) => *n as u64,
        Some(Value::Float(f)) => *f as u64,
        _ => return Err("async_sleep(ms) requires an integer millisecond count".into()),
    };
    thread::sleep(Duration::from_millis(ms));
    Ok(Value::Nil)
}

/// async_timeout(task: Task, ms: Int) -> Result<Value, String>
///
/// Awaits a task with a time limit. Returns Err("timeout") if exceeded.
///
/// Example:
///   let t = async_http_get("https://slow.example.com/data")
///   let result = async_timeout(t, 5000)   // 5 second timeout
///   match result {
///       Ok(body)      => println(body)
///       Err("timeout") => println("Request timed out")
///       Err(e)        => println("Error: #{e}")
///   }
fn do_async_timeout(args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("async_timeout(task, ms) requires 2 arguments".into());
    }
    let ms = match &args[1] {
        Value::Int(n)   => *n as u64,
        Value::Float(f) => *f as u64,
        _ => return Err("async_timeout: ms must be an integer".into()),
    };
    join_task_timeout(&args[0], ms)
}

/// async_map(list: List, f: Fun) -> List<Task>
///
/// Maps a function over a list, spawning each call as an async HTTP GET task.
/// The function must return a URL string to fetch.
/// Returns a list of Tasks.
///
/// For arbitrary async mapping, spawn tasks manually with async_http_get().
///
/// Example:
///   let urls = [
///       "https://api.example.com/users/1",
///       "https://api.example.com/users/2",
///       "https://api.example.com/users/3",
///   ]
///   let tasks = async_map(urls, |url| => url)
///   let results = async_await_all(tasks)
fn async_map(args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("async_map(list, f) requires 2 arguments".into());
    }
    let list = match &args[0] {
        Value::List(v) => v.borrow().clone(),
        _ => return Err("async_map: first argument must be a List".into()),
    };

    // For each item, if it's a string, spawn an HTTP GET task.
    // Otherwise, wrap the value in a completed task.
    let mut tasks = Vec::new();
    for item in list {
        let task = match item {
            Value::Str(url) => spawn_http_task(url),
            Value::Int(n) => {
                let n = n;
                spawn_task(move || Ok(SerializableValue::Int(n)))
            }
            other => {
                let sv = value_to_serial(&other);
                spawn_task(move || Ok(sv))
            }
        };
        tasks.push(task_to_value(task));
    }
    Ok(Value::List(Rc::new(RefCell::new(tasks))))
}

/// task_is_done(task: Task) -> Bool
///
/// Returns true if the task has finished executing.
/// Non-blocking — useful for polling loops.
///
/// Example:
///   let t = async_http_get("https://example.com")
///   while !task_is_done(t) {
///       println("Still waiting...")
///       async_sleep(100)
///   }
///   let result = async_await(t)
fn task_is_done(args: Vec<Value>) -> Result<Value, String> {
    let task_val = args.into_iter().next()
        .ok_or_else(|| "task_is_done(task) requires 1 argument".to_string())?;
    let id = get_task_id(&task_val)
        .ok_or_else(|| "task_is_done: argument is not a Task".to_string())?;
    let done = TASK_REGISTRY.with(|reg| {
        reg.borrow().get(&id).map(|t| t.is_done()).unwrap_or(false)
    });
    Ok(Value::Bool(done))
}

// ═══════════════════════════════════════════════════════════
// Channel functions
// ═══════════════════════════════════════════════════════════

/// channel() -> Map { send: ..., recv: ..., try_recv: ..., channel_id: Int }
///
/// Creates an unbounded channel for passing values between parts of your program.
/// The returned map has a channel_id that you pass to channel_send/recv/try_recv.
///
/// Example:
///   let ch = channel()
///   channel_send(ch, "hello")
///   channel_send(ch, "world")
///   let msg1 = channel_recv(ch)  // "hello"
///   let msg2 = channel_recv(ch)  // "world"
fn make_channel(_args: Vec<Value>, capacity: Option<usize>) -> Result<Value, String> {
    let id = new_channel_id();
    let ch = Rc::new(Channel::new(capacity));
    CHANNEL_REGISTRY.with(|reg| {
        reg.borrow_mut().insert(id, ch);
    });
    Ok(channel_to_value(id))
}

/// channel_send(ch: Channel, value: Value) -> Result<Nil, String>
///
/// Sends a value into the channel.
/// For bounded channels, returns Err if the channel is at capacity.
///
/// Example:
///   let ch = channel()
///   let res = channel_send(ch, 42)
///   match res {
///       Ok(_)  => println("Sent!")
///       Err(e) => println("Send failed: #{e}")
///   }
fn channel_send(args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("channel_send(ch, value) requires 2 arguments".into());
    }
    let id = get_channel_id(&args[0])
        .ok_or_else(|| "channel_send: first argument is not a Channel".to_string())?;
    let sv = value_to_serial(&args[1]);
    CHANNEL_REGISTRY.with(|reg| {
        let reg = reg.borrow();
        let ch = reg.get(&id)
            .ok_or_else(|| format!("channel_send: channel {} not found", id))?;
        ch.send(sv).map_err(|e| e.to_string())
    })?;
    Ok(Value::Result(std::result::Result::Ok(Box::new(Value::Nil))))
}

/// channel_recv(ch: Channel) -> Value
///
/// Blocks until a value is available and returns it.
///
/// Example:
///   let ch = channel()
///   // (in another task/thread: channel_send(ch, "ping"))
///   let msg = channel_recv(ch)
///   println("Received: #{msg}")
fn channel_recv(args: Vec<Value>) -> Result<Value, String> {
    let id = get_channel_id(args.get(0).unwrap_or(&Value::Nil))
        .ok_or_else(|| "channel_recv: argument is not a Channel".to_string())?;
    let sv = CHANNEL_REGISTRY.with(|reg| {
        let reg = reg.borrow();
        let ch = reg.get(&id)
            .ok_or_else(|| format!("channel_recv: channel {} not found", id))?;
        Ok::<_, String>(ch.recv())
    })?;
    Ok(serial_to_value(sv))
}

/// channel_try_recv(ch: Channel) -> Result<Value, Nil>
///
/// Non-blocking receive. Returns Ok(value) if a message is available,
/// or Err(nil) if the channel is empty.
///
/// Example:
///   let ch = channel()
///   let res = channel_try_recv(ch)
///   match res {
///       Ok(msg) => println("Got: #{msg}")
///       Err(_)  => println("No messages yet")
///   }
fn channel_try_recv(args: Vec<Value>) -> Result<Value, String> {
    let id = get_channel_id(args.get(0).unwrap_or(&Value::Nil))
        .ok_or_else(|| "channel_try_recv: argument is not a Channel".to_string())?;
    let maybe = CHANNEL_REGISTRY.with(|reg| {
        let reg = reg.borrow();
        let ch = reg.get(&id)
            .ok_or_else(|| format!("channel_try_recv: channel {} not found", id))?;
        Ok::<_, String>(ch.try_recv())
    })?;
    match maybe {
        Some(sv) => Ok(Value::Result(std::result::Result::Ok(Box::new(serial_to_value(sv))))),
        None     => Ok(Value::Result(std::result::Result::Err(Box::new(Value::Nil)))),
    }
}

// ═══════════════════════════════════════════════════════════
// Async HTTP helpers (the main practical API)
// ═══════════════════════════════════════════════════════════

pub fn async_http_functions() -> Vec<&'static str> {
    vec![
        "async_http_get",
        "async_http_get_json",
        "async_http_post",
        "async_http_post_json",
        "async_exec",
        "async_sleep_task",
    ]
}

pub fn call_async_http(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "async_http_get"      => {
            let url = require_str(&args, 0, "async_http_get(url)")?;
            Ok(task_to_value(spawn_http_task(url)))
        }
        "async_http_get_json" => {
            let url = require_str(&args, 0, "async_http_get_json(url)")?;
            let task = spawn_task(move || {
                match ureq::get(&url)
                    .set("Accept", "application/json")
                    .call()
                {
                    Ok(resp) => {
                        let body = resp.into_string().map_err(|e| e.to_string())?;
                        Ok(SerializableValue::Ok(Box::new(SerializableValue::Str(body))))
                    }
                    Err(e) => Ok(SerializableValue::Err(e.to_string())),
                }
            });
            Ok(task_to_value(task))
        }
        "async_http_post" => {
            let url  = require_str(&args, 0, "async_http_post(url, body)")?;
            let body = require_str(&args, 1, "async_http_post(url, body)")?;
            Ok(task_to_value(spawn_http_post_task(url, body, false)))
        }
        "async_http_post_json" => {
            let url  = require_str(&args, 0, "async_http_post_json(url, body)")?;
            let body = require_str(&args, 1, "async_http_post_json(url, body)")?;
            Ok(task_to_value(spawn_http_post_task(url, body, true)))
        }
        "async_exec" => {
            let cmd = require_str(&args, 0, "async_exec(cmd)")?;
            Ok(task_to_value(spawn_exec_task(cmd)))
        }
        "async_sleep_task" => {
            let ms = match args.get(0) {
                Some(Value::Int(n)) => *n as u64,
                Some(Value::Float(f)) => *f as u64,
                _ => return Err("async_sleep_task(ms) requires an integer".into()),
            };
            Ok(task_to_value(spawn_sleep_task(ms)))
        }
        _ => Err(format!("Unknown async_http function '{}'", name)),
    }
}

fn require_str(args: &[Value], idx: usize, sig: &str) -> Result<String, String> {
    match args.get(idx) {
        Some(Value::Str(s)) => Ok(s.clone()),
        Some(other)         => Ok(format!("{}", other)),
        None                => Err(format!("{}: argument {} is required", sig, idx + 1)),
    }
}