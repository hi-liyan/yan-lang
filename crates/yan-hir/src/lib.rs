//! 与 parser 和执行后端解耦的 Yan 高层中间表示。

use yan_source::Span;
use yan_syntax::{
    Enum as SyntaxEnum, Expression as SyntaxExpression, Field as SyntaxField,
    MapEntry as SyntaxMapEntry, MatchArm as SyntaxMatchArm, Statement as SyntaxStatement,
    SyntaxProgram, TypeSyntax,
};

/// 已降低为编译器语义阶段使用的程序。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Program {
    /// 源文件声明的模块路径；M3 允许省略。
    pub module: Option<Vec<String>>,
    /// 显式导入的模块路径。
    pub imports: Vec<Vec<String>>,
    /// 源文件中的真正新类型声明。
    pub newtypes: Vec<Newtype>,
    /// 源文件中的结构体声明。
    pub structs: Vec<Struct>,
    /// 源文件中的封闭枚举声明。
    pub enums: Vec<Enum>,
    /// 程序定义的函数。
    pub functions: Vec<Function>,
}

/// 已 lowering 的真正新类型声明。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Newtype {
    /// 新类型名称。
    pub name: String,
    /// 新类型名称的位置。
    pub name_span: Span,
    /// 新类型包装的底层类型。
    pub underlying: Type,
}

/// 已 lowering 的具名结构体声明。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Struct {
    /// 结构体名称。
    pub name: String,
    /// 结构体名称的位置。
    pub name_span: Span,
    /// 按声明顺序排列的字段。
    pub fields: Vec<Field>,
}

/// 已 lowering 的封闭枚举声明。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Enum {
    /// 枚举名称。
    pub name: String,
    /// 枚举名称在源文件中的位置。
    pub name_span: Span,
    /// 按声明顺序排列的变体。
    pub variants: Vec<EnumVariant>,
}

/// 已 lowering 的枚举变体。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnumVariant {
    /// 变体名称。
    pub name: String,
    /// 变体名称在源文件中的位置。
    pub name_span: Span,
    /// 可选的单个具名载荷。
    pub payload: Option<EnumPayload>,
}

/// 枚举单载荷变体的名称与类型。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnumPayload {
    /// 载荷名称。
    pub name: String,
    /// 载荷名称在源文件中的位置。
    pub name_span: Span,
    /// 载荷类型。
    pub ty: Type,
}

/// 已 lowering 的结构体字段。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Field {
    /// 字段名称。
    pub name: String,
    /// 字段名称的位置。
    pub name_span: Span,
    /// 字段类型。
    pub ty: Type,
    /// 声明时给出的可选默认值。
    pub default: Option<Expression>,
}

/// M3 支持的函数定义。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Function {
    /// 函数名称。
    pub name: String,
    /// 函数名称的位置。
    pub name_span: Span,
    /// 按声明顺序排列的具名参数。
    pub parameters: Vec<Parameter>,
    /// 函数的声明返回类型。
    pub return_type: Type,
    /// 函数体语句。
    pub statements: Vec<Statement>,
}

/// 已 lowering 的函数参数。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Parameter {
    /// 参数名称。
    pub name: String,
    /// 参数名称的位置。
    pub name_span: Span,
    /// 参数的显式类型。
    pub ty: Type,
}

/// M3 支持的语言类型。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Type {
    /// 有符号 64 位整数。
    Int,
    /// IEEE 754 双精度浮点数。
    Float,
    /// 不可变字节序列。
    Bytes,
    /// 布尔值。
    Bool,
    /// UTF-8 文本。
    String,
    /// 无有效返回值。
    Unit,
    /// 元素类型统一的有序列表。
    List(Box<Type>),
    /// 键固定为 string、值类型统一的不可变 map。
    Map(Box<Type>),
    /// 由源文件声明的名义类型，包括新类型和结构体。
    Named(String),
}

/// HIR 语句。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Statement {
    /// 声明局部变量。
    Let {
        mutable: bool,
        name: String,
        name_span: Span,
        annotation: Option<Type>,
        value: Expression,
    },
    /// 重写已有变量的值。
    Assign {
        name: String,
        name_span: Span,
        value: Expression,
    },
    /// 为副作用执行表达式。
    Expression(Expression),
}

