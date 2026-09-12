use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Provider {
    pub id: Uuid,
    pub label: String,
    pub endpoint: String,
    pub model: String,
    pub has_secret: bool,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderDraft {
    pub label: String,
    pub endpoint: String,
    pub model: String,
    pub secret: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub id: Uuid,
    pub title: String,
    pub provider_id: Option<Uuid>,
    pub space_id: Option<String>,
    pub updated_at: String,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: Uuid,
    pub role: String,
    pub content: String,
    pub status: String,
    pub error: Option<String>,
    pub tokens: Option<i64>,
    pub source_id: Option<String>,
    pub memory_status: Option<String>,
    pub citations: Vec<MemoryCitation>,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemoryCitation {
    pub id: String,
    pub title: String,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    pub conversation: Conversation,
    pub messages: Vec<Message>,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConversationDraft {
    pub provider_id: Option<Uuid>,
    pub title: String,
    pub space_id: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Prompt {
    pub request_id: Uuid,
    pub content: String,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub allowed_endpoints: Vec<String>,
    pub providers: Vec<Provider>,
    pub max_prompt_chars: usize,
    pub memory_available: bool,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub struct Failure {
    pub error: String,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemorySpace {
    pub id: String,
    pub title: String,
    pub personal: bool,
    pub role: String,
    pub model_binding: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SecretReference {
    pub id: String,
    pub label: String,
    pub source_id: String,
    pub can_reveal: bool,
    pub can_manage: bool,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemorySource {
    pub id: String,
    pub space_id: String,
    pub text: String,
    pub status: String,
    pub secrets: Vec<SecretReference>,
    pub created_by: String,
    pub updated_at: i64,
    pub error: Option<String>,
}
