//! downloads tracks the way the app does and holds every field against what
//! the catalogues say.
//!
//!     cargo test --test abgleich_report -- --ignored --nocapture
//!
//! the yardstick is deezer: one query gives title, artist and album of the
//! recording, a second gives the kind of that album. unlike a plausibility
//! check this says not only "a value is there" but "the value is right".
//!
//! what is compared is meaning, not spelling. "Tití Me Preguntó" and "Titi Me
//! Pregunto" are the same track, and a catalogue writing "(Remastered 2011)"
//! after the title has not named a different one.

use robify_lib::downloader::{self, DownloadOptions, DownloadRegistry};
use robify_lib::online;
use std::sync::Arc;

/// widely spread: languages, decades, genres, and every kind of release.
const SONGS: [(&str, &str); 25] = [
    // rap, international
    ("Kendrick Lamar", "Money Trees"),
    ("Travis Scott", "SICKO MODE"),
    ("Yeat", "Breathe"),
    ("Drake", "Passionfruit"),
    // german
    ("AnnenMayKantereit", "Oft gefragt"),
    ("Pashanim", "Airwaves"),
    ("Kraftwerk", "Das Model"),
    ("Rammstein", "Sonne"),
    // pop
    ("Billie Eilish", "bad guy"),
    ("Dua Lipa", "Levitating"),
    ("The Weeknd", "Blinding Lights"),
    // rock
    ("Queen", "Bohemian Rhapsody"),
    ("Nirvana", "Come As You Are"),
    ("Radiohead", "Creep"),
    ("Arctic Monkeys", "505"),
    // electronic
    ("Daft Punk", "Instant Crush"),
    ("Aphex Twin", "Xtal"),
    ("Bicep", "Glue"),
    ("Burial", "Archangel"),
    // older
    ("Michael Jackson", "Billie Jean"),
    ("ABBA", "Dancing Queen"),
    ("Fleetwood Mac", "Dreams"),
    ("Nina Simone", "Feeling Good"),
    // beyond the anglophone world
    ("Bad Bunny", "Tití Me Preguntó"),
    ("Stromae", "Alors on danse"),
];

/// what the catalogue says a recording is.
#[derive(Debug, Default)]
struct Wahrheit {
    titel: Option<String>,
    kuenstler: Option<String>,
    album: Option<String>,
    art: Option<String>,
    jahr: Option<i64>,
}

async fn wahrheit_holen(artist: &str, titel: &str) -> Wahrheit {
    let url = format!(
        "https://api.deezer.com/search?q={}&limit=1",
        urlencoding::encode(&format!("{artist} {titel}"))
    );
    let Ok(antwort) = online::client().get(&url).send().await else {
        return Wahrheit::default();
    };
    let Ok(daten) = antwort.json::<serde_json::Value>().await else {
        return Wahrheit::default();
    };
    let Some(treffer) = daten["data"].as_array().and_then(|l| l.first()) else {
        return Wahrheit::default();
    };

    let mut w = Wahrheit {
        titel: treffer["title"].as_str().map(str::to_string),
        kuenstler: treffer["artist"]["name"].as_str().map(str::to_string),
        album: treffer["album"]["title"].as_str().map(str::to_string),
        ..Wahrheit::default()
    };

    // the kind of the release stands on the album, not on the track
    if let Some(album_id) = treffer["album"]["id"].as_i64() {
        let url = format!("https://api.deezer.com/album/{album_id}");
        if let Ok(antwort) = online::client().get(&url).send().await {
            if let Ok(album) = antwort.json::<serde_json::Value>().await {
                w.art = album["record_type"].as_str().map(|art| {
                    match art {
                        "single" => "single",
                        "ep" => "ep",
                        _ => "album",
                    }
                    .to_string()
                });
                w.jahr = album["release_date"]
                    .as_str()
                    .and_then(|d| d.get(0..4))
                    .and_then(|j| j.parse().ok());
            }
        }
    }
    w
}

/// whether two texts name the same thing.
///
/// a catalogue writes "(Remastered 2011)" behind a title and means the same
/// recording, and accents fall away in one place and not in the other. what
/// counts is that the shorter is contained in the longer.
fn gleiche_sache(unser: &str, echt: &str) -> bool {
    let a = online::normalize_words(unser);
    let b = online::normalize_words(echt);
    if a.is_empty() || b.is_empty() {
        return false;
    }
    online::contains_word_sequence(&a, &b) || online::contains_word_sequence(&b, &a)
}