/// HIR 表达式。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Expression {
    /// 整数字面量。
    Integer { value: i64, span: Span },
    /// 浮点数字面量。
    Float { value: String, span: Span },
    /// 布尔字面量。
    Boolean { value: bool, span: Span },
    /// 由文本和变量插值片段构成的字符串字面量。
    String { parts: Vec<StringPart>, span: Span },
    /// 列表字面量。
    List { values: Vec<Expression>, span: Span },
    /// 键为 string 的 map 字面量。
    Map { entries: Vec<MapEntry>, span: Span },
    /// 对 enum 值进行穷尽匹配的表达式。
    Match {
        target: Box<Expression>,
        arms: Vec<MatchArm>,
        span: Span,
    },
    /// 局部变量读取。
    Variable { name: String, span: Span },
    /// 平台或后续普通函数调用。
    Call {
        path: Vec<String>,
        arguments: Vec<Expression>,
        span: Span,
    },
    /// 整数加法。
    Add {
        left: Box<Expression>,
        right: Box<Expression>,
        span: Span,
    },
    /// 整数乘法。
    Multiply {
        left: Box<Expression>,
        right: Box<Expression>,
        span: Span,
    },
    /// 同类型基础值相等比较。
    Equal {
        left: Box<Expression>,
        right: Box<Expression>,
        span: Span,
    },
    /// 具名结构体字面量。
    StructLiteral {
        name: String,
        name_span: Span,
        fields: Vec<StructFieldValue>,
        span: Span,
    },
    /// 结构体字段读取。
    FieldAccess {
        target: Box<Expression>,
        field: String,
        field_span: Span,
        span: Span,
    },
}

impl Expression {
    /// 返回该表达式在源文件中的区间。
    pub const fn span(&self) -> Span {
        match self {
            Self::Integer { span, .. }
            | Self::Float { span, .. }
            | Self::Boolean { span, .. }
            | Self::String { span, .. }
            | Self::List { span, .. }
            | Self::Map { span, .. }
            | Self::Match { span, .. }
            | Self::Variable { span, .. }
            | Self::Call { span, .. }
            | Self::Add { span, .. }
            | Self::Multiply { span, .. }
            | Self::Equal { span, .. } => *span,
            Self::StructLiteral { span, .. } | Self::FieldAccess { span, .. } => *span,
        }
    }
}

/// HIR map 字面量中的一个字符串键值对。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MapEntry {
    /// 不含双引号的键文本。
    pub key: String,
    /// 键字符串字面量在源文件中的位置。
    pub key_span: Span,
    /// 与键关联的值表达式。
    pub value: Expression,
}

/// HIR match 分支中对 enum 变体的模式。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnumPattern {
    /// 枚举名称。
    pub enum_name: String,
    /// 枚举名称在源文件中的位置。
    pub enum_name_span: Span,
    /// 变体名称。
    pub variant: String,
    /// 变体名称在源文件中的位置。
    pub variant_span: Span,
    /// 有载荷变体在分支内使用的可选局部绑定。
    pub binding: Option<(String, Span)>,
}

/// HIR match 表达式的一个分支。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MatchArm {
    /// 分支匹配的 enum 变体模式。
    pub pattern: EnumPattern,
    /// 该分支被选中时求值的表达式。
    pub value: Expression,
}

/// 结构体字面量中的一个具名字段赋值。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StructFieldValue {
    /// 字段名称。
    pub name: String,
    /// 字段名称的位置。
    pub name_span: Span,
    /// 字段值表达式。
    pub value: Expression,
}

/// 字符串字面量的组成部分。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StringPart {
    /// 不需要运行时求值的普通文本。
    Text(String),
    /// `{name}` 形式的局部变量插值。
    Variable { name: String, span: Span },
}

