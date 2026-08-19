package app.inkuna.android.reminder

import android.Manifest
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import androidx.work.Data
import androidx.work.ExistingWorkPolicy
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.Worker
import androidx.work.WorkerParameters
import app.inkuna.android.MainActivity
import app.inkuna.android.R
import app.inkuna.android.model.AppSettings
import java.time.Duration
import java.time.LocalTime
import java.time.ZoneId
import java.time.ZonedDateTime

/**
 * The daily evening reading reminder — one quiet notification at the
 * reading hour. The on/off choice lives in the core's settings record;
 * this owns only the platform scheduling.
 *
 * A self-chaining one-time worker instead of periodic work: WorkManager's
 * periodic interval drifts from run time, while re-enqueueing against the
 * next reading hour keeps the nudge at the hour it promises. WorkManager
 * persists the chain across reboots, so no boot receiver is needed.
 */
object EveningReminder {
    private const val WORK_NAME = "evening-reminder"
    private const val CHANNEL_ID = "reading-reminders"
    private const val NOTIFICATION_ID = 1

    /** The chosen reading hour rides the work request, so the chain's
     *  re-enqueue keeps it without reopening the settings store from the
     *  worker thread. */
    internal const val KEY_MINUTES = "minutes"

    /** Schedules (or, under REPLACE, re-anchors) the next nudge at
     *  [minutes] after local midnight. */
    fun schedule(context: Context, minutes: Int) {
        val readingHour = LocalTime.of(
            (minutes / 60).coerceIn(0, 23),
            (minutes % 60).coerceIn(0, 59),
        )
        // Zoned instants, not LocalDateTime: the initial delay is elapsed
        // time, so wall-clock arithmetic would land the nudge an hour off
        // across a DST transition and stay anchored to a left timezone.
        val now = ZonedDateTime.now(ZoneId.systemDefault())
        var next = now.with(readingHour)
        if (!next.isAfter(now)) next = next.plusDays(1)
        val request = OneTimeWorkRequestBuilder<EveningReminderWorker>()
            .setInitialDelay(Duration.between(now, next))
            .setInputData(Data.Builder().putInt(KEY_MINUTES, minutes).build())
            .build()
        WorkManager.getInstance(context)
            .enqueueUniqueWork(WORK_NAME, ExistingWorkPolicy.REPLACE, request)
    }

    fun cancel(context: Context) {
        WorkManager.getInstance(context).cancelUniqueWork(WORK_NAME)
    }

    internal fun postNotification(context: Context) {
        // The permission can be revoked after scheduling; a silent skip
        // matches a nudge's manners better than a crash.
        val granted = context.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) ==
            PackageManager.PERMISSION_GRANTED
        if (!granted) return

        val manager = context.getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel(
                CHANNEL_ID,
                context.getString(R.string.settings_notif_channel),
                // Low importance: a banner-less, sound-less entry — the
                // design promises "a quiet nudge", not an interruption.
                NotificationManager.IMPORTANCE_LOW,
            ),
        )
        val notification = Notification.Builder(context, CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(context.getString(R.string.settings_reminder_notif_title))
            .setContentText(context.getString(R.string.settings_reminder_notif_body))
            .setContentIntent(
                android.app.PendingIntent.getActivity(
                    context,
                    0,
                    // Explicit over getLaunchIntentForPackage: that lookup is
                    // nullable and costs a package-manager round trip on the
                    // worker thread.
                    Intent(context, MainActivity::class.java).apply {
                        flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP
                    },
                    android.app.PendingIntent.FLAG_IMMUTABLE,
                ),
            )
            .setAutoCancel(true)
            .build()
        manager.notify(NOTIFICATION_ID, notification)
    }
}

/** Posts tonight's nudge, then enqueues tomorrow's. */
class EveningReminderWorker(
    context: Context,
    parameters: WorkerParameters,
) : Worker(context, parameters) {
    override fun doWork(): Result {
        // cancelUniqueWork only signals a running worker; without these
        // guards a toggle-off landing mid-run would still post tonight's
        // nudge and re-arm tomorrow's chain against the stored "off".
        if (isStopped) return Result.success()
        EveningReminder.postNotification(applicationContext)
        val minutes = inputData.getInt(
            EveningReminder.KEY_MINUTES,
            AppSettings.DEFAULT_REMINDER_MINUTES,
        )
        if (!isStopped) EveningReminder.schedule(applicationContext, minutes)
        return Result.success()
    }
}
