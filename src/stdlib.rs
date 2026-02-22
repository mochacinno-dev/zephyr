// ═══════════════════════════════════════════════════════════
// Zephyr Standard Library — built-in functions and methods
// ═══════════════════════════════════════════════════════════

use std::rc::Rc;
use std::cell::RefCell;
use crate::interpreter::{Value, Env, ZephyrFn};
use crate::net;
use crate::json;
use crate::process;
use crate::zfs as fs;

pub fn register(env: &Env) {
    let natives = [
        // I/O
        "print", "println", "input", "eprint", "eprintln",
        // Type conversion
        "int", "float", "str", "bool",
        // Type checking
        "type_of",
        // Math
        "abs", "sqrt", "pow", "min", "max", "floor", "ceil", "round",
        // Collections
        "len", "push", "pop", "range",
        // Functional
        "map", "filter", "reduce", "zip", "enumerate", "sorted",
        // String
        "split", "join", "trim",
        // Option/Result
        "some", "ok", "err", "unwrap",
        // Misc
        "assert", "panic", "exit",
    ];
    for name in natives {
        env.define(name, Value::Function(ZephyrFn::Native(name.to_string())));
    }
    for name in net::net_functions() {
        env.define(name, Value::Function(ZephyrFn::Native(name.to_string())));
    }
    for name in json::json_functions() {
        env.define(name, Value::Function(ZephyrFn::Native(name.to_string())));
    }
    for name in process::process_functions() {
        env.define(name, Value::Function(ZephyrFn::Native(name.to_string())));
    }
    for name in fs::fs_functions() {
        env.define(name, Value::Function(ZephyrFn::Native(name.to_string())));
    }
}

