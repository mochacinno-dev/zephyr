// ═══════════════════════════════════════════════════════════
// Zephyr Bytecode — AST serialization to .zphc files
// ═══════════════════════════════════════════════════════════
//
// .zphc file format:
//
//   [4 bytes]  magic: 0x5A504843  ("ZPHC")
//   [2 bytes]  version: u16 little-endian  (current: 1)
//   [8 bytes]  source hash: u64 (FNV-1a of original source)
//   [4 bytes]  stmt count: u32
//   [N bytes]  serialized statements (recursive binary encoding)
//
// All multi-byte integers are little-endian.
// Strings are: [4-byte length][UTF-8 bytes]
// Booleans are: 0x00 (false) or 0x01 (true)
// Optional<T> is: 0x00 (None) or 0x01 followed by T (Some)
// Vec<T> is: [4-byte count] followed by count T values
//
// ═══════════════════════════════════════════════════════════

use std::io::{self, Read, Write};
use crate::ast::*;

// ── Constants ─────────────────────────────────────────────────────────────────

const MAGIC: u32 = 0x5A504843; // "ZPHC"
const VERSION: u16 = 1;

// ── Tag bytes for each AST variant ───────────────────────────────────────────
// Expr tags
const TAG_EXPR_INT: u8          = 0x01;
const TAG_EXPR_FLOAT: u8        = 0x02;
const TAG_EXPR_BOOL: u8         = 0x03;
const TAG_EXPR_STRING: u8       = 0x04;
const TAG_EXPR_NIL: u8          = 0x05;
const TAG_EXPR_INTERP: u8       = 0x06;
const TAG_EXPR_VAR: u8          = 0x07;
const TAG_EXPR_TUPLE: u8        = 0x08;
const TAG_EXPR_LIST: u8         = 0x09;
const TAG_EXPR_MAPLIT: u8       = 0x0A;
const TAG_EXPR_BLOCK: u8        = 0x0B;
const TAG_EXPR_BINOP: u8        = 0x0C;
const TAG_EXPR_UNARYOP: u8      = 0x0D;
const TAG_EXPR_CALL: u8         = 0x0E;
const TAG_EXPR_METHODCALL: u8   = 0x0F;
const TAG_EXPR_FIELDACCESS: u8  = 0x10;
const TAG_EXPR_INDEX: u8        = 0x11;
const TAG_EXPR_IF: u8           = 0x12;
const TAG_EXPR_MATCH: u8        = 0x13;
const TAG_EXPR_CLOSURE: u8      = 0x14;
const TAG_EXPR_STRUCTCREATE: u8 = 0x15;
const TAG_EXPR_ENUMVARIANT: u8  = 0x16;
const TAG_EXPR_RANGE: u8        = 0x17;
const TAG_EXPR_SOME: u8         = 0x18;
const TAG_EXPR_OK: u8           = 0x19;
const TAG_EXPR_ERR: u8          = 0x1A;
const TAG_EXPR_QUESTION: u8     = 0x1B;
const TAG_EXPR_BOX: u8          = 0x1C;
const TAG_EXPR_REF: u8          = 0x1D;
const TAG_EXPR_ASSIGN: u8       = 0x1E;
const TAG_EXPR_AWAIT: u8        = 0x1F;

// Stmt tags
const TAG_STMT_LET: u8          = 0x40;
const TAG_STMT_EXPR: u8         = 0x41;
const TAG_STMT_RETURN: u8       = 0x42;
const TAG_STMT_BREAK: u8        = 0x43;
const TAG_STMT_CONTINUE: u8     = 0x44;
const TAG_STMT_WHILE: u8        = 0x45;
const TAG_STMT_FOR: u8          = 0x46;
const TAG_STMT_FUNDEF: u8       = 0x47;
const TAG_STMT_STRUCTDEF: u8    = 0x48;
const TAG_STMT_ENUMDEF: u8      = 0x49;
const TAG_STMT_IMPLBLOCK: u8    = 0x4A;
const TAG_STMT_MODDEF: u8       = 0x4B;
const TAG_STMT_IMPORT: u8       = 0x4C;
const TAG_STMT_TYPEALIAS: u8    = 0x4D;

// Type tags
const TAG_TYPE_INT: u8          = 0x80;
const TAG_TYPE_FLOAT: u8        = 0x81;
const TAG_TYPE_BOOL: u8         = 0x82;
const TAG_TYPE_STRING: u8       = 0x83;
const TAG_TYPE_NIL: u8          = 0x84;
const TAG_TYPE_OPTION: u8       = 0x85;
const TAG_TYPE_RESULT: u8       = 0x86;
const TAG_TYPE_LIST: u8         = 0x87;
const TAG_TYPE_MAP: u8          = 0x88;
const TAG_TYPE_TUPLE: u8        = 0x89;
const TAG_TYPE_NAMED: u8        = 0x8A;
const TAG_TYPE_GENERIC: u8      = 0x8B;
const TAG_TYPE_FUNCTION: u8     = 0x8C;
const TAG_TYPE_INFERRED: u8     = 0x8D;

// BinOp tags
const TAG_BINOP_ADD: u8   = 0x01;
const TAG_BINOP_SUB: u8   = 0x02;
const TAG_BINOP_MUL: u8   = 0x03;
const TAG_BINOP_DIV: u8   = 0x04;
const TAG_BINOP_MOD: u8   = 0x05;
const TAG_BINOP_EQ: u8    = 0x06;
const TAG_BINOP_NEQ: u8   = 0x07;
const TAG_BINOP_LT: u8    = 0x08;
const TAG_BINOP_LTEQ: u8  = 0x09;
const TAG_BINOP_GT: u8    = 0x0A;
const TAG_BINOP_GTEQ: u8  = 0x0B;
const TAG_BINOP_AND: u8   = 0x0C;
const TAG_BINOP_OR: u8    = 0x0D;
const TAG_BINOP_DOTDOT: u8= 0x0E;

