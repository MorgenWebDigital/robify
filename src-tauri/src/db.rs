//! schema, migrations and the small settings table.
//!
//! note: several connections point at the same file (ui thread and audio
//! thread), which is why wal and a busy timeout are set on every one of them.

use anyhow::Result;
use rusqlite::{params, Connection};
use std::path::Path;

/// opens a connection and sets the pragmas the app relies on
pub fn open(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;
         PRAGMA temp_store = MEMORY;",
    )?;
    Ok(conn)
}

/// creates the schema, adds columns retrofitted later and merges duplicates
pub fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS artists (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL,
    sort_name   TEXT NOT NULL,
    name_key    TEXT NOT NULL UNIQUE,
    mbid        TEXT,
    created_at  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS albums (
    id                  INTEGER PRIMARY KEY,
    artist_id           INTEGER NOT NULL REFERENCES artists(id) ON DELETE CASCADE,
    title               TEXT NOT NULL,
    title_key           TEXT NOT NULL,
    release_type        TEXT NOT NULL DEFAULT 'album',
    release_type_locked INTEGER NOT NULL DEFAULT 0,
    year                INTEGER,
    mbid                TEXT,
    cover               BLOB,
    cover_mime          TEXT,
    created_at          INTEGER NOT NULL,
    UNIQUE (artist_id, title_key)
);

CREATE TABLE IF NOT EXISTS tracks (
    id          INTEGER PRIMARY KEY,
    path        TEXT NOT NULL UNIQUE,
    title       TEXT NOT NULL,
    artist_id   INTEGER NOT NULL REFERENCES artists(id) ON DELETE CASCADE,
    album_id    INTEGER NOT NULL REFERENCES albums(id) ON DELETE CASCADE,
    track_no    INTEGER,
    disc_no     INTEGER,
    duration_ms INTEGER NOT NULL DEFAULT 0,
    genre       TEXT,
    year        INTEGER,
    format      TEXT NOT NULL DEFAULT '',
    source      TEXT,
    source_url  TEXT,
    favorite    INTEGER NOT NULL DEFAULT 0,
    added_at    INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_tracks_artist ON tracks(artist_id);
CREATE INDEX IF NOT EXISTS idx_tracks_album  ON tracks(album_id);
CREATE INDEX IF NOT EXISTS idx_tracks_title  ON tracks(title);

-- a track can come from several artists. `tracks.artist_id` stays the lead
-- artist, this table holds everyone involved, guest contributions included.
CREATE TABLE IF NOT EXISTS track_artists (
    track_id  INTEGER NOT NULL REFERENCES tracks(id)  ON DELETE CASCADE,
    artist_id INTEGER NOT NULL REFERENCES artists(id) ON DELETE CASCADE,
    role      TEXT NOT NULL DEFAULT 'main',
    position  INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (track_id, artist_id)
);
CREATE INDEX IF NOT EXISTS idx_track_artists_artist ON track_artists(artist_id);

CREATE TABLE IF NOT EXISTS lyrics (
    track_id   INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
    synced     TEXT,
    plain      TEXT,
    source     TEXT,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS playlists (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL,
    description TEXT,
    created_at  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS playlist_tracks (
    playlist_id INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
    track_id    INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    position    INTEGER NOT NULL,
    added_at    INTEGER NOT NULL,
    PRIMARY KEY (playlist_id, track_id)
);
CREATE INDEX IF NOT EXISTS idx_pltracks_pos ON playlist_tracks(playlist_id, position);

CREATE TABLE IF NOT EXISTS plays (
    id        INTEGER PRIMARY KEY,
    track_id  INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    played_at INTEGER NOT NULL,
    ms_played INTEGER NOT NULL,
    completed INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_plays_time  ON plays(played_at);
CREATE INDEX IF NOT EXISTS idx_plays_track ON plays(track_id);

CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS recommendations (
    id         INTEGER PRIMARY KEY,
    week_key   TEXT NOT NULL,
    track_id   INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    reason     TEXT NOT NULL,
    position   INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE (week_key, track_id)
);
"#,
    )?;

    // columns added later on, older databases do not know them
    for (table, column, definition) in [
        ("artists", "image", "BLOB"),
        ("artists", "image_mime", "TEXT"),
        ("artists", "bio", "TEXT"),
        ("artists", "source_url", "TEXT"),
        // removed tracks stay as a row so the listening history is not
        // deleted with them (`plays` hangs off it by foreign key). they
        // disappear from the library through this timestamp
        ("tracks", "deleted_at", "INTEGER"),
        // a playlist cover of its own. without it the frontend keeps
        // assembling the mosaic from the albums inside
        ("playlists", "cover", "BLOB"),
        ("playlists", "cover_mime", "TEXT"),
        // removed playlists stay until the action can no longer be undone
        ("playlists", "deleted_at", "INTEGER"),
        // an order of one's own among the favourites.
        //
        // without it whatever came last always stood on top, and reordering
        // was impossible. the default is filled from the previous order just
        // below, so an update rearranges nothing
        ("tracks", "favorite_position", "INTEGER NOT NULL DEFAULT 0"),
        // when the track last ran, regardless of whether it ran long enough
        // to count in the statistics.
        //
        // "recently played" on the home page is a memory of what was heard,
        // not an evaluation. read from `plays` it missed every track skipped
        // after twenty seconds, and that is exactly the case where one wants
        // to find it again
        ("tracks", "last_played_at", "INTEGER"),
        // an order of one's own in the collection.
        //
        // the default is filled from `created_at` when the column is added:
        // that keeps the previous order, newest first, exactly intact for
        // existing collections. left at zero all of them would rank equal,
        // and the collection would look arbitrarily rearranged after an
        // update
        ("playlists", "position", "INTEGER NOT NULL DEFAULT 0"),
    ] {
        add_column_if_missing(conn, table, column, definition)?;
    }

    // playlists without an order of their own take the previous one: newest
    // on top. runs only while not a single one carries a position, so a later
    // reordering is never overwritten
    let ohne_ordnung: i64 = conn.query_row(
        "SELECT COUNT(*) FROM playlists WHERE position != 0",
        [],
        |r| r.get(0),
    )?;
    if ohne_ordnung == 0 {
        conn.execute(
            "UPDATE playlists SET position = (
                 SELECT COUNT(*) FROM playlists p2
                 WHERE p2.created_at > playlists.created_at
                    OR (p2.created_at = playlists.created_at AND p2.id > playlists.id)
             ) + 1",
            [],
        )?;
    }

    // pin down the previous order of the favourites: last added on top.
    // runs only while none of them carries a position, so a later reordering
    // is never overwritten
    let ohne_ordnung: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tracks WHERE favorite = 1 AND favorite_position != 0",
        [],
        |r| r.get(0),
    )?;
    if ohne_ordnung == 0 {
        conn.execute(
            "UPDATE tracks SET favorite_position = (
                 SELECT COUNT(*) FROM tracks t2
                 WHERE t2.favorite = 1
                   AND (t2.added_at > tracks.added_at
                        OR (t2.added_at = tracks.added_at AND t2.id > tracks.id))
             ) + 1
             WHERE favorite = 1",
            [],
        )?;
    }

    // whatever listening history exists fills the new column. without this
    // step "recently played" would stand empty after an update although the
    // rows in `plays` lie untouched next to it
    conn.execute(
        "UPDATE tracks SET last_played_at = (
             SELECT MAX(played_at) FROM plays WHERE plays.track_id = tracks.id
         )
         WHERE last_played_at IS NULL",
        [],
    )?;

    // catch up collections from older versions: until now every track had
    // exactly one artist
    conn.execute(
        "INSERT OR IGNORE INTO track_artists (track_id, artist_id, role, position)
         SELECT id, artist_id, 'main', 0 FROM tracks",
        [],
    )?;

    // releases from before the kinds were told apart.
    //
    // until now the import wrote its guess with the same lock a decision of
    // the user gets, and the guess for anything with an album name was
    // "album". those rows are set loose again so the classification by what
    // is actually there can reach them.
    //
    // only "album", and only where the lock stands at 1: every other value
    // was set on purpose, and what is loosened here is exactly what the
    // faulty import produced. whoever really wants an album says so again,
    // and it stands for good.
    conn.execute(
        "UPDATE albums SET release_type_locked = 0
         WHERE release_type_locked = 1 AND release_type = 'album'",
        [],
    )?;

    merge_duplicates(conn)?;
    Ok(())
}

