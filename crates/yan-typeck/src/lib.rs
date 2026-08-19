//! M3 HIR 类型检查与函数调用边界验证。

use std::collections::HashMap;

use yan_hir::{Expression, Field, Function, Program, Statement, StringPart, Type};
use yan_source::Span;

/// 类型检查发现的源程序错误。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypeError {
    /// 错误对应的源文件区间。
    pub span: Span,
    /// 面向用户的稳定错误原因。
    pub message: String,
}

/// 验证 M3 程序是否满足函数、类型与平台调用边界。
pub fn check(program: &Program) -> Result<(), TypeError> {
    let console_imported = check_imports(program)?;
    let signatures = collect_signatures(program)?;
    let declarations = collect_declarations(program)?;
    check_no_recursion(program)?;
    for function in &program.functions {
        check_function(function, &signatures, &declarations, console_imported)?;
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct Declarations {
    newtypes: HashMap<String, Type>,
    structs: HashMap<String, Vec<Field>>,
}

fn collect_declarations(program: &Program) -> Result<Declarations, TypeError> {
    let mut newtypes = HashMap::new();
    let mut structs = HashMap::new();
    for newtype in &program.newtypes {
        if newtypes.contains_key(&newtype.name) || structs.contains_key(&newtype.name) {
            return Err(error(
                newtype.name_span,
                format!("type `{}` is already defined", newtype.name),
            ));
        }
        newtypes.insert(newtype.name.clone(), newtype.underlying.clone());
    }
    for structure in &program.structs {
        if newtypes.contains_key(&structure.name) || structs.contains_key(&structure.name) {
            return Err(error(
                structure.name_span,
                format!("type `{}` is already defined", structure.name),
            ));
        }
        let mut names = HashMap::new();
        for field in &structure.fields {
            if names.insert(field.name.as_str(), ()).is_some() {
                return Err(error(
                    field.name_span,
                    format!("field `{}` is already defined", field.name),
                ));
            }
        }
        structs.insert(structure.name.clone(), structure.fields.clone());
    }
    let declarations = Declarations { newtypes, structs };
    for underlying in declarations.newtypes.values() {
        check_declared_type(underlying, &declarations, Span::default())?;
    }
    for fields in declarations.structs.values() {
        for field in fields {
            check_declared_type(&field.ty, &declarations, field.name_span)?;
            if let Some(default) = &field.default {
                let actual = type_of(
                    default,
                    &HashMap::new(),
                    &HashMap::new(),
                    &declarations,
                    false,
                )?;
                if actual != field.ty {
                    return Err(error(
                        field.name_span,
                        format!(
                            "default value for field `{}` does not match its type",
                            field.name
                        ),
                    ));
                }
            }
        }
    }
    Ok(declarations)
}

fn check_declared_type(
    ty: &Type,
    declarations: &Declarations,
    span: Span,
) -> Result<(), TypeError> {
    match ty {
        Type::List(element) | Type::Map(element) => {
            check_declared_type(element, declarations, span)
        }
        Type::Named(name)
            if !declarations.newtypes.contains_key(name)
                && !declarations.structs.contains_key(name) =>
        {
            Err(error(span, format!("undefined type `{name}`")))
        }
        _ => Ok(()),
    }
}

/// M3 的函数调用只用于表达无循环的复用逻辑。递归需要明确的终止语义、资源限制和
/// 未来控制流配套设计，因此在该阶段统一拒绝直接与间接递归，避免解释器无界占用调用栈。
fn check_no_recursion(program: &Program) -> Result<(), TypeError> {
    let edges = program
        .functions
        .iter()
        .map(|function| {
            (
                function.name.as_str(),
                function
                    .statements
                    .iter()
                    .flat_map(statement_calls)
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<HashMap<_, _>>();
    let mut states = HashMap::new();

    for function in &program.functions {
        visit_call_graph(function.name.as_str(), &edges, &mut states)?;
    }
    Ok(())
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum VisitState {
    Visiting,
    Visited,
}

fn visit_call_graph(
    name: &str,
    edges: &HashMap<&str, Vec<(&str, Span)>>,
    states: &mut HashMap<String, VisitState>,
) -> Result<(), TypeError> {
    if states.get(name) == Some(&VisitState::Visited) {
        return Ok(());
    }
    states.insert(name.to_owned(), VisitState::Visiting);

    if let Some(calls) = edges.get(name) {
        for (target, span) in calls {
            if states.get(*target) == Some(&VisitState::Visiting) {
                return Err(error(*span, "M3 does not support recursive function calls"));
            }
            if edges.contains_key(*target) {
                visit_call_graph(target, edges, states)?;
            }
        }
    }

    states.insert(name.to_owned(), VisitState::Visited);
    Ok(())
}

fn statement_calls(statement: &Statement) -> Vec<(&str, Span)> {
    match statement {
        Statement::Let { value, .. } | Statement::Assign { value, .. } => expression_calls(value),
        Statement::Expression(expression) => expression_calls(expression),
    }
}

fn expression_calls(expression: &Expression) -> Vec<(&str, Span)> {
    match expression {
        Expression::Call {
            path,
            arguments,
            span,
        } => {
            let mut calls = arguments
                .iter()
                .flat_map(expression_calls)
                .collect::<Vec<_>>();
            if path.len() == 1 {
                calls.push((&path[0], *span));
            }
            calls
        }
        Expression::List { values, .. } => values.iter().flat_map(expression_calls).collect(),
        Expression::Map { entries, .. } => entries
            .iter()
            .flat_map(|entry| expression_calls(&entry.value))
            .collect(),
        Expression::StructLiteral { fields, .. } => fields
            .iter()
            .flat_map(|field| expression_calls(&field.value))
            .collect(),
        Expression::FieldAccess { target, .. } => expression_calls(target),
        Expression::Add { left, right, .. }
        | Expression::Multiply { left, right, .. }
        | Expression::Equal { left, right, .. } => {
            let mut calls = expression_calls(left);
            calls.extend(expression_calls(right));
            calls
        }
        Expression::Integer { .. }
        | Expression::Float { .. }
        | Expression::Boolean { .. }
        | Expression::String { .. }
        | Expression::Variable { .. } => Vec::new(),
    }
}

#[derive(Clone, Debug)]
struct Binding {
    ty: Type,
    mutable: bool,
}

#[derive(Clone, Debug)]
struct Signature {
    parameters: Vec<Type>,
    return_type: Type,
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
            format!("M3 does not support import `{}`", path.join(".")),
        ));
    }
    Ok(console_imported)
}

fn collect_signatures(program: &Program) -> Result<HashMap<String, Signature>, TypeError> {
    let mut signatures = HashMap::new();
    let mut main_count = 0;

    for function in &program.functions {
        if signatures.contains_key(&function.name) {
            return Err(error(
                function.name_span,
                format!("function `{}` is already defined", function.name),
            ));
        }
        if function.name == "main" {
            main_count += 1;
            if !function.parameters.is_empty() || function.return_type != Type::Unit {
                return Err(error(
                    function.name_span,
                    "main must not have parameters and must declare `-> unit`",
                ));
            }
        }
        signatures.insert(
            function.name.clone(),
            Signature {
                parameters: function
                    .parameters
                    .iter()
                    .map(|parameter| parameter.ty.clone())
                    .collect(),
                return_type: function.return_type.clone(),
            },
        );
    }

    if main_count != 1 {
        return Err(error(
            Span::default(),
            "an M3 source file must define exactly one main function",
        ));
    }
    Ok(signatures)
}

fn check_function(
    function: &Function,
    signatures: &HashMap<String, Signature>,
    declarations: &Declarations,
    console_imported: bool,
) -> Result<(), TypeError> {
    let mut bindings = HashMap::new();
    for parameter in &function.parameters {
        check_declared_type(&parameter.ty, declarations, parameter.name_span)?;
        if bindings.contains_key(&parameter.name) {
            return Err(error(
                parameter.name_span,
                format!("parameter `{}` is already defined", parameter.name),
            ));
        }
        bindings.insert(
            parameter.name.clone(),
            Binding {
                ty: parameter.ty.clone(),
                mutable: false,
            },
        );
    }

    let mut tail_type = Type::Unit;
    let count = function.statements.len();
    for (index, statement) in function.statements.iter().enumerate() {
        let is_tail = index + 1 == count;
        if let Some(ty) = check_statement(
            statement,
            &mut bindings,
            signatures,
            declarations,
            console_imported,
        )? {
            if is_tail {
                tail_type = ty;
            } else if ty != Type::Unit {
                return Err(error(
                    statement_span(statement),
                    "only the final expression in a function body may produce a return value",
                ));
            }
        }
    }
    if tail_type != function.return_type {
        return Err(error(
            function.name_span,
            format!(
                "the final expression in function `{}` does not match its declared return type",
                function.name
            ),
        ));
    }
    Ok(())
}

fn check_statement(
    statement: &Statement,
    bindings: &mut HashMap<String, Binding>,
    signatures: &HashMap<String, Signature>,
    declarations: &Declarations,
    console_imported: bool,
) -> Result<Option<Type>, TypeError> {
    match statement {
        Statement::Let {
            mutable,
            name,
            name_span,
            annotation,
            value,
        } => {
            if bindings.contains_key(name) {
                return Err(error(
                    *name_span,
                    format!("variable `{name}` is already defined"),
                ));
            }
            let actual = type_of(value, bindings, signatures, declarations, console_imported)?;
            if annotation
                .as_ref()
                .is_some_and(|expected| expected != &actual)
            {
                return Err(error(
                    *name_span,
                    format!("variable `{name}` annotation does not match its initial value type"),
                ));
            }
            bindings.insert(
                name.clone(),
                Binding {
                    ty: actual,
                    mutable: *mutable,
                },
            );
            Ok(None)
        }
        Statement::Assign {
            name,
            name_span,
            value,
        } => {
            let binding = bindings
                .get(name)
                .ok_or_else(|| error(*name_span, format!("undefined variable `{name}`")))?;
            if !binding.mutable {
                return Err(error(
                    *name_span,
                    format!("variable `{name}` is not mutable"),
                ));
            }
            if binding.ty != type_of(value, bindings, signatures, declarations, console_imported)? {
                return Err(error(
                    *name_span,
                    format!("cannot assign a value of a different type to variable `{name}`"),
                ));
            }
            Ok(None)
        }
        Statement::Expression(expression) => type_of(
            expression,
            bindings,
            signatures,
            declarations,
            console_imported,
        )
        .map(Some),
    }
}

fn type_of(
    expression: &Expression,
    bindings: &HashMap<String, Binding>,
    signatures: &HashMap<String, Signature>,
    declarations: &Declarations,
    console_imported: bool,
) -> Result<Type, TypeError> {
    match expression {
        Expression::Integer { .. } => Ok(Type::Int),
        Expression::Float { .. } => Ok(Type::Float),
        Expression::Boolean { .. } => Ok(Type::Bool),
        Expression::String { parts, .. } => {
            for part in parts {
                if let StringPart::Variable { name, span } = part {
                    let binding = bindings.get(name).ok_or_else(|| {
                        error(
                            *span,
                            format!("string interpolation references undefined variable `{name}`"),
                        )
                    })?;
                    if !matches!(binding.ty, Type::Int | Type::Bool | Type::String) {
                        return Err(error(
                            *span,
                            "string interpolation only supports int, bool, or string",
                        ));
                    }
                }
            }
            Ok(Type::String)
        }
        Expression::Variable { name, span } => bindings
            .get(name)
            .map(|binding| binding.ty.clone())
            .ok_or_else(|| error(*span, format!("undefined variable `{name}`"))),
        Expression::List { values, span } => {
            let Some((first, rest)) = values.split_first() else {
                return Err(error(
                    *span,
                    "M3 does not support empty lists with an uninferred element type",
                ));
            };
            let element_type =
                type_of(first, bindings, signatures, declarations, console_imported)?;
            for value in rest {
                if type_of(value, bindings, signatures, declarations, console_imported)?
                    != element_type
                {
                    return Err(error(value.span(), "list elements must have the same type"));
                }
            }
            Ok(Type::List(Box::new(element_type)))
        }
        Expression::Map { entries, span } => {
            let Some((first, rest)) = entries.split_first() else {
                return Err(error(
                    *span,
                    "map literals require at least one entry to infer their value type",
                ));
            };
            let value_type = type_of(
                &first.value,
                bindings,
                signatures,
                declarations,
                console_imported,
            )?;
            for entry in rest {
                if type_of(
                    &entry.value,
                    bindings,
                    signatures,
                    declarations,
                    console_imported,
                )? != value_type
                {
                    return Err(error(
                        entry.value.span(),
                        "map values must have the same type",
                    ));
                }
            }
            Ok(Type::Map(Box::new(value_type)))
        }
        Expression::Add { left, right, span } | Expression::Multiply { left, right, span } => {
            let left_type = type_of(left, bindings, signatures, declarations, console_imported)?;
            let right_type = type_of(right, bindings, signatures, declarations, console_imported)?;
            if left_type == Type::Int && right_type == Type::Int {
                Ok(Type::Int)
            } else {
                Err(error(*span, "M3 arithmetic operands must both be int"))
            }
        }
        Expression::Equal { left, right, span } => {
            let left_type = type_of(left, bindings, signatures, declarations, console_imported)?;
            let right_type = type_of(right, bindings, signatures, declarations, console_imported)?;
            if left_type == right_type && matches!(left_type, Type::Int | Type::Bool | Type::String)
            {
                Ok(Type::Bool)
            } else {
                Err(error(
                    *span,
                    "M3 `==` only supports matching int, bool, or string operands",
                ))
            }
        }
        Expression::Call {
            path,
            arguments,
            span,
        } if path.iter().map(String::as_str).eq(["bytes", "from_hex"]) => {
            if arguments.len() != 1 {
                return Err(error(*span, "bytes.from_hex requires exactly one argument"));
            }
            if type_of(
                &arguments[0],
                bindings,
                signatures,
                declarations,
                console_imported,
            )? != Type::String
            {
                return Err(error(
                    arguments[0].span(),
                    "bytes.from_hex requires a string argument",
                ));
            }
            Ok(Type::Bytes)
        }
        Expression::Call {
            path,
            arguments,
            span,
        } if path.iter().map(String::as_str).eq(["console", "println"]) => {
            if !console_imported {
                return Err(error(
                    *span,
                    "console.println requires import yan.platform.console",
                ));
            }
            if arguments.len() != 1 {
                return Err(error(
                    *span,
                    "console.println requires exactly one argument",
                ));
            }
            let _ = type_of(
                &arguments[0],
                bindings,
                signatures,
                declarations,
                console_imported,
            )?;
            Ok(Type::Unit)
        }
        Expression::Call {
            path,
            arguments,
            span,
        } if path.len() == 1 && !declarations.newtypes.contains_key(&path[0]) => {
            let name = &path[0];
            let signature = signatures
                .get(name)
                .ok_or_else(|| error(*span, format!("undefined function `{name}`")))?;
            if signature.parameters.len() != arguments.len() {
                return Err(error(
                    *span,
                    format!("function `{name}` argument count does not match"),
                ));
            }
            for (argument, expected) in arguments.iter().zip(&signature.parameters) {
                if &type_of(
                    argument,
                    bindings,
                    signatures,
                    declarations,
                    console_imported,
                )? != expected
                {
                    return Err(error(
                        argument.span(),
                        format!("function `{name}` argument type does not match"),
                    ));
                }
            }
            Ok(signature.return_type.clone())
        }
        Expression::Call {
            path,
            arguments,
            span,
        } if path.len() == 1 && declarations.newtypes.contains_key(&path[0]) => {
            if arguments.len() != 1 {
                return Err(error(
                    *span,
                    format!("newtype `{}` requires exactly one argument", path[0]),
                ));
            }
            let underlying = &declarations.newtypes[&path[0]];
            if type_of(
                &arguments[0],
                bindings,
                signatures,
                declarations,
                console_imported,
            )? != *underlying
            {
                return Err(error(
                    arguments[0].span(),
                    format!(
                        "newtype `{}` constructor argument does not match its underlying type",
                        path[0]
                    ),
                ));
            }
            Ok(Type::Named(path[0].clone()))
        }
        Expression::StructLiteral {
            name, fields, span, ..
        } => {
            let declared = declarations
                .structs
                .get(name)
                .ok_or_else(|| error(*span, format!("undefined struct `{name}`")))?;
            let mut provided = HashMap::new();
            for field in fields {
                if provided.insert(field.name.as_str(), ()).is_some() {
                    return Err(error(
                        field.name_span,
                        format!("field `{}` is specified more than once", field.name),
                    ));
                }
                let expected = declared
                    .iter()
                    .find(|declared| declared.name == field.name)
                    .ok_or_else(|| {
                        error(
                            field.name_span,
                            format!("struct `{name}` has no field `{}`", field.name),
                        )
                    })?;
                if type_of(
                    &field.value,
                    bindings,
                    signatures,
                    declarations,
                    console_imported,
                )? != expected.ty
                {
                    return Err(error(
                        field.value.span(),
                        format!("field `{}` does not match its declared type", field.name),
                    ));
                }
            }
            for field in declared {
                if !provided.contains_key(field.name.as_str()) && field.default.is_none() {
                    return Err(error(
                        *span,
                        format!("struct `{name}` is missing required field `{}`", field.name),
                    ));
                }
            }
            Ok(Type::Named(name.clone()))
        }
        Expression::FieldAccess {
            target,
            field,
            field_span,
            ..
        } => {
            let Type::Named(name) =
                type_of(target, bindings, signatures, declarations, console_imported)?
            else {
                return Err(error(*field_span, "field access requires a struct value"));
            };
            let fields = declarations.structs.get(&name).ok_or_else(|| {
                error(*field_span, format!("type `{name}` does not define fields"))
            })?;
            fields
                .iter()
                .find(|candidate| candidate.name == *field)
                .map(|candidate| candidate.ty.clone())
                .ok_or_else(|| {
                    error(
                        *field_span,
                        format!("struct `{name}` has no field `{field}`"),
                    )
                })
        }
        Expression::Call { span, .. } => Err(error(*span, "M4 does not support this call path")),
    }
}

fn statement_span(statement: &Statement) -> Span {
    match statement {
        Statement::Let { name_span, .. } | Statement::Assign { name_span, .. } => *name_span,
        Statement::Expression(expression) => expression.span(),
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
    fn checks_function_call_and_interpolation() {
        let source = "import yan.platform.console fn twice(value: int) -> int { value * 2 } fn label(total: int) -> string { \"total: {total}\" } fn main() -> unit { let total = twice(3) console.println(label(total)) }";

        check_source(source).expect("函数调用和字符串插值应通过类型检查");
    }

    #[test]
    fn rejects_assignment_to_immutable_binding() {
        let source = "import yan.platform.console fn main() -> unit { let count = 0 count = 1 }";

        let error = check_source(source).expect_err("不可变绑定不能重新赋值");
        assert!(error.message.contains("is not mutable"));
    }

    #[test]
    fn rejects_unknown_variable() {
        let source = "import yan.platform.console fn main() -> unit { console.println(value) }";

        let error = check_source(source).expect_err("未定义变量必须失败");
        assert!(error.message.contains("undefined variable"));
    }

    #[test]
    fn rejects_mismatched_type_annotation() {
        let error = check_source("fn main() -> unit { let value: string = 1 }")
            .expect_err("错误类型标注必须失败");

        assert!(error.message.contains("annotation does not match"));
    }

    #[test]
    fn rejects_unsupported_import() {
        let error = check_source("import yan.platform.files fn main() -> unit { }")
            .expect_err("M3 外的平台导入必须失败");

        assert!(error.message.contains("does not support import"));
    }

    #[test]
    fn rejects_function_argument_type_mismatch() {
        let source =
            "fn twice(value: int) -> int { value * 2 } fn main() -> unit { twice(\"bad\") }";

        let error = check_source(source).expect_err("函数参数类型不匹配必须失败");
        assert!(error.message.contains("argument type does not match"));
    }

    #[test]
    fn rejects_implicit_return_type_mismatch() {
        let source = "fn amount() -> int { \"wrong\" } fn main() -> unit { }";

        let error = check_source(source).expect_err("函数尾表达式类型不匹配必须失败");
        assert!(error.message.contains("final expression"));
    }

    #[test]
    fn rejects_interpolation_of_unknown_variable() {
        let source = "fn label() -> string { \"total: {total}\" } fn main() -> unit { }";

        let error = check_source(source).expect_err("插值引用未定义变量必须失败");
        assert!(error
            .message
            .contains("interpolation references undefined variable"));
    }

    #[test]
    fn rejects_indirect_recursion() {
        let source =
            "fn first() -> int { second() } fn second() -> int { first() } fn main() -> unit { }";

        let error = check_source(source).expect_err("M3 不应接受间接递归");
        assert!(error.message.contains("does not support recursive"));
    }

    #[test]
    fn checks_newtype_struct_and_default_field() {
        let source = "import yan.platform.console type UserId = int struct User { id: UserId name: string active: bool = true } fn main() -> unit { let user = User { id: UserId(42) name: \"Lin\" } console.println(user.name) }";

        check_source(source).expect("新类型与省略默认字段的结构体应通过类型检查");
    }

    #[test]
    fn rejects_underlying_value_for_newtype_field() {
        let source = "type UserId = int struct User { id: UserId } fn main() -> unit { let user = User { id: 42 } }";

        let error = check_source(source).expect_err("新类型字段不应接受底层类型值");
        assert!(error.message.contains("does not match its declared type"));
    }

    #[test]
    fn rejects_unknown_struct_field() {
        let source = "struct User { name: string } fn main() -> unit { let user = User { email: \"lin@example.com\" } }";

        let error = check_source(source).expect_err("未知结构体字段必须失败");
        assert!(error.message.contains("has no field"));
    }

    #[test]
    fn checks_string_keyed_map_values() {
        let source =
            "fn main() -> unit { let ports: map<string, int> = { \"http\": 80 \"https\": 443 } }";

        check_source(source).expect("字符串键且值类型一致的 map 应通过类型检查");
    }

    #[test]
    fn rejects_map_with_mixed_value_types() {
        let source = "fn main() -> unit { let ports = { \"http\": 80 \"name\": \"http\" } }";

        let error = check_source(source).expect_err("不同值类型的 map 必须失败");
        assert_eq!(error.message, "map values must have the same type");
    }
}
