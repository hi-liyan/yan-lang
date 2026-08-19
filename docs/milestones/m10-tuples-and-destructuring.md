# M10：二元与三元元组解构

状态：已完成
范围：仅使 `examples/language-design/07-collections/02_tuples_and_destructuring.yan` 能被 `yanc run` 检查并执行。

## 验收标准

```powershell
cargo run -p yanc -- run examples/language-design/07-collections/02_tuples_and_destructuring.yan
```

标准输出必须为：

```text
Lin Yan
```

并且：

- 支持 `(T1, T2)`、`(T1, T2, T3)` 类型与对应字面量。
- 顶层 `let (name1, name2) = value` 或 `let (name1, name2, name3) = value` 依次绑定元素类型和值。
- 解构绑定名不能重复，也不能与既有局部绑定冲突。
- 不支持 `.0` 位置访问、嵌套或赋值解构、忽略/剩余模式、单元素或四个以上元素的元组。
- M2 至 M9 示例持续可执行。

## 实现策略

parser 将括号内带逗号的二元或三元写法区分为类型、字面量和 let 解构。HIR 用明确的 tuple 类型和值表示它们；类型检查器在解构处验证元素类型，解释器按顺序绑定值。
