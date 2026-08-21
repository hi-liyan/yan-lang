//! 已类型化 Yan 程序到目标无关控制流中间表示的 lowering。

use std::collections::{HashMap, HashSet};

pub use yan_hir::{DefId, FieldId, LocalId, Type, VariantId};
pub use yan_source::{SourceId, SourceLocation, Span};
use yan_typeck::{
    TypedCallTarget, TypedExpression, TypedExpressionKind, TypedFunction, TypedPattern,
    TypedProgram, TypedStatement, TypedStringPart, TypedStruct,
};

/// MIR 程序。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Program {
    /// 入口模块所属的源文件。
    pub source: SourceId,
    /// 按稳定声明 ID 排列的函数控制流图。
    pub functions: Vec<Function>,
    /// 结构体布局元数据。
    pub structs: Vec<Struct>,
    /// 枚举布局元数据。
    pub enums: Vec<Enum>,
    /// 新类型布局元数据。
    pub newtypes: Vec<Newtype>,
}

/// 已通过 MIR 验证的只读程序。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedProgram(Program);

impl VerifiedProgram {
    /// 返回经验证的函数列表。
    pub fn functions(&self) -> &[Function] {
        &self.0.functions
    }

    /// 返回结构体布局列表。
    pub fn structs(&self) -> &[Struct] {
        &self.0.structs
    }

    /// 返回枚举布局列表。
    pub fn enums(&self) -> &[Enum] {
        &self.0.enums
    }

    /// 返回新类型布局列表。
    pub fn newtypes(&self) -> &[Newtype] {
        &self.0.newtypes
    }

    /// 返回程序默认诊断所属的源文件。
    pub fn source(&self) -> SourceId {
        self.0.source
    }
}

/// MIR lowering 发现的内部结构错误。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LowerError {
    /// 错误对应的完整源位置。
    pub location: SourceLocation,
    /// 不包含实现细节的稳定英文原因。
    pub message: String,
}

/// MIR 验证失败的稳定诊断。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifyError {
    /// 违反不变量的完整源位置。
    pub location: SourceLocation,
    /// 面向用户的稳定英文错误原因。
    pub message: String,
}

/// MIR 函数的稳定声明 ID。
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FunctionId(
    /// 对应 HIR 顶层函数声明的编译会话 ID。
    pub DefId,
);

/// 函数内等于块数组下标的稳定基本块 ID。
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BasicBlockId(
    /// 所属函数基本块数组中的零起始下标。
    pub u32,
);

/// 函数内由指令定义的稳定临时值 ID。
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ValueId(
    /// 所属函数内按定义顺序分配的零起始编号。
    pub u32,
);

/// 结构体运行时布局。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Struct {
    /// 结构体声明 ID。
    pub id: DefId,
    /// 结构体名义类型；验证器通过声明 ID 取得该类型，不执行名称查找。
    pub ty: Type,
    /// 按声明顺序排列的字段布局。
    pub fields: Vec<Field>,
}

/// 结构体字段运行时布局。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Field {
    /// 字段声明 ID。
    pub id: FieldId,
    /// 字段的 Yan 类型。
    pub ty: Type,
}

/// 枚举运行时布局。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Enum {
    /// 枚举声明 ID。
    pub id: DefId,
    /// 枚举名义类型；验证器通过声明 ID 取得该类型，不执行名称查找。
    pub ty: Type,
    /// 按声明顺序排列的变体布局。
    pub variants: Vec<Variant>,
}

/// 枚举变体运行时布局。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Variant {
    /// 变体声明 ID。
    pub id: VariantId,
    /// 单载荷类型；无载荷变体为 `None`。
    pub payload: Option<Type>,
}

/// 新类型运行时布局。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Newtype {
    /// 新类型声明 ID。
    pub id: DefId,
    /// 新类型的名义结果类型。
    pub ty: Type,
    /// 运行时透明保存的底层类型。
    pub underlying: Type,
}

/// 已降低函数的完整控制流图。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Function {
    /// 函数声明 ID。
    pub id: FunctionId,
    /// 仅用于入口选择和调试显示的源名称。
    pub name: String,
    /// 函数声明的完整源位置。
    pub location: SourceLocation,
    /// 函数返回类型。
    pub return_type: Type,
    /// 按声明顺序排列的参数局部位置。
    pub parameters: Vec<Local>,
    /// 参数和函数体局部位置的完整集合。
    pub locals: Vec<Local>,
    /// 从 `BasicBlockId(0)` 开始的基本块图。
    pub blocks: Vec<BasicBlock>,
}

/// MIR 局部存储位置。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Local {
    /// 函数内稳定局部 ID。
    pub id: LocalId,
    /// 局部值的 Yan 类型。
    pub ty: Type,
    /// 是否允许初始化后的再次写入。
    pub mutable: bool,
    /// 声明的完整源位置。
    pub location: SourceLocation,
}

/// 一段顺序执行并以唯一终结指令离开的 MIR 基本块。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BasicBlock {
    /// 基本块 ID。
    pub id: BasicBlockId,
    /// 按源码求值顺序排列的无控制流指令。
    pub instructions: Vec<Instruction>,
    /// 当前块唯一的控制流出口。
    pub terminator: Terminator,
}

/// 无需进入新基本块即可读取的指令输入。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Operand {
    /// 已初始化的源程序局部位置。
    Local(LocalId),
    /// 先前 MIR 指令定义的临时值。
    Value(ValueId),
    /// 无副作用且无需名称查找的常量。
    Constant(Constant),
}

/// MIR 可直接物化的常量。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Constant {
    /// 64 位有符号整数。
    Integer(i64),
    /// 保留规范源文本的浮点值。
    Float(String),
    /// 布尔值。
    Boolean(bool),
    /// UTF-8 字符串。
    String(String),
    /// `unit` 值。
    Unit,
    /// 无载荷 `None` 值，具体 Option 类型由消费位置给出。
    None,
    /// 已解析的无载荷用户枚举变体。
    Variant(VariantId),
}

/// 字符串构造中的静态文本或已求值插值值。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StringPart {
    /// 原样写入的静态文本。
    Text(String),
    /// 按 Yan 显示规则格式化的操作数。
    Value(Operand),
}

/// 已解析且不需要运行时名称查找的调用目标。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CallTarget {
    /// 用户函数声明。
    Function(DefId),
    /// 运行时透明的新类型构造器。
    Newtype(DefId),
    /// 有载荷用户枚举变体构造器。
    Variant(VariantId),
    /// 内建 `Some` 构造器。
    Some,
    /// 内建 `Ok` 构造器。
    Ok,
    /// 内建 `Err` 构造器。
    Err,
    /// 内建 `bytes.from_hex` 函数。
    BytesFromHex,
    /// 平台 `console.println` 函数。
    ConsolePrintln,
    /// `string.to_int` 及其已解析接收者局部。
    StringToInt(LocalId),
}

/// MIR 支持的二元操作。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BinaryOperator {
    /// 整数加法。
    Add,
    /// 整数乘法。
    Multiply,
    /// 同类型基础值相等比较。
    Equal,
}

/// MIR 基本块内按顺序执行的类型化指令。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Instruction {
    /// 将操作数复制到新临时值。
    Assign {
        /// 新定义的临时值。
        destination: ValueId,
        /// 被复制的输入。
        operand: Operand,
        /// 结果类型。
        ty: Type,
        /// 指令源位置。
        location: SourceLocation,
    },
    /// 初始化或覆盖源程序局部位置。
    StoreLocal {
        /// 被写入的局部 ID。
        local: LocalId,
        /// 已求值输入。
        value: Operand,
        /// 写入值类型。
        ty: Type,
        /// 写入源位置。
        location: SourceLocation,
    },
    /// 执行类型化二元运算。
    Binary {
        /// 运算结果临时值。
        destination: ValueId,
        /// 已确定的运算符。
        operator: BinaryOperator,
        /// 左操作数。
        left: Operand,
        /// 右操作数。
        right: Operand,
        /// 结果类型。
        ty: Type,
        /// 表达式源位置。
        location: SourceLocation,
    },
    /// 按片段顺序构造插值字符串。
    BuildString {
        /// 构造结果临时值。
        destination: ValueId,
        /// 静态文本和插值操作数。
        parts: Vec<StringPart>,
        /// 固定为 `string` 的结果类型。
        ty: Type,
        /// 表达式源位置。
        location: SourceLocation,
    },
    /// 按求值顺序构造不可变列表。
    BuildList {
        /// 构造结果临时值。
        destination: ValueId,
        /// 已求值元素。
        elements: Vec<Operand>,
        /// 完整 List 类型。
        ty: Type,
        /// 表达式源位置。
        location: SourceLocation,
    },
    /// 按求值顺序构造不可变 map。
    BuildMap {
        /// 构造结果临时值。
        destination: ValueId,
        /// 静态字符串键和已求值值。
        entries: Vec<(String, Operand)>,
        /// 完整 Map 类型。
        ty: Type,
        /// 表达式源位置。
        location: SourceLocation,
    },
    /// 按求值顺序构造元组。
    BuildTuple {
        /// 构造结果临时值。
        destination: ValueId,
        /// 已求值元素。
        elements: Vec<Operand>,
        /// 完整元组类型。
        ty: Type,
        /// 表达式源位置。
        location: SourceLocation,
    },
    /// 从已验证元组读取固定位置元素。
    TupleElement {
        /// 元素结果临时值。
        destination: ValueId,
        /// 已求值元组。
        tuple: Operand,
        /// 零起始元素位置。
        index: u32,
        /// 元素类型。
        ty: Type,
        /// 解构源位置。
        location: SourceLocation,
    },
    /// 构造已解析结构体值。
    BuildStruct {
        /// 构造结果临时值。
        destination: ValueId,
        /// 结构体声明 ID。
        structure: DefId,
        /// 按源码顺序排列的字段和值。
        fields: Vec<(FieldId, Operand)>,
        /// 结构体名义类型。
        ty: Type,
        /// 构造源位置。
        location: SourceLocation,
    },
    /// 从结构体值读取已解析字段。
    LoadField {
        /// 字段结果临时值。
        destination: ValueId,
        /// 结构体值。
        target: Operand,
        /// 字段声明 ID。
        field: FieldId,
        /// 字段类型。
        ty: Type,
        /// 访问源位置。
        location: SourceLocation,
    },
    /// 调用已解析函数、构造器或内建函数。
    Call {
        /// 调用结果临时值，包括 unit 结果。
        destination: ValueId,
        /// 已解析调用目标。
        target: CallTarget,
        /// 按源码顺序求值的实参。
        arguments: Vec<Operand>,
        /// 调用结果类型。
        ty: Type,
        /// 调用源位置。
        location: SourceLocation,
    },
    /// 在汇合块选择前驱产生的值。
    Phi {
        /// 汇合后的新临时值。
        destination: ValueId,
        /// 前驱块及其提供的操作数。
        incoming: Vec<(BasicBlockId, Operand)>,
        /// 各输入共同类型。
        ty: Type,
        /// 控制流表达式源位置。
        location: SourceLocation,
    },
    /// 为 List 创建 MIR 内部遍历状态。
    IterInit {
        /// 内部遍历状态临时值。
        destination: ValueId,
        /// 已求值 List。
        iterable: Operand,
        /// 完整 List 类型。
        ty: Type,
        /// for 表达式源位置。
        location: SourceLocation,
    },
    /// 推进内部遍历状态并读取下一元素。
    IterNext {
        /// 被原地推进的遍历状态。
        iterator: ValueId,
        /// 当前元素结果临时值。
        item_destination: ValueId,
        /// 是否存在元素的 bool 临时值。
        has_value_destination: ValueId,
        /// 元素类型。
        item_ty: Type,
        /// for 表达式源位置。
        location: SourceLocation,
    },
}

/// match 分派支持的已解析模式。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MatchPattern {
    /// 用户 enum 变体。
    Variant(VariantId),
    /// 内建 Some 变体。
    Some,
    /// 内建 None 变体。
    None,
    /// 内建 Ok 变体。
    Ok,
    /// 内建 Err 变体。
    Err,
}

/// match 终结指令的一条已解析出边。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MatchTarget {
    /// 选择此出边的模式。
    pub pattern: MatchPattern,
    /// 进入目标块前接收载荷的可选局部。
    pub binding: Option<LocalId>,
    /// 模式匹配时进入的块。
    pub block: BasicBlockId,
}

/// MIR 基本块唯一的控制流出口。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Terminator {
    /// 无条件进入目标块。
    Goto {
        /// 后继块。
        target: BasicBlockId,
        /// 控制流源位置。
        location: SourceLocation,
    },
    /// 根据 bool 选择两个后继之一。
    Branch {
        /// 已验证 bool 条件。
        condition: Operand,
        /// 条件为真的后继。
        then_block: BasicBlockId,
        /// 条件为假的后继。
        else_block: BasicBlockId,
        /// 控制流源位置。
        location: SourceLocation,
    },
    /// 根据 enum、Option 或 Result 变体选择后继。
    Match {
        /// 已求值目标。
        target: Operand,
        /// 按源码顺序排列的模式出边。
        arms: Vec<MatchTarget>,
        /// 无模式匹配时的不可达块。
        otherwise: BasicBlockId,
        /// match 源位置。
        location: SourceLocation,
    },
    /// 从当前函数返回。
    Return {
        /// unit 返回为 None，其他返回为已求值操作数。
        value: Option<Operand>,
        /// 返回源位置。
        location: SourceLocation,
    },
    /// 对 Result 执行错误传播。
    PropagateErr {
        /// 已求值 Result。
        result: Operand,
        /// 成功路径接收 Ok 载荷的临时值。
        destination: ValueId,
        /// Ok 时进入的后继。
        success: BasicBlockId,
        /// Ok 载荷类型。
        ty: Type,
        /// 问号表达式源位置。
        location: SourceLocation,
    },
    /// 表示类型检查已证明无法正常进入的路径。
    Unreachable {
        /// 不可达控制流源位置。
        location: SourceLocation,
    },
}

/// 将 Typed HIR 降低为只含操作数、指令和终结指令的 MIR。
pub fn lower(typed: TypedProgram) -> Result<Program, LowerError> {
    let functions = typed
        .functions
        .iter()
        .map(|function| lower_function(function, &typed.structs))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Program {
        source: typed.source,
        functions,
        structs: typed
            .structs
            .iter()
            .map(|item| Struct {
                id: item.id,
                ty: Type::Named(item.name.clone()),
                fields: item
                    .fields
                    .iter()
                    .map(|field| Field {
                        id: field.id,
                        ty: field.ty.clone(),
                    })
                    .collect(),
            })
            .collect(),
        enums: typed
            .enums
            .iter()
            .map(|item| Enum {
                id: item.id,
                ty: Type::Named(item.name.clone()),
                variants: item
                    .variants
                    .iter()
                    .map(|variant| Variant {
                        id: variant.id,
                        payload: variant.payload.clone(),
                    })
                    .collect(),
            })
            .collect(),
        newtypes: typed
            .newtypes
            .iter()
            .map(|item| Newtype {
                id: item.id,
                ty: Type::Named(item.name.clone()),
                underlying: item.underlying.clone(),
            })
            .collect(),
    })
}

