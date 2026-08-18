# Änderungen

Diese Datei folgt [Keep a Changelog](https://keepachangelog.com/de/1.1.0/),
die Versionen der [semantischen Versionierung](https://semver.org/lang/de/).

## [Unveröffentlicht]

## [0.1.0] – noch nicht veröffentlicht

Erste Fassung.

### Wiedergabe

- Lokale Wiedergabe über `rodio`/`symphonia`: mp3, flac, m4a, aac, ogg, opus,
  wav, aiff, ape
- Warteschlange mit Umsortieren per Ziehen, Zufallswiedergabe, Wiederholung
  von Titel oder Liste
- Sleeptimer nach Minuten oder bis zum Ende des laufenden Titels
- Lyrics, zeitsynchron mitlaufend (LRC) und anklickbar zum Springen, sonst
  als einfacher Text; eigener Takt-Editor zum Setzen der Zeitmarken

### Bibliothek

- Import einzelner Dateien oder ganzer Ordner, mit Fortschrittsanzeige
- Künstlerseiten mit Trennung nach Singles, EPs und Alben, jederzeit von Hand
  korrigierbar
- Mehrere Künstler je Titel, Gastbeiträge eingeschlossen
- Playlists mit eigenem Cover, umsortierbar per Ziehen
- Favoriten ohne eigene Playlist
- Abgleich mit dem Bibliotheksordner: verwaiste Dateien und fehlende Einträge
  werden gemeldet, nie stillschweigend gelöscht
- Sicherung der Datenbank über `VACUUM INTO`, die letzten fünf bleiben liegen

### Auswertung

- Wochenmix aus den dreißig meistgehörten Titeln der Woche
- Rückblick nach Monat, Jahr oder gesamt, mit Verlauf und stärkstem Tag

### Downloader

- Erkennt Links von Spotify, YouTube, SoundCloud, Bandcamp und hunderten
  weiteren Seiten, auch ganze Alben und Playlists
- Ohne Link wird in allen Quellen gleichzeitig gesucht, mit Ausweichkette:
  scheitert eine Quelle, übernimmt die nächste
- Metadaten, Cover und Lyrics werden online nachgeschlagen und beim Import
  in die Datei geschrieben
- Warnung, wenn das Geladene nicht zur Suche passt

### Oberfläche

- Sieben Sprachen: Deutsch, English, Español, Français, Русский, العربية, 中文;
  ab Werk die Sprache des Systems
- Hell und Dunkel, dem Erscheinungsbild des Systems folgend oder fest gewählt
- Frei wählbare Akzentfarbe, eigene Töne über einen Farbwähler

### Technik

- Tauri 2 mit Rust-Backend und SQLite
- Höchstens zwei gleichzeitige yt-dlp-Aufrufe samt Mindestabstand je Quelle,
  gegen die 403-Sperre bei Dauerlast
- Zeitlimits auf allen äußeren Aufrufen, dazu eine Untätigkeitsgrenze
- Installer für deb, rpm, AppImage, exe, msi, dmg (Intel und Apple Silicon),
  ein Arch-Paket und ein Android-APK
- Veröffentlichte Dateien tragen eine OpenPGP-Unterschrift; siehe README

[Unveröffentlicht]: https://github.com/MorgenWebDigital/robify/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/MorgenWebDigital/robify/releases/tag/v0.1.0
