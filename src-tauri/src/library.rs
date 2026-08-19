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

/// Trennzeichen für mehrere Künstler in einem Textfeld.
const ARTIST_SEPARATOR: char = ';';

/// Wörter, hinter denen Gastkünstler folgen.
const FEATURE_MARKERS: [&str; 5] = [" feat. ", " feat ", " ft. ", " ft ", " featuring "];

/// Zerlegt „A; B; C“ in einzelne Namen. Kommas und Ampersands bleiben
/// unangetastet, sonst zerfielen Bandnamen wie „Earth, Wind & Fire“.
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

/// Trennt ein Künstlerfeld in Haupt- und Gastkünstler. Ein enthaltenes
/// „feat.“ schiebt alles Folgende zu den Gästen.
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

/// Hinweise auf die Machart, die im Songtitel nichts zu suchen haben.
/// Gastkünstler stehen ebenfalls in Klammern und bleiben deshalb erhalten,
/// entfernt wird nur, was ausschließlich aus diesen Wörtern besteht.
const TITLE_TAGS: [&str; 21] = [
    "official",
    // Deutsche Uploads schreiben „(Offizielles Video)“.
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

/// Entfernt Klammerzusätze, die nur die Machart beschreiben.
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
        // Nur wegwerfen, wenn ausschließlich Machart-Wörter drinstehen.
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

/// Zerlegt einen Videotitel der Form „Künstler - Titel“.
///
/// Nötig, weil Lyric-, Sampler- und Repost-Kanäle keine Musikfelder mitgeben:
/// Dort steckt beides im Titel, und ohne Zerlegung landet der ganze Videotitel
/// als Songtitel und der Kanalname als Künstler in der Bibliothek.
///
/// Steht der Kanalname auf der rechten Seite („Oft Gefragt - AnnenMayKantereit“),
/// sind die Hälften vertauscht, dann wird gedreht.
pub fn split_video_title(raw: &str, uploader: Option<&str>) -> Option<(String, String)> {
    let cleaned = strip_title_tags(raw);

    // Auch mit Leerzeichen nur auf einer Seite: „The Killers- Mr. Brightside“
    // blieb sonst ungetrennt, und der Kanalname wurde zum Künstler. Ein
    // blanker Bindestrich fehlt bewusst, er steckt in „Jay-Z“ und „T-Pain“.
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

/// Macht aus einem Kanalnamen den vermuteten Künstlernamen.
/// YouTube hängt an automatisch erzeugte Kanäle „ - Topic“ an, Labelkanäle
/// enden oft auf „VEVO“.
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

/// Stellt den Künstler nach vorn, unter dessen Konto der Titel veröffentlicht
/// wurde, die Übrigen werden zu Gastkünstlern.
///
/// Greift nur, wenn der Kanal tatsächlich einem der Beteiligten entspricht.
/// Bei Label-, Sampler- oder Repost-Kanälen bleibt die Reihenfolge, wie sie
/// die Metadatenquelle geliefert hat.
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
            // Kanäle wie „PA69 Official“ enthalten den Namen als Wortfolge.
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

/// Lädt die Beteiligten für eine ganze Trefferliste in einer Abfrage,
/// sonst gäbe es pro Titel eine eigene Runde zur Datenbank.
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
            // Fällt nur an, wenn die Verknüpfung fehlt, dann der Hauptkünstler.
            vec![TrackArtist {
                id: track.artist_id,
                name: track.artist_name.clone(),
                role: "main".into(),
            }]
        });
    }
    Ok(())
}

/// Schreibt die Beteiligten eines Titels neu.
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

