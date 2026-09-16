use anyhow::{Result, ensure};
use az_agent_engine::InputRequired;
use serde_json::{Value, json};
use uuid::Uuid;

/// 目标来源仅限本次用户文本或浏览器选择，模型生成的 ID 不能替用户选择设备。
pub(super) fn select(devices: &Value, selected: Option<Uuid>, prompt: &str) -> Result<Value> {
    let devices = devices
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("设备列表无效"))?;
    let explicit_name = prompt
        .split_once("打开")
        .and_then(|(prefix, _)| prefix.rsplit_once('在').map(|(_, name)| name.trim()))
        .filter(|name| !name.is_empty());
    let normalize = |s: &str| {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    let named: Vec<_> = devices
        .iter()
        .filter(|device| {
            let label = device["label"].as_str().unwrap_or("");
            !label.is_empty()
                && (prompt.to_lowercase().contains(&label.to_lowercase())
                    || explicit_name
                        .is_some_and(|name| normalize(label).contains(&normalize(name))))
        })
        .collect();
    ensure!(
        explicit_name.is_none() || !named.is_empty(),
        "指定设备未找到，请在会话设备菜单中选择目标"
    );
    let explicit = match named.as_slice() {
        [device] => Some((*device).clone()),
        [] => selected.and_then(|id| devices.iter().find(|d| d["id"] == id.to_string()).cloned()),
        _ => selected.and_then(|id| {
            named
                .iter()
                .find(|device| device["id"] == id.to_string())
                .map(|device| (*device).clone())
        }),
    };
    if let Some(device) = explicit {
        ensure!(
            device["status"] == "online",
            "选定设备离线，未转发给其他电脑"
        );
        return Ok(device);
    }
    ensure!(
        selected.is_none() || !named.is_empty(),
        "会话选定设备不可用，请重新选择"
    );
    let online: Vec<_> = devices
        .iter()
        .filter(|device| device["status"] == "online")
        .collect();
    ensure!(!online.is_empty(), "没有已授权且在线的设备");
    if online.len() == 1 && named.len() <= 1 {
        return Ok(online[0].clone());
    }
    let options: Vec<_> = online.iter().take(32).map(|device| json!({"value":device["id"],"label":format!("{} · {}",device["label"].as_str().unwrap_or("设备"),device["platform"].as_str().unwrap_or("未知系统"))})).collect();
    Err(InputRequired { retry:true, request:json!({"kind":"device","questions":[{"id":"device","title":"在哪台设备执行？此次选择会用于本会话后续的设备操作。","options":options,"allowText":false}]}) }.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn multiple_devices_require_real_selection_and_never_fail_over() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let devices = json!([{"id":a,"label":"Mac mini","status":"online","platform":"darwin"},{"id":b,"label":"MacBook","status":"online","platform":"darwin"}]);
        assert!(
            select(&devices, None, "打开 QQ")
                .unwrap_err()
                .is::<InputRequired>()
        );
        assert_eq!(
            select(&devices, Some(a), "打开 QQ").unwrap()["id"],
            a.to_string()
        );
        assert_eq!(
            select(&devices, Some(a), "在 MacBook 打开 QQ").unwrap()["id"],
            b.to_string()
        );
        let mut offline = devices.clone();
        offline[0]["status"] = json!("offline");
        assert!(select(&offline, Some(a), "打开 QQ").is_err());
        assert!(select(&offline, None, "在 Mac mini 打开 QQ").is_err());
        assert!(select(&devices, Some(Uuid::new_v4()), "打开 QQ").is_err());
        assert_eq!(
            select(&devices, None, "在macmini打开 QQ").unwrap()["id"],
            a.to_string()
        );
        assert!(select(&devices, None, "在Windows打开 QQ").is_err());
    }
}
