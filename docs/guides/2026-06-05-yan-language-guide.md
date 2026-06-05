# Yan 语言指南

日期：2026-06-05  
状态：语言设计阶段草案

## 1. 这份文档的用途

这是一份面向语言使用者的 Yan 入门指南。

它的目标不是解释编译器如何实现，而是先回答这些问题：

- Yan 是一门什么样的语言
- Yan 用来解决什么问题
- Yan 的程序如何组织
- Yan 的基础语法如何书写
- Yan 如何定义接口
- Yan 如何接入数据源
- Yan 如何处理错误、分支、循环和业务逻辑

这份文档应当作为后续语言设计收敛的参考文本。  
在语言层面没有完全定稿之前，所有示例都应理解为“推荐方向”，而不是最终标准。

## 2. Yan 是什么

Yan 是一门面向后端开发的编译型强类型语言。

Yan 的目标不是成为通用系统底层语言，而是让开发者更快地完成：

- HTTP API 开发
- 业务逻辑编写
- 原生 SQL 驱动的数据访问
- 多数据源接入
- 显式错误处理
- 安全并发的服务程序开发

Yan 的设计目标可以概括为：

> 像现代后端框架一样高效，但把大量框架约定前移到语言层做静态约束。

## 3. Yan 的基本设计原则

Yan 当前遵循以下原则：

- 编译型语言，产出本地可执行文件
- 强静态类型
- 默认非空
- 错误显式，不依赖异常作为主控制流
- 后端领域概念优先，例如 `app`、`module`、`endpoint`、`datasource`
- 保留普通编程能力，不把整门语言做成纯 DSL
- 原生 SQL 是主要数据访问方式之一，但 SQL 本身不提升为语言关键字
- 事务不是语言关键字，而是标准库提供的能力

## 4. 一个 Yan 程序长什么样

Yan 程序围绕几个核心顶层构造组织：

- `app`
- `module`
- `endpoint`
- `record`
- `error`
- `datasource`
- `fn`

一个最小程序的大致结构如下：

```yan
datasource main_db: Postgres {
  url: env("DATABASE_URL")
  pool_size: 32
  timeout_ms: 3000
}

app api {
  use user
  use main_db

  listen ":8080"
}

module user {
  route GET "/users/:id" -> get_user
}

record UserView {
  id: UserId
  name: Str
  email: Str
}

error UserError {
  NotFound
  InvalidId
}

endpoint get_user(ctx: RequestCtx, id: UserId) -> Result<UserView, UserError> {
  let row = ctx.main_db.query_one[UserView](
    "
    SELECT id, name, email
    FROM users
    WHERE id = $1
    ",
    [id],
  )?

  Ok(row)
}
```

这段代码体现了 Yan 的核心思路：

- 数据源是语言一等公民
- 应用通过 `app` 装配模块和资源
- 模块通过 `route` 暴露接口
- `endpoint` 是接口处理函数
- 数据访问依赖标准库，不依赖 ORM
- 错误通过 `Result` 显式表达

## 5. 顶层构造说明

### 5.1 `app`

`app` 表示应用入口与装配边界。

它负责：

- 装载模块
- 引入数据源或其他共享资源
- 配置监听地址
- 描述应用的运行入口

示例：

```yan
app api {
  use user
  use auth
  use main_db
  use main_cache

  listen ":8080"
}
```

### 5.2 `module`

`module` 表示业务模块边界。

它的目标是：

- 按业务拆分接口
- 让路由组织更清晰
- 避免所有 endpoint 堆在一起

示例：

```yan
module user {
  route GET "/users/:id" -> get_user
  route POST "/users" -> create_user
}
```

### 5.3 `endpoint`

`endpoint` 是对外接口处理单元。

它和普通函数不同，因为它天然带有：

- 请求上下文
- 输入提取
- 输出契约
- 错误边界

示例：

```yan
endpoint get_user(ctx: RequestCtx, id: UserId) -> Result<UserView, UserError> {
  ...
}
```

### 5.4 `record`

`record` 是主要数据结构，用来定义请求、响应、业务对象、配置结构等。

示例：

```yan
record CreateUserInput {
  name: Str
  email: Str
}
```

### 5.5 `error`

`error` 用来显式定义错误类型。

示例：

```yan
error CreateUserError {
  InvalidEmail
  EmailTaken
}
```

### 5.6 `datasource`

`datasource` 是 Yan 的一等公民，用于声明应用所使用的数据源。

它与普通对象不同，因为它代表一类受语言理解的外部资源。

示例：

```yan
datasource main_db: Postgres {
  url: env("DATABASE_URL")
  pool_size: 32
  timeout_ms: 3000
}
```

目前推荐的方向是：

