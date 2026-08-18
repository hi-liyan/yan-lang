//! Yan 源文件的最小词法层。

use std::fmt;

use yan_source::Span;

#[derive(Clone, Debug, Eq, PartialEq)]
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
    Comma,
    Colon,
    Equals,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Token {
    /// token 的语法类别。
    pub kind: TokenKind,
    /// token 在原始源文件中的半开字节区间。
    pub span: Span,
}

/// 词法分析发现的用户输入错误。
///
/// 该错误始终包含源文件位置，但不包含文件路径或行列号；诊断展示层负责结合
/// `yan-source` 计算位置，保证 lexer 可以脱离文件系统复用。
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

/// 将 Yan 源文本转换为带位置的 token 序列。
///
/// M1 只识别后续语法实现所需的最小 token 集。遇到无法识别的字节或未闭合字符串时，
/// 立即返回 `LexError`，不尝试猜测用户意图，以便上层提供确定且可测试的诊断。
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
            b'\"' => {
                index += 1;
                while index < bytes.len() && bytes[index] != b'\"' {
                    if bytes[index] == b'\n' {
                        return Err(error(
                            start,
                            index,
                            "string literals cannot contain a newline",
                        ));
                    }
                    index += 1;
                }
                if index == bytes.len() {
                    return Err(error(start, index, "unterminated string literal"));
                }
                index += 1;
                tokens.push(token(TokenKind::String, start, index));
            }
            b'(' => push_symbol(&mut tokens, TokenKind::LeftParen, &mut index),
            b')' => push_symbol(&mut tokens, TokenKind::RightParen, &mut index),
            b'{' => push_symbol(&mut tokens, TokenKind::LeftBrace, &mut index),
            b'}' => push_symbol(&mut tokens, TokenKind::RightBrace, &mut index),
            b',' => push_symbol(&mut tokens, TokenKind::Comma, &mut index),
            b':' => push_symbol(&mut tokens, TokenKind::Colon, &mut index),
            b'=' => push_symbol(&mut tokens, TokenKind::Equals, &mut index),
            _ => return Err(error(start, start + 1, "unexpected character")),
        }
    }

    Ok(tokens)
}

fn token(kind: TokenKind, start: usize, end: usize) -> Token {
    Token {
        kind,
        span: Span::new(start, end),
    }
}

fn error(start: usize, end: usize, message: &str) -> LexError {
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
    use super::{lex, TokenKind};

    #[test]
    fn lexes_minimal_declaration() {
        let tokens = lex("let value = 42").expect("source should lex");
        let kinds: Vec<_> = tokens.into_iter().map(|token| token.kind).collect();

        assert_eq!(
            kinds,
            vec![
                TokenKind::Identifier,
                TokenKind::Identifier,
                TokenKind::Equals,
                TokenKind::Integer,
            ]
        );
    }

    #[test]
    fn reports_unterminated_string() {
        let error = lex("\"yan").expect_err("source should fail");

        assert_eq!(error.span.start, 0);
        assert_eq!(error.message, "unterminated string literal");
    }
}
