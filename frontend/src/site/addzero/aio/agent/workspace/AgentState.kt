package site.addzero.aio.agent.workspace

import androidx.compose.runtime.*
import kotlinx.coroutines.*
import kotlinx.serialization.json.Json
import site.addzero.aio.agent.model.*
import site.addzero.aio.agent.transport.AgentClient
import site.addzero.aio.agent.transport.requestId

internal class AgentState(private val scope: CoroutineScope) {
    var settings by
        mutableStateOf(
            Settings(
                allowedEndpoints = emptyList(),
                maxPromptChars = 16000,
                memoryAvailable = false,
                providers = emptyList(),
            )
        )
    var spaces by mutableStateOf<List<MemorySpace>>(emptyList())
    var conversations by mutableStateOf<List<Conversation>>(emptyList())
    var thread by mutableStateOf<Thread?>(null)
    var draft by mutableStateOf("")
    var search by mutableStateOf("")
    var busy by mutableStateOf(false)
    var error by mutableStateOf<String?>(null)
    var dialog by mutableStateOf<AgentDialog?>(null)
    var showHistory by mutableStateOf(false)
    var showGraph by mutableStateOf(true)
    var focusedMessageId by mutableStateOf<String?>(null)
    private var generation = 0
    private var poll: Job? = null
    private var pending: Pair<String, Prompt>? = null
    val running
        get() = thread?.messages?.any { it.status == "generating" } == true

    val processing
        get() =
            thread?.messages?.any {
                it.status == "queued" ||
                    (it.memoryStatus in setOf("pending", "processing") &&
                        spaces.any { space ->
                            space.id == thread?.conversation?.spaceId && space.modelBinding != null
                        })
            } == true

    fun run(action: suspend () -> Unit) {
        if (busy) return
        busy = true
        error = null
        scope.launch {
            try {
                action()
            } catch (c: CancellationException) {
                throw c
            } catch (c: Throwable) {
                report(c)
            } finally {
                busy = false
            }
        }
    }

    private fun report(c: Throwable) {
        val text = c.message ?: "请求失败"
        error =
            runCatching { Json.decodeFromString<Failure>(text).error }.getOrDefault(text.take(200))
    }

    fun refresh() = run {
        settings = AgentClient.settings()
        if (settings.memoryAvailable) spaces = AgentClient.spaces()
        conversations = AgentClient.conversations()
        if (thread == null) {
            val conversation =
                conversations.firstOrNull()
                    ?: AgentClient.create(
                        ConversationDraft(
                            title = "记忆对话",
                            providerId = settings.providers.firstOrNull()?.id,
                            spaceId = spaces.firstOrNull { it.personal }?.id,
                        )
                    )
            thread = AgentClient.thread(conversation.id)
            conversations = AgentClient.conversations()
            watch(generation)
        }
    }

    fun select(id: String) = run {
        val version = ++generation
        poll?.cancel()
        thread = AgentClient.thread(id)
        focusedMessageId = null
        draft = ""
        pending = null
        showHistory = false
        watch(version)
    }

    fun create(provider: String, title: String, spaceId: String? = null) = run {
        val space = spaces.firstOrNull { it.id == spaceId }
        if (space?.role == "OWNER" && space.modelBinding == null && provider.isNotEmpty()) {
            AgentClient.bindModel(space, provider)
            spaces = AgentClient.spaces()
        }
        val created =
            AgentClient.create(
                ConversationDraft(
                    providerId = provider.ifEmpty { null },
                    title = title,
                    spaceId = spaceId,
                )
            )
        dialog = null
        conversations = AgentClient.conversations()
        thread = AgentClient.thread(created.id)
        focusedMessageId = null
        draft = ""
        pending = null
        poll?.cancel()
        generation++
        showHistory = false
    }

    fun send() = run {
        val id = thread?.conversation?.id ?: return@run
        val input = draft
        if (input.isBlank()) return@run
        val prompt =
            pending?.takeIf { it.first == id && it.second.content == input }?.second
                ?: Prompt(input, requestId()).also { pending = id to it }
        val receipt = AgentClient.send(id, prompt)
        focusedMessageId = null
        val ids = receipt.messages.map { it.id }.toSet()
        val previous = thread?.takeIf { it.conversation.id == id }?.messages.orEmpty()
        thread = receipt.copy(messages = previous.filter { it.id !in ids } + receipt.messages)
        draft = ""
        pending = null
        watch(generation)
        conversations = AgentClient.conversations()
    }

    fun stop() = run {
        thread?.let {
            thread = AgentClient.cancel(it.conversation.id)
            watch(generation)
        }
    }

    private fun watch(version: Int) {
        poll?.cancel()
        if (!running && !processing) return
        val id = thread!!.conversation.id
        poll =
            scope.launch {
                while (version == generation && (running || processing)) {
                    delay(if (running) 350 else 2000)
                    try {
                        val next = AgentClient.thread(id)
                        if (version == generation) thread = next
                    } catch (c: CancellationException) {
                        throw c
                    } catch (c: Throwable) {
                        report(c)
                        break
                    }
                }
                if (version == generation && !running) {
                    try {
                        conversations = AgentClient.conversations()
                    } catch (c: CancellationException) {
                        throw c
                    } catch (c: Throwable) {
                        report(c)
                    }
                }
            }
    }

    fun saveProvider(id: String?, value: ProviderDraft) = run {
        AgentClient.provider(id, value)
        settings = AgentClient.settings()
        dialog = AgentDialog.Settings
    }

    fun openSource(id: String) = run { dialog = AgentDialog.Source(AgentClient.source(id)) }

    fun openEntry(id: String) = run {
        val entry = AgentClient.memory("GET", "/nodes/$id") as kotlinx.serialization.json.JsonObject
        if ((entry["kind"] as? kotlinx.serialization.json.JsonPrimitive)?.content == "SOURCE") {
            dialog = AgentDialog.Source(AgentClient.source(id))
            return@run
        }
        dialog =
            AgentDialog.Entry(
                id,
                entry["title"]
                    ?.let { it as? kotlinx.serialization.json.JsonPrimitive }
                    ?.content
                    .orEmpty(),
                entry["content"]
                    ?.let { it as? kotlinx.serialization.json.JsonPrimitive }
                    ?.content
                    .orEmpty(),
            )
    }

    fun configureSpace() {
        dialog = AgentDialog.Spaces
    }

    fun remove(target: AgentDialog.Delete) = run {
        AgentClient.remove(target.path)
        dialog = null
        settings = AgentClient.settings()
        conversations = AgentClient.conversations()
        if (target.path == "/conversations/${thread?.conversation?.id}") {
            poll?.cancel()
            generation++
            thread = null
            draft = ""
        }
    }
}

internal sealed interface AgentDialog {
    data object New : AgentDialog

    data object Settings : AgentDialog

    data class ProviderEditor(val provider: Provider? = null) : AgentDialog

    data class Delete(val title: String, val path: String) : AgentDialog

    data class Source(val source: MemorySource) : AgentDialog

    data class Entry(val id: String, val title: String, val content: String) : AgentDialog

    data object Spaces : AgentDialog
}
