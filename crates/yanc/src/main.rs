use std::{env, fs, path::Path, process::ExitCode};

use yan_source::SourceFile;
use yan_syntax::lex;

const USAGE: &str = "Usage:\n  yanc check <file.yan>\n  yanc --help";

fn main() -> ExitCode {
    match env::args().skip(1).collect::<Vec<_>>().as_slice() {
        [command] if command == "--help" || command == "-h" => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        [command, path] if command == "check" => check(Path::new(path)),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn check(path: &Path) -> ExitCode {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("{}: {error}", path.display());
            return ExitCode::FAILURE;
        }
    };
    let source = SourceFile::new(path, text);

    match lex(source.text()) {
        Ok(tokens) => {
            println!(
                "{}: checked {} tokens",
                source.path().display(),
                tokens.len()
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            // lexer 仅从同一份不可变源码生成 span，因此错误起点必定是有效 UTF-8 边界；
            // 若该不变量被破坏，说明是编译器缺陷，而不是用户输入错误。
            let (line, column) = source
                .line_column(error.span.start)
                .expect("lexer 产生的 span 必须是有效的源码偏移");
            eprintln!(
                "{}:{line}:{column}: error: {}",
                source.path().display(),
                error.message
            );
            ExitCode::FAILURE
        }
    }
}
