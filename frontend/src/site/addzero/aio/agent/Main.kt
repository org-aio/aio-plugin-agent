package site.addzero.aio.agent

import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.window.ComposeViewport
import site.addzero.aio.agent.theme.AgentTheme
import site.addzero.aio.agent.workspace.AgentScreen

@OptIn(ExperimentalComposeUiApi::class)
fun main() {
    ComposeViewport { AgentTheme { AgentScreen() } }
}
