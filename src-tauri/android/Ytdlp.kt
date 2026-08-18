package de.robify.player

import com.yausername.youtubedl_android.YoutubeDL
import com.yausername.youtubedl_android.YoutubeDLRequest
import org.json.JSONObject
import java.util.concurrent.ConcurrentHashMap

/**
 * Brücke zu yt-dlp auf Android.
 *
 * Auf dem Rechner startet Robify yt-dlp als eigenes Programm und liest dessen
 * Ausgabe. Auf Android gibt es dieses Programm nicht: Es ist in Python
 * geschrieben, und selbst die Linux-Binärdatei läuft hier nicht, weil Android
 * eine andere C-Bibliothek verwendet.
 *
 * `youtubedl-android` bringt yt-dlp samt einer Python-Laufzeit als Bibliothek
 * mit. Ihre Schnittstelle nimmt dieselben Schalter entgegen wie das Programm,
 * der Rust-Teil kann seine Aufrufe also unverändert weiterreichen; nur der Weg
 * dorthin ist ein anderer.
 *
 * Die Methoden sind `@JvmStatic`, damit der Rust-Teil sie über JNI ohne
 * Umweg über eine Instanz erreicht.
 */
object Ytdlp {
    /**
     * Fortschritt je laufendem Auftrag, in Prozent.
     *
     * Ein Rückruf nach Rust wäre der geradere Weg, verlangte dort aber eine
     * eigene native Methode und einen Faden, der an der Java-Laufzeit hängt.
     * Eine Tafel, die der Rust-Teil abfragt, kommt ohne das aus: Er fragt
     * ohnehin im Takt nach, um die Oberfläche zu versorgen.
     */
    private val fortschritte = ConcurrentHashMap<String, Float>()

    /**
     * Führt yt-dlp mit den übergebenen Schaltern aus und wartet auf das Ende.
     *
     * Gibt JSON zurück: `code`, `out`, `err`. Damit sieht der Rust-Teil
     * dasselbe wie bei einem eigenen Programm, und die Auswertung dort bleibt,
     * wie sie ist.
     */
    @JvmStatic
    fun ausfuehren(id: String, args: Array<String>): String {
        val antwort = JSONObject()
        try {
            val auftrag = YoutubeDLRequest(emptyList())
            args.forEach { auftrag.addOption(it) }

            fortschritte[id] = 0f
            val ergebnis = YoutubeDL.getInstance().execute(auftrag, id) { prozent, _, _ ->
                fortschritte[id] = prozent
            }

            antwort.put("code", ergebnis.exitCode)
            antwort.put("out", ergebnis.out)
            antwort.put("err", ergebnis.err)
        } catch (fehler: Throwable) {
            // Auch ein Fehlschlag kommt als Antwort zurück, nicht als
            // Ausnahme durch JNI: Eine geworfene Ausnahme müsste der Rust-Teil
            // eigens abholen, und vergisst er das, stürzt die App ab.
            antwort.put("code", -1)
            antwort.put("out", "")
            antwort.put("err", fehler.message ?: fehler.toString())
        } finally {
            fortschritte.remove(id)
        }
        return antwort.toString()
    }

    /** Fortschritt eines Auftrags in Prozent, oder -1, wenn er nicht läuft. */
    @JvmStatic
    fun fortschritt(id: String): Float = fortschritte[id] ?: -1f

    /** Bricht einen laufenden Auftrag ab. */
    @JvmStatic
    fun abbrechen(id: String) {
        try {
            YoutubeDL.getInstance().destroyProcessById(id)
        } catch (_: Throwable) {
            // Ein Auftrag, den es nicht mehr gibt, ist kein Fehler.
        }
    }
}
