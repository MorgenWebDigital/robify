# Robify

Plattformübergreifender Musikplayer mit lokaler Bibliothek, Playlists, Hörstatistiken
und einem Downloader inklusive Metadaten-Bearbeitung.

**Frontend:** React 19 + TypeScript + Vite + Tailwind CSS 4
**Backend:** Rust (Tauri 2), SQLite, `rodio`/`symphonia` für die Wiedergabe, `lofty` für Tags

---

## Funktionsumfang

### Player

- Songtitel, Cover, Künstlername
- Vorheriger / nächster Titel, Start-Pause
- Wiederholen: aus · gesamte Warteschlange · nur dieser Titel
- Zufallswiedergabe, Warteschlange mit Umsortieren und Entfernen
- Sleeptimer: feste Dauer (Vorgaben oder eigene Minutenzahl) oder „am Ende des Titels“
- Lyrics, zeitsynchron (LRC) mitlaufend und anklickbar zum Springen, sonst als Text
- Vollbildansicht mit großem Cover und Lyrics nebeneinander

### Bibliothek

- Import einzelner Dateien oder ganzer Ordner (mp3, flac, m4a, aac, ogg, opus, wav, aiff, ape, …)
- **Beim Import werden dürftige Angaben nachgeschlagen.** Steht im Titelfeld nur
  ein Dateiname oder fehlen Cover und Lyrics, sucht Robify die richtigen Daten
  online nach. In den Einstellungen abschaltbar, ab Werk an.
- Künstlerseiten mit Gruppierung nach **Singles**, **EPs** und **Alben**
  (automatisch anhand Titelanzahl bzw. Online-Daten, jederzeit manuell korrigierbar)
- **Mehrere Künstler pro Titel**: Haupt- und Gastkünstler werden getrennt geführt.
  Ein Titel taucht bei jedem Beteiligten auf; die Künstlerseite hat einen eigenen
  Abschnitt „Als Gast dabei“. Im Editor trennt ein Semikolon mehrere Namen
  (`A; B`), Kommas und Ampersands bleiben unangetastet, damit Bandnamen wie
  „Earth, Wind & Fire“ heil bleiben. In der Datei landet alles als
  `Haupt feat. Gast` im Künstlerfeld und wird beim Einlesen wieder aufgetrennt.
- **Künstlerseiten mit Profilbild und Beschreibung.** Fehlen die Angaben, holt
  ein Klick auf „Metadaten holen“ Bild und Text von Genius. Über „Bearbeiten“
  lässt sich alles jederzeit ändern, Name, Beschreibung, eigenes Bild aus einer
  Datei, oder ein anderer Vorschlag aus der Online-Suche übernehmen.
- **Releases nachträglich bearbeiten**: Titel, Jahr, Einordnung und Cover,
  wahlweise von Hand oder mit „Angaben online suchen“.
- Playlists anlegen, umbenennen, befüllen, löschen; eigenes Cover aus einer
  Datei, und die Reihenfolge lässt sich per Ziehen ändern
- **Favoriten** stehen als eigene Playlist vorn, ohne dass man sie anlegen muss
- **Kein Titel landet zweimal in der Bibliothek**: Gleicher Künstler, gleicher
  Name und eine Laufzeit im Abstand von höchstens fünf Sekunden gelten als
  derselbe Titel, egal aus welcher Quelle er kommt
- **Löschen ist umkehrbar.** Der Eintrag wird nur ausgeblendet, die Datei wandert
  in den Papierkorb von Robify; über die Meldung lässt sich beides zurückholen.
  Wer die Rückfrage nicht mehr braucht, schaltet sie im Dialog ab
- Sortierung nach zuletzt hinzugefügt, Titel, Künstler, Album oder Jahr. Nach
  Künstler heißt: alphabetisch, darin die Releases von neu nach alt, darin die
  Titelnummern
- **Abgleich mit dem Ordner**: meldet Dateien ohne Eintrag und Einträge ohne
  Datei, fasst aber nichts ohne Zustimmung an
- Volltextsuche, Cover direkt aus der Datenbank

