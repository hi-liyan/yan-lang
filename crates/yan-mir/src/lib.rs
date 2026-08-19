//! 已类型化 Yan 程序到目标无关中间表示的 lowering。
//!
//! M14 的首个切片先建立 MIR 程序、函数和基本块的稳定数据边界。后续提交会在不增加
//! Yan 表面语法的前提下，把 Typed HIR 中的表达式和控制流逐步降低为指令与终结指令。

use yan_hir::{DefId, Type};
use yan_source::Span;
use yan_typeck::{TypedExpression, TypedFunction, TypedProgram, TypedStatement};

/// MIR 程序。
///
/// MIR 只接受已经通过类型检查的程序，因而后端和解释器不需要重新决定 Yan 的名称或
/// 类型规则。每个源函数都对应一个 MIR 函数。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Program {
    /// 保留的已类型化程序，供后续完整 lowering 使用。
    typed: TypedProgram,
    /// 按源声明顺序排列的函数控制流图。
    pub functions: Vec<Function>,
}

impl Program {
    /// 返回生成本 MIR 的已类型化程序。
    pub const fn typed_program(&self) -> &TypedProgram {
        &self.typed
    }
}

/// MIR 内函数的稳定索引。
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FunctionId(pub DefId);

/// MIR 内基本块的稳定索引。
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BasicBlockId(pub u32);

/// MIR 内局部存储位置的稳定索引。
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LocalId(pub u32);

/// 已降低函数的控制流图。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Function {
    /// 函数在 MIR 程序中的索引。
    pub id: FunctionId,
    /// 源函数名称，仅供诊断和调试显示，不能用于语义查找。
    pub name: String,
    /// 函数声明位置。
    pub span: Span,
    /// 函数返回类型。
    pub return_type: Type,
    /// 此函数的局部存储位置。
    pub locals: Vec<Local>,
    /// 按稳定索引排列的基本块。
    pub blocks: Vec<BasicBlock>,
}

/// 一个 MIR 局部存储位置。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Local {
    /// 局部位置在所属函数中的索引。
    pub id: LocalId,
    /// 该位置承载的 Yan 类型。
    pub ty: Type,
    /// 是否允许赋值覆盖该局部位置。
    pub mutable: bool,
    /// 与此局部有关的源码位置。
    pub span: Span,
}

/// 一个 MIR 基本块。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BasicBlock {
    /// 基本块在所属函数中的索引。
    pub id: BasicBlockId,
    /// 块内按顺序执行的指令。
    pub statements: Vec<Statement>,
    /// 块的唯一控制流出口。
    pub terminator: Terminator,
}

/// MIR 指令。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Statement {
    /// 初始化一个局部位置。
    Declare {
        local: LocalId,
        value: TypedExpression,
        span: Span,
    },
    /// 覆盖一个已类型检查为可变的局部位置。
    Assign {
        local: LocalId,
        value: TypedExpression,
        span: Span,
    },
    /// 为副作用执行非尾表达式。
    Evaluate { value: TypedExpression, span: Span },
}

/// MIR 基本块终结指令。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Terminator {
    /// 返回尾表达式；无值表示 unit。
    Return {
        value: Option<TypedExpression>,
        span: Span,
    },
}

/// 将已类型化 HIR 建立为最小 MIR 控制流图。
///
/// 顺序函数体会进入一个入口基本块；嵌套控制流保持显式 pending，不能被后端执行。
pub fn lower(typed: TypedProgram) -> Program {
    let functions = typed
        .functions
        .iter()
        .map(lower_function)
        .collect();
    Program { typed, functions }
}

/// 为每个 Typed HIR 函数建立独立的 MIR 控制流图入口。
fn lower_function(function: &TypedFunction) -> Function {
    let mut locals = function
        .parameters
        .iter()
        .map(|parameter| Local {
            id: LocalId(parameter.id.0),
            ty: parameter.ty.clone(),
            mutable: parameter.mutable,
            span: parameter.span,
        })
        .collect::<Vec<_>>();
    let mut statements = Vec::new();
    let mut terminator = Terminator::Return {
        value: None,
        span: function.span,
    };
    for (index, statement) in function.statements.iter().enumerate() {
        let is_tail = index + 1 == function.statements.len();
        match statement {
            TypedStatement::Let { local, value } => {
                let id = LocalId(local.id.0);
                locals.push(Local {
                    id,
                    ty: local.ty.clone(),
                    mutable: local.mutable,
                    span: local.span,
                });
                statements.push(Statement::Declare {
                    local: id,
                    value: value.clone(),
                    span: local.span,
                });
            }
            TypedStatement::Assign { local, value, span } => statements.push(Statement::Assign {
                local: LocalId(local.0),
                value: value.clone(),
                span: *span,
            }),
            TypedStatement::Destructure { locals: bindings, value } => {
                for local in bindings {
                    let id = LocalId(local.id.0);
                    locals.push(Local {
                        id,
                        ty: local.ty.clone(),
                        mutable: false,
                        span: local.span,
                    });
                }
                statements.push(Statement::Evaluate {
                    value: value.clone(),
                    span: value.span,
                });
            }
            TypedStatement::Expression(value) if is_tail => {
                terminator = Terminator::Return {
                    value: Some(value.clone()),
                    span: value.span,
                };
            }
            TypedStatement::Expression(value) => statements.push(Statement::Evaluate {
                value: value.clone(),
                span: value.span,
            }),
        }
    }
    Function {
        id: FunctionId(function.id),
        name: function.name.clone(),
        span: function.span,
        return_type: function.return_type.clone(),
        locals,
        blocks: vec![BasicBlock {
            id: BasicBlockId(0),
            statements,
            terminator,
        }],
    }
}

#[cfg(test)]
mod tests {
    use yan_hir::{lower as lower_hir, DefId};
    use yan_syntax::{lex, parse};
    use yan_typeck::check;

    use super::{lower, BasicBlockId, FunctionId, Statement, Terminator};

    #[test]
    fn lowers_sequential_bindings_into_locals_and_return() {
        let source = "import yan.platform.console fn main() -> unit { let mut count = 1 count = count + 1 console.println(count) }";
        let tokens = lex(source).expect("fixture must lex");
        let syntax = parse(source, &tokens).expect("fixture must parse");
        let typed = check(&lower_hir(syntax).expect("fixture must lower"))
            .expect("fixture must type check");

        let program = lower(typed);

        assert_eq!(program.functions.len(), 1);
        assert_eq!(program.functions[0].id, FunctionId(DefId(0)));
        assert_eq!(program.functions[0].blocks[0].id, BasicBlockId(0));
        assert_eq!(program.functions[0].locals.len(), 1);
        assert_eq!(program.functions[0].blocks[0].statements.len(), 2);
        assert!(matches!(
            program.functions[0].blocks[0].statements[0],
            Statement::Declare { .. }
        ));
        assert!(matches!(
            program.functions[0].blocks[0].terminator,
            Terminator::Return { .. }
        ));
    }
}
