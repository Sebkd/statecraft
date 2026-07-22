//! D1 observability: a self-emitted event with no handler in the current state
//! is logged unconditionally at WARN with structured `state`/`event` fields.

use std::io;
use std::sync::{Arc, Mutex};

use statecraft_fsm::fsm;
use tracing_subscriber::fmt::MakeWriter;

#[derive(Clone, Default)]
struct CaptureWriter(Arc<Mutex<Vec<u8>>>);

impl io::Write for CaptureWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for CaptureWriter {
    type Writer = CaptureWriter;
    fn make_writer(&'a self) -> CaptureWriter {
        self.clone()
    }
}

#[fsm(initial = Idle)]
impl Skip {
    type Context = ();

    #[on(state = Idle, event = Go, next = Done)]
    async fn on_go(&mut self) {
        // Other is only handled in Elsewhere, never in Done.
        self.emit(SkipEvent::Other);
    }

    #[on(state = Elsewhere, event = Other, next = Elsewhere)]
    async fn on_other(&mut self) {}
}

#[tokio::test]
async fn test_unhandled_self_emit_logs_warn_with_fields() {
    let buf = CaptureWriter::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(buf.clone())
        .with_max_level(tracing::Level::WARN)
        .without_time()
        .with_ansi(false)
        .finish();
    let guard = tracing::subscriber::set_default(subscriber);

    let mut m = Skip::new(());
    m.apply(SkipEvent::Go).await.unwrap();

    drop(guard);
    let out = String::from_utf8(buf.0.lock().unwrap().clone()).unwrap();

    assert!(
        out.contains("self-emitted event has no handler"),
        "expected warning message, got: {out:?}"
    );
    assert!(
        out.contains("event=Other"),
        "expected event field, got: {out:?}"
    );
    assert!(
        out.contains("state=Done"),
        "expected state field, got: {out:?}"
    );
}
