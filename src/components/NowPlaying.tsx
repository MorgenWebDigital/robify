import { useEffect, useRef, useState } from "react";
import { Link } from "react-router-dom";
import { t } from "../lib/i18n";
import { api, errorMessage, fallback } from "../lib/api";
import { albumCover, artistImage } from "../lib/cover";
import { formatTime, releaseLabel } from "../lib/format";
import { useLibrary } from "../store/library";
import { usePlayer } from "../store/player";
import { useUi } from "../store/ui";
import { Cover } from "./Cover";
import {
  ChevronDownIcon,
  HeartIcon,
  LyricsIcon,
  NextIcon,
  PauseIcon,
  PlayIcon,
  PlusIcon,
  PrevIcon,
  QueueIcon,
  RepeatIcon,
  RepeatOneIcon,
  ShuffleIcon,
} from "./Icons";
import type { Track } from "../types";
import { LyricsPanel } from "./LyricsPanel";
import { activeLineIndex, parseLrc, type LyricLine } from "../lib/lrc";
import { useAusblenden } from "../lib/ausblenden";
import { useSchliesstBeimSeitenwechsel } from "../lib/seitenwechsel";

/** Muss zur Dauer von `.animate-stage-out` im Stylesheet passen. */
const ZU_MS = 280;

/** Vollbildansicht mit großem Cover und mitlaufenden Lyrics. */
export function NowPlaying() {
  const currentTrack = usePlayer((s) => s.currentTrack);
  const positionMs = usePlayer((s) => s.positionMs);
  const durationMs = usePlayer((s) => s.durationMs);
  const playing = usePlayer((s) => s.playing);
  const shuffle = usePlayer((s) => s.shuffle);
  const repeat = usePlayer((s) => s.repeat);
  const toggle = usePlayer((s) => s.toggle);
  const next = usePlayer((s) => s.next);
  const previous = usePlayer((s) => s.previous);
  const toggleShuffle = usePlayer((s) => s.toggleShuffle);
  const cycleRepeat = usePlayer((s) => s.cycleRepeat);
  const {
    nowPlayingOpen,
    setNowPlayingOpen,
    setQueueOpen,
    openAddToPlaylist,
    notify,
  } = useUi();
  const refreshLibrary = useLibrary((s) => s.refresh);
  const { sichtbar, schliesst } = useAusblenden(nowPlayingOpen, ZU_MS);
  useSchliesstBeimSeitenwechsel(setNowPlayingOpen);
  /* Nur am Telefon: Dort steht der Text nicht daneben, sondern hinter einer
     Kachel. Am Rechner ist er ohnehin die ganze Zeit zu sehen. */
  const [textOffen, setTextOffen] = useState(false);
  /** Die Vorschau als Text, wenn keine zeitsynchrone Fassung vorliegt. */
  const [anfang, setAnfang] = useState<string[]>([]);
  /** Die zeitsynchrone Fassung, sofern es eine gibt. */
  const [synchron, setSynchron] = useState<LyricLine[]>([]);

  const trackId = currentTrack?.id ?? null;

  /*
   * Die ersten Zeilen für die Kachel.
   *
   * Ohne sie stünde dort nur „Songtext“, und man müsste tippen, um zu sehen,
   * ob überhaupt einer hinterlegt ist. Acht Zeilen sind schon ein Stück Text
   * und geben der Kachel genug Höhe, um neben dem Cover zu bestehen.
   * Der volle Text wird davon nicht doppelt geladen: Die Vollbildansicht holt
   * ihn selbst, und der Befehl liest aus der Datenbank, nicht aus dem Netz.
   */
  useEffect(() => {
    if (!trackId) {
      setAnfang([]);
      setSynchron([]);
      return;
    }
    let gilt = true;
    void api
      .getLyrics(trackId)
      .catch(fallback(null, t("Lyrics")))
      .then((lyrics) => {
        if (!gilt) return;
        setSynchron(lyrics?.synced ? parseLrc(lyrics.synced) : []);
        const roh = lyrics?.plain || lyrics?.synced || "";
        setAnfang(
          roh
            // Zeitmarken einer synchronen Fassung gehören nicht in die Vorschau.
            .replace(/\[\d{1,2}:\d{2}(?:[.:]\d{1,3})?\]/g, "")
            .split("\n")
            .map((zeile) => zeile.trim())
            .filter(Boolean)
            .slice(0, 8),
        );
      });
    return () => {
      gilt = false;
    };
  }, [trackId]);

  /*
   * Der Ausschnitt, der in der Kachel steht.
   *
   * Liegt der Text zeitsynchron vor, wandert er mit: Die gerade gesungene
   * Zeile steht in der zweiten von acht, davor zwei zum Nachlesen, dahinter
   * fünf zum Vorauslesen. Vorher standen dort immer dieselben acht Zeilen vom
   * Anfang, und beim dritten Refrain hatte das mit dem, was zu hören war,
   * nichts mehr zu tun.
   *
   * Ohne Zeitmarken bleibt es beim Anfang: Etwas mitlaufen zu lassen, das
   * nicht weiß, wo es steht, wäre geraten.
   */
  const SICHTBAR = 8;
  const VORLAUF = 2;
  const vorschau = (() => {
    if (synchron.length === 0) return anfang;
    const jetzt = Math.max(0, activeLineIndex(synchron, positionMs));
    const start = Math.min(
      Math.max(0, jetzt - VORLAUF),
      Math.max(0, synchron.length - SICHTBAR),
    );
    return synchron
      .slice(start, start + SICHTBAR)
      .map((zeile) => zeile.text)
      .filter(Boolean);
  })();

  // Beim Titelwechsel schließt sich der Vollbildtext: Er gehörte zum vorigen.
  useEffect(() => setTextOffen(false), [trackId]);

  /*
   * Der Favoritenstand hier, nicht nur im geladenen Titel.
   *
   * `currentTrack` wird beim Titelwechsel einmal geholt und danach nicht mehr;
   * ein Druck auf das Herz änderte die Datenbank, das Herz selbst bliebe aber
   * leer. Der eigene Stand springt sofort um und geht zurück, falls das
   * Speichern scheitert.
   */
  const [favorit, setFavorit] = useState(false);
  useEffect(
    () => setFavorit(Boolean(currentTrack?.favorite)),
    [currentTrack?.id, currentTrack?.favorite],
  );

  const favoritUmschalten = async () => {
    if (!currentTrack) return;
    const neu = !favorit;
    setFavorit(neu);
    try {
      await api.setFavorite(currentTrack.id, neu);
      void refreshLibrary();
    } catch (error) {
      setFavorit(!neu);
      notify(errorMessage(error), "error");
    }
  };

  /*
   * Wischen im Titel: zur Seite blättert, nach unten legt zu.
   *
   * Welche der beiden Richtungen gilt, entscheidet die größere Strecke. Ohne
   * diese Trennung löste jeder schräge Zug beides aus, und man landete beim
   * nächsten Titel, obwohl man zuklappen wollte.
   *
   * Zuklappen zusätzlich nur von ganz oben: Weiter unten ist der Zug nach
   * unten das Blättern durch Songtext und Künstler. Beim Aufsetzen des
   * Fingers wird deshalb gemerkt, ob die Spalte schon oben stand. Für das
   * seitliche Blättern gilt das nicht — waagerecht rollt hier nichts.
   */
  const zugBeginn = useRef<{ x: number; y: number; oben: boolean } | null>(
    null,
  );
  /** Ab hier ist es eine Geste und nicht mehr ein Verrutschen. */
  const ZUG_SCHWELLE = 90;

  /*
   * Wohin gerade geblättert wird, für die kurze Bewegung dabei.
   *
   * Der Wechsel selbst kommt aus dem Rust-Teil und braucht einen Augenblick;
   * ohne Bewegung stünde in dieser Zeit einfach der alte Titel da, und der
   * Wisch sähe aus, als sei er ins Leere gegangen.
   */
  const [blaettert, setBlaettert] = useState<"vor" | "zurueck" | null>(null);
  const blaetterUhr = useRef<number | null>(null);

  const blaettern = (richtung: "vor" | "zurueck") => {
    if (blaetterUhr.current) window.clearTimeout(blaetterUhr.current);
    setBlaettert(richtung);
    // Muss zur Dauer von `blaettern-vor` im Stylesheet passen.
    blaetterUhr.current = window.setTimeout(() => setBlaettert(null), 260);
  };

  useEffect(
    () => () => {
      if (blaetterUhr.current) window.clearTimeout(blaetterUhr.current);
    },
    [],
  );

  const zugStart = (event: React.TouchEvent<HTMLDivElement>) => {
    zugBeginn.current = {
      x: event.touches[0].clientX,
      y: event.touches[0].clientY,
      oben: event.currentTarget.scrollTop <= 0,
    };
  };

  const zugEnde = (event: React.TouchEvent<HTMLDivElement>) => {
    const start = zugBeginn.current;
    zugBeginn.current = null;
    if (!start) return;
    const dx = event.changedTouches[0].clientX - start.x;
    const dy = event.changedTouches[0].clientY - start.y;

    if (Math.abs(dx) > Math.abs(dy)) {
      if (Math.abs(dx) < ZUG_SCHWELLE) return;
      // Nach links kommt das Nächste ins Bild, wie beim Umblättern.
      blaettern(dx < 0 ? "vor" : "zurueck");
      void (dx < 0 ? next() : previous());
      return;
    }

    if (start.oben && dy > ZUG_SCHWELLE) setNowPlayingOpen(false);
  };

  const artistId = currentTrack?.artistId ?? null;
  const [weitere, setWeitere] = useState<Track[]>([]);

  /*
   * Mehr vom selben Künstler, unter dem Künstlerbild.
   *
   * Nach Gehörtem sortiert, nicht nach Titel: Was man ohnehin oft hört, ist
   * die bessere Anregung als das, was zufällig vorn im Alphabet steht. Der
   * laufende Titel fällt heraus — er steht ja oben — und fünf sind genug;
   * mehr wäre keine Anregung, sondern eine zweite Bibliothek.
   */
  useEffect(() => {
    if (!artistId) {
      setWeitere([]);
      return;
    }
    let gilt = true;
    void api
      .artistTracks(artistId)
      .catch(fallback([] as Track[], t("Künstler")))
      .then((titel) => {
        if (!gilt) return;
        setWeitere(
          titel
            .filter((s) => s.id !== trackId)
            .sort((a, b) => b.playCount - a.playCount)
            .slice(0, 5),
        );
      });
    return () => {
      gilt = false;
    };
  }, [artistId, trackId]);

  if (!sichtbar) return null;

  const progress = durationMs ? (positionMs / durationMs) * 100 : 0;

  return (
    <div
      className={`absolute inset-0 z-20 flex flex-col bg-frame p-3 ${
        schliesst ? "animate-stage-out" : "animate-stage"
      }`}
    >
      {/* Eine einzige Fläche hinter allem: Hintergrund und Schimmer wie bei der
          Player-Pille unten, Cover und Lyrics liegen gemeinsam darauf. */}
      <div className="sunken-deep flex min-h-0 flex-1 flex-col rounded-3xl bg-ink-950">
        {/* Ohne Beschriftung, dass gerade gespielt wird, sieht man. */}
        {/* Nur das Zuklappen, und links: Es ist der Weg zurück, und der steht
            überall sonst in der App auch links. Die Warteschlange ist zu den
            übrigen Handgriffen unter die Zeitleiste gewandert. */}
        <div className="flex items-center px-6 py-4">
          <button
            type="button"
            onClick={() => setNowPlayingOpen(false)}
            aria-label={t("Vollbild schließen")}
            className="pill-btn is-raised h-9 w-9"
          >
            <ChevronDownIcon />
          </button>
        </div>

        {currentTrack ? (
          <div className="grid min-h-0 flex-1 gap-8 overflow-hidden px-6 pb-8 lg:grid-cols-2 lg:px-12">
            {/* Am Telefon die einzige Spalte, und sie rollt.
                Vorher teilte das Gitter die Höhe fest zwischen Cover und Text
                auf: 383 zu 178 Pixel. Das Cover bekam davon 224 statt seiner
                339 und stand als breiter Streifen da, der Text als leerer
                Kasten darunter. */}
            <div
              onTouchStart={zugStart}
              onTouchEnd={zugEnde}
              className={`flex min-h-0 flex-col items-center gap-6 overflow-y-auto lg:justify-center lg:overflow-visible ${
                schliesst ? "" : "animate-content"
              } ${
                blaettert === "vor"
                  ? "blaettert-vor"
                  : blaettert === "zurueck"
                    ? "blaettert-zurueck"
                    : ""
              }`}
            >
              <Cover
                src={albumCover(currentTrack.albumId)}
                alt={currentTrack.albumTitle}
                seed={currentTrack.albumId}
                className="raised-cover aspect-square w-full max-w-sm shrink-0"
                rounded="rounded-2xl"
              />
              <div className="w-full max-w-sm text-center">
                <h2
                  className="truncate text-2xl font-bold"
                  title={currentTrack.title}
                >
                  {currentTrack.title}
                </h2>
                <Link
                  to={`/artist/${currentTrack.artistId}`}
                  onClick={() => setNowPlayingOpen(false)}
                  className="mt-1 inline-block text-mute transition hover:text-fg hover:underline"
                >
                  {currentTrack.artistName}
                </Link>
                <Link
                  to={`/album/${currentTrack.albumId}`}
                  onClick={() => setNowPlayingOpen(false)}
                  className="mt-2 block truncate text-sm text-mute/80 transition hover:text-fg"
                >
                  {releaseLabel(currentTrack.releaseType)} ·{" "}
                  {currentTrack.albumTitle}
                  {currentTrack.year ? ` · ${currentTrack.year}` : ""}
                </Link>

                <div className="mt-5">
                  <div className="h-1 w-full overflow-hidden rounded-full bg-ink-700">
                    <div
                      className="h-full rounded-full transition-[width] duration-200"
                      style={{
                        width: `${progress}%`,
                        background: "var(--accent)",
                      }}
                    />
                  </div>
                  <div className="mt-1.5 flex justify-between text-[11px] tabular-nums text-mute">
                    <span>{formatTime(positionMs)}</span>
                    <span>{formatTime(durationMs)}</span>
                  </div>
                </div>

                {/* Ablegen und Warteschlange: klein und links, unter der
                    Zeitleiste. Bewusst neben und nicht in der Knopfreihe
                    darunter — die trägt das Abspielen, und was mit dem Titel
                    geschehen soll, gehört nicht in dieselbe Reihe wie Pause
                    und Weiter.

                    Ab `md` steht beides in der Leiste unten, die dort immer
                    sichtbar ist; hier wäre es doppelt. */}
                <div className="mt-4 flex items-center gap-2 md:hidden">
                  <button
                    type="button"
                    onClick={() => openAddToPlaylist([currentTrack.id])}
                    aria-label={t("Zu Playlist hinzufügen")}
                    title={t("Zu Playlist hinzufügen")}
                    className="pill-btn is-raised h-8 w-8"
                  >
                    <PlusIcon size={16} />
                  </button>
                  <button
                    type="button"
                    onClick={() => void favoritUmschalten()}
                    aria-label={
                      favorit ? t("Aus Favoriten entfernen") : t("Zu Favoriten")
                    }
                    title={
                      favorit ? t("Aus Favoriten entfernen") : t("Zu Favoriten")
                    }
                    aria-pressed={favorit}
                    className={`pill-btn is-raised h-8 w-8 ${favorit ? "is-on" : ""}`}
                  >
                    <HeartIcon size={16} filled={favorit} />
                  </button>
                  <button
                    type="button"
                    onClick={() => setQueueOpen(true)}
                    aria-label={t("Warteschlange")}
                    title={t("Warteschlange")}
                    className="pill-btn is-raised h-8 w-8"
                  >
                    <QueueIcon size={16} />
                  </button>
                </div>

                {/* Die Steuerung, nur am Telefon: Ab `md` steht sie unten in
                    der Leiste, die dort immer sichtbar ist — dieselbe Grenze,
                    an der die Leiste erscheint. */}
                <div className="mt-3 flex items-center justify-center gap-2 md:hidden">
                  <button
                    type="button"
                    onClick={() => void toggleShuffle()}
                    aria-label={t("Zufallswiedergabe")}
                    aria-pressed={shuffle}
                    className={`pill-btn is-raised h-9 w-9 ${shuffle ? "is-on" : ""}`}
                  >
                    <ShuffleIcon size={18} />
                  </button>
                  <button
                    type="button"
                    onClick={() => void previous()}
                    aria-label={t("Vorheriger Titel")}
                    className="pill-btn is-raised h-10 w-10"
                  >
                    <PrevIcon size={20} />
                  </button>
                  <button
                    type="button"
                    onClick={() => void toggle()}
                    aria-label={playing ? t("Pause") : t("Abspielen")}
                    className="pill-btn is-raised is-accent h-14 w-14"
                  >
                    {playing ? (
                      <PauseIcon size={22} />
                    ) : (
                      <PlayIcon size={22} className="ml-0.5" />
                    )}
                  </button>
                  <button
                    type="button"
                    onClick={() => void next()}
                    aria-label={t("Nächster Titel")}
                    className="pill-btn is-raised h-10 w-10"
                  >
                    <NextIcon size={20} />
                  </button>
                  <button
                    type="button"
                    onClick={() => void cycleRepeat()}
                    aria-label={
                      repeat === "off"
                        ? t("Wiederholen aus")
                        : repeat === "all"
                          ? t("Alle wiederholen")
                          : t("Titel wiederholen")
                    }
                    aria-pressed={repeat !== "off"}
                    className={`pill-btn is-raised h-9 w-9 ${repeat !== "off" ? "is-on" : ""}`}
                  >
                    {repeat === "one" ? (
                      <RepeatOneIcon size={18} />
                    ) : (
                      <RepeatIcon size={18} />
                    )}
                  </button>
                </div>
              </div>

              {/* Zwei Kacheln, nur am Telefon: Dort ist neben dem Cover kein
                  Platz für den Text, und die Künstlerseite liegt sonst drei
                  Griffe entfernt. Am Rechner steht beides ohnehin da. */}
              <div className="w-full max-w-sm shrink-0 space-y-3 pb-2 lg:hidden">
                <button
                  type="button"
                  onClick={() => setTextOffen(true)}
                  className="kachel-akzent w-full p-4 text-start"
                >
                  <span className="flex items-center gap-2 text-xs font-semibold tracking-wide uppercase">
                    <LyricsIcon size={14} />
                    {t("Songtext")}
                  </span>
                  <span className="mt-2 block text-base leading-snug">
                    {vorschau.length > 0
                      ? vorschau.join("\n")
                      : t("Noch keiner hinterlegt.")}
                  </span>
                </button>

                {/* Der Künstler zuletzt, ohne eigene Fläche.
                    Ein Kasten machte ihn zu einem Eintrag unter anderen; er
                    steht aber für sich, so wie Cover und Titel oben. Das Bild
                    trägt den Abschnitt, der Rahmen wäre nur Beiwerk. */}
                <div className="w-full pt-2">
                  <Link
                    to={`/artist/${currentTrack.artistId}`}
                    onClick={() => setNowPlayingOpen(false)}
                    className="flex flex-col items-center gap-3"
                  >
                    <Cover
                      src={artistImage(currentTrack.artistId)}
                      alt={currentTrack.artistName}
                      seed={currentTrack.artistName}
                      className="raised-cover h-40 w-40 shrink-0"
                      rounded="rounded-full"
                    />
                    <span className="max-w-full truncate text-2xl font-bold">
                      {currentTrack.artistName}
                    </span>
                  </Link>

                  {weitere.length > 0 && (
                    <div className="mt-4">
                      <p className="mb-1 eyebrow">{t("Mehr davon")}</p>
                      <ul>
                        {weitere.map((titel) => (
                          <li key={titel.id}>
                            <button
                              type="button"
                              onClick={() => void api.playTracks([titel.id], 0)}
                              className="flex w-full items-center gap-3 rounded-lg py-1.5 text-start"
                            >
                              <Cover
                                src={albumCover(titel.albumId)}
                                alt={titel.albumTitle}
                                seed={titel.albumId}
                                className="h-10 w-10 shrink-0"
                                rounded="rounded-md"
                              />
                              <span className="min-w-0 flex-1 truncate text-sm">
                                {titel.title}
                              </span>
                              <span className="shrink-0 text-xs tabular-nums text-mute">
                                {formatTime(titel.durationMs)}
                              </span>
                            </button>
                          </li>
                        ))}
                      </ul>
                    </div>
                  )}
                </div>
              </div>
            </div>

            {/* Am Rechner steht der Text neben dem Cover; dort ist Platz, und
                er ist das, worauf der Blick am Ende ruht. Am Telefon führt der
                Weg über die Kachel ins Vollbild. */}
            <div
              className={`surface hidden min-h-0 overflow-hidden p-5 lg:block ${schliesst ? "" : "animate-content-late"}`}
            >
              <LyricsPanel track={currentTrack} />
            </div>
          </div>
        ) : (
          <div className="grid flex-1 place-items-center text-mute">
            {t("Starte einen Titel, um ihn hier zu sehen.")}
          </div>
        )}

        {/* Der Text im Vollbild, über allem.
            Innerhalb derselben Fläche, nicht als eigenes Fenster: So bleibt
            die Pille unten bedienbar, und der Weg zurück ist derselbe Griff
            wie überall — ein Pfeil oben rechts. */}
        {textOffen && currentTrack && (
          <div className="absolute inset-0 z-30 flex flex-col rounded-3xl bg-ink-950 p-5 animate-stage">
            <div className="mb-2 flex shrink-0 items-center justify-between gap-3">
              <div className="min-w-0">
                <p className="eyebrow">{t("Songtext")}</p>
                <p className="truncate text-sm font-medium">
                  {currentTrack.title}
                </p>
              </div>
              <button
                type="button"
                onClick={() => setTextOffen(false)}
                aria-label={t("Songtext schließen")}
                className="pill-btn is-raised h-9 w-9 shrink-0"
              >
                <ChevronDownIcon />
              </button>
            </div>
            <div className="min-h-0 flex-1">
              <LyricsPanel track={currentTrack} />
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
