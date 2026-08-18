//! Lädt einen echten Titel herunter und prüft die fertigen Metadaten.
//!
//! Die Quellen sperren zu dichte Zugriffe (YouTube antwortet dann mit 403),
//! deshalb laufen diese Tests einzeln:
//!
//!     cargo test --test download_live -- --ignored --nocapture --test-threads=1

use robify_lib::downloader::{self, DownloadOptions, DownloadRegistry, SearchSource};
use std::sync::Arc;

/// Zusätzliche Absicherung gegen parallele Zugriffe. Bewusst ein
/// blockierender Mutex: jeder `#[tokio::test]` bringt eine eigene Laufzeit
/// mit, ein `tokio::sync::Mutex` serialisiert über deren Grenzen hinweg nicht
/// verlässlich.
static SOURCES: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialize() -> std::sync::MutexGuard<'static, ()> {
    SOURCES.lock().unwrap_or_else(|poison| poison.into_inner())
}

/// Sperrt die Quelle den Zugriff (YouTube antwortet nach vielen Abrufen mit
/// 403), lässt sich nichts prüfen. Das ist kein Fehler im Programm, deshalb
/// wird der Test übersprungen statt fehlzuschlagen.
fn ist_gesperrt(error: &str) -> bool {
    let lower = error.to_lowercase();
    // Der Downloader übersetzt 403/429 bereits in Klartext.
    lower.contains("(403)") || lower.contains("403") || lower.contains("429")
}

#[tokio::test]
#[ignore = "benötigt Internet, yt-dlp und ffmpeg"]
async fn kanal_wird_hauptkuenstler_und_metadaten_stimmen() {
    let _guard = serialize();
    let ytdlp = downloader::find_ytdlp(None, std::path::Path::new(".")).expect("yt-dlp gefunden");
    let work_dir = std::env::temp_dir().join("robify-download-live");
    let _ = std::fs::remove_dir_all(&work_dir);

    // Tauri-App für die Fortschrittsereignisse.
    let app = tauri::test::mock_app();

    let outcome = downloader::download(
        app.handle().clone(),
        Arc::new(DownloadRegistry::default()),
        ytdlp,
        work_dir.clone(),
        "live-test".into(),
        DownloadOptions {
            url: "scsearch1:PA69 Die Welt zu Gast bei Feinden".into(),
            // Genau wie in der App: SoundCloud zuerst, YouTube als Ausweich.
            fallbacks: vec!["ytsearch1:PA69 Die Welt zu Gast bei Feinden audio".into()],
            format: "mp3".into(),
            quality: Some("7".into()),
            // Eingeschaltet, damit der Weg über die Nachbearbeitung mitläuft:
            // Ohne die Python-Bibliothek `mutagen` brach yt-dlp hier den
            // ganzen Download ab, obwohl der Ton längst geladen war.
            embed_thumbnail: true,
            metadata: None,
            auto_match: true,
            auto_cover: true,
            auto_lyrics: true,
            expected_duration_ms: None,
            match_query: None,
            intent: None,
        },
    )
    .await;

    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(error) if ist_gesperrt(&error.to_string()) => {
            eprintln!("Quelle sperrt gerade den Zugriff, Test übersprungen: {error}");
            return;
        }
        Err(error) => panic!("Download fehlgeschlagen: {error}"),
    };

    assert!(
        downloader::is_playable(std::path::Path::new(&outcome.path)),
        "nicht abspielbares Format: {}",
        outcome.path
    );

    let m = &outcome.metadata;
    println!("Titel:       {}", m.title);
    println!("Künstler:    {}", m.artist);
    println!("Gäste:       {:?}", m.featured_artists);
    println!("Album:       {:?}", m.album);
    println!("Release-Art: {:?}", m.release_type);
    println!("Jahr/Genre:  {:?} / {:?}", m.year, m.genre);
    println!("Cover:       {}", m.cover_base64.is_some());
    println!(
        "Lyrics:      {} Zeilen",
        m.lyrics_plain.as_deref().unwrap_or("").lines().count()
    );

    // Das SoundCloud-Konto „PA69“ ist der Hauptkünstler …
    assert_eq!(m.artist, "PA69", "Kanal wurde nicht zum Hauptkünstler");
    // … und der Rest steht als Gast dabei.
    assert_eq!(
        m.featured_artists.as_deref(),
        Some("Drunken Masters"),
        "Gastkünstler stimmen nicht"
    );
    assert_eq!(m.release_type.as_deref(), Some("single"));
    assert!(m.lyrics_plain.is_some(), "Lyrics fehlen");
    // Der ganze Titel, kein 30-Sekunden-Ausschnitt.
    assert!(
        outcome.duration_ms > 90_000,
        "nur ein Ausschnitt: {} ms",
        outcome.duration_ms
    );

    let _ = std::fs::remove_dir_all(&work_dir);
}

