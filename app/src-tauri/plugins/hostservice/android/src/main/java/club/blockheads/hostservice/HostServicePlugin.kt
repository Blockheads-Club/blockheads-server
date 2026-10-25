package club.blockheads.hostservice

import android.Manifest
import android.app.Activity
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.core.content.FileProvider
import java.io.File
import androidx.core.content.ContextCompat
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Channel
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@InvokeArg
class StartArgs {
    var title: String = "Blockheads Server"
    var text: String = ""
    var bigText: String? = null
    var startedAt: Long? = null
    var loading: Boolean = false
    var address: String? = null
    var stopLabel: String = "Stop server"
    var copyLabel: String = "Copy address"
    var openLabel: String = "Console"
}

@InvokeArg
class AlertArgs {
    var title: String = "Blockheads Server"
    var text: String = ""
    var openLabel: String = "Open log"
}

@InvokeArg
class StopHandlerArgs {
    lateinit var channel: Channel
}

@InvokeArg
class ApkArgs {
    var path: String = ""
}

@TauriPlugin
class HostServicePlugin(private val activity: Activity) : Plugin(activity) {
    @Command
    fun start(invoke: Invoke) {
        val args = invoke.parseArgs(StartArgs::class.java)
        val intent = Intent(activity, ServerService::class.java)
            .putExtra(ServerService.EXTRA_TITLE, args.title)
            .putExtra(ServerService.EXTRA_TEXT, args.text)
            .putExtra(ServerService.EXTRA_BIG_TEXT, args.bigText)
            .putExtra(ServerService.EXTRA_STARTED_AT, args.startedAt ?: 0L)
            .putExtra(ServerService.EXTRA_LOADING, args.loading)
            .putExtra(ServerService.EXTRA_ADDRESS, args.address)
            .putExtra(ServerService.EXTRA_STOP_LABEL, args.stopLabel)
            .putExtra(ServerService.EXTRA_COPY_LABEL, args.copyLabel)
            .putExtra(ServerService.EXTRA_OPEN_LABEL, args.openLabel)
        ContextCompat.startForegroundService(activity, intent)
        invoke.resolve(JSObject())
    }

    @Command
    fun stop(invoke: Invoke) {
        // stopService never starts the service (startService would throw in the background on Android 8+);
        // onDestroy releases the wake and Wi-Fi locks and removes the notification.
        activity.stopService(Intent(activity, ServerService::class.java))
        invoke.resolve(JSObject())
    }

    @Command
    fun alert(invoke: Invoke) {
        val args = invoke.parseArgs(AlertArgs::class.java)
        ServerService.postAlert(activity, args.title, args.text, args.openLabel)
        invoke.resolve(JSObject())
    }

    @Command
    fun setStopHandler(invoke: Invoke) {
        val args = invoke.parseArgs(StopHandlerArgs::class.java)
        // The service lives in the same process; a tap on "Stop server" reaches Rust through this channel even
        // while the WebView is suspended.
        ServerService.stopRequested = { args.channel.send(JSObject()) }
        invoke.resolve(JSObject())
    }

    /** Resolves `{allowed}`; when Android does not allow this app to install packages yet, opens that settings screen. */
    @Command
    fun installAllowed(invoke: Invoke) {
        val ok = Build.VERSION.SDK_INT < 26 || activity.packageManager.canRequestPackageInstalls()
        if (!ok) {
            val settings = Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:${activity.packageName}"))
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            try {
                activity.startActivity(settings)
            } catch (e: Exception) {
                // no such screen on this device: the answer is still "not allowed"
            }
        }
        invoke.resolve(JSObject().put("allowed", ok))
    }

    /** Where update APKs are downloaded: the folder `res/xml/file_paths.xml` shares with the installer. */
    @Command
    fun updatesDir(invoke: Invoke) {
        invoke.resolve(JSObject().put("path", File(activity.filesDir, "updates").absolutePath))
    }

    /** Opens the system installer for a downloaded update APK. Only files in `<files>/updates/` are accepted. */
    @Command
    fun installApk(invoke: Invoke) {
        val args = invoke.parseArgs(ApkArgs::class.java)
        val base = File(activity.filesDir, "updates").canonicalFile
        val file = try { File(args.path).canonicalFile } catch (e: Exception) { null }
        if (file == null || !file.path.startsWith(base.path + File.separator) || !file.name.endsWith(".apk") || !file.isFile) {
            invoke.reject("invalid_path")
            return
        }
        try {
            val uri = FileProvider.getUriForFile(activity, "${activity.packageName}.fileprovider", file)
            val view = Intent(Intent.ACTION_VIEW)
                .setDataAndType(uri, "application/vnd.android.package-archive")
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
            activity.startActivity(view)
            invoke.resolve(JSObject())
        } catch (e: Exception) {
            invoke.reject("launch: ${e.message}")
        }
    }

    @Command
    fun requestNotificationPermission(invoke: Invoke) {
        if (Build.VERSION.SDK_INT >= 33 &&
            ContextCompat.checkSelfPermission(activity, Manifest.permission.POST_NOTIFICATIONS) !=
            PackageManager.PERMISSION_GRANTED
        ) {
            activity.requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 4711)
        }
        invoke.resolve(JSObject())
    }
}
