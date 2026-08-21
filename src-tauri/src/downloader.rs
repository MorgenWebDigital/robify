//! downloader built on `yt-dlp`. the user decides the source: either a
//! direct url, yt-dlp supports a great many portals, or a search in one of
//! the search providers.
//!
//! note: everything is downloaded into a working directory, and the file
//! moves into the library only once the user has confirmed the metadata.

use crate::models::TrackMetadata;
use crate::tags;
use anyhow::{anyhow, bail, Result};
use base64::Engine;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
// only the route through a process of its own needs them, on android yt-dlp
// and ffmpeg run over the java bridge
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

/// registry of running jobs, so downloads can be cancelled
#[derive(Default)]
pub struct DownloadRegistry {
    cancels: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl DownloadRegistry {
    /// registers a job. where the id is already running, its flag is kept:
    /// otherwise a cancellation would silently lose its effect by pointing at
    /// the replaced flag.
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
    /// the music service behind youtube. its search delivers the official
    /// release rather than lyric videos and fan uploads.
    YoutubeMusic,
    Soundcloud,
    /// artists upload there themselves, mostly complete and in good quality
    Bandcamp,
    /// open music platform, delivers mp3 at up to 320 kbit/s
    Audius,
    /// the input is a url already and goes to yt-dlp directly
    Url,
}

/// sources queried at the same time on a text search
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
            // these are searched through their own interfaces
            SearchSource::Bandcamp | SearchSource::Audius | SearchSource::Url => input.to_string(),
        }
    }

    /// youtube music names running times on a full query only. that takes
    /// longer and is worth the detail: without it neither an excerpt could be
    /// recognised nor the majority length formed.
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

    /// deduction on the score, the larger the more reliably the source
    /// delivers the whole track in good quality.
    ///
    /// deliberately kept small: a measured run over 15 tracks showed bandcamp
    /// delivering mostly foreign rework of well-known songs. the source may
    /// tip the scale where everything else is equal, but never win against
    /// the better matching track.
    ///
    /// youtube music comes first because the artist's release lies there, not
    /// a third party's rework.
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

/// searches bandcamp, which offers an open search without credentials.
///
/// it does not supply running times though.
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
        // "t" stands for a single track
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

/// searches audius, which is openly reachable and names the running time,
/// which makes finding the matching recording particularly reliable.
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

/// an entry that can be downloaded, either directly over a url or over a
/// search where the source supplies no audio data itself.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadPlan {
    /// goes to yt-dlp unchanged, with spotify it is a search expression
    pub url: String,
    /// fallback addresses where the first source delivers nothing
    #[serde(default)]
    pub fallbacks: Vec<String>,
    /// search term the best hit is determined from
    #[serde(default)]
    pub match_query: Option<String>,
    /// what the user searched for. serves the check after downloading only,
    /// unlike `match_query` it triggers no new search.
    #[serde(default)]
    pub intent: Option<String>,
    pub title: String,
    pub subtitle: Option<String>,
    pub thumbnail: Option<String>,
    pub duration_ms: Option<i64>,
    pub source: String,
    /// metadata known beforehand, which wins over the tags of the file
    pub metadata: Option<TrackMetadata>,
    /// whether a track of the same name by the same artist lies in the
    /// library already. the entry can be skipped in a batch then.
    #[serde(default)]
    pub already_in_library: bool,
}

/// a hint the ui puts into words itself
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanHinweis {
    /// which hint. the ui holds the wording in every language.
    pub code: String,
    /// values in the order of the placeholders `{0}`, `{1}` and so on
    pub args: Vec<String>,
}

/// what sits behind a pasted link
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkPlan {
    pub label: String,
    pub kind: String,
    /// hints for the ui, as an id rather than a finished sentence.
    ///
    /// the rust side does not know the selected interface language, it lives
    /// in the frontend. a sentence assembled here would therefore arrive in
    /// german whatever the language, and that is exactly how the spotify hint
    /// stood in the russian version too.
    pub notes: Vec<PlanHinweis>,
    /// whether the entries belong together, an album or a playlist. then
    /// downloading all of them makes sense, with search hits it does not.
    pub batch: bool,
    pub items: Vec<DownloadPlan>,
}

// whether one hit serves as a substitute for another.
//
// a plain comparison of names does not do: the same recording is called
// "BIRDS OF A FEATHER" at one source and "Billie Eilish - BIRDS OF A FEATHER"
// at the next. it is therefore enough for one title to sit inside the other
// as a word sequence, as long as no different version is announced
fn same_song(candidate: &SearchResult, wanted: &SearchResult) -> bool {
    let a = crate::online::normalize_for_match(&candidate.title);
    let b = crate::online::normalize_for_match(&wanted.title);
    if a.is_empty() || b.is_empty() {
        return false;
    }
    let gleicher_name = crate::online::contains_word_sequence(&a, &b)
        || crate::online::contains_word_sequence(&b, &a);

    // the same name does not mean the same song: "Naked" exists by yeat (93 s)
    // and by kraak & smaak. the running time separates the two, and where it
    // is missing the hit does not serve as a substitute. used unchecked, the
    // wrong song landed in the library, and that is hard to notice later
    let gleiche_laenge = match (candidate.duration_ms, wanted.duration_ms) {
        (Some(a), Some(b)) => a.saturating_sub(b).saturating_abs() <= SAME_TAKE_MS,
        _ => false,
    };

    gleicher_name && gleiche_laenge && version_penalty(&candidate.title, &wanted.title) == 0.0
}

