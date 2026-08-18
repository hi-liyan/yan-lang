# M4：新类型与结构体

状态：进行中
范围：仅使 `examples/language-design/03-structs/01_structs.yan` 能被 `yanc run` 检查并执行

## 前提

用户已确认该示例中的 `type` 新类型、具名 `struct` 字段、字段默认值、具名结构体字面量、点字段读取，以及结构体作为函数参数与返回表达式中的值。

## 验收标准

以下命令成功执行：

```powershell
cargo run -p yanc -- run examples/language-design/03-structs/01_structs.yan
```

标准输出必须为：

```text
Lin
```

并且：

- `UserId` 是真正的新类型，不可与 `int` 隐式互换；`UserId(42)` 是唯一支持的构造方式。
- 结构体字面量必须使用具名字段；缺少无默认值字段、重复字段、未知字段或字段类型不匹配时，`yanc check` 报告带位置的英文诊断。
- 省略带默认值的字段时使用声明默认值；显式提供时使用显式值。
- 点访问仅允许读取已定义的结构体字段。
- M2 的 `01-data-types/01_variables_and_bindings.yan` 与 M3 的 `02-functions/01_functions.yan` 持续可执行。

## 包含语法

- `type Name = ExistingType` 声明真正的新类型。
- `struct Name { field: Type [= default] }` 具名字段声明。
- `Name(value)` 新类型构造、`Name { field: value }` 具名结构体字面量与 `value.field` 字段读取。
- 既有函数调用、变量绑定、字符串插值与 `console.println`。

## 不包含

- 类型别名、隐式转换、显式解包或转换函数。
- struct 方法、构造器重载、解构、结构体更新语法、嵌套 struct 定义、泛型 struct、可见性修饰符与派生能力。
- enum、match、option、result、控制流、递归、用户模块、async、HTTP、数据库和 Rust 代码生成。

## 实现策略

parser 将类型声明、结构体声明、具名字段与点访问写入 AST。HIR 保留与后端无关的声明和表达式；类型检查器建立源文件级类型声明表，并在所有函数体前检查默认字段值。解释器以具名字段映射和显式新类型包装值执行同一 HIR。