/// 将 parser 产生的语法树转换为后续阶段稳定消费的 HIR。
pub fn lower(program: SyntaxProgram) -> Result<Program, LowerError> {
    Ok(Program {
        module: program.module.map(|path| path.segments),
        imports: program
            .imports
            .into_iter()
            .map(|import| import.path.segments)
            .collect(),
        newtypes: program
            .newtypes
            .into_iter()
            .map(lower_newtype)
            .collect::<Result<Vec<_>, _>>()?,
        structs: program
            .structs
            .into_iter()
            .map(lower_struct)
            .collect::<Result<Vec<_>, _>>()?,
        enums: program
            .enums
            .into_iter()
            .map(lower_enum)
            .collect::<Result<Vec<_>, _>>()?,
        functions: program
            .functions
            .into_iter()
            .map(lower_function)
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn lower_newtype(newtype: yan_syntax::Newtype) -> Result<Newtype, LowerError> {
    Ok(Newtype {
        name: newtype.name,
        name_span: newtype.name_span,
        underlying: lower_type(newtype.underlying)?,
    })
}

fn lower_struct(structure: yan_syntax::Struct) -> Result<Struct, LowerError> {
    Ok(Struct {
        name: structure.name,
        name_span: structure.name_span,
        fields: structure
            .fields
            .into_iter()
            .map(lower_declared_field)
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn lower_enum(enumeration: SyntaxEnum) -> Result<Enum, LowerError> {
    Ok(Enum {
        name: enumeration.name,
        name_span: enumeration.name_span,
        variants: enumeration
            .variants
            .into_iter()
            .map(|variant| {
                Ok(EnumVariant {
                    name: variant.name,
                    name_span: variant.name_span,
                    payload: variant
                        .payload
                        .map(|payload| {
                            Ok(EnumPayload {
                                name: payload.name,
                                name_span: payload.name_span,
                                ty: lower_type(payload.ty)?,
                            })
                        })
                        .transpose()?,
                })
            })
            .collect::<Result<Vec<_>, LowerError>>()?,
    })
}

fn lower_declared_field(field: SyntaxField) -> Result<Field, LowerError> {
    let Some(ty) = field.ty else {
        return Err(LowerError {
            span: field.name_span,
            message: "struct field is missing a type".to_owned(),
        });
    };
    Ok(Field {
        name: field.name,
        name_span: field.name_span,
        ty: lower_type(ty)?,
        default: field.default.map(lower_expression).transpose()?,
    })
}

/// lowering 中发现的当前阶段不支持的语法类型。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LowerError {
    /// 不支持写法的位置。
    pub span: Span,
    /// 面向用户的错误原因。
    pub message: String,
}

fn lower_function(function: yan_syntax::Function) -> Result<Function, LowerError> {
    Ok(Function {
        name: function.name,
        name_span: function.name_span,
        parameters: function
            .parameters
            .into_iter()
            .map(|parameter| {
                Ok(Parameter {
                    name: parameter.name,
                    name_span: parameter.name_span,
                    ty: lower_type(parameter.ty)?,
                })
            })
            .collect::<Result<Vec<_>, LowerError>>()?,
        return_type: lower_type(function.return_type)?,
        statements: function
            .statements
            .into_iter()
            .map(lower_statement)
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn lower_type(ty: TypeSyntax) -> Result<Type, LowerError> {
    let unsupported = || LowerError {
        span: ty.span,
        message: format!("M3 does not support type `{}`", ty.name),
    };
    match (ty.name.as_str(), ty.arguments.as_slice()) {
        ("int", []) => Ok(Type::Int),
        ("float", []) => Ok(Type::Float),
        ("bytes", []) => Ok(Type::Bytes),
        ("bool", []) => Ok(Type::Bool),
        ("string", []) => Ok(Type::String),
        ("unit", []) => Ok(Type::Unit),
        ("List", [element]) => Ok(Type::List(Box::new(lower_type(element.clone())?))),
        ("Map", [key, value]) if key.name == "string" && key.arguments.is_empty() => {
            Ok(Type::Map(Box::new(lower_type(value.clone())?)))
        }
        (name, []) => Ok(Type::Named(name.to_owned())),
        _ => Err(unsupported()),
    }
}

fn lower_statement(statement: SyntaxStatement) -> Result<Statement, LowerError> {
    match statement {
        SyntaxStatement::Let {
            mutable,
            name,
            name_span,
            annotation,
            value,
        } => Ok(Statement::Let {
            mutable,
            name,
            name_span,
            annotation: annotation.map(lower_type).transpose()?,
            value: lower_expression(value)?,
        }),
        SyntaxStatement::Assign {
            name,
            name_span,
            value,
        } => Ok(Statement::Assign {
            name,
            name_span,
            value: lower_expression(value)?,
        }),
        SyntaxStatement::Expression(expression) => {
            Ok(Statement::Expression(lower_expression(expression)?))
        }
    }
}

fn lower_expression(expression: SyntaxExpression) -> Result<Expression, LowerError> {
    Ok(match expression {
        SyntaxExpression::Integer { value, span } => Expression::Integer { value, span },
        SyntaxExpression::Float { value, span } => Expression::Float { value, span },
        SyntaxExpression::Boolean { value, span } => Expression::Boolean { value, span },
        SyntaxExpression::String { value, span } => Expression::String {
            parts: lower_string_parts(&value, span)?,
            span,
        },
        SyntaxExpression::List { values, span } => Expression::List {
            values: values
                .into_iter()
                .map(lower_expression)
                .collect::<Result<Vec<_>, _>>()?,
            span,
        },
        SyntaxExpression::Map { entries, span } => Expression::Map {
            entries: entries
                .into_iter()
                .map(lower_map_entry)
                .collect::<Result<Vec<_>, _>>()?,
            span,
        },
        SyntaxExpression::Match { target, arms, span } => Expression::Match {
            target: Box::new(lower_expression(*target)?),
            arms: arms
                .into_iter()
                .map(lower_match_arm)
                .collect::<Result<Vec<_>, _>>()?,
            span,
        },
        SyntaxExpression::Variable { name, span } => Expression::Variable { name, span },
        SyntaxExpression::Call {
            path,
            arguments,
            span,
        } => Expression::Call {
            path,
            arguments: arguments
                .into_iter()
                .map(lower_expression)
                .collect::<Result<Vec<_>, _>>()?,
            span,
        },
        SyntaxExpression::Add { left, right, span } => Expression::Add {
            left: Box::new(lower_expression(*left)?),
            right: Box::new(lower_expression(*right)?),
            span,
        },
        SyntaxExpression::Multiply { left, right, span } => Expression::Multiply {
            left: Box::new(lower_expression(*left)?),
            right: Box::new(lower_expression(*right)?),
            span,
        },
        SyntaxExpression::Equal { left, right, span } => Expression::Equal {
            left: Box::new(lower_expression(*left)?),
            right: Box::new(lower_expression(*right)?),
            span,
        },
        SyntaxExpression::StructLiteral {
            name,
            name_span,
            fields,
            span,
        } => Expression::StructLiteral {
            name,
            name_span,
            fields: fields
                .into_iter()
                .map(lower_struct_field_value)
                .collect::<Result<Vec<_>, _>>()?,
            span,
        },
        SyntaxExpression::FieldAccess {
            target,
            field,
            field_span,
            span,
        } => Expression::FieldAccess {
            target: Box::new(lower_expression(*target)?),
            field,
            field_span,
            span,
        },
    })
}

fn lower_map_entry(entry: SyntaxMapEntry) -> Result<MapEntry, LowerError> {
    Ok(MapEntry {
        key: entry.key,
        key_span: entry.key_span,
        value: lower_expression(entry.value)?,
    })
}

fn lower_match_arm(arm: SyntaxMatchArm) -> Result<MatchArm, LowerError> {
    Ok(MatchArm {
        pattern: EnumPattern {
            enum_name: arm.pattern.enum_name,
            enum_name_span: arm.pattern.enum_name_span,
            variant: arm.pattern.variant,
            variant_span: arm.pattern.variant_span,
            binding: arm.pattern.binding,
        },
        value: lower_expression(arm.value)?,
    })
}

fn lower_struct_field_value(field: SyntaxField) -> Result<StructFieldValue, LowerError> {
    let Some(value) = field.value else {
        return Err(LowerError {
            span: field.name_span,
            message: "struct literal field is missing a value".to_owned(),
        });
    };
    Ok(StructFieldValue {
        name: field.name,
        name_span: field.name_span,
        value: lower_expression(value)?,
    })
}

fn lower_string_parts(value: &str, span: Span) -> Result<Vec<StringPart>, LowerError> {
    let mut parts = Vec::new();
    let mut text_start = 0;
    let bytes = value.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            b'{' => {
                if text_start < index {
                    parts.push(StringPart::Text(value[text_start..index].to_owned()));
                }
                let name_start = index + 1;
                let Some(relative_end) = value[name_start..].find('}') else {
                    return Err(string_error(
                        span,
                        index,
                        value.len(),
                        "string interpolation is missing `}`",
                    ));
                };
                let name_end = name_start + relative_end;
                let name = &value[name_start..name_end];
                if !is_identifier(name) {
                    return Err(string_error(
                        span,
                        name_start,
                        name_end,
                        "string interpolation must use `{identifier}`",
                    ));
                }
                parts.push(StringPart::Variable {
                    name: name.to_owned(),
                    // span 的起点包含字符串开头的双引号，因此插值名称再偏移一个字节。
                    span: Span::new(span.start + name_start + 1, span.start + name_end + 1),
                });
                index = name_end + 1;
                text_start = index;
            }
            b'}' => {
                return Err(string_error(
                    span,
                    index,
                    index + 1,
                    "string interpolation has an unmatched `}`",
                ));
            }
            _ => index += 1,
        }
    }

    if text_start < value.len() || parts.is_empty() {
        parts.push(StringPart::Text(value[text_start..].to_owned()));
    }
    Ok(parts)
}

fn string_error(span: Span, start: usize, end: usize, message: &str) -> LowerError {
    LowerError {
        // span 的首字节是开引号，字面量内容从下一个字节开始。
        span: Span::new(span.start + start + 1, span.start + end + 1),
        message: message.to_owned(),
    }
}

fn is_identifier(value: &str) -> bool {
    let bytes = value.as_bytes();
    matches!(bytes.first(), Some(b'a'..=b'z' | b'A'..=b'Z' | b'_'))
        && bytes[1..]
            .iter()
            .all(|byte| matches!(byte, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_'))
}