- `datasource` 是顶层声明
- `app` 通过 `use main_db` 这类写法引入可用数据源
- 常见类型如 `Postgres`、`MySql`、`Redis` 由标准库内置
- 事务、查询、连接池行为通过标准库 API 暴露
- SQL 和事务不作为语言关键字存在

## 6. 变量、函数和类型

Yan 保留常规编程语言的基本结构，不在这些部分做过度创新。

### 6.1 变量绑定

```yan
let name = "yan"
let count = 1
let mut total = 0
```

建议规则：

- `let` 用于普通绑定
- `let mut` 用于可变绑定
- 默认不可变

### 6.2 函数

```yan
fn add(a: Int, b: Int) -> Int {
  a + b
}
```

建议规则：

- 参数采用 `name: Type`
- 返回类型采用 `->`
- 不要求以分号作为所有语句结尾

### 6.3 基础类型

当前建议 Yan 至少支持：

- `Int`
- `Bool`
- `Str`
- `Float`
- `Option<T>`
- `Result<T, E>`
- `List<T>`
- `Map<K, V>`

后续可以继续扩展更细的标准类型，例如：

- `Email`
- `Url`
- `Uuid`
- `DateTime`

## 7. 分支与模式匹配

Yan 不应在基础控制流上做过度发明，优先采用熟悉且清晰的形式。

### 7.1 `if / else`

```yan
if user.is_admin {
  allow()
} else {
  deny()
}
```

### 7.2 `match`

```yan
match result {
  Ok(user) => render(user)
  Err(err) => handle(err)
}
```

`match` 的主要用途包括：

- 匹配 `Result`
- 匹配 `Option`
- 匹配业务枚举
- 清晰表达多分支逻辑

## 8. 循环

Yan 建议保留三类常规循环结构。

### 8.1 `while`

```yan
while retry < 3 {
  retry = retry + 1
}
```

### 8.2 `for`

```yan
for user in users {
  io.println(user.name)
}
```

### 8.3 `loop`

```yan
loop {
  if done {
    break
  }
}
```

对 Yan 来说，接口开发和业务逻辑是重点，循环语法应尽量常规，不必刻意追求新颖。

## 9. 错误处理

Yan 的主错误模型是：

- 使用 `Result<T, E>`
- 使用 `?` 上传错误
- 使用 `match` 显式处理错误
- 不把异常作为默认控制流

### 9.1 定义错误

```yan
error UserError {
  NotFound
  InvalidEmail
}
```

### 9.2 返回错误

```yan
fn validate_name(name: Str) -> Result<Str, UserError> {
  if name == "" {
    return Err(UserError.InvalidEmail)
  }

  Ok(name)
}
```

### 9.3 错误上传

```yan
fn load_user(id: UserId, db: DbConn) -> Result<UserView, UserError> {
  let row = db.query_one[UserView](
    "
    SELECT id, name, email
    FROM users
    WHERE id = $1
    ",
    [id],
  )?

  Ok(row)
}
```

### 9.4 错误捕获

```yan
match load_user(id, db) {
  Ok(user) => Ok(user)
  Err(UserError.NotFound) => Err(HttpError.NotFound)
  Err(err) => Err(HttpError.Internal(err))
}
```

当前建议是：Yan 首版不提供 `try/catch` 作为主模型，而是用 `Result + ? + match` 解决绝大多数错误控制流。

## 10. 请求处理

Yan 的重点之一是让接口定义更清晰。

当前推荐的 endpoint 形式如下：

```yan
endpoint create_user(ctx: RequestCtx, body: Json<CreateUserInput>) -> Result<UserView, CreateUserError> {
  ...
}
```

这里表达了几层意思：

- `ctx` 是请求上下文
- `body` 来自请求体
- `CreateUserInput` 是强类型输入模型
- 返回值是显式 `Result`

后续有两条可能的语法方向：

1. 继续沿用包装类型，例如 `Json<T>`
2. 改成更显式的参数来源标记，例如 `body input: CreateUserInput`

这部分仍在设计中，但无论采用哪一种，目标都不变：

- 接口输入来源明确
- 空值和错误显式
- 参数解析尽量自动化

## 11. 数据源接入

Yan 的数据源接入原则是：

- 数据源在语言层声明
- 数据库操作通过标准库完成
- SQL 和事务不作为关键字
- 编译器关注类型边界和资源边界

### 11.1 声明数据源

```yan
datasource main_db: Postgres {
  url: env("DATABASE_URL")
  pool_size: 32
  timeout_ms: 3000
}
```

也可以有其他内置数据源类型：

```yan
datasource cache: Redis {
  url: env("REDIS_URL")
  pool_size: 16
}
```

### 11.2 在 `app` 中引入

```yan
app api {
  use user
  use main_db
  use cache

  listen ":8080"
}
```

