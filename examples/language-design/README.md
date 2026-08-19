# Yan 语言示例

本目录按语言概念分类，而不是按实现时间平铺。下表标记当前状态：`已实现` 的示例可由 `yanc run` 执行；`设计提案` 仅用于审阅语法与语义，不得据此扩展编译器。

| 主题 | 状态 | 示例 |
| --- | --- | --- |
| 数据类型与变量绑定 | 部分已实现 | `01-data-types/01_variables_and_bindings.yan`、`02_int.yan`、`03_bool.yan`、`04_string.yan`、`05_list.yan`、`06_unit.yan`、`07_bytes.yan`、`08_map.yan`、`09_float.yan` |
| 函数 | 已实现 | `02-functions/01_functions.yan` |
| 新类型与结构体 | 已实现 | `03-structs/01_structs.yan` |
| 枚举与 match | 已实现 | `04-enums-and-match/01_enums_and_match.yan` |
| Option | 设计提案 | `05-option/01_option.yan` |
| Result | 设计提案 | `06-result/01_result.yan` |
| 集合与元组 | 元组为设计提案 | `07-collections/02_tuples_and_destructuring.yan` |
| 条件 | 设计提案 | `08-conditions/01_if.yan` |
| 循环 | 设计提案 | `09-loops/01_for.yan` |
| 模块 | 设计提案 | `10-modules/01_modules.yan`、`02_module_declaration/` |
| 平台库 | 设计提案 | `11-platform/01_cli.yan`、`02_web_api.yan` |
| 项目组织 | 设计提案 | `12-project-shape/01_domain_service.yan` |

## 当前已实现的类型

当前 `yanc` 支持 `int`、`float`、`bool`、`string`、`bytes`、`unit`、`List<T>`、`Map<string, T>`、新类型、struct、封闭 enum 与穷尽 `match`，并支持局部 `let` / `mut` 绑定。`Option<T>` 与 `Result<T, E>` 仍为设计提案，不能使用当前 `yanc` 执行。

## 共同约定

- 使用 Java/Kotlin 风格的 `import`、具名数据类型与花括号代码块；源码模块使用 `module` 声明。
- 使用 `name: Type` 标注类型，局部变量允许在右侧足够明确时省略类型。
- 基础类型使用小写；用户类型、enum 变体与复合类型构造器使用 PascalCase；函数、变量和 module 路径使用小写 snake_case。
- 默认省略分号，以换行分隔语句；formatter 将在未来固定这一规则。
- 使用 `struct + fn + enum`，不引入类、继承、异常、`null`、隐式转换或注解魔法。
- `type UserId = int` 是真正的新类型，构造时使用 `UserId(42)`，不会与底层类型隐式互换。
- HTTP、CLI、文件、JSON、控制台和数据库通过 `yan.platform.*` 官方平台库导入，不作为语言关键字。
- 一个 `.yan` 文件对应一个 module；目录只表示命名空间，不是 module，也不使用 `mod.yan`。
- 元组提议只用于少量位置型返回值；首版仅支持 `(T1, T2)` 形式的返回类型、元组字面量和顶层 `let (name1, name2) = value` 解构，不支持位置访问、嵌套解构、忽略项、剩余模式或赋值解构。
