use super::model::*;
use crate::runtime::Tool;
use anyhow::{Result, ensure};
use az_agent_engine::InputRequired;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashSet;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Questions {
    questions: Vec<InputQuestion>,
}

pub(super) struct AskUser;
#[async_trait::async_trait]
impl Tool for AskUser {
    fn definition(&self) -> Value {
        json!({"type":"function","function":{"name":"request_user_input","description":"缺少执行必需的信息时，向当前会话用户提出 1 至 3 个问题并暂停。答案返回后继续原任务。不用于询问密码或密钥；设备选择由设备工具自动处理。","parameters":{"type":"object","properties":{"questions":{"type":"array","minItems":1,"maxItems":3,"items":{"type":"object","properties":{"id":{"type":"string"},"title":{"type":"string"},"options":{"type":"array","items":{"type":"object","properties":{"value":{"type":"string"},"label":{"type":"string"}},"required":["value","label"],"additionalProperties":false}},"allowText":{"type":"boolean"}},"required":["id","title","options","allowText"],"additionalProperties":false}}},"required":["questions"],"additionalProperties":false}}})
    }
    async fn invoke(&self, arguments: Value) -> Result<Value> {
        let request: Questions = serde_json::from_value(arguments)?;
        validate(&request.questions)?;
        Err(InputRequired {
            request: json!({"kind":"questions","questions":request.questions}),
            retry: false,
        }
        .into())
    }
}

pub(super) fn validate(questions: &[InputQuestion]) -> Result<()> {
    ensure!((1..=3).contains(&questions.len()), "一次需要 1 至 3 个问题");
    let mut ids = HashSet::new();
    for question in questions {
        ensure!(
            !question.id.is_empty() && question.id.len() <= 64 && ids.insert(&question.id),
            "问题 ID 无效或重复"
        );
        ensure!(
            !question.title.trim().is_empty() && question.title.len() <= 1200,
            "问题标题无效"
        );
        ensure!(
            question.options.len() <= 32 && (question.allow_text || !question.options.is_empty()),
            "问题选项无效"
        );
        let mut values = HashSet::new();
        for option in &question.options {
            ensure!(
                !option.value.is_empty()
                    && option.value.len() <= 128
                    && values.insert(&option.value),
                "选项值无效或重复"
            );
            ensure!(
                !option.label.trim().is_empty() && option.label.len() <= 512,
                "选项标题无效"
            );
        }
    }
    Ok(())
}

pub(super) fn validate_answers(questions: &[InputQuestion], answer: &InputAnswer) -> Result<()> {
    ensure!(questions.len() == answer.answers.len(), "请回答全部问题");
    for question in questions {
        let value = answer
            .answers
            .get(&question.id)
            .ok_or_else(|| anyhow::anyhow!("问题答案缺失"))?;
        ensure!(
            !value.trim().is_empty() && value.len() <= 4000,
            "答案为空或过长"
        );
        ensure!(
            question.allow_text || question.options.iter().any(|option| &option.value == value),
            "请选择列表中的选项"
        );
    }
    Ok(())
}
