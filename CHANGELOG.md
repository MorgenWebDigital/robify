# Changelog

This file follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
the versions follow [semantic versioning](https://semver.org/).

## [Unreleased]

## [0.1.0] – not released yet

The first version.

### Added

- local playback through `rodio`/`symphonia`: mp3, flac, m4a, aac, ogg, opus,
  wav, aiff, ape
- a queue that can be reordered by dragging, shuffle, and repeat of one track
  or of the list
- a sleep timer by minutes or until the end of the running track
- lyrics running along time-synced (lrc) and clickable to jump, otherwise as
  plain text, with an editor of its own for setting the timestamps
- import of single files or whole folders, with a progress display
- artist pages separated into singles, eps and albums, correctable by hand at
  any time
- several artists per track, guest contributions included
- playlists with a cover of their own, reorderable by dragging
- favourites without a playlist of their own
- reconciliation with the library folder: orphaned files and missing rows are
  reported, never deleted quietly
- a backup of the database through `VACUUM INTO`, the last five are kept
- a weekly mix of the thirty most played tracks of the week
- a review by month, year or all time, with a history and the strongest day
- link detection for spotify, youtube, soundcloud, bandcamp and hundreds of
  further sites, whole albums and playlists included
- a search across every source at once where no link is given, with a fallback
  chain: does one source fail, the next takes over
- metadata, cover and lyrics looked up online and written into the file at
  import
- a warning where the download does not match the search
- seven languages: Deutsch, English, Español, Français, Русский, العربية, 中文,
  out of the box the language of the system
- light and dark, following the appearance of the system or fixed by choice
- a freely selectable accent colour, with own tones through a colour picker
- tauri 2 with a rust backend and sqlite
- at most two yt-dlp calls at a time plus a minimum distance per source,
  against the 403 block under continuous load
- timeouts on every outward call, plus an idle bound
- installers for deb, rpm, appimage, exe, msi, dmg (intel and apple silicon),
  an arch package and an android apk
- donation addresses in seven cryptocurrencies, copied by a click, shown
  only where one is entered
- a mark beside the settings as soon as a newer robify or a newer yt-dlp
  exists, and nothing at all while everything is up to date
- renewal in place where the app is a self-contained thing: windows, macos and
  the appimage. a deb, an rpm, the arch package and android belong to whatever
  installed them, and there the way leads to the release page
- the notes of a release are read out of the changelog, so what changed can be
  read in the app
- a copy of yt-dlp of its own where the one that was found belongs to pip or
  to a package manager and will not renew itself
- an openpgp signature on every published file, see the readme

[Unreleased]: https://github.com/MorgenWebDigital/robify/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/MorgenWebDigital/robify/releases/tag/v0.1.0
