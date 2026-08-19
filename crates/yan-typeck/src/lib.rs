//! M3 HIR 类型检查与函数调用边界验证。

use std::collections::HashMap;

use yan_hir::{
    DefId, Expression, Field, FieldId, Function, LocalId, Program, Statement, StringPart, Type,
    VariantId,
};
use yan_source::Span;

/// 类型检查发现的源程序错误。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypeError {
    /// 错误对应的源文件区间。
    pub span: Span,
    /// 面向用户的稳定错误原因。
    pub message: String,
}

/// 已完成名称、类型与控制流验证的 Yan 程序。
///
/// 该类型是类型检查阶段成功时唯一的输出边界。它持有不可变 HIR，后续 MIR lowering
/// 和解释执行只能消费本类型，避免绕过类型检查重新使用原始程序。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedProgram {
    /// 运行时和 MIR lowering 消费的已类型化函数体。
    pub functions: Vec<TypedFunction>,
    /// 已验证的结构体字段类型与默认值。
    pub structs: Vec<TypedStruct>,
    /// 已验证的 enum 声明，供构造与模式执行使用。
    pub enums: Vec<TypedEnum>,
    /// 已验证的新类型声明。
    pub newtypes: Vec<TypedNewtype>,
}

/// 一个已验证、可直接 lowering 的函数定义。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedFunction {
    /// 函数声明的稳定标识。
    pub id: DefId,
    /// 仅供诊断和调试显示的源函数名称。
    pub name: String,
    /// 函数名称在源文件中的位置。
    pub span: Span,
    /// 参数局部位置。
    pub parameters: Vec<TypedLocal>,
    /// 已类型化的函数体。
    pub statements: Vec<TypedStatement>,
    /// 函数的声明返回类型。
    pub return_type: Type,
}

/// 已验证的局部绑定。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedLocal {
    /// 局部存储位置。
    pub id: LocalId,
    /// 仅供诊断与调试使用的源名称。
    pub name: String,
    /// 声明位置。
    pub span: Span,
    /// 已确定的 Yan 类型。
    pub ty: Type,
    /// 是否允许赋值覆盖。
    pub mutable: bool,
}

/// 已类型化语句。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TypedStatement {
    /// 元组解构并初始化多个局部位置。
    Destructure { locals: Vec<TypedLocal>, value: TypedExpression },
    /// 初始化一个局部位置。
    Let { local: TypedLocal, value: TypedExpression },
    /// 覆盖类型检查已确认可变的局部位置。
    Assign { local: LocalId, value: TypedExpression, span: Span },
    /// 执行表达式；末尾表达式由 MIR lowering 转换为函数返回值。
    Expression(TypedExpression),
}

/// 已类型化值表达式。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedExpression {
    /// 表达式在源文件中的位置。
    pub span: Span,
    /// 表达式的确定 Yan 类型。
    pub ty: Type,
    /// 不再引用 HIR `Expression` 的可执行节点。
    pub kind: TypedExpressionKind,
}

/// 已类型化表达式的执行语义。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TypedExpressionKind {
    /// 基础字面量。
    Integer(i64),
    /// 浮点字面量的原始规范文本。
    Float(String),
    /// 布尔字面量。
    Boolean(bool),
    /// 字符串片段。
    String(Vec<TypedStringPart>),
    /// 列表构造。
    List(Vec<TypedExpression>),
    /// map 构造。
    Map(Vec<(String, TypedExpression)>),
    /// 元组构造。
    Tuple(Vec<TypedExpression>),
    /// 已解析的模式分派。
    Match { target: Box<TypedExpression>, arms: Vec<TypedMatchArm> },
    /// 条件分支。
    If { condition: Box<TypedExpression>, then_statements: Vec<TypedStatement>, else_statements: Vec<TypedStatement> },
    /// 列表循环。
    For { local: TypedLocal, iterable: Box<TypedExpression>, statements: Vec<TypedStatement> },
    /// 函数返回。
    Return(Box<TypedExpression>),
    /// Result 错误传播。
    Try(Box<TypedExpression>),
    /// 已解析局部读取。
    Local(LocalId),
    /// Option 空值构造。
    None,
    /// 已解析调用或固定内建调用。
    Call { target: TypedCallTarget, arguments: Vec<TypedExpression> },
    /// 整数加法。
    Add(Box<TypedExpression>, Box<TypedExpression>),
    /// 整数乘法。
    Multiply(Box<TypedExpression>, Box<TypedExpression>),
    /// 同类型基础值相等比较。
    Equal(Box<TypedExpression>, Box<TypedExpression>),
    /// 已解析结构体构造。
    Struct { structure: DefId, fields: Vec<(FieldId, TypedExpression)> },
    /// 已解析字段读取。
    Field { target: Box<TypedExpression>, field: FieldId },
    /// 无载荷 enum 构造。
    Variant(VariantId),
}

/// 字符串片段的已解析形式。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TypedStringPart {
    /// 原样文本。
    Text(String),
    /// 已解析局部变量插值。
    Local(LocalId),
}

/// match 分支的已验证模式与局部绑定。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedMatchArm {
    /// 被分派的 enum、Option 或 Result 变体。
    pub pattern: TypedPattern,
    /// 仅在该分支可见的可选载荷绑定。
    pub binding: Option<TypedLocal>,
    /// 分支结果。
    pub value: TypedExpression,
}

/// 已解析的模式目标。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TypedPattern {
    /// 用户 enum 变体。
    Variant(VariantId),
    /// 内建 Option 的 Some 分支。
    Some,
    /// 内建 Option 的 None 分支。
    None,
    /// 内建 Result 的 Ok 分支。
    Ok,
    /// 内建 Result 的 Err 分支。
    Err,
}

/// 调用目标的已解析形式。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TypedCallTarget {
    /// 用户函数。
    Function(DefId),
    /// 新类型构造。
    Newtype(DefId),
    /// 有载荷 enum 变体构造。
    Variant(VariantId),
    /// `Some` 构造。
    Some,
    /// `Ok` 构造。
    Ok,
    /// `Err` 构造。
    Err,
    /// `bytes.from_hex`。
    BytesFromHex,
    /// `console.println`。
    ConsolePrintln,
    /// `string.to_int`，接收解析后的接收者局部位置。
    StringToInt(LocalId),
}