/// SoundCloud gibt für manche Titel nur einen 30-Sekunden-Ausschnitt heraus.
/// Dann muss der Download auf die nächste Quelle ausweichen.
#[tokio::test]
#[ignore = "benötigt Internet, yt-dlp und ffmpeg"]
async fn weicht_bei_vorschauen_auf_die_naechste_quelle_aus() {
    let _guard = serialize();
    let ytdlp = downloader::find_ytdlp(None, std::path::Path::new(".")).expect("yt-dlp gefunden");
    let work_dir = std::env::temp_dir().join("robify-preview-live");
    let _ = std::fs::remove_dir_all(&work_dir);
    let app = tauri::test::mock_app();

    let outcome = downloader::download(
        app.handle().clone(),
        Arc::new(DownloadRegistry::default()),
        ytdlp,
        work_dir.clone(),
        "preview-test".into(),
        DownloadOptions {
            // Diese SoundCloud-Seite bietet ausschließlich Vorschau-Formate.
            url: "https://soundcloud.com/pa69-music/die-welt-zu-gast-bei-feinden".into(),
            fallbacks: vec!["ytsearch1:PA69 Die Welt zu Gast bei Feinden audio".into()],
            format: "mp3".into(),
            quality: Some("7".into()),
            embed_thumbnail: false,
            metadata: None,
            auto_match: false,
            auto_cover: false,
            auto_lyrics: false,
            expected_duration_ms: None,
            match_query: None,
            intent: None,
        },
    )
    .await;

    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(error) if ist_gesperrt(&error.to_string()) => {
            eprintln!("Quelle sperrt gerade den Zugriff, Test übersprungen: {error}");
            return;
        }
        Err(error) => panic!("Download fehlgeschlagen: {error}"),
    };

    let seconds = outcome.duration_ms / 1000;
    println!("Länge: {} Sekunden ({})", seconds, outcome.path);
    assert!(
        seconds > 90,
        "es wurde nur ein Ausschnitt geladen: {seconds} Sekunden"
    );

    let _ = std::fs::remove_dir_all(&work_dir);
}

/// Aus mehreren Treffern muss der mit der passenden Laufzeit gewählt werden,
/// so wie es bei einem Spotify-Link passiert.
#[tokio::test]
#[ignore = "benötigt Internet und yt-dlp"]
async fn waehlt_die_passende_aufnahme_aus_mehreren_treffern() {
    let _guard = serialize();
    let ytdlp = downloader::find_ytdlp(None, std::path::Path::new(".")).expect("yt-dlp gefunden");

    // Die echte Laufzeit, wie Spotify sie kennt.
    let expected_ms = 167_000;
    let urls = downloader::select_matches(
        &ytdlp,
        "PA69 Die Welt zu Gast bei Feinden",
        Some(expected_ms),
        "Die Welt zu Gast bei Feinden",
    )
    .await
    .expect("keine Treffer");

    println!("Reihenfolge:");
    for url in &urls {
        println!("   {url}");
    }
    assert!(!urls.is_empty(), "Auswahl lieferte nichts");

    // Der erste Vorschlag muss längenmäßig passen.
    let best = downloader::search(&ytdlp, &urls[0], SearchSource::Url, 1)
        .await
        .unwrap();
    let duration = best[0].duration_ms.unwrap_or(0);
    println!("Bester Treffer: {} ({} ms)", best[0].title, duration);
    assert!(
        (duration - expected_ms).abs() < 10_000,
        "Länge passt nicht: {duration} statt {expected_ms}"
    );
}

