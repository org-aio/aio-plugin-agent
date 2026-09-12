package site.addzero.aio.agent.editor

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.serialization.json.*
import site.addzero.aio.agent.model.*
import site.addzero.aio.agent.transport.AgentClient
import site.addzero.aio.agent.workspace.*

@Composable
internal fun SourceDialog(state: AgentState, source: MemorySource, dismiss: () -> Unit) {
    var grant by remember(source.id) { mutableStateOf<SecretReference?>(null) }
    var reviewing by remember(source.id) { mutableStateOf(false) }
    AlertDialog(
        dismiss,
        title = { Text("来源资料") },
        text = {
            Column(
                Modifier.fillMaxWidth().verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                SelectionContainer { Text(source.text) }
                source.error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
                source.secrets.forEach { secret ->
                    SecretField(secret, onGrant = { grant = secret })
                }
                if (source.status == "conflict")
                    TextButton({ reviewing = true }) {
                        Icon(Icons.Default.FactCheck, null)
                        Text("核实修订")
                    }
                if (source.status == "failed")
                    TextButton({
                        state.run {
                            AgentClient.memory("POST", "/sources/${source.id}/retry")
                            state.dialog = AgentDialog.Source(AgentClient.source(source.id))
                        }
                    }) {
                        Icon(Icons.Default.Refresh, null)
                        Text("重新整理")
                    }
                state.error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
            }
        },
        confirmButton = { TextButton(dismiss) { Text("关闭") } },
    )
    grant?.let { secret -> GrantDialog(state, secret) { grant = null } }
    if (reviewing)
        SourceReviewDialog(state, source.id, { reviewing = false }) {
            reviewing = false
            state.dialog = AgentDialog.Source(it)
        }
}

@Composable
private fun SecretField(secret: SecretReference, onGrant: () -> Unit) {
    var value by remember(secret.id) { mutableStateOf<String?>(null) }
    var error by remember(secret.id) { mutableStateOf<String?>(null) }
    var loading by remember(secret.id) { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    DisposableEffect(secret.id) { onDispose { value = null } }
    LaunchedEffect(value) {
        if (value != null) {
            delay(30_000)
            value = null
        }
    }
    Column {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(secret.label, Modifier.weight(1f))
            Tool(
                if (value == null) "查看秘密" else "隐藏秘密",
                if (value == null) Icons.Default.Visibility else Icons.Default.VisibilityOff,
                secret.canReveal && !loading,
            ) {
                if (value != null) value = null
                else {
                    loading = true
                    scope.launch {
                        try {
                            value = AgentClient.reveal(secret.id)
                            error = null
                        } catch (_: Exception) {
                            error = "暂时无法查看"
                        } finally {
                            loading = false
                        }
                    }
                }
            }
            Tool("复制秘密", Icons.Default.ContentCopy, value != null) {
                value?.let { secretValue ->
                    scope.launch {
                        try {
                            AgentClient.copy(secretValue)
                            error = null
                        } catch (_: Exception) {
                            error = "复制未获授权"
                        }
                    }
                }
            }
            if (secret.canManage) Tool("秘密授权", Icons.Default.PersonAdd) { onGrant() }
        }
        if (loading) LinearProgressIndicator(Modifier.fillMaxWidth())
        Box(Modifier.fillMaxWidth().height(96.dp).verticalScroll(rememberScrollState())) {
            SelectionContainer {
                Text(value ?: "********", style = MaterialTheme.typography.bodyMedium)
            }
        }
        error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
    }
}

@Composable
private fun GrantDialog(state: AgentState, secret: SecretReference, dismiss: () -> Unit) {
    var user by remember { mutableStateOf("") }
    var reveal by remember { mutableStateOf(true) }
    var manage by remember { mutableStateOf(false) }
    AlertDialog(
        dismiss,
        title = { Text("秘密授权") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(secret.label)
                OutlinedTextField(
                    user,
                    { user = it },
                    label = { Text("空间成员 ID") },
                    singleLine = true,
                )
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Checkbox(reveal, { reveal = it })
                    Text("允许查看与复制")
                }
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Checkbox(manage, { manage = it })
                    Text("允许管理授权")
                }
                state.error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
            }
        },
        confirmButton = {
            TextButton(
                {
                    state.run {
                        AgentClient.memory(
                            "PUT",
                            "/secrets/${secret.id}/grants",
                            buildJsonObject {
                                put("userId", user.trim())
                                put("reveal", reveal)
                                put("manage", manage)
                            },
                        )
                        dismiss()
                    }
                },
                enabled = !state.busy && user.isNotBlank(),
            ) {
                Text("保存")
            }
        },
        dismissButton = { TextButton(dismiss) { Text("取消") } },
    )
}

