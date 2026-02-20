// Zephyr Abstract Syntax Tree

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Int,
    Float,
    Bool,
    StringT,
    Nil,
    Option(Box<Type>),
    Result(Box<Type>, Box<Type>),
    List(Box<Type>),
    Map(Box<Type>, Box<Type>),
    Tuple(Vec<Type>),
    Named(String),
    Generic(String, Vec<Type>),
    Function(Vec<Type>, Box<Type>),
    Inferred,
}

#[derive(Debug, Clone)]
pub enum Expr {
    // Literals
    Int(i64),
    Float(f64),
    Bool(bool),
    StringLit(String),
    Nil,

    // Interpolated string: parts alternate text and expr
    InterpolatedString(Vec<StringPart>),

    // Variable
    Var(String),

    // Tuple
    Tuple(Vec<Expr>),

    // List literal
    List(Vec<Expr>),

    // Map literal
    MapLit(Vec<(Expr, Expr)>),

    // Block expression
    Block(Vec<Stmt>, Option<Box<Expr>>),

    // Binary operations
    BinOp(Box<Expr>, BinOp, Box<Expr>),

    // Unary operations
    UnaryOp(UnaryOp, Box<Expr>),

    // Function call
    Call(Box<Expr>, Vec<Expr>),

    // Method call: obj.method(args)
    MethodCall(Box<Expr>, String, Vec<Expr>),

    // Field access: obj.field
    FieldAccess(Box<Expr>, String),

    // Index: obj[idx]
    Index(Box<Expr>, Box<Expr>),

    // If expression
    If(Box<Expr>, Box<Expr>, Vec<(Expr, Expr)>, Option<Box<Expr>>),

    // Match expression
    Match(Box<Expr>, Vec<MatchArm>),

    // Closure: |args| => expr  or  |args| { body }
    Closure(Vec<(String, Option<Type>)>, Box<Expr>),

    // Struct creation: MyStruct { field: val, ... }
    StructCreate(String, Vec<(String, Expr)>),

    // Enum variant: MyEnum::Variant(args)
    EnumVariant(String, String, Vec<Expr>),

    // Range: start..end
    Range(Box<Expr>, Box<Expr>),

    // Option: Some(x) / nil
    Some(Box<Expr>),

    // Result: Ok(x) / Err(x)
    Ok(Box<Expr>),
    Err(Box<Expr>),

    // Question mark unwrap operator (like Rust's ?)
    Question(Box<Expr>),

    // box expr — heap allocation hint
    BoxExpr(Box<Expr>),

    // ref expr — reference
    RefExpr(Box<Expr>),

    // Assignment expression (for var reassignment)
    Assign(Box<Expr>, Box<Expr>),

    // Await (future use)
    Await(Box<Expr>),
}

#[derive(Debug, Clone)]
pub enum StringPart {
    Literal(String),
    Interpolated(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinOp {
    Add, Sub, Mul, Div, Mod,
    Eq, NotEq, Lt, LtEq, Gt, GtEq,
    And, Or,
    DotDot, // range
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: Expr,
}

#[derive(Debug, Clone)]
pub enum Pattern {
    Wildcard,
    Ident(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    StringLit(String),
    Nil,
    Tuple(Vec<Pattern>),
    List(Vec<Pattern>),
    Struct(String, Vec<(String, Pattern)>),
    EnumVariant(String, String, Vec<Pattern>),
    Some(Box<Pattern>),
    Ok(Box<Pattern>),
    Err(Box<Pattern>),
    Or(Box<Pattern>, Box<Pattern>),
    Range(Box<Pattern>, Box<Pattern>),
}

#[derive(Debug, Clone)]
pub enum Stmt {
    // let x: Type = expr
    Let(String, Option<Type>, Expr, bool), // (name, type, value, is_mutable)

    // Expression statement
    Expr(Expr),

    // return expr
    Return(Option<Expr>),

    // break / continue
    Break,
    Continue,

    // while cond { body }
    While(Expr, Vec<Stmt>),

    // for x in iter { body }
    For(String, Expr, Vec<Stmt>),

    // Function definition
    FunDef(FunDef),

    // Struct definition
    StructDef(StructDef),

    // Enum definition
    EnumDef(EnumDef),

    // impl block
    ImplBlock(ImplBlock),

    // Module
    ModDef(String, Vec<Stmt>),

    // Import
    Import(Vec<String>),

    // Type alias
    TypeAlias(String, Vec<String>, Type),
}

#[derive(Debug, Clone)]
pub struct FunDef {
    pub name: String,
    pub generics: Vec<String>,
    pub params: Vec<Param>,
    pub return_type: Option<Type>,
    pub body: Vec<Stmt>,
    pub is_pub: bool,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: Option<Type>,
    pub default: Option<Expr>,
}

#[derive(Debug, Clone)]
pub struct StructDef {
    pub name: String,
    pub generics: Vec<String>,
    pub fields: Vec<StructField>,
    pub is_pub: bool,
}

#[derive(Debug, Clone)]
pub struct StructField {
    pub name: String,
    pub ty: Type,
    pub is_pub: bool,
}

#[derive(Debug, Clone)]
pub struct EnumDef {
    pub name: String,
    pub generics: Vec<String>,
    pub variants: Vec<EnumVariant>,
    pub is_pub: bool,
}

#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub name: String,
    pub fields: Vec<Type>,
}

#[derive(Debug, Clone)]
pub struct ImplBlock {
    pub target: String,
    pub generics: Vec<String>,
    pub methods: Vec<FunDef>,
}