//! Yan 编译器命令行入口。
//!
//! CLI 只负责读取源码、编排编译阶段和展示诊断，不包含 lexer、parser、类型检查或执行规则。

use std::{
    collections::HashSet,
    env, fs,
    hash::{Hash, Hasher},
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command as ProcessCommand, ExitCode, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use yan_eval::execute;
use yan_hir::{lower_with_source, resolve_modules, ModuleGraph, ModuleId, ModuleInput, Program};
use yan_mir::{lower as lower_mir, verify as verify_mir, VerifiedProgram};
use yan_rust_backend::generate;
use yan_source::{SourceFile, SourceLocation, SourceMap, Span};
use yan_syntax::{lex, parse, SyntaxProgram};
use yan_typeck::{check, check_library};

/// `yanc` 的稳定帮助文本。CLI 面向用户的全部输出均使用英文。
const USAGE: &str = "Usage:\n  yanc check <file.yan>\n  yanc run <file.yan>\n  yanc build <file.yan>\n  yanc --help";

/// 已解析的 `yanc` 命令及其唯一文件参数。
///
/// `build` 仅在前端产出已验证 MIR 后生成编译器控制的 Cargo 项目、调用 Cargo，并发布
/// 可执行文件，避免命令分派绕过既有编译管线或接受用户控制的 Rust 构建配置。
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
        Command::Build(path) => build_command(&path, stdout, stderr),
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

/// 生成受控 Cargo 项目并发布其可执行文件。
///
/// 前端诊断保留原有位置；生成、物化或 Cargo 失败只报告入口文件的稳定后端诊断，不能
/// 泄露子进程输出或本机工具链细节。
fn build_command(
    path: &Path,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> io::Result<ExitCode> {
    match compile(path, true) {
        // 前端错误必须保留其模块来源与精确位置，不能被后端尚未实现的占位错误覆盖。
        Err(diagnostic) => render_diagnostic_to(&diagnostic, stderr),
        Ok(compiled) => match build_binary(path, &compiled.mir) {
            Ok(binary_path) => {
                writeln!(
                    stdout,
                    "{}: build succeeded: {}",
                    path.display(),
                    binary_path.display()
                )?;
                Ok(ExitCode::SUCCESS)
            }
            Err(()) => backend_build_diagnostic(path, stderr),
        },
    }
}

/// 将生成器输出写入稳定且仅由编译器拥有的 Cargo 项目，并调用固定 Cargo 命令。
fn build_binary(path: &Path, program: &VerifiedProgram) -> Result<PathBuf, ()> {
    let generated = generate(program).map_err(|_| ())?;
    let entry_path = path.canonicalize().map_err(|_| ())?;
    let source = fs::read_to_string(&entry_path).map_err(|_| ())?;
    let build_root = workspace_target_dir()
        .join("yan")
        .join(build_hash(&entry_path, &source));
    let _build_lock = BuildLock::acquire(&build_root)?;
    let cargo_root = build_root.join("cargo");
    materialize_project(&cargo_root, &generated, &runtime_path()?)?;

    let manifest_path = cargo_root.join("Cargo.toml");
    let isolation = CargoIsolation::create()?;
    let status = cargo_build_command(&isolation, &manifest_path)?
        .status()
        .map_err(|_| ())?;
    if !status.success() {
        return Err(());
    }

    let entry_stem = entry_path.file_stem().ok_or(())?;
    let binary_path = build_root.join("bin").join(format!(
        "{}{}",
        entry_stem.to_string_lossy(),
        env::consts::EXE_SUFFIX
    ));
    let generated_binary = cargo_root
        .join("target")
        .join("debug")
        .join(format!("yan-generated{}", env::consts::EXE_SUFFIX));
    fs::create_dir_all(binary_path.parent().ok_or(())?).map_err(|_| ())?;
    fs::copy(generated_binary, &binary_path).map_err(|_| ())?;
    Ok(binary_path)
}

/// 在编译器锁保护下写入固定项目文件。
///
/// Windows 无法安全地原子替换非空目录，因此不删除旧目录或使用固定临时名称；生成文本
/// 已在写入前完成，随后只覆写本函数拥有的两个固定文件。
fn materialize_project(
    cargo_root: &Path,
    generated: &yan_rust_backend::GeneratedProgram,
    runtime_path: &Path,
) -> Result<(), ()> {
    fs::create_dir_all(cargo_root.join("src")).map_err(|_| ())?;
    let runtime_path = runtime_path.to_string_lossy().replace('\\', "\\\\");
    let manifest = generated
        .manifest_toml
        .replace("__YAN_RUNTIME_PATH__", &runtime_path);
    fs::write(cargo_root.join("Cargo.toml"), manifest).map_err(|_| ())?;
    fs::write(cargo_root.join("src").join("main.rs"), &generated.main_rs).map_err(|_| ())
}

/// 构造不继承用户 Cargo 配置或包装器的固定构建进程。
fn cargo_build_command(
    isolation: &CargoIsolation,
    manifest_path: &Path,
) -> Result<ProcessCommand, ()> {
    let mut command = ProcessCommand::new("cargo");
    command
        // MSVC 工具链依赖进程环境中的安装信息，不能清空环境；Cargo 配置与 wrapper
        // 变量仍须显式覆盖或移除，避免用户构建配置影响编译器拥有的生成项目。
        .env("CARGO_HOME", &isolation.cargo_home)
        .env_remove("CARGO_TARGET_DIR")
        .env_remove("CARGO_BUILD_TARGET_DIR")
        .env_remove("CARGO_BUILD_TARGET")
        .env_remove("CARGO_BUILD_RUSTC_WRAPPER")
        .env_remove("CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER")
        .env_remove("CARGO_BUILD_RUSTFLAGS")
        .env_remove("CARGO_BUILD_RUSTC")
        .env_remove("RUSTC")
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("RUSTC_WRAPPER")
        .env_remove("RUSTC_WORKSPACE_WRAPPER")
        .current_dir(&isolation.cwd)
        .args(["build", "--quiet", "--manifest-path"])
        .arg(manifest_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    remove_cargo_target_configuration_environment(
        &mut command,
        env::vars_os().map(|(name, _)| name),
    );
    Ok(command)
}

/// 移除 Cargo target 专用环境配置，保留 MSVC 所需的系统工具链环境。
///
/// Cargo 会读取 `CARGO_TARGET_<TRIPLE>_*`，其中包括 linker 与 rustflags。它们与
/// `CARGO_BUILD_*` 一样属于用户可控的构建配置，但 `PATH`、`LIB` 和 `INCLUDE` 不属于
/// 此前缀，必须保留以便 MSVC 找到系统链接器与库。
fn remove_cargo_target_configuration_environment(
    command: &mut ProcessCommand,
    names: impl IntoIterator<Item = std::ffi::OsString>,
) {
    for name in names {
        if name
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("CARGO_TARGET_")
        {
            command.env_remove(name);
        }
    }
}

/// Cargo 子进程的系统临时隔离目录。
///
/// 生成项目仍保留在 Yan workspace 的 `target/yan` 下，但 Cargo 的当前目录与 Home 位于
/// workspace 外，防止其从 Yan 项目的父目录发现 `.cargo/config.toml`。
struct CargoIsolation {
    cwd: PathBuf,
    cargo_home: PathBuf,
}

impl CargoIsolation {
    fn create() -> Result<Self, ()> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ())?
            .as_nanos();
        let root = cargo_isolation_root()?;
        fs::create_dir_all(&root).map_err(|_| ())?;
        rejects_cargo_configuration_in_ancestors(&root)?;
        let created_cwd = root.join(format!("cargo-{}-{unique}", std::process::id()));
        let cargo_home = created_cwd.join("cargo-home");

        // 独占创建本次 cwd，避免同名目录已被其他进程预先写入 Cargo 配置时继续执行。
        fs::create_dir(&created_cwd).map_err(|_| ())?;
        if fs::create_dir(&cargo_home).is_err() {
            let _ = fs::remove_dir_all(&created_cwd);
            return Err(());
        }

        // Windows junction 或被改写的环境路径可能令新目录解析到另一条祖先路径。
        // 必须按传给子进程的规范化 cwd 再次检查，否则 Cargo 的配置发现边界会与检查结果不一致。
        let cwd = match created_cwd.canonicalize() {
            Ok(cwd)
                if rejects_cargo_configuration_in_ancestors(&cwd).is_ok()
                    && rejects_cargo_home_configuration(&cwd.join("cargo-home")).is_ok() =>
            {
                cwd
            }
            _ => {
                let _ = fs::remove_dir_all(&created_cwd);
                return Err(());
            }
        };
        let cargo_home = cwd.join("cargo-home");
        Ok(Self { cwd, cargo_home })
    }
}

/// 返回 Cargo 进程配置发现的隔离根目录。
fn cargo_isolation_root() -> Result<PathBuf, ()> {
    #[cfg(windows)]
    {
        // `%TEMP%` 通常位于用户 Profile 内，Cargo 会从其父目录发现用户 `.cargo`。
        // Public profile 不经过当前用户的 Home，能隔离正常用户 Cargo Home 的配置搜索。
        let public = env::var_os("PUBLIC").ok_or(())?;
        return cargo_isolation_root_from_public(Path::new(&public));
    }
    #[cfg(not(windows))]
    Ok(env::temp_dir().join("yanc"))
}

/// 基于 Windows Public profile 返回 Cargo 配置发现的隔离根目录。
///
/// `PUBLIC` 是进程环境变量，可能被调用者改写。只有已存在的绝对目录才可作为 Cargo
/// 工作目录根；先规范化可防止相对路径被 `current_dir` 按 Yan workspace 重新解释。
#[cfg(windows)]
fn cargo_isolation_root_from_public(public: &Path) -> Result<PathBuf, ()> {
    if !public.is_absolute() {
        return Err(());
    }
    Ok(public.canonicalize().map_err(|_| ())?.join("yanc"))
}

/// 拒绝 Cargo 当前目录祖先中的项目配置。
///
/// `CARGO_HOME` 不能阻止 Cargo 向上搜索 `.cargo/config.toml`。发现任一配置时必须在
/// 启动子进程前失败，避免用户或宿主机目录配置穿透 Yan 后端边界。
fn rejects_cargo_configuration_in_ancestors(cwd: &Path) -> Result<(), ()> {
    let mut current = Some(cwd);
    while let Some(directory) = current {
        let cargo = directory.join(".cargo");
        if cargo.join("config.toml").is_file() || cargo.join("config").is_file() {
            return Err(());
        }
        current = directory.parent();
    }
    Ok(())
}

/// 拒绝受控 `CARGO_HOME` 根目录直接加载的配置文件。
///
/// Cargo 除了搜索 cwd 的祖先目录外，还会直接读取 `CARGO_HOME/config.toml` 和旧版
/// `config`；这两个文件不位于 `.cargo` 子目录，因此必须单独检查。
fn rejects_cargo_home_configuration(cargo_home: &Path) -> Result<(), ()> {
    if cargo_home.join("config.toml").is_file() || cargo_home.join("config").is_file() {
        return Err(());
    }
    Ok(())
}

impl Drop for CargoIsolation {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.cwd);
    }
}

