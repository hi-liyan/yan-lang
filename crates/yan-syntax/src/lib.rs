//! Yan 源文件的词法分析与 M3 最小语法树。
//!
//! 本 crate 只负责从文本构造语法结构，不读取文件、不检查类型，也不执行程序。

use std::fmt;

use yan_source::Span;

/// M3 词法分析可识别的 token 类别。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TokenKind {
    /// 标识符及尚未由 parser 区分的关键字。
    Identifier,
    /// 仅由十进制数字组成的整数字面量。
    Integer,
    /// 包含小数点的十进制浮点数字面量。
    Float,
    /// 由双引号包围且不跨行的字符串字面量。
    String,
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    Comma,
    Colon,
    Dot,
    Equals,
    EqualEqual,
    Plus,
    Asterisk,
    Less,
    Greater,
    Arrow,
    FatArrow,
    Question,
}

/// 一个带原始源文件位置的 token。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Token {
    /// token 的语法类别。
    pub kind: TokenKind,
    /// token 在原始源文件中的半开字节区间。
    pub span: Span,
}

/// 词法分析发现的用户输入错误。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LexError {
    /// 出错字符或未闭合结构对应的源文件区间。
    pub span: Span,
    /// 面向用户的稳定错误原因。
    pub message: String,
}

impl fmt::Display for LexError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for LexError {}

/// 语法分析发现的用户输入错误。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseError {
    /// 第一个无法按语法解释的 token，或文件结尾的位置。
    pub span: Span,
    /// 面向用户的稳定错误原因。
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ParseError {}

/// M3 支持的完整源文件语法树。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyntaxProgram {
    /// 可选的源码模块声明。
    pub module: Option<ModulePath>,
    /// 源文件显式引入的平台模块。
    pub imports: Vec<Import>,
    /// 源文件中的新类型声明。
    pub newtypes: Vec<Newtype>,
    /// 源文件中的结构体声明。
    pub structs: Vec<Struct>,
    /// 源文件中的枚举声明。
    pub enums: Vec<Enum>,
    /// 源文件中的函数定义。
    pub functions: Vec<Function>,
}

/// 真正的新类型声明，而非其底层类型的别名。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Newtype {
    /// 是否允许其他模块显式导入该声明。
    pub public: bool,
    /// 新类型名称。
    pub name: String,
    /// 新类型名称的位置。
    pub name_span: Span,
    /// 被包装的底层类型。
    pub underlying: TypeSyntax,
}

/// 具名结构体声明。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Struct {
    /// 是否允许其他模块显式导入该声明。
    pub public: bool,
    /// 结构体名称。
    pub name: String,
    /// 结构体名称的位置。
    pub name_span: Span,
    /// 按声明顺序排列的字段。
    pub fields: Vec<Field>,
}

/// 封闭枚举声明。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Enum {
    /// 是否允许其他模块显式导入该声明。
    pub public: bool,
    /// 枚举名称。
    pub name: String,
    /// 枚举名称在源文件中的位置。
    pub name_span: Span,
    /// 按声明顺序排列的变体。
    pub variants: Vec<EnumVariant>,
}

/// 枚举的一个零载荷或单载荷变体。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnumVariant {
    /// 变体名称。
    pub name: String,
    /// 变体名称在源文件中的位置。
    pub name_span: Span,
    /// 可选的单个具名载荷。
    pub payload: Option<Parameter>,
}

/// 结构体声明或字面量中的一个具名字段。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Field {
    /// 字段名称。
    pub name: String,
    /// 字段名称的位置。
    pub name_span: Span,
    /// 声明字段的类型；字面量字段没有类型标注。
    pub ty: Option<TypeSyntax>,
    /// 声明字段的可选默认值；字面量字段没有默认值。
    pub default: Option<Expression>,
    /// 字面量字段的可选显式值；声明字段没有显式值。
    pub value: Option<Expression>,
}

/// map 字面量中的一个字符串键值对。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MapEntry {
    /// 不含双引号的键文本。
    pub key: String,
    /// 键字符串字面量在源文件中的位置。
    pub key_span: Span,
    /// 与键关联的值表达式。
    pub value: Expression,
}

/// match 分支中对 enum 变体的模式。
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

/// match 表达式的一个分支。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MatchArm {
    /// 分支匹配的 enum 变体模式。
    pub pattern: EnumPattern,
    /// 该分支被选中时求值的表达式。
    pub value: Expression,
}

/// 用点分隔的模块路径。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModulePath {
    /// 路径中从左至右的各段名称。
    pub segments: Vec<String>,
    /// 整个路径在源文件中的位置。
    pub span: Span,
}

/// 单个 import 声明。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Import {
    /// 被引入的模块路径。
    pub path: ModulePath,
}

/// M3 支持的函数定义。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Function {
    /// 是否允许其他模块显式导入该声明。
    pub public: bool,
    /// 函数名称。
    pub name: String,
    /// 函数名称在源文件中的位置。
    pub name_span: Span,
    /// 按声明顺序排列的具名参数。
    pub parameters: Vec<Parameter>,
    /// 显式声明的返回类型。
    pub return_type: TypeSyntax,
    /// 函数体中的顺序语句。
    pub statements: Vec<Statement>,
}

