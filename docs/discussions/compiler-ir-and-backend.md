# 编译中间表示与多后端设计

日期：2026-08-19
状态：已确认设计，M14 仅实现前端中间表示；Rust 代码生成留待后续里程碑

## 目标

为 Yan 建立可验证、可演进且不绑定 Rust 的编译边界。Rust 是第一个实现后端，而不是 Yan 的语义来源；未来增加 WASM 或其他目标时，不得复制 parser、类型检查或语言语义。

## 管线

```text
Yan Source
  -> Lexer / Parser
  -> AST
  -> 名称与模块解析
  -> HIR
  -> 类型检查
  -> Typed HIR
  -> MIR
  -> 目标无关优化、推断与 lowering
  -> Backend
       -> Rust source + yan runtime -> Cargo/rustc -> native executable
       -> WASM
       -> future targets
```

`yanc` 负责文件读取、编译流程编排、构建目录管理和诊断展示。语法、名称解析、类型规则、MIR 构造和后端代码生成分别位于对应前端或后端 crate，不得复制到 CLI。

## 各层职责

| 层 | 输入 | 输出 | 必须保证 | 不得承担 |
| --- | --- | --- | --- | --- |
| AST | token 与源文本 | 语法树 | 保留语法结构和 span | 名称、类型或执行语义决定 |
| 解析与 lowering | AST | HIR | 每个引用均指向 `DefId`、`LocalId`、字段或 variant ID；语法糖已消除 | Rust/WASM 类型、构建或执行 |
| 类型检查 | HIR | Typed HIR | 每个值表达式有确定 Yan 类型；所有调用、构造、模式与赋值均已验证 | 目标平台类型、借用或布局 |
| MIR lowering | Typed HIR | MIR | 控制流显式，局部读写、调用、返回和 Result 传播可直接执行 | 源码表面结构、后端专有语义 |
| 优化与后端 lowering | MIR | 目标专用 IR | 保持 Yan 可观察语义 | 重新进行名称或类型推断 |
| Backend | 目标专用 IR | 构建产物 | 不把目标实现细节暴露为 Yan 语义 | 接受 AST、未类型化 HIR 或用户任意 Rust crate |

span 作为诊断来源持续随节点或独立 source map 保存；稳定 ID 不使用用户可见名称替代。名称可在诊断和调试输出中通过符号表还原。

## HIR

HIR 是跨后端的已解析程序表示。它至少定义：

- `ModuleId`、`DefId`、`LocalId`、`FieldId` 与 `VariantId`，均只在本次编译会话中有效。
- 以 ID 指向的函数、newtype、struct、enum 与公开可见性信息。
- 以 `LocalId` 指向的局部读取、绑定和赋值，不再以 `String` 查找变量。
- 已解析的函数调用、struct 构造、字段读取、enum 构造和 match 模式。
- 保持 Yan 原生 `Type` 的声明位置；此时表达式类型尚未写入节点。

模块解析在进入 HIR 前完成。文件系统读取仍属于 `yanc`，但解析结果必须作为显式模块图输入交给前端；不得以 CLI 中拼接声明的方式替代 HIR 名称解析。

## Typed HIR

类型检查成功返回 `TypedProgram`，而非 `Result<(), TypeError>`。它拥有 HIR 的已解析引用，并补充：

- 每个值表达式的确定 `TypeId` 或等价的规范化 Yan 类型。
- 函数调用的参数、返回类型和已解析 `DefId`。
- 字段、enum variant、newtype 与 match 绑定的已验证语义目标。
- `let mut` 的局部可变性和赋值兼容性结论。
- `return`、`?`、if、match、for 的已验证控制流类型。

Typed HIR 的类型只能来自 Yan 类型系统。不得记录 Rust `String`、`Vec`、trait、生命周期、WASM 表示或平台 adapter 的内部类型。

## MIR

MIR 是后端唯一的共同输入，按函数保存控制流图：

- 函数由固定顺序的基本块构成；每个基本块以一个终结指令结束。
- 局部值使用 `LocalId` 表示；`let mut` 仅允许 MIR 中对相应局部位置产生赋值，不引入可变字段、可变集合或全局状态。
- 指令最小集合包括赋值、聚合值构造、字段读取、二元运算、函数调用与平台调用。
- 终结指令最小集合包括跳转、条件分支、match 分派、返回、Result 错误传播和不可达。
- 复杂表达式按源代码求值顺序拆分为临时局部，确保解释器、Rust 和 WASM 后端观察到相同副作用顺序。

MIR 不在 M14 增加新的 Yan 语法。它只能覆盖当期已经被 parser、HIR 和类型检查接受的语义。

## 解释器与 Rust 后端

解释器是语义对照实现，不是部署目标。M14 后它应消费 Typed HIR 或 MIR；选择 MIR 时优先，因为这能使解释器与后端共享控制流语义。

第一个 Rust 后端应从最小纯程序开始：基础类型、局部绑定与赋值、纯函数调用、算术、比较、if、match、return 和现有 Result 传播。它生成仅位于构建目录的 Rust crate，并由固定的 Yan runtime crate 承担 Yan 值表示和平台 API 映射。生成文件不属于用户 API，也不得手工编辑。

Rust 后端、runtime 和 adapter 的接口必须由后续里程碑分别定义；M14 不创建 Cargo 项目、不调用 rustc、不引入第三方依赖，也不决定 adapter ABI。

## 验证原则

每个已支持语义都需要同一 Yan fixture 的三类断言：

1. 类型检查成功或得到确定的 Yan 诊断。
2. 解释器在 Typed HIR/MIR 上输出既定结果。
3. Rust 后端引入后，生成产物输出与解释器完全一致。

后端失败必须转换为稳定的 Yan 构建诊断；不得透传 Rust 编译器内部类型、调用栈或操作系统本地化文本。

## 明确不做

- M14 不新增 Yan 语法、类型、标准库 API、CLI 命令、包模型或 capability 检查。
- M14 不实现 Rust、WASM 或其他后端，不调用 Cargo/rustc，也不生成 Rust 代码。
- 不将 Rust 所有权、借用、生命周期、trait、宏、`unsafe` 或任意 crate 导入 HIR、Typed HIR、MIR 或 Yan 源码。
- 不预先设计通用优化框架、插件点、跨目标 ABI 或未使用的 trait 抽象。
