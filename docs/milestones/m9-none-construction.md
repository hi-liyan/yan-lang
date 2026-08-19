# M9：None 构造推断

状态：进行中
范围：使 `None` 能作为参数传给期望 `Option<T>` 的函数，例如 `parse_port(None)?`。

## 验收标准

以下调用可由 `yanc run examples/language-design/06-result/01_result.yan` 的同类型上下文检查：

```yan
parse_port(None)?
```

并且：

- 当函数参数的已声明类型为 `Option<T>` 时，`None` 构造该参数类型的空值。
- 裸 `None` 不作为变量解析；没有 `Option<T>` 参数上下文时仍产生英文诊断。
- `None` 导致的 Err 经 `?` 传播到 main 时，`yanc run` 必须以非零状态和英文执行诊断结束，不得静默成功。
- 不新增 Option 方法、嵌套 Option、局部变量或返回值标注的 None 推断。
- M2 至 M8 示例持续可执行。

## 实现策略

类型检查器在验证已知函数签名的实参时，根据对应 `Option<T>` 参数类型解释 `None`。解释器只在已通过该检查的程序中将 `None` 求值为无载荷 Option 值。
