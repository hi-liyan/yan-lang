# 一个文件一个模块

Yan 规定一个 `.yan` 文件对应一个模块。目录用于组织命名空间，不是独立模块，也不需要 `mod.yan`、`mod.rs` 或额外的加载声明。

假设以下文件位于项目 `src/` 目录：

```text
src/
  examples/
    module_declaration/
      explicit.yan
      implicit.yan
```

编译器从路径推导的模块名分别是：

```text
examples.module_declaration.explicit
examples.module_declaration.implicit
```

`explicit.yan` 在文件头显式写出模块名，便于读者立即识别归属。`implicit.yan` 省略声明，编译器使用文件路径作为默认模块名。

若文件存在显式 `module` 声明，其值必须与路径推导结果完全一致；不一致时是编译错误。这个规则使移动文件、导入模块和定位代码都有唯一答案。