/// 已验证结构体声明。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedStruct {
    /// 结构体声明 ID。
    pub id: DefId,
    /// 仅供诊断与调试显示的名称。
    pub name: String,
    /// 按字段 ID 的声明顺序排列。
    pub fields: Vec<TypedField>,
}

/// 已验证结构体字段。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedField {
    /// 字段 ID。
    pub id: FieldId,
    /// 字段类型。
    pub ty: Type,
    /// 已类型化的可选默认值。
    pub default: Option<TypedExpression>,
}

/// 已验证 enum 声明。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedEnum {
    /// enum 声明 ID。
    pub id: DefId,
    /// 仅供诊断与调试显示的名称。
    pub name: String,
    /// 声明顺序稳定的变体。
    pub variants: Vec<TypedVariant>,
}

/// 已验证 enum 变体。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedVariant {
    /// 变体 ID。
    pub id: VariantId,
    /// 可选单载荷类型。
    pub payload: Option<Type>,
}

/// 已验证新类型声明。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedNewtype {
    /// 声明 ID。
    pub id: DefId,
    /// 仅供诊断与调试显示的名称。
    pub name: String,
    /// 底层类型。
    pub underlying: Type,
}

/// 验证 M3 程序是否满足函数、类型与平台调用边界。
pub fn check(program: &Program) -> Result<TypedProgram, TypeError> {
    check_program(program, true)
}

/// 验证不包含入口函数的库模块。
///
/// 库模块与可执行模块使用相同的声明、调用和类型规则，但不要求定义 `main`。
pub fn check_library(program: &Program) -> Result<TypedProgram, TypeError> {
    check_program(program, false)
}

fn check_program(program: &Program, require_main: bool) -> Result<TypedProgram, TypeError> {
    let console_imported = check_imports(program)?;
    let signatures = collect_signatures(program, require_main)?;
    let declarations = collect_declarations(program)?;
    check_no_recursion(program)?;
    for function in &program.functions {
        check_function(function, &signatures, &declarations, console_imported)?;
    }
    build_typed_program(program, &signatures, &declarations, console_imported)
}

/// 将已完成验证的 HIR 转换为不含 HIR 表达式的 Typed HIR。
fn build_typed_program(
    program: &Program,
    signatures: &HashMap<String, Signature>,
    declarations: &Declarations,
    console_imported: bool,
) -> Result<TypedProgram, TypeError> {
    let functions = program
        .functions
        .iter()
        .map(|function| build_typed_function(function, program, signatures, declarations, console_imported))
        .collect::<Result<Vec<_>, _>>()?;
    let structs = program
        .structs
        .iter()
        .map(|structure| {
            let fields = structure
                .fields
                .iter()
                .map(|field| {
                    Ok(TypedField {
                        id: field.id,
                        ty: field.ty.clone(),
                        default: field
                            .default
                            .as_ref()
                            .map(|value| {
                                build_expression(
                                    value,
                                    &HashMap::new(),
                                    program,
                                    signatures,
                                    declarations,
                                    console_imported,
                                    Some(&field.ty),
                                )
                            })
                            .transpose()?,
                    })
                })
                .collect::<Result<Vec<_>, TypeError>>()?;
            Ok(TypedStruct {
                id: structure.id,
                name: structure.name.clone(),
                fields,
            })
        })
        .collect::<Result<Vec<_>, TypeError>>()?;
    let enums = program
        .enums
        .iter()
        .map(|enumeration| TypedEnum {
            id: enumeration.id,
            name: enumeration.name.clone(),
            variants: enumeration
                .variants
                .iter()
                .map(|variant| TypedVariant {
                    id: variant.id,
                    payload: variant.payload.as_ref().map(|payload| payload.ty.clone()),
                })
                .collect(),
        })
        .collect();
    let newtypes = program
        .newtypes
        .iter()
        .map(|newtype| TypedNewtype {
            id: newtype.id,
            name: newtype.name.clone(),
            underlying: newtype.underlying.clone(),
        })
        .collect();
    Ok(TypedProgram {
        functions,
        structs,
        enums,
        newtypes,
    })
}

/// 生成一个函数的 Typed HIR，并使参数、局部和语句只通过 ID 相互关联。
fn build_typed_function(
    function: &Function,
    program: &Program,
    signatures: &HashMap<String, Signature>,
    declarations: &Declarations,
    console_imported: bool,
) -> Result<TypedFunction, TypeError> {
    let parameters = function
        .parameters
        .iter()
        .map(|parameter| TypedLocal {
            id: parameter.id,
            name: parameter.name.clone(),
            span: parameter.name_span,
            ty: parameter.ty.clone(),
            mutable: false,
        })
        .collect::<Vec<_>>();
    let mut bindings = function
        .parameters
        .iter()
        .map(|parameter| {
            (
                parameter.name.clone(),
                Binding {
                    ty: parameter.ty.clone(),
                    mutable: false,
                },
            )
        })
        .collect::<HashMap<_, _>>();
    let statements = build_statements(
        &function.statements,
        &mut bindings,
        program,
        signatures,
        declarations,
        console_imported,
    )?;
    Ok(TypedFunction {
        id: function.id,
        name: function.name.clone(),
        span: function.name_span,
        parameters,
        statements,
        return_type: function.return_type.clone(),
    })
}

