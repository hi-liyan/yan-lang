# M14：已类型化 HIR 与最小 MIR

状态：实施中（已建立 TypedProgram 与最小 MIR 程序边界；名称 ID、表达式类型表、完整控制流 lowering 与解释器迁移待完成）
范围：在不增加 Yan 表面语法、标准库、CLI 命令或代码生成的前提下，建立已解析 HIR、Typed HIR 与最小 MIR，并迁移解释器消费稳定的已类型化表示。

## 目标

将当前“类型检查成功或失败”的前端闭环改为“类型检查产出可被执行和未来后端消费的 Typed HIR”。随后将当期已支持语义 lowering 为最小 MIR，使 Rust、WASM 和未来后端拥有唯一的共同输入边界。

详细分层和数据边界见[编译中间表示与多后端设计](../discussions/compiler-ir-and-backend.md)。

## 验收标准

- `yan-hir` 为模块、顶层声明、局部绑定、字段与 enum variant 建立稳定的编译会话内 ID；变量读取、调用、字段访问和模式不再由后续阶段以裸 `String` 重新查找。
- `yan-typeck` 对成功程序返回 `TypedProgram`；失败仍返回带 `Span` 的稳定 Yan 诊断。
- Typed HIR 为每个值表达式记录确定 Yan 类型，并保留函数调用、构造、字段、模式、赋值、`return`、`?`、if、match 与 for 的已验证语义目标。
- 新的 MIR 仅覆盖 M2 至 M13 已实现的语义，并以基本块、局部位置、指令与终结指令表达控制流和求值顺序。
- `yan-eval` 改为执行 Typed HIR 或 MIR；不得重新进行名字查找或类型判断。
- M2 至 M13 的可执行示例持续输出既定结果，且对应类型错误仍使用既有稳定英文诊断格式。
- 不改变 `yanc check`、`yanc run` 或 `yanc --help` 的文本输出和退出码。

## 包与依赖

本阶段允许新增 `yan-mir` crate。依赖方向固定为：

```text
yan-source <- yan-syntax <- yan-hir <- yan-typeck <- yan-mir
                                            ^            ^
                                            |            |
                                         yan-eval -------+
                                               \
                                                yanc
```

- `yan-hir` 只定义已解析、后端无关的 HIR，不依赖 `yan-typeck`、`yan-mir`、`yan-eval` 或 `yanc`。
- `yan-typeck` 只消费 HIR 并产出 Typed HIR；不得读取文件、输出文本或执行程序。
- `yan-mir` 只消费 Typed HIR，并负责控制流 lowering；不得读取文件、调用后端或决定 CLI 行为。
- `yan-eval` 只执行 Typed HIR 或 MIR；不得重新实现 parser、名称解析或类型规则。
- `yanc` 继续负责文件读取、模块图构建、阶段编排和诊断渲染。

若 Typed HIR 需要与原 HIR 共享稳定 ID 和类型定义，可放在 `yan-hir`；不得为此引入循环依赖。

## 实现顺序

1. 先为 M2 至 M13 的现有 fixture 补充 HIR、Typed HIR 与 MIR 的结构断言，锁定不新增语义的边界。
2. 引入 ID 与解析表，将模块链接结果从 CLI 声明拼接迁移为前端可消费的已解析模块图输入。
3. 将类型检查 API 改为返回 Typed HIR，并保留现有错误坐标与用户可见诊断。
4. 新增最小 MIR crate，将 Typed HIR 的函数体 lowering 为基本块和局部操作。
5. 将解释器迁移至 Typed HIR 或 MIR；优先选择 MIR，除非无法在不扩大本阶段语义的前提下完成。
6. 用同一批示例验证 parser、类型检查、MIR 与解释执行结果；Rust 后端另立里程碑。

## 非目标

- 不实现 Yan 到 Rust、WASM 或其他目标的代码生成，不创建构建目录，也不调用 Cargo/rustc。
- 不新增 `build` CLI 命令、项目清单、锁文件、formatter、capability、async、adapter ABI 或运行时 crate。
- 不新增或扩展类型别名、隐式转换、浮点混合算术、指数记法、可变 Map、Map 索引或遍历、bytes I/O、块注释、文档生成 CLI、struct 方法、构造器重载、嵌套/赋值解构、位置访问、忽略或剩余模式、单元素或高阶元组、泛型 struct、enum 方法、泛型 enum、多载荷变体、wildcard 或 guard 模式、嵌套模式、Option 方法、嵌套 Option、Result 方法、任意成员调用、省略 `else` 的 if、while、break、continue、范围/map 循环、iterator 方法、除 Result 传播外的 `?`、递归、package、跨项目依赖、相对/通配符/别名 import、重导出、循环导入、可变字段/集合/全局状态、`pub` 字段/方法/enum variant、async、HTTP、数据库、Rust 代码生成、包管理、formatter 或 capability 检查。
- 不引入第三方 Rust 依赖、通用优化框架、插件点或后端 trait；这些必须由后续有明确交付目标的里程碑证明必要性。

## 后续门槛

只有在 M14 的 Typed HIR/MIR 结构、解释执行和 M2 至 M13 回归都稳定后，才可设计 M15：最小 Rust 后端。M15 必须先定义生成 Rust 的构建目录、固定 runtime 边界、支持的 MIR 子集、后端失败的 Yan 诊断映射、端到端 fixture 与 `yanc build` 输出及退出码，才允许实现代码生成。
