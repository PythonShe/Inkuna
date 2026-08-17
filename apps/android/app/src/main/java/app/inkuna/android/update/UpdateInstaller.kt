package app.inkuna.android.update

import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import android.content.pm.PackageManager
import kotlinx.coroutines.flow.MutableSharedFlow
import java.io.File
import java.net.HttpURLConnection
import java.net.URL

/**
 * The in-app half of the Android updater: downloads the release APK and
 * hands it to the platform [PackageInstaller] session API — the modern
 * install path (ACTION_VIEW on an APK is long deprecated). The system
 * confirms the install with the user (routing through the "install
 * unknown apps" grant on first use); once this app is the update's
 * installer of record, later updates may skip the confirmation entirely
 * via USER_ACTION_NOT_REQUIRED.
 */
object UpdateInstaller {
    /** What the committed install session reported back. */
    sealed interface Event {
        /** The user backed out of the system confirmation. */
        data object Aborted : Event

        data object Failed : Event
        // Success needs no event: the process is replaced by the update.
    }

    /**
     * Session outcomes from [InstallResultReceiver]; a SharedFlow because
     * the manifest-registered receiver cannot reach the ViewModel.
     */
    val events = MutableSharedFlow<Event>(extraBufferCapacity = 4)

    /**
     * Blocking; call on a background dispatcher. Streams [url] into the
     * app's cache and reports whole percents to [onProgress] (only when
     * the server declares a length). Throws on network failure — the
     * caller owns the "couldn't download" rendering.
     */
    fun download(context: Context, url: String, onProgress: (Int) -> Unit): File {
        val target = File(context.cacheDir, "update/inkuna-update.apk")
        target.parentFile?.mkdirs()
        val connection = URL(url).openConnection() as HttpURLConnection
        try {
            connection.connectTimeout = 10_000
            connection.readTimeout = 30_000
            // GitHub serves release assets through a redirect to its CDN;
            // HttpURLConnection follows same-protocol redirects itself.
            val total = connection.contentLengthLong
            connection.inputStream.use { input ->
                target.outputStream().use { output ->
                    val buffer = ByteArray(64 * 1024)
                    var copied = 0L
                    var lastPercent = -1
                    while (true) {
                        val read = input.read(buffer)
                        if (read < 0) break
                        output.write(buffer, 0, read)
                        copied += read
                        if (total > 0) {
                            val percent = ((copied * 100) / total).toInt()
                            if (percent != lastPercent) {
                                lastPercent = percent
                                onProgress(percent)
                            }
                        }
                    }
                }
            }
        } catch (failure: Exception) {
            target.delete()
            throw failure
        } finally {
            connection.disconnect()
        }
        return target
    }

    /**
     * Opens a full-install session over [apk], commits it, and deletes
     * the download — the session holds its own copy from then on. The
     * outcome arrives later through [events]; a system confirmation
     * dialog (when required) is launched by [InstallResultReceiver].
     */
    fun install(context: Context, apk: File) {
        val installer = context.packageManager.packageInstaller
        val params = PackageInstaller.SessionParams(
            PackageInstaller.SessionParams.MODE_FULL_INSTALL,
        ).apply {
            setAppPackageName(context.packageName)
            setInstallReason(PackageManager.INSTALL_REASON_USER)
            // A hint, not a demand: the system still confirms until this
            // app is the installer of record with the grant in hand.
            setRequireUserAction(
                PackageInstaller.SessionParams.USER_ACTION_NOT_REQUIRED,
            )
        }
        val sessionId = installer.createSession(params)
        try {
            installer.openSession(sessionId).use { session ->
                session.openWrite("inkuna-update.apk", 0, apk.length()).use { output ->
                    apk.inputStream().use { it.copyTo(output) }
                    session.fsync(output)
                }
                val fillIn = Intent(context, InstallResultReceiver::class.java)
                    .setPackage(context.packageName)
                // Mutable so the system can attach the status extras.
                val pending = PendingIntent.getBroadcast(
                    context,
                    sessionId,
                    fillIn,
                    PendingIntent.FLAG_MUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
                )
                session.commit(pending.intentSender)
            }
        } catch (failure: Exception) {
            installer.abandonSession(sessionId)
            throw failure
        } finally {
            apk.delete()
        }
    }
}
