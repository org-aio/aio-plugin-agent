@file:OptIn(
    kotlin.js.ExperimentalWasmJsInterop::class,
    kotlin.wasm.unsafe.UnsafeWasmMemoryApi::class,
)

package site.addzero.aio.agent.theme

import androidx.compose.runtime.*
import androidx.compose.ui.platform.LocalFontFamilyResolver
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.platform.Font
import kotlin.js.*
import kotlin.wasm.unsafe.withScopedMemoryAllocator
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.await

private fun fetchFont(): Promise<JsAny> =
    js(
        "fetch(new URL('noto-cjk.otf',document.baseURI)).then(r=>{if(!r.ok)throw new Error('Font unavailable');return r.arrayBuffer()}).then(b=>new Uint8Array(b))"
    )

private fun length(bytes: JsAny): Int = js("bytes.length")

private fun copy(bytes: JsAny, address: Int): Unit =
    js("new Uint8Array(wasmExports.memory.buffer,address,bytes.length).set(bytes)")

internal class FontState {
    var font by mutableStateOf<FontFamily?>(null)
    var failed by mutableStateOf(false)
    var attempt by mutableStateOf(0)
}

@Composable
internal fun rememberAgentFont(): FontState {
    val resolver = LocalFontFamilyResolver.current
    val state = remember { FontState() }
    LaunchedEffect(resolver, state.attempt) {
        state.failed = false
        try {
            val bytes = fetchFont().await<JsAny>()
            val data = withScopedMemoryAllocator { allocator ->
                val pointer = allocator.allocate(length(bytes))
                copy(bytes, pointer.address.toInt())
                ByteArray(length(bytes)) { (pointer + it).loadByte() }
            }
            val family = FontFamily(Font(identity = "agent-noto-cjk", data = data))
            resolver.preload(family)
            state.font = family
        } catch (c: CancellationException) {
            throw c
        } catch (_: Throwable) {
            state.failed = true
        }
    }
    return state
}