pub fn call_native(name: &str, args: Vec<Value>, _env: &Env) -> Result<Value, String> {
    match name {
        // ── I/O ─────────────────────────────────────────────────────────────

        "print" => {
            let parts: Vec<String> = args.iter().map(|v| format!("{}", v)).collect();
            print!("{}", parts.join(" "));
            Ok(Value::Nil)
        }

        "println" => {
            let parts: Vec<String> = args.iter().map(|v| format!("{}", v)).collect();
            println!("{}", parts.join(" "));
            Ok(Value::Nil)
        }

        "eprint" => {
            let parts: Vec<String> = args.iter().map(|v| format!("{}", v)).collect();
            eprint!("{}", parts.join(" "));
            Ok(Value::Nil)
        }

        "eprintln" => {
            let parts: Vec<String> = args.iter().map(|v| format!("{}", v)).collect();
            eprintln!("{}", parts.join(" "));
            Ok(Value::Nil)
        }

        "input" => {
            if let Some(prompt) = args.first() {
                print!("{}", prompt);
                use std::io::Write;
                std::io::stdout().flush().ok();
            }
            let mut line = String::new();
            std::io::stdin().read_line(&mut line).map_err(|e| e.to_string())?;
            Ok(Value::Str(line.trim_end_matches('\n').to_string()))
        }

        // ── Type conversion ───────────────────────────────────────────────

        "int" => {
            let arg = args.into_iter().next().ok_or("int() requires 1 argument")?;
            match arg {
                Value::Int(n)   => Ok(Value::Int(n)),
                Value::Float(f) => Ok(Value::Int(f as i64)),
                Value::Str(s)   => s.trim().parse::<i64>()
                    .map(Value::Int)
                    .map_err(|_| format!("Cannot convert '{}' to Int", s)),
                Value::Bool(b)  => Ok(Value::Int(if b { 1 } else { 0 })),
                other => Err(format!("Cannot convert {} to Int", other))
            }
        }

        "float" => {
            let arg = args.into_iter().next().ok_or("float() requires 1 argument")?;
            match arg {
                Value::Float(f) => Ok(Value::Float(f)),
                Value::Int(n)   => Ok(Value::Float(n as f64)),
                Value::Str(s)   => s.trim().parse::<f64>()
                    .map(Value::Float)
                    .map_err(|_| format!("Cannot convert '{}' to Float", s)),
                other => Err(format!("Cannot convert {} to Float", other))
            }
        }

        "str" => {
            let arg = args.into_iter().next().ok_or("str() requires 1 argument")?;
            Ok(Value::Str(format!("{}", arg)))
        }

        "bool" => {
            let arg = args.into_iter().next().ok_or("bool() requires 1 argument")?;
            Ok(Value::Bool(crate::interpreter::is_truthy(&arg)))
        }

        "type_of" => {
            let arg = args.into_iter().next().ok_or("type_of() requires 1 argument")?;
            Ok(Value::Str(crate::interpreter::value_type_name(&arg)))
        }

        // ── Math ──────────────────────────────────────────────────────────

        "abs" => {
            let n = args.into_iter().next().ok_or("abs() requires 1 argument")?;
            match n {
                Value::Int(v)   => Ok(Value::Int(v.abs())),
                Value::Float(f) => Ok(Value::Float(f.abs())),
                other => Err(format!("abs() expects number, got {}", other))
            }
        }

        "sqrt" => {
            let n = args.into_iter().next().ok_or("sqrt() requires 1 argument")?;
            match n {
                Value::Int(v)   => Ok(Value::Float((v as f64).sqrt())),
                Value::Float(f) => Ok(Value::Float(f.sqrt())),
                other => Err(format!("sqrt() expects number, got {}", other))
            }
        }

        "pow" => {
            if args.len() < 2 { return Err("pow(base, exp) requires 2 arguments".into()); }
            let base = to_f64(&args[0])?;
            let exp = to_f64(&args[1])?;
            Ok(Value::Float(base.powf(exp)))
        }

        "min" => {
            if args.len() < 2 { return Err("min() requires at least 2 arguments".into()); }
            let a = to_f64(&args[0])?;
            let b = to_f64(&args[1])?;
            if matches!(args[0], Value::Int(_)) && matches!(args[1], Value::Int(_)) {
                Ok(Value::Int(a.min(b) as i64))
            } else {
                Ok(Value::Float(a.min(b)))
            }
        }

        "max" => {
            if args.len() < 2 { return Err("max() requires at least 2 arguments".into()); }
            let a = to_f64(&args[0])?;
            let b = to_f64(&args[1])?;
            if matches!(args[0], Value::Int(_)) && matches!(args[1], Value::Int(_)) {
                Ok(Value::Int(a.max(b) as i64))
            } else {
                Ok(Value::Float(a.max(b)))
            }
        }

        "floor" => {
            let n = to_f64(&args.into_iter().next().ok_or("floor() requires 1 argument")?)?;
            Ok(Value::Int(n.floor() as i64))
        }

        "ceil" => {
            let n = to_f64(&args.into_iter().next().ok_or("ceil() requires 1 argument")?)?;
            Ok(Value::Int(n.ceil() as i64))
        }

        "round" => {
            let n = to_f64(&args.into_iter().next().ok_or("round() requires 1 argument")?)?;
            Ok(Value::Int(n.round() as i64))
        }

        // ── Collections ───────────────────────────────────────────────────

        "len" => {
            let arg = args.into_iter().next().ok_or("len() requires 1 argument")?;
            match arg {
                Value::List(v)  => Ok(Value::Int(v.borrow().len() as i64)),
                Value::Str(s)   => Ok(Value::Int(s.chars().count() as i64)),
                Value::Map(m)   => Ok(Value::Int(m.borrow().len() as i64)),
                Value::Tuple(t) => Ok(Value::Int(t.len() as i64)),
                other => Err(format!("len() not supported for {}", other))
            }
        }

        "push" => {
            if args.len() < 2 { return Err("push(list, item) requires 2 arguments".into()); }
            if let Value::List(v) = &args[0] {
                v.borrow_mut().push(args[1].clone());
                Ok(Value::Nil)
            } else {
                Err("push() requires a List as first argument".into())
            }
        }

        "pop" => {
            let arg = args.into_iter().next().ok_or("pop() requires 1 argument")?;
            if let Value::List(v) = arg {
                Ok(v.borrow_mut().pop().unwrap_or(Value::Nil))
            } else {
                Err("pop() requires a List".into())
            }
        }

        "range" => {
            match args.len() {
                1 => {
                    let end = crate::interpreter::require_int(&args[0]).map_err(|e| format!("{:?}", e))?;
                    let list: Vec<Value> = (0..end).map(Value::Int).collect();
                    Ok(Value::List(Rc::new(RefCell::new(list))))
                }
                2 => {
                    let start = crate::interpreter::require_int(&args[0]).map_err(|e| format!("{:?}", e))?;
                    let end = crate::interpreter::require_int(&args[1]).map_err(|e| format!("{:?}", e))?;
                    let list: Vec<Value> = (start..end).map(Value::Int).collect();
                    Ok(Value::List(Rc::new(RefCell::new(list))))
                }
                3 => {
                    let start = crate::interpreter::require_int(&args[0]).map_err(|e| format!("{:?}", e))?;
                    let end = crate::interpreter::require_int(&args[1]).map_err(|e| format!("{:?}", e))?;
                    let step = crate::interpreter::require_int(&args[2]).map_err(|e| format!("{:?}", e))?;
                    if step == 0 { return Err("range() step cannot be 0".into()); }
                    let step_abs = step.unsigned_abs() as usize;
                    let list: Vec<Value> = if step > 0 {
                        (start..end).step_by(step_abs).map(Value::Int).collect()
                    } else {
                        (end+1..=start).rev().step_by(step_abs).map(Value::Int).collect()
                    };
                    Ok(Value::List(Rc::new(RefCell::new(list))))
                }
                _ => Err("range() takes 1-3 arguments".into())
            }
        }

        // ── Functional ────────────────────────────────────────────────────

        "map" | "filter" | "reduce" | "zip" | "enumerate" | "sorted" => {
            // These need a callable and collection — handled via method calls on List
            // Here we provide a bare function alias
            Err(format!("{} is better used as a method: list.{}(fn)", name, name))
        }

        // ── String ────────────────────────────────────────────────────────

        "split" => {
            if args.len() < 2 { return Err("split(str, sep) requires 2 arguments".into()); }
            if let (Value::Str(s), Value::Str(sep)) = (&args[0], &args[1]) {
                let parts: Vec<Value> = s.split(sep.as_str()).map(|p| Value::Str(p.to_string())).collect();
                Ok(Value::List(Rc::new(RefCell::new(parts))))
            } else {
                Err("split() requires (String, String)".into())
            }
        }

        "join" => {
            if args.len() < 2 { return Err("join(list, sep) requires 2 arguments".into()); }
            if let (Value::List(v), Value::Str(sep)) = (&args[0], &args[1]) {
                let strs: Vec<String> = v.borrow().iter().map(|x| format!("{}", x)).collect();
                Ok(Value::Str(strs.join(sep)))
            } else {
                Err("join() requires (List, String)".into())
            }
        }

        "trim" => {
            let arg = args.into_iter().next().ok_or("trim() requires 1 argument")?;
            if let Value::Str(s) = arg {
                Ok(Value::Str(s.trim().to_string()))
            } else {
                Err("trim() requires String".into())
            }
        }

        // ── Option/Result ─────────────────────────────────────────────────

        "some" => {
            let arg = args.into_iter().next().ok_or("some() requires 1 argument")?;
            Ok(Value::Option(Some(Box::new(arg))))
        }

        "ok" => {
            let arg = args.into_iter().next().ok_or("ok() requires 1 argument")?;
            Ok(Value::Result(std::result::Result::Ok(Box::new(arg))))
        }

        "err" => {
            let arg = args.into_iter().next().ok_or("err() requires 1 argument")?;
            Ok(Value::Result(std::result::Result::Err(Box::new(arg))))
        }

        "unwrap" => {
            let arg = args.into_iter().next().ok_or("unwrap() requires 1 argument")?;
            match arg {
                Value::Option(Some(v)) => Ok(*v),
                Value::Option(None)    => Err("unwrap() called on nil Option".into()),
                Value::Result(std::result::Result::Ok(v)) => Ok(*v),
                Value::Result(std::result::Result::Err(e)) => Err(format!("unwrap() on Err: {}", e)),
                other => Ok(other),
            }
        }

        // ── Misc ──────────────────────────────────────────────────────────

        "assert" => {
            let cond = args.first().ok_or("assert() requires at least 1 argument")?;
            if !crate::interpreter::is_truthy(cond) {
                let msg = args.get(1)
                    .map(|v| format!("{}", v))
                    .unwrap_or_else(|| "Assertion failed".to_string());
                Err(msg)
            } else {
                Ok(Value::Nil)
            }
        }

        "panic" => {
            let msg = args.first().map(|v| format!("{}", v)).unwrap_or_else(|| "explicit panic".to_string());
            eprintln!("\x1b[31m[Zephyr panic]\x1b[0m {}", msg);
            std::process::exit(1);
        }

        "exit" => {
            let code = args.first().and_then(|v| if let Value::Int(n) = v { Some(*n) } else { None }).unwrap_or(0);
            std::process::exit(code as i32);
        }

        // ── Net ───────────────────────────────────────────────────────────────
        name if net::net_functions().contains(&name) => {
            net::call_net(name, args).map_err(|e| e)
        }

        // ── JSON ──────────────────────────────────────────────────────────────
        name if json::json_functions().contains(&name) => {
            json::call_json(name, args).map_err(|e| e)
        }
        // ── Process ───────────────────────────────────────────────────────────
        name if process::process_functions().contains(&name) => {
            process::call_process(name, args).map_err(|e| e)
        }
        // ── File System ───────────────────────────────────────────────────────
        name if fs::fs_functions().contains(&name) => {
            fs::call_fs(name, args).map_err(|e| e)
        }
        _ => Err(format!("Unknown native function '{}'", name))
    }
}

