# Yan 后端语言设计

日期：2026-06-05  
状态：待评审草案

## 1. 目标

Yan 是一门编译型、强类型的后端编程语言，目标是在不放弃接近 Rust 安全等级的前提下，显著提升 API 开发和业务服务开发效率。

Yan 不打算成为另一门低层系统语言克隆。它的核心目标是把后端开发中的高频工作直接提升为语言一等能力，包括：

- HTTP API 开发
- 业务逻辑组织
- 显式错误处理
- 原生 SQL 执行
- 事务安全的数据访问
- 并发安全的服务代码

这门语言应当编译为本地可执行文件，并把大量传统后端框架依赖运行时约定的能力，前移为编译器可检查的语言语义。

## 2. 产品定位

Yan 是一门通用后端语言，但重点优化 HTTP API 服务开发。

其核心约束如下：

- 编译型语言
- 强静态类型
- 默认类型安全
- 默认非空
- 错误模型显式
- 支持原生 SQL，但不试图用 ORM DSL 取代 SQL
- 安全目标接近 Rust，但不照搬 Rust 的表层模型

Yan 不以以下方向为主要优化目标：

- 操作系统内核或嵌入式开发
- 依赖宏系统的元编程
- 高性能数值计算
- 依赖注解和框架魔法的开发模式

## 3. 核心价值主张

Yan 试图结合两类通常分离的能力：

- 现代后端框架所提供的开发效率
- 安全系统语言所提供的显式性和编译期保证

它的核心思想是：

> 将路由、请求提取、事务作用域、SQL 绑定、错误映射等后端框架约定提升为语言能力，使编译器可以直接检查。

## 4. 语言模型

Yan 的程序组织方式以“后端服务边界”为中心，而不是只有文件和自由函数。

### 4.1 顶层构造

- `app`：应用装配与启动边界
- `module`：业务模块边界
- `route`：HTTP 路由声明
- `endpoint`：HTTP 处理单元
- `fn`：普通内部函数
- `record`：主要数据类型
- `error`：显式错误类型
- `datasource`：数据源声明

### 4.2 结构目标

这套结构的目标是：

- 服务边界清晰可见
- 接口组织比传统框架装配式代码更容易扫描
- 业务逻辑不退化为杂乱的 controller 代码
- 数据源和请求作用域资源能够被编译器感知

## 5. 示例形态

```yan
datasource main_db: Postgres {
  url: env("DATABASE_URL")
  pool_size: 32
  timeout_ms: 3000
}

app api {
  listen ":8080"

  use user
  use main_db
}

module user {
  route GET "/users/:id" -> get_user
  route POST "/users" -> create_user
}

record UserView {
  id: UserId
  name: Str
  email: Str
}

record CreateUserInput {
  name: Str
  email: Str
}

error CreateUserError {
  InvalidEmail
  EmailTaken
}

endpoint get_user(ctx: RequestCtx, id: UserId) -> Result<UserView, HttpError> {
  let row = ctx.main_db.query_one[UserRow](
    "
    SELECT id, name, email
    FROM users
    WHERE id = $1
    ",
    [id],
  )?

  Ok(UserView {
    id: row.id
    name: row.name
    email: row.email
  })
}

endpoint create_user(ctx: RequestCtx, body: Json<CreateUserInput>) -> Result<UserView, CreateUserError> {
  let input = body.value

  let email = check input.email as Email else CreateUserError.InvalidEmail

  ctx.main_db.transaction(fn(tx) {
    let inserted = tx.query_one[UserRow](
      "
      INSERT INTO users(name, email)
      VALUES ($1, $2)
      RETURNING id, name, email
      ",
      [input.name, email],
    )?

    Ok(UserView.from(inserted))
  })
}
```

这个示例不是最终语法定稿，而是用来表达 Yan 期望提供的语义能力和使用体验。

## 6. 安全模型

Yan 必须在聚焦后端开发的同时，保持较高的静态安全标准。

### 6.1 默认非空

所有类型默认非空。只有通过 `Option<T>` 才能表达“值可能不存在”。

这样可以避免隐式空值传播，并强制开发者显式建模缺失状态。

### 6.2 错误显式

所有可能失败的操作都必须通过 `Result<T, E>` 暴露失败路径。

Yan 不应当采用“默认依赖未受检异常”的控制流模型。

