// ═══════════════════════════════════════════════════════════
// Zephyr — The Zephyr Programming Language
// ═══════════════════════════════════════════════════════════

mod lexer;
mod ast;
mod parser;
mod interpreter;
mod stdlib;
mod net;
mod json;
mod process;
mod zfs;
mod async_rt;
mod bytecode;
mod bundle;

use std::env;
use std::fs;
use std::io::{self, Write, BufRead};
use std::path::Path;
use interpreter::{Interpreter, Signal};

fn main() {
    // ── Bundled-binary check ───────────────────────────────────────────────
    // Before doing anything else, check if this binary has a Zephyr bytecode
    // payload appended to it. If so, extract and run it immediately — this
    // binary IS the program, not the Zephyr CLI.
    if let Some(stmts) = bundle::extract_payload() {
        let mut interp = Interpreter::new();
        match interp.run(&stmts) {
            Ok(_) | Err(Signal::Return(_)) => std::process::exit(0),
            Err(Signal::Error(e)) => {
                eprintln!("\x1b[31m[error]\x1b[0m {}", e);
                std::process::exit(1);
            }
            Err(Signal::PropagateErr(v)) => {
                eprintln!("\x1b[31m[unhandled error]\x1b[0m {}", v);
                std::process::exit(1);
            }
            Err(Signal::Break) | Err(Signal::Continue) => std::process::exit(0),
        }
    }

    // ── Normal Zephyr CLI ──────────────────────────────────────────────────
    let args: Vec<String> = env::args().collect();

    match args.get(1).map(|s| s.as_str()) {
        Some("repl") | None => repl(),

        Some("run") => {
            let file = args.get(2).expect("Usage: zephyr run <file.zph>");
            run_file(file);
        }

        Some("compile") => {
            compile_command(&args[2..]);
        }

        Some("check") => {
            let file = args.get(2).expect("Usage: zephyr check <file.zph>");
            check_file(file);
        }

        Some(file) if file.ends_with(".zph")  => run_file(file),
        Some(file) if file.ends_with(".zphc") => run_bytecode_file(file),

        Some(cmd) => {
            eprintln!("Unknown command '{}'. Try: zephyr [run|compile|check|repl] ...", cmd);
            eprintln!();
            eprintln!("  zephyr run <file.zph>          Run a source file");
            eprintln!("  zephyr compile <file.zph>      Compile to bytecode + native executable");
            eprintln!("  zephyr compile -o <out> <file> Specify output path (no extension)");
            eprintln!("  zephyr check <file.zph>        Parse-check without running");
            eprintln!("  zephyr repl                    Start interactive REPL");
            std::process::exit(1);
        }

        _ => repl(),
    }
}

// ═══════════════════════════════════════════════════════════
// compile subcommand
// ═══════════════════════════════════════════════════════════