/// 函数声明中的一个具名且带类型的参数。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Parameter {
    /// 参数名称。
    pub name: String,
    /// 参数名称在源文件中的位置。
    pub name_span: Span,
    /// 参数的显式类型标注。
    pub ty: TypeSyntax,
}

/// 源码中出现的类型写法。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypeSyntax {
    /// 类型名称，例如 `int` 或 `List`。
    pub name: String,
    /// 泛型类型参数；当前已实现 `List<T>` 与 `Map<string, T>` 形式。
    pub arguments: Vec<TypeSyntax>,
    /// 整个类型写法的位置。
    pub span: Span,
    /// 二元或三元元组的元素类型；非元组类型为空。
    pub tuple_elements: Vec<TypeSyntax>,
}

/// M3 函数体可出现的语句。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Statement {
    /// 将二元或三元元组按顺序解构为局部绑定。
    Destructure {
        names: Vec<(String, Span)>,
        value: Expression,
    },
    /// 声明局部绑定。
    Let {
        mutable: bool,
        name: String,
        name_span: Span,
        annotation: Option<TypeSyntax>,
        value: Expression,
    },
    /// 为已有可变绑定重新赋值。
    Assign {
        name: String,
        name_span: Span,
        value: Expression,
    },
    /// 仅为副作用求值的表达式。
    Expression(Expression),
}

/// M3 支持的表达式。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Expression {
    /// 十进制整数。
    Integer {
        value: i64,
        span: Span,
    },
    /// 十进制浮点数。
    Float {
        value: String,
        span: Span,
    },
    /// 布尔值。
    Boolean {
        value: bool,
        span: Span,
    },
    /// 未转义的字符串内容。
    String {
        value: String,
        span: Span,
    },
    /// 有序列表字面量。
    List {
        values: Vec<Expression>,
        span: Span,
    },
    /// 键为字符串的 map 字面量。
    Map {
        entries: Vec<MapEntry>,
        span: Span,
    },
    /// 二元或三元元组字面量。
    Tuple {
        values: Vec<Expression>,
        span: Span,
    },
    /// 对 enum 值进行穷尽匹配的表达式。
    Match {
        target: Box<Expression>,
        arms: Vec<MatchArm>,
        span: Span,
    },
    /// 根据 bool 条件从两个语句块中选择一个求值。
    If {
        condition: Box<Expression>,
        then_statements: Vec<Statement>,
        else_statements: Vec<Statement>,
        span: Span,
    },
    /// 遍历列表中的每个元素并执行语句块。
    For {
        name: String,
        name_span: Span,
        iterable: Box<Expression>,
        statements: Vec<Statement>,
        span: Span,
    },
    Return {
        value: Box<Expression>,
        span: Span,
    },
    Try {
        value: Box<Expression>,
        span: Span,
    },
    /// 局部变量引用。
    Variable {
        name: String,
        span: Span,
    },
    /// 以模块式名称调用的函数，例如 `console.println(value)`。
    Call {
        path: Vec<String>,
        arguments: Vec<Expression>,
        span: Span,
    },
    /// M3 中仅支持整数相加。
    Add {
        left: Box<Expression>,
        right: Box<Expression>,
        span: Span,
    },
    /// M3 中仅支持整数乘法。
    Multiply {
        left: Box<Expression>,
        right: Box<Expression>,
        span: Span,
    },
    /// M3 中同类型基础值的相等比较。
    Equal {
        left: Box<Expression>,
        right: Box<Expression>,
        span: Span,
    },
    /// 具名结构体字面量。
    StructLiteral {
        name: String,
        name_span: Span,
        fields: Vec<Field>,
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
    /// 返回表达式覆盖的源文件区间，供后续阶段产生精确诊断。
    pub const fn span(&self) -> Span {
        match self {
            Self::Integer { span, .. }
            | Self::Float { span, .. }
            | Self::Boolean { span, .. }
            | Self::String { span, .. }
            | Self::List { span, .. }
            | Self::Map { span, .. }
            | Self::Tuple { span, .. }
            | Self::Match { span, .. }
            | Self::If { span, .. }
            | Self::For { span, .. }
            | Self::Return { span, .. }
            | Self::Try { span, .. }
            | Self::Variable { span, .. }
            | Self::Call { span, .. }
            | Self::Add { span, .. }
            | Self::Multiply { span, .. }
            | Self::Equal { span, .. }
            | Self::StructLiteral { span, .. }
            | Self::FieldAccess { span, .. } => *span,
        }
    }
}

