# 文件模块示例

`src/` 是模块根目录。`examples.modules.application` 对应
`src/examples/modules/application.yan`，并通过单符号 import 使用同目录中的
`examples.modules.domain` 公开声明。

`module_declaration/explicit.yan` 显式声明模块路径；`implicit.yan` 省略声明，由文件路径推导。
