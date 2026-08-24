package app.inkuna.android.model

import android.graphics.fonts.FontStyle
import android.graphics.fonts.SystemFonts
import app.inkuna.core.SystemFontFace
import app.inkuna.core.SystemFontRole

/**
 * Resolves the platform reading faces — Noto Serif for `system-serif`,
 * Roboto for `system-sans` — down to the font files the engine can shape
 * with, from [SystemFonts.getAvailableFonts].
 *
 * Discovery is best-effort by design: a face without a readable file is
 * silently skipped, and the engine substitutes the bundled Noto for that
 * role at selection time. Nothing here throws.
 */
object SystemReadingFonts {

    /**
     * The regular/bold × upright/italic grid for both roles. The platform
     * exposes no family names here, so the family gate is the well-known
     * file names; weight and slant come from each font's declared
     * [FontStyle], never from the name. A variable file enumerated once
     * per named instance collapses to one entry — the core instances
     * weights from its `wght` axis itself.
     */
    fun discover(): List<SystemFontFace> {
        val faces = mutableListOf<SystemFontFace>()
        val seen = mutableSetOf<Triple<String, Int, Boolean>>()
        for (font in SystemFonts.getAvailableFonts()) {
            val file = font.file ?: continue
            val role = roleFor(file.name) ?: continue
            val weight = font.style.weight
            if (weight != FontStyle.FONT_WEIGHT_NORMAL && weight != FontStyle.FONT_WEIGHT_BOLD) continue
            if (!file.canRead()) continue
            val italic = font.style.slant == FontStyle.FONT_SLANT_ITALIC
            if (!seen.add(Triple(file.absolutePath, font.ttcIndex, italic))) continue
            faces += SystemFontFace(
                role = role,
                italic = italic,
                weight = weight.toUShort(),
                filePath = file.absolutePath,
                // The engine opens the face by its collection index; the
                // name scan is iOS's path, where no index is exposed.
                postScriptName = null,
                ttcHint = font.ttcIndex.toUInt(),
            )
        }
        return faces
    }

    /**
     * Latin core families only. The per-script Notos (NotoSerifThai-…,
     * NotoSansCJK-…) belong to the platform's fallback chain, not to the
     * reading role, so the patterns anchor the whole base name.
     */
    private fun roleFor(fileName: String): SystemFontRole? = when {
        SERIF_FILE.matches(fileName) -> SystemFontRole.SERIF
        SANS_FILE.matches(fileName) -> SystemFontRole.SANS
        else -> null
    }

    private val SERIF_FILE = Regex("NotoSerif-\\w+\\.(ttf|otf)", RegexOption.IGNORE_CASE)
    private val SANS_FILE = Regex("Roboto-\\w+\\.(ttf|otf)", RegexOption.IGNORE_CASE)
}
