//! Yan 编译器命令行入口。
//!
//! CLI 只负责读取源码、编排编译阶段和展示诊断，不包含 lexer、parser、类型检查或执行规则。

use std::{
    collections::HashSet,
    env, fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

use yan_eval::execute;
use yan_hir::{lower, Program};
use yan_mir::lower as lower_mir;
use yan_source::{SourceFile, Span};
use yan_syntax::{lex, parse, SyntaxProgram};
use yan_typeck::{check, check_library, TypedProgram};

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
    match compile(path, false) {
        Ok(_) => {
            println!("{}: check succeeded", path.display());
            ExitCode::SUCCESS
        }
        Err(diagnostic) => render_diagnostic(&diagnostic),
    }
}

fn run_command(path: &Path) -> ExitCode {
    let compiled = match compile(path, true) {
        Ok(compiled) => compiled,
        Err(diagnostic) => return render_diagnostic(&diagnostic),
    };

    match execute(&compiled.mir) {
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

fn compile(path: &Path, require_main: bool) -> Result<CompiledProgram, Diagnostic> {
    let entry = read_module(path)?;
    let source_root = find_source_root(path);
    if entry.has_user_imports() && source_root.is_none() {
        return Err(Diagnostic {
            source: entry.source.clone(),
            span: Span::default(),
            message: "user module imports require a containing `src` directory".to_owned(),
        });
    }
    if let Some(source_root) = &source_root {
        validate_module_path(&entry, source_root)?;
    }

    let mut program = entry.program.clone();
    program.imports.retain(|path| is_platform_import(path));
    if let Some(source_root) = source_root {
        let mut visiting = HashSet::new();
        link_imports(&entry, &source_root, &mut visiting, &mut program)?;
    }
    let checked_as_library = !require_main
        && !program
            .functions
            .iter()
            .any(|function| function.name == "main");
    let typed = if checked_as_library {
        check_library(&program)
    } else {
        check(&program)
    }
    .map_err(|error| Diagnostic {
        source: entry.source.clone(),
        span: error.span,
        message: error.message,
    })?;
    // M14 先建立从 Typed HIR 到 MIR 的编译边界。当前解释器仍在下一子阶段迁移，
    // 因此这里只验证 MIR lowering 能覆盖已通过类型检查的每个函数。
    let mir = lower_mir(typed.clone());
    Ok(CompiledProgram {
        source: entry.source,
        typed,
        mir,
    })
}

/// 已解析并 lowering 的单个模块文件，同时保留模块解析所需的源信息。
struct ModuleFile {
    source: SourceFile,
    syntax: SyntaxProgram,
    program: Program,
}

impl ModuleFile {
    fn has_user_imports(&self) -> bool {
        self.program
            .imports
            .iter()
            .any(|path| !is_platform_import(path))
    }
}

fn read_module(path: &Path) -> Result<ModuleFile, Diagnostic> {
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
    let program = lower(syntax.clone()).map_err(|error| Diagnostic {
        source: source.clone(),
        span: error.span,
        message: error.message,
    })?;
    Ok(ModuleFile {
        source,
        syntax,
        program,
    })
}

fn find_source_root(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|ancestor| ancestor.file_name().is_some_and(|name| name == "src"))
        .map(Path::to_path_buf)
}

fn validate_module_path(module: &ModuleFile, source_root: &Path) -> Result<(), Diagnostic> {
    let relative = module
        .source
        .path()
        .strip_prefix(source_root)
        .map_err(|_| Diagnostic {
            source: module.source.clone(),
            span: Span::default(),
            message: "source file is outside its `src` directory".to_owned(),
        })?;
    let mut expected = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let Some(file_name) = expected.last_mut() else {
        return Err(Diagnostic {
            source: module.source.clone(),
            span: Span::default(),
            message: "source file path cannot define a module name".to_owned(),
        });
    };
    let Some(name) = file_name.strip_suffix(".yan") else {
        return Err(Diagnostic {
            source: module.source.clone(),
            span: Span::default(),
            message: "module source files must use the `.yan` extension".to_owned(),
        });
    };
    *file_name = name.to_owned();
    let declared = module
        .syntax
        .module
        .as_ref()
        .map(|path| path.segments.clone())
        .unwrap_or_else(|| expected.clone());
    if declared != expected {
        return Err(Diagnostic {
            source: module.source.clone(),
            span: module
                .syntax
                .module
                .as_ref()
                .map(|path| path.span)
                .unwrap_or_default(),
            message: format!(
                "module declaration must match path `{}`",
                expected.join(".")
            ),
        });
    }
    Ok(())
}

fn link_imports(
    module: &ModuleFile,
    source_root: &Path,
    visiting: &mut HashSet<PathBuf>,
    linked: &mut Program,
) -> Result<(), Diagnostic> {
    for import in &module.syntax.imports {
        if is_platform_import(&import.path.segments) {
            continue;
        }
        let Some((symbol, module_path)) = import.path.segments.split_last() else {
            continue;
        };
        if module_path.is_empty() {
            return Err(import_error(
                module,
                import.path.span,
                "import must name a module and symbol",
            ));
        }
        let file_path = module_file_path(source_root, module_path);
        if !visiting.insert(file_path.clone()) {
            return Err(import_error(
                module,
                import.path.span,
                "cyclic module imports are not supported",
            ));
        }
        if !file_path.is_file() {
            return Err(import_error(
                module,
                import.path.span,
                format!("imported module `{}` was not found", module_path.join(".")),
            ));
        }
        let dependency = read_module(&file_path)?;
        validate_module_path(&dependency, source_root)?;
        link_imports(&dependency, source_root, visiting, linked)?;
        append_public_symbol(&dependency, symbol, linked)
            .map_err(|message| import_error(module, import.path.span, message))?;
        visiting.remove(&file_path);
    }
    Ok(())
}

fn module_file_path(source_root: &Path, module_path: &[String]) -> PathBuf {
    let mut path = source_root.to_path_buf();
    for segment in module_path {
        path.push(segment);
    }
    path.set_extension("yan");
    path
}

fn append_public_symbol(
    module: &ModuleFile,
    symbol: &str,
    linked: &mut Program,
) -> Result<(), String> {
    if let Some(declaration) = module
        .program
        .newtypes
        .iter()
        .find(|declaration| declaration.name == symbol)
    {
        if !declaration.public {
            return Err(format!("imported symbol `{symbol}` is not public"));
        }
        linked.newtypes.push(declaration.clone());
        return Ok(());
    }
    if let Some(declaration) = module
        .program
        .structs
        .iter()
        .find(|declaration| declaration.name == symbol)
    {
        if !declaration.public {
            return Err(format!("imported symbol `{symbol}` is not public"));
        }
        linked.structs.push(declaration.clone());
        return Ok(());
    }
    if let Some(declaration) = module
        .program
        .enums
        .iter()
        .find(|declaration| declaration.name == symbol)
    {
        if !declaration.public {
            return Err(format!("imported symbol `{symbol}` is not public"));
        }
        linked.enums.push(declaration.clone());
        return Ok(());
    }
    if let Some(declaration) = module
        .program
        .functions
        .iter()
        .find(|declaration| declaration.name == symbol)
    {
        if !declaration.public {
            return Err(format!("imported symbol `{symbol}` is not public"));
        }
        linked.functions.push(declaration.clone());
        return Ok(());
    }
    Err(format!("imported symbol `{symbol}` was not found"))
}

fn is_platform_import(path: &[String]) -> bool {
    path.first().is_some_and(|segment| segment == "yan")
}

fn import_error(module: &ModuleFile, span: Span, message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        source: module.source.clone(),
        span,
        message: message.into(),
    }
}

