use az_agent_model::*;
use schemars::{JsonSchema, schema_for};
use serde::Serialize;

#[derive(JsonSchema, Serialize)]
struct Contract {
    provider: Provider,
    provider_draft: ProviderDraft,
    model_list_request: ModelListRequest,
    model_selection: ModelSelection,
    conversation: Conversation,
    conversation_draft: ConversationDraft,
    message: Message,
    thread: Thread,
    prompt: Prompt,
    settings: Settings,
    failure: Failure,
    memory_space: MemorySpace,
    memory_source: MemorySource,
    memory_graph: MemoryGraph,
}
fn main() {
    println!(
        "{}",
        serde_json::to_string_pretty(&schema_for!(Contract)).expect("生成模型契约")
    );
}
