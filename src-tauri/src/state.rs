use crate::downloader::DownloadRegistry;
use crate::player::PlayerHandle;
use parking_lot::Mutex;
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Arc;

pub struct AppState {
    pub db: Mutex<Connection>,
    pub db_path: PathBuf,
    /// Arbeitsverzeichnis für laufende Downloads.
    pub work_dir: PathBuf,
    /// Musikordner des Systems. Rückfallebene für die Bibliothek.
    pub default_library_dir: PathBuf,
    /// Stehen die Ordner fest, oder darf der Nutzer sie wählen?
    ///
    /// Auf dem Telefon stehen sie fest — aber nur, wenn Robify im
    /// Gerätespeicher auch schreiben darf. Fehlt die Erlaubnis, arbeitet es
    /// im eigenen Ordner, und dann wäre „deine Titel liegen in Robify“ eine
    /// Unwahrheit. Darum das gemessene Ergebnis und nicht `cfg!`.
    pub feste_orte: bool,
    pub player: PlayerHandle,
    pub downloads: Arc<DownloadRegistry>,
}

impl AppState {
    /// Die Datenbank, mit einer Spur für den Fall, dass sie klemmt.
    ///
    /// `parking_lot::Mutex` wartet ohne Ende und ohne ein Wort. Stirbt ein
    /// Befehl, während er die Sperre hält — in einer Testfassung reicht dafür
    /// ein Überlauf beim Rechnen —, bleibt sie für immer zu: Jeder weitere
    /// Aufruf hängt, die Oberfläche wartet auf Antworten, die nie kommen,
    /// und nichts davon steht irgendwo. Genau dieses Bild trat einmal auf und
    /// ließ sich hinterher nicht nachstellen.
    ///
    /// Behoben ist es damit nicht, aber sichtbar: Nach zehn Sekunden steht im
    /// Systemprotokoll, dass die Datenbank blockiert, und beim nächsten Mal
    /// gibt es etwas zu lesen statt nur eine stehende App.
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

    /// Ablage für selbst beschaffte Hilfsprogramme (yt-dlp).
    pub fn tools_dir(&self) -> PathBuf {
        self.db_path
            .parent()
            .map(|p| p.join("werkzeuge"))
            .unwrap_or_else(|| PathBuf::from("werkzeuge"))
    }

    /// Ablage für entfernte Dateien. Wird beim ersten Bedarf angelegt.
    ///
    /// Liegt neben der Datenbank, nicht im Musikordner: Der Abgleich soll die
    /// Dateien dort nicht als verwaist wieder einsammeln.
    pub fn trash_dir(&self) -> PathBuf {
        self.db_path
            .parent()
            .map(|p| p.join("papierkorb"))
            .unwrap_or_else(|| PathBuf::from("papierkorb"))
    }

    /// Zielordner der Bibliothek. Einstellung, sonst der Musikordner.
    ///
    /// Auf dem Telefon gibt es die Einstellung nicht: Dort steht der Ordner
    /// fest. Eine früher einmal gesetzte Angabe wird bewusst übergangen — sie
    /// zeigte auf einen Ort, den seit Android 11 kein Dateimanager mehr
    /// sieht, und niemand käme an sie heran, um sie zu ändern.
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

/// Fehlerbrücke zwischen `anyhow` und den Tauri-Commands.
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
