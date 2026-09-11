// 由 Rust JsonSchema 生成，修改模型后重新运行生成器。
package site.addzero.aio.agent.model

import kotlinx.serialization.Serializable

@Serializable
data class Conversation(
    val id: String,
    val providerId: String,
    val title: String,
    val updatedAt: String
)

@Serializable
data class ConversationDraft(
    val providerId: String,
    val title: String
)

@Serializable
data class Failure(
    val error: String
)

@Serializable
data class Message(
    val content: String,
    val error: String? = null,
    val id: String,
    val role: String,
    val status: String,
    val tokens: Long? = null
)

@Serializable
data class Prompt(
    val content: String,
    val requestId: String
)

@Serializable
data class Provider(
    val endpoint: String,
    val hasSecret: Boolean,
    val id: String,
    val label: String,
    val model: String
)

@Serializable
data class ProviderDraft(
    val endpoint: String,
    val label: String,
    val model: String,
    val secret: String? = null
)

@Serializable
data class Settings(
    val allowedEndpoints: List<String>,
    val maxPromptChars: Long,
    val providers: List<Provider>
)

@Serializable
data class Thread(
    val conversation: Conversation,
    val messages: List<Message>
)
