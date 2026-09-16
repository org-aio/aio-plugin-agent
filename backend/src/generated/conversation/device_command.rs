use super::{device_tools, model::Scope, service_impl::Core};
use crate::runtime::Tool;
use anyhow::Result;
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

pub(super) struct Command {
    application: String,
    open: Arc<dyn Tool>,
}

pub(super) fn prepare(
    core: &Core,
    scope: &Scope,
    assistant: Uuid,
    selected: Option<Uuid>,
    content: &str,
) -> Option<Command> {
    let application = application(content)?;
    let mut tools = device_tools::tools(core, scope, assistant, selected, content).into_iter();
    Some(Command {
        application,
        open: tools.nth(1)?,
    })
}

fn application(content: &str) -> Option<String> {
    // 只接受当前用户消息的完整打开指令，检索资料和模型文本不能触发此路径。
    let content = content.trim();
    let content = content
        .strip_prefix("请帮我")
        .or_else(|| content.strip_prefix("帮我"))
        .or_else(|| content.strip_prefix("请"))
        .unwrap_or(content)
        .trim();
    let (prefix, application) = content.split_once("打开")?;
    if !prefix.trim().is_empty() && !prefix.trim().starts_with('在') {
        return None;
    }
    let application = application.trim();
    (!application.is_empty()
        && application.len() <= 128
        && !application.starts_with('-')
        && application
            .chars()
            .all(|ch| ch.is_alphanumeric() || " ._+-".contains(ch)))
    .then(|| application.to_owned())
}

impl Command {
    pub(super) async fn execute(self) -> Result<String> {
        let result = self
            .open
            .invoke(json!({"application":self.application}))
            .await?;
        let label = result["device"]["label"].as_str().unwrap_or("会话选定设备");
        Ok(match result["state"].as_str() {
            Some("complete") => format!(
                "已在 {label} 打开 {}，客户端已确认进程 PID {}。",
                self.application, result["result"]["pid"]
            ),
            Some("failed" | "cancelled" | "interrupted") => format!(
                "打开 {} 未完成：{}",
                self.application,
                result["error"].as_str().unwrap_or("设备执行失败")
            ),
            _ => format!(
                "已向 {label} 提交打开 {} 的任务，尚未收到执行确认。可在“我的设备”查看任务结果。",
                self.application
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_explicit_current_commands_are_selected() {
        assert_eq!(application("打开 Postman"), Some("Postman".into()));
        assert_eq!(application("帮我打开 QQ"), Some("QQ".into()));
        assert_eq!(application("在 Mac mini 打开 QQ"), Some("QQ".into()));
        for text in [
            "如何打开 Postman?",
            "文档说：打开 Postman",
            "打开 $(id)",
            "打开 /tmp/test",
            "打开",
        ] {
            assert!(application(text).is_none());
        }
    }
}
