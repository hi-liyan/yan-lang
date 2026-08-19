# M11：条件表达式与列表循环

状态：进行中
范围：仅使 `examples/language-design/08-conditions/01_if.yan` 与 `examples/language-design/09-loops/01_for.yan` 能被 `yanc run` 检查并执行。

## 验收标准

```powershell
cargo run -p yanc -- run examples/language-design/08-conditions/01_if.yan
cargo run -p yanc -- run examples/language-design/09-loops/01_for.yan
```

标准输出必须分别为：

```text
pending
```

```text
cli
web
rust
```

并且：

- 支持 `if condition { ... } else { ... }` 表达式，condition 必须为 `bool`。
- 两个 `if` 分支必须产生兼容的类型；空代码块的类型为 `unit`。
- 支持 `for name in List<T> { ... }`，循环变量不可变且仅在循环体内可见。
- `for` 的表达式类型和求值结果固定为 `unit`；循环体不得产生非 `unit` 尾值。
- 代码块内声明的局部变量不得泄漏到外层作用域。
- M2 至 M10 示例持续可执行。

## 非目标

- 不支持省略 `else` 的 `if`、`while`、`break`、`continue`、范围循环、map 循环或 iterator 方法。
- 不支持循环变量重新赋值，也不支持将循环体作为一般值表达式使用。

## 实现策略

parser 将 `if` 和 `for` 作为表达式解析，并用显式语句块保存其嵌套语句。HIR 保持相同结构；类型检查器在克隆的局部绑定表中检查分支和循环体，解释器同样隔离局部环境，保证 `for` 最终返回 `unit`。