@Composable
internal fun EntryDialog(state: AgentState, entry: AgentDialog.Entry, dismiss: () -> Unit) {
    var sources by remember(entry.id) { mutableStateOf<List<MemorySource>>(emptyList()) }
    LaunchedEffect(entry.id) {
        runCatching {
            sources =
                Json.decodeFromJsonElement(AgentClient.memory("GET", "/nodes/${entry.id}/sources"))
        }
    }
    AlertDialog(
        dismiss,
        title = { Text(entry.title) },
        text = {
            Column(
                Modifier.fillMaxWidth().verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                SelectionContainer { Text(entry.content) }
                sources.forEach { source ->
                    TextButton({ state.openSource(source.id) }) {
                        Icon(Icons.Default.Description, null)
                        Text(source.text.lineSequence().firstOrNull().orEmpty().take(60))
                    }
                }
            }
        },
        confirmButton = { TextButton(dismiss) { Text("关闭") } },
    )
}

@Composable
internal fun SpaceDialog(state: AgentState, dismiss: () -> Unit) {
    var selected by remember {
        mutableStateOf(
            state.thread?.conversation?.spaceId ?: state.spaces.firstOrNull()?.id.orEmpty()
        )
    }
    val current = state.spaces.firstOrNull { it.id == selected }
    var title by remember(selected) { mutableStateOf(current?.title.orEmpty()) }
    var model by remember(selected) { mutableStateOf(current?.modelBinding.orEmpty()) }
    var member by remember(selected) { mutableStateOf("") }
    var role by remember(selected) { mutableStateOf("EDITOR") }
    var members by remember(selected) { mutableStateOf<JsonArray>(JsonArray(emptyList())) }
    var removing by remember(selected) { mutableStateOf<String?>(null) }
    LaunchedEffect(selected) {
        if (current?.role == "OWNER" && !current.personal)
            runCatching {
                members = AgentClient.memory("GET", "/spaces/$selected/members").jsonArray
            }
    }
    AlertDialog(
        dismiss,
        title = { Text("记忆空间") },
        text = {
            Column(
                Modifier.fillMaxWidth().verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                Choice(
                    "空间",
                    state.spaces.map { it.id to it.title } + listOf("" to "新建团队空间"),
                    selected,
                    { selected = it },
                )
                OutlinedTextField(
                    title,
                    { title = it },
                    Modifier.fillMaxWidth(),
                    label = { Text("名称") },
                    singleLine = true,
                    enabled = current == null || current.role == "OWNER",
                )
                if (current == null || current.role == "OWNER")
                    Choice(
                        "整理模型",
                        listOf("" to "暂不自动整理") + state.settings.providers.map { it.id to it.label },
                        model,
                        { model = it },
                    )
                if (current?.role == "OWNER" && !current.personal) {
                    HorizontalDivider()
                    members.forEach { item ->
                        val value = item.jsonObject
                        val id = value["userId"]!!.jsonPrimitive.content
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Column(Modifier.weight(1f)) {
                                Text(id)
                                Text(
                                    value["role"]!!.jsonPrimitive.content,
                                    style = MaterialTheme.typography.labelSmall,
                                )
                            }
                            Tool("移除成员", Icons.Default.PersonRemove, !state.busy) { removing = id }
                        }
                    }
                    OutlinedTextField(
                        member,
                        { member = it },
                        Modifier.fillMaxWidth(),
                        label = { Text("成员 ID") },
                        singleLine = true,
                    )
                    Choice(
                        "成员角色",
                        listOf("EDITOR" to "编辑者", "READER" to "阅读者", "OWNER" to "管理者"),
                        role,
                        { role = it },
                    )
                    TextButton(
                        {
                            state.run {
                                AgentClient.memory(
                                    "POST",
                                    "/spaces/$selected/members",
                                    buildJsonObject {
                                        put("userId", member.trim())
                                        put("role", role)
                                    },
                                )
                                members =
                                    AgentClient.memory("GET", "/spaces/$selected/members").jsonArray
                                member = ""
                            }
                        },
                        enabled = !state.busy && member.isNotBlank(),
                    ) {
                        Icon(Icons.Default.PersonAdd, null)
                        Text("保存成员")
                    }
                }
                state.error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
            }
        },
        confirmButton = {
            TextButton(
                {
                    state.run {
                        AgentClient.memory(
                            if (selected.isEmpty()) "POST" else "PUT",
                            if (selected.isEmpty()) "/spaces" else "/spaces/$selected",
                            buildJsonObject {
                                put("title", title.trim())
                                put("modelBinding", model.ifEmpty { null })
                            },
                        )
                        state.spaces = AgentClient.spaces()
                        dismiss()
                    }
                },
                enabled =
                    !state.busy &&
                        title.isNotBlank() &&
                        (current == null || current.role == "OWNER"),
            ) {
                Text("保存")
            }
        },
        dismissButton = { TextButton(dismiss) { Text("关闭") } },
    )
    removing?.let { id ->
        AlertDialog(
            { removing = null },
            title = { Text("移除成员") },
            text = { Text(id) },
            confirmButton = {
                TextButton({
                    state.run {
                        AgentClient.memory("DELETE", "/spaces/$selected/members/$id")
                        members = AgentClient.memory("GET", "/spaces/$selected/members").jsonArray
                        removing = null
                    }
                }) {
                    Text("确认移除")
                }
            },
            dismissButton = { TextButton({ removing = null }) { Text("取消") } },
        )
    }
}