/// 将 Yan 源文本转换为带位置的 token 序列。
pub fn lex(source: &str) -> Result<Vec<Token>, LexError> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;

    while index < bytes.len() {
        let start = index;
        match bytes[index] {
            b' ' | b'\t' | b'\r' | b'\n' => index += 1,
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                index += 2;
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                index += 1;
                while matches!(
                    bytes.get(index),
                    Some(b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_')
                ) {
                    index += 1;
                }
                tokens.push(token(TokenKind::Identifier, start, index));
            }
            b'0'..=b'9' => {
                index += 1;
                while matches!(bytes.get(index), Some(b'0'..=b'9')) {
                    index += 1;
                }
                let kind = if bytes.get(index) == Some(&b'.')
                    && matches!(bytes.get(index + 1), Some(b'0'..=b'9'))
                {
                    index += 2;
                    while matches!(bytes.get(index), Some(b'0'..=b'9')) {
                        index += 1;
                    }
                    TokenKind::Float
                } else {
                    TokenKind::Integer
                };
                tokens.push(token(kind, start, index));
            }
            b'"' => {
                index += 1;
                while index < bytes.len() && bytes[index] != b'"' {
                    if bytes[index] == b'\n' {
                        return Err(lex_error(
                            start,
                            index,
                            "string literals cannot contain newlines",
                        ));
                    }
                    index += 1;
                }
                if index == bytes.len() {
                    return Err(lex_error(start, index, "unterminated string literal"));
                }
                index += 1;
                tokens.push(token(TokenKind::String, start, index));
            }
            b'-' if bytes.get(index + 1) == Some(&b'>') => {
                index += 2;
                tokens.push(token(TokenKind::Arrow, start, index));
            }
            b'=' if bytes.get(index + 1) == Some(&b'>') => {
                index += 2;
                tokens.push(token(TokenKind::FatArrow, start, index));
            }
            b'(' => push_symbol(&mut tokens, TokenKind::LeftParen, &mut index),
            b')' => push_symbol(&mut tokens, TokenKind::RightParen, &mut index),
            b'{' => push_symbol(&mut tokens, TokenKind::LeftBrace, &mut index),
            b'}' => push_symbol(&mut tokens, TokenKind::RightBrace, &mut index),
            b'[' => push_symbol(&mut tokens, TokenKind::LeftBracket, &mut index),
            b']' => push_symbol(&mut tokens, TokenKind::RightBracket, &mut index),
            b',' => push_symbol(&mut tokens, TokenKind::Comma, &mut index),
            b':' => push_symbol(&mut tokens, TokenKind::Colon, &mut index),
            b'.' => push_symbol(&mut tokens, TokenKind::Dot, &mut index),
            b'=' if bytes.get(index + 1) == Some(&b'=') => {
                index += 2;
                tokens.push(token(TokenKind::EqualEqual, start, index));
            }
            b'=' => push_symbol(&mut tokens, TokenKind::Equals, &mut index),
            b'+' => push_symbol(&mut tokens, TokenKind::Plus, &mut index),
            b'*' => push_symbol(&mut tokens, TokenKind::Asterisk, &mut index),
            b'<' => push_symbol(&mut tokens, TokenKind::Less, &mut index),
            b'>' => push_symbol(&mut tokens, TokenKind::Greater, &mut index),
            b'?' => push_symbol(&mut tokens, TokenKind::Question, &mut index),
            _ => return Err(lex_error(start, start + 1, "unrecognized character")),
        }
    }

    Ok(tokens)
}

/// 根据 token 序列构造 M3 最小语法树。
pub fn parse(source: &str, tokens: &[Token]) -> Result<SyntaxProgram, ParseError> {
    Parser::new(source, tokens).parse_program()
}

struct Parser<'source, 'tokens> {
    source: &'source str,
    tokens: &'tokens [Token],
    position: usize,
    allow_struct_literal: bool,
}

impl<'source, 'tokens> Parser<'source, 'tokens> {
    fn new(source: &'source str, tokens: &'tokens [Token]) -> Self {
        Self {
            source,
            tokens,
            position: 0,
            allow_struct_literal: true,
        }
    }

    fn parse_program(mut self) -> Result<SyntaxProgram, ParseError> {
        let module = if self.peek_text() == Some("module") {
            self.advance();
            Some(self.parse_module_path()?)
        } else {
            None
        };

        let mut imports = Vec::new();
        while self.peek_text() == Some("import") {
            self.advance();
            imports.push(Import {
                path: self.parse_module_path()?,
            });
        }

        let mut newtypes = Vec::new();
        let mut structs = Vec::new();
        let mut enums = Vec::new();
        let mut functions = Vec::new();
        while !self.at_end() {
            let public = if self.peek_text() == Some("public") {
                self.advance();
                true
            } else {
                false
            };
            match self.peek_text() {
                Some("type") => newtypes.push(self.parse_newtype(public)?),
                Some("struct") => structs.push(self.parse_struct(public)?),
                Some("enum") => enums.push(self.parse_enum(public)?),
                Some("fn") => functions.push(self.parse_function(public)?),
                _ => return Err(self.error_here("a declaration")),
            }
        }

        Ok(SyntaxProgram {
            module,
            imports,
            newtypes,
            structs,
            enums,
            functions,
        })
    }

    fn parse_newtype(&mut self, public: bool) -> Result<Newtype, ParseError> {
        self.consume_text("type", "a newtype declaration")?;
        let (name, name_span) = self.consume_identifier("newtype name")?;
        self.consume_kind(TokenKind::Equals, "`=`")?;
        let underlying = self.parse_type()?;
        Ok(Newtype {
            public,
            name,
            name_span,
            underlying,
        })
    }

    fn parse_struct(&mut self, public: bool) -> Result<Struct, ParseError> {
        self.consume_text("struct", "a struct declaration")?;
        let (name, name_span) = self.consume_identifier("struct name")?;
        self.consume_kind(TokenKind::LeftBrace, "`{`")?;
        let mut fields = Vec::new();
        while !self.at_end() && !self.at_kind(TokenKind::RightBrace) {
            fields.push(self.parse_struct_field()?);
        }
        self.consume_kind(TokenKind::RightBrace, "`}`")?;
        Ok(Struct {
            public,
            name,
            name_span,
            fields,
        })
    }

