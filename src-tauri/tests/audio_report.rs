//! Prüft die Trefferauswahl **auf Höhe der Tonspur**: Stimmt die gewählte
//! Aufnahme wirklich mit dem überein, was andere Quellen unter demselben
//! Titel führen?
//!
//!     cargo test --test audio_report -- --ignored --nocapture
//!
//! Titel und Laufzeit können täuschen, zwei Uploads mit gleichem Namen und
//! gleicher Länge sind trotzdem oft verschiedene Aufnahmen. Deshalb wird von
//! jedem Kandidaten ein Ausschnitt geladen und ein akustischer Fingerabdruck
//! gebildet (Chromaprint, in ffmpeg enthalten).
//!
//! Gemessene Werte für die Schwelle:
//!
//! * dieselbe Aufnahme über YouTube und SoundCloud → 0.993
//! * zwei verschiedene Songs                       → 0.539
//!
//! Der Abstand ist so groß, dass 0.80 sicher trennt.

use robify_lib::downloader::{self, SearchResult};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Ab dieser Ähnlichkeit gilt die Tonspur als dieselbe Aufnahme.
const SAME_RECORDING: f64 = 0.80;

/// Ausschnitt, der verglichen wird: ab Sekunde 20, 45 Sekunden lang. Weit
/// genug im Titel, um Vorspann und Stille am Anfang zu überspringen.
const START_SECONDS: &str = "20";
const LENGTH_SECONDS: &str = "45";

const SONGS: [(&str, &str); 30] = [
    // Rap international
    ("Yeat", "Naked"),
    ("Playboi Carti", "Sky"),
    ("Kendrick Lamar", "Money Trees"),
    ("Travis Scott", "SICKO MODE"),
    ("Drake", "Passionfruit"),
    ("A$AP Rocky", "Praise The Lord"),
    // Rap deutsch
    ("PA69", "Tropical Island"),
    ("Sido", "Astronaut"),
    ("Haftbefehl", "Chabos wissen wer der Babo ist"),
    ("Nina Chuba", "Wildberry Lillet"),
    // Pop
    ("Billie Eilish", "Birds of a Feather"),
    ("The Weeknd", "Blinding Lights"),
    ("Dua Lipa", "Levitating"),
    ("Harry Styles", "As It Was"),
    ("Sabrina Carpenter", "Espresso"),
    // Rock und Indie
    ("Radiohead", "Creep"),
    ("Tame Impala", "The Less I Know The Better"),
    ("Arctic Monkeys", "505"),
    ("Nirvana", "Come As You Are"),
    ("Fleetwood Mac", "Dreams"),
    // Elektronisch
    ("Fred again..", "Delilah"),
    ("Daft Punk", "Instant Crush"),
    ("Aphex Twin", "Xtal"),
    ("Boards of Canada", "Roygbiv"),
    ("Bicep", "Glue"),
    // Deutschsprachig, Liedermacher, Klassik, Jazz
    ("AnnenMayKantereit", "Oft gefragt"),
    ("Kraftwerk", "Das Model"),
    ("Nina Simone", "Feeling Good"),
    ("Miles Davis", "So What"),
    ("Ludovico Einaudi", "Nuvole Bianche"),
];

// ------------------------------------------------------------ Fingerabdruck

/// Liest den Chromaprint-Fingerabdruck einer Datei als Folge von 32-Bit-Werten.
fn fingerprint(path: &Path) -> Option<Vec<u32>> {
    let output = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-ss",
            START_SECONDS,
            "-t",
            LENGTH_SECONDS,
            "-i",
        ])
        .arg(path)
        .args([
            "-ac", "1", "-ar", "11025", "-f", "chromaprint", "-fp_format", "raw", "-",
        ])
        .output()
        .ok()?;

    if !output.status.success() || output.stdout.len() < 160 {
        return None;
    }
    Some(
        output
            .stdout
            .chunks_exact(4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect(),
    )
}

/// Ähnlichkeit zweier Fingerabdrücke, 0.0 bis 1.0.
///
/// Die Ausschnitte können um einige Bilder versetzt sein, etwa wenn ein
/// Upload eine Sekunde Stille vorweg hat. Deshalb wird die beste
/// Überlagerung gesucht statt stur Position für Position zu vergleichen.
fn similarity(a: &[u32], b: &[u32]) -> f64 {
    let mut best: f64 = 0.0;
    for offset in -25i32..=25 {
        let (left, right): (&[u32], &[u32]) = if offset >= 0 {
            (&a[(offset as usize).min(a.len())..], b)
        } else {
            (a, &b[((-offset) as usize).min(b.len())..])
        };
        let overlap = left.len().min(right.len());
        if overlap < 40 {
            continue;
        }
        let differing: u32 = (0..overlap).map(|i| (left[i] ^ right[i]).count_ones()).sum();
        let score = 1.0 - f64::from(differing) / (overlap * 32) as f64;
        best = best.max(score);
    }
    best
}

