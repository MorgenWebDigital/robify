//! Prüft die Link- und Sucherkennung gegen die echten Dienste. Braucht Netz
//! und yt-dlp, deshalb standardmäßig deaktiviert:
//!
//!     cargo test --test resolve_live -- --ignored --nocapture

use robify_lib::downloader::{self, SearchSource};

#[tokio::test]
#[ignore = "benötigt Internet und yt-dlp"]
async fn suche_mischt_mehrere_quellen() {
    let ytdlp = downloader::find_ytdlp(None, std::path::Path::new(".")).expect("yt-dlp gefunden");
    let results = downloader::search_everywhere(&ytdlp, "rick astley never gonna give you up", 3)
        .await
        .unwrap();

    assert!(results.len() > 2, "zu wenige Treffer");
    let sources: std::collections::HashSet<&str> =
        results.iter().map(|r| r.source.as_str()).collect();
    assert!(
        sources.len() > 1,
        "es sollte mehr als eine Quelle vertreten sein, war: {sources:?}"
    );

    // Die Mischung sorgt für Vielfalt, die Reihenfolge aber für Treffsicherheit:
    // oben steht, was zum Suchbegriff passt, egal von welcher Quelle.
    for treffer in results.iter().take(3) {
        let text = format!("{} {}", treffer.title, treffer.uploader.clone().unwrap_or_default())
            .to_lowercase();
        assert!(
            text.contains("never gonna give you up") || text.contains("astley"),
            "unpassender Treffer weit oben: [{}] {}",
            treffer.source,
            treffer.title
        );
    }
}

/// Der gemeldete Fehler: ein fremder Titel (oder dessen Remix) wurde geladen.
#[tokio::test]
#[ignore = "benötigt Internet und yt-dlp"]
async fn auswahl_trifft_den_gesuchten_titel() {
    let ytdlp = downloader::find_ytdlp(None, std::path::Path::new(".")).expect("yt-dlp gefunden");

    for (query, titel, dauer_ms) in [
        ("PA69 Tropical Island", "Tropical Island", Some(139_000)),
        ("Radiohead Creep", "Creep", Some(238_000)),
    ] {
        let urls = downloader::select_matches(&ytdlp, query, dauer_ms, titel)
            .await
            .unwrap();
        assert!(!urls.is_empty(), "keine Auswahl für {query}");

        // Was oben landet, muss auch textlich passen.
        let treffer = downloader::search_everywhere(&ytdlp, query, 5).await.unwrap();
        let bester = treffer
            .iter()
            .find(|t| t.url == urls[0])
            .expect("gewählte Adresse stammt aus der Suche");
        println!("{query} → [{}] {}", bester.source, bester.title);

        let text = format!("{} {}", bester.title, bester.uploader.clone().unwrap_or_default())
            .to_lowercase();
        assert!(
            text.contains(&titel.to_lowercase()),
            "gewählt wurde ein anderer Titel: [{}] {}",
            bester.source,
            bester.title
        );
        assert!(
            !text.contains("remix"),
            "ein Remix wurde vorgezogen: {}",
            bester.title
        );
    }
}

#[tokio::test]
#[ignore = "benötigt Internet und yt-dlp"]
async fn einzelner_titel_bleibt_einzeln() {
    let ytdlp = downloader::find_ytdlp(None, std::path::Path::new(".")).expect("yt-dlp gefunden");
    let results = downloader::search(
        &ytdlp,
        "https://www.youtube.com/watch?v=LrM_Y39Gmhk",
        SearchSource::Url,
        1,
    )
    .await
    .unwrap();

    assert_eq!(results.len(), 1, "ein Video darf nicht zur Liste werden");
    assert!(!results[0].title.is_empty());
}

#[tokio::test]
#[ignore = "benötigt Internet und yt-dlp"]
async fn playlist_wird_aufgeklappt() {
    let ytdlp = downloader::find_ytdlp(None, std::path::Path::new(".")).expect("yt-dlp gefunden");
    let results = downloader::search(
        &ytdlp,
        "https://www.youtube.com/playlist?list=PLFgquLnL59alCl_2TQvOiD5Vgm1hCaGSI",
        SearchSource::Url,
        1,
    )
    .await
    .unwrap();

    assert!(results.len() > 1, "Playlist wurde nicht aufgeklappt");
}

/// Alle vier Quellen müssen etwas beisteuern.
#[tokio::test]
#[ignore = "benötigt Internet und yt-dlp"]
async fn alle_quellen_liefern_treffer() {
    let ytdlp = downloader::find_ytdlp(None, std::path::Path::new(".")).expect("yt-dlp gefunden");
    let results = downloader::search_everywhere(&ytdlp, "Radiohead Creep", 3)
        .await
        .unwrap();

    let mut nach_quelle: std::collections::BTreeMap<&str, usize> = Default::default();
    for treffer in &results {
        *nach_quelle.entry(treffer.source.as_str()).or_default() += 1;
    }
    println!("Treffer je Quelle: {nach_quelle:?}");
    for treffer in results.iter().take(6) {
        println!(
            "   [{}] {} | {:?} ms",
            treffer.source, treffer.title, treffer.duration_ms
        );
    }

    for quelle in ["Bandcamp", "Audius", "SoundCloud", "YouTube"] {
        assert!(
            nach_quelle.contains_key(quelle),
            "{quelle} lieferte nichts, gefunden: {nach_quelle:?}"
        );
    }

    // Audius nennt Laufzeiten, damit die Auswahl sicher greifen kann.
    assert!(
        results
            .iter()
            .any(|t| t.source == "Audius" && t.duration_ms.unwrap_or(0) > 0),
        "Audius ohne Laufzeit"
    );
}
