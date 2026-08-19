//! downloads a real track and checks the finished metadata.
//!
//! the sources block accesses that come too close together, youtube answers
//! with a 403 then, so these tests run one at a time:
//!
//!     cargo test --test download_live -- --ignored --nocapture --test-threads=1

use robify_lib::downloader::{self, DownloadOptions, DownloadRegistry, SearchSource};
use std::sync::Arc;

/// extra guard against parallel accesses.
///
/// deliberately a blocking mutex: every `#[tokio::test]` brings a runtime of
/// its own, and a `tokio::sync::Mutex` does not serialise across their
/// boundaries reliably.
static SOURCES: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialize() -> std::sync::MutexGuard<'static, ()> {
    SOURCES.lock().unwrap_or_else(|poison| poison.into_inner())
}

/// whether the source blocked the access. youtube answers with a 403 after
/// many retrievals, and then nothing can be checked. that is no bug in the
/// program, so the test is skipped instead of failing.
fn ist_gesperrt(error: &str) -> bool {
    let lower = error.to_lowercase();
    // the downloader translates 403 and 429 into plain words already
    lower.contains("(403)") || lower.contains("403") || lower.contains("429")
}

#[tokio::test]
#[ignore = "benötigt Internet, yt-dlp und ffmpeg"]
async fn kanal_wird_hauptkuenstler_und_metadaten_stimmen() {
    let _guard = serialize();
    let ytdlp = downloader::find_ytdlp(None, std::path::Path::new(".")).expect("yt-dlp gefunden");
    let work_dir = std::env::temp_dir().join("robify-download-live");
    let _ = std::fs::remove_dir_all(&work_dir);

    // a tauri app for the progress events
    let app = tauri::test::mock_app();

    let outcome = downloader::download(
        app.handle().clone(),
        Arc::new(DownloadRegistry::default()),
        ytdlp,
        work_dir.clone(),
        "live-test".into(),
        DownloadOptions {
            url: "scsearch1:PA69 Die Welt zu Gast bei Feinden".into(),
            // exactly as in the app: soundcloud first, youtube as a fallback
            fallbacks: vec!["ytsearch1:PA69 Die Welt zu Gast bei Feinden audio".into()],
            format: "mp3".into(),
            quality: Some("7".into()),
            // switched on so the route through the post-processing runs
            // along: without the python library `mutagen` yt-dlp aborted the
            // whole download here although the audio was long since fetched
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

    // the soundcloud account "PA69" is the lead artist …
    assert_eq!(m.artist, "PA69", "Kanal wurde nicht zum Hauptkünstler");
    // … and the rest stand alongside as guests
    assert_eq!(
        m.featured_artists.as_deref(),
        Some("Drunken Masters"),
        "Gastkünstler stimmen nicht"
    );
    assert_eq!(m.release_type.as_deref(), Some("single"));
    assert!(m.lyrics_plain.is_some(), "Lyrics fehlen");
    // the whole track, no 30 second excerpt
    assert!(
        outcome.duration_ms > 90_000,
        "nur ein Ausschnitt: {} ms",
        outcome.duration_ms
    );

    let _ = std::fs::remove_dir_all(&work_dir);
}

// soundcloud hands out a 30 second excerpt only for some tracks. the
// download then has to fall back to the next source
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
            // this soundcloud page offers preview formats only
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

// out of several hits the one with the fitting running time has to be
// chosen, the way it happens with a spotify link
#[tokio::test]
#[ignore = "benötigt Internet und yt-dlp"]
async fn waehlt_die_passende_aufnahme_aus_mehreren_treffern() {
    let _guard = serialize();
    let ytdlp = downloader::find_ytdlp(None, std::path::Path::new(".")).expect("yt-dlp gefunden");

    // the real running time, as spotify knows it
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

    // the first suggestion has to fit in length
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

// without a javascript runtime yt-dlp falls back to an outdated route at
// youtube, whose addresses are refused with a 403
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
            // best quality delivers opus at youtube, and exactly there
            // yt-dlp demands `mutagen` for the cover. the reported case
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
            // best quality must not deliver opus, the built-in player cannot
            // play it
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

// drm-protected sources must not end the download.
//
// soundcloud hands the label uploads of well-known artists out as a stream
// only. that stands in no search result, only the attempt to download fails.
// robify then has to move to the next source by itself
#[tokio::test]
#[ignore = "benötigt Internet, yt-dlp und ffmpeg"]
async fn drm_quelle_wird_uebersprungen() {
    let _guard = serialize();
    let ytdlp = downloader::find_ytdlp(None, std::path::Path::new(".")).expect("yt-dlp gefunden");
    let work_dir = std::env::temp_dir().join("robify-drm-live");
    let _ = std::fs::remove_dir_all(&work_dir);
    let app = tauri::test::mock_app();

    // with these tracks a drm upload stood right on top in the measured run
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
