// ═══════════════════════════════════════════════════════════
// Zephyr Interpreter — tree-walking evaluator
// ═══════════════════════════════════════════════════════════

use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;
use std::fmt;

use crate::ast::*;
use crate::stdlib;

// ── Values ────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Nil,
    Tuple(Vec<Value>),
    List(Rc<RefCell<Vec<Value>>>),
    Map(Rc<RefCell<HashMap<String, Value>>>),
    Struct(String, Rc<RefCell<HashMap<String, Value>>>),
    Enum(String, String, Vec<Value>),   // type_name, variant, fields
    Option(Option<Box<Value>>),          // Some(v) or None→Nil
    Result(std::result::Result<Box<Value>, Box<Value>>),
    Function(ZephyrFn),
    Ref(Rc<RefCell<Value>>),
}

#[derive(Clone, Debug)]
pub enum ZephyrFn {
    UserDefined {
        name: Option<String>,
        params: Vec<Param>,
        body: Vec<Stmt>,
        closure_env: Env,
    },
    Native(String),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(n)    => write!(f, "{}", n),
            Value::Float(n)  => {
                if n.fract() == 0.0 { write!(f, "{:.1}", n) } else { write!(f, "{}", n) }
            }
            Value::Bool(b)   => write!(f, "{}", b),
            Value::Str(s)    => write!(f, "{}", s),
            Value::Nil       => write!(f, "nil"),
            Value::Tuple(v)  => {
                write!(f, "(")?;
                for (i, x) in v.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", x)?;
                }
                write!(f, ")")
            }
            Value::List(v)   => {
                write!(f, "[")?;
                for (i, x) in v.borrow().iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", x)?;
                }
                write!(f, "]")
            }
            Value::Map(m)    => {
                write!(f, "{{")?;
                for (i, (k, v)) in m.borrow().iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}: {}", k, v)?;
                }
                write!(f, "}}")
            }
            Value::Struct(name, fields) => {
                write!(f, "{} {{", name)?;
                for (i, (k, v)) in fields.borrow().iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}: {}", k, v)?;
                }
                write!(f, "}}")
            }
            Value::Enum(_, variant, fields) => {
                if fields.is_empty() {
                    write!(f, "{}", variant)
                } else {
                    write!(f, "{}(", variant)?;
                    for (i, x) in fields.iter().enumerate() {
                        if i > 0 { write!(f, ", ")?; }
                        write!(f, "{}", x)?;
                    }
                    write!(f, ")")
                }
            }
            Value::Option(Some(v)) => write!(f, "Some({})", v),
            Value::Option(None)    => write!(f, "nil"),
            Value::Result(Ok(v))   => write!(f, "Ok({})", v),
            Value::Result(Err(e))  => write!(f, "Err({})", e),
            Value::Function(ZephyrFn::UserDefined { name, .. }) => {
                write!(f, "<fun {}>", name.as_deref().unwrap_or("<closure>"))
            }
            Value::Function(ZephyrFn::Native(n)) => write!(f, "<native {}>", n),
            Value::Ref(r) => write!(f, "ref({})", r.borrow()),
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Int(a), Value::Int(b))     => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Int(a), Value::Float(b))   => (*a as f64) == *b,
            (Value::Float(a), Value::Int(b))   => *a == (*b as f64),
            (Value::Bool(a), Value::Bool(b))   => a == b,
            (Value::Str(a), Value::Str(b))     => a == b,
            (Value::Nil, Value::Nil)           => true,
            (Value::Option(None), Value::Nil)  => true,
            (Value::Nil, Value::Option(None))  => true,
            (Value::Option(a), Value::Option(b)) => a == b,
            (Value::Tuple(a), Value::Tuple(b)) => a == b,
            _                                   => false,
        }
    }
}

// ── Environment ───────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct Env(Rc<RefCell<EnvInner>>);

#[derive(Debug)]
struct EnvInner {
    vars: HashMap<String, Rc<RefCell<Value>>>,
    parent: Option<Env>,
}

