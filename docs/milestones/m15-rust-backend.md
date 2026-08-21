# M15：最小 Rust 原生后端

状态：已完成（2026-08-21）
范围：新增 `yanc build <file.yan>`，将已验证 MIR 编译为受控 Rust Cargo 项目并产出本地可执行二进制；为 M16 的 Yan 源标准库固定内部模块和运行时边界。

## 目标

M15 将 M14 的 `VerifiedProgram` 变为可交付的本地二进制。`yanc build` 必须真实生成 Rust 源、调用 Cargo、在构建成功后输出可执行文件路径。Rust 仍是实现后端，不成为 Yan 语言语义或用户 API。

M15 还必须为 M16 建立最小前提：编译器随附的 Yan 源模块能够与应用模块处于同一编译单元，并可由同一条前端和后端管线处理。M15 不实现或公开任何标准库 API。

## 编译管线

```text
应用 Yan 模块
  -> AST / resolved HIR / Typed HIR / verified MIR
  -> yan-rust-backend
  -> target/yan/<entry-hash>/cargo
       -> 固定 Cargo.toml、生成的 Rust 源、yan-runtime
  -> cargo build
  -> target/yan/<entry-hash>/bin/<entry-name>[.exe]
```

- `yan-rust-backend` 只消费 `VerifiedProgram`，不得读取 AST、HIR 或 Typed HIR，也不得重新解析名称或类型。
- `yan-runtime` 是由编译器控制的固定 Rust crate，只承载 Yan 值、既有语义和受控 intrinsic；不得成为用户可配置依赖。
- `yanc` 负责构建目录、Cargo 调用、二进制发布和诊断渲染，不得复制 lowering 或运行时语义。
- 构建目录和最终二进制为生成物，不得提交到版本控制，也不允许用户编辑或传入 Cargo 配置。

## 支持语义

M15 必须将 M2 至 M13 的全部已实现语义编译为二进制并保持与 MIR 解释器一致：基础值、局部绑定与 `mut`、函数调用、字符串插值、struct、enum、Option、Result、元组、`if`、`match`、`for`、`return` 与 Result `?`。

`console.println` 是本阶段唯一允许的受控 runtime intrinsic。它的二进制输出必须与 `yanc run` 完全一致。

## CLI 与诊断

`yanc --help` 增加：

```text
  yanc build <file.yan>
```

`yanc build` 成功时仅向标准输出写入：

```text
<path>: build succeeded: <binary-path>
```

并以状态码 `0` 退出。无效参数沿用帮助输出并以状态码 `2` 退出。前端、MIR 或 backend lowering 错误继续使用既有 `error: <path>:<line>:<column>: <message>` 格式。Cargo 或链接失败必须转换为以入口文件 `1:1` 定位的稳定英文诊断 `backend build failed`；不得透传本地化 Cargo/Rust 输出、Rust 类型名或调用栈。

## M16 前置边界

- 保留编译器内部根命名空间 `yan.std` 与随编译器发行的内部标准库目录约定。
- M15 不包含 `yan.std` Yan 源文件，不接受用户对 `yan.std` 的 import，也不增加标准库 API。
- 模块收集、解析、类型检查、MIR lowering 和 Rust 生成必须能以同一规则处理应用模块与未来的编译器内置 Yan 模块。
- M16 将标准库 Yan 源加入同一构建单元并开放受限 `yan.std` import；不以预编译标准库、包管理、动态链接或用户 Rust 依赖替代此模式。

## 验收标准

- `yanc build` 对每个 M2 至 M13 可执行 fixture 生成可运行的本地二进制。
- 每个 fixture 的二进制标准输出逐行等于 `yanc run` 的输出。
- 覆盖跨模块调用、`mut`、struct、enum/match、Option、Result、元组、if、for、return 与 `?` 的端到端二进制回归。
- 后端仅接受 `VerifiedProgram`；生成 Rust 不暴露为 Yan API，且不含用户可控 Cargo 依赖或配置。
- Cargo/link 失败、后端 lowering 失败与 Yan 运行时失败均通过稳定 Yan 诊断报告，包含受控路径和位置。
- 有回归证明用户无法在 M15 导入保留的 `yan.std`，同时内部模块输入可通过同一编译单元处理。
- `cargo fmt --all -- --check`、`cargo test --workspace`、`git diff --check` 均通过，且不提交生成物。

## 验收结果

- `yanc build` 对 M2 至 M13 的全部既有可执行 fixture 生成本地二进制；二进制标准输出逐行等于同一已验证 MIR 的解释器输出。回归还覆盖跨模块公开声明、`mut`、struct 字段、enum/Option/Result match、tuple 解构、if、for、早期 return 与 Result `?`。
- 前端诊断保留其原始文件与位置；实际 Cargo 失败稳定映射为入口文件 `1:1` 的 `backend build failed`，不转发 Cargo 或链接器文本。
- `yan.std` 仍是 M16 的保留内部根命名空间：M15 用户导入会被拒绝，编译器拥有的内部模块可使用同一模块图、类型检查、MIR 与 Rust 后端流程，但尚未提供标准库源文件或 API。
- 验证命令 `cargo fmt --all -- --check`、`cargo test --workspace` 与 `git diff --check` 已通过；生成的 `target/yan`、Cargo 项目和二进制未提交。

## 非目标

- 不新增 Yan 语法、类型、标准库 API、包模型、锁文件、formatter、capability、async、HTTP、数据库、WASM、优化、SSA 或代码生成以外的后端。
- 不支持用户自定义 Cargo profile、feature、依赖、build script、任意 Rust crate 或 FFI。
- 不实现 M16 标准库源文件、预编译标准库缓存、动态链接、交叉编译或增量构建策略。
