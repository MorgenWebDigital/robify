//! Downloader auf Basis von `yt-dlp`. Die Quelle bestimmt der Nutzer:
//! entweder eine direkte URL (yt-dlp unterstützt sehr viele Portale) oder
//! eine Suche in einem der Suchanbieter.
//!
//! Heruntergeladen wird in ein Arbeitsverzeichnis; erst wenn der Nutzer die
//! Metadaten bestätigt hat, wandert die Datei in die Bibliothek.

use crate::models::TrackMetadata;
use crate::tags;
use anyhow::{anyhow, bail, Result};
use base64::Engine;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
// Nur der Weg über einen eigenen Prozess braucht sie; auf Android laufen
// yt-dlp und ffmpeg über die Java-Brücke.
#[cfg(not(target_os = "android"))]
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Runtime};
#[cfg(not(target_os = "android"))]
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use crate::fehler;

const PROGRESS_MARKER: &str = "ROBIFYPROGRESS";

/// Registry laufender Jobs, damit sich Downloads abbrechen lassen.
#[derive(Default)]
pub struct DownloadRegistry {
    cancels: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl DownloadRegistry {
    /// Meldet einen Auftrag an. Läuft die Kennung bereits, wird deren Flag
    /// weiterverwendet, sonst verlöre ein Abbruch stillschweigend seine
    /// Wirkung, weil er auf das ersetzte Flag zeigt.
    fn register(&self, job_id: &str) -> Arc<AtomicBool> {
        let mut cancels = self.cancels.lock();
        cancels
            .entry(job_id.to_string())
            .or_insert_with(|| Arc::new(AtomicBool::new(false)))
            .clone()
    }

    fn finish(&self, job_id: &str) {
        self.cancels.lock().remove(job_id);
    }

    pub fn cancel(&self, job_id: &str) -> bool {
        match self.cancels.lock().get(job_id) {
            Some(flag) => {
                flag.store(true, Ordering::SeqCst);
                true
            }
            None => false,
        }
    }

    pub fn active(&self) -> Vec<String> {
        self.cancels.lock().keys().cloned().collect()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SearchSource {
    Youtube,
    /// Der Musikdienst hinter YouTube. Seine Suche liefert die offizielle
    /// Veröffentlichung statt Lyric-Videos und Fanuploads.
    YoutubeMusic,
    Soundcloud,
    /// Künstler laden dort selbst hoch, meist vollständig und gut.
    Bandcamp,
    /// Offene Musikplattform, liefert MP3 mit bis zu 320 kbit/s.
    Audius,
    /// Die Eingabe ist bereits eine URL und wird direkt an yt-dlp gegeben.
    Url,
}

/// Quellen, die bei einer Textsuche gleichzeitig befragt werden.
const SEARCHED_SOURCES: [SearchSource; 5] = [
    SearchSource::YoutubeMusic,
    SearchSource::Bandcamp,
    SearchSource::Audius,
    SearchSource::Soundcloud,
    SearchSource::Youtube,
];

impl SearchSource {
    fn query_for(self, input: &str, limit: usize) -> String {
        match self {
            SearchSource::Youtube => format!("ytsearch{limit}:{input}"),
            SearchSource::Soundcloud => format!("scsearch{limit}:{input}"),
            SearchSource::YoutubeMusic => format!(
                "https://music.youtube.com/search?q={}",
                urlencoding::encode(input)
            ),
            // Werden über ihre eigenen Schnittstellen gesucht.
            SearchSource::Bandcamp | SearchSource::Audius | SearchSource::Url => input.to_string(),
        }
    }

    /// YouTube Music nennt Laufzeiten nur bei vollständiger Abfrage. Die
    /// dauert länger, ist die Angabe aber wert: Ohne sie ließe sich weder
    /// ein Ausschnitt erkennen noch die Mehrheitslänge bilden.
    fn needs_full_extraction(self) -> bool {
        self == SearchSource::YoutubeMusic
    }

    pub fn label(self) -> &'static str {
        match self {
            SearchSource::Youtube => "YouTube",
            SearchSource::YoutubeMusic => "YouTube Music",
            SearchSource::Soundcloud => "SoundCloud",
            SearchSource::Bandcamp => "Bandcamp",
            SearchSource::Audius => "Audius",
            SearchSource::Url => "Link",
        }
    }

    /// Abschlag auf die Bewertung, je verlässlicher die Quelle den ganzen
    /// Titel in guter Qualität liefert, desto größer.
    ///
    /// Bewusst klein gehalten: Ein Messlauf über 15 Titel zeigte, dass
    /// Bandcamp bei bekannten Songs überwiegend fremde Bearbeitungen
    /// ausliefert. Die Quelle darf den Ausschlag geben, wenn sonst alles
    /// gleich ist, aber nie gegen den passenderen Titel gewinnen.
    ///
    /// YouTube Music steht vorn, weil dort die Veröffentlichung des
    /// Künstlers liegt und nicht die Nachbearbeitung eines Dritten.
    fn quality_bonus(self) -> f64 {
        match self {
            SearchSource::YoutubeMusic => 4.0,
            SearchSource::Bandcamp => 3.0,
            SearchSource::Audius => 2.5,
            SearchSource::Soundcloud => 2.0,
            SearchSource::Youtube | SearchSource::Url => 0.0,
        }
    }

    fn from_label(label: &str) -> Option<SearchSource> {
        [
            SearchSource::Youtube,
            SearchSource::YoutubeMusic,
            SearchSource::Soundcloud,
            SearchSource::Bandcamp,
            SearchSource::Audius,
        ]
        .into_iter()
        .find(|source| source.label() == label)
    }
}

/// Bandcamp bietet eine offene Suche ohne Zugangsdaten. Laufzeiten liefert
/// sie allerdings nicht mit.
async fn search_bandcamp(query: &str, limit: usize) -> Result<Vec<SearchResult>> {
    let body = serde_json::json!({
        "search_text": query,
        "search_filter": "t",
        "full_page": false,
        "fan_id": null,
    });

    let response: serde_json::Value = crate::online::client()
        .post("https://bandcamp.com/api/bcsearch_public_api/1/autocomplete_elastic")
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let results = response["auto"]["results"].as_array().cloned().unwrap_or_default();
    Ok(results
        .iter()
        // "t" steht für einen einzelnen Titel.
        .filter(|item| item["type"].as_str() == Some("t"))
        .filter_map(|item| {
            let url = item["item_url_path"].as_str()?.to_string();
            Some(SearchResult {
                id: item["id"].to_string(),
                title: item["name"].as_str()?.to_string(),
                uploader: item["band_name"].as_str().map(str::to_string),
                duration_ms: None,
                url,
                thumbnail: item["img"].as_str().map(str::to_string),
                source: SearchSource::Bandcamp.label().to_string(),
            })
        })
        .take(limit)
        .collect())
}

/// Audius ist offen zugänglich und nennt die Laufzeit, dadurch lässt sich
/// die passende Aufnahme besonders sicher bestimmen.
async fn search_audius(query: &str, limit: usize) -> Result<Vec<SearchResult>> {
    let url = format!(
        "https://api.audius.co/v1/tracks/search?query={}&app_name=Robify",
        urlencoding::encode(query)
    );
    let response: serde_json::Value = crate::online::client()
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    Ok(response["data"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|track| {
            let id = track["id"].as_str()?;
            Some(SearchResult {
                id: id.to_string(),
                title: track["title"].as_str()?.to_string(),
                uploader: track["user"]["name"].as_str().map(str::to_string),
                duration_ms: sane_duration(track["duration"].as_i64().map(|s| s.saturating_mul(1000))),
                url: format!(
                    "https://api.audius.co/v1/tracks/{id}/stream?app_name=Robify"
                ),
                thumbnail: track["artwork"]["480x480"].as_str().map(str::to_string),
                source: SearchSource::Audius.label().to_string(),
            })
        })
        .take(limit)
        .collect())
}

/// Ein Eintrag, der heruntergeladen werden kann, entweder direkt über eine
/// URL oder über eine Suche, wenn die Quelle selbst keine Audiodaten liefert.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadPlan {
    /// Wird unverändert an yt-dlp gegeben; bei Spotify ein Suchausdruck.
    pub url: String,
    /// Ausweichadressen, falls die erste Quelle nichts liefert.
    #[serde(default)]
    pub fallbacks: Vec<String>,
    /// Suchbegriff, aus dem der beste Treffer bestimmt wird.
    #[serde(default)]
    pub match_query: Option<String>,
    /// Wonach der Nutzer gesucht hat. Dient nur der Gegenprobe nach dem
    /// Laden, anders als `match_query` löst es keine neue Suche aus.
    #[serde(default)]
    pub intent: Option<String>,
    pub title: String,
    pub subtitle: Option<String>,
    pub thumbnail: Option<String>,
    pub duration_ms: Option<i64>,
    pub source: String,
    /// Vorbekannte Metadaten, die den Tags der geladenen Datei vorgehen.
    pub metadata: Option<TrackMetadata>,
    /// Liegt schon ein Titel gleichen Namens desselben Künstlers in der
    /// Bibliothek? Dann lässt sich der Eintrag beim Stapel überspringen.
    #[serde(default)]
    pub already_in_library: bool,
}

/// Ein Hinweis, den die Oberfläche selbst in Worte fasst.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanHinweis {
    /// Welcher Hinweis; die Oberfläche hält den Wortlaut in allen Sprachen.
    pub code: String,
    /// Einsetzwerte in der Reihenfolge der Platzhalter `{0}`, `{1}`, …
    pub args: Vec<String>,
}

/// Was hinter einem eingefügten Link steckt.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkPlan {
    pub label: String,
    pub kind: String,
    /// Hinweise für die Oberfläche, als Kennung statt als fertiger Satz.
    ///
    /// Der Rust-Teil kennt die eingestellte Oberflächensprache nicht, sie
    /// steht im Frontend. Ein hier zusammengesetzter Satz käme darum in jeder
    /// Sprache auf Deutsch an, und genau so stand der Spotify-Hinweis auch in
    /// der russischen Fassung.
    pub notes: Vec<PlanHinweis>,
    /// Gehören die Einträge zusammen (Album, Playlist)? Dann ergibt
    /// „Alle laden“ Sinn, bei Suchtreffern nicht.
    pub batch: bool,
    pub items: Vec<DownloadPlan>,
}

/// Macht aus Suchtreffern Download-Aufträge und hängt jedem die übrigen
/// Treffer als Ausweichadressen an.
///
/// Nötig, weil sich manche Hindernisse erst beim Laden zeigen: SoundCloud
/// gibt die Label-Uploads bekannter Künstler DRM-geschützt heraus, und das
/// steht in keinem Suchergebnis. Ein Messlauf über 30 Titel traf das bei
/// acht davon. Ohne Ausweichkette bliebe es bei „nicht möglich“, obwohl
/// dieselbe Aufnahme über eine andere Quelle bereitsteht.
/// Taugt ein Treffer als Ersatz für einen anderen?
///
/// Ein reiner Namensvergleich reicht nicht: Dieselbe Aufnahme heißt bei einer
/// Quelle „BIRDS OF A FEATHER“ und bei der nächsten „Billie Eilish - BIRDS OF
/// A FEATHER“. Deshalb genügt es, wenn der eine Titel im anderen als
/// Wortfolge steckt, solange keine andere Fassung angekündigt wird.
fn same_song(candidate: &SearchResult, wanted: &SearchResult) -> bool {
    let a = crate::online::normalize_for_match(&candidate.title);
    let b = crate::online::normalize_for_match(&wanted.title);
    if a.is_empty() || b.is_empty() {
        return false;
    }
    let gleicher_name = crate::online::contains_word_sequence(&a, &b)
        || crate::online::contains_word_sequence(&b, &a);

    // Gleicher Name heißt nicht gleicher Song: „Naked“ gibt es von Yeat (93 s)
    // und von Kraak & Smaak. Die Laufzeit trennt beide, fehlt sie, taugt der
    // Treffer nicht als Ersatz. Ungeprüft eingesetzt landete sonst der falsche
    // Song in der Bibliothek, und das fällt später kaum noch auf.
    let gleiche_laenge = match (candidate.duration_ms, wanted.duration_ms) {
        (Some(a), Some(b)) => a.saturating_sub(b).saturating_abs() <= SAME_TAKE_MS,
        _ => false,
    };

    gleicher_name && gleiche_laenge && version_penalty(&candidate.title, &wanted.title) == 0.0
}

pub fn plans_with_fallbacks(found: Vec<SearchResult>) -> Vec<DownloadPlan> {
    let alle = found.clone();
    found
        .into_iter()
        .map(|treffer| {
            // Nur Treffer, die denselben Titel meinen. Vorher stand hier die
            // ganze Trefferliste, bei „Yeat Naked“ wich der Download dann
            // bis auf „Back Home“ aus, einen völlig anderen Song.
            let mut ausweich: Vec<&SearchResult> = alle
                .iter()
                .filter(|andere| andere.url != treffer.url && same_song(andere, &treffer))
                .collect();
            // Erst eine andere Quelle, dann der Rest derselben.
            //
            // Sagt eine Quelle ab, sagt sie meist für alle ihre Treffer ab:
            // Bei YouTube endeten drei Ausweichadressen dreimal mit demselben
            // 403, während der SoundCloud-Treffer unversucht danebenlag. Die
            // Sortierung ist stabil, die Reihenfolge nach Passgenauigkeit
            // bleibt innerhalb jeder Gruppe erhalten.
            let eigene = source_label(&treffer.url);
            ausweich.sort_by_key(|andere| source_label(&andere.url) == eigene);
            let fallbacks = ausweich
                .into_iter()
                .map(|andere| andere.url.clone())
                .take(3)
                .collect();
            DownloadPlan {
                fallbacks,
                ..DownloadPlan::from(treffer)
            }
        })
        .collect()
}