// ── Built-in methods ──────────────────────────────────────────────────────────

pub fn call_builtin_method(obj: Value, method: &str, args: Vec<Value>, _env: &Env) -> Result<Value, String> {
    match (&obj, method) {
        // ── List methods ─────────────────────────────────────────────────

        (Value::List(v), "push") => {
            let item = args.into_iter().next().ok_or("push() requires 1 argument")?;
            v.borrow_mut().push(item);
            Ok(Value::Nil)
        }
        (Value::List(v), "pop") => {
            Ok(v.borrow_mut().pop().unwrap_or(Value::Nil))
        }
        (Value::List(v), "len") => Ok(Value::Int(v.borrow().len() as i64)),
        (Value::List(v), "is_empty") => Ok(Value::Bool(v.borrow().is_empty())),
        (Value::List(v), "first") => {
            Ok(v.borrow().first().cloned().map(|x| Value::Option(Some(Box::new(x)))).unwrap_or(Value::Option(None)))
        }
        (Value::List(v), "last") => {
            Ok(v.borrow().last().cloned().map(|x| Value::Option(Some(Box::new(x)))).unwrap_or(Value::Option(None)))
        }
        (Value::List(v), "contains") => {
            let item = args.into_iter().next().ok_or("contains() requires 1 argument")?;
            Ok(Value::Bool(v.borrow().contains(&item)))
        }
        (Value::List(v), "join") => {
            let sep = args.into_iter().next().ok_or("join() requires separator")?;
            let sep = format!("{}", sep);
            let parts: Vec<String> = v.borrow().iter().map(|x| format!("{}", x)).collect();
            Ok(Value::Str(parts.join(&sep)))
        }
        (Value::List(v), "reverse") => {
            let mut list = v.borrow().clone();
            list.reverse();
            Ok(Value::List(Rc::new(RefCell::new(list))))
        }
        (Value::List(v), "sort") => {
            let mut list = v.borrow().clone();
            list.sort_by(|a, b| {
                match (a, b) {
                    (Value::Int(x), Value::Int(y)) => x.cmp(y),
                    (Value::Float(x), Value::Float(y)) => x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal),
                    (Value::Str(x), Value::Str(y)) => x.cmp(y),
                    _ => std::cmp::Ordering::Equal,
                }
            });
            Ok(Value::List(Rc::new(RefCell::new(list))))
        }
        (Value::List(v), "slice") => {
            let start = args.get(0).and_then(|a| if let Value::Int(n) = a { Some(*n as usize) } else { None }).unwrap_or(0);
            let list = v.borrow();
            let end = args.get(1)
                .and_then(|a| if let Value::Int(n) = a { Some(*n as usize) } else { None })
                .unwrap_or(list.len());
            Ok(Value::List(Rc::new(RefCell::new(list[start.min(list.len())..end.min(list.len())].to_vec()))))
        }
        (Value::List(v), "map") | (Value::List(v), "filter") | (Value::List(v), "reduce") => {
            let func = args.into_iter().next().ok_or(format!("{}() requires a function", method))?;
            let list = v.borrow().clone();
            match method {
                "map" => {
                    // We can't call interpreter from here, so we create a transformed list
                    // This requires calling the function — we'll return a note to user
                    Err("map() on List requires the Zephyr interpreter context; use a for loop or call via global map(list, fn)".into())
                }
                _ => Err(format!("{}() requires interpreter context", method))
            }
        }
        (Value::List(v), "enumerate") => {
            let pairs: Vec<Value> = v.borrow().iter().enumerate()
                .map(|(i, x)| Value::Tuple(vec![Value::Int(i as i64), x.clone()]))
                .collect();
            Ok(Value::List(Rc::new(RefCell::new(pairs))))
        }

        // ── String methods ────────────────────────────────────────────────

        (Value::Str(s), "len")        => Ok(Value::Int(s.chars().count() as i64)),
        (Value::Str(s), "is_empty")   => Ok(Value::Bool(s.is_empty())),
        (Value::Str(s), "to_upper")   => Ok(Value::Str(s.to_uppercase())),
        (Value::Str(s), "to_lower")   => Ok(Value::Str(s.to_lowercase())),
        (Value::Str(s), "trim")       => Ok(Value::Str(s.trim().to_string())),
        (Value::Str(s), "trim_start") => Ok(Value::Str(s.trim_start().to_string())),
        (Value::Str(s), "trim_end")   => Ok(Value::Str(s.trim_end().to_string())),
        (Value::Str(s), "chars")      => {
            let chars: Vec<Value> = s.chars().map(|c| Value::Str(c.to_string())).collect();
            Ok(Value::List(Rc::new(RefCell::new(chars))))
        }
        (Value::Str(s), "split") => {
            let sep = args.into_iter().next().ok_or("split() requires separator")?;
            let sep = format!("{}", sep);
            let parts: Vec<Value> = s.split(sep.as_str()).map(|p| Value::Str(p.to_string())).collect();
            Ok(Value::List(Rc::new(RefCell::new(parts))))
        }
        (Value::Str(s), "starts_with") => {
            let prefix = args.into_iter().next().ok_or("starts_with() requires argument")?;
            Ok(Value::Bool(s.starts_with(&format!("{}", prefix))))
        }
        (Value::Str(s), "ends_with") => {
            let suffix = args.into_iter().next().ok_or("ends_with() requires argument")?;
            Ok(Value::Bool(s.ends_with(&format!("{}", suffix))))
        }
        (Value::Str(s), "contains") => {
            let needle = args.into_iter().next().ok_or("contains() requires argument")?;
            Ok(Value::Bool(s.contains(&format!("{}", needle))))
        }
        (Value::Str(s), "replace") => {
            if args.len() < 2 { return Err("replace(from, to) requires 2 arguments".into()); }
            let from = format!("{}", args[0]);
            let to = format!("{}", args[1]);
            Ok(Value::Str(s.replace(&from, &to)))
        }
        (Value::Str(s), "parse_int") => {
            s.trim().parse::<i64>()
                .map(|n| Value::Result(std::result::Result::Ok(Box::new(Value::Int(n)))))
                .or_else(|_| Ok(Value::Result(std::result::Result::Err(Box::new(Value::Str(format!("Cannot parse '{}' as Int", s)))))))
        }
        (Value::Str(s), "parse_float") => {
            s.trim().parse::<f64>()
                .map(|f| Value::Result(std::result::Result::Ok(Box::new(Value::Float(f)))))
                .or_else(|_| Ok(Value::Result(std::result::Result::Err(Box::new(Value::Str(format!("Cannot parse '{}' as Float", s)))))))
        }
        (Value::Str(s), "repeat") => {
            let n = args.into_iter().next().ok_or("repeat() requires argument")?;
            if let Value::Int(n) = n { Ok(Value::Str(s.repeat(n as usize))) }
            else { Err("repeat() requires Int".into()) }
        }
        (Value::Str(s), "lines") => {
            let lines: Vec<Value> = s.lines().map(|l| Value::Str(l.to_string())).collect();
            Ok(Value::List(Rc::new(RefCell::new(lines))))
        }

        // ── Int methods ────────────────────────────────────────────────────

        (Value::Int(n), "to_str")   => Ok(Value::Str(n.to_string())),
        (Value::Int(n), "abs")      => Ok(Value::Int(n.abs())),
        (Value::Int(n), "to_float") => Ok(Value::Float(*n as f64)),
        (Value::Int(n), "pow") => {
            let exp = args.into_iter().next().ok_or("pow() requires argument")?;
            if let Value::Int(e) = exp { Ok(Value::Int(n.pow(e as u32))) }
            else if let Value::Float(e) = exp { Ok(Value::Float((*n as f64).powf(e))) }
            else { Err("pow() requires number".into()) }
        }

        // ── Float methods ──────────────────────────────────────────────────

        (Value::Float(f), "to_str")   => Ok(Value::Str(format!("{}", f))),
        (Value::Float(f), "abs")      => Ok(Value::Float(f.abs())),
        (Value::Float(f), "floor")    => Ok(Value::Int(f.floor() as i64)),
        (Value::Float(f), "ceil")     => Ok(Value::Int(f.ceil() as i64)),
        (Value::Float(f), "round")    => Ok(Value::Int(f.round() as i64)),
        (Value::Float(f), "sqrt")     => Ok(Value::Float(f.sqrt())),
        (Value::Float(f), "to_int")   => Ok(Value::Int(*f as i64)),

        // ── Map methods ────────────────────────────────────────────────────

        (Value::Map(m), "get") => {
            let key = args.into_iter().next().ok_or("get() requires key")?;
            let key = format!("{}", key);
            match m.borrow().get(&key) {
                Some(v) => Ok(Value::Option(Some(Box::new(v.clone())))),
                None    => Ok(Value::Option(None)),
            }
        }
        (Value::Map(m), "set") => {
            if args.len() < 2 { return Err("set(key, val) requires 2 arguments".into()); }
            let key = format!("{}", args[0]);
            m.borrow_mut().insert(key, args[1].clone());
            Ok(Value::Nil)
        }
        (Value::Map(m), "contains_key") => {
            let key = format!("{}", args.into_iter().next().ok_or("contains_key() requires key")?);
            Ok(Value::Bool(m.borrow().contains_key(&key)))
        }
        (Value::Map(m), "remove") => {
            let key = format!("{}", args.into_iter().next().ok_or("remove() requires key")?);
            Ok(m.borrow_mut().remove(&key).unwrap_or(Value::Nil))
        }
        (Value::Map(m), "keys") => {
            let keys: Vec<Value> = m.borrow().keys().map(|k| Value::Str(k.clone())).collect();
            Ok(Value::List(Rc::new(RefCell::new(keys))))
        }
        (Value::Map(m), "values") => {
            let vals: Vec<Value> = m.borrow().values().cloned().collect();
            Ok(Value::List(Rc::new(RefCell::new(vals))))
        }
        (Value::Map(m), "len") => Ok(Value::Int(m.borrow().len() as i64)),
        (Value::Map(m), "is_empty") => Ok(Value::Bool(m.borrow().is_empty())),

        // ── Option methods ─────────────────────────────────────────────────

        (Value::Option(inner), "is_some") => Ok(Value::Bool(inner.is_some())),
        (Value::Option(inner), "is_none") => Ok(Value::Bool(inner.is_none())),
        (Value::Option(Some(v)), "unwrap") => Ok(*v.clone()),
        (Value::Option(None), "unwrap") => Err("unwrap() on nil Option".into()),
        (Value::Option(inner), "unwrap_or") => {
            let default = args.into_iter().next().ok_or("unwrap_or() requires default")?;
            Ok(inner.as_ref().map(|v| *v.clone()).unwrap_or(default))
        }

        // ── Result methods ─────────────────────────────────────────────────

        (Value::Result(r), "is_ok")  => Ok(Value::Bool(r.is_ok())),
        (Value::Result(r), "is_err") => Ok(Value::Bool(r.is_err())),
        (Value::Result(Ok(v)), "unwrap") => Ok(*v.clone()),
        (Value::Result(Err(e)), "unwrap") => Err(format!("unwrap() on Err: {}", e)),
        (Value::Result(r), "unwrap_or") => {
            let default = args.into_iter().next().ok_or("unwrap_or() requires default")?;
            Ok(match r { Ok(v) => *v.clone(), Err(_) => default })
        }

        // ── Ref methods ────────────────────────────────────────────────────

        (Value::Ref(r), "get") => Ok(r.borrow().clone()),
        (Value::Ref(r), "set") => {
            let val = args.into_iter().next().ok_or("set() requires argument")?;
            *r.borrow_mut() = val;
            Ok(Value::Nil)
        }

        _ => Err(format!("No method '{}' on type {}", method, crate::interpreter::value_type_name(&obj)))
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn to_f64(val: &Value) -> Result<f64, String> {
    match val {
        Value::Int(n)   => Ok(*n as f64),
        Value::Float(f) => Ok(*f),
        other => Err(format!("Expected number, got {}", other))
    }
}

trait ValueExt { fn is_int(&self) -> bool; }
impl ValueExt for Value {
    fn is_int(&self) -> bool {
        match self {
            Value::Int(_) => true,
            _ => false,
        }
    }
}