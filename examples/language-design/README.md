# Yan 语言示例评审

本目录中的 `.yan` 文件是语法与使用体验提案，不是当前 `yanc` 已支持的可编译程序。请按编号阅读；每个文件只增加少量语言概念。

| 示例 | 需要确认的重点 |
| --- | --- |
| `01_values.yan` | `package`、`import`、`let`、类型标注、默认不可变、`mut` |
| `02_functions.yan` | 函数、显式返回类型、调用与表达式块 |
| `03_structs.yan` | `struct`、具名字段、构造、点访问与 `type` 新类型构造 |
| `04_enums_and_match.yan` | 封闭 `enum`、`match` 与穷尽分支 |
| `05_option_and_result.yan` | `option<T>`、`result<T, E>`、`?` 与显式错误转换 |
| `06_modules.yan` | 包、导入、纯业务代码与平台能力的边界 |
| `07_cli.yan` | 通用语言如何以 CLI 库交付工具 |
| `08_web_api.yan` | Web 如何作为库而非语言关键字出现 |
| `09_project_shape.yan` | 面向 AI 可维护性的业务模块写法 |
| `10_module_declaration/` | 一个文件一个模块；显式与路径推导的模块声明 |

共同约定：

- 使用 Java/Kotlin 风格的 `import`、具名数据类型与花括号代码块；源码模块使用 `module` 声明。
- 使用 `name: Type` 标注类型，局部变量允许在右侧足够明确时省略类型。
- 默认省略分号，以换行分隔语句；formatter 将在未来固定这一规则。
- 使用 `struct + fn + enum`，不引入类、继承、异常、`null`、隐式转换或注解魔法。
- `type UserId = int` 提议为真正的新类型，构造时使用 `UserId(42)`；这项语义需要审核确认。
- 字符串中的 `{name}` 提议为受限插值语法；这项语义需要审核确认。
- HTTP、CLI、文件、JSON、控制台和数据库通过 `yan.platform.*` 官方平台库导入，不作为语言关键字。
- 一个 `.yan` 文件对应一个 module；目录只表示命名空间，不是 module，也不使用 `mod.yan`。
- `module` 可显式写在文件头部；省略时由 `src/` 下的文件路径推导，编译器会以同一规则校验显式声明。

确认示例后，M2 会把它们转换为 parser、类型检查和诊断的验收样例。
