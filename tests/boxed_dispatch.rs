//! Heap placement of handler futures: the `boxed` mark on `#[on]` and the
//! wholesale `boxed-all` feature.
//!
//! What is being pinned down: the future returned by `apply` is sized by the
//! largest handler inlined into dispatch, so a single heavy transition sets the
//! stack cost of the whole machine. Boxing takes that handler out of the
//! dispatch coroutine and leaves a pointer behind.
//!
//! Thresholds are absolute byte counts on purpose. "Smaller than without the
//! mark" would pass while both numbers are equally bad.

#![cfg(feature = "diagnostics")]

use statecraft_fsm::fsm;

/// Bounded dispatch: a pointer plus the dispatch bookkeeping, nothing more.
const BOUNDED: usize = 256;
/// One heavy local. Two of them live across awaits in every heavy handler.
const HEAVY: usize = 512 * 1024;

/// A suspend point. Always ready, so the tests need no runtime to poll it, but
/// the coroutine layout still has to keep locals alive across it.
async fn tick() -> u8 {
    std::future::ready(7).await
}

// --- a machine whose one transition is heavy and marked --------------------

#[fsm(initial = Idle)]
impl Marked {
    type Context = usize;

    #[on(state = Idle, event = Go, next = Done, boxed)]
    async fn on_go(&mut self) {
        let mut a = [0u8; HEAVY];
        a[0] = tick().await;
        let mut b = [0u8; HEAVY];
        b[0] = tick().await;
        self.context += a[0] as usize + b[0] as usize;
    }
}

// --- the same machine, unmarked -------------------------------------------

#[fsm(initial = Idle)]
impl Unmarked {
    type Context = usize;

    #[on(state = Idle, event = Go, next = Done)]
    async fn on_go(&mut self) {
        let mut a = [0u8; HEAVY];
        a[0] = tick().await;
        let mut b = [0u8; HEAVY];
        b[0] = tick().await;
        self.context += a[0] as usize + b[0] as usize;
    }
}

// --- heavy marked transition next to a hot trivial one ---------------------

#[fsm(initial = Idle)]
impl Mixed {
    type Context = usize;

    #[on(state = Idle, event = Heavy, next = Idle, boxed)]
    async fn on_heavy(&mut self) {
        let mut a = [0u8; HEAVY];
        a[0] = tick().await;
        let mut b = [0u8; HEAVY];
        b[0] = tick().await;
        self.context += a[0] as usize + b[0] as usize;
    }

    #[on(state = Idle, event = Tick, next = Idle)]
    async fn on_tick(&mut self) {
        self.context += 1;
    }
}

#[test]
fn marked_transition_keeps_dispatch_bounded() {
    let mut m = Marked::new(0);
    let size = m.apply_future_size(MarkedEvent::Go);
    assert!(
        size <= BOUNDED,
        "apply future is {size} B, expected <= {BOUNDED}"
    );
}

#[test]
fn mixed_machine_is_bounded_by_its_marked_transition() {
    let mut m = Mixed::new(0);
    let size = m.apply_future_size(MixedEvent::Tick);
    assert!(
        size <= BOUNDED,
        "apply future is {size} B, expected <= {BOUNDED}"
    );
}

#[tokio::test]
async fn unmarked_transition_in_a_mixed_machine_still_runs() {
    let mut m = Mixed::new(0);
    m.apply(MixedEvent::Tick).await.unwrap();
    assert_eq!(m.state(), MixedState::Idle);
    assert_eq!(m.context, 1);
}

/// Without the mark and without `boxed-all`, the handler is inlined into
/// dispatch. This is the behaviour the mark exists to change; asserting it
/// keeps the other tests honest.
#[cfg(not(feature = "boxed-all"))]
#[test]
fn unmarked_transition_is_inlined_into_dispatch() {
    let mut m = Unmarked::new(0);
    let size = m.apply_future_size(UnmarkedEvent::Go);
    assert!(size > HEAVY, "apply future is {size} B, expected > {HEAVY}");
}

/// With `boxed-all` every transition is boxed, marked or not.
#[cfg(feature = "boxed-all")]
#[test]
fn boxed_all_bounds_an_unmarked_transition() {
    let mut m = Unmarked::new(0);
    let size = m.apply_future_size(UnmarkedEvent::Go);
    assert!(
        size <= BOUNDED,
        "apply future is {size} B, expected <= {BOUNDED}"
    );
}

/// Marks are redundant under `boxed-all`, not harmful.
#[cfg(feature = "boxed-all")]
#[test]
fn boxed_all_leaves_marked_transitions_bounded() {
    let mut m = Marked::new(0);
    let size = m.apply_future_size(MarkedEvent::Go);
    assert!(
        size <= BOUNDED,
        "apply future is {size} B, expected <= {BOUNDED}"
    );
}

// --- switching the policy must not change what the machine does ------------

/// Everything the two policies could plausibly disturb, in one machine:
/// a self-emit cascade, a branching transition, an event payload, and a
/// fallible handler. The assertions below are identical in both builds — that
/// is the point. Run under `--features diagnostics` and again with
/// `boxed-all` added; a difference in outcome means the policy is not the
/// no-op it claims to be.
#[derive(Debug, PartialEq, Eq)]
pub struct Boom(u32);

#[derive(Debug, Default)]
pub struct Trace {
    steps: Vec<&'static str>,
    total: u32,
}

#[fsm(initial = Start)]
impl Neutral {
    type Context = Trace;
    type Error = Boom;

    // Marked: boxed under both policies.
    #[on(state = Start, event = Begin(u32), next = Counting, boxed)]
    async fn on_begin(&mut self, seed: u32) {
        self.context.steps.push("begin");
        self.context.total += seed;
        self.emit(NeutralEvent::Step);
    }

    // Unmarked and branching: boxed only under `boxed-all`.
    #[on(state = Counting, event = Step, next = [Counting, Ready])]
    async fn on_step(&mut self) -> CountingStepNext {
        self.context.steps.push("step");
        self.context.total += 1;
        if self.context.total >= 3 {
            CountingStepNext::Ready
        } else {
            self.emit(NeutralEvent::Step);
            CountingStepNext::Counting
        }
    }

    // Unmarked and fallible.
    #[on(state = Ready, event = Finish, next = Done)]
    async fn on_finish(&mut self) -> Result<(), Boom> {
        self.context.steps.push("finish");
        Err(Boom(self.context.total))
    }
}

#[tokio::test]
async fn policy_does_not_change_cascade_branching_or_payload() {
    let mut m = Neutral::new(Trace::default());

    m.apply(NeutralEvent::Begin(1)).await.unwrap();
    assert_eq!(m.state(), NeutralState::Ready);
    assert_eq!(m.context.total, 3);
    assert_eq!(m.context.steps, ["begin", "step", "step"]);
}

#[tokio::test]
async fn policy_does_not_change_handler_errors() {
    let mut m = Neutral::new(Trace::default());
    m.apply(NeutralEvent::Begin(1)).await.unwrap();

    let err = m.apply(NeutralEvent::Finish).await.unwrap_err();
    assert_eq!(err, statecraft_fsm::ApplyError::Handler(Boom(3)));
}

#[tokio::test]
async fn policy_does_not_change_missing_transitions() {
    let mut m = Neutral::new(Trace::default());
    let err = m.apply(NeutralEvent::Step).await.unwrap_err();
    assert_eq!(err, statecraft_fsm::ApplyError::NoTransition);
    assert_eq!(m.state(), NeutralState::Start);
}
