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
    enums: HashMap<String, Vec<yan_hir::EnumVariant>>,
}

fn collect_declarations(program: &Program) -> Result<Declarations, TypeError> {
    let mut newtypes = HashMap::new();
    let mut structs = HashMap::new();
    let mut enums = HashMap::new();
    for newtype in &program.newtypes {
        if newtypes.contains_key(&newtype.name)
            || structs.contains_key(&newtype.name)
            || enums.contains_key(&newtype.name)
        {
            return Err(error(
                newtype.name_span,
                format!("type `{}` is already defined", newtype.name),
            ));
        }
        newtypes.insert(newtype.name.clone(), newtype.underlying.clone());
    }
    for structure in &program.structs {
        if newtypes.contains_key(&structure.name)
            || structs.contains_key(&structure.name)
            || enums.contains_key(&structure.name)
        {
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
    for enumeration in &program.enums {
        if newtypes.contains_key(&enumeration.name)
            || structs.contains_key(&enumeration.name)
            || enums.contains_key(&enumeration.name)
        {
            return Err(error(
                enumeration.name_span,
                format!("type `{}` is already defined", enumeration.name),
            ));
        }
        if enumeration.variants.is_empty() {
            return Err(error(
                enumeration.name_span,
                format!(
                    "enum `{}` must define at least one variant",
                    enumeration.name
                ),
            ));
        }
        let mut names = HashMap::new();
        for variant in &enumeration.variants {
            if names.insert(variant.name.as_str(), ()).is_some() {
                return Err(error(
                    variant.name_span,
                    format!("variant `{}` is already defined", variant.name),
                ));
            }
        }
        enums.insert(enumeration.name.clone(), enumeration.variants.clone());
    }
    let declarations = Declarations {
        newtypes,
        structs,
        enums,
    };
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
    for variants in declarations.enums.values() {
        for variant in variants {
            if let Some(payload) = &variant.payload {
                check_declared_type(&payload.ty, &declarations, payload.name_span)?;
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
        Type::List(element) | Type::Map(element) | Type::Option(element) => {
            check_declared_type(element, declarations, span)
        }
        Type::Result(ok, error) => {
            check_declared_type(ok, declarations, span)?;
            check_declared_type(error, declarations, span)
        }
        Type::Named(name)
            if !declarations.newtypes.contains_key(name)
                && !declarations.structs.contains_key(name)
                && !declarations.enums.contains_key(name) =>
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
        Expression::Match { target, arms, .. } => {
            let mut calls = expression_calls(target);
            calls.extend(arms.iter().flat_map(|arm| expression_calls(&arm.value)));
            calls
        }
        Expression::Return { value, .. } | Expression::Try { value, .. } => expression_calls(value),
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
            if !function.parameters.is_empty()
                || !matches!(function.return_type, Type::Unit | Type::Result(_, _))
            {
                return Err(error(
                    function.name_span,
                    "main must not have parameters and must declare `-> unit` or `-> Result<T, E>`",
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
    if !types_compatible(&tail_type, &function.return_type) {
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
        Expression::Match { target, arms, span } => type_of_match(
            target,
            arms,
            *span,
            bindings,
            signatures,
            declarations,
            console_imported,
        ),
        Expression::Return { .. } => Ok(Type::Never),
        Expression::Try { value, span } => {
            let Type::Result(ok, _) =
                type_of(value, bindings, signatures, declarations, console_imported)?
            else {
                return Err(error(*span, "`?` requires a Result value"));
            };
            Ok(*ok)
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
        } if path.iter().map(String::as_str).eq(["Some"]) => {
            if arguments.len() != 1 {
                return Err(error(*span, "Some requires exactly one argument"));
            }
            let element = type_of(
                &arguments[0],
                bindings,
                signatures,
                declarations,
                console_imported,
            )?;
            if matches!(element, Type::Option(_)) {
                return Err(error(
                    arguments[0].span(),
                    "M7 does not support nested Option values",
                ));
            }
            Ok(Type::Option(Box::new(element)))
        }
        Expression::Call {
            path,
            arguments,
            span,
        } if path.iter().map(String::as_str).eq(["Ok"]) => {
            let [value] = arguments.as_slice() else {
                return Err(error(*span, "Ok requires exactly one argument"));
            };
            Ok(Type::Result(
                Box::new(type_of(
                    value,
                    bindings,
                    signatures,
                    declarations,
                    console_imported,
                )?),
                Box::new(Type::Never),
            ))
        }
        Expression::Call {
            path,
            arguments,
            span,
        } if path.iter().map(String::as_str).eq(["Err"]) => {
            let [value] = arguments.as_slice() else {
                return Err(error(*span, "Err requires exactly one argument"));
            };
            Ok(Type::Result(
                Box::new(Type::Never),
                Box::new(type_of(
                    value,
                    bindings,
                    signatures,
                    declarations,
                    console_imported,
                )?),
            ))
        }
        Expression::Call {
            path,
            arguments,
            span,
        } if path.len() == 2 && path[1] == "to_int" => {
            if !arguments.is_empty() {
                return Err(error(*span, "string.to_int does not accept arguments"));
            }
            if bindings.get(&path[0]).map(|binding| &binding.ty) != Some(&Type::String) {
                return Err(error(*span, "string.to_int requires a string variable"));
            }
            Ok(Type::Result(Box::new(Type::Int), Box::new(Type::Unit)))
        }
        Expression::Call {
            path,
            arguments,
            span,
        } if path.len() == 2 && declarations.enums.contains_key(&path[0]) => {
            type_of_enum_constructor(
                path,
                arguments,
                *span,
                bindings,
                signatures,
                declarations,
                console_imported,
            )
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
                if !argument_matches_expected_type(
                    argument,
                    expected,
                    bindings,
                    signatures,
                    declarations,
                    console_imported,
                )? {
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
            if let Expression::Variable { name, .. } = target.as_ref() {
                if let Some(variants) = declarations.enums.get(name) {
                    let variant = variants
                        .iter()
                        .find(|variant| variant.name == *field)
                        .ok_or_else(|| {
                            error(
                                *field_span,
                                format!("enum `{name}` has no variant `{field}`"),
                            )
                        })?;
                    if variant.payload.is_some() {
                        return Err(error(
                            *field_span,
                            format!("enum variant `{name}.{field}` requires one argument"),
                        ));
                    }
                    return Ok(Type::Named(name.clone()));
                }
            }
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

/// 在已知函数参数类型的唯一上下文中，将 `None` 构造为对应的 Option 空值。
fn argument_matches_expected_type(
    argument: &Expression,
    expected: &Type,
    bindings: &HashMap<String, Binding>,
    signatures: &HashMap<String, Signature>,
    declarations: &Declarations,
    console_imported: bool,
) -> Result<bool, TypeError> {
    if matches!(argument, Expression::Variable { name, .. } if name == "None") {
        return Ok(matches!(expected, Type::Option(_)));
    }
    Ok(types_compatible(
        &type_of(
            argument,
            bindings,
            signatures,
            declarations,
            console_imported,
        )?,
        expected,
    ))
}

/// 验证 enum 变体构造的载荷数量和类型。构造表达式的路径已由 parser 保留，必须在
/// 声明表建立后才能区分它与普通模块调用，避免语法层依赖类型信息。
fn type_of_enum_constructor(
    path: &[String],
    arguments: &[Expression],
    span: Span,
    bindings: &HashMap<String, Binding>,
    signatures: &HashMap<String, Signature>,
    declarations: &Declarations,
    console_imported: bool,
) -> Result<Type, TypeError> {
    let enum_name = &path[0];
    let variant_name = &path[1];
    let variants = declarations
        .enums
        .get(enum_name)
        .ok_or_else(|| error(span, format!("undefined enum `{enum_name}`")))?;
    let variant = variants
        .iter()
        .find(|variant| variant.name == *variant_name)
        .ok_or_else(|| {
            error(
                span,
                format!("enum `{enum_name}` has no variant `{variant_name}`"),
            )
        })?;

    match &variant.payload {
        None if arguments.is_empty() => Ok(Type::Named(enum_name.clone())),
        None => Err(error(
            span,
            format!("enum variant `{enum_name}.{variant_name}` does not accept arguments"),
        )),
        Some(payload) if arguments.len() != 1 => Err(error(
            span,
            format!("enum variant `{enum_name}.{variant_name}` requires exactly one argument"),
        )),
        Some(payload) => {
            let actual = type_of(
                &arguments[0],
                bindings,
                signatures,
                declarations,
                console_imported,
            )?;
            if actual != payload.ty {
                return Err(error(
                    arguments[0].span(),
                    format!(
                        "enum variant `{enum_name}.{variant_name}` argument does not match its payload type"
                    ),
                ));
            }
            Ok(Type::Named(enum_name.clone()))
        }
    }
}

/// 根据目标类型分派 enum 或 Option 的受限穷尽匹配校验。
fn type_of_match(
    target: &Expression,
    arms: &[yan_hir::MatchArm],
    span: Span,
    bindings: &HashMap<String, Binding>,
    signatures: &HashMap<String, Signature>,
    declarations: &Declarations,
    console_imported: bool,
) -> Result<Type, TypeError> {
    match type_of(target, bindings, signatures, declarations, console_imported)? {
        Type::Named(enum_name) if declarations.enums.contains_key(&enum_name) => {
            type_of_enum_match(
                &enum_name,
                arms,
                span,
                bindings,
                signatures,
                declarations,
                console_imported,
            )
        }
        Type::Option(element) => type_of_option_match(
            &element,
            arms,
            span,
            bindings,
            signatures,
            declarations,
            console_imported,
        ),
        Type::Result(ok, error) => type_of_result_match(
            &ok,
            &error,
            arms,
            span,
            bindings,
            signatures,
            declarations,
            console_imported,
        ),
        _ => Err(error(
            target.span(),
            "match requires an enum or Option value",
        )),
    }
}

/// 验证 match 只覆盖目标 enum 的全部变体，并让有载荷变体仅在对应分支内引入绑定。
/// 分支使用独立绑定表，避免载荷名称泄漏到 match 外或影响相邻分支。
fn type_of_enum_match(
    enum_name: &str,
    arms: &[yan_hir::MatchArm],
    span: Span,
    bindings: &HashMap<String, Binding>,
    signatures: &HashMap<String, Signature>,
    declarations: &Declarations,
    console_imported: bool,
) -> Result<Type, TypeError> {
    let variants = declarations.enums.get(enum_name).ok_or_else(|| {
        error(
            span,
            format!("undefined enum `{enum_name}` in type-checked match"),
        )
    })?;
    let mut seen = HashMap::new();
    let mut result_type = None;

    for arm in arms {
        if arm.pattern.enum_name != enum_name {
            return Err(error(
                arm.pattern.enum_name_span,
                format!("match arm must use enum `{enum_name}`"),
            ));
        }
        let variant = variants
            .iter()
            .find(|variant| variant.name == arm.pattern.variant)
            .ok_or_else(|| {
                error(
                    arm.pattern.variant_span,
                    format!(
                        "enum `{enum_name}` has no variant `{}`",
                        arm.pattern.variant
                    ),
                )
            })?;
        if seen.insert(variant.name.as_str(), ()).is_some() {
            return Err(error(
                arm.pattern.variant_span,
                format!("match arm for `{enum_name}.{}` is duplicated", variant.name),
            ));
        }

        let mut arm_bindings = bindings.clone();
        match (&variant.payload, &arm.pattern.binding) {
            (None, None) => {}
            (None, Some(_)) => {
                return Err(error(
                    arm.pattern.variant_span,
                    format!("enum variant `{enum_name}.{}` has no payload", variant.name),
                ));
            }
            (Some(_), None) => {
                return Err(error(
                    arm.pattern.variant_span,
                    format!(
                        "enum variant `{enum_name}.{}` requires a payload binding",
                        variant.name
                    ),
                ));
            }
            (Some(payload), Some((binding, _))) => {
                arm_bindings.insert(
                    binding.clone(),
                    Binding {
                        ty: payload.ty.clone(),
                        mutable: false,
                    },
                );
            }
        }
        let arm_type = type_of(
            &arm.value,
            &arm_bindings,
            signatures,
            declarations,
            console_imported,
        )?;
        check_match_result_type(&mut result_type, arm_type, arm.value.span())?;
    }

    for variant in variants {
        if !seen.contains_key(variant.name.as_str()) {
            return Err(error(
                span,
                format!(
                    "match is missing enum variant `{enum_name}.{}`",
                    variant.name
                ),
            ));
        }
    }
    result_type.ok_or_else(|| error(span, "match must contain at least one arm"))
}

/// 验证内建 Option 的 Some/None 两个固定分支，避免将其与用户 enum 变体混用。
fn type_of_option_match(
    element: &Type,
    arms: &[yan_hir::MatchArm],
    span: Span,
    bindings: &HashMap<String, Binding>,
    signatures: &HashMap<String, Signature>,
    declarations: &Declarations,
    console_imported: bool,
) -> Result<Type, TypeError> {
    let mut seen = HashMap::new();
    let mut result_type = None;

    for arm in arms {
        if !arm.pattern.enum_name.is_empty() {
            return Err(error(
                arm.pattern.enum_name_span,
                "Option match arms must use `Some` or `None`",
            ));
        }
        if seen.insert(arm.pattern.variant.as_str(), ()).is_some() {
            return Err(error(
                arm.pattern.variant_span,
                format!("Option match arm `{}` is duplicated", arm.pattern.variant),
            ));
        }

        let mut arm_bindings = bindings.clone();
        match (arm.pattern.variant.as_str(), &arm.pattern.binding) {
            ("Some", Some((binding, _))) => {
                arm_bindings.insert(
                    binding.clone(),
                    Binding {
                        ty: element.clone(),
                        mutable: false,
                    },
                );
            }
            ("Some", None) => {
                return Err(error(
                    arm.pattern.variant_span,
                    "Some match arm requires a payload binding",
                ));
            }
            ("None", None) => {}
            ("None", Some(_)) => {
                return Err(error(
                    arm.pattern.variant_span,
                    "None match arm has no payload",
                ));
            }
            _ => {
                return Err(error(
                    arm.pattern.variant_span,
                    "Option match arms must use `Some` or `None`",
                ));
            }
        }
        let arm_type = type_of(
            &arm.value,
            &arm_bindings,
            signatures,
            declarations,
            console_imported,
        )?;
        check_match_result_type(&mut result_type, arm_type, arm.value.span())?;
    }

    for variant in ["Some", "None"] {
        if !seen.contains_key(variant) {
            return Err(error(
                span,
                format!("Option match is missing `{variant}` arm"),
            ));
        }
    }
    result_type.ok_or_else(|| error(span, "match must contain at least one arm"))
}

fn type_of_result_match(
    ok: &Type,
    error_type: &Type,
    arms: &[yan_hir::MatchArm],
    span: Span,
    bindings: &HashMap<String, Binding>,
    signatures: &HashMap<String, Signature>,
    declarations: &Declarations,
    console_imported: bool,
) -> Result<Type, TypeError> {
    let mut seen = HashMap::new();
    let mut result_type = None;
    for arm in arms {
        if !arm.pattern.enum_name.is_empty() {
            return Err(error(
                arm.pattern.enum_name_span,
                "Result match arms must use `Ok` or `Err`",
            ));
        }
        if seen.insert(arm.pattern.variant.as_str(), ()).is_some() {
            return Err(error(
                arm.pattern.variant_span,
                format!("Result match arm `{}` is duplicated", arm.pattern.variant),
            ));
        }
        let payload_type = match arm.pattern.variant.as_str() {
            "Ok" => ok,
            "Err" => error_type,
            _ => {
                return Err(error(
                    arm.pattern.variant_span,
                    "Result match arms must use `Ok` or `Err`",
                ))
            }
        };
        let Some((binding, _)) = &arm.pattern.binding else {
            return Err(error(
                arm.pattern.variant_span,
                "Result match arm requires a payload binding",
            ));
        };
        let mut arm_bindings = bindings.clone();
        arm_bindings.insert(
            binding.clone(),
            Binding {
                ty: payload_type.clone(),
                mutable: false,
            },
        );
        let arm_type = type_of(
            &arm.value,
            &arm_bindings,
            signatures,
            declarations,
            console_imported,
        )?;
        check_match_result_type(&mut result_type, arm_type, arm.value.span())?;
    }
    for variant in ["Ok", "Err"] {
        if !seen.contains_key(variant) {
            return Err(error(
                span,
                format!("Result match is missing `{variant}` arm"),
            ));
        }
    }
    result_type.ok_or_else(|| error(span, "match must contain at least one arm"))
}

/// 统一校验所有 match 分支的结果类型，保证 match 保持单一表达式类型。
fn check_match_result_type(
    result_type: &mut Option<Type>,
    arm_type: Type,
    span: Span,
) -> Result<(), TypeError> {
    if arm_type == Type::Never {
        return Ok(());
    }
    if let Some(expected) = result_type {
        if !types_compatible(&arm_type, expected) {
            return Err(error(span, "match arm result types must be the same"));
        }
    } else {
        *result_type = Some(arm_type);
    }
    Ok(())
}

fn types_compatible(actual: &Type, expected: &Type) -> bool {
    match (actual, expected) {
        (Type::Never, _) | (_, Type::Never) => true,
        (Type::Result(actual_ok, actual_error), Type::Result(expected_ok, expected_error)) => {
            types_compatible(actual_ok, expected_ok)
                && types_compatible(actual_error, expected_error)
        }
        _ => actual == expected,
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
            "fn main() -> unit { let ports: Map<string, int> = { \"http\": 80 \"https\": 443 } }";

        check_source(source).expect("字符串键且值类型一致的 map 应通过类型检查");
    }

    #[test]
    fn rejects_map_with_mixed_value_types() {
        let source = "fn main() -> unit { let ports = { \"http\": 80 \"name\": \"http\" } }";

        let error = check_source(source).expect_err("不同值类型的 map 必须失败");
        assert_eq!(error.message, "map values must have the same type");
    }

    #[test]
    fn checks_exhaustive_enum_match_with_payload_binding() {
        let source = "enum State { Ready Failed(reason: string) } fn label(state: State) -> string { match state { State.Ready => \"ready\" State.Failed(reason) => \"failed: {reason}\" } } fn main() -> unit { }";

        check_source(source).expect("穷尽 enum match 应通过类型检查");
    }

    #[test]
    fn rejects_non_exhaustive_enum_match() {
        let source = "enum State { Ready Failed(reason: string) } fn label(state: State) -> string { match state { State.Ready => \"ready\" } } fn main() -> unit { }";

        let error = check_source(source).expect_err("缺少变体的 match 必须失败");
        assert_eq!(
            error.message,
            "match is missing enum variant `State.Failed`"
        );
    }

    #[test]
    fn checks_option_match_with_some_binding() {
        let source = "fn display_name(name: Option<string>) -> string { match name { Some(value) => value None => \"anonymous\" } } fn main() -> unit { }";

        check_source(source).expect("Option 的 Some/None 穷尽匹配应通过类型检查");
    }

    #[test]
    fn rejects_option_match_without_none_arm() {
        let source = "fn display_name(name: Option<string>) -> string { match name { Some(value) => value } } fn main() -> unit { }";

        let error = check_source(source).expect_err("缺少 None 分支的 Option match 必须失败");
        assert_eq!(error.message, "Option match is missing `None` arm");
    }

    #[test]
    fn infers_none_from_option_function_parameter() {
        let source =
            "fn accept(value: Option<string>) -> unit { } fn main() -> unit { accept(None) }";

        check_source(source).expect("None 应从 Option 参数类型推断为空值");
    }
}
