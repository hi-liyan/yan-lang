//! 后续编译阶段使用的稳定高层中间表示边界。

/// 名称已解析的 Yan 程序。
///
/// M1 有意保持为空。后续 parser 和 resolver 将填充此类型，而前端通过该边界与后端
/// 解耦，避免把 Rust 代码生成细节泄漏到 Yan 的语言语义中。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Program;
