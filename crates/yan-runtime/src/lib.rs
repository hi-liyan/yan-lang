//! Yan 原生后端生成程序使用的受控值和 intrinsic 运行时。
//!
//! 本 crate 不解析 Yan 源码、不调用 Cargo，也不提供用户可配置依赖；后端只生成对本 ABI
//! 的固定调用，以保持二进制与 MIR 解释器的可观察值语义一致。

use std::io::{self, Write};

/// Yan 运行时可物化的值。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Value {
    /// 64 位有符号整数。
    Integer(i64),
    /// 保留规范源文本的浮点值。
    Float(String),
    /// 布尔值。
    Boolean(bool),
    /// UTF-8 字符串。
    String(String),
    /// 原始字节序列。
    Bytes(Vec<u8>),
    /// 不可变列表。
    List(Vec<Value>),
    /// 保持源码顺序的字符串键 map。
    Map(Vec<(String, Value)>),
    /// 固定长度元组。
    Tuple(Vec<Value>),
    /// Result 值。
    Result(Result<Box<Value>, Box<Value>>),
    /// Option 值。
    Option(Option<Box<Value>>),
    /// 数字字段 ID 到值的结构体布局。
    Struct(Vec<(u32, Value)>),
    /// 数字变体 ID 及其可选载荷。
    Enum(u32, Option<Box<Value>>),
    /// unit 值。
    Unit,
}

/// 受控运行时 intrinsic 的稳定失败原因。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuntimeError {
    /// 整数加法超出 i64 范围。
    IntegerAdditionOverflow,
    /// 整数乘法超出 i64 范围。
    IntegerMultiplicationOverflow,
    /// 运算符接收到不支持的值类型。
    InvalidOperand,
    /// 元组值或索引无效。
    InvalidTupleElement,
    /// 结构体字段不存在或目标不是结构体。
    InvalidStructField,
    /// 十六进制输入不是合法 bytes 文本。
    InvalidHex,
    /// 字符串不能转换为 int。
    InvalidIntegerString,
    /// 控制台输出无法写入或刷新。
    ConsoleWriteFailed,
}

