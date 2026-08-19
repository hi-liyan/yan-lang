# M7：Option

状态：进行中
范围：仅使 `examples/language-design/05-option/01_option.yan` 能被 `yanc run` 检查并执行。

## 前提

用户已确认内建泛型类型 `Option<T>`、`Some(value)` 构造，以及对 `Option<T>` 局部值使用 `Some(binding)` 和 `None` 的穷尽 `match`。

## 验收标准

以下命令成功执行：

```powershell
cargo run -p yanc -- run examples/language-design/05-option/01_option.yan
```

标准输出必须为：

```text
Lin
```

并且：

- `Option<T>` 只能有一个显式类型参数，且其元素类型必须是已定义类型。
- `Some(value)` 必须恰有一个实参，结果类型为 `Option<T>`；`Some` 不是可重定义的用户函数。
- 以 `Option<T>` 为目标的 match 必须恰好包含一次 `Some(binding)` 与一次 `None`，且 `binding` 仅在对应 Some 分支内可见并具有类型 `T`。
- Option match 的所有分支结果类型必须一致；把 `Some` 或 `None` 用于 enum match、或把 enum 变体用于 Option match 时产生带位置的英文诊断。
- M2、M3、M4、M5 和 M6 示例持续可执行。

## 包含语法

- 类型写法 `Option<T>`。
- 值构造 `Some(expression)`。
- `match option_value { Some(binding) => expression None => expression }` 作为既有 match 表达式的一种目标类型。

## 不包含

- 独立 `None` 构造、None 的类型推断、Option 方法、嵌套 Option、Option 与其他类型的隐式转换。
- Result、`?`、`Ok`、`Err`、wildcard、guard、嵌套/解构模式、match 分支代码块，以及对 struct、List、Map、基本值的匹配。
- `if`、`for`、`while`、`return`、递归、用户模块、async、HTTP、数据库和 Rust 代码生成。

## 实现策略

parser 将无前缀 `Some(binding)` 与 `None` 记录为 match 模式。HIR 以单独的 `Option` 类型而非用户 enum 表示内建可选值。类型检查器在现有 match 规则旁验证 Option 的固定两种模式与载荷绑定；解释器以带或不带载荷的 Option 值执行选中分支。