/// 在维护类型绑定环境的同时转换一个语句块。
fn build_statements(
    statements: &[Statement],
    bindings: &mut HashMap<String, Binding>,
    program: &Program,
    signatures: &HashMap<String, Signature>,
    declarations: &Declarations,
    console_imported: bool,
) -> Result<Vec<TypedStatement>, TypeError> {
    let mut typed = Vec::new();
    for statement in statements {
        match statement {
            Statement::Destructure { locals, names, value } => {
                let value = build_expression(value, bindings, program, signatures, declarations, console_imported, None)?;
                let Type::Tuple(elements) = value.ty.clone() else {
                    return Err(error(value.span, "type-checked destructuring requires a tuple value"));
                };
                let locals = names
                    .iter()
                    .zip(locals)
                    .zip(elements)
                    .map(|(((name, span), id), ty)| TypedLocal {
                        id: *id,
                        name: name.clone(),
                        span: *span,
                        ty,
                        mutable: false,
                    })
                    .collect::<Vec<_>>();
                for local in &locals {
                    bindings.insert(local.name.clone(), Binding { ty: local.ty.clone(), mutable: false });
                }
                typed.push(TypedStatement::Destructure { locals, value });
            }
            Statement::Let { local, mutable, name, name_span, annotation, value } => {
                let value = build_expression(value, bindings, program, signatures, declarations, console_imported, annotation.as_ref())?;
                let ty = annotation.clone().unwrap_or_else(|| value.ty.clone());
                let local = TypedLocal { id: *local, name: name.clone(), span: *name_span, ty: ty.clone(), mutable: *mutable };
                bindings.insert(name.clone(), Binding { ty, mutable: *mutable });
                typed.push(TypedStatement::Let { local, value });
            }
            Statement::Assign { local, name_span, value, .. } => {
                let expected = bindings.get(match statement { Statement::Assign { name, .. } => name, _ => unreachable!() }).map(|binding| &binding.ty);
                let value = build_expression(value, bindings, program, signatures, declarations, console_imported, expected)?;
                typed.push(TypedStatement::Assign { local: *local, value, span: *name_span });
            }
            Statement::Expression(value) => typed.push(TypedStatement::Expression(build_expression(
                value, bindings, program, signatures, declarations, console_imported, None,
            )?)),
        }
    }
    Ok(typed)
}

