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
 * inside the zip — so the set is copied once into `noBackupFilesDir/fonts/`
 * and the core is handed that. The copy is the whole `assets/fonts/`
 * directory, not a list of names: which faces exist (the CJK `.ttc` pair
 * included) is the core's business, and a name list here would go stale.
 *
 * Under `noBackupFilesDir`, not `filesDir`, because the set is ~100 MB of
 * bytes reproduced verbatim from the APK. Auto Backup covers `filesDir`,
 * so parking it there would push a duplicate of every face into cloud
 * backup and device transfer, and an over-quota payload fails Auto Backup
 * for the whole app — taking the library database's backup down with it.
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
        deleteLegacyCopy(app)
        val target = File(app.noBackupFilesDir, DIRECTORY)
        // `assets.list` is the manifest of what shipped; the directory is
        // flat by construction, so a name is always a file to open.
        val names = app.assets.list(ASSET_DIRECTORY).orEmpty().sorted()
        if (names.isEmpty()) {
            throw IOException("No fonts bundled under assets/$ASSET_DIRECTORY")
        }

        // What the marker promises: the extracted copy was written by a
        // build carrying this version code and exactly these face names, so
        // any released build that adds, drops or renames a face — or that
        // ships under a new version code — re-extracts.
        //
        // What it does not promise: that the bytes match. A face whose
        // content changes under an unchanged name in a build whose version
        // code did not move — a debug rebuild reinstalled over its own data
        // — produces an identical marker and the previous copy is kept.
        // Wiping the app's data (or bumping the version code) is the fix
        // when iterating on the font set itself; hashing ~100 MB on every
        // cold open is not worth paying for that case.
        val marker = (listOf(BuildConfig.VERSION_CODE.toString()) + names).joinToString("\n")
        val markerFile = File(target, MARKER)
        if (markerFile.isFile && runCatching { markerFile.readText() }.getOrNull() == marker) {
            return target
        }

        val staging = File(app.noBackupFilesDir, STAGING)
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

    /**
     * Removes the copy earlier builds left under `filesDir`.
     *
     * A device that ran one of those builds would otherwise keep ~100 MB
     * stranded in a backed-up directory forever. Best-effort: a failure
     * here only means the stale copy survives another launch, which must
     * not fail the open.
     */
    private fun deleteLegacyCopy(app: Context) {
        for (name in listOf(DIRECTORY, STAGING)) {
            val legacy = File(app.filesDir, name)
            if (legacy.exists()) {
                runCatching { legacy.deleteRecursively() }
            }
        }
    }

    /** Where the fonts live inside the APK. */
    private const val ASSET_DIRECTORY = "fonts"

    /** Where they live once extracted, under `noBackupFilesDir`. */
    private const val DIRECTORY = "fonts"

    /** Half-written copies land here and are renamed into place. */
    private const val STAGING = "fonts.tmp"

    /** Names the extracted set, so a stale one is spotted without hashing. */
    private const val MARKER = ".version"
}
