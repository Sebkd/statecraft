//! D2 prototype: compile-time-checked branching.
//!
//! `Check` in `Running` may go to `Running` or `Done`. The handler returns the
//! generated `RunningCheckNext` enum, so an undeclared target cannot compile.

use statecraft::fsm;

#[derive(Debug, Default)]
struct Counter {
    count: usize,
}

#[fsm(initial = Idle)]
impl Machine {
    type Context = Counter;

    #[on(state = Idle, event = Start, next = Running)]
    async fn on_start(&mut self) {
        self.context.count += 1;
    }

    #[on(state = Running, event = Check, next = [Running, Done])]
    async fn on_check(&mut self) -> RunningCheckNext {
        if self.context.count >= 3 {
            RunningCheckNext::Done
        } else {
            self.context.count += 1;
            RunningCheckNext::Running
        }
    }

    #[on(state = Done, event = Reset, next = Idle)]
    async fn on_reset(&mut self) {
        self.context.count = 0;
    }
}

#[tokio::test]
async fn test_branching_loops_then_reaches_done() {
    let mut m = Machine::new(Counter::default());
    assert_eq!(m.state(), MachineState::Idle);

    m.apply(MachineEvent::Start).await.unwrap();
    assert_eq!(m.state(), MachineState::Running);
    assert_eq!(m.context.count, 1);

    // count: 1 -> 2 (Running) -> 3 (Running) -> then Check sees >= 3 -> Done
    m.apply(MachineEvent::Check).await.unwrap();
    assert_eq!(m.state(), MachineState::Running);
    m.apply(MachineEvent::Check).await.unwrap();
    assert_eq!(m.state(), MachineState::Running);
    m.apply(MachineEvent::Check).await.unwrap();
    assert_eq!(m.state(), MachineState::Done);

    m.apply(MachineEvent::Reset).await.unwrap();
    assert_eq!(m.state(), MachineState::Idle);
    assert_eq!(m.context.count, 0);
}

#[tokio::test]
async fn test_undeclared_transition_is_runtime_error() {
    let mut m = Machine::new(Counter::default());
    // Reset is not valid in Idle: no handler for (Idle, Reset).
    let err = m.apply(MachineEvent::Reset).await.unwrap_err();
    assert_eq!(err, statecraft::ApplyError::NoTransition);
}
