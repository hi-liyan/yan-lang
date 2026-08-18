# Yan

> 一门面向个人开发者使用 AI 可以快速开发、快速交付的轻量编译型编程语言。

Yan 是一门通用编译型语言，而不是 Web DSL。它以小型语言核心、明确的副作用边界和可检查的项目契约，减少个人项目在多轮 AI 修改后的结构膨胀与维护成本。

当前编译器 `yanc` 由 Rust 实现。Rust 仅是启动阶段的实现后端：Yan 将拥有自己的语义、工具链和标准库，不向用户暴露 Rust 的借用、生命周期或 trait 细节。

## 当前阶段

当前处于 M1“编译器前端地基”阶段：建立 Rust workspace，并提供源文件位置模型、最小词法分析器和 `yanc check` 命令。完整目标见 [M1 目标](docs/milestones/m1-compiler-foundation.md)。

这不是可用于生产开发的语言版本。类型检查、解析、代码生成、包管理、标准库和平台库尚未实现。

## 快速开始

需要稳定版 Rust 工具链。

```powershell
cargo run -p yanc -- --help
cargo run -p yanc -- check examples/hello.yan
```

`check` 当前只执行词法分析，并验证源文件可以被读取；它尚不代表完整语法或类型检查。

## 仓库结构

```text
crates/
  yan-source/  源码文件、位置和 span
  yan-syntax/  token 与 lexer
  yan-hir/     稳定的 Yan 高层中间表示边界
  yanc/        编译器 CLI
docs/
  yan-language-design.md       语言设计基线
  milestones/                  可验收的阶段性目标
```

## 设计

语言定位、AI 可维护性约束、生态策略和长期自举路线见 [Yan 语言设计基线](docs/yan-language-design.md)。
