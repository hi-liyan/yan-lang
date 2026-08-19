//! MIR 的受限解释执行器。

use std::collections::HashMap;

use yan_hir::{DefId, FieldId, LocalId, VariantId};
use yan_mir::Program;
use yan_source::Span;
use yan_typeck::{TypedCallTarget, TypedExpression, TypedExpressionKind, TypedStatement};

/// 执行 MIR 程序并返回控制台输出。
pub fn execute(program: &Program) -> Result<Vec<String>, EvalError> {
    let main = program
        .typed_program()
        .functions
        .iter()
        .find(|function| function.name == "main")
        .ok_or_else(|| EvalError::new(Span::default(), "undefined function `main`"))?;
    let mut output = Vec::new();
    match call(program, main.id, Vec::new(), &mut output)? {
        Flow::Value(Value::Unit) | Flow::Value(Value::Outcome(Ok(_))) | Flow::Return(_) => Ok(output),
        Flow::Value(Value::Outcome(Err(value))) => Err(EvalError::new(main.span, format!("main returned Err({})", value.display()))),
        Flow::Value(value) => Err(EvalError::new(main.span, format!("main returned an unsupported value `{}`", value.display()))),
    }
}

/// 执行期间的稳定错误。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvalError { pub span: Span, pub message: String }
impl EvalError { pub fn new(span: Span, message: impl Into<String>) -> Self { Self { span, message: message.into() } } }

#[derive(Clone, Debug, Eq, PartialEq)]
enum Value { Integer(i64), Boolean(bool), String(String), List(Vec<Value>), Tuple(Vec<Value>), Outcome(Result<Box<Value>,Box<Value>>), Optional(Option<Box<Value>>), Struct(HashMap<FieldId,Value>), Enum(VariantId,Option<Box<Value>>), Unit }
impl Value { fn display(&self)->String { match self { Self::Integer(v)=>v.to_string(),Self::Boolean(v)=>v.to_string(),Self::String(v)=>v.clone(),Self::Unit=>"unit".into(),Self::Outcome(Ok(v))=>format!("Ok({})",v.display()),Self::Outcome(Err(v))=>format!("Err({})",v.display()),Self::Optional(Some(v))=>format!("Some({})",v.display()),Self::Optional(None)=>"None".into(),Self::List(v)=>format!("[{}]",v.iter().map(Value::display).collect::<Vec<_>>().join(", ")),Self::Tuple(v)=>format!("({})",v.iter().map(Value::display).collect::<Vec<_>>().join(", ")),Self::Struct(_)=>"struct".into(),Self::Enum(_,_)=>"enum".into()} } }
enum Flow { Value(Value), Return(Value) }

