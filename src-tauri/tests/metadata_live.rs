//! the metadata search against the real services:
//!
//!     cargo test --test metadata_live -- --ignored --nocapture

use robify_lib::online;

/// genius throttles simultaneous requests, so the tests run one after
/// another, otherwise they fail at random.
///
/// deliberately a blocking mutex: every `#[tokio::test]` brings a runtime of
/// its own, and a `tokio::sync::Mutex` does not serialise across their
/// boundaries reliably.
static REQUESTS: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialize() -> std::sync::MutexGuard<'static, ()> {
    REQUESTS.lock().unwrap_or_else(|poison| poison.into_inner())
}

#[tokio::test]
#[ignore = "benötigt eine Internetverbindung"]
async fn genius_steht_vorn_und_trennt_gastkuenstler() {
    let _guard = serialize();
    let results = online::search_metadata("kanye west monster").await.unwrap();
    assert!(!results.is_empty());

    assert_eq!(
        results[0].source, "Genius",
        "Genius soll die erste Quelle sein"
    );

    let monster = results
        .iter()
        .find(|c| c.source == "Genius" && c.title.eq_ignore_ascii_case("Monster"))
        .expect("Titel nicht gefunden");

    println!(
        "{}, {} (feat. {:?}) | Album: {} | {:?} | {:?}",
        monster.artist, monster.title, monster.featured_artists, monster.album,
        monster.year, monster.genre
    );

    assert_eq!(monster.artist, "Kanye West");
    let featured = monster.featured_artists.as_deref().unwrap_or("");
    assert!(featured.contains("Nicki Minaj"), "Gastkünstler fehlen: {featured}");
    assert!(featured.contains(';'), "mehrere Gäste müssen getrennt sein");
    assert!(!monster.album.is_empty(), "Album fehlt");
    assert!(monster.cover_url.is_some(), "Cover fehlt");

    // the other sources stay as a fallback
    let sources: std::collections::HashSet<&str> =
        results.iter().map(|c| c.source.as_str()).collect();
    assert!(sources.len() > 1, "nur eine Quelle geliefert: {sources:?}");
}

#[tokio::test]
#[ignore = "benötigt eine Internetverbindung"]
async fn treffer_wird_vollstaendig_angereichert() {
    let _guard = serialize();
    let results = online::search_metadata("kanye west monster").await.unwrap();
    let candidate = results
        .iter()
        .find(|c| c.source == "Genius" && c.title.eq_ignore_ascii_case("Monster"))
        .expect("Genius-Treffer fehlt");

    let full = online::enrich(candidate, Some(383_000), true, true).await;

    println!("Titel:        {}", full.title);
    println!("Künstler:     {}", full.artist);
    println!("Gäste:        {:?}", full.featured_artists);
    println!("Album:        {}", full.album);
    println!("Albumkünstler:{:?}", full.album_artist);
    println!("Release-Art:  {:?}", full.release_type);
    println!("Titelnummer:  {:?}", full.track_no);
    println!("Jahr/Genre:   {:?} / {:?}", full.year, full.genre);
    println!("Cover:        {} Zeichen", full.cover_base64.as_deref().unwrap_or("").len());
    let plain = full.lyrics_plain.as_deref().unwrap_or("");
    println!("Lyrics:       {} Zeilen", plain.lines().count());
    println!("erste Zeile:  {:?}", plain.lines().next());

    // guest artists land separately in a field of their own
    let featured = full.featured_artists.as_deref().unwrap_or("");
    assert!(featured.contains("Nicki Minaj"), "Gäste fehlen");

    // album name and album artist are set automatically
    assert_eq!(full.album, "My Beautiful Dark Twisted Fantasy");
    assert_eq!(full.album_artist.as_deref(), Some("Kanye West"));

    // release type and track number come from the album track list
    assert_eq!(full.release_type.as_deref(), Some("album"));
    assert!(full.track_no.unwrap_or(0) > 0, "Titelnummer fehlt");

    // lyrics arrive without genius headers
    assert!(plain.lines().count() > 20, "Lyrics zu kurz");
    assert!(!plain.contains("Read More"), "Kopfbereich nicht entfernt");
    assert!(!plain.contains("Contributors"), "Kopfbereich nicht entfernt");
    assert!(plain.starts_with('['), "Lyrics beginnen nicht am Abschnitt");

    assert!(full.cover_base64.is_some(), "Cover fehlt");
}

#[tokio::test]
#[ignore = "benötigt eine Internetverbindung"]
async fn korrigiert_grobe_angaben_aus_einem_download() {
    let _guard = serialize();
    // this is how the details typically come out of a video description:
    // both artists in one field, no album, no lyrics
    let from_file = robify_lib::models::TrackMetadata {
        title: "Die Welt zu Gast bei Feinden".into(),
        artist: "PA69, Drunken Masters".into(),
        featured_artists: None,
        album: String::new(),
        album_artist: None,
        release_type: None,
        year: Some(2026),
        track_no: None,
        disc_no: None,
        genre: Some("Hip-Hop/Rap".into()),
        cover_base64: None,
        cover_mime: None,
        lyrics_synced: None,
        lyrics_plain: None,
    };

    let found = online::auto_match(&from_file, None, true, true)
        .await
        .expect("kein Treffer zugeordnet");
    let merged = online::merge_match(from_file, found);

    println!("Titel:       {}", merged.title);
    println!("Künstler:    {}", merged.artist);
    println!("Gäste:       {:?}", merged.featured_artists);
    println!("Album:       {:?}", merged.album);
    println!("Release-Art: {:?}", merged.release_type);
    println!("Cover:       {}", merged.cover_base64.is_some());
    let plain = merged.lyrics_plain.as_deref().unwrap_or("");
    println!("Lyrics:      {} Zeilen", plain.lines().count());

    assert_eq!(merged.title, "Die Welt zu Gast bei Feinden");
    // the artists now stand separately instead of in one field
    assert!(merged.artist.contains("PA69"));
    assert!(merged.artist.contains(';'), "Künstler nicht getrennt: {}", merged.artist);
    // without an album it is a single, not an album
    assert_eq!(merged.release_type.as_deref(), Some("single"));
    assert!(!plain.is_empty(), "Lyrics fehlen");
}