// UnaryOp tags
const TAG_UNARY_NEG: u8 = 0x01;
const TAG_UNARY_NOT: u8 = 0x02;

// Pattern tags
const TAG_PAT_WILDCARD: u8    = 0xC0;
const TAG_PAT_IDENT: u8       = 0xC1;
const TAG_PAT_INT: u8         = 0xC2;
const TAG_PAT_FLOAT: u8       = 0xC3;
const TAG_PAT_BOOL: u8        = 0xC4;
const TAG_PAT_STRING: u8      = 0xC5;
const TAG_PAT_NIL: u8         = 0xC6;
const TAG_PAT_TUPLE: u8       = 0xC7;
const TAG_PAT_LIST: u8        = 0xC8;
const TAG_PAT_STRUCT: u8      = 0xC9;
const TAG_PAT_ENUMVARIANT: u8 = 0xCA;
const TAG_PAT_SOME: u8        = 0xCB;
const TAG_PAT_OK: u8          = 0xCC;
const TAG_PAT_ERR: u8         = 0xCD;
const TAG_PAT_OR: u8          = 0xCE;
const TAG_PAT_RANGE: u8       = 0xCF;

// StringPart tags
const TAG_STRPART_LITERAL: u8 = 0x01;
const TAG_STRPART_INTERP: u8  = 0x02;

// ═══════════════════════════════════════════════════════════
// Encoder
// ═══════════════════════════════════════════════════════════

pub struct Encoder {
    buf: Vec<u8>,
}

impl Encoder {
    pub fn new() -> Self { Encoder { buf: Vec::new() } }

    pub fn finish(self) -> Vec<u8> { self.buf }

    // ── Primitives ────────────────────────────────────────────────────────

