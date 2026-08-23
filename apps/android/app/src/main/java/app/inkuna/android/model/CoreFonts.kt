package app.inkuna.android.model

import android.content.Context
import app.inkuna.android.BuildConfig
import java.io.File
import java.io.IOException

/**
 * The bundled font set, unpacked where the core can open it.
 *
 * The reader engine shapes text with real files: it is given a directory
 * path and reads the faces itself. APK assets are not files — they live
 * inside the zip — so the set is copied once into `filesDir/fonts/` and the
 * core is handed that. The copy is the whole `assets/fonts/` directory, not
 * a list of names: which faces exist (the CJK `.ttc` pair included) is the
 * core's business, and a name list here would go stale.
 *
 * iOS needs none of this — a bundle resource directory already is a real
 * path — so this file has no Swift sibling.
 */
object CoreFonts {

    /**
     * Extracts the bundled fonts if they are not already current and
     * returns the directory holding them.
     *
     * Blocking: it copies tens of megabytes on a cold install. Callers are
     * expected to already be off the main thread ([LibraryStore] opens on
     * [kotlinx.coroutines.Dispatchers.IO]).
     *
     * Throws [IOException] if the copy cannot be completed. Nothing partial
     * is ever left in place to be handed to the core: the copy lands in a
     * staging directory and is renamed over the live one only once the
     * marker naming its contents has been written, so an interrupted copy
     * simply fails the marker check on the next call and is redone.
     */
    @Throws(IOException::class)
    fun ensureExtracted(context: Context): File {
        val app = context.applicationContext
        val target = File(app.filesDir, DIRECTORY)
        // `assets.list` is the manifest of what shipped; the directory is
        // flat by construction, so a name is always a file to open.
        val names = app.assets.list(ASSET_DIRECTORY).orEmpty().sorted()
        if (names.isEmpty()) {
            throw IOException("No fonts bundled under assets/$ASSET_DIRECTORY")
        }

        // The version code alone would miss a debug rebuild that changed
        // the font set without bumping it; the name list alone would miss a
        // face whose bytes changed under an unchanged name. Together they
        // cover both, and a match means the extracted copy is this build's.
        val marker = (listOf(BuildConfig.VERSION_CODE.toString()) + names).joinToString("\n")
        val markerFile = File(target, MARKER)
        if (markerFile.isFile && runCatching { markerFile.readText() }.getOrNull() == marker) {
            return target
        }

        val staging = File(app.filesDir, STAGING)
        staging.deleteRecursively()
        if (!staging.mkdirs()) {
            throw IOException("Could not create ${staging.absolutePath}")
        }
        for (name in names) {
            app.assets.open("$ASSET_DIRECTORY/$name").use { source ->
                File(staging, name).outputStream().use(source::copyTo)
            }
        }
        // Written last: the marker is what promises the directory is whole.
        File(staging, MARKER).writeText(marker)

        target.deleteRecursively()
        if (!staging.renameTo(target)) {
            staging.deleteRecursively()
            throw IOException("Could not install fonts into ${target.absolutePath}")
        }
        return target
    }

    /** Where the fonts live inside the APK. */
    private const val ASSET_DIRECTORY = "fonts"

    /** Where they live once extracted, under `filesDir`. */
    private const val DIRECTORY = "fonts"

    /** Half-written copies land here and are renamed into place. */
    private const val STAGING = "fonts.tmp"

    /** Names the extracted set, so a stale one is spotted without hashing. */
    private const val MARKER = ".version"
}
