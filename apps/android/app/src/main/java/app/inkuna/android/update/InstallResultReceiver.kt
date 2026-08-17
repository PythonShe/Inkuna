package app.inkuna.android.update

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller

/**
 * Receives the committed install session's status. The interesting case
 * is PENDING_USER_ACTION: the system hands back a confirmation activity
 * (which also walks the user through the "install unknown apps" grant on
 * first use) that must be launched from here. Success needs no handling —
 * the update replaces this process.
 */
class InstallResultReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        when (
            intent.getIntExtra(
                PackageInstaller.EXTRA_STATUS,
                PackageInstaller.STATUS_FAILURE,
            )
        ) {
            PackageInstaller.STATUS_PENDING_USER_ACTION -> {
                val confirm = intent.getParcelableExtra(
                    Intent.EXTRA_INTENT,
                    Intent::class.java,
                ) ?: return
                // A receiver has no activity context to launch from.
                confirm.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                context.startActivity(confirm)
            }
            PackageInstaller.STATUS_SUCCESS -> Unit
            PackageInstaller.STATUS_FAILURE_ABORTED ->
                UpdateInstaller.events.tryEmit(UpdateInstaller.Event.Aborted)
            else -> UpdateInstaller.events.tryEmit(UpdateInstaller.Event.Failed)
        }
    }
}