    fn parse_enum(&mut self, public: bool) -> Result<Enum, ParseError> {
        self.consume_text("enum", "an enum declaration")?;
        let (name, name_span) = self.consume_identifier("enum name")?;
        self.consume_kind(TokenKind::LeftBrace, "`{`")?;
        let mut variants = Vec::new();
        while !self.at_end() && !self.at_kind(TokenKind::RightBrace) {
            let (variant_name, variant_span) = self.consume_identifier("enum variant name")?;
            let payload = if self.consume_if(TokenKind::LeftParen).is_some() {
                let (payload_name, payload_span) = self.consume_identifier("enum payload name")?;
                self.consume_kind(TokenKind::Colon, "`:`")?;
                let ty = self.parse_type()?;
                self.consume_kind(TokenKind::RightParen, "`)`")?;
                Some(Parameter {
                    name: payload_name,
                    name_span: payload_span,
                    ty,
                })
            } else {
                None
            };
            variants.push(EnumVariant {
                name: variant_name,
                name_span: variant_span,
                payload,
            });
            self.consume_if(TokenKind::Comma);
        }
        self.consume_kind(TokenKind::RightBrace, "`}`")?;
        Ok(Enum {
            public,
            name,
            name_span,
            variants,
        })
    }

    fn parse_struct_field(&mut self) -> Result<Field, ParseError> {
        let (name, name_span) = self.consume_identifier("field name")?;
        self.consume_kind(TokenKind::Colon, "`:`")?;
        let ty = self.parse_type()?;
        let default = if self.consume_if(TokenKind::Equals).is_some() {
            Some(self.parse_expression()?)
        } else {
            None
        };
        Ok(Field {
            name,
            name_span,
            ty: Some(ty),
            default,
            value: None,
        })
    }

    fn parse_module_path(&mut self) -> Result<ModulePath, ParseError> {
        let (first, start) = self.consume_identifier("module path")?;
        let mut segments = vec![first];
        let mut end = start.end;

        while self.consume_if(TokenKind::Dot).is_some() {
            let (segment, span) = self.consume_identifier("module path segment")?;
            end = span.end;
            segments.push(segment);
        }

        Ok(ModulePath {
            segments,
            span: Span::new(start.start, end),
        })
    }

    fn parse_function(&mut self, public: bool) -> Result<Function, ParseError> {
        self.consume_text("fn", "function declaration")?;
        let (name, name_span) = self.consume_identifier("function name")?;
        self.consume_kind(TokenKind::LeftParen, "`(`")?;
        let parameters = self.parse_parameters()?;
        self.consume_kind(TokenKind::RightParen, "`)`")?;
        self.consume_kind(TokenKind::Arrow, "`->`")?;
        let return_type = self.parse_type()?;
        let statements = self.parse_statement_block()?;

        Ok(Function {
            public,
            name,
            name_span,
            parameters,
            return_type,
            statements,
        })
    }

    fn parse_statement_block(&mut self) -> Result<Vec<Statement>, ParseError> {
        self.consume_kind(TokenKind::LeftBrace, "`{`")?;
        let mut statements = Vec::new();
        while !self.at_end() && !self.at_kind(TokenKind::RightBrace) {
            statements.push(self.parse_statement()?);
        }
        self.consume_kind(TokenKind::RightBrace, "`}`")?;
        Ok(statements)
    }

    fn parse_parameters(&mut self) -> Result<Vec<Parameter>, ParseError> {
        let mut parameters = Vec::new();
        while !self.at_end() && !self.at_kind(TokenKind::RightParen) {
            let (name, name_span) = self.consume_identifier("parameter name")?;
            self.consume_kind(TokenKind::Colon, "`:`")?;
            let ty = self.parse_type()?;
            parameters.push(Parameter {
                name,
                name_span,
                ty,
            });
            if self.consume_if(TokenKind::Comma).is_none() {
                break;
            }
        }
        Ok(parameters)
    }

    fn parse_type(&mut self) -> Result<TypeSyntax, ParseError> {
        if self.at_kind(TokenKind::LeftParen) {
            let opening = self.consume_kind(TokenKind::LeftParen, "`(`")?;
            let elements = self.parse_tuple_items(|parser| parser.parse_type())?;
            let closing = self.consume_kind(TokenKind::RightParen, "`)`")?;
            return Ok(TypeSyntax {
                name: "()".to_owned(),
                arguments: Vec::new(),
                span: Span::new(opening.span.start, closing.span.end),
                tuple_elements: elements,
            });
        }
        let (name, start) = self.consume_identifier("type name")?;
        let mut arguments = Vec::new();
        let end = if self.consume_if(TokenKind::Less).is_some() {
            loop {
                let argument = self.parse_type()?;
                arguments.push(argument);
                if self.consume_if(TokenKind::Comma).is_none() {
                    break;
                }
            }
            let closing = self.consume_kind(TokenKind::Greater, "`>`")?;
            closing.span.end
        } else {
            start.end
        };

        Ok(TypeSyntax {
            name,
            arguments,
            span: Span::new(start.start, end),
            tuple_elements: Vec::new(),
        })
    }

