use super::{device_tools, model::Scope, service_impl::Core};
use crate::runtime::Tool;
use anyhow::{Context, Result};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

pub(super) struct Command {
    application: String,
    list: Arc<dyn Tool>,
    open: Arc<dyn Tool>,
}

pub(super) fn prepare(
    core: &Core,
    scope: &Scope,
    assistant: Uuid,
    content: &str,
) -> Option<Command> {
    let application = application(content)?;
    let mut tools = device_tools::tools(core, scope, assistant).into_iter();
    Some(Command {
        application,
        list: tools.next()?,
        open: tools.next()?,
    })
}

fn application(content: &str) -> Option<String> {
    // 只接受当前用户消息的完整打开指令，检索资料和模型文本不能触发此路径。
    let application = content.trim().strip_prefix("打开")?.trim();
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
        let devices = self.list.invoke(json!({})).await?;
        let online: Vec<_> = devices
            .as_array()
            .context("设备列表无效")?
            .iter()
            .filter(|device| device["status"] == "online")
            .collect();
        if online.is_empty() {
            return Ok(
                "没有已授权且在线的设备。请在“我的设备”启用应用控制，并保持客户端在线。".into(),
            );
        }
        if online.len() != 1 {
            return Ok("有多台设备在线，请指定要在哪台设备打开应用。".into());
        }
        let device = online[0];
        let result = self
            .open
            .invoke(json!({"worker_id":device["id"],"application":self.application}))
            .await?;
        let label = device["label"].as_str().unwrap_or("已配对设备");
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
    use serde_json::Value;
    struct Fixed(Value);
    #[async_trait::async_trait]
    impl Tool for Fixed {
        fn definition(&self) -> Value {
            json!({})
        }
        async fn invoke(&self, _: Value) -> Result<Value> {
            Ok(self.0.clone())
        }
    }
    struct Never;
    #[async_trait::async_trait]
    impl Tool for Never {
        fn definition(&self) -> Value {
            json!({})
        }
        async fn invoke(&self, _: Value) -> Result<Value> {
            panic!("设备不明确时禁止执行")
        }
    }
    #[test]
    fn only_explicit_current_commands_are_selected() {
        assert_eq!(application("打开 Postman"), Some("Postman".into()));
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
    #[tokio::test]
    async fn never_guesses_between_devices_or_reports_pending_as_success() -> Result<()> {
        for devices in [json!([]), json!([{"status":"online"},{"status":"online"}])] {
            let command = Command {
                application: "Postman".into(),
                list: Arc::new(Fixed(devices)),
                open: Arc::new(Never),
            };
            assert!(!command.execute().await?.contains("客户端已确认"));
        }
        for (state, confirmed) in [("queued", false), ("failed", false), ("complete", true)] {
            let command = Command {
                application: "Postman".into(),
                list: Arc::new(Fixed(
                    json!([{"id":Uuid::nil(),"status":"online","label":"Mac"}]),
                )),
                open: Arc::new(Fixed(json!({"state":state,"result":{"pid":42}}))),
            };
            assert_eq!(command.execute().await?.contains("客户端已确认"), confirmed);
        }
        Ok(())
    }
}
