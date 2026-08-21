//! evaluations: the weekly mixes and the monthly and yearly wrapped

use crate::db::now;
use crate::library;
use crate::models::Track;
use anyhow::{anyhow, Result};
use chrono::{Datelike, Local, NaiveDate, TimeZone};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use crate::fehler;

// --- wrapped ---

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
    /// whether an artist image is stored. without it the ui would have to
    /// request the image blindly and catch the failure.
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
    /// total time of the five most played tracks taken together
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

// determines the period. `offset` = 0 is the running one, -1 the one before.
//
// returns the bounds only, no display name: month names belong to the
// language of the ui, and the rust side does not know it. the frontend builds
// the heading from `period` and `start` itself
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

/// the review of one month, one year or of everything played so far
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

    // counted through `track_artists`, not through `tracks.artist_id`: only
    // the lead artist stands there, guest contributions would fall away
    let distinct_artists: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT ta.artist_id) FROM plays p
         JOIN track_artists ta ON ta.track_id = p.track_id
         WHERE p.played_at >= ?1 AND p.played_at < ?2",
        params![start, end],
        |r| r.get(0),
    )?;

    // --- top tracks ---
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

    // --- top artists ---
    //
    // through `track_artists` so guest contributions count: whoever listens
    // to "Money Trees" listens to kendrick lamar and jay rock. every
    // participant is credited the full listening time, so the sum over all
    // artists exceeds the total listening time, and that is correct
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

    // --- top releases ---
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

    // --- history ---
    //
    // month in days, year and all-time in months. all-time reaches back to
    // the first track ever heard. counted in days that would be 365 bars
    // after a year of daily listening, and in one row on a phone no pixel is
    // left for any of them, leaving the card blank
    let bucket_format = if period == "month" { "%Y-%m-%d" } else { "%Y-%m" };
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

// --- weekly mixes ---

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recommendation {
    pub track: Track,
    /// how often the track ran that week.
    ///
    /// numbers instead of a finished sentence: a "5 Mal · 19 min" from the
    /// rust side would arrive in german whatever the language. what becomes
    /// of it is up to the ui.
    pub play_count: i64,
    /// how long it ran in total while doing so
    pub ms_played: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeeklyMix {
    /// calendar week, "2026-KW33" for instance
    pub week_key: String,
    /// running number counted from the first week with listening data.
    ///
    /// the display name grows out of it in the frontend: "Wochenmix 7" is ui
    /// text and has to follow the ui language.
    pub number: i64,
    pub start: i64,
    pub end: i64,
    /// how many weeks back. 0 is the running one.
    pub offset: i64,
    /// whether another week with listening data lies before it
    pub has_older: bool,
    pub items: Vec<Recommendation>,
}

/// short form of a weekly mix for the overview, without the tracks
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeeklyMixSummary {
    pub week_key: String,
    pub number: i64,
    pub start: i64,
    pub end: i64,
    pub offset: i64,
    pub track_count: i64,
    /// albums of the most played tracks, the mosaic cover grows out of them
    pub cover_album_ids: Vec<i64>,
}

/// the last weeks with listening data, the running one first.
///
/// weeks without a single play are skipped, the running one always stays even
/// while it is still empty.
pub fn weekly_mixes(conn: &Connection, limit: usize) -> Result<Vec<WeeklyMixSummary>> {
    let mut out = Vec::with_capacity(limit);

    for offset in 0.. {
        if out.len() >= limit {
            break;
        }
        let (start, end, key) = week_range(offset);

        // before the first play there is nothing left to fetch
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

        // the mosaic counts distinct albums, not distinct tracks
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

// start of the iso week `offset` weeks back
fn week_range(offset: i64) -> (i64, i64, String) {
    let heute = Local::now().date_naive();
    // monday of the running week, then step back by the weeks
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

/// this many tracks make up a weekly mix
const WEEKLY_SIZE: usize = 30;

/// the weekly mix of one calendar week: the thirty most played tracks.
///
/// no recommendation engine but a look back, the week as it was. nothing is
/// stored for it: every past week can be derived from the plays again at any
/// time, years later too.
///
/// `offset` counts weeks back, 0 is the running one whose order still shifts
/// with every play.
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
        // deleted tracks fall out silently
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

// which week with listening data this is.
//
// counted from the very first week, so that "Wochenmix 1" still means the
// same week a year from now
fn week_number(conn: &Connection, start: i64) -> Result<i64> {
    let erster: Option<i64> =
        conn.query_row("SELECT MIN(played_at) FROM plays", [], |r| r.get(0))?;

    let Some(erster) = erster else {
        return Ok(1);
    };

    // bring both to the start of their week and count the weeks in between
    let woche = 7 * 86_400;
    let erster_start = wochenbeginn(erster);
    Ok(((start - erster_start).max(0) / woche) + 1)
}

// reduces a timestamp to the start of its iso week (monday, local time)
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
