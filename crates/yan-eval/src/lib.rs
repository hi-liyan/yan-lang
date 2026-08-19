//! M3 已检查 HIR 的受限解释执行器。

use std::collections::HashMap;

use yan_hir::{Expression, Function, Program, Statement, StringPart};
use yan_source::Span;
use yan_typeck::TypedProgram;

/// 执行已通过类型检查的 Yan 程序，并返回平台控制台产生的输出行。
///
/// 解释器只接受 [`TypedProgram`]，从类型边界上禁止调用方绕过 `yan-typeck`。M14 完成
/// 完整 MIR lowering 后，解释器将进一步改为消费 MIR，而后端仍不得重新执行类型规则。
pub fn execute(typed: &TypedProgram) -> Result<Vec<String>, EvalError> {
    let program = typed.program();
    let main = find_function(program, "main", Span::default())?;
    let mut output = Vec::new();
    match execute_function(program, main, Vec::new(), &mut output)? {
        Value::Outcome(Ok(_)) | Value::Unit => Ok(output),
        Value::Outcome(Err(error)) => Err(EvalError::new(
            main.name_span,
            format!("main returned Err({})", error.display()),
        )),
        value => Err(EvalError::new(
            main.name_span,
            format!("main returned an unsupported value `{}`", value.display()),
        )),
    }
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
    Float(String),
    Bytes(String),
    Boolean(bool),
    String(String),
    List(Vec<Value>),
    Map(Vec<(String, Value)>),
    Tuple(Vec<Value>),
    Optional(Option<Box<Value>>),
    Outcome(Result<Box<Value>, Box<Value>>),
    Return(Box<Value>),
    Enum {
        enum_name: String,
        variant: String,
        payload: Option<Box<Value>>,
    },
    Newtype(String, Box<Value>),
    Struct {
        name: String,
        fields: HashMap<String, Value>,
    },
    Unit,
}

