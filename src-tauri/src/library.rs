//! everything the library holds: artists, albums, tracks, playlists and the
//! queries the ui reads them through.

use crate::db::{key_of, now};
use crate::models::*;
use anyhow::{anyhow, Result};
use rusqlite::{params, Connection, OptionalExtension, Row};
use crate::fehler;

const TRACK_SELECT: &str = r#"
SELECT t.id, t.path, t.title, t.artist_id, ar.name, t.album_id, al.title, al.release_type,
       t.track_no, t.disc_no, t.duration_ms, t.genre, t.year, t.format,
       (al.cover IS NOT NULL),
       EXISTS (SELECT 1 FROM lyrics l WHERE l.track_id = t.id
                AND (COALESCE(l.synced, '') <> '' OR COALESCE(l.plain, '') <> '')),
       t.added_at,
       (SELECT COUNT(*) FROM plays p WHERE p.track_id = t.id),
       t.favorite, t.source, (t.deleted_at IS NOT NULL)
FROM tracks t
JOIN artists ar ON ar.id = t.artist_id
JOIN albums  al ON al.id = t.album_id
"#;

/// separator for several artists inside one text field.
const ARTIST_SEPARATOR: char = ';';

/// words that guest artists follow.
const FEATURE_MARKERS: [&str; 5] = [" feat. ", " feat ", " ft. ", " ft ", " featuring "];