    fn write_u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    fn write_u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn write_u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn write_u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn write_i64(&mut self, v: i64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn write_f64(&mut self, v: f64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn write_bool(&mut self, v: bool) {
        self.write_u8(if v { 1 } else { 0 });
    }

    fn write_str(&mut self, s: &str) {
        let bytes = s.as_bytes();
        self.write_u32(bytes.len() as u32);
        self.buf.extend_from_slice(bytes);
    }

    fn write_opt<T, F: Fn(&mut Self, &T)>(&mut self, opt: &Option<T>, f: F) {
        match opt {
            None    => self.write_u8(0),
            Some(v) => { self.write_u8(1); f(self, v); }
        }
    }

    fn write_vec<T, F: Fn(&mut Self, &T)>(&mut self, vec: &[T], f: F) {
        self.write_u32(vec.len() as u32);
        for item in vec { f(self, item); }
    }

    // ── Types ─────────────────────────────────────────────────────────────

    fn write_type(&mut self, ty: &Type) {
        match ty {
            Type::Int             => self.write_u8(TAG_TYPE_INT),
            Type::Float           => self.write_u8(TAG_TYPE_FLOAT),
            Type::Bool            => self.write_u8(TAG_TYPE_BOOL),
            Type::StringT         => self.write_u8(TAG_TYPE_STRING),
            Type::Nil             => self.write_u8(TAG_TYPE_NIL),
            Type::Inferred        => self.write_u8(TAG_TYPE_INFERRED),
            Type::Option(inner)   => { self.write_u8(TAG_TYPE_OPTION); self.write_type(inner); }
            Type::Result(ok, err) => { self.write_u8(TAG_TYPE_RESULT); self.write_type(ok); self.write_type(err); }
            Type::List(inner)     => { self.write_u8(TAG_TYPE_LIST); self.write_type(inner); }
            Type::Map(k, v)       => { self.write_u8(TAG_TYPE_MAP); self.write_type(k); self.write_type(v); }
            Type::Tuple(ts)       => { self.write_u8(TAG_TYPE_TUPLE); self.write_vec(ts, |e, t| e.write_type(t)); }
            Type::Named(name)     => { self.write_u8(TAG_TYPE_NAMED); self.write_str(name); }
            Type::Generic(name, args) => {
                self.write_u8(TAG_TYPE_GENERIC);
                self.write_str(name);
                self.write_vec(args, |e, t| e.write_type(t));
            }
            Type::Function(params, ret) => {
                self.write_u8(TAG_TYPE_FUNCTION);
                self.write_vec(params, |e, t| e.write_type(t));
                self.write_type(ret);
            }
        }
    }

    // ── Patterns ──────────────────────────────────────────────────────────

    fn write_pattern(&mut self, pat: &Pattern) {
        match pat {
            Pattern::Wildcard         => self.write_u8(TAG_PAT_WILDCARD),
            Pattern::Nil              => self.write_u8(TAG_PAT_NIL),
            Pattern::Bool(b)          => { self.write_u8(TAG_PAT_BOOL); self.write_bool(*b); }
            Pattern::Int(n)           => { self.write_u8(TAG_PAT_INT); self.write_i64(*n); }
            Pattern::Float(f)         => { self.write_u8(TAG_PAT_FLOAT); self.write_f64(*f); }
            Pattern::StringLit(s)     => { self.write_u8(TAG_PAT_STRING); self.write_str(s); }
            Pattern::Ident(name)      => { self.write_u8(TAG_PAT_IDENT); self.write_str(name); }
            Pattern::Tuple(pats)      => { self.write_u8(TAG_PAT_TUPLE); self.write_vec(pats, |e, p| e.write_pattern(p)); }
            Pattern::List(pats)       => { self.write_u8(TAG_PAT_LIST); self.write_vec(pats, |e, p| e.write_pattern(p)); }
            Pattern::Some(inner)      => { self.write_u8(TAG_PAT_SOME); self.write_pattern(inner); }
            Pattern::Ok(inner)        => { self.write_u8(TAG_PAT_OK); self.write_pattern(inner); }
            Pattern::Err(inner)       => { self.write_u8(TAG_PAT_ERR); self.write_pattern(inner); }
            Pattern::Or(a, b)         => { self.write_u8(TAG_PAT_OR); self.write_pattern(a); self.write_pattern(b); }
            Pattern::Range(lo, hi)    => { self.write_u8(TAG_PAT_RANGE); self.write_pattern(lo); self.write_pattern(hi); }
            Pattern::EnumVariant(en, var, fields) => {
                self.write_u8(TAG_PAT_ENUMVARIANT);
                self.write_str(en);
                self.write_str(var);
                self.write_vec(fields, |e, p| e.write_pattern(p));
            }
            Pattern::Struct(name, fields) => {
                self.write_u8(TAG_PAT_STRUCT);
                self.write_str(name);
                self.write_u32(fields.len() as u32);
                for (fname, fpat) in fields {
                    self.write_str(fname);
                    self.write_pattern(fpat);
                }
            }
        }
    }

    // ── String parts ──────────────────────────────────────────────────────

    fn write_string_part(&mut self, part: &StringPart) {
        match part {
            StringPart::Literal(s)       => { self.write_u8(TAG_STRPART_LITERAL); self.write_str(s); }
            StringPart::Interpolated(e)  => { self.write_u8(TAG_STRPART_INTERP); self.write_expr(e); }
        }
    }

    // ── Match arm ─────────────────────────────────────────────────────────

    fn write_match_arm(&mut self, arm: &MatchArm) {
        self.write_pattern(&arm.pattern);
        self.write_opt(&arm.guard, |e, g| e.write_expr(g));
        self.write_expr(&arm.body);
    }

    // ── Param ─────────────────────────────────────────────────────────────

    fn write_param(&mut self, param: &Param) {
        self.write_str(&param.name);
        self.write_opt(&param.ty, |e, t| e.write_type(t));
        self.write_opt(&param.default, |e, d| e.write_expr(d));
    }

    // ── BinOp / UnaryOp ───────────────────────────────────────────────────

    fn write_binop(&mut self, op: &BinOp) {
        let tag = match op {
            BinOp::Add    => TAG_BINOP_ADD,
            BinOp::Sub    => TAG_BINOP_SUB,
            BinOp::Mul    => TAG_BINOP_MUL,
            BinOp::Div    => TAG_BINOP_DIV,
            BinOp::Mod    => TAG_BINOP_MOD,
            BinOp::Eq     => TAG_BINOP_EQ,
            BinOp::NotEq  => TAG_BINOP_NEQ,
            BinOp::Lt     => TAG_BINOP_LT,
            BinOp::LtEq   => TAG_BINOP_LTEQ,
            BinOp::Gt     => TAG_BINOP_GT,
            BinOp::GtEq   => TAG_BINOP_GTEQ,
            BinOp::And    => TAG_BINOP_AND,
            BinOp::Or     => TAG_BINOP_OR,
            BinOp::DotDot => TAG_BINOP_DOTDOT,
        };
        self.write_u8(tag);
    }

    fn write_unaryop(&mut self, op: &UnaryOp) {
        self.write_u8(match op { UnaryOp::Neg => TAG_UNARY_NEG, UnaryOp::Not => TAG_UNARY_NOT });
    }

    // ── Expressions ───────────────────────────────────────────────────────

    pub fn write_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Int(n)    => { self.write_u8(TAG_EXPR_INT); self.write_i64(*n); }
            Expr::Float(f)  => { self.write_u8(TAG_EXPR_FLOAT); self.write_f64(*f); }
            Expr::Bool(b)   => { self.write_u8(TAG_EXPR_BOOL); self.write_bool(*b); }
            Expr::Nil       => self.write_u8(TAG_EXPR_NIL),
            Expr::StringLit(s) => { self.write_u8(TAG_EXPR_STRING); self.write_str(s); }

            Expr::InterpolatedString(parts) => {
                self.write_u8(TAG_EXPR_INTERP);
                self.write_vec(parts, |e, p| e.write_string_part(p));
            }

            Expr::Var(name) => { self.write_u8(TAG_EXPR_VAR); self.write_str(name); }

            Expr::Tuple(elems) => {
                self.write_u8(TAG_EXPR_TUPLE);
                self.write_vec(elems, |e, x| e.write_expr(x));
            }
            Expr::List(elems) => {
                self.write_u8(TAG_EXPR_LIST);
                self.write_vec(elems, |e, x| e.write_expr(x));
            }
            Expr::MapLit(pairs) => {
                self.write_u8(TAG_EXPR_MAPLIT);
                self.write_u32(pairs.len() as u32);
                for (k, v) in pairs { self.write_expr(k); self.write_expr(v); }
            }
            Expr::Block(stmts, tail) => {
                self.write_u8(TAG_EXPR_BLOCK);
                self.write_vec(stmts, |e, s| e.write_stmt(s));
                self.write_opt(tail, |e, t| e.write_expr(t));
            }
            Expr::BinOp(l, op, r) => {
                self.write_u8(TAG_EXPR_BINOP);
                self.write_expr(l);
                self.write_binop(op);
                self.write_expr(r);
            }
            Expr::UnaryOp(op, inner) => {
                self.write_u8(TAG_EXPR_UNARYOP);
                self.write_unaryop(op);
                self.write_expr(inner);
            }
            Expr::Call(callee, args) => {
                self.write_u8(TAG_EXPR_CALL);
                self.write_expr(callee);
                self.write_vec(args, |e, a| e.write_expr(a));
            }
            Expr::MethodCall(obj, method, args) => {
                self.write_u8(TAG_EXPR_METHODCALL);
                self.write_expr(obj);
                self.write_str(method);
                self.write_vec(args, |e, a| e.write_expr(a));
            }
            Expr::FieldAccess(obj, field) => {
                self.write_u8(TAG_EXPR_FIELDACCESS);
                self.write_expr(obj);
                self.write_str(field);
            }
            Expr::Index(obj, idx) => {
                self.write_u8(TAG_EXPR_INDEX);
                self.write_expr(obj);
                self.write_expr(idx);
            }
            Expr::If(cond, then, elifs, else_) => {
                self.write_u8(TAG_EXPR_IF);
                self.write_expr(cond);
                self.write_expr(then);
                self.write_u32(elifs.len() as u32);
                for (c, b) in elifs { self.write_expr(c); self.write_expr(b); }
                self.write_opt(else_, |e, x| e.write_expr(x));
            }
            Expr::Match(subj, arms) => {
                self.write_u8(TAG_EXPR_MATCH);
                self.write_expr(subj);
                self.write_vec(arms, |e, a| e.write_match_arm(a));
            }
            Expr::Closure(params, body) => {
                self.write_u8(TAG_EXPR_CLOSURE);
                self.write_u32(params.len() as u32);
                for (name, ty) in params {
                    self.write_str(name);
                    self.write_opt(ty, |e, t| e.write_type(t));
                }
                self.write_expr(body);
            }
            Expr::StructCreate(name, fields) => {
                self.write_u8(TAG_EXPR_STRUCTCREATE);
                self.write_str(name);
                self.write_u32(fields.len() as u32);
                for (fname, fval) in fields { self.write_str(fname); self.write_expr(fval); }
            }
            Expr::EnumVariant(en, var, args) => {
                self.write_u8(TAG_EXPR_ENUMVARIANT);
                self.write_str(en);
                self.write_str(var);
                self.write_vec(args, |e, a| e.write_expr(a));
            }
            Expr::Range(start, end) => {
                self.write_u8(TAG_EXPR_RANGE);
                self.write_expr(start);
                self.write_expr(end);
            }
            Expr::Some(inner)     => { self.write_u8(TAG_EXPR_SOME); self.write_expr(inner); }
            Expr::Ok(inner)       => { self.write_u8(TAG_EXPR_OK); self.write_expr(inner); }
            Expr::Err(inner)      => { self.write_u8(TAG_EXPR_ERR); self.write_expr(inner); }
            Expr::Question(inner) => { self.write_u8(TAG_EXPR_QUESTION); self.write_expr(inner); }
            Expr::BoxExpr(inner)  => { self.write_u8(TAG_EXPR_BOX); self.write_expr(inner); }
            Expr::RefExpr(inner)  => { self.write_u8(TAG_EXPR_REF); self.write_expr(inner); }
            Expr::Assign(target, val) => {
                self.write_u8(TAG_EXPR_ASSIGN);
                self.write_expr(target);
                self.write_expr(val);
            }
            Expr::Await(inner) => { self.write_u8(TAG_EXPR_AWAIT); self.write_expr(inner); }
        }
    }

