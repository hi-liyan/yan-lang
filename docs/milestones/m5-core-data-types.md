# M5：核心数据类型与注释

状态：部分完成（示例验收已通过；文档注释 HIR 附着尚未实现）
范围：使 `examples/language-design/01-data-types/07_bytes.yan`、`08_map.yan` 与 `09_float.yan` 可由 `yanc run` 检查并执行。

## 包含

- `float`：固定 IEEE 754 `f64`，不与 `int` 隐式转换。
- `bytes` 与内建构造 `bytes.from_hex(string)`；非法十六进制字符串产生带位置英文诊断。
- `Map<string, T>` 与 `{ "key": value }` 字面量；键必须为 string，值类型一致。
- `//` 普通注释与 `///` 文档注释。普通注释作为 trivia 忽略；文档注释附着到紧随其后的顶层声明，并由 HIR 保留。

## 验收

```powershell
cargo run -p yanc -- run examples/language-design/01-data-types/07_bytes.yan
cargo run -p yanc -- run examples/language-design/01-data-types/08_map.yan
cargo run -p yanc -- run examples/language-design/01-data-types/09_float.yan
```

三条命令均成功，且 M2、M3、M4 示例持续可执行。

## 不包含

- 浮点与整数混合算术、指数记法、NaN 专用语义。
- 可变 Map、索引、遍历、Map 方法、任意键类型与嵌套 Map 语法扩展。
- bytes I/O、编码库、base64、切片、索引和可变 bytes。
- 块注释、嵌套注释、注释指令、文档生成 CLI 与 API JSON 输出。