#[tokio::test]
#[ignore = "benötigt Internet, yt-dlp und ffmpeg; dauert 15 bis 25 Minuten"]
async fn abgleich_mit_den_katalogen() {
    let ytdlp = downloader::find_ytdlp(None, std::path::Path::new(".")).expect("yt-dlp gefunden");
    let work = std::env::temp_dir().join("robify-abgleich");
    let _ = std::fs::remove_dir_all(&work);
    let app = tauri::test::mock_app();

    let mut geprueft = 0usize;
    let mut treffer: std::collections::BTreeMap<&str, (usize, usize)> = Default::default();
    let mut abweichungen: Vec<String> = Vec::new();
    let mut ausgefallen: Vec<String> = Vec::new();

    println!("\n{:<34} {:<8} {}", "Titel", "Feld", "unser  ·  echt");
    println!("{}", "-".repeat(96));

    for (index, (artist, titel)) in SONGS.iter().enumerate() {
        let query = format!("{artist} {titel}");

        let gefunden = downloader::search_everywhere(&ytdlp, &query, 5)
            .await
            .unwrap_or_default();
        let plaene = downloader::plans_with_fallbacks(gefunden);
        let Some(plan) = plaene.first() else {
            ausgefallen.push(format!("{query}: keine Treffer"));
            continue;
        };

        let outcome = downloader::download(
            app.handle().clone(),
            Arc::new(DownloadRegistry::default()),
            ytdlp.clone(),
            work.clone(),
            format!("abgleich-{index}"),
            DownloadOptions {
                url: plan.url.clone(),
                fallbacks: plan.fallbacks.clone(),
                match_query: None,
                format: "best".into(),
                quality: None,
                embed_thumbnail: true,
                metadata: None,
                auto_match: true,
                auto_cover: true,
                auto_lyrics: true,
                expected_duration_ms: None,
                intent: Some(query.clone()),
            },
        )
        .await;

        let outcome = match outcome {
            Ok(werte) => werte,
            Err(fehler) => {
                let grund = fehler.to_string().lines().next().unwrap_or("").to_string();
                ausgefallen.push(format!("{query}: {}", grund.chars().take(70).collect::<String>()));
                continue;
            }
        };

        geprueft += 1;
        let unser = &outcome.metadata;
        let echt = wahrheit_holen(artist, titel).await;

        let mut zeile = |feld: &'static str, unser: &str, echt: Option<&str>, gleich: bool| {
            let Some(echt) = echt else { return };
            let eintrag = treffer.entry(feld).or_default();
            eintrag.1 += 1;
            if gleich {
                eintrag.0 += 1;
            } else {
                abweichungen.push(format!("{query} · {feld}: „{unser}“ statt „{echt}“"));
                println!("{:<34} {:<8} „{}“  ·  „{}“", query, feld, unser, echt);
            }
        };

        zeile(
            "Titel",
            &unser.title,
            echt.titel.as_deref(),
            echt.titel.as_deref().is_some_and(|e| gleiche_sache(&unser.title, e)),
        );
        zeile(
            "Künstler",
            &unser.artist,
            echt.kuenstler.as_deref(),
            echt.kuenstler.as_deref().is_some_and(|e| gleiche_sache(&unser.artist, e)),
        );
        zeile(
            "Album",
            &unser.album,
            echt.album.as_deref(),
            echt.album.as_deref().is_some_and(|e| gleiche_sache(&unser.album, e)),
        );
        zeile(
            "Art",
            unser.release_type.as_deref().unwrap_or("—"),
            echt.art.as_deref(),
            unser.release_type.as_deref() == echt.art.as_deref(),
        );
        let unser_jahr = unser.year.map(|j| j.to_string()).unwrap_or_else(|| "—".into());
        zeile(
            "Jahr",
            &unser_jahr,
            echt.jahr.map(|j| j.to_string()).as_deref(),
            unser.year == echt.jahr,
        );
    }

    println!("\n{}", "=".repeat(96));
    println!("{geprueft} von {} Titeln geladen", SONGS.len());
    for (feld, (gut, gesamt)) in &treffer {
        let anteil = if *gesamt > 0 { gut * 100 / gesamt } else { 0 };
        println!("  {feld:<10} {gut:>3} von {gesamt:>3}   {anteil:>3} %");
    }
    if !ausgefallen.is_empty() {
        println!("\nnicht geladen:");
        for zeile in &ausgefallen {
            println!("  {zeile}");
        }
    }
}
