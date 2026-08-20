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
 * the player the phone shows itself.
 *
 * on a desktop the window is the only place robify is operated from. on a
 * phone it is not: title, cover and the buttons for pause and next belong on
 * the lock screen and into the notifications there, otherwise one has to look
 * for the app first in order to skip a track.
 *
 * two things hang off it that look like one from outside:
 *
 * * a `MediaSession` tells the system what is running and what can be done
 *   with it. android builds the player on the lock screen out of it, and it
 *   takes in the keys of headphones and car radios as well.
 * * a foreground service keeps the app alive while music is running. without
 *   it android may clear the process away in the background and playback
 *   breaks off mid-track.
 *
 * the actual player stays on the rust side. only what the system is to see
 * stands here, and the buttons pass their commands back there.
 */
object Wiedergabe {
    /** what is running, the service reads it while building the display. */
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
     * a button in the system player was pressed.
     *
     * the rust side carries the counterpart, and the name form is the one jni
     * prescribes, hence `Java_de_robify_player_Wiedergabe_befehl` there.
     */
    @JvmStatic external fun befehl(name: String, wert: Long)

    /**
     * a new state from the rust side.
     *
     * the cover comes as raw image bytes and not as a path: robify keeps its
     * covers in the database, there is no file to point at.
     *
     * three cases apply to the image, and the difference between the last two
     * matters:
     *
     * * bytes: a new image.
     * * an empty array: the track has none, the old one has to go.
     * * `null`: unchanged, not sent along.
     *
     * the rust side sends it on a track change only, and pushing a few
     * hundred kilobytes through the bridge at every press on pause would be
     * waste. instead of the three cases only image-or-no-image stood here
     * before, and the cover disappeared at the first pause.
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

    /** nothing is playing any more: drop the display, stop the service. */
    @JvmStatic
    fun beenden(kontext: Context) {
        val anwendung = kontext.applicationContext
        haupt.post {
            anwendung.stopService(Intent(anwendung, Wiedergabedienst::class.java))
        }
    }
}

/**
 * keeps the playback alive and displays it.
 *
 * the service plays nothing itself, the rust side carries on doing that. it
 * only tells android that something is running here which must not be cleared
 * away, and it carries the display.
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

        // without a display within a few seconds android ends the service by
        // itself, with a crash. hence at once, before the first refresh
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

    /** a new state: pull session and display along. */
    fun auffrischen() {
        sitzung.setMetadata(
            MediaMetadataCompat.Builder()
                .putString(MediaMetadataCompat.METADATA_KEY_TITLE, Wiedergabe.titel)
                .putString(MediaMetadataCompat.METADATA_KEY_ARTIST, Wiedergabe.kuenstler)
                .putString(MediaMetadataCompat.METADATA_KEY_ALBUM, Wiedergabe.album)
                // the duration is what turns the bar on the lock screen into
                // a slider, without it it stays a line
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
                // the speed is no trimming: android carries the position on
                // from it itself. with a 0 standing there during playback the
                // time on the lock screen would stand still and would have to
                // be reported anew every second
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
                // our mark and not the triangle of the system: it stands at
                // the top left of the notification, in the status bar and in
                // the player on the lock screen
                .setSmallIcon(R.drawable.ic_notification)
                .setContentIntent(oeffnen)
                .setVisibility(NotificationCompat.VISIBILITY_PUBLIC)
                // the running track is no message one swipes away
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
                // which of the three keys stand in the collapsed display
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
                // low: the player is a display, not a notification. rated
                // higher it rang at every track change
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
