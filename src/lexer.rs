// Zephyr Lexer — tokenizes .zph source files

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Literals
    Int(i64),
    Float(f64),
    Bool(bool),
    StringLit(String),
    Nil,

    // Identifiers & keywords
    Ident(String),
    Fun,
    Let,
    Var,
    If,
    Else,
    Elif,
    While,
    For,
    In,
    Return,
    Struct,
    Enum,
    Impl,
    Match,
    Import,
    Pub,
    Priv,
    Mod,
    Break,
    Continue,
    New,
    Ref,
    Box,
    Type,

    // Types
    IntType,
    FloatType,
    BoolType,
    StringType,
    NilType,

    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Eq,
    EqEq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    And,
    Or,
    Not,
    Dot,
    DotDot,
    Arrow,
    FatArrow,
    Pipe,
    Amp,
    Hash,
    Question,

    // Delimiters
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Colon,
    Semicolon,
    Newline,

    // Special
    Eof,
}

#[derive(Debug, Clone)]
pub struct Span {
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone)]
pub struct TokenWithSpan {
    pub token: Token,
    pub span: Span,
}

pub struct Lexer {
    source: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
}

impl Lexer {
    pub fn new(source: &str) -> Self {
        Lexer {
            source: source.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    fn peek(&self) -> Option<char> {
        self.source.get(self.pos).copied()
    }

    fn peek2(&self) -> Option<char> {
        self.source.get(self.pos + 1).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.source.get(self.pos).copied();
        if let Some(c) = ch {
            self.pos += 1;
            if c == '\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
        }
        ch
    }

    fn current_span(&self) -> Span {
        Span { line: self.line, col: self.col }
    }

    fn skip_whitespace_no_newline(&mut self) {
        while let Some(c) = self.peek() {
            if c == ' ' || c == '\t' || c == '\r' {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn read_string(&mut self) -> Result<Token, String> {
        // Supports #{} interpolation by returning a joined string at lex time
        // (full interpolation is handled in parser)
        let mut result = String::new();
        loop {
            match self.advance() {
                None => return Err("Unterminated string literal".to_string()),
                Some('"') => break,
                Some('\\') => {
                    match self.advance() {
                        Some('n') => result.push('\n'),
                        Some('t') => result.push('\t'),
                        Some('r') => result.push('\r'),
                        Some('"') => result.push('"'),
                        Some('\\') => result.push('\\'),
                        Some('0') => result.push('\0'),
                        Some(c) => { result.push('\\'); result.push(c); }
                        None => return Err("Unterminated escape".to_string()),
                    }
                }
                Some(c) => result.push(c),
            }
        }
        Ok(Token::StringLit(result))
    }

    fn read_number(&mut self, first: char) -> Token {
        let mut s = String::new();
        s.push(first);
        let mut is_float = false;

        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                s.push(c);
                self.advance();
            } else if c == '.' && self.peek2().map_or(false, |x| x.is_ascii_digit()) {
                is_float = true;
                s.push(c);
                self.advance();
            } else {
                break;
            }
        }

        if is_float {
            Token::Float(s.parse().unwrap_or(0.0))
        } else {
            Token::Int(s.parse().unwrap_or(0))
        }
    }

    fn read_ident(&mut self, first: char) -> Token {
        let mut s = String::new();
        s.push(first);
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' {
                s.push(c);
                self.advance();
            } else {
                break;
            }
        }
        match s.as_str() {
            "fun"      => Token::Fun,
            "let"      => Token::Let,
            "var"      => Token::Var,
            "if"       => Token::If,
            "else"     => Token::Else,
            "elif"     => Token::Elif,
            "while"    => Token::While,
            "for"      => Token::For,
            "in"       => Token::In,
            "return"   => Token::Return,
            "struct"   => Token::Struct,
            "enum"     => Token::Enum,
            "impl"     => Token::Impl,
            "match"    => Token::Match,
            "import"   => Token::Import,
            "pub"      => Token::Pub,
            "priv"     => Token::Priv,
            "mod"      => Token::Mod,
            "break"    => Token::Break,
            "continue" => Token::Continue,
            "new"      => Token::New,
            "ref"      => Token::Ref,
            "box"      => Token::Box,
            "type"     => Token::Type,
            "true"     => Token::Bool(true),
            "false"    => Token::Bool(false),
            "nil"      => Token::Nil,
            "Int"      => Token::IntType,
            "Float"    => Token::FloatType,
            "Bool"     => Token::BoolType,
            "String"   => Token::StringType,
            "Nil"      => Token::NilType,
            _          => Token::Ident(s),
        }
    }

    pub fn tokenize(&mut self) -> Result<Vec<TokenWithSpan>, String> {
        let mut tokens = Vec::new();

        loop {
            self.skip_whitespace_no_newline();
            let span = self.current_span();

            let ch = match self.peek() {
                None => {
                    tokens.push(TokenWithSpan { token: Token::Eof, span });
                    break;
                }
                Some(c) => c,
            };

            // Comments
            if ch == '/' && self.peek2() == Some('/') {
                while let Some(c) = self.peek() {
                    if c == '\n' { break; }
                    self.advance();
                }
                continue;
            }

            // Multi-line comments
            if ch == '/' && self.peek2() == Some('*') {
                self.advance(); self.advance();
                loop {
                    match self.advance() {
                        None => return Err("Unterminated block comment".to_string()),
                        Some('*') if self.peek() == Some('/') => { self.advance(); break; }
                        _ => {}
                    }
                }
                continue;
            }

            self.advance();

            let token = match ch {
                '\n' => Token::Newline,
                '"' => self.read_string()?,
                c if c.is_ascii_digit() => self.read_number(c),
                c if c.is_alphabetic() || c == '_' => self.read_ident(c),
                '+' => Token::Plus,
                '-' => {
                    if self.peek() == Some('>') { self.advance(); Token::Arrow }
                    else { Token::Minus }
                }
                '*' => Token::Star,
                '/' => Token::Slash,
                '%' => Token::Percent,
                '=' => {
                    if self.peek() == Some('=') { self.advance(); Token::EqEq }
                    else if self.peek() == Some('>') { self.advance(); Token::FatArrow }
                    else { Token::Eq }
                }
                '!' => {
                    if self.peek() == Some('=') { self.advance(); Token::NotEq }
                    else { Token::Not }
                }
                '<' => {
                    if self.peek() == Some('=') { self.advance(); Token::LtEq }
                    else { Token::Lt }
                }
                '>' => {
                    if self.peek() == Some('=') { self.advance(); Token::GtEq }
                    else { Token::Gt }
                }
                '&' => {
                    if self.peek() == Some('&') { self.advance(); Token::And }
                    else { Token::Amp }
                }
                '|' => {
                    if self.peek() == Some('|') { self.advance(); Token::Or }
                    else { Token::Pipe }
                }
                '.' => {
                    if self.peek() == Some('.') { self.advance(); Token::DotDot }
                    else { Token::Dot }
                }
                '#' => Token::Hash,
                '?' => Token::Question,
                '(' => Token::LParen,
                ')' => Token::RParen,
                '{' => Token::LBrace,
                '}' => Token::RBrace,
                '[' => Token::LBracket,
                ']' => Token::RBracket,
                ',' => Token::Comma,
                ':' => Token::Colon,
                ';' => Token::Semicolon,
                _ => return Err(format!("Unexpected character '{}' at line {}, col {}", ch, span.line, span.col)),
            };

            tokens.push(TokenWithSpan { token, span });
        }

        Ok(tokens)
    }
}