### Statistiken

- Wöchentlicher Empfehlungsmix, der sich aus dem Hörverhalten speist
  (meistgehörte Künstler, passende Genres, lange nicht Gehörtes, unentdeckte Titel).
  Der Mix bleibt eine Kalenderwoche stabil und lässt sich manuell neu berechnen.
- Wrapped für **Monat**, **Jahr** und **gesamt**: Gesamthörzeit, Wiedergaben,
  Top 5 Titel mit Anzahl und Einzelzeit **plus Gesamtzeit der Top 5**,
  Top-Künstler, Top-Releases, Verlaufsdiagramm und stärkster Tag
- **Gastbeiträge zählen mit.** Wer „Money Trees“ hört, hört Kendrick Lamar und
  Jay Rock; beide bekommen die Hörzeit gutgeschrieben
- Gelöschte Titel bleiben im Rückblick stehen und knüpfen wieder an, sobald sie
  zurückkehren, die Hörgeschichte reißt nicht ab, nur weil eine Datei ging
- In den Einstellungen wählbar: immer der laufende Zeitraum, immer der letzte
  abgeschlossene Monat, immer das letzte abgeschlossene Jahr, oder ausgeblendet

### Downloader

- **Ein einziges Eingabefeld.** Link einfügen oder einfach suchen, Robify
  erkennt selbst, was gemeint ist. Keine Quelle auswählen.
- Erkannt werden Links von Spotify, YouTube, SoundCloud, Bandcamp und
  hunderten weiteren Seiten (alles, was `yt-dlp` kennt)
- Ganze Alben, Playlists und SoundCloud-Sets werden zur Titelliste
  aufgeklappt; ein einzelner Titel bleibt einzeln, auch wenn im Link noch
  eine Playlist mitsteht
- Bei gleicher Passgenauigkeit entscheidet die Verlässlichkeit der Quelle:
  **Bandcamp** (von Künstlern selbst hochgeladen) vor **Audius** (offene
  Plattform, MP3 bis 320 kbit/s) vor **SoundCloud** vor **YouTube**.
  Bandcamp und Audius brauchen keinen Zugangsschlüssel.
- **Bei Spotify-Links wird die passendste Aufnahme gesucht.** Statt blind den
  ersten Treffer zu nehmen, holt Robify mehrere von SoundCloud und YouTube und
  wählt anhand der von Spotify bekannten Laufzeit aus. Abweichungen über eine
  halbe Minute fliegen raus, Zusätze wie „Remix“, „Live“, „Sped Up“ oder
  „Cover“ werden abgestraft, es sei denn, sie stehen im gesuchten Titel.
- **Vorschauen werden abgewiesen.** SoundCloud gibt für kostenpflichtige Titel
  nur 30-Sekunden-Ausschnitte heraus; solche Formate werden ausgeschlossen und
  der Download weicht auf die nächste Quelle aus. Ist die Länge des Titels
  bekannt (etwa aus einem Spotify-Link), wird das Ergebnis zusätzlich dagegen
  geprüft.
- **Spotify**: Track-, Album-, Playlist- und Künstler-Links werden aufgelöst.
  Spotify gibt seine Aufnahmen ausschließlich verschlüsselt (DRM) heraus, ein
  direkter Download ist technisch unmöglich. Übernommen werden deshalb die
  **Metadaten** (Titel, Künstler, Album, Titelnummer, Jahr, Cover); die Audiospur
  wird passend dazu über die übrigen Quellen geladen. Das ist derselbe Weg, den
  auch spotdl geht. Kein API-Schlüssel nötig.
  Spotify unterscheidet keine Gastbeiträge, dort stehen alle Beteiligten
  gleichberechtigt in einer Liste (getrennt mit Komma + geschütztem
  Leerzeichen). Robify trennt diese Liste auf und zieht ein „(feat. …)“ aus dem
  Titel; die eigentliche Rollenverteilung liefert anschließend Genius.
