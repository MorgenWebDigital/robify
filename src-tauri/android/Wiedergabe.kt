package de.robify.player

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.support.v4.media.MediaMetadataCompat
import android.support.v4.media.session.MediaSessionCompat
import android.support.v4.media.session.PlaybackStateCompat
import androidx.core.app.NotificationCompat
import androidx.core.content.ContextCompat
import androidx.media.session.MediaButtonReceiver

/**
 * Der Player, den das Telefon selbst zeigt.
 *
 * Auf dem Rechner ist das Fenster der einzige Ort, an dem Robify bedient wird.
 * Auf einem Telefon nicht: Dort gehören Titel, Cover und die Knöpfe für
 * Pause und Weiter auf den Sperrbildschirm und in die Benachrichtigungen —
 * sonst muss man die App erst suchen, um einen Titel zu überspringen.
 *
 * Zwei Dinge hängen daran, die nach außen wie eines aussehen:
 *
 * * Eine `MediaSession` sagt dem System, was läuft und was sich damit machen
 *   lässt. Aus ihr baut Android den Player auf dem Sperrbildschirm, sie nimmt
 *   auch die Tasten von Kopfhörern und Autoradios entgegen.
 * * Ein Vordergrunddienst hält die App am Leben, solange Musik läuft. Ohne
 *   ihn darf Android den Prozess im Hintergrund abräumen, und die Wiedergabe
 *   bricht mitten im Titel ab.
 *
 * Der eigentliche Player bleibt im Rust-Teil. Hier steht nur, was das System
 * sehen soll, und die Knöpfe reichen ihre Befehle dorthin zurück.
 */
object Wiedergabe {
    /** Was gerade läuft; der Dienst liest es beim Aufbauen der Anzeige. */
    internal var titel: String = ""
    internal var kuenstler: String = ""
    internal var album: String = ""
    internal var dauerMs: Long = 0
    internal var positionMs: Long = 0
    internal var laeuft: Boolean = false
    internal var cover: Bitmap? = null


    private val haupt = Handler(Looper.getMainLooper())
    internal var dienst: Wiedergabedienst? = null

    /**
     * Ein Knopf im Systemplayer wurde gedrückt.
     *
     * Der Rust-Teil trägt die Gegenstelle; die Namensform ist die von JNI
     * vorgegebene, deshalb steht dort `Java_de_robify_player_Wiedergabe_befehl`.
     */
    @JvmStatic external fun befehl(name: String, wert: Long)

    /**
     * Neuer Stand aus dem Rust-Teil.
     *
     * Das Cover kommt als Bild in Rohform, nicht als Pfad: Robify hält seine
     * Cover in der Datenbank, es gibt keine Datei, auf die man zeigen könnte.
     *
     * Für das Bild gelten drei Fälle, und der Unterschied zwischen den letzten
     * beiden ist wichtig:
     *
     * * Bytes — neues Bild.
     * * Leeres Feld — der Titel hat keins, das alte muss weg.
     * * `null` — unverändert, nicht mitgeschickt.
     *
     * Der Rust-Teil schickt es nur beim Titelwechsel; ein paar hundert
     * Kilobyte bei jedem Druck auf Pause durch die Brücke zu schieben wäre
     * Verschwendung. Vorher stand hier statt der drei Fälle nur „Bild oder
     * kein Bild“, und das Cover verschwand beim ersten Pausieren.
     */
    @JvmStatic
    fun melden(
        kontext: Context,
        titel: String,
        kuenstler: String,
        album: String,
        dauerMs: Long,
        positionMs: Long,
        laeuft: Boolean,
        cover: ByteArray?,
    ) {
        this.titel = titel
        this.kuenstler = kuenstler
        this.album = album
        this.dauerMs = dauerMs
        this.positionMs = positionMs
        this.laeuft = laeuft

        if (cover != null) {
            this.cover =
                if (cover.isEmpty()) null
                else runCatching { BitmapFactory.decodeByteArray(cover, 0, cover.size) }.getOrNull()
        }

        val anwendung = kontext.applicationContext
        haupt.post {
            val laufend = dienst
            if (laufend == null) {
                ContextCompat.startForegroundService(
                    anwendung,
                    Intent(anwendung, Wiedergabedienst::class.java),
                )
            } else {
                laufend.auffrischen()
            }
        }
    }

