package de.robify.player

import android.content.Context
import com.yausername.youtubedl_android.YoutubeDL
import com.yausername.youtubedl_android.YoutubeDLRequest
import org.json.JSONObject
import java.io.File
import java.util.concurrent.ConcurrentHashMap

/**
 * the bridge to yt-dlp on android.
 *
 * on a desktop robify starts yt-dlp as a program of its own and reads its
 * output. on android that program does not exist: it is written in python,
 * and even the linux binary does not run here because android uses a
 * different c library.
 *
 * `youtubedl-android` brings yt-dlp together with a python runtime as a
 * library. its interface takes the same switches as the program, so the rust
 * side can pass its calls on unchanged, only the way there differs.
 *
 * the methods are `@JvmStatic` so the rust side reaches them over jni without
 * a detour through an instance.
 */
object Ytdlp {
    /**
     * progress per running job, in percent.
     *
     * a callback into rust would be the straighter way but would demand a
     * native method of its own there and a thread attached to the java
     * runtime. a table the rust side polls gets by without that: it asks on a
     * tick anyway to supply the ui.
     */
    private val fortschritte = ConcurrentHashMap<String, Float>()

    /**
     * runs yt-dlp with the switches handed over and waits for the end.
     *
     * returns json: `code`, `out`, `err`. the rust side therefore sees the
     * same as with a program of its own, and the evaluation there stays as it
     * is.
     */
    @JvmStatic
    fun ausfuehren(id: String, args: Array<String>): String {
        val antwort = JSONObject()
        try {
            // `addCommands` passes the list on unchanged. `addOption` would
            // not: it puts every entry into a map as a key, and a value that
            // occurred before falls out in doing so. `--extractor-retries 3
            // --retry-sleep 3` therefore became `--extractor-retries 3
            // --retry-sleep`, and yt-dlp read the next switch as the sleep
            // time: "invalid http retry sleep expression '--progress'"
            val auftrag = YoutubeDLRequest(emptyList()).addCommands(args.toList())

            fortschritte[id] = 0f
            val ergebnis = YoutubeDL.getInstance().execute(auftrag, id) { prozent, _, _ ->
                fortschritte[id] = prozent
            }

            antwort.put("code", ergebnis.exitCode)
            antwort.put("out", ergebnis.out)
            antwort.put("err", ergebnis.err)
        } catch (fehler: Throwable) {
            // a failure comes back as an answer as well, not as an exception
            // through jni: a thrown exception would have to be collected by
            // the rust side on purpose, and forgetting that crashes the app
            antwort.put("code", -1)
            antwort.put("out", "")
            antwort.put("err", fehler.message ?: fehler.toString())
        } finally {
            fortschritte.remove(id)
        }
        return antwort.toString()
    }

    /**
     * runs the bundled ffmpeg.
     *
     * yt-dlp gets ffmpeg handed to it by the library through
     * `--ffmpeg-location` and can convert with it. robify also converts once
     * itself though: where a source offers the track in a format alone that
     * the player does not know, the finished file is converted afterwards.
     * there is no way through yt-dlp for that.
     *
     * the program lies in the library folder of the app as `libffmpeg.so`,
     * one of the few places android still allows execution in. the
     * environment is the same one `youtubedl-android` sets when it starts
     * ffmpeg for yt-dlp, and without it the program would not find its own
     * libraries.
     */
    @JvmStatic
    fun umwandeln(kontext: Context, args: Array<String>): String {
        val antwort = JSONObject()
        try {
            val binOrdner = File(kontext.applicationInfo.nativeLibraryDir)
            val pakete = File(File(kontext.noBackupFilesDir, "youtubedl-android"), "packages")
            val python = File(pakete, "python")
            val ffmpeg = File(pakete, "ffmpeg")

            val befehl = ArrayList<String>()
            befehl.add(File(binOrdner, "libffmpeg.so").absolutePath)
            befehl.addAll(args)

            val bau = ProcessBuilder(befehl)
            bau.environment().apply {
                put(
                    "LD_LIBRARY_PATH",
                    "${python.absolutePath}/usr/lib:${ffmpeg.absolutePath}/usr/lib",
                )
                put("PATH", "${System.getenv("PATH")}:${binOrdner.absolutePath}")
                put("HOME", kontext.cacheDir.absolutePath)
                put("TMPDIR", kontext.cacheDir.absolutePath)
            }
            // both streams in one: reading them one after another, one
            // blocks while the other fills up and the call never returns.
            // ffmpeg writes its messages to stderr alone anyway
            bau.redirectErrorStream(true)

            val prozess = bau.start()
            val ausgabe = prozess.inputStream.bufferedReader().use { it.readText() }
            val code = prozess.waitFor()

            antwort.put("code", code)
            antwort.put("out", "")
            antwort.put("err", ausgabe)
        } catch (fehler: Throwable) {
            antwort.put("code", -1)
            antwort.put("out", "")
            antwort.put("err", fehler.message ?: fehler.toString())
        }
        return antwort.toString()
    }

    /**
     * fetches the newest version of yt-dlp and lays it over the bundled one.
     *
     * the library brings yt-dlp along but in the state it had when it was
     * released, here november 2025 while july 2026 already stands above.
     * youtube keeps changing its player and turns old versions away, and that
     * is exactly where the 403s on the phone came from. on a desktop one
     * helps oneself with `yt-dlp -U`, and here this is the way.
     *
     * the version that applies afterwards comes back.
     */
    @JvmStatic
    fun aktualisieren(kontext: Context): String {
        val antwort = JSONObject()
        try {
            YoutubeDL.getInstance()
                .updateYoutubeDL(kontext, YoutubeDL.UpdateChannel._STABLE)
            antwort.put("code", 0)
            antwort.put("out", YoutubeDL.getInstance().version(kontext) ?: "")
            antwort.put("err", "")
        } catch (fehler: Throwable) {
            antwort.put("code", -1)
            antwort.put("out", "")
            antwort.put("err", fehler.message ?: fehler.toString())
        }
        return antwort.toString()
    }

    /** progress of a job in percent, or -1 where it is not running. */
    @JvmStatic
    fun fortschritt(id: String): Float = fortschritte[id] ?: -1f

    /** cancels a running job. */
    @JvmStatic
    fun abbrechen(id: String) {
        try {
            YoutubeDL.getInstance().destroyProcessById(id)
        } catch (_: Throwable) {
            // a job that no longer exists is no error
        }
    }
}
