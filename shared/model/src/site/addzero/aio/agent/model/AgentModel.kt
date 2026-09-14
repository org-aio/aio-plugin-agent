// 由 Rust JsonSchema 生成，修改模型后重新运行生成器。
package site.addzero.aio.agent.model

import kotlinx.serialization.Serializable

@Serializable
data class Conversation(
    val id: String,
    val providerId: String? = null,
    val spaceId: String? = null,
    val title: String,
    val updatedAt: String
)

@Serializable
data class ConversationDraft(
    val providerId: String? = null,
    val spaceId: String? = null,
    val title: String
)

@Serializable
data class Failure(
    val error: String
)

@Serializable
data class MemoryCitation(
    val id: String,
    val title: String
)

@Serializable
data class MemoryEdge(
    val evidence: String,
    val id: String,
    val relation: String,
    val source: String,
    val target: String
)

@Serializable
data class MemoryGraph(
    val edges: List<MemoryEdge>,
    val nodes: List<MemoryNode>,
    val total: Long,
    val truncated: Boolean
)

@Serializable
data class MemoryNode(
    val aliases: List<String>,
    val content: String,
    val id: String,
    val kind: String,
    val tags: List<String>,
    val title: String,
    val updatedAt: Long,
    val url: String,
    val version: Long
)

@Serializable
data class MemorySource(
    val createdBy: String,
    val error: String? = null,
    val id: String,
    val secrets: List<SecretReference>,
    val spaceId: String,
    val status: String,
    val text: String,
    val updatedAt: Long
)

@Serializable
data class MemorySpace(
    val id: String,
    val modelBinding: String? = null,
    val personal: Boolean,
    val role: String,
    val title: String
)

@Serializable
data class Message(
    val activatedNodeIds: List<String>,
    val citations: List<MemoryCitation>,
    val content: String,
    val error: String? = null,
    val id: String,
    val matchedNodeIds: List<String>,
    val memoryStatus: String? = null,
    val role: String,
    val route: String? = null,
    val sourceId: String? = null,
    val status: String,
    val tokens: Long? = null
)

@Serializable
data class ModelListRequest(
    val endpoint: String,
    val providerId: String? = null,
    val secret: String? = null
)

@Serializable
data class ModelSelection(
    val providerId: String? = null
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
data class SecretReference(
    val canManage: Boolean,
    val canReveal: Boolean,
    val id: String,
    val label: String,
    val sourceId: String
)

@Serializable
data class Settings(
    val allowedEndpoints: List<String>,
    val maxPromptChars: Long,
    val memoryAvailable: Boolean,
    val providers: List<Provider>
)

@Serializable
data class Thread(
    val conversation: Conversation,
    val messages: List<Message>
)
