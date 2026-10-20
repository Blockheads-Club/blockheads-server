package club.blockheads.hostservice

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.wifi.WifiManager
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import androidx.core.app.NotificationCompat

/**
 * Foreground service that keeps the process (and the in-process server thread) alive while the screen is off:
 * ongoing notification, partial wake lock and a high-performance Wi-Fi lock.
 */
class ServerService : Service() {
    private var wakeLock: PowerManager.WakeLock? = null
    private var wifiLock: WifiManager.WifiLock? = null
    private var address: String? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent == null || intent.action == ACTION_STOP) {
            // A null intent means Android restarted the service after killing the process: there is no server
            // thread any more, so do not post a notification or take locks for nothing.
            releaseLocks()
            stopForeground(STOP_FOREGROUND_REMOVE)
            stopSelf()
            return START_NOT_STICKY
        }
        if (intent.action == ACTION_COPY) {
            // Android 10+ only lets the foreground app read the clipboard, but any app may write it.
            address?.let {
                val cm = getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                cm.setPrimaryClip(ClipData.newPlainText("Blockheads Server", it))
            }
            return START_NOT_STICKY
        }
        if (intent.action == ACTION_REQUEST_STOP) {
            // Ask Rust to save and stop the server; it then stops this service. Without a handler there is no
            // server to stop, so just end the service.
            val handler = stopRequested
            if (handler != null) handler() else {
                releaseLocks()
                stopForeground(STOP_FOREGROUND_REMOVE)
                stopSelf()
            }
            return START_NOT_STICKY
        }
        val title = intent.getStringExtra(EXTRA_TITLE) ?: "Blockheads Server"
        val text = intent.getStringExtra(EXTRA_TEXT) ?: ""
        address = intent.getStringExtra(EXTRA_ADDRESS)
        val notification = buildNotification(intent, title, text)
        if (Build.VERSION.SDK_INT >= 34) {
            startForeground(NOTIFICATION_ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE)
        } else {
            startForeground(NOTIFICATION_ID, notification)
        }
        acquireLocks()
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        releaseLocks()
        super.onDestroy()
    }

    private fun acquireLocks() {
        if (wakeLock == null) {
            val pm = getSystemService(Context.POWER_SERVICE) as PowerManager
            wakeLock = pm.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "blockheads:server").apply {
                setReferenceCounted(false)
                acquire()
            }
        }
        if (wifiLock == null) {
            val wm = applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
            @Suppress("DEPRECATION")
            val mode = if (Build.VERSION.SDK_INT >= 29) {
                WifiManager.WIFI_MODE_FULL_LOW_LATENCY
            } else {
                WifiManager.WIFI_MODE_FULL_HIGH_PERF
            }
            wifiLock = wm.createWifiLock(mode, "blockheads:server").apply {
                setReferenceCounted(false)
                acquire()
            }
        }
    }

    private fun releaseLocks() {
        wakeLock?.let { if (it.isHeld) it.release() }
        wakeLock = null
        wifiLock?.let { if (it.isHeld) it.release() }
        wifiLock = null
    }

    private fun buildNotification(intent: Intent, title: String, text: String): Notification {
        ensureChannels(this)
        val open = launchIntent(this)
        val builder = NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle(title)
            .setContentText(text)
            .setSmallIcon(applicationInfo.icon)
            .setColor(0xFFFF7A45.toInt())
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setContentIntent(open)
        intent.getStringExtra(EXTRA_BIG_TEXT)?.takeIf { it.isNotEmpty() }?.let {
            builder.setStyle(NotificationCompat.BigTextStyle().bigText("$text\n$it"))
        }
        val startedAt = intent.getLongExtra(EXTRA_STARTED_AT, 0L)
        if (startedAt > 0L) builder.setWhen(startedAt).setShowWhen(true).setUsesChronometer(true)
        if (intent.getBooleanExtra(EXTRA_LOADING, false)) builder.setProgress(0, 0, true)
        builder.addAction(0, intent.getStringExtra(EXTRA_STOP_LABEL) ?: "Stop server", serviceAction(ACTION_REQUEST_STOP, 1))
        if (address != null) {
            builder.addAction(0, intent.getStringExtra(EXTRA_COPY_LABEL) ?: "Copy address", serviceAction(ACTION_COPY, 2))
        }
        builder.addAction(0, intent.getStringExtra(EXTRA_OPEN_LABEL) ?: "Console", open)
        return builder.build()
    }

    private fun serviceAction(action: String, code: Int): PendingIntent =
        PendingIntent.getService(
            this, code, Intent(this, ServerService::class.java).setAction(action),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )

    companion object {
        const val ACTION_STOP = "club.blockheads.hostservice.STOP"
        const val ACTION_REQUEST_STOP = "club.blockheads.hostservice.REQUEST_STOP"
        const val ACTION_COPY = "club.blockheads.hostservice.COPY"
        const val EXTRA_BIG_TEXT = "bigText"
        const val EXTRA_STARTED_AT = "startedAt"
        const val EXTRA_LOADING = "loading"
        const val EXTRA_ADDRESS = "address"
        const val EXTRA_STOP_LABEL = "stopLabel"
        const val EXTRA_COPY_LABEL = "copyLabel"
        const val EXTRA_OPEN_LABEL = "openLabel"
        const val EXTRA_TITLE = "title"
        const val EXTRA_TEXT = "text"
        private const val CHANNEL_ID = "blockheads_server"
        private const val ALERT_CHANNEL_ID = "blockheads_alert"
        private const val NOTIFICATION_ID = 1517
        private const val ALERT_ID = 1518

        /** Set by the plugin: asks the Rust side to stop the server. */
        @Volatile
        var stopRequested: (() -> Unit)? = null

        private fun launchIntent(context: Context): PendingIntent? {
            val launch = context.packageManager.getLaunchIntentForPackage(context.packageName) ?: return null
            return PendingIntent.getActivity(
                context, 0, launch, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
            )
        }

        private fun ensureChannels(context: Context) {
            if (Build.VERSION.SDK_INT < 26) return
            val nm = context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
            nm.createNotificationChannel(NotificationChannel(CHANNEL_ID, context.getString(R.string.hostservice_channel_server), NotificationManager.IMPORTANCE_LOW))
            nm.createNotificationChannel(NotificationChannel(ALERT_CHANNEL_ID, context.getString(R.string.hostservice_channel_alerts), NotificationManager.IMPORTANCE_HIGH))
        }

        /** The crash alert: not ongoing, on its own high-importance channel. */
        fun postAlert(context: Context, title: String, text: String, openLabel: String) {
            ensureChannels(context)
            val open = launchIntent(context)
            val n = NotificationCompat.Builder(context, ALERT_CHANNEL_ID)
                .setContentTitle(title)
                .setContentText(text)
                .setSmallIcon(context.applicationInfo.icon)
                .setColor(0xFFFF8F80.toInt())
                .setAutoCancel(true)
                .setPriority(NotificationCompat.PRIORITY_HIGH)
                .setContentIntent(open)
                .addAction(0, openLabel, open)
                .build()
            val nm = context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
            nm.notify(ALERT_ID, n)
        }
    }
}