这条规则对以下场景尤其重要：

- 数据库访问
- 请求校验
- 权限校验
- 外部服务调用
- 文件和网络 I/O

### 6.3 资源受作用域约束

代表资源的值不是普通可自由复制的业务值。

例如：

- `DbConn`
- `DbTx`
- `RequestCtx`
- 文件句柄
- 套接字

编译器必须阻止这些资源逃逸其有效作用域。

禁止的行为示例：

- 从 endpoint 返回一个仍然存活的事务对象
- 将请求作用域对象存入全局状态
- 在没有安全证明的情况下，把不可共享资源捕获到后台任务中

### 6.4 可变性受控

局部绑定默认不可变。变更必须显式声明。

共享可变状态必须通过编译器认可的同步或所有权规则来保护。

### 6.5 并发安全

Yan 在 v0.1 中应当支持 async 后端编程，但必须拒绝不安全的跨任务共享。

跨 async 任务边界传递的值，必须满足语言定义的可发送或可共享约束。

请求作用域和事务作用域对象默认不允许随意跨任务传递，除非编译器能够证明这样做是合法的。

### 6.6 `unsafe` 边界

`unsafe` 只应保留给底层 runtime、FFI，以及那些无法通过静态分析证明安全的极少数操作。

普通应用代码不应依赖 `unsafe`。

## 7. 后端原生语义

Yan 应当把常见后端概念视为语言概念，而不是仅仅作为库约定存在。

### 7.1 Endpoint 语义

`endpoint` 不是普通函数别名，而是一个受检查的接口单元，包含：

- HTTP 方法与路径绑定
- 类型化输入提取
- 类型化输出契约
- 显式错误边界
- 与认证、上下文、事务规则的集成

### 7.2 请求提取

Yan 应支持对以下输入进行类型化提取：

- 路径参数
- 查询参数
- 请求头
- JSON 请求体
- 请求上下文

编译器应检查提取规则是否合法，并确保可选性通过类型显式表达。

### 7.3 鉴权

鉴权能力必须以编译器可以结构化理解的方式表达。

这并不意味着 v0.1 就要做完整的策略证明系统，而是要求鉴权不能隐藏在无类型的框架钩子中。

Yan 应为以下能力预留模型：

- 已认证主体访问
- endpoint 内策略校验
- 显式受保护操作

### 7.4 数据源与事务边界

数据源必须是语言可见的顶层资源，事务则通过标准库 API 表达，而不是通过语言关键字表达。

这样编译器才能检查：

- 数据源是否已被 `app` 引入
- endpoint 中访问的资源是否存在
- 事务资源边界
- 查询上下文是否合法
- 提交和回滚流程是否遵守约束

## 8. SQL 策略

Yan 将原生 SQL 作为一等能力支持，但 SQL 通过标准库 API 使用，而不是定义为语言关键字。

Yan 明确不打算用大型查询 DSL 或 ORM 抽象来取代 SQL。

### 8.1 设计原则

开发者负责编写 SQL，标准库负责执行，编译器负责检查语言与 SQL 的集成边界。

### 8.2 编译器职责

编译器应检查：

- SQL 参数是否全部绑定
- 绑定值类型是否兼容
- 标准查询 API 的返回类型与调用场景是否匹配
- 查询结果列是否能映射到目标 `record`
- 查询执行是否发生在合法的连接或事务上下文中

### 8.3 v0.1 范围

v0.1 应支持：

- 原生 SQL 字符串输入
- 标准库参数绑定
- 结果映射
- 感知事务作用域的执行模型

v0.1 不要求必须依赖真实数据库在线 schema 检查。

后续阶段可以通过读取 migration 元数据或数据库 schema 文件增强校验能力。

## 9. 类型系统方向

Yan 的表层使用体验应比 Rust 更轻，但仍然保持强静态保证。

### 9.1 主要类型构件

- 基础类型
- `record`
- `enum`
- `Option<T>`
- `Result<T, E>`
- 集合类型
- 请求和 SQL 包装类型，例如 `Json<T>`

### 9.2 错误类型

错误必须是显式命名的类型，并且属于 API 设计的一部分。

Endpoint 错误应能够通过语言规则或标准库规则稳定映射为 HTTP 响应。

### 9.3 资源分类