- **Beste Qualität ist Standard**: die Originalspur wird aus dem Container
  gelöst, ohne sie neu zu kodieren (bei YouTube meist Opus ~130 kbit/s).
  Wer feste Dateiformate braucht, stellt MP3, M4A, FLAC oder OGG ein,
  dann wird umgewandelt.
- Fortschritt live mit Geschwindigkeit und Restzeit, Abbrechen jederzeit möglich
- **Nach dem Download werden die Metadaten automatisch nachgeschlagen.** Was aus
  einer Videobeschreibung kommt, ist meist grob: alle Künstler in einem Feld,
  kein Album, keine Lyrics. Robify sucht den Titel deshalb online und übernimmt
  die sauberen Angaben, aber nur bei einem eindeutigen Treffer, sonst bleibt
  alles wie aus der Datei. Ob Cover und Lyrics dabei geholt werden, steuern die
  beiden Schalter in den Einstellungen.
- **Das Konto, von dem geladen wurde, wird zum Hauptkünstler**, alle weiteren
  Beteiligten werden zu Gastkünstlern. Das greift nur, wenn der Kanal wirklich
  einem der Beteiligten entspricht, bei Label-, Sampler- oder Repost-Kanälen
  bleibt die Reihenfolge der Metadatenquelle. Automatisch erzeugte Kanäle
  („PA69 - Topic“, „RihannaVEVO“) werden dabei erkannt.
- Danach öffnet sich die Metadatenprüfung: Titel, Künstler, Gastkünstler,
  Album-Künstler, Album, Art der Veröffentlichung, Genre, Jahr, Titel-/CD-Nummer,
  Cover und Lyrics. Alles ist vorausgefüllt, übernehmen oder vorher ändern.
  Bei „Alle laden“ entfällt die Rückfrage, sonst wären es zwölf Dialoge pro Album.
- Metadatensuche über **Genius** (Hauptquelle), dahinter iTunes und MusicBrainz.
  Ein Klick auf einen Treffer holt automatisch alles nach:
  Gastkünstler ins eigene Feld, Albumname und Albumkünstler, Cover in 1000×1000,
  Jahr und Genre, die **Titelnummer** sowie die **Release-Art**, Single, EP oder
  Album, abgeleitet aus der Länge der Albumtitelliste. Dazu die **Lyrics**:
  zeitsynchron von LRCLIB, der Fließtext von Genius. Kein API-Schlüssel nötig.
- Beim Übernehmen wird die Datei getaggt und nach
  `Bibliothek/Künstler/Album/01 - Titel.ext` einsortiert.
- **Bereits vorhandene Titel werden übersprungen.** Wer erst einen Titel und
  später das Album dazu lädt, wartet nur auf den Rest.
- **Aus einer geladenen Playlist entsteht eine Playlist.** Beim zweiten Durchlauf
  kommen nur die neuen Titel hinzu, statt dass eine zweite Liste entsteht. Über
  die Einstellungen abschaltbar.
- **Passt das Ergebnis nicht zur Suche, sagt Robify das.** Kommen die Wörter der
  Eingabe im geladenen Titel nicht vor, steht eine Warnung im Metadatendialog,
  statt dass eine fremde Aufnahme stillschweigend in der Bibliothek landet.

> Der Downloader ist ein Werkzeug ohne eigene Inhalte. Ob du eine bestimmte Quelle
> herunterladen darfst, richtet sich nach deren Nutzungsbedingungen und dem
> Urheberrecht, das liegt in deiner Verantwortung.

---

## Voraussetzungen

| Werkzeug      | Zweck                                            |
| ------------- | ------------------------------------------------ |
| Node.js ≥ 20  | Frontend                                         |
| Rust (stable) | Backend                                          |
| `ffmpeg`      | Formatumwandlung und Cover-Einbettung (optional) |

`yt-dlp` muss nicht installiert werden: Robify lädt es beim ersten Download
selbst herunter und legt es ins App-Datenverzeichnis. Ein bereits vorhandenes
`yt-dlp` im Suchpfad wird bevorzugt.

### Systempakete

**Fedora / RHEL**

