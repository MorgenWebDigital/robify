package de.robify.player

import android.content.Context
import android.net.Uri
import android.provider.OpenableColumns
import java.io.File
import java.io.FileOutputStream

/**
 * Holt Dateien aus der Dateiauswahl von Android herein.
 *
 * Die Auswahl gibt keinen Pfad zurück, sondern eine Adresse der Form
 * `content://com.android.externalstorage.documents/document/primary%3A…`.
 * Dahinter steht nicht zwingend eine Datei: Es kann ebenso ein Eintrag in
 * einer Cloud sein, den erst der Anbieter herunterlädt. Nur der
 * ContentResolver weiß, wie er an die Bytes kommt.
 *
 * Der Rust-Teil kennt nur Pfade. Also wird hier kopiert, und zwar in den
 * Ordner, den der Nutzer ohnehin für eigene Musik hat; danach ist es eine
 * gewöhnliche Datei, die eingelesen wird wie jede andere.
 */
object Dateien {
    /**
     * Kopiert die Adresse in den Zielordner und gibt den Pfad der Kopie
     * zurück. Eine leere Antwort heißt: hat nicht geklappt.
     */
    @JvmStatic
    fun holen(
        kontext: Context,
        adresse: String,
        zielordner: String,
    ): String =
        runCatching {
            val quelle = Uri.parse(adresse)
            val ordner = File(zielordner)
            ordner.mkdirs()

            val ziel = freierName(ordner, anzeigename(kontext, quelle))
            kontext.contentResolver.openInputStream(quelle).use { ein ->
                requireNotNull(ein) { "kein Strom zu $adresse" }
                FileOutputStream(ziel).use { aus -> ein.copyTo(aus) }
            }
            ziel.absolutePath
        }.getOrElse { "" }

    /**
     * Der Name, den die Quelle selbst angibt.
     *
     * Der letzte Abschnitt der Adresse taugt nicht immer: Bei einer Cloud
     * steht dort eine Kennung wie `acc=1;doc=42`. Die Spalte `DISPLAY_NAME`
     * liefert den Namen, den auch die Dateiauswahl anzeigt.
     */
    private fun anzeigename(
        kontext: Context,
        quelle: Uri,
    ): String {
        kontext.contentResolver
            .query(quelle, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
            ?.use { zeiger ->
                if (zeiger.moveToFirst() && !zeiger.isNull(0)) {
                    val name = zeiger.getString(0)
                    if (name.isNotBlank()) return name
                }
            }
        return quelle.lastPathSegment?.substringAfterLast('/')?.ifBlank { null }
            ?: "Unbenannt"
    }

    /**
     * Ein Name, der im Zielordner noch frei ist.
     *
     * Zweimal dieselbe Datei zu wählen soll die erste nicht überschreiben —
     * es könnten zwei verschiedene Aufnahmen gleichen Namens sein.
     */
    private fun freierName(
        ordner: File,
        name: String,
    ): File {
        val sauber = name.replace('/', '_')
        val stamm = sauber.substringBeforeLast('.', sauber)
        val endung = sauber.substringAfterLast('.', "")
        var ziel = File(ordner, sauber)
        var zahl = 2
        while (ziel.exists()) {
            val neu = if (endung.isEmpty()) "$stamm ($zahl)" else "$stamm ($zahl).$endung"
            ziel = File(ordner, neu)
            zahl += 1
        }
        return ziel
    }
}
