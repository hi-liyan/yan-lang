# M1：编译器前端地基

状态：已完成，等待 M0 语言示例评审后进入下一阶段
范围：`yanc` 的第一个可验证垂直切片

## 目标

建立可持续演进的 Rust 编译器工程，使 `yanc check <file.yan>` 能读取 Yan 源文件、进行词法分析，并以稳定格式报告第一个词法错误的位置。

M1 的价值不是支持应用开发，而是验证未来所有前端能力都可建立在同一套源码位置、诊断、token 和 CLI 约定上。

## 验收标准

- `cargo check --workspace` 成功。
- `cargo test --workspace` 成功。
- `yanc --help` 展示稳定的命令用法。
- `yanc check <file>` 可读取文件并完成最小词法分析。
- 非法字符或未闭合字符串的诊断包含文件路径、行、列和明确原因。
- token 与错误位置有单元测试。

## 本阶段包含

- Rust workspace 和严格的基础 lint。
- `yan-source`：源码、偏移和 span。
- `yan-syntax`：最小 token 集合与 lexer。
- `yan-hir`：不含语义的稳定数据边界占位。
- `yanc`：`check` 命令与文本诊断。

## 本阶段不包含

- 完整语法、AST parser、名称解析或类型检查。
- Yan 到 Rust 的代码生成。
- 包管理、formatter、架构分层检查和 capability 检查。
- async、HTTP、数据库、标准库或 Web 模板。

## 后续门槛

必须先完成 [M0：语言示例评审](m0-language-example-review.md)。用户确认语言示例后，才进入 M2：根据已确认的示例定义最小语法、构建 AST parser，并在语法层锁定不可变的模块规则。