fn compile_command(args: &[String]) {
    let mut output_path: Option<String> = None;
    let mut input_path: Option<String> = None;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => {
                i += 1;
                output_path = args.get(i).cloned();
            }
            flag if flag.starts_with('-') => {
                eprintln!("Unknown flag '{}'. Supported: -o <output>", flag);
                std::process::exit(1);
            }
            path => {
                if input_path.is_some() {
                    eprintln!("Multiple input files given. Only one is supported.");
                    std::process::exit(1);
                }
                input_path = Some(path.to_string());
            }
        }
        i += 1;
    }

    let input = input_path.unwrap_or_else(|| {
        eprintln!("Usage: zephyr compile [-o output] <file.zph>");
        std::process::exit(1);
    });

    // Default output stem (no extension — we add .zphc and OS-specific binary ext ourselves)
    let stem = output_path.unwrap_or_else(|| {
        Path::new(&input)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output")
            .to_string()
    });

    let source = fs::read_to_string(&input).unwrap_or_else(|e| {
        eprintln!("\x1b[31m[Zephyr]\x1b[0m Cannot read '{}': {}", input, e);
        std::process::exit(1);
    });

    let stmts = parse_source(&source, &input);

    // 1. Write .zphc bytecode
    let zphc_path = format!("{}.zphc", stem);
    let encoded = bytecode::encode(&stmts, &source);
    fs::write(&zphc_path, &encoded).unwrap_or_else(|e| {
        eprintln!("\x1b[31m[Zephyr]\x1b[0m Cannot write '{}': {}", zphc_path, e);
        std::process::exit(1);
    });
    eprintln!(
        "\x1b[36m[Zephyr]\x1b[0m Bytecode  → \x1b[32m{}\x1b[0m ({:.1} KB)",
        zphc_path,
        encoded.len() as f64 / 1024.0
    );

    // 2. Write native executable (OS-dependent name)
    let exe_path = bundle::exe_path(&stem);
    let exe_size = bundle::write_executable(&encoded, &exe_path).unwrap_or_else(|e| {
        eprintln!("\x1b[31m[Zephyr]\x1b[0m Failed to create executable '{}': {}", exe_path, e);
        std::process::exit(1);
    });
    eprintln!(
        "\x1b[36m[Zephyr]\x1b[0m Executable → \x1b[32m{}\x1b[0m ({:.1} MB)",
        exe_path,
        exe_size as f64 / (1024.0 * 1024.0)
    );
    eprintln!("  Run with: ./{}", exe_path);
}

// ═══════════════════════════════════════════════════════════
// check subcommand
// ═══════════════════════════════════════════════════════════

fn check_file(path: &str) {
    let source = fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("\x1b[31m[Zephyr]\x1b[0m Cannot read '{}': {}", path, e);
        std::process::exit(1);
    });
    let mut lexer = lexer::Lexer::new(&source);
    let tokens = lexer.tokenize().unwrap_or_else(|e| {
        eprintln!("\x1b[31m[lex error]\x1b[0m in {}: {}", path, e);
        std::process::exit(1);
    });
    let mut parser = parser::Parser::new(tokens);
    let ast = parser.parse_program().unwrap_or_else(|e| {
        eprintln!("\x1b[31m[parse error]\x1b[0m in {}: {}", path, e);
        std::process::exit(1);
    });
    println!("\x1b[32m✓\x1b[0m {} — OK ({} top-level statements)", path, ast.len());
}

// ═══════════════════════════════════════════════════════════
// Running .zphc bytecode files
// ═══════════════════════════════════════════════════════════

fn run_bytecode_file(path: &str) {
    let data = fs::read(path).unwrap_or_else(|e| {
        eprintln!("\x1b[31m[Zephyr]\x1b[0m Cannot read '{}': {}", path, e);
        std::process::exit(1);
    });

    // Stale-bytecode warning
    let source_path = path.replace(".zphc", ".zph");
    if let Ok(source) = fs::read_to_string(&source_path) {
        if !bytecode::is_fresh(&data, &source) {
            eprintln!(
                "\x1b[33m[Zephyr warning]\x1b[0m '{}' is stale — source changed since last compile.",
                path
            );
            eprintln!("  Recompile with: zephyr compile {}", source_path);
        }
    }

    let (stmts, _hash) = bytecode::decode(&data).unwrap_or_else(|e| {
        eprintln!("\x1b[31m[bytecode error]\x1b[0m {}", e);
        std::process::exit(1);
    });

    let mut interp = Interpreter::new();
    match interp.run(&stmts) {
        Ok(_) | Err(Signal::Return(_)) => {}
        Err(Signal::Error(e)) => {
            eprintln!("\x1b[31m[runtime error]\x1b[0m {}", e);
            std::process::exit(1);
        }
        Err(Signal::PropagateErr(v)) => {
            eprintln!("\x1b[31m[unhandled error]\x1b[0m {}", v);
            std::process::exit(1);
        }
        Err(Signal::Break) | Err(Signal::Continue) => {}
    }
}

// ═══════════════════════════════════════════════════════════
// Shared helpers
// ═══════════════════════════════════════════════════════════

