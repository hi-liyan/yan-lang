//! 与 parser 和执行后端解耦的 Yan 高层中间表示。

use yan_source::Span;
use yan_syntax::{
    Expression as SyntaxExpression, Statement as SyntaxStatement, SyntaxProgram, TypeSyntax,
};

/// 已降低为编译器语义阶段使用的程序。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Program {
    /// 源文件声明的模块路径；M2 允许省略。
    pub module: Option<Vec<String>>,
    /// 显式导入的模块路径。
    pub imports: Vec<Vec<String>>,
    /// 程序定义的函数。
    pub functions: Vec<Function>,
}

/// M2 支持的函数定义。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Function {
    /// 函数名称。
    pub name: String,
    /// 函数名称的位置。
    pub name_span: Span,
    /// 函数的声明返回类型。
    pub return_type: Type,
    /// 函数体语句。
    pub statements: Vec<Statement>,
}

/// M2 支持的语言类型。
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
    /// 字符串字面量。
    String { value: String, span: Span },
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
            | Self::Equal { span, .. } => *span,
        }
    }
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
        message: format!("M2 不支持类型 `{}`", ty.name),
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
        SyntaxExpression::String { value, span } => Expression::String { value, span },
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
        SyntaxExpression::Equal { left, right, span } => Expression::Equal {
            left: Box::new(lower_expression(*left)?),
            right: Box::new(lower_expression(*right)?),
            span,
        },
    })
}
