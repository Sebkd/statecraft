//! D1: self-emit. Handlers enqueue follow-up events via `self.emit`, processed
//! after the current transition, FIFO, within the same `apply` call.

use statecraft::{ApplyError, fsm};

// --- cascade drives itself forward in one apply, follow-up sees new state ---

#[fsm(initial = Idle)]
impl Casc {
    type Context = ();

    #[on(state = Idle, event = Go, next = Working)]
    async fn on_go(&mut self) {
        // Work is handled in Working, the state we transition INTO.
        self.emit(CascEvent::Work);
    }

    #[on(state = Working, event = Work, next = Done)]
    async fn on_work(&mut self) {}
}

#[tokio::test]
async fn test_cascade_reaches_done_in_one_apply() {
    let mut m = Casc::new(());
    m.apply(CascEvent::Go).await.unwrap();
    // Idle -> Working (on_go) -> Done (emitted Work handled in Working).
    assert_eq!(m.state(), CascState::Done);
}

// --- FIFO (breadth-first) ordering ---

#[derive(Debug, Default)]
struct Order {
    seen: Vec<char>,
}

#[fsm(initial = S)]
impl Fifo {
    type Context = Order;

    #[on(state = S, event = Start, next = S)]
    async fn on_start(&mut self) {
        self.emit(FifoEvent::A);
        self.emit(FifoEvent::B);
    }

    #[on(state = S, event = A, next = S)]
    async fn on_a(&mut self) {
        self.context.seen.push('A');
        self.emit(FifoEvent::C);
    }

    #[on(state = S, event = B, next = S)]
    async fn on_b(&mut self) {
        self.context.seen.push('B');
    }

    #[on(state = S, event = C, next = S)]
    async fn on_c(&mut self) {
        self.context.seen.push('C');
    }
}

#[tokio::test]
async fn test_fifo_breadth_first_order() {
    let mut m = Fifo::new(Order::default());
    m.apply(FifoEvent::Start).await.unwrap();
    // on_start emits [A, B]; A emits C. FIFO => A, B, C (not A, C, B).
    assert_eq!(m.context.seen, vec!['A', 'B', 'C']);
}

// --- self-emitted event with no handler here: skipped, cascade continues ---

#[derive(Debug, Default)]
struct Mark {
    n: usize,
}

#[fsm(initial = Idle)]
impl Skip {
    type Context = Mark;

    #[on(state = Idle, event = Go, next = Done)]
    async fn on_go(&mut self) {
        self.context.n = 42;
        // Other is only handled in Elsewhere, never in Done.
        self.emit(SkipEvent::Other);
    }

    #[on(state = Elsewhere, event = Other, next = Elsewhere)]
    async fn on_other(&mut self) {
        self.context.n = 999;
    }
}

#[tokio::test]
async fn test_unhandled_self_emit_is_skipped_not_errored() {
    let mut m = Skip::new(Mark::default());
    // Ok despite the emitted Other having no handler in Done.
    m.apply(SkipEvent::Go).await.unwrap();
    assert_eq!(m.state(), SkipState::Done);
    // on_other never ran, so n stays 42.
    assert_eq!(m.context.n, 42);
}

// --- runaway cascade hits the limit ---

#[fsm(initial = Loop)]
impl Ov {
    type Context = ();

    #[on(state = Loop, event = Tick, next = Loop)]
    async fn on_tick(&mut self) {
        self.emit(OvEvent::Tick); // re-triggers itself forever
    }
}

#[tokio::test]
async fn test_runaway_cascade_overflows() {
    let mut m = Ov::new(());
    let err = m.apply(OvEvent::Tick).await.unwrap_err();
    assert_eq!(err, ApplyError::CascadeOverflow);
}

// --- compile-time cascade-limit parsing ---

#[test]
fn test_cascade_limit_parsing() {
    assert_eq!(
        statecraft::cascade_limit(None),
        statecraft::DEFAULT_CASCADE_LIMIT
    );
    assert_eq!(statecraft::cascade_limit(Some("5")), 5);
    assert_eq!(statecraft::cascade_limit(Some("0")), 0); // 0 = unbounded
    assert_eq!(
        statecraft::cascade_limit(Some("nope")),
        statecraft::DEFAULT_CASCADE_LIMIT
    );
    assert_eq!(
        statecraft::cascade_limit(Some("")),
        statecraft::DEFAULT_CASCADE_LIMIT
    );
}
