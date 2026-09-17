use anyhow::Result;
use serde::Deserialize;
use serde_json::Value;

pub enum Delta {
    Text(String),
    Tokens(i64),
    Waiting {
        state: RunState,
        input: InputRequired,
    },
}

/// 可持久化执行位置；不包含模型地址、密钥或工具实例。
#[derive(Clone, serde::Serialize, Deserialize)]
pub struct RunState {
    pub messages: Vec<Value>,
    pub pending: Vec<ToolCall>,
    pub rounds_left: u8,
    pub tokens: i64,
    /// 尚未提交给模型的工具图片，随用户输入检查点一起保存。
    #[serde(default)]
    pub observations: Vec<Value>,
    /// 本次执行的图片请求被拒绝后，只用界面文字继续，恢复时不重复尝试图片。
    #[serde(default)]
    pub text_observations_only: bool,
}

impl RunState {
    pub fn new(messages: Vec<Value>) -> Self {
        Self {
            messages,
            pending: Vec::new(),
            rounds_left: 8,
            tokens: 0,
            observations: Vec::new(),
            text_observations_only: false,
        }
    }
    /// 把用户答案作为原工具调用的结果注入，已完成工具不重新执行。
    pub fn answer(&mut self, answer: Value) -> Result<()> {
        anyhow::ensure!(!self.pending.is_empty(), "没有等待回答的工具调用");
        let call = self.pending.remove(0);
        self.messages.push(serde_json::json!({"role":"tool","tool_call_id":call.id,"content":serde_json::to_string(&answer)?}));
        Ok(())
    }
}

/// 宿主工具请求中断。retry 表示答案改变执行条件，恢复时重新校验当前工具。
#[derive(Clone, Debug, serde::Serialize, Deserialize)]
pub struct InputRequired {
    pub request: Value,
    pub retry: bool,
}
impl std::fmt::Display for InputRequired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("等待用户回答")
    }
}
impl std::error::Error for InputRequired {}

/// 工具名称属于模型线协议；实例由调用方注入，不用名称注册应用插件。
#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    fn definition(&self) -> Value;
    async fn invoke(&self, arguments: Value) -> Result<Value>;
    /// 由可信工具适配器提取图片并移除文本回执中的编码数据；普通工具默认仅返回文本。
    fn take_images(&self, _result: &mut Value) -> Vec<String> {
        Vec::new()
    }
}

#[derive(Default)]
pub(crate) struct Completion {
    pub text: String,
    pub calls: std::collections::BTreeMap<usize, ToolCall>,
    pub tokens: i64,
    pub finished: bool,
}

#[derive(Clone, Default, serde::Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub function: Function,
}

#[derive(Clone, Default, serde::Serialize, Deserialize)]
pub struct Function {
    pub name: String,
    pub arguments: String,
}

#[derive(Deserialize)]
pub(crate) struct Chunk {
    #[serde(default)]
    pub choices: Vec<Choice>,
    pub usage: Option<Usage>,
    pub error: Option<Value>,
}

#[derive(Deserialize)]
pub(crate) struct Usage {
    pub total_tokens: i64,
}

#[derive(Deserialize)]
pub(crate) struct Choice {
    pub delta: Change,
    pub finish_reason: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct Change {
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<CallChange>,
}

#[derive(Deserialize)]
pub(crate) struct CallChange {
    pub index: usize,
    pub id: Option<String>,
    pub function: Option<FunctionChange>,
}

#[derive(Deserialize)]
pub(crate) struct FunctionChange {
    pub name: Option<String>,
    pub arguments: Option<String>,
}
