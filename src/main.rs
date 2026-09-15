#![allow(warnings)]

use std::process::Command;

mod ast;
mod codegen;
mod error;
mod lexer;
mod parser;
mod semantic;
mod token;

fn print_usage(program_name: &std::ffi::OsStr) {
    eprintln!(
        "Usage: {} [--print-ast] <source-file>",
        program_name.to_string_lossy()
    );
}

fn main() {
    let mut args = std::env::args_os();
    let program_name = args.next().unwrap_or_else(|| "lcc".into());
    let mut filename = None;
    let mut print_ast = false;

    for arg in args {
        if arg == "--print-ast" {
            print_ast = true;
        } else if filename.is_none() {
            filename = Some(arg);
        } else {
            print_usage(&program_name);
            std::process::exit(2);
        }
    }

    let Some(filename) = filename else {
        print_usage(&program_name);
        std::process::exit(2);
    };

    let source = match std::fs::read_to_string(&filename) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("Failed to read {}: {error}", filename.to_string_lossy());
            std::process::exit(1);
        }
    };

    let filename = filename.to_string_lossy().to_string();

    let lexer = lexer::Lexer::new(&source);

    let mut parser = parser::Parser::new(lexer, &filename);
    let tree = match parser.parse_translation_unit() {
        Ok(program) => program,
        Err(error) => {
            error.print_error();
            std::process::exit(1);
        }
    };

    if print_ast {
        let output = ast::pretty_printer::PrettyPrinter::new().print(&tree);
        print!("{output}");
    }

    let semantic_analyzer = semantic::SemanticAnalyzer::new(&tree, &source, &filename);

    let tir = match semantic_analyzer.analyze() {
        Ok(tir) => tir,

        Err(error) => {
            error.print_error();
            std::process::exit(1);
        }
    };

    let assembly = codegen::Codegen::new().generate(&tir);
    let assembly_path = std::path::Path::new(&filename).with_extension("s");
    let executable_path = std::path::Path::new(&filename).with_extension("");

    if let Err(error) = std::fs::write(&assembly_path, assembly) {
        eprintln!("Failed to write assembly: {error}");
        std::process::exit(1);
    }

    // use rosetta 2 on arm machines
    let result = if cfg!(target_arch = "aarch64") {
        Command::new("arch")
            .arg("-x86_64")
            .arg("clang")
            .arg(&assembly_path)
            .arg("-o")
            .arg(&executable_path)
            .status()
    } else {
        Command::new("clang")
            .arg(&assembly_path)
            .arg("-o")
            .arg(&executable_path)
            .status()
    };

    match result {
        Ok(status) if status.success() => {
            let _ = std::fs::remove_file(&assembly_path);
        }

        Ok(status) => {
            let _ = std::fs::remove_file(&assembly_path);

            eprintln!("clang failed with status: {status}");
            std::process::exit(1);
        }

        Err(error) => {
            let _ = std::fs::remove_file(&assembly_path);

            eprintln!("Failed to start clang: {error}");
            std::process::exit(1);
        }
    }
}
