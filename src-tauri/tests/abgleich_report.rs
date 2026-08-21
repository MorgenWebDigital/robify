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

/// widely spread: languages, decades, genres, and every kind of release
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

/// what the catalogue says a recording is
#[derive(Debug, Default)]
struct Wahrheit {
    titel: Option<String>,
    kuenstler: Option<String>,
    album: Option<String>,
    art: Option<String>,
    jahr: Option<i64>,
}

/// what the catalogues say about the recording.
///
/// two of them, and they are asked different questions — each what it
/// actually knows.
///
/// **deezer** for title and artist: one query, cleanly separated fields, and
/// its answer agreed with the reality of every hand-checked case.
///
/// **itunes** for the year: it stamps the original release date onto a track
/// even where the track sits on a later record, and the earliest of its hits
/// is the first appearance. deezer cannot do this — its `release_date` is the
/// date of the catalogue entry, and "A Night at the Opera" reads 2005 there.
///
/// **for album and kind neither of them is enough.** deezer names the version
/// played most — "Dancing Queen" out of "ABBA Gold", "Feeling Good" as a
/// remix from 2022 —, and itunes hangs the original date onto a greatest-hits
/// record. so both are asked, and the field only counts where they agree.
/// where they do not, it stays empty and the report leaves it out of the
/// score rather than measuring its own yardstick.
async fn wahrheit_holen(artist: &str, titel: &str) -> Wahrheit {
    let (bei_deezer, bei_itunes) = tokio::join!(
        deezer_wahrheit(artist, titel),
        itunes_wahrheit(artist, titel)
    );

    let (titel_echt, kuenstler_echt, album_deezer, art_deezer) = bei_deezer;
    let (jahr_echt, album_itunes) = bei_itunes;

    // album and kind only where both catalogues name the same record
    let einig = match (album_deezer.as_deref(), album_itunes.as_deref()) {
        (Some(a), Some(b)) => gleiche_sache(a, b),
        _ => false,
    };

    Wahrheit {
        titel: titel_echt,
        kuenstler: kuenstler_echt,
        album: if einig { album_deezer.clone() } else { None },
        art: if einig { art_deezer } else { None },
        jahr: jahr_echt,
    }
}

/// whether a hit means this recording and not a version of it
fn meint_die_aufnahme(name: &str, wer: &str, titel: &str, artist: &str) -> bool {
    gleiche_sache(name, titel)
        && gleiche_sache(wer, artist)
        // "(Don Diablo Extended Remix)" is a different recording
        && online::klammerzusaetze(name)
            .iter()
            .all(|zusatz| online::ist_nur_beiwerk(zusatz))
}

/// title, artist, album and kind as deezer carries them
async fn deezer_wahrheit(
    artist: &str,
    titel: &str,
) -> (Option<String>, Option<String>, Option<String>, Option<String>) {
    let leer = (None, None, None, None);
    let url = format!(
        "https://api.deezer.com/search/track?q={}&limit=25",
        urlencoding::encode(&format!("{artist} {titel}"))
    );
    let Ok(antwort) = online::client().get(&url).send().await else {
        return leer;
    };
    let Ok(daten) = antwort.json::<serde_json::Value>().await else {
        return leer;
    };
    let Some(treffer) = daten["data"].as_array().and_then(|liste| {
        liste.iter().find(|treffer| {
            meint_die_aufnahme(
                treffer["title"].as_str().unwrap_or_default(),
                treffer["artist"]["name"].as_str().unwrap_or_default(),
                titel,
                artist,
            )
        })
    }) else {
        return leer;
    };

    let mut art = None;
    // the kind of the release stands on the album, not on the track
    if let Some(album_id) = treffer["album"]["id"].as_i64() {
        let url = format!("https://api.deezer.com/album/{album_id}");
        if let Ok(antwort) = online::client().get(&url).send().await {
            if let Ok(album) = antwort.json::<serde_json::Value>().await {
                art = album["record_type"].as_str().map(|art| {
                    match art {
                        "single" => "single",
                        "ep" => "ep",
                        _ => "album",
                    }
                    .to_string()
                });
            }
        }
    }

    (
        treffer["title"].as_str().map(str::to_string),
        treffer["artist"]["name"].as_str().map(str::to_string),
        treffer["album"]["title"].as_str().map(str::to_string),
        art,
    )
}

