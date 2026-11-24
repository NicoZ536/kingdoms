//! Saving in the background: the state is copied on the main thread (cheap), encoded and
//! written on a worker thread, so autosaves never stall the game. One save runs at a time; a
//! save requested meanwhile waits (a newer request replaces a waiting one of the same file).

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use kingdoms_sim::save::{SaveError, SaveHeader, encode};
use kingdoms_sim::state::GameState;

use super::slots::{MAX_SAVED_NOTIFICATIONS, SaveMeta, SavedNotification, SlotKind, write_meta, write_save_bytes};
use crate::session::Session;

/// Everything needed to write one save, owned so it can move to the worker.
pub struct SaveJob {
    pub path: PathBuf,
    pub kind: SlotKind,
    header: SaveHeader,
    state: GameState,
    meta: SaveMeta,
}

impl SaveJob {
    /// Captures the session now. `name` is stored in the header (shown in the save list).
    pub fn new(session: &Session, path: PathBuf, kind: SlotKind, name: &str, saved_at_unix: u64) -> Self {
        let state = session.state();
        Self {
            header: SaveHeader::for_state(state, session.defs(), name, saved_at_unix),
            meta: SaveMeta::of(state, saved_at_unix, session.play_seconds()),
            state: state.clone(),
            path,
            kind,
        }
    }

    /// Keeps the newest `notifications` (newest first) with the save.
    pub fn with_notifications(mut self, mut notifications: Vec<SavedNotification>) -> Self {
        notifications.truncate(MAX_SAVED_NOTIFICATIONS);
        self.meta.notifications = notifications;
        self
    }

    /// Simulation tick of the captured state.
    pub fn tick(&self) -> u64 {
        self.meta.tick
    }

    /// Encodes and writes the save and its metadata (on the calling thread).
    pub fn run(self) -> SaveOutcome {
        let started = Instant::now();
        let result = encode(&self.header, &self.state).and_then(|bytes| {
            write_save_bytes(&self.path, &bytes)?;
            log::info!("game saved to {} ({} bytes)", self.path.display(), bytes.len());
            Ok(())
        });
        if result.is_ok()
            && let Err(err) = write_meta(&self.path, &self.meta)
        {
            // The save itself is fine; the list then shows fewer details.
            log::warn!("cannot write the metadata of {}: {err}", self.path.display());
        }
        SaveOutcome { path: self.path, kind: self.kind, name: self.header.name, tick: self.meta.tick, result, elapsed: started.elapsed() }
    }
}

/// Result of a finished save.
#[derive(Debug)]
pub struct SaveOutcome {
    pub path: PathBuf,
    pub kind: SlotKind,
    pub name: String,
    /// Simulation tick that was saved.
    pub tick: u64,
    pub result: Result<(), SaveError>,
    pub elapsed: Duration,
}

/// Runs [`SaveJob`]s on a worker thread, one at a time.
#[derive(Default)]
pub struct SaveWriter {
    running: Option<mpsc::Receiver<SaveOutcome>>,
    waiting: Vec<SaveJob>,
    /// Saves that ran on the main thread because no worker could be started.
    done_inline: Vec<SaveOutcome>,
}

impl SaveWriter {
    /// Queues a save; it starts right away if no other save is running.
    pub fn submit(&mut self, job: SaveJob) {
        self.waiting.retain(|w| w.path != job.path);
        self.waiting.push(job);
        self.start_next();
    }

    /// True while a save is running or waiting.
    pub fn busy(&self) -> bool {
        self.running.is_some() || !self.waiting.is_empty()
    }

    /// Finished saves since the last call (never blocks).
    pub fn poll(&mut self) -> Vec<SaveOutcome> {
        let mut done = std::mem::take(&mut self.done_inline);
        while let Some(receiver) = &self.running {
            match receiver.try_recv() {
                Ok(outcome) => {
                    self.running = None;
                    done.push(outcome);
                    self.start_next();
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    log::error!("the save thread ended without a result");
                    self.running = None;
                    self.start_next();
                }
            }
        }
        done
    }

    /// Blocks until every queued save is written (before leaving the game).
    pub fn finish(&mut self) -> Vec<SaveOutcome> {
        let mut done = std::mem::take(&mut self.done_inline);
        while self.busy() {
            if let Some(receiver) = self.running.take() {
                match receiver.recv() {
                    Ok(outcome) => done.push(outcome),
                    Err(_) => log::error!("the save thread ended without a result"),
                }
            }
            self.start_next();
        }
        done
    }

    fn start_next(&mut self) {
        if self.running.is_some() || self.waiting.is_empty() {
            return;
        }
        let job = self.waiting.remove(0);
        let (job_sender, job_receiver) = mpsc::channel::<SaveJob>();
        let (sender, receiver) = mpsc::channel();
        let worker = std::thread::Builder::new().name("save-writer".into()).spawn(move || {
            if let Ok(job) = job_receiver.recv() {
                let _ = sender.send(job.run());
            }
        });
        match worker {
            Ok(_) => match job_sender.send(job) {
                Ok(()) => self.running = Some(receiver),
                Err(mpsc::SendError(job)) => self.done_inline.push(job.run()),
            },
            Err(err) => {
                log::error!("cannot start the save thread ({err}); saving on the main thread");
                self.done_inline.push(job.run());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use kingdoms_sim::defs::Defs;
    use kingdoms_sim::save::load_from_file;
    use kingdoms_sim::state::GameSettings;
    use kingdoms_sim::world::WorldSize;

    use super::super::slots::{list_saves, read_meta, save_path};
    use super::*;

    fn defs() -> Arc<Defs> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
        match Defs::load(&dir) {
            Ok(defs) => Arc::new(defs),
            Err(e) => panic!("shipped game data must load: {e}"),
        }
    }

    #[test]
    fn background_saves_write_save_and_metadata() {
        let defs = defs();
        let mut session = Session::new(defs.clone(), GameSettings::new(&defs, 11, WorldSize::Small));
        for _ in 0..20 {
            session.tick();
        }
        session.add_play_time(125.0);
        let dir = std::env::temp_dir().join(format!("kingdoms-writer-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut writer = SaveWriter::default();
        let path = save_path(&dir, "mein_reich");
        writer.submit(SaveJob::new(&session, path.clone(), SlotKind::Manual, "Mein Reich", 1_700_000_000));
        // A second request for the same file while the first runs replaces nothing that runs.
        session.tick();
        writer.submit(SaveJob::new(&session, path.clone(), SlotKind::Manual, "Mein Reich", 1_700_000_060));
        assert!(writer.busy());
        let done = writer.finish();
        assert!(!writer.busy());
        assert!(done.iter().all(|o| o.result.is_ok()), "{done:?}");
        let last = done.last().expect("outcome");
        assert_eq!(last.tick, session.state().clock().tick);
        let loaded = load_from_file(&path, &defs).expect("load");
        assert_eq!(loaded.state.clock(), session.state().clock());
        assert_eq!(loaded.header.name, "Mein Reich");
        let meta = read_meta(&path).expect("metadata");
        assert!(meta.matches(&loaded.header));
        assert_eq!(meta.play_seconds, 125);
        assert_eq!(meta.population as usize, session.state().population().len());
        let entries = list_saves(&dir, defs.fingerprint());
        assert_eq!(entries.len(), 1);
        assert!(entries[0].loadable() && entries[0].meta.is_some());
        assert_eq!(entries[0].display_name(), "Mein Reich");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
