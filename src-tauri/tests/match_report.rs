//! measured run for picking hits: for a list of known tracks it is checked
//! whether the downloader actually chooses the right recording.
//!
//! not a pass/fail test but a report, so it is off by default:
//!
//!     cargo test --test match_report -- --ignored --nocapture
//!
//! the online metadata (genius, itunes, musicbrainz) serves as the yardstick.
//! its running time says whether the chosen recording is plausible.

use robify_lib::{downloader, online};

/// a colourful mix: rap, pop, rock, electronic, german and english, well
/// known and less so.
const SONGS: [(&str, &str); 15] = [
    ("Yeat", "Naked"),
    ("PA69", "Tropical Island"),
    ("Radiohead", "Creep"),
    ("Billie Eilish", "Birds of a Feather"),
    ("Kendrick Lamar", "Money Trees"),
    ("Fred again..", "Delilah"),
    ("AnnenMayKantereit", "Oft gefragt"),
    ("Tame Impala", "The Less I Know The Better"),
    ("Travis Scott", "SICKO MODE"),
    ("Nina Chuba", "Wildberry Lillet"),
    ("Daft Punk", "Instant Crush"),
    ("Playboi Carti", "Sky"),
    ("Aphex Twin", "Xtal"),
    ("Sido", "Astronaut"),
    ("The Weeknd", "Blinding Lights"),
];

/// words giving a foreign rework away.
///
/// deliberately listed here again: the report is to check independently of
/// what the downloader itself considers a suffix.
const FREMDE_FASSUNG: [&str; 12] = [
    "remix", "cover", "edit", "bootleg", "flip", "rework", "live", "instrumental", "karaoke",
    "slowed", "sped", "mashup",
];

/// median running time of the hits, as an emergency yardstick where the
/// metadata sources hand out nothing.
///
/// deliberately simple and calculated differently from the downloader, so the
/// report does not confirm the very assumption it is meant to check.
fn median_dauer(treffer: &[downloader::SearchResult]) -> Option<i64> {
    let mut dauern: Vec<i64> = treffer
        .iter()
        .filter_map(|t| t.duration_ms)
        .filter(|ms| *ms > 0)
        .collect();
    if dauern.len() < 3 {
        return None;
    }
    dauern.sort_unstable();
    Some(dauern[dauern.len() / 2])
}

fn ist_fremde_fassung(titel: &str, gesucht: &str) -> bool {
    let titel = online::normalize_words(titel);
    let gesucht = online::normalize_words(gesucht);
    FREMDE_FASSUNG.iter().any(|marke| {
        online::contains_word_sequence(&titel, marke)
            && !online::contains_word_sequence(&gesucht, marke)
    })
}

/// running time and written form of the track from the metadata sources
async fn referenz(artist: &str, titel: &str) -> Option<(String, i64)> {
    let treffer = online::search_metadata(&format!("{artist} {titel}")).await.ok()?;
    let mit_laufzeit: Vec<_> = treffer
        .into_iter()
        .filter(|c| c.duration_ms.unwrap_or(0) > 0 && !ist_fremde_fassung(&c.title, titel))
        .collect();

    // the hit where title and artist both fit first …
    mit_laufzeit
        .iter()
        .find(|c| {
            online::looks_like_same(&c.title, titel) && online::looks_like_same(&c.artist, artist)
        })
        // … otherwise the title suffices
        .or_else(|| {
            mit_laufzeit
                .iter()
                .find(|c| online::looks_like_same(&c.title, titel))
        })
        .map(|c| (c.title.clone(), c.duration_ms.unwrap_or(0)))
}