    /** Nichts läuft mehr: Anzeige weg, Dienst beenden. */
    @JvmStatic
    fun beenden(kontext: Context) {
        val anwendung = kontext.applicationContext
        haupt.post {
            anwendung.stopService(Intent(anwendung, Wiedergabedienst::class.java))
        }
    }
}

/**
 * Hält die Wiedergabe am Leben und zeigt sie an.
 *
 * Der Dienst spielt selbst nichts ab — das tut der Rust-Teil weiter. Er sagt
 * Android nur, dass hier etwas läuft, das nicht abgeräumt werden darf, und
 * trägt die Anzeige.
 */
class Wiedergabedienst : Service() {
    private lateinit var sitzung: MediaSessionCompat

    override fun onCreate() {
        super.onCreate()
        Wiedergabe.dienst = this

        kanalAnlegen()
        sitzung =
            MediaSessionCompat(this, "Robify").apply {
                setCallback(
                    object : MediaSessionCompat.Callback() {
                        override fun onPlay() = Wiedergabe.befehl("resume", 0)

                        override fun onPause() = Wiedergabe.befehl("pause", 0)

                        override fun onSkipToNext() = Wiedergabe.befehl("next", 0)

                        override fun onSkipToPrevious() = Wiedergabe.befehl("prev", 0)

                        override fun onSeekTo(pos: Long) = Wiedergabe.befehl("seek", pos)

                        override fun onStop() = Wiedergabe.befehl("pause", 0)
                    },
                )
                isActive = true
            }

        // Ohne eine Anzeige binnen weniger Sekunden beendet Android den Dienst
        // von sich aus, mit einem Absturz. Darum sofort, noch vor dem ersten
        // Auffrischen.
        starten(anzeige())
    }

    override fun onStartCommand(absicht: Intent?, flaggen: Int, kennung: Int): Int {
        MediaButtonReceiver.handleIntent(sitzung, absicht)
        auffrischen()
        return START_NOT_STICKY
    }

    override fun onBind(absicht: Intent?): IBinder? = null

    override fun onDestroy() {
        Wiedergabe.dienst = null
        sitzung.isActive = false
        sitzung.release()
        super.onDestroy()
    }

    /** Neuer Stand: Sitzung und Anzeige nachziehen. */
    fun auffrischen() {
        sitzung.setMetadata(
            MediaMetadataCompat.Builder()
                .putString(MediaMetadataCompat.METADATA_KEY_TITLE, Wiedergabe.titel)
                .putString(MediaMetadataCompat.METADATA_KEY_ARTIST, Wiedergabe.kuenstler)
                .putString(MediaMetadataCompat.METADATA_KEY_ALBUM, Wiedergabe.album)
                // Die Laufzeit macht aus dem Balken auf dem Sperrbildschirm
                // erst einen Regler; ohne sie bleibt er ein Strich.
                .putLong(MediaMetadataCompat.METADATA_KEY_DURATION, Wiedergabe.dauerMs)
                .putBitmap(MediaMetadataCompat.METADATA_KEY_ALBUM_ART, Wiedergabe.cover)
                .build(),
        )

        sitzung.setPlaybackState(
            PlaybackStateCompat.Builder()
                .setActions(
                    PlaybackStateCompat.ACTION_PLAY or
                        PlaybackStateCompat.ACTION_PAUSE or
                        PlaybackStateCompat.ACTION_PLAY_PAUSE or
                        PlaybackStateCompat.ACTION_SKIP_TO_NEXT or
                        PlaybackStateCompat.ACTION_SKIP_TO_PREVIOUS or
                        PlaybackStateCompat.ACTION_SEEK_TO or
                        PlaybackStateCompat.ACTION_STOP,
                )
                // Die Geschwindigkeit ist kein Beiwerk: Android rechnet die
                // Position damit selbst weiter. Stünde dort beim Abspielen 0,
                // bliebe die Zeit auf dem Sperrbildschirm stehen, und wir
                // müssten sie jede Sekunde neu melden.
                .setState(
                    if (Wiedergabe.laeuft) PlaybackStateCompat.STATE_PLAYING
                    else PlaybackStateCompat.STATE_PAUSED,
                    Wiedergabe.positionMs,
                    if (Wiedergabe.laeuft) 1f else 0f,
                )
                .build(),
        )

        val verwaltung = getSystemService(NotificationManager::class.java)
        verwaltung.notify(KENNUNG, anzeige())
    }