fn call(program:&Program,id:DefId,args:Vec<Value>,out:&mut Vec<String>)->Result<Flow,EvalError>{let f=program.typed_program().functions.iter().find(|f|f.id==id).ok_or_else(||EvalError::new(Span::default(),"MIR function ID is missing"))?;let mut slots=HashMap::new();for(p,v)in f.parameters.iter().zip(args){slots.insert(p.id,v);} block(program,&f.statements,&mut slots,out)}
fn block(program:&Program,ss:&[TypedStatement],slots:&mut HashMap<LocalId,Value>,out:&mut Vec<String>)->Result<Flow,EvalError>{for(i,s)in ss.iter().enumerate(){let tail=i+1==ss.len();match s{TypedStatement::Let{local,value}=>match eval(program,value,slots,out)?{Flow::Value(v)=>{slots.insert(local.id,v);}f=>return Ok(f)},TypedStatement::Assign{local,value,..}=>match eval(program,value,slots,out)?{Flow::Value(v)=>{slots.insert(*local,v);}f=>return Ok(f)},TypedStatement::Expression(e)=>{let f=eval(program,e,slots,out)?;if tail||matches!(f,Flow::Return(_)){return Ok(f)}},TypedStatement::Destructure{locals,value}=>match eval(program,value,slots,out)?{Flow::Value(Value::Tuple(v))=>for(l,v)in locals.iter().zip(v){slots.insert(l.id,v);},Flow::Value(_)=>return Err(EvalError::new(value.span,"typed destructuring requires a tuple")),f=>return Ok(f)}}}Ok(Flow::Value(Value::Unit))}
fn value(program:&Program,e:&TypedExpression,s:&mut HashMap<LocalId,Value>,o:&mut Vec<String>)->Result<Value,EvalError>{match eval(program,e,s,o)?{Flow::Value(v)=>Ok(v),Flow::Return(_)=>Err(EvalError::new(e.span,"unexpected return"))}}
fn eval(program:&Program,e:&TypedExpression,s:&mut HashMap<LocalId,Value>,o:&mut Vec<String>)->Result<Flow,EvalError>{let v=match &e.kind{TypedExpressionKind::Integer(v)=>Value::Integer(*v),TypedExpressionKind::Boolean(v)=>Value::Boolean(*v),TypedExpressionKind::Local(id)=>s.get(id).cloned().ok_or_else(||EvalError::new(e.span,"MIR local is missing"))?,TypedExpressionKind::None=>Value::Optional(None),TypedExpressionKind::List(v)=>Value::List(v.iter().map(|x|value(program,x,s,o)).collect::<Result<_,_>>()?),TypedExpressionKind::Tuple(v)=>Value::Tuple(v.iter().map(|x|value(program,x,s,o)).collect::<Result<_,_>>()?),TypedExpressionKind::Add(a,b)=>match(value(program,a,s,o)?,value(program,b,s,o)?){(Value::Integer(a),Value::Integer(b))=>Value::Integer(a.checked_add(b).ok_or_else(||EvalError::new(e.span,"integer addition overflow"))?),_=>return Err(EvalError::new(e.span,"typed addition requires int"))},TypedExpressionKind::Multiply(a,b)=>match(value(program,a,s,o)?,value(program,b,s,o)?){(Value::Integer(a),Value::Integer(b))=>Value::Integer(a.checked_mul(b).ok_or_else(||EvalError::new(e.span,"integer multiplication overflow"))?),_=>return Err(EvalError::new(e.span,"typed multiplication requires int"))},TypedExpressionKind::Equal(a,b)=>Value::Boolean(value(program,a,s,o)?==value(program,b,s,o)?),TypedExpressionKind::Return(v)=>return Ok(Flow::Return(value(program,v,s,o)?)),TypedExpressionKind::Try(v)=>match value(program,v,s,o)?{Value::Outcome(Ok(v))=>*v,Value::Outcome(Err(v))=>return Ok(Flow::Return(Value::Outcome(Err(v)))),_=>return Err(EvalError::new(e.span,"typed `?` requires Result"))},TypedExpressionKind::Call{target,arguments}=>return call_target(program,target,arguments,s,o,e.span),TypedExpressionKind::String(parts)=>{let mut t=String::new();for p in parts{match p{yan_typeck::TypedStringPart::Text(x)=>t.push_str(x),yan_typeck::TypedStringPart::Local(id)=>t.push_str(&s.get(id).ok_or_else(||EvalError::new(e.span,"MIR local is missing"))?.display())}}Value::String(t)},_=>return Err(EvalError::new(e.span,"MIR expression lowering is not implemented"))};Ok(Flow::Value(v))}
fn call_target(p:&Program,t:&TypedCallTarget,a:&[TypedExpression],s:&mut HashMap<LocalId,Value>,o:&mut Vec<String>,span:Span)->Result<Flow,EvalError>{let v=a.iter().map(|e|value(p,e,s,o)).collect::<Result<Vec<_>,_>>()?;match t{TypedCallTarget::Function(id)=>call(p,*id,v,o),TypedCallTarget::ConsolePrintln=>{o.push(v.first().ok_or_else(||EvalError::new(span,"console.println argument is missing"))?.display());Ok(Flow::Value(Value::Unit))},TypedCallTarget::Some=>Ok(Flow::Value(Value::Optional(Some(Box::new(v.into_iter().next().ok_or_else(||EvalError::new(span,"Some argument is missing"))?))))),TypedCallTarget::Ok=>Ok(Flow::Value(Value::Outcome(Ok(Box::new(v.into_iter().next().ok_or_else(||EvalError::new(span,"Ok argument is missing"))?))))),TypedCallTarget::Err=>Ok(Flow::Value(Value::Outcome(Err(Box::new(v.into_iter().next().ok_or_else(||EvalError::new(span,"Err argument is missing"))?))))),TypedCallTarget::Variant(id)=>Ok(Flow::Value(Value::Enum(*id,v.into_iter().next().map(Box::new)))),_=>Err(EvalError::new(span,"MIR call lowering is not implemented"))}}
