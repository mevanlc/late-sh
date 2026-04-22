use std::{
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

use anyhow::Context;
use dartboard_core::Canvas;
use dartboard_local::{CanvasStore, ColorSelectionMode, ServerHandle};
use late_core::{MutexRecover, db::Db, models::artboard::Snapshot};

pub const CANVAS_WIDTH: usize = 384;
pub const CANVAS_HEIGHT: usize = 192;
const DEFAULT_PERSIST_INTERVAL: Duration = Duration::from_secs(5 * 60);

#[derive(Default)]
struct LateShCanvasStore;

impl CanvasStore for LateShCanvasStore {
    fn load(&self) -> Option<Canvas> {
        Some(blank_canvas())
    }

    fn save(&mut self, _canvas: &Canvas) {}
}

#[derive(Default)]
struct PersistState {
    latest_canvas: Option<Canvas>,
    dirty: bool,
}

struct PostgresCanvasStore {
    initial_canvas: Canvas,
    persist_state: Arc<Mutex<PersistState>>,
    persist_notify_tx: mpsc::Sender<()>,
}

impl PostgresCanvasStore {
    fn new(db: Db, initial_canvas: Option<Canvas>, persist_interval: Duration) -> Self {
        let initial_canvas = initial_canvas.unwrap_or_else(blank_canvas);
        let persist_state = Arc::new(Mutex::new(PersistState::default()));
        let (persist_notify_tx, persist_notify_rx) = mpsc::channel();

        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => {
                let thread_state = persist_state.clone();
                thread::Builder::new()
                    .name("dartboard-persist".to_string())
                    .spawn(move || {
                        run_persist_loop(
                            db,
                            thread_state,
                            persist_notify_rx,
                            runtime,
                            persist_interval,
                        )
                    })
                    .expect("failed to spawn dartboard persist loop");
            }
            Err(error) => {
                tracing::warn!(
                    error = ?error,
                    "dartboard persistence disabled: no tokio runtime available"
                );
            }
        }

        Self {
            initial_canvas,
            persist_state,
            persist_notify_tx,
        }
    }
}

impl CanvasStore for PostgresCanvasStore {
    fn load(&self) -> Option<Canvas> {
        Some(self.initial_canvas.clone())
    }

    fn save(&mut self, canvas: &Canvas) {
        let mut state = self.persist_state.lock_recover();
        state.latest_canvas = Some(canvas.clone());
        if state.dirty {
            return;
        }
        state.dirty = true;
        drop(state);
        let _ = self.persist_notify_tx.send(());
    }
}

pub async fn load_persisted_canvas(db: &Db) -> anyhow::Result<Option<Canvas>> {
    let client = db.get().await.context("failed to get db client")?;
    let Some(snapshot) = Snapshot::find_by_board_key(&client, Snapshot::MAIN_BOARD_KEY)
        .await
        .context("failed to load artboard snapshot row")?
    else {
        return Ok(None);
    };
    let canvas =
        serde_json::from_value(snapshot.canvas).context("failed to decode artboard snapshot")?;
    Ok(Some(canvas))
}

pub async fn flush_server_snapshot(db: &Db, server: &ServerHandle) -> anyhow::Result<()> {
    let canvas = server.canvas_snapshot();
    save_canvas_snapshot(db, &canvas).await
}

pub fn spawn_server() -> ServerHandle {
    ServerHandle::spawn_local_with_color_selection_mode(
        LateShCanvasStore,
        ColorSelectionMode::RandomUnique,
    )
}

pub fn spawn_persistent_server(db: Db, initial_canvas: Option<Canvas>) -> ServerHandle {
    spawn_persistent_server_with_interval(db, initial_canvas, DEFAULT_PERSIST_INTERVAL)
}

pub fn spawn_persistent_server_with_interval(
    db: Db,
    initial_canvas: Option<Canvas>,
    persist_interval: Duration,
) -> ServerHandle {
    ServerHandle::spawn_local_with_color_selection_mode(
        PostgresCanvasStore::new(db, initial_canvas, persist_interval),
        ColorSelectionMode::RandomUnique,
    )
}

fn blank_canvas() -> Canvas {
    Canvas::with_size(CANVAS_WIDTH, CANVAS_HEIGHT)
}

fn run_persist_loop(
    db: Db,
    persist_state: Arc<Mutex<PersistState>>,
    persist_notify_rx: mpsc::Receiver<()>,
    runtime: tokio::runtime::Handle,
    persist_interval: Duration,
) {
    loop {
        match persist_notify_rx.recv() {
            Ok(()) => {}
            Err(_) => {
                flush_dirty_canvas(&db, &persist_state, &runtime);
                return;
            }
        }

        loop {
            let deadline = Instant::now() + persist_interval;
            loop {
                let now = Instant::now();
                if now >= deadline {
                    break;
                }
                let timeout = deadline.saturating_duration_since(now);
                match persist_notify_rx.recv_timeout(timeout) {
                    Ok(()) => {}
                    Err(mpsc::RecvTimeoutError::Timeout) => break,
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        flush_dirty_canvas(&db, &persist_state, &runtime);
                        return;
                    }
                }
            }

            if !flush_dirty_canvas(&db, &persist_state, &runtime) {
                break;
            }
        }
    }
}

fn flush_dirty_canvas(
    db: &Db,
    persist_state: &Arc<Mutex<PersistState>>,
    runtime: &tokio::runtime::Handle,
) -> bool {
    let canvas = {
        let mut state = persist_state.lock_recover();
        if !state.dirty {
            return false;
        }
        state.dirty = false;
        state.latest_canvas.clone()
    };

    let Some(canvas) = canvas else {
        return false;
    };

    if let Err(error) = persist_canvas(runtime, db, &canvas) {
        tracing::error!(error = ?error, "failed to persist artboard snapshot");
        let mut state = persist_state.lock_recover();
        state.latest_canvas = Some(canvas);
        state.dirty = true;
        return true;
    }

    tracing::debug!("persisted artboard snapshot");
    persist_state.lock_recover().dirty
}

fn persist_canvas(
    runtime: &tokio::runtime::Handle,
    db: &Db,
    canvas: &Canvas,
) -> anyhow::Result<()> {
    runtime.block_on(save_canvas_snapshot(db, canvas))
}

async fn save_canvas_snapshot(db: &Db, canvas: &Canvas) -> anyhow::Result<()> {
    let canvas = serde_json::to_value(canvas).context("failed to serialize artboard canvas")?;
    let client = db
        .get()
        .await
        .context("failed to get db client for artboard save")?;
    Snapshot::upsert(&client, Snapshot::MAIN_BOARD_KEY, canvas)
        .await
        .context("failed to upsert artboard snapshot")?;
    Ok(())
}