    fn parse_statement(&mut self) -> Result<Statement, ParseError> {
        if self.peek_text() == Some("let") {
            self.advance();
            if self.at_kind(TokenKind::LeftParen) {
                self.consume_kind(TokenKind::LeftParen, "`(`")?;
                let names =
                    self.parse_tuple_items(|parser| parser.consume_identifier("destructure name"))?;
                self.consume_kind(TokenKind::RightParen, "`)`")?;
                self.consume_kind(TokenKind::Equals, "`=`")?;
                return Ok(Statement::Destructure {
                    names,
                    value: self.parse_expression()?,
                });
            }
            let mutable = if self.peek_text() == Some("mut") {
                self.advance();
                true
            } else {
                false
            };
            let (name, name_span) = self.consume_identifier("variable name")?;
            let annotation = if self.consume_if(TokenKind::Colon).is_some() {
                Some(self.parse_type()?)
            } else {
                None
            };
            self.consume_kind(TokenKind::Equals, "`=`")?;
            let value = self.parse_expression()?;
            return Ok(Statement::Let {
                mutable,
                name,
                name_span,
                annotation,
                value,
            });
        }

        if self.at_kind(TokenKind::Identifier) && self.peek_kind(1) == Some(TokenKind::Equals) {
            let (name, name_span) = self.consume_identifier("variable name")?;
            self.consume_kind(TokenKind::Equals, "`=`")?;
            let value = self.parse_expression()?;
            return Ok(Statement::Assign {
                name,
                name_span,
                value,
            });
        }

        Ok(Statement::Expression(self.parse_expression()?))
    }

    fn parse_expression(&mut self) -> Result<Expression, ParseError> {
        let mut expression = self.parse_addition()?;
        while self.consume_if(TokenKind::EqualEqual).is_some() {
            let right = self.parse_addition()?;
            let span = Span::new(expression.span().start, right.span().end);
            expression = Expression::Equal {
                left: Box::new(expression),
                right: Box::new(right),
                span,
            };
        }
        Ok(expression)
    }

    fn parse_addition(&mut self) -> Result<Expression, ParseError> {
        let mut expression = self.parse_multiplication()?;
        while self.consume_if(TokenKind::Plus).is_some() {
            let right = self.parse_multiplication()?;
            let span = Span::new(expression.span().start, right.span().end);
            expression = Expression::Add {
                left: Box::new(expression),
                right: Box::new(right),
                span,
            };
        }
        Ok(expression)
    }

    fn parse_multiplication(&mut self) -> Result<Expression, ParseError> {
        let mut expression = self.parse_postfix()?;
        while self.consume_if(TokenKind::Asterisk).is_some() {
            let right = self.parse_postfix()?;
            let span = Span::new(expression.span().start, right.span().end);
            expression = Expression::Multiply {
                left: Box::new(expression),
                right: Box::new(right),
                span,
            };
        }
        Ok(expression)
    }

    fn parse_postfix(&mut self) -> Result<Expression, ParseError> {
        let mut expression = self.parse_primary()?;
        while let Some(question) = self.consume_if(TokenKind::Question) {
            expression = Expression::Try {
                span: Span::new(expression.span().start, question.span.end),
                value: Box::new(expression),
            };
        }
        Ok(expression)
    }

    fn parse_primary(&mut self) -> Result<Expression, ParseError> {
        let token = self
            .current()
            .ok_or_else(|| self.error_at_end("an expression"))?
            .clone();
        match token.kind {
            TokenKind::Integer => {
                self.advance();
                let text = self.text_for(&token);
                let value = text.parse::<i64>().map_err(|_| ParseError {
                    span: token.span,
                    message: "integer literal is outside the M3 supported range".to_owned(),
                })?;
                Ok(Expression::Integer {
                    value,
                    span: token.span,
                })
            }
            TokenKind::Float => {
                self.advance();
                let value = self.text_for(&token).to_owned();
                Ok(Expression::Float {
                    value,
                    span: token.span,
                })
            }
            TokenKind::String => {
                self.advance();
                let text = self.text_for(&token);
                Ok(Expression::String {
                    value: text[1..text.len() - 1].to_owned(),
                    span: token.span,
                })
            }
            TokenKind::LeftBracket => self.parse_list(),
            TokenKind::LeftParen => self.parse_tuple(),
            TokenKind::LeftBrace => self.parse_map(),
            TokenKind::Identifier if self.peek_text() == Some("match") => self.parse_match(),
            TokenKind::Identifier if self.peek_text() == Some("if") => self.parse_if(),
            TokenKind::Identifier if self.peek_text() == Some("for") => self.parse_for(),
            TokenKind::Identifier if self.peek_text() == Some("return") => self.parse_return(),
            TokenKind::Identifier => self.parse_identifier_expression(),
            _ => Err(ParseError {
                span: token.span,
                message: "expected an expression".to_owned(),
            }),
        }
    }

    fn parse_if(&mut self) -> Result<Expression, ParseError> {
        let keyword = self.consume_text("if", "`if`")?;
        // 条件后的 `{` 是 if 分支起始，不能被变量后的结构体字面量解析抢先消费。
        self.allow_struct_literal = false;
        let condition = self.parse_expression()?;
        self.allow_struct_literal = true;
        let then_statements = self.parse_statement_block()?;
        self.consume_text("else", "`else`")?;
        let else_statements = self.parse_statement_block()?;
        let end = self.tokens[self.position - 1].span.end;
        Ok(Expression::If {
            condition: Box::new(condition),
            then_statements,
            else_statements,
            span: Span::new(keyword.span.start, end),
        })
    }