impl Env {
    pub fn new() -> Self {
        Env(Rc::new(RefCell::new(EnvInner {
            vars: HashMap::new(),
            parent: None,
        })))
    }

    pub fn child(parent: &Env) -> Self {
        Env(Rc::new(RefCell::new(EnvInner {
            vars: HashMap::new(),
            parent: Some(parent.clone()),
        })))
    }

    pub fn define(&self, name: &str, val: Value) {
        self.0.borrow_mut().vars.insert(name.to_string(), Rc::new(RefCell::new(val)));
    }

    pub fn set(&self, name: &str, val: Value) -> bool {
        let mut inner = self.0.borrow_mut();
        if let Some(cell) = inner.vars.get(name) {
            *cell.borrow_mut() = val;
            return true;
        }
        if let Some(parent) = &inner.parent.clone() {
            return parent.set(name, val);
        }
        false
    }

    pub fn get(&self, name: &str) -> Option<Value> {
        let inner = self.0.borrow();
        if let Some(cell) = inner.vars.get(name) {
            return Some(cell.borrow().clone());
        }
        if let Some(parent) = &inner.parent {
            return parent.get(name);
        }
        None
    }
}

// ── Control flow signals ──────────────────────────────────────────────────────

#[derive(Debug)]
pub enum Signal {
    Return(Value),
    Break,
    Continue,
    Error(String),
    PropagateErr(Value), // for ? operator
}

impl From<String> for Signal {
    fn from(s: String) -> Self { Signal::Error(s) }
}

type EvalResult = std::result::Result<Value, Signal>;

// ── Interpreter ───────────────────────────────────────────────────────────────

pub struct Interpreter {
    pub global: Env,
    // struct/enum definitions
    pub struct_defs: HashMap<String, StructDef>,
    pub enum_defs: HashMap<String, EnumDef>,
    pub impl_methods: HashMap<String, HashMap<String, ZephyrFn>>,
    pub modules: HashMap<String, Env>,
}

impl Interpreter {
    pub fn new() -> Self {
        let global = Env::new();
        stdlib::register(&global);
        Interpreter {
            global,
            struct_defs: HashMap::new(),
            enum_defs: HashMap::new(),
            impl_methods: HashMap::new(),
            modules: HashMap::new(),
        }
    }

    pub fn run(&mut self, stmts: &[Stmt]) -> EvalResult {
        let env = self.global.clone();
        self.exec_block(stmts, &env)
    }

    fn exec_block(&mut self, stmts: &[Stmt], env: &Env) -> EvalResult {
        let mut last = Value::Nil;
        for stmt in stmts {
            last = self.exec_stmt(stmt, env)?;
        }
        Ok(last)
    }

