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

    /** where the converter lies, or `null` where it did not come along */
    private fun ffmpeg(kontext: Context): File? =
        File(kontext.applicationInfo.nativeLibraryDir, "libffmpeg.so").takeIf { it.exists() }

    /**
     * hands the converter to yt-dlp.
     *
     * the name is no whim: android unpacks and marks executable only what is
     * called `lib*.so` and lies in the library folder. yt-dlp gets along with
     * it — it reads "ffmpeg" out of the file name and finds `libffprobe.so`
     * beside it by replacing that word.
     */
    private fun ffmpegOrt(kontext: Context): List<String> {
        val ffmpeg = ffmpeg(kontext) ?: return emptyList()
        return listOf("--ffmpeg-location", ffmpeg.absolutePath)
    }

    /**
     * the surroundings a program started here needs.
     *
     * ffmpeg is built as shared libraries, and the linker of a new process
     * does not look into the library folder of the app by itself.
     */
    private fun umgebungSetzen(bau: ProcessBuilder, kontext: Context) {
        val ordner = kontext.applicationInfo.nativeLibraryDir
        bau.environment().apply {
            put("LD_LIBRARY_PATH", ordner)
            put("PATH", "${System.getenv("PATH")}:$ordner")
            put("HOME", kontext.cacheDir.absolutePath)
            put("TMPDIR", kontext.cacheDir.absolutePath)
        }
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

    /** the interpreter, started on first use */
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
            alle.addAll(ffmpegOrt(kontext))
            modul()
                .callAttr(
                    "lauf",
                    id,
                    alle.toTypedArray(),
                    kontext.applicationInfo.nativeLibraryDir,
                )
                .toString()
        } catch (fehler: Throwable) {
            fehlerAntwort(fehler)
        }
    }

    /**
     * converts a file with the ffmpeg that came along.
     *
     * a process of its own, as on the desktop — only the path is a different
     * one, and the linker has to be told where the shared libraries lie.
     */
    @JvmStatic
    fun umwandeln(kontext: Context, args: Array<String>): String {
        val antwort = JSONObject()
        try {
            val ffmpeg = ffmpeg(kontext)
            if (ffmpeg == null) {
                antwort.put("code", -1)
                antwort.put("out", "")
                antwort.put("err", "ffmpeg wurde nicht gefunden.")
                return antwort.toString()
            }

            val befehl = ArrayList<String>()
            befehl.add(ffmpeg.absolutePath)
            befehl.addAll(args)

            val bau = ProcessBuilder(befehl)
            umgebungSetzen(bau, kontext)
            // both streams in one: reading them one after another, one blocks
            // while the other fills up and the call never returns
            bau.redirectErrorStream(true)

            val prozess = bau.start()
            val ausgabe = prozess.inputStream.bufferedReader().use { it.readText() }
            val code = prozess.waitFor()

            antwort.put("code", code)
            antwort.put("out", "")
            antwort.put("err", ausgabe)
        } catch (fehler: Throwable) {
            return fehlerAntwort(fehler)
        }
        return antwort.toString()
    }

    /**
     * yt-dlp comes with the app and is renewed with it.
     *
     * it is installed at build time into the app's own python, so there is
     * nothing to fetch at runtime. the answer names the version in place.
     */
    // the context stays in the signature although nothing here needs it: the
    // rust side calls it by that exact signature over jni
    @Suppress("UNUSED_PARAMETER")
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

    /** progress of a job in percent, or -1 where it is not running */
    @JvmStatic
    fun fortschritt(id: String): Float {
        return try {
            modul().callAttr("fortschritt", id).toFloat()
        } catch (_: Throwable) {
            -1f
        }
    }

    /** cancels a running job */
    @JvmStatic
    fun abbrechen(id: String) {
        try {
            modul().callAttr("abbrechen", id)
        } catch (_: Throwable) {
            // a job that no longer exists is no error
        }
    }
}