    fn parse_for(&mut self) -> Result<Expression, ParseError> {
        let keyword = self.consume_text("for", "`for`")?;
        let (name, name_span) = self.consume_identifier("loop variable name")?;
        self.consume_text("in", "`in`")?;
        // iterable 后的 `{` 是循环体起始，不能被结构体字面量解析消费。
        self.allow_struct_literal = false;
        let iterable = self.parse_expression()?;
        self.allow_struct_literal = true;
        let statements = self.parse_statement_block()?;
        let end = self.tokens[self.position - 1].span.end;
        Ok(Expression::For {
            name,
            name_span,
            iterable: Box::new(iterable),
            statements,
            span: Span::new(keyword.span.start, end),
        })
    }

    fn parse_tuple(&mut self) -> Result<Expression, ParseError> {
        let opening = self.consume_kind(TokenKind::LeftParen, "`(`")?;
        let values = self.parse_tuple_items(|parser| parser.parse_expression())?;
        let closing = self.consume_kind(TokenKind::RightParen, "`)`")?;
        Ok(Expression::Tuple {
            values,
            span: Span::new(opening.span.start, closing.span.end),
        })
    }

    fn parse_tuple_items<T>(
        &mut self,
        mut parse_item: impl FnMut(&mut Self) -> Result<T, ParseError>,
    ) -> Result<Vec<T>, ParseError> {
        let mut items = vec![parse_item(self)?];
        self.consume_kind(TokenKind::Comma, "`,`")?;
        items.push(parse_item(self)?);
        if self.consume_if(TokenKind::Comma).is_some() {
            items.push(parse_item(self)?);
        }
        if self.at_kind(TokenKind::Comma) {
            return Err(self.error_here("a two- or three-element tuple"));
        }
        Ok(items)
    }

    fn parse_return(&mut self) -> Result<Expression, ParseError> {
        let keyword = self.consume_text("return", "`return`")?;
        let value = self.parse_expression()?;
        Ok(Expression::Return {
            span: Span::new(keyword.span.start, value.span().end),
            value: Box::new(value),
        })
    }

    fn parse_list(&mut self) -> Result<Expression, ParseError> {
        let opening = self.consume_kind(TokenKind::LeftBracket, "`[`")?;
        let mut values = Vec::new();
        while !self.at_end() && !self.at_kind(TokenKind::RightBracket) {
            values.push(self.parse_expression()?);
            if self.consume_if(TokenKind::Comma).is_none() {
                break;
            }
        }
        let closing = self.consume_kind(TokenKind::RightBracket, "`]`")?;
        Ok(Expression::List {
            values,
            span: Span::new(opening.span.start, closing.span.end),
        })
    }

    fn parse_map(&mut self) -> Result<Expression, ParseError> {
        let opening = self.consume_kind(TokenKind::LeftBrace, "`{`")?;
        let mut entries = Vec::new();
        while !self.at_end() && !self.at_kind(TokenKind::RightBrace) {
            let key = self.consume_kind(TokenKind::String, "a string map key")?;
            let text = self.text_for(&key);
            let key_text = text[1..text.len() - 1].to_owned();
            self.consume_kind(TokenKind::Colon, "`:`")?;
            let value = self.parse_expression()?;
            entries.push(MapEntry {
                key: key_text,
                key_span: key.span,
                value,
            });
            self.consume_if(TokenKind::Comma);
        }
        let closing = self.consume_kind(TokenKind::RightBrace, "`}`")?;
        Ok(Expression::Map {
            entries,
            span: Span::new(opening.span.start, closing.span.end),
        })
    }

    fn parse_match(&mut self) -> Result<Expression, ParseError> {
        let opening = self.consume_text("match", "`match`")?;
        // M6 的 match 目标限定为局部 enum 值；直接读取标识符可避免紧随的 `{`
        // 被既有 parser 当成结构体字面量，从而把分支体错误地解析为字段。
        let (target_name, target_span) = self.consume_identifier("match target variable")?;
        let target = if self.consume_if(TokenKind::Dot).is_some() {
            let (method, method_span) = self.consume_identifier("match target method")?;
            self.consume_kind(TokenKind::LeftParen, "`(`")?;
            self.consume_kind(TokenKind::RightParen, "`)`")?;
            Expression::Call {
                path: vec![target_name, method],
                arguments: Vec::new(),
                span: Span::new(target_span.start, method_span.end + 2),
            }
        } else {
            Expression::Variable {
                name: target_name,
                span: target_span,
            }
        };
        self.consume_kind(TokenKind::LeftBrace, "`{`")?;
        let mut arms = Vec::new();
        while !self.at_end() && !self.at_kind(TokenKind::RightBrace) {
            let pattern = self.parse_enum_pattern()?;
            self.consume_kind(TokenKind::FatArrow, "`=>`")?;
            let value = self.parse_expression()?;
            arms.push(MatchArm { pattern, value });
            self.consume_if(TokenKind::Comma);
        }
        let closing = self.consume_kind(TokenKind::RightBrace, "`}`")?;
        Ok(Expression::Match {
            target: Box::new(target),
            arms,
            span: Span::new(opening.span.start, closing.span.end),
        })
    }

