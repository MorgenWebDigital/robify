//! Auswertungen: wöchentliche Empfehlungen und das monatliche bzw.
//! jährliche Wrapped.

use crate::db::now;
use crate::library;
use crate::models::Track;
use anyhow::{anyhow, Result};
use chrono::{Datelike, Local, NaiveDate, TimeZone};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use crate::fehler;

// ---------------------------------------------------------------- Wrapped

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WrappedTrack {
    pub track: Track,
    pub play_count: i64,
    pub ms_played: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WrappedArtist {
    pub artist_id: i64,
    pub name: String,
    pub play_count: i64,
    pub ms_played: i64,
    pub track_count: i64,
    /// Ob ein Künstlerbild hinterlegt ist. Ohne diese Angabe müsste die
    /// Oberfläche das Bild blind anfordern und den Fehlschlag abfangen.
    pub has_image: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WrappedAlbum {
    pub album_id: i64,
    pub title: String,
    pub artist_name: String,
    pub ms_played: i64,
    pub has_cover: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeBucket {
    pub label: String,
    pub ms_played: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Wrapped {
    pub period: String,
    pub start: i64,
    pub end: i64,
    pub total_ms: i64,
    pub total_plays: i64,
    pub distinct_tracks: i64,
    pub distinct_artists: i64,
    /// Gesamtzeit der fünf meistgehörten Titel zusammengenommen.
    pub top_tracks_total_ms: i64,
    pub top_tracks: Vec<WrappedTrack>,
    pub top_artists: Vec<WrappedArtist>,
    pub top_albums: Vec<WrappedAlbum>,
    pub buckets: Vec<TimeBucket>,
    pub busiest_day: Option<TimeBucket>,
}

fn day_start(date: NaiveDate) -> i64 {
    Local
        .from_local_datetime(&date.and_hms_opt(0, 0, 0).expect("gültige Uhrzeit"))
        .single()
        .map(|dt| dt.timestamp())
        .unwrap_or(0)
}

/// Zeitraum bestimmen. `offset` = 0 ist der laufende Zeitraum, -1 der davor.
///
/// Gibt nur die Grenzen zurück, keinen Anzeigenamen: Monatsnamen gehören zur
/// Sprache der Oberfläche, und die kennt der Rust-Teil nicht. Aus `period` und
/// `start` baut das Frontend die Überschrift selbst.
fn period_range(period: &str, offset: i64) -> Result<(i64, i64)> {
    let today = Local::now().date_naive();
    match period {
        "month" => {
            let total = today.year() as i64 * 12 + (today.month0() as i64) + offset;
            let year = total.div_euclid(12) as i32;
            let month0 = total.rem_euclid(12) as u32;
            let start = NaiveDate::from_ymd_opt(year, month0 + 1, 1)
                .ok_or_else(|| anyhow!(fehler!("Ungültiger Monat")))?;
            let end = if month0 == 11 {
                NaiveDate::from_ymd_opt(year + 1, 1, 1)
            } else {
                NaiveDate::from_ymd_opt(year, month0 + 2, 1)
            }
            .ok_or_else(|| anyhow!(fehler!("Ungültiger Monat")))?;
            Ok((day_start(start), day_start(end)))
        }
        "year" => {
            let year = today.year() + offset as i32;
            let start = NaiveDate::from_ymd_opt(year, 1, 1).ok_or_else(|| anyhow!(fehler!("Ungültiges Jahr")))?;
            let end =
                NaiveDate::from_ymd_opt(year + 1, 1, 1).ok_or_else(|| anyhow!(fehler!("Ungültiges Jahr")))?;
            Ok((day_start(start), day_start(end)))
        }
        "all" => Ok((0, now() + 86_400)),
        other => Err(anyhow!(fehler!("Unbekannter Zeitraum: {0}", other))),
    }
}

pub fn wrapped(conn: &Connection, period: &str, offset: i64) -> Result<Wrapped> {
    let (start, end) = period_range(period, offset)?;

    let (total_ms, total_plays): (i64, i64) = conn.query_row(
        "SELECT COALESCE(SUM(ms_played), 0), COUNT(*) FROM plays
         WHERE played_at >= ?1 AND played_at < ?2",
        params![start, end],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;

    let distinct_tracks: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT track_id) FROM plays WHERE played_at >= ?1 AND played_at < ?2",
        params![start, end],
        |r| r.get(0),
    )?;

    // Gezählt wird über `track_artists`, nicht über `tracks.artist_id`: Dort
    // steht nur der Hauptkünstler, Gastbeiträge fielen sonst unter den Tisch.
    let distinct_artists: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT ta.artist_id) FROM plays p
         JOIN track_artists ta ON ta.track_id = p.track_id
         WHERE p.played_at >= ?1 AND p.played_at < ?2",
        params![start, end],
        |r| r.get(0),
    )?;

    // --- Top-Titel
    let mut stmt = conn.prepare(
        "SELECT p.track_id, COUNT(*), SUM(p.ms_played) FROM plays p
         WHERE p.played_at >= ?1 AND p.played_at < ?2
         GROUP BY p.track_id ORDER BY SUM(p.ms_played) DESC LIMIT 5",
    )?;
    let raw: Vec<(i64, i64, i64)> = stmt
        .query_map(params![start, end], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    drop(stmt);

    let mut top_tracks = Vec::new();
    for (track_id, play_count, ms_played) in raw {
        if let Ok(track) = library::get_track(conn, track_id) {
            top_tracks.push(WrappedTrack {
                track,
                play_count,
                ms_played,
            });
        }
    }
    let top_tracks_total_ms = top_tracks.iter().map(|t| t.ms_played).sum();

    // --- Top-Künstler
    //
    // Über `track_artists`, damit Gastbeiträge mitzählen: Wer „Money Trees“
    // hört, hört Kendrick Lamar *und* Jay Rock. Jeder Beteiligte bekommt die
    // volle Hörzeit gutgeschrieben, die Summe aller Künstler übersteigt
    // dadurch die Gesamthörzeit, und das ist richtig so.
    let mut stmt = conn.prepare(
        "SELECT ar.id, ar.name, COUNT(*), SUM(p.ms_played), COUNT(DISTINCT p.track_id),
                (ar.image IS NOT NULL)
         FROM plays p
         JOIN track_artists ta ON ta.track_id = p.track_id
         JOIN artists ar      ON ar.id = ta.artist_id
         WHERE p.played_at >= ?1 AND p.played_at < ?2
         GROUP BY ar.id ORDER BY SUM(p.ms_played) DESC LIMIT 5",
    )?;
    let top_artists: Vec<WrappedArtist> = stmt
        .query_map(params![start, end], |r| {
            Ok(WrappedArtist {
                artist_id: r.get(0)?,
                name: r.get(1)?,
                play_count: r.get(2)?,
                ms_played: r.get(3)?,
                track_count: r.get(4)?,
                has_image: r.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    drop(stmt);

    // --- Top-Releases
    let mut stmt = conn.prepare(
        "SELECT al.id, al.title, ar.name, SUM(p.ms_played), (al.cover IS NOT NULL)
         FROM plays p
         JOIN tracks t   ON t.id = p.track_id
         JOIN albums al  ON al.id = t.album_id
         JOIN artists ar ON ar.id = al.artist_id
         WHERE p.played_at >= ?1 AND p.played_at < ?2
         GROUP BY al.id ORDER BY SUM(p.ms_played) DESC LIMIT 5",
    )?;
    let top_albums: Vec<WrappedAlbum> = stmt
        .query_map(params![start, end], |r| {
            Ok(WrappedAlbum {
                album_id: r.get(0)?,
                title: r.get(1)?,
                artist_name: r.get(2)?,
                ms_played: r.get(3)?,
                has_cover: r.get::<_, i64>(4)? != 0,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    drop(stmt);

    // --- Verlauf: Jahr in Monaten, Monat in Tagen.
    let bucket_format = if period == "year" { "%Y-%m" } else { "%Y-%m-%d" };
    let mut stmt = conn.prepare(
        "SELECT strftime(?3, datetime(played_at, 'unixepoch', 'localtime')) AS bucket,
                SUM(ms_played)
         FROM plays WHERE played_at >= ?1 AND played_at < ?2
         GROUP BY bucket ORDER BY bucket",
    )?;
    let buckets: Vec<TimeBucket> = stmt
        .query_map(params![start, end, bucket_format], |r| {
            Ok(TimeBucket {
                label: r.get(0)?,
                ms_played: r.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    drop(stmt);

    let mut stmt = conn.prepare(
        "SELECT strftime('%Y-%m-%d', datetime(played_at, 'unixepoch', 'localtime')) AS day,
                SUM(ms_played)
         FROM plays WHERE played_at >= ?1 AND played_at < ?2
         GROUP BY day ORDER BY SUM(ms_played) DESC LIMIT 1",
    )?;
    let busiest_day = stmt
        .query_map(params![start, end], |r| {
            Ok(TimeBucket {
                label: r.get(0)?,
                ms_played: r.get(1)?,
            })
        })?
        .next()
        .transpose()?;
    drop(stmt);

    Ok(Wrapped {
        period: period.to_string(),
        start,
        end,
        total_ms,
        total_plays,
        distinct_tracks,
        distinct_artists,
        top_tracks_total_ms,
        top_tracks,
        top_artists,
        top_albums,
        buckets,
        busiest_day,
    })
}

// ---------------------------------------------------------- Empfehlungen

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recommendation {
    pub track: Track,
    /// Wie oft der Titel in dieser Woche lief.
    ///
    /// Zahlen statt fertigem Satz: Ein „5 Mal · 19 min“ aus dem Rust-Teil käme
    /// in jeder Sprache auf Deutsch an. Was daraus wird, entscheidet die
    /// Oberfläche.
    pub play_count: i64,
    /// Wie lange er dabei insgesamt lief.
    pub ms_played: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeeklyMix {
    /// Kalenderwoche, z. B. „2026-KW33“.
    pub week_key: String,
    /// Fortlaufende Nummer ab der ersten Woche mit Hördaten.
    ///
    /// Der Anzeigename entsteht daraus im Frontend: „Wochenmix 7“ ist Text der
    /// Oberfläche und muss ihrer Sprache folgen.
    pub number: i64,
    pub start: i64,
    pub end: i64,
    /// Wie viele Wochen zurück. 0 ist die laufende.
    pub offset: i64,
    /// Gibt es davor noch eine Woche mit Hördaten?
    pub has_older: bool,
    pub items: Vec<Recommendation>,
}

/// Kurzfassung eines Wochenmix für die Übersicht, ohne die Titel selbst.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeeklyMixSummary {
    pub week_key: String,
    pub number: i64,
    pub start: i64,
    pub end: i64,
    pub offset: i64,
    pub track_count: i64,
    /// Alben der meistgehörten Titel, daraus entsteht das Mosaik-Cover.
    pub cover_album_ids: Vec<i64>,
}

/// Die letzten Wochen mit Hördaten, die laufende zuerst.
///
/// Wochen ohne einen einzigen Abspielvorgang werden übersprungen; die
/// laufende bleibt immer stehen, auch wenn sie noch leer ist.
pub fn weekly_mixes(conn: &Connection, limit: usize) -> Result<Vec<WeeklyMixSummary>> {
    let mut out = Vec::with_capacity(limit);

    for offset in 0.. {
        if out.len() >= limit {
            break;
        }
        let (start, end, key) = week_range(offset);

        // Vor der ersten Wiedergabe gibt es nichts mehr zu holen.
        if offset > 0 && !has_plays_before(conn, end)? {
            break;
        }

        let mut stmt = conn.prepare(
            "SELECT t.album_id, SUM(p.ms_played) AS gehoert
               FROM plays p
               JOIN tracks t ON t.id = p.track_id
              WHERE p.played_at >= ?1 AND p.played_at < ?2
              GROUP BY p.track_id
              ORDER BY gehoert DESC
              LIMIT ?3",
        )?;
        let alben: Vec<i64> = stmt
            .query_map(params![start, end, WEEKLY_SIZE as i64], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        drop(stmt);

        if alben.is_empty() && offset > 0 {
            continue;
        }

        // Für das Mosaik zählen verschiedene Alben, nicht verschiedene Titel.
        let mut cover_album_ids: Vec<i64> = Vec::new();
        for album in &alben {
            if !cover_album_ids.contains(album) {
                cover_album_ids.push(*album);
            }
            if cover_album_ids.len() == 4 {
                break;
            }
        }

        out.push(WeeklyMixSummary {
            number: week_number(conn, start)?,
            week_key: key,
            start,
            end,
            offset,
            track_count: alben.len() as i64,
            cover_album_ids,
        });
    }

    Ok(out)
}

/// Beginn der ISO-Woche, die `offset` Wochen zurückliegt.
fn week_range(offset: i64) -> (i64, i64, String) {
    let heute = Local::now().date_naive();
    // Montag der laufenden Woche, dann um die Wochen zurückgehen.
    let montag = heute - chrono::Duration::days(heute.weekday().num_days_from_monday() as i64);
    let start_tag = montag - chrono::Duration::weeks(offset);
    let end_tag = start_tag + chrono::Duration::weeks(1);

    let iso = start_tag.iso_week();
    (
        day_start(start_tag),
        day_start(end_tag),
        format!("{}-KW{:02}", iso.year(), iso.week()),
    )
}

/// So viele Titel umfasst ein Wochenmix.
const WEEKLY_SIZE: usize = 30;

/// Der Wochenmix einer Kalenderwoche: die dreißig meistgehörten Titel.
///
/// Kein Vorschlagswesen, sondern ein Rückblick, die Woche, wie sie war.
/// Deshalb wird nichts gespeichert: Aus den Abspielvorgängen lässt sich jede
/// vergangene Woche jederzeit neu ableiten, auch Jahre später.
///
/// `offset` zählt Wochen zurück; 0 ist die laufende, deren Reihenfolge sich
/// mit jedem Hören noch verschiebt.
pub fn weekly_mix(conn: &Connection, offset: i64) -> Result<WeeklyMix> {
    let offset = offset.max(0);
    let (start, end, key) = week_range(offset);

    let mut stmt = conn.prepare(
        "SELECT p.track_id, SUM(p.ms_played) AS gehoert, COUNT(*) AS male
           FROM plays p
          WHERE p.played_at >= ?1 AND p.played_at < ?2
          GROUP BY p.track_id
          ORDER BY gehoert DESC, male DESC
          LIMIT ?3",
    )?;
    let rows: Vec<(i64, i64, i64)> = stmt
        .query_map(params![start, end, WEEKLY_SIZE as i64], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    drop(stmt);

    let mut items = Vec::with_capacity(rows.len());
    for (track_id, gehoert, male) in rows {
        // Gelöschte Titel fallen still heraus.
        if let Ok(track) = library::get_track(conn, track_id) {
            items.push(Recommendation {
                track,
                play_count: male,
                ms_played: gehoert,
            });
        }
    }

    Ok(WeeklyMix {
        number: week_number(conn, start)?,
        week_key: key,
        start,
        end,
        offset,
        has_older: has_plays_before(conn, start)?,
        items,
    })
}

/// Die wievielte Woche mit Hördaten ist das?
///
/// Gezählt wird ab der ersten Woche überhaupt, damit „Wochenmix 1“ auch in
/// einem Jahr noch dieselbe Woche meint.
fn week_number(conn: &Connection, start: i64) -> Result<i64> {
    let erster: Option<i64> =
        conn.query_row("SELECT MIN(played_at) FROM plays", [], |r| r.get(0))?;

    let Some(erster) = erster else {
        return Ok(1);
    };

    // Beide auf ihren Wochenbeginn bringen und die Wochen dazwischen zählen.
    let woche = 7 * 86_400;
    let erster_start = wochenbeginn(erster);
    Ok(((start - erster_start).max(0) / woche) + 1)
}

/// Zeitstempel auf den Beginn seiner ISO-Woche (Montag, lokal) zurückführen.
fn wochenbeginn(zeitpunkt: i64) -> i64 {
    let datum = chrono::DateTime::from_timestamp(zeitpunkt, 0)
        .map(|utc| utc.with_timezone(&Local).date_naive())
        .unwrap_or_else(|| Local::now().date_naive());
    let montag = datum - chrono::Duration::days(datum.weekday().num_days_from_monday() as i64);
    day_start(montag)
}

fn has_plays_before(conn: &Connection, start: i64) -> Result<bool> {
    let anzahl: i64 = conn.query_row(
        "SELECT COUNT(*) FROM plays WHERE played_at < ?1",
        [start],
        |r| r.get(0),
    )?;
    Ok(anzahl > 0)
}
