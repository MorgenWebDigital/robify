//! Wiedergabe-Engine. Läuft in einem eigenen Thread, besitzt den Audio-Sink
//! und eine eigene SQLite-Verbindung (für Pfade und Wiedergabe-Statistik).

use crate::db;
use anyhow::{anyhow, Result};
use parking_lot::Mutex;
use rand::seq::SliceRandom;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use crate::fehler;

const TICK: Duration = Duration::from_millis(250);
/// So oft wird die laufende Abspielposition gesichert.
const REMEMBER_EVERY: Duration = Duration::from_secs(5);
/// Ab dieser Hördauer zählt ein Titel als "gehört" (analog zu gängigen Diensten).
const MIN_PLAY_MS: u64 = 30_000;
/// Ab dieser Hördauer merkt sich Robify einen Titel als zuletzt gespielt.
///
/// Viel weniger als für die Statistik, und mit Absicht: „Zuletzt gespielt“
/// beantwortet die Frage „was lief gerade?“, nicht „was höre ich viel?“. Wer
/// einen Titel anspielt und nach zwanzig Sekunden weiterschaltet, will ihn
/// dort wiederfinden. Fünf Sekunden halten nur das draußen, was beim
/// Durchtippen einer Liste entsteht.
const VERLAUF_MS: u64 = 5_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RepeatMode {
    /// Nach dem letzten Titel ist Schluss.
    Off,
    /// Warteschlange endlos wiederholen.
    All,
    /// Aktuellen Titel endlos wiederholen.
    One,
}

