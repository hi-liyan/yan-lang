//! 与 parser 和执行后端解耦的 Yan 高层中间表示。

use std::collections::HashMap;

use yan_source::Span;
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
    /// 当前 HIR 模块的稳定标识。
    pub id: ModuleId,
    /// 源文件声明的模块路径；M3 允许省略。
    pub module: Option<Vec<String>>,
    /// 显式导入的模块路径。
    pub imports: Vec<Vec<String>>,
    /// 源文件中的真正新类型声明。
    pub newtypes: Vec<Newtype>,
    /// 源文件中的结构体声明。
    pub structs: Vec<Struct>,
    /// 源文件中的封闭枚举声明。
    pub enums: Vec<Enum>,
    /// 程序定义的函数。
    pub functions: Vec<Function>,
}

/// 已 lowering 的真正新类型声明。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Newtype {
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
        /// 已解析的普通函数目标；内建调用和构造不使用此字段。
        function: Option<DefId>,
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
    let mut next_def = 0_u32;
    let mut next_field = 0_u32;
    let mut next_variant = 0_u32;
    let newtypes = program
        .newtypes
        .into_iter()
        .map(|newtype| {
            let id = DefId(next_def);
            next_def += 1;
            lower_newtype(newtype, id)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let structs = program
        .structs
        .into_iter()
        .map(|structure| {
            let id = DefId(next_def);
            next_def += 1;
            lower_struct(structure, id, &mut next_field)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let enums = program
        .enums
        .into_iter()
        .map(|enumeration| {
            let id = DefId(next_def);
            next_def += 1;
            lower_enum(enumeration, id, &mut next_variant)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let functions = program
        .functions
        .into_iter()
        .map(|function| {
            let id = DefId(next_def);
            next_def += 1;
            lower_function(function, id)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut lowered = Program {
        id: ModuleId(0),
        module: program.module.map(|path| path.segments),
        imports: program
            .imports
            .into_iter()
            .map(|import| import.path.segments)
            .collect(),
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
    let functions = program
        .functions
        .iter()
        .map(|function| (function.name.clone(), function.id))
        .collect::<HashMap<_, _>>();
    let structures = program
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
                        .map(|field| (field.name.clone(), field.id))
                        .collect::<HashMap<_, _>>(),
                ),
            )
        })
        .collect::<HashMap<_, _>>();
    let variants = program
        .enums
        .iter()
        .flat_map(|enumeration| {
            enumeration.variants.iter().map(move |variant| {
                (
                    (enumeration.name.clone(), variant.name.clone()),
                    variant.id,
                )
            })
        })
        .collect::<HashMap<_, _>>();

    for function in &mut program.functions {
        let mut locals = function
            .parameters
            .iter()
            .map(|parameter| (parameter.name.clone(), parameter.id))
            .collect::<HashMap<_, _>>();
        let mut next_local = function.parameters.len() as u32;
        resolve_statements(
            &mut function.statements,
            &mut locals,
            &mut next_local,
            &functions,
            &structures,
            &variants,
        );
    }
}

/// 解析一个顺序语句块，并把块内绑定限定在调用方传入的作用域中。
fn resolve_statements(
    statements: &mut [Statement],
    locals: &mut HashMap<String, LocalId>,
    next_local: &mut u32,
    functions: &HashMap<String, DefId>,
    structures: &HashMap<String, (DefId, HashMap<String, FieldId>)>,
    variants: &HashMap<(String, String), VariantId>,
) {
    for statement in statements {
        match statement {
            Statement::Destructure {
                locals: ids,
                names,
                value,
            } => {
                resolve_expression(value, locals, next_local, functions, structures, variants);
                *ids = names
                    .iter()
                    .map(|(name, _)| {
                        let id = allocate_local(next_local);
                        locals.insert(name.clone(), id);
                        id
                    })
                    .collect();
            }
            Statement::Let {
                local,
                name,
                value,
                ..
            } => {
                resolve_expression(value, locals, next_local, functions, structures, variants);
                let id = allocate_local(next_local);
                *local = id;
                locals.insert(name.clone(), id);
            }
            Statement::Assign { local, name, value, .. } => {
                resolve_expression(value, locals, next_local, functions, structures, variants);
                if let Some(id) = locals.get(name) {
                    *local = *id;
                }
            }
            Statement::Expression(value) => {
                resolve_expression(value, locals, next_local, functions, structures, variants);
            }
        }
    }
}

/// 递归回填表达式及嵌套语句块的引用 ID。
fn resolve_expression(
    expression: &mut Expression,
    locals: &HashMap<String, LocalId>,
    next_local: &mut u32,
    functions: &HashMap<String, DefId>,
    structures: &HashMap<String, (DefId, HashMap<String, FieldId>)>,
    variants: &HashMap<(String, String), VariantId>,
) {
    match expression {
        Expression::String { parts, .. } => {
            for part in parts {
                if let StringPart::Variable { local, name, .. } = part {
                    *local = locals.get(name).copied();
                }
            }
        }
        Expression::List { values, .. } | Expression::Tuple { values, .. } => {
            for value in values {
                resolve_expression(value, locals, next_local, functions, structures, variants);
            }
        }
        Expression::Map { entries, .. } => {
            for entry in entries {
                resolve_expression(
                    &mut entry.value,
                    locals,
                    next_local,
                    functions,
                    structures,
                    variants,
                );
            }
        }
        Expression::Match { target, arms, .. } => {
            resolve_expression(target, locals, next_local, functions, structures, variants);
            for arm in arms {
                arm.pattern.variant_id = variants
                    .get(&(arm.pattern.enum_name.clone(), arm.pattern.variant.clone()))
                    .copied();
                let mut arm_locals = locals.clone();
                if let Some((name, _)) = &arm.pattern.binding {
                    let id = allocate_local(next_local);
                    arm.pattern.binding_local = Some(id);
                    arm_locals.insert(name.clone(), id);
                }
                resolve_expression(
                    &mut arm.value,
                    &arm_locals,
                    next_local,
                    functions,
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
            resolve_expression(condition, locals, next_local, functions, structures, variants);
            let mut then_locals = locals.clone();
            resolve_statements(
                then_statements,
                &mut then_locals,
                next_local,
                functions,
                structures,
                variants,
            );
            let mut else_locals = locals.clone();
            resolve_statements(
                else_statements,
                &mut else_locals,
                next_local,
                functions,
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
            resolve_expression(iterable, locals, next_local, functions, structures, variants);
            let id = allocate_local(next_local);
            *local = id;
            let mut loop_locals = locals.clone();
            loop_locals.insert(name.clone(), id);
            resolve_statements(
                statements,
                &mut loop_locals,
                next_local,
                functions,
                structures,
                variants,
            );
        }
        Expression::Return { value, .. } | Expression::Try { value, .. } => {
            resolve_expression(value, locals, next_local, functions, structures, variants);
        }
        Expression::Variable { local, name, .. } => *local = locals.get(name).copied(),
        Expression::Call {
            function,
            path,
            arguments,
            ..
        } => {
            *function = (path.len() == 1)
                .then(|| functions.get(&path[0]).copied())
                .flatten();
            for argument in arguments {
                resolve_expression(
                    argument,
                    locals,
                    next_local,
                    functions,
                    structures,
                    variants,
                );
            }
        }
        Expression::Add { left, right, .. }
        | Expression::Multiply { left, right, .. }
        | Expression::Equal { left, right, .. } => {
            resolve_expression(left, locals, next_local, functions, structures, variants);
            resolve_expression(right, locals, next_local, functions, structures, variants);
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
                    field.field_id = declared_fields.get(&field.name).copied();
                    resolve_expression(
                        &mut field.value,
                        locals,
                        next_local,
                        functions,
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
            if let Expression::Variable { name, .. } = target.as_ref() {
                *variant = variants.get(&(name.clone(), field.clone())).copied();
            }
            // 字段访问的结构体类型由 typeck 确定；此处仍应解析目标的局部读取。
            resolve_expression(target, locals, next_local, functions, structures, variants);
            let _ = field_id;
        }
        Expression::Integer { .. }
        | Expression::Float { .. }
        | Expression::Boolean { .. } => {}
    }
}

/// 分配函数内唯一局部 ID，避免嵌套块和 match 分支复用同一槽位。
fn allocate_local(next_local: &mut u32) -> LocalId {
    let id = LocalId(*next_local);
    *next_local += 1;
    id
}

fn lower_newtype(newtype: yan_syntax::Newtype, id: DefId) -> Result<Newtype, LowerError> {
    Ok(Newtype {
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
    next_field: &mut u32,
) -> Result<Struct, LowerError> {
    Ok(Struct {
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
                lower_declared_field(field, id)
            })
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn lower_enum(
    enumeration: SyntaxEnum,
    id: DefId,
    next_variant: &mut u32,
) -> Result<Enum, LowerError> {
    Ok(Enum {
        id,
        public: enumeration.public,
        name: enumeration.name,
        name_span: enumeration.name_span,
        variants: enumeration
            .variants
            .into_iter()
            .map(|variant| {
                Ok(EnumVariant {
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

fn lower_declared_field(field: SyntaxField, id: FieldId) -> Result<Field, LowerError> {
    let Some(ty) = field.ty else {
        return Err(LowerError {
            span: field.name_span,
            message: "struct field is missing a type".to_owned(),
        });
    };
    Ok(Field {
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
    pub span: Span,
    /// 面向用户的错误原因。
    pub message: String,
}

fn lower_function(function: yan_syntax::Function, id: DefId) -> Result<Function, LowerError> {
    Ok(Function {
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
        span: ty.span,
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
                    span: ty.span,
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
            function: None,
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
            span: field.name_span,
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
        span: Span::new(span.start + start + 1, span.start + end + 1),
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

    use super::{lower, Expression, Statement, StringPart};

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
}