    // ── Statements ────────────────────────────────────────────────────────

    pub fn write_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let(name, ty, val, mutable) => {
                self.write_u8(TAG_STMT_LET);
                self.write_str(name);
                self.write_opt(ty, |e, t| e.write_type(t));
                self.write_expr(val);
                self.write_bool(*mutable);
            }
            Stmt::Expr(expr) => {
                self.write_u8(TAG_STMT_EXPR);
                self.write_expr(expr);
            }
            Stmt::Return(expr) => {
                self.write_u8(TAG_STMT_RETURN);
                self.write_opt(expr, |e, x| e.write_expr(x));
            }
            Stmt::Break    => self.write_u8(TAG_STMT_BREAK),
            Stmt::Continue => self.write_u8(TAG_STMT_CONTINUE),
            Stmt::While(cond, body) => {
                self.write_u8(TAG_STMT_WHILE);
                self.write_expr(cond);
                self.write_vec(body, |e, s| e.write_stmt(s));
            }
            Stmt::For(var, iter, body) => {
                self.write_u8(TAG_STMT_FOR);
                self.write_str(var);
                self.write_expr(iter);
                self.write_vec(body, |e, s| e.write_stmt(s));
            }
            Stmt::FunDef(f) => {
                self.write_u8(TAG_STMT_FUNDEF);
                self.write_fundef(f);
            }
            Stmt::StructDef(s) => {
                self.write_u8(TAG_STMT_STRUCTDEF);
                self.write_structdef(s);
            }
            Stmt::EnumDef(en) => {
                self.write_u8(TAG_STMT_ENUMDEF);
                self.write_enumdef(en);
            }
            Stmt::ImplBlock(ib) => {
                self.write_u8(TAG_STMT_IMPLBLOCK);
                self.write_implblock(ib);
            }
            Stmt::ModDef(name, stmts) => {
                self.write_u8(TAG_STMT_MODDEF);
                self.write_str(name);
                self.write_vec(stmts, |e, s| e.write_stmt(s));
            }
            Stmt::Import(path) => {
                self.write_u8(TAG_STMT_IMPORT);
                self.write_vec(path, |e, s| e.write_str(s));
            }
            Stmt::TypeAlias(name, generics, ty) => {
                self.write_u8(TAG_STMT_TYPEALIAS);
                self.write_str(name);
                self.write_vec(generics, |e, s| e.write_str(s));
                self.write_type(ty);
            }
        }
    }

    fn write_fundef(&mut self, f: &FunDef) {
        self.write_str(&f.name);
        self.write_vec(&f.generics, |e, s| e.write_str(s));
        self.write_vec(&f.params, |e, p| e.write_param(p));
        self.write_opt(&f.return_type, |e, t| e.write_type(t));
        self.write_vec(&f.body, |e, s| e.write_stmt(s));
        self.write_bool(f.is_pub);
    }

    fn write_structdef(&mut self, s: &StructDef) {
        self.write_str(&s.name);
        self.write_vec(&s.generics, |e, g| e.write_str(g));
        self.write_u32(s.fields.len() as u32);
        for field in &s.fields {
            self.write_str(&field.name);
            self.write_type(&field.ty);
            self.write_bool(field.is_pub);
        }
        self.write_bool(s.is_pub);
    }

    fn write_enumdef(&mut self, en: &EnumDef) {
        self.write_str(&en.name);
        self.write_vec(&en.generics, |e, g| e.write_str(g));
        self.write_u32(en.variants.len() as u32);
        for v in &en.variants {
            self.write_str(&v.name);
            self.write_vec(&v.fields, |e, t| e.write_type(t));
        }
        self.write_bool(en.is_pub);
    }

    fn write_implblock(&mut self, ib: &ImplBlock) {
        self.write_str(&ib.target);
        self.write_vec(&ib.generics, |e, g| e.write_str(g));
        self.write_vec(&ib.methods, |e, m| e.write_fundef(m));
    }
}