// ---------------------------------------------------------------- Upserts

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

    // Sortiername ohne führenden Artikel, damit "The Beatles" unter B steht.
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
    // Unsichtbare Zeichen aus fremden Titeln fliegen raus, sonst stehen
    // zwei scheinbar gleiche Alben nebeneinander.
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
            conn.execute(
                "UPDATE albums SET release_type = ?2, release_type_locked = 1 WHERE id = ?1",
                params![id, rt.as_str()],
            )?;
        }
        return Ok(id);
    }

    let (rt, locked) = match release_type {
        Some(rt) => (rt, 1),
        None => (ReleaseType::Album, 0),
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
    /// Rohes Künstlerfeld; darf mehrere Namen und ein „feat.“ enthalten.
    pub artist: String,
    /// Zusätzliche Gastkünstler, mit Semikolon getrennt.
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

/// Legt einen Titel an bzw. aktualisiert ihn anhand des Dateipfads.
/// Ohne Albumangabe landet der Titel in einem gleichnamigen Single-Release.
/// Sucht einen früher entfernten Titel desselben Künstlers mit gleichem Namen.
///
/// Verglichen wird über `db::key_of`, also ohne Groß- und Kleinschreibung,
/// Satzzeichen und unsichtbare Zeichen. Gibt es mehrere, gewinnt der zuletzt
/// entfernte.
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

/// Sucht einen bereits vorhandenen Titel desselben Künstlers mit gleichem
/// Namen und ähnlicher Länge.
///
/// Der Name allein genügt nicht: Albumfassung und Single-Fassung heißen gleich
/// und sind verschiedene Aufnahmen. Erst zusammen mit der Laufzeit wird daraus
/// „derselbe Titel“, deshalb die Toleranz von fünf Sekunden, die Kodierungs-
/// und Schnittunterschiede abdeckt, ohne einen Remix mit einzufangen.
///
/// Ist die Länge unbekannt (null), wird nicht zusammengelegt: Lieber ein
/// doppelter Eintrag als ein verschluckter Titel.
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
    // Wer schon Hauptkünstler ist, taucht nicht zusätzlich als Gast auf.
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

    // Erst am Pfad, dann an einem früher entfernten Titel desselben Künstlers
    // festmachen. Der zweite Fall ist der wichtige: Wer einen gelöschten Titel
    // erneut lädt, bekommt eine andere Datei, soll aber seine Hörhistorie
    // behalten. Ohne diesen Griff entstünde ein zweiter Eintrag und die alten
    // Wiedergaben blieben an der Leiche hängen.
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

    // Derselbe Titel, andere Datei: nicht ein zweites Mal aufnehmen. Das
    // passiert beim Import einer Kopie und beim erneuten Laden eines Titels,
    // der schon in der Bibliothek liegt.
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
    Ok(track_id)
}