// merges rows that share the same key by today's reading.
//
// invisible characters in titles (see `is_invisible`) used to produce two
// rows that looked identical in the ui, "RETOX" and "RETOX\u{3164}" for
// instance. the key is insensitive to them by now, and this pass clears up
// the duplicates already in place.
//
// runs at every start and does nothing when there is nothing to do
fn merge_duplicates(conn: &Connection) -> Result<()> {
    // artists first: albums and tracks hang off them
    let artists: Vec<(i64, String, String)> = conn
        .prepare("SELECT id, name, name_key FROM artists ORDER BY id")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<std::result::Result<_, _>>()?;

    let mut behalten: std::collections::HashMap<String, i64> = Default::default();
    let mut zusammenlegen: Vec<(i64, i64)> = Vec::new();
    let mut aenderung = false;

    for (id, name, alter_key) in &artists {
        let key = key_of(name);
        aenderung |= key != *alter_key;
        // the displayed name is to be clean as well
        let sichtbar = clean_text(name);
        if sichtbar != *name {
            conn.execute(
                "UPDATE artists SET name = ?2 WHERE id = ?1",
                params![id, &sichtbar],
            )?;
        }
        match behalten.get(&key) {
            Some(&ziel) => zusammenlegen.push((*id, ziel)),
            None => {
                behalten.insert(key, *id);
            }
        }
    }

    if aenderung || !zusammenlegen.is_empty() {
        for (id, ziel) in zusammenlegen {
            // being named twice on the same track violates the primary key,
            // such rows fall away
            conn.execute(
                "UPDATE OR IGNORE track_artists SET artist_id = ?2 WHERE artist_id = ?1",
                params![id, ziel],
            )?;
            conn.execute("DELETE FROM track_artists WHERE artist_id = ?1", [id])?;
            conn.execute(
                "UPDATE tracks SET artist_id = ?2 WHERE artist_id = ?1",
                params![id, ziel],
            )?;
            conn.execute(
                "UPDATE albums SET artist_id = ?2 WHERE artist_id = ?1",
                params![id, ziel],
            )?;
            conn.execute("DELETE FROM artists WHERE id = ?1", [id])?;
        }

        // to a guaranteed free intermediate value first, then to the real
        // key: otherwise two rows block each other during the swap ("RETOX"
        // already holds the key that "RETOX␣" is to receive)
        conn.execute("UPDATE artists SET name_key = '#' || id", [])?;
        for (key, id) in &behalten {
            conn.execute(
                "UPDATE artists SET name_key = ?2 WHERE id = ?1",
                params![id, key],
            )?;
        }
    }

    // then the albums. the key is artist plus cleaned title
    let albums: Vec<(i64, i64, String, String)> = conn
        .prepare("SELECT id, artist_id, title, title_key FROM albums ORDER BY id")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<std::result::Result<_, _>>()?;

    let mut behalten: std::collections::HashMap<(i64, String), i64> = Default::default();
    let mut zusammenlegen: Vec<(i64, i64)> = Vec::new();
    let mut aenderung = false;

    for (id, artist_id, title, alter_key) in &albums {
        let key = key_of(title);
        aenderung |= key != *alter_key;
        let sichtbar = clean_text(title);
        if sichtbar != *title {
            conn.execute(
                "UPDATE albums SET title = ?2 WHERE id = ?1",
                params![id, &sichtbar],
            )?;
        }
        match behalten.get(&(*artist_id, key.clone())) {
            Some(&ziel) => zusammenlegen.push((*id, ziel)),
            None => {
                behalten.insert((*artist_id, key), *id);
            }
        }
    }

    if !aenderung && zusammenlegen.is_empty() {
        return Ok(());
    }

    for (id, ziel) in zusammenlegen {
        conn.execute(
            "UPDATE tracks SET album_id = ?2 WHERE album_id = ?1",
            params![id, ziel],
        )?;
        // do not lose what the duplicate brought along
        conn.execute(
            "UPDATE albums SET
                 cover      = COALESCE(cover, (SELECT cover FROM albums WHERE id = ?2)),
                 cover_mime = COALESCE(cover_mime, (SELECT cover_mime FROM albums WHERE id = ?2)),
                 year       = COALESCE(year, (SELECT year FROM albums WHERE id = ?2))
             WHERE id = ?1",
            params![ziel, id],
        )?;
        conn.execute("DELETE FROM albums WHERE id = ?1", [id])?;
    }

    conn.execute("UPDATE albums SET title_key = '#' || id", [])?;
    for ((_, key), id) in &behalten {
        conn.execute(
            "UPDATE albums SET title_key = ?2 WHERE id = ?1",
            params![id, key],
        )?;
    }

    Ok(())
}