// ═══════════════════════════════════════════════════════════
// Decoder
// ═══════════════════════════════════════════════════════════

pub struct Decoder<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Decoder<'a> {
    pub fn new(data: &'a [u8]) -> Self { Decoder { data, pos: 0 } }

    // ── Primitives ────────────────────────────────────────────────────────

    fn read_u8(&mut self) -> io::Result<u8> {
        if self.pos >= self.data.len() {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "unexpected EOF"));
        }
        let v = self.data[self.pos];
        self.pos += 1;
        Ok(v)
    }

    fn read_u16(&mut self) -> io::Result<u16> {
        let b = self.read_bytes(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn read_u32(&mut self) -> io::Result<u32> {
        let b = self.read_bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn read_u64(&mut self) -> io::Result<u64> {
        let b = self.read_bytes(8)?;
        Ok(u64::from_le_bytes(b.try_into().unwrap()))
    }

    fn read_i64(&mut self) -> io::Result<i64> {
        let b = self.read_bytes(8)?;
        Ok(i64::from_le_bytes(b.try_into().unwrap()))
    }

    fn read_f64(&mut self) -> io::Result<f64> {
        let b = self.read_bytes(8)?;
        Ok(f64::from_le_bytes(b.try_into().unwrap()))
    }

    fn read_bool(&mut self) -> io::Result<bool> {
        Ok(self.read_u8()? != 0)
    }

    fn read_str(&mut self) -> io::Result<String> {
        let len = self.read_u32()? as usize;
        let bytes = self.read_bytes(len)?;
        String::from_utf8(bytes.to_vec())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    fn read_bytes(&mut self, n: usize) -> io::Result<&'a [u8]> {
        if self.pos + n > self.data.len() {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "unexpected EOF"));
        }
        let slice = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    fn read_opt<T, F: Fn(&mut Self) -> io::Result<T>>(&mut self, f: F) -> io::Result<Option<T>> {
        match self.read_u8()? {
            0 => Ok(None),
            _ => Ok(Some(f(self)?)),
        }
    }

    fn read_vec<T, F: Fn(&mut Self) -> io::Result<T>>(&mut self, f: F) -> io::Result<Vec<T>> {
        let count = self.read_u32()? as usize;
        let mut v = Vec::with_capacity(count);
        for _ in 0..count { v.push(f(self)?); }
        Ok(v)
    }

    // ── Types ─────────────────────────────────────────────────────────────

    fn read_type(&mut self) -> io::Result<Type> {
        Ok(match self.read_u8()? {
            TAG_TYPE_INT      => Type::Int,
            TAG_TYPE_FLOAT    => Type::Float,
            TAG_TYPE_BOOL     => Type::Bool,
            TAG_TYPE_STRING   => Type::StringT,
            TAG_TYPE_NIL      => Type::Nil,
            TAG_TYPE_INFERRED => Type::Inferred,
            TAG_TYPE_OPTION   => Type::Option(Box::new(self.read_type()?)),
            TAG_TYPE_RESULT   => {
                let ok = self.read_type()?;
                let err = self.read_type()?;
                Type::Result(Box::new(ok), Box::new(err))
            }
            TAG_TYPE_LIST  => Type::List(Box::new(self.read_type()?)),
            TAG_TYPE_MAP   => {
                let k = self.read_type()?;
                let v = self.read_type()?;
                Type::Map(Box::new(k), Box::new(v))
            }
            TAG_TYPE_TUPLE   => Type::Tuple(self.read_vec(|d| d.read_type())?),
            TAG_TYPE_NAMED   => Type::Named(self.read_str()?),
            TAG_TYPE_GENERIC => {
                let name = self.read_str()?;
                let args = self.read_vec(|d| d.read_type())?;
                Type::Generic(name, args)
            }
            TAG_TYPE_FUNCTION => {
                let params = self.read_vec(|d| d.read_type())?;
                let ret = self.read_type()?;
                Type::Function(params, Box::new(ret))
            }
            tag => return Err(io::Error::new(io::ErrorKind::InvalidData, format!("unknown type tag: 0x{:02X}", tag)))
        })
    }

    // ── Patterns ──────────────────────────────────────────────────────────

    fn read_pattern(&mut self) -> io::Result<Pattern> {
        Ok(match self.read_u8()? {
            TAG_PAT_WILDCARD => Pattern::Wildcard,
            TAG_PAT_NIL      => Pattern::Nil,
            TAG_PAT_BOOL     => Pattern::Bool(self.read_bool()?),
            TAG_PAT_INT      => Pattern::Int(self.read_i64()?),
            TAG_PAT_FLOAT    => Pattern::Float(self.read_f64()?),
            TAG_PAT_STRING   => Pattern::StringLit(self.read_str()?),
            TAG_PAT_IDENT    => Pattern::Ident(self.read_str()?),
            TAG_PAT_TUPLE    => Pattern::Tuple(self.read_vec(|d| d.read_pattern())?),
            TAG_PAT_LIST     => Pattern::List(self.read_vec(|d| d.read_pattern())?),
            TAG_PAT_SOME     => Pattern::Some(Box::new(self.read_pattern()?)),
            TAG_PAT_OK       => Pattern::Ok(Box::new(self.read_pattern()?)),
            TAG_PAT_ERR      => Pattern::Err(Box::new(self.read_pattern()?)),
            TAG_PAT_OR       => {
                let a = self.read_pattern()?;
                let b = self.read_pattern()?;
                Pattern::Or(Box::new(a), Box::new(b))
            }
            TAG_PAT_RANGE => {
                let lo = self.read_pattern()?;
                let hi = self.read_pattern()?;
                Pattern::Range(Box::new(lo), Box::new(hi))
            }
            TAG_PAT_ENUMVARIANT => {
                let en = self.read_str()?;
                let var = self.read_str()?;
                let fields = self.read_vec(|d| d.read_pattern())?;
                Pattern::EnumVariant(en, var, fields)
            }
            TAG_PAT_STRUCT => {
                let name = self.read_str()?;
                let count = self.read_u32()? as usize;
                let mut fields = Vec::new();
                for _ in 0..count {
                    let fname = self.read_str()?;
                    let fpat = self.read_pattern()?;
                    fields.push((fname, fpat));
                }
                Pattern::Struct(name, fields)
            }
            tag => return Err(io::Error::new(io::ErrorKind::InvalidData, format!("unknown pattern tag: 0x{:02X}", tag)))
        })
    }

    // ── String parts ──────────────────────────────────────────────────────

    fn read_string_part(&mut self) -> io::Result<StringPart> {
        Ok(match self.read_u8()? {
            TAG_STRPART_LITERAL => StringPart::Literal(self.read_str()?),
            TAG_STRPART_INTERP  => StringPart::Interpolated(self.read_expr()?),
            tag => return Err(io::Error::new(io::ErrorKind::InvalidData, format!("unknown string part tag: 0x{:02X}", tag)))
        })
    }

    // ── Match arm ─────────────────────────────────────────────────────────

    fn read_match_arm(&mut self) -> io::Result<MatchArm> {
        let pattern = self.read_pattern()?;
        let guard = self.read_opt(|d| d.read_expr())?;
        let body = self.read_expr()?;
        Ok(MatchArm { pattern, guard, body })
    }

    // ── Param ─────────────────────────────────────────────────────────────

    fn read_param(&mut self) -> io::Result<Param> {
        let name = self.read_str()?;
        let ty = self.read_opt(|d| d.read_type())?;
        let default = self.read_opt(|d| d.read_expr())?;
        Ok(Param { name, ty, default })
    }

    // ── BinOp / UnaryOp ───────────────────────────────────────────────────

    fn read_binop(&mut self) -> io::Result<BinOp> {
        Ok(match self.read_u8()? {
            TAG_BINOP_ADD    => BinOp::Add,
            TAG_BINOP_SUB    => BinOp::Sub,
            TAG_BINOP_MUL    => BinOp::Mul,
            TAG_BINOP_DIV    => BinOp::Div,
            TAG_BINOP_MOD    => BinOp::Mod,
            TAG_BINOP_EQ     => BinOp::Eq,
            TAG_BINOP_NEQ    => BinOp::NotEq,
            TAG_BINOP_LT     => BinOp::Lt,
            TAG_BINOP_LTEQ   => BinOp::LtEq,
            TAG_BINOP_GT     => BinOp::Gt,
            TAG_BINOP_GTEQ   => BinOp::GtEq,
            TAG_BINOP_AND    => BinOp::And,
            TAG_BINOP_OR     => BinOp::Or,
            TAG_BINOP_DOTDOT => BinOp::DotDot,
            tag => return Err(io::Error::new(io::ErrorKind::InvalidData, format!("unknown binop tag: 0x{:02X}", tag)))
        })
    }

    fn read_unaryop(&mut self) -> io::Result<UnaryOp> {
        Ok(match self.read_u8()? {
            TAG_UNARY_NEG => UnaryOp::Neg,
            TAG_UNARY_NOT => UnaryOp::Not,
            tag => return Err(io::Error::new(io::ErrorKind::InvalidData, format!("unknown unaryop tag: 0x{:02X}", tag)))
        })
    }

    // ── Expressions ───────────────────────────────────────────────────────

    pub fn read_expr(&mut self) -> io::Result<Expr> {
        Ok(match self.read_u8()? {
            TAG_EXPR_INT    => Expr::Int(self.read_i64()?),
            TAG_EXPR_FLOAT  => Expr::Float(self.read_f64()?),
            TAG_EXPR_BOOL   => Expr::Bool(self.read_bool()?),
            TAG_EXPR_NIL    => Expr::Nil,
            TAG_EXPR_STRING => Expr::StringLit(self.read_str()?),
            TAG_EXPR_INTERP => Expr::InterpolatedString(self.read_vec(|d| d.read_string_part())?),
            TAG_EXPR_VAR    => Expr::Var(self.read_str()?),
            TAG_EXPR_TUPLE  => Expr::Tuple(self.read_vec(|d| d.read_expr())?),
            TAG_EXPR_LIST   => Expr::List(self.read_vec(|d| d.read_expr())?),
            TAG_EXPR_MAPLIT => {
                let count = self.read_u32()? as usize;
                let mut pairs = Vec::new();
                for _ in 0..count { pairs.push((self.read_expr()?, self.read_expr()?)); }
                Expr::MapLit(pairs)
            }
            TAG_EXPR_BLOCK => {
                let stmts = self.read_vec(|d| d.read_stmt())?;
                let tail = self.read_opt(|d| d.read_expr())?;
                Expr::Block(stmts, tail.map(Box::new))
            }
            TAG_EXPR_BINOP => {
                let l = self.read_expr()?;
                let op = self.read_binop()?;
                let r = self.read_expr()?;
                Expr::BinOp(Box::new(l), op, Box::new(r))
            }
            TAG_EXPR_UNARYOP => {
                let op = self.read_unaryop()?;
                let inner = self.read_expr()?;
                Expr::UnaryOp(op, Box::new(inner))
            }
            TAG_EXPR_CALL => {
                let callee = self.read_expr()?;
                let args = self.read_vec(|d| d.read_expr())?;
                Expr::Call(Box::new(callee), args)
            }
            TAG_EXPR_METHODCALL => {
                let obj = self.read_expr()?;
                let method = self.read_str()?;
                let args = self.read_vec(|d| d.read_expr())?;
                Expr::MethodCall(Box::new(obj), method, args)
            }
            TAG_EXPR_FIELDACCESS => {
                let obj = self.read_expr()?;
                let field = self.read_str()?;
                Expr::FieldAccess(Box::new(obj), field)
            }
            TAG_EXPR_INDEX => {
                let obj = self.read_expr()?;
                let idx = self.read_expr()?;
                Expr::Index(Box::new(obj), Box::new(idx))
            }
            TAG_EXPR_IF => {
                let cond = self.read_expr()?;
                let then = self.read_expr()?;
                let elif_count = self.read_u32()? as usize;
                let mut elifs = Vec::new();
                for _ in 0..elif_count { elifs.push((self.read_expr()?, self.read_expr()?)); }
                let else_ = self.read_opt(|d| d.read_expr())?;
                Expr::If(Box::new(cond), Box::new(then), elifs, else_.map(Box::new))
            }
            TAG_EXPR_MATCH => {
                let subj = self.read_expr()?;
                let arms = self.read_vec(|d| d.read_match_arm())?;
                Expr::Match(Box::new(subj), arms)
            }
            TAG_EXPR_CLOSURE => {
                let count = self.read_u32()? as usize;
                let mut params = Vec::new();
                for _ in 0..count {
                    let name = self.read_str()?;
                    let ty = self.read_opt(|d| d.read_type())?;
                    params.push((name, ty));
                }
                let body = self.read_expr()?;
                Expr::Closure(params, Box::new(body))
            }
            TAG_EXPR_STRUCTCREATE => {
                let name = self.read_str()?;
                let count = self.read_u32()? as usize;
                let mut fields = Vec::new();
                for _ in 0..count { fields.push((self.read_str()?, self.read_expr()?)); }
                Expr::StructCreate(name, fields)
            }
            TAG_EXPR_ENUMVARIANT => {
                let en = self.read_str()?;
                let var = self.read_str()?;
                let args = self.read_vec(|d| d.read_expr())?;
                Expr::EnumVariant(en, var, args)
            }
            TAG_EXPR_RANGE => {
                let s = self.read_expr()?;
                let e = self.read_expr()?;
                Expr::Range(Box::new(s), Box::new(e))
            }
            TAG_EXPR_SOME     => Expr::Some(Box::new(self.read_expr()?)),
            TAG_EXPR_OK       => Expr::Ok(Box::new(self.read_expr()?)),
            TAG_EXPR_ERR      => Expr::Err(Box::new(self.read_expr()?)),
            TAG_EXPR_QUESTION => Expr::Question(Box::new(self.read_expr()?)),
            TAG_EXPR_BOX      => Expr::BoxExpr(Box::new(self.read_expr()?)),
            TAG_EXPR_REF      => Expr::RefExpr(Box::new(self.read_expr()?)),
            TAG_EXPR_ASSIGN   => {
                let t = self.read_expr()?;
                let v = self.read_expr()?;
                Expr::Assign(Box::new(t), Box::new(v))
            }
            TAG_EXPR_AWAIT => Expr::Await(Box::new(self.read_expr()?)),
            tag => return Err(io::Error::new(io::ErrorKind::InvalidData, format!("unknown expr tag: 0x{:02X}", tag)))
        })
    }

    // ── Statements ────────────────────────────────────────────────────────

    pub fn read_stmt(&mut self) -> io::Result<Stmt> {
        Ok(match self.read_u8()? {
            TAG_STMT_LET => {
                let name = self.read_str()?;
                let ty = self.read_opt(|d| d.read_type())?;
                let val = self.read_expr()?;
                let mutable = self.read_bool()?;
                Stmt::Let(name, ty, val, mutable)
            }
            TAG_STMT_EXPR     => Stmt::Expr(self.read_expr()?),
            TAG_STMT_RETURN   => Stmt::Return(self.read_opt(|d| d.read_expr())?),
            TAG_STMT_BREAK    => Stmt::Break,
            TAG_STMT_CONTINUE => Stmt::Continue,
            TAG_STMT_WHILE => {
                let cond = self.read_expr()?;
                let body = self.read_vec(|d| d.read_stmt())?;
                Stmt::While(cond, body)
            }
            TAG_STMT_FOR => {
                let var = self.read_str()?;
                let iter = self.read_expr()?;
                let body = self.read_vec(|d| d.read_stmt())?;
                Stmt::For(var, iter, body)
            }
            TAG_STMT_FUNDEF    => Stmt::FunDef(self.read_fundef()?),
            TAG_STMT_STRUCTDEF => Stmt::StructDef(self.read_structdef()?),
            TAG_STMT_ENUMDEF   => Stmt::EnumDef(self.read_enumdef()?),
            TAG_STMT_IMPLBLOCK => Stmt::ImplBlock(self.read_implblock()?),
            TAG_STMT_MODDEF => {
                let name = self.read_str()?;
                let stmts = self.read_vec(|d| d.read_stmt())?;
                Stmt::ModDef(name, stmts)
            }
            TAG_STMT_IMPORT    => Stmt::Import(self.read_vec(|d| d.read_str())?),
            TAG_STMT_TYPEALIAS => {
                let name = self.read_str()?;
                let generics = self.read_vec(|d| d.read_str())?;
                let ty = self.read_type()?;
                Stmt::TypeAlias(name, generics, ty)
            }
            tag => return Err(io::Error::new(io::ErrorKind::InvalidData, format!("unknown stmt tag: 0x{:02X}", tag)))
        })
    }

    fn read_fundef(&mut self) -> io::Result<FunDef> {
        let name = self.read_str()?;
        let generics = self.read_vec(|d| d.read_str())?;
        let params = self.read_vec(|d| d.read_param())?;
        let return_type = self.read_opt(|d| d.read_type())?;
        let body = self.read_vec(|d| d.read_stmt())?;
        let is_pub = self.read_bool()?;
        Ok(FunDef { name, generics, params, return_type, body, is_pub })
    }

    fn read_structdef(&mut self) -> io::Result<StructDef> {
        let name = self.read_str()?;
        let generics = self.read_vec(|d| d.read_str())?;
        let count = self.read_u32()? as usize;
        let mut fields = Vec::new();
        for _ in 0..count {
            let fname = self.read_str()?;
            let fty = self.read_type()?;
            let is_pub = self.read_bool()?;
            fields.push(crate::ast::StructField { name: fname, ty: fty, is_pub });
        }
        let is_pub = self.read_bool()?;
        Ok(StructDef { name, generics, fields, is_pub })
    }

    fn read_enumdef(&mut self) -> io::Result<EnumDef> {
        let name = self.read_str()?;
        let generics = self.read_vec(|d| d.read_str())?;
        let count = self.read_u32()? as usize;
        let mut variants = Vec::new();
        for _ in 0..count {
            let vname = self.read_str()?;
            let vfields = self.read_vec(|d| d.read_type())?;
            variants.push(crate::ast::EnumVariant { name: vname, fields: vfields });
        }
        let is_pub = self.read_bool()?;
        Ok(EnumDef { name, generics, variants, is_pub })
    }

    fn read_implblock(&mut self) -> io::Result<ImplBlock> {
        let target = self.read_str()?;
        let generics = self.read_vec(|d| d.read_str())?;
        let methods = self.read_vec(|d| d.read_fundef())?;
        Ok(ImplBlock { target, generics, methods })
    }
}

// ═══════════════════════════════════════════════════════════
// Public API — write/read .zphc files
// ═══════════════════════════════════════════════════════════

/// FNV-1a 64-bit hash — for detecting stale bytecode
fn fnv1a(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000000001b3);
    }
    hash
}

