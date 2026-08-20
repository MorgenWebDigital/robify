package de.robify.player

import android.content.Context
import android.net.Uri
import android.provider.OpenableColumns
import java.io.File
import java.io.FileOutputStream

/**
 * brings files in from the android file picker.
 *
 * the picker returns no path but an address of the form
 * `content://com.android.externalstorage.documents/document/primary%3A…`. a
 * file does not necessarily stand behind it: it can just as well be an entry
 * in a cloud the provider downloads first. only the content resolver knows
 * how to get at the bytes.
 *
 * the rust side knows paths alone. so it is copied here, into the folder the
 * user has for their own music anyway, and afterwards it is an ordinary file
 * read like any other.
 */
object Dateien {
    /**
     * copies the address into the target folder and returns the path of the
     * copy. an empty answer means it did not work.
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
     * the name the source gives itself.
     *
     * the last segment of the address does not always do: with a cloud an id
     * such as `acc=1;doc=42` stands there. the column `DISPLAY_NAME` delivers
     * the name the file picker shows too.
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
     * a name still free in the target folder.
     *
     * choosing the same file twice is not to overwrite the first, they could
     * be two different recordings of the same name.
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