    fn exec_stmt(&mut self, stmt: &Stmt, env: &Env) -> EvalResult {
        match stmt {
            Stmt::Let(name, _ty, expr, _mutable) => {
                let val = self.eval_expr(expr, env)?;
                env.define(name, val);
                Ok(Value::Nil)
            }

            Stmt::Expr(expr) => self.eval_expr(expr, env),

            Stmt::Return(expr) => {
                let val = if let Some(e) = expr.as_ref() { self.eval_expr(e, env)? } else { Value::Nil };
                Err(Signal::Return(val))
            }

            Stmt::Break    => Err(Signal::Break),
            Stmt::Continue => Err(Signal::Continue),

            Stmt::While(cond, body) => {
                loop {
                    let c = self.eval_expr(cond, env)?;
                    if !is_truthy(&c) { break; }
                    let loop_env = Env::child(env);
                    match self.exec_block(body, &loop_env) {
                        Ok(_) => {}
                        Err(Signal::Break) => break,
                        Err(Signal::Continue) => continue,
                        Err(e) => return Err(e),
                    }
                }
                Ok(Value::Nil)
            }

            Stmt::For(var, iter_expr, body) => {
                let iter_val = self.eval_expr(iter_expr, env)?;
                let items = self.value_to_iter(iter_val)?;
                for item in items {
                    let loop_env = Env::child(env);
                    loop_env.define(var, item);
                    match self.exec_block(body, &loop_env) {
                        Ok(_) => {}
                        Err(Signal::Break) => break,
                        Err(Signal::Continue) => continue,
                        Err(e) => return Err(e),
                    }
                }
                Ok(Value::Nil)
            }

            Stmt::FunDef(fun) => {
                let func = Value::Function(ZephyrFn::UserDefined {
                    name: Some(fun.name.clone()),
                    params: fun.params.clone(),
                    body: fun.body.clone(),
                    closure_env: env.clone(),
                });
                env.define(&fun.name, func);
                Ok(Value::Nil)
            }

            Stmt::StructDef(def) => {
                self.struct_defs.insert(def.name.clone(), def.clone());
                Ok(Value::Nil)
            }

            Stmt::EnumDef(def) => {
                self.enum_defs.insert(def.name.clone(), def.clone());
                // Register variant constructors as functions in env
                for variant in &def.variants {
                    let enum_name = def.name.clone();
                    let var_name = variant.name.clone();
                    let arity = variant.fields.len();
                    if arity == 0 {
                        env.define(&format!("{}::{}", enum_name, var_name),
                            Value::Enum(enum_name.clone(), var_name.clone(), vec![]));
                    }
                    // Constructors for variants with fields are handled by EnumVariant expr
                }
                Ok(Value::Nil)
            }

            Stmt::ImplBlock(block) => {
                let methods = self.impl_methods.entry(block.target.clone()).or_default();
                for method in &block.methods {
                    methods.insert(method.name.clone(), ZephyrFn::UserDefined {
                        name: Some(method.name.clone()),
                        params: method.params.clone(),
                        body: method.body.clone(),
                        closure_env: env.clone(),
                    });
                }
                Ok(Value::Nil)
            }

            Stmt::ModDef(name, stmts) => {
                let mod_env = Env::child(env);
                self.exec_block(stmts, &mod_env)?;
                self.modules.insert(name.clone(), mod_env.clone());
                env.define(name, Value::Nil); // placeholder
                Ok(Value::Nil)
            }

            Stmt::Import(path) => {
                // Simple module import — just note it for now
                // Full module system would load files from disk
                let _path_str = path.join(".");
                Ok(Value::Nil)
            }

            Stmt::TypeAlias(_, _, _) => Ok(Value::Nil), // type aliases are for type checking
        }
    }

    // ── Expression evaluation ─────────────────────────────────────────────────

