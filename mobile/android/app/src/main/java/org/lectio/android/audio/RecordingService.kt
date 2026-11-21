package org.lectio.android.audio

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import org.lectio.android.LectioApplication
import org.lectio.android.MainActivity
import org.lectio.android.R

/**
 * Keeps a recording going with the screen off: a foreground service of type
 * "microphone", with an ongoing notification (elapsed time and a Stop button)
 * and a partial wake lock. The recording itself is [RecorderController]'s;
 * this service only keeps the process allowed to use the microphone.
 */
class RecordingService : Service() {
    private var wakeLock: PowerManager.WakeLock? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_STOP) {
            (application as LectioApplication).graph?.recorder?.stop()
            return START_NOT_STICKY
        }
        val title = intent?.getStringExtra(EXTRA_TITLE).orEmpty()
        val type = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE else 0
        ServiceCompat.startForeground(this, NOTIFICATION_ID, notification(title), type)
        if (wakeLock == null) {
            wakeLock = getSystemService(PowerManager::class.java)
                .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "Lectio:recording")
                .apply { acquire(MAX_RECORDING_MS) }
        }
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        wakeLock?.takeIf { it.isHeld }?.release()
        wakeLock = null
        super.onDestroy()
    }

    private fun notification(title: String) = NotificationCompat.Builder(this, CHANNEL)
        .setSmallIcon(R.drawable.ic_stat_recording)
        .setContentTitle(getString(R.string.recording_notification_title))
        .setContentText(title)
        .setOngoing(true)
        .setUsesChronometer(true)
        .setWhen(System.currentTimeMillis())
        .setCategory(NotificationCompat.CATEGORY_SERVICE)
        .setForegroundServiceBehavior(NotificationCompat.FOREGROUND_SERVICE_IMMEDIATE)
        .setContentIntent(
            PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE),
        )
        .addAction(
            0,
            getString(R.string.recording_notification_stop),
            PendingIntent.getService(this, 1, Intent(this, RecordingService::class.java).setAction(ACTION_STOP), PendingIntent.FLAG_IMMUTABLE),
        )
        .build()
        .also { ensureChannel() }

    private fun ensureChannel() {
        val channel = NotificationChannel(CHANNEL, getString(R.string.recording_channel), NotificationManager.IMPORTANCE_LOW)
            .apply { description = getString(R.string.recording_channel_description) }
        getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
    }

    companion object {
        private const val CHANNEL = "recording"
        private const val NOTIFICATION_ID = 1
        private const val ACTION_STOP = "org.lectio.android.STOP_RECORDING"
        private const val EXTRA_TITLE = "title"
        /** A safety limit for the wake lock: longer than any lecture. */
        private const val MAX_RECORDING_MS = 6L * 60 * 60 * 1000

        fun start(context: Context, title: String) {
            ContextCompat.startForegroundService(context, Intent(context, RecordingService::class.java).putExtra(EXTRA_TITLE, title))
        }

        fun stop(context: Context) {
            context.stopService(Intent(context, RecordingService::class.java))
        }
    }
}
