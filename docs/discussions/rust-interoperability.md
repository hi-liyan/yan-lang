# Rust 生态互操作探讨

日期：2026-08-18
状态：后续探讨，未纳入 M0、M1 或 M2 实现范围

## 背景

Yan 希望提供轻量编译型语言的开发体验，同时复用 Rust 在原生二进制、跨平台构建、异步运行时、HTTP、TLS、数据库驱动和基础库方面的成熟生态。

需要解决的问题不是“Yan 能否调用 Rust”，而是“如何在不把 Rust 的高认知负担泄漏给 Yan 开发者和 AI 的前提下复用 Rust crate”。

## 当前方向

Yan 应支持 Rust 生态互操作，但不允许 Yan 应用源码直接无约束引入任意 Rust crate。

推荐模型：

```text
Rust crate
  -> Yan adapter crate
  -> 稳定的 Yan 平台 API
  -> Yan 应用代码
```

例如，Yan 应用使用 `yan.http` 或 `yan.database.postgres`；这些平台包内部可以分别由 `axum`、`tokio`、`sqlx` 等 Rust crate 实现。Yan 的公开 API 不应直接暴露这些 crate 的 Rust 类型。

## 选择该模型的原因

允许如下形式会破坏 Yan 的核心价值：

```yan
import rust.axum
import rust.sqlx
```

任意 Rust crate 会把 trait、生命周期、借用、宏、复杂泛型、`Future` 和不一致的错误模型带入 Yan 项目。AI 虽然可以生成这类调用，却难以在多轮修改后保持稳定、统一和可维护。

adapter 包可以将生态复用限制在经过审查的边界内：Yan 保留自己的类型、错误、并发与包管理模型；Rust 负责平台实现细节。

## 边界约束草案

Yan 与 Rust adapter 的跨语言边界只应支持：

- `bool`、`int`、`float`、`string`、`bytes`。
- `List<T>`、`Map<string, T>`。
- 已显式映射的 Yan `struct` 与 `enum`。
- `Option<T>` 与 `Result<T, E>`。
- 明确标记为不透明资源的句柄，例如 `db.Connection`。

边界不得直接暴露：

- Rust 引用、生命周期或裸指针。
- 任意 trait object、泛型类型或宏展开结果。
- 未包装的 Rust `Future`、线程、锁或共享可变状态。
- 可从 Yan 调用路径传播的 panic。
- `unsafe` 能力。

Rust adapter 必须负责：将 Rust 错误映射为 Yan `Result`、将异步实现包装为 Yan 的结构化并发模型、以及将资源包装为受控的不透明句柄。

## 尚未决定的问题

1. Yan adapter 的声明形式、代码生成方式和 ABI。
2. `yan.project` 如何声明、锁定和审计底层 Cargo 依赖。
3. Yan `struct`/`enum` 与 Rust 类型的序列化及版本兼容规则。
4. async、取消和资源释放跨边界的精确语义。
5. Rust panic 的隔离和诊断策略。
6. 第三方 adapter 的发布、审核和兼容性策略。

## 进入实现前的门槛

在实现互操作层前，必须先完成 M0 语言示例评审，并分别补充：Yan 包模型、类型边界、错误模型、async 模型和 adapter 使用体验的示例。任何实现都不得要求 Yan 应用开发者理解 Rust 生命周期或 trait 约束。
