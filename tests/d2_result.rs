//! SEBKD-2: fallible handler returns. A handler may return `Result<_, E>`;
//! its error propagates through `apply` as `ApplyError::Handler`.

use statecraft_fsm::fsm;

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
enum MyError {
    #[error("too many")]
    TooMany,
}

#[derive(Debug, Default)]
struct Counter {
    count: usize,
}

#[fsm(initial = Idle)]
impl Machine {
    type Context = Counter;
    type Error = MyError;

    // Fallible + single target.
    #[on(state = Idle, event = Start, next = Running)]
    async fn on_start(&mut self) -> Result<(), MyError> {
        self.context.count += 1;
        Ok(())
    }

    // Fallible + branching: returns Err when the counter is exhausted.
    #[on(state = Running, event = Check, next = [Running, Done])]
    async fn on_check(&mut self) -> Result<RunningCheckNext, MyError> {
        self.context.count += 1;
        if self.context.count > 5 {
            Err(MyError::TooMany)
        } else if self.context.count >= 3 {
            Ok(RunningCheckNext::Done)
        } else {
            Ok(RunningCheckNext::Running)
        }
    }
}

#[tokio::test]
async fn test_ok_path_transitions() {
    let mut m = Machine::new(Counter::default());
    m.apply(MachineEvent::Start).await.unwrap(); // count 1
    assert_eq!(m.state(), MachineState::Running);

    m.apply(MachineEvent::Check).await.unwrap(); // count 2, Running
    assert_eq!(m.state(), MachineState::Running);
    m.apply(MachineEvent::Check).await.unwrap(); // count 3, Done
    assert_eq!(m.state(), MachineState::Done);
}

#[tokio::test]
async fn test_handler_error_propagates() {
    let mut m = Machine::new(Counter { count: 5 });
    // Start moves to Running (count 6), then Check errors (count 7 > 5).
    m.apply(MachineEvent::Start).await.unwrap();
    let err = m.apply(MachineEvent::Check).await.unwrap_err();
    assert_eq!(err, statecraft_fsm::ApplyError::Handler(MyError::TooMany));
    // State is unchanged on error.
    assert_eq!(m.state(), MachineState::Running);
}
