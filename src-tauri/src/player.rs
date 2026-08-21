//! the playback engine. runs on a thread of its own, owns the audio sink and
//! a sqlite connection of its own (for paths and playback statistics).

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
/// this often the running playback position is stored
const REMEMBER_EVERY: Duration = Duration::from_secs(5);
/// from this listening time on a track counts as heard, as with the common
/// services.
const MIN_PLAY_MS: u64 = 30_000;
/// gain at the full swing of the slider
const OBERGRENZE: f32 = 0.85;

/// from this listening time on robify remembers a track as recently played.
///
/// far less than for the statistics, and deliberately so: "recently played"
/// answers the question of what just ran, not what is heard a lot. whoever
/// starts a track and moves on after twenty seconds wants to find it there.
/// five seconds keep out only what comes from tapping through a list.
const VERLAUF_MS: u64 = 5_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RepeatMode {
    /// after the last track it ends
    Off,
    /// repeat the queue endlessly
    All,
    /// repeat the current track endlessly
    One,
}

impl RepeatMode {
    /// short name for the settings table
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
    /// pause after a fixed duration
    Duration,
    /// pause at the end of the running track
    EndOfTrack,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SleepTimerState {
    pub mode: SleepTimerMode,
    pub total_ms: u64,
    pub remaining_ms: u64,
}

/// the complete state the frontend needs for rendering
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
    /// positions of the queue in the order they run in.
    ///
    /// without shuffle that is plainly `0, 1, 2, …`, with it the drawn
    /// sequence. the ui needs it to show what is still to come: under shuffle
    /// the coming tracks do not stand behind the running one but lie
    /// scattered over the whole list.
    pub order: Vec<usize>,
    /// where in `order` the running track stands
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

/// the running player, for commands from outside the app.
///
/// the system player on the lock screen does not come in through a tauri
/// command, it calls from the java runtime into the library directly, without
/// ui state and without an `AppHandle`. for it to reach anybody at all, the
/// handle is deposited here once.
static GRIFF: OnceLock<PlayerHandle> = OnceLock::new();

/// takes in what was pressed on the system player.
///
/// unknown names stay silent: the call comes from the java side, and a crash
/// on the audio thread could not be recovered from there.
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
    /// playback order as indices into `queue`, shuffled where drawn
    order: Vec<usize>,
    order_pos: Option<usize>,

    repeat: RepeatMode,
    shuffle: bool,
    volume: f32,
    muted: bool,

    /// whether a source is currently running in the sink
    loaded: bool,
    duration_ms: u64,
    /// milliseconds of the running track actually heard
    listened_ms: u64,
    last_tick: Instant,
    /// when the state was last stored
    last_remembered: Instant,
    /// what last went out as a tick. where nothing changes nothing is sent,
    /// otherwise a message would cross the bridge four times a second while
    /// idle and trigger a repaint in the frontend.
    letzter_takt: Option<(bool, u64, u64, Option<u64>)>,
    /// which track was last reported to the system.
    ///
    /// the cover travels along on a change only, otherwise it would cross the
    /// java bridge again at every press on pause.
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
    // --- commands ---

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
                // back within the first seconds, otherwise to the start
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

    /// sets the volume by ear rather than in a straight line.
    ///
    /// the slider is linear while hearing is logarithmic: half the gain does
    /// not sound half as loud, only slightly quieter. set to twenty percent it
    /// was therefore still clearly audible, and the lower half of the slider
    /// did almost nothing.
    ///
    /// the second power models that: at half swing a quarter of the gain is
    /// left, a good twelve decibels below full. the third was too much of a
    /// good thing, it made the lower half of the slider almost uselessly
    /// quiet.
    ///
    /// on top of it a cap: at the very top stand 85 percent instead of 100.
    /// turned up fully it was a touch too loud, and the headroom costs
    /// nothing where the system lays its own volume over it.
    ///
    /// zero stays zero, muted is muted.
    fn apply_volume(&self) {
        let stand = if self.muted { 0.0 } else { self.volume };
        self.player.set_volume((OBERGRENZE * stand * stand) as _);
    }

    // --- order ---

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

    /// rebuilds the order without interrupting the running track
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

    // --- playback ---

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

    /// jumps to a position inside the running track.
    ///
    /// the decoder can refuse to seek: with an mp3 lacking a seek table
    /// symphonia has to guess and gives up on some files. the track is then
    /// opened anew and started at the target position right away, a fresh
    /// decoder reads the file from the top and usually gets through where the
    /// started one fails.
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
        // the listening time counted so far would be lost on a reload
        let gehoert = self.listened_ms;
        self.load_at(track_id, ms, lief)
            .map_err(|e| anyhow!(fehler!("Spulen nicht möglich: {0}", e)))?;
        self.listened_ms = gehoert;

        // `load_at` swallows a failed jump so a track at least runs from the
        // top if need be. here that is the wrong answer: whoever seeks does
        // not want to end up at the start quietly
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

