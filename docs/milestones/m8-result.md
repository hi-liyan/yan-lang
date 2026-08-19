# M8：Result 与错误传播

状态：进行中
范围：仅使 `examples/language-design/06-result/01_result.yan` 能被 `yanc run` 检查并执行。

## 前提

用户已确认内建泛型类型 `Result<T, E>`、`Ok(value)`/`Err(error)` 构造、Result match、显式 `return`、同错误类型的 `?` 传播，以及 `string.to_int()` 的受限转换。

## 验收标准

以下命令成功执行：

```powershell
cargo run -p yanc -- run examples/language-design/06-result/01_result.yan
```

标准输出必须为：

```text
8080
```

并且：

- `Result<T, E>` 必须恰有两个已定义的类型参数；`Ok` 与 `Err` 分别构造成功值或错误值。
- 以 `Result<T, E>` 为目标的 match 必须恰好包含 `Ok(binding)` 与 `Err(binding)` 两个分支，且每个绑定只在对应分支内可见。
- `return expression` 的表达式类型必须与当前函数的返回类型一致；return 在 match 分支中终止当前函数，而不是仅终止分支。
- `expression?` 仅接受 `Result<T, E>`，产出 `T`；当前函数返回类型必须为同一错误类型的 `Result<U, E>`，错误值原样传播。
- `string.to_int()` 是唯一允许的成员式调用，返回 `Result<int, unit>`；无效十进制文本产生 Err，而非编译器或解释器 panic。
- M2 至 M7 示例持续可执行。

## 包含语法

- 类型写法 `Result<T, E>`、构造 `Ok(expression)` 与 `Err(expression)`。
- `match result_value { Ok(binding) => expression Err(binding) => expression }`。
- `return expression` 与 `expression?`。
- `string.to_int()`。

## 不包含

- Result/Option 方法、任意成员调用、嵌套 Result、错误类型转换、不同错误类型的 `?`、try/catch、异常、defer、finally。
- wildcard、guard、嵌套/解构模式、match 分支代码块，以及 if、for、while、递归、用户模块、async、HTTP、数据库和 Rust 代码生成。

## 实现策略

parser 在既有表达式上记录 return、`?` 与唯一的成员调用路径。HIR 使用 Result、return 和传播表达式保留控制转移意图。类型检查器在函数返回类型上下文中验证 return 与 `?`，并用受限的 Result 构造器合成分支类型；解释器通过内部控制转移值在函数调用边界传播 return 和 Err。