/// the year of the first appearance, and the record itunes hangs it on
async fn itunes_wahrheit(artist: &str, titel: &str) -> (Option<i64>, Option<String>) {
    let url = format!(
        "https://itunes.apple.com/search?term={}&entity=song&limit=50",
        urlencoding::encode(&format!("{artist} {titel}"))
    );
    let Ok(antwort) = online::client().get(&url).send().await else {
        return (None, None);
    };
    let Ok(daten) = antwort.json::<serde_json::Value>().await else {
        return (None, None);
    };

    let mut frueheste: Option<(String, Option<String>)> = None;
    for treffer in daten["results"].as_array().map(|l| l.as_slice()).unwrap_or_default() {
        if !meint_die_aufnahme(
            treffer["trackName"].as_str().unwrap_or_default(),
            treffer["artistName"].as_str().unwrap_or_default(),
            titel,
            artist,
        ) {
            continue;
        }
        let Some(datum) = treffer["releaseDate"].as_str().filter(|d| d.len() >= 4) else {
            continue;
        };
        // dates compare as text: they all read yyyy-mm-dd
        if frueheste.as_ref().is_none_or(|(bisher, _)| datum < bisher.as_str()) {
            frueheste = Some((
                datum.to_string(),
                treffer["collectionName"].as_str().map(str::to_string),
            ));
        }
    }

    match frueheste {
        Some((datum, sammlung)) => (
            datum.get(0..4).and_then(|jahr| jahr.parse().ok()),
            // itunes writes the kind into the name ("… - EP"), and that is
            // not part of the album name
            sammlung.map(|name| {
                name.trim_end_matches(" - EP")
                    .trim_end_matches(" - Single")
                    .to_string()
            }),
        ),
        None => (None, None),
    }
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
    // the same yt-dlp the app works with, not whatever lies in the path.
    //
    // that mattered: the system copy came from pip and was two months old,
    // and youtube answered eight of the twenty-five with 403. the same
    // addresses load without a complaint through a current one. robify
    // fetches a standalone build of its own for exactly this reason, and the
    // report has to measure that one
    let werkzeuge = std::env::temp_dir().join("robify-werkzeuge");
    let _ = std::fs::create_dir_all(&werkzeuge);
    let ytdlp = downloader::managed_ytdlp(&werkzeuge);
    // `ensure_ytdlp` would take whatever lies in the path, and that is
    // exactly what must not be measured here
    let ytdlp = if ytdlp.exists() {
        ytdlp
    } else {
        downloader::eigenes_holen(&werkzeuge)
            .await
            .expect("yt-dlp geholt")
    };
    let fassung = tokio::process::Command::new(&ytdlp)
        .arg("--version")
        .output()
        .await
        .map(|a| String::from_utf8_lossy(&a.stdout).trim().to_string())
        .unwrap_or_default();
    println!("yt-dlp {fassung} aus {}", ytdlp.display());
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
                // as in the app: the name from the search travels along
                plan_title: Some(plan.title.clone()),
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
        let offen = geprueft.saturating_sub(*gesamt);
        // a field counted on fewer titles than were loaded is no gap in
        // robify: there the two catalogues named different records, and a
        // yardstick that contradicts itself measures nothing
        let dazu = if offen > 0 {
            format!("   ({offen} ohne verlässlichen Vergleich)")
        } else {
            String::new()
        };
        println!("  {feld:<10} {gut:>3} von {gesamt:>3}   {anteil:>3} %{dazu}");
    }
    if !ausgefallen.is_empty() {
        println!("\nnicht geladen:");
        for zeile in &ausgefallen {
            println!("  {zeile}");
        }
    }
}
