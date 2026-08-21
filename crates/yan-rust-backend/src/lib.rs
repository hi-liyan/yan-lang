//! Yan 已验证 MIR 的 Rust 后端边界。
//!
//! 本 crate 的生产依赖仅为 `yan-mir` 与 `yan-runtime`，公开后端入口只能接收
//! `yan_mir::VerifiedProgram`，不能依赖 AST、HIR 或 Typed HIR。M15 Task 3 仅将单基本块
//! 顺序 MIR 生成为受控 Rust 文本，不写入文件、不调用 Cargo，也不处理控制流。
//!
//! `yan-hir`、`yan-syntax` 与 `yan-typeck` 仅作为开发依赖，用于测试中构造真实
//! `VerifiedProgram` fixture；它们不会进入后端生产构建产物或公开 API。

use yan_mir::{
    BinaryOperator, CallTarget, Constant, Instruction, Operand, SourceLocation, StringPart,
    Terminator, VerifiedProgram,
};

/// Rust 后端无法完成生成时返回的稳定错误。
///
/// 该错误不携带 Rust、Cargo 或操作系统的内部文本；CLI 负责将其映射为 Yan 诊断。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendError {
    /// 当前阶段尚未支持将已验证 MIR 生成为受控 Rust 构建产物。
    UnsupportedProgram,
    /// 当前 M15 Task 3 尚未支持的 MIR 控制流或指令。
    UnsupportedMir {
        /// 未支持 MIR 节点对应的 Yan 源位置。
        location: SourceLocation,
        /// 不包含 Rust 实现细节的稳定英文原因。
        message: &'static str,
    },
}

/// Rust 后端生成的受控 Cargo 项目源码布局。
///
/// 两个字段均由后端生成，`yanc` 负责将其写入隔离构建目录；调用者不得从 Yan 源码传入
/// Cargo 清单或 Rust 源码，以防用户配置突破后端与运行时边界。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedProgram {
    /// 固定依赖和包元数据组成的 Cargo 清单文本。
    pub manifest_toml: String,
    /// 仅由已验证 MIR 转换得到的 Rust 入口源码文本。
    pub main_rs: String,
}

/// 从已验证 MIR 生成受控的 Rust 后端产物。
///
/// 入口仅接受 `VerifiedProgram`，从类型边界禁止后端重新解析前端表示。当前仅生成单基本块
/// 的顺序 MIR；控制流由后续任务处理，且本函数不写入文件或调用 Cargo。
pub fn generate(program: &VerifiedProgram) -> Result<GeneratedProgram, BackendError> {
    let mut main_rs = String::from(RUNTIME_PRELUDE);
    for function in program.functions() {
        render_function(&mut main_rs, function)?;
    }
    let entry = program
        .functions()
        .iter()
        .find(|function| function.name == "main")
        .or_else(|| program.functions().first())
        .ok_or(BackendError::UnsupportedProgram)?;
    main_rs.push_str(&format!(
        "fn main() {{ match yan_fn_{}(Vec::new()) {{ Ok(_) => (), Err(_) => std::process::exit(1) }} }}\n",
        (entry.id.0).0
    ));
    Ok(GeneratedProgram {
        manifest_toml: "[package]\nname = \"yan-generated\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nyan-runtime = { path = \"__YAN_RUNTIME_PATH__\" }\n".to_owned(),
        main_rs,
    })
}

