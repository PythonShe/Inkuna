package app.inkuna.android.update

import org.json.JSONArray
import java.net.HttpURLConnection
import java.net.URL

/** What a completed update check found. */
sealed interface UpdateCheck {
    data object UpToDate : UpdateCheck

    /** A newer APK exists; [url] is its GitHub release page. */
    data class Available(val versionName: String, val url: String) : UpdateCheck
}

/**
 * Android-only in-app update check against the GitHub releases feed. iOS
 * has no counterpart — it updates through the App Store.
 *
 * The repository hosts both platforms' releases, so only `android-v` tags
 * count, and test builds are published as prereleases — which is why this
 * reads the release list rather than `/releases/latest`.
 */
object UpdateChecker {
    private const val RELEASES_URL =
        "https://api.github.com/repos/PythonShe/Inkuna/releases?per_page=30"

    /**
     * Blocking; call on a background dispatcher. Throws on network or
     * parse failure — the caller owns the "couldn't check" rendering.
     *
     * Tags are `android-vX.Y.Z+N` where N is the versionCode the release
     * shipped with; the newest Android entry decides the answer.
     */
    fun check(currentVersionCode: Long): UpdateCheck {
        val connection = URL(RELEASES_URL).openConnection() as HttpURLConnection
        val body = try {
            connection.connectTimeout = 10_000
            connection.readTimeout = 10_000
            connection.setRequestProperty("Accept", "application/vnd.github+json")
            connection.inputStream.bufferedReader().use { it.readText() }
        } finally {
            connection.disconnect()
        }

        val releases = JSONArray(body)
        for (i in 0 until releases.length()) {
            val release = releases.getJSONObject(i)
            if (release.optBoolean("draft")) continue
            val tag = release.optString("tag_name")
            if (!tag.startsWith("android-v")) continue
            val build = tag.substringAfter('+', "").toLongOrNull() ?: continue
            return if (build > currentVersionCode) {
                UpdateCheck.Available(
                    versionName = tag.removePrefix("android-v").substringBefore('+'),
                    url = release.optString("html_url"),
                )
            } else {
                UpdateCheck.UpToDate
            }
        }
        return UpdateCheck.UpToDate
    }
}
