//! Yan 源文件的词法分析与 M2 最小语法树。
//!
//! 本 crate 只负责从文本构造语法结构，不读取文件、不检查类型，也不执行程序。

use std::fmt;

use yan_source::Span;

/// M2 词法分析可识别的 token 类别。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TokenKind {
    /// 标识符及尚未由 parser 区分的关键字。
    Identifier,
    /// 仅由十进制数字组成的整数字面量。
    Integer,
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
    Less,
    Greater,
    Arrow,
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

/// M2 支持的完整源文件语法树。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyntaxProgram {
    /// 可选的源码模块声明。
    pub module: Option<ModulePath>,
    /// 源文件显式引入的平台模块。
    pub imports: Vec<Import>,
    /// 源文件中的函数定义。
    pub functions: Vec<Function>,
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

/// M2 支持的无参数函数定义。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Function {
    /// 函数名称。
    pub name: String,
    /// 函数名称在源文件中的位置。
    pub name_span: Span,
    /// 显式声明的返回类型。
    pub return_type: TypeSyntax,
    /// 函数体中的顺序语句。
    pub statements: Vec<Statement>,
}

/// 源码中出现的类型写法。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypeSyntax {
    /// 类型名称，例如 `int` 或 `list`。
    pub name: String,
    /// 泛型类型参数；M2 只需 `list<string>`。
    pub arguments: Vec<TypeSyntax>,
    /// 整个类型写法的位置。
    pub span: Span,
}

/// M2 函数体可出现的语句。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Statement {
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

/// M2 支持的表达式。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Expression {
    /// 十进制整数。
    Integer { value: i64, span: Span },
    /// 布尔值。
    Boolean { value: bool, span: Span },
    /// 未转义的字符串内容。
    String { value: String, span: Span },
    /// 有序列表字面量。
    List { values: Vec<Expression>, span: Span },
    /// 局部变量引用。
    Variable { name: String, span: Span },
    /// 以模块式名称调用的函数，例如 `console.println(value)`。
    Call {
        path: Vec<String>,
        arguments: Vec<Expression>,
        span: Span,
    },
    /// M2 中仅支持整数相加。
    Add {
        left: Box<Expression>,
        right: Box<Expression>,
        span: Span,
    },
    /// M2 中同类型基础值的相等比较。
    Equal {
        left: Box<Expression>,
        right: Box<Expression>,
        span: Span,
    },
}

impl Expression {
    /// 返回表达式覆盖的源文件区间，供后续阶段产生精确诊断。
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

/// 将 Yan 源文本转换为带位置的 token 序列。
pub fn lex(source: &str) -> Result<Vec<Token>, LexError> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;

    while index < bytes.len() {
        let start = index;
        match bytes[index] {
            b' ' | b'\t' | b'\r' | b'\n' => index += 1,
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
                tokens.push(token(TokenKind::Integer, start, index));
            }
            b'"' => {
                index += 1;
                while index < bytes.len() && bytes[index] != b'"' {
                    if bytes[index] == b'\n' {
                        return Err(lex_error(start, index, "字符串字面量不能换行"));
                    }
                    index += 1;
                }
                if index == bytes.len() {
                    return Err(lex_error(start, index, "未闭合的字符串字面量"));
                }
                index += 1;
                tokens.push(token(TokenKind::String, start, index));
            }
            b'-' if bytes.get(index + 1) == Some(&b'>') => {
                index += 2;
                tokens.push(token(TokenKind::Arrow, start, index));
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
            b'<' => push_symbol(&mut tokens, TokenKind::Less, &mut index),
            b'>' => push_symbol(&mut tokens, TokenKind::Greater, &mut index),
            _ => return Err(lex_error(start, start + 1, "无法识别的字符")),
        }
    }

    Ok(tokens)
}

/// 根据 token 序列构造 M2 最小语法树。
pub fn parse(source: &str, tokens: &[Token]) -> Result<SyntaxProgram, ParseError> {
    Parser::new(source, tokens).parse_program()
}

struct Parser<'source, 'tokens> {
    source: &'source str,
    tokens: &'tokens [Token],
    position: usize,
}

