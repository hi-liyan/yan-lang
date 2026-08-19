# M2：可执行值与绑定子集

状态：已完成
范围：仅使 `examples/language-design/01-data-types/01_variables_and_bindings.yan` 能被 `yanc run` 检查并执行

## 前提

用户已确认 `01_variables_and_bindings.yan` 中的模块、导入、绑定、类型标注、默认不可变、`mut`、列表、整数加法及 `yan.platform.console` 的使用体验。其他示例仍处于设计评审，不能据此扩展 M2。

## 验收标准

以下命令成功执行：

```powershell
cargo run -p yanc -- run examples/language-design/01-data-types/01_variables_and_bindings.yan
```

标准输出必须为：

```text
Yan
1
```

并且：

- `yanc check` 可以完成该子集的解析和类型检查，不只报告 token 数量。
- 错误的变量名、错误赋值、错误类型标注和不支持的导入会产生带位置的诊断。
- lexer、parser、类型检查和执行器均有针对性测试。

## 包含语法

- 可选的 `module a.b` 头部与 `import yan.platform.console`。
- 无参数的 `fn main() -> unit { ... }`。
- `let`、`let mut`、可选 `name: type` 标注和赋值。
- `int`、`bool`、`string`、`unit`、`List<string>`。
- 整数 `+`、同类型基础值 `==`、变量引用、字符串/整数/布尔/列表字面量。
- `console.println(value)`。

## 不包含

- 其他函数、参数、返回值、控制流、struct、enum、Option、Result、字符串插值。
- 用户 module 导入、项目文件路径一致性校验、package、async、文件 I/O、HTTP、数据库和 Rust 代码生成。
- 对其他 `examples/language-design/` 文件的兼容承诺。

## 实现策略

`yanc run` 在 M2 使用解释执行器验证语义闭环：源码经 lexer、parser、HIR lowering、类型检查后执行。解释器不是长期部署后端；未来 Rust 代码生成必须消费相同的 HIR。
