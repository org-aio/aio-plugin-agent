use crate::transport;
use az_agent_model::*;
use dioxus::prelude::*;
use serde_json::{Value, json};
use uuid::Uuid;
#[derive(Clone, PartialEq)]
pub enum Dialog {
    Settings,
    Provider(Option<Provider>),
    New,
    Delete {
        title: String,
        path: String,
    },
    Source(MemorySource),
    Entry {
        id: String,
        title: String,
        content: String,
    },
    Spaces,
}
#[derive(Default, Clone)]
pub struct AgentState {
    pub settings: Option<Settings>,
    pub settings_page: bool,
    pub spaces: Vec<MemorySpace>,
    pub conversations: Vec<Conversation>,
    pub thread: Option<Thread>,
    pub draft: String,
    pub search: String,
    pub busy: bool,
    pub error: Option<String>,
    pub dialog: Option<Dialog>,
    pub show_history: bool,
    pub show_graph: bool,
    pub focused: Option<Uuid>,
    pub generation: u64,
    pub pending: Option<(Uuid, Prompt)>,
}
impl AgentState {
    pub fn settings_dialog(&self) -> Option<Dialog> {
        if self.settings_page {
            None
        } else {
            Some(Dialog::Settings)
        }
    }
    pub fn running(&self) -> bool {
        self.thread
            .as_ref()
            .is_some_and(|t| t.messages.iter().any(|m| m.status == "generating"))
    }
    pub fn processing(&self) -> bool {
        self.thread.as_ref().is_some_and(|t| {
            t.messages.iter().any(|m| {
                matches!(m.status.as_str(), "queued" | "generating")
                    || (matches!(m.memory_status.as_deref(), Some("pending" | "processing"))
                        && self.spaces.iter().any(|space| {
                            Some(&space.id) == t.conversation.space_id.as_ref()
                                && space.model_binding.is_some()
                        }))
            })
        })
    }
}
pub fn run(
    mut state: Signal<AgentState>,
    action: impl std::future::Future<Output = Result<(), String>> + 'static,
) {
    if state.peek().busy {
        return;
    }
    state.write().generation += 1;
    state.write().busy = true;
    state.write().error = None;
    spawn(async move {
        let result = action.await;
        state.write().busy = false;
        if let Err(error) = result {
            state.write().error = Some(error);
        }
    });
}
pub async fn load(mut state: Signal<AgentState>, conversation: bool) -> Result<(), String> {
    let settings: Settings = transport::request("GET", "/settings", Value::Null).await?;
    let spaces = if settings.memory_available {
        transport::memory("GET", "/spaces", Value::Null).await?
    } else {
        Vec::<MemorySpace>::new()
    };
    state.write().settings = Some(settings.clone());
    state.write().spaces = spaces.clone();
    if !conversation {
        return Ok(());
    }
    let conversations: Vec<Conversation> =
        transport::request("GET", "/conversations", Value::Null).await?;
    state.write().conversations = conversations.clone();
    if state.peek().thread.is_none() {
        let current = if let Some(current) = conversations.first() {
            current.clone()
        } else {
            transport::request(
                "POST",
                "/conversations",
                json!(
                    { "title" : "新对话", "providerId" : settings.providers
                    .first().map(| p | p.id), "spaceId" : spaces.iter().find(| s | s
                    .personal).map(| s |& s.id) }
                ),
            )
            .await?
        };
        select(state, current.id).await?;
        state.write().conversations =
            transport::request("GET", "/conversations", Value::Null).await?;
    }
    Ok(())
}
pub async fn select(mut state: Signal<AgentState>, id: Uuid) -> Result<(), String> {
    let thread = transport::get_thread(id).await?;
    let mut state = state.write();
    state.generation += 1;
    state.thread = Some(thread);
    state.draft.clear();
    state.pending = None;
    state.focused = None;
    state.show_history = false;
    Ok(())
}
pub async fn send(mut state: Signal<AgentState>) -> Result<(), String> {
    let Some(id) = state.peek().thread.as_ref().map(|t| t.conversation.id) else {
        return Ok(());
    };
    let content = state.peek().draft.clone();
    if content.trim().is_empty() {
        return Ok(());
    }
    let prompt = state
        .peek()
        .pending
        .as_ref()
        .filter(|(prior, p)| *prior == id && p.content == content)
        .map(|(_, p)| p.clone())
        .unwrap_or(Prompt {
            request_id: Uuid::new_v4(),
            content,
        });
    state.write().pending = Some((id, prompt.clone()));
    let receipt: Thread = transport::request(
        "POST",
        &format!("/conversations/{id}/messages"),
        json!(prompt),
    )
    .await?;
    let mut previous = state
        .peek()
        .thread
        .clone()
        .map(|t| t.messages)
        .unwrap_or_default();
    previous.retain(|old| !receipt.messages.iter().any(|m| m.id == old.id));
    previous.extend(receipt.messages);
    state.write().thread = Some(Thread {
        messages: previous,
        conversation: receipt.conversation,
    });
    state.write().draft.clear();
    state.write().pending = None;
    state.write().conversations = transport::request("GET", "/conversations", Value::Null).await?;
    Ok(())
}
pub async fn open_entry(mut state: Signal<AgentState>, id: String) -> Result<(), String> {
    let node: Value = transport::memory("GET", &format!("/nodes/{id}"), Value::Null).await?;
    if node["kind"] == "SOURCE" {
        open_source(state, id).await
    } else {
        state.write().dialog = Some(Dialog::Entry {
            id,
            title: node["title"].as_str().unwrap_or_default().into(),
            content: node["content"].as_str().unwrap_or_default().into(),
        });
        Ok(())
    }
}
pub async fn open_source(mut state: Signal<AgentState>, id: String) -> Result<(), String> {
    state.write().dialog = Some(Dialog::Source(
        transport::memory("GET", &format!("/sources/{id}"), Value::Null).await?,
    ));
    Ok(())
}
