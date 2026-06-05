# Yan 语法待定清单

日期：2026-06-05  
状态：待评审草案

## 1. 目的

本文档用于整理 Yan 当前设计中尚未定稿的表面语法，尤其是那些已经在示例代码中出现、但仍然属于候选形式的语法点。

这些语法的共同特点是：

- 已经承载了明确语义
- 是否应该做成关键字、内建表达式或标准库接口，仍未最终确定
- 会直接影响语言的可读性、一致性和编译器实现复杂度

本文档只讨论“表面语法如何表达”，不推翻已在主设计文档中确定的核心语义方向。

## 2. 当前待定项总览

当前最主要的语法待定项包括：

- `validate`
- `ok`
- `fail`
- `app` 中引入 `datasource` 的方式
- `body: Json<T>` 这类请求提取写法
- `route ... -> handler` 的声明形式

其中优先级最高的是：

1. `validate`
2. `ok`
3. `fail`

因为这三者直接决定了业务逻辑和错误控制流的日常书写风格。

## 3. `validate` 待定项

### 3.1 当前语义

示例中的：

```yan
validate input.email as Email else CreateUserError.InvalidEmail
```

表达的语义是：

- 将 `input.email` 按 `Email` 类型约束进行校验或转换
- 如果成功则继续执行
- 如果失败则立即进入指定错误路径

### 3.2 核心问题

`validate` 是否应该是语言关键字？

### 3.3 候选方案

#### 方案 A：关键字语句

```yan
validate input.email as Email else CreateUserError.InvalidEmail
```

优点：

- 最接近自然语言
- 非常适合后端“输入校验”场景
- 读代码时能立刻看出这是一个校验点

缺点：

- 更像专用语句，组合性较弱
- 如果校验后需要拿到更强类型的新值，语义会略显别扭

#### 方案 B：内建表达式

```yan
let email = check input.email as Email else CreateUserError.InvalidEmail
```

优点：

- 校验和类型提升一步完成
- 更符合“表达式语言”的统一风格
- 后续可以扩展到更多受约束类型转换场景

缺点：

- 比 `validate` 略抽象
- 需要额外确定 `check` 是否为关键字

#### 方案 C：标准库接口

```yan
let email = Email.parse(input.email)?
```

或：

```yan
let email = Email.try_from(input.email)?
```

优点：

- 语言核心更小
- 形式上更接近普通类型系统扩展

缺点：

- 会退回传统库式写法
- 不利于形成 Yan 在后端校验上的语言辨识度

### 3.4 推荐

推荐采用方案 B，即“内建表达式”路线：

```yan
let email = check input.email as Email else CreateUserError.InvalidEmail
```

推荐理由：

- 比 `validate` 更适合产生一个新的强类型值
- 比标准库函数更有语言级表达力
- 更利于编译器把校验结果纳入控制流分析和类型收窄

## 4. `ok` 待定项

### 4.1 当前语义

示例中的：

```yan
ok UserView.from(inserted)
```

表达的语义是：

- 构造一个成功结果
- 在需要时等价于 `Result::Ok(...)`

### 4.2 核心问题

成功路径是否需要专门语法？

### 4.3 候选方案

#### 方案 A：保留 `ok` 关键字

```yan
ok UserView.from(inserted)
```

优点：

- 成功/失败路径视觉上非常明确
- 与 `fail` 能形成对称结构
- endpoint 场景中读起来很直接

缺点：

- 新增语言保留字
- 与普通表达式返回风格不完全一致

#### 方案 B：使用标准构造

```yan
Ok(UserView.from(inserted))
```

优点：

- 简单直接
- 接近很多强类型语言已有习惯
- 降低语法设计成本

缺点：

- 风格更像库，而不是语言
- 与 Yan 想强调的后端语义可能不够统一

#### 方案 C：直接 `return`

```yan
return UserView.from(inserted)
```

优点：

- 最简洁
- 日常函数风格统一

缺点：

- 如果返回类型是 `Result<T, E>`，这里会引入隐式包装还是要求显式包装，必须另定规则
- 会削弱成功/失败路径的显式程度

### 4.4 推荐

推荐优先比较方案 A 和方案 B。

当前更偏向方案 B：

```yan
Ok(UserView.from(inserted))
```

推荐理由：

- 能减少语言关键字数量
- 仍然保持成功路径显式
- 对编译器和用户心智都更稳定

如果后续确定 Yan 要更强调“控制流语法的一致审美”，再考虑保留 `ok`。

## 5. `fail` 待定项

### 5.1 当前语义

示例中的：

```yan
fail CreateUserError.EmailTaken
```

表达的语义是：

- 立即结束当前流程
- 返回一个失败结果

### 5.2 候选方案

#### 方案 A：保留 `fail` 关键字

```yan
fail CreateUserError.EmailTaken
```

优点：

- 与 `ok` 对称
- 业务错误路径可读性很强

缺点：

- 再增加一个语言关键字
- 与普通 `Result` 生态可能出现双表达体系

#### 方案 B：使用标准构造返回

```yan
return Err(CreateUserError.EmailTaken)
```

优点：

- 形式稳定
- 和 `Result` 模型自然一致

缺点：

- 语义略长
- 后端业务代码里频繁书写时不如 `fail` 紧凑

#### 方案 C：依赖 `?` 与转换

```yan
Err(CreateUserError.EmailTaken)?
```

优点：

- 与错误上传风格一致

缺点：

- 可读性差
- 不适合作为显式业务失败主写法

### 5.3 推荐

推荐采用方案 B：

```yan
return Err(CreateUserError.EmailTaken)
```

推荐理由：

