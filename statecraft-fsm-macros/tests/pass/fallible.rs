// A fallible handler whose error type matches `type Error` compiles, for both
// single-target and branching transitions.
use statecraft_fsm::fsm;

#[derive(Debug)]
struct Boom;

#[derive(Debug, Default)]
struct Counter {
    count: usize,
}

#[fsm(initial = Idle)]
impl Machine {
    type Context = Counter;
    type Error = Boom;

    #[on(state = Idle, event = Start, next = Running)]
    async fn on_start(&mut self) -> Result<(), Boom> {
        self.context.count += 1;
        Ok(())
    }

    #[on(state = Running, event = Check, next = [Running, Done])]
    async fn on_check(&mut self) -> Result<RunningCheckNext, Boom> {
        Ok(RunningCheckNext::Done)
    }
}

fn main() {}
