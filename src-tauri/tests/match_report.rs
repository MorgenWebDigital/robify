//! Messlauf für die Trefferauswahl: Für eine Liste bekannter Titel wird
//! geprüft, ob der Downloader tatsächlich die richtige Aufnahme wählt.
//!
//! Kein Erfolgstest, sondern ein Bericht, deshalb standardmäßig aus:
//!
//!     cargo test --test match_report -- --ignored --nocapture
//!
//! Als Vergleichsmaßstab dienen die Online-Metadaten (Genius, iTunes,
//! MusicBrainz). Deren Laufzeit sagt, ob die gewählte Aufnahme plausibel ist.

use robify_lib::{downloader, online};

/// Bunt gemischt: Rap, Pop, Rock, Elektronisches, deutsch- und
/// englischsprachig, bekannt und weniger bekannt.
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

/// Wortmarken, die eine fremde Bearbeitung verraten. Bewusst hier nochmal
/// aufgeführt: Der Bericht soll unabhängig davon prüfen, was der Downloader
/// selbst für einen Zusatz hält.
const FREMDE_FASSUNG: [&str; 12] = [
    "remix", "cover", "edit", "bootleg", "flip", "rework", "live", "instrumental", "karaoke",
    "slowed", "sped", "mashup",
];

/// Mittlere Laufzeit der Treffer, als Notmaßstab, wenn die Metadatenquellen
/// nichts hergeben. Bewusst simpel und anders gerechnet als im Downloader,
/// damit der Bericht nicht dieselbe Annahme bestätigt, die er prüfen soll.
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

/// Laufzeit und geschriebene Form des Titels aus den Metadatenquellen.
async fn referenz(artist: &str, titel: &str) -> Option<(String, i64)> {
    let treffer = online::search_metadata(&format!("{artist} {titel}")).await.ok()?;
    let mit_laufzeit: Vec<_> = treffer
        .into_iter()
        .filter(|c| c.duration_ms.unwrap_or(0) > 0 && !ist_fremde_fassung(&c.title, titel))
        .collect();

    // Erst der Treffer, bei dem Titel und Künstler passen …
    mit_laufzeit
        .iter()
        .find(|c| {
            online::looks_like_same(&c.title, titel) && online::looks_like_same(&c.artist, artist)
        })
        // … sonst genügt der Titel.
        .or_else(|| {
            mit_laufzeit
                .iter()
                .find(|c| online::looks_like_same(&c.title, titel))
        })
        .map(|c| (c.title.clone(), c.duration_ms.unwrap_or(0)))
}

/// Einzelfall-Werkzeug: zeigt alle Treffer einer Suche in der Reihenfolge,
/// in der Robify sie anbietet.
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

        // Drei Prüfsteine: Steht der Titel drin? Ist es die Originalfassung?
        // Passt die Länge?
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
        // Eine fremde Bearbeitung ist ein Fehlgriff, auch wenn der Titel
        // darin vorkommt, das hat der erste Bericht noch durchgehen lassen.
        let echte_fassung = !ist_fremde_fassung(&bester.title, titel);
        // Ohne Referenz muss der Mittelwert der Treffer als Maßstab reichen.
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
            // Zum Nachvollziehen: was stand sonst noch zur Wahl?
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