- 和 `Result<T, E>` 模型一致
- 不必额外引入 `fail` 关键字
- 比较符合强类型编译语言的常见直觉

## 6. `app` 中引入 `datasource` 的写法

### 6.1 当前语义

示例中的：

```yan
app api {
  use user
  use main_db
}
```

表达的语义是：

- 将顶层定义的数据源引入当前应用
- 使 endpoint 可以通过 `ctx.main_db` 访问该数据源
- 不再单独引入 `bind` 之类的资源别名语法

### 6.2 候选方案

#### 方案 A：直接 `use` 数据源，推荐

```yan
app api {
  use user
  use main_db
}
```

优点：

- 写法简单
- 和 `app use module` 风格统一
- `ctx.main_db` 的来源清晰

缺点：

- endpoint 中访问名和顶层数据源名绑定较紧

#### 方案 B：保留资源别名写法

```yan
app api {
  use user
  bind db = main_db
}
```

优点：

- 可以在应用层重命名资源

缺点：

- 需要额外语法
- `ctx.db` 的来源不如 `ctx.main_db` 直观

### 6.3 推荐

推荐采用方案 A。

当前已确认：`app` 通过 `use main_db` 引入数据源，endpoint 通过 `ctx.main_db` 访问数据源。

## 7. SQL 调用形态待定项

### 7.1 当前语义

示例中的：

```yan
let row = ctx.main_db.query_one[UserRow](
  "
  SELECT ...
  ",
  [id],
)?
```

表达的语义是：

- 执行原生 SQL
- 声明目标映射类型
- 声明执行上下文
- 通过不同标准库 API 区分结果基数

### 7.2 核心问题

结果基数应通过 API 名称表达，还是通过类型推断表达？

### 7.3 候选方案

#### 方案 A：通过标准库 API 显式表达，推荐

```yan
let row = ctx.main_db.query_one[UserRow](sql, [id])?
let row = ctx.main_db.query_optional[UserRow](sql, [id])?
let rows = ctx.main_db.query_many[UserRow](sql, [active])?
```

优点：

- 结果预期非常清晰
- 不需要为 SQL 本身引入专用语法
- 与“SQL 不做语言关键字”方向一致

缺点：

- API 命名需要尽早定稿

#### 方案 B：通过目标类型推断

```yan
let row: UserRow = ctx.main_db.query(sql, [id])?
let row: Option<UserRow> = ctx.main_db.query(sql, [id])?
let rows: Vec<UserRow> = ctx.main_db.query(sql, [active])?
```

优点：

- 语法更统一

缺点：

- 结果预期不够显眼
- SQL 本身的阅读点和外部类型提示被分散

### 7.4 推荐

推荐保留方案 A。

对后端开发来说，查询结果基数是重要语义，不应完全隐藏到类型推断里。

## 8. 请求提取写法待定项

### 8.1 当前形式

```yan
endpoint create_user(ctx: RequestCtx, body: Json<CreateUserInput>) -> Result<UserView, CreateUserError>
```

### 8.2 核心问题

请求来源是否应通过参数类型隐式表达，还是增加参数修饰语法？

### 8.3 候选方案

#### 方案 A：依赖包装类型

```yan
body: Json<CreateUserInput>
```

优点：

- 简单直接
- 编译器容易实现

缺点：

- 需要多个包装类型共同表达 path/query/header/body

#### 方案 B：增加参数来源修饰

```yan
body input: CreateUserInput
query filter: UserFilter
path id: UserId
```

优点：

- 来源一眼可见
- 更符合“后端语义在语言表面可见”的目标

缺点：

- 参数语法会变得更特殊

### 8.4 推荐

推荐后续重点评估方案 B。

如果 Yan 要强调“接口定义非常清晰”，那么参数来源显式化会优于完全依赖包装类型。

## 9. `route ... -> handler` 待定项

### 9.1 当前形式

```yan
route GET "/users/:id" -> get_user
```

### 9.2 候选方案

#### 方案 A：保留当前声明式绑定

```yan
route GET "/users/:id" -> get_user
```

优点：

- 路由表和处理逻辑解耦
- 模块级视图清晰

缺点：

- route 和 endpoint 分成两处，存在跳转

#### 方案 B：把路由直接写入 endpoint 头部

```yan
endpoint GET "/users/:id" get_user(ctx: RequestCtx, id: UserId) -> Result<UserView, HttpError> {
  ...
}
```

优点：

- 定义聚合在一处
- 对小型服务更直观

缺点：

- 模块级总览稍弱

### 9.3 推荐

推荐优先比较方案 A 与方案 B，暂不定稿。

如果目标更偏“模块可扫描性”，保留 A；如果目标更偏“单接口紧凑表达”，倾向 B。

## 10. 当前推荐结论

基于现阶段设计，推荐方向如下：

- `validate`：不保留原样，优先转向 `check ... else ...` 内建表达式
- `ok`：优先改为 `Ok(...)`
- `fail`：优先改为 `return Err(...)`
- `app` 与 `datasource`：采用 `use main_db` 直接引入
- SQL 调用：优先采用 `query_one/query_optional/query_many` 这类标准库 API
- 请求提取：继续评估是否改成显式来源修饰语法
- `route` 声明：继续评估是否与 `endpoint` 头部合并

## 11. 建议的下一步

建议在下一轮设计中，专门收敛以下两个问题：

1. 错误控制流语法是否尽量少关键字化  
2. endpoint 参数提取语法是否要进一步显式化

这两个决定会直接影响 Yan 的整体语言气质：

- 是更偏“语义内建但语法克制”
- 还是更偏“后端概念直接暴露在语法表面”