    private fun starten(anzeige: Notification) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            startForeground(
                KENNUNG,
                anzeige,
                android.content.pm.ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK,
            )
        } else {
            startForeground(KENNUNG, anzeige)
        }
    }

    private fun anzeige(): Notification {
        val oeffnen =
            PendingIntent.getActivity(
                this,
                0,
                Intent(this, MainActivity::class.java),
                PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
            )

        val bau =
            NotificationCompat.Builder(this, KANAL)
                .setContentTitle(Wiedergabe.titel)
                .setContentText(Wiedergabe.kuenstler)
                .setSubText(Wiedergabe.album.ifEmpty { null })
                .setLargeIcon(Wiedergabe.cover)
                // Unser Zeichen, nicht das Dreieck des Systems: Es steht
                // oben links in der Benachrichtigung, in der Statusleiste und
                // im Player auf dem Sperrbildschirm.
                .setSmallIcon(R.drawable.ic_notification)
                .setContentIntent(oeffnen)
                .setVisibility(NotificationCompat.VISIBILITY_PUBLIC)
                // Der laufende Titel ist keine Nachricht, die man wegwischt.
                .setOngoing(Wiedergabe.laeuft)
                .setShowWhen(false)

        bau.addAction(
            android.R.drawable.ic_media_previous,
            "Zurück",
            knopf(PlaybackStateCompat.ACTION_SKIP_TO_PREVIOUS),
        )
        bau.addAction(
            if (Wiedergabe.laeuft) android.R.drawable.ic_media_pause
            else android.R.drawable.ic_media_play,
            if (Wiedergabe.laeuft) "Pause" else "Abspielen",
            knopf(PlaybackStateCompat.ACTION_PLAY_PAUSE),
        )
        bau.addAction(
            android.R.drawable.ic_media_next,
            "Weiter",
            knopf(PlaybackStateCompat.ACTION_SKIP_TO_NEXT),
        )

        bau.setStyle(
            androidx.media.app.NotificationCompat.MediaStyle()
                .setMediaSession(sitzung.sessionToken)
                // Welche der drei Tasten in der eingeklappten Anzeige stehen.
                .setShowActionsInCompactView(0, 1, 2),
        )

        return bau.build()
    }

    private fun knopf(aktion: Long): PendingIntent =
        MediaButtonReceiver.buildMediaButtonPendingIntent(this, aktion)

    private fun kanalAnlegen() {
        val kanal =
            NotificationChannel(
                KANAL,
                "Wiedergabe",
                // Niedrig: Der Player ist eine Anzeige, keine Meldung. Höher
                // eingestuft klingelte er bei jedem Titelwechsel.
                NotificationManager.IMPORTANCE_LOW,
            )
        kanal.setShowBadge(false)
        getSystemService(NotificationManager::class.java).createNotificationChannel(kanal)
    }

    private companion object {
        const val KANAL = "wiedergabe"
        const val KENNUNG = 1
    }
}