这里的规则是：

- `main_db` 是顶层定义的数据源名
- `app` 通过 `use main_db` 将其纳入当前应用
- endpoint 中通过 `ctx.main_db` 访问该数据源
- 不再单独引入 `bind` 这类资源别名语法

### 11.3 在 endpoint 中使用

```yan
endpoint get_user(ctx: RequestCtx, id: UserId) -> Result<UserView, UserError> {
  let row = ctx.main_db.query_one[UserRow](
    "
    SELECT id, name, email
    FROM users
    WHERE id = $1
    ",
    [id],
  )?

  Ok(UserView.from(row))
}
```

### 11.4 事务处理

事务不是语言关键字，推荐通过标准库 API 表达：

```yan
endpoint create_user(ctx: RequestCtx, body: Json<CreateUserInput>) -> Result<UserView, CreateUserError> {
  let input = body.value

  ctx.main_db.transaction(fn(tx) {
    let row = tx.query_one[UserRow](
      "
      INSERT INTO users(name, email)
      VALUES ($1, $2)
      RETURNING id, name, email
      ",
      [input.name, input.email],
    )?

    Ok(UserView.from(row))
  })
}
```

这套设计的重点是：

- `datasource` 进入语言模型
- 事务行为仍保持普通 API 风格
- SQL 仍然是原生 SQL 字符串或原生 SQL 输入形式

## 12. SQL 使用方式

Yan 当前不建议为 SQL 发明大型 DSL。

推荐方向是：

- 使用原生 SQL
- 通过标准库提供 `query_one`、`query_optional`、`query_many`、`execute` 等接口
- 编译器后续可针对这些标准接口做更强的静态检查

示例：

```yan
let users = ctx.main_db.query_many[UserRow](
  "
  SELECT id, name, email
  FROM users
  WHERE active = $1
  ",
  [true],
)?
```

## 13. 普通业务函数

Yan 不应让所有逻辑都堆在 endpoint 中。

业务逻辑仍然应该通过普通 `fn` 抽离：

```yan
fn ensure_name(name: Str) -> Result<Str, CreateUserError> {
  if name == "" {
    return Err(CreateUserError.InvalidName)
  }

  Ok(name)
}
```

然后在 endpoint 中调用：

```yan
endpoint create_user(ctx: RequestCtx, body: Json<CreateUserInput>) -> Result<UserView, CreateUserError> {
  let input = body.value
  let name = ensure_name(input.name)?

  ...
}
```

Yan 的方向不是让 endpoint 替代函数，而是让 endpoint 成为“接口层”，函数继续承担业务逻辑组织职责。

## 14. 一段完整示例

```yan
datasource main_db: Postgres {
  url: env("DATABASE_URL")
  pool_size: 32
}

app api {
  use user
  use main_db

  listen ":8080"
}

module user {
  route GET "/users/:id" -> get_user
  route POST "/users" -> create_user
}

record UserRow {
  id: UserId
  name: Str
  email: Str
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
  InvalidName
}

fn ensure_name(name: Str) -> Result<Str, CreateUserError> {
  if name == "" {
    return Err(CreateUserError.InvalidName)
  }

  Ok(name)
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

  Ok(UserView.from(row))
}

endpoint create_user(ctx: RequestCtx, body: Json<CreateUserInput>) -> Result<UserView, CreateUserError> {
  let input = body.value
  let name = ensure_name(input.name)?

  ctx.main_db.transaction(fn(tx) {
    let row = tx.query_one[UserRow](
      "
      INSERT INTO users(name, email)
      VALUES ($1, $2)
      RETURNING id, name, email
      ",
      [name, input.email],
    )?

    Ok(UserView.from(row))
  })
}
```

## 15. 当前仍未定稿的部分

虽然这份 guide 以“如何使用”来组织，但以下内容仍然没有完全定稿：

- endpoint 参数来源语法是否继续使用 `Json<T>` 这类包装类型
- `route` 是否继续和 `endpoint` 分离
- `Ok(...)` / `Err(...)` 是否保持当前形式，还是进一步包装
- 用户自定义 `datasource` 类型如何声明
- 数据库 API 是否最终采用 `query_one/query_many` 这类命名

因此，这份文档更适合作为“当前推荐用法说明”，而不是最终语言规范。

## 16. 建议的下一步

如果你要先把语言层面完全定下来，再开始做编译器，那么下一步建议按这个顺序继续：

1. 先定 Yan 的最小语法规范
2. 再定 endpoint 参数提取语法
3. 再定 datasource 的扩展模型
4. 再定标准库数据库 API 形态
5. 最后才进入编译器分层设计

这样做的好处是：编译器实现会服务于语言，而不是反过来绑架语言设计。
