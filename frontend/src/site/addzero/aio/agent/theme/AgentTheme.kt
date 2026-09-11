package site.addzero.aio.agent.theme

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

private fun typography(font: FontFamily?): Typography {
    val b = Typography()
    return Typography(
        displayLarge = b.displayLarge.copy(fontFamily = font, letterSpacing = 0.sp),
        displayMedium = b.displayMedium.copy(fontFamily = font, letterSpacing = 0.sp),
        displaySmall = b.displaySmall.copy(fontFamily = font, letterSpacing = 0.sp),
        headlineLarge = b.headlineLarge.copy(fontFamily = font, letterSpacing = 0.sp),
        headlineMedium = b.headlineMedium.copy(fontFamily = font, letterSpacing = 0.sp),
        headlineSmall = b.headlineSmall.copy(fontFamily = font, letterSpacing = 0.sp),
        titleLarge = b.titleLarge.copy(fontFamily = font, letterSpacing = 0.sp),
        titleMedium = b.titleMedium.copy(fontFamily = font, letterSpacing = 0.sp),
        titleSmall = b.titleSmall.copy(fontFamily = font, letterSpacing = 0.sp),
        bodyLarge = b.bodyLarge.copy(fontFamily = font, letterSpacing = 0.sp),
        bodyMedium = b.bodyMedium.copy(fontFamily = font, letterSpacing = 0.sp),
        bodySmall = b.bodySmall.copy(fontFamily = font, letterSpacing = 0.sp),
        labelLarge = b.labelLarge.copy(fontFamily = font, letterSpacing = 0.sp),
        labelMedium = b.labelMedium.copy(fontFamily = font, letterSpacing = 0.sp),
        labelSmall = b.labelSmall.copy(fontFamily = font, letterSpacing = 0.sp),
    )
}

@Composable
internal fun AgentTheme(content: @Composable () -> Unit) {
    val fonts = rememberAgentFont()
    MaterialTheme(
        colorScheme =
            lightColorScheme(
                primary = Color(0xFF267868),
                secondary = Color(0xFF536CAD),
                tertiary = Color(0xFFAB426D),
                surface = Color(0xFFFCFDFD),
                background = Color(0xFFFCFDFD),
                onSurface = Color(0xFF262C2D),
                secondaryContainer = Color(0xFFEAF0FA),
                surfaceContainerHigh = Color(0xFFF2F4F5),
                surfaceContainer = Color(0xFFF2F4F5),
                outlineVariant = Color(0xFFDDE2E6),
            ),
        typography = remember(fonts.font) { typography(fonts.font) },
        shapes =
            Shapes(
                small = RoundedCornerShape(4.dp),
                medium = RoundedCornerShape(8.dp),
                large = RoundedCornerShape(8.dp),
                extraLarge = RoundedCornerShape(8.dp),
            ),
    ) {
        Surface(Modifier.fillMaxSize()) {
            if (fonts.font != null) content()
            else
                Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                    if (fonts.failed)
                        Column(horizontalAlignment = Alignment.CenterHorizontally) {
                            Text("Font unavailable")
                            Button({ fonts.attempt++ }) { Text("Retry") }
                        }
                    else CircularProgressIndicator()
                }
        }
    }
}
