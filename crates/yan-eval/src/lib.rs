//! M2 已检查 HIR 的受限解释执行器。

use std::collections::HashMap;

use yan_hir::{Expression, Program, Statement};
use yan_source::Span;

/// 执行已通过类型检查的 M2 程序，并返回平台控制台产生的输出行。
///
/// M2 使用解释执行验证语言语义闭环。该接口不定义长期部署模型，未来 Rust 后端将消费
/// 相同 HIR；调用方必须先运行 `yan-typeck`，否则本函数会把违反内部不变量的情况报告为错误。
pub fn execute(program: &Program) -> Result<Vec<String>, EvalError> {
    let main = program
        .functions
        .iter()
        .find(|function| function.name == "main")
        .ok_or_else(|| EvalError::new(Span::default(), "找不到可执行的 main 函数"))?;
    let mut bindings = HashMap::new();
    let mut output = Vec::new();

    for statement in &main.statements {
        execute_statement(statement, &mut bindings, &mut output)?;
    }
    Ok(output)
}

/// 解释执行中发现的内部不变量破坏。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvalError {
    /// 与错误相关的源码区间。
    pub span: Span,
    /// 面向开发者的错误原因。
    pub message: String,
}

impl EvalError {
    /// 构造执行错误。
    pub fn new(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Value {
    Integer(i64),
    Boolean(bool),
    String(String),
    List(Vec<Value>),
    Unit,
}

impl Value {
    fn display(&self) -> String {
        match self {
            Self::Integer(value) => value.to_string(),
            Self::Boolean(value) => value.to_string(),
            Self::String(value) => value.clone(),
            Self::List(values) => {
                let rendered = values
                    .iter()
                    .map(Value::display)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("[{rendered}]")
            }
            Self::Unit => "unit".to_owned(),
        }
    }
}

fn execute_statement(
    statement: &Statement,
    bindings: &mut HashMap<String, Value>,
    output: &mut Vec<String>,
) -> Result<(), EvalError> {
    match statement {
        Statement::Let { name, value, .. } => {
            let value = evaluate(value, bindings, output)?;
            bindings.insert(name.clone(), value);
        }
        Statement::Assign {
            name,
            name_span,
            value,
        } => {
            let value = evaluate(value, bindings, output)?;
            let Some(binding) = bindings.get_mut(name) else {
                return Err(EvalError::new(*name_span, format!("未定义变量 `{name}`")));
            };
            *binding = value;
        }
        Statement::Expression(expression) => {
            let _ = evaluate(expression, bindings, output)?;
        }
    }
    Ok(())
}

fn evaluate(
    expression: &Expression,
    bindings: &HashMap<String, Value>,
    output: &mut Vec<String>,
) -> Result<Value, EvalError> {
    match expression {
        Expression::Integer { value, .. } => Ok(Value::Integer(*value)),
        Expression::Boolean { value, .. } => Ok(Value::Boolean(*value)),
        Expression::String { value, .. } => Ok(Value::String(value.clone())),
        Expression::List { values, .. } => values
            .iter()
            .map(|value| evaluate(value, bindings, output))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::List),
        Expression::Variable { name, span } => bindings
            .get(name)
            .cloned()
            .ok_or_else(|| EvalError::new(*span, format!("未定义变量 `{name}`"))),
        Expression::Add { left, right, span } => {
            let left = evaluate(left, bindings, output)?;
            let right = evaluate(right, bindings, output)?;
            match (left, right) {
                (Value::Integer(left), Value::Integer(right)) => left
                    .checked_add(right)
                    .map(Value::Integer)
                    .ok_or_else(|| EvalError::new(*span, "整数加法溢出")),
                _ => Err(EvalError::new(*span, "类型检查后的加法必须使用 int")),
            }
        }
        Expression::Equal { left, right, span } => {
            let left = evaluate(left, bindings, output)?;
            let right = evaluate(right, bindings, output)?;
            match (left, right) {
                (Value::Integer(left), Value::Integer(right)) => Ok(Value::Boolean(left == right)),
                (Value::Boolean(left), Value::Boolean(right)) => Ok(Value::Boolean(left == right)),
                (Value::String(left), Value::String(right)) => Ok(Value::Boolean(left == right)),
                _ => Err(EvalError::new(
                    *span,
                    "类型检查后的 `==` 必须比较同类型基础值",
                )),
            }
        }
        Expression::Call {
            path,
            arguments,
            span,
        } if path.iter().map(String::as_str).eq(["console", "println"]) => {
            let Some(argument) = arguments.first() else {
                return Err(EvalError::new(*span, "console.println 缺少参数"));
            };
            let rendered = evaluate(argument, bindings, output)?.display();
            output.push(rendered);
            Ok(Value::Unit)
        }
        Expression::Call { span, .. } => Err(EvalError::new(
            *span,
            "类型检查后的调用必须是 console.println",
        )),
    }
}

#[cfg(test)]
mod tests {
    use yan_hir::lower;
    use yan_syntax::{lex, parse};
    use yan_typeck::check;

    use super::execute;

    #[test]
    fn executes_console_output_after_assignment() {
        let source = "import yan.platform.console fn main() -> unit { let mut count = 0 count = count + 1 console.println(count) }";
        let tokens = lex(source).expect("测试源码应完成词法分析");
        let syntax = parse(source, &tokens).expect("测试源码应完成语法分析");
        let program = lower(syntax).expect("测试源码应完成 lowering");
        check(&program).expect("测试源码应通过类型检查");

        assert_eq!(execute(&program).expect("测试源码应能执行"), vec!["1"]);
    }
}
