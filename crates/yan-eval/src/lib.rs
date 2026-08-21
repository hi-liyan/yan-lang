//! MIR 的受限解释执行器。
use std::collections::HashMap;
use yan_mir::{
    BasicBlockId, BinaryOperator, CallTarget, Constant, DefId, FieldId, Function, Instruction,
    LocalId, MatchPattern, Operand, SourceLocation, Span, StringPart, Terminator, ValueId,
    VariantId, VerifiedProgram,
};

/// 执行已经通过 MIR 验证的程序，并返回平台控制台输出。
///
/// 解释器只消费验证后的稳定 MIR ID，不重新进行名称解析或类型检查。
pub fn execute(program: &VerifiedProgram) -> Result<Vec<String>, EvalError> {
    let main = program
        .functions()
        .iter()
        .find(|f| f.name == "main")
        .ok_or_else(|| {
            EvalError::new(
                SourceLocation::new(program.source(), Span::default()),
                "undefined function `main`",
            )
        })?;
    let mut out = Vec::new();
    match call(program, main.id.0, Vec::new(), &mut out)? {
        // 旧解释器将顶层 `return`（包括 `?` 传播的 Err）视为程序正常结束；
        // 保持该可观察 CLI 行为，错误 Result 只由 Yan 程序自身决定如何输出。
        Value::Unit | Value::Outcome(Ok(_)) | Value::Outcome(Err(_)) => Ok(out),
        v => Err(EvalError::new(
            main.location,
            format!("main returned an unsupported value `{}`", v.display()),
        )),
    }
}
/// 执行期间可映射回 Yan 源码位置的稳定错误。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvalError {
    /// 产生错误的完整源位置。
    pub location: SourceLocation,
    /// 稳定英文错误原因。
    pub message: String,
}
impl EvalError {
    /// 以完整源位置和稳定英文原因构造错误。
    pub fn new(location: SourceLocation, message: impl Into<String>) -> Self {
        Self {
            location,
            message: message.into(),
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
enum Value {
    Integer(i64),
    Float(String),
    Boolean(bool),
    String(String),
    Bytes(Vec<u8>),
    List(Vec<Value>),
    Map(Vec<(String, Value)>),
    Tuple(Vec<Value>),
    Outcome(Result<Box<Value>, Box<Value>>),
    Optional(Option<Box<Value>>),
    Struct(HashMap<FieldId, Value>),
    Enum(VariantId, Option<Box<Value>>),
    Iterator { values: Vec<Value>, next: usize },
    Unit,
}
impl Value {
    fn display(&self) -> String {
        match self {
            Self::Integer(v) => v.to_string(),
            Self::Float(v) | Self::String(v) => v.clone(),
            Self::Boolean(v) => v.to_string(),
            Self::Bytes(v) => format!(
                "0x{}",
                v.iter().map(|x| format!("{x:02x}")).collect::<String>()
            ),
            Self::Unit => "unit".into(),
            Self::Outcome(Ok(v)) => format!("Ok({})", v.display()),
            Self::Outcome(Err(v)) => format!("Err({})", v.display()),
            Self::Optional(Some(v)) => format!("Some({})", v.display()),
            Self::Optional(None) => "None".into(),
            Self::List(v) | Self::Tuple(v) => format!(
                "{}{}{}",
                if matches!(self, Self::List(_)) {
                    "["
                } else {
                    "("
                },
                v.iter().map(Value::display).collect::<Vec<_>>().join(", "),
                if matches!(self, Self::List(_)) {
                    ']'
                } else {
                    ')'
                }
            ),
            Self::Map(v) => format!(
                "{{{}}}",
                v.iter()
                    .map(|(k, v)| format!("{k}: {}", v.display()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Struct(_) => "struct".into(),
            Self::Enum(_, _) => "enum".into(),
            Self::Iterator { .. } => "iterator".into(),
        }
    }
}
struct Frame {
    locals: HashMap<LocalId, Value>,
    values: HashMap<ValueId, Value>,
}
impl Frame {
    fn new(f: &Function, args: Vec<Value>) -> Self {
        Self {
            locals: f
                .parameters
                .iter()
                .zip(args)
                .map(|(p, v)| (p.id, v))
                .collect(),
            values: HashMap::new(),
        }
    }
    fn get(&self, o: &Operand, l: SourceLocation) -> Result<Value, EvalError> {
        match o {
            Operand::Constant(c) => Ok(constant(c)),
            Operand::Local(id) => self
                .locals
                .get(id)
                .cloned()
                .ok_or_else(|| EvalError::new(l, "MIR local is missing")),
            Operand::Value(id) => self
                .values
                .get(id)
                .cloned()
                .ok_or_else(|| EvalError::new(l, "MIR value is missing")),
        }
    }
}
enum Control {
    Next(BasicBlockId, Option<(LocalId, Value)>),
    Return(Value),
}
fn call(
    p: &VerifiedProgram,
    id: DefId,
    args: Vec<Value>,
    out: &mut Vec<String>,
) -> Result<Value, EvalError> {
    let f = p.functions().iter().find(|f| f.id.0 == id).ok_or_else(|| {
        EvalError::new(
            SourceLocation::new(p.source(), Span::default()),
            "MIR function ID is missing",
        )
    })?;
    let mut frame = Frame::new(f, args);
    let (mut current, mut previous) = (BasicBlockId(0), None);
    loop {
        let b = f
            .blocks
            .get(current.0 as usize)
            .ok_or_else(|| EvalError::new(f.location, "MIR basic block is missing"))?;
        for i in &b.instructions {
            instruction(p, i, previous, &mut frame, out)?
        }
        match terminator(p, &b.terminator, &mut frame, out)? {
            Control::Return(v) => return Ok(v),
            Control::Next(next, binding) => {
                previous = Some(current);
                current = next;
                if let Some((id, v)) = binding {
                    frame.locals.insert(id, v);
                }
            }
        }
    }
}
fn put(f: &mut Frame, id: ValueId, v: Value) {
    f.values.insert(id, v);
}
fn operands(f: &Frame, os: &[Operand], l: SourceLocation) -> Result<Vec<Value>, EvalError> {
    os.iter().map(|o| f.get(o, l)).collect()
}
fn instruction(
    p: &VerifiedProgram,
    i: &Instruction,
    prev: Option<BasicBlockId>,
    f: &mut Frame,
    out: &mut Vec<String>,
) -> Result<(), EvalError> {
    match i {
        Instruction::Assign {
            destination,
            operand,
            location,
            ..
        } => {
            let v = f.get(operand, *location)?;
            put(f, *destination, v)
        }
        Instruction::StoreLocal {
            local,
            value,
            location,
            ..
        } => {
            let v = f.get(value, *location)?;
            f.locals.insert(*local, v);
        }
        Instruction::Binary {
            destination,
            operator,
            left,
            right,
            location,
            ..
        } => {
            let a = f.get(left, *location)?;
            let b = f.get(right, *location)?;
            put(f, *destination, binary(*operator, a, b, *location)?)
        }
        Instruction::BuildString {
            destination,
            parts,
            location,
            ..
        } => {
            let mut s = String::new();
            for x in parts {
                match x {
                    StringPart::Text(v) => s.push_str(v),
                    StringPart::Value(v) => s.push_str(&f.get(v, *location)?.display()),
                }
            }
            put(f, *destination, Value::String(s))
        }
        Instruction::BuildList {
            destination,
            elements,
            location,
            ..
        } => put(
            f,
            *destination,
            Value::List(operands(f, elements, *location)?),
        ),
        Instruction::BuildMap {
            destination,
            entries,
            location,
            ..
        } => {
            let v = entries
                .iter()
                .map(|(k, v)| Ok((k.clone(), f.get(v, *location)?)))
                .collect::<Result<_, EvalError>>()?;
            put(f, *destination, Value::Map(v))
        }
        Instruction::BuildTuple {
            destination,
            elements,
            location,
            ..
        } => put(
            f,
            *destination,
            Value::Tuple(operands(f, elements, *location)?),
        ),
        Instruction::TupleElement {
            destination,
            tuple,
            index,
            location,
            ..
        } => {
            let Value::Tuple(v) = f.get(tuple, *location)? else {
                return Err(EvalError::new(
                    *location,
                    "MIR tuple element requires a tuple",
                ));
            };
            put(
                f,
                *destination,
                v.get(*index as usize)
                    .cloned()
                    .ok_or_else(|| EvalError::new(*location, "MIR tuple element is missing"))?,
            )
        }
        Instruction::BuildStruct {
            destination,
            fields,
            location,
            ..
        } => {
            let mut v = HashMap::new();
            for (k, x) in fields {
                v.insert(*k, f.get(x, *location)?);
            }
            put(f, *destination, Value::Struct(v))
        }
        Instruction::LoadField {
            destination,
            target,
            field,
            location,
            ..
        } => {
            let Value::Struct(v) = f.get(target, *location)? else {
                return Err(EvalError::new(
                    *location,
                    "MIR field access requires struct",
                ));
            };
            put(
                f,
                *destination,
                v.get(field)
                    .cloned()
                    .ok_or_else(|| EvalError::new(*location, "MIR struct field is missing"))?,
            )
        }
        Instruction::Call {
            destination,
            target,
            arguments,
            location,
            ..
        } => {
            let v = target_call(
                p,
                *target,
                operands(f, arguments, *location)?,
                f,
                out,
                *location,
            )?;
            put(f, *destination, v)
        }
        Instruction::Phi {
            destination,
            incoming,
            location,
            ..
        } => {
            let prev =
                prev.ok_or_else(|| EvalError::new(*location, "MIR phi has no predecessor"))?;
            let o = incoming
                .iter()
                .find(|(b, _)| *b == prev)
                .map(|(_, o)| o)
                .ok_or_else(|| EvalError::new(*location, "MIR phi predecessor is missing"))?;
            let v = f.get(o, *location)?;
            put(f, *destination, v)
        }
        Instruction::IterInit {
            destination,
            iterable,
            location,
            ..
        } => {
            let Value::List(v) = f.get(iterable, *location)? else {
                return Err(EvalError::new(*location, "MIR for requires List"));
            };
            put(f, *destination, Value::Iterator { values: v, next: 0 })
        }
        Instruction::IterNext {
            iterator,
            item_destination,
            has_value_destination,
            location,
            ..
        } => {
            let Some(Value::Iterator { values, next }) = f.values.get_mut(iterator) else {
                return Err(EvalError::new(*location, "MIR iterator is missing"));
            };
            let item = values.get(*next).cloned();
            if item.is_some() {
                *next += 1
            }
            f.values
                .insert(*item_destination, item.clone().unwrap_or(Value::Unit));
            f.values
                .insert(*has_value_destination, Value::Boolean(item.is_some()));
        }
    }
    Ok(())
}
fn terminator(
    _program: &VerifiedProgram,
    t: &Terminator,
    f: &mut Frame,
    _output: &mut Vec<String>,
) -> Result<Control, EvalError> {
    match t {
        Terminator::Goto { target, .. } => Ok(Control::Next(*target, None)),
        Terminator::Branch {
            condition,
            then_block,
            else_block,
            location,
        } => {
            let Value::Boolean(v) = f.get(condition, *location)? else {
                return Err(EvalError::new(*location, "MIR if condition requires bool"));
            };
            Ok(Control::Next(
                if v { *then_block } else { *else_block },
                None,
            ))
        }
        Terminator::Match {
            target,
            arms,
            otherwise,
            location,
        } => {
            let v = f.get(target, *location)?;
            for arm in arms {
                if let Some(payload) = payload(arm.pattern, &v) {
                    return Ok(Control::Next(arm.block, arm.binding.map(|x| (x, payload))));
                }
            }
            Ok(Control::Next(*otherwise, None))
        }
        Terminator::Return { value, location } => Ok(Control::Return(match value {
            Some(v) => f.get(v, *location)?,
            None => Value::Unit,
        })),
        Terminator::PropagateErr {
            result,
            destination,
            success,
            location,
            ..
        } => match f.get(result, *location)? {
            Value::Outcome(Ok(v)) => {
                f.values.insert(*destination, *v);
                Ok(Control::Next(*success, None))
            }
            Value::Outcome(Err(v)) => Ok(Control::Return(Value::Outcome(Err(v)))),
            _ => Err(EvalError::new(*location, "typed `?` requires Result")),
        },
        Terminator::Unreachable { location } => {
            Err(EvalError::new(*location, "entered unreachable MIR block"))
        }
    }
}
fn constant(c: &Constant) -> Value {
    match c {
        Constant::Integer(v) => Value::Integer(*v),
        Constant::Float(v) => Value::Float(v.clone()),
        Constant::Boolean(v) => Value::Boolean(*v),
        Constant::String(v) => Value::String(v.clone()),
        Constant::Unit => Value::Unit,
        Constant::None => Value::Optional(None),
        Constant::Variant(v) => Value::Enum(*v, None),
    }
}
fn binary(o: BinaryOperator, a: Value, b: Value, l: SourceLocation) -> Result<Value, EvalError> {
    match (o, a, b) {
        (BinaryOperator::Add, Value::Integer(a), Value::Integer(b)) => a
            .checked_add(b)
            .map(Value::Integer)
            .ok_or_else(|| EvalError::new(l, "integer addition overflow")),
        (BinaryOperator::Multiply, Value::Integer(a), Value::Integer(b)) => a
            .checked_mul(b)
            .map(Value::Integer)
            .ok_or_else(|| EvalError::new(l, "integer multiplication overflow")),
        (BinaryOperator::Equal, a, b) => Ok(Value::Boolean(a == b)),
        (BinaryOperator::Add, ..) => Err(EvalError::new(l, "typed addition requires int")),
        (BinaryOperator::Multiply, ..) => {
            Err(EvalError::new(l, "typed multiplication requires int"))
        }
    }
}
fn payload(p: MatchPattern, v: &Value) -> Option<Value> {
    match (p, v) {
        (MatchPattern::Some, Value::Optional(Some(v)))
        | (MatchPattern::Ok, Value::Outcome(Ok(v)))
        | (MatchPattern::Err, Value::Outcome(Err(v))) => Some((**v).clone()),
        (MatchPattern::None, Value::Optional(None)) => Some(Value::Unit),
        (MatchPattern::Variant(x), Value::Enum(y, v)) if x == *y => {
            Some(v.as_deref().cloned().unwrap_or(Value::Unit))
        }
        _ => None,
    }
}
fn target_call(
    p: &VerifiedProgram,
    t: CallTarget,
    mut a: Vec<Value>,
    f: &Frame,
    out: &mut Vec<String>,
    l: SourceLocation,
) -> Result<Value, EvalError> {
    match t {
        CallTarget::Function(id) => call(p, id, a, out),
        CallTarget::ConsolePrintln => {
            out.push(
                a.first()
                    .ok_or_else(|| EvalError::new(l, "console.println argument is missing"))?
                    .display(),
            );
            Ok(Value::Unit)
        }
        CallTarget::Some => Ok(Value::Optional(Some(Box::new(first(&mut a, l, "Some")?)))),
        CallTarget::Ok => Ok(Value::Outcome(Ok(Box::new(first(&mut a, l, "Ok")?)))),
        CallTarget::Err => Ok(Value::Outcome(Err(Box::new(first(&mut a, l, "Err")?)))),
        CallTarget::Variant(id) => Ok(Value::Enum(id, a.into_iter().next().map(Box::new))),
        CallTarget::Newtype(_) => first(&mut a, l, "newtype"),
        CallTarget::BytesFromHex => hex(&mut a, l),
        CallTarget::StringToInt(id) => match f.locals.get(&id).cloned() {
            Some(Value::String(s)) => s
                .parse()
                .map(Value::Integer)
                .map_err(|_| EvalError::new(l, "string.to_int requires an integer string")),
            Some(_) => Err(EvalError::new(l, "string.to_int requires a string")),
            None => Err(EvalError::new(l, "MIR local is missing")),
        },
    }
}
fn first(a: &mut Vec<Value>, l: SourceLocation, n: &str) -> Result<Value, EvalError> {
    if a.is_empty() {
        Err(EvalError::new(l, format!("{n} argument is missing")))
    } else {
        Ok(a.remove(0))
    }
}
fn hex(a: &mut Vec<Value>, l: SourceLocation) -> Result<Value, EvalError> {
    let Value::String(s) = first(a, l, "bytes.from_hex")? else {
        return Err(EvalError::new(l, "bytes.from_hex requires a string"));
    };
    if s.len() % 2 != 0 {
        return Err(EvalError::new(
            l,
            "bytes.from_hex requires an even-length string",
        ));
    }
    let mut b = Vec::new();
    for i in (0..s.len()).step_by(2) {
        b.push(
            u8::from_str_radix(&s[i..i + 2], 16)
                .map_err(|_| EvalError::new(l, "bytes.from_hex requires hexadecimal text"))?,
        )
    }
    Ok(Value::Bytes(b))
}
#[cfg(test)]
mod tests {
    use super::execute;
    use yan_hir::lower;
    use yan_mir::{lower as mir, verify, VerifiedProgram};
    use yan_syntax::{lex, parse};
    use yan_typeck::check;
    fn fixture(s: &str) -> VerifiedProgram {
        let t = lex(s).expect("fixture lex");
        let p = parse(s, &t).expect("fixture parse");
        let h = lower(p).expect("fixture lower");
        let m = mir(check(&h).expect("fixture check")).expect("fixture MIR lower");
        verify(m).expect("fixture verify")
    }
    #[test]
    fn executes_verified_mir_with_result_propagation() {
        let p=fixture("import yan.platform.console fn unwrap(value: Result<int, unit>) -> Result<int, unit> { let item = value? Ok(item) } fn main() -> unit { console.println(unwrap(Ok(3))) }");
        assert_eq!(execute(&p).expect("run"), ["Ok(3)"])
    }
    #[test]
    fn executes_cfg_branches_loops_and_matches() {
        let p=fixture("import yan.platform.console fn main() -> unit { let mut total = 0 for item in [1, 2] { total = total + item } let name = Some(\"Lin\") console.println(if true { total } else { 0 }) console.println(match name { Some(value) => value None => \"none\" }) }");
        assert_eq!(execute(&p).expect("run"), ["3", "Lin"])
    }
}
