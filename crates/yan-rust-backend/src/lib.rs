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

/// Rust 后端生成的受控 Cargo 项目源码布局。
///
/// 两个字段均由后端生成，`yanc` 负责将其写入隔离构建目录；调用者不得从 Yan 源码传入
/// Cargo 清单或 Rust 源码，以防用户配置突破后端与运行时边界。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedProgram {
    /// 固定依赖和包元数据组成的 Cargo 清单文本。
    pub manifest_toml: String,
    /// 仅由已验证 MIR 转换得到的 Rust 入口源码文本。
    pub main_rs: String,
}

/// 从已验证 MIR 生成受控的 Rust 后端产物。
///
/// 入口仅接受 `VerifiedProgram`，从类型边界禁止后端重新解析前端表示。M15 Task 1 尚未
/// 实现源码生成，因此所有输入都返回 [`BackendError::UnsupportedProgram`]。
pub fn generate(_program: &VerifiedProgram) -> Result<GeneratedProgram, BackendError> {
    Err(BackendError::UnsupportedProgram)
}

#[cfg(test)]
mod tests {
    use super::GeneratedProgram;

    #[test]
    fn generated_program_owns_the_controlled_cargo_source_layout() {
        let generated = GeneratedProgram {
            manifest_toml: "[package]".to_owned(),
            main_rs: "fn main() {}".to_owned(),
        };

        assert_eq!(generated.manifest_toml, "[package]");
        assert_eq!(generated.main_rs, "fn main() {}");
    }
}