fn parse_source(source: &str, filename: &str) -> Vec<ast::Stmt> {
    let mut lex = lexer::Lexer::new(source);
    let tokens = lex.tokenize().unwrap_or_else(|e| {
        eprintln!("\x1b[31m[lex error]\x1b[0m in {}: {}", filename, e);
        std::process::exit(1);
    });
    let mut parser = parser::Parser::new(tokens);
    parser.parse_program().unwrap_or_else(|e| {
        eprintln!("\x1b[31m[parse error]\x1b[0m in {}: {}", filename, e);
        std::process::exit(1);
    })
}

fn run_file(path: &str) {
    let source = fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("\x1b[31m[Zephyr]\x1b[0m Cannot read '{}': {}", path, e);
        std::process::exit(1);
    });
    match run_source(&source, path) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("\x1b[31m[Zephyr error]\x1b[0m {}", e);
            std::process::exit(1);
        }
    }
}

fn run_source(source: &str, filename: &str) -> Result<(), String> {
    let mut lexer = lexer::Lexer::new(source);
    let tokens = lexer.tokenize().map_err(|e| format!("Lex error in {}: {}", filename, e))?;
    let mut parser = parser::Parser::new(tokens);
    let ast = parser.parse_program().map_err(|e| format!("Parse error in {}: {}", filename, e))?;
    let mut interp = Interpreter::new();
    match interp.run(&ast) {
        Ok(_)                        => Ok(()),
        Err(Signal::Return(_))       => Ok(()),
        Err(Signal::Error(e))        => Err(format!("Runtime error: {}", e)),
        Err(Signal::PropagateErr(v)) => Err(format!("Unhandled error: {}", v)),
        Err(Signal::Break)           => Err("break outside loop".into()),
        Err(Signal::Continue)        => Err("continue outside loop".into()),
    }
}

// ═══════════════════════════════════════════════════════════
// REPL
// ═══════════════════════════════════════════════════════════

fn repl() {
    println!("\x1b[36m");
    println!("  ███████╗███████╗██████╗ ██╗  ██╗██╗   ██╗██████╗ ");
    println!("  ╚══███╔╝██╔════╝██╔══██╗██║  ██║╚██╗ ██╔╝██╔══██╗");
    println!("    ███╔╝ █████╗  ██████╔╝███████║ ╚████╔╝ ██████╔╝");
    println!("   ███╔╝  ██╔══╝  ██╔═══╝ ██╔══██║  ╚██╔╝  ██╔══██╗");
    println!("  ███████╗███████╗██║     ██║  ██║   ██║   ██║  ██║");
    println!("  ╚══════╝╚══════╝╚═╝     ╚═╝  ╚═╝   ╚═╝   ╚═╝  ╚═╝");
    println!("\x1b[0m");
    println!("  \x1b[90mThe Zephyr Programming Language v0.9.9\x1b[0m");
    println!("  \x1b[90mType :help for help, :quit to exit\x1b[0m");
    println!();

    let mut interp = Interpreter::new();
    let stdin = io::stdin();
    let mut input_buffer = String::new();
    let mut multiline = false;
    let mut brace_depth: i32 = 0;

    loop {
        let prompt = if multiline { "  ... " } else { "\x1b[36mzph>\x1b[0m " };
        print!("{}", prompt);
        io::stdout().flush().unwrap();

        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(e) => { eprintln!("Read error: {}", e); break; }
        }

        let trimmed = line.trim();
        match trimmed {
            ":quit" | ":q" | ":exit" => { println!("Goodbye!"); break; }
            ":help" | ":h"           => { print_help(); continue; }
            ":clear" => { print!("\x1b[2J\x1b[H"); io::stdout().flush().unwrap(); continue; }
            "" if !multiline => continue,
            _ => {}
        }

        for ch in trimmed.chars() {
            match ch { '{' => brace_depth += 1, '}' => brace_depth -= 1, _ => {} }
        }

        input_buffer.push_str(&line);

        if brace_depth > 0 { multiline = true; continue; }
        multiline = false;
        brace_depth = 0;
        let source = input_buffer.clone();
        input_buffer.clear();

        let tokens = match lexer::Lexer::new(&source).tokenize() {
            Ok(t) => t,
            Err(e) => { eprintln!("\x1b[31m[lex error]\x1b[0m {}", e); continue; }
        };
        let ast = match parser::Parser::new(tokens).parse_program() {
            Ok(a) => a,
            Err(e) => { eprintln!("\x1b[31m[parse error]\x1b[0m {}", e); continue; }
        };

        match interp.run(&ast) {
            Ok(interpreter::Value::Nil) => {}
            Ok(val)                      => println!("\x1b[32m=> {}\x1b[0m", val),
            Err(Signal::Return(v))       => println!("\x1b[32m=> {}\x1b[0m", v),
            Err(Signal::Error(e))        => eprintln!("\x1b[31m[runtime error]\x1b[0m {}", e),
            Err(Signal::PropagateErr(v)) => eprintln!("\x1b[31m[error propagated]\x1b[0m {}", v),
            Err(Signal::Break)           => eprintln!("\x1b[33m[warning]\x1b[0m break outside loop"),
            Err(Signal::Continue)        => eprintln!("\x1b[33m[warning]\x1b[0m continue outside loop"),
        }
    }
}