/// single-case tool: shows every hit of a search in the order robify offers
/// them in.
///
///     ROBIFY_QUERY="Yeat Naked" cargo test --test match_report -- --ignored alle_treffer --nocapture
#[tokio::test]
#[ignore = "benötigt Internet und yt-dlp"]
async fn alle_treffer_einer_suche() {
    let query = std::env::var("ROBIFY_QUERY").unwrap_or_else(|_| "Yeat Naked".into());
    let ytdlp = downloader::find_ytdlp(None, std::path::Path::new(".")).expect("yt-dlp gefunden");

    let treffer = downloader::search_everywhere(&ytdlp, &query, 5)
        .await
        .unwrap_or_default();

    println!("\n„{query}“, {} Treffer:", treffer.len());
    for (rang, kandidat) in treffer.iter().enumerate() {
        println!(
            "{:>3}. [{:>10}] {:<60} {:>4} s  von {}",
            rang + 1,
            kandidat.source,
            kandidat.title.chars().take(60).collect::<String>(),
            kandidat.duration_ms.unwrap_or(0) / 1000,
            kandidat.uploader.clone().unwrap_or_else(|| "?".into()),
        );
    }
}

#[tokio::test]
#[ignore = "benötigt Internet und yt-dlp"]
async fn bericht_ueber_fehlgriffe() {
    let ytdlp = downloader::find_ytdlp(None, std::path::Path::new(".")).expect("yt-dlp gefunden");

    let mut geprueft = 0;
    let mut daneben = Vec::new();
    let mut ohne_referenz = Vec::new();

    for (artist, titel) in SONGS {
        let query = format!("{artist} {titel}");

        let referenzdaten = referenz(artist, titel).await;

        let treffer = downloader::search_everywhere(&ytdlp, &query, 5)
            .await
            .unwrap_or_default();
        let Some(bester) = treffer.first() else {
            daneben.push(format!("{query}: gar kein Treffer"));
            println!("!  {query:<45} keine Treffer");
            continue;
        };

        geprueft += 1;

        // three touchstones: is the title in there, is it the original
        // version, does the length fit?
        let text = format!(
            "{} {}",
            bester.title,
            bester.uploader.clone().unwrap_or_default()
        );
        let titel_passt = referenzdaten
            .as_ref()
            .is_some_and(|(referenz_titel, _)| {
                online::looks_like_same(&bester.title, referenz_titel)
            })
            || online::looks_like_same(&bester.title, titel)
            || online::normalize_for_match(&text)
                .contains(&online::normalize_for_match(titel));
        // a foreign rework is a misgrasp even where the title appears in it,
        // which the first report still let pass
        let echte_fassung = !ist_fremde_fassung(&bester.title, titel);
        // without a reference the median of the hits has to serve as the yardstick
        let massstab = referenzdaten
            .as_ref()
            .map(|(_, dauer)| ("Referenz", *dauer))
            .or_else(|| median_dauer(&treffer).map(|ms| ("Mittel", ms)));

        let laenge_passt = match (massstab, bester.duration_ms) {
            (Some((_, soll)), Some(ms)) => (ms - soll).abs() < 20_000,
            _ => true,
        };

        let vergleich = match massstab {
            Some((woher, soll)) => {
                if referenzdaten.is_none() {
                    ohne_referenz.push(query.clone());
                }
                format!(
                    "({} s statt {} s laut {woher})",
                    bester.duration_ms.unwrap_or(0) / 1000,
                    soll / 1000
                )
            }
            None => {
                ohne_referenz.push(query.clone());
                format!("({} s, kein Maßstab)", bester.duration_ms.unwrap_or(0) / 1000)
            }
        };

        let zeichen = if titel_passt && echte_fassung && laenge_passt {
            "ok"
        } else {
            "XX"
        };
        println!(
            "{zeichen} {query:<45} → [{:>10}] {}  {vergleich}",
            bester.source, bester.title,
        );

        if !titel_passt || !echte_fassung || !laenge_passt {
            daneben.push(format!(
                "{query} → [{}] {} {vergleich}",
                bester.source, bester.title
            ));
            // for tracing: what else stood to choose from?
            for kandidat in treffer.iter().take(5) {
                println!(
                    "      [{:>10}] {}  ({} s)",
                    kandidat.source,
                    kandidat.title,
                    kandidat.duration_ms.unwrap_or(0) / 1000
                );
            }
        }
    }

    println!("\n──────── Ergebnis ────────");
    println!("geprüft:       {geprueft}");
    println!("Fehlgriffe:    {}", daneben.len());
    println!("ohne Referenz: {}", ohne_referenz.len());
    for eintrag in &daneben {
        println!("  XX {eintrag}");
    }
}
