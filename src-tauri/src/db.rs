use anyhow::Result;
use rusqlite::{params, Connection};
use std::path::Path;

/// Öffnet eine Verbindung und stellt sicher, dass Schema und Pragmas stimmen.
/// Es werden mehrere Verbindungen auf dieselbe Datei geöffnet (UI-Thread und
/// Audio-Thread), deshalb WAL + busy_timeout.
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

-- Ein Titel kann von mehreren Künstlern stammen. `tracks.artist_id` bleibt
-- der Hauptkünstler; hier stehen alle Beteiligten inklusive Gastbeiträgen.
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

    // Nachträglich ergänzte Spalten, ältere Datenbanken kennen sie nicht.
    for (table, column, definition) in [
        ("artists", "image", "BLOB"),
        ("artists", "image_mime", "TEXT"),
        ("artists", "bio", "TEXT"),
        ("artists", "source_url", "TEXT"),
        // Entfernte Titel bleiben als Eintrag stehen, damit die Hörhistorie
        // nicht mitgelöscht wird (`plays` hängt per Fremdschlüssel daran).
        // Aus der Bibliothek verschwinden sie über diesen Zeitstempel.
        ("tracks", "deleted_at", "INTEGER"),
        // Eigenes Playlist-Cover. Fehlt es, setzt das Frontend weiter das
        // Mosaik aus den enthaltenen Alben zusammen.
        ("playlists", "cover", "BLOB"),
        ("playlists", "cover_mime", "TEXT"),
        // Entfernte Playlists bleiben stehen, bis der Griff nicht mehr
        // zurückgenommen werden kann.
        ("playlists", "deleted_at", "INTEGER"),
        // Wann der Titel zuletzt lief, unabhängig davon, ob er lang genug
        // lief, um in der Statistik zu zählen.
        //
        // „Zuletzt gespielt“ auf der Startseite ist eine Erinnerung daran,
        // was man gehört hat, keine Auswertung. Aus `plays` gelesen fehlte
        // dort jeder Titel, den man nach zwanzig Sekunden weitergeschaltet
        // hat — und das ist genau der Fall, in dem man ihn wiederfinden will.
        ("tracks", "last_played_at", "INTEGER"),
        // Selbst gewählte Reihenfolge in der Sammlung.
        //
        // Der Vorgabewert ist mit Bedacht negativ und wird beim Nachrüsten
        // aus `created_at` gefüllt: So bleibt die bisherige Ordnung, neueste
        // zuerst, für bestehende Bestände genau erhalten. Mit einer schlichten
        // Null stünden alle gleichauf, und die Sammlung sähe nach dem
        // Aktualisieren willkürlich umsortiert aus.
        ("playlists", "position", "INTEGER NOT NULL DEFAULT 0"),
    ] {
        add_column_if_missing(conn, table, column, definition)?;
    }

    // Playlists ohne eigene Reihenfolge übernehmen die bisherige: neueste
    // oben. Läuft nur, solange noch keine einzige eine Stelle trägt, ein
    // späteres Umsortieren wird dadurch also nie überschrieben.
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

    // Was bisher an Hörhistorie da ist, füllt die neue Spalte. Ohne diesen
    // Schritt stünde „Zuletzt gespielt“ nach dem Aktualisieren leer da,
    // obwohl die Einträge in `plays` unberührt daneben liegen.
    conn.execute(
        "UPDATE tracks SET last_played_at = (
             SELECT MAX(played_at) FROM plays WHERE plays.track_id = tracks.id
         )
         WHERE last_played_at IS NULL",
        [],
    )?;

    // Bestände aus älteren Fassungen nachtragen: bisher hatte jeder Titel
    // genau einen Künstler.
    conn.execute(
        "INSERT OR IGNORE INTO track_artists (track_id, artist_id, role, position)
         SELECT id, artist_id, 'main', 0 FROM tracks",
        [],
    )?;

    merge_duplicates(conn)?;
    Ok(())
}

/// Führt Einträge zusammen, die nach heutiger Lesart denselben Schlüssel
/// haben.
///
/// Unsichtbare Zeichen in Titeln (siehe [`is_invisible`]) haben früher zwei
/// Einträge erzeugt, die in der Oberfläche identisch aussahen, etwa „RETOX“
/// und „RETOX\u{3164}“. Der Schlüssel ist inzwischen unempfindlich dagegen;
/// die bereits doppelten Bestände räumt dieser Durchlauf auf.
///
/// Läuft bei jedem Start und tut nichts, wenn es nichts zu tun gibt.
fn merge_duplicates(conn: &Connection) -> Result<()> {
    // Erst die Künstler: Alben und Titel hängen an ihnen.
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
        // Der angezeigte Name soll ebenfalls sauber sein.
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
            // Zweifachnennung im selben Titel verletzt den Primärschlüssel,
            // solche Zeilen fallen weg.
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

        // Erst auf einen garantiert freien Zwischenwert, dann auf den echten
        // Schlüssel: Sonst blockieren sich zwei Zeilen beim Tausch gegenseitig
        // („RETOX“ hält bereits den Schlüssel, den „RETOX␣“ bekommen soll).
        conn.execute("UPDATE artists SET name_key = '#' || id", [])?;
        for (key, id) in &behalten {
            conn.execute(
                "UPDATE artists SET name_key = ?2 WHERE id = ?1",
                params![id, key],
            )?;
        }
    }

    // Dann die Alben. Schlüssel ist Künstler plus bereinigter Titel.
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
        // Was der Doppelgänger mitbrachte, nicht verlieren.
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

/// `ALTER TABLE … ADD COLUMN` scheitert, wenn es die Spalte schon gibt,
/// deshalb vorher nachsehen.
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

pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Normalisierter Schlüssel für Duplikaterkennung von Künstlern/Alben.
/// Unsichtbare Zeichen, die in Titeln aus dem Netz mitreisen.
///
/// Sie ändern die Schreibweise nicht, wohl aber den Vergleichsschlüssel,
/// „RETOX“ und „RETOX\u{3164}“ sahen dadurch gleich aus, galten aber als
/// zwei Alben. Die Hangul-Füllzeichen zählen für Rust sogar als Buchstaben
/// und überleben deshalb jede Prüfung auf alphanumerisch.
fn is_invisible(c: char) -> bool {
    matches!(c,
        '\u{00ad}'                  // weiches Trennzeichen
        | '\u{115f}' | '\u{1160}'   // Hangul-Füllzeichen (Anlaut/Vokal)
        | '\u{180e}'
        | '\u{200b}'..='\u{200f}'   // Nullbreiten- und Richtungszeichen
        | '\u{202a}'..='\u{202e}'
        | '\u{2060}'..='\u{2064}'
        | '\u{3164}'                // Hangul-Füllzeichen
        | '\u{feff}'                // Byte-Reihenfolge-Markierung
        | '\u{ffa0}'
    )
}

/// Entfernt unsichtbare Zeichen aus einem angezeigten Namen.
///
/// Sie stammen aus Titeln fremder Quellen, sind nicht zu sehen und stiften
/// nur Verwirrung: Suche und Sortierung stolpern darüber, und beim Abtippen
/// findet man den Eintrag nicht wieder.
pub fn clean_text(value: &str) -> String {
    value
        .chars()
        .filter(|c| !is_invisible(*c))
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

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

pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    use rusqlite::OptionalExtension;
    let value = conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
            r.get::<_, String>(0)
        })
        .optional()?;
    Ok(value)
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![key, value],
    )?;
    Ok(())
}
