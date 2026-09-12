package site.addzero.aio.agent.graph

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
import site.addzero.aio.agent.model.*
import site.addzero.aio.agent.transport.AgentClient
import site.addzero.aio.agent.workspace.AgentState
import site.addzero.aio.agent.workspace.Tool
import site.addzero.component.knowledge_graph.main_logic.KnowledgeGraph
import site.addzero.component.knowledge_graph.model.*

@Composable
internal fun ConversationGraph(state: AgentState, modifier: Modifier = Modifier) {
    val thread = state.thread
    val spaceId = thread?.conversation?.spaceId
    val message =
        thread?.messages?.firstOrNull { it.id == state.focusedMessageId }
            ?: thread?.messages?.lastOrNull { it.role == "assistant" }
    var graph by remember(spaceId) { mutableStateOf<MemoryGraph?>(null) }
    var error by remember(spaceId) { mutableStateOf(false) }
    var loading by remember(spaceId) { mutableStateOf(false) }
    var refresh by remember { mutableStateOf(0) }
    var selected by remember(thread?.conversation?.id) { mutableStateOf<String?>(null) }
    var animate by remember { mutableStateOf(true) }
    var zoom by remember { mutableStateOf(1f) }
    var list by remember { mutableStateOf(false) }
    val ids = message?.activatedNodeIds.orEmpty()
    LaunchedEffect(
        spaceId,
        thread?.conversation?.id,
        message?.id,
        ids,
        message?.memoryStatus,
        refresh,
    ) {
        graph = null
        selected = null
        if (spaceId == null) return@LaunchedEffect
        while (true) {
            loading = true
            try {
                graph = AgentClient.activation(spaceId, ids.take(24))
                selected = selected?.takeIf { id -> graph?.nodes?.any { it.id == id } == true }
                error = false
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (_: Throwable) {
                graph = null
                selected = null
                error = true
            } finally {
                loading = false
            }
            delay(15000)
        }
    }
    val nodes = graph?.nodes.orEmpty()
    val visibleIds = nodes.map { it.id }.toSet()
    val direct = message?.matchedNodeIds.orEmpty().toSet().intersect(visibleIds)
    val linked =
        if (message?.route == "save")
            graph
                ?.edges
                .orEmpty()
                .filter { it.source in ids || it.target in ids }
                .flatMap { listOf(it.source, it.target) }
        else emptyList()
    val activated = (ids + linked).toSet().intersect(visibleIds)
    val data =
        remember(nodes, graph?.edges) {
            GraphData(
                nodes.map { GraphNode(it.id, it.title, NodeCategory.DEFAULT, null, null, it.kind) },
                graph?.edges.orEmpty().map { GraphEdge(it.source, it.target, it.relation) },
            )
        }
    Column(modifier) {
        Row(
            Modifier.fillMaxWidth().height(48.dp).padding(start = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(
                Icons.Default.Hub,
                null,
                Modifier.size(18.dp),
                tint = MaterialTheme.colorScheme.primary,
            )
            Spacer(Modifier.width(8.dp))
            Text(
                "知识图谱",
                style = MaterialTheme.typography.titleSmall,
                modifier = Modifier.weight(1f),
            )
            if (state.focusedMessageId != null)
                Tool("回到最新一轮", Icons.Default.Update) { state.focusedMessageId = null }
            Tool("切换图谱视图", if (list) Icons.Default.Hub else Icons.Default.List) { list = !list }
            Tool("刷新图谱", Icons.Default.Refresh, !loading) { refresh++ }
            Tool("收起图谱", Icons.Default.Close) { state.showGraph = false }
        }
        if (loading) LinearProgressIndicator(Modifier.fillMaxWidth().height(2.dp))
        else HorizontalDivider(Modifier.height(2.dp))
        Box(Modifier.weight(1f).fillMaxWidth()) {
            when {
                error ->
                    Text(
                        "图谱暂不可用",
                        Modifier.align(Alignment.Center),
                        color = MaterialTheme.colorScheme.error,
                    )
                nodes.isEmpty() ->
                    Text(if (loading) "加载中" else "暂无记忆", Modifier.align(Alignment.Center))
                list ->
                    LazyColumn(Modifier.fillMaxSize()) {
                        items(nodes.sortedByDescending { it.id in activated }, key = { it.id }) {
                            node ->
                            ListItem(
                                headlineContent = {
                                    Text(node.title, maxLines = 1, overflow = TextOverflow.Ellipsis)
                                },
                                leadingContent = {
                                    Icon(
                                        if (node.id in activated) Icons.Default.RadioButtonChecked
                                        else Icons.Default.RadioButtonUnchecked,
                                        null,
                                        tint =
                                            if (node.id in direct) Color(0xFFB06712)
                                            else MaterialTheme.colorScheme.primary,
                                    )
                                },
                                modifier = Modifier.clickable { selected = node.id },
                                colors =
                                    ListItemDefaults.colors(
                                        containerColor =
                                            if (node.id == selected)
                                                MaterialTheme.colorScheme.secondaryContainer
                                            else MaterialTheme.colorScheme.surface
                                    ),
                            )
                        }
                    }
                else ->
                    KnowledgeGraph(
                        data,
                        modifier =
                            Modifier.semantics {
                                contentDescription =
                                    "知识图谱，${nodes.size} 个节点，${activated.size} 个激活节点"
                            },
                        onNodeClick = { selected = it.id },
                        selectedNodeId = selected,
                        highlightedNodeIds = activated,
                        nodeColors =
                            nodes.associate {
                                it.id to
                                    if (it.id in direct) Color(0xFFB06712) else nodeColor(it.kind)
                            },
                        edgeLabelProvider = { edge ->
                            if (edge.source == selected || edge.target == selected)
                                edge.label.orEmpty()
                            else ""
                        },
                        animate = animate,
                        zoom = zoom,
                    )
            }
        }
        HorizontalDivider()
        Row(
            Modifier.fillMaxWidth().height(42.dp).padding(start = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            val selectedNode = nodes.firstOrNull { it.id == selected }
            Text(
                selectedNode?.title
                    ?: ("命中 ${direct.size} · 关联 ${(activated - direct).size}" +
                        if (graph?.truncated == true) " · 部分图谱" else ""),
                Modifier.weight(1f),
                style = MaterialTheme.typography.labelSmall,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Tool("打开记忆条目", Icons.Default.OpenInNew, selectedNode != null && !state.busy) {
                selectedNode?.let { node ->
                    if (node.kind == "SOURCE") state.openSource(node.id)
                    else state.openEntry(node.id)
                }
            }
            if (!list) {
                Tool(
                    if (animate) "暂停图谱布局" else "继续图谱布局",
                    if (animate) Icons.Default.Pause else Icons.Default.PlayArrow,
                ) {
                    animate = !animate
                }
                Tool("缩小图谱", Icons.Default.ZoomOut, zoom > .4f) {
                    zoom = (zoom - .2f).coerceAtLeast(.4f)
                }
                Tool("放大图谱", Icons.Default.ZoomIn, zoom < 3f) {
                    zoom = (zoom + .2f).coerceAtMost(3f)
                }
            }
        }
    }
}

private fun nodeColor(kind: String): Color =
    when (kind) {
        "PROJECT" -> Color(0xFF36776B)
        "PERSON" -> Color(0xFF9D487B)
        "CONCEPT" -> Color(0xFF476FBD)
        "EVENT" -> Color(0xFF947425)
        "SOURCE" -> Color(0xFF686C75)
        else -> Color(0xFF397CA0)
    }
