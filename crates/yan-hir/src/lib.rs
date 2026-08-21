//! 与 parser 和执行后端解耦的 Yan 高层中间表示。

use std::collections::{HashMap, HashSet};

use yan_source::{SourceId, SourceLocation, Span};
use yan_syntax::{
    Enum as SyntaxEnum, Expression as SyntaxExpression, Field as SyntaxField,
    MapEntry as SyntaxMapEntry, MatchArm as SyntaxMatchArm, Statement as SyntaxStatement,
    SyntaxProgram, TypeSyntax,
};

/// 模块在单次编译会话内的稳定标识。
///
/// 该标识只用于连接 HIR、Typed HIR 与 MIR，不能写入缓存或暴露为用户可见名称。
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ModuleId(pub u32);

/// 顶层声明在单次编译会话内的稳定标识。
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DefId(pub u32);

/// 函数局部绑定在所属函数内的稳定标识。
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LocalId(pub u32);

/// 结构体字段在单次编译会话内的稳定标识。
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FieldId(pub u32);

/// 枚举变体在单次编译会话内的稳定标识。
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct VariantId(pub u32);

/// 已降低为编译器语义阶段使用的程序。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Program {
    /// 当前 HIR 模块所属的编译会话源文件。
    ///
    /// 所有由该模块产生的诊断均以此 ID 与其文件内 [`Span`] 组成完整位置。
    pub source: SourceId,
    /// 当前 HIR 模块的稳定标识。
    pub id: ModuleId,
    /// 源文件声明的模块路径；M3 允许省略。
    pub module: Option<Vec<String>>,
    /// 显式导入的模块路径。
    pub imports: Vec<Vec<String>>,
    /// 与 `imports` 同序的精确源码位置，用于导入解析诊断。
    pub import_locations: Vec<SourceLocation>,
    /// 源文件中的真正新类型声明。
    pub newtypes: Vec<Newtype>,
    /// 源文件中的结构体声明。
    pub structs: Vec<Struct>,
    /// 源文件中的封闭枚举声明。
    pub enums: Vec<Enum>,
    /// 程序定义的函数。
    pub functions: Vec<Function>,
}

/// 参与一次模块图解析的单个已 lowering 模块。
///
/// `id` 由编译会话分配，`program.id` 会在解析开始时同步为该值。模块图不读取文件，
/// 因而文件系统、导入路径校验和循环导入诊断仍由 CLI 编排层负责。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleInput {
    /// 当前模块在本次编译会话内的稳定标识。
    pub id: ModuleId,
    /// 已从对应源文件 lowering 的 HIR 模块。
    pub program: Program,
}

impl ModuleInput {
    /// 使用编译会话分配的模块 ID 创建模块图输入。
    pub const fn new(id: ModuleId, program: Program) -> Self {
        Self { id, program }
    }
}

/// 由编排层收集、由 HIR 负责解析语义引用的模块图输入。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleGraph {
    /// 图内全部模块；其顺序决定稳定的全局声明 ID 分配顺序。
    pub modules: Vec<ModuleInput>,
    /// 用户请求检查或执行的入口模块。
    pub entry: ModuleId,
}

impl ModuleGraph {
    /// 创建一个不访问文件系统的模块图。
    pub const fn new(modules: Vec<ModuleInput>, entry: ModuleId) -> Self {
        Self { modules, entry }
    }
}

/// 已完成全局声明 ID 分配和跨模块语义引用解析的程序集合。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedProgram {
    /// 每个模块各自保留的已解析 HIR；不会由 CLI 拼接声明后再解析引用。
    pub modules: Vec<Program>,
    /// 编译会话请求的入口模块。
    pub entry: ModuleId,
}

impl ResolvedProgram {
    /// 为现有单程序类型检查阶段提供入口模块可见声明的视图。
    ///
    /// 此兼容视图只复制入口模块直接或传递导入的公开声明；所有引用早已在模块图内
    /// 解析为会话 ID，CLI 不参与声明筛选或名称解析。
    pub fn entry_program(&self) -> Result<Program, ResolveError> {
        let module_indices = module_indices(&self.modules)?;
        let entry_index = module_indices
            .get(&self.entry)
            .copied()
            .ok_or_else(|| ResolveError {
                location: SourceLocation::new(SourceId(0), Span::default()),
                message: "module graph does not contain the entry module".to_owned(),
            })?;
        let mut entry = self.modules[entry_index].clone();
        let platform_imports = entry
            .imports
            .into_iter()
            .zip(entry.import_locations)
            .filter(|(path, _)| is_platform_import(path))
            .collect::<Vec<_>>();
        (entry.imports, entry.import_locations) = platform_imports.into_iter().unzip();
        let mut appended = HashSet::new();
        let mut visited = HashSet::new();
        append_imported_declarations(
            entry_index,
            &self.modules,
            &module_indices,
            &mut entry,
            &mut appended,
            &mut visited,
        )?;
        Ok(entry)
    }
}

/// 模块图语义解析失败时的稳定 Yan 诊断数据。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolveError {
    /// 失败位置；模块图输入不携带 import token 时使用模块起始位置。
    pub location: SourceLocation,
    /// 面向用户的稳定英文错误文本。
    pub message: String,
}

/// 分配会话全局声明 ID，并解析模块内和已导入公开声明的所有语义目标。
///
/// 该入口不读取文件、不决定模块路径和不渲染诊断。调用方必须先完成文件读取、模块
/// 路径校验和循环导入检测，再将完整模块集合交给 HIR。
pub fn resolve_modules(graph: ModuleGraph) -> Result<ResolvedProgram, ResolveError> {
    let mut modules = graph
        .modules
        .into_iter()
        .map(|input| {
            let mut program = input.program;
            program.id = input.id;
            program
        })
        .collect::<Vec<_>>();
    let module_indices = module_indices(&modules)?;
    if !module_indices.contains_key(&graph.entry) {
        return Err(ResolveError {
            location: SourceLocation::new(SourceId(0), Span::default()),
            message: "module graph does not contain the entry module".to_owned(),
        });
    }

    assign_global_ids(&mut modules);
    for index in 0..modules.len() {
        let symbols = visible_symbols(index, &modules, &module_indices)?;
        resolve_references_with_symbols(&mut modules[index], &symbols);
    }
    Ok(ResolvedProgram {
        modules,
        entry: graph.entry,
    })
}

/// 已 lowering 的真正新类型声明。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Newtype {
    /// 声明所属的编译会话源文件。
    pub source: SourceId,
    /// 此顶层声明的稳定标识。
    pub id: DefId,
    /// 是否允许其他模块显式导入该声明。
    pub public: bool,
    /// 新类型名称。
    pub name: String,
    /// 新类型名称的位置。
    pub name_span: Span,
    /// 新类型包装的底层类型。
    pub underlying: Type,
}

/// 已 lowering 的具名结构体声明。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Struct {
    /// 声明所属的编译会话源文件。
    pub source: SourceId,
    /// 此顶层声明的稳定标识。
    pub id: DefId,
    /// 是否允许其他模块显式导入该声明。
    pub public: bool,
    /// 结构体名称。
    pub name: String,
    /// 结构体名称的位置。
    pub name_span: Span,
    /// 按声明顺序排列的字段。
    pub fields: Vec<Field>,
}

/// 已 lowering 的封闭枚举声明。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Enum {
    /// 声明所属的编译会话源文件。
    pub source: SourceId,
    /// 此顶层声明的稳定标识。
    pub id: DefId,
    /// 是否允许其他模块显式导入该声明。
    pub public: bool,
    /// 枚举名称。
    pub name: String,
    /// 枚举名称在源文件中的位置。
    pub name_span: Span,
    /// 按声明顺序排列的变体。
    pub variants: Vec<EnumVariant>,
}

/// 已 lowering 的枚举变体。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnumVariant {
    /// 变体声明所属的编译会话源文件。
    pub source: SourceId,
    /// 此枚举变体的稳定标识。
    pub id: VariantId,
    /// 变体名称。
    pub name: String,
    /// 变体名称在源文件中的位置。
    pub name_span: Span,
    /// 可选的单个具名载荷。
    pub payload: Option<EnumPayload>,
}

