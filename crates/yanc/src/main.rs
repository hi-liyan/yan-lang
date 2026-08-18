//! Yan 编译器命令行入口。
//!
//! CLI 只负责读取源码、编排编译阶段和展示诊断，不包含 lexer、parser、类型检查或执行规则。

use std::{env, fs, path::Path, process::ExitCode};

use yan_eval::execute;
use yan_hir::{lower, Program};
use yan_source::{SourceFile, Span};
use yan_syntax::{lex, parse};
use yan_typeck::check;

/// `yanc` 的稳定帮助文本。CLI 面向用户的全部输出均使用英文。
const USAGE: &str = "Usage:\n  yanc check <file.yan>\n  yanc run <file.yan>\n  yanc --help";

fn main() -> ExitCode {
    match env::args().skip(1).collect::<Vec<_>>().as_slice() {
        [command] if command == "--help" || command == "-h" => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        [command, path] if command == "check" => check_command(Path::new(path)),
        [command, path] if command == "run" => run_command(Path::new(path)),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn check_command(path: &Path) -> ExitCode {
    match compile(path) {
        Ok(_) => {
            println!("{}: check succeeded", path.display());
            ExitCode::SUCCESS
        }
        Err(diagnostic) => render_diagnostic(&diagnostic),
    }
}

fn run_command(path: &Path) -> ExitCode {
    let compiled = match compile(path) {
        Ok(compiled) => compiled,
        Err(diagnostic) => return render_diagnostic(&diagnostic),
    };

    match execute(&compiled.program) {
        Ok(lines) => {
            for line in lines {
                println!("{line}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => render_diagnostic(&Diagnostic {
            source: compiled.source,
            span: error.span,
            message: error.message,
        }),
    }
}

fn compile(path: &Path) -> Result<CompiledProgram, Diagnostic> {
    let text = fs::read_to_string(path).map_err(|_| Diagnostic {
        source: SourceFile::new(path, ""),
        span: Span::default(),
        // 不直接输出操作系统提供的错误文本，避免 CLI 在不同系统语言下产生不稳定文案。
        message: "failed to read file".to_owned(),
    })?;
    let source = SourceFile::new(path, text);
    let tokens = lex(source.text()).map_err(|error| Diagnostic {
        source: source.clone(),
        span: error.span,
        message: error.message,
    })?;
    let syntax = parse(source.text(), &tokens).map_err(|error| Diagnostic {
        source: source.clone(),
        span: error.span,
        message: error.message,
    })?;
    let program = lower(syntax).map_err(|error| Diagnostic {
        source: source.clone(),
        span: error.span,
        message: error.message,
    })?;
    check(&program).map_err(|error| Diagnostic {
        source: source.clone(),
        span: error.span,
        message: error.message,
    })?;
    Ok(CompiledProgram { source, program })
}

/// 同时保存已检查 HIR 与其原始文本，供后续阶段复用相同的诊断坐标。
struct CompiledProgram {
    source: SourceFile,
    program: Program,
}

struct Diagnostic {
    source: SourceFile,
    span: Span,
    message: String,
}

fn render_diagnostic(diagnostic: &Diagnostic) -> ExitCode {
    // 编译阶段产生的 span 均来自 SourceFile；无效位置只可能来自读取文件失败这类无源码场景。
    let (line, column) = diagnostic
        .source
        .line_column(diagnostic.span.start)
        .unwrap_or((1, 1));
    eprintln!(
        "error: {}:{line}:{column}: {}",
        diagnostic.source.path().display(),
        diagnostic.message
    );
    ExitCode::FAILURE
}
