//! reading and writing audio tags (id3, vorbis, mp4 and so on) through `lofty`.

use crate::models::TrackMetadata;
use anyhow::{anyhow, Result};
use base64::Engine;
use lofty::config::WriteOptions;
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::prelude::*;
use lofty::probe::read_from_path;
use lofty::tag::Tag;
use std::path::Path;
use crate::fehler;

pub const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "m4a", "mp4", "aac", "ogg", "oga", "opus", "wav", "wv", "aiff", "aif", "ape",
    "mpc",
];

/// whether the extension is one of the audio formats robify reads.
pub fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// what reading a file yields: tags plus technical details.
pub struct FileTags {
    pub metadata: TrackMetadata,
    pub duration_ms: i64,
    pub format: String,
    pub cover: Option<(Vec<u8>, String)>,
}

// pulls the year out of the various date fields.
//
// yt-dlp writes the date compactly as `20240111`. without the cut to four
// digits that number landed in the library unchanged as the year
fn parse_year(value: &str) -> Option<i64> {
    let mut digits: String = value.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.len() > 4 {
        digits.truncate(4);
    }
    digits.parse::<i64>().ok().filter(|y| *y > 0)
}

/// reads tags, duration and cover out of an audio file.
pub fn read(path: &Path) -> Result<FileTags> {
    let tagged = read_from_path(path)?;
    let duration_ms = tagged.properties().duration().as_millis() as i64;
    let format = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let fallback_title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Unbekannter Titel")
        .to_string();

    let mut metadata = TrackMetadata {
        title: fallback_title,
        artist: "Unbekannter Künstler".into(),
        album: String::new(),
        ..Default::default()
    };
    let mut cover = None;

    if let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) {
        if let Some(v) = tag.title().filter(|v| !v.trim().is_empty()) {
            metadata.title = v.into_owned();
        }
        if let Some(v) = tag.artist().filter(|v| !v.trim().is_empty()) {
            metadata.artist = v.into_owned();
        }
        if let Some(v) = tag.album() {
            metadata.album = v.into_owned();
        }
        metadata.album_artist = tag.get_string(ItemKey::AlbumArtist).map(str::to_string);
        // merge artists stored more than once (id3v2.4, vorbis)
        let all: Vec<&str> = tag.get_strings(ItemKey::TrackArtist).collect();
        if all.len() > 1 {
            metadata.artist = all.join("; ");
        }
        metadata.genre = tag.genre().map(|g| g.into_owned());
        metadata.track_no = tag.track().map(i64::from);
        metadata.disc_no = tag.disk().map(i64::from);
        metadata.year = tag
            .get_string(ItemKey::Year)
            .or_else(|| tag.get_string(ItemKey::RecordingDate))
            .or_else(|| tag.get_string(ItemKey::ReleaseDate))
            .and_then(parse_year);

        if let Some(lyrics) = tag
            .get_string(ItemKey::Lyrics)
            .or_else(|| tag.get_string(ItemKey::UnsyncLyrics))
        {
            // lrc is recognisable by timestamps of the form [mm:ss.xx]
            if lyrics.contains('[') && lyrics.contains(':') && lyrics.contains(']') {
                metadata.lyrics_synced = Some(lyrics.to_string());
            } else {
                metadata.lyrics_plain = Some(lyrics.to_string());
            }
        }

        if let Some(pic) = tag
            .get_picture_type(PictureType::CoverFront)
            .or_else(|| tag.pictures().first())
        {
            let mime = pic
                .mime_type()
                .map(|m| m.to_string())
                .unwrap_or_else(|| "image/jpeg".into());
            cover = Some((pic.data().to_vec(), mime));
        }
    }

    Ok(FileTags {
        metadata,
        duration_ms,
        format,
        cover,
    })
}

fn mime_from_str(mime: &str) -> MimeType {
    match mime.to_ascii_lowercase().as_str() {
        "image/png" => MimeType::Png,
        "image/gif" => MimeType::Gif,
        "image/bmp" => MimeType::Bmp,
        "image/tiff" => MimeType::Tiff,
        _ => MimeType::Jpeg,
    }
}

/// writes the edited metadata back into the file.
pub fn write(path: &Path, meta: &TrackMetadata) -> Result<()> {
    let mut tagged = read_from_path(path)?;

    let tag_type = tagged
        .primary_tag()
        .map(|t| t.tag_type())
        .unwrap_or_else(|| tagged.file_type().primary_tag_type());

    if tagged.tag(tag_type).is_none() {
        tagged.insert_tag(Tag::new(tag_type));
    }
    let tag = tagged
        .tag_mut(tag_type)
        .ok_or_else(|| anyhow!(fehler!("Konnte kein Tag für {0} anlegen", path.display())))?;

    tag.set_title(meta.title.clone());
    // guest artists go into the artist field as "feat." so other players see
    // them too. reading splits it apart again
    tag.set_artist(match meta.featured_artists.as_deref() {
        Some(featured) if !featured.trim().is_empty() => {
            format!("{} feat. {}", meta.artist.trim(), featured.trim())
        }
        _ => meta.artist.clone(),
    });
    if meta.album.trim().is_empty() {
        tag.remove_album();
    } else {
        tag.set_album(meta.album.clone());
    }

    match meta.album_artist.as_deref() {
        Some(a) if !a.trim().is_empty() => {
            tag.insert_text(ItemKey::AlbumArtist, a.to_string());
        }
        _ => tag.remove_key(ItemKey::AlbumArtist),
    }
    match meta.genre.as_deref() {
        Some(g) if !g.trim().is_empty() => tag.set_genre(g.to_string()),
        _ => tag.remove_genre(),
    }
    match meta.year {
        Some(y) if y > 0 => {
            tag.insert_text(ItemKey::Year, y.to_string());
            tag.insert_text(ItemKey::RecordingDate, y.to_string());
        }
        _ => {
            tag.remove_key(ItemKey::Year);
            tag.remove_key(ItemKey::RecordingDate);
        }
    }
    match meta.track_no {
        Some(n) if n > 0 => tag.set_track(n as u32),
        _ => tag.remove_track(),
    }
    match meta.disc_no {
        Some(n) if n > 0 => tag.set_disk(n as u32),
        _ => tag.remove_disk(),
    }

    // synced lyrics win so the timestamps survive
    let lyrics = meta
        .lyrics_synced
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            meta.lyrics_plain
                .as_deref()
                .filter(|s| !s.trim().is_empty())
        });
    match lyrics {
        Some(l) => {
            // id3v2 has no `Lyrics` field, uslt (`UnsyncLyrics`) is the
            // usual place there. vorbis and mp4 take `Lyrics`
            if !tag.insert_text(ItemKey::Lyrics, l.to_string()) {
                tag.insert_text(ItemKey::UnsyncLyrics, l.to_string());
            }
        }
        None => {
            tag.remove_key(ItemKey::Lyrics);
            tag.remove_key(ItemKey::UnsyncLyrics);
        }
    }

    if let Some(b64) = meta.cover_base64.as_deref().filter(|s| !s.is_empty()) {
        let data = base64::engine::general_purpose::STANDARD.decode(b64)?;
        let mime = meta.cover_mime.as_deref().unwrap_or("image/jpeg");
        let picture = Picture::unchecked(data)
            .pic_type(PictureType::CoverFront)
            .mime_type(mime_from_str(mime))
            .build();
        while tag.picture_count() > 0 {
            tag.remove_picture(0);
        }
        tag.push_picture(picture);
    }

    tagged.save_to_path(path, WriteOptions::default())?;
    Ok(())
}
