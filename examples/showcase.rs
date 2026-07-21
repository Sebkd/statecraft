//! Owned-mode showcase: `self.emit` (internal emitter), several branches from
//! one `(state, event)`, and `self.emit_replace`.
//!
//! Run: `cargo run --example showcase`

use statecraft::fsm;

#[derive(Debug, Default)]
struct Ctx {
    urgent: bool,
    tries: u32,
    log: Vec<&'static str>,
}

#[fsm(initial = Idle)]
impl Downloader {
    type Context = Ctx;

    #[on(state = Idle, event = Begin, next = Fetching)]
    async fn on_begin(&mut self) {
        self.context.log.push("begin");
        // Queue a warmup then a fetch...
        self.emit(DownloaderEvent::Warmup);
        self.emit(DownloaderEvent::Fetch);
        // ...but if urgent, the queued steps are obsolete: keep only Fetch.
        if self.context.urgent {
            self.emit_replace(DownloaderEvent::Fetch);
        }
    }

    #[on(state = Fetching, event = Warmup, next = Fetching)]
    async fn on_warmup(&mut self) {
        self.context.log.push("warmup");
    }

    // One `(Fetching, Fetch)` fans out to three outcomes, chosen at runtime but
    // checked at compile time (the handler returns `FetchingFetchNext`).
    #[on(state = Fetching, event = Fetch, next = [Fetching, Done, Failed])]
    async fn on_fetch(&mut self) -> FetchingFetchNext {
        self.context.tries += 1;
        self.context.log.push("fetch");
        match self.context.tries {
            1 | 2 => {
                self.emit(DownloaderEvent::Fetch); // retry via self-emit
                FetchingFetchNext::Fetching
            }
            3 => FetchingFetchNext::Done,
            _ => FetchingFetchNext::Failed,
        }
    }
}

#[tokio::main]
async fn main() {
    // Normal: begin -> warmup -> fetch x3 -> Done. One `apply` runs the whole
    // self-emit cascade to completion.
    let mut normal = Downloader::new(Ctx::default());
    normal.apply(DownloaderEvent::Begin).await.unwrap();
    println!(
        "normal: state={:?} log={:?}",
        normal.state(),
        normal.context.log
    );
    assert_eq!(normal.state(), DownloaderState::Done);

    // Urgent: `emit_replace` drops the queued Warmup, so the log has no warmup.
    let mut urgent = Downloader::new(Ctx {
        urgent: true,
        ..Ctx::default()
    });
    urgent.apply(DownloaderEvent::Begin).await.unwrap();
    println!(
        "urgent: state={:?} log={:?}",
        urgent.state(),
        urgent.context.log
    );
    assert_eq!(urgent.state(), DownloaderState::Done);
    assert!(!urgent.context.log.contains(&"warmup"));
}