/// 验证 MIR 的控制流、值流、局部位置、声明目标和 Yan 类型不变量。
///
/// 验证只读取稳定 ID 与 lowering 附带的声明元数据，不按源码名称重新解析任何目标。
/// 成功返回的不透明包装是解释器和后端可以消费 MIR 的唯一入口。
pub fn verify(program: Program) -> Result<VerifiedProgram, VerifyError> {
    let declarations = Declarations::collect(&program)?;
    for function in &program.functions {
        FunctionVerifier::new(function, &declarations)?.verify()?;
    }
    Ok(VerifiedProgram(program))
}

struct Declarations<'a> {
    functions: HashMap<DefId, &'a Function>,
    structs: HashMap<DefId, &'a Struct>,
    fields: HashMap<FieldId, (&'a Struct, &'a Field)>,
    variants: HashMap<VariantId, (&'a Enum, &'a Variant)>,
    enums: Vec<&'a Enum>,
    newtypes: HashMap<DefId, &'a Newtype>,
}

impl<'a> Declarations<'a> {
    fn collect(program: &'a Program) -> Result<Self, VerifyError> {
        let fallback = SourceLocation::new(program.source, Span::default());
        let mut top_level = HashSet::new();
        let mut functions = HashMap::new();
        for function in &program.functions {
            if !top_level.insert(function.id.0)
                || functions.insert(function.id.0, function).is_some()
            {
                return Err(verify_error(
                    function.location,
                    "duplicate MIR declaration ID",
                ));
            }
        }
        let mut structs = HashMap::new();
        let mut fields = HashMap::new();
        for structure in &program.structs {
            if !top_level.insert(structure.id) || structs.insert(structure.id, structure).is_some()
            {
                return Err(verify_error(fallback, "duplicate MIR declaration ID"));
            }
            for field in &structure.fields {
                if fields.insert(field.id, (structure, field)).is_some() {
                    return Err(verify_error(fallback, "duplicate MIR field ID"));
                }
            }
        }
        let mut enums = HashMap::new();
        let mut variants = HashMap::new();
        for enumeration in &program.enums {
            if !top_level.insert(enumeration.id)
                || enums.insert(enumeration.id, enumeration).is_some()
            {
                return Err(verify_error(fallback, "duplicate MIR declaration ID"));
            }
            for variant in &enumeration.variants {
                if variants
                    .insert(variant.id, (enumeration, variant))
                    .is_some()
                {
                    return Err(verify_error(fallback, "duplicate MIR variant ID"));
                }
            }
        }
        let mut newtypes = HashMap::new();
        for newtype in &program.newtypes {
            if !top_level.insert(newtype.id) || newtypes.insert(newtype.id, newtype).is_some() {
                return Err(verify_error(fallback, "duplicate MIR declaration ID"));
            }
        }
        Ok(Self {
            functions,
            structs,
            fields,
            variants,
            enums: enums.into_values().collect(),
            newtypes,
        })
    }
}

#[derive(Clone, Copy)]
enum DefinitionPosition {
    Phi,
    Instruction(usize),
    Terminator,
}

struct ValueDefinition {
    block: BasicBlockId,
    position: DefinitionPosition,
    ty: Type,
}

struct FunctionVerifier<'a> {
    function: &'a Function,
    declarations: &'a Declarations<'a>,
    locals: HashMap<LocalId, &'a Local>,
    definitions: HashMap<ValueId, ValueDefinition>,
    predecessors: Vec<Vec<BasicBlockId>>,
    dominators: Vec<HashSet<BasicBlockId>>,
    reachable: Vec<bool>,
    match_bindings: HashMap<(BasicBlockId, BasicBlockId), Vec<LocalId>>,
}

impl<'a> FunctionVerifier<'a> {
    fn new(
        function: &'a Function,
        declarations: &'a Declarations<'a>,
    ) -> Result<Self, VerifyError> {
        if function.blocks.is_empty() {
            return Err(verify_error(
                function.location,
                "MIR function has no entry block",
            ));
        }
        for (expected, block) in function.blocks.iter().enumerate() {
            if block.id != BasicBlockId(expected as u32) {
                return Err(verify_error(function.location, "invalid MIR block ID"));
            }
        }

        let mut locals = HashMap::new();
        for local in &function.locals {
            if locals.insert(local.id, local).is_some() {
                return Err(verify_error(local.location, "duplicate MIR local ID"));
            }
        }
        let mut parameters = HashSet::new();
        for parameter in &function.parameters {
            if !parameters.insert(parameter.id) {
                return Err(verify_error(
                    parameter.location,
                    "duplicate MIR parameter ID",
                ));
            }
            let Some(local) = locals.get(&parameter.id) else {
                return Err(verify_error(
                    parameter.location,
                    "MIR parameter is missing from locals",
                ));
            };
            if *local != parameter {
                return Err(verify_error(
                    parameter.location,
                    "MIR parameter does not match its local",
                ));
            }
        }

        let mut predecessors = vec![Vec::new(); function.blocks.len()];
        let mut match_bindings = HashMap::<_, Vec<_>>::new();
        for block in &function.blocks {
            for target in terminator_targets(&block.terminator) {
                if target.0 as usize >= function.blocks.len() {
                    return Err(verify_error(
                        terminator_location(&block.terminator),
                        "invalid MIR jump target",
                    ));
                }
                predecessors[target.0 as usize].push(block.id);
            }
            if let Terminator::Match {
                arms,
                otherwise,
                location,
                ..
            } = &block.terminator
            {
                let mut patterns = HashSet::new();
                let mut target_bindings = HashMap::new();
                for arm in arms {
                    if arm.block == *otherwise {
                        return Err(verify_error(
                            *location,
                            "MIR match arm targets otherwise block",
                        ));
                    }
                    if !patterns.insert(arm.pattern) {
                        return Err(verify_error(*location, "duplicate MIR match pattern"));
                    }
                    if let Some(previous) = target_bindings.insert(arm.block, arm.binding) {
                        if previous != arm.binding {
                            return Err(verify_error(
                                *location,
                                "ambiguous MIR match target binding",
                            ));
                        }
                    }
                    if let Some(binding) = arm.binding {
                        match_bindings
                            .entry((block.id, arm.block))
                            .or_default()
                            .push(binding);
                    }
                }
                let fallback = &function.blocks[otherwise.0 as usize];
                if !matches!(fallback.terminator, Terminator::Unreachable { .. }) {
                    return Err(verify_error(
                        *location,
                        "MIR match otherwise block must be unreachable",
                    ));
                }
            }
        }

        let definitions = collect_value_definitions(function)?;
        let reachable = compute_reachable(function);
        let dominators = compute_dominators(function.blocks.len(), &predecessors, &reachable);
        Ok(Self {
            function,
            declarations,
            locals,
            definitions,
            predecessors,
            dominators,
            reachable,
            match_bindings,
        })
    }

    fn verify(&self) -> Result<(), VerifyError> {
        self.verify_immutable_writes()?;
        let initialized = self.compute_initialized_locals();
        for block in &self.function.blocks {
            let mut block_initialized = initialized[block.id.0 as usize].clone();
            let mut saw_non_phi = false;
            for (index, instruction) in block.instructions.iter().enumerate() {
                if matches!(instruction, Instruction::Phi { .. }) {
                    if saw_non_phi {
                        return Err(verify_error(
                            instruction_location(instruction),
                            "MIR phi follows a non-phi instruction",
                        ));
                    }
                } else {
                    saw_non_phi = true;
                }
                self.verify_instruction(block, index, instruction, &block_initialized)?;
                if let Instruction::StoreLocal { local, .. } = instruction {
                    block_initialized.insert(*local);
                }
            }
            self.verify_terminator(block, &block_initialized)?;
        }
        Ok(())
    }

    fn verify_immutable_writes(&self) -> Result<(), VerifyError> {
        let parameters = self
            .function
            .parameters
            .iter()
            .map(|parameter| parameter.id)
            .collect::<HashSet<_>>();
        let bound = self
            .match_bindings
            .values()
            .flatten()
            .copied()
            .collect::<HashSet<_>>();
        let mut stores = HashSet::new();
        for instruction in self
            .function
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
        {
            let Instruction::StoreLocal {
                local, location, ..
            } = instruction
            else {
                continue;
            };
            let Some(binding) = self.locals.get(local) else {
                return Err(verify_error(
                    *location,
                    "MIR store targets an undefined local",
                ));
            };
            if !binding.mutable
                && (parameters.contains(local) || bound.contains(local) || !stores.insert(*local))
            {
                return Err(verify_error(*location, "MIR writes an immutable local"));
            }
            stores.insert(*local);
        }
        Ok(())
    }

    fn compute_initialized_locals(&self) -> Vec<HashSet<LocalId>> {
        let all = self.locals.keys().copied().collect::<HashSet<_>>();
        let parameters = self
            .function
            .parameters
            .iter()
            .map(|parameter| parameter.id)
            .collect::<HashSet<_>>();
        let mut entries = vec![all; self.function.blocks.len()];
        entries[0] = parameters;
        let mut changed = true;
        while changed {
            changed = false;
            for block_index in 1..self.function.blocks.len() {
                if !self.reachable[block_index] {
                    entries[block_index].clear();
                    continue;
                }
                let block_id = BasicBlockId(block_index as u32);
                let predecessors = &self.predecessors[block_index];
                let next = if predecessors.is_empty() {
                    HashSet::new()
                } else {
                    let mut incoming = predecessors.iter().map(|predecessor| {
                        let mut state = entries[predecessor.0 as usize].clone();
                        for instruction in
                            &self.function.blocks[predecessor.0 as usize].instructions
                        {
                            if let Instruction::StoreLocal { local, .. } = instruction {
                                state.insert(*local);
                            }
                        }
                        if let Some(bindings) = self.match_bindings.get(&(*predecessor, block_id)) {
                            state.extend(bindings);
                        }
                        state
                    });
                    let mut intersection = incoming.next().unwrap_or_default();
                    for state in incoming {
                        intersection.retain(|local| state.contains(local));
                    }
                    intersection
                };
                if entries[block_index] != next {
                    entries[block_index] = next;
                    changed = true;
                }
            }
        }
        entries
    }

    fn verify_instruction(
        &self,
        block: &BasicBlock,
        index: usize,
        instruction: &Instruction,
        initialized: &HashSet<LocalId>,
    ) -> Result<(), VerifyError> {
        let location = instruction_location(instruction);
        match instruction {
            Instruction::Assign { operand, ty, .. } => {
                self.expect_operand_type(block.id, index, operand, ty, initialized, location)
                    .map_err(|error| {
                        remap_type_error(error, "MIR operand type does not match instruction type")
                    })?;
            }
            Instruction::StoreLocal {
                local, value, ty, ..
            } => {
                let Some(binding) = self.locals.get(local) else {
                    return Err(verify_error(
                        location,
                        "MIR store targets an undefined local",
                    ));
                };
                if &binding.ty != ty {
                    return Err(verify_error(
                        location,
                        "MIR store type does not match local type",
                    ));
                }
                self.expect_operand_type(block.id, index, value, ty, initialized, location)
                    .map_err(|error| {
                        remap_type_error(error, "MIR store value type does not match local type")
                    })?;
            }
            Instruction::Binary {
                operator,
                left,
                right,
                ty,
                ..
            } => {
                let (operand_ty, result_ty) = match operator {
                    BinaryOperator::Add | BinaryOperator::Multiply => (Type::Int, Type::Int),
                    BinaryOperator::Equal => {
                        let left_ty =
                            self.operand_type(block.id, index, left, None, initialized, location)?;
                        if !matches!(left_ty, Type::Int | Type::Bool | Type::String) {
                            return Err(verify_error(location, "invalid MIR binary operand type"));
                        }
                        (left_ty, Type::Bool)
                    }
                };
                self.expect_operand_type(block.id, index, left, &operand_ty, initialized, location)
                    .map_err(|error| remap_type_error(error, "invalid MIR binary operand type"))?;
                self.expect_operand_type(
                    block.id,
                    index,
                    right,
                    &operand_ty,
                    initialized,
                    location,
                )
                .map_err(|error| remap_type_error(error, "invalid MIR binary operand type"))?;
                if ty != &result_ty {
                    return Err(verify_error(location, "invalid MIR binary result type"));
                }
            }
            Instruction::BuildString { parts, ty, .. } => {
                if ty != &Type::String {
                    return Err(verify_error(location, "invalid MIR string result type"));
                }
                for part in parts {
                    if let StringPart::Value(value) = part {
                        self.operand_type(block.id, index, value, None, initialized, location)?;
                    }
                }
            }
            Instruction::BuildList { elements, ty, .. } => {
                let Type::List(element_ty) = ty else {
                    return Err(verify_error(location, "invalid MIR list result type"));
                };
                for element in elements {
                    self.expect_operand_type(
                        block.id,
                        index,
                        element,
                        element_ty,
                        initialized,
                        location,
                    )?;
                }
            }
            Instruction::BuildMap { entries, ty, .. } => {
                let Type::Map(value_ty) = ty else {
                    return Err(verify_error(location, "invalid MIR map result type"));
                };
                for (_, value) in entries {
                    self.expect_operand_type(
                        block.id,
                        index,
                        value,
                        value_ty,
                        initialized,
                        location,
                    )?;
                }
            }
            Instruction::BuildTuple { elements, ty, .. } => {
                let Type::Tuple(element_types) = ty else {
                    return Err(verify_error(location, "invalid MIR tuple result type"));
                };
                if elements.len() != element_types.len() {
                    return Err(verify_error(location, "invalid MIR tuple arity"));
                }
                for (element, element_ty) in elements.iter().zip(element_types) {
                    self.expect_operand_type(
                        block.id,
                        index,
                        element,
                        element_ty,
                        initialized,
                        location,
                    )?;
                }
            }
            Instruction::TupleElement {
                tuple,
                index: tuple_index,
                ty,
                ..
            } => {
                let tuple_ty =
                    self.operand_type(block.id, index, tuple, None, initialized, location)?;
                let Type::Tuple(elements) = tuple_ty else {
                    return Err(verify_error(location, "invalid MIR tuple operand type"));
                };
                if elements.get(*tuple_index as usize) != Some(ty) {
                    return Err(verify_error(location, "invalid MIR tuple element type"));
                }
            }
            Instruction::BuildStruct {
                structure,
                fields,
                ty,
                ..
            } => self.verify_struct_build(
                block.id,
                index,
                *structure,
                fields,
                ty,
                initialized,
                location,
            )?,
            Instruction::LoadField {
                target, field, ty, ..
            } => {
                let Some((structure, declaration)) = self.declarations.fields.get(field) else {
                    return Err(verify_error(location, "invalid MIR field target ID"));
                };
                self.expect_operand_type(
                    block.id,
                    index,
                    target,
                    &structure.ty,
                    initialized,
                    location,
                )?;
                if ty != &declaration.ty {
                    return Err(verify_error(location, "invalid MIR field result type"));
                }
            }
            Instruction::Call {
                target,
                arguments,
                ty,
                ..
            } => self.verify_call(
                block.id,
                index,
                *target,
                arguments,
                ty,
                initialized,
                location,
            )?,
            Instruction::Phi { incoming, ty, .. } => {
                self.verify_phi(block, incoming, ty, initialized, location)?;
            }
            Instruction::IterInit { iterable, ty, .. } => {
                if !matches!(ty, Type::List(_)) {
                    return Err(verify_error(location, "invalid MIR iterator type"));
                }
                self.expect_operand_type(block.id, index, iterable, ty, initialized, location)?;
            }
            Instruction::IterNext {
                iterator, item_ty, ..
            } => {
                self.expect_value_type_at(
                    block.id,
                    index,
                    *iterator,
                    &Type::List(Box::new(item_ty.clone())),
                    location,
                )?;
            }
        }
        Ok(())
    }

