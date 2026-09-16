mod conversation;
mod graph;
mod memory;
mod models;
mod settings;
mod state;
mod transport;
use dioxus::prelude::*;
use state::AgentState;
fn main() {
    console_error_panic_hook::set_once();
    LaunchBuilder::web()
        .with_cfg(
            dioxus::web::Config::new()
                .history(std::rc::Rc::new(dioxus::history::MemoryHistory::default())),
        )
        .launch(App);
}
#[component]
fn App() -> Element {
    let mut state = use_signal(AgentState::default);
    let mut settings_page = use_signal(|| None::<bool>);
    use_context_provider(|| state);
    use_future(move || async move {
        let settings = document::eval(
            "return document.querySelector('meta[name=aio-page]')?.content === 'settings';",
        )
        .await
        .unwrap_or_default()
        .as_bool()
        .unwrap_or(false);
        settings_page.set(Some(settings));
        state.write().settings_page = settings;
        if let Err(error) = state::load(state, !settings).await {
            state.write().error = Some(error);
        }
    });
    use_future(move || async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(650).await;
            let current = state.peek().thread.clone();
            if state.peek().busy {
                continue;
            }
            if let Some(thread) = current {
                if !state.peek().processing() {
                    continue;
                }
                let version = state.peek().generation;
                let result = transport::get_thread(thread.conversation.id).await;
                if state.peek().generation != version {
                    continue;
                }
                match result {
                    Ok(thread) => {
                        state.write().thread = Some(thread);
                    }
                    Err(error) => {
                        state.write().error = Some(error);
                    }
                }
            }
        }
    });
    rsx! {
        az_ui_components::UiStylesheets { relative_paths: true }
        if state.read().settings.is_none() {
            az_ui_components::admin::RequestState { error: state
                        .read().error.clone().unwrap_or_default() }
        } else if settings_page() == Some(true) {
            settings::SettingsPanel {}
        } else {
            conversation::ConversationPage {}
        }
        if let Some(dialog) = state.read().dialog.clone() {
            settings::Dialogs { dialog }
        }
    }
}
