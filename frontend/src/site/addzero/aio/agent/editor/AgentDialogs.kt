package site.addzero.aio.agent.editor

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
import site.addzero.aio.agent.model.ModelListRequest
import site.addzero.aio.agent.model.ProviderDraft
import site.addzero.aio.agent.transport.AgentClient
import site.addzero.aio.agent.workspace.*

@Composable
internal fun AgentDialogs(state: AgentState) {
    val dialog = state.dialog ?: return
    val dismiss = { if (!state.busy) state.dialog = null }
    when (dialog) {
        AgentDialog.New -> {
            var title by remember { mutableStateOf("") }
            var provider by remember {
                mutableStateOf(state.settings.providers.firstOrNull()?.id.orEmpty())
            }
            var spaceId by remember {
                mutableStateOf(state.spaces.firstOrNull { it.personal }?.id.orEmpty())
            }
            AlertDialog(
                dismiss,
                title = { Text("新建会话") },
                text = {
                    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        OutlinedTextField(
                            title,
                            { title = it },
                            label = { Text("标题") },
                            singleLine = true,
                            modifier = Modifier.fillMaxWidth(),
                        )
                        Choice(
                            "模型",
                            listOf("" to "暂不使用模型") +
                                state.settings.providers.map { it.id to it.label },
                            provider,
                            { provider = it },
                        )
                        if (state.spaces.isNotEmpty())
                            Choice(
                                "记忆空间",
                                state.spaces
                                    .filter { it.role != "READER" }
                                    .map { it.id to it.title },
                                spaceId,
                                { spaceId = it },
                            )
                        Feedback(state)
                    }
                },
                confirmButton = {
                    TextButton(
                        { state.create(provider, title.trim(), spaceId.ifEmpty { null }) },
                        enabled = !state.busy && title.isNotBlank(),
                    ) {
                        Text("创建")
                    }
                },
                dismissButton = { TextButton(dismiss) { Text("取消") } },
            )
        }
        AgentDialog.Settings ->
            AlertDialog(
                dismiss,
                title = { Text("模型设置") },
                text = {
                    Column(
                        Modifier.fillMaxWidth().verticalScroll(rememberScrollState()),
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        if (state.settings.providers.isEmpty()) Text("尚未配置模型")
                        state.settings.providers.forEach { provider ->
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Column(Modifier.weight(1f)) {
                                    Text(provider.label)
                                    Text(
                                        provider.model,
                                        style = MaterialTheme.typography.labelSmall,
                                    )
                                }
                                Tool("编辑 ${provider.label}", Icons.Default.Edit, !state.busy) {
                                    state.dialog = AgentDialog.ProviderEditor(provider)
                                }
                                Tool("删除 ${provider.label}", Icons.Default.Delete, !state.busy) {
                                    state.dialog =
                                        AgentDialog.Delete(
                                            provider.label,
                                            "/providers/${provider.id}",
                                        )
                                }
                            }
                        }
                        OutlinedButton(
                            { state.dialog = AgentDialog.ProviderEditor() },
                            enabled = state.settings.allowedEndpoints.isNotEmpty(),
                        ) {
                            Icon(Icons.Default.Add, null)
                            Text("添加模型")
                        }
                        if (state.settings.allowedEndpoints.isEmpty())
                            Text("宿主尚未授权模型地址", color = MaterialTheme.colorScheme.error)
                        Feedback(state)
                    }
                },
                confirmButton = { TextButton(dismiss) { Text("关闭") } },
            )
        is AgentDialog.ProviderEditor -> {
            var model by remember(dialog) { mutableStateOf(dialog.provider?.model.orEmpty()) }
            var endpoint by
                remember(dialog) {
                    mutableStateOf(
                        dialog.provider?.endpoint
                            ?: state.settings.allowedEndpoints.firstOrNull().orEmpty()
                    )
                }
            var secret by remember(dialog) { mutableStateOf("") }
            var clearSecret by remember(dialog) { mutableStateOf(false) }
            var models by remember(dialog) { mutableStateOf<List<String>>(emptyList()) }
            var loading by remember(dialog) { mutableStateOf(false) }
            var listError by remember(dialog) { mutableStateOf<String?>(null) }
            var refresh by remember(dialog) { mutableStateOf(0) }
            LaunchedEffect(endpoint, secret, clearSecret, refresh) {
                models = emptyList()
                listError = null
                loading = endpoint.isNotBlank()
                if (!loading) return@LaunchedEffect
                try {
                    delay(500)
                    models = AgentClient.models(
                        ModelListRequest(
                            endpoint = endpoint.trim().trimEnd('/'),
                            providerId = dialog.provider?.id,
                            secret = if (clearSecret) "" else secret.ifEmpty { null },
                        )
                    )
                    if (models.isEmpty()) listError = "服务未返回可用模型"
                } catch (c: CancellationException) {
                    throw c
                } catch (c: Throwable) {
                    listError = c.message?.take(200) ?: "获取模型列表失败"
                } finally {
                    loading = false
                }
            }
            AlertDialog(
                dismiss,
                title = { Text(if (dialog.provider == null) "添加模型" else "编辑模型") },
                text = {
                    Column(
                        Modifier.fillMaxWidth().verticalScroll(rememberScrollState()),
                        verticalArrangement = Arrangement.spacedBy(12.dp),
                    ) {
                        OutlinedTextField(
                            endpoint,
                            {
                                endpoint = it
                                model = ""
                            },
                            label = { Text("服务地址") },
                            placeholder = { Text("https://api.example.com/v1") },
                            singleLine = true,
                            modifier = Modifier.fillMaxWidth().semantics { contentDescription = "服务地址" },
                        )
                        OutlinedTextField(
                            secret,
                            { secret = it },
                            label = {
                                Text(
                                    if (dialog.provider?.hasSecret == true) "新密钥（留空保留）"
                                    else "API 密钥"
                                )
                            },
                            singleLine = true,
                            visualTransformation = PasswordVisualTransformation(),
                            modifier = Modifier.fillMaxWidth().semantics { contentDescription = "API 密钥" },
                            enabled = !clearSecret,
                        )
                        if (dialog.provider?.hasSecret == true)
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Checkbox(clearSecret, { clearSecret = it })
                                Text("移除已保存密钥")
                            }
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Box(Modifier.weight(1f)) {
                                Choice(
                                    "模型",
                                    models.map { it to it },
                                    model,
                                    { model = it },
                                    enabled = !loading && models.isNotEmpty(),
                                )
                            }
                            Tool(
                                "刷新模型列表",
                                Icons.Default.Refresh,
                                !loading && endpoint.isNotBlank(),
                            ) {
                                refresh++
                            }
                        }
                        if (loading) LinearProgressIndicator(Modifier.fillMaxWidth())
                        listError?.let { Text(it, color = MaterialTheme.colorScheme.error) }
                        Feedback(state)
                    }
                },
                confirmButton = {
                    TextButton(
                        {
                            state.saveProvider(
                                dialog.provider?.id,
                                ProviderDraft(
                                    endpoint = endpoint.trim().trimEnd('/'),
                                    label = "",
                                    model = model,
                                    secret = if (clearSecret) "" else secret.ifEmpty { null },
                                ),
                            )
                        },
                        enabled =
                            !state.busy &&
                                model.isNotBlank() &&
                                endpoint.isNotBlank(),
                    ) {
                        Text("保存")
                    }
                },
                dismissButton = { TextButton(dismiss) { Text("取消") } },
            )
        }
        is AgentDialog.Source -> SourceDialog(state, dialog.source, dismiss)
        is AgentDialog.Entry -> EntryDialog(state, dialog, dismiss)
        AgentDialog.Spaces -> SpaceDialog(state, dismiss)
        is AgentDialog.Delete ->
            AlertDialog(
                dismiss,
                title = { Text("确认删除") },
                text = {
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Text(dialog.title)
                        Feedback(state)
                    }
                },
                confirmButton = {
                    TextButton({ state.remove(dialog) }, enabled = !state.busy) { Text("确认删除") }
                },
                dismissButton = { TextButton(dismiss) { Text("取消") } },
            )
    }
}

