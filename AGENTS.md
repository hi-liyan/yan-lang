# Yan AI 开发约束

本文件约束所有参与 Yan 仓库的 AI Coding Agent。它将需求、架构和代码质量边界变成可重复执行的规则，防止 AI 在连续修改中引入无关能力、重复抽象或不可维护的实现。

## 1. 工作原则

1. 修改前确认当前阶段目标、验收条件和非目标。
2. 只实现用户明确请求或当前里程碑明确包含的能力；不得顺带实现后续阶段特性。
3. 每次修改保持最小范围。需求、重构和格式化不能混在同一次提交中。
4. 未写入语言设计或里程碑的语义、标准库 API、包格式和运行时行为，必须先设计再实现。
5. 不提交 `target/`、生成产物、临时文件或本地配置。

## 2. 需求与阶段边界

开发前必须读取：

- `docs/yan-language-design.md`：语言定位、核心原则和长期边界。
- `docs/milestones/`：当前阶段的目标、验收标准、包含范围与非目标。

当前处于 M4 新类型与结构体。只允许实现 `docs/milestones/m4-structs.md` 中列出的 `03-structs/01_structs.yan` 语法、类型检查、解释执行和诊断，并保持 M2、M3 可用。除维护已有功能外，禁止加入类型别名、隐式转换、显式转换函数、struct 方法、构造器重载、解构、结构体更新语法、泛型 struct、enum、match、option、result、控制流、递归、用户模块、package、async、HTTP、数据库、Rust 代码生成、包管理、formatter 或 capability 检查。

用户需求与当前里程碑冲突时，先更新或新增里程碑文档，再开始实现。不得以“预留接口”为理由创建未使用的抽象、配置项或依赖。

## 3. 架构与依赖边界

| crate | 唯一职责 | 允许依赖 |
| --- | --- | --- |
| `yan-source` | 源文件、位置、span 和诊断位置基础数据 | Rust 标准库 |
| `yan-syntax` | token、lexer，以及后续纯语法层能力 | `yan-source` |
| `yan-hir` | 与后端无关的稳定高层中间表示 | Rust 标准库；后续仅增加前端数据 crate |
| `yan-typeck` | HIR 的类型检查与语义诊断 | `yan-hir`、`yan-source` |
| `yan-eval` | 已通过类型检查的 HIR 的受限解释执行 | `yan-hir` |
| `yanc` | CLI、文件读取、编译流程编排和诊断展示 | 所有前端 crate |

必须遵守：

- 禁止循环依赖，低层 crate 不得依赖 `yanc`。
- `yan-source` 不得知道 token、AST、HIR、CLI 或代码生成概念。
- `yan-syntax` 只能处理源文本与语法，不得读取文件、访问环境变量或决定 CLI 行为。
- `yan-hir` 不得引入 Rust 后端、HTTP、数据库或其他平台类型。
- `yan-typeck` 不得读取文件、输出文本或执行程序；它只验证 HIR。
- `yan-eval` 不得重新实现解析或类型规则；它只执行已通过类型检查的 HIR。
- `yanc` 只负责编排，不得把 lexer、parser、类型检查规则复制到 CLI 中。
- 未来代码生成必须消费 HIR，禁止从 token 或 AST 直接生成 Rust。

## 4. 语言与工程边界

- Yan 是独立的通用语言；Rust 是当前 `yanc` 的实现语言和早期构建后端，不是 Yan 语义的一部分。
- 不得将 Rust 的生命周期、借用、trait 边界、`serde`、`tokio`、`axum` 等类型泄漏到 Yan 前端数据模型或公开 CLI 输出。
- 不得把 HTTP、路由、数据库、ORM、middleware 或 AI provider 设计为 Yan 关键字；它们只能在未来以平台库形式出现。
- 新增 Rust 第三方依赖前，必须说明其解决的当前问题、替代方案和体积/维护影响。优先使用标准库。
- 诊断信息必须稳定、可定位、面向用户；不得用 `panic!` 报告普通用户输入错误。
- `yanc` 面向用户的命令行输出必须统一使用英文，包括标准输出、错误输出和诊断文本；不得在同一条 CLI 输出中混用中文。Yan 程序自身写出的运行时数据不属于 CLI 文案，必须原样转发。源码注释、Rustdoc 与设计文档继续使用中文。
- 默认人类可读输出必须遵守 `docs/yan-language-design.md` 的“`yanc` 文本输出契约”：帮助为 `Usage:` 格式，`check` 成功为 `<path>: check succeeded`，诊断为 `error: <path>:<line>:<column>: <message>`。新增命令必须先补充语言设计中的输出与退出码约定。
- 诊断 `<message>` 必须使用不带句号的简短英文句式；普通英文单词使用小写，语言名与版本号可保留其正式大小写；源码标识符、类型、模块路径和源码片段用反引号包围。不得透传操作系统本地化错误文本、Rust 类型名、调用栈或实现细节。
- 除不可恢复的编译器内部不变量外，禁止 `unwrap`、`expect` 和 `panic!`。内部不变量的 `expect` 必须说明其成立原因。

## 5. 代码与注释规范

### 中文注释

- 所有对外可见的 Rust `pub` 类型、字段、函数、trait 和模块必须使用详细中文 Rustdoc 注释，说明职责、输入输出、约束或生命周期语义。
- 复杂算法、状态转换、错误分支和编译阶段边界必须使用中文行内注释，解释“为什么这样做”和“不这样做会发生什么”。
- 注释必须与代码同步修改；过期注释比缺少注释更严重。
- 禁止翻译代码本身的无意义注释，例如“给变量赋值”。
- 禁止用注释掩盖模糊设计。语义不清时应先修改命名、类型或设计文档。

### 代码结构

- 函数只承担一个明确职责；超过一个编译阶段或一个错误边界时必须拆分。
- 所有公开 API 的参数、返回值和错误行为必须显式，避免布尔开关或不透明字符串协议。
- 不为尚未发生的需求创建 trait、泛型层、builder、全局注册表或插件点。
- 测试名称描述可观察行为；每个 bug 修复必须增加能复现该 bug 的测试。

## 6. 验证要求

涉及 Rust 代码时，至少执行：

```powershell
cargo fmt --all -- --check
cargo test --workspace
git diff --check
git status --short
git diff
```

无法执行时，必须在最终说明中明确未执行的命令、原因和风险。不得将未验证状态表述为已通过。

## 7. Git 提交规范

提交必须使用 **Conventional Commits** 格式，而不是自由文本：

```text
<type>(<scope>): <中文祈使句描述>
<type>: <中文祈使句描述>
```

- `type` 只能使用：`feat`、`fix`、`docs`、`style`、`refactor`、`perf`、`test`、`build`、`ci`、`chore`。
- `scope` 可选，使用稳定模块名，例如 `lexer`、`source`、`cli`、`docs`。
- description 必须使用中文、祈使句、简洁明确，不以句号结尾。
- 禁止 `update`、`fix bug`、`done`、`随便改了一下` 等模糊描述。

示例：

```text
feat(lexer): 添加字符串字面量词法分析
fix(source): 修复多字节字符列号计算
docs: 更新 M1 编译器前端验收标准
chore: 初始化 Rust workspace
```

一个 commit 只能包含一个完整、可运行、可回滚的逻辑变更。提交前必须检查 `git status` 和 `git diff`，确认没有调试代码、无关格式化噪音、生成产物或超出需求边界的修改。
