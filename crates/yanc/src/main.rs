//! Yan 编译器命令行入口。
//!
//! CLI 只负责读取源码、编排编译阶段和展示诊断，不包含 lexer、parser、类型检查或执行规则。

use std::{
    collections::HashSet,
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};

use yan_eval::execute;
use yan_hir::{lower_with_source, resolve_modules, ModuleGraph, ModuleId, ModuleInput, Program};
use yan_mir::{lower as lower_mir, verify as verify_mir, VerifiedProgram};
use yan_source::{SourceFile, SourceLocation, SourceMap, Span};
use yan_syntax::{lex, parse, SyntaxProgram};
use yan_typeck::{check, check_library};

/// `yanc` 的稳定帮助文本。CLI 面向用户的全部输出均使用英文。
const USAGE: &str = "Usage:\n  yanc check <file.yan>\n  yanc run <file.yan>\n  yanc build <file.yan>\n  yanc --help";

/// 已解析的 `yanc` 命令及其唯一文件参数。
///
/// `build` 在此阶段先占用稳定 CLI 契约；后端实际生成与 Cargo 调用将在后续任务接入，
/// 以避免命令分派绕过已验证 MIR 的编译管线。
enum Command {
    Help,
    Check(PathBuf),
    Run(PathBuf),
    Build(PathBuf),
    Invalid,
}

fn main() -> ExitCode {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    let mut stdout = io::stdout().lock();
    let mut stderr = io::stderr().lock();
    match dispatch(&arguments, &mut stdout, &mut stderr) {
        Ok(exit_code) => exit_code,
        // 无法写入 CLI 输出时不能安全地继续报告状态，直接使用标准失败状态退出。
        Err(_) => ExitCode::FAILURE,
    }
}

