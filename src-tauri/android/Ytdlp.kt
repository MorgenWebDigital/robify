package de.robify.player

import android.content.Context
import com.chaquo.python.Python
import com.chaquo.python.android.AndroidPlatform
import org.json.JSONObject
import java.io.File

/**
 * the bridge to yt-dlp on android.
 *
 * yt-dlp is python, and python lives in this app as a library rather than as
 * a program. so nothing is started here: the interpreter runs in the process
 * and `robify_ytdlp.py` makes the call look like one from the outside — exit
 * code, output, error text, packed into json, exactly as the rust side has
 * always read it.
 *
 * the methods are `@JvmStatic` so the rust side reaches them over jni without
 * a detour through an instance.
 */
object Ytdlp {
    /**
     * the javascript runtime yt-dlp needs for youtube.
     *
     * since yt-dlp 2025.11.12 the n-parameter challenge is unsolvable without
     * one, and formats fall away. deno, node and bun do not exist on android;
     * quickjs is built along and lies in the library folder, because android
     * marks executable only what is called `lib*.so` and lies there.
     */
    private fun jsLaufzeit(kontext: Context): List<String> {
        val qjs = File(kontext.applicationInfo.nativeLibraryDir, "libqjs.so")
        if (!qjs.exists()) return emptyList()
        return listOf("--js-runtimes", "quickjs:${qjs.absolutePath}")
    }

    /**
     * the application, without anybody handing it over.
     *
     * the rust side calls these methods from a thread of its own and has no
     * activity at hand. the same route is taken in `android.rs` to reach the
     * context out of `JNI_OnLoad`.
     */
    private fun anwendung(): Context {
        val klasse = Class.forName("android.app.ActivityThread")
        return klasse.getMethod("currentApplication").invoke(null) as Context
    }

    /** the interpreter, started on first use. */
    @Synchronized
    private fun python(): Python {
        if (!Python.isStarted()) {
            Python.start(AndroidPlatform(anwendung()))
        }
        return Python.getInstance()
    }

    private fun modul() = python().getModule("robify_ytdlp")

    private fun fehlerAntwort(fehler: Throwable): String =
        JSONObject().apply {
            put("code", -1)
            put("out", "")
            put("err", fehler.message ?: fehler.toString())
        }.toString()

    @JvmStatic
    fun ausfuehren(id: String, args: Array<String>): String {
        return try {
            val kontext = anwendung()
            val alle = args.toMutableList()
            alle.addAll(jsLaufzeit(kontext))
            modul().callAttr("lauf", id, alle.toTypedArray()).toString()
        } catch (fehler: Throwable) {
            fehlerAntwort(fehler)
        }
    }

    /**
     * conversion is not on offer here.
     *
     * it would take ffmpeg, and the only maintained build for android stands
     * under the gpl, which does not go together with robify's licence. audio
     * is downloaded in a format that plays as it is; cover and tags robify
     * writes itself.
     */
    @JvmStatic
    fun umwandeln(kontext: Context, args: Array<String>): String =
        JSONObject().apply {
            put("code", -1)
            put("out", "")
            put(
                "err",
                "Umwandeln ist auf Android nicht möglich. " +
                    "Der Titel bleibt in seinem ursprünglichen Format.",
            )
        }.toString()

    /**
     * yt-dlp comes with the app and is renewed with it.
     *
     * it is installed at build time into the app's own python, so there is
     * nothing to fetch at runtime. the answer names the version in place.
     */
    @JvmStatic
    fun aktualisieren(kontext: Context): String {
        return try {
            JSONObject().apply {
                put("code", 0)
                put("out", modul().callAttr("fassung").toString())
                put("err", "")
            }.toString()
        } catch (fehler: Throwable) {
            fehlerAntwort(fehler)
        }
    }

    /** progress of a job in percent, or -1 where it is not running. */
    @JvmStatic
    fun fortschritt(id: String): Float {
        return try {
            modul().callAttr("fortschritt", id).toFloat()
        } catch (_: Throwable) {
            -1f
        }
    }

    /** cancels a running job. */
    @JvmStatic
    fun abbrechen(id: String) {
        try {
            modul().callAttr("abbrechen", id)
        } catch (_: Throwable) {
            // a job that no longer exists is no error
        }
    }
}
