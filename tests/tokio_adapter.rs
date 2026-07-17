//! D3 Tokio adapter: `spawn` / `Handle` / `watch`. Runs only with the `tokio`
//! feature.
#![cfg(feature = "tokio")]

use statecraft::fsm;

#[fsm(initial = Idle)]
impl Worker {
    type Context = ();

    #[on(state = Idle, event = Start, next = Running)]
    async fn on_start(&mut self) {}

    #[on(state = Running, event = Stop, next = Idle)]
    async fn on_stop(&mut self) {}
}

#[tokio::test]
async fn test_spawn_send_watch() {
    let (handle, _join) = Worker::spawn(());
    let mut states = handle.watch();

    handle.send(WorkerEvent::Start).await.unwrap();
    states.changed().await.unwrap();
    assert_eq!(*states.borrow(), WorkerState::Running);

    handle.send(WorkerEvent::Stop).await.unwrap();
    states.changed().await.unwrap();
    assert_eq!(*states.borrow(), WorkerState::Idle);
}

// --- self-emit cascade runs inside one apply, in spawned mode ---

#[fsm(initial = Idle)]
impl Casc {
    type Context = ();

    #[on(state = Idle, event = Go, next = Working)]
    async fn on_go(&mut self) {
        self.emit(CascEvent::Work);
    }

    #[on(state = Working, event = Work, next = Done)]
    async fn on_work(&mut self) {}
}

#[tokio::test]
async fn test_spawn_self_emit_cascade() {
    let (handle, _join) = Casc::spawn(());
    let mut states = handle.watch();

    handle.send(CascEvent::Go).await.unwrap();
    // One apply drives Idle -> Working -> Done; watch updates once, to Done.
    states.changed().await.unwrap();
    assert_eq!(*states.borrow(), CascState::Done);
}

// --- payload flows through the channel and drives a branch ---

#[derive(Debug)]
pub struct Amount(u32);

#[fsm(initial = Idle)]
impl Gate {
    type Context = ();

    #[on(state = Idle, event = Check(Amount), next = [Approved, Denied])]
    async fn on_check(&mut self, amount: Amount) -> IdleCheckNext {
        if amount.0 >= 100 {
            IdleCheckNext::Approved
        } else {
            IdleCheckNext::Denied
        }
    }
}

#[tokio::test]
async fn test_spawn_payload() {
    let (handle, _join) = Gate::spawn(());
    let mut states = handle.watch();

    handle.send(GateEvent::Check(Amount(150))).await.unwrap();
    states.changed().await.unwrap();
    assert_eq!(*states.borrow(), GateState::Approved);
}

// --- a failing handler is logged and the task keeps running ---

#[derive(Debug)]
pub struct Boom;

#[fsm(initial = Idle)]
impl Resil {
    type Context = ();
    type Error = Boom;

    #[on(state = Idle, event = Bad, next = Idle)]
    async fn on_bad(&mut self) -> Result<(), Boom> {
        Err(Boom)
    }

    #[on(state = Idle, event = Good, next = Done)]
    async fn on_good(&mut self) -> Result<(), Boom> {
        Ok(())
    }
}

#[tokio::test]
async fn test_task_survives_handler_error() {
    let (handle, _join) = Resil::spawn(());
    let mut states = handle.watch();

    handle.send(ResilEvent::Bad).await.unwrap(); // errors, logged, no state change
    handle.send(ResilEvent::Good).await.unwrap(); // still processed
    states.changed().await.unwrap();
    assert_eq!(*states.borrow(), ResilState::Done);
}

// --- lifecycle: dropping the last handle ends the task gracefully ---

#[tokio::test]
async fn test_drop_handle_ends_task() {
    let (handle, join) = Worker::spawn(());
    drop(handle);
    join.await.unwrap();
}

// --- graceful shutdown works even with a live clone ---

#[tokio::test]
async fn test_graceful_shutdown() {
    let (handle, join) = Worker::spawn(());
    let _clone = handle.clone(); // channel stays open; only shutdown() stops it
    handle.shutdown();
    join.await.unwrap();
}

// --- hard shutdown aborts the task ---

#[tokio::test]
async fn test_hard_shutdown_aborts() {
    let (handle, join) = Worker::spawn(());
    let _clone = handle.clone();
    handle.shutdown_now();
    let result = join.await;
    assert!(result.is_err() && result.unwrap_err().is_cancelled());
}