impl From<SearchResult> for DownloadPlan {
    fn from(result: SearchResult) -> Self {
        DownloadPlan {
            url: result.url,
            fallbacks: Vec::new(),
            intent: None,
            match_query: None,
            title: result.title,
            subtitle: result.uploader,
            thumbnail: result.thumbnail,
            duration_ms: result.duration_ms,
            source: result.source,
            metadata: None,
            already_in_library: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    pub uploader: Option<String>,
    pub duration_ms: Option<i64>,
    pub url: String,
    pub thumbnail: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadOptions {
    pub url: String,
    /// "mp3", "opus", "flac", "m4a", "vorbis" oder "best" (keine Umwandlung).
    #[serde(default = "default_format")]
    pub format: String,
    /// 0 = beste Qualität … 9 (nur bei verlustbehafteten Formaten relevant).
    #[serde(default)]
    pub quality: Option<String>,
    #[serde(default = "default_true")]
    pub embed_thumbnail: bool,
    /// Bereits bekannte Metadaten (z. B. aus einem Spotify-Link). Sie haben
    /// Vorrang vor dem, was in der heruntergeladenen Datei steht.
    #[serde(default)]
    pub metadata: Option<TrackMetadata>,
    /// Wird `url` nicht fündig, werden diese Adressen der Reihe nach probiert.
    #[serde(default)]
    pub fallbacks: Vec<String>,
    /// Nach dem Laden online nach passenden Metadaten suchen.
    #[serde(default = "default_true")]
    pub auto_match: bool,
    #[serde(default = "default_true")]
    pub auto_cover: bool,
    #[serde(default = "default_true")]
    pub auto_lyrics: bool,
    /// Bekannte Länge des Titels. Ist die geladene Datei deutlich kürzer,
    /// war es ein Ausschnitt, dann wird die nächste Quelle versucht.
    #[serde(default)]
    pub expected_duration_ms: Option<i64>,
    /// Statt einer festen Adresse: passenden Treffer selbst heraussuchen.
    /// Wird für Spotify-Links genutzt, wo nur Metadaten vorliegen.
    #[serde(default)]
    pub match_query: Option<String>,
    /// Wonach der Nutzer gesucht hat. Nur für die Gegenprobe nach dem Laden,
    /// anders als `match_query` löst es keine neue Suche aus.
    #[serde(default)]
    pub intent: Option<String>,
}

fn default_format() -> String {
    "mp3".into()
}
fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub job_id: String,
    pub status: String,
    pub percent: f64,
    pub downloaded_bytes: Option<i64>,
    pub total_bytes: Option<i64>,
    pub speed_bytes: Option<f64>,
    pub eta_seconds: Option<i64>,
    pub message: Option<String>,
}

/// Ergebnis eines Downloads: die Datei liegt im Arbeitsverzeichnis und die
/// Metadaten sind ein Vorschlag, den der Nutzer noch ändern kann.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadOutcome {
    pub job_id: String,
    pub path: String,
    pub duration_ms: i64,
    pub format: String,
    pub metadata: TrackMetadata,
    pub source_url: String,
    /// Gesetzt, wenn das Ergebnis nicht zur Sucheingabe passt.
    #[serde(default)]
    pub warning: Option<String>,
}

/// Passt das Geladene zu dem, wonach gesucht wurde?
///
/// Manche Titel gibt es auf keiner erreichbaren Quelle in sauberer Fassung.
/// Dann lädt Robify, was am nächsten kommt, bei „The Killers. Mr. Brightside“
/// war das ein Upload von „Julia“. Die Metadaten beschreiben ihn korrekt, nur
/// eben den falschen Song. Erfinden lässt sich der richtige nicht; verschweigen
/// sollte man den Fehlgriff aber auch nicht.
///
/// Geprüft wird, ob jedes bedeutsame Wort der Eingabe irgendwo in Titel,
/// Künstler oder Gästen vorkommt.
/// `unbestaetigt` heißt: Die Metadatensuche hat nichts gefunden. Dann fehlt
/// die Bestätigung von außen, und schwächere Anzeichen wiegen schwerer.
fn intent_warning(
    intent: Option<&str>,
    metadata: &TrackMetadata,
    unbestaetigt: bool,
) -> Option<String> {
    let intent = intent.map(str::trim).filter(|text| !text.is_empty())?;

    let gesucht = crate::online::normalize_words(intent);
    let woerter: Vec<&str> = gesucht.split(' ').filter(|w| !w.is_empty()).collect();
    if woerter.is_empty() {
        return None;
    }

    let vorhanden = crate::online::normalize_words(&format!(
        "{} {} {}",
        metadata.title,
        metadata.artist,
        metadata.featured_artists.clone().unwrap_or_default()
    ));
    let fehlend: Vec<&str> = woerter
        .iter()
        .filter(|wort| !crate::online::contains_word_sequence(&vorhanden, wort))
        .copied()
        .collect();

    // Ein einzelnes fehlendes Wort ist Alltag. Schreibweisen weichen ab,
    // Zusätze fallen weg. Erst wenn die Hälfte fehlt, stimmt etwas nicht.
    if fehlend.len() * 2 < woerter.len() {
        // Zweite Prüfung: Steht der Künstler überhaupt in der Suche?
        //
        // Nötig, weil der Künstlername oft auch im Titel steht. Bei
        // „The Killers- Mr. Brightside“ von „Julia“ waren alle gesuchten
        // Wörter vorhanden, im Titel. Das Künstlerfeld war trotzdem falsch.
        //
        // Wer nur einen Songtitel sucht, kennt den Künstler womöglich nicht;
        // deshalb zählt das allein noch nicht. Kam aber auch online nichts
        // an, deutet alles auf einen Fehlgriff.
        let kuenstler = crate::online::normalize_words(&metadata.artist);
        let kuenstler_gesucht = kuenstler
            .split(' ')
            .filter(|wort| !wort.is_empty())
            .any(|wort| crate::online::contains_word_sequence(&gesucht, wort));

        if kuenstler_gesucht || !unbestaetigt {
            return None;
        }
    }

    Some(format!(
        "Das Geladene passt nicht zu „{intent}“: „{} · {}“. \
         Prüfe die Angaben oder wähle einen anderen Treffer.",
        metadata.artist, metadata.title
    ))
}

/// Findet yt-dlp: erst der in den Einstellungen hinterlegte Pfad, sonst PATH.
/// Dateiname des eigenständigen yt-dlp für dieses Betriebssystem.
///
/// yt-dlp veröffentlicht je Plattform ein Programm ohne Abhängigkeiten; die
/// Namen sind bei jeder Veröffentlichung gleich.
fn ytdlp_asset() -> &'static str {
    if cfg!(target_os = "windows") {
        "yt-dlp.exe"
    } else if cfg!(target_os = "macos") {
        "yt-dlp_macos"
    } else {
        "yt-dlp_linux"
    }
}

/// Wo Robify sein eigenes yt-dlp ablegt.
pub fn managed_ytdlp(tools_dir: &Path) -> PathBuf {
    tools_dir.join(if cfg!(target_os = "windows") {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    })
}

/// Sucht yt-dlp, ohne etwas zu laden: erst der eingestellte Pfad, dann die
/// eigene Ablage, dann das System.
pub fn find_ytdlp(configured: Option<&str>, tools_dir: &Path) -> Option<PathBuf> {
    if let Some(path) = configured.filter(|p| !p.trim().is_empty()) {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }
    let eigenes = managed_ytdlp(tools_dir);
    if eigenes.exists() {
        return Some(eigenes);
    }
    which::which("yt-dlp")
        .or_else(|_| which::which("yt-dlp.exe"))
        .ok()
}

/// Liefert yt-dlp und holt es beim ersten Mal selbst, wenn nichts da ist.
///
/// Bewusst erst bei Bedarf statt beim Start: Wer die App nur zum Abspielen
/// benutzt, soll keine 30 MB laden. Heruntergeladen wird die eigenständige
/// Fassung von GitHub, danach einmal `--version` zur Probe, ein halb
/// geladenes Programm wäre schlimmer als gar keines und würde später mit
/// unverständlichen Fehlern auffallen.
pub async fn ensure_ytdlp(configured: Option<&str>, tools_dir: &Path) -> Result<PathBuf> {
    // Auf Android gibt es nichts zu holen: yt-dlp liegt als Bibliothek bei,
    // samt Python-Laufzeit, und wird beim Start eingerichtet. Ohne diese
    // Ausnahme lud die App die Linux-Binärdatei herunter und scheiterte
    // danach an der Probe, weil Android eine andere C-Bibliothek verwendet.
    //
    // Der Pfad ist ein Platzhalter: Die Brücke in `crate::ytdlp` braucht ihn
    // nicht, sie ruft in die Java-Laufzeit statt ein Programm zu starten.
    if cfg!(target_os = "android") {
        return Ok(PathBuf::from("eingebaut"));
    }

    if let Some(path) = find_ytdlp(configured, tools_dir) {
        return Ok(path);
    }

    let ziel = managed_ytdlp(tools_dir);
    let url = format!(
        "https://github.com/yt-dlp/yt-dlp/releases/latest/download/{}",
        ytdlp_asset()
    );

    let daten = crate::online::client()
        .get(&url)
        // Das Programm ist rund 30 MB groß, das übliche Zeitlimit von 20
        // Sekunden reicht dafür auf langsamen Leitungen nicht.
        .timeout(Duration::from_secs(300))
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| anyhow!(fehler!("yt-dlp konnte nicht geladen werden: {0}", e)))?
        .bytes()
        .await
        .map_err(|e| anyhow!(fehler!("yt-dlp konnte nicht geladen werden: {0}", e)))?;

    std::fs::create_dir_all(tools_dir)?;
    // Erst daneben schreiben, dann umbenennen: Bricht das Laden ab, bleibt
    // kein halbes Programm unter dem richtigen Namen liegen.
    let vorlaeufig = ziel.with_extension("teil");
    std::fs::write(&vorlaeufig, &daten)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&vorlaeufig, std::fs::Permissions::from_mode(0o755))?;
    }
    std::fs::rename(&vorlaeufig, &ziel)?;

    let probe = tokio::process::Command::new(&ziel)
        .arg("--version")
        .output()
        .await;
    match probe {
        Ok(out) if out.status.success() => Ok(ziel),
        _ => {
            let _ = std::fs::remove_file(&ziel);
            Err(anyhow!(
                "yt-dlp wurde geladen, ließ sich aber nicht ausführen. \
                 Bitte von Hand installieren (https://github.com/yt-dlp/yt-dlp)."
            ))
        }
    }
}

pub fn ffmpeg_available() -> bool {
    // Auf Android liegt ffmpeg als Bibliothek bei und wird beim Start
    // eingerichtet; im Suchpfad steht es dort nie. Ohne diese Ausnahme
    // meldete die Oberfläche „ffmpeg fehlt“, obwohl es zur Verfügung steht.
    if cfg!(target_os = "android") {
        return true;
    }
    which::which("ffmpeg").is_ok()
}

/// YouTube stellt beim Abruf eine JavaScript-Aufgabe. Ohne Laufzeitumgebung
/// weicht yt-dlp auf einen veralteten Weg aus, dessen Adressen häufig mit
/// „403 Forbidden“ abgelehnt werden. Deno ist die Vorgabe von yt-dlp; Node
/// bringt dieses Projekt ohnehin mit.
pub fn js_runtime() -> Option<&'static str> {
    // Android bringt keine dieser Laufzeiten mit, und installieren lässt sich
    // dort auch keine. Die Warnung bliebe also für immer stehen, ohne dass
    // jemand etwas tun könnte; yt-dlp weicht dann auf seinen älteren Weg aus.
    if cfg!(target_os = "android") {
        return None;
    }
    ["deno", "node", "bun", "qjs"]
        .into_iter()
        .find(|runtime| which::which(runtime).is_ok())
}

/// Ältere yt-dlp-Fassungen kennen `--js-runtimes` noch nicht. Einmal prüfen
/// und merken. `--help` antwortet in Sekundenbruchteilen.
async fn supports_js_runtimes(ytdlp: &Path) -> bool {
    static SUPPORTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if let Some(known) = SUPPORTED.get() {
        return *known;
    }

    let supported = match crate::ytdlp::einmal(ytdlp, &["--help".into()]).await {
        Ok(ausgabe) => ausgabe.stdout.contains("--js-runtimes"),
        Err(_) => false,
    };
    *SUPPORTED.get_or_init(|| supported)
}

/// Dateiendungen, die der eingebaute Player entschlüsseln kann.
///
/// **Opus fehlt bewusst.** rodio dekodiert über Symphonia, und Symphonia
/// bringt keinen Opus-Dekoder mit. Eine `.opus`-Datei landet zwar sauber in
/// der Bibliothek, lässt sich dort aber nicht abspielen.
pub const PLAYABLE_EXTENSIONS: [&str; 9] = [
    "mp3", "m4a", "mp4", "aac", "flac", "ogg", "oga", "wav", "aiff",
];

/// Ist die Datei mit Bordmitteln abspielbar?
pub fn is_playable(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .is_some_and(|ext| PLAYABLE_EXTENSIONS.contains(&ext.as_str()))
}

/// Welche Aufnahme yt-dlp laden soll.
///
/// Bei „Beste Qualität (Original)“ wird nicht umgewandelt, deshalb muss
/// schon die Auswahl auf ein abspielbares Format fallen. YouTube bietet
/// neben Opus fast immer M4A in derselben Bitrate an; ohne diese Vorgabe
/// gewinnt Opus und der Titel bleibt stumm.
///
/// `[format_id!*=preview]` hält SoundCloud-Vorschauen heraus, die nur
/// 30 Sekunden lang sind.
fn format_selector(original: bool) -> String {
    let mut wahl: Vec<&str> = Vec::new();
    if original {
        wahl.extend([
            "bestaudio[acodec^=mp4a]",
            "bestaudio[acodec^=mp3]",
            "bestaudio[acodec=flac]",
            "bestaudio[acodec=vorbis]",
            "bestaudio[ext=m4a]",
            "bestaudio[ext=mp3]",
        ]);
    }
    // Zuletzt zählt nur noch, dass überhaupt etwas kommt. Wird umgewandelt,
    // ist das Ausgangsformat ohnehin gleichgültig.
    wahl.extend(["bestaudio", "best"]);

    wahl.iter()
        .map(|filter| format!("{filter}[format_id!*=preview]"))
        .collect::<Vec<_>>()
        .join("/")
}

/// Kann yt-dlp Cover in die Audiodatei schreiben?
///
/// Für Opus und OGG braucht es dafür die Python-Bibliothek `mutagen`. Fehlt
/// sie, scheitert nicht nur das Einbetten, sondern der ganze Download, die
/// fertige Audiodatei wird mit der Nachbearbeitung verworfen.
///
/// yt-dlp nennt seine Zusatzbibliotheken selbst, wenn man es ausführlich
/// bittet. Das ist genauer als eine Suche nach `mutagen` im System: yt-dlp
/// bringt je nach Installationsart seine eigene Python-Umgebung mit.
async fn supports_thumbnail_embedding(ytdlp: &Path) -> bool {
    static SUPPORTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if let Some(known) = SUPPORTED.get() {
        return *known;
    }

    // Ohne Adresse bricht yt-dlp sofort ab, die Diagnosezeilen stehen aber
    // schon vorher auf der Fehlerausgabe. Kein Netzzugriff, rund 0,5 s.
    let supported = match crate::ytdlp::einmal(ytdlp, &["--verbose".into(), String::new()]).await {
        Ok(ausgabe) => ausgabe
            .stderr
            .lines()
            .find(|line| line.contains("Optional libraries"))
            .is_some_and(|line| line.contains("mutagen")),
        Err(_) => false,
    };
    *SUPPORTED.get_or_init(|| supported)
}

/// Die Laufzeitumgebung als Argumentliste, sofern vorhanden und unterstützt.
async fn js_runtime_args(ytdlp: &Path) -> Vec<String> {
    match js_runtime() {
        Some(runtime) if supports_js_runtimes(ytdlp).await => {
            vec!["--js-runtimes".into(), runtime.to_string()]
        }
        _ => Vec::new(),
    }
}

// ------------------------------------------------------------- Lastbremse
//
// yt-dlp-Aufrufe müssen sich gegenseitig aus dem Weg gehen. Eine einzelne
// Suche startet bis zu vier Prozesse gleichzeitig, ein Album mit dreißig
// Titeln über hundert nacheinander, ohne Pause. Genau dieses Muster
// beantwortet YouTube mit „403 Forbidden“, und die Sperre trifft dann auch
// alles Folgende.

/// So viele yt-dlp-Prozesse laufen höchstens gleichzeitig.
const MAX_PARALLEL_YTDLP: usize = 2;

// -------------------------------------------------------------- Zeitlimits
//
// Ohne Grenze blockiert ein hängender Prozess die Oberfläche dauerhaft:
// Suchen lassen sich gar nicht abbrechen, und die Ausgabeschleife eines
// Downloads dreht sich endlos weiter, solange yt-dlp nichts schreibt.

/// Nach dieser Zeit gilt eine Suche als gescheitert.
const SEARCH_TIMEOUT: Duration = Duration::from_secs(90);

/// Obergrenze für einen einzelnen Download.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// So lange darf ein laufender Download schweigen, bevor er als hängend gilt.
const IDLE_TIMEOUT: Duration = Duration::from_secs(120);

/// Obergrenze für die Umwandlung in ein abspielbares Format.
const CONVERT_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// Mindestabstand zwischen zwei Zugriffen auf dieselbe Quelle.
const MIN_SPACING: Duration = Duration::from_millis(700);

fn ytdlp_slots() -> &'static tokio::sync::Semaphore {
    static SLOTS: OnceLock<tokio::sync::Semaphore> = OnceLock::new();
    SLOTS.get_or_init(|| tokio::sync::Semaphore::new(MAX_PARALLEL_YTDLP))
}

/// Wann zuletzt auf eine Quelle zugegriffen wurde.
fn last_access() -> &'static Mutex<HashMap<&'static str, Instant>> {
    static LAST: OnceLock<Mutex<HashMap<&'static str, Instant>>> = OnceLock::new();
    LAST.get_or_init(Default::default)
}

/// Räumt liegengebliebene Auftragsordner weg.
///
/// Ein Download, der weder importiert noch abgebrochen wurde, hinterlässt
/// seinen kompletten Ordner samt Audiodatei. Über Wochen summiert sich das,
/// im Betrieb waren es 16 MB, ohne dass jemand etwas davon hatte.
///
/// Verschont wird [`crate::commands::KEEP_DIR`]: Dort liegen übernommene
/// Titel, auf die die Bibliothek zeigt.
pub fn cleanup_work_dir(work_dir: &Path, max_age: Duration) -> usize {
    let Ok(entries) = std::fs::read_dir(work_dir) else {
        return 0;
    };

    let mut entfernt = 0;
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if !path.is_dir() || !crate::commands::is_job_dir(&path) {
            continue;
        }
        let zu_alt = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .map(|zeitpunkt| zeitpunkt.elapsed().unwrap_or_default() > max_age)
            // Ohne lesbaren Zeitstempel lieber stehen lassen.
            .unwrap_or(false);

        if zu_alt && std::fs::remove_dir_all(&path).is_ok() {
            entfernt += 1;
        }
    }
    entfernt
}

