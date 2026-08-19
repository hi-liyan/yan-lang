# M6：枚举与穷尽匹配

状态：进行中
范围：仅使 `examples/language-design/04-enums-and-match/01_enums_and_match.yan` 能被 `yanc run` 检查并执行。

## 前提

用户已确认该示例中的封闭 `enum`、零或一个具名载荷的变体、`Enum.Variant` 构造、`Enum.Variant(binding)` 构造，以及以 enum 局部值为目标的穷尽 `match` 表达式。

## 验收标准

以下命令成功执行：

```powershell
cargo run -p yanc -- run examples/language-design/04-enums-and-match/01_enums_and_match.yan
```

标准输出必须为：

```text
succeeded
```

并且：

- enum 名称不能与已有新类型或结构体名称重复，变体名称在同一 enum 内必须唯一。
- 变体载荷只能是零个或一个具名且显式类型的值；构造时的载荷数量与类型必须匹配。
- `match` 的目标必须是已定义的 enum 值；每个分支必须匹配该 enum 的一个变体。
- 每个变体必须恰好出现一次。缺失、重复、未知或属于其他 enum 的变体均由 `yanc check` 报告带位置的英文诊断。
- 有载荷变体的分支必须用一个局部绑定接收载荷；无载荷变体不得声明绑定。绑定仅在该分支表达式内可见。
- M2、M3、M4 和 M5 示例持续可执行。

## 包含语法

- 顶层 `enum Name { Variant Variant(value: Type) }` 声明；enum 不支持泛型、方法或嵌套定义。
- `Name.Variant` 与 `Name.Variant(value)` 变体构造。
- `match value { Name.Variant => expression Name.Variant(binding) => expression }` 作为表达式；分支体只能是单一既有表达式。
- 复合类型构造器统一采用 PascalCase：`List<T>`、`Map<string, T>`、`Option<T>`、`Result<T, E>`；本阶段仅迁移已实现的 `List` 与 `Map` 拼写。

## 不包含

- Option、Result、泛型 enum、enum 方法、可见性、派生能力、递归 enum、多个或匿名变体载荷。
- wildcard、guard、嵌套/解构模式、match 语句、match 分支代码块，以及对 struct、List、Map、基本值的匹配。
- `if`、`for`、`while`、`return`、递归、用户模块、async、HTTP、数据库和 Rust 代码生成。

## 实现策略

parser 将 enum 声明、变体构造与 match 分支写入语法树。HIR 保留后端无关的 enum、构造和值匹配表达式；类型检查器先收集源文件级 enum 声明，再验证构造、载荷绑定、分支返回类型与穷尽性。解释器以 enum 名称、变体名称和可选载荷执行同一 HIR，且仅在选中分支的局部环境中提供载荷绑定。
