//! Import lokaler Dateien und Ordner in die Bibliothek.

use crate::library::{self, TrackInsert};
use crate::models::ReleaseType;
use crate::tags;
use anyhow::Result;
use parking_lot::Mutex;
use rusqlite::Connection;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};
use walkdir::WalkDir;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub scanned: usize,
    pub imported: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanProgress {
    current: usize,
    total: usize,
    file: String,
}

pub fn collect_audio_files(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for root in roots {
        if root.is_file() {
            if tags::is_audio_file(root) {
                files.push(root.clone());
            }
            continue;
        }
        for entry in WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file() && tags::is_audio_file(path) {
                files.push(path.to_path_buf());
            }
        }
    }
    files.sort();
    files.dedup();
    files
}

/// Importiert eine einzelne Datei. Cover und Lyrics aus den Tags werden
/// mit übernommen, sofern das Release noch keines hat.
pub fn import_file(conn: &Connection, path: &Path, source: Option<&str>) -> Result<i64> {
    let file_tags = tags::read(path)?;
    let meta = &file_tags.metadata;

    let track_id = library::upsert_track(
        conn,
        &TrackInsert {
            path: path.to_string_lossy().to_string(),
            title: meta.title.clone(),
            artist: meta.artist.clone(),
            featured_artists: meta.featured_artists.clone(),
            album: if meta.album.trim().is_empty() {
                None
            } else {
                Some(meta.album.clone())
            },
            album_artist: meta.album_artist.clone(),
            release_type: meta.release_type.as_deref().map(ReleaseType::parse),
            track_no: meta.track_no,
            disc_no: meta.disc_no,
            duration_ms: file_tags.duration_ms,
            genre: meta.genre.clone(),
            year: meta.year,
            format: file_tags.format.clone(),
            source: source.map(str::to_string),
            source_url: None,
        },
    )?;

    if let Some((data, mime)) = file_tags.cover {
        let album_id: i64 = conn.query_row(
            "SELECT album_id FROM tracks WHERE id = ?1",
            [track_id],
            |r| r.get(0),
        )?;
        if library::album_cover(conn, album_id)?.is_none() {
            library::set_album_cover(conn, album_id, &data, &mime)?;
        }
    }

    if meta.lyrics_synced.is_some() || meta.lyrics_plain.is_some() {
        library::set_lyrics(
            conn,
            track_id,
            meta.lyrics_synced.as_deref(),
            meta.lyrics_plain.as_deref(),
            Some("datei"),
        )?;
    }

    Ok(track_id)
}

pub fn scan(
    app: &AppHandle,
    conn: &Mutex<Connection>,
    roots: Vec<PathBuf>,
) -> Result<ScanResult> {
    let files = collect_audio_files(&roots);
    let total = files.len();
    let mut result = ScanResult {
        scanned: total,
        ..Default::default()
    };

    for (index, path) in files.iter().enumerate() {
        let _ = app.emit(
            "library:scan-progress",
            ScanProgress {
                current: index + 1,
                total,
                file: path.file_name().unwrap_or_default().to_string_lossy().into(),
            },
        );

        let guard = conn.lock();
        match import_file(&guard, path, Some("lokal")) {
            Ok(track_id) => {
                // `upsert_track` gibt bei einem Doppelten die Kennung des
                // vorhandenen Titels zurück. Erkennbar am Pfad: Er zeigt dann
                // auf die andere Datei.
                let gleicher_pfad = guard
                    .query_row("SELECT path FROM tracks WHERE id = ?1", [track_id], |r| {
                        r.get::<_, String>(0)
                    })
                    .map(|vorhanden| vorhanden == path.to_string_lossy())
                    .unwrap_or(true);
                if gleicher_pfad {
                    result.imported += 1;
                } else {
                    result.skipped += 1;
                }
            }
            Err(err) => {
                result.skipped += 1;
                if result.errors.len() < 25 {
                    result.errors.push(format!("{}: {err}", path.display()));
                }
            }
        }
    }

    let guard = conn.lock();
    library::refresh_release_types(&guard)?;
    drop(guard);

    let _ = app.emit("library:changed", ());
    Ok(result)
}