```bash
sudo dnf install webkit2gtk4.1-devel gtk3-devel alsa-lib-devel \
                 openssl-devel curl wget file libappstream-glib rpm-build
```

**Debian / Ubuntu**

```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libasound2-dev \
                 librsvg2-dev patchelf build-essential curl wget file libssl-dev
```

**Arch**

```bash
sudo pacman -S webkit2gtk-4.1 gtk3 alsa-lib openssl base-devel
```

**Windows**: Visual Studio Build Tools (C++) und WebView2 (ab Windows 10 vorinstalliert)
**macOS**: Xcode Command Line Tools

---

## Entwicklung

```bash
npm install
npm run app:dev      # Tauri-Fenster mit Hot Reload
```

Nur das Frontend im Browser (ohne Backend-Funktionen):

```bash
npm run dev
```

---

## Tests

```bash
npm test                       # Oberfläche
cd src-tauri && cargo test     # Backend
node scripts/deutsch-finden.mjs   # deutscher Text ohne t()
```

Die Prüfungen der Oberfläche zerfallen in drei Gruppen:

- **Rechnung** (`lib/`): Formatierung, Sortierung, Farben, LRC. Kein Dokument
  nötig.
- **Übersetzung** (`lib/deckung.test.ts`): Jeder Begriff der Oberfläche und
  jede Fehlermeldung des Rust-Teils muss in allen sieben Sprachen vorliegen,
  jedes gezählte Wort in allen Beugungen. Zusätzlich darf kein `t()` auf
  Modulebene stehen, sonst friert die Beschriftung beim Sprachwechsel ein.
- **Bauteile** (`components/*.test.tsx`): laufen in einem nachgebauten
  Dokument (`jsdom`). Sie decken die Klasse Fehler ab, die sich nur dort
  zeigt: ein Fokus, der beim Tippen aus dem Feld springt, oder ein Menü, das
  die Bildlauffläche verlängert.

`scripts/deutsch-finden.mjs` geht die andere Richtung als die Deckungswache:
Diese prüft, ob jeder übersetzte Begriff eine Fassung hat, jener sucht Text,
den niemand in `t()` gehüllt hat. Er läuft bei jedem Testlauf mit.

Beide Läufe kommen ohne Netz aus. Was eine Verbindung braucht, ist mit
`#[ignore]` versehen und wird gesondert gestartet:

```bash
cd src-tauri
cargo test --test metadata_live -- --ignored --test-threads=1
cargo test --test download_live -- --ignored --test-threads=1
```

Daneben liegen Berichte statt Prüfungen (`metadata_report`, `match_report`,
`audio_report`): Sie behaupten nichts, sondern messen die Trefferquote über
viele echte Titel und schreiben sie auf.

Bei jedem Push läuft `.github/workflows/ci.yml`: Typen, Formatierung, beide
Testläufe und Clippy. Die Installer entstehen erst beim Release, siehe
[CHANGELOG.md](CHANGELOG.md) für die Fassungen.

---

## Installer bauen

### Alles, was auf dem aktuellen System möglich ist

```bash
npm run app:build
```

Ergebnisse liegen unter `src-tauri/target/release/bundle/`.

### Version hochziehen

Die Nummer steht an vier Stellen, jede wird von einem anderen Werkzeug
gelesen. Ein Befehl setzt sie überall:

```bash
npm run version -- 0.2.0
```

Ohne Argument prüft derselbe Befehl nur. Er läuft bei jedem `npm run build`
mit und bricht ab, sobald die Stellen auseinandergehen, sonst nennt sich das
Arch-Paket irgendwann anders als der Rest, und auffallen würde es erst nach
der Veröffentlichung.

### Gezielt einzelne Formate

| Ziel          | Befehl                                                    | Bauen auf |
| ------------- | --------------------------------------------------------- | --------- |
| `.deb`        | `npm run tauri build -- --bundles deb`                    | Linux     |
| `.rpm`        | `npm run tauri build -- --bundles rpm`                    | Linux     |
| `.AppImage`   | `NO_STRIP=true npm run tauri build -- --bundles appimage` | Linux     |
| `.exe` (NSIS) | `npm run tauri build -- --bundles nsis`                   | Windows   |
| `.msi`        | `npm run tauri build -- --bundles msi`                    | Windows   |
| `.dmg`        | `npm run tauri build -- --bundles dmg`                    | macOS     |

