# Robify

A cross-platform music player with a local library, playlists, listening
statistics and a downloader including metadata editing.

Frontend: React 19, TypeScript, Vite, Tailwind CSS 4.
Backend: Rust (Tauri 2), SQLite, `rodio`/`symphonia` for playback, `lofty` for
tags.

---

## 1. Features

### Player

- title, cover and artist name
- previous and next track, play and pause
- repeat: off, the whole queue, or this track alone
- shuffle, and a queue that can be reordered and thinned out
- sleep timer: a fixed duration, from presets or an own number of minutes, or
  at the end of the running track
- lyrics, running along time-synced (LRC) and clickable to jump, otherwise as
  plain text
- a full screen view with a large cover and the lyrics next to it

### Library

- import of single files or whole folders (mp3, flac, m4a, aac, ogg, opus,
  wav, aiff, ape and more)
- poor details are looked up at import. where nothing but a filename stands in
  the title field, or cover and lyrics are missing, robify searches for the
  right data online. it can be switched off in the settings and is on out of
  the box.
- artist pages grouped into singles, eps and albums, classified automatically
  from the track count or from online data and correctable by hand at any time
- several artists per track: lead and guest artists are kept apart. a track
  shows up under every participant, and the artist page carries a section of
  its own for guest appearances. in the editor a semicolon separates several
  names (`A; B`) while commas and ampersands stay untouched, so band names such
  as "Earth, Wind & Fire" survive intact. in the file everything lands in the
  artist field as `lead feat. guest` and is split apart again while reading.
- artist pages with a profile image and a description. where the details are
  missing, one press fetches image and text from genius. everything can be
  changed at any time through the edit dialog: name, description, an image of
  one's own from a file, or a different suggestion from the online search.
- releases can be edited afterwards: title, year, classification and cover,
  either by hand or through an online lookup
- playlists can be created, renamed, filled and deleted, they take a cover of
  their own from a file, and their order can be changed by dragging
- the favourites stand at the front as a playlist of their own, without having
  to be created
- no track lands in the library twice: the same artist, the same name and a
  running time within five seconds count as the same track, whatever source it
  comes from
- deleting can be undone. the row is only hidden and the file travels into
  robify's trash, and the message brings both back. whoever no longer needs
  the confirmation switches it off in the dialog.
- sorting by recently added, title, artist, album or year. by artist means
  alphabetically, inside that the releases from new to old, inside that the
  track numbers.
- reconciliation with the folder: it reports files without a row and rows
  without a file, and touches nothing without consent
- full text search, and covers served straight out of the database

### Statistics

- a weekly mix fed by the listening behaviour: the most played artists,
  matching genres, what has not been heard for a long time, and undiscovered
  tracks. the mix stays stable for one calendar week and can be recalculated by
  hand.
- a review for the month, the year and for everything: total listening time,
  plays, the top five tracks with count and individual time plus the total time
  of the top five, top artists, top releases, a history chart and the strongest
  day
- guest contributions count. whoever listens to "Money Trees" listens to
  kendrick lamar and jay rock, and both are credited the listening time.
- deleted tracks stay in the review and tie back in as soon as they return, so
  the listening history does not break off just because a file went away
- selectable in the settings: always the running period, always the last
  completed month, always the last completed year, or hidden

### Downloader

- one single input field. paste a link or simply search, robify recognises by
  itself what is meant. no source has to be picked.
- links from spotify, youtube, soundcloud, bandcamp and hundreds of further
  sites are recognised, everything `yt-dlp` knows
- whole albums, playlists and soundcloud sets are unfolded into a track list,
  and a single track stays single even where a playlist stands in the link as
  well
- at equal fit the reliability of the source decides: bandcamp (uploaded by
  the artists themselves) before audius (an open platform, mp3 up to
  320 kbit/s) before soundcloud before youtube. bandcamp and audius need no
  access key.
- with spotify links the best matching recording is searched for. instead of
  taking the first hit blindly, robify fetches several from soundcloud and
  youtube and picks by the running time known from spotify. deviations of more
  than half a minute fly out, and suffixes such as remix, live, sped up or
  cover are penalised unless they stand in the track searched for.