/// splits "A; B; C" into single names. commas and ampersands stay untouched,
/// otherwise band names such as "Earth, Wind & Fire" would fall apart.
pub fn split_artists(value: &str) -> Vec<String> {
    value
        .split(ARTIST_SEPARATOR)
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

pub fn join_artists(names: &[String]) -> String {
    names.join("; ")
}

/// splits an artist field into lead and guest artists. a "feat." inside it
/// pushes everything after it over to the guests.
pub fn parse_artist_field(value: &str) -> (Vec<String>, Vec<String>) {
    let lower = value.to_lowercase();
    for marker in FEATURE_MARKERS {
        if let Some(index) = lower.find(marker) {
            let main = &value[..index];
            let featured = &value[index + marker.len()..];
            return (split_artists(main), split_artists(featured));
        }
    }
    (split_artists(value), Vec::new())
}

/// hints about the production that have no business in a song title.
///
/// guest artists stand in brackets as well and are therefore kept, only what
/// consists of nothing but these words is removed.
const TITLE_TAGS: [&str; 21] = [
    "official",
    // german uploads write "(Offizielles Video)"
    "offiziell",
    "offizielle",
    "offizielles",
    "musikvideo",
    "songtext",
    "video",
    "audio",
    "lyric",
    "lyrics",
    "visualizer",
    "music",
    "hd",
    "hq",
    "4k",
    "full",
    "song",
    "clip",
    "mv",
    "explicit",
    "remastered",
];

/// strips bracketed suffixes that only describe the production.
fn strip_title_tags(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;

    while let Some(start) = rest.find(['(', '[']) {
        let close = if rest[start..].starts_with('(') { ')' } else { ']' };
        let Some(offset) = rest[start..].find(close) else {
            break;
        };
        let inner = &rest[start + 1..start + offset];

        out.push_str(&rest[..start]);
        // throw away only where nothing but production words stand inside
        let nur_hinweise = !inner.trim().is_empty()
            && crate::online::normalize_words(inner)
                .split(' ')
                .filter(|w| !w.is_empty())
                .all(|wort| TITLE_TAGS.contains(&wort));
        if !nur_hinweise {
            out.push_str(&rest[start..start + offset + 1]);
        }
        rest = &rest[start + offset + 1..];
    }
    out.push_str(rest);

    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// splits a video title of the form "artist - title".
///
/// needed because lyric, sampler and repost channels supply no music fields:
/// both sit in the title there, and without splitting the entire video title
/// lands in the library as the song title and the channel name as the artist.
///
/// where the channel name stands on the right ("Oft Gefragt -
/// AnnenMayKantereit") the halves are swapped, and it turns them around.
pub fn split_video_title(raw: &str, uploader: Option<&str>) -> Option<(String, String)> {
    let cleaned = strip_title_tags(raw);

    // with a space on one side only too: "The Killers- Mr. Brightside" stayed
    // unsplit otherwise and the channel name became the artist. a bare hyphen
    // is deliberately absent, it sits inside "Jay-Z" and "T-Pain"
    for separator in [
        " - ", " – ", ", ", " -- ", " | ", " ~ ", " • ", "- ", "– ", " -", " –",
    ] {
        let Some((left, right)) = cleaned.split_once(separator) else {
            continue;
        };
        let (left, right) = (left.trim(), right.trim());
        if left.is_empty() || right.is_empty() {
            continue;
        }

        let passt = |seite: &str| {
            uploader.is_some_and(|kanal| {
                let kanal = crate::online::normalize_for_match(&channel_to_artist(kanal));
                !kanal.is_empty()
                    && crate::online::contains_word_sequence(
                        &crate::online::normalize_for_match(seite),
                        &kanal,
                    )
            })
        };

        return if passt(right) && !passt(left) {
            Some((right.to_string(), left.to_string()))
        } else {
            Some((left.to_string(), right.to_string()))
        };
    }
    None
}

/// turns a channel name into the presumed artist name.
///
/// youtube appends " - Topic" to automatically generated channels, and label
/// channels often end in "VEVO".
fn channel_to_artist(name: &str) -> String {
    let mut cleaned = name.trim();
    for suffix in [" - Topic", " - topic", " - TOPIC"] {
        cleaned = cleaned.strip_suffix(suffix).unwrap_or(cleaned);
    }
    let mut cleaned = cleaned.trim().to_string();

    if cleaned.to_lowercase().ends_with("vevo") && cleaned.len() > 4 {
        let cut = cleaned.len() - 4;
        if cleaned.is_char_boundary(cut) {
            cleaned = cleaned[..cut].trim().to_string();
        }
    }
    cleaned
}

/// moves the artist whose account published the track to the front, the
/// others become guest artists.
///
/// takes effect only where the channel actually matches one of the
/// participants. with label, sampler or repost channels the order stays as
/// the metadata source delivered it.
pub fn promote_uploader(
    artist_field: &str,
    featured_field: Option<&str>,
    uploader: &str,
) -> Option<(String, Option<String>)> {
    let uploader = channel_to_artist(uploader);
    if uploader.is_empty() {
        return None;
    }

    let mut everyone = split_artists(artist_field);
    everyone.extend(featured_field.map(split_artists).unwrap_or_default());
    if everyone.len() < 2 {
        return None;
    }

    let position = everyone.iter().position(|name| {
        crate::online::looks_like_same(name, &uploader)
            // channels such as "PA69 Official" carry the name as a word sequence
            || crate::online::contains_word_sequence(
                &crate::online::normalize_for_match(&uploader),
                &crate::online::normalize_for_match(name),
            )
    })?;

    let main = everyone.remove(position);
    let featured = everyone;
    Some((
        main,
        (!featured.is_empty()).then(|| join_artists(&featured)),
    ))
}

fn map_track(row: &Row) -> rusqlite::Result<Track> {
    Ok(Track {
        id: row.get(0)?,
        path: row.get(1)?,
        title: row.get(2)?,
        artist_id: row.get(3)?,
        artist_name: row.get(4)?,
        artists: Vec::new(),
        album_id: row.get(5)?,
        album_title: row.get(6)?,
        release_type: ReleaseType::parse(&row.get::<_, String>(7)?),
        track_no: row.get(8)?,
        disc_no: row.get(9)?,
        duration_ms: row.get(10)?,
        genre: row.get(11)?,
        year: row.get(12)?,
        format: row.get(13)?,
        has_cover: row.get::<_, i64>(14)? != 0,
        has_lyrics: row.get::<_, i64>(15)? != 0,
        added_at: row.get(16)?,
        play_count: row.get(17)?,
        favorite: row.get::<_, i64>(18)? != 0,
        source: row.get(19)?,
        deleted: row.get::<_, i64>(20)? != 0,
    })
}

// loads the participants for a whole result list in one query, otherwise
// there would be a round trip to the database per track
fn attach_artists(conn: &Connection, tracks: &mut [Track]) -> Result<()> {
    if tracks.is_empty() {
        return Ok(());
    }

    let placeholders = std::iter::repeat_n("?", tracks.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT ta.track_id, ar.id, ar.name, ta.role
         FROM track_artists ta JOIN artists ar ON ar.id = ta.artist_id
         WHERE ta.track_id IN ({placeholders})
         ORDER BY ta.track_id,
                  CASE ta.role WHEN 'main' THEN 0 ELSE 1 END,
                  ta.position"
    );

    let ids: Vec<i64> = tracks.iter().map(|t| t.id).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(ids), |r| {
        Ok((
            r.get::<_, i64>(0)?,
            TrackArtist {
                id: r.get(1)?,
                name: r.get(2)?,
                role: r.get(3)?,
            },
        ))
    })?;

    let mut by_track: std::collections::HashMap<i64, Vec<TrackArtist>> =
        std::collections::HashMap::new();
    for row in rows {
        let (track_id, artist) = row?;
        by_track.entry(track_id).or_default().push(artist);
    }

    for track in tracks.iter_mut() {
        track.artists = by_track.remove(&track.id).unwrap_or_else(|| {
            // only comes up where the link is missing, then the lead artist
            vec![TrackArtist {
                id: track.artist_id,
                name: track.artist_name.clone(),
                role: "main".into(),
            }]
        });
    }
    Ok(())
}

/// rewrites the participants of a track.
pub fn set_track_artists(
    conn: &Connection,
    track_id: i64,
    main: &[String],
    featured: &[String],
) -> Result<i64> {
    conn.execute("DELETE FROM track_artists WHERE track_id = ?1", [track_id])?;

    let mut primary_id = None;
    for (position, name) in main.iter().enumerate() {
        let artist_id = upsert_artist(conn, name)?;
        primary_id.get_or_insert(artist_id);
        conn.execute(
            "INSERT OR IGNORE INTO track_artists (track_id, artist_id, role, position)
             VALUES (?1, ?2, 'main', ?3)",
            params![track_id, artist_id, position as i64],
        )?;
    }
    for (position, name) in featured.iter().enumerate() {
        let artist_id = upsert_artist(conn, name)?;
        primary_id.get_or_insert(artist_id);
        conn.execute(
            "INSERT OR IGNORE INTO track_artists (track_id, artist_id, role, position)
             VALUES (?1, ?2, 'feature', ?3)",
            params![track_id, artist_id, position as i64],
        )?;
    }

    let primary = match primary_id {
        Some(id) => id,
        None => upsert_artist(conn, "Unbekannter Künstler")?,
    };
    conn.execute(
        "UPDATE tracks SET artist_id = ?2 WHERE id = ?1",
        params![track_id, primary],
    )?;
    Ok(primary)
}

// --- upserts ---

pub fn upsert_artist(conn: &Connection, name: &str) -> Result<i64> {
    let name = crate::db::clean_text(name);
    let name = if name.is_empty() {
        "Unbekannter Künstler"
    } else {
        name.as_str()
    };
    let key = key_of(name);
    if let Some(id) = conn
        .query_row("SELECT id FROM artists WHERE name_key = ?1", [&key], |r| {
            r.get::<_, i64>(0)
        })
        .optional()?
    {
        return Ok(id);
    }

    // sort name without a leading article so "The Beatles" stands under b
    let sort_name = strip_leading_article(name);
    conn.execute(
        "INSERT INTO artists (name, sort_name, name_key, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![name, sort_name, key, now()],
    )?;
    Ok(conn.last_insert_rowid())
}

fn strip_leading_article(name: &str) -> String {
    for article in ["the ", "die ", "der ", "das ", "a ", "an "] {
        if name.to_lowercase().starts_with(article) {
            return name[article.len()..].to_string();
        }
    }
    name.to_string()
}

pub fn upsert_album(
    conn: &Connection,
    artist_id: i64,
    title: &str,
    year: Option<i64>,
    release_type: Option<ReleaseType>,
) -> Result<i64> {
    // invisible characters from foreign titles fly out, otherwise two
    // seemingly equal albums stand next to each other
    let title = crate::db::clean_text(title);
    let title = if title.is_empty() {
        "Unbekanntes Album"
    } else {
        title.as_str()
    };
    let key = key_of(title);
    let existing = conn
        .query_row(
            "SELECT id FROM albums WHERE artist_id = ?1 AND title_key = ?2",
            params![artist_id, &key],
            |r| r.get::<_, i64>(0),
        )
        .optional()?;

    if let Some(id) = existing {
        if let Some(y) = year {
            conn.execute(
                "UPDATE albums SET year = COALESCE(year, ?2) WHERE id = ?1",
                params![id, y],
            )?;
        }
        if let Some(rt) = release_type {
            // never over a decision of the user: theirs stands above what a
            // source says
            conn.execute(
                "UPDATE albums SET release_type = ?2, release_type_locked = ?3
                 WHERE id = ?1 AND release_type_locked <> ?4",
                params![id, rt.as_str(), ART_AUS_QUELLE, ART_VOM_NUTZER],
            )?;
        }
        return Ok(id);
    }

    let (rt, locked) = match release_type {
        Some(rt) => (rt, ART_AUS_QUELLE),
        // nothing known yet. the classification by the tracks that are here
        // follows right after the track is in
        None => (ReleaseType::Album, ART_GERATEN),
    };
    conn.execute(
        "INSERT INTO albums (artist_id, title, title_key, release_type, release_type_locked, year, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![artist_id, title, key, rt.as_str(), locked, year, now()],
    )?;
    Ok(conn.last_insert_rowid())
}

pub struct TrackInsert {
    pub path: String,
    pub title: String,
    /// raw artist field, may hold several names and a "feat.".
    pub artist: String,
    /// additional guest artists, separated by semicolons.
    pub featured_artists: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub release_type: Option<ReleaseType>,
    pub track_no: Option<i64>,
    pub disc_no: Option<i64>,
    pub duration_ms: i64,
    pub genre: Option<String>,
    pub year: Option<i64>,
    pub format: String,
    pub source: Option<String>,
    pub source_url: Option<String>,
}

// looks for a previously removed track of the same artist with the same name.
//
// the comparison runs through `db::key_of`, so without case, punctuation and
// invisible characters. where several exist, the most recently removed one
// wins
fn entfernten_titel_finden(
    conn: &Connection,
    artist_id: i64,
    title: &str,
) -> Result<Option<i64>> {
    let gesucht = crate::db::key_of(title);
    if gesucht.is_empty() {
        return Ok(None);
    }

    let mut stmt = conn.prepare(
        "SELECT id, title FROM tracks
         WHERE artist_id = ?1 AND deleted_at IS NOT NULL
         ORDER BY deleted_at DESC",
    )?;
    let treffer = stmt
        .query_map([artist_id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(treffer
        .into_iter()
        .find(|(_, vorhanden)| crate::db::key_of(vorhanden) == gesucht)
        .map(|(id, _)| id))
}

// looks for an already present track of the same artist with the same name
// and a similar length.
//
// the name alone does not do: album version and single version carry the same
// name and are different recordings. only together with the running time does
// it become the same track, hence the tolerance of five seconds, which covers
// differences in encoding and editing without catching a remix.
//
// where the length is unknown (zero) nothing is merged: better a duplicate
// row than a swallowed track
fn doppelten_titel_finden(
    conn: &Connection,
    artist_id: i64,
    title: &str,
    duration_ms: i64,
    eigener_pfad: &str,
) -> Result<Option<i64>> {
    let gesucht = crate::db::key_of(title);
    if gesucht.is_empty() || duration_ms <= 0 {
        return Ok(None);
    }

    let mut stmt = conn.prepare(
        "SELECT id, title, duration_ms, path FROM tracks
         WHERE artist_id = ?1 AND deleted_at IS NULL",
    )?;
    let treffer = stmt
        .query_map([artist_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(treffer
        .into_iter()
        .find(|(_, vorhandener_titel, laenge, pfad)| {
            pfad != eigener_pfad
                && *laenge > 0
                && (laenge - duration_ms).abs() <= 5_000
                && crate::db::key_of(vorhandener_titel) == gesucht
        })
        .map(|(id, _, _, _)| id))
}

pub fn upsert_track(conn: &Connection, t: &TrackInsert) -> Result<i64> {
    let (mut main, mut featured) = parse_artist_field(&t.artist);
    if main.is_empty() {
        main.push("Unbekannter Künstler".into());
    }
    if let Some(extra) = t.featured_artists.as_deref() {
        for name in split_artists(extra) {
            if !featured.contains(&name) {
                featured.push(name);
            }
        }
    }
    // whoever is already a lead artist does not show up as a guest as well
    featured.retain(|name| !main.contains(name));

    let track_artist_id = upsert_artist(conn, &main[0])?;
    let album_artist_id = match t.album_artist.as_deref() {
        Some(a) if !a.trim().is_empty() => upsert_artist(conn, a)?,
        _ => track_artist_id,
    };

    let (album_title, release_type) = match t.album.as_deref() {
        Some(a) if !a.trim().is_empty() => (a.to_string(), t.release_type),
        _ => (t.title.clone(), Some(ReleaseType::Single)),
    };
    let album_id = upsert_album(conn, album_artist_id, &album_title, t.year, release_type)?;

    // fasten it to the path first, then to a previously removed track of the
    // same artist. the second case is the important one: whoever loads a
    // deleted track again gets a different file but is to keep their
    // listening history. without this reach a second row would appear and the
    // old plays would hang on the corpse
    let existing = conn
        .query_row("SELECT id FROM tracks WHERE path = ?1", [&t.path], |r| {
            r.get::<_, i64>(0)
        })
        .optional()?
        .or(entfernten_titel_finden(conn, track_artist_id, &t.title)?);

    if let Some(id) = existing {
        set_track_artists(conn, id, &main, &featured)?;
        conn.execute(
            "UPDATE tracks SET path = ?2, title = ?3, artist_id = ?4, album_id = ?5,
                    track_no = ?6, disc_no = ?7, duration_ms = ?8, genre = ?9,
                    year = ?10, format = ?11, deleted_at = NULL
             WHERE id = ?1",
            params![
                id,
                t.path,
                t.title,
                track_artist_id,
                album_id,
                t.track_no,
                t.disc_no,
                t.duration_ms,
                t.genre,
                t.year,
                t.format
            ],
        )?;
        return Ok(id);
    }

    // the same track, a different file: do not take it in a second time. that
    // happens when importing a copy and when loading a track that already
    // lies in the library
    if let Some(id) =
        doppelten_titel_finden(conn, track_artist_id, &t.title, t.duration_ms, &t.path)?
    {
        return Ok(id);
    }

    conn.execute(
        "INSERT INTO tracks (path, title, artist_id, album_id, track_no, disc_no, duration_ms,
                             genre, year, format, source, source_url, added_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            t.path,
            t.title,
            track_artist_id,
            album_id,
            t.track_no,
            t.disc_no,
            t.duration_ms,
            t.genre,
            t.year,
            t.format,
            t.source,
            t.source_url,
            now()
        ],
    )?;
    let track_id = conn.last_insert_rowid();
    set_track_artists(conn, track_id, &main, &featured)?;
    // right here and not only at the next folder scan: whoever downloads a
    // track sees its release on the artist page a moment later, and it is to
    // stand under the right heading then
    refresh_release_type(conn, album_id)?;
    Ok(track_id)
}

/// where the kind of a release comes from.
///
/// one column, three meanings, and mixing them up was the fault: the import
/// wrote its guess with the same lock the user gets, and from then on nothing
/// corrected it. every downloaded track carried an album name, therefore
/// counted as an album, and stood as one for good.
///
/// guessed is revised as the library grows. what a source said stays. what
/// the user set stands above both.
pub const ART_GERATEN: i64 = 0;
pub const ART_VOM_NUTZER: i64 = 1;
pub const ART_AUS_QUELLE: i64 = 2;

/// classifies one album by the tracks lying here, where nothing better is
/// known.
///
/// called after every track that arrives, so the kind is right at once
/// instead of only after the next folder scan.
pub fn refresh_release_type(conn: &Connection, album_id: i64) -> Result<()> {
    let art: i64 = conn.query_row(
        "SELECT release_type_locked FROM albums WHERE id = ?1",
        [album_id],
        |r| r.get(0),
    )?;
    if art != ART_GERATEN {
        return Ok(());
    }
    conn.execute(
        "UPDATE albums SET release_type = ?2 WHERE id = ?1",
        params![album_id, geschaetzte_art(conn, album_id)?.as_str()],
    )?;
    Ok(())
}

/// how large a release is, judged by what lies here.
///
/// not the number of tracks alone. whoever holds track nine holds a piece of
/// at least nine, and that number stands in the file: it turns three tracks
/// out of a record of twelve from an apparent ep into what it is. the bound
/// is free and needs no source.
fn geschaetzte_art(conn: &Connection, album_id: i64) -> Result<ReleaseType> {
    let (anzahl, hoechste): (i64, i64) = conn.query_row(
        "SELECT COUNT(*), COALESCE(MAX(track_no), 0) FROM tracks WHERE album_id = ?1",
        [album_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok(ReleaseType::from_track_count(anzahl.max(hoechste)))
}

/// releases whose kind is still only guessed, with the artist to ask under.
///
/// what carries the name of its own track is left out: that is what a track
/// without an album is called here, and it is a single by construction.
pub fn releases_needing_kind(conn: &Connection, limit: i64) -> Result<Vec<(i64, String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT al.id, ar.name, al.title
           FROM albums al
           JOIN artists ar ON ar.id = al.artist_id
          WHERE al.release_type_locked = 0
            AND TRIM(al.title) <> ''
            AND al.id NOT IN (
                SELECT album_id FROM tracks WHERE title = (
                    SELECT title FROM albums WHERE id = album_id
                )
            )
          ORDER BY al.created_at DESC
          LIMIT ?1",
    )?;
    let rows = stmt
        .query_map([limit], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// writes down what a source said about the kind of a release.
pub fn set_release_kind(conn: &Connection, album_id: i64, art: ReleaseType) -> Result<()> {
    conn.execute(
        "UPDATE albums SET release_type = ?2, release_type_locked = ?3
         WHERE id = ?1 AND release_type_locked <> ?4",
        params![album_id, art.as_str(), ART_AUS_QUELLE, ART_VOM_NUTZER],
    )?;
    Ok(())
}

/// classifies every release not set by hand from its track count.
pub fn refresh_release_types(conn: &Connection) -> Result<()> {
    let mut stmt = conn.prepare(
        "SELECT id FROM albums WHERE release_type_locked = 0", // ART_GERATEN
    )?;
    let rows: Vec<i64> = stmt
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    drop(stmt);

    for album_id in rows {
        refresh_release_type(conn, album_id)?;
    }
    Ok(())
}

/// removes artists and albums with no tracks left.
pub fn prune_empty(conn: &Connection) -> Result<()> {
    conn.execute(
        "DELETE FROM albums WHERE id NOT IN (SELECT DISTINCT album_id FROM tracks)",
        [],
    )?;
    conn.execute(
        "DELETE FROM artists WHERE id NOT IN (SELECT artist_id FROM tracks)
                               AND id NOT IN (SELECT artist_id FROM albums)
                               AND id NOT IN (SELECT artist_id FROM track_artists)",
        [],
    )?;
    Ok(())
}

// --- queries ---

pub fn list_tracks(conn: &Connection, search: Option<&str>, limit: i64) -> Result<Vec<Track>> {
    let sql = format!(
        "{TRACK_SELECT}
         WHERE t.deleted_at IS NULL
           AND (?1 = '' OR t.title LIKE ?2 OR ar.name LIKE ?2 OR al.title LIKE ?2)
         ORDER BY t.added_at DESC, t.id DESC LIMIT ?3"
    );
    let q = search.unwrap_or("").trim().to_string();
    let like = format!("%{q}%");
    let mut stmt = conn.prepare(&sql)?;
    let mut out = stmt
        .query_map(params![q, like, limit], map_track)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    attach_artists(conn, &mut out)?;
    Ok(out)
}

pub fn get_track(conn: &Connection, id: i64) -> Result<Track> {
    let sql = format!("{TRACK_SELECT} WHERE t.id = ?1");
    let track = conn
        .query_row(&sql, [id], map_track)
        .map_err(|e| anyhow!(fehler!("Titel {0} nicht gefunden: {1}", id, e)))?;
    let mut one = [track];
    attach_artists(conn, &mut one)?;
    let [track] = one;
    Ok(track)
}

pub fn get_tracks(conn: &Connection, ids: &[i64]) -> Result<Vec<Track>> {
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        if let Ok(t) = get_track(conn, *id) {
            out.push(t);
        }
    }
    Ok(out)
}

pub fn list_artists(conn: &Connection, search: Option<&str>) -> Result<Vec<Artist>> {
    let q = search.unwrap_or("").trim().to_string();
    let like = format!("%{q}%");
    let mut stmt = conn.prepare(
        "SELECT ar.id, ar.name, ar.sort_name, ar.mbid,
                (SELECT COUNT(*) FROM track_artists ta
                  JOIN tracks t ON t.id = ta.track_id
                  WHERE ta.artist_id = ar.id AND t.deleted_at IS NULL),
                (SELECT COUNT(*) FROM albums al WHERE al.artist_id = ar.id
                  AND EXISTS (SELECT 1 FROM tracks t
                              WHERE t.album_id = al.id AND t.deleted_at IS NULL)),
                (ar.image IS NOT NULL), ar.bio, ar.source_url
         FROM artists ar
         WHERE (?1 = '' OR ar.name LIKE ?2)
         ORDER BY ar.sort_name COLLATE NOCASE",
    )?;
    let out = stmt
        .query_map(params![q, like], |r| {
            Ok(Artist {
                id: r.get(0)?,
                name: r.get(1)?,
                sort_name: r.get(2)?,
                mbid: r.get(3)?,
                track_count: r.get(4)?,
                release_count: r.get(5)?,
                has_image: r.get::<_, i64>(6)? != 0,
                bio: r.get(7)?,
                source_url: r.get(8)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(out.into_iter().filter(|a| a.track_count > 0).collect())
}

pub fn get_artist(conn: &Connection, id: i64) -> Result<Artist> {
    let mut stmt = conn.prepare(
        "SELECT ar.id, ar.name, ar.sort_name, ar.mbid,
                (SELECT COUNT(*) FROM track_artists ta
                  JOIN tracks t ON t.id = ta.track_id
                  WHERE ta.artist_id = ar.id AND t.deleted_at IS NULL),
                (SELECT COUNT(*) FROM albums al WHERE al.artist_id = ar.id
                  AND EXISTS (SELECT 1 FROM tracks t
                              WHERE t.album_id = al.id AND t.deleted_at IS NULL)),
                (ar.image IS NOT NULL), ar.bio, ar.source_url
         FROM artists ar WHERE ar.id = ?1",
    )?;
    let a = stmt.query_row([id], |r| {
        Ok(Artist {
            id: r.get(0)?,
            name: r.get(1)?,
            sort_name: r.get(2)?,
            mbid: r.get(3)?,
            track_count: r.get(4)?,
            release_count: r.get(5)?,
            has_image: r.get::<_, i64>(6)? != 0,
            bio: r.get(7)?,
            source_url: r.get(8)?,
        })
    })?;
    Ok(a)
}

const ALBUM_SELECT: &str = r#"
SELECT al.id, al.title, al.artist_id, ar.name, al.release_type, al.year, al.mbid,
       (al.cover IS NOT NULL),
       (SELECT COUNT(*) FROM tracks t WHERE t.album_id = al.id AND t.deleted_at IS NULL),
       COALESCE((SELECT SUM(t.duration_ms) FROM tracks t
                  WHERE t.album_id = al.id AND t.deleted_at IS NULL), 0)
FROM albums al JOIN artists ar ON ar.id = al.artist_id
"#;

fn map_album(row: &Row) -> rusqlite::Result<Album> {
    Ok(Album {
        id: row.get(0)?,
        title: row.get(1)?,
        artist_id: row.get(2)?,
        artist_name: row.get(3)?,
        release_type: ReleaseType::parse(&row.get::<_, String>(4)?),
        year: row.get(5)?,
        mbid: row.get(6)?,
        has_cover: row.get::<_, i64>(7)? != 0,
        track_count: row.get(8)?,
        duration_ms: row.get(9)?,
    })
}

pub fn list_albums(conn: &Connection, search: Option<&str>) -> Result<Vec<Album>> {
    let q = search.unwrap_or("").trim().to_string();
    let like = format!("%{q}%");
    let sql = format!(
        "{ALBUM_SELECT} WHERE (?1 = '' OR al.title LIKE ?2 OR ar.name LIKE ?2)
         ORDER BY al.year DESC NULLS LAST, al.title COLLATE NOCASE"
    );
    let mut stmt = conn.prepare(&sql)?;
    let out = stmt
        .query_map(params![q, like], map_album)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(out.into_iter().filter(|a| a.track_count > 0).collect())
}

/// every release of an artist, the frontend groups them by `releaseType`.
pub fn artist_releases(conn: &Connection, artist_id: i64) -> Result<Vec<Album>> {
    let sql = format!(
        "{ALBUM_SELECT} WHERE al.artist_id = ?1
         ORDER BY al.year DESC NULLS LAST, al.title COLLATE NOCASE"
    );
    let mut stmt = conn.prepare(&sql)?;
    let out = stmt
        .query_map([artist_id], map_album)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(out.into_iter().filter(|a| a.track_count > 0).collect())
}

pub fn get_album(conn: &Connection, id: i64) -> Result<Album> {
    let sql = format!("{ALBUM_SELECT} WHERE al.id = ?1");
    conn.query_row(&sql, [id], map_album)
        .map_err(|e| anyhow!(fehler!("Release {0} nicht gefunden: {1}", id, e)))
}

pub fn album_tracks(conn: &Connection, album_id: i64) -> Result<Vec<Track>> {
    let sql = format!(
        "{TRACK_SELECT} WHERE t.album_id = ?1 AND t.deleted_at IS NULL
         ORDER BY COALESCE(t.disc_no, 1), COALESCE(t.track_no, 9999), t.title COLLATE NOCASE"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut out = stmt
        .query_map([album_id], map_track)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    attach_artists(conn, &mut out)?;
    Ok(out)
}

/// every track the artist takes part in, as a guest too.
pub fn artist_tracks(conn: &Connection, artist_id: i64) -> Result<Vec<Track>> {
    let sql = format!(
        "{TRACK_SELECT}
         WHERE t.deleted_at IS NULL
           AND EXISTS (SELECT 1 FROM track_artists ta
                       WHERE ta.track_id = t.id AND ta.artist_id = ?1)
         ORDER BY (SELECT COUNT(*) FROM plays p WHERE p.track_id = t.id) DESC,
                  t.title COLLATE NOCASE"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut out = stmt
        .query_map([artist_id], map_track)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    attach_artists(conn, &mut out)?;
    Ok(out)
}

/// tracks the artist is only a guest on.
pub fn artist_features(conn: &Connection, artist_id: i64) -> Result<Vec<Track>> {
    let sql = format!(
        "{TRACK_SELECT}
         WHERE t.deleted_at IS NULL
           AND EXISTS (SELECT 1 FROM track_artists ta
                       WHERE ta.track_id = t.id AND ta.artist_id = ?1
                         AND ta.role = 'feature')
         ORDER BY t.title COLLATE NOCASE"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut out = stmt
        .query_map([artist_id], map_track)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    attach_artists(conn, &mut out)?;
    Ok(out)
}

/// the favourites in the order chosen for them.
///
/// `added_at` stays as a second criterion: whoever has never reordered still
/// sees the last one marked on top, and two tracks sharing a position do not
/// stand arbitrarily to each other.
pub fn favorite_tracks(conn: &Connection) -> Result<Vec<Track>> {
    let sql = format!("{TRACK_SELECT} WHERE t.favorite = 1 AND t.deleted_at IS NULL
         ORDER BY t.favorite_position, t.added_at DESC");
    let mut stmt = conn.prepare(&sql)?;
    let mut out = stmt
        .query_map([], map_track)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    attach_artists(conn, &mut out)?;
    Ok(out)
}

pub fn set_favorite(conn: &Connection, track_id: i64, favorite: bool) -> Result<()> {
    conn.execute(
        "UPDATE tracks SET favorite = ?2 WHERE id = ?1",
        params![track_id, i64::from(favorite)],
    )?;
    // newly marked means on top: what one just tapped is what one is looking
    // for. a position before the previously first, without touching the rest
    if favorite {
        conn.execute(
            "UPDATE tracks SET favorite_position =
                 COALESCE((SELECT MIN(favorite_position) FROM tracks WHERE favorite = 1), 1) - 1
             WHERE id = ?1",
            params![track_id],
        )?;
    }
    Ok(())
}

/// sets the order of the favourites anew, driven by drag and drop in the ui.
pub fn reorder_favorites(conn: &Connection, track_ids: &[i64]) -> Result<()> {
    for (stelle, track_id) in track_ids.iter().enumerate() {
        conn.execute(
            "UPDATE tracks SET favorite_position = ?2 WHERE id = ?1",
            params![track_id, stelle as i64],
        )?;
    }
    Ok(())
}

/// tracks whose file no longer exists.
///
/// files disappear outside the app: moved, renamed, deleted. the row then
/// stands as a corpse and cannot be played.
pub fn tracks_without_file(conn: &Connection) -> Result<Vec<(i64, String)>> {
    let mut stmt = conn.prepare("SELECT id, path FROM tracks WHERE deleted_at IS NULL ORDER BY id")?;
    let alle: Vec<(i64, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?;

    Ok(alle
        .into_iter()
        .filter(|(_, path)| !std::path::Path::new(path).exists())
        .collect())
}

/// every known file path, the basis for recognising orphans.
pub fn known_paths(conn: &Connection) -> Result<std::collections::HashSet<String>> {
    let mut stmt = conn.prepare("SELECT path FROM tracks WHERE deleted_at IS NULL")?;
    let pfade = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<_, _>>()?;
    Ok(pfade)
}

/// whether this title looks like a filename rather than a song title.
///
/// without tags the import takes the filename as the title. typical then are
/// underscores instead of spaces, a leading number ("03 - ") or a separator
/// with the artist hiding behind it.
pub fn looks_like_filename(title: &str) -> bool {
    let t = title.trim();
    if t.is_empty() {
        return true;
    }
    if t.contains('_') {
        return true;
    }
    // "03 - Titel", "03. Titel", "03 Titel"
    let mut zeichen = t.chars();
    let ziffern: String = zeichen.by_ref().take_while(|c| c.is_ascii_digit()).collect();
    if ziffern.len() >= 2 && t.len() > ziffern.len() {
        return true;
    }
    // a separator hints at "artist - title" in the filename
    t.contains(" - ")
}

/// tracks whose details look unreliable and are worth looking up.
///
/// the criteria: no recognisable artist, a title that looks like a filename,
/// or a missing album. all three come out of importing files without usable
/// tags.
///
/// locally imported tracks only: what the downloader fetched was checked and
/// enriched while loading.
pub fn tracks_with_weak_metadata(conn: &Connection, limit: usize) -> Result<Vec<Track>> {
    let sql = format!(
        "{TRACK_SELECT}
         WHERE t.deleted_at IS NULL AND COALESCE(t.source, '') = 'lokal'
         ORDER BY t.added_at DESC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let alle = stmt
        .query_map([], map_track)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    Ok(alle
        .into_iter()
        .filter(weak_metadata)
        .take(limit)
        .collect())
}

/// the check behind `tracks_with_weak_metadata`, usable on its own.
pub fn weak_metadata(track: &Track) -> bool {
    track.artist_name == "Unbekannter Künstler"
        || looks_like_filename(&track.title)
        || track.album_title.trim() == track.title.trim()
}

/// every locally imported track worth looking something up for: uncertain
/// details, a missing cover or missing lyrics.
///
/// kept apart from `tracks_with_weak_metadata` because cleanly tagged files
/// are among them here, only missing the trimmings.
pub fn tracks_needing_lookup(conn: &Connection, limit: usize) -> Result<Vec<Track>> {
    let sql = format!(
        "{TRACK_SELECT}
         WHERE t.deleted_at IS NULL AND COALESCE(t.source, '') = 'lokal'
         ORDER BY t.added_at DESC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let alle = stmt
        .query_map([], map_track)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    Ok(alle
        .into_iter()
        .filter(|track| weak_metadata(track) || !track.has_cover || !track.has_lyrics)
        .take(limit)
        .collect())
}

/// artists of a track for whom neither image nor description is on hand.
///
/// the basis for fetching the details automatically at the first track.
pub fn artists_missing_metadata(
    conn: &Connection,
    track_id: i64,
) -> Result<Vec<(i64, String, Vec<String>)>> {
    let mut stmt = conn.prepare(
        "SELECT a.id, a.name
           FROM track_artists ta
           JOIN artists a ON a.id = ta.artist_id
          WHERE ta.track_id = ?1
            AND a.image IS NULL
            AND (a.bio IS NULL OR a.bio = '')
          ORDER BY ta.position",
    )?;
    let rows = stmt.query_map([track_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    let gefunden: Vec<(i64, String)> = rows.collect::<std::result::Result<_, _>>()?;
    drop(stmt);
    with_titles(conn, gefunden)
}

/// the same for the whole library, a folder scan often brings many new
/// artists at once. `limit` keeps the number of queries in check.
pub fn all_artists_missing_metadata(
    conn: &Connection,
    limit: i64,
) -> Result<Vec<(i64, String, Vec<String>)>> {
    let mut stmt = conn.prepare(
        "SELECT id, name FROM artists
          WHERE image IS NULL AND (bio IS NULL OR bio = '')
          ORDER BY created_at DESC
          LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit], |r| Ok((r.get(0)?, r.get(1)?)))?;
    let gefunden: Vec<(i64, String)> = rows.collect::<std::result::Result<_, _>>()?;
    drop(stmt);
    with_titles(conn, gefunden)
}

/// adds a few tracks from the library per artist.
///
/// they serve as evidence where names collide: whoever carries them is the
/// one meant.
fn with_titles(
    conn: &Connection,
    artists: Vec<(i64, String)>,
) -> Result<Vec<(i64, String, Vec<String>)>> {
    let mut out = Vec::with_capacity(artists.len());
    for (id, name) in artists {
        let titel = artist_tracks(conn, id)
            .unwrap_or_default()
            .into_iter()
            .map(|track| track.title)
            .take(20)
            .collect();
        out.push((id, name, titel));
    }
    Ok(out)
}

/// profile image of an artist.
pub fn set_artist_image(conn: &Connection, artist_id: i64, data: &[u8], mime: &str) -> Result<()> {
    conn.execute(
        "UPDATE artists SET image = ?2, image_mime = ?3 WHERE id = ?1",
        params![artist_id, data, mime],
    )?;
    Ok(())
}

pub fn artist_image(conn: &Connection, artist_id: i64) -> Result<Option<(Vec<u8>, String)>> {
    let row = conn
        .query_row(
            "SELECT image, COALESCE(image_mime, 'image/jpeg') FROM artists WHERE id = ?1",
            [artist_id],
            |r| Ok((r.get::<_, Option<Vec<u8>>>(0)?, r.get::<_, String>(1)?)),
        )
        .optional()?;
    Ok(row.and_then(|(data, mime)| data.map(|d| (d, mime))))
}

/// changes the master data of an artist. a name already taken is refused,
/// otherwise there would be two rows for the same artist.
pub fn update_artist(
    conn: &Connection,
    artist_id: i64,
    name: &str,
    bio: Option<&str>,
    source_url: Option<&str>,
) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err(anyhow!(fehler!("Der Name darf nicht leer sein.")));
    }

    let key = key_of(name);
    let clash: Option<i64> = conn
        .query_row(
            "SELECT id FROM artists WHERE name_key = ?1 AND id <> ?2",
            params![&key, artist_id],
            |r| r.get(0),
        )
        .optional()?;
    if clash.is_some() {
        return Err(anyhow!(fehler!("„{0}“ gibt es in der Bibliothek bereits.", name)));
    }

    conn.execute(
        "UPDATE artists SET name = ?2, sort_name = ?3, name_key = ?4, bio = ?5, source_url = ?6
         WHERE id = ?1",
        params![
            artist_id,
            name,
            strip_leading_article(name),
            key,
            bio.filter(|b| !b.trim().is_empty()),
            source_url.filter(|u| !u.trim().is_empty())
        ],
    )?;
    Ok(())
}

/// changes title, year and classification of a release.
pub fn update_album(
    conn: &Connection,
    album_id: i64,
    title: &str,
    year: Option<i64>,
    release_type: ReleaseType,
) -> Result<()> {
    let title = title.trim();
    if title.is_empty() {
        return Err(anyhow!(fehler!("Der Titel darf nicht leer sein.")));
    }
    conn.execute(
        // the decision of the user, and it stands above everything a source
        // says or a count suggests
        "UPDATE albums SET title = ?2, title_key = ?3, year = ?4,
                release_type = ?5, release_type_locked = 1 -- ART_VOM_NUTZER
         WHERE id = ?1",
        params![
            album_id,
            title,
            key_of(title),
            year,
            release_type.as_str()
        ],
    )?;
    Ok(())
}

// --- covers ---

pub fn set_album_cover(conn: &Connection, album_id: i64, data: &[u8], mime: &str) -> Result<()> {
    conn.execute(
        "UPDATE albums SET cover = ?2, cover_mime = ?3 WHERE id = ?1",
        params![album_id, data, mime],
    )?;
    Ok(())
}

pub fn album_cover(conn: &Connection, album_id: i64) -> Result<Option<(Vec<u8>, String)>> {
    let row = conn
        .query_row(
            "SELECT cover, COALESCE(cover_mime, 'image/jpeg') FROM albums WHERE id = ?1",
            [album_id],
            |r| Ok((r.get::<_, Option<Vec<u8>>>(0)?, r.get::<_, String>(1)?)),
        )
        .optional()?;
    Ok(row.and_then(|(data, mime)| data.map(|d| (d, mime))))
}

pub fn track_cover(conn: &Connection, track_id: i64) -> Result<Option<(Vec<u8>, String)>> {
    let album_id = conn
        .query_row("SELECT album_id FROM tracks WHERE id = ?1", [track_id], |r| {
            r.get::<_, i64>(0)
        })
        .optional()?;
    match album_id {
        Some(id) => album_cover(conn, id),
        None => Ok(None),
    }
}

// --- lyrics ---

pub fn set_lyrics(
    conn: &Connection,
    track_id: i64,
    synced: Option<&str>,
    plain: Option<&str>,
    source: Option<&str>,
) -> Result<()> {
    conn.execute(
        "INSERT INTO lyrics (track_id, synced, plain, source, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(track_id) DO UPDATE SET
             synced = excluded.synced, plain = excluded.plain,
             source = excluded.source, updated_at = excluded.updated_at",
        params![track_id, synced, plain, source, now()],
    )?;
    Ok(())
}

pub fn get_lyrics(conn: &Connection, track_id: i64) -> Result<Option<Lyrics>> {
    let row = conn
        .query_row(
            "SELECT track_id, synced, plain, source, updated_at FROM lyrics WHERE track_id = ?1",
            [track_id],
            |r| {
                Ok(Lyrics {
                    track_id: r.get(0)?,
                    synced: r.get(1)?,
                    plain: r.get(2)?,
                    source: r.get(3)?,
                    updated_at: r.get(4)?,
                })
            },
        )
        .optional()?;
    Ok(row)
}

// --- playlists ---

/// whether a playlist of this name exists already.
pub fn playlist_name_taken(conn: &Connection, name: &str) -> Result<bool> {
    let anzahl: i64 = conn.query_row(
        "SELECT COUNT(*) FROM playlists WHERE name = ?1 AND deleted_at IS NULL",
        [name],
        |r| r.get(0),
    )?;
    Ok(anzahl > 0)
}

/// recently played tracks, each of them once and the youngest first.
pub fn recently_played(conn: &Connection, limit: i64) -> Result<Vec<Track>> {
    // through `tracks.last_played_at` and not through `plays`: only what ran
    // long enough to count in the statistics stands there, thirty seconds. a
    // track heard briefly and then skipped was missing at exactly the place
    // where one looks for it again
    let sql = format!(
        "{TRACK_SELECT}
         WHERE t.deleted_at IS NULL AND t.last_played_at IS NOT NULL
         ORDER BY t.last_played_at DESC
         LIMIT ?1"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut tracks: Vec<Track> = stmt
        .query_map([limit], map_track)?
        .collect::<rusqlite::Result<_>>()?;
    drop(stmt);
    attach_artists(conn, &mut tracks)?;
    Ok(tracks)
}

/// stores an image of its own for the playlist. `None` takes it away again.
pub fn set_playlist_cover(
    conn: &Connection,
    playlist_id: i64,
    bild: Option<(&[u8], &str)>,
) -> Result<()> {
    match bild {
        Some((data, mime)) => conn.execute(
            "UPDATE playlists SET cover = ?2, cover_mime = ?3 WHERE id = ?1",
            params![playlist_id, data, mime],
        )?,
        None => conn.execute(
            "UPDATE playlists SET cover = NULL, cover_mime = NULL WHERE id = ?1",
            [playlist_id],
        )?,
    };
    Ok(())
}

pub fn playlist_cover(conn: &Connection, playlist_id: i64) -> Result<Option<(Vec<u8>, String)>> {
    let row = conn
        .query_row(
            "SELECT cover, COALESCE(cover_mime, 'image/jpeg') FROM playlists WHERE id = ?1",
            [playlist_id],
            |r| Ok((r.get::<_, Option<Vec<u8>>>(0)?, r.get::<_, String>(1)?)),
        )
        .optional()?;
    Ok(row.and_then(|(data, mime)| data.map(|d| (d, mime))))
}

pub fn create_playlist(conn: &Connection, name: &str, description: Option<&str>) -> Result<i64> {
    // a new playlist stands at the front, as it always did: the order ran by
    // creation date, newest first. instead of pushing all the others along it
    // gets a position before the previously first
    conn.execute(
        "INSERT INTO playlists (name, description, created_at, position)
         VALUES (?1, ?2, ?3, (SELECT COALESCE(MIN(position), 1) - 1 FROM playlists))",
        params![name.trim(), description, now()],
    )?;
    Ok(conn.last_insert_rowid())
}

/// sets the order of the collection anew.
///
/// `ids` is the complete list in the order wanted. playlists not in it,
/// created from somewhere else in the meantime for instance, keep their place
/// before all others: they get no new number, and the numbers handed out
/// start at one.
pub fn reorder_playlists(conn: &Connection, ids: &[i64]) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    {
        let mut stmt = tx.prepare("UPDATE playlists SET position = ?2 WHERE id = ?1")?;
        for (stelle, id) in ids.iter().enumerate() {
            stmt.execute(params![id, stelle as i64 + 1])?;
        }
    }
    tx.commit()?;
    Ok(())
}

pub fn update_playlist(
    conn: &Connection,
    id: i64,
    name: &str,
    description: Option<&str>,
) -> Result<()> {
    conn.execute(
        "UPDATE playlists SET name = ?2, description = ?3 WHERE id = ?1",
        params![id, name.trim(), description],
    )?;
    Ok(())
}

/// removes a playlist. as with tracks it is only marked, so the action can be
/// undone without having to collect the tracks inside again.
pub fn delete_playlist(conn: &Connection, id: i64) -> Result<()> {
    conn.execute(
        "UPDATE playlists SET deleted_at = ?2 WHERE id = ?1",
        params![id, now()],
    )?;
    Ok(())
}

pub fn restore_playlist(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("UPDATE playlists SET deleted_at = NULL WHERE id = ?1", [id])?;
    Ok(())
}

pub fn list_playlists(conn: &Connection) -> Result<Vec<Playlist>> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.name, p.description, p.created_at,
                (SELECT COUNT(*) FROM playlist_tracks pt WHERE pt.playlist_id = p.id),
                COALESCE((SELECT SUM(t.duration_ms) FROM playlist_tracks pt
                          JOIN tracks t ON t.id = pt.track_id
                          WHERE pt.playlist_id = p.id), 0),
                (p.cover IS NOT NULL)
         FROM playlists p WHERE p.deleted_at IS NULL
         ORDER BY p.position, p.created_at DESC",
    )?;
    let base: Vec<Playlist> = stmt
        .query_map([], |r| {
            Ok(Playlist {
                id: r.get(0)?,
                name: r.get(1)?,
                description: r.get(2)?,
                created_at: r.get(3)?,
                track_count: r.get(4)?,
                duration_ms: r.get(5)?,
                cover_album_ids: Vec::new(),
                has_cover: r.get::<_, i64>(6)? != 0,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    drop(stmt);

    let mut cover_stmt = conn.prepare(
        "SELECT DISTINCT t.album_id FROM playlist_tracks pt
         JOIN tracks t ON t.id = pt.track_id
         JOIN albums al ON al.id = t.album_id
         WHERE pt.playlist_id = ?1 AND al.cover IS NOT NULL
         ORDER BY pt.position LIMIT 4",
    )?;
    let mut out = Vec::with_capacity(base.len());
    for mut pl in base {
        pl.cover_album_ids = cover_stmt
            .query_map([pl.id], |r| r.get::<_, i64>(0))?
            .collect::<rusqlite::Result<_>>()?;
        out.push(pl);
    }
    Ok(out)
}

pub fn get_playlist(conn: &Connection, id: i64) -> Result<Playlist> {
    list_playlists(conn)?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| anyhow!(fehler!("Playlist {0} nicht gefunden", id)))
}

pub fn playlist_tracks(conn: &Connection, playlist_id: i64) -> Result<Vec<Track>> {
    let sql = format!(
        "{TRACK_SELECT}
         JOIN playlist_tracks pt ON pt.track_id = t.id
         WHERE pt.playlist_id = ?1 AND t.deleted_at IS NULL
         ORDER BY pt.position"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut out = stmt
        .query_map([playlist_id], map_track)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    attach_artists(conn, &mut out)?;
    Ok(out)
}

/// counts per playlist how many of the tracks already lie there.
///
/// the basis for the warning in the add dialog: duplicate entries are skipped
/// silently on insert, and without a hint one wonders why the playlist does
/// not grow.
pub fn playlists_containing(
    conn: &Connection,
    track_ids: &[i64],
) -> Result<Vec<(i64, i64)>> {
    if track_ids.is_empty() {
        return Ok(Vec::new());
    }
    let platzhalter = vec!["?"; track_ids.len()].join(",");
    let sql = format!(
        "SELECT playlist_id, COUNT(*) FROM playlist_tracks
          WHERE track_id IN ({platzhalter})
          GROUP BY playlist_id"
    );
    let mut stmt = conn.prepare(&sql)?;
    let werte = rusqlite::params_from_iter(track_ids.iter());
    let rows = stmt.query_map(werte, |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

pub fn add_to_playlist(conn: &Connection, playlist_id: i64, track_ids: &[i64]) -> Result<()> {
    let mut next: i64 = conn.query_row(
        "SELECT COALESCE(MAX(position), -1) + 1 FROM playlist_tracks WHERE playlist_id = ?1",
        [playlist_id],
        |r| r.get(0),
    )?;
    for track_id in track_ids {
        let changed = conn.execute(
            "INSERT OR IGNORE INTO playlist_tracks (playlist_id, track_id, position, added_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![playlist_id, track_id, next, now()],
        )?;
        if changed > 0 {
            next += 1;
        }
    }
    Ok(())
}

pub fn remove_from_playlist(conn: &Connection, playlist_id: i64, track_id: i64) -> Result<()> {
    conn.execute(
        "DELETE FROM playlist_tracks WHERE playlist_id = ?1 AND track_id = ?2",
        params![playlist_id, track_id],
    )?;
    Ok(())
}

/// sets the order completely anew, driven by drag and drop in the ui.
pub fn reorder_playlist(conn: &Connection, playlist_id: i64, track_ids: &[i64]) -> Result<()> {
    for (position, track_id) in track_ids.iter().enumerate() {
        conn.execute(
            "UPDATE playlist_tracks SET position = ?3 WHERE playlist_id = ?1 AND track_id = ?2",
            params![playlist_id, track_id, position as i64],
        )?;
    }
    Ok(())
}

// --- deletion ---

/// removes a track from the library.
///
/// the row is only marked as removed, not deleted. the reason is the
/// listening history: `plays` hangs off the track by foreign key and would go
/// with it, and the review would lose hours retroactively. this way it stays
/// complete, and creating the same track again later has `upsert_track` tie
/// into the same row and simply carry on counting.
///
/// the file travels into `papierkorb` instead of being deleted. only that way
/// can the action be undone, a deleted file is gone.
pub fn delete_track(
    conn: &Connection,
    track_id: i64,
    delete_file: bool,
    papierkorb: Option<&std::path::Path>,
) -> Result<()> {
    let path: Option<String> = conn
        .query_row("SELECT path FROM tracks WHERE id = ?1", [track_id], |r| {
            r.get(0)
        })
        .optional()?;

    conn.execute(
        "UPDATE tracks SET deleted_at = ?2, favorite = 0 WHERE id = ?1",
        params![track_id, now()],
    )?;
    // it really disappears from playlists, it would only be a gap there
    conn.execute(
        "DELETE FROM playlist_tracks WHERE track_id = ?1",
        [track_id],
    )?;

    if delete_file {
        if let Some(p) = path {
            let quelle = std::path::Path::new(&p);
            match papierkorb {
                Some(ordner) => {
                    let _ = std::fs::create_dir_all(ordner);
                    let _ = std::fs::rename(quelle, papierkorb_datei(ordner, track_id, quelle));
                }
                None => {
                    let _ = std::fs::remove_file(quelle);
                }
            }
        }
    }
    Ok(())
}

/// clears the trash out: whatever is older than `max_age` goes for good.
///
/// without it the folder grows without bound, every deleted file staying
/// forever. the deadline gives undoing enough time, after that the decision
/// has been made.
pub fn cleanup_trash(papierkorb: &std::path::Path, max_age: std::time::Duration) -> usize {
    let Ok(eintraege) = std::fs::read_dir(papierkorb) else {
        return 0;
    };

    let mut entfernt = 0;
    for eintrag in eintraege.filter_map(Result::ok) {
        let pfad = eintrag.path();
        if !pfad.is_file() {
            continue;
        }
        let zu_alt = eintrag
            .metadata()
            .and_then(|meta| meta.modified())
            .map(|zeit| zeit.elapsed().unwrap_or_default() > max_age)
            // without a readable timestamp, better leave it standing
            .unwrap_or(false);

        if zu_alt && std::fs::remove_file(&pfad).is_ok() {
            entfernt += 1;
        }
    }
    entfernt
}

/// fetches tracks from an old library folder to the new one.
///
/// needed on a phone: until version 0.1.0 the tracks lay in the app's own
/// folder, since then in `Robify` in the device storage. the library
/// remembers complete paths though, and a plain move would leave every row
/// pointing nowhere.
///
/// file by file, with the row pulled along immediately: where it breaks off
/// midway, out of space or with the permission withdrawn, not a single row
/// points at a file that is not there. the rest travels at the next start.
///
/// what is not in the library travels along anyway, it lay in the music
/// folder and belongs there.
pub fn bibliothek_umziehen(
    conn: &Connection,
    alt: &std::path::Path,
    neu: &std::path::Path,
) -> usize {
    if !alt.is_dir() || alt == neu {
        return 0;
    }

    let mut gewandert = 0;
    umziehen_rekursiv(conn, alt, alt, neu, &mut gewandert);

    // empty folders are left behind, those may go, the rest stays
    let _ = entleerte_ordner_entfernen(alt);
    gewandert
}

fn umziehen_rekursiv(
    conn: &Connection,
    wurzel: &std::path::Path,
    ordner: &std::path::Path,
    ziel_wurzel: &std::path::Path,
    gewandert: &mut usize,
) {
    let Ok(eintraege) = std::fs::read_dir(ordner) else {
        return;
    };

    for eintrag in eintraege.filter_map(Result::ok) {
        let quelle = eintrag.path();
        if quelle.is_dir() {
            umziehen_rekursiv(conn, wurzel, &quelle, ziel_wurzel, gewandert);
            continue;
        }

        let Ok(relativ) = quelle.strip_prefix(wurzel) else {
            continue;
        };
        let ziel = ziel_wurzel.join(relativ);
        if ziel.exists() {
            continue;
        }
        if let Some(eltern) = ziel.parent() {
            if std::fs::create_dir_all(eltern).is_err() {
                continue;
            }
        }
        if crate::verschieben(&quelle, &ziel).is_err() {
            continue;
        }

        let _ = conn.execute(
            "UPDATE tracks SET path = ?2 WHERE path = ?1",
            params![quelle.to_string_lossy(), ziel.to_string_lossy()],
        );
        *gewandert += 1;
    }
}

// clears empty folders away from the bottom up
fn entleerte_ordner_entfernen(ordner: &std::path::Path) -> std::io::Result<()> {
    for eintrag in std::fs::read_dir(ordner)?.filter_map(Result::ok) {
        let pfad = eintrag.path();
        if pfad.is_dir() {
            let _ = entleerte_ordner_entfernen(&pfad);
        }
    }
    std::fs::remove_dir(ordner)
}

// place in the trash: id of the track plus the original extension. through
// the id, restoring finds the file again without fail, whatever it was called
// originally
fn papierkorb_datei(ordner: &std::path::Path, track_id: i64, quelle: &std::path::Path) -> std::path::PathBuf {
    match quelle.extension().and_then(|e| e.to_str()) {
        Some(endung) => ordner.join(format!("{track_id}.{endung}")),
        None => ordner.join(track_id.to_string()),
    }
}

/// undoes the removal: the row visible again, the file back in its place.
/// where the file is missing the row stays anyway, and the reconciliation
/// pass reports it as missing then.
pub fn restore_track(
    conn: &Connection,
    track_id: i64,
    papierkorb: Option<&std::path::Path>,
) -> Result<()> {
    let path: String = conn
        .query_row("SELECT path FROM tracks WHERE id = ?1", [track_id], |r| {
            r.get(0)
        })
        .optional()?
        .ok_or_else(|| anyhow!(fehler!("Titel {0} gibt es nicht mehr", track_id)))?;

    let ziel = std::path::Path::new(&path);
    if !ziel.exists() {
        if let Some(ordner) = papierkorb {
            let quelle = papierkorb_datei(ordner, track_id, ziel);
            if quelle.exists() {
                if let Some(eltern) = ziel.parent() {
                    let _ = std::fs::create_dir_all(eltern);
                }
                let _ = std::fs::rename(&quelle, ziel);
            }
        }
    }

    conn.execute(
        "UPDATE tracks SET deleted_at = NULL WHERE id = ?1",
        [track_id],
    )?;
    Ok(())
}

pub fn library_stats(conn: &Connection) -> Result<LibraryStats> {
    let track_count = conn.query_row(
        "SELECT COUNT(*) FROM tracks WHERE deleted_at IS NULL",
        [],
        |r| r.get(0),
    )?;
    let artist_count = conn.query_row(
        "SELECT COUNT(DISTINCT artist_id) FROM tracks WHERE deleted_at IS NULL",
        [],
        |r| r.get(0),
    )?;
    let album_count = conn.query_row(
        "SELECT COUNT(DISTINCT album_id) FROM tracks WHERE deleted_at IS NULL",
        [],
        |r| r.get(0),
    )?;
    let playlist_count = conn.query_row("SELECT COUNT(*) FROM playlists", [], |r| r.get(0))?;
    let total_duration_ms =
        conn.query_row(
            "SELECT COALESCE(SUM(duration_ms), 0) FROM tracks WHERE deleted_at IS NULL",
            [],
            |r| r.get(0),
        )?;
    let total_listened_ms =
        conn.query_row("SELECT COALESCE(SUM(ms_played), 0) FROM plays", [], |r| {
            r.get(0)
        })?;
    Ok(LibraryStats {
        track_count,
        artist_count,
        album_count,
        playlist_count,
        total_duration_ms,
        total_listened_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// a folder under `target` that does not collide with other runs.
    fn testordner(name: &str) -> std::path::PathBuf {
        let pfad = std::env::temp_dir().join(format!("robify-umzug-{name}"));
        let _ = std::fs::remove_dir_all(&pfad);
        std::fs::create_dir_all(&pfad).expect("Testordner");
        pfad
    }

    /// the move on a phone: the file travels, the row follows.
    ///
    /// the sore point is not the moving but keeping the two in step. a row
    /// pointing at the old place while the file already lies at the new one
    /// is a track that can no longer be played.
    #[test]
    fn umzug_zieht_die_eintraege_mit() {
        let basis = testordner("mit");
        let alt = basis.join("alt");
        let neu = basis.join("neu");
        let ordner = alt.join("Yeat").join("2093");
        std::fs::create_dir_all(&ordner).unwrap();
        let datei = ordner.join("03 - Breathe.m4a");
        std::fs::write(&datei, b"ton").unwrap();

        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrate(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO artists (id, name, name_key, sort_name, created_at)
                 VALUES (1, 'Yeat', 'yeat', 'Yeat', 0);
             INSERT INTO albums (id, title, title_key, artist_id, release_type, created_at)
                 VALUES (1, '2093', '2093', 1, 'album', 0);",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tracks (id, path, title, artist_id, album_id, added_at)
             VALUES (1, ?1, 'Breathe', 1, 1, 0)",
            params![datei.to_string_lossy()],
        )
        .unwrap();

        assert_eq!(bibliothek_umziehen(&conn, &alt, &neu), 1);

        let ziel = neu.join("Yeat").join("2093").join("03 - Breathe.m4a");
        assert!(ziel.is_file(), "die Datei liegt am neuen Ort");
        assert!(!datei.exists(), "am alten liegt sie nicht mehr");
        let gespeichert: String = conn
            .query_row("SELECT path FROM tracks WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(gespeichert, ziel.to_string_lossy());
        assert!(!alt.exists(), "der leergeräumte Ordner ist weg");

        let _ = std::fs::remove_dir_all(&basis);
    }

    /// what already lies at the target is not overwritten.
    #[test]
    fn umzug_laesst_vorhandenes_stehen() {
        let basis = testordner("vorhanden");
        let alt = basis.join("alt");
        let neu = basis.join("neu");
        std::fs::create_dir_all(&alt).unwrap();
        std::fs::create_dir_all(&neu).unwrap();
        std::fs::write(alt.join("a.m4a"), b"alt").unwrap();
        std::fs::write(neu.join("a.m4a"), b"neu").unwrap();

        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrate(&conn).unwrap();

        assert_eq!(bibliothek_umziehen(&conn, &alt, &neu), 0);
        assert_eq!(std::fs::read(neu.join("a.m4a")).unwrap(), b"neu");
        assert!(alt.join("a.m4a").is_file(), "das alte bleibt liegen");

        let _ = std::fs::remove_dir_all(&basis);
    }

    #[test]
    fn zerlegt_videotitel_in_kuenstler_und_titel() {
        // exactly the cases from the measured run where the lyric channel
        // landed in the library as the artist
        assert_eq!(
            split_video_title("Nina Chuba - WILDBERRY LILLET [Lyrics]", Some("xTheLYRICS")),
            Some(("Nina Chuba".into(), "WILDBERRY LILLET".into()))
        );
        assert_eq!(
            split_video_title("Kendrick Lamar - Money Trees (Lyrics)", Some("Vibe Music")),
            Some(("Kendrick Lamar".into(), "Money Trees".into()))
        );

        // where the channel stands on the right, the halves are swapped
        assert_eq!(
            split_video_title(
                "Oft Gefragt - AnnenMayKantereit (Offizielles Video)",
                Some("AnnenMayKantereit")
            ),
            Some(("AnnenMayKantereit".into(), "Oft Gefragt".into()))
        );

        // en dash instead of hyphen
        assert_eq!(
            split_video_title("PA69 – Tropical Island", Some("PA69")),
            Some(("PA69".into(), "Tropical Island".into()))
        );

        // a space on one side only, from the measured run
        assert_eq!(
            split_video_title("The Killers- Mr. Brightside", Some("Julia")),
            Some(("The Killers".into(), "Mr. Brightside".into()))
        );

        // without a separator there is nothing to split
        assert_eq!(split_video_title("Naked", Some("Yeat")), None);

        // hyphens inside names must not tear apart
        assert_eq!(split_video_title("Jay-Z", Some("Jay-Z")), None);
        assert_eq!(split_video_title("Blink-182", None), None);
        assert_eq!(
            split_video_title("Jay-Z - Empire State of Mind", None),
            Some(("Jay-Z".into(), "Empire State of Mind".into()))
        );
    }

    #[test]
    fn gastkuenstler_ueberleben_die_titelreinigung() {
        // brackets carry more than hints about the production
        assert_eq!(
            split_video_title("Drake - Passionfruit (feat. Someone)", None),
            Some(("Drake".into(), "Passionfruit (feat. Someone)".into()))
        );
        // mixed brackets: "Official Video" out, the version stays
        assert_eq!(
            split_video_title("Band - Lied (Live) (Official Video)", None),
            Some(("Band".into(), "Lied (Live)".into()))
        );
    }

    #[test]
    fn trennt_kuenstler_nur_am_semikolon() {
        // commas and ampersands often belong to the band name
        assert_eq!(split_artists("Earth, Wind & Fire"), vec!["Earth, Wind & Fire"]);
        assert_eq!(split_artists("A; B; C"), vec!["A", "B", "C"]);
        assert_eq!(split_artists("  A ;  B  "), vec!["A", "B"]);
        assert!(split_artists("   ").is_empty());
    }

    #[test]
    fn erkennt_gastkuenstler_im_feld() {
        let (main, featured) = parse_artist_field("Kanye West feat. Bon Iver; Nicki Minaj");
        assert_eq!(main, vec!["Kanye West"]);
        assert_eq!(featured, vec!["Bon Iver", "Nicki Minaj"]);

        let (main, featured) = parse_artist_field("Drake ft. Future");
        assert_eq!(main, vec!["Drake"]);
        assert_eq!(featured, vec!["Future"]);

        let (main, featured) = parse_artist_field("Calvin Harris; Dua Lipa");
        assert_eq!(main, vec!["Calvin Harris", "Dua Lipa"]);
        assert!(featured.is_empty());

        // without a marker everything stays a lead artist
        let (main, featured) = parse_artist_field("Daft Punk");
        assert_eq!(main, vec!["Daft Punk"]);
        assert!(featured.is_empty());
    }

    #[test]
    fn kanal_wird_zum_hauptkuenstler() {
        // the case from practice: the soundcloud account "PA69"
        let (main, featured) =
            promote_uploader("PA69; Drunken Masters", None, "PA69").unwrap();
        assert_eq!(main, "PA69");
        assert_eq!(featured.as_deref(), Some("Drunken Masters"));

        // even where the channel stands at the end of the list
        let (main, featured) =
            promote_uploader("Drunken Masters; PA69", None, "PA69").unwrap();
        assert_eq!(main, "PA69");
        assert_eq!(featured.as_deref(), Some("Drunken Masters"));

        // already listed as a guest, then it moves to the front
        let (main, featured) =
            promote_uploader("Kanye West", Some("Nicki Minaj"), "Nicki Minaj").unwrap();
        assert_eq!(main, "Nicki Minaj");
        assert_eq!(featured.as_deref(), Some("Kanye West"));
    }

    #[test]
    fn erkennt_automatisch_erzeugte_kanaele() {
        let (main, _) = promote_uploader("A; PA69", None, "PA69 - Topic").unwrap();
        assert_eq!(main, "PA69");
        let (main, _) = promote_uploader("A; Rihanna", None, "RihannaVEVO").unwrap();
        assert_eq!(main, "Rihanna");
    }

    #[test]
    fn fremde_kanaele_aendern_nichts() {
        // label, sampler and repost channels belong to no participant
        assert!(promote_uploader("PA69; Drunken Masters", None, "Hip Hop Charts").is_none());
        assert!(promote_uploader("PA69; Drunken Masters", None, "").is_none());
        // with a single artist there is nothing to rearrange
        assert!(promote_uploader("PA69", None, "PA69").is_none());
    }

    #[test]
    fn fuegt_getrennte_namen_wieder_zusammen() {
        let names = vec!["A".to_string(), "B".to_string()];
        assert_eq!(join_artists(&names), "A; B");
        let (main, _) = parse_artist_field(&join_artists(&names));
        assert_eq!(main, names);
    }
}
