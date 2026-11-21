package org.lectio.android.ui.theme

import android.os.Build
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext

/** The icon's indigo, and a lighter one that keeps its contrast on dark backgrounds (as on iOS). */
val LectioIndigo = Color(0xFF4F46E5)
val LectioIndigoOnDark = Color(0xFFA59CFA)

/**
 * Material You (wallpaper colours, Android 12+) for surfaces and the quieter
 * accents, with Lectio's indigo kept as the primary colour, so buttons and
 * the record/play controls look the same on every phone. Text sizes come from
 * Material's type scale in sp, so they follow the system font size.
 */
@Composable
fun LectioTheme(dark: Boolean = isSystemInDarkTheme(), content: @Composable () -> Unit) {
    val context = LocalContext.current
    val base: ColorScheme = when {
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.S && dark -> dynamicDarkColorScheme(context)
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.S -> dynamicLightColorScheme(context)
        dark -> darkColorScheme()
        else -> lightColorScheme()
    }
    val scheme = if (dark) {
        base.copy(primary = LectioIndigoOnDark, onPrimary = Color(0xFF1B1452), primaryContainer = Color(0xFF3730A3), onPrimaryContainer = Color(0xFFE0DDFF))
    } else {
        base.copy(primary = LectioIndigo, onPrimary = Color.White, primaryContainer = Color(0xFFE0DDFF), onPrimaryContainer = Color(0xFF1B1452))
    }
    MaterialTheme(colorScheme = scheme, content = content)
}