@Composable
private fun Feedback(state: AgentState) {
    state.error?.let {
        Text(
            it,
            color = MaterialTheme.colorScheme.error,
            style = MaterialTheme.typography.bodySmall,
        )
    }
}

@Composable
internal fun Choice(
    label: String,
    items: List<Pair<String, String>>,
    value: String,
    onChange: (String) -> Unit,
    enabled: Boolean = true,
) {
    var expanded by remember { mutableStateOf(false) }
    Column {
        Text(label, style = MaterialTheme.typography.labelMedium)
        Box {
            OutlinedButton({ expanded = true }, Modifier.fillMaxWidth(), enabled = enabled) {
                Text(
                    items.firstOrNull { it.first == value }?.second ?: value.ifEmpty { "未选择" },
                    Modifier.weight(1f),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Icon(Icons.Default.ArrowDropDown, null)
            }
            DropdownMenu(
                expanded && enabled,
                { expanded = false },
                modifier = Modifier.heightIn(max = 320.dp),
            ) {
                items.forEach { item ->
                    DropdownMenuItem(
                        text = { Text(item.second, maxLines = 2, overflow = TextOverflow.Ellipsis) },
                        onClick = {
                            onChange(item.first)
                            expanded = false
                        },
                    )
                }
            }
        }
    }
}