/// 枚举单载荷变体的名称与类型。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnumPayload {
    /// 载荷名称。
    pub name: String,
    /// 载荷名称在源文件中的位置。
    pub name_span: Span,
    /// 载荷类型。
    pub ty: Type,
}

/// 已 lowering 的结构体字段。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Field {
    /// 字段声明所属的编译会话源文件。
    pub source: SourceId,
    /// 此字段的稳定标识。
    pub id: FieldId,
    /// 字段名称。
    pub name: String,
    /// 字段名称的位置。
    pub name_span: Span,
    /// 字段类型。
    pub ty: Type,
    /// 声明时给出的可选默认值。
    pub default: Option<Expression>,
}

/// M3 支持的函数定义。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Function {
    /// 函数定义所属的编译会话源文件。
    pub source: SourceId,
    /// 此顶层函数声明的稳定标识。
    pub id: DefId,
    /// 是否允许其他模块显式导入该声明。
    pub public: bool,
    /// 函数名称。
    pub name: String,
    /// 函数名称的位置。
    pub name_span: Span,
    /// 按声明顺序排列的具名参数。
    pub parameters: Vec<Parameter>,
    /// 函数的声明返回类型。
    pub return_type: Type,
    /// 函数体语句。
    pub statements: Vec<Statement>,
}

/// 已 lowering 的函数参数。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Parameter {
    /// 此参数在所属函数内的局部标识。
    pub id: LocalId,
    /// 参数名称。
    pub name: String,
    /// 参数名称的位置。
    pub name_span: Span,
    /// 参数的显式类型。
    pub ty: Type,
}

/// M3 支持的语言类型。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Type {
    /// 有符号 64 位整数。
    Int,
    /// IEEE 754 双精度浮点数。
    Float,
    /// 不可变字节序列。
    Bytes,
    /// 布尔值。
    Bool,
    /// UTF-8 文本。
    String,
    /// 无有效返回值。
    Unit,
    /// 元素类型统一的有序列表。
    List(Box<Type>),
    /// 键固定为 string、值类型统一的不可变 map。
    Map(Box<Type>),
    /// 内建可选值，包含一个 T 值或不包含任何值。
    Option(Box<Type>),
    Result(Box<Type>, Box<Type>),
    Tuple(Vec<Type>),
    Never,
    /// 由源文件声明的名义类型，包括新类型和结构体。
    Named(String),
}

/// HIR 语句。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Statement {
    Destructure {
        /// 解构产生的局部绑定标识，顺序与 names 一致。
        locals: Vec<LocalId>,
        names: Vec<(String, Span)>,
        value: Expression,
    },
    /// 声明局部变量。
    Let {
        /// 新局部绑定在所属函数内的稳定标识。
        local: LocalId,
        mutable: bool,
        name: String,
        name_span: Span,
        annotation: Option<Type>,
        value: Expression,
    },
    /// 重写已有变量的值。
    Assign {
        /// 已解析的被赋值局部绑定。
        local: LocalId,
        name: String,
        name_span: Span,
        value: Expression,
    },
    /// 为副作用执行表达式。
    Expression(Expression),
}

/// HIR 在名称解析阶段确定的调用语义目标。
///
/// `path` 仍保留在表达式中用于类型错误诊断，但类型检查与后续阶段只能消费本枚举，
/// 不得再次通过源码名称猜测函数、构造器或接收者。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CallTarget {
    /// 用户函数声明。
    Function(DefId),
    /// 真正新类型构造器。
    Newtype(DefId),
    /// 有载荷用户 enum 变体构造器。
    Variant(VariantId),
    /// `Some` 内建构造器。
    Some,
    /// `Ok` 内建构造器。
    Ok,
    /// `Err` 内建构造器。
    Err,
    /// `bytes.from_hex` 内建函数。
    BytesFromHex,
    /// `console.println` 平台函数。
    ConsolePrintln,
    /// `string.to_int`，携带已解析的接收者局部绑定。
    StringToInt(LocalId),
}

/// HIR 表达式。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Expression {
    /// 整数字面量。
    Integer {
        value: i64,
        span: Span,
    },
    /// 浮点数字面量。
    Float {
        value: String,
        span: Span,
    },
    /// 布尔字面量。
    Boolean {
        value: bool,
        span: Span,
    },
    /// 由文本和变量插值片段构成的字符串字面量。
    String {
        parts: Vec<StringPart>,
        span: Span,
    },
    /// 列表字面量。
    List {
        values: Vec<Expression>,
        span: Span,
    },
    /// 键为 string 的 map 字面量。
    Map {
        entries: Vec<MapEntry>,
        span: Span,
    },
    Tuple {
        values: Vec<Expression>,
        span: Span,
    },
    /// 对 enum 值进行穷尽匹配的表达式。
    Match {
        target: Box<Expression>,
        arms: Vec<MatchArm>,
        span: Span,
    },
    /// 根据 bool 条件选择一个局部语句块求值。
    If {
        condition: Box<Expression>,
        then_statements: Vec<Statement>,
        else_statements: Vec<Statement>,
        span: Span,
    },
    /// 遍历列表元素的副作用表达式，结果始终为 unit。
    For {
        /// 循环变量在循环体作用域内的稳定标识。
        local: LocalId,
        name: String,
        name_span: Span,
        iterable: Box<Expression>,
        statements: Vec<Statement>,
        span: Span,
    },
    Return {
        value: Box<Expression>,
        span: Span,
    },
    Try {
        value: Box<Expression>,
        span: Span,
    },
    /// 局部变量读取。
    Variable {
        /// 已解析的局部读取目标；`None` 等内建构造不使用此字段。
        local: Option<LocalId>,
        name: String,
        span: Span,
    },
    /// 平台或后续普通函数调用。
    Call {
        /// 已解析调用目标；未解析名称保留为空，由类型检查产生稳定用户诊断。
        target: Option<CallTarget>,
        path: Vec<String>,
        arguments: Vec<Expression>,
        span: Span,
    },
    /// 整数加法。
    Add {
        left: Box<Expression>,
        right: Box<Expression>,
        span: Span,
    },
    /// 整数乘法。
    Multiply {
        left: Box<Expression>,
        right: Box<Expression>,
        span: Span,
    },
    /// 同类型基础值相等比较。
    Equal {
        left: Box<Expression>,
        right: Box<Expression>,
        span: Span,
    },
    /// 具名结构体字面量。
    StructLiteral {
        /// 已解析的结构体声明。
        structure: DefId,
        name: String,
        name_span: Span,
        fields: Vec<StructFieldValue>,
        span: Span,
    },
    /// 结构体字段读取。
    FieldAccess {
        /// 结构体字段读取或无载荷 enum 构造的已解析字段目标。
        field_id: Option<FieldId>,
        /// 无载荷 enum 构造的已解析变体目标。
        variant: Option<VariantId>,
        target: Box<Expression>,
        field: String,
        field_span: Span,
        span: Span,
    },
}

impl Expression {
    /// 返回该表达式在源文件中的区间。
    pub const fn span(&self) -> Span {
        match self {
            Self::Integer { span, .. }
            | Self::Float { span, .. }
            | Self::Boolean { span, .. }
            | Self::String { span, .. }
            | Self::List { span, .. }
            | Self::Map { span, .. }
            | Self::Tuple { span, .. }
            | Self::Match { span, .. }
            | Self::If { span, .. }
            | Self::For { span, .. }
            | Self::Return { span, .. }
            | Self::Try { span, .. }
            | Self::Variable { span, .. }
            | Self::Call { span, .. }
            | Self::Add { span, .. }
            | Self::Multiply { span, .. }
            | Self::Equal { span, .. } => *span,
            Self::StructLiteral { span, .. } | Self::FieldAccess { span, .. } => *span,
        }
    }
}

/// HIR map 字面量中的一个字符串键值对。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MapEntry {
    /// 不含双引号的键文本。
    pub key: String,
    /// 键字符串字面量在源文件中的位置。
    pub key_span: Span,
    /// 与键关联的值表达式。
    pub value: Expression,
}