fn print_help() {
    println!();
    println!("  \x1b[1mZephyr Language Quick Reference\x1b[0m");
    println!();
    println!("  \x1b[33mCLI commands:\x1b[0m");
    println!("    zephyr run <file.zph>          Run source file");
    println!("    zephyr <file.zph>              Shorthand for run");
    println!("    zephyr <file.zphc>             Run compiled bytecode");
    println!("    zephyr compile <file.zph>      Compile → .zphc + native executable");
    println!("    zephyr compile -o <stem> <f>   Custom output name (no extension)");
    println!("    zephyr check <file.zph>        Parse-check without running");
    println!("    zephyr repl                    Start REPL");
    println!();
    println!("  \x1b[33mVariables:\x1b[0m");
    println!("    let x = 42          // immutable");
    println!("    var y = \"hello\"     // mutable");
    println!();
    println!("  \x1b[33mFunctions:\x1b[0m");
    println!("    fun add(a: Int, b: Int) -> Int {{ a + b }}");
    println!("    fun greet(name) {{ println(\"Hello, #{{name}}!\") }}");
    println!();
    println!("  \x1b[33mTypes:\x1b[0m  Int, Float, Bool, String, Nil");
    println!("  \x1b[33mNullable:\x1b[0m  Option<T>,  Result<T, E>");
    println!();
    println!("  \x1b[33mControl flow:\x1b[0m");
    println!("    if x > 0 {{ ... }} elif x == 0 {{ ... }} else {{ ... }}");
    println!("    while cond {{ ... }}     for i in 0..10 {{ ... }}");
    println!("    match val {{ pattern => expr, _ => fallback }}");
    println!();
    println!("  \x1b[33mCollections:\x1b[0m");
    println!("    let list  = [1, 2, 3]");
    println!("    let map   = {{\"key\": value}}");
    println!("    let tuple = (1, \"hello\", true)");
    println!();
    println!("  \x1b[33mStructs & Enums:\x1b[0m");
    println!("    struct Point {{ x: Int, y: Int }}");
    println!("    enum Shape {{ Circle(Float), Rect(Float, Float) }}");
    println!();
    println!("  \x1b[33mAsync:\x1b[0m");
    println!("    let t  = async_http_get(url)       // spawn task");
    println!("    let r  = async_await(t)             // await result");
    println!("    let rs = async_await_all([t1, t2])  // await all");
    println!("    let ch = channel()                  // create channel");
    println!("    channel_send(ch, value)  channel_recv(ch)");
    println!();
    println!("  \x1b[33mREPL commands:\x1b[0m  :help  :clear  :quit");
    println!();
}