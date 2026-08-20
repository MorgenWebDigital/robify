//! integration test over real audio files: writing and reading tags,
//! importing, classifying releases, playlists and evaluations.
//!
//! note: needs `ffmpeg` in the path to create the test files.

use robify_lib::models::{ReleaseType, TrackMetadata};
use robify_lib::{db, library, scanner, stats, tags};
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::process::Command;

fn ffmpeg_available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// creates a silent mp3 file of the wanted length.
fn make_mp3(dir: &Path, name: &str, seconds: u32) -> PathBuf {
    let path = dir.join(format!("{name}.mp3"));
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "anullsrc=r=44100:cl=stereo",
            "-t",
            &seconds.to_string(),
            "-b:a",
            "64k",
        ])
        .arg(&path)
        .output()
        .expect("ffmpeg startet");
    assert!(status.status.success(), "ffmpeg konnte {name} nicht erzeugen");
    path
}

/// creates a silent file in the wanted format.
fn make_audio(dir: &Path, name: &str, extension: &str, seconds: u32) -> PathBuf {
    let path = dir.join(format!("{name}.{extension}"));
    let output = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "anullsrc=r=48000:cl=stereo",
            "-t",
            &seconds.to_string(),
        ])
        .arg(&path)
        .output()
        .expect("ffmpeg startet");
    assert!(output.status.success(), "ffmpeg konnte {name}.{extension} nicht erzeugen");
    path
}

fn metadata(title: &str, artist: &str, album: Option<&str>, track_no: Option<i64>) -> TrackMetadata {
    TrackMetadata {
        title: title.into(),
        artist: artist.into(),
        featured_artists: None,
        album: album.unwrap_or("").into(),
        album_artist: None,
        release_type: None,
        year: Some(2024),
        track_no,
        disc_no: Some(1),
        genre: Some("Testgenre".into()),
        cover_base64: None,
        cover_mime: None,
        lyrics_synced: None,
        lyrics_plain: None,
    }
}

fn setup_db(dir: &Path) -> Connection {
    let conn = db::open(&dir.join("test.db")).expect("Datenbank öffnet");
    db::migrate(&conn).expect("Migration läuft");
    conn
}

