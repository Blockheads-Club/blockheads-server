package club.blockheads.server

import android.os.Bundle
import androidx.activity.enableEdgeToEdge
import androidx.core.splashscreen.SplashScreen.Companion.installSplashScreen

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    // Android 12+ shows the system splash (Theme.App.Starting); older versions get the compat version of it.
    // Must run before super.onCreate: it switches to the app theme (postSplashScreenTheme).
    installSplashScreen()
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
  }
}
