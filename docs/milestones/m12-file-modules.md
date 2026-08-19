# M12：文件模块与显式导入

状态：已完成
范围：仅实现单个 `src/` 项目根目录内的文件模块解析，并将 `examples/language-design/10-modules/` 重组为与模块路径一致的多文件 fixture。

## 模块模型

- 一个 `.yan` 文件是一个模块；模块路径的每个小写段映射为项目根目录下的目录，最后一个段映射为 `.yan` 文件，例如 `examples.modules.domain` 映射为 `examples/modules/domain.yan`。
- `yanc check` 或 `yanc run` 的目标文件必须位于某个 `src/` 目录中；该目录是本次编译的项目根目录。目标文件显式声明 `module` 时，其路径必须与此映射一致；省略时从相对路径推导模块名。
- `import module.path.Symbol` 只导入一个 `pub` 的函数、struct、enum 或 newtype；同一模块的多个符号必须分别显式导入。
- `pub` 仅允许修饰顶层函数、struct、enum 与 newtype。未标记的声明只能在当前模块使用。
- 被导入模块是库模块，不要求定义 `main`；只有 `run` 的目标模块必须定义唯一且无参数的 `main`。

## 验收标准

```powershell
cargo run -p yanc -- run examples/language-design/10-modules/src/examples/modules/application.yan
cargo run -p yanc -- check examples/language-design/10-modules/src/examples/module_declaration/explicit.yan
cargo run -p yanc -- check examples/language-design/10-modules/src/examples/module_declaration/implicit.yan
```

`01_modules.yan` 的标准输出必须为：

```text
approve Yan syntax
```

并且：

- 模块声明与文件位置不匹配时产生可定位的英文诊断。
- 导入不存在模块、不存在符号或非 `pub` 符号时产生可定位的英文诊断。
- 导入的类型和函数与本模块声明在同一类型检查、调用与解释执行上下文中可用。
- M2 至 M11 的单文件示例持续可执行。

## 非目标

- 不支持 package、跨项目或网络依赖、相对导入、通配符导入、导入别名、重导出、循环导入、模块初始化代码、目录模块文件或自动发现所有源码文件。
- 不支持 `pub` 字段、方法、局部变量或 enum variant，也不在本阶段增加依赖方向与 capability 检查。

## 实现策略

先将当前概念性示例替换为 `src/examples/modules/application.yan`、`domain.yan` 和 `src/examples/module_declaration/` 的真实文件树。CLI 在读取入口文件后负责根据 import 路径定位并读取直接依赖，逐个完成 lexer、parser 和 lowering。前端只消费已链接的 HIR 声明；类型检查器验证可见性与重复定义，解释器以链接后的程序执行入口 `main`。文件系统访问和诊断渲染保持在 `yanc`，不下沉到 syntax、HIR、typeck 或 eval。
