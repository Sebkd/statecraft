//! What boxing a transition costs and what it buys, measured at runtime.
//!
//! Run it:
//!
//! ```text
//! cargo run --release --features diagnostics --example stack_frame
//! cargo run --release --features diagnostics,boxed-all --example stack_frame
//! ```
//!
//! The second run shows the wholesale policy: every transition boxed, marks or
//! no marks.

use statecraft_fsm::fsm;
use std::time::Instant;

/// Two of these live across suspends in the heavy handlers below — the shape of
/// a handler that awaits a database, then an HTTP call, holding both results.
const HEAVY: usize = 512 * 1024;

/// A real suspend point: always-ready futures let the optimiser collapse the
/// coroutine and quietly delete the locals being measured.
struct YieldOnce(bool);

impl std::future::Future for YieldOnce {
    type Output = u8;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<u8> {
        if self.0 {
            std::task::Poll::Ready(7)
        } else {
            self.0 = true;
            cx.waker().wake_by_ref();
            std::task::Poll::Pending
        }
    }
}

async fn tick() -> u8 {
    YieldOnce(false).await
}

/// Heavy transition, left inlined into dispatch.
#[fsm(initial = Idle)]
impl Inlined {
    type Context = usize;

    #[on(state = Idle, event = Go, next = Idle)]
    async fn on_go(&mut self) {
        let mut a = [0u8; HEAVY];
        a[0] = tick().await;
        let mut b = [0u8; HEAVY];
        b[0] = tick().await;
        self.context += std::hint::black_box(&a)[0] as usize + std::hint::black_box(&b)[0] as usize;
    }

    /// A hot, trivial transition sharing the machine with the heavy one.
    #[on(state = Idle, event = Tick, next = Idle)]
    async fn on_tick(&mut self) {
        self.context += 1;
    }
}

/// The same machine with the heavy transition marked.
#[fsm(initial = Idle)]
impl Marked {
    type Context = usize;

    #[on(state = Idle, event = Go, next = Idle, boxed)]
    async fn on_go(&mut self) {
        let mut a = [0u8; HEAVY];
        a[0] = tick().await;
        let mut b = [0u8; HEAVY];
        b[0] = tick().await;
        self.context += std::hint::black_box(&a)[0] as usize + std::hint::black_box(&b)[0] as usize;
    }

    #[on(state = Idle, event = Tick, next = Idle)]
    async fn on_tick(&mut self) {
        self.context += 1;
    }
}

/// Cost of one trivial transition, in nanoseconds. A macro rather than a
/// function: a closure returning an `async` block cannot borrow the machine
/// across calls.
macro_rules! time_trivial {
    ($fsm:expr, $event:expr) => {{
        for _ in 0..100_000 {
            $fsm.apply($event).await.unwrap();
        }
        let n = 2_000_000u32;
        let start = Instant::now();
        for _ in 0..n {
            $fsm.apply($event).await.unwrap();
        }
        start.elapsed().as_nanos() as f64 / f64::from(n)
    }};
}

#[tokio::main]
async fn main() {
    let policy = if cfg!(feature = "boxed-all") {
        "boxed-all (every transition boxed)"
    } else {
        "default (only #[on(.., boxed)] transitions)"
    };
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    println!("policy : {policy}");
    println!("profile: {profile}");
    println!("handler: two {} KiB locals across suspends\n", HEAVY / 1024);

    let mut inlined = Inlined::new(0);
    let mut marked = Marked::new(0);
    let inlined_size = inlined.apply_future_size(InlinedEvent::Tick);
    let marked_size = marked.apply_future_size(MarkedEvent::Tick);

    println!("size of the apply future");
    println!("  heavy transition unmarked : {inlined_size:>9} B");
    println!("  heavy transition marked   : {marked_size:>9} B");

    let inlined_ns = time_trivial!(inlined, InlinedEvent::Tick);
    let marked_ns = time_trivial!(marked, MarkedEvent::Tick);

    println!("\ncost of the *trivial* transition sharing the machine");
    println!("  heavy transition unmarked : {inlined_ns:>9.1} ns");
    println!("  heavy transition marked   : {marked_ns:>9.1} ns");

    println!(
        "\nThe machine's stack cost is set by its largest inlined handler, so marking\n\
         the heavy transition bounds the whole machine. Under the default policy the\n\
         trivial transition stays inlined and keeps its speed; under boxed-all it is\n\
         boxed too, and pays one allocation per transition."
    );
}
