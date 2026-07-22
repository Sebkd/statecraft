// A branching handler that returns a declared target compiles fine.
use statecraft_fsm::fsm;

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
}

fn main() {}