// ----------------------------------------------------- Absagen von YouTube
//
// YouTube gibt Tondateien nur noch heraus, wenn yt-dlp die Abrufadressen auf
// dem neuen Weg bildet, und dafür braucht es eine JavaScript-Laufzeit. Fehlt
// sie, weicht yt-dlp auf einen Notweg aus, und jeder Download endet mit
// „HTTP Error 403“ — jeder, nicht nur gesperrte Titel. Auf einem Telefon
// lässt sich dagegen nichts installieren.
//
// Die anderen Quellen sind davon nicht betroffen. Statt weiter gegen eine
// verschlossene Tür zu laufen, merkt Robify sich die Absage und stellt für
// eine Weile SoundCloud, Bandcamp und Audius nach vorn. Der Vermerk verfällt
// von selbst: Die Lage bei YouTube ändert sich, und ohne Ablauf bliebe
// Robify für den Rest der Sitzung bei den Ausweichquellen.

/// So lange gilt eine Absage von YouTube als noch aktuell.
const ABSAGE_GILT: Duration = Duration::from_secs(30 * 60);

/// Abschlag für YouTube-Treffer, solange die Absage gilt.
///
/// Kleiner als ein fehlendes Suchwort (20). Der Abzug soll gleichwertige
/// Treffer umsortieren, nicht einen unpassenden Titel nach vorn holen: Lieber
/// ein Download, der scheitert, als der falsche Song in der Bibliothek.
const ABSAGE_ABZUG: f64 = 12.0;

fn absage_vermerk() -> &'static Mutex<Option<Instant>> {
    static VERMERK: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();
    VERMERK.get_or_init(|| Mutex::new(None))
}

/// Gehört die Adresse zu YouTube oder YouTube Music?
fn ist_youtube(url: &str) -> bool {
    let quelle = source_label(url);
    quelle == SearchSource::Youtube.label() || quelle == SearchSource::YoutubeMusic.label()
}

/// Hält fest, dass YouTube den Zugriff verweigert hat.
fn absage_merken() {
    *absage_vermerk().lock() = Some(Instant::now());
}

/// Sagt YouTube gerade ab?
fn youtube_sagt_ab() -> bool {
    matches!(*absage_vermerk().lock(), Some(zeit) if zeit.elapsed() < ABSAGE_GILT)
}

/// Hat die Quelle den Zugriff verweigert?
fn zugriff_verweigert(fehler: &anyhow::Error) -> bool {
    fehler.to_string().contains("(403)")
}

/// Der Abschlag für einen Treffer, solange die Absage gilt.
fn absage_abzug(url: &str) -> f64 {
    abzug_bei(url, youtube_sagt_ab())
}

/// Dasselbe ohne den Vermerk, damit es sich prüfen lässt.
///
/// Der Vermerk ist modulweit; ein Test, der ihn setzt, färbte auf alle
/// nebenher laufenden ab.
fn abzug_bei(url: &str, sagt_ab: bool) -> f64 {
    if sagt_ab && ist_youtube(url) {
        ABSAGE_ABZUG
    } else {
        0.0
    }
}

fn source_label(url: &str) -> &'static str {
    let lower = url.to_ascii_lowercase();
    if lower.contains("music.youtube") || lower.starts_with("ytmsearch") {
        SearchSource::YoutubeMusic.label()
    } else if lower.contains("youtube.") || lower.contains("youtu.be") || lower.starts_with("ytsearch")
    {
        SearchSource::Youtube.label()
    } else if lower.contains("soundcloud") || lower.starts_with("scsearch") {
        SearchSource::Soundcloud.label()
    } else if lower.contains("bandcamp") {
        SearchSource::Bandcamp.label()
    } else if lower.contains("audius") {
        SearchSource::Audius.label()
    } else {
        "sonstige"
    }
}

/// Ordnet eine Adresse ihrer Quelle zu, für den Mindestabstand.
///
/// Grob, aber ausreichend: Es geht nur darum, Zugriffe auf denselben Dienst
/// auseinanderzuziehen. Was sich nicht zuordnen lässt, teilt sich einen Topf.
/// Belegt einen Platz und wartet, bis die Quelle wieder an der Reihe ist.
///
/// Die Genehmigung wird zurückgegeben; solange sie lebt, ist der Platz belegt.
/// Absichtlich modulweit statt im `AppState`: Die Live-Tests rufen diese
/// Funktionen unmittelbar auf und leiden am stärksten unter der Sperre.
async fn acquire_slot(source: &'static str) -> tokio::sync::SemaphorePermit<'static> {
    let permit = ytdlp_slots()
        .acquire()
        .await
        .expect("Lastbremse wird nie geschlossen");

    // Erst nach dem Platz warten, sonst hielte die Wartezeit den Platz frei
    // und zwei Aufrufer stünden gleichzeitig vor derselben Quelle.
    let warten = {
        let mut karte = last_access().lock();
        let jetzt = Instant::now();
        let rest = karte
            .get(source)
            .map(|zuletzt| MIN_SPACING.saturating_sub(jetzt.duration_since(*zuletzt)))
            .unwrap_or_default();
        // Den Zeitpunkt gleich vormerken, damit Wartende sich einreihen.
        karte.insert(source, jetzt + rest);
        rest
    };
    if !warten.is_zero() {
        tokio::time::sleep(warten).await;
    }

    permit
}

/// Versteckt das Konsolenfenster unter Windows.
pub(crate) fn configure(cmd: &mut Command) {
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let _ = cmd;
}

/// YouTube beantwortet manche Suchbegriffe mit einer Seite, aus der yt-dlp
/// keine Einträge lesen kann, ohne Fehler, einfach leer. Ein zusätzliches
/// Wort ändert die Antwort.
///
/// Nachgewiesen an „Yeat Naked“: keine Treffer, während „Yeat Naked audio“
/// sofort das Original liefert. Ohne den zweiten Versuch fehlte YouTube
/// vollständig, und die Auswahl bestand nur aus fremden Fassungen.
pub async fn search(
    ytdlp: &Path,
    query: &str,
    source: SearchSource,
    limit: usize,
) -> Result<Vec<SearchResult>> {
    let results = search_once(ytdlp, query, source, limit).await?;
    if !results.is_empty() || source != SearchSource::Youtube {
        return Ok(results);
    }
    search_once(ytdlp, &format!("{} audio", query.trim()), source, limit).await
}

async fn search_once(
    ytdlp: &Path,
    query: &str,
    source: SearchSource,
    limit: usize,
) -> Result<Vec<SearchResult>> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }

    // Diese beiden haben eigene Schnittstellen und brauchen kein yt-dlp.
    match source {
        SearchSource::Bandcamp => return search_bandcamp(query, limit).await,
        SearchSource::Audius => return search_audius(query, limit).await,
        _ => {}
    }

    let mut args: Vec<String> = [
        "--dump-json",
        "--no-warnings",
        "--ignore-errors",
        // Dieselbe Nachsicht wie beim Download: Eine kurze Störung darf die
        // Quelle nicht aus der Trefferliste werfen.
        "--retries",
        "3",
        "--retry-sleep",
        "2",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    if source.needs_full_extraction() {
        // Die Trefferseite ist selbst eine Liste, sie darf nicht als
        // einzelner Titel behandelt werden.
        args.push("--playlist-items".into());
        args.push(format!("1-{limit}"));
    } else {
        args.push("--flat-playlist".into());
        // Sammlungen nur aufklappen, wenn die Adresse wirklich auf eine zeigt.
        if source != SearchSource::Url || !is_collection_url(query) {
            args.push("--no-playlist".into());
        }
    }
    args.extend(js_runtime_args(ytdlp).await);
    args.push(source.query_for(query, limit));

    let _slot = acquire_slot(source.label()).await;
    let ausgabe = tokio::time::timeout(SEARCH_TIMEOUT, crate::ytdlp::einmal(ytdlp, &args))
        .await
        .map_err(|_| anyhow!(fehler!("Die Suche bei {0} antwortet nicht.", source.label())))??;
    if !ausgabe.erfolg && ausgabe.stdout.is_empty() {
        bail!("Suche fehlgeschlagen: {}", explain_failure(&ausgabe.stderr));
    }

    let mut results = Vec::new();
    for line in ausgabe.stdout.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let id = value["id"].as_str().unwrap_or_default().to_string();
        let title = value["title"].as_str().unwrap_or("Ohne Titel").to_string();
        let url = value["webpage_url"]
            .as_str()
            .or_else(|| value["url"].as_str())
            .unwrap_or_default()
            .to_string();
        if url.is_empty() {
            continue;
        }
        // Die Trefferseite von YouTube Music enthält auch Künstler- und
        // Albumseiten. Nur einzelne Titel sind brauchbar.
        if source.needs_full_extraction() && !url.contains("watch?v=") {
            continue;
        }
        results.push(SearchResult {
            id,
            title,
            uploader: value["uploader"]
                .as_str()
                .or_else(|| value["channel"].as_str())
                .or_else(|| value["artist"].as_str())
                .map(str::to_string),
            duration_ms: sane_duration(value["duration"].as_f64().map(|d| (d * 1000.0) as i64)),
            url,
            thumbnail: value["thumbnail"].as_str().map(str::to_string),
            source: source.label().to_string(),
        });
    }
    Ok(results)
}

// ------------------------------------------------------- Trefferauswahl
//
// Für einen Spotify-Link ist die genaue Laufzeit bekannt. Statt blind den
// ersten Suchtreffer zu nehmen, holen wir mehrere und wählen den aus, der am
// besten passt, so fallen Remixe, Live-Fassungen und Videos mit Vorspann weg.

/// Zusätze, die auf eine andere Fassung hindeuten.
const VERSION_MARKERS: [&str; 34] = [
    "remix",
    "rmx",
    "live",
    "sped up",
    "speed up",
    "slowed",
    "reverb",
    "nightcore",
    "cover",
    "karaoke",
    "instrumental",
    "8d audio",
    "bass boost",
    "mashup",
    "snippet",
    "teaser",
    "acapella",
    // Auf Bandcamp und SoundCloud stehen fremde Fassungen fast immer unter
    // einem dieser Wörter, ohne sie gewinnen sie gegen das Original.
    "edit",
    "bootleg",
    "flip",
    "rework",
    "vip",
    "tribute",
    "in the style of",
    "made famous by",
    "type beat",
    // Aus dem Tonspur-Vergleich über 30 Titel nachgetragen: „Xtal (Duty Paid
    // Refix)“ und „Tropical Island [TEKK]“ gewannen gegen das Original.
    "refix",
    "tekk",
    // „Creep (Acoustic)“ gewann gegen die Albumfassung.
    "acoustic",
    "akustik",
    "unplugged",
    // Spuren von Mitschnitt-Diensten: „Money Trees (HD Lyrics) - [www Flvto
    // Com]“ stand über der offiziellen Veröffentlichung.
    "flvto",
    "y2mate",
    "320kbps",
];

/// Abschlag für Hinweise auf eine andere Fassung, die im gesuchten Titel
/// nicht vorkommen. Wer einen Remix sucht, bekommt ihn also weiterhin.
///
/// Verglichen wird wortweise: „edit“ darf nicht auf „editor“ anspringen.
fn version_penalty(candidate_title: &str, wanted_title: &str) -> f64 {
    let candidate = crate::online::normalize_words(candidate_title);
    let wanted = crate::online::normalize_words(wanted_title);
    VERSION_MARKERS
        .iter()
        .filter(|marker| {
            crate::online::contains_word_sequence(&candidate, marker)
                && !crate::online::contains_word_sequence(&wanted, marker)
        })
        .count() as f64
        * 25.0
}

/// Wie weit liegt ein Treffer textlich vom Gesuchten weg? 0.0 heißt: jedes
/// gesuchte Wort kommt in Titel oder Kanalname vor.
///
/// Ohne diese Prüfung entschieden allein Laufzeit und Quelle, und wer
/// irgendetwas Ähnliches anbot, gewann gegen den richtigen Titel.
fn coverage_penalty(query: &str, candidate: &SearchResult) -> f64 {
    let wanted = crate::online::normalize_for_match(query);
    let wanted_words: Vec<&str> = wanted.split(' ').filter(|w| !w.is_empty()).collect();
    if wanted_words.is_empty() {
        return 0.0;
    }

    let title = crate::online::normalize_for_match(&candidate.title);
    let mut haystack = title.clone();
    if let Some(uploader) = &candidate.uploader {
        haystack.push(' ');
        haystack.push_str(&crate::online::normalize_for_match(uploader));
    }
    let available: Vec<&str> = haystack.split(' ').filter(|w| !w.is_empty()).collect();

    let missing = wanted_words
        .iter()
        .filter(|word| !available.contains(*word))
        .count();

    // Zusätzliche Wörter im Titel deuten auf eine andere Fassung hin
    // („… (XY Remix)“), wiegen aber weniger als ein fehlendes Wort.
    let extra = title
        .split(' ')
        .filter(|word| !word.is_empty() && !wanted_words.contains(word))
        .count();

    missing as f64 * 20.0 + (extra as f64 * 3.0).min(15.0)
}

/// Kürzer als das ist kein ganzer Titel, sondern eine Vorschau.
const PREVIEW_LIMIT_MS: i64 = 60_000;

/// Länger als das ist kein Musiktitel. Kaputte oder böswillige Angaben aus
/// einer Suchantwort landen sonst als `i64::MAX` in der Bewertung. `as i64`
/// sättigt still auf den Höchstwert, statt zu scheitern.
const MAX_DURATION_MS: i64 = 24 * 60 * 60 * 1000;

/// Nimmt nur Laufzeiten an, mit denen sich rechnen lässt.
fn sane_duration(value: Option<i64>) -> Option<i64> {
    value.filter(|ms| *ms > 0 && *ms <= MAX_DURATION_MS)
}

/// Laufzeiten, die weiter als das auseinanderliegen, gehören nicht zur
/// selben Aufnahme.
const SAME_TAKE_MS: i64 = 15_000;

/// Wie lang ist der gesuchte Titel wirklich? Die Quellen wissen es gemeinsam:
/// Dieselbe Aufnahme taucht mehrfach auf, ihre Laufzeiten bilden eine Traube.
/// Ausreißer sind Ausschnitte, verlängerte Fassungen oder ein anderer Titel.
///
/// Das ersetzt eine Vorgabe von außen, wenn es keine gibt, also bei jeder
/// Suche über das Eingabefeld.
fn consensus_duration_ms(results: &[SearchResult]) -> Option<i64> {
    let mut durations: Vec<i64> = results
        .iter()
        .filter_map(|r| r.duration_ms)
        .filter(|ms| *ms > 0)
        .collect();
    // Unter drei Angaben ist von einer Mehrheit keine Rede.
    if durations.len() < 3 {
        return None;
    }
    durations.sort_unstable();

    // Die größte Gruppe, in der alle nah beieinanderliegen.
    let mut best: Option<(usize, i64)> = None;
    for (index, start) in durations.iter().enumerate() {
        let group: Vec<i64> = durations[index..]
            .iter()
            .copied()
            .take_while(|ms| ms.saturating_sub(*start) <= SAME_TAKE_MS)
            .collect();
        if best.is_none_or(|(count, _)| group.len() > count) {
            best = Some((group.len(), group[group.len() / 2]));
        }
    }

    best.filter(|(count, _)| *count >= 2).map(|(_, ms)| ms)
}

/// Abschlag für Abweichungen von der Mehrheitsmeinung. Gedeckelt, damit die
/// Länge den Titelabgleich unterstützt, ihn aber nicht überstimmt.
fn consensus_penalty(candidate: &SearchResult, consensus: Option<i64>) -> f64 {
    let (Some(consensus), Some(actual)) = (consensus, candidate.duration_ms) else {
        return 0.0;
    };
    let difference_seconds = (actual.saturating_sub(consensus).saturating_abs() as f64) / 1000.0;
    if difference_seconds <= 15.0 {
        return 0.0;
    }
    (difference_seconds * 0.5).min(40.0)
}

/// Abschlag für offensichtliche Ausschnitte. Greift nur, solange die
/// eigentliche Laufzeit unbekannt ist, sonst entscheidet der Vergleich.
///
/// Bewusst kleiner als ein fehlendes Suchwort: Der Ausschnitt des richtigen
/// Titels ist immer noch näher dran als ein fremder Titel in voller Länge.
fn preview_penalty(candidate: &SearchResult) -> f64 {
    match candidate.duration_ms {
        Some(ms) if ms > 0 && ms < PREVIEW_LIMIT_MS => 15.0,
        _ => 0.0,
    }
}

