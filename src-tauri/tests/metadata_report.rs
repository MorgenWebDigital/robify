//! large measured run: downloads many tracks the way the app does and
//! examines every metadata field separately.
//!
//!     cargo test --test metadata_report -- --ignored --nocapture
//!
//! two yardsticks are applied:
//!
//! * the source. yt-dlp delivers structured fields for music (`track`,
//!   `artist`, `album`, `release_year`). the address actually downloaded from
//!   is queried, not the one originally suggested.
//! * plausibility. even without a value to compare against, a field has to
//!   hold together: no video hints in the title, a year of four digits, no
//!   invisible characters, a running time above one minute.

use robify_lib::downloader::{self, DownloadOptions, DownloadRegistry};
use robify_lib::{db, library, online};
use std::collections::BTreeMap;
use std::process::Command;
use std::sync::Arc;

/// widely spread: languages, decades, genres, well known and less so
const SONGS: [(&str, &str); 40] = [
    // rap, international
    ("Yeat", "Naked"),
    ("Playboi Carti", "Sky"),
    ("Kendrick Lamar", "Money Trees"),
    ("Travis Scott", "SICKO MODE"),
    ("Drake", "Passionfruit"),
    ("A$AP Rocky", "Praise The Lord"),
    ("Tyler, The Creator", "EARFQUAKE"),
    ("J. Cole", "No Role Modelz"),
    // rap, german
    ("PA69", "Tropical Island"),
    ("Sido", "Astronaut"),
    ("Haftbefehl", "Chabos wissen wer der Babo ist"),
    ("Nina Chuba", "Wildberry Lillet"),
    ("Cro", "Easy"),
    ("Apache 207", "Roller"),
    // pop
    ("Billie Eilish", "Birds of a Feather"),
    ("The Weeknd", "Blinding Lights"),
    ("Dua Lipa", "Levitating"),
    ("Harry Styles", "As It Was"),
    ("Sabrina Carpenter", "Espresso"),
    ("Taylor Swift", "Anti-Hero"),
    ("Michael Jackson", "Billie Jean"),
    ("ABBA", "Dancing Queen"),
    // rock and indie
    ("Radiohead", "Creep"),
    ("Tame Impala", "The Less I Know The Better"),
    ("Arctic Monkeys", "505"),
    ("Nirvana", "Come As You Are"),
    ("Fleetwood Mac", "Dreams"),
    ("Queen", "Bohemian Rhapsody"),
    ("The Killers", "Mr. Brightside"),
    ("Rammstein", "Sonne"),
    // electronic
    ("Fred again..", "Delilah"),
    ("Daft Punk", "Instant Crush"),
    ("Aphex Twin", "Xtal"),
    ("Boards of Canada", "Roygbiv"),
    ("Bicep", "Glue"),
    ("Burial", "Archangel"),
    // german language, jazz, classical
    ("AnnenMayKantereit", "Oft gefragt"),
    ("Kraftwerk", "Das Model"),
    ("Nina Simone", "Feeling Good"),
    ("Ludovico Einaudi", "Nuvole Bianche"),
];

/// hints about the production that belong in no song title
const TITELMUELL: [&str; 11] = [
    "official video",
    "official audio",
    "official music video",
    "lyrics",
    "lyric video",
    "offizielles video",
    "visualizer",
    "full song",
    "free download",
    "www.",
    "download)",
];

#[derive(Debug, Default)]
struct Quelle {
    track: Option<String>,
    artist: Option<String>,
    jahr: Option<String>,
}

fn feld(wert: &str) -> Option<String> {
    let wert = wert.trim();
    (!wert.is_empty() && wert != "NA").then(|| wert.to_string())
}