    /// loads a track, optionally paused and from a given position.
    ///
    /// this is how the last heard track comes back at the start of the app:
    /// visible in the bar, at the position of back then, but silent. whoever
    /// wants to keep listening presses play, whoever does not at least sees
    /// where they left off.
    fn load_at(&mut self, track_id: i64, start_ms: u64, play: bool) -> Result<()> {
        let (path, duration_ms): (String, i64) = self.conn.query_row(
            "SELECT path, duration_ms FROM tracks WHERE id = ?1",
            [track_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;

        let file = File::open(&path).map_err(|e| anyhow!(fehler!("Datei nicht lesbar ({0}): {1}", path, e)))?;
        let source = rodio::Decoder::try_from(BufReader::new(file)).map_err(|e| {
            // opus is the frequent case: symphonia brings no decoder for it.
            // new downloads avoid the format, older files still lie in the
            // library
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

        // seek first, then decide whether it starts
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

    /// brings the last heard track back, paused and at the position it was
    /// left at.
    ///
    /// where anything about it fails, a missing file or an unreadable format,
    /// the player stays empty. a failed restore must not hold up the
    /// start.
    fn restore(&mut self) {
        // the repeat mode holds regardless of whether a queue is remembered,
        // hence before the early return further down
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
            // the track is no longer playable, so be it
            self.queue.clear();
            self.order.clear();
            self.order_pos = None;
            self.stop_playback();
        }
    }

    /// remembers what is running: queue, position within it and playback
    /// position. at the next start the track stands there again.
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

    /// next track. `manual` tells a user press from the end of a track.
    fn advance(&mut self, manual: bool) -> Result<()> {
        self.record_play();

        // repeat-one holds for a press as well.
        //
        // before, a press jumped on anyway, on the reasoning that the user
        // wants to change track right now. in the hand it reads differently:
        // whoever puts a track on endless repeat wants to hear it again when
        // tapping on, not have the setting bypassed
        if self.repeat == RepeatMode::One {
            return self.start_current();
        }
        if self.queue.is_empty() {
            self.stop_playback();
            return Ok(());
        }

        let am_ende = matches!(self.order_pos, Some(pos) if pos + 1 >= self.order.len());
        let next = naechste_stelle(self.order_pos, self.order.len(), manual, self.repeat);

        // a new shuffle round on wrap-around, otherwise the order repeats
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

    /// drops what lies too far back.
    ///
    /// heard tracks stay in the queue, otherwise the step backwards would
    /// lead nowhere. it is not to grow endlessly over that though: on every
    /// change the ui fetches the details of every entry from the database,
    /// including the ones nobody sees any more. after an evening of music
    /// that would be hundreds of queries per track change.
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

    // --- statistics ---

    /// writes the listening time of the running track into the database
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

        // the history first: it has its own, far lower threshold and must
        // not depend on whether the track counts for the statistics
        if listened >= VERLAUF_MS {
            let _ = self.conn.execute(
                "UPDATE tracks SET last_played_at = ?2 WHERE id = ?1",
                params![track_id, db::now()],
            );
        }

        let completed = self.duration_ms > 0 && listened * 100 >= self.duration_ms * 90;
        if listened < MIN_PLAY_MS && !completed {
            // too short for the evaluation. the ui learns of it anyway,
            // otherwise the home page would stand still until something ran
            // long enough
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

    // --- tick ---

    fn tick(&mut self) {
        let elapsed = self.last_tick.elapsed();
        self.last_tick = Instant::now();
        let elapsed_ms = elapsed.as_millis() as u64;
        let is_playing = self.loaded && !self.player.is_paused();

        if is_playing {
            self.listened_ms += elapsed_ms;
            // the position moves constantly, every few seconds is enough
            if self.last_remembered.elapsed() >= REMEMBER_EVERY {
                self.remember();
            }
        }

        // track finished: the sink is empty although nothing was paused
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

    /// sends the state out. `full` emits the complete snapshot, for events
    /// that change more than the position: track change, queue, volume.
    /// otherwise only the cheap position tick goes out.
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

    /// tells the system what is running so it can show its own player.
    ///
    /// on the full state only, not on every tick: besides the position the
    /// report carries the speed, and android carries the time on from that
    /// itself. four reports a second would be the same display at four times
    /// the price, each of them a jump into the java runtime.
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

        // fetch the cover on a track change only: it lies in the database as
        // a blob, and pushing a few hundred kilobytes through the java bridge
        // at every press on pause would be waste.
        //
        // `None` means unchanged over there, not none. a track without a
        // cover therefore sends an empty array, otherwise the image of the
        // previous one would stay. the other way round it disappeared at the
        // first pause early on, because both looked the same there
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

    /// only the four values that change constantly during playback.
    ///
    /// where everything stays the same, paused, stopped, nothing loaded,
    /// nothing goes out. before, the tick ran while idle too: four messages a
    /// second including a copy of the whole queue, plus a repaint in the
    /// frontend although nothing moved.
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

        // the cached state hangs on the position and has to travel along.
        // the queue is copied once in doing so, but only where something has
        // actually moved
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

/// this many heard tracks stay reachable behind the running one.
///
/// twenty rather than a span of time: going back is a step through tracks,
/// not through minutes. how far one gets should be countable and not depend
/// on how long the pieces were or how long the app stood open.
const MAX_VORGESCHICHTE: usize = 20;

/// trims the history and returns queue, order and position.
///
/// `None` where there is nothing to do. what is removed is exactly the first
/// entries of the playback order, and which queue positions those are stands
/// only in there. under shuffle they lie scattered, hence the conversion:
/// every remaining position moves up by as many places as fell away before
/// it.
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

    // mapping old position to new position, in one pass with the sieving
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

/// which position comes after the running one.
///
/// at the end of the list it wraps around where the user taps on themselves:
/// whoever presses once more on the last track of a playlist wants to start
/// over, not to land nowhere. by itself playback stops there, except under
/// repeat-all, which exists for exactly that.
///
/// a function of its own so the four cases can be checked without building a
/// player and an audio device.
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

/// where the step backwards leads.
///
/// at the start it goes to the end: the list is a ring, and one can walk it
/// as far as one likes in both directions.
///
/// the opposite stood here before, on the grounds that whoever goes back is
/// looking for what was just heard, and that never lies right at the end.
/// that holds for the single step but not for a list understood as a ring:
/// there the last track is the neighbour of the first, in both directions.
///
/// a free function so the rule can be checked without audio output and
/// database, which would not work inside the player itself.
fn vorherige_stelle(order_pos: Option<usize>, laenge: usize) -> usize {
    match order_pos {
        Some(pos) if pos > 0 => pos - 1,
        // from the first to the end. with an empty list only zero is left,
        // and the caller checks that beforehand anyway
        Some(_) => laenge.saturating_sub(1),
        None => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::{naechste_stelle, vorgeschichte_kuerzen, vorherige_stelle, RepeatMode};

    /// by hand at the end: the first one again.
    ///
    /// playback used to stop there, and on the last track of a playlist one
    /// stood before a button that did nothing any more.
    #[test]
    fn von_hand_laeuft_am_ende_um() {
        assert_eq!(naechste_stelle(Some(2), 3, true, RepeatMode::Off), Some(0));
        assert_eq!(naechste_stelle(Some(2), 3, true, RepeatMode::All), Some(0));
    }

    /// not by itself: every list would run on endlessly then, and repeat-all
    /// would carry no meaning any more.
    #[test]
    fn von_selbst_endet_die_liste() {
        assert_eq!(naechste_stelle(Some(2), 3, false, RepeatMode::Off), None);
        assert_eq!(naechste_stelle(Some(2), 3, false, RepeatMode::All), Some(0));
    }

    #[test]
    fn mittendrin_geht_es_schlicht_weiter() {
        assert_eq!(naechste_stelle(Some(0), 3, true, RepeatMode::Off), Some(1));
        assert_eq!(naechste_stelle(Some(1), 3, false, RepeatMode::Off), Some(2));
        // without a running position the list starts from the top
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

    /// the actual point: the list is a ring.
    ///
    /// from the first track the step backwards leads to the end, just as the
    /// step forward leads from the last to the start. the opposite stood here
    /// before.
    #[test]
    fn vom_ersten_geht_es_ans_ende() {
        assert_eq!(vorherige_stelle(Some(0), 5), 4);
        assert_eq!(vorherige_stelle(Some(0), 1), 0);
        // without a running position there is nothing to wrap around
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

        // four out of ten fall away, the running track stays the same
        assert_eq!(neue_queue, vec![14, 15, 16, 17, 18, 19]);
        assert_eq!(neue_order, vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(stelle, 2);
        assert_eq!(neue_queue[neue_order[stelle]], queue[order[6]]);
    }

    /// under shuffle the heard positions lie scattered, and the conversion
    /// still has to point at the same track.
    #[test]
    fn gemischte_reihenfolge_zeigt_weiter_auf_denselben_titel() {
        let queue: Vec<i64> = vec![100, 101, 102, 103, 104, 105];
        let order: Vec<usize> = vec![3, 0, 5, 1, 4, 2];
        let laufender = queue[order[4]];

        let (neue_queue, neue_order, stelle) =
            vorgeschichte_kuerzen(&queue, &order, 4, 1).expect("es gibt etwas zu kürzen");

        assert_eq!(neue_queue.len(), 3, "drei Gehörte fallen weg");
        assert_eq!(neue_order.len(), 3);
        // the positions are converted, not merely cut off
        assert_eq!(neue_order, vec![0, 2, 1]);
        assert_eq!(neue_queue[neue_order[stelle]], laufender);
        // the remaining order still points at valid positions
        assert!(neue_order.iter().all(|stelle| *stelle < neue_queue.len()));
    }
}