/// Je kleiner, desto besser. `None` bedeutet: kommt nicht infrage.
fn score_candidate(
    candidate: &SearchResult,
    query: &str,
    expected_duration_ms: Option<i64>,
    expected_title: &str,
) -> Option<f64> {
    let mut score = coverage_penalty(query, candidate);

    match expected_duration_ms.filter(|ms| *ms > 0) {
        Some(expected) => match candidate.duration_ms {
            Some(actual) => {
                let difference_seconds = (actual.saturating_sub(expected).saturating_abs() as f64) / 1000.0;
                // Mehr als eine halbe Minute daneben ist ein anderer Titel.
                if difference_seconds > 30.0 {
                    return None;
                }
                score += difference_seconds * 2.0;
            }
            // Bandcamp nennt keine Laufzeit. Solche Treffer fliegen nicht
            // raus, rutschen aber hinter alle nachweislich passenden.
            None => score += 12.0,
        },
        // Ohne Vorgabe bleibt nur die Plausibilität der Länge.
        None => score += preview_penalty(candidate),
    }

    // Zusätze, die im gesuchten Titel nicht vorkommen, sprechen dagegen.
    score += version_penalty(&candidate.title, expected_title);

    // Quellen, die verlässlich den ganzen Titel in guter Qualität liefern,
    // bekommen einen Vorsprung.
    if let Some(source) = SearchSource::from_label(&candidate.source) {
        score -= source.quality_bonus();
    }

    Some(score)
}

/// Ordnet gefundene Treffer nach Passgenauigkeit. Zurück kommen die Adressen
/// in der Reihenfolge, in der sie probiert werden.
fn rank_candidates(
    found: &[SearchResult],
    query: &str,
    expected_duration_ms: Option<i64>,
    expected_title: &str,
) -> Vec<String> {
    // Ohne Vorgabe von außen entscheidet die Mehrheit der Quellen, wie lang
    // der Titel ist. Nur abwerten, nicht ausschließen: Die Mehrheit kann
    // sich irren, eine bekannte Laufzeit nicht.
    let consensus = match expected_duration_ms.filter(|ms| *ms > 0) {
        Some(_) => None,
        None => consensus_duration_ms(found),
    };

    let mut ranked: Vec<(f64, &SearchResult)> = found
        .iter()
        .filter_map(|candidate| {
            score_candidate(candidate, query, expected_duration_ms, expected_title).map(|score| {
                (
                    score
                        + consensus_penalty(candidate, consensus)
                        + absage_abzug(&candidate.url),
                    candidate,
                )
            })
        })
        .collect();

    if ranked.is_empty() {
        // Kein Treffer hielt der Längenprüfung stand. Dann wenigstens nach
        // Textnähe ordnen, statt blind den ersten zu nehmen.
        ranked = found
            .iter()
            .map(|candidate| {
                (
                    coverage_penalty(query, candidate) + absage_abzug(&candidate.url),
                    candidate,
                )
            })
            .collect();
    }

    ranked.sort_by(|a, b| a.0.total_cmp(&b.0));
    ranked
        .into_iter()
        .map(|(_, candidate)| candidate.url.clone())
        .take(4)
        .collect()
}

/// Sucht mehrere Treffer und ordnet sie nach Passgenauigkeit.
pub async fn select_matches(
    ytdlp: &Path,
    query: &str,
    expected_duration_ms: Option<i64>,
    expected_title: &str,
) -> Result<Vec<String>> {
    let found = search_everywhere(ytdlp, query, 5).await?;
    Ok(rank_candidates(&found, query, expected_duration_ms, expected_title))
}

/// Zeigt die Adresse auf eine ganze Sammlung (Album, Playlist, Set)?
///
/// Ein YouTube-Link mit `v=` bleibt ein einzelnes Video, auch wenn zusätzlich
/// eine Playlist im Link steht, sonst würde aus einem kopierten Titel
/// versehentlich eine ganze Playlist.
fn is_collection_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();

    let has_video_id = lower.contains("v=") || lower.contains("youtu.be/");
    if has_video_id {
        return false;
    }

    const COLLECTION_MARKERS: [&str; 6] = [
        "/playlist",
        "/sets/",
        "/album/",
        "/list=",
        "list=",
        "/discography",
    ];
    COLLECTION_MARKERS
        .iter()
        .any(|marker| lower.contains(marker))
}

/// Ordnet Suchtreffer für die Anzeige: erst was zum Suchbegriff passt, dann
/// der Rest. Die Sortierung ist stabil, deshalb bleibt die Mischung der
/// Quellen bei gleichwertigen Treffern erhalten.
///
/// Ungeordnet stand sonst der erste Treffer irgendeiner Quelle oben, auch
/// wenn es ein ganz anderer Titel oder nur ein Ausschnitt war.
fn sort_by_relevance(results: &mut [SearchResult], query: &str) {
    let consensus = consensus_duration_ms(results);
    let rang = |candidate: &SearchResult| {
        coverage_penalty(query, candidate)
            // Ohne diese Prüfung standen Remixe und Edits ganz oben: der
            // Wortabgleich blendet Klammerinhalte aus, dort stehen sie aber.
            + version_penalty(&candidate.title, query)
            + preview_penalty(candidate)
            // Angeschnittene Uploads erkennt man daran, dass alle anderen
            // Quellen sich auf eine andere Länge einigen.
            + consensus_penalty(candidate, consensus)
            // Ohne Laufzeitangabe lässt sich weder ein Ausschnitt erkennen
            // noch die Mehrheitslänge prüfen. Im Tonspur-Vergleich waren das
            // durchweg Bandcamp-Treffer, und darunter auffällig viele
            // Fremdfassungen, die sonst ungeprüft nach oben rutschten.
            + if candidate.duration_ms.is_none() { 12.0 } else { 0.0 }
    };
    results.sort_by(|a, b| rang(a).total_cmp(&rang(b)));
}

/// Fragt alle Suchquellen gleichzeitig ab und mischt die Treffer, sodass jede
/// Quelle oben vertreten ist. So muss niemand vorher eine Quelle auswählen.
pub async fn search_everywhere(
    ytdlp: &Path,
    query: &str,
    per_source: usize,
) -> Result<Vec<SearchResult>> {
    Ok(search_all_sources(ytdlp, query, per_source).await?.0)
}

/// Wie [`search_everywhere`], nennt zusätzlich die ausgefallenen Quellen.
///
/// Fällt eine Quelle aus, während andere liefern, wirkt die Trefferliste nur
/// dünn, der Nutzer sieht nicht, dass ihm etwas fehlt. Bei einer Sperre von
/// YouTube betrifft das genau die Quelle mit der größten Auswahl.
pub async fn search_all_sources(
    ytdlp: &Path,
    query: &str,
    per_source: usize,
) -> Result<(Vec<SearchResult>, Vec<String>)> {
    let (musik, bandcamp, audius, soundcloud, youtube) = tokio::join!(
        search(ytdlp, query, SEARCHED_SOURCES[0], per_source),
        search(ytdlp, query, SEARCHED_SOURCES[1], per_source),
        search(ytdlp, query, SEARCHED_SOURCES[2], per_source),
        search(ytdlp, query, SEARCHED_SOURCES[3], per_source),
        search(ytdlp, query, SEARCHED_SOURCES[4], per_source),
    );

    let mut lists: Vec<std::vec::IntoIter<SearchResult>> = Vec::new();
    let mut first_error = None;
    let mut ausgefallen: Vec<String> = Vec::new();
    for (quelle, result) in SEARCHED_SOURCES
        .iter()
        .zip([musik, bandcamp, audius, soundcloud, youtube])
    {
        match result {
            Ok(items) => lists.push(items.into_iter()),
            Err(err) => {
                ausgefallen.push(quelle.label().to_string());
                first_error = first_error.or(Some(err));
            }
        }
    }

    // Reihum je einen Treffer entnehmen.
    let mut merged: Vec<SearchResult> = Vec::new();
    loop {
        let mut added = false;
        for list in lists.iter_mut() {
            if let Some(item) = list.next() {
                if !merged.iter().any(|existing| existing.url == item.url) {
                    merged.push(item);
                }
                added = true;
            }
        }
        if !added {
            break;
        }
    }

    sort_by_relevance(&mut merged, query);

    if merged.is_empty() {
        if let Some(err) = first_error {
            return Err(err);
        }
    }
    Ok((merged, ausgefallen))
}

/// Übersetzt die Ausgabe von yt-dlp in eine Meldung, mit der man etwas
/// anfangen kann. yt-dlp verteilt Fehler oft über mehrere Zeilen und hängt
/// Hinweise für Issue-Meldungen an, die reine letzte Zeile ist deshalb
/// meist wertlos.
/// Erklärung für einen abgelehnten Zugriff. Ohne JavaScript-Laufzeit ist das
/// fast immer die Ursache, mit einer ist es meist nur eine kurze Drosselung.
fn blocked_message(has_js_runtime: bool) -> String {
    // Auf Android geht beides ins Leere, was die anderen beiden Sätze raten:
    // Eine JavaScript-Laufzeit lässt sich nicht nachinstallieren, und `yt-dlp
    // -U` gibt es nicht, weil yt-dlp dort keine Datei ist, sondern in der
    // Bibliothek steckt. Fehlen tut auch nichts: Dieselbe Bibliothek bringt
    // QuickJS mit und reicht es yt-dlp über `--js-runtimes` weiter. Bleibt
    // also nur die Drosselung — und der Rat, es woanders zu versuchen.
    if cfg!(target_os = "android") {
        return fehler!(
            "Die Quelle hat den Zugriff abgelehnt (403). Das kann an zu vielen Abrufen kurz hintereinander liegen. Warte ein paar Minuten oder versuche einen anderen Treffer; oft liegt derselbe Titel auch bei SoundCloud oder Bandcamp."
        );
    }
    if has_js_runtime {
        fehler!(
            "Die Quelle hat den Zugriff abgelehnt (403). Das kann an zu vielen Abrufen kurz hintereinander liegen. Warte ein paar Minuten. Hilft das nicht, aktualisiere yt-dlp (`yt-dlp -U`)."
        )
    } else {
        fehler!(
            "Die Quelle hat den Zugriff abgelehnt (403). Es ist keine JavaScript-Laufzeit installiert. Ohne sie kann yt-dlp die Abrufadressen von YouTube nicht korrekt bilden. Installiere Node.js, Deno oder Bun, dann funktioniert es dauerhaft."
        )
    }
}

/// Hängt die Originalzeile an, damit die Ursache nachvollziehbar bleibt.
/// Hängt die wörtliche Meldung der Quelle an die eigene Erklärung.
///
/// Verkettet statt zusammengesetzt: Beide Sätze bleiben einzeln nachschlagbar,
/// und der Nachsatz muss nicht in jede der Erklärungen hineingeschrieben
/// werden. Die Meldung der Quelle selbst ist Englisch und bleibt es, sie
/// stammt von yt-dlp.
fn with_details(message: String, raw: &str) -> String {
    let detail = raw.trim().trim_start_matches("ERROR:").trim();
    if detail.is_empty() {
        return message;
    }
    crate::meldung::verketten(message, fehler!("Meldung der Quelle: {0}", detail))
}

fn explain_failure(stderr: &str) -> String {
    let lines: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();

    let raw = lines
        .iter()
        .find(|line| line.starts_with("ERROR:"))
        .or_else(|| lines.last())
        .copied()
        .unwrap_or("unbekannter Fehler");

    // Der komplette Text wird durchsucht, weil die Ursache manchmal in einer
    // Zeile vor oder nach der ERROR-Zeile steht.
    let haystack = stderr.to_lowercase();

    // Nur echte HTTP-Fehler, nicht jede Ziffernfolge: „403“ kann auch in
    // einer Video-Kennung oder Byte-Zahl stecken.
    if haystack.contains("http error 403")
        || haystack.contains("http error 429")
        || haystack.contains("too many requests")
    {
        return with_details(blocked_message(js_runtime().is_some()), raw);
    }
    // Die Nachbearbeitung stolpert über eine fehlende Python-Bibliothek.
    // Der Ton wäre da gewesen, nur das Cover ließ sich nicht einbetten.
    if haystack.contains("module mutagen was not found") {
        return with_details(
            fehler!(
                "Zum Einbetten des Covers fehlt yt-dlp die Bibliothek „mutagen“. Robify lässt das Cover künftig weg und schreibt es beim Import selbst hinein. Versuche den Titel einfach erneut. Wer es lieber direkt von yt-dlp hätte: `python3 -m pip install mutagen`."
            ),
            raw,
        );
    }
    if haystack.contains("requested format is not available") {
        return with_details(
            fehler!(
                "Von dieser Quelle gibt es nur eine Vorschau statt des ganzen Titels. Versuche einen anderen Treffer oder eine andere Quelle."
            ),
            raw,
        );
    }
    if haystack.contains("drm") {
        return with_details(
            fehler!(
                "Diese Aufnahme ist kopiergeschützt und gibt die Quelle nur als Stream heraus, daran lässt sich nichts ändern. Robify probiert deshalb die übrigen Treffer; scheitern alle, gibt es den Titel derzeit auf keiner erreichbaren Quelle frei."
            ),
            raw,
        );
    }
    if haystack.contains("sign in to confirm") || haystack.contains("not a bot") {
        return with_details(
            fehler!(
                "YouTube verlangt für diesen Titel eine Anmeldung (Bot-Prüfung). Versuche eine andere Quelle oder einen anderen Treffer."
            ),
            raw,
        );
    }
    if haystack.contains("private video") {
        return fehler!("Das Video ist privat und nicht abrufbar.");
    }
    if haystack.contains("video unavailable") || haystack.contains("removed by the uploader") {
        return fehler!("Das Video ist nicht (mehr) verfügbar.");
    }
    if haystack.contains("not available in your country") || haystack.contains("geo") {
        return fehler!("Dieser Titel ist in deiner Region gesperrt.");
    }
    if haystack.contains("members-only") || haystack.contains("premium") {
        return fehler!("Der Titel ist nur für zahlende Mitglieder abrufbar.");
    }
    if haystack.contains("unable to extract")
        || haystack.contains("nsig")
        || haystack.contains("player response")
    {
        return with_details(
            fehler!(
                "yt-dlp konnte die Quelle nicht auslesen. Meist hilft ein Update: `yt-dlp -U` (oder über den Paketmanager)."
            ),
            raw,
        );
    }
    if haystack.contains("ffmpeg") {
        return fehler!(
            "ffmpeg wird für dieses Format benötigt, wurde aber nicht gefunden oder ist fehlgeschlagen."
        );
    }

    fehler!("yt-dlp: {0}", raw.trim_start_matches("ERROR:").trim())
}

/// Lässt yt-dlp laufen und meldet den Fortschritt.
///
/// Auf dem Rechner ein eigener Prozess, dessen Ausgabe zeilenweise mitgelesen
/// wird: So greift ein Abbruch zeitnah, und ein hängender Lauf fällt an der
/// ausbleibenden Zeile auf.
#[cfg(not(target_os = "android"))]
async fn laufen_lassen<R: Runtime>(
    app: &AppHandle<R>,
    job_id: &str,
    ytdlp: &Path,
    args: &[String],
    job_dir: &Path,
    cancel: &Arc<AtomicBool>,
) -> Result<()> {
    let mut cmd = Command::new(ytdlp);
    cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
    configure(&mut cmd);

    let mut child = cmd
        .spawn()
        .map_err(|e| anyhow!(fehler!("yt-dlp-Start fehlgeschlagen: {0}", e)))?;
    let stdout = child.stdout.take().expect("stdout ist gesetzt");
    let stderr = child.stderr.take().expect("stderr ist gesetzt");

    let gestartet = Instant::now();
    let mut letzte_zeile = Instant::now();
    let mut out_lines = BufReader::new(stdout).lines();
    let stderr_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        let mut collected = String::new();
        while let Ok(Some(line)) = lines.next_line().await {
            if collected.len() < 4000 {
                collected.push_str(&line);
                collected.push('\n');
            }
        }
        collected
    });

    loop {
        if cancel.load(Ordering::SeqCst) {
            let _ = child.kill().await;
            let _ = std::fs::remove_dir_all(job_dir);
            bail!(fehler!("Download abgebrochen"));
        }

        // Ein hängender Prozess schreibt nichts mehr. Beides begrenzen: die
        // Gesamtdauer und die Stille dazwischen.
        if gestartet.elapsed() > DOWNLOAD_TIMEOUT || letzte_zeile.elapsed() > IDLE_TIMEOUT {
            let _ = child.kill().await;
            let _ = std::fs::remove_dir_all(job_dir);
            bail!(
                "Die Quelle antwortet nicht mehr, nach {} ohne Fortschritt abgebrochen.",
                format_seconds(letzte_zeile.elapsed().as_millis() as i64)
            );
        }

        // Zeilenweise lesen, damit der Abbruch zeitnah greift.
        let next =
            tokio::time::timeout(std::time::Duration::from_millis(400), out_lines.next_line())
                .await;
        match next {
            Ok(Ok(Some(line))) => {
                letzte_zeile = Instant::now();
                if let Some(progress) = parse_progress(job_id, &line) {
                    emit(app, progress);
                } else if line.starts_with("[ExtractAudio]") || line.starts_with("[Metadata]") {
                    emit(app, status(job_id, "processing", 99.0, Some(line)));
                }
            }
            Ok(Ok(None)) => break,
            Ok(Err(err)) => bail!(fehler!("Fehler beim Lesen der Ausgabe: {0}", err)),
            Err(_) => continue,
        }
    }

    let exit = child.wait().await?;
    let stderr_text = stderr_task.await.unwrap_or_default();
    if !exit.success() {
        bail!("{}", explain_failure(&stderr_text));
    }
    Ok(())
}