    pub fn eval_expr(&mut self, expr: &Expr, env: &Env) -> EvalResult {
        match expr {
            Expr::Int(n)    => Ok(Value::Int(*n)),
            Expr::Float(f)  => Ok(Value::Float(*f)),
            Expr::Bool(b)   => Ok(Value::Bool(*b)),
            Expr::Nil       => Ok(Value::Nil),
            Expr::StringLit(s) => Ok(Value::Str(s.clone())),

            Expr::InterpolatedString(parts) => {
                let mut result = String::new();
                for part in parts {
                    match part {
                        crate::ast::StringPart::Literal(s) => result.push_str(s),
                        crate::ast::StringPart::Interpolated(e) => {
                            let v = self.eval_expr(&e, env)?;
                            result.push_str(&format!("{}", v));
                        }
                    }
                }
                Ok(Value::Str(result))
            }

            Expr::Var(name) => {
                env.get(name)
                    .or_else(|| self.global.get(name))
                    .ok_or_else(|| Signal::Error(format!("Undefined variable '{}'", name)))
            }

            Expr::Tuple(elems) => {
                let vals: std::result::Result<Vec<_>, _> = elems.iter().map(|e| self.eval_expr(e, env)).collect();
                Ok(Value::Tuple(vals?))
            }

            Expr::List(elems) => {
                let vals: std::result::Result<Vec<_>, _> = elems.iter().map(|e| self.eval_expr(e, env)).collect();
                Ok(Value::List(Rc::new(RefCell::new(vals?))))
            }

            Expr::MapLit(pairs) => {
                let mut map = HashMap::new();
                for (k, v) in pairs {
                    let kv = self.eval_expr(k, env)?;
                    let vv = self.eval_expr(v, env)?;
                    map.insert(format!("{}", kv), vv);
                }
                Ok(Value::Map(Rc::new(RefCell::new(map))))
            }

            Expr::Block(stmts, tail) => {
                let block_env = Env::child(env);
                for s in stmts { self.exec_stmt(s, &block_env)?; }
                if let Some(e) = tail {
                    self.eval_expr(e, &block_env)
                } else {
                    Ok(Value::Nil)
                }
            }

            Expr::BinOp(left, op, right) => {
                let l = self.eval_expr(left, env)?;
                let r = self.eval_expr(right, env)?;
                eval_binop(l, op, r)
            }

            Expr::UnaryOp(op, expr) => {
                let val = self.eval_expr(expr, env)?;
                match op {
                    UnaryOp::Neg => match val {
                        Value::Int(n)   => Ok(Value::Int(-n)),
                        Value::Float(f) => Ok(Value::Float(-f)),
                        other => Err(Signal::Error(format!("Cannot negate {:?}", other)))
                    }
                    UnaryOp::Not => Ok(Value::Bool(!is_truthy(&val)))
                }
            }

            Expr::Assign(target, value) => {
                let val = self.eval_expr(value, env)?;
                match target.as_ref() {
                    Expr::Var(name) => {
                        if !env.set(name, val.clone()) {
                            env.define(name, val);
                        }
                        Ok(Value::Nil)
                    }
                    Expr::Index(obj_expr, idx_expr) => {
                        let idx = self.eval_expr(idx_expr, env)?;
                        let obj = self.eval_expr(obj_expr, env)?;
                        match &obj {
                            Value::List(v) => {
                                let i = match require_int(&idx) { Ok(n) => n as usize, Err(e) => return Err(e) };
                                let mut list = v.borrow_mut();
                                if i < list.len() {
                                    list[i] = val;
                                    Ok(Value::Nil)
                                } else {
                                    Err(Signal::Error(format!("Index {} out of bounds", i)))
                                }
                            }
                            Value::Map(m) => {
                                m.borrow_mut().insert(format!("{}", idx), val);
                                Ok(Value::Nil)
                            }
                            _ => Err(Signal::Error("Cannot index-assign this value".into()))
                        }
                    }
                    Expr::FieldAccess(obj_expr, field) => {
                        let obj = self.eval_expr(obj_expr, env)?;
                        if let Value::Struct(_, fields) = obj {
                            fields.borrow_mut().insert(field.clone(), val);
                            Ok(Value::Nil)
                        } else {
                            Err(Signal::Error("Cannot assign field on non-struct".into()))
                        }
                    }
                    _ => Err(Signal::Error("Invalid assignment target".into()))
                }
            }

            Expr::Call(callee_expr, args) => {
                let callee = self.eval_expr(callee_expr, env)?;
                let arg_vals: std::result::Result<Vec<_>, _> = args.iter().map(|a| self.eval_expr(a, env)).collect();
                let arg_vals = arg_vals?;
                self.call_value(callee, arg_vals, env)
            }

            Expr::MethodCall(obj_expr, method, args) => {
                let obj = self.eval_expr(obj_expr, env)?;
                let arg_vals: std::result::Result<Vec<_>, _> = args.iter().map(|a| self.eval_expr(a, env)).collect();
                let arg_vals = arg_vals?;
                self.call_method(obj, method, arg_vals, env)
            }

            Expr::FieldAccess(obj_expr, field) => {
                let obj = self.eval_expr(obj_expr, env)?;
                match &obj {
                    Value::Struct(_, fields) => {
                        fields.borrow().get(field)
                            .cloned()
                            .ok_or_else(|| Signal::Error(format!("No field '{}'", field)))
                    }
                    Value::Tuple(elems) => {
                        let idx: usize = field.parse()
                            .map_err(|_| Signal::Error(format!("Invalid tuple index '{}'", field)))?;
                        elems.get(idx).cloned()
                            .ok_or_else(|| Signal::Error(format!("Tuple index {} out of bounds", idx)))
                    }
                    _ => Err(Signal::Error(format!("Cannot access field '{}' on {:?}", field, obj)))
                }
            }

            Expr::Index(obj_expr, idx_expr) => {
                let obj = self.eval_expr(obj_expr, env)?;
                let idx = self.eval_expr(idx_expr, env)?;
                match &obj {
                    Value::List(v) => {
                        let i = match require_int(&idx) { Ok(n) => n as usize, Err(e) => return Err(e) };
                        v.borrow().get(i).cloned()
                            .ok_or_else(|| Signal::Error(format!("Index {} out of bounds", i)))
                    }
                    Value::Str(s) => {
                        let i = match require_int(&idx) { Ok(n) => n as usize, Err(e) => return Err(e) };
                        s.chars().nth(i)
                            .map(|c| Value::Str(c.to_string()))
                            .ok_or_else(|| Signal::Error(format!("String index {} out of bounds", i)))
                    }
                    Value::Map(m) => {
                        let key = format!("{}", idx);
                        m.borrow().get(&key).cloned()
                            .ok_or_else(|| Signal::Error(format!("Key '{}' not found", key)))
                    }
                    _ => Err(Signal::Error("Cannot index this value".into()))
                }
            }

            Expr::If(cond, then_expr, elif_branches, else_expr) => {
                let cond_val = self.eval_expr(cond, env)?;
                if is_truthy(&cond_val) {
                    self.eval_expr(then_expr, env)
                } else {
                    for (elif_cond, elif_body) in elif_branches {
                        let cv = self.eval_expr(elif_cond, env)?;
                        if is_truthy(&cv) {
                            return self.eval_expr(elif_body, env);
                        }
                    }
                    if let Some(e) = else_expr {
                        self.eval_expr(e, env)
                    } else {
                        Ok(Value::Nil)
                    }
                }
            }

            Expr::Match(subject, arms) => {
                let val = self.eval_expr(subject, env)?;
                for arm in arms {
                    let match_env = Env::child(env);
                    if match_pattern(&arm.pattern, &val, &match_env)? {
                        if let Some(guard) = &arm.guard {
                            let gv = self.eval_expr(guard, &match_env)?;
                            if !is_truthy(&gv) { continue; }
                        }
                        return self.eval_expr(&arm.body, &match_env);
                    }
                }
                Err(Signal::Error("Non-exhaustive match".into()))
            }

            Expr::Closure(params, body) => {
                Ok(Value::Function(ZephyrFn::UserDefined {
                    name: None,
                    params: params.iter().map(|(n, t)| Param {
                        name: n.clone(), ty: t.clone(), default: None
                    }).collect(),
                    body: vec![Stmt::Return(Some(*body.clone()))],
                    closure_env: env.clone(),
                }))
            }

            Expr::StructCreate(name, field_exprs) => {
                let mut fields = HashMap::new();
                for (fname, fexpr) in field_exprs {
                    fields.insert(fname.clone(), self.eval_expr(fexpr, env)?);
                }
                Ok(Value::Struct(name.clone(), Rc::new(RefCell::new(fields))))
            }

            Expr::EnumVariant(enum_name, variant, args) => {
                let vals: std::result::Result<Vec<_>, _> = args.iter().map(|a| self.eval_expr(a, env)).collect();
                Ok(Value::Enum(enum_name.clone(), variant.clone(), vals?))
            }

            Expr::Range(start, end) => {
                let s = require_int(&self.eval_expr(start, env)?)?;
                let e = require_int(&self.eval_expr(end, env)?)?;
                let list: Vec<Value> = (s..e).map(Value::Int).collect();
                Ok(Value::List(Rc::new(RefCell::new(list))))
            }

            Expr::Some(inner) => {
                let v = self.eval_expr(inner, env)?;
                Ok(Value::Option(Some(Box::new(v))))
            }

            Expr::Ok(inner) => {
                let v = self.eval_expr(inner, env)?;
                Ok(Value::Result(std::result::Result::Ok(Box::new(v))))
            }

            Expr::Err(inner) => {
                let v = self.eval_expr(inner, env)?;
                Ok(Value::Result(std::result::Result::Err(Box::new(v))))
            }

            Expr::Question(inner) => {
                let val = self.eval_expr(inner, env)?;
                match val {
                    Value::Result(std::result::Result::Ok(v)) => Ok(*v),
                    Value::Result(std::result::Result::Err(e)) => Err(Signal::PropagateErr(*e)),
                    Value::Option(Some(v)) => Ok(*v),
                    Value::Option(None) | Value::Nil =>
                        Err(Signal::PropagateErr(Value::Str("None".into()))),
                    other => Ok(other),
                }
            }

            Expr::BoxExpr(inner) => {
                // In our interpreter, box just returns the value (GC handles memory)
                self.eval_expr(inner, env)
            }

            Expr::RefExpr(inner) => {
                let val = self.eval_expr(inner, env)?;
                Ok(Value::Ref(Rc::new(RefCell::new(val))))
            }

            Expr::Await(inner) => {
                // Await is not supported in the tree-walking interpreter
                self.eval_expr(inner, env)
            }
        }
    }

