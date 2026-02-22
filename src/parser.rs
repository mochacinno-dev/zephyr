// ═══════════════════════════════════════════════════════════
// Zephyr Parser — turns tokens into AST
// ═══════════════════════════════════════════════════════════

use crate::lexer::{Token, TokenWithSpan};
use crate::ast::*;

pub struct Parser {
    tokens: Vec<TokenWithSpan>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<TokenWithSpan>) -> Self {
        Parser { tokens, pos: 0 }
    }

    // ── Token navigation ──────────────────────────────────────────────────────

    fn peek(&self) -> &Token {
        &self.tokens[self.pos].token
    }

    fn peek2(&self) -> &Token {
        self.tokens.get(self.pos + 1).map(|t| &t.token).unwrap_or(&Token::Eof)
    }

    fn span_line(&self) -> usize {
        self.tokens[self.pos].span.line
    }

    fn advance(&mut self) -> &Token {
        let t = &self.tokens[self.pos].token;
        if self.pos + 1 < self.tokens.len() { self.pos += 1; }
        t
    }

    fn skip_newlines(&mut self) {
        while self.peek() == &Token::Newline { self.advance(); }
    }

    fn expect(&mut self, expected: &Token) -> Result<(), String> {
        if self.peek() == expected {
            self.advance();
            Ok(())
        } else {
            Err(format!("Expected {:?}, got {:?} at line {}", expected, self.peek(), self.span_line()))
        }
    }

    fn check(&self, t: &Token) -> bool { self.peek() == t }

    fn eat(&mut self, t: &Token) -> bool {
        if self.peek() == t { self.advance(); true } else { false }
    }

    fn eat_newlines(&mut self) {
        while matches!(self.peek(), Token::Newline | Token::Semicolon) { self.advance(); }
    }

    // ── Top-level parsing ─────────────────────────────────────────────────────

    pub fn parse_program(&mut self) -> Result<Vec<Stmt>, String> {
        let mut stmts = Vec::new();
        self.eat_newlines();
        while !self.check(&Token::Eof) {
            stmts.push(self.parse_stmt()?);
            self.eat_newlines();
        }
        Ok(stmts)
    }

    // ── Statements ────────────────────────────────────────────────────────────

    fn parse_stmt(&mut self) -> Result<Stmt, String> {
        self.skip_newlines();
        match self.peek().clone() {
            Token::Let | Token::Var => self.parse_let(),
            Token::Fun              => self.parse_fun_def(false),
            Token::Pub              => {
                self.advance();
                match self.peek().clone() {
                    Token::Fun    => self.parse_fun_def(true),
                    Token::Struct => self.parse_struct_def(true),
                    Token::Enum   => self.parse_enum_def(true),
                    other => Err(format!("Expected fun/struct/enum after pub, got {:?}", other))
                }
            }
            Token::Struct => self.parse_struct_def(false),
            Token::Enum   => self.parse_enum_def(false),
            Token::Impl   => self.parse_impl_block(),
            Token::Mod    => self.parse_mod(),
            Token::Import => self.parse_import(),
            Token::Return => {
                self.advance();
                if matches!(self.peek(), Token::Newline | Token::Semicolon | Token::Eof) {
                    Ok(Stmt::Return(None))
                } else {
                    Ok(Stmt::Return(Some(self.parse_expr()?)))
                }
            }
            Token::While    => self.parse_while(),
            Token::For      => self.parse_for(),
            Token::Break    => { self.advance(); Ok(Stmt::Break) }
            Token::Continue => { self.advance(); Ok(Stmt::Continue) }
            Token::Type     => self.parse_type_alias(),
            _               => {
                let expr = self.parse_expr()?;
                Ok(Stmt::Expr(expr))
            }
        }
    }

    fn parse_let(&mut self) -> Result<Stmt, String> {
        let is_mutable = self.peek() == &Token::Var;
        self.advance(); // consume let/var

        let name = self.expect_ident()?;

        let ty = if self.eat(&Token::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };

        self.expect(&Token::Eq)?;
        let value = self.parse_expr()?;

        Ok(Stmt::Let(name, ty, value, is_mutable))
    }

    fn parse_fun_def(&mut self, is_pub: bool) -> Result<Stmt, String> {
        self.expect(&Token::Fun)?;
        let name = self.expect_ident()?;

        let generics = self.parse_generics_decl()?;

        self.expect(&Token::LParen)?;
        let params = self.parse_params()?;
        self.expect(&Token::RParen)?;

        let return_type = if self.eat(&Token::Arrow) {
            Some(self.parse_type()?)
        } else {
            None
        };

        self.skip_newlines();
        self.expect(&Token::LBrace)?;
        let body = self.parse_block_body()?;
        self.expect(&Token::RBrace)?;

        Ok(Stmt::FunDef(FunDef { name, generics, params, return_type, body, is_pub }))
    }

    fn parse_params(&mut self) -> Result<Vec<Param>, String> {
        let mut params = Vec::new();
        self.skip_newlines();
        while !self.check(&Token::RParen) {
            let name = self.expect_ident()?;
            let ty = if self.eat(&Token::Colon) { Some(self.parse_type()?) } else { None };
            let default = if self.eat(&Token::Eq) { Some(self.parse_expr()?) } else { None };
            params.push(Param { name, ty, default });
            self.skip_newlines();
            if !self.eat(&Token::Comma) { break; }
            self.skip_newlines();
        }
        Ok(params)
    }

    fn parse_block_body(&mut self) -> Result<Vec<Stmt>, String> {
        let mut stmts = Vec::new();
        self.eat_newlines();
        while !self.check(&Token::RBrace) && !self.check(&Token::Eof) {
            stmts.push(self.parse_stmt()?);
            self.eat_newlines();
        }
        Ok(stmts)
    }

    fn parse_struct_def(&mut self, is_pub: bool) -> Result<Stmt, String> {
        self.expect(&Token::Struct)?;
        let name = self.expect_ident()?;
        let generics = self.parse_generics_decl()?;
        self.skip_newlines();
        self.expect(&Token::LBrace)?;
        self.eat_newlines();
        let mut fields = Vec::new();
        while !self.check(&Token::RBrace) {
            let field_pub = self.eat(&Token::Pub);
            let fname = self.expect_ident()?;
            self.expect(&Token::Colon)?;
            let ftype = self.parse_type()?;
            fields.push(StructField { name: fname, ty: ftype, is_pub: field_pub });
            self.eat_newlines();
            self.eat(&Token::Comma);
            self.eat_newlines();
        }
        self.expect(&Token::RBrace)?;
        Ok(Stmt::StructDef(StructDef { name, generics, fields, is_pub }))
    }

    fn parse_enum_def(&mut self, is_pub: bool) -> Result<Stmt, String> {
        self.expect(&Token::Enum)?;
        let name = self.expect_ident()?;
        let generics = self.parse_generics_decl()?;
        self.skip_newlines();
        self.expect(&Token::LBrace)?;
        self.eat_newlines();
        let mut variants = Vec::new();
        while !self.check(&Token::RBrace) {
            let vname = self.expect_ident()?;
            let mut vfields = Vec::new();
            if self.eat(&Token::LParen) {
                while !self.check(&Token::RParen) {
                    vfields.push(self.parse_type()?);
                    if !self.eat(&Token::Comma) { break; }
                }
                self.expect(&Token::RParen)?;
            }
            variants.push(EnumVariant { name: vname, fields: vfields });
            self.eat_newlines();
            self.eat(&Token::Comma);
            self.eat_newlines();
        }
        self.expect(&Token::RBrace)?;
        Ok(Stmt::EnumDef(EnumDef { name, generics, variants, is_pub }))
    }

    fn parse_impl_block(&mut self) -> Result<Stmt, String> {
        self.expect(&Token::Impl)?;
        let generics = self.parse_generics_decl()?;
        let target = self.expect_ident()?;
        self.skip_newlines();
        self.expect(&Token::LBrace)?;
        self.eat_newlines();
        let mut methods = Vec::new();
        while !self.check(&Token::RBrace) {
            let is_pub = self.eat(&Token::Pub);
            self.expect(&Token::Fun)?;
            let name = self.expect_ident()?;
            let mg = self.parse_generics_decl()?;
            self.expect(&Token::LParen)?;
            let params = self.parse_params()?;
            self.expect(&Token::RParen)?;
            let return_type = if self.eat(&Token::Arrow) { Some(self.parse_type()?) } else { None };
            self.skip_newlines();
            self.expect(&Token::LBrace)?;
            let body = self.parse_block_body()?;
            self.expect(&Token::RBrace)?;
            methods.push(FunDef { name, generics: mg, params, return_type, body, is_pub });
            self.eat_newlines();
        }
        self.expect(&Token::RBrace)?;
        Ok(Stmt::ImplBlock(ImplBlock { target, generics, methods }))
    }

    fn parse_mod(&mut self) -> Result<Stmt, String> {
        self.expect(&Token::Mod)?;
        let name = self.expect_ident()?;
        self.skip_newlines();
        self.expect(&Token::LBrace)?;
        let body = self.parse_block_body()?;
        self.expect(&Token::RBrace)?;
        Ok(Stmt::ModDef(name, body))
    }

    fn parse_import(&mut self) -> Result<Stmt, String> {
        self.expect(&Token::Import)?;
        let mut path = vec![self.expect_ident()?];
        while self.eat(&Token::Dot) {
            path.push(self.expect_ident()?);
        }
        Ok(Stmt::Import(path))
    }

    fn parse_while(&mut self) -> Result<Stmt, String> {
        self.expect(&Token::While)?;
        let cond = self.parse_expr()?;
        self.skip_newlines();
        self.expect(&Token::LBrace)?;
        let body = self.parse_block_body()?;
        self.expect(&Token::RBrace)?;
        Ok(Stmt::While(cond, body))
    }

    fn parse_for(&mut self) -> Result<Stmt, String> {
        self.expect(&Token::For)?;
        let var = self.expect_ident()?;
        self.expect(&Token::In)?;
        let iter = self.parse_expr()?;
        self.skip_newlines();
        self.expect(&Token::LBrace)?;
        let body = self.parse_block_body()?;
        self.expect(&Token::RBrace)?;
        Ok(Stmt::For(var, iter, body))
    }

    fn parse_type_alias(&mut self) -> Result<Stmt, String> {
        self.expect(&Token::Type)?;
        let name = self.expect_ident()?;
        let generics = self.parse_generics_decl()?;
        self.expect(&Token::Eq)?;
        let ty = self.parse_type()?;
        Ok(Stmt::TypeAlias(name, generics, ty))
    }

    // ── Types ─────────────────────────────────────────────────────────────────

    fn parse_type(&mut self) -> Result<Type, String> {
        match self.peek().clone() {
            Token::IntType    => { self.advance(); Ok(Type::Int) }
            Token::FloatType  => { self.advance(); Ok(Type::Float) }
            Token::BoolType   => { self.advance(); Ok(Type::Bool) }
            Token::StringType => { self.advance(); Ok(Type::StringT) }
            Token::NilType    => { self.advance(); Ok(Type::Nil) }
            Token::LBracket   => {
                self.advance();
                let inner = self.parse_type()?;
                self.expect(&Token::RBracket)?;
                Ok(Type::List(Box::new(inner)))
            }
            Token::LParen => {
                self.advance();
                let mut types = Vec::new();
                while !self.check(&Token::RParen) {
                    types.push(self.parse_type()?);
                    if !self.eat(&Token::Comma) { break; }
                }
                self.expect(&Token::RParen)?;
                Ok(Type::Tuple(types))
            }
            Token::Ident(name) => {
                let name = name.clone();
                self.advance();
                // Generic: Name<T, U>
                if self.check(&Token::Lt) {
                    self.advance();
                    let mut args = Vec::new();
                    while !self.check(&Token::Gt) {
                        args.push(self.parse_type()?);
                        if !self.eat(&Token::Comma) { break; }
                    }
                    self.expect(&Token::Gt)?;
                    match name.as_str() {
                        "Option" => Ok(Type::Option(Box::new(args.remove(0)))),
                        "Result" if args.len() >= 2 => {
                            let e = args.remove(1);
                            Ok(Type::Result(Box::new(args.remove(0)), Box::new(e)))
                        }
                        "Map" if args.len() >= 2 => {
                            let v = args.remove(1);
                            Ok(Type::Map(Box::new(args.remove(0)), Box::new(v)))
                        }
                        _ => Ok(Type::Generic(name, args)),
                    }
                } else {
                    match name.as_str() {
                        "Option" => Ok(Type::Named(name)),
                        _ => Ok(Type::Named(name))
                    }
                }
            }
            other => Err(format!("Expected type, got {:?} at line {}", other, self.span_line()))
        }
    }

    fn parse_generics_decl(&mut self) -> Result<Vec<String>, String> {
        let mut generics = Vec::new();
        if self.check(&Token::Lt) {
            self.advance();
            while !self.check(&Token::Gt) {
                generics.push(self.expect_ident()?);
                if !self.eat(&Token::Comma) { break; }
            }
            self.expect(&Token::Gt)?;
        }
        Ok(generics)
    }

    // ── Expressions ───────────────────────────────────────────────────────────

    fn parse_expr(&mut self) -> Result<Expr, String> {
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> Result<Expr, String> {
        let lhs = self.parse_or()?;
        if self.eat(&Token::Eq) {
            let rhs = self.parse_assignment()?;
            return Ok(Expr::Assign(Box::new(lhs), Box::new(rhs)));
        }
        Ok(lhs)
    }

    fn parse_or(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_and()?;
        while self.check(&Token::Or) {
            self.advance();
            let right = self.parse_and()?;
            left = Expr::BinOp(Box::new(left), BinOp::Or, Box::new(right));
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_equality()?;
        while self.check(&Token::And) {
            self.advance();
            let right = self.parse_equality()?;
            left = Expr::BinOp(Box::new(left), BinOp::And, Box::new(right));
        }
        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_comparison()?;
        loop {
            let op = match self.peek() {
                Token::EqEq  => BinOp::Eq,
                Token::NotEq => BinOp::NotEq,
                _ => break,
            };
            self.advance();
            let right = self.parse_comparison()?;
            left = Expr::BinOp(Box::new(left), op, Box::new(right));
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_range()?;
        loop {
            let op = match self.peek() {
                Token::Lt   => BinOp::Lt,
                Token::LtEq => BinOp::LtEq,
                Token::Gt   => BinOp::Gt,
                Token::GtEq => BinOp::GtEq,
                _ => break,
            };
            self.advance();
            let right = self.parse_range()?;
            left = Expr::BinOp(Box::new(left), op, Box::new(right));
        }
        Ok(left)
    }

    fn parse_range(&mut self) -> Result<Expr, String> {
        let left = self.parse_addition()?;
        if self.eat(&Token::DotDot) {
            let right = self.parse_addition()?;
            return Ok(Expr::Range(Box::new(left), Box::new(right)));
        }
        Ok(left)
    }

    fn parse_addition(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_multiplication()?;
        loop {
            let op = match self.peek() {
                Token::Plus  => BinOp::Add,
                Token::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_multiplication()?;
            left = Expr::BinOp(Box::new(left), op, Box::new(right));
        }
        Ok(left)
    }

    fn parse_multiplication(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                Token::Star    => BinOp::Mul,
                Token::Slash   => BinOp::Div,
                Token::Percent => BinOp::Mod,
                _ => break,
            };
            self.advance();
            let right = self.parse_unary()?;
            left = Expr::BinOp(Box::new(left), op, Box::new(right));
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        match self.peek().clone() {
            Token::Minus => { self.advance(); Ok(Expr::UnaryOp(UnaryOp::Neg, Box::new(self.parse_unary()?))) }
            Token::Not   => { self.advance(); Ok(Expr::UnaryOp(UnaryOp::Not, Box::new(self.parse_unary()?))) }
            Token::Ref   => { self.advance(); Ok(Expr::RefExpr(Box::new(self.parse_unary()?))) }
            Token::Box   => { self.advance(); Ok(Expr::BoxExpr(Box::new(self.parse_unary()?))) }
            _            => self.parse_postfix()
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_primary()?;
        loop {
            match self.peek().clone() {
                Token::Dot => {
                    self.advance();
                    let field = self.expect_ident()?;
                    if self.eat(&Token::LParen) {
                        let args = self.parse_args()?;
                        self.expect(&Token::RParen)?;
                        expr = Expr::MethodCall(Box::new(expr), field, args);
                    } else {
                        expr = Expr::FieldAccess(Box::new(expr), field);
                    }
                }
                Token::LParen => {
                    self.advance();
                    let args = self.parse_args()?;
                    self.expect(&Token::RParen)?;
                    expr = Expr::Call(Box::new(expr), args);
                }
                Token::LBracket => {
                    self.advance();
                    let idx = self.parse_expr()?;
                    self.expect(&Token::RBracket)?;
                    expr = Expr::Index(Box::new(expr), Box::new(idx));
                }
                Token::Question => {
                    self.advance();
                    expr = Expr::Question(Box::new(expr));
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_args(&mut self) -> Result<Vec<Expr>, String> {
        let mut args = Vec::new();
        self.skip_newlines();
        while !self.check(&Token::RParen) && !self.check(&Token::Eof) {
            args.push(self.parse_expr()?);
            self.skip_newlines();
            if !self.eat(&Token::Comma) { break; }
            self.skip_newlines();
        }
        Ok(args)
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        match self.peek().clone() {
            Token::Int(n)    => { self.advance(); Ok(Expr::Int(n)) }
            Token::Float(f)  => { self.advance(); Ok(Expr::Float(f)) }
            Token::Bool(b)   => { self.advance(); Ok(Expr::Bool(b)) }
            Token::Nil       => { self.advance(); Ok(Expr::Nil) }

            Token::StringLit(s) => {
                self.advance();
                // Parse interpolation: #{expr}
                if s.contains("#{") {
                    Ok(Expr::InterpolatedString(parse_interpolation(&s)?))
                } else {
                    Ok(Expr::StringLit(s))
                }
            }

            Token::If => self.parse_if(),

            Token::Match => self.parse_match(),

            Token::Pipe => self.parse_closure(),

            Token::LParen => {
                self.advance();
                self.skip_newlines();
                if self.eat(&Token::RParen) {
                    return Ok(Expr::Tuple(vec![]));
                }
                let first = self.parse_expr()?;
                self.skip_newlines();
                if self.eat(&Token::Comma) {
                    let mut elems = vec![first];
                    self.skip_newlines();
                    while !self.check(&Token::RParen) {
                        elems.push(self.parse_expr()?);
                        self.skip_newlines();
                        if !self.eat(&Token::Comma) { break; }
                        self.skip_newlines();
                    }
                    self.expect(&Token::RParen)?;
                    Ok(Expr::Tuple(elems))
                } else {
                    self.expect(&Token::RParen)?;
                    Ok(first)
                }
            }

            Token::LBracket => {
                self.advance();
                let mut elems = Vec::new();
                self.skip_newlines();
                while !self.check(&Token::RBracket) {
                    elems.push(self.parse_expr()?);
                    self.skip_newlines();
                    if !self.eat(&Token::Comma) { break; }
                    self.skip_newlines();
                }
                self.expect(&Token::RBracket)?;
                Ok(Expr::List(elems))
            }

            Token::LBrace => {
                // Map literal or block — heuristic: if first token is a string/ident followed by colon → map
                self.advance();
                self.skip_newlines();

                if self.check(&Token::RBrace) {
                    self.advance();
                    return Ok(Expr::MapLit(vec![]));
                }
                
                // Check for map literal: could start with expr: expr
                // We parse it as a block unless we detect key: value pattern
                let mut stmts = Vec::new();
                let mut last_expr: Option<Expr> = None;
                while !self.check(&Token::RBrace) && !self.check(&Token::Eof) {
                    let s = self.parse_stmt()?;
                    self.eat_newlines();
                    if self.check(&Token::RBrace) {
                        // Last element — could be trailing expr
                        if let Stmt::Expr(e) = s {
                            last_expr = Some(e);
                        } else {
                            stmts.push(s);
                        }
                    } else {
                        stmts.push(s);
                    }
                }
                self.expect(&Token::RBrace)?;
                Ok(Expr::Block(stmts, last_expr.map(Box::new)))
            }

            Token::Ident(name) => {
                let name = name.clone();
                self.advance();

                // Enum variant: Name::Variant
                if self.eat(&Token::Colon) {
                    if self.eat(&Token::Colon) {
                        let variant = self.expect_ident()?;
                        let mut fields = Vec::new();
                        if self.eat(&Token::LParen) {
                            while !self.check(&Token::RParen) {
                                fields.push(self.parse_expr()?);
                                if !self.eat(&Token::Comma) { break; }
                            }
                            self.expect(&Token::RParen)?;
                        }
                        return Ok(Expr::EnumVariant(name, variant, fields));
                    }
                }

                // Struct creation: Name { field: val }
                // Only if next token is { and peek inside has ident:
                if self.check(&Token::LBrace) && self.peek2() != &Token::RBrace {
                    // Heuristic: could be struct literal
                    // We peek ahead to see if it's "ident: expr"
                    let saved_pos = self.pos;
                    self.advance(); // consume {
                    self.skip_newlines();
                    if let Token::Ident(_) = self.peek().clone() {
                        let maybe_field_pos = self.pos;
                        self.advance();
                        if self.eat(&Token::Colon) {
                            // It's a struct literal
                            self.pos = maybe_field_pos;
                            let mut fields = Vec::new();
                            while !self.check(&Token::RBrace) {
                                let fname = self.expect_ident()?;
                                self.expect(&Token::Colon)?;
                                let fval = self.parse_expr()?;
                                fields.push((fname, fval));
                                self.skip_newlines();
                                if !self.eat(&Token::Comma) { break; }
                                self.skip_newlines();
                            }
                            self.expect(&Token::RBrace)?;
                            return Ok(Expr::StructCreate(name, fields));
                        } else {
                            self.pos = saved_pos;
                        }
                    } else {
                        self.pos = saved_pos;
                    }
                }

                Ok(Expr::Var(name))
            }

            other => Err(format!("Unexpected token in expression: {:?} at line {}", other, self.span_line()))
        }
    }

    fn parse_if(&mut self) -> Result<Expr, String> {
        self.expect(&Token::If)?;
        let cond = self.parse_expr()?;
        self.skip_newlines();
        self.expect(&Token::LBrace)?;
        let then_body = self.parse_block_body()?;
        self.expect(&Token::RBrace)?;

        let mut elif_branches = Vec::new();
        let mut else_branch = None;

        loop {
            self.skip_newlines();
            if self.eat(&Token::Elif) {
                let elif_cond = self.parse_expr()?;
                self.skip_newlines();
                self.expect(&Token::LBrace)?;
                let elif_body = self.parse_block_body()?;
                self.expect(&Token::RBrace)?;
                let elif_expr = Expr::Block(elif_body, None);
                elif_branches.push((elif_cond, elif_expr));
            } else if self.eat(&Token::Else) {
                self.skip_newlines();
                self.expect(&Token::LBrace)?;
                let else_body = self.parse_block_body()?;
                self.expect(&Token::RBrace)?;
                else_branch = Some(Box::new(Expr::Block(else_body, None)));
                break;
            } else {
                break;
            }
        }

        let then_expr = Expr::Block(then_body, None);
        Ok(Expr::If(Box::new(cond), Box::new(then_expr), elif_branches, else_branch))
    }

    fn parse_match(&mut self) -> Result<Expr, String> {
        self.expect(&Token::Match)?;
        let subject = self.parse_expr()?;
        self.skip_newlines();
        self.expect(&Token::LBrace)?;
        self.eat_newlines();
        let mut arms = Vec::new();
        while !self.check(&Token::RBrace) && !self.check(&Token::Eof) {
            let pattern = self.parse_pattern()?;
            let guard = if self.eat(&Token::If) { Some(self.parse_expr()?) } else { None };
            self.eat(&Token::FatArrow);
            let body = if self.check(&Token::LBrace) {
                self.advance();
                let stmts = self.parse_block_body()?;
                self.expect(&Token::RBrace)?;
                Expr::Block(stmts, None)
            } else {
                self.parse_expr()?
            };
            arms.push(MatchArm { pattern, guard, body });
            self.eat_newlines();
            self.eat(&Token::Comma);
            self.eat_newlines();
        }
        self.expect(&Token::RBrace)?;
        Ok(Expr::Match(Box::new(subject), arms))
    }

    fn parse_pattern(&mut self) -> Result<Pattern, String> {
        let pat = self.parse_pattern_atom()?;
        if self.eat(&Token::Pipe) {
            let right = self.parse_pattern()?;
            return Ok(Pattern::Or(Box::new(pat), Box::new(right)));
        }
        Ok(pat)
    }

    fn parse_pattern_atom(&mut self) -> Result<Pattern, String> {
        match self.peek().clone() {
            Token::Ident(name) => {
                let name = name.clone();
                self.advance();
                if self.eat(&Token::Colon) && self.eat(&Token::Colon) {
                    let variant = self.expect_ident()?;
                    let mut fields = Vec::new();
                    if self.eat(&Token::LParen) {
                        while !self.check(&Token::RParen) {
                            fields.push(self.parse_pattern()?);
                            if !self.eat(&Token::Comma) { break; }
                        }
                        self.expect(&Token::RParen)?;
                    }
                    return Ok(Pattern::EnumVariant(name, variant, fields));
                }
                match name.as_str() {
                    "_"    => Ok(Pattern::Wildcard),
                    "nil"  => Ok(Pattern::Nil),
                    "Some" => {
                        self.expect(&Token::LParen)?;
                        let inner = self.parse_pattern()?;
                        self.expect(&Token::RParen)?;
                        Ok(Pattern::Some(Box::new(inner)))
                    }
                    "Ok" => {
                        self.expect(&Token::LParen)?;
                        let inner = self.parse_pattern()?;
                        self.expect(&Token::RParen)?;
                        Ok(Pattern::Ok(Box::new(inner)))
                    }
                    "Err" => {
                        self.expect(&Token::LParen)?;
                        let inner = self.parse_pattern()?;
                        self.expect(&Token::RParen)?;
                        Ok(Pattern::Err(Box::new(inner)))
                    }
                    _ => Ok(Pattern::Ident(name)),
                }
            }
            Token::Int(n)        => { let n = n; self.advance(); Ok(Pattern::Int(n)) }
            Token::Float(f)      => { let f = f; self.advance(); Ok(Pattern::Float(f)) }
            Token::Bool(b)       => { let b = b; self.advance(); Ok(Pattern::Bool(b)) }
            Token::StringLit(s)  => { let s = s.clone(); self.advance(); Ok(Pattern::StringLit(s)) }
            Token::Nil           => { self.advance(); Ok(Pattern::Nil) }
            Token::LParen => {
                self.advance();
                let mut pats = Vec::new();
                while !self.check(&Token::RParen) {
                    pats.push(self.parse_pattern()?);
                    if !self.eat(&Token::Comma) { break; }
                }
                self.expect(&Token::RParen)?;
                Ok(Pattern::Tuple(pats))
            }
            Token::LBracket => {
                self.advance();
                let mut pats = Vec::new();
                while !self.check(&Token::RBracket) {
                    pats.push(self.parse_pattern()?);
                    if !self.eat(&Token::Comma) { break; }
                }
                self.expect(&Token::RBracket)?;
                Ok(Pattern::List(pats))
            }
            other => Err(format!("Expected pattern, got {:?}", other))
        }
    }

    fn parse_closure(&mut self) -> Result<Expr, String> {
        self.expect(&Token::Pipe)?;
        let mut params = Vec::new();
        while !self.check(&Token::Pipe) {
            let name = self.expect_ident()?;
            let ty = if self.eat(&Token::Colon) { Some(self.parse_type()?) } else { None };
            params.push((name, ty));
            if !self.eat(&Token::Comma) { break; }
        }
        self.expect(&Token::Pipe)?;
        let body = if self.eat(&Token::FatArrow) {
            self.parse_expr()?
        } else {
            self.skip_newlines();
            self.expect(&Token::LBrace)?;
            let stmts = self.parse_block_body()?;
            self.expect(&Token::RBrace)?;
            Expr::Block(stmts, None)
        };
        Ok(Expr::Closure(params, Box::new(body)))
    }

    fn expect_ident(&mut self) -> Result<String, String> {
        match self.peek().clone() {
            Token::Ident(s) => { self.advance(); Ok(s) }
            other => Err(format!("Expected identifier, got {:?} at line {}", other, self.span_line()))
        }
    }
}

// Parse string interpolation: "Hello #{name}, you are #{age} years old"
fn parse_interpolation(s: &str) -> Result<Vec<StringPart>, String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '#' && i + 1 < chars.len() && chars[i + 1] == '{' {
            if !current.is_empty() {
                parts.push(StringPart::Literal(current.clone()));
                current.clear();
            }
            i += 2; // skip #{
            let mut expr_src = String::new();
            let mut depth = 1;
            while i < chars.len() {
                if chars[i] == '{' { depth += 1; }
                else if chars[i] == '}' {
                    depth -= 1;
                    if depth == 0 { i += 1; break; }
                }
                expr_src.push(chars[i]);
                i += 1;
            }
            // Parse the expression inside #{}
            let mut lex = crate::lexer::Lexer::new(&expr_src);
            let tokens = lex.tokenize().map_err(|e| format!("In interpolation: {}", e))?;
            let mut parser = crate::parser::Parser::new(tokens);
            let expr = parser.parse_expr().map_err(|e| format!("In interpolation: {}", e))?;
            parts.push(StringPart::Interpolated(expr));
        } else {
            current.push(chars[i]);
            i += 1;
        }
    }

    if !current.is_empty() {
        parts.push(StringPart::Literal(current));
    }

    Ok(parts)
}