从设计角度，值可以先分成三类：

1. 普通业务值
2. 受作用域约束的资源值
3. 可安全共享的服务值

这允许编译器在不同类别上应用不同的移动、共享和逃逸规则，而不必让所有应用代码都暴露成 Rust 式借用语法。

## 10. v0.1 范围

第一版的目标是可靠地支撑小型到中型 HTTP API 服务开发。

### 10.1 包含内容

- 模块系统
- endpoint 声明
- route 声明
- datasource 声明
- record 和 enum
- 显式错误类型
- `Option` 和 `Result`
- async endpoint
- JSON 请求与响应处理
- 请求上下文
- 数据源接入
- 标准库事务 API
- SQL 绑定与结果映射检查
- 单元测试
- endpoint 集成测试
- 编译器 CLI

### 10.2 不包含内容

- 宏系统
- 复杂泛型抽象体系
- 大型 trait 或 typeclass 系统
- ORM
- GraphQL 和 gRPC 一等支持
- 插件生态
- actor runtime
- 热重载
- 注解式框架元编程

## 11. 编译器架构

Yan 将使用 Rust 实现。

推荐的编译流程如下：

1. lexer
2. parser
3. resolver
4. type checker
5. backend semantic checker
6. lowering
7. code generation
8. linking/runtime integration

### 11.1 IR 分层

- `AST`：面向源码结构的语法树，用于解析和诊断
- `HIR`：去语法糖后的高层中间表示，便于类型与语义检查
- `MIR`：更接近执行模型的中层表示
- `LLVM IR`：用于优化和目标平台代码生成

### 11.2 选择 LLVM 的原因

v0.1 推荐走 LLVM 路线，原因是：

- 可以显著降低机器码后端实现复杂度
- 让早期工作重点放在语言前端语义
- Rust 生态下落地较现实

## 12. Runtime 策略

Yan 可以有 runtime，但 runtime 必须刻意保持小型化。

Runtime 只应承载必要基础设施：

- async 执行
- HTTP server 适配
- 数据库连接池与事务桥接
- JSON 编解码
- 日志与配置接入点

Runtime 不应反过来成为真正的语言本体。

语言语义必须主要存在于编译器和标准模型中，而不是隐藏在框架魔法里。

## 13. 工具链规划

初始 CLI 至少应包含：

- `yan new`
- `yan build`
- `yan run`
- `yan test`
- `yan check`
- `yan fmt`

后续可选扩展：

- `yan doc`
- `yan schema`
- `yan migrate`

## 14. 建议的仓库结构

```text
yanc/          # 编译器 CLI
yan-parser/    # 词法与语法分析
yan-hir/       # HIR 定义
yan-typeck/    # 类型与语义检查
yan-backend/   # 后端领域规则检查
yan-ir/        # MIR / 共享 IR
yan-codegen/   # LLVM lowering 与代码生成
yan-runtime/   # 最小 runtime
yan-std/       # 标准库
```

实现上可以从一个 Rust workspace 多 crate 结构开始。

## 15. 设计取舍

Yan 明确选择：

- 以后端优先语义替代最大化通用性
- 以原生 SQL 替代 ORM 抽象
- 以编译期服务结构检查替代框架约定
- 以显式错误处理替代隐藏异常
- 以小 runtime 替代重魔法平台行为

这条路线的主要风险在于：语言可能过度领域化，削弱一般编程体验。

为控制这个风险，Yan 应保留普通函数、普通数据结构和可读控制流，不让所有代码都退化成 DSL。

## 16. v0.1 成功标准

如果 Yan v0.1 能证明以下几点，则说明方向成立：

- 可以端到端构建一个小型 API 服务
- endpoint 定义比等价 Rust Web 代码更短、更清晰
- SQL 书写体验自然
- 常见后端错误能在编译期被拒绝
- 生成的可执行文件不依赖庞大框架 runtime
- 诊断信息足够清晰，不会因为高抽象而难以理解

## 17. 下一步

这份设计之后的直接下一步，是编写 v0.1 的实现计划。

该计划应至少覆盖：

- 仓库初始化
- 编译器 crate 边界
- 最小语法设计
- 第一版 AST/HIR 定义
- 初始 endpoint 模型
- 初始 SQL 块检查策略
- 第一个可运行 hello-world API 里程碑