    fn parse_enum_pattern(&mut self) -> Result<EnumPattern, ParseError> {
        let (first, first_span) = self.consume_identifier("match variant name")?;
        let (enum_name, enum_name_span, variant, variant_span) =
            if self.consume_if(TokenKind::Dot).is_some() {
                let (variant, variant_span) = self.consume_identifier("enum variant name")?;
                (first, first_span, variant, variant_span)
            } else {
                // Option 的 Some/None 是内建模式，不带用户 enum 前缀。
                (String::new(), first_span, first, first_span)
            };
        let binding = if self.consume_if(TokenKind::LeftParen).is_some() {
            let (name, span) = self.consume_identifier("enum payload binding")?;
            self.consume_kind(TokenKind::RightParen, "`)`")?;
            Some((name, span))
        } else {
            None
        };
        Ok(EnumPattern {
            enum_name,
            enum_name_span,
            variant,
            variant_span,
            binding,
        })
    }

    fn parse_identifier_expression(&mut self) -> Result<Expression, ParseError> {
        let (first, first_span) = self.consume_identifier("an identifier")?;
        if first == "true" || first == "false" {
            return Ok(Expression::Boolean {
                value: first == "true",
                span: first_span,
            });
        }

        if self.allow_struct_literal && self.at_kind(TokenKind::LeftBrace) {
            return self.parse_struct_literal(first, first_span);
        }

        let mut path = vec![first];
        let mut end = first_span.end;
        let mut last_span = first_span;
        while self.consume_if(TokenKind::Dot).is_some() {
            let (segment, span) = self.consume_identifier("call path segment")?;
            end = span.end;
            last_span = span;
            path.push(segment);
        }

        if self.consume_if(TokenKind::LeftParen).is_some() {
            let mut arguments = Vec::new();
            while !self.at_end() && !self.at_kind(TokenKind::RightParen) {
                arguments.push(self.parse_expression()?);
                if self.consume_if(TokenKind::Comma).is_none() {
                    break;
                }
            }
            let closing = self.consume_kind(TokenKind::RightParen, "`)`")?;
            return Ok(Expression::Call {
                path,
                arguments,
                span: Span::new(first_span.start, closing.span.end),
            });
        }

        if path.len() == 1 {
            return Ok(Expression::Variable {
                name: path.remove(0),
                span: first_span,
            });
        }

        if path.len() == 2 {
            return Ok(Expression::FieldAccess {
                target: Box::new(Expression::Variable {
                    name: path.remove(0),
                    span: first_span,
                }),
                field: path.remove(0),
                field_span: last_span,
                span: Span::new(first_span.start, end),
            });
        }

        Err(ParseError {
            span: Span::new(first_span.start, end),
            message: "M3 only permits dotted paths for function calls".to_owned(),
        })
    }

    fn parse_struct_literal(
        &mut self,
        name: String,
        name_span: Span,
    ) -> Result<Expression, ParseError> {
        self.consume_kind(TokenKind::LeftBrace, "`{`")?;
        let mut fields = Vec::new();
        while !self.at_end() && !self.at_kind(TokenKind::RightBrace) {
            let (field_name, field_span) = self.consume_identifier("field name")?;
            self.consume_kind(TokenKind::Colon, "`:`")?;
            let value = self.parse_expression()?;
            fields.push(Field {
                name: field_name,
                name_span: field_span,
                ty: None,
                default: None,
                value: Some(value),
            });
        }
        let closing = self.consume_kind(TokenKind::RightBrace, "`}`")?;
        Ok(Expression::StructLiteral {
            name,
            name_span,
            fields,
            span: Span::new(name_span.start, closing.span.end),
        })
    }

    fn consume_identifier(&mut self, expected: &str) -> Result<(String, Span), ParseError> {
        let token = self.consume_kind(TokenKind::Identifier, expected)?;
        Ok((self.text_for(&token).to_owned(), token.span))
    }

    fn consume_text(&mut self, text: &str, expected: &str) -> Result<Token, ParseError> {
        if self.peek_text() == Some(text) {
            return Ok(self.advance().expect("已确认存在当前 token"));
        }
        Err(self.error_here(expected))
    }

    fn consume_kind(&mut self, kind: TokenKind, expected: &str) -> Result<Token, ParseError> {
        if self.at_kind(kind) {
            return Ok(self.advance().expect("已确认存在当前 token"));
        }
        Err(self.error_here(expected))
    }

    fn consume_if(&mut self, kind: TokenKind) -> Option<Token> {
        if self.at_kind(kind) {
            return self.advance();
        }
        None
    }

    fn at_kind(&self, kind: TokenKind) -> bool {
        self.current().is_some_and(|token| token.kind == kind)
    }

    fn peek_kind(&self, offset: usize) -> Option<TokenKind> {
        self.tokens
            .get(self.position + offset)
            .map(|token| token.kind)
    }

    fn peek_text(&self) -> Option<&str> {
        self.current().map(|token| self.text_for(token))
    }

    fn current(&self) -> Option<&Token> {
        self.tokens.get(self.position)
    }

    fn advance(&mut self) -> Option<Token> {
        let token = self.current()?.clone();
        self.position += 1;
        Some(token)
    }

    fn at_end(&self) -> bool {
        self.position == self.tokens.len()
    }