    fn verify_terminator(
        &self,
        block: &BasicBlock,
        initialized: &HashSet<LocalId>,
    ) -> Result<(), VerifyError> {
        let location = terminator_location(&block.terminator);
        let index = block.instructions.len();
        match &block.terminator {
            Terminator::Goto { .. } | Terminator::Unreachable { .. } => {}
            Terminator::Branch { condition, .. } => {
                self.expect_operand_type(
                    block.id,
                    index,
                    condition,
                    &Type::Bool,
                    initialized,
                    location,
                )
                .map_err(|error| remap_type_error(error, "invalid MIR branch condition type"))?;
            }
            Terminator::Match { target, arms, .. } => {
                let target_ty =
                    self.operand_type(block.id, index, target, None, initialized, location)?;
                self.verify_match_target(&target_ty, location)?;
                for arm in arms {
                    self.verify_match_arm(&target_ty, arm, location)?;
                }
                self.verify_match_exhaustive(&target_ty, arms, location)?;
            }
            Terminator::Return { value, .. } => match (&self.function.return_type, value) {
                (Type::Unit, None) => {}
                (Type::Unit, Some(_)) | (_, None) => {
                    return Err(verify_error(location, "invalid MIR return value"));
                }
                (return_ty, Some(value)) => {
                    let actual = self
                        .operand_type(
                            block.id,
                            index,
                            value,
                            Some(return_ty),
                            initialized,
                            location,
                        )
                        .map_err(|error| remap_type_error(error, "invalid MIR return type"))?;
                    if !types_compatible(&actual, return_ty) {
                        return Err(verify_error(location, "invalid MIR return type"));
                    }
                }
            },
            Terminator::PropagateErr { result, ty, .. } => {
                let result_ty =
                    self.operand_type(block.id, index, result, None, initialized, location)?;
                let Type::Result(ok, error) = result_ty else {
                    return Err(verify_error(
                        location,
                        "invalid MIR propagation operand type",
                    ));
                };
                if ok.as_ref() != ty {
                    return Err(verify_error(
                        location,
                        "invalid MIR propagation result type",
                    ));
                }
                let Type::Result(_, function_error) = &self.function.return_type else {
                    return Err(verify_error(
                        location,
                        "invalid MIR propagation return type",
                    ));
                };
                if function_error.as_ref() != error.as_ref() {
                    return Err(verify_error(location, "invalid MIR propagation error type"));
                }
            }
        }
        Ok(())
    }

    fn verify_struct_build(
        &self,
        block: BasicBlockId,
        index: usize,
        structure_id: DefId,
        fields: &[(FieldId, Operand)],
        ty: &Type,
        initialized: &HashSet<LocalId>,
        location: SourceLocation,
    ) -> Result<(), VerifyError> {
        let Some(structure) = self.declarations.structs.get(&structure_id) else {
            return Err(verify_error(location, "invalid MIR struct target ID"));
        };
        if ty != &structure.ty {
            return Err(verify_error(location, "invalid MIR struct result type"));
        }
        if fields.len() != structure.fields.len() {
            return Err(verify_error(location, "invalid MIR struct field count"));
        }
        let mut seen = HashSet::new();
        for (field_id, value) in fields {
            if !seen.insert(*field_id) {
                return Err(verify_error(location, "duplicate MIR struct field ID"));
            }
            let Some((owner, field)) = self.declarations.fields.get(field_id) else {
                return Err(verify_error(location, "invalid MIR field target ID"));
            };
            if owner.id != structure_id {
                return Err(verify_error(
                    location,
                    "MIR field belongs to another struct",
                ));
            }
            self.expect_operand_type(block, index, value, &field.ty, initialized, location)?;
        }
        Ok(())
    }

    fn verify_call(
        &self,
        block: BasicBlockId,
        index: usize,
        target: CallTarget,
        arguments: &[Operand],
        result_ty: &Type,
        initialized: &HashSet<LocalId>,
        location: SourceLocation,
    ) -> Result<(), VerifyError> {
        let (parameter_types, expected_result) = match target {
            CallTarget::Function(id) => {
                let Some(function) = self.declarations.functions.get(&id) else {
                    return Err(verify_error(location, "invalid MIR function target ID"));
                };
                (
                    function
                        .parameters
                        .iter()
                        .map(|parameter| parameter.ty.clone())
                        .collect(),
                    function.return_type.clone(),
                )
            }
            CallTarget::Newtype(id) => {
                let Some(newtype) = self.declarations.newtypes.get(&id) else {
                    return Err(verify_error(location, "invalid MIR newtype target ID"));
                };
                (vec![newtype.underlying.clone()], newtype.ty.clone())
            }
            CallTarget::Variant(id) => {
                let Some((enumeration, variant)) = self.declarations.variants.get(&id) else {
                    return Err(verify_error(location, "invalid MIR variant target ID"));
                };
                (
                    variant.payload.iter().cloned().collect(),
                    enumeration.ty.clone(),
                )
            }
            CallTarget::Some => {
                let Type::Option(payload) = result_ty else {
                    return Err(verify_error(location, "invalid MIR call result type"));
                };
                (vec![payload.as_ref().clone()], result_ty.clone())
            }
            CallTarget::Ok => {
                let Type::Result(ok, _) = result_ty else {
                    return Err(verify_error(location, "invalid MIR call result type"));
                };
                (vec![ok.as_ref().clone()], result_ty.clone())
            }
            CallTarget::Err => {
                let Type::Result(_, error) = result_ty else {
                    return Err(verify_error(location, "invalid MIR call result type"));
                };
                (vec![error.as_ref().clone()], result_ty.clone())
            }
            CallTarget::BytesFromHex => (vec![Type::String], Type::Bytes),
            CallTarget::ConsolePrintln => {
                if arguments.len() != 1 {
                    return Err(verify_error(location, "invalid MIR call arity"));
                }
                self.operand_type(block, index, &arguments[0], None, initialized, location)?;
                (Vec::new(), Type::Unit)
            }
            CallTarget::StringToInt(receiver) => {
                let Some(local) = self.locals.get(&receiver) else {
                    return Err(verify_error(
                        location,
                        "MIR call uses an undefined receiver local",
                    ));
                };
                if local.ty != Type::String || !initialized.contains(&receiver) {
                    return Err(verify_error(location, "invalid MIR call receiver type"));
                }
                (
                    Vec::new(),
                    Type::Result(Box::new(Type::Int), Box::new(Type::Unit)),
                )
            }
        };
        if target != CallTarget::ConsolePrintln && arguments.len() != parameter_types.len() {
            return Err(verify_error(location, "invalid MIR call arity"));
        }
        for (argument, parameter_ty) in arguments.iter().zip(&parameter_types) {
            let actual = self
                .operand_type(
                    block,
                    index,
                    argument,
                    Some(parameter_ty),
                    initialized,
                    location,
                )
                .map_err(|error| remap_type_error(error, "invalid MIR call argument type"))?;
            if !types_compatible(&actual, parameter_ty) {
                return Err(verify_error(location, "invalid MIR call argument type"));
            }
        }
        if result_ty != &expected_result {
            return Err(verify_error(location, "invalid MIR call result type"));
        }
        Ok(())
    }

    fn verify_phi(
        &self,
        block: &BasicBlock,
        incoming: &[(BasicBlockId, Operand)],
        ty: &Type,
        initialized: &HashSet<LocalId>,
        location: SourceLocation,
    ) -> Result<(), VerifyError> {
        let expected = self.predecessors[block.id.0 as usize]
            .iter()
            .copied()
            .collect::<HashSet<_>>();
        let actual = incoming
            .iter()
            .map(|(predecessor, _)| *predecessor)
            .collect::<HashSet<_>>();
        if incoming.is_empty() || actual.len() != incoming.len() || actual != expected {
            return Err(verify_error(location, "invalid MIR phi predecessor"));
        }
        for (predecessor, operand) in incoming {
            let actual_ty = self.operand_type_at_predecessor(
                *predecessor,
                operand,
                Some(ty),
                initialized,
                location,
            )?;
            if !types_compatible(&actual_ty, ty) {
                return Err(verify_error(
                    location,
                    "MIR phi input type does not match result type",
                ));
            }
        }
        Ok(())
    }

    fn verify_match_exhaustive(
        &self,
        target_ty: &Type,
        arms: &[MatchTarget],
        location: SourceLocation,
    ) -> Result<(), VerifyError> {
        self.verify_match_target(target_ty, location)?;
        let expected = match target_ty {
            Type::Option(_) => HashSet::from([MatchPattern::Some, MatchPattern::None]),
            Type::Result(_, _) => HashSet::from([MatchPattern::Ok, MatchPattern::Err]),
            _ => {
                let Some(enumeration) = self
                    .declarations
                    .enums
                    .iter()
                    .find(|enumeration| &enumeration.ty == target_ty)
                else {
                    return Err(verify_error(location, "invalid MIR match target type"));
                };
                enumeration
                    .variants
                    .iter()
                    .map(|variant| MatchPattern::Variant(variant.id))
                    .collect()
            }
        };
        let actual = arms.iter().map(|arm| arm.pattern).collect::<HashSet<_>>();
        if actual != expected {
            return Err(verify_error(location, "non-exhaustive MIR match"));
        }
        Ok(())
    }

    fn verify_match_target(
        &self,
        target_ty: &Type,
        location: SourceLocation,
    ) -> Result<(), VerifyError> {
        if matches!(target_ty, Type::Option(_) | Type::Result(_, _))
            || self
                .declarations
                .enums
                .iter()
                .any(|enumeration| &enumeration.ty == target_ty)
        {
            return Ok(());
        }
        Err(verify_error(location, "invalid MIR match target type"))
    }

    fn verify_match_arm(
        &self,
        target_ty: &Type,
        arm: &MatchTarget,
        location: SourceLocation,
    ) -> Result<(), VerifyError> {
        let payload = match (target_ty, arm.pattern) {
            (Type::Option(payload), MatchPattern::Some) => Some(payload.as_ref()),
            (Type::Option(_), MatchPattern::None) => None,
            (Type::Result(ok, _), MatchPattern::Ok) => Some(ok.as_ref()),
            (Type::Result(_, error), MatchPattern::Err) => Some(error.as_ref()),
            (target_ty, MatchPattern::Variant(id)) => {
                let Some((enumeration, variant)) = self.declarations.variants.get(&id) else {
                    return Err(verify_error(location, "invalid MIR variant target ID"));
                };
                if &enumeration.ty != target_ty {
                    return Err(verify_error(
                        location,
                        "invalid MIR match pattern for target type",
                    ));
                }
                variant.payload.as_ref()
            }
            _ => {
                return Err(verify_error(
                    location,
                    "invalid MIR match pattern for target type",
                ));
            }
        };
        match (payload, arm.binding) {
            (None, None) => Ok(()),
            (None, Some(_)) | (Some(_), None) => {
                Err(verify_error(location, "invalid MIR match payload binding"))
            }
            (Some(payload), Some(binding)) => {
                let Some(local) = self.locals.get(&binding) else {
                    return Err(verify_error(location, "MIR match binds an undefined local"));
                };
                if &local.ty != payload {
                    return Err(verify_error(
                        location,
                        "MIR match binding type does not match payload",
                    ));
                }
                Ok(())
            }
        }
    }

    fn expect_operand_type(
        &self,
        block: BasicBlockId,
        index: usize,
        operand: &Operand,
        expected: &Type,
        initialized: &HashSet<LocalId>,
        location: SourceLocation,
    ) -> Result<(), VerifyError> {
        let actual =
            self.operand_type(block, index, operand, Some(expected), initialized, location)?;
        if &actual != expected {
            return Err(verify_error(location, "MIR operand type mismatch"));
        }
        Ok(())
    }

    fn operand_type(
        &self,
        block: BasicBlockId,
        index: usize,
        operand: &Operand,
        expected: Option<&Type>,
        initialized: &HashSet<LocalId>,
        location: SourceLocation,
    ) -> Result<Type, VerifyError> {
        match operand {
            Operand::Local(id) => {
                let Some(local) = self.locals.get(id) else {
                    return Err(verify_error(location, "MIR uses an undefined local"));
                };
                if !initialized.contains(id) {
                    return Err(verify_error(location, "MIR uses an uninitialized local"));
                }
                Ok(local.ty.clone())
            }
            Operand::Value(id) => {
                let Some(definition) = self.definitions.get(id) else {
                    return Err(verify_error(location, "MIR uses an undefined value"));
                };
                if !self.value_available(definition, block, index) {
                    return Err(verify_error(
                        location,
                        "MIR value is used before definition",
                    ));
                }
                Ok(definition.ty.clone())
            }
            Operand::Constant(constant) => self.constant_type(constant, expected, location),
        }
    }

    fn operand_type_at_predecessor(
        &self,
        predecessor: BasicBlockId,
        operand: &Operand,
        expected: Option<&Type>,
        initialized: &HashSet<LocalId>,
        location: SourceLocation,
    ) -> Result<Type, VerifyError> {
        match operand {
            Operand::Value(id) => {
                let Some(definition) = self.definitions.get(id) else {
                    return Err(verify_error(location, "MIR uses an undefined value"));
                };
                if !self.value_available(
                    definition,
                    predecessor,
                    self.function.blocks[predecessor.0 as usize]
                        .instructions
                        .len(),
                ) {
                    return Err(verify_error(
                        location,
                        "MIR value is used before definition",
                    ));
                }
                Ok(definition.ty.clone())
            }
            _ => self.operand_type(
                predecessor,
                usize::MAX,
                operand,
                expected,
                initialized,
                location,
            ),
        }
    }

    fn constant_type(
        &self,
        constant: &Constant,
        expected: Option<&Type>,
        location: SourceLocation,
    ) -> Result<Type, VerifyError> {
        match constant {
            Constant::Integer(_) => Ok(Type::Int),
            Constant::Float(_) => Ok(Type::Float),
            Constant::Boolean(_) => Ok(Type::Bool),
            Constant::String(_) => Ok(Type::String),
            Constant::Unit => Ok(Type::Unit),
            Constant::None => match expected {
                Some(Type::Option(payload)) => Ok(Type::Option(payload.clone())),
                _ => Err(verify_error(
                    location,
                    "MIR None constant lacks an Option type",
                )),
            },
            Constant::Variant(id) => self
                .declarations
                .variants
                .get(id)
                .map(|(enumeration, variant)| {
                    if variant.payload.is_some() {
                        Err(verify_error(
                            location,
                            "MIR payload variant cannot be a constant",
                        ))
                    } else {
                        Ok(enumeration.ty.clone())
                    }
                })
                .transpose()?
                .ok_or_else(|| verify_error(location, "invalid MIR variant target ID")),
        }
    }

