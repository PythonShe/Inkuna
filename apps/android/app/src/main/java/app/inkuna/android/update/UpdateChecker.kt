package app.inkuna.android.update

import org.json.JSONArray
import java.net.HttpURLConnection
import java.net.URL

/** What a completed update check found. */
sealed interface UpdateCheck {
    data object UpToDate : UpdateCheck

    /**
     * A newer APK exists; [url] is its GitHub release page and [apkUrl]
     * the release's APK asset for the in-app install — null when the
     * release carries no APK, leaving the browser page as the fallback.
     */
    data class Available(
        val versionName: String,
        val url: String,
        val apkUrl: String?,
    ) : UpdateCheck
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
     * shipped with; the highest build among stable (non-draft,
     * non-prerelease) Android entries decides the answer, so a malformed
     * or out-of-order tag in the feed cannot shadow the real newest.
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
        var bestBuild = Long.MIN_VALUE
        var bestVersionName = ""
        var bestUrl = ""
        var bestApkUrl: String? = null
        for (i in 0 until releases.length()) {
            val release = releases.getJSONObject(i)
            // Prereleases are the test channel (see the class comment);
            // stable users must never be offered one.
            if (release.optBoolean("draft") || release.optBoolean("prerelease")) continue
            val tag = release.optString("tag_name")
            if (!tag.startsWith("android-v")) continue
            val build = tag.substringAfter('+', "").toLongOrNull() ?: continue
            // No page to open means nothing to offer.
            val url = release.optString("html_url")
            if (url.isEmpty()) continue
            if (build > bestBuild) {
                bestBuild = build
                bestVersionName = tag.removePrefix("android-v").substringBefore('+')
                bestUrl = url
                bestApkUrl = apkAssetUrl(release.optJSONArray("assets"))
            }
        }
        return if (bestBuild > currentVersionCode) {
            UpdateCheck.Available(
                versionName = bestVersionName,
                url = bestUrl,
                apkUrl = bestApkUrl,
            )
        } else {
            UpdateCheck.UpToDate
        }
    }

    /** The release's APK download URL, or null when no asset is one. */
    private fun apkAssetUrl(assets: JSONArray?): String? {
        if (assets == null) return null
        for (i in 0 until assets.length()) {
            val asset = assets.getJSONObject(i)
            if (!asset.optString("name").endsWith(".apk")) continue
            val url = asset.optString("browser_download_url")
            if (url.isNotEmpty()) return url
        }
        return null
    }
}