/// Ordnet alle nicht manuell gesetzten Releases anhand der Titelanzahl ein.
pub fn refresh_release_types(conn: &Connection) -> Result<()> {
    let mut stmt = conn.prepare(
        "SELECT al.id, COUNT(t.id)
         FROM albums al LEFT JOIN tracks t ON t.album_id = al.id
         WHERE al.release_type_locked = 0
         GROUP BY al.id",
    )?;
    let rows: Vec<(i64, i64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    drop(stmt);

    for (album_id, count) in rows {
        conn.execute(
            "UPDATE albums SET release_type = ?2 WHERE id = ?1",
            params![album_id, ReleaseType::from_track_count(count).as_str()],
        )?;
    }
    Ok(())
}

/// Entfernt Künstler und Alben ohne verbleibende Titel.
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

// ---------------------------------------------------------------- Abfragen

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

/// Alle Releases eines Künstlers, das Frontend gruppiert nach `releaseType`.
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

/// Alle Titel, an denen der Künstler beteiligt ist, auch als Gast.
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

/// Titel, bei denen der Künstler nur zu Gast ist.
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

pub fn favorite_tracks(conn: &Connection) -> Result<Vec<Track>> {
    let sql = format!("{TRACK_SELECT} WHERE t.favorite = 1 AND t.deleted_at IS NULL
         ORDER BY t.added_at DESC");
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
    Ok(())
}

/// Titel, deren Datei nicht mehr existiert.
///
/// Dateien verschwinden außerhalb der App, verschoben, umbenannt, gelöscht.
/// Der Eintrag bleibt dann als Leiche stehen und lässt sich nicht abspielen.
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

/// Alle bekannten Dateipfade. Grundlage, um Verwaistes zu erkennen.
pub fn known_paths(conn: &Connection) -> Result<std::collections::HashSet<String>> {
    let mut stmt = conn.prepare("SELECT path FROM tracks WHERE deleted_at IS NULL")?;
    let pfade = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<_, _>>()?;
    Ok(pfade)
}

/// Sieht dieser Titel nach einem Dateinamen statt nach einem Songtitel aus?
///
/// Ohne Tags nimmt der Import den Dateinamen als Titel. Typisch sind dann
/// Unterstriche statt Leerzeichen, eine vorangestellte Nummer („03 - “) oder
/// ein Trennstrich, hinter dem eigentlich der Künstler steckt.
pub fn looks_like_filename(title: &str) -> bool {
    let t = title.trim();
    if t.is_empty() {
        return true;
    }
    if t.contains('_') {
        return true;
    }
    // „03 - Titel“, „03. Titel“, „03 Titel“
    let mut zeichen = t.chars();
    let ziffern: String = zeichen.by_ref().take_while(|c| c.is_ascii_digit()).collect();
    if ziffern.len() >= 2 && t.len() > ziffern.len() {
        return true;
    }
    // Ein Trennstrich deutet auf „Künstler - Titel“ im Dateinamen hin.
    t.contains(" - ")
}

/// Titel, deren Angaben unzuverlässig wirken und ein Nachschlagen lohnen.
///
/// Kriterien: kein erkennbarer Künstler, ein Titel, der wie ein Dateiname
/// aussieht, oder ein fehlendes Album. Alles drei entsteht beim Import von
/// Dateien ohne brauchbare Tags.
///
/// Nur lokal importierte Titel: Was der Downloader geholt hat, ist bereits
/// beim Laden geprüft und angereichert worden.
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

/// Die Prüfung hinter `tracks_with_weak_metadata`, einzeln benutzbar.
pub fn weak_metadata(track: &Track) -> bool {
    track.artist_name == "Unbekannter Künstler"
        || looks_like_filename(&track.title)
        || track.album_title.trim() == track.title.trim()
}

/// Alle lokal importierten Titel, bei denen etwas nachzuschlagen lohnt:
/// unsichere Angaben, fehlendes Cover oder fehlende Lyrics.
///
/// Getrennt von `tracks_with_weak_metadata`, weil hier auch sauber getaggte
/// Dateien dabei sind, denen fehlt nur das Beiwerk.
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

/// Künstler eines Titels, zu denen weder Bild noch Beschreibung vorliegen.
/// Grundlage dafür, die Angaben beim ersten Titel automatisch nachzuladen.
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

/// Dasselbe für die gesamte Bibliothek, nach einem Ordner-Scan gibt es oft
/// viele neue Künstler auf einmal. `limit` hält die Zahl der Abfragen im Zaum.
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

/// Ergänzt je Künstler ein paar Titel aus der Bibliothek.
///
/// Sie dienen als Beleg bei Namensgleichheit: Wer sie führt, ist gemeint.
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

/// Profilbild eines Künstlers.
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

/// Ändert die Stammdaten eines Künstlers. Ein Name, den es schon gibt, wird
/// abgelehnt, sonst gäbe es zwei Einträge für denselben Künstler.
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

/// Ändert Titel, Jahr und Einordnung eines Releases.
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
        "UPDATE albums SET title = ?2, title_key = ?3, year = ?4,
                release_type = ?5, release_type_locked = 1
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

// ---------------------------------------------------------------- Cover

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

// ---------------------------------------------------------------- Lyrics

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

// ---------------------------------------------------------------- Playlists

/// Gibt es schon eine Playlist dieses Namens?
pub fn playlist_name_taken(conn: &Connection, name: &str) -> Result<bool> {
    let anzahl: i64 = conn.query_row(
        "SELECT COUNT(*) FROM playlists WHERE name = ?1 AND deleted_at IS NULL",
        [name],
        |r| r.get(0),
    )?;
    Ok(anzahl > 0)
}

/// Zuletzt gespielte Titel, jeder nur einmal und der jüngste zuerst.
pub fn recently_played(conn: &Connection, limit: i64) -> Result<Vec<Track>> {
    // Über `tracks.last_played_at` und nicht über `plays`: Dort steht nur,
    // was lange genug lief, um in der Statistik zu zählen — dreißig Sekunden.
    // Ein Titel, den man kurz angehört und dann weitergeschaltet hat, fehlte
    // damit ausgerechnet an der Stelle, an der man ihn wiedersucht.
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

/// Hinterlegt ein eigenes Bild für die Playlist. `None` nimmt es wieder weg.
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
    // Eine neue Playlist steht vorn, wie bisher auch: Die Ordnung lief nach
    // Anlagedatum, neueste zuerst. Statt alle anderen weiterzuschieben,
    // bekommt sie eine Stelle vor der bisher ersten.
    conn.execute(
        "INSERT INTO playlists (name, description, created_at, position)
         VALUES (?1, ?2, ?3, (SELECT COALESCE(MIN(position), 1) - 1 FROM playlists))",
        params![name.trim(), description, now()],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Legt die Reihenfolge der Sammlung neu fest.
///
/// `ids` ist die vollständige Liste in der gewünschten Ordnung. Playlists, die
/// nicht darin vorkommen, etwa weil sie inzwischen von woanders angelegt
/// wurden, behalten ihre Stelle vor allen anderen: Sie bekommen keine neue
/// Nummer, und die vergebenen beginnen bei eins.
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

/// Entfernt eine Playlist. Wie bei Titeln nur markiert, damit sich der Griff
/// zurücknehmen lässt, ohne die enthaltenen Titel neu einsammeln zu müssen.
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

/// Zählt je Playlist, wie viele der Titel dort schon liegen.
///
/// Grundlage für die Warnung im Hinzufügen-Dialog: Doppelte Einträge werden
/// beim Einfügen stillschweigend übergangen, ohne Hinweis wundert man sich,
/// warum die Playlist nicht länger wird.
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

/// Setzt die Reihenfolge komplett neu (Drag & Drop im Frontend).
pub fn reorder_playlist(conn: &Connection, playlist_id: i64, track_ids: &[i64]) -> Result<()> {
    for (position, track_id) in track_ids.iter().enumerate() {
        conn.execute(
            "UPDATE playlist_tracks SET position = ?3 WHERE playlist_id = ?1 AND track_id = ?2",
            params![playlist_id, track_id, position as i64],
        )?;
    }
    Ok(())
}

// ---------------------------------------------------------------- Löschen

/// Entfernt einen Titel aus der Bibliothek.
///
/// Der Eintrag wird nur als entfernt markiert, nicht gelöscht. Grund ist die
/// Hörhistorie: `plays` hängt per Fremdschlüssel am Titel und würde beim
/// Löschen mitgehen, der Rückblick verlöre rückwirkend Stunden. So bleibt er
/// vollständig, und legt man denselben Titel später wieder an, knüpft
/// `upsert_track` an denselben Eintrag an und zählt einfach weiter.
///
/// Die Datei wandert in `papierkorb`, statt gelöscht zu werden. Nur so lässt
/// sich der Griff zurücknehmen; eine gelöschte Datei ist weg.
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
    // Aus Playlists verschwindet er wirklich, dort wäre er nur eine Lücke.
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

/// Räumt den Papierkorb auf: Was älter ist als `max_age`, verschwindet
/// endgültig.
///
/// Ohne das wächst er unbegrenzt, jede gelöschte Datei bleibt für immer
/// liegen. Die Frist gibt dem Zurücknehmen genug Zeit; danach ist die
/// Entscheidung gefallen.
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
            // Ohne lesbaren Zeitstempel lieber stehen lassen.
            .unwrap_or(false);

        if zu_alt && std::fs::remove_file(&pfad).is_ok() {
            entfernt += 1;
        }
    }
    entfernt
}

/// Holt Titel von einem alten Bibliotheksordner an den neuen.
///
/// Nötig auf dem Telefon: Bis Fassung 0.1.0 lagen die Titel im eigenen Ordner
/// der App, seither in `Robify` im Gerätespeicher. Die Bibliothek merkt sich
/// aber vollständige Pfade, ein bloßes Verschieben ließe jeden Eintrag ins
/// Leere zeigen.
///
/// Datei für Datei, und der Eintrag wird sofort nachgezogen: Bricht es
/// mittendrin ab — kein Platz mehr, Erlaubnis entzogen —, zeigt kein einziger
/// Eintrag auf eine Datei, die dort nicht liegt. Der Rest wandert beim
/// nächsten Start.
///
/// Was nicht in der Bibliothek steht, wandert trotzdem mit; es lag im
/// Musikordner und gehört dorthin.
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

    // Zurück bleiben leere Ordner; die dürfen weg, der Rest bleibt liegen.
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

/// Räumt leere Ordner von unten nach oben weg.
fn entleerte_ordner_entfernen(ordner: &std::path::Path) -> std::io::Result<()> {
    for eintrag in std::fs::read_dir(ordner)?.filter_map(Result::ok) {
        let pfad = eintrag.path();
        if pfad.is_dir() {
            let _ = entleerte_ordner_entfernen(&pfad);
        }
    }
    std::fs::remove_dir(ordner)
}

/// Ablage im Papierkorb: Kennung des Titels plus ursprüngliche Endung. Über
/// die Kennung findet das Wiederherstellen die Datei zielsicher wieder,
/// unabhängig davon, wie sie ursprünglich hieß.
fn papierkorb_datei(ordner: &std::path::Path, track_id: i64, quelle: &std::path::Path) -> std::path::PathBuf {
    match quelle.extension().and_then(|e| e.to_str()) {
        Some(endung) => ordner.join(format!("{track_id}.{endung}")),
        None => ordner.join(track_id.to_string()),
    }
}

/// Nimmt das Entfernen zurück: Eintrag wieder sichtbar, Datei zurück an ihren
/// Platz. Fehlt die Datei, bleibt der Eintrag trotzdem stehen; der Abgleich
/// meldet ihn dann als fehlend.
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

    /// Ein Ordner unter `target`, der sich nicht mit anderen Läufen beißt.
    fn testordner(name: &str) -> std::path::PathBuf {
        let pfad = std::env::temp_dir().join(format!("robify-umzug-{name}"));
        let _ = std::fs::remove_dir_all(&pfad);
        std::fs::create_dir_all(&pfad).expect("Testordner");
        pfad
    }

    /// Der Umzug auf dem Telefon: Datei wandert, Eintrag zeigt hinterher.
    ///
    /// Der wunde Punkt ist nicht das Verschieben, sondern der Gleichlauf. Ein
    /// Eintrag, der auf den alten Ort zeigt, während die Datei schon am neuen
    /// liegt, ist ein Titel, der sich nicht mehr abspielen lässt.
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

    /// Was am Ziel schon liegt, wird nicht überschrieben.
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
        // Genau die Fälle aus dem Messlauf, bei denen der Lyric-Kanal als
        // Künstler in der Bibliothek landete.
        assert_eq!(
            split_video_title("Nina Chuba - WILDBERRY LILLET [Lyrics]", Some("xTheLYRICS")),
            Some(("Nina Chuba".into(), "WILDBERRY LILLET".into()))
        );
        assert_eq!(
            split_video_title("Kendrick Lamar - Money Trees (Lyrics)", Some("Vibe Music")),
            Some(("Kendrick Lamar".into(), "Money Trees".into()))
        );

        // Steht der Kanal rechts, sind die Hälften vertauscht.
        assert_eq!(
            split_video_title(
                "Oft Gefragt - AnnenMayKantereit (Offizielles Video)",
                Some("AnnenMayKantereit")
            ),
            Some(("AnnenMayKantereit".into(), "Oft Gefragt".into()))
        );

        // Gedankenstrich statt Bindestrich.
        assert_eq!(
            split_video_title("PA69 – Tropical Island", Some("PA69")),
            Some(("PA69".into(), "Tropical Island".into()))
        );

        // Leerzeichen nur auf einer Seite, aus dem Messlauf.
        assert_eq!(
            split_video_title("The Killers- Mr. Brightside", Some("Julia")),
            Some(("The Killers".into(), "Mr. Brightside".into()))
        );

        // Ohne Trenner bleibt nichts zu zerlegen.
        assert_eq!(split_video_title("Naked", Some("Yeat")), None);

        // Bindestriche in Namen dürfen nicht zerreißen.
        assert_eq!(split_video_title("Jay-Z", Some("Jay-Z")), None);
        assert_eq!(split_video_title("Blink-182", None), None);
        assert_eq!(
            split_video_title("Jay-Z - Empire State of Mind", None),
            Some(("Jay-Z".into(), "Empire State of Mind".into()))
        );
    }

    #[test]
    fn gastkuenstler_ueberleben_die_titelreinigung() {
        // Klammern tragen nicht nur Hinweise auf die Machart.
        assert_eq!(
            split_video_title("Drake - Passionfruit (feat. Someone)", None),
            Some(("Drake".into(), "Passionfruit (feat. Someone)".into()))
        );
        // Gemischte Klammern: „Official Video“ raus, Fassung bleibt.
        assert_eq!(
            split_video_title("Band - Lied (Live) (Official Video)", None),
            Some(("Band".into(), "Lied (Live)".into()))
        );
    }

    #[test]
    fn trennt_kuenstler_nur_am_semikolon() {
        // Kommas und Ampersands gehören oft zum Bandnamen.
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

        // Ohne Marker bleibt alles Hauptkünstler.
        let (main, featured) = parse_artist_field("Daft Punk");
        assert_eq!(main, vec!["Daft Punk"]);
        assert!(featured.is_empty());
    }

    #[test]
    fn kanal_wird_zum_hauptkuenstler() {
        // Der Fall aus der Praxis: SoundCloud-Konto „PA69“.
        let (main, featured) =
            promote_uploader("PA69; Drunken Masters", None, "PA69").unwrap();
        assert_eq!(main, "PA69");
        assert_eq!(featured.as_deref(), Some("Drunken Masters"));

        // Auch wenn der Kanal hinten in der Liste steht.
        let (main, featured) =
            promote_uploader("Drunken Masters; PA69", None, "PA69").unwrap();
        assert_eq!(main, "PA69");
        assert_eq!(featured.as_deref(), Some("Drunken Masters"));

        // Bereits als Gast geführt? Dann rückt er nach vorn.
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
        // Label-, Sampler- und Repost-Kanäle gehören zu keinem Beteiligten.
        assert!(promote_uploader("PA69; Drunken Masters", None, "Hip Hop Charts").is_none());
        assert!(promote_uploader("PA69; Drunken Masters", None, "").is_none());
        // Bei einem einzelnen Künstler gibt es nichts umzustellen.
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