- previews are turned away. soundcloud hands out 30 second excerpts alone for
  paid tracks, such formats are excluded and the download falls back to the
  next source. where the length of the track is known, from a spotify link for
  instance, the result is checked against it as well.
- spotify: track, album, playlist and artist links are resolved. spotify hands
  its recordings out encrypted (drm) only, and a direct download is technically
  impossible. the metadata is therefore taken over (title, artist, album, track
  number, year, cover), and the audio is fetched to match through the remaining
  sources. that is the same route spotdl takes. no api key is needed.
  spotify draws no line around guest contributions, everyone involved stands
  there as an equal in one list, separated by a comma and a non-breaking space.
  robify splits that list apart and pulls a "(feat. …)" out of the title, and
  genius then delivers the actual distribution of roles.
- best quality is the default: the original track is lifted out of the
  container without re-encoding it, at youtube usually opus at around
  130 kbit/s. whoever needs fixed file formats sets mp3, m4a, flac or ogg, and
  it is converted then.
- progress live with speed and time left, and cancelling is possible at any
  moment
- the metadata is looked up automatically after the download. what comes out
  of a video description is usually rough: every artist in one field, no album,
  no lyrics. robify therefore searches for the track online and takes the clean
  details over, but only on an unambiguous hit, otherwise everything stays as
  it came out of the file. whether cover and lyrics come along is steered by
  the two switches in the settings.
- the account downloaded from becomes the lead artist and every further
  participant becomes a guest artist. that takes effect only where the channel
  really matches one of the participants, and with label, sampler or repost
  channels the order of the metadata source stays. automatically generated
  channels ("PA69 - Topic", "RihannaVEVO") are recognised in doing so.
- afterwards the metadata review opens: title, artist, guest artists, album
  artist, album, type of release, genre, year, track and disc number, cover and
  lyrics. everything is pre-filled, to be taken over or changed first. with a
  batch download the confirmation is dropped, otherwise it would be twelve
  dialogs per album.
- the metadata search runs over genius as the main source, with itunes and
  musicbrainz behind it. one press on a hit fetches everything: guest artists
  into their own field, album name and album artist, the cover at 1000 by 1000,
  year and genre, the track number and the release type (single, ep or album,
  derived from the length of the album track list). the lyrics come along too:
  time-synced from lrclib, the running text from genius. no api key is needed.
- on taking it over, the file is tagged and filed into
  `library/artist/album/01 - title.ext`
- tracks already present are skipped. whoever downloads one track first and the
  album to it later waits for the rest alone.
- a downloaded playlist becomes a playlist. on a second run only the new tracks
  are added instead of a second list coming into being. it can be switched off
  in the settings.
- where the result does not match the search, robify says so. do the words of
  the input not appear in the downloaded track, a warning stands in the
  metadata dialog instead of a foreign recording landing in the library
  quietly.

> The downloader is a tool without content of its own. Whether a particular
> source may be downloaded follows from its terms of use and from copyright
> law, and that responsibility lies with whoever uses it.

---

## 2. Installation

### Requirements

| Tool          | Purpose                                         |
| ------------- | ----------------------------------------------- |
| Node.js ≥ 20  | frontend                                        |
| Rust (stable) | backend                                         |
| `ffmpeg`      | format conversion and cover embedding, optional |

`yt-dlp` does not have to be installed: robify downloads it itself at the
first download and puts it into the app data directory. A `yt-dlp` already in
the search path is preferred.

### System packages

```bash
# fedora / rhel
sudo dnf install webkit2gtk4.1-devel gtk3-devel alsa-lib-devel \
                 openssl-devel curl wget file libappstream-glib rpm-build

# debian / ubuntu
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libasound2-dev \
                 librsvg2-dev patchelf build-essential curl wget file libssl-dev

# arch
sudo pacman -S webkit2gtk-4.1 gtk3 alsa-lib openssl base-devel
```

