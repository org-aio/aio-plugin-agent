package site.addzero.aio.agent.workspace

import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.withLink
import site.addzero.aio.agent.model.Message

private val memoryLink = Regex("\\[([^\\]\\r\\n]+)]\\(memory:([^\\s)]+)\\)")

@Composable
internal fun MessageBody(message: Message, enabled: Boolean, openEntry: (String) -> Unit) {
    val color = MaterialTheme.colorScheme.primary
    val content = remember(
        message.content, message.citations, message.role, message.status, enabled, color, openEntry,
    ) {
        buildAnnotatedString {
            val text = message.content.ifEmpty { if (message.status == "generating") "…" else "" }
            val citations = message.citations.associateBy { it.id }
            var offset = 0
            if (message.role == "assistant") memoryLink.findAll(text).forEach { match ->
                append(text.substring(offset, match.range.first))
                val citation = citations[match.groupValues[2]]
                // 模型生成的地址不作为权限依据，只打开服务端返回的可见引用。
                if (citation != null && enabled) {
                    withLink(
                        LinkAnnotation.Clickable(
                            citation.id,
                            TextLinkStyles(
                                SpanStyle(color = color, textDecoration = TextDecoration.Underline)
                            ),
                        ) { openEntry(citation.id) }
                    ) { append(citation.title) }
                } else {
                    append(citation?.title ?: "${match.groupValues[1]}（来源不可用）")
                }
                offset = match.range.last + 1
            }
            append(text.substring(offset))
        }
    }
    SelectionContainer { Text(content, style = MaterialTheme.typography.bodyLarge) }
}