/// 同时保存已检查 HIR 与其原始文本，供后续阶段复用相同的诊断坐标。
struct CompiledProgram {
    source: SourceFile,
    typed: TypedProgram,
    mir: yan_mir::Program,
}

#[derive(Debug)]
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{
        append_public_symbol, compile, find_source_root, read_module, validate_module_path, Program,
    };

    #[test]
    fn links_public_declarations_from_file_modules() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("examples/language-design/10-modules/src/examples/modules/application.yan");

        let compiled = compile(&path, true).expect("模块示例应完成链接与类型检查");
        assert!(compiled
            .typed
            .structs
            .iter()
            .any(|structure| structure.name == "Task"));
        assert!(compiled
            .typed
            .functions
            .iter()
            .any(|function| function.name == "rename_task"));
    }

    #[test]
    fn rejects_module_declaration_that_does_not_match_its_path() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(
            "examples/language-design/10-modules/src/examples/module_declaration/explicit.yan",
        );
        let source_root = find_source_root(&path).expect("fixture path must contain src");
        let mut module = read_module(&path).expect("fixture module must parse");
        module
            .syntax
            .module
            .as_mut()
            .expect("fixture must declare a module")
            .segments = vec!["wrong".to_owned()];

        let error = validate_module_path(&module, &source_root)
            .expect_err("mismatched module declaration must fail");
        assert!(error.message.contains("module declaration must match path"));
    }

    #[test]
    fn rejects_import_of_non_public_symbol() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("examples/language-design/10-modules/src/examples/modules/domain.yan");
        let mut module = read_module(&path).expect("fixture module must parse");
        module.program.structs[0].public = false;

        let error = append_public_symbol(
            &module,
            "Task",
            &mut Program {
                id: yan_hir::ModuleId(0),
                module: None,
                imports: Vec::new(),
                newtypes: Vec::new(),
                structs: Vec::new(),
                enums: Vec::new(),
                functions: Vec::new(),
            },
        )
        .expect_err("private declaration must not be imported");
        assert_eq!(error, "imported symbol `Task` is not public");
    }
}