/// turns search hits into download jobs and hangs the remaining hits onto
/// each of them as fallback addresses.
///
/// needed because some obstacles show only while downloading: soundcloud
/// hands out the label uploads of well-known artists drm-protected, and that
/// stands in no search result. a measured run over 30 tracks hit it on eight
/// of them. without a fallback chain it would stop at "not possible" although
/// the same recording stands ready at another source.
pub fn plans_with_fallbacks(found: Vec<SearchResult>) -> Vec<DownloadPlan> {
    let alle = found.clone();
    found
        .into_iter()
        .map(|treffer| {
            // hits meaning the same track only. the whole result list stood
            // here before, and with "Yeat Naked" the download strayed all the
            // way to "Back Home", an entirely different song
            let mut ausweich: Vec<&SearchResult> = alle
                .iter()
                .filter(|andere| andere.url != treffer.url && same_song(andere, &treffer))
                .collect();
            // a different source first, then the rest of the same one.
            //
            // where a source refuses, it usually refuses for all of its hits:
            // at youtube three fallback addresses ended three times in the
            // same 403 while the soundcloud hit lay untried next to them. the
            // sort is stable, so the order by fit is kept inside each group
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
    /// "mp3", "opus", "flac", "m4a", "vorbis" or "best" (no conversion)
    #[serde(default = "default_format")]
    pub format: String,
    /// 0 is the best quality up to 9, relevant with lossy formats only
    #[serde(default)]
    pub quality: Option<String>,
    #[serde(default = "default_true")]
    pub embed_thumbnail: bool,
    /// metadata known already, from a spotify link for instance. it wins over
    /// whatever stands in the downloaded file.
    #[serde(default)]
    pub metadata: Option<TrackMetadata>,
    /// where `url` finds nothing, these addresses are tried in order
    #[serde(default)]
    pub fallbacks: Vec<String>,
    /// search online for matching metadata after downloading
    #[serde(default = "default_true")]
    pub auto_match: bool,
    #[serde(default = "default_true")]
    pub auto_cover: bool,
    #[serde(default = "default_true")]
    pub auto_lyrics: bool,
    /// known length of the track. where the downloaded file is markedly
    /// shorter it was an excerpt, and the next source is tried.
    #[serde(default)]
    pub expected_duration_ms: Option<i64>,
    /// instead of a fixed address, pick the matching hit here.
    ///
    /// used for spotify links, where only metadata is on hand.
    #[serde(default)]
    pub match_query: Option<String>,
    /// what the user searched for. for the check after downloading only,
    /// unlike `match_query` it triggers no new search.
    #[serde(default)]
    pub intent: Option<String>,
    /// the name the hit carried in the search.
    ///
    /// needed where the file brings none worth the word. audius hands its
    /// tracks out over a bare stream address, and what comes back is named
    /// after its content address — the search knew "Passionfruit" all along,
    /// only the download does not see it.
    #[serde(default)]
    pub plan_title: Option<String>,
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

/// result of a download: the file lies in the working directory and the
/// metadata is a suggestion the user can still change.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadOutcome {
    pub job_id: String,
    pub path: String,
    pub duration_ms: i64,
    pub format: String,
    pub metadata: TrackMetadata,
    pub source_url: String,
    /// set where the result does not match the search input
    #[serde(default)]
    pub warning: Option<String>,
}

// whether what was downloaded matches what was searched for.
//
// some tracks exist in a clean version at no reachable source. robify then
// downloads what comes closest, and with "The Killers. Mr. Brightside" that
// was an upload by "Julia". the metadata describes it correctly, only it is
// the wrong song. the right one cannot be invented, but the misgrasp should
// not be kept quiet either.
//
// what is checked is whether every meaningful word of the input appears
// somewhere in title, artist or guests. `unbestaetigt` means the metadata
// search found nothing: confirmation from outside is missing then, and weaker
// signs weigh heavier
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

    // a single missing word is everyday. spellings differ, suffixes fall
    // away. only once half of them are missing is something wrong
    if fehlend.len() * 2 < woerter.len() {
        // second check: does the artist appear in the search at all?
        //
        // needed because the artist name often stands in the title as well.
        // with "The Killers- Mr. Brightside" by "Julia" every searched word
        // was there, in the title. the artist field was wrong all the same.
        //
        // whoever searches for a song title alone may not know the artist, so
        // this does not count on its own. where nothing arrived online
        // either, everything points at a misgrasp
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

// filename of the standalone yt-dlp for this operating system.
//
// yt-dlp publishes a program without dependencies per platform, and the names
// are the same at every release
fn ytdlp_asset() -> &'static str {
    if cfg!(target_os = "windows") {
        "yt-dlp.exe"
    } else if cfg!(target_os = "macos") {
        "yt-dlp_macos"
    } else {
        "yt-dlp_linux"
    }
}

/// where robify keeps its own yt-dlp
pub fn managed_ytdlp(tools_dir: &Path) -> PathBuf {
    tools_dir.join(if cfg!(target_os = "windows") {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    })
}

/// looks for yt-dlp without downloading anything: the configured path first,
/// then its own storage, then the system.
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

/// delivers yt-dlp and fetches it on the first run where nothing is there.
///
/// deliberately on demand rather than at startup: whoever uses the app for
/// playback alone is not to download 30 mb. the standalone build comes from
/// github, followed by one `--version` as a probe, a half-downloaded program
/// would be worse than none and would show up later with incomprehensible
/// errors.
pub async fn ensure_ytdlp(configured: Option<&str>, tools_dir: &Path) -> Result<PathBuf> {
    // on android there is nothing to fetch: yt-dlp ships as a library,
    // python runtime included, and is set up at startup. without this
    // exception the app downloaded the linux binary and then failed the probe
    // because android uses a different c library.
    //
    // the path is a placeholder: the bridge in `crate::ytdlp` does not need
    // it, it calls into the java runtime instead of starting a program
    if cfg!(target_os = "android") {
        return Ok(PathBuf::from("eingebaut"));
    }

    if let Some(path) = find_ytdlp(configured, tools_dir) {
        return Ok(path);
    }

    eigenes_holen(tools_dir).await
}

/// fetches the standalone build into the tools folder, come what may.
///
/// `ensure_ytdlp` stops as soon as any yt-dlp is found. this one does not: it
/// is called where one was found but refuses to renew itself, because it
/// belongs to pip or to a package manager. from then on `find_ytdlp` takes
/// this copy — it stands before the search path — and it can renew itself
/// with `-U` for good.
///
/// an existing copy is replaced. that is the point of the call.
pub async fn eigenes_holen(tools_dir: &Path) -> Result<PathBuf> {
    let ziel = managed_ytdlp(tools_dir);
    let url = format!(
        "https://github.com/yt-dlp/yt-dlp/releases/latest/download/{}",
        ytdlp_asset()
    );

    let daten = crate::online::client()
        .get(&url)
        // the program is around 30 mb, and the usual timeout of 20 seconds
        // does not stretch that far on slow lines
        .timeout(Duration::from_secs(300))
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| anyhow!(fehler!("yt-dlp konnte nicht geladen werden: {0}", e)))?
        .bytes()
        .await
        .map_err(|e| anyhow!(fehler!("yt-dlp konnte nicht geladen werden: {0}", e)))?;

    std::fs::create_dir_all(tools_dir)?;
    // write next to it first, then rename: where the download breaks off, no
    // half a program is left lying under the right name
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
    // on android it comes along in the package and never stands in the search
    // path. it is a build of robify's own, without the gpl parts — those are
    // video encoders, and nothing here touches video. the bridge in
    // `de.robify.player.Ytdlp` knows where it lies and hands it to yt-dlp
    if cfg!(target_os = "android") {
        return true;
    }
    which::which("ffmpeg").is_ok()
}

/// whether a javascript runtime is available.
///
/// youtube poses a javascript challenge on retrieval. without a runtime
/// yt-dlp falls back to an outdated route whose addresses are frequently
/// refused with "403 Forbidden". deno is yt-dlp's default, and node comes
/// with this project anyway.
pub fn js_runtime() -> Option<&'static str> {
    // none of these programs exists on android, and none can be installed
    // there afterwards. quickjs is built along and lies among the libraries
    // of the app; which path that is only the java side knows, so the bridge
    // in `de.robify.player.Ytdlp` appends `--js-runtimes` itself. nothing is
    // missing here, and there is nothing to search for either
    if cfg!(target_os = "android") {
        return None;
    }
    ["deno", "node", "bun", "qjs"]
        .into_iter()
        .find(|runtime| which::which(runtime).is_ok())
}

// older yt-dlp versions do not know `--js-runtimes` yet. check once and
// remember, `--help` answers in fractions of a second
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

/// file extensions the built-in player can decode.
///
/// opus is deliberately absent: rodio decodes through symphonia, and
/// symphonia brings no opus decoder. an `.opus` file lands in the library
/// cleanly enough but cannot be played there.
pub const PLAYABLE_EXTENSIONS: [&str; 9] = [
    "mp3", "m4a", "mp4", "aac", "flac", "ogg", "oga", "wav", "aiff",
];

/// whether the file can be played with what is on board
pub fn is_playable(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .is_some_and(|ext| PLAYABLE_EXTENSIONS.contains(&ext.as_str()))
}

/// which recording yt-dlp is to download.
///
/// at best quality nothing is converted, so the choice itself has to fall on
/// a playable format. besides opus youtube almost always offers m4a at the
/// same bitrate, and without this rule opus wins and the track stays mute.
///
/// `[format_id!*=preview]` keeps soundcloud previews out, which run for 30
/// seconds only.
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
    // in the end only getting anything at all counts. where it is converted,
    // the source format does not matter anyway
    wahl.extend(["bestaudio", "best"]);

    wahl.iter()
        .map(|filter| format!("{filter}[format_id!*=preview]"))
        .collect::<Vec<_>>()
        .join("/")
}

/// whether yt-dlp can write covers into the audio file.
///
/// for opus and ogg it needs the python library `mutagen`. without it not
/// only the embedding fails but the whole download, the finished audio file
/// being discarded along with the post-processing.
///
/// yt-dlp names its extra libraries itself when asked verbosely. that is more
/// precise than searching the system for `mutagen`: depending on how it was
/// installed, yt-dlp brings a python environment of its own.
async fn supports_thumbnail_embedding(ytdlp: &Path) -> bool {
    static SUPPORTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if let Some(known) = SUPPORTED.get() {
        return *known;
    }

    // without an address yt-dlp stops right away, but the diagnostic lines
    // stand on stderr before that. no network access, around 0.5 s
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

/// the runtime as an argument list, where present and supported
async fn js_runtime_args(ytdlp: &Path) -> Vec<String> {
    match js_runtime() {
        Some(runtime) if supports_js_runtimes(ytdlp).await => {
            vec!["--js-runtimes".into(), runtime.to_string()]
        }
        _ => Vec::new(),
    }
}

// --- load brake ---
//
// yt-dlp calls have to keep out of each other's way. a single search starts
// up to four processes at once, an album of thirty tracks over a hundred in
// sequence without a pause. youtube answers exactly that pattern with "403
// Forbidden", and the block then hits everything that follows.

/// this many yt-dlp processes run at once at most
const MAX_PARALLEL_YTDLP: usize = 2;

// --- timeouts ---
//
// without a limit a hanging process blocks the ui for good: searches cannot
// be cancelled at all, and the output loop of a download keeps turning
// endlessly as long as yt-dlp writes nothing.

/// this many hits at most are fetched in full from one source.
///
/// youtube music names running times on a full query only, and that costs one
/// request per hit. on a phone those were measured 6.8 seconds apiece: a
/// search over twelve hits per source took 74 seconds, the same over three
/// only 13. the remaining sources query flat and cost one request whatever
/// the number.
///
/// four are enough for what the full query is for, having a running time to
/// compare against. the breadth of the result list comes from the other
/// sources anyway, and they stay untouched by it.
const VOLLE_ABFRAGE_MAX: usize = 4;

/// after this time a search counts as failed
const SEARCH_TIMEOUT: Duration = Duration::from_secs(90);

/// upper bound for a single download
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// this long a running download may stay silent before it counts as hanging
const IDLE_TIMEOUT: Duration = Duration::from_secs(120);

/// upper bound for the conversion into a playable format
const CONVERT_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// minimum distance between two accesses to the same source
const MIN_SPACING: Duration = Duration::from_millis(700);

/// the distance while youtube is refusing.
///
/// seven hundred milliseconds are enough as long as nothing is amiss. once a
/// 403 has come, they are not: the block holds for a while and every further
/// call runs into it. in a run over five and twenty tracks eight of them were
/// lost that way, all of them to the same cause. backing off after the first
/// refusal costs a few seconds and saves the rest of the run.
const ABSAGE_SPACING: Duration = Duration::from_secs(5);

fn ytdlp_slots() -> &'static tokio::sync::Semaphore {
    static SLOTS: OnceLock<tokio::sync::Semaphore> = OnceLock::new();
    SLOTS.get_or_init(|| tokio::sync::Semaphore::new(MAX_PARALLEL_YTDLP))
}

/// when a source was last accessed
fn last_access() -> &'static Mutex<HashMap<&'static str, Instant>> {
    static LAST: OnceLock<Mutex<HashMap<&'static str, Instant>>> = OnceLock::new();
    LAST.get_or_init(Default::default)
}

/// clears leftover job folders away.
///
/// a download neither imported nor cancelled leaves its complete folder
/// behind, audio file included. over weeks that adds up, in practice it was
/// 16 mb without anybody getting anything out of it.
///
/// `crate::commands::KEEP_DIR` is spared: tracks taken over lie there and the
/// library points at them.
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
            // without a readable timestamp, better leave it standing
            .unwrap_or(false);

        if zu_alt && std::fs::remove_dir_all(&path).is_ok() {
            entfernt += 1;
        }
    }
    entfernt
}

