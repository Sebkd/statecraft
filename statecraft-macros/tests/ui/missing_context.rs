// The macro requires an associated `type Context`. Omitting it is our own
// diagnostic, not a downstream rustc error.
use statecraft::fsm;

#[fsm(initial = Idle)]
impl Machine {
    #[on(state = Idle, event = Start, next = Running)]
    async fn on_start(&mut self) {}
}

fn main() {}