On Windows the Visual Studio Build Tools (C++) and WebView2 are needed, the
latter pre-installed from Windows 10 on. On macOS the Xcode command line tools
are needed.

### Building the installers

```bash
# everything possible on the current system.
# the results end up under src-tauri/target/release/bundle/
npm run app:build
```

Operating systems cannot sensibly cross-compile for each other. For every
installer at once there is the workflow
[`.github/workflows/release.yml`](.github/workflows/release.yml). It builds
deb, rpm, appimage, exe, msi, dmg for intel and apple silicon, the arch package
and the android apk, and uploads all of it as artifacts.

| Target        | Command                                                   | Build on |
| ------------- | --------------------------------------------------------- | -------- |
| `.deb`        | `npm run tauri build -- --bundles deb`                    | Linux    |
| `.rpm`        | `npm run tauri build -- --bundles rpm`                    | Linux    |
| `.AppImage`   | `NO_STRIP=true npm run tauri build -- --bundles appimage` | Linux    |
| `.exe` (NSIS) | `npm run tauri build -- --bundles nsis`                   | Windows  |
| `.msi`        | `npm run tauri build -- --bundles msi`                    | Windows  |
| `.dmg`        | `npm run tauri build -- --bundles dmg`                    | macOS    |

> `NO_STRIP=true` for the appimage. The tool `linuxdeploy`, which tauri fetches
> for it, brings a `strip` of its own that is several years old. It does not
> know the compressed relocations (`.relr.dyn`) current distributions ship
> their libraries with and breaks off on every single one. The environment
> variable skips that step, which makes the appimage larger, around 100 MB
> instead of about 60, but it runs faultlessly. On the older base of the release
> workflow (Ubuntu 22.04) the problem does not occur and the variable is
> unnecessary.

### Arch Linux

Tauri knows no pacman target, so a PKGBUILD ships along. Arch packages are
called `.pkg.tar.zst`, an extension `.pacman` does not exist.

```bash
cd packaging/arch
makepkg -f
sudo pacman -U robify-*.pkg.tar.zst
```

### Android

The android sdk and ndk are needed, with `JAVA_HOME`, `ANDROID_HOME` and
`NDK_HOME` set. The minimum version stands at api 26 and not at the default 24:
`cpal` binds aaudio for the audio output, and that library exists only from 26
on. With 24 the linking already breaks off with `unable to find library
-laaudio`.

```bash
# the rust targets, once
rustup target add aarch64-linux-android armv7-linux-androideabi \
                  i686-linux-android x86_64-linux-android

# sdk including ndk, around 2.4 gb
export ANDROID_HOME="$HOME/Android/Sdk"
sdkmanager --install "platform-tools" "platforms;android-34" \
           "build-tools;34.0.0" "ndk;27.0.12077973"

npm run android:init
npm run android:dev      # test on a device or emulator
npm run android:build    # build the apk
```

Two things are built alongside and are looked for by the retrofit script:

```bash
uv python install 3.11              # chaquopy runs pip with it
node scripts/quickjs-bauen.mjs      # the javascript runtime, once per version
node scripts/ffmpeg-bauen.mjs       # the converter, likewise once
```

Python 3.11 and not something newer: chaquopy carries `armeabi-v7a` and `x86`
up to that version only, and with a newer one every 32-bit device would lose
the app. Any python 3.11 does — from the package manager, from `uv`, from
anywhere on the path. `npm run android:build` calls both builds itself; they
do nothing when the versions in place already match.

FFmpeg takes a few minutes per architecture and is built without its gpl
parts — those are video encoders, and robify converts audio only. The result
weighs some five megabytes per architecture where the ready-made android
builds bring thirty-five.

The apk ends up under `src-tauri/gen/android/app/build/outputs/apk/`. For a
release in the play store it still has to be signed.

Fedora ships a java runtime alone while gradle needs a compiler, so a jdk of
its own lies next to the sdk instead of in the system:

