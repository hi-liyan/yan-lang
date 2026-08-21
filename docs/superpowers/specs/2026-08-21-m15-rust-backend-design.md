# M15 Rust Backend Design

日期：2026-08-21
状态：已确认设计，等待实现计划

## 决策

M15 通过 `yanc build <file.yan>` 真实调用 Cargo 并生成本地二进制。每次构建从当前应用模块的 `VerifiedProgram` 生成受控 Cargo 项目；编译器不接受用户 Cargo 配置。M15 支持全部 M2 至 M13 已有语义，不采用仅覆盖 bootstrap 子集的后端。

M16 的标准库将以 Yan 源文件随编译器发行，并由 `yanc` 自动与应用源一同编译链接。因此 M15 必须固定内部标准库目录和 `yan.std` 命名空间边界，但不实现、公开或允许导入标准库。

## 方案比较

选择受控 Cargo 构建目录，而不直接调用单文件 `rustc` 或只生成 Rust 源。Cargo 为固定 runtime crate、未来标准库与多模块生成提供唯一稳定的构建接口；单文件模式会在 M16 重新引入 Cargo 项目管理问题，只生成源码则不满足原生二进制交付目标。

## 组件边界

| 组件 | 输入 | 输出 | 约束 |
| --- | --- | --- | --- |
| `yan-rust-backend` | `VerifiedProgram` | Rust module text 与构建清单数据 | 不依赖 parser、HIR、Typed HIR、CLI 或文件系统 |
| `yan-runtime` | 生成 Rust 的固定调用 | Yan 运行时值和 intrinsic | 无用户可见 Rust API、无用户配置依赖 |
| `yanc` | 入口文件与 backend 产物 | 构建目录、Cargo 子进程、二进制路径、Yan 诊断 | 不实现 lowering 或运行时语义 |

后端不得接受未验证 MIR。运行时不得重新做 Yan 名称解析或类型检查。Yan 对 Rust 生成策略无可见语义依赖。

## 构建与发布

构建输入的稳定哈希决定 `target/yan/<entry-hash>/`。其下的 `cargo/` 为内部 Cargo 项目，`bin/` 为仅在 Cargo 成功后发布的最终二进制。失败时不得把不完整二进制作为成功产物报告。该目录被视为生成物，不能提交、编辑或通过 CLI 配置。

Cargo 项目位置与 Cargo 配置发现位置必须分离：生成清单仍位于 `target/yan/<entry-hash>/cargo/Cargo.toml`，但 `yanc` 从不位于当前用户 Profile 下的编译器专属目录启动 Cargo，并以绝对 `--manifest-path` 指向该清单。Windows 使用经过绝对路径校验与规范化的 `%PUBLIC%\\yanc`，其他平台使用系统临时目录；隔离目录同时承载受控 `CARGO_HOME`。在创建目录前及以规范化 cwd 启动 Cargo 前，`yanc` 都扫描 cwd 到盘符根的 `.cargo/config.toml` 与旧版 `config`，并检查 `CARGO_HOME` 根的同名配置；发现任一配置即拒绝构建，而不继承它。启动子进程前还移除用户 `CARGO_BUILD_*`、`CARGO_TARGET_<TRIPLE>_*`、Rust wrapper 与 rustflags 环境变量，同时保留 MSVC 所需系统工具链环境。独占目录创建可避免已有同名目录的配置碰撞，但该策略不构成宿主机级完全隔离，也不抵御同权限进程在检查后修改文件系统。

`yanc build` 的成功文本固定为 `<path>: build succeeded: <binary-path>`。前端和 lowering 诊断保留当前带行列的格式；Cargo 或链接失败统一转换为 `error: <entry-path>:1:1: backend build failed`，以隔离 Rust 与操作系统文本。

## 语义映射

M15 直接生成 M14 已验证 CFG 的 Rust 控制流。函数、局部、临时值、分支、match、循环、return 与 Result 传播均以 MIR ID 和控制流为基础，不得从名称或表面语法推断。

所有 M2 至 M13 语义必须与解释器保持输出一致。`console.println` 通过固定 runtime intrinsic 实现，是本阶段唯一运行时 I/O。新 runtime API、标准库 API 和平台能力均推迟。

## M16 衔接

M15 预留 `yan.std`，并使内部模块的来源、模块 ID、可见性、诊断和生成流程与应用模块一致。用户在 M15 导入该根命名空间必须收到稳定 Yan 诊断。M16 仅添加随编译器发行的 Yan 源模块和受控导入规则，不改变后端 ABI、构建目录或链接模型。

## 验证

每个 M2 至 M13 可执行 fixture 运行三次：MIR 解释器、`yanc build` 及生成的二进制。后二者的标准输出逐行与解释器完全一致。测试还覆盖跨模块调用、构建失败诊断、保留命名空间拒绝与内部标准库模块输入。

## 非目标

不增加语言语法、标准库实现、包管理、用户依赖、Cargo 配置、WASM、优化、SSA、交叉编译、增量构建、动态链接或任意 Rust 互操作。
