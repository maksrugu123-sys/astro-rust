package net.astrorust.game

import com.google.androidgamesdk.GameActivity

class MainActivity : GameActivity() {
    companion object {
        init {
            System.loadLibrary("astro_rust")
        }
    }
}