    fn expect_value_type_at(
        &self,
        block: BasicBlockId,
        index: usize,
        value: ValueId,
        expected: &Type,
        location: SourceLocation,
    ) -> Result<(), VerifyError> {
        let Some(definition) = self.definitions.get(&value) else {
            return Err(verify_error(location, "MIR uses an undefined value"));
        };
        if !self.value_available(definition, block, index) {
            return Err(verify_error(
                location,
                "MIR value is used before definition",
            ));
        }
        if &definition.ty != expected {
            return Err(verify_error(location, "invalid MIR iterator type"));
        }
        Ok(())
    }

    fn value_available(
        &self,
        definition: &ValueDefinition,
        use_block: BasicBlockId,
        use_index: usize,
    ) -> bool {
        if definition.block == use_block {
            return match definition.position {
                DefinitionPosition::Phi => true,
                DefinitionPosition::Instruction(index) => index < use_index,
                DefinitionPosition::Terminator => false,
            };
        }
        self.dominators[use_block.0 as usize].contains(&definition.block)
    }
}

/// 保持 MIR 验证与类型检查阶段对不可返回值的兼容规则一致。
///
/// `Ok(value)` 和 `Err(value)` 分别以另一侧为 `never` 构造；它们可作为函数或分支
/// 期望的具体 `Result` 类型使用。若在此处改为严格相等，已通过类型检查的程序会在
/// MIR 边界被错误拒绝。
fn types_compatible(actual: &Type, expected: &Type) -> bool {
    match (actual, expected) {
        (Type::Never, _) | (_, Type::Never) => true,
        (Type::Result(actual_ok, actual_error), Type::Result(expected_ok, expected_error)) => {
            types_compatible(actual_ok, expected_ok)
                && types_compatible(actual_error, expected_error)
        }
        _ => actual == expected,
    }
}

fn collect_value_definitions(
    function: &Function,
) -> Result<HashMap<ValueId, ValueDefinition>, VerifyError> {
    let mut definitions = HashMap::new();
    for block in &function.blocks {
        for (index, instruction) in block.instructions.iter().enumerate() {
            for (destination, ty, position) in instruction_definitions(instruction, index) {
                if definitions
                    .insert(
                        destination,
                        ValueDefinition {
                            block: block.id,
                            position,
                            ty,
                        },
                    )
                    .is_some()
                {
                    return Err(verify_error(
                        instruction_location(instruction),
                        "duplicate MIR value ID",
                    ));
                }
            }
        }
        if let Terminator::PropagateErr {
            destination,
            ty,
            location,
            ..
        } = &block.terminator
        {
            if definitions
                .insert(
                    *destination,
                    ValueDefinition {
                        block: block.id,
                        position: DefinitionPosition::Terminator,
                        ty: ty.clone(),
                    },
                )
                .is_some()
            {
                return Err(verify_error(*location, "duplicate MIR value ID"));
            }
        }
    }
    Ok(definitions)
}

fn instruction_definitions(
    instruction: &Instruction,
    index: usize,
) -> Vec<(ValueId, Type, DefinitionPosition)> {
    let normal = |destination, ty| {
        vec![(
            destination,
            ty,
            if matches!(instruction, Instruction::Phi { .. }) {
                DefinitionPosition::Phi
            } else {
                DefinitionPosition::Instruction(index)
            },
        )]
    };
    match instruction {
        Instruction::Assign {
            destination, ty, ..
        }
        | Instruction::Binary {
            destination, ty, ..
        }
        | Instruction::BuildString {
            destination, ty, ..
        }
        | Instruction::BuildList {
            destination, ty, ..
        }
        | Instruction::BuildMap {
            destination, ty, ..
        }
        | Instruction::BuildTuple {
            destination, ty, ..
        }
        | Instruction::TupleElement {
            destination, ty, ..
        }
        | Instruction::BuildStruct {
            destination, ty, ..
        }
        | Instruction::LoadField {
            destination, ty, ..
        }
        | Instruction::Call {
            destination, ty, ..
        }
        | Instruction::Phi {
            destination, ty, ..
        }
        | Instruction::IterInit {
            destination, ty, ..
        } => normal(*destination, ty.clone()),
        Instruction::IterNext {
            item_destination,
            has_value_destination,
            item_ty,
            ..
        } => vec![
            (
                *item_destination,
                item_ty.clone(),
                DefinitionPosition::Instruction(index),
            ),
            (
                *has_value_destination,
                Type::Bool,
                DefinitionPosition::Instruction(index),
            ),
        ],
        Instruction::StoreLocal { .. } => Vec::new(),
    }
}

fn terminator_targets(terminator: &Terminator) -> Vec<BasicBlockId> {
    match terminator {
        Terminator::Goto { target, .. } => vec![*target],
        Terminator::Branch {
            then_block,
            else_block,
            ..
        } => vec![*then_block, *else_block],
        Terminator::Match {
            arms, otherwise, ..
        } => arms
            .iter()
            .map(|arm| arm.block)
            .chain(std::iter::once(*otherwise))
            .collect(),
        Terminator::PropagateErr { success, .. } => vec![*success],
        Terminator::Return { .. } | Terminator::Unreachable { .. } => Vec::new(),
    }
}

fn compute_dominators(
    block_count: usize,
    predecessors: &[Vec<BasicBlockId>],
    reachable: &[bool],
) -> Vec<HashSet<BasicBlockId>> {
    let all = (0..block_count)
        .map(|index| BasicBlockId(index as u32))
        .collect::<HashSet<_>>();
    let mut dominators = vec![all; block_count];
    dominators[0] = HashSet::from([BasicBlockId(0)]);
    for index in 1..block_count {
        if !reachable[index] {
            dominators[index] = HashSet::from([BasicBlockId(index as u32)]);
        }
    }
    let mut changed = true;
    while changed {
        changed = false;
        for block_index in 1..block_count {
            if !reachable[block_index] {
                continue;
            }
            let mut next = if predecessors[block_index].is_empty() {
                HashSet::new()
            } else {
                let mut incoming = predecessors[block_index]
                    .iter()
                    .map(|predecessor| dominators[predecessor.0 as usize].clone());
                let mut intersection = incoming.next().unwrap_or_default();
                for set in incoming {
                    intersection.retain(|block| set.contains(block));
                }
                intersection
            };
            next.insert(BasicBlockId(block_index as u32));
            if dominators[block_index] != next {
                dominators[block_index] = next;
                changed = true;
            }
        }
    }
    dominators
}

fn compute_reachable(function: &Function) -> Vec<bool> {
    let mut reachable = vec![false; function.blocks.len()];
    let mut pending = vec![BasicBlockId(0)];
    while let Some(block) = pending.pop() {
        if reachable[block.0 as usize] {
            continue;
        }
        reachable[block.0 as usize] = true;
        pending.extend(terminator_targets(
            &function.blocks[block.0 as usize].terminator,
        ));
    }
    reachable
}

fn verify_error(location: SourceLocation, message: &str) -> VerifyError {
    VerifyError {
        location,
        message: message.to_owned(),
    }
}

fn remap_type_error(mut error: VerifyError, message: &str) -> VerifyError {
    if error.message == "MIR operand type mismatch" {
        error.message = message.to_owned();
    }
    error
}

#[derive(Clone, Debug)]
struct PendingBlock {
    id: BasicBlockId,
    instructions: Vec<Instruction>,
    terminator: Option<Terminator>,
}

struct FunctionLowerer<'a> {
    function: &'a TypedFunction,
    structs: &'a [TypedStruct],
    blocks: Vec<PendingBlock>,
    current: Option<BasicBlockId>,
    next_value: u32,
    locals: Vec<Local>,
}

impl<'a> FunctionLowerer<'a> {
    fn new(function: &'a TypedFunction, structs: &'a [TypedStruct]) -> Self {
        Self {
            function,
            structs,
            blocks: vec![PendingBlock {
                id: BasicBlockId(0),
                instructions: Vec::new(),
                terminator: None,
            }],
            current: Some(BasicBlockId(0)),
            next_value: 0,
            locals: function.parameters.iter().map(lower_local).collect(),
        }
    }

    fn lower(mut self) -> Result<Function, LowerError> {
        let tail = self.lower_statements(&self.function.statements)?;
        if self.current.is_some() {
            self.terminate(Terminator::Return {
                value: tail.filter(|_| self.function.return_type != Type::Unit),
                location: SourceLocation::new(self.function.source, self.function.span),
            })?;
        }
        let blocks = self
            .blocks
            .into_iter()
            .map(|block| {
                Ok(BasicBlock {
                    id: block.id,
                    instructions: block.instructions,
                    terminator: block.terminator.ok_or_else(|| LowerError {
                        location: SourceLocation::new(self.function.source, self.function.span),
                        message: "MIR basic block has no terminator".to_owned(),
                    })?,
                })
            })
            .collect::<Result<Vec<_>, LowerError>>()?;
        Ok(Function {
            id: FunctionId(self.function.id),
            name: self.function.name.clone(),
            location: SourceLocation::new(self.function.source, self.function.span),
            return_type: self.function.return_type.clone(),
            parameters: self.function.parameters.iter().map(lower_local).collect(),
            locals: self.locals,
            blocks,
        })
    }

    fn lower_statements(
        &mut self,
        statements: &[TypedStatement],
    ) -> Result<Option<Operand>, LowerError> {
        let mut tail = Some(Operand::Constant(Constant::Unit));
        for (index, statement) in statements.iter().enumerate() {
            if self.current.is_none() {
                return Ok(None);
            }
            let is_tail = index + 1 == statements.len();
            match statement {
                TypedStatement::Let { local, value } => {
                    self.locals.push(lower_local(local));
                    let Some(operand) = self.lower_expression(value)? else {
                        return Ok(None);
                    };
                    self.emit(Instruction::StoreLocal {
                        local: local.id,
                        value: operand,
                        ty: local.ty.clone(),
                        location: local.location,
                    })?;
                    tail = Some(Operand::Constant(Constant::Unit));
                }
                TypedStatement::Assign {
                    local,
                    value,
                    location,
                    ..
                } => {
                    let ty = value.ty.clone();
                    let Some(operand) = self.lower_expression(value)? else {
                        return Ok(None);
                    };
                    self.emit(Instruction::StoreLocal {
                        local: *local,
                        value: operand,
                        ty,
                        location: *location,
                    })?;
                    tail = Some(Operand::Constant(Constant::Unit));
                }
                TypedStatement::Destructure { locals, value } => {
                    self.locals.extend(locals.iter().map(lower_local));
                    let Some(tuple) = self.lower_expression(value)? else {
                        return Ok(None);
                    };
                    for (index, local) in locals.iter().enumerate() {
                        let destination = self.new_value();
                        self.emit(Instruction::TupleElement {
                            destination,
                            tuple: tuple.clone(),
                            index: index as u32,
                            ty: local.ty.clone(),
                            location: value.location,
                        })?;
                        self.emit(Instruction::StoreLocal {
                            local: local.id,
                            value: Operand::Value(destination),
                            ty: local.ty.clone(),
                            location: local.location,
                        })?;
                    }
                    tail = Some(Operand::Constant(Constant::Unit));
                }
                TypedStatement::Expression(expression) => {
                    tail = self.lower_expression(expression)?;
                    if tail.is_none() {
                        return Ok(None);
                    }
                    if !is_tail {
                        tail = Some(Operand::Constant(Constant::Unit));
                    }
                }
            }
        }
        Ok(tail)
    }

    fn lower_expression(
        &mut self,
        expression: &TypedExpression,
    ) -> Result<Option<Operand>, LowerError> {
        let operand = match &expression.kind {
            TypedExpressionKind::Integer(value) => Operand::Constant(Constant::Integer(*value)),
            TypedExpressionKind::Float(value) => Operand::Constant(Constant::Float(value.clone())),
            TypedExpressionKind::Boolean(value) => Operand::Constant(Constant::Boolean(*value)),
            TypedExpressionKind::Local(local) => {
                self.snapshot_local(*local, expression.ty.clone(), expression.location)?
            }
            TypedExpressionKind::None => Operand::Constant(Constant::None),
            TypedExpressionKind::Variant(id) => Operand::Constant(Constant::Variant(*id)),
            TypedExpressionKind::String(parts) => self.lower_string(parts, expression)?,
            TypedExpressionKind::List(values) => return self.lower_list(values, expression),
            TypedExpressionKind::Map(values) => return self.lower_map(values, expression),
            TypedExpressionKind::Tuple(values) => return self.lower_tuple(values, expression),
            TypedExpressionKind::Add(left, right) => {
                return self.lower_binary(BinaryOperator::Add, left, right, expression);
            }
            TypedExpressionKind::Multiply(left, right) => {
                return self.lower_binary(BinaryOperator::Multiply, left, right, expression);
            }
            TypedExpressionKind::Equal(left, right) => {
                return self.lower_binary(BinaryOperator::Equal, left, right, expression);
            }
            TypedExpressionKind::Call { target, arguments } => {
                return self.lower_call(*target, arguments, expression);
            }
            TypedExpressionKind::Struct { structure, fields } => {
                return self.lower_struct(*structure, fields, expression);
            }
            TypedExpressionKind::Field { target, field } => {
                let Some(target) = self.lower_expression(target)? else {
                    return Ok(None);
                };
                let destination = self.new_value();
                self.emit(Instruction::LoadField {
                    destination,
                    target,
                    field: *field,
                    ty: expression.ty.clone(),
                    location: expression.location,
                })?;
                Operand::Value(destination)
            }
            TypedExpressionKind::If {
                condition,
                then_statements,
                else_statements,
            } => {
                return self.lower_if(condition, then_statements, else_statements, expression);
            }
            TypedExpressionKind::Match { target, arms } => {
                return self.lower_match(target, arms, expression);
            }
            TypedExpressionKind::For {
                local,
                iterable,
                statements,
            } => return self.lower_for(local, iterable, statements, expression),
            TypedExpressionKind::Return(value) => {
                let is_unit = value.ty == Type::Unit;
                let Some(value) = self.lower_expression(value)? else {
                    return Ok(None);
                };
                self.terminate(Terminator::Return {
                    value: (!is_unit).then_some(value),
                    location: expression.location,
                })?;
                return Ok(None);
            }
            TypedExpressionKind::Try(value) => {
                let Some(result) = self.lower_expression(value)? else {
                    return Ok(None);
                };
                let success = self.new_block();
                let destination = self.new_value();
                self.terminate(Terminator::PropagateErr {
                    result,
                    destination,
                    success,
                    ty: expression.ty.clone(),
                    location: expression.location,
                })?;
                self.switch_to(success);
                Operand::Value(destination)
            }
        };
        Ok(Some(operand))
    }

