pub(crate) mod conversation;
pub(crate) mod memory;
pub(crate) mod settings;
pub(crate) mod shell;
pub(crate) mod skills;
pub(crate) mod tasks;
mod widgets;

use serde_json::Value;
fn text<'a>(value: &'a Value, field: &str) -> &'a str {
    value.get(field).and_then(Value::as_str).unwrap_or("")
}
fn list<'a>(value: &'a Value, field: &str) -> &'a [Value] {
    value
        .get(field)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}
