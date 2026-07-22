//! Axum example: a *registry* of spawned FSMs, one per id, driven over HTTP.
//!
//! Each FSM runs on its own background Tokio task. The registry keeps **both**
//! the FSM's cloneable `Handle` (to send events / read state) **and** its
//! `JoinHandle`, so the task's lifecycle is controlled explicitly. Stopping is
//! graceful (`shutdown` + await) with a timeout fallback to a hard
//! `shutdown_now` so a stuck handler can never hang shutdown. `DELETE` stops one
//! FSM; Ctrl-C drains them all.
//!
//! Run: `cargo run` (from this directory), then e.g.:
//!   curl -XPOST localhost:3000/fsm/a/start
//!   curl -XPOST localhost:3000/fsm/a/tick   # repeat: Running -> ... -> Done
//!   curl        localhost:3000/fsm/a         # state
//!   curl        localhost:3000/fsm           # list all
//!   curl -XPOST localhost:3000/fsm/a/pause
//!   curl -XDELETE localhost:3000/fsm/a       # graceful stop, awaited

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use statecraft_fsm::fsm;
use tokio::task::JoinHandle;

#[derive(Debug, Default)]
struct Task {
    progress: u32,
}

#[fsm(initial = Idle)]
impl Worker {
    type Context = Task;

    #[on(state = Idle, event = Start, next = Running)]
    async fn on_start(&mut self) {}

    // Several outgoing transitions from `Running`, one of them fanning out to
    // three compile-time-checked targets.
    #[on(state = Running, event = Tick, next = [Running, Done, Failed])]
    async fn on_tick(&mut self) -> RunningTickNext {
        self.context.progress += 25;
        if self.context.progress >= 100 {
            RunningTickNext::Done
        } else {
            RunningTickNext::Running
        }
    }

    #[on(state = Running, event = Pause, next = Paused)]
    async fn on_pause(&mut self) {}

    #[on(state = Running, event = Cancel, next = Failed)]
    async fn on_cancel(&mut self) {}

    // From `Paused`, `Resume` fans out too: resume work, or finish if it was
    // already complete.
    #[on(state = Paused, event = Resume, next = [Running, Done])]
    async fn on_resume(&mut self) -> PausedResumeNext {
        if self.context.progress >= 100 {
            PausedResumeNext::Done
        } else {
            PausedResumeNext::Running
        }
    }

    #[on(state = Paused, event = Cancel, next = Failed)]
    async fn on_cancel_paused(&mut self) {}
}

/// A running FSM: its `Handle` (send/watch) and its `JoinHandle` (lifecycle).
struct Running {
    handle: WorkerHandle,
    join: JoinHandle<()>,
}

/// Registry of running FSMs, keyed by id.
type Registry = Arc<Mutex<HashMap<String, Running>>>;

#[derive(Clone)]
struct AppState {
    fsms: Registry,
}

#[tokio::main]
async fn main() {
    let registry: Registry = Arc::new(Mutex::new(HashMap::new()));

    let app = Router::new()
        .route("/fsm", get(list))
        .route("/fsm/{id}/{event}", post(send_event))
        .route("/fsm/{id}", get(get_state).delete(remove))
        .with_state(AppState {
            fsms: registry.clone(),
        });

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    println!("listening on http://127.0.0.1:3000  (Ctrl-C to stop)");
    println!("  POST   /fsm/{{id}}/{{event}}   event: start|tick|pause|resume|cancel");
    println!("  GET    /fsm/{{id}}             state of one FSM");
    println!("  GET    /fsm                   list all");
    println!("  DELETE /fsm/{{id}}             graceful stop (awaited)");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown(registry))
        .await
        .unwrap();
}

async fn send_event(
    State(app): State<AppState>,
    Path((id, event)): Path<(String, String)>,
) -> Result<String, (StatusCode, String)> {
    let ev = match event.as_str() {
        "start" => WorkerEvent::Start,
        "tick" => WorkerEvent::Tick,
        "pause" => WorkerEvent::Pause,
        "resume" => WorkerEvent::Resume,
        "cancel" => WorkerEvent::Cancel,
        other => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("unknown event `{other}`\n"),
            ));
        }
    };

    // Spawn the FSM for this id on first use, keeping both its Handle and its
    // JoinHandle so we control the task explicitly (see `remove`/`shutdown`).
    let handle = {
        let mut map = app.fsms.lock().unwrap();
        let entry = map.entry(id.clone()).or_insert_with(|| {
            let (handle, join) = Worker::spawn(Task::default());
            Running { handle, join }
        });
        entry.handle.clone()
    }; // lock released before the await below

    let _ = handle.send(ev).await; // fire-and-forget
    Ok(format!("{id}: sent {event}\n"))
}

async fn get_state(
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> Result<String, StatusCode> {
    let handle = app.fsms.lock().unwrap().get(&id).map(|r| r.handle.clone());
    match handle {
        Some(h) => Ok(format!("{id}: {:?}\n", *h.watch().borrow())),
        None => Err(StatusCode::NOT_FOUND),
    }
}

async fn list(State(app): State<AppState>) -> String {
    let handles: Vec<(String, WorkerHandle)> = {
        let map = app.fsms.lock().unwrap();
        map.iter()
            .map(|(k, r)| (k.clone(), r.handle.clone()))
            .collect()
    };
    if handles.is_empty() {
        return "(no fsms)\n".to_string();
    }
    let mut out = String::new();
    for (id, h) in handles {
        out.push_str(&format!("{id}: {:?}\n", *h.watch().borrow()));
    }
    out
}

async fn remove(State(app): State<AppState>, Path(id): Path<String>) -> StatusCode {
    let fsm = app.fsms.lock().unwrap().remove(&id);
    match fsm {
        Some(fsm) => {
            stop(fsm).await;
            StatusCode::NO_CONTENT
        }
        None => StatusCode::NOT_FOUND,
    }
}

/// On Ctrl-C, stop every FSM and wait for each task to finish.
async fn shutdown(registry: Registry) {
    let _ = tokio::signal::ctrl_c().await;
    let fsms: Vec<Running> = registry.lock().unwrap().drain().map(|(_, r)| r).collect();
    println!("\nshutting down; draining {} FSM(s)", fsms.len());
    for fsm in fsms {
        stop(fsm).await;
    }
}

/// Stop a running FSM in a controlled way: ask it to drain gracefully, but do
/// not wait forever — if it has not finished within the grace period, abort it
/// hard with `shutdown_now` so we never hang on a stuck handler.
async fn stop(fsm: Running) {
    fsm.handle.shutdown(); // graceful: drain queued events, then stop
    if tokio::time::timeout(Duration::from_secs(5), fsm.join)
        .await
        .is_err()
    {
        // Still running after the grace period: hard-abort the task.
        fsm.handle.shutdown_now();
    }
}