    fn lower_string(
        &mut self,
        parts: &[TypedStringPart],
        expression: &TypedExpression,
    ) -> Result<Operand, LowerError> {
        if let [TypedStringPart::Text(text)] = parts {
            return Ok(Operand::Constant(Constant::String(text.clone())));
        }
        let destination = self.new_value();
        let mut lowered_parts = Vec::with_capacity(parts.len());
        for part in parts {
            match part {
                TypedStringPart::Text(text) => {
                    lowered_parts.push(StringPart::Text(text.clone()));
                }
                TypedStringPart::Local(local) => {
                    let ty = self
                        .locals
                        .iter()
                        .find(|binding| binding.id == *local)
                        .map(|binding| binding.ty.clone())
                        .ok_or_else(|| LowerError {
                            location: expression.location,
                            message: "MIR interpolation local is missing".to_owned(),
                        })?;
                    lowered_parts.push(StringPart::Value(self.snapshot_local(
                        *local,
                        ty,
                        expression.location,
                    )?));
                }
            }
        }
        self.emit(Instruction::BuildString {
            destination,
            parts: lowered_parts,
            ty: expression.ty.clone(),
            location: expression.location,
        })?;
        Ok(Operand::Value(destination))
    }

    fn lower_list(
        &mut self,
        values: &[TypedExpression],
        expression: &TypedExpression,
    ) -> Result<Option<Operand>, LowerError> {
        let Some(elements) = self.lower_values(values)? else {
            return Ok(None);
        };
        let destination = self.new_value();
        self.emit(Instruction::BuildList {
            destination,
            elements,
            ty: expression.ty.clone(),
            location: expression.location,
        })?;
        Ok(Some(Operand::Value(destination)))
    }

    fn lower_map(
        &mut self,
        values: &[(String, TypedExpression)],
        expression: &TypedExpression,
    ) -> Result<Option<Operand>, LowerError> {
        let mut entries = Vec::with_capacity(values.len());
        for (key, value) in values {
            let Some(value) = self.lower_expression(value)? else {
                return Ok(None);
            };
            entries.push((key.clone(), value));
        }
        let destination = self.new_value();
        self.emit(Instruction::BuildMap {
            destination,
            entries,
            ty: expression.ty.clone(),
            location: expression.location,
        })?;
        Ok(Some(Operand::Value(destination)))
    }

    fn lower_tuple(
        &mut self,
        values: &[TypedExpression],
        expression: &TypedExpression,
    ) -> Result<Option<Operand>, LowerError> {
        let Some(elements) = self.lower_values(values)? else {
            return Ok(None);
        };
        let destination = self.new_value();
        self.emit(Instruction::BuildTuple {
            destination,
            elements,
            ty: expression.ty.clone(),
            location: expression.location,
        })?;
        Ok(Some(Operand::Value(destination)))
    }

    fn lower_binary(
        &mut self,
        operator: BinaryOperator,
        left: &TypedExpression,
        right: &TypedExpression,
        expression: &TypedExpression,
    ) -> Result<Option<Operand>, LowerError> {
        let Some(left) = self.lower_expression(left)? else {
            return Ok(None);
        };
        let Some(right) = self.lower_expression(right)? else {
            return Ok(None);
        };
        let destination = self.new_value();
        self.emit(Instruction::Binary {
            destination,
            operator,
            left,
            right,
            ty: expression.ty.clone(),
            location: expression.location,
        })?;
        Ok(Some(Operand::Value(destination)))
    }

    fn lower_call(
        &mut self,
        target: TypedCallTarget,
        arguments: &[TypedExpression],
        expression: &TypedExpression,
    ) -> Result<Option<Operand>, LowerError> {
        let Some(arguments) = self.lower_values(arguments)? else {
            return Ok(None);
        };
        let destination = self.new_value();
        self.emit(Instruction::Call {
            destination,
            target: lower_call_target(target),
            arguments,
            ty: expression.ty.clone(),
            location: expression.location,
        })?;
        Ok(Some(Operand::Value(destination)))
    }

    fn lower_struct(
        &mut self,
        structure: DefId,
        fields: &[(FieldId, TypedExpression)],
        expression: &TypedExpression,
    ) -> Result<Option<Operand>, LowerError> {
        let mut lowered = Vec::with_capacity(fields.len());
        for (field, value) in fields {
            let Some(value) = self.lower_expression(value)? else {
                return Ok(None);
            };
            lowered.push((*field, value));
        }
        let declared = self
            .structs
            .iter()
            .find(|item| item.id == structure)
            .ok_or_else(|| LowerError {
                location: expression.location,
                message: "MIR struct declaration is missing".to_owned(),
            })?;
        for field in &declared.fields {
            if lowered.iter().any(|(id, _)| *id == field.id) {
                continue;
            }
            if let Some(default) = &field.default {
                let Some(value) = self.lower_expression(default)? else {
                    return Ok(None);
                };
                lowered.push((field.id, value));
            }
        }
        let destination = self.new_value();
        self.emit(Instruction::BuildStruct {
            destination,
            structure,
            fields: lowered,
            ty: expression.ty.clone(),
            location: expression.location,
        })?;
        Ok(Some(Operand::Value(destination)))
    }

    fn lower_values(
        &mut self,
        values: &[TypedExpression],
    ) -> Result<Option<Vec<Operand>>, LowerError> {
        let mut lowered = Vec::with_capacity(values.len());
        for value in values {
            let Some(value) = self.lower_expression(value)? else {
                return Ok(None);
            };
            lowered.push(value);
        }
        Ok(Some(lowered))
    }

    fn lower_if(
        &mut self,
        condition: &TypedExpression,
        then_statements: &[TypedStatement],
        else_statements: &[TypedStatement],
        expression: &TypedExpression,
    ) -> Result<Option<Operand>, LowerError> {
        let Some(condition) = self.lower_expression(condition)? else {
            return Ok(None);
        };
        let then_block = self.new_block();
        let else_block = self.new_block();
        let join = self.new_block();
        self.terminate(Terminator::Branch {
            condition,
            then_block,
            else_block,
            location: expression.location,
        })?;

        let mut incoming = Vec::new();
        self.switch_to(then_block);
        if let Some(value) = self.lower_statements(then_statements)? {
            let predecessor = self.current_id(expression.location)?;
            self.terminate(Terminator::Goto {
                target: join,
                location: expression.location,
            })?;
            incoming.push((predecessor, value));
        }
        self.switch_to(else_block);
        if let Some(value) = self.lower_statements(else_statements)? {
            let predecessor = self.current_id(expression.location)?;
            self.terminate(Terminator::Goto {
                target: join,
                location: expression.location,
            })?;
            incoming.push((predecessor, value));
        }
        self.join_value(join, incoming, expression)
    }

    fn lower_match(
        &mut self,
        target: &TypedExpression,
        arms: &[yan_typeck::TypedMatchArm],
        expression: &TypedExpression,
    ) -> Result<Option<Operand>, LowerError> {
        let Some(target) = self.lower_expression(target)? else {
            return Ok(None);
        };
        let arm_blocks = (0..arms.len())
            .map(|_| self.new_block())
            .collect::<Vec<_>>();
        let otherwise = self.new_block();
        let join = self.new_block();
        let targets = arms
            .iter()
            .zip(&arm_blocks)
            .map(|(arm, block)| MatchTarget {
                pattern: lower_pattern(arm.pattern),
                binding: arm.binding.as_ref().map(|binding| binding.id),
                block: *block,
            })
            .collect();
        self.terminate(Terminator::Match {
            target,
            arms: targets,
            otherwise,
            location: expression.location,
        })?;

        let mut incoming = Vec::new();
        for (arm, block) in arms.iter().zip(arm_blocks) {
            if let Some(binding) = &arm.binding {
                self.locals.push(lower_local(binding));
            }
            self.switch_to(block);
            if let Some(value) = self.lower_expression(&arm.value)? {
                let predecessor = self.current_id(expression.location)?;
                self.terminate(Terminator::Goto {
                    target: join,
                    location: expression.location,
                })?;
                incoming.push((predecessor, value));
            }
        }
        self.switch_to(otherwise);
        self.terminate(Terminator::Unreachable {
            location: expression.location,
        })?;
        self.join_value(join, incoming, expression)
    }

    fn lower_for(
        &mut self,
        local: &yan_typeck::TypedLocal,
        iterable: &TypedExpression,
        statements: &[TypedStatement],
        expression: &TypedExpression,
    ) -> Result<Option<Operand>, LowerError> {
        self.locals.push(lower_local(local));
        let Some(iterable) = self.lower_expression(iterable)? else {
            return Ok(None);
        };
        let iterator = self.new_value();
        self.emit(Instruction::IterInit {
            destination: iterator,
            iterable,
            ty: Type::List(Box::new(local.ty.clone())),
            location: expression.location,
        })?;
        let condition = self.new_block();
        let body = self.new_block();
        let exit = self.new_block();
        self.terminate(Terminator::Goto {
            target: condition,
            location: expression.location,
        })?;

        self.switch_to(condition);
        let item = self.new_value();
        let has_value = self.new_value();
        self.emit(Instruction::IterNext {
            iterator,
            item_destination: item,
            has_value_destination: has_value,
            item_ty: local.ty.clone(),
            location: expression.location,
        })?;
        self.terminate(Terminator::Branch {
            condition: Operand::Value(has_value),
            then_block: body,
            else_block: exit,
            location: expression.location,
        })?;

        self.switch_to(body);
        self.emit(Instruction::StoreLocal {
            local: local.id,
            value: Operand::Value(item),
            ty: local.ty.clone(),
            location: local.location,
        })?;
        let _ = self.lower_statements(statements)?;
        if self.current.is_some() {
            self.terminate(Terminator::Goto {
                target: condition,
                location: expression.location,
            })?;
        }
        self.switch_to(exit);
        Ok(Some(Operand::Constant(Constant::Unit)))
    }

    fn join_value(
        &mut self,
        join: BasicBlockId,
        incoming: Vec<(BasicBlockId, Operand)>,
        expression: &TypedExpression,
    ) -> Result<Option<Operand>, LowerError> {
        self.switch_to(join);
        if incoming.is_empty() {
            self.terminate(Terminator::Unreachable {
                location: expression.location,
            })?;
            return Ok(None);
        }
        let destination = self.new_value();
        self.emit(Instruction::Phi {
            destination,
            incoming,
            ty: expression.ty.clone(),
            location: expression.location,
        })?;
        Ok(Some(Operand::Value(destination)))
    }

    fn new_value(&mut self) -> ValueId {
        let value = ValueId(self.next_value);
        self.next_value += 1;
        value
    }

    fn snapshot_local(
        &mut self,
        local: LocalId,
        ty: Type,
        location: SourceLocation,
    ) -> Result<Operand, LowerError> {
        let destination = self.new_value();
        self.emit(Instruction::Assign {
            destination,
            operand: Operand::Local(local),
            ty,
            location,
        })?;
        Ok(Operand::Value(destination))
    }

    fn new_block(&mut self) -> BasicBlockId {
        let id = BasicBlockId(self.blocks.len() as u32);
        self.blocks.push(PendingBlock {
            id,
            instructions: Vec::new(),
            terminator: None,
        });
        id
    }

    fn switch_to(&mut self, block: BasicBlockId) {
        self.current = Some(block);
    }

    fn current_id(&self, location: SourceLocation) -> Result<BasicBlockId, LowerError> {
        self.current.ok_or_else(|| LowerError {
            location,
            message: "MIR instruction follows a terminator".to_owned(),
        })
    }

    fn emit(&mut self, instruction: Instruction) -> Result<(), LowerError> {
        let location = instruction_location(&instruction);
        let current = self.current_id(location)?;
        let block = &mut self.blocks[current.0 as usize];
        if block.terminator.is_some() {
            return Err(LowerError {
                location,
                message: "MIR instruction follows a terminator".to_owned(),
            });
        }
        block.instructions.push(instruction);
        Ok(())
    }

    fn terminate(&mut self, terminator: Terminator) -> Result<(), LowerError> {
        let location = terminator_location(&terminator);
        let Some(current) = self.current else {
            return Err(LowerError {
                location,
                message: "MIR basic block has multiple terminators".to_owned(),
            });
        };
        let block = &mut self.blocks[current.0 as usize];
        if block.terminator.replace(terminator).is_some() {
            return Err(LowerError {
                location,
                message: "MIR basic block has multiple terminators".to_owned(),
            });
        }
        self.current = None;
        Ok(())
    }
}

fn lower_function(
    function: &TypedFunction,
    structs: &[TypedStruct],
) -> Result<Function, LowerError> {
    FunctionLowerer::new(function, structs).lower()
}

fn lower_local(local: &yan_typeck::TypedLocal) -> Local {
    Local {
        id: local.id,
        ty: local.ty.clone(),
        mutable: local.mutable,
        location: local.location,
    }
}

fn lower_call_target(target: TypedCallTarget) -> CallTarget {
    match target {
        TypedCallTarget::Function(id) => CallTarget::Function(id),
        TypedCallTarget::Newtype(id) => CallTarget::Newtype(id),
        TypedCallTarget::Variant(id) => CallTarget::Variant(id),
        TypedCallTarget::Some => CallTarget::Some,
        TypedCallTarget::Ok => CallTarget::Ok,
        TypedCallTarget::Err => CallTarget::Err,
        TypedCallTarget::BytesFromHex => CallTarget::BytesFromHex,
        TypedCallTarget::ConsolePrintln => CallTarget::ConsolePrintln,
        TypedCallTarget::StringToInt(local) => CallTarget::StringToInt(local),
    }
}

fn lower_pattern(pattern: TypedPattern) -> MatchPattern {
    match pattern {
        TypedPattern::Variant(id) => MatchPattern::Variant(id),
        TypedPattern::Some => MatchPattern::Some,
        TypedPattern::None => MatchPattern::None,
        TypedPattern::Ok => MatchPattern::Ok,
        TypedPattern::Err => MatchPattern::Err,
    }
}

fn instruction_location(instruction: &Instruction) -> SourceLocation {
    match instruction {
        Instruction::Assign { location, .. }
        | Instruction::StoreLocal { location, .. }
        | Instruction::Binary { location, .. }
        | Instruction::BuildString { location, .. }
        | Instruction::BuildList { location, .. }
        | Instruction::BuildMap { location, .. }
        | Instruction::BuildTuple { location, .. }
        | Instruction::TupleElement { location, .. }
        | Instruction::BuildStruct { location, .. }
        | Instruction::LoadField { location, .. }
        | Instruction::Call { location, .. }
        | Instruction::Phi { location, .. }
        | Instruction::IterInit { location, .. }
        | Instruction::IterNext { location, .. } => *location,
    }
}