#[tokio::test]
#[ignore = "benötigt eine Internetverbindung"]
async fn ordnet_nichts_zu_wenn_es_nicht_passt() {
    let _guard = serialize();
    let nonsense = robify_lib::models::TrackMetadata {
        title: "Zzzq Xkcd Nichtvorhanden 99182".into(),
        artist: "Qqxz Unbekannt".into(),
        ..Default::default()
    };
    assert!(
        online::auto_match(&nonsense, None, false, false).await.is_none(),
        "es darf kein fremder Treffer übernommen werden"
    );
}

#[tokio::test]
#[ignore = "benötigt eine Internetverbindung"]
async fn findet_kuenstlerbild_und_beschreibung() {
    let _guard = serialize();
    let candidates = online::search_artists("PA69").await.unwrap();
    let best = candidates
        .iter()
        .find(|c| c.name.eq_ignore_ascii_case("PA69"))
        .expect("Künstler nicht gefunden");

    println!("Name: {}", best.name);
    println!("Bild: {:?}", best.image_url);
    println!("Info: {:?}", best.bio.as_deref().map(|b| &b[..b.len().min(120)]));

    assert!(best.image_url.is_some(), "Profilbild fehlt");
    assert!(
        best.bio.as_deref().unwrap_or("").len() > 20,
        "Beschreibung fehlt"
    );
    assert!(best.url.is_some());

    // the image has to be loadable as well
    let (data, mime) = online::fetch_image(best.image_url.as_deref().unwrap())
        .await
        .unwrap();
    assert!(data.len() > 1000, "Bilddatei zu klein");
    assert!(mime.starts_with("image/"), "unerwarteter Typ: {mime}");
}

// artists sharing a name must not be mixed up.
//
// under "Julia" genius lists "Julia Michaels", "Julian Casablancas" and
// "Julia Engelmann" but no "Julia". the first hit used to be taken over
// plainly, and a stranger's face stood in the profile then
#[tokio::test]
#[ignore = "benötigt Internet"]
async fn fremde_kuenstler_mit_aehnlichem_namen_werden_abgelehnt() {
    let _guard = serialize();

    let treffer = online::search_artists("Julia").await.unwrap_or_default();
    println!("Genius zu „Julia“:");
    for kandidat in treffer.iter().take(5) {
        println!("   {}", kandidat.name);
    }

    // without an unambiguous name nothing may be taken over
    let gewaehlt = robify_lib::commands::best_artist_match("Julia", &[]).await;
    assert!(
        gewaehlt.is_none(),
        "fremder Künstler übernommen: {:?}",
        gewaehlt.map(|k| k.name)
    );

    // an unambiguous name still works
    let radiohead = robify_lib::commands::best_artist_match("Radiohead", &[])
        .await
        .expect("Radiohead nicht gefunden");
    assert!(
        online::looks_like_same(&radiohead.name, "Radiohead"),
        "unerwartet: {}",
        radiohead.name
    );
}

// the reported case: a namesake from kansas got the profile of a german rap
// group. as soon as tracks of one's own are on hand, the work has to match
#[tokio::test]
#[ignore = "benötigt Internet"]
async fn kuenstler_muessen_zum_eigenen_werk_passen() {
    let _guard = serialize();

    // a fitting body of work, the match succeeds
    let treffer = robify_lib::commands::best_artist_match(
        "Radiohead",
        &["Creep".to_string(), "Karma Police".to_string()],
    )
    .await;
    assert!(treffer.is_some(), "richtiger Künstler wurde abgelehnt");

    // the same name but a work nobody there carries: refused
    let fremd = robify_lib::commands::best_artist_match(
        "Radiohead",
        &["Ein Lied das es dort nicht gibt 12345".to_string()],
    )
    .await;
    assert!(
        fremd.is_none(),
        "fremdes Profil übernommen: {:?}",
        fremd.map(|k| k.name)
    );
}

/// the kind of a release, asked by the name of the album.
///
///     cargo test --test metadata_live arten -- --ignored --nocapture
#[tokio::test]
#[ignore]
async fn arten_werden_erkannt() {
    let _reihe = serialize();
    for (kuenstler, album, erwartet) in [
        ("Yeat", "2093", "album"),
        ("Yeat", "ADL", "album"),
        ("Yeat", "DANGEROUS SUMMER", "ep"),
        ("Yeat", "Million Dollar Minion", "single"),
        ("Don Toliver", "OCTANE", "album"),
        ("Don Toliver", "Love Sick", "album"),
        ("Pashanim", "grünewürfelflow", "?"),
    ] {
        let gefunden = online::release_kind(kuenstler, album).await;
        println!("{album:24} erwartet {erwartet:8} gefunden {gefunden:?}");
    }
}