> **`NO_STRIP=true` beim AppImage.** Das Werkzeug `linuxdeploy`, das Tauri
> dafür nachlädt, bringt ein eigenes, mehrere Jahre altes `strip` mit. Es kennt
> die komprimierten Relokationen (`.relr.dyn`) nicht, mit denen aktuelle
> Distributionen ihre Bibliotheken ausliefern, und bricht bei jeder einzelnen
> ab. Die Umgebungsvariable überspringt diesen Schritt; das AppImage wird
> dadurch größer (rund 100 MB statt etwa 60), läuft aber einwandfrei. Auf der
> älteren Basis des Release-Ablaufs (Ubuntu 22.04) tritt das Problem nicht auf,
> dort ist die Variable unnötig.

Betriebssysteme lassen sich nicht sinnvoll gegenseitig cross-kompilieren,
für alle Installer auf einmal gibt es den Workflow
[`.github/workflows/release.yml`](.github/workflows/release.yml).
Er baut deb, rpm, AppImage, exe, msi, dmg (Intel und Apple Silicon),
das Arch-Paket und die Android-APK und lädt alles als Artefakte hoch.

### Arch Linux (`.pkg.tar.zst`)

Tauri kennt kein pacman-Ziel, deshalb liegt ein PKGBUILD bei:

```bash
cd packaging/arch
makepkg -f
sudo pacman -U robify-*.pkg.tar.zst
```

> Arch-Pakete heißen `.pkg.tar.zst`, eine Endung `.pacman` gibt es nicht.

### Android (`.apk`)

Vorbereitung: Android SDK + NDK, `JAVA_HOME`, `ANDROID_HOME` und `NDK_HOME` setzen.

```bash
rustup target add aarch64-linux-android armv7-linux-androideabi \
                  i686-linux-android x86_64-linux-android
npm run android:init
npm run android:dev      # auf Gerät/Emulator testen
npm run android:build    # APK erzeugen
```

Die APK landet unter
`src-tauri/gen/android/app/build/outputs/apk/`.
Für die Veröffentlichung im Play Store muss sie noch signiert werden.

---

## Testgerät

Jeder Push auf `main` baut eine Android-APK und hängt sie an die Vorabversion
`testgeraet`. Ein Aktualisierungsprogramm auf dem Gerät fragt immer dieselbe
Adresse ab und holt sich den neuen Stand.

### Einmalig: Schlüssel anlegen

Android lässt ein Update nur zu, wenn die Unterschrift dieselbe ist wie bei
der installierten Fassung. Deshalb gleich der eigene Schlüssel und nicht die
Debug-Signatur: Sie trägt vom ersten Testbau bis zum Play Store.

```bash
keytool -genkeypair -v \
  -keystore robify-release.keystore \
  -alias robify \
  -keyalg RSA -keysize 4096 -validity 10000 \
  -dname "CN=Robify, O=MorgenWebDigital, C=DE"
```

Der Befehl fragt nach einem Kennwort. Es gehört in den Passwortspeicher, und
die Datei an einen Ort außerhalb des Projekts: **Ohne sie ist der Weg zum Play
Store für diese App zu**, denn eine App lässt sich dort nie mit einem anderen
Schlüssel fortsetzen.

Danach drei Geheimnisse im Repository hinterlegen:

| Geheimnis                   | Inhalt                               |
| --------------------------- | ------------------------------------ |
| `ANDROID_KEYSTORE`          | `base64 -w0 robify-release.keystore` |
| `ANDROID_KEYSTORE_PASSWORD` | das eben gesetzte Kennwort           |
| `ANDROID_KEY_ALIAS`         | `robify`                             |

### Auf dem Gerät