impl Value {
    fn display(&self) -> String {
        match self {
            Self::Integer(value) => value.to_string(),
            Self::Float(value) => value.clone(),
            Self::Bytes(value) => format!("0x{value}"),
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
            Self::Map(entries) => {
                let rendered = entries
                    .iter()
                    .map(|(key, value)| format!("\"{key}\": {}", value.display()))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{{rendered}}}")
            }
            Self::Tuple(values) => format!(
                "({})",
                values
                    .iter()
                    .map(Value::display)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Optional(Some(value)) => format!("Some({})", value.display()),
            Self::Optional(None) => "None".to_owned(),
            Self::Outcome(Ok(value)) => format!("Ok({})", value.display()),
            Self::Outcome(Err(value)) => format!("Err({})", value.display()),
            Self::Return(value) => value.display(),
            Self::Enum {
                enum_name,
                variant,
                payload,
            } => match payload {
                Some(payload) => format!("{enum_name}.{variant}({})", payload.display()),
                None => format!("{enum_name}.{variant}"),
            },
            Self::Newtype(_, value) => value.display(),
            Self::Struct { name, .. } => name.clone(),
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
        if let Some(value) = execute_statement(statement, program, &mut bindings, output)? {
            return Ok(value);
        }
    }
    Ok(Value::Unit)
}

fn execute_statement(
    statement: &Statement,
    program: &Program,
    bindings: &mut HashMap<String, Value>,
    output: &mut Vec<String>,
) -> Result<Option<Value>, EvalError> {
    match statement {
        Statement::Destructure { names, value } => {
            let Value::Tuple(values) = evaluate(value, program, bindings, output)? else {
                return Err(EvalError::new(
                    value.span(),
                    "type-checked destructuring requires a tuple value",
                ));
            };
            for ((name, _), value) in names.iter().zip(values) {
                bindings.insert(name.clone(), value);
            }
        }
        Statement::Let { name, value, .. } => {
            let value = evaluate(value, program, bindings, output)?;
            if let Value::Return(value) = value {
                return Ok(Some(*value));
            }
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
            if let Value::Return(value) = value {
                return Ok(Some(*value));
            }
            *binding = value;
        }
        Statement::Expression(expression) => {
            if let Value::Return(value) = evaluate(expression, program, bindings, output)? {
                return Ok(Some(*value));
            }
        }
    }
    Ok(None)
}

fn evaluate(
    expression: &Expression,
    program: &Program,
    bindings: &HashMap<String, Value>,
    output: &mut Vec<String>,
) -> Result<Value, EvalError> {
    match expression {
        Expression::Integer { value, .. } => Ok(Value::Integer(*value)),
        Expression::Float { value, .. } => Ok(Value::Float(value.clone())),
        Expression::Boolean { value, .. } => Ok(Value::Boolean(*value)),
        Expression::String { parts, .. } => render_string(parts, bindings),
        Expression::List { values, .. } => values
            .iter()
            .map(|value| evaluate(value, program, bindings, output))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::List),
        Expression::Map { entries, .. } => entries
            .iter()
            .map(|entry| {
                Ok((
                    entry.key.clone(),
                    evaluate(&entry.value, program, bindings, output)?,
                ))
            })
            .collect::<Result<Vec<_>, EvalError>>()
            .map(Value::Map),
        Expression::Tuple { values, .. } => values
            .iter()
            .map(|value| evaluate(value, program, bindings, output))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Tuple),
        Expression::Match { target, arms, span } => {
            evaluate_match(target, arms, *span, program, bindings, output)
        }
        Expression::If {
            condition,
            then_statements,
            else_statements,
            span,
        } => match evaluate(condition, program, bindings, output)? {
            Value::Boolean(true) => evaluate_block(then_statements, program, bindings, output),
            Value::Boolean(false) => evaluate_block(else_statements, program, bindings, output),
            _ => Err(EvalError::new(
                *span,
                "type-checked if condition must use bool",
            )),
        },
        Expression::For {
            name,
            iterable,
            statements,
            span,
            ..
        } => {
            let Value::List(values) = evaluate(iterable, program, bindings, output)? else {
                return Err(EvalError::new(
                    *span,
                    "type-checked for must iterate over a List value",
                ));
            };
            for value in values {
                let mut loop_bindings = bindings.clone();
                loop_bindings.insert(name.clone(), value);
                if let Value::Return(value) =
                    evaluate_block(statements, program, &loop_bindings, output)?
                {
                    return Ok(Value::Return(value));
                }
            }
            Ok(Value::Unit)
        }
        Expression::Return { value, .. } => Ok(Value::Return(Box::new(evaluate(
            value, program, bindings, output,
        )?))),
        Expression::Try { value, span } => match evaluate(value, program, bindings, output)? {
            Value::Outcome(Ok(value)) => Ok(*value),
            Value::Outcome(Err(error)) => Ok(Value::Return(Box::new(Value::Outcome(Err(error))))),
            _ => Err(EvalError::new(
                *span,
                "type-checked `?` must use a Result value",
            )),
        },
        Expression::Variable { name, .. } if name == "None" => Ok(Value::Optional(None)),
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
        Expression::StructLiteral {
            name, fields, span, ..
        } => {
            let structure = program
                .structs
                .iter()
                .find(|structure| structure.name == *name)
                .ok_or_else(|| EvalError::new(*span, format!("undefined struct `{name}`")))?;
            let mut values = HashMap::new();
            for field in fields {
                if values
                    .insert(
                        field.name.clone(),
                        evaluate(&field.value, program, bindings, output)?,
                    )
                    .is_some()
                {
                    return Err(EvalError::new(
                        field.name_span,
                        format!("field `{}` is specified more than once", field.name),
                    ));
                }
            }
            for field in &structure.fields {
                if !values.contains_key(&field.name) {
                    let default = field.default.as_ref().ok_or_else(|| {
                        EvalError::new(
                            field.name_span,
                            format!("struct `{name}` is missing required field `{}`", field.name),
                        )
                    })?;
                    values.insert(
                        field.name.clone(),
                        evaluate(default, program, bindings, output)?,
                    );
                }
            }
            Ok(Value::Struct {
                name: name.clone(),
                fields: values,
            })
        }
        Expression::FieldAccess {
            target,
            field,
            field_span,
            ..
        } => {
            if let Expression::Variable { name, .. } = target.as_ref() {
                if let Some(variant) = find_enum_variant(program, name, field) {
                    if variant.payload.is_some() {
                        return Err(EvalError::new(
                            *field_span,
                            format!("enum variant `{name}.{field}` requires one argument"),
                        ));
                    }
                    return Ok(Value::Enum {
                        enum_name: name.clone(),
                        variant: field.clone(),
                        payload: None,
                    });
                }
            }
            let value = evaluate(target, program, bindings, output)?;
            match value {
                Value::Struct { fields, .. } => fields.get(field).cloned().ok_or_else(|| {
                    EvalError::new(*field_span, format!("undefined field `{field}`"))
                }),
                _ => Err(EvalError::new(
                    *field_span,
                    "field access requires a struct value",
                )),
            }
        }
        Expression::Call {
            path,
            arguments,
            span,
        } if path.iter().map(String::as_str).eq(["bytes", "from_hex"]) => {
            let Some(argument) = arguments.first() else {
                return Err(EvalError::new(
                    *span,
                    "bytes.from_hex is missing an argument",
                ));
            };
            let Value::String(value) = evaluate(argument, program, bindings, output)? else {
                return Err(EvalError::new(
                    *span,
                    "bytes.from_hex requires a string argument",
                ));
            };
            if value.len() % 2 != 0 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(EvalError::new(
                    *span,
                    "bytes.from_hex requires an even-length hexadecimal string",
                ));
            }
            Ok(Value::Bytes(value))
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
        } if path.iter().map(String::as_str).eq(["Some"]) => {
            let [argument] = arguments.as_slice() else {
                return Err(EvalError::new(*span, "Some requires exactly one argument"));
            };
            Ok(Value::Optional(Some(Box::new(evaluate(
                argument, program, bindings, output,
            )?))))
        }
        Expression::Call {
            path,
            arguments,
            span,
        } if path.iter().map(String::as_str).eq(["Ok"]) => {
            let [value] = arguments.as_slice() else {
                return Err(EvalError::new(*span, "Ok requires exactly one argument"));
            };
            Ok(Value::Outcome(Ok(Box::new(evaluate(
                value, program, bindings, output,
            )?))))
        }
        Expression::Call {
            path,
            arguments,
            span,
        } if path.iter().map(String::as_str).eq(["Err"]) => {
            let [value] = arguments.as_slice() else {
                return Err(EvalError::new(*span, "Err requires exactly one argument"));
            };
            Ok(Value::Outcome(Err(Box::new(evaluate(
                value, program, bindings, output,
            )?))))
        }
        Expression::Call {
            path,
            arguments,
            span,
        } if path.len() == 2 && path[1] == "to_int" => {
            if !arguments.is_empty() {
                return Err(EvalError::new(
                    *span,
                    "string.to_int does not accept arguments",
                ));
            }
            let Some(Value::String(value)) = bindings.get(&path[0]) else {
                return Err(EvalError::new(
                    *span,
                    "string.to_int requires a string variable",
                ));
            };
            match value.parse::<i64>() {
                Ok(value) => Ok(Value::Outcome(Ok(Box::new(Value::Integer(value))))),
                Err(_) => Ok(Value::Outcome(Err(Box::new(Value::Unit)))),
            }
        }
        Expression::Call {
            path,
            arguments,
            span,
        } if path.len() == 2
            && program
                .enums
                .iter()
                .any(|enumeration| enumeration.name == path[0]) =>
        {
            let enum_name = &path[0];
            let variant_name = &path[1];
            let variant = find_enum_variant(program, enum_name, variant_name).ok_or_else(|| {
                EvalError::new(
                    *span,
                    format!("undefined enum variant `{enum_name}.{variant_name}`"),
                )
            })?;
            match (&variant.payload, arguments.as_slice()) {
                (None, []) => Ok(Value::Enum {
                    enum_name: enum_name.clone(),
                    variant: variant_name.clone(),
                    payload: None,
                }),
                (Some(_), [argument]) => Ok(Value::Enum {
                    enum_name: enum_name.clone(),
                    variant: variant_name.clone(),
                    payload: Some(Box::new(evaluate(argument, program, bindings, output)?)),
                }),
                (None, _) => Err(EvalError::new(
                    *span,
                    format!("enum variant `{enum_name}.{variant_name}` does not accept arguments"),
                )),
                (Some(_), _) => Err(EvalError::new(
                    *span,
                    format!(
                        "enum variant `{enum_name}.{variant_name}` requires exactly one argument"
                    ),
                )),
            }
        }
        Expression::Call {
            path,
            arguments,
            span,
        } if path.len() == 1
            && program
                .newtypes
                .iter()
                .any(|newtype| newtype.name == path[0]) =>
        {
            let Some(argument) = arguments.first() else {
                return Err(EvalError::new(
                    *span,
                    format!("newtype `{}` is missing an argument", path[0]),
                ));
            };
            Ok(Value::Newtype(
                path[0].clone(),
                Box::new(evaluate(argument, program, bindings, output)?),
            ))
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

/// 在独立局部作用域中执行嵌套语句块，并保留 return 控制流供外层函数处理。
fn evaluate_block(
    statements: &[Statement],
    program: &Program,
    bindings: &HashMap<String, Value>,
    output: &mut Vec<String>,
) -> Result<Value, EvalError> {
    let mut local_bindings = bindings.clone();
    let statement_count = statements.len();
    for (index, statement) in statements.iter().enumerate() {
        if index + 1 == statement_count {
            if let Statement::Expression(expression) = statement {
                return evaluate(expression, program, &local_bindings, output);
            }
        }
        if let Some(value) = execute_statement(statement, program, &mut local_bindings, output)? {
            return Ok(Value::Return(Box::new(value)));
        }
    }
    Ok(Value::Unit)
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

/// 在已检查的 enum 或 Option 值上选择分支，并将可选载荷限定在该分支的局部绑定表中。
fn evaluate_match(
    target: &Expression,
    arms: &[yan_hir::MatchArm],
    span: Span,
    program: &Program,
    bindings: &HashMap<String, Value>,
    output: &mut Vec<String>,
) -> Result<Value, EvalError> {
    match evaluate(target, program, bindings, output)? {
        Value::Enum {
            enum_name,
            variant,
            payload,
        } => {
            let arm = arms
                .iter()
                .find(|arm| arm.pattern.enum_name == enum_name && arm.pattern.variant == variant)
                .ok_or_else(|| {
                    EvalError::new(
                        span,
                        format!("type-checked match is missing `{enum_name}.{variant}`"),
                    )
                })?;
            evaluate_match_arm(arm, payload, program, bindings, output)
        }
        Value::Optional(payload) => {
            let variant = if payload.is_some() { "Some" } else { "None" };
            let arm = arms
                .iter()
                .find(|arm| arm.pattern.enum_name.is_empty() && arm.pattern.variant == variant)
                .ok_or_else(|| {
                    EvalError::new(
                        span,
                        format!("type-checked Option match is missing `{variant}` arm"),
                    )
                })?;
            evaluate_match_arm(arm, payload, program, bindings, output)
        }
        Value::Outcome(outcome) => {
            let (variant, payload) = match outcome {
                Ok(value) => ("Ok", Some(value)),
                Err(value) => ("Err", Some(value)),
            };
            let arm = arms
                .iter()
                .find(|arm| arm.pattern.enum_name.is_empty() && arm.pattern.variant == variant)
                .ok_or_else(|| {
                    EvalError::new(
                        span,
                        format!("type-checked Result match is missing `{variant}` arm"),
                    )
                })?;
            evaluate_match_arm(arm, payload, program, bindings, output)
        }
        _ => Err(EvalError::new(
            span,
            "type-checked match must use an enum or Option value",
        )),
    }
}

/// 求值已选中的 match 分支；有绑定时必须有载荷，否则表示类型检查后的内部不变量破坏。
fn evaluate_match_arm(
    arm: &yan_hir::MatchArm,
    payload: Option<Box<Value>>,
    program: &Program,
    bindings: &HashMap<String, Value>,
    output: &mut Vec<String>,
) -> Result<Value, EvalError> {
    let mut arm_bindings = bindings.clone();
    if let Some((binding, binding_span)) = &arm.pattern.binding {
        let payload = payload.ok_or_else(|| {
            EvalError::new(
                *binding_span,
                "type-checked match binding requires a payload",
            )
        })?;
        arm_bindings.insert(binding.clone(), *payload);
    }
    evaluate(&arm.value, program, &arm_bindings, output)
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

fn find_enum_variant<'program>(
    program: &'program Program,
    enum_name: &str,
    variant_name: &str,
) -> Option<&'program yan_hir::EnumVariant> {
    program
        .enums
        .iter()
        .find(|enumeration| enumeration.name == enum_name)
        .and_then(|enumeration| {
            enumeration
                .variants
                .iter()
                .find(|variant| variant.name == variant_name)
        })
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
        let typed = check(&program).expect("测试源码应通过类型检查");

        assert_eq!(execute(&typed).expect("测试源码应能执行"), vec!["total: 6"]);
    }

    #[test]
    fn executes_map_literal_and_displays_entries_in_source_order() {
        let source = "import yan.platform.console fn main() -> unit { let ports: Map<string, int> = { \"http\": 80 \"https\": 443 } console.println(ports) }";
        let tokens = lex(source).expect("测试源码应完成词法分析");
        let syntax = parse(source, &tokens).expect("测试源码应完成语法分析");
        let program = lower(syntax).expect("测试源码应完成 lowering");
        let typed = check(&program).expect("测试源码应通过类型检查");

        assert_eq!(
            execute(&typed).expect("测试源码应能执行"),
            vec!["{\"http\": 80, \"https\": 443}"]
        );
    }

    #[test]
    fn executes_enum_match_with_payload_binding() {
        let source = "import yan.platform.console enum State { Ready Failed(reason: string) } fn label(state: State) -> string { match state { State.Ready => \"ready\" State.Failed(reason) => \"failed: {reason}\" } } fn main() -> unit { console.println(label(State.Failed(\"network\"))) }";
        let tokens = lex(source).expect("测试源码应完成词法分析");
        let syntax = parse(source, &tokens).expect("测试源码应完成语法分析");
        let program = lower(syntax).expect("测试源码应完成 lowering");
        let typed = check(&program).expect("测试源码应通过类型检查");

        assert_eq!(
            execute(&typed).expect("测试源码应能执行"),
            vec!["failed: network"]
        );
    }

    #[test]
    fn executes_option_match_with_some_binding() {
        let source = "import yan.platform.console fn display_name(name: Option<string>) -> string { match name { Some(value) => value None => \"anonymous\" } } fn main() -> unit { console.println(display_name(Some(\"Lin\"))) }";
        let tokens = lex(source).expect("测试源码应完成词法分析");
        let syntax = parse(source, &tokens).expect("测试源码应完成语法分析");
        let program = lower(syntax).expect("测试源码应完成 lowering");
        let typed = check(&program).expect("测试源码应通过类型检查");

        assert_eq!(execute(&typed).expect("测试源码应能执行"), vec!["Lin"]);
    }

    #[test]
    fn executes_if_condition_and_for_loop() {
        let source = "import yan.platform.console fn main() -> unit { let targets = [\"cli\", \"web\"] for target in targets { if target == \"cli\" { console.println(\"command\") } else { console.println(\"browser\") } } }";
        let tokens = lex(source).expect("测试源码应完成词法分析");
        let syntax = parse(source, &tokens).expect("测试源码应完成语法分析");
        let program = lower(syntax).expect("测试源码应完成 lowering");
        let typed = check(&program).expect("测试源码应通过类型检查");

        assert_eq!(
            execute(&typed).expect("测试源码应能执行"),
            vec!["command", "browser"]
        );
    }

    #[test]
    fn executes_result_match_and_propagation() {
        let source = "import yan.platform.console enum ConfigError { MissingPort InvalidPort(value: string) } fn parse_port(value: Option<string>) -> Result<int, ConfigError> { let text = match value { Some(text) => text None => return Err(ConfigError.MissingPort) } match text.to_int() { Ok(port) => Ok(port) Err(_) => Err(ConfigError.InvalidPort(text)) } } fn main() -> Result<int, ConfigError> { let port = parse_port(Some(\"8080\"))? console.println(port) Ok(0) }";
        let tokens = lex(source).expect("测试源码应完成词法分析");
        let syntax = parse(source, &tokens).expect("测试源码应完成语法分析");
        let program = lower(syntax).expect("测试源码应完成 lowering");
        let typed = check(&program).expect("测试源码应通过类型检查");

        assert_eq!(execute(&typed).expect("测试源码应能执行"), vec!["8080"]);
    }

    #[test]
    fn reports_err_returned_from_main() {
        let source = "enum ConfigError { MissingPort } fn parse_port(value: Option<string>) -> Result<int, ConfigError> { match value { Some(text) => Ok(text.to_int()?) None => Err(ConfigError.MissingPort) } } fn main() -> Result<int, ConfigError> { let port = parse_port(None)? Ok(port) }";
        let tokens = lex(source).expect("测试源码应完成词法分析");
        let syntax = parse(source, &tokens).expect("测试源码应完成语法分析");
        let program = lower(syntax).expect("测试源码应完成 lowering");
        let typed = check(&program).expect("测试源码应通过类型检查");

        let error = execute(&typed).expect_err("main 返回 Err 必须作为执行失败报告");
        assert_eq!(error.message, "main returned Err(ConfigError.MissingPort)");
    }
}