/// HIR match 分支中对 enum 变体的模式。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnumPattern {
    /// 已解析的 enum 变体目标；Option 和 Result 的内建模式不使用此字段。
    pub variant_id: Option<VariantId>,
    /// 分支载荷绑定在该分支作用域内的稳定标识。
    pub binding_local: Option<LocalId>,
    /// 枚举名称。
    pub enum_name: String,
    /// 枚举名称在源文件中的位置。
    pub enum_name_span: Span,
    /// 变体名称。
    pub variant: String,
    /// 变体名称在源文件中的位置。
    pub variant_span: Span,
    /// 有载荷变体在分支内使用的可选局部绑定。
    pub binding: Option<(String, Span)>,
}

/// HIR match 表达式的一个分支。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MatchArm {
    /// 分支匹配的 enum 变体模式。
    pub pattern: EnumPattern,
    /// 该分支被选中时求值的表达式。
    pub value: Expression,
}

/// 结构体字面量中的一个具名字段赋值。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StructFieldValue {
    /// 已解析的结构体字段声明。
    pub field_id: Option<FieldId>,
    /// 字段名称。
    pub name: String,
    /// 字段名称的位置。
    pub name_span: Span,
    /// 字段值表达式。
    pub value: Expression,
}

/// 字符串字面量的组成部分。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StringPart {
    /// 不需要运行时求值的普通文本。
    Text(String),
    /// `{name}` 形式的局部变量插值。
    Variable {
        /// 已解析的局部绑定；未声明名称保留为空，供类型检查产生用户诊断。
        local: Option<LocalId>,
        /// 仅用于诊断显示的源码名称，后续阶段不得重新按名称查找。
        name: String,
        /// 插值名称在源码中的位置。
        span: Span,
    },
}

/// 将 parser 产生的语法树转换为后续阶段稳定消费的 HIR。
pub fn lower(program: SyntaxProgram) -> Result<Program, LowerError> {
    lower_with_source(program, SourceId(0))
}

/// 将 parser 产生的语法树转换为带有所属源文件的稳定 HIR。
///
/// CLI 为每个已读取文件分配 [`SourceId`] 后必须调用本入口，使跨模块诊断能回到
/// 原始文件；内存测试可继续使用 [`lower`]，其默认源 ID 仅限单文件会话。
pub fn lower_with_source(program: SyntaxProgram, source: SourceId) -> Result<Program, LowerError> {
    lower_unlocated(program, source).map_err(|error| error.with_source(source))
}

