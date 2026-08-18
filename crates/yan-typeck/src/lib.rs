//! M2 HIR 类型检查与平台调用边界验证。

use std::collections::HashMap;

use yan_hir::{Expression, Function, Program, Statement, Type};
use yan_source::Span;

/// 类型检查发现的源程序错误。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypeError {
    /// 错误对应的源文件区间。
    pub span: Span,
    /// 面向用户的稳定错误原因。
    pub message: String,
}

/// 验证 M2 程序是否满足可执行子集的类型和平台边界。
///
/// 成功仅表示程序可交给 `yan-eval` 执行，不意味着已经支持完整 Yan 语言。
pub fn check(program: &Program) -> Result<(), TypeError> {
    let main = find_main(program)?;
    if main.return_type != Type::Unit {
        return Err(error(main.name_span, "M2 的 main 函数必须声明为 `-> unit`"));
    }

    let console_imported = check_imports(program)?;
    let mut bindings = HashMap::new();
    for statement in &main.statements {
        check_statement(statement, &mut bindings, console_imported)?;
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct Binding {
    ty: Type,
    mutable: bool,
}

fn find_main(program: &Program) -> Result<&Function, TypeError> {
    if program.functions.len() != 1 {
        let span = program
            .functions
            .first()
            .map(|function| function.name_span)
            .unwrap_or_default();
        return Err(error(span, "M2 源文件只能定义一个 main 函数"));
    }

    let function = &program.functions[0];
    if function.name != "main" {
        return Err(error(function.name_span, "M2 源文件必须定义 main 函数"));
    }
    Ok(function)
}

fn check_imports(program: &Program) -> Result<bool, TypeError> {
    let mut console_imported = false;
    for path in &program.imports {
        if path
            .iter()
            .map(String::as_str)
            .eq(["yan", "platform", "console"])
        {
            console_imported = true;
            continue;
        }
        return Err(error(
            Span::default(),
            format!("M2 不支持导入 `{}`", path.join(".")),
        ));
    }
    Ok(console_imported)
}

fn check_statement(
    statement: &Statement,
    bindings: &mut HashMap<String, Binding>,
    console_imported: bool,
) -> Result<(), TypeError> {
    match statement {
        Statement::Let {
            mutable,
            name,
            name_span,
            annotation,
            value,
        } => {
            if bindings.contains_key(name) {
                return Err(error(*name_span, format!("变量 `{name}` 已经定义")));
            }
            let actual = type_of(value, bindings, console_imported)?;
            if let Some(expected) = annotation {
                if expected != &actual {
                    return Err(error(
                        *name_span,
                        format!("变量 `{name}` 的声明类型与初始值类型不一致"),
                    ));
                }
            }
            bindings.insert(
                name.clone(),
                Binding {
                    ty: actual,
                    mutable: *mutable,
                },
            );
            Ok(())
        }
        Statement::Assign {
            name,
            name_span,
            value,
        } => {
            let binding = bindings
                .get(name)
                .ok_or_else(|| error(*name_span, format!("未定义变量 `{name}`")))?;
            if !binding.mutable {
                return Err(error(*name_span, format!("变量 `{name}` 不是可变绑定")));
            }
            let value_type = type_of(value, bindings, console_imported)?;
            if binding.ty != value_type {
                return Err(error(
                    *name_span,
                    format!("不能将不同类型的值赋给变量 `{name}`"),
                ));
            }
            Ok(())
        }
        Statement::Expression(expression) => {
            let _ = type_of(expression, bindings, console_imported)?;
            Ok(())
        }
    }
}

fn type_of(
    expression: &Expression,
    bindings: &HashMap<String, Binding>,
    console_imported: bool,
) -> Result<Type, TypeError> {
    match expression {
        Expression::Integer { .. } => Ok(Type::Int),
        Expression::Boolean { .. } => Ok(Type::Bool),
        Expression::String { .. } => Ok(Type::String),
        Expression::Variable { name, span } => bindings
            .get(name)
            .map(|binding| binding.ty.clone())
            .ok_or_else(|| error(*span, format!("未定义变量 `{name}`"))),
        Expression::List { values, span } => {
            let Some((first, rest)) = values.split_first() else {
                return Err(error(*span, "M2 暂不支持无法推导元素类型的空列表"));
            };
            let element_type = type_of(first, bindings, console_imported)?;
            for value in rest {
                if type_of(value, bindings, console_imported)? != element_type {
                    return Err(error(value.span(), "列表元素必须具有相同类型"));
                }
            }
            Ok(Type::List(Box::new(element_type)))
        }
        Expression::Add { left, right, span } => {
            let left_type = type_of(left, bindings, console_imported)?;
            let right_type = type_of(right, bindings, console_imported)?;
            if left_type == Type::Int && right_type == Type::Int {
                Ok(Type::Int)
            } else {
                Err(error(*span, "M2 中 `+` 两侧必须都是 int"))
            }
        }
        Expression::Equal { left, right, span } => {
            let left_type = type_of(left, bindings, console_imported)?;
            let right_type = type_of(right, bindings, console_imported)?;
            if left_type == right_type && matches!(left_type, Type::Int | Type::Bool | Type::String)
            {
                Ok(Type::Bool)
            } else {
                Err(error(
                    *span,
                    "M2 中 `==` 仅支持同类型的 int、bool 或 string",
                ))
            }
        }
        Expression::Call {
            path,
            arguments,
            span,
        } => {
            if !path.iter().map(String::as_str).eq(["console", "println"]) {
                return Err(error(*span, "M2 仅支持 console.println 调用"));
            }
            if !console_imported {
                return Err(error(
                    *span,
                    "使用 console.println 前必须 import yan.platform.console",
                ));
            }
            if arguments.len() != 1 {
                return Err(error(*span, "console.println 必须接收一个参数"));
            }
            let _ = type_of(&arguments[0], bindings, console_imported)?;
            Ok(Type::Unit)
        }
    }
}

fn error(span: Span, message: impl Into<String>) -> TypeError {
    TypeError {
        span,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use yan_hir::lower;
    use yan_syntax::{lex, parse};

    use super::check;

    fn check_source(source: &str) -> Result<(), super::TypeError> {
        let tokens = lex(source).expect("测试源码应完成词法分析");
        let syntax = parse(source, &tokens).expect("测试源码应完成语法分析");
        let program = lower(syntax).expect("测试源码应完成 lowering");
        check(&program)
    }

    #[test]
    fn rejects_assignment_to_immutable_binding() {
        let source = "import yan.platform.console fn main() -> unit { let count = 0 count = 1 }";
        let error = check_source(source).expect_err("不可变绑定不能重新赋值");
        assert!(error.message.contains("不是可变绑定"));
    }

    #[test]
    fn rejects_unknown_variable() {
        let error = check_source(
            "import yan.platform.console fn main() -> unit { console.println(value) }",
        )
        .expect_err("未定义变量必须失败");

        assert!(error.message.contains("未定义变量"));
    }

    #[test]
    fn rejects_mismatched_type_annotation() {
        let error = check_source("fn main() -> unit { let value: string = 1 }")
            .expect_err("错误类型标注必须失败");

        assert!(error.message.contains("声明类型"));
    }

    #[test]
    fn rejects_unsupported_import() {
        let error = check_source("import yan.platform.files fn main() -> unit { }")
            .expect_err("M2 外的平台导入必须失败");

        assert!(error.message.contains("不支持导入"));
    }
}