    fn call_value(&mut self, callee: Value, args: Vec<Value>, env: &Env) -> EvalResult {
        match callee {
            Value::Function(ZephyrFn::Native(name)) => {
                stdlib::call_native(&name, args, env)
                    .map_err(|e| Signal::Error(e))
            }
            Value::Function(ZephyrFn::UserDefined { params, body, closure_env, .. }) => {
                let call_env = Env::child(&closure_env);
                for (i, param) in params.iter().enumerate() {
                    let val = if i < args.len() {
                        args[i].clone()
                    } else if let Some(default) = &param.default {
                        self.eval_expr(default, env)?
                    } else {
                        return Err(Signal::Error(format!("Missing argument '{}'", param.name)));
                    };
                    call_env.define(&param.name, val);
                }
                match self.exec_block(&body, &call_env) {
                    Ok(v) => Ok(v),
                    Err(Signal::Return(v)) => Ok(v),
                    Err(Signal::PropagateErr(e)) => Ok(Value::Result(std::result::Result::Err(Box::new(e)))),
                    Err(e) => Err(e),
                }
            }
            other => Err(Signal::Error(format!("'{}' is not a function", other)))
        }
    }

    fn call_method(&mut self, obj: Value, method: &str, mut args: Vec<Value>, env: &Env) -> EvalResult {
        // Check impl methods first
        let type_name = value_type_name(&obj);
        if let Some(methods) = self.impl_methods.get(&type_name).cloned() {
            if let Some(func) = methods.get(method).cloned() {
                let mut all_args = vec![obj];
                all_args.append(&mut args);
                return self.call_value(Value::Function(func), all_args, env);
            }
        }

        // Built-in methods
        stdlib::call_builtin_method(obj, method, args, env)
            .map_err(|e| Signal::Error(e))
    }