/// 受控 Rust 源的固定运行时片段，不从 Yan 源码复制标识符或 Rust 代码。
const RUNTIME_PRELUDE: &str = r#"#![allow(dead_code, unused_assignments, unused_variables)]
use yan_runtime::{add, bytes_from_hex, console_println, equal, field, multiply, string_to_int, tuple_element, RuntimeError, Value};
fn runtime_value(result: Result<Value, RuntimeError>) -> Result<Value, RuntimeError> { result }
fn display_value(value: &Value) -> String { value.display() }
fn value_add(left: Value, right: Value) -> Result<Value, RuntimeError> { runtime_value(add(left, right)) }
fn value_multiply(left: Value, right: Value) -> Result<Value, RuntimeError> { runtime_value(multiply(left, right)) }
fn value_equal(left: Value, right: Value) -> Result<Value, RuntimeError> { runtime_value(equal(left, right)) }
fn value_tuple_element(value: &Value, index: usize) -> Result<Value, RuntimeError> { runtime_value(tuple_element(value, index)) }
fn value_struct_field(value: &Value, field_id: u32) -> Result<Value, RuntimeError> { runtime_value(field(value, field_id)) }
fn value_bytes_from_hex(value: Value) -> Result<Value, RuntimeError> { match value { Value::String(text) => runtime_value(bytes_from_hex(&text)), _ => Err(RuntimeError::InvalidOperand) } }
fn value_string_to_int(value: &Value) -> Result<Value, RuntimeError> { match value { Value::String(text) => Ok(string_to_int(text)), _ => Err(RuntimeError::InvalidOperand) } }
fn value_console_println(value: &Value) -> Result<Value, RuntimeError> { console_println(value).map(|_| Value::Unit) }
fn argument(values: &[Value], index: usize) -> Value { match values.get(index) { Some(value) => value.clone(), None => Value::Unit } }
"#;

fn render_function(output: &mut String, function: &yan_mir::Function) -> Result<(), BackendError> {
    for block in &function.blocks {
        if matches!(
            block.terminator,
            Terminator::Match { .. } | Terminator::PropagateErr { .. }
        ) {
            return Err(unsupported(terminator_location(&block.terminator)));
        }
        for instruction in &block.instructions {
            if matches!(
                instruction,
                Instruction::IterInit { .. } | Instruction::IterNext { .. }
            ) {
                return Err(unsupported(instruction_location(instruction)));
            }
        }
    }
    output.push_str(&format!(
        "fn yan_fn_{}(args: Vec<Value>) -> Result<Value, RuntimeError> {{\n",
        (function.id.0).0
    ));
    for local in &function.locals {
        let parameter = function
            .parameters
            .iter()
            .position(|item| item.id == local.id);
        let initializer = match parameter {
            Some(index) => format!("argument(&args, {index})"),
            None => "Value::Unit".to_owned(),
        };
        let binding = if local_is_stored(&function.blocks, local.id) {
            "let mut"
        } else {
            "let"
        };
        output.push_str(&format!("{binding} l_{} = {initializer};\n", (local.id).0));
    }
    let destinations = value_destinations(&function.blocks);
    for destination in destinations {
        output.push_str(&format!("let mut v_{destination} = Value::Unit;\n"));
    }
    let tracks_predecessor = has_phi(&function.blocks);
    let block_binding = if has_jump(&function.blocks) {
        "let mut"
    } else {
        "let"
    };
    output.push_str(&format!("{block_binding} block: u32 = 0;\n"));
    if tracks_predecessor {
        output.push_str("let mut previous_block: u32 = u32::MAX;\n");
    }
    output.push_str("loop { match block {\n");
    for block in &function.blocks {
        output.push_str(&format!("{} => {{\n", (block.id).0));
        for instruction in &block.instructions {
            render_instruction(output, instruction)?;
        }
        render_terminator(output, &block.terminator, tracks_predecessor);
        output.push_str("}\n");
    }
    output.push_str("_ => return Err(RuntimeError::InvalidOperand), } }\n}\n");
    Ok(())
}

fn local_is_stored(blocks: &[yan_mir::BasicBlock], local: yan_mir::LocalId) -> bool {
    blocks.iter().any(|block| {
        block.instructions.iter().any(
            |instruction| matches!(instruction, Instruction::StoreLocal { local: stored, .. } if *stored == local),
        )
    })
}

fn has_phi(blocks: &[yan_mir::BasicBlock]) -> bool {
    blocks.iter().any(|block| {
        block
            .instructions
            .iter()
            .any(|instruction| matches!(instruction, Instruction::Phi { .. }))
    })
}

fn has_jump(blocks: &[yan_mir::BasicBlock]) -> bool {
    blocks.iter().any(|block| {
        matches!(
            block.terminator,
            Terminator::Goto { .. } | Terminator::Branch { .. }
        )
    })
}

