package site.addzero.aio.agent.workspace

import androidx.compose.runtime.*
import kotlinx.coroutines.*
import kotlinx.serialization.json.Json
import site.addzero.aio.agent.model.*
import site.addzero.aio.agent.transport.AgentClient
import site.addzero.aio.agent.transport.requestId

internal class AgentState(private val scope: CoroutineScope) {
    var settings by mutableStateOf(Settings(emptyList(), 16000, emptyList()))
    var conversations by mutableStateOf<List<Conversation>>(emptyList())
    var thread by mutableStateOf<Thread?>(null)
    var draft by mutableStateOf("")
    var search by mutableStateOf("")
    var busy by mutableStateOf(false)
    var error by mutableStateOf<String?>(null)
    var dialog by mutableStateOf<AgentDialog?>(null)
    var showHistory by mutableStateOf(false)
    private var generation = 0
    private var poll: Job? = null
    private var pending: Pair<String, Prompt>? = null
    val running
        get() = thread?.messages?.any { it.status == "generating" } == true

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
        conversations = AgentClient.conversations()
    }

    fun select(id: String) = run {
        val version = ++generation
        poll?.cancel()
        thread = AgentClient.thread(id)
        draft = ""
        pending = null
        showHistory = false
        watch(version)
    }

    fun create(provider: String, title: String) = run {
        val created = AgentClient.create(ConversationDraft(provider, title))
        dialog = null
        conversations = AgentClient.conversations()
        thread = AgentClient.thread(created.id)
        draft = ""
        pending = null
        poll?.cancel()
        generation++
        showHistory = false
    }

    fun send() = run {
        val id = thread?.conversation?.id ?: return@run
        val input = draft.trim()
        if (input.isEmpty()) return@run
        val prompt =
            pending?.takeIf { it.first == id && it.second.content == input }?.second
                ?: Prompt(input, requestId()).also { pending = id to it }
        thread = AgentClient.send(id, prompt)
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
        if (!running) return
        val id = thread!!.conversation.id
        poll =
            scope.launch {
                while (version == generation && running) {
                    delay(350)
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
}