impl<'source, 'tokens> Parser<'source, 'tokens> {
    fn new(source: &'source str, tokens: &'tokens [Token]) -> Self {
        Self {
            source,
            tokens,
            position: 0,
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

        let mut functions = Vec::new();
        while !self.at_end() {
            functions.push(self.parse_function()?);
        }

        Ok(SyntaxProgram {
            module,
            imports,
            functions,
        })
    }

    fn parse_module_path(&mut self) -> Result<ModulePath, ParseError> {
        let (first, start) = self.consume_identifier("模块路径")?;
        let mut segments = vec![first];
        let mut end = start.end;

        while self.consume_if(TokenKind::Dot).is_some() {
            let (segment, span) = self.consume_identifier("模块路径段")?;
            end = span.end;
            segments.push(segment);
        }

        Ok(ModulePath {
            segments,
            span: Span::new(start.start, end),
        })
    }

    fn parse_function(&mut self) -> Result<Function, ParseError> {
        self.consume_text("fn", "函数声明")?;
        let (name, name_span) = self.consume_identifier("函数名称")?;
        self.consume_kind(TokenKind::LeftParen, "`(`")?;
        self.consume_kind(TokenKind::RightParen, "`)`")?;
        self.consume_kind(TokenKind::Arrow, "`->`")?;
        let return_type = self.parse_type()?;
        self.consume_kind(TokenKind::LeftBrace, "`{`")?;

        let mut statements = Vec::new();
        while !self.at_end() && !self.at_kind(TokenKind::RightBrace) {
            statements.push(self.parse_statement()?);
        }
        self.consume_kind(TokenKind::RightBrace, "`}`")?;

        Ok(Function {
            name,
            name_span,
            return_type,
            statements,
        })
    }

    fn parse_type(&mut self) -> Result<TypeSyntax, ParseError> {
        let (name, start) = self.consume_identifier("类型名称")?;
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
        })
    }

    fn parse_statement(&mut self) -> Result<Statement, ParseError> {
        if self.peek_text() == Some("let") {
            self.advance();
            let mutable = if self.peek_text() == Some("mut") {
                self.advance();
                true
            } else {
                false
            };
            let (name, name_span) = self.consume_identifier("变量名称")?;
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
            let (name, name_span) = self.consume_identifier("变量名称")?;
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
        let mut expression = self.parse_primary()?;
        while self.consume_if(TokenKind::Plus).is_some() {
            let right = self.parse_primary()?;
            let span = Span::new(expression.span().start, right.span().end);
            expression = Expression::Add {
                left: Box::new(expression),
                right: Box::new(right),
                span,
            };
        }
        Ok(expression)
    }

    fn parse_primary(&mut self) -> Result<Expression, ParseError> {
        let token = self
            .current()
            .ok_or_else(|| self.error_at_end("表达式"))?
            .clone();
        match token.kind {
            TokenKind::Integer => {
                self.advance();
                let text = self.text_for(&token);
                let value = text.parse::<i64>().map_err(|_| ParseError {
                    span: token.span,
                    message: "整数超出 M2 支持范围".to_owned(),
                })?;
                Ok(Expression::Integer {
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
            TokenKind::Identifier => self.parse_identifier_expression(),
            _ => Err(ParseError {
                span: token.span,
                message: "此处需要表达式".to_owned(),
            }),
        }
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

    fn parse_identifier_expression(&mut self) -> Result<Expression, ParseError> {
        let (first, first_span) = self.consume_identifier("标识符")?;
        if first == "true" || first == "false" {
            return Ok(Expression::Boolean {
                value: first == "true",
                span: first_span,
            });
        }

        let mut path = vec![first];
        let mut end = first_span.end;
        while self.consume_if(TokenKind::Dot).is_some() {
            let (segment, span) = self.consume_identifier("调用路径段")?;
            end = span.end;
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

        Err(ParseError {
            span: Span::new(first_span.start, end),
            message: "M2 仅允许将带点路径用于函数调用".to_owned(),
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
                message: format!("此处需要 {expected}"),
            })
            .unwrap_or_else(|| self.error_at_end(expected))
    }

    fn error_at_end(&self, expected: &str) -> ParseError {
        ParseError {
            span: Span::new(self.source.len(), self.source.len()),
            message: format!("文件结尾处需要 {expected}"),
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
        assert_eq!(error.message, "未闭合的字符串字面量");
    }
}
