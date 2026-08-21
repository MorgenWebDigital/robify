//! shared application state: database, paths and the running player

use crate::downloader::DownloadRegistry;
use crate::player::PlayerHandle;
use parking_lot::Mutex;
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Arc;

pub struct AppState {
    pub db: Mutex<Connection>,
    pub db_path: PathBuf,
    /// working directory for downloads in flight
    pub work_dir: PathBuf,
    /// music folder of the system, the fallback for the library
    pub default_library_dir: PathBuf,
    /// whether the folders are fixed or the user picks them.
    ///
    /// on a phone they are fixed, but only where robify may actually write to
    /// the device storage. without that permission it works inside its own
    /// folder, and then "your tracks are in Robify" would be untrue. hence a
    /// measured result rather than `cfg!`.
    pub feste_orte: bool,
    pub player: PlayerHandle,
    pub downloads: Arc<DownloadRegistry>,
}

impl AppState {
    /// the database, with a trace for the case where it jams.
    ///
    /// `parking_lot::Mutex` waits forever and without a word. if a command
    /// dies while holding the lock, and in a debug build an arithmetic
    /// overflow is enough for that, it stays shut for good: every further
    /// call hangs, the ui waits for answers that never come, and none of it
    /// is written down anywhere. exactly that picture appeared once and could
    /// not be reproduced afterwards.
    ///
    /// this does not fix it, it makes it visible: after ten seconds the
    /// system log says the database is blocked, and next time there is
    /// something to read instead of just a standing app.
    pub fn db(&self) -> parking_lot::MutexGuard<'_, Connection> {
        match self.db.try_lock_for(std::time::Duration::from_secs(10)) {
            Some(griff) => griff,
            None => {
                eprintln!(
                    "Die Datenbank ist seit zehn Sekunden gesperrt. \
                     Ein Befehl hält sie fest oder ist unter ihr gestorben."
                );
                self.db.lock()
            }
        }
    }

    /// storage for tools fetched by the app itself (yt-dlp)
    pub fn tools_dir(&self) -> PathBuf {
        self.db_path
            .parent()
            .map(|p| p.join("werkzeuge"))
            .unwrap_or_else(|| PathBuf::from("werkzeuge"))
    }

    /// storage for removed files, created when first needed.
    ///
    /// sits next to the database, not in the music folder: the reconciliation
    /// pass must not pick these files up again as orphans.
    pub fn trash_dir(&self) -> PathBuf {
        self.db_path
            .parent()
            .map(|p| p.join("papierkorb"))
            .unwrap_or_else(|| PathBuf::from("papierkorb"))
    }

    /// target folder of the library: the setting, otherwise the music folder.
    ///
    /// on a phone the setting does not exist, the folder is fixed there. a
    /// value set at some earlier point is deliberately ignored, it pointed at
    /// a place no file manager has shown since android 11 and nobody could
    /// reach it to change it.
    pub fn library_dir(&self) -> PathBuf {
        if self.feste_orte {
            return self.default_library_dir.clone();
        }

        let configured = {
            let conn = self.db();
            crate::db::get_setting(&conn, "library_dir").ok().flatten()
        };
        match configured.filter(|p| !p.trim().is_empty()) {
            Some(p) => PathBuf::from(p),
            None => self.default_library_dir.clone(),
        }
    }
}

/// error bridge between `anyhow` and the tauri commands
#[derive(Debug)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl serde::Serialize for Error {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl From<anyhow::Error> for Error {
    fn from(err: anyhow::Error) -> Self {
        Error(format!("{err:#}"))
    }
}

impl From<rusqlite::Error> for Error {
    fn from(err: rusqlite::Error) -> Self {
        Error(err.to_string())
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error(err.to_string())
    }
}

impl From<tauri::Error> for Error {
    fn from(err: tauri::Error) -> Self {
        Error(err.to_string())
    }
}

impl From<base64::DecodeError> for Error {
    fn from(err: base64::DecodeError) -> Self {
        Error(err.to_string())
    }
}

impl From<String> for Error {
    fn from(err: String) -> Self {
        Error(err)
    }
}

pub type CmdResult<T> = Result<T, Error>;