// --- refusals from youtube ---
//
// youtube hands audio files out only where yt-dlp forms the retrieval
// addresses the new way, and that takes a javascript runtime. without one
// yt-dlp falls back to an emergency route and every download ends in "HTTP
// Error 403", every one of them, not only blocked tracks. on a phone nothing
// can be installed against it.
//
// the other sources are untouched by this. instead of running against a
// locked door, robify remembers the refusal and puts soundcloud, bandcamp and
// audius in front for a while. the note expires by itself: the situation at
// youtube changes, and without an expiry robify would stay with the fallback
// sources for the rest of the session.

/// this long a refusal from youtube counts as current
const ABSAGE_GILT: Duration = Duration::from_secs(30 * 60);

/// deduction for youtube hits while the refusal holds.
///
/// smaller than a missing search word (20). the deduction is to reorder hits
/// of equal standing, not to pull an unfitting track forward: better a
/// download that fails than the wrong song in the library.
const ABSAGE_ABZUG: f64 = 12.0;

fn absage_vermerk() -> &'static Mutex<Option<Instant>> {
    static VERMERK: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();
    VERMERK.get_or_init(|| Mutex::new(None))
}

/// whether the address belongs to youtube or youtube music
fn ist_youtube(url: &str) -> bool {
    let quelle = source_label(url);
    quelle == SearchSource::Youtube.label() || quelle == SearchSource::YoutubeMusic.label()
}

/// records that youtube refused the access
fn absage_merken() {
    *absage_vermerk().lock() = Some(Instant::now());
}

/// whether youtube is refusing right now
fn youtube_sagt_ab() -> bool {
    matches!(*absage_vermerk().lock(), Some(zeit) if zeit.elapsed() < ABSAGE_GILT)
}

/// whether the source refused the access
fn zugriff_verweigert(fehler: &anyhow::Error) -> bool {
    fehler.to_string().contains("(403)")
}

/// the deduction for a hit while the refusal holds
fn absage_abzug(url: &str) -> f64 {
    abzug_bei(url, youtube_sagt_ab())
}

/// the same without the note, so it can be tested.
///
/// the note is module-wide, and a test setting it rubbed off on every test
/// running alongside.
fn abzug_bei(url: &str, sagt_ab: bool) -> f64 {
    if sagt_ab && ist_youtube(url) {
        ABSAGE_ABZUG
    } else {
        0.0
    }
}

// maps an address to its source, for the minimum distance.
//
// rough but sufficient: all it does is pull accesses to the same service
// apart. whatever cannot be mapped shares one pot
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

// takes a slot and waits until the source is due again.
//
// the permit is returned, and the slot stays taken as long as it lives.
// deliberately module-wide rather than in `AppState`: the live tests call
// these functions directly and suffer most under the block
async fn acquire_slot(source: &'static str) -> tokio::sync::SemaphorePermit<'static> {
    let permit = ytdlp_slots()
        .acquire()
        .await
        .expect("Lastbremse wird nie geschlossen");

    // wait for the slot first, otherwise the waiting time would keep the slot
    // free and two callers would stand before the same source at once
    // a source that has just refused is approached more slowly
    let abstand = if source == SearchSource::Youtube.label() && youtube_sagt_ab() {
        ABSAGE_SPACING
    } else {
        MIN_SPACING
    };
    let warten = {
        let mut karte = last_access().lock();
        let jetzt = Instant::now();
        let rest = karte
            .get(source)
            .map(|zuletzt| abstand.saturating_sub(jetzt.duration_since(*zuletzt)))
            .unwrap_or_default();
        // note the moment right away so waiters queue up
        karte.insert(source, jetzt + rest);
        rest
    };
    if !warten.is_zero() {
        tokio::time::sleep(warten).await;
    }

    permit
}

/// hides the console window under windows
pub(crate) fn configure(cmd: &mut Command) {
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let _ = cmd;
}

/// a second attempt for youtube with one more word.
///
/// youtube answers some search terms with a page yt-dlp can read no entries
/// from, without an error, just empty. one extra word changes the answer.
///
/// shown with "Yeat Naked": no hits, while "Yeat Naked audio" delivers the
/// original right away. without the second attempt youtube was missing
/// entirely and the choice consisted of foreign versions only.
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

    // these two have interfaces of their own and need no yt-dlp
    match source {
        SearchSource::Bandcamp => return search_bandcamp(query, limit).await,
        SearchSource::Audius => return search_audius(query, limit).await,
        _ => {}
    }

    let mut args: Vec<String> = [
        "--dump-json",
        "--no-warnings",
        "--ignore-errors",
        // the same leniency as with a download: a brief disturbance must not
        // throw the source out of the result list
        "--retries",
        "3",
        "--retry-sleep",
        "2",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    if source.needs_full_extraction() {
        // the result page is a list itself and must not be treated as a
        // single track
        args.push("--playlist-items".into());
        args.push(format!("1-{}", limit.min(VOLLE_ABFRAGE_MAX)));
    } else {
        args.push("--flat-playlist".into());
        // unfold collections only where the address really points at one
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
        // the result page of youtube music holds artist and album pages as
        // well. only single tracks are usable
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

// --- picking a hit ---
//
// for a spotify link the exact running time is known. instead of taking the
// first search hit blindly, several are fetched and the best fitting one is
// chosen, which drops remixes, live versions and videos with an intro.

/// suffixes hinting at a different version
const VERSION_MARKERS: [&str; 45] = [
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
    // on bandcamp and soundcloud foreign versions almost always stand under
    // one of these words, and without them they win against the original
    "edit",
    "bootleg",
    "flip",
    "rework",
    "vip",
    "tribute",
    "in the style of",
    "made famous by",
    "type beat",
    // added from the audio comparison over 30 tracks: "Xtal (Duty Paid
    // Refix)" and "Tropical Island [TEKK]" won against the original
    "refix",
    "tekk",
    // "Creep (Acoustic)" won against the album version
    "acoustic",
    "akustik",
    "unplugged",
    // traces of ripping services: "Money Trees (HD Lyrics) - [www Flvto Com]"
    // stood above the official release
    "flvto",
    "y2mate",
    "320kbps",
    // versions that stand under a word of their own without being called a
    // remix. "mix" alone stays out: it turns up in enough proper titles
    "extended",
    "extended mix",
    "club mix",
    "radio mix",
    "dj mix",
    "dub",
    "chopped",
    "screwed",
    "reprise",
    "medley",
    // "rammstein - sonne (maukook sunrise cut)" went in as the original:
    // youtube had blocked, and the bootleg stood ready as the fallback
    "cut",
];

/// the version markers a title carries that the search did not ask for.
///
/// the comparison runs word by word: "edit" must not fire on "editor". and
/// whoever searches for a remix still gets it — a marker standing in the
/// search counts for nothing.
fn fremde_fassungen(candidate_title: &str, wanted_title: &str) -> Vec<&'static str> {
    let candidate = crate::online::normalize_words(candidate_title);
    let wanted = crate::online::normalize_words(wanted_title);
    VERSION_MARKERS
        .iter()
        .copied()
        .filter(|marker| {
            crate::online::contains_word_sequence(&candidate, marker)
                && !crate::online::contains_word_sequence(&wanted, marker)
        })
        .collect()
}

/// whether the candidate carries a bracketed addition the searched title does
/// not, and one that names something other than trivia or a guest.
///
/// the markers above catch what somebody has written down; this catches the
/// rest. "Sonne (Maukook Sunrise Cut)" names its version in a bracket without
/// using a single known word, and that is how bootlegs are titled: the name
/// of whoever cut it, then what they did.
fn fremde_klammer(candidate_title: &str, wanted_title: &str) -> bool {
    let gesucht = crate::online::normalize_words(wanted_title);
    crate::online::klammerzusaetze(candidate_title)
        .iter()
        .any(|zusatz| {
            if crate::online::ist_nur_beiwerk(zusatz) {
                return false;
            }
            // what the searched title carries itself is no foreign addition
            let worte = crate::online::normalize_words(zusatz);
            !worte.is_empty() && !crate::online::contains_word_sequence(&gesucht, &worte)
        })
}

/// deduction for hints at a different version that do not appear in the
/// track searched for.
fn version_penalty(candidate_title: &str, wanted_title: &str) -> f64 {
    let bekannt = fremde_fassungen(candidate_title, wanted_title).len() as f64 * 25.0;
    // an unnamed version weighs as much as a named one: whether the bracket
    // says "Remix" or "Sunrise Cut" changes nothing about the recording
    let ungenannt = if fremde_klammer(candidate_title, wanted_title) {
        25.0
    } else {
        0.0
    };
    bekannt + ungenannt
}

/// how far a hit lies from what was searched for, in text. 0.0 means every
/// searched word appears in the title or the channel name.
///
/// without this check running time and source decided alone, and whoever
/// offered anything similar won against the right track.
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

    // a word in the title that was not searched for points at a different
    // version, and now and then at a different track: "Love Bicep Glue" is a
    // mashup and went into the library as "Bicep - Glue".
    //
    // trivia and guest credits are no such sign. "Money Trees ft. Jay Rock"
    // is exactly what was searched for, and charging for the guests would
    // push the official upload behind a bare reupload
    let mut extra = 0usize;
    for word in title.split(' ').filter(|word| !word.is_empty()) {
        // behind "feat", "ft" or "prod" stands a name, never a version
        if crate::online::ist_namenswort(word) {
            break;
        }
        if wanted_words.contains(&word) || crate::online::ist_beiwerk_wort(word) {
            continue;
        }
        extra += 1;
    }

    // deliberately below what a missing word costs: a hit carrying every
    // searched word and a label name on top is still the better one
    missing as f64 * 20.0 + (extra as f64 * 9.0).min(18.0)
}

/// shorter than this is no whole track but a preview
const PREVIEW_LIMIT_MS: i64 = 60_000;

/// longer than this is no music track.
///
/// broken or malicious values from a search answer would otherwise land in
/// the score as `i64::MAX`, since `as i64` saturates at the maximum silently
/// instead of failing.
const MAX_DURATION_MS: i64 = 24 * 60 * 60 * 1000;

/// accepts only running times that can be calculated with
fn sane_duration(value: Option<i64>) -> Option<i64> {
    value.filter(|ms| *ms > 0 && *ms <= MAX_DURATION_MS)
}

/// running times further apart than this do not belong to the same
/// recording.
const SAME_TAKE_MS: i64 = 15_000;

/// how long the searched track really is. the sources know it together: the
/// same recording turns up several times and their running times form a
/// cluster. outliers are excerpts, extended versions or a different track.
///
/// this stands in for a value from outside where there is none, so on every
/// search through the input field.
fn consensus_duration_ms(results: &[SearchResult], query: &str) -> Option<i64> {
    // only hits that mean the track itself have a say.
    //
    // otherwise the foreign versions set the norm, and next to a well-known
    // track there are more of them than of the original: under "Bicep Glue"
    // stand seven bootlegs and edits, each of a length of its own. their
    // middle became the majority, and the official recording was charged for
    // deviating from it — while a hit naming no length at all got away with
    // the flat twelve. that is how a mashup came to stand first.
    let mut durations: Vec<i64> = results
        .iter()
        .filter(|r| {
            coverage_penalty(query, r) == 0.0 && version_penalty(&r.title, query) == 0.0
        })
        .filter_map(|r| r.duration_ms)
        .filter(|ms| *ms > 0)
        .collect();
    // below three values there is no talk of a majority
    if durations.len() < 3 {
        return None;
    }
    durations.sort_unstable();

    // the largest group in which all of them lie close together
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

/// deduction for straying from the majority opinion.
///
/// capped, so the length supports the title comparison without outvoting
/// it.
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

/// deduction for obvious excerpts. takes effect only while the actual
/// running time is unknown, otherwise the comparison decides.
///
/// deliberately smaller than a missing search word: an excerpt of the right
/// track is still closer than a foreign track at full length.
fn preview_penalty(candidate: &SearchResult) -> f64 {
    match candidate.duration_ms {
        Some(ms) if ms > 0 && ms < PREVIEW_LIMIT_MS => 15.0,
        _ => 0.0,
    }
}

/// scores a hit, the smaller the better. `None` means out of the question.
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
                // more than half a minute off is a different track
                if difference_seconds > 30.0 {
                    return None;
                }
                score += difference_seconds * 2.0;
            }
            // bandcamp names no running time. such hits do not fly out but
            // slide behind every provably fitting one
            None => score += 12.0,
        },
        // without a given value only the plausibility of the length is left
        None => score += preview_penalty(candidate),
    }

    // suffixes not appearing in the searched track speak against it
    score += version_penalty(&candidate.title, expected_title);

    // sources that reliably deliver the whole track in good quality get a
    // head start
    if let Some(source) = SearchSource::from_label(&candidate.source) {
        score -= source.quality_bonus();
    }

    Some(score)
}

