use super::model::*;
use async_trait::async_trait;
use uuid::Uuid;

#[async_trait]
pub trait AgentService: super::super::skills::service::SkillService + Send + Sync {
    async fn save_search(
        &self,
        scope: &Scope,
        draft: SearchSettingsDraft,
    ) -> ServiceResult<SearchSettings>;
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
    async fn answer_input(
        &self,
        scope: &Scope,
        id: Uuid,
        answer: InputAnswer,
    ) -> ServiceResult<Thread>;
    async fn devices(&self, scope: &Scope) -> ServiceResult<serde_json::Value>;
    async fn select_device(
        &self,
        scope: &Scope,
        id: Uuid,
        selection: DeviceSelection,
    ) -> ServiceResult<Conversation>;
    async fn swarm_tasks(&self, scope: &Scope, id: Uuid) -> ServiceResult<Vec<SwarmTask>>;
    async fn cancel_swarm_task(&self, scope: &Scope, id: Uuid, task: Uuid) -> ServiceResult<()>;
    async fn shutdown(&self);
    async fn memory_request(
        &self,
        scope: &Scope,
        method: &str,
        path: &str,
        body: serde_json::Value,
    ) -> ServiceResult<serde_json::Value>;
}