/// Serialize a parsed AST to .zphc bytes.
pub fn encode(stmts: &[Stmt], source: &str) -> Vec<u8> {
    let mut out = Vec::new();

    // Header
    out.extend_from_slice(&MAGIC.to_le_bytes());
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&fnv1a(source.as_bytes()).to_le_bytes());
    out.extend_from_slice(&(stmts.len() as u32).to_le_bytes());

    // Body
    let mut enc = Encoder::new();
    for stmt in stmts { enc.write_stmt(stmt); }
    out.extend_from_slice(&enc.finish());

    out
}

/// Decode .zphc bytes back to an AST.
/// Returns (stmts, source_hash).
pub fn decode(data: &[u8]) -> Result<(Vec<Stmt>, u64), String> {
    if data.len() < 18 {
        return Err("File too short to be a valid .zphc".into());
    }

    // Check magic
    let magic = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    if magic != MAGIC {
        return Err(format!("Invalid magic: expected 0x{:08X}, got 0x{:08X}", MAGIC, magic));
    }

    // Check version
    let version = u16::from_le_bytes([data[4], data[5]]);
    if version != VERSION {
        return Err(format!("Unsupported bytecode version: {} (this runtime supports {})", version, VERSION));
    }

    let source_hash = u64::from_le_bytes(data[6..14].try_into().unwrap());
    let stmt_count = u32::from_le_bytes([data[14], data[15], data[16], data[17]]) as usize;

    let mut dec = Decoder::new(&data[18..]);
    let mut stmts = Vec::with_capacity(stmt_count);
    for _ in 0..stmt_count {
        stmts.push(dec.read_stmt().map_err(|e| format!("Decode error: {}", e))?);
    }

    Ok((stmts, source_hash))
}

/// Check if a .zphc file is still valid for the given source.
pub fn is_fresh(bytecode: &[u8], source: &str) -> bool {
    if bytecode.len() < 14 { return false; }
    let stored_hash = u64::from_le_bytes(bytecode[6..14].try_into().unwrap());
    stored_hash == fnv1a(source.as_bytes())
}