/// Lädt die Tonspur. Bei Misserfolg kommt die Meldung der Quelle zurück,
/// DRM, Sperre oder nur eine Vorschau sehen sonst alle gleich aus.
///
/// Bewusst ganz statt nur ein Stück: `--download-sections` scheiterte bei
/// mehreren Quellen mit „ffmpeg exited with code 8“ und hätte die Messung
/// unbrauchbar gemacht. Geschnitten wird erst beim Fingerabdruck.
fn fetch_excerpt(ytdlp: &Path, url: &str, target: &Path) -> Result<PathBuf, String> {
    let pattern = target.with_extension("%(ext)s");
    let mut cmd = Command::new(ytdlp);
    cmd.args(["--no-warnings", "--no-playlist"]);
    if let Some(runtime) = downloader::js_runtime() {
        cmd.arg("--js-runtimes").arg(runtime);
    }
    cmd.args([
        "-x",
        "-f",
        "bestaudio[format_id!*=preview]/best[format_id!*=preview]",
        "-o",
    ])
    .arg(&pattern)
    .arg(url);

    let output = cmd.output().map_err(|e| e.to_string())?;
    let stem = target
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("kein Dateiname")?;
    let datei = target
        .parent()
        .and_then(|dir| std::fs::read_dir(dir).ok())
        .and_then(|eintraege| {
            eintraege
                .filter_map(Result::ok)
                .map(|e| e.path())
                .find(|p| p.file_stem().and_then(|s| s.to_str()) == Some(stem))
        });

    // Auf die Datei kommt es an, nicht auf den Rückgabewert: yt-dlp meldet
    // auch dann einen Fehler, wenn nur die Nachbearbeitung stolperte.
    match datei {
        Some(pfad) => Ok(pfad),
        None => {
            let meldung = String::from_utf8_lossy(&output.stderr);
            Err(meldung
                .lines()
                .find(|l| l.starts_with("ERROR:"))
                .unwrap_or_else(|| meldung.lines().last().unwrap_or("ohne Meldung"))
                .chars()
                .take(110)
                .collect())
        }
    }
}

/// Lädt, was die App liefern würde: erste Wahl, sonst die Ausweichadressen.
///
/// Ohne diesen Schritt misst der Bericht nur die erste Wahl, und ein
/// DRM-geschützter Treffer sähe aus wie ein Totalausfall, obwohl Robify
/// längst zur nächsten Quelle gewechselt wäre.
fn fetch_wie_die_app<'a>(
    ytdlp: &Path,
    treffer: &'a [SearchResult],
    ziel: &Path,
) -> Result<(PathBuf, &'a SearchResult), String> {
    let mut letzter_fehler = String::from("keine Quelle");
    for kandidat in treffer.iter().take(4) {
        match fetch_excerpt(ytdlp, &kandidat.url, ziel) {
            Ok(pfad) => return Ok((pfad, kandidat)),
            Err(fehler) => letzter_fehler = fehler,
        }
    }
    Err(letzter_fehler)
}

#[tokio::test]
#[ignore = "benötigt Internet, yt-dlp und ffmpeg; dauert mehrere Minuten"]
async fn tonspuren_stimmen_ueberein() {
    let ytdlp = downloader::find_ytdlp(None, std::path::Path::new(".")).expect("yt-dlp gefunden");
    let work = std::env::temp_dir().join("robify-audio-report");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).expect("Arbeitsordner");

    let mut gleich = 0;
    let mut verschieden = Vec::new();
    let mut unpruefbar = Vec::new();

    for (index, (artist, titel)) in SONGS.iter().enumerate() {
        let query = format!("{artist} {titel}");

        let treffer = downloader::search_everywhere(&ytdlp, &query, 4)
            .await
            .unwrap_or_default();
        if treffer.is_empty() {
            unpruefbar.push(format!("{query}: keine Treffer"));
            println!("?? {query:<48} keine Treffer");
            continue;
        }

        // Das, was am Ende in der Bibliothek landen würde.
        let (a, gewaehlt) = match fetch_wie_die_app(&ytdlp, &treffer, &work.join(format!("a{index}")))
        {
            Ok(werte) => werte,
            Err(grund) => {
                unpruefbar.push(format!("{query}: {grund}"));
                println!("?? {query:<48} {grund}");
                continue;
            }
        };

        // Gegenprobe von einer anderen Quelle, ebenfalls mit Ausweichen.
        let andere: Vec<SearchResult> = treffer
            .iter()
            .filter(|k| k.source != gewaehlt.source && k.url != gewaehlt.url)
            .cloned()
            .collect();
        let (b, vergleich) = match fetch_wie_die_app(&ytdlp, &andere, &work.join(format!("b{index}")))
        {
            Ok(werte) => werte,
            Err(grund) => {
                unpruefbar.push(format!("{query}: Gegenprobe, {grund}"));
                println!("?? {query:<48} Gegenprobe, {grund}");
                continue;
            }
        };

        let (Some(fp_a), Some(fp_b)) = (fingerprint(&a), fingerprint(&b)) else {
            unpruefbar.push(format!("{query}: kein Fingerabdruck"));
            println!("?? {query:<48} kein Fingerabdruck");
            continue;
        };

        let score = similarity(&fp_a, &fp_b);
        let passt = score >= SAME_RECORDING;
        println!(
            "{} {query:<48} {score:.3}  [{}] {} ↔ [{}] {}",
            if passt { "ok" } else { "XX" },
            gewaehlt.source,
            gewaehlt.title.chars().take(34).collect::<String>(),
            vergleich.source,
            vergleich.title.chars().take(34).collect::<String>(),
        );

        if passt {
            gleich += 1;
        } else {
            verschieden.push(format!(
                "{query} ({score:.3}), [{}] {} statt [{}] {}",
                gewaehlt.source, gewaehlt.title, vergleich.source, vergleich.title
            ));
        }
    }

    println!("\n──────── Ergebnis ────────");
    println!("Tonspur bestätigt: {gleich}");
    println!("abweichend:        {}", verschieden.len());
    println!("nicht prüfbar:     {}", unpruefbar.len());
    for eintrag in &verschieden {
        println!("  XX {eintrag}");
    }
    for eintrag in &unpruefbar {
        println!("  ?? {eintrag}");
    }

    let _ = std::fs::remove_dir_all(&work);
}
