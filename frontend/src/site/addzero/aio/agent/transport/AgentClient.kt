@file:OptIn(kotlin.js.ExperimentalWasmJsInterop::class)

package site.addzero.aio.agent.transport

import kotlin.js.*
import kotlinx.coroutines.await
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import site.addzero.aio.agent.model.*

private fun invoke(method: JsString, path: JsString, payload: JsString): Promise<JsString> =
    js(
        "window.aioPlugin.json(method,path,payload ? JSON.parse(payload) : undefined).then(value=>JSON.stringify(value ?? null))"
    )

fun requestId(): String = uuid().toString()

private fun uuid(): JsString = js("crypto.randomUUID()")

internal object AgentClient {
    private suspend inline fun <reified T> read(
        method: String,
        path: String,
        payload: String = "",
    ): T =
        Json.decodeFromString(
            invoke(method.toJsString(), path.toJsString(), payload.toJsString())
                .await<JsString>()
                .toString()
        )

    suspend fun settings(): Settings = read("GET", "/settings")

    suspend fun conversations(): List<Conversation> = read("GET", "/conversations")

    suspend fun thread(id: String): Thread = read("GET", "/conversations/$id")

    suspend fun create(draft: ConversationDraft): Conversation =
        read("POST", "/conversations", Json.encodeToString(draft))

    suspend fun send(id: String, prompt: Prompt): Thread =
        read("POST", "/conversations/$id/messages", Json.encodeToString(prompt))

    suspend fun cancel(id: String): Thread = read("POST", "/conversations/$id/cancel")

    suspend fun provider(id: String?, draft: ProviderDraft): Provider =
        read(
            if (id == null) "POST" else "PUT",
            if (id == null) "/providers" else "/providers/$id",
            Json.encodeToString(draft),
        )

    suspend fun remove(path: String) {
        invoke("DELETE".toJsString(), path.toJsString(), "".toJsString()).await<JsString>()
    }
}
