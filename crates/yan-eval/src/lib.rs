//! M3 已检查 HIR 的受限解释执行器。

use std::collections::HashMap;

use yan_hir::{Expression, Function, Program, Statement, StringPart};
use yan_source::Span;

/// 执行已通过类型检查的 M3 程序，并返回平台控制台产生的输出行。
///
/// M3 使用解释执行验证函数语义闭环。未来 Rust 后端应消费相同 HIR；调用方必须先运行
/// `yan-typeck`，否则本函数会将违反内部不变量的情况报告为错误。
pub fn execute(program: &Program) -> Result<Vec<String>, EvalError> {
    let main = find_function(program, "main", Span::default())?;
    let mut output = Vec::new();
    let _ = execute_function(program, main, Vec::new(), &mut output)?;
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

fn execute_function(
    program: &Program,
    function: &Function,
    arguments: Vec<Value>,
    output: &mut Vec<String>,
) -> Result<Value, EvalError> {
    if function.parameters.len() != arguments.len() {
        return Err(EvalError::new(
            function.name_span,
            format!(
                "function `{}` argument count disagrees with type checking",
                function.name
            ),
        ));
    }

    let mut bindings = HashMap::new();
    for (parameter, argument) in function.parameters.iter().zip(arguments) {
        bindings.insert(parameter.name.clone(), argument);
    }

    let statement_count = function.statements.len();
    for (index, statement) in function.statements.iter().enumerate() {
        if index + 1 == statement_count {
            if let Statement::Expression(expression) = statement {
                return evaluate(expression, program, &bindings, output);
            }
        }
        execute_statement(statement, program, &mut bindings, output)?;
    }
    Ok(Value::Unit)
}

fn execute_statement(
    statement: &Statement,
    program: &Program,
    bindings: &mut HashMap<String, Value>,
    output: &mut Vec<String>,
) -> Result<(), EvalError> {
    match statement {
        Statement::Let { name, value, .. } => {
            let value = evaluate(value, program, bindings, output)?;
            bindings.insert(name.clone(), value);
        }
        Statement::Assign {
            name,
            name_span,
            value,
        } => {
            let value = evaluate(value, program, bindings, output)?;
            let Some(binding) = bindings.get_mut(name) else {
                return Err(EvalError::new(
                    *name_span,
                    format!("undefined variable `{name}`"),
                ));
            };
            *binding = value;
        }
        Statement::Expression(expression) => {
            let _ = evaluate(expression, program, bindings, output)?;
        }
    }
    Ok(())
}

fn evaluate(
    expression: &Expression,
    program: &Program,
    bindings: &HashMap<String, Value>,
    output: &mut Vec<String>,
) -> Result<Value, EvalError> {
    match expression {
        Expression::Integer { value, .. } => Ok(Value::Integer(*value)),
        Expression::Boolean { value, .. } => Ok(Value::Boolean(*value)),
        Expression::String { parts, .. } => render_string(parts, bindings),
        Expression::List { values, .. } => values
            .iter()
            .map(|value| evaluate(value, program, bindings, output))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::List),
        Expression::Variable { name, span } => bindings
            .get(name)
            .cloned()
            .ok_or_else(|| EvalError::new(*span, format!("undefined variable `{name}`"))),
        Expression::Add { left, right, span } => {
            let left = evaluate(left, program, bindings, output)?;
            let right = evaluate(right, program, bindings, output)?;
            match (left, right) {
                (Value::Integer(left), Value::Integer(right)) => left
                    .checked_add(right)
                    .map(Value::Integer)
                    .ok_or_else(|| EvalError::new(*span, "integer addition overflow")),
                _ => Err(EvalError::new(*span, "type-checked addition must use int")),
            }
        }
        Expression::Multiply { left, right, span } => {
            let left = evaluate(left, program, bindings, output)?;
            let right = evaluate(right, program, bindings, output)?;
            match (left, right) {
                (Value::Integer(left), Value::Integer(right)) => left
                    .checked_mul(right)
                    .map(Value::Integer)
                    .ok_or_else(|| EvalError::new(*span, "integer multiplication overflow")),
                _ => Err(EvalError::new(
                    *span,
                    "type-checked multiplication must use int",
                )),
            }
        }
        Expression::Equal { left, right, span } => {
            let left = evaluate(left, program, bindings, output)?;
            let right = evaluate(right, program, bindings, output)?;
            match (left, right) {
                (Value::Integer(left), Value::Integer(right)) => Ok(Value::Boolean(left == right)),
                (Value::Boolean(left), Value::Boolean(right)) => Ok(Value::Boolean(left == right)),
                (Value::String(left), Value::String(right)) => Ok(Value::Boolean(left == right)),
                _ => Err(EvalError::new(
                    *span,
                    "type-checked `==` must compare matching primitive values",
                )),
            }
        }
        Expression::Call {
            path,
            arguments,
            span,
        } if path.iter().map(String::as_str).eq(["console", "println"]) => {
            let Some(argument) = arguments.first() else {
                return Err(EvalError::new(
                    *span,
                    "console.println is missing an argument",
                ));
            };
            let rendered = evaluate(argument, program, bindings, output)?.display();
            output.push(rendered);
            Ok(Value::Unit)
        }
        Expression::Call {
            path,
            arguments,
            span,
        } if path.len() == 1 => {
            let function = find_function(program, &path[0], *span)?;
            let evaluated = arguments
                .iter()
                .map(|argument| evaluate(argument, program, bindings, output))
                .collect::<Result<Vec<_>, _>>()?;
            execute_function(program, function, evaluated, output)
        }
        Expression::Call { span, .. } => Err(EvalError::new(
            *span,
            "type-checked call path is unsupported",
        )),
    }
}

fn render_string(
    parts: &[StringPart],
    bindings: &HashMap<String, Value>,
) -> Result<Value, EvalError> {
    let mut rendered = String::new();
    for part in parts {
        match part {
            StringPart::Text(text) => rendered.push_str(text),
            StringPart::Variable { name, span } => {
                let value = bindings
                    .get(name)
                    .ok_or_else(|| EvalError::new(*span, format!("undefined variable `{name}`")))?;
                rendered.push_str(&value.display());
            }
        }
    }
    Ok(Value::String(rendered))
}

fn find_function<'program>(
    program: &'program Program,
    name: &str,
    span: Span,
) -> Result<&'program Function, EvalError> {
    program
        .functions
        .iter()
        .find(|function| function.name == name)
        .ok_or_else(|| EvalError::new(span, format!("undefined function `{name}`")))
}

#[cfg(test)]
mod tests {
    use yan_hir::lower;
    use yan_syntax::{lex, parse};
    use yan_typeck::check;

    use super::execute;

    #[test]
    fn executes_function_call_and_interpolation() {
        let source = "import yan.platform.console fn twice(value: int) -> int { value * 2 } fn label(total: int) -> string { \"total: {total}\" } fn main() -> unit { let total = twice(3) console.println(label(total)) }";
        let tokens = lex(source).expect("测试源码应完成词法分析");
        let syntax = parse(source, &tokens).expect("测试源码应完成语法分析");
        let program = lower(syntax).expect("测试源码应完成 lowering");
        check(&program).expect("测试源码应通过类型检查");

        assert_eq!(
            execute(&program).expect("测试源码应能执行"),
            vec!["total: 6"]
        );
    }
}
