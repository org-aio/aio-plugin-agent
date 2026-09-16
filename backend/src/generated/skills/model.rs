use serde::Deserialize;
use serde_json::Value;

/// 所有者由宿主注入，任何操作正文都不能传入租户或账号。
#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "camelCase", deny_unknown_fields)]
pub enum Request {
    List,
    Read {
        path: String,
    },
    Write {
        path: String,
        expected: Option<String>,
        content: Option<String>,
        executable: bool,
    },
    Status {
        report: Value,
    },
    Resolve {
        device: String,
        path: String,
        side: String,
        local: Option<String>,
        remote: Option<String>,
    },
}