```bash
# into ~/.bashrc
export JAVA_HOME="$HOME/Android/jdk"
export ANDROID_HOME="$HOME/Android/Sdk"
export NDK_HOME="$ANDROID_HOME/ndk/27.0.12077973"
export PATH="$JAVA_HOME/bin:$ANDROID_HOME/platform-tools:$ANDROID_HOME/cmdline-tools/latest/bin:$PATH"
```

### Checking the installers

A signed `SHA256SUMS` ships with every release. It holds the checksum of every
published file, so one signature covers them all. The signing key:

```
MorgenWebDigital (Robify release signing) <info@morgenwebdigital.de>
51C7 35F6 20DF 2108 2405  210D 3049 3374 8411 E706
```

The public key ships as
[`packaging/robify-signing-key.asc`](packaging/robify-signing-key.asc).

```bash
gpg --import packaging/robify-signing-key.asc

# the signature vouches for the checksum file
gpg --verify SHA256SUMS.asc SHA256SUMS

# and the checksum file for whatever was downloaded
sha256sum --check --ignore-missing SHA256SUMS
```

Both steps together, and in this order. A checksum file on its own proves
nothing: whoever can exchange an installer can exchange the checksums next to
it. Only the signature ties them to this key.

`Good signature` together with the fingerprint above means the file comes from
this project unchanged. The warning that the key is not certified is to be
expected, it only says that nobody in the local web of trust has vouched for
it. The fingerprint is what counts.

---

## 3. Usage

### Development

```bash
npm install

# the tauri window with hot reload
npm run app:dev

# the frontend alone in a browser, without the backend features
npm run dev
```

### Tests

```bash
npm test                          # the ui
cd src-tauri && cargo test        # the backend
node scripts/deutsch-finden.mjs   # german text without t()
```

The checks of the ui fall into three groups:

- calculation (`lib/`): formatting, sorting, colours, lrc. no document needed.
- translation (`lib/deckung.test.ts`): every term of the ui and every error
  message of the rust side has to exist in all seven languages, and every
  counted word in all of its inflections. on top of that no `t()` may stand at
  module level, otherwise the label freezes at a language switch.
- components (`components/*.test.tsx`): these run in a rebuilt document
  (`jsdom`). they cover the class of bug that shows only there: a focus jumping
  out of a field while typing, or a menu extending the scrolling area.

`scripts/deutsch-finden.mjs` goes the other direction than the coverage guard:
that one checks whether every translated term has a version, this one looks for
text nobody wrapped in `t()`. It runs along with every test run.

Both runs get by without a network. What needs a connection carries `#[ignore]`
and is started separately:

```bash
cd src-tauri
cargo test --test metadata_live -- --ignored --test-threads=1
cargo test --test download_live -- --ignored --test-threads=1
```

Next to them lie reports rather than checks (`metadata_report`,
`match_report`, `audio_report`): they claim nothing but measure the hit rate
over many real tracks and write it down.

At every push `.github/workflows/ci.yml` runs: types, formatting, both test
runs and clippy. The installers come into being at release, see
[CHANGELOG.md](CHANGELOG.md) for the versions.

### Keyboard shortcuts

| Key          | Effect         |
| ------------ | -------------- |
| `Space`      | play and pause |
| `Ctrl` + `→` | next track     |
| `Ctrl` + `←` | previous track |
| `Esc`        | close a dialog |

### Updating in place

Robify carries the updater of tauri. Where the app is a self-contained thing
it fetches the new version itself, checks its signature, puts it in place of
the running one and starts anew: on windows, on macos, and in the appimage on
linux.

A deb, an rpm and the arch package are left out, and rightly so. Their files
belong to the manager that installed them, and exchanging those behind its
back would leave it with a bookkeeping that no longer matches the disk. There
the button leads to the page the new version lies on. Android is left out as
well; whatever installed the apk fetches the new one.

The key that signs the updates is generated once and kept:

```bash
npx tauri signer generate -w ~/.tauri/robify.key
```

The public half goes into `src-tauri/tauri.conf.json` under
`plugins.updater.pubkey`, the private half into the repository secrets. Losing
it means no further update reaches anyone who installed the app: the signature
of a new key does not match the old one, and robify refuses it. Whoever holds
it can replace robify on every machine that runs it.