/// 单次构建持有的目录锁，防止相同哈希的构建并发覆写受控目录。
struct BuildLock {
    path: PathBuf,
}

impl BuildLock {
    /// 创建唯一锁目录；已存在时直接失败，以免不确定所有者的生成物被覆盖。
    fn acquire(build_root: &Path) -> Result<Self, ()> {
        fs::create_dir_all(build_root).map_err(|_| ())?;
        let path = build_root.join(".lock");
        fs::create_dir(&path).map_err(|_| ())?;
        Ok(Self { path })
    }
}

impl Drop for BuildLock {
    fn drop(&mut self) {
        // 锁目录只由当前进程通过 create_dir 获得，Drop 只删除这一精确目录。
        let _ = fs::remove_dir(&self.path);
    }
}

/// 计算构建目录名；入口绝对路径与入口源码共同决定产物，避免同名文件相互覆盖。
fn build_hash(entry_path: &Path, source: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    entry_path.hash(&mut hasher);
    source.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// 返回工作区内由 `yanc` 控制的生成根目录，不从用户输入或环境变量接收输出位置。
fn workspace_target_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("target")
}

/// 返回随编译器发布的固定运行时 crate 的规范路径。
fn runtime_path() -> Result<PathBuf, ()> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../yan-runtime")
        .canonicalize()
        .map_err(|_| ())
}