impl RepeatMode {
    /// Kurzname für die Einstellungstabelle.
    fn as_str(self) -> &'static str {
        match self {
            RepeatMode::Off => "off",
            RepeatMode::All => "all",
            RepeatMode::One => "one",
        }
    }

    fn parse(value: &str) -> Self {
        match value {
            "all" => RepeatMode::All,
            "one" => RepeatMode::One,
            _ => RepeatMode::Off,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SleepTimerMode {
    /// Nach einer festen Dauer pausieren.
    Duration,
    /// Erst am Ende des laufenden Titels pausieren.
    EndOfTrack,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SleepTimerState {
    pub mode: SleepTimerMode,
    pub total_ms: u64,
    pub remaining_ms: u64,
}

/// Vollständiger Zustand, den das Frontend zum Rendern braucht.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerState {
    pub playing: bool,
    pub track_id: Option<i64>,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub volume: f32,
    pub muted: bool,
    pub repeat: RepeatMode,
    pub shuffle: bool,
    pub queue: Vec<i64>,
    pub queue_index: Option<usize>,
    /// Stellen der Warteschlange in der Reihenfolge, in der sie laufen.
    ///
    /// Ohne Zufallswiedergabe ist das schlicht `0, 1, 2, …`; mit ist es die
    /// gewürfelte Folge. Die Oberfläche braucht sie, um zu zeigen, was noch
    /// kommt: Bei Zufallswiedergabe steht das Kommende nicht hinter dem
    /// laufenden Titel, sondern über die ganze Liste verstreut.
    pub order: Vec<usize>,
    /// Wo in `order` der laufende Titel steht.
    pub order_pos: Option<usize>,
    pub sleep_timer: Option<SleepTimerState>,
}

impl Default for PlayerState {
    fn default() -> Self {
        Self {
            playing: false,
            track_id: None,
            position_ms: 0,
            duration_ms: 0,
            volume: 1.0,
            muted: false,
            repeat: RepeatMode::Off,
            shuffle: false,
            queue: Vec::new(),
            queue_index: None,
            order: Vec::new(),
            order_pos: None,
            sleep_timer: None,
        }
    }
}

pub enum Cmd {
    SetQueue { track_ids: Vec<i64>, start: usize },
    Enqueue(Vec<i64>),
    PlayNext(Vec<i64>),
    RemoveFromQueue(usize),
    ClearQueue,
    Resume,
    Pause,
    TogglePlay,
    Next,
    Prev,
    Stop,
    Seek(u64),
    SetVolume(f32),
    SetMuted(bool),
    SetRepeat(RepeatMode),
    SetShuffle(bool),
    SetSleepTimer(Option<(SleepTimerMode, u64)>),
    Shutdown,
}

#[derive(Clone)]
pub struct PlayerHandle {
    tx: Sender<Cmd>,
    state: Arc<Mutex<PlayerState>>,
}

impl PlayerHandle {
    pub fn send(&self, cmd: Cmd) -> Result<()> {
        self.tx
            .send(cmd)
            .map_err(|_| anyhow!(fehler!("Wiedergabe-Thread reagiert nicht mehr")))
    }

    pub fn state(&self) -> PlayerState {
        self.state.lock().clone()
    }
}

/// Der laufende Player, für Befehle von außerhalb der App.
///
/// Der Systemplayer auf dem Sperrbildschirm kommt nicht über einen
/// Tauri-Befehl herein — er ruft aus der Java-Laufzeit direkt in die
/// Bibliothek, ohne Zustand der Oberfläche und ohne `AppHandle`. Damit er
/// überhaupt jemanden erreicht, wird der Griff hier einmal hinterlegt.
static GRIFF: OnceLock<PlayerHandle> = OnceLock::new();

/// Nimmt entgegen, was am Systemplayer gedrückt wurde.
///
/// Unbekannte Namen bleiben still: Der Aufruf kommt von der Java-Seite, und
/// ein Absturz im Ton-Faden wäre dort nicht zu retten.
pub fn fernbefehl(name: &str, wert: u64) {
    let Some(griff) = GRIFF.get() else {
        return;
    };
    let befehl = match name {
        "resume" => Cmd::Resume,
        "pause" => Cmd::Pause,
        "toggle" => Cmd::TogglePlay,
        "next" => Cmd::Next,
        "prev" => Cmd::Prev,
        "seek" => Cmd::Seek(wert),
        _ => return,
    };
    let _ = griff.send(befehl);
}

pub fn spawn(app: AppHandle, db_path: PathBuf) -> Result<PlayerHandle> {
    let (tx, rx) = std::sync::mpsc::channel();
    let state = Arc::new(Mutex::new(PlayerState::default()));
    let handle = PlayerHandle {
        tx,
        state: state.clone(),
    };
    let _ = GRIFF.set(handle.clone());

    std::thread::Builder::new()
        .name("robify-audio".into())
        .spawn(move || {
            if let Err(err) = run(app.clone(), db_path, rx, state) {
                let _ = app.emit("player:error", err.to_string());
            }
        })?;

    Ok(handle)
}

struct Engine {
    app: AppHandle,
    conn: Connection,
    _speakers: rodio::MixerDeviceSink,
    player: rodio::Player,
    shared: Arc<Mutex<PlayerState>>,

    queue: Vec<i64>,
    /// Abspielreihenfolge als Indizes in `queue` (bei Zufall gemischt).
    order: Vec<usize>,
    order_pos: Option<usize>,

    repeat: RepeatMode,
    shuffle: bool,
    volume: f32,
    muted: bool,

    /// Läuft gerade eine Quelle im Sink?
    loaded: bool,
    duration_ms: u64,
    /// Tatsächlich gehörte Millisekunden des laufenden Titels.
    listened_ms: u64,
    last_tick: Instant,
    /// Wann der Zustand zuletzt gesichert wurde.
    last_remembered: Instant,
    /// Was zuletzt als Takt hinausging. Ändert sich nichts, wird nichts
    /// gesendet, sonst liefe im Leerlauf viermal je Sekunde eine Meldung
    /// über die Brücke und löste im Frontend ein Neuzeichnen aus.
    letzter_takt: Option<(bool, u64, u64, Option<u64>)>,
    /// Welcher Titel dem System zuletzt gemeldet wurde.
    ///
    /// Nur beim Wechsel wird das Cover mitgeschickt; sonst ginge es bei jedem
    /// Druck auf Pause erneut durch die Java-Brücke.
    gemeldeter_titel: Option<i64>,

    sleep_mode: Option<SleepTimerMode>,
    sleep_total_ms: u64,
    sleep_remaining_ms: u64,
}

fn run(
    app: AppHandle,
    db_path: PathBuf,
    rx: Receiver<Cmd>,
    shared: Arc<Mutex<PlayerState>>,
) -> Result<()> {
    let speakers = rodio::DeviceSinkBuilder::open_default_sink()
        .map_err(|e| anyhow!(fehler!("Kein Audiogerät verfügbar: {0}", e)))?;
    let player = rodio::Player::connect_new(speakers.mixer());

    let conn = db::open(&db_path)?;
    let volume = db::get_setting(&conn, "volume")?
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    player.set_volume(volume as _);

    let mut engine = Engine {
        app,
        conn,
        _speakers: speakers,
        player,
        shared,
        queue: Vec::new(),
        order: Vec::new(),
        order_pos: None,
        repeat: RepeatMode::Off,
        shuffle: false,
        volume,
        muted: false,
        loaded: false,
        duration_ms: 0,
        listened_ms: 0,
        last_tick: Instant::now(),
        last_remembered: Instant::now(),
        letzter_takt: None,
        gemeldeter_titel: None,
        sleep_mode: None,
        sleep_total_ms: 0,
        sleep_remaining_ms: 0,
    };
    engine.restore();
    engine.publish(true);

    loop {
        match rx.recv_timeout(TICK) {
            Ok(Cmd::Shutdown) | Err(RecvTimeoutError::Disconnected) => {
                engine.record_play();
                engine.remember();
                return Ok(());
            }
            Ok(cmd) => {
                if let Err(err) = engine.handle(cmd) {
                    let _ = engine.app.emit("player:error", err.to_string());
                }
                engine.publish(true);
                engine.remember();
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
        engine.tick();
    }
}

impl Engine {
    // ------------------------------------------------------------ Kommandos

    fn handle(&mut self, cmd: Cmd) -> Result<()> {
        match cmd {
            Cmd::SetQueue { track_ids, start } => {
                self.record_play();
                self.queue = track_ids;
                self.rebuild_order(Some(start.min(self.queue.len().saturating_sub(1))));
                self.start_current()?;
            }
            Cmd::Enqueue(ids) => {
                let was_empty = self.queue.is_empty();
                let first_new = self.queue.len();
                self.queue.extend(ids);
                self.extend_order(first_new);
                if was_empty && !self.queue.is_empty() {
                    self.rebuild_order(Some(0));
                    self.start_current()?;
                }
            }
            Cmd::PlayNext(ids) => {
                let insert_at = self
                    .current_queue_index()
                    .map(|i| i + 1)
                    .unwrap_or(self.queue.len());
                for (offset, id) in ids.into_iter().enumerate() {
                    self.queue.insert(insert_at + offset, id);
                }
                let current = self.current_queue_index();
                self.rebuild_order_keeping(current);
            }
            Cmd::RemoveFromQueue(index) => {
                if index < self.queue.len() {
                    let current = self.current_queue_index();
                    if current == Some(index) {
                        self.queue.remove(index);
                        self.rebuild_order(Some(index.min(self.queue.len().saturating_sub(1))));
                        if self.queue.is_empty() {
                            self.stop_playback();
                        } else {
                            self.start_current()?;
                        }
                    } else {
                        self.queue.remove(index);
                        let keep = current.map(|c| if c > index { c - 1 } else { c });
                        self.rebuild_order_keeping(keep);
                    }
                }
            }
            Cmd::ClearQueue => {
                self.record_play();
                self.queue.clear();
                self.order.clear();
                self.order_pos = None;
                self.stop_playback();
            }
            Cmd::Resume => {
                if self.loaded {
                    self.player.play();
                } else if !self.queue.is_empty() {
                    if self.order_pos.is_none() {
                        self.order_pos = Some(0);
                    }
                    self.start_current()?;
                }
            }
            Cmd::Pause => self.player.pause(),
            Cmd::TogglePlay => {
                if self.loaded && !self.player.is_paused() {
                    self.player.pause();
                } else {
                    return self.handle(Cmd::Resume);
                }
            }
            Cmd::Next => self.advance(true)?,
            Cmd::Prev => {
                // Innerhalb der ersten Sekunden zurück, sonst an den Anfang.
                if self.position_ms() > 3_000 {
                    let _ = self.player.try_seek(Duration::ZERO);
                } else {
                    self.previous()?;
                }
            }
            Cmd::Stop => {
                self.record_play();
                self.stop_playback();
            }
            Cmd::Seek(ms) => {
                if self.loaded {
                    self.seek_to(ms)?;
                }
            }
            Cmd::SetVolume(v) => {
                self.volume = v.clamp(0.0, 1.0);
                self.apply_volume();
                let _ = db::set_setting(&self.conn, "volume", &self.volume.to_string());
            }
            Cmd::SetMuted(m) => {
                self.muted = m;
                self.apply_volume();
            }
            Cmd::SetRepeat(mode) => {
                self.repeat = mode;
                let _ = db::set_setting(&self.conn, "player_repeat", mode.as_str());
            }
            Cmd::SetShuffle(on) => {
                self.shuffle = on;
                let current = self.current_queue_index();
                self.rebuild_order_keeping(current);
            }
            Cmd::SetSleepTimer(timer) => match timer {
                Some((mode, ms)) => {
                    self.sleep_mode = Some(mode);
                    self.sleep_total_ms = ms;
                    self.sleep_remaining_ms = ms;
                }
                None => {
                    self.sleep_mode = None;
                    self.sleep_total_ms = 0;
                    self.sleep_remaining_ms = 0;
                }
            },
            Cmd::Shutdown => {}
        }
        Ok(())
    }

    fn apply_volume(&self) {
        let effective = if self.muted { 0.0 } else { self.volume };
        self.player.set_volume(effective as _);
    }

    // ------------------------------------------------------- Reihenfolge

    fn current_queue_index(&self) -> Option<usize> {
        self.order_pos.and_then(|p| self.order.get(p).copied())
    }

    fn rebuild_order(&mut self, start: Option<usize>) {
        self.order = (0..self.queue.len()).collect();
        if self.shuffle {
            if let Some(start) = start {
                self.order.retain(|i| *i != start);
                self.order.shuffle(&mut rand::rng());
                self.order.insert(0, start);
                self.order_pos = Some(0);
                return;
            }
            self.order.shuffle(&mut rand::rng());
        }
        self.order_pos = start.filter(|_| !self.queue.is_empty()).map(|s| {
            self.order
                .iter()
                .position(|i| *i == s)
                .unwrap_or(0)
        });
    }

    /// Reihenfolge neu aufbauen, ohne den laufenden Titel zu unterbrechen.
    fn rebuild_order_keeping(&mut self, current: Option<usize>) {
        match current {
            Some(c) if c < self.queue.len() => self.rebuild_order(Some(c)),
            _ => {
                self.order = (0..self.queue.len()).collect();
                if self.shuffle {
                    self.order.shuffle(&mut rand::rng());
                }
                self.order_pos = None;
            }
        }
    }

    fn extend_order(&mut self, from: usize) {
        let new: Vec<usize> = (from..self.queue.len()).collect();
        self.order.extend(new);
    }

    // -------------------------------------------------------- Wiedergabe

    fn start_current(&mut self) -> Result<()> {
        let Some(index) = self.current_queue_index() else {
            self.stop_playback();
            return Ok(());
        };
        let Some(track_id) = self.queue.get(index).copied() else {
            self.stop_playback();
            return Ok(());
        };
        self.load(track_id)
    }

    fn load(&mut self, track_id: i64) -> Result<()> {
        self.load_at(track_id, 0, true)
    }

    /// Lädt einen Titel, wahlweise pausiert und ab einer Stelle.
    ///
    /// Beim Start der App wird der zuletzt gehörte Titel so zurückgeholt:
    /// sichtbar in der Leiste, an der Stelle von damals, aber still. Wer
    /// weiterhören will, drückt Abspielen; wer nicht, sieht wenigstens, wo
    /// er stehengeblieben ist.
    /// Springt an eine Stelle im laufenden Titel.
    ///
    /// Der Dekoder kann das Spulen ablehnen, bei MP3 ohne Sprungtabelle etwa
    /// muss Symphonia schätzen und gibt bei manchen Dateien auf. Dann wird der
    /// Titel neu geöffnet und gleich an der Zielstelle begonnen: Ein frischer
    /// Dekoder liest die Datei von vorn und kommt meist durch, wo der
    /// angefangene scheitert.
    fn seek_to(&mut self, ms: u64) -> Result<()> {
        let ziel = Duration::from_millis(ms);
        if self.player.try_seek(ziel).is_ok() {
            return Ok(());
        }

        let Some(index) = self.current_queue_index() else {
            return Err(anyhow!(fehler!("Spulen nicht möglich: Es läuft nichts.")));
        };
        let Some(track_id) = self.queue.get(index).copied() else {
            return Err(anyhow!(fehler!("Spulen nicht möglich: Es läuft nichts.")));
        };

        let lief = !self.player.is_paused();
        // Die bereits gezählte Hördauer geht beim Neuladen sonst verloren.
        let gehoert = self.listened_ms;
        self.load_at(track_id, ms, lief)
            .map_err(|e| anyhow!(fehler!("Spulen nicht möglich: {0}", e)))?;
        self.listened_ms = gehoert;

        // `load_at` verschluckt einen gescheiterten Sprung, damit ein Titel
        // notfalls wenigstens von vorn läuft. Hier ist das die falsche
        // Antwort: Wer spult, will nicht heimlich am Anfang landen.
        let erreicht = self.player.get_pos().as_millis() as u64;
        if ms > 2_000 && erreicht + 2_000 < ms {
            return Err(anyhow!(
                "In dieser Datei lässt sich nicht springen. Das liegt an ihrer \
                 Kodierung, nicht an der Stelle: Bei MP3 ohne Sprungtabelle \
                 kommt das vor. Ein erneuter Download in einem anderen Format \
                 behebt es."
            ));
        }
        Ok(())
    }

    fn load_at(&mut self, track_id: i64, start_ms: u64, play: bool) -> Result<()> {
        let (path, duration_ms): (String, i64) = self.conn.query_row(
            "SELECT path, duration_ms FROM tracks WHERE id = ?1",
            [track_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;

        let file = File::open(&path).map_err(|e| anyhow!(fehler!("Datei nicht lesbar ({0}): {1}", path, e)))?;
        let source = rodio::Decoder::try_from(BufReader::new(file)).map_err(|e| {
            // Opus ist der häufige Fall: Symphonia bringt dafür keinen
            // Dekoder mit. Neue Downloads meiden das Format, ältere Dateien
            // liegen aber noch in der Bibliothek.
            if !crate::downloader::is_playable(std::path::Path::new(&path)) {
                anyhow!(
                    "Dieses Format kann Robify nicht abspielen ({path}). \
                     Lade den Titel erneut, der Downloader wählt jetzt ein \
                     abspielbares Format."
                )
            } else {
                anyhow!(fehler!("Datei lässt sich nicht lesen ({0}): {1}", path, e))
            }
        })?;

        self.player.clear();
        self.player.append(source);
        self.apply_volume();

        self.loaded = true;
        self.duration_ms = duration_ms.max(0) as u64;
        self.listened_ms = 0;
        self.last_tick = Instant::now();

        // Erst spulen, dann entscheiden, ob es losgeht.
        if start_ms > 0 {
            let ziel = Duration::from_millis(start_ms.min(self.duration_ms));
            let _ = self.player.try_seek(ziel);
        }
        if play {
            self.player.play();
        } else {
            self.player.pause();
        }
        Ok(())
    }

    /// Holt den zuletzt gehörten Titel zurück, pausiert und an der Stelle,
    /// an der er verlassen wurde.
    ///
    /// Scheitert irgendetwas daran (Datei weg, Format nicht lesbar), bleibt
    /// der Player einfach leer. Ein fehlgeschlagenes Wiederherstellen darf
    /// den Start nicht aufhalten.
    fn restore(&mut self) {
        // Die Wiederholart gilt unabhängig davon, ob noch eine Warteschlange
        // gemerkt ist, deshalb vor dem Rücksprung weiter unten.
        if let Ok(Some(wert)) = db::get_setting(&self.conn, "player_repeat") {
            self.repeat = RepeatMode::parse(&wert);
        }

        let gespeichert = db::get_setting(&self.conn, "player_queue")
            .ok()
            .flatten()
            .unwrap_or_default();

        let queue: Vec<i64> = gespeichert
            .split(',')
            .filter_map(|id| id.trim().parse().ok())
            .collect();
        if queue.is_empty() {
            return;
        }

        let zahl = |schluessel: &str| -> u64 {
            db::get_setting(&self.conn, schluessel)
                .ok()
                .flatten()
                .and_then(|wert| wert.parse().ok())
                .unwrap_or(0)
        };
        let index = (zahl("player_index") as usize).min(queue.len() - 1);
        let position = zahl("player_position_ms");

        self.queue = queue;
        self.rebuild_order(Some(index));

        let Some(track_id) = self.queue.get(index).copied() else {
            return;
        };
        if self.load_at(track_id, position, false).is_err() {
            // Der Titel ist nicht mehr abspielbar, dann eben ohne.
            self.queue.clear();
            self.order.clear();
            self.order_pos = None;
            self.stop_playback();
        }
    }

    /// Merkt sich, was gerade läuft. Warteschlange, Stelle darin und
    /// Abspielposition. Beim nächsten Start steht der Titel wieder da.
    fn remember(&mut self) {
        let queue = self
            .queue
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let index = self.current_queue_index().unwrap_or(0);

        let _ = db::set_setting(&self.conn, "player_queue", &queue);
        let _ = db::set_setting(&self.conn, "player_repeat", self.repeat.as_str());
        let _ = db::set_setting(&self.conn, "player_index", &index.to_string());
        let _ = db::set_setting(
            &self.conn,
            "player_position_ms",
            &self.position_ms().to_string(),
        );
        self.last_remembered = Instant::now();
    }

    fn stop_playback(&mut self) {
        self.player.clear();
        self.loaded = false;
        self.duration_ms = 0;
        self.listened_ms = 0;
    }

    fn position_ms(&self) -> u64 {
        if self.loaded {
            self.player.get_pos().as_millis() as u64
        } else {
            0
        }
    }

    /// Nächster Titel. `manual` unterscheidet Nutzerklick von Titelende.
    fn advance(&mut self, manual: bool) -> Result<()> {
        self.record_play();

        // „Titel wiederholen“ gilt auch für den Klick.
        //
        // Vorher sprang ein Klick trotzdem weiter, mit der Überlegung, der
        // Nutzer wolle ja gerade wechseln. In der Hand liest es sich anders:
        // Wer einen Titel auf Dauerschleife stellt, will ihn beim Weitertippen
        // wieder hören, nicht die Einstellung umgangen bekommen.
        if self.repeat == RepeatMode::One {
            return self.start_current();
        }
        if self.queue.is_empty() {
            self.stop_playback();
            return Ok(());
        }

        let am_ende = matches!(self.order_pos, Some(pos) if pos + 1 >= self.order.len());
        let next = naechste_stelle(self.order_pos, self.order.len(), manual, self.repeat);

        // Neue Zufallsrunde beim Umlauf, sonst wiederholt sich die Reihenfolge.
        if am_ende && next == Some(0) && self.shuffle {
            self.order.shuffle(&mut rand::rng());
        }

        match next {
            Some(pos) => {
                self.order_pos = Some(pos);
                self.vorgeschichte_kuerzen();
                self.start_current()
            }
            None => {
                self.order_pos = None;
                self.stop_playback();
                Ok(())
            }
        }
    }

    /// Wirft ab, was zu lange her ist.
    ///
    /// Gehörte Titel bleiben in der Warteschlange stehen, sonst führte der
    /// Rückwärtsschritt ins Leere. Endlos wachsen soll sie deswegen aber
    /// nicht: Bei jeder Änderung holt die Oberfläche zu *jedem* Eintrag die
    /// Angaben aus der Datenbank, auch zu denen, die niemand mehr sieht. Nach
    /// einem Abend Musik wären das Hunderte Abfragen je Titelwechsel.
    fn vorgeschichte_kuerzen(&mut self) {
        let Some(pos) = self.order_pos else { return };
        let Some((queue, order, neue_stelle)) =
            vorgeschichte_kuerzen(&self.queue, &self.order, pos, MAX_VORGESCHICHTE)
        else {
            return;
        };
        self.queue = queue;
        self.order = order;
        self.order_pos = Some(neue_stelle);
    }

    fn previous(&mut self) -> Result<()> {
        self.record_play();
        if self.queue.is_empty() {
            return Ok(());
        }
        self.order_pos = Some(vorherige_stelle(self.order_pos, self.order.len()));
        self.start_current()
    }

    // ------------------------------------------------------------ Statistik

    /// Schreibt die Hördauer des laufenden Titels in die Datenbank.
    fn record_play(&mut self) {
        let Some(index) = self.current_queue_index() else {
            self.listened_ms = 0;
            return;
        };
        let Some(track_id) = self.queue.get(index).copied() else {
            self.listened_ms = 0;
            return;
        };
        let listened = self.listened_ms;
        self.listened_ms = 0;

        // Der Verlauf zuerst: Er hat seine eigene, viel niedrigere Schwelle
        // und darf nicht daran hängen, ob der Titel für die Statistik zählt.
        if listened >= VERLAUF_MS {
            let _ = self.conn.execute(
                "UPDATE tracks SET last_played_at = ?2 WHERE id = ?1",
                params![track_id, db::now()],
            );
        }

        let completed = self.duration_ms > 0 && listened * 100 >= self.duration_ms * 90;
        if listened < MIN_PLAY_MS && !completed {
            // Für die Auswertung zu kurz. Die Oberfläche erfährt es trotzdem,
            // sonst bliebe die Startseite stehen, bis etwas lange genug lief.
            if listened >= VERLAUF_MS {
                let _ = self.app.emit("library:plays-changed", track_id);
            }
            return;
        }
        let _ = self.conn.execute(
            "INSERT INTO plays (track_id, played_at, ms_played, completed) VALUES (?1, ?2, ?3, ?4)",
            params![
                track_id,
                db::now(),
                listened as i64,
                i64::from(completed)
            ],
        );
        let _ = self.app.emit("library:plays-changed", track_id);
    }

    // ---------------------------------------------------------------- Tick

    fn tick(&mut self) {
        let elapsed = self.last_tick.elapsed();
        self.last_tick = Instant::now();
        let elapsed_ms = elapsed.as_millis() as u64;
        let is_playing = self.loaded && !self.player.is_paused();

        if is_playing {
            self.listened_ms += elapsed_ms;
            // Die Position wandert ständig; alle paar Sekunden genügt.
            if self.last_remembered.elapsed() >= REMEMBER_EVERY {
                self.remember();
            }
        }

        // Titel zu Ende: Sink ist leer, obwohl nicht pausiert wurde.
        if self.loaded && self.player.empty() {
            self.loaded = false;
            if self.sleep_mode == Some(SleepTimerMode::EndOfTrack) {
                self.record_play();
                self.stop_playback();
                self.clear_sleep_timer("Sleeptimer: Wiedergabe beendet");
            } else if let Err(err) = self.advance(false) {
                let _ = self.app.emit("player:error", err.to_string());
            }
            self.publish(true);
            return;
        }

        if is_playing && self.sleep_mode == Some(SleepTimerMode::Duration) {
            self.sleep_remaining_ms = self.sleep_remaining_ms.saturating_sub(elapsed_ms);
            if self.sleep_remaining_ms == 0 {
                self.player.pause();
                self.clear_sleep_timer("Sleeptimer abgelaufen, Wiedergabe pausiert");
                self.publish(true);
                return;
            }
        }

        self.publish(false);
    }

    fn clear_sleep_timer(&mut self, message: &str) {
        self.sleep_mode = None;
        self.sleep_total_ms = 0;
        self.sleep_remaining_ms = 0;
        let _ = self.app.emit("player:sleep-timer-fired", message);
    }

    fn snapshot(&self) -> PlayerState {
        PlayerState {
            playing: self.loaded && !self.player.is_paused(),
            track_id: self.current_queue_index().and_then(|i| self.queue.get(i).copied()),
            position_ms: self.position_ms(),
            duration_ms: self.duration_ms,
            volume: self.volume,
            muted: self.muted,
            repeat: self.repeat,
            shuffle: self.shuffle,
            queue: self.queue.clone(),
            queue_index: self.current_queue_index(),
            order: self.order.clone(),
            order_pos: self.order_pos,
            sleep_timer: self.sleep_mode.map(|mode| SleepTimerState {
                mode,
                total_ms: self.sleep_total_ms,
                remaining_ms: self.sleep_remaining_ms,
            }),
        }
    }

    /// `full` löst ein State-Event aus; sonst nur der günstige Positions-Tick.
    /// Vollständiger Zustand. Für Ereignisse, die mehr als die Position
    /// ändern: Titelwechsel, Warteschlange, Lautstärke.
    fn publish(&mut self, full: bool) {
        if full {
            let snapshot = self.snapshot();
            *self.shared.lock() = snapshot.clone();
            self.letzter_takt = Some((
                snapshot.playing,
                snapshot.position_ms,
                snapshot.duration_ms,
                snapshot.sleep_timer.as_ref().map(|s| s.remaining_ms),
            ));
            let _ = self.app.emit("player:state", &snapshot);
            self.dem_system_melden(&snapshot);
        } else {
            self.publish_tick();
        }
    }

    /// Sagt dem System, was läuft, damit es seinen eigenen Player zeigt.
    ///
    /// Nur beim vollen Stand, nicht im Takt: Die Meldung trägt neben der
    /// Position auch die Geschwindigkeit, und damit rechnet Android die Zeit
    /// selbst weiter. Vier Meldungen je Sekunde wären dieselbe Anzeige zum
    /// vierfachen Preis, jede davon mit einem Sprung in die Java-Laufzeit.
    fn dem_system_melden(&mut self, snapshot: &PlayerState) {
        let Some(track_id) = snapshot.track_id else {
            crate::medien::beenden();
            return;
        };

        let angaben: rusqlite::Result<(String, Option<String>, Option<String>, Option<i64>)> =
            self.conn.query_row(
                "SELECT t.title,
                        (SELECT name FROM artists WHERE id = t.artist_id),
                        (SELECT title FROM albums WHERE id = t.album_id),
                        t.album_id
                   FROM tracks t WHERE t.id = ?1",
                [track_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            );
        let Ok((titel, kuenstler, album, album_id)) = angaben else {
            return;
        };

        // Das Cover nur beim Titelwechsel holen: Es liegt als Blob in der
        // Datenbank, und ein paar hundert Kilobyte bei jedem Druck auf Pause
        // durch die Java-Brücke zu schieben wäre Verschwendung.
        //
        // `None` heißt drüben „unverändert“, nicht „keins“. Ein Titel ohne
        // Cover schickt deshalb ein leeres Feld: Sonst bliebe das Bild des
        // vorigen stehen. Umgekehrt verschwand es anfangs beim ersten
        // Pausieren, weil dort beides gleich aussah.
        let wechsel = self.gemeldeter_titel != Some(track_id);
        self.gemeldeter_titel = Some(track_id);
        let cover = wechsel.then(|| {
            album_id
                .and_then(|id| crate::library::album_cover(&self.conn, id).ok().flatten())
                .map(|(daten, _mime)| daten)
                .unwrap_or_default()
        });

        crate::medien::melden(&crate::medien::Angabe {
            titel,
            kuenstler: kuenstler.unwrap_or_default(),
            album: album.unwrap_or_default(),
            dauer_ms: snapshot.duration_ms,
            position_ms: snapshot.position_ms,
            laeuft: snapshot.playing,
            cover,
        });
    }

    /// Nur die vier Werte, die sich beim Abspielen ständig ändern.
    ///
    /// Bleibt alles gleich, pausiert, gestoppt, nichts geladen, geht nichts
    /// hinaus. Vorher lief der Takt auch im Leerlauf: viermal je Sekunde eine
    /// Meldung samt Abschrift der ganzen Warteschlange, dazu ein Neuzeichnen
    /// im Frontend, obwohl sich nichts bewegte.
    fn publish_tick(&mut self) {
        let jetzt = (
            self.loaded && !self.player.is_paused(),
            self.position_ms(),
            self.duration_ms,
            self.sleep_mode.map(|_| self.sleep_remaining_ms),
        );
        if self.letzter_takt == Some(jetzt) {
            return;
        }
        self.letzter_takt = Some(jetzt);

        // Der zwischengespeicherte Zustand hängt an der Position und muss
        // deshalb mitwandern; die Warteschlange wird dabei einmal kopiert,
        // aber eben nur, wenn sich wirklich etwas bewegt hat.
        *self.shared.lock() = self.snapshot();

        let _ = self.app.emit(
            "player:tick",
            serde_json::json!({
                "playing": jetzt.0,
                "positionMs": jetzt.1,
                "durationMs": jetzt.2,
                "sleepRemainingMs": jetzt.3,
            }),
        );
    }
}

/// So viele gehörte Titel bleiben hinter dem laufenden erreichbar.
///
/// Zwanzig statt einer Zeitspanne: Zurückgehen ist ein Schritt durch Titel,
/// nicht durch Minuten. Wie weit man kommt, soll sich zählen lassen und nicht
/// davon abhängen, wie lange die Stücke waren oder wie lange die App offen
/// stand.
const MAX_VORGESCHICHTE: usize = 20;

/// Kürzt die Vorgeschichte und liefert Warteschlange, Reihenfolge und Stelle.
///
/// `None`, wenn nichts zu tun ist. Entfernt werden genau die ersten Einträge
/// der Abspielreihenfolge; welche Stellen der Warteschlange das sind, steht
/// erst darin. Bei Zufallswiedergabe liegen sie verstreut, deshalb die
/// Umrechnung: Jede verbleibende Stelle rutscht um so viele Plätze vor, wie
/// vor ihr weggefallen sind.
fn vorgeschichte_kuerzen(
    queue: &[i64],
    order: &[usize],
    order_pos: usize,
    behalten: usize,
) -> Option<(Vec<i64>, Vec<usize>, usize)> {
    let zu_viel = order_pos.checked_sub(behalten)?;
    if zu_viel == 0 {
        return None;
    }

    let weg: std::collections::HashSet<usize> = order[..zu_viel].iter().copied().collect();

    // Abbildung alte Stelle -> neue Stelle, in einem Durchgang mit dem Aussieben.
    let mut neue_stelle = vec![usize::MAX; queue.len()];
    let mut neue_queue = Vec::with_capacity(queue.len() - zu_viel);
    for (alt, id) in queue.iter().enumerate() {
        if weg.contains(&alt) {
            continue;
        }
        neue_stelle[alt] = neue_queue.len();
        neue_queue.push(*id);
    }

    let neue_order = order[zu_viel..].iter().map(|alt| neue_stelle[*alt]).collect();
    Some((neue_queue, neue_order, order_pos - zu_viel))
}

/// Welche Stelle nach der laufenden kommt.
///
/// Am Ende der Liste wird umgelaufen, wenn der Nutzer selbst weitertippt: Wer
/// in einer Playlist beim letzten Titel noch einmal drückt, will wieder von
/// vorn, nicht ins Leere. Von selbst hört die Wiedergabe dort auf — außer bei
/// „Alle wiederholen“, das dafür da ist.
///
/// Als eigene Funktion, damit sich die vier Fälle prüfen lassen, ohne einen
/// Player samt Tongerät zu bauen.
fn naechste_stelle(
    stelle: Option<usize>,
    laenge: usize,
    von_hand: bool,
    wiederholen: RepeatMode,
) -> Option<usize> {
    if laenge == 0 {
        return None;
    }
    match stelle {
        Some(pos) if pos + 1 < laenge => Some(pos + 1),
        Some(_) => (von_hand || wiederholen == RepeatMode::All).then_some(0),
        None => Some(0),
    }
}

/// Wohin der Rückwärtsschritt führt.
///
/// Am Anfang geht es ans Ende: Die Liste ist ein Ring, und man kann in beide
/// Richtungen beliebig weit darin gehen.
///
/// Hier stand vorher das Gegenteil, mit der Begründung, wer zurückgeht, suche
/// das eben Gehörte, und das liege nie ganz hinten. Das stimmt für den
/// einzelnen Schritt — nicht aber für eine Liste, die man als Ring versteht:
/// Dort ist der letzte Titel der Nachbar des ersten, in beide Richtungen.
///
/// Als freie Funktion, damit sich die Regel ohne Tonausgabe und Datenbank
/// prüfen lässt; im Player selbst ginge das nicht.
fn vorherige_stelle(order_pos: Option<usize>, laenge: usize) -> usize {
    match order_pos {
        Some(pos) if pos > 0 => pos - 1,
        // Vom ersten ans Ende. Bei leerer Liste bleibt nur die Null; der
        // Aufrufer prüft das ohnehin vorher.
        Some(_) => laenge.saturating_sub(1),
        None => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::{naechste_stelle, vorgeschichte_kuerzen, vorherige_stelle, RepeatMode};

    /// Von Hand am Ende: wieder der erste.
    ///
    /// Vorher hörte die Wiedergabe dort auf, und in einer Playlist stand man
    /// beim letzten Titel vor einem Knopf, der nichts mehr tat.
    #[test]
    fn von_hand_laeuft_am_ende_um() {
        assert_eq!(naechste_stelle(Some(2), 3, true, RepeatMode::Off), Some(0));
        assert_eq!(naechste_stelle(Some(2), 3, true, RepeatMode::All), Some(0));
    }

    /// Von selbst nicht: Sonst liefe jede Liste endlos weiter, und „Alle
    /// wiederholen“ hätte keinen Sinn mehr.
    #[test]
    fn von_selbst_endet_die_liste() {
        assert_eq!(naechste_stelle(Some(2), 3, false, RepeatMode::Off), None);
        assert_eq!(naechste_stelle(Some(2), 3, false, RepeatMode::All), Some(0));
    }

    #[test]
    fn mittendrin_geht_es_schlicht_weiter() {
        assert_eq!(naechste_stelle(Some(0), 3, true, RepeatMode::Off), Some(1));
        assert_eq!(naechste_stelle(Some(1), 3, false, RepeatMode::Off), Some(2));
        // Ohne laufende Stelle beginnt die Liste von vorn.
        assert_eq!(naechste_stelle(None, 3, false, RepeatMode::Off), Some(0));
    }

    #[test]
    fn eine_leere_liste_hat_kein_weiter() {
        assert_eq!(naechste_stelle(None, 0, true, RepeatMode::All), None);
        assert_eq!(naechste_stelle(Some(0), 0, true, RepeatMode::All), None);
    }

    #[test]
    fn zurueck_geht_eine_stelle_zurueck() {
        assert_eq!(vorherige_stelle(Some(3), 5), 2);
        assert_eq!(vorherige_stelle(Some(1), 5), 0);
    }

    /// Der eigentliche Punkt: Die Liste ist ein Ring.
    ///
    /// Vom ersten Titel führt der Rückwärtsschritt ans Ende, so wie der
    /// Vorwärtsschritt vom letzten an den Anfang. Hier stand vorher das
    /// Gegenteil.
    #[test]
    fn vom_ersten_geht_es_ans_ende() {
        assert_eq!(vorherige_stelle(Some(0), 5), 4);
        assert_eq!(vorherige_stelle(Some(0), 1), 0);
        // Ohne laufende Stelle gibt es nichts zu umlaufen.
        assert_eq!(vorherige_stelle(None, 5), 0);
        assert_eq!(vorherige_stelle(Some(0), 0), 0);
    }

    #[test]
    fn kurze_vorgeschichte_bleibt_unangetastet() {
        let queue: Vec<i64> = (10..20).collect();
        let order: Vec<usize> = (0..10).collect();
        assert!(vorgeschichte_kuerzen(&queue, &order, 3, 20).is_none());
        assert!(vorgeschichte_kuerzen(&queue, &order, 20, 20).is_none());
    }

    #[test]
    fn zu_lange_vorgeschichte_wird_vorne_gekappt() {
        let queue: Vec<i64> = (10..20).collect();
        let order: Vec<usize> = (0..10).collect();
        let (neue_queue, neue_order, stelle) =
            vorgeschichte_kuerzen(&queue, &order, 6, 2).expect("es gibt etwas zu kürzen");

        // Vier von zehn fallen weg, der laufende Titel bleibt derselbe.
        assert_eq!(neue_queue, vec![14, 15, 16, 17, 18, 19]);
        assert_eq!(neue_order, vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(stelle, 2);
        assert_eq!(neue_queue[neue_order[stelle]], queue[order[6]]);
    }

    /// Bei Zufallswiedergabe liegen die gehörten Stellen verstreut; die
    /// Umrechnung muss trotzdem auf denselben Titel zeigen.
    #[test]
    fn gemischte_reihenfolge_zeigt_weiter_auf_denselben_titel() {
        let queue: Vec<i64> = vec![100, 101, 102, 103, 104, 105];
        let order: Vec<usize> = vec![3, 0, 5, 1, 4, 2];
        let laufender = queue[order[4]];

        let (neue_queue, neue_order, stelle) =
            vorgeschichte_kuerzen(&queue, &order, 4, 1).expect("es gibt etwas zu kürzen");

        assert_eq!(neue_queue.len(), 3, "drei Gehörte fallen weg");
        assert_eq!(neue_order.len(), 3);
        // Die Stellen sind umgerechnet, nicht bloß abgeschnitten.
        assert_eq!(neue_order, vec![0, 2, 1]);
        assert_eq!(neue_queue[neue_order[stelle]], laufender);
        // Die verbliebene Reihenfolge zeigt weiterhin auf gültige Stellen.
        assert!(neue_order.iter().all(|stelle| *stelle < neue_queue.len()));
    }
}
