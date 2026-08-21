//! 为各编译阶段提供统一的源文件文本与稳定位置模型。

use std::path::{Path, PathBuf};

/// 源文件在单次编译会话内的稳定标识。
///
/// 该标识只关联同一 [`SourceMap`] 中的不可变文件，不能序列化为跨会话缓存键。
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SourceId(pub u32);

/// 由源文件标识与字节区间构成的完整诊断位置。
///
/// 跨文件编译阶段必须传递本类型，避免把某个文件的 [`Span`] 错误映射到入口文件。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceLocation {
    /// 位置所属的编译会话源文件。
    pub source: SourceId,
    /// 该文件内的半开字节区间。
    pub span: Span,
}

impl SourceLocation {
    /// 使用文件标识和字节区间构造完整位置。
    pub const fn new(source: SourceId, span: Span) -> Self {
        Self { source, span }
    }
}

/// 单次编译会话参与文件的只读索引表。
///
/// 文件读取仍由 CLI 完成；本类型只为已读取文件分配稳定 ID 并提供诊断查找。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SourceMap {
    files: Vec<SourceFile>,
}

impl SourceMap {
    /// 创建空的编译会话源文件表。
    pub const fn new() -> Self {
        Self { files: Vec::new() }
    }

    /// 插入已读取源文件并返回其稳定会话 ID。
    pub fn insert(&mut self, file: SourceFile) -> SourceId {
        let id = SourceId(self.files.len() as u32);
        self.files.push(file);
        id
    }

    /// 返回指定会话源文件；未知 ID 返回 `None` 供调用方报告内部不变量错误。
    pub fn get(&self, id: SourceId) -> Option<&SourceFile> {
        self.files.get(id.0 as usize)
    }
}

/// 源文件中的半开字节区间 `[start, end)`。
///
/// 编译器内部统一以 UTF-8 字节偏移保存位置，避免 lexer、parser 和诊断层各自维护
/// 不一致的坐标体系。向用户展示行列号时必须通过 [`SourceFile::line_column`] 转换。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    /// 使用起始和结束字节偏移构造一个半开区间。
    ///
    /// 调用方必须保证两个偏移来自同一源文件且 `start <= end`。M1 保持该数据类型轻量，
    /// 更完整的 span 合法性检查将在诊断基础设施成熟后集中处理。
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// 不可变的源文件文本及其用于诊断展示的路径。
///
/// 该类型不负责文件 I/O。CLI 或包管理层负责读取文件后再创建 `SourceFile`，从而使
/// 编译前端能够在测试中直接使用内存中的文本，并保持 `yan-source` 不依赖环境状态。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceFile {
    path: PathBuf,
    text: String,
}

impl SourceFile {
    /// 以已读取的文本创建源文件模型。
    ///
    /// `path` 只用于诊断和展示，不在此处访问文件系统；`text` 必须保持为原始 UTF-8
    /// 源文本，以确保后续 span 的字节偏移不会失效。
    pub fn new(path: impl Into<PathBuf>, text: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            text: text.into(),
        }
    }

    /// 返回源文件在用户诊断中展示的路径。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 返回未经转换的 UTF-8 源代码文本。
    ///
    /// lexer 等前端阶段只读取该文本，不能修改它，否则此前生成的 span 将不再有效。
    pub fn text(&self) -> &str {
        &self.text
    }

    /// 将有效 UTF-8 字节偏移转换为从一开始计数的行号和列号。
    ///
    /// 若偏移超出文本范围，或落在多字节字符中间，则返回 `None`。这使诊断层不会把
    /// 编译器内部位置错误伪装成用户源代码的位置。
    pub fn line_column(&self, offset: usize) -> Option<(usize, usize)> {
        if offset > self.text.len() || !self.text.is_char_boundary(offset) {
            return None;
        }

        let prefix = &self.text[..offset];
        let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
        let column = prefix.rsplit('\n').next()?.chars().count() + 1;
        Some((line, column))
    }
}

#[cfg(test)]
mod tests {
    use super::{SourceFile, SourceLocation, SourceMap, Span};

    #[test]
    fn calculates_utf8_line_and_column() {
        let source = SourceFile::new("test.yan", "let name = \"Yan\"\n你好");

        assert_eq!(source.line_column(0), Some((1, 1)));
        assert_eq!(source.line_column(17), Some((2, 1)));
        assert_eq!(source.line_column(20), Some((2, 2)));
    }

    #[test]
    fn resolves_identical_spans_to_their_own_session_source_files() {
        let mut sources = SourceMap::new();
        let first = sources.insert(SourceFile::new("first.yan", "let value = 1"));
        let second = sources.insert(SourceFile::new("second.yan", "let value = 2"));

        let location = SourceLocation::new(second, Span::new(4, 9));

        assert_eq!(
            sources.get(location.source).map(SourceFile::path),
            Some(std::path::Path::new("second.yan"))
        );
        assert_ne!(first, second);
    }
}