/// Lässt yt-dlp laufen, über die Java-Brücke.
///
/// Dort gibt es keinen Prozess und keine Ausgabe zum Mitlesen: Der Aufruf
/// blockiert bis zum Ende und liefert erst dann. Den Fortschritt führt die
/// Brücke deshalb als abfragbaren Wert, den dieser Lauf im Takt abholt und
/// weitergibt. Ein Abbruch geht denselben Weg zurück.
#[cfg(target_os = "android")]
async fn laufen_lassen<R: Runtime>(
    app: &AppHandle<R>,
    job_id: &str,
    _ytdlp: &Path,
    args: &[String],
    job_dir: &Path,
    cancel: &Arc<AtomicBool>,
) -> Result<()> {
    let kennung = format!("robify-{job_id}");
    let mitgabe = args.to_vec();
    let fuer_faden = kennung.clone();
    let lauf = tokio::task::spawn_blocking(move || {
        crate::ytdlp::bruecke_rufen(&fuer_faden, &mitgabe)
    });
    tokio::pin!(lauf);

    let gestartet = Instant::now();
    let mut zuletzt = 0.0_f32;
    let mut seit_bewegung = Instant::now();

    let ausgabe = loop {
        tokio::select! {
            fertig = &mut lauf => break fertig??,
            _ = tokio::time::sleep(std::time::Duration::from_millis(400)) => {
                if cancel.load(Ordering::SeqCst) {
                    crate::ytdlp::abbrechen(&kennung);
                    let _ = std::fs::remove_dir_all(job_dir);
                    bail!(fehler!("Download abgebrochen"));
                }
                if gestartet.elapsed() > DOWNLOAD_TIMEOUT || seit_bewegung.elapsed() > IDLE_TIMEOUT {
                    crate::ytdlp::abbrechen(&kennung);
                    let _ = std::fs::remove_dir_all(job_dir);
                    bail!(
                        "Die Quelle antwortet nicht mehr, nach {} ohne Fortschritt abgebrochen.",
                        format_seconds(seit_bewegung.elapsed().as_millis() as i64)
                    );
                }
                if let Some(prozent) = crate::ytdlp::fortschritt(&kennung) {
                    if prozent > zuletzt {
                        zuletzt = prozent;
                        seit_bewegung = Instant::now();
                        emit(app, status(job_id, "downloading", prozent as f64, None));
                    }
                }
            }
        }
    };

    if !ausgabe.erfolg {
        bail!("{}", explain_failure(&ausgabe.stderr));
    }
    Ok(())
}

fn format_seconds(ms: i64) -> String {
    let total = ms.max(0) / 1000;
    format!("{}:{:02}", total / 60, total % 60)
}

fn parse_opt<T: std::str::FromStr>(value: &str) -> Option<T> {
    let value = value.trim();
    if value.is_empty() || value == "NA" || value == "None" {
        return None;
    }
    value.parse().ok()
}

fn parse_progress(job_id: &str, line: &str) -> Option<DownloadProgress> {
    let rest = line.strip_prefix(PROGRESS_MARKER)?;
    let fields: Vec<&str> = rest.split('|').collect();
    if fields.len() < 5 {
        return None;
    }
    let downloaded: Option<i64> = parse_opt(fields[1]);
    let total: Option<i64> = parse_opt::<i64>(fields[2]).or_else(|| parse_opt(fields[3]));
    let percent = match (downloaded, total) {
        (Some(d), Some(t)) if t > 0 => (d as f64 / t as f64 * 100.0).clamp(0.0, 100.0),
        _ => 0.0,
    };
    Some(DownloadProgress {
        job_id: job_id.to_string(),
        status: "downloading".into(),
        percent,
        downloaded_bytes: downloaded,
        total_bytes: total,
        speed_bytes: parse_opt(fields[4]),
        eta_seconds: fields.get(5).and_then(|f| parse_opt(f)),
        message: None,
    })
}

fn emit<R: Runtime>(app: &AppHandle<R>, progress: DownloadProgress) {
    let _ = app.emit("download:progress", progress);
}

