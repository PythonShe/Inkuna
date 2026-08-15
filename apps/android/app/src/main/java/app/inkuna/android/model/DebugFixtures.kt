package app.inkuna.android.model

import android.content.Context
import android.content.pm.ApplicationInfo
import android.util.Log
import java.io.File

/**
 * Debug-only import path. No import UI has shipped yet, so on debuggable
 * builds any EPUB dropped into `files/debug-import/` (adb push + `run-as
 * app.inkuna.android`) is swept into the core library at the next launch.
 * Release builds never look; core import is content-hash idempotent, so a
 * repeated sweep is harmless.
 *
 * TODO(core): delete once the real import UI (document picker) lands.
 */
object DebugFixtures {
    private const val TAG = "InkunaDebugFixtures"

    suspend fun importPending(context: Context) {
        val app = context.applicationContext
        val debuggable = (app.applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE) != 0
        if (!debuggable) return
        // Outside books/ and covers/, so the core's orphaned-file sweep
        // never touches the drop directory.
        val pending = File(app.filesDir, "debug-import")
            .listFiles { file -> file.isFile && !file.name.startsWith(".") }
            .orEmpty()
        if (pending.isEmpty()) return
        val bookshelf = LibraryStore.bookshelf(app)
        for (file in pending) {
            runCatching { bookshelf.import(file.absolutePath) }
                .onSuccess { file.delete() }
                .onFailure { Log.w(TAG, "fixture import failed: ${file.name}", it) }
        }
    }
}