/// 将单个已验证 HIR 表达式转换为自包含 Typed HIR 节点。
fn build_expression(
    expression: &Expression,
    bindings: &HashMap<String, Binding>,
    program: &Program,
    signatures: &HashMap<String, Signature>,
    declarations: &Declarations,
    console_imported: bool,
    expected: Option<&Type>,
) -> Result<TypedExpression, TypeError> {
    let span = expression.span();
    let ty = if matches!(expression, Expression::Variable { name, .. } if name == "None") {
        expected.cloned().ok_or_else(|| error(span, "`None` requires an Option type context"))?
    } else {
        type_of(expression, bindings, signatures, declarations, console_imported)?
    };
    let kind = match expression {
        Expression::Integer { value, .. } => TypedExpressionKind::Integer(*value),
        Expression::Float { value, .. } => TypedExpressionKind::Float(value.clone()),
        Expression::Boolean { value, .. } => TypedExpressionKind::Boolean(*value),
        Expression::String { parts, .. } => TypedExpressionKind::String(
            parts
                .iter()
                .map(|part| match part {
                    StringPart::Text(text) => Ok(TypedStringPart::Text(text.clone())),
                    StringPart::Variable { local, name, span } => local
                        .filter(|_| bindings.contains_key(name))
                        .map(TypedStringPart::Local)
                        .ok_or_else(|| error(*span, format!("undefined variable `{name}`"))),
                })
                .collect::<Result<Vec<_>, _>>()?,
        ),
        Expression::List { values, .. } => TypedExpressionKind::List(
            values
                .iter()
                .map(|value| build_expression(value, bindings, program, signatures, declarations, console_imported, None))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        Expression::Map { entries, .. } => TypedExpressionKind::Map(
            entries
                .iter()
                .map(|entry| Ok((entry.key.clone(), build_expression(&entry.value, bindings, program, signatures, declarations, console_imported, None)?)))
                .collect::<Result<Vec<_>, TypeError>>()?,
        ),
        Expression::Tuple { values, .. } => TypedExpressionKind::Tuple(
            values
                .iter()
                .map(|value| build_expression(value, bindings, program, signatures, declarations, console_imported, None))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        Expression::Match { target, arms, .. } => {
            let typed_target = build_expression(target, bindings, program, signatures, declarations, console_imported, None)?;
            let target_type = typed_target.ty.clone();
            let arms = arms
                .iter()
                .map(|arm| {
                    let pattern = match arm.pattern.variant_id {
                        Some(id) => TypedPattern::Variant(id),
                        None => match arm.pattern.variant.as_str() {
                            "Some" => TypedPattern::Some,
                            "None" => TypedPattern::None,
                            "Ok" => TypedPattern::Ok,
                            "Err" => TypedPattern::Err,
                            _ => return Err(error(arm.pattern.variant_span, "type-checked match has an unresolved variant")),
                        },
                    };
                    let mut arm_bindings = bindings.clone();
                    let binding = match (&arm.pattern.binding, arm.pattern.binding_local) {
                        (Some((name, span)), Some(id)) => {
                            let ty = match_binding_type(&target_type, &arm.pattern, declarations)
                                .ok_or_else(|| error(*span, "type-checked match binding has no payload type"))?;
                            arm_bindings.insert(name.clone(), Binding { ty: ty.clone(), mutable: false });
                            Some(TypedLocal { id, name: name.clone(), span: *span, ty, mutable: false })
                        }
                        (None, None) => None,
                        _ => return Err(error(arm.value.span(), "type-checked match binding is inconsistent")),
                    };
                    let value = build_expression(&arm.value, &arm_bindings, program, signatures, declarations, console_imported, None)?;
                    Ok(TypedMatchArm { pattern, binding, value })
                })
                .collect::<Result<Vec<_>, TypeError>>()?;
            TypedExpressionKind::Match { target: Box::new(typed_target), arms }
        }
        Expression::If { condition, then_statements, else_statements, .. } => {
            let condition = Box::new(build_expression(condition, bindings, program, signatures, declarations, console_imported, Some(&Type::Bool))?);
            let mut then_bindings = bindings.clone();
            let then_statements = build_statements(then_statements, &mut then_bindings, program, signatures, declarations, console_imported)?;
            let mut else_bindings = bindings.clone();
            let else_statements = build_statements(else_statements, &mut else_bindings, program, signatures, declarations, console_imported)?;
            TypedExpressionKind::If { condition, then_statements, else_statements }
        }
        Expression::For { local, name, name_span, iterable, statements, .. } => {
            let iterable = Box::new(build_expression(iterable, bindings, program, signatures, declarations, console_imported, None)?);
            let Type::List(element) = iterable.ty.clone() else {
                return Err(error(iterable.span, "type-checked for must iterate over a List value"));
            };
            let local = TypedLocal { id: *local, name: name.clone(), span: *name_span, ty: *element, mutable: false };
            let mut loop_bindings = bindings.clone();
            loop_bindings.insert(name.clone(), Binding { ty: local.ty.clone(), mutable: false });
            let statements = build_statements(statements, &mut loop_bindings, program, signatures, declarations, console_imported)?;
            TypedExpressionKind::For { local, iterable, statements }
        }
        Expression::Return { value, .. } => TypedExpressionKind::Return(Box::new(build_expression(value, bindings, program, signatures, declarations, console_imported, None)?)),
        Expression::Try { value, .. } => TypedExpressionKind::Try(Box::new(build_expression(value, bindings, program, signatures, declarations, console_imported, None)?)),
        Expression::Variable { name, local, .. } if name == "None" => TypedExpressionKind::None,
        Expression::Variable { name, local, .. } => TypedExpressionKind::Local(
            (*local)
                .or_else(|| local_for_name(program, name))
                .ok_or_else(|| error(span, format!("undefined variable `{name}`")))?,
        ),
        Expression::Call { path, arguments, function, .. } => {
            let target = typed_call_target(path, *function, program, span)?;
            let expected_arguments = expected_argument_types(&target, path, program, signatures);
            let arguments = arguments
                .iter()
                .enumerate()
                .map(|(index, argument)| build_expression(argument, bindings, program, signatures, declarations, console_imported, expected_arguments.get(index).copied()))
                .collect::<Result<Vec<_>, _>>()?;
            TypedExpressionKind::Call { target, arguments }
        }
        Expression::Add { left, right, .. } => TypedExpressionKind::Add(
            Box::new(build_expression(left, bindings, program, signatures, declarations, console_imported, Some(&Type::Int))?),
            Box::new(build_expression(right, bindings, program, signatures, declarations, console_imported, Some(&Type::Int))?),
        ),
        Expression::Multiply { left, right, .. } => TypedExpressionKind::Multiply(
            Box::new(build_expression(left, bindings, program, signatures, declarations, console_imported, Some(&Type::Int))?),
            Box::new(build_expression(right, bindings, program, signatures, declarations, console_imported, Some(&Type::Int))?),
        ),
        Expression::Equal { left, right, .. } => TypedExpressionKind::Equal(
            Box::new(build_expression(left, bindings, program, signatures, declarations, console_imported, None)?),
            Box::new(build_expression(right, bindings, program, signatures, declarations, console_imported, None)?),
        ),
        Expression::StructLiteral { structure, name, fields, .. } => TypedExpressionKind::Struct {
            structure: program
                .structs
                .iter()
                .find(|candidate| candidate.name == *name)
                .map(|candidate| candidate.id)
                .unwrap_or(*structure),
            fields: fields
                .iter()
                .map(|field| {
                    let field_id = field.field_id.or_else(|| {
                        program
                            .structs
                            .iter()
                            .find(|candidate| candidate.name == *name)
                            .and_then(|candidate| {
                                candidate
                                    .fields
                                    .iter()
                                    .find(|candidate| candidate.name == field.name)
                            })
                            .map(|candidate| candidate.id)
                    }).ok_or_else(|| error(field.name_span, "type-checked struct field has no ID"))?;
                    Ok((field_id, build_expression(&field.value, bindings, program, signatures, declarations, console_imported, None)?))
                })
                .collect::<Result<Vec<_>, TypeError>>()?,
        },
        Expression::FieldAccess { target, field_id, variant, .. } if let Some(variant) = variant => TypedExpressionKind::Variant(*variant),
        Expression::FieldAccess { target, field_id, .. } => {
            let target = Box::new(build_expression(target, bindings, program, signatures, declarations, console_imported, None)?);
            let field = (*field_id).or_else(|| field_id_for_type(program, &target.ty, expression).copied())
                .ok_or_else(|| error(span, "type-checked field access has no field ID"))?;
            TypedExpressionKind::Field { target, field }
        }
    };
    Ok(TypedExpression { span, ty, kind })
}

/// 根据稳定函数内局部 ID 还原绑定；该辅助仅用于将旧 HIR 的字符串插值转入 Typed HIR。
fn local_for_name(program: &Program, name: &str) -> Option<LocalId> {
    program.functions.iter().find_map(|function| {
        function
            .parameters
            .iter()
            .find(|parameter| parameter.name == name)
            .map(|parameter| parameter.id)
            .or_else(|| local_in_statements(&function.statements, name))
    })
}

/// 在 HIR 的已解析绑定声明中定位局部 ID；Typed HIR 不保留这个按名称的回退路径。
fn local_in_statements(statements: &[Statement], name: &str) -> Option<LocalId> {
    statements.iter().find_map(|statement| match statement {
        Statement::Let { local, name: candidate, .. } if candidate == name => Some(*local),
        Statement::Destructure { locals, names, .. } => names
            .iter()
            .position(|(candidate, _)| candidate == name)
            .and_then(|index| locals.get(index).copied()),
        Statement::Expression(expression) => local_in_expression(expression, name),
        Statement::Let { value, .. }
        | Statement::Assign { value, .. }
        | Statement::Destructure { value, .. } => local_in_expression(value, name),
        _ => None,
    })
}

fn local_in_expression(expression: &Expression, name: &str) -> Option<LocalId> {
    match expression {
        Expression::If { then_statements, else_statements, .. } => local_in_statements(then_statements, name).or_else(|| local_in_statements(else_statements, name)),
        Expression::For { local, name: candidate, statements, .. } => (candidate == name).then_some(*local).or_else(|| local_in_statements(statements, name)),
        Expression::Match { arms, .. } => arms.iter().find_map(|arm| {
            arm.pattern.binding.as_ref().filter(|(candidate, _)| candidate == name).and(arm.pattern.binding_local).or_else(|| local_in_expression(&arm.value, name))
        }),
        _ => None,
    }
}

fn typed_call_target(path: &[String], function: Option<DefId>, program: &Program, span: Span) -> Result<TypedCallTarget, TypeError> {
    match path.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["bytes", "from_hex"] => Ok(TypedCallTarget::BytesFromHex),
        ["console", "println"] => Ok(TypedCallTarget::ConsolePrintln),
        ["Some"] => Ok(TypedCallTarget::Some),
        ["Ok"] => Ok(TypedCallTarget::Ok),
        ["Err"] => Ok(TypedCallTarget::Err),
        [receiver, "to_int"] => local_for_name(program, receiver).map(TypedCallTarget::StringToInt).ok_or_else(|| error(span, "type-checked string receiver has no local ID")),
        [name] => {
            if let Some(id) = function.or_else(|| {
                program
                    .functions
                    .iter()
                    .find(|candidate| candidate.name == *name)
                    .map(|candidate| candidate.id)
            }) {
                return Ok(TypedCallTarget::Function(id));
            }
            program.newtypes.iter().find(|item| item.name == *name).map(|item| TypedCallTarget::Newtype(item.id)).ok_or_else(|| error(span, "type-checked call has no target ID"))
        }
        [enum_name, variant_name] => program.enums.iter().find(|item| item.name == *enum_name).and_then(|item| item.variants.iter().find(|variant| variant.name == *variant_name)).map(|variant| TypedCallTarget::Variant(variant.id)).ok_or_else(|| error(span, "type-checked enum constructor has no variant ID")),
        _ => Err(error(span, "type-checked call path is unsupported")),
    }
}

fn expected_argument_types<'a>(target: &TypedCallTarget, path: &[String], program: &'a Program, signatures: &'a HashMap<String, Signature>) -> Vec<&'a Type> {
    match target {
        TypedCallTarget::Function(_) => path.first().and_then(|name| signatures.get(name)).map(|signature| signature.parameters.iter().collect()).unwrap_or_default(),
        TypedCallTarget::Newtype(id) => program.newtypes.iter().find(|item| item.id == *id).map(|item| vec![&item.underlying]).unwrap_or_default(),
        TypedCallTarget::Variant(id) => program.enums.iter().flat_map(|item| &item.variants).find(|item| item.id == *id).and_then(|item| item.payload.as_ref().map(|payload| vec![&payload.ty])).unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn field_id_for_type<'a>(program: &'a Program, ty: &Type, expression: &Expression) -> Option<&'a FieldId> {
    let Type::Named(name) = ty else { return None; };
    let Expression::FieldAccess { field, .. } = expression else { return None; };
    program.structs.iter().find(|structure| structure.name == *name).and_then(|structure| structure.fields.iter().find(|candidate| candidate.name == *field)).map(|field| &field.id)
}

/// 旧实现的内部验证记录，保留至节点构造迁移完成后删除。
///
/// 它不属于 `TypedProgram`，后续阶段不可见也不可消费。
#[derive(Clone, Debug, Eq, PartialEq)]
struct RecordedExpression {
    span: Span,
    ty: Type,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct ResolvedLocalId(u32);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ResolvedTarget {
    Local(ResolvedLocalId),
    Function(DefId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ResolvedReference {
    span: Span,
    target: ResolvedTarget,
}

/// 收集函数顶层顺序语句的名称解析结果。
fn collect_sequential_references(
    program: &Program,
    functions: &[TypedFunction],
) -> Vec<ResolvedReference> {
    let definitions = functions
        .iter()
        .map(|function| (function.name.as_str(), function.id))
        .collect::<HashMap<_, _>>();
    let mut references = Vec::new();
    for function in &program.functions {
        let mut locals = HashMap::new();
        let mut next = 0_u32;
        for parameter in &function.parameters {
            locals.insert(parameter.name.as_str(), ResolvedLocalId(next));
            next += 1;
        }
        for statement in &function.statements {
            match statement {
                Statement::Let { name, value, .. } => {
                    collect_direct_references(value, &locals, &definitions, &mut references);
                    locals.insert(name.as_str(), ResolvedLocalId(next));
                    next += 1;
                }
                Statement::Assign {
                    name,
                    name_span,
                    value,
                    ..
                } => {
                    if let Some(local) = locals.get(name.as_str()) {
                        references.push(ResolvedReference {
                            span: *name_span,
                            target: ResolvedTarget::Local(*local),
                        });
                    }
                    collect_direct_references(value, &locals, &definitions, &mut references);
                }
                Statement::Expression(value) | Statement::Destructure { value, .. } => {
                    collect_direct_references(value, &locals, &definitions, &mut references);
                }
            }
        }
    }
    references
}

/// 记录一个顺序表达式直接包含的变量读取与普通函数调用。
fn collect_direct_references(
    expression: &Expression,
    locals: &HashMap<&str, ResolvedLocalId>,
    functions: &HashMap<&str, DefId>,
    references: &mut Vec<ResolvedReference>,
) {
    match expression {
        Expression::Variable { name, span, .. } => {
            if let Some(local) = locals.get(name.as_str()) {
                references.push(ResolvedReference {
                    span: *span,
                    target: ResolvedTarget::Local(*local),
                });
            }
        }
        Expression::Call {
            path,
            arguments,
            span,
            ..
        } => {
            if let [name] = path.as_slice() {
                if let Some(function) = functions.get(name.as_str()) {
                    references.push(ResolvedReference {
                        span: *span,
                        target: ResolvedTarget::Function(*function),
                    });
                }
            }
            for argument in arguments {
                collect_direct_references(argument, locals, functions, references);
            }
        }
        Expression::Add { left, right, .. }
        | Expression::Multiply { left, right, .. }
        | Expression::Equal { left, right, .. } => {
            collect_direct_references(left, locals, functions, references);
            collect_direct_references(right, locals, functions, references);
        }
        Expression::Return { value, .. }
        | Expression::Try { value, .. }
        | Expression::FieldAccess { target: value, .. } => {
            collect_direct_references(value, locals, functions, references)
        }
        Expression::List { values, .. } | Expression::Tuple { values, .. } => {
            for value in values {
                collect_direct_references(value, locals, functions, references);
            }
        }
        Expression::Map { entries, .. } => {
            for entry in entries {
                collect_direct_references(&entry.value, locals, functions, references);
            }
        }
        Expression::StructLiteral { fields, .. } => {
            for field in fields {
                collect_direct_references(&field.value, locals, functions, references);
            }
        }
        Expression::Integer { .. }
        | Expression::Float { .. }
        | Expression::Boolean { .. }
        | Expression::String { .. }
        | Expression::Match { .. }
        | Expression::If { .. }
        | Expression::For { .. } => {}
    }
}

/// 在既有类型检查成功后，以相同作用域规则记录每个值表达式的确定类型。
///
/// 此处不产生新的类型结论；每次记录仍调用 `type_of`，因此若前一阶段的规则与记录过程
/// 不一致会立刻作为内部检查错误暴露，而不会向 MIR 暴露未验证表达式。
fn collect_expression_types(
    program: &Program,
    signatures: &HashMap<String, Signature>,
    declarations: &Declarations,
    console_imported: bool,
) -> Result<Vec<RecordedExpression>, TypeError> {
    let mut entries = Vec::new();
    for structure in &program.structs {
        for field in &structure.fields {
            if let Some(default) = &field.default {
                record_expression(
                    default,
                    &HashMap::new(),
                    signatures,
                    declarations,
                    console_imported,
                    &mut entries,
                )?;
            }
        }
    }
    for function in &program.functions {
        let mut bindings = HashMap::new();
        for parameter in &function.parameters {
            bindings.insert(
                parameter.name.clone(),
                Binding {
                    ty: parameter.ty.clone(),
                    mutable: false,
                },
            );
        }
        record_statements(
            &function.statements,
            &mut bindings,
            signatures,
            declarations,
            console_imported,
            &mut entries,
        )?;
    }
    Ok(entries)
}

/// 按源码顺序记录语句中的表达式，并同步更新局部绑定环境。
fn record_statements(
    statements: &[Statement],
    bindings: &mut HashMap<String, Binding>,
    signatures: &HashMap<String, Signature>,
    declarations: &Declarations,
    console_imported: bool,
    entries: &mut Vec<RecordedExpression>,
) -> Result<(), TypeError> {
    for statement in statements {
        match statement {
            Statement::Destructure { names, value, .. } => {
                record_expression(
                    value,
                    bindings,
                    signatures,
                    declarations,
                    console_imported,
                    entries,
                )?;
                let Type::Tuple(elements) =
                    type_of(value, bindings, signatures, declarations, console_imported)?
                else {
                    return Err(error(
                        value.span(),
                        "type-checked destructuring requires a tuple value",
                    ));
                };
                for ((name, _), ty) in names.iter().zip(elements) {
                    bindings.insert(name.clone(), Binding { ty, mutable: false });
                }
            }
            Statement::Let {
                mutable,
                name,
                value,
                ..
            } => {
                record_expression(
                    value,
                    bindings,
                    signatures,
                    declarations,
                    console_imported,
                    entries,
                )?;
                let ty = type_of(value, bindings, signatures, declarations, console_imported)?;
                bindings.insert(
                    name.clone(),
                    Binding {
                        ty,
                        mutable: *mutable,
                    },
                );
            }
            Statement::Assign { value, .. } | Statement::Expression(value) => record_expression(
                value,
                bindings,
                signatures,
                declarations,
                console_imported,
                entries,
            )?,
        }
    }
    Ok(())
}

/// 记录一个表达式及其子表达式的类型，并为嵌套块建立独立的局部作用域。
fn record_expression(
    expression: &Expression,
    bindings: &HashMap<String, Binding>,
    signatures: &HashMap<String, Signature>,
    declarations: &Declarations,
    console_imported: bool,
    entries: &mut Vec<RecordedExpression>,
) -> Result<(), TypeError> {
    // `None` 没有独立类型，只能由函数参数等 Option 上下文推断；父表达式已记录该结论。
    if matches!(expression, Expression::Variable { name, .. } if name == "None") {
        return Ok(());
    }
    let ty = type_of(
        expression,
        bindings,
        signatures,
        declarations,
        console_imported,
    )?;
    entries.push(RecordedExpression {
        span: expression.span(),
        ty,
    });
    match expression {
        Expression::List { values, .. } | Expression::Tuple { values, .. } => {
            for value in values {
                record_expression(
                    value,
                    bindings,
                    signatures,
                    declarations,
                    console_imported,
                    entries,
                )?;
            }
        }
        Expression::Map { entries: map, .. } => {
            for entry in map {
                record_expression(
                    &entry.value,
                    bindings,
                    signatures,
                    declarations,
                    console_imported,
                    entries,
                )?;
            }
        }
        Expression::Match { target, arms, .. } => {
            record_expression(
                target,
                bindings,
                signatures,
                declarations,
                console_imported,
                entries,
            )?;
            let target_type =
                type_of(target, bindings, signatures, declarations, console_imported)?;
            for arm in arms {
                let mut arm_bindings = bindings.clone();
                if let Some((name, _)) = &arm.pattern.binding {
                    if let Some(ty) = match_binding_type(&target_type, &arm.pattern, declarations) {
                        arm_bindings.insert(name.clone(), Binding { ty, mutable: false });
                    }
                }
                record_expression(
                    &arm.value,
                    &arm_bindings,
                    signatures,
                    declarations,
                    console_imported,
                    entries,
                )?;
            }
        }
        Expression::If {
            condition,
            then_statements,
            else_statements,
            ..
        } => {
            record_expression(
                condition,
                bindings,
                signatures,
                declarations,
                console_imported,
                entries,
            )?;
            record_block(
                then_statements,
                bindings,
                signatures,
                declarations,
                console_imported,
                entries,
            )?;
            record_block(
                else_statements,
                bindings,
                signatures,
                declarations,
                console_imported,
                entries,
            )?;
        }
        Expression::For {
            name,
            iterable,
            statements,
            ..
        } => {
            record_expression(
                iterable,
                bindings,
                signatures,
                declarations,
                console_imported,
                entries,
            )?;
            let mut loop_bindings = bindings.clone();
            if let Type::List(element) = type_of(
                iterable,
                bindings,
                signatures,
                declarations,
                console_imported,
            )? {
                loop_bindings.insert(
                    name.clone(),
                    Binding {
                        ty: *element,
                        mutable: false,
                    },
                );
            }
            record_block(
                statements,
                &loop_bindings,
                signatures,
                declarations,
                console_imported,
                entries,
            )?;
        }
        Expression::Return { value, .. } | Expression::Try { value, .. } => record_expression(
            value,
            bindings,
            signatures,
            declarations,
            console_imported,
            entries,
        )?,
        Expression::FieldAccess { target, .. } if matches!(target.as_ref(), Expression::Variable { name, .. } if declarations.enums.contains_key(name)) =>
            {}
        Expression::FieldAccess { target, .. } => record_expression(
            target,
            bindings,
            signatures,
            declarations,
            console_imported,
            entries,
        )?,
        Expression::Call { arguments, .. } => {
            for argument in arguments {
                record_expression(
                    argument,
                    bindings,
                    signatures,
                    declarations,
                    console_imported,
                    entries,
                )?;
            }
        }
        Expression::Add { left, right, .. }
        | Expression::Multiply { left, right, .. }
        | Expression::Equal { left, right, .. } => {
            record_expression(
                left,
                bindings,
                signatures,
                declarations,
                console_imported,
                entries,
            )?;
            record_expression(
                right,
                bindings,
                signatures,
                declarations,
                console_imported,
                entries,
            )?;
        }
        Expression::StructLiteral { fields, .. } => {
            for field in fields {
                record_expression(
                    &field.value,
                    bindings,
                    signatures,
                    declarations,
                    console_imported,
                    entries,
                )?;
            }
        }
        Expression::Integer { .. }
        | Expression::Float { .. }
        | Expression::Boolean { .. }
        | Expression::String { .. }
        | Expression::Variable { .. } => {}
    }
    Ok(())
}

/// 记录嵌套块时克隆外层绑定，避免块内声明泄漏到父作用域。
fn record_block(
    statements: &[Statement],
    bindings: &HashMap<String, Binding>,
    signatures: &HashMap<String, Signature>,
    declarations: &Declarations,
    console_imported: bool,
    entries: &mut Vec<RecordedExpression>,
) -> Result<(), TypeError> {
    let mut block_bindings = bindings.clone();
    record_statements(
        statements,
        &mut block_bindings,
        signatures,
        declarations,
        console_imported,
        entries,
    )
}

/// 返回 match 分支载荷在该分支内对应的局部绑定类型。
fn match_binding_type(
    target: &Type,
    pattern: &yan_hir::EnumPattern,
    declarations: &Declarations,
) -> Option<Type> {
    match target {
        Type::Option(element) if pattern.variant == "Some" => Some((**element).clone()),
        Type::Result(ok, _) if pattern.variant == "Ok" => Some((**ok).clone()),
        Type::Result(_, error) if pattern.variant == "Err" => Some((**error).clone()),
        Type::Named(name) => declarations
            .enums
            .get(name)
            .and_then(|variants| {
                variants
                    .iter()
                    .find(|variant| variant.name == pattern.variant)
            })
            .and_then(|variant| variant.payload.as_ref())
            .map(|payload| payload.ty.clone()),
        _ => None,
    }
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
        Type::Tuple(elements) => {
            for element in elements {
                check_declared_type(element, declarations, span)?;
            }
            Ok(())
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
        Statement::Destructure { value, .. } => expression_calls(value),
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
            ..
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
        Expression::Tuple { values, .. } => values.iter().flat_map(expression_calls).collect(),
        Expression::Match { target, arms, .. } => {
            let mut calls = expression_calls(target);
            calls.extend(arms.iter().flat_map(|arm| expression_calls(&arm.value)));
            calls
        }
        Expression::If {
            condition,
            then_statements,
            else_statements,
            ..
        } => {
            let mut calls = expression_calls(condition);
            calls.extend(then_statements.iter().flat_map(statement_calls));
            calls.extend(else_statements.iter().flat_map(statement_calls));
            calls
        }
        Expression::For {
            iterable,
            statements,
            ..
        } => {
            let mut calls = expression_calls(iterable);
            calls.extend(statements.iter().flat_map(statement_calls));
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

fn collect_signatures(
    program: &Program,
    require_main: bool,
) -> Result<HashMap<String, Signature>, TypeError> {
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

    if require_main && main_count != 1 {
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
        Statement::Destructure { names, value, .. } => {
            if names.len() < 2 || names.len() > 3 {
                return Err(error(
                    value.span(),
                    "tuple destructuring requires two or three names",
                ));
            }
            let Type::Tuple(elements) =
                type_of(value, bindings, signatures, declarations, console_imported)?
            else {
                return Err(error(
                    value.span(),
                    "tuple destructuring requires a tuple value",
                ));
            };
            if elements.len() != names.len() {
                return Err(error(
                    value.span(),
                    "tuple destructuring length does not match value type",
                ));
            }
            for ((name, span), ty) in names.iter().zip(elements) {
                if bindings.contains_key(name) {
                    return Err(error(
                        *span,
                        format!("variable `{name}` is already defined"),
                    ));
                }
                bindings.insert(name.clone(), Binding { ty, mutable: false });
            }
            Ok(None)
        }
        Statement::Let {
            mutable,
            name,
            name_span,
            annotation,
            value,
            ..
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
            ..
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

/// 在独立的局部作用域中检查语句块，并返回其最后一个表达式的类型。
fn type_of_block(
    statements: &[Statement],
    bindings: &HashMap<String, Binding>,
    signatures: &HashMap<String, Signature>,
    declarations: &Declarations,
    console_imported: bool,
) -> Result<Type, TypeError> {
    let mut local_bindings = bindings.clone();
    let mut tail_type = Type::Unit;
    let count = statements.len();
    for (index, statement) in statements.iter().enumerate() {
        if let Some(ty) = check_statement(
            statement,
            &mut local_bindings,
            signatures,
            declarations,
            console_imported,
        )? {
            if index + 1 == count {
                tail_type = ty;
            } else if ty != Type::Unit {
                return Err(error(
                    statement_span(statement),
                    "only the final expression in a block may produce a return value",
                ));
            }
        }
    }
    Ok(tail_type)
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
                if let StringPart::Variable { name, span, .. } = part {
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
        Expression::Variable { name, span, .. } => bindings
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
        Expression::Tuple { values, span } => {
            if values.len() < 2 || values.len() > 3 {
                return Err(error(*span, "tuple literals require two or three values"));
            }
            Ok(Type::Tuple(
                values
                    .iter()
                    .map(|value| {
                        type_of(value, bindings, signatures, declarations, console_imported)
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            ))
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
        Expression::If {
            condition,
            then_statements,
            else_statements,
            span,
        } => {
            if type_of(
                condition,
                bindings,
                signatures,
                declarations,
                console_imported,
            )? != Type::Bool
            {
                return Err(error(condition.span(), "if condition must have type bool"));
            }
            let then_type = type_of_block(
                then_statements,
                bindings,
                signatures,
                declarations,
                console_imported,
            )?;
            let else_type = type_of_block(
                else_statements,
                bindings,
                signatures,
                declarations,
                console_imported,
            )?;
            if !types_compatible(&then_type, &else_type) {
                return Err(error(*span, "if branch result types must be the same"));
            }
            Ok(if then_type == Type::Never {
                else_type
            } else {
                then_type
            })
        }
        Expression::For {
            name,
            name_span,
            iterable,
            statements,
            span,
            ..
        } => {
            if bindings.contains_key(name) {
                return Err(error(
                    *name_span,
                    format!("variable `{name}` is already defined"),
                ));
            }
            let Type::List(element) = type_of(
                iterable,
                bindings,
                signatures,
                declarations,
                console_imported,
            )?
            else {
                return Err(error(iterable.span(), "for requires a List value"));
            };
            let mut loop_bindings = bindings.clone();
            loop_bindings.insert(
                name.clone(),
                Binding {
                    ty: *element,
                    mutable: false,
                },
            );
            let body_type = type_of_block(
                statements,
                &loop_bindings,
                signatures,
                declarations,
                console_imported,
            )?;
            if !types_compatible(&body_type, &Type::Unit) {
                return Err(error(
                    statements.last().map(statement_span).unwrap_or(*span),
                    "for body must not produce a return value",
                ));
            }
            Ok(Type::Unit)
        }
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
            ..
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
            ..
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
            ..
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
            ..
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
            ..
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
            ..
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
            ..
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
            ..
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
            ..
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
        Statement::Destructure { names, value, .. } => {
            names.first().map(|(_, span)| *span).unwrap_or(value.span())
        }
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
    use yan_hir::{lower, Type};
    use yan_syntax::{lex, parse};

    use super::{check, check_library, TypedStatement};

    fn check_source(source: &str) -> Result<(), super::TypeError> {
        let tokens = lex(source).expect("测试源码应完成词法分析");
        let syntax = parse(source, &tokens).expect("测试源码应完成语法分析");
        let program = lower(syntax).expect("测试源码应完成 lowering");
        check(&program).map(|_| ())
    }

    #[test]
    fn checks_function_call_and_interpolation() {
        let source = "import yan.platform.console fn twice(value: int) -> int { value * 2 } fn label(total: int) -> string { \"total: {total}\" } fn main() -> unit { let total = twice(3) console.println(label(total)) }";

        check_source(source).expect("函数调用和字符串插值应通过类型检查");
    }

    #[test]
    fn builds_typed_nodes_for_checked_value_expressions() {
        let source = "import yan.platform.console fn main() -> unit { let count = 1 console.println(count) }";
        let tokens = lex(source).expect("测试源码应完成词法分析");
        let syntax = parse(source, &tokens).expect("测试源码应完成语法分析");
        let program = lower(syntax).expect("测试源码应完成 lowering");

        let typed = check(&program).expect("测试源码应通过类型检查");

        let main = typed
            .functions
            .iter()
            .find(|function| function.name == "main")
            .expect("类型检查必须产出 main 的 Typed HIR");
        assert!(matches!(
            main.statements.first(),
            Some(TypedStatement::Let { value, .. }) if value.ty == Type::Int
        ));
        assert!(matches!(
            main.statements.last(),
            Some(TypedStatement::Expression(value)) if value.ty == Type::Unit
        ));
    }

    #[test]
    fn checks_library_module_without_main() {
        let source = "pub fn greeting() -> string { \"hello\" }";
        let tokens = lex(source).expect("测试源码应完成词法分析");
        let syntax = parse(source, &tokens).expect("测试源码应完成语法分析");
        let program = lower(syntax).expect("测试源码应完成 lowering");

        check_library(&program).expect("库模块不应要求 main 函数");
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
    fn checks_bool_if_expression_and_unit_for_loop() {
        let source = "import yan.platform.console fn label(ready: bool) -> string { if ready { \"ready\" } else { \"pending\" } } fn main() -> unit { let targets = [\"cli\", \"web\"] for target in targets { console.println(label(target == \"cli\")) } }";

        check_source(source).expect("if 和 for 应通过类型检查");
    }

    #[test]
    fn rejects_non_bool_if_condition() {
        let error = check_source("fn main() -> unit { if 1 { } else { } }")
            .expect_err("非 bool 条件必须失败");

        assert_eq!(error.message, "if condition must have type bool");
    }

    #[test]
    fn rejects_value_producing_for_body() {
        let error = check_source("fn main() -> unit { for value in [1, 2] { value } }")
            .expect_err("for 循环体不能产生值");

        assert_eq!(error.message, "for body must not produce a return value");
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

    #[test]
    fn checks_three_element_tuple_destructuring() {
        let source = "fn values() -> (int, string, bool) { (1, \"yan\", true) } fn main() -> unit { let (count, name, enabled) = values() }";

        check_source(source).expect("三元元组应能返回并解构");
    }
}