// `ALTER TABLE … ADD COLUMN` fails when the column is already there, so look
// first
fn add_column_if_missing(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let exists = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(std::result::Result::ok)
        .any(|name| name == column);
    drop(stmt);

    if !exists {
        conn.execute_batch(&format!(
            "ALTER TABLE {table} ADD COLUMN {column} {definition}"
        ))?;
    }
    Ok(())
}

/// the current time as a unix timestamp in seconds
pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

// invisible characters that travel along in titles taken from the net.
//
// they do not change the spelling, they change the comparison key: "RETOX"
// and "RETOX\u{3164}" looked the same and still counted as two albums. the
// hangul fillers even count as letters to rust and survive every check for
// alphanumeric
fn is_invisible(c: char) -> bool {
    matches!(c,
        '\u{00ad}'                  // soft hyphen
        | '\u{115f}' | '\u{1160}'   // hangul fillers (initial/vowel)
        | '\u{180e}'
        | '\u{200b}'..='\u{200f}'   // zero-width and direction marks
        | '\u{202a}'..='\u{202e}'
        | '\u{2060}'..='\u{2064}'
        | '\u{3164}'                // hangul filler
        | '\u{feff}'                // byte order mark
        | '\u{ffa0}'
    )
}

/// strips invisible characters out of a displayed name.
///
/// they come from titles of foreign sources, cannot be seen and only cause
/// confusion: search and sorting stumble over them, and typing the name out
/// does not find the entry again.
pub fn clean_text(value: &str) -> String {
    value
        .chars()
        .filter(|c| !is_invisible(*c))
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// normalised key for recognising duplicate artists and albums
pub fn key_of(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| !is_invisible(*c))
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// reads one setting, `None` where it was never set
pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    use rusqlite::OptionalExtension;
    let value = conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
            r.get::<_, String>(0)
        })
        .optional()?;
    Ok(value)
}

/// writes one setting, overwriting whatever stood there
pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![key, value],
    )?;
    Ok(())
}