fn terminator_location(terminator: &Terminator) -> SourceLocation {
    match terminator {
        Terminator::Goto { location, .. }
        | Terminator::Branch { location, .. }
        | Terminator::Match { location, .. }
        | Terminator::Return { location, .. }
        | Terminator::PropagateErr { location, .. }
        | Terminator::Unreachable { location } => *location,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        lower, terminator_location, verify, BasicBlock, BasicBlockId, BinaryOperator, CallTarget,
        Constant, FunctionId, FunctionLowerer, Instruction, Local, MatchPattern, Operand,
        StringPart, Terminator, ValueId,
    };
    use yan_hir::{lower as lower_hir, DefId, FieldId, LocalId, Type, VariantId};
    use yan_syntax::{lex, parse};
    use yan_typeck::check;

    fn lower_fixture(source: &str) -> super::Program {
        lower(typed_fixture(source)).expect("fixture must lower to MIR")
    }

    fn typed_fixture(source: &str) -> yan_typeck::TypedProgram {
        let tokens = lex(source).expect("fixture must lex");
        let syntax = parse(source, &tokens).expect("fixture must parse");
        check(&lower_hir(syntax).expect("fixture must lower")).expect("fixture must type check")
    }

    fn second_function_call(program: &mut super::Program) -> &mut Instruction {
        program.functions[1].blocks[0]
            .instructions
            .iter_mut()
            .find(|instruction| matches!(instruction, Instruction::Call { .. }))
            .expect("fixture must contain call")
    }

    fn propagation_block(program: &mut super::Program) -> &mut BasicBlock {
        program.functions[0]
            .blocks
            .iter_mut()
            .find(|block| matches!(block.terminator, Terminator::PropagateErr { .. }))
            .expect("fixture must propagate")
    }

    #[test]
    fn lowers_sequential_bindings_into_locals_and_return() {
        let program = lower_fixture("import yan.platform.console fn main() -> unit { let mut count = 1 count = count + 1 console.println(count) }");
        assert_eq!(program.functions[0].id, FunctionId(DefId(0)));
        assert_eq!(program.functions[0].blocks[0].id, BasicBlockId(0));
        assert!(matches!(
            program.functions[0].blocks[0].instructions[0],
            Instruction::StoreLocal { .. }
        ));
        assert!(matches!(
            program.functions[0].blocks[0].terminator,
            Terminator::Return { .. }
        ));
    }

    #[test]
    fn lowers_m2_to_m13_semantic_category_fixtures_to_core_mir_shapes() {
        let fixtures: [(&str, &str, fn(&super::Program) -> bool); 4] = [
            (
                "values and bindings",
                "fn value() -> int { let value = 1 value } fn main() -> unit { }",
                |program| {
                    let entry = &program.functions[0].blocks[0];
                    matches!(
                        entry.terminator,
                        Terminator::Return {
                            value: Some(Operand::Value(_)),
                            ..
                        }
                    ) && entry.instructions.iter().any(|instruction| matches!(
                        instruction,
                        Instruction::Assign {
                            operand: Operand::Local(_),
                            ty: Type::Int,
                            ..
                        }
                    ))
                },
            ),
            (
                "struct construction",
                "struct User { name: string } fn build() -> User { User { name: \"Yan\" } } fn main() -> unit { }",
                |program| program.structs.len() == 1 && program.functions[0].blocks[0].instructions.iter().any(|instruction| matches!(instruction, Instruction::BuildStruct { .. })),
            ),
            (
                "enum option result and match",
                "enum State { Ready } fn value() -> Result<int, unit> { let value = Some(1) match value { Some(item) => Ok(item) None => Ok(0) } } fn main() -> unit { }",
                |program| program.enums.len() == 1 && program.functions[0].blocks.iter().any(|block| matches!(block.terminator, Terminator::Match { .. })) && program.functions[0].blocks.iter().flat_map(|block| &block.instructions).any(|instruction| matches!(instruction, Instruction::Call { target: CallTarget::Ok, .. })),
            ),
            (
                "mutation tuple if and for",
                "fn main() -> unit { let mut total = 0 let (left, right) = (1, 2) for item in [left, right] { total = total + item } if total == 3 { } else { } }",
                |program| program.functions[0].blocks.iter().flat_map(|block| &block.instructions).any(|instruction| matches!(instruction, Instruction::TupleElement { .. })) && program.functions[0].blocks.iter().flat_map(|block| &block.instructions).any(|instruction| matches!(instruction, Instruction::IterNext { .. })) && program.functions[0].blocks.iter().any(|block| matches!(block.terminator, Terminator::Branch { .. })),
            ),
        ];
        for (category, source, asserts_shape) in fixtures {
            let program = lower_fixture(source);
            verify(program.clone()).expect("category MIR must verify");
            assert!(
                asserts_shape(&program),
                "{category} fixture must retain its core MIR shape"
            );
        }
    }

    #[test]
    fn lowers_addition_into_typed_operands_and_destination() {
        let program =
            lower_fixture("fn sum() -> int { let value = 1 + 2 value } fn main() -> unit { }");
        assert!(matches!(
            program.functions[0].blocks[0].instructions[0],
            Instruction::Binary {
                operator: BinaryOperator::Add,
                ty: Type::Int,
                ..
            }
        ));
    }

    #[test]
    fn verifies_a_lowered_program_before_execution() {
        verify(lower_fixture("fn main() -> unit { }")).expect("lowered MIR must verify");
    }

    #[test]
    fn rejects_store_to_immutable_local() {
        let mut program = lower_fixture("fn main() -> unit { let mut value = 1 value = 2 }");
        program.functions[0].locals[0].mutable = false;
        let error = verify(program).expect_err("immutable store must be rejected");
        assert_eq!(error.message, "MIR writes an immutable local");
    }

    #[test]
    fn rejects_branch_to_missing_block() {
        let mut program = lower_fixture("fn main() -> unit { }");
        let location = program.functions[0].location;
        program.functions[0].blocks[0].terminator = Terminator::Goto {
            target: BasicBlockId(9),
            location,
        };

        let error = verify(program).expect_err("missing block must be rejected");
        assert_eq!(error.message, "invalid MIR jump target");
    }

    #[test]
    fn rejects_undefined_and_use_before_definition_values() {
        let mut undefined = lower_fixture("fn value() -> int { 1 } fn main() -> unit { }");
        let location = undefined.functions[0].location;
        undefined.functions[0].blocks[0].terminator = Terminator::Return {
            value: Some(Operand::Value(ValueId(99))),
            location,
        };
        let error = verify(undefined).expect_err("undefined value must be rejected");
        assert_eq!(error.message, "MIR uses an undefined value");

        let mut before =
            lower_fixture("fn value() -> int { let result = 1 + 2 result } fn main() -> unit { }");
        let Instruction::Binary {
            destination, left, ..
        } = &mut before.functions[0].blocks[0].instructions[0]
        else {
            panic!("fixture must start with binary instruction");
        };
        *left = Operand::Value(*destination);
        let error = verify(before).expect_err("self-use must be rejected");
        assert_eq!(error.message, "MIR value is used before definition");
    }

    #[test]
    fn rejects_duplicate_value_definition() {
        let mut program = lower_fixture(
            "fn value() -> int { let left = 1 + 2 let right = 3 + 4 left + right } fn main() -> unit { }",
        );
        let first = match &program.functions[0].blocks[0].instructions[0] {
            Instruction::Binary { destination, .. } => *destination,
            _ => panic!("fixture must start with binary instruction"),
        };
        let Instruction::Binary { destination, .. } =
            &mut program.functions[0].blocks[0].instructions[2]
        else {
            panic!("fixture must contain a second binary instruction");
        };
        *destination = first;

        let error = verify(program).expect_err("duplicate value must be rejected");
        assert_eq!(error.message, "duplicate MIR value ID");
    }

    #[test]
    fn rejects_undefined_local_reads_and_parameter_inconsistency() {
        let mut undefined =
            lower_fixture("fn value(input: int) -> int { input } fn main() -> unit { }");
        let Instruction::Assign { operand, .. } =
            &mut undefined.functions[0].blocks[0].instructions[0]
        else {
            panic!("fixture must snapshot its parameter");
        };
        *operand = Operand::Local(LocalId(99));
        let error = verify(undefined).expect_err("undefined local must be rejected");
        assert_eq!(error.message, "MIR uses an undefined local");

        let mut missing =
            lower_fixture("fn value(input: int) -> int { input } fn main() -> unit { }");
        missing.functions[0].locals.clear();
        let error = verify(missing).expect_err("parameter missing from locals must be rejected");
        assert_eq!(error.message, "MIR parameter is missing from locals");

        let mut mismatch =
            lower_fixture("fn value(input: int) -> int { input } fn main() -> unit { }");
        mismatch.functions[0].parameters[0].ty = Type::Bool;
        let error = verify(mismatch).expect_err("parameter metadata mismatch must be rejected");
        assert_eq!(error.message, "MIR parameter does not match its local");
    }

    #[test]
    fn rejects_operand_and_instruction_type_mismatches() {
        let mut assign =
            lower_fixture("fn value(input: int) -> int { input } fn main() -> unit { }");
        let Instruction::Assign { ty, .. } = &mut assign.functions[0].blocks[0].instructions[0]
        else {
            panic!("fixture must snapshot its parameter");
        };
        *ty = Type::Bool;
        let error = verify(assign).expect_err("assign mismatch must be rejected");
        assert_eq!(
            error.message,
            "MIR operand type does not match instruction type"
        );

        let mut binary = lower_fixture("fn value() -> int { 1 + 2 } fn main() -> unit { }");
        let Instruction::Binary { left, .. } = &mut binary.functions[0].blocks[0].instructions[0]
        else {
            panic!("fixture must contain binary instruction");
        };
        *left = Operand::Constant(Constant::Boolean(true));
        let error = verify(binary).expect_err("binary operand mismatch must be rejected");
        assert_eq!(error.message, "invalid MIR binary operand type");
    }

    #[test]
    fn rejects_phi_predecessor_and_type_mismatches() {
        let mut predecessor = lower_fixture(
            "fn choose(value: bool) -> int { if value { 1 } else { 2 } } fn main() -> unit { }",
        );
        let phi = predecessor.functions[0]
            .blocks
            .iter_mut()
            .flat_map(|block| &mut block.instructions)
            .find(|instruction| matches!(instruction, Instruction::Phi { .. }))
            .expect("fixture must contain phi");
        let Instruction::Phi { incoming, .. } = phi else {
            unreachable!();
        };
        incoming[0].0 = BasicBlockId(0);
        let error = verify(predecessor).expect_err("non-predecessor phi input must be rejected");
        assert_eq!(error.message, "invalid MIR phi predecessor");

        let mut mismatch = lower_fixture(
            "fn choose(value: bool) -> int { if value { 1 } else { 2 } } fn main() -> unit { }",
        );
        let phi = mismatch.functions[0]
            .blocks
            .iter_mut()
            .flat_map(|block| &mut block.instructions)
            .find(|instruction| matches!(instruction, Instruction::Phi { .. }))
            .expect("fixture must contain phi");
        let Instruction::Phi { incoming, .. } = phi else {
            unreachable!();
        };
        incoming[0].1 = Operand::Constant(Constant::Boolean(true));
        let error = verify(mismatch).expect_err("phi type mismatch must be rejected");
        assert_eq!(
            error.message,
            "MIR phi input type does not match result type"
        );
    }

    #[test]
    fn rejects_invalid_declaration_targets() {
        let mut function = lower_fixture(
            "fn identity(value: int) -> int { value } fn main() -> unit { let result = identity(1) }",
        );
        let call = function.functions[1].blocks[0]
            .instructions
            .iter_mut()
            .find(|instruction| matches!(instruction, Instruction::Call { .. }))
            .expect("fixture must contain call");
        let Instruction::Call { target, .. } = call else {
            unreachable!();
        };
        *target = CallTarget::Function(DefId(99));
        let error = verify(function).expect_err("invalid function ID must be rejected");
        assert_eq!(error.message, "invalid MIR function target ID");

        let mut newtype = lower_fixture(
            "type UserId = int fn build() -> UserId { UserId(1) } fn main() -> unit { }",
        );
        let Instruction::Call { target, .. } = &mut newtype.functions[0].blocks[0].instructions[0]
        else {
            panic!("fixture must contain newtype call");
        };
        *target = CallTarget::Newtype(DefId(99));
        let error = verify(newtype).expect_err("invalid newtype ID must be rejected");
        assert_eq!(error.message, "invalid MIR newtype target ID");

        let mut variant = lower_fixture(
            "enum Choice { Value(value: int) } fn build() -> Choice { Choice.Value(1) } fn main() -> unit { }",
        );
        let Instruction::Call { target, .. } = &mut variant.functions[0].blocks[0].instructions[0]
        else {
            panic!("fixture must contain variant call");
        };
        *target = CallTarget::Variant(VariantId(99));
        let error = verify(variant).expect_err("invalid variant ID must be rejected");
        assert_eq!(error.message, "invalid MIR variant target ID");

        let mut field_program = lower_fixture(
            "struct User { name: string } fn read(user: User) -> string { user.name } fn main() -> unit { }",
        );
        let load = field_program.functions[0].blocks[0]
            .instructions
            .iter_mut()
            .find(|instruction| matches!(instruction, Instruction::LoadField { .. }))
            .expect("fixture must contain field load");
        let Instruction::LoadField { field, .. } = load else {
            unreachable!();
        };
        *field = FieldId(99);
        let error = verify(field_program).expect_err("invalid field ID must be rejected");
        assert_eq!(error.message, "invalid MIR field target ID");
    }

    #[test]
    fn rejects_incompatible_call_arity_arguments_and_results() {
        let fixture = || {
            lower_fixture(
                "fn identity(value: int) -> int { value } fn main() -> unit { let result = identity(1) }",
            )
        };
        let mut arity = fixture();
        let Instruction::Call { arguments, .. } = second_function_call(&mut arity) else {
            unreachable!();
        };
        arguments.clear();
        let error = verify(arity).expect_err("wrong call arity must be rejected");
        assert_eq!(error.message, "invalid MIR call arity");

        let mut argument = fixture();
        let Instruction::Call { arguments, .. } = second_function_call(&mut argument) else {
            unreachable!();
        };
        arguments[0] = Operand::Constant(Constant::Boolean(true));
        let error = verify(argument).expect_err("wrong argument type must be rejected");
        assert_eq!(error.message, "invalid MIR call argument type");

        let mut result = fixture();
        let Instruction::Call { ty, .. } = second_function_call(&mut result) else {
            unreachable!();
        };
        *ty = Type::Bool;
        let error = verify(result).expect_err("wrong result type must be rejected");
        assert_eq!(error.message, "invalid MIR call result type");
    }

    #[test]
    fn rejects_match_pattern_binding_and_target_mismatches() {
        let mut pattern = lower_fixture(
            "fn pick(value: Option<int>) -> int { match value { Some(item) => item None => 0 } } fn main() -> unit { }",
        );
        let Terminator::Match { arms, .. } = &mut pattern.functions[0].blocks[0].terminator else {
            panic!("fixture must contain match");
        };
        arms[0].pattern = MatchPattern::Ok;
        let error = verify(pattern).expect_err("wrong pattern kind must be rejected");
        assert_eq!(error.message, "invalid MIR match pattern for target type");

        let mut binding = lower_fixture(
            "fn pick(value: Option<int>) -> int { match value { Some(item) => item None => 0 } } fn main() -> unit { }",
        );
        let Terminator::Match { arms, .. } = &mut binding.functions[0].blocks[0].terminator else {
            panic!("fixture must contain match");
        };
        arms[0].binding = Some(LocalId(99));
        let error = verify(binding).expect_err("undefined binding must be rejected");
        assert_eq!(error.message, "MIR match binds an undefined local");

        let mut binding_type = lower_fixture(
            "fn pick(value: Option<int>) -> int { match value { Some(item) => item None => 0 } } fn main() -> unit { }",
        );
        let binding_id = match &binding_type.functions[0].blocks[0].terminator {
            Terminator::Match { arms, .. } => arms[0].binding.expect("Some arm must bind payload"),
            _ => panic!("fixture must contain match"),
        };
        binding_type.functions[0]
            .locals
            .iter_mut()
            .find(|local| local.id == binding_id)
            .expect("binding local must exist")
            .ty = Type::Bool;
        let error = verify(binding_type).expect_err("binding type mismatch must be rejected");
        assert_eq!(
            error.message,
            "MIR match binding type does not match payload"
        );
    }

    #[test]
    fn rejects_uninitialized_local_and_non_dominating_value_in_unreachable_cycles() {
        let mut local_program = lower_fixture("fn main() -> unit { }");
        let location = local_program.functions[0].location;
        local_program.functions[0].locals.push(Local {
            id: LocalId(0),
            ty: Type::Int,
            mutable: false,
            location,
        });
        local_program.functions[0].blocks.extend([
            BasicBlock {
                id: BasicBlockId(1),
                instructions: vec![Instruction::Assign {
                    destination: ValueId(0),
                    operand: Operand::Local(LocalId(0)),
                    ty: Type::Int,
                    location,
                }],
                terminator: Terminator::Goto {
                    target: BasicBlockId(2),
                    location,
                },
            },
            BasicBlock {
                id: BasicBlockId(2),
                instructions: Vec::new(),
                terminator: Terminator::Goto {
                    target: BasicBlockId(1),
                    location,
                },
            },
        ]);
        let error = verify(local_program).expect_err("unreachable cycle cannot initialize locals");
        assert_eq!(error.message, "MIR uses an uninitialized local");

        let mut value_program = lower_fixture("fn main() -> unit { }");
        let location = value_program.functions[0].location;
        value_program.functions[0].blocks.extend([
            BasicBlock {
                id: BasicBlockId(1),
                instructions: vec![Instruction::Assign {
                    destination: ValueId(0),
                    operand: Operand::Constant(Constant::Integer(1)),
                    ty: Type::Int,
                    location,
                }],
                terminator: Terminator::Goto {
                    target: BasicBlockId(2),
                    location,
                },
            },
            BasicBlock {
                id: BasicBlockId(2),
                instructions: vec![Instruction::Assign {
                    destination: ValueId(1),
                    operand: Operand::Value(ValueId(0)),
                    ty: Type::Int,
                    location,
                }],
                terminator: Terminator::Goto {
                    target: BasicBlockId(1),
                    location,
                },
            },
        ]);
        let error = verify(value_program).expect_err("unreachable cycle cannot create dominance");
        assert_eq!(error.message, "MIR value is used before definition");
    }

    #[test]
    fn rejects_cross_branch_value_and_phi_after_non_phi_instruction() {
        let mut cross_branch = lower_fixture(
            "fn choose(value: bool) -> int { if value { 1 + 2 } else { 3 + 4 } } fn main() -> unit { }",
        );
        let then_value = match &cross_branch.functions[0].blocks[1].instructions[0] {
            Instruction::Binary { destination, .. } => *destination,
            _ => panic!("then arm must define a value"),
        };
        let phi = cross_branch.functions[0]
            .blocks
            .iter_mut()
            .flat_map(|block| &mut block.instructions)
            .find(|instruction| matches!(instruction, Instruction::Phi { .. }))
            .expect("fixture must contain phi");
        let Instruction::Phi { incoming, .. } = phi else {
            unreachable!();
        };
        incoming[1].1 = Operand::Value(then_value);
        let error = verify(cross_branch).expect_err("cross-branch value must not dominate");
        assert_eq!(error.message, "MIR value is used before definition");

        let mut placement = lower_fixture(
            "fn choose(value: bool) -> int { if value { 1 } else { 2 } } fn main() -> unit { }",
        );
        let location = placement.functions[0].location;
        let join = placement.functions[0]
            .blocks
            .iter_mut()
            .find(|block| matches!(block.instructions.first(), Some(Instruction::Phi { .. })))
            .expect("fixture must contain join");
        join.instructions.insert(
            0,
            Instruction::Assign {
                destination: ValueId(99),
                operand: Operand::Constant(Constant::Integer(0)),
                ty: Type::Int,
                location,
            },
        );
        let error = verify(placement).expect_err("phi after instruction must be rejected");
        assert_eq!(error.message, "MIR phi follows a non-phi instruction");
    }

    #[test]
    fn rejects_ambiguous_duplicate_and_non_unreachable_match_edges() {
        let fixture = || {
            lower_fixture(
                "fn pick(value: Option<int>) -> int { match value { Some(item) => item None => 0 } } fn main() -> unit { }",
            )
        };
        let mut shared = fixture();
        let Terminator::Match { arms, .. } = &mut shared.functions[0].blocks[0].terminator else {
            panic!("fixture must contain match");
        };
        arms[1].block = arms[0].block;
        let error =
            verify(shared).expect_err("shared arm target with different binding is ambiguous");
        assert_eq!(error.message, "ambiguous MIR match target binding");

        let mut duplicate = fixture();
        let Terminator::Match { arms, .. } = &mut duplicate.functions[0].blocks[0].terminator
        else {
            panic!("fixture must contain match");
        };
        arms[1].pattern = MatchPattern::Some;
        let error = verify(duplicate).expect_err("duplicate match pattern must be rejected");
        assert_eq!(error.message, "duplicate MIR match pattern");

        let mut fallback = fixture();
        let otherwise_block = match &fallback.functions[0].blocks[0].terminator {
            Terminator::Match { otherwise, .. } => *otherwise,
            _ => panic!("fixture must contain match"),
        };
        let join = fallback.functions[0]
            .blocks
            .iter()
            .find(|block| matches!(block.instructions.first(), Some(Instruction::Phi { .. })))
            .expect("fixture must contain join")
            .id;
        let location = fallback.functions[0].location;
        fallback.functions[0].blocks[otherwise_block.0 as usize].terminator = Terminator::Goto {
            target: join,
            location,
        };
        let error = verify(fallback).expect_err("match fallback must remain unreachable");
        assert_eq!(
            error.message,
            "MIR match otherwise block must be unreachable"
        );

        let mut fallback_arm = fixture();
        let Terminator::Match {
            arms, otherwise, ..
        } = &mut fallback_arm.functions[0].blocks[0].terminator
        else {
            panic!("fixture must contain match");
        };
        arms[0].block = *otherwise;
        let error = verify(fallback_arm).expect_err("match arm must not target fallback");
        assert_eq!(error.message, "MIR match arm targets otherwise block");
    }

    #[test]
    fn rejects_non_exhaustive_and_non_matchable_match_targets() {
        let mut option = lower_fixture(
            "fn pick(value: Option<int>) -> int { match value { Some(item) => item None => 0 } } fn main() -> unit { }",
        );
        let Terminator::Match { arms, .. } = &mut option.functions[0].blocks[0].terminator else {
            panic!("fixture must contain match");
        };
        arms.pop();
        assert_eq!(
            verify(option)
                .expect_err("Option match must be exhaustive")
                .message,
            "non-exhaustive MIR match"
        );

        let mut result = lower_fixture(
            "fn pick(value: Result<int, string>) -> int { match value { Ok(item) => item Err(reason) => 0 } } fn main() -> unit { }",
        );
        let Terminator::Match { arms, .. } = &mut result.functions[0].blocks[0].terminator else {
            panic!("fixture must contain match");
        };
        arms.pop();
        assert_eq!(
            verify(result)
                .expect_err("Result match must be exhaustive")
                .message,
            "non-exhaustive MIR match"
        );

        let mut enumeration = lower_fixture(
            "enum Choice { First Second } fn pick(value: Choice) -> int { match value { Choice.First => 1 Choice.Second => 2 } } fn main() -> unit { }",
        );
        let Terminator::Match { arms, .. } = &mut enumeration.functions[0].blocks[0].terminator
        else {
            panic!("fixture must contain match");
        };
        arms.pop();
        assert_eq!(
            verify(enumeration)
                .expect_err("enum match must be exhaustive")
                .message,
            "non-exhaustive MIR match"
        );

        let mut integer = lower_fixture(
            "fn pick(value: Option<int>) -> int { match value { Some(item) => item None => 0 } } fn main() -> unit { }",
        );
        let Terminator::Match { target, .. } = &mut integer.functions[0].blocks[0].terminator
        else {
            panic!("fixture must contain match");
        };
        *target = Operand::Constant(Constant::Integer(1));
        assert_eq!(
            verify(integer)
                .expect_err("int cannot be a match target")
                .message,
            "invalid MIR match target type"
        );
    }

    #[test]
    fn rejects_phi_without_predecessors() {
        let mut program = lower_fixture("fn main() -> unit { }");
        let location = program.functions[0].location;
        program.functions[0].blocks.push(BasicBlock {
            id: BasicBlockId(1),
            instructions: vec![Instruction::Phi {
                destination: ValueId(0),
                incoming: Vec::new(),
                ty: Type::Int,
                location,
            }],
            terminator: Terminator::Unreachable { location },
        });
        assert_eq!(
            verify(program)
                .expect_err("phi requires at least one predecessor")
                .message,
            "invalid MIR phi predecessor"
        );
    }

    #[test]
    fn rejects_payload_variant_constant() {
        let mut program = lower_fixture(
            "enum Choice { Empty Value(value: int) } fn build() -> Choice { Choice.Empty } fn main() -> unit { }",
        );
        let payload = program.enums[0].variants[1].id;
        let Terminator::Return {
            value: Some(Operand::Constant(Constant::Variant(variant))),
            ..
        } = &mut program.functions[0].blocks[0].terminator
        else {
            panic!("fixture must return a variant constant");
        };
        *variant = payload;
        let error = verify(program).expect_err("payload variant needs a constructor call");
        assert_eq!(error.message, "MIR payload variant cannot be a constant");
    }

    #[test]
    fn rejects_duplicate_and_misaligned_ids() {
        let mut block = lower_fixture("fn main() -> unit { }");
        block.functions[0].blocks[0].id = BasicBlockId(2);
        assert_eq!(
            verify(block)
                .expect_err("block ID must match index")
                .message,
            "invalid MIR block ID"
        );

        let mut local = lower_fixture("fn main() -> unit { let left = 1 let right = 2 }");
        local.functions[0].locals[1].id = local.functions[0].locals[0].id;
        assert_eq!(
            verify(local)
                .expect_err("duplicate local must fail")
                .message,
            "duplicate MIR local ID"
        );

        let mut parameter =
            lower_fixture("fn pair(left: int, right: int) -> int { left } fn main() -> unit { }");
        parameter.functions[0].parameters[1].id = parameter.functions[0].parameters[0].id;
        assert_eq!(
            verify(parameter)
                .expect_err("duplicate parameter must fail")
                .message,
            "duplicate MIR parameter ID"
        );

        let mut top_level = lower_fixture("struct User { name: string } fn main() -> unit { }");
        top_level.structs[0].id = top_level.functions[0].id.0;
        assert_eq!(
            verify(top_level)
                .expect_err("duplicate declaration must fail")
                .message,
            "duplicate MIR declaration ID"
        );

        let mut field =
            lower_fixture("struct User { name: string active: bool } fn main() -> unit { }");
        field.structs[0].fields[1].id = field.structs[0].fields[0].id;
        assert_eq!(
            verify(field)
                .expect_err("duplicate field must fail")
                .message,
            "duplicate MIR field ID"
        );

        let mut variant = lower_fixture("enum Choice { First Second } fn main() -> unit { }");
        variant.enums[0].variants[1].id = variant.enums[0].variants[0].id;
        assert_eq!(
            verify(variant)
                .expect_err("duplicate variant must fail")
                .message,
            "duplicate MIR variant ID"
        );
    }

    #[test]
    fn rejects_representative_instruction_type_and_layout_errors() {
        let mut string = lower_fixture(
            "fn render(value: int) -> string { \"value: {value}\" } fn main() -> unit { }",
        );
        let Instruction::BuildString { ty, parts, .. } =
            &mut string.functions[0].blocks[0].instructions[1]
        else {
            panic!("fixture must build an interpolated string");
        };
        assert!(matches!(parts[1], StringPart::Value(_)));
        *ty = Type::Bool;
        assert_eq!(
            verify(string)
                .expect_err("string result type must fail")
                .message,
            "invalid MIR string result type"
        );

        let mut list = lower_fixture("fn values() -> List<int> { [1, 2] } fn main() -> unit { }");
        let Instruction::BuildList { ty, .. } = &mut list.functions[0].blocks[0].instructions[0]
        else {
            panic!("fixture must build a list");
        };
        *ty = Type::List(Box::new(Type::Bool));
        assert_eq!(
            verify(list)
                .expect_err("list element type must fail")
                .message,
            "MIR operand type mismatch"
        );

        let mut map = lower_fixture(
            "fn values() -> Map<string, int> { { \"port\": 80 } } fn main() -> unit { }",
        );
        let Instruction::BuildMap { ty, .. } = &mut map.functions[0].blocks[0].instructions[0]
        else {
            panic!("fixture must build a map");
        };
        *ty = Type::Map(Box::new(Type::Bool));
        assert_eq!(
            verify(map).expect_err("map value type must fail").message,
            "MIR operand type mismatch"
        );

        let mut tuple =
            lower_fixture("fn values() -> (int, bool) { (1, true) } fn main() -> unit { }");
        let Instruction::BuildTuple { ty, .. } = &mut tuple.functions[0].blocks[0].instructions[0]
        else {
            panic!("fixture must build a tuple");
        };
        *ty = Type::Tuple(vec![Type::Int]);
        assert_eq!(
            verify(tuple).expect_err("tuple arity must fail").message,
            "invalid MIR tuple arity"
        );

        let mut structure = lower_fixture(
            "struct User { name: string active: bool } fn build() -> User { User { name: \"Lin\" active: true } } fn main() -> unit { }",
        );
        let Instruction::BuildStruct { fields, .. } =
            &mut structure.functions[0].blocks[0].instructions[0]
        else {
            panic!("fixture must build a struct");
        };
        fields.pop();
        assert_eq!(
            verify(structure)
                .expect_err("struct layout must fail")
                .message,
            "invalid MIR struct field count"
        );

        let mut field = lower_fixture(
            "struct User { name: string } fn read(user: User) -> string { user.name } fn main() -> unit { }",
        );
        let load = field.functions[0].blocks[0]
            .instructions
            .iter_mut()
            .find(|instruction| matches!(instruction, Instruction::LoadField { .. }))
            .expect("fixture must load a field");
        let Instruction::LoadField { target, .. } = load else {
            unreachable!();
        };
        *target = Operand::Constant(Constant::Integer(1));
        assert_eq!(
            verify(field)
                .expect_err("field target type must fail")
                .message,
            "MIR operand type mismatch"
        );

        let mut iterator =
            lower_fixture("fn main() -> unit { for item in [1] { let seen = item } }");
        let init = iterator.functions[0]
            .blocks
            .iter_mut()
            .flat_map(|block| &mut block.instructions)
            .find(|instruction| matches!(instruction, Instruction::IterInit { .. }))
            .expect("fixture must initialize an iterator");
        let Instruction::IterInit { ty, .. } = init else {
            unreachable!();
        };
        *ty = Type::Bool;
        assert_eq!(
            verify(iterator)
                .expect_err("iterator type must fail")
                .message,
            "invalid MIR iterator type"
        );
    }

    #[test]
    fn rejects_builtin_and_receiver_call_signature_errors() {
        let mut bytes =
            lower_fixture("fn decode() -> bytes { bytes.from_hex(\"a1\") } fn main() -> unit { }");
        let Instruction::Call { arguments, .. } = &mut bytes.functions[0].blocks[0].instructions[0]
        else {
            panic!("fixture must call bytes.from_hex");
        };
        arguments[0] = Operand::Constant(Constant::Boolean(true));
        assert_eq!(
            verify(bytes)
                .expect_err("bytes argument type must fail")
                .message,
            "invalid MIR call argument type"
        );

        let mut some = lower_fixture("fn wrap() -> Option<int> { Some(1) } fn main() -> unit { }");
        let Instruction::Call { ty, .. } = &mut some.functions[0].blocks[0].instructions[0] else {
            panic!("fixture must call Some");
        };
        *ty = Type::Bool;
        assert_eq!(
            verify(some)
                .expect_err("Some result type must fail")
                .message,
            "invalid MIR call result type"
        );

        let mut receiver = lower_fixture(
            "fn parse(text: string, flag: bool) -> Result<int, unit> { text.to_int() } fn main() -> unit { }",
        );
        let flag = receiver.functions[0].parameters[1].id;
        let call = receiver.functions[0].blocks[0]
            .instructions
            .iter_mut()
            .find(|instruction| matches!(instruction, Instruction::Call { .. }))
            .expect("fixture must call string.to_int");
        let Instruction::Call { target, .. } = call else {
            unreachable!();
        };
        *target = CallTarget::StringToInt(flag);
        let error = verify(receiver).expect_err("receiver type must fail");
        assert_eq!(error.message, "invalid MIR call receiver type");
    }

    #[test]
    fn verifies_string_to_int_builtin_signature() {
        verify(lower_fixture(
            "fn parse(text: string) -> Result<int, unit> { text.to_int() } fn main() -> unit { }",
        ))
        .expect("lowered string.to_int signature must verify");
    }

    #[test]
    fn rejects_branch_return_and_match_payload_contract_errors() {
        let mut branch = lower_fixture(
            "fn choose(value: bool) -> int { if value { 1 } else { 2 } } fn main() -> unit { }",
        );
        let Terminator::Branch { condition, .. } = &mut branch.functions[0].blocks[0].terminator
        else {
            panic!("fixture must branch");
        };
        *condition = Operand::Constant(Constant::Integer(1));
        assert_eq!(
            verify(branch)
                .expect_err("branch condition must fail")
                .message,
            "invalid MIR branch condition type"
        );

        let mut unit_return = lower_fixture("fn main() -> unit { }");
        let location = unit_return.functions[0].location;
        unit_return.functions[0].blocks[0].terminator = Terminator::Return {
            value: Some(Operand::Constant(Constant::Integer(1))),
            location,
        };
        assert_eq!(
            verify(unit_return)
                .expect_err("unit return operand must fail")
                .message,
            "invalid MIR return value"
        );

        let mut typed_return = lower_fixture("fn value() -> int { 1 } fn main() -> unit { }");
        let Terminator::Return { value, .. } = &mut typed_return.functions[0].blocks[0].terminator
        else {
            panic!("fixture must return");
        };
        *value = Some(Operand::Constant(Constant::Boolean(true)));
        assert_eq!(
            verify(typed_return)
                .expect_err("return type must fail")
                .message,
            "invalid MIR return type"
        );

        let mut binding = lower_fixture(
            "fn pick(value: Option<int>) -> int { match value { Some(item) => item None => 0 } } fn main() -> unit { }",
        );
        let Terminator::Match { arms, .. } = &mut binding.functions[0].blocks[0].terminator else {
            panic!("fixture must match");
        };
        arms[0].binding = None;
        assert_eq!(
            verify(binding)
                .expect_err("payload pattern requires binding")
                .message,
            "invalid MIR match payload binding"
        );
    }

    #[test]
    fn rejects_result_propagation_contract_errors() {
        let fixture = || {
            lower_fixture(
                "fn unwrap(value: Result<int, string>) -> Result<int, string> { let item = value? Ok(item) } fn main() -> unit { }",
            )
        };
        let mut operand = fixture();
        let Terminator::PropagateErr { result, .. } =
            &mut propagation_block(&mut operand).terminator
        else {
            unreachable!();
        };
        *result = Operand::Constant(Constant::Integer(1));
        assert_eq!(
            verify(operand)
                .expect_err("propagation operand must fail")
                .message,
            "invalid MIR propagation operand type"
        );

        let mut result = fixture();
        let Terminator::PropagateErr { ty, .. } = &mut propagation_block(&mut result).terminator
        else {
            unreachable!();
        };
        *ty = Type::Bool;
        assert_eq!(
            verify(result)
                .expect_err("propagation result must fail")
                .message,
            "invalid MIR propagation result type"
        );

        let mut error = fixture();
        error.functions[0].return_type = Type::Result(Box::new(Type::Int), Box::new(Type::Bool));
        assert_eq!(
            verify(error)
                .expect_err("propagation error must fail")
                .message,
            "invalid MIR propagation error type"
        );

        let mut destination = fixture();
        let existing = match &destination.functions[0].blocks[0].instructions[0] {
            Instruction::Assign { destination, .. } => *destination,
            _ => panic!("fixture must snapshot its Result parameter"),
        };
        let Terminator::PropagateErr {
            destination: propagated,
            ..
        } = &mut propagation_block(&mut destination).terminator
        else {
            unreachable!();
        };
        *propagated = existing;
        assert_eq!(
            verify(destination)
                .expect_err("propagation destination must be unique")
                .message,
            "duplicate MIR value ID"
        );
    }

    #[test]
    fn lowers_if_to_branch_then_else_and_join_blocks() {
        let program =
            lower_fixture("fn choose() -> int { if true { 1 } else { 2 } } fn main() -> unit { }");
        let function = &program.functions[0];
        assert!(matches!(
            function.blocks[0].terminator,
            Terminator::Branch {
                then_block: BasicBlockId(1),
                else_block: BasicBlockId(2),
                ..
            }
        ));
        let join = function
            .blocks
            .iter()
            .find(|block| matches!(block.instructions.first(), Some(Instruction::Phi { .. })))
            .expect("if value must have a join block");
        let Instruction::Phi { incoming, ty, .. } = &join.instructions[0] else {
            panic!("join must start with phi");
        };
        assert_eq!(ty, &Type::Int);
        assert_eq!(incoming.len(), 2);
        for (predecessor, value) in incoming {
            assert!(matches!(
                value,
                Operand::Constant(super::Constant::Integer(_))
            ));
            assert!(matches!(
                function.blocks[predecessor.0 as usize].terminator,
                Terminator::Goto { target, .. } if target == join.id
            ));
        }
    }

    #[test]
    fn lowers_option_result_and_enum_matches_to_match_targets() {
        for source in [
            "fn pick(value: Option<int>) -> int { match value { Some(item) => item None => 0 } } fn main() -> unit { }",
            "fn pick(value: Result<int, string>) -> int { match value { Ok(item) => item Err(reason) => 0 } } fn main() -> unit { }",
            "enum Choice { First Second } fn pick(value: Choice) -> int { match value { Choice.First => 1 Choice.Second => 2 } } fn main() -> unit { }",
        ] {
            let program = lower_fixture(source);
            let Terminator::Match { arms, .. } = &program.functions[0].blocks[0].terminator else {
                panic!("match expression must lower to a match terminator");
            };
            assert!(!arms.is_empty());
            assert!(arms.iter().all(|arm| matches!(
                arm.pattern,
                MatchPattern::Variant(_)
                    | MatchPattern::Some
                    | MatchPattern::None
                    | MatchPattern::Ok
                    | MatchPattern::Err
            )));
        }
    }

    #[test]
    fn lowers_for_to_iterator_branch_and_back_edge() {
        let program = lower_fixture("fn main() -> unit { for item in [1, 2] { let seen = item } }");
        let function = &program.functions[0];
        let condition = function
            .blocks
            .iter()
            .find(|block| {
                block
                    .instructions
                    .iter()
                    .any(|instruction| matches!(instruction, Instruction::IterNext { .. }))
            })
            .expect("for must have an iterator condition block");
        let body = match condition.terminator {
            Terminator::Branch { then_block, .. } => then_block,
            _ => panic!("iterator condition must branch"),
        };
        assert!(matches!(
            function.blocks[body.0 as usize].terminator,
            Terminator::Goto { target, .. } if target == condition.id
        ));
    }

    #[test]
    fn lowers_early_return_to_return_terminator() {
        let program = lower_fixture(
            "fn choose(value: bool) -> int { if value { return 1 } else { 2 } } fn main() -> unit { }",
        );
        assert!(program.functions[0]
            .blocks
            .iter()
            .skip(1)
            .any(|block| matches!(block.terminator, Terminator::Return { .. })));
    }

    #[test]
    fn lowers_result_propagation_to_propagate_err_terminator() {
        let program = lower_fixture(
            "fn unwrap(value: Result<int, string>) -> Result<int, string> { let item = value? Ok(item) } fn main() -> unit { }",
        );
        assert!(program.functions[0]
            .blocks
            .iter()
            .any(|block| matches!(block.terminator, Terminator::PropagateErr { .. })));
    }

    #[test]
    fn verifies_result_return_with_never_constructor_error() {
        let program = lower_fixture(
            "fn unwrap(value: Result<int, unit>) -> Result<int, unit> { let item = value? Ok(item) } fn main() -> unit { }",
        );

        verify(program).expect("a Result constructor with Never must return from Result context");
    }

    #[test]
    fn lowers_tuple_destructure_to_element_stores() {
        let program = lower_fixture(
            "fn first() -> int { let (left, right) = (1, 2) left } fn main() -> unit { }",
        );
        let instructions = &program.functions[0].blocks[0].instructions;
        assert_eq!(
            instructions
                .iter()
                .filter(|item| matches!(item, Instruction::TupleElement { .. }))
                .count(),
            2
        );
        assert!(
            instructions
                .iter()
                .filter(|item| matches!(item, Instruction::StoreLocal { .. }))
                .count()
                >= 2
        );
        assert!(instructions.iter().any(|instruction| matches!(
            instruction,
            Instruction::StoreLocal {
                value: Operand::Value(_),
                ..
            }
        )));
    }

    #[test]
    fn lowers_struct_defaults_into_each_construction() {
        let program = lower_fixture(
            "struct User { name: string active: bool = true } fn build() -> User { User { name: \"Lin\" } } fn main() -> unit { }",
        );
        assert!(program.functions[0].blocks[0]
            .instructions
            .iter()
            .any(|instruction| matches!(
                instruction,
                Instruction::BuildStruct { fields, .. } if fields.len() == 2
            )));
    }

    #[test]
    fn rejects_a_second_terminator_with_the_specific_lowering_error() {
        let typed = typed_fixture("fn main() -> unit { }");
        let function = &typed.functions[0];
        let mut lowerer = FunctionLowerer::new(function, &typed.structs);
        let location = yan_source::SourceLocation::new(function.source, function.span);
        lowerer
            .terminate(Terminator::Return {
                value: None,
                location,
            })
            .expect("first terminator must be accepted");

        let error = lowerer
            .terminate(Terminator::Unreachable { location })
            .expect_err("second terminator must be rejected");
        assert_eq!(error.location, location);
        assert_eq!(error.message, "MIR basic block has multiple terminators");
    }

    #[test]
    fn preserves_types_and_source_locations_on_complete_cfg_blocks() {
        let program = lower_fixture(
            "fn choose(value: bool) -> int { if value { 1 + 2 } else { 3 } } fn main() -> unit { }",
        );
        let function = &program.functions[0];

        assert!(function.blocks.iter().all(|block| matches!(
            block.terminator,
            Terminator::Goto { .. }
                | Terminator::Branch { .. }
                | Terminator::Match { .. }
                | Terminator::Return { .. }
                | Terminator::PropagateErr { .. }
                | Terminator::Unreachable { .. }
        )));
        assert!(
            function
                .blocks
                .iter()
                .all(|block| terminator_location(&block.terminator).source
                    == function.location.source)
        );
        let binary = function
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .find(|instruction| matches!(instruction, Instruction::Binary { .. }))
            .expect("then branch must contain binary instruction");
        let Instruction::Binary { ty, location, .. } = binary else {
            panic!("selected instruction must remain binary");
        };
        assert_eq!(ty, &Type::Int);
        assert_eq!(location.source, function.location.source);
        assert_ne!(location.span, yan_source::Span::default());
    }

    #[test]
    fn snapshots_local_reads_before_later_branch_side_effects() {
        let program = lower_fixture(
            "fn calculate() -> int { let mut value = 1 value + if true { value = 2 value } else { value } } fn main() -> unit { }",
        );
        let function = &program.functions[0];
        let Instruction::Assign {
            destination: snapshot,
            operand: Operand::Local(_),
            ty: Type::Int,
            ..
        } = &function.blocks[0].instructions[1]
        else {
            panic!("left local read must be snapshotted before branching");
        };
        let binary = function
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .find(|instruction| matches!(instruction, Instruction::Binary { .. }))
            .expect("expression must contain the final addition");
        assert!(matches!(
            binary,
            Instruction::Binary {
                left: Operand::Value(value),
                ..
            } if value == snapshot
        ));
    }

    #[test]
    fn propagates_nested_return_from_call_argument() {
        let program = lower_fixture(
            "fn consume(value: int) -> int { value } fn choose() -> int { consume(return 2) } fn main() -> unit { }",
        );
        let function = &program.functions[1];

        assert!(matches!(
            function.blocks[0].terminator,
            Terminator::Return {
                value: Some(Operand::Constant(super::Constant::Integer(2))),
                ..
            }
        ));
        assert!(!function.blocks[0]
            .instructions
            .iter()
            .any(|instruction| matches!(instruction, Instruction::Call { .. })));
    }

    #[test]
    fn normalizes_explicit_unit_return_to_no_operand() {
        let program = lower_fixture(
            "import yan.platform.console fn finish() -> unit { return console.println(\"done\") } fn main() -> unit { }",
        );

        assert!(matches!(
            program.functions[0].blocks[0].terminator,
            Terminator::Return { value: None, .. }
        ));
    }
}
