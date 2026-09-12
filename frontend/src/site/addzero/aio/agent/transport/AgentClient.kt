@file:OptIn(kotlin.js.ExperimentalWasmJsInterop::class)

package site.addzero.aio.agent.transport

import kotlin.js.*
import kotlinx.coroutines.await
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.*
import kotlinx.serialization.json.Json
import site.addzero.aio.agent.model.*

private fun invoke(method: JsString, path: JsString, payload: JsString): Promise<JsString> =
    js(
        "window.aioPlugin.json(method,path,payload ? JSON.parse(payload) : undefined).then(value=>JSON.stringify(value ?? null))"
    )

fun requestId(): String = uuid().toString()

private fun uuid(): JsString = js("crypto.randomUUID()")

private fun copyText(value: JsString): Promise<JsAny?> = js("window.aioPlugin.copy(value)")

internal object AgentClient {
    suspend fun copy(value: String) {
        copyText(value.toJsString()).await<JsAny?>()
    }

    suspend fun memory(method: String, path: String, body: JsonElement = JsonNull): JsonElement =
        read(
            "POST",
            "/memory",
            buildJsonObject {
                    put("method", method)
                    put("path", path)
                    put("body", body)
                }
                .toString(),
        )

    suspend fun spaces(): List<MemorySpace> = Json.decodeFromJsonElement(memory("GET", "/spaces"))

    suspend fun source(id: String): MemorySource =
        Json.decodeFromJsonElement(memory("GET", "/sources/$id"))

    suspend fun reveal(id: String): String =
        memory("POST", "/secrets/$id/reveal").jsonObject["value"]!!.jsonPrimitive.content

    suspend fun bindModel(space: MemorySpace, provider: String) {
        memory(
            "PUT",
            "/spaces/${space.id}",
            buildJsonObject {
                put("title", space.title)
                put("modelBinding", provider)
            },
        )
    }

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