    fn value_to_iter(&self, val: Value) -> std::result::Result<Vec<Value>, Signal> {
        match val {
            Value::List(v) => Ok(v.borrow().clone()),
            Value::Str(s)  => Ok(s.chars().map(|c| Value::Str(c.to_string())).collect()),
            other => Err(Signal::Error(format!("'{}' is not iterable", other)))
        }
    }
}

// ── Binary operations ─────────────────────────────────────────────────────────

fn eval_binop(l: Value, op: &BinOp, r: Value) -> EvalResult {
    match op {
        BinOp::Add => match (&l, &r) {
            (Value::Int(a), Value::Int(b))     => Ok(Value::Int(a + b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
            (Value::Int(a), Value::Float(b))   => Ok(Value::Float(*a as f64 + b)),
            (Value::Float(a), Value::Int(b))   => Ok(Value::Float(a + *b as f64)),
            (Value::Str(a), Value::Str(b))     => Ok(Value::Str(format!("{}{}", a, b))),
            (Value::Str(a), b)                 => Ok(Value::Str(format!("{}{}", a, b))),
            (Value::List(a), Value::List(b)) => {
                let mut combined = a.borrow().clone();
                combined.extend(b.borrow().clone());
                Ok(Value::List(Rc::new(RefCell::new(combined))))
            }
            _ => Err(Signal::Error(format!("Cannot add {} and {}", l, r)))
        },
        BinOp::Sub => numeric_op(&l, &r, |a, b| a - b, |a, b| a - b),
        BinOp::Mul => match (&l, &r) {
            (Value::Str(s), Value::Int(n)) => Ok(Value::Str(s.repeat(*n as usize))),
            _ => numeric_op(&l, &r, |a, b| a * b, |a, b| a * b),
        },
        BinOp::Div => match (&l, &r) {
            (_, Value::Int(0)) => Err(Signal::Error("Division by zero".into())),
            (_, Value::Float(f)) if *f == 0.0 => Err(Signal::Error("Division by zero".into())),
            _ => numeric_op(&l, &r, |a, b| a / b, |a, b| a / b),
        },
        BinOp::Mod => numeric_op(&l, &r, |a, b| a % b, |a, b| a % b),
        BinOp::Eq    => Ok(Value::Bool(l == r)),
        BinOp::NotEq => Ok(Value::Bool(l != r)),
        BinOp::Lt    => compare_op(&l, &r, |o| o == std::cmp::Ordering::Less),
        BinOp::LtEq  => compare_op(&l, &r, |o| o != std::cmp::Ordering::Greater),
        BinOp::Gt    => compare_op(&l, &r, |o| o == std::cmp::Ordering::Greater),
        BinOp::GtEq  => compare_op(&l, &r, |o| o != std::cmp::Ordering::Less),
        BinOp::And   => Ok(Value::Bool(is_truthy(&l) && is_truthy(&r))),
        BinOp::Or    => {
            if is_truthy(&l) { Ok(l) } else { Ok(r) }
        }
        BinOp::DotDot => Err(Signal::Error("Range not valid in this context".into())),
    }
}

fn numeric_op(l: &Value, r: &Value, int_op: impl Fn(i64, i64) -> i64, float_op: impl Fn(f64, f64) -> f64) -> EvalResult {
    match (l, r) {
        (Value::Int(a), Value::Int(b))     => Ok(Value::Int(int_op(*a, *b))),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(float_op(*a, *b))),
        (Value::Int(a), Value::Float(b))   => Ok(Value::Float(float_op(*a as f64, *b))),
        (Value::Float(a), Value::Int(b))   => Ok(Value::Float(float_op(*a, *b as f64))),
        _ => Err(Signal::Error(format!("Type error in numeric operation: {} and {}", l, r)))
    }
}

fn compare_op(l: &Value, r: &Value, pred: impl Fn(std::cmp::Ordering) -> bool) -> EvalResult {
    let ord = match (l, r) {
        (Value::Int(a), Value::Int(b))     => a.cmp(b),
        (Value::Float(a), Value::Float(b)) => a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal),
        (Value::Int(a), Value::Float(b))   => (*a as f64).partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal),
        (Value::Float(a), Value::Int(b))   => a.partial_cmp(&(*b as f64)).unwrap_or(std::cmp::Ordering::Equal),
        (Value::Str(a), Value::Str(b))     => a.cmp(b),
        _ => return Err(Signal::Error(format!("Cannot compare {} and {}", l, r)))
    };
    Ok(Value::Bool(pred(ord)))
}