/// Ohne JavaScript-Laufzeit weicht yt-dlp bei YouTube auf einen veralteten
/// Weg aus, dessen Adressen mit 403 abgelehnt werden.
#[tokio::test]
#[ignore = "benötigt Internet und yt-dlp"]
async fn youtube_download_ohne_403() {
    let _guard = serialize();
    println!("JavaScript-Laufzeit: {:?}", downloader::js_runtime());

    let ytdlp = downloader::find_ytdlp(None, std::path::Path::new(".")).expect("yt-dlp gefunden");
    let work_dir = std::env::temp_dir().join("robify-js-live");
    let _ = std::fs::remove_dir_all(&work_dir);
    let app = tauri::test::mock_app();

    let outcome = downloader::download(
        app.handle().clone(),
        Arc::new(DownloadRegistry::default()),
        ytdlp,
        work_dir.clone(),
        "js-test".into(),
        DownloadOptions {
            url: "https://www.youtube.com/watch?v=8FLjrUXg5RY".into(),
            fallbacks: Vec::new(),
            match_query: None,
            format: "best".into(),
            quality: None,
            // „Beste Qualität“ liefert bei YouTube Opus, und genau dort
            // verlangt yt-dlp `mutagen` fürs Cover. Der gemeldete Fall.
            embed_thumbnail: true,
            metadata: None,
            auto_match: false,
            auto_cover: false,
            auto_lyrics: false,
            expected_duration_ms: None,
            intent: None,
        },
    )
    .await;

    match outcome {
        Ok(outcome) => {
            println!("Geladen: {} ({} ms)", outcome.path, outcome.duration_ms);
            assert!(outcome.duration_ms > 60_000, "zu kurz");
            // „Beste Qualität“ darf kein Opus liefern, das kann der
            // eingebaute Player nicht abspielen.
            assert!(
                downloader::is_playable(std::path::Path::new(&outcome.path)),
                "nicht abspielbares Format: {}",
                outcome.path
            );
        }
        Err(error) if ist_gesperrt(&error.to_string()) => {
            eprintln!("Quelle sperrt gerade den Zugriff, Test übersprungen: {error}");
        }
        Err(error) => panic!("YouTube-Download fehlgeschlagen: {error}"),
    }

    let _ = std::fs::remove_dir_all(&work_dir);
}

/// DRM-geschützte Quellen dürfen den Download nicht beenden.
///
/// SoundCloud gibt die Label-Uploads bekannter Künstler nur als Stream
/// heraus. Das steht in keinem Suchergebnis, erst der Ladeversuch scheitert.
/// Robify muss dann selbstständig zur nächsten Quelle wechseln.
#[tokio::test]
#[ignore = "benötigt Internet, yt-dlp und ffmpeg"]
async fn drm_quelle_wird_uebersprungen() {
    let _guard = serialize();
    let ytdlp = downloader::find_ytdlp(None, std::path::Path::new(".")).expect("yt-dlp gefunden");
    let work_dir = std::env::temp_dir().join("robify-drm-live");
    let _ = std::fs::remove_dir_all(&work_dir);
    let app = tauri::test::mock_app();

    // Bei diesen Titeln lag im Messlauf ein DRM-Upload ganz oben.
    let treffer = downloader::search_everywhere(&ytdlp, "Nina Chuba Wildberry Lillet", 4)
        .await
        .expect("Suche");
    let plaene = downloader::plans_with_fallbacks(treffer);
    let plan = plaene.first().expect("mindestens ein Treffer");
    println!("Erste Wahl: [{}] {}", plan.source, plan.title);
    assert!(!plan.fallbacks.is_empty(), "ohne Ausweichadressen kein Ausweg");

    let outcome = downloader::download(
        app.handle().clone(),
        Arc::new(DownloadRegistry::default()),
        ytdlp,
        work_dir.clone(),
        "drm-test".into(),
        DownloadOptions {
            url: plan.url.clone(),
            fallbacks: plan.fallbacks.clone(),
            match_query: None,
            format: "best".into(),
            quality: None,
            embed_thumbnail: false,
            metadata: None,
            auto_match: false,
            auto_cover: false,
            auto_lyrics: false,
            expected_duration_ms: None,
            intent: None,
        },
    )
    .await;

    match outcome {
        Ok(outcome) => {
            println!("Geladen: {} ({} ms)", outcome.path, outcome.duration_ms);
            assert!(outcome.duration_ms > 60_000, "zu kurz");
        }
        Err(error) if ist_gesperrt(&error.to_string()) => {
            eprintln!("Quellen gesperrt, Test übersprungen: {error}");
        }
        Err(error) => panic!("keine Quelle lieferte den Titel: {error}"),
    }

    let _ = std::fs::remove_dir_all(&work_dir);
}