/// 按已解析命令执行 CLI 分派，并将帮助与参数错误写入指定输出流。
///
/// 显式传入输出流让帮助文本与退出码可在不启动子进程的情况下被回归测试，防止 `build`
/// 的参数错误意外写入标准输出或改变为成功状态。
fn dispatch(
    arguments: &[String],
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> io::Result<ExitCode> {
    match parse_command(arguments) {
        Command::Help => {
            writeln!(stdout, "{USAGE}")?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Check(path) => Ok(check_command(&path)),
        Command::Run(path) => Ok(run_command(&path)),
        Command::Build(path) => build_command(&path, stderr),
        Command::Invalid => {
            writeln!(stderr, "{USAGE}")?;
            Ok(ExitCode::from(2))
        }
    }
}

/// 将命令行实参归类为受支持命令，未提供文件的 `build` 与其他非法组合统一走帮助错误路径。
fn parse_command(arguments: &[String]) -> Command {
    match arguments {
        [command] if command == "--help" || command == "-h" => Command::Help,
        [command, path] if command == "check" => Command::Check(PathBuf::from(path)),
        [command, path] if command == "run" => Command::Run(PathBuf::from(path)),
        [command, path] if command == "build" => Command::Build(PathBuf::from(path)),
        _ => Command::Invalid,
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
        Err(error) => {
            let diagnostic = compiled.runtime_diagnostic(error);
            render_diagnostic(&diagnostic)
        }
    }
}

/// 报告尚未接入实际 Rust 生成的 `build` 占位诊断。
///
/// 此函数不得尝试调用 Cargo 或生成文件；M15 的后续任务会在 Verified MIR 后端可生成
/// 受控项目时替换此分支。现在保留既定诊断格式，避免向用户暴露内部未完成状态。
fn build_command(path: &Path, stderr: &mut dyn Write) -> io::Result<ExitCode> {
    render_diagnostic_to(
        &Diagnostic {
            source: SourceFile::new(path, ""),
            span: Span::default(),
            message: "backend build failed".to_owned(),
        },
        stderr,
    )
}

fn compile(path: &Path, require_main: bool) -> Result<CompiledProgram, Diagnostic> {
    let mut sources = SourceMap::new();
    let entry = read_module(path, &mut sources)?;
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

    let mut modules = vec![entry];
    if let Some(source_root) = source_root {
        let mut visiting = HashSet::new();
        collect_imported_modules(0, &source_root, &mut visiting, &mut modules, &mut sources)?;
    }
    let graph = ModuleGraph::new(
        modules
            .into_iter()
            .enumerate()
            .map(|(index, module)| ModuleInput::new(ModuleId(index as u32), module.program))
            .collect(),
        ModuleId(0),
    );
    let resolved = resolve_modules(graph)
        .map_err(|error| diagnostic_at(&sources, error.location, error.message))?;
    let program = resolved
        .entry_program()
        .map_err(|error| diagnostic_at(&sources, error.location, error.message))?;
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
    .map_err(|error| diagnostic_at(&sources, error.location, error.message))?;
    let lowered =
        lower_mir(typed).map_err(|error| diagnostic_at(&sources, error.location, error.message))?;
    let mir = verify_mir(lowered)
        .map_err(|error| diagnostic_at(&sources, error.location, error.message))?;
    Ok(CompiledProgram { mir, sources })
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

fn read_module(path: &Path, sources: &mut SourceMap) -> Result<ModuleFile, Diagnostic> {
    let text = fs::read_to_string(path).map_err(|_| Diagnostic {
        source: SourceFile::new(path, ""),
        span: Span::default(),
        // 不直接输出操作系统提供的错误文本，避免 CLI 在不同系统语言下产生不稳定文案。
        message: "failed to read file".to_owned(),
    })?;
    let source = SourceFile::new(path, text);
    let source_id = sources.insert(source.clone());
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
    let program = lower_with_source(syntax.clone(), source_id).map_err(|error| Diagnostic {
        source: source.clone(),
        span: error.location.span,
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

fn collect_imported_modules(
    module_index: usize,
    source_root: &Path,
    visiting: &mut HashSet<PathBuf>,
    modules: &mut Vec<ModuleFile>,
    sources: &mut SourceMap,
) -> Result<(), Diagnostic> {
    let imports = modules[module_index].syntax.imports.clone();
    for import in &imports {
        if is_platform_import(&import.path.segments) {
            continue;
        }
        let Some((_, module_path)) = import.path.segments.split_last() else {
            continue;
        };
        if module_path.is_empty() {
            return Err(import_error(
                &modules[module_index],
                import.path.span,
                "import must name a module and symbol",
            ));
        }
        let file_path = module_file_path(source_root, module_path);
        if !visiting.insert(file_path.clone()) {
            return Err(import_error(
                &modules[module_index],
                import.path.span,
                "cyclic module imports are not supported",
            ));
        }
        if !file_path.is_file() {
            return Err(import_error(
                &modules[module_index],
                import.path.span,
                format!("imported module `{}` was not found", module_path.join(".")),
            ));
        }
        if modules
            .iter()
            .any(|candidate| candidate.source.path() == file_path)
        {
            visiting.remove(&file_path);
            continue;
        }
        let dependency = read_module(&file_path, sources)?;
        validate_module_path(&dependency, source_root)?;
        let dependency_index = modules.len();
        modules.push(dependency);
        collect_imported_modules(dependency_index, source_root, visiting, modules, sources)?;
        visiting.remove(&file_path);
    }
    Ok(())
}

fn diagnostic_at(sources: &SourceMap, location: SourceLocation, message: String) -> Diagnostic {
    let source = sources
        .get(location.source)
        .cloned()
        .unwrap_or_else(|| SourceFile::new("<internal>", ""));
    Diagnostic {
        source,
        span: location.span,
        message: if sources.get(location.source).is_some() {
            message
        } else {
            "invalid source location".to_owned()
        },
    }
}

fn module_file_path(source_root: &Path, module_path: &[String]) -> PathBuf {
    let mut path = source_root.to_path_buf();
    for segment in module_path {
        path.push(segment);
    }
    path.set_extension("yan");
    path
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

/// 保存已验证 MIR 与本次编译会话的源文件表，供执行阶段复用相同的诊断坐标。
struct CompiledProgram {
    mir: VerifiedProgram,
    sources: SourceMap,
}

impl CompiledProgram {
    /// 将解释器的 MIR 源位置转换为 CLI 可展示的稳定诊断。
    ///
    /// MIR 的位置可能来自任意已导入模块，不能回退到入口文件；未知源 ID 则保留编译期
    /// 既有的 `invalid source location` 文案，避免泄露内部实现细节。
    fn runtime_diagnostic(&self, error: yan_eval::EvalError) -> Diagnostic {
        diagnostic_at(&self.sources, error.location, error.message)
    }
}

#[derive(Debug)]
struct Diagnostic {
    source: SourceFile,
    span: Span,
    message: String,
}

fn render_diagnostic(diagnostic: &Diagnostic) -> ExitCode {
    let mut stderr = io::stderr().lock();
    match render_diagnostic_to(diagnostic, &mut stderr) {
        Ok(exit_code) => exit_code,
        Err(_) => ExitCode::FAILURE,
    }
}

/// 将诊断写入指定错误流，使 CLI 分派可验证标准错误与退出码的稳定契约。
fn render_diagnostic_to(diagnostic: &Diagnostic, stderr: &mut dyn Write) -> io::Result<ExitCode> {
    // 编译阶段产生的 span 均来自 SourceFile；无效位置只可能来自读取文件失败这类无源码场景。
    let (line, column) = diagnostic
        .source
        .line_column(diagnostic.span.start)
        .unwrap_or((1, 1));
    writeln!(
        stderr,
        "error: {}:{line}:{column}: {}",
        diagnostic.source.path().display(),
        diagnostic.message
    )?;
    Ok(ExitCode::FAILURE)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::fs;
    use std::path::Path;
    use std::process::ExitCode;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{compile, dispatch, find_source_root, read_module, validate_module_path, USAGE};
    use yan_eval::execute;
    use yan_source::SourceMap;

    #[test]
    fn usage_lists_build_and_build_without_file_prints_usage_to_stderr(
    ) -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            USAGE,
            "Usage:\n  yanc check <file.yan>\n  yanc run <file.yan>\n  yanc build <file.yan>\n  yanc --help"
        );
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let exit_code = dispatch(&["build".to_owned()], &mut stdout, &mut stderr)?;

        assert_eq!(exit_code, ExitCode::from(2));
        assert!(stdout.is_empty());
        assert_eq!(String::from_utf8(stderr)?, format!("{USAGE}\n"));
        Ok(())
    }

    #[test]
    fn build_with_file_prints_the_stable_backend_diagnostic_to_stderr(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let path = "example.yan";
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let exit_code = dispatch(
            &["build".to_owned(), path.to_owned()],
            &mut stdout,
            &mut stderr,
        )?;

        assert_ne!(exit_code, ExitCode::SUCCESS);
        assert!(stdout.is_empty());
        assert_eq!(
            String::from_utf8(stderr)?,
            "error: example.yan:1:1: backend build failed\n"
        );
        Ok(())
    }

    #[test]
    fn links_public_declarations_from_file_modules() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("examples/language-design/10-modules/src/examples/modules/application.yan");

        let compiled = compile(&path, true).expect("模块示例应完成链接与类型检查");
        assert!(compiled
            .mir
            .functions()
            .iter()
            .any(|function| function.name == "rename_task"));
        let function_ids = compiled
            .mir
            .functions()
            .iter()
            .map(|function| function.id)
            .collect::<HashSet<_>>();
        assert_eq!(function_ids.len(), compiled.mir.functions().len());
    }

    #[test]
    fn executes_m2_to_m13_fixture_matrix() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for (fixture, expected) in [
            ("examples/language-design/01-data-types/01_variables_and_bindings.yan", &["Yan", "1"][..]),
            ("examples/language-design/01-data-types/02_int.yan", &["100"][..]),
            ("examples/language-design/01-data-types/03_bool.yan", &["true"][..]),
            ("examples/language-design/01-data-types/04_string.yan", &["Yan"][..]),
            ("examples/language-design/01-data-types/05_list.yan", &["[cli, web]"][..]),
            ("examples/language-design/01-data-types/06_unit.yan", &["started"][..]),
            ("examples/language-design/01-data-types/07_bytes.yan", &["0xa13f"][..]),
            ("examples/language-design/01-data-types/08_map.yan", &["{http: 80, https: 443}"][..]),
            ("examples/language-design/01-data-types/09_float.yan", &["0.10"][..]),
            ("examples/language-design/02-functions/01_functions.yan", &["total: 597"][..]),
            ("examples/language-design/03-structs/01_structs.yan", &["Lin"][..]),
            ("examples/language-design/04-enums-and-match/01_enums_and_match.yan", &["succeeded"][..]),
            ("examples/language-design/05-option/01_option.yan", &["Lin"][..]),
            ("examples/language-design/06-result/01_result.yan", &[][..]),
            ("examples/language-design/07-collections/02_tuples_and_destructuring.yan", &["Lin Yan"][..]),
            ("examples/language-design/08-conditions/01_if.yan", &["ready"][..]),
            ("examples/language-design/09-loops/01_for.yan", &["cli", "web"][..]),
            ("examples/language-design/10-modules/src/examples/modules/application.yan", &["approve Yan syntax"][..]),
            ("examples/language-design/13-mutation-and-visibility/01_mut.yan", &["2"][..]),
            ("examples/language-design/13-mutation-and-visibility/src/examples/visibility/application.yan", &["visible"][..]),
        ] {
            let compiled = compile(&workspace.join(fixture), true)
                .unwrap_or_else(|error| panic!("{fixture} must compile: {}", error.message));
            assert_eq!(execute(&compiled.mir).expect("fixture must execute"), expected, "{fixture}");
        }
    }

    #[test]
    fn rejects_module_declaration_that_does_not_match_its_path() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(
            "examples/language-design/10-modules/src/examples/module_declaration/explicit.yan",
        );
        let source_root = find_source_root(&path).expect("fixture path must contain src");
        let mut sources = SourceMap::new();
        let mut module = read_module(&path, &mut sources).expect("fixture module must parse");
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
    fn cross_module_diagnostic_uses_imported_source_file() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("测试时钟必须晚于 Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "yan-m14-cross-module-{}-{unique}",
            std::process::id()
        ));
        let source_root = root.join("src");
        fs::create_dir_all(&source_root).expect("测试 src 目录必须创建成功");
        let entry = source_root.join("app.yan");
        let imported = source_root.join("library.yan");
        fs::write(
            &entry,
            "module app\nimport library.broken\nfn main() -> unit {\n  broken()\n}\n",
        )
        .expect("入口测试源码必须写入成功");
        fs::write(
            &imported,
            "module library\npub fn broken() -> unit {\n  missing\n}\n",
        )
        .expect("导入测试源码必须写入成功");

        let error = match compile(&entry, false) {
            Err(error) => error,
            Ok(_) => panic!("导入模块的未定义变量必须失败"),
        };
        assert_eq!(error.source.path(), imported.as_path());
        assert_eq!(error.source.line_column(error.span.start), Some((3, 3)));
        assert_eq!(error.message, "undefined variable `missing`");

        fs::remove_dir_all(&root).expect("测试临时目录必须清理成功");
    }

    #[test]
    fn cross_module_runtime_diagnostic_uses_imported_source_file() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("测试时钟必须晚于 Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "yan-m14-cross-module-runtime-{}-{unique}",
            std::process::id()
        ));
        let source_root = root.join("src");
        fs::create_dir_all(&source_root).expect("测试 src 目录必须创建成功");
        let entry = source_root.join("app.yan");
        let imported = source_root.join("library.yan");
        fs::write(
            &entry,
            "module app\nimport library.overflow\nfn main() -> unit {\n  let value = overflow()\n}\n",
        )
        .expect("入口测试源码必须写入成功");
        fs::write(
            &imported,
            "module library\npub fn overflow() -> int {\n  9223372036854775807 + 1\n}\n",
        )
        .expect("导入测试源码必须写入成功");

        let compiled = compile(&entry, true).expect("运行时错误 fixture 必须通过编译");
        let error = execute(&compiled.mir).expect_err("导入模块的整数溢出必须执行失败");
        let diagnostic = compiled.runtime_diagnostic(error);
        assert_eq!(diagnostic.source.path(), imported.as_path());
        assert_eq!(
            diagnostic.source.line_column(diagnostic.span.start),
            Some((3, 3))
        );
        assert_eq!(diagnostic.message, "integer addition overflow");

        fs::remove_dir_all(&root).expect("测试临时目录必须清理成功");
    }

    #[test]
    fn imported_default_diagnostic_uses_declaration_source_file() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("测试时钟必须晚于 Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "yan-m14-imported-default-{}-{unique}",
            std::process::id()
        ));
        let source_root = root.join("src");
        fs::create_dir_all(&source_root).expect("测试 src 目录必须创建成功");
        let entry = source_root.join("app.yan");
        let imported = source_root.join("settings.yan");
        fs::write(
            &entry,
            "module app\nimport settings.Config\nfn main() -> unit { }\n",
        )
        .expect("入口测试源码必须写入成功");
        fs::write(
            &imported,
            "module settings\npub struct Config {\n  port: int = \"bad\"\n}\n",
        )
        .expect("导入测试源码必须写入成功");

        let error = match compile(&entry, false) {
            Err(error) => error,
            Ok(_) => panic!("导入声明的错误默认值必须失败"),
        };
        assert_eq!(error.source.path(), imported.as_path());
        assert_eq!(error.source.line_column(error.span.start), Some((3, 3)));
        assert_eq!(
            error.message,
            "default value for field `port` does not match its type"
        );

        fs::remove_dir_all(&root).expect("测试临时目录必须清理成功");
    }

    #[test]
    fn private_and_missing_import_diagnostics_use_exact_import_location() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("测试时钟必须晚于 Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "yan-m14-private-import-{}-{unique}",
            std::process::id()
        ));
        let source_root = root.join("src");
        fs::create_dir_all(&source_root).expect("测试 src 目录必须创建成功");
        let entry = source_root.join("app.yan");
        let imported = source_root.join("library.yan");
        fs::write(
            &entry,
            "module app\nimport library.hidden\nfn main() -> unit { }\n",
        )
        .expect("入口测试源码必须写入成功");
        fs::write(&imported, "module library\nfn hidden() -> unit { }\n")
            .expect("导入测试源码必须写入成功");

        let error = match compile(&entry, false) {
            Err(error) => error,
            Ok(_) => panic!("私有符号导入必须失败"),
        };
        assert_eq!(error.source.path(), entry.as_path());
        assert_eq!(error.source.line_column(error.span.start), Some((2, 8)));
        assert_eq!(error.message, "imported symbol `hidden` is not public");

        fs::write(
            &entry,
            "module app\nimport library.absent\nfn main() -> unit { }\n",
        )
        .expect("缺失符号入口源码必须写入成功");
        let error = match compile(&entry, false) {
            Err(error) => error,
            Ok(_) => panic!("缺失符号导入必须失败"),
        };
        assert_eq!(error.source.path(), entry.as_path());
        assert_eq!(error.source.line_column(error.span.start), Some((2, 8)));
        assert_eq!(error.message, "imported symbol `absent` was not found");

        fs::remove_dir_all(&root).expect("测试临时目录必须清理成功");
    }
}
