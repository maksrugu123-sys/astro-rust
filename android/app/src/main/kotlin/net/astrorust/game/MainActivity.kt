package net.astrorust.game

import android.os.Build
import android.os.Bundle
import android.os.Process
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import com.google.androidgamesdk.GameActivity
import java.io.File
import java.io.FileOutputStream
import java.io.PrintWriter
import java.io.StringWriter
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

class MainActivity : GameActivity() {
    private lateinit var debugDirectory: File

    private external fun nativeSetDebugDirectory(path: String)

    override fun onCreate(savedInstanceState: Bundle?) {
        // Set the native log destination before GameActivity starts the game runtime.
        val appFiles = getExternalFilesDir(null) ?: filesDir
        debugDirectory = File(appFiles, "ASTRO-RUST/debug")
        debugDirectory.mkdirs()
        nativeSetDebugDirectory(debugDirectory.absolutePath)
        installCrashHandler()
        appendDebugLine(
            "android.log",
            "START model=${Build.MANUFACTURER} ${Build.MODEL}; Android SDK=${Build.VERSION.SDK_INT}"
        )
        writeReadme()

        super.onCreate(savedInstanceState)
        WindowCompat.enableEdgeToEdge(window)
        appendDebugLine("android.log", "ACTIVITY onCreate completed")
    }

    override fun onStart() {
        super.onStart()
        appendDebugLine("android.log", "LIFECYCLE onStart")
    }

    override fun onResume() {
        super.onResume()
        appendDebugLine("android.log", "LIFECYCLE onResume")
    }

    override fun onPause() {
        appendDebugLine("android.log", "LIFECYCLE onPause")
        super.onPause()
    }

    override fun onStop() {
        appendDebugLine("android.log", "LIFECYCLE onStop")
        super.onStop()
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus) hideSystemUi()
    }

    private fun hideSystemUi() {
        window.decorView.post {
            val controller = WindowCompat.getInsetsController(window, window.decorView)
            controller.systemBarsBehavior =
                WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
            controller.hide(WindowInsetsCompat.Type.systemBars())
        }
    }

    private fun installCrashHandler() {
        val previous = Thread.getDefaultUncaughtExceptionHandler()
        Thread.setDefaultUncaughtExceptionHandler { thread, throwable ->
            val trace = StringWriter()
            throwable.printStackTrace(PrintWriter(trace))
            appendDebugLine(
                "java-crash.log",
                "UNCAUGHT thread=${thread.name}\n$trace"
            )
            previous?.uncaughtException(thread, throwable)
                ?: run {
                    Process.killProcess(Process.myPid())
                    kotlin.system.exitProcess(10)
                }
        }
    }

    private fun appendDebugLine(fileName: String, message: String) {
        try {
            debugDirectory.mkdirs()
            val timestamp = SimpleDateFormat("yyyy-MM-dd HH:mm:ss.SSS", Locale.US).format(Date())
            FileOutputStream(File(debugDirectory, fileName), true).bufferedWriter().use {
                it.append("[$timestamp] $message")
                it.newLine()
            }
        } catch (_: Exception) {
            // Diagnostics must never prevent the game from starting.
        }
    }

    private fun writeReadme() {
        try {
            File(debugDirectory, "README.txt").writeText(
                "ASTRO-RUST diagnostics\n" +
                    "Files: session.log (Rust/Bevy), panic.log (Rust panic), " +
                    "android.log (Android lifecycle), java-crash.log (Java/Kotlin crash).\n" +
                    "If the game crashes, send all files in this folder to the developer.\n"
            )
        } catch (_: Exception) {
            // Diagnostics must never prevent the game from starting.
        }
    }

    companion object {
        init {
            System.loadLibrary("astro_rust")
        }
    }
}