### The test device

Every push to `main` builds an android apk and attaches it to the pre-release
`testgeraet`. An updater on the device polls the same address every time and
fetches the new state. [Obtainium](https://github.com/ImranR98/Obtainium) is
suited to it: it watches the releases of a repository and installs new
versions. Installing from unknown sources has to be allowed for the source on
the device.

That pre-release does not lie in this repository but in a private one of its
own, named in the secret `TESTGERAET_REPO`. A test build stands next to the
released version otherwise, installable and half finished, and a stranger can
hardly tell the two apart. The other repository needs nothing but a first
commit; the workflow writes only the release into it.

The workflow reaches it with `TESTGERAET_TOKEN`, a token of its own, as the
token of a run never leaves the repository it runs in. A fine-grained token
with `Contents: read and write` on that one repository is enough. Obtainium in
turn needs a github token in its settings, as the assets of a private
repository are not handed out without authentication.

For the work itself the detour through github is too slow. With a device
attached and usb debugging switched on, `npm run android:dev` puts changes to
the ui onto the device within seconds.

---

## 4. Configuration

The settings page holds the appearance (accent colour, light and dark, ui
language), the storage locations, the scope of the review, the switches for the
automatic lookups, and a button for a backup of the database.

### Storage locations

The data lies in a sqlite database in the app data directory, and its path
stands in the settings. The trash and the backups lie there too. A backup can be
made in the settings at any time, and the last five are kept.

On a phone the locations are fixed: the tracks lie in `Robify` and everything
else in `.robify`, both directly in the device storage. They are visible in the
file manager there and survive the removal of the app, unlike everything under
`Android/data`. The folder choice is therefore hidden there. `Robify/Eigene
Songs` is the way in for music that does not come through the downloader, and
what is dropped in is read at the next start.

### Version numbers

The number stands in four places, and each one is read by a different tool. One
command sets it everywhere:

```bash
# without an argument the same command only checks
npm run version -- 0.2.0
```

The check runs along with every `npm run build` and stops as soon as the places
drift apart, otherwise the arch package calls itself differently from the rest
at some point, and it would show only after the release.

### Repository secrets

The release workflow signs at the end of a release run and the test device
workflow signs the apk. Both need secrets in the repository:

| Secret                               | Content                                                    |
| ------------------------------------ | ---------------------------------------------------------- |
| `PGP_PRIVATE_KEY`                    | output of `gpg --armor --export-secret-keys <fingerprint>` |
| `PGP_PASSPHRASE`                     | password of the key, where one is set                      |
| `ANDROID_KEYSTORE`                   | `base64 -w0 robify-release.keystore`                       |
| `ANDROID_KEYSTORE_PASSWORD`          | the password set for the keystore                          |
| `ANDROID_KEY_ALIAS`                  | `robify`                                                   |
| `TESTGERAET_REPO`                    | the private repository for the test builds, `owner/name`   |
| `TAURI_SIGNING_PRIVATE_KEY`          | output of `npx tauri signer generate`, the private half    |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | the password set for that key                              |
| `TESTGERAET_TOKEN`                   | a token with write access to that repository               |

The last two belong to the test device alone, see the section on it. Kept as a
secret rather than as a variable, the name of the private repository does not
stand in the public log of a run either.

Android allows an update only where the signature is the same as on the
installed version, hence a key of one's own from the first test build rather
than the debug signature. It carries all the way to the play store, and without
it the way there is shut for this app, as an app can never be continued there
with a different key.

```bash
# create the key once, keep it outside the project and the password in a
# password manager
keytool -genkeypair -v \
  -keystore robify-release.keystore \
  -alias robify \
  -keyalg RSA -keysize 4096 -validity 10000 \
  -dname "CN=Robify, O=MorgenWebDigital, C=DE"
```

---

## 5. Project structure

```
src/                     the react ui
  components/            player, lists, dialogs, metadata editor
  pages/                 home, library, artists, playlists, review, downloader, settings
  store/                 state for player, library and ui (zustand)
  lib/                   api bridge, lrc parser, formatting, sorting, cover urls
src-tauri/src/
  player.rs              the playback thread: queue, repeat, shuffle, sleep timer
  library.rs             every database query
  db.rs                  schema and migrations
  scanner.rs             file and folder import
  tags.rs                reading and writing tags
  online.rs              genius, itunes, musicbrainz, cover art archive, lrclib
  spotify.rs             resolving spotify links, metadata only, no api key
  downloader.rs          the yt-dlp binding, link detection, multi-source search
  stats.rs               the weekly mix and the review
  commands.rs            the interface to the frontend
packaging/               the desktop file and the arch pkgbuild
scripts/                 version check, licence collection, android retrofit
```

Covers are stored as blobs and served to the ui over a custom `robify:` scheme
instead of being pushed through the ipc bridge as base64.

---

## 6. License

Robify is licensed under the PolyForm Noncommercial 1.0.0, see
[LICENSE](LICENSE). In one sentence: using, changing and passing it on is
allowed to everybody as long as the purpose is not commercial, so privately, in
research and teaching, at non-profit bodies and public authorities. Nobody may
sell robify, and using it in a business is not allowed either.

Whoever wants to all the same turns to MorgenWebDigital. The commercial rights
lie there entirely, and a separate licence or a sale is possible at any time
without anybody else having to be asked.

Robify is therefore source-available but not open source in the sense of the
OSI, whose definition demands that commercial use be allowed as well. In
practice that means f-droid does not take the app in and some distributions do
not take it into their package sources. Releases of our own, the arch package
and the apk are untouched by it.

### Third-party libraries

Robify ships with around 400 third-party packages (rust, npm and, for the apk,
what gradle and pip add). Overwhelmingly mit and apache-2.0, plus mpl-2.0 for
symphonia and a few smaller ones. There is no gpl, agpl, lgpl or sspl in the
tree, so nothing forces the disclosure of source code of our own.

That holds for the android build as well, and it did not always. Until the
python runtime was exchanged the apk carried `youtubedl-android` and its
ffmpeg, both gpl-3.0, and the gpl forbids further restrictions — a
non-commercial clause is one. The apk could not have been passed on that way.
In its place stand chaquopy (mit) with a python of its own, yt-dlp (unlicense)
installed into it by pip, quickjs (mit) as the javascript runtime youtube
demands, and an ffmpeg built here without its gpl parts (lgpl), with lame
(lgpl) for mp3 and libvorbis (bsd) for ogg. The lgpl asks that whoever gets the program can exchange those
parts, so they lie beside it as separate shared libraries rather than being
built into one binary.

All three expressly allow the whole to be passed on under different terms, so
the licence of robify may be stricter than theirs. Mpl-2.0 acts on the files of
symphonia itself alone: are those changed, exactly those files stay under mpl
and their source has to stay open.

Mit and apache-2.0 both demand that copyright notice and licence text ship
along. `scripts/lizenzen.mjs` takes care of that: it collects the details from
`cargo tree -e normal` and from the npm production tree into
`public/lizenzen.json`, which the app displays under settings, legal. What the
apk adds beyond that stands written out in the same script, and it refuses to
run as soon as the android build file names a dependency nobody has looked at.

A list is only ever as good as what somebody entered into it, though.
`scripts/apk-lizenzen.mjs` therefore reads the other way round: out of the
finished apk, where chaquopy leaves the metadata of every python package, and
it stops the build on anything gpl. It earned its keep straight away —
`yt-dlp[default]` had quietly brought mutagen along, and that is gpl.

```bash
# runs with every `npm run build` anyway
npm run lizenzen
```

With close to 30 packages no licence text ships in the published archive, and
the list shows the spdx value and the pointer to the source there.

### Third-party content

Robify ships no music. Whether a particular recording may be downloaded follows
from copyright law and from the terms of the platform. The metadata search
reads publicly reachable pages of spotify, genius and others, and their terms of
use generally do not allow that. Both lie in the responsibility of whoever uses
the tool.