    fn text_for(&self, token: &Token) -> &str {
        &self.source[token.span.start..token.span.end]
    }

    fn error_here(&self, expected: &str) -> ParseError {
        self.current()
            .map(|token| ParseError {
                span: token.span,
                message: format!("expected {expected}"),
            })
            .unwrap_or_else(|| self.error_at_end(expected))
    }

    fn error_at_end(&self, expected: &str) -> ParseError {
        ParseError {
            span: Span::new(self.source.len(), self.source.len()),
            message: format!("unexpected end of file; expected {expected}"),
        }
    }
}

fn token(kind: TokenKind, start: usize, end: usize) -> Token {
    Token {
        kind,
        span: Span::new(start, end),
    }
}

fn lex_error(start: usize, end: usize, message: &str) -> LexError {
    LexError {
        span: Span::new(start, end),
        message: message.to_owned(),
    }
}

fn push_symbol(tokens: &mut Vec<Token>, kind: TokenKind, index: &mut usize) {
    let start = *index;
    *index += 1;
    tokens.push(token(kind, start, *index));
}

#[cfg(test)]
mod tests {
    use super::{lex, parse, Expression, Statement};

    #[test]
    fn parses_value_bindings_and_console_call() {
        let source =
            "fn main() -> unit { let mut count = 0 count = count + 1 console.println(count) }";
        let tokens = lex(source).expect("测试源码应能完成词法分析");
        let program = parse(source, &tokens).expect("测试源码应能完成语法分析");

        assert_eq!(program.functions.len(), 1);
        assert!(matches!(
            program.functions[0].statements[0],
            Statement::Let { mutable: true, .. }
        ));
        assert!(matches!(
            program.functions[0].statements[1],
            Statement::Assign { .. }
        ));
        assert!(matches!(
            program.functions[0].statements[2],
            Statement::Expression(Expression::Call { .. })
        ));
    }

    #[test]
    fn reports_unterminated_string() {
        let error = lex("\"yan").expect_err("未闭合字符串必须失败");

        assert_eq!(error.span.start, 0);
        assert_eq!(error.message, "unterminated string literal");
    }

    #[test]
    fn parses_string_keyed_map_literal() {
        let source =
            "fn main() -> unit { let ports: Map<string, int> = { \"http\": 80 \"https\": 443 } }";
        let tokens = lex(source).expect("测试源码应能完成词法分析");
        let program = parse(source, &tokens).expect("测试源码应能完成语法分析");

        assert!(matches!(
            &program.functions[0].statements[0],
            Statement::Let {
                value: Expression::Map { entries, .. },
                ..
            } if entries.len() == 2
        ));
    }

    #[test]
    fn parses_enum_declaration_and_match_expression() {
        let source = "enum State { Ready Failed(reason: string) } fn label(state: State) -> string { match state { State.Ready => \"ready\" State.Failed(reason) => \"failed: {reason}\" } } fn main() -> unit { }";
        let tokens = lex(source).expect("测试源码应能完成词法分析");
        let program = parse(source, &tokens).expect("测试源码应能完成语法分析");

        assert_eq!(program.enums[0].variants.len(), 2);
        assert!(matches!(
            &program.functions[0].statements[0],
            Statement::Expression(Expression::Match { arms, .. }) if arms.len() == 2
        ));
    }

    #[test]
    fn parses_if_and_for_statement_blocks() {
        let source = "fn main() -> unit { let ready = true if ready == true { console.println(\"ready\") } else { console.println(\"pending\") } for target in [\"cli\", \"web\"] { console.println(target) } }";
        let tokens = lex(source).expect("测试源码应能完成词法分析");
        let program = parse(source, &tokens).expect("测试源码应能完成语法分析");

        assert!(matches!(
            &program.functions[0].statements[1],
            Statement::Expression(Expression::If {
                then_statements,
                else_statements,
                ..
            }) if then_statements.len() == 1 && else_statements.len() == 1
        ));
        assert!(matches!(
            &program.functions[0].statements[2],
            Statement::Expression(Expression::For { statements, .. }) if statements.len() == 1
        ));
    }

    #[test]
    fn parses_public_top_level_declaration() {
        let source = "public fn greeting() -> string { \"hello\" } fn main() -> unit { }";
        let tokens = lex(source).expect("测试源码应能完成词法分析");
        let program = parse(source, &tokens).expect("测试源码应能完成语法分析");

        assert!(program.functions[0].public);
        assert!(!program.functions[1].public);
    }

    #[test]
    fn parses_unqualified_option_match_patterns() {
        let source = "fn label(name: Option<string>) -> string { match name { Some(value) => value None => \"anonymous\" } } fn main() -> unit { }";
        let tokens = lex(source).expect("测试源码应能完成词法分析");
        let program = parse(source, &tokens).expect("测试源码应能完成语法分析");

        assert!(matches!(
            &program.functions[0].statements[0],
            Statement::Expression(Expression::Match { .. })
        ));
        let Statement::Expression(Expression::Match { arms, .. }) =
            &program.functions[0].statements[0]
        else {
            return;
        };
        assert_eq!(arms[0].pattern.enum_name, "");
        assert_eq!(arms[0].pattern.variant, "Some");
        assert_eq!(arms[1].pattern.variant, "None");
    }
}