fn lower_unlocated(program: SyntaxProgram, source: SourceId) -> Result<Program, LowerError> {
    let mut next_def = 0_u32;
    let mut next_field = 0_u32;
    let mut next_variant = 0_u32;
    let newtypes = program
        .newtypes
        .into_iter()
        .map(|newtype| {
            let id = DefId(next_def);
            next_def += 1;
            lower_newtype(newtype, id, source)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let structs = program
        .structs
        .into_iter()
        .map(|structure| {
            let id = DefId(next_def);
            next_def += 1;
            lower_struct(structure, id, source, &mut next_field)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let enums = program
        .enums
        .into_iter()
        .map(|enumeration| {
            let id = DefId(next_def);
            next_def += 1;
            lower_enum(enumeration, id, source, &mut next_variant)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let functions = program
        .functions
        .into_iter()
        .map(|function| {
            let id = DefId(next_def);
            next_def += 1;
            lower_function(function, id, source)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let imports = program
        .imports
        .into_iter()
        .map(|import| {
            (
                import.path.segments,
                SourceLocation::new(source, import.path.span),
            )
        })
        .collect::<Vec<_>>();
    let (import_paths, import_locations) = imports.into_iter().unzip();
    let mut lowered = Program {
        source,
        id: ModuleId(0),
        module: program.module.map(|path| path.segments),
        imports: import_paths,
        import_locations,
        newtypes,
        structs,
        enums,
        functions,
    };
    resolve_references(&mut lowered);
    Ok(lowered)
}

/// 在 HIR 形成后回填所有可由当前模块图确定的语义引用。
///
/// 未声明的名称刻意保留为空，后续类型检查据此报告既有的用户诊断；lowering 不把名称
/// 错误伪装成语法错误。这样每个成功进入 Typed HIR 的引用都已有稳定 ID。
fn resolve_references(program: &mut Program) {
    let symbols = SemanticSymbols {
        functions: program
            .functions
            .iter()
            .map(|function| {
                (
                    function.name.clone(),
                    (function.id, function.return_type.clone()),
                )
            })
            .collect(),
        newtypes: program
            .newtypes
            .iter()
            .map(|newtype| (newtype.name.clone(), newtype.id))
            .collect(),
        structures: program
            .structs
            .iter()
            .map(|structure| {
                (
                    structure.name.clone(),
                    (
                        structure.id,
                        structure
                            .fields
                            .iter()
                            .map(|field| {
                                (
                                    field.name.clone(),
                                    SemanticField {
                                        id: field.id,
                                        ty: field.ty.clone(),
                                    },
                                )
                            })
                            .collect::<HashMap<_, _>>(),
                    ),
                )
            })
            .collect(),
        variants: program
            .enums
            .iter()
            .flat_map(|enumeration| {
                enumeration.variants.iter().map(move |variant| {
                    (
                        (enumeration.name.clone(), variant.name.clone()),
                        SemanticVariant {
                            id: variant.id,
                            enum_type: Type::Named(enumeration.name.clone()),
                            payload: variant.payload.as_ref().map(|payload| payload.ty.clone()),
                        },
                    )
                })
            })
            .collect(),
    };
    resolve_references_with_symbols(program, &symbols);
}

/// 模块图解析所需的已见声明集合；名称只在 HIR 解析期用于建立稳定 ID。
struct SemanticSymbols {
    functions: HashMap<String, (DefId, Type)>,
    newtypes: HashMap<String, DefId>,
    structures: HashMap<String, (DefId, HashMap<String, SemanticField>)>,
    variants: HashMap<(String, String), SemanticVariant>,
}

#[derive(Clone)]
struct SemanticField {
    id: FieldId,
    ty: Type,
}

#[derive(Clone)]
struct SemanticVariant {
    id: VariantId,
    enum_type: Type,
    payload: Option<Type>,
}

#[derive(Clone)]
struct ResolvedLocal {
    id: LocalId,
    ty: Option<Type>,
}

/// 使用模块私有声明与显式导入的公开声明回填当前模块的语义 ID。
fn resolve_references_with_symbols(program: &mut Program, symbols: &SemanticSymbols) {
    for function in &mut program.functions {
        let mut locals = function
            .parameters
            .iter()
            .map(|parameter| {
                (
                    parameter.name.clone(),
                    ResolvedLocal {
                        id: parameter.id,
                        ty: Some(parameter.ty.clone()),
                    },
                )
            })
            .collect::<HashMap<_, _>>();
        let mut next_local = function.parameters.len() as u32;
        resolve_statements(
            &mut function.statements,
            &mut locals,
            &mut next_local,
            &symbols.functions,
            &symbols.newtypes,
            &symbols.structures,
            &symbols.variants,
        );
    }
}

fn module_indices(modules: &[Program]) -> Result<HashMap<ModuleId, usize>, ResolveError> {
    let mut indices = HashMap::new();
    for (index, module) in modules.iter().enumerate() {
        if indices.insert(module.id, index).is_some() {
            return Err(ResolveError {
                location: SourceLocation::new(module.source, Span::default()),
                message: "module graph contains a duplicate module ID".to_owned(),
            });
        }
    }
    Ok(indices)
}

fn assign_global_ids(modules: &mut [Program]) {
    let mut next_def = 0;
    let mut next_field = 0;
    let mut next_variant = 0;
    for module in modules {
        for declaration in &mut module.newtypes {
            declaration.id = DefId(next_def);
            next_def += 1;
        }
        for declaration in &mut module.structs {
            declaration.id = DefId(next_def);
            next_def += 1;
            for field in &mut declaration.fields {
                field.id = FieldId(next_field);
                next_field += 1;
            }
        }
        for declaration in &mut module.enums {
            declaration.id = DefId(next_def);
            next_def += 1;
            for variant in &mut declaration.variants {
                variant.id = VariantId(next_variant);
                next_variant += 1;
            }
        }
        for function in &mut module.functions {
            function.id = DefId(next_def);
            next_def += 1;
        }
    }
}

fn visible_symbols(
    module_index: usize,
    modules: &[Program],
    module_indices: &HashMap<ModuleId, usize>,
) -> Result<SemanticSymbols, ResolveError> {
    let module = &modules[module_index];
    let mut symbols = own_symbols(module);
    for (index, import) in module.imports.iter().enumerate() {
        let location = module
            .import_locations
            .get(index)
            .copied()
            .unwrap_or_else(|| SourceLocation::new(module.source, Span::default()));
        if is_platform_import(import) {
            continue;
        }
        let (symbol, module_path) = import
            .split_last()
            .ok_or_else(|| resolve_error_at(location, "import must name a module and symbol"))?;
        if module_path.is_empty() {
            return Err(resolve_error_at(
                location,
                "import must name a module and symbol",
            ));
        }
        let dependency_index = modules
            .iter()
            .position(|candidate| candidate.module.as_deref() == Some(module_path))
            .ok_or_else(|| {
                resolve_error_at(
                    location,
                    format!("imported module `{}` was not found", module_path.join(".")),
                )
            })?;
        let dependency = &modules[dependency_index];
        add_public_symbol(&mut symbols, dependency, symbol)
            .map_err(|message| resolve_error_at(location, message))?;
    }
    let _ = module_indices;
    Ok(symbols)
}

fn own_symbols(module: &Program) -> SemanticSymbols {
    SemanticSymbols {
        functions: module
            .functions
            .iter()
            .map(|function| {
                (
                    function.name.clone(),
                    (function.id, function.return_type.clone()),
                )
            })
            .collect(),
        newtypes: module
            .newtypes
            .iter()
            .map(|newtype| (newtype.name.clone(), newtype.id))
            .collect(),
        structures: module
            .structs
            .iter()
            .map(|structure| {
                (
                    structure.name.clone(),
                    (
                        structure.id,
                        structure
                            .fields
                            .iter()
                            .map(|field| {
                                (
                                    field.name.clone(),
                                    SemanticField {
                                        id: field.id,
                                        ty: field.ty.clone(),
                                    },
                                )
                            })
                            .collect(),
                    ),
                )
            })
            .collect(),
        variants: module
            .enums
            .iter()
            .flat_map(|enumeration| {
                enumeration.variants.iter().map(move |variant| {
                    (
                        (enumeration.name.clone(), variant.name.clone()),
                        SemanticVariant {
                            id: variant.id,
                            enum_type: Type::Named(enumeration.name.clone()),
                            payload: variant.payload.as_ref().map(|payload| payload.ty.clone()),
                        },
                    )
                })
            })
            .collect(),
    }
}

fn add_public_symbol(
    symbols: &mut SemanticSymbols,
    module: &Program,
    symbol: &str,
) -> Result<(), String> {
    if let Some(structure) = module.structs.iter().find(|item| item.name == symbol) {
        if !structure.public {
            return Err(format!("imported symbol `{symbol}` is not public"));
        }
        symbols.structures.insert(
            structure.name.clone(),
            (
                structure.id,
                structure
                    .fields
                    .iter()
                    .map(|field| {
                        (
                            field.name.clone(),
                            SemanticField {
                                id: field.id,
                                ty: field.ty.clone(),
                            },
                        )
                    })
                    .collect(),
            ),
        );
        return Ok(());
    }
    if let Some(enumeration) = module.enums.iter().find(|item| item.name == symbol) {
        if !enumeration.public {
            return Err(format!("imported symbol `{symbol}` is not public"));
        }
        for variant in &enumeration.variants {
            symbols.variants.insert(
                (enumeration.name.clone(), variant.name.clone()),
                SemanticVariant {
                    id: variant.id,
                    enum_type: Type::Named(enumeration.name.clone()),
                    payload: variant.payload.as_ref().map(|payload| payload.ty.clone()),
                },
            );
        }
        return Ok(());
    }
    if let Some(function) = module.functions.iter().find(|item| item.name == symbol) {
        if !function.public {
            return Err(format!("imported symbol `{symbol}` is not public"));
        }
        symbols.functions.insert(
            function.name.clone(),
            (function.id, function.return_type.clone()),
        );
        return Ok(());
    }
    if let Some(newtype) = module.newtypes.iter().find(|item| item.name == symbol) {
        if !newtype.public {
            return Err(format!("imported symbol `{symbol}` is not public"));
        }
        symbols.newtypes.insert(newtype.name.clone(), newtype.id);
        return Ok(());
    }
    Err(format!("imported symbol `{symbol}` was not found"))
}

fn resolve_error_at(location: SourceLocation, message: impl Into<String>) -> ResolveError {
    ResolveError {
        location,
        message: message.into(),
    }
}

fn is_platform_import(path: &[String]) -> bool {
    path.first().is_some_and(|segment| segment == "yan")
}

fn append_imported_declarations(
    module_index: usize,
    modules: &[Program],
    module_indices: &HashMap<ModuleId, usize>,
    entry: &mut Program,
    appended: &mut HashSet<DefId>,
    visited: &mut HashSet<ModuleId>,
) -> Result<(), ResolveError> {
    let module = &modules[module_index];
    if !visited.insert(module.id) {
        return Ok(());
    }
    for (index, import) in module.imports.iter().enumerate() {
        let location = module
            .import_locations
            .get(index)
            .copied()
            .unwrap_or_else(|| SourceLocation::new(module.source, Span::default()));
        if is_platform_import(import) {
            continue;
        }
        let (symbol, path) = import
            .split_last()
            .ok_or_else(|| resolve_error_at(location, "import must name a module and symbol"))?;
        let dependency_index = modules
            .iter()
            .position(|candidate| candidate.module.as_deref() == Some(path))
            .ok_or_else(|| {
                resolve_error_at(
                    location,
                    format!("imported module `{}` was not found", path.join(".")),
                )
            })?;
        let dependency = &modules[dependency_index];
        append_public_declaration(entry, dependency, symbol, appended)
            .map_err(|message| resolve_error_at(location, message))?;
        append_imported_declarations(
            dependency_index,
            modules,
            module_indices,
            entry,
            appended,
            visited,
        )?;
    }
    let _ = module_indices;
    Ok(())
}

fn append_public_declaration(
    entry: &mut Program,
    module: &Program,
    symbol: &str,
    appended: &mut HashSet<DefId>,
) -> Result<(), String> {
    if let Some(declaration) = module.newtypes.iter().find(|item| item.name == symbol) {
        if !declaration.public {
            return Err(format!("imported symbol `{symbol}` is not public"));
        }
        if appended.insert(declaration.id) {
            entry.newtypes.push(declaration.clone());
        }
        return Ok(());
    }
    if let Some(declaration) = module.structs.iter().find(|item| item.name == symbol) {
        if !declaration.public {
            return Err(format!("imported symbol `{symbol}` is not public"));
        }
        if appended.insert(declaration.id) {
            entry.structs.push(declaration.clone());
        }
        return Ok(());
    }
    if let Some(declaration) = module.enums.iter().find(|item| item.name == symbol) {
        if !declaration.public {
            return Err(format!("imported symbol `{symbol}` is not public"));
        }
        if appended.insert(declaration.id) {
            entry.enums.push(declaration.clone());
        }
        return Ok(());
    }
    if let Some(declaration) = module.functions.iter().find(|item| item.name == symbol) {
        if !declaration.public {
            return Err(format!("imported symbol `{symbol}` is not public"));
        }
        append_function_closure(entry, module, declaration.id, appended);
        return Ok(());
    }
    Err(format!("imported symbol `{symbol}` was not found"))
}

/// 将公开入口函数及其在同一模块内调用的私有函数闭包加入入口视图。
fn append_function_closure(
    entry: &mut Program,
    module: &Program,
    function_id: DefId,
    appended: &mut HashSet<DefId>,
) {
    if !appended.insert(function_id) {
        return;
    }
    let Some(function) = module
        .functions
        .iter()
        .find(|function| function.id == function_id)
    else {
        return;
    };
    let callees = called_functions_in_statements(&function.statements);
    entry.functions.push(function.clone());
    for callee in callees {
        append_function_closure(entry, module, callee, appended);
    }
}

fn called_functions_in_statements(statements: &[Statement]) -> Vec<DefId> {
    statements
        .iter()
        .flat_map(|statement| match statement {
            Statement::Destructure { value, .. }
            | Statement::Let { value, .. }
            | Statement::Assign { value, .. }
            | Statement::Expression(value) => called_functions(value),
        })
        .collect()
}

fn called_functions(expression: &Expression) -> Vec<DefId> {
    let mut calls = Vec::new();
    match expression {
        Expression::Call {
            target, arguments, ..
        } => {
            if let Some(CallTarget::Function(id)) = target {
                calls.push(*id);
            }
            calls.extend(arguments.iter().flat_map(called_functions));
        }
        Expression::List { values, .. } | Expression::Tuple { values, .. } => {
            calls.extend(values.iter().flat_map(called_functions));
        }
        Expression::Map { entries, .. } => {
            calls.extend(
                entries
                    .iter()
                    .flat_map(|entry| called_functions(&entry.value)),
            );
        }
        Expression::Match { target, arms, .. } => {
            calls.extend(called_functions(target));
            calls.extend(arms.iter().flat_map(|arm| called_functions(&arm.value)));
        }
        Expression::If {
            condition,
            then_statements,
            else_statements,
            ..
        } => {
            calls.extend(called_functions(condition));
            calls.extend(called_functions_in_statements(then_statements));
            calls.extend(called_functions_in_statements(else_statements));
        }
        Expression::For {
            iterable,
            statements,
            ..
        } => {
            calls.extend(called_functions(iterable));
            calls.extend(called_functions_in_statements(statements));
        }
        Expression::Return { value, .. } | Expression::Try { value, .. } => {
            calls.extend(called_functions(value));
        }
        Expression::Add { left, right, .. }
        | Expression::Multiply { left, right, .. }
        | Expression::Equal { left, right, .. } => {
            calls.extend(called_functions(left));
            calls.extend(called_functions(right));
        }
        Expression::StructLiteral { fields, .. } => {
            calls.extend(
                fields
                    .iter()
                    .flat_map(|field| called_functions(&field.value)),
            );
        }
        Expression::FieldAccess { target, .. } => calls.extend(called_functions(target)),
        Expression::Integer { .. }
        | Expression::Float { .. }
        | Expression::Boolean { .. }
        | Expression::String { .. }
        | Expression::Variable { .. } => {}
    }
    calls
}

/// 解析一个顺序语句块，并把块内绑定限定在调用方传入的作用域中。
fn resolve_statements(
    statements: &mut [Statement],
    locals: &mut HashMap<String, ResolvedLocal>,
    next_local: &mut u32,
    functions: &HashMap<String, (DefId, Type)>,
    newtypes: &HashMap<String, DefId>,
    structures: &HashMap<String, (DefId, HashMap<String, SemanticField>)>,
    variants: &HashMap<(String, String), SemanticVariant>,
) {
    for statement in statements {
        match statement {
            Statement::Destructure {
                locals: ids,
                names,
                value,
            } => {
                resolve_expression(
                    value, locals, next_local, functions, newtypes, structures, variants,
                );
                *ids = names
                    .iter()
                    .map(|(name, _)| {
                        let id = allocate_local(next_local);
                        locals.insert(name.clone(), ResolvedLocal { id, ty: None });
                        id
                    })
                    .collect();
            }
            Statement::Let {
                local,
                name,
                annotation,
                value,
                ..
            } => {
                resolve_expression(
                    value, locals, next_local, functions, newtypes, structures, variants,
                );
                let id = allocate_local(next_local);
                *local = id;
                let ty = annotation.clone().or_else(|| {
                    resolved_expression_type(value, locals, functions, structures, variants)
                });
                locals.insert(name.clone(), ResolvedLocal { id, ty });
            }
            Statement::Assign {
                local, name, value, ..
            } => {
                resolve_expression(
                    value, locals, next_local, functions, newtypes, structures, variants,
                );
                if let Some(binding) = locals.get(name) {
                    *local = binding.id;
                }
            }
            Statement::Expression(value) => {
                resolve_expression(
                    value, locals, next_local, functions, newtypes, structures, variants,
                );
            }
        }
    }
}

/// 递归回填表达式及嵌套语句块的引用 ID。
fn resolve_expression(
    expression: &mut Expression,
    locals: &HashMap<String, ResolvedLocal>,
    next_local: &mut u32,
    functions: &HashMap<String, (DefId, Type)>,
    newtypes: &HashMap<String, DefId>,
    structures: &HashMap<String, (DefId, HashMap<String, SemanticField>)>,
    variants: &HashMap<(String, String), SemanticVariant>,
) {
    match expression {
        Expression::String { parts, .. } => {
            for part in parts {
                if let StringPart::Variable { local, name, .. } = part {
                    *local = locals.get(name).map(|binding| binding.id);
                }
            }
        }
        Expression::List { values, .. } | Expression::Tuple { values, .. } => {
            for value in values {
                resolve_expression(
                    value, locals, next_local, functions, newtypes, structures, variants,
                );
            }
        }
        Expression::Map { entries, .. } => {
            for entry in entries {
                resolve_expression(
                    &mut entry.value,
                    locals,
                    next_local,
                    functions,
                    newtypes,
                    structures,
                    variants,
                );
            }
        }
        Expression::Match { target, arms, .. } => {
            resolve_expression(
                target, locals, next_local, functions, newtypes, structures, variants,
            );
            for arm in arms {
                arm.pattern.variant_id = variants
                    .get(&(arm.pattern.enum_name.clone(), arm.pattern.variant.clone()))
                    .map(|variant| variant.id);
                let mut arm_locals = locals.clone();
                if let Some((name, _)) = &arm.pattern.binding {
                    let id = allocate_local(next_local);
                    arm.pattern.binding_local = Some(id);
                    arm_locals.insert(name.clone(), ResolvedLocal { id, ty: None });
                }
                resolve_expression(
                    &mut arm.value,
                    &arm_locals,
                    next_local,
                    functions,
                    newtypes,
                    structures,
                    variants,
                );
            }
        }
        Expression::If {
            condition,
            then_statements,
            else_statements,
            ..
        } => {
            resolve_expression(
                condition, locals, next_local, functions, newtypes, structures, variants,
            );
            let mut then_locals = locals.clone();
            resolve_statements(
                then_statements,
                &mut then_locals,
                next_local,
                functions,
                newtypes,
                structures,
                variants,
            );
            let mut else_locals = locals.clone();
            resolve_statements(
                else_statements,
                &mut else_locals,
                next_local,
                functions,
                newtypes,
                structures,
                variants,
            );
        }
        Expression::For {
            local,
            name,
            iterable,
            statements,
            ..
        } => {
            resolve_expression(
                iterable, locals, next_local, functions, newtypes, structures, variants,
            );
            let id = allocate_local(next_local);
            *local = id;
            let mut loop_locals = locals.clone();
            loop_locals.insert(name.clone(), ResolvedLocal { id, ty: None });
            resolve_statements(
                statements,
                &mut loop_locals,
                next_local,
                functions,
                newtypes,
                structures,
                variants,
            );
        }
        Expression::Return { value, .. } | Expression::Try { value, .. } => {
            resolve_expression(
                value, locals, next_local, functions, newtypes, structures, variants,
            );
        }
        Expression::Variable { local, name, .. } => {
            *local = locals.get(name).map(|binding| binding.id)
        }
        Expression::Call {
            target,
            path,
            arguments,
            ..
        } => {
            *target = match path
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .as_slice()
            {
                ["bytes", "from_hex"] => Some(CallTarget::BytesFromHex),
                ["console", "println"] => Some(CallTarget::ConsolePrintln),
                ["Some"] => Some(CallTarget::Some),
                ["Ok"] => Some(CallTarget::Ok),
                ["Err"] => Some(CallTarget::Err),
                [receiver, "to_int"] => locals
                    .get(*receiver)
                    .map(|binding| CallTarget::StringToInt(binding.id)),
                [name] => functions
                    .get(*name)
                    .map(|(id, _)| CallTarget::Function(*id))
                    .or_else(|| newtypes.get(*name).copied().map(CallTarget::Newtype)),
                [enum_name, variant] => variants
                    .get(&((*enum_name).to_owned(), (*variant).to_owned()))
                    .map(|variant| CallTarget::Variant(variant.id)),
                _ => None,
            };
            for argument in arguments {
                resolve_expression(
                    argument, locals, next_local, functions, newtypes, structures, variants,
                );
            }
        }
        Expression::Add { left, right, .. }
        | Expression::Multiply { left, right, .. }
        | Expression::Equal { left, right, .. } => {
            resolve_expression(
                left, locals, next_local, functions, newtypes, structures, variants,
            );
            resolve_expression(
                right, locals, next_local, functions, newtypes, structures, variants,
            );
        }
        Expression::StructLiteral {
            structure,
            name,
            fields,
            ..
        } => {
            if let Some((id, declared_fields)) = structures.get(name) {
                *structure = *id;
                for field in fields {
                    field.field_id = declared_fields.get(&field.name).map(|field| field.id);
                    resolve_expression(
                        &mut field.value,
                        locals,
                        next_local,
                        functions,
                        newtypes,
                        structures,
                        variants,
                    );
                }
            }
        }
        Expression::FieldAccess {
            target,
            field,
            field_id,
            variant,
            ..
        } => {
            let target_type = match target.as_ref() {
                Expression::Variable { name, .. } => {
                    locals.get(name).and_then(|binding| binding.ty.clone())
                }
                Expression::StructLiteral { name, .. } => Some(Type::Named(name.clone())),
                Expression::Call {
                    target: Some(CallTarget::Function(id)),
                    ..
                } => functions
                    .values()
                    .find(|(candidate, _)| candidate == id)
                    .map(|(_, ty)| ty.clone()),
                _ => None,
            };
            if let Expression::Variable { name, .. } = target.as_ref() {
                *variant = variants
                    .get(&(name.clone(), field.clone()))
                    .map(|variant| variant.id);
            }
            if variant.is_none() {
                *field_id = target_type
                    .as_ref()
                    .and_then(|ty| match ty {
                        Type::Named(name) => structures.get(name),
                        _ => None,
                    })
                    .and_then(|(_, fields)| fields.get(field))
                    .map(|field| field.id);
            }
            // 先用当前作用域的已声明名义类型解析字段，再递归解析目标局部读取。
            resolve_expression(
                target, locals, next_local, functions, newtypes, structures, variants,
            );
        }
        Expression::Integer { .. } | Expression::Float { .. } | Expression::Boolean { .. } => {}
    }
}

/// 从已解析目标推导名称解析阶段可确定的值类型，仅用于继续解析字段等语义 ID。
fn resolved_expression_type(
    expression: &Expression,
    locals: &HashMap<String, ResolvedLocal>,
    functions: &HashMap<String, (DefId, Type)>,
    structures: &HashMap<String, (DefId, HashMap<String, SemanticField>)>,
    variants: &HashMap<(String, String), SemanticVariant>,
) -> Option<Type> {
    match expression {
        Expression::Integer { .. } => Some(Type::Int),
        Expression::Float { .. } => Some(Type::Float),
        Expression::Boolean { .. } => Some(Type::Bool),
        Expression::String { .. } => Some(Type::String),
        Expression::Variable {
            local: Some(id), ..
        } => locals
            .values()
            .find(|binding| binding.id == *id)
            .and_then(|binding| binding.ty.clone()),
        Expression::StructLiteral { name, .. } => Some(Type::Named(name.clone())),
        Expression::Call {
            target: Some(CallTarget::Function(id)),
            ..
        } => functions
            .values()
            .find(|(candidate, _)| candidate == id)
            .map(|(_, ty)| ty.clone()),
        Expression::Call {
            target: Some(CallTarget::Variant(id)),
            ..
        }
        | Expression::FieldAccess {
            variant: Some(id), ..
        } => variants
            .values()
            .find(|variant| variant.id == *id)
            .map(|variant| variant.enum_type.clone()),
        Expression::If {
            then_statements,
            else_statements,
            ..
        } => {
            let then_ty = resolved_statement_tail_type(
                then_statements,
                locals,
                functions,
                structures,
                variants,
            )?;
            let else_ty = resolved_statement_tail_type(
                else_statements,
                locals,
                functions,
                structures,
                variants,
            )?;
            (then_ty == else_ty).then_some(then_ty)
        }
        Expression::Match { arms, .. } => {
            let mut result = None;
            for arm in arms {
                let arm_ty = match (&arm.value, arm.pattern.binding_local) {
                    (
                        Expression::Variable {
                            local: Some(local), ..
                        },
                        Some(binding),
                    ) if *local == binding => arm.pattern.variant_id.and_then(|id| {
                        variants
                            .values()
                            .find(|variant| variant.id == id)
                            .and_then(|variant| variant.payload.clone())
                    }),
                    _ => resolved_expression_type(
                        &arm.value, locals, functions, structures, variants,
                    ),
                }?;
                if result.as_ref().is_some_and(|expected| expected != &arm_ty) {
                    return None;
                }
                result = Some(arm_ty);
            }
            result
        }
        Expression::FieldAccess {
            target,
            field_id: Some(field_id),
            ..
        } => {
            let Type::Named(name) =
                resolved_expression_type(target, locals, functions, structures, variants)?
            else {
                return None;
            };
            structures
                .get(&name)
                .and_then(|(_, fields)| fields.values().find(|field| field.id == *field_id))
                .map(|field| field.ty.clone())
        }
        _ => None,
    }
}

fn resolved_statement_tail_type(
    statements: &[Statement],
    locals: &HashMap<String, ResolvedLocal>,
    functions: &HashMap<String, (DefId, Type)>,
    structures: &HashMap<String, (DefId, HashMap<String, SemanticField>)>,
    variants: &HashMap<(String, String), SemanticVariant>,
) -> Option<Type> {
    let Statement::Expression(expression) = statements.last()? else {
        return None;
    };
    resolved_expression_type(expression, locals, functions, structures, variants)
}

/// 分配函数内唯一局部 ID，避免嵌套块和 match 分支复用同一槽位。
fn allocate_local(next_local: &mut u32) -> LocalId {
    let id = LocalId(*next_local);
    *next_local += 1;
    id
}

fn lower_newtype(
    newtype: yan_syntax::Newtype,
    id: DefId,
    source: SourceId,
) -> Result<Newtype, LowerError> {
    Ok(Newtype {
        source,
        id,
        public: newtype.public,
        name: newtype.name,
        name_span: newtype.name_span,
        underlying: lower_type(newtype.underlying)?,
    })
}

fn lower_struct(
    structure: yan_syntax::Struct,
    id: DefId,
    source: SourceId,
    next_field: &mut u32,
) -> Result<Struct, LowerError> {
    Ok(Struct {
        source,
        id,
        public: structure.public,
        name: structure.name,
        name_span: structure.name_span,
        fields: structure
            .fields
            .into_iter()
            .map(|field| {
                let id = FieldId(*next_field);
                *next_field += 1;
                lower_declared_field(field, id, source)
            })
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn lower_enum(
    enumeration: SyntaxEnum,
    id: DefId,
    source: SourceId,
    next_variant: &mut u32,
) -> Result<Enum, LowerError> {
    Ok(Enum {
        source,
        id,
        public: enumeration.public,
        name: enumeration.name,
        name_span: enumeration.name_span,
        variants: enumeration
            .variants
            .into_iter()
            .map(|variant| {
                Ok(EnumVariant {
                    source,
                    id: {
                        let id = VariantId(*next_variant);
                        *next_variant += 1;
                        id
                    },
                    name: variant.name,
                    name_span: variant.name_span,
                    payload: variant
                        .payload
                        .map(|payload| {
                            Ok(EnumPayload {
                                name: payload.name,
                                name_span: payload.name_span,
                                ty: lower_type(payload.ty)?,
                            })
                        })
                        .transpose()?,
                })
            })
            .collect::<Result<Vec<_>, LowerError>>()?,
    })
}

fn lower_declared_field(
    field: SyntaxField,
    id: FieldId,
    source: SourceId,
) -> Result<Field, LowerError> {
    let Some(ty) = field.ty else {
        return Err(LowerError {
            location: SourceLocation::new(SourceId(0), field.name_span),
            message: "struct field is missing a type".to_owned(),
        });
    };
    Ok(Field {
        source,
        id,
        name: field.name,
        name_span: field.name_span,
        ty: lower_type(ty)?,
        default: field.default.map(lower_expression).transpose()?,
    })
}

/// lowering 中发现的当前阶段不支持的语法类型。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LowerError {
    /// 不支持写法的位置。
    pub location: SourceLocation,
    /// 面向用户的错误原因。
    pub message: String,
}

impl LowerError {
    fn with_source(mut self, source: SourceId) -> Self {
        self.location.source = source;
        self
    }
}

fn lower_function(
    function: yan_syntax::Function,
    id: DefId,
    source: SourceId,
) -> Result<Function, LowerError> {
    Ok(Function {
        source,
        id,
        public: function.public,
        name: function.name,
        name_span: function.name_span,
        parameters: function
            .parameters
            .into_iter()
            .enumerate()
            .map(|(index, parameter)| {
                Ok(Parameter {
                    id: LocalId(index as u32),
                    name: parameter.name,
                    name_span: parameter.name_span,
                    ty: lower_type(parameter.ty)?,
                })
            })
            .collect::<Result<Vec<_>, LowerError>>()?,
        return_type: lower_type(function.return_type)?,
        statements: function
            .statements
            .into_iter()
            .map(lower_statement)
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn lower_type(ty: TypeSyntax) -> Result<Type, LowerError> {
    let unsupported = || LowerError {
        location: SourceLocation::new(SourceId(0), ty.span),
        message: format!("M3 does not support type `{}`", ty.name),
    };
    if ty.name == "()" {
        return Ok(Type::Tuple(
            ty.tuple_elements
                .into_iter()
                .map(lower_type)
                .collect::<Result<Vec<_>, _>>()?,
        ));
    }
    match (ty.name.as_str(), ty.arguments.as_slice()) {
        ("int", []) => Ok(Type::Int),
        ("float", []) => Ok(Type::Float),
        ("bytes", []) => Ok(Type::Bytes),
        ("bool", []) => Ok(Type::Bool),
        ("string", []) => Ok(Type::String),
        ("unit", []) => Ok(Type::Unit),
        ("List", [element]) => Ok(Type::List(Box::new(lower_type(element.clone())?))),
        ("Map", [key, value]) if key.name == "string" && key.arguments.is_empty() => {
            Ok(Type::Map(Box::new(lower_type(value.clone())?)))
        }
        ("Option", [element]) => {
            let element = lower_type(element.clone())?;
            if matches!(element, Type::Option(_)) {
                return Err(LowerError {
                    location: SourceLocation::new(SourceId(0), ty.span),
                    message: "M7 does not support nested `Option` types".to_owned(),
                });
            }
            Ok(Type::Option(Box::new(element)))
        }
        ("Result", [ok, error]) => Ok(Type::Result(
            Box::new(lower_type(ok.clone())?),
            Box::new(lower_type(error.clone())?),
        )),
        (name, []) => Ok(Type::Named(name.to_owned())),
        _ => Err(unsupported()),
    }
}

fn lower_statement(statement: SyntaxStatement) -> Result<Statement, LowerError> {
    match statement {
        SyntaxStatement::Destructure { names, value } => Ok(Statement::Destructure {
            locals: Vec::new(),
            names,
            value: lower_expression(value)?,
        }),
        SyntaxStatement::Let {
            mutable,
            name,
            name_span,
            annotation,
            value,
        } => Ok(Statement::Let {
            local: LocalId(0),
            mutable,
            name,
            name_span,
            annotation: annotation.map(lower_type).transpose()?,
            value: lower_expression(value)?,
        }),
        SyntaxStatement::Assign {
            name,
            name_span,
            value,
        } => Ok(Statement::Assign {
            local: LocalId(0),
            name,
            name_span,
            value: lower_expression(value)?,
        }),
        SyntaxStatement::Expression(expression) => {
            Ok(Statement::Expression(lower_expression(expression)?))
        }
    }
}

fn lower_expression(expression: SyntaxExpression) -> Result<Expression, LowerError> {
    Ok(match expression {
        SyntaxExpression::Integer { value, span } => Expression::Integer { value, span },
        SyntaxExpression::Float { value, span } => Expression::Float { value, span },
        SyntaxExpression::Boolean { value, span } => Expression::Boolean { value, span },
        SyntaxExpression::String { value, span } => Expression::String {
            parts: lower_string_parts(&value, span)?,
            span,
        },
        SyntaxExpression::List { values, span } => Expression::List {
            values: values
                .into_iter()
                .map(lower_expression)
                .collect::<Result<Vec<_>, _>>()?,
            span,
        },
        SyntaxExpression::Map { entries, span } => Expression::Map {
            entries: entries
                .into_iter()
                .map(lower_map_entry)
                .collect::<Result<Vec<_>, _>>()?,
            span,
        },
        SyntaxExpression::Tuple { values, span } => Expression::Tuple {
            values: values
                .into_iter()
                .map(lower_expression)
                .collect::<Result<Vec<_>, _>>()?,
            span,
        },
        SyntaxExpression::Match { target, arms, span } => Expression::Match {
            target: Box::new(lower_expression(*target)?),
            arms: arms
                .into_iter()
                .map(lower_match_arm)
                .collect::<Result<Vec<_>, _>>()?,
            span,
        },
        SyntaxExpression::If {
            condition,
            then_statements,
            else_statements,
            span,
        } => Expression::If {
            condition: Box::new(lower_expression(*condition)?),
            then_statements: then_statements
                .into_iter()
                .map(lower_statement)
                .collect::<Result<Vec<_>, _>>()?,
            else_statements: else_statements
                .into_iter()
                .map(lower_statement)
                .collect::<Result<Vec<_>, _>>()?,
            span,
        },
        SyntaxExpression::For {
            name,
            name_span,
            iterable,
            statements,
            span,
        } => Expression::For {
            local: LocalId(0),
            name,
            name_span,
            iterable: Box::new(lower_expression(*iterable)?),
            statements: statements
                .into_iter()
                .map(lower_statement)
                .collect::<Result<Vec<_>, _>>()?,
            span,
        },
        SyntaxExpression::Return { value, span } => Expression::Return {
            value: Box::new(lower_expression(*value)?),
            span,
        },
        SyntaxExpression::Try { value, span } => Expression::Try {
            value: Box::new(lower_expression(*value)?),
            span,
        },
        SyntaxExpression::Variable { name, span } => Expression::Variable {
            local: None,
            name,
            span,
        },
        SyntaxExpression::Call {
            path,
            arguments,
            span,
        } => Expression::Call {
            target: None,
            path,
            arguments: arguments
                .into_iter()
                .map(lower_expression)
                .collect::<Result<Vec<_>, _>>()?,
            span,
        },
        SyntaxExpression::Add { left, right, span } => Expression::Add {
            left: Box::new(lower_expression(*left)?),
            right: Box::new(lower_expression(*right)?),
            span,
        },
        SyntaxExpression::Multiply { left, right, span } => Expression::Multiply {
            left: Box::new(lower_expression(*left)?),
            right: Box::new(lower_expression(*right)?),
            span,
        },
        SyntaxExpression::Equal { left, right, span } => Expression::Equal {
            left: Box::new(lower_expression(*left)?),
            right: Box::new(lower_expression(*right)?),
            span,
        },
        SyntaxExpression::StructLiteral {
            name,
            name_span,
            fields,
            span,
        } => Expression::StructLiteral {
            structure: DefId(0),
            name,
            name_span,
            fields: fields
                .into_iter()
                .map(lower_struct_field_value)
                .collect::<Result<Vec<_>, _>>()?,
            span,
        },
        SyntaxExpression::FieldAccess {
            target,
            field,
            field_span,
            span,
        } => Expression::FieldAccess {
            field_id: None,
            variant: None,
            target: Box::new(lower_expression(*target)?),
            field,
            field_span,
            span,
        },
    })
}

fn lower_map_entry(entry: SyntaxMapEntry) -> Result<MapEntry, LowerError> {
    Ok(MapEntry {
        key: entry.key,
        key_span: entry.key_span,
        value: lower_expression(entry.value)?,
    })
}

fn lower_match_arm(arm: SyntaxMatchArm) -> Result<MatchArm, LowerError> {
    Ok(MatchArm {
        pattern: EnumPattern {
            variant_id: None,
            binding_local: None,
            enum_name: arm.pattern.enum_name,
            enum_name_span: arm.pattern.enum_name_span,
            variant: arm.pattern.variant,
            variant_span: arm.pattern.variant_span,
            binding: arm.pattern.binding,
        },
        value: lower_expression(arm.value)?,
    })
}

fn lower_struct_field_value(field: SyntaxField) -> Result<StructFieldValue, LowerError> {
    let Some(value) = field.value else {
        return Err(LowerError {
            location: SourceLocation::new(SourceId(0), field.name_span),
            message: "struct literal field is missing a value".to_owned(),
        });
    };
    Ok(StructFieldValue {
        field_id: None,
        name: field.name,
        name_span: field.name_span,
        value: lower_expression(value)?,
    })
}

fn lower_string_parts(value: &str, span: Span) -> Result<Vec<StringPart>, LowerError> {
    let mut parts = Vec::new();
    let mut text_start = 0;
    let bytes = value.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            b'{' => {
                if text_start < index {
                    parts.push(StringPart::Text(value[text_start..index].to_owned()));
                }
                let name_start = index + 1;
                let Some(relative_end) = value[name_start..].find('}') else {
                    return Err(string_error(
                        span,
                        index,
                        value.len(),
                        "string interpolation is missing `}`",
                    ));
                };
                let name_end = name_start + relative_end;
                let name = &value[name_start..name_end];
                if !is_identifier(name) {
                    return Err(string_error(
                        span,
                        name_start,
                        name_end,
                        "string interpolation must use `{identifier}`",
                    ));
                }
                parts.push(StringPart::Variable {
                    local: None,
                    name: name.to_owned(),
                    // span 的起点包含字符串开头的双引号，因此插值名称再偏移一个字节。
                    span: Span::new(span.start + name_start + 1, span.start + name_end + 1),
                });
                index = name_end + 1;
                text_start = index;
            }
            b'}' => {
                return Err(string_error(
                    span,
                    index,
                    index + 1,
                    "string interpolation has an unmatched `}`",
                ));
            }
            _ => index += 1,
        }
    }

    if text_start < value.len() || parts.is_empty() {
        parts.push(StringPart::Text(value[text_start..].to_owned()));
    }
    Ok(parts)
}

fn string_error(span: Span, start: usize, end: usize, message: &str) -> LowerError {
    LowerError {
        // span 的首字节是开引号，字面量内容从下一个字节开始。
        location: SourceLocation::new(
            SourceId(0),
            Span::new(span.start + start + 1, span.start + end + 1),
        ),
        message: message.to_owned(),
    }
}

fn is_identifier(value: &str) -> bool {
    let bytes = value.as_bytes();
    matches!(bytes.first(), Some(b'a'..=b'z' | b'A'..=b'Z' | b'_'))
        && bytes[1..]
            .iter()
            .all(|byte| matches!(byte, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_'))
}

#[cfg(test)]
mod tests {
    use yan_syntax::{lex, parse};

    use super::{
        lower, resolve_modules, CallTarget, DefId, Expression, LocalId, ModuleGraph, ModuleId,
        ModuleInput, Statement, StringPart, VariantId,
    };

    #[test]
    fn resolves_imported_public_function_without_cli_declaration_append() {
        let entry = lower_program(
            "module example.entry import example.message.display fn main() -> unit { display() }",
        );
        let message = lower_program(
            "module example.message pub fn display() -> unit { console.println(\"ready\") }",
        );
        let graph = ModuleGraph::new(
            vec![
                ModuleInput::new(ModuleId(0), entry),
                ModuleInput::new(ModuleId(1), message),
            ],
            ModuleId(0),
        );

        let resolved = resolve_modules(graph).expect("测试模块图必须完成解析");
        let expression = match resolved.modules[0].functions[0]
            .statements
            .last()
            .expect("main 必须包含函数调用")
        {
            Statement::Expression(expression) => expression,
            _ => panic!("main 尾语句必须是表达式"),
        };
        let Expression::Call { target, .. } = expression else {
            panic!("main 尾表达式必须是函数调用");
        };

        assert_eq!(target, &Some(CallTarget::Function(DefId(1))));
    }

    fn lower_program(source: &str) -> super::Program {
        let tokens = lex(source).expect("测试源码应完成词法分析");
        let syntax = parse(source, &tokens).expect("测试源码应完成语法分析");
        lower(syntax).expect("测试源码应完成 lowering")
    }

    #[test]
    fn resolves_string_interpolation_to_its_local_id() {
        let source = "fn main() -> string { let title = \"Yan\" \"{title}\" }";
        let tokens = lex(source).expect("测试源码应完成词法分析");
        let syntax = parse(source, &tokens).expect("测试源码应完成语法分析");
        let program = lower(syntax).expect("测试源码应完成 lowering");

        let Statement::Expression(Expression::String { parts, .. }) = program.functions[0]
            .statements
            .last()
            .expect("main 必须包含尾表达式")
        else {
            panic!("尾表达式必须是字符串插值");
        };

        assert!(matches!(
            parts.as_slice(),
            [StringPart::Variable { local: Some(_), name, .. }] if name == "title"
        ));
    }

    #[test]
    fn resolves_struct_field_access_to_its_semantic_id() {
        let program = lower_program(
            "struct User { name: string } fn label(user: User) -> string { user.name } fn main() -> unit { }",
        );
        let Statement::Expression(Expression::FieldAccess { field_id, .. }) =
            &program.functions[0].statements[0]
        else {
            panic!("label 的尾表达式必须读取结构体字段")
        };

        assert_eq!(*field_id, Some(super::FieldId(0)));
    }

    #[test]
    fn resolves_every_call_kind_before_type_checking() {
        let program = lower_program(
            "type Port = int enum State { Failed(reason: string) } fn helper() -> unit { } fn convert(text: string) -> unit { helper() Port(3) State.Failed(\"bad\") text.to_int() bytes.from_hex(\"a1\") console.println(\"ready\") Some(1) Ok(1) Err(\"bad\") } fn main() -> unit { }",
        );
        let targets = program.functions[1]
            .statements
            .iter()
            .map(|statement| {
                let Statement::Expression(Expression::Call { target, .. }) = statement else {
                    panic!("convert 只能包含调用表达式")
                };
                *target
            })
            .collect::<Vec<_>>();

        assert_eq!(
            targets,
            vec![
                Some(CallTarget::Function(DefId(2))),
                Some(CallTarget::Newtype(DefId(0))),
                Some(CallTarget::Variant(VariantId(0))),
                Some(CallTarget::StringToInt(LocalId(0))),
                Some(CallTarget::BytesFromHex),
                Some(CallTarget::ConsolePrintln),
                Some(CallTarget::Some),
                Some(CallTarget::Ok),
                Some(CallTarget::Err),
            ]
        );
    }

    #[test]
    fn entry_program_retains_private_functions_reachable_from_public_imports() {
        let entry = lower_program(
            "module example.entry import example.library.public_fn fn main() -> unit { public_fn() }",
        );
        let library = lower_program(
            "module example.library fn private_fn() -> unit { } pub fn public_fn() -> unit { private_fn() }",
        );
        let resolved = resolve_modules(ModuleGraph::new(
            vec![
                ModuleInput::new(ModuleId(0), entry),
                ModuleInput::new(ModuleId(1), library),
            ],
            ModuleId(0),
        ))
        .expect("测试模块图必须完成解析");

        let program = resolved.entry_program().expect("入口视图必须建立成功");
        assert!(program
            .functions
            .iter()
            .any(|function| function.name == "public_fn"));
        assert!(program
            .functions
            .iter()
            .any(|function| function.name == "private_fn"));
    }

    #[test]
    fn resolves_field_after_if_and_match_produce_struct_values() {
        let program = lower_program(
            "struct User { name: string } enum Choice { Left(user: User) Right(user: User) } fn from_if(flag: bool) -> string { let user = if flag { User { name: \"left\" } } else { User { name: \"right\" } } user.name } fn from_match(choice: Choice) -> string { let user = match choice { Choice.Left(value) => value Choice.Right(value) => value } user.name } fn main() -> unit { }",
        );

        for function in &program.functions[..2] {
            let Statement::Expression(Expression::FieldAccess { field_id, .. }) =
                function.statements.last().expect("函数必须读取字段")
            else {
                panic!("函数尾表达式必须读取字段")
            };
            assert_eq!(*field_id, Some(super::FieldId(0)), "{}", function.name);
        }
    }
}
