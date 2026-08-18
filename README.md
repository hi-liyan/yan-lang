# Yan

> 一门面向个人开发者使用 AI 可以快速开发、快速交付的轻量编译型编程语言。

Yan 是一门通用编译型语言，而不是 Web DSL。它以小型语言核心、明确的副作用边界和可检查的项目契约，减少个人项目在多轮 AI 修改后的结构膨胀与维护成本。

当前编译器 `yanc` 由 Rust 实现。Rust 仅是启动阶段的实现后端：Yan 将拥有自己的语义、工具链和标准库，不向用户暴露 Rust 的借用、生命周期或 trait 细节。

## 当前阶段

当前处于 M2“可执行值与绑定子集”阶段：以已确认的 `01_values.yan` 为唯一实现边界，建立 parser、HIR、类型检查和解释执行闭环。范围与验收标准见 [M2 目标](docs/milestones/m2-executable-values.md)。其余语言示例仍处于 [M0 评审](examples/language-design/README.md)。

这不是可用于生产开发的语言版本。M2 仅支持值与绑定子集；其他类型、控制流、代码生成、包管理、完整标准库和平台库尚未实现。

## 快速开始

需要稳定版 Rust 工具链。

```powershell
cargo run -p yanc -- --help
cargo run -p yanc -- check examples/language-design/01_values.yan
cargo run -p yanc -- run examples/language-design/01_values.yan
```

`check` 当前可检查 M2 的值与绑定子集。`run` 可解释执行已确认的 `01_values.yan`，输出 `Yan`、`1` 与 `true`。其他 `examples/language-design/` 提案仍不能使用这些命令检查。

## 仓库结构

```text
crates/
  yan-source/  源码文件、位置和 span
  yan-syntax/  token 与 lexer
  yan-hir/     稳定的 Yan 高层中间表示
  yan-typeck/  M2 类型检查
  yan-eval/    M2 解释执行器
  yanc/        编译器 CLI
docs/
  yan-language-design.md       语言设计基线
  milestones/                  可验收的阶段性目标
```

## 设计

语言定位、AI 可维护性约束、生态策略和长期自举路线见 [Yan 语言设计基线](docs/yan-language-design.md)。
