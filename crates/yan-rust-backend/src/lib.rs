//! Yan 已验证 MIR 的 Rust 后端边界。
//!
//! 本 crate 只能接收 `yan_mir::VerifiedProgram`，不能依赖 AST、HIR 或 Typed HIR。M15
//! Task 1 不生成 Rust 源码、不调用 Cargo，也不定义运行时值。

use yan_mir::VerifiedProgram;

/// Rust 后端无法完成生成时返回的稳定错误。
///
/// 该错误不携带 Rust、Cargo 或操作系统的内部文本；CLI 负责将其映射为 Yan 诊断。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendError {
    /// 当前阶段尚未支持将已验证 MIR 生成为受控 Rust 构建产物。
    UnsupportedProgram,
}

/// 从已验证 MIR 生成受控的 Rust 后端产物。
///
/// 入口仅接受 `VerifiedProgram`，从类型边界禁止后端重新解析前端表示。M15 Task 1 尚未
/// 实现源码生成，因此所有输入都返回 [`BackendError::UnsupportedProgram`]。
pub fn generate(_program: &VerifiedProgram) -> Result<(), BackendError> {
    Err(BackendError::UnsupportedProgram)
}
