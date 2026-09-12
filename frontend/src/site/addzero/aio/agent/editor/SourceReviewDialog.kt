package site.addzero.aio.agent.editor

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.CancellationException
import kotlinx.serialization.json.*
import site.addzero.aio.agent.model.MemorySource
import site.addzero.aio.agent.transport.AgentClient
import site.addzero.aio.agent.workspace.AgentState

@Composable
internal fun SourceReviewDialog(
    state: AgentState,
    id: String,
    dismiss: () -> Unit,
    resolved: (MemorySource) -> Unit,
) {
    var proposal by remember(id) { mutableStateOf<JsonObject?>(null) }
    var current by remember(id) { mutableStateOf<Map<String, JsonObject>>(emptyMap()) }
    var error by remember(id) { mutableStateOf<String?>(null) }
    LaunchedEffect(id) {
        try {
            val result = AgentClient.memory("GET", "/sources/$id/proposal").jsonObject
            current =
                result["entries"]!!
                    .jsonArray
                    .mapNotNull { entry ->
                        entry.jsonObject["existingId"]?.jsonPrimitive?.contentOrNull
                    }
                    .distinct()
                    .associateWith { existing ->
                        AgentClient.memory("GET", "/nodes/$existing").jsonObject
                    }
            proposal = result
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (_: Exception) {
            error = "暂时无法读取修订"
        }
    }
    fun resolve(accept: Boolean) =
        state.run {
            AgentClient.memory(
                "POST",
                "/sources/$id/resolve",
                buildJsonObject {
                    put("accept", accept)
                    put(
                        "versions",
                        buildJsonObject {
                            current.forEach { (nodeId, node) -> put(nodeId, node["version"]!!) }
                        },
                    )
                },
            )
            resolved(AgentClient.source(id))
        }
    AlertDialog(
        dismiss,
        title = { Text("核实修订") },
        text = {
            Column(
                Modifier.fillMaxWidth()
                    .heightIn(max = 480.dp)
                    .verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                if (proposal == null && error == null)
                    LinearProgressIndicator(Modifier.fillMaxWidth())
                proposal?.get("entries")?.jsonArray?.forEach { value ->
                    val entry = value.jsonObject
                    val draft = entry["draft"]!!.jsonObject
                    Text(
                        draft["title"]!!.jsonPrimitive.content,
                        style = MaterialTheme.typography.titleMedium,
                    )
                    current[entry["existingId"]?.jsonPrimitive?.contentOrNull]?.let { node ->
                        Text("当前 v${node["version"]}", style = MaterialTheme.typography.labelMedium)
                        SelectionContainer { Text(node["content"]!!.jsonPrimitive.content) }
                    }
                    Text("建议修订", style = MaterialTheme.typography.labelMedium)
                    SelectionContainer { Text(draft["content"]!!.jsonPrimitive.content) }
                    HorizontalDivider()
                }
                proposal?.get("relations")?.jsonArray?.forEach { relation ->
                    val item = relation.jsonObject
                    Text(
                        "${item["relation"]!!.jsonPrimitive.content}：${item["evidence"]?.jsonPrimitive?.content.orEmpty()}"
                    )
                }
                (error ?: state.error)?.let { Text(it, color = MaterialTheme.colorScheme.error) }
            }
        },
        confirmButton = {
            TextButton({ resolve(true) }, enabled = proposal != null && !state.busy) {
                Text("接受修订")
            }
        },
        dismissButton = {
            TextButton({ resolve(false) }, enabled = proposal != null && !state.busy) {
                Text("保留现有内容")
            }
        },
    )
}
