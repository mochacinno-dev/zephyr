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

use std::env;
use std::fs;
use std::io::{self, Write, BufRead};
use interpreter::{Interpreter, Signal};

fn main() {
    let args: Vec<String> = env::args().collect();

    match args.get(1).map(|s| s.as_str()) {
        Some("repl") | None => repl(),
        Some("run")  => {
            let file = args.get(2).expect("Usage: zephyr run <file.zph>");
            run_file(file);
        }
        Some(file) if file.ends_with(".zph") => run_file(file),
        Some(cmd) => {
            eprintln!("Unknown command '{}'. Usage: zephyr [run] <file.zph>", cmd);
            std::process::exit(1);
        }
        _ => repl(),
    }
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
    // Lex
    let mut lexer = lexer::Lexer::new(source);
    let tokens = lexer.tokenize().map_err(|e| format!("Lex error in {}: {}", filename, e))?;

    // Parse
    let mut parser = parser::Parser::new(tokens);
    let ast = parser.parse_program().map_err(|e| format!("Parse error in {}: {}", filename, e))?;

    // Interpret
    let mut interp = Interpreter::new();
    match interp.run(&ast) {
        Ok(_) => Ok(()),
        Err(Signal::Return(_)) => Ok(()),
        Err(Signal::Error(e)) => Err(format!("Runtime error: {}", e)),
        Err(Signal::PropagateErr(v)) => Err(format!("Unhandled error: {}", v)),
        Err(Signal::Break) => Err("break outside loop".into()),
        Err(Signal::Continue) => Err("continue outside loop".into()),
    }
}

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
            Ok(0) => break, // EOF
            Ok(_) => {}
            Err(e) => { eprintln!("Read error: {}", e); break; }
        }

        let trimmed = line.trim();

        // REPL commands
        match trimmed {
            ":quit" | ":q" | ":exit" => {
                println!("Goodbye!");
                break;
            }
            ":help" | ":h" => {
                print_help();
                continue;
            }
            ":clear" => {
                print!("\x1b[2J\x1b[H");
                io::stdout().flush().unwrap();
                continue;
            }
            "" if !multiline => continue,
            _ => {}
        }

        // Track brace depth for multiline
        for ch in trimmed.chars() {
            match ch {
                '{' => brace_depth += 1,
                '}' => brace_depth -= 1,
                _ => {}
            }
        }

        input_buffer.push_str(&line);

        if brace_depth > 0 {
            multiline = true;
            continue;
        }

        multiline = false;
        brace_depth = 0;
        let source = input_buffer.clone();
        input_buffer.clear();

        // Lex
        let tokens = match lexer::Lexer::new(&source).tokenize() {
            Ok(t) => t,
            Err(e) => { eprintln!("\x1b[31m[lex error]\x1b[0m {}", e); continue; }
        };

        // Parse
        let ast = match parser::Parser::new(tokens).parse_program() {
            Ok(a) => a,
            Err(e) => { eprintln!("\x1b[31m[parse error]\x1b[0m {}", e); continue; }
        };

        // Eval
        match interp.run(&ast) {
            Ok(interpreter::Value::Nil) => {}
            Ok(val) => println!("\x1b[32m=> {}\x1b[0m", val),
            Err(Signal::Return(v)) => println!("\x1b[32m=> {}\x1b[0m", v),
            Err(Signal::Error(e)) => eprintln!("\x1b[31m[runtime error]\x1b[0m {}", e),
            Err(Signal::PropagateErr(v)) => eprintln!("\x1b[31m[error propagated]\x1b[0m {}", v),
            Err(Signal::Break) => eprintln!("\x1b[33m[warning]\x1b[0m break outside loop"),
            Err(Signal::Continue) => eprintln!("\x1b[33m[warning]\x1b[0m continue outside loop"),
        }
    }
}

fn print_help() {
    println!();
    println!("  \x1b[1mZephyr Language Quick Reference\x1b[0m");
    println!();
    println!("  \x1b[33mVariables:\x1b[0m");
    println!("    let x = 42              // immutable");
    println!("    var y = \"hello\"         // mutable");
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
    println!("    while cond {{ ... }}");
    println!("    for i in 0..10 {{ ... }}");
    println!("    match val {{ pattern => expr, _ => fallback }}");
    println!();
    println!("  \x1b[33mCollections:\x1b[0m");
    println!("    let list = [1, 2, 3]");
    println!("    let tuple = (1, \"hello\", true)");
    println!();
    println!("  \x1b[33mStructs & Enums:\x1b[0m");
    println!("    struct Point {{ x: Int, y: Int }}");
    println!("    enum Shape {{ Circle(Float), Rect(Float, Float) }}");
    println!();
    println!("  \x1b[33mClosures:\x1b[0m  |x| => x * 2   or   |x, y| {{ x + y }}");
    println!();
    println!("  \x1b[33mREPL commands:\x1b[0m  :help  :clear  :quit");
    println!();
}