/// orders the hits found by fit. the addresses come back in the order they
/// are to be tried in.
fn rank_candidates(
    found: &[SearchResult],
    query: &str,
    expected_duration_ms: Option<i64>,
    expected_title: &str,
) -> Vec<String> {
    // without a value from outside the majority of the sources decides how
    // long the track is. only deduct, do not exclude: the majority can be
    // wrong where a known running time cannot
    let consensus = match expected_duration_ms.filter(|ms| *ms > 0) {
        Some(_) => None,
        None => consensus_duration_ms(found, query),
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
        // no hit held up to the length check. then at least order by textual
        // closeness instead of taking the first one blindly.
        //
        // the version deduction belongs here as well, and its absence was a
        // hole: exactly where the length check lets nothing through, the
        // original is missing from the list, and a remix stood first without
        // anything speaking against it
        ranked = found
            .iter()
            .map(|candidate| {
                (
                    coverage_penalty(query, candidate)
                        + version_penalty(&candidate.title, expected_title)
                        + preview_penalty(candidate)
                        + absage_abzug(&candidate.url),
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

/// searches several hits and orders them by fit
pub async fn select_matches(
    ytdlp: &Path,
    query: &str,
    expected_duration_ms: Option<i64>,
    expected_title: &str,
) -> Result<Vec<String>> {
    let found = search_everywhere(ytdlp, query, 5).await?;
    Ok(rank_candidates(&found, query, expected_duration_ms, expected_title))
}

/// whether the address points at a whole collection: album, playlist, set.
///
/// a youtube link with `v=` stays a single video even where a playlist stands
/// in the link as well, otherwise a copied track would accidentally turn into
/// a whole playlist.
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

/// orders search hits for display: what fits the search term first, then the
/// rest. the sort is stable, so the mixture of sources is kept among hits of
/// equal standing.
///
/// unordered, the first hit of whichever source stood on top, even where it
/// was an entirely different track or only an excerpt.
fn sort_by_relevance(results: &mut [SearchResult], query: &str) {
    let consensus = consensus_duration_ms(results, query);
    let rang = |candidate: &SearchResult| {
        coverage_penalty(query, candidate)
            // without this check remixes and edits stood right on top: the
            // word comparison hides bracket contents, and that is where they
            // stand
            + version_penalty(&candidate.title, query)
            + preview_penalty(candidate)
            // truncated uploads are recognised by every other source
            // agreeing on a different length
            + consensus_penalty(candidate, consensus)
            // without a running time neither an excerpt can be recognised
            // nor the majority length checked. in the audio comparison those
            // were bandcamp hits throughout, and among them strikingly many
            // foreign versions that would otherwise slide up unchecked
            + if candidate.duration_ms.is_none() { 12.0 } else { 0.0 }
    };
    results.sort_by(|a, b| rang(a).total_cmp(&rang(b)));
}

/// queries every search source at once and mixes the hits so each source is
/// represented at the top. nobody has to pick a source beforehand this way.
pub async fn search_everywhere(
    ytdlp: &Path,
    query: &str,
    per_source: usize,
) -> Result<Vec<SearchResult>> {
    Ok(search_all_sources(ytdlp, query, per_source).await?.0)
}

/// like `search_everywhere`, and names the sources that dropped out as well.
///
/// where one source drops out while others deliver, the result list merely
/// looks thin and the user does not see that something is missing. with a
/// block from youtube that hits exactly the source with the widest choice.
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

    // take one hit from each in turn
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

// explanation for a refused access. without a javascript runtime that is
// almost always the cause, with one it is usually a brief throttling
fn blocked_message(has_js_runtime: bool) -> String {
    // on android both pieces of advice the other two sentences give lead
    // nowhere: a javascript runtime cannot be installed afterwards, and
    // `yt-dlp -U` does not exist because yt-dlp is no file there but sits in
    // the app. nothing is missing either: quickjs is built along and the
    // bridge hands it to yt-dlp through `--js-runtimes`, and yt-dlp is
    // renewed with the app. so only the throttling is left, and the advice
    // to try elsewhere
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
            "Die Quelle hat den Zugriff abgelehnt (403). Es ist keine JavaScript-Laufzeit installiert. Ohne sie kann yt-dlp die Abrufadressen von YouTube nicht bilden, und dort scheitert dann jeder Download. Installiere Node.js, Deno oder Bun, dann funktioniert es dauerhaft."
        )
    }
}

// appends the verbatim message of the source to our own explanation.
//
// chained rather than assembled: both sentences stay individually
// translatable, and the trailing one does not have to be written into each of
// the explanations. the message of the source itself is english and stays
// that way, it comes from yt-dlp
fn with_details(message: String, raw: &str) -> String {
    let detail = raw.trim().trim_start_matches("ERROR:").trim();
    if detail.is_empty() {
        return message;
    }
    crate::meldung::verketten(message, fehler!("Meldung der Quelle: {0}", detail))
}

// translates the output of yt-dlp into a message one can act on.
//
// yt-dlp spreads errors over several lines and appends notes for issue
// reports, so the last line on its own is usually worthless
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

    // the complete text is searched because the cause sometimes stands in a
    // line before or after the error line
    let haystack = stderr.to_lowercase();

    // real http errors only, not every run of digits: "403" can sit inside a
    // video id or a byte count as well
    if haystack.contains("http error 403")
        || haystack.contains("http error 429")
        || haystack.contains("too many requests")
    {
        return with_details(blocked_message(js_runtime().is_some()), raw);
    }
    // the post-processing stumbles over a missing python library. the audio
    // would have been there, only the cover could not be embedded
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

/// runs yt-dlp and reports the progress.
///
/// on a desktop a process of its own whose output is read line by line: that
/// way a cancellation takes effect promptly, and a hanging run shows up
/// through the line that fails to arrive.
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

        // a hanging process writes nothing any more. bound both: the total
        // duration and the silence in between
        if gestartet.elapsed() > DOWNLOAD_TIMEOUT || letzte_zeile.elapsed() > IDLE_TIMEOUT {
            let _ = child.kill().await;
            let _ = std::fs::remove_dir_all(job_dir);
            bail!(
                "Die Quelle antwortet nicht mehr, nach {} ohne Fortschritt abgebrochen.",
                format_seconds(letzte_zeile.elapsed().as_millis() as i64)
            );
        }

        // read line by line so the cancellation takes effect promptly
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

/// runs yt-dlp over the java bridge.
///
/// there is no process and no output to read along there: the call blocks
/// until the end and only delivers then. the bridge therefore keeps the
/// progress as a value to be polled, which this run picks up on a tick and
/// passes on. a cancellation goes back the same way.
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

    // either pick the best hit out …
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
                // where the picking finds nothing, the original route stays
                _ => std::iter::once(options.url.clone())
                    .chain(options.fallbacks.iter().cloned())
                    .collect(),
            }
        }
        // … or take the given address along with its fallback list
        _ => std::iter::once(options.url.clone())
            .chain(options.fallbacks.iter().cloned())
            .collect(),
    };

    // where youtube is refusing right now, it goes to the back.
    //
    // the chosen hit loses its precedence that way, but an attempt known to
    // fail costs half a minute. the order among the rest stays as it was
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
        // a refusal from youtube holds for all of its hits, not for this one
        // alone. recognised by the "(403)" from `blocked_message`, the
        // message is translated in the ui only and the number stands in every
        // language
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
            // no address delivered. fragments and partial files would
            // otherwise lie in the working directory for good
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
    // on a second attempt nothing from the first is to lie around
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

    // built as a list rather than on the command: on android no program
    // starts, the same list goes to yt-dlp over the java bridge there
    let mut args: Vec<String> = vec![
        "--newline".into(),
        "--no-playlist".into(),
        // the warnings stay on.
        //
        // they used to sit under `--no-warnings`, and that made exactly the
        // note disappear which cleared up a 403 that had been inexplicable
        // for days: "No supported JavaScript runtime could be found … YouTube
        // extraction without a JS runtime has been deprecated." the error
        // names the refusal only, the warning before it names the reason.
        // `explain_failure` looks for the error line anyway and takes the
        // last line only where there is none
        // short-lived blocks (http 403) usually go away by themselves
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
        // the account the track was published under
        "--print-to-file".into(),
        "%(uploader,channel,creator,artist)s".into(),
        uploader_file.to_string_lossy().into_owned(),
        // the music fields of the source. where they exist (youtube music,
        // official uploads, soundcloud) they are cleaner than anything that
        // can be derived from the video title
        "--print-to-file".into(),
        "%(track)s\u{1f}%(artist)s\u{1f}%(album)s\u{1f}%(release_year)s\u{1f}%(track_number)s".into(),
        music_file.to_string_lossy().into_owned(),
        // which playback client yt-dlp uses at youtube.
        //
        // left alone yt-dlp picks `android_vr`, whose retrieval addresses
        // youtube now refuses with "HTTP Error 403", on every track, freely
        // licensed ones included. `web_embedded` delivers addresses youtube
        // accepts, provided a javascript runtime is there. measured:
        // `default` fails, `web_embedded,default` downloads.
        //
        // the remaining clients stay behind it as a fallback, otherwise what
        // only one of them hands out would fall away. the option carries the
        // `youtube:` namespace and is no business of other sources
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
        // `-x` lifts the audio track out of the container. without
        // `--audio-format` it stays unchanged, no second lossy pass
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
    // where `mutagen` is missing, the cover is left out rather than losing
    // the download. robify writes it in itself at import anyway
    if options.embed_thumbnail && ffmpeg_available() && supports_thumbnail_embedding(ytdlp).await {
        args.push("--embed-thumbnail".into());
    }
    // the tags too are written by ffmpeg, and yt-dlp breaks the download off
    // where it is missing: "ffmpeg wird für dieses Format benötigt". on
    // android that hit every single download, and there is nothing lost by
    // leaving it out — title, artist, album and cover robify writes itself
    // at import, out of what the metadata search found
    if ffmpeg_available() {
        args.push("--embed-metadata".into());
    }
    args.push(options.url.clone());
    args.extend(js_runtime_args(ytdlp).await);

    // the slot stays taken for the whole duration of the download, otherwise
    // any number of transfers would run side by side on "download all"
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

    // a markedly too short result is an excerpt, not a whole track
    if let Some(expected) = options.expected_duration_ms.filter(|ms| *ms > 0) {
        let actual = file_tags.duration_ms;
        // as a ratio rather than a product: a broken running time must not
        // make the calculation overflow
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

    // the fields of the source first, then the video title. both stand
    // before what yt-dlp wrote into the file for lack of music fields, where
    // the channel name lands as the artist otherwise
    let source = SourceMetadata::read(&music_file);
    apply_source_metadata(&mut metadata, &source, uploader.as_deref());

    // where the file names itself after its content address, take the name
    // the search hit carried. otherwise everything downstream works on a
    // checksum: the online lookup finds nothing under it, the album stays
    // empty, and the track lands in the library under a row of characters
    if unbrauchbarer_titel(&metadata.title) {
        if let Some(name) = options
            .plan_title
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty() && !unbrauchbarer_titel(name))
        {
            metadata.title = strip_file_suffix(name);
            if let Some((kuenstler, titel)) =
                crate::library::split_video_title(&metadata.title, uploader.as_deref())
            {
                metadata.artist = kuenstler;
                metadata.title = titel;
            }
        }
    }

    // details from a video description are often rough. look them up online
    // and correct them where the hit is unambiguous. after a cancellation the
    // search is no longer worth it
    if options.auto_match && !cancel.load(Ordering::SeqCst) {
        emit(
            app,
            status(job_id, "processing", 99.0, Some("Metadaten werden gesucht…".into())),
        );
        let mut gefunden = crate::online::auto_match(
            &metadata,
            Some(file_tags.duration_ms),
            options.auto_cover,
            options.auto_lyrics,
        )
        .await;

        // second attempt over what the user typed.
        //
        // the first needs title and artist of the file to fit a catalogue,
        // and where the artist is the name of a reupload channel it fits
        // nothing. then the input is the better clue — it named the artist,
        // which is exactly what the file does not know
        if gefunden.is_none() {
            if let Some(absicht) = options
                .intent
                .as_deref()
                .map(str::trim)
                .filter(|text| !text.is_empty())
            {
                gefunden = crate::online::match_aus_absicht(
                    absicht,
                    Some(file_tags.duration_ms),
                    options.auto_cover,
                    options.auto_lyrics,
                )
                .await;
            }
        }

        if let Some(found) = gefunden {
            metadata = crate::online::merge_match(metadata, found);
        }
    }

    // guests standing in the title belong in the guest field, otherwise they
    // show up under no artist in the library. this used to happen with
    // spotify links only, the other sources write it into the title just the
    // same
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

    // the source names the release its own copy comes from, and that is not
    // always the one the recording belongs to. youtube music serves "Billie
    // Jean" out of "HIStory" and "Das Model" out of the live box "3-D Der
    // Katalog" — the audio is right both times, the album is a best-of.
    //
    // so it only fills a gap now. it used to have the last word, and back
    // then that was the better rule: the metadata search took the first hit
    // that fitted and landed on a vinyl cut or a sampler often enough. since
    // four catalogues are asked at once and the album is decided by majority
    // among them, they are the more reliable of the two.
    //
    // the second case: where the album carries the same name as the track it
    // is only the placeholder of a single, and the search leads to the real
    // album more often ("Creep" to "Pablo Honey")
    if metadata.album.trim().is_empty() {
        if let Some(album) = source
            .album
            .as_deref()
            .filter(|album| !crate::online::looks_like_same(album, &metadata.title))
        {
            metadata.album = album.to_string();
        }
    }

    // details known beforehand, from a spotify link for instance, have the
    // last word
    if let Some(known) = &options.metadata {
        metadata = merge_metadata(known, metadata);
    }

    // spotify knows no guest roles and throws everyone involved into one
    // list. where the online search recognised guests, they do not belong
    // among the lead artists as well
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

    // the channel downloaded from is the lead artist, provided it belongs to
    // the participants at all
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

    // the kind of release, in three steps.
    //
    // it used to say "album" for everything carrying an album name, and that
    // was a guess dressed up as knowledge. counting the tracks that happen to
    // lie in the library is no better: two out of a record of twenty-two are
    // no single.
    if metadata.album.trim().is_empty() {
        // no album, no question
        metadata.release_type = Some("single".to_string());
    } else if !cancel.load(Ordering::SeqCst) {
        // asked always, not only where nothing is known yet.
        //
        // the match on the track brings a kind along, but a derived one:
        // itunes hides it in the album name and otherwise it is counted, and
        // counting says "album" for an ep of eleven tracks. it arrived first
        // and blocked the better answer. asked by the name of the album,
        // musicbrainz keeps the kind as a curated field and says outright
        // what a release is.
        if let Some((art, _)) =
            crate::online::release_kind(&metadata.artist, &metadata.album).await
        {
            metadata.release_type = Some(art);
        }
    }

    // without a cover the metadata search found nothing either, which was
    // the case with every misgrasp observed
    let unbestaetigt = metadata.cover_base64.is_none();
    let mut warnungen: Vec<String> = Vec::new();

    if let Some(text) = intent_warning(options.intent.as_deref(), &metadata, unbestaetigt) {
        warnungen.push(if unbestaetigt {
            format!("{text} Auch online war dazu nichts zu finden.")
        } else {
            text
        });
    }

    // the picking pushes a foreign version far down, but it cannot conjure
    // the original where no source offers it. what stays is to say so:
    // otherwise a remix lands in the library under the name of the original,
    // and nothing points at it any more
    if let Some(marker) = options
        .intent
        .as_deref()
        .map(|absicht| fremde_fassungen(&metadata.title, absicht))
        .and_then(|treffer| treffer.first().copied())
    {
        warnungen.push(format!(
            "Das Geladene ist eine abweichende Fassung („{marker}“ steht im Titel, in deiner Suche nicht): \
             „{}“. Prüfe die Angaben oder wähle einen anderen Treffer.",
            metadata.title
        ));
    }

    let warning = (!warnungen.is_empty()).then(|| warnungen.join(" "));

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

// joins metadata known beforehand with that from the file. what the source
// knows for certain wins, everything else is filled from the file
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

/// what the source itself knows about the track.
///
/// yt-dlp fills these fields for music, at youtube music always, at official
/// youtube uploads and soundcloud mostly. foreign lyric and repost channels
/// deliver nothing, and only the video title is left there.
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

    /// the fields arrive separated by `\u{1f}`, and "NA" stands for unknown
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

/// takes title and artist from the best source available.
///
/// the order: music fields of the source, then splitting the video title,
/// then what already stood in the file. without this step, uploads without
/// music fields put the complete video title into the library as the song
/// title and the channel name as the artist ("xTheLYRICS" instead of "Nina
/// Chuba").
/// whether a title is no title but the name of a file on some storage.
///
/// a content address is long, holds no space, mixes letters and digits and
/// means nothing. a real title of that length without a single space does not
/// occur; the digit is what tells the two apart, for a german compound word
/// carries none.
fn unbrauchbarer_titel(titel: &str) -> bool {
    let text = titel.trim();
    text.chars().count() >= 24
        && !text.contains(char::is_whitespace)
        && text.chars().all(|zeichen| zeichen.is_ascii_alphanumeric())
        && text.chars().any(|zeichen| zeichen.is_ascii_digit())
}

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

    // soundcloud plainly sets `track` to the name of the uploaded file,
    // extension included. without cutting it the track would be called
    // "Xtal.mp3"
    metadata.title = strip_file_suffix(&metadata.title);

    let Some((artist, title)) = crate::library::split_video_title(&metadata.title, uploader) else {
        return;
    };

    // when does the title carry the whole upload name?
    //
    // * there is no title field at all, then everything stands in the video
    //   title.
    // * there is no artist. both fields come out of the same extraction:
    //   where one is missing, the other is unfiltered as well.
    // * the left half is provably the artist already known
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

/// extensions that stay behind in upload names
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

/// whether `name` means the same artist as the value already known
fn bezeichnet_denselben(name: &str, bekannt: Option<&str>) -> bool {
    bekannt.is_some_and(|bekannt| crate::online::looks_like_same(name, bekannt))
}

/// makes sure the downloaded file can be played.
///
/// the format selection prefers fitting codecs already. some sources offer
/// opus alone though, and then only a conversion is left, otherwise a mute
/// track would lie in the library.
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
            // through the template, not finished: the ui assembles the
            // sentence in its own language
            Some(crate::meldung::bauen(
                "{0} wird in ein abspielbares Format gebracht…",
                &[&format],
            )),
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

// calls ffmpeg, whatever the system.
//
// on a desktop it is a program in the search path. on android it ships as a
// library, there is no `ffmpeg` to find there, and starting a process of our
// own already failed on android not allowing execution outside the library
// folder
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

// yt-dlp writes the final path into `result.txt`. where that fails, the
// newest audio file in the working directory is taken
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
        consensus_penalty,
        coverage_penalty, explain_failure,
        ist_youtube, is_collection_url, plans_with_fallbacks, rank_candidates, score_candidate,
        fremde_fassungen, fremde_klammer, same_song, sort_by_relevance, version_penalty,
        SearchResult, TrackMetadata, ABSAGE_ABZUG,
    };

    /// a hit with an address the source can be recognised by
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

    /// the reported case: youtube had blocked, and a bootleg edit stood ready
    /// as the fallback address. it went into the library as the original,
    /// album "PLAY045 - THE MEME CUTS III" and all
    /// "Bicep - Glue" came out of the library as "Love Bicep Glue" by
    /// BLINDMANZ. the foreign word carries no bracket, so only its weight in
    /// the ranking separates the mashup from the track
    #[test]
    fn ein_fremdes_wort_im_titel_wiegt_schwerer_als_beiwerk() {
        let mashup = treffer_bei("Love Bicep Glue", 269, "https://bandcamp.com/a");
        let echt = treffer_bei("BICEP | GLUE (Official Video)", 285, "https://youtube.com/b");

        assert!(
            coverage_penalty("Bicep Glue", &mashup) > coverage_penalty("Bicep Glue", &echt),
            "das Mashup steht nicht hinter der Aufnahme"
        );
        assert_eq!(coverage_penalty("Bicep Glue", &echt), 0.0);
    }

    /// guests and label names must not push the official upload down
    #[test]
    fn gaeste_und_beiwerk_kosten_nichts() {
        let mit_gaesten = treffer_bei(
            "Kendrick Lamar - Money Trees ft. Jay Rock",
            386,
            "https://youtube.com/a",
        );
        assert_eq!(
            coverage_penalty("Kendrick Lamar Money Trees", &mit_gaesten),
            0.0
        );

        let mit_beiwerk = treffer_bei(
            "Michael Jackson - Billie Jean 2009 Official HD",
            294,
            "https://youtube.com/b",
        );
        assert_eq!(
            coverage_penalty("Michael Jackson Billie Jean", &mit_beiwerk),
            0.0
        );
    }

    /// what only ffmpeg can do must not be asked for where it is missing.
    ///
    /// on android it is missing on purpose, and `--embed-metadata` broke off
    /// every download there: yt-dlp writes the tags with ffmpeg and refuses
    /// without it. the same holds for `-x` and `--embed-thumbnail`
    #[test]
    fn ohne_ffmpeg_wird_nichts_verlangt_was_ffmpeg_braucht() {
        // measured rather than claimed: the flags are read out of the source
        // itself, so a new one cannot be added past this test
        let quelle = include_str!("downloader.rs");
        for schalter in ["--embed-metadata", "--embed-thumbnail", "--audio-format"] {
            let stelle = quelle
                .find(&format!("args.push(\"{schalter}\".into())"))
                .unwrap_or_else(|| panic!("{schalter} nicht mehr im Quelltext"));
            // in the thousand characters before it `ffmpeg_available` has to
            // stand — that is the guard
            let davor = &quelle[stelle.saturating_sub(1000)..stelle];
            assert!(
                davor.contains("ffmpeg_available()"),
                "{schalter} wird ohne Prüfung auf ffmpeg übergeben"
            );
        }
    }

    #[test]
    fn ein_bootleg_taugt_nicht_als_ausweichadresse() {
        let echt = treffer_bei(
            "Rammstein - Sonne (Official Video)",
            272,
            "https://www.youtube.com/watch?v=abc",
        );
        let bootleg = treffer_bei(
            "rammstein - sonne (maukook sunrise cut)",
            270,
            "https://soundcloud.com/maukook/sonne",
        );
        assert!(
            !same_song(&bootleg, &echt),
            "das Bootleg gilt als dieselbe Aufnahme"
        );
    }

    #[test]
    fn beiwerk_in_der_klammer_stoert_nicht() {
        let echt = treffer_bei("Sonne", 272, "https://www.youtube.com/watch?v=abc");

        // trivia and guest credits leave the recording what it is
        for titel in [
            "Rammstein - Sonne (Official Video)",
            "Rammstein - Sonne [HD]",
            "Rammstein - Sonne (Official Audio) [Free Download]",
            "Daft Punk - Instant Crush (feat. Julian Casablancas)",
        ] {
            assert!(
                !fremde_klammer(titel, "Sonne"),
                "„{titel}“ wurde als fremde Fassung gewertet"
            );
        }

        let gleich = treffer_bei(
            "Rammstein - Sonne (Official Video)",
            270,
            "https://soundcloud.com/rammstein/sonne",
        );
        assert!(same_song(&gleich, &echt));
    }

    #[test]
    fn eine_klammer_die_im_gesuchten_steht_zaehlt_nicht() {
        // whoever searches for the live version may have it
        assert!(!fremde_klammer(
            "Rammstein - Sonne (Live at Rock im Park)",
            "Sonne (Live at Rock im Park)"
        ));
        assert!(fremde_klammer(
            "Rammstein - Sonne (Live at Rock im Park)",
            "Sonne"
        ));
    }

    #[test]
    fn youtube_wird_an_der_adresse_erkannt() {
        assert!(ist_youtube("https://www.youtube.com/watch?v=abc"));
        assert!(ist_youtube("https://youtu.be/abc"));
        assert!(ist_youtube("https://music.youtube.com/watch?v=abc"));
        assert!(!ist_youtube("https://soundcloud.com/wer/was"));
        assert!(!ist_youtube("https://kuenstler.bandcamp.com/track/was"));
    }

    /// the deduction may reorder but must pull nothing wrong to the front.
    ///
    /// a missing search word costs 20. were the deduction above that, a
    /// soundcloud hit with a foreign title would win against the right song
    /// at youtube, and a wrong track in the library is hard to notice later
    /// while a failed download shows at once.
    #[test]
    fn abschlag_bleibt_unter_einem_fehlenden_wort() {
        // measured rather than claimed: what a missing word costs stands in
        // `coverage_penalty` and may change without this test quietly
        // becoming useless
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
        // the refusal never hits the remaining sources
        assert_eq!(abzug_bei("https://soundcloud.com/wer/was", true), 0.0);
    }

    /// the fallback list is to change the source.
    ///
    /// where youtube refuses, it refuses for all of its hits. three youtube
    /// addresses used to stand in the list and ended three times in the same
    /// 403 while the soundcloud hit lay untried next to them.
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
        // exactly the route `download` takes: explained message, then check
        let meldung = explain_failure("ERROR: unable to download video data: HTTP Error 403: Forbidden\n");
        assert!(super::zugriff_verweigert(&anyhow::anyhow!(meldung)));
        assert!(!super::zugriff_verweigert(&anyhow::anyhow!(explain_failure(
            "ERROR: [youtube] abc: Private video"
        ))));
    }

    #[test]
    fn drm_meldung_wird_erklaert() {
        // original output of yt-dlp: the cause is not in the last line
        let stderr = "\
[youtube] AbCdEf: Some formats are drm protected
ERROR: [youtube] AbCdEf: This video is DRM protected
Please DO NOT open an issue, unless you have evidence that the video is not DRM protected
";
        let message = explain_failure(stderr);
        assert!(message.contains("kopiergeschützt"), "unerwartet: {message}");
        // the message has to name the way out, not the obstacle alone
        assert!(message.contains("übrigen Treffer"), "kein Ausweg genannt: {message}");
        assert!(!message.contains("DO NOT open an issue"));
    }

    #[test]
    fn abgelehnter_zugriff_wird_erklaert() {
        let message =
            explain_failure("ERROR: unable to download video data: HTTP Error 403: Forbidden\n");
        assert!(message.contains("(403)"), "unerwartet: {message}");
        // the original message is kept so the cause stays traceable
        assert!(message.contains("Meldung der Quelle"), "Details fehlen: {message}");

        assert!(explain_failure("ERROR: HTTP Error 429: Too Many Requests").contains("(403)"));
    }

    #[test]
    #[cfg(not(target_os = "android"))]
    fn fehlende_js_laufzeit_wird_als_ursache_genannt() {
        // without a runtime that is the actual cause …
        assert!(blocked_message(false).contains("JavaScript-Laufzeit"));
        assert!(blocked_message(false).contains("Node.js"));
        // … with one, only the throttling is left as an explanation
        assert!(!blocked_message(true).contains("JavaScript-Laufzeit"));
        assert!(blocked_message(true).contains("yt-dlp -U"));
    }

    /// on android neither piece of advice is any use.
    ///
    /// node.js cannot be installed there, and `yt-dlp -U` reaches into thin
    /// air because yt-dlp comes out of the library. exactly that stood on the
    /// phone after a 403: "no javascript runtime is installed", although
    /// quickjs ships with it and yt-dlp uses it.
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
        // "403" sits inside an id here, not inside an http error
        let stderr = "ERROR: [soundcloud] 403291102: Requested format is not available\n";
        let message = explain_failure(stderr);
        assert!(
            message.contains("nur eine Vorschau"),
            "falsch eingeordnet: {message}"
        );
        assert!(!message.contains("vorübergehend gesperrt"));

        // a byte count holding 429 must trigger nothing either
        let stderr = "ERROR: [generic] xyz: Kaputt nach 4291 Bytes\n";
        assert!(!explain_failure(stderr).contains("vorübergehend gesperrt"));
    }

    // the length check throws every hit out where the given running time
    // fits none of them. what is ordered then is ordered by text alone, and
    // without the version deduction a remix stood first — exactly where the
    // original is missing from the list and nothing else speaks against it
    #[test]
    fn auch_ohne_passende_laenge_verliert_der_remix() {
        let treffer = vec![
            treffer_bei("Yeat - Breathe (Sped Up Remix)", 130, "youtube:a"),
            treffer_bei("Yeat - Breathe", 128, "youtube:b"),
        ];
        // ten minutes: no hit survives the check
        let reihe = rank_candidates(&treffer, "Yeat Breathe", Some(600_000), "Breathe");
        assert_eq!(reihe.first().map(String::as_str), Some("youtube:b"));
    }

    #[test]
    fn eine_fremde_fassung_wird_benannt() {
        let treffer = fremde_fassungen("Breathe (Slowed + Reverb)", "Breathe");
        assert!(treffer.contains(&"slowed"));
        assert!(treffer.contains(&"reverb"));
    }

    // whoever searches for a remix is to get one without a warning
    #[test]
    fn was_in_der_suche_steht_gilt_nicht_als_fremd() {
        assert!(fremde_fassungen("Breathe (XY Remix)", "Breathe Remix").is_empty());
    }

    // the new markers must not fire on ordinary words
    #[test]
    fn merkmale_treffen_nur_ganze_woerter() {
        assert!(fremde_fassungen("Dubstep Anthem", "Dubstep Anthem").is_empty());
        assert!(fremde_fassungen("Dubstep Anthem", "Anthem").is_empty());
        assert!(!fremde_fassungen("Anthem (Dub)", "Anthem").is_empty());
    }

    #[test]
    fn veraltetes_ytdlp_bekommt_update_hinweis() {
        let stderr = "ERROR: [youtube] xyz: Unable to extract player response\n";
        assert!(explain_failure(stderr).contains("yt-dlp -U"));
    }

    /// template and value travel separately so the ui can translate.
    ///
    /// the line from yt-dlp itself stays untouched: it is english and does
    /// not come from here.
    #[test]
    fn unbekannter_fehler_zeigt_die_error_zeile() {
        let stderr = "[debug] irgendwas\nERROR: [generic] xyz: Kaputt\n";
        let teile: Vec<String> = explain_failure(stderr)
            .split(crate::meldung::TRENNER)
            .map(str::to_string)
            .collect();
        assert_eq!(teile, vec!["yt-dlp: {0}", "[generic] xyz: Kaputt"]);
    }

    /// explanation and source message stay two translatable sentences
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
        // a ten minute mix is not the same track
        let langer_mix = treffer("Die Welt zu Gast bei Feinden Mix", 600, "YouTube");
        assert!(score_candidate(&langer_mix, "Die Welt zu Gast bei Feinden", Some(167_000), "Die Welt zu Gast bei Feinden").is_none());

        // just beyond the bound it is discarded as well
        let knapp = treffer("Die Welt zu Gast bei Feinden", 167 + 31, "YouTube");
        assert!(score_candidate(&knapp, "Die Welt zu Gast bei Feinden", Some(167_000), "Die Welt zu Gast bei Feinden").is_none());

        // missing running times are covered by `unbekannte_laenge_wird_abgewertet_aber_zugelassen`
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
        // whoever searches for a remix is to get it
        let remix = treffer("Song (Remix)", 200, "YouTube");
        let a = score_candidate(&remix, "Song (Remix)", Some(200_000), "Song (Remix)").unwrap();
        let b = score_candidate(&remix, "Song", Some(200_000), "Song").unwrap();
        assert!(a < b);
    }

    #[test]
    fn bessere_quellen_haben_bei_gleichstand_vorrang() {
        // at identical length the reliability of the source decides
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
        // bandcamp names no running time, the hit stays usable anyway
        let mut ohne = treffer("Song", 0, "Bandcamp");
        ohne.duration_ms = None;
        let bandcamp = score_candidate(&ohne, "Song", Some(200_000), "Song")
            .expect("darf nicht ausgeschlossen werden");

        // a provably fitting hit wins all the same
        let genau = treffer("Song", 200, "YouTube");
        assert!(score_candidate(&genau, "Song", Some(200_000), "Song").unwrap() < bandcamp);
    }

    #[test]
    fn ein_fremder_titel_verliert_gegen_den_gesuchten() {
        // the core of the bug: without a text comparison the source alone
        // won, and bandcamp delivered an entirely different song
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
        // exactly the reported case: a remix of a different song
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
        // every hit falls through the length check, and then the first one
        // must not be taken over unchecked
        let treffer_liste = [
            treffer("Ganz anderer Song", 600, "Bandcamp"),
            treffer("Tropical Island", 600, "YouTube"),
        ];

        let reihenfolge = rank_candidates(&treffer_liste, "Tropical Island", Some(200_000), "Tropical Island");
        assert_eq!(reihenfolge.first().map(String::as_str), Some(treffer_liste[1].url.as_str()));
    }

    #[test]
    fn kuenstler_im_kanalnamen_zaehlt_mit() {
        // soundcloud often names the artist in the channel only, not in the title
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
        // this is how the list used to arrive: mixed in turn, without order
        let mut liste = vec![
            treffer("Ganz anderer Song", 200, "Bandcamp"),
            treffer("Creep", 30, "SoundCloud"),
            treffer("Radiohead - Creep", 238, "YouTube"),
        ];
        sort_by_relevance(&mut liste, "Radiohead Creep");

        let titel: Vec<&str> = liste.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titel[0], "Radiohead - Creep", "Reihenfolge: {titel:?}");
        // a 30 second excerpt is no good first suggestion …
        assert_eq!(titel[1], "Creep", "Reihenfolge: {titel:?}");
        // … but a foreign track is a worse one
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
        // the observed case: the search was "The Killers Mr. Brightside"
        let warnung = intent_warning(Some("The Killers Mr. Brightside"), &julia, true)
            .expect("Fehlgriff blieb unbemerkt");
        assert!(warnung.contains("Mr. Brightside"), "Suche fehlt: {warnung}");
        assert!(warnung.contains("Julia"), "Ergebnis fehlt: {warnung}");

        // the second observed case: the artist name sat inside the title, so
        // every searched word seemed present while the artist field carried
        // the channel name
        let getarnt = TrackMetadata {
            title: "The Killers- Mr. Brightside".into(),
            artist: "Julia".into(),
            ..Default::default()
        };
        assert!(
            intent_warning(Some("The Killers Mr. Brightside"), &getarnt, true).is_some(),
            "Künstler im Titel verdeckte den Fehlgriff"
        );

        // where the metadata search confirms the find, the weaker sign no
        // longer counts
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

        // guest artists count as well
        let mit_gast = TrackMetadata {
            title: "Money Trees".into(),
            artist: "Kendrick Lamar".into(),
            featured_artists: Some("Jay Rock".into()),
            ..Default::default()
        };
        assert!(intent_warning(Some("Kendrick Lamar Money Trees Jay Rock"), &mit_gast, true).is_none());

        // a single differing word is everyday, not a misgrasp
        let fast = TrackMetadata {
            title: "Naked".into(),
            artist: "Yeat".into(),
            ..Default::default()
        };
        assert!(intent_warning(Some("Yeat Naked Official"), &fast, true).is_none());

        // whoever searches for the title alone may not know the artist. as
        // long as the metadata search agrees, that is no misgrasp
        let nur_titel = TrackMetadata {
            title: "Wildberry Lillet".into(),
            artist: "Nina Chuba".into(),
            ..Default::default()
        };
        assert!(intent_warning(Some("Wildberry Lillet"), &nur_titel, false).is_none());

        // without an intent there is nothing to check
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

        // with a very large age bound everything stays
        assert_eq!(super::cleanup_work_dir(&basis, Duration::from_secs(3600)), 0);
        assert!(alt.exists());

        // with a bound of zero the job folder counts as old, the others do not
        assert_eq!(super::cleanup_work_dir(&basis, Duration::ZERO), 1);
        assert!(!alt.exists(), "Auftragsordner blieb liegen");
        assert!(behalten.exists(), "übernommene Titel wurden gelöscht");
        assert!(fremd.exists(), "fremder Ordner wurde gelöscht");

        let _ = std::fs::remove_dir_all(&basis);
    }

    #[test]
    fn adressen_werden_ihrer_quelle_zugeordnet() {
        // the basis of the minimum distance: accesses to the same service
        // have to be recognised as such, whatever the spelling
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
        // two accesses to the same source in sequence
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

        // different sources do not brake each other
        let start = Instant::now();
        let _c = super::acquire_slot("AndereQuelle").await;
        assert!(start.elapsed() < super::MIN_SPACING);
    }

    #[test]
    fn quellenangaben_haben_vorrang_vor_dem_videotitel() {
        let mut metadata = TrackMetadata {
            // this is how yt-dlp writes it into the file without music fields
            title: "Nina Chuba - WILDBERRY LILLET [Lyrics]".into(),
            artist: "xTheLYRICS".into(),
            ..Default::default()
        };

        // with music fields only those count
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
        // soundcloud fills `track` with the filename of the upload. in the
        // large measured run "Aphex Twin. Xtal.mp3" therefore stood in the
        // library as the song title, and the metadata search found nothing
        // for it
        let mut metadata = TrackMetadata::default();
        let quelle = super::SourceMetadata::parse(
            "Aphex Twin – Xtal.mp3\u{1f}Aphex Twin\u{1f}NA\u{1f}NA\u{1f}NA",
        );
        apply_source_metadata(&mut metadata, &quelle, Some("Aphex Twin"));
        assert_eq!(metadata.title, "Xtal");
        assert_eq!(metadata.artist, "Aphex Twin");

        // without an artist the channel name suffices as evidence
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
        // a song title may hold a separator. it is split only where the left
        // half is provably the artist
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

        // lyric channels deliver no music fields, everything stands in the title
        let leer = super::SourceMetadata::parse("NA\u{1f}NA\u{1f}NA\u{1f}NA\u{1f}NA");
        assert_eq!(leer, super::SourceMetadata::default());

        apply_source_metadata(&mut metadata, &leer, Some("xTheLYRICS"));
        assert_eq!(metadata.title, "WILDBERRY LILLET");
        assert_eq!(metadata.artist, "Nina Chuba", "Kanalname wurde übernommen");
    }

    #[test]
    fn ausweichadressen_bleiben_beim_selben_titel() {
        // "Yeat Naked" strayed all the way to "Back Home", a different song
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
        // sources write the same title differently, and the substitute must
        // not fail on that, otherwise a drm track is left without a way out
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
        // a different version is no substitute
        assert!(!plaene[0].fallbacks.contains(&plaene[2].url));
    }

    #[test]
    fn gleicher_name_bei_anderer_laenge_ist_kein_ersatz() {
        // "Naked" exists by yeat (93 s) and by kraak & smaak. without the
        // length check the wrong song landed in the library
        let plaene = super::plans_with_fallbacks(vec![
            treffer("Naked", 93, "YouTube Music"),
            treffer("Naked", 214, "SoundCloud"),
        ]);
        assert!(plaene[0].fallbacks.is_empty(), "fremder Song als Ersatz");

        // without a running time nothing can be ruled out, and bandcamp names none
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
        // without it a drm-protected hit stays a dead end download.
        // the same track at three sources, exactly the drm case
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
        // the order of the ways out follows the score
        assert_eq!(plaene[0].fallbacks[0], plaene[1].url);
    }

    #[test]
    fn youtube_music_steht_vor_den_uebrigen_quellen() {
        // the artist's release lies there, not a third party's rework
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
        // no reason to speak of a block here
        assert!(!message.contains("403"), "falsch eingeordnet: {message}");
    }

    #[test]
    fn zusaetze_in_klammern_zaehlen_mit() {
        // the measured run showed foreign versions standing in brackets
        // almost always, and those are exactly what the word comparison hides
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

        // whole words only: "edit" sits inside "Editors" and "Credits" too
        assert_eq!(version_penalty("Editors - Munich", "Munich"), 0.0);
        assert!(fremde_fassungen("Song (Credits Version)", "Song").is_empty());
        // it counts all the same, but through the general rule: a bracket
        // naming neither trivia nor a guest is a version of its own
        assert!(version_penalty("Song (Credits Version)", "Song") > 0.0);

        // whoever searches for the version gets it
        assert_eq!(version_penalty("Song (Remix)", "Song (Remix)"), 0.0);
    }

    /// foreign versions must not set the norm.
    ///
    /// under "Bicep Glue" stand seven bootlegs and edits, each of a length of
    /// its own, and only one official recording. their middle became the
    /// majority, the original was charged for deviating from it, and a
    /// mashup naming no length at all ended up first.
    #[test]
    fn fremde_fassungen_bestimmen_die_mehrheitslaenge_nicht() {
        let liste = [
            treffer("BICEP | GLUE (Official Video)", 285, "YouTube"),
            treffer("Bicep - Glue", 285, "SoundCloud"),
            treffer("BICEP GLUE", 284, "YouTube Music"),
            treffer("Bicep - Glue (Livsey Bootleg)", 203, "Bandcamp"),
            treffer("Bicep - Glue (OAO Edit)", 199, "Bandcamp"),
            treffer("Bicep - Glue (Aand Remix)", 328, "Bandcamp"),
            treffer("Love Bicep Glue", 205, "Bandcamp"),
        ];
        let mehrheit = consensus_duration_ms(&liste, "Bicep Glue");
        assert_eq!(
            mehrheit,
            Some(285_000),
            "die Bootlegs haben die Mehrheitslänge bestimmt"
        );

        // and the official recording is charged nothing for it
        assert_eq!(consensus_penalty(&liste[0], mehrheit), 0.0);
        // the edits are, they are a different recording
        assert!(consensus_penalty(&liste[3], mehrheit) > 0.0);

        // where too few clean hits stand, nothing is stated at all — that is
        // better than letting the foreign versions decide
        assert!(consensus_duration_ms(&liste[3..], "Bicep Glue").is_none());
    }

    #[test]
    fn mehrheit_der_quellen_bestimmt_die_laenge() {
        // four sources carry the same recording, one an excerpt
        let liste = [
            treffer("Money Trees", 91, "SoundCloud"),
            treffer("Money Trees", 387, "YouTube"),
            treffer("Money Trees", 395, "SoundCloud"),
            treffer("Money Trees", 387, "YouTube"),
        ];
        let consensus = consensus_duration_ms(&liste, "Money Trees").expect("Mehrheit gefunden");
        assert!(
            (consensus - 387_000).abs() < 10_000,
            "unerwartete Länge: {consensus} ms"
        );

        // too few values: better no statement than a poor one
        assert!(consensus_duration_ms(&liste[..2], "Money Trees").is_none());
    }

    #[test]
    fn angeschnittene_uploads_verlieren_gegen_die_mehrheit() {
        // the reported case from the measured run: a 91 second upload stood
        // above the complete recording
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
        // at equal fit the order stays as the mixing produced it, and the
        // variety is not lost
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
        // single tracks stay single, playlist parameter or not
        assert!(!is_collection_url("https://www.youtube.com/watch?v=abc"));
        assert!(!is_collection_url("https://www.youtube.com/watch?v=abc&list=PL123"));
        assert!(!is_collection_url("https://youtu.be/abc"));
        assert!(!is_collection_url("https://soundcloud.com/kuenstler/titel"));

        // real collections are unfolded
        assert!(is_collection_url("https://www.youtube.com/playlist?list=PL123"));
        assert!(is_collection_url("https://soundcloud.com/kuenstler/sets/mein-album"));
        assert!(is_collection_url("https://band.bandcamp.com/album/mein-album"));
    }

    #[test]
    fn leere_ausgabe_stuerzt_nicht_ab() {
        assert!(explain_failure("").contains("unbekannter Fehler"));
    }
}