#[test]
fn tags_werden_geschrieben_und_wieder_gelesen() {
    if !ffmpeg_available() {
        eprintln!("ffmpeg fehlt, Test übersprungen");
        return;
    }
    let dir = tempdir("tags");
    let path = make_mp3(&dir, "probe", 2);

    // a 1x1 png as the cover, so the image path is covered too
    let png = base64_png();
    let mut meta = metadata("Nachtfahrt", "Testkünstler", Some("Testalbum"), Some(3));
    meta.cover_base64 = Some(png.clone());
    meta.cover_mime = Some("image/png".into());
    meta.lyrics_synced = Some("[00:01.00] Erste Zeile\n[00:04.50] Zweite Zeile".into());

    tags::write(&path, &meta).expect("Tags schreiben");
    let read = tags::read(&path).expect("Tags lesen");

    assert_eq!(read.metadata.title, "Nachtfahrt");
    assert_eq!(read.metadata.artist, "Testkünstler");
    assert_eq!(read.metadata.album, "Testalbum");
    assert_eq!(read.metadata.track_no, Some(3));
    assert_eq!(read.metadata.year, Some(2024));
    assert_eq!(read.metadata.genre.as_deref(), Some("Testgenre"));
    assert!(read.metadata.lyrics_synced.is_some(), "LRC muss erhalten bleiben");
    assert!(read.cover.is_some(), "Cover muss erhalten bleiben");
    assert!(read.duration_ms >= 1500, "Dauer wurde nicht erkannt");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn import_gruppiert_nach_single_ep_und_album() {
    if !ffmpeg_available() {
        eprintln!("ffmpeg fehlt, Test übersprungen");
        return;
    }
    let dir = tempdir("import");
    let conn = setup_db(&dir);

    // ep: three tracks on the same album
    for number in 1..=3 {
        let path = make_mp3(&dir, &format!("ep{number}"), 2);
        tags::write(
            &path,
            &metadata(&format!("EP Titel {number}"), "Band Eins", Some("Kurzspieler"), Some(number)),
        )
        .unwrap();
        scanner::import_file(&conn, &path, Some("test")).unwrap();
    }

    // album: seven tracks
    for number in 1..=7 {
        let path = make_mp3(&dir, &format!("lp{number}"), 2);
        tags::write(
            &path,
            &metadata(&format!("LP Titel {number}"), "Band Eins", Some("Langspieler"), Some(number)),
        )
        .unwrap();
        scanner::import_file(&conn, &path, Some("test")).unwrap();
    }

    // single: without an album
    let single = make_mp3(&dir, "single", 2);
    tags::write(&single, &metadata("Alleinstellung", "Band Zwei", None, None)).unwrap();
    let single_id = scanner::import_file(&conn, &single, Some("test")).unwrap();

    library::refresh_release_types(&conn).unwrap();

    let artists = library::list_artists(&conn, None).unwrap();
    assert_eq!(artists.len(), 2, "zwei Künstler erwartet");

    let band_eins = artists.iter().find(|a| a.name == "Band Eins").unwrap();
    let releases = library::artist_releases(&conn, band_eins.id).unwrap();
    assert_eq!(releases.len(), 2);

    let ep = releases.iter().find(|r| r.title == "Kurzspieler").unwrap();
    let album = releases.iter().find(|r| r.title == "Langspieler").unwrap();
    assert_eq!(ep.release_type, ReleaseType::Ep, "3 Titel ⇒ EP");
    assert_eq!(album.release_type, ReleaseType::Album, "7 Titel ⇒ Album");
    assert_eq!(ep.track_count, 3);
    assert_eq!(album.track_count, 7);

    // tracks of a release come in track order
    let ep_tracks = library::album_tracks(&conn, ep.id).unwrap();
    assert_eq!(
        ep_tracks.iter().map(|t| t.track_no).collect::<Vec<_>>(),
        vec![Some(1), Some(2), Some(3)]
    );

    // the track without an album lands as a single under its own name
    let single_track = library::get_track(&conn, single_id).unwrap();
    assert_eq!(single_track.release_type, ReleaseType::Single);
    assert_eq!(single_track.album_title, "Alleinstellung");

    // importing the same file again must duplicate nothing
    scanner::import_file(&conn, &single, Some("test")).unwrap();
    assert_eq!(library::library_stats(&conn).unwrap().track_count, 11);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn playlists_und_auswertungen() {
    if !ffmpeg_available() {
        eprintln!("ffmpeg fehlt, Test übersprungen");
        return;
    }
    let dir = tempdir("stats");
    let conn = setup_db(&dir);

    let mut ids = Vec::new();
    for number in 1..=4 {
        let path = make_mp3(&dir, &format!("t{number}"), 2);
        tags::write(
            &path,
            &metadata(&format!("Titel {number}"), "Statistikband", Some("Zahlenwerk"), Some(number)),
        )
        .unwrap();
        ids.push(scanner::import_file(&conn, &path, Some("test")).unwrap());
    }

    // --- playlist ---
    let playlist_id = library::create_playlist(&conn, "Testliste", Some("Beschreibung")).unwrap();
    library::add_to_playlist(&conn, playlist_id, &ids).unwrap();
    assert_eq!(library::playlist_tracks(&conn, playlist_id).unwrap().len(), 4);

    // adding it twice changes nothing
    library::add_to_playlist(&conn, playlist_id, &ids).unwrap();
    assert_eq!(library::playlist_tracks(&conn, playlist_id).unwrap().len(), 4);

    // reordering turns the order around
    let reversed: Vec<i64> = ids.iter().rev().copied().collect();
    library::reorder_playlist(&conn, playlist_id, &reversed).unwrap();
    let ordered: Vec<i64> = library::playlist_tracks(&conn, playlist_id)
        .unwrap()
        .iter()
        .map(|t| t.id)
        .collect();
    assert_eq!(ordered, reversed);

    library::remove_from_playlist(&conn, playlist_id, ids[0]).unwrap();
    assert_eq!(library::playlist_tracks(&conn, playlist_id).unwrap().len(), 3);

    // --- record plays: track 1 is the clear leader ---
    let now = db::now();
    let plays: [(usize, i64, i64); 4] = [(0, 5, 200_000), (1, 3, 120_000), (2, 2, 80_000), (3, 1, 40_000)];
    for (index, count, ms_each) in plays {
        for repeat in 0..count {
            conn.execute(
                "INSERT INTO plays (track_id, played_at, ms_played, completed) VALUES (?1, ?2, ?3, 1)",
                rusqlite::params![ids[index], now - repeat * 3_600, ms_each],
            )
            .unwrap();
        }
        // the history stands on the track and not in `plays`: only what ran
        // long enough for the statistics counts there, while the history
        // holds the short ones too. the player writes both, here the test
        // does
        conn.execute(
            "UPDATE tracks SET last_played_at = ?2 WHERE id = ?1",
            rusqlite::params![ids[index], now],
        )
        .unwrap();
    }

    let wrapped = stats::wrapped(&conn, "all", 0).unwrap();
    assert_eq!(wrapped.total_plays, 11);
    assert_eq!(wrapped.distinct_tracks, 4);
    assert_eq!(wrapped.distinct_artists, 1);
    assert_eq!(wrapped.top_tracks.len(), 4);
    assert_eq!(wrapped.top_tracks[0].track.id, ids[0], "meistgehörter Titel zuerst");
    assert_eq!(wrapped.top_tracks[0].play_count, 5);

    // the total time of the top tracks has to equal the sum of the single times
    let expected: i64 = wrapped.top_tracks.iter().map(|t| t.ms_played).sum();
    assert_eq!(wrapped.top_tracks_total_ms, expected);
    assert_eq!(wrapped.total_ms, 5 * 200_000 + 3 * 120_000 + 2 * 80_000 + 40_000);

    assert_eq!(wrapped.top_artists.len(), 1);
    assert_eq!(wrapped.top_artists[0].name, "Statistikband");

    // month and year must not hold more than all-time
    let month = stats::wrapped(&conn, "month", 0).unwrap();
    assert!(month.total_ms <= wrapped.total_ms);
    let long_ago = stats::wrapped(&conn, "year", -5).unwrap();
    assert_eq!(long_ago.total_plays, 0, "leerer Zeitraum liefert Nullwerte");

    // --- weekly mix: the most played tracks of the running week ---
    let mix = stats::weekly_mix(&conn, 0).unwrap();
    assert!(!mix.items.is_empty(), "Mix darf nicht leer sein");
    assert!(mix.items.len() <= 30, "höchstens dreißig Titel");
    // numbers instead of a finished sentence, what becomes of it is up to the ui
    assert!(mix.items.iter().all(|item| item.play_count > 0));
    assert_eq!(mix.offset, 0);
    // the display name grows in the frontend, here only the number it is
    // built from counts
    assert!(mix.number >= 1, "Wochennummer beginnt bei eins: {}", mix.number);

    let unique: std::collections::HashSet<i64> =
        mix.items.iter().map(|item| item.track.id).collect();
    assert_eq!(unique.len(), mix.items.len(), "keine Dubletten im Mix");

    // sorted by listening time, the most played track stands first
    assert_eq!(
        mix.items[0].track.id, ids[0],
        "Reihenfolge folgt nicht der Hörzeit"
    );

    // the same call delivers the same: the mix is derived from the plays, not
    // drawn at random
    let again = stats::weekly_mix(&conn, 0).unwrap();
    assert_eq!(
        again.items.iter().map(|i| i.track.id).collect::<Vec<_>>(),
        mix.items.iter().map(|i| i.track.id).collect::<Vec<_>>()
    );

    // a week without listening data stays empty but does not crash
    let leer = stats::weekly_mix(&conn, 300).unwrap();
    assert!(leer.items.is_empty());
    assert_eq!(leer.offset, 300);

    // --- recently played ---
    let zuletzt = library::recently_played(&conn, 10).unwrap();
    assert!(!zuletzt.is_empty(), "keine Wiedergaben gefunden");
    let einmalig: std::collections::HashSet<i64> =
        zuletzt.iter().map(|track| track.id).collect();
    assert_eq!(einmalig.len(), zuletzt.len(), "Titel doppelt aufgeführt");

    // --- the cleanup removes orphaned albums and artists as well ---
    for id in &ids {
        library::delete_track(&conn, *id, false, None).unwrap();
    }
    let stats_after = library::library_stats(&conn).unwrap();
    assert_eq!(stats_after.track_count, 0);
    assert!(library::list_artists(&conn, None).unwrap().is_empty());

    std::fs::remove_dir_all(&dir).ok();
}

fn tempdir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("robify-test-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("Testordner anlegen");
    dir
}

/// the smallest valid png, base64 encoded.
fn base64_png() -> String {
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==".into()
}

#[test]
fn titel_koennen_mehrere_kuenstler_haben() {
    if !ffmpeg_available() {
        eprintln!("ffmpeg fehlt, Test übersprungen");
        return;
    }
    let dir = tempdir("multi");
    let conn = setup_db(&dir);

    // a track with lead and guest artists, the way genius delivers it
    let path = make_mp3(&dir, "monster", 2);
    let mut meta = metadata("Monster", "Kanye West", Some("MBDTF"), Some(7));
    meta.featured_artists = Some("Bon Iver; Nicki Minaj".into());
    tags::write(&path, &meta).unwrap();
    let track_id = scanner::import_file(&conn, &path, Some("test")).unwrap();

    let track = library::get_track(&conn, track_id).unwrap();
    assert_eq!(track.artist_name, "Kanye West", "Hauptkünstler bleibt vorn");
    assert_eq!(track.artists.len(), 3, "alle Beteiligten erfasst");
    assert_eq!(track.artists[0].role, "main");
    let featured: Vec<&str> = track
        .artists
        .iter()
        .filter(|a| a.role == "feature")
        .map(|a| a.name.as_str())
        .collect();
    assert_eq!(featured, vec!["Bon Iver", "Nicki Minaj"]);

    // the track shows up under every participant
    for artist in &track.artists {
        let tracks = library::artist_tracks(&conn, artist.id).unwrap();
        assert!(
            tracks.iter().any(|t| t.id == track_id),
            "{} sieht den Titel nicht",
            artist.name
        );
    }

    // guest appearances can be queried separately
    let bon_iver = track.artists.iter().find(|a| a.name == "Bon Iver").unwrap();
    let features = library::artist_features(&conn, bon_iver.id).unwrap();
    assert_eq!(features.len(), 1);
    assert!(library::artist_features(&conn, track.artist_id).unwrap().is_empty());

    // through the file the assignment survives (feat. in the artist field)
    let reread = tags::read(&path).unwrap();
    assert!(reread.metadata.artist.contains("Kanye West"));
    assert!(reread.metadata.artist.contains("Bon Iver"));

    // participants can be set anew, old links disappear
    library::set_track_artists(&conn, track_id, &["Jon Hopkins".into()], &[]).unwrap();
    let track = library::get_track(&conn, track_id).unwrap();
    assert_eq!(track.artists.len(), 1);
    assert_eq!(track.artist_name, "Jon Hopkins");

    std::fs::remove_dir_all(&dir).ok();
}

// a guest contribution is one play for every participant.
//
// the evaluation used to count through `tracks.artist_id` and saw the lead
// artist alone: whoever listened to an album full of guest appearances got a
// list with a single name at the end of the year
#[test]
fn gastkuenstler_zaehlen_im_rueckblick_mit() {
    if !ffmpeg_available() {
        eprintln!("ffmpeg fehlt, Test übersprungen");
        return;
    }
    let dir = tempdir("gaeste");
    let conn = setup_db(&dir);

    let path = make_mp3(&dir, "monster", 2);
    let mut meta = metadata("Monster", "Kanye West", Some("MBDTF"), Some(7));
    meta.featured_artists = Some("Bon Iver; Nicki Minaj".into());
    tags::write(&path, &meta).unwrap();
    let track_id = scanner::import_file(&conn, &path, Some("test")).unwrap();

    let now = db::now();
    for repeat in 0..3 {
        conn.execute(
            "INSERT INTO plays (track_id, played_at, ms_played, completed) VALUES (?1, ?2, ?3, 1)",
            rusqlite::params![track_id, now - repeat * 3_600, 200_000],
        )
        .unwrap();
    }

    let wrapped = stats::wrapped(&conn, "all", 0).unwrap();
    assert_eq!(wrapped.total_plays, 3, "die Wiedergaben selbst bleiben drei");
    assert_eq!(wrapped.distinct_artists, 3, "alle drei Beteiligten gezählt");

    let namen: Vec<&str> = wrapped.top_artists.iter().map(|a| a.name.as_str()).collect();
    for erwartet in ["Kanye West", "Bon Iver", "Nicki Minaj"] {
        assert!(namen.contains(&erwartet), "{erwartet} fehlt in {namen:?}");
    }

    // every participant is credited the full listening time, not a fraction
    for artist in &wrapped.top_artists {
        assert_eq!(artist.ms_played, 600_000, "{} zu wenig", artist.name);
        assert_eq!(artist.play_count, 3);
        assert_eq!(artist.track_count, 1);
    }

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn kuenstler_und_release_lassen_sich_nachtraeglich_bearbeiten() {
    if !ffmpeg_available() {
        eprintln!("ffmpeg fehlt, Test übersprungen");
        return;
    }
    let dir = tempdir("edit");
    let conn = setup_db(&dir);

    let path = make_mp3(&dir, "edit1", 2);
    tags::write(&path, &metadata("Ein Titel", "Altname", Some("Altalbum"), Some(1))).unwrap();
    let track_id = scanner::import_file(&conn, &path, Some("test")).unwrap();
    let track = library::get_track(&conn, track_id).unwrap();

    // --- artist: without image and description at first ---
    let artist = library::get_artist(&conn, track.artist_id).unwrap();
    assert!(!artist.has_image);
    assert!(artist.bio.is_none());

    library::update_artist(
        &conn,
        artist.id,
        "Neuname",
        Some("Eine Beschreibung"),
        Some("https://genius.com/artists/Neuname"),
    )
    .unwrap();
    library::set_artist_image(&conn, artist.id, &[1, 2, 3, 4], "image/png").unwrap();

    let artist = library::get_artist(&conn, artist.id).unwrap();
    assert_eq!(artist.name, "Neuname");
    assert_eq!(artist.bio.as_deref(), Some("Eine Beschreibung"));
    assert!(artist.has_image);
    let (data, mime) = library::artist_image(&conn, artist.id).unwrap().unwrap();
    assert_eq!(data, vec![1, 2, 3, 4]);
    assert_eq!(mime, "image/png");

    // the track shows the new name
    assert_eq!(library::get_track(&conn, track_id).unwrap().artist_name, "Neuname");

    // a name already taken is refused
    let other = library::upsert_artist(&conn, "Jemand Anderes").unwrap();
    let clash = library::update_artist(&conn, other, "Neuname", None, None);
    assert!(clash.is_err(), "doppelter Name muss abgelehnt werden");
    // its own name stays allowed
    assert!(library::update_artist(&conn, artist.id, "Neuname", None, None).is_ok());

    // --- release: change title, year and classification ---
    let album = library::get_album(&conn, track.album_id).unwrap();
    library::update_album(&conn, album.id, "Neues Album", Some(1999), ReleaseType::Ep).unwrap();
    let album = library::get_album(&conn, album.id).unwrap();
    assert_eq!(album.title, "Neues Album");
    assert_eq!(album.year, Some(1999));
    assert_eq!(album.release_type, ReleaseType::Ep);

    // the classification is pinned now and is not overwritten
    library::refresh_release_types(&conn).unwrap();
    assert_eq!(
        library::get_album(&conn, album.id).unwrap().release_type,
        ReleaseType::Ep,
        "manuelle Einordnung darf nicht zurückfallen"
    );

    // empty input is turned away
    assert!(library::update_album(&conn, album.id, "  ", None, ReleaseType::Album).is_err());
    assert!(library::update_artist(&conn, artist.id, " ", None, None).is_err());

    std::fs::remove_dir_all(&dir).ok();
}


// since best quality became the default, downloads often arrive as opus, m4a
// or flac. the tags have to hold in all of these formats
#[test]
fn tags_halten_auch_in_opus_m4a_und_flac() {
    if !ffmpeg_available() {
        eprintln!("ffmpeg fehlt, Test übersprungen");
        return;
    }
    let dir = tempdir("formate");

    for extension in ["opus", "m4a", "flac", "ogg"] {
        let path = make_audio(&dir, &format!("probe_{extension}"), extension, 2);

        let mut meta = metadata("Nachtfahrt", "Hauptkünstler", Some("Testalbum"), Some(4));
        meta.featured_artists = Some("Gastkünstler".into());
        meta.cover_base64 = Some(base64_png());
        meta.cover_mime = Some("image/png".into());
        meta.lyrics_plain = Some("Eine Zeile".into());

        tags::write(&path, &meta).unwrap_or_else(|e| panic!("{extension}: Schreiben, {e}"));
        let read = tags::read(&path).unwrap_or_else(|e| panic!("{extension}: Lesen, {e}"));

        assert_eq!(read.metadata.title, "Nachtfahrt", "{extension}: Titel");
        assert!(
            read.metadata.artist.contains("Hauptkünstler"),
            "{extension}: Künstler fehlt, {}",
            read.metadata.artist
        );
        assert!(
            read.metadata.artist.contains("Gastkünstler"),
            "{extension}: Gastkünstler fehlt, {}",
            read.metadata.artist
        );
        assert_eq!(read.metadata.album, "Testalbum", "{extension}: Album");
        assert_eq!(read.metadata.track_no, Some(4), "{extension}: Titelnummer");
        assert_eq!(read.metadata.year, Some(2024), "{extension}: Jahr");
        assert!(read.duration_ms >= 1500, "{extension}: Dauer");
    }

    std::fs::remove_dir_all(&dir).ok();
}

// at the first track of an artist their details are due to be fetched
#[test]
fn neue_kuenstler_werden_zum_nachladen_gemeldet() {
    if !ffmpeg_available() {
        eprintln!("ffmpeg fehlt, Test übersprungen");
        return;
    }
    let dir = tempdir("kuenstlerdaten");
    let conn = setup_db(&dir);

    let path = make_mp3(&dir, "erster", 2);
    let mut meta = metadata("Erster Titel", "Neuer Künstler", None, None);
    meta.featured_artists = Some("Gast".into());
    tags::write(&path, &meta).unwrap();
    let track_id = scanner::import_file(&conn, &path, Some("test")).unwrap();

    // lead and guest artist both carry nothing yet
    let offen = library::artists_missing_metadata(&conn, track_id).unwrap();
    let namen: Vec<&str> = offen.iter().map(|(_, name, _)| name.as_str()).collect();
    assert_eq!(namen, vec!["Neuer Künstler", "Gast"], "Reihenfolge und Auswahl");

    // whoever has an image is no longer due
    let (haupt_id, _, _) = offen[0];
    library::set_artist_image(&conn, haupt_id, b"nicht wirklich ein bild", "image/jpeg").unwrap();
    let offen = library::artists_missing_metadata(&conn, track_id).unwrap();
    assert_eq!(offen.len(), 1, "der Hauptkünstler ist versorgt");

    // a description alone suffices as well
    let (gast_id, gast_name, _) = offen[0].clone();
    library::update_artist(&conn, gast_id, &gast_name, Some("Kurzbeschreibung"), None).unwrap();
    assert!(library::artists_missing_metadata(&conn, track_id).unwrap().is_empty());

    // a second track by the same artist triggers nothing any more
    let path2 = make_mp3(&dir, "zweiter", 2);
    let meta2 = metadata("Zweiter Titel", "Neuer Künstler", None, None);
    tags::write(&path2, &meta2).unwrap();
    let track2 = scanner::import_file(&conn, &path2, Some("test")).unwrap();
    assert!(library::artists_missing_metadata(&conn, track2).unwrap().is_empty());

    std::fs::remove_dir_all(&dir).ok();
}

// invisible characters must not produce a second album
#[test]
fn unsichtbare_zeichen_erzeugen_kein_doppeltes_album() {
    if !ffmpeg_available() {
        eprintln!("ffmpeg fehlt, Test übersprungen");
        return;
    }
    let dir = tempdir("unsichtbar");
    let conn = setup_db(&dir);

    // titles from the net occasionally carry a hangul filler (u+3164). it is
    // invisible and still counts as a letter to rust
    let erster = make_mp3(&dir, "a", 2);
    tags::write(&erster, &metadata("Track A", "TIEFBASSKOMMANDO", Some("RETOX"), Some(1))).unwrap();
    scanner::import_file(&conn, &erster, Some("test")).unwrap();

    let zweiter = make_mp3(&dir, "b", 2);
    tags::write(
        &zweiter,
        &metadata("Track B", "TIEFBASSKOMMANDO", Some("RETOX\u{3164}"), Some(2)),
    )
    .unwrap();
    scanner::import_file(&conn, &zweiter, Some("test")).unwrap();

    let alben = library::list_albums(&conn, None).unwrap();
    assert_eq!(
        alben.len(),
        1,
        "doppeltes Album: {:?}",
        alben.iter().map(|a| &a.title).collect::<Vec<_>>()
    );
    assert_eq!(alben[0].track_count, 2, "Titel wurden nicht zusammengeführt");

    std::fs::remove_dir_all(&dir).ok();
}

// folder and database drift apart in daily use, and the reconciliation has to
// recognise both directions
#[test]
fn abgleich_findet_verwaiste_und_fehlende_dateien() {
    if !ffmpeg_available() {
        eprintln!("ffmpeg fehlt, Test übersprungen");
        return;
    }
    let dir = tempdir("abgleich");
    let conn = setup_db(&dir);

    // import two tracks, one of which disappears afterwards
    let bleibt = make_mp3(&dir, "bleibt", 2);
    tags::write(&bleibt, &metadata("Bleibt", "Künstler", None, None)).unwrap();
    scanner::import_file(&conn, &bleibt, Some("test")).unwrap();

    let verschwindet = make_mp3(&dir, "weg", 2);
    tags::write(&verschwindet, &metadata("Weg", "Künstler", None, None)).unwrap();
    scanner::import_file(&conn, &verschwindet, Some("test")).unwrap();

    assert!(library::tracks_without_file(&conn).unwrap().is_empty());

    std::fs::remove_file(&verschwindet).unwrap();
    let fehlend = library::tracks_without_file(&conn).unwrap();
    assert_eq!(fehlend.len(), 1, "verschwundene Datei nicht erkannt");
    assert!(fehlend[0].1.ends_with("weg.mp3"));

    // a file never imported counts as an orphan
    let verwaist = make_mp3(&dir, "verwaist", 2);
    let bekannt = library::known_paths(&conn).unwrap();
    assert!(bekannt.contains(&bleibt.to_string_lossy().to_string()));
    assert!(!bekannt.contains(&verwaist.to_string_lossy().to_string()));

    // after the removal the database holds together again
    library::delete_track(&conn, fehlend[0].0, false, None).unwrap();
    assert!(library::tracks_without_file(&conn).unwrap().is_empty());

    std::fs::remove_dir_all(&dir).ok();
}

// whoever throws a track out of the library is to keep their review. and
// creating the same track again later, robify carries on counting instead of
// starting at zero
#[test]
fn entfernte_titel_bleiben_im_rueckblick_und_knuepfen_wieder_an() {
    let conn = Connection::open_in_memory().expect("Speicher-Datenbank");
    db::migrate(&conn).expect("Migration");

    let einfuegen = |pfad: &str| {
        library::upsert_track(
            &conn,
            &library::TrackInsert {
                path: pfad.into(),
                title: "Nachtfahrt".into(),
                artist: "Gedächtnisband".into(),
                featured_artists: None,
                album: Some("Erinnerung".into()),
                album_artist: None,
                release_type: Some(ReleaseType::Album),
                track_no: Some(1),
                disc_no: None,
                duration_ms: 200_000,
                genre: None,
                year: Some(2026),
                format: "mp3".into(),
                source: None,
                source_url: None,
            },
        )
        .expect("Titel anlegen")
    };

    let id = einfuegen("/musik/nachtfahrt.mp3");
    let jetzt = chrono::Utc::now().timestamp();
    for versatz in 0..3 {
        conn.execute(
            "INSERT INTO plays (track_id, played_at, ms_played, completed)
             VALUES (?1, ?2, ?3, 1)",
            rusqlite::params![id, jetzt - versatz * 60, 200_000],
        )
        .expect("Wiedergabe vermerken");
    }

    let vorher = stats::wrapped(&conn, "all", 0).expect("Rückblick");
    assert_eq!(vorher.total_plays, 3);
    assert_eq!(vorher.top_tracks.len(), 1);
    assert_eq!(vorher.top_artists.len(), 1);

    // --- removal: gone from the library, kept in the review ---
    library::delete_track(&conn, id, false, None).expect("entfernen");

    assert!(
        library::list_tracks(&conn, None, 100).unwrap().is_empty(),
        "entfernter Titel steht noch in der Bibliothek"
    );
    assert_eq!(library::library_stats(&conn).unwrap().track_count, 0);
    assert!(library::list_artists(&conn, None).unwrap().is_empty());

    let danach = stats::wrapped(&conn, "all", 0).expect("Rückblick");
    assert_eq!(danach.total_plays, 3, "Hörhistorie ging verloren");
    assert_eq!(danach.top_tracks.len(), 1, "Titel fehlt im Rückblick");
    assert!(danach.top_tracks[0].track.deleted, "nicht als entfernt gemeldet");
    assert_eq!(danach.top_artists.len(), 1, "Künstler fehlt im Rückblick");
    assert_eq!(danach.top_artists[0].name, "Gedächtnisband");

    // --- create it again, this time under a different path: the same row ---
    let neu = einfuegen("/musik/neu/nachtfahrt.mp3");
    assert_eq!(neu, id, "es entstand ein zweiter Eintrag");
    assert_eq!(library::list_tracks(&conn, None, 100).unwrap().len(), 1);

    let track = library::get_track(&conn, id).unwrap();
    assert!(!track.deleted);
    assert_eq!(track.play_count, 3, "Zählerstand nicht übernommen");
    assert_eq!(track.path, "/musik/neu/nachtfahrt.mp3");
}

// what was deleted has to be recoverable: the row back, the file back in its
// old place
#[test]
fn geloeschte_titel_lassen_sich_zurueckholen() {
    let conn = Connection::open_in_memory().expect("Speicher-Datenbank");
    db::migrate(&conn).expect("Migration");

    let dir = tempdir("papierkorb");
    let musik = dir.join("musik");
    let papierkorb = dir.join("papierkorb");
    std::fs::create_dir_all(&musik).expect("Musikordner");
    let datei = musik.join("lied.mp3");
    std::fs::write(&datei, b"nicht wirklich audio").expect("Datei anlegen");

    let id = library::upsert_track(
        &conn,
        &library::TrackInsert {
            path: datei.to_string_lossy().into(),
            title: "Rückholung".into(),
            artist: "Testband".into(),
            featured_artists: None,
            album: Some("Probe".into()),
            album_artist: None,
            release_type: Some(ReleaseType::Album),
            track_no: Some(1),
            disc_no: None,
            duration_ms: 120_000,
            genre: None,
            year: None,
            format: "mp3".into(),
            source: None,
            source_url: None,
        },
    )
    .expect("Titel anlegen");

    library::delete_track(&conn, id, true, Some(&papierkorb)).expect("löschen");
    assert!(!datei.exists(), "Datei liegt noch am alten Platz");
    assert!(papierkorb.join(format!("{id}.mp3")).exists(), "nichts im Papierkorb");
    assert!(library::list_tracks(&conn, None, 100).unwrap().is_empty());

    library::restore_track(&conn, id, Some(&papierkorb)).expect("zurückholen");
    assert!(datei.exists(), "Datei kam nicht zurück");
    assert_eq!(library::list_tracks(&conn, None, 100).unwrap().len(), 1);

    // --- the same for playlists ---
    let playlist = library::create_playlist(&conn, "Zum Löschen", None).unwrap();
    library::add_to_playlist(&conn, playlist, &[id]).unwrap();
    library::delete_playlist(&conn, playlist).unwrap();
    assert!(library::list_playlists(&conn).unwrap().is_empty());

    library::restore_playlist(&conn, playlist).unwrap();
    let zurueck = library::list_playlists(&conn).unwrap();
    assert_eq!(zurueck.len(), 1);
    assert_eq!(zurueck[0].track_count, 1, "Titel der Playlist fehlen");

    std::fs::remove_dir_all(&dir).ok();
}

// the import recognises by itself where the details are no good. without that
// distinction either nothing would be looked up at all or every cleanly
// tagged track would be queried needlessly
#[test]
fn schwache_angaben_werden_erkannt() {
    // --- titles that look like filenames ---
    for name in [
        "",
        "03 - Nachtfahrt",
        "03. Nachtfahrt",
        "Gedaechtnisband_-_Nachtfahrt",
        "Gedächtnisband - Nachtfahrt",
    ] {
        assert!(
            library::looks_like_filename(name),
            "„{name}“ müsste als Dateiname gelten"
        );
    }

    // --- real titles stay untouched ---
    for name in ["Nachtfahrt", "Bohemian Rhapsody", "9 to 5", "Sieben Leben"] {
        assert!(
            !library::looks_like_filename(name),
            "„{name}“ ist ein Titel, kein Dateiname"
        );
    }

    // --- in the library: only the local import with poor details ---
    let conn = Connection::open_in_memory().expect("Speicher-Datenbank");
    db::migrate(&conn).expect("Migration");

    let anlegen = |pfad: &str, titel: &str, kuenstler: &str, quelle: &str| {
        library::upsert_track(
            &conn,
            &library::TrackInsert {
                path: pfad.into(),
                title: titel.into(),
                artist: kuenstler.into(),
                featured_artists: None,
                album: Some("Ein Album".into()),
                album_artist: None,
                release_type: Some(ReleaseType::Album),
                track_no: Some(1),
                disc_no: None,
                duration_ms: 180_000,
                genre: None,
                year: None,
                format: "mp3".into(),
                source: Some(quelle.into()),
                source_url: None,
            },
        )
        .expect("Titel anlegen")
    };

    anlegen("/m/a.mp3", "Sauber Getaggt", "Echte Band", "lokal");
    let ohne_kuenstler = anlegen("/m/b.mp3", "Nachtfahrt", "Unbekannter Künstler", "lokal");
    let dateiname = anlegen("/m/c.mp3", "04 - Irgendwas", "Echte Band", "lokal");
    // from the downloader: it was checked while loading there
    anlegen("/m/d.mp3", "07 - Geladen", "Unbekannter Künstler", "youtube");

    let schwach = library::tracks_with_weak_metadata(&conn, 10).unwrap();
    let ids: Vec<i64> = schwach.iter().map(|t| t.id).collect();

    assert!(ids.contains(&ohne_kuenstler), "Titel ohne Künstler fehlt");
    assert!(ids.contains(&dateiname), "Titel mit Dateinamen fehlt");
    assert_eq!(ids.len(), 2, "unerwartete Auswahl: {schwach:?}");
}

// the same track must not land in the library twice, not even where the file
// lies elsewhere. different recordings of the same name stay apart though, and
// only the running time tells them apart
#[test]
fn derselbe_titel_landet_nicht_zweimal_in_der_bibliothek() {
    let conn = Connection::open_in_memory().expect("Speicher-Datenbank");
    db::migrate(&conn).expect("Migration");

    let anlegen = |pfad: &str, titel: &str, kuenstler: &str, dauer: i64| {
        library::upsert_track(
            &conn,
            &library::TrackInsert {
                path: pfad.into(),
                title: titel.into(),
                artist: kuenstler.into(),
                featured_artists: None,
                album: Some("Ein Album".into()),
                album_artist: None,
                release_type: Some(ReleaseType::Album),
                track_no: Some(1),
                disc_no: None,
                duration_ms: dauer,
                genre: None,
                year: None,
                format: "mp3".into(),
                source: Some("lokal".into()),
                source_url: None,
            },
        )
        .expect("Titel anlegen")
    };

    let erster = anlegen("/musik/a/nachtfahrt.mp3", "Nachtfahrt", "Testband", 200_000);

    // a copy elsewhere, spelled differently: the same row
    let kopie = anlegen("/musik/b/Nachtfahrt (1).mp3", "nachtfahrt!", "Testband", 201_500);
    assert_eq!(kopie, erster, "die Kopie wurde als zweiter Titel angelegt");

    // a markedly different length: a different recording, so a row of its own
    let langfassung = anlegen("/musik/c/nachtfahrt-live.mp3", "Nachtfahrt", "Testband", 320_000);
    assert_ne!(langfassung, erster, "die Langfassung wurde einkassiert");

    // without a running time nothing is merged
    let ohne_laenge = anlegen("/musik/d/nachtfahrt.mp3", "Nachtfahrt", "Testband", 0);
    assert_ne!(ohne_laenge, erster, "ohne Länge darf nicht zusammengelegt werden");

    // the same name, a different artist: two different tracks
    let andere_band = anlegen("/musik/e/nachtfahrt.mp3", "Nachtfahrt", "Zweitband", 200_000);
    assert_ne!(andere_band, erster);

    assert_eq!(library::list_tracks(&conn, None, 100).unwrap().len(), 4);
}

// the collection can be reordered, and new arrivals stand at the front.
//
// kept apart from `playlists_und_auswertungen` because neither tracks nor
// ffmpeg are needed here: this is about the order of the lists alone
#[test]
fn playlists_lassen_sich_umsortieren() {
    let dir = tempdir("ordnung");
    let conn = setup_db(&dir);

    let a = library::create_playlist(&conn, "Alpha", None).unwrap();
    let b = library::create_playlist(&conn, "Beta", None).unwrap();
    let c = library::create_playlist(&conn, "Gamma", None).unwrap();

    let namen = |conn: &rusqlite::Connection| -> Vec<String> {
        library::list_playlists(conn)
            .unwrap()
            .into_iter()
            .map(|p| p.name)
            .collect()
    };

    // left alone the last created stands first, as it did before the change
    assert_eq!(namen(&conn), vec!["Gamma", "Beta", "Alpha"]);

    library::reorder_playlists(&conn, &[a, c, b]).unwrap();
    assert_eq!(namen(&conn), vec!["Alpha", "Gamma", "Beta"]);

    // a new playlist queues in before the previously first, without touching
    // the chosen order of the rest
    library::create_playlist(&conn, "Delta", None).unwrap();
    assert_eq!(namen(&conn), vec!["Delta", "Alpha", "Gamma", "Beta"]);

    std::fs::remove_dir_all(&dir).ok();
}

// the kind of a release used to be almost always wrong, and the cause was one
// column carrying three meanings: the import wrote its guess with the same
// lock a decision of the user gets. since almost every downloaded track brings
// an album name along, almost everything counted as an album and stayed that
// way.
#[test]
fn geratene_art_wird_nachgezogen_gesicherte_nicht() {
    let conn = Connection::open_in_memory().expect("Speicher-Datenbank");
    db::migrate(&conn).expect("Migration");

    let einfuegen = |pfad: &str, titel: &str, album: &str, art: Option<ReleaseType>| {
        library::upsert_track(
            &conn,
            &library::TrackInsert {
                path: pfad.into(),
                title: titel.into(),
                artist: "Probeband".into(),
                featured_artists: None,
                album: Some(album.into()),
                album_artist: None,
                release_type: art,
                track_no: None,
                disc_no: None,
                duration_ms: 200_000,
                genre: None,
                year: None,
                format: "mp3".into(),
                source: None,
                source_url: None,
            },
        )
        .expect("Titel anlegen")
    };

    let art_von = |album: &str| -> String {
        conn.query_row(
            "SELECT release_type FROM albums WHERE title = ?1",
            [album],
            |r| r.get(0),
        )
        .expect("Album")
    };

    // nothing known: one track alone is a single, and it stays revisable
    einfuegen("/m/1.mp3", "Eins", "Ohne Angabe", None);
    assert_eq!(art_von("Ohne Angabe"), "single");

    // four of them make an ep, without anybody having to scan a folder
    for (nummer, titel) in [(2, "Zwei"), (3, "Drei"), (4, "Vier")] {
        einfuegen(&format!("/m/{nummer}.mp3"), titel, "Ohne Angabe", None);
    }
    assert_eq!(art_von("Ohne Angabe"), "ep");

    // what a source said is not overwritten by what happens to lie here
    einfuegen("/m/x.mp3", "Erster", "Mit Angabe", Some(ReleaseType::Album));
    assert_eq!(art_von("Mit Angabe"), "album");
    library::refresh_release_types(&conn).expect("Auffrischen");
    assert_eq!(art_von("Mit Angabe"), "album");

    // and a decision of the user stands above both
    let id: i64 = conn
        .query_row("SELECT id FROM albums WHERE title = ?1", ["Ohne Angabe"], |r| r.get(0))
        .expect("Album");
    library::update_album(&conn, id, "Ohne Angabe", None, ReleaseType::Album).expect("ändern");
    einfuegen("/m/5.mp3", "Fuenf", "Ohne Angabe", None);
    assert_eq!(art_von("Ohne Angabe"), "album");
}