„Unbekannte Apps installieren" für die Quelle erlauben, dann die APK von der
Seite der Vorabversion laden. Für das selbsttätige Aktualisieren eignet sich
[Obtainium](https://github.com/ImranR98/Obtainium): Es beobachtet die
Veröffentlichungen des Repositories und installiert neue Fassungen.

Bei einem **privaten** Repository braucht es dafür einen GitHub-Token in den
Einstellungen von Obtainium, denn Anhänge privater Repositories geben sich
nicht ohne Anmeldung heraus. Klappt das nicht, bleibt der Weg über den
Browser: Auf dem Handy bei GitHub angemeldet lässt sich die APK von Hand
laden.

### Einmalig: Werkzeuge lokal

Für das Bauen auf dem eigenen Rechner braucht es SDK, NDK, ein JDK und die
Rust-Ziele. Fedora liefert nur eine Java-Laufzeit, Gradle braucht aber einen
Übersetzer; deshalb liegt hier ein eigenes JDK neben dem SDK statt im System.

```bash
rustup target add aarch64-linux-android armv7-linux-androideabi \
                  i686-linux-android x86_64-linux-android

# SDK samt NDK, rund 2,4 GB
export ANDROID_HOME="$HOME/Android/Sdk"
sdkmanager --install "platform-tools" "platforms;android-34" \
           "build-tools;34.0.0" "ndk;27.0.12077973"
```

In die `~/.bashrc`:

```bash
export JAVA_HOME="$HOME/Android/jdk"
export ANDROID_HOME="$HOME/Android/Sdk"
export NDK_HOME="$ANDROID_HOME/ndk/27.0.12077973"
export PATH="$JAVA_HOME/bin:$ANDROID_HOME/platform-tools:$ANDROID_HOME/cmdline-tools/latest/bin:$PATH"
```

Die Mindestfassung steht auf **API 26** (`bundle.android.minSdkVersion`), nicht
auf der Vorgabe 24: `cpal` bindet AAudio für die Tonausgabe, und die Bibliothek
gibt es erst ab 26. Mit 24 bricht schon das Binden mit
`unable to find library -laaudio` ab.

### Der schnelle Weg daneben

Für das Arbeiten selbst ist der Umweg über GitHub zu langsam. Mit
angeschlossenem Gerät und eingeschalteter USB-Fehlersuche:

```bash
npm run android:dev
```

Änderungen an der Oberfläche erscheinen dann in Sekunden auf dem Gerät. Nötig
sind dafür lokal das Android-SDK, das NDK und die Rust-Ziele; siehe
[Voraussetzungen](#voraussetzungen).

---

## Installer prüfen

Jede veröffentlichte Datei trägt eine abgetrennte OpenPGP-Unterschrift
(`.asc`) daneben, dazu liegt eine unterschriebene `SHA256SUMS` bei.

Signaturschlüssel:

```
MorgenWebDigital (Robify release signing) <info@morgenwebdigital.de>
51C7 35F6 20DF 2108 2405  210D 3049 3374 8411 E706
```

Der öffentliche Schlüssel liegt als
[`packaging/robify-signing-key.asc`](packaging/robify-signing-key.asc) bei.

```bash
gpg --import packaging/robify-signing-key.asc

# Eine einzelne Datei prüfen
gpg --verify Robify_0.1.0_amd64.deb.asc Robify_0.1.0_amd64.deb

# Oder alle auf einmal über die Prüfsummen
gpg --verify SHA256SUMS.asc SHA256SUMS
sha256sum --check --ignore-missing SHA256SUMS
```

`Good signature` zusammen mit dem oben genannten Fingerabdruck heißt: Die
Datei stammt unverändert aus diesem Projekt. Die Warnung, der Schlüssel sei
nicht beglaubigt, ist dabei zu erwarten; sie besagt nur, dass niemand aus
deinem Netz für ihn gebürgt hat. Maßgeblich ist der Fingerabdruck.

Der Ablauf in `.github/workflows/release.yml` unterschreibt am Ende eines
Release-Laufs. Er braucht zwei Geheimnisse im Repository:

| Geheimnis         | Inhalt                                                         |
| ----------------- | -------------------------------------------------------------- |
| `PGP_PRIVATE_KEY` | Ausgabe von `gpg --armor --export-secret-keys <Fingerabdruck>` |
| `PGP_PASSPHRASE`  | Kennwort des Schlüssels, falls eines gesetzt ist               |

---

## Aufbau

```
src/                     React-Oberfläche
  components/            Player, Listen, Dialoge, Metadateneditor
  pages/                 Start, Bibliothek, Künstler, Playlists, Wrapped, Downloader, Einstellungen
  store/                 Zustand für Player, Bibliothek und UI (zustand)
  lib/                   API-Brücke, LRC-Parser, Formatierung, Sortierung, Cover-URLs
src-tauri/src/
  player.rs              Wiedergabe-Thread: Warteschlange, Repeat, Shuffle, Sleeptimer
  library.rs             Alle Datenbankabfragen
  db.rs                  Schema und Migration
  scanner.rs             Datei-/Ordnerimport
  tags.rs                Tags lesen und schreiben
  online.rs              Genius, iTunes, MusicBrainz, Cover Art Archive, LRCLIB
  spotify.rs             Spotify-Links auflösen (nur Metadaten, ohne API-Schlüssel)
  downloader.rs          yt-dlp-Anbindung, Linkerkennung, Mehrquellen-Suche
  stats.rs               Wochenmix und Wrapped
  commands.rs            Schnittstelle zum Frontend
packaging/               Desktop-Datei und Arch-PKGBUILD
```

Daten liegen in einer SQLite-Datenbank im App-Datenverzeichnis
(Pfad steht in den Einstellungen unter „Speicherorte“). Dort liegen auch der
Papierkorb und die Sicherungen; eine Sicherung lässt sich in den Einstellungen
jederzeit anlegen, die letzten fünf bleiben erhalten.
Cover werden als BLOB gespeichert und über ein eigenes `robify:`-Protokoll
an die Oberfläche ausgeliefert, statt sie als Base64 durch die IPC-Brücke zu schicken.

---

## Tastenkürzel

| Taste        | Wirkung           |
| ------------ | ----------------- |
| `Leertaste`  | Abspielen / Pause |
| `Strg` + `→` | Nächster Titel    |
| `Strg` + `←` | Vorheriger Titel  |
| `Esc`        | Dialog schließen  |

---

## Lizenz

Robify selbst steht unter MIT, siehe [LICENSE](LICENSE).

### Fremdbibliotheken

Robify wird mit rund 400 Fremdpaketen ausgeliefert (Rust und npm, ohne
Bauwerkzeuge). Ganz überwiegend MIT und Apache-2.0; dazu MPL-2.0 für Symphonia
und einige kleinere. **Kein GPL, AGPL, LGPL oder SSPL im Baum**, nichts zwingt
also zur Offenlegung eigenen Quelltextes.

MIT und Apache-2.0 verlangen beide, dass Urheberrechtsvermerk und Lizenztext
mitgeliefert werden. Das übernimmt `scripts/lizenzen.mjs`: Es sammelt die
Angaben aus `cargo tree -e normal` und dem npm-Produktivbaum nach
`public/lizenzen.json`, das die App unter _Einstellungen → Rechtliches_ anzeigt.

```bash
npm run lizenzen     # läuft ohnehin bei jedem `npm run build`
```

Bei knapp 30 Paketen liegt dem veröffentlichten Archiv kein Lizenztext bei; dort
zeigt die Liste die SPDX-Angabe und den Verweis auf die Quelle.

### Fremde Inhalte

Robify liefert keine Musik mit. Ob eine bestimmte Aufnahme heruntergeladen
werden darf, richtet sich nach dem Urheberrecht und den Bedingungen der
Plattform. Die Metadatensuche liest öffentlich erreichbare Seiten von Spotify,
Genius und anderen aus; deren Nutzungsbedingungen erlauben das in der Regel
nicht. Beides liegt in der Verantwortung dessen, der das Werkzeug benutzt.
