package site.addzero.aio.agent.workspace

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import site.addzero.aio.agent.editor.AgentDialogs

@Composable
internal fun AgentScreen() {
    val scope = rememberCoroutineScope()
    val state = remember(scope) { AgentState(scope) }
    LaunchedEffect(state) { state.refresh() }
    BoxWithConstraints(Modifier.fillMaxSize()) {
        val compact = maxWidth < 800.dp
        Column(Modifier.fillMaxSize()) {
            Row(
                Modifier.fillMaxWidth().height(64.dp).padding(horizontal = 16.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                if (compact)
                    Tool(
                        if (state.showHistory) "返回对话" else "会话列表",
                        if (state.showHistory) Icons.Default.ArrowBack else Icons.Default.Menu,
                    ) {
                        state.showHistory = !state.showHistory
                    }
                Icon(Icons.Default.AutoAwesome, null, tint = MaterialTheme.colorScheme.primary)
                Spacer(Modifier.width(10.dp))
                Text("Agent", style = MaterialTheme.typography.titleLarge)
                Spacer(Modifier.weight(1f))
                if (state.settings.memoryAvailable)
                    Tool("记忆空间", Icons.Default.Workspaces, !state.busy) { state.configureSpace() }
                Tool("刷新", Icons.Default.Refresh, !state.busy) {
                    state.thread?.conversation?.id?.let(state::select) ?: state.refresh()
                }
                Tool("模型设置", Icons.Default.Settings, !state.busy) {
                    state.dialog = AgentDialog.Settings
                }
                Tool("新建会话", Icons.Default.Add, !state.busy) { state.dialog = AgentDialog.New }
            }
            HorizontalDivider()
            if (state.busy) LinearProgressIndicator(Modifier.fillMaxWidth().height(2.dp))
            else Spacer(Modifier.height(2.dp))
            state.error?.let { error ->
                Row(
                    Modifier.fillMaxWidth()
                        .background(MaterialTheme.colorScheme.errorContainer)
                        .padding(start = 16.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(error, Modifier.weight(1f), maxLines = 3)
                    Tool("关闭错误", Icons.Default.Close) { state.error = null }
                }
            }
            Row(Modifier.weight(1f)) {
                if (compact && state.showHistory) {
                    History(state)
                } else {
                    if (!compact) {
                        Box(Modifier.width(260.dp).fillMaxHeight()) { History(state) }
                        VerticalDivider()
                    }
                    Chat(state, Modifier.weight(1f).fillMaxHeight())
                }
            }
        }
        AgentDialogs(state)
    }
}

@Composable
private fun History(state: AgentState) {
    Column(Modifier.fillMaxSize()) {
        OutlinedTextField(
            state.search,
            { state.search = it },
            Modifier.fillMaxWidth().padding(12.dp).semantics { contentDescription = "搜索会话" },
            singleLine = true,
            placeholder = { Text("搜索会话") },
            leadingIcon = { Icon(Icons.Default.Search, null) },
        )
        LazyColumn(Modifier.weight(1f)) {
            items(
                state.conversations.filter { it.title.contains(state.search, true) },
                key = { it.id },
            ) { item ->
                ListItem(
                    headlineContent = {
                        Text(item.title, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    },
                    supportingContent = {
                        Text(
                            state.settings.providers.find { it.id == item.providerId }?.label
                                ?: "模型",
                            maxLines = 1,
                        )
                    },
                    modifier = Modifier.clickable(enabled = !state.busy) { state.select(item.id) },
                    colors =
                        ListItemDefaults.colors(
                            containerColor =
                                if (state.thread?.conversation?.id == item.id)
                                    MaterialTheme.colorScheme.secondaryContainer
                                else MaterialTheme.colorScheme.surface
                        ),
                )
            }
        }
    }
}

@Composable
private fun Chat(state: AgentState, modifier: Modifier) {
    val thread = state.thread
    if (thread == null) {
        Box(modifier, contentAlignment = Alignment.Center) {
            Column(
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                Icon(
                    Icons.Default.Forum,
                    null,
                    Modifier.size(42.dp),
                    tint = MaterialTheme.colorScheme.primary,
                )
                Text("新对话", style = MaterialTheme.typography.headlineSmall)
                Button({ state.dialog = AgentDialog.New }) {
                    Icon(Icons.Default.Add, null)
                    Text("新建会话")
                }
            }
        }
        return
    }
    val list = rememberLazyListState()
    LaunchedEffect(
        thread.conversation.id,
        thread.messages.size,
        thread.messages.lastOrNull()?.content?.length,
    ) {
        if (
            !list.isScrollInProgress &&
                (!list.canScrollForward ||
                    list.layoutInfo.visibleItemsInfo.lastOrNull()?.index ==
                        thread.messages.lastIndex)
        )
            list.scrollToItem((thread.messages.size - 1).coerceAtLeast(0))
    }
    Column(modifier) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(Modifier.weight(1f)) {
                Text(
                    thread.conversation.title,
                    style = MaterialTheme.typography.titleMedium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    state.settings.providers
                        .find { it.id == thread.conversation.providerId }
                        ?.model
                        .orEmpty(),
                    style = MaterialTheme.typography.labelSmall,
                )
                Text(
                    state.spaces.firstOrNull { it.id == thread.conversation.spaceId }?.title
                        ?: "个人记忆",
                    style = MaterialTheme.typography.labelSmall,
                )
            }
            Tool("删除会话", Icons.Default.Delete, !state.running && !state.busy) {
                state.dialog =
                    AgentDialog.Delete(
                        thread.conversation.title,
                        "/conversations/${thread.conversation.id}",
                    )
            }
        }
        HorizontalDivider()
        LazyColumn(
            Modifier.weight(1f).fillMaxWidth(),
            state = list,
            contentPadding = PaddingValues(20.dp),
            verticalArrangement = Arrangement.spacedBy(24.dp),
        ) {
            items(thread.messages, key = { it.id }) { message ->
                Column(Modifier.fillMaxWidth()) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        Icon(
                            if (message.role == "user") Icons.Default.Person
                            else Icons.Default.AutoAwesome,
                            null,
                            Modifier.size(18.dp),
                            tint =
                                if (message.role == "user") MaterialTheme.colorScheme.secondary
                                else MaterialTheme.colorScheme.primary,
                        )
                        Text(
                            if (message.role == "user") "你" else "Agent",
                            style = MaterialTheme.typography.labelLarge,
                        )
                        if (message.status == "generating")
                            CircularProgressIndicator(Modifier.size(12.dp), strokeWidth = 2.dp)
                    }
                    Spacer(Modifier.height(8.dp))
                    SelectionContainer {
                        Text(
                            message.content.ifEmpty {
                                if (message.status == "generating") "…" else ""
                            },
                            style = MaterialTheme.typography.bodyLarge,
                        )
                    }
                    message.error?.let {
                        Text(
                            it,
                            color = MaterialTheme.colorScheme.error,
                            style = MaterialTheme.typography.bodySmall,
                        )
                    }
                    message.sourceId?.let { id ->
                        TextButton(
                            { state.openSource(id) },
                            enabled = !state.busy && message.memoryStatus != "unavailable",
                        ) {
                            Icon(Icons.Default.Description, null, Modifier.size(16.dp))
                            Text(
                                when (message.memoryStatus) {
                                    "complete" -> "已整理"
                                    "processing" -> "整理中"
                                    "quarantined" -> "保密暂存"
                                    "conflict" -> "待整理"
                                    "failed" -> "整理失败"
                                    "unavailable" -> "来源不可访问"
                                    else -> "已收下"
                                }
                            )
                        }
                    }
                    var expanded by remember(message.id) { mutableStateOf(false) }
                    message.citations.take(if (expanded) message.citations.size else 4).forEach {
                        citation ->
                        TextButton({ state.openEntry(citation.id) }, enabled = !state.busy) {
                            Icon(Icons.Default.Link, null, Modifier.size(16.dp))
                            Text(citation.title, maxLines = 2, overflow = TextOverflow.Ellipsis)
                        }
                    }
                    if (message.citations.size > 4)
                        TextButton({ expanded = !expanded }) {
                            Icon(
                                if (expanded) Icons.Default.ExpandLess
                                else Icons.Default.ExpandMore,
                                null,
                            )
                            Text(if (expanded) "收起来源" else "全部 ${message.citations.size} 个来源")
                        }
                    if (message.status == "cancelled")
                        Text("已停止", style = MaterialTheme.typography.labelSmall)
                    message.tokens?.let {
                        Text(
                            "$it tokens",
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            }
        }
        HorizontalDivider()
        Row(
            Modifier.fillMaxWidth().padding(12.dp),
            verticalAlignment = Alignment.Bottom,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            OutlinedTextField(
                state.draft,
                { if (it.length <= 16000) state.draft = it },
                Modifier.weight(1f).semantics { contentDescription = "消息输入" },
                placeholder = { Text("发送消息") },
                minLines = 2,
                maxLines = 5,
            )
            if (state.running) Tool("停止生成", Icons.Default.Stop, !state.busy) { state.stop() }
            else
                Tool("发送", Icons.Default.ArrowUpward, !state.busy && state.draft.isNotBlank()) {
                    state.send()
                }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun Tool(label: String, icon: ImageVector, enabled: Boolean = true, action: () -> Unit) {
    key(label) {
        TooltipBox(
            positionProvider =
                TooltipDefaults.rememberTooltipPositionProvider(TooltipAnchorPosition.Above),
            tooltip = { PlainTooltip { Text(label) } },
            state = rememberTooltipState(),
        ) {
            IconButton(action, enabled = enabled, modifier = Modifier.size(40.dp)) {
                Icon(icon, label, Modifier.size(20.dp))
            }
        }
    }
}