/// 按 Yan 的用户可见规则显示值。
pub fn display(value: &Value) -> String {
    match value {
        Value::Integer(v) => v.to_string(),
        Value::Float(v) | Value::String(v) => v.clone(),
        Value::Boolean(v) => v.to_string(),
        Value::Bytes(v) => format!(
            "0x{}",
            v.iter().map(|x| format!("{x:02x}")).collect::<String>()
        ),
        Value::List(v) => joined("[", v, "]"),
        Value::Tuple(v) => joined("(", v, ")"),
        Value::Map(v) => format!(
            "{{{}}}",
            v.iter()
                .map(|(k, v)| format!("{k}: {}", display(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Value::Result(Ok(v)) => format!("Ok({})", display(v)),
        Value::Result(Err(v)) => format!("Err({})", display(v)),
        Value::Option(Some(v)) => format!("Some({})", display(v)),
        Value::Option(None) => "None".to_owned(),
        Value::Struct(_) => "struct".to_owned(),
        Value::Enum(_, _) => "enum".to_owned(),
        Value::Unit => "unit".to_owned(),
    }
}
impl Value {
    /// 按 Yan 的用户可见规则显示本值。
    pub fn display(&self) -> String {
        display(self)
    }
}
fn joined(prefix: &str, values: &[Value], suffix: &str) -> String {
    format!(
        "{prefix}{}{suffix}",
        values.iter().map(display).collect::<Vec<_>>().join(", ")
    )
}
/// 执行带溢出检查的整数加法。
pub fn add(left: Value, right: Value) -> Result<Value, RuntimeError> {
    match (left, right) {
        (Value::Integer(a), Value::Integer(b)) => a
            .checked_add(b)
            .map(Value::Integer)
            .ok_or(RuntimeError::IntegerAdditionOverflow),
        _ => Err(RuntimeError::InvalidOperand),
    }
}
/// 执行带溢出检查的整数乘法。
pub fn multiply(left: Value, right: Value) -> Result<Value, RuntimeError> {
    match (left, right) {
        (Value::Integer(a), Value::Integer(b)) => a
            .checked_mul(b)
            .map(Value::Integer)
            .ok_or(RuntimeError::IntegerMultiplicationOverflow),
        _ => Err(RuntimeError::InvalidOperand),
    }
}
/// 比较两个 Yan 值是否相等。
pub fn equal(left: Value, right: Value) -> Result<Value, RuntimeError> {
    match (&left, &right) {
        (Value::Integer(_), Value::Integer(_))
        | (Value::Boolean(_), Value::Boolean(_))
        | (Value::String(_), Value::String(_)) => Ok(Value::Boolean(left == right)),
        _ => Err(RuntimeError::InvalidOperand),
    }
}
/// 读取元组的固定位置元素。
pub fn tuple_element(value: &Value, index: usize) -> Result<Value, RuntimeError> {
    match value {
        Value::Tuple(values) => values
            .get(index)
            .cloned()
            .ok_or(RuntimeError::InvalidTupleElement),
        _ => Err(RuntimeError::InvalidTupleElement),
    }
}
/// 读取数字字段 ID 对应的结构体字段。
pub fn field(value: &Value, id: u32) -> Result<Value, RuntimeError> {
    match value {
        Value::Struct(fields) => fields
            .iter()
            .find(|(field, _)| *field == id)
            .map(|(_, value)| value.clone())
            .ok_or(RuntimeError::InvalidStructField),
        _ => Err(RuntimeError::InvalidStructField),
    }
}
/// 不可变 List 的受控迭代状态。
#[derive(Clone, Debug)]
pub struct ListIterator {
    values: Vec<Value>,
    next: usize,
}
/// 从 List 创建受控迭代器。
pub fn list_iterator(value: &Value) -> Result<ListIterator, RuntimeError> {
    match value {
        Value::List(values) => Ok(ListIterator {
            values: values.clone(),
            next: 0,
        }),
        _ => Err(RuntimeError::InvalidOperand),
    }
}
/// 返回下一项；遍历结束返回 None。
pub fn iterator_next(iterator: &mut ListIterator) -> Option<Value> {
    let value = iterator.values.get(iterator.next).cloned();
    if value.is_some() {
        iterator.next += 1
    }
    value
}
/// 将偶数长度十六进制字符串转换为 bytes。
pub fn bytes_from_hex(text: &str) -> Result<Value, RuntimeError> {
    if text.len() % 2 != 0 {
        return Err(RuntimeError::InvalidHex);
    }
    let mut bytes = Vec::with_capacity(text.len() / 2);
    for index in (0..text.len()).step_by(2) {
        let pair = text.get(index..index + 2).ok_or(RuntimeError::InvalidHex)?;
        bytes.push(u8::from_str_radix(pair, 16).map_err(|_| RuntimeError::InvalidHex)?)
    }
    Ok(Value::Bytes(bytes))
}
/// 将字符串转换为 Yan Result<int, unit>。
pub fn string_to_int(text: &str) -> Value {
    match text.parse::<i64>() {
        Ok(value) => Value::Result(Ok(Box::new(Value::Integer(value)))),
        Err(_) => Value::Result(Err(Box::new(Value::Unit))),
    }
}
/// 将值写入标准输出并追加换行。
///
/// 写入或刷新失败返回稳定运行时错误，避免后端二进制因输出设备关闭而 panic。
pub fn console_println(value: &Value) -> Result<(), RuntimeError> {
    let stdout = io::stdout();
    let mut output = stdout.lock();
    console_println_to(&mut output, value)
}

fn console_println_to(output: &mut dyn Write, value: &Value) -> Result<(), RuntimeError> {
    output
        .write_all(format!("{}\n", display(value)).as_bytes())
        .map_err(|_| RuntimeError::ConsoleWriteFailed)?;
    output.flush().map_err(|_| RuntimeError::ConsoleWriteFailed)
}

#[cfg(test)]
mod tests {
    use std::io::{self, Write};

    use super::{
        add, bytes_from_hex, console_println_to, equal, field, iterator_next, list_iterator,
        multiply, string_to_int, tuple_element, RuntimeError, Value,
    };

    struct FailingWriter;

    impl Write for FailingWriter {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("failed"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn reports_console_output_failure_without_panicking() {
        assert_eq!(
            console_println_to(&mut FailingWriter, &Value::Integer(1)),
            Err(RuntimeError::ConsoleWriteFailed)
        );
    }

    #[test]
    fn displays_composite_yan_values_with_interpreter_formatting() {
        assert_eq!(
            Value::List(vec![Value::Integer(1), Value::Option(None)]).display(),
            "[1, None]"
        );
        assert_eq!(Value::Bytes(vec![0xa1, 0x3f]).display(), "0xa13f");
    }

    #[test]
    fn reports_integer_addition_overflow_without_panicking() {
        assert_eq!(
            add(Value::Integer(i64::MAX), Value::Integer(1)),
            Err(super::RuntimeError::IntegerAdditionOverflow)
        );
    }

    #[test]
    fn covers_result_operations_access_and_intrinsic_errors() {
        assert_eq!(
            Value::Result(Ok(Box::new(Value::Integer(2)))).display(),
            "Ok(2)"
        );
        assert_eq!(
            multiply(Value::Integer(i64::MAX), Value::Integer(2)),
            Err(RuntimeError::IntegerMultiplicationOverflow)
        );
        assert_eq!(
            equal(Value::String("a".into()), Value::String("a".into())),
            Ok(Value::Boolean(true))
        );
        assert_eq!(
            tuple_element(&Value::Tuple(vec![]), 0),
            Err(RuntimeError::InvalidTupleElement)
        );
        assert_eq!(
            field(&Value::Struct(vec![]), 1),
            Err(RuntimeError::InvalidStructField)
        );
        let mut iterator =
            list_iterator(&Value::List(vec![Value::Integer(1)])).expect("list iterator");
        assert_eq!(iterator_next(&mut iterator), Some(Value::Integer(1)));
        assert_eq!(iterator_next(&mut iterator), None);
        assert_eq!(bytes_from_hex("g0"), Err(RuntimeError::InvalidHex));
        assert_eq!(
            string_to_int("bad"),
            Value::Result(Err(Box::new(Value::Unit)))
        );
        assert!(matches!(Value::Option(None), Value::Option(None)));
        assert!(matches!(Value::Enum(1, None), Value::Enum(1, None)));
    }
}
