# M3：函数与字符串插值

状态：进行中
范围：仅使 `examples/language-design/02_functions.yan` 能被 `yanc run` 检查并执行

## 前提

用户已确认该示例中的具名函数、`name: type` 参数、显式返回类型、函数体最后一个表达式作为返回值、整数乘法、用户函数调用及 `{name}` 受限字符串插值。

## 验收标准

以下命令成功执行：

```powershell
cargo run -p yanc -- run examples/language-design/02_functions.yan
```

标准输出必须为：

```text
total: 597
```

并且：

- 函数参数数量和参数类型不匹配时，`yanc check` 报告带位置的诊断。
- 非 `unit` 函数的最后一个表达式必须与声明返回类型一致。
- 插值仅允许 `{identifier}`，且 identifier 必须是当前函数作用域中的变量。
- M2 的 `01_values.yan` 持续可执行。

## 包含语法

- 多个顶层 `fn`，具名且带类型的参数，以及显式返回类型。
- 函数体最后一个表达式作为隐式返回值。
- 用户函数调用、整数 `*` 与既有的 `+`、`==`。
- 字符串中的 `{identifier}` 插值。

## 不包含

- `return` 关键字、递归、闭包、函数值、函数重载、默认参数和泛型函数。
- 控制流、struct、enum、option、result、用户模块、async、HTTP、数据库和 Rust 代码生成。

## 实现策略

parser 将函数参数与表达式优先级写入 AST，HIR 将字符串拆分为字面文本和变量插值片段。类型检查器先收集函数签名，再检查函数体；解释器在同一 HIR 上调用用户函数。未来代码生成后端复用该 HIR。