fn status(job_id: &str, status: &str, percent: f64, message: Option<String>) -> DownloadProgress {
    DownloadProgress {
        job_id: job_id.to_string(),
        status: status.to_string(),
        percent,
        downloaded_bytes: None,
        total_bytes: None,
        speed_bytes: None,
        eta_seconds: None,
        message,
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn download<R: Runtime>(
    app: AppHandle<R>,
    registry: Arc<DownloadRegistry>,
    ytdlp: PathBuf,
    work_dir: PathBuf,
    job_id: String,
    options: DownloadOptions,
) -> Result<DownloadOutcome> {
    let cancel = registry.register(&job_id);

    // Entweder den besten Treffer heraussuchen …
    let attempts = match options.match_query.as_deref() {
        Some(query) if !query.trim().is_empty() => {
            emit(
                &app,
                status(&job_id, "starting", 0.0, Some("Passende Aufnahme wird gesucht…".into())),
            );
            let expected_title = options
                .metadata
                .as_ref()
                .map(|m| m.title.as_str())
                .unwrap_or(query);

            match select_matches(&ytdlp, query, options.expected_duration_ms, expected_title).await
            {
                Ok(urls) if !urls.is_empty() => urls,
                // Findet die Auswahl nichts, bleibt der ursprüngliche Weg.
                _ => std::iter::once(options.url.clone())
                    .chain(options.fallbacks.iter().cloned())
                    .collect(),
            }
        }
        // … oder die vorgegebene Adresse samt Ausweichliste nehmen.
        _ => std::iter::once(options.url.clone())
            .chain(options.fallbacks.iter().cloned())
            .collect(),
    };

    // Sagt YouTube gerade ab, kommt es nach hinten.
    //
    // Der gewählte Treffer verliert damit seinen Vorrang, aber ein Versuch,
    // von dem man weiß, dass er scheitert, kostet nur eine halbe Minute. Die
    // Reihenfolge unter den übrigen bleibt, wie sie war.
    let mut attempts = attempts;
    if youtube_sagt_ab() {
        attempts.sort_by_key(|url| ist_youtube(url));
    }

    let mut result = Err(anyhow!(fehler!("Keine Quelle angegeben")));
    for (index, url) in attempts.iter().enumerate() {
        let attempt = DownloadOptions {
            url: url.clone(),
            ..options.clone()
        };
        result = download_inner(&app, &cancel, &ytdlp, &work_dir, &job_id, &attempt).await;

        if result.is_ok() || cancel.load(Ordering::SeqCst) {
            break;
        }
        // Eine Absage von YouTube gilt für alle seine Treffer, nicht nur für
        // diesen. Erkannt am „(403)“ aus `blocked_message`; die Meldung wird
        // erst in der Oberfläche übersetzt, die Nummer steht in jeder Sprache.
        if ist_youtube(url) && result.as_ref().err().is_some_and(zugriff_verweigert) {
            absage_merken();
        }
        if index + 1 < attempts.len() {
            emit(
                &app,
                status(
                    &job_id,
                    "starting",
                    0.0,
                    Some("Nichts gefunden, nächste Quelle wird versucht…".into()),
                ),
            );
        }
    }
    registry.finish(&job_id);

    match &result {
        Ok(outcome) => {
            emit(
                &app,
                status(&job_id, "done", 100.0, Some(outcome.path.clone())),
            );
        }
        Err(err) => {
            // Keine Adresse hat geliefert. Bruchstücke und Teildateien
            // hätten sonst dauerhaft im Arbeitsverzeichnis gelegen.
            let _ = std::fs::remove_dir_all(work_dir.join(&job_id));
            emit(&app, status(&job_id, "error", 0.0, Some(err.to_string())));
        }
    }
    result
}

async fn download_inner<R: Runtime>(
    app: &AppHandle<R>,
    cancel: &Arc<AtomicBool>,
    ytdlp: &Path,
    work_dir: &Path,
    job_id: &str,
    options: &DownloadOptions,
) -> Result<DownloadOutcome> {
    if options.url.trim().is_empty() {
        bail!(fehler!("Keine Quelle angegeben"));
    }
    // Bei einem zweiten Versuch soll nichts vom ersten herumliegen.
    let job_dir = work_dir.join(job_id);
    let _ = std::fs::remove_dir_all(&job_dir);
    std::fs::create_dir_all(&job_dir)?;
    let result_file = job_dir.join("result.txt");
    let uploader_file = job_dir.join("uploader.txt");
    let music_file = job_dir.join("musik.txt");

    emit(
        app,
        status(job_id, "starting", 0.0, Some("Quelle wird gelesen…".into())),
    );

    // Als Liste statt am Befehl aufgebaut: Auf Android startet kein Programm,
    // dort geht dieselbe Liste über die Java-Brücke an yt-dlp.
    let mut args: Vec<String> = vec![
        "--newline".into(),
        "--no-playlist".into(),
        // Die Warnungen bleiben an.
        //
        // Sie standen früher unter `--no-warnings`, und damit verschwand
        // ausgerechnet der Hinweis, der einen tagelang unerklärlichen 403
        // aufklärte: „No supported JavaScript runtime could be found …
        // YouTube extraction without a JS runtime has been deprecated.“ Der
        // Fehler nennt nur die Absage, den Grund nennt die Warnung davor.
        // `explain_failure` sucht ohnehin die ERROR-Zeile und nimmt die
        // letzte Zeile nur, wenn es keine gibt.
        // Kurzzeitige Sperren (HTTP 403) verschwinden meist von selbst.
        "--retries".into(),
        "5".into(),
        "--extractor-retries".into(),
        "3".into(),
        "--retry-sleep".into(),
        "3".into(),
        "--progress".into(),
        "--progress-template".into(),
        format!(
            "download:{PROGRESS_MARKER}|%(progress.downloaded_bytes)s|%(progress.total_bytes)s|\
             %(progress.total_bytes_estimate)s|%(progress.speed)s|%(progress.eta)s"
        ),
        "--print-to-file".into(),
        "after_move:filepath".into(),
        result_file.to_string_lossy().into_owned(),
        // Das Konto, unter dem der Titel veröffentlicht wurde.
        "--print-to-file".into(),
        "%(uploader,channel,creator,artist)s".into(),
        uploader_file.to_string_lossy().into_owned(),
        // Die Musikangaben der Quelle. Wo es sie gibt (YouTube Music, offizielle
        // Uploads, SoundCloud), sind sie sauberer als alles, was sich aus dem
        // Videotitel ableiten lässt.
        "--print-to-file".into(),
        "%(track)s\u{1f}%(artist)s\u{1f}%(album)s\u{1f}%(release_year)s\u{1f}%(track_number)s".into(),
        music_file.to_string_lossy().into_owned(),
        // Welchen Abspiel-Client yt-dlp bei YouTube benutzt.
        //
        // Von sich aus wählt yt-dlp `android_vr`; dessen Abrufadressen weist
        // YouTube inzwischen mit „HTTP Error 403“ ab — bei jedem Titel, auch
        // bei frei lizenzierten. `web_embedded` liefert Adressen, die YouTube
        // annimmt, sofern eine JavaScript-Laufzeit da ist. Nachgemessen:
        // `default` scheitert, `web_embedded,default` lädt.
        //
        // Die übrigen Clients bleiben als Rückfall dahinter stehen, sonst
        // fiele weg, was nur einer von ihnen hergibt. Die Angabe trägt den
        // Namensraum `youtube:` und geht andere Quellen nichts an.
        "--extractor-args".into(),
        "youtube:player_client=web_embedded,default".into(),
        "-f".into(),
        format_selector(options.format == "best"),
        "-o".into(),
        job_dir
            .join("%(title).150B.%(ext)s")
            .to_string_lossy()
            .into_owned(),
    ];

    if ffmpeg_available() {
        // `-x` löst die Audiospur aus dem Container. Ohne `--audio-format`
        // bleibt sie unverändert, kein zweiter verlustbehafteter Durchgang.
        args.push("-x".into());
        if options.format != "best" {
            args.push("--audio-format".into());
            args.push(options.format.clone());
            args.push("--audio-quality".into());
            args.push(options.quality.clone().unwrap_or_else(|| "0".into()));
        }
    } else if options.format != "best" {
        bail!(
            "ffmpeg wird für die Umwandlung nach {} benötigt.",
            options.format
        );
    }
    // Fehlt `mutagen`, wird das Cover ausgelassen statt den Download zu
    // verlieren. Robify schreibt es beim Import ohnehin selbst hinein.
    if options.embed_thumbnail && ffmpeg_available() && supports_thumbnail_embedding(ytdlp).await {
        args.push("--embed-thumbnail".into());
    }
    args.push("--embed-metadata".into());
    args.push(options.url.clone());
    args.extend(js_runtime_args(ytdlp).await);

    // Der Platz bleibt für die gesamte Dauer des Downloads belegt, sonst
    // liefen bei „Alle laden“ beliebig viele Übertragungen nebeneinander.
    let _slot = acquire_slot(source_label(&options.url)).await;

    laufen_lassen(app, job_id, ytdlp, &args, &job_dir, cancel).await?;

    emit(
        app,
        status(job_id, "processing", 99.0, Some("Metadaten werden gelesen…".into())),
    );

    if cancel.load(Ordering::SeqCst) {
        let _ = std::fs::remove_dir_all(&job_dir);
        bail!(fehler!("Download abgebrochen"));
    }

    let path = locate_output(&result_file, &job_dir)?;
    let path = ensure_playable(app, job_id, path).await?;
    let file_tags = tags::read(&path)?;

    // Ein deutlich zu kurzes Ergebnis ist ein Ausschnitt, kein ganzer Titel.
    if let Some(expected) = options.expected_duration_ms.filter(|ms| *ms > 0) {
        let actual = file_tags.duration_ms;
        // Als Verhältnis statt als Produkt: Eine kaputte Laufzeitangabe darf
        // die Rechnung nicht überlaufen lassen.
        if (actual as f64) < (expected as f64) * 0.8 {
            bail!(
                "Diese Quelle lieferte nur {} von {}, vermutlich eine Vorschau.",
                format_seconds(actual),
                format_seconds(expected)
            );
        }
    }

    let mut metadata = file_tags.metadata;
    if let Some((data, mime)) = file_tags.cover {
        metadata.cover_base64 = Some(base64::engine::general_purpose::STANDARD.encode(&data));
        metadata.cover_mime = Some(mime);
    }

    let uploader = std::fs::read_to_string(&uploader_file)
        .ok()
        .and_then(|text| text.lines().next().map(str::trim).map(str::to_string))
        .filter(|name| !name.is_empty() && name != "NA");

    // Erst die Angaben der Quelle, dann der Videotitel. Beides steht vor dem,
    // was yt-dlp mangels Musikfeldern in die Datei geschrieben hat, dort
    // landet sonst der Kanalname als Künstler.
    let source = SourceMetadata::read(&music_file);
    apply_source_metadata(&mut metadata, &source, uploader.as_deref());

    // Die Angaben aus einer Videobeschreibung sind oft grob, online
    // nachschlagen und korrigieren, sofern der Treffer eindeutig ist.
    // Nach einem Abbruch lohnt die Suche nicht mehr.
    if options.auto_match && !cancel.load(Ordering::SeqCst) {
        emit(
            app,
            status(job_id, "processing", 99.0, Some("Metadaten werden gesucht…".into())),
        );
        if let Some(found) = crate::online::auto_match(
            &metadata,
            Some(file_tags.duration_ms),
            options.auto_cover,
            options.auto_lyrics,
        )
        .await
        {
            metadata = crate::online::merge_match(metadata, found);
        }
    }

    // Gäste, die im Titel stehen, gehören ins Gästefeld, sonst tauchen sie
    // in der Bibliothek bei keinem Künstler auf. Bisher passierte das nur bei
    // Spotify-Links; die übrigen Quellen schreiben es genauso in den Titel.
    let (ohne_gaeste, aus_titel) = crate::spotify::split_feature_suffix(&metadata.title);
    if !aus_titel.is_empty() {
        metadata.title = ohne_gaeste;
        let mut gaeste = crate::library::split_artists(
            metadata.featured_artists.as_deref().unwrap_or_default(),
        );
        for name in aus_titel {
            if !gaeste.iter().any(|vorhanden| vorhanden.eq_ignore_ascii_case(&name)) {
                gaeste.push(name);
            }
        }
        metadata.featured_artists = Some(crate::library::join_artists(&gaeste));
    }

    // Die Quelle weiß, aus welcher Veröffentlichung die Aufnahme stammt,
    // die Metadatensuche rät dagegen und landet auch mal bei einer
    // Vinyl-Auskopplung oder einem Sampler.
    //
    // Ausnahme: Heißt das „Album“ genauso wie der Titel, ist es nur der
    // Platzhalter einer Single. Dann führt die Suche eher zum echten Album
    // („Creep“ → „Pablo Honey“).
    if let Some(album) = source
        .album
        .as_deref()
        .filter(|album| !crate::online::looks_like_same(album, &metadata.title))
    {
        metadata.album = album.to_string();
    }

    // Vorbekannte Angaben (z. B. aus einem Spotify-Link) haben das letzte Wort.
    if let Some(known) = &options.metadata {
        metadata = merge_metadata(known, metadata);
    }

    // Spotify kennt keine Gastrollen und wirft alle Beteiligten in eine Liste.
    // Hat die Online-Suche Gäste erkannt, gehören sie nicht zusätzlich unter
    // die Hauptkünstler.
    if let Some(featured) = metadata.featured_artists.clone() {
        let guests = crate::library::split_artists(&featured);
        let mains: Vec<String> = crate::library::split_artists(&metadata.artist)
            .into_iter()
            .filter(|name| !guests.iter().any(|guest| guest.eq_ignore_ascii_case(name)))
            .collect();
        if !mains.is_empty() {
            metadata.artist = crate::library::join_artists(&mains);
        }
    }

    // Der Kanal, von dem geladen wurde, ist der Hauptkünstler, sofern er
    // überhaupt zu den Beteiligten gehört.
    if let Some(uploader) = uploader {
        if let Some((main, featured)) = crate::library::promote_uploader(
            &metadata.artist,
            metadata.featured_artists.as_deref(),
            &uploader,
        ) {
            metadata.artist = main;
            metadata.featured_artists = featured;
        }
    }

    // Ohne Album ist es eine Single. „Album“ wäre hier schlicht falsch.
    if metadata.release_type.is_none() {
        metadata.release_type = Some(
            if metadata.album.trim().is_empty() {
                "single"
            } else {
                "album"
            }
            .to_string(),
        );
    }

    // Ohne Cover hat auch die Metadatensuche nichts gefunden, bei allen
    // beobachteten Fehlgriffen war das so.
    let unbestaetigt = metadata.cover_base64.is_none();
    let warning = intent_warning(options.intent.as_deref(), &metadata, unbestaetigt).map(|text| {
        if unbestaetigt {
            format!("{text} Auch online war dazu nichts zu finden.")
        } else {
            text
        }
    });

    Ok(DownloadOutcome {
        job_id: job_id.to_string(),
        path: path.to_string_lossy().to_string(),
        duration_ms: file_tags.duration_ms,
        format: file_tags.format,
        metadata,
        source_url: options.url.clone(),
        warning,
    })
}

/// Verbindet vorbekannte Metadaten mit denen aus der Datei. Was die Quelle
/// sicher weiß, gewinnt; alles andere wird aus der Datei aufgefüllt.
fn merge_metadata(known: &TrackMetadata, from_file: TrackMetadata) -> TrackMetadata {
    let pick = |preferred: &str, fallback: String| {
        if preferred.trim().is_empty() {
            fallback
        } else {
            preferred.to_string()
        }
    };
    let pick_opt = |preferred: &Option<String>, fallback: Option<String>| {
        preferred
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
            .or(fallback)
    };

    TrackMetadata {
        title: pick(&known.title, from_file.title),
        artist: pick(&known.artist, from_file.artist),
        featured_artists: pick_opt(&known.featured_artists, from_file.featured_artists),
        album: pick(&known.album, from_file.album),
        album_artist: pick_opt(&known.album_artist, from_file.album_artist),
        release_type: pick_opt(&known.release_type, from_file.release_type),
        year: known.year.or(from_file.year),
        track_no: known.track_no.or(from_file.track_no),
        disc_no: known.disc_no.or(from_file.disc_no),
        genre: pick_opt(&known.genre, from_file.genre),
        cover_base64: pick_opt(&known.cover_base64, from_file.cover_base64),
        cover_mime: pick_opt(&known.cover_mime, from_file.cover_mime),
        lyrics_synced: pick_opt(&known.lyrics_synced, from_file.lyrics_synced),
        lyrics_plain: pick_opt(&known.lyrics_plain, from_file.lyrics_plain),
    }
}

/// yt-dlp schreibt den Endpfad in `result.txt`; falls das fehlschlägt,
/// nehmen wir die neueste Audiodatei im Arbeitsverzeichnis.
/// Was die Quelle selbst über den Titel weiß.
///
/// yt-dlp füllt diese Felder für Musik, bei YouTube Music immer, bei
/// offiziellen YouTube-Uploads und SoundCloud meistens. Fremde Lyric- und
/// Repost-Kanäle liefern nichts; dort bleibt nur der Videotitel.
#[derive(Debug, Default, PartialEq)]
pub struct SourceMetadata {
    pub track: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub year: Option<i64>,
    pub track_no: Option<i64>,
}

impl SourceMetadata {
    fn read(path: &Path) -> SourceMetadata {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| text.lines().next().map(SourceMetadata::parse))
            .unwrap_or_default()
    }

    /// Die Felder kommen durch `\u{1f}` getrennt; „NA“ steht für „unbekannt“.
    fn parse(line: &str) -> SourceMetadata {
        let mut felder = line.split('\u{1f}').map(|wert| {
            let wert = wert.trim();
            (!wert.is_empty() && wert != "NA").then(|| wert.to_string())
        });
        let mut next = || felder.next().flatten();
        SourceMetadata {
            track: next(),
            artist: next(),
            album: next(),
            year: next().and_then(|wert| wert.parse().ok()),
            track_no: next().and_then(|wert| wert.parse().ok()),
        }
    }
}

/// Übernimmt Titel und Künstler aus der besten verfügbaren Quelle.
///
/// Reihenfolge: Musikfelder der Quelle → Zerlegung des Videotitels → das, was
/// schon in der Datei stand. Ohne diesen Schritt steht bei Uploads ohne
/// Musikfelder der komplette Videotitel als Songtitel und der Kanalname als
/// Künstler in der Bibliothek („xTheLYRICS“ statt „Nina Chuba“).
fn apply_source_metadata(
    metadata: &mut TrackMetadata,
    source: &SourceMetadata,
    uploader: Option<&str>,
) {
    if let Some(track) = &source.track {
        metadata.title = track.clone();
    }
    if let Some(artist) = &source.artist {
        metadata.artist = artist.clone();
    }
    if let Some(album) = &source.album {
        if metadata.album.trim().is_empty() {
            metadata.album = album.clone();
        }
    }
    metadata.year = metadata.year.or(source.year);
    metadata.track_no = metadata.track_no.or(source.track_no);

    // SoundCloud setzt `track` einfach auf den Namen der hochgeladenen Datei,
    // samt Endung. Ohne das Abschneiden hieße der Titel „Xtal.mp3“.
    metadata.title = strip_file_suffix(&metadata.title);

    let Some((artist, title)) = crate::library::split_video_title(&metadata.title, uploader) else {
        return;
    };

    // Wann trägt der Titel den ganzen Upload-Namen?
    //
    // * Es gibt gar kein Titelfeld, dann steht alles im Videotitel.
    // * Es gibt keine Künstlerangabe. Beide Felder kommen aus derselben
    //   Auswertung: Fehlt die eine, ist die andere ebenfalls ungefiltert.
    // * Die linke Hälfte ist nachweislich der bereits bekannte Künstler.
    let ungefiltert = source.track.is_none()
        || source.artist.is_none()
        || bezeichnet_denselben(&artist, source.artist.as_deref().or(uploader));

    if ungefiltert {
        metadata.title = title;
    }
    if source.artist.is_none() {
        metadata.artist = artist;
    }
}

/// Endungen, die in Upload-Namen stehen bleiben.
const FILE_SUFFIXES: [&str; 8] = [
    ".mp3", ".wav", ".flac", ".m4a", ".ogg", ".opus", ".aac", ".aiff",
];

fn strip_file_suffix(title: &str) -> String {
    let lower = title.to_lowercase();
    for suffix in FILE_SUFFIXES {
        if let Some(rest) = lower.strip_suffix(suffix) {
            return title[..rest.len()].trim_end().to_string();
        }
    }
    title.to_string()
}

/// Meint `name` denselben Künstler wie die bekannte Angabe?
fn bezeichnet_denselben(name: &str, bekannt: Option<&str>) -> bool {
    bekannt.is_some_and(|bekannt| crate::online::looks_like_same(name, bekannt))
}

/// Sorgt dafür, dass die geladene Datei auch abspielbar ist.
///
/// Die Formatauswahl bevorzugt bereits passende Codecs. Manche Quellen bieten
/// aber ausschließlich Opus an, dann bleibt nur eine Umwandlung, sonst läge
/// ein stummer Titel in der Bibliothek.
async fn ensure_playable<R: Runtime>(
    app: &AppHandle<R>,
    job_id: &str,
    path: PathBuf,
) -> Result<PathBuf> {
    if is_playable(&path) {
        return Ok(path);
    }

    let format = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("unbekannt")
        .to_string();

    if !ffmpeg_available() {
        bail!(
            "Diese Quelle bietet den Titel nur als {format} an, was Robify nicht \
             abspielen kann. Für die Umwandlung wird ffmpeg benötigt."
        );
    }

    emit(
        app,
        status(
            job_id,
            "processing",
            99.0,
            Some(format!("{format} wird in ein abspielbares Format gebracht…")),
        ),
    );

    let target = path.with_extension("m4a");
    let args = vec![
        "-y".to_string(),
        "-loglevel".to_string(),
        "error".to_string(),
        "-i".to_string(),
        path.to_string_lossy().into_owned(),
        "-c:a".to_string(),
        "aac".to_string(),
        "-b:a".to_string(),
        "192k".to_string(),
        target.to_string_lossy().into_owned(),
    ];

    let ausgabe = tokio::time::timeout(CONVERT_TIMEOUT, ffmpeg_lassen(args))
        .await
        .map_err(|_| anyhow!(fehler!("Die Umwandlung von {0} dauert zu lange.", format)))??;
    if !ausgabe.erfolg || !target.exists() {
        bail!(
            "Umwandlung von {format} fehlgeschlagen: {}",
            ausgabe.stderr.trim()
        );
    }

    let _ = std::fs::remove_file(&path);
    Ok(target)
}

/// Ruft ffmpeg auf, gleich auf welchem System.
///
/// Auf dem Rechner ist es ein Programm im Suchpfad. Auf Android liegt es als
/// Bibliothek bei — dort gibt es kein `ffmpeg` zu finden, und ein eigener
/// Prozessstart scheiterte schon daran, dass Android das Ausführen außerhalb
/// des Bibliotheksordners nicht erlaubt.
#[cfg(not(target_os = "android"))]
async fn ffmpeg_lassen(args: Vec<String>) -> Result<crate::ytdlp::Ausgabe> {
    let mut cmd = Command::new("ffmpeg");
    cmd.args(&args).stdout(Stdio::null()).stderr(Stdio::piped());
    configure(&mut cmd);

    let ausgabe = cmd.output().await?;
    Ok(crate::ytdlp::Ausgabe {
        erfolg: ausgabe.status.success(),
        stdout: String::new(),
        stderr: String::from_utf8_lossy(&ausgabe.stderr).into_owned(),
    })
}

#[cfg(target_os = "android")]
async fn ffmpeg_lassen(args: Vec<String>) -> Result<crate::ytdlp::Ausgabe> {
    crate::ytdlp::ffmpeg(args).await
}

fn locate_output(result_file: &Path, job_dir: &Path) -> Result<PathBuf> {
    if let Ok(content) = std::fs::read_to_string(result_file) {
        if let Some(line) = content.lines().find(|l| !l.trim().is_empty()) {
            let candidate = PathBuf::from(line.trim());
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }

    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in std::fs::read_dir(job_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !tags::is_audio_file(&path) {
            continue;
        }
        let modified = entry.metadata()?.modified()?;
        if newest.as_ref().map(|(t, _)| modified > *t).unwrap_or(true) {
            newest = Some((modified, path));
        }
    }
    newest
        .map(|(_, p)| p)
        .ok_or_else(|| anyhow!(fehler!("Heruntergeladene Datei wurde nicht gefunden")))
}

#[cfg(test)]
mod tests {
    use super::{
        abzug_bei, apply_source_metadata, blocked_message, consensus_duration_ms,
        coverage_penalty, explain_failure,
        ist_youtube, is_collection_url, plans_with_fallbacks, rank_candidates, score_candidate,
        sort_by_relevance, version_penalty, SearchResult, TrackMetadata, ABSAGE_ABZUG,
    };

    /// Ein Treffer mit einer Adresse, an der die Quelle erkennbar ist.
    fn treffer_bei(titel: &str, sekunden: i64, adresse: &str) -> SearchResult {
        SearchResult {
            id: adresse.into(),
            title: titel.into(),
            uploader: None,
            duration_ms: Some(sekunden * 1000),
            url: adresse.into(),
            thumbnail: None,
            source: adresse.into(),
        }
    }

    #[test]
    fn youtube_wird_an_der_adresse_erkannt() {
        assert!(ist_youtube("https://www.youtube.com/watch?v=abc"));
        assert!(ist_youtube("https://youtu.be/abc"));
        assert!(ist_youtube("https://music.youtube.com/watch?v=abc"));
        assert!(!ist_youtube("https://soundcloud.com/wer/was"));
        assert!(!ist_youtube("https://kuenstler.bandcamp.com/track/was"));
    }

    /// Der Abschlag darf umsortieren, aber nichts Falsches nach vorn holen.
    ///
    /// Ein fehlendes Suchwort kostet 20. Läge der Abschlag darüber, gewänne
    /// ein SoundCloud-Treffer mit fremdem Titel gegen den richtigen Song bei
    /// YouTube — und ein falscher Titel in der Bibliothek fällt später kaum
    /// noch auf, ein gescheiterter Download dagegen sofort.
    #[test]
    fn abschlag_bleibt_unter_einem_fehlenden_wort() {
        // Gemessen statt behauptet: Was ein fehlendes Wort kostet, steht in
        // `coverage_penalty` und darf sich ändern, ohne dass dieser Test
        // stillschweigend nutzlos wird.
        let fremd = treffer_bei("Ganz was anderes", 199, "https://soundcloud.com/wer/was");
        let fehlendes_wort = coverage_penalty("Impact Prelude", &fremd);
        assert!(
            ABSAGE_ABZUG < fehlendes_wort,
            "Abschlag {ABSAGE_ABZUG} überholt einen fremden Titel ({fehlendes_wort})"
        );
    }

    #[test]
    fn ohne_absage_gibt_es_keinen_abschlag() {
        let youtube = "https://www.youtube.com/watch?v=abc";
        assert_eq!(abzug_bei(youtube, false), 0.0);
        assert_eq!(abzug_bei(youtube, true), ABSAGE_ABZUG);
        // Die übrigen Quellen trifft die Absage nie.
        assert_eq!(abzug_bei("https://soundcloud.com/wer/was", true), 0.0);
    }

    /// Die Ausweichliste soll die Quelle wechseln.
    ///
    /// Sagt YouTube ab, sagt es für alle seine Treffer ab. Vorher standen
    /// drei YouTube-Adressen in der Liste und endeten dreimal mit demselben
    /// 403, während der SoundCloud-Treffer unversucht danebenlag.
    #[test]
    fn die_ausweichliste_wechselt_zuerst_die_quelle() {
        let treffer = vec![
            treffer_bei("Impact Prelude", 199, "https://www.youtube.com/watch?v=eins"),
            treffer_bei("Impact Prelude", 199, "https://www.youtube.com/watch?v=zwei"),
            treffer_bei("Impact Prelude", 199, "https://soundcloud.com/wer/impact"),
        ];

        let plaene = plans_with_fallbacks(treffer);
        let erster = &plaene[0];
        assert_eq!(erster.url, "https://www.youtube.com/watch?v=eins");
        assert_eq!(
            erster.fallbacks.first().map(String::as_str),
            Some("https://soundcloud.com/wer/impact"),
            "die andere Quelle muss zuerst versucht werden: {:?}",
            erster.fallbacks
        );
    }

    #[test]
    fn abgelehnter_zugriff_wird_als_absage_erkannt() {
        // Genau der Weg, den `download` geht: erklärte Meldung, dann Prüfung.
        let meldung = explain_failure("ERROR: unable to download video data: HTTP Error 403: Forbidden\n");
        assert!(super::zugriff_verweigert(&anyhow::anyhow!(meldung)));
        assert!(!super::zugriff_verweigert(&anyhow::anyhow!(explain_failure(
            "ERROR: [youtube] abc: Private video"
        ))));
    }

    #[test]
    fn drm_meldung_wird_erklaert() {
        // Originalausgabe von yt-dlp: die Ursache steht nicht in der letzten Zeile.
        let stderr = "\
[youtube] AbCdEf: Some formats are drm protected
ERROR: [youtube] AbCdEf: This video is DRM protected
Please DO NOT open an issue, unless you have evidence that the video is not DRM protected
";
        let message = explain_failure(stderr);
        assert!(message.contains("kopiergeschützt"), "unerwartet: {message}");
        // Die Meldung muss den Ausweg nennen, nicht nur das Hindernis.
        assert!(message.contains("übrigen Treffer"), "kein Ausweg genannt: {message}");
        assert!(!message.contains("DO NOT open an issue"));
    }

    #[test]
    fn abgelehnter_zugriff_wird_erklaert() {
        let message =
            explain_failure("ERROR: unable to download video data: HTTP Error 403: Forbidden\n");
        assert!(message.contains("(403)"), "unerwartet: {message}");
        // Die Originalmeldung bleibt zur Nachvollziehbarkeit erhalten.
        assert!(message.contains("Meldung der Quelle"), "Details fehlen: {message}");

        assert!(explain_failure("ERROR: HTTP Error 429: Too Many Requests").contains("(403)"));
    }

    #[test]
    #[cfg(not(target_os = "android"))]
    fn fehlende_js_laufzeit_wird_als_ursache_genannt() {
        // Ohne Laufzeit ist das die eigentliche Ursache …
        assert!(blocked_message(false).contains("JavaScript-Laufzeit"));
        assert!(blocked_message(false).contains("Node.js"));
        // … mit Laufzeit bleibt nur die Drosselung als Erklärung.
        assert!(!blocked_message(true).contains("JavaScript-Laufzeit"));
        assert!(blocked_message(true).contains("yt-dlp -U"));
    }

    /// Auf Android taugt keiner der beiden Ratschläge.
    ///
    /// Node.js lässt sich dort nicht installieren, und `yt-dlp -U` greift ins
    /// Leere, weil yt-dlp aus der Bibliothek kommt. Genau das stand nach einem
    /// 403 auf dem Telefon: „Es ist keine JavaScript-Laufzeit installiert“ —
    /// obwohl QuickJS mitgeliefert wird und yt-dlp es benutzt.
    #[test]
    #[cfg(target_os = "android")]
    fn auf_android_wird_nichts_zum_installieren_geraten() {
        for mit_laufzeit in [true, false] {
            let meldung = blocked_message(mit_laufzeit);
            assert!(meldung.contains("(403)"));
            assert!(!meldung.contains("Node.js"));
            assert!(!meldung.contains("yt-dlp -U"));
        }
    }

    #[test]
    fn ziffernfolgen_loesen_keine_falsche_sperrmeldung_aus() {
        // „403“ steckt hier in einer Kennung, nicht in einem HTTP-Fehler.
        let stderr = "ERROR: [soundcloud] 403291102: Requested format is not available\n";
        let message = explain_failure(stderr);
        assert!(
            message.contains("nur eine Vorschau"),
            "falsch eingeordnet: {message}"
        );
        assert!(!message.contains("vorübergehend gesperrt"));

        // Auch eine Bytezahl mit 429 darf nichts auslösen.
        let stderr = "ERROR: [generic] xyz: Kaputt nach 4291 Bytes\n";
        assert!(!explain_failure(stderr).contains("vorübergehend gesperrt"));
    }

    #[test]
    fn veraltetes_ytdlp_bekommt_update_hinweis() {
        let stderr = "ERROR: [youtube] xyz: Unable to extract player response\n";
        assert!(explain_failure(stderr).contains("yt-dlp -U"));
    }

    /// Vorlage und Wert reisen getrennt, damit die Oberfläche übersetzen kann.
    ///
    /// Die Zeile von yt-dlp selbst bleibt unangetastet: Sie ist Englisch und
    /// stammt nicht von uns.
    #[test]
    fn unbekannter_fehler_zeigt_die_error_zeile() {
        let stderr = "[debug] irgendwas\nERROR: [generic] xyz: Kaputt\n";
        let teile: Vec<String> = explain_failure(stderr)
            .split(crate::meldung::TRENNER)
            .map(str::to_string)
            .collect();
        assert_eq!(teile, vec!["yt-dlp: {0}", "[generic] xyz: Kaputt"]);
    }

    /// Erklärung und Quellmeldung bleiben zwei nachschlagbare Sätze.
    #[test]
    fn erklaerung_und_quellmeldung_stehen_getrennt() {
        let stderr = "ERROR: [youtube] xyz: Private video\n";
        let ganz = explain_failure(stderr);
        assert!(
            !ganz.contains(crate::meldung::ABSATZ),
            "ohne Zusatzangabe braucht es keinen zweiten Absatz: {ganz}"
        );

        let mit_grund = explain_failure("ERROR: [youtube] xyz: Unable to extract player response\n");
        let absaetze: Vec<&str> = mit_grund.split(crate::meldung::ABSATZ).collect();
        assert_eq!(absaetze.len(), 2, "Erklärung und Quellmeldung: {mit_grund}");
        assert_eq!(
            absaetze[1].split(crate::meldung::TRENNER).next().unwrap(),
            "Meldung der Quelle: {0}"
        );
    }

    fn treffer(title: &str, seconds: i64, source: &str) -> SearchResult {
        SearchResult {
            id: title.into(),
            title: title.into(),
            uploader: None,
            duration_ms: Some(seconds * 1000),
            url: format!("https://example.test/{source}/{title}"),
            thumbnail: None,
            source: source.into(),
        }
    }

    #[test]
    fn waehlt_den_treffer_mit_passender_laenge() {
        let expected = 167_000;
        let original = treffer("Die Welt zu Gast bei Feinden", 167, "YouTube");
        let daneben = treffer("Die Welt zu Gast bei Feinden", 172, "YouTube");

        let a = score_candidate(&original, "Die Welt zu Gast bei Feinden", Some(expected), "Die Welt zu Gast bei Feinden").unwrap();
        let b = score_candidate(&daneben, "Die Welt zu Gast bei Feinden", Some(expected), "Die Welt zu Gast bei Feinden").unwrap();
        assert!(a < b, "die genauere Länge muss gewinnen");
    }

    #[test]
    fn verwirft_deutlich_abweichende_laengen() {
        // Ein Mix von zehn Minuten ist nicht derselbe Titel.
        let langer_mix = treffer("Die Welt zu Gast bei Feinden Mix", 600, "YouTube");
        assert!(score_candidate(&langer_mix, "Die Welt zu Gast bei Feinden", Some(167_000), "Die Welt zu Gast bei Feinden").is_none());

        // Auch knapp jenseits der Grenze wird verworfen.
        let knapp = treffer("Die Welt zu Gast bei Feinden", 167 + 31, "YouTube");
        assert!(score_candidate(&knapp, "Die Welt zu Gast bei Feinden", Some(167_000), "Die Welt zu Gast bei Feinden").is_none());

        // Fehlende Laufzeiten deckt `unbekannte_laenge_wird_abgewertet_aber_zugelassen` ab.
    }

    #[test]
    fn straft_andere_fassungen_ab() {
        let expected = 167_000;
        let original = treffer("Die Welt zu Gast bei Feinden", 167, "YouTube");
        let remix = treffer("Die Welt zu Gast bei Feinden (Remix)", 167, "YouTube");
        let live = treffer("Die Welt zu Gast bei Feinden (Live)", 167, "YouTube");

        let base = score_candidate(&original, "Die Welt zu Gast bei Feinden", Some(expected), "Die Welt zu Gast bei Feinden").unwrap();
        for andere in [&remix, &live] {
            let score = score_candidate(andere, "Die Welt zu Gast bei Feinden", Some(expected), "Die Welt zu Gast bei Feinden").unwrap();
            assert!(score > base + 20.0, "Zusatz wurde nicht abgestraft: {}", andere.title);
        }
    }

    #[test]
    fn gewollte_fassungen_bleiben_unbestraft() {
        // Wer einen Remix sucht, soll ihn auch bekommen.
        let remix = treffer("Song (Remix)", 200, "YouTube");
        let a = score_candidate(&remix, "Song (Remix)", Some(200_000), "Song (Remix)").unwrap();
        let b = score_candidate(&remix, "Song", Some(200_000), "Song").unwrap();
        assert!(a < b);
    }

    #[test]
    fn bessere_quellen_haben_bei_gleichstand_vorrang() {
        // Bei identischer Länge entscheidet die Verlässlichkeit der Quelle.
        let reihenfolge = ["Bandcamp", "Audius", "SoundCloud", "YouTube"];
        let bewertungen: Vec<f64> = reihenfolge
            .iter()
            .map(|quelle| {
                score_candidate(&treffer("Song", 200, quelle), "Song", Some(200_000), "Song").unwrap()
            })
            .collect();

        for paar in bewertungen.windows(2) {
            assert!(paar[0] < paar[1], "Reihenfolge stimmt nicht: {bewertungen:?}");
        }
    }

    #[test]
    fn unbekannte_laenge_wird_abgewertet_aber_zugelassen() {
        // Bandcamp nennt keine Laufzeit, der Treffer bleibt trotzdem nutzbar.
        let mut ohne = treffer("Song", 0, "Bandcamp");
        ohne.duration_ms = None;
        let bandcamp = score_candidate(&ohne, "Song", Some(200_000), "Song")
            .expect("darf nicht ausgeschlossen werden");

        // Ein nachweislich passender Treffer gewinnt trotzdem.
        let genau = treffer("Song", 200, "YouTube");
        assert!(score_candidate(&genau, "Song", Some(200_000), "Song").unwrap() < bandcamp);
    }

    #[test]
    fn ein_fremder_titel_verliert_gegen_den_gesuchten() {
        // Der Kern des Fehlers: ohne Textvergleich gewann allein die Quelle,
        // und Bandcamp lieferte einen ganz anderen Song.
        let fremd = treffer("Ganz anderer Song", 200, "Bandcamp");
        let richtig = treffer("Tropical Island", 200, "YouTube");

        let a = score_candidate(&richtig, "PA69 Tropical Island", Some(200_000), "Tropical Island")
            .unwrap();
        let b = score_candidate(&fremd, "PA69 Tropical Island", Some(200_000), "Tropical Island")
            .unwrap();
        assert!(a < b, "fremder Titel gewann: {a} vs {b}");
    }

    #[test]
    fn remix_eines_fremden_titels_landet_hinten() {
        // Genau der gemeldete Fall: ein Remix eines anderen Songs.
        let gesucht = "PA69 Tropical Island";
        let treffer_liste = [
            treffer("Irgendwas anderes (PA69 Remix)", 200, "Bandcamp"),
            treffer("Tropical Island", 200, "YouTube"),
        ];

        let reihenfolge = rank_candidates(&treffer_liste, gesucht, Some(200_000), "Tropical Island");
        assert_eq!(reihenfolge.first().map(String::as_str), Some(treffer_liste[1].url.as_str()));
    }

    #[test]
    fn ohne_brauchbare_laenge_bleibt_die_textnaehe_massgeblich() {
        // Alle Treffer fallen durch die Längenprüfung, dann darf nicht
        // einfach der erste genommen werden.
        let treffer_liste = [
            treffer("Ganz anderer Song", 600, "Bandcamp"),
            treffer("Tropical Island", 600, "YouTube"),
        ];

        let reihenfolge = rank_candidates(&treffer_liste, "Tropical Island", Some(200_000), "Tropical Island");
        assert_eq!(reihenfolge.first().map(String::as_str), Some(treffer_liste[1].url.as_str()));
    }

    #[test]
    fn kuenstler_im_kanalnamen_zaehlt_mit() {
        // SoundCloud nennt den Künstler oft nur im Kanal, nicht im Titel.
        let mut vom_kuenstler = treffer("Tropical Island", 200, "SoundCloud");
        vom_kuenstler.uploader = Some("PA69".into());
        let fremd = treffer("Tropical Island", 200, "SoundCloud");

        let query = "PA69 Tropical Island";
        let a = score_candidate(&vom_kuenstler, query, Some(200_000), "Tropical Island").unwrap();
        let b = score_candidate(&fremd, query, Some(200_000), "Tropical Island").unwrap();
        assert!(a < b, "Kanalname wurde nicht berücksichtigt");
    }

    #[test]
    fn passende_treffer_stehen_in_der_liste_oben() {
        // So kam die Liste bisher an: reihum gemischt, ohne Ordnung.
        let mut liste = vec![
            treffer("Ganz anderer Song", 200, "Bandcamp"),
            treffer("Creep", 30, "SoundCloud"),
            treffer("Radiohead - Creep", 238, "YouTube"),
        ];
        sort_by_relevance(&mut liste, "Radiohead Creep");

        let titel: Vec<&str> = liste.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titel[0], "Radiohead - Creep", "Reihenfolge: {titel:?}");
        // Ein 30-Sekunden-Ausschnitt ist kein guter erster Vorschlag …
        assert_eq!(titel[1], "Creep", "Reihenfolge: {titel:?}");
        // … ein fremder Titel aber ein noch schlechterer.
        assert_eq!(titel[2], "Ganz anderer Song", "Reihenfolge: {titel:?}");
    }

    #[test]
    fn fehlgriffe_werden_gemeldet() {
        use super::intent_warning;
        let julia = TrackMetadata {
            title: "Julia".into(),
            artist: "Julia".into(),
            ..Default::default()
        };
        // Der beobachtete Fall: gesucht war „The Killers Mr. Brightside“.
        let warnung = intent_warning(Some("The Killers Mr. Brightside"), &julia, true)
            .expect("Fehlgriff blieb unbemerkt");
        assert!(warnung.contains("Mr. Brightside"), "Suche fehlt: {warnung}");
        assert!(warnung.contains("Julia"), "Ergebnis fehlt: {warnung}");

        // Der zweite beobachtete Fall: Der Künstlername steckte im Titel,
        // deshalb schienen alle gesuchten Wörter vorhanden, das Künstlerfeld
        // trug aber den Kanalnamen.
        let getarnt = TrackMetadata {
            title: "The Killers- Mr. Brightside".into(),
            artist: "Julia".into(),
            ..Default::default()
        };
        assert!(
            intent_warning(Some("The Killers Mr. Brightside"), &getarnt, true).is_some(),
            "Künstler im Titel verdeckte den Fehlgriff"
        );

        // Bestätigt die Metadatensuche den Fund, zählt das schwächere
        // Anzeichen nicht mehr.
        assert!(intent_warning(Some("The Killers Mr. Brightside"), &getarnt, false).is_none());
    }

    #[test]
    fn passende_treffer_loesen_keine_warnung_aus() {
        use super::intent_warning;
        let treffer = TrackMetadata {
            title: "Mr. Brightside".into(),
            artist: "The Killers".into(),
            ..Default::default()
        };
        assert!(intent_warning(Some("The Killers Mr. Brightside"), &treffer, true).is_none());

        // Gastkünstler zählen mit.
        let mit_gast = TrackMetadata {
            title: "Money Trees".into(),
            artist: "Kendrick Lamar".into(),
            featured_artists: Some("Jay Rock".into()),
            ..Default::default()
        };
        assert!(intent_warning(Some("Kendrick Lamar Money Trees Jay Rock"), &mit_gast, true).is_none());

        // Ein einzelnes abweichendes Wort ist Alltag, kein Fehlgriff.
        let fast = TrackMetadata {
            title: "Naked".into(),
            artist: "Yeat".into(),
            ..Default::default()
        };
        assert!(intent_warning(Some("Yeat Naked Official"), &fast, true).is_none());

        // Wer nur den Titel sucht, kennt den Künstler vielleicht nicht,
        // solange die Metadatensuche zustimmt, ist das kein Fehlgriff.
        let nur_titel = TrackMetadata {
            title: "Wildberry Lillet".into(),
            artist: "Nina Chuba".into(),
            ..Default::default()
        };
        assert!(intent_warning(Some("Wildberry Lillet"), &nur_titel, false).is_none());

        // Ohne Absicht gibt es nichts zu prüfen.
        assert!(intent_warning(None, &fast, true).is_none());
        assert!(intent_warning(Some("   "), &fast, true).is_none());
    }

    #[test]
    fn aufraeumen_trifft_nur_alte_auftragsordner() {
        use std::time::Duration;
        let basis = std::env::temp_dir().join(format!("robify-aufraeumtest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&basis);

        let alt = basis.join("job-1-1");
        let behalten = basis.join(super::super::commands::KEEP_DIR);
        let fremd = basis.join("etwas-anderes");
        for ordner in [&alt, &behalten, &fremd] {
            std::fs::create_dir_all(ordner).unwrap();
            std::fs::write(ordner.join("datei.txt"), b"x").unwrap();
        }

        // Mit sehr großer Altersgrenze bleibt alles stehen.
        assert_eq!(super::cleanup_work_dir(&basis, Duration::from_secs(3600)), 0);
        assert!(alt.exists());

        // Mit Grenze Null gilt der Auftragsordner als alt, die anderen nicht.
        assert_eq!(super::cleanup_work_dir(&basis, Duration::ZERO), 1);
        assert!(!alt.exists(), "Auftragsordner blieb liegen");
        assert!(behalten.exists(), "übernommene Titel wurden gelöscht");
        assert!(fremd.exists(), "fremder Ordner wurde gelöscht");

        let _ = std::fs::remove_dir_all(&basis);
    }

    #[test]
    fn adressen_werden_ihrer_quelle_zugeordnet() {
        // Grundlage des Mindestabstands: Zugriffe auf denselben Dienst
        // müssen als solche erkannt werden, egal in welcher Schreibweise.
        use super::source_label;
        assert_eq!(source_label("https://music.youtube.com/watch?v=x"), "YouTube Music");
        assert_eq!(source_label("https://www.youtube.com/watch?v=x"), "YouTube");
        assert_eq!(source_label("https://youtu.be/x"), "YouTube");
        assert_eq!(source_label("ytsearch5:etwas"), "YouTube");
        assert_eq!(source_label("scsearch1:etwas"), "SoundCloud");
        assert_eq!(source_label("https://soundcloud.com/a/b"), "SoundCloud");
        assert_eq!(source_label("https://band.bandcamp.com/track/x"), "Bandcamp");
        assert_eq!(source_label("https://api.audius.co/v1/tracks/x/stream"), "Audius");
        assert_eq!(source_label("https://example.test/lied.mp3"), "sonstige");
    }

    #[tokio::test]
    async fn die_lastbremse_zieht_zugriffe_auseinander() {
        use std::time::Instant;
        let start = Instant::now();
        // Zwei Zugriffe auf dieselbe Quelle nacheinander.
        {
            let _a = super::acquire_slot("Testquelle").await;
        }
        {
            let _b = super::acquire_slot("Testquelle").await;
        }
        assert!(
            start.elapsed() >= super::MIN_SPACING,
            "der zweite Zugriff kam zu früh: {:?}",
            start.elapsed()
        );

        // Verschiedene Quellen bremsen sich nicht gegenseitig aus.
        let start = Instant::now();
        let _c = super::acquire_slot("AndereQuelle").await;
        assert!(start.elapsed() < super::MIN_SPACING);
    }

    #[test]
    fn quellenangaben_haben_vorrang_vor_dem_videotitel() {
        let mut metadata = TrackMetadata {
            // So schreibt yt-dlp es ohne Musikfelder in die Datei.
            title: "Nina Chuba - WILDBERRY LILLET [Lyrics]".into(),
            artist: "xTheLYRICS".into(),
            ..Default::default()
        };

        // 1) Mit Musikfeldern zählen nur diese.
        let quelle = super::SourceMetadata::parse("Naked\u{1f}Yeat\u{1f}ADL\u{1f}2026\u{1f}3");
        apply_source_metadata(&mut metadata, &quelle, Some("Yeat"));
        assert_eq!(metadata.title, "Naked");
        assert_eq!(metadata.artist, "Yeat");
        assert_eq!(metadata.album, "ADL");
        assert_eq!(metadata.year, Some(2026));
        assert_eq!(metadata.track_no, Some(3));
    }

    #[test]
    fn upload_namen_im_musikfeld_werden_zerlegt() {
        // SoundCloud füllt `track` mit dem Dateinamen des Uploads. Im großen
        // Messlauf stand deshalb „Aphex Twin. Xtal.mp3“ als Songtitel in der
        // Bibliothek, und die Metadatensuche fand dazu nichts.
        let mut metadata = TrackMetadata::default();
        let quelle = super::SourceMetadata::parse(
            "Aphex Twin – Xtal.mp3\u{1f}Aphex Twin\u{1f}NA\u{1f}NA\u{1f}NA",
        );
        apply_source_metadata(&mut metadata, &quelle, Some("Aphex Twin"));
        assert_eq!(metadata.title, "Xtal");
        assert_eq!(metadata.artist, "Aphex Twin");

        // Ohne Künstlerangabe genügt der Kanalname als Beleg.
        let mut metadata = TrackMetadata::default();
        let quelle = super::SourceMetadata::parse(
            "Boards of Canada - Roygbiv\u{1f}NA\u{1f}NA\u{1f}NA\u{1f}NA",
        );
        apply_source_metadata(&mut metadata, &quelle, Some("Boards of Canada"));
        assert_eq!(metadata.title, "Roygbiv");
        assert_eq!(metadata.artist, "Boards of Canada");
    }

    #[test]
    fn titel_mit_bindestrich_bleiben_unangetastet() {
        // Ein Songtitel darf einen Trenner enthalten. Zerlegt wird nur, wenn
        // die linke Hälfte nachweislich der Künstler ist.
        let mut metadata = TrackMetadata::default();
        let quelle = super::SourceMetadata::parse(
            "Sunday Bloody Sunday - Live\u{1f}U2\u{1f}NA\u{1f}NA\u{1f}NA",
        );
        apply_source_metadata(&mut metadata, &quelle, Some("U2"));
        assert_eq!(metadata.title, "Sunday Bloody Sunday - Live");
        assert_eq!(metadata.artist, "U2");
    }

    #[test]
    fn ohne_quellenangaben_wird_der_videotitel_zerlegt() {
        let mut metadata = TrackMetadata {
            title: "Nina Chuba - WILDBERRY LILLET [Lyrics]".into(),
            artist: "xTheLYRICS".into(),
            ..Default::default()
        };

        // Lyric-Kanäle liefern keine Musikfelder, alles steht im Titel.
        let leer = super::SourceMetadata::parse("NA\u{1f}NA\u{1f}NA\u{1f}NA\u{1f}NA");
        assert_eq!(leer, super::SourceMetadata::default());

        apply_source_metadata(&mut metadata, &leer, Some("xTheLYRICS"));
        assert_eq!(metadata.title, "WILDBERRY LILLET");
        assert_eq!(metadata.artist, "Nina Chuba", "Kanalname wurde übernommen");
    }

    #[test]
    fn ausweichadressen_bleiben_beim_selben_titel() {
        // „Yeat Naked“ wich bis auf „Back Home“ aus, ein anderer Song.
        let plaene = super::plans_with_fallbacks(vec![
            treffer("Naked", 93, "YouTube Music"),
            treffer("Naked", 94, "YouTube"),
            treffer("Back Home", 194, "YouTube Music"),
        ]);

        assert_eq!(plaene[0].fallbacks, vec![plaene[1].url.clone()]);
        assert!(
            plaene[2].fallbacks.is_empty(),
            "fremder Titel bekam Ausweichadressen"
        );
    }

    #[test]
    fn derselbe_song_zaehlt_auch_mit_kuenstler_im_titel() {
        // Quellen schreiben denselben Titel unterschiedlich, der Ersatz darf
        // daran nicht scheitern, sonst bleibt ein DRM-Titel ohne Ausweg.
        let plaene = super::plans_with_fallbacks(vec![
            treffer("BIRDS OF A FEATHER", 210, "SoundCloud"),
            treffer("Billie Eilish - BIRDS OF A FEATHER", 210, "YouTube"),
            treffer("BIRDS OF A FEATHER (sped up)", 180, "YouTube"),
        ]);

        assert_eq!(
            plaene[0].fallbacks,
            vec![plaene[1].url.clone()],
            "Schreibweise mit Künstler wurde nicht erkannt"
        );
        // Eine andere Fassung ist kein Ersatz.
        assert!(!plaene[0].fallbacks.contains(&plaene[2].url));
    }

    #[test]
    fn gleicher_name_bei_anderer_laenge_ist_kein_ersatz() {
        // „Naked“ gibt es von Yeat (93 s) und von Kraak & Smaak. Ohne
        // Längenprüfung landete der falsche Song in der Bibliothek.
        let plaene = super::plans_with_fallbacks(vec![
            treffer("Naked", 93, "YouTube Music"),
            treffer("Naked", 214, "SoundCloud"),
        ]);
        assert!(plaene[0].fallbacks.is_empty(), "fremder Song als Ersatz");

        // Ohne Laufzeit lässt sich nichts ausschließen. Bandcamp nennt keine.
        let mut ohne_laufzeit = treffer("Naked", 0, "Bandcamp");
        ohne_laufzeit.duration_ms = None;
        let plaene = super::plans_with_fallbacks(vec![
            treffer("Naked", 93, "YouTube Music"),
            ohne_laufzeit,
        ]);
        assert!(
            plaene[0].fallbacks.is_empty(),
            "ungeprüfter Treffer wurde als Ersatz zugelassen"
        );
    }

    #[test]
    fn jeder_treffer_bekommt_die_uebrigen_als_ausweg() {
        // Ohne das bleibt ein DRM-geschützter Treffer ein Sackgassen-Download.
        // Derselbe Titel bei drei Quellen, genau der DRM-Fall.
        let plaene = super::plans_with_fallbacks(vec![
            treffer("Song", 200, "SoundCloud"),
            treffer("Song", 200, "YouTube Music"),
            treffer("Song", 200, "YouTube"),
        ]);

        assert_eq!(plaene.len(), 3);
        for plan in &plaene {
            assert_eq!(plan.fallbacks.len(), 2, "zu wenige Auswege");
            assert!(
                !plan.fallbacks.contains(&plan.url),
                "ein Treffer weicht auf sich selbst aus"
            );
        }
        // Die Reihenfolge der Auswege folgt der Bewertung.
        assert_eq!(plaene[0].fallbacks[0], plaene[1].url);
    }

    #[test]
    fn youtube_music_steht_vor_den_uebrigen_quellen() {
        // Dort liegt die Veröffentlichung des Künstlers, nicht die
        // Nachbearbeitung eines Dritten.
        let reihenfolge = ["YouTube Music", "Bandcamp", "Audius", "SoundCloud", "YouTube"];
        let bewertungen: Vec<f64> = reihenfolge
            .iter()
            .map(|quelle| {
                score_candidate(&treffer("Song", 200, quelle), "Song", Some(200_000), "Song")
                    .unwrap()
            })
            .collect();

        for paar in bewertungen.windows(2) {
            assert!(paar[0] < paar[1], "Reihenfolge stimmt nicht: {bewertungen:?}");
        }
    }

    #[test]
    fn fehlende_bibliothek_wird_erklaert() {
        let stderr = "\
[ExtractAudio] Destination: Naked.opus
ERROR: Postprocessing: module mutagen was not found. Please install using `python3 -m pip install mutagen`
";
        let message = explain_failure(stderr);
        assert!(message.contains("mutagen"), "unerwartet: {message}");
        assert!(message.contains("Cover"), "Zusammenhang fehlt: {message}");
        // Kein Grund, hier von einer Sperre zu sprechen.
        assert!(!message.contains("403"), "falsch eingeordnet: {message}");
    }

    #[test]
    fn zusaetze_in_klammern_zaehlen_mit() {
        // Der Messlauf zeigte: Fremdfassungen stehen fast immer in Klammern,
        // und genau die blendet der Wortabgleich aus.
        let fremd = [
            "Money Trees (Kolosal Cover)",
            "SICKO MODE (HYLO EDIT)",
            "Blinding Lights [Remix Rodrics]",
            "Wildberry Lillet (Raffi Balboa Edit)",
        ];
        for titel in fremd {
            assert!(
                version_penalty(titel, "Money Trees") > 0.0,
                "nicht erkannt: {titel}"
            );
        }

        // Nur ganze Wörter: „edit“ steckt auch in „Editors“ und „Credits“.
        assert_eq!(version_penalty("Editors - Munich", "Munich"), 0.0);
        assert_eq!(version_penalty("Song (Credits Version)", "Song"), 0.0);

        // Wer die Fassung sucht, bekommt sie.
        assert_eq!(version_penalty("Song (Remix)", "Song (Remix)"), 0.0);
    }

    #[test]
    fn mehrheit_der_quellen_bestimmt_die_laenge() {
        // Vier Quellen führen dieselbe Aufnahme, eine einen Ausschnitt.
        let liste = [
            treffer("Money Trees", 91, "SoundCloud"),
            treffer("Money Trees", 387, "YouTube"),
            treffer("Money Trees", 395, "SoundCloud"),
            treffer("Money Trees", 387, "YouTube"),
        ];
        let consensus = consensus_duration_ms(&liste).expect("Mehrheit gefunden");
        assert!(
            (consensus - 387_000).abs() < 10_000,
            "unerwartete Länge: {consensus} ms"
        );

        // Zu wenige Angaben: lieber keine Aussage als eine schlechte.
        assert!(consensus_duration_ms(&liste[..2]).is_none());
    }

    #[test]
    fn angeschnittene_uploads_verlieren_gegen_die_mehrheit() {
        // Der gemeldete Fall aus dem Messlauf: ein 91-Sekunden-Upload stand
        // über der vollständigen Aufnahme.
        let mut liste = vec![
            treffer("Kendrick Lamar - Money Trees", 91, "SoundCloud"),
            treffer("Money Trees", 387, "YouTube"),
            treffer("Kendrick Lamar - Money Trees", 395, "SoundCloud"),
            treffer("Kendrick Lamar - Money Trees", 387, "YouTube"),
        ];
        sort_by_relevance(&mut liste, "Kendrick Lamar Money Trees");

        assert!(
            liste[0].duration_ms.unwrap() > 300_000,
            "oben steht immer noch ein Ausschnitt: {:?}",
            liste.iter().map(|t| t.duration_ms).collect::<Vec<_>>()
        );
    }

    #[test]
    fn gleichwertige_treffer_behalten_die_quellenmischung() {
        // Bei gleicher Passgenauigkeit bleibt die Reihenfolge, wie sie das
        // Mischen erzeugt hat, die Vielfalt geht nicht verloren.
        let mut liste = vec![
            treffer("Song", 200, "Bandcamp"),
            treffer("Song", 200, "Audius"),
            treffer("Song", 200, "YouTube"),
        ];
        sort_by_relevance(&mut liste, "Song");

        let quellen: Vec<&str> = liste.iter().map(|t| t.source.as_str()).collect();
        assert_eq!(quellen, ["Bandcamp", "Audius", "YouTube"]);
    }

    #[test]
    fn sammlungen_werden_nur_bei_echten_listen_aufgeklappt() {
        // Einzelne Titel bleiben einzeln, auch mit Playlist-Parameter.
        assert!(!is_collection_url("https://www.youtube.com/watch?v=abc"));
        assert!(!is_collection_url("https://www.youtube.com/watch?v=abc&list=PL123"));
        assert!(!is_collection_url("https://youtu.be/abc"));
        assert!(!is_collection_url("https://soundcloud.com/kuenstler/titel"));

        // Echte Sammlungen werden aufgeklappt.
        assert!(is_collection_url("https://www.youtube.com/playlist?list=PL123"));
        assert!(is_collection_url("https://soundcloud.com/kuenstler/sets/mein-album"));
        assert!(is_collection_url("https://band.bandcamp.com/album/mein-album"));
    }

    #[test]
    fn leere_ausgabe_stuerzt_nicht_ab() {
        assert!(explain_failure("").contains("unbekannter Fehler"));
    }
}
