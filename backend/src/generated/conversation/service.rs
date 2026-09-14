use super::model::*;
use async_trait::async_trait;
use uuid::Uuid;

#[async_trait]
pub trait AgentService: Send + Sync {
    async fn settings(&self, scope: &Scope) -> ServiceResult<Settings>;
    async fn models(&self, scope: &Scope, request: ModelListRequest) -> ServiceResult<Vec<String>>;
    async fn select_model(
        &self,
        scope: &Scope,
        id: Uuid,
        selection: ModelSelection,
    ) -> ServiceResult<Conversation>;
    async fn save_provider(
        &self,
        scope: &Scope,
        id: Option<Uuid>,
        draft: ProviderDraft,
    ) -> ServiceResult<Provider>;
    async fn delete_provider(&self, scope: &Scope, id: Uuid) -> ServiceResult<()>;
    async fn conversations(&self, scope: &Scope) -> ServiceResult<Vec<Conversation>>;
    async fn create(&self, scope: &Scope, draft: ConversationDraft) -> ServiceResult<Conversation>;
    async fn thread(&self, scope: &Scope, id: Uuid) -> ServiceResult<Thread>;
    async fn delete(&self, scope: &Scope, id: Uuid) -> ServiceResult<()>;
    async fn send(&self, scope: &Scope, id: Uuid, prompt: Prompt) -> ServiceResult<Thread>;
    async fn cancel(&self, scope: &Scope, id: Uuid) -> ServiceResult<Thread>;
    async fn shutdown(&self);
    async fn memory_request(
        &self,
        scope: &Scope,
        method: &str,
        path: &str,
        body: serde_json::Value,
    ) -> ServiceResult<serde_json::Value>;
}
