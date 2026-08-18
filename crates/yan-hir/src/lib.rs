//! 与 parser 和执行后端解耦的 Yan 高层中间表示。

use yan_source::Span;
use yan_syntax::{
    Expression as SyntaxExpression, Statement as SyntaxStatement, SyntaxProgram, TypeSyntax,
};

/// 已降低为编译器语义阶段使用的程序。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Program {
    /// 源文件声明的模块路径；M3 允许省略。
    pub module: Option<Vec<String>>,
    /// 显式导入的模块路径。
    pub imports: Vec<Vec<String>>,
    /// 程序定义的函数。
    pub functions: Vec<Function>,
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
    /// 布尔值。
    Bool,
    /// UTF-8 文本。
    String,
    /// 无有效返回值。
    Unit,
    /// 元素类型统一的有序列表。
    List(Box<Type>),
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
    /// 布尔字面量。
    Boolean { value: bool, span: Span },
    /// 由文本和变量插值片段构成的字符串字面量。
    String { parts: Vec<StringPart>, span: Span },
    /// 列表字面量。
    List { values: Vec<Expression>, span: Span },
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
}

impl Expression {
    /// 返回该表达式在源文件中的区间。
    pub const fn span(&self) -> Span {
        match self {
            Self::Integer { span, .. }
            | Self::Boolean { span, .. }
            | Self::String { span, .. }
            | Self::List { span, .. }
            | Self::Variable { span, .. }
            | Self::Call { span, .. }
            | Self::Add { span, .. }
            | Self::Multiply { span, .. }
            | Self::Equal { span, .. } => *span,
        }
    }
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
        functions: program
            .functions
            .into_iter()
            .map(lower_function)
            .collect::<Result<Vec<_>, _>>()?,
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
        ("bool", []) => Ok(Type::Bool),
        ("string", []) => Ok(Type::String),
        ("unit", []) => Ok(Type::Unit),
        ("list", [element]) => Ok(Type::List(Box::new(lower_type(element.clone())?))),
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