// ── Pattern matching ──────────────────────────────────────────────────────────

fn match_pattern(pat: &Pattern, val: &Value, env: &Env) -> std::result::Result<bool, Signal> {
    match pat {
        Pattern::Wildcard  => Ok(true),
        Pattern::Nil       => Ok(matches!(val, Value::Nil | Value::Option(None))),
        Pattern::Bool(b)   => Ok(val == &Value::Bool(*b)),
        Pattern::Int(n)    => Ok(val == &Value::Int(*n)),
        Pattern::Float(f)  => Ok(val == &Value::Float(*f)),
        Pattern::StringLit(s) => Ok(val == &Value::Str(s.clone())),

        Pattern::Ident(name) => {
            env.define(name, val.clone());
            Ok(true)
        }

        Pattern::Tuple(pats) => {
            if let Value::Tuple(elems) = val {
                if elems.len() != pats.len() { return Ok(false); }
                for (p, e) in pats.iter().zip(elems.iter()) {
                    if !match_pattern(p, e, env)? { return Ok(false); }
                }
                Ok(true)
            } else { Ok(false) }
        }

        Pattern::List(pats) => {
            if let Value::List(elems) = val {
                let elems = elems.borrow();
                if elems.len() != pats.len() { return Ok(false); }
                for (p, e) in pats.iter().zip(elems.iter()) {
                    if !match_pattern(p, e, env)? { return Ok(false); }
                }
                Ok(true)
            } else { Ok(false) }
        }

        Pattern::EnumVariant(enum_name, variant, field_pats) => {
            if let Value::Enum(en, vn, fields) = val {
                if en != enum_name || vn != variant { return Ok(false); }
                if fields.len() != field_pats.len() { return Ok(false); }
                for (p, f) in field_pats.iter().zip(fields.iter()) {
                    if !match_pattern(p, f, env)? { return Ok(false); }
                }
                Ok(true)
            } else { Ok(false) }
        }

        Pattern::Some(inner) => match val {
            Value::Option(Some(v)) => match_pattern(inner, v, env),
            _ => Ok(false)
        }

        Pattern::Ok(inner) => match val {
            Value::Result(std::result::Result::Ok(v)) => match_pattern(inner, v, env),
            _ => Ok(false)
        }

        Pattern::Err(inner) => match val {
            Value::Result(std::result::Result::Err(e)) => match_pattern(inner, e, env),
            _ => Ok(false)
        }

        Pattern::Or(a, b) => {
            if match_pattern(a, val, env)? { Ok(true) }
            else { match_pattern(b, val, env) }
        }

        Pattern::Range(lo, hi) => {
            if let (Pattern::Int(lo), Pattern::Int(hi)) = (lo.as_ref(), hi.as_ref()) {
                if let Value::Int(n) = val {
                    return Ok(n >= lo && n < hi);
                }
            }
            Ok(false)
        }

        Pattern::Struct(_, _) => Ok(false), // TODO: struct pattern matching
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

pub fn is_truthy(val: &Value) -> bool {
    match val {
        Value::Bool(b)        => *b,
        Value::Nil            => false,
        Value::Option(None)   => false,
        Value::Int(0)         => false,
        Value::Str(s)         => !s.is_empty(),
        _                     => true,
    }
}

pub fn require_int(val: &Value) -> std::result::Result<i64, Signal> {
    match val {
        Value::Int(n)   => Ok(*n),
        Value::Float(f) => Ok(*f as i64),
        other => Err(Signal::Error(format!("Expected integer, got {}", other)))
    }
}

pub fn value_type_name(val: &Value) -> String {
    match val {
        Value::Int(_)    => "Int".into(),
        Value::Float(_)  => "Float".into(),
        Value::Bool(_)   => "Bool".into(),
        Value::Str(_)    => "String".into(),
        Value::Nil       => "Nil".into(),
        Value::List(_)   => "List".into(),
        Value::Map(_)    => "Map".into(),
        Value::Tuple(_)  => "Tuple".into(),
        Value::Struct(n, _) => n.clone(),
        Value::Enum(n, _, _) => n.clone(),
        Value::Option(_) => "Option".into(),
        Value::Result(_) => "Result".into(),
        Value::Function(_) => "Function".into(),
        Value::Ref(_)    => "Ref".into(),
    }
}