/// 将后端阶段的任意内部失败统一转换为入口文件的位置稳定诊断。
fn backend_build_diagnostic(path: &Path, stderr: &mut dyn Write) -> io::Result<ExitCode> {
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
    compile_with_internal_modules(entry, Vec::new(), require_main, sources)
}

/// 使用附加的编译器内部模块完成既有前端与后端编译流程。
///
/// CLI 始终传入空列表；此参数仅供编译器拥有的模块在 M16 接入前验证其能够使用同一
/// 模块图、类型检查与 MIR 后端流程，不能由用户路径、配置或导入填充。
fn compile_with_internal_modules(
    entry: ModuleFile,
    internal_modules: Vec<ModuleFile>,
    require_main: bool,
    mut sources: SourceMap,
) -> Result<CompiledProgram, Diagnostic> {
    reject_reserved_yan_std_imports(&entry)?;
    let source_root = find_source_root(entry.source.path());
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
    modules.extend(internal_modules);
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
        if is_reserved_yan_std_import(&import.path.segments) {
            return Err(import_error(
                &modules[module_index],
                import.span,
                "reserved module namespace `yan.std`",
            ));
        }
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

/// 拒绝用户模块在 M15 提前导入仅供 M16 使用的内部标准库根命名空间。
///
/// 此检查位于文件系统模块收集之前，使保留命名空间不因被视为 platform import 而绕过
/// 诊断。编译器拥有的内部模块不经过本函数，因而未来仍可通过同一模块图加入会话。
fn reject_reserved_yan_std_imports(module: &ModuleFile) -> Result<(), Diagnostic> {
    for import in &module.syntax.imports {
        if is_reserved_yan_std_import(&import.path.segments) {
            return Err(import_error(
                module,
                import.span,
                "reserved module namespace `yan.std`",
            ));
        }
    }
    Ok(())
}

/// 判断导入路径是否以 M16 保留的 `yan.std` 根命名空间开始。
fn is_reserved_yan_std_import(path: &[String]) -> bool {
    matches!(path, [yan, standard_library, ..] if yan == "yan" && standard_library == "std")
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
    use std::ffi::OsString;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::{Command, ExitCode};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        build_hash, cargo_build_command, cargo_isolation_root, cargo_isolation_root_from_public,
        compile, compile_with_internal_modules, dispatch, find_source_root, read_module,
        rejects_cargo_configuration_in_ancestors, rejects_cargo_home_configuration,
        remove_cargo_target_configuration_environment, validate_module_path, CargoIsolation, USAGE,
    };
    use yan_eval::execute;
    use yan_rust_backend::generate;
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
    fn build_preserves_frontend_diagnostics_before_backend_lowering(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let unique = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let root = std::env::temp_dir().join(format!(
            "yan-m15-build-diagnostics-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root)?;
        let syntax_error = root.join("syntax.yan");
        let type_error = root.join("type.yan");
        let valid = root.join("valid.yan");
        fs::write(&syntax_error, "fn main() -> unit {")?;
        fs::write(&type_error, "fn main() -> unit { 1 }")?;
        fs::write(&valid, "fn main() -> unit { }")?;
        let _build_cleanup = RemoveDirectoryOnDrop(generated_build_root(&valid)?);

        for path in [root.join("missing.yan"), syntax_error, type_error] {
            let expected = match compile(&path, true) {
                Err(diagnostic) => {
                    let (line, column) = diagnostic
                        .source
                        .line_column(diagnostic.span.start)
                        .ok_or("fixture diagnostic must have a source location")?;
                    format!(
                        "error: {}:{line}:{column}: {}\n",
                        diagnostic.source.path().display(),
                        diagnostic.message
                    )
                }
                Ok(_) => return Err("invalid build fixture unexpectedly compiled".into()),
            };
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let exit_code = dispatch(
                &["build".to_owned(), path.display().to_string()],
                &mut stdout,
                &mut stderr,
            )?;

            assert_ne!(exit_code, ExitCode::SUCCESS);
            assert!(stdout.is_empty());
            assert_eq!(String::from_utf8(stderr)?, expected);
        }

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let exit_code = dispatch(
            &["build".to_owned(), valid.display().to_string()],
            &mut stdout,
            &mut stderr,
        )?;
        assert_eq!(exit_code, ExitCode::SUCCESS);
        assert!(stderr.is_empty());
        assert!(String::from_utf8(stdout)?
            .starts_with(&format!("{}: build succeeded: ", valid.display())));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn rejects_user_import_of_reserved_yan_std_namespace() -> Result<(), Box<dyn std::error::Error>>
    {
        let entry = temporary_entry("module app\nimport yan.std.text\nfn main() -> unit { }")?;
        let _entry_cleanup = RemoveFileOnDrop(entry.clone());
        let diagnostic = match compile(&entry, true) {
            Ok(_) => return Err("M15 must reserve `yan.std`".into()),
            Err(diagnostic) => diagnostic,
        };

        assert_eq!(diagnostic.source.path(), entry.as_path());
        assert_eq!(
            diagnostic.source.line_column(diagnostic.span.start),
            Some((2, 1))
        );
        assert_eq!(diagnostic.message, "reserved module namespace `yan.std`");
        Ok(())
    }

    #[test]
    fn compiler_owned_yan_std_module_uses_the_regular_compilation_pipeline(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let entry_path = temporary_entry("fn main() -> unit { }")?;
        let internal_path = temporary_entry("module yan.std.fixture\npub fn value() -> int { 1 }")?;
        let _entry_cleanup = RemoveFileOnDrop(entry_path.clone());
        let _internal_cleanup = RemoveFileOnDrop(internal_path.clone());
        let mut sources = SourceMap::new();
        let entry = read_module(&entry_path, &mut sources).map_err(|error| error.message)?;
        let internal = read_module(&internal_path, &mut sources).map_err(|error| error.message)?;

        let compiled = compile_with_internal_modules(entry, vec![internal], true, sources)
            .map_err(|error| error.message)?;
        assert!(generate(&compiled.mir).is_ok());
        Ok(())
    }

    #[test]
    fn build_writes_only_a_deterministic_owned_cargo_project(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let entry = temporary_entry(
            "import yan.platform.console fn main() -> unit { console.println(\"Yan\") }",
        )?;
        let _entry_cleanup = RemoveFileOnDrop(entry.clone());
        let expected_build_root = generated_build_root(&entry)?;
        let _build_cleanup = RemoveDirectoryOnDrop(expected_build_root.clone());
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit_code = dispatch(
            &["build".to_owned(), entry.display().to_string()],
            &mut stdout,
            &mut stderr,
        )?;

        assert_eq!(exit_code, ExitCode::SUCCESS);
        assert!(stderr.is_empty());
        let success = String::from_utf8(stdout)?;
        let binary_path = PathBuf::from(
            success
                .strip_prefix(&format!("{}: build succeeded: ", entry.display()))
                .ok_or("build success output must include the entry path")?
                .trim(),
        );
        assert!(binary_path.is_file());
        let binary_output = Command::new(&binary_path).output()?;
        assert!(binary_output.status.success());
        assert!(binary_output.stderr.is_empty());
        assert_eq!(binary_output.stdout, b"Yan\n");
        let build_root = binary_path
            .parent()
            .and_then(Path::parent)
            .ok_or("published binary must be nested below the generated project root")?;
        assert!(build_root.starts_with(workspace_target_dir().join("yan")));
        assert_eq!(build_root, expected_build_root);
        let cargo_root = build_root.join("cargo");
        assert_eq!(
            cargo_root.file_name().and_then(|name| name.to_str()),
            Some("cargo")
        );

        let manifest = fs::read_to_string(cargo_root.join("Cargo.toml"))?;
        assert!(manifest.starts_with("[workspace]\n"));
        let dependencies = manifest
            .split_once("[dependencies]\n")
            .ok_or("generated manifest must contain a dependency section")?
            .1
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .take_while(|line| !line.starts_with('['))
            .collect::<Vec<_>>();
        let runtime_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../yan-runtime")
            .canonicalize()?
            .to_string_lossy()
            .replace('\\', "\\\\");
        assert_eq!(
            dependencies,
            [format!("yan-runtime = {{ path = \"{runtime_path}\" }}")]
        );
        Ok(())
    }

    #[test]
    fn build_does_not_inherit_generated_project_parent_cargo_configuration(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let entry = temporary_entry("fn main() -> unit { }")?;
        let _entry_cleanup = RemoveFileOnDrop(entry.clone());
        let _build_cleanup = RemoveDirectoryOnDrop(generated_build_root(&entry)?);
        let configuration = workspace_target_dir().join("yan/.cargo");
        let _configuration_cleanup = RemoveDirectoryOnDrop(configuration.clone());
        fs::create_dir_all(&configuration)?;
        fs::write(
            configuration.join("config.toml"),
            "[build]\nrustc-wrapper = \"missing-yanc-wrapper\"\n",
        )?;

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let exit_code = dispatch(
            &["build".to_owned(), entry.display().to_string()],
            &mut stdout,
            &mut stderr,
        )?;

        assert_eq!(exit_code, ExitCode::SUCCESS);
        assert!(stderr.is_empty());
        Ok(())
    }

    #[test]
    fn cargo_build_command_removes_user_build_configuration_environment() -> Result<(), String> {
        let isolation = CargoIsolation {
            cwd: PathBuf::from("C:/yan-isolation/cwd"),
            cargo_home: PathBuf::from("C:/yan-isolation/cargo-home"),
        };
        let command = cargo_build_command(&isolation, Path::new("C:/yan-generated/Cargo.toml"))
            .map_err(|_| "Cargo command must be constructible".to_owned())?;
        let removed = command
            .get_envs()
            .filter_map(|(name, value)| {
                value
                    .is_none()
                    .then_some(name.to_string_lossy().into_owned())
            })
            .collect::<std::collections::HashSet<_>>();

        for name in [
            "CARGO_TARGET_DIR",
            "CARGO_BUILD_TARGET_DIR",
            "CARGO_BUILD_TARGET",
            "CARGO_BUILD_RUSTC_WRAPPER",
            "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
            "CARGO_BUILD_RUSTC",
            "CARGO_BUILD_RUSTFLAGS",
            "RUSTC",
            "RUSTFLAGS",
            "CARGO_ENCODED_RUSTFLAGS",
            "RUSTC_WRAPPER",
            "RUSTC_WORKSPACE_WRAPPER",
        ] {
            assert!(removed.contains(name), "{name} must be removed");
        }
        Ok(())
    }

    #[test]
    fn removes_target_specific_cargo_configuration_environment() {
        let mut command = Command::new("cargo");
        remove_cargo_target_configuration_environment(
            &mut command,
            [
                OsString::from("CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER"),
                OsString::from("CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS"),
                OsString::from("PATH"),
            ],
        );
        let removed = command
            .get_envs()
            .filter_map(|(name, value)| value.is_none().then_some(name.to_os_string()))
            .collect::<HashSet<_>>();

        assert!(removed.contains(&OsString::from(
            "CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER"
        )));
        assert!(removed.contains(&OsString::from(
            "CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS"
        )));
        assert!(!removed.contains(&OsString::from("PATH")));
    }

    #[cfg(windows)]
    #[test]
    fn cargo_isolation_root_is_outside_the_user_profile() -> Result<(), String> {
        let root = cargo_isolation_root()
            .map_err(|_| "Cargo isolation root must be constructible".to_owned())?;
        let profile = std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .ok_or("Windows test requires USERPROFILE")?;
        assert!(!root.starts_with(profile));
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn cargo_isolation_root_rejects_relative_public_directory() {
        assert!(cargo_isolation_root_from_public(Path::new(".")).is_err());
    }

    #[test]
    fn rejects_cargo_configuration_in_isolation_ancestors() -> Result<(), Box<dyn std::error::Error>>
    {
        let root = workspace_target_dir().join(format!(
            "yan-isolation-config-test-{}",
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
        let _cleanup = RemoveDirectoryOnDrop(root.clone());
        let cwd = root.join("cwd");
        fs::create_dir_all(root.join(".cargo"))?;
        fs::write(root.join(".cargo/config.toml"), "[build]\nrustflags = []\n")?;

        assert!(rejects_cargo_configuration_in_ancestors(&cwd).is_err());
        Ok(())
    }

    #[test]
    fn rejects_cargo_configuration_in_isolated_cargo_home() -> Result<(), Box<dyn std::error::Error>>
    {
        let root = workspace_target_dir().join(format!(
            "yan-cargo-home-config-test-{}",
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
        let _cleanup = RemoveDirectoryOnDrop(root.clone());
        fs::create_dir_all(&root)?;
        fs::write(root.join("config.toml"), "[build]\nrustflags = []\n")?;

        assert!(rejects_cargo_home_configuration(&root).is_err());
        Ok(())
    }

    fn temporary_entry(source: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let unique = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let entry = std::env::temp_dir().join(format!(
            "yan-m15-build-entry-{}-{unique}.yan",
            std::process::id()
        ));
        fs::write(&entry, source)?;
        Ok(entry)
    }

    fn workspace_target_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("target")
    }

    fn generated_build_root(entry: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let entry_path = entry.canonicalize()?;
        let source = fs::read_to_string(&entry_path)?;
        Ok(workspace_target_dir()
            .join("yan")
            .join(build_hash(&entry_path, &source)))
    }

    /// 测试夹具的入口文件守卫，确保断言失败时也不遗留临时源文件。
    struct RemoveFileOnDrop(PathBuf);

    impl Drop for RemoveFileOnDrop {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    /// 测试专用的生成目录守卫，避免真实 Cargo 构建留下可被后续测试误用的产物。
    struct RemoveDirectoryOnDrop(PathBuf);

    impl Drop for RemoveDirectoryOnDrop {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
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