fn value_destinations(blocks: &[yan_mir::BasicBlock]) -> Vec<u32> {
    let mut destinations = Vec::new();
    for block in blocks {
        for instruction in &block.instructions {
            let destination = match instruction {
                Instruction::Assign { destination, .. }
                | Instruction::Binary { destination, .. }
                | Instruction::BuildString { destination, .. }
                | Instruction::BuildList { destination, .. }
                | Instruction::BuildMap { destination, .. }
                | Instruction::BuildTuple { destination, .. }
                | Instruction::TupleElement { destination, .. }
                | Instruction::BuildStruct { destination, .. }
                | Instruction::LoadField { destination, .. }
                | Instruction::Call { destination, .. }
                | Instruction::Phi { destination, .. } => Some(destination.0),
                Instruction::StoreLocal { .. }
                | Instruction::IterInit { .. }
                | Instruction::IterNext { .. } => None,
            };
            if let Some(destination) = destination {
                if !destinations.contains(&destination) {
                    destinations.push(destination);
                }
            }
        }
    }
    destinations
}

fn render_instruction(output: &mut String, instruction: &Instruction) -> Result<(), BackendError> {
    match instruction {
        Instruction::Assign {
            destination,
            operand,
            ..
        } => value_definition(
            output,
            destination.0,
            format!("Ok({})", render_operand(operand)),
        ),
        Instruction::StoreLocal { local, value, .. } => {
            output.push_str(&format!("l_{} = {};\n", local.0, render_operand(value)))
        }
        Instruction::Binary {
            destination,
            operator,
            left,
            right,
            ..
        } => {
            let helper = match operator {
                BinaryOperator::Add => "value_add",
                BinaryOperator::Multiply => "value_multiply",
                BinaryOperator::Equal => "value_equal",
            };
            value_definition(
                output,
                destination.0,
                format!(
                    "{helper}({}, {})",
                    render_operand(left),
                    render_operand(right)
                ),
            );
        }
        Instruction::BuildString {
            destination, parts, ..
        } => value_definition(
            output,
            destination.0,
            format!("Ok({})", render_string(parts)),
        ),
        Instruction::BuildList {
            destination,
            elements,
            ..
        } => value_definition(
            output,
            destination.0,
            format!("Ok(Value::List(vec![{}]))", render_operands(elements)),
        ),
        Instruction::BuildMap {
            destination,
            entries,
            ..
        } => value_definition(
            output,
            destination.0,
            format!(
                "Ok(Value::Map(vec![{}]))",
                entries
                    .iter()
                    .map(|(key, value)| format!(
                        "({}, {})",
                        rust_string(key),
                        render_operand(value)
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ),
        Instruction::BuildTuple {
            destination,
            elements,
            ..
        } => value_definition(
            output,
            destination.0,
            format!("Ok(Value::Tuple(vec![{}]))", render_operands(elements)),
        ),
        Instruction::TupleElement {
            destination,
            tuple,
            index,
            ..
        } => value_definition(
            output,
            destination.0,
            format!("value_tuple_element(&{}, {})", render_operand(tuple), index),
        ),
        Instruction::BuildStruct {
            destination,
            fields,
            ..
        } => value_definition(
            output,
            destination.0,
            format!(
                "Ok(Value::Struct(vec![{}]))",
                fields
                    .iter()
                    .map(|(field, value)| format!("({}, {})", field.0, render_operand(value)))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ),
        Instruction::LoadField {
            destination,
            target,
            field,
            ..
        } => value_definition(
            output,
            destination.0,
            format!(
                "value_struct_field(&{}, {})",
                render_operand(target),
                field.0
            ),
        ),
        Instruction::Call {
            destination,
            target,
            arguments,
            ..
        } => value_definition(output, destination.0, render_call(*target, arguments)),
        Instruction::Phi {
            destination,
            incoming,
            location,
            ..
        } => {
            if incoming.is_empty() {
                return Err(unsupported(*location));
            }
            output.push_str(&format!("v_{} = match previous_block {{", destination.0));
            for (predecessor, value) in incoming {
                output.push_str(&format!(" {} => {},", predecessor.0, render_operand(value)));
            }
            output.push_str(" _ => return Err(RuntimeError::InvalidOperand), };\n");
        }
        Instruction::IterInit { location, .. } | Instruction::IterNext { location, .. } => {
            return Err(unsupported(*location))
        }
    }
    Ok(())
}

fn value_definition(output: &mut String, destination: u32, expression: String) {
    output.push_str(&format!("v_{destination} = {expression}?;\n"));
}

fn render_terminator(output: &mut String, terminator: &Terminator, tracks_predecessor: bool) {
    match terminator {
        Terminator::Goto { target, .. } => {
            if tracks_predecessor {
                output.push_str("previous_block = block; ");
            }
            output.push_str(&format!("block = {}; continue;\n", target.0));
        }
        Terminator::Branch {
            condition,
            then_block,
            else_block,
            ..
        } => {
            let predecessor = if tracks_predecessor {
                "previous_block = block; "
            } else {
                ""
            };
            output.push_str(&format!(
                "match {} {{ Value::Boolean(true) => {{ {predecessor}block = {}; continue; }}, Value::Boolean(false) => {{ {predecessor}block = {}; continue; }}, _ => return Err(RuntimeError::InvalidOperand), }}\n",
                render_operand(condition), then_block.0, else_block.0
            ));
        }
        Terminator::Return { value, .. } => {
            let value = match value {
                Some(value) => render_operand(value),
                None => "Value::Unit".to_owned(),
            };
            output.push_str(&format!("return Ok({value});\n"));
        }
        Terminator::Unreachable { .. } => {
            output.push_str("return Err(RuntimeError::InvalidOperand);\n")
        }
        Terminator::Match { .. } | Terminator::PropagateErr { .. } => {}
    }
}

fn render_operand(operand: &Operand) -> String {
    match operand {
        // 基本块调度循环可能再次进入同一段生成代码；复制不可变 Yan 值可避免 Rust
        // 所有权移动阻止该控制流被编译，同时不改变 MIR 对操作数的值语义。
        Operand::Local(id) => format!("l_{}.clone()", id.0),
        Operand::Value(id) => format!("v_{}.clone()", id.0),
        Operand::Constant(value) => render_constant(value),
    }
}

fn render_constant(value: &Constant) -> String {
    match value {
        Constant::Integer(value) => format!("Value::Integer({value})"),
        Constant::Float(value) => format!("Value::Float({})", rust_string(value)),
        Constant::Boolean(value) => format!("Value::Boolean({value})"),
        Constant::String(value) => format!("Value::String({})", rust_string(value)),
        Constant::Unit => "Value::Unit".to_owned(),
        Constant::None => "Value::Option(None)".to_owned(),
        Constant::Variant(id) => format!("Value::Enum({}, None)", id.0),
    }
}

fn render_string(parts: &[StringPart]) -> String {
    let mut expression = String::from("({ let mut text = String::new(); ");
    for part in parts {
        match part {
            StringPart::Text(text) => {
                expression.push_str(&format!("text.push_str({}); ", rust_string(text)))
            }
            StringPart::Value(value) => expression.push_str(&format!(
                "text.push_str(&display_value(&{})); ",
                render_operand(value)
            )),
        }
    }
    expression.push_str("Value::String(text) })");
    expression
}

fn render_call(target: CallTarget, arguments: &[Operand]) -> String {
    let arguments = render_operands(arguments);
    match target {
        CallTarget::Function(id) => format!("yan_fn_{}(vec![{arguments}])", id.0),
        CallTarget::Newtype(_) => format!("Ok({})", first_argument(&arguments)),
        CallTarget::Variant(id) => format!(
            "Ok(Value::Enum({}, Some(Box::new({}))))",
            id.0,
            first_argument(&arguments)
        ),
        CallTarget::Some => format!(
            "Ok(Value::Option(Some(Box::new({}))))",
            first_argument(&arguments)
        ),
        CallTarget::Ok => format!(
            "Ok(Value::Result(Ok(Box::new({}))))",
            first_argument(&arguments)
        ),
        CallTarget::Err => format!(
            "Ok(Value::Result(Err(Box::new({}))))",
            first_argument(&arguments)
        ),
        CallTarget::BytesFromHex => format!("value_bytes_from_hex({})", first_argument(&arguments)),
        CallTarget::ConsolePrintln => {
            format!("value_console_println(&{})", first_argument(&arguments))
        }
        CallTarget::StringToInt(local) => format!("value_string_to_int(&l_{})", local.0),
    }
}

fn first_argument(arguments: &str) -> String {
    if arguments.is_empty() {
        "Value::Unit".to_owned()
    } else {
        arguments.to_owned()
    }
}
fn render_operands(operands: &[Operand]) -> String {
    operands
        .iter()
        .map(render_operand)
        .collect::<Vec<_>>()
        .join(", ")
}
fn rust_string(value: &str) -> String {
    format!("{value:?}")
}
fn unsupported(location: SourceLocation) -> BackendError {
    BackendError::UnsupportedMir {
        location,
        message: "unsupported MIR control flow",
    }
}
fn terminator_location(terminator: &Terminator) -> SourceLocation {
    match terminator {
        Terminator::Goto { location, .. }
        | Terminator::Branch { location, .. }
        | Terminator::Match { location, .. }
        | Terminator::Return { location, .. }
        | Terminator::PropagateErr { location, .. }
        | Terminator::Unreachable { location } => *location,
    }
}

fn instruction_location(instruction: &Instruction) -> SourceLocation {
    match instruction {
        Instruction::Assign { location, .. }
        | Instruction::StoreLocal { location, .. }
        | Instruction::Binary { location, .. }
        | Instruction::BuildString { location, .. }
        | Instruction::BuildList { location, .. }
        | Instruction::BuildMap { location, .. }
        | Instruction::BuildTuple { location, .. }
        | Instruction::TupleElement { location, .. }
        | Instruction::BuildStruct { location, .. }
        | Instruction::LoadField { location, .. }
        | Instruction::Call { location, .. }
        | Instruction::Phi { location, .. }
        | Instruction::IterInit { location, .. }
        | Instruction::IterNext { location, .. } => *location,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{generate, BackendError, GeneratedProgram};
    use yan_hir::lower;
    use yan_mir::{lower as lower_mir, verify, VerifiedProgram};
    use yan_syntax::{lex, parse};
    use yan_typeck::check;

    fn verified_fixture(source: &str) -> Result<VerifiedProgram, String> {
        let tokens = lex(source).map_err(|error| error.message)?;
        let syntax = parse(source, &tokens).map_err(|error| error.message)?;
        let hir = lower(syntax).map_err(|error| error.message)?;
        let typed = check(&hir).map_err(|error| error.message)?;
        let mir = lower_mir(typed).map_err(|error| error.message)?;
        verify(mir).map_err(|error| error.message)
    }

    #[test]
    fn generated_program_owns_the_controlled_cargo_source_layout() {
        let generated = GeneratedProgram {
            manifest_toml: "[package]".to_owned(),
            main_rs: "fn main() {}".to_owned(),
        };

        assert_eq!(generated.manifest_toml, "[package]");
        assert_eq!(generated.main_rs, "fn main() {}");
    }

    #[test]
    fn generates_straightline_verified_mir_with_runtime_value_and_console_intrinsic(
    ) -> Result<(), String> {
        let program = verified_fixture(
            "import yan.platform.console fn main() -> unit { let user_binding = [1, 2] console.println(user_binding) }",
        )?;
        let api: fn(&VerifiedProgram) -> Result<GeneratedProgram, BackendError> = generate;
        let generated = api(&program).map_err(|error| format!("{error:?}"))?;

        assert!(generated.manifest_toml.contains("yan-runtime"));
        assert!(generated.main_rs.contains("use yan_runtime::{"));
        assert!(generated.main_rs.contains("Value::List(vec!["));
        assert!(generated.main_rs.contains("console_println"));
        assert!(!generated.main_rs.contains("user_binding"));
        Ok(())
    }

    #[test]
    fn rejects_match_control_flow_with_a_stable_mir_diagnostic() -> Result<(), String> {
        let program = verified_fixture(
            "fn display_name(name: Option<string>) -> string { match name { Some(value) => value None => \"anonymous\" } } fn main() -> unit { let value = display_name(Some(\"yan\")) }",
        )?;

        match generate(&program) {
            Err(BackendError::UnsupportedMir { message, .. }) => {
                assert_eq!(message, "unsupported MIR control flow");
                Ok(())
            }
            Err(error) => Err(format!("unexpected backend error: {error:?}")),
            Ok(_) => Err("match control flow must remain unsupported in Task4b1".to_owned()),
        }
    }

    #[test]
    fn generated_straightline_project_propagates_runtime_errors_and_compiles(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let program = verified_fixture(
            "import yan.platform.console fn main() -> unit { let value = 1 console.println(value) }",
        )?;
        let generated = generate(&program).map_err(|error| format!("{error:?}"))?;

        assert!(generated.manifest_toml.contains("__YAN_RUNTIME_PATH__"));
        assert!(generated.main_rs.contains("Result<Value, RuntimeError>"));
        assert!(!generated.main_rs.contains("let _ = console_println"));

        let unique = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let project =
            std::env::temp_dir().join(format!("yan-m15-generated-{unique}-{}", std::process::id()));
        let source = project.join("src");
        fs::create_dir_all(&source)?;
        let runtime = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../yan-runtime")
            .canonicalize()?;
        let runtime_path = runtime.to_string_lossy().replace('\\', "\\\\");
        fs::write(
            project.join("Cargo.toml"),
            generated
                .manifest_toml
                .replace("__YAN_RUNTIME_PATH__", &runtime_path),
        )?;
        fs::write(source.join("main.rs"), generated.main_rs)?;
        let status = Command::new("cargo")
            .arg("check")
            .arg("--quiet")
            .current_dir(&project)
            .status()?;
        fs::remove_dir_all(&project)?;
        if status.success() {
            Ok(())
        } else {
            Err("generated straightline project must compile".into())
        }
    }

    fn run_generated(source: &str) -> Result<std::process::Output, Box<dyn std::error::Error>> {
        let program = verified_fixture(source)?;
        let generated = generate(&program).map_err(|error| format!("{error:?}"))?;
        let unique = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let project =
            std::env::temp_dir().join(format!("yan-m15-branch-{unique}-{}", std::process::id()));
        let source_dir = project.join("src");
        fs::create_dir_all(&source_dir)?;
        let runtime = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../yan-runtime")
            .canonicalize()?;
        let runtime_path = runtime.to_string_lossy().replace('\\', "\\\\");
        fs::write(
            project.join("Cargo.toml"),
            generated
                .manifest_toml
                .replace("__YAN_RUNTIME_PATH__", &runtime_path),
        )?;
        fs::write(source_dir.join("main.rs"), generated.main_rs)?;
        let output = Command::new("cargo")
            .arg("run")
            .arg("--quiet")
            .current_dir(&project)
            .output()?;
        fs::remove_dir_all(project)?;
        Ok(output)
    }

    #[test]
    fn generated_basic_blocks_execute_if_and_early_return_paths(
    ) -> Result<(), Box<dyn std::error::Error>> {
        for (source, expected) in [
            (
                "import yan.platform.console fn choose(flag: bool) -> int { if flag { 1 } else { 2 } } fn main() -> unit { console.println(choose(true)) }",
                "1\n",
            ),
            (
                "import yan.platform.console fn choose(flag: bool) -> int { if flag { 1 } else { 2 } } fn main() -> unit { console.println(choose(false)) }",
                "2\n",
            ),
            (
                "import yan.platform.console fn choose(flag: bool) -> int { if flag { return 1 } else { 2 } } fn main() -> unit { console.println(choose(true)) }",
                "1\n",
            ),
        ] {
            let output = run_generated(source)?;
            assert!(
                output.status.success(),
                "generated program failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                output.stderr.is_empty(),
                "generated program emitted stderr: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(String::from_utf8(output.stdout)?, expected);
        }
        Ok(())
    }
}
