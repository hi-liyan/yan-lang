# M13：可变绑定与公开声明关键字

状态：已完成
范围：固定 `let mut` 的可变局部绑定语义，并将模块公开声明关键字从 `public` 统一替换为 `pub`。

## 验收标准

```powershell
cargo run -p yanc -- run examples/language-design/13-mutation-and-visibility/01_mut.yan
cargo run -p yanc -- run examples/language-design/13-mutation-and-visibility/src/examples/visibility/application.yan
```

标准输出必须分别为：

```text
2
```

```text
visible
```

并且：

- `let mut name = value` 声明后可使用 `name = value` 重新赋值，赋值类型必须与初始类型一致。
- 未使用 `mut` 的绑定、函数参数、循环变量和 match 绑定仍不可赋值。
- `pub` 仅允许修饰顶层 `type`、`struct`、`enum` 与 `fn` 声明，并允许被 `import module.Symbol` 引入。
- `public` 不再是语言关键字，作为顶层声明前缀时必须产生语法诊断。
- M2 至 M12 示例持续可执行。

## 非目标

- 不支持可变字段、可变集合、可变全局状态、`pub` 字段/方法/enum variant、可见性分级或 `pub(crate)` 形式。
- 不改变赋值作用域、循环控制流或模块路径与导入语义。

## 实现策略

保留既有 `Statement::Let { mutable }` 与赋值类型检查、解释执行逻辑，并以独立示例锁定行为。parser 将顶层可见性前缀识别为 `pub`；HIR 继续用布尔可见性字段表示语义，CLI 模块链接只选择该字段为真的声明。
