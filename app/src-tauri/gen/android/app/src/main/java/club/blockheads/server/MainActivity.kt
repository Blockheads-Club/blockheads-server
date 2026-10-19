package club.blockheads.server

import android.os.Bundle
import android.os.SystemClock
import androidx.activity.enableEdgeToEdge
import androidx.core.splashscreen.SplashScreen.Companion.installSplashScreen

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    // Android 12+ shows the system splash (Theme.App.Starting); older versions get the compat version of it.
    // Must run before super.onCreate: it switches to the app theme (postSplashScreenTheme).
    // It stays up at least SPLASH_MIN_MS, so it is seen rather than flashed.
    val start = SystemClock.uptimeMillis()
    installSplashScreen().setKeepOnScreenCondition { SystemClock.uptimeMillis() - start < SPLASH_MIN_MS }
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
  }

  companion object {
    const val SPLASH_MIN_MS = 700L
  }
}