fn quellenangaben(ytdlp: &std::path::Path, url: &str) -> Quelle {
    let mut cmd = Command::new(ytdlp);
    cmd.args(["--no-warnings", "--no-playlist"]);
    if let Some(runtime) = downloader::js_runtime() {
        cmd.arg("--js-runtimes").arg(runtime);
    }
    cmd.arg("--print")
        .arg("%(track)s\u{1f}%(artist)s\u{1f}%(release_year)s")
        .arg(url);

    let Ok(output) = cmd.output() else {
        return Quelle::default();
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let teile: Vec<&str> = text.lines().next().unwrap_or("").split('\u{1f}').collect();
    Quelle {
        track: teile.first().and_then(|w| feld(w)),
        artist: teile.get(1).and_then(|w| feld(w)),
        jahr: teile.get(2).and_then(|w| feld(w)),
    }
}

/// whether `nadel` sits in `heu` as a word sequence
fn enthaelt(heu: &str, nadel: &str) -> bool {
    online::contains_word_sequence(
        &online::normalize_words(heu),
        &online::normalize_words(nadel),
    )
}

#[tokio::test]
#[ignore = "benötigt Internet, yt-dlp und ffmpeg; dauert 15 bis 25 Minuten"]
async fn grosser_metadatenlauf() {
    let ytdlp = downloader::find_ytdlp(None, std::path::Path::new(".")).expect("yt-dlp gefunden");
    let work = std::env::temp_dir().join("robify-metadata-gross");
    let _ = std::fs::remove_dir_all(&work);
    let app = tauri::test::mock_app();

    // every check counts hits and failures separately
    let mut bestanden: BTreeMap<&str, usize> = BTreeMap::new();
    let mut gepruefte: BTreeMap<&str, usize> = BTreeMap::new();
    let mut maengel: Vec<String> = Vec::new();
    let mut ausgefallen: Vec<String> = Vec::new();
    let mut geladen = 0;
    let mut sauber = 0;
    // misgrasps robify reports of its own accord
    let mut gemeldet = 0;

    // re-checking single cases: ROBIFY_ONLY="Aphex,Bicep" narrows it down
    let nur = std::env::var("ROBIFY_ONLY").unwrap_or_default();
    let filter: Vec<&str> = nur.split(',').map(str::trim).filter(|f| !f.is_empty()).collect();

    for (index, (artist, titel)) in SONGS.iter().enumerate() {
        if !filter.is_empty() && !filter.iter().any(|f| artist.contains(f) || titel.contains(f)) {
            continue;
        }
        let query = format!("{artist} {titel}");

        let treffer = downloader::search_everywhere(&ytdlp, &query, 5)
            .await
            .unwrap_or_default();
        let plaene = downloader::plans_with_fallbacks(treffer);
        let Some(plan) = plaene.first() else {
            ausgefallen.push(format!("{query}: keine Treffer"));
            println!("?? {query:<46} keine Treffer");
            continue;
        };

        let outcome = downloader::download(
            app.handle().clone(),
            Arc::new(DownloadRegistry::default()),
            ytdlp.clone(),
            work.clone(),
            format!("gross-{index}"),
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
                // the search input, this way the downloader checks its own
                // result against it, as it does in the app
                intent: Some(query.clone()),
                plan_title: None,
            },
        )
        .await;

        let outcome = match outcome {
            Ok(werte) => werte,
            Err(fehler) => {
                let grund = fehler.to_string().lines().next().unwrap_or("").to_string();
                ausgefallen.push(format!(
                    "{query}: {}",
                    grund.chars().take(70).collect::<String>()
                ));
                println!("?? {query:<46} {}", grund.chars().take(58).collect::<String>());
                continue;
            }
        };

        geladen += 1;
        let m = &outcome.metadata;
        if let Some(warnung) = &outcome.warning {
            println!("   !! {warnung}");
        }
        // the yardstick is the address actually downloaded from
        let quelle = quellenangaben(&ytdlp, &outcome.source_url);

        let mut fehler: Vec<String> = Vec::new();
        {
            let mut pruefe = |name: &'static str, ok: bool, meldung: String| {
                *gepruefte.entry(name).or_default() += 1;
                if ok {
                    *bestanden.entry(name).or_default() += 1;
                } else {
                    fehler.push(meldung);
                }
            };

            // --- against the source ---
            if let Some(track) = &quelle.track {
                pruefe(
                    "Titel = Quelle",
                    online::looks_like_same(track, &m.title) || enthaelt(track, &m.title),
                    format!("Titel „{}“ statt „{track}“", m.title),
                );
            }
            if let Some(kuenstler) = &quelle.artist {
                let alle = format!(
                    "{} {}",
                    m.artist,
                    m.featured_artists.clone().unwrap_or_default()
                );
                // the source lists several separated by commas, all of them have to appear
                let ok = kuenstler
                    .split(',')
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .all(|name| enthaelt(&alle, name));
                pruefe(
                    "Künstler = Quelle",
                    ok,
                    format!(
                        "Künstler „{}“ (+{:?}) statt „{kuenstler}“",
                        m.artist, m.featured_artists
                    ),
                );
            }
            if let Some(jahr) = quelle.jahr.as_ref().and_then(|j| j.parse::<i64>().ok()) {
                pruefe(
                    "Jahr = Quelle",
                    m.year.is_some_and(|eigen| (eigen - jahr).abs() <= 1),
                    format!("Jahr {:?} statt {jahr}", m.year),
                );
            }

            // --- plausibility ---
            let titel_klein = m.title.to_lowercase();
            pruefe(
                "Titel ohne Videohinweise",
                !TITELMUELL.iter().any(|muell| titel_klein.contains(muell)),
                format!("Videohinweis im Titel: „{}“", m.title),
            );
            pruefe(
                "Titel ohne Künstlertrenner",
                library::split_video_title(&m.title, None).is_none(),
                format!("Künstler steckt noch im Titel: „{}“", m.title),
            );
            pruefe(
                "gesuchter Künstler enthalten",
                enthaelt(
                    &format!(
                        "{} {}",
                        m.artist,
                        m.featured_artists.clone().unwrap_or_default()
                    ),
                    artist,
                ),
                format!("„{artist}“ fehlt in „{}“", m.artist),
            );
            pruefe(
                "Jahr glaubwürdig",
                m.year.is_none_or(|jahr| (1900..=2030).contains(&jahr)),
                format!("unglaubwürdiges Jahr: {:?}", m.year),
            );
            pruefe("Cover vorhanden", m.cover_base64.is_some(), "kein Cover".into());
            pruefe(
                "Release-Art gesetzt",
                m.release_type.is_some(),
                "keine Release-Art".into(),
            );
            pruefe(
                "Album passt zur Release-Art",
                match m.release_type.as_deref() {
                    Some("single") => true,
                    Some(_) => !m.album.trim().is_empty(),
                    None => false,
                },
                format!("Art {:?} ohne Album", m.release_type),
            );
            let felder = format!("{} {} {}", m.title, m.artist, m.album);
            pruefe(
                "keine unsichtbaren Zeichen",
                db::clean_text(&felder) == felder.split_whitespace().collect::<Vec<_>>().join(" "),
                "unsichtbares Zeichen in den Feldern".into(),
            );
            pruefe(
                "vollständige Aufnahme",
                outcome.duration_ms > 60_000,
                format!("nur {} s", outcome.duration_ms / 1000),
            );
        }

        // instrumentals have no lyrics, which is no shortcoming but a
        // property of the track. count it as coverage only
        *gepruefte.entry("Lyrics gefunden (Abdeckung)").or_default() += 1;
        if m.lyrics_plain.is_some() || m.lyrics_synced.is_some() {
            *bestanden.entry("Lyrics gefunden (Abdeckung)").or_default() += 1;
        }

        if outcome.warning.is_some() {
            gemeldet += 1;
        }
        if fehler.is_empty() {
            sauber += 1;
        }
        println!(
            "{} {query:<46} {} | {} | {} | {:?}",
            if fehler.is_empty() { "ok" } else { "XX" },
            m.title.chars().take(26).collect::<String>(),
            m.artist.chars().take(22).collect::<String>(),
            m.album.chars().take(20).collect::<String>(),
            m.year,
        );
        for problem in &fehler {
            println!("     XX {problem}");
            maengel.push(format!("{query}: {problem}"));
        }
    }

    // --- evaluation ---
    println!("\n════════ Ergebnis ════════");
    println!("Titel angefordert:   {}", SONGS.len());
    println!("geladen:             {geladen}");
    println!("davon ohne Mangel:   {sauber}");
    println!("selbst gemeldet:     {gemeldet}");
    println!("nicht ladbar:        {}", ausgefallen.len());

    println!("\nPrüfung                         bestanden");
    for (name, anzahl) in &gepruefte {
        let ok = bestanden.get(name).copied().unwrap_or(0);
        let anteil = if *anzahl > 0 { ok * 100 / anzahl } else { 0 };
        println!("  {name:<29} {ok:>3}/{anzahl:<3} {anteil:>3}%");
    }

    println!("\nMängel ({}):", maengel.len());
    for eintrag in &maengel {
        println!("  XX {eintrag}");
    }
    println!("\nNicht ladbar ({}):", ausgefallen.len());
    for eintrag in &ausgefallen {
        println!("  ?? {eintrag}");
    }

    let _ = std::fs::remove_dir_all(&